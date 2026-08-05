//! Persistent exact-label interning for streamed reference definitions.
//!
//! Normalized labels are bounded by the shared CommonMark label service, but
//! they are not hashes and their IDs are never caller-authored.  This module
//! stores exact UTF-8 keys in chunked arena pages and keeps the directory in a
//! persistent balanced sequence ordered by those bytes.  Lookup descends by
//! exact comparisons against authenticated subtree maxima; one poll reads or
//! allocates at most one arena page (the sequence splice has the same existing
//! one-branch-per-poll contract).
//!
//! The parser-output adapter remains in `candidate_writer`: production callers
//! cannot construct [`CandidateReferenceLabel`] from a free `String`.  The
//! proof-only constructor below exists only for this isolated kernel's tests.

use std::cmp::Ordering;
use std::fmt;

use flark_reference_label_service::MAX_NORMALIZED_REFERENCE_LABEL_BYTES;

use crate::reference_restart_index::ReferenceIndexInternerMint;

use crate::arena::{
    ARENA_PAGE_BYTES, AllocationReceipt, ArenaBuildError, ArenaBuildId, ArenaBuildOwner,
    ArenaBuildSession, ArenaError, ArenaId, ArenaScopedId, PageArena,
};
use crate::persistent_blob::{
    PersistentBlobBuildProgress, PersistentBlobError, PersistentBlobReadProgress,
    PersistentByteBlob, PersistentByteBlobBuilder, PersistentByteBlobMetadata,
    PersistentByteBlobReadCursor,
};
use crate::persistent_sequence::{
    ResumableSequenceSplice, ResumableSequenceSplitProgress, SequenceMutationReceipt,
    SequenceNodeKind, SequenceSpec, sequence_node,
};

const FORMAT_VERSION: u8 = 1;
const LABEL_LEAF_TAG: u8 = 0xc2;
const LABEL_BRANCH_TAG: u8 = 0xc3;
const LABEL_MANIFEST_TAG: u8 = 0xc4;

const LABEL_LEAF_BYTES: usize = 64;
const LABEL_BRANCH_BYTES: usize = 160;
const LABEL_MANIFEST_BYTES: usize = 56;
const MAX_SEQUENCE_HEIGHT: u16 = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceLabelInternerError {
    Arena(ArenaError),
    ArenaBuild(ArenaBuildError),
    Blob(PersistentBlobError),
    Invalid(&'static str),
    Corrupt(&'static str),
    Overflow(&'static str),
    InjectedFault(u64),
    ReplayedAck,
    CrossedAck,
}

impl From<ArenaError> for ReferenceLabelInternerError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

impl From<ArenaBuildError> for ReferenceLabelInternerError {
    fn from(value: ArenaBuildError) -> Self {
        Self::ArenaBuild(value)
    }
}

impl From<PersistentBlobError> for ReferenceLabelInternerError {
    fn from(value: PersistentBlobError) -> Self {
        Self::Blob(value)
    }
}

impl fmt::Display for ReferenceLabelInternerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "reference label interner error: {self:?}")
    }
}

impl std::error::Error for ReferenceLabelInternerError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LabelKeyDescriptor {
    blob: ArenaId,
    bytes: u64,
    chunks: u64,
    height: u16,
    label_id: u64,
}

impl LabelKeyDescriptor {
    fn validate(self) -> Result<(), ReferenceLabelInternerError> {
        if self.bytes == 0
            || self.chunks == 0
            || !(1..=MAX_SEQUENCE_HEIGHT).contains(&self.height)
            || self.label_id == 0
        {
            return Err(ReferenceLabelInternerError::Corrupt(
                "label key descriptor is empty",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LabelSequenceSummary {
    leaves: u64,
    height: u16,
    first: LabelKeyDescriptor,
    last: LabelKeyDescriptor,
    left_leaves: u64,
    separator: LabelKeyDescriptor,
}

#[derive(Debug)]
struct LabelSequenceSpec;

impl SequenceSpec for LabelSequenceSpec {
    type Summary = LabelSequenceSummary;
    type Error = ReferenceLabelInternerError;
    type BranchPayload = [u8; LABEL_BRANCH_BYTES];

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(LABEL_LEAF_TAG) {
            return Ok(None);
        }
        let leaf = decode_label_leaf_payload(payload)?;
        Ok(Some(LabelSequenceSummary {
            leaves: 1,
            height: 1,
            first: leaf,
            last: leaf,
            left_leaves: 0,
            separator: leaf,
        }))
    }

    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(LABEL_BRANCH_TAG) {
            return Ok(None);
        }
        if payload.len() != LABEL_BRANCH_BYTES || payload[1] != FORMAT_VERSION {
            return Err(ReferenceLabelInternerError::Corrupt(
                "invalid label-directory branch",
            ));
        }
        let summary = LabelSequenceSummary {
            leaves: get_u64(payload, 8),
            height: get_u16(payload, 16),
            first: decode_key_descriptor(payload, 24),
            last: decode_key_descriptor(payload, 64),
            separator: decode_key_descriptor(payload, 104),
            left_leaves: get_u64(payload, 144),
        };
        summary.first.validate()?;
        summary.last.validate()?;
        summary.separator.validate()?;
        if summary.leaves < 2
            || !(2..=MAX_SEQUENCE_HEIGHT).contains(&summary.height)
            || summary.left_leaves == 0
            || summary.left_leaves >= summary.leaves
        {
            return Err(ReferenceLabelInternerError::Corrupt(
                "invalid label-directory branch summary",
            ));
        }
        Ok(Some(summary))
    }

    fn encode_branch(summary: Self::Summary) -> Self::BranchPayload {
        let mut payload = [0_u8; LABEL_BRANCH_BYTES];
        payload[0] = LABEL_BRANCH_TAG;
        payload[1] = FORMAT_VERSION;
        put_u64(&mut payload, 8, summary.leaves);
        put_u16(&mut payload, 16, summary.height);
        encode_key_descriptor(&mut payload, 24, summary.first);
        encode_key_descriptor(&mut payload, 64, summary.last);
        encode_key_descriptor(&mut payload, 104, summary.separator);
        put_u64(&mut payload, 144, summary.left_leaves);
        payload
    }

    fn branch_witness(summary: Self::Summary) -> Option<ArenaId> {
        Some(summary.separator.blob)
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        left.first.validate()?;
        left.last.validate()?;
        right.first.validate()?;
        right.last.validate()?;
        let leaves =
            left.leaves
                .checked_add(right.leaves)
                .ok_or(ReferenceLabelInternerError::Overflow(
                    "label-directory leaf count",
                ))?;
        let height = left.height.max(right.height).checked_add(1).ok_or(
            ReferenceLabelInternerError::Overflow("label-directory height"),
        )?;
        if height > MAX_SEQUENCE_HEIGHT {
            return Err(ReferenceLabelInternerError::Corrupt(
                "label-directory height exceeds its authenticated envelope",
            ));
        }
        Ok(LabelSequenceSummary {
            leaves,
            height,
            first: left.first,
            last: right.last,
            left_leaves: left.leaves,
            separator: left.last,
        })
    }

    fn leaves(summary: Self::Summary) -> u64 {
        summary.leaves
    }

    fn height(summary: Self::Summary) -> u16 {
        summary.height
    }

    fn invalid(message: &'static str) -> Self::Error {
        ReferenceLabelInternerError::Invalid(message)
    }
}

/// Candidate-writer-minted normalized key. The production constructor is
/// intentionally absent until the reference output/range rendezvous consumes
/// its non-cloneable parser token in `candidate_writer`.
#[derive(Debug)]
pub(crate) struct CandidateReferenceLabel {
    normalized: String,
    request_nonce: u64,
}

impl CandidateReferenceLabel {
    fn validate(&self) -> Result<(), ReferenceLabelInternerError> {
        if self.request_nonce == 0
            || self.normalized.is_empty()
            || self.normalized.len() > MAX_NORMALIZED_REFERENCE_LABEL_BYTES
            || !self.normalized.is_char_boundary(self.normalized.len())
        {
            return Err(ReferenceLabelInternerError::Invalid(
                "candidate reference label is not a bounded normalized parser output",
            ));
        }
        Ok(())
    }

    /// Production construction seam. The normalized label is moved directly
    /// out of one parser output while CandidateWriter retains its output ack;
    /// the private writer mint prevents free strings from entering the
    /// persistent exact-label directory elsewhere in the crate.
    pub(crate) fn from_writer_join(
        normalized: String,
        request_nonce: u64,
        _mint: &mut crate::candidate_writer::ReferenceCandidateIndexWriterMint,
    ) -> Result<Self, ReferenceLabelInternerError> {
        let candidate = Self {
            normalized,
            request_nonce,
        };
        candidate.validate()?;
        Ok(candidate)
    }

    #[cfg(test)]
    pub(crate) fn proof_only(normalized: &str, request_nonce: u64) -> Self {
        Self {
            normalized: normalized.to_owned(),
            request_nonce,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LabelJoinIdentity {
    build: ArenaBuildId,
    interner_generation: u64,
    interner_root: ArenaId,
    label_id: u64,
    request_nonce: u64,
}

/// Non-cloneable exact-label capability consumed by the occurrence builder.
#[derive(Debug)]
#[must_use = "the exact label must be consumed by one occurrence publication"]
pub(crate) struct InternedReferenceLabel {
    join: LabelJoinIdentity,
}

impl InternedReferenceLabel {
    /// Consumes one exact label into the sole writer-authorized occurrence
    /// index. The acknowledgement remains withheld until both the global and
    /// per-label sequences have durably advanced.
    pub(crate) fn consume_for_reference_index(
        self,
        _mint: &mut ReferenceIndexInternerMint,
    ) -> (u64, ReferenceLabelUseAck) {
        self.into_index_parts()
    }

    #[cfg(test)]
    fn proof_only_consume_for_index(self) -> (u64, ReferenceLabelUseAck) {
        self.into_index_parts()
    }

    fn into_index_parts(self) -> (u64, ReferenceLabelUseAck) {
        (
            self.join.label_id,
            ReferenceLabelUseAck {
                join: Some(self.join),
            },
        )
    }
}

/// Lineage returned only after the occurrence index has durably consumed the
/// label. It binds the exact root, not merely a stable scalar label ID.
#[derive(Debug)]
#[must_use = "the label-use acknowledgement must rearm its exact interner"]
pub(crate) struct ReferenceLabelUseAck {
    join: Option<LabelJoinIdentity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReferenceLabelInternerManifestIdentity {
    pub(crate) build: ArenaBuildId,
    pub(crate) generation: u64,
    pub(crate) label_count: u64,
    pub(crate) label_id_high_water: u64,
}

#[derive(Debug)]
#[must_use = "the exact-label manifest must be joined into reference publication"]
pub(crate) struct ReferenceLabelInternerManifest {
    owner: ArenaBuildOwner,
    identity: ReferenceLabelInternerManifestIdentity,
}

/// Reusable, arena-scoped authority for one committed exact-label manifest.
///
/// The reference index is the only module able to mint this capability.  A
/// stale document release is detected by revalidating `manifest` on every
/// lookup/adoption boundary; callers never supply label IDs themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommittedReferenceLabelInterner {
    manifest: ArenaScopedId,
    generation: u64,
    label_count: u64,
    label_id_high_water: u64,
}

impl CommittedReferenceLabelInterner {
    pub(crate) fn from_reference_index_join(
        arena: &PageArena,
        manifest: ArenaScopedId,
        expected_generation: u64,
        expected_label_id_high_water: u64,
        _mint: &mut ReferenceIndexInternerMint,
    ) -> Result<Self, ReferenceLabelInternerError> {
        let local = arena.local_id(manifest)?;
        let decoded = decode_manifest(arena, local)?;
        if decoded.generation != expected_generation
            || decoded.label_id_high_water != expected_label_id_high_water
        {
            return Err(ReferenceLabelInternerError::Corrupt(
                "committed label manifest crossed its reference-index authority",
            ));
        }
        Ok(Self {
            manifest,
            generation: decoded.generation,
            label_count: decoded.count,
            label_id_high_water: decoded.label_id_high_water,
        })
    }

    fn revalidate(
        self,
        arena: &PageArena,
    ) -> Result<DecodedLabelManifest, ReferenceLabelInternerError> {
        let decoded = decode_manifest(arena, arena.local_id(self.manifest)?)?;
        if decoded.generation != self.generation
            || decoded.count != self.label_count
            || decoded.label_id_high_water != self.label_id_high_water
        {
            return Err(ReferenceLabelInternerError::Corrupt(
                "committed label capability no longer matches its manifest",
            ));
        }
        Ok(decoded)
    }

    pub(crate) const fn generation(self) -> u64 {
        self.generation
    }

    pub(crate) const fn label_count(self) -> u64 {
        self.label_count
    }

    pub(crate) const fn label_id_high_water(self) -> u64 {
        self.label_id_high_water
    }

    pub(crate) fn is_direct_adoption_of(
        self,
        arena: &PageArena,
        parent: Self,
    ) -> Result<bool, ReferenceLabelInternerError> {
        let decoded = self.revalidate(arena)?;
        parent.revalidate(arena)?;
        Ok(
            decoded.parent_manifest == Some(arena.local_id(parent.manifest)?)
                && self.generation
                    == parent.generation.checked_add(1).ok_or(
                        ReferenceLabelInternerError::Overflow("committed adoption generation"),
                    )?
                && self.label_count >= parent.label_count
                && self.label_id_high_water >= parent.label_id_high_water,
        )
    }
}

/// Linear adoption request for a committed exact-label directory.  The
/// adopted interner retains both the exact trie root and this parent manifest
/// before the donor document may retire.
#[derive(Debug)]
pub(crate) struct ReferenceLabelInternerAdoption {
    committed: CommittedReferenceLabelInterner,
}

impl ReferenceLabelInternerAdoption {
    pub(crate) const fn from_committed(committed: CommittedReferenceLabelInterner) -> Self {
        Self { committed }
    }
}

impl ReferenceLabelInternerManifest {
    pub(crate) const fn identity(&self) -> ReferenceLabelInternerManifestIdentity {
        self.identity
    }

    pub(crate) fn consume_for_reference_index(
        self,
        _mint: &mut ReferenceIndexInternerMint,
    ) -> ArenaBuildOwner {
        self.owner
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReferenceLabelInternerReceipt {
    pub(crate) polls: u64,
    pub(crate) pages_read: u64,
    pub(crate) pages_allocated: u64,
    pub(crate) exact_bytes_compared: u64,
    pub(crate) sequence_nodes_visited: u64,
    pub(crate) sequence_branches_allocated: usize,
    pub(crate) labels_inserted: u64,
    pub(crate) labels_reused: u64,
    pub(crate) maximum_pages_read_per_poll: u64,
    pub(crate) maximum_pages_allocated_per_poll: u64,
    pub(crate) maximum_retained_label_bytes: usize,
    pub(crate) maximum_blob_pages_per_label: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceLabelInternerProgress {
    Pending,
    ReadyForLabel,
    LabelReady,
    ManifestReady,
}

#[derive(Debug)]
struct BlobComparison {
    descriptor: LabelKeyDescriptor,
    cursor: PersistentByteBlobReadCursor,
    offset: usize,
}

#[derive(Debug)]
enum SearchDecision {
    Branch {
        left: ArenaId,
        right: ArenaId,
        left_leaves: u64,
    },
    Leaf {
        leaf: LabelKeyDescriptor,
    },
}

#[derive(Debug)]
struct SearchJob {
    node: ArenaId,
    base_index: u64,
    comparison: Option<BlobComparison>,
    decision: Option<SearchDecision>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CommittedReferenceLabelLookupReceipt {
    pub(crate) polls: u64,
    pub(crate) pages_read: u64,
    pub(crate) exact_bytes_compared: u64,
    pub(crate) maximum_pages_read_per_poll: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommittedReferenceLabelLookupProgress {
    Pending,
    Ready,
}

#[derive(Debug)]
struct CommittedLookupComparison {
    descriptor: LabelKeyDescriptor,
    cursor: PersistentByteBlobReadCursor,
    offset: usize,
}

#[derive(Debug)]
enum CommittedLookupDecision {
    Branch { left: ArenaId, right: ArenaId },
    Leaf { label_id: u64 },
}

#[derive(Debug)]
enum CommittedLookupPhase {
    Descend(ArenaId),
    Compare {
        comparison: CommittedLookupComparison,
        decision: CommittedLookupDecision,
    },
    Ready,
    Taken,
    Failed,
}

/// One-page-per-poll exact lookup over a committed normalized-label trie.
/// Hashes are never semantic authority: the candidate bytes are compared
/// directly against the retained exact key blobs.
#[derive(Debug)]
pub(crate) struct CommittedReferenceLabelLookup {
    authority: CommittedReferenceLabelInterner,
    normalized: String,
    result: Option<Option<u64>>,
    phase: CommittedLookupPhase,
    receipt: CommittedReferenceLabelLookupReceipt,
}

impl CommittedReferenceLabelInterner {
    pub(crate) fn begin_lookup_normalized(
        self,
        arena: &PageArena,
        normalized: String,
    ) -> Result<CommittedReferenceLabelLookup, ReferenceLabelInternerError> {
        if normalized.is_empty() || normalized.len() > MAX_NORMALIZED_REFERENCE_LABEL_BYTES {
            return Err(ReferenceLabelInternerError::Invalid(
                "committed label query is empty or exceeds the normalized bound",
            ));
        }
        let decoded = self.revalidate(arena)?;
        let (phase, result) = match decoded.root {
            Some(root) => (CommittedLookupPhase::Descend(root), None),
            None => (CommittedLookupPhase::Ready, Some(None)),
        };
        Ok(CommittedReferenceLabelLookup {
            authority: self,
            normalized,
            result,
            phase,
            receipt: CommittedReferenceLabelLookupReceipt::default(),
        })
    }
}

impl CommittedReferenceLabelLookup {
    pub(crate) const fn receipt(&self) -> CommittedReferenceLabelLookupReceipt {
        self.receipt
    }

    pub(crate) fn poll(
        &mut self,
        arena: &PageArena,
    ) -> Result<CommittedReferenceLabelLookupProgress, ReferenceLabelInternerError> {
        // The scoped parent manifest is the lifetime authority for every root
        // captured by this query. Revalidating it detects stale/cross-arena
        // use without retaining document-sized state in the query.
        arena.local_id(self.authority.manifest)?;
        match self.phase {
            CommittedLookupPhase::Ready => {
                return Ok(CommittedReferenceLabelLookupProgress::Ready);
            }
            CommittedLookupPhase::Taken | CommittedLookupPhase::Failed => {
                return Err(ReferenceLabelInternerError::Invalid(
                    "committed label lookup is consumed or poisoned",
                ));
            }
            _ => {}
        }
        let pages_before = self.receipt.pages_read;
        let phase = std::mem::replace(&mut self.phase, CommittedLookupPhase::Failed);
        let result = self.poll_phase(arena, phase);
        self.receipt.polls =
            self.receipt
                .polls
                .checked_add(1)
                .ok_or(ReferenceLabelInternerError::Overflow(
                    "committed label lookup polls",
                ))?;
        let pages_this_poll = self.receipt.pages_read.checked_sub(pages_before).ok_or(
            ReferenceLabelInternerError::Corrupt("committed lookup pages moved backwards"),
        )?;
        self.receipt.maximum_pages_read_per_poll = self
            .receipt
            .maximum_pages_read_per_poll
            .max(pages_this_poll);
        if pages_this_poll > 1 {
            self.phase = CommittedLookupPhase::Failed;
            return Err(ReferenceLabelInternerError::Corrupt(
                "one committed label lookup poll read more than one page",
            ));
        }
        match result {
            Ok(next) => {
                self.phase = next;
                Ok(if matches!(self.phase, CommittedLookupPhase::Ready) {
                    CommittedReferenceLabelLookupProgress::Ready
                } else {
                    CommittedReferenceLabelLookupProgress::Pending
                })
            }
            Err(error) => {
                self.phase = CommittedLookupPhase::Failed;
                Err(error)
            }
        }
    }

    fn poll_phase(
        &mut self,
        arena: &PageArena,
        phase: CommittedLookupPhase,
    ) -> Result<CommittedLookupPhase, ReferenceLabelInternerError> {
        match phase {
            CommittedLookupPhase::Descend(node) => {
                let (summary, kind) = sequence_node::<LabelSequenceSpec>(arena, node)?;
                self.receipt.pages_read = self.receipt.pages_read.checked_add(1).ok_or(
                    ReferenceLabelInternerError::Overflow("committed lookup page reads"),
                )?;
                let (descriptor, decision) = match kind {
                    SequenceNodeKind::Leaf => {
                        let leaf = decode_label_leaf(arena, node)?;
                        (
                            leaf,
                            CommittedLookupDecision::Leaf {
                                label_id: leaf.label_id,
                            },
                        )
                    }
                    SequenceNodeKind::Branch { left, right } => (
                        summary.separator,
                        CommittedLookupDecision::Branch { left, right },
                    ),
                };
                Ok(CommittedLookupPhase::Compare {
                    comparison: CommittedLookupComparison {
                        descriptor,
                        cursor: PersistentByteBlobReadCursor::try_new(
                            PersistentByteBlobMetadata {
                                root: Some(descriptor.blob),
                                bytes: descriptor.bytes,
                                chunks: descriptor.chunks,
                                height: descriptor.height,
                            },
                        )?,
                        offset: 0,
                    },
                    decision,
                })
            }
            CommittedLookupPhase::Compare {
                mut comparison,
                decision,
            } => {
                let Some(ordering) = self.poll_committed_comparison(arena, &mut comparison)? else {
                    return Ok(CommittedLookupPhase::Compare {
                        comparison,
                        decision,
                    });
                };
                match decision {
                    CommittedLookupDecision::Branch { left, right } => Ok(
                        CommittedLookupPhase::Descend(if ordering == Ordering::Greater {
                            right
                        } else {
                            left
                        }),
                    ),
                    CommittedLookupDecision::Leaf { label_id } => {
                        self.result = Some((ordering == Ordering::Equal).then_some(label_id));
                        Ok(CommittedLookupPhase::Ready)
                    }
                }
            }
            CommittedLookupPhase::Ready
            | CommittedLookupPhase::Taken
            | CommittedLookupPhase::Failed => Err(ReferenceLabelInternerError::Invalid(
                "committed label lookup phase is not pollable",
            )),
        }
    }

    fn poll_committed_comparison(
        &mut self,
        arena: &PageArena,
        comparison: &mut CommittedLookupComparison,
    ) -> Result<Option<Ordering>, ReferenceLabelInternerError> {
        comparison.descriptor.validate()?;
        let chunk = match comparison.cursor.poll(arena)? {
            PersistentBlobReadProgress::Pending => {
                self.receipt.pages_read = self.receipt.pages_read.checked_add(1).ok_or(
                    ReferenceLabelInternerError::Overflow("committed lookup page reads"),
                )?;
                return Ok(None);
            }
            PersistentBlobReadProgress::Chunk(chunk) => chunk,
            PersistentBlobReadProgress::Complete => {
                return Err(ReferenceLabelInternerError::Corrupt(
                    "committed label blob completed without a comparison decision",
                ));
            }
        };
        self.receipt.pages_read =
            self.receipt
                .pages_read
                .checked_add(1)
                .ok_or(ReferenceLabelInternerError::Overflow(
                    "committed lookup page reads",
                ))?;
        let bytes = chunk.bytes(arena)?;
        let start = usize::try_from(chunk.absolute_start)
            .map_err(|_| ReferenceLabelInternerError::Overflow("committed key chunk start"))?;
        let key_len = usize::try_from(comparison.descriptor.bytes)
            .map_err(|_| ReferenceLabelInternerError::Overflow("committed key length"))?;
        if start != comparison.offset || start + bytes.len() > key_len {
            return Err(ReferenceLabelInternerError::Corrupt(
                "committed key chunks are crossed",
            ));
        }
        let candidate_end = self.normalized.len().min(start + bytes.len());
        let candidate = self.normalized.as_bytes().get(start..candidate_end).ok_or(
            ReferenceLabelInternerError::Corrupt("committed candidate escaped its bytes"),
        )?;
        let common = candidate.len().min(bytes.len());
        self.receipt.exact_bytes_compared =
            self.receipt
                .exact_bytes_compared
                .checked_add(u64::try_from(common).map_err(|_| {
                    ReferenceLabelInternerError::Overflow("committed comparison bytes")
                })?)
                .ok_or(ReferenceLabelInternerError::Overflow(
                    "committed comparison bytes",
                ))?;
        let ordering = candidate[..common].cmp(&bytes[..common]);
        if ordering != Ordering::Equal {
            return Ok(Some(ordering));
        }
        if candidate.len() < bytes.len() {
            return Ok(Some(Ordering::Less));
        }
        comparison.offset = start + bytes.len();
        if self.normalized.len() <= comparison.offset && comparison.offset < key_len {
            Ok(Some(Ordering::Less))
        } else if comparison.offset == key_len {
            Ok(Some(self.normalized.len().cmp(&comparison.offset)))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn take_label_id(&mut self) -> Result<Option<u64>, ReferenceLabelInternerError> {
        if !matches!(self.phase, CommittedLookupPhase::Ready) {
            return Err(ReferenceLabelInternerError::Invalid(
                "committed label lookup result is not ready",
            ));
        }
        let result = self
            .result
            .take()
            .ok_or(ReferenceLabelInternerError::Corrupt(
                "committed label lookup result disappeared",
            ))?;
        self.phase = CommittedLookupPhase::Taken;
        Ok(result)
    }
}

#[derive(Debug)]
struct BlobBuildJob {
    builder: PersistentByteBlobBuilder,
    input_offset: usize,
    finishing: bool,
}

#[derive(Debug)]
enum InternerPhase {
    Ready,
    Search(SearchJob),
    BuildBlob(BlobBuildJob),
    AllocateLeaf { blob: PersistentByteBlob },
    BeginSplice { leaf: ArenaBuildOwner },
    Splice(ResumableSequenceSplice<LabelSequenceSpec>),
    LabelReady,
    AwaitingAck,
    AllocateManifest,
    ManifestReady,
    Taken,
    Failed,
}

/// One initial or restarted exact-label interner inside an unpublished arena
/// build. Root ownership never leaves this value until the terminal manifest
/// is consumed by the composite reference builder.
#[derive(Debug)]
pub(crate) struct ReferenceLabelInterner {
    build: ArenaBuildId,
    generation: u64,
    root: Option<ArenaBuildOwner>,
    root_id: Option<ArenaId>,
    count: u64,
    id_high_water: u64,
    insertion_index: u64,
    pending: Option<CandidateReferenceLabel>,
    pending_label_id: Option<u64>,
    pending_join: Option<LabelJoinIdentity>,
    parent_manifest: Option<ArenaBuildOwner>,
    manifest: Option<ArenaBuildOwner>,
    phase: InternerPhase,
    sequence_receipt: SequenceMutationReceipt,
    receipt: ReferenceLabelInternerReceipt,
    fault_after_poll: Option<u64>,
}

impl ReferenceLabelInterner {
    pub(crate) fn new_initial(
        build: ArenaBuildId,
        generation: u64,
    ) -> Result<Self, ReferenceLabelInternerError> {
        if generation == 0 {
            return Err(ReferenceLabelInternerError::Invalid(
                "interner generation is zero",
            ));
        }
        Ok(Self {
            build,
            generation,
            root: None,
            root_id: None,
            count: 0,
            id_high_water: 0,
            insertion_index: 0,
            pending: None,
            pending_label_id: None,
            pending_join: None,
            parent_manifest: None,
            manifest: None,
            phase: InternerPhase::Ready,
            sequence_receipt: SequenceMutationReceipt::default(),
            receipt: ReferenceLabelInternerReceipt::default(),
            fault_after_poll: None,
        })
    }

    /// Starts a new unpublished interner by structurally adopting one exact
    /// committed directory. Existing IDs and key blobs are retained; only
    /// labels first seen in this candidate allocate IDs above the donor high
    /// water. The next manifest records the donor manifest as its parent.
    pub(crate) fn new_adopted(
        session: &mut ArenaBuildSession<'_>,
        adoption: ReferenceLabelInternerAdoption,
    ) -> Result<Self, ReferenceLabelInternerError> {
        let committed = adoption.committed;
        let decoded = committed.revalidate(session.arena())?;
        let generation =
            decoded
                .generation
                .checked_add(1)
                .ok_or(ReferenceLabelInternerError::Overflow(
                    "adopted interner generation",
                ))?;
        let root = decoded.root.map(|root| session.retain(root)).transpose()?;
        let parent_manifest = Some(session.retain(session.arena().local_id(committed.manifest)?)?);
        Ok(Self {
            build: session.id(),
            generation,
            root,
            root_id: decoded.root,
            count: decoded.count,
            id_high_water: decoded.label_id_high_water,
            insertion_index: 0,
            pending: None,
            pending_label_id: None,
            pending_join: None,
            parent_manifest,
            manifest: None,
            phase: InternerPhase::Ready,
            sequence_receipt: SequenceMutationReceipt::default(),
            receipt: ReferenceLabelInternerReceipt::default(),
            fault_after_poll: None,
        })
    }

    #[cfg(test)]
    fn with_fault_after_poll(mut self, poll: u64) -> Self {
        self.fault_after_poll = Some(poll);
        self
    }

    pub(crate) const fn receipt(&self) -> ReferenceLabelInternerReceipt {
        self.receipt
    }

    pub(crate) fn begin_intern(
        &mut self,
        label: CandidateReferenceLabel,
    ) -> Result<(), ReferenceLabelInternerError> {
        if !matches!(self.phase, InternerPhase::Ready)
            || self.pending.is_some()
            || self.pending_join.is_some()
        {
            return Err(ReferenceLabelInternerError::Invalid(
                "interner is not ready for another label",
            ));
        }
        label.validate()?;
        self.receipt.maximum_retained_label_bytes = self
            .receipt
            .maximum_retained_label_bytes
            .max(label.normalized.capacity());
        self.pending = Some(label);
        self.insertion_index = 0;
        self.pending_label_id = None;
        self.phase = match self.root_id {
            Some(root) => InternerPhase::Search(SearchJob {
                node: root,
                base_index: 0,
                comparison: None,
                decision: None,
            }),
            None => self.begin_missing_label()?,
        };
        Ok(())
    }

    fn begin_missing_label(&mut self) -> Result<InternerPhase, ReferenceLabelInternerError> {
        let label_id =
            self.id_high_water
                .checked_add(1)
                .ok_or(ReferenceLabelInternerError::Overflow(
                    "reference label identity allocator",
                ))?;
        self.pending_label_id = Some(label_id);
        Ok(InternerPhase::BuildBlob(BlobBuildJob {
            builder: PersistentByteBlobBuilder::try_new(self.build)?,
            input_offset: 0,
            finishing: false,
        }))
    }

    pub(crate) fn poll(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<ReferenceLabelInternerProgress, ReferenceLabelInternerError> {
        if session.id() != self.build {
            return Err(ReferenceLabelInternerError::Invalid(
                "interner crossed arena build authority",
            ));
        }
        if matches!(self.phase, InternerPhase::Failed | InternerPhase::Taken) {
            return Err(ReferenceLabelInternerError::Invalid(
                "interner is failed or consumed",
            ));
        }
        let pages_before = self.receipt.pages_read;
        let allocations_before = self.receipt.pages_allocated;
        let live_nodes_before = session.arena().metrics().live_nodes;
        let phase = std::mem::replace(&mut self.phase, InternerPhase::Failed);
        let result = self.poll_phase(session, phase);
        let allocated = session
            .arena()
            .metrics()
            .live_nodes
            .checked_sub(live_nodes_before)
            .ok_or(ReferenceLabelInternerError::Corrupt(
                "arena allocation count moved backwards during interner poll",
            ))?;
        self.receipt.pages_allocated =
            self.receipt
                .pages_allocated
                .checked_add(u64::try_from(allocated).map_err(|_| {
                    ReferenceLabelInternerError::Overflow("interner page allocations")
                })?)
                .ok_or(ReferenceLabelInternerError::Overflow(
                    "interner pages allocated",
                ))?;
        self.receipt.sequence_branches_allocated = self.sequence_receipt.branches_allocated;
        self.receipt.polls = self
            .receipt
            .polls
            .checked_add(1)
            .ok_or(ReferenceLabelInternerError::Overflow("interner polls"))?;
        self.receipt.maximum_pages_read_per_poll = self
            .receipt
            .maximum_pages_read_per_poll
            .max(self.receipt.pages_read.saturating_sub(pages_before));
        self.receipt.maximum_pages_allocated_per_poll =
            self.receipt.maximum_pages_allocated_per_poll.max(
                self.receipt
                    .pages_allocated
                    .saturating_sub(allocations_before),
            );
        if self.receipt.maximum_pages_read_per_poll > 1
            || self.receipt.maximum_pages_allocated_per_poll > 1
        {
            self.phase = InternerPhase::Failed;
            return Err(ReferenceLabelInternerError::Corrupt(
                "one interner poll exceeded its page-work envelope",
            ));
        }
        if self.fault_after_poll == Some(self.receipt.polls) {
            self.phase = InternerPhase::Failed;
            return Err(ReferenceLabelInternerError::InjectedFault(
                self.receipt.polls,
            ));
        }
        match result {
            Ok((phase, progress)) => {
                self.phase = phase;
                Ok(progress)
            }
            Err(error) => {
                self.phase = InternerPhase::Failed;
                Err(error)
            }
        }
    }

    fn poll_phase(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        phase: InternerPhase,
    ) -> Result<(InternerPhase, ReferenceLabelInternerProgress), ReferenceLabelInternerError> {
        match phase {
            InternerPhase::Ready => Ok((
                InternerPhase::Ready,
                ReferenceLabelInternerProgress::ReadyForLabel,
            )),
            InternerPhase::Search(mut search) => {
                let progress = self.poll_search(session, &mut search)?;
                Ok((
                    progress.unwrap_or(InternerPhase::Search(search)),
                    ReferenceLabelInternerProgress::Pending,
                ))
            }
            InternerPhase::BuildBlob(mut job) => {
                let input_len = self.pending_bytes()?.len();
                if !job.finishing
                    && job.input_offset < input_len
                    && job.builder.is_ready_for_bytes()
                {
                    let copied = job
                        .builder
                        .push_bytes(&self.pending_bytes()?[job.input_offset..])?;
                    if copied == 0 {
                        return Err(ReferenceLabelInternerError::Corrupt(
                            "ready label blob builder accepted no input",
                        ));
                    }
                    job.input_offset = job.input_offset.checked_add(copied).ok_or(
                        ReferenceLabelInternerError::Overflow("label blob input offset"),
                    )?;
                    return Ok((
                        InternerPhase::BuildBlob(job),
                        ReferenceLabelInternerProgress::Pending,
                    ));
                }
                if !job.finishing
                    && job.input_offset == input_len
                    && job.builder.is_ready_for_bytes()
                {
                    job.builder.begin_finish()?;
                    job.finishing = true;
                    return Ok((
                        InternerPhase::BuildBlob(job),
                        ReferenceLabelInternerProgress::Pending,
                    ));
                }
                let progress = job.builder.poll(session)?;
                if job.finishing && progress == PersistentBlobBuildProgress::Complete {
                    let blob = job.builder.take_blob()?;
                    let pages = usize::try_from(
                        job.builder
                            .receipt()
                            .chunk_pages_allocated
                            .checked_add(
                                u64::try_from(job.builder.receipt().branch_pages_allocated)
                                    .map_err(|_| {
                                        ReferenceLabelInternerError::Overflow(
                                            "label blob branch pages",
                                        )
                                    })?,
                            )
                            .ok_or(ReferenceLabelInternerError::Overflow("label blob pages"))?,
                    )
                    .map_err(|_| ReferenceLabelInternerError::Overflow("label blob pages"))?;
                    self.receipt.maximum_blob_pages_per_label =
                        self.receipt.maximum_blob_pages_per_label.max(pages);
                    return Ok((
                        InternerPhase::AllocateLeaf { blob },
                        ReferenceLabelInternerProgress::Pending,
                    ));
                }
                Ok((
                    InternerPhase::BuildBlob(job),
                    ReferenceLabelInternerProgress::Pending,
                ))
            }
            InternerPhase::AllocateLeaf { blob } => {
                let label_id =
                    self.pending_label_id
                        .ok_or(ReferenceLabelInternerError::Corrupt(
                            "pending label ID disappeared",
                        ))?;
                let metadata = blob.metadata(session)?;
                let blob_id = metadata.root.ok_or(ReferenceLabelInternerError::Corrupt(
                    "normalized label produced an empty persistent blob",
                ))?;
                if metadata.bytes
                    != u64::try_from(self.pending_bytes()?.len()).map_err(|_| {
                        ReferenceLabelInternerError::Overflow("normalized label bytes")
                    })?
                {
                    return Err(ReferenceLabelInternerError::Corrupt(
                        "normalized label blob length changed during build",
                    ));
                }
                let descriptor = LabelKeyDescriptor {
                    blob: blob_id,
                    bytes: metadata.bytes,
                    chunks: metadata.chunks,
                    height: metadata.height,
                    label_id,
                };
                let payload = encode_label_leaf_payload(descriptor);
                let (leaf, allocation) = session.allocate_packed(&payload, &[blob_id])?;
                self.record_allocation(allocation)?;
                session.release(blob.into_owner().ok_or(
                    ReferenceLabelInternerError::Corrupt("label blob owner disappeared"),
                )?)?;
                Ok((
                    InternerPhase::BeginSplice { leaf },
                    ReferenceLabelInternerProgress::Pending,
                ))
            }
            InternerPhase::BeginSplice { leaf } => {
                let working = self.root.take();
                let splice = ResumableSequenceSplice::<LabelSequenceSpec>::try_from_owned(
                    session,
                    working,
                    self.insertion_index..self.insertion_index,
                    Some(leaf),
                    &mut self.sequence_receipt,
                )?;
                Ok((
                    InternerPhase::Splice(splice),
                    ReferenceLabelInternerProgress::Pending,
                ))
            }
            InternerPhase::Splice(mut splice) => {
                if splice.poll(session, &mut self.sequence_receipt)?
                    == ResumableSequenceSplitProgress::Pending
                {
                    return Ok((
                        InternerPhase::Splice(splice),
                        ReferenceLabelInternerProgress::Pending,
                    ));
                }
                let root = splice
                    .take_root()?
                    .ok_or(ReferenceLabelInternerError::Corrupt(
                        "label insertion produced no root",
                    ))?;
                let root_id = session.owner_id(&root)?;
                self.root = Some(root);
                self.root_id = Some(root_id);
                self.count =
                    self.count
                        .checked_add(1)
                        .ok_or(ReferenceLabelInternerError::Overflow(
                            "interner label count",
                        ))?;
                let label_id =
                    self.pending_label_id
                        .ok_or(ReferenceLabelInternerError::Corrupt(
                            "inserted label ID disappeared",
                        ))?;
                self.id_high_water = label_id;
                self.receipt.labels_inserted = self.receipt.labels_inserted.checked_add(1).ok_or(
                    ReferenceLabelInternerError::Overflow("inserted label count"),
                )?;
                self.install_ready_join(root_id, label_id)?;
                Ok((
                    InternerPhase::LabelReady,
                    ReferenceLabelInternerProgress::LabelReady,
                ))
            }
            InternerPhase::LabelReady => Ok((
                InternerPhase::LabelReady,
                ReferenceLabelInternerProgress::LabelReady,
            )),
            InternerPhase::AwaitingAck => Err(ReferenceLabelInternerError::Invalid(
                "interner label acknowledgement is still outstanding",
            )),
            InternerPhase::AllocateManifest => {
                let root_id = self
                    .root
                    .as_ref()
                    .map(|owner| session.owner_id(owner))
                    .transpose()?;
                let parent_id = self
                    .parent_manifest
                    .as_ref()
                    .map(|owner| session.owner_id(owner))
                    .transpose()?;
                let payload = encode_manifest(
                    self.generation,
                    self.count,
                    self.id_high_water,
                    root_id,
                    parent_id,
                );
                let mut children = [ArenaId::default(); 2];
                let mut child_count = 0;
                if let Some(root) = root_id {
                    children[child_count] = root;
                    child_count += 1;
                }
                if let Some(parent) = parent_id {
                    children[child_count] = parent;
                    child_count += 1;
                }
                let (manifest, allocation) =
                    session.allocate_packed(&payload, &children[..child_count])?;
                self.record_allocation(allocation)?;
                if let Some(root) = self.root.take() {
                    session.release(root)?;
                }
                if let Some(parent) = self.parent_manifest.take() {
                    session.release(parent)?;
                }
                self.manifest = Some(manifest);
                Ok((
                    InternerPhase::ManifestReady,
                    ReferenceLabelInternerProgress::ManifestReady,
                ))
            }
            InternerPhase::ManifestReady => Ok((
                InternerPhase::ManifestReady,
                ReferenceLabelInternerProgress::ManifestReady,
            )),
            InternerPhase::Taken | InternerPhase::Failed => Err(
                ReferenceLabelInternerError::Invalid("interner is failed or consumed"),
            ),
        }
    }

    fn poll_search(
        &mut self,
        session: &ArenaBuildSession<'_>,
        search: &mut SearchJob,
    ) -> Result<Option<InternerPhase>, ReferenceLabelInternerError> {
        if let Some(comparison) = search.comparison.as_mut() {
            let ordering = self.poll_blob_comparison(session, comparison)?;
            let Some(ordering) = ordering else {
                return Ok(None);
            };
            let decision = search
                .decision
                .take()
                .ok_or(ReferenceLabelInternerError::Corrupt(
                    "label comparison lost its decision",
                ))?;
            search.comparison = None;
            match decision {
                SearchDecision::Branch {
                    left,
                    right,
                    left_leaves,
                } => {
                    if ordering == Ordering::Greater {
                        search.node = right;
                        search.base_index = search.base_index.checked_add(left_leaves).ok_or(
                            ReferenceLabelInternerError::Overflow("label insertion index"),
                        )?;
                    } else {
                        search.node = left;
                    }
                    return Ok(None);
                }
                SearchDecision::Leaf { leaf } => {
                    return match ordering {
                        Ordering::Equal => {
                            let root = self.root_id.ok_or(ReferenceLabelInternerError::Corrupt(
                                "interner root disappeared",
                            ))?;
                            self.receipt.labels_reused =
                                self.receipt.labels_reused.checked_add(1).ok_or(
                                    ReferenceLabelInternerError::Overflow("reused label count"),
                                )?;
                            self.pending_label_id = Some(leaf.label_id);
                            self.install_ready_join(root, leaf.label_id)?;
                            Ok(Some(InternerPhase::LabelReady))
                        }
                        Ordering::Less | Ordering::Greater => {
                            self.insertion_index = search
                                .base_index
                                .checked_add(u64::from(ordering == Ordering::Greater))
                                .ok_or(ReferenceLabelInternerError::Overflow(
                                    "label insertion index",
                                ))?;
                            self.begin_missing_label().map(Some)
                        }
                    };
                }
            }
        }

        let (summary, kind) = sequence_node::<LabelSequenceSpec>(session.arena(), search.node)?;
        self.receipt.pages_read = self
            .receipt
            .pages_read
            .checked_add(1)
            .ok_or(ReferenceLabelInternerError::Overflow("interner page reads"))?;
        self.receipt.sequence_nodes_visited =
            self.receipt.sequence_nodes_visited.checked_add(1).ok_or(
                ReferenceLabelInternerError::Overflow("interner sequence nodes"),
            )?;
        match kind {
            SequenceNodeKind::Leaf => {
                let leaf = decode_label_leaf(session.arena(), search.node)?;
                search.comparison = Some(BlobComparison {
                    descriptor: leaf,
                    cursor: PersistentByteBlobReadCursor::try_new(PersistentByteBlobMetadata {
                        root: Some(leaf.blob),
                        bytes: leaf.bytes,
                        chunks: leaf.chunks,
                        height: leaf.height,
                    })?,
                    offset: 0,
                });
                search.decision = Some(SearchDecision::Leaf { leaf });
            }
            SequenceNodeKind::Branch { left, right } => {
                if summary.height > MAX_SEQUENCE_HEIGHT {
                    return Err(ReferenceLabelInternerError::Corrupt(
                        "label search exceeded sequence height",
                    ));
                }
                search.comparison = Some(BlobComparison {
                    descriptor: summary.separator,
                    cursor: PersistentByteBlobReadCursor::try_new(PersistentByteBlobMetadata {
                        root: Some(summary.separator.blob),
                        bytes: summary.separator.bytes,
                        chunks: summary.separator.chunks,
                        height: summary.separator.height,
                    })?,
                    offset: 0,
                });
                search.decision = Some(SearchDecision::Branch {
                    left,
                    right,
                    left_leaves: summary.left_leaves,
                });
            }
        }
        Ok(None)
    }

    fn poll_blob_comparison(
        &mut self,
        session: &ArenaBuildSession<'_>,
        comparison: &mut BlobComparison,
    ) -> Result<Option<Ordering>, ReferenceLabelInternerError> {
        comparison.descriptor.validate()?;
        let chunk = match comparison.cursor.poll(session.arena())? {
            PersistentBlobReadProgress::Pending => {
                self.receipt.pages_read = self
                    .receipt
                    .pages_read
                    .checked_add(1)
                    .ok_or(ReferenceLabelInternerError::Overflow("interner page reads"))?;
                return Ok(None);
            }
            PersistentBlobReadProgress::Chunk(chunk) => chunk,
            PersistentBlobReadProgress::Complete => {
                return Err(ReferenceLabelInternerError::Corrupt(
                    "label blob completed without a comparison decision",
                ));
            }
        };
        self.receipt.pages_read = self
            .receipt
            .pages_read
            .checked_add(1)
            .ok_or(ReferenceLabelInternerError::Overflow("interner page reads"))?;
        let bytes = chunk.bytes(session.arena())?;
        let start = usize::try_from(chunk.absolute_start)
            .map_err(|_| ReferenceLabelInternerError::Overflow("label blob chunk start"))?;
        let descriptor_bytes = usize::try_from(comparison.descriptor.bytes)
            .map_err(|_| ReferenceLabelInternerError::Overflow("label blob length"))?;
        if start != comparison.offset
            || start.checked_add(bytes.len()).is_none()
            || start + bytes.len() > descriptor_bytes
        {
            return Err(ReferenceLabelInternerError::Corrupt(
                "label blob chunks are crossed",
            ));
        }
        let (candidate_len, candidate_slice_len, common, ordering) = {
            let candidate = self.pending_bytes()?;
            let candidate_end = candidate.len().min(start + bytes.len());
            let candidate_slice =
                candidate
                    .get(start..candidate_end)
                    .ok_or(ReferenceLabelInternerError::Corrupt(
                        "candidate label comparison escaped its bytes",
                    ))?;
            let common = candidate_slice.len().min(bytes.len());
            (
                candidate.len(),
                candidate_slice.len(),
                common,
                candidate_slice[..common].cmp(&bytes[..common]),
            )
        };
        self.receipt.exact_bytes_compared = self
            .receipt
            .exact_bytes_compared
            .checked_add(
                u64::try_from(common)
                    .map_err(|_| ReferenceLabelInternerError::Overflow("label comparison bytes"))?,
            )
            .ok_or(ReferenceLabelInternerError::Overflow(
                "label comparison bytes",
            ))?;
        if ordering != Ordering::Equal {
            return Ok(Some(ordering));
        }
        if candidate_slice_len < bytes.len() {
            return Ok(Some(Ordering::Less));
        }
        comparison.offset = start + bytes.len();
        if candidate_len <= comparison.offset && comparison.offset < descriptor_bytes {
            Ok(Some(Ordering::Less))
        } else if comparison.offset == descriptor_bytes {
            Ok(Some(candidate_len.cmp(&comparison.offset)))
        } else {
            Ok(None)
        }
    }

    fn install_ready_join(
        &mut self,
        root: ArenaId,
        label_id: u64,
    ) -> Result<(), ReferenceLabelInternerError> {
        let request_nonce = self
            .pending
            .as_ref()
            .ok_or(ReferenceLabelInternerError::Corrupt(
                "ready label lost its parser request",
            ))?
            .request_nonce;
        self.pending_join = Some(LabelJoinIdentity {
            build: self.build,
            interner_generation: self.generation,
            interner_root: root,
            label_id,
            request_nonce,
        });
        self.pending = None;
        Ok(())
    }

    pub(crate) fn take_label(
        &mut self,
    ) -> Result<InternedReferenceLabel, ReferenceLabelInternerError> {
        if !matches!(self.phase, InternerPhase::LabelReady) {
            return Err(ReferenceLabelInternerError::Invalid(
                "interner label is not ready",
            ));
        }
        let join = self
            .pending_join
            .ok_or(ReferenceLabelInternerError::Corrupt(
                "ready label join disappeared",
            ))?;
        self.phase = InternerPhase::AwaitingAck;
        Ok(InternedReferenceLabel { join })
    }

    pub(crate) fn acknowledge_label_use(
        &mut self,
        mut ack: ReferenceLabelUseAck,
    ) -> Result<(), ReferenceLabelInternerError> {
        if !matches!(self.phase, InternerPhase::AwaitingAck) {
            return Err(ReferenceLabelInternerError::ReplayedAck);
        }
        let actual = ack
            .join
            .take()
            .ok_or(ReferenceLabelInternerError::ReplayedAck)?;
        let expected = self
            .pending_join
            .ok_or(ReferenceLabelInternerError::Corrupt(
                "interner awaiting ack lost its join identity",
            ))?;
        if actual != expected
            || self.root_id != Some(actual.interner_root)
            || actual.build != self.build
            || actual.interner_generation != self.generation
        {
            return Err(ReferenceLabelInternerError::CrossedAck);
        }
        self.pending_join = None;
        self.pending_label_id = None;
        self.phase = InternerPhase::Ready;
        Ok(())
    }

    pub(crate) fn begin_finish(&mut self) -> Result<(), ReferenceLabelInternerError> {
        if !matches!(self.phase, InternerPhase::Ready)
            || self.pending.is_some()
            || self.pending_join.is_some()
        {
            return Err(ReferenceLabelInternerError::Invalid(
                "interner finish requires no pending label or acknowledgement",
            ));
        }
        self.phase = InternerPhase::AllocateManifest;
        Ok(())
    }

    pub(crate) fn take_manifest(
        &mut self,
    ) -> Result<ReferenceLabelInternerManifest, ReferenceLabelInternerError> {
        if !matches!(self.phase, InternerPhase::ManifestReady) {
            return Err(ReferenceLabelInternerError::Invalid(
                "interner manifest is not ready",
            ));
        }
        let owner = self
            .manifest
            .take()
            .ok_or(ReferenceLabelInternerError::Corrupt(
                "ready interner manifest disappeared",
            ))?;
        self.phase = InternerPhase::Taken;
        Ok(ReferenceLabelInternerManifest {
            owner,
            identity: ReferenceLabelInternerManifestIdentity {
                build: self.build,
                generation: self.generation,
                label_count: self.count,
                label_id_high_water: self.id_high_water,
            },
        })
    }

    fn pending_bytes(&self) -> Result<&[u8], ReferenceLabelInternerError> {
        self.pending
            .as_ref()
            .map(|label| label.normalized.as_bytes())
            .ok_or(ReferenceLabelInternerError::Corrupt(
                "interner pending label disappeared",
            ))
    }

    fn record_allocation(
        &mut self,
        allocation: AllocationReceipt,
    ) -> Result<(), ReferenceLabelInternerError> {
        if allocation.payload_bytes_copied > ARENA_PAGE_BYTES {
            return Err(ReferenceLabelInternerError::Corrupt(
                "interner allocation exceeded arena page",
            ));
        }
        Ok(())
    }
}

fn encode_label_leaf_payload(key: LabelKeyDescriptor) -> [u8; LABEL_LEAF_BYTES] {
    let mut payload = [0_u8; LABEL_LEAF_BYTES];
    payload[0] = LABEL_LEAF_TAG;
    payload[1] = FORMAT_VERSION;
    put_u64(&mut payload, 8, key.label_id);
    put_u64(&mut payload, 16, key.bytes);
    put_u64(&mut payload, 24, key.chunks);
    put_u16(&mut payload, 32, key.height);
    encode_arena_id(&mut payload, 40, key.blob);
    payload
}

fn decode_label_leaf_payload(
    payload: &[u8],
) -> Result<LabelKeyDescriptor, ReferenceLabelInternerError> {
    if payload.len() != LABEL_LEAF_BYTES
        || payload[0] != LABEL_LEAF_TAG
        || payload[1] != FORMAT_VERSION
    {
        return Err(ReferenceLabelInternerError::Corrupt(
            "invalid label-directory leaf",
        ));
    }
    let key = LabelKeyDescriptor {
        label_id: get_u64(payload, 8),
        bytes: get_u64(payload, 16),
        chunks: get_u64(payload, 24),
        height: get_u16(payload, 32),
        blob: decode_arena_id(payload, 40),
    };
    key.validate()?;
    Ok(key)
}

fn decode_label_leaf(
    arena: &crate::PageArena,
    leaf: ArenaId,
) -> Result<LabelKeyDescriptor, ReferenceLabelInternerError> {
    let key = decode_label_leaf_payload(arena.payload(leaf)?)?;
    if arena.packed_child_count(leaf)? != 1 || arena.packed_child_at(leaf, 0)? != key.blob {
        return Err(ReferenceLabelInternerError::Corrupt(
            "label leaf does not retain its exact blob",
        ));
    }
    Ok(key)
}

fn encode_manifest(
    generation: u64,
    count: u64,
    id_high_water: u64,
    root: Option<ArenaId>,
    parent_manifest: Option<ArenaId>,
) -> [u8; LABEL_MANIFEST_BYTES] {
    let mut payload = [0_u8; LABEL_MANIFEST_BYTES];
    payload[0] = LABEL_MANIFEST_TAG;
    payload[1] = FORMAT_VERSION;
    payload[2] = u8::from(root.is_some());
    payload[3] = u8::from(parent_manifest.is_some());
    put_u64(&mut payload, 8, generation);
    put_u64(&mut payload, 16, count);
    put_u64(&mut payload, 24, id_high_water);
    if let Some(root) = root {
        encode_arena_id(&mut payload, 32, root);
    }
    if let Some(parent_manifest) = parent_manifest {
        encode_arena_id(&mut payload, 40, parent_manifest);
    }
    payload
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedLabelManifest {
    generation: u64,
    count: u64,
    label_id_high_water: u64,
    root: Option<ArenaId>,
    parent_manifest: Option<ArenaId>,
}

fn decode_manifest(
    arena: &PageArena,
    manifest: ArenaId,
) -> Result<DecodedLabelManifest, ReferenceLabelInternerError> {
    let payload = arena.payload(manifest)?;
    if payload.len() != LABEL_MANIFEST_BYTES
        || payload[0] != LABEL_MANIFEST_TAG
        || payload[1] != FORMAT_VERSION
        || payload[2] > 1
        || payload[3] > 1
    {
        return Err(ReferenceLabelInternerError::Corrupt(
            "invalid committed label manifest",
        ));
    }
    let has_root = payload[2] != 0;
    let has_parent = payload[3] != 0;
    let expected_children = usize::from(has_root) + usize::from(has_parent);
    if arena.packed_child_count(manifest)? != expected_children {
        return Err(ReferenceLabelInternerError::Corrupt(
            "committed label manifest child shape is invalid",
        ));
    }
    let root = has_root.then(|| decode_arena_id(payload, 32));
    let parent_manifest = has_parent.then(|| decode_arena_id(payload, 40));
    let mut child_index = 0;
    if let Some(root) = root {
        if arena.packed_child_at(manifest, child_index)? != root {
            return Err(ReferenceLabelInternerError::Corrupt(
                "committed label manifest crossed its exact root",
            ));
        }
        child_index += 1;
    }
    if let Some(parent) = parent_manifest {
        if arena.packed_child_at(manifest, child_index)? != parent {
            return Err(ReferenceLabelInternerError::Corrupt(
                "committed label manifest crossed its parent",
            ));
        }
    }
    let decoded = DecodedLabelManifest {
        generation: get_u64(payload, 8),
        count: get_u64(payload, 16),
        label_id_high_water: get_u64(payload, 24),
        root,
        parent_manifest,
    };
    if decoded.generation == 0
        || (decoded.count == 0) != decoded.root.is_none()
        || decoded.label_id_high_water < decoded.count
    {
        return Err(ReferenceLabelInternerError::Corrupt(
            "committed label manifest counters are inconsistent",
        ));
    }
    if let Some(root) = decoded.root {
        let summary = sequence_node::<LabelSequenceSpec>(arena, root)?.0;
        if summary.leaves != decoded.count {
            return Err(ReferenceLabelInternerError::Corrupt(
                "committed label manifest count crossed its exact root",
            ));
        }
    }
    if let Some(parent) = decoded.parent_manifest {
        let parent = decode_manifest_shallow(arena, parent)?;
        if decoded.generation
            != parent
                .generation
                .checked_add(1)
                .ok_or(ReferenceLabelInternerError::Overflow(
                    "committed interner parent generation",
                ))?
            || decoded.count < parent.count
            || decoded.label_id_high_water < parent.label_id_high_water
        {
            return Err(ReferenceLabelInternerError::Corrupt(
                "adopted label manifest regressed behind its parent",
            ));
        }
    }
    Ok(decoded)
}

fn decode_manifest_shallow(
    arena: &PageArena,
    manifest: ArenaId,
) -> Result<DecodedLabelManifest, ReferenceLabelInternerError> {
    let payload = arena.payload(manifest)?;
    if payload.len() != LABEL_MANIFEST_BYTES
        || payload[0] != LABEL_MANIFEST_TAG
        || payload[1] != FORMAT_VERSION
        || payload[2] > 1
        || payload[3] > 1
    {
        return Err(ReferenceLabelInternerError::Corrupt(
            "invalid parent label manifest",
        ));
    }
    let has_root = payload[2] != 0;
    let has_parent = payload[3] != 0;
    if arena.packed_child_count(manifest)? != usize::from(has_root) + usize::from(has_parent) {
        return Err(ReferenceLabelInternerError::Corrupt(
            "parent label manifest child shape is invalid",
        ));
    }
    Ok(DecodedLabelManifest {
        generation: get_u64(payload, 8),
        count: get_u64(payload, 16),
        label_id_high_water: get_u64(payload, 24),
        root: has_root.then(|| decode_arena_id(payload, 32)),
        parent_manifest: has_parent.then(|| decode_arena_id(payload, 40)),
    })
}

fn encode_key_descriptor(output: &mut [u8], offset: usize, key: LabelKeyDescriptor) {
    encode_arena_id(output, offset, key.blob);
    put_u64(output, offset + 8, key.bytes);
    put_u64(output, offset + 16, key.chunks);
    put_u16(output, offset + 24, key.height);
    put_u64(output, offset + 32, key.label_id);
}

fn decode_key_descriptor(input: &[u8], offset: usize) -> LabelKeyDescriptor {
    LabelKeyDescriptor {
        blob: decode_arena_id(input, offset),
        bytes: get_u64(input, offset + 8),
        chunks: get_u64(input, offset + 16),
        height: get_u16(input, offset + 24),
        label_id: get_u64(input, offset + 32),
    }
}

fn encode_arena_id(output: &mut [u8], offset: usize, id: ArenaId) {
    put_u32(output, offset, id.index);
    put_u32(output, offset + 4, id.generation);
}

fn decode_arena_id(input: &[u8], offset: usize) -> ArenaId {
    ArenaId {
        index: get_u32(input, offset),
        generation: get_u32(input, offset + 4),
    }
}

fn put_u16(output: &mut [u8], offset: usize, value: u16) {
    output[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(output: &mut [u8], offset: usize, value: u32) {
    output[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(output: &mut [u8], offset: usize, value: u64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn get_u16(input: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(input[offset..offset + 2].try_into().expect("u16 field"))
}

fn get_u32(input: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(input[offset..offset + 4].try_into().expect("u32 field"))
}

fn get_u64(input: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(input[offset..offset + 8].try_into().expect("u64 field"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PageArena;

    fn drive_ready(
        arena: &mut PageArena,
        ticket: &mut Option<crate::ArenaBuildTicket>,
        interner: &mut ReferenceLabelInterner,
    ) -> InternedReferenceLabel {
        for _ in 0..100_000 {
            let current = ticket.take().expect("test ticket");
            let mut session = arena.resume_build(current).unwrap();
            let progress = interner.poll(&mut session).unwrap();
            *ticket = Some(session.suspend().unwrap());
            if progress == ReferenceLabelInternerProgress::LabelReady {
                return interner.take_label().unwrap();
            }
        }
        panic!("interner did not produce a label")
    }

    fn intern(
        arena: &mut PageArena,
        ticket: &mut Option<crate::ArenaBuildTicket>,
        interner: &mut ReferenceLabelInterner,
        label: &str,
        nonce: u64,
    ) -> u64 {
        interner
            .begin_intern(CandidateReferenceLabel::proof_only(label, nonce))
            .unwrap();
        let ready = drive_ready(arena, ticket, interner);
        let (id, ack) = ready.proof_only_consume_for_index();
        interner.acknowledge_label_use(ack).unwrap();
        id
    }

    #[test]
    fn exact_duplicates_reuse_stable_ids_and_prefixes_do_not_alias() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let build = ticket.id();
        let mut ticket = Some(ticket);
        let mut interner = ReferenceLabelInterner::new_initial(build, 1).unwrap();

        let alpha = intern(&mut arena, &mut ticket, &mut interner, "alpha", 1);
        let alphabet = intern(&mut arena, &mut ticket, &mut interner, "alphabet", 2);
        let alpha_again = intern(&mut arena, &mut ticket, &mut interner, "alpha", 3);
        let unicode = intern(&mut arena, &mut ticket, &mut interner, "strasse", 4);

        assert_eq!(alpha, alpha_again);
        assert_ne!(alpha, alphabet);
        assert_ne!(alpha, unicode);
        assert_eq!(interner.receipt().labels_inserted, 3);
        assert_eq!(interner.receipt().labels_reused, 1);
        assert!(interner.receipt().maximum_pages_read_per_poll <= 1);
        assert!(interner.receipt().maximum_pages_allocated_per_poll <= 1);
    }

    #[test]
    fn worst_case_shared_prefix_crosses_blob_pages_without_hash_authority() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let build = ticket.id();
        let mut ticket = Some(ticket);
        let mut interner = ReferenceLabelInterner::new_initial(build, 7).unwrap();
        let prefix = "a".repeat(MAX_NORMALIZED_REFERENCE_LABEL_BYTES - 1);
        let first = format!("{prefix}b");
        let second = format!("{prefix}c");

        let first_id = intern(&mut arena, &mut ticket, &mut interner, &first, 11);
        let second_id = intern(&mut arena, &mut ticket, &mut interner, &second, 12);
        let first_again = intern(&mut arena, &mut ticket, &mut interner, &first, 13);

        assert_ne!(first_id, second_id);
        assert_eq!(first_id, first_again);
        assert!(interner.receipt().maximum_blob_pages_per_label >= 2);
        assert!(interner.receipt().exact_bytes_compared >= prefix.len() as u64);
        assert!(interner.receipt().maximum_pages_read_per_poll <= 1);
        assert!(interner.receipt().maximum_pages_allocated_per_poll <= 1);
    }

    #[test]
    fn sorted_insertions_remain_balanced_and_poll_bounded() {
        const COUNT: usize = 4_096;
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let build = ticket.id();
        let mut ticket = Some(ticket);
        let mut interner = ReferenceLabelInterner::new_initial(build, 3).unwrap();

        for index in 0..COUNT {
            let label = format!("label-{index:08}");
            let id = intern(
                &mut arena,
                &mut ticket,
                &mut interner,
                &label,
                index as u64 + 1,
            );
            assert_eq!(id, index as u64 + 1);
        }

        let root = interner.root_id.unwrap();
        let summary = sequence_node::<LabelSequenceSpec>(&arena, root).unwrap().0;
        assert_eq!(summary.leaves, COUNT as u64);
        assert!(
            summary.height < 32,
            "sorted insertion lost balance: {summary:?}"
        );
        assert!(interner.receipt().maximum_pages_read_per_poll <= 1);
        assert!(interner.receipt().maximum_pages_allocated_per_poll <= 1);
    }

    #[test]
    fn crossed_and_replayed_ack_fail_closed() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let build = ticket.id();
        let mut ticket = Some(ticket);
        let mut first = ReferenceLabelInterner::new_initial(build, 1).unwrap();
        first
            .begin_intern(CandidateReferenceLabel::proof_only("x", 1))
            .unwrap();
        let ready = drive_ready(&mut arena, &mut ticket, &mut first);
        let (_, mut ack) = ready.proof_only_consume_for_index();
        let original = ack.join.unwrap();
        ack.join = Some(LabelJoinIdentity {
            interner_generation: original.interner_generation + 1,
            ..original
        });
        assert_eq!(
            first.acknowledge_label_use(ack),
            Err(ReferenceLabelInternerError::CrossedAck)
        );
        // A crossed acknowledgement burns its own value but does not rearm
        // the interner; cancellation of the unpublished build is now required.
        assert!(matches!(first.phase, InternerPhase::AwaitingAck));
    }

    #[test]
    fn injected_fault_leaves_root_unpublishable_inside_build_journal() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let build = ticket.id();
        let mut interner = ReferenceLabelInterner::new_initial(build, 1)
            .unwrap()
            .with_fault_after_poll(1);
        interner
            .begin_intern(CandidateReferenceLabel::proof_only("fault", 1))
            .unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        assert_eq!(
            interner.poll(&mut session),
            Err(ReferenceLabelInternerError::InjectedFault(1))
        );
        let abort = session.begin_abort().unwrap();
        for _ in 0..100 {
            if arena.poll_build_abort(abort, 1).unwrap().complete {
                return;
            }
        }
        panic!("faulted interner did not retire through the fuelled build journal")
    }
}

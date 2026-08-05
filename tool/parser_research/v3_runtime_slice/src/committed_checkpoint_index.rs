//! Storage-only prototype of the committed checkpoint index.
//!
//! A standalone committed index is storage/query authority only. Exact donor
//! state may be reconstructed internally, but the non-test crate restricts
//! donor lookup to this module and the restart-authoritative composite parent;
//! no independently owned index can become the product resume gateway.
//!
//! Document order has one measured persistent sequence of partitions. A large
//! normalization group is one outer partition whose private manifest owns a
//! second, group-relative measured sequence of sparse samples. Keeping samples
//! below the group manifest avoids rebasing the rest of the document and lets
//! a changed group path-copy only its own sample tree.

#[cfg(feature = "exact-parser")]
pub(crate) mod suffix_splice;

use std::fmt;

use crate::arena::{
    ARENA_PAGE_BYTES, ArenaBuildError, ArenaBuildId, ArenaBuildOwner, ArenaBuildSession,
    ArenaBuildTicket, ArenaError, ArenaId, ArenaScopedId, MAX_PACKED_ARENA_CHILDREN, OwnedArenaRef,
    PageArena,
};
#[cfg(feature = "exact-parser")]
use crate::candidate_writer::FinalizedWriterCheckpointPartition;
#[cfg(feature = "exact-parser")]
use crate::indexed_donor_checkpoint::{
    DONOR_FRAME_BYTES, DONOR_HEADER_BYTES, IndexedDonorCheckpointRecipe, OpaqueDonorCaptureDraft,
    OpaqueDonorFrame, OpaqueDonorHeader, OpaqueDonorIdentityWitness,
};
use crate::persistent_sequence::{
    ResumableSequenceProgress, ResumableStreamingSequenceBuilder, SequenceMutationReceipt,
    SequenceNodeKind, SequenceSpec, sequence_node,
};
#[cfg(feature = "exact-parser")]
use crate::serialized_green::{GreenHeadingOpenFacts, GreenHeadingStyle};
#[cfg(feature = "exact-parser")]
use crate::{BlockId, LiveCandidateEpoch};
#[cfg(feature = "exact-parser")]
use flark_comrak_value_block_core::{
    DirectDurableGrammarCapture, DirectGrammarContinuation, DirectRestartLineLocalContinuation,
    ParseError,
};

const FORMAT_VERSION: u8 = 2;
const INDEX_LEAF_TAG: u8 = 0xc1;
const INDEX_BRANCH_TAG: u8 = 0xc2;
const NORMALIZATION_MANIFEST_TAG: u8 = 0xc3;
const SAMPLE_LEAF_TAG: u8 = 0xc4;
const SAMPLE_BRANCH_TAG: u8 = 0xc5;
#[cfg(feature = "exact-parser")]
const DONOR_SAMPLE_LEAF_TAG: u8 = 0xc6;
#[cfg(feature = "exact-parser")]
const DONOR_SAMPLE_BRANCH_TAG: u8 = 0xc7;
#[cfg(feature = "exact-parser")]
const DONOR_PATH_NODE_TAG: u8 = 0xc8;
#[cfg(feature = "exact-parser")]
const DONOR_PARTITION_MANIFEST_TAG: u8 = 0xc9;

const SUMMARY_BYTES: usize = 80;
const PARTITION_RECORD_BYTES: usize = 56;
const SAMPLE_RECORD_BYTES: usize = 40;
/// Exact-only extension of the legacy 40-byte relative measure with the
/// donor's 64-byte opaque header. Its terminal path is child-at-record-index,
/// so no raw arena ID or redundant child ordinal enters the payload.
#[cfg(feature = "exact-parser")]
const DONOR_SAMPLE_RECORD_BYTES: usize = SAMPLE_RECORD_BYTES + DONOR_HEADER_BYTES;
#[cfg(feature = "exact-parser")]
const DONOR_PATH_NODE_BYTES: usize = 8 + DONOR_FRAME_BYTES;
const MANIFEST_BYTES: usize = 80;
const NO_CHILD_ORDINAL: u16 = u16::MAX;

const DIRECT_PARTITION_TAG: u8 = 1;
const NORMALIZATION_PARTITION_TAG: u8 = 2;
#[cfg(feature = "exact-parser")]
const DONOR_PARTITION_TAG: u8 = 3;
const TERMINAL_TAIL_PARTITION_TAG: u8 = 4;
const SETEXT_HEADING_OUTCOME_TAG: u8 = 1;
#[cfg(feature = "exact-parser")]
const DONOR_DIRECT_ROLE_TAG: u8 = 1;
#[cfg(feature = "exact-parser")]
const DONOR_NORMALIZATION_ROLE_TAG: u8 = 2;

/// Additive, coordinate-free coverage of one index interval.
///
/// No field is an absolute document position. Queries recover absolute
/// positions exclusively by folding sequence summaries on their search path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RelativeCheckpointMeasure {
    source_bytes: u64,
    source_utf16: u64,
    physical_lines: u64,
    green_events: u64,
    projection_runs: u64,
}

impl RelativeCheckpointMeasure {
    pub(crate) const fn new(
        source_bytes: u64,
        source_utf16: u64,
        physical_lines: u64,
        green_events: u64,
        projection_runs: u64,
    ) -> Self {
        Self {
            source_bytes,
            source_utf16,
            physical_lines,
            green_events,
            projection_runs,
        }
    }

    pub(crate) const fn source_bytes(self) -> u64 {
        self.source_bytes
    }

    pub(crate) const fn source_utf16(self) -> u64 {
        self.source_utf16
    }

    pub(crate) const fn physical_lines(self) -> u64 {
        self.physical_lines
    }

    pub(crate) const fn green_events(self) -> u64 {
        self.green_events
    }

    pub(crate) const fn projection_runs(self) -> u64 {
        self.projection_runs
    }

    pub(crate) fn checked_add(self, suffix: Self) -> Result<Self, CommittedCheckpointIndexError> {
        Ok(Self {
            source_bytes: checked_add(self.source_bytes, suffix.source_bytes, "source bytes")?,
            source_utf16: checked_add(self.source_utf16, suffix.source_utf16, "source UTF-16")?,
            physical_lines: checked_add(
                self.physical_lines,
                suffix.physical_lines,
                "physical lines",
            )?,
            green_events: checked_add(self.green_events, suffix.green_events, "green events")?,
            projection_runs: checked_add(
                self.projection_runs,
                suffix.projection_runs,
                "projection runs",
            )?,
        })
    }

    /// Recovers one relative interval from two actor-observed cumulative
    /// cuts. Any regressing axis rejects the whole cut; the caller cannot
    /// silently mix coordinates from different checkpoints.
    pub(crate) fn checked_difference_from(
        self,
        prefix: Self,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        Ok(Self {
            source_bytes: checked_difference(
                self.source_bytes,
                prefix.source_bytes,
                "checkpoint source-byte cut regresses",
            )?,
            source_utf16: checked_difference(
                self.source_utf16,
                prefix.source_utf16,
                "checkpoint source-UTF-16 cut regresses",
            )?,
            physical_lines: checked_difference(
                self.physical_lines,
                prefix.physical_lines,
                "checkpoint physical-line cut regresses",
            )?,
            green_events: checked_difference(
                self.green_events,
                prefix.green_events,
                "checkpoint green-event cut regresses",
            )?,
            projection_runs: checked_difference(
                self.projection_runs,
                prefix.projection_runs,
                "checkpoint projection-run cut regresses",
            )?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CheckpointIndexSummary {
    leaf_pages: u64,
    partitions: u64,
    samples: u64,
    height: u16,
    measure: RelativeCheckpointMeasure,
    terminal_tail: bool,
}

impl CheckpointIndexSummary {
    fn followed_by(self, suffix: Self) -> Result<Self, CommittedCheckpointIndexError> {
        if self.terminal_tail && suffix.partitions != 0 {
            return Err(CommittedCheckpointIndexError::Invalid(
                "terminal tail is not the final checkpoint-index partition",
            ));
        }
        if self.terminal_tail && suffix.terminal_tail {
            return Err(CommittedCheckpointIndexError::Invalid(
                "checkpoint index has more than one terminal tail",
            ));
        }
        Ok(Self {
            leaf_pages: checked_add(self.leaf_pages, suffix.leaf_pages, "leaf pages")?,
            partitions: checked_add(self.partitions, suffix.partitions, "partitions")?,
            samples: checked_add(self.samples, suffix.samples, "samples")?,
            height: match (self.height, suffix.height) {
                (0, right) => right,
                (left, 0) => left,
                (left, right) => left
                    .max(right)
                    .checked_add(1)
                    .ok_or(CommittedCheckpointIndexError::Overflow("tree height"))?,
            },
            measure: self.measure.checked_add(suffix.measure)?,
            terminal_tail: self.terminal_tail || suffix.terminal_tail,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StorageOnlyNormalizationOutcome {
    SetextHeading { level: u8 },
}

impl StorageOnlyNormalizationOutcome {
    fn encode(self) -> Result<(u8, u8), CommittedCheckpointIndexError> {
        match self {
            Self::SetextHeading {
                level: level @ (1 | 2),
            } => Ok((SETEXT_HEADING_OUTCOME_TAG, level)),
            Self::SetextHeading { .. } => Err(CommittedCheckpointIndexError::Invalid(
                "Setext level is outside the selected profile",
            )),
        }
    }

    fn decode(tag: u8, detail: u8) -> Result<Self, CommittedCheckpointIndexError> {
        match (tag, detail) {
            (SETEXT_HEADING_OUTCOME_TAG, level @ (1 | 2)) => Ok(Self::SetextHeading { level }),
            _ => Err(CommittedCheckpointIndexError::Corrupt(
                "unknown normalization outcome",
            )),
        }
    }
}

#[derive(Debug)]
pub(crate) enum StorageOnlyCheckpointPartition {
    Direct {
        interval: RelativeCheckpointMeasure,
    },
    NormalizationGroup {
        group: u64,
        outcome: StorageOnlyNormalizationOutcome,
        samples: Vec<RelativeCheckpointMeasure>,
    },
    /// Exact-parser-only donor samples. Direct runs use `group == None`;
    /// normalization regions carry their typed group and outcome but share the
    /// identical sample/path storage schema.
    #[cfg(feature = "exact-parser")]
    Donor {
        group: Option<(u64, StorageOnlyNormalizationOutcome)>,
        samples: Vec<DonorCheckpointSampleDraft>,
    },
    /// Final semantic progress emitted after the last restart-bearing physical
    /// line cut. This partition is never donor state and advances no source
    /// coordinate; its sole purpose is to make the committed index total
    /// exactly equal the completed green/composer totals.
    TerminalTail {
        interval: RelativeCheckpointMeasure,
    },
}

impl StorageOnlyCheckpointPartition {
    pub(crate) const fn direct(interval: RelativeCheckpointMeasure) -> Self {
        Self::Direct { interval }
    }

    pub(crate) const fn terminal_tail(interval: RelativeCheckpointMeasure) -> Self {
        Self::TerminalTail { interval }
    }

    pub(crate) fn normalization_group(
        group: u64,
        outcome: StorageOnlyNormalizationOutcome,
        samples: Vec<RelativeCheckpointMeasure>,
    ) -> Self {
        Self::NormalizationGroup {
            group,
            outcome,
            samples,
        }
    }

    #[cfg(feature = "exact-parser")]
    pub(crate) fn donor_direct(sample: DonorCheckpointSampleDraft) -> Self {
        Self::Donor {
            group: None,
            samples: vec![sample],
        }
    }

    #[cfg(feature = "exact-parser")]
    pub(crate) fn donor_direct_samples(samples: Vec<DonorCheckpointSampleDraft>) -> Self {
        Self::Donor {
            group: None,
            samples,
        }
    }

    /// Sole production bridge from writer-owned sample grouping into durable
    /// index partitions. Role, provisional Paragraph identity, final Setext
    /// facts, and sample membership are all consumed from one non-cloneable
    /// writer token; this constructor accepts no caller-authored role scalar.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn from_finalized_writer_partition(
        partition: FinalizedWriterCheckpointPartition,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        let (samples, normalization) = partition.into_checkpoint_index_parts();
        if samples.is_empty() {
            return Err(CommittedCheckpointIndexError::Invalid(
                "writer finalized an empty donor checkpoint partition",
            ));
        }
        let mut total = RelativeCheckpointMeasure::default();
        for sample in &samples {
            require_nonempty_interval(sample.interval())?;
            total = total.checked_add(sample.interval())?;
        }
        if total == RelativeCheckpointMeasure::default() {
            return Err(CommittedCheckpointIndexError::Invalid(
                "writer checkpoint partition has no progress",
            ));
        }
        let group = match normalization {
            None => None,
            Some((block, final_heading)) => {
                if block.0 == 0 || final_heading.style() != GreenHeadingStyle::Setext {
                    return Err(CommittedCheckpointIndexError::Invalid(
                        "writer normalization partition lacks typed Setext authority",
                    ));
                }
                let outcome = StorageOnlyNormalizationOutcome::SetextHeading {
                    level: final_heading.level(),
                };
                outcome.encode()?;
                Some((block.0, outcome))
            }
        };
        Ok(Self::Donor { group, samples })
    }

    #[cfg(all(feature = "exact-parser", test))]
    pub(crate) fn donor_normalization_group(
        group: u64,
        outcome: StorageOnlyNormalizationOutcome,
        samples: Vec<DonorCheckpointSampleDraft>,
    ) -> Self {
        Self::Donor {
            group: Some((group, outcome)),
            samples,
        }
    }
}

/// Typed exact-parser input. Construction consumes a donor capture; there is
/// no constructor accepting a raw 64-byte header or raw 48-byte frame list.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct DonorCheckpointSampleDraft {
    interval: RelativeCheckpointMeasure,
    donor: OpaqueDonorCaptureDraft,
}

#[cfg(feature = "exact-parser")]
impl DonorCheckpointSampleDraft {
    pub(crate) fn try_new(
        interval: RelativeCheckpointMeasure,
        capture: DirectDurableGrammarCapture,
    ) -> Result<Self, CommittedCheckpointIndexError> {
        require_nonempty_interval(interval)?;
        let donor = OpaqueDonorCaptureDraft::try_from_capture(capture)
            .map_err(CommittedCheckpointIndexError::Allocation)?;
        if donor.frames().is_empty() || donor.retained_source_bytes() != 0 {
            return Err(CommittedCheckpointIndexError::Invalid(
                "donor checkpoint must contain a source-free open path",
            ));
        }
        Ok(Self { interval, donor })
    }

    /// Integrated first-sample constructor. The duplicate opaque witness is
    /// retained only by the in-memory restart half; callers never receive raw
    /// header/frame bytes with which to author a different donor recipe.
    pub(crate) fn try_new_with_identity_witness(
        interval: RelativeCheckpointMeasure,
        capture: DirectDurableGrammarCapture,
    ) -> Result<(Self, OpaqueDonorIdentityWitness), CommittedCheckpointIndexError> {
        let sample = Self::try_new(interval, capture)?;
        let witness = sample
            .donor
            .identity_witness()
            .map_err(CommittedCheckpointIndexError::Allocation)?;
        Ok((sample, witness))
    }

    pub(crate) const fn interval(&self) -> RelativeCheckpointMeasure {
        self.interval
    }

    pub(crate) fn path_depth(&self) -> usize {
        self.donor.frames().len()
    }

    #[cfg(test)]
    pub(crate) fn retained_source_bytes_for_test(&self) -> usize {
        self.donor.retained_source_bytes()
    }
}

/// Heap-only cancellation payload for unencoded donor drafts. Every unit of
/// fuel destroys at most one draft or one auxiliary frame vector. A draft's
/// frame vector contains fixed-size byte arrays with no destructor, so its
/// backing allocation is released in O(1); the potentially unbounded work is
/// walking the outer draft chain, which this type keeps cooperative.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct DonorCheckpointHeapRetirement {
    samples: std::vec::IntoIter<DonorCheckpointSampleDraft>,
    final_sample: Option<DonorCheckpointSampleDraft>,
    extra_frames_a: Option<Vec<OpaqueDonorFrame>>,
    extra_frames_b: Option<Vec<OpaqueDonorFrame>>,
}

#[cfg(feature = "exact-parser")]
impl DonorCheckpointHeapRetirement {
    pub(crate) fn empty() -> Self {
        Self::from_samples(Vec::new())
    }

    pub(crate) fn from_samples(samples: Vec<DonorCheckpointSampleDraft>) -> Self {
        Self {
            samples: samples.into_iter(),
            final_sample: None,
            extra_frames_a: None,
            extra_frames_b: None,
        }
    }

    pub(in crate::committed_checkpoint_index) fn from_segmented(
        samples: std::vec::IntoIter<DonorCheckpointSampleDraft>,
        final_sample: Option<DonorCheckpointSampleDraft>,
        extra_frames_a: Option<Vec<OpaqueDonorFrame>>,
        extra_frames_b: Option<Vec<OpaqueDonorFrame>>,
    ) -> Self {
        Self {
            samples,
            final_sample,
            extra_frames_a,
            extra_frames_b,
        }
    }

    /// Returns the number of bounded heap releases performed.
    pub(crate) fn poll(&mut self, fuel: usize) -> usize {
        let mut transitions = 0;
        while transitions < fuel && self.poll_one() {
            transitions += 1;
        }
        transitions
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.extra_frames_a.is_none()
            && self.extra_frames_b.is_none()
            && self.samples.len() == 0
            && self.final_sample.is_none()
    }

    fn poll_one(&mut self) -> bool {
        if self.extra_frames_a.take().is_some() {
            return true;
        }
        if self.extra_frames_b.take().is_some() {
            return true;
        }
        if self.samples.next().is_some() {
            return true;
        }
        if self.final_sample.take().is_some() {
            return true;
        }
        false
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CommittedCheckpointIndexBuildReceipt {
    pub(crate) outer_leaf_pages: usize,
    pub(crate) sample_leaf_pages: usize,
    pub(crate) normalization_manifests: usize,
    pub(crate) maximum_page_payload_bytes: usize,
    pub(crate) payload_bytes_copied: usize,
    pub(crate) edge_bytes_copied: usize,
    pub(crate) sequence: SequenceMutationReceipt,
    #[cfg(feature = "exact-parser")]
    pub(crate) donor_sample_headers: usize,
    #[cfg(feature = "exact-parser")]
    pub(crate) donor_partition_manifests: usize,
    #[cfg(feature = "exact-parser")]
    pub(crate) donor_sample_header_bytes: usize,
    #[cfg(feature = "exact-parser")]
    pub(crate) donor_materialized_path_records: usize,
    #[cfg(feature = "exact-parser")]
    pub(crate) donor_materialized_path_bytes: usize,
    #[cfg(feature = "exact-parser")]
    pub(crate) maximum_donor_capture_conversion_scratch_bytes: usize,
    #[cfg(feature = "exact-parser")]
    pub(crate) maximum_donor_partition_draft_bytes: usize,
    #[cfg(feature = "exact-parser")]
    pub(crate) donor_builder_queued_draft_bytes: usize,
    #[cfg(feature = "exact-parser")]
    pub(crate) maximum_donor_path_build_scratch_bytes: usize,
    #[cfg(feature = "exact-parser")]
    pub(crate) donor_path_nodes_allocated: usize,
    #[cfg(feature = "exact-parser")]
    pub(crate) donor_path_prefix_records_reused: usize,
    #[cfg(feature = "exact-parser")]
    pub(crate) donor_sample_path_edges: usize,
    #[cfg(feature = "exact-parser")]
    pub(crate) donor_retained_payload_bytes: usize,
    #[cfg(feature = "exact-parser")]
    pub(crate) donor_retained_edge_bytes: usize,
    #[cfg(feature = "exact-parser")]
    pub(crate) retained_source_bytes: usize,
}

/// The one build-journal-owned index root before either direct test commit or
/// transfer beneath the storage-only composite document manifest.
///
/// Keeping the owner behind this type prevents an arbitrary arena page from
/// being attached as the checkpoint-index child without first passing the
/// index decoder and exact build-generation checks.
#[derive(Debug)]
pub(crate) struct StorageOnlyCheckpointIndexBuildManifest {
    build: ArenaBuildId,
    owner: ArenaBuildOwner,
    receipt: CommittedCheckpointIndexBuildReceipt,
}

/// Revalidated identity and complete summary of the checkpoint-index child
/// accepted by a same-arena composite parent. The root stays module-private:
/// callers can compare semantic totals, but cannot use this value as a raw
/// independently ownable child handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommittedCheckpointIndexCompositeDescriptor {
    root: ArenaId,
    summary: CheckpointIndexSummary,
}

impl CommittedCheckpointIndexCompositeDescriptor {
    pub(crate) const fn final_measure(self) -> RelativeCheckpointMeasure {
        self.summary.measure
    }

    pub(crate) const fn leaf_pages(self) -> u64 {
        self.summary.leaf_pages
    }

    pub(crate) const fn partitions(self) -> u64 {
        self.summary.partitions
    }

    pub(crate) const fn samples(self) -> u64 {
        self.summary.samples
    }

    pub(crate) const fn height(self) -> u16 {
        self.summary.height
    }

    pub(crate) const fn has_terminal_tail(self) -> bool {
        self.summary.terminal_tail
    }
}

/// Typed, parent-derived borrow of the checkpoint child already retained by
/// a fresh adoption journal. This is the sole bridge intended for
/// `suffix_splice`: it cannot outlive the two-child parent lease, owns no
/// independently transferable reference, and exposes no root to its caller.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentRetainedCheckpointIndexLease<'lease> {
    build: ArenaBuildId,
    parent_activation: ArenaScopedId,
    owner: &'lease ArenaBuildOwner,
    descriptor: CommittedCheckpointIndexCompositeDescriptor,
}

#[cfg(feature = "exact-parser")]
impl<'lease> ParentRetainedCheckpointIndexLease<'lease> {
    pub(crate) fn from_parent_mint(
        mint: crate::storage_only_composite_document::RestartCheckpointLeaseMint<'lease>,
    ) -> Self {
        let (build, parent_activation, owner, descriptor) = mint.into_checkpoint_lease_parts();
        Self {
            build,
            parent_activation,
            owner,
            descriptor,
        }
    }

    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    /// Test-only bridge for the pre-composite splice mechanism fixture. The
    /// production constructor remains the private composite-parent mint; this
    /// helper still requires a live ABA-safe activation stamp and retained
    /// journal owner so the fixture exercises the same lifecycle checks.
    #[cfg(test)]
    pub(super) const fn mechanism_only_from_retained_test_index(
        build: ArenaBuildId,
        parent_activation: ArenaScopedId,
        owner: &'lease ArenaBuildOwner,
        descriptor: CommittedCheckpointIndexCompositeDescriptor,
    ) -> Self {
        Self {
            build,
            parent_activation,
            owner,
            descriptor,
        }
    }

    pub(crate) fn validate_session(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<(), CommittedCheckpointIndexError> {
        self.validated_root(session).map(|_| ())
    }

    /// Available to this module's splice child, but never returned through
    /// the composite-parent API.
    fn validated_root(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<ArenaId, CommittedCheckpointIndexError> {
        if session.id() != self.build {
            return Err(CommittedCheckpointIndexError::Invalid(
                "parent-retained checkpoint lease and arena build differ",
            ));
        }
        session.arena().local_id(self.parent_activation)?;
        let root = session.owner_id(self.owner)?;
        let descriptor =
            validate_committed_checkpoint_index_composite_child(session.arena(), root)?;
        if descriptor != self.descriptor {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "parent-retained checkpoint lease descriptor changed",
            ));
        }
        Ok(root)
    }

    /// Suspended read sibling used while the exact parser and candidate writer
    /// are jointly parked at a line boundary. The ticket proves the retained
    /// journal cannot mutate; no build session or independently transferable
    /// index handle is manufactured.
    fn validated_suspended_root(
        &self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
    ) -> Result<ArenaId, CommittedCheckpointIndexError> {
        if ticket.id() != self.build {
            return Err(CommittedCheckpointIndexError::Invalid(
                "parent-retained checkpoint lease and suspended build differ",
            ));
        }
        arena.local_id(self.parent_activation)?;
        let root = arena.suspended_owner_id(ticket, self.owner)?;
        let descriptor = validate_committed_checkpoint_index_composite_child(arena, root)?;
        if descriptor != self.descriptor {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "parent-retained checkpoint lease descriptor changed while suspended",
            ));
        }
        Ok(root)
    }
}

/// Read-side composite validation. The caller supplies the child selected by
/// a parent edge; this decoder authenticates the exact committed root and
/// folds its final summary before returning semantic totals.
pub(crate) fn validate_committed_checkpoint_index_composite_child(
    arena: &PageArena,
    root: ArenaId,
) -> Result<CommittedCheckpointIndexCompositeDescriptor, CommittedCheckpointIndexError> {
    let summary = sequence_node::<CheckpointIndexSpec>(arena, root)?.0;
    Ok(CommittedCheckpointIndexCompositeDescriptor { root, summary })
}

#[cfg(feature = "exact-parser")]
fn revalidate_composite_query_root(
    arena: &PageArena,
    descriptor: CommittedCheckpointIndexCompositeDescriptor,
) -> Result<(ArenaId, ArenaScopedId), CommittedCheckpointIndexError> {
    let current = validate_committed_checkpoint_index_composite_child(arena, descriptor.root)?;
    if current != descriptor {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "parent checkpoint descriptor no longer matches its committed child",
        ));
    }
    let scoped = arena.scoped_query_id(descriptor.root)?;
    Ok((descriptor.root, scoped))
}

/// Internal read bridge for the restart-authoritative parent. The descriptor
/// was minted by that parent's child-edge decoder; this function revalidates
/// the complete index before selecting a recipe and never returns its root.
#[cfg(feature = "exact-parser")]
pub(crate) fn locate_parent_bound_donor_checkpoint_at_or_before_cut(
    mint: crate::storage_only_composite_document::RestartCheckpointQueryMint<'_>,
    source_cut: u64,
) -> Result<Option<LocatedDonorCheckpointRecipe>, CommittedCheckpointIndexError> {
    let (arena, descriptor) = mint.into_query_parts();
    let (root, scoped_root) = revalidate_composite_query_root(arena, descriptor)?;
    locate_donor_checkpoint_for_root(arena, root, scoped_root, source_cut)
}

/// Rechecks a previously selected parent-bound recipe against the current
/// child tree immediately before parser resume. No standalone index owner or
/// query root is constructed in this path.
#[cfg(feature = "exact-parser")]
pub(crate) fn revalidate_parent_bound_donor_checkpoint(
    mint: crate::storage_only_composite_document::RestartCheckpointQueryMint<'_>,
    recipe: &LocatedDonorCheckpointRecipe,
) -> Result<(), CommittedCheckpointIndexError> {
    let (arena, descriptor) = mint.into_query_parts();
    let (root, scoped_root) = revalidate_composite_query_root(arena, descriptor)?;
    validate_donor_checkpoint_for_root(arena, root, scoped_root, recipe).map(|_| ())
}

/// Converts a located donor recipe into a restart anchor only through the
/// private mint produced by the fully validated composite parent. This is the
/// sole constructor: a standalone checkpoint-index query cannot manufacture a
/// parent-bound anchor even when it locates identical persisted bytes.
#[cfg(feature = "exact-parser")]
pub(crate) fn bind_parent_selected_restart_anchor(
    mint: crate::storage_only_composite_document::RestartAnchorMint<'_>,
    recipe: LocatedDonorCheckpointRecipe,
) -> Result<ParentSelectedRestartAnchor, CommittedCheckpointIndexError> {
    let (arena, descriptor, parent_root) = mint.into_anchor_parts();
    arena.local_id(parent_root)?;
    let (root, scoped_root) = revalidate_composite_query_root(arena, descriptor)?;
    validate_donor_checkpoint_for_root(arena, root, scoped_root, &recipe)?;
    if parent_root.arena() != scoped_root.arena() {
        return Err(CommittedCheckpointIndexError::Invalid(
            "restart anchor parent and checkpoint index belong to different arenas",
        ));
    }
    Ok(ParentSelectedRestartAnchor {
        parent_root,
        recipe,
    })
}

/// Authenticates one parent-selected donor as a normalization checkpoint
/// without constructing a standalone index owner or exposing persisted role
/// scalars. The returned one-shot proof intentionally drops the arena borrow;
/// the suffix-splice job must revalidate its copied binding against the exact
/// parent-retained child before use.
#[cfg(feature = "exact-parser")]
pub(crate) fn authenticate_parent_bound_normalization_checkpoint(
    mint: crate::storage_only_composite_document::RestartCheckpointQueryMint<'_>,
    recipe: &LocatedDonorCheckpointRecipe,
) -> Result<ParentBoundNormalizationCheckpoint, CommittedCheckpointIndexError> {
    let (arena, descriptor) = mint.into_query_parts();
    let (root, scoped_root) = revalidate_composite_query_root(arena, descriptor)?;
    let authority = validate_donor_checkpoint_for_root(arena, root, scoped_root, recipe)?;
    let DonorPartitionRole::Normalization { group, outcome } = recipe.role else {
        return Err(CommittedCheckpointIndexError::Invalid(
            "parent-selected donor is not a normalization checkpoint",
        ));
    };
    let bounds = CommittedNormalizationGroupBounds {
        start: authority.partition.prefix,
        interval: authority.partition.interval,
        end: authority
            .partition
            .prefix
            .checked_add(authority.partition.interval)?,
    };
    Ok(ParentBoundNormalizationCheckpoint {
        binding: CommittedNormalizationGroupBinding {
            index_root: recipe.authority.index_root,
            partition_manifest: authority.donor_partition.manifest,
            group,
            outcome,
            bounds,
        },
        checkpoint_cut: recipe.checkpoint_cut,
    })
}

/// Revalidates one parent-selected checkpoint and derives the only role token
/// accepted by the current-green restart adapter. A direct checkpoint needs
/// no rewrite. A normalization checkpoint binds the persisted writer-owned
/// Paragraph `BlockId`, exact five-axis cut, and typed finalized Setext facts.
/// The joined source/green mint later decides from A=P versus A=P-1 whether
/// this authority must actually invert the finalized terminal frame.
#[cfg(feature = "exact-parser")]
pub(crate) fn authenticate_parent_bound_current_restart_role(
    mint: crate::storage_only_composite_document::RestartCheckpointQueryMint<'_>,
    recipe: &LocatedDonorCheckpointRecipe,
) -> Result<ParentBoundCurrentRestartRole, CommittedCheckpointIndexError> {
    let (arena, descriptor) = mint.into_query_parts();
    let (root, scoped_root) = revalidate_composite_query_root(arena, descriptor)?;
    let authority = validate_donor_checkpoint_for_root(arena, root, scoped_root, recipe)?;
    let DonorPartitionRole::Normalization { group, outcome } = recipe.role else {
        return Ok(ParentBoundCurrentRestartRole::Direct);
    };
    let bounds = CommittedNormalizationGroupBounds {
        start: authority.partition.prefix,
        interval: authority.partition.interval,
        end: authority
            .partition
            .prefix
            .checked_add(authority.partition.interval)?,
    };
    let binding = CommittedNormalizationGroupBinding {
        index_root: recipe.authority.index_root,
        partition_manifest: authority.donor_partition.manifest,
        group,
        outcome,
        bounds,
    };
    let final_facts = match outcome {
        StorageOnlyNormalizationOutcome::SetextHeading { level } => {
            GreenHeadingOpenFacts::setext(level).map_err(|_| {
                CommittedCheckpointIndexError::Corrupt(
                    "persisted Setext outcome has invalid final heading facts",
                )
            })?
        }
    };
    Ok(ParentBoundCurrentRestartRole::Normalization(
        ParentBoundCurrentRestartNormalization {
            checkpoint_cut: recipe.checkpoint_cut,
            target: BlockId(group),
            final_facts,
            _binding: binding,
        },
    ))
}

impl StorageOnlyCheckpointIndexBuildManifest {
    pub(crate) const fn build_id(&self) -> ArenaBuildId {
        self.build
    }

    pub(crate) const fn receipt(&self) -> CommittedCheckpointIndexBuildReceipt {
        self.receipt
    }

    pub(crate) fn validate_composite_child(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<ArenaId, CommittedCheckpointIndexError> {
        Ok(self.composite_descriptor(session)?.root)
    }

    pub(crate) fn composite_descriptor(
        &self,
        session: &ArenaBuildSession<'_>,
    ) -> Result<CommittedCheckpointIndexCompositeDescriptor, CommittedCheckpointIndexError> {
        if session.id() != self.build {
            return Err(CommittedCheckpointIndexError::Invalid(
                "checkpoint index and arena session build generations differ",
            ));
        }
        let root = session.owner_id(&self.owner)?;
        validate_committed_checkpoint_index_composite_child(session.arena(), root)
    }

    pub(crate) fn into_composite_parts(
        self,
    ) -> (ArenaBuildOwner, CommittedCheckpointIndexBuildReceipt) {
        (self.owner, self.receipt)
    }

    #[cfg(test)]
    pub(crate) fn from_unchecked_test_owner(build: ArenaBuildId, owner: ArenaBuildOwner) -> Self {
        Self {
            build,
            owner,
            receipt: CommittedCheckpointIndexBuildReceipt::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn commit(
        self,
        session: ArenaBuildSession<'_>,
    ) -> Result<
        (
            StorageOnlyCommittedCheckpointIndex,
            CommittedCheckpointIndexBuildReceipt,
        ),
        CommittedCheckpointIndexError,
    > {
        if session.id() != self.build {
            return Err(CommittedCheckpointIndexError::Invalid(
                "checkpoint index and arena session build generations differ",
            ));
        }
        let owner = session.commit(self.owner)?;
        Ok((
            StorageOnlyCommittedCheckpointIndex { owner: Some(owner) },
            self.receipt,
        ))
    }
}

/// The committed owner is queryable storage only. It has no parser-resume API.
#[derive(Debug)]
pub(crate) struct StorageOnlyCommittedCheckpointIndex {
    owner: Option<OwnedArenaRef>,
}

impl StorageOnlyCommittedCheckpointIndex {
    fn scoped_root_id(&self) -> ArenaScopedId {
        self.owner
            .as_ref()
            .expect("live storage-only index owns its root")
            .scoped_id()
    }

    fn checked_root_id(&self, arena: &PageArena) -> Result<ArenaId, CommittedCheckpointIndexError> {
        arena
            .local_id(self.scoped_root_id())
            .map_err(CommittedCheckpointIndexError::Arena)
    }

    fn summary(
        &self,
        arena: &PageArena,
    ) -> Result<CheckpointIndexSummary, CommittedCheckpointIndexError> {
        Ok(sequence_node::<CheckpointIndexSpec>(arena, self.checked_root_id(arena)?)?.0)
    }

    fn locate_source_byte(
        &self,
        arena: &PageArena,
        source_byte: u64,
    ) -> Result<LocatedCheckpointPartition, CommittedCheckpointIndexError> {
        locate_outer_partition(arena, self.checked_root_id(arena)?, source_byte)
    }

    fn locate_group_sample(
        &self,
        arena: &PageArena,
        group: &LocatedNormalizationGroup,
        relative_source_byte: u64,
    ) -> Result<LocatedNormalizationSample, CommittedCheckpointIndexError> {
        if group.index_root != self.checked_root_id(arena)? {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "normalization group belongs to another checkpoint index",
            ));
        }
        let manifest = decode_normalization_manifest(arena, group.manifest)?;
        if manifest.group != group.group
            || manifest.measure != group.interval
            || manifest.samples != group.samples
        {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "normalization partition and manifest disagree",
            ));
        }
        locate_sample(arena, manifest.sample_root, relative_source_byte)
    }

    /// Selects only within the current restart-bearing donor run.
    ///
    /// Legacy storage-only partitions are deliberate restart barriers: this
    /// lookup never walks across them looking for stale donor state. A cut in
    /// a donor partition needs at most one fallback to the immediately prior
    /// partition when the current partition's first sample is not complete.
    /// The prior donor partition's full measure necessarily ends at a sample,
    /// so the outer tree is descended at most twice.
    #[cfg(all(feature = "exact-parser", not(test)))]
    pub(in crate::committed_checkpoint_index) fn locate_donor_checkpoint_at_or_before_cut(
        &self,
        arena: &PageArena,
        source_cut: u64,
    ) -> Result<Option<LocatedDonorCheckpointRecipe>, CommittedCheckpointIndexError> {
        self.locate_donor_checkpoint_for_test_or_internal_use(arena, source_cut)
    }

    /// Test-only compatibility for the pre-parent mechanism fixtures. The
    /// non-test crate surface restricts standalone lookup to this module and
    /// exposes restart selection only through the v2 composite parent.
    #[cfg(all(feature = "exact-parser", test))]
    pub(crate) fn locate_donor_checkpoint_at_or_before_cut(
        &self,
        arena: &PageArena,
        source_cut: u64,
    ) -> Result<Option<LocatedDonorCheckpointRecipe>, CommittedCheckpointIndexError> {
        self.locate_donor_checkpoint_for_test_or_internal_use(arena, source_cut)
    }

    #[cfg(feature = "exact-parser")]
    fn locate_donor_checkpoint_for_test_or_internal_use(
        &self,
        arena: &PageArena,
        source_cut: u64,
    ) -> Result<Option<LocatedDonorCheckpointRecipe>, CommittedCheckpointIndexError> {
        let index_root = self.checked_root_id(arena)?;
        locate_donor_checkpoint_for_root(arena, index_root, self.scoped_root_id(), source_cut)
    }

    #[cfg(feature = "exact-parser")]
    fn reconstruct_located_donor_checkpoint(
        &self,
        arena: &PageArena,
        partition: LocatedCheckpointPartition,
        donor_partition: LocatedDonorPartition,
        manifest: DecodedDonorPartitionManifest,
        located: LocatedOpaqueDonorSample,
    ) -> Result<LocatedDonorCheckpointRecipe, CommittedCheckpointIndexError> {
        reconstruct_located_donor_checkpoint_for_root(
            arena,
            self.scoped_root_id(),
            partition,
            donor_partition,
            manifest,
            located,
        )
    }

    #[cfg(feature = "exact-parser")]
    fn validate_donor_checkpoint_authority(
        &self,
        arena: &PageArena,
        recipe: &LocatedDonorCheckpointRecipe,
    ) -> Result<RevalidatedDonorCheckpointAuthority, CommittedCheckpointIndexError> {
        let index_root = self.checked_root_id(arena)?;
        validate_donor_checkpoint_for_root(arena, index_root, self.scoped_root_id(), recipe)
    }

    #[cfg(feature = "exact-parser")]
    fn validate_normalization_completion_capability(
        &self,
        arena: &PageArena,
        capability: &CommittedNormalizationOutcomeCapability<'_>,
    ) -> Result<RevalidatedNormalizationCompletionCapability, CommittedCheckpointIndexError> {
        let index_root = self.checked_root_id(arena)?;
        let binding = capability.binding;
        let recipe_authority = &capability.recipe.authority;
        if binding.index_root != self.scoped_root_id() || binding.index_root.local() != index_root {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "normalization completion capability belongs to another checkpoint index",
            ));
        }
        if recipe_authority.index_root != binding.index_root
            || recipe_authority.partition_manifest != binding.partition_manifest
            || recipe_authority.partition_prefix != binding.bounds.start
            || capability.recipe.role
                != (DonorPartitionRole::Normalization {
                    group: binding.group,
                    outcome: binding.outcome,
                })
        {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "normalization completion capability binding mismatch",
            ));
        }

        let manifest = decode_donor_partition_manifest(arena, binding.partition_manifest)?;
        if manifest.role
            != (DonorPartitionRole::Normalization {
                group: binding.group,
                outcome: binding.outcome,
            })
            || manifest.measure != binding.bounds.interval
            || binding.bounds.start.checked_add(manifest.measure)? != binding.bounds.end
        {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "normalization completion manifest and capability disagree",
            ));
        }
        Ok(RevalidatedNormalizationCompletionCapability {
            index_root,
            binding,
            partition_ordinal: recipe_authority.partition_ordinal,
            manifest,
        })
    }

    #[cfg(feature = "exact-parser")]
    fn locate_normalization_completion_checkpoint(
        &self,
        arena: &PageArena,
        capability: &CommittedNormalizationOutcomeCapability<'_>,
    ) -> Result<CommittedNormalizationCompletionCheckpoint, CommittedCheckpointIndexError> {
        let validated = self.validate_normalization_completion_capability(arena, capability)?;
        let index_root = validated.index_root;
        let binding = validated.binding;
        let manifest = validated.manifest;

        let final_sample_ordinal =
            manifest
                .samples
                .checked_sub(1)
                .ok_or(CommittedCheckpointIndexError::Corrupt(
                    "normalization completion manifest has no samples",
                ))?;
        let located = locate_donor_sample_by_ordinal_with_receipt(
            arena,
            manifest.sample_root,
            final_sample_ordinal,
        )?;
        let relative_end = located.sample.prefix.checked_add(located.sample.interval)?;
        if relative_end != manifest.measure {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "normalization completion sample does not end at the group frontier",
            ));
        }
        let absolute_end = binding.bounds.start.checked_add(relative_end)?;
        if absolute_end != binding.bounds.end {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "normalization completion checkpoint has the wrong absolute end",
            ));
        }

        let donor_partition = LocatedDonorPartition {
            index_root,
            manifest: binding.partition_manifest,
            interval: manifest.measure,
            samples: manifest.samples,
        };
        let partition = LocatedCheckpointPartition {
            ordinal: validated.partition_ordinal,
            prefix: binding.bounds.start,
            interval: manifest.measure,
            kind: LocatedCheckpointPartitionKind::Donor(donor_partition),
        };
        let donor = self.reconstruct_located_donor_checkpoint(
            arena,
            partition,
            donor_partition,
            manifest,
            located.sample,
        )?;
        if donor.checkpoint_cut != binding.bounds.end
            || donor.authority.sample_ordinal != final_sample_ordinal
            || donor.authority.partition_manifest != binding.partition_manifest
            || donor.role
                != (DonorPartitionRole::Normalization {
                    group: binding.group,
                    outcome: binding.outcome,
                })
        {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "normalization completion reconstruction lost its authority binding",
            ));
        }
        let donor_receipt = donor.receipt;
        let receipt = NormalizationCompletionLookupReceipt {
            group_samples: manifest.samples,
            sample_tree_height: located.tree_height,
            sample_tree_nodes_visited: located.nodes_visited,
            sample_leaf_temporary_bytes: located.maximum_temporary_bytes,
            path_nodes_visited: donor_receipt.path_nodes_visited,
            reconstructed_opaque_path_bytes: donor_receipt.reconstructed_opaque_path_bytes,
            donor_typed_recipe_bytes: donor_receipt.donor_typed_recipe_bytes,
            maximum_temporary_bytes: located
                .maximum_temporary_bytes
                .max(donor_receipt.maximum_temporary_bytes),
            retained_source_bytes: donor_receipt.retained_source_bytes,
        };
        Ok(CommittedNormalizationCompletionCheckpoint {
            binding,
            final_sample_ordinal,
            donor,
            receipt,
        })
    }

    pub(crate) fn release_later(
        mut self,
        arena: &mut PageArena,
    ) -> Result<(), CommittedCheckpointIndexError> {
        let owner = self
            .owner
            .take()
            .expect("live storage-only index owns its root");
        arena
            .release_later(owner)
            .map_err(|failure| failure.error.into())
    }
}

#[cfg(feature = "exact-parser")]
fn locate_donor_checkpoint_for_root(
    arena: &PageArena,
    index_root: ArenaId,
    scoped_root: ArenaScopedId,
    source_cut: u64,
) -> Result<Option<LocatedDonorCheckpointRecipe>, CommittedCheckpointIndexError> {
    let total = sequence_node::<CheckpointIndexSpec>(arena, index_root)?
        .0
        .measure
        .source_bytes;
    if source_cut > total {
        return Err(CommittedCheckpointIndexError::SourceOutOfBounds);
    }
    let mut boundary_cut = source_cut;
    let mut outer_descents = 0_u8;
    while boundary_cut != 0 {
        outer_descents =
            outer_descents
                .checked_add(1)
                .ok_or(CommittedCheckpointIndexError::Overflow(
                    "donor lookup outer descents",
                ))?;
        if outer_descents > 2 {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "donor predecessor crossed more than one partition boundary",
            ));
        }
        let partition = locate_outer_partition(arena, index_root, boundary_cut - 1)?;
        let LocatedCheckpointPartitionKind::Donor(donor_partition) = partition.kind else {
            return Ok(None);
        };
        if donor_partition.index_root != index_root
            || donor_partition.interval != partition.interval
        {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "donor partition belongs to another checkpoint index",
            ));
        }
        let manifest = decode_donor_partition_manifest(arena, donor_partition.manifest)?;
        if manifest.measure != donor_partition.interval
            || manifest.samples != donor_partition.samples
        {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "donor partition and manifest disagree",
            ));
        }
        let relative_cut = boundary_cut
            .checked_sub(partition.prefix.source_bytes)
            .ok_or(CommittedCheckpointIndexError::Corrupt(
                "donor partition prefix exceeds selected source cut",
            ))?;
        let Some(located) =
            locate_donor_sample_predecessor(arena, manifest.sample_root, relative_cut)?
        else {
            boundary_cut = partition.prefix.source_bytes;
            continue;
        };
        return reconstruct_located_donor_checkpoint_for_root(
            arena,
            scoped_root,
            partition,
            donor_partition,
            manifest,
            located,
        )
        .map(Some);
    }
    Ok(None)
}

#[cfg(feature = "exact-parser")]
fn reconstruct_located_donor_checkpoint_for_root(
    arena: &PageArena,
    scoped_root: ArenaScopedId,
    partition: LocatedCheckpointPartition,
    donor_partition: LocatedDonorPartition,
    manifest: DecodedDonorPartitionManifest,
    located: LocatedOpaqueDonorSample,
) -> Result<LocatedDonorCheckpointRecipe, CommittedCheckpointIndexError> {
    // Reject a malformed header before walking or allocating from its
    // separately stored path-depth topology.
    let header = IndexedDonorCheckpointRecipe::validate_header(&located.header).map_err(|_| {
        CommittedCheckpointIndexError::Corrupt("donor rejected persisted header schema")
    })?;
    let reconstructed = reconstruct_donor_path(arena, located.path_terminal)?;
    let raw_scratch = reconstructed.scratch_storage_bytes;
    let path_nodes_visited = reconstructed.nodes_visited;
    let reconstructed_opaque_path_bytes = reconstructed
        .frames
        .len()
        .checked_mul(DONOR_FRAME_BYTES)
        .ok_or(CommittedCheckpointIndexError::Overflow(
            "reconstructed donor path bytes",
        ))?;
    let donor = IndexedDonorCheckpointRecipe::from_validated_storage(header, reconstructed.frames)
        .map_err(|_| {
            CommittedCheckpointIndexError::Corrupt("donor rejected persisted opaque frame schema")
        })?;
    let donor_typed_recipe_bytes = donor.scratch_storage_bytes();
    let prefix = partition.prefix.checked_add(located.prefix)?;
    let checkpoint_cut = prefix.checked_add(located.interval)?;
    let authority = DonorCheckpointAuthorityBinding {
        index_root: scoped_root,
        partition_ordinal: partition.ordinal,
        partition_prefix: partition.prefix,
        partition_manifest: donor_partition.manifest,
        sample_ordinal: located.ordinal,
        sample_header: located.header,
        sample_path_terminal: located.path_terminal,
    };
    let receipt = DonorCheckpointLookupReceipt {
        path_nodes_visited,
        reconstructed_opaque_path_bytes,
        donor_typed_recipe_bytes,
        maximum_temporary_bytes: raw_scratch.checked_add(donor_typed_recipe_bytes).ok_or(
            CommittedCheckpointIndexError::Overflow("donor lookup temporary bytes"),
        )?,
        retained_source_bytes: 0,
    };
    Ok(LocatedDonorCheckpointRecipe {
        ordinal: located.ordinal,
        prefix,
        checkpoint_cut,
        interval: located.interval,
        role: manifest.role,
        authority,
        donor,
        receipt,
    })
}

#[cfg(feature = "exact-parser")]
fn validate_donor_checkpoint_for_root(
    arena: &PageArena,
    index_root: ArenaId,
    scoped_root: ArenaScopedId,
    recipe: &LocatedDonorCheckpointRecipe,
) -> Result<RevalidatedDonorCheckpointAuthority, CommittedCheckpointIndexError> {
    let authority = &recipe.authority;
    if authority.index_root != scoped_root || authority.index_root.local() != index_root {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor role authority belongs to another checkpoint index",
        ));
    }

    let partition =
        locate_outer_partition(arena, index_root, authority.partition_prefix.source_bytes)?;
    let LocatedCheckpointPartitionKind::Donor(donor_partition) = partition.kind else {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor role authority no longer selects a donor partition",
        ));
    };
    if partition.ordinal != authority.partition_ordinal
        || partition.prefix != authority.partition_prefix
        || donor_partition.index_root != index_root
        || donor_partition.manifest != authority.partition_manifest
        || donor_partition.interval != partition.interval
    {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor role authority partition binding mismatch",
        ));
    }

    let manifest = decode_donor_partition_manifest(arena, donor_partition.manifest)?;
    if manifest.measure != donor_partition.interval || manifest.samples != donor_partition.samples {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor role authority manifest binding mismatch",
        ));
    }
    if manifest.role != recipe.role {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor role authority group or outcome mismatch",
        ));
    }

    let sample =
        locate_donor_sample_by_ordinal(arena, manifest.sample_root, authority.sample_ordinal)?;
    if sample.ordinal != authority.sample_ordinal
        || sample.header != authority.sample_header
        || sample.path_terminal != authority.sample_path_terminal
        || recipe.ordinal != authority.sample_ordinal
        || recipe.interval != sample.interval
    {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor role authority sample binding mismatch",
        ));
    }
    let prefix = partition.prefix.checked_add(sample.prefix)?;
    let checkpoint_cut = prefix.checked_add(sample.interval)?;
    if recipe.prefix != prefix || recipe.checkpoint_cut != checkpoint_cut {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor role authority selected-cut binding mismatch",
        ));
    }
    Ok(RevalidatedDonorCheckpointAuthority {
        partition,
        donor_partition,
        manifest,
        sample,
    })
}

#[derive(Debug, Default)]
pub(crate) struct StorageOnlyCheckpointIndexBuilder {
    partitions: Vec<StorageOnlyCheckpointPartition>,
}

impl StorageOnlyCheckpointIndexBuilder {
    pub(crate) fn push(
        &mut self,
        partition: StorageOnlyCheckpointPartition,
    ) -> Result<(), CommittedCheckpointIndexError> {
        #[cfg(feature = "exact-parser")]
        let partition = match partition {
            StorageOnlyCheckpointPartition::Donor {
                group: None,
                mut samples,
            } => {
                if let Some(StorageOnlyCheckpointPartition::Donor {
                    group: None,
                    samples: previous,
                }) = self.partitions.last_mut()
                {
                    previous.try_reserve(samples.len()).map_err(|_| {
                        CommittedCheckpointIndexError::Allocation("coalesced direct donor samples")
                    })?;
                    previous.append(&mut samples);
                    return Ok(());
                }
                StorageOnlyCheckpointPartition::Donor {
                    group: None,
                    samples,
                }
            }
            partition => partition,
        };
        self.partitions
            .try_reserve(1)
            .map_err(|_| CommittedCheckpointIndexError::Allocation("partition drafts"))?;
        self.partitions.push(partition);
        Ok(())
    }

    /// Synchronous orchestration for the storage milestone only.
    ///
    /// Every allocation is still journal-owned and every persistent-sequence
    /// push is allocation-granular, but production scheduling must wrap this
    /// state in a pollable job before it is integrated with the parser.
    #[allow(clippy::too_many_lines)] // One pass preserves partition order and tail finality.
    pub(crate) fn build_in_session(
        self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<StorageOnlyCheckpointIndexBuildManifest, CommittedCheckpointIndexError> {
        if self.partitions.is_empty() {
            return Err(CommittedCheckpointIndexError::Invalid(
                "checkpoint index requires at least one nonempty partition",
            ));
        }
        let mut receipt = CommittedCheckpointIndexBuildReceipt::default();
        #[cfg(feature = "exact-parser")]
        initialize_donor_builder_receipt(
            &mut receipt,
            self.partitions.capacity(),
            &self.partitions,
        )?;
        let mut sequence = ResumableStreamingSequenceBuilder::<CheckpointIndexSpec>::try_new(
            &mut receipt.sequence,
        )?;
        let mut page = OuterLeafEncoder::new()?;
        #[cfg(feature = "exact-parser")]
        let mut donor_path_cache = DonorPathCache::default();
        let mut terminal_tail_seen = false;

        for partition in self.partitions {
            if terminal_tail_seen {
                return Err(CommittedCheckpointIndexError::Invalid(
                    "terminal tail is not the final checkpoint-index partition",
                ));
            }
            match partition {
                StorageOnlyCheckpointPartition::Direct { interval } => {
                    require_nonempty_interval(interval)?;
                    if !page.can_fit(false) {
                        flush_outer_leaf(session, &mut sequence, &mut page, &mut receipt)?;
                    }
                    page.push_direct(interval)?;
                }
                StorageOnlyCheckpointPartition::NormalizationGroup {
                    group,
                    outcome,
                    samples,
                } => {
                    if group == 0 {
                        return Err(CommittedCheckpointIndexError::Invalid(
                            "normalization group identity is zero",
                        ));
                    }
                    let group_summary = summarize_samples(&samples)?;
                    if !page.can_fit(true) {
                        flush_outer_leaf(session, &mut sequence, &mut page, &mut receipt)?;
                    }
                    let sample_root = build_sample_sequence(session, samples, &mut receipt)?;
                    let sample_root_id = session.owner_id(&sample_root)?;
                    let manifest_payload = encode_normalization_manifest(
                        group,
                        outcome,
                        group_summary.measure,
                        group_summary.samples,
                    )?;
                    let (manifest, allocation) =
                        session.allocate(&manifest_payload, &[sample_root_id])?;
                    observe_allocation(&mut receipt, allocation);
                    receipt.normalization_manifests =
                        receipt.normalization_manifests.checked_add(1).ok_or(
                            CommittedCheckpointIndexError::Overflow("normalization manifest count"),
                        )?;
                    session.release(sample_root)?;
                    page.push_normalization(
                        group_summary.measure,
                        group_summary.samples,
                        manifest,
                    )?;
                }
                #[cfg(feature = "exact-parser")]
                StorageOnlyCheckpointPartition::Donor { group, samples } => {
                    validate_donor_partition_shape(group, &samples)?;
                    if !page.can_fit(true) {
                        flush_outer_leaf(session, &mut sequence, &mut page, &mut receipt)?;
                    }
                    let (summary, manifest) = build_donor_partition(
                        session,
                        group,
                        samples,
                        &mut donor_path_cache,
                        &mut receipt,
                    )?;
                    page.push_donor(summary.measure, summary.samples, manifest)?;
                }
                StorageOnlyCheckpointPartition::TerminalTail { interval } => {
                    require_terminal_tail_interval(interval)?;
                    if !page.can_fit(false) {
                        flush_outer_leaf(session, &mut sequence, &mut page, &mut receipt)?;
                    }
                    page.push_terminal_tail(interval)?;
                    terminal_tail_seen = true;
                }
            }
        }
        #[cfg(feature = "exact-parser")]
        donor_path_cache.release_terminal_if_present(session)?;
        flush_outer_leaf(session, &mut sequence, &mut page, &mut receipt)?;
        let root = finish_sequence(session, &mut sequence, &mut receipt.sequence)?;
        Ok(StorageOnlyCheckpointIndexBuildManifest {
            build: session.id(),
            owner: root,
            receipt,
        })
    }

    #[cfg(test)]
    pub(crate) fn commit(
        self,
        arena: &mut PageArena,
    ) -> Result<
        (
            StorageOnlyCommittedCheckpointIndex,
            CommittedCheckpointIndexBuildReceipt,
        ),
        CommittedCheckpointIndexError,
    > {
        let ticket = arena.begin_build()?;
        let session = arena
            .resume_build(ticket)
            .map_err(|failure| failure.error)?;
        let mut session = session;
        self.build_in_session(&mut session)?.commit(session)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecodedPartitionKind {
    Direct,
    TerminalTail,
    Normalization {
        child_ordinal: u16,
    },
    #[cfg(feature = "exact-parser")]
    Donor {
        child_ordinal: u16,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedPartitionRecord {
    kind: DecodedPartitionKind,
    interval: RelativeCheckpointMeasure,
    samples: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocatedCheckpointPartition {
    ordinal: u64,
    prefix: RelativeCheckpointMeasure,
    interval: RelativeCheckpointMeasure,
    kind: LocatedCheckpointPartitionKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocatedCheckpointPartitionKind {
    Direct,
    TerminalTail,
    Normalization(LocatedNormalizationGroup),
    #[cfg(feature = "exact-parser")]
    Donor(LocatedDonorPartition),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocatedNormalizationGroup {
    index_root: ArenaId,
    group: u64,
    manifest: ArenaId,
    interval: RelativeCheckpointMeasure,
    samples: u64,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocatedDonorPartition {
    index_root: ArenaId,
    manifest: ArenaId,
    interval: RelativeCheckpointMeasure,
    samples: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocatedNormalizationSample {
    ordinal: u64,
    prefix: RelativeCheckpointMeasure,
    interval: RelativeCheckpointMeasure,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DonorCheckpointAuthorityBinding {
    index_root: ArenaScopedId,
    partition_ordinal: u64,
    partition_prefix: RelativeCheckpointMeasure,
    partition_manifest: ArenaId,
    sample_ordinal: u64,
    sample_header: OpaqueDonorHeader,
    sample_path_terminal: ArenaId,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RevalidatedDonorCheckpointAuthority {
    partition: LocatedCheckpointPartition,
    donor_partition: LocatedDonorPartition,
    manifest: DecodedDonorPartitionManifest,
    sample: LocatedOpaqueDonorSample,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RevalidatedNormalizationCompletionCapability {
    index_root: ArenaId,
    binding: CommittedNormalizationGroupBinding,
    partition_ordinal: u64,
    manifest: DecodedDonorPartitionManifest,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct LocatedDonorCheckpointRecipe {
    ordinal: u64,
    prefix: RelativeCheckpointMeasure,
    checkpoint_cut: RelativeCheckpointMeasure,
    interval: RelativeCheckpointMeasure,
    role: DonorPartitionRole,
    authority: DonorCheckpointAuthorityBinding,
    donor: IndexedDonorCheckpointRecipe,
    receipt: DonorCheckpointLookupReceipt,
}

/// Linear authority for the exact parent-selected checkpoint used to restart
/// one candidate. The persisted cut and parent binding remain private; later
/// suffix sampling can only be seeded after the installed writer reproduces
/// the full five-axis restart cut in the same arena.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentSelectedRestartAnchor {
    parent_root: ArenaScopedId,
    recipe: LocatedDonorCheckpointRecipe,
}

/// Restart anchor after its one legal suffix-sample origin has been minted.
/// This distinct type makes a second origin unrepresentable while preserving
/// the exact parent/sample authority for later successor selection.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentSelectedSeededRestartAnchor {
    parent_root: ArenaScopedId,
    recipe: LocatedDonorCheckpointRecipe,
}

/// One-shot origin for the candidate's first sparse sample after a retained
/// restart. Unlike a document-origin sample chain, this starts at the
/// authenticated retained cut rather than at zero.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentSelectedSuffixSampleOrigin {
    epoch: LiveCandidateEpoch,
    authority: DonorCheckpointAuthorityBinding,
    restart_cut: RelativeCheckpointMeasure,
}

/// Linear cursor for subsequent candidate-observed sparse samples. It carries
/// the retained parent's authority and cumulative five-axis cut without
/// exposing a caller-constructible scalar restart origin.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentSelectedSuffixSampleCursor {
    epoch: LiveCandidateEpoch,
    authority: DonorCheckpointAuthorityBinding,
    sample_ordinal: u64,
    cumulative_cut: RelativeCheckpointMeasure,
}

/// Transactional predecessor for one freshly observed convergence probe.
/// The candidate owns the advanced cursor while the probe owns this rollback
/// half; only an exact rejection join can restore the predecessor. Successful
/// convergence consumes the probe without exposing either authority.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentSelectedSuffixSampleRollback {
    epoch: LiveCandidateEpoch,
    authority: DonorCheckpointAuthorityBinding,
    rejected_sample_ordinal: u64,
    rejected_cumulative_cut: RelativeCheckpointMeasure,
    prior: ParentSelectedSuffixSamplePrior,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) enum ParentSelectedSuffixSamplePrior {
    AwaitingFirst(ParentSelectedSuffixSampleOrigin),
    Continuing(ParentSelectedSuffixSampleCursor),
}

/// The next persisted checkpoint after the authenticated restart point, still
/// bound to the exact retained composite parent. This is deliberately distinct
/// from `ParentSelectedSuffixSampleCursor`: that cursor counts newly observed
/// candidate samples, while this value advances through the old committed
/// partition. Equal numeric ordinals across those domains carry no authority.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentBoundDonorSuccessor {
    parent_root: ArenaScopedId,
    restart_authority: DonorCheckpointAuthorityBinding,
    recipe: LocatedDonorCheckpointRecipe,
}

/// One old convergence checkpoint joined back to the exact retained restart
/// anchor that began its successor chain. The source mapper may split this
/// value only through its private mint; callers never author R/C coordinates.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentBoundSourceConvergence {
    restart_cut: RelativeCheckpointMeasure,
    old_convergence: ParentBoundDonorSuccessor,
}

/// Why the first partition-scoped convergence search cannot advance farther.
/// Partition boundaries are semantic: Direct and normalization partitions
/// cannot be crossed until the later splice owns a typed transition for both.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParentBoundDonorSuccessorBoundaryKind {
    RestartBarrier,
    TerminalSemanticTail,
    SourceEof,
}

/// One-shot proof that the exact final checkpoint of one donor manifest is
/// immediately followed by another donor manifest under the same retained
/// composite parent. It authorizes selection only; semantic tail reuse still
/// requires source lineage, a matching live donor, and both storage splices.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentBoundDonorPartitionTransition {
    parent_root: ArenaScopedId,
    restart_authority: DonorCheckpointAuthorityBinding,
    final_authority: DonorCheckpointAuthorityBinding,
    next_partition_ordinal: u64,
    next_partition_prefix: RelativeCheckpointMeasure,
    next_partition_manifest: ArenaId,
    next_partition_interval: RelativeCheckpointMeasure,
}

/// Authenticated normal end of one old-checkpoint partition. This is not an
/// error and never silently skips into a different semantic partition.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentBoundDonorSuccessorBoundary {
    parent_root: ArenaScopedId,
    restart_authority: DonorCheckpointAuthorityBinding,
    final_authority: DonorCheckpointAuthorityBinding,
    kind: ParentBoundDonorSuccessorBoundaryKind,
}

/// One bounded old-index successor step. Callers either receive exactly the
/// next checkpoint in the same manifest or a typed partition boundary.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) enum ParentBoundDonorSuccessorStep {
    Checkpoint(ParentBoundDonorSuccessor),
    NextPartition(ParentBoundDonorPartitionTransition),
    PartitionEnd(ParentBoundDonorSuccessorBoundary),
}

/// Revalidated role of one located donor checkpoint. Neither variant can be
/// constructed from persisted scalar fields alone; both borrow the live index,
/// arena, and exact selected recipe that jointly established the authority.
#[cfg(feature = "exact-parser")]
pub(crate) enum CommittedDonorCheckpointRole<'authority> {
    DirectRun(CommittedDirectDonorCapability<'authority>),
    Normalization(CommittedNormalizationOutcomeCapability<'authority>),
}

/// Explicit proof that the selected donor belongs to a non-normalization run.
#[cfg(feature = "exact-parser")]
pub(crate) struct CommittedDirectDonorCapability<'authority> {
    recipe: &'authority LocatedDonorCheckpointRecipe,
    _index: &'authority StorageOnlyCommittedCheckpointIndex,
    _arena: &'authority PageArena,
}

#[cfg(feature = "exact-parser")]
impl CommittedDirectDonorCapability<'_> {
    pub(crate) const fn checkpoint_cut(&self) -> RelativeCheckpointMeasure {
        self.recipe.checkpoint_cut
    }
}

/// Opaque authority to use one committed normalization result. Group and
/// outcome become observable only after the selected persisted sample and all
/// of its root/partition bindings have been revalidated.
#[cfg(feature = "exact-parser")]
pub(crate) struct CommittedNormalizationOutcomeCapability<'authority> {
    recipe: &'authority LocatedDonorCheckpointRecipe,
    _origin_index: &'authority StorageOnlyCommittedCheckpointIndex,
    _origin_arena: &'authority PageArena,
    binding: CommittedNormalizationGroupBinding,
}

/// Opaque normalization role proof minted only through a restart-authoritative
/// composite parent's query descriptor. It owns no index root or arena borrow,
/// exposes no group/outcome scalars, and becomes useful only when paired with a
/// second proof and revalidated against the retained child at splice admission.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentBoundNormalizationCheckpoint {
    binding: CommittedNormalizationGroupBinding,
    checkpoint_cut: RelativeCheckpointMeasure,
}

/// Revalidated role of one parent-selected restart checkpoint at the seam
/// where committed green is mapped into donor-facing restart state.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) enum ParentBoundCurrentRestartRole {
    Direct,
    Normalization(ParentBoundCurrentRestartNormalization),
}

/// Opaque authorization for exactly one persisted normalization group to
/// rewrite exactly one current-green frame into its donor-facing restart
/// shape. The persisted group identity is the writer-owned provisional
/// Paragraph `BlockId`; callers cannot construct this value from echoed
/// group, level, or cut scalars.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
pub(crate) struct ParentBoundCurrentRestartNormalization {
    checkpoint_cut: RelativeCheckpointMeasure,
    target: BlockId,
    final_facts: GreenHeadingOpenFacts,
    _binding: CommittedNormalizationGroupBinding,
}

#[cfg(feature = "exact-parser")]
impl ParentBoundCurrentRestartNormalization {
    /// Linear handoff to the single current-green normalization adapter. The
    /// adapter rechecks the full cut and exact deepest target frame before
    /// changing only its donor-facing kind.
    pub(crate) fn into_current_restart_path_parts(
        self,
    ) -> (RelativeCheckpointMeasure, BlockId, GreenHeadingOpenFacts) {
        (self.checkpoint_cut, self.target, self.final_facts)
    }
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CommittedNormalizationGroupBinding {
    index_root: ArenaScopedId,
    partition_manifest: ArenaId,
    group: u64,
    outcome: StorageOnlyNormalizationOutcome,
    bounds: CommittedNormalizationGroupBounds,
}

/// Absolute document bounds of one committed normalization partition. `start`
/// and `end` are folded document coordinates on every checkpoint axis;
/// `interval` remains the partition's coordinate-free additive measure.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommittedNormalizationGroupBounds {
    start: RelativeCheckpointMeasure,
    interval: RelativeCheckpointMeasure,
    end: RelativeCheckpointMeasure,
}

#[cfg(feature = "exact-parser")]
impl CommittedNormalizationGroupBounds {
    pub(crate) const fn start(&self) -> RelativeCheckpointMeasure {
        self.start
    }

    pub(crate) const fn interval(&self) -> RelativeCheckpointMeasure {
        self.interval
    }

    pub(crate) const fn end(&self) -> RelativeCheckpointMeasure {
        self.end
    }
}

#[cfg(feature = "exact-parser")]
impl CommittedNormalizationOutcomeCapability<'_> {
    pub(crate) const fn group(&self) -> u64 {
        self.binding.group
    }

    pub(crate) const fn outcome(&self) -> StorageOnlyNormalizationOutcome {
        self.binding.outcome
    }

    pub(crate) const fn bounds(&self) -> CommittedNormalizationGroupBounds {
        self.binding.bounds
    }

    /// Selects the actual final sample under this committed group manifest.
    /// The supplied index and arena are checked again so a capability crossed
    /// onto another root cannot authorize suffix adoption there.
    pub(crate) fn completion_checkpoint(
        &self,
        index: &StorageOnlyCommittedCheckpointIndex,
        arena: &PageArena,
    ) -> Result<CommittedNormalizationCompletionCheckpoint, CommittedCheckpointIndexError> {
        index.locate_normalization_completion_checkpoint(arena, self)
    }
}

/// Authenticated final restart sample of a committed normalization group.
/// This is the only index-side value that proves the restart cut is at the
/// outcome frontier rather than merely somewhere inside the group.
#[cfg(feature = "exact-parser")]
pub(crate) struct CommittedNormalizationCompletionCheckpoint {
    binding: CommittedNormalizationGroupBinding,
    final_sample_ordinal: u64,
    donor: LocatedDonorCheckpointRecipe,
    receipt: NormalizationCompletionLookupReceipt,
}

#[cfg(feature = "exact-parser")]
impl CommittedNormalizationCompletionCheckpoint {
    pub(crate) const fn group(&self) -> u64 {
        self.binding.group
    }

    pub(crate) const fn outcome(&self) -> StorageOnlyNormalizationOutcome {
        self.binding.outcome
    }

    pub(crate) const fn bounds(&self) -> CommittedNormalizationGroupBounds {
        self.binding.bounds
    }

    pub(crate) const fn final_sample_ordinal(&self) -> u64 {
        self.final_sample_ordinal
    }

    pub(crate) const fn checkpoint_cut(&self) -> RelativeCheckpointMeasure {
        self.donor.checkpoint_cut
    }

    pub(crate) const fn receipt(&self) -> NormalizationCompletionLookupReceipt {
        self.receipt
    }

    /// Preserves the authenticated final-sample selection while handing the
    /// donor recipe to the later composite source/green/writer join.
    pub(crate) fn into_located_donor(self) -> LocatedDonorCheckpointRecipe {
        self.donor
    }
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NormalizationCompletionLookupReceipt {
    pub(crate) group_samples: u64,
    pub(crate) sample_tree_height: u16,
    pub(crate) sample_tree_nodes_visited: usize,
    pub(crate) sample_leaf_temporary_bytes: usize,
    pub(crate) path_nodes_visited: usize,
    pub(crate) reconstructed_opaque_path_bytes: usize,
    pub(crate) donor_typed_recipe_bytes: usize,
    pub(crate) maximum_temporary_bytes: usize,
    pub(crate) retained_source_bytes: usize,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DonorCheckpointLookupReceipt {
    pub(crate) path_nodes_visited: usize,
    pub(crate) reconstructed_opaque_path_bytes: usize,
    pub(crate) donor_typed_recipe_bytes: usize,
    pub(crate) maximum_temporary_bytes: usize,
    pub(crate) retained_source_bytes: usize,
}

#[cfg(feature = "exact-parser")]
impl LocatedDonorCheckpointRecipe {
    pub(crate) const fn ordinal(&self) -> u64 {
        self.ordinal
    }

    pub(crate) const fn prefix(&self) -> RelativeCheckpointMeasure {
        self.prefix
    }

    pub(crate) const fn interval(&self) -> RelativeCheckpointMeasure {
        self.interval
    }

    /// Absolute document measure at the completed interval end represented by
    /// this donor recipe.
    pub(crate) const fn checkpoint_cut(&self) -> RelativeCheckpointMeasure {
        self.checkpoint_cut
    }

    #[allow(clippy::unused_self)] // Instance-level proof receipt; the recipe retains no source.
    pub(crate) const fn retained_source_bytes(&self) -> usize {
        IndexedDonorCheckpointRecipe::retained_source_bytes()
    }

    pub(crate) const fn receipt(&self) -> DonorCheckpointLookupReceipt {
        self.receipt
    }

    /// Revalidates the opaque role and its exact selection binding against the
    /// current committed index. The normalization outcome is unavailable on
    /// every mismatch path.
    pub(crate) fn committed_role<'authority>(
        &'authority self,
        index: &'authority StorageOnlyCommittedCheckpointIndex,
        arena: &'authority PageArena,
    ) -> Result<CommittedDonorCheckpointRole<'authority>, CommittedCheckpointIndexError> {
        let authority = index.validate_donor_checkpoint_authority(arena, self)?;
        Ok(match self.role {
            DonorPartitionRole::DirectRun => {
                CommittedDonorCheckpointRole::DirectRun(CommittedDirectDonorCapability {
                    recipe: self,
                    _index: index,
                    _arena: arena,
                })
            }
            DonorPartitionRole::Normalization { group, outcome } => {
                let bounds = CommittedNormalizationGroupBounds {
                    start: authority.partition.prefix,
                    interval: authority.partition.interval,
                    end: authority
                        .partition
                        .prefix
                        .checked_add(authority.partition.interval)?,
                };
                CommittedDonorCheckpointRole::Normalization(
                    CommittedNormalizationOutcomeCapability {
                        recipe: self,
                        _origin_index: index,
                        _origin_arena: arena,
                        binding: CommittedNormalizationGroupBinding {
                            index_root: self.authority.index_root,
                            partition_manifest: authority.donor_partition.manifest,
                            group,
                            outcome,
                            bounds,
                        },
                    },
                )
            }
        })
    }

    pub(crate) fn matches_identity_witness(&self, witness: &OpaqueDonorIdentityWitness) -> bool {
        self.donor.matches_identity_witness(witness)
    }

    /// Decodes only grammar plus its opaque line-local half. Neither value can
    /// resume until the parent coordinator proves the sample's prefix/suffix
    /// induction and supplies current committed-green output.
    pub(crate) fn decode_grammar_parts(
        &self,
    ) -> Result<
        (
            DirectGrammarContinuation,
            DirectRestartLineLocalContinuation,
        ),
        ParseError,
    > {
        self.donor.decode_grammar_parts()
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedRestartAnchor {
    pub(crate) fn matches_parent_root(&self, parent_root: ArenaScopedId) -> bool {
        self.parent_root == parent_root
    }

    #[cfg(test)]
    pub(crate) const fn checkpoint_cut_for_test(&self) -> RelativeCheckpointMeasure {
        self.recipe.checkpoint_cut
    }

    /// Decodes the exact grammar state while preserving the linear anchor for
    /// the later installed-writer and convergence joins.
    pub(crate) fn decode_grammar_parts(
        &self,
    ) -> Result<
        (
            DirectGrammarContinuation,
            DirectRestartLineLocalContinuation,
        ),
        ParseError,
    > {
        self.recipe.decode_grammar_parts()
    }

    /// Seeds post-restart sparse sampling only when the installed candidate
    /// independently reproduces the retained checkpoint on every measured
    /// axis and belongs to the same arena as the selected parent.
    pub(crate) fn try_seed_suffix_samples(
        self,
        epoch: LiveCandidateEpoch,
        writer_cut: RelativeCheckpointMeasure,
    ) -> Result<
        (
            ParentSelectedSeededRestartAnchor,
            ParentSelectedSuffixSampleOrigin,
        ),
        CommittedCheckpointIndexError,
    > {
        if self.recipe.checkpoint_cut != writer_cut
            || self.recipe.authority.index_root.arena() != epoch.arena_identity()
        {
            return Err(CommittedCheckpointIndexError::Invalid(
                "installed candidate does not reproduce the parent-selected restart cut",
            ));
        }
        let origin = ParentSelectedSuffixSampleOrigin {
            epoch,
            authority: self.recipe.authority,
            restart_cut: self.recipe.checkpoint_cut,
        };
        Ok((
            ParentSelectedSeededRestartAnchor {
                parent_root: self.parent_root,
                recipe: self.recipe,
            },
            origin,
        ))
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedSeededRestartAnchor {
    /// Rejoins an old successor to the exact R authority that seeded its
    /// chain. A checkpoint from another parent, manifest, or restart sample is
    /// rejected even when all visible cuts happen to be equal.
    pub(crate) fn bind_old_convergence(
        &self,
        old_convergence: ParentBoundDonorSuccessor,
    ) -> Result<ParentBoundSourceConvergence, CommittedCheckpointIndexError> {
        if old_convergence.parent_root != self.parent_root
            || old_convergence.restart_authority != self.recipe.authority
        {
            return Err(CommittedCheckpointIndexError::Invalid(
                "old convergence checkpoint did not descend from the selected restart",
            ));
        }
        let interval = old_convergence
            .recipe
            .checkpoint_cut
            .checked_difference_from(self.recipe.checkpoint_cut)?;
        validate_parent_selected_suffix_interval(interval)?;
        Ok(ParentBoundSourceConvergence {
            restart_cut: self.recipe.checkpoint_cut,
            old_convergence,
        })
    }
}

#[cfg(feature = "exact-parser")]
impl ParentBoundSourceConvergence {
    pub(crate) fn green_event_cut(
        &self,
        _mint: crate::serialized_green::ParentBoundGreenConvergenceMint,
    ) -> u64 {
        self.old_convergence.recipe.checkpoint_cut.green_events
    }

    pub(crate) fn parent_root(
        &self,
        _mint: crate::serialized_green::ParentBoundGreenConvergenceMint,
    ) -> ArenaScopedId {
        self.old_convergence.parent_root
    }

    pub(crate) fn into_lineage_parts(
        self,
        _mint: crate::parent_selected_convergence::ParentBoundSourceLineageMint,
    ) -> (
        RelativeCheckpointMeasure,
        RelativeCheckpointMeasure,
        ParentBoundDonorSuccessor,
    ) {
        let convergence_cut = self.old_convergence.recipe.checkpoint_cut;
        (self.restart_cut, convergence_cut, self.old_convergence)
    }
}

#[cfg(feature = "exact-parser")]
impl ParentBoundDonorSuccessor {
    /// Compares one freshly captured live donor without exposing either
    /// donor's serialized grammar payload or persisted checkpoint recipe.
    pub(crate) fn matches_identity_witness(&self, witness: &OpaqueDonorIdentityWitness) -> bool {
        self.recipe.matches_identity_witness(witness)
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedSuffixSampleOrigin {
    /// Derives the first suffix-local interval from the authenticated restart
    /// cut. Physical-line checkpoints must make forward progress in source
    /// bytes, UTF-16 units, and line count; zero or regressing intervals fail
    /// closed before a cursor is minted.
    pub(crate) fn begin(
        self,
        epoch: LiveCandidateEpoch,
        current_cut: RelativeCheckpointMeasure,
    ) -> Result<
        (RelativeCheckpointMeasure, ParentSelectedSuffixSampleCursor),
        CommittedCheckpointIndexError,
    > {
        if self.epoch != epoch {
            return Err(CommittedCheckpointIndexError::Invalid(
                "parent-selected suffix sample epoch changed",
            ));
        }
        let interval = current_cut.checked_difference_from(self.restart_cut)?;
        validate_parent_selected_suffix_interval(interval)?;
        Ok((
            interval,
            ParentSelectedSuffixSampleCursor {
                epoch,
                authority: self.authority,
                sample_ordinal: 1,
                cumulative_cut: current_cut,
            },
        ))
    }

    /// Advances the origin while retaining one linear rollback half for a
    /// convergence probe that may be rejected as ephemeral evidence.
    pub(crate) fn begin_reversible(
        self,
        epoch: LiveCandidateEpoch,
        current_cut: RelativeCheckpointMeasure,
    ) -> Result<
        (
            RelativeCheckpointMeasure,
            ParentSelectedSuffixSampleCursor,
            ParentSelectedSuffixSampleRollback,
        ),
        CommittedCheckpointIndexError,
    > {
        if self.epoch != epoch {
            return Err(CommittedCheckpointIndexError::Invalid(
                "parent-selected suffix sample epoch changed",
            ));
        }
        let interval = current_cut.checked_difference_from(self.restart_cut)?;
        validate_parent_selected_suffix_interval(interval)?;
        let next = ParentSelectedSuffixSampleCursor {
            epoch,
            authority: self.authority,
            sample_ordinal: 1,
            cumulative_cut: current_cut,
        };
        let rollback = ParentSelectedSuffixSampleRollback {
            epoch,
            authority: self.authority,
            rejected_sample_ordinal: 1,
            rejected_cumulative_cut: current_cut,
            prior: ParentSelectedSuffixSamplePrior::AwaitingFirst(self),
        };
        Ok((interval, next, rollback))
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedSuffixSampleCursor {
    pub(crate) const fn epoch(&self) -> LiveCandidateEpoch {
        self.epoch
    }

    pub(crate) const fn sample_ordinal(&self) -> u64 {
        self.sample_ordinal
    }

    pub(crate) const fn cumulative_cut(&self) -> RelativeCheckpointMeasure {
        self.cumulative_cut
    }

    /// Advances one line-boundary sample using only the actor-observed current
    /// cut and this linear cursor's prior cut.
    pub(crate) fn advance(
        self,
        epoch: LiveCandidateEpoch,
        current_cut: RelativeCheckpointMeasure,
    ) -> Result<
        (RelativeCheckpointMeasure, ParentSelectedSuffixSampleCursor),
        CommittedCheckpointIndexError,
    > {
        if self.epoch != epoch {
            return Err(CommittedCheckpointIndexError::Invalid(
                "parent-selected suffix sample epoch changed",
            ));
        }
        let interval = current_cut.checked_difference_from(self.cumulative_cut)?;
        validate_parent_selected_suffix_interval(interval)?;
        let sample_ordinal =
            self.sample_ordinal
                .checked_add(1)
                .ok_or(CommittedCheckpointIndexError::Overflow(
                    "parent-selected suffix sample ordinal",
                ))?;
        Ok((
            interval,
            ParentSelectedSuffixSampleCursor {
                epoch,
                authority: self.authority,
                sample_ordinal,
                cumulative_cut: current_cut,
            },
        ))
    }

    /// Advances this cursor while retaining its exact predecessor in a
    /// one-shot rollback authority owned by the speculative probe.
    pub(crate) fn advance_reversible(
        self,
        epoch: LiveCandidateEpoch,
        current_cut: RelativeCheckpointMeasure,
    ) -> Result<
        (
            RelativeCheckpointMeasure,
            ParentSelectedSuffixSampleCursor,
            ParentSelectedSuffixSampleRollback,
        ),
        CommittedCheckpointIndexError,
    > {
        if self.epoch != epoch {
            return Err(CommittedCheckpointIndexError::Invalid(
                "parent-selected suffix sample epoch changed",
            ));
        }
        let interval = current_cut.checked_difference_from(self.cumulative_cut)?;
        validate_parent_selected_suffix_interval(interval)?;
        let sample_ordinal =
            self.sample_ordinal
                .checked_add(1)
                .ok_or(CommittedCheckpointIndexError::Overflow(
                    "parent-selected suffix sample ordinal",
                ))?;
        let next = ParentSelectedSuffixSampleCursor {
            epoch,
            authority: self.authority,
            sample_ordinal,
            cumulative_cut: current_cut,
        };
        let rollback = ParentSelectedSuffixSampleRollback {
            epoch,
            authority: self.authority,
            rejected_sample_ordinal: sample_ordinal,
            rejected_cumulative_cut: current_cut,
            prior: ParentSelectedSuffixSamplePrior::Continuing(self),
        };
        Ok((interval, next, rollback))
    }

    /// Confirms this current-suffix chain still descends from the exact parent
    /// checkpoint that seeded it. Persisted coordinates never leave the
    /// module as independently forgeable convergence authority.
    pub(crate) fn matches_parent_anchor(&self, anchor: &ParentSelectedSeededRestartAnchor) -> bool {
        self.authority == anchor.recipe.authority
    }

    #[cfg(test)]
    pub(crate) const fn sample_ordinal_for_test(&self) -> u64 {
        self.sample_ordinal
    }
}

#[cfg(feature = "exact-parser")]
impl ParentSelectedSuffixSampleRollback {
    /// Restores the exact cursor predecessor only while the candidate still
    /// owns the advanced cursor minted beside this rollback half.
    pub(crate) fn restore(
        self,
        epoch: LiveCandidateEpoch,
        current: ParentSelectedSuffixSampleCursor,
    ) -> Result<ParentSelectedSuffixSamplePrior, CommittedCheckpointIndexError> {
        if self.epoch != epoch
            || current.epoch != epoch
            || current.authority != self.authority
            || current.sample_ordinal != self.rejected_sample_ordinal
            || current.cumulative_cut != self.rejected_cumulative_cut
        {
            return Err(CommittedCheckpointIndexError::Invalid(
                "rejected suffix probe crossed its advanced cursor",
            ));
        }
        Ok(self.prior)
    }
}

#[cfg(feature = "exact-parser")]
impl ParentRetainedCheckpointIndexLease<'_> {
    /// Selects the first persisted convergence candidate strictly after the
    /// authenticated restart checkpoint. Selection is ordinal-based inside
    /// the exact same donor manifest, so it cannot accidentally return R.
    pub(crate) fn begin_parent_bound_donor_successor(
        &self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        anchor: &ParentSelectedSeededRestartAnchor,
    ) -> Result<ParentBoundDonorSuccessorStep, CommittedCheckpointIndexError> {
        let root = self.validated_suspended_root(ticket, arena)?;
        let scoped_root = arena.scoped_query_id(root)?;
        if self.parent_activation != anchor.parent_root
            || anchor.recipe.authority.index_root != scoped_root
        {
            return Err(CommittedCheckpointIndexError::Invalid(
                "restart anchor and retained checkpoint parent differ",
            ));
        }
        let validated =
            validate_donor_checkpoint_for_root(arena, root, scoped_root, &anchor.recipe)?;
        select_parent_bound_donor_successor(
            arena,
            root,
            scoped_root,
            anchor.parent_root,
            anchor.recipe.authority,
            &anchor.recipe,
            validated,
        )
    }

    /// Advances after a live-C mismatch. The old checkpoint is revalidated
    /// before its exact persisted ordinal advances in the same manifest.
    pub(crate) fn advance_parent_bound_donor_successor(
        &self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        current: ParentBoundDonorSuccessor,
    ) -> Result<ParentBoundDonorSuccessorStep, CommittedCheckpointIndexError> {
        let root = self.validated_suspended_root(ticket, arena)?;
        let scoped_root = arena.scoped_query_id(root)?;
        if self.parent_activation != current.parent_root
            || current.restart_authority.index_root != scoped_root
            || current.recipe.authority.index_root != scoped_root
        {
            return Err(CommittedCheckpointIndexError::Invalid(
                "old convergence checkpoint and retained parent differ",
            ));
        }
        let validated =
            validate_donor_checkpoint_for_root(arena, root, scoped_root, &current.recipe)?;
        select_parent_bound_donor_successor(
            arena,
            root,
            scoped_root,
            current.parent_root,
            current.restart_authority,
            &current.recipe,
            validated,
        )
    }

    /// Consumes exactly one authenticated donor-to-donor outer transition.
    /// Restart barriers, terminal tails, and EOF never mint this capability,
    /// so this method cannot skip a semantic partition by coordinate.
    pub(crate) fn advance_parent_bound_donor_partition(
        &self,
        ticket: &ArenaBuildTicket,
        arena: &PageArena,
        transition: ParentBoundDonorPartitionTransition,
    ) -> Result<ParentBoundDonorSuccessorStep, CommittedCheckpointIndexError> {
        let root = self.validated_suspended_root(ticket, arena)?;
        let scoped_root = arena.scoped_query_id(root)?;
        if self.parent_activation != transition.parent_root
            || transition.restart_authority.index_root != scoped_root
            || transition.final_authority.index_root != scoped_root
        {
            return Err(CommittedCheckpointIndexError::Invalid(
                "donor partition transition and retained parent differ",
            ));
        }
        select_first_parent_bound_donor_after_transition(arena, root, scoped_root, transition)
            .map(ParentBoundDonorSuccessorStep::Checkpoint)
    }
}

#[cfg(feature = "exact-parser")]
fn select_first_parent_bound_donor_after_transition(
    arena: &PageArena,
    index_root: ArenaId,
    scoped_root: ArenaScopedId,
    transition: ParentBoundDonorPartitionTransition,
) -> Result<ParentBoundDonorSuccessor, CommittedCheckpointIndexError> {
    let previous_ordinal = transition.final_authority.partition_ordinal;
    if previous_ordinal.checked_add(1) != Some(transition.next_partition_ordinal) {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor transition does not name adjacent outer partitions",
        ));
    }
    let previous = locate_outer_partition_by_ordinal(arena, index_root, previous_ordinal)?;
    let LocatedCheckpointPartitionKind::Donor(previous_donor) = previous.kind else {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor transition predecessor is no longer a donor partition",
        ));
    };
    if previous.prefix != transition.final_authority.partition_prefix
        || previous_donor.index_root != index_root
        || previous_donor.manifest != transition.final_authority.partition_manifest
        || previous_donor.interval != previous.interval
    {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor transition predecessor binding changed",
        ));
    }
    let previous_manifest = decode_donor_partition_manifest(arena, previous_donor.manifest)?;
    if previous_manifest.measure != previous.interval
        || previous_manifest.samples != previous_donor.samples
        || transition.final_authority.sample_ordinal.checked_add(1)
            != Some(previous_manifest.samples)
    {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor transition predecessor is not the final sample",
        ));
    }
    let final_sample = locate_donor_sample_by_ordinal(
        arena,
        previous_manifest.sample_root,
        transition.final_authority.sample_ordinal,
    )?;
    let previous_end = previous.prefix.checked_add(previous.interval)?;
    if final_sample.header != transition.final_authority.sample_header
        || final_sample.path_terminal != transition.final_authority.sample_path_terminal
        || previous
            .prefix
            .checked_add(final_sample.prefix)?
            .checked_add(final_sample.interval)?
            != previous_end
        || previous_end != transition.next_partition_prefix
    {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor transition final authority no longer reaches the next partition",
        ));
    }

    let next =
        locate_outer_partition_by_ordinal(arena, index_root, transition.next_partition_ordinal)?;
    let LocatedCheckpointPartitionKind::Donor(next_donor) = next.kind else {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor transition target is no longer a donor partition",
        ));
    };
    if next.prefix != transition.next_partition_prefix
        || next.interval != transition.next_partition_interval
        || next_donor.index_root != index_root
        || next_donor.manifest != transition.next_partition_manifest
        || next_donor.interval != next.interval
        || next_donor.samples == 0
    {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor transition target binding changed",
        ));
    }
    let next_manifest = decode_donor_partition_manifest(arena, next_donor.manifest)?;
    if next_manifest.measure != next.interval || next_manifest.samples != next_donor.samples {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor transition target manifest changed",
        ));
    }
    let first = locate_donor_sample_by_ordinal(arena, next_manifest.sample_root, 0)?;
    let recipe = reconstruct_located_donor_checkpoint_for_root(
        arena,
        scoped_root,
        next,
        next_donor,
        next_manifest,
        first,
    )?;
    if recipe.ordinal != 0 || recipe.prefix != next.prefix {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor transition did not select the first target checkpoint",
        ));
    }
    Ok(ParentBoundDonorSuccessor {
        parent_root: transition.parent_root,
        restart_authority: transition.restart_authority,
        recipe,
    })
}

#[cfg(feature = "exact-parser")]
fn select_parent_bound_donor_successor(
    arena: &PageArena,
    index_root: ArenaId,
    scoped_root: ArenaScopedId,
    parent_root: ArenaScopedId,
    restart_authority: DonorCheckpointAuthorityBinding,
    current: &LocatedDonorCheckpointRecipe,
    validated: RevalidatedDonorCheckpointAuthority,
) -> Result<ParentBoundDonorSuccessorStep, CommittedCheckpointIndexError> {
    if current.authority != validated_sample_authority(scoped_root, validated) {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "revalidated convergence predecessor authority changed",
        ));
    }
    let next_ordinal =
        current
            .ordinal
            .checked_add(1)
            .ok_or(CommittedCheckpointIndexError::Overflow(
                "old convergence checkpoint ordinal",
            ))?;
    if next_ordinal < validated.manifest.samples {
        let next =
            locate_donor_sample_by_ordinal(arena, validated.manifest.sample_root, next_ordinal)?;
        let expected_relative_prefix = validated
            .sample
            .prefix
            .checked_add(validated.sample.interval)?;
        if next.prefix != expected_relative_prefix {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "old convergence successor is not contiguous with its predecessor",
            ));
        }
        let recipe = reconstruct_located_donor_checkpoint_for_root(
            arena,
            scoped_root,
            validated.partition,
            validated.donor_partition,
            validated.manifest,
            next,
        )?;
        if recipe.prefix != current.checkpoint_cut
            || recipe.role != current.role
            || recipe.authority.partition_manifest != current.authority.partition_manifest
        {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "old convergence successor crossed its authenticated donor manifest",
            ));
        }
        return Ok(ParentBoundDonorSuccessorStep::Checkpoint(
            ParentBoundDonorSuccessor {
                parent_root,
                restart_authority,
                recipe,
            },
        ));
    }
    if next_ordinal != validated.manifest.samples {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "old convergence predecessor ordinal exceeds its donor manifest",
        ));
    }
    select_parent_bound_donor_partition_end(
        arena,
        index_root,
        parent_root,
        restart_authority,
        current,
        validated,
    )
}

#[cfg(feature = "exact-parser")]
fn validated_sample_authority(
    scoped_root: ArenaScopedId,
    validated: RevalidatedDonorCheckpointAuthority,
) -> DonorCheckpointAuthorityBinding {
    DonorCheckpointAuthorityBinding {
        index_root: scoped_root,
        partition_ordinal: validated.partition.ordinal,
        partition_prefix: validated.partition.prefix,
        partition_manifest: validated.donor_partition.manifest,
        sample_ordinal: validated.sample.ordinal,
        sample_header: validated.sample.header,
        sample_path_terminal: validated.sample.path_terminal,
    }
}

#[cfg(feature = "exact-parser")]
fn select_parent_bound_donor_partition_end(
    arena: &PageArena,
    index_root: ArenaId,
    parent_root: ArenaScopedId,
    restart_authority: DonorCheckpointAuthorityBinding,
    current: &LocatedDonorCheckpointRecipe,
    validated: RevalidatedDonorCheckpointAuthority,
) -> Result<ParentBoundDonorSuccessorStep, CommittedCheckpointIndexError> {
    let partition_end = validated
        .partition
        .prefix
        .checked_add(validated.partition.interval)?;
    if current.checkpoint_cut != partition_end {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "final donor sample does not end at its partition boundary",
        ));
    }
    let summary = sequence_node::<CheckpointIndexSpec>(arena, index_root)?.0;
    let next_partition_ordinal = validated.partition.ordinal.checked_add(1).ok_or(
        CommittedCheckpointIndexError::Overflow("old convergence outer partition ordinal"),
    )?;
    let kind = if next_partition_ordinal == summary.partitions {
        ParentBoundDonorSuccessorBoundaryKind::SourceEof
    } else if next_partition_ordinal < summary.partitions {
        let next = locate_outer_partition_by_ordinal(arena, index_root, next_partition_ordinal)?;
        if next.prefix != partition_end {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "checkpoint partition after convergence predecessor is not contiguous",
            ));
        }
        match next.kind {
            LocatedCheckpointPartitionKind::Donor(donor) => {
                if donor.index_root != index_root
                    || donor.interval != next.interval
                    || donor.samples == 0
                {
                    return Err(CommittedCheckpointIndexError::Corrupt(
                        "next donor partition changed during transition mint",
                    ));
                }
                return Ok(ParentBoundDonorSuccessorStep::NextPartition(
                    ParentBoundDonorPartitionTransition {
                        parent_root,
                        restart_authority,
                        final_authority: current.authority,
                        next_partition_ordinal,
                        next_partition_prefix: next.prefix,
                        next_partition_manifest: donor.manifest,
                        next_partition_interval: next.interval,
                    },
                ));
            }
            LocatedCheckpointPartitionKind::TerminalTail => {
                ParentBoundDonorSuccessorBoundaryKind::TerminalSemanticTail
            }
            LocatedCheckpointPartitionKind::Direct
            | LocatedCheckpointPartitionKind::Normalization(_) => {
                ParentBoundDonorSuccessorBoundaryKind::RestartBarrier
            }
        }
    } else {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "old convergence partition ordinal exceeds checkpoint index",
        ));
    };
    Ok(ParentBoundDonorSuccessorStep::PartitionEnd(
        ParentBoundDonorSuccessorBoundary {
            parent_root,
            restart_authority,
            final_authority: current.authority,
            kind,
        },
    ))
}

#[cfg(all(feature = "exact-parser", test))]
impl ParentBoundDonorSuccessor {
    pub(crate) const fn checkpoint_cut_for_test(&self) -> RelativeCheckpointMeasure {
        self.recipe.checkpoint_cut
    }

    pub(crate) const fn ordinal_for_test(&self) -> u64 {
        self.recipe.ordinal
    }
}

#[cfg(all(feature = "exact-parser", test))]
impl ParentBoundDonorSuccessorBoundary {
    pub(crate) const fn kind_for_test(&self) -> &'static str {
        match self.kind {
            ParentBoundDonorSuccessorBoundaryKind::RestartBarrier => "restart-barrier",
            ParentBoundDonorSuccessorBoundaryKind::TerminalSemanticTail => "terminal-tail",
            ParentBoundDonorSuccessorBoundaryKind::SourceEof => "source-eof",
        }
    }
}

#[cfg(all(feature = "exact-parser", test))]
impl ParentBoundDonorSuccessorStep {
    pub(crate) const fn checkpoint_cut_for_test(&self) -> Option<RelativeCheckpointMeasure> {
        match self {
            Self::Checkpoint(checkpoint) => Some(checkpoint.recipe.checkpoint_cut),
            Self::NextPartition(_) | Self::PartitionEnd(_) => None,
        }
    }

    pub(crate) const fn checkpoint_ordinal_for_test(&self) -> Option<u64> {
        match self {
            Self::Checkpoint(checkpoint) => Some(checkpoint.recipe.ordinal),
            Self::NextPartition(_) | Self::PartitionEnd(_) => None,
        }
    }

    pub(crate) const fn boundary_kind_for_test(&self) -> Option<&'static str> {
        match self {
            Self::Checkpoint(_) => None,
            Self::NextPartition(_) => Some("next-donor-partition"),
            Self::PartitionEnd(boundary) => Some(boundary.kind_for_test()),
        }
    }
}

#[cfg(feature = "exact-parser")]
fn validate_parent_selected_suffix_interval(
    interval: RelativeCheckpointMeasure,
) -> Result<(), CommittedCheckpointIndexError> {
    if interval.source_bytes == 0 || interval.source_utf16 == 0 || interval.physical_lines == 0 {
        return Err(CommittedCheckpointIndexError::Invalid(
            "parent-selected suffix sample did not advance a physical line",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedNormalizationManifest {
    group: u64,
    outcome: StorageOnlyNormalizationOutcome,
    measure: RelativeCheckpointMeasure,
    samples: u64,
    sample_root: ArenaId,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DonorPartitionRole {
    DirectRun,
    Normalization {
        group: u64,
        outcome: StorageOnlyNormalizationOutcome,
    },
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedDonorPartitionManifest {
    role: DonorPartitionRole,
    measure: RelativeCheckpointMeasure,
    samples: u64,
    sample_root: ArenaId,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedDonorSampleRecord {
    interval: RelativeCheckpointMeasure,
    header: OpaqueDonorHeader,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocatedOpaqueDonorSample {
    ordinal: u64,
    prefix: RelativeCheckpointMeasure,
    interval: RelativeCheckpointMeasure,
    header: OpaqueDonorHeader,
    path_terminal: ArenaId,
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocatedDonorSampleByOrdinalReceipt {
    sample: LocatedOpaqueDonorSample,
    tree_height: u16,
    nodes_visited: usize,
    maximum_temporary_bytes: usize,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
struct ReconstructedDonorPath {
    frames: Vec<OpaqueDonorFrame>,
    nodes_visited: usize,
    scratch_storage_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommittedCheckpointIndexError {
    Arena(ArenaError),
    ArenaBuild(ArenaBuildError),
    Invalid(&'static str),
    Corrupt(&'static str),
    Overflow(&'static str),
    Allocation(&'static str),
    SourceOutOfBounds,
}

impl From<ArenaError> for CommittedCheckpointIndexError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

impl From<ArenaBuildError> for CommittedCheckpointIndexError {
    fn from(value: ArenaBuildError) -> Self {
        Self::ArenaBuild(value)
    }
}

impl fmt::Display for CommittedCheckpointIndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arena(error) => error.fmt(formatter),
            Self::ArenaBuild(error) => error.fmt(formatter),
            Self::Invalid(message) => write!(formatter, "invalid checkpoint index: {message}"),
            Self::Corrupt(message) => write!(formatter, "corrupt checkpoint index: {message}"),
            Self::Overflow(field) => write!(formatter, "checkpoint index {field} overflow"),
            Self::Allocation(component) => {
                write!(formatter, "checkpoint index could not allocate {component}")
            }
            Self::SourceOutOfBounds => {
                formatter.write_str("checkpoint index source is out of bounds")
            }
        }
    }
}

impl std::error::Error for CommittedCheckpointIndexError {}

#[derive(Debug)]
struct CheckpointIndexSpec;

impl SequenceSpec for CheckpointIndexSpec {
    type Summary = CheckpointIndexSummary;
    type Error = CommittedCheckpointIndexError;
    type BranchPayload = [u8; SUMMARY_BYTES];

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(INDEX_LEAF_TAG) {
            return Ok(None);
        }
        let expected = decode_summary(payload, INDEX_LEAF_TAG)?;
        let records = decode_outer_leaf_records(payload)?;
        let actual = records.into_iter().try_fold(
            CheckpointIndexSummary {
                leaf_pages: 1,
                height: 1,
                ..CheckpointIndexSummary::default()
            },
            |summary, record| {
                summary.followed_by(CheckpointIndexSummary {
                    partitions: 1,
                    samples: record.samples,
                    measure: record.interval,
                    terminal_tail: matches!(record.kind, DecodedPartitionKind::TerminalTail),
                    ..CheckpointIndexSummary::default()
                })
            },
        )?;
        if expected != actual {
            return Err(Self::invalid("outer leaf summary disagrees with records"));
        }
        Ok(Some(actual))
    }

    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        (payload.first().copied() == Some(INDEX_BRANCH_TAG))
            .then(|| decode_summary(payload, INDEX_BRANCH_TAG))
            .transpose()
    }

    fn encode_branch(summary: Self::Summary) -> Self::BranchPayload {
        encode_summary(INDEX_BRANCH_TAG, summary)
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        left.followed_by(right)
    }

    fn leaves(summary: Self::Summary) -> u64 {
        summary.leaf_pages
    }

    fn height(summary: Self::Summary) -> u16 {
        summary.height
    }

    fn invalid(message: &'static str) -> Self::Error {
        CommittedCheckpointIndexError::Corrupt(message)
    }
}

#[derive(Debug)]
struct NormalizationSampleSpec;

impl SequenceSpec for NormalizationSampleSpec {
    type Summary = CheckpointIndexSummary;
    type Error = CommittedCheckpointIndexError;
    type BranchPayload = [u8; SUMMARY_BYTES];

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(SAMPLE_LEAF_TAG) {
            return Ok(None);
        }
        let expected = decode_summary(payload, SAMPLE_LEAF_TAG)?;
        let records = decode_sample_leaf_records(payload)?;
        let actual = records.into_iter().try_fold(
            CheckpointIndexSummary {
                leaf_pages: 1,
                height: 1,
                ..CheckpointIndexSummary::default()
            },
            |summary, interval| {
                summary.followed_by(CheckpointIndexSummary {
                    partitions: 1,
                    samples: 1,
                    measure: interval,
                    ..CheckpointIndexSummary::default()
                })
            },
        )?;
        if expected != actual {
            return Err(Self::invalid("sample leaf summary disagrees with records"));
        }
        Ok(Some(actual))
    }

    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        (payload.first().copied() == Some(SAMPLE_BRANCH_TAG))
            .then(|| decode_summary(payload, SAMPLE_BRANCH_TAG))
            .transpose()
    }

    fn encode_branch(summary: Self::Summary) -> Self::BranchPayload {
        encode_summary(SAMPLE_BRANCH_TAG, summary)
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        left.followed_by(right)
    }

    fn leaves(summary: Self::Summary) -> u64 {
        summary.leaf_pages
    }

    fn height(summary: Self::Summary) -> u16 {
        summary.height
    }

    fn invalid(message: &'static str) -> Self::Error {
        CommittedCheckpointIndexError::Corrupt(message)
    }
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
struct DonorSampleSpec;

#[cfg(feature = "exact-parser")]
impl SequenceSpec for DonorSampleSpec {
    type Summary = CheckpointIndexSummary;
    type Error = CommittedCheckpointIndexError;
    type BranchPayload = [u8; SUMMARY_BYTES];

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(DONOR_SAMPLE_LEAF_TAG) {
            return Ok(None);
        }
        let expected = decode_summary(payload, DONOR_SAMPLE_LEAF_TAG)?;
        let records = decode_donor_sample_leaf_payload(payload)?;
        let actual = records.into_iter().try_fold(
            CheckpointIndexSummary {
                leaf_pages: 1,
                height: 1,
                ..CheckpointIndexSummary::default()
            },
            |summary, record| {
                summary.followed_by(CheckpointIndexSummary {
                    partitions: 1,
                    samples: 1,
                    measure: record.interval,
                    ..CheckpointIndexSummary::default()
                })
            },
        )?;
        if expected != actual {
            return Err(Self::invalid(
                "donor sample leaf summary disagrees with records",
            ));
        }
        Ok(Some(actual))
    }

    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        (payload.first().copied() == Some(DONOR_SAMPLE_BRANCH_TAG))
            .then(|| decode_summary(payload, DONOR_SAMPLE_BRANCH_TAG))
            .transpose()
    }

    fn encode_branch(summary: Self::Summary) -> Self::BranchPayload {
        encode_summary(DONOR_SAMPLE_BRANCH_TAG, summary)
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        left.followed_by(right)
    }

    fn leaves(summary: Self::Summary) -> u64 {
        summary.leaf_pages
    }

    fn height(summary: Self::Summary) -> u16 {
        summary.height
    }

    fn invalid(message: &'static str) -> Self::Error {
        CommittedCheckpointIndexError::Corrupt(message)
    }
}

#[derive(Debug)]
struct OuterLeafEncoder {
    payload: Vec<u8>,
    children: Vec<ArenaBuildOwner>,
    summary: CheckpointIndexSummary,
}

impl OuterLeafEncoder {
    fn new() -> Result<Self, CommittedCheckpointIndexError> {
        let mut payload = Vec::new();
        payload
            .try_reserve_exact(ARENA_PAGE_BYTES)
            .map_err(|_| CommittedCheckpointIndexError::Allocation("outer leaf payload"))?;
        payload.resize(SUMMARY_BYTES, 0);
        let mut children = Vec::new();
        children
            .try_reserve_exact(MAX_PACKED_ARENA_CHILDREN)
            .map_err(|_| CommittedCheckpointIndexError::Allocation("outer leaf children"))?;
        Ok(Self {
            payload,
            children,
            summary: CheckpointIndexSummary {
                leaf_pages: 1,
                height: 1,
                ..CheckpointIndexSummary::default()
            },
        })
    }

    fn is_empty(&self) -> bool {
        self.summary.partitions == 0
    }

    fn can_fit(&self, adds_child: bool) -> bool {
        let children = self.children.len() + usize::from(adds_child);
        children <= MAX_PACKED_ARENA_CHILDREN
            && self.payload.len() + PARTITION_RECORD_BYTES + children * 8 <= ARENA_PAGE_BYTES
    }

    fn push_direct(
        &mut self,
        interval: RelativeCheckpointMeasure,
    ) -> Result<(), CommittedCheckpointIndexError> {
        debug_assert!(self.can_fit(false));
        encode_partition_record(
            &mut self.payload,
            DIRECT_PARTITION_TAG,
            NO_CHILD_ORDINAL,
            interval,
            1,
        );
        self.observe(interval, 1)
    }

    fn push_terminal_tail(
        &mut self,
        interval: RelativeCheckpointMeasure,
    ) -> Result<(), CommittedCheckpointIndexError> {
        debug_assert!(self.can_fit(false));
        encode_partition_record(
            &mut self.payload,
            TERMINAL_TAIL_PARTITION_TAG,
            NO_CHILD_ORDINAL,
            interval,
            0,
        );
        self.summary = self.summary.followed_by(CheckpointIndexSummary {
            partitions: 1,
            measure: interval,
            terminal_tail: true,
            ..CheckpointIndexSummary::default()
        })?;
        Ok(())
    }

    fn push_normalization(
        &mut self,
        interval: RelativeCheckpointMeasure,
        samples: u64,
        manifest: ArenaBuildOwner,
    ) -> Result<(), CommittedCheckpointIndexError> {
        debug_assert!(self.can_fit(true));
        let child_ordinal = u16::try_from(self.children.len()).map_err(|_| {
            CommittedCheckpointIndexError::Invalid("outer child ordinal exceeds u16")
        })?;
        self.children.push(manifest);
        encode_partition_record(
            &mut self.payload,
            NORMALIZATION_PARTITION_TAG,
            child_ordinal,
            interval,
            samples,
        );
        self.observe(interval, samples)
    }

    #[cfg(feature = "exact-parser")]
    fn push_donor(
        &mut self,
        interval: RelativeCheckpointMeasure,
        samples: u64,
        manifest: ArenaBuildOwner,
    ) -> Result<(), CommittedCheckpointIndexError> {
        debug_assert!(self.can_fit(true));
        let child_ordinal = u16::try_from(self.children.len()).map_err(|_| {
            CommittedCheckpointIndexError::Invalid("outer child ordinal exceeds u16")
        })?;
        self.children.push(manifest);
        encode_partition_record(
            &mut self.payload,
            DONOR_PARTITION_TAG,
            child_ordinal,
            interval,
            samples,
        );
        self.observe(interval, samples)
    }

    fn observe(
        &mut self,
        interval: RelativeCheckpointMeasure,
        samples: u64,
    ) -> Result<(), CommittedCheckpointIndexError> {
        self.summary = self.summary.followed_by(CheckpointIndexSummary {
            partitions: 1,
            samples,
            measure: interval,
            ..CheckpointIndexSummary::default()
        })?;
        Ok(())
    }

    fn reset(&mut self) {
        self.payload.clear();
        self.payload.resize(SUMMARY_BYTES, 0);
        self.children.clear();
        self.summary = CheckpointIndexSummary {
            leaf_pages: 1,
            height: 1,
            ..CheckpointIndexSummary::default()
        };
    }
}

fn flush_outer_leaf(
    session: &mut ArenaBuildSession<'_>,
    sequence: &mut ResumableStreamingSequenceBuilder<CheckpointIndexSpec>,
    page: &mut OuterLeafEncoder,
    receipt: &mut CommittedCheckpointIndexBuildReceipt,
) -> Result<(), CommittedCheckpointIndexError> {
    if page.is_empty() {
        return Ok(());
    }
    let header = encode_summary(INDEX_LEAF_TAG, page.summary);
    page.payload[..SUMMARY_BYTES].copy_from_slice(&header);
    let child_ids = page
        .children
        .iter()
        .map(|owner| session.owner_id(owner))
        .collect::<Result<Vec<_>, _>>()?;
    let (leaf, allocation) = session.allocate_packed(&page.payload, &child_ids)?;
    observe_allocation(receipt, allocation);
    receipt.outer_leaf_pages = receipt
        .outer_leaf_pages
        .checked_add(1)
        .ok_or(CommittedCheckpointIndexError::Overflow("outer leaf count"))?;
    for owner in page.children.drain(..) {
        session.release(owner)?;
    }
    sequence.begin_push(session, leaf, &mut receipt.sequence)?;
    while sequence.poll_push(session, &mut receipt.sequence)? == ResumableSequenceProgress::Pending
    {
    }
    page.reset();
    Ok(())
}

fn build_sample_sequence(
    session: &mut ArenaBuildSession<'_>,
    samples: Vec<RelativeCheckpointMeasure>,
    receipt: &mut CommittedCheckpointIndexBuildReceipt,
) -> Result<ArenaBuildOwner, CommittedCheckpointIndexError> {
    if samples.is_empty() {
        return Err(CommittedCheckpointIndexError::Invalid(
            "normalization group has no restart samples",
        ));
    }
    let mut sequence = ResumableStreamingSequenceBuilder::<NormalizationSampleSpec>::try_new(
        &mut receipt.sequence,
    )?;
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(ARENA_PAGE_BYTES)
        .map_err(|_| CommittedCheckpointIndexError::Allocation("sample leaf payload"))?;
    payload.resize(SUMMARY_BYTES, 0);
    let mut summary = empty_leaf_summary();

    for interval in samples {
        require_nonempty_interval(interval)?;
        if payload.len() + SAMPLE_RECORD_BYTES > ARENA_PAGE_BYTES {
            flush_sample_leaf(session, &mut sequence, &mut payload, summary, receipt)?;
            summary = empty_leaf_summary();
        }
        encode_measure(&mut payload, interval);
        summary = summary.followed_by(CheckpointIndexSummary {
            partitions: 1,
            samples: 1,
            measure: interval,
            ..CheckpointIndexSummary::default()
        })?;
    }
    flush_sample_leaf(session, &mut sequence, &mut payload, summary, receipt)?;
    finish_sequence(session, &mut sequence, &mut receipt.sequence)
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
struct DonorSampleLeafEncoder {
    payload: Vec<u8>,
    path_owners: Vec<ArenaBuildOwner>,
    summary: CheckpointIndexSummary,
}

#[cfg(feature = "exact-parser")]
impl DonorSampleLeafEncoder {
    fn new() -> Result<Self, CommittedCheckpointIndexError> {
        let mut payload = Vec::new();
        payload
            .try_reserve_exact(ARENA_PAGE_BYTES)
            .map_err(|_| CommittedCheckpointIndexError::Allocation("donor sample leaf payload"))?;
        payload.resize(SUMMARY_BYTES, 0);
        let mut path_owners = Vec::new();
        path_owners
            .try_reserve_exact(MAX_PACKED_ARENA_CHILDREN)
            .map_err(|_| CommittedCheckpointIndexError::Allocation("donor sample path owners"))?;
        Ok(Self {
            payload,
            path_owners,
            summary: empty_leaf_summary(),
        })
    }

    fn can_fit(&self) -> bool {
        let children = self.path_owners.len() + 1;
        children <= MAX_PACKED_ARENA_CHILDREN
            && self.payload.len() + DONOR_SAMPLE_RECORD_BYTES + children * 8 <= ARENA_PAGE_BYTES
    }

    fn push(
        &mut self,
        interval: RelativeCheckpointMeasure,
        header: &OpaqueDonorHeader,
        path_owner: ArenaBuildOwner,
    ) -> Result<(), CommittedCheckpointIndexError> {
        debug_assert!(self.can_fit());
        encode_measure(&mut self.payload, interval);
        self.payload.extend_from_slice(header);
        self.path_owners.push(path_owner);
        self.summary = self.summary.followed_by(CheckpointIndexSummary {
            partitions: 1,
            samples: 1,
            measure: interval,
            ..CheckpointIndexSummary::default()
        })?;
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.summary.samples == 0
    }

    fn reset(&mut self) {
        self.payload.clear();
        self.payload.resize(SUMMARY_BYTES, 0);
        self.path_owners.clear();
        self.summary = empty_leaf_summary();
    }
}

#[cfg(feature = "exact-parser")]
#[derive(Debug, Default)]
struct DonorPathCache {
    previous_frames: Vec<OpaqueDonorFrame>,
    previous_nodes: Vec<ArenaId>,
    terminal: Option<ArenaBuildOwner>,
}

#[cfg(feature = "exact-parser")]
impl DonorPathCache {
    fn sample_path_owner(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
        frames: &[OpaqueDonorFrame],
        receipt: &mut CommittedCheckpointIndexBuildReceipt,
    ) -> Result<ArenaBuildOwner, CommittedCheckpointIndexError> {
        if frames.is_empty() {
            return Err(CommittedCheckpointIndexError::Invalid(
                "donor sample has an empty open path",
            ));
        }
        let common = self
            .previous_frames
            .iter()
            .zip(frames)
            .take_while(|(left, right)| left == right)
            .count();
        receipt.donor_path_prefix_records_reused = receipt
            .donor_path_prefix_records_reused
            .checked_add(common)
            .ok_or(CommittedCheckpointIndexError::Overflow(
                "donor path prefix reuse receipt",
            ))?;
        receipt.maximum_donor_path_build_scratch_bytes =
            receipt.maximum_donor_path_build_scratch_bytes.max(
                self.previous_frames.capacity() * DONOR_FRAME_BYTES
                    + frames.len() * DONOR_FRAME_BYTES
                    + self.previous_nodes.capacity() * std::mem::size_of::<ArenaId>(),
            );
        self.previous_nodes.truncate(common);
        self.previous_nodes
            .try_reserve_exact(frames.len().saturating_sub(self.previous_nodes.len()))
            .map_err(|_| CommittedCheckpointIndexError::Allocation("donor path node cache"))?;
        receipt.maximum_donor_path_build_scratch_bytes =
            receipt.maximum_donor_path_build_scratch_bytes.max(
                self.previous_frames.capacity() * DONOR_FRAME_BYTES
                    + frames.len() * DONOR_FRAME_BYTES
                    + self.previous_nodes.capacity() * std::mem::size_of::<ArenaId>(),
            );

        let identical = common == frames.len() && common == self.previous_frames.len();
        let replacement_terminal = if identical {
            None
        } else if common == frames.len() {
            let prefix =
                *self
                    .previous_nodes
                    .last()
                    .ok_or(CommittedCheckpointIndexError::Corrupt(
                        "donor prefix path lost its terminal",
                    ))?;
            Some(session.retain(prefix)?)
        } else {
            let mut previous_owner: Option<ArenaBuildOwner> = None;
            let mut prior = self.previous_nodes.last().copied();
            for (index, frame) in frames.iter().copied().enumerate().skip(common) {
                let depth = u32::try_from(index + 1).map_err(|_| {
                    CommittedCheckpointIndexError::Invalid("donor path depth exceeds u32")
                })?;
                let payload = encode_donor_path_node(depth, frame);
                let (owner, allocation) = match prior {
                    Some(prior) => session.allocate_packed(&payload, &[prior])?,
                    None => session.allocate_packed(&payload, &[])?,
                };
                observe_allocation(receipt, allocation);
                receipt.donor_path_nodes_allocated =
                    receipt.donor_path_nodes_allocated.checked_add(1).ok_or(
                        CommittedCheckpointIndexError::Overflow("donor path node count"),
                    )?;
                receipt.donor_retained_payload_bytes = receipt
                    .donor_retained_payload_bytes
                    .checked_add(DONOR_PATH_NODE_BYTES)
                    .ok_or(CommittedCheckpointIndexError::Overflow(
                        "donor retained payload bytes",
                    ))?;
                if prior.is_some() {
                    receipt.donor_retained_edge_bytes =
                        receipt.donor_retained_edge_bytes.checked_add(8).ok_or(
                            CommittedCheckpointIndexError::Overflow("donor retained edge bytes"),
                        )?;
                }
                let id = session.owner_id(&owner)?;
                if let Some(previous_owner) = previous_owner.replace(owner) {
                    session.release(previous_owner)?;
                }
                self.previous_nodes.push(id);
                prior = Some(id);
            }
            Some(previous_owner.ok_or(CommittedCheckpointIndexError::Corrupt(
                "donor suffix allocation produced no terminal",
            ))?)
        };

        if let Some(replacement_terminal) = replacement_terminal
            && let Some(old_terminal) = self.terminal.replace(replacement_terminal)
        {
            session.release(old_terminal)?;
        }
        self.remember_frames(frames)?;
        let terminal = self
            .terminal
            .as_ref()
            .ok_or(CommittedCheckpointIndexError::Corrupt(
                "donor path cache has no terminal owner",
            ))?;
        let terminal_id = session.owner_id(terminal)?;
        session.retain(terminal_id).map_err(Into::into)
    }

    fn remember_frames(
        &mut self,
        frames: &[OpaqueDonorFrame],
    ) -> Result<(), CommittedCheckpointIndexError> {
        if self.previous_frames.capacity() < frames.len() {
            self.previous_frames
                .try_reserve_exact(frames.len().saturating_sub(self.previous_frames.len()))
                .map_err(|_| CommittedCheckpointIndexError::Allocation("donor path frame cache"))?;
        }
        self.previous_frames.clear();
        self.previous_frames.extend_from_slice(frames);
        Ok(())
    }

    fn release_terminal_if_present(
        &mut self,
        session: &mut ArenaBuildSession<'_>,
    ) -> Result<(), CommittedCheckpointIndexError> {
        if let Some(terminal) = self.terminal.take() {
            session.release(terminal)?;
        }
        Ok(())
    }
}

#[cfg(feature = "exact-parser")]
fn build_donor_sample_sequence(
    session: &mut ArenaBuildSession<'_>,
    samples: Vec<DonorCheckpointSampleDraft>,
    path_cache: &mut DonorPathCache,
    receipt: &mut CommittedCheckpointIndexBuildReceipt,
) -> Result<ArenaBuildOwner, CommittedCheckpointIndexError> {
    if samples.is_empty() {
        return Err(CommittedCheckpointIndexError::Invalid(
            "donor partition has no restart samples",
        ));
    }
    let partition_draft_bytes = samples.capacity()
        * std::mem::size_of::<DonorCheckpointSampleDraft>()
        + samples
            .iter()
            .map(|sample| sample.donor.draft_storage_bytes())
            .sum::<usize>();
    receipt.maximum_donor_partition_draft_bytes = receipt
        .maximum_donor_partition_draft_bytes
        .max(partition_draft_bytes);

    let mut sequence =
        ResumableStreamingSequenceBuilder::<DonorSampleSpec>::try_new(&mut receipt.sequence)?;
    let mut page = DonorSampleLeafEncoder::new()?;

    for sample in samples {
        if !page.can_fit() {
            flush_donor_sample_leaf(session, &mut sequence, &mut page, receipt)?;
        }
        require_nonempty_interval(sample.interval)?;
        let header = *sample.donor.header();
        let path_records = sample.donor.frames().len();
        receipt.donor_sample_headers = receipt.donor_sample_headers.checked_add(1).ok_or(
            CommittedCheckpointIndexError::Overflow("donor sample header count"),
        )?;
        receipt.donor_sample_header_bytes = receipt
            .donor_sample_header_bytes
            .checked_add(DONOR_HEADER_BYTES)
            .ok_or(CommittedCheckpointIndexError::Overflow(
                "donor sample header bytes",
            ))?;
        receipt.donor_materialized_path_records = receipt
            .donor_materialized_path_records
            .checked_add(path_records)
            .ok_or(CommittedCheckpointIndexError::Overflow(
                "donor materialized path records",
            ))?;
        receipt.donor_materialized_path_bytes = receipt
            .donor_materialized_path_bytes
            .checked_add(sample.donor.donor_materialized_path_bytes())
            .ok_or(CommittedCheckpointIndexError::Overflow(
                "donor materialized path bytes",
            ))?;
        receipt.maximum_donor_capture_conversion_scratch_bytes = receipt
            .maximum_donor_capture_conversion_scratch_bytes
            .max(sample.donor.conversion_scratch_bytes());
        receipt.retained_source_bytes = receipt
            .retained_source_bytes
            .checked_add(sample.donor.retained_source_bytes())
            .ok_or(CommittedCheckpointIndexError::Overflow(
                "donor retained source bytes",
            ))?;
        receipt.donor_retained_payload_bytes = receipt
            .donor_retained_payload_bytes
            .checked_add(DONOR_HEADER_BYTES)
            .ok_or(CommittedCheckpointIndexError::Overflow(
                "donor retained payload bytes",
            ))?;
        let path_owner = path_cache.sample_path_owner(session, sample.donor.frames(), receipt)?;
        receipt.donor_sample_path_edges = receipt.donor_sample_path_edges.checked_add(1).ok_or(
            CommittedCheckpointIndexError::Overflow("donor sample path edges"),
        )?;
        receipt.donor_retained_edge_bytes =
            receipt.donor_retained_edge_bytes.checked_add(8).ok_or(
                CommittedCheckpointIndexError::Overflow("donor retained edge bytes"),
            )?;
        page.push(sample.interval, &header, path_owner)?;
    }
    flush_donor_sample_leaf(session, &mut sequence, &mut page, receipt)?;
    finish_sequence(session, &mut sequence, &mut receipt.sequence)
}

#[cfg(feature = "exact-parser")]
fn build_donor_partition(
    session: &mut ArenaBuildSession<'_>,
    group: Option<(u64, StorageOnlyNormalizationOutcome)>,
    samples: Vec<DonorCheckpointSampleDraft>,
    path_cache: &mut DonorPathCache,
    receipt: &mut CommittedCheckpointIndexBuildReceipt,
) -> Result<(CheckpointIndexSummary, ArenaBuildOwner), CommittedCheckpointIndexError> {
    let summary = summarize_donor_samples(&samples)?;
    let sample_root = build_donor_sample_sequence(session, samples, path_cache, receipt)?;
    let sample_root_id = session.owner_id(&sample_root)?;
    let payload = encode_donor_partition_manifest(group, summary.measure, summary.samples)?;
    let (manifest, allocation) = session.allocate(&payload, &[sample_root_id])?;
    observe_allocation(receipt, allocation);
    receipt.donor_partition_manifests = receipt.donor_partition_manifests.checked_add(1).ok_or(
        CommittedCheckpointIndexError::Overflow("donor partition manifest count"),
    )?;
    session.release(sample_root)?;
    Ok((summary, manifest))
}

#[cfg(feature = "exact-parser")]
fn flush_donor_sample_leaf(
    session: &mut ArenaBuildSession<'_>,
    sequence: &mut ResumableStreamingSequenceBuilder<DonorSampleSpec>,
    page: &mut DonorSampleLeafEncoder,
    receipt: &mut CommittedCheckpointIndexBuildReceipt,
) -> Result<(), CommittedCheckpointIndexError> {
    if page.is_empty() {
        return Ok(());
    }
    let header = encode_summary(DONOR_SAMPLE_LEAF_TAG, page.summary);
    page.payload[..SUMMARY_BYTES].copy_from_slice(&header);
    let path_ids = page
        .path_owners
        .iter()
        .map(|owner| session.owner_id(owner))
        .collect::<Result<Vec<_>, _>>()?;
    let (leaf, allocation) = session.allocate_packed(&page.payload, &path_ids)?;
    observe_allocation(receipt, allocation);
    receipt.sample_leaf_pages =
        receipt
            .sample_leaf_pages
            .checked_add(1)
            .ok_or(CommittedCheckpointIndexError::Overflow(
                "donor sample leaf count",
            ))?;
    for owner in page.path_owners.drain(..) {
        session.release(owner)?;
    }
    sequence.begin_push(session, leaf, &mut receipt.sequence)?;
    while sequence.poll_push(session, &mut receipt.sequence)? == ResumableSequenceProgress::Pending
    {
    }
    page.reset();
    Ok(())
}

fn flush_sample_leaf(
    session: &mut ArenaBuildSession<'_>,
    sequence: &mut ResumableStreamingSequenceBuilder<NormalizationSampleSpec>,
    payload: &mut Vec<u8>,
    summary: CheckpointIndexSummary,
    receipt: &mut CommittedCheckpointIndexBuildReceipt,
) -> Result<(), CommittedCheckpointIndexError> {
    if summary.samples == 0 {
        return Ok(());
    }
    let header = encode_summary(SAMPLE_LEAF_TAG, summary);
    payload[..SUMMARY_BYTES].copy_from_slice(&header);
    let (leaf, allocation) = session.allocate(payload, &[])?;
    observe_allocation(receipt, allocation);
    receipt.sample_leaf_pages = receipt
        .sample_leaf_pages
        .checked_add(1)
        .ok_or(CommittedCheckpointIndexError::Overflow("sample leaf count"))?;
    sequence.begin_push(session, leaf, &mut receipt.sequence)?;
    while sequence.poll_push(session, &mut receipt.sequence)? == ResumableSequenceProgress::Pending
    {
    }
    payload.clear();
    payload.resize(SUMMARY_BYTES, 0);
    Ok(())
}

fn finish_sequence<Spec>(
    session: &mut ArenaBuildSession<'_>,
    sequence: &mut ResumableStreamingSequenceBuilder<Spec>,
    receipt: &mut SequenceMutationReceipt,
) -> Result<ArenaBuildOwner, CommittedCheckpointIndexError>
where
    Spec: SequenceSpec<Error = CommittedCheckpointIndexError>,
    CommittedCheckpointIndexError: From<ArenaBuildError>,
{
    sequence.begin_finish(receipt)?;
    while sequence.poll_finish(session, receipt)? == ResumableSequenceProgress::Pending {}
    sequence.take_root()
}

fn summarize_samples(
    samples: &[RelativeCheckpointMeasure],
) -> Result<CheckpointIndexSummary, CommittedCheckpointIndexError> {
    if samples.is_empty() {
        return Err(CommittedCheckpointIndexError::Invalid(
            "normalization group has no restart samples",
        ));
    }
    samples
        .iter()
        .copied()
        .try_fold(CheckpointIndexSummary::default(), |summary, interval| {
            require_nonempty_interval(interval)?;
            summary.followed_by(CheckpointIndexSummary {
                partitions: 1,
                samples: 1,
                measure: interval,
                ..CheckpointIndexSummary::default()
            })
        })
}

#[cfg(feature = "exact-parser")]
fn validate_donor_partition_shape(
    group: Option<(u64, StorageOnlyNormalizationOutcome)>,
    samples: &[DonorCheckpointSampleDraft],
) -> Result<(), CommittedCheckpointIndexError> {
    if samples.is_empty() {
        return Err(CommittedCheckpointIndexError::Invalid(
            "donor partition has no samples",
        ));
    }
    if let Some((group, outcome)) = group {
        if group == 0 {
            return Err(CommittedCheckpointIndexError::Invalid(
                "donor normalization group identity is zero",
            ));
        }
        outcome.encode()?;
    }
    Ok(())
}

#[cfg(feature = "exact-parser")]
fn donor_builder_draft_bytes(
    partition_capacity: usize,
    partitions: &[StorageOnlyCheckpointPartition],
) -> Result<usize, CommittedCheckpointIndexError> {
    let mut bytes = partition_capacity
        .checked_mul(std::mem::size_of::<StorageOnlyCheckpointPartition>())
        .ok_or(CommittedCheckpointIndexError::Overflow(
            "donor builder partition draft bytes",
        ))?;
    for partition in partitions {
        let StorageOnlyCheckpointPartition::Donor { samples, .. } = partition else {
            continue;
        };
        bytes = bytes
            .checked_add(
                samples
                    .capacity()
                    .checked_mul(std::mem::size_of::<DonorCheckpointSampleDraft>())
                    .ok_or(CommittedCheckpointIndexError::Overflow(
                        "donor sample draft descriptor bytes",
                    ))?,
            )
            .ok_or(CommittedCheckpointIndexError::Overflow(
                "donor builder queued draft bytes",
            ))?;
        for sample in samples {
            bytes = bytes
                .checked_add(sample.donor.draft_storage_bytes())
                .ok_or(CommittedCheckpointIndexError::Overflow(
                    "donor builder queued path draft bytes",
                ))?;
        }
    }
    Ok(bytes)
}

#[cfg(feature = "exact-parser")]
fn initialize_donor_builder_receipt(
    receipt: &mut CommittedCheckpointIndexBuildReceipt,
    partition_capacity: usize,
    partitions: &[StorageOnlyCheckpointPartition],
) -> Result<(), CommittedCheckpointIndexError> {
    receipt.donor_builder_queued_draft_bytes =
        donor_builder_draft_bytes(partition_capacity, partitions)?;
    Ok(())
}

#[cfg(feature = "exact-parser")]
fn summarize_donor_samples(
    samples: &[DonorCheckpointSampleDraft],
) -> Result<CheckpointIndexSummary, CommittedCheckpointIndexError> {
    samples
        .iter()
        .try_fold(CheckpointIndexSummary::default(), |summary, sample| {
            require_nonempty_interval(sample.interval)?;
            summary.followed_by(CheckpointIndexSummary {
                partitions: 1,
                samples: 1,
                measure: sample.interval,
                ..CheckpointIndexSummary::default()
            })
        })
}

fn empty_leaf_summary() -> CheckpointIndexSummary {
    CheckpointIndexSummary {
        leaf_pages: 1,
        height: 1,
        ..CheckpointIndexSummary::default()
    }
}

fn require_nonempty_interval(
    interval: RelativeCheckpointMeasure,
) -> Result<(), CommittedCheckpointIndexError> {
    if interval.source_bytes == 0 || interval.source_utf16 == 0 {
        return Err(CommittedCheckpointIndexError::Invalid(
            "storage milestone does not yet represent zero-source intervals",
        ));
    }
    Ok(())
}

fn require_terminal_tail_interval(
    interval: RelativeCheckpointMeasure,
) -> Result<(), CommittedCheckpointIndexError> {
    if interval.source_bytes != 0
        || interval.source_utf16 != 0
        || interval.physical_lines != 0
        || interval.green_events == 0 && interval.projection_runs == 0
        || interval.projection_runs > interval.green_events
    {
        return Err(CommittedCheckpointIndexError::Invalid(
            "terminal tail must contain only nonempty semantic output progress",
        ));
    }
    Ok(())
}

fn locate_outer_partition(
    arena: &PageArena,
    root: ArenaId,
    source_byte: u64,
) -> Result<LocatedCheckpointPartition, CommittedCheckpointIndexError> {
    let root_summary = sequence_node::<CheckpointIndexSpec>(arena, root)?.0;
    if source_byte >= root_summary.measure.source_bytes {
        return Err(CommittedCheckpointIndexError::SourceOutOfBounds);
    }
    let index_root = root;
    let mut node = root;
    let mut prefix = CheckpointIndexSummary::default();
    loop {
        match sequence_node::<CheckpointIndexSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => {
                let records = decode_outer_leaf_records_in_arena(arena, node)?;
                for record in records {
                    let end = checked_add(
                        prefix.measure.source_bytes,
                        record.interval.source_bytes,
                        "lookup source bytes",
                    )?;
                    if source_byte < end {
                        let kind = match record.kind {
                            DecodedPartitionKind::Direct => LocatedCheckpointPartitionKind::Direct,
                            DecodedPartitionKind::TerminalTail => {
                                LocatedCheckpointPartitionKind::TerminalTail
                            }
                            DecodedPartitionKind::Normalization { child_ordinal } => {
                                let manifest =
                                    arena.packed_child_at(node, usize::from(child_ordinal))?;
                                let decoded = decode_normalization_manifest(arena, manifest)?;
                                if decoded.measure != record.interval
                                    || decoded.samples != record.samples
                                {
                                    return Err(CommittedCheckpointIndexError::Corrupt(
                                        "normalization record and manifest disagree",
                                    ));
                                }
                                LocatedCheckpointPartitionKind::Normalization(
                                    LocatedNormalizationGroup {
                                        index_root,
                                        group: decoded.group,
                                        manifest,
                                        interval: record.interval,
                                        samples: record.samples,
                                    },
                                )
                            }
                            #[cfg(feature = "exact-parser")]
                            DecodedPartitionKind::Donor { child_ordinal } => {
                                let manifest =
                                    arena.packed_child_at(node, usize::from(child_ordinal))?;
                                let decoded = decode_donor_partition_manifest(arena, manifest)?;
                                if decoded.measure != record.interval
                                    || decoded.samples != record.samples
                                {
                                    return Err(CommittedCheckpointIndexError::Corrupt(
                                        "donor record and manifest disagree",
                                    ));
                                }
                                LocatedCheckpointPartitionKind::Donor(LocatedDonorPartition {
                                    index_root,
                                    manifest,
                                    interval: record.interval,
                                    samples: record.samples,
                                })
                            }
                        };
                        return Ok(LocatedCheckpointPartition {
                            ordinal: prefix.partitions,
                            prefix: prefix.measure,
                            interval: record.interval,
                            kind,
                        });
                    }
                    prefix = prefix.followed_by(CheckpointIndexSummary {
                        partitions: 1,
                        samples: record.samples,
                        measure: record.interval,
                        ..CheckpointIndexSummary::default()
                    })?;
                }
                return Err(CommittedCheckpointIndexError::Corrupt(
                    "outer leaf did not cover the selected byte",
                ));
            }
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<CheckpointIndexSpec>(arena, left)?.0;
                let left_end = checked_add(
                    prefix.measure.source_bytes,
                    left_summary.measure.source_bytes,
                    "lookup branch source bytes",
                )?;
                if source_byte < left_end {
                    node = left;
                } else {
                    prefix = prefix.followed_by(left_summary)?;
                    node = right;
                }
            }
        }
    }
}

/// Ordinal sibling of `locate_outer_partition`. Convergence uses this path at
/// a donor partition end because the following terminal semantic tail has a
/// zero-width source interval and therefore cannot be addressed by byte cut.
#[cfg(feature = "exact-parser")]
fn locate_outer_partition_by_ordinal(
    arena: &PageArena,
    root: ArenaId,
    ordinal: u64,
) -> Result<LocatedCheckpointPartition, CommittedCheckpointIndexError> {
    let root_summary = sequence_node::<CheckpointIndexSpec>(arena, root)?.0;
    if ordinal >= root_summary.partitions {
        return Err(CommittedCheckpointIndexError::SourceOutOfBounds);
    }
    let index_root = root;
    let mut node = root;
    let mut prefix = CheckpointIndexSummary::default();
    loop {
        match sequence_node::<CheckpointIndexSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => {
                let records = decode_outer_leaf_records_in_arena(arena, node)?;
                for record in records {
                    if prefix.partitions == ordinal {
                        let kind = match record.kind {
                            DecodedPartitionKind::Direct => LocatedCheckpointPartitionKind::Direct,
                            DecodedPartitionKind::TerminalTail => {
                                LocatedCheckpointPartitionKind::TerminalTail
                            }
                            DecodedPartitionKind::Normalization { child_ordinal } => {
                                let manifest =
                                    arena.packed_child_at(node, usize::from(child_ordinal))?;
                                let decoded = decode_normalization_manifest(arena, manifest)?;
                                if decoded.measure != record.interval
                                    || decoded.samples != record.samples
                                {
                                    return Err(CommittedCheckpointIndexError::Corrupt(
                                        "normalization record and manifest disagree",
                                    ));
                                }
                                LocatedCheckpointPartitionKind::Normalization(
                                    LocatedNormalizationGroup {
                                        index_root,
                                        group: decoded.group,
                                        manifest,
                                        interval: record.interval,
                                        samples: record.samples,
                                    },
                                )
                            }
                            DecodedPartitionKind::Donor { child_ordinal } => {
                                let manifest =
                                    arena.packed_child_at(node, usize::from(child_ordinal))?;
                                let decoded = decode_donor_partition_manifest(arena, manifest)?;
                                if decoded.measure != record.interval
                                    || decoded.samples != record.samples
                                {
                                    return Err(CommittedCheckpointIndexError::Corrupt(
                                        "donor record and manifest disagree",
                                    ));
                                }
                                LocatedCheckpointPartitionKind::Donor(LocatedDonorPartition {
                                    index_root,
                                    manifest,
                                    interval: record.interval,
                                    samples: record.samples,
                                })
                            }
                        };
                        return Ok(LocatedCheckpointPartition {
                            ordinal,
                            prefix: prefix.measure,
                            interval: record.interval,
                            kind,
                        });
                    }
                    prefix = prefix.followed_by(CheckpointIndexSummary {
                        partitions: 1,
                        samples: record.samples,
                        measure: record.interval,
                        terminal_tail: matches!(record.kind, DecodedPartitionKind::TerminalTail),
                        ..CheckpointIndexSummary::default()
                    })?;
                }
                return Err(CommittedCheckpointIndexError::Corrupt(
                    "outer leaf did not contain selected partition ordinal",
                ));
            }
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<CheckpointIndexSpec>(arena, left)?.0;
                let left_end = prefix
                    .partitions
                    .checked_add(left_summary.partitions)
                    .ok_or(CommittedCheckpointIndexError::Overflow(
                        "outer ordinal lookup partitions",
                    ))?;
                if ordinal < left_end {
                    node = left;
                } else {
                    prefix = prefix.followed_by(left_summary)?;
                    node = right;
                }
            }
        }
    }
}

fn locate_sample(
    arena: &PageArena,
    root: ArenaId,
    relative_source_byte: u64,
) -> Result<LocatedNormalizationSample, CommittedCheckpointIndexError> {
    let root_summary = sequence_node::<NormalizationSampleSpec>(arena, root)?.0;
    if relative_source_byte >= root_summary.measure.source_bytes {
        return Err(CommittedCheckpointIndexError::SourceOutOfBounds);
    }
    let mut node = root;
    let mut prefix = CheckpointIndexSummary::default();
    loop {
        match sequence_node::<NormalizationSampleSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => {
                for interval in decode_sample_leaf_records(arena.payload(node)?)? {
                    let end = checked_add(
                        prefix.measure.source_bytes,
                        interval.source_bytes,
                        "sample lookup source bytes",
                    )?;
                    if relative_source_byte < end {
                        return Ok(LocatedNormalizationSample {
                            ordinal: prefix.samples,
                            prefix: prefix.measure,
                            interval,
                        });
                    }
                    prefix = prefix.followed_by(CheckpointIndexSummary {
                        partitions: 1,
                        samples: 1,
                        measure: interval,
                        ..CheckpointIndexSummary::default()
                    })?;
                }
                return Err(CommittedCheckpointIndexError::Corrupt(
                    "sample leaf did not cover the selected byte",
                ));
            }
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<NormalizationSampleSpec>(arena, left)?.0;
                let left_end = checked_add(
                    prefix.measure.source_bytes,
                    left_summary.measure.source_bytes,
                    "sample lookup branch bytes",
                )?;
                if relative_source_byte < left_end {
                    node = left;
                } else {
                    prefix = prefix.followed_by(left_summary)?;
                    node = right;
                }
            }
        }
    }
}

#[cfg(feature = "exact-parser")]
fn decode_donor_sample_leaf_in_arena(
    arena: &PageArena,
    leaf: ArenaId,
) -> Result<Vec<DecodedDonorSampleRecord>, CommittedCheckpointIndexError> {
    let records = decode_donor_sample_leaf_payload(arena.payload(leaf)?)?;
    if arena.packed_child_count(leaf)? != records.len() {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor sample records and path children disagree",
        ));
    }
    Ok(records)
}

#[cfg(feature = "exact-parser")]
fn locate_donor_sample_containing(
    arena: &PageArena,
    root: ArenaId,
    relative_source_byte: u64,
) -> Result<LocatedOpaqueDonorSample, CommittedCheckpointIndexError> {
    let root_summary = sequence_node::<DonorSampleSpec>(arena, root)?.0;
    if relative_source_byte >= root_summary.measure.source_bytes {
        return Err(CommittedCheckpointIndexError::SourceOutOfBounds);
    }
    let mut node = root;
    let mut prefix = CheckpointIndexSummary::default();
    loop {
        match sequence_node::<DonorSampleSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => {
                for (index, record) in decode_donor_sample_leaf_in_arena(arena, node)?
                    .into_iter()
                    .enumerate()
                {
                    let end = checked_add(
                        prefix.measure.source_bytes,
                        record.interval.source_bytes,
                        "donor sample lookup source bytes",
                    )?;
                    if relative_source_byte < end {
                        return Ok(LocatedOpaqueDonorSample {
                            ordinal: prefix.samples,
                            prefix: prefix.measure,
                            interval: record.interval,
                            header: record.header,
                            path_terminal: arena.packed_child_at(node, index)?,
                        });
                    }
                    prefix = prefix.followed_by(CheckpointIndexSummary {
                        partitions: 1,
                        samples: 1,
                        measure: record.interval,
                        ..CheckpointIndexSummary::default()
                    })?;
                }
                return Err(CommittedCheckpointIndexError::Corrupt(
                    "donor sample leaf did not cover the selected byte",
                ));
            }
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<DonorSampleSpec>(arena, left)?.0;
                let left_end = checked_add(
                    prefix.measure.source_bytes,
                    left_summary.measure.source_bytes,
                    "donor sample lookup branch bytes",
                )?;
                if relative_source_byte < left_end {
                    node = left;
                } else {
                    prefix = prefix.followed_by(left_summary)?;
                    node = right;
                }
            }
        }
    }
}

#[cfg(feature = "exact-parser")]
fn locate_donor_sample_by_ordinal(
    arena: &PageArena,
    root: ArenaId,
    ordinal: u64,
) -> Result<LocatedOpaqueDonorSample, CommittedCheckpointIndexError> {
    Ok(locate_donor_sample_by_ordinal_with_receipt(arena, root, ordinal)?.sample)
}

#[cfg(feature = "exact-parser")]
fn locate_donor_sample_by_ordinal_with_receipt(
    arena: &PageArena,
    root: ArenaId,
    ordinal: u64,
) -> Result<LocatedDonorSampleByOrdinalReceipt, CommittedCheckpointIndexError> {
    let root_summary = sequence_node::<DonorSampleSpec>(arena, root)?.0;
    if ordinal >= root_summary.samples {
        return Err(CommittedCheckpointIndexError::SourceOutOfBounds);
    }
    let mut node = root;
    let mut prefix = CheckpointIndexSummary::default();
    let mut nodes_visited = 0_usize;
    loop {
        nodes_visited =
            nodes_visited
                .checked_add(1)
                .ok_or(CommittedCheckpointIndexError::Overflow(
                    "donor ordinal lookup nodes visited",
                ))?;
        match sequence_node::<DonorSampleSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => {
                let records = decode_donor_sample_leaf_in_arena(arena, node)?;
                let maximum_temporary_bytes = records
                    .capacity()
                    .checked_mul(std::mem::size_of::<DecodedDonorSampleRecord>())
                    .ok_or(CommittedCheckpointIndexError::Overflow(
                        "donor ordinal lookup temporary bytes",
                    ))?;
                let index = usize::try_from(ordinal - prefix.samples).map_err(|_| {
                    CommittedCheckpointIndexError::Corrupt(
                        "donor sample ordinal exceeds leaf address space",
                    )
                })?;
                let record =
                    records
                        .get(index)
                        .copied()
                        .ok_or(CommittedCheckpointIndexError::Corrupt(
                            "donor sample ordinal is absent from selected leaf",
                        ))?;
                for prior in &records[..index] {
                    prefix = prefix.followed_by(CheckpointIndexSummary {
                        partitions: 1,
                        samples: 1,
                        measure: prior.interval,
                        ..CheckpointIndexSummary::default()
                    })?;
                }
                return Ok(LocatedDonorSampleByOrdinalReceipt {
                    sample: LocatedOpaqueDonorSample {
                        ordinal,
                        prefix: prefix.measure,
                        interval: record.interval,
                        header: record.header,
                        path_terminal: arena.packed_child_at(node, index)?,
                    },
                    tree_height: root_summary.height,
                    nodes_visited,
                    maximum_temporary_bytes,
                });
            }
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<DonorSampleSpec>(arena, left)?.0;
                let left_end = prefix.samples.checked_add(left_summary.samples).ok_or(
                    CommittedCheckpointIndexError::Overflow("donor sample lookup ordinal"),
                )?;
                if ordinal < left_end {
                    node = left;
                } else {
                    prefix = prefix.followed_by(left_summary)?;
                    node = right;
                }
            }
        }
    }
}

#[cfg(feature = "exact-parser")]
fn locate_donor_sample_predecessor(
    arena: &PageArena,
    root: ArenaId,
    relative_source_cut: u64,
) -> Result<Option<LocatedOpaqueDonorSample>, CommittedCheckpointIndexError> {
    let summary = sequence_node::<DonorSampleSpec>(arena, root)?.0;
    if relative_source_cut > summary.measure.source_bytes {
        return Err(CommittedCheckpointIndexError::SourceOutOfBounds);
    }
    if relative_source_cut == 0 {
        return Ok(None);
    }
    let containing = locate_donor_sample_containing(arena, root, relative_source_cut - 1)?;
    let containing_end = containing
        .prefix
        .source_bytes
        .checked_add(containing.interval.source_bytes)
        .ok_or(CommittedCheckpointIndexError::Overflow(
            "donor sample predecessor end",
        ))?;
    if containing_end <= relative_source_cut {
        Ok(Some(containing))
    } else if containing.ordinal == 0 {
        Ok(None)
    } else {
        locate_donor_sample_by_ordinal(arena, root, containing.ordinal - 1).map(Some)
    }
}

fn encode_summary(tag: u8, summary: CheckpointIndexSummary) -> [u8; SUMMARY_BYTES] {
    let mut output = [0_u8; SUMMARY_BYTES];
    output[0] = tag;
    output[1] = FORMAT_VERSION;
    output[2..4].copy_from_slice(&summary.height.to_le_bytes());
    output[8..16].copy_from_slice(&summary.leaf_pages.to_le_bytes());
    output[16..24].copy_from_slice(&summary.partitions.to_le_bytes());
    output[24..32].copy_from_slice(&summary.samples.to_le_bytes());
    encode_measure_into(&mut output[32..72], summary.measure);
    output[72] = u8::from(summary.terminal_tail);
    output
}

fn decode_summary(
    payload: &[u8],
    expected_tag: u8,
) -> Result<CheckpointIndexSummary, CommittedCheckpointIndexError> {
    if payload.len() < SUMMARY_BYTES
        || payload[0] != expected_tag
        || payload[1] != FORMAT_VERSION
        || payload[4..8] != [0; 4]
        || payload[72] > 1
        || payload[73..80] != [0; 7]
    {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "invalid sequence summary header",
        ));
    }
    let summary = CheckpointIndexSummary {
        height: read_u16(&payload[2..4]),
        leaf_pages: read_u64(&payload[8..16]),
        partitions: read_u64(&payload[16..24]),
        samples: read_u64(&payload[24..32]),
        measure: decode_measure(&payload[32..72]),
        terminal_tail: payload[72] == 1,
    };
    if summary.terminal_tail && !is_index_sequence_tag(expected_tag)
        || summary.leaf_pages == 0
        || summary.partitions == 0
        || summary.samples == 0 && !summary.terminal_tail
        || summary.height == 0
        || (summary.measure.source_bytes == 0 || summary.measure.source_utf16 == 0)
            && !summary.terminal_tail
        || is_sequence_leaf_tag(expected_tag) && (summary.leaf_pages != 1 || summary.height != 1)
        || is_sequence_branch_tag(expected_tag)
            && (summary.leaf_pages < 2 || summary.height < 2 || payload.len() != SUMMARY_BYTES)
    {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "invalid sequence summary values",
        ));
    }
    Ok(summary)
}

fn is_index_sequence_tag(tag: u8) -> bool {
    tag == INDEX_LEAF_TAG || tag == INDEX_BRANCH_TAG
}

fn is_sequence_leaf_tag(tag: u8) -> bool {
    tag == INDEX_LEAF_TAG
        || tag == SAMPLE_LEAF_TAG
        || cfg!(feature = "exact-parser") && tag == exact_donor_sample_leaf_tag()
}

fn is_sequence_branch_tag(tag: u8) -> bool {
    tag == INDEX_BRANCH_TAG
        || tag == SAMPLE_BRANCH_TAG
        || cfg!(feature = "exact-parser") && tag == exact_donor_sample_branch_tag()
}

#[cfg(feature = "exact-parser")]
const fn exact_donor_sample_leaf_tag() -> u8 {
    DONOR_SAMPLE_LEAF_TAG
}

#[cfg(not(feature = "exact-parser"))]
const fn exact_donor_sample_leaf_tag() -> u8 {
    u8::MAX
}

#[cfg(feature = "exact-parser")]
const fn exact_donor_sample_branch_tag() -> u8 {
    DONOR_SAMPLE_BRANCH_TAG
}

#[cfg(not(feature = "exact-parser"))]
const fn exact_donor_sample_branch_tag() -> u8 {
    u8::MAX
}

fn encode_partition_record(
    output: &mut Vec<u8>,
    tag: u8,
    child_ordinal: u16,
    interval: RelativeCheckpointMeasure,
    samples: u64,
) {
    output.push(tag);
    output.push(0);
    output.extend_from_slice(&child_ordinal.to_le_bytes());
    output.extend_from_slice(&[0; 4]);
    encode_measure(output, interval);
    output.extend_from_slice(&samples.to_le_bytes());
}

fn decode_outer_leaf_records(
    payload: &[u8],
) -> Result<Vec<DecodedPartitionRecord>, CommittedCheckpointIndexError> {
    if payload.len() < SUMMARY_BYTES
        || !(payload.len() - SUMMARY_BYTES).is_multiple_of(PARTITION_RECORD_BYTES)
    {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "outer leaf records are misaligned",
        ));
    }
    let mut records = Vec::new();
    records
        .try_reserve_exact((payload.len() - SUMMARY_BYTES) / PARTITION_RECORD_BYTES)
        .map_err(|_| CommittedCheckpointIndexError::Allocation("decoded outer records"))?;
    for record in payload[SUMMARY_BYTES..].chunks_exact(PARTITION_RECORD_BYTES) {
        if record[1] != 0 || record[4..8] != [0; 4] {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "outer leaf record reserved bytes are nonzero",
            ));
        }
        let child = read_u16(&record[2..4]);
        let kind = match (record[0], child) {
            (DIRECT_PARTITION_TAG, NO_CHILD_ORDINAL) => DecodedPartitionKind::Direct,
            (TERMINAL_TAIL_PARTITION_TAG, NO_CHILD_ORDINAL) => DecodedPartitionKind::TerminalTail,
            (NORMALIZATION_PARTITION_TAG, ordinal) if ordinal != NO_CHILD_ORDINAL => {
                DecodedPartitionKind::Normalization {
                    child_ordinal: ordinal,
                }
            }
            #[cfg(feature = "exact-parser")]
            (DONOR_PARTITION_TAG, ordinal) if ordinal != NO_CHILD_ORDINAL => {
                DecodedPartitionKind::Donor {
                    child_ordinal: ordinal,
                }
            }
            _ => {
                return Err(CommittedCheckpointIndexError::Corrupt(
                    "outer partition tag and child role disagree",
                ));
            }
        };
        let interval = decode_measure(&record[8..48]);
        let samples = read_u64(&record[48..56]);
        match kind {
            DecodedPartitionKind::TerminalTail => require_terminal_tail_interval(interval)
                .map_err(|_| CommittedCheckpointIndexError::Corrupt("invalid terminal tail"))?,
            _ => require_nonempty_interval(interval)
                .map_err(|_| CommittedCheckpointIndexError::Corrupt("empty outer interval"))?,
        }
        if matches!(kind, DecodedPartitionKind::TerminalTail) && samples != 0
            || !matches!(kind, DecodedPartitionKind::TerminalTail)
                && (samples == 0 || matches!(kind, DecodedPartitionKind::Direct) && samples != 1)
        {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "outer partition sample count is invalid",
            ));
        }
        records.push(DecodedPartitionRecord {
            kind,
            interval,
            samples,
        });
    }
    if records.is_empty() {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "outer leaf has no records",
        ));
    }
    Ok(records)
}

fn decode_outer_leaf_records_in_arena(
    arena: &PageArena,
    leaf: ArenaId,
) -> Result<Vec<DecodedPartitionRecord>, CommittedCheckpointIndexError> {
    let records = decode_outer_leaf_records(arena.payload(leaf)?)?;
    let mut expected_child = 0_u16;
    for record in &records {
        let child = match record.kind {
            DecodedPartitionKind::Direct | DecodedPartitionKind::TerminalTail => continue,
            DecodedPartitionKind::Normalization { child_ordinal } => child_ordinal,
            #[cfg(feature = "exact-parser")]
            DecodedPartitionKind::Donor { child_ordinal } => child_ordinal,
        };
        if child != expected_child {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "outer partition child ordinals are not canonical",
            ));
        }
        expected_child =
            expected_child
                .checked_add(1)
                .ok_or(CommittedCheckpointIndexError::Corrupt(
                    "outer child ordinal overflow",
                ))?;
    }
    if arena.packed_child_count(leaf)? != usize::from(expected_child) {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "outer partition records and child edges disagree",
        ));
    }
    Ok(records)
}

fn decode_sample_leaf_records(
    payload: &[u8],
) -> Result<Vec<RelativeCheckpointMeasure>, CommittedCheckpointIndexError> {
    if payload.len() < SUMMARY_BYTES
        || !(payload.len() - SUMMARY_BYTES).is_multiple_of(SAMPLE_RECORD_BYTES)
    {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "sample leaf records are misaligned",
        ));
    }
    let mut records = Vec::new();
    records
        .try_reserve_exact((payload.len() - SUMMARY_BYTES) / SAMPLE_RECORD_BYTES)
        .map_err(|_| CommittedCheckpointIndexError::Allocation("decoded sample records"))?;
    for record in payload[SUMMARY_BYTES..].chunks_exact(SAMPLE_RECORD_BYTES) {
        let interval = decode_measure(record);
        require_nonempty_interval(interval)
            .map_err(|_| CommittedCheckpointIndexError::Corrupt("empty sample interval"))?;
        records.push(interval);
    }
    if records.is_empty() {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "sample leaf has no records",
        ));
    }
    Ok(records)
}

#[cfg(feature = "exact-parser")]
fn decode_donor_sample_leaf_payload(
    payload: &[u8],
) -> Result<Vec<DecodedDonorSampleRecord>, CommittedCheckpointIndexError> {
    if payload.len() < SUMMARY_BYTES
        || !(payload.len() - SUMMARY_BYTES).is_multiple_of(DONOR_SAMPLE_RECORD_BYTES)
    {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor sample leaf records are misaligned",
        ));
    }
    let mut records = Vec::new();
    records
        .try_reserve_exact((payload.len() - SUMMARY_BYTES) / DONOR_SAMPLE_RECORD_BYTES)
        .map_err(|_| CommittedCheckpointIndexError::Allocation("decoded donor samples"))?;
    for record in payload[SUMMARY_BYTES..].chunks_exact(DONOR_SAMPLE_RECORD_BYTES) {
        let interval = decode_measure(&record[..SAMPLE_RECORD_BYTES]);
        require_nonempty_interval(interval)
            .map_err(|_| CommittedCheckpointIndexError::Corrupt("empty donor sample interval"))?;
        let header: OpaqueDonorHeader = record[SAMPLE_RECORD_BYTES..]
            .try_into()
            .map_err(|_| CommittedCheckpointIndexError::Corrupt("donor header has wrong size"))?;
        records.push(DecodedDonorSampleRecord { interval, header });
    }
    if records.is_empty() {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor sample leaf has no records",
        ));
    }
    Ok(records)
}

#[cfg(feature = "exact-parser")]
fn encode_donor_path_node(depth: u32, frame: OpaqueDonorFrame) -> [u8; DONOR_PATH_NODE_BYTES] {
    let mut payload = [0_u8; DONOR_PATH_NODE_BYTES];
    payload[0] = DONOR_PATH_NODE_TAG;
    payload[1] = FORMAT_VERSION;
    payload[4..8].copy_from_slice(&depth.to_le_bytes());
    payload[8..].copy_from_slice(&frame);
    payload
}

#[cfg(feature = "exact-parser")]
fn decode_donor_path_node(
    arena: &PageArena,
    node: ArenaId,
) -> Result<(u32, OpaqueDonorFrame, Option<ArenaId>), CommittedCheckpointIndexError> {
    let payload = arena.payload(node)?;
    if payload.len() != DONOR_PATH_NODE_BYTES
        || payload[0] != DONOR_PATH_NODE_TAG
        || payload[1] != FORMAT_VERSION
        || payload[2..4] != [0; 2]
    {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "invalid opaque donor path node",
        ));
    }
    let depth = read_u32(&payload[4..8]);
    if depth == 0 {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "opaque donor path depth is zero",
        ));
    }
    let child_count = arena.packed_child_count(node)?;
    let prior = match (depth, child_count) {
        (1, 0) => None,
        (2.., 1) => Some(arena.packed_child_at(node, 0)?),
        _ => {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "opaque donor path depth and child role disagree",
            ));
        }
    };
    let frame: OpaqueDonorFrame = payload[8..]
        .try_into()
        .map_err(|_| CommittedCheckpointIndexError::Corrupt("donor frame has wrong size"))?;
    Ok((depth, frame, prior))
}

#[cfg(feature = "exact-parser")]
fn reconstruct_donor_path(
    arena: &PageArena,
    terminal: ArenaId,
) -> Result<ReconstructedDonorPath, CommittedCheckpointIndexError> {
    let (depth, _, _) = decode_donor_path_node(arena, terminal)?;
    let depth = usize::try_from(depth)
        .map_err(|_| CommittedCheckpointIndexError::Corrupt("donor path depth exceeds usize"))?;
    if depth > arena.metrics().live_nodes {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor path depth exceeds live arena nodes",
        ));
    }

    // First validate the complete decreasing-depth chain. A forged terminal
    // depth therefore cannot drive a large allocation before topology proof.
    let mut node = Some(terminal);
    let mut expected_depth = depth;
    let mut validation_visits = 0_usize;
    while let Some(current) = node {
        let (actual_depth, _, prior) = decode_donor_path_node(arena, current)?;
        validation_visits =
            validation_visits
                .checked_add(1)
                .ok_or(CommittedCheckpointIndexError::Overflow(
                    "donor path validation visits",
                ))?;
        if usize::try_from(actual_depth).ok() != Some(expected_depth) {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "donor path depth chain is discontinuous",
            ));
        }
        node = prior;
        if node.is_some() {
            expected_depth =
                expected_depth
                    .checked_sub(1)
                    .ok_or(CommittedCheckpointIndexError::Corrupt(
                        "donor path depth underflow",
                    ))?;
        }
    }
    if validation_visits != depth || expected_depth != 1 {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor path terminated at the wrong depth",
        ));
    }

    let mut reversed = Vec::new();
    reversed
        .try_reserve_exact(depth)
        .map_err(|_| CommittedCheckpointIndexError::Allocation("donor path reconstruction"))?;
    let mut node = Some(terminal);
    let mut materialization_visits = 0_usize;
    while let Some(current) = node {
        let (_, frame, prior) = decode_donor_path_node(arena, current)?;
        reversed.push(frame);
        materialization_visits = materialization_visits.checked_add(1).ok_or(
            CommittedCheckpointIndexError::Overflow("donor path materialization visits"),
        )?;
        node = prior;
    }
    if reversed.len() != depth {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor path changed between validation and materialization",
        ));
    }
    reversed.reverse();
    Ok(ReconstructedDonorPath {
        scratch_storage_bytes: reversed.capacity().checked_mul(DONOR_FRAME_BYTES).ok_or(
            CommittedCheckpointIndexError::Overflow("donor path reconstruction scratch"),
        )?,
        frames: reversed,
        nodes_visited: validation_visits
            .checked_add(materialization_visits)
            .ok_or(CommittedCheckpointIndexError::Overflow(
                "donor path total visits",
            ))?,
    })
}

fn encode_normalization_manifest(
    group: u64,
    outcome: StorageOnlyNormalizationOutcome,
    measure: RelativeCheckpointMeasure,
    samples: u64,
) -> Result<[u8; MANIFEST_BYTES], CommittedCheckpointIndexError> {
    let (outcome_tag, outcome_detail) = outcome.encode()?;
    let mut output = [0_u8; MANIFEST_BYTES];
    output[0] = NORMALIZATION_MANIFEST_TAG;
    output[1] = FORMAT_VERSION;
    output[2] = outcome_tag;
    output[3] = outcome_detail;
    output[8..16].copy_from_slice(&group.to_le_bytes());
    encode_measure_into(&mut output[16..56], measure);
    output[56..64].copy_from_slice(&samples.to_le_bytes());
    // The sample root is child zero in PageArena. No ArenaId is serialized.
    Ok(output)
}

fn decode_normalization_manifest(
    arena: &PageArena,
    id: ArenaId,
) -> Result<DecodedNormalizationManifest, CommittedCheckpointIndexError> {
    let payload = arena.payload(id)?;
    if payload.len() != MANIFEST_BYTES
        || payload[0] != NORMALIZATION_MANIFEST_TAG
        || payload[1] != FORMAT_VERSION
        || payload[4..8] != [0; 4]
        || payload[64..80] != [0; 16]
        || arena.packed_child_count(id)? != 1
    {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "invalid normalization manifest",
        ));
    }
    let group = read_u64(&payload[8..16]);
    let measure = decode_measure(&payload[16..56]);
    let samples = read_u64(&payload[56..64]);
    if group == 0 || samples == 0 {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "normalization manifest identity or sample count is invalid",
        ));
    }
    let sample_root = arena.packed_child_at(id, 0)?;
    let sample_summary = sequence_node::<NormalizationSampleSpec>(arena, sample_root)?.0;
    if sample_summary.measure != measure || sample_summary.samples != samples {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "normalization manifest and sample tree disagree",
        ));
    }
    Ok(DecodedNormalizationManifest {
        group,
        outcome: StorageOnlyNormalizationOutcome::decode(payload[2], payload[3])?,
        measure,
        samples,
        sample_root,
    })
}

#[cfg(feature = "exact-parser")]
fn encode_donor_partition_manifest(
    group: Option<(u64, StorageOnlyNormalizationOutcome)>,
    measure: RelativeCheckpointMeasure,
    samples: u64,
) -> Result<[u8; MANIFEST_BYTES], CommittedCheckpointIndexError> {
    let (role, outcome_tag, outcome_detail, group) = match group {
        None => (DONOR_DIRECT_ROLE_TAG, 0, 0, 0),
        Some((group, outcome)) if group != 0 => {
            let (outcome_tag, outcome_detail) = outcome.encode()?;
            (
                DONOR_NORMALIZATION_ROLE_TAG,
                outcome_tag,
                outcome_detail,
                group,
            )
        }
        Some(_) => {
            return Err(CommittedCheckpointIndexError::Invalid(
                "donor normalization group identity is zero",
            ));
        }
    };
    let mut output = [0_u8; MANIFEST_BYTES];
    output[0] = DONOR_PARTITION_MANIFEST_TAG;
    output[1] = FORMAT_VERSION;
    output[2] = role;
    output[3] = outcome_tag;
    output[4] = outcome_detail;
    output[8..16].copy_from_slice(&group.to_le_bytes());
    encode_measure_into(&mut output[16..56], measure);
    output[56..64].copy_from_slice(&samples.to_le_bytes());
    Ok(output)
}

#[cfg(feature = "exact-parser")]
fn decode_donor_partition_manifest(
    arena: &PageArena,
    id: ArenaId,
) -> Result<DecodedDonorPartitionManifest, CommittedCheckpointIndexError> {
    let payload = arena.payload(id)?;
    if payload.len() != MANIFEST_BYTES
        || payload[0] != DONOR_PARTITION_MANIFEST_TAG
        || payload[1] != FORMAT_VERSION
        || payload[5..8] != [0; 3]
        || payload[64..80] != [0; 16]
        || arena.packed_child_count(id)? != 1
    {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "invalid donor partition manifest",
        ));
    }
    let group = read_u64(&payload[8..16]);
    let role = match (payload[2], payload[3], payload[4], group) {
        (DONOR_DIRECT_ROLE_TAG, 0, 0, 0) => DonorPartitionRole::DirectRun,
        (DONOR_NORMALIZATION_ROLE_TAG, outcome_tag, outcome_detail, group) if group != 0 => {
            DonorPartitionRole::Normalization {
                group,
                outcome: StorageOnlyNormalizationOutcome::decode(outcome_tag, outcome_detail)?,
            }
        }
        _ => {
            return Err(CommittedCheckpointIndexError::Corrupt(
                "donor partition role and normalization fields disagree",
            ));
        }
    };
    let measure = decode_measure(&payload[16..56]);
    let samples = read_u64(&payload[56..64]);
    if samples == 0 {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor partition has no samples",
        ));
    }
    let sample_root = arena.packed_child_at(id, 0)?;
    let sample_summary = sequence_node::<DonorSampleSpec>(arena, sample_root)?.0;
    if sample_summary.measure != measure || sample_summary.samples != samples {
        return Err(CommittedCheckpointIndexError::Corrupt(
            "donor partition manifest and sample tree disagree",
        ));
    }
    Ok(DecodedDonorPartitionManifest {
        role,
        measure,
        samples,
        sample_root,
    })
}

fn encode_measure(output: &mut Vec<u8>, measure: RelativeCheckpointMeasure) {
    output.extend_from_slice(&measure.source_bytes.to_le_bytes());
    output.extend_from_slice(&measure.source_utf16.to_le_bytes());
    output.extend_from_slice(&measure.physical_lines.to_le_bytes());
    output.extend_from_slice(&measure.green_events.to_le_bytes());
    output.extend_from_slice(&measure.projection_runs.to_le_bytes());
}

fn encode_measure_into(output: &mut [u8], measure: RelativeCheckpointMeasure) {
    debug_assert_eq!(output.len(), SAMPLE_RECORD_BYTES);
    output[0..8].copy_from_slice(&measure.source_bytes.to_le_bytes());
    output[8..16].copy_from_slice(&measure.source_utf16.to_le_bytes());
    output[16..24].copy_from_slice(&measure.physical_lines.to_le_bytes());
    output[24..32].copy_from_slice(&measure.green_events.to_le_bytes());
    output[32..40].copy_from_slice(&measure.projection_runs.to_le_bytes());
}

fn decode_measure(payload: &[u8]) -> RelativeCheckpointMeasure {
    debug_assert_eq!(payload.len(), SAMPLE_RECORD_BYTES);
    RelativeCheckpointMeasure {
        source_bytes: read_u64(&payload[0..8]),
        source_utf16: read_u64(&payload[8..16]),
        physical_lines: read_u64(&payload[16..24]),
        green_events: read_u64(&payload[24..32]),
        projection_runs: read_u64(&payload[32..40]),
    }
}

fn observe_allocation(
    receipt: &mut CommittedCheckpointIndexBuildReceipt,
    allocation: crate::AllocationReceipt,
) {
    receipt.maximum_page_payload_bytes = receipt
        .maximum_page_payload_bytes
        .max(allocation.payload_bytes_copied);
    receipt.payload_bytes_copied += allocation.payload_bytes_copied;
    receipt.edge_bytes_copied += allocation.edge_bytes_copied;
}

fn checked_add(
    left: u64,
    right: u64,
    field: &'static str,
) -> Result<u64, CommittedCheckpointIndexError> {
    left.checked_add(right)
        .ok_or(CommittedCheckpointIndexError::Overflow(field))
}

fn checked_difference(
    end: u64,
    start: u64,
    message: &'static str,
) -> Result<u64, CommittedCheckpointIndexError> {
    end.checked_sub(start)
        .ok_or(CommittedCheckpointIndexError::Invalid(message))
}

fn read_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes.try_into().expect("two-byte codec field"))
}

#[cfg(feature = "exact-parser")]
fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("four-byte codec field"))
}

fn read_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("eight-byte codec field"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "exact-parser")]
    use flark_comrak_value_block_core::{DirectPollStatus, DirectValueBlockParser, SyntaxProfile};

    fn measure(bytes: u64, lines: u64) -> RelativeCheckpointMeasure {
        RelativeCheckpointMeasure::new(bytes, bytes, lines, lines + 1, lines)
    }

    fn settle(arena: &mut PageArena) {
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(1).unwrap();
        }
    }

    #[cfg(feature = "exact-parser")]
    fn started_donor() -> DirectValueBlockParser {
        let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
        assert!(parser.pending_command().is_some());
        parser.acknowledge_command().unwrap();
        parser
    }

    #[cfg(feature = "exact-parser")]
    fn drive_donor_line(parser: &mut DirectValueBlockParser, line: &str) {
        parser.begin_line(line.to_owned()).unwrap();
        let limit = line.len().saturating_mul(8).saturating_add(256);
        for _ in 0..limit {
            let receipt = parser.poll_line(1).unwrap();
            assert!(receipt.transitions <= 1);
            match receipt.status {
                DirectPollStatus::CommandReady => parser.acknowledge_command().unwrap(),
                DirectPollStatus::Pending => {}
                DirectPollStatus::ExternalWorkReady => {
                    panic!("non-reference donor fixture unexpectedly requested external work")
                }
                DirectPollStatus::Complete => return,
            }
        }
        panic!("donor line did not converge");
    }

    #[cfg(feature = "exact-parser")]
    fn donor_after_line(line: &str) -> DirectValueBlockParser {
        let mut parser = started_donor();
        drive_donor_line(&mut parser, line);
        parser
    }

    #[cfg(feature = "exact-parser")]
    #[derive(Debug)]
    struct ForgedDonorIndex {
        header: OpaqueDonorHeader,
        frames: Vec<OpaqueDonorFrame>,
        terminal_depth: Option<u32>,
        terminal_tag: u8,
        sample_child_copies: usize,
        manifest_child_copies: usize,
        outer_child_ordinal: u16,
        outer_child_copies: usize,
    }

    #[cfg(feature = "exact-parser")]
    impl ForgedDonorIndex {
        fn from_parser(parser: &DirectValueBlockParser) -> Self {
            let capture = parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap();
            let draft = OpaqueDonorCaptureDraft::try_from_capture(capture).unwrap();
            Self {
                header: *draft.header(),
                frames: draft.frames().to_vec(),
                terminal_depth: None,
                terminal_tag: DONOR_PATH_NODE_TAG,
                sample_child_copies: 1,
                manifest_child_copies: 1,
                outer_child_ordinal: 0,
                outer_child_copies: 1,
            }
        }

        fn commit(
            self,
            arena: &mut PageArena,
        ) -> Result<StorageOnlyCommittedCheckpointIndex, CommittedCheckpointIndexError> {
            assert!(!self.frames.is_empty());
            let interval = measure(10, 1);
            let ticket = arena.begin_build()?;
            let mut session = arena
                .resume_build(ticket)
                .map_err(|failure| failure.error)?;

            let frame_count = self.frames.len();
            let mut path_owner: Option<ArenaBuildOwner> = None;
            for (index, frame) in self.frames.into_iter().enumerate() {
                let depth = if index + 1 == frame_count {
                    self.terminal_depth
                        .unwrap_or(u32::try_from(index + 1).unwrap())
                } else {
                    u32::try_from(index + 1).unwrap()
                };
                let mut payload = encode_donor_path_node(depth, frame);
                if index + 1 == frame_count {
                    payload[0] = self.terminal_tag;
                }
                let prior_id = path_owner
                    .as_ref()
                    .map(|owner| session.owner_id(owner))
                    .transpose()?;
                let children = prior_id.as_slice();
                let (next, _) = session.allocate_packed(&payload, children)?;
                if let Some(prior) = path_owner.replace(next) {
                    session.release(prior)?;
                }
            }
            let path_owner = path_owner.ok_or(CommittedCheckpointIndexError::Corrupt(
                "forged donor path has no terminal",
            ))?;
            let path_terminal = session.owner_id(&path_owner)?;

            let sample_summary = CheckpointIndexSummary {
                leaf_pages: 1,
                partitions: 1,
                samples: 1,
                height: 1,
                measure: interval,
                terminal_tail: false,
            };
            let mut sample_payload = encode_summary(DONOR_SAMPLE_LEAF_TAG, sample_summary).to_vec();
            encode_measure(&mut sample_payload, interval);
            sample_payload.extend_from_slice(&self.header);
            let sample_children = vec![path_terminal; self.sample_child_copies];
            let (sample_root, _) = session.allocate_packed(&sample_payload, &sample_children)?;
            session.release(path_owner)?;
            let sample_root_id = session.owner_id(&sample_root)?;

            let manifest_payload = encode_donor_partition_manifest(None, interval, 1)?;
            let manifest_children = vec![sample_root_id; self.manifest_child_copies];
            let (manifest, _) = session.allocate_packed(&manifest_payload, &manifest_children)?;
            session.release(sample_root)?;
            let manifest_id = session.owner_id(&manifest)?;

            let outer_summary = CheckpointIndexSummary {
                leaf_pages: 1,
                partitions: 1,
                samples: 1,
                height: 1,
                measure: interval,
                terminal_tail: false,
            };
            let mut outer_payload = encode_summary(INDEX_LEAF_TAG, outer_summary).to_vec();
            encode_partition_record(
                &mut outer_payload,
                DONOR_PARTITION_TAG,
                self.outer_child_ordinal,
                interval,
                1,
            );
            let outer_children = vec![manifest_id; self.outer_child_copies];
            let (root, _) = session.allocate_packed(&outer_payload, &outer_children)?;
            session.release(manifest)?;
            let owner = session.commit(root)?;
            Ok(StorageOnlyCommittedCheckpointIndex { owner: Some(owner) })
        }
    }

    #[test]
    fn relative_prefix_lookup_crosses_direct_and_group_partitions() {
        let mut arena = PageArena::new();
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::direct(measure(100, 1)))
            .unwrap();
        builder
            .push(StorageOnlyCheckpointPartition::normalization_group(
                7,
                StorageOnlyNormalizationOutcome::SetextHeading { level: 2 },
                vec![measure(30, 1), measure(40, 2), measure(50, 3)],
            ))
            .unwrap();
        builder
            .push(StorageOnlyCheckpointPartition::direct(measure(200, 4)))
            .unwrap();

        let (index, _) = builder.commit(&mut arena).unwrap();
        let first = index.locate_source_byte(&arena, 99).unwrap();
        assert_eq!(first.ordinal, 0);
        assert_eq!(first.prefix.source_bytes(), 0);
        assert!(matches!(first.kind, LocatedCheckpointPartitionKind::Direct));

        let group_partition = index.locate_source_byte(&arena, 100).unwrap();
        assert_eq!(group_partition.ordinal, 1);
        assert_eq!(group_partition.prefix.source_bytes(), 100);
        let LocatedCheckpointPartitionKind::Normalization(group) = group_partition.kind else {
            panic!("expected normalization group");
        };
        let first_sample = index.locate_group_sample(&arena, &group, 29).unwrap();
        assert_eq!(first_sample.ordinal, 0);
        assert_eq!(first_sample.prefix.source_bytes(), 0);
        let second_sample = index.locate_group_sample(&arena, &group, 30).unwrap();
        assert_eq!(second_sample.ordinal, 1);
        assert_eq!(second_sample.prefix.source_bytes(), 30);
        let third_sample = index.locate_group_sample(&arena, &group, 119).unwrap();
        assert_eq!(third_sample.ordinal, 2);
        assert_eq!(third_sample.prefix.source_bytes(), 70);

        let last = index.locate_source_byte(&arena, 220).unwrap();
        assert_eq!(last.ordinal, 2);
        assert_eq!(last.prefix.source_bytes(), 220);
        assert_eq!(last.interval.source_bytes(), 200);

        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn manifest_owns_sample_root_only_through_an_arena_child_edge() {
        let mut arena = PageArena::new();
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::normalization_group(
                11,
                StorageOnlyNormalizationOutcome::SetextHeading { level: 1 },
                vec![measure(16, 1), measure(16, 1)],
            ))
            .unwrap();
        let (index, _) = builder.commit(&mut arena).unwrap();
        let located = index.locate_source_byte(&arena, 0).unwrap();
        let LocatedCheckpointPartitionKind::Normalization(group) = located.kind else {
            panic!("expected normalization group");
        };
        let manifest_payload = arena.payload(group.manifest).unwrap();
        assert_eq!(manifest_payload.len(), MANIFEST_BYTES);
        assert_eq!(arena.packed_child_count(group.manifest).unwrap(), 1);
        assert_eq!(manifest_payload[64..80], [0; 16]);
        let decoded = decode_normalization_manifest(&arena, group.manifest).unwrap();
        assert_eq!(decoded.group, 11);
        assert_eq!(
            decoded.outcome,
            StorageOnlyNormalizationOutcome::SetextHeading { level: 1 }
        );
        assert!(arena.contains(decoded.sample_root));

        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn candidate_abort_reclaims_index_manifest_and_sample_tree() {
        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::normalization_group(
                13,
                StorageOnlyNormalizationOutcome::SetextHeading { level: 2 },
                (0..160).map(|_| measure(1024, 1)).collect(),
            ))
            .unwrap();
        let _manifest = builder.build_in_session(&mut session).unwrap();
        assert_eq!(session.live_owners().unwrap(), 1);
        let build = session.begin_abort().unwrap();
        loop {
            let poll = arena.poll_build_abort(build, 1).unwrap();
            if poll.complete {
                break;
            }
        }
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_storage_bytes, 0);
    }

    #[test]
    fn terminal_tail_is_final_parent_bound_output_progress_across_branch_splits() {
        let mut arena = PageArena::new();
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        for _ in 0..160 {
            builder
                .push(StorageOnlyCheckpointPartition::direct(measure(10, 1)))
                .unwrap();
        }
        let tail = RelativeCheckpointMeasure::new(0, 0, 0, 5, 3);
        builder
            .push(StorageOnlyCheckpointPartition::terminal_tail(tail))
            .unwrap();

        let (index, _) = builder.commit(&mut arena).unwrap();
        let summary = index.summary(&arena).unwrap();
        assert!(summary.height > 1, "fixture must exercise branch summaries");
        assert!(summary.terminal_tail);
        assert_eq!(summary.partitions, 161);
        assert_eq!(summary.samples, 160);
        assert_eq!(summary.measure.source_bytes(), 1_600);
        assert_eq!(summary.measure.source_utf16(), 1_600);
        assert_eq!(summary.measure.physical_lines(), 160);
        assert_eq!(summary.measure.green_events(), 325);
        assert_eq!(summary.measure.projection_runs(), 163);

        let last_source = index.locate_source_byte(&arena, 1_599).unwrap();
        assert!(matches!(
            last_source.kind,
            LocatedCheckpointPartitionKind::Direct
        ));
        assert!(matches!(
            index.locate_source_byte(&arena, 1_600),
            Err(CommittedCheckpointIndexError::SourceOutOfBounds)
        ));

        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn terminal_tail_shape_and_order_fail_closed() {
        for malformed in [
            RelativeCheckpointMeasure::new(1, 0, 0, 1, 0),
            RelativeCheckpointMeasure::new(0, 1, 0, 1, 0),
            RelativeCheckpointMeasure::new(0, 0, 1, 1, 0),
            RelativeCheckpointMeasure::new(0, 0, 0, 0, 0),
            RelativeCheckpointMeasure::new(0, 0, 0, 1, 2),
        ] {
            assert!(matches!(
                require_terminal_tail_interval(malformed),
                Err(CommittedCheckpointIndexError::Invalid(_))
            ));
        }

        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::direct(measure(10, 1)))
            .unwrap();
        builder
            .push(StorageOnlyCheckpointPartition::terminal_tail(
                RelativeCheckpointMeasure::new(0, 0, 0, 2, 1),
            ))
            .unwrap();
        builder
            .push(StorageOnlyCheckpointPartition::direct(measure(10, 1)))
            .unwrap();

        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        assert!(matches!(
            builder.build_in_session(&mut session),
            Err(CommittedCheckpointIndexError::Invalid(
                "terminal tail is not the final checkpoint-index partition"
            ))
        ));
        let build = session.begin_abort().unwrap();
        while !arena.poll_build_abort(build, 1).unwrap().complete {}
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn terminal_tail_summary_bit_is_forbidden_in_nested_sample_sequences() {
        let summary = CheckpointIndexSummary {
            leaf_pages: 1,
            partitions: 1,
            samples: 1,
            height: 1,
            measure: measure(10, 1),
            terminal_tail: true,
        };
        let encoded = encode_summary(SAMPLE_LEAF_TAG, summary);
        assert!(matches!(
            decode_summary(&encoded, SAMPLE_LEAF_TAG),
            Err(CommittedCheckpointIndexError::Corrupt(
                "invalid sequence summary values"
            ))
        ));
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn terminal_tail_eof_lookup_selects_the_last_donor_predecessor() {
        let parser = donor_after_line("alpha\n");
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::donor_direct(
                DonorCheckpointSampleDraft::try_new(
                    measure(10, 1),
                    parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap(),
            ))
            .unwrap();
        builder
            .push(StorageOnlyCheckpointPartition::terminal_tail(
                RelativeCheckpointMeasure::new(0, 0, 0, 2, 1),
            ))
            .unwrap();

        let mut arena = PageArena::new();
        let (index, _) = builder.commit(&mut arena).unwrap();
        let selected = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .expect("EOF must select the donor before the nonrestart tail");
        assert_eq!(selected.checkpoint_cut().source_bytes(), 10);
        assert!(index.summary(&arena).unwrap().terminal_tail);

        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn ten_mib_group_index_is_sparse_and_below_storage_only_cap() {
        const TEN_MIB: u64 = 10 * 1024 * 1024;
        const SAMPLE_BYTES: u64 = 16 * 1024;
        const EXPECTED_SAMPLES: usize = (TEN_MIB / SAMPLE_BYTES) as usize;
        const EXPECTED_STORAGE_ONLY_BYTES: usize = 26_968;
        const STORAGE_ONLY_CAP: usize = 160 * 1024;

        assert_eq!(EXPECTED_SAMPLES, 640);
        let mut arena = PageArena::new();
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::normalization_group(
                17,
                StorageOnlyNormalizationOutcome::SetextHeading { level: 1 },
                (0..EXPECTED_SAMPLES)
                    .map(|_| measure(SAMPLE_BYTES, 16))
                    .collect(),
            ))
            .unwrap();
        let (index, receipt) = builder.commit(&mut arena).unwrap();
        let summary = index.summary(&arena).unwrap();
        // Retire any build-owner transfers that no longer contribute to the
        // committed root before measuring its reachable storage.
        settle(&mut arena);
        let retained = arena.metrics().live_storage_bytes;

        assert_eq!(summary.partitions, 1);
        assert_eq!(summary.samples, EXPECTED_SAMPLES as u64);
        assert_eq!(summary.measure.source_bytes(), TEN_MIB);
        assert_eq!(summary.measure.source_utf16(), TEN_MIB);
        assert_eq!(receipt.outer_leaf_pages, 1);
        assert_eq!(receipt.normalization_manifests, 1);
        assert_eq!(receipt.sample_leaf_pages, 7);
        assert!(receipt.maximum_page_payload_bytes <= ARENA_PAGE_BYTES);
        assert_eq!(
            retained, EXPECTED_STORAGE_ONLY_BYTES,
            "the exact storage-only topology receipt changed; reassess the checkpoint budget"
        );
        assert!(
            retained < STORAGE_ONLY_CAP,
            "storage-only checkpoint index retained {retained} bytes; cap is {STORAGE_ONLY_CAP}"
        );
        eprintln!(
            "checkpoint_index_10mib samples={EXPECTED_SAMPLES} retained={retained} cap={STORAGE_ONLY_CAP} sample_leaves={}",
            receipt.sample_leaf_pages
        );

        for (byte, expected_ordinal) in [
            (0, 0),
            (SAMPLE_BYTES - 1, 0),
            (SAMPLE_BYTES, 1),
            (TEN_MIB / 2, 320),
            (TEN_MIB - 1, 639),
        ] {
            let partition = index.locate_source_byte(&arena, byte).unwrap();
            let LocatedCheckpointPartitionKind::Normalization(group) = partition.kind else {
                panic!("10 MiB fixture must be one normalization group");
            };
            let sample = index.locate_group_sample(&arena, &group, byte).unwrap();
            assert_eq!(sample.ordinal, expected_ordinal);
            assert_eq!(
                sample.prefix.source_bytes(),
                expected_ordinal * SAMPLE_BYTES
            );
        }

        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn exact_donor_10mib_all_direct_coalesces_and_shares_one_path_under_cap() {
        const TEN_MIB: u64 = 10 * 1024 * 1024;
        const SAMPLE_BYTES: u64 = 16 * 1024;
        const EXPECTED_SAMPLES: usize = (TEN_MIB / SAMPLE_BYTES) as usize;
        const RETAINED_CAP: usize = 160 * 1024;

        let parser = donor_after_line("alpha\n");
        let first_capture = parser
            .capture_durable_grammar_line_boundary_checkpoint()
            .unwrap();
        let shared_depth = first_capture.receipt().materialized_path_records;
        assert_eq!(shared_depth, 2);

        let mut arena = PageArena::new();
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::donor_direct(
                DonorCheckpointSampleDraft::try_new(measure(SAMPLE_BYTES, 16), first_capture)
                    .unwrap(),
            ))
            .unwrap();
        for _ in 1..EXPECTED_SAMPLES {
            let capture = parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap();
            builder
                .push(StorageOnlyCheckpointPartition::donor_direct(
                    DonorCheckpointSampleDraft::try_new(measure(SAMPLE_BYTES, 16), capture)
                        .unwrap(),
                ))
                .unwrap();
        }

        // Exercise the public one-sample direct API: coalescing must happen in
        // the builder rather than being hidden in a bulk-only test fixture.
        assert_eq!(builder.partitions.len(), 1);
        let (index, receipt) = builder.commit(&mut arena).unwrap();
        settle(&mut arena);
        let retained = arena.metrics().live_storage_bytes;
        let summary = index.summary(&arena).unwrap();

        assert_eq!(summary.partitions, 1);
        assert_eq!(summary.samples, EXPECTED_SAMPLES as u64);
        assert_eq!(summary.measure.source_bytes(), TEN_MIB);
        assert_eq!(receipt.donor_sample_headers, EXPECTED_SAMPLES);
        assert_eq!(receipt.donor_partition_manifests, 1);
        assert_eq!(receipt.donor_path_nodes_allocated, shared_depth);
        assert_eq!(
            receipt.donor_path_prefix_records_reused,
            (EXPECTED_SAMPLES - 1) * shared_depth
        );
        assert_eq!(receipt.donor_sample_path_edges, EXPECTED_SAMPLES);
        assert_eq!(receipt.retained_source_bytes, 0);
        assert!(receipt.donor_builder_queued_draft_bytes > retained / 2);
        assert!(
            retained < RETAINED_CAP,
            "exact donor index retained {retained} bytes; cap is {RETAINED_CAP}"
        );

        assert!(
            index
                .locate_donor_checkpoint_at_or_before_cut(&arena, 0)
                .unwrap()
                .is_none()
        );
        assert!(
            index
                .locate_donor_checkpoint_at_or_before_cut(&arena, SAMPLE_BYTES - 1)
                .unwrap()
                .is_none()
        );
        let first = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, SAMPLE_BYTES)
            .unwrap()
            .unwrap();
        assert_eq!(first.ordinal(), 0);
        assert_eq!(first.prefix().source_bytes(), 0);
        assert_eq!(first.interval().source_bytes(), SAMPLE_BYTES);
        assert_eq!(first.checkpoint_cut().source_bytes(), SAMPLE_BYTES);
        assert_eq!(first.retained_source_bytes(), 0);
        assert_eq!(first.receipt().retained_source_bytes, 0);
        first.decode_grammar_parts().unwrap();

        let between = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, SAMPLE_BYTES + 1)
            .unwrap()
            .unwrap();
        assert_eq!(between.ordinal(), 0);
        assert_eq!(between.checkpoint_cut().source_bytes(), SAMPLE_BYTES);
        let last = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, TEN_MIB)
            .unwrap()
            .unwrap();
        assert_eq!(last.ordinal(), (EXPECTED_SAMPLES - 1) as u64);
        assert_eq!(last.checkpoint_cut().source_bytes(), TEN_MIB);
        assert!(matches!(
            index.locate_donor_checkpoint_at_or_before_cut(&arena, TEN_MIB + 1),
            Err(CommittedCheckpointIndexError::SourceOutOfBounds)
        ));
        eprintln!(
            "exact_checkpoint_index_10mib samples={EXPECTED_SAMPLES} retained={retained} cap={RETAINED_CAP} path_nodes={} reused={} leaves={} queued_draft={}",
            receipt.donor_path_nodes_allocated,
            receipt.donor_path_prefix_records_reused,
            receipt.sample_leaf_pages,
            receipt.donor_builder_queued_draft_bytes,
        );

        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_storage_bytes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn exact_donor_depth_258_reuses_full_path_and_reconstructs_bounded_scratch() {
        let line = format!("{}alpha\n", "> ".repeat(256));
        let parser = donor_after_line(&line);
        let first_capture = parser
            .capture_durable_grammar_line_boundary_checkpoint()
            .unwrap();
        let second_capture = parser
            .capture_durable_grammar_line_boundary_checkpoint()
            .unwrap();
        assert_eq!(first_capture.receipt().materialized_path_records, 258);
        assert_eq!(second_capture.receipt().materialized_path_records, 258);

        let interval = measure(line.len() as u64, 1);
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::donor_direct(
                DonorCheckpointSampleDraft::try_new(interval, first_capture).unwrap(),
            ))
            .unwrap();
        builder
            .push(StorageOnlyCheckpointPartition::donor_direct(
                DonorCheckpointSampleDraft::try_new(interval, second_capture).unwrap(),
            ))
            .unwrap();
        let mut arena = PageArena::new();
        let (index, receipt) = builder.commit(&mut arena).unwrap();
        settle(&mut arena);

        assert_eq!(receipt.donor_path_nodes_allocated, 258);
        assert_eq!(receipt.donor_path_prefix_records_reused, 258);
        assert_eq!(receipt.retained_source_bytes, 0);
        let located = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, interval.source_bytes())
            .unwrap()
            .unwrap();
        let lookup = located.receipt();
        assert_eq!(lookup.path_nodes_visited, 2 * 258);
        assert_eq!(
            lookup.reconstructed_opaque_path_bytes,
            258 * DONOR_FRAME_BYTES
        );
        assert!(lookup.maximum_temporary_bytes < 32 * 1024);
        assert_eq!(lookup.retained_source_bytes, 0);
        located.decode_grammar_parts().unwrap();

        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn exact_donor_local_path_churn_stays_below_one_mib_without_claiming_global_sharing() {
        const SAMPLES: usize = 640;
        const RETAINED_CAP: usize = 1024 * 1024;

        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        let mut maximum_depth = 0;
        let mut materialized_records = 0;
        for index in 0..SAMPLES {
            // Sawtooth nesting changes the open-path suffix and occasionally
            // its root frame. This is intentionally less shareable than the
            // repeated-path storage receipt above.
            let quote_depth = index % 17;
            let line = format!("{}item-{index}\n", "> ".repeat(quote_depth));
            let parser = donor_after_line(&line);
            let capture = parser
                .capture_durable_grammar_line_boundary_checkpoint()
                .unwrap();
            let depth = capture.receipt().materialized_path_records;
            maximum_depth = maximum_depth.max(depth);
            materialized_records += depth;
            builder
                .push(StorageOnlyCheckpointPartition::donor_direct(
                    DonorCheckpointSampleDraft::try_new(measure(16 * 1024, 1), capture).unwrap(),
                ))
                .unwrap();
        }

        let mut arena = PageArena::new();
        let (index, receipt) = builder.commit(&mut arena).unwrap();
        settle(&mut arena);
        let retained = arena.metrics().live_storage_bytes;

        assert_eq!(receipt.donor_sample_headers, SAMPLES);
        assert_eq!(
            receipt.donor_materialized_path_records,
            materialized_records
        );
        assert!(receipt.donor_path_nodes_allocated >= maximum_depth);
        assert!(receipt.donor_path_nodes_allocated < materialized_records);
        assert!(receipt.donor_path_prefix_records_reused > 0);
        assert_eq!(receipt.retained_source_bytes, 0);
        assert!(
            retained < RETAINED_CAP,
            "churning donor paths retained {retained} bytes; cap is {RETAINED_CAP}"
        );
        eprintln!(
            "exact_checkpoint_index_churn samples={SAMPLES} retained={retained} cap={RETAINED_CAP} materialized={} allocated={} reused={} max_depth={maximum_depth}",
            receipt.donor_materialized_path_records,
            receipt.donor_path_nodes_allocated,
            receipt.donor_path_prefix_records_reused,
        );

        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn exact_donor_cut_lookup_uses_completed_endpoints_and_absolute_prefixes() {
        let parser = donor_after_line("alpha\n");
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::direct(measure(100, 1)))
            .unwrap();
        builder
            .push(StorageOnlyCheckpointPartition::donor_normalization_group(
                91,
                StorageOnlyNormalizationOutcome::SetextHeading { level: 1 },
                vec![
                    DonorCheckpointSampleDraft::try_new(
                        measure(10, 1),
                        parser
                            .capture_durable_grammar_line_boundary_checkpoint()
                            .unwrap(),
                    )
                    .unwrap(),
                ],
            ))
            .unwrap();
        builder
            .push(StorageOnlyCheckpointPartition::donor_direct(
                DonorCheckpointSampleDraft::try_new(
                    measure(20, 1),
                    parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap(),
            ))
            .unwrap();

        let mut arena = PageArena::new();
        let (index, _) = builder.commit(&mut arena).unwrap();
        for cut in [0, 99, 100, 109] {
            assert!(
                index
                    .locate_donor_checkpoint_at_or_before_cut(&arena, cut)
                    .unwrap()
                    .is_none(),
                "cut {cut} must not select an unfinished checkpoint interval"
            );
        }

        let first = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 110)
            .unwrap()
            .unwrap();
        assert_eq!(first.prefix().source_bytes(), 100);
        assert_eq!(first.checkpoint_cut().source_bytes(), 110);

        let boundary_fallback = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 111)
            .unwrap()
            .unwrap();
        assert_eq!(boundary_fallback.prefix().source_bytes(), 100);
        assert_eq!(boundary_fallback.checkpoint_cut().source_bytes(), 110);

        let eof = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 130)
            .unwrap()
            .unwrap();
        assert_eq!(eof.prefix().source_bytes(), 110);
        assert_eq!(eof.interval().source_bytes(), 20);
        assert_eq!(eof.checkpoint_cut().source_bytes(), 130);

        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    #[allow(clippy::too_many_lines)] // One fixture keeps every crossed authority axis directly comparable.
    fn exact_donor_committed_role_rejects_crossed_authority_bindings() {
        let parser = donor_after_line("alpha\n");
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::donor_normalization_group(
                91,
                StorageOnlyNormalizationOutcome::SetextHeading { level: 1 },
                vec![
                    DonorCheckpointSampleDraft::try_new(
                        measure(10, 1),
                        parser
                            .capture_durable_grammar_line_boundary_checkpoint()
                            .unwrap(),
                    )
                    .unwrap(),
                    DonorCheckpointSampleDraft::try_new(
                        measure(10, 1),
                        parser
                            .capture_durable_grammar_line_boundary_checkpoint()
                            .unwrap(),
                    )
                    .unwrap(),
                ],
            ))
            .unwrap();
        builder
            .push(StorageOnlyCheckpointPartition::donor_normalization_group(
                92,
                StorageOnlyNormalizationOutcome::SetextHeading { level: 2 },
                vec![
                    DonorCheckpointSampleDraft::try_new(
                        measure(10, 1),
                        parser
                            .capture_durable_grammar_line_boundary_checkpoint()
                            .unwrap(),
                    )
                    .unwrap(),
                ],
            ))
            .unwrap();

        let mut arena = PageArena::new();
        let (index, _) = builder.commit(&mut arena).unwrap();

        let first = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .unwrap();
        let CommittedDonorCheckpointRole::Normalization(outcome) =
            first.committed_role(&index, &arena).unwrap()
        else {
            panic!("normalization donor must retain its committed role");
        };
        assert_eq!(outcome.group(), 91);
        assert_eq!(
            outcome.outcome(),
            StorageOnlyNormalizationOutcome::SetextHeading { level: 1 }
        );

        let second_sample = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 20)
            .unwrap()
            .unwrap();
        let second_partition = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 30)
            .unwrap()
            .unwrap();

        let mut other_builder = StorageOnlyCheckpointIndexBuilder::default();
        other_builder
            .push(StorageOnlyCheckpointPartition::donor_normalization_group(
                191,
                StorageOnlyNormalizationOutcome::SetextHeading { level: 1 },
                vec![
                    DonorCheckpointSampleDraft::try_new(
                        measure(10, 1),
                        parser
                            .capture_durable_grammar_line_boundary_checkpoint()
                            .unwrap(),
                    )
                    .unwrap(),
                ],
            ))
            .unwrap();
        let (other_index, _) = other_builder.commit(&mut arena).unwrap();

        let crossed_root = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .unwrap();
        assert!(matches!(
            crossed_root.committed_role(&other_index, &arena),
            Err(CommittedCheckpointIndexError::Corrupt(
                "donor role authority belongs to another checkpoint index"
            ))
        ));

        let foreign_arena = PageArena::new();
        assert!(matches!(
            crossed_root.committed_role(&index, &foreign_arena),
            Err(CommittedCheckpointIndexError::Arena(
                ArenaError::WrongArena { .. }
            ))
        ));

        let mut forged_root = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .unwrap();
        forged_root.authority.index_root = other_index.scoped_root_id();
        assert!(forged_root.committed_role(&index, &arena).is_err());

        let mut crossed_partition = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .unwrap();
        crossed_partition.authority.partition_manifest =
            second_partition.authority.partition_manifest;
        assert!(matches!(
            crossed_partition.committed_role(&index, &arena),
            Err(CommittedCheckpointIndexError::Corrupt(
                "donor role authority partition binding mismatch"
            ))
        ));

        let mut forged_group = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .unwrap();
        forged_group.role = DonorPartitionRole::Normalization {
            group: 92,
            outcome: StorageOnlyNormalizationOutcome::SetextHeading { level: 1 },
        };
        assert!(matches!(
            forged_group.committed_role(&index, &arena),
            Err(CommittedCheckpointIndexError::Corrupt(
                "donor role authority group or outcome mismatch"
            ))
        ));

        let mut forged_outcome = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .unwrap();
        forged_outcome.role = DonorPartitionRole::Normalization {
            group: 91,
            outcome: StorageOnlyNormalizationOutcome::SetextHeading { level: 2 },
        };
        assert!(forged_outcome.committed_role(&index, &arena).is_err());

        let mut crossed_sample = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .unwrap();
        crossed_sample.authority.sample_ordinal = second_sample.authority.sample_ordinal;
        crossed_sample.authority.sample_header = second_sample.authority.sample_header;
        crossed_sample.authority.sample_path_terminal =
            second_sample.authority.sample_path_terminal;
        assert!(matches!(
            crossed_sample.committed_role(&index, &arena),
            Err(CommittedCheckpointIndexError::Corrupt(
                "donor role authority sample binding mismatch"
            ))
        ));

        let mut forged_sample = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .unwrap();
        forged_sample.authority.sample_ordinal = u64::MAX;
        assert!(forged_sample.committed_role(&index, &arena).is_err());

        other_index.release_later(&mut arena).unwrap();
        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn exact_donor_role_tracks_the_selected_predecessor_across_normalization_frontiers() {
        #[derive(Clone, Copy)]
        enum ExpectedRole {
            Direct,
            Normalization,
        }

        let parser = donor_after_line("alpha\n");
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::donor_direct(
                DonorCheckpointSampleDraft::try_new(
                    measure(10, 1),
                    parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap(),
            ))
            .unwrap();
        builder
            .push(StorageOnlyCheckpointPartition::donor_normalization_group(
                41,
                StorageOnlyNormalizationOutcome::SetextHeading { level: 1 },
                vec![
                    DonorCheckpointSampleDraft::try_new(
                        measure(10, 1),
                        parser
                            .capture_durable_grammar_line_boundary_checkpoint()
                            .unwrap(),
                    )
                    .unwrap(),
                    DonorCheckpointSampleDraft::try_new(
                        measure(10, 1),
                        parser
                            .capture_durable_grammar_line_boundary_checkpoint()
                            .unwrap(),
                    )
                    .unwrap(),
                ],
            ))
            .unwrap();
        builder
            .push(StorageOnlyCheckpointPartition::donor_direct(
                DonorCheckpointSampleDraft::try_new(
                    measure(10, 1),
                    parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap(),
            ))
            .unwrap();

        let mut arena = PageArena::new();
        let (index, _) = builder.commit(&mut arena).unwrap();
        for (source_cut, checkpoint_cut, expected) in [
            (10, 10, ExpectedRole::Direct),
            // The containing normalization partition has no completed sample
            // yet, so selection belongs to the prior direct partition.
            (11, 10, ExpectedRole::Direct),
            (20, 20, ExpectedRole::Normalization),
            (21, 20, ExpectedRole::Normalization),
            (30, 30, ExpectedRole::Normalization),
            // The containing direct partition has no completed sample yet, so
            // the last normalization result remains the selected authority.
            (31, 30, ExpectedRole::Normalization),
            (40, 40, ExpectedRole::Direct),
        ] {
            let located = index
                .locate_donor_checkpoint_at_or_before_cut(&arena, source_cut)
                .unwrap()
                .unwrap();
            assert_eq!(located.checkpoint_cut().source_bytes(), checkpoint_cut);
            match (located.committed_role(&index, &arena).unwrap(), expected) {
                (CommittedDonorCheckpointRole::DirectRun(direct), ExpectedRole::Direct) => {
                    assert_eq!(direct.checkpoint_cut().source_bytes(), checkpoint_cut);
                }
                (
                    CommittedDonorCheckpointRole::Normalization(outcome),
                    ExpectedRole::Normalization,
                ) => {
                    assert_eq!(outcome.group(), 41);
                    assert_eq!(
                        outcome.outcome(),
                        StorageOnlyNormalizationOutcome::SetextHeading { level: 1 }
                    );
                }
                _ => panic!("cut {source_cut} retained the wrong donor partition role"),
            }
        }

        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn exact_normalization_completion_selects_the_final_sample_with_logarithmic_receipt() {
        const SAMPLES: usize = 160;
        const SAMPLE_BYTES: u64 = 64;
        const PREFIX_BYTES: u64 = 25;

        let parser = donor_after_line("alpha\n");
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::donor_direct(
                DonorCheckpointSampleDraft::try_new(
                    measure(PREFIX_BYTES, 1),
                    parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap(),
            ))
            .unwrap();
        builder
            .push(StorageOnlyCheckpointPartition::donor_normalization_group(
                501,
                StorageOnlyNormalizationOutcome::SetextHeading { level: 2 },
                (0..SAMPLES)
                    .map(|_| {
                        DonorCheckpointSampleDraft::try_new(
                            measure(SAMPLE_BYTES, 1),
                            parser
                                .capture_durable_grammar_line_boundary_checkpoint()
                                .unwrap(),
                        )
                        .unwrap()
                    })
                    .collect(),
            ))
            .unwrap();

        let mut arena = PageArena::new();
        let (index, _) = builder.commit(&mut arena).unwrap();
        let early = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, PREFIX_BYTES + SAMPLE_BYTES)
            .unwrap()
            .unwrap();
        assert_eq!(early.ordinal(), 0);
        let CommittedDonorCheckpointRole::Normalization(capability) =
            early.committed_role(&index, &arena).unwrap()
        else {
            panic!("first group sample must carry normalization authority");
        };
        let bounds = capability.bounds();
        assert_eq!(bounds.start().source_bytes(), PREFIX_BYTES);
        assert_eq!(
            bounds.interval().source_bytes(),
            SAMPLES as u64 * SAMPLE_BYTES
        );
        assert_eq!(
            bounds.end().source_bytes(),
            PREFIX_BYTES + SAMPLES as u64 * SAMPLE_BYTES
        );

        let completion = capability.completion_checkpoint(&index, &arena).unwrap();
        assert_eq!(completion.group(), 501);
        assert_eq!(
            completion.outcome(),
            StorageOnlyNormalizationOutcome::SetextHeading { level: 2 }
        );
        assert_eq!(completion.bounds(), bounds);
        assert_eq!(completion.final_sample_ordinal(), (SAMPLES - 1) as u64);
        assert_ne!(completion.final_sample_ordinal(), early.ordinal());
        assert_eq!(completion.checkpoint_cut(), bounds.end());
        assert_eq!(
            completion.donor.authority.sample_ordinal,
            (SAMPLES - 1) as u64
        );
        assert_eq!(
            completion.donor.authority.partition_manifest,
            early.authority.partition_manifest
        );

        let receipt = completion.receipt();
        assert_eq!(receipt.group_samples, SAMPLES as u64);
        assert!(receipt.sample_tree_height > 1);
        assert!(receipt.sample_tree_nodes_visited <= usize::from(receipt.sample_tree_height));
        assert!(receipt.sample_tree_nodes_visited < SAMPLES);
        assert!(receipt.sample_leaf_temporary_bytes > 0);
        assert!(receipt.path_nodes_visited > 0);
        assert!(receipt.reconstructed_opaque_path_bytes > 0);
        assert!(receipt.donor_typed_recipe_bytes > 0);
        assert!(receipt.maximum_temporary_bytes >= receipt.sample_leaf_temporary_bytes);
        assert_eq!(receipt.retained_source_bytes, 0);

        let final_donor = completion.into_located_donor();
        let CommittedDonorCheckpointRole::Normalization(final_capability) =
            final_donor.committed_role(&index, &arena).unwrap()
        else {
            panic!("completion donor must remain bound to its normalization group");
        };
        assert_eq!(
            final_capability.bounds().end(),
            final_donor.checkpoint_cut()
        );

        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn exact_normalization_completion_rejects_crossed_capability_roots_and_manifests() {
        let parser = donor_after_line("alpha\n");
        let build_group = |group, outcome| {
            let mut builder = StorageOnlyCheckpointIndexBuilder::default();
            builder
                .push(StorageOnlyCheckpointPartition::donor_normalization_group(
                    group,
                    outcome,
                    vec![
                        DonorCheckpointSampleDraft::try_new(
                            measure(10, 1),
                            parser
                                .capture_durable_grammar_line_boundary_checkpoint()
                                .unwrap(),
                        )
                        .unwrap(),
                        DonorCheckpointSampleDraft::try_new(
                            measure(10, 1),
                            parser
                                .capture_durable_grammar_line_boundary_checkpoint()
                                .unwrap(),
                        )
                        .unwrap(),
                    ],
                ))
                .unwrap();
            builder
        };

        let mut arena = PageArena::new();
        let (first_index, _) = build_group(
            601,
            StorageOnlyNormalizationOutcome::SetextHeading { level: 1 },
        )
        .commit(&mut arena)
        .unwrap();
        let (second_index, _) = build_group(
            602,
            StorageOnlyNormalizationOutcome::SetextHeading { level: 2 },
        )
        .commit(&mut arena)
        .unwrap();

        let first = first_index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .unwrap();
        let second = second_index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .unwrap();
        let CommittedDonorCheckpointRole::Normalization(mut capability) =
            first.committed_role(&first_index, &arena).unwrap()
        else {
            panic!("fixture must produce normalization authority");
        };

        assert!(matches!(
            capability.completion_checkpoint(&second_index, &arena),
            Err(CommittedCheckpointIndexError::Corrupt(
                "normalization completion capability belongs to another checkpoint index"
            ))
        ));
        let foreign_arena = PageArena::new();
        assert!(matches!(
            capability.completion_checkpoint(&first_index, &foreign_arena),
            Err(CommittedCheckpointIndexError::Arena(
                ArenaError::WrongArena { .. }
            ))
        ));

        capability.binding.partition_manifest = second.authority.partition_manifest;
        assert!(matches!(
            capability.completion_checkpoint(&first_index, &arena),
            Err(CommittedCheckpointIndexError::Corrupt(
                "normalization completion capability binding mismatch"
            ))
        ));

        second_index.release_later(&mut arena).unwrap();
        first_index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn exact_donor_lookup_treats_any_legacy_run_as_a_restart_barrier() {
        let parser = donor_after_line("alpha\n");
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        builder
            .push(StorageOnlyCheckpointPartition::donor_direct(
                DonorCheckpointSampleDraft::try_new(
                    measure(10, 1),
                    parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap(),
            ))
            .unwrap();
        for _ in 0..512 {
            builder
                .push(StorageOnlyCheckpointPartition::direct(measure(10, 1)))
                .unwrap();
        }

        let mut arena = PageArena::new();
        let (index, _) = builder.commit(&mut arena).unwrap();
        let donor_end = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .unwrap();
        assert_eq!(donor_end.checkpoint_cut().source_bytes(), 10);
        assert!(
            index
                .locate_donor_checkpoint_at_or_before_cut(&arena, 20)
                .unwrap()
                .is_none(),
            "a legacy interval must block fallback to older donor state"
        );
        assert!(
            index
                .locate_donor_checkpoint_at_or_before_cut(&arena, 5130)
                .unwrap()
                .is_none(),
            "lookup must not scan hundreds of legacy partitions"
        );

        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn exact_donor_persisted_schema_topology_and_child_corruption_fail_closed() {
        let parser = donor_after_line("> alpha\n");

        let mut invalid_header = ForgedDonorIndex::from_parser(&parser);
        invalid_header.header[32] ^= 1;
        invalid_header.terminal_depth = Some(u32::MAX);
        let mut arena = PageArena::new();
        let index = invalid_header.commit(&mut arena).unwrap();
        assert!(
            matches!(
                index.locate_donor_checkpoint_at_or_before_cut(&arena, 10),
                Err(CommittedCheckpointIndexError::Corrupt(
                    "donor rejected persisted header schema"
                ))
            ),
            "header validation must happen before forged path depth is walked"
        );
        index.release_later(&mut arena).unwrap();
        settle(&mut arena);

        let mut foreign_frame = ForgedDonorIndex::from_parser(&parser);
        foreign_frame.frames[0][0] ^= 1;
        let index = foreign_frame.commit(&mut arena).unwrap();
        assert!(matches!(
            index.locate_donor_checkpoint_at_or_before_cut(&arena, 10),
            Err(CommittedCheckpointIndexError::Corrupt(
                "donor rejected persisted opaque frame schema"
            ))
        ));
        index.release_later(&mut arena).unwrap();
        settle(&mut arena);

        let mut discontinuous_depth = ForgedDonorIndex::from_parser(&parser);
        let actual_depth = u32::try_from(discontinuous_depth.frames.len()).unwrap();
        discontinuous_depth.terminal_depth = Some(actual_depth + 1);
        let index = discontinuous_depth.commit(&mut arena).unwrap();
        assert!(matches!(
            index.locate_donor_checkpoint_at_or_before_cut(&arena, 10),
            Err(CommittedCheckpointIndexError::Corrupt(
                "donor path depth chain is discontinuous"
            ))
        ));
        index.release_later(&mut arena).unwrap();
        settle(&mut arena);

        let mut foreign_path_wrapper = ForgedDonorIndex::from_parser(&parser);
        foreign_path_wrapper.terminal_tag ^= 1;
        let index = foreign_path_wrapper.commit(&mut arena).unwrap();
        assert!(matches!(
            index.locate_donor_checkpoint_at_or_before_cut(&arena, 10),
            Err(CommittedCheckpointIndexError::Corrupt(
                "invalid opaque donor path node"
            ))
        ));
        index.release_later(&mut arena).unwrap();
        settle(&mut arena);

        let mut sample_child_mismatch = ForgedDonorIndex::from_parser(&parser);
        sample_child_mismatch.sample_child_copies = 2;
        let index = sample_child_mismatch.commit(&mut arena).unwrap();
        assert!(matches!(
            index.locate_donor_checkpoint_at_or_before_cut(&arena, 10),
            Err(CommittedCheckpointIndexError::Corrupt(
                "donor sample records and path children disagree"
            ))
        ));
        index.release_later(&mut arena).unwrap();
        settle(&mut arena);

        let mut manifest_child_mismatch = ForgedDonorIndex::from_parser(&parser);
        manifest_child_mismatch.manifest_child_copies = 2;
        let index = manifest_child_mismatch.commit(&mut arena).unwrap();
        assert!(matches!(
            index.locate_donor_checkpoint_at_or_before_cut(&arena, 10),
            Err(CommittedCheckpointIndexError::Corrupt(
                "invalid donor partition manifest"
            ))
        ));
        index.release_later(&mut arena).unwrap();
        settle(&mut arena);

        let mut outer_child_mismatch = ForgedDonorIndex::from_parser(&parser);
        outer_child_mismatch.outer_child_copies = 2;
        let index = outer_child_mismatch.commit(&mut arena).unwrap();
        assert!(matches!(
            index.locate_donor_checkpoint_at_or_before_cut(&arena, 10),
            Err(CommittedCheckpointIndexError::Corrupt(
                "outer partition records and child edges disagree"
            ))
        ));
        index.release_later(&mut arena).unwrap();
        settle(&mut arena);

        let mut outer_ordinal_mismatch = ForgedDonorIndex::from_parser(&parser);
        outer_ordinal_mismatch.outer_child_ordinal = 1;
        let index = outer_ordinal_mismatch.commit(&mut arena).unwrap();
        assert!(matches!(
            index.locate_donor_checkpoint_at_or_before_cut(&arena, 10),
            Err(CommittedCheckpointIndexError::Corrupt(
                "outer partition child ordinals are not canonical"
            ))
        ));
        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_storage_bytes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn exact_donor_contextual_path_mismatch_is_rejected_only_by_typed_grammar_decode() {
        let parser = donor_after_line("> alpha\n");
        let mut missing_suffix = ForgedDonorIndex::from_parser(&parser);
        assert!(missing_suffix.frames.len() > 1);
        missing_suffix.frames.pop();

        let mut arena = PageArena::new();
        let index = missing_suffix.commit(&mut arena).unwrap();
        let recipe = index
            .locate_donor_checkpoint_at_or_before_cut(&arena, 10)
            .unwrap()
            .unwrap();
        assert_eq!(recipe.retained_source_bytes(), 0);
        assert!(
            recipe.decode_grammar_parts().is_err(),
            "storage validates individual opaque records; only the donor may interpret contextual count/checksum state"
        );

        index.release_later(&mut arena).unwrap();
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn exact_donor_candidate_abort_reclaims_shared_path_graph_with_fuel_one() {
        let parser = donor_after_line(&format!("{}alpha\n", "> ".repeat(64)));
        let mut builder = StorageOnlyCheckpointIndexBuilder::default();
        for _ in 0..160 {
            builder
                .push(StorageOnlyCheckpointPartition::donor_direct(
                    DonorCheckpointSampleDraft::try_new(
                        measure(1024, 1),
                        parser
                            .capture_durable_grammar_line_boundary_checkpoint()
                            .unwrap(),
                    )
                    .unwrap(),
                ))
                .unwrap();
        }

        let mut arena = PageArena::new();
        let ticket = arena.begin_build().unwrap();
        let mut session = arena.resume_build(ticket).unwrap();
        let _manifest = builder.build_in_session(&mut session).unwrap();
        assert_eq!(session.live_owners().unwrap(), 1);
        let build = session.begin_abort().unwrap();
        loop {
            let poll = arena.poll_build_abort(build, 1).unwrap();
            assert!(poll.owners_scheduled <= 1);
            if poll.complete {
                break;
            }
        }
        settle(&mut arena);
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_storage_bytes, 0);
    }
}

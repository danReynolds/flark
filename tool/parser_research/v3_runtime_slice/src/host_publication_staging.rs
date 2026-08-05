//! Standalone staged host-publication mechanism probe.
//!
//! This module deliberately does not wire the moving `host_mirror` protocol.
//! It exercises the production-shaped lifetime seam independently: one
//! credited bounded wire buffer, incremental hashing/closed-leaf admission, an
//! arena-owned resumable sequence build, an exact-current commit gate, atomic
//! manifest replacement, and separately fuelled abort/root reclamation.

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use crate::arena::{
    ArenaBuildError, ArenaBuildId, ArenaBuildOwner, ArenaBuildSession, ArenaBuildTicket,
};
use crate::persistent_sequence::{
    ResumableSequenceProgress, ResumableSequenceSplice, ResumableSequenceSplitProgress,
    ResumableStreamingSequenceBuilder, SequenceDeletedSummaryGuard, SequenceMutationReceipt,
    SequenceSpec, sequence_node,
};
use crate::serialized_green::{
    COPIED_GREEN_CLOSURE_MAX_BYTES, CopiedGreenClosureValidator, CopiedGreenLeafDecoded,
    CopiedGreenValidationFuel, CopiedGreenValidationProgress, SerializedGreenError,
    SerializedMetric,
};
use crate::{
    ARENA_PAGE_BYTES, ArenaError, ArenaId, MAX_PACKED_ARENA_CHILDREN, OwnedArenaRef, PageArena,
};

const CHUNK_MAX_BYTES: usize = 256 * 1024;
const OFFER_MAX_CHUNKS: u64 = 1 << 20;
const OFFER_MAX_WIRE_BYTES: u64 = 512 * 1024 * 1024;
const RECORD_HEADER_BYTES: usize = 17;
const PROGRAM_LENGTH_BYTES: usize = 2;
const RECORD_TAG: u8 = 0x71;
const LEAF_TAG: u8 = 0xd1;
const BRANCH_TAG: u8 = 0xd2;
const MANIFEST_TAG: u8 = 0xd3;
const SUMMARY_BYTES: usize = 75;
const LEAF_BYTES: usize = SUMMARY_BYTES + 8;
const BRANCH_BYTES: usize = SUMMARY_BYTES;
const MANIFEST_BYTES: usize = 81;
const HASH_BASE: u128 = 0x0000_0000_0000_0000_0000_0100_0000_01b3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Metric {
    bytes: u64,
    utf16: u64,
}

impl Metric {
    fn checked_add(self, other: Self) -> Result<Self, StagingError> {
        Ok(Self {
            bytes: self
                .bytes
                .checked_add(other.bytes)
                .ok_or(StagingError::Invalid("byte metric overflow"))?,
            utf16: self
                .utf16
                .checked_add(other.utf16)
                .ok_or(StagingError::Invalid("UTF-16 metric overflow"))?,
        })
    }
}

impl From<SerializedMetric> for Metric {
    fn from(value: SerializedMetric) -> Self {
        Self {
            bytes: value.bytes,
            utf16: value.utf16,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SourceStamp {
    revision: u64,
    metric: Metric,
    content_hash: u128,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SequenceSummary {
    leaves: u64,
    metric: Metric,
    content_digest: u128,
    polynomial_factor: u128,
    height: u16,
    balance: i64,
    minimum_prefix: i64,
}

impl SequenceSummary {
    fn leaf(metric: Metric, content_digest: u128, balance: i64, minimum_prefix: i64) -> Self {
        Self {
            leaves: 1,
            metric,
            content_digest,
            polynomial_factor: HASH_BASE,
            height: 1,
            balance,
            minimum_prefix,
        }
    }

    fn combine(self, other: Self) -> Result<Self, StagingError> {
        Ok(Self {
            leaves: self
                .leaves
                .checked_add(other.leaves)
                .ok_or(StagingError::Invalid("leaf count overflow"))?,
            metric: self.metric.checked_add(other.metric)?,
            content_digest: self
                .content_digest
                .wrapping_mul(other.polynomial_factor)
                .wrapping_add(other.content_digest),
            polynomial_factor: self.polynomial_factor.wrapping_mul(other.polynomial_factor),
            height: self
                .height
                .max(other.height)
                .checked_add(1)
                .ok_or(StagingError::Invalid("sequence height overflow"))?,
            balance: self
                .balance
                .checked_add(other.balance)
                .ok_or(StagingError::Invalid("sequence balance overflow"))?,
            minimum_prefix: self.minimum_prefix.min(
                self.balance
                    .checked_add(other.minimum_prefix)
                    .ok_or(StagingError::Invalid("sequence minimum-prefix overflow"))?,
            ),
        })
    }
}

/// Height-free source-order accumulator used while records stream. It must not
/// pretend that a left fold has the height of the balanced tree being built.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct StreamSummary {
    leaves: u64,
    metric: Metric,
    content_digest: u128,
    polynomial_factor: u128,
}

impl StreamSummary {
    fn append_leaf(self, leaf: SequenceSummary) -> Result<Self, StagingError> {
        Ok(Self {
            leaves: self
                .leaves
                .checked_add(1)
                .ok_or(StagingError::Invalid("stream leaf count overflow"))?,
            metric: self.metric.checked_add(leaf.metric)?,
            content_digest: self
                .content_digest
                .wrapping_mul(leaf.polynomial_factor)
                .wrapping_add(leaf.content_digest),
            polynomial_factor: self.polynomial_factor.wrapping_mul(leaf.polynomial_factor),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PublicationAck {
    session: u128,
    target_revision: u64,
    source: SourceStamp,
    summary: SequenceSummary,
    manifest_digest: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OfferMode {
    Snapshot,
    Delta {
        base: PublicationAck,
        old_start: u64,
        old_delete: u64,
        deleted: DeletedRangeProof,
    },
}

/// Height-free proof of the exact base range an actor intends to replace.
/// Tree height is deliberately excluded because it is an implementation
/// detail of the persistent sequence rather than source-order identity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DeletedRangeProof {
    leaves: u64,
    metric: Metric,
    content_digest: u128,
    polynomial_factor: u128,
    balance: i64,
    minimum_prefix: i64,
}

impl DeletedRangeProof {
    fn from_summary(summary: SequenceSummary) -> Self {
        Self {
            leaves: summary.leaves,
            metric: summary.metric,
            content_digest: summary.content_digest,
            polynomial_factor: summary.polynomial_factor,
            balance: summary.balance,
            minimum_prefix: summary.minimum_prefix,
        }
    }

    fn expected_summary(self) -> Option<SequenceSummary> {
        (self.leaves != 0).then_some(SequenceSummary {
            leaves: self.leaves,
            metric: self.metric,
            content_digest: self.content_digest,
            polynomial_factor: self.polynomial_factor,
            height: 0,
            balance: self.balance,
            minimum_prefix: self.minimum_prefix,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OfferBegin {
    offer: u64,
    session: u128,
    target_revision: u64,
    source: SourceStamp,
    mode: OfferMode,
    inserted_leaves: u64,
    target_leaf_count: u64,
    target_metric: Metric,
    maximum_chunks: u64,
    maximum_wire_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CommitClaim {
    offer: u64,
    actual_chunks: u64,
    actual_wire_bytes: u64,
    rolling_transport_digest: u128,
    inserted_stream_digest: u128,
}

#[derive(Clone, Debug)]
struct PublicationChunk {
    offer: u64,
    ordinal: u64,
    first_leaf: u64,
    leaf_count: u32,
    payload: Arc<[u8]>,
    digest: u128,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PublicationPollReceipt {
    wire_bytes_hashed: usize,
    wire_bytes_copied: usize,
    transitions: usize,
    pages_allocated: usize,
    branches_allocated: usize,
    maximum_live_chunk_bytes: usize,
    committed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PublicationFuel {
    inspect_bytes: usize,
    copy_bytes: usize,
    transitions: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicationProgress {
    Pending,
    ChunkAccepted { ordinal: u64 },
    Committed(PublicationAck),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StagingError {
    Arena(ArenaError),
    Build(ArenaBuildError),
    Green(SerializedGreenError),
    Invalid(&'static str),
    Backpressure,
    ExactCurrentMismatch,
    BaseMismatch,
}

impl From<ArenaError> for StagingError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

impl From<ArenaBuildError> for StagingError {
    fn from(value: ArenaBuildError) -> Self {
        Self::Build(value)
    }
}

impl From<SerializedGreenError> for StagingError {
    fn from(value: SerializedGreenError) -> Self {
        Self::Green(value)
    }
}

impl fmt::Display for StagingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arena(error) => error.fmt(formatter),
            Self::Build(error) => error.fmt(formatter),
            Self::Green(error) => error.fmt(formatter),
            Self::Invalid(message) => write!(formatter, "invalid staged publication: {message}"),
            Self::Backpressure => formatter.write_str("staged publication is backpressured"),
            Self::ExactCurrentMismatch => {
                formatter.write_str("staged publication is not exact-current")
            }
            Self::BaseMismatch => formatter.write_str("staged publication base mismatch"),
        }
    }
}

impl std::error::Error for StagingError {}

#[derive(Debug)]
struct HostSequenceSpec;

impl SequenceSpec for HostSequenceSpec {
    type Summary = SequenceSummary;
    type Error = StagingError;
    type BranchPayload = [u8; BRANCH_BYTES];

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(LEAF_TAG) {
            return Ok(None);
        }
        if payload.len() != LEAF_BYTES {
            return Err(StagingError::Invalid("malformed staged leaf wrapper"));
        }
        Ok(Some(decode_summary(&payload[..SUMMARY_BYTES])?))
    }

    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(BRANCH_TAG) {
            return Ok(None);
        }
        if payload.len() != BRANCH_BYTES {
            return Err(StagingError::Invalid("malformed staged branch"));
        }
        Ok(Some(decode_summary(payload)?))
    }

    fn encode_branch(summary: Self::Summary) -> Self::BranchPayload {
        encode_summary(BRANCH_TAG, summary)
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        left.combine(right)
    }

    fn leaves(summary: Self::Summary) -> u64 {
        summary.leaves
    }

    fn height(summary: Self::Summary) -> u16 {
        summary.height
    }

    fn invalid(message: &'static str) -> Self::Error {
        StagingError::Invalid(message)
    }
}

#[derive(Debug)]
struct ProgramOwner {
    raw: Range<usize>,
    owner: Option<ArenaBuildOwner>,
}

#[derive(Debug)]
struct LeafAssembly {
    leaf_id: u64,
    raw: Range<usize>,
    cursor: usize,
    closure_end: usize,
    program_count: usize,
    programs: Vec<ProgramOwner>,
    validator: Option<CopiedGreenClosureValidator>,
    decoded: Option<CopiedGreenLeafDecoded>,
    content_hash_cursor: usize,
    closure_digest: u128,
    canonical_leaf_owner: Option<ArenaBuildOwner>,
    wrapper_owner: Option<ArenaBuildOwner>,
    summary: Option<SequenceSummary>,
    phase: LeafPhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeafPhase {
    ParsePrograms,
    ValidateClosure,
    HashClosure,
    AllocatePrograms(usize),
    AllocateCanonicalLeaf,
    ReleasePrograms(usize),
    AllocateWrapper,
    ReleaseCanonicalLeaf,
    BeginPush,
    PollPush,
}

#[derive(Debug)]
struct ChunkCursor {
    chunk: PublicationChunk,
    hash_cursor: usize,
    hash: u128,
    decode_cursor: usize,
    decoded_leaves: u32,
    leaf: Option<LeafAssembly>,
}

#[derive(Debug)]
enum OfferPhase {
    Receiving,
    Finishing,
    NeedSplice {
        replacement: Option<ArenaBuildOwner>,
    },
    Splicing,
    NeedManifest {
        target_root: Option<ArenaBuildOwner>,
    },
    ReadySwap {
        manifest: ArenaBuildOwner,
        summary: SequenceSummary,
        manifest_digest: u128,
    },
}

#[derive(Debug)]
struct ActiveOffer {
    begin: OfferBegin,
    ticket: Option<ArenaBuildTicket>,
    builder: ResumableStreamingSequenceBuilder<HostSequenceSpec>,
    splice: ResumableSequenceSplice<HostSequenceSpec>,
    sequence_receipt: SequenceMutationReceipt,
    chunk: Option<ChunkCursor>,
    next_chunk: u64,
    received_chunks: u64,
    received_wire_bytes: u64,
    received_leaves: u64,
    inserted_summary: StreamSummary,
    rolling_transport_digest: u128,
    phase: OfferPhase,
}

#[derive(Debug)]
struct InstalledRoot {
    owner: OwnedArenaRef,
    sequence_root: Option<ArenaId>,
    ack: PublicationAck,
}

#[derive(Debug)]
struct StagedPublicationHost {
    arena: PageArena,
    current_source: SourceStamp,
    installed: Option<InstalledRoot>,
    active: Option<ActiveOffer>,
    aborting: Option<ArenaBuildId>,
    retired: [Option<OwnedArenaRef>; 2],
}

impl StagedPublicationHost {
    fn new(current_source: SourceStamp) -> Self {
        Self {
            arena: PageArena::new(),
            current_source,
            installed: None,
            active: None,
            aborting: None,
            retired: [None, None],
        }
    }

    fn begin_offer(&mut self, begin: OfferBegin) -> Result<(), StagingError> {
        if self.active.is_some() || self.aborting.is_some() {
            return Err(StagingError::Backpressure);
        }
        validate_begin(
            begin,
            self.current_source,
            self.installed.as_ref().map(|root| root.ack),
        )?;
        let ticket = self.arena.begin_build()?;
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let builder = match ResumableStreamingSequenceBuilder::try_new(&mut sequence_receipt) {
            Ok(builder) => builder,
            Err(error) => {
                let abort = self
                    .arena
                    .begin_build_abort(ticket)
                    .map_err(|failure| failure.error)?;
                debug_assert!(self.arena.poll_build_abort(abort, 0)?.complete);
                return Err(error);
            }
        };
        let splice = match ResumableSequenceSplice::try_preallocated_for_build(
            ticket.id(),
            &mut sequence_receipt,
        ) {
            Ok(splice) => splice,
            Err(error) => {
                let abort = self
                    .arena
                    .begin_build_abort(ticket)
                    .map_err(|failure| failure.error)?;
                debug_assert!(self.arena.poll_build_abort(abort, 0)?.complete);
                return Err(error);
            }
        };
        self.active = Some(ActiveOffer {
            begin,
            ticket: Some(ticket),
            builder,
            splice,
            sequence_receipt,
            chunk: None,
            next_chunk: 0,
            received_chunks: 0,
            received_wire_bytes: 0,
            received_leaves: 0,
            inserted_summary: StreamSummary::default(),
            rolling_transport_digest: 0,
            phase: OfferPhase::Receiving,
        });
        Ok(())
    }

    fn offer_chunk(&mut self, chunk: PublicationChunk) -> Result<(), StagingError> {
        let active = self
            .active
            .as_mut()
            .ok_or(StagingError::Invalid("no active offer"))?;
        if !matches!(active.phase, OfferPhase::Receiving) || active.chunk.is_some() {
            return Err(StagingError::Backpressure);
        }
        if chunk.offer != active.begin.offer
            || chunk.ordinal != active.next_chunk
            || chunk.first_leaf != active.received_leaves
            || chunk.payload.is_empty()
            || chunk.payload.len() > CHUNK_MAX_BYTES
        {
            return Err(StagingError::Invalid(
                "chunk identity, order, or size mismatch",
            ));
        }
        let next_chunks = active
            .received_chunks
            .checked_add(1)
            .ok_or(StagingError::Invalid("chunk count overflow"))?;
        let next_bytes = active
            .received_wire_bytes
            .checked_add(
                u64::try_from(chunk.payload.len())
                    .map_err(|_| StagingError::Invalid("chunk byte length exceeds u64"))?,
            )
            .ok_or(StagingError::Invalid("offer wire byte overflow"))?;
        if next_chunks > active.begin.maximum_chunks || next_bytes > active.begin.maximum_wire_bytes
        {
            return Err(StagingError::Invalid("offer transport envelope exceeded"));
        }
        active.chunk = Some(ChunkCursor {
            chunk,
            hash_cursor: 0,
            hash: 0,
            decode_cursor: 0,
            decoded_leaves: 0,
            leaf: None,
        });
        Ok(())
    }

    fn request_commit(&mut self, claim: CommitClaim) -> Result<(), StagingError> {
        let active = self
            .active
            .as_mut()
            .ok_or(StagingError::Invalid("no active offer"))?;
        if !matches!(active.phase, OfferPhase::Receiving) || active.chunk.is_some() {
            return Err(StagingError::Backpressure);
        }
        if claim.offer != active.begin.offer
            || claim.actual_chunks != active.received_chunks
            || claim.actual_wire_bytes != active.received_wire_bytes
            || claim.rolling_transport_digest != active.rolling_transport_digest
            || active.received_leaves != active.begin.inserted_leaves
            || claim.inserted_stream_digest != active.inserted_summary.content_digest
        {
            return Err(StagingError::Invalid("commit transport totals mismatch"));
        }
        if active.received_leaves == 0 {
            active.phase = if matches!(active.begin.mode, OfferMode::Snapshot) {
                OfferPhase::NeedManifest { target_root: None }
            } else {
                OfferPhase::NeedSplice { replacement: None }
            };
        } else {
            active.builder.begin_finish(&mut active.sequence_receipt)?;
            active.phase = OfferPhase::Finishing;
        }
        Ok(())
    }

    fn poll(
        &mut self,
        fuel: PublicationFuel,
    ) -> Result<(PublicationProgress, PublicationPollReceipt), StagingError> {
        let mut active = self
            .active
            .take()
            .ok_or(StagingError::Invalid("no active offer"))?;
        let ticket = active
            .ticket
            .take()
            .ok_or(StagingError::Invalid("offer lost build ticket"))?;
        let mut session = self
            .arena
            .resume_build(ticket)
            .map_err(|failure| failure.error)?;
        let base_sequence = self.installed.as_ref().and_then(|root| root.sequence_root);
        let before_branches = active.sequence_receipt.branches_allocated;
        let mut receipt = PublicationPollReceipt {
            maximum_live_chunk_bytes: active
                .chunk
                .as_ref()
                .map_or(0, |chunk| chunk.chunk.payload.len()),
            ..PublicationPollReceipt::default()
        };
        let progress =
            match advance_offer(&mut active, &mut session, base_sequence, fuel, &mut receipt) {
                Ok(progress) => progress,
                Err(error) => {
                    let abort = session.begin_abort()?;
                    self.aborting = Some(abort);
                    return Err(error);
                }
            };
        receipt.branches_allocated = active
            .sequence_receipt
            .branches_allocated
            .saturating_sub(before_branches);

        if matches!(progress, AdvanceProgress::ReadySwap) {
            let OfferPhase::ReadySwap {
                manifest,
                summary,
                manifest_digest,
            } = std::mem::replace(&mut active.phase, OfferPhase::Receiving)
            else {
                return Err(StagingError::Invalid("ready swap lost manifest"));
            };
            if self.current_source != active.begin.source
                || !base_is_still_exact(active.begin.mode, self.installed.as_ref())
            {
                let abort = session.begin_abort()?;
                self.aborting = Some(abort);
                return Err(StagingError::ExactCurrentMismatch);
            }
            let Some(retirement_slot) = self.retired.iter().position(Option::is_none) else {
                let abort = session.begin_abort()?;
                self.aborting = Some(abort);
                return Err(StagingError::Backpressure);
            };
            let sequence_root = session.arena().children(session.owner_id(&manifest)?)?[0];
            let owner = session.commit(manifest)?;
            let ack = PublicationAck {
                session: active.begin.session,
                target_revision: active.begin.target_revision,
                source: active.begin.source,
                summary,
                manifest_digest,
            };
            if let Some(old) = self.installed.take() {
                self.retired[retirement_slot] = Some(old.owner);
            }
            self.installed = Some(InstalledRoot {
                owner,
                sequence_root,
                ack,
            });
            receipt.committed = true;
            return Ok((PublicationProgress::Committed(ack), receipt));
        }

        let ticket = session.suspend()?;
        active.ticket = Some(ticket);
        let external = match progress {
            AdvanceProgress::Pending | AdvanceProgress::ReadySwap => PublicationProgress::Pending,
            AdvanceProgress::ChunkAccepted { ordinal } => {
                PublicationProgress::ChunkAccepted { ordinal }
            }
        };
        self.active = Some(active);
        Ok((external, receipt))
    }

    fn observe_source_advance(&mut self, next: SourceStamp) -> Result<(), StagingError> {
        if next.revision <= self.current_source.revision {
            return Err(StagingError::Invalid("source revision did not advance"));
        }
        self.current_source = next;
        if let Some(mut active) = self.active.take() {
            let ticket = active
                .ticket
                .take()
                .ok_or(StagingError::Invalid("offer lost build ticket"))?;
            let abort = self
                .arena
                .begin_build_abort(ticket)
                .map_err(|failure| failure.error)?;
            self.aborting = Some(abort);
        }
        Ok(())
    }

    fn abort_offer(&mut self) -> Result<(), StagingError> {
        let mut active = self
            .active
            .take()
            .ok_or(StagingError::Invalid("no active offer"))?;
        let ticket = active
            .ticket
            .take()
            .ok_or(StagingError::Invalid("offer lost build ticket"))?;
        self.aborting = Some(
            self.arena
                .begin_build_abort(ticket)
                .map_err(|failure| failure.error)?,
        );
        Ok(())
    }

    fn poll_disposal(&mut self, fuel: usize) -> Result<usize, StagingError> {
        let mut transitions = 0;
        if transitions < fuel {
            if let Some(abort) = self.aborting {
                let step = self.arena.poll_build_abort(abort, fuel - transitions)?;
                transitions += step.owners_scheduled;
                if step.complete {
                    self.aborting = None;
                }
            }
        }
        for slot in &mut self.retired {
            if transitions == fuel {
                break;
            }
            if let Some(owner) = slot.take() {
                self.arena
                    .release_later(owner)
                    .map_err(|failure| failure.error)?;
                transitions += 1;
            }
        }
        if transitions < fuel {
            let reclaimed = self
                .arena
                .poll_reclaim(fuel - transitions)
                .map_err(|failure| failure.error)?;
            transitions += reclaimed.reference_transitions;
        }
        Ok(transitions)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AdvanceProgress {
    Pending,
    ChunkAccepted { ordinal: u64 },
    ReadySwap,
}

fn advance_offer(
    active: &mut ActiveOffer,
    session: &mut ArenaBuildSession<'_>,
    base_sequence: Option<ArenaId>,
    mut fuel: PublicationFuel,
    receipt: &mut PublicationPollReceipt,
) -> Result<AdvanceProgress, StagingError> {
    loop {
        match active.phase {
            OfferPhase::Receiving => {
                let Some(mut chunk) = active.chunk.take() else {
                    return Ok(AdvanceProgress::Pending);
                };
                match advance_chunk(active, session, &mut chunk, &mut fuel, receipt)? {
                    ChunkProgress::Pending => {
                        active.chunk = Some(chunk);
                        return Ok(AdvanceProgress::Pending);
                    }
                    ChunkProgress::Complete => {
                        let ordinal = chunk.chunk.ordinal;
                        active.received_chunks += 1;
                        active.received_wire_bytes += u64::try_from(chunk.chunk.payload.len())
                            .map_err(|_| StagingError::Invalid("chunk length exceeds u64"))?;
                        active.rolling_transport_digest =
                            roll_transport_digest(active.rolling_transport_digest, &chunk.chunk);
                        active.next_chunk += 1;
                        return Ok(AdvanceProgress::ChunkAccepted { ordinal });
                    }
                }
            }
            OfferPhase::Finishing => {
                if fuel.transitions == 0 {
                    return Ok(AdvanceProgress::Pending);
                }
                fuel.transitions -= 1;
                receipt.transitions += 1;
                match active
                    .builder
                    .poll_finish(session, &mut active.sequence_receipt)?
                {
                    ResumableSequenceProgress::Pending => return Ok(AdvanceProgress::Pending),
                    ResumableSequenceProgress::Complete => {
                        let root = active.builder.take_root()?;
                        active.phase = if matches!(active.begin.mode, OfferMode::Snapshot) {
                            OfferPhase::NeedManifest {
                                target_root: Some(root),
                            }
                        } else {
                            OfferPhase::NeedSplice {
                                replacement: Some(root),
                            }
                        };
                    }
                }
            }
            OfferPhase::NeedSplice { .. } => {
                if fuel.transitions == 0 {
                    return Ok(AdvanceProgress::Pending);
                }
                let OfferPhase::NeedSplice { replacement } =
                    std::mem::replace(&mut active.phase, OfferPhase::Receiving)
                else {
                    unreachable!();
                };
                let OfferMode::Delta {
                    old_start,
                    old_delete,
                    deleted,
                    ..
                } = active.begin.mode
                else {
                    return Err(StagingError::Invalid(
                        "snapshot publication entered delta splice",
                    ));
                };
                let old_end = old_start
                    .checked_add(old_delete)
                    .ok_or(StagingError::Invalid("delta delete range overflow"))?;
                let working_root = base_sequence.map(|root| session.retain(root)).transpose()?;
                active.splice.begin_from_owned_validating_deleted(
                    session,
                    working_root,
                    old_start..old_end,
                    replacement,
                    SequenceDeletedSummaryGuard::new(
                        deleted.expected_summary(),
                        deleted_summaries_equivalent,
                    ),
                    &mut active.sequence_receipt,
                )?;
                active.phase = OfferPhase::Splicing;
                fuel.transitions -= 1;
                receipt.transitions += 1;
            }
            OfferPhase::Splicing => {
                if fuel.transitions == 0 {
                    return Ok(AdvanceProgress::Pending);
                }
                let progress = active.splice.poll(session, &mut active.sequence_receipt)?;
                fuel.transitions -= 1;
                receipt.transitions += 1;
                if progress == ResumableSequenceSplitProgress::Complete {
                    active.phase = OfferPhase::NeedManifest {
                        target_root: active.splice.take_root()?,
                    };
                } else {
                    return Ok(AdvanceProgress::Pending);
                }
            }
            OfferPhase::NeedManifest { .. } => {
                if fuel.transitions == 0 || fuel.copy_bytes < MANIFEST_BYTES {
                    return Ok(AdvanceProgress::Pending);
                }
                let OfferPhase::NeedManifest { target_root } =
                    std::mem::replace(&mut active.phase, OfferPhase::Receiving)
                else {
                    unreachable!();
                };
                let summary = target_root
                    .as_ref()
                    .map(|root| {
                        sequence_node::<HostSequenceSpec>(session.arena(), session.owner_id(root)?)
                            .map(|node| node.0)
                    })
                    .transpose()?
                    .unwrap_or_default();
                if summary.leaves != active.begin.target_leaf_count
                    || summary.metric != active.begin.target_metric
                    || summary.balance != 0
                    || summary.minimum_prefix < 0
                {
                    return Err(StagingError::Invalid(
                        "staging root disagrees with target shape or complete structure",
                    ));
                }
                let manifest_digest = manifest_digest(active.begin, summary);
                let payload = encode_manifest(active.begin, summary, manifest_digest);
                let children = target_root
                    .as_ref()
                    .map(|root| session.owner_id(root))
                    .transpose()?
                    .into_iter()
                    .collect::<Vec<_>>();
                let (manifest, _) = session.allocate(&payload, &children)?;
                receipt.pages_allocated += 1;
                receipt.transitions += 1;
                receipt.wire_bytes_copied += MANIFEST_BYTES;
                fuel.transitions -= 1;
                fuel.copy_bytes -= MANIFEST_BYTES;
                if let Some(root) = target_root {
                    session.release(root)?;
                }
                active.phase = OfferPhase::ReadySwap {
                    manifest,
                    summary,
                    manifest_digest,
                };
            }
            OfferPhase::ReadySwap { .. } => return Ok(AdvanceProgress::ReadySwap),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChunkProgress {
    Pending,
    Complete,
}

fn advance_chunk(
    active: &mut ActiveOffer,
    session: &mut ArenaBuildSession<'_>,
    chunk: &mut ChunkCursor,
    fuel: &mut PublicationFuel,
    receipt: &mut PublicationPollReceipt,
) -> Result<ChunkProgress, StagingError> {
    if chunk.hash_cursor != chunk.chunk.payload.len() {
        if fuel.inspect_bytes == 0 || fuel.transitions == 0 {
            return Ok(ChunkProgress::Pending);
        }
        let count = fuel
            .inspect_bytes
            .min(ARENA_PAGE_BYTES)
            .min(chunk.chunk.payload.len() - chunk.hash_cursor);
        for byte in &chunk.chunk.payload[chunk.hash_cursor..chunk.hash_cursor + count] {
            chunk.hash = hash_push(chunk.hash, *byte);
        }
        chunk.hash_cursor += count;
        fuel.inspect_bytes -= count;
        fuel.transitions -= 1;
        receipt.wire_bytes_hashed += count;
        receipt.transitions += 1;
        if chunk.hash_cursor == chunk.chunk.payload.len() && chunk.hash != chunk.chunk.digest {
            return Err(StagingError::Invalid("chunk digest mismatch"));
        }
        return Ok(ChunkProgress::Pending);
    }

    if let Some(mut leaf) = chunk.leaf.take() {
        let complete = advance_leaf(active, session, chunk, &mut leaf, fuel, receipt)?;
        if complete {
            chunk.decode_cursor = leaf.cursor;
            chunk.decoded_leaves += 1;
            active.received_leaves = active
                .received_leaves
                .checked_add(1)
                .ok_or(StagingError::Invalid("received leaf count overflow"))?;
            if active.received_leaves > active.begin.inserted_leaves {
                return Err(StagingError::Invalid("offer emitted too many leaves"));
            }
        } else {
            chunk.leaf = Some(leaf);
        }
        return Ok(ChunkProgress::Pending);
    }

    if chunk.decode_cursor == chunk.chunk.payload.len() {
        if chunk.decoded_leaves != chunk.chunk.leaf_count {
            return Err(StagingError::Invalid("chunk leaf count mismatch"));
        }
        return Ok(ChunkProgress::Complete);
    }
    if fuel.inspect_bytes < RECORD_HEADER_BYTES || fuel.transitions == 0 {
        return Ok(ChunkProgress::Pending);
    }
    let bytes = &chunk.chunk.payload;
    let start = chunk.decode_cursor;
    let end = start
        .checked_add(RECORD_HEADER_BYTES)
        .ok_or(StagingError::Invalid("record header overflow"))?;
    if end > bytes.len() || bytes[start] != RECORD_TAG {
        return Err(StagingError::Invalid("truncated or mistagged leaf record"));
    }
    let leaf_payload_len = usize::from(read_u16(bytes, start + 1)?);
    let program_count = usize::from(read_u16(bytes, start + 3)?);
    let closure_bytes = usize::try_from(read_u32(bytes, start + 5)?)
        .map_err(|_| StagingError::Invalid("closure length exceeds usize"))?;
    let leaf_id = read_u64(bytes, start + 9)?;
    let validator = CopiedGreenClosureValidator::try_new_summary_only(
        leaf_payload_len,
        program_count,
        closure_bytes,
    )?;
    let raw_start = end;
    let raw_end = raw_start
        .checked_add(leaf_payload_len)
        .ok_or(StagingError::Invalid("leaf payload overflow"))?;
    let closure_end = end
        .checked_add(closure_bytes)
        .ok_or(StagingError::Invalid("closure range overflow"))?;
    if raw_end > bytes.len()
        || closure_end > bytes.len()
        || closure_bytes < leaf_payload_len
        || closure_bytes > COPIED_GREEN_CLOSURE_MAX_BYTES
    {
        return Err(StagingError::Invalid("truncated closed leaf"));
    }
    let mut programs = Vec::new();
    programs
        .try_reserve_exact(program_count)
        .map_err(|_| StagingError::Invalid("Program-owner reservation failed"))?;
    chunk.leaf = Some(LeafAssembly {
        leaf_id,
        raw: raw_start..raw_end,
        cursor: raw_end,
        closure_end,
        program_count,
        programs,
        validator: Some(validator),
        decoded: None,
        content_hash_cursor: raw_start,
        closure_digest: hash_bytes(&leaf_id.to_le_bytes()),
        canonical_leaf_owner: None,
        wrapper_owner: None,
        summary: None,
        phase: LeafPhase::ParsePrograms,
    });
    fuel.inspect_bytes -= RECORD_HEADER_BYTES;
    fuel.transitions -= 1;
    receipt.wire_bytes_hashed += RECORD_HEADER_BYTES;
    receipt.transitions += 1;
    Ok(ChunkProgress::Pending)
}

fn advance_leaf(
    active: &mut ActiveOffer,
    session: &mut ArenaBuildSession<'_>,
    chunk: &ChunkCursor,
    leaf: &mut LeafAssembly,
    fuel: &mut PublicationFuel,
    receipt: &mut PublicationPollReceipt,
) -> Result<bool, StagingError> {
    match leaf.phase {
        LeafPhase::ParsePrograms => {
            if leaf.programs.len() == leaf.program_count {
                if leaf.cursor != leaf.closure_end {
                    return Err(StagingError::Invalid(
                        "closed leaf has trailing or missing Program bytes",
                    ));
                }
                leaf.phase = LeafPhase::ValidateClosure;
                return Ok(false);
            }
            if fuel.inspect_bytes < PROGRAM_LENGTH_BYTES || fuel.transitions == 0 {
                return Ok(false);
            }
            let length = usize::from(read_u16(&chunk.chunk.payload, leaf.cursor)?);
            if length > ARENA_PAGE_BYTES {
                return Err(StagingError::Invalid("Program page exceeds arena page"));
            }
            let start = leaf
                .cursor
                .checked_add(PROGRAM_LENGTH_BYTES)
                .ok_or(StagingError::Invalid("Program start overflow"))?;
            let end = start
                .checked_add(length)
                .ok_or(StagingError::Invalid("Program end overflow"))?;
            if end > leaf.closure_end || end > chunk.chunk.payload.len() {
                return Err(StagingError::Invalid("truncated Program page"));
            }
            leaf.programs.push(ProgramOwner {
                raw: start..end,
                owner: None,
            });
            leaf.cursor = end;
            fuel.inspect_bytes -= PROGRAM_LENGTH_BYTES;
            fuel.transitions -= 1;
            receipt.wire_bytes_hashed += PROGRAM_LENGTH_BYTES;
            receipt.transitions += 1;
            Ok(false)
        }
        LeafPhase::ValidateClosure => {
            let raw = &chunk.chunk.payload[leaf.raw.clone()];
            let programs = &leaf.programs;
            let validator = leaf.validator.as_mut().ok_or(StagingError::Invalid(
                "closed leaf lost its canonical validator",
            ))?;
            let (progress, validation) =
                validator.poll(
                    raw,
                    |ordinal| {
                        let range = programs.get(ordinal).ok_or(SerializedGreenError::Corrupt(
                            "copied Program ordinal is out of range",
                        ))?;
                        chunk.chunk.payload.get(range.raw.clone()).ok_or(
                            SerializedGreenError::Corrupt("copied Program range escaped closure"),
                        )
                    },
                    CopiedGreenValidationFuel {
                        inspect_bytes: fuel.inspect_bytes,
                        copy_bytes: fuel.copy_bytes,
                        transitions: fuel.transitions,
                    },
                )?;
            fuel.inspect_bytes = fuel
                .inspect_bytes
                .checked_sub(validation.inspected_bytes)
                .ok_or(StagingError::Invalid("canonical inspection exceeded fuel"))?;
            fuel.copy_bytes = fuel
                .copy_bytes
                .checked_sub(validation.copied_bytes)
                .ok_or(StagingError::Invalid("canonical copy exceeded fuel"))?;
            fuel.transitions = fuel
                .transitions
                .checked_sub(validation.transitions)
                .ok_or(StagingError::Invalid("canonical transitions exceeded fuel"))?;
            receipt.wire_bytes_hashed += validation.inspected_bytes;
            receipt.wire_bytes_copied += validation.copied_bytes;
            receipt.transitions += validation.transitions;
            if progress == CopiedGreenValidationProgress::Complete {
                leaf.decoded = Some(validator.take_decoded()?);
                leaf.validator = None;
                leaf.phase = LeafPhase::HashClosure;
            }
            Ok(false)
        }
        LeafPhase::HashClosure => {
            if leaf.content_hash_cursor == leaf.closure_end {
                let decoded = leaf.decoded.as_ref().ok_or(StagingError::Invalid(
                    "validated closure lost its canonical summary",
                ))?;
                leaf.summary = Some(SequenceSummary::leaf(
                    decoded.summary.metric.into(),
                    leaf.closure_digest,
                    decoded.summary.balance,
                    decoded.summary.minimum_prefix,
                ));
                leaf.phase = LeafPhase::AllocatePrograms(0);
                return Ok(false);
            }
            if fuel.inspect_bytes == 0 || fuel.transitions == 0 {
                return Ok(false);
            }
            let count = fuel
                .inspect_bytes
                .min(ARENA_PAGE_BYTES)
                .min(leaf.closure_end - leaf.content_hash_cursor);
            for byte in
                &chunk.chunk.payload[leaf.content_hash_cursor..leaf.content_hash_cursor + count]
            {
                leaf.closure_digest = hash_push(leaf.closure_digest, *byte);
            }
            leaf.content_hash_cursor += count;
            fuel.inspect_bytes -= count;
            fuel.transitions -= 1;
            receipt.wire_bytes_hashed += count;
            receipt.transitions += 1;
            Ok(false)
        }
        LeafPhase::AllocatePrograms(index) => {
            if index == leaf.programs.len() {
                leaf.phase = LeafPhase::AllocateCanonicalLeaf;
                return Ok(false);
            }
            let raw = leaf.programs[index].raw.clone();
            let required = raw.len();
            if fuel.copy_bytes < required || fuel.transitions == 0 {
                return Ok(false);
            }
            let (owner, _) = session.allocate(&chunk.chunk.payload[raw], &[])?;
            leaf.programs[index].owner = Some(owner);
            leaf.phase = LeafPhase::AllocatePrograms(index + 1);
            fuel.copy_bytes -= required;
            fuel.transitions -= 1;
            receipt.wire_bytes_copied += required;
            receipt.transitions += 1;
            receipt.pages_allocated += 1;
            Ok(false)
        }
        LeafPhase::AllocateCanonicalLeaf => {
            let raw = &chunk.chunk.payload[leaf.raw.clone()];
            let edge_bytes = leaf
                .programs
                .len()
                .checked_mul(std::mem::size_of::<ArenaId>())
                .ok_or(StagingError::Invalid("canonical leaf edge bytes overflow"))?;
            let required = raw
                .len()
                .checked_add(edge_bytes)
                .ok_or(StagingError::Invalid("canonical leaf copy work overflow"))?;
            if fuel.copy_bytes < required || fuel.transitions == 0 {
                return Ok(false);
            }
            let mut children = [ArenaId::default(); MAX_PACKED_ARENA_CHILDREN];
            for (index, program) in leaf.programs.iter().enumerate() {
                let owner = program
                    .owner
                    .as_ref()
                    .ok_or(StagingError::Invalid("Program owner disappeared"))?;
                children[index] = session.owner_id(owner)?;
            }
            let (owner, _) = session.allocate_packed(raw, &children[..leaf.programs.len()])?;
            leaf.canonical_leaf_owner = Some(owner);
            leaf.phase = LeafPhase::ReleasePrograms(0);
            fuel.copy_bytes -= required;
            fuel.transitions -= 1;
            receipt.wire_bytes_copied += required;
            receipt.transitions += 1;
            receipt.pages_allocated += 1;
            Ok(false)
        }
        LeafPhase::ReleasePrograms(index) => {
            if index == leaf.programs.len() {
                leaf.phase = LeafPhase::AllocateWrapper;
                return Ok(false);
            }
            if fuel.transitions == 0 {
                return Ok(false);
            }
            let owner = leaf.programs[index]
                .owner
                .take()
                .ok_or(StagingError::Invalid("Program owner already released"))?;
            session.release(owner)?;
            leaf.phase = LeafPhase::ReleasePrograms(index + 1);
            fuel.transitions -= 1;
            receipt.transitions += 1;
            Ok(false)
        }
        LeafPhase::AllocateWrapper => {
            let required = LEAF_BYTES + std::mem::size_of::<ArenaId>();
            if fuel.copy_bytes < required || fuel.transitions == 0 {
                return Ok(false);
            }
            let summary = leaf
                .summary
                .ok_or(StagingError::Invalid("leaf wrapper lost its summary"))?;
            let mut payload = [0_u8; LEAF_BYTES];
            payload[..SUMMARY_BYTES].copy_from_slice(&encode_summary(LEAF_TAG, summary));
            payload[SUMMARY_BYTES..].copy_from_slice(&leaf.leaf_id.to_le_bytes());
            let canonical = leaf
                .canonical_leaf_owner
                .as_ref()
                .ok_or(StagingError::Invalid(
                    "leaf wrapper lost its canonical child",
                ))?;
            let child = session.owner_id(canonical)?;
            let (owner, _) = session.allocate(&payload, &[child])?;
            leaf.wrapper_owner = Some(owner);
            leaf.phase = LeafPhase::ReleaseCanonicalLeaf;
            fuel.copy_bytes -= required;
            fuel.transitions -= 1;
            receipt.wire_bytes_copied += required;
            receipt.transitions += 1;
            receipt.pages_allocated += 1;
            Ok(false)
        }
        LeafPhase::ReleaseCanonicalLeaf => {
            if fuel.transitions == 0 {
                return Ok(false);
            }
            let owner = leaf
                .canonical_leaf_owner
                .take()
                .ok_or(StagingError::Invalid(
                    "canonical leaf owner already released",
                ))?;
            session.release(owner)?;
            leaf.phase = LeafPhase::BeginPush;
            fuel.transitions -= 1;
            receipt.transitions += 1;
            Ok(false)
        }
        LeafPhase::BeginPush => {
            if fuel.transitions == 0 {
                return Ok(false);
            }
            let owner = leaf
                .wrapper_owner
                .take()
                .ok_or(StagingError::Invalid("leaf wrapper owner disappeared"))?;
            active
                .builder
                .begin_push(session, owner, &mut active.sequence_receipt)?;
            leaf.phase = LeafPhase::PollPush;
            fuel.transitions -= 1;
            receipt.transitions += 1;
            Ok(false)
        }
        LeafPhase::PollPush => {
            if fuel.transitions == 0 {
                return Ok(false);
            }
            let complete = active
                .builder
                .poll_push(session, &mut active.sequence_receipt)?
                == ResumableSequenceProgress::Complete;
            if complete {
                let summary = leaf
                    .summary
                    .take()
                    .ok_or(StagingError::Invalid("completed leaf lost summary"))?;
                active.inserted_summary = active.inserted_summary.append_leaf(summary)?;
            }
            fuel.transitions -= 1;
            receipt.transitions += 1;
            Ok(complete)
        }
    }
}

fn validate_begin(
    begin: OfferBegin,
    current: SourceStamp,
    installed: Option<PublicationAck>,
) -> Result<(), StagingError> {
    if begin.offer == 0
        || begin.session == 0
        || begin.target_revision == 0
        || begin.source != current
        || begin.target_metric != begin.source.metric
        || begin.maximum_chunks == 0
        || begin.maximum_chunks > OFFER_MAX_CHUNKS
        || begin.maximum_wire_bytes == 0
        || begin.maximum_wire_bytes > OFFER_MAX_WIRE_BYTES
    {
        return Err(StagingError::ExactCurrentMismatch);
    }
    match begin.mode {
        OfferMode::Snapshot => {
            if begin.target_leaf_count != begin.inserted_leaves {
                return Err(StagingError::Invalid("snapshot target leaf count mismatch"));
            }
        }
        OfferMode::Delta {
            base,
            old_start,
            old_delete,
            deleted,
        } => {
            if installed != Some(base)
                || base.session != begin.session
                || begin.target_revision <= base.target_revision
            {
                return Err(StagingError::BaseMismatch);
            }
            let old_end = old_start
                .checked_add(old_delete)
                .ok_or(StagingError::Invalid("delta delete range overflow"))?;
            if deleted.leaves != old_delete
                || (old_delete == 0 && deleted != DeletedRangeProof::default())
                || old_end > base.summary.leaves
                || base
                    .summary
                    .leaves
                    .checked_sub(old_delete)
                    .and_then(|leaves| leaves.checked_add(begin.inserted_leaves))
                    != Some(begin.target_leaf_count)
            {
                return Err(StagingError::Invalid("delta target leaf count mismatch"));
            }
        }
    }
    Ok(())
}

fn deleted_summaries_equivalent(
    actual: Option<SequenceSummary>,
    expected: Option<SequenceSummary>,
) -> bool {
    match (actual, expected) {
        (None, None) => true,
        (Some(actual), Some(expected)) => {
            actual.leaves == expected.leaves
                && actual.metric == expected.metric
                && actual.content_digest == expected.content_digest
                && actual.polynomial_factor == expected.polynomial_factor
                && actual.balance == expected.balance
                && actual.minimum_prefix == expected.minimum_prefix
        }
        (None, Some(_)) | (Some(_), None) => false,
    }
}

fn base_is_still_exact(mode: OfferMode, installed: Option<&InstalledRoot>) -> bool {
    match mode {
        OfferMode::Snapshot => true,
        OfferMode::Delta { base, .. } => installed.is_some_and(|root| root.ack == base),
    }
}

fn encode_summary(tag: u8, summary: SequenceSummary) -> [u8; BRANCH_BYTES] {
    let mut output = [0_u8; BRANCH_BYTES];
    output[0] = tag;
    output[1..9].copy_from_slice(&summary.leaves.to_le_bytes());
    output[9..17].copy_from_slice(&summary.metric.bytes.to_le_bytes());
    output[17..25].copy_from_slice(&summary.metric.utf16.to_le_bytes());
    output[25..41].copy_from_slice(&summary.content_digest.to_le_bytes());
    output[41..57].copy_from_slice(&summary.polynomial_factor.to_le_bytes());
    output[57..59].copy_from_slice(&summary.height.to_le_bytes());
    output[59..67].copy_from_slice(&summary.balance.to_le_bytes());
    output[67..75].copy_from_slice(&summary.minimum_prefix.to_le_bytes());
    output
}

fn decode_summary(payload: &[u8]) -> Result<SequenceSummary, StagingError> {
    Ok(SequenceSummary {
        leaves: read_u64(payload, 1)?,
        metric: Metric {
            bytes: read_u64(payload, 9)?,
            utf16: read_u64(payload, 17)?,
        },
        content_digest: read_u128(payload, 25)?,
        polynomial_factor: read_u128(payload, 41)?,
        height: read_u16(payload, 57)?,
        balance: read_i64(payload, 59)?,
        minimum_prefix: read_i64(payload, 67)?,
    })
}

fn encode_manifest(
    begin: OfferBegin,
    summary: SequenceSummary,
    digest: u128,
) -> [u8; MANIFEST_BYTES] {
    let mut output = [0_u8; MANIFEST_BYTES];
    output[0] = MANIFEST_TAG;
    output[1..17].copy_from_slice(&begin.session.to_le_bytes());
    output[17..25].copy_from_slice(&begin.target_revision.to_le_bytes());
    output[25..33].copy_from_slice(&begin.source.revision.to_le_bytes());
    output[33..41].copy_from_slice(&summary.leaves.to_le_bytes());
    output[41..49].copy_from_slice(&summary.metric.bytes.to_le_bytes());
    output[49..57].copy_from_slice(&summary.metric.utf16.to_le_bytes());
    output[57..73].copy_from_slice(&summary.content_digest.to_le_bytes());
    output[73..81].copy_from_slice(&(digest as u64).to_le_bytes());
    output
}

fn manifest_digest(begin: OfferBegin, summary: SequenceSummary) -> u128 {
    let mut digest = 0;
    for bytes in [
        begin.session.to_le_bytes().as_slice(),
        begin.target_revision.to_le_bytes().as_slice(),
        begin.source.revision.to_le_bytes().as_slice(),
        summary.leaves.to_le_bytes().as_slice(),
        summary.metric.bytes.to_le_bytes().as_slice(),
        summary.metric.utf16.to_le_bytes().as_slice(),
        summary.content_digest.to_le_bytes().as_slice(),
    ] {
        for byte in bytes {
            digest = hash_push(digest, *byte);
        }
    }
    digest
}

fn roll_transport_digest(prior: u128, chunk: &PublicationChunk) -> u128 {
    let mut digest = prior;
    for bytes in [
        chunk.ordinal.to_le_bytes().as_slice(),
        chunk.first_leaf.to_le_bytes().as_slice(),
        chunk.leaf_count.to_le_bytes().as_slice(),
        u64::try_from(chunk.payload.len())
            .unwrap_or(u64::MAX)
            .to_le_bytes()
            .as_slice(),
        chunk.digest.to_le_bytes().as_slice(),
    ] {
        for byte in bytes {
            digest = hash_push(digest, *byte);
        }
    }
    digest
}

const fn hash_push(state: u128, byte: u8) -> u128 {
    state.wrapping_mul(HASH_BASE).wrapping_add(byte as u128 + 1)
}

fn hash_bytes(bytes: &[u8]) -> u128 {
    bytes.iter().fold(0, |state, byte| hash_push(state, *byte))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, StagingError> {
    let end = offset
        .checked_add(2)
        .ok_or(StagingError::Invalid("u16 range overflow"))?;
    let raw: [u8; 2] = bytes
        .get(offset..end)
        .ok_or(StagingError::Invalid("truncated u16"))?
        .try_into()
        .map_err(|_| StagingError::Invalid("malformed u16"))?;
    Ok(u16::from_le_bytes(raw))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, StagingError> {
    let end = offset
        .checked_add(4)
        .ok_or(StagingError::Invalid("u32 range overflow"))?;
    let raw: [u8; 4] = bytes
        .get(offset..end)
        .ok_or(StagingError::Invalid("truncated u32"))?
        .try_into()
        .map_err(|_| StagingError::Invalid("malformed u32"))?;
    Ok(u32::from_le_bytes(raw))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, StagingError> {
    let end = offset
        .checked_add(8)
        .ok_or(StagingError::Invalid("u64 range overflow"))?;
    let raw: [u8; 8] = bytes
        .get(offset..end)
        .ok_or(StagingError::Invalid("truncated u64"))?
        .try_into()
        .map_err(|_| StagingError::Invalid("malformed u64"))?;
    Ok(u64::from_le_bytes(raw))
}

fn read_i64(bytes: &[u8], offset: usize) -> Result<i64, StagingError> {
    let end = offset
        .checked_add(8)
        .ok_or(StagingError::Invalid("i64 range overflow"))?;
    let raw: [u8; 8] = bytes
        .get(offset..end)
        .ok_or(StagingError::Invalid("truncated i64"))?
        .try_into()
        .map_err(|_| StagingError::Invalid("malformed i64"))?;
    Ok(i64::from_le_bytes(raw))
}

fn read_u128(bytes: &[u8], offset: usize) -> Result<u128, StagingError> {
    let end = offset
        .checked_add(16)
        .ok_or(StagingError::Invalid("u128 range overflow"))?;
    let raw: [u8; 16] = bytes
        .get(offset..end)
        .ok_or(StagingError::Invalid("truncated u128"))?
        .try_into()
        .map_err(|_| StagingError::Invalid("malformed u128"))?;
    Ok(u128::from_le_bytes(raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistent_sequence::SequenceNodeKind;
    use crate::serialized_green::{
        serialized_green_staging_test_closure, serialized_green_staging_test_max_page_closure,
        serialized_green_staging_test_structural_closure,
        serialized_green_staging_test_zero_program_closure, validate_copied_green_leaf_closure,
    };
    use std::time::Instant;

    const TEST_FUEL: PublicationFuel = PublicationFuel {
        inspect_bytes: 16 * 1024,
        copy_bytes: 16 * 1024,
        transitions: 64,
    };
    const MAX_INLINE_FACT_TEST_FUEL: usize = 256;

    struct SnapshotProducer {
        offer: u64,
        leaf_count: u64,
        first_leaf_id: u64,
        logical_leaf_bytes: u64,
        leaf_payload: Arc<[u8]>,
        programs: Vec<Arc<[u8]>>,
        structural: crate::serialized_green::CopiedGreenLeafSummary,
        next_leaf: u64,
        next_chunk: u64,
        chunks: u64,
        wire_bytes: u64,
        rolling_transport_digest: u128,
        inserted_summary: StreamSummary,
    }

    impl SnapshotProducer {
        fn new(offer: u64, leaf_count: u64, logical_leaf_bytes: u64) -> Self {
            Self::new_with_first_leaf_id(offer, leaf_count, 1, logical_leaf_bytes)
        }

        fn new_with_first_leaf_id(
            offer: u64,
            leaf_count: u64,
            first_leaf_id: u64,
            logical_leaf_bytes: u64,
        ) -> Self {
            let (leaf_payload, programs) =
                serialized_green_staging_test_closure(SerializedMetric {
                    bytes: logical_leaf_bytes,
                    utf16: logical_leaf_bytes,
                });
            let program_refs = programs.iter().map(Vec::as_slice).collect::<Vec<_>>();
            let structural = validate_copied_green_leaf_closure(&leaf_payload, &program_refs)
                .unwrap()
                .summary;
            Self {
                offer,
                leaf_count,
                first_leaf_id,
                logical_leaf_bytes,
                leaf_payload: leaf_payload.into(),
                programs: programs.into_iter().map(Arc::from).collect(),
                structural,
                next_leaf: 0,
                next_chunk: 0,
                chunks: 0,
                wire_bytes: 0,
                rolling_transport_digest: 0,
                inserted_summary: StreamSummary::default(),
            }
        }

        fn next_chunk(&mut self) -> Option<PublicationChunk> {
            if self.next_leaf == self.leaf_count {
                return None;
            }
            let first_leaf = self.next_leaf;
            let mut payload = Vec::new();
            let mut leaves = 0_u32;
            while self.next_leaf != self.leaf_count {
                let record_len = RECORD_HEADER_BYTES
                    + self.leaf_payload.len()
                    + self
                        .programs
                        .iter()
                        .map(|program| PROGRAM_LENGTH_BYTES + program.len())
                        .sum::<usize>();
                if !payload.is_empty() && payload.len() + record_len > CHUNK_MAX_BYTES {
                    break;
                }
                let record = self.record(self.next_leaf);
                assert!(record.len() <= CHUNK_MAX_BYTES);
                payload.extend_from_slice(&record);
                self.next_leaf += 1;
                leaves += 1;
            }
            let payload: Arc<[u8]> = payload.into();
            let chunk = PublicationChunk {
                offer: self.offer,
                ordinal: self.next_chunk,
                first_leaf,
                leaf_count: leaves,
                digest: hash_bytes(&payload),
                payload,
            };
            self.next_chunk += 1;
            self.chunks += 1;
            self.wire_bytes += u64::try_from(chunk.payload.len()).unwrap();
            self.rolling_transport_digest =
                roll_transport_digest(self.rolling_transport_digest, &chunk);
            Some(chunk)
        }

        fn record(&mut self, leaf_index: u64) -> Vec<u8> {
            let leaf_id = self.first_leaf_id.checked_add(leaf_index).unwrap();
            let metric = Metric {
                bytes: self.logical_leaf_bytes,
                utf16: self.logical_leaf_bytes,
            };
            let mut closure_digest = hash_bytes(&leaf_id.to_le_bytes());
            let closure_bytes = self.leaf_payload.len()
                + self
                    .programs
                    .iter()
                    .map(|program| PROGRAM_LENGTH_BYTES + program.len())
                    .sum::<usize>();
            let mut output = Vec::with_capacity(RECORD_HEADER_BYTES + closure_bytes);
            output.push(RECORD_TAG);
            output.extend_from_slice(
                &u16::try_from(self.leaf_payload.len())
                    .unwrap()
                    .to_le_bytes(),
            );
            output.extend_from_slice(&u16::try_from(self.programs.len()).unwrap().to_le_bytes());
            output.extend_from_slice(&u32::try_from(closure_bytes).unwrap().to_le_bytes());
            output.extend_from_slice(&leaf_id.to_le_bytes());
            debug_assert_eq!(output.len(), RECORD_HEADER_BYTES);
            output.extend_from_slice(&self.leaf_payload);
            for program in &self.programs {
                output.extend_from_slice(&u16::try_from(program.len()).unwrap().to_le_bytes());
                output.extend_from_slice(&program);
            }
            for byte in &output[RECORD_HEADER_BYTES..] {
                closure_digest = hash_push(closure_digest, *byte);
            }
            self.inserted_summary = self
                .inserted_summary
                .append_leaf(SequenceSummary::leaf(
                    metric,
                    closure_digest,
                    self.structural.balance,
                    self.structural.minimum_prefix,
                ))
                .unwrap();
            output
        }

        fn claim(&self) -> CommitClaim {
            CommitClaim {
                offer: self.offer,
                actual_chunks: self.chunks,
                actual_wire_bytes: self.wire_bytes,
                rolling_transport_digest: self.rolling_transport_digest,
                inserted_stream_digest: self.inserted_summary.content_digest,
            }
        }
    }

    fn begin_for(source: SourceStamp, offer: u64, session: u128, leaf_count: u64) -> OfferBegin {
        OfferBegin {
            offer,
            session,
            target_revision: offer,
            source,
            mode: OfferMode::Snapshot,
            inserted_leaves: leaf_count,
            target_leaf_count: leaf_count,
            target_metric: source.metric,
            maximum_chunks: OFFER_MAX_CHUNKS,
            maximum_wire_bytes: OFFER_MAX_WIRE_BYTES,
        }
    }

    fn begin_delta_for(
        source: SourceStamp,
        offer: u64,
        base: PublicationAck,
        old_start: u64,
        old_delete: u64,
        deleted: DeletedRangeProof,
        inserted_leaves: u64,
        target_leaf_count: u64,
    ) -> OfferBegin {
        OfferBegin {
            offer,
            session: base.session,
            target_revision: offer,
            source,
            mode: OfferMode::Delta {
                base,
                old_start,
                old_delete,
                deleted,
            },
            inserted_leaves,
            target_leaf_count,
            target_metric: source.metric,
            maximum_chunks: OFFER_MAX_CHUNKS,
            maximum_wire_bytes: OFFER_MAX_WIRE_BYTES,
        }
    }

    fn installed_leaf_entries(
        host: &StagedPublicationHost,
    ) -> Vec<(ArenaId, u64, SequenceSummary)> {
        fn visit(
            arena: &PageArena,
            node: ArenaId,
            output: &mut Vec<(ArenaId, u64, SequenceSummary)>,
        ) {
            let (summary, kind) = sequence_node::<HostSequenceSpec>(arena, node).unwrap();
            match kind {
                SequenceNodeKind::Leaf => {
                    let payload = arena.payload(node).unwrap();
                    output.push((node, read_u64(payload, SUMMARY_BYTES).unwrap(), summary));
                }
                SequenceNodeKind::Branch { left, right } => {
                    visit(arena, left, output);
                    visit(arena, right, output);
                }
            }
        }

        let mut output = Vec::new();
        if let Some(root) = host
            .installed
            .as_ref()
            .and_then(|installed| installed.sequence_root)
        {
            visit(&host.arena, root, &mut output);
        }
        output
    }

    fn deleted_proof_from_entries(
        entries: &[(ArenaId, u64, SequenceSummary)],
        start: usize,
        count: usize,
    ) -> DeletedRangeProof {
        let mut summaries = entries[start..start + count].iter().map(|entry| entry.2);
        let Some(first) = summaries.next() else {
            return DeletedRangeProof::default();
        };
        let summary = summaries
            .try_fold(first, |left, right| left.combine(right))
            .unwrap();
        DeletedRangeProof::from_summary(summary)
    }

    fn offer_all_chunks(host: &mut StagedPublicationHost, producer: &mut SnapshotProducer) {
        while let Some(chunk) = producer.next_chunk() {
            let ordinal = chunk.ordinal;
            host.offer_chunk(chunk).unwrap();
            let mut maximum_buffer = 0;
            let mut maximum_inspect = 0;
            let mut maximum_copy = 0;
            let mut maximum_transitions = 0;
            drain_chunk(
                host,
                ordinal,
                &mut maximum_buffer,
                &mut maximum_inspect,
                &mut maximum_copy,
                &mut maximum_transitions,
            );
        }
    }

    fn commit_claim_with_fuel(
        host: &mut StagedPublicationHost,
        claim: CommitClaim,
        fuel: PublicationFuel,
    ) -> (PublicationAck, usize) {
        host.request_commit(claim).unwrap();
        let mut polls = 0;
        loop {
            let (progress, receipt) = host.poll(fuel).unwrap();
            polls += 1;
            assert!(receipt.wire_bytes_hashed <= fuel.inspect_bytes);
            assert!(receipt.wire_bytes_copied <= fuel.copy_bytes);
            assert!(receipt.transitions <= fuel.transitions);
            assert!(receipt.branches_allocated <= 1);
            if let PublicationProgress::Committed(ack) = progress {
                return (ack, polls);
            }
        }
    }

    fn record_from_canonical_closure(
        leaf_id: u64,
        leaf_payload: &[u8],
        programs: &[Vec<u8>],
    ) -> (Vec<u8>, SequenceSummary) {
        let program_refs = programs.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let decoded = validate_copied_green_leaf_closure(leaf_payload, &program_refs).unwrap();
        let closure_bytes = leaf_payload.len()
            + programs
                .iter()
                .map(|program| PROGRAM_LENGTH_BYTES + program.len())
                .sum::<usize>();
        let mut output = Vec::with_capacity(RECORD_HEADER_BYTES + closure_bytes);
        output.push(RECORD_TAG);
        output.extend_from_slice(&u16::try_from(leaf_payload.len()).unwrap().to_le_bytes());
        output.extend_from_slice(&u16::try_from(programs.len()).unwrap().to_le_bytes());
        output.extend_from_slice(&u32::try_from(closure_bytes).unwrap().to_le_bytes());
        output.extend_from_slice(&leaf_id.to_le_bytes());
        output.extend_from_slice(leaf_payload);
        for program in programs {
            output.extend_from_slice(&u16::try_from(program.len()).unwrap().to_le_bytes());
            output.extend_from_slice(program);
        }
        let mut digest = hash_bytes(&leaf_id.to_le_bytes());
        for byte in &output[RECORD_HEADER_BYTES..] {
            digest = hash_push(digest, *byte);
        }
        (
            output,
            SequenceSummary::leaf(
                decoded.summary.metric.into(),
                digest,
                decoded.summary.balance,
                decoded.summary.minimum_prefix,
            ),
        )
    }

    fn drain_chunk(
        host: &mut StagedPublicationHost,
        expected_ordinal: u64,
        maximum_buffer: &mut usize,
        maximum_poll_inspect_bytes: &mut usize,
        maximum_poll_copy_bytes: &mut usize,
        maximum_poll_transitions: &mut usize,
    ) {
        loop {
            let (progress, receipt) = host.poll(TEST_FUEL).unwrap();
            *maximum_buffer = (*maximum_buffer).max(receipt.maximum_live_chunk_bytes);
            *maximum_poll_inspect_bytes =
                (*maximum_poll_inspect_bytes).max(receipt.wire_bytes_hashed);
            *maximum_poll_copy_bytes = (*maximum_poll_copy_bytes).max(receipt.wire_bytes_copied);
            *maximum_poll_transitions = (*maximum_poll_transitions).max(receipt.transitions);
            assert!(receipt.wire_bytes_hashed <= TEST_FUEL.inspect_bytes);
            assert!(receipt.wire_bytes_copied <= TEST_FUEL.copy_bytes);
            assert!(receipt.transitions <= TEST_FUEL.transitions);
            if matches!(
                progress,
                PublicationProgress::ChunkAccepted { ordinal }
                    if ordinal == expected_ordinal
            ) {
                break;
            }
        }
    }

    fn commit_producer(
        host: &mut StagedPublicationHost,
        producer: &SnapshotProducer,
    ) -> (PublicationAck, PublicationPollReceipt) {
        host.request_commit(producer.claim()).unwrap();
        loop {
            let (progress, receipt) = host.poll(TEST_FUEL).unwrap();
            assert!(receipt.wire_bytes_hashed <= TEST_FUEL.inspect_bytes);
            assert!(receipt.wire_bytes_copied <= TEST_FUEL.copy_bytes);
            assert!(receipt.transitions <= TEST_FUEL.transitions);
            if let PublicationProgress::Committed(ack) = progress {
                return (ack, receipt);
            }
        }
    }

    #[test]
    fn hundred_mib_snapshot_streams_once_with_one_buffer_and_constant_final_work() {
        const LEAF_LOGICAL_BYTES: u64 = 3_820;
        const LEAVES: u64 = 27_450;
        const LOGICAL_BYTES: u64 = LEAVES * LEAF_LOGICAL_BYTES;
        let leaves = LEAVES;
        assert!(LOGICAL_BYTES > 100 * 1024 * 1024);
        assert!(leaves > 8_192, "fixture must exceed the old object cap");
        let source = SourceStamp {
            revision: 1,
            metric: Metric {
                bytes: LOGICAL_BYTES,
                utf16: LOGICAL_BYTES,
            },
            content_hash: 0x100,
        };
        let mut host = StagedPublicationHost::new(source);
        host.begin_offer(begin_for(source, 1, 0xabc, leaves))
            .unwrap();
        let mut producer = SnapshotProducer::new(1, leaves, LEAF_LOGICAL_BYTES);
        let started = Instant::now();
        let mut maximum_buffer = 0;
        let mut maximum_poll_inspect_bytes = 0;
        let mut maximum_poll_copy_bytes = 0;
        let mut maximum_poll_transitions = 0;
        while let Some(chunk) = producer.next_chunk() {
            let ordinal = chunk.ordinal;
            host.offer_chunk(chunk).unwrap();
            drain_chunk(
                &mut host,
                ordinal,
                &mut maximum_buffer,
                &mut maximum_poll_inspect_bytes,
                &mut maximum_poll_copy_bytes,
                &mut maximum_poll_transitions,
            );
        }
        assert!(
            producer.wire_bytes > 100 * 1024 * 1024,
            "fixture must exceed 100 MiB on both logical and wire axes"
        );
        let (ack, publication_poll_receipt) = commit_producer(&mut host, &producer);
        assert_eq!(ack.summary.leaves, leaves);
        assert_eq!(ack.summary.metric, source.metric);
        assert_eq!(
            ack.summary.content_digest,
            producer.inserted_summary.content_digest
        );
        assert!(maximum_buffer <= CHUNK_MAX_BYTES);
        assert!(maximum_poll_inspect_bytes <= TEST_FUEL.inspect_bytes);
        assert!(maximum_poll_copy_bytes <= TEST_FUEL.copy_bytes);
        assert!(maximum_poll_transitions <= TEST_FUEL.transitions);
        assert!(
            publication_poll_receipt.transitions <= 2,
            "last preparation plus atomic-swap poll must not scale with leaves"
        );
        assert!(publication_poll_receipt.wire_bytes_copied <= MANIFEST_BYTES);
        assert_eq!(host.active.is_some(), false);
        eprintln!(
            "staged-publication-100mib logical_bytes={} leaves={} chunks={} wire_bytes={} max_live_chunk={} max_poll_inspect_bytes={} max_poll_copy_bytes={} max_poll_transitions={} publication_poll_transitions={} publication_poll_copy_bytes={} elapsed_ms={} arena_high_water_bytes={}",
            LOGICAL_BYTES,
            leaves,
            producer.chunks,
            producer.wire_bytes,
            maximum_buffer,
            maximum_poll_inspect_bytes,
            maximum_poll_copy_bytes,
            maximum_poll_transitions,
            publication_poll_receipt.transitions,
            publication_poll_receipt.wire_bytes_copied,
            started.elapsed().as_millis(),
            host.arena.metrics().high_water_storage_bytes,
        );
    }

    fn install_small_snapshot(
        host: &mut StagedPublicationHost,
        source: SourceStamp,
        offer: u64,
        session: u128,
    ) -> PublicationAck {
        install_snapshot(host, source, offer, session, 2, source.metric.bytes / 2)
    }

    fn install_snapshot(
        host: &mut StagedPublicationHost,
        source: SourceStamp,
        offer: u64,
        session: u128,
        leaves: u64,
        logical_leaf_bytes: u64,
    ) -> PublicationAck {
        host.begin_offer(begin_for(source, offer, session, leaves))
            .unwrap();
        let mut producer = SnapshotProducer::new(offer, leaves, logical_leaf_bytes);
        offer_all_chunks(host, &mut producer);
        commit_producer(host, &producer).0
    }

    #[test]
    fn authenticated_middle_delta_reuses_exact_prefix_and_suffix_leaf_pages_with_fuel_one() {
        let source1 = SourceStamp {
            revision: 1,
            metric: Metric {
                bytes: 8 * 32,
                utf16: 8 * 32,
            },
            content_hash: 1,
        };
        let mut host = StagedPublicationHost::new(source1);
        let base = install_snapshot(&mut host, source1, 1, 0xabc, 8, 32);
        let base_root = host.installed.as_ref().unwrap().sequence_root;
        let base_entries = installed_leaf_entries(&host);
        let deleted = deleted_proof_from_entries(&base_entries, 2, 3);

        let source2 = SourceStamp {
            revision: 2,
            metric: Metric {
                bytes: 7 * 32,
                utf16: 7 * 32,
            },
            content_hash: 2,
        };
        host.observe_source_advance(source2).unwrap();
        host.begin_offer(begin_delta_for(source2, 2, base, 2, 3, deleted, 2, 7))
            .unwrap();
        let mut inserted = SnapshotProducer::new_with_first_leaf_id(2, 2, 1_000, 32);
        offer_all_chunks(&mut host, &mut inserted);
        let (ack, polls) = commit_claim_with_fuel(
            &mut host,
            inserted.claim(),
            PublicationFuel {
                inspect_bytes: 16 * 1024,
                copy_bytes: 16 * 1024,
                transitions: 1,
            },
        );

        let target_entries = installed_leaf_entries(&host);
        assert_eq!(ack.summary.leaves, 7);
        assert_eq!(ack.summary.metric, source2.metric);
        assert!(polls > 1, "delta must actually yield under one-credit fuel");
        assert_ne!(host.installed.as_ref().unwrap().sequence_root, base_root);
        assert_eq!(target_entries[0].0, base_entries[0].0);
        assert_eq!(target_entries[1].0, base_entries[1].0);
        assert_eq!(target_entries[4].0, base_entries[5].0);
        assert_eq!(target_entries[5].0, base_entries[6].0);
        assert_eq!(target_entries[6].0, base_entries[7].0);
        assert_eq!(
            target_entries
                .iter()
                .map(|entry| entry.1)
                .collect::<Vec<_>>(),
            vec![1, 2, 1_000, 1_001, 6, 7, 8]
        );
        for deleted in &base_entries[2..5] {
            assert!(!target_entries.iter().any(|target| target.0 == deleted.0));
        }
    }

    #[test]
    fn equal_shape_wrong_deleted_range_proof_fails_closed_and_preserves_base() {
        let source1 = SourceStamp {
            revision: 1,
            metric: Metric {
                bytes: 8 * 32,
                utf16: 8 * 32,
            },
            content_hash: 1,
        };
        let mut host = StagedPublicationHost::new(source1);
        let base = install_snapshot(&mut host, source1, 1, 0xabc, 8, 32);
        let base_root = host.installed.as_ref().unwrap().sequence_root;
        let base_entries = installed_leaf_entries(&host);
        let correct = deleted_proof_from_entries(&base_entries, 2, 3);
        let wrong_equal_shape = deleted_proof_from_entries(&base_entries, 3, 3);
        assert_eq!(correct.leaves, wrong_equal_shape.leaves);
        assert_eq!(correct.metric, wrong_equal_shape.metric);
        assert_ne!(correct.content_digest, wrong_equal_shape.content_digest);

        let source2 = SourceStamp {
            revision: 2,
            metric: Metric {
                bytes: 7 * 32,
                utf16: 7 * 32,
            },
            content_hash: 2,
        };
        host.observe_source_advance(source2).unwrap();
        host.begin_offer(begin_delta_for(
            source2,
            2,
            base,
            2,
            3,
            wrong_equal_shape,
            2,
            7,
        ))
        .unwrap();
        let mut inserted = SnapshotProducer::new_with_first_leaf_id(2, 2, 1_000, 32);
        offer_all_chunks(&mut host, &mut inserted);
        host.request_commit(inserted.claim()).unwrap();
        let error = loop {
            match host.poll(PublicationFuel {
                inspect_bytes: 16 * 1024,
                copy_bytes: 16 * 1024,
                transitions: 1,
            }) {
                Ok(_) => {}
                Err(error) => break error,
            }
        };
        assert_eq!(
            error,
            StagingError::Invalid("deleted sequence summary mismatch")
        );
        assert_eq!(host.installed.as_ref().unwrap().ack, base);
        assert_eq!(host.installed.as_ref().unwrap().sequence_root, base_root);
        while host.aborting.is_some()
            || host.arena.metrics().pending_releases != 0
            || host.arena.metrics().queued_release_nodes != 0
        {
            assert!(host.poll_disposal(1).unwrap() <= 1);
        }
        assert_eq!(host.installed.as_ref().unwrap().ack, base);
        assert_eq!(installed_leaf_entries(&host), base_entries);
    }

    #[test]
    fn delta_supports_delete_to_empty_then_insert_into_empty() {
        let source1 = SourceStamp {
            revision: 1,
            metric: Metric {
                bytes: 64,
                utf16: 64,
            },
            content_hash: 1,
        };
        let mut host = StagedPublicationHost::new(source1);
        let base = install_snapshot(&mut host, source1, 1, 0xabc, 2, 32);
        let base_entries = installed_leaf_entries(&host);
        let deleted = deleted_proof_from_entries(&base_entries, 0, 2);

        let source2 = SourceStamp {
            revision: 2,
            metric: Metric::default(),
            content_hash: 2,
        };
        host.observe_source_advance(source2).unwrap();
        host.begin_offer(begin_delta_for(source2, 2, base, 0, 2, deleted, 0, 0))
            .unwrap();
        let (empty, _) = commit_claim_with_fuel(
            &mut host,
            CommitClaim {
                offer: 2,
                actual_chunks: 0,
                actual_wire_bytes: 0,
                rolling_transport_digest: 0,
                inserted_stream_digest: 0,
            },
            PublicationFuel {
                inspect_bytes: 16 * 1024,
                copy_bytes: 16 * 1024,
                transitions: 1,
            },
        );
        assert_eq!(empty.summary, SequenceSummary::default());
        assert_eq!(host.installed.as_ref().unwrap().sequence_root, None);

        let source3 = SourceStamp {
            revision: 3,
            metric: Metric {
                bytes: 32,
                utf16: 32,
            },
            content_hash: 3,
        };
        host.observe_source_advance(source3).unwrap();
        host.begin_offer(begin_delta_for(
            source3,
            3,
            empty,
            0,
            0,
            DeletedRangeProof::default(),
            1,
            1,
        ))
        .unwrap();
        let mut inserted = SnapshotProducer::new_with_first_leaf_id(3, 1, 900, 32);
        offer_all_chunks(&mut host, &mut inserted);
        let (one, _) = commit_claim_with_fuel(
            &mut host,
            inserted.claim(),
            PublicationFuel {
                inspect_bytes: 16 * 1024,
                copy_bytes: 16 * 1024,
                transitions: 1,
            },
        );
        assert_eq!(one.summary.leaves, 1);
        assert_eq!(one.summary.metric, source3.metric);
        assert_eq!(installed_leaf_entries(&host)[0].1, 900);
    }

    #[test]
    fn delta_splice_cancellation_and_supersession_keep_exact_base_under_fuel_one() {
        let mut saw_finishing = false;
        let mut saw_need_splice = false;
        let mut saw_splicing = false;
        let mut saw_need_manifest = false;
        let mut explicit_cancellations = 0;
        let mut supersessions = 0;
        for cut in 0..32 {
            let source1 = SourceStamp {
                revision: 1,
                metric: Metric {
                    bytes: 8 * 32,
                    utf16: 8 * 32,
                },
                content_hash: 1,
            };
            let mut host = StagedPublicationHost::new(source1);
            let base = install_snapshot(&mut host, source1, 1, 0xabc, 8, 32);
            let base_root = host.installed.as_ref().unwrap().sequence_root;
            let base_entries = installed_leaf_entries(&host);
            let deleted = deleted_proof_from_entries(&base_entries, 2, 3);
            let source2 = SourceStamp {
                revision: 2,
                metric: Metric {
                    bytes: 7 * 32,
                    utf16: 7 * 32,
                },
                content_hash: 2,
            };
            host.observe_source_advance(source2).unwrap();
            host.begin_offer(begin_delta_for(source2, 2, base, 2, 3, deleted, 2, 7))
                .unwrap();
            let mut inserted = SnapshotProducer::new_with_first_leaf_id(2, 2, 1_000, 32);
            offer_all_chunks(&mut host, &mut inserted);
            host.request_commit(inserted.claim()).unwrap();
            for _ in 0..cut {
                let Some(active) = host.active.as_ref() else {
                    break;
                };
                match &active.phase {
                    OfferPhase::Finishing => saw_finishing = true,
                    OfferPhase::NeedSplice { .. } => saw_need_splice = true,
                    OfferPhase::Splicing => saw_splicing = true,
                    OfferPhase::NeedManifest { .. } => saw_need_manifest = true,
                    OfferPhase::Receiving | OfferPhase::ReadySwap { .. } => {}
                }
                let _ = host
                    .poll(PublicationFuel {
                        inspect_bytes: 16 * 1024,
                        copy_bytes: 16 * 1024,
                        transitions: 1,
                    })
                    .unwrap();
            }
            let Some(active) = host.active.as_ref() else {
                continue;
            };
            match &active.phase {
                OfferPhase::Finishing => saw_finishing = true,
                OfferPhase::NeedSplice { .. } => saw_need_splice = true,
                OfferPhase::Splicing => saw_splicing = true,
                OfferPhase::NeedManifest { .. } => saw_need_manifest = true,
                OfferPhase::Receiving | OfferPhase::ReadySwap { .. } => {}
            }
            if cut % 2 == 0 {
                host.abort_offer().unwrap();
                explicit_cancellations += 1;
            } else {
                host.observe_source_advance(SourceStamp {
                    revision: 3,
                    metric: Metric {
                        bytes: 7 * 32 + 1,
                        utf16: 7 * 32 + 1,
                    },
                    content_hash: 3,
                })
                .unwrap();
                supersessions += 1;
            }
            assert_eq!(host.installed.as_ref().unwrap().ack, base);
            assert_eq!(host.installed.as_ref().unwrap().sequence_root, base_root);
            while host.aborting.is_some()
                || host.arena.metrics().pending_releases != 0
                || host.arena.metrics().queued_release_nodes != 0
            {
                assert!(host.poll_disposal(1).unwrap() <= 1);
            }
            assert_eq!(installed_leaf_entries(&host), base_entries);
        }
        assert!(saw_finishing);
        assert!(saw_need_splice);
        assert!(saw_splicing);
        assert!(saw_need_manifest);
        assert!(explicit_cancellations > 0);
        assert!(supersessions > 0);
    }

    #[test]
    fn supersession_is_constant_time_and_abort_reclaim_is_fuel_one() {
        for cut in 0..18 {
            let source1 = SourceStamp {
                revision: 1,
                metric: Metric {
                    bytes: 64,
                    utf16: 64,
                },
                content_hash: 1,
            };
            let mut host = StagedPublicationHost::new(source1);
            let base = install_small_snapshot(&mut host, source1, 1, 0x11);
            let source2 = SourceStamp {
                revision: 2,
                metric: Metric {
                    bytes: 96,
                    utf16: 96,
                },
                content_hash: 2,
            };
            host.observe_source_advance(source2).unwrap();
            host.begin_offer(begin_for(source2, 2, 0x22, 3)).unwrap();
            let mut producer = SnapshotProducer::new(2, 3, 32);
            let chunk = producer.next_chunk().unwrap();
            host.offer_chunk(chunk).unwrap();
            for _ in 0..cut {
                let _ = host.poll(PublicationFuel {
                    inspect_bytes: ARENA_PAGE_BYTES,
                    copy_bytes: ARENA_PAGE_BYTES,
                    transitions: 1,
                });
                if host.active.is_none() {
                    break;
                }
            }
            if host.active.is_none() {
                continue;
            }
            let source3 = SourceStamp {
                revision: 3,
                metric: Metric {
                    bytes: 97,
                    utf16: 97,
                },
                content_hash: 3,
            };
            host.observe_source_advance(source3).unwrap();
            assert_eq!(host.installed.as_ref().unwrap().ack, base);
            while host.aborting.is_some()
                || host.arena.metrics().pending_releases != 0
                || host.arena.metrics().queued_release_nodes != 0
            {
                assert!(host.poll_disposal(1).unwrap() <= 1);
            }
            assert_eq!(host.installed.as_ref().unwrap().ack, base);
        }
    }

    #[test]
    fn one_credit_and_corrupt_chunk_fail_closed_without_replacing_base() {
        let source1 = SourceStamp {
            revision: 1,
            metric: Metric {
                bytes: 64,
                utf16: 64,
            },
            content_hash: 1,
        };
        let mut host = StagedPublicationHost::new(source1);
        let base = install_small_snapshot(&mut host, source1, 1, 0x11);
        let source2 = SourceStamp {
            revision: 2,
            metric: Metric {
                bytes: 96,
                utf16: 96,
            },
            content_hash: 2,
        };
        host.observe_source_advance(source2).unwrap();
        host.begin_offer(begin_for(source2, 2, 0x22, 3)).unwrap();
        let mut producer = SnapshotProducer::new(2, 3, 32);
        let mut first = producer.next_chunk().unwrap();
        first.digest ^= 1;
        host.offer_chunk(first.clone()).unwrap();
        assert_eq!(host.offer_chunk(first), Err(StagingError::Backpressure));
        let error = loop {
            match host.poll(TEST_FUEL) {
                Ok(_) => {}
                Err(error) => break error,
            }
        };
        assert_eq!(error, StagingError::Invalid("chunk digest mismatch"));
        assert_eq!(host.installed.as_ref().unwrap().ack, base);
    }

    #[test]
    fn canonical_corrupt_closure_with_valid_transport_digest_fails_closed() {
        let source1 = SourceStamp {
            revision: 1,
            metric: Metric {
                bytes: 64,
                utf16: 64,
            },
            content_hash: 1,
        };
        let mut host = StagedPublicationHost::new(source1);
        let base = install_small_snapshot(&mut host, source1, 1, 0x11);
        let source2 = SourceStamp {
            revision: 2,
            metric: Metric {
                bytes: 32,
                utf16: 32,
            },
            content_hash: 2,
        };
        host.observe_source_advance(source2).unwrap();
        host.begin_offer(begin_for(source2, 2, 0x22, 1)).unwrap();
        let mut producer = SnapshotProducer::new(2, 1, 32);
        let chunk = producer.next_chunk().unwrap();
        let mut payload = Vec::from(chunk.payload.as_ref());
        let program_tag = RECORD_HEADER_BYTES + producer.leaf_payload.len() + PROGRAM_LENGTH_BYTES;
        payload[program_tag] ^= 0xff;
        let payload: Arc<[u8]> = payload.into();
        let corrupt = PublicationChunk {
            digest: hash_bytes(&payload),
            payload,
            ..chunk
        };
        host.offer_chunk(corrupt).unwrap();
        let error = loop {
            match host.poll(TEST_FUEL) {
                Ok(_) => {}
                Err(error) => break error,
            }
        };
        assert!(matches!(
            error,
            StagingError::Green(SerializedGreenError::Corrupt(_))
        ));
        assert_eq!(host.installed.as_ref().unwrap().ack, base);
    }

    #[test]
    fn canonically_valid_cross_leaf_underflow_is_rejected_at_manifest() {
        let source1 = SourceStamp {
            revision: 1,
            metric: Metric {
                bytes: 64,
                utf16: 64,
            },
            content_hash: 1,
        };
        let mut host = StagedPublicationHost::new(source1);
        let base = install_small_snapshot(&mut host, source1, 1, 0x11);
        let source2 = SourceStamp {
            revision: 2,
            metric: Metric {
                bytes: 64,
                utf16: 64,
            },
            content_hash: 2,
        };
        host.observe_source_advance(source2).unwrap();
        host.begin_offer(begin_for(source2, 2, 0x22, 2)).unwrap();
        let metric = SerializedMetric {
            bytes: 32,
            utf16: 32,
        };
        let close = serialized_green_staging_test_structural_closure(metric, false);
        let open = serialized_green_staging_test_structural_closure(metric, true);
        let (close_record, close_summary) = record_from_canonical_closure(1, &close, &[]);
        let (open_record, open_summary) = record_from_canonical_closure(2, &open, &[]);
        let mut payload = close_record;
        payload.extend_from_slice(&open_record);
        let payload: Arc<[u8]> = payload.into();
        let chunk = PublicationChunk {
            offer: 2,
            ordinal: 0,
            first_leaf: 0,
            leaf_count: 2,
            digest: hash_bytes(&payload),
            payload,
        };
        let claim = CommitClaim {
            offer: 2,
            actual_chunks: 1,
            actual_wire_bytes: u64::try_from(chunk.payload.len()).unwrap(),
            rolling_transport_digest: roll_transport_digest(0, &chunk),
            inserted_stream_digest: StreamSummary::default()
                .append_leaf(close_summary)
                .unwrap()
                .append_leaf(open_summary)
                .unwrap()
                .content_digest,
        };
        host.offer_chunk(chunk).unwrap();
        let mut a = 0;
        let mut b = 0;
        let mut c = 0;
        let mut d = 0;
        drain_chunk(&mut host, 0, &mut a, &mut b, &mut c, &mut d);
        host.request_commit(claim).unwrap();
        let error = loop {
            match host.poll(TEST_FUEL) {
                Ok(_) => {}
                Err(error) => break error,
            }
        };
        assert_eq!(
            error,
            StagingError::Invalid("staging root disagrees with target shape or complete structure")
        );
        assert_eq!(host.installed.as_ref().unwrap().ack, base);
    }

    #[test]
    fn canonical_max_page_program_and_zero_program_validation_are_fuel_one() {
        let metric = SerializedMetric {
            bytes: 4 * 1024,
            utf16: 4 * 1024,
        };
        let (program_leaf, programs) = serialized_green_staging_test_max_page_closure(metric);
        let zero_program_leaf = serialized_green_staging_test_zero_program_closure(metric);
        assert_eq!(
            program_leaf.len() + std::mem::size_of::<ArenaId>(),
            ARENA_PAGE_BYTES
        );
        assert_eq!(zero_program_leaf.len(), ARENA_PAGE_BYTES);

        for (leaf_payload, programs) in [
            (program_leaf.as_slice(), programs.as_slice()),
            (zero_program_leaf.as_slice(), &[][..]),
        ] {
            let closure_bytes = leaf_payload.len() + programs.iter().map(Vec::len).sum::<usize>();
            let mut validator = CopiedGreenClosureValidator::try_new_summary_only(
                leaf_payload.len(),
                programs.len(),
                closure_bytes,
            )
            .unwrap();
            loop {
                let (progress, receipt) = validator
                    .poll(
                        leaf_payload,
                        |ordinal| {
                            programs.get(ordinal).map(Vec::as_slice).ok_or(
                                SerializedGreenError::Corrupt(
                                    "test Program ordinal is out of range",
                                ),
                            )
                        },
                        CopiedGreenValidationFuel {
                            inspect_bytes: ARENA_PAGE_BYTES,
                            copy_bytes: MAX_INLINE_FACT_TEST_FUEL,
                            transitions: 1,
                        },
                    )
                    .unwrap();
                assert!(receipt.inspected_bytes <= ARENA_PAGE_BYTES);
                assert!(receipt.copied_bytes <= MAX_INLINE_FACT_TEST_FUEL);
                assert!(receipt.transitions <= 1);
                if progress == CopiedGreenValidationProgress::Complete {
                    let decoded = validator.take_decoded().unwrap();
                    assert_eq!(decoded.summary.metric, metric);
                    assert!(decoded.structural_events.is_empty());
                    break;
                }
            }
        }
        assert!(matches!(
            CopiedGreenClosureValidator::try_new_summary_only(
                ARENA_PAGE_BYTES,
                0,
                COPIED_GREEN_CLOSURE_MAX_BYTES + 1,
            ),
            Err(SerializedGreenError::Corrupt(_))
        ));
    }
}

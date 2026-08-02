//! Endpoint-owned exact-candidate publication state.
//!
//! This module joins one accepted clean-EOF certification to the exact parser,
//! sealed candidate publication, and the credited parser-to-host protocol. It
//! owns no host arena: every packet is a bounded set of self-contained copied
//! snapshot frames.

use std::{collections::VecDeque, fmt, ops::Range};

use flark_engine::parser_internal::{
    M11_MAX_ROLE_RECORDS, M11_MAX_SNAPSHOT_FRAME_BYTES, M11BlockQuoteProjectionError,
    M11BlockQuoteProjectionRoot, M11BlockSequenceEntryKind, M11BlockSequencePoint,
    M11BlockSequenceSpliceSelection, M11CandidateDescriptor, M11CandidatePublication,
    M11HotInlineCanonicalLineEnding, M11HotInlineSidecarBinding, M11HotInlineSidecarDescriptor,
    M11HotInlineSidecarDisposition, M11HotInlineSidecarFrame, M11HotInlineSidecarFrameKind,
    M11HotInlineSidecarOwner, M11HotInlineSidecarSnapshotEncoder, M11HotInlineSidecarSnapshotPoll,
    M11IndentedCodeProjectionError, M11IndentedCodeProjectionRoot, M11InlineProjectionError,
    M11InlineProjectionRoot, M11MarkedLineProjectionKind, M11OwnedSnapshotPoll,
    M11OwnedSnapshotStream, M11ParserPageError, M11ParserSourceRangeAuthority, M11PublicationError,
    M11RecursiveGreenFrameId, M11RecursiveGreenLocation, M11RecursiveGreenPoint,
    M11RecursiveGreenRowEditCapability,
    M11RecursiveGreenRowQueryLimits, M11ReferenceResolver, M11RetainedCandidatePublication,
    M11SnapshotFrame, M11SnapshotFrameKind,
};
use flark_engine::{
    CertifiedSource, DocumentRuntime, DocumentRuntimeError, IncrementalSourceFactsPlan,
    PersistentCertifiedSource, PersistentSourceFactsDeltaWitness, SourceBoundaryAffinity,
    SourceSnapshotLease,
};
use flark_parser::{
    LeadingReferencesCheckpointError, LeadingReferencesRestartCheckpoint,
    M11_BLOCK_QUOTE_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
    M11_BULLET_LIST_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
    M11_INDENTED_CODE_PROJECTION_JOB_MAX_POLL_TRANSITIONS, M11_INLINE_META_RECORD_BYTES,
    M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS, M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES,
    M11BlockQuoteProjectionJob, M11BlockQuoteProjectionJobError,
    M11BlockQuoteProjectionJobPollStatus, M11BulletListItemProjectionJob,
    M11BulletListItemProjectionJobPollStatus, M11BulletListLocalDeltaError,
    M11BulletListLocalDeltaJob, M11BulletListLocalDeltaPlan, M11BulletListLocalDeltaPoll,
    M11BulletListProjectionJob, M11CandidateDerivationError, M11CleanDocumentResult, M11CleanLeaf,
    M11CleanParseJob, M11CleanParseJobError, M11CleanParsePoll, M11ExactSegmentedCandidateInput,
    M11IndentedCodeProjectionJob, M11IndentedCodeProjectionJobError,
    M11IndentedCodeProjectionJobPollStatus, M11InlineProjectionJob, M11InlineProjectionJobError,
    M11InlineProjectionJobPollStatus, M11InlineProjectionPublication,
    M11InlineProjectionUnsupportedRecord, M11InlinePublicationError, M11LeadingReferencesCropError,
    M11LeadingReferencesCropParseJob, M11LeadingReferencesCropPoll, M11LeadingReferencesCropResult,
    M11OrdinaryParagraphBofCropParseJob, M11OrdinaryParagraphBofCropPlan,
    M11OrdinaryParagraphBofCropSelection, M11OrdinaryParagraphBoundaryCropError,
    M11OrdinaryParagraphBoundaryCropPlanError, M11OrdinaryParagraphBoundaryCropPoll,
    M11OrdinaryParagraphBoundaryCropResult, M11OrdinaryParagraphCheckpointError,
    M11OrdinaryParagraphCropError, M11OrdinaryParagraphCropParseJob, M11OrdinaryParagraphCropPlan,
    M11OrdinaryParagraphCropPlanError, M11OrdinaryParagraphCropPoll,
    M11OrdinaryParagraphCropResult, M11OrdinaryParagraphCropSelection,
    M11OrdinaryParagraphEofCropParseJob, M11OrdinaryParagraphEofCropPlan,
    M11OrdinaryParagraphEofCropSelection, M11OrdinaryParagraphRestartCheckpoints, M11ParserBinding,
    M11ParserCandidate, M11ParserCandidateWriter, M11ParserCandidateWriterPoll,
    M11PersistentRecursiveGreenAdoption, M11PersistentRecursiveGreenAdoptionStatus,
    M11PersistentRecursiveGreenBuildStatus, M11PersistentRecursiveGreenCleanBuild,
    M11PersistentRecursiveGreenCleanPlan, M11PersistentRecursiveGreenSession,
    M11PersistentRecursiveGreenSessionError, M11PersistentRecursiveGreenUpdate,
    M11PublishedBlockQuoteLeafFence, M11PublishedBulletListItemInlineFenceOutcome,
    M11PublishedBulletListItemProjectionFence, M11PublishedBulletListLeafFence,
    M11PublishedIndentedCodeLeafFence, M11PublishedInlineLeafFence,
    M11PublishedInlineLeafFenceResolution, M11PublishedInlineRangeError,
    M11PublishedOrderedListItemInlineFenceOutcome, M11PublishedOrderedListItemProjectionFence,
    M11RecursiveGreenParagraphPreparationError,
    block_core::{M11RecursiveGreenInlineLeafFence, M11RecursiveGreenInlineLeafKind},
    resolve_m11_published_block_quote_leaf_fence, resolve_m11_published_bullet_list_item_fences,
    resolve_m11_published_bullet_list_item_inline_fence,
    resolve_m11_published_bullet_list_leaf_fence, resolve_m11_published_indented_code_leaf_fence,
    resolve_m11_published_inline_leaf_fence, resolve_m11_published_ordered_list_item_fences,
};

use crate::v3_publication_wire::{
    CandidateSnapshotFrameKind, CandidateTransportDigest, CandidateTransportDigestError,
    CommitRequest, DecodeError, EncodeError, HOT_INLINE_SIDECAR_SCHEMA, HostPollOutcome,
    HostPollPhase, HostPollResult, HotInlineSidecarBegin, HotInlineSidecarBinding,
    HotInlineSidecarCommitRequest, HotInlineSidecarDisposition, HotInlineSidecarEnvelopeMetrics,
    HotInlineSidecarEventBody, HotInlineSidecarFrameKind, HotInlineSidecarMode,
    HotInlineSidecarOwner, HotInlineSidecarTransportDigest, InlineSidecarAck,
    InlineSidecarAckDisposition, InlineSidecarHostPollOutcome, InlineSidecarHostPollPhase,
    InlineSidecarHostPollResult, MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES,
    MAXIMUM_PACKET_ENCODED_BYTES, MAXIMUM_PACKET_FRAME_COUNT, OfferBegin, OfferLimits,
    PACKET_FRAME_DESCRIPTOR_BYTES, PACKET_HEADER_BYTES, ProtocolDigestDomain, PublicationEventBody,
    PublicationMode, PublicationPacketFrameInput, PublicationPacketInput, SourceVersion,
    StructuralAck, VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES,
    VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES, VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES,
    VIEWPORT_PRESENTATION_END_FRAME_BYTES, VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES,
    ViewportPresentationAck, ViewportPresentationBegin, ViewportPresentationBinding,
    ViewportPresentationChildFrameInput, ViewportPresentationCommitRequest,
    ViewportPresentationDirectoryEntry, ViewportPresentationEndFrame,
    ViewportPresentationEventBody, ViewportPresentationFrameKind,
    ViewportPresentationHostPollOutcome, ViewportPresentationHostPollPhase,
    ViewportPresentationHostPollResult, ViewportPresentationMetricRange, ViewportPresentationMode,
    ViewportPresentationOfferLimits, ViewportPresentationQueryLimits,
    ViewportPresentationTransportDigest, ViewportPresentationTransportDigestError,
    ViewportPresentationVisitStart, decode_publication_packet, encode_publication_packet_into,
    encode_viewport_presentation_child_frame_into, encode_viewport_presentation_directory_into,
    encode_viewport_presentation_end_frame_into, encode_viewport_presentation_parent_frame_into,
    protocol_digest128_from_blake3, viewport_presentation_aggregate_envelope_digest256,
    viewport_presentation_root_stream_digest256,
};
use crate::v3_session_wire::{
    InlinePointAffinity, InlineRefinementCommand, InlineRefinementTarget, SessionBinding,
    SourceFactsCompletionEvent,
};

const MANIFEST_SCHEMA: u32 = 1;
const GRAMMAR_REVISION: u32 = crate::FLARK_V3_GRAMMAR_REVISION;
const AUTHORITY_MASK_ALL_ROLES: u32 = 0x1f;
const HOT_INLINE_UNSUPPORTED_NOT_INLINE_LEAF: u32 = 0x2000_0001;
const HOT_INLINE_UNSUPPORTED_PARSER: u32 = 0x2000_0002;
const HOT_INLINE_UNSUPPORTED_LEGACY_BLOCK_TARGET: u32 = 0x2000_0003;

#[path = "v3_candidate_endpoint_contract.rs"]
mod contract;
pub(crate) use contract::*;

#[path = "v3_candidate_endpoint_hot_inline.rs"]
mod hot_inline;

#[path = "v3_candidate_endpoint_candidate.rs"]
mod candidate;
use candidate::*;

#[path = "v3_candidate_endpoint_viewport.rs"]
mod viewport;
use viewport::prepare_viewport_presentation;

#[path = "v3_candidate_endpoint_recursive_green.rs"]
mod recursive_green;
use recursive_green::RecursiveGreenEndpointSlot;
pub(crate) use recursive_green::RecursiveGreenPathReceipt;

#[derive(Clone, Copy)]
struct CandidateContext {
    binding: SessionBinding,
    completion: SourceFactsCompletionEvent,
    parse_generation: u32,
}

enum ActiveCandidate {
    Parsing(Box<ParsingCandidate>),
    Building {
        context: CandidateContext,
        writer: Box<M11ParserCandidateWriter>,
        next_restart: Option<CandidateRestartAuthority>,
    },
    ParsingExact(Box<ParsingExactCandidate>),
    ParsingOrdinaryExact(Box<ParsingOrdinaryExactCandidate>),
    AwaitingRecursiveGreenExact(Box<AwaitingRecursiveGreenExactCandidate>),
    ParsingBulletListLocal(Box<ParsingBulletListLocalCandidate>),
    ParsingExactFallback(Box<ParsingExactFallbackCandidate>),
    BuildingExactFallback {
        context: CandidateContext,
        writer: Box<M11ParserCandidateWriter>,
        base: ExactCandidateBase,
        next_restart: Option<CandidateRestartAuthority>,
    },
    BuildingExact {
        context: CandidateContext,
        writer: Box<M11ParserCandidateWriter>,
        base: ExactCandidateBase,
        witness: Box<PersistentSourceFactsDeltaWitness>,
        next_restart: CandidateRestartAuthority,
        structural_path: ExactStructuralPath,
    },
    Streaming(Box<StreamingCandidate>),
}

struct ParsingCandidate {
    context: CandidateContext,
    certified: CertifiedSource,
    job: M11CleanParseJob,
    publication_path: CleanPublicationPath,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CleanPublicationPath {
    RecursiveGreenInitial,
    LegacyBlocks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExactStructuralPath {
    LegacyBlocks,
    RecursiveGreen,
}

struct ParsingExactCandidate {
    context: CandidateContext,
    job: M11LeadingReferencesCropParseJob,
    base: ExactCandidateBase,
    witness: Box<PersistentSourceFactsDeltaWitness>,
}

struct ParsingOrdinaryExactCandidate {
    context: CandidateContext,
    job: OrdinaryExactJob,
    base: ExactCandidateBase,
    witness: Box<PersistentSourceFactsDeltaWitness>,
}

/// Exact-base transaction parked behind the authoritative recursive-Green
/// adoption. No clean parser is allocated unless that adoption explicitly
/// requests the clean fallback.
struct AwaitingRecursiveGreenExactCandidate {
    context: CandidateContext,
    certified: PersistentCertifiedSource,
    base: ExactCandidateBase,
    witness: Box<PersistentSourceFactsDeltaWitness>,
}

struct ParsingBulletListLocalCandidate {
    context: CandidateContext,
    job: M11BulletListLocalDeltaJob,
    base: ExactCandidateBase,
    witness: Box<PersistentSourceFactsDeltaWitness>,
    target_source: flark_engine::SourceVersion,
    target_binding: M11ParserBinding,
    predecessor_end_byte: usize,
    predecessor_end_utf16: usize,
    successor_start_byte: usize,
    successor_start_utf16: usize,
}

struct ParsingExactFallbackCandidate {
    context: CandidateContext,
    certified: PersistentCertifiedSource,
    job: M11CleanParseJob,
    base: ExactCandidateBase,
    witness: Box<PersistentSourceFactsDeltaWitness>,
}

enum OrdinaryExactJob {
    Interior(M11OrdinaryParagraphCropParseJob),
    FromBof(M11OrdinaryParagraphBofCropParseJob),
    ToEof(M11OrdinaryParagraphEofCropParseJob),
}

enum OrdinaryExactPoll {
    Pending {
        transitions: usize,
    },
    Complete {
        transitions: usize,
        result: OrdinaryExactResult,
    },
}

enum OrdinaryExactResult {
    Interior(M11OrdinaryParagraphCropResult),
    Boundary(M11OrdinaryParagraphBoundaryCropResult),
}

#[derive(Clone, Copy)]
enum OrdinaryCropRoute {
    Interior(M11OrdinaryParagraphCropSelection),
    FromBof(M11OrdinaryParagraphBofCropSelection),
    ToEof(M11OrdinaryParagraphEofCropSelection),
}

impl OrdinaryExactJob {
    fn poll(&mut self, fuel: usize) -> Result<OrdinaryExactPoll, CandidateEndpointError> {
        match self {
            Self::Interior(job) => Ok(match job.poll(fuel)? {
                M11OrdinaryParagraphCropPoll::Pending { transitions } => {
                    OrdinaryExactPoll::Pending { transitions }
                }
                M11OrdinaryParagraphCropPoll::Complete {
                    transitions,
                    result,
                } => OrdinaryExactPoll::Complete {
                    transitions,
                    result: OrdinaryExactResult::Interior(result),
                },
            }),
            Self::FromBof(job) => Ok(match job.poll(fuel)? {
                M11OrdinaryParagraphBoundaryCropPoll::Pending { transitions } => {
                    OrdinaryExactPoll::Pending { transitions }
                }
                M11OrdinaryParagraphBoundaryCropPoll::Complete {
                    transitions,
                    result,
                } => OrdinaryExactPoll::Complete {
                    transitions,
                    result: OrdinaryExactResult::Boundary(result),
                },
            }),
            Self::ToEof(job) => Ok(match job.poll(fuel)? {
                M11OrdinaryParagraphBoundaryCropPoll::Pending { transitions } => {
                    OrdinaryExactPoll::Pending { transitions }
                }
                M11OrdinaryParagraphBoundaryCropPoll::Complete {
                    transitions,
                    result,
                } => OrdinaryExactPoll::Complete {
                    transitions,
                    result: OrdinaryExactResult::Boundary(result),
                },
            }),
        }
    }

    fn cancel_into_base_restart_checkpoints(
        self,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoints, CandidateEndpointError> {
        match self {
            Self::Interior(job) => Ok(job.cancel_into_base_restart_checkpoints()?),
            Self::FromBof(job) => Ok(job.cancel_into_base_restart_checkpoints()?),
            Self::ToEof(job) => Ok(job.cancel_into_base_restart_checkpoints()?),
        }
    }
}

impl OrdinaryExactResult {
    fn take_base_restart_checkpoints(
        &mut self,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoints, M11OrdinaryParagraphCheckpointError> {
        match self {
            Self::Interior(result) => result.take_base_restart_checkpoints(),
            Self::Boundary(result) => result.take_base_restart_checkpoints(),
        }
    }

    fn take_next_restart_checkpoints(
        &mut self,
    ) -> Result<M11OrdinaryParagraphRestartCheckpoints, M11OrdinaryParagraphCheckpointError> {
        match self {
            Self::Interior(result) => result.take_next_restart_checkpoints(),
            Self::Boundary(result) => result.take_next_restart_checkpoints(),
        }
    }

    fn into_exact_segmented_candidate_input(
        self,
    ) -> Result<M11ExactSegmentedCandidateInput, M11CandidateDerivationError> {
        match self {
            Self::Interior(result) => result.into_exact_segmented_candidate_input(),
            Self::Boundary(result) => result.into_exact_segmented_candidate_input(),
        }
    }
}

enum CandidateCleanup {
    Writer {
        writer: Box<M11ParserCandidateWriter>,
        begun: bool,
    },
    Publication {
        publication: Box<M11CandidatePublication>,
        begun: bool,
    },
    Stream {
        stream: Box<M11OwnedSnapshotStream>,
        begun: bool,
    },
    RetainedPublication {
        publication: Box<M11RetainedCandidatePublication>,
        begun: bool,
    },
    ExactPublications {
        target: Box<M11CandidatePublication>,
        target_begun: bool,
        target_complete: bool,
        base: Box<M11RetainedCandidatePublication>,
        base_begun: bool,
    },
    RetainedPair {
        target: Box<M11RetainedCandidatePublication>,
        target_begun: bool,
        target_complete: bool,
        base: Box<M11RetainedCandidatePublication>,
        base_begun: bool,
    },
    StreamAndRetained {
        stream: Box<M11OwnedSnapshotStream>,
        stream_begun: bool,
        stream_complete: bool,
        base: Box<M11RetainedCandidatePublication>,
        base_begun: bool,
    },
    ExactStreamAndRestore {
        stream: Box<M11OwnedSnapshotStream>,
        stream_begun: bool,
        base: Option<Box<M11RetainedCandidatePublication>>,
        recovery: Option<ExactBaseRecovery>,
    },
}

struct RetainedCandidateBase {
    publication: Box<M11RetainedCandidatePublication>,
    ack: StructuralAck,
    restart: Option<CandidateRestartAuthority>,
}

enum CandidateRestartAuthority {
    Leading(LeadingReferencesRestartCheckpoint),
    Ordinary(M11OrdinaryParagraphRestartCheckpoints),
    /// Exact retained publication whose bounded restart authority is owned by
    /// the endpoint's matching persistent recursive-Green session.
    RecursiveGreen {
        source: flark_engine::SourceVersion,
        binding: M11ParserBinding,
    },
    /// Exact retained publication authority without a bounded parser restart.
    ///
    /// This keeps segmented documents eligible for an exact-base transaction
    /// whose discovery phase is still a definitive clean parse. It does not
    /// claim incremental parser work.
    ExactBaseOnly {
        source: flark_engine::SourceVersion,
        binding: M11ParserBinding,
    },
}

impl CandidateRestartAuthority {
    const fn source(&self) -> flark_engine::SourceVersion {
        match self {
            Self::Leading(restart) => restart.source(),
            Self::Ordinary(restarts) => restarts.source(),
            Self::RecursiveGreen { source, .. } => *source,
            Self::ExactBaseOnly { source, .. } => *source,
        }
    }

    const fn binding(&self) -> M11ParserBinding {
        match self {
            Self::Leading(restart) => restart.binding(),
            Self::Ordinary(restarts) => restarts.binding(),
            Self::RecursiveGreen { binding, .. } => *binding,
            Self::ExactBaseOnly { binding, .. } => *binding,
        }
    }
}

struct ExactCandidateBase {
    publication: Box<M11RetainedCandidatePublication>,
    ack: StructuralAck,
    restart: Option<CandidateRestartAuthority>,
}

struct ExactBaseRecovery {
    ack: StructuralAck,
    restart: CandidateRestartAuthority,
}

struct ExactBuildStartFailure {
    error: CandidateEndpointError,
    base: ExactCandidateBase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StreamPhase {
    NeedBegin,
    AwaitBeginReceipt,
    NeedPacket,
    AwaitPacketReceipt {
        first_frame_ordinal: u32,
        frame_count: u32,
        end: bool,
    },
    AwaitPacketHost {
        poll_ticket: u32,
        next_frame_ordinal: u32,
        end: bool,
    },
    NeedCommit,
    AwaitCommitReceipt,
    AwaitCommitHost {
        poll_ticket: u32,
    },
    AwaitRecursiveGreenDelivery,
    AwaitDeliveryReceipt,
}

struct PacketFrame {
    record_count: u32,
    digest: [u32; 4],
    bytes: Box<[u8]>,
}

#[derive(Default)]
struct PacketBuilder {
    first_frame_ordinal: Option<u32>,
    first_record_ordinal: u32,
    aggregate_record_count: u32,
    aggregate_frame_bytes: usize,
    frames: Vec<PacketFrame>,
    end: bool,
}

struct StreamingCandidate {
    stream: Option<M11OwnedSnapshotStream>,
    sealed_publication: Option<M11RetainedCandidatePublication>,
    offer: OfferBegin,
    descriptor: M11CandidateDescriptor,
    phase: StreamPhase,
    transport: Option<CandidateTransportDigest>,
    next_frame_ordinal: u32,
    next_record_ordinal: u32,
    next_node_ordinal: Option<u64>,
    packet: PacketBuilder,
    lookahead: Option<M11SnapshotFrame>,
    /// An exact-base producer barrier was reached. The stream may resume only
    /// after the host has returned credit for the packet containing all pages
    /// accepted before that barrier.
    resume_after_packet_credit: bool,
    canonical_stream_digest: Option<[u32; 4]>,
    commit: Option<CommitRequest>,
    expected_ack: Option<StructuralAck>,
    next_restart: Option<CandidateRestartAuthority>,
    superseded_exact_base: Option<Box<M11RetainedCandidatePublication>>,
    exact_base_recovery: Option<ExactBaseRecovery>,
}

struct StreamingHotInlineSidecar {
    encoder: M11HotInlineSidecarSnapshotEncoder,
    root: Option<Box<HotInlineProjectionRoot>>,
    authority: Option<M11ParserSourceRangeAuthority>,
    offer: HotInlineSidecarBegin,
    phase: StreamPhase,
    transport: Option<HotInlineSidecarTransportDigest>,
    next_frame_ordinal: u32,
    next_record_ordinal: u32,
    next_node_ordinal: Option<u64>,
    packet: PacketBuilder,
    lookahead: Option<M11HotInlineSidecarFrame>,
    root_stream_digest: Option<[u32; 4]>,
    commit: Option<HotInlineSidecarCommitRequest>,
    expected_ack: Option<InlineSidecarAck>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HotInlineLeafOwner {
    BlockOrdinal(u64),
    RecursiveGreenFrame(M11RecursiveGreenFrameId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HotInlineLeafIdentity {
    kind: M11BlockSequenceEntryKind,
    byte_start: u32,
    byte_end: u32,
    utf16_start: u32,
    utf16_end: u32,
    inline_byte_start: u32,
    inline_byte_end: u32,
    inline_utf16_start: u32,
    inline_utf16_end: u32,
    owner: HotInlineLeafOwner,
}

const fn recursive_green_inline_leaf_sequence_kind(
    kind: M11RecursiveGreenInlineLeafKind,
) -> M11BlockSequenceEntryKind {
    match kind {
        M11RecursiveGreenInlineLeafKind::Paragraph => M11BlockSequenceEntryKind::Paragraph,
        M11RecursiveGreenInlineLeafKind::Heading => M11BlockSequenceEntryKind::Structured,
    }
}

fn resolved_recursive_green_inline_leaf(
    session: &M11PersistentRecursiveGreenSession,
    runtime: &DocumentRuntime,
    command: InlineRefinementCommand,
    byte_offset: usize,
    utf16_offset: usize,
    affinity: SourceBoundaryAffinity,
) -> Result<ResolvedHotInlineDemand, CandidateEndpointError> {
    let parser_profile =
        flark_engine::ParserProfileId::new(u64::from(command.base_ack.syntax_profile))
            .ok_or(CandidateEndpointError::MetricOverflow)?;
    let prepared = session.prepare_inline_leaf(
        runtime,
        M11RecursiveGreenPoint::new(byte_offset, utf16_offset, affinity),
    )?;
    let block_source = prepared.block_source_range();
    let block_source_utf16 = prepared.block_source_utf16_range();
    let inline_source = prepared.inline_source_range();
    let inline_source_utf16 = prepared.inline_source_utf16_range();
    let fence = prepared.into_fence();
    let identity = HotInlineLeafIdentity {
        kind: recursive_green_inline_leaf_sequence_kind(fence.kind()),
        byte_start: block_source.start,
        byte_end: block_source.end,
        utf16_start: block_source_utf16.start,
        utf16_end: block_source_utf16.end,
        inline_byte_start: inline_source.start,
        inline_byte_end: inline_source.end,
        inline_utf16_start: inline_source_utf16.start,
        inline_utf16_end: inline_source_utf16.end,
        owner: HotInlineLeafOwner::RecursiveGreenFrame(fence.frame()),
    };
    Ok(ResolvedHotInlineDemand::PreparedInlineLeaf {
        command,
        identity,
        inline_source,
        inline_source_utf16,
        parser_profile,
        fence,
    })
}

fn resolved_recursive_green_automatic(
    session: &M11PersistentRecursiveGreenSession,
    runtime: &DocumentRuntime,
    command: InlineRefinementCommand,
    byte_offset: usize,
    utf16_offset: usize,
    affinity: SourceBoundaryAffinity,
) -> Result<ResolvedHotInlineDemand, CandidateEndpointError> {
    let point = M11RecursiveGreenPoint::new(byte_offset, utf16_offset, affinity);
    let location = session
        .locate_point(runtime, point)?
        .ok_or(CandidateEndpointError::InvalidState)?;
    if M11RecursiveGreenInlineLeafKind::from_green_kind(location.owner().kind()).is_some() {
        return resolved_recursive_green_inline_leaf(
            session,
            runtime,
            command,
            byte_offset,
            utf16_offset,
            affinity,
        );
    }

    resolved_recursive_green_unsupported(
        command,
        location,
        HotInlineUnsupported::NotInlineLeaf {
            kind: M11BlockSequenceEntryKind::Structured,
        },
    )
}

fn resolved_recursive_green_legacy_target(
    session: &M11PersistentRecursiveGreenSession,
    runtime: &DocumentRuntime,
    command: InlineRefinementCommand,
    byte_offset: usize,
    utf16_offset: usize,
    affinity: SourceBoundaryAffinity,
) -> Result<ResolvedHotInlineDemand, CandidateEndpointError> {
    let location = session
        .locate_point(
            runtime,
            M11RecursiveGreenPoint::new(byte_offset, utf16_offset, affinity),
        )?
        .ok_or(CandidateEndpointError::InvalidState)?;
    resolved_recursive_green_unsupported(
        command,
        location,
        HotInlineUnsupported::LegacyBlockTarget {
            target: command.target,
        },
    )
}

fn resolved_recursive_green_unsupported(
    command: InlineRefinementCommand,
    location: M11RecursiveGreenLocation,
    unsupported: HotInlineUnsupported,
) -> Result<ResolvedHotInlineDemand, CandidateEndpointError> {
    let source = location.byte_range();
    let source_utf16 = location.utf16_range();
    let byte_start = u32::try_from(source.start)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let byte_end =
        u32::try_from(source.end).map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let utf16_start = u32::try_from(source_utf16.start)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let utf16_end =
        u32::try_from(source_utf16.end).map_err(|_| CandidateEndpointError::MetricOverflow)?;
    if byte_start >= byte_end || utf16_start >= utf16_end {
        return Err(CandidateEndpointError::InvalidState);
    }
    let kind = M11BlockSequenceEntryKind::Structured;
    let parser_profile =
        flark_engine::ParserProfileId::new(u64::from(command.base_ack.syntax_profile))
            .ok_or(CandidateEndpointError::MetricOverflow)?;
    Ok(ResolvedHotInlineDemand::Unsupported(Box::new(HotInlineReady {
        command,
        identity: HotInlineLeafIdentity {
            kind,
            byte_start,
            byte_end,
            utf16_start,
            utf16_end,
            inline_byte_start: byte_start,
            inline_byte_end: byte_end,
            inline_utf16_start: utf16_start,
            inline_utf16_end: utf16_end,
            owner: HotInlineLeafOwner::RecursiveGreenFrame(location.owner().frame()),
        },
        inline_source: byte_start..byte_end,
        inline_source_utf16: utf16_start..utf16_end,
        parser_profile,
        authority: None,
        publication: HotInlineReadyPublication::Unsupported(unsupported),
    })))
}

impl HotInlineLeafIdentity {
    fn inline_leaf(fence: &M11PublishedInlineLeafFence) -> Self {
        let source = fence.block_source_range();
        let source_utf16 = fence.block_source_utf16_range();
        let inline_source = fence.inline_source_range();
        let inline_source_utf16 = fence.inline_source_utf16_range();
        Self {
            kind: fence.kind(),
            byte_start: source.start,
            byte_end: source.end,
            utf16_start: source_utf16.start,
            utf16_end: source_utf16.end,
            inline_byte_start: inline_source.start,
            inline_byte_end: inline_source.end,
            inline_utf16_start: inline_source_utf16.start,
            inline_utf16_end: inline_source_utf16.end,
            owner: HotInlineLeafOwner::BlockOrdinal(fence.entry_ordinal()),
        }
    }

    fn indented_code_leaf(fence: &M11PublishedIndentedCodeLeafFence) -> Self {
        let source = fence.block_source_range();
        let source_utf16 = fence.block_source_utf16_range();
        Self {
            kind: M11BlockSequenceEntryKind::Structured,
            byte_start: source.start,
            byte_end: source.end,
            utf16_start: source_utf16.start,
            utf16_end: source_utf16.end,
            inline_byte_start: source.start,
            inline_byte_end: source.end,
            inline_utf16_start: source_utf16.start,
            inline_utf16_end: source_utf16.end,
            owner: HotInlineLeafOwner::BlockOrdinal(fence.entry_ordinal()),
        }
    }

    fn block_quote_leaf(fence: &M11PublishedBlockQuoteLeafFence) -> Self {
        let source = fence.block_source_range();
        let source_utf16 = fence.block_source_utf16_range();
        Self {
            kind: M11BlockSequenceEntryKind::Structured,
            byte_start: source.start,
            byte_end: source.end,
            utf16_start: source_utf16.start,
            utf16_end: source_utf16.end,
            inline_byte_start: source.start,
            inline_byte_end: source.end,
            inline_utf16_start: source_utf16.start,
            inline_utf16_end: source_utf16.end,
            owner: HotInlineLeafOwner::BlockOrdinal(fence.entry_ordinal()),
        }
    }

    fn bullet_list_leaf(fence: &M11PublishedBulletListLeafFence) -> Self {
        let source = fence.block_source_range();
        let source_utf16 = fence.block_source_utf16_range();
        Self {
            kind: M11BlockSequenceEntryKind::Structured,
            byte_start: source.start,
            byte_end: source.end,
            utf16_start: source_utf16.start,
            utf16_end: source_utf16.end,
            inline_byte_start: source.start,
            inline_byte_end: source.end,
            inline_utf16_start: source_utf16.start,
            inline_utf16_end: source_utf16.end,
            owner: HotInlineLeafOwner::BlockOrdinal(fence.entry_ordinal()),
        }
    }

    fn bullet_list_item(fence: &M11PublishedBulletListItemProjectionFence) -> Self {
        let source = fence.block_source_range();
        let source_utf16 = fence.block_source_utf16_range();
        let item = fence.item_source_range();
        let item_utf16 = fence.item_source_utf16_range();
        Self {
            kind: M11BlockSequenceEntryKind::Structured,
            byte_start: source.start,
            byte_end: source.end,
            utf16_start: source_utf16.start,
            utf16_end: source_utf16.end,
            inline_byte_start: item.start,
            inline_byte_end: item.end,
            inline_utf16_start: item_utf16.start,
            inline_utf16_end: item_utf16.end,
            owner: HotInlineLeafOwner::BlockOrdinal(fence.entry_ordinal()),
        }
    }

    fn ordered_list_item(fence: &M11PublishedOrderedListItemProjectionFence) -> Self {
        let source = fence.block_source_range();
        let source_utf16 = fence.block_source_utf16_range();
        let item = fence.item_source_range();
        let item_utf16 = fence.item_source_utf16_range();
        Self {
            kind: M11BlockSequenceEntryKind::Structured,
            byte_start: source.start,
            byte_end: source.end,
            utf16_start: source_utf16.start,
            utf16_end: source_utf16.end,
            inline_byte_start: item.start,
            inline_byte_end: item.end,
            inline_utf16_start: item_utf16.start,
            inline_utf16_end: item_utf16.end,
            owner: HotInlineLeafOwner::BlockOrdinal(fence.entry_ordinal()),
        }
    }

    const fn source_range(self) -> std::ops::Range<u32> {
        self.byte_start..self.byte_end
    }

    const fn source_utf16_range(self) -> std::ops::Range<u32> {
        self.utf16_start..self.utf16_end
    }

    const fn inline_source_range(self) -> std::ops::Range<u32> {
        self.inline_byte_start..self.inline_byte_end
    }

    const fn inline_source_utf16_range(self) -> std::ops::Range<u32> {
        self.inline_utf16_start..self.inline_utf16_end
    }
}

enum ResolvedHotInlineDemand {
    InlineLeaf {
        command: InlineRefinementCommand,
        identity: HotInlineLeafIdentity,
        inline_source: std::ops::Range<u32>,
        inline_source_utf16: std::ops::Range<u32>,
        fence: M11PublishedInlineLeafFence,
    },
    PreparedInlineLeaf {
        command: InlineRefinementCommand,
        identity: HotInlineLeafIdentity,
        inline_source: std::ops::Range<u32>,
        inline_source_utf16: std::ops::Range<u32>,
        parser_profile: flark_engine::ParserProfileId,
        fence: M11RecursiveGreenInlineLeafFence,
    },
    IndentedCodeLeaf {
        command: InlineRefinementCommand,
        identity: HotInlineLeafIdentity,
        parser_profile: flark_engine::ParserProfileId,
        fence: M11PublishedIndentedCodeLeafFence,
    },
    BlockQuoteLeaf {
        command: InlineRefinementCommand,
        identity: HotInlineLeafIdentity,
        parser_profile: flark_engine::ParserProfileId,
        fence: M11PublishedBlockQuoteLeafFence,
    },
    BulletListLeaf {
        command: InlineRefinementCommand,
        identity: HotInlineLeafIdentity,
        parser_profile: flark_engine::ParserProfileId,
        fence: M11PublishedBulletListLeafFence,
    },
    BulletListItem {
        command: InlineRefinementCommand,
        identity: HotInlineLeafIdentity,
        parser_profile: flark_engine::ParserProfileId,
        fence: M11PublishedBulletListItemProjectionFence,
    },
    OrderedListItem {
        command: InlineRefinementCommand,
        identity: HotInlineLeafIdentity,
        parser_profile: flark_engine::ParserProfileId,
        fence: M11PublishedOrderedListItemProjectionFence,
    },
    Unsupported(Box<HotInlineReady>),
}

impl ResolvedHotInlineDemand {
    const fn identity(&self) -> HotInlineLeafIdentity {
        match self {
            Self::InlineLeaf { identity, .. } => *identity,
            Self::PreparedInlineLeaf { identity, .. } => *identity,
            Self::IndentedCodeLeaf { identity, .. } => *identity,
            Self::BlockQuoteLeaf { identity, .. } => *identity,
            Self::BulletListLeaf { identity, .. } => *identity,
            Self::BulletListItem { identity, .. } => *identity,
            Self::OrderedListItem { identity, .. } => *identity,
            Self::Unsupported(ready) => ready.identity,
        }
    }

    const fn command(&self) -> InlineRefinementCommand {
        match self {
            Self::InlineLeaf { command, .. } => *command,
            Self::PreparedInlineLeaf { command, .. } => *command,
            Self::IndentedCodeLeaf { command, .. } => *command,
            Self::BlockQuoteLeaf { command, .. } => *command,
            Self::BulletListLeaf { command, .. } => *command,
            Self::BulletListItem { command, .. } => *command,
            Self::OrderedListItem { command, .. } => *command,
            Self::Unsupported(ready) => ready.command,
        }
    }
}

enum RunningHotInlineJob {
    Inline(Box<M11InlineProjectionJob>),
    IndentedCode(Box<M11IndentedCodeProjectionJob>),
    BlockQuote(Box<M11BlockQuoteProjectionJob>),
    BulletList(Box<M11BulletListProjectionJob>),
    BulletListItem(Box<M11BulletListItemProjectionJob>),
}

impl RunningHotInlineJob {
    const fn maximum_poll_transitions(&self) -> usize {
        match self {
            Self::Inline(_) => M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
            Self::IndentedCode(_) => M11_INDENTED_CODE_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
            Self::BlockQuote(_) => M11_BLOCK_QUOTE_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
            Self::BulletList(_) => M11_BULLET_LIST_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
            Self::BulletListItem(_) => M11_BULLET_LIST_PROJECTION_JOB_MAX_POLL_TRANSITIONS,
        }
    }

    fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), CandidateEndpointError> {
        match self {
            Self::Inline(job) => job.begin_abort(runtime)?,
            Self::IndentedCode(job) => job.begin_cancel(runtime)?,
            Self::BlockQuote(job) => job.begin_cancel(runtime)?,
            Self::BulletList(job) => job.begin_cancel(runtime)?,
            Self::BulletListItem(job) => job.begin_cancel(runtime)?,
        }
        Ok(())
    }

    fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<(usize, bool), CandidateEndpointError> {
        match self {
            Self::Inline(job) => {
                let poll = job.poll_abort(runtime, fuel)?;
                Ok((poll.transitions(), poll.complete()))
            }
            Self::IndentedCode(job) => {
                let poll = job.poll_cancel(runtime, fuel)?;
                Ok((poll.transitions(), poll.complete()))
            }
            Self::BlockQuote(job) => {
                let poll = job.poll_cancel(runtime, fuel)?;
                Ok((poll.transitions(), poll.complete()))
            }
            Self::BulletList(job) => {
                let poll = job.poll_cancel(runtime, fuel)?;
                Ok((poll.transitions(), poll.complete()))
            }
            Self::BulletListItem(job) => {
                let poll = job.poll_cancel(runtime, fuel)?;
                Ok((poll.transitions(), poll.complete()))
            }
        }
    }
}

struct RunningHotInline {
    command: InlineRefinementCommand,
    identity: HotInlineLeafIdentity,
    inline_source: std::ops::Range<u32>,
    inline_source_utf16: std::ops::Range<u32>,
    parser_profile: flark_engine::ParserProfileId,
    job: RunningHotInlineJob,
}

pub(crate) enum HotInlineProjectionRoot {
    Inline(M11InlineProjectionRoot),
    IndentedCode(M11IndentedCodeProjectionRoot),
    BlockQuote(M11BlockQuoteProjectionRoot),
    BulletList(M11BlockQuoteProjectionRoot),
    BulletListItem {
        root: M11BlockQuoteProjectionRoot,
        selected_item_ordinal: u32,
        canonical_line_ending: M11HotInlineCanonicalLineEnding,
    },
    OrderedListItem {
        root: M11BlockQuoteProjectionRoot,
        selected_item_ordinal: u32,
        canonical_line_ending: M11HotInlineCanonicalLineEnding,
        opening_marker_start: u32,
        opening_marker_end: u32,
        marker_value: u32,
    },
}

impl HotInlineProjectionRoot {
    fn begin_release(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), CandidateEndpointError> {
        match self {
            Self::Inline(root) => root.begin_release(runtime)?,
            Self::IndentedCode(root) => root.begin_release(runtime)?,
            Self::BlockQuote(root) => root.begin_release(runtime)?,
            Self::BulletList(root) => root.begin_release(runtime)?,
            Self::BulletListItem { root, .. } => root.begin_release(runtime)?,
            Self::OrderedListItem { root, .. } => root.begin_release(runtime)?,
        }
        Ok(())
    }

    fn poll_release(
        &self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<(usize, bool), CandidateEndpointError> {
        let poll = match self {
            Self::Inline(root) => root.poll_release(runtime, fuel)?,
            Self::IndentedCode(root) => root.poll_release(runtime, fuel)?,
            Self::BlockQuote(root) => root.poll_release(runtime, fuel)?,
            Self::BulletList(root) => root.poll_release(runtime, fuel)?,
            Self::BulletListItem { root, .. } => root.poll_release(runtime, fuel)?,
            Self::OrderedListItem { root, .. } => root.poll_release(runtime, fuel)?,
        };
        Ok((poll.receipt().transitions, poll.complete()))
    }
}

enum HotInlineState {
    AwaitingReferenceResolver(Box<ResolvedHotInlineDemand>),
    Running(Box<RunningHotInline>),
    Ready(Box<HotInlineReady>),
    Cancelling {
        job: RunningHotInlineJob,
        begun: bool,
        replacement: Option<Box<ResolvedHotInlineDemand>>,
    },
    Releasing {
        root: Box<HotInlineProjectionRoot>,
        authority: Option<M11ParserSourceRangeAuthority>,
        begun: bool,
        replacement: Option<Box<ResolvedHotInlineDemand>>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ViewportInlineBatchLimits {
    pub(crate) maximum_structural_entries: u32,
    pub(crate) maximum_storage_pages: u32,
    pub(crate) maximum_inline_leaves: u32,
    pub(crate) maximum_inline_leaf_source_bytes: u32,
    pub(crate) maximum_inline_source_bytes: u64,
    pub(crate) maximum_fact_records: u64,
    pub(crate) maximum_projection_bytes: u64,
    pub(crate) maximum_parser_transitions: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ViewportInlineBatchCommand {
    pub(crate) binding: SessionBinding,
    pub(crate) viewport_generation: u32,
    pub(crate) source_version: SourceVersion,
    pub(crate) base_ack: StructuralAck,
    pub(crate) start_entry_ordinal: u64,
    pub(crate) start_byte_offset: u32,
    pub(crate) start_utf16_offset: u32,
    pub(crate) end_byte_offset: u32,
    pub(crate) end_utf16_offset: u32,
    pub(crate) limits: ViewportInlineBatchLimits,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ViewportInlineLeafGeometry {
    kind: M11BlockSequenceEntryKind,
    entry_ordinal: u64,
    frame: M11RecursiveGreenFrameId,
    block_source: Range<u32>,
    block_source_utf16: Range<u32>,
    inline_source: Range<u32>,
    inline_source_utf16: Range<u32>,
}

struct ViewportRecursiveGreenInlineLeaf {
    geometry: ViewportInlineLeafGeometry,
    fence: M11RecursiveGreenInlineLeafFence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ViewportGreenRowReceipt {
    visited_rows: u64,
    storage_pages_visited: u64,
    next_row_ordinal: u64,
    next_byte_offset: u64,
    next_utf16_offset: u64,
}

impl ViewportGreenRowReceipt {
    const fn visited_entries(self) -> u64 {
        self.visited_rows
    }

    const fn storage_pages_visited(self) -> u64 {
        self.storage_pages_visited
    }

    const fn next_byte_offset(self) -> u64 {
        self.next_byte_offset
    }

    const fn next_utf16_offset(self) -> u64 {
        self.next_utf16_offset
    }
}

struct ViewportInlineActiveJob {
    geometry: ViewportInlineLeafGeometry,
    job: Box<M11InlineProjectionJob>,
}

enum ViewportInlineLeafPublication {
    Authoritative(M11InlineProjectionRoot),
    Unsupported(M11InlineProjectionUnsupportedRecord),
}

struct ViewportInlineLeafReady {
    geometry: ViewportInlineLeafGeometry,
    parser_profile: flark_engine::ParserProfileId,
    authority: Option<M11ParserSourceRangeAuthority>,
    publication: ViewportInlineLeafPublication,
}

struct RunningViewportInlineBatch {
    command: ViewportInlineBatchCommand,
    descriptor: M11CandidateDescriptor,
    range_receipt: ViewportGreenRowReceipt,
    pending: VecDeque<ViewportRecursiveGreenInlineLeaf>,
    reference_resolver: Option<M11ReferenceResolver>,
    active: Option<ViewportInlineActiveJob>,
    ready: Vec<ViewportInlineLeafReady>,
    total_inline_source_bytes: u64,
    total_parser_transitions: u64,
    total_fact_records: u64,
    total_ready_roots: u32,
}

struct ViewportInlineBatchReady {
    command: ViewportInlineBatchCommand,
    descriptor: M11CandidateDescriptor,
    range_receipt: ViewportGreenRowReceipt,
    leaves: Vec<ViewportInlineLeafReady>,
    total_inline_source_bytes: u64,
    total_parser_transitions: u64,
    total_fact_records: u64,
    total_ready_roots: u32,
}

struct ReleasingViewportInlineRoot {
    root: M11InlineProjectionRoot,
    authority: Option<M11ParserSourceRangeAuthority>,
    begun: bool,
}

enum ViewportPreparedChildPublication {
    Authoritative(M11InlineProjectionRoot),
    Unsupported(Box<[u8]>),
}

struct ViewportPreparedChild {
    geometry: ViewportInlineLeafGeometry,
    parser_profile: flark_engine::ParserProfileId,
    authority: Option<M11ParserSourceRangeAuthority>,
    publication: ViewportPreparedChildPublication,
    binding: M11HotInlineSidecarBinding,
    directory: ViewportPresentationDirectoryEntry,
}

struct ViewportActiveChild {
    directory_index: u32,
    encoder: M11HotInlineSidecarSnapshotEncoder,
    root: Option<M11InlineProjectionRoot>,
    authority: Option<M11ParserSourceRangeAuthority>,
    next_frame_ordinal: u32,
    next_node_ordinal: Option<u64>,
}

struct StreamingViewportPresentation {
    offer: ViewportPresentationBegin,
    directory: Vec<ViewportPresentationDirectoryEntry>,
    directory_frame: Option<Box<[u8]>>,
    pending: Vec<ViewportPreparedChild>,
    active: Option<ViewportActiveChild>,
    releasing: Option<ReleasingViewportInlineRoot>,
    phase: StreamPhase,
    transport: Option<ViewportPresentationTransportDigest>,
    next_frame_ordinal: u32,
    next_record_ordinal: u32,
    packet: PacketBuilder,
    lookahead: Option<M11HotInlineSidecarFrame>,
    actual_child_frame_count: u32,
    actual_child_encoded_bytes: u32,
    commit: Option<ViewportPresentationCommitRequest>,
    expected_ack: Option<ViewportPresentationAck>,
}

struct ViewportPresentationPreparationFailure {
    error: CandidateEndpointError,
    cleanup: ViewportInlineBatchCleanup,
}

struct ViewportInlineBatchCleanup {
    active_job: Option<Box<M11InlineProjectionJob>>,
    active_abort_begun: bool,
    ready: Vec<ViewportInlineLeafReady>,
    prepared: Vec<ViewportPreparedChild>,
    active_child: Option<ViewportActiveChild>,
    releasing: Option<ReleasingViewportInlineRoot>,
    hot_replacement: Option<Box<ResolvedHotInlineDemand>>,
}

enum ViewportInlineBatchState {
    Running(Box<RunningViewportInlineBatch>),
    Ready(Box<ViewportInlineBatchReady>),
    Streaming(Box<StreamingViewportPresentation>),
    Cancelling(Box<ViewportInlineBatchCleanup>),
}

impl RunningViewportInlineBatch {
    fn into_ready(self) -> ViewportInlineBatchReady {
        debug_assert!(self.pending.is_empty());
        debug_assert!(self.active.is_none());
        ViewportInlineBatchReady {
            command: self.command,
            descriptor: self.descriptor,
            range_receipt: self.range_receipt,
            leaves: self.ready,
            total_inline_source_bytes: self.total_inline_source_bytes,
            total_parser_transitions: self.total_parser_transitions,
            total_fact_records: self.total_fact_records,
            total_ready_roots: self.total_ready_roots,
        }
    }

    fn into_cleanup(mut self) -> ViewportInlineBatchCleanup {
        drop(self.pending);
        ViewportInlineBatchCleanup {
            active_job: self.active.take().map(|active| active.job),
            active_abort_begun: false,
            ready: self.ready,
            prepared: Vec::new(),
            active_child: None,
            releasing: None,
            hot_replacement: None,
        }
    }
}

impl ViewportInlineBatchReady {
    fn into_cleanup(self) -> ViewportInlineBatchCleanup {
        ViewportInlineBatchCleanup {
            active_job: None,
            active_abort_begun: false,
            ready: self.leaves,
            prepared: Vec::new(),
            active_child: None,
            releasing: None,
            hot_replacement: None,
        }
    }
}

impl StreamingViewportPresentation {
    fn into_cleanup(mut self) -> ViewportInlineBatchCleanup {
        ViewportInlineBatchCleanup {
            active_job: None,
            active_abort_begun: false,
            ready: Vec::new(),
            prepared: self.pending,
            active_child: self.active.take(),
            releasing: self.releasing.take(),
            hot_replacement: None,
        }
    }
}

/// Typed, move-only late-inline result ready for the credited sidecar layer.
///
/// It deliberately is not a canonical candidate role and cannot enter the
/// structural publication stream.
pub(crate) struct HotInlineReady {
    command: InlineRefinementCommand,
    identity: HotInlineLeafIdentity,
    inline_source: std::ops::Range<u32>,
    inline_source_utf16: std::ops::Range<u32>,
    parser_profile: flark_engine::ParserProfileId,
    authority: Option<M11ParserSourceRangeAuthority>,
    publication: HotInlineReadyPublication,
}

pub(crate) enum HotInlineReadyPublication {
    Authoritative(Box<HotInlineProjectionRoot>),
    Unsupported(HotInlineUnsupported),
}

pub(crate) enum HotInlineUnsupported {
    NotInlineLeaf { kind: M11BlockSequenceEntryKind },
    Parser(M11InlineProjectionUnsupportedRecord),
    LegacyBlockTarget { target: InlineRefinementTarget },
}

#[allow(dead_code)] // Accessors support focused parser/sidecar authority tests.
impl HotInlineReady {
    #[must_use]
    pub(crate) const fn command(&self) -> InlineRefinementCommand {
        self.command
    }

    #[must_use]
    pub(crate) const fn block_kind(&self) -> M11BlockSequenceEntryKind {
        self.identity.kind
    }

    #[must_use]
    pub(crate) const fn entry_ordinal(&self) -> Option<u64> {
        match self.identity.owner {
            HotInlineLeafOwner::BlockOrdinal(ordinal) => Some(ordinal),
            HotInlineLeafOwner::RecursiveGreenFrame(_) => None,
        }
    }

    #[must_use]
    pub(crate) const fn block_source_range(&self) -> std::ops::Range<u32> {
        self.identity.source_range()
    }

    #[must_use]
    pub(crate) const fn block_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.identity.source_utf16_range()
    }

    #[must_use]
    pub(crate) fn inline_source_range(&self) -> std::ops::Range<u32> {
        self.inline_source.clone()
    }

    #[must_use]
    pub(crate) fn inline_source_utf16_range(&self) -> std::ops::Range<u32> {
        self.inline_source_utf16.clone()
    }

    #[must_use]
    pub(crate) const fn parser_profile(&self) -> flark_engine::ParserProfileId {
        self.parser_profile
    }

    #[must_use]
    fn into_parts(
        self,
    ) -> (
        InlineRefinementCommand,
        std::ops::Range<u32>,
        std::ops::Range<u32>,
        std::ops::Range<u32>,
        std::ops::Range<u32>,
        HotInlineLeafOwner,
        flark_engine::ParserProfileId,
        Option<M11ParserSourceRangeAuthority>,
        HotInlineReadyPublication,
    ) {
        (
            self.command,
            self.identity.source_range(),
            self.identity.source_utf16_range(),
            self.inline_source,
            self.inline_source_utf16,
            self.identity.owner,
            self.parser_profile,
            self.authority,
            self.publication,
        )
    }
}

/// One active exact candidate plus at most one explicitly fuel-drained owner.
struct RollingBulletListLocalEdit {
    plan: M11BulletListLocalDeltaPlan,
    current_source: flark_engine::SourceVersion,
    predecessor_end_byte: usize,
    predecessor_end_utf16: usize,
    successor_start_byte: usize,
    successor_start_utf16: usize,
}

pub(crate) struct CandidateEndpoint {
    active: Option<ActiveCandidate>,
    cleanup: Option<CandidateCleanup>,
    retained: Option<RetainedCandidateBase>,
    recursive_green: RecursiveGreenEndpointSlot,
    bullet_list_local_edit: Option<RollingBulletListLocalEdit>,
    viewport_inline_batch: Option<ViewportInlineBatchState>,
    pending_viewport_unavailable: Option<(u32, ViewportPresentationUnavailableReason)>,
    last_viewport_generation: u32,
    hot_inline: Option<HotInlineState>,
    hot_inline_sidecar: Option<StreamingHotInlineSidecar>,
    last_hot_inline_generation: u32,
    closing: bool,
}

impl CandidateEndpoint {
    pub(crate) const fn new() -> Self {
        Self {
            active: None,
            cleanup: None,
            retained: None,
            recursive_green: RecursiveGreenEndpointSlot::new(),
            bullet_list_local_edit: None,
            viewport_inline_batch: None,
            pending_viewport_unavailable: None,
            last_viewport_generation: 0,
            hot_inline: None,
            hot_inline_sidecar: None,
            last_hot_inline_generation: 0,
            closing: false,
        }
    }

    pub(crate) fn discard_bullet_list_local_edit_plan(&mut self) {
        self.bullet_list_local_edit = None;
    }

    /// Prepares one exact local list island while the runtime is still at the
    /// edit base. Failed or ineligible preparation always invalidates any
    /// previously rolling authority.
    pub(crate) fn prepare_bullet_list_local_edit(
        &mut self,
        runtime: &DocumentRuntime,
        changed_bytes: Range<usize>,
        changed_utf16: Range<usize>,
    ) -> Result<bool, CandidateEndpointError> {
        let previous = self.bullet_list_local_edit.take();
        let prepared = (|| {
            if self.closing || self.active.is_some() {
                return Err(CandidateEndpointError::Busy);
            }
            if changed_bytes.start > changed_bytes.end
                || changed_utf16.start > changed_utf16.end
                || changed_bytes.is_empty() != changed_utf16.is_empty()
            {
                return Err(CandidateEndpointError::InvalidAuthority);
            }
            let current_source = runtime
                .current_source_version()
                .ok_or(CandidateEndpointError::InvalidAuthority)?;
            let lease = runtime.snapshot_current_source()?;
            if lease.utf16_offset_for_byte(changed_bytes.start).ok() != Some(changed_utf16.start)
                || lease.utf16_offset_for_byte(changed_bytes.end).ok() != Some(changed_utf16.end)
            {
                return Err(CandidateEndpointError::InvalidAuthority);
            }
            drop(lease);

            if let Some(rolling) = previous {
                let remains_inside = rolling.current_source == current_source
                    && changed_bytes.start > rolling.predecessor_end_byte
                    && changed_bytes.end < rolling.successor_start_byte
                    && changed_utf16.start > rolling.predecessor_end_utf16
                    && changed_utf16.end < rolling.successor_start_utf16;
                if remains_inside {
                    return Ok(Some(rolling));
                }
            }

            let Some(retained) = self.retained.as_ref() else {
                return Ok(None);
            };
            let point = M11BlockSequencePoint::new(
                changed_bytes.start,
                changed_utf16.start,
                SourceBoundaryAffinity::After,
            );
            let fence = match resolve_m11_published_bullet_list_leaf_fence(
                runtime,
                &retained.publication,
                point,
            ) {
                Ok(fence) => fence,
                Err(_) => return Ok(None),
            };
            let plan = match M11BulletListLocalDeltaPlan::new(runtime, fence, changed_bytes) {
                Ok(plan) => plan,
                Err(_) => return Ok(None),
            };
            Ok(Some(RollingBulletListLocalEdit {
                current_source,
                predecessor_end_byte: plan.prefix_witness_byte_end(),
                predecessor_end_utf16: plan.prefix_witness_utf16_end(),
                successor_start_byte: plan.suffix_witness_byte_start(),
                successor_start_utf16: plan.suffix_witness_utf16_start(),
                plan,
            }))
        })();
        match prepared {
            Ok(Some(rolling)) => {
                self.bullet_list_local_edit = Some(rolling);
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Cancels target work before a new source edit while preserving any
    /// reusable local-list authority. Exact stream recovery is specialized
    /// below once the local path has entered publication.
    pub(crate) fn cancel_for_edit(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), CandidateEndpointError> {
        self.cancel_hot_inline();
        self.recursive_green.request_cancel_pending()?;
        if self.cleanup.is_some() {
            if self.active.is_none() && self.retained.is_some() {
                return Ok(());
            }
            self.bullet_list_local_edit = None;
            return Err(CandidateEndpointError::InvalidState);
        }

        match self.active.take() {
            Some(ActiveCandidate::Streaming(mut streaming)) => {
                let can_detach_exact_base = streaming.stream.is_some()
                    && streaming.sealed_publication.is_none()
                    && streaming.superseded_exact_base.is_none()
                    && streaming.exact_base_recovery.is_some();
                if can_detach_exact_base {
                    let mut stream = streaming
                        .stream
                        .take()
                        .ok_or(CandidateEndpointError::InvalidState)?;
                    let base = match stream.take_exact_base_for_cancel(runtime) {
                        Ok(base) => base,
                        Err(error) => {
                            streaming.stream = Some(stream);
                            self.active = Some(ActiveCandidate::Streaming(streaming));
                            self.bullet_list_local_edit = None;
                            return Err(error.into());
                        }
                    };
                    let recovery = streaming
                        .exact_base_recovery
                        .take()
                        .ok_or(CandidateEndpointError::InvalidState)?;
                    self.restore_exact_base(ExactCandidateBase {
                        publication: Box::new(base),
                        ack: recovery.ack,
                        restart: Some(recovery.restart),
                    })?;
                    self.cleanup = Some(CandidateCleanup::Stream {
                        stream: Box::new(stream),
                        begun: false,
                    });
                    return Ok(());
                }
                self.active = Some(ActiveCandidate::Streaming(streaming));
            }
            other => self.active = other,
        }

        let result = self.cancel_preserving_bullet_list_local_edit();
        if result.is_err() || (self.bullet_list_local_edit.is_some() && self.retained.is_none()) {
            self.bullet_list_local_edit = None;
        }
        result
    }

    /// Clears any residual candidate work when an already-applied edit begins
    /// SourceFacts, without discarding the exact local-list authority prepared
    /// against that edit's base immediately before the source advanced.
    pub(crate) fn cancel_for_source_facts_after_edit(
        &mut self,
    ) -> Result<(), CandidateEndpointError> {
        self.cancel_preserving_bullet_list_local_edit()
    }

    pub(crate) fn start(
        &mut self,
        certified: CertifiedSource,
        binding: SessionBinding,
        completion: SourceFactsCompletionEvent,
    ) -> Result<(), CandidateEndpointError> {
        if self.closing || self.active.is_some() {
            return Err(CandidateEndpointError::Busy);
        }
        let source = certified.source();
        if source.revision().get() != u64::from(completion.worker_replica_revision)
            || source.byte_len()
                != usize::try_from(completion.utf8_length)
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?
            || source.utf16_len()
                != usize::try_from(completion.utf16_length)
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?
            || certified.facts().fingerprint().rolling_hash().words() != completion.content_hash128
        {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let parse_generation = binding
            .worker_generation
            .checked_add(completion.ui_revision)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        if parse_generation == 0 {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let syntax_profile = u32::try_from(certified.parser_profile().get())
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let recursive_green_plan = M11PersistentRecursiveGreenCleanPlan::new(
            certified.exact_parse_lease(),
            certified.exact_parse_lease(),
            syntax_profile,
        )?;
        let job = M11CleanParseJob::new(certified.exact_parse_lease())?;
        let publication_path = if self.retained.is_none() && self.recursive_green.is_unowned() {
            CleanPublicationPath::RecursiveGreenInitial
        } else {
            CleanPublicationPath::LegacyBlocks
        };
        self.bullet_list_local_edit = None;
        self.cancel_hot_inline();
        self.active = Some(ActiveCandidate::Parsing(Box::new(ParsingCandidate {
            context: CandidateContext {
                binding,
                completion,
                parse_generation,
            },
            certified,
            job,
            publication_path,
        })));
        self.recursive_green.start_clean(recursive_green_plan)?;
        Ok(())
    }

    /// Starts one exact-base candidate after the source owner has accepted the
    /// runtime's bounded persistent-SourceFacts splice for `completion`.
    ///
    /// A typed restart ineligibility may fall back to definitive exact-clean
    /// parsing while retaining exact-base publication reuse. Invalid source,
    /// lineage, binding, or host authority still surfaces to the caller rather
    /// than silently becoming a full snapshot.
    pub(crate) fn start_incremental(
        &mut self,
        runtime: &DocumentRuntime,
        target_lease: SourceSnapshotLease,
        witness: Box<PersistentSourceFactsDeltaWitness>,
        binding: SessionBinding,
        completion: SourceFactsCompletionEvent,
    ) -> Result<(), CandidateEndpointError> {
        if self.closing || self.active.is_some() {
            return Err(CandidateEndpointError::Busy);
        }
        let target = witness.target();
        let persistent = runtime
            .persistent_source_facts()
            .ok_or(CandidateEndpointError::InvalidAuthority)?;
        let summary = persistent.summary();
        if target_lease.version() != target
            || persistent.source() != target
            || persistent.parser_profile() != witness.parser_profile()
            || persistent.profile() != witness.profile()
            || target.revision().get() != u64::from(completion.worker_replica_revision)
            || target.byte_len()
                != usize::try_from(completion.utf8_length)
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?
            || target.utf16_len()
                != usize::try_from(completion.utf16_length)
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?
            || summary.byte_len() != u64::from(completion.utf8_length)
            || summary.utf16_len() != u64::from(completion.utf16_length)
            || summary.rolling_hash().words() != completion.content_hash128
        {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let parse_generation = binding
            .worker_generation
            .checked_add(completion.ui_revision)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        if parse_generation == 0 {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let base_source_bytes = u64::try_from(witness.base().byte_len())
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let base_source_utf16 = u64::try_from(witness.base().utf16_len())
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let syntax_profile = u32::try_from(witness.parser_profile().get())
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;

        self.cancel_hot_inline();
        let mut retained = self
            .retained
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?;
        let restart = match retained.restart.take() {
            Some(restart) => restart,
            None => {
                self.retained = Some(retained);
                return Err(CandidateEndpointError::InvalidState);
            }
        };
        let descriptor = match retained.publication.descriptor(runtime) {
            Ok(descriptor) => descriptor,
            Err(error) => {
                retained.restart = Some(restart);
                self.retained = Some(retained);
                return Err(error.into());
            }
        };
        if descriptor.source_revision != witness.base().revision().get()
            || descriptor.source_root != witness.base().root().get()
            || descriptor.source_bytes != base_source_bytes
            || descriptor.source_utf16 != base_source_utf16
            || descriptor.syntax_profile != syntax_profile
            || retained.ack.host_revision >= parse_generation
        {
            retained.restart = Some(restart);
            self.retained = Some(retained);
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let parser_binding = M11ParserBinding::new(witness.parser_profile(), GRAMMAR_REVISION);
        let context = CandidateContext {
            binding,
            completion,
            parse_generation,
        };
        let base = ExactCandidateBase {
            publication: retained.publication,
            ack: retained.ack,
            restart: None,
        };
        let recursive_green_base_ack = base.ack;
        let recursive_green_base_edit = witness
            .exact_parser_base_byte_range()
            .unwrap_or(witness.base_byte_range())
            .clone();
        if let Some(rolling) = self.bullet_list_local_edit.take() {
            if rolling.plan.source() == witness.base() && rolling.plan.binding() == parser_binding {
                let mut base = base;
                base.restart = Some(restart);
                let prefix = runtime
                    .mint_exact_unchanged_prefix_witness(
                        rolling.plan.source(),
                        rolling.plan.prefix_witness_byte_end(),
                        rolling.plan.prefix_witness_utf16_end(),
                    )
                    .and_then(|witness| runtime.take_exact_unchanged_prefix_witness(witness));
                let suffix = runtime
                    .mint_exact_unchanged_suffix_witness(
                        rolling.plan.source(),
                        rolling.plan.suffix_witness_byte_start(),
                        rolling.plan.suffix_witness_utf16_start(),
                    )
                    .and_then(|witness| runtime.take_exact_unchanged_suffix_witness(witness));
                let (prefix, suffix) = match (prefix, suffix) {
                    (Ok(prefix), Ok(suffix)) => (prefix, suffix),
                    _ => {
                        drop(target_lease);
                        return self.activate_exact_clean_fallback(runtime, context, base, witness);
                    }
                };
                let predecessor_end_byte = prefix.byte_end();
                let predecessor_end_utf16 = prefix.utf16_end();
                let successor_start_byte = suffix.target_byte_start();
                let successor_start_utf16 = suffix.target_utf16_start();
                let job = match M11BulletListLocalDeltaJob::new(
                    rolling.plan,
                    prefix,
                    suffix,
                    target_lease,
                ) {
                    Ok(job) => job,
                    Err(_) => {
                        return self.activate_exact_clean_fallback(runtime, context, base, witness);
                    }
                };
                self.active = Some(ActiveCandidate::ParsingBulletListLocal(Box::new(
                    ParsingBulletListLocalCandidate {
                        context,
                        job,
                        base,
                        witness,
                        target_source: target,
                        target_binding: parser_binding,
                        predecessor_end_byte,
                        predecessor_end_utf16,
                        successor_start_byte,
                        successor_start_utf16,
                    },
                )));
                self.recursive_green.start_incremental(
                    runtime,
                    recursive_green_base_ack,
                    recursive_green_base_edit,
                    syntax_profile,
                )?;
                return Ok(());
            }
        }
        match restart {
            CandidateRestartAuthority::Leading(checkpoint) => {
                let prefix = match runtime.mint_exact_unchanged_prefix_witness(
                    checkpoint.source(),
                    checkpoint.prefix_end_byte() as usize,
                    checkpoint.prefix_end_utf16() as usize,
                ) {
                    Ok(prefix) => prefix,
                    Err(error) => {
                        self.retained = Some(RetainedCandidateBase {
                            publication: base.publication,
                            ack: base.ack,
                            restart: Some(CandidateRestartAuthority::Leading(checkpoint)),
                        });
                        return Err(error.into());
                    }
                };
                let restart_is_exact = match target_physical_line_cut_is_exact(
                    &target_lease,
                    prefix.byte_end(),
                    prefix.utf16_end(),
                ) {
                    Ok(is_exact) => is_exact && prefix.target() == target,
                    Err(error) => {
                        self.retained = Some(RetainedCandidateBase {
                            publication: base.publication,
                            ack: base.ack,
                            restart: Some(CandidateRestartAuthority::Leading(checkpoint)),
                        });
                        return Err(error);
                    }
                };
                if !restart_is_exact {
                    self.retained = Some(RetainedCandidateBase {
                        publication: base.publication,
                        ack: base.ack,
                        restart: Some(CandidateRestartAuthority::Leading(checkpoint)),
                    });
                    return Err(CandidateEndpointError::InvalidAuthority);
                }
                let prefix = match runtime.take_exact_unchanged_prefix_witness(prefix) {
                    Ok(prefix) => prefix,
                    Err(error) => {
                        self.retained = Some(RetainedCandidateBase {
                            publication: base.publication,
                            ack: base.ack,
                            restart: Some(CandidateRestartAuthority::Leading(checkpoint)),
                        });
                        return Err(error.into());
                    }
                };
                let job = match M11LeadingReferencesCropParseJob::new(
                    checkpoint,
                    prefix,
                    target_lease,
                    parser_binding,
                ) {
                    Ok(job) => job,
                    Err(error) => {
                        self.retained = Some(RetainedCandidateBase {
                            publication: base.publication,
                            ack: base.ack,
                            restart: None,
                        });
                        return Err(error.into());
                    }
                };
                self.active = Some(ActiveCandidate::ParsingExact(Box::new(
                    ParsingExactCandidate {
                        context,
                        job,
                        base,
                        witness,
                    },
                )));
            }
            CandidateRestartAuthority::Ordinary(checkpoints) => {
                let route = match select_ordinary_crop_route(
                    &checkpoints,
                    witness
                        .exact_parser_base_byte_range()
                        .unwrap_or(witness.base_byte_range())
                        .clone(),
                ) {
                    Ok(Some(route)) => route,
                    Ok(None) => {
                        let mut base = base;
                        base.restart = Some(CandidateRestartAuthority::Ordinary(checkpoints));
                        return self.activate_exact_clean_fallback(runtime, context, base, witness);
                    }
                    Err(error) => {
                        self.retained = Some(RetainedCandidateBase {
                            publication: base.publication,
                            ack: base.ack,
                            restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                        });
                        return Err(error);
                    }
                };
                let job = match route {
                    OrdinaryCropRoute::Interior(selection) => {
                        let prefix = match runtime.mint_exact_unchanged_prefix_witness(
                            selection.source(),
                            selection.restart_prefix_end_byte() as usize,
                            selection.restart_prefix_end_utf16() as usize,
                        ) {
                            Ok(prefix) => prefix,
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                                });
                                return Err(error.into());
                            }
                        };
                        let suffix = match runtime.mint_exact_unchanged_suffix_witness(
                            selection.source(),
                            selection.convergence_suffix_start_byte() as usize,
                            selection.convergence_suffix_start_utf16() as usize,
                        ) {
                            Ok(suffix) => suffix,
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                                });
                                return Err(error.into());
                            }
                        };
                        let prefix = match runtime.take_exact_unchanged_prefix_witness(prefix) {
                            Ok(prefix) => prefix,
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                                });
                                return Err(error.into());
                            }
                        };
                        let suffix = match runtime.take_exact_unchanged_suffix_witness(suffix) {
                            Ok(suffix) => suffix,
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                                });
                                return Err(error.into());
                            }
                        };
                        let cuts_are_exact = match (
                            target_physical_line_cut_is_exact(
                                &target_lease,
                                prefix.byte_end(),
                                prefix.utf16_end(),
                            ),
                            target_physical_line_cut_is_exact(
                                &target_lease,
                                suffix.target_byte_start(),
                                suffix.target_utf16_start(),
                            ),
                        ) {
                            (Ok(restart), Ok(convergence)) => {
                                restart
                                    && convergence
                                    && prefix.byte_end() <= suffix.target_byte_start()
                                    && prefix.utf16_end() <= suffix.target_utf16_start()
                            }
                            (Err(error), _) | (_, Err(error)) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                                });
                                return Err(error);
                            }
                        };
                        if !cuts_are_exact {
                            self.retained = Some(RetainedCandidateBase {
                                publication: base.publication,
                                ack: base.ack,
                                restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                            });
                            return Err(CandidateEndpointError::InvalidAuthority);
                        }
                        let exceeds_crop_cap = match segmented_crop_exceeds_byte_cap(
                            &checkpoints,
                            selection,
                            prefix.byte_end(),
                            suffix.target_byte_start(),
                        ) {
                            Ok(exceeds) => exceeds,
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                                });
                                return Err(error);
                            }
                        };
                        if exceeds_crop_cap {
                            let mut base = base;
                            base.restart = Some(CandidateRestartAuthority::Ordinary(checkpoints));
                            return self
                                .activate_exact_clean_fallback(runtime, context, base, witness);
                        }
                        let plan = match M11OrdinaryParagraphCropPlan::new(checkpoints, selection) {
                            Ok(plan) => plan,
                            Err(_) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: None,
                                });
                                return Err(CandidateEndpointError::InvalidState);
                            }
                        };
                        match M11OrdinaryParagraphCropParseJob::new(
                            plan,
                            prefix,
                            suffix,
                            target_lease,
                            parser_binding,
                        ) {
                            Ok(job) => OrdinaryExactJob::Interior(job),
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: None,
                                });
                                return Err(error.into());
                            }
                        }
                    }
                    OrdinaryCropRoute::FromBof(selection) => {
                        let suffix = match runtime.mint_exact_unchanged_suffix_witness(
                            selection.source(),
                            selection.convergence_suffix_start_byte() as usize,
                            selection.convergence_suffix_start_utf16() as usize,
                        ) {
                            Ok(suffix) => suffix,
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                                });
                                return Err(error.into());
                            }
                        };
                        let suffix = match runtime.take_exact_unchanged_suffix_witness(suffix) {
                            Ok(suffix) => suffix,
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                                });
                                return Err(error.into());
                            }
                        };
                        let convergence_is_exact = match target_physical_line_cut_is_exact(
                            &target_lease,
                            suffix.target_byte_start(),
                            suffix.target_utf16_start(),
                        ) {
                            Ok(is_exact) => is_exact,
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                                });
                                return Err(error);
                            }
                        };
                        if !convergence_is_exact {
                            self.retained = Some(RetainedCandidateBase {
                                publication: base.publication,
                                ack: base.ack,
                                restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                            });
                            return Err(CandidateEndpointError::InvalidAuthority);
                        }
                        let exceeds_crop_cap = match segmented_bof_crop_exceeds_byte_cap(
                            &checkpoints,
                            selection,
                            suffix.target_byte_start(),
                        ) {
                            Ok(exceeds) => exceeds,
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                                });
                                return Err(error);
                            }
                        };
                        if exceeds_crop_cap {
                            let mut base = base;
                            base.restart = Some(CandidateRestartAuthority::Ordinary(checkpoints));
                            return self
                                .activate_exact_clean_fallback(runtime, context, base, witness);
                        }
                        let plan =
                            match M11OrdinaryParagraphBofCropPlan::new(checkpoints, selection) {
                                Ok(plan) => plan,
                                Err(_) => {
                                    self.retained = Some(RetainedCandidateBase {
                                        publication: base.publication,
                                        ack: base.ack,
                                        restart: None,
                                    });
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                        match M11OrdinaryParagraphBofCropParseJob::new(
                            plan,
                            suffix,
                            target_lease,
                            parser_binding,
                        ) {
                            Ok(job) => OrdinaryExactJob::FromBof(job),
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: None,
                                });
                                return Err(error.into());
                            }
                        }
                    }
                    OrdinaryCropRoute::ToEof(selection) => {
                        let prefix = match runtime.mint_exact_unchanged_prefix_witness(
                            selection.source(),
                            selection.restart_prefix_end_byte() as usize,
                            selection.restart_prefix_end_utf16() as usize,
                        ) {
                            Ok(prefix) => prefix,
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                                });
                                return Err(error.into());
                            }
                        };
                        let prefix = match runtime.take_exact_unchanged_prefix_witness(prefix) {
                            Ok(prefix) => prefix,
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                                });
                                return Err(error.into());
                            }
                        };
                        let target_eof_is_exact = target_lease
                            .utf16_offset_for_byte(target.byte_len())
                            .map(|observed| observed == target.utf16_len())
                            .and_then(|forward| {
                                target_lease
                                    .byte_offset_for_utf16(target.utf16_len())
                                    .map(|observed| forward && observed == target.byte_len())
                            });
                        let restart_is_exact = target_physical_line_cut_is_exact(
                            &target_lease,
                            prefix.byte_end(),
                            prefix.utf16_end(),
                        );
                        let (target_eof_is_exact, restart_is_exact) =
                            match (target_eof_is_exact, restart_is_exact) {
                                (Ok(eof), Ok(restart)) => (eof, restart),
                                (Err(_), _) | (_, Err(_)) => {
                                    self.retained = Some(RetainedCandidateBase {
                                        publication: base.publication,
                                        ack: base.ack,
                                        restart: Some(CandidateRestartAuthority::Ordinary(
                                            checkpoints,
                                        )),
                                    });
                                    return Err(CandidateEndpointError::InvalidAuthority);
                                }
                            };
                        if !restart_is_exact || !target_eof_is_exact {
                            self.retained = Some(RetainedCandidateBase {
                                publication: base.publication,
                                ack: base.ack,
                                restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                            });
                            return Err(CandidateEndpointError::InvalidAuthority);
                        }
                        let exceeds_crop_cap = match segmented_eof_crop_exceeds_byte_cap(
                            &checkpoints,
                            selection,
                            prefix.byte_end(),
                            target.byte_len(),
                        ) {
                            Ok(exceeds) => exceeds,
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: Some(CandidateRestartAuthority::Ordinary(checkpoints)),
                                });
                                return Err(error);
                            }
                        };
                        if exceeds_crop_cap {
                            let mut base = base;
                            base.restart = Some(CandidateRestartAuthority::Ordinary(checkpoints));
                            return self
                                .activate_exact_clean_fallback(runtime, context, base, witness);
                        }
                        let plan =
                            match M11OrdinaryParagraphEofCropPlan::new(checkpoints, selection) {
                                Ok(plan) => plan,
                                Err(_) => {
                                    self.retained = Some(RetainedCandidateBase {
                                        publication: base.publication,
                                        ack: base.ack,
                                        restart: None,
                                    });
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                        match M11OrdinaryParagraphEofCropParseJob::new(
                            plan,
                            prefix,
                            target_lease,
                            parser_binding,
                        ) {
                            Ok(job) => OrdinaryExactJob::ToEof(job),
                            Err(error) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base.publication,
                                    ack: base.ack,
                                    restart: None,
                                });
                                return Err(error.into());
                            }
                        }
                    }
                };
                self.active = Some(ActiveCandidate::ParsingOrdinaryExact(Box::new(
                    ParsingOrdinaryExactCandidate {
                        context,
                        job,
                        base,
                        witness,
                    },
                )));
            }
            CandidateRestartAuthority::RecursiveGreen { source, binding } => {
                let mut base = base;
                base.restart = Some(CandidateRestartAuthority::RecursiveGreen {
                    source,
                    binding,
                });
                if source != witness.base()
                    || binding != parser_binding
                    || !self
                        .recursive_green
                        .owns_recursive_base_authority(recursive_green_base_ack)
                {
                    self.restore_exact_base(base)?;
                    return Err(CandidateEndpointError::InvalidAuthority);
                }
                let certified = match runtime.certify_current_persistent_source() {
                    Ok(certified) => certified,
                    Err(error) => {
                        self.restore_exact_base(base)?;
                        return Err(error.into());
                    }
                };
                if certified.source() != target
                    || certified.parser_profile() != witness.parser_profile()
                    || certified.source_facts_profile() != witness.profile()
                {
                    self.restore_exact_base(base)?;
                    return Err(CandidateEndpointError::InvalidAuthority);
                }
                self.active = Some(ActiveCandidate::AwaitingRecursiveGreenExact(Box::new(
                    AwaitingRecursiveGreenExactCandidate {
                        context,
                        certified,
                        base,
                        witness,
                    },
                )));
            }
            CandidateRestartAuthority::ExactBaseOnly { source, binding } => {
                let mut base = base;
                base.restart = Some(CandidateRestartAuthority::ExactBaseOnly { source, binding });
                return self.activate_exact_clean_fallback(runtime, context, base, witness);
            }
        }
        self.recursive_green.start_incremental(
            runtime,
            recursive_green_base_ack,
            recursive_green_base_edit,
            syntax_profile,
        )?;
        Ok(())
    }

    fn activate_exact_clean_fallback(
        &mut self,
        runtime: &DocumentRuntime,
        context: CandidateContext,
        base: ExactCandidateBase,
        witness: Box<PersistentSourceFactsDeltaWitness>,
    ) -> Result<(), CandidateEndpointError> {
        let recursive_green_base_ack = base.ack;
        let recursive_green_base_edit = witness
            .exact_parser_base_byte_range()
            .unwrap_or(witness.base_byte_range())
            .clone();
        let syntax_profile = u32::try_from(witness.parser_profile().get())
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        match begin_exact_clean_fallback(runtime, context, base, witness) {
            Ok(active) => {
                self.active = Some(active);
                self.recursive_green.start_incremental(
                    runtime,
                    recursive_green_base_ack,
                    recursive_green_base_edit,
                    syntax_profile,
                )?;
                Ok(())
            }
            Err(failure) => {
                self.cleanup = Some(CandidateCleanup::RetainedPublication {
                    publication: failure.base.publication,
                    begun: false,
                });
                Err(failure.error)
            }
        }
    }

    pub(crate) fn request_viewport_inline_batch(
        &mut self,
        runtime: &DocumentRuntime,
        command: ViewportInlineBatchCommand,
    ) -> Result<(), CandidateEndpointError> {
        if self.closing
            || self.active.is_some()
            || self.viewport_inline_batch.is_some()
            || self.pending_viewport_unavailable.is_some()
            || self.hot_inline.is_some()
            || self.hot_inline_sidecar.is_some()
            || self.cleanup.as_ref().is_some_and(|cleanup| {
                !matches!(cleanup, CandidateCleanup::RetainedPublication { .. })
            })
        {
            return Err(CandidateEndpointError::Busy);
        }
        let row_limits = M11RecursiveGreenRowQueryLimits::new(
            command.limits.maximum_structural_entries,
            u64::from(command.limits.maximum_storage_pages),
            command.limits.maximum_parser_transitions,
            64,
            command.limits.maximum_parser_transitions,
        )
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
        if command.viewport_generation == 0
            || command.viewport_generation <= self.last_viewport_generation
            || command.source_version != command.base_ack.source_version
            || command.binding.document_session != command.source_version.document_session
            || command.start_byte_offset >= command.end_byte_offset
            || command.start_utf16_offset >= command.end_utf16_offset
            || command.end_byte_offset > command.source_version.utf8_length
            || command.end_utf16_offset > command.source_version.utf16_length
        {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let retained = self
            .retained
            .as_ref()
            .ok_or(CandidateEndpointError::InvalidState)?;
        if retained.ack != command.base_ack {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let descriptor = retained.publication.descriptor(runtime)?;
        let source = runtime
            .current_source_version()
            .ok_or(CandidateEndpointError::InvalidAuthority)?;
        if source.revision().get() != u64::from(command.source_version.revision)
            || source.byte_len()
                != usize::try_from(command.source_version.utf8_length)
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?
            || source.utf16_len()
                != usize::try_from(command.source_version.utf16_length)
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?
            || split_u64(source.root().get()) != command.base_ack.source_root
            || descriptor.source_revision != source.revision().get()
            || descriptor.source_root != source.root().get()
            || descriptor.source_bytes
                != u64::try_from(source.byte_len())
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?
            || descriptor.source_utf16
                != u64::try_from(source.utf16_len())
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?
            || descriptor.parse_generation != u64::from(command.base_ack.parse_generation)
            || descriptor.syntax_profile != command.base_ack.syntax_profile
        {
            return Err(CandidateEndpointError::InvalidAuthority);
        }

        let green = self.recursive_green.installed_session(command.base_ack)?;
        let row_window = green.query_renderable_rows(
            runtime,
            M11RecursiveGreenPoint::new(
                usize::try_from(command.start_byte_offset)
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?,
                usize::try_from(command.start_utf16_offset)
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?,
                SourceBoundaryAffinity::After,
            ),
            u64::from(command.end_byte_offset),
            row_limits,
        )?;
        if row_window.start_ordinal() != command.start_entry_ordinal || row_window.rows().is_empty()
        {
            return Err(CandidateEndpointError::InvalidAuthority);
        }

        let query_work = |receipt: flark_engine::parser_internal::M11RecursiveGreenQueryReceipt| {
            receipt
                .node_headers_decoded()
                .checked_add(receipt.summary_combinations())?
                .checked_add(receipt.events_scanned())
        };
        let mut total_parser_transitions =
            query_work(row_window.receipt()).ok_or(CandidateEndpointError::MetricOverflow)?;
        let mut storage_pages_visited = row_window.receipt().storage_pages_visited();
        let mut total_inline_source_bytes = 0_u64;
        let mut pending = VecDeque::new();
        pending
            .try_reserve_exact(
                usize::try_from(command.limits.maximum_inline_leaves)
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?,
            )
            .map_err(|_| CandidateEndpointError::AllocationFailed)?;
        let mut visited_rows = 0_u64;
        let mut next_row_ordinal = command.start_entry_ordinal;
        let mut next_byte_offset = u64::from(command.start_byte_offset);
        let mut next_utf16_offset = u64::from(command.start_utf16_offset);
        for row in row_window.rows() {
            if row.ordinal() != next_row_ordinal {
                return Err(CandidateEndpointError::InvalidAuthority);
            }
            let physical = row.physical_range();
            let physical_utf16 = row.physical_utf16_range();
            let editable = row.editable_range();
            let editable_utf16 = row.editable_utf16_range();
            let editable_valid = match (&editable, &editable_utf16) {
                (Some(bytes), Some(utf16)) => {
                    bytes.start >= physical.start
                        && bytes.end <= physical.end
                        && utf16.start >= physical_utf16.start
                        && utf16.end <= physical_utf16.end
                }
                (None, None) => {
                    row.edit_capability() != M11RecursiveGreenRowEditCapability::Contiguous
                }
                _ => false,
            };
            if (visited_rows == 0
                && (physical.start != next_byte_offset
                    || physical_utf16.start != next_utf16_offset))
                || (visited_rows != 0
                    && (physical.start < next_byte_offset
                        || physical_utf16.start < next_utf16_offset))
                || physical.end > u64::from(command.end_byte_offset)
                || physical_utf16.end > u64::from(command.end_utf16_offset)
                || !editable_valid
            {
                return Err(CandidateEndpointError::InvalidAuthority);
            }
            visited_rows = visited_rows
                .checked_add(1)
                .ok_or(CandidateEndpointError::MetricOverflow)?;
            next_row_ordinal = next_row_ordinal
                .checked_add(1)
                .ok_or(CandidateEndpointError::MetricOverflow)?;
            next_byte_offset = physical.end;
            next_utf16_offset = physical_utf16.end;

            // Paragraph and Heading are the only passive children that carry
            // an inline projection closure. Fenced code is rendered directly
            // from this same row's parser-authenticated editable span.
            if M11RecursiveGreenInlineLeafKind::from_green_kind(row.kind()).is_some()
                && row.edit_capability() == M11RecursiveGreenRowEditCapability::Contiguous
            {
                let editable = editable.ok_or(CandidateEndpointError::InvalidAuthority)?;
                let editable_utf16 =
                    editable_utf16.ok_or(CandidateEndpointError::InvalidAuthority)?;
                if pending.len()
                    == usize::try_from(command.limits.maximum_inline_leaves)
                        .map_err(|_| CandidateEndpointError::MetricOverflow)?
                {
                    return Err(CandidateEndpointError::ViewportInlineLimitExceeded(
                        "inline leaf count",
                    ));
                }
                let inline_bytes = editable
                    .end
                    .checked_sub(editable.start)
                    .ok_or(CandidateEndpointError::InvalidAuthority)?;
                if inline_bytes > u64::from(command.limits.maximum_inline_leaf_source_bytes) {
                    return Err(CandidateEndpointError::ViewportInlineLimitExceeded(
                        "inline leaf source bytes",
                    ));
                }
                total_inline_source_bytes = total_inline_source_bytes
                    .checked_add(inline_bytes)
                    .ok_or(CandidateEndpointError::MetricOverflow)?;
                if total_inline_source_bytes > command.limits.maximum_inline_source_bytes {
                    return Err(CandidateEndpointError::ViewportInlineLimitExceeded(
                        "inline source bytes",
                    ));
                }
                let prepared = green.prepare_inline_leaf(
                    runtime,
                    M11RecursiveGreenPoint::new(
                        usize::try_from(physical.start)
                            .map_err(|_| CandidateEndpointError::MetricOverflow)?,
                        usize::try_from(physical_utf16.start)
                            .map_err(|_| CandidateEndpointError::MetricOverflow)?,
                        SourceBoundaryAffinity::After,
                    ),
                )?;
                if prepared.block_source_range()
                    != (u32::try_from(physical.start)
                        .map_err(|_| CandidateEndpointError::MetricOverflow)?
                        ..u32::try_from(physical.end)
                            .map_err(|_| CandidateEndpointError::MetricOverflow)?)
                    || prepared.block_source_utf16_range()
                        != (u32::try_from(physical_utf16.start)
                            .map_err(|_| CandidateEndpointError::MetricOverflow)?
                            ..u32::try_from(physical_utf16.end)
                                .map_err(|_| CandidateEndpointError::MetricOverflow)?)
                    || prepared.inline_source_range()
                        != (u32::try_from(editable.start)
                            .map_err(|_| CandidateEndpointError::MetricOverflow)?
                            ..u32::try_from(editable.end)
                                .map_err(|_| CandidateEndpointError::MetricOverflow)?)
                    || prepared.inline_source_utf16_range()
                        != (u32::try_from(editable_utf16.start)
                            .map_err(|_| CandidateEndpointError::MetricOverflow)?
                            ..u32::try_from(editable_utf16.end)
                                .map_err(|_| CandidateEndpointError::MetricOverflow)?)
                {
                    return Err(CandidateEndpointError::InvalidAuthority);
                }
                storage_pages_visited = storage_pages_visited
                    .checked_add(prepared.query_receipt().storage_pages_visited())
                    .ok_or(CandidateEndpointError::MetricOverflow)?;
                total_parser_transitions = total_parser_transitions
                    .checked_add(
                        query_work(prepared.query_receipt())
                            .ok_or(CandidateEndpointError::MetricOverflow)?,
                    )
                    .ok_or(CandidateEndpointError::MetricOverflow)?;
                let frame = row.frame();
                let geometry = ViewportInlineLeafGeometry {
                    kind: recursive_green_inline_leaf_sequence_kind(prepared.kind()),
                    entry_ordinal: row.ordinal(),
                    frame,
                    block_source: prepared.block_source_range(),
                    block_source_utf16: prepared.block_source_utf16_range(),
                    inline_source: prepared.inline_source_range(),
                    inline_source_utf16: prepared.inline_source_utf16_range(),
                };
                let fence = prepared.into_fence();
                if fence.frame() != frame {
                    return Err(CandidateEndpointError::InvalidAuthority);
                }
                pending.push_back(ViewportRecursiveGreenInlineLeaf { geometry, fence });
            }
            if next_byte_offset == u64::from(command.end_byte_offset)
                && next_utf16_offset == u64::from(command.end_utf16_offset)
            {
                break;
            }
        }
        if !row_window.complete()
            || next_byte_offset > u64::from(command.end_byte_offset)
            || next_utf16_offset > u64::from(command.end_utf16_offset)
            || visited_rows > u64::from(command.limits.maximum_structural_entries)
            || storage_pages_visited > u64::from(command.limits.maximum_storage_pages)
            || total_parser_transitions > command.limits.maximum_parser_transitions
        {
            return Err(CandidateEndpointError::ViewportInlineLimitExceeded(
                "recursive-Green viewport range budget",
            ));
        }
        // Render rows need not own the blank separator bytes before the next
        // ordinal boundary. The row query's bounded successor proof closes
        // that exact requested cut, so the aggregate range advances across
        // the separator without manufacturing a presentation child.
        next_byte_offset = u64::from(command.end_byte_offset);
        next_utf16_offset = u64::from(command.end_utf16_offset);
        let range_receipt = ViewportGreenRowReceipt {
            visited_rows,
            storage_pages_visited,
            next_row_ordinal,
            next_byte_offset,
            next_utf16_offset,
        };
        let mut ready = Vec::new();
        ready
            .try_reserve_exact(pending.len())
            .map_err(|_| CandidateEndpointError::AllocationFailed)?;
        self.viewport_inline_batch = Some(ViewportInlineBatchState::Running(Box::new(
            RunningViewportInlineBatch {
                command,
                descriptor,
                range_receipt,
                pending,
                reference_resolver: None,
                active: None,
                ready,
                total_inline_source_bytes,
                total_parser_transitions,
                total_fact_records: 0,
                total_ready_roots: 0,
            },
        )));
        self.last_viewport_generation = command.viewport_generation;
        Ok(())
    }

    fn cancel_viewport_inline_batch(&mut self) {
        let Some(state) = self.viewport_inline_batch.take() else {
            return;
        };
        self.viewport_inline_batch = Some(match state {
            ViewportInlineBatchState::Running(running) => {
                ViewportInlineBatchState::Cancelling(Box::new((*running).into_cleanup()))
            }
            ViewportInlineBatchState::Ready(ready) => {
                ViewportInlineBatchState::Cancelling(Box::new((*ready).into_cleanup()))
            }
            ViewportInlineBatchState::Streaming(streaming) => {
                ViewportInlineBatchState::Cancelling(Box::new((*streaming).into_cleanup()))
            }
            ViewportInlineBatchState::Cancelling(mut cleanup) => {
                cleanup.hot_replacement = None;
                ViewportInlineBatchState::Cancelling(cleanup)
            }
        });
    }

    pub(crate) fn cancel_viewport_presentation(&mut self) {
        self.pending_viewport_unavailable = None;
        self.cancel_viewport_inline_batch();
    }

    fn install_viewport_unavailable_cleanup(
        &mut self,
        viewport_generation: u32,
        reason: ViewportPresentationUnavailableReason,
        cleanup: ViewportInlineBatchCleanup,
    ) -> Result<(), CandidateEndpointError> {
        if self.pending_viewport_unavailable.is_some() {
            return Err(CandidateEndpointError::InvalidState);
        }
        self.pending_viewport_unavailable = Some((viewport_generation, reason));
        self.viewport_inline_batch = Some(ViewportInlineBatchState::Cancelling(Box::new(cleanup)));
        Ok(())
    }

    fn install_hot_inline_preempting_viewport(
        &mut self,
        runtime: &DocumentRuntime,
        resolved: ResolvedHotInlineDemand,
    ) -> Result<(), CandidateEndpointError> {
        let Some(state) = self.viewport_inline_batch.take() else {
            self.pending_viewport_unavailable = None;
            return self.install_latest_hot_inline(runtime, resolved);
        };
        if self.hot_inline.is_some() {
            self.viewport_inline_batch = Some(state);
            return Err(CandidateEndpointError::InvalidState);
        }
        self.pending_viewport_unavailable = None;
        let mut cleanup = match state {
            ViewportInlineBatchState::Running(running) => (*running).into_cleanup(),
            ViewportInlineBatchState::Ready(ready) => (*ready).into_cleanup(),
            ViewportInlineBatchState::Streaming(streaming) => (*streaming).into_cleanup(),
            ViewportInlineBatchState::Cancelling(cleanup) => *cleanup,
        };
        cleanup.hot_replacement = Some(Box::new(resolved));
        self.viewport_inline_batch = Some(ViewportInlineBatchState::Cancelling(Box::new(cleanup)));
        Ok(())
    }

    pub(crate) fn request_hot_inline(
        &mut self,
        runtime: &mut DocumentRuntime,
        command: InlineRefinementCommand,
    ) -> Result<(), CandidateEndpointError> {
        if self.closing
            || self.active.is_some()
            || matches!(
                self.viewport_inline_batch.as_ref(),
                Some(ViewportInlineBatchState::Streaming(streaming))
                    if streaming.phase != StreamPhase::NeedBegin
            )
            || self.cleanup.as_ref().is_some_and(|cleanup| {
                !matches!(cleanup, CandidateCleanup::RetainedPublication { .. })
            })
        {
            return Err(CandidateEndpointError::Busy);
        }
        if command.refinement_generation == 0
            || command.refinement_generation <= self.last_hot_inline_generation
            || command.source_version != command.base_ack.source_version
            || command.binding.document_session != command.source_version.document_session
        {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let retained = self
            .retained
            .as_ref()
            .ok_or(CandidateEndpointError::InvalidState)?;
        if retained.ack != command.base_ack {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let descriptor = retained.publication.descriptor(runtime)?;
        let source = runtime
            .current_source_version()
            .ok_or(CandidateEndpointError::InvalidAuthority)?;
        if source.revision().get() != u64::from(command.source_version.revision)
            || source.byte_len()
                != usize::try_from(command.source_version.utf8_length)
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?
            || source.utf16_len()
                != usize::try_from(command.source_version.utf16_length)
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?
            || split_u64(source.root().get()) != command.base_ack.source_root
            || descriptor.source_revision != source.revision().get()
            || descriptor.source_root != source.root().get()
            || descriptor.source_bytes
                != u64::try_from(source.byte_len())
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?
            || descriptor.source_utf16
                != u64::try_from(source.utf16_len())
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?
            || descriptor.syntax_profile != command.base_ack.syntax_profile
            || command.byte_offset > command.source_version.utf8_length
            || command.utf16_offset > command.source_version.utf16_length
        {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let byte_offset = usize::try_from(command.byte_offset)
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let utf16_offset = usize::try_from(command.utf16_offset)
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let affinity = match command.affinity {
            InlinePointAffinity::Before => SourceBoundaryAffinity::Before,
            InlinePointAffinity::After => SourceBoundaryAffinity::After,
        };
        let point = M11BlockSequencePoint::new(byte_offset, utf16_offset, affinity);
        let resolved = match command.target {
            InlineRefinementTarget::RecursiveGreenParagraph => {
                resolved_recursive_green_inline_leaf(
                    self.recursive_green.installed_session(command.base_ack)?,
                    runtime,
                    command,
                    byte_offset,
                    utf16_offset,
                    affinity,
                )?
            }
            InlineRefinementTarget::Automatic
                if self
                    .recursive_green
                    .has_installed_session_for(command.base_ack) =>
            {
                resolved_recursive_green_automatic(
                    self.recursive_green.installed_session(command.base_ack)?,
                    runtime,
                    command,
                    byte_offset,
                    utf16_offset,
                    affinity,
                )?
            }
            InlineRefinementTarget::BulletListItemProjection
            | InlineRefinementTarget::BulletListItemInline
            | InlineRefinementTarget::OrderedListItemProjection
            | InlineRefinementTarget::OrderedListItemInline
                if self
                    .recursive_green
                    .has_installed_session_for(command.base_ack) =>
            {
                resolved_recursive_green_legacy_target(
                    self.recursive_green.installed_session(command.base_ack)?,
                    runtime,
                    command,
                    byte_offset,
                    utf16_offset,
                    affinity,
                )?
            }
            InlineRefinementTarget::Automatic => match resolve_m11_published_inline_leaf_fence(
                runtime,
                &retained.publication,
                point,
            )? {
                M11PublishedInlineLeafFenceResolution::InlineLeaf(fence) => {
                    resolved_inline_leaf(command, fence)
                }
                M11PublishedInlineLeafFenceResolution::NotInlineLeaf {
                    kind,
                    entry_ordinal,
                    source,
                    source_utf16,
                    query_receipt: _,
                } => {
                    if kind == M11BlockSequenceEntryKind::Structured {
                        match resolve_m11_published_indented_code_leaf_fence(
                            runtime,
                            &retained.publication,
                            point,
                        ) {
                            Ok(fence) => {
                                let identity = HotInlineLeafIdentity::indented_code_leaf(&fence);
                                let parser_profile = fence.binding().syntax_profile();
                                ResolvedHotInlineDemand::IndentedCodeLeaf {
                                    command,
                                    identity,
                                    parser_profile,
                                    fence,
                                }
                            }
                            Err(
                                M11CandidateDerivationError::PublishedIndentedCodeLeafFenceNotIndentedCode,
                            ) => match resolve_m11_published_block_quote_leaf_fence(
                                runtime,
                                &retained.publication,
                                point,
                            ) {
                                Ok(fence) => {
                                    let identity = HotInlineLeafIdentity::block_quote_leaf(&fence);
                                    let parser_profile = fence.binding().syntax_profile();
                                    ResolvedHotInlineDemand::BlockQuoteLeaf {
                                        command,
                                        identity,
                                        parser_profile,
                                        fence,
                                    }
                                }
                                Err(
                                    M11CandidateDerivationError::PublishedBlockQuoteLeafFenceNotBlockQuote,
                                ) => match resolve_m11_published_bullet_list_leaf_fence(
                                    runtime,
                                    &retained.publication,
                                    point,
                                ) {
                                    Ok(fence) => {
                                        let identity =
                                            HotInlineLeafIdentity::bullet_list_leaf(&fence);
                                        let parser_profile = fence.binding().syntax_profile();
                                        ResolvedHotInlineDemand::BulletListLeaf {
                                            command,
                                            identity,
                                            parser_profile,
                                            fence,
                                        }
                                    }
                                    Err(
                                        M11CandidateDerivationError::PublishedBulletListLeafFenceNotBulletList,
                                    ) => {
                                        let parser_profile =
                                            flark_engine::ParserProfileId::new(u64::from(
                                                command.base_ack.syntax_profile,
                                            ))
                                            .ok_or(CandidateEndpointError::MetricOverflow)?;
                                        ResolvedHotInlineDemand::Unsupported(Box::new(
                                            HotInlineReady {
                                                command,
                                                identity: HotInlineLeafIdentity {
                                                    kind,
                                                    byte_start: source.start,
                                                    byte_end: source.end,
                                                    utf16_start: source_utf16.start,
                                                    utf16_end: source_utf16.end,
                                                    inline_byte_start: source.start,
                                                    inline_byte_end: source.end,
                                                    inline_utf16_start: source_utf16.start,
                                                    inline_utf16_end: source_utf16.end,
                                                    owner: HotInlineLeafOwner::BlockOrdinal(
                                                        entry_ordinal,
                                                    ),
                                                },
                                                inline_source: source,
                                                inline_source_utf16: source_utf16,
                                                parser_profile,
                                                authority: None,
                                                publication:
                                                    HotInlineReadyPublication::Unsupported(
                                                        HotInlineUnsupported::NotInlineLeaf {
                                                            kind,
                                                        },
                                                    ),
                                            },
                                        ))
                                    }
                                    Err(error) => return Err(error.into()),
                                },
                                Err(error) => return Err(error.into()),
                            },
                            Err(error) => return Err(error.into()),
                        }
                    } else {
                        let parser_profile = flark_engine::ParserProfileId::new(u64::from(
                            command.base_ack.syntax_profile,
                        ))
                        .ok_or(CandidateEndpointError::MetricOverflow)?;
                        ResolvedHotInlineDemand::Unsupported(Box::new(HotInlineReady {
                            command,
                            identity: HotInlineLeafIdentity {
                                kind,
                                byte_start: source.start,
                                byte_end: source.end,
                                utf16_start: source_utf16.start,
                                utf16_end: source_utf16.end,
                                inline_byte_start: source.start,
                                inline_byte_end: source.end,
                                inline_utf16_start: source_utf16.start,
                                inline_utf16_end: source_utf16.end,
                                owner: HotInlineLeafOwner::BlockOrdinal(entry_ordinal),
                            },
                            inline_source: source,
                            inline_source_utf16: source_utf16,
                            parser_profile,
                            authority: None,
                            publication: HotInlineReadyPublication::Unsupported(
                                HotInlineUnsupported::NotInlineLeaf { kind },
                            ),
                        }))
                    }
                }
            },
            InlineRefinementTarget::BulletListItemProjection => {
                match resolve_m11_published_bullet_list_item_fences(
                    runtime,
                    &retained.publication,
                    point,
                )? {
                    M11PublishedBulletListItemInlineFenceOutcome::Inline(fence) => {
                        let (projection_fence, inline_fence) =
                            fence.into_projection_and_inline_fences();
                        drop(inline_fence);
                        resolved_bullet_list_item(command, projection_fence)
                    }
                    M11PublishedBulletListItemInlineFenceOutcome::TerminalEmpty(terminal) => {
                        resolved_bullet_list_item(command, terminal.into_projection_fence())
                    }
                }
            }
            InlineRefinementTarget::BulletListItemInline => {
                match resolve_m11_published_bullet_list_item_inline_fence(
                    runtime,
                    &retained.publication,
                    point,
                )? {
                    M11PublishedBulletListItemInlineFenceOutcome::Inline(fence) => {
                        resolved_inline_leaf(command, fence.into_inline_leaf_fence())
                    }
                    M11PublishedBulletListItemInlineFenceOutcome::TerminalEmpty(terminal) => {
                        let physical = terminal.block_source_range();
                        let physical_utf16 = terminal.block_source_utf16_range();
                        let visible = terminal.item_source_range();
                        let visible_utf16 = terminal.item_source_utf16_range();
                        ResolvedHotInlineDemand::Unsupported(Box::new(HotInlineReady {
                            command,
                            identity: HotInlineLeafIdentity {
                                kind: M11BlockSequenceEntryKind::Structured,
                                byte_start: physical.start,
                                byte_end: physical.end,
                                utf16_start: physical_utf16.start,
                                utf16_end: physical_utf16.end,
                                inline_byte_start: visible.start,
                                inline_byte_end: visible.end,
                                inline_utf16_start: visible_utf16.start,
                                inline_utf16_end: visible_utf16.end,
                                owner: HotInlineLeafOwner::BlockOrdinal(terminal.entry_ordinal()),
                            },
                            inline_source: visible,
                            inline_source_utf16: visible_utf16,
                            parser_profile: terminal.binding().syntax_profile(),
                            authority: None,
                            publication: HotInlineReadyPublication::Unsupported(
                                HotInlineUnsupported::NotInlineLeaf {
                                    kind: M11BlockSequenceEntryKind::Structured,
                                },
                            ),
                        }))
                    }
                }
            }
            InlineRefinementTarget::OrderedListItemProjection => {
                match resolve_m11_published_ordered_list_item_fences(
                    runtime,
                    &retained.publication,
                    point,
                )? {
                    M11PublishedOrderedListItemInlineFenceOutcome::Inline(fence) => {
                        let (projection_fence, inline_fence) =
                            fence.into_projection_and_inline_fences();
                        drop(inline_fence);
                        resolved_ordered_list_item(command, projection_fence)
                    }
                    M11PublishedOrderedListItemInlineFenceOutcome::TerminalEmpty(terminal) => {
                        resolved_ordered_list_item(command, terminal.into_projection_fence())
                    }
                }
            }
            InlineRefinementTarget::OrderedListItemInline => {
                match resolve_m11_published_ordered_list_item_fences(
                    runtime,
                    &retained.publication,
                    point,
                )? {
                    M11PublishedOrderedListItemInlineFenceOutcome::Inline(fence) => {
                        resolved_inline_leaf(command, fence.into_inline_leaf_fence())
                    }
                    M11PublishedOrderedListItemInlineFenceOutcome::TerminalEmpty(terminal) => {
                        let projection = terminal.into_projection_fence();
                        let physical = projection.block_source_range();
                        let physical_utf16 = projection.block_source_utf16_range();
                        let visible = projection.item_source_range();
                        let visible_utf16 = projection.item_source_utf16_range();
                        ResolvedHotInlineDemand::Unsupported(Box::new(HotInlineReady {
                            command,
                            identity: HotInlineLeafIdentity {
                                kind: M11BlockSequenceEntryKind::Structured,
                                byte_start: physical.start,
                                byte_end: physical.end,
                                utf16_start: physical_utf16.start,
                                utf16_end: physical_utf16.end,
                                inline_byte_start: visible.start,
                                inline_byte_end: visible.end,
                                inline_utf16_start: visible_utf16.start,
                                inline_utf16_end: visible_utf16.end,
                                owner: HotInlineLeafOwner::BlockOrdinal(projection.entry_ordinal()),
                            },
                            inline_source: visible,
                            inline_source_utf16: visible_utf16,
                            parser_profile: projection.binding().syntax_profile(),
                            authority: None,
                            publication: HotInlineReadyPublication::Unsupported(
                                HotInlineUnsupported::NotInlineLeaf {
                                    kind: M11BlockSequenceEntryKind::Structured,
                                },
                            ),
                        }))
                    }
                }
            }
        };
        self.cancel_hot_inline_sidecar();
        self.install_hot_inline_preempting_viewport(runtime, resolved)?;
        self.last_hot_inline_generation = command.refinement_generation;
        Ok(())
    }

    fn install_latest_hot_inline(
        &mut self,
        runtime: &DocumentRuntime,
        resolved: ResolvedHotInlineDemand,
    ) -> Result<(), CandidateEndpointError> {
        let Some(current) = self.hot_inline.take() else {
            self.hot_inline = Some(stage_resolved_hot_inline(runtime, resolved)?);
            return Ok(());
        };
        let next_identity = resolved.identity();
        self.hot_inline = Some(match current {
            HotInlineState::AwaitingReferenceResolver(current)
                if current.identity() == next_identity =>
            {
                drop(current);
                HotInlineState::AwaitingReferenceResolver(Box::new(resolved))
            }
            HotInlineState::AwaitingReferenceResolver(current) => {
                drop(current);
                stage_resolved_hot_inline(runtime, resolved)?
            }
            HotInlineState::Running(mut running) if running.identity == next_identity => {
                running.command = resolved.command();
                drop(resolved);
                HotInlineState::Running(running)
            }
            HotInlineState::Ready(mut ready) if ready.identity == next_identity => {
                ready.command = resolved.command();
                drop(resolved);
                HotInlineState::Ready(ready)
            }
            HotInlineState::Running(running) => HotInlineState::Cancelling {
                job: running.job,
                begun: false,
                replacement: Some(Box::new(resolved)),
            },
            HotInlineState::Ready(ready) => match (*ready).into_parts() {
                (
                    _,
                    _,
                    _,
                    _,
                    _,
                    _,
                    _,
                    authority,
                    HotInlineReadyPublication::Authoritative(root),
                ) => HotInlineState::Releasing {
                    root,
                    authority,
                    begun: false,
                    replacement: Some(Box::new(resolved)),
                },
                (
                    _,
                    _,
                    _,
                    _,
                    _,
                    _,
                    _,
                    authority,
                    HotInlineReadyPublication::Unsupported(unsupported),
                ) => {
                    drop(unsupported);
                    drop(authority);
                    stage_resolved_hot_inline(runtime, resolved)?
                }
            },
            HotInlineState::Cancelling {
                job,
                begun,
                replacement: _,
            } => HotInlineState::Cancelling {
                job,
                begun,
                replacement: Some(Box::new(resolved)),
            },
            HotInlineState::Releasing {
                root,
                authority,
                begun,
                replacement: _,
            } => HotInlineState::Releasing {
                root,
                authority,
                begun,
                replacement: Some(Box::new(resolved)),
            },
        });
        Ok(())
    }

    pub(crate) fn cancel_hot_inline(&mut self) {
        self.pending_viewport_unavailable = None;
        self.cancel_viewport_inline_batch();
        self.cancel_hot_inline_sidecar();
        let Some(current) = self.hot_inline.take() else {
            return;
        };
        self.hot_inline = match current {
            HotInlineState::AwaitingReferenceResolver(resolved) => {
                drop(resolved);
                None
            }
            HotInlineState::Running(running) => Some(HotInlineState::Cancelling {
                job: running.job,
                begun: false,
                replacement: None,
            }),
            HotInlineState::Ready(ready) => match (*ready).into_parts() {
                (
                    _,
                    _,
                    _,
                    _,
                    _,
                    _,
                    _,
                    authority,
                    HotInlineReadyPublication::Authoritative(root),
                ) => Some(HotInlineState::Releasing {
                    root,
                    authority,
                    begun: false,
                    replacement: None,
                }),
                (
                    _,
                    _,
                    _,
                    _,
                    _,
                    _,
                    _,
                    authority,
                    HotInlineReadyPublication::Unsupported(unsupported),
                ) => {
                    drop(unsupported);
                    drop(authority);
                    None
                }
            },
            HotInlineState::Cancelling {
                job,
                begun,
                replacement: _,
            } => Some(HotInlineState::Cancelling {
                job,
                begun,
                replacement: None,
            }),
            HotInlineState::Releasing {
                root,
                authority,
                begun,
                replacement: _,
            } => Some(HotInlineState::Releasing {
                root,
                authority,
                begun,
                replacement: None,
            }),
        };
    }

    fn take_hot_inline_ready(&mut self) -> Option<HotInlineReady> {
        let HotInlineState::Ready(_) = self.hot_inline.as_ref()? else {
            return None;
        };
        let Some(HotInlineState::Ready(ready)) = self.hot_inline.take() else {
            unreachable!("ready hot-inline state was checked")
        };
        Some(*ready)
    }


    fn schedule_failed_hot_inline_publication(
        &mut self,
        publication: HotInlineReadyPublication,
        authority: Option<M11ParserSourceRangeAuthority>,
    ) {
        match publication {
            HotInlineReadyPublication::Authoritative(root) => {
                self.schedule_hot_inline_root_release(Some(root), authority);
            }
            HotInlineReadyPublication::Unsupported(_) => drop(authority),
        }
    }

    fn schedule_hot_inline_root_release(
        &mut self,
        root: Option<Box<HotInlineProjectionRoot>>,
        authority: Option<M11ParserSourceRangeAuthority>,
    ) {
        if let Some(root) = root {
            debug_assert!(self.hot_inline.is_none());
            self.hot_inline = Some(HotInlineState::Releasing {
                root,
                authority,
                begun: false,
                replacement: None,
            });
        } else {
            drop(authority);
        }
    }

    fn cancel_hot_inline_sidecar(&mut self) {
        let Some(mut sidecar) = self.hot_inline_sidecar.take() else {
            return;
        };
        self.schedule_hot_inline_root_release(sidecar.root.take(), sidecar.authority.take());
    }

    fn poll_hot_inline(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<usize, CandidateEndpointError> {
        let mut transitions = 0;
        while transitions < fuel {
            let Some(state) = self.hot_inline.take() else {
                return Ok(transitions);
            };
            match state {
                HotInlineState::AwaitingReferenceResolver(resolved) => {
                    let retained = match self.retained.as_mut() {
                        Some(retained) => retained,
                        None => {
                            self.hot_inline =
                                Some(HotInlineState::AwaitingReferenceResolver(resolved));
                            return Err(CandidateEndpointError::InvalidAuthority);
                        }
                    };
                    let polled = match retained
                        .publication
                        .poll_reference_resolver(runtime, fuel - transitions)
                    {
                        Ok(polled) => polled,
                        Err(error) => {
                            self.hot_inline =
                                Some(HotInlineState::AwaitingReferenceResolver(resolved));
                            return Err(error.into());
                        }
                    };
                    transitions = checked_add(transitions, polled.transitions())?;
                    if transitions > fuel {
                        self.hot_inline = Some(HotInlineState::AwaitingReferenceResolver(resolved));
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    if !polled.ready() || transitions == fuel {
                        self.hot_inline = Some(HotInlineState::AwaitingReferenceResolver(resolved));
                        return Ok(transitions);
                    }
                    let reference_resolver = match retained.publication.reference_resolver(runtime)
                    {
                        Ok(Some(reference_resolver)) => reference_resolver,
                        Ok(None) => {
                            self.hot_inline =
                                Some(HotInlineState::AwaitingReferenceResolver(resolved));
                            return Err(CandidateEndpointError::InvalidState);
                        }
                        Err(error) => {
                            self.hot_inline =
                                Some(HotInlineState::AwaitingReferenceResolver(resolved));
                            return Err(error.into());
                        }
                    };
                    let running = match start_resolved_hot_inline(
                        runtime,
                        *resolved,
                        Some(reference_resolver),
                    ) {
                        Ok(running) => running,
                        Err(error) => return Err(error),
                    };
                    transitions = checked_add(transitions, 1)?;
                    self.hot_inline = Some(running);
                    if transitions == fuel {
                        return Ok(transitions);
                    }
                }
                HotInlineState::Running(mut running) => {
                    let poll_fuel =
                        (fuel - transitions).min(running.job.maximum_poll_transitions());
                    let (consumed, parser_profile, authority, publication) = match &mut running.job
                    {
                        RunningHotInlineJob::Inline(job) => {
                            let polled = match job.poll(runtime, poll_fuel) {
                                Ok(polled) => polled,
                                Err(error) => {
                                    self.hot_inline = Some(HotInlineState::Running(running));
                                    return Err(error.into());
                                }
                            };
                            if polled.status() == M11InlineProjectionJobPollStatus::Pending {
                                transitions = checked_add(transitions, polled.transitions())?;
                                if transitions > fuel {
                                    self.hot_inline = Some(HotInlineState::Running(running));
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                                self.hot_inline = Some(HotInlineState::Running(running));
                                return Ok(transitions);
                            }
                            let output = job
                                .take_output()
                                .ok_or(CandidateEndpointError::InvalidState)?;
                            let (source, source_range, parser_profile, authority, publication) =
                                output.into_publication_parts().into_parts();
                            if source.revision().get()
                                != u64::from(running.command.source_version.revision)
                                || split_u64(source.root().get())
                                    != running.command.base_ack.source_root
                                || source_range != running.inline_source
                                || parser_profile != running.parser_profile
                                || parser_profile.get()
                                    != u64::from(running.command.base_ack.syntax_profile)
                            {
                                let cleanup = match publication {
                                    M11InlineProjectionPublication::Authoritative(root) => {
                                        HotInlineState::Releasing {
                                            root: Box::new(HotInlineProjectionRoot::Inline(root)),
                                            authority: Some(authority),
                                            begun: false,
                                            replacement: None,
                                        }
                                    }
                                    M11InlineProjectionPublication::Unsupported(record) => {
                                        drop(record);
                                        drop(authority);
                                        return Err(CandidateEndpointError::InvalidAuthority);
                                    }
                                };
                                self.hot_inline = Some(cleanup);
                                return Err(CandidateEndpointError::InvalidAuthority);
                            }
                            (
                                polled.transitions(),
                                parser_profile,
                                Some(authority),
                                match publication {
                                    M11InlineProjectionPublication::Authoritative(root) => {
                                        HotInlineReadyPublication::Authoritative(Box::new(
                                            HotInlineProjectionRoot::Inline(root),
                                        ))
                                    }
                                    M11InlineProjectionPublication::Unsupported(record) => {
                                        HotInlineReadyPublication::Unsupported(
                                            HotInlineUnsupported::Parser(record),
                                        )
                                    }
                                },
                            )
                        }
                        RunningHotInlineJob::IndentedCode(job) => {
                            let polled = match job.poll(runtime, poll_fuel) {
                                Ok(polled) => polled,
                                Err(error) => {
                                    self.hot_inline = Some(HotInlineState::Running(running));
                                    return Err(error.into());
                                }
                            };
                            if polled.status() == M11IndentedCodeProjectionJobPollStatus::Pending {
                                transitions = checked_add(transitions, polled.transitions())?;
                                if transitions > fuel {
                                    self.hot_inline = Some(HotInlineState::Running(running));
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                                self.hot_inline = Some(HotInlineState::Running(running));
                                return Ok(transitions);
                            }
                            let root = job
                                .take_root()
                                .ok_or(CandidateEndpointError::InvalidState)?;
                            let descriptor = root.descriptor();
                            let source = descriptor.source();
                            if source.revision().get()
                                != u64::from(running.command.source_version.revision)
                                || split_u64(source.root().get())
                                    != running.command.base_ack.source_root
                                || descriptor.physical_block_range() != &running.inline_source
                                || descriptor.requested_window() != &running.inline_source
                                || descriptor.parser_profile() != running.parser_profile
                                || running.parser_profile.get()
                                    != u64::from(running.command.base_ack.syntax_profile)
                            {
                                self.hot_inline = Some(HotInlineState::Releasing {
                                    root: Box::new(HotInlineProjectionRoot::IndentedCode(root)),
                                    authority: None,
                                    begun: false,
                                    replacement: None,
                                });
                                return Err(CandidateEndpointError::InvalidAuthority);
                            }
                            (
                                polled.transitions(),
                                running.parser_profile,
                                None,
                                HotInlineReadyPublication::Authoritative(Box::new(
                                    HotInlineProjectionRoot::IndentedCode(root),
                                )),
                            )
                        }
                        RunningHotInlineJob::BlockQuote(job) => {
                            let polled = match job.poll(runtime, poll_fuel) {
                                Ok(polled) => polled,
                                Err(error) => {
                                    self.hot_inline = Some(HotInlineState::Running(running));
                                    return Err(error.into());
                                }
                            };
                            if polled.status() == M11BlockQuoteProjectionJobPollStatus::Pending {
                                transitions = checked_add(transitions, polled.transitions())?;
                                if transitions > fuel {
                                    self.hot_inline = Some(HotInlineState::Running(running));
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                                self.hot_inline = Some(HotInlineState::Running(running));
                                return Ok(transitions);
                            }
                            let root = job
                                .take_root()
                                .ok_or(CandidateEndpointError::InvalidState)?;
                            let descriptor = root.descriptor();
                            let source = descriptor.source();
                            if source.revision().get()
                                != u64::from(running.command.source_version.revision)
                                || split_u64(source.root().get())
                                    != running.command.base_ack.source_root
                                || descriptor.physical_block_range() != &running.inline_source
                                || descriptor.requested_window() != &running.inline_source
                                || descriptor.parser_profile() != running.parser_profile
                                || running.parser_profile.get()
                                    != u64::from(running.command.base_ack.syntax_profile)
                            {
                                self.hot_inline = Some(HotInlineState::Releasing {
                                    root: Box::new(HotInlineProjectionRoot::BlockQuote(root)),
                                    authority: None,
                                    begun: false,
                                    replacement: None,
                                });
                                return Err(CandidateEndpointError::InvalidAuthority);
                            }
                            (
                                polled.transitions(),
                                running.parser_profile,
                                None,
                                HotInlineReadyPublication::Authoritative(Box::new(
                                    HotInlineProjectionRoot::BlockQuote(root),
                                )),
                            )
                        }
                        RunningHotInlineJob::BulletList(job) => {
                            let polled = match job.poll(runtime, poll_fuel) {
                                Ok(polled) => polled,
                                Err(error) => {
                                    self.hot_inline = Some(HotInlineState::Running(running));
                                    return Err(error.into());
                                }
                            };
                            if polled.status() == M11BlockQuoteProjectionJobPollStatus::Pending {
                                transitions = checked_add(transitions, polled.transitions())?;
                                if transitions > fuel {
                                    self.hot_inline = Some(HotInlineState::Running(running));
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                                self.hot_inline = Some(HotInlineState::Running(running));
                                return Ok(transitions);
                            }
                            let root = job
                                .take_root()
                                .ok_or(CandidateEndpointError::InvalidState)?;
                            let descriptor = root.descriptor();
                            let source = descriptor.source();
                            if source.revision().get()
                                != u64::from(running.command.source_version.revision)
                                || split_u64(source.root().get())
                                    != running.command.base_ack.source_root
                                || descriptor.physical_block_range() != &running.inline_source
                                || descriptor.requested_window() != &running.inline_source
                                || descriptor.parser_profile() != running.parser_profile
                                || running.parser_profile.get()
                                    != u64::from(running.command.base_ack.syntax_profile)
                            {
                                self.hot_inline = Some(HotInlineState::Releasing {
                                    root: Box::new(HotInlineProjectionRoot::BulletList(root)),
                                    authority: None,
                                    begun: false,
                                    replacement: None,
                                });
                                return Err(CandidateEndpointError::InvalidAuthority);
                            }
                            (
                                polled.transitions(),
                                running.parser_profile,
                                None,
                                HotInlineReadyPublication::Authoritative(Box::new(
                                    HotInlineProjectionRoot::BulletList(root),
                                )),
                            )
                        }
                        RunningHotInlineJob::BulletListItem(job) => {
                            let polled = match job.poll(runtime, poll_fuel) {
                                Ok(polled) => polled,
                                Err(error) => {
                                    self.hot_inline = Some(HotInlineState::Running(running));
                                    return Err(error.into());
                                }
                            };
                            if polled.status() == M11BulletListItemProjectionJobPollStatus::Pending
                            {
                                transitions = checked_add(transitions, polled.transitions())?;
                                if transitions > fuel {
                                    self.hot_inline = Some(HotInlineState::Running(running));
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                                self.hot_inline = Some(HotInlineState::Running(running));
                                return Ok(transitions);
                            }
                            let output = job
                                .take_output()
                                .ok_or(CandidateEndpointError::InvalidState)?;
                            let (
                                root,
                                selected_item_ordinal,
                                canonical_line_ending,
                                _terminal_empty,
                                ordered_item,
                            ) = output.into_parts_with_metadata();
                            let has_ordered_item = ordered_item.is_some();
                            let canonical_line_ending = match canonical_line_ending {
                                flark_parser::M11LineEnding::Lf => {
                                    M11HotInlineCanonicalLineEnding::Lf
                                }
                                flark_parser::M11LineEnding::CrLf => {
                                    M11HotInlineCanonicalLineEnding::CrLf
                                }
                                flark_parser::M11LineEnding::Cr => {
                                    M11HotInlineCanonicalLineEnding::Cr
                                }
                                flark_parser::M11LineEnding::Eof => {
                                    self.hot_inline = Some(HotInlineState::Releasing {
                                        root: Box::new(hot_inline_list_item_root(
                                            root,
                                            selected_item_ordinal,
                                            M11HotInlineCanonicalLineEnding::Lf,
                                            ordered_item,
                                        )),
                                        authority: None,
                                        begun: false,
                                        replacement: None,
                                    });
                                    return Err(CandidateEndpointError::InvalidAuthority);
                                }
                            };
                            let descriptor = root.descriptor();
                            let source = descriptor.source();
                            if source.revision().get()
                                != u64::from(running.command.source_version.revision)
                                || split_u64(source.root().get())
                                    != running.command.base_ack.source_root
                                || descriptor.physical_block_range()
                                    != &running.identity.source_range()
                                || descriptor.requested_window() != &running.inline_source
                                || descriptor.parser_profile() != running.parser_profile
                                || descriptor.line_count() != 1
                                || !list_item_projection_matches_target(
                                    running.command.target,
                                    descriptor.projection_kind(),
                                    has_ordered_item,
                                )
                                || running.parser_profile.get()
                                    != u64::from(running.command.base_ack.syntax_profile)
                            {
                                self.hot_inline = Some(HotInlineState::Releasing {
                                    root: Box::new(hot_inline_list_item_root(
                                        root,
                                        selected_item_ordinal,
                                        canonical_line_ending,
                                        ordered_item,
                                    )),
                                    authority: None,
                                    begun: false,
                                    replacement: None,
                                });
                                return Err(CandidateEndpointError::InvalidAuthority);
                            }
                            (
                                polled.transitions(),
                                running.parser_profile,
                                None,
                                HotInlineReadyPublication::Authoritative(Box::new(
                                    hot_inline_list_item_root(
                                        root,
                                        selected_item_ordinal,
                                        canonical_line_ending,
                                        ordered_item,
                                    ),
                                )),
                            )
                        }
                    };
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel {
                        self.hot_inline = Some(HotInlineState::Running(running));
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    drop(running.job);
                    self.hot_inline = Some(HotInlineState::Ready(Box::new(HotInlineReady {
                        command: running.command,
                        identity: running.identity,
                        inline_source: running.inline_source,
                        inline_source_utf16: running.inline_source_utf16,
                        parser_profile,
                        authority,
                        publication,
                    })));
                    return Ok(transitions);
                }
                HotInlineState::Ready(ready) => {
                    self.hot_inline = Some(HotInlineState::Ready(ready));
                    return Ok(transitions);
                }
                HotInlineState::Cancelling {
                    mut job,
                    mut begun,
                    replacement,
                } => {
                    if !begun {
                        if let Err(error) = job.begin_cancel(runtime) {
                            self.hot_inline = Some(HotInlineState::Cancelling {
                                job,
                                begun,
                                replacement,
                            });
                            return Err(error);
                        }
                        begun = true;
                        transitions = checked_add(transitions, 1)?;
                        if transitions == fuel {
                            self.hot_inline = Some(HotInlineState::Cancelling {
                                job,
                                begun,
                                replacement,
                            });
                            return Ok(transitions);
                        }
                    }
                    let poll_fuel = (fuel - transitions).min(job.maximum_poll_transitions());
                    let (consumed, complete) = match job.poll_cancel(runtime, poll_fuel) {
                        Ok(polled) => polled,
                        Err(error) => {
                            self.hot_inline = Some(HotInlineState::Cancelling {
                                job,
                                begun,
                                replacement,
                            });
                            return Err(error);
                        }
                    };
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel {
                        self.hot_inline = Some(HotInlineState::Cancelling {
                            job,
                            begun,
                            replacement,
                        });
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    if !complete {
                        self.hot_inline = Some(HotInlineState::Cancelling {
                            job,
                            begun,
                            replacement,
                        });
                        return Ok(transitions);
                    }
                    drop(job);
                    self.hot_inline = match replacement {
                        Some(replacement) => {
                            Some(stage_resolved_hot_inline(runtime, *replacement)?)
                        }
                        None => None,
                    };
                }
                HotInlineState::Releasing {
                    mut root,
                    authority,
                    mut begun,
                    replacement,
                } => {
                    if !begun {
                        if let Err(error) = root.begin_release(runtime) {
                            self.hot_inline = Some(HotInlineState::Releasing {
                                root,
                                authority,
                                begun,
                                replacement,
                            });
                            return Err(error);
                        }
                        begun = true;
                        transitions = checked_add(transitions, 1)?;
                        if transitions == fuel {
                            self.hot_inline = Some(HotInlineState::Releasing {
                                root,
                                authority,
                                begun,
                                replacement,
                            });
                            return Ok(transitions);
                        }
                    }
                    let (consumed, complete) = match root.poll_release(runtime, fuel - transitions)
                    {
                        Ok(polled) => polled,
                        Err(error) => {
                            self.hot_inline = Some(HotInlineState::Releasing {
                                root,
                                authority,
                                begun,
                                replacement,
                            });
                            return Err(error);
                        }
                    };
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel {
                        self.hot_inline = Some(HotInlineState::Releasing {
                            root,
                            authority,
                            begun,
                            replacement,
                        });
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    if !complete {
                        self.hot_inline = Some(HotInlineState::Releasing {
                            root,
                            authority,
                            begun,
                            replacement,
                        });
                        return Ok(transitions);
                    }
                    drop(root);
                    drop(authority);
                    self.hot_inline = match replacement {
                        Some(replacement) => {
                            Some(stage_resolved_hot_inline(runtime, *replacement)?)
                        }
                        None => None,
                    };
                }
            }
        }
        Ok(transitions)
    }

    fn poll_viewport_inline_batch(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<usize, CandidateEndpointError> {
        if fuel == 0 {
            return Err(CandidateEndpointError::InvalidState);
        }
        let mut transitions = 0_usize;
        while transitions < fuel {
            let Some(state) = self.viewport_inline_batch.take() else {
                return Ok(transitions);
            };
            match state {
                ViewportInlineBatchState::Running(mut running) => {
                    if running.active.is_none() && running.pending.is_empty() {
                        self.viewport_inline_batch = Some(ViewportInlineBatchState::Ready(
                            Box::new((*running).into_ready()),
                        ));
                        return Ok(transitions);
                    }
                    if running.active.is_none() && running.reference_resolver.is_none() {
                        let command_remaining = running
                            .command
                            .limits
                            .maximum_parser_transitions
                            .checked_sub(running.total_parser_transitions)
                            .ok_or(CandidateEndpointError::InvalidState)?;
                        if command_remaining == 0 {
                            let viewport_generation = running.command.viewport_generation;
                            self.install_viewport_unavailable_cleanup(
                                viewport_generation,
                                ViewportPresentationUnavailableReason::BudgetExceeded,
                                (*running).into_cleanup(),
                            )?;
                            transitions = checked_add(transitions, 1)?;
                            return Ok(transitions);
                        }
                        let resolver_poll_fuel = (fuel - transitions)
                            .min(usize::try_from(command_remaining).unwrap_or(usize::MAX));
                        let resolver_poll = {
                            let retained = self
                                .retained
                                .as_mut()
                                .ok_or(CandidateEndpointError::InvalidState)?;
                            retained
                                .publication
                                .poll_reference_resolver(runtime, resolver_poll_fuel)?
                        };
                        transitions = checked_add(transitions, resolver_poll.transitions())?;
                        running.total_parser_transitions = running
                            .total_parser_transitions
                            .checked_add(
                                u64::try_from(resolver_poll.transitions())
                                    .map_err(|_| CandidateEndpointError::MetricOverflow)?,
                            )
                            .ok_or(CandidateEndpointError::MetricOverflow)?;
                        if transitions > fuel
                            || running.total_parser_transitions
                                > running.command.limits.maximum_parser_transitions
                        {
                            self.viewport_inline_batch =
                                Some(ViewportInlineBatchState::Cancelling(Box::new(
                                    (*running).into_cleanup(),
                                )));
                            return Err(CandidateEndpointError::InvalidState);
                        }
                        if !resolver_poll.ready() {
                            self.viewport_inline_batch =
                                Some(ViewportInlineBatchState::Running(running));
                            return Ok(transitions);
                        }
                        running.reference_resolver = Some(
                            self.retained
                                .as_ref()
                                .ok_or(CandidateEndpointError::InvalidState)?
                                .publication
                                .reference_resolver(runtime)?
                                .ok_or(CandidateEndpointError::InvalidState)?,
                        );
                        if transitions == fuel {
                            self.viewport_inline_batch =
                                Some(ViewportInlineBatchState::Running(running));
                            return Ok(transitions);
                        }
                    }
                    if running.active.is_none() {
                        if running.total_parser_transitions
                            == running.command.limits.maximum_parser_transitions
                        {
                            let viewport_generation = running.command.viewport_generation;
                            self.install_viewport_unavailable_cleanup(
                                viewport_generation,
                                ViewportPresentationUnavailableReason::BudgetExceeded,
                                (*running).into_cleanup(),
                            )?;
                            transitions = checked_add(transitions, 1)?;
                            return Ok(transitions);
                        }
                        let pending = running
                            .pending
                            .pop_front()
                            .ok_or(CandidateEndpointError::InvalidState)?;
                        let reference_resolver = running
                            .reference_resolver
                            .as_ref()
                            .ok_or(CandidateEndpointError::InvalidState)?
                            .clone();
                        let job = match M11InlineProjectionJob::new_for_recursive_green_inline_leaf_with_reference_resolver(
                            runtime,
                            pending.fence,
                            M11ParserBinding::current(
                                flark_engine::ParserProfileId::new(u64::from(
                                    running.command.base_ack.syntax_profile,
                                ))
                                .ok_or(CandidateEndpointError::MetricOverflow)?,
                            ),
                            reference_resolver,
                        ) {
                            Ok(job) => job,
                            Err(_error) => {
                                let viewport_generation = running.command.viewport_generation;
                                self.install_viewport_unavailable_cleanup(
                                    viewport_generation,
                                    ViewportPresentationUnavailableReason::DerivationFailed,
                                    (*running).into_cleanup(),
                                )?;
                                transitions = checked_add(transitions, 1)?;
                                return Ok(transitions);
                            }
                        };
                        running.active = Some(ViewportInlineActiveJob {
                            geometry: pending.geometry,
                            job: Box::new(job),
                        });
                    }

                    let command_remaining = running
                        .command
                        .limits
                        .maximum_parser_transitions
                        .checked_sub(running.total_parser_transitions)
                        .ok_or(CandidateEndpointError::InvalidState)?;
                    if command_remaining == 0 {
                        let viewport_generation = running.command.viewport_generation;
                        self.install_viewport_unavailable_cleanup(
                            viewport_generation,
                            ViewportPresentationUnavailableReason::BudgetExceeded,
                            (*running).into_cleanup(),
                        )?;
                        transitions = checked_add(transitions, 1)?;
                        return Ok(transitions);
                    }
                    let command_poll_fuel =
                        usize::try_from(command_remaining).unwrap_or(usize::MAX);
                    let poll_fuel = (fuel - transitions)
                        .min(M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS)
                        .min(command_poll_fuel);
                    let polled = match running
                        .active
                        .as_mut()
                        .ok_or(CandidateEndpointError::InvalidState)?
                        .job
                        .poll(runtime, poll_fuel)
                    {
                        Ok(polled) => polled,
                        Err(_error) => {
                            let viewport_generation = running.command.viewport_generation;
                            self.install_viewport_unavailable_cleanup(
                                viewport_generation,
                                ViewportPresentationUnavailableReason::DerivationFailed,
                                (*running).into_cleanup(),
                            )?;
                            transitions = checked_add(transitions, 1)?;
                            return Ok(transitions);
                        }
                    };
                    transitions = checked_add(transitions, polled.transitions())?;
                    running.total_parser_transitions = running
                        .total_parser_transitions
                        .checked_add(
                            u64::try_from(polled.transitions())
                                .map_err(|_| CandidateEndpointError::MetricOverflow)?,
                        )
                        .ok_or(CandidateEndpointError::MetricOverflow)?;
                    if transitions > fuel
                        || running.total_parser_transitions
                            > running.command.limits.maximum_parser_transitions
                    {
                        self.viewport_inline_batch = Some(ViewportInlineBatchState::Cancelling(
                            Box::new((*running).into_cleanup()),
                        ));
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    if polled.status() == M11InlineProjectionJobPollStatus::Pending {
                        self.viewport_inline_batch =
                            Some(ViewportInlineBatchState::Running(running));
                        return Ok(transitions);
                    }

                    let mut active = running
                        .active
                        .take()
                        .ok_or(CandidateEndpointError::InvalidState)?;
                    let output = active
                        .job
                        .take_output()
                        .ok_or(CandidateEndpointError::InvalidState)?;
                    let (source, source_range, parser_profile, authority, publication) =
                        output.into_publication_parts().into_parts();
                    let authority_valid = source.revision().get()
                        == u64::from(running.command.source_version.revision)
                        && split_u64(source.root().get()) == running.command.base_ack.source_root
                        && source_range == active.geometry.inline_source
                        && parser_profile.get()
                            == u64::from(running.command.base_ack.syntax_profile);
                    let (fact_records, ready_roots, publication_valid) = match &publication {
                        M11InlineProjectionPublication::Authoritative(root) => {
                            let root_descriptor = root.descriptor();
                            (
                                root_descriptor.fact_count(),
                                1_u32,
                                root_descriptor.source() == source
                                    && root_descriptor.parser_profile() == parser_profile
                                    && root_descriptor.source_range()
                                        == &active.geometry.inline_source,
                            )
                        }
                        M11InlineProjectionPublication::Unsupported(record) => (
                            0,
                            0,
                            record.source() == source
                                && record.parser_profile() == parser_profile
                                && record.source_range() == active.geometry.inline_source,
                        ),
                    };
                    let ready = ViewportInlineLeafReady {
                        geometry: active.geometry,
                        parser_profile,
                        authority: Some(authority),
                        publication: match publication {
                            M11InlineProjectionPublication::Authoritative(root) => {
                                ViewportInlineLeafPublication::Authoritative(root)
                            }
                            M11InlineProjectionPublication::Unsupported(record) => {
                                ViewportInlineLeafPublication::Unsupported(record)
                            }
                        },
                    };
                    drop(active.job);
                    running.ready.push(ready);
                    if !authority_valid || !publication_valid {
                        self.viewport_inline_batch = Some(ViewportInlineBatchState::Cancelling(
                            Box::new((*running).into_cleanup()),
                        ));
                        return Err(CandidateEndpointError::InvalidAuthority);
                    }

                    running.total_fact_records = running
                        .total_fact_records
                        .checked_add(fact_records)
                        .ok_or(CandidateEndpointError::MetricOverflow)?;
                    running.total_ready_roots = running
                        .total_ready_roots
                        .checked_add(ready_roots)
                        .ok_or(CandidateEndpointError::MetricOverflow)?;
                    let limit_failure = if running.total_fact_records
                        > running.command.limits.maximum_fact_records
                    {
                        Some("total fact records")
                    } else if running.total_ready_roots
                        > running.command.limits.maximum_inline_leaves
                    {
                        Some("total ready roots")
                    } else {
                        None
                    };
                    if let Some(_limit) = limit_failure {
                        let viewport_generation = running.command.viewport_generation;
                        self.install_viewport_unavailable_cleanup(
                            viewport_generation,
                            ViewportPresentationUnavailableReason::BudgetExceeded,
                            (*running).into_cleanup(),
                        )?;
                        if transitions == 0 {
                            transitions = 1;
                        }
                        return Ok(transitions);
                    }
                    if running.pending.is_empty() {
                        self.viewport_inline_batch = Some(ViewportInlineBatchState::Ready(
                            Box::new((*running).into_ready()),
                        ));
                        return Ok(transitions);
                    }
                    self.viewport_inline_batch = Some(ViewportInlineBatchState::Running(running));
                }
                ViewportInlineBatchState::Ready(ready) => {
                    self.viewport_inline_batch = Some(ViewportInlineBatchState::Ready(ready));
                    return Ok(transitions);
                }
                ViewportInlineBatchState::Streaming(streaming) => {
                    self.viewport_inline_batch =
                        Some(ViewportInlineBatchState::Streaming(streaming));
                    return Ok(transitions);
                }
                ViewportInlineBatchState::Cancelling(mut cleanup) => {
                    if let Some(job) = cleanup.active_job.as_mut() {
                        if !cleanup.active_abort_begun {
                            if let Err(error) = job.begin_abort(runtime) {
                                self.viewport_inline_batch =
                                    Some(ViewportInlineBatchState::Cancelling(cleanup));
                                return Err(error.into());
                            }
                            cleanup.active_abort_begun = true;
                            transitions = checked_add(transitions, 1)?;
                            if transitions == fuel {
                                self.viewport_inline_batch =
                                    Some(ViewportInlineBatchState::Cancelling(cleanup));
                                return Ok(transitions);
                            }
                        }
                        let poll_fuel = (fuel - transitions)
                            .min(M11_INLINE_PROJECTION_JOB_MAX_POLL_TRANSITIONS);
                        let polled = match job.poll_abort(runtime, poll_fuel) {
                            Ok(polled) => polled,
                            Err(error) => {
                                self.viewport_inline_batch =
                                    Some(ViewportInlineBatchState::Cancelling(cleanup));
                                return Err(error.into());
                            }
                        };
                        transitions = checked_add(transitions, polled.transitions())?;
                        if !polled.complete() {
                            self.viewport_inline_batch =
                                Some(ViewportInlineBatchState::Cancelling(cleanup));
                            return Ok(transitions);
                        }
                        drop(cleanup.active_job.take());
                        cleanup.active_abort_begun = false;
                    }

                    if let Some(releasing) = cleanup.releasing.as_mut() {
                        if !releasing.begun {
                            if let Err(error) = releasing.root.begin_release(runtime) {
                                self.viewport_inline_batch =
                                    Some(ViewportInlineBatchState::Cancelling(cleanup));
                                return Err(error.into());
                            }
                            releasing.begun = true;
                            transitions = checked_add(transitions, 1)?;
                            if transitions == fuel {
                                self.viewport_inline_batch =
                                    Some(ViewportInlineBatchState::Cancelling(cleanup));
                                return Ok(transitions);
                            }
                        }
                        let polled = match releasing.root.poll_release(runtime, fuel - transitions)
                        {
                            Ok(polled) => polled,
                            Err(error) => {
                                self.viewport_inline_batch =
                                    Some(ViewportInlineBatchState::Cancelling(cleanup));
                                return Err(error.into());
                            }
                        };
                        transitions = checked_add(transitions, polled.receipt().transitions)?;
                        if !polled.complete() {
                            self.viewport_inline_batch =
                                Some(ViewportInlineBatchState::Cancelling(cleanup));
                            return Ok(transitions);
                        }
                        let released = cleanup
                            .releasing
                            .take()
                            .ok_or(CandidateEndpointError::InvalidState)?;
                        drop(released.root);
                        drop(released.authority);
                    }

                    if let Some(mut active) = cleanup.active_child.take() {
                        drop(active.encoder);
                        if let Some(root) = active.root.take() {
                            cleanup.releasing = Some(ReleasingViewportInlineRoot {
                                root,
                                authority: active.authority.take(),
                                begun: false,
                            });
                            self.viewport_inline_batch =
                                Some(ViewportInlineBatchState::Cancelling(cleanup));
                            continue;
                        }
                        drop(active.authority.take());
                        transitions = checked_add(transitions, 1)?;
                    }

                    while transitions < fuel {
                        let Some(mut prepared) = cleanup.prepared.pop() else {
                            break;
                        };
                        match prepared.publication {
                            ViewportPreparedChildPublication::Authoritative(root) => {
                                cleanup.releasing = Some(ReleasingViewportInlineRoot {
                                    root,
                                    authority: prepared.authority.take(),
                                    begun: false,
                                });
                                break;
                            }
                            ViewportPreparedChildPublication::Unsupported(metadata) => {
                                drop(metadata);
                                drop(prepared.authority.take());
                                transitions = checked_add(transitions, 1)?;
                            }
                        }
                    }
                    if cleanup.releasing.is_some() {
                        self.viewport_inline_batch =
                            Some(ViewportInlineBatchState::Cancelling(cleanup));
                        continue;
                    }

                    while transitions < fuel {
                        let Some(mut ready) = cleanup.ready.pop() else {
                            if !cleanup.prepared.is_empty() || cleanup.active_child.is_some() {
                                self.viewport_inline_batch =
                                    Some(ViewportInlineBatchState::Cancelling(cleanup));
                                return Err(CandidateEndpointError::InvalidState);
                            }
                            let hot_replacement = cleanup.hot_replacement.take();
                            self.viewport_inline_batch = None;
                            if let Some(replacement) = hot_replacement {
                                if self.hot_inline.is_some() {
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                                self.hot_inline =
                                    Some(stage_resolved_hot_inline(runtime, *replacement)?);
                            }
                            return Ok(transitions);
                        };
                        match ready.publication {
                            ViewportInlineLeafPublication::Authoritative(root) => {
                                cleanup.releasing = Some(ReleasingViewportInlineRoot {
                                    root,
                                    authority: ready.authority.take(),
                                    begun: false,
                                });
                                break;
                            }
                            ViewportInlineLeafPublication::Unsupported(record) => {
                                drop(record);
                                drop(ready.authority.take());
                                transitions = checked_add(transitions, 1)?;
                            }
                        }
                    }
                    self.viewport_inline_batch =
                        Some(ViewportInlineBatchState::Cancelling(cleanup));
                }
            }
        }
        Ok(transitions)
    }

    /// A completed recursive-Green adoption is already the definitive exact
    /// structural parse. Once it is ready, do not spend another quantum on a
    /// parked legacy crop or whole-document fallback merely to manufacture
    /// the old flat block roles.
    fn preempt_parked_exact_with_recursive_green(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<bool, CandidateEndpointError> {
        let Some((base_ack, target_source, syntax_profile, can_preempt_to_clean)) =
            self.active.as_ref().and_then(|active| match active {
                ActiveCandidate::ParsingExact(parsing) => {
                    Some((
                        parsing.base.ack,
                        parsing.witness.target(),
                        parsing.witness.parser_profile(),
                        true,
                    ))
                }
                ActiveCandidate::ParsingOrdinaryExact(parsing) => {
                    Some((
                        parsing.base.ack,
                        parsing.witness.target(),
                        parsing.witness.parser_profile(),
                        true,
                    ))
                }
                ActiveCandidate::AwaitingRecursiveGreenExact(awaiting) => {
                    Some((
                        awaiting.base.ack,
                        awaiting.witness.target(),
                        awaiting.witness.parser_profile(),
                        true,
                    ))
                }
                ActiveCandidate::ParsingExactFallback(parsing) => {
                    Some((
                        parsing.base.ack,
                        parsing.witness.target(),
                        parsing.witness.parser_profile(),
                        false,
                    ))
                }
                _ => None,
            })
        else {
            return Ok(false);
        };
        let syntax_profile = u32::try_from(syntax_profile.get())
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let update_ready = self
            .recursive_green
            .ready_update_for(base_ack, target_source)
            .is_some();
        let clean_ready = self
            .recursive_green
            .incremental_clean_ready_for_recursive_base(
                base_ack,
                target_source,
                syntax_profile,
            );
        if !update_ready && !(clean_ready && can_preempt_to_clean) {
            return Ok(false);
        }

        let active = self
            .active
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?;
        let (context, base, witness, certified) = match active {
            ActiveCandidate::ParsingExact(parsing) => {
                let ParsingExactCandidate {
                    context,
                    job,
                    mut base,
                    witness,
                } = *parsing;
                let restart = match job.cancel_into_base_restart_checkpoint() {
                    Ok(restart) => restart,
                    Err(_) => {
                        self.recursive_green.request_cancel_pending()?;
                        self.cleanup = Some(CandidateCleanup::RetainedPublication {
                            publication: base.publication,
                            begun: false,
                        });
                        return Err(CandidateEndpointError::InvalidState);
                    }
                };
                base.restart = Some(CandidateRestartAuthority::Leading(restart));
                let certified = match runtime.certify_current_persistent_source() {
                    Ok(certified) => certified,
                    Err(error) => {
                        self.recursive_green.request_cancel_pending()?;
                        self.cleanup = Some(CandidateCleanup::RetainedPublication {
                            publication: base.publication,
                            begun: false,
                        });
                        return Err(error.into());
                    }
                };
                (context, base, witness, certified)
            }
            ActiveCandidate::ParsingOrdinaryExact(parsing) => {
                let ParsingOrdinaryExactCandidate {
                    context,
                    job,
                    mut base,
                    witness,
                } = *parsing;
                let restart = match job.cancel_into_base_restart_checkpoints() {
                    Ok(restart) => restart,
                    Err(_) => {
                        self.recursive_green.request_cancel_pending()?;
                        self.cleanup = Some(CandidateCleanup::RetainedPublication {
                            publication: base.publication,
                            begun: false,
                        });
                        return Err(CandidateEndpointError::InvalidState);
                    }
                };
                base.restart = Some(CandidateRestartAuthority::Ordinary(restart));
                let certified = match runtime.certify_current_persistent_source() {
                    Ok(certified) => certified,
                    Err(error) => {
                        self.recursive_green.request_cancel_pending()?;
                        self.cleanup = Some(CandidateCleanup::RetainedPublication {
                            publication: base.publication,
                            begun: false,
                        });
                        return Err(error.into());
                    }
                };
                (context, base, witness, certified)
            }
            ActiveCandidate::AwaitingRecursiveGreenExact(awaiting) => {
                let AwaitingRecursiveGreenExactCandidate {
                    context,
                    certified,
                    base,
                    witness,
                } = *awaiting;
                (context, base, witness, certified)
            }
            ActiveCandidate::ParsingExactFallback(parsing) => {
                let ParsingExactFallbackCandidate {
                    context,
                    certified,
                    job,
                    base,
                    witness,
                } = *parsing;
                drop(job);
                (context, base, witness, certified)
            }
            other => {
                self.active = Some(other);
                return Ok(false);
            }
        };

        if certified.source() != target_source
            || certified.parser_profile() != witness.parser_profile()
            || certified.source_facts_profile() != witness.profile()
            || base.restart.is_none()
        {
            self.recursive_green.request_cancel_pending()?;
            self.cleanup = Some(CandidateCleanup::RetainedPublication {
                publication: base.publication,
                begun: false,
            });
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let next_restart = CandidateRestartAuthority::RecursiveGreen {
            source: target_source,
            binding: M11ParserBinding::new(witness.parser_profile(), GRAMMAR_REVISION),
        };
        if clean_ready && can_preempt_to_clean {
            let job = match M11CleanParseJob::new(certified.exact_parse_lease()) {
                Ok(job) => job,
                Err(error) => {
                    self.recursive_green.request_cancel_pending()?;
                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                        publication: base.publication,
                        begun: false,
                    });
                    return Err(error.into());
                }
            };
            self.active = Some(ActiveCandidate::ParsingExactFallback(Box::new(
                ParsingExactFallbackCandidate {
                    context,
                    certified,
                    job,
                    base,
                    witness,
                },
            )));
            return Ok(true);
        }
        let update = self
            .recursive_green
            .ready_update_for(base_ack, target_source)
            .ok_or(CandidateEndpointError::InvalidAuthority)?;
        let candidate = match M11ParserCandidate::derive_with_recursive_green_reusing_references(
            certified, update,
        ) {
            Ok(candidate) => candidate,
            Err(error) => {
                self.recursive_green.request_cancel_pending()?;
                self.cleanup = Some(CandidateCleanup::RetainedPublication {
                    publication: base.publication,
                    begun: false,
                });
                return Err(error.into());
            }
        };
        let publication = derive_identity(
            b"publication",
            context.binding,
            context.completion,
            context.parse_generation,
        );
        let update = self
            .recursive_green
            .ready_update_for(base_ack, target_source)
            .ok_or(CandidateEndpointError::InvalidAuthority)?;
        let writer = match candidate.into_writer_with_recursive_green_reusing_references(
            runtime,
            document_bytes(context.binding.document_session),
            publication,
            u64::from(context.parse_generation),
            update,
            &base.publication,
        ) {
            Ok(writer) => writer,
            Err(error) => {
                self.recursive_green.request_cancel_pending()?;
                self.cleanup = Some(CandidateCleanup::RetainedPublication {
                    publication: base.publication,
                    begun: false,
                });
                return Err(error.into());
            }
        };
        self.active = Some(ActiveCandidate::BuildingExact {
            context,
            writer: Box::new(writer),
            base,
            witness,
            next_restart,
            structural_path: ExactStructuralPath::RecursiveGreen,
        });
        Ok(true)
    }

    pub(crate) fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<CandidatePoll, CandidateEndpointError> {
        if fuel == 0 {
            return Err(CandidateEndpointError::InvalidState);
        }
        if self.cleanup.is_some() {
            let complete = self.poll_cleanup(runtime, fuel)?;
            return Ok(CandidatePoll::Pending {
                transitions: if complete { 1 } else { fuel },
            });
        }
        if self.recursive_green.target_work_pending() {
            let transitions = self.recursive_green.poll_target(runtime, fuel)?;
            if let Some(event) = self.take_recursive_green_delivery_event()? {
                return Ok(CandidatePoll::Event {
                    transitions,
                    event: Box::new(event),
                });
            }
            return Ok(CandidatePoll::Pending { transitions });
        }
        if let Some(event) = self.take_recursive_green_delivery_event()? {
            return Ok(CandidatePoll::Event {
                transitions: 0,
                event: Box::new(event),
            });
        }
        if self.preempt_parked_exact_with_recursive_green(runtime)? {
            return Ok(CandidatePoll::Pending { transitions: 1 });
        }
        if self.active.is_none() && self.recursive_green.cleanup_pending() {
            let transitions = self.recursive_green.poll_cleanup(runtime, fuel)?;
            return Ok(CandidatePoll::Pending { transitions });
        }
        if self.active.is_none() {
            let mut transitions = 0;
            if self.viewport_inline_batch_has_poll_work() {
                transitions = self.poll_viewport_inline_batch(runtime, fuel - transitions)?;
                if self.viewport_inline_batch_has_poll_work() || transitions == fuel {
                    return Ok(CandidatePoll::Pending { transitions });
                }
            }
            if self.viewport_inline_batch.is_none() {
                if let Some((viewport_generation, reason)) =
                    self.pending_viewport_unavailable.take()
                {
                    return Ok(CandidatePoll::ViewportPresentationUnavailable {
                        transitions,
                        viewport_generation,
                        reason,
                    });
                }
            }
            if matches!(
                self.viewport_inline_batch,
                Some(ViewportInlineBatchState::Ready(_))
            ) {
                let Some(ViewportInlineBatchState::Ready(ready)) =
                    self.viewport_inline_batch.take()
                else {
                    return Err(CandidateEndpointError::InvalidState);
                };
                let Some(retained) = self.retained.as_ref() else {
                    self.viewport_inline_batch = Some(ViewportInlineBatchState::Cancelling(
                        Box::new((*ready).into_cleanup()),
                    ));
                    return Err(CandidateEndpointError::InvalidAuthority);
                };
                let viewport_generation = ready.command.viewport_generation;
                match prepare_viewport_presentation(runtime, retained, *ready) {
                    Ok(streaming) => {
                        self.viewport_inline_batch =
                            Some(ViewportInlineBatchState::Streaming(Box::new(streaming)));
                    }
                    Err(failure) => {
                        let reason = match &failure.error {
                            CandidateEndpointError::ViewportInlineLimitExceeded(_) => {
                                Some(ViewportPresentationUnavailableReason::BudgetExceeded)
                            }
                            CandidateEndpointError::Publication(_)
                            | CandidateEndpointError::InlineProjection(_)
                            | CandidateEndpointError::InlineProjectionJob(_) => {
                                Some(ViewportPresentationUnavailableReason::DerivationFailed)
                            }
                            _ => None,
                        };
                        if let Some(reason) = reason {
                            self.install_viewport_unavailable_cleanup(
                                viewport_generation,
                                reason,
                                failure.cleanup,
                            )?;
                            if transitions == 0 {
                                transitions = 1;
                            }
                            return Ok(CandidatePoll::Pending { transitions });
                        }
                        self.viewport_inline_batch = Some(ViewportInlineBatchState::Cancelling(
                            Box::new(failure.cleanup),
                        ));
                        return Err(failure.error);
                    }
                }
            }
            if matches!(
                self.viewport_inline_batch,
                Some(ViewportInlineBatchState::Streaming(_))
            ) {
                let Some(ViewportInlineBatchState::Streaming(mut streaming)) =
                    self.viewport_inline_batch.take()
                else {
                    return Err(CandidateEndpointError::InvalidState);
                };
                let polled = streaming.poll_event(runtime, fuel.saturating_sub(transitions));
                self.viewport_inline_batch = Some(ViewportInlineBatchState::Streaming(streaming));
                return match polled? {
                    CandidatePoll::Pending {
                        transitions: consumed,
                    } => Ok(CandidatePoll::Pending {
                        transitions: checked_add(transitions, consumed)?,
                    }),
                    CandidatePoll::ViewportPresentationEvent {
                        transitions: consumed,
                        event,
                    } => Ok(CandidatePoll::ViewportPresentationEvent {
                        transitions: checked_add(transitions, consumed)?,
                        event,
                    }),
                    CandidatePoll::Event { .. }
                    | CandidatePoll::HotInlineEvent { .. }
                    | CandidatePoll::ViewportPresentationUnavailable { .. } => {
                        Err(CandidateEndpointError::InvalidState)
                    }
                };
            }
            if self.hot_inline_sidecar.is_none() {
                transitions = checked_add(
                    transitions,
                    self.poll_hot_inline(runtime, fuel - transitions)?,
                )?;
                if let Some(ready) = self.take_hot_inline_ready() {
                    self.begin_hot_inline_sidecar(runtime, ready)?;
                }
            }
            let Some(mut sidecar) = self.hot_inline_sidecar.take() else {
                return Ok(CandidatePoll::Pending { transitions });
            };
            let polled = sidecar.poll_event(runtime, fuel.saturating_sub(transitions));
            self.hot_inline_sidecar = Some(sidecar);
            return match polled? {
                CandidatePoll::Pending {
                    transitions: consumed,
                } => Ok(CandidatePoll::Pending {
                    transitions: checked_add(transitions, consumed)?,
                }),
                CandidatePoll::HotInlineEvent {
                    transitions: consumed,
                    event,
                } => Ok(CandidatePoll::HotInlineEvent {
                    transitions: checked_add(transitions, consumed)?,
                    event,
                }),
                CandidatePoll::Event { .. }
                | CandidatePoll::ViewportPresentationEvent { .. }
                | CandidatePoll::ViewportPresentationUnavailable { .. } => {
                    Err(CandidateEndpointError::InvalidState)
                }
            };
        }

        let mut transitions = 0;
        while transitions < fuel {
            let active = self
                .active
                .take()
                .ok_or(CandidateEndpointError::InvalidState)?;
            match active {
                ActiveCandidate::Parsing(mut parsing) => {
                    let polled = match parsing.job.poll(fuel - transitions) {
                        Ok(polled) => polled,
                        Err(error) => {
                            self.active = Some(ActiveCandidate::Parsing(parsing));
                            return Err(error.into());
                        }
                    };
                    match polled {
                        M11CleanParsePoll::Pending {
                            transitions: consumed,
                        } => {
                            transitions = checked_add(transitions, consumed)?;
                            self.active = Some(ActiveCandidate::Parsing(parsing));
                            return Ok(CandidatePoll::Pending { transitions });
                        }
                        M11CleanParsePoll::Complete {
                            transitions: consumed,
                            mut result,
                        } => {
                            transitions = checked_add(transitions, consumed)?;
                            let ParsingCandidate {
                                context,
                                certified,
                                publication_path,
                                ..
                            } = *parsing;
                            let source = certified.source();
                            let syntax_profile = u32::try_from(certified.parser_profile().get())
                                .map_err(|_| CandidateEndpointError::MetricOverflow)?;
                            let parser_binding =
                                M11ParserBinding::current(certified.parser_profile());
                            let parser_restart =
                                take_candidate_restart_authority(&mut result, parser_binding)?;
                            let next_restart = match publication_path {
                                CleanPublicationPath::RecursiveGreenInitial => {
                                    Some(CandidateRestartAuthority::RecursiveGreen {
                                        source,
                                        binding: parser_binding,
                                    })
                                }
                                CleanPublicationPath::LegacyBlocks => parser_restart,
                            };
                            let publication = derive_identity(
                                b"publication",
                                context.binding,
                                context.completion,
                                context.parse_generation,
                            );
                            let writer = match publication_path {
                                CleanPublicationPath::RecursiveGreenInitial => {
                                    let session = self
                                        .recursive_green
                                        .initial_clean_ready_session(source, syntax_profile)?;
                                    let candidate =
                                        M11ParserCandidate::derive_with_recursive_green(
                                            certified, &result, session,
                                        )?;
                                    candidate.into_writer_with_recursive_green(
                                        runtime,
                                        document_bytes(context.binding.document_session),
                                        publication,
                                        u64::from(context.parse_generation),
                                        session,
                                    )?
                                }
                                CleanPublicationPath::LegacyBlocks => {
                                    let candidate =
                                        M11ParserCandidate::derive_segmented(certified, result)?;
                                    candidate.into_writer(
                                        runtime,
                                        document_bytes(context.binding.document_session),
                                        publication,
                                        u64::from(context.parse_generation),
                                    )?
                                }
                            };
                            self.active = Some(ActiveCandidate::Building {
                                context,
                                writer: Box::new(writer),
                                next_restart,
                            });
                        }
                    }
                }
                ActiveCandidate::Building {
                    context,
                    mut writer,
                    next_restart,
                } => {
                    let polled = match writer.poll(runtime, fuel - transitions) {
                        Ok(polled) => polled,
                        Err(error) => {
                            self.active = Some(ActiveCandidate::Building {
                                context,
                                writer,
                                next_restart,
                            });
                            return Err(error.into());
                        }
                    };
                    match polled {
                        M11ParserCandidateWriterPoll::Pending {
                            transitions: consumed,
                        } => {
                            transitions = checked_add(transitions, consumed)?;
                            self.active = Some(ActiveCandidate::Building {
                                context,
                                writer,
                                next_restart,
                            });
                            return Ok(CandidatePoll::Pending { transitions });
                        }
                        M11ParserCandidateWriterPoll::Published {
                            transitions: consumed,
                            publication,
                        } => {
                            transitions = checked_add(transitions, consumed)?;
                            let descriptor = match publication.descriptor(runtime) {
                                Ok(descriptor) => descriptor,
                                Err(error) => {
                                    self.cleanup = Some(CandidateCleanup::Publication {
                                        publication,
                                        begun: false,
                                    });
                                    return Err(error.into());
                                }
                            };
                            let stream = match publication.into_snapshot_stream(runtime) {
                                Ok(stream) => stream,
                                Err(failure) => {
                                    let (error, publication) = failure.into_parts();
                                    self.cleanup = Some(CandidateCleanup::Publication {
                                        publication,
                                        begun: false,
                                    });
                                    return Err(error.into());
                                }
                            };
                            let offer = match offer_begin(context, descriptor) {
                                Ok(offer) => offer,
                                Err(error) => {
                                    self.cleanup = Some(CandidateCleanup::Stream {
                                        stream: Box::new(stream),
                                        begun: false,
                                    });
                                    return Err(error);
                                }
                            };
                            self.active =
                                Some(ActiveCandidate::Streaming(Box::new(StreamingCandidate {
                                    stream: Some(stream),
                                    sealed_publication: None,
                                    offer,
                                    descriptor,
                                    phase: StreamPhase::NeedBegin,
                                    transport: Some(CandidateTransportDigest::new()),
                                    next_frame_ordinal: 0,
                                    next_record_ordinal: 0,
                                    next_node_ordinal: None,
                                    packet: PacketBuilder::default(),
                                    lookahead: None,
                                    resume_after_packet_credit: false,
                                    canonical_stream_digest: None,
                                    commit: None,
                                    expected_ack: None,
                                    next_restart,
                                    superseded_exact_base: None,
                                    exact_base_recovery: None,
                                })));
                        }
                    }
                }
                ActiveCandidate::ParsingExact(mut parsing) => {
                    let polled = match parsing.job.poll(fuel - transitions) {
                        Ok(polled) => polled,
                        Err(error) if leading_crop_declined_semantically(&error) => {
                            let ParsingExactCandidate {
                                context,
                                job,
                                mut base,
                                witness,
                            } = *parsing;
                            let base_restart = match job.cancel_into_base_restart_checkpoint() {
                                Ok(restart) => restart,
                                Err(_) => {
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: base.publication,
                                        begun: false,
                                    });
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                            base.restart = Some(CandidateRestartAuthority::Leading(base_restart));
                            match begin_exact_clean_fallback(runtime, context, base, witness) {
                                Ok(active) => {
                                    self.active = Some(active);
                                    return Ok(CandidatePoll::Pending { transitions: fuel });
                                }
                                Err(failure) => {
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: failure.base.publication,
                                        begun: false,
                                    });
                                    return Err(failure.error);
                                }
                            }
                        }
                        Err(error) => {
                            self.active = Some(ActiveCandidate::ParsingExact(parsing));
                            return Err(error.into());
                        }
                    };
                    match polled {
                        M11LeadingReferencesCropPoll::Pending {
                            transitions: consumed,
                        } => {
                            transitions = match checked_add(transitions, consumed) {
                                Ok(next) if next <= fuel => next,
                                Ok(_) | Err(_) => {
                                    self.active = Some(ActiveCandidate::ParsingExact(parsing));
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                            self.active = Some(ActiveCandidate::ParsingExact(parsing));
                            return Ok(CandidatePoll::Pending { transitions });
                        }
                        M11LeadingReferencesCropPoll::Complete {
                            transitions: consumed,
                            result,
                        } => {
                            let ParsingExactCandidate {
                                context,
                                base,
                                witness,
                                ..
                            } = *parsing;
                            transitions = match checked_add(transitions, consumed) {
                                Ok(transitions) => transitions,
                                Err(error) => {
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: base.publication,
                                        begun: false,
                                    });
                                    return Err(error);
                                }
                            };
                            match begin_exact_candidate_build(
                                runtime, context, base, witness, result,
                            ) {
                                Ok(active) => self.active = Some(active),
                                Err(failure) => {
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: failure.base.publication,
                                        begun: false,
                                    });
                                    return Err(failure.error);
                                }
                            }
                        }
                    }
                }
                ActiveCandidate::ParsingOrdinaryExact(mut parsing) => {
                    let polled = match parsing.job.poll(fuel - transitions) {
                        Ok(polled) => polled,
                        Err(error) if ordinary_crop_declined_semantically(&error) => {
                            let ParsingOrdinaryExactCandidate {
                                context,
                                job,
                                mut base,
                                witness,
                            } = *parsing;
                            let base_restart = match job.cancel_into_base_restart_checkpoints() {
                                Ok(restart) => restart,
                                Err(_) => {
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: base.publication,
                                        begun: false,
                                    });
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                            base.restart = Some(CandidateRestartAuthority::Ordinary(base_restart));
                            match begin_exact_clean_fallback(runtime, context, base, witness) {
                                Ok(active) => {
                                    self.active = Some(active);
                                    return Ok(CandidatePoll::Pending { transitions: fuel });
                                }
                                Err(failure) => {
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: failure.base.publication,
                                        begun: false,
                                    });
                                    return Err(failure.error);
                                }
                            }
                        }
                        Err(error) => {
                            self.active = Some(ActiveCandidate::ParsingOrdinaryExact(parsing));
                            return Err(error);
                        }
                    };
                    match polled {
                        OrdinaryExactPoll::Pending {
                            transitions: consumed,
                        } => {
                            transitions = match checked_add(transitions, consumed) {
                                Ok(next) if next <= fuel => next,
                                Ok(_) | Err(_) => {
                                    self.active =
                                        Some(ActiveCandidate::ParsingOrdinaryExact(parsing));
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                            self.active = Some(ActiveCandidate::ParsingOrdinaryExact(parsing));
                            return Ok(CandidatePoll::Pending { transitions });
                        }
                        OrdinaryExactPoll::Complete {
                            transitions: consumed,
                            result,
                        } => {
                            let ParsingOrdinaryExactCandidate {
                                context,
                                base,
                                witness,
                                ..
                            } = *parsing;
                            transitions = match checked_add(transitions, consumed) {
                                Ok(transitions) => transitions,
                                Err(error) => {
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: base.publication,
                                        begun: false,
                                    });
                                    return Err(error);
                                }
                            };
                            match begin_exact_candidate_build_ordinary(
                                runtime, context, base, witness, result,
                            ) {
                                Ok(active) => self.active = Some(active),
                                Err(failure) => {
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: failure.base.publication,
                                        begun: false,
                                    });
                                    return Err(failure.error);
                                }
                            }
                        }
                    }
                }
                ActiveCandidate::AwaitingRecursiveGreenExact(awaiting) => {
                    self.active = Some(ActiveCandidate::AwaitingRecursiveGreenExact(awaiting));
                    return Err(CandidateEndpointError::InvalidState);
                }
                ActiveCandidate::ParsingBulletListLocal(mut parsing) => {
                    let polled = match parsing.job.poll(fuel - transitions) {
                        Ok(polled) => polled,
                        Err(_) => {
                            let ParsingBulletListLocalCandidate {
                                context,
                                job,
                                base,
                                witness,
                                ..
                            } = *parsing;
                            drop(job.cancel_into_source_authority());
                            self.bullet_list_local_edit = None;
                            match begin_exact_clean_fallback(runtime, context, base, witness) {
                                Ok(active) => {
                                    self.active = Some(active);
                                    return Ok(CandidatePoll::Pending { transitions: fuel });
                                }
                                Err(failure) => {
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: failure.base.publication,
                                        begun: false,
                                    });
                                    return Err(failure.error);
                                }
                            }
                        }
                    };
                    match polled {
                        M11BulletListLocalDeltaPoll::Pending {
                            transitions: consumed,
                        } => {
                            transitions = match checked_add(transitions, consumed) {
                                Ok(next) if next <= fuel => next,
                                _ => {
                                    self.active =
                                        Some(ActiveCandidate::ParsingBulletListLocal(parsing));
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                            self.active = Some(ActiveCandidate::ParsingBulletListLocal(parsing));
                            return Ok(CandidatePoll::Pending { transitions });
                        }
                        M11BulletListLocalDeltaPoll::Complete {
                            transitions: consumed,
                            mut result,
                        } => {
                            transitions = match checked_add(transitions, consumed) {
                                Ok(next) if next <= fuel => next,
                                _ => return Err(CandidateEndpointError::InvalidState),
                            };
                            let ParsingBulletListLocalCandidate {
                                context,
                                base,
                                witness,
                                target_source,
                                target_binding,
                                predecessor_end_byte,
                                predecessor_end_utf16,
                                successor_start_byte,
                                successor_start_utf16,
                                ..
                            } = *parsing;
                            let Some(target_lease) = result.take_target_source_lease() else {
                                return Err(CandidateEndpointError::InvalidState);
                            };
                            let input =
                                M11ExactSegmentedCandidateInput::from_bullet_list_local_delta(
                                    target_lease,
                                    result.terminal(),
                                );
                            let input = match input {
                                Ok(input) => input,
                                Err(_) => {
                                    self.bullet_list_local_edit = None;
                                    match begin_exact_clean_fallback(
                                        runtime, context, base, witness,
                                    ) {
                                        Ok(active) => {
                                            self.active = Some(active);
                                            return Ok(CandidatePoll::Pending {
                                                transitions: fuel,
                                            });
                                        }
                                        Err(failure) => {
                                            self.cleanup =
                                                Some(CandidateCleanup::RetainedPublication {
                                                    publication: failure.base.publication,
                                                    begun: false,
                                                });
                                            return Err(failure.error);
                                        }
                                    }
                                }
                            };
                            let Some(plan) = result.take_base_plan() else {
                                drop(input);
                                return Err(CandidateEndpointError::InvalidState);
                            };
                            self.bullet_list_local_edit = Some(RollingBulletListLocalEdit {
                                plan,
                                current_source: target_source,
                                predecessor_end_byte,
                                predecessor_end_utf16,
                                successor_start_byte,
                                successor_start_utf16,
                            });
                            let next_restart = CandidateRestartAuthority::ExactBaseOnly {
                                source: target_source,
                                binding: target_binding,
                            };
                            match begin_exact_candidate_build_from_terminal(
                                runtime,
                                context,
                                base,
                                witness,
                                input,
                                next_restart,
                            ) {
                                Ok(active) => self.active = Some(active),
                                Err(failure) => {
                                    self.bullet_list_local_edit = None;
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: failure.base.publication,
                                        begun: false,
                                    });
                                    return Err(failure.error);
                                }
                            }
                        }
                    }
                }
                ActiveCandidate::ParsingExactFallback(mut parsing) => {
                    let polled = match parsing.job.poll(fuel - transitions) {
                        Ok(polled) => polled,
                        Err(error) => {
                            self.active = Some(ActiveCandidate::ParsingExactFallback(parsing));
                            return Err(error.into());
                        }
                    };
                    match polled {
                        M11CleanParsePoll::Pending {
                            transitions: consumed,
                        } => {
                            transitions = match checked_add(transitions, consumed) {
                                Ok(next) if next <= fuel => next,
                                Ok(_) | Err(_) => {
                                    self.active =
                                        Some(ActiveCandidate::ParsingExactFallback(parsing));
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                            self.active = Some(ActiveCandidate::ParsingExactFallback(parsing));
                            return Ok(CandidatePoll::Pending { transitions });
                        }
                        M11CleanParsePoll::Complete {
                            transitions: consumed,
                            mut result,
                        } => {
                            let ParsingExactFallbackCandidate {
                                context,
                                certified,
                                base,
                                witness,
                                ..
                            } = *parsing;
                            transitions = match checked_add(transitions, consumed) {
                                Ok(next) if next <= fuel => next,
                                Ok(_) | Err(_) => {
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: base.publication,
                                        begun: false,
                                    });
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                            let parser_binding =
                                M11ParserBinding::current(certified.parser_profile());
                            let next_restart =
                                match take_candidate_restart_authority(&mut result, parser_binding)
                                {
                                    Ok(restart) => restart,
                                    Err(error) => {
                                        self.cleanup =
                                            Some(CandidateCleanup::RetainedPublication {
                                                publication: base.publication,
                                                begun: false,
                                            });
                                        return Err(error);
                                    }
                                };
                            let target_source = certified.source();
                            let syntax_profile = u32::try_from(certified.parser_profile().get())
                                .map_err(|_| CandidateEndpointError::MetricOverflow)?;
                            if self
                                .recursive_green
                                .incremental_clean_ready_session(target_source, syntax_profile)
                                .is_some()
                            {
                                let consumed_witness =
                                    match runtime.take_persistent_source_facts_delta(witness) {
                                        Ok(witness) => witness,
                                        Err(error) => {
                                            self.cleanup =
                                                Some(CandidateCleanup::RetainedPublication {
                                                    publication: base.publication,
                                                    begun: false,
                                                });
                                            return Err(error.into());
                                        }
                                };
                                drop(consumed_witness);
                                let session = self
                                    .recursive_green
                                    .incremental_clean_ready_session(
                                        target_source,
                                        syntax_profile,
                                    )
                                    .ok_or(CandidateEndpointError::InvalidState)?;
                                let candidate = match M11ParserCandidate::
                                    derive_with_recursive_green_from_persistent(
                                        certified, &result, session,
                                    )
                                {
                                    Ok(candidate) => candidate,
                                    Err(error) => {
                                        self.cleanup =
                                            Some(CandidateCleanup::RetainedPublication {
                                                publication: base.publication,
                                                begun: false,
                                            });
                                        return Err(error.into());
                                    }
                                };
                                let publication = derive_identity(
                                    b"publication",
                                    context.binding,
                                    context.completion,
                                    context.parse_generation,
                                );
                                let writer = match candidate.into_writer_with_recursive_green(
                                    runtime,
                                    document_bytes(context.binding.document_session),
                                    publication,
                                    u64::from(context.parse_generation),
                                    session,
                                ) {
                                    Ok(writer) => writer,
                                    Err(error) => {
                                        self.cleanup =
                                            Some(CandidateCleanup::RetainedPublication {
                                                publication: base.publication,
                                                begun: false,
                                            });
                                        return Err(error.into());
                                    }
                                };
                                self.active = Some(ActiveCandidate::BuildingExactFallback {
                                    context,
                                    writer: Box::new(writer),
                                    base,
                                    next_restart,
                                });
                                continue;
                            }
                            let block_splice = match plan_exact_clean_block_splice(
                                runtime,
                                &base.publication,
                                base.restart.as_ref(),
                                &witness,
                                &result,
                            ) {
                                Ok(selection) => selection,
                                Err(error) => {
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: base.publication,
                                        begun: false,
                                    });
                                    return Err(error);
                                }
                            };
                            if let Some(selection) = block_splice {
                                let candidate = match M11ParserCandidate::
                                    derive_segmented_from_persistent_reusing_references(
                                        certified,
                                        result,
                                        selection,
                                    ) {
                                    Ok(candidate) => candidate,
                                    Err(error) => {
                                        self.cleanup =
                                            Some(CandidateCleanup::RetainedPublication {
                                                publication: base.publication,
                                                begun: false,
                                            });
                                        return Err(error.into());
                                    }
                                };
                                let publication = derive_identity(
                                    b"publication",
                                    context.binding,
                                    context.completion,
                                    context.parse_generation,
                                );
                                let writer = match candidate.into_writer(
                                    runtime,
                                    document_bytes(context.binding.document_session),
                                    publication,
                                    u64::from(context.parse_generation),
                                ) {
                                    Ok(writer) => writer,
                                    Err(error) => {
                                        self.cleanup =
                                            Some(CandidateCleanup::RetainedPublication {
                                                publication: base.publication,
                                                begun: false,
                                            });
                                        return Err(error.into());
                                    }
                                };
                                let Some(next_restart) = next_restart else {
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: base.publication,
                                        begun: false,
                                    });
                                    return Err(CandidateEndpointError::InvalidState);
                                };
                                self.active = Some(ActiveCandidate::BuildingExact {
                                    context,
                                    writer: Box::new(writer),
                                    base,
                                    witness,
                                    next_restart,
                                    structural_path: ExactStructuralPath::LegacyBlocks,
                                });
                                continue;
                            }
                            let consumed_witness = match runtime
                                .take_persistent_source_facts_delta(witness)
                            {
                                Ok(witness) => witness,
                                Err(error) => {
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: base.publication,
                                        begun: false,
                                    });
                                    return Err(error.into());
                                }
                            };
                            drop(consumed_witness);
                            let candidate =
                                match M11ParserCandidate::derive_segmented_from_persistent(
                                    certified, result,
                                ) {
                                    Ok(candidate) => candidate,
                                    Err(error) => {
                                        self.cleanup =
                                            Some(CandidateCleanup::RetainedPublication {
                                                publication: base.publication,
                                                begun: false,
                                            });
                                        return Err(error.into());
                                    }
                                };
                            let publication = derive_identity(
                                b"publication",
                                context.binding,
                                context.completion,
                                context.parse_generation,
                            );
                            let writer = match candidate.into_writer(
                                runtime,
                                document_bytes(context.binding.document_session),
                                publication,
                                u64::from(context.parse_generation),
                            ) {
                                Ok(writer) => writer,
                                Err(error) => {
                                    self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                        publication: base.publication,
                                        begun: false,
                                    });
                                    return Err(error.into());
                                }
                            };
                            self.active = Some(ActiveCandidate::BuildingExactFallback {
                                context,
                                writer: Box::new(writer),
                                base,
                                next_restart,
                            });
                        }
                    }
                }
                ActiveCandidate::BuildingExactFallback {
                    context,
                    mut writer,
                    mut base,
                    next_restart,
                } => {
                    let polled = match writer.poll(runtime, fuel - transitions) {
                        Ok(polled) => polled,
                        Err(error) => {
                            self.active = Some(ActiveCandidate::BuildingExactFallback {
                                context,
                                writer,
                                base,
                                next_restart,
                            });
                            return Err(error.into());
                        }
                    };
                    match polled {
                        M11ParserCandidateWriterPoll::Pending {
                            transitions: consumed,
                        } => {
                            transitions = match checked_add(transitions, consumed) {
                                Ok(next) if next <= fuel => next,
                                Ok(_) | Err(_) => {
                                    self.active = Some(ActiveCandidate::BuildingExactFallback {
                                        context,
                                        writer,
                                        base,
                                        next_restart,
                                    });
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                            self.active = Some(ActiveCandidate::BuildingExactFallback {
                                context,
                                writer,
                                base,
                                next_restart,
                            });
                            return Ok(CandidatePoll::Pending { transitions });
                        }
                        M11ParserCandidateWriterPoll::Published {
                            transitions: consumed,
                            publication,
                        } => {
                            transitions = match checked_add(transitions, consumed) {
                                Ok(next) if next <= fuel => next,
                                Ok(_) | Err(_) => {
                                    self.cleanup = Some(CandidateCleanup::ExactPublications {
                                        target: publication,
                                        target_begun: false,
                                        target_complete: false,
                                        base: base.publication,
                                        base_begun: false,
                                    });
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                            let descriptor = match publication.descriptor(runtime) {
                                Ok(descriptor) => descriptor,
                                Err(error) => {
                                    self.cleanup = Some(CandidateCleanup::ExactPublications {
                                        target: publication,
                                        target_begun: false,
                                        target_complete: false,
                                        base: base.publication,
                                        base_begun: false,
                                    });
                                    return Err(error.into());
                                }
                            };
                            let stream = match publication.into_snapshot_stream(runtime) {
                                Ok(stream) => stream,
                                Err(failure) => {
                                    let (error, target) = failure.into_parts();
                                    self.cleanup = Some(CandidateCleanup::ExactPublications {
                                        target,
                                        target_begun: false,
                                        target_complete: false,
                                        base: base.publication,
                                        base_begun: false,
                                    });
                                    return Err(error.into());
                                }
                            };
                            let base_restart = match base.restart.take() {
                                Some(restart) => restart,
                                None => {
                                    self.cleanup = Some(CandidateCleanup::StreamAndRetained {
                                        stream: Box::new(stream),
                                        stream_begun: false,
                                        stream_complete: false,
                                        base: base.publication,
                                        base_begun: false,
                                    });
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                            let offer = match offer_begin(context, descriptor) {
                                Ok(offer) => offer,
                                Err(error) => {
                                    self.restore_exact_base(ExactCandidateBase {
                                        publication: base.publication,
                                        ack: base.ack,
                                        restart: Some(base_restart),
                                    })?;
                                    self.cleanup = Some(CandidateCleanup::Stream {
                                        stream: Box::new(stream),
                                        begun: false,
                                    });
                                    return Err(error);
                                }
                            };
                            self.active =
                                Some(ActiveCandidate::Streaming(Box::new(StreamingCandidate {
                                    stream: Some(stream),
                                    sealed_publication: None,
                                    offer,
                                    descriptor,
                                    phase: StreamPhase::NeedBegin,
                                    transport: Some(CandidateTransportDigest::new()),
                                    next_frame_ordinal: 0,
                                    next_record_ordinal: 0,
                                    next_node_ordinal: None,
                                    packet: PacketBuilder::default(),
                                    lookahead: None,
                                    resume_after_packet_credit: false,
                                    canonical_stream_digest: None,
                                    commit: None,
                                    expected_ack: None,
                                    next_restart,
                                    superseded_exact_base: Some(base.publication),
                                    exact_base_recovery: Some(ExactBaseRecovery {
                                        ack: base.ack,
                                        restart: base_restart,
                                    }),
                                })));
                        }
                    }
                }
                ActiveCandidate::BuildingExact {
                    context,
                    mut writer,
                    mut base,
                    witness,
                    next_restart,
                    structural_path,
                } => {
                    if structural_path == ExactStructuralPath::LegacyBlocks
                        && self
                            .recursive_green
                            .owns_recursive_base_authority(base.ack)
                    {
                        self.active = Some(ActiveCandidate::BuildingExact {
                            context,
                            writer,
                            base,
                            witness,
                            next_restart,
                            structural_path,
                        });
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    let polled = match writer.poll_reusing_references(
                        runtime,
                        fuel - transitions,
                        &base.publication,
                    ) {
                        Ok(polled) => polled,
                        Err(error) => {
                            self.active = Some(ActiveCandidate::BuildingExact {
                                context,
                                writer,
                                base,
                                witness,
                                next_restart,
                                structural_path,
                            });
                            return Err(error.into());
                        }
                    };
                    match polled {
                        M11ParserCandidateWriterPoll::Pending {
                            transitions: consumed,
                        } => {
                            transitions = match checked_add(transitions, consumed) {
                                Ok(next) if next <= fuel => next,
                                Ok(_) | Err(_) => {
                                    self.active = Some(ActiveCandidate::BuildingExact {
                                        context,
                                        writer,
                                        base,
                                        witness,
                                        next_restart,
                                        structural_path,
                                    });
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                            self.active = Some(ActiveCandidate::BuildingExact {
                                context,
                                writer,
                                base,
                                witness,
                                next_restart,
                                structural_path,
                            });
                            return Ok(CandidatePoll::Pending { transitions });
                        }
                        M11ParserCandidateWriterPoll::Published {
                            transitions: consumed,
                            publication,
                        } => {
                            transitions = match checked_add(transitions, consumed) {
                                Ok(next) if next <= fuel => next,
                                Ok(_) | Err(_) => {
                                    self.cleanup = Some(CandidateCleanup::ExactPublications {
                                        target: publication,
                                        target_begun: false,
                                        target_complete: false,
                                        base: base.publication,
                                        base_begun: false,
                                    });
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                            let descriptor = match publication.descriptor(runtime) {
                                Ok(descriptor) => descriptor,
                                Err(error) => {
                                    self.cleanup = Some(CandidateCleanup::ExactPublications {
                                        target: publication,
                                        target_begun: false,
                                        target_complete: false,
                                        base: base.publication,
                                        base_begun: false,
                                    });
                                    return Err(error.into());
                                }
                            };
                            let base_restart = match base.restart.take() {
                                Some(restart) => restart,
                                None => {
                                    self.cleanup = Some(CandidateCleanup::ExactPublications {
                                        target: publication,
                                        target_begun: false,
                                        target_complete: false,
                                        base: base.publication,
                                        base_begun: false,
                                    });
                                    return Err(CandidateEndpointError::InvalidState);
                                }
                            };
                            let base_ack = base.ack;
                            let recursive_green_selection = match structural_path {
                                ExactStructuralPath::LegacyBlocks => None,
                                ExactStructuralPath::RecursiveGreen => {
                                    match self
                                        .recursive_green
                                        .ready_update_for(base_ack, witness.target())
                                    {
                                        Some(update) => {
                                            Some(update.recursive_green_splice_selection())
                                        }
                                        None => {
                                            self.cleanup =
                                                Some(CandidateCleanup::ExactPublications {
                                                    target: publication,
                                                    target_begun: false,
                                                    target_complete: false,
                                                    base: base.publication,
                                                    base_begun: false,
                                                });
                                            return Err(CandidateEndpointError::InvalidAuthority);
                                        }
                                    }
                                }
                            };
                            let stream_result = match recursive_green_selection {
                                Some(selection) => publication
                                    .into_exact_base_snapshot_stream_selecting_recursive_green_splice(
                                        runtime,
                                        base.publication,
                                        witness,
                                        selection,
                                    ),
                                None => publication
                                    .into_exact_base_snapshot_stream_selecting_block_splice(
                                        runtime,
                                        base.publication,
                                        witness,
                                    ),
                            };
                            let stream = match stream_result {
                                Ok(stream) => stream,
                                Err(failure) => {
                                    let (error, target, base, _witness) = failure.into_parts();
                                    self.restore_exact_base(ExactCandidateBase {
                                        publication: base,
                                        ack: base_ack,
                                        restart: Some(base_restart),
                                    })?;
                                    self.cleanup = Some(CandidateCleanup::Publication {
                                        publication: target,
                                        begun: false,
                                    });
                                    return Err(error.into());
                                }
                            };
                            let transferred_record_count =
                                stream.transferred_canonical_record_count();
                            let offer = match offer_begin_exact(
                                context,
                                descriptor,
                                transferred_record_count,
                                base_ack,
                            ) {
                                Ok(offer) => offer,
                                Err(error) => {
                                    self.cleanup = Some(CandidateCleanup::ExactStreamAndRestore {
                                        stream: Box::new(stream),
                                        stream_begun: false,
                                        base: None,
                                        recovery: Some(ExactBaseRecovery {
                                            ack: base_ack,
                                            restart: base_restart,
                                        }),
                                    });
                                    return Err(error);
                                }
                            };
                            self.active =
                                Some(ActiveCandidate::Streaming(Box::new(StreamingCandidate {
                                    stream: Some(stream),
                                    sealed_publication: None,
                                    offer,
                                    descriptor,
                                    phase: StreamPhase::NeedBegin,
                                    transport: Some(CandidateTransportDigest::new()),
                                    next_frame_ordinal: 0,
                                    next_record_ordinal: 0,
                                    next_node_ordinal: None,
                                    packet: PacketBuilder::default(),
                                    lookahead: None,
                                    resume_after_packet_credit: false,
                                    canonical_stream_digest: None,
                                    commit: None,
                                    expected_ack: None,
                                    next_restart: Some(next_restart),
                                    superseded_exact_base: None,
                                    exact_base_recovery: Some(ExactBaseRecovery {
                                        ack: base_ack,
                                        restart: base_restart,
                                    }),
                                })));
                        }
                    }
                }
                ActiveCandidate::Streaming(mut streaming) => {
                    let event = match streaming.poll_event(runtime, fuel - transitions) {
                        Ok(event) => event,
                        Err(error) => {
                            self.active = Some(ActiveCandidate::Streaming(streaming));
                            return Err(error);
                        }
                    };
                    self.active = Some(ActiveCandidate::Streaming(streaming));
                    return match event {
                        CandidatePoll::Pending {
                            transitions: consumed,
                        } => Ok(CandidatePoll::Pending {
                            transitions: checked_add(transitions, consumed)?,
                        }),
                        CandidatePoll::Event {
                            transitions: consumed,
                            event,
                        } => Ok(CandidatePoll::Event {
                            transitions: checked_add(transitions, consumed)?,
                            event,
                        }),
                        CandidatePoll::HotInlineEvent { .. }
                        | CandidatePoll::ViewportPresentationEvent { .. }
                        | CandidatePoll::ViewportPresentationUnavailable { .. } => {
                            Err(CandidateEndpointError::InvalidState)
                        }
                    };
                }
            }
        }
        Ok(CandidatePoll::Pending { transitions })
    }

    pub(crate) fn accept_credit(
        &mut self,
        credit: CandidateCredit,
        event_id: u32,
    ) -> Result<(), CandidateEndpointError> {
        let streaming = self.streaming_mut()?;
        streaming.phase = match (streaming.phase, credit) {
            (StreamPhase::AwaitBeginReceipt, CandidateCredit::Begin) => StreamPhase::NeedPacket,
            (
                StreamPhase::AwaitPacketReceipt {
                    first_frame_ordinal: expected_first,
                    frame_count: expected_count,
                    end: expected_end,
                },
                CandidateCredit::Packet {
                    first_frame_ordinal,
                    frame_count,
                    end,
                },
            ) if expected_first == first_frame_ordinal
                && expected_count == frame_count
                && expected_end == end =>
            {
                StreamPhase::AwaitPacketHost {
                    poll_ticket: event_id,
                    next_frame_ordinal: first_frame_ordinal
                        .checked_add(frame_count)
                        .ok_or(CandidateEndpointError::MetricOverflow)?,
                    end,
                }
            }
            (StreamPhase::AwaitCommitReceipt, CandidateCredit::Commit) => {
                StreamPhase::AwaitCommitHost {
                    poll_ticket: event_id,
                }
            }
            (StreamPhase::AwaitDeliveryReceipt, CandidateCredit::Delivery) => {
                self.finish_delivery()?;
                return Ok(());
            }
            _ => return Err(CandidateEndpointError::InvalidState),
        };
        Ok(())
    }

    pub(crate) fn accept_hot_inline_credit(
        &mut self,
        credit: HotInlineCredit,
        event_id: u32,
    ) -> Result<(), CandidateEndpointError> {
        let sidecar = self
            .hot_inline_sidecar
            .as_mut()
            .ok_or(CandidateEndpointError::InvalidState)?;
        sidecar.phase = match (sidecar.phase, credit) {
            (StreamPhase::AwaitBeginReceipt, HotInlineCredit::Begin) => StreamPhase::NeedPacket,
            (
                StreamPhase::AwaitPacketReceipt {
                    first_frame_ordinal: expected_first,
                    frame_count: expected_count,
                    end: expected_end,
                },
                HotInlineCredit::Packet {
                    first_frame_ordinal,
                    frame_count,
                    end,
                },
            ) if expected_first == first_frame_ordinal
                && expected_count == frame_count
                && expected_end == end =>
            {
                StreamPhase::AwaitPacketHost {
                    poll_ticket: event_id,
                    next_frame_ordinal: first_frame_ordinal
                        .checked_add(frame_count)
                        .ok_or(CandidateEndpointError::MetricOverflow)?,
                    end,
                }
            }
            (StreamPhase::AwaitCommitReceipt, HotInlineCredit::Commit) => {
                StreamPhase::AwaitCommitHost {
                    poll_ticket: event_id,
                }
            }
            (StreamPhase::AwaitDeliveryReceipt, HotInlineCredit::Delivery) => {
                self.finish_hot_inline_delivery()?;
                return Ok(());
            }
            _ => return Err(CandidateEndpointError::InvalidState),
        };
        Ok(())
    }

    pub(crate) fn accept_viewport_presentation_credit(
        &mut self,
        credit: ViewportPresentationCredit,
        event_id: u32,
    ) -> Result<(), CandidateEndpointError> {
        let Some(ViewportInlineBatchState::Streaming(streaming)) =
            self.viewport_inline_batch.as_mut()
        else {
            return Err(CandidateEndpointError::InvalidState);
        };
        streaming.phase = match (streaming.phase, credit) {
            (StreamPhase::AwaitBeginReceipt, ViewportPresentationCredit::Begin) => {
                StreamPhase::NeedPacket
            }
            (
                StreamPhase::AwaitPacketReceipt {
                    first_frame_ordinal: expected_first,
                    frame_count: expected_count,
                    end: expected_end,
                },
                ViewportPresentationCredit::Packet {
                    first_frame_ordinal,
                    frame_count,
                    end,
                },
            ) if expected_first == first_frame_ordinal
                && expected_count == frame_count
                && expected_end == end =>
            {
                StreamPhase::AwaitPacketHost {
                    poll_ticket: event_id,
                    next_frame_ordinal: first_frame_ordinal
                        .checked_add(frame_count)
                        .ok_or(CandidateEndpointError::MetricOverflow)?,
                    end,
                }
            }
            (StreamPhase::AwaitCommitReceipt, ViewportPresentationCredit::Commit) => {
                StreamPhase::AwaitCommitHost {
                    poll_ticket: event_id,
                }
            }
            (StreamPhase::AwaitDeliveryReceipt, ViewportPresentationCredit::Delivery) => {
                self.finish_viewport_presentation_delivery()?;
                return Ok(());
            }
            _ => return Err(CandidateEndpointError::InvalidState),
        };
        Ok(())
    }

    pub(crate) fn handle_viewport_presentation_host_poll(
        &mut self,
        poll_ticket: u32,
        offer_id: [u32; 4],
        phase: ViewportPresentationHostPollPhase,
        result: ViewportPresentationHostPollResult,
    ) -> Result<Option<CandidateViewportPresentationEvent>, CandidateEndpointError> {
        let Some(ViewportInlineBatchState::Streaming(streaming)) =
            self.viewport_inline_batch.as_ref()
        else {
            return Err(CandidateEndpointError::InvalidState);
        };
        let exact_ticket = match streaming.phase {
            StreamPhase::AwaitPacketHost {
                poll_ticket: expected,
                ..
            } => {
                phase == ViewportPresentationHostPollPhase::PacketCredit
                    && expected == poll_ticket
                    && streaming.offer.offer_id == offer_id
            }
            StreamPhase::AwaitCommitHost {
                poll_ticket: expected,
            } => {
                phase == ViewportPresentationHostPollPhase::Commit
                    && expected == poll_ticket
                    && streaming.offer.offer_id == offer_id
            }
            _ => false,
        };
        if !exact_ticket {
            return Err(CandidateEndpointError::InvalidState);
        }
        if matches!(result, ViewportPresentationHostPollResult::Rejected(_)) {
            self.cancel_viewport_presentation();
            return Ok(None);
        }
        enum HostEffect {
            None,
            Delivery(ViewportPresentationAck),
        }
        let effect = {
            let Some(ViewportInlineBatchState::Streaming(streaming)) =
                self.viewport_inline_batch.as_mut()
            else {
                return Err(CandidateEndpointError::InvalidState);
            };
            match (streaming.phase, phase, result) {
                (
                    StreamPhase::AwaitPacketHost {
                        poll_ticket: expected_ticket,
                        next_frame_ordinal,
                        end,
                    },
                    ViewportPresentationHostPollPhase::PacketCredit,
                    ViewportPresentationHostPollResult::Completed(
                        ViewportPresentationHostPollOutcome::PacketCredit {
                            offer_id,
                            next_frame_ordinal: credited_next_frame_ordinal,
                        },
                    ),
                ) if expected_ticket == poll_ticket
                    && offer_id == streaming.offer.offer_id
                    && credited_next_frame_ordinal == next_frame_ordinal =>
                {
                    streaming.phase = if end {
                        StreamPhase::NeedCommit
                    } else {
                        StreamPhase::NeedPacket
                    };
                    HostEffect::None
                }
                (
                    StreamPhase::AwaitCommitHost {
                        poll_ticket: expected_ticket,
                    },
                    ViewportPresentationHostPollPhase::Commit,
                    ViewportPresentationHostPollResult::Completed(
                        ViewportPresentationHostPollOutcome::Committed(ack),
                    ),
                ) if expected_ticket == poll_ticket && streaming.expected_ack == Some(ack) => {
                    streaming.phase = StreamPhase::AwaitDeliveryReceipt;
                    HostEffect::Delivery(ack)
                }
                _ => return Err(CandidateEndpointError::InvalidState),
            }
        };
        Ok(match effect {
            HostEffect::None => None,
            HostEffect::Delivery(ack) => Some(CandidateViewportPresentationEvent {
                credit: ViewportPresentationCredit::Delivery,
                body: CandidateViewportPresentationEventBody::DeliveryAcknowledged(ack),
            }),
        })
    }

    fn finish_viewport_presentation_delivery(&mut self) -> Result<(), CandidateEndpointError> {
        let Some(state) = self.viewport_inline_batch.take() else {
            return Err(CandidateEndpointError::InvalidState);
        };
        let ViewportInlineBatchState::Streaming(streaming) = state else {
            self.viewport_inline_batch = Some(state);
            return Err(CandidateEndpointError::InvalidState);
        };
        if streaming.phase != StreamPhase::AwaitDeliveryReceipt
            || streaming.expected_ack.is_none()
            || streaming.transport.is_some()
            || streaming.active.is_some()
            || !streaming.pending.is_empty()
            || streaming.releasing.is_some()
        {
            self.viewport_inline_batch = Some(ViewportInlineBatchState::Streaming(streaming));
            return Err(CandidateEndpointError::InvalidState);
        }
        Ok(())
    }

    pub(crate) fn handle_hot_inline_host_poll(
        &mut self,
        poll_ticket: u32,
        offer_id: [u32; 4],
        phase: InlineSidecarHostPollPhase,
        result: InlineSidecarHostPollResult,
    ) -> Result<Option<HotInlineEvent>, CandidateEndpointError> {
        let sidecar = self
            .hot_inline_sidecar
            .as_ref()
            .ok_or(CandidateEndpointError::InvalidState)?;
        let exact_ticket = match sidecar.phase {
            StreamPhase::AwaitPacketHost {
                poll_ticket: expected,
                ..
            } => {
                phase == InlineSidecarHostPollPhase::PacketCredit
                    && expected == poll_ticket
                    && sidecar.offer.offer_id == offer_id
            }
            StreamPhase::AwaitCommitHost {
                poll_ticket: expected,
            } => {
                phase == InlineSidecarHostPollPhase::Commit
                    && expected == poll_ticket
                    && sidecar.offer.offer_id == offer_id
            }
            _ => false,
        };
        if !exact_ticket {
            return Err(CandidateEndpointError::InvalidState);
        }
        if matches!(result, InlineSidecarHostPollResult::Rejected(_)) {
            self.cancel_hot_inline_sidecar();
            return Ok(None);
        }
        enum HostEffect {
            None,
            Delivery(InlineSidecarAck),
        }
        let effect = {
            let sidecar = self
                .hot_inline_sidecar
                .as_mut()
                .ok_or(CandidateEndpointError::InvalidState)?;
            match (sidecar.phase, phase, result) {
                (
                    StreamPhase::AwaitPacketHost {
                        poll_ticket: expected_ticket,
                        next_frame_ordinal,
                        end,
                    },
                    InlineSidecarHostPollPhase::PacketCredit,
                    InlineSidecarHostPollResult::Completed(
                        InlineSidecarHostPollOutcome::PacketCredit {
                            offer_id,
                            next_frame_ordinal: credited_next_frame_ordinal,
                        },
                    ),
                ) if expected_ticket == poll_ticket
                    && offer_id == sidecar.offer.offer_id
                    && credited_next_frame_ordinal == next_frame_ordinal =>
                {
                    sidecar.phase = if end {
                        StreamPhase::NeedCommit
                    } else {
                        StreamPhase::NeedPacket
                    };
                    HostEffect::None
                }
                (
                    StreamPhase::AwaitCommitHost {
                        poll_ticket: expected_ticket,
                    },
                    InlineSidecarHostPollPhase::Commit,
                    InlineSidecarHostPollResult::Completed(
                        InlineSidecarHostPollOutcome::Committed(ack),
                    ),
                ) if expected_ticket == poll_ticket && sidecar.expected_ack == Some(ack) => {
                    sidecar.phase = StreamPhase::AwaitDeliveryReceipt;
                    HostEffect::Delivery(ack)
                }
                _ => return Err(CandidateEndpointError::InvalidState),
            }
        };
        match effect {
            HostEffect::None => Ok(None),
            HostEffect::Delivery(ack) => Ok(Some(HotInlineEvent {
                credit: HotInlineCredit::Delivery,
                body: HotInlineEventBody::DeliveryAcknowledged(ack),
            })),
        }
    }

    fn finish_hot_inline_delivery(&mut self) -> Result<(), CandidateEndpointError> {
        let mut sidecar = self
            .hot_inline_sidecar
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?;
        if sidecar.phase != StreamPhase::AwaitDeliveryReceipt
            || sidecar.expected_ack.is_none()
            || sidecar.transport.is_some()
        {
            self.hot_inline_sidecar = Some(sidecar);
            return Err(CandidateEndpointError::InvalidState);
        }
        self.schedule_hot_inline_root_release(sidecar.root.take(), sidecar.authority.take());
        Ok(())
    }

    pub(crate) fn handle_host_poll(
        &mut self,
        poll_ticket: u32,
        offer_id: [u32; 4],
        phase: HostPollPhase,
        result: HostPollResult,
    ) -> Result<Option<CandidateEvent>, CandidateEndpointError> {
        let streaming = self.streaming_mut()?;
        let exact_ticket = match streaming.phase {
            StreamPhase::AwaitPacketHost {
                poll_ticket: expected,
                ..
            } => {
                phase == HostPollPhase::PacketCredit
                    && expected == poll_ticket
                    && streaming.offer.offer_id == offer_id
            }
            StreamPhase::AwaitCommitHost {
                poll_ticket: expected,
            } => {
                phase == HostPollPhase::Commit
                    && expected == poll_ticket
                    && streaming.offer.offer_id == offer_id
            }
            _ => false,
        };
        if !exact_ticket {
            return Err(CandidateEndpointError::InvalidState);
        }
        if matches!(result, HostPollResult::Rejected(_)) {
            self.cancel()?;
            return Ok(None);
        }
        enum HostEffect {
            None,
            Delivery(StructuralAck),
        }
        let effect = {
            let streaming = self.streaming_mut()?;
            match (streaming.phase, phase, result) {
                (
                    StreamPhase::AwaitPacketHost {
                        poll_ticket: expected_ticket,
                        next_frame_ordinal,
                        end,
                    },
                    HostPollPhase::PacketCredit,
                    HostPollResult::Completed(HostPollOutcome::PacketCredit {
                        offer_id,
                        next_frame_ordinal: credited_next_frame_ordinal,
                    }),
                ) if expected_ticket == poll_ticket
                    && offer_id == streaming.offer.offer_id
                    && credited_next_frame_ordinal == next_frame_ordinal =>
                {
                    if streaming.resume_after_packet_credit {
                        streaming
                            .stream
                            .as_mut()
                            .ok_or(CandidateEndpointError::InvalidState)?
                            .resume_exact_base_delta()?;
                        streaming.resume_after_packet_credit = false;
                    }
                    streaming.phase = if end {
                        StreamPhase::NeedCommit
                    } else {
                        StreamPhase::NeedPacket
                    };
                    HostEffect::None
                }
                (
                    StreamPhase::AwaitCommitHost {
                        poll_ticket: expected_ticket,
                    },
                    HostPollPhase::Commit,
                    HostPollResult::Completed(HostPollOutcome::Committed(ack)),
                ) if expected_ticket == poll_ticket && streaming.expected_ack == Some(ack) => {
                    streaming.phase = StreamPhase::AwaitRecursiveGreenDelivery;
                    HostEffect::Delivery(ack)
                }
                _ => return Err(CandidateEndpointError::InvalidState),
            }
        };
        match effect {
            HostEffect::None => Ok(None),
            HostEffect::Delivery(_) => self.take_recursive_green_delivery_event(),
        }
    }

    fn restore_exact_base(
        &mut self,
        mut base: ExactCandidateBase,
    ) -> Result<(), CandidateEndpointError> {
        let Some(restart) = base.restart.take() else {
            self.cleanup = Some(CandidateCleanup::RetainedPublication {
                publication: base.publication,
                begun: false,
            });
            return Err(CandidateEndpointError::InvalidState);
        };
        if self.retained.is_some() {
            self.cleanup = Some(CandidateCleanup::RetainedPublication {
                publication: base.publication,
                begun: false,
            });
            return Err(CandidateEndpointError::InvalidState);
        }
        self.retained = Some(RetainedCandidateBase {
            publication: base.publication,
            ack: base.ack,
            restart: Some(restart),
        });
        Ok(())
    }

    pub(crate) fn cancel(&mut self) -> Result<(), CandidateEndpointError> {
        let result = self.cancel_preserving_bullet_list_local_edit();
        self.bullet_list_local_edit = None;
        result
    }

    fn cancel_preserving_bullet_list_local_edit(&mut self) -> Result<(), CandidateEndpointError> {
        self.cancel_hot_inline();
        self.recursive_green.request_cancel_pending()?;
        if self.cleanup.is_some() {
            return match self.active.take() {
                None | Some(ActiveCandidate::Parsing(_)) => Ok(()),
                Some(active) => {
                    self.active = Some(active);
                    Err(CandidateEndpointError::InvalidState)
                }
            };
        }
        let Some(active) = self.active.take() else {
            return Ok(());
        };
        match active {
            ActiveCandidate::Parsing(_) => Ok(()),
            ActiveCandidate::Building {
                context,
                writer,
                next_restart,
            } => {
                if self.cleanup.is_some() {
                    self.active = Some(ActiveCandidate::Building {
                        context,
                        writer,
                        next_restart,
                    });
                    return Err(CandidateEndpointError::Busy);
                }
                self.cleanup = Some(CandidateCleanup::Writer {
                    writer,
                    begun: false,
                });
                Ok(())
            }
            ActiveCandidate::ParsingExact(parsing) => {
                if self.cleanup.is_some() {
                    self.active = Some(ActiveCandidate::ParsingExact(parsing));
                    return Err(CandidateEndpointError::Busy);
                }
                let ParsingExactCandidate { job, mut base, .. } = *parsing;
                let restart = match job.cancel_into_base_restart_checkpoint() {
                    Ok(restart) => restart,
                    Err(_) => {
                        self.cleanup = Some(CandidateCleanup::RetainedPublication {
                            publication: base.publication,
                            begun: false,
                        });
                        return Err(CandidateEndpointError::InvalidState);
                    }
                };
                base.restart = Some(CandidateRestartAuthority::Leading(restart));
                self.restore_exact_base(base)
            }
            ActiveCandidate::ParsingOrdinaryExact(parsing) => {
                if self.cleanup.is_some() {
                    self.active = Some(ActiveCandidate::ParsingOrdinaryExact(parsing));
                    return Err(CandidateEndpointError::Busy);
                }
                let ParsingOrdinaryExactCandidate { job, mut base, .. } = *parsing;
                let restart = match job.cancel_into_base_restart_checkpoints() {
                    Ok(restart) => restart,
                    Err(_) => {
                        self.cleanup = Some(CandidateCleanup::RetainedPublication {
                            publication: base.publication,
                            begun: false,
                        });
                        return Err(CandidateEndpointError::InvalidState);
                    }
                };
                base.restart = Some(CandidateRestartAuthority::Ordinary(restart));
                self.restore_exact_base(base)
            }
            ActiveCandidate::AwaitingRecursiveGreenExact(awaiting) => {
                if self.cleanup.is_some() {
                    self.active = Some(ActiveCandidate::AwaitingRecursiveGreenExact(awaiting));
                    return Err(CandidateEndpointError::Busy);
                }
                self.restore_exact_base(awaiting.base)
            }
            ActiveCandidate::ParsingBulletListLocal(parsing) => {
                let ParsingBulletListLocalCandidate {
                    job,
                    base,
                    target_source,
                    predecessor_end_byte,
                    predecessor_end_utf16,
                    successor_start_byte,
                    successor_start_utf16,
                    ..
                } = *parsing;
                let mut cancellation = job.cancel_into_source_authority()?;
                let plan = cancellation
                    .take_base_plan()
                    .ok_or(CandidateEndpointError::InvalidState)?;
                drop(cancellation.take_target_source_lease());
                self.bullet_list_local_edit = Some(RollingBulletListLocalEdit {
                    plan,
                    current_source: target_source,
                    predecessor_end_byte,
                    predecessor_end_utf16,
                    successor_start_byte,
                    successor_start_utf16,
                });
                self.restore_exact_base(base)
            }
            ActiveCandidate::ParsingExactFallback(parsing) => {
                if self.cleanup.is_some() {
                    self.active = Some(ActiveCandidate::ParsingExactFallback(parsing));
                    return Err(CandidateEndpointError::Busy);
                }
                self.restore_exact_base(parsing.base)
            }
            ActiveCandidate::BuildingExactFallback {
                context,
                writer,
                base,
                next_restart,
            } => {
                if self.cleanup.is_some() {
                    self.active = Some(ActiveCandidate::BuildingExactFallback {
                        context,
                        writer,
                        base,
                        next_restart,
                    });
                    return Err(CandidateEndpointError::Busy);
                }
                self.restore_exact_base(base)?;
                self.cleanup = Some(CandidateCleanup::Writer {
                    writer,
                    begun: false,
                });
                Ok(())
            }
            ActiveCandidate::BuildingExact {
                context,
                writer,
                base,
                witness,
                next_restart,
                structural_path,
            } => {
                if self.cleanup.is_some() {
                    self.active = Some(ActiveCandidate::BuildingExact {
                        context,
                        writer,
                        base,
                        witness,
                        next_restart,
                        structural_path,
                    });
                    return Err(CandidateEndpointError::Busy);
                }
                self.restore_exact_base(base)?;
                self.cleanup = Some(CandidateCleanup::Writer {
                    writer,
                    begun: false,
                });
                Ok(())
            }
            ActiveCandidate::Streaming(mut streaming) => {
                if self.cleanup.is_some() {
                    self.active = Some(ActiveCandidate::Streaming(streaming));
                    return Err(CandidateEndpointError::Busy);
                }
                match (
                    streaming.stream.is_some(),
                    streaming.sealed_publication.is_some(),
                ) {
                    (true, false) => {
                        let stream = streaming
                            .stream
                            .take()
                            .expect("stream presence was checked");
                        match (
                            streaming.superseded_exact_base.take(),
                            streaming.exact_base_recovery.take(),
                        ) {
                            (Some(base), Some(recovery)) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base,
                                    ack: recovery.ack,
                                    restart: Some(recovery.restart),
                                });
                                self.cleanup = Some(CandidateCleanup::Stream {
                                    stream: Box::new(stream),
                                    begun: false,
                                });
                            }
                            (None, Some(recovery)) => {
                                self.cleanup = Some(CandidateCleanup::ExactStreamAndRestore {
                                    stream: Box::new(stream),
                                    stream_begun: false,
                                    base: None,
                                    recovery: Some(recovery),
                                });
                            }
                            (Some(base), None) => {
                                self.cleanup = Some(CandidateCleanup::StreamAndRetained {
                                    stream: Box::new(stream),
                                    stream_begun: false,
                                    stream_complete: false,
                                    base,
                                    base_begun: false,
                                });
                            }
                            (None, None) => {
                                self.cleanup = Some(CandidateCleanup::Stream {
                                    stream: Box::new(stream),
                                    begun: false,
                                });
                            }
                        }
                    }
                    (false, true) => {
                        let publication = streaming
                            .sealed_publication
                            .take()
                            .expect("sealed publication presence was checked");
                        match (
                            streaming.superseded_exact_base.take(),
                            streaming.exact_base_recovery.take(),
                        ) {
                            (Some(base), Some(recovery)) => {
                                self.retained = Some(RetainedCandidateBase {
                                    publication: base,
                                    ack: recovery.ack,
                                    restart: Some(recovery.restart),
                                });
                                self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                    publication: Box::new(publication),
                                    begun: false,
                                });
                            }
                            (Some(base), None) => {
                                self.cleanup = Some(CandidateCleanup::RetainedPair {
                                    target: Box::new(publication),
                                    target_begun: false,
                                    target_complete: false,
                                    base,
                                    base_begun: false,
                                });
                            }
                            (None, None) => {
                                self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                    publication: Box::new(publication),
                                    begun: false,
                                });
                            }
                            (None, Some(_)) => {
                                self.cleanup = Some(CandidateCleanup::RetainedPublication {
                                    publication: Box::new(publication),
                                    begun: false,
                                });
                                return Err(CandidateEndpointError::InvalidState);
                            }
                        }
                    }
                    _ => {
                        self.active = Some(ActiveCandidate::Streaming(streaming));
                        return Err(CandidateEndpointError::InvalidState);
                    }
                }
                Ok(())
            }
        }
    }

    pub(crate) fn poll_cleanup(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<bool, CandidateEndpointError> {
        if fuel == 0 {
            return Err(CandidateEndpointError::InvalidState);
        }
        if self.recursive_green.cleanup_pending() {
            let _ = self.recursive_green.poll_cleanup(runtime, fuel)?;
            if self.closing {
                self.recursive_green.begin_close()?;
            }
            if self.recursive_green.cleanup_pending() {
                return Ok(false);
            }
            // Keep recursive-Green and legacy reclamation as separate bounded
            // quanta; the next poll resumes the existing cleanup actor.
            if self.cleanup.is_some() {
                return Ok(false);
            }
        }
        let Some(cleanup) = self.cleanup.as_mut() else {
            if self.viewport_inline_batch_has_poll_work() {
                let _ = self.poll_viewport_inline_batch(runtime, fuel)?;
                if self.viewport_inline_batch_has_poll_work() {
                    return Ok(false);
                }
            }
            if self.hot_inline_has_poll_work() {
                let _ = self.poll_hot_inline(runtime, fuel)?;
                if self.hot_inline_has_poll_work() {
                    return Ok(false);
                }
            }
            if self.schedule_close_cleanup()? {
                return Ok(false);
            }
            return Ok(!self.cleanup_pending());
        };
        let complete = match cleanup {
            CandidateCleanup::Writer { writer, begun } => {
                let mut remaining = fuel;
                if !*begun {
                    writer.begin_abort(runtime)?;
                    *begun = true;
                    remaining -= 1;
                    if remaining == 0 {
                        return Ok(false);
                    }
                }
                writer.poll_abort(runtime, remaining)?
            }
            CandidateCleanup::Publication { publication, begun } => {
                let mut remaining = fuel;
                if !*begun {
                    publication.begin_close(runtime)?;
                    *begun = true;
                    remaining -= 1;
                    if remaining == 0 {
                        return Ok(false);
                    }
                }
                publication.poll_close(runtime, remaining)?
            }
            CandidateCleanup::Stream { stream, begun } => {
                let mut remaining = fuel;
                if !*begun {
                    stream.begin_close(runtime)?;
                    *begun = true;
                    remaining -= 1;
                    if remaining == 0 {
                        return Ok(false);
                    }
                }
                stream.poll_close(runtime, remaining)?
            }
            CandidateCleanup::RetainedPublication { publication, begun } => {
                let mut remaining = fuel;
                if !*begun {
                    publication.begin_close(runtime)?;
                    *begun = true;
                    remaining -= 1;
                    if remaining == 0 {
                        return Ok(false);
                    }
                }
                publication.poll_close(runtime, remaining)?
            }
            CandidateCleanup::ExactPublications {
                target,
                target_begun,
                target_complete,
                base,
                base_begun,
            } => {
                if !*target_complete {
                    let mut remaining = fuel;
                    if !*target_begun {
                        target.begin_close(runtime)?;
                        *target_begun = true;
                        remaining -= 1;
                        if remaining == 0 {
                            return Ok(false);
                        }
                    }
                    if !target.poll_close(runtime, remaining)? {
                        return Ok(false);
                    }
                    *target_complete = true;
                    return Ok(false);
                }
                let mut remaining = fuel;
                if !*base_begun {
                    base.begin_close(runtime)?;
                    *base_begun = true;
                    remaining -= 1;
                    if remaining == 0 {
                        return Ok(false);
                    }
                }
                base.poll_close(runtime, remaining)?
            }
            CandidateCleanup::RetainedPair {
                target,
                target_begun,
                target_complete,
                base,
                base_begun,
            } => {
                if !*target_complete {
                    let mut remaining = fuel;
                    if !*target_begun {
                        target.begin_close(runtime)?;
                        *target_begun = true;
                        remaining -= 1;
                        if remaining == 0 {
                            return Ok(false);
                        }
                    }
                    if !target.poll_close(runtime, remaining)? {
                        return Ok(false);
                    }
                    *target_complete = true;
                    return Ok(false);
                }
                let mut remaining = fuel;
                if !*base_begun {
                    base.begin_close(runtime)?;
                    *base_begun = true;
                    remaining -= 1;
                    if remaining == 0 {
                        return Ok(false);
                    }
                }
                base.poll_close(runtime, remaining)?
            }
            CandidateCleanup::StreamAndRetained {
                stream,
                stream_begun,
                stream_complete,
                base,
                base_begun,
            } => {
                if !*stream_complete {
                    let mut remaining = fuel;
                    if !*stream_begun {
                        stream.begin_close(runtime)?;
                        *stream_begun = true;
                        remaining -= 1;
                        if remaining == 0 {
                            return Ok(false);
                        }
                    }
                    if !stream.poll_close(runtime, remaining)? {
                        return Ok(false);
                    }
                    *stream_complete = true;
                    return Ok(false);
                }
                let mut remaining = fuel;
                if !*base_begun {
                    base.begin_close(runtime)?;
                    *base_begun = true;
                    remaining -= 1;
                    if remaining == 0 {
                        return Ok(false);
                    }
                }
                base.poll_close(runtime, remaining)?
            }
            CandidateCleanup::ExactStreamAndRestore {
                stream,
                stream_begun,
                base,
                recovery: _,
            } => {
                let mut remaining = fuel;
                if base.is_none() {
                    *base = Some(Box::new(stream.take_exact_base_for_cancel(runtime)?));
                    remaining -= 1;
                    if remaining == 0 {
                        return Ok(false);
                    }
                }
                if !*stream_begun {
                    stream.begin_close(runtime)?;
                    *stream_begun = true;
                    remaining -= 1;
                    if remaining == 0 {
                        return Ok(false);
                    }
                }
                stream.poll_close(runtime, remaining)?
            }
        };
        if complete {
            match self.cleanup.take() {
                Some(CandidateCleanup::ExactStreamAndRestore {
                    base: Some(base),
                    recovery: Some(recovery),
                    ..
                }) => {
                    if self.retained.is_some() {
                        self.cleanup = Some(CandidateCleanup::RetainedPublication {
                            publication: base,
                            begun: false,
                        });
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    self.retained = Some(RetainedCandidateBase {
                        publication: base,
                        ack: recovery.ack,
                        restart: Some(recovery.restart),
                    });
                }
                Some(CandidateCleanup::ExactStreamAndRestore { .. }) => {
                    return Err(CandidateEndpointError::InvalidState);
                }
                _ => {}
            }
            if self.schedule_close_cleanup()? {
                return Ok(false);
            }
        }
        Ok(complete)
    }

    pub(crate) fn cleanup_pending(&self) -> bool {
        self.cleanup.is_some()
            || self.recursive_green.has_work()
            || matches!(
                self.hot_inline,
                Some(HotInlineState::Cancelling { .. } | HotInlineState::Releasing { .. })
            )
            || matches!(
                self.viewport_inline_batch,
                Some(ViewportInlineBatchState::Cancelling(_))
            )
            || (self.closing
                && (self.active.is_some()
                    || self.retained.is_some()
                    || self.recursive_green.has_installed_session()
                    || self.viewport_inline_batch.is_some()
                    || self.hot_inline.is_some()
                    || self.hot_inline_sidecar.is_some()))
    }

    /// Test evidence for the only architectural distinction this counter
    /// exists to make: a committed edit reused the persistent Green base, or
    /// it reached the definitive clean escape hatch. Initial clean delivery is
    /// deliberately excluded from both totals.
    pub(crate) const fn recursive_green_path_receipt(&self) -> RecursiveGreenPathReceipt {
        self.recursive_green.path_receipt()
    }

    pub(crate) fn has_poll_work(&self) -> bool {
        self.active.is_some()
            || self.cleanup_pending()
            || self.viewport_inline_batch_has_poll_work()
            || self.viewport_presentation_stream_has_poll_work()
            || self.pending_viewport_unavailable.is_some()
            || self.hot_inline_has_poll_work()
            || self.hot_inline_sidecar.is_some()
    }

    fn hot_inline_has_poll_work(&self) -> bool {
        matches!(
            self.hot_inline,
            Some(
                HotInlineState::AwaitingReferenceResolver(_)
                    | HotInlineState::Running(_)
                    | HotInlineState::Cancelling { .. }
                    | HotInlineState::Releasing { .. }
            )
        )
    }

    fn viewport_inline_batch_has_poll_work(&self) -> bool {
        matches!(
            self.viewport_inline_batch,
            Some(ViewportInlineBatchState::Running(_) | ViewportInlineBatchState::Cancelling(_))
        )
    }

    fn viewport_presentation_stream_has_poll_work(&self) -> bool {
        matches!(
            self.viewport_inline_batch,
            Some(ViewportInlineBatchState::Streaming(ref streaming))
                if matches!(
                    streaming.phase,
                    StreamPhase::NeedBegin
                        | StreamPhase::NeedPacket
                        | StreamPhase::NeedCommit
                )
        )
    }

    pub(crate) fn has_exact_base_for(
        &self,
        runtime: &DocumentRuntime,
        source: flark_engine::SourceVersion,
    ) -> Result<bool, CandidateEndpointError> {
        if self.closing || self.active.is_some() {
            return Ok(false);
        }
        let Some(retained) = self.retained.as_ref() else {
            return Ok(false);
        };
        let Some(restart) = retained.restart.as_ref() else {
            return Ok(false);
        };
        let descriptor = retained.publication.descriptor(runtime)?;
        Ok(restart.source() == source
            && restart.binding().grammar_revision() == GRAMMAR_REVISION
            && restart.binding().syntax_profile().get() == u64::from(descriptor.syntax_profile)
            && descriptor.source_revision == source.revision().get()
            && descriptor.source_root == source.root().get()
            && descriptor.source_bytes
                == u64::try_from(source.byte_len())
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?
            && descriptor.source_utf16
                == u64::try_from(source.utf16_len())
                    .map_err(|_| CandidateEndpointError::MetricOverflow)?)
    }

    #[cfg(test)]
    pub(crate) fn retained_ordinary_restart_receipt(
        &self,
    ) -> Option<(flark_engine::SourceVersion, usize)> {
        let retained = self.retained.as_ref()?;
        let CandidateRestartAuthority::Ordinary(checkpoints) = retained.restart.as_ref()? else {
            return None;
        };
        Some((checkpoints.source(), checkpoints.len()))
    }

    #[cfg(test)]
    pub(crate) fn active_phase_for_test(&self) -> &'static str {
        match self.active.as_ref() {
            Some(ActiveCandidate::Parsing(_)) => "Parsing",
            Some(ActiveCandidate::Building { .. }) => "Building",
            Some(ActiveCandidate::ParsingExact(_)) => "ParsingExact",
            Some(ActiveCandidate::ParsingOrdinaryExact(_)) => "ParsingOrdinaryExact",
            Some(ActiveCandidate::AwaitingRecursiveGreenExact(_)) => {
                "AwaitingRecursiveGreenExact"
            }
            Some(ActiveCandidate::ParsingBulletListLocal(_)) => "ParsingBulletListLocal",
            Some(ActiveCandidate::ParsingExactFallback(_)) => "ParsingExactFallback",
            Some(ActiveCandidate::BuildingExactFallback { .. }) => "BuildingExactFallback",
            Some(ActiveCandidate::BuildingExact { .. }) => "BuildingExact",
            Some(ActiveCandidate::Streaming(_)) => "Streaming",
            None if self.cleanup.is_some() => "Cleanup",
            None => "Idle",
        }
    }

    #[cfg(test)]
    pub(crate) fn has_bullet_list_local_edit_plan_for_test(&self) -> bool {
        self.bullet_list_local_edit.is_some()
    }

    /// Returns whether the retained exact base can cover this exact
    /// SourceFacts crop plan, not merely whether its publication is installed.
    ///
    /// Planning remains borrow-only. Move-only parser authority is consumed
    /// only after the SourceFacts delta has completed and its exact plan
    /// coordinates have been revalidated against the resulting witness.
    pub(crate) fn has_incremental_base_for_plan(
        &self,
        runtime: &DocumentRuntime,
        plan: &IncrementalSourceFactsPlan,
    ) -> Result<bool, CandidateEndpointError> {
        if !self.has_exact_base_for(runtime, plan.base())? {
            return Ok(false);
        }
        let target = runtime.snapshot_current_source()?;
        if target.version() != plan.source() {
            return Ok(false);
        }
        if let Some(local) = self.bullet_list_local_edit.as_ref() {
            if local.plan.source() != plan.base() {
                return Ok(false);
            }
            let prefix = match runtime.mint_exact_unchanged_prefix_witness(
                local.plan.source(),
                local.plan.prefix_witness_byte_end(),
                local.plan.prefix_witness_utf16_end(),
            ) {
                Ok(prefix) => prefix,
                Err(DocumentRuntimeError::ExactUnchangedPrefixLineageUnavailable) => {
                    return Ok(false);
                }
                Err(error) => return Err(error.into()),
            };
            let suffix = match runtime.mint_exact_unchanged_suffix_witness(
                local.plan.source(),
                local.plan.suffix_witness_byte_start(),
                local.plan.suffix_witness_utf16_start(),
            ) {
                Ok(suffix) => suffix,
                Err(DocumentRuntimeError::ExactUnchangedSuffixLineageUnavailable) => {
                    return Ok(false);
                }
                Err(error) => return Err(error.into()),
            };
            return Ok(prefix.target() == plan.source() && suffix.target() == plan.source());
        }
        let restart = self
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref())
            .ok_or(CandidateEndpointError::InvalidState)?;
        match restart {
            CandidateRestartAuthority::Leading(restart) => {
                match runtime.mint_exact_unchanged_prefix_witness(
                    restart.source(),
                    restart.prefix_end_byte() as usize,
                    restart.prefix_end_utf16() as usize,
                ) {
                    Ok(prefix) => Ok(prefix.target() == plan.source()
                        && target_physical_line_cut_is_exact(
                            &target,
                            prefix.byte_end(),
                            prefix.utf16_end(),
                        )?),
                    Err(DocumentRuntimeError::ExactUnchangedPrefixLineageUnavailable) => Ok(false),
                    Err(error) => Err(error.into()),
                }
            }
            CandidateRestartAuthority::Ordinary(restarts) => {
                let Some(route) = select_ordinary_crop_route(
                    restarts,
                    plan.exact_parser_base_byte_range()
                        .unwrap_or(plan.base_byte_range())
                        .clone(),
                )?
                else {
                    return Ok(false);
                };
                match route {
                    OrdinaryCropRoute::Interior(selection) => {
                        let prefix = match runtime.mint_exact_unchanged_prefix_witness(
                            selection.source(),
                            selection.restart_prefix_end_byte() as usize,
                            selection.restart_prefix_end_utf16() as usize,
                        ) {
                            Ok(prefix) => prefix,
                            Err(DocumentRuntimeError::ExactUnchangedPrefixLineageUnavailable) => {
                                return Ok(false);
                            }
                            Err(error) => return Err(error.into()),
                        };
                        let suffix = match runtime.mint_exact_unchanged_suffix_witness(
                            selection.source(),
                            selection.convergence_suffix_start_byte() as usize,
                            selection.convergence_suffix_start_utf16() as usize,
                        ) {
                            Ok(suffix) => suffix,
                            Err(DocumentRuntimeError::ExactUnchangedSuffixLineageUnavailable) => {
                                return Ok(false);
                            }
                            Err(error) => return Err(error.into()),
                        };
                        let restart_is_exact = target_physical_line_cut_is_exact(
                            &target,
                            prefix.byte_end(),
                            prefix.utf16_end(),
                        )?;
                        let convergence_is_exact = target_physical_line_cut_is_exact(
                            &target,
                            suffix.target_byte_start(),
                            suffix.target_utf16_start(),
                        )?;
                        Ok(prefix.target() == plan.source()
                            && suffix.target() == plan.source()
                            && prefix.byte_end() <= suffix.target_byte_start()
                            && prefix.utf16_end() <= suffix.target_utf16_start()
                            && restart_is_exact
                            && convergence_is_exact)
                    }
                    OrdinaryCropRoute::FromBof(selection) => {
                        let suffix = match runtime.mint_exact_unchanged_suffix_witness(
                            selection.source(),
                            selection.convergence_line_start_byte() as usize,
                            selection.convergence_line_start_utf16() as usize,
                        ) {
                            Ok(suffix) => suffix,
                            Err(DocumentRuntimeError::ExactUnchangedSuffixLineageUnavailable) => {
                                return Ok(false);
                            }
                            Err(error) => return Err(error.into()),
                        };
                        Ok(suffix.target() == plan.source()
                            && target_physical_line_cut_is_exact(
                                &target,
                                suffix.target_byte_start(),
                                suffix.target_utf16_start(),
                            )?)
                    }
                    OrdinaryCropRoute::ToEof(selection) => {
                        let prefix = match runtime.mint_exact_unchanged_prefix_witness(
                            selection.source(),
                            selection.restart_prefix_end_byte() as usize,
                            selection.restart_prefix_end_utf16() as usize,
                        ) {
                            Ok(prefix) => prefix,
                            Err(DocumentRuntimeError::ExactUnchangedPrefixLineageUnavailable) => {
                                return Ok(false);
                            }
                            Err(error) => return Err(error.into()),
                        };
                        let target_eof_is_exact = target
                            .utf16_offset_for_byte(plan.source().byte_len())
                            .map(|observed| observed == plan.source().utf16_len())
                            .and_then(|forward| {
                                target
                                    .byte_offset_for_utf16(plan.source().utf16_len())
                                    .map(|observed| forward && observed == plan.source().byte_len())
                            })
                            .map_err(|_| CandidateEndpointError::InvalidAuthority)?;
                        Ok(prefix.target() == plan.source()
                            && target_physical_line_cut_is_exact(
                                &target,
                                prefix.byte_end(),
                                prefix.utf16_end(),
                            )?
                            && target_eof_is_exact)
                    }
                }
            }
            CandidateRestartAuthority::RecursiveGreen { source, binding } => {
                let retained = self
                    .retained
                    .as_ref()
                    .ok_or(CandidateEndpointError::InvalidState)?;
                Ok(*source == plan.base()
                    && plan.source() == target.version()
                    && binding.grammar_revision() == GRAMMAR_REVISION
                    && self
                        .recursive_green
                        .owns_recursive_base_authority(retained.ack))
            }
            CandidateRestartAuthority::ExactBaseOnly { source, .. } => {
                Ok(*source == plan.base() && plan.source() == target.version())
            }
        }
    }

    pub(crate) fn begin_close(&mut self) -> Result<(), CandidateEndpointError> {
        self.closing = true;
        self.pending_viewport_unavailable = None;
        self.bullet_list_local_edit = None;
        let _ = self.schedule_close_cleanup()?;
        Ok(())
    }

    pub(crate) fn emergency_close(&mut self, runtime: &mut DocumentRuntime) {
        let _ = self.begin_close();
        while self.cleanup_pending() {
            if self.poll_cleanup(runtime, usize::MAX).is_err() {
                break;
            }
        }
    }

    fn schedule_close_cleanup(&mut self) -> Result<bool, CandidateEndpointError> {
        if !self.closing {
            return Ok(self.cleanup.is_some() || self.recursive_green.has_work());
        }
        self.recursive_green.begin_close()?;
        if self.cleanup.is_some() {
            return Ok(true);
        }
        self.cancel_hot_inline();
        if self.active.is_some() {
            self.cancel()?;
            if self.cleanup.is_some() {
                return Ok(true);
            }
        }
        if let Some(retained) = self.retained.take() {
            self.cleanup = Some(CandidateCleanup::RetainedPublication {
                publication: retained.publication,
                begun: false,
            });
        }
        Ok(self.cleanup.is_some() || self.recursive_green.has_work())
    }

    fn streaming_mut(&mut self) -> Result<&mut StreamingCandidate, CandidateEndpointError> {
        match self.active.as_mut() {
            Some(ActiveCandidate::Streaming(streaming)) => Ok(streaming),
            _ => Err(CandidateEndpointError::InvalidState),
        }
    }

    fn take_recursive_green_delivery_event(
        &mut self,
    ) -> Result<Option<CandidateEvent>, CandidateEndpointError> {
        let Some(ActiveCandidate::Streaming(streaming)) = self.active.as_mut() else {
            return Ok(None);
        };
        if streaming.phase != StreamPhase::AwaitRecursiveGreenDelivery {
            return Ok(None);
        }
        let ack = streaming
            .expected_ack
            .ok_or(CandidateEndpointError::InvalidState)?;
        if !self.recursive_green.ready_for(ack) {
            return Ok(None);
        }
        streaming.phase = StreamPhase::AwaitDeliveryReceipt;
        Ok(Some(CandidateEvent {
            credit: CandidateCredit::Delivery,
            body: CandidateEventBody::DeliveryAcknowledged(ack),
        }))
    }

    fn finish_delivery(&mut self) -> Result<(), CandidateEndpointError> {
        let Some(ActiveCandidate::Streaming(mut streaming)) = self.active.take() else {
            return Err(CandidateEndpointError::InvalidState);
        };
        if self.closing
            || streaming.phase != StreamPhase::AwaitDeliveryReceipt
            || self.cleanup.is_some()
            || streaming.stream.is_some()
        {
            self.active = Some(ActiveCandidate::Streaming(streaming));
            return Err(CandidateEndpointError::InvalidState);
        }
        let ack = streaming
            .expected_ack
            .ok_or(CandidateEndpointError::InvalidState)?;
        if self
            .retained
            .as_ref()
            .is_some_and(|retained| ack.host_revision <= retained.ack.host_revision)
            || !self.recursive_green.ready_for(ack)
        {
            self.active = Some(ActiveCandidate::Streaming(streaming));
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        if self.retained.is_some() && streaming.superseded_exact_base.is_some() {
            self.active = Some(ActiveCandidate::Streaming(streaming));
            return Err(CandidateEndpointError::InvalidState);
        }
        let publication = streaming
            .sealed_publication
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?;
        let mut publication = Box::new(publication);
        // A FullSnapshot fallback may still carry `superseded_exact_base`
        // solely so delivery can fuel-close the old publication. Only the
        // ExactBaseDelta stream setup authenticated equal canonical References
        // roots, so only that mode may transfer the winner-index handle.
        if streaming.offer.mode == PublicationMode::ExactBaseDelta {
            let Some(base) = streaming.superseded_exact_base.as_mut() else {
                streaming.sealed_publication = Some(*publication);
                self.active = Some(ActiveCandidate::Streaming(streaming));
                return Err(CandidateEndpointError::InvalidState);
            };
            if let Err(error) = publication.adopt_exact_base_reference_resolver(base) {
                streaming.sealed_publication = Some(*publication);
                self.active = Some(ActiveCandidate::Streaming(streaming));
                return Err(error.into());
            }
        }
        if let Err(error) = self.recursive_green.commit_delivery(ack) {
            streaming.sealed_publication = Some(*publication);
            self.active = Some(ActiveCandidate::Streaming(streaming));
            return Err(error);
        }
        self.bullet_list_local_edit = None;
        let previous = self.retained.replace(RetainedCandidateBase {
            publication,
            ack,
            restart: streaming.next_restart.take(),
        });
        match (previous, streaming.superseded_exact_base.take()) {
            (Some(previous), None) => {
                self.cleanup = Some(CandidateCleanup::RetainedPublication {
                    publication: previous.publication,
                    begun: false,
                });
            }
            (None, Some(previous)) => {
                self.cleanup = Some(CandidateCleanup::RetainedPublication {
                    publication: previous,
                    begun: false,
                });
            }
            (None, None) => {}
            (Some(previous), Some(exact_previous)) => {
                self.cleanup = Some(CandidateCleanup::RetainedPair {
                    target: previous.publication,
                    target_begun: false,
                    target_complete: false,
                    base: exact_previous,
                    base_begun: false,
                });
                return Err(CandidateEndpointError::InvalidState);
            }
        }
        drop(streaming);
        Ok(())
    }
}

impl PacketBuilder {
    fn can_accept(
        &self,
        frame_bytes: usize,
        maximum_packet_bytes: usize,
    ) -> Result<bool, CandidateEndpointError> {
        self.can_accept_with_frame_limit(
            frame_bytes,
            maximum_packet_bytes,
            M11_MAX_SNAPSHOT_FRAME_BYTES,
        )
    }

    fn can_accept_with_frame_limit(
        &self,
        frame_bytes: usize,
        maximum_packet_bytes: usize,
        maximum_frame_bytes: usize,
    ) -> Result<bool, CandidateEndpointError> {
        if frame_bytes == 0 || frame_bytes > maximum_frame_bytes {
            return Err(CandidateEndpointError::InvalidState);
        }
        if self.frames.len() >= MAXIMUM_PACKET_FRAME_COUNT as usize || self.end {
            return Ok(false);
        }
        let frame_count = self
            .frames
            .len()
            .checked_add(1)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let aggregate_frame_bytes = self
            .aggregate_frame_bytes
            .checked_add(frame_bytes)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let encoded_bytes = PACKET_HEADER_BYTES
            .checked_add(
                frame_count
                    .checked_mul(PACKET_FRAME_DESCRIPTOR_BYTES)
                    .ok_or(CandidateEndpointError::MetricOverflow)?,
            )
            .and_then(|bytes| bytes.checked_add(aggregate_frame_bytes))
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        Ok(
            aggregate_frame_bytes <= MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES as usize
                && encoded_bytes <= maximum_packet_bytes
                && encoded_bytes <= MAXIMUM_PACKET_ENCODED_BYTES,
        )
    }

    fn push(
        &mut self,
        ordinal: u32,
        first_record_ordinal: u32,
        record_count: u32,
        digest: [u32; 4],
        bytes: Box<[u8]>,
        end: bool,
    ) -> Result<(), CandidateEndpointError> {
        self.frames
            .try_reserve(1)
            .map_err(|_| CandidateEndpointError::AllocationFailed)?;
        if let Some(first_frame_ordinal) = self.first_frame_ordinal {
            let expected_ordinal = first_frame_ordinal
                .checked_add(
                    u32::try_from(self.frames.len())
                        .map_err(|_| CandidateEndpointError::MetricOverflow)?,
                )
                .ok_or(CandidateEndpointError::MetricOverflow)?;
            let expected_record_ordinal = self
                .first_record_ordinal
                .checked_add(self.aggregate_record_count)
                .ok_or(CandidateEndpointError::MetricOverflow)?;
            if ordinal != expected_ordinal || first_record_ordinal != expected_record_ordinal {
                return Err(CandidateEndpointError::InvalidState);
            }
        } else {
            self.first_frame_ordinal = Some(ordinal);
            self.first_record_ordinal = first_record_ordinal;
        }
        self.aggregate_record_count = self
            .aggregate_record_count
            .checked_add(record_count)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        self.aggregate_frame_bytes = self
            .aggregate_frame_bytes
            .checked_add(bytes.len())
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        self.frames.push(PacketFrame {
            record_count,
            digest,
            bytes,
        });
        self.end = end;
        Ok(())
    }

    fn encoded_len(&self) -> Result<usize, CandidateEndpointError> {
        PACKET_HEADER_BYTES
            .checked_add(
                self.frames
                    .len()
                    .checked_mul(PACKET_FRAME_DESCRIPTOR_BYTES)
                    .ok_or(CandidateEndpointError::MetricOverflow)?,
            )
            .and_then(|bytes| bytes.checked_add(self.aggregate_frame_bytes))
            .ok_or(CandidateEndpointError::MetricOverflow)
    }

    fn saturated(&self, maximum_packet_bytes: usize) -> Result<bool, CandidateEndpointError> {
        Ok(self.frames.len() == MAXIMUM_PACKET_FRAME_COUNT as usize
            || self.aggregate_frame_bytes == MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES as usize
            || self.encoded_len()? == maximum_packet_bytes
            || self.encoded_len()? == MAXIMUM_PACKET_ENCODED_BYTES)
    }

    fn encode(self, offer_id: [u32; 4]) -> Result<Vec<u8>, CandidateEndpointError> {
        let first_frame_ordinal = self
            .first_frame_ordinal
            .ok_or(CandidateEndpointError::InvalidState)?;
        if self.frames.is_empty() {
            return Err(CandidateEndpointError::InvalidState);
        }
        let mut inputs = Vec::new();
        inputs
            .try_reserve_exact(self.frames.len())
            .map_err(|_| CandidateEndpointError::AllocationFailed)?;
        inputs.extend(self.frames.iter().map(|frame| PublicationPacketFrameInput {
            record_count: frame.record_count,
            digest: frame.digest,
            bytes: &frame.bytes,
        }));
        let encoded_len = self.encoded_len()?;
        let mut encoded = Vec::new();
        encoded
            .try_reserve_exact(encoded_len)
            .map_err(|_| CandidateEndpointError::AllocationFailed)?;
        encoded.resize(encoded_len, 0);
        let written = encode_publication_packet_into(
            PublicationPacketInput {
                offer_id,
                first_frame_ordinal,
                first_record_ordinal: self.first_record_ordinal,
                frames: &inputs,
            },
            &mut encoded,
        )?;
        if written != encoded.len() {
            return Err(CandidateEndpointError::InvalidState);
        }
        Ok(encoded)
    }
}




fn resolved_inline_leaf(
    command: InlineRefinementCommand,
    fence: M11PublishedInlineLeafFence,
) -> ResolvedHotInlineDemand {
    let identity = HotInlineLeafIdentity::inline_leaf(&fence);
    let inline_source = fence.inline_source_range();
    let inline_source_utf16 = fence.inline_source_utf16_range();
    ResolvedHotInlineDemand::InlineLeaf {
        command,
        identity,
        inline_source,
        inline_source_utf16,
        fence,
    }
}

fn resolved_bullet_list_item(
    command: InlineRefinementCommand,
    fence: M11PublishedBulletListItemProjectionFence,
) -> ResolvedHotInlineDemand {
    let identity = HotInlineLeafIdentity::bullet_list_item(&fence);
    let parser_profile = fence.binding().syntax_profile();
    ResolvedHotInlineDemand::BulletListItem {
        command,
        identity,
        parser_profile,
        fence,
    }
}

fn resolved_ordered_list_item(
    command: InlineRefinementCommand,
    fence: M11PublishedOrderedListItemProjectionFence,
) -> ResolvedHotInlineDemand {
    let identity = HotInlineLeafIdentity::ordered_list_item(&fence);
    let parser_profile = fence.binding().syntax_profile();
    ResolvedHotInlineDemand::OrderedListItem {
        command,
        identity,
        parser_profile,
        fence,
    }
}

fn hot_inline_list_item_root(
    root: M11BlockQuoteProjectionRoot,
    selected_item_ordinal: u32,
    canonical_line_ending: M11HotInlineCanonicalLineEnding,
    ordered_item: Option<flark_parser::M11OrderedListItemProjectionMetadata>,
) -> HotInlineProjectionRoot {
    match ordered_item {
        Some(ordered) => HotInlineProjectionRoot::OrderedListItem {
            root,
            selected_item_ordinal,
            canonical_line_ending,
            opening_marker_start: ordered.opening_marker_start(),
            opening_marker_end: ordered.opening_marker_end(),
            marker_value: ordered.marker_value(),
        },
        None => HotInlineProjectionRoot::BulletListItem {
            root,
            selected_item_ordinal,
            canonical_line_ending,
        },
    }
}

fn list_item_projection_matches_target(
    target: InlineRefinementTarget,
    projection_kind: M11MarkedLineProjectionKind,
    has_ordered_item_metadata: bool,
) -> bool {
    match target {
        InlineRefinementTarget::BulletListItemProjection => {
            projection_kind == M11MarkedLineProjectionKind::BulletList && !has_ordered_item_metadata
        }
        InlineRefinementTarget::OrderedListItemProjection => {
            projection_kind == M11MarkedLineProjectionKind::OrderedList && has_ordered_item_metadata
        }
        InlineRefinementTarget::Automatic
        | InlineRefinementTarget::RecursiveGreenParagraph
        | InlineRefinementTarget::BulletListItemInline
        | InlineRefinementTarget::OrderedListItemInline => false,
    }
}

fn stage_resolved_hot_inline(
    runtime: &DocumentRuntime,
    resolved: ResolvedHotInlineDemand,
) -> Result<HotInlineState, CandidateEndpointError> {
    if matches!(
        resolved,
        ResolvedHotInlineDemand::InlineLeaf { .. }
            | ResolvedHotInlineDemand::PreparedInlineLeaf { .. }
    ) {
        Ok(HotInlineState::AwaitingReferenceResolver(Box::new(
            resolved,
        )))
    } else {
        start_resolved_hot_inline(runtime, resolved, None)
    }
}

fn start_resolved_hot_inline(
    runtime: &DocumentRuntime,
    resolved: ResolvedHotInlineDemand,
    reference_resolver: Option<M11ReferenceResolver>,
) -> Result<HotInlineState, CandidateEndpointError> {
    match resolved {
        ResolvedHotInlineDemand::InlineLeaf {
            command,
            identity,
            inline_source,
            inline_source_utf16,
            fence,
        } => {
            let parser_profile = fence.binding().syntax_profile();
            let reference_resolver =
                reference_resolver.ok_or(CandidateEndpointError::InvalidState)?;
            let job =
                M11InlineProjectionJob::new_for_published_inline_leaf_with_reference_resolver(
                    runtime,
                    fence,
                    reference_resolver,
                )?;
            Ok(HotInlineState::Running(Box::new(RunningHotInline {
                command,
                identity,
                inline_source,
                inline_source_utf16,
                parser_profile,
                job: RunningHotInlineJob::Inline(Box::new(job)),
            })))
        }
        ResolvedHotInlineDemand::PreparedInlineLeaf {
            command,
            identity,
            inline_source,
            inline_source_utf16,
            parser_profile,
            fence,
        } => {
            let reference_resolver =
                reference_resolver.ok_or(CandidateEndpointError::InvalidState)?;
            let job =
                M11InlineProjectionJob::new_for_recursive_green_inline_leaf_with_reference_resolver(
                    runtime,
                    fence,
                    M11ParserBinding::current(parser_profile),
                    reference_resolver,
                )?;
            Ok(HotInlineState::Running(Box::new(RunningHotInline {
                command,
                identity,
                inline_source,
                inline_source_utf16,
                parser_profile,
                job: RunningHotInlineJob::Inline(Box::new(job)),
            })))
        }
        ResolvedHotInlineDemand::IndentedCodeLeaf {
            command,
            identity,
            parser_profile,
            fence,
        } => {
            let inline_source = identity.inline_source_range();
            let inline_source_utf16 = identity.inline_source_utf16_range();
            let job = M11IndentedCodeProjectionJob::new(runtime, fence)?;
            Ok(HotInlineState::Running(Box::new(RunningHotInline {
                command,
                identity,
                inline_source,
                inline_source_utf16,
                parser_profile,
                job: RunningHotInlineJob::IndentedCode(Box::new(job)),
            })))
        }
        ResolvedHotInlineDemand::BlockQuoteLeaf {
            command,
            identity,
            parser_profile,
            fence,
        } => {
            let inline_source = identity.inline_source_range();
            let inline_source_utf16 = identity.inline_source_utf16_range();
            let job = M11BlockQuoteProjectionJob::new(runtime, fence)?;
            Ok(HotInlineState::Running(Box::new(RunningHotInline {
                command,
                identity,
                inline_source,
                inline_source_utf16,
                parser_profile,
                job: RunningHotInlineJob::BlockQuote(Box::new(job)),
            })))
        }
        ResolvedHotInlineDemand::BulletListLeaf {
            command,
            identity,
            parser_profile,
            fence,
        } => {
            let inline_source = identity.inline_source_range();
            let inline_source_utf16 = identity.inline_source_utf16_range();
            let job = M11BulletListProjectionJob::new(runtime, fence)?;
            Ok(HotInlineState::Running(Box::new(RunningHotInline {
                command,
                identity,
                inline_source,
                inline_source_utf16,
                parser_profile,
                job: RunningHotInlineJob::BulletList(Box::new(job)),
            })))
        }
        ResolvedHotInlineDemand::BulletListItem {
            command,
            identity,
            parser_profile,
            fence,
        } => {
            let inline_source = identity.inline_source_range();
            let inline_source_utf16 = identity.inline_source_utf16_range();
            let job = M11BulletListItemProjectionJob::new(runtime, fence)?;
            Ok(HotInlineState::Running(Box::new(RunningHotInline {
                command,
                identity,
                inline_source,
                inline_source_utf16,
                parser_profile,
                job: RunningHotInlineJob::BulletListItem(Box::new(job)),
            })))
        }
        ResolvedHotInlineDemand::OrderedListItem {
            command,
            identity,
            parser_profile,
            fence,
        } => {
            let inline_source = identity.inline_source_range();
            let inline_source_utf16 = identity.inline_source_utf16_range();
            let job = M11BulletListItemProjectionJob::new_ordered(runtime, fence)?;
            Ok(HotInlineState::Running(Box::new(RunningHotInline {
                command,
                identity,
                inline_source,
                inline_source_utf16,
                parser_profile,
                job: RunningHotInlineJob::BulletListItem(Box::new(job)),
            })))
        }
        ResolvedHotInlineDemand::Unsupported(ready) => Ok(HotInlineState::Ready(ready)),
    }
}


fn derive_identity(
    domain: &[u8],
    binding: SessionBinding,
    completion: SourceFactsCompletionEvent,
    parse_generation: u32,
) -> [u8; 16] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.v3.candidate.identity.v1\0");
    hasher.update(domain);
    hasher.update(&[0]);
    for word in binding.document_session {
        hasher.update(&word.to_le_bytes());
    }
    hasher.update(&binding.source_session_identity.to_le_bytes());
    hasher.update(&binding.worker_generation.to_le_bytes());
    hasher.update(&completion.ui_revision.to_le_bytes());
    hasher.update(&completion.worker_replica_revision.to_le_bytes());
    hasher.update(&completion.utf8_length.to_le_bytes());
    hasher.update(&completion.utf16_length.to_le_bytes());
    for word in completion.content_hash128 {
        hasher.update(&word.to_le_bytes());
    }
    hasher.update(&parse_generation.to_le_bytes());
    let mut identity = [0_u8; 16];
    identity.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    if identity == [0; 16] {
        identity[0] = 1;
    }
    identity
}

fn hot_inline_envelope_from_descriptor(
    descriptor: M11HotInlineSidecarDescriptor,
) -> HotInlineSidecarEnvelopeMetrics {
    let disposition = match descriptor.disposition() {
        M11HotInlineSidecarDisposition::Authoritative {
            logical_page_count,
            fact_count,
            storage_page_count,
            link_value_entry_count,
            link_value_storage_page_count,
            link_value_encoded_bytes,
            ordered_commitment256,
        } => HotInlineSidecarDisposition::Authoritative {
            logical_page_count,
            fact_count,
            storage_page_count,
            link_value_entry_count,
            link_value_encoded_bytes,
            link_value_storage_page_count,
            ordered_commitment256,
        },
        M11HotInlineSidecarDisposition::IndentedCodeAuthoritative {
            logical_page_count,
            line_count,
            storage_page_count,
            ordered_commitment256,
        }
        | M11HotInlineSidecarDisposition::BlockQuoteAuthoritative {
            logical_page_count,
            line_count,
            storage_page_count,
            ordered_commitment256,
        } => HotInlineSidecarDisposition::Authoritative {
            logical_page_count,
            fact_count: line_count,
            storage_page_count,
            link_value_entry_count: 0,
            link_value_encoded_bytes: 0,
            link_value_storage_page_count: 0,
            ordered_commitment256,
        },
        M11HotInlineSidecarDisposition::BulletListAuthoritative {
            logical_page_count,
            item_count,
            storage_page_count,
            ordered_commitment256,
            ..
        }
        | M11HotInlineSidecarDisposition::OrderedListAuthoritative {
            logical_page_count,
            item_count,
            storage_page_count,
            ordered_commitment256,
            ..
        } => HotInlineSidecarDisposition::Authoritative {
            logical_page_count,
            fact_count: item_count,
            storage_page_count,
            link_value_entry_count: 0,
            link_value_encoded_bytes: 0,
            link_value_storage_page_count: 0,
            ordered_commitment256,
        },
        M11HotInlineSidecarDisposition::Unsupported {
            reason,
            metadata_commitment256,
        } => HotInlineSidecarDisposition::Unsupported {
            reason,
            metadata_commitment256,
        },
    };
    HotInlineSidecarEnvelopeMetrics {
        hio1_encoded_bytes: descriptor.hio1_encoded_bytes(),
        ipr2_descriptor_bytes: descriptor.ipr2_descriptor_bytes(),
        transferred_node_count: descriptor.transferred_node_count(),
        hio1_envelope_digest256: descriptor.hio1_envelope_digest256(),
        disposition,
    }
}

fn hot_inline_wire_binding(binding: &M11HotInlineSidecarBinding) -> HotInlineSidecarBinding {
    let owner = match binding.owner() {
        M11HotInlineSidecarOwner::BlockOrdinal(ordinal) => {
            HotInlineSidecarOwner::BlockOrdinal(ordinal)
        }
        M11HotInlineSidecarOwner::RecursiveGreenFrame(frame) => {
            HotInlineSidecarOwner::RecursiveGreenFrame(frame.get())
        }
    };
    HotInlineSidecarBinding {
        parser_profile: binding.parser_profile().get(),
        refinement_generation: binding.refinement_generation(),
        block_ordinal: owner
            .into_wire()
            .expect("engine-owned sidecar identity fits the wire"),
        physical_start_utf8: binding.physical_range().start,
        physical_end_utf8: binding.physical_range().end,
        visible_start_utf8: binding.visible_range().start,
        visible_end_utf8: binding.visible_range().end,
        physical_start_utf16: binding.physical_range_utf16().start,
        physical_end_utf16: binding.physical_range_utf16().end,
        visible_start_utf16: binding.visible_range_utf16().start,
        visible_end_utf16: binding.visible_range_utf16().end,
    }
}



fn hash_source_version(hasher: &mut blake3::Hasher, source: SourceVersion) {
    for word in source.document_session {
        hasher.update(&word.to_le_bytes());
    }
    hasher.update(&source.revision.to_le_bytes());
    hasher.update(&source.utf8_length.to_le_bytes());
    hasher.update(&source.utf16_length.to_le_bytes());
    for word in source.content_hash128 {
        hasher.update(&word.to_le_bytes());
    }
}

fn hash_structural_ack(hasher: &mut blake3::Hasher, ack: StructuralAck) {
    for word in ack.publication_session {
        hasher.update(&word.to_le_bytes());
    }
    hasher.update(&ack.host_revision.to_le_bytes());
    hash_source_version(hasher, ack.source_version);
    for word in ack.source_root {
        hasher.update(&word.to_le_bytes());
    }
    hasher.update(&ack.parse_generation.to_le_bytes());
    hasher.update(&ack.grammar_revision.to_le_bytes());
    hasher.update(&ack.syntax_profile.to_le_bytes());
    hasher.update(&ack.authority_mask.to_le_bytes());
    hasher.update(&ack.record_count.to_le_bytes());
    for word in ack.sequence_digest {
        hasher.update(&word.to_le_bytes());
    }
    for word in ack.manifest_digest {
        hasher.update(&word.to_le_bytes());
    }
}

fn document_bytes(document: [u32; 4]) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    for (index, word) in document.into_iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn digest_words(bytes: [u8; 16]) -> [u32; 4] {
    std::array::from_fn(|index| {
        u32::from_le_bytes(
            bytes[index * 4..index * 4 + 4]
                .try_into()
                .expect("four-byte identity lane"),
        )
    })
}

const fn split_u64(value: u64) -> [u32; 2] {
    [(value >> 32) as u32, value as u32]
}

fn checked_add(left: usize, right: usize) -> Result<usize, CandidateEndpointError> {
    left.checked_add(right)
        .ok_or(CandidateEndpointError::MetricOverflow)
}

#[cfg(test)]
#[path = "v3_candidate_endpoint_tests.rs"]
mod tests;

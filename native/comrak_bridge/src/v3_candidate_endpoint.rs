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
    M11RecursiveGreenFrameId, M11RecursiveGreenPoint, M11RecursiveGreenRowEditCapability,
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
/// Maximum base or target semantic entries inspected atomically while
/// deciding whether an exact-clean fallback can reuse packed block pages.
///
/// Larger affected regions remain correct by taking the existing full
/// publication fallback; general restart/convergence will make discovery
/// itself incremental in a later cut.
const EXACT_CLEAN_BLOCK_SPLICE_MAX_AFFECTED_ENTRIES: u64 = 256;

#[derive(Debug)]
pub(crate) enum CandidateEndpointError {
    Busy,
    InvalidState,
    InvalidAuthority,
    MetricOverflow,
    AllocationFailed,
    Parse(M11CleanParseJobError),
    Crop(M11LeadingReferencesCropError),
    OrdinaryCrop(M11OrdinaryParagraphCropError),
    OrdinaryBoundaryCrop(M11OrdinaryParagraphBoundaryCropError),
    Document(DocumentRuntimeError),
    Derive(M11CandidateDerivationError),
    InlinePublication(M11InlinePublicationError),
    InlineRange(M11PublishedInlineRangeError),
    InlineProjectionJob(M11InlineProjectionJobError),
    InlineProjection(M11InlineProjectionError),
    RecursiveGreenParagraph(M11RecursiveGreenParagraphPreparationError),
    PersistentRecursiveGreen(M11PersistentRecursiveGreenSessionError),
    IndentedCodeProjectionJob(M11IndentedCodeProjectionJobError),
    IndentedCodeProjection(M11IndentedCodeProjectionError),
    BlockQuoteProjectionJob(M11BlockQuoteProjectionJobError),
    BlockQuoteProjection(M11BlockQuoteProjectionError),
    BulletListLocal(M11BulletListLocalDeltaError),
    ParserPage(M11ParserPageError),
    Publication(M11PublicationError),
    Transport(CandidateTransportDigestError),
    ViewportTransport(ViewportPresentationTransportDigestError),
    Wire(EncodeError),
    ViewportInlineLimitExceeded(&'static str),
}

impl fmt::Display for CandidateEndpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Flark v3 candidate endpoint failure: ")?;
        match self {
            Self::Busy => formatter.write_str("busy"),
            Self::InvalidState => formatter.write_str("invalid state"),
            Self::InvalidAuthority => formatter.write_str("invalid authority"),
            Self::MetricOverflow => formatter.write_str("metric overflow"),
            Self::AllocationFailed => formatter.write_str("bounded allocation failed"),
            Self::Parse(error) => error.fmt(formatter),
            Self::Crop(error) => error.fmt(formatter),
            Self::OrdinaryCrop(error) => error.fmt(formatter),
            Self::OrdinaryBoundaryCrop(error) => error.fmt(formatter),
            Self::Document(error) => error.fmt(formatter),
            Self::Derive(error) => error.fmt(formatter),
            Self::InlinePublication(error) => error.fmt(formatter),
            Self::InlineRange(error) => error.fmt(formatter),
            Self::InlineProjectionJob(error) => error.fmt(formatter),
            Self::InlineProjection(error) => error.fmt(formatter),
            Self::RecursiveGreenParagraph(error) => error.fmt(formatter),
            Self::PersistentRecursiveGreen(error) => error.fmt(formatter),
            Self::IndentedCodeProjectionJob(error) => error.fmt(formatter),
            Self::IndentedCodeProjection(error) => error.fmt(formatter),
            Self::BlockQuoteProjectionJob(error) => error.fmt(formatter),
            Self::BlockQuoteProjection(error) => error.fmt(formatter),
            Self::BulletListLocal(error) => write!(formatter, "{error:?}"),
            Self::ParserPage(error) => error.fmt(formatter),
            Self::Publication(error) => error.fmt(formatter),
            Self::Transport(error) => write!(formatter, "{error:?}"),
            Self::ViewportTransport(error) => write!(formatter, "{error:?}"),
            Self::Wire(error) => error.fmt(formatter),
            Self::ViewportInlineLimitExceeded(limit) => {
                write!(formatter, "viewport inline batch exceeded {limit}")
            }
        }
    }
}

impl std::error::Error for CandidateEndpointError {}

impl From<M11CleanParseJobError> for CandidateEndpointError {
    fn from(error: M11CleanParseJobError) -> Self {
        Self::Parse(error)
    }
}

impl From<M11LeadingReferencesCropError> for CandidateEndpointError {
    fn from(error: M11LeadingReferencesCropError) -> Self {
        Self::Crop(error)
    }
}

impl From<M11OrdinaryParagraphCropError> for CandidateEndpointError {
    fn from(error: M11OrdinaryParagraphCropError) -> Self {
        Self::OrdinaryCrop(error)
    }
}

impl From<M11OrdinaryParagraphBoundaryCropError> for CandidateEndpointError {
    fn from(error: M11OrdinaryParagraphBoundaryCropError) -> Self {
        Self::OrdinaryBoundaryCrop(error)
    }
}

impl From<DocumentRuntimeError> for CandidateEndpointError {
    fn from(error: DocumentRuntimeError) -> Self {
        Self::Document(error)
    }
}

impl From<M11CandidateDerivationError> for CandidateEndpointError {
    fn from(error: M11CandidateDerivationError) -> Self {
        Self::Derive(error)
    }
}

impl From<M11InlinePublicationError> for CandidateEndpointError {
    fn from(error: M11InlinePublicationError) -> Self {
        Self::InlinePublication(error)
    }
}

impl From<M11PublishedInlineRangeError> for CandidateEndpointError {
    fn from(error: M11PublishedInlineRangeError) -> Self {
        Self::InlineRange(error)
    }
}

impl From<M11InlineProjectionJobError> for CandidateEndpointError {
    fn from(error: M11InlineProjectionJobError) -> Self {
        Self::InlineProjectionJob(error)
    }
}

impl From<M11InlineProjectionError> for CandidateEndpointError {
    fn from(error: M11InlineProjectionError) -> Self {
        Self::InlineProjection(error)
    }
}

impl From<M11RecursiveGreenParagraphPreparationError> for CandidateEndpointError {
    fn from(error: M11RecursiveGreenParagraphPreparationError) -> Self {
        Self::RecursiveGreenParagraph(error)
    }
}

impl From<M11PersistentRecursiveGreenSessionError> for CandidateEndpointError {
    fn from(error: M11PersistentRecursiveGreenSessionError) -> Self {
        Self::PersistentRecursiveGreen(error)
    }
}

impl From<M11IndentedCodeProjectionJobError> for CandidateEndpointError {
    fn from(error: M11IndentedCodeProjectionJobError) -> Self {
        Self::IndentedCodeProjectionJob(error)
    }
}

impl From<M11IndentedCodeProjectionError> for CandidateEndpointError {
    fn from(error: M11IndentedCodeProjectionError) -> Self {
        Self::IndentedCodeProjection(error)
    }
}

impl From<M11BlockQuoteProjectionJobError> for CandidateEndpointError {
    fn from(error: M11BlockQuoteProjectionJobError) -> Self {
        Self::BlockQuoteProjectionJob(error)
    }
}

impl From<M11BlockQuoteProjectionError> for CandidateEndpointError {
    fn from(error: M11BlockQuoteProjectionError) -> Self {
        Self::BlockQuoteProjection(error)
    }
}

impl From<M11BulletListLocalDeltaError> for CandidateEndpointError {
    fn from(error: M11BulletListLocalDeltaError) -> Self {
        Self::BulletListLocal(error)
    }
}

impl From<M11ParserPageError> for CandidateEndpointError {
    fn from(error: M11ParserPageError) -> Self {
        Self::ParserPage(error)
    }
}

impl From<M11PublicationError> for CandidateEndpointError {
    fn from(error: M11PublicationError) -> Self {
        Self::Publication(error)
    }
}

impl From<CandidateTransportDigestError> for CandidateEndpointError {
    fn from(error: CandidateTransportDigestError) -> Self {
        Self::Transport(error)
    }
}

impl From<ViewportPresentationTransportDigestError> for CandidateEndpointError {
    fn from(error: ViewportPresentationTransportDigestError) -> Self {
        Self::ViewportTransport(error)
    }
}

impl From<EncodeError> for CandidateEndpointError {
    fn from(error: EncodeError) -> Self {
        Self::Wire(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CandidateCredit {
    Begin,
    Packet {
        first_frame_ordinal: u32,
        frame_count: u32,
        end: bool,
    },
    Commit,
    Delivery,
}

pub(crate) enum CandidateEventBody {
    Begin(OfferBegin),
    Packet { encoded: Vec<u8> },
    Commit(CommitRequest),
    DeliveryAcknowledged(StructuralAck),
}

impl CandidateEventBody {
    pub(crate) fn borrowed(&self) -> Result<PublicationEventBody<'_>, DecodeError> {
        Ok(match self {
            Self::Begin(begin) => PublicationEventBody::Begin(*begin),
            Self::Packet { encoded } => {
                PublicationEventBody::Packet(decode_publication_packet(encoded)?)
            }
            Self::Commit(commit) => PublicationEventBody::Commit(*commit),
            Self::DeliveryAcknowledged(ack) => PublicationEventBody::DeliveryAcknowledged(*ack),
        })
    }
}

pub(crate) struct CandidateEvent {
    pub(crate) credit: CandidateCredit,
    pub(crate) body: CandidateEventBody,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HotInlineCredit {
    Begin,
    Packet {
        first_frame_ordinal: u32,
        frame_count: u32,
        end: bool,
    },
    Commit,
    Delivery,
}

pub(crate) enum HotInlineEventBody {
    Begin(HotInlineSidecarBegin),
    Packet { encoded: Vec<u8> },
    Commit(HotInlineSidecarCommitRequest),
    DeliveryAcknowledged(InlineSidecarAck),
}

impl HotInlineEventBody {
    pub(crate) fn borrowed(&self) -> Result<HotInlineSidecarEventBody<'_>, DecodeError> {
        Ok(match self {
            Self::Begin(begin) => HotInlineSidecarEventBody::Begin(*begin),
            Self::Packet { encoded } => {
                HotInlineSidecarEventBody::Packet(decode_publication_packet(encoded)?)
            }
            Self::Commit(commit) => HotInlineSidecarEventBody::Commit(*commit),
            Self::DeliveryAcknowledged(ack) => {
                HotInlineSidecarEventBody::DeliveryAcknowledged(*ack)
            }
        })
    }
}

pub(crate) struct HotInlineEvent {
    pub(crate) credit: HotInlineCredit,
    pub(crate) body: HotInlineEventBody,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ViewportPresentationCredit {
    Begin,
    Packet {
        first_frame_ordinal: u32,
        frame_count: u32,
        end: bool,
    },
    Commit,
    Delivery,
}

pub(crate) enum CandidateViewportPresentationEventBody {
    Begin(ViewportPresentationBegin),
    Packet { encoded: Vec<u8> },
    Commit(ViewportPresentationCommitRequest),
    DeliveryAcknowledged(ViewportPresentationAck),
}

impl CandidateViewportPresentationEventBody {
    pub(crate) fn borrowed(&self) -> Result<ViewportPresentationEventBody<'_>, DecodeError> {
        Ok(match self {
            Self::Begin(begin) => ViewportPresentationEventBody::Begin(*begin),
            Self::Packet { encoded } => {
                ViewportPresentationEventBody::Packet(decode_publication_packet(encoded)?)
            }
            Self::Commit(commit) => ViewportPresentationEventBody::Commit(*commit),
            Self::DeliveryAcknowledged(ack) => {
                ViewportPresentationEventBody::DeliveryAcknowledged(*ack)
            }
        })
    }
}

pub(crate) struct CandidateViewportPresentationEvent {
    pub(crate) credit: ViewportPresentationCredit,
    pub(crate) body: CandidateViewportPresentationEventBody,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ViewportPresentationUnavailableReason {
    BudgetExceeded,
    DerivationFailed,
}

pub(crate) enum CandidatePoll {
    Pending {
        transitions: usize,
    },
    Event {
        transitions: usize,
        event: Box<CandidateEvent>,
    },
    HotInlineEvent {
        transitions: usize,
        event: Box<HotInlineEvent>,
    },
    ViewportPresentationEvent {
        transitions: usize,
        event: Box<CandidateViewportPresentationEvent>,
    },
    ViewportPresentationUnavailable {
        transitions: usize,
        viewport_generation: u32,
        reason: ViewportPresentationUnavailableReason,
    },
}

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
            Self::ExactBaseOnly { source, .. } => *source,
        }
    }

    const fn binding(&self) -> M11ParserBinding {
        match self {
            Self::Leading(restart) => restart.binding(),
            Self::Ordinary(restarts) => restarts.binding(),
            Self::ExactBaseOnly { binding, .. } => *binding,
        }
    }
}

struct ExactCandidateBase {
    publication: Box<M11RetainedCandidatePublication>,
    ack: StructuralAck,
    restart: Option<CandidateRestartAuthority>,
}

struct InstalledRecursiveGreen {
    ack: StructuralAck,
    session: M11PersistentRecursiveGreenSession,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecursiveGreenCleanOrigin {
    Initial,
    IncrementalFallback,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RecursiveGreenPathReceipt {
    pub(crate) local_adoption_deliveries: u64,
    pub(crate) clean_fallback_deliveries: u64,
}

enum PendingRecursiveGreen {
    CleanPlan {
        plan: Option<M11PersistentRecursiveGreenCleanPlan>,
        base: Option<InstalledRecursiveGreen>,
        origin: RecursiveGreenCleanOrigin,
    },
    CleanBuild {
        build: M11PersistentRecursiveGreenCleanBuild,
        base: Option<InstalledRecursiveGreen>,
        origin: RecursiveGreenCleanOrigin,
    },
    Adoption {
        base_ack: StructuralAck,
        adoption: M11PersistentRecursiveGreenAdoption,
    },
    CancellingAdoptionForFallback {
        base_ack: StructuralAck,
        syntax_profile: u32,
        adoption: M11PersistentRecursiveGreenAdoption,
        begun: bool,
    },
    ReadyClean {
        target: M11PersistentRecursiveGreenSession,
        base: Option<InstalledRecursiveGreen>,
        origin: RecursiveGreenCleanOrigin,
    },
    ReadyUpdate {
        base_ack: StructuralAck,
        update: M11PersistentRecursiveGreenUpdate,
    },
}

enum RecursiveGreenCleanup {
    Session {
        session: M11PersistentRecursiveGreenSession,
        begun: bool,
    },
    CleanBuild {
        build: M11PersistentRecursiveGreenCleanBuild,
        restore: Option<InstalledRecursiveGreen>,
        begun: bool,
    },
    Adoption {
        adoption: M11PersistentRecursiveGreenAdoption,
        restore_ack: StructuralAck,
        begun: bool,
    },
}

/// Endpoint-owned recursive-Green/reference authority. The candidate's flat
/// publication remains a sidecar and cross-check; it never owns the ranges
/// returned for the recursive-Green inline-leaf target.
struct RecursiveGreenEndpointSlot {
    installed: Option<InstalledRecursiveGreen>,
    pending: Option<PendingRecursiveGreen>,
    cleanup: VecDeque<RecursiveGreenCleanup>,
    path_receipt: RecursiveGreenPathReceipt,
}

impl RecursiveGreenEndpointSlot {
    const fn new() -> Self {
        Self {
            installed: None,
            pending: None,
            cleanup: VecDeque::new(),
            path_receipt: RecursiveGreenPathReceipt {
                local_adoption_deliveries: 0,
                clean_fallback_deliveries: 0,
            },
        }
    }

    fn start_clean(
        &mut self,
        plan: M11PersistentRecursiveGreenCleanPlan,
    ) -> Result<(), CandidateEndpointError> {
        if self.pending.is_some() || (!self.cleanup.is_empty() && self.installed.is_none()) {
            return Err(CandidateEndpointError::Busy);
        }
        self.pending = Some(PendingRecursiveGreen::CleanPlan {
            plan: Some(plan),
            origin: if self.installed.is_some() {
                RecursiveGreenCleanOrigin::IncrementalFallback
            } else {
                RecursiveGreenCleanOrigin::Initial
            },
            base: self.installed.take(),
        });
        Ok(())
    }

    fn start_incremental(
        &mut self,
        runtime: &DocumentRuntime,
        base_ack: StructuralAck,
        base_edit: Range<usize>,
        syntax_profile: u32,
    ) -> Result<(), CandidateEndpointError> {
        if self.pending.is_some() || (!self.cleanup.is_empty() && self.installed.is_none()) {
            return Err(CandidateEndpointError::Busy);
        }
        let target_plan = || -> Result<_, CandidateEndpointError> {
            Ok(M11PersistentRecursiveGreenCleanPlan::new(
                runtime.snapshot_current_source()?,
                runtime.snapshot_current_source()?,
                syntax_profile,
            )?)
        };
        let Some(installed) = self.installed.as_ref() else {
            self.pending = Some(PendingRecursiveGreen::CleanPlan {
                plan: Some(target_plan()?),
                base: None,
                origin: RecursiveGreenCleanOrigin::IncrementalFallback,
            });
            return Ok(());
        };
        if installed.ack != base_ack
            || installed.session.source().revision().get()
                != u64::from(base_ack.source_version.revision)
            || installed.session.syntax_profile() != syntax_profile
        {
            let plan = target_plan()?;
            let installed = self
                .installed
                .take()
                .ok_or(CandidateEndpointError::InvalidState)?;
            self.pending = Some(PendingRecursiveGreen::CleanPlan {
                plan: Some(plan),
                base: Some(installed),
                origin: RecursiveGreenCleanOrigin::IncrementalFallback,
            });
            return Ok(());
        }
        let target_lease = runtime.snapshot_current_source()?;
        let installed = self
            .installed
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?;
        match installed
            .session
            .begin_local_adoption(runtime, target_lease, base_edit)
        {
            Ok(adoption) => {
                self.pending = Some(PendingRecursiveGreen::Adoption { base_ack, adoption });
            }
            Err(failure) => {
                let base = InstalledRecursiveGreen {
                    ack: base_ack,
                    session: failure.into_base(),
                };
                let plan = match target_plan() {
                    Ok(plan) => plan,
                    Err(error) => {
                        self.installed = Some(base);
                        return Err(error);
                    }
                };
                self.pending = Some(PendingRecursiveGreen::CleanPlan {
                    plan: Some(plan),
                    base: Some(base),
                    origin: RecursiveGreenCleanOrigin::IncrementalFallback,
                });
            }
        }
        Ok(())
    }

    const fn target_work_pending(&self) -> bool {
        self.pending.is_some()
            && !matches!(
                self.pending.as_ref(),
                Some(
                    PendingRecursiveGreen::ReadyClean { .. }
                        | PendingRecursiveGreen::ReadyUpdate { .. }
                )
            )
    }

    fn cleanup_pending(&self) -> bool {
        !self.cleanup.is_empty()
    }

    fn has_work(&self) -> bool {
        self.pending.is_some() || !self.cleanup.is_empty()
    }

    fn poll_target(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<usize, CandidateEndpointError> {
        if fuel == 0 || !self.target_work_pending() {
            return Ok(0);
        }
        let pending = self
            .pending
            .as_mut()
            .ok_or(CandidateEndpointError::InvalidState)?;
        match pending {
            PendingRecursiveGreen::CleanPlan { plan, .. } => {
                let plan = plan.take().ok_or(CandidateEndpointError::InvalidState)?;
                let build = plan.begin(runtime)?;
                let (base, origin) = match self
                    .pending
                    .take()
                    .ok_or(CandidateEndpointError::InvalidState)?
                {
                    PendingRecursiveGreen::CleanPlan { base, origin, .. } => (base, origin),
                    _ => return Err(CandidateEndpointError::InvalidState),
                };
                self.pending = Some(PendingRecursiveGreen::CleanBuild {
                    build,
                    base,
                    origin,
                });
                Ok(1)
            }
            PendingRecursiveGreen::CleanBuild { build, .. } => {
                let poll = build.poll(runtime, fuel)?;
                let transitions = poll.transitions();
                if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
                    let target = build
                        .take_session()
                        .ok_or(CandidateEndpointError::InvalidState)?;
                    let (base, origin) = match self
                        .pending
                        .take()
                        .ok_or(CandidateEndpointError::InvalidState)?
                    {
                        PendingRecursiveGreen::CleanBuild { base, origin, .. } => (base, origin),
                        _ => return Err(CandidateEndpointError::InvalidState),
                    };
                    self.pending = Some(PendingRecursiveGreen::ReadyClean {
                        target,
                        base,
                        origin,
                    });
                }
                Ok(transitions)
            }
            PendingRecursiveGreen::Adoption { base_ack, adoption } => {
                let poll = adoption.poll(runtime, fuel)?;
                let transitions = poll.transitions();
                match poll.status() {
                    M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
                    M11PersistentRecursiveGreenAdoptionStatus::Complete => {
                        let update = adoption
                            .take_update()
                            .ok_or(CandidateEndpointError::InvalidState)?;
                        self.pending = Some(PendingRecursiveGreen::ReadyUpdate {
                            base_ack: *base_ack,
                            update,
                        });
                    }
                    M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                        let base_ack = *base_ack;
                        let syntax_profile = u32::try_from(base_ack.syntax_profile)
                            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
                        let adoption = match self
                            .pending
                            .take()
                            .ok_or(CandidateEndpointError::InvalidState)?
                        {
                            PendingRecursiveGreen::Adoption { adoption, .. } => adoption,
                            _ => return Err(CandidateEndpointError::InvalidState),
                        };
                        self.pending = Some(PendingRecursiveGreen::CancellingAdoptionForFallback {
                            base_ack,
                            syntax_profile,
                            adoption,
                            begun: false,
                        });
                    }
                    M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                }
                Ok(transitions)
            }
            PendingRecursiveGreen::CancellingAdoptionForFallback {
                base_ack,
                syntax_profile,
                adoption,
                begun,
            } => {
                let mut transitions = 0;
                if !*begun {
                    adoption.begin_cancel(runtime)?;
                    *begun = true;
                    transitions = 1;
                    if transitions == fuel {
                        return Ok(transitions);
                    }
                }
                if adoption.poll_cancel(runtime, fuel - transitions)? {
                    let plan = M11PersistentRecursiveGreenCleanPlan::new(
                        runtime.snapshot_current_source()?,
                        runtime.snapshot_current_source()?,
                        *syntax_profile,
                    )?;
                    let base = adoption
                        .take_base_after_cancel()
                        .ok_or(CandidateEndpointError::InvalidState)?;
                    let base_ack = *base_ack;
                    self.pending = Some(PendingRecursiveGreen::CleanPlan {
                        plan: Some(plan),
                        base: Some(InstalledRecursiveGreen {
                            ack: base_ack,
                            session: base,
                        }),
                        origin: RecursiveGreenCleanOrigin::IncrementalFallback,
                    });
                }
                Ok(fuel)
            }
            PendingRecursiveGreen::ReadyClean { .. }
            | PendingRecursiveGreen::ReadyUpdate { .. } => Ok(0),
        }
    }

    fn ready_for(&self, ack: StructuralAck) -> bool {
        match self.pending.as_ref() {
            Some(PendingRecursiveGreen::ReadyClean { target, .. }) => {
                recursive_green_session_matches_ack(target, ack)
            }
            Some(PendingRecursiveGreen::ReadyUpdate { update, .. }) => {
                recursive_green_session_matches_ack(update.target_session(), ack)
            }
            _ => false,
        }
    }

    fn initial_clean_ready_session(
        &self,
        source: flark_engine::SourceVersion,
        syntax_profile: u32,
    ) -> Result<&M11PersistentRecursiveGreenSession, CandidateEndpointError> {
        match self.pending.as_ref() {
            Some(PendingRecursiveGreen::ReadyClean {
                target,
                base: None,
                origin: RecursiveGreenCleanOrigin::Initial,
            }) if target.source() == source && target.syntax_profile() == syntax_profile => {
                Ok(target)
            }
            Some(PendingRecursiveGreen::ReadyClean { .. }) => {
                Err(CandidateEndpointError::InvalidAuthority)
            }
            _ => Err(CandidateEndpointError::InvalidState),
        }
    }

    fn incremental_clean_ready_session(
        &self,
        source: flark_engine::SourceVersion,
        syntax_profile: u32,
    ) -> Option<&M11PersistentRecursiveGreenSession> {
        match self.pending.as_ref() {
            Some(PendingRecursiveGreen::ReadyClean {
                target,
                origin: RecursiveGreenCleanOrigin::IncrementalFallback,
                ..
            }) if target.source() == source && target.syntax_profile() == syntax_profile => {
                Some(target)
            }
            _ => None,
        }
    }

    fn ready_update_for(
        &self,
        base_ack: StructuralAck,
        target: flark_engine::SourceVersion,
    ) -> Option<&M11PersistentRecursiveGreenUpdate> {
        match self.pending.as_ref() {
            Some(PendingRecursiveGreen::ReadyUpdate {
                base_ack: ready_base,
                update,
            }) if *ready_base == base_ack && update.target_source() == target => Some(update),
            _ => None,
        }
    }

    fn commit_delivery(&mut self, ack: StructuralAck) -> Result<(), CandidateEndpointError> {
        if !self.ready_for(ack) || self.installed.is_some() {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        self.cleanup
            .try_reserve(1)
            .map_err(|_| CandidateEndpointError::AllocationFailed)?;
        let ready = self
            .pending
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?;
        let (target, base, clean_origin) = match ready {
            PendingRecursiveGreen::ReadyClean {
                target,
                base,
                origin,
            } => (
                target,
                base.map(|installed| installed.session),
                Some(origin),
            ),
            PendingRecursiveGreen::ReadyUpdate {
                base_ack,
                mut update,
            } => {
                if base_ack.host_revision >= ack.host_revision {
                    self.pending = Some(PendingRecursiveGreen::ReadyUpdate { base_ack, update });
                    return Err(CandidateEndpointError::InvalidAuthority);
                }
                let Some(target) = update.take_target() else {
                    self.pending = Some(PendingRecursiveGreen::ReadyUpdate { base_ack, update });
                    return Err(CandidateEndpointError::InvalidState);
                };
                let Some(base) = update.take_base() else {
                    self.cleanup.push_back(RecursiveGreenCleanup::Session {
                        session: target,
                        begun: false,
                    });
                    return Err(CandidateEndpointError::InvalidState);
                };
                (target, Some(base), None)
            }
            other => {
                self.pending = Some(other);
                return Err(CandidateEndpointError::InvalidState);
            }
        };
        if let Some(session) = base {
            self.cleanup.push_back(RecursiveGreenCleanup::Session {
                session,
                begun: false,
            });
        }
        self.installed = Some(InstalledRecursiveGreen {
            ack,
            session: target,
        });
        match clean_origin {
            None => {
                self.path_receipt.local_adoption_deliveries = self
                    .path_receipt
                    .local_adoption_deliveries
                    .saturating_add(1);
            }
            Some(RecursiveGreenCleanOrigin::IncrementalFallback) => {
                self.path_receipt.clean_fallback_deliveries = self
                    .path_receipt
                    .clean_fallback_deliveries
                    .saturating_add(1);
            }
            Some(RecursiveGreenCleanOrigin::Initial) => {}
        }
        Ok(())
    }

    const fn path_receipt(&self) -> RecursiveGreenPathReceipt {
        self.path_receipt
    }

    fn installed_session(
        &self,
        ack: StructuralAck,
    ) -> Result<&M11PersistentRecursiveGreenSession, CandidateEndpointError> {
        let installed = self
            .installed
            .as_ref()
            .ok_or(CandidateEndpointError::InvalidState)?;
        if installed.ack != ack || !recursive_green_session_matches_ack(&installed.session, ack) {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        Ok(&installed.session)
    }

    fn request_cancel_pending(&mut self) -> Result<(), CandidateEndpointError> {
        if self.pending.is_none() {
            return Ok(());
        }
        self.cleanup
            .try_reserve(1)
            .map_err(|_| CandidateEndpointError::AllocationFailed)?;
        let pending = self
            .pending
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?;
        match pending {
            PendingRecursiveGreen::CleanPlan { base, .. } => {
                self.installed = base;
            }
            PendingRecursiveGreen::CleanBuild { build, base, .. } => {
                self.cleanup.push_back(RecursiveGreenCleanup::CleanBuild {
                    build,
                    restore: base,
                    begun: false,
                });
            }
            PendingRecursiveGreen::Adoption { base_ack, adoption } => {
                self.cleanup.push_back(RecursiveGreenCleanup::Adoption {
                    adoption,
                    restore_ack: base_ack,
                    begun: false,
                });
            }
            PendingRecursiveGreen::CancellingAdoptionForFallback {
                base_ack,
                adoption,
                begun,
                ..
            } => {
                self.cleanup.push_back(RecursiveGreenCleanup::Adoption {
                    adoption,
                    restore_ack: base_ack,
                    begun,
                });
            }
            PendingRecursiveGreen::ReadyClean { target, base, .. } => {
                self.installed = base;
                self.cleanup.push_back(RecursiveGreenCleanup::Session {
                    session: target,
                    begun: false,
                });
            }
            PendingRecursiveGreen::ReadyUpdate {
                base_ack,
                mut update,
            } => {
                let Some(target) = update.take_target() else {
                    self.pending = Some(PendingRecursiveGreen::ReadyUpdate { base_ack, update });
                    return Err(CandidateEndpointError::InvalidState);
                };
                let Some(base) = update.take_base() else {
                    self.cleanup.push_back(RecursiveGreenCleanup::Session {
                        session: target,
                        begun: false,
                    });
                    return Err(CandidateEndpointError::InvalidState);
                };
                self.installed = Some(InstalledRecursiveGreen {
                    ack: base_ack,
                    session: base,
                });
                self.cleanup.push_back(RecursiveGreenCleanup::Session {
                    session: target,
                    begun: false,
                });
            }
        }
        Ok(())
    }

    fn begin_close(&mut self) -> Result<(), CandidateEndpointError> {
        self.request_cancel_pending()?;
        if self.installed.is_some() {
            self.cleanup
                .try_reserve(1)
                .map_err(|_| CandidateEndpointError::AllocationFailed)?;
        }
        if let Some(installed) = self.installed.take() {
            self.cleanup.push_back(RecursiveGreenCleanup::Session {
                session: installed.session,
                begun: false,
            });
        }
        Ok(())
    }

    fn poll_cleanup(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<usize, CandidateEndpointError> {
        if fuel == 0 || self.cleanup.is_empty() {
            return Ok(0);
        }
        let cleanup = self
            .cleanup
            .front_mut()
            .ok_or(CandidateEndpointError::InvalidState)?;
        match cleanup {
            RecursiveGreenCleanup::Session { session, begun } => {
                let mut consumed = 0;
                if !*begun {
                    session.begin_release(runtime)?;
                    *begun = true;
                    consumed = 1;
                    if consumed == fuel {
                        return Ok(consumed);
                    }
                }
                if session.poll_release(runtime, fuel - consumed)? {
                    drop(
                        self.cleanup
                            .pop_front()
                            .ok_or(CandidateEndpointError::InvalidState)?,
                    );
                }
                Ok(fuel)
            }
            RecursiveGreenCleanup::CleanBuild {
                build,
                restore,
                begun,
            } => {
                let mut consumed = 0;
                if !*begun {
                    build.begin_cancel(runtime)?;
                    *begun = true;
                    consumed = 1;
                    if consumed == fuel {
                        return Ok(consumed);
                    }
                }
                if build.poll_cancel(runtime, fuel - consumed)?.status()
                    == M11PersistentRecursiveGreenBuildStatus::Cancelled
                {
                    if self.installed.is_some() && restore.is_some() {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    let restore = restore.take();
                    drop(
                        self.cleanup
                            .pop_front()
                            .ok_or(CandidateEndpointError::InvalidState)?,
                    );
                    if let Some(restore) = restore {
                        self.installed = Some(restore);
                    }
                }
                Ok(fuel)
            }
            RecursiveGreenCleanup::Adoption {
                adoption,
                restore_ack,
                begun,
            } => {
                let mut consumed = 0;
                if !*begun {
                    adoption.begin_cancel(runtime)?;
                    *begun = true;
                    consumed = 1;
                    if consumed == fuel {
                        return Ok(consumed);
                    }
                }
                if adoption.poll_cancel(runtime, fuel - consumed)? {
                    if self.installed.is_some() {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    let base = adoption
                        .take_base_after_cancel()
                        .ok_or(CandidateEndpointError::InvalidState)?;
                    let restore_ack = *restore_ack;
                    drop(
                        self.cleanup
                            .pop_front()
                            .ok_or(CandidateEndpointError::InvalidState)?,
                    );
                    self.installed = Some(InstalledRecursiveGreen {
                        ack: restore_ack,
                        session: base,
                    });
                }
                Ok(fuel)
            }
        }
    }
}

fn recursive_green_session_matches_ack(
    session: &M11PersistentRecursiveGreenSession,
    ack: StructuralAck,
) -> bool {
    let source = session.source();
    source.revision().get() == u64::from(ack.source_version.revision)
        && source.byte_len()
            == usize::try_from(ack.source_version.utf8_length).unwrap_or(usize::MAX)
        && source.utf16_len()
            == usize::try_from(ack.source_version.utf16_length).unwrap_or(usize::MAX)
        && split_u64(source.root().get()) == ack.source_root
        && session.syntax_profile() == ack.syntax_profile
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

fn prepare_viewport_presentation(
    runtime: &DocumentRuntime,
    retained: &RetainedCandidateBase,
    ready: ViewportInlineBatchReady,
) -> Result<StreamingViewportPresentation, ViewportPresentationPreparationFailure> {
    let ViewportInlineBatchReady {
        command,
        descriptor,
        range_receipt,
        mut leaves,
        total_inline_source_bytes,
        total_parser_transitions,
        total_fact_records,
        total_ready_roots,
    } = ready;
    let mut prepared = Vec::new();
    let mut directory = Vec::new();
    let reserve_result = prepared
        .try_reserve_exact(leaves.len())
        .and_then(|()| directory.try_reserve_exact(leaves.len()));
    if reserve_result.is_err() {
        return Err(ViewportPresentationPreparationFailure {
            error: CandidateEndpointError::AllocationFailed,
            cleanup: ViewportInlineBatchCleanup {
                active_job: None,
                active_abort_begun: false,
                ready: leaves,
                prepared,
                active_child: None,
                releasing: None,
                hot_replacement: None,
            },
        });
    }
    let fail = |error, leaves, prepared| ViewportPresentationPreparationFailure {
        error,
        cleanup: ViewportInlineBatchCleanup {
            active_job: None,
            active_abort_begun: false,
            ready: leaves,
            prepared,
            active_child: None,
            releasing: None,
            hot_replacement: None,
        },
    };
    let retained_descriptor = match retained.publication.descriptor(runtime) {
        Ok(descriptor) => descriptor,
        Err(error) => return Err(fail(error.into(), leaves, prepared)),
    };
    if retained.ack != command.base_ack || retained_descriptor != descriptor {
        return Err(fail(
            CandidateEndpointError::InvalidAuthority,
            leaves,
            prepared,
        ));
    }

    let mut transferred_node_count = 0_u32;
    let mut descriptor_fact_count = 0_u64;
    let mut authoritative_root_count = 0_u32;
    let mut maximum_encoded_child_bytes = 0_u64;
    while let Some(leaf) = leaves.pop() {
        let ordered_child_index = match u32::try_from(leaves.len()) {
            Ok(value) => value,
            Err(_) => {
                leaves.push(leaf);
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        };
        let global_row_ordinal = leaf.geometry.entry_ordinal;
        let binding = match retained
            .publication
            .recursive_green_hot_inline_sidecar_binding(
                runtime,
                leaf.parser_profile,
                u64::from(command.viewport_generation),
                leaf.geometry.frame,
                leaf.geometry.block_source.clone(),
                leaf.geometry.inline_source.clone(),
                leaf.geometry.block_source_utf16.clone(),
                leaf.geometry.inline_source_utf16.clone(),
            ) {
            Ok(binding) => binding,
            Err(error) => {
                leaves.push(leaf);
                return Err(fail(error.into(), leaves, prepared));
            }
        };
        let wire_binding = hot_inline_wire_binding(&binding);
        let ViewportInlineLeafReady {
            geometry,
            parser_profile,
            mut authority,
            publication,
        } = leaf;
        let (publication, descriptor) = match publication {
            ViewportInlineLeafPublication::Authoritative(root) => {
                match M11HotInlineSidecarSnapshotEncoder::authoritative(
                    runtime,
                    binding.clone(),
                    &root,
                ) {
                    Ok(encoder) => {
                        let descriptor = encoder.descriptor();
                        drop(encoder);
                        authoritative_root_count = match authoritative_root_count.checked_add(1) {
                            Some(value) => value,
                            None => {
                                leaves.push(ViewportInlineLeafReady {
                                    geometry,
                                    parser_profile,
                                    authority,
                                    publication: ViewportInlineLeafPublication::Authoritative(root),
                                });
                                return Err(fail(
                                    CandidateEndpointError::MetricOverflow,
                                    leaves,
                                    prepared,
                                ));
                            }
                        };
                        (
                            ViewportPreparedChildPublication::Authoritative(root),
                            descriptor,
                        )
                    }
                    Err(error) => {
                        leaves.push(ViewportInlineLeafReady {
                            geometry,
                            parser_profile,
                            authority,
                            publication: ViewportInlineLeafPublication::Authoritative(root),
                        });
                        return Err(fail(error.into(), leaves, prepared));
                    }
                }
            }
            ViewportInlineLeafPublication::Unsupported(record) => {
                let metadata = record.into_encoded();
                match M11HotInlineSidecarSnapshotEncoder::unsupported(
                    runtime,
                    binding.clone(),
                    HOT_INLINE_UNSUPPORTED_PARSER,
                    metadata.clone(),
                ) {
                    Ok(encoder) => {
                        let descriptor = encoder.descriptor();
                        drop(encoder);
                        (
                            ViewportPreparedChildPublication::Unsupported(metadata),
                            descriptor,
                        )
                    }
                    Err(error) => {
                        drop(metadata);
                        drop(authority.take());
                        return Err(fail(error.into(), leaves, prepared));
                    }
                }
            }
        };
        let hio1_envelope = hot_inline_envelope_from_descriptor(descriptor);
        transferred_node_count =
            match transferred_node_count.checked_add(hio1_envelope.transferred_node_count) {
                Some(value) => value,
                None => {
                    prepared.push(ViewportPreparedChild {
                        geometry,
                        parser_profile,
                        authority,
                        publication,
                        binding,
                        directory: ViewportPresentationDirectoryEntry {
                            ordered_child_index,
                            global_row_ordinal,
                            binding: wire_binding,
                            hio1_envelope,
                        },
                    });
                    return Err(fail(
                        CandidateEndpointError::MetricOverflow,
                        leaves,
                        prepared,
                    ));
                }
            };
        let child_frame_count = match hio1_envelope.transferred_node_count.checked_add(2) {
            Some(value) => value,
            None => {
                prepared.push(ViewportPreparedChild {
                    geometry,
                    parser_profile,
                    authority,
                    publication,
                    binding,
                    directory: ViewportPresentationDirectoryEntry {
                        ordered_child_index,
                        global_row_ordinal,
                        binding: wire_binding,
                        hio1_envelope,
                    },
                });
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        };
        maximum_encoded_child_bytes = match maximum_encoded_child_bytes
            .checked_add(u64::from(hio1_envelope.hio1_encoded_bytes))
            .and_then(|bytes| bytes.checked_add(u64::from(hio1_envelope.ipr2_descriptor_bytes)))
            // HIO1 Begin has a small fixed header before the authenticated
            // envelope and optional descriptor; End is likewise fixed-width.
            // Three public metadata-record widths safely dominate both
            // private fixed headers without depending on their exact layout.
            .and_then(|bytes| {
                bytes.checked_add(
                    u64::try_from(M11_INLINE_META_RECORD_BYTES)
                        .ok()?
                        .checked_mul(3)?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    u64::from(hio1_envelope.transferred_node_count)
                        .checked_mul(u64::try_from(M11_MAX_SNAPSHOT_FRAME_BYTES).ok()?)?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    u64::from(child_frame_count)
                        * u64::try_from(VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES).ok()?,
                )
            }) {
            Some(value) => value,
            None => {
                prepared.push(ViewportPreparedChild {
                    geometry,
                    parser_profile,
                    authority,
                    publication,
                    binding,
                    directory: ViewportPresentationDirectoryEntry {
                        ordered_child_index,
                        global_row_ordinal,
                        binding: wire_binding,
                        hio1_envelope,
                    },
                });
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        };
        if let HotInlineSidecarDisposition::Authoritative { fact_count, .. } =
            hio1_envelope.disposition
        {
            descriptor_fact_count = match descriptor_fact_count.checked_add(fact_count) {
                Some(value) => value,
                None => {
                    prepared.push(ViewportPreparedChild {
                        geometry,
                        parser_profile,
                        authority,
                        publication,
                        binding,
                        directory: ViewportPresentationDirectoryEntry {
                            ordered_child_index,
                            global_row_ordinal,
                            binding: wire_binding,
                            hio1_envelope,
                        },
                    });
                    return Err(fail(
                        CandidateEndpointError::MetricOverflow,
                        leaves,
                        prepared,
                    ));
                }
            };
        }
        let entry = ViewportPresentationDirectoryEntry {
            ordered_child_index,
            global_row_ordinal,
            binding: wire_binding,
            hio1_envelope,
        };
        directory.push(entry);
        prepared.push(ViewportPreparedChild {
            geometry,
            parser_profile,
            authority,
            publication,
            binding,
            directory: entry,
        });
    }
    directory.reverse();

    let leaf_count = match u32::try_from(directory.len()) {
        Ok(value) => value,
        Err(_) => {
            return Err(fail(
                CandidateEndpointError::MetricOverflow,
                leaves,
                prepared,
            ));
        }
    };
    if descriptor_fact_count != total_fact_records
        || total_ready_roots != authoritative_root_count
        || prepared.len() != directory.len()
    {
        return Err(fail(
            CandidateEndpointError::InvalidAuthority,
            leaves,
            prepared,
        ));
    }
    let binding = ViewportPresentationBinding {
        viewport_generation: command.viewport_generation,
        requested_range: ViewportPresentationMetricRange {
            start_utf8: command.start_byte_offset,
            start_utf16: command.start_utf16_offset,
            end_utf8: command.end_byte_offset,
            end_utf16: command.end_utf16_offset,
        },
        covered_range: ViewportPresentationMetricRange {
            start_utf8: command.start_byte_offset,
            start_utf16: command.start_utf16_offset,
            end_utf8: match u32::try_from(range_receipt.next_byte_offset) {
                Ok(value) => value,
                Err(_) => {
                    return Err(fail(
                        CandidateEndpointError::MetricOverflow,
                        leaves,
                        prepared,
                    ));
                }
            },
            end_utf16: match u32::try_from(range_receipt.next_utf16_offset) {
                Ok(value) => value,
                Err(_) => {
                    return Err(fail(
                        CandidateEndpointError::MetricOverflow,
                        leaves,
                        prepared,
                    ));
                }
            },
        },
        start: ViewportPresentationVisitStart {
            block_ordinal: command.start_entry_ordinal,
            utf8_offset: command.start_byte_offset,
            utf16_offset: command.start_utf16_offset,
        },
        next: ViewportPresentationVisitStart {
            block_ordinal: range_receipt.next_row_ordinal,
            utf8_offset: match u32::try_from(range_receipt.next_byte_offset) {
                Ok(value) => value,
                Err(_) => {
                    return Err(fail(
                        CandidateEndpointError::MetricOverflow,
                        leaves,
                        prepared,
                    ));
                }
            },
            utf16_offset: match u32::try_from(range_receipt.next_utf16_offset) {
                Ok(value) => value,
                Err(_) => {
                    return Err(fail(
                        CandidateEndpointError::MetricOverflow,
                        leaves,
                        prepared,
                    ));
                }
            },
        },
        complete: range_receipt.next_byte_offset == u64::from(command.end_byte_offset)
            && range_receipt.next_utf16_offset == u64::from(command.end_utf16_offset),
    };
    let query_limits = ViewportPresentationQueryLimits {
        maximum_structural_entries: command.limits.maximum_structural_entries,
        maximum_storage_pages: command.limits.maximum_storage_pages,
        maximum_inline_leaves: command.limits.maximum_inline_leaves,
        maximum_inline_leaf_source_bytes: command.limits.maximum_inline_leaf_source_bytes,
        maximum_inline_source_bytes: match u32::try_from(command.limits.maximum_inline_source_bytes)
        {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        maximum_fact_records: match u32::try_from(command.limits.maximum_fact_records) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        maximum_encoded_frame_bytes: match u32::try_from(command.limits.maximum_projection_bytes) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        maximum_parser_transitions: match u32::try_from(command.limits.maximum_parser_transitions) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
    };
    let envelope = crate::v3_publication_wire::ViewportPresentationEnvelopeMetrics {
        visited_structural_entries: match u32::try_from(range_receipt.visited_rows) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        visited_storage_pages: match u32::try_from(range_receipt.storage_pages_visited) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        ordered_leaf_count: leaf_count,
        inline_source_bytes: match u32::try_from(total_inline_source_bytes) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        fact_count: match u32::try_from(total_fact_records) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        transferred_node_count,
        parser_transitions: match u32::try_from(total_parser_transitions) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        aggregate_envelope_digest256: [0; 32],
    };
    let directory_entries_bytes = match directory
        .len()
        .checked_mul(VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES)
    {
        Some(value) => value,
        None => {
            return Err(fail(
                CandidateEndpointError::MetricOverflow,
                leaves,
                prepared,
            ));
        }
    };
    let directory_bytes =
        match VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES.checked_add(directory_entries_bytes) {
            Some(value) => value,
            None => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        };
    let frame_count = match leaf_count
        .checked_mul(2)
        .and_then(|count| count.checked_add(transferred_node_count))
        .and_then(|count| count.checked_add(3))
    {
        Some(value) => value,
        None => {
            return Err(fail(
                CandidateEndpointError::MetricOverflow,
                leaves,
                prepared,
            ));
        }
    };
    // This is the one authoritative admission check for the private VPB1
    // stream. It charges the exact selected HIO1 envelopes, IPR3 descriptors,
    // transferred-node ceiling, VPB1 wrappers, and directory. Do not add an
    // earlier logical-page * maximum-record charge: logical pages are semantic
    // units, not independently transferred maximum-size frames, and that
    // approximation rejects valid bounded batches long before this transport
    // limit is reached.
    let maximum_encoded_bytes = match maximum_encoded_child_bytes
        .checked_add(u64::try_from(VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES).unwrap_or(u64::MAX))
        .and_then(|bytes| bytes.checked_add(u64::try_from(directory_bytes).ok()?))
        .and_then(|bytes| {
            bytes.checked_add(u64::try_from(VIEWPORT_PRESENTATION_END_FRAME_BYTES).ok()?)
        })
        .and_then(|bytes| u32::try_from(bytes).ok())
    {
        Some(value) => value,
        None => {
            return Err(fail(
                CandidateEndpointError::MetricOverflow,
                leaves,
                prepared,
            ));
        }
    };
    let maximum_child_frame_bytes =
        match VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES.checked_add(M11_MAX_SNAPSHOT_FRAME_BYTES) {
            Some(value) => value,
            None => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        };
    if maximum_encoded_bytes > query_limits.maximum_encoded_frame_bytes {
        return Err(fail(
            CandidateEndpointError::ViewportInlineLimitExceeded("encoded frame bytes"),
            leaves,
            prepared,
        ));
    }
    let maximum_frame_bytes = match u32::try_from(directory_bytes.max(maximum_child_frame_bytes)) {
        Ok(value) => value,
        Err(_) => {
            return Err(fail(
                CandidateEndpointError::MetricOverflow,
                leaves,
                prepared,
            ));
        }
    };
    let limits = ViewportPresentationOfferLimits {
        maximum_frame_count: frame_count,
        maximum_encoded_frame_bytes: query_limits.maximum_encoded_frame_bytes,
        maximum_packet_bytes: u32::try_from(MAXIMUM_PACKET_ENCODED_BYTES)
            .expect("bounded packet bytes fit u32"),
        maximum_frame_bytes,
        maximum_program_children: u32::try_from(M11_MAX_ROLE_RECORDS)
            .expect("bounded program children fit u32"),
    };
    let offer_id = derive_viewport_identity(b"offer", command);
    let publication_session = derive_viewport_identity(b"publication-session", command);
    if offer_id == publication_session
        || publication_session == command.base_ack.publication_session
    {
        return Err(fail(
            CandidateEndpointError::InvalidAuthority,
            leaves,
            prepared,
        ));
    }
    let mut offer = ViewportPresentationBegin {
        schema: MANIFEST_SCHEMA,
        mode: ViewportPresentationMode::AggregatePage,
        offer_id,
        publication_session,
        base_ack: command.base_ack,
        binding,
        envelope,
        query_limits,
        limits,
    };
    let mut directory_frame = Vec::new();
    if directory_frame.try_reserve_exact(directory_bytes).is_err() {
        return Err(fail(
            CandidateEndpointError::AllocationFailed,
            leaves,
            prepared,
        ));
    }
    directory_frame.resize(directory_bytes, 0);
    let written = match encode_viewport_presentation_directory_into(
        offer,
        &directory,
        &mut directory_frame,
    ) {
        Ok(written) => written,
        Err(error) => return Err(fail(error.into(), leaves, prepared)),
    };
    if written != directory_frame.len() {
        return Err(fail(CandidateEndpointError::InvalidState, leaves, prepared));
    }
    offer.envelope.aggregate_envelope_digest256 =
        match viewport_presentation_aggregate_envelope_digest256(
            offer.binding,
            offer.envelope,
            &directory_frame,
        ) {
            Ok(digest) => digest,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::InvalidAuthority,
                    leaves,
                    prepared,
                ));
            }
        };
    Ok(StreamingViewportPresentation {
        offer,
        directory,
        directory_frame: Some(directory_frame.into_boxed_slice()),
        pending: prepared,
        active: None,
        releasing: None,
        phase: StreamPhase::NeedBegin,
        transport: Some(ViewportPresentationTransportDigest::new()),
        next_frame_ordinal: 0,
        next_record_ordinal: 0,
        packet: PacketBuilder::default(),
        lookahead: None,
        actual_child_frame_count: 0,
        actual_child_encoded_bytes: 0,
        commit: None,
        expected_ack: None,
    })
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
        let publication_path = if self.retained.is_none()
            && self.recursive_green.installed.is_none()
            && self.recursive_green.pending.is_none()
        {
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
                let parser_profile =
                    flark_engine::ParserProfileId::new(u64::from(command.base_ack.syntax_profile))
                        .ok_or(CandidateEndpointError::MetricOverflow)?;
                let prepared = self
                    .recursive_green
                    .installed_session(command.base_ack)?
                    .prepare_inline_leaf(
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
                ResolvedHotInlineDemand::PreparedInlineLeaf {
                    command,
                    identity,
                    inline_source,
                    inline_source_utf16,
                    parser_profile,
                    fence,
                }
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

    fn begin_hot_inline_sidecar(
        &mut self,
        runtime: &DocumentRuntime,
        ready: HotInlineReady,
    ) -> Result<(), CandidateEndpointError> {
        if self.hot_inline_sidecar.is_some() || self.active.is_some() || self.cleanup.is_some() {
            return Err(CandidateEndpointError::Busy);
        }
        let (
            command,
            physical_range,
            physical_range_utf16,
            visible_range,
            visible_range_utf16,
            owner,
            parser_profile,
            authority,
            publication,
        ) = ready.into_parts();
        let Some(retained) = self.retained.as_ref() else {
            self.schedule_failed_hot_inline_publication(publication, authority);
            return Err(CandidateEndpointError::InvalidAuthority);
        };
        if retained.ack != command.base_ack {
            self.schedule_failed_hot_inline_publication(publication, authority);
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let binding_result = match owner {
            HotInlineLeafOwner::BlockOrdinal(entry_ordinal) => {
                retained.publication.hot_inline_sidecar_binding(
                    runtime,
                    parser_profile,
                    u64::from(command.refinement_generation),
                    entry_ordinal,
                    physical_range,
                    visible_range,
                    physical_range_utf16,
                    visible_range_utf16,
                )
            }
            HotInlineLeafOwner::RecursiveGreenFrame(frame) => retained
                .publication
                .recursive_green_hot_inline_sidecar_binding(
                    runtime,
                    parser_profile,
                    u64::from(command.refinement_generation),
                    frame,
                    physical_range,
                    visible_range,
                    physical_range_utf16,
                    visible_range_utf16,
                ),
        };
        let binding = match binding_result {
            Ok(binding) => binding,
            Err(error) => {
                self.schedule_failed_hot_inline_publication(publication, authority);
                return Err(error.into());
            }
        };
        let wire_binding = hot_inline_wire_binding(&binding);
        let (encoder, root, authority) = match publication {
            HotInlineReadyPublication::Authoritative(root) => {
                let encoder = match root.as_ref() {
                    HotInlineProjectionRoot::Inline(root) => {
                        M11HotInlineSidecarSnapshotEncoder::authoritative(runtime, binding, root)
                    }
                    HotInlineProjectionRoot::IndentedCode(root) => {
                        M11HotInlineSidecarSnapshotEncoder::authoritative_indented_code(
                            runtime, binding, root,
                        )
                    }
                    HotInlineProjectionRoot::BlockQuote(root) => {
                        M11HotInlineSidecarSnapshotEncoder::authoritative_block_quote(
                            runtime, binding, root,
                        )
                    }
                    HotInlineProjectionRoot::BulletList(root) => {
                        M11HotInlineSidecarSnapshotEncoder::authoritative_bullet_list(
                            runtime, binding, root,
                        )
                    }
                    HotInlineProjectionRoot::BulletListItem {
                        root,
                        selected_item_ordinal,
                        canonical_line_ending,
                    } => M11HotInlineSidecarSnapshotEncoder::authoritative_bullet_list_item(
                        runtime,
                        binding,
                        root,
                        *selected_item_ordinal,
                        *canonical_line_ending,
                    ),
                    HotInlineProjectionRoot::OrderedListItem {
                        root,
                        selected_item_ordinal,
                        canonical_line_ending,
                        opening_marker_start,
                        opening_marker_end,
                        marker_value,
                    } => M11HotInlineSidecarSnapshotEncoder::authoritative_ordered_list_item(
                        runtime,
                        binding,
                        root,
                        *selected_item_ordinal,
                        *canonical_line_ending,
                        *opening_marker_start,
                        *opening_marker_end,
                        *marker_value,
                    ),
                };
                match encoder {
                    Ok(encoder) => (encoder, Some(root), authority),
                    Err(error) => {
                        self.hot_inline = Some(HotInlineState::Releasing {
                            root,
                            authority,
                            begun: false,
                            replacement: None,
                        });
                        return Err(error.into());
                    }
                }
            }
            HotInlineReadyPublication::Unsupported(unsupported) => {
                let (reason, metadata) = match unsupported {
                    HotInlineUnsupported::NotInlineLeaf { kind } => (
                        HOT_INLINE_UNSUPPORTED_NOT_INLINE_LEAF,
                        encode_not_inline_leaf_metadata(kind),
                    ),
                    HotInlineUnsupported::Parser(record) => {
                        (HOT_INLINE_UNSUPPORTED_PARSER, record.into_encoded())
                    }
                };
                (
                    M11HotInlineSidecarSnapshotEncoder::unsupported(
                        runtime, binding, reason, metadata,
                    )?,
                    None,
                    authority,
                )
            }
        };
        let descriptor = encoder.descriptor();
        let maximum_frame_bytes = match u32::try_from(M11_MAX_SNAPSHOT_FRAME_BYTES) {
            Ok(value) => value,
            Err(_) => {
                self.schedule_hot_inline_root_release(root, authority);
                return Err(CandidateEndpointError::MetricOverflow);
            }
        };
        let Some(maximum_frame_count) = descriptor.transferred_node_count().checked_add(2) else {
            self.schedule_hot_inline_root_release(root, authority);
            return Err(CandidateEndpointError::MetricOverflow);
        };
        let Some(maximum_encoded_frame_bytes) =
            maximum_frame_count.checked_mul(maximum_frame_bytes)
        else {
            self.schedule_hot_inline_root_release(root, authority);
            return Err(CandidateEndpointError::MetricOverflow);
        };
        let owner_wire = wire_binding.block_ordinal;
        let offer_id = derive_hot_inline_identity(b"offer", command, owner_wire);
        let publication_session =
            derive_hot_inline_identity(b"publication-session", command, owner_wire);
        if publication_session == command.base_ack.publication_session {
            self.schedule_hot_inline_root_release(root, authority);
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let offer = HotInlineSidecarBegin {
            schema: HOT_INLINE_SIDECAR_SCHEMA,
            mode: HotInlineSidecarMode::HotInlineSidecar,
            offer_id,
            publication_session,
            base_ack: command.base_ack,
            binding: wire_binding,
            envelope: hot_inline_envelope_from_descriptor(descriptor),
            limits: OfferLimits {
                maximum_frame_count,
                maximum_encoded_frame_bytes,
                maximum_packet_bytes: u32::try_from(MAXIMUM_PACKET_ENCODED_BYTES)
                    .expect("bounded packet bytes fit u32"),
                maximum_frame_bytes,
                maximum_program_children: u32::try_from(M11_MAX_ROLE_RECORDS)
                    .expect("bounded program children fit u32"),
            },
        };
        self.hot_inline_sidecar = Some(StreamingHotInlineSidecar {
            encoder,
            root,
            authority,
            offer,
            phase: StreamPhase::NeedBegin,
            transport: Some(HotInlineSidecarTransportDigest::new()),
            next_frame_ordinal: 0,
            next_record_ordinal: 0,
            next_node_ordinal: None,
            packet: PacketBuilder::default(),
            lookahead: None,
            root_stream_digest: None,
            commit: None,
            expected_ack: None,
        });
        Ok(())
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
                        let resolver_poll_fuel = (fuel - transitions).min(
                            usize::try_from(command_remaining).unwrap_or(usize::MAX),
                        );
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
                            self.viewport_inline_batch = Some(
                                ViewportInlineBatchState::Cancelling(Box::new(
                                    (*running).into_cleanup(),
                                )),
                            );
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
        let Some((base_ack, target_source)) =
            self.active.as_ref().and_then(|active| match active {
                ActiveCandidate::ParsingExact(parsing) => {
                    Some((parsing.base.ack, parsing.witness.target()))
                }
                ActiveCandidate::ParsingOrdinaryExact(parsing) => {
                    Some((parsing.base.ack, parsing.witness.target()))
                }
                ActiveCandidate::ParsingExactFallback(parsing) => {
                    Some((parsing.base.ack, parsing.witness.target()))
                }
                _ => None,
            })
        else {
            return Ok(false);
        };
        if self
            .recursive_green
            .ready_update_for(base_ack, target_source)
            .is_none()
        {
            return Ok(false);
        }

        let active = self
            .active
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?;
        let (context, mut base, witness, certified) = match active {
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
        let next_restart = CandidateRestartAuthority::ExactBaseOnly {
            source: target_source,
            binding: M11ParserBinding::new(witness.parser_profile(), GRAMMAR_REVISION),
        };
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
                            let next_restart =
                                take_candidate_restart_authority(&mut result, parser_binding)?;
                            let publication = derive_identity(
                                b"publication",
                                context.binding,
                                context.completion,
                                context.parse_generation,
                            );
                            let writer = match publication_path {
                                CleanPublicationPath::RecursiveGreenInitial => {
                                    let candidate =
                                        M11ParserCandidate::derive_with_recursive_green(
                                            certified, &result,
                                        )?;
                                    let session = self
                                        .recursive_green
                                        .initial_clean_ready_session(source, syntax_profile)?;
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
                                let candidate = match M11ParserCandidate::
                                    derive_with_recursive_green_from_persistent(certified, &result)
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
                                let session = self
                                    .recursive_green
                                    .incremental_clean_ready_session(target_source, syntax_profile)
                                    .ok_or(CandidateEndpointError::InvalidState)?;
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
                    || self.recursive_green.installed.is_some()
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

impl StreamingCandidate {
    fn poll_event(
        &mut self,
        runtime: &DocumentRuntime,
        fuel: usize,
    ) -> Result<CandidatePoll, CandidateEndpointError> {
        match self.phase {
            StreamPhase::NeedBegin => {
                self.phase = StreamPhase::AwaitBeginReceipt;
                Ok(CandidatePoll::Event {
                    transitions: 0,
                    event: Box::new(CandidateEvent {
                        credit: CandidateCredit::Begin,
                        body: CandidateEventBody::Begin(self.offer),
                    }),
                })
            }
            StreamPhase::NeedPacket => self.poll_packet(runtime, fuel),
            StreamPhase::NeedCommit => {
                let commit = self.commit.ok_or(CandidateEndpointError::InvalidState)?;
                self.phase = StreamPhase::AwaitCommitReceipt;
                Ok(CandidatePoll::Event {
                    transitions: 0,
                    event: Box::new(CandidateEvent {
                        credit: CandidateCredit::Commit,
                        body: CandidateEventBody::Commit(commit),
                    }),
                })
            }
            _ => Ok(CandidatePoll::Pending { transitions: 0 }),
        }
    }

    fn poll_packet(
        &mut self,
        runtime: &DocumentRuntime,
        fuel: usize,
    ) -> Result<CandidatePoll, CandidateEndpointError> {
        let maximum_packet_bytes = usize::try_from(self.offer.limits.maximum_packet_bytes)
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let mut transitions = 0_usize;
        loop {
            if self.packet.end || self.packet.saturated(maximum_packet_bytes)? {
                let event = self.take_packet_event()?;
                return Ok(CandidatePoll::Event {
                    transitions,
                    event: Box::new(event),
                });
            }
            let polled = if let Some(frame) = self.lookahead.take() {
                M11OwnedSnapshotPoll::Frame {
                    transitions: 0,
                    frame,
                }
            } else if transitions == fuel {
                return Ok(CandidatePoll::Pending { transitions });
            } else {
                let stream = self
                    .stream
                    .as_mut()
                    .ok_or(CandidateEndpointError::InvalidState)?;
                if self.next_frame_ordinal == 0 {
                    if !self.packet.frames.is_empty() {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    M11OwnedSnapshotPoll::Frame {
                        transitions: 0,
                        frame: stream.begin_frame()?,
                    }
                } else {
                    stream.poll(runtime, fuel - transitions)?
                }
            };
            match polled {
                M11OwnedSnapshotPoll::Pending {
                    transitions: consumed,
                } => {
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    return Ok(CandidatePoll::Pending { transitions });
                }
                M11OwnedSnapshotPoll::Frame {
                    transitions: consumed,
                    frame,
                } => {
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    if !self
                        .packet
                        .can_accept(frame.bytes.len(), maximum_packet_bytes)?
                    {
                        if self.packet.frames.is_empty() || self.lookahead.is_some() {
                            return Err(CandidateEndpointError::InvalidState);
                        }
                        self.lookahead = Some(frame);
                        let event = self.take_packet_event()?;
                        return Ok(CandidatePoll::Event {
                            transitions,
                            event: Box::new(event),
                        });
                    }
                    self.append_frame(runtime, frame)?;
                    if self.packet.end || self.packet.saturated(maximum_packet_bytes)? {
                        let event = self.take_packet_event()?;
                        return Ok(CandidatePoll::Event {
                            transitions,
                            event: Box::new(event),
                        });
                    }
                    if transitions == fuel {
                        return Ok(CandidatePoll::Pending { transitions });
                    }
                }
                M11OwnedSnapshotPoll::ReplayRequired {
                    transitions: consumed,
                } => {
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel || self.resume_after_packet_credit {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    if self.packet.frames.is_empty() {
                        // The final replacement page may have exactly filled a
                        // prior packet. Reaching this branch means that packet
                        // has already received host credit, so its replay is
                        // complete and the producer can resume immediately.
                        self.stream
                            .as_mut()
                            .ok_or(CandidateEndpointError::InvalidState)?
                            .resume_exact_base_delta()?;
                        continue;
                    }
                    self.resume_after_packet_credit = true;
                    let event = self.take_packet_event()?;
                    return Ok(CandidatePoll::Event {
                        transitions,
                        event: Box::new(event),
                    });
                }
            }
        }
    }

    fn append_frame(
        &mut self,
        runtime: &DocumentRuntime,
        frame: M11SnapshotFrame,
    ) -> Result<(), CandidateEndpointError> {
        let ordinal = self.next_frame_ordinal;
        let first_record_ordinal = self.next_record_ordinal;
        let wire_kind = match frame.kind {
            M11SnapshotFrameKind::Begin => CandidateSnapshotFrameKind::Begin,
            M11SnapshotFrameKind::Node => CandidateSnapshotFrameKind::Node,
            M11SnapshotFrameKind::End => CandidateSnapshotFrameKind::End,
            M11SnapshotFrameKind::SourceFactsReplacementPage => {
                CandidateSnapshotFrameKind::SourceFactsReplacementPage
            }
            M11SnapshotFrameKind::BlockSequenceReplacementPage => {
                CandidateSnapshotFrameKind::BlockSequenceReplacementPage
            }
            M11SnapshotFrameKind::RecursiveGreenReplacementPage => {
                CandidateSnapshotFrameKind::RecursiveGreenReplacementPage
            }
        };
        if matches!(frame.kind, M11SnapshotFrameKind::Begin) != (ordinal == 0)
            || frame.bytes.is_empty()
            || frame.bytes.len() > M11_MAX_SNAPSHOT_FRAME_BYTES
        {
            return Err(CandidateEndpointError::InvalidState);
        }
        if frame.kind == M11SnapshotFrameKind::Node {
            let node_ordinal = frame
                .node_ordinal
                .ok_or(CandidateEndpointError::InvalidState)?;
            if self
                .next_node_ordinal
                .is_some_and(|expected| node_ordinal != expected)
            {
                return Err(CandidateEndpointError::InvalidState);
            }
            self.next_node_ordinal = Some(
                node_ordinal
                    .checked_add(1)
                    .ok_or(CandidateEndpointError::MetricOverflow)?,
            );
        } else if frame.node_ordinal.is_some() {
            return Err(CandidateEndpointError::InvalidState);
        }
        let next_frame_ordinal = ordinal
            .checked_add(1)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let next_record_ordinal = first_record_ordinal
            .checked_add(frame.canonical_record_count)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let digest256 = self
            .transport
            .as_mut()
            .ok_or(CandidateEndpointError::InvalidState)?
            .push(
                ordinal,
                first_record_ordinal,
                frame.canonical_record_count,
                wire_kind,
                &frame.bytes,
            )?;
        let end = frame.kind == M11SnapshotFrameKind::End;
        let canonical_digest = frame.canonical_stream_digest256;
        self.packet.push(
            ordinal,
            first_record_ordinal,
            frame.canonical_record_count,
            protocol_digest128_from_blake3(ProtocolDigestDomain::CandidateFrame, digest256),
            frame.bytes,
            end,
        )?;
        self.next_frame_ordinal = next_frame_ordinal;
        self.next_record_ordinal = next_record_ordinal;
        if end {
            self.finish_frame_stream(runtime, canonical_digest)?;
        }
        Ok(())
    }

    fn finish_frame_stream(
        &mut self,
        runtime: &DocumentRuntime,
        canonical_digest: Option<[u8; 32]>,
    ) -> Result<(), CandidateEndpointError> {
        if self.next_record_ordinal != self.offer.transferred_record_count {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        if self.sealed_publication.is_some() {
            return Err(CandidateEndpointError::InvalidState);
        }
        let canonical_stream_digest = protocol_digest128_from_blake3(
            ProtocolDigestDomain::CandidateStream,
            canonical_digest.ok_or(CandidateEndpointError::InvalidState)?,
        );
        let mut stream = self
            .stream
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?;
        match self.offer.mode {
            PublicationMode::FullSnapshot | PublicationMode::ExactBaseReferencesDelta => {}
            PublicationMode::ExactBaseDelta => {
                if self.superseded_exact_base.is_some() {
                    self.stream = Some(stream);
                    return Err(CandidateEndpointError::InvalidState);
                }
                let superseded_exact_base = match stream.take_superseded_exact_base(runtime) {
                    Ok(Some(base)) => base,
                    Ok(None) => {
                        self.stream = Some(stream);
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    Err(error) => {
                        self.stream = Some(stream);
                        return Err(error.into());
                    }
                };
                self.superseded_exact_base = Some(Box::new(superseded_exact_base));
            }
        }
        let publication = match stream.into_retained_publication(runtime) {
            Ok(publication) => publication,
            Err(failure) => {
                let (error, stream) = failure.into_parts();
                self.stream = Some(stream);
                return Err(error.into());
            }
        };
        self.sealed_publication = Some(publication);
        self.canonical_stream_digest = Some(canonical_stream_digest);
        let transport = self
            .transport
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?
            .finish();
        if transport.frame_count != self.next_frame_ordinal {
            return Err(CandidateEndpointError::InvalidState);
        }
        self.commit = Some(CommitRequest {
            offer_id: self.offer.offer_id,
            actual_frame_count: transport.frame_count,
            actual_encoded_frame_bytes: transport.encoded_frame_bytes,
            rolling_transport_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::CandidateTransport,
                transport.digest256,
            ),
            canonical_stream_digest,
        });
        self.expected_ack = Some(StructuralAck {
            publication_session: self.offer.publication_session,
            host_revision: self.offer.target_host_revision,
            source_version: self.offer.source_version,
            source_root: self.offer.source_root,
            parse_generation: self.offer.parse_generation,
            grammar_revision: self.offer.grammar_revision,
            syntax_profile: self.offer.syntax_profile,
            authority_mask: self.offer.authority_mask,
            record_count: self.offer.target_record_count,
            sequence_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::CandidateAckSequence,
                self.descriptor.manifest_digest256,
            ),
            manifest_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::CandidateManifest,
                self.descriptor.manifest_digest256,
            ),
        });
        Ok(())
    }

    fn take_packet_event(&mut self) -> Result<CandidateEvent, CandidateEndpointError> {
        let packet = std::mem::take(&mut self.packet);
        let first_frame_ordinal = packet
            .first_frame_ordinal
            .ok_or(CandidateEndpointError::InvalidState)?;
        let frame_count = u32::try_from(packet.frames.len())
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let end = packet.end;
        let encoded = packet.encode(self.offer.offer_id)?;
        self.phase = StreamPhase::AwaitPacketReceipt {
            first_frame_ordinal,
            frame_count,
            end,
        };
        Ok(CandidateEvent {
            credit: CandidateCredit::Packet {
                first_frame_ordinal,
                frame_count,
                end,
            },
            body: CandidateEventBody::Packet { encoded },
        })
    }
}

impl StreamingHotInlineSidecar {
    fn poll_event(
        &mut self,
        runtime: &DocumentRuntime,
        fuel: usize,
    ) -> Result<CandidatePoll, CandidateEndpointError> {
        match self.phase {
            StreamPhase::NeedBegin => {
                self.phase = StreamPhase::AwaitBeginReceipt;
                Ok(CandidatePoll::HotInlineEvent {
                    transitions: 0,
                    event: Box::new(HotInlineEvent {
                        credit: HotInlineCredit::Begin,
                        body: HotInlineEventBody::Begin(self.offer),
                    }),
                })
            }
            StreamPhase::NeedPacket => self.poll_packet(runtime, fuel),
            StreamPhase::NeedCommit => {
                let commit = self.commit.ok_or(CandidateEndpointError::InvalidState)?;
                self.phase = StreamPhase::AwaitCommitReceipt;
                Ok(CandidatePoll::HotInlineEvent {
                    transitions: 0,
                    event: Box::new(HotInlineEvent {
                        credit: HotInlineCredit::Commit,
                        body: HotInlineEventBody::Commit(commit),
                    }),
                })
            }
            _ => Ok(CandidatePoll::Pending { transitions: 0 }),
        }
    }

    fn poll_packet(
        &mut self,
        runtime: &DocumentRuntime,
        fuel: usize,
    ) -> Result<CandidatePoll, CandidateEndpointError> {
        let maximum_packet_bytes = usize::try_from(self.offer.limits.maximum_packet_bytes)
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let mut transitions = 0_usize;
        loop {
            if self.packet.end || self.packet.saturated(maximum_packet_bytes)? {
                let event = self.take_packet_event()?;
                return Ok(CandidatePoll::HotInlineEvent {
                    transitions,
                    event: Box::new(event),
                });
            }
            let polled = if let Some(frame) = self.lookahead.take() {
                M11HotInlineSidecarSnapshotPoll::Frame {
                    transitions: 0,
                    frame,
                }
            } else if transitions == fuel {
                return Ok(CandidatePoll::Pending { transitions });
            } else if self.next_frame_ordinal == 0 {
                if !self.packet.frames.is_empty() {
                    return Err(CandidateEndpointError::InvalidState);
                }
                M11HotInlineSidecarSnapshotPoll::Frame {
                    transitions: 0,
                    frame: self.encoder.begin_frame()?,
                }
            } else {
                self.encoder.poll(runtime, fuel - transitions)?
            };
            match polled {
                M11HotInlineSidecarSnapshotPoll::Pending {
                    transitions: consumed,
                } => {
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    return Ok(CandidatePoll::Pending { transitions });
                }
                M11HotInlineSidecarSnapshotPoll::Frame {
                    transitions: consumed,
                    frame,
                } => {
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    if !self
                        .packet
                        .can_accept(frame.bytes.len(), maximum_packet_bytes)?
                    {
                        if self.packet.frames.is_empty() || self.lookahead.is_some() {
                            return Err(CandidateEndpointError::InvalidState);
                        }
                        self.lookahead = Some(frame);
                        let event = self.take_packet_event()?;
                        return Ok(CandidatePoll::HotInlineEvent {
                            transitions,
                            event: Box::new(event),
                        });
                    }
                    self.append_frame(frame)?;
                    if self.packet.end || self.packet.saturated(maximum_packet_bytes)? {
                        let event = self.take_packet_event()?;
                        return Ok(CandidatePoll::HotInlineEvent {
                            transitions,
                            event: Box::new(event),
                        });
                    }
                    if transitions == fuel {
                        return Ok(CandidatePoll::Pending { transitions });
                    }
                }
            }
        }
    }

    fn append_frame(
        &mut self,
        frame: M11HotInlineSidecarFrame,
    ) -> Result<(), CandidateEndpointError> {
        let ordinal = self.next_frame_ordinal;
        let first_record_ordinal = self.next_record_ordinal;
        let (wire_kind, record_count) = match frame.kind {
            M11HotInlineSidecarFrameKind::Begin => (HotInlineSidecarFrameKind::Begin, 0),
            M11HotInlineSidecarFrameKind::Node => (HotInlineSidecarFrameKind::Node, 1),
            M11HotInlineSidecarFrameKind::End => (HotInlineSidecarFrameKind::End, 0),
        };
        if matches!(frame.kind, M11HotInlineSidecarFrameKind::Begin) != (ordinal == 0)
            || frame.bytes.is_empty()
            || frame.bytes.len() > M11_MAX_SNAPSHOT_FRAME_BYTES
        {
            return Err(CandidateEndpointError::InvalidState);
        }
        if frame.kind == M11HotInlineSidecarFrameKind::Node {
            let node_ordinal = frame
                .node_ordinal
                .ok_or(CandidateEndpointError::InvalidState)?;
            if self
                .next_node_ordinal
                .is_some_and(|expected| node_ordinal != expected)
            {
                return Err(CandidateEndpointError::InvalidState);
            }
            self.next_node_ordinal = Some(
                node_ordinal
                    .checked_add(1)
                    .ok_or(CandidateEndpointError::MetricOverflow)?,
            );
        } else if frame.node_ordinal.is_some() {
            return Err(CandidateEndpointError::InvalidState);
        }
        let next_frame_ordinal = ordinal
            .checked_add(1)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let next_record_ordinal = first_record_ordinal
            .checked_add(record_count)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let digest256 = self
            .transport
            .as_mut()
            .ok_or(CandidateEndpointError::InvalidState)?
            .push(
                ordinal,
                first_record_ordinal,
                record_count,
                wire_kind,
                &frame.bytes,
            )?;
        let end = frame.kind == M11HotInlineSidecarFrameKind::End;
        self.packet.push(
            ordinal,
            first_record_ordinal,
            record_count,
            protocol_digest128_from_blake3(ProtocolDigestDomain::HotInlineSidecarFrame, digest256),
            frame.bytes,
            end,
        )?;
        self.next_frame_ordinal = next_frame_ordinal;
        self.next_record_ordinal = next_record_ordinal;
        if end {
            self.finish_frame_stream(
                frame
                    .root_stream_digest256
                    .ok_or(CandidateEndpointError::InvalidState)?,
            )?;
        } else if frame.root_stream_digest256.is_some() {
            return Err(CandidateEndpointError::InvalidState);
        }
        Ok(())
    }

    fn finish_frame_stream(
        &mut self,
        root_stream_digest256: [u8; 32],
    ) -> Result<(), CandidateEndpointError> {
        if self.next_record_ordinal != self.offer.envelope.transferred_node_count {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let root_stream_digest = protocol_digest128_from_blake3(
            ProtocolDigestDomain::HotInlineSidecarRootStream,
            root_stream_digest256,
        );
        self.root_stream_digest = Some(root_stream_digest);
        let transport = self
            .transport
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?
            .finish();
        if transport.frame_count != self.next_frame_ordinal
            || transport.frame_count > self.offer.limits.maximum_frame_count
            || transport.encoded_frame_bytes > self.offer.limits.maximum_encoded_frame_bytes
        {
            return Err(CandidateEndpointError::InvalidState);
        }
        self.commit = Some(HotInlineSidecarCommitRequest {
            offer_id: self.offer.offer_id,
            actual_frame_count: transport.frame_count,
            actual_encoded_frame_bytes: transport.encoded_frame_bytes,
            rolling_transport_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::HotInlineSidecarTransport,
                transport.digest256,
            ),
            root_stream_digest,
        });
        self.expected_ack = Some(InlineSidecarAck {
            publication_session: self.offer.publication_session,
            base_ack: self.offer.base_ack,
            refinement_generation: self.offer.binding.refinement_generation,
            block_ordinal: self.offer.binding.block_ordinal,
            transferred_node_count: self.offer.envelope.transferred_node_count,
            disposition: match self.offer.envelope.disposition {
                HotInlineSidecarDisposition::Authoritative { .. } => {
                    InlineSidecarAckDisposition::Authoritative
                }
                HotInlineSidecarDisposition::Unsupported { .. } => {
                    InlineSidecarAckDisposition::Unsupported
                }
            },
            hio1_envelope_digest256: self.offer.envelope.hio1_envelope_digest256,
            root_stream_digest,
        });
        Ok(())
    }

    fn take_packet_event(&mut self) -> Result<HotInlineEvent, CandidateEndpointError> {
        let packet = std::mem::take(&mut self.packet);
        let first_frame_ordinal = packet
            .first_frame_ordinal
            .ok_or(CandidateEndpointError::InvalidState)?;
        let frame_count = u32::try_from(packet.frames.len())
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let end = packet.end;
        let encoded = packet.encode(self.offer.offer_id)?;
        self.phase = StreamPhase::AwaitPacketReceipt {
            first_frame_ordinal,
            frame_count,
            end,
        };
        Ok(HotInlineEvent {
            credit: HotInlineCredit::Packet {
                first_frame_ordinal,
                frame_count,
                end,
            },
            body: HotInlineEventBody::Packet { encoded },
        })
    }
}

impl StreamingViewportPresentation {
    fn poll_event(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<CandidatePoll, CandidateEndpointError> {
        match self.phase {
            StreamPhase::NeedBegin => {
                self.phase = StreamPhase::AwaitBeginReceipt;
                Ok(CandidatePoll::ViewportPresentationEvent {
                    transitions: 0,
                    event: Box::new(CandidateViewportPresentationEvent {
                        credit: ViewportPresentationCredit::Begin,
                        body: CandidateViewportPresentationEventBody::Begin(self.offer),
                    }),
                })
            }
            StreamPhase::NeedPacket => self.poll_packet(runtime, fuel),
            StreamPhase::NeedCommit => {
                let commit = self.commit.ok_or(CandidateEndpointError::InvalidState)?;
                self.phase = StreamPhase::AwaitCommitReceipt;
                Ok(CandidatePoll::ViewportPresentationEvent {
                    transitions: 0,
                    event: Box::new(CandidateViewportPresentationEvent {
                        credit: ViewportPresentationCredit::Commit,
                        body: CandidateViewportPresentationEventBody::Commit(commit),
                    }),
                })
            }
            _ => Ok(CandidatePoll::Pending { transitions: 0 }),
        }
    }

    fn poll_packet(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<CandidatePoll, CandidateEndpointError> {
        let maximum_packet_bytes = usize::try_from(self.offer.limits.maximum_packet_bytes)
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let maximum_frame_bytes = usize::try_from(self.offer.limits.maximum_frame_bytes)
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let mut transitions = 0_usize;
        loop {
            if self.packet.end || self.packet.saturated(maximum_packet_bytes)? {
                return Ok(CandidatePoll::ViewportPresentationEvent {
                    transitions,
                    event: Box::new(self.take_packet_event()?),
                });
            }
            if self.next_frame_ordinal == 0 {
                let mut bytes = vec![0_u8; VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES];
                let written =
                    encode_viewport_presentation_parent_frame_into(self.offer, &mut bytes)?;
                if written != bytes.len() {
                    return Err(CandidateEndpointError::InvalidState);
                }
                if !self.packet.can_accept_with_frame_limit(
                    bytes.len(),
                    maximum_packet_bytes,
                    maximum_frame_bytes,
                )? {
                    return Err(CandidateEndpointError::InvalidState);
                }
                self.append_outer_frame(
                    ViewportPresentationFrameKind::Begin,
                    0,
                    bytes.into_boxed_slice(),
                    false,
                )?;
                continue;
            }
            if self.next_frame_ordinal == 1 {
                let bytes = self
                    .directory_frame
                    .take()
                    .ok_or(CandidateEndpointError::InvalidState)?;
                if !self.packet.can_accept_with_frame_limit(
                    bytes.len(),
                    maximum_packet_bytes,
                    maximum_frame_bytes,
                )? {
                    if self.packet.frames.is_empty() {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    self.directory_frame = Some(bytes);
                    return Ok(CandidatePoll::ViewportPresentationEvent {
                        transitions,
                        event: Box::new(self.take_packet_event()?),
                    });
                }
                self.append_outer_frame(
                    ViewportPresentationFrameKind::Directory,
                    self.offer.envelope.ordered_leaf_count,
                    bytes,
                    false,
                )?;
                continue;
            }

            if let Some(releasing) = self.releasing.as_mut() {
                if transitions == fuel {
                    return Ok(CandidatePoll::Pending { transitions });
                }
                if !releasing.begun {
                    releasing.root.begin_release(runtime)?;
                    releasing.begun = true;
                    transitions = checked_add(transitions, 1)?;
                    if transitions == fuel {
                        return Ok(CandidatePoll::Pending { transitions });
                    }
                }
                let polled = releasing.root.poll_release(runtime, fuel - transitions)?;
                transitions = checked_add(transitions, polled.receipt().transitions)?;
                if transitions > fuel {
                    return Err(CandidateEndpointError::InvalidState);
                }
                if !polled.complete() {
                    return Ok(CandidatePoll::Pending { transitions });
                }
                let released = self
                    .releasing
                    .take()
                    .ok_or(CandidateEndpointError::InvalidState)?;
                drop(released.root);
                drop(released.authority);
                continue;
            }

            if self.active.is_none() {
                if let Some(mut prepared) = self.pending.pop() {
                    let expected_directory_index = self
                        .offer
                        .envelope
                        .ordered_leaf_count
                        .checked_sub(
                            u32::try_from(self.pending.len())
                                .map_err(|_| CandidateEndpointError::MetricOverflow)?,
                        )
                        .and_then(|count| count.checked_sub(1))
                        .ok_or(CandidateEndpointError::InvalidState)?;
                    if prepared.directory.ordered_child_index != expected_directory_index
                        || prepared.parser_profile.get()
                            != prepared.directory.binding.parser_profile
                        || hot_inline_wire_binding(&prepared.binding) != prepared.directory.binding
                        || prepared.geometry.entry_ordinal != prepared.directory.global_row_ordinal
                        || prepared.directory.binding.owner()
                            != Some(HotInlineSidecarOwner::RecursiveGreenFrame(
                                prepared.geometry.frame.get(),
                            ))
                    {
                        self.pending.push(prepared);
                        return Err(CandidateEndpointError::InvalidAuthority);
                    }
                    let (encoder, root) = match prepared.publication {
                        ViewportPreparedChildPublication::Authoritative(root) => (
                            M11HotInlineSidecarSnapshotEncoder::authoritative(
                                runtime,
                                prepared.binding,
                                &root,
                            )?,
                            Some(root),
                        ),
                        ViewportPreparedChildPublication::Unsupported(metadata) => (
                            M11HotInlineSidecarSnapshotEncoder::unsupported(
                                runtime,
                                prepared.binding,
                                HOT_INLINE_UNSUPPORTED_PARSER,
                                metadata,
                            )?,
                            None,
                        ),
                    };
                    self.active = Some(ViewportActiveChild {
                        directory_index: expected_directory_index,
                        encoder,
                        root,
                        authority: prepared.authority.take(),
                        next_frame_ordinal: 0,
                        next_node_ordinal: None,
                    });
                } else {
                    let transport = self
                        .transport
                        .as_ref()
                        .ok_or(CandidateEndpointError::InvalidState)?
                        .receipt();
                    let actual_frame_count = transport
                        .frame_count
                        .checked_add(1)
                        .ok_or(CandidateEndpointError::MetricOverflow)?;
                    let actual_encoded_frame_bytes = transport
                        .encoded_frame_bytes
                        .checked_add(
                            u32::try_from(VIEWPORT_PRESENTATION_END_FRAME_BYTES)
                                .expect("VPB1 End width fits u32"),
                        )
                        .ok_or(CandidateEndpointError::MetricOverflow)?;
                    let mut bytes = vec![0_u8; VIEWPORT_PRESENTATION_END_FRAME_BYTES];
                    let written = encode_viewport_presentation_end_frame_into(
                        self.offer,
                        ViewportPresentationEndFrame {
                            ordered_leaf_count: self.offer.envelope.ordered_leaf_count,
                            actual_frame_count,
                            actual_encoded_frame_bytes,
                            aggregate_envelope_digest256: self
                                .offer
                                .envelope
                                .aggregate_envelope_digest256,
                        },
                        &mut bytes,
                    )?;
                    if written != bytes.len() {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    if !self.packet.can_accept_with_frame_limit(
                        bytes.len(),
                        maximum_packet_bytes,
                        maximum_frame_bytes,
                    )? {
                        if self.packet.frames.is_empty() {
                            return Err(CandidateEndpointError::InvalidState);
                        }
                        return Ok(CandidatePoll::ViewportPresentationEvent {
                            transitions,
                            event: Box::new(self.take_packet_event()?),
                        });
                    }
                    self.append_outer_frame(
                        ViewportPresentationFrameKind::End,
                        0,
                        bytes.into_boxed_slice(),
                        true,
                    )?;
                    self.finish_frame_stream()?;
                    return Ok(CandidatePoll::ViewportPresentationEvent {
                        transitions,
                        event: Box::new(self.take_packet_event()?),
                    });
                }
            }

            let polled = if let Some(frame) = self.lookahead.take() {
                M11HotInlineSidecarSnapshotPoll::Frame {
                    transitions: 0,
                    frame,
                }
            } else {
                let active = self
                    .active
                    .as_mut()
                    .ok_or(CandidateEndpointError::InvalidState)?;
                if active.next_frame_ordinal == 0 {
                    M11HotInlineSidecarSnapshotPoll::Frame {
                        transitions: 0,
                        frame: active.encoder.begin_frame()?,
                    }
                } else if transitions == fuel {
                    return Ok(CandidatePoll::Pending { transitions });
                } else {
                    active.encoder.poll(runtime, fuel - transitions)?
                }
            };
            match polled {
                M11HotInlineSidecarSnapshotPoll::Pending {
                    transitions: consumed,
                } => {
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    return Ok(CandidatePoll::Pending { transitions });
                }
                M11HotInlineSidecarSnapshotPoll::Frame {
                    transitions: consumed,
                    frame,
                } => {
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    let wrapped_len = VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES
                        .checked_add(frame.bytes.len())
                        .ok_or(CandidateEndpointError::MetricOverflow)?;
                    if !self.packet.can_accept_with_frame_limit(
                        wrapped_len,
                        maximum_packet_bytes,
                        maximum_frame_bytes,
                    )? {
                        if self.packet.frames.is_empty() || self.lookahead.is_some() {
                            return Err(CandidateEndpointError::InvalidState);
                        }
                        self.lookahead = Some(frame);
                        return Ok(CandidatePoll::ViewportPresentationEvent {
                            transitions,
                            event: Box::new(self.take_packet_event()?),
                        });
                    }
                    self.append_child_frame(frame, wrapped_len)?;
                    if self.packet.saturated(maximum_packet_bytes)? {
                        return Ok(CandidatePoll::ViewportPresentationEvent {
                            transitions,
                            event: Box::new(self.take_packet_event()?),
                        });
                    }
                    if transitions == fuel {
                        return Ok(CandidatePoll::Pending { transitions });
                    }
                }
            }
        }
    }

    fn append_child_frame(
        &mut self,
        frame: M11HotInlineSidecarFrame,
        wrapped_len: usize,
    ) -> Result<(), CandidateEndpointError> {
        let active = self
            .active
            .as_mut()
            .ok_or(CandidateEndpointError::InvalidState)?;
        let child_frame_ordinal = active.next_frame_ordinal;
        let (kind, record_count) = match frame.kind {
            M11HotInlineSidecarFrameKind::Begin => (HotInlineSidecarFrameKind::Begin, 0),
            M11HotInlineSidecarFrameKind::Node => (HotInlineSidecarFrameKind::Node, 1),
            M11HotInlineSidecarFrameKind::End => (HotInlineSidecarFrameKind::End, 0),
        };
        if matches!(frame.kind, M11HotInlineSidecarFrameKind::Begin) != (child_frame_ordinal == 0)
            || frame.bytes.is_empty()
            || frame.bytes.len() > M11_MAX_SNAPSHOT_FRAME_BYTES
        {
            return Err(CandidateEndpointError::InvalidState);
        }
        if frame.kind == M11HotInlineSidecarFrameKind::Node {
            let node_ordinal = frame
                .node_ordinal
                .ok_or(CandidateEndpointError::InvalidState)?;
            if active
                .next_node_ordinal
                .is_some_and(|expected| expected != node_ordinal)
            {
                return Err(CandidateEndpointError::InvalidState);
            }
            active.next_node_ordinal = Some(
                node_ordinal
                    .checked_add(1)
                    .ok_or(CandidateEndpointError::MetricOverflow)?,
            );
        } else if frame.node_ordinal.is_some() {
            return Err(CandidateEndpointError::InvalidState);
        }
        if frame.kind == M11HotInlineSidecarFrameKind::End && frame.root_stream_digest256.is_none()
            || frame.kind != M11HotInlineSidecarFrameKind::End
                && frame.root_stream_digest256.is_some()
        {
            return Err(CandidateEndpointError::InvalidState);
        }
        let mut wrapped = Vec::new();
        wrapped
            .try_reserve_exact(wrapped_len)
            .map_err(|_| CandidateEndpointError::AllocationFailed)?;
        wrapped.resize(wrapped_len, 0);
        let written = encode_viewport_presentation_child_frame_into(
            self.offer,
            ViewportPresentationChildFrameInput {
                directory_index: active.directory_index,
                child_frame_ordinal,
                kind,
                record_count,
                payload: &frame.bytes,
            },
            &mut wrapped,
        )?;
        if written != wrapped.len() {
            return Err(CandidateEndpointError::InvalidState);
        }
        active.next_frame_ordinal = active
            .next_frame_ordinal
            .checked_add(1)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let end = frame.kind == M11HotInlineSidecarFrameKind::End;
        self.actual_child_frame_count = self
            .actual_child_frame_count
            .checked_add(1)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        self.actual_child_encoded_bytes = self
            .actual_child_encoded_bytes
            .checked_add(
                u32::try_from(wrapped.len()).map_err(|_| CandidateEndpointError::MetricOverflow)?,
            )
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        self.append_outer_frame(
            ViewportPresentationFrameKind::Child,
            record_count,
            wrapped.into_boxed_slice(),
            false,
        )?;
        if end {
            let mut active = self
                .active
                .take()
                .ok_or(CandidateEndpointError::InvalidState)?;
            let directory = self
                .directory
                .get(
                    usize::try_from(active.directory_index)
                        .map_err(|_| CandidateEndpointError::MetricOverflow)?,
                )
                .ok_or(CandidateEndpointError::InvalidState)?;
            let expected_child_frames = directory
                .hio1_envelope
                .transferred_node_count
                .checked_add(2)
                .ok_or(CandidateEndpointError::MetricOverflow)?;
            if active.next_frame_ordinal != expected_child_frames {
                self.active = Some(active);
                return Err(CandidateEndpointError::InvalidAuthority);
            }
            drop(active.encoder);
            if let Some(root) = active.root.take() {
                self.releasing = Some(ReleasingViewportInlineRoot {
                    root,
                    authority: active.authority.take(),
                    begun: false,
                });
            } else {
                drop(active.authority.take());
            }
        }
        Ok(())
    }

    fn append_outer_frame(
        &mut self,
        kind: ViewportPresentationFrameKind,
        record_count: u32,
        bytes: Box<[u8]>,
        end: bool,
    ) -> Result<(), CandidateEndpointError> {
        let ordinal = self.next_frame_ordinal;
        let first_record_ordinal = self.next_record_ordinal;
        let next_frame_ordinal = ordinal
            .checked_add(1)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let next_record_ordinal = first_record_ordinal
            .checked_add(record_count)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let receipt = self
            .transport
            .as_ref()
            .ok_or(CandidateEndpointError::InvalidState)?
            .receipt();
        if receipt
            .encoded_frame_bytes
            .checked_add(
                u32::try_from(bytes.len()).map_err(|_| CandidateEndpointError::MetricOverflow)?,
            )
            .ok_or(CandidateEndpointError::MetricOverflow)?
            > self.offer.limits.maximum_encoded_frame_bytes
            || next_frame_ordinal > self.offer.limits.maximum_frame_count
        {
            return Err(CandidateEndpointError::ViewportInlineLimitExceeded(
                "publication transport",
            ));
        }
        let digest256 = self
            .transport
            .as_mut()
            .ok_or(CandidateEndpointError::InvalidState)?
            .push(ordinal, first_record_ordinal, record_count, kind, &bytes)?;
        self.packet.push(
            ordinal,
            first_record_ordinal,
            record_count,
            protocol_digest128_from_blake3(
                ProtocolDigestDomain::ViewportPresentationFrame,
                digest256,
            ),
            bytes,
            end,
        )?;
        self.next_frame_ordinal = next_frame_ordinal;
        self.next_record_ordinal = next_record_ordinal;
        Ok(())
    }

    fn finish_frame_stream(&mut self) -> Result<(), CandidateEndpointError> {
        let expected_records = self
            .offer
            .envelope
            .ordered_leaf_count
            .checked_add(self.offer.envelope.transferred_node_count)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        if self.next_record_ordinal != expected_records
            || self.next_frame_ordinal != self.offer.limits.maximum_frame_count
            || self.active.is_some()
            || !self.pending.is_empty()
            || self.releasing.is_some()
        {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let transport = self
            .transport
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?
            .finish()?;
        if transport.frame_count != self.next_frame_ordinal
            || transport.encoded_frame_bytes > self.offer.limits.maximum_encoded_frame_bytes
        {
            return Err(CandidateEndpointError::InvalidState);
        }
        let root_digest256 = viewport_presentation_root_stream_digest256(
            self.offer.envelope.aggregate_envelope_digest256,
            transport,
        );
        let root_stream_digest = protocol_digest128_from_blake3(
            ProtocolDigestDomain::ViewportPresentationRootStream,
            root_digest256,
        );
        self.commit = Some(ViewportPresentationCommitRequest {
            offer_id: self.offer.offer_id,
            actual_frame_count: transport.frame_count,
            actual_encoded_frame_bytes: transport.encoded_frame_bytes,
            rolling_transport_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::ViewportPresentationTransport,
                transport.digest256,
            ),
            aggregate_root_stream_digest: root_stream_digest,
        });
        self.expected_ack = Some(ViewportPresentationAck {
            publication_session: self.offer.publication_session,
            base_ack: self.offer.base_ack,
            binding: self.offer.binding,
            envelope: self.offer.envelope,
            actual_frame_count: transport.frame_count,
            actual_encoded_frame_bytes: transport.encoded_frame_bytes,
            aggregate_root_stream_digest: root_stream_digest,
        });
        Ok(())
    }

    fn take_packet_event(
        &mut self,
    ) -> Result<CandidateViewportPresentationEvent, CandidateEndpointError> {
        let packet = std::mem::take(&mut self.packet);
        let first_frame_ordinal = packet
            .first_frame_ordinal
            .ok_or(CandidateEndpointError::InvalidState)?;
        let frame_count = u32::try_from(packet.frames.len())
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let end = packet.end;
        let encoded = packet.encode(self.offer.offer_id)?;
        self.phase = StreamPhase::AwaitPacketReceipt {
            first_frame_ordinal,
            frame_count,
            end,
        };
        Ok(CandidateViewportPresentationEvent {
            credit: ViewportPresentationCredit::Packet {
                first_frame_ordinal,
                frame_count,
                end,
            },
            body: CandidateViewportPresentationEventBody::Packet { encoded },
        })
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

fn begin_exact_candidate_build(
    runtime: &mut DocumentRuntime,
    context: CandidateContext,
    mut base: ExactCandidateBase,
    witness: Box<PersistentSourceFactsDeltaWitness>,
    mut result: M11LeadingReferencesCropResult,
) -> Result<ActiveCandidate, ExactBuildStartFailure> {
    let base_restart = match result.take_base_restart_checkpoint() {
        Ok(checkpoint) => checkpoint,
        Err(_) => {
            return Err(ExactBuildStartFailure {
                error: CandidateEndpointError::InvalidState,
                base,
            });
        }
    };
    base.restart = Some(CandidateRestartAuthority::Leading(base_restart));
    let next_restart = match result.take_next_restart_checkpoint() {
        Ok(checkpoint) => checkpoint,
        Err(_) => {
            return Err(ExactBuildStartFailure {
                error: CandidateEndpointError::InvalidState,
                base,
            });
        }
    };
    let input = match result.into_exact_segmented_candidate_input() {
        Ok(input) => input,
        Err(error) => {
            return Err(ExactBuildStartFailure {
                error: error.into(),
                base,
            });
        }
    };
    let next_restart = CandidateRestartAuthority::Leading(next_restart);
    begin_exact_candidate_build_from_terminal(runtime, context, base, witness, input, next_restart)
}

fn select_ordinary_crop_route(
    checkpoints: &M11OrdinaryParagraphRestartCheckpoints,
    base_byte_range: std::ops::Range<usize>,
) -> Result<Option<OrdinaryCropRoute>, CandidateEndpointError> {
    if base_byte_range.start == 0 && base_byte_range.end == checkpoints.source().byte_len() {
        return Ok(None);
    }
    match checkpoints.select_crop(base_byte_range.clone()) {
        Ok(selection) => Ok(Some(OrdinaryCropRoute::Interior(selection))),
        Err(M11OrdinaryParagraphCropPlanError::NoRestartCheckpoint) => {
            // The exact edit can sit inside the first Paragraph even though
            // the only valid parser crop begins at BOF. Extend only the
            // parser-selected boundary lane; the SourceFacts page range and
            // exact edit envelope remain independently authoritative.
            match checkpoints.select_bof_crop(0..base_byte_range.end) {
                Ok(selection) => Ok(Some(OrdinaryCropRoute::FromBof(selection))),
                Err(
                    M11OrdinaryParagraphBoundaryCropPlanError::SegmentedTopLevelIneligible
                    | M11OrdinaryParagraphBoundaryCropPlanError::FrozenReferencesIneligible
                    | M11OrdinaryParagraphBoundaryCropPlanError::NoConvergenceCheckpoint
                    | M11OrdinaryParagraphBoundaryCropPlanError::WholeSourceIneligible,
                ) => Ok(None),
                Err(M11OrdinaryParagraphBoundaryCropPlanError::InvalidChangedRange) => {
                    Err(CandidateEndpointError::InvalidAuthority)
                }
                Err(
                    M11OrdinaryParagraphBoundaryCropPlanError::InvalidCheckpoint
                    | M11OrdinaryParagraphBoundaryCropPlanError::SelectionMismatch
                    | M11OrdinaryParagraphBoundaryCropPlanError::NotBofBoundary
                    | M11OrdinaryParagraphBoundaryCropPlanError::NotEofBoundary
                    | M11OrdinaryParagraphBoundaryCropPlanError::NoRestartCheckpoint,
                ) => Err(CandidateEndpointError::InvalidState),
            }
        }
        Err(M11OrdinaryParagraphCropPlanError::NoConvergenceCheckpoint) => {
            // Symmetrically, an edit inside the final Paragraph requires a
            // restart-to-EOF crop even when the exact edit does not touch the
            // final source byte.
            match checkpoints
                .select_eof_crop(base_byte_range.start..checkpoints.source().byte_len())
            {
                Ok(selection) => Ok(Some(OrdinaryCropRoute::ToEof(selection))),
                Err(
                    M11OrdinaryParagraphBoundaryCropPlanError::SegmentedTopLevelIneligible
                    | M11OrdinaryParagraphBoundaryCropPlanError::FrozenReferencesIneligible
                    | M11OrdinaryParagraphBoundaryCropPlanError::NoRestartCheckpoint
                    | M11OrdinaryParagraphBoundaryCropPlanError::WholeSourceIneligible,
                ) => Ok(None),
                Err(M11OrdinaryParagraphBoundaryCropPlanError::InvalidChangedRange) => {
                    Err(CandidateEndpointError::InvalidAuthority)
                }
                Err(
                    M11OrdinaryParagraphBoundaryCropPlanError::InvalidCheckpoint
                    | M11OrdinaryParagraphBoundaryCropPlanError::SelectionMismatch
                    | M11OrdinaryParagraphBoundaryCropPlanError::NotBofBoundary
                    | M11OrdinaryParagraphBoundaryCropPlanError::NotEofBoundary
                    | M11OrdinaryParagraphBoundaryCropPlanError::NoConvergenceCheckpoint,
                ) => Err(CandidateEndpointError::InvalidState),
            }
        }
        Err(M11OrdinaryParagraphCropPlanError::InvalidChangedRange) => {
            Err(CandidateEndpointError::InvalidAuthority)
        }
        Err(
            M11OrdinaryParagraphCropPlanError::InvalidCheckpoint
            | M11OrdinaryParagraphCropPlanError::SelectionMismatch,
        ) => Err(CandidateEndpointError::InvalidState),
    }
}

fn segmented_crop_exceeds_byte_cap(
    checkpoints: &M11OrdinaryParagraphRestartCheckpoints,
    selection: M11OrdinaryParagraphCropSelection,
    target_crop_start: usize,
    target_suffix_start: usize,
) -> Result<bool, CandidateEndpointError> {
    if !selection.is_segmented_top_level() {
        return Ok(false);
    }
    let convergence = checkpoints
        .checkpoints()
        .get(selection.convergence_index())
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
    if convergence.source() != selection.source()
        || convergence.binding() != selection.binding()
        || convergence.paragraph_source_start_byte() != selection.convergence_suffix_start_byte()
        || convergence.paragraph_source_start_utf16() != selection.convergence_suffix_start_utf16()
    {
        return Err(CandidateEndpointError::InvalidAuthority);
    }
    let line_offset = usize::try_from(
        convergence
            .preceding_line_start_byte()
            .checked_sub(convergence.paragraph_source_start_byte())
            .ok_or(CandidateEndpointError::InvalidAuthority)?,
    )
    .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let target_line_end = target_suffix_start
        .checked_add(line_offset)
        .and_then(|start| {
            start.checked_add(usize::try_from(convergence.preceding_line_physical_bytes()).ok()?)
        })
        .ok_or(CandidateEndpointError::MetricOverflow)?;
    let target_crop_bytes = target_line_end
        .checked_sub(target_crop_start)
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
    Ok(target_crop_bytes > M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES)
}

fn segmented_bof_crop_exceeds_byte_cap(
    checkpoints: &M11OrdinaryParagraphRestartCheckpoints,
    selection: M11OrdinaryParagraphBofCropSelection,
    target_suffix_start: usize,
) -> Result<bool, CandidateEndpointError> {
    if !selection.is_segmented_top_level() {
        return Ok(false);
    }
    let convergence = checkpoints
        .checkpoints()
        .get(selection.convergence_index())
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
    if convergence.source() != selection.source()
        || convergence.binding() != selection.binding()
        || convergence.paragraph_source_start_byte() != selection.convergence_suffix_start_byte()
        || convergence.paragraph_source_start_utf16() != selection.convergence_suffix_start_utf16()
        || convergence.block_entry_ordinal() != selection.convergence_block_entry_ordinal()
    {
        return Err(CandidateEndpointError::InvalidAuthority);
    }
    let line_offset = usize::try_from(
        convergence
            .preceding_line_start_byte()
            .checked_sub(convergence.paragraph_source_start_byte())
            .ok_or(CandidateEndpointError::InvalidAuthority)?,
    )
    .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let target_crop_end = target_suffix_start
        .checked_add(line_offset)
        .and_then(|start| {
            start.checked_add(usize::try_from(convergence.preceding_line_physical_bytes()).ok()?)
        })
        .ok_or(CandidateEndpointError::MetricOverflow)?;
    Ok(target_crop_end > M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES)
}

fn segmented_eof_crop_exceeds_byte_cap(
    checkpoints: &M11OrdinaryParagraphRestartCheckpoints,
    selection: M11OrdinaryParagraphEofCropSelection,
    target_crop_start: usize,
    target_eof: usize,
) -> Result<bool, CandidateEndpointError> {
    if !selection.is_segmented_top_level() {
        return Ok(false);
    }
    let restart = checkpoints
        .checkpoints()
        .get(selection.restart_index())
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
    if restart.source() != selection.source()
        || restart.binding() != selection.binding()
        || restart.prefix_end_byte() != selection.restart_prefix_end_byte()
        || restart.prefix_end_utf16() != selection.restart_prefix_end_utf16()
        || restart.block_entry_ordinal() != selection.restart_block_entry_ordinal()
        || checkpoints.top_level_block_count() != selection.base_block_entry_count()
    {
        return Err(CandidateEndpointError::InvalidAuthority);
    }
    let target_crop_bytes = target_eof
        .checked_sub(target_crop_start)
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
    Ok(target_crop_bytes > M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES)
}

fn target_physical_line_cut_is_exact(
    target: &SourceSnapshotLease,
    byte_start: usize,
    utf16_start: usize,
) -> Result<bool, CandidateEndpointError> {
    let is_line_start = target
        .is_physical_line_start(byte_start)
        .map_err(|_| CandidateEndpointError::InvalidAuthority)?;
    let observed_utf16 = target
        .utf16_offset_for_byte(byte_start)
        .map_err(|_| CandidateEndpointError::InvalidAuthority)?;
    let observed_byte = target
        .byte_offset_for_utf16(utf16_start)
        .map_err(|_| CandidateEndpointError::InvalidAuthority)?;
    Ok(is_line_start && observed_utf16 == utf16_start && observed_byte == byte_start)
}

fn leading_crop_declined_semantically(error: &M11LeadingReferencesCropError) -> bool {
    matches!(
        error,
        M11LeadingReferencesCropError::CropAcceptedDefinition
            | M11LeadingReferencesCropError::Unknown(_)
    )
}

fn ordinary_crop_declined_semantically(error: &CandidateEndpointError) -> bool {
    matches!(
        error,
        CandidateEndpointError::OrdinaryCrop(M11OrdinaryParagraphCropError::CropDiverged)
            | CandidateEndpointError::OrdinaryBoundaryCrop(
                M11OrdinaryParagraphBoundaryCropError::CropDiverged
            )
    )
}

fn take_candidate_restart_authority(
    result: &mut flark_parser::M11CleanDocumentResult,
    parser_binding: M11ParserBinding,
) -> Result<Option<CandidateRestartAuthority>, CandidateEndpointError> {
    match result.take_leading_references_restart_checkpoint(parser_binding) {
        Ok(restart) => Ok(Some(CandidateRestartAuthority::Leading(restart))),
        Err(LeadingReferencesCheckpointError::Ineligible) => {
            match result.take_ordinary_paragraph_restart_checkpoints(parser_binding) {
                Ok(restarts) => Ok(Some(CandidateRestartAuthority::Ordinary(restarts))),
                Err(M11OrdinaryParagraphCheckpointError::Ineligible) => {
                    Ok(Some(CandidateRestartAuthority::ExactBaseOnly {
                        source: result.source_version(),
                        binding: parser_binding,
                    }))
                }
                Err(M11OrdinaryParagraphCheckpointError::AllocationFailed) => {
                    Err(CandidateEndpointError::AllocationFailed)
                }
                Err(M11OrdinaryParagraphCheckpointError::AlreadyTaken) => {
                    Err(CandidateEndpointError::InvalidState)
                }
            }
        }
        Err(LeadingReferencesCheckpointError::AlreadyTaken) => {
            Err(CandidateEndpointError::InvalidState)
        }
    }
}

fn plan_exact_clean_block_splice(
    runtime: &DocumentRuntime,
    base: &M11RetainedCandidatePublication,
    base_restart: Option<&CandidateRestartAuthority>,
    witness: &PersistentSourceFactsDeltaWitness,
    target: &M11CleanDocumentResult,
) -> Result<Option<M11BlockSequenceSpliceSelection>, CandidateEndpointError> {
    if target.source_version() != witness.target()
        || witness.base_byte_range().is_empty()
        || witness.target_byte_range().is_empty()
        || target.leaves().is_empty()
    {
        return Ok(None);
    }

    // `Before` at the changed-page start and `After` at its end deliberately
    // include neighboring leaves when either cut lands on a block boundary.
    // This trades a little transfer width for a simpler, fail-closed seam.
    let base_first = base
        .locate_exact_base_block_byte(
            runtime,
            witness.base_byte_range().start,
            SourceBoundaryAffinity::Before,
        )?
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
    let base_last = base
        .locate_exact_base_block_byte(
            runtime,
            witness.base_byte_range().end,
            SourceBoundaryAffinity::After,
        )?
        .ok_or(CandidateEndpointError::InvalidAuthority)?;
    if base_first.entry_ordinal() > base_last.entry_ordinal()
        || base_first.byte_range().start
            > u64::try_from(witness.base_byte_range().start)
                .map_err(|_| CandidateEndpointError::MetricOverflow)?
        || base_last.byte_range().end
            < u64::try_from(witness.base_byte_range().end)
                .map_err(|_| CandidateEndpointError::MetricOverflow)?
    {
        return Ok(None);
    }
    let base_affected_entries = base_last
        .entry_ordinal()
        .checked_sub(base_first.entry_ordinal())
        .and_then(|count| count.checked_add(1))
        .ok_or(CandidateEndpointError::MetricOverflow)?;
    if base_affected_entries > EXACT_CLEAN_BLOCK_SPLICE_MAX_AFFECTED_ENTRIES {
        return Ok(None);
    }
    if base_first.byte_range().start == 0
        && base_last.byte_range().end
            == u64::try_from(witness.base().byte_len())
                .map_err(|_| CandidateEndpointError::MetricOverflow)?
    {
        // A whole-block-root replacement has no packed page to preserve.
        return Ok(None);
    }

    let Some(target_first_index) = clean_leaf_index_at(
        target.leaves(),
        witness.target_byte_range().start,
        witness.target().byte_len(),
        SourceBoundaryAffinity::Before,
    ) else {
        return Ok(None);
    };
    let Some(target_last_index) = clean_leaf_index_at(
        target.leaves(),
        witness.target_byte_range().end,
        witness.target().byte_len(),
        SourceBoundaryAffinity::After,
    ) else {
        return Ok(None);
    };
    if target_first_index > target_last_index {
        return Ok(None);
    }
    let target_affected_entries = target_last_index
        .checked_sub(target_first_index)
        .and_then(|count| count.checked_add(1))
        .ok_or(CandidateEndpointError::MetricOverflow)?;
    if u64::try_from(target_affected_entries).map_err(|_| CandidateEndpointError::MetricOverflow)?
        > EXACT_CLEAN_BLOCK_SPLICE_MAX_AFFECTED_ENTRIES
    {
        return Ok(None);
    }
    let target_first = &target.leaves()[target_first_index];
    let target_last = &target.leaves()[target_last_index];
    let target_first_source = target_first.source_range();
    let target_last_source = target_last.source_range();
    let target_first_byte = usize::try_from(target_first_source.start)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let target_last_byte = usize::try_from(target_last_source.end)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    if target_first_byte > witness.target_byte_range().start
        || target_last_byte < witness.target_byte_range().end
    {
        return Ok(None);
    }

    let target_start_ordinal =
        u64::try_from(target_first_index).map_err(|_| CandidateEndpointError::MetricOverflow)?;
    if base_first.entry_ordinal() != target_start_ordinal {
        return Ok(None);
    }

    let mut base_reference_definitions = 0_u64;
    let mut base_location = base_first.clone();
    loop {
        if base_location.entry().kind() == M11BlockSequenceEntryKind::Unsupported {
            return Ok(None);
        }
        base_reference_definitions = base_reference_definitions
            .checked_add(base_location.entry().reference_definition_count())
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        if base_location.entry_ordinal() == base_last.entry_ordinal() {
            break;
        }
        let next_byte = usize::try_from(base_location.byte_range().end)
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let next = base
            .locate_exact_base_block_byte(runtime, next_byte, SourceBoundaryAffinity::After)?
            .ok_or(CandidateEndpointError::InvalidAuthority)?;
        if next.entry_ordinal()
            != base_location
                .entry_ordinal()
                .checked_add(1)
                .ok_or(CandidateEndpointError::MetricOverflow)?
        {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        base_location = next;
    }
    if base_reference_definitions != 0
        || target.leaves()[target_first_index..=target_last_index]
            .iter()
            .any(|leaf| {
                leaf.reference_definition_count() != 0
                    || matches!(leaf, M11CleanLeaf::Unsupported { .. })
            })
    {
        return Ok(None);
    }
    if target.definition_count() != 0
        && !matches!(base_restart, Some(CandidateRestartAuthority::Leading(_)))
        && (witness.base().byte_len() != witness.target().byte_len()
            || witness.base().utf16_len() != witness.target().utf16_len())
    {
        // Canonical reference records currently own absolute byte and UTF-16
        // ranges. An unchanged suffix witness proves source identity, but a
        // nonzero coordinate delta still shifts every later definition. Only
        // a leading-reference checkpoint proves all retained definitions lie
        // in the exact unchanged prefix. Other definition-bearing documents
        // must rebuild References until the persistent reference index can
        // splice and rebase them explicitly.
        return Ok(None);
    }

    let base_prefix_end = usize::try_from(base_first.byte_range().start)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let base_prefix_utf16_end = usize::try_from(base_first.utf16_range().start)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let target_prefix_end = usize::try_from(target_first_source.start)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let target_prefix_utf16_end = usize::try_from(target_first.source_utf16_range().start)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    if base_prefix_end == 0 {
        if target_prefix_end != 0 || target_prefix_utf16_end != 0 {
            return Ok(None);
        }
    } else {
        let prefix = match runtime.mint_exact_unchanged_prefix_witness(
            witness.base(),
            base_prefix_end,
            base_prefix_utf16_end,
        ) {
            Ok(prefix) => prefix,
            Err(DocumentRuntimeError::ExactUnchangedPrefixLineageUnavailable) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let prefix = match runtime.take_exact_unchanged_prefix_witness(prefix) {
            Ok(prefix) => prefix,
            Err(DocumentRuntimeError::ExactUnchangedPrefixStale) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if prefix.target() != witness.target()
            || prefix.byte_end() != target_prefix_end
            || prefix.utf16_end() != target_prefix_utf16_end
        {
            return Ok(None);
        }
    }

    let base_suffix_start = usize::try_from(base_last.byte_range().end)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let base_suffix_utf16_start = usize::try_from(base_last.utf16_range().end)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let target_suffix_start = usize::try_from(target_last_source.end)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let target_suffix_utf16_start = usize::try_from(target_last.source_utf16_range().end)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    if base_suffix_start == witness.base().byte_len() {
        if target_suffix_start != witness.target().byte_len()
            || target_suffix_utf16_start != witness.target().utf16_len()
        {
            return Ok(None);
        }
    } else {
        let suffix = match runtime.mint_exact_unchanged_suffix_witness(
            witness.base(),
            base_suffix_start,
            base_suffix_utf16_start,
        ) {
            Ok(suffix) => suffix,
            Err(DocumentRuntimeError::ExactUnchangedSuffixLineageUnavailable) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let suffix = match runtime.take_exact_unchanged_suffix_witness(suffix) {
            Ok(suffix) => suffix,
            Err(DocumentRuntimeError::ExactUnchangedSuffixStale) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if suffix.target() != witness.target()
            || suffix.target_byte_start() != target_suffix_start
            || suffix.target_utf16_start() != target_suffix_utf16_start
        {
            return Ok(None);
        }
    }

    let base_end = base_last
        .entry_ordinal()
        .checked_add(1)
        .ok_or(CandidateEndpointError::MetricOverflow)?;
    let target_end = u64::try_from(
        target_last_index
            .checked_add(1)
            .ok_or(CandidateEndpointError::MetricOverflow)?,
    )
    .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let selection = M11BlockSequenceSpliceSelection::new(
        base_first.entry_ordinal()..base_end,
        target_start_ordinal..target_end,
    )
    .map_err(M11CandidateDerivationError::from)?;
    Ok(Some(selection))
}

fn clean_leaf_index_at(
    leaves: &[M11CleanLeaf],
    byte_offset: usize,
    source_bytes: usize,
    affinity: SourceBoundaryAffinity,
) -> Option<usize> {
    if leaves.is_empty() || source_bytes == 0 || byte_offset > source_bytes {
        return None;
    }
    let probe = match affinity {
        SourceBoundaryAffinity::Before if byte_offset > 0 => byte_offset - 1,
        SourceBoundaryAffinity::Before => 0,
        SourceBoundaryAffinity::After if byte_offset < source_bytes => byte_offset,
        SourceBoundaryAffinity::After => source_bytes - 1,
    };
    let probe = u32::try_from(probe).ok()?;
    let index = leaves.partition_point(|leaf| leaf.source_range().end <= probe);
    let range = leaves.get(index)?.source_range();
    (range.start <= probe && probe < range.end).then_some(index)
}

fn begin_exact_clean_fallback(
    runtime: &DocumentRuntime,
    context: CandidateContext,
    base: ExactCandidateBase,
    witness: Box<PersistentSourceFactsDeltaWitness>,
) -> Result<ActiveCandidate, ExactBuildStartFailure> {
    let certified = match runtime.certify_current_persistent_source() {
        Ok(certified) => certified,
        Err(error) => {
            return Err(ExactBuildStartFailure {
                error: error.into(),
                base,
            });
        }
    };
    if certified.source() != witness.target()
        || certified.parser_profile() != witness.parser_profile()
        || certified.source_facts_profile() != witness.profile()
    {
        return Err(ExactBuildStartFailure {
            error: CandidateEndpointError::InvalidAuthority,
            base,
        });
    }
    let job = match M11CleanParseJob::new(certified.exact_parse_lease()) {
        Ok(job) => job,
        Err(error) => {
            return Err(ExactBuildStartFailure {
                error: error.into(),
                base,
            });
        }
    };
    Ok(ActiveCandidate::ParsingExactFallback(Box::new(
        ParsingExactFallbackCandidate {
            context,
            certified,
            job,
            base,
            witness,
        },
    )))
}

fn begin_exact_candidate_build_ordinary(
    runtime: &mut DocumentRuntime,
    context: CandidateContext,
    mut base: ExactCandidateBase,
    witness: Box<PersistentSourceFactsDeltaWitness>,
    mut result: OrdinaryExactResult,
) -> Result<ActiveCandidate, ExactBuildStartFailure> {
    let base_restart = match result.take_base_restart_checkpoints() {
        Ok(checkpoints) => checkpoints,
        Err(_) => {
            return Err(ExactBuildStartFailure {
                error: CandidateEndpointError::InvalidState,
                base,
            });
        }
    };
    base.restart = Some(CandidateRestartAuthority::Ordinary(base_restart));
    let next_restart = match result.take_next_restart_checkpoints() {
        Ok(checkpoints) => checkpoints,
        Err(_) => {
            return Err(ExactBuildStartFailure {
                error: CandidateEndpointError::InvalidState,
                base,
            });
        }
    };
    let input = match result.into_exact_segmented_candidate_input() {
        Ok(input) => input,
        Err(error) => {
            return Err(ExactBuildStartFailure {
                error: error.into(),
                base,
            });
        }
    };
    let next_restart = CandidateRestartAuthority::Ordinary(next_restart);
    begin_exact_candidate_build_from_terminal(runtime, context, base, witness, input, next_restart)
}

#[allow(clippy::too_many_arguments)]
fn begin_exact_candidate_build_from_terminal(
    runtime: &mut DocumentRuntime,
    context: CandidateContext,
    base: ExactCandidateBase,
    witness: Box<PersistentSourceFactsDeltaWitness>,
    input: M11ExactSegmentedCandidateInput,
    next_restart: CandidateRestartAuthority,
) -> Result<ActiveCandidate, ExactBuildStartFailure> {
    if input.source() != witness.target() {
        return Err(ExactBuildStartFailure {
            error: CandidateEndpointError::InvalidAuthority,
            base,
        });
    }
    let candidate = match M11ParserCandidate::derive_segmented_reusing_references(
        input,
        witness.parser_profile(),
        witness.profile(),
    ) {
        Ok(candidate) => candidate,
        Err(error) => {
            return Err(ExactBuildStartFailure {
                error: error.into(),
                base,
            });
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
            return Err(ExactBuildStartFailure {
                error: error.into(),
                base,
            });
        }
    };
    Ok(ActiveCandidate::BuildingExact {
        context,
        writer: Box::new(writer),
        base,
        witness,
        next_restart,
        structural_path: ExactStructuralPath::LegacyBlocks,
    })
}

fn offer_begin(
    context: CandidateContext,
    descriptor: M11CandidateDescriptor,
) -> Result<OfferBegin, CandidateEndpointError> {
    offer_begin_with_mode(
        context,
        descriptor,
        PublicationMode::FullSnapshot,
        None,
        descriptor.canonical_record_count,
    )
}

fn offer_begin_exact(
    context: CandidateContext,
    descriptor: M11CandidateDescriptor,
    transferred_record_count: u64,
    base_ack: StructuralAck,
) -> Result<OfferBegin, CandidateEndpointError> {
    offer_begin_with_mode(
        context,
        descriptor,
        PublicationMode::ExactBaseDelta,
        Some(base_ack),
        transferred_record_count,
    )
}

fn offer_begin_with_mode(
    context: CandidateContext,
    descriptor: M11CandidateDescriptor,
    mode: PublicationMode,
    base_ack: Option<StructuralAck>,
    transferred_record_count: u64,
) -> Result<OfferBegin, CandidateEndpointError> {
    if descriptor.document != document_bytes(context.binding.document_session)
        || descriptor.source_revision != u64::from(context.completion.worker_replica_revision)
        || descriptor.source_bytes != u64::from(context.completion.utf8_length)
        || descriptor.source_utf16 != u64::from(context.completion.utf16_length)
        || descriptor.parse_generation != u64::from(context.parse_generation)
        || descriptor.syntax_profile == 0
    {
        return Err(CandidateEndpointError::InvalidAuthority);
    }
    let target_record_count = u32::try_from(descriptor.canonical_record_count)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let transferred_record_count = u32::try_from(transferred_record_count)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let maximum_frame_count = u32::try_from(descriptor.maximum_snapshot_frames)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let maximum_encoded_frame_bytes = u32::try_from(descriptor.maximum_snapshot_encoded_bytes)
        .map_err(|_| CandidateEndpointError::MetricOverflow)?;
    let source_version = SourceVersion {
        document_session: context.binding.document_session,
        revision: context.completion.ui_revision,
        utf8_length: context.completion.utf8_length,
        utf16_length: context.completion.utf16_length,
        content_hash128: context.completion.content_hash128,
    };
    Ok(OfferBegin {
        schema: MANIFEST_SCHEMA,
        offer_id: digest_words(derive_identity(
            b"offer",
            context.binding,
            context.completion,
            context.parse_generation,
        )),
        publication_session: digest_words(descriptor.publication),
        target_host_revision: context.parse_generation,
        source_version,
        source_root: split_u64(descriptor.source_root),
        parse_generation: context.parse_generation,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: descriptor.syntax_profile,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        mode,
        base_ack,
        transferred_record_count,
        target_record_count,
        limits: OfferLimits {
            maximum_frame_count,
            maximum_encoded_frame_bytes,
            maximum_packet_bytes: u32::try_from(MAXIMUM_PACKET_ENCODED_BYTES)
                .map_err(|_| CandidateEndpointError::MetricOverflow)?,
            // A protocol record is carried inside a complete snapshot Node
            // frame, so this ceiling includes the engine frame header.
            maximum_frame_bytes: u32::try_from(M11_MAX_SNAPSHOT_FRAME_BYTES)
                .map_err(|_| CandidateEndpointError::MetricOverflow)?,
            maximum_program_children: u32::try_from(M11_MAX_ROLE_RECORDS)
                .map_err(|_| CandidateEndpointError::MetricOverflow)?,
        },
    })
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

fn encode_not_inline_leaf_metadata(kind: M11BlockSequenceEntryKind) -> Box<[u8]> {
    let mut encoded = [0_u8; 12];
    encoded[0..4].copy_from_slice(b"HUN1");
    encoded[4..8].copy_from_slice(&1_u32.to_le_bytes());
    encoded[8] = kind as u8;
    encoded.into()
}

fn derive_hot_inline_identity(
    domain: &[u8],
    command: InlineRefinementCommand,
    block_ordinal: u64,
) -> [u32; 4] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.v3.hot-inline-sidecar.identity.v1\0");
    hasher.update(domain);
    hasher.update(&[0]);
    for word in command.binding.document_session {
        hasher.update(&word.to_le_bytes());
    }
    hasher.update(&command.binding.source_session_identity.to_le_bytes());
    hasher.update(&command.binding.worker_generation.to_le_bytes());
    hasher.update(&command.refinement_generation.to_le_bytes());
    hasher.update(&block_ordinal.to_le_bytes());
    hasher.update(&command.byte_offset.to_le_bytes());
    hasher.update(&command.utf16_offset.to_le_bytes());
    hasher.update(&[match command.affinity {
        InlinePointAffinity::Before => 1,
        InlinePointAffinity::After => 2,
    }]);
    hash_source_version(&mut hasher, command.source_version);
    hash_structural_ack(&mut hasher, command.base_ack);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    let mut identity = digest_words(bytes);
    if identity == [0; 4] {
        identity[0] = 1;
    }
    identity
}

fn derive_viewport_identity(domain: &[u8], command: ViewportInlineBatchCommand) -> [u32; 4] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.v3.viewport-presentation.identity.v1\0");
    hasher.update(domain);
    hasher.update(&[0]);
    for word in command.binding.document_session {
        hasher.update(&word.to_le_bytes());
    }
    hasher.update(&command.binding.source_session_identity.to_le_bytes());
    hasher.update(&command.binding.worker_generation.to_le_bytes());
    hasher.update(&command.viewport_generation.to_le_bytes());
    hasher.update(&command.start_entry_ordinal.to_le_bytes());
    hasher.update(&command.start_byte_offset.to_le_bytes());
    hasher.update(&command.start_utf16_offset.to_le_bytes());
    hasher.update(&command.end_byte_offset.to_le_bytes());
    hasher.update(&command.end_utf16_offset.to_le_bytes());
    hash_source_version(&mut hasher, command.source_version);
    hash_structural_ack(&mut hasher, command.base_ack);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    let mut identity = digest_words(bytes);
    if identity == [0; 4] {
        identity[0] = 1;
    }
    identity
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
mod tests {
    use super::*;
    use crate::v3_endpoint::standard_document_runtime_config;
    use crate::v3_host_store::{
        HOST_M11_VIEWPORT_BYTES, HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES,
        HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES, HOST_RECURSIVE_GREEN_ROW_RANGE_SCHEMA,
        HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES, HostBlockRangeBudget, HostBlockRangeOutcome,
        HostBlockRangeQuery, HostConfig, HostInlineSidecarQueryOutcome, HostMetricAffinity,
        HostMetricRange, HostPointQuery, HostPollOutcome as NativeHostPollOutcome, HostQueryBudget,
        HostSourceMetric, HostStructuralOrdinalWindowBudget, HostStructuralOrdinalWindowOutcome,
        HostStructuralOrdinalWindowQuery, HostStructuralQueryOutcome,
        HostViewportPresentationPollOutcome as NativeViewportPresentationPollOutcome,
        HostWorkGrant, InlineSidecarHostPollOutcome as NativeInlineSidecarHostPollOutcome,
        NativeCandidateHost,
    };
    use crate::v3_publication_wire::{
        decode_viewport_presentation_child_frame, decode_viewport_presentation_directory,
        decode_viewport_presentation_end_frame, decode_viewport_presentation_parent_frame,
        viewport_presentation_frame_digest256,
    };
    use flark_engine::m11_host::{M11CandidateHost, M11HostFrameKind};
    use flark_engine::parser_internal::{
        M11CandidateBuild, M11CandidateBuildPoll, M11InlineProjectionKind, M11RoleRecords,
    };
    use flark_engine::{
        ParserProfileId, RuntimeSourceFactsPoll, SourceFactsRootLimits, SourceFactsScanProfile,
        SourceRevision, SourceSeedBuilder,
    };
    use flark_parser::M11_INLINE_FACT_RECORD_BYTES;
    const TEST_SOURCE: &str = "candidate packet test\n";

    fn test_streaming(source_fact_records: usize) -> (DocumentRuntime, StreamingCandidate) {
        let mut runtime = DocumentRuntime::new(TEST_SOURCE, standard_document_runtime_config())
            .expect("test runtime");
        let streaming = streaming_for_runtime(&mut runtime, source_fact_records, 1);
        (runtime, streaming)
    }

    fn streaming_for_runtime(
        runtime: &mut DocumentRuntime,
        source_fact_records: usize,
        generation: u32,
    ) -> StreamingCandidate {
        let utf16_length = TEST_SOURCE.encode_utf16().count();
        let mut seed =
            SourceSeedBuilder::new(SourceRevision::new(u64::from(generation)), utf16_length);
        seed.append_page(0..utf16_length, TEST_SOURCE)
            .expect("test source page");
        let source = seed.finalize().expect("test source");
        let records = M11RoleRecords::new(
            (0..source_fact_records).map(|ordinal| {
                vec![u8::try_from(ordinal & 0xff).expect("bounded ordinal")].into_boxed_slice()
            }),
            Box::<[u8]>::from(&b"green"[..]),
            Box::<[u8]>::from(&b"projection"[..]),
        )
        .expect("test role records");
        let publication_seed = u8::try_from(generation.checked_add(1).expect("bounded generation"))
            .expect("bounded generation");
        let mut build = M11CandidateBuild::new(
            runtime,
            [1; 16],
            [publication_seed; 16],
            source.version(),
            u64::from(generation),
            1,
            records,
        )
        .expect("test candidate build");
        build.finish_references(runtime).expect("finish references");
        while let M11CandidateBuildPoll::Pending { .. } =
            build.poll(runtime, 256).expect("candidate build poll")
        {}
        let publication = build.into_publication().expect("test publication");
        let descriptor = publication.descriptor(runtime).expect("test descriptor");
        let stream = Box::new(publication)
            .into_snapshot_stream(runtime)
            .expect("test snapshot stream");
        let record_count =
            u32::try_from(descriptor.canonical_record_count).expect("bounded test record count");
        StreamingCandidate {
            stream: Some(stream),
            sealed_publication: None,
            offer: OfferBegin {
                schema: MANIFEST_SCHEMA,
                offer_id: [generation; 4],
                publication_session: digest_words(descriptor.publication),
                target_host_revision: generation,
                source_version: SourceVersion {
                    document_session: [5, 6, 7, 8],
                    revision: generation,
                    utf8_length: u32::try_from(TEST_SOURCE.len()).expect("bounded source"),
                    utf16_length: u32::try_from(utf16_length).expect("bounded source"),
                    content_hash128: [generation; 4],
                },
                source_root: split_u64(descriptor.source_root),
                parse_generation: generation,
                grammar_revision: GRAMMAR_REVISION,
                syntax_profile: descriptor.syntax_profile,
                authority_mask: AUTHORITY_MASK_ALL_ROLES,
                mode: PublicationMode::FullSnapshot,
                base_ack: None,
                transferred_record_count: record_count,
                target_record_count: record_count,
                limits: OfferLimits {
                    maximum_frame_count: u32::try_from(descriptor.maximum_snapshot_frames)
                        .expect("bounded frames"),
                    maximum_encoded_frame_bytes: u32::try_from(
                        descriptor.maximum_snapshot_encoded_bytes,
                    )
                    .expect("bounded bytes"),
                    maximum_packet_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
                    maximum_frame_bytes: M11_MAX_SNAPSHOT_FRAME_BYTES as u32,
                    maximum_program_children: M11_MAX_ROLE_RECORDS as u32,
                },
            },
            descriptor,
            phase: StreamPhase::NeedPacket,
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
            next_restart: None,
            superseded_exact_base: None,
            exact_base_recovery: None,
        }
    }

    fn cancel_streaming_to_zero(runtime: DocumentRuntime, streaming: StreamingCandidate) {
        cancel_endpoint_to_zero(
            runtime,
            CandidateEndpoint {
                active: Some(ActiveCandidate::Streaming(Box::new(streaming))),
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
            },
        );
    }

    fn cancel_endpoint_to_zero(mut runtime: DocumentRuntime, mut endpoint: CandidateEndpoint) {
        endpoint.cancel().expect("cancel streaming candidate");
        for _ in 0..100_000 {
            if endpoint
                .poll_cleanup(&mut runtime, 1)
                .expect("bounded cleanup")
            {
                assert!(!endpoint.cleanup_pending());
                assert!(!endpoint.has_poll_work());
                runtime.begin_close().expect("begin runtime close");
                while !runtime.poll_close(256).expect("runtime close").complete {}
                assert_eq!(runtime.arena_metrics().resident_nodes, 0);
                return;
            }
        }
        panic!("streaming candidate did not reclaim to zero");
    }

    fn poll_to_packet_event(
        runtime: &DocumentRuntime,
        streaming: &mut StreamingCandidate,
        fuel: usize,
    ) -> (usize, CandidateEvent) {
        let mut pending_polls = 0;
        for _ in 0..100_000 {
            match streaming
                .poll_event(runtime, fuel)
                .expect("candidate packet poll")
            {
                CandidatePoll::Pending { transitions } => {
                    assert!(transitions <= fuel);
                    pending_polls += 1;
                }
                CandidatePoll::Event { transitions, event } => {
                    assert!(transitions <= fuel);
                    assert!(matches!(event.body, CandidateEventBody::Packet { .. }));
                    return (pending_polls, *event);
                }
                CandidatePoll::HotInlineEvent { .. } => {
                    panic!("structural stream emitted a hot-inline event")
                }
                CandidatePoll::ViewportPresentationEvent { .. } => {
                    panic!("structural stream emitted a viewport event")
                }
                CandidatePoll::ViewportPresentationUnavailable { .. } => {
                    panic!("structural stream emitted viewport unavailability")
                }
            }
        }
        panic!("candidate packet did not become available");
    }

    fn seal_stream(runtime: &DocumentRuntime, streaming: &mut StreamingCandidate) {
        loop {
            let (_, event) = poll_to_packet_event(runtime, streaming, 256);
            let CandidateCredit::Packet { end, .. } = event.credit else {
                panic!("snapshot traversal must emit a packet");
            };
            if end {
                assert!(streaming.stream.is_none());
                assert!(streaming.sealed_publication.is_some());
                return;
            }
            streaming.phase = StreamPhase::NeedPacket;
        }
    }

    fn deliver_stream(
        endpoint: &mut CandidateEndpoint,
        runtime: &DocumentRuntime,
        mut streaming: StreamingCandidate,
    ) -> StructuralAck {
        assert!(endpoint.active.is_none());
        seal_stream(runtime, &mut streaming);
        streaming.phase = StreamPhase::AwaitDeliveryReceipt;
        let ack = streaming.expected_ack.expect("sealed expected ACK");
        endpoint.active = Some(ActiveCandidate::Streaming(Box::new(streaming)));
        endpoint
            .accept_credit(CandidateCredit::Delivery, 1)
            .expect("accept exact delivery receipt");
        assert_eq!(
            endpoint.retained.as_ref().map(|retained| retained.ack),
            Some(ack)
        );
        ack
    }

    fn drain_candidate_cleanup(endpoint: &mut CandidateEndpoint, runtime: &mut DocumentRuntime) {
        drain_candidate_cleanup_with_fuel(endpoint, runtime, 1);
    }

    fn drain_candidate_cleanup_with_fuel(
        endpoint: &mut CandidateEndpoint,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) {
        assert!(fuel > 0);
        for _ in 0..100_000 {
            if !endpoint.cleanup_pending() {
                return;
            }
            endpoint
                .poll_cleanup(runtime, fuel)
                .expect("bounded candidate cleanup");
        }
        panic!("candidate cleanup did not complete");
    }

    fn install_persistent_source_facts(runtime: &mut DocumentRuntime) {
        runtime
            .begin_source_facts(
                SourceFactsScanProfile::new(4).expect("source-fact profile"),
                ParserProfileId::new(1).expect("parser profile"),
                SourceFactsRootLimits::default(),
            )
            .expect("begin persistent SourceFacts");
        loop {
            match runtime
                .poll_source_facts(128, 64)
                .expect("bounded SourceFacts poll")
            {
                RuntimeSourceFactsPoll::Pending(_)
                | RuntimeSourceFactsPoll::PromotionPending { .. }
                | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
                RuntimeSourceFactsPoll::Complete { .. } => break,
                RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
                | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                    panic!("clean SourceFacts job became incremental")
                }
            }
        }
        drop(
            runtime
                .take_certified_source()
                .expect("completed certification"),
        );
    }

    fn complete_clean_source_facts(
        runtime: &mut DocumentRuntime,
        profile: SourceFactsScanProfile,
        parser_profile: ParserProfileId,
        certification_id: u32,
        ui_revision: u32,
    ) -> (CertifiedSource, SourceFactsCompletionEvent) {
        runtime
            .begin_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
            .expect("begin clean SourceFacts");
        loop {
            match runtime
                .poll_source_facts(128, 64)
                .expect("bounded clean SourceFacts poll")
            {
                RuntimeSourceFactsPoll::Pending(_)
                | RuntimeSourceFactsPoll::PromotionPending { .. }
                | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
                RuntimeSourceFactsPoll::Complete { .. } => break,
                RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
                | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                    panic!("clean SourceFacts job became incremental")
                }
            }
        }
        let certified = runtime
            .take_certified_source()
            .expect("completed clean certification");
        let facts = certified.facts();
        let fingerprint = facts.fingerprint();
        let completion = SourceFactsCompletionEvent {
            certification_id,
            worker_replica_revision: u32::try_from(certified.source().revision().get())
                .expect("test revision"),
            ui_revision,
            utf16_length: u32::try_from(certified.source().utf16_len()).expect("test UTF-16"),
            intent_high_water: ui_revision,
            fingerprint_algorithm: fingerprint.algorithm(),
            utf8_length: u32::try_from(fingerprint.byte_len()).expect("test bytes"),
            logical_line_breaks: u32::try_from(facts.logical_line_breaks())
                .expect("test line breaks"),
            checkpoint_spacing_utf16: u32::try_from(facts.profile().checkpoint_spacing_utf16())
                .expect("test checkpoint spacing"),
            checkpoint_count: u32::try_from(facts.checkpoint_count())
                .expect("test checkpoint count"),
            page_count: u32::try_from(facts.page_count()).expect("test page count"),
            content_hash128: fingerprint.rolling_hash().words(),
            // Candidate authority consumes the content proof. The session
            // certification layer independently authenticates this page proof.
            checkpoint_hash128: [certification_id; 4],
        };
        (certified, completion)
    }

    fn complete_incremental_source_facts(
        runtime: &mut DocumentRuntime,
    ) -> Box<PersistentSourceFactsDeltaWitness> {
        loop {
            match runtime
                .poll_source_facts(128, 64)
                .expect("bounded incremental SourceFacts poll")
            {
                RuntimeSourceFactsPoll::Pending(_)
                | RuntimeSourceFactsPoll::PromotionPending { .. }
                | RuntimeSourceFactsPoll::IncrementalScanComplete { .. } => {}
                RuntimeSourceFactsPoll::IncrementalComplete { witness, .. } => return witness,
                RuntimeSourceFactsPoll::ScanComplete { .. }
                | RuntimeSourceFactsPoll::Complete { .. } => {
                    panic!("incremental SourceFacts job became clean")
                }
            }
        }
    }

    fn completion_for_persistent_target(
        runtime: &DocumentRuntime,
        certification_id: u32,
        ui_revision: u32,
    ) -> SourceFactsCompletionEvent {
        let facts = runtime
            .persistent_source_facts()
            .expect("persistent target facts");
        let summary = facts.summary();
        SourceFactsCompletionEvent {
            certification_id,
            worker_replica_revision: u32::try_from(facts.source().revision().get())
                .expect("test revision"),
            ui_revision,
            utf16_length: u32::try_from(summary.utf16_len()).expect("test UTF-16"),
            intent_high_water: ui_revision,
            fingerprint_algorithm: facts.profile().content_fingerprint_algorithm(),
            utf8_length: u32::try_from(summary.byte_len()).expect("test bytes"),
            logical_line_breaks: u32::try_from(summary.logical_line_breaks())
                .expect("test line breaks"),
            checkpoint_spacing_utf16: u32::try_from(facts.profile().checkpoint_spacing_utf16())
                .expect("test checkpoint spacing"),
            checkpoint_count: u32::try_from(facts.checkpoint_count())
                .expect("test checkpoint count"),
            page_count: u32::try_from(facts.page_count()).expect("test page count"),
            content_hash128: summary.rolling_hash().words(),
            checkpoint_hash128: [certification_id; 4],
        }
    }

    fn source_version_for(
        binding: SessionBinding,
        completion: SourceFactsCompletionEvent,
    ) -> SourceVersion {
        SourceVersion {
            document_session: binding.document_session,
            revision: completion.ui_revision,
            utf8_length: completion.utf8_length,
            utf16_length: completion.utf16_length,
            content_hash128: completion.content_hash128,
        }
    }

    fn assert_installed_candidate_has_no_inline(
        host: &NativeCandidateHost,
        source_version: SourceVersion,
    ) {
        let mut output = vec![0_u8; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version,
                    position: HostSourceMetric { bytes: 0, utf16: 0 },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                        maximum_open_depth: 1,
                        maximum_leaf_count: 3,
                        maximum_tree_nodes_visited: 3,
                    },
                },
                &mut output,
            )
            .expect("query installed exact candidate");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("exact candidate must expose its structural viewport: {outcome:?}");
        };
        assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
        assert_eq!(
            u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
            1,
            "canonical publication must not embed inline presentation authority"
        );
    }

    #[derive(Debug)]
    struct ExactDelivery {
        offer: OfferBegin,
        ack: StructuralAck,
        packet_frames: Vec<Vec<(CandidateSnapshotFrameKind, u32)>>,
        contains_recursive_green_leaf: bool,
        contains_recursive_green_branch: bool,
    }

    struct OrdinaryCancellationFixture {
        profile: SourceFactsScanProfile,
        parser_profile: ParserProfileId,
        binding: SessionBinding,
        base_source: String,
        base_version: flark_engine::SourceVersion,
        base_ack: StructuralAck,
        runtime: DocumentRuntime,
        endpoint: CandidateEndpoint,
        host: NativeCandidateHost,
    }

    impl OrdinaryCancellationFixture {
        fn new(document_session: [u32; 4]) -> Self {
            let profile = SourceFactsScanProfile::new(2).expect("dense test profile");
            let parser_profile = ParserProfileId::new(1).expect("parser profile");
            let binding = SessionBinding {
                document_session,
                source_session_identity: document_session[3] + 1,
                worker_generation: 1,
            };
            let base_source: String = (0..1_024)
                .map(|ordinal| format!("ordinary prose line {ordinal:04} {}\n", "a".repeat(40)))
                .collect();
            let mut runtime =
                DocumentRuntime::new(&base_source, standard_document_runtime_config())
                    .expect("ordinary cancellation runtime");
            let (certified, base_completion) =
                complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
            let base_version = certified.source();
            let mut endpoint = CandidateEndpoint::new();
            endpoint
                .start(certified, binding, base_completion)
                .expect("start clean ordinary base candidate");
            let mut host = NativeCandidateHost::new(HostConfig {
                document_session: binding.document_session,
                grammar_revision: GRAMMAR_REVISION,
                syntax_profile: 1,
                authority_mask: AUTHORITY_MASK_ALL_ROLES,
                maximum_query_bytes: 64 * 1024,
            })
            .expect("independent candidate host");
            host.observe_source_version(source_version_for(binding, base_completion))
                .expect("host observes ordinary base");
            let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
                &mut endpoint,
                &mut runtime,
                &mut host,
            );
            assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
            Self {
                profile,
                parser_profile,
                binding,
                base_source,
                base_version,
                base_ack: base_delivery.ack,
                runtime,
                endpoint,
                host,
            }
        }

        fn edit_offset(&self, line_ordinal: usize) -> usize {
            let prefix = format!("ordinary prose line {line_ordinal:04} ");
            self.base_source
                .find(&prefix)
                .expect("fixture line")
                .checked_add(prefix.len() + 20)
                .expect("bounded fixture offset")
        }

        fn start_target(
            &mut self,
            edit_start: usize,
            replacement: &str,
            certification_id: u32,
            ui_revision: u32,
        ) -> flark_engine::SourceVersion {
            let current = self
                .runtime
                .current_source_version()
                .expect("current fixture source");
            let target = self
                .runtime
                .apply_edit(current, edit_start..edit_start + 1, replacement)
                .expect("apply ordinary target edit")
                .source()
                .current();
            let plan = self
                .runtime
                .begin_incremental_source_facts(
                    self.profile,
                    self.parser_profile,
                    SourceFactsRootLimits::default(),
                )
                .expect("plan bounded SourceFacts replacement");
            assert_eq!(
                plan.base(),
                self.base_version,
                "an uncommitted cancelled target must roll back to the installed base"
            );
            assert!(
                self.endpoint
                    .has_incremental_base_for_plan(&self.runtime, &plan)
                    .expect("preflight ordinary crop"),
                "replacement must retain exact parser authority for the original base"
            );
            let witness = complete_incremental_source_facts(&mut self.runtime);
            let target_lease = self
                .runtime
                .snapshot_current_source()
                .expect("borrow exact target source");
            let completion =
                completion_for_persistent_target(&self.runtime, certification_id, ui_revision);
            self.host
                .observe_source_version(source_version_for(self.binding, completion))
                .expect("host observes target source");
            self.endpoint
                .start_incremental(
                    &self.runtime,
                    target_lease,
                    witness,
                    self.binding,
                    completion,
                )
                .expect("start authenticated ordinary crop candidate");
            target
        }

        fn assert_original_base_restored(&self) {
            let retained = self
                .endpoint
                .retained
                .as_ref()
                .expect("cancelled target restores retained base");
            assert_eq!(retained.ack, self.base_ack);
            assert_eq!(
                retained
                    .restart
                    .as_ref()
                    .expect("restored parser restart")
                    .source(),
                self.base_version
            );
            assert!(
                self.endpoint
                    .has_exact_base_for(&self.runtime, self.base_version)
                    .expect("inspect restored exact base")
            );
        }

        fn deliver_replacement(&mut self, target: flark_engine::SourceVersion) -> ExactDelivery {
            let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
                &mut self.endpoint,
                &mut self.runtime,
                &mut self.host,
            );
            assert_eq!(delivery.offer.mode, PublicationMode::ExactBaseDelta);
            assert_eq!(delivery.offer.base_ack, Some(self.base_ack));
            assert!(
                !self
                    .runtime
                    .commit_persistent_source_facts_delta(target)
                    .expect("inspect delivered SourceFacts transaction"),
                "the delivery helper must mirror production and commit before returning"
            );
            drain_candidate_cleanup(&mut self.endpoint, &mut self.runtime);
            assert!(
                self.endpoint
                    .has_exact_base_for(&self.runtime, target)
                    .expect("replacement becomes exact base")
            );
            delivery
        }
    }

    fn deliver_endpoint_to_independent_host_with_unit_fuel(
        endpoint: &mut CandidateEndpoint,
        runtime: &mut DocumentRuntime,
        host: &mut NativeCandidateHost,
    ) -> ExactDelivery {
        deliver_endpoint_to_independent_host_with_fuel(endpoint, runtime, host, 1)
    }

    fn deliver_endpoint_to_independent_host_with_fuel(
        endpoint: &mut CandidateEndpoint,
        runtime: &mut DocumentRuntime,
        host: &mut NativeCandidateHost,
        fuel: usize,
    ) -> ExactDelivery {
        assert!(fuel > 0);
        let host_transitions = u32::try_from(fuel).expect("bounded host fuel");
        let mut next_event_id = 1_u32;
        let mut pending_event = None;
        let mut offer = None;
        let mut committed = None;
        let mut packet_frames = Vec::new();
        let mut contains_recursive_green_leaf = false;
        let mut contains_recursive_green_branch = false;
        for _ in 0..1_000_000 {
            let event = match pending_event.take() {
                Some(event) => event,
                None => match endpoint.poll(runtime, fuel).unwrap_or_else(|error| {
                    panic!(
                        "fuelled producer poll in phase {} (cleanup={}): {error:?}",
                        endpoint.active_phase_for_test(),
                        endpoint.cleanup.is_some(),
                    )
                }) {
                    CandidatePoll::Pending { transitions } => {
                        assert!(
                            (1..=fuel).contains(&transitions),
                            "a ready fuelled candidate poll must make bounded progress or emit"
                        );
                        continue;
                    }
                    CandidatePoll::Event { transitions, event } => {
                        assert!(transitions <= fuel);
                        *event
                    }
                    CandidatePoll::HotInlineEvent { .. } => {
                        panic!("structural delivery emitted a hot-inline event")
                    }
                    CandidatePoll::ViewportPresentationEvent { .. } => {
                        panic!("structural delivery emitted a viewport event")
                    }
                    CandidatePoll::ViewportPresentationUnavailable { .. } => {
                        panic!("structural delivery emitted viewport unavailability")
                    }
                },
            };
            let event_id = next_event_id;
            next_event_id = next_event_id.checked_add(1).expect("test event id");
            let CandidateEvent { credit, body } = event;
            match body {
                CandidateEventBody::Begin(begin) => {
                    host.begin_offer(begin)
                        .expect("independent host begins offer");
                    endpoint
                        .accept_credit(credit, event_id)
                        .expect("producer accepts Begin credit");
                    offer = Some(begin);
                }
                CandidateEventBody::Packet { encoded } => {
                    let packet =
                        decode_publication_packet(&encoded).expect("decode producer packet");
                    let offer_id = packet.offer_id;
                    let frames = packet
                        .frames()
                        .map(|frame| {
                            let frame = frame.expect("validated producer frame");
                            contains_recursive_green_leaf |=
                                frame.bytes.windows(4).any(|window| window == b"RGL1");
                            contains_recursive_green_branch |=
                                frame.bytes.windows(4).any(|window| window == b"RGB1");
                            let kind = match M11CandidateHost::classify_frame(frame.bytes)
                                .expect("independent frame classification")
                                .kind
                            {
                                M11HostFrameKind::Begin => CandidateSnapshotFrameKind::Begin,
                                M11HostFrameKind::SourceFactsReplacementPage => {
                                    CandidateSnapshotFrameKind::SourceFactsReplacementPage
                                }
                                M11HostFrameKind::BlockSequenceReplacementPage => {
                                    CandidateSnapshotFrameKind::BlockSequenceReplacementPage
                                }
                                M11HostFrameKind::RecursiveGreenReplacementPage => {
                                    CandidateSnapshotFrameKind::RecursiveGreenReplacementPage
                                }
                                M11HostFrameKind::Node => CandidateSnapshotFrameKind::Node,
                                M11HostFrameKind::End => CandidateSnapshotFrameKind::End,
                            };
                            (kind, frame.record_count)
                        })
                        .collect::<Vec<_>>();
                    host.admit_packet(packet)
                        .expect("independent host admits packet");
                    endpoint
                        .accept_credit(credit, event_id)
                        .expect("producer accepts packet event credit");
                    let (credited_offer_id, next_frame_ordinal) = loop {
                        match host
                            .poll(HostWorkGrant {
                                inspect_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
                                copy_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
                                transitions: host_transitions,
                            })
                            .expect("fuelled host packet poll")
                        {
                            NativeHostPollOutcome::Pending => {}
                            NativeHostPollOutcome::PacketCredit {
                                offer_id,
                                next_frame_ordinal,
                            } => break (offer_id, next_frame_ordinal),
                            outcome => panic!("unexpected packet outcome: {outcome:?}"),
                        }
                    };
                    packet_frames.push(frames);
                    assert!(
                        endpoint
                            .handle_host_poll(
                                event_id,
                                offer_id,
                                HostPollPhase::PacketCredit,
                                HostPollResult::Completed(HostPollOutcome::PacketCredit {
                                    offer_id: credited_offer_id,
                                    next_frame_ordinal,
                                }),
                            )
                            .expect("producer accepts exact host packet credit")
                            .is_none()
                    );
                }
                CandidateEventBody::Commit(commit) => {
                    host.request_commit(commit)
                        .expect("independent host accepts commit");
                    endpoint
                        .accept_credit(credit, event_id)
                        .expect("producer accepts commit event credit");
                    let outcome = loop {
                        match host
                            .poll(HostWorkGrant {
                                inspect_bytes: 0,
                                copy_bytes: 0,
                                transitions: host_transitions,
                            })
                            .expect("fuelled host install poll")
                        {
                            NativeHostPollOutcome::Pending => {}
                            outcome @ NativeHostPollOutcome::Committed(_) => break outcome,
                            outcome => panic!("unexpected commit outcome: {outcome:?}"),
                        }
                    };
                    let NativeHostPollOutcome::Committed(ack) = outcome else {
                        unreachable!("matched above")
                    };
                    committed = Some(ack);
                    pending_event = endpoint
                        .handle_host_poll(
                            event_id,
                            commit.offer_id,
                            HostPollPhase::Commit,
                            HostPollResult::Completed(HostPollOutcome::Committed(ack)),
                        )
                        .expect("producer accepts exact host commit");
                }
                CandidateEventBody::DeliveryAcknowledged(ack) => {
                    assert_eq!(committed, Some(ack));
                    let target = runtime
                        .current_source_version()
                        .expect("delivered target source");
                    runtime
                        .commit_persistent_source_facts_delta(target)
                        .expect("commit delivered SourceFacts target");
                    host.acknowledge_delivery(ack)
                        .expect("independent host accepts delivery");
                    endpoint
                        .accept_credit(credit, event_id)
                        .expect("producer accepts delivery credit");
                    return ExactDelivery {
                        offer: offer.expect("producer emitted Begin"),
                        ack,
                        packet_frames,
                        contains_recursive_green_leaf,
                        contains_recursive_green_branch,
                    };
                }
            }
        }
        panic!("fuelled candidate delivery did not complete");
    }

    fn close_exact_pair_to_zero(
        endpoint: &mut CandidateEndpoint,
        runtime: &mut DocumentRuntime,
        host: &mut NativeCandidateHost,
    ) {
        close_exact_pair_to_zero_with_fuel(endpoint, runtime, host, 1);
    }

    fn close_exact_pair_to_zero_with_fuel(
        endpoint: &mut CandidateEndpoint,
        runtime: &mut DocumentRuntime,
        host: &mut NativeCandidateHost,
        fuel: usize,
    ) {
        assert!(fuel > 0);
        let host_transitions = u32::try_from(fuel).expect("bounded host close fuel");
        endpoint.begin_close().expect("begin producer close");
        for _ in 0..1_000_000 {
            if !endpoint.cleanup_pending() {
                break;
            }
            let poll = endpoint
                .poll(runtime, fuel)
                .expect("fuelled producer close");
            assert!(matches!(poll, CandidatePoll::Pending { transitions } if transitions <= fuel));
        }
        assert!(!endpoint.cleanup_pending());
        runtime.begin_close().expect("begin runtime close");
        for _ in 0..1_000_000 {
            if runtime
                .poll_close(fuel)
                .expect("fuelled runtime close")
                .complete
            {
                break;
            }
        }
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);

        host.begin_close().expect("begin independent host close");
        for _ in 0..1_000_000 {
            match host
                .poll(HostWorkGrant {
                    inspect_bytes: 0,
                    copy_bytes: 0,
                    transitions: host_transitions,
                })
                .expect("fuelled independent host close")
            {
                NativeHostPollOutcome::Pending => {}
                NativeHostPollOutcome::Closed => break,
                outcome => panic!("unexpected close outcome: {outcome:?}"),
            }
        }
        assert!(host.is_removable());
    }

    #[test]
    fn clean_cm321_schema9_point_and_schema10_viewport_are_typed_and_exact() {
        const CM321: &str = "- a\n  > **b** and _c_\n  ```\n  code\n  ```\n- **d**\n";
        let profile = SourceFactsScanProfile::new(4).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [321, 322, 323, 324],
            source_session_identity: 325,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new(CM321, standard_document_runtime_config()).expect("CM321 runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start CM321 candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("CM321 host");
        host.observe_source_version(source_version_for(binding, completion))
            .expect("host observes CM321 source");

        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );

        assert_eq!(delivery.offer.mode, PublicationMode::FullSnapshot);
        assert!(
            delivery.contains_recursive_green_leaf,
            "clean CM321 candidate must transport an RGL1 recursive-Green leaf"
        );
        assert_eq!(
            endpoint
                .retained
                .as_ref()
                .expect("installed parser publication")
                .ack,
            delivery.ack
        );
        assert_eq!(
            endpoint
                .recursive_green
                .installed
                .as_ref()
                .expect("installed recursive-Green session")
                .ack,
            delivery.ack
        );

        let source_version = source_version_for(binding, completion);
        let selected_byte = CM321.find('b').expect("nested strong content");
        let neighbor_byte = CM321.rfind('d').expect("neighbor Paragraph content");
        let selected_frame =
            recursive_green_owner_frame(&host, source_version, selected_byte, selected_byte);
        let neighbor_frame =
            recursive_green_owner_frame(&host, source_version, neighbor_byte, neighbor_byte);
        assert_ne!(selected_frame, neighbor_frame);

        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 1,
                    source_version,
                    base_ack: delivery.ack,
                    byte_offset: u32::try_from(selected_byte).expect("selected byte"),
                    utf16_offset: u32::try_from(selected_byte).expect("ASCII selected UTF-16"),
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::RecursiveGreenParagraph,
                },
            )
            .expect("request fresh schema-9 Green Paragraph sidecar");
        let pending = commit_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            32_100,
        );
        assert_eq!(pending.hio1_schema, 4, "Green owner uses typed HIO1 schema");
        assert_eq!(
            pending.begin.binding.owner(),
            Some(HotInlineSidecarOwner::RecursiveGreenFrame(selected_frame))
        );
        assert_ne!(pending.begin.binding.block_ordinal & (1_u64 << 63), 0);
        assert_eq!(
            pending.ack.block_ordinal,
            pending.begin.binding.block_ordinal
        );
        assert!(matches!(
            pending.begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative { fact_count, .. }
                if fact_count >= 2
        ));

        host.acknowledge_inline_sidecar_delivery(pending.ack)
            .expect("host acknowledges exact Green sidecar");
        endpoint
            .accept_hot_inline_credit(pending.credit, pending.event_id)
            .expect("producer accepts Green sidecar delivery");

        let mut facts = [0_u8; 8 * M11_INLINE_FACT_RECORD_BYTES];
        let fact_count = match host
            .query_inline_sidecar(pending.begin.binding, &mut facts)
            .expect("query installed Green Paragraph sidecar")
        {
            HostInlineSidecarQueryOutcome::Authoritative { fact_count, .. } => fact_count,
            outcome => panic!("Green Paragraph sidecar must be authoritative: {outcome:?}"),
        };
        let kinds = facts
            .chunks_exact(M11_INLINE_FACT_RECORD_BYTES)
            .take(fact_count as usize)
            .map(|record| record[0])
            .collect::<Vec<_>>();
        assert!(kinds.contains(&(M11InlineProjectionKind::Strong as u8)));
        assert!(kinds.contains(&(M11InlineProjectionKind::Emphasis as u8)));

        let mut neighbor_binding = pending.begin.binding;
        neighbor_binding.block_ordinal = HotInlineSidecarOwner::RecursiveGreenFrame(neighbor_frame)
            .into_wire()
            .expect("neighbor frame fits owner slot");
        assert!(matches!(
            host.query_inline_sidecar(neighbor_binding, &mut facts)
                .expect("query neighboring Green owner"),
            HostInlineSidecarQueryOutcome::Unavailable
        ));

        let mut stale = pending.begin;
        stale.offer_id[0] ^= 0x8000_0000;
        stale.publication_session[0] ^= 0x4000_0000;
        stale.binding.refinement_generation = 2;
        stale.base_ack.host_revision = stale
            .base_ack
            .host_revision
            .checked_add(1)
            .expect("stale test revision");
        assert_eq!(
            host.begin_inline_sidecar_offer(stale)
                .expect_err("stale structural ACK cannot attach")
                .reason(),
            crate::v3_host_store::HostRejectReason::BaseMismatch
        );

        for _ in 0..10_000 {
            if !endpoint.hot_inline_has_poll_work() {
                break;
            }
            assert!(
                endpoint
                    .poll_hot_inline(&mut runtime, 1)
                    .expect("release delivered CM321 point sidecar")
                    <= 1
            );
        }
        assert!(!endpoint.hot_inline_has_poll_work());

        let row_window = endpoint
            .recursive_green
            .installed_session(delivery.ack)
            .expect("CM321 Green session remains exact-current")
            .query_renderable_rows(
                &runtime,
                M11RecursiveGreenPoint::new(
                    selected_byte,
                    selected_byte,
                    SourceBoundaryAffinity::After,
                ),
                u64::try_from(CM321.len()).expect("bounded CM321 end"),
                M11RecursiveGreenRowQueryLimits::new(8, 128, 65_536, 64, 65_536)
                    .expect("nonzero CM321 row limits"),
            )
            .expect("query CM321 Green rows");
        assert!(row_window.rows().len() >= 2);
        let paragraph_row = &row_window.rows()[0];
        let fence_row = &row_window.rows()[1];
        assert_eq!(paragraph_row.kind().get(), 5);
        assert_eq!(fence_row.kind().get(), 7);
        let paragraph_physical = paragraph_row.physical_range();
        let paragraph_physical_utf16 = paragraph_row.physical_utf16_range();
        let paragraph_editable = paragraph_row
            .editable_range()
            .expect("CM321 Paragraph has contiguous byte edit authority");
        let paragraph_editable_utf16 = paragraph_row
            .editable_utf16_range()
            .expect("CM321 Paragraph has contiguous UTF-16 edit authority");
        assert_eq!(paragraph_physical, 8..22);
        assert_eq!(paragraph_physical_utf16, 8..22);
        assert_eq!(paragraph_editable, 8..21);
        assert_eq!(paragraph_editable_utf16, 8..21);

        let HostStructuralOrdinalWindowOutcome::Window {
            total_entry_count,
            start_entry_ordinal,
            next_entry_ordinal,
            start,
            next,
            complete,
            receipt: ordinal_receipt,
            ..
        } = host
            .query_structural_ordinal_window(HostStructuralOrdinalWindowQuery {
                source_version,
                start_entry_ordinal: paragraph_row.ordinal(),
                budget: HostStructuralOrdinalWindowBudget {
                    maximum_entries: 3,
                    maximum_storage_pages_visited: 8,
                    maximum_tree_nodes_visited: 128,
                    maximum_packed_entries_inspected: 1024,
                },
            })
            .expect("query CM321 Green ordinal window")
        else {
            panic!("CM321 Green row ordinals must map to exact source cuts");
        };
        assert_eq!(total_entry_count, 4);
        assert_eq!(start_entry_ordinal, paragraph_row.ordinal());
        assert_eq!(next_entry_ordinal, 4);
        assert_eq!(start, HostSourceMetric { bytes: 8, utf16: 8 });
        assert_eq!(
            next,
            HostSourceMetric {
                bytes: CM321.len() as u32,
                utf16: CM321.len() as u32,
            }
        );
        assert!(complete);
        assert!(ordinal_receipt.storage_pages_visited <= 8);
        assert!(ordinal_receipt.tree_nodes_visited <= 128);
        assert!(ordinal_receipt.packed_entries_inspected <= 1024);

        let requested_range = HostMetricRange { start, end: next };
        let mut row_bytes = vec![0xa5_u8; 16 * 1024];
        let HostBlockRangeOutcome::Page {
            requested_range: observed_request,
            covered_range,
            continuation,
            receipt,
            ..
        } = host
            .query_structural_range(
                HostBlockRangeQuery {
                    source_version,
                    requested_range,
                    budget: HostBlockRangeBudget {
                        maximum_encoded_bytes: row_bytes.len() as u32,
                        maximum_block_count: 8,
                        maximum_storage_pages_visited: 128,
                        maximum_open_depth: 64,
                        maximum_tree_nodes_visited: 65_536,
                    },
                    continuation: None,
                },
                &mut row_bytes,
            )
            .expect("query schema-10 CM321 row directory")
        else {
            panic!("CM321 row directory must be exact-current");
        };
        assert_eq!(observed_request, requested_range);
        assert_eq!(
            covered_range,
            HostMetricRange {
                start: HostSourceMetric { bytes: 8, utf16: 8 },
                end: HostSourceMetric {
                    bytes: CM321.len() as u32,
                    utf16: CM321.len() as u32,
                },
            }
        );
        assert!(continuation.is_none());
        assert!(receipt.complete);
        assert_eq!(receipt.block_count, 3);
        assert!(receipt.storage_pages_visited <= 128);
        assert!(receipt.open_depth <= 64);
        assert!(receipt.tree_nodes_visited <= 65_536);
        assert!(receipt.packed_entries_inspected <= 128 * 128);

        let single_row_request = HostMetricRange {
            start: HostSourceMetric { bytes: 8, utf16: 8 },
            end: HostSourceMetric {
                bytes: 22,
                utf16: 22,
            },
        };
        let mut single_row_bytes = vec![0xa5_u8; 16 * 1024];
        let HostBlockRangeOutcome::Page {
            continuation: single_row_continuation,
            receipt: single_row_receipt,
            ..
        } = host
            .query_structural_range(
                HostBlockRangeQuery {
                    source_version,
                    requested_range: single_row_request,
                    budget: HostBlockRangeBudget {
                        maximum_encoded_bytes: single_row_bytes.len() as u32,
                        maximum_block_count: 1,
                        maximum_storage_pages_visited: 128,
                        maximum_open_depth: 64,
                        maximum_tree_nodes_visited: 65_536,
                    },
                    continuation: None,
                },
                &mut single_row_bytes,
            )
            .expect("query exact nonterminal CM321 Green row")
        else {
            panic!("one exact nonterminal Green row must be a complete range page");
        };
        assert!(single_row_continuation.is_none());
        assert!(single_row_receipt.complete);
        assert_eq!(
            u32::from_le_bytes(
                single_row_bytes[32..36]
                    .try_into()
                    .expect("single-row completion flag"),
            ),
            1,
        );

        let read_u16 = |offset: usize| {
            u16::from_le_bytes(row_bytes[offset..offset + 2].try_into().expect("wire u16"))
        };
        let read_u32 = |offset: usize| {
            u32::from_le_bytes(row_bytes[offset..offset + 4].try_into().expect("wire u32"))
        };
        let read_u64 = |offset: usize| {
            u64::from_le_bytes(row_bytes[offset..offset + 8].try_into().expect("wire u64"))
        };
        assert_eq!(&row_bytes[..8], b"FLKVR001");
        assert_eq!(read_u32(8), HOST_RECURSIVE_GREEN_ROW_RANGE_SCHEMA);
        assert_eq!(
            read_u32(12),
            HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES as u32
        );
        assert_eq!(read_u32(16), HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES as u32);
        assert_eq!(
            read_u32(20),
            HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES as u32
        );
        assert_eq!(read_u32(24), 3);
        assert_eq!(read_u32(28), 13);
        assert_eq!(read_u32(32), 1);
        assert_eq!(read_u32(36), 0);
        assert_eq!(read_u64(40), paragraph_row.ordinal());
        assert_eq!(read_u64(48), 4);
        assert_eq!(read_u32(56), 1);
        assert_eq!(read_u32(60), 1);
        assert_eq!(read_u32(64), delivery.ack.source_version.revision);
        assert_eq!(read_u32(68), delivery.ack.parse_generation);
        for (index, word) in delivery.ack.publication_session.into_iter().enumerate() {
            assert_eq!(read_u32(72 + index * 4), word);
        }
        assert_eq!(
            receipt.encoded_bytes as usize,
            HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES
                + 3 * HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES
                + 13 * HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES
        );

        let row = HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES;
        assert_eq!(read_u64(row), paragraph_row.ordinal());
        assert_eq!(read_u64(row + 8), selected_frame);
        assert_eq!(read_u16(row + 16), 5);
        assert_eq!(read_u16(row + 18), 0b11);
        assert_eq!(read_u32(row + 20), 0);
        assert_eq!(read_u32(row + 24), 5);
        assert_eq!(read_u16(row + 28), 1);
        assert_eq!(read_u16(row + 30), 1);
        assert_eq!(
            (
                read_u32(row + 32),
                read_u32(row + 36),
                read_u32(row + 40),
                read_u32(row + 44)
            ),
            (8, 8, 22, 22)
        );
        assert_eq!(
            (
                read_u32(row + 48),
                read_u32(row + 52),
                read_u32(row + 56),
                read_u32(row + 60)
            ),
            (8, 8, 21, 21)
        );

        let paths =
            HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES + 3 * HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES;
        let path_offset = |index: usize| paths + index * HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES;
        let kinds = (0..5)
            .map(|index| read_u16(path_offset(index) + 8))
            .collect::<Vec<_>>();
        assert_eq!(kinds, vec![1, 3, 4, 2, 5]);
        let list = path_offset(1);
        assert_eq!(read_u16(list + 10), 0b1110);
        assert_eq!(read_u16(list + 12), 1);
        assert_eq!(
            (
                read_u32(list + 32),
                read_u32(list + 36),
                read_u32(list + 40),
                read_u32(list + 44),
            ),
            (1, u32::from(b'-'), 1, 1)
        );
        let item = path_offset(2);
        assert_eq!(read_u16(item + 10), 0b0110);
        assert_eq!(read_u16(item + 12), 2);
        assert_eq!((read_u32(item + 32), read_u32(item + 36)), (0, 2));
        let quote = path_offset(3);
        assert_eq!(read_u16(quote + 8), 2);
        assert_eq!(read_u16(quote + 10), 0b0010);
        assert_eq!(read_u16(quote + 12), 0);
        let owner = path_offset(4);
        assert_eq!(read_u16(owner + 8), 5);
        assert_eq!(read_u16(owner + 10), 0b0001);
        assert_eq!(
            (
                read_u32(owner + 16),
                read_u32(owner + 20),
                read_u32(owner + 24),
                read_u32(owner + 28)
            ),
            (8, 8, 22, 22)
        );
        let full_row_window = endpoint
            .recursive_green
            .installed_session(delivery.ack)
            .expect("CM321 Green session remains exact-current")
            .query_renderable_rows(
                &runtime,
                M11RecursiveGreenPoint::new(0, 0, SourceBoundaryAffinity::After),
                u64::try_from(CM321.len()).expect("bounded CM321 end"),
                M11RecursiveGreenRowQueryLimits::new(8, 128, 65_536, 64, 65_536)
                    .expect("nonzero CM321 full-row limits"),
            )
            .expect("query full CM321 Green rows");
        assert_eq!(full_row_window.start_ordinal(), 0);
        assert_eq!(full_row_window.rows().len(), 4);
        assert_eq!(
            full_row_window
                .rows()
                .iter()
                .map(|row| row.kind().get())
                .collect::<Vec<_>>(),
            vec![5, 5, 7, 5]
        );
        let first_row = &full_row_window.rows()[0];
        let last_row = &full_row_window.rows()[3];
        let first_physical = first_row.physical_range();
        let first_physical_utf16 = first_row.physical_utf16_range();
        let last_physical = last_row.physical_range();
        let last_physical_utf16 = last_row.physical_utf16_range();
        let viewport_command = ViewportInlineBatchCommand {
            binding,
            viewport_generation: 1,
            source_version,
            base_ack: delivery.ack,
            start_entry_ordinal: first_row.ordinal(),
            start_byte_offset: u32::try_from(first_physical.start)
                .expect("bounded first-row start"),
            start_utf16_offset: u32::try_from(first_physical_utf16.start)
                .expect("bounded first-row UTF-16 start"),
            end_byte_offset: u32::try_from(last_physical.end).expect("bounded last-row end"),
            end_utf16_offset: u32::try_from(last_physical_utf16.end)
                .expect("bounded last-row UTF-16 end"),
            limits: ViewportInlineBatchLimits {
                maximum_structural_entries: 4,
                maximum_storage_pages: 25,
                maximum_inline_leaves: 3,
                maximum_inline_leaf_source_bytes: 8 * 1024,
                maximum_inline_source_bytes: 64 * 1024,
                maximum_fact_records: 2_048,
                maximum_projection_bytes: 2 * 1024 * 1024,
                maximum_parser_transitions: 250_000,
            },
        };
        endpoint
            .request_viewport_inline_batch(&runtime, viewport_command)
            .expect("request schema-10 CM321 Green viewport");
        for _ in 0..100_000 {
            if matches!(
                endpoint.viewport_inline_batch,
                Some(ViewportInlineBatchState::Ready(_))
            ) {
                break;
            }
            assert!(
                endpoint
                    .poll_viewport_inline_batch(&mut runtime, 1)
                    .expect("poll CM321 Green viewport")
                    <= 1
            );
        }
        let Some(ViewportInlineBatchState::Ready(ready)) = endpoint.viewport_inline_batch.as_ref()
        else {
            panic!("CM321 Green viewport did not become ready");
        };
        assert_eq!(ready.range_receipt.visited_rows, 4);
        assert_eq!(
            ready
                .leaves
                .iter()
                .map(|leaf| leaf.geometry.entry_ordinal)
                .collect::<Vec<_>>(),
            vec![0, 1, 3],
            "only the three Paragraph rows have HIO1 children"
        );
        let geometry = &ready
            .leaves
            .iter()
            .find(|leaf| leaf.geometry.entry_ordinal == paragraph_row.ordinal())
            .expect("nested Paragraph child remains present")
            .geometry;
        assert_eq!(geometry.entry_ordinal, paragraph_row.ordinal());
        assert_eq!(geometry.frame, paragraph_row.frame());
        assert_eq!(
            geometry.block_source,
            u32::try_from(paragraph_physical.start).unwrap()
                ..u32::try_from(paragraph_physical.end).unwrap()
        );
        assert_eq!(
            geometry.block_source_utf16,
            u32::try_from(paragraph_physical_utf16.start).unwrap()
                ..u32::try_from(paragraph_physical_utf16.end).unwrap()
        );
        assert_eq!(
            geometry.inline_source,
            u32::try_from(paragraph_editable.start).unwrap()
                ..u32::try_from(paragraph_editable.end).unwrap()
        );
        assert_eq!(
            geometry.inline_source_utf16,
            u32::try_from(paragraph_editable_utf16.start).unwrap()
                ..u32::try_from(paragraph_editable_utf16.end).unwrap()
        );
        let (viewport_begin, viewport_ack, authoritative, unsupported, child_closures) =
            deliver_viewport_presentation_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
        assert_eq!(viewport_begin.envelope.visited_structural_entries, 4);
        assert_eq!(viewport_begin.envelope.ordered_leaf_count, 3);
        assert_eq!(
            viewport_begin.binding.start.block_ordinal,
            first_row.ordinal()
        );
        assert_eq!(
            viewport_begin.binding.next.block_ordinal,
            last_row.ordinal() + 1
        );
        assert_eq!(authoritative, 3);
        assert_eq!(unsupported, 0);
        assert_eq!(child_closures, 3);
        assert_eq!(viewport_ack.base_ack, delivery.ack);
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    fn recursive_green_query_shape(
        host: &NativeCandidateHost,
        source_version: SourceVersion,
        byte_offset: usize,
        utf16_offset: usize,
    ) -> (u16, [u32; 4], Vec<u16>) {
        let mut output = vec![0_u8; 4 * 1024];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version,
                    position: HostSourceMetric {
                        bytes: u32::try_from(byte_offset).expect("test byte point"),
                        utf16: u32::try_from(utf16_offset).expect("test UTF-16 point"),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: 4 * 1024,
                        maximum_open_depth: 16,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut output,
            )
            .expect("query recursive-Green viewport");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("recursive-Green query returned a gap: {outcome:?}");
        };
        let encoded_bytes = usize::try_from(receipt.encoded_bytes).expect("viewport bytes");
        assert!(encoded_bytes >= 112);
        assert_eq!(
            u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
            9
        );
        let ancestry_count = usize::try_from(u32::from_le_bytes(
            output[36..40].try_into().expect("ancestry count"),
        ))
        .expect("ancestry count fits");
        assert_eq!(encoded_bytes, 112 + ancestry_count * 16);
        let owner_kind = u16::from_le_bytes(output[44..46].try_into().expect("owner kind"));
        let range = [
            u32::from_le_bytes(output[48..52].try_into().expect("byte start")),
            u32::from_le_bytes(output[52..56].try_into().expect("byte end")),
            u32::from_le_bytes(output[56..60].try_into().expect("UTF-16 start")),
            u32::from_le_bytes(output[60..64].try_into().expect("UTF-16 end")),
        ];
        let ancestry = (0..ancestry_count)
            .map(|index| {
                let start = 112 + index * 16;
                u16::from_le_bytes(
                    output[start + 8..start + 10]
                        .try_into()
                        .expect("ancestor kind"),
                )
            })
            .collect();
        (owner_kind, range, ancestry)
    }

    fn recursive_green_owner_frame(
        host: &NativeCandidateHost,
        source_version: SourceVersion,
        byte_offset: usize,
        utf16_offset: usize,
    ) -> u64 {
        let mut output = vec![0_u8; 4 * 1024];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version,
                    position: HostSourceMetric {
                        bytes: u32::try_from(byte_offset).expect("test byte point"),
                        utf16: u32::try_from(utf16_offset).expect("test UTF-16 point"),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: 4 * 1024,
                        maximum_open_depth: 16,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut output,
            )
            .expect("query recursive-Green owner");
        let HostStructuralQueryOutcome::Viewport { .. } = outcome else {
            panic!("recursive-Green owner query returned a gap: {outcome:?}");
        };
        assert_eq!(
            u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
            9
        );
        let ancestry_count = usize::try_from(u32::from_le_bytes(
            output[36..40].try_into().expect("ancestry count"),
        ))
        .expect("ancestry count fits");
        let owner_index = usize::try_from(u32::from_le_bytes(
            output[40..44].try_into().expect("owner index"),
        ))
        .expect("owner index fits");
        assert!(owner_index < ancestry_count);
        let start = 112 + owner_index * 16;
        u64::from_le_bytes(output[start..start + 8].try_into().expect("owner frame ID"))
    }

    #[test]
    fn nested_local_edit_preempts_legacy_parse_and_installs_exact_recursive_green_delta() {
        const CM321: &str = "- a\n  > b\n  ```\n  c\n  ```\n- d\n";
        const CM325: &str = "* foo\n  * bar\n\n  baz\n";
        const LENGTH_DELTA: usize = "beta".len() - 1;
        let mut source = String::new();
        for ordinal in 0..9_000 {
            source.push_str(&format!(
                "Prefix paragraph {ordinal:05} carries enough ordinary source for sparse restart spacing.\n\n"
            ));
        }
        let cm321_start = source.len();
        source.push_str(CM321);
        source.push('\n');
        source.push_str(CM325);
        source.push('\n');
        for ordinal in 0..1_000 {
            source.push_str(&format!(
                "Trailing paragraph {ordinal:04} remains an unchanged serialized-Green sibling.\n\n"
            ));
        }
        assert!(source.len() > 512 * 1024);

        let profile = SourceFactsScanProfile::new(4).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [331, 332, 333, 334],
            source_session_identity: 335,
            worker_generation: 1,
        };
        let mut runtime = DocumentRuntime::new(&source, standard_document_runtime_config())
            .expect("large CM321 runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_source = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start CM321 base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("CM321 host");
        let base_wire_source = source_version_for(binding, base_completion);
        host.observe_source_version(base_wire_source)
            .expect("host observes CM321 base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

        let distant_byte = source
            .rfind("Trailing paragraph 0999")
            .expect("distant trailing sibling");
        let distant_utf16 = source[..distant_byte].encode_utf16().count();
        let distant_before =
            recursive_green_query_shape(&host, base_wire_source, distant_byte, distant_utf16);

        endpoint
            .cancel_for_edit(&mut runtime)
            .expect("prepare nested edit");
        let edited_byte = cm321_start + CM321.find("> b").expect("nested quote content") + 2;
        runtime
            .apply_edit(base_source, edited_byte..edited_byte + 1, "beta")
            .expect("apply nested local edit");
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("begin incremental source facts");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("exact-base preflight")
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("target source lease");
        let target_source = target_lease.version();
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        let target_wire_source = source_version_for(binding, target_completion);
        host.observe_source_version(target_wire_source)
            .expect("host observes CM321 target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start nested exact candidate");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingExactFallback",
            "the legacy route is parked before recursive-Green adoption completes"
        );

        while endpoint.recursive_green.target_work_pending() {
            let polled = endpoint
                .poll(&mut runtime, 1)
                .expect("advance only recursive-Green adoption");
            assert!(matches!(polled, CandidatePoll::Pending { transitions: 1 }));
            assert_eq!(
                active_candidate_phase(endpoint.active.as_ref()),
                "ParsingExactFallback",
                "the scheduler must not poll the parked whole-document parser"
            );
        }
        assert!(
            endpoint
                .recursive_green
                .ready_update_for(base_delivery.ack, target_source)
                .is_some()
        );

        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
        assert!(
            target_delivery
                .packet_frames
                .iter()
                .flatten()
                .any(|(kind, _)| {
                    *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage
                })
        );
        assert!(
            target_delivery
                .packet_frames
                .iter()
                .flatten()
                .all(|(kind, _)| {
                    *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage
                })
        );
        assert!(target_delivery.contains_recursive_green_leaf);
        assert!(
            !target_delivery.contains_recursive_green_branch,
            "RGB1 branches must be rebuilt by the independent host, not transported"
        );
        assert_eq!(
            endpoint.recursive_green_path_receipt(),
            RecursiveGreenPathReceipt {
                local_adoption_deliveries: 1,
                clean_fallback_deliveries: 0,
            }
        );

        let distant_after = recursive_green_query_shape(
            &host,
            target_wire_source,
            distant_byte + LENGTH_DELTA,
            distant_utf16 + LENGTH_DELTA,
        );
        assert_eq!(distant_after.0, distant_before.0);
        assert_eq!(distant_after.2, distant_before.2);
        assert_eq!(
            distant_after.1,
            distant_before.1.map(|metric| metric + LENGTH_DELTA as u32)
        );
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn small_nested_edit_clean_fallback_publishes_recursive_green_snapshot() {
        const SOURCE: &str = "- a\n  > **b** and _c_\n  ```\n  code\n  ```\n- **d**\n";
        let profile = SourceFactsScanProfile::new(4).expect("dense test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [341, 342, 343, 344],
            source_session_identity: 345,
            worker_generation: 1,
        };
        let mut runtime = DocumentRuntime::new(SOURCE, standard_document_runtime_config())
            .expect("small nested runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_source = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start small nested base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("small nested host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes small nested base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

        endpoint
            .cancel_for_edit(&mut runtime)
            .expect("prepare small nested edit");
        let caret = SOURCE.find("b**").expect("nested strong content") + 1;
        let caret_utf16 = SOURCE[..caret].encode_utf16().count();
        assert!(
            !endpoint
                .prepare_bullet_list_local_edit(&runtime, caret..caret, caret_utf16..caret_utf16)
                .expect("classify nested edit route"),
            "the nested quote edit is not admitted by the list-local lane"
        );
        runtime
            .apply_edit(base_source, caret..caret, "x")
            .expect("apply nested insertion");
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan small nested incremental facts");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("small nested exact-base preflight")
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("small nested target source");
        let target_source = target_lease.version();
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        let target_wire_source = source_version_for(binding, target_completion);
        host.observe_source_version(target_wire_source)
            .expect("host observes small nested target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start small nested candidate");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingExactFallback"
        );

        while endpoint.recursive_green.target_work_pending() {
            assert!(matches!(
                endpoint
                    .poll(&mut runtime, 1)
                    .expect("advance small nested Green adoption"),
                CandidatePoll::Pending { transitions: 1 }
            ));
        }
        assert!(
            endpoint
                .recursive_green
                .ready_update_for(base_delivery.ack, target_source)
                .is_none()
        );
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingExactFallback"
        );
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(delivery.offer.base_ack, None);
        assert!(delivery.packet_frames.iter().flatten().all(|(kind, _)| {
            *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage
        }));
        assert!(delivery.contains_recursive_green_leaf);
        let (owner_kind, _, ancestry) =
            recursive_green_query_shape(&host, target_wire_source, caret - 1, caret_utf16 - 1);
        assert_eq!(owner_kind, 5, "the edited owner remains a Green Paragraph");
        assert!(!ancestry.is_empty());
        assert_eq!(
            endpoint.recursive_green_path_receipt(),
            RecursiveGreenPathReceipt {
                local_adoption_deliveries: 0,
                clean_fallback_deliveries: 1,
            }
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn bullet_list_local_edit_delivers_exact_delta_with_unit_fuel() {
        let profile = SourceFactsScanProfile::new(4).expect("dense test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [91, 92, 93, 94],
            source_session_identity: 95,
            worker_generation: 1,
        };
        let source: String = (0..200)
            .map(|ordinal| format!("- item-{ordinal:04} café 😀\n"))
            .collect();
        let mut runtime = DocumentRuntime::new(&source, standard_document_runtime_config())
            .expect("list runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start list base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("list host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes list base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

        let caret =
            source.find("item-0100 café").expect("middle list item") + "item-0100 café".len();
        let caret_utf16 = source[..caret].encode_utf16().count();
        endpoint
            .cancel_for_edit(&mut runtime)
            .expect("prepare edit cancellation");
        assert!(
            endpoint
                .prepare_bullet_list_local_edit(&runtime, caret..caret, caret_utf16..caret_utf16,)
                .expect("prepare local list edit")
        );
        let target_version = runtime
            .apply_edit(base_version, caret..caret, "🧪")
            .expect("apply local insertion")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan incremental list facts");
        let eligible = endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("local list exact-base preflight");
        let retained = endpoint.retained.as_ref().expect("retained list base");
        let descriptor = retained
            .publication
            .descriptor(&runtime)
            .expect("retained list descriptor");
        assert!(
            eligible,
            "local preflight: base={:?} target={:?} retained={:?} binding={:?} descriptor=({},{},{},{},{}) active={} cleanup={}",
            plan.base(),
            plan.source(),
            retained
                .restart
                .as_ref()
                .map(CandidateRestartAuthority::source),
            retained
                .restart
                .as_ref()
                .map(CandidateRestartAuthority::binding),
            descriptor.source_revision,
            descriptor.source_root,
            descriptor.source_bytes,
            descriptor.source_utf16,
            descriptor.syntax_profile,
            endpoint.active.is_some(),
            endpoint.cleanup.is_some(),
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("local list target source");
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        host.observe_source_version(source_version_for(binding, target_completion))
            .expect("host observes list target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start local list candidate");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingBulletListLocal"
        );

        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(delivery.offer.base_ack, Some(base_delivery.ack));
        assert_eq!(delivery.ack.source_version.revision, 1);
        assert_eq!(
            runtime.current_source_version().expect("delivered target"),
            target_version
        );
        assert!(
            delivery.packet_frames.iter().flatten().any(|(kind, _)| {
                *kind == CandidateSnapshotFrameKind::BlockSequenceReplacementPage
            }),
            "local list delivery must use the existing exact block-splice stream"
        );
        assert!(
            endpoint.bullet_list_local_edit.is_none(),
            "delivery must clear rolling local authority"
        );
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    fn started_bullet_list_local_fixture(
        document_session: [u32; 4],
    ) -> (
        DocumentRuntime,
        CandidateEndpoint,
        NativeCandidateHost,
        flark_engine::SourceVersion,
        usize,
        usize,
    ) {
        let profile = SourceFactsScanProfile::new(4).expect("dense test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session,
            source_session_identity: document_session[3] + 1,
            worker_generation: 1,
        };
        let source: String = (0..120)
            .map(|ordinal| format!("- item-{ordinal:04} café 😀\n"))
            .collect();
        let mut runtime = DocumentRuntime::new(&source, standard_document_runtime_config())
            .expect("list cancellation runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start cancellation list base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("list cancellation host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes cancellation list base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

        let caret =
            source.find("item-0060 café").expect("middle list item") + "item-0060 café".len();
        let caret_utf16 = source[..caret].encode_utf16().count();
        endpoint
            .cancel_for_edit(&mut runtime)
            .expect("pre-edit cancellation");
        assert!(
            endpoint
                .prepare_bullet_list_local_edit(&runtime, caret..caret, caret_utf16..caret_utf16,)
                .expect("prepare cancellation list edit")
        );
        runtime
            .apply_edit(base_version, caret..caret, "x")
            .expect("apply cancellation list edit");
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan cancellation list facts");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("cancellation local preflight")
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("cancellation target source");
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        host.observe_source_version(source_version_for(binding, target_completion))
            .expect("host observes cancellation target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start cancellation local candidate");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingBulletListLocal"
        );
        (runtime, endpoint, host, base_version, caret, caret_utf16)
    }

    #[test]
    fn bullet_list_local_edit_cancellation_restores_base_across_pipeline_phases() {
        for (case, phase) in [
            ([101, 102, 103, 104], "ParsingBulletListLocal"),
            ([111, 112, 113, 114], "BuildingExact"),
            ([121, 122, 123, 124], "Streaming"),
        ] {
            let (mut runtime, mut endpoint, mut host, base, caret, caret_utf16) =
                started_bullet_list_local_fixture(case);
            for _ in 0..100_000 {
                if active_candidate_phase(endpoint.active.as_ref()) == phase {
                    break;
                }
                match endpoint
                    .poll(&mut runtime, 1)
                    .expect("unit-fuel phase poll")
                {
                    CandidatePoll::Pending { transitions } => assert_eq!(transitions, 1),
                    CandidatePoll::Event { .. } => {
                        panic!("phase {phase} was skipped before cancellation")
                    }
                    CandidatePoll::HotInlineEvent { .. } => {
                        panic!("local structural candidate emitted hot-inline work")
                    }
                    CandidatePoll::ViewportPresentationEvent { .. } => {
                        panic!("local structural candidate emitted viewport work")
                    }
                    CandidatePoll::ViewportPresentationUnavailable { .. } => {
                        panic!("local structural candidate emitted viewport unavailability")
                    }
                }
            }
            assert_eq!(active_candidate_phase(endpoint.active.as_ref()), phase);
            endpoint
                .cancel_for_edit(&mut runtime)
                .expect("edit cancellation restores exact base");
            assert!(endpoint.active.is_none());
            assert!(endpoint.retained.is_some());
            assert!(endpoint.bullet_list_local_edit.is_some());
            assert!(
                endpoint
                    .has_exact_base_for(&runtime, base)
                    .expect("restored exact base remains eligible during target cleanup")
            );

            if phase == "ParsingBulletListLocal" {
                assert!(
                    endpoint
                        .prepare_bullet_list_local_edit(
                            &runtime,
                            caret..caret,
                            caret_utf16..caret_utf16,
                        )
                        .expect("preserve cumulative local island")
                );
                endpoint.cancel().expect("normal cancel");
                assert!(
                    endpoint.bullet_list_local_edit.is_none(),
                    "normal cancellation must discard rolling local authority"
                );
            } else if phase == "BuildingExact" {
                assert!(
                    !endpoint
                        .prepare_bullet_list_local_edit(&runtime, 3..3, 3..3)
                        .expect("outside-island preparation")
                );
                assert!(
                    endpoint.bullet_list_local_edit.is_none(),
                    "outside-island preparation must drop rolling authority"
                );
            }
            close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
        }
    }

    fn active_candidate_phase(active: Option<&ActiveCandidate>) -> &'static str {
        match active {
            Some(ActiveCandidate::Parsing(_)) => "Parsing",
            Some(ActiveCandidate::Building { .. }) => "Building",
            Some(ActiveCandidate::ParsingExact(_)) => "ParsingExact",
            Some(ActiveCandidate::ParsingOrdinaryExact(_)) => "ParsingOrdinaryExact",
            Some(ActiveCandidate::ParsingBulletListLocal(_)) => "ParsingBulletListLocal",
            Some(ActiveCandidate::ParsingExactFallback(_)) => "ParsingExactFallback",
            Some(ActiveCandidate::BuildingExactFallback { .. }) => "BuildingExactFallback",
            Some(ActiveCandidate::BuildingExact { .. }) => "BuildingExact",
            Some(ActiveCandidate::Streaming(_)) => "Streaming",
            None => "None",
        }
    }

    fn push_test_frame(builder: &mut PacketBuilder, ordinal: u32, byte_len: usize) {
        assert!(
            builder
                .can_accept(byte_len, MAXIMUM_PACKET_ENCODED_BYTES)
                .expect("bounded packet metric")
        );
        builder
            .push(
                ordinal,
                0,
                0,
                [ordinal; 4],
                vec![ordinal as u8; byte_len].into_boxed_slice(),
                false,
            )
            .expect("test packet frame");
    }

    fn deliver_hot_inline_sidecar_with_unit_fuel(
        endpoint: &mut CandidateEndpoint,
        runtime: &mut DocumentRuntime,
        first_event_id: u32,
    ) -> (HotInlineSidecarBegin, InlineSidecarAck) {
        let mut next_event_id = first_event_id;
        let mut pending_event = None;
        let mut begin = None;
        for _ in 0..1_000_000 {
            let event = match pending_event.take() {
                Some(event) => event,
                None => match endpoint.poll(runtime, 1).expect("unit-fuel sidecar poll") {
                    CandidatePoll::Pending { transitions } => {
                        assert!(transitions <= 1);
                        continue;
                    }
                    CandidatePoll::HotInlineEvent { transitions, event } => {
                        assert!(transitions <= 1);
                        *event
                    }
                    CandidatePoll::Event { .. } => {
                        panic!("hot-inline publication must not emit structural events")
                    }
                    CandidatePoll::ViewportPresentationEvent { .. } => {
                        panic!("hot-inline publication must not emit viewport events")
                    }
                    CandidatePoll::ViewportPresentationUnavailable { .. } => {
                        panic!("hot-inline publication emitted viewport unavailability")
                    }
                },
            };
            let event_id = next_event_id;
            next_event_id = next_event_id.checked_add(1).expect("sidecar event id");
            let HotInlineEvent { credit, body } = event;
            match body {
                HotInlineEventBody::Begin(offer) => {
                    assert_eq!(offer.mode, HotInlineSidecarMode::HotInlineSidecar);
                    assert_eq!(
                        offer.base_ack,
                        endpoint.retained.as_ref().expect("base").ack
                    );
                    endpoint
                        .accept_hot_inline_credit(credit, event_id)
                        .expect("accept sidecar Begin credit");
                    begin = Some(offer);
                }
                HotInlineEventBody::Packet { encoded } => {
                    let packet =
                        decode_publication_packet(&encoded).expect("decode sidecar packet");
                    let offer_id = packet.offer_id;
                    let next_frame_ordinal = packet
                        .first_frame_ordinal
                        .checked_add(packet.frame_count)
                        .expect("bounded sidecar frame cursor");
                    let packet_record_count = packet
                        .frames()
                        .map(|frame| frame.expect("validated sidecar frame").record_count)
                        .sum::<u32>();
                    assert!(
                        packet_record_count
                            <= begin
                                .expect("Begin before packet")
                                .envelope
                                .transferred_node_count
                    );
                    endpoint
                        .accept_hot_inline_credit(credit, event_id)
                        .expect("accept sidecar packet credit");
                    pending_event = endpoint
                        .handle_hot_inline_host_poll(
                            event_id,
                            offer_id,
                            InlineSidecarHostPollPhase::PacketCredit,
                            InlineSidecarHostPollResult::Completed(
                                InlineSidecarHostPollOutcome::PacketCredit {
                                    offer_id,
                                    next_frame_ordinal,
                                },
                            ),
                        )
                        .expect("accept exact sidecar packet cursor");
                }
                HotInlineEventBody::Commit(commit) => {
                    endpoint
                        .accept_hot_inline_credit(credit, event_id)
                        .expect("accept sidecar Commit credit");
                    let ack = endpoint
                        .hot_inline_sidecar
                        .as_ref()
                        .and_then(|sidecar| sidecar.expected_ack)
                        .expect("producer committed exact sidecar ACK");
                    pending_event = endpoint
                        .handle_hot_inline_host_poll(
                            event_id,
                            commit.offer_id,
                            InlineSidecarHostPollPhase::Commit,
                            InlineSidecarHostPollResult::Completed(
                                InlineSidecarHostPollOutcome::Committed(ack),
                            ),
                        )
                        .expect("accept exact sidecar commit ACK");
                }
                HotInlineEventBody::DeliveryAcknowledged(ack) => {
                    endpoint
                        .accept_hot_inline_credit(credit, event_id)
                        .expect("accept sidecar delivery credit");
                    assert!(endpoint.hot_inline_sidecar.is_none());
                    return (begin.expect("sidecar Begin"), ack);
                }
            }
        }
        panic!("unit-fuel sidecar delivery did not complete");
    }

    struct PendingHotInlineDelivery {
        begin: HotInlineSidecarBegin,
        ack: InlineSidecarAck,
        hio1_schema: u32,
        credit: HotInlineCredit,
        event_id: u32,
    }

    fn commit_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        endpoint: &mut CandidateEndpoint,
        runtime: &mut DocumentRuntime,
        host: &mut NativeCandidateHost,
        first_event_id: u32,
    ) -> PendingHotInlineDelivery {
        let mut next_event_id = first_event_id;
        let mut pending_event = None;
        let mut begin = None;
        let mut hio1_schema = None;
        for _ in 0..1_000_000 {
            let event = match pending_event.take() {
                Some(event) => event,
                None => match endpoint.poll(runtime, 1).expect("unit-fuel sidecar poll") {
                    CandidatePoll::Pending { transitions } => {
                        assert!(transitions <= 1);
                        continue;
                    }
                    CandidatePoll::HotInlineEvent { transitions, event } => {
                        assert!(transitions <= 1);
                        *event
                    }
                    CandidatePoll::Event { .. } => {
                        panic!("hot-inline publication must not emit structural events")
                    }
                    CandidatePoll::ViewportPresentationEvent { .. } => {
                        panic!("hot-inline publication must not emit viewport events")
                    }
                    CandidatePoll::ViewportPresentationUnavailable { .. } => {
                        panic!("hot-inline publication emitted viewport unavailability")
                    }
                },
            };
            let event_id = next_event_id;
            next_event_id = next_event_id.checked_add(1).expect("sidecar event id");
            let HotInlineEvent { credit, body } = event;
            match body {
                HotInlineEventBody::Begin(offer) => {
                    host.begin_inline_sidecar_offer(offer)
                        .expect("independent host begins sidecar offer");
                    endpoint
                        .accept_hot_inline_credit(credit, event_id)
                        .expect("accept sidecar Begin credit");
                    begin = Some(offer);
                }
                HotInlineEventBody::Packet { encoded } => {
                    let packet =
                        decode_publication_packet(&encoded).expect("decode sidecar packet");
                    let offer_id = packet.offer_id;
                    for decoded in packet.frames() {
                        let frame = decoded.expect("validated sidecar frame");
                        if frame.ordinal == 0 {
                            assert!(frame.bytes.len() >= 24, "HIO1 Begin carries its envelope");
                            hio1_schema = Some(u32::from_le_bytes(
                                frame.bytes[20..24].try_into().expect("HIO1 schema"),
                            ));
                        }
                    }
                    host.admit_inline_sidecar_packet(packet)
                        .expect("independent host admits sidecar packet");
                    endpoint
                        .accept_hot_inline_credit(credit, event_id)
                        .expect("accept sidecar packet credit");
                    let (credited_offer_id, next_frame_ordinal) = loop {
                        match host
                            .poll_inline_sidecar(HostWorkGrant {
                                inspect_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
                                copy_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
                                transitions: 1,
                            })
                            .expect("unit-fuel host sidecar packet poll")
                        {
                            NativeInlineSidecarHostPollOutcome::Pending => {}
                            NativeInlineSidecarHostPollOutcome::PacketCredit {
                                offer_id,
                                next_frame_ordinal,
                            } => break (offer_id, next_frame_ordinal),
                            outcome => panic!("unexpected sidecar packet outcome: {outcome:?}"),
                        }
                    };
                    pending_event = endpoint
                        .handle_hot_inline_host_poll(
                            event_id,
                            offer_id,
                            InlineSidecarHostPollPhase::PacketCredit,
                            InlineSidecarHostPollResult::Completed(
                                InlineSidecarHostPollOutcome::PacketCredit {
                                    offer_id: credited_offer_id,
                                    next_frame_ordinal,
                                },
                            ),
                        )
                        .expect("producer accepts exact sidecar packet credit");
                }
                HotInlineEventBody::Commit(commit) => {
                    host.request_inline_sidecar_commit(commit)
                        .expect("independent host accepts sidecar commit");
                    endpoint
                        .accept_hot_inline_credit(credit, event_id)
                        .expect("accept sidecar Commit credit");
                    let ack = loop {
                        match host
                            .poll_inline_sidecar(HostWorkGrant {
                                inspect_bytes: 0,
                                copy_bytes: 0,
                                transitions: 1,
                            })
                            .expect("unit-fuel host sidecar install poll")
                        {
                            NativeInlineSidecarHostPollOutcome::Pending => {}
                            NativeInlineSidecarHostPollOutcome::Committed(ack) => break ack,
                            outcome => panic!("unexpected sidecar commit outcome: {outcome:?}"),
                        }
                    };
                    pending_event = endpoint
                        .handle_hot_inline_host_poll(
                            event_id,
                            commit.offer_id,
                            InlineSidecarHostPollPhase::Commit,
                            InlineSidecarHostPollResult::Completed(
                                InlineSidecarHostPollOutcome::Committed(ack),
                            ),
                        )
                        .expect("producer accepts exact sidecar commit ACK");
                }
                HotInlineEventBody::DeliveryAcknowledged(ack) => {
                    return PendingHotInlineDelivery {
                        begin: begin.expect("sidecar Begin"),
                        ack,
                        hio1_schema: hio1_schema.expect("sidecar HIO1 Begin schema"),
                        credit,
                        event_id,
                    };
                }
            }
        }
        panic!("unit-fuel sidecar commit to independent host did not complete");
    }

    fn deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        endpoint: &mut CandidateEndpoint,
        runtime: &mut DocumentRuntime,
        host: &mut NativeCandidateHost,
        first_event_id: u32,
    ) -> (HotInlineSidecarBegin, InlineSidecarAck) {
        let pending = commit_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            endpoint,
            runtime,
            host,
            first_event_id,
        );
        host.acknowledge_inline_sidecar_delivery(pending.ack)
            .expect("independent host accepts sidecar delivery");
        endpoint
            .accept_hot_inline_credit(pending.credit, pending.event_id)
            .expect("accept sidecar delivery credit");
        (pending.begin, pending.ack)
    }

    #[test]
    fn source_root_wire_lanes_are_high_word_then_low_word() {
        assert_eq!(split_u64(0x1122_3344_5566_7788), [0x1122_3344, 0x5566_7788]);
    }

    #[test]
    fn selected_list_item_target_requires_its_exact_projection_kind_and_metadata() {
        use InlineRefinementTarget::{
            Automatic, BulletListItemInline, BulletListItemProjection, OrderedListItemInline,
            OrderedListItemProjection,
        };
        use M11MarkedLineProjectionKind::{BulletList, OrderedList};

        assert!(list_item_projection_matches_target(
            BulletListItemProjection,
            BulletList,
            false,
        ));
        assert!(list_item_projection_matches_target(
            OrderedListItemProjection,
            OrderedList,
            true,
        ));
        for (target, kind, ordered_metadata) in [
            (BulletListItemProjection, OrderedList, false),
            (BulletListItemProjection, BulletList, true),
            (OrderedListItemProjection, BulletList, true),
            (OrderedListItemProjection, OrderedList, false),
            (Automatic, OrderedList, true),
            (BulletListItemInline, BulletList, false),
            (OrderedListItemInline, OrderedList, true),
        ] {
            assert!(
                !list_item_projection_matches_target(target, kind, ordered_metadata),
                "{target:?} must reject {kind:?} with ordered metadata={ordered_metadata}",
            );
        }
    }

    fn deliver_viewport_presentation_with_unit_fuel(
        endpoint: &mut CandidateEndpoint,
        runtime: &mut DocumentRuntime,
        host: &mut NativeCandidateHost,
    ) -> (
        ViewportPresentationBegin,
        ViewportPresentationAck,
        usize,
        usize,
        usize,
    ) {
        let mut next_event_id = 40_000_u32;
        let mut pending_event = None;
        let mut offer = None;
        let mut authoritative = 0_usize;
        let mut unsupported = 0_usize;
        let mut child_closures = 0_usize;
        let mut active_child = None;
        let mut next_child_frame_ordinal = 0_u32;
        let mut observed_frame_count = 0_u32;
        let mut observed_frame_bytes = 0_u32;
        for _ in 0..1_000_000 {
            let event = match pending_event.take() {
                Some(event) => event,
                None => match endpoint
                    .poll(runtime, 1)
                    .expect("unit-fuel viewport producer poll")
                {
                    CandidatePoll::Pending { transitions } => {
                        assert!(transitions <= 1);
                        continue;
                    }
                    CandidatePoll::ViewportPresentationEvent { transitions, event } => {
                        assert!(transitions <= 1);
                        *event
                    }
                    CandidatePoll::Event { .. } => {
                        panic!("viewport delivery emitted a structural event")
                    }
                    CandidatePoll::HotInlineEvent { .. } => {
                        panic!("viewport delivery emitted a point-sidecar event")
                    }
                    CandidatePoll::ViewportPresentationUnavailable { .. } => {
                        panic!("admitted viewport delivery became unavailable")
                    }
                },
            };
            let event_id = next_event_id;
            next_event_id = next_event_id.checked_add(1).expect("viewport event id");
            let CandidateViewportPresentationEvent { credit, body } = event;
            match body {
                CandidateViewportPresentationEventBody::Begin(begin) => {
                    assert_eq!(begin.mode, ViewportPresentationMode::AggregatePage);
                    assert_eq!(
                        begin.limits.maximum_frame_count,
                        begin
                            .envelope
                            .ordered_leaf_count
                            .checked_mul(2)
                            .and_then(|count| {
                                count.checked_add(begin.envelope.transferred_node_count)
                            })
                            .and_then(|count| count.checked_add(3))
                            .expect("bounded viewport frame count")
                    );
                    host.begin_viewport_presentation_offer(begin)
                        .expect("independent host begins viewport offer");
                    endpoint
                        .accept_viewport_presentation_credit(credit, event_id)
                        .expect("accept viewport Begin credit");
                    assert!(
                        endpoint.has_poll_work(),
                        "accepted viewport Begin must wake packet production"
                    );
                    offer = Some(begin);
                }
                CandidateViewportPresentationEventBody::Packet { encoded } => {
                    let begin = offer.expect("viewport Begin precedes packets");
                    let packet =
                        decode_publication_packet(&encoded).expect("decode viewport packet");
                    let packet_offer_id = packet.offer_id;
                    let first_frame_ordinal = packet.first_frame_ordinal;
                    let frame_count = packet.frame_count;
                    let end = first_frame_ordinal
                        .checked_add(frame_count)
                        .is_some_and(|next| next == begin.limits.maximum_frame_count);
                    for decoded in packet.frames() {
                        let frame = decoded.expect("validated viewport packet frame");
                        let kind = if frame.ordinal == 0 {
                            decode_viewport_presentation_parent_frame(frame.bytes, begin)
                                .expect("decode viewport parent");
                            assert_eq!(frame.record_count, 0);
                            ViewportPresentationFrameKind::Begin
                        } else if frame.ordinal == 1 {
                            let directory =
                                decode_viewport_presentation_directory(frame.bytes, begin)
                                    .expect("decode viewport directory");
                            assert_eq!(frame.record_count, begin.envelope.ordered_leaf_count);
                            for entry in directory.entries() {
                                match entry.hio1_envelope.disposition {
                                    HotInlineSidecarDisposition::Authoritative { .. } => {
                                        authoritative += 1
                                    }
                                    HotInlineSidecarDisposition::Unsupported { .. } => {
                                        unsupported += 1
                                    }
                                }
                            }
                            ViewportPresentationFrameKind::Directory
                        } else if frame.ordinal
                            == begin
                                .limits
                                .maximum_frame_count
                                .checked_sub(1)
                                .expect("viewport has End")
                        {
                            let terminal =
                                decode_viewport_presentation_end_frame(frame.bytes, begin)
                                    .expect("decode viewport End");
                            assert_eq!(
                                terminal.actual_frame_count,
                                begin.limits.maximum_frame_count
                            );
                            assert!(
                                terminal.actual_encoded_frame_bytes
                                    <= begin.limits.maximum_encoded_frame_bytes
                            );
                            assert_eq!(frame.record_count, 0);
                            assert!(active_child.is_none());
                            ViewportPresentationFrameKind::End
                        } else {
                            let child =
                                decode_viewport_presentation_child_frame(frame.bytes, begin)
                                    .expect("decode opaque HIO1 child wrapper");
                            assert_eq!(frame.record_count, child.record_count);
                            match child.kind {
                                HotInlineSidecarFrameKind::Begin => {
                                    assert_eq!(child.child_frame_ordinal, 0);
                                    assert!(active_child.replace(child.directory_index).is_none());
                                    next_child_frame_ordinal = 1;
                                }
                                HotInlineSidecarFrameKind::Node => {
                                    assert_eq!(active_child, Some(child.directory_index));
                                    assert_eq!(child.child_frame_ordinal, next_child_frame_ordinal);
                                    assert_eq!(child.record_count, 1);
                                    next_child_frame_ordinal = next_child_frame_ordinal
                                        .checked_add(1)
                                        .expect("bounded child frame ordinal");
                                }
                                HotInlineSidecarFrameKind::End => {
                                    assert_eq!(active_child, Some(child.directory_index));
                                    assert_eq!(child.child_frame_ordinal, next_child_frame_ordinal);
                                    assert_eq!(child.record_count, 0);
                                    active_child = None;
                                    child_closures += 1;
                                }
                            }
                            ViewportPresentationFrameKind::Child
                        };
                        assert_eq!(
                            frame.digest,
                            protocol_digest128_from_blake3(
                                ProtocolDigestDomain::ViewportPresentationFrame,
                                viewport_presentation_frame_digest256(
                                    frame.ordinal,
                                    kind,
                                    frame.bytes,
                                ),
                            )
                        );
                        observed_frame_count = observed_frame_count
                            .checked_add(1)
                            .expect("bounded observed frame count");
                        observed_frame_bytes = observed_frame_bytes
                            .checked_add(
                                u32::try_from(frame.bytes.len())
                                    .expect("bounded observed frame bytes"),
                            )
                            .expect("bounded observed frame bytes");
                    }
                    host.admit_viewport_presentation_packet(packet)
                        .expect("independent host admits viewport packet");
                    endpoint
                        .accept_viewport_presentation_credit(credit, event_id)
                        .expect("accept viewport packet credit");
                    let (credited_offer_id, credited_next_frame_ordinal) = loop {
                        match host
                            .poll_viewport_presentation(HostWorkGrant {
                                inspect_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
                                copy_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
                                transitions: 1,
                            })
                            .expect("independent host polls viewport packet")
                        {
                            NativeViewportPresentationPollOutcome::Pending => {}
                            NativeViewportPresentationPollOutcome::PacketCredit {
                                offer_id,
                                next_frame_ordinal,
                            } => break (offer_id, next_frame_ordinal),
                            outcome => panic!("unexpected viewport packet outcome: {outcome:?}"),
                        }
                    };
                    assert!(
                        endpoint
                            .handle_viewport_presentation_host_poll(
                                event_id,
                                packet_offer_id,
                                ViewportPresentationHostPollPhase::PacketCredit,
                                ViewportPresentationHostPollResult::Completed(
                                    ViewportPresentationHostPollOutcome::PacketCredit {
                                        offer_id: credited_offer_id,
                                        next_frame_ordinal: credited_next_frame_ordinal,
                                    },
                                ),
                            )
                            .expect("accept exact viewport packet credit")
                            .is_none()
                    );
                    assert!(
                        endpoint.has_poll_work(),
                        "accepted packet credit must wake the next packet or commit"
                    );
                    if end {
                        assert_eq!(
                            first_frame_ordinal + frame_count,
                            begin.limits.maximum_frame_count
                        );
                    }
                }
                CandidateViewportPresentationEventBody::Commit(commit) => {
                    let Some(ViewportInlineBatchState::Streaming(streaming)) =
                        endpoint.viewport_inline_batch.as_ref()
                    else {
                        panic!("viewport commit retains streaming state")
                    };
                    let ack = streaming.expected_ack.expect("viewport expected ACK");
                    assert_eq!(commit.actual_frame_count, observed_frame_count);
                    assert_eq!(commit.actual_encoded_frame_bytes, observed_frame_bytes);
                    host.request_viewport_presentation_commit(commit)
                        .expect("independent host accepts viewport commit");
                    endpoint
                        .accept_viewport_presentation_credit(credit, event_id)
                        .expect("accept viewport commit credit");
                    let committed = loop {
                        match host
                            .poll_viewport_presentation(HostWorkGrant {
                                inspect_bytes: 0,
                                copy_bytes: 0,
                                transitions: 1,
                            })
                            .expect("independent host polls viewport commit")
                        {
                            NativeViewportPresentationPollOutcome::Pending => {}
                            NativeViewportPresentationPollOutcome::Committed(ack) => break ack,
                            outcome => panic!("unexpected viewport commit outcome: {outcome:?}"),
                        }
                    };
                    assert_eq!(committed, ack);
                    pending_event = endpoint
                        .handle_viewport_presentation_host_poll(
                            event_id,
                            commit.offer_id,
                            ViewportPresentationHostPollPhase::Commit,
                            ViewportPresentationHostPollResult::Completed(
                                ViewportPresentationHostPollOutcome::Committed(ack),
                            ),
                        )
                        .expect("accept exact viewport commit");
                }
                CandidateViewportPresentationEventBody::DeliveryAcknowledged(ack) => {
                    host.acknowledge_viewport_presentation_delivery(ack)
                        .expect("independent host acknowledges viewport delivery");
                    endpoint
                        .accept_viewport_presentation_credit(credit, event_id)
                        .expect("accept viewport delivery credit");
                    assert!(endpoint.viewport_inline_batch.is_none());
                    return (
                        offer.expect("viewport Begin"),
                        ack,
                        authoritative,
                        unsupported,
                        child_closures,
                    );
                }
            }
        }
        panic!("unit-fuel viewport presentation did not complete");
    }

    #[test]
    fn viewport_directory_product_max_fits_only_the_vpb1_wrapper_bound() {
        let bytes = VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES
            + 128 * VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES;
        assert!(bytes > M11_MAX_SNAPSHOT_FRAME_BYTES);
        assert!(bytes <= MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES as usize);
    }

    #[test]
    fn focused_inline_delivery_then_live_checkpoint_viewport_reaches_terminal() {
        const SOURCE: &str = "# A live document\n\
\n\
Write with **bold**, _emphasis_, `inline code`, and ~~strikethrough~~ while Flark keeps canonical Markdown exact.\n\
\n\
Browse <https://commonmark.org> or email <hello@example.com>. Links stay marker-free while their exact targets remain parser-owned.\n\
\n\
## A second idea\n\
\n\
```dart\n\
final message = 'Hello from Flark';\n\
```\n\
\n\
Tap any block to move the live editor, then start typing.";
        let profile = SourceFactsScanProfile::new(4).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [731, 732, 733, 734],
            source_session_identity: 735,
            worker_generation: 1,
        };
        let mut runtime = DocumentRuntime::new(SOURCE, standard_document_runtime_config())
            .expect("checkpoint runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start checkpoint candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("checkpoint host");
        host.observe_source_version(source_version_for(binding, completion))
            .expect("host observes checkpoint source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let viewport_command = |viewport_generation| ViewportInlineBatchCommand {
            binding,
            viewport_generation,
            source_version: delivery.ack.source_version,
            base_ack: delivery.ack,
            start_entry_ordinal: 0,
            start_byte_offset: 0,
            start_utf16_offset: 0,
            end_byte_offset: u32::try_from(SOURCE.len()).expect("bounded source"),
            end_utf16_offset: u32::try_from(SOURCE.encode_utf16().count()).expect("bounded UTF-16"),
            limits: ViewportInlineBatchLimits {
                maximum_structural_entries: 64,
                maximum_storage_pages: 25,
                maximum_inline_leaves: 64,
                maximum_inline_leaf_source_bytes: 8 * 1024,
                maximum_inline_source_bytes: 64 * 1024,
                maximum_fact_records: 2_048,
                maximum_projection_bytes: 512 * 1024,
                maximum_parser_transitions: 250_000,
            },
        };
        let focused_offset = SOURCE
            .find("start typing")
            .expect("focused checkpoint leaf");
        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 1,
                    source_version: delivery.ack.source_version,
                    base_ack: delivery.ack,
                    byte_offset: u32::try_from(focused_offset).expect("bounded focused point"),
                    utf16_offset: u32::try_from(SOURCE[..focused_offset].encode_utf16().count())
                        .expect("bounded focused UTF-16 point"),
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::Automatic,
                },
            )
            .expect("request checkpoint focused inline");
        let pending_inline = commit_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            30_000,
        );
        assert!(matches!(
            endpoint.request_viewport_inline_batch(&runtime, viewport_command(1)),
            Err(CandidateEndpointError::Busy)
        ));
        host.acknowledge_inline_sidecar_delivery(pending_inline.ack)
            .expect("host acknowledges focused inline delivery");
        endpoint
            .accept_hot_inline_credit(pending_inline.credit, pending_inline.event_id)
            .expect("parser accepts focused inline delivery");
        for _ in 0..10_000 {
            if !endpoint.hot_inline_has_poll_work() {
                break;
            }
            let transitions = endpoint
                .poll_hot_inline(&mut runtime, 1)
                .expect("release delivered focused inline");
            assert!(transitions <= 1);
        }
        assert!(!endpoint.hot_inline_has_poll_work());

        endpoint
            .request_viewport_inline_batch(&runtime, viewport_command(2))
            .expect("accepted checkpoint viewport");

        let mut preparation_polls = 0_usize;
        let mut preparation_transitions = 0_usize;
        loop {
            match endpoint.viewport_inline_batch.as_ref() {
                Some(ViewportInlineBatchState::Running(_)) => {}
                Some(ViewportInlineBatchState::Ready(ready)) => {
                    assert_eq!(ready.leaves.len(), 5);
                    assert_eq!(ready.total_ready_roots, 5);
                    assert_eq!(
                        ready.total_parser_transitions,
                        u64::try_from(preparation_transitions)
                            .expect("bounded preparation transitions")
                    );
                    assert!(ready.total_parser_transitions < 10_000);
                    break;
                }
                Some(ViewportInlineBatchState::Streaming(_)) => {
                    panic!("direct preparation polling must stop before streaming")
                }
                Some(ViewportInlineBatchState::Cancelling(_)) => {
                    panic!("accepted checkpoint viewport entered cleanup")
                }
                None => panic!("accepted checkpoint viewport disappeared"),
            }
            let transitions = endpoint
                .poll_viewport_inline_batch(&mut runtime, 1)
                .expect("bounded checkpoint viewport preparation");
            assert!(transitions <= 1);
            preparation_polls += 1;
            preparation_transitions += transitions;
            assert!(
                preparation_polls < 10_000,
                "checkpoint viewport preparation did not converge"
            );
            if matches!(
                endpoint.viewport_inline_batch,
                Some(ViewportInlineBatchState::Running(_))
            ) {
                assert_eq!(
                    transitions, 1,
                    "a still-running checkpoint inline job must make unit-fuel progress"
                );
            }
        }

        let (begin, ack, authoritative, unsupported, child_closures) =
            deliver_viewport_presentation_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
        assert_eq!(begin.binding.viewport_generation, 2);
        assert_eq!(ack.binding.viewport_generation, 2);
        assert_eq!(authoritative + unsupported, 5);
        assert_eq!(child_closures, 5);
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn default_sixty_four_leaf_profile_fits_the_private_vpb1_stream_ceiling() {
        const LEAF_COUNT: usize = 64;
        // At the default 64-leaf / 2,048-fact / 64-KiB-source caps, the
        // current parser needs at most 84 fact-tree nodes and 98 value-tree
        // nodes after per-root fragmentation. Every authoritative leaf adds
        // one synthetic bundle node. Keep the components explicit so a
        // projection-layout change cannot hide behind an aggregate.
        const MAXIMUM_FACT_TREE_NODES: usize = 84;
        const MAXIMUM_VALUE_TREE_NODES: usize = 98;
        const MAXIMUM_BUNDLE_NODES: usize = LEAF_COUNT;
        const MAXIMUM_TRANSFERRED_NODES: usize =
            MAXIMUM_FACT_TREE_NODES + MAXIMUM_VALUE_TREE_NODES + MAXIMUM_BUNDLE_NODES;
        const HIO1_ENVELOPE_BYTES: usize = 256;
        const IPR3_DESCRIPTOR_BYTES: usize = 280;
        const PRIVATE_STREAM_CEILING: usize = 2 * 1024 * 1024;

        assert_eq!(VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES, 144);
        assert_eq!(VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES, 12);
        assert_eq!(VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES, 184);
        assert_eq!(VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES, 28);
        assert_eq!(VIEWPORT_PRESENTATION_END_FRAME_BYTES, 52);
        assert_eq!(M11_INLINE_META_RECORD_BYTES, 48);
        assert_eq!(M11_MAX_SNAPSHOT_FRAME_BYTES, 5_140);
        assert_eq!(MAXIMUM_TRANSFERRED_NODES, 246);

        let outer_bytes = VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES
            + VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES
            + VIEWPORT_PRESENTATION_END_FRAME_BYTES;
        let per_leaf_bytes = VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES
            + HIO1_ENVELOPE_BYTES
            + IPR3_DESCRIPTOR_BYTES
            + 3 * M11_INLINE_META_RECORD_BYTES
            + 2 * VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES;
        let per_transferred_node_bytes =
            M11_MAX_SNAPSHOT_FRAME_BYTES + VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES;
        let maximum_encoded_bytes = outer_bytes
            + LEAF_COUNT * per_leaf_bytes
            + MAXIMUM_TRANSFERRED_NODES * per_transferred_node_bytes;

        assert_eq!(outer_bytes, 208);
        assert_eq!(per_leaf_bytes, 920);
        assert_eq!(per_transferred_node_bytes, 5_168);
        assert_eq!(maximum_encoded_bytes, 1_330_416);
        assert!(maximum_encoded_bytes <= PRIVATE_STREAM_CEILING);
    }

    #[test]
    fn viewport_inline_batch_publishes_twenty_four_children_then_post_begin_point_waits() {
        const PARAGRAPHS: usize = 24;
        const UNSUPPORTED_ORDINAL: usize = 12;
        let mut source = String::new();
        let mut paragraph_starts = Vec::with_capacity(PARAGRAPHS);
        for ordinal in 0..PARAGRAPHS {
            if ordinal != 0 {
                source.push_str("\n\n");
            }
            paragraph_starts.push(source.len());
            if ordinal == UNSUPPORTED_ORDINAL {
                source.push_str("before <tag>");
            } else {
                source.push_str(&format!(
                    "**bold{ordinal:02}** *em{ordinal:02}* `code{ordinal:02}`"
                ));
            }
        }
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [681, 682, 683, 684],
            source_session_identity: 685,
            worker_generation: 1,
        };
        let mut runtime = DocumentRuntime::new(&source, standard_document_runtime_config())
            .expect("viewport batch runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start segmented viewport candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("viewport batch host");
        host.observe_source_version(source_version_for(binding, completion))
            .expect("host observes viewport source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        let retained_descriptor = endpoint
            .retained
            .as_ref()
            .expect("retained viewport base")
            .publication
            .descriptor(&runtime)
            .expect("retained descriptor");
        let baseline_metrics = runtime.arena_metrics();

        let limits = ViewportInlineBatchLimits {
            maximum_structural_entries: 47,
            maximum_storage_pages: 2,
            maximum_inline_leaves: PARAGRAPHS as u32,
            maximum_inline_leaf_source_bytes: 8 * 1024,
            maximum_inline_source_bytes: 8 * 1024,
            maximum_fact_records: ((PARAGRAPHS - 1) * 3) as u64,
            maximum_projection_bytes: 1024 * 1024,
            maximum_parser_transitions: 100_000,
        };
        let command = |generation| ViewportInlineBatchCommand {
            binding,
            viewport_generation: generation,
            source_version: delivery.ack.source_version,
            base_ack: delivery.ack,
            start_entry_ordinal: 0,
            start_byte_offset: 0,
            start_utf16_offset: 0,
            end_byte_offset: u32::try_from(source.len()).expect("bounded source"),
            end_utf16_offset: u32::try_from(source.encode_utf16().count()).expect("bounded UTF-16"),
            limits,
        };
        let mut truncated_end = command(1);
        truncated_end.end_byte_offset = truncated_end.end_byte_offset.saturating_sub(1);
        truncated_end.end_utf16_offset = truncated_end.end_utf16_offset.saturating_sub(1);
        assert!(matches!(
            endpoint.request_viewport_inline_batch(&runtime, truncated_end),
            Err(CandidateEndpointError::Derive(
                M11CandidateDerivationError::ResultRangeMismatch
            ))
        ));
        assert!(endpoint.viewport_inline_batch.is_none());
        let mut one_byte_leaf_limit = command(1);
        one_byte_leaf_limit.limits.maximum_inline_leaf_source_bytes = 1;
        assert!(matches!(
            endpoint.request_viewport_inline_batch(&runtime, one_byte_leaf_limit),
            Err(CandidateEndpointError::ViewportInlineLimitExceeded(
                "inline leaf source bytes"
            ))
        ));
        assert!(endpoint.viewport_inline_batch.is_none());
        let mut aggregate_source_limited = command(1);
        aggregate_source_limited
            .limits
            .maximum_inline_leaf_source_bytes = 64;
        aggregate_source_limited.limits.maximum_inline_source_bytes = 64;
        let aggregate_source_result =
            endpoint.request_viewport_inline_batch(&runtime, aggregate_source_limited);
        assert!(
            matches!(
                aggregate_source_result,
                Err(CandidateEndpointError::ViewportInlineLimitExceeded(
                    "viewport range budget"
                ))
            ),
            "unexpected aggregate source bound result: {aggregate_source_result:?}",
        );
        assert!(endpoint.viewport_inline_batch.is_none());

        let first_blank_start = paragraph_starts[1] - 1;
        let mut blank_only = command(1);
        blank_only.start_entry_ordinal = 1;
        blank_only.start_byte_offset = first_blank_start as u32;
        blank_only.start_utf16_offset = first_blank_start as u32;
        blank_only.end_byte_offset = paragraph_starts[1] as u32;
        blank_only.end_utf16_offset = paragraph_starts[1] as u32;
        blank_only.limits.maximum_structural_entries = 1;
        blank_only.limits.maximum_inline_leaves = 1;
        blank_only.limits.maximum_inline_source_bytes = 1;
        blank_only.limits.maximum_fact_records = 1;
        blank_only.limits.maximum_projection_bytes = 1;
        blank_only.limits.maximum_parser_transitions = 1;
        endpoint
            .request_viewport_inline_batch(&runtime, blank_only)
            .expect("admit blank-only viewport");
        assert!(matches!(
            endpoint.viewport_inline_batch,
            Some(ViewportInlineBatchState::Running(ref running))
                if running.active.is_none()
                    && running.pending.is_empty()
        ));
        let before_blank_ready = runtime.arena_metrics();
        assert_eq!(
            endpoint
                .poll_viewport_inline_batch(&mut runtime, 1)
                .expect("finish blank-only viewport without reference scan"),
            0
        );
        assert!(matches!(
            endpoint.viewport_inline_batch,
            Some(ViewportInlineBatchState::Ready(ref ready))
                if ready.leaves.is_empty() && ready.total_parser_transitions == 0
        ));
        assert_eq!(runtime.arena_metrics(), before_blank_ready);
        endpoint.cancel_viewport_presentation();
        assert_eq!(
            endpoint
                .poll_viewport_inline_batch(&mut runtime, 1)
                .expect("close blank-only viewport"),
            0
        );
        assert!(endpoint.viewport_inline_batch.is_none());

        let mut fact_limited = command(2);
        fact_limited.limits.maximum_fact_records = 1;
        endpoint
            .request_viewport_inline_batch(&runtime, fact_limited)
            .expect("admit asynchronously fact-limited viewport");
        let mut failure_transitions = 0_usize;
        loop {
            match endpoint
                .poll(&mut runtime, 1)
                .expect("fact-limited viewport remains attempt-local")
            {
                CandidatePoll::Pending { transitions } => {
                    assert!(transitions <= 1);
                    failure_transitions += transitions;
                }
                CandidatePoll::ViewportPresentationUnavailable {
                    transitions,
                    viewport_generation,
                    reason,
                } => {
                    assert!(transitions <= 1);
                    failure_transitions += transitions;
                    assert_eq!(viewport_generation, 2);
                    assert_eq!(
                        reason,
                        ViewportPresentationUnavailableReason::BudgetExceeded
                    );
                    break;
                }
                CandidatePoll::ViewportPresentationEvent { .. }
                | CandidatePoll::Event { .. }
                | CandidatePoll::HotInlineEvent { .. } => {
                    panic!("fact-limited viewport must fail before publication")
                }
            }
        }
        assert!(failure_transitions > 0);
        assert!(endpoint.viewport_inline_batch.is_none());
        assert!(endpoint.pending_viewport_unavailable.is_none());

        let mut preempted_failure = command(3);
        preempted_failure.limits.maximum_fact_records = 1;
        endpoint
            .request_viewport_inline_batch(&runtime, preempted_failure)
            .expect("admit preempted fact-limited viewport");
        for _ in 0..1_000_000 {
            assert!(matches!(
                endpoint
                    .poll(&mut runtime, 1)
                    .expect("advance preempted viewport failure"),
                CandidatePoll::Pending { transitions } if transitions <= 1
            ));
            if endpoint.pending_viewport_unavailable.is_some() {
                break;
            }
        }
        assert_eq!(
            endpoint.pending_viewport_unavailable,
            Some((3, ViewportPresentationUnavailableReason::BudgetExceeded))
        );
        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 1,
                    source_version: delivery.ack.source_version,
                    base_ack: delivery.ack,
                    byte_offset: u32::try_from(paragraph_starts[0]).expect("bounded point"),
                    utf16_offset: 0,
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::Automatic,
                },
            )
            .expect("focused demand supersedes pending viewport failure");
        assert!(endpoint.pending_viewport_unavailable.is_none());
        for _ in 0..1_000_000 {
            if endpoint.viewport_inline_batch.is_none() {
                break;
            }
            assert!(
                endpoint
                    .poll_viewport_inline_batch(&mut runtime, 1)
                    .expect("drain superseded failure cleanup")
                    <= 1
            );
        }
        assert!(matches!(
            endpoint.hot_inline,
            Some(HotInlineState::AwaitingReferenceResolver(_))
        ));
        endpoint.cancel_hot_inline();
        for _ in 0..1_000_000 {
            if !endpoint.hot_inline_has_poll_work() {
                break;
            }
            assert!(
                endpoint
                    .poll_hot_inline(&mut runtime, 1)
                    .expect("drain superseding focused demand")
                    <= 1
            );
        }
        assert!(endpoint.pending_viewport_unavailable.is_none());
        endpoint
            .request_viewport_inline_batch(&runtime, command(4))
            .expect("start one-walk viewport batch");
        for _ in 0..1_000_000 {
            if matches!(
                endpoint.viewport_inline_batch,
                Some(ViewportInlineBatchState::Ready(_))
            ) {
                break;
            }
            let transitions = endpoint
                .poll_viewport_inline_batch(&mut runtime, 1)
                .expect("unit-fuel viewport poll");
            assert!(transitions <= 1);
        }
        let Some(ViewportInlineBatchState::Ready(ready)) = endpoint.viewport_inline_batch.as_ref()
        else {
            panic!("24-leaf viewport batch did not become ready");
        };
        assert_eq!(ready.command, command(4));
        assert_eq!(ready.descriptor, retained_descriptor);
        assert_eq!(ready.range_receipt.visited_entries(), 47);
        assert!(ready.range_receipt.storage_pages_visited() <= 2);
        assert_eq!(
            ready.range_receipt.next_byte_offset(),
            u64::try_from(source.len()).expect("bounded source")
        );
        assert_eq!(
            ready.range_receipt.next_utf16_offset(),
            u64::try_from(source.encode_utf16().count()).expect("bounded UTF-16")
        );
        assert_eq!(ready.leaves.len(), PARAGRAPHS);
        assert!(ready.total_inline_source_bytes <= limits.maximum_inline_source_bytes);
        assert!(ready.total_parser_transitions <= limits.maximum_parser_transitions);
        assert_eq!(ready.total_fact_records, limits.maximum_fact_records);
        assert_eq!(ready.total_ready_roots, (PARAGRAPHS - 1) as u32);
        let mut authoritative = 0_usize;
        let mut unsupported = 0_usize;
        for (index, leaf) in ready.leaves.iter().enumerate() {
            assert_eq!(leaf.geometry.kind, M11BlockSequenceEntryKind::Paragraph);
            assert_eq!(leaf.geometry.entry_ordinal, (index * 2) as u64);
            assert_eq!(
                leaf.geometry.block_source.start,
                paragraph_starts[index] as u32
            );
            assert!(leaf.geometry.block_source.start < leaf.geometry.block_source.end);
            assert!(leaf.geometry.block_source_utf16.start < leaf.geometry.block_source_utf16.end);
            assert!(leaf.geometry.inline_source.start < leaf.geometry.inline_source.end);
            assert!(
                leaf.geometry.inline_source.end - leaf.geometry.inline_source.start
                    <= limits.maximum_inline_leaf_source_bytes
            );
            assert!(
                leaf.geometry.inline_source_utf16.start < leaf.geometry.inline_source_utf16.end
            );
            assert_eq!(leaf.parser_profile, parser_profile);
            match &leaf.publication {
                ViewportInlineLeafPublication::Authoritative(root) => {
                    assert_ne!(index, UNSUPPORTED_ORDINAL);
                    assert_eq!(root.descriptor().fact_count(), 3);
                    assert_eq!(
                        root.descriptor().source_range(),
                        &leaf.geometry.inline_source
                    );
                    authoritative += 1;
                }
                ViewportInlineLeafPublication::Unsupported(record) => {
                    assert_eq!(index, UNSUPPORTED_ORDINAL);
                    assert_eq!(record.source_range(), leaf.geometry.inline_source);
                    unsupported += 1;
                }
            }
        }
        assert_eq!(authoritative, PARAGRAPHS - 1);
        assert_eq!(unsupported, 1);

        let (
            viewport_offer,
            viewport_ack,
            published_authoritative,
            published_unsupported,
            closures,
        ) = deliver_viewport_presentation_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
        assert_eq!(viewport_offer.base_ack, delivery.ack);
        assert_eq!(viewport_offer.binding.viewport_generation, 4);
        assert_eq!(
            viewport_offer.envelope.ordered_leaf_count,
            PARAGRAPHS as u32
        );
        assert_eq!(viewport_ack.base_ack, delivery.ack);
        assert_eq!(viewport_ack.binding, viewport_offer.binding);
        assert_eq!(viewport_ack.envelope, viewport_offer.envelope);
        assert_eq!(published_authoritative, PARAGRAPHS - 1);
        assert_eq!(published_unsupported, 1);
        assert_eq!(closures, PARAGRAPHS);
        let settled_baseline = runtime.arena_metrics();
        assert!(
            settled_baseline.resident_nodes <= baseline_metrics.resident_nodes
                && settled_baseline.live_payload_bytes <= baseline_metrics.live_payload_bytes
                && settled_baseline.reserved_external_payload_bytes
                    > baseline_metrics.reserved_external_payload_bytes
                && settled_baseline.pending_reclaims == 0
                && settled_baseline.live_builds == 0,
            "ready viewport roots and authorities must leave only the retained reference index: \
             before={baseline_metrics:?}, after={settled_baseline:?}"
        );

        let mut transition_limited = command(5);
        transition_limited.limits.maximum_parser_transitions = 1;
        endpoint
            .request_viewport_inline_batch(&runtime, transition_limited)
            .expect("start transition-limited viewport batch");
        let mut observed_transition_limit = false;
        for _ in 0..1_000_000 {
            match endpoint
                .poll(&mut runtime, 1)
                .expect("transition-limited viewport remains attempt-local")
            {
                CandidatePoll::Pending { transitions } => {
                    assert_eq!(
                        transitions, 1,
                        "active bounded viewport work must never yield zero progress"
                    );
                }
                CandidatePoll::ViewportPresentationUnavailable {
                    transitions,
                    viewport_generation,
                    reason,
                } => {
                    assert!(transitions <= 1);
                    assert_eq!(viewport_generation, 5);
                    assert_eq!(
                        reason,
                        ViewportPresentationUnavailableReason::BudgetExceeded
                    );
                    observed_transition_limit = true;
                    break;
                }
                CandidatePoll::ViewportPresentationEvent { .. }
                | CandidatePoll::Event { .. }
                | CandidatePoll::HotInlineEvent { .. } => {
                    panic!("transition-limited viewport must fail before publication")
                }
            }
        }
        assert!(observed_transition_limit);
        assert!(endpoint.viewport_inline_batch.is_none());
        assert!(endpoint.pending_viewport_unavailable.is_none());

        endpoint
            .request_viewport_inline_batch(&runtime, command(6))
            .expect("start parser-local preempted viewport batch");
        let parser_local_point = InlineRefinementCommand {
            binding,
            refinement_generation: 2,
            source_version: delivery.ack.source_version,
            base_ack: delivery.ack,
            byte_offset: u32::try_from(paragraph_starts[0]).expect("bounded point"),
            utf16_offset: 0,
            affinity: InlinePointAffinity::After,
            target: InlineRefinementTarget::Automatic,
        };
        endpoint
            .request_hot_inline(&mut runtime, parser_local_point)
            .expect("focused point preempts viewport work before Begin escapes");
        assert!(matches!(
            endpoint.viewport_inline_batch,
            Some(ViewportInlineBatchState::Cancelling(ref cleanup))
                if cleanup.hot_replacement.is_some()
        ));
        for _ in 0..1_000_000 {
            if endpoint.viewport_inline_batch.is_none() {
                break;
            }
            assert!(
                endpoint
                    .poll_viewport_inline_batch(&mut runtime, 1)
                    .expect("drain parser-local viewport preemption")
                    <= 1
            );
        }
        assert!(matches!(
            endpoint.hot_inline,
            Some(HotInlineState::AwaitingReferenceResolver(_))
        ));
        endpoint.cancel_hot_inline();
        for _ in 0..1_000_000 {
            if !endpoint.hot_inline_has_poll_work() {
                break;
            }
            assert!(
                endpoint
                    .poll_hot_inline(&mut runtime, 1)
                    .expect("drain parser-local focused demand")
                    <= 1
            );
        }
        assert!(!endpoint.hot_inline_has_poll_work());

        endpoint
            .request_viewport_inline_batch(&runtime, command(7))
            .expect("start post-Begin protected viewport batch");
        let begin_credit = loop {
            match endpoint
                .poll(&mut runtime, 1)
                .expect("derive cancellable viewport stream")
            {
                CandidatePoll::Pending { transitions } => assert!(transitions <= 1),
                CandidatePoll::ViewportPresentationEvent { event, .. } => {
                    let CandidateViewportPresentationEvent {
                        credit,
                        body: CandidateViewportPresentationEventBody::Begin(begin),
                    } = *event
                    else {
                        panic!("cancellable viewport must emit Begin first")
                    };
                    assert_eq!(begin.binding.viewport_generation, 7);
                    break credit;
                }
                CandidatePoll::Event { .. }
                | CandidatePoll::HotInlineEvent { .. }
                | CandidatePoll::ViewportPresentationUnavailable { .. } => {
                    panic!("cancellable viewport emitted an unrelated event")
                }
            }
        };
        endpoint
            .accept_viewport_presentation_credit(begin_credit, 50_000)
            .expect("accept cancellable viewport Begin");
        assert!(matches!(
            endpoint
                .poll(&mut runtime, 1)
                .expect("buffer one partial viewport child"),
            CandidatePoll::Pending { transitions: 1 }
        ));
        assert!(matches!(
            endpoint.viewport_inline_batch,
            Some(ViewportInlineBatchState::Streaming(ref streaming))
                if streaming.active.is_some() && !streaming.packet.frames.is_empty()
        ));
        let post_begin_point = InlineRefinementCommand {
            binding,
            refinement_generation: 3,
            source_version: delivery.ack.source_version,
            base_ack: delivery.ack,
            byte_offset: u32::try_from(paragraph_starts[0]).expect("bounded point"),
            utf16_offset: 0,
            affinity: InlinePointAffinity::After,
            target: InlineRefinementTarget::Automatic,
        };
        assert!(matches!(
            endpoint.request_hot_inline(&mut runtime, post_begin_point),
            Err(CandidateEndpointError::Busy)
        ));
        assert!(matches!(
            endpoint.viewport_inline_batch,
            Some(ViewportInlineBatchState::Streaming(ref streaming))
                if streaming.phase != StreamPhase::NeedBegin
        ));
        assert!(endpoint.hot_inline.is_none());
        endpoint.cancel_viewport_presentation();
        assert!(matches!(
            endpoint.viewport_inline_batch,
            Some(ViewportInlineBatchState::Cancelling(_))
        ));
        for _ in 0..1_000_000 {
            if endpoint.viewport_inline_batch.is_none() {
                break;
            }
            assert!(
                endpoint
                    .poll_viewport_inline_batch(&mut runtime, 1)
                    .expect("drain preempted viewport batch")
                    <= 1
            );
        }
        assert!(endpoint.viewport_inline_batch.is_none());
        endpoint
            .request_hot_inline(&mut runtime, post_begin_point)
            .expect("focused point retries after viewport terminal cleanup");
        assert!(matches!(
            endpoint.hot_inline,
            Some(HotInlineState::AwaitingReferenceResolver(_))
        ));
        endpoint.cancel_hot_inline();
        for _ in 0..1_000_000 {
            if !endpoint.hot_inline_has_poll_work() {
                break;
            }
            assert!(
                endpoint
                    .poll_hot_inline(&mut runtime, 1)
                    .expect("drain urgent point cancellation")
                    <= 1
            );
        }
        assert!(!endpoint.hot_inline_has_poll_work());
        let after_preemption = runtime.arena_metrics();
        assert_eq!(
            after_preemption.resident_nodes,
            settled_baseline.resident_nodes
        );
        assert_eq!(
            after_preemption.live_payload_bytes,
            settled_baseline.live_payload_bytes
        );
        assert_eq!(
            after_preemption.reserved_external_payload_bytes,
            settled_baseline.reserved_external_payload_bytes
        );
        assert_eq!(after_preemption.pending_reclaims, 0);
        assert_eq!(after_preemption.live_builds, 0);
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn late_inline_sidecar_publishes_authoritative_and_unsupported_then_cancels_exactly() {
        const SOURCE: &str = "p\n\n**bold**\n\nq";
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [701, 702, 703, 704],
            source_session_identity: 705,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start segmented candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(source_version_for(binding, completion))
            .expect("host observes source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        let retained_descriptor = endpoint
            .retained
            .as_ref()
            .expect("retained structural base")
            .publication
            .descriptor(&runtime)
            .expect("retained descriptor");

        let command = |generation: u32, byte_offset: usize| InlineRefinementCommand {
            binding,
            refinement_generation: generation,
            source_version: delivery.ack.source_version,
            base_ack: delivery.ack,
            byte_offset: u32::try_from(byte_offset).expect("bounded point"),
            utf16_offset: u32::try_from(SOURCE[..byte_offset].encode_utf16().count())
                .expect("bounded UTF-16 point"),
            affinity: InlinePointAffinity::After,
            target: InlineRefinementTarget::Automatic,
        };
        let middle = SOURCE.find("**bold**").expect("middle Paragraph");
        endpoint
            .request_hot_inline(&mut runtime, command(1, middle))
            .expect("first demand");
        endpoint
            .request_hot_inline(&mut runtime, command(2, middle + 2))
            .expect("same-leaf demand coalesces");
        assert!(matches!(
            endpoint.request_hot_inline(&mut runtime, command(2, middle + 3)),
            Err(CandidateEndpointError::InvalidAuthority)
        ));
        let (authoritative_begin, authoritative_ack) =
            deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 10_000);
        assert_eq!(authoritative_begin.base_ack, delivery.ack);
        assert_eq!(authoritative_begin.binding.refinement_generation, 2);
        assert_eq!(
            authoritative_begin.binding.physical_start_utf8 as usize,
            middle
        );
        assert!(matches!(
            authoritative_begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative { fact_count, .. } if fact_count > 0
        ));
        assert_eq!(
            authoritative_ack.disposition,
            InlineSidecarAckDisposition::Authoritative
        );
        assert_eq!(
            authoritative_ack.transferred_node_count,
            authoritative_begin.envelope.transferred_node_count
        );

        let blank = middle - 1;
        endpoint
            .request_hot_inline(&mut runtime, command(3, blank))
            .expect("blank demand");
        let (unsupported_begin, unsupported_ack) =
            deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 20_000);
        assert!(matches!(
            unsupported_begin.envelope.disposition,
            HotInlineSidecarDisposition::Unsupported {
                reason: HOT_INLINE_UNSUPPORTED_NOT_INLINE_LEAF,
                ..
            }
        ));
        assert_eq!(
            unsupported_ack.disposition,
            InlineSidecarAckDisposition::Unsupported
        );
        assert_eq!(unsupported_ack.transferred_node_count, 1);
        assert_ne!(
            authoritative_begin.publication_session,
            unsupported_begin.publication_session
        );

        let tail = SOURCE.rfind('q').expect("tail Paragraph");
        endpoint
            .request_hot_inline(&mut runtime, command(4, tail))
            .expect("tail demand");
        let cancelled_begin = loop {
            match endpoint
                .poll(&mut runtime, 1)
                .expect("unit-fuel cancellable sidecar poll")
            {
                CandidatePoll::Pending { transitions } => assert!(transitions <= 1),
                CandidatePoll::HotInlineEvent { event, .. } => {
                    let HotInlineEvent {
                        credit,
                        body: HotInlineEventBody::Begin(begin),
                    } = *event
                    else {
                        panic!("cancellable sidecar must begin before packet emission");
                    };
                    endpoint
                        .accept_hot_inline_credit(credit, 30_000)
                        .expect("accept cancellable Begin credit");
                    break begin;
                }
                CandidatePoll::Event { .. } => {
                    panic!("late inline demand must not republish structure")
                }
                CandidatePoll::ViewportPresentationEvent { .. } => {
                    panic!("late inline demand must not emit viewport work")
                }
                CandidatePoll::ViewportPresentationUnavailable { .. } => {
                    panic!("late inline demand emitted stale viewport unavailability")
                }
            }
        };
        assert_eq!(cancelled_begin.binding.refinement_generation, 4);
        endpoint.cancel_hot_inline();
        assert!(endpoint.hot_inline_sidecar.is_none());
        for _ in 0..100_000 {
            if !endpoint.hot_inline_has_poll_work() {
                break;
            }
            assert!(matches!(
                endpoint
                    .poll(&mut runtime, 1)
                    .expect("fuelled cancelled sidecar reclamation"),
                CandidatePoll::Pending { transitions } if transitions <= 1
            ));
        }
        assert!(!endpoint.hot_inline_has_poll_work());

        assert_eq!(
            endpoint
                .retained
                .as_ref()
                .expect("structural base remains retained")
                .publication
                .descriptor(&runtime)
                .expect("descriptor after inline demands"),
            retained_descriptor,
            "late inline work must not republish the canonical candidate"
        );
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn character_references_reach_the_independent_host_as_fixed_width_cooked_scalars() {
        const SOURCE: &str = "&copy; &NotEqualTilde;";
        const FACT_RECORD_BYTES: usize = 20;
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [706, 707, 708, 709],
            source_session_identity: 710,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start character-reference candidate");
        let source_version = source_version_for(binding, completion);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent character-reference host");
        host.observe_source_version(source_version)
            .expect("host observes character-reference source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 1,
                    source_version: delivery.ack.source_version,
                    base_ack: delivery.ack,
                    byte_offset: 1,
                    utf16_offset: 1,
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::Automatic,
                },
            )
            .expect("request character-reference inline authority");
        let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            35_000,
        );
        assert_eq!(begin.binding.physical_start_utf8, 0);
        assert_eq!(begin.binding.physical_end_utf8, SOURCE.len() as u32);
        assert_eq!(begin.binding.visible_start_utf8, 0);
        assert_eq!(begin.binding.visible_end_utf8, SOURCE.len() as u32);
        assert_eq!(begin.binding.visible_start_utf16, 0);
        assert_eq!(
            begin.binding.visible_end_utf16,
            SOURCE.encode_utf16().count() as u32
        );
        assert!(matches!(
            begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative { fact_count: 2, .. }
        ));
        assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);

        let mut facts = [0_u8; 2 * FACT_RECORD_BYTES];
        assert!(matches!(
            host.query_inline_sidecar(begin.binding, &mut facts)
                .expect("query character-reference inline sidecar"),
            HostInlineSidecarQueryOutcome::Authoritative {
                fact_count: 2,
                encoded_bytes: 40,
                ..
            }
        ));
        let first = &facts[..FACT_RECORD_BYTES];
        assert_eq!(first[0], M11InlineProjectionKind::CharacterReference as u8);
        assert_eq!(first[1], 1);
        assert_eq!(&first[2..4], &[0; 2]);
        assert_eq!(u32::from_le_bytes(first[4..8].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(first[8..12].try_into().unwrap()), 6);
        assert_eq!(
            u32::from_le_bytes(first[12..16].try_into().unwrap()),
            '©' as u32
        );
        assert_eq!(u32::from_le_bytes(first[16..20].try_into().unwrap()), 0);

        let second = &facts[FACT_RECORD_BYTES..];
        assert_eq!(second[0], M11InlineProjectionKind::CharacterReference as u8);
        assert_eq!(second[1], 2);
        assert_eq!(&second[2..4], &[0; 2]);
        assert_eq!(u32::from_le_bytes(second[4..8].try_into().unwrap()), 7);
        assert_eq!(u32::from_le_bytes(second[8..12].try_into().unwrap()), 15);
        assert_eq!(
            u32::from_le_bytes(second[12..16].try_into().unwrap()),
            '\u{2242}' as u32
        );
        assert_eq!(
            u32::from_le_bytes(second[16..20].try_into().unwrap()),
            '\u{0338}' as u32
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn retained_reference_winner_reaches_hot_inline_sidecar_without_shortcut_fallback() {
        const SOURCE: &str = "[foo][bar][baz] padding long enough\n\n[baz]: /baz\n";
        const FACT_RECORD_BYTES: usize = 20;
        const LINK_VALUE_PREFIX_BYTES: usize = 16;
        const LINK_VALUE_ENTRY_BYTES: usize = 32;
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [711, 712, 713, 714],
            source_session_identity: 715,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start reference candidate");
        let source_version = source_version_for(binding, completion);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent reference host");
        host.observe_source_version(source_version)
            .expect("host observes reference source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 1,
                    source_version: delivery.ack.source_version,
                    base_ack: delivery.ack,
                    byte_offset: 1,
                    utf16_offset: 1,
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::Automatic,
                },
            )
            .expect("request reference inline authority");
        assert!(matches!(
            endpoint.hot_inline,
            Some(HotInlineState::AwaitingReferenceResolver(_))
        ));
        let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            35_000,
        );
        assert!(matches!(
            begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative { fact_count: 1, .. }
        ));
        assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);

        let mut encoded = [0_u8; 128];
        let HostInlineSidecarQueryOutcome::Authoritative {
            fact_count,
            value_entry_count,
            value_encoded_bytes,
            encoded_bytes,
            ..
        } = host
            .query_inline_sidecar(begin.binding, &mut encoded)
            .expect("query resolved-reference inline sidecar")
        else {
            panic!("resolved reference must publish authoritative inline facts")
        };
        let destination_start = SOURCE.find("/baz").expect("definition destination") as u32;
        assert_eq!(fact_count, 1);
        assert_eq!(value_entry_count, 1);
        assert_eq!(value_encoded_bytes, 52);
        assert_eq!(encoded_bytes, 72);

        let fact = &encoded[..FACT_RECORD_BYTES];
        assert_eq!(fact[0], M11InlineProjectionKind::ReferenceLink as u8);
        assert_eq!(u32::from_le_bytes(fact[4..8].try_into().unwrap()), 5);
        assert_eq!(u32::from_le_bytes(fact[8..12].try_into().unwrap()), 10);
        assert_eq!(u32::from_le_bytes(fact[12..16].try_into().unwrap()), 6);
        assert_eq!(u32::from_le_bytes(fact[16..20].try_into().unwrap()), 3);

        let values = &encoded[FACT_RECORD_BYTES..encoded_bytes as usize];
        assert_eq!(&values[..8], b"FLKIV001");
        assert_eq!(u32::from_le_bytes(values[8..12].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(values[12..16].try_into().unwrap()), 1);
        let entry = &values[LINK_VALUE_PREFIX_BYTES..];
        assert_eq!(u32::from_le_bytes(entry[0..4].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(entry[4..8].try_into().unwrap()), 0);
        assert_eq!(
            u32::from_le_bytes(entry[8..12].try_into().unwrap()),
            destination_start
        );
        assert_eq!(u32::from_le_bytes(entry[12..16].try_into().unwrap()), 4);
        assert_eq!(u32::from_le_bytes(entry[16..20].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(entry[20..24].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(entry[24..28].try_into().unwrap()), 4);
        assert_eq!(u32::from_le_bytes(entry[28..32].try_into().unwrap()), 0);
        assert_eq!(
            &entry[LINK_VALUE_ENTRY_BYTES..LINK_VALUE_ENTRY_BYTES + 4],
            b"/baz"
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn length_changing_direct_link_edit_before_late_references_recertifies_inline() {
        const BASE_SOURCE: &str = "Read the [Flark architecture notes](https://flark.dev/revision-7 \"Revision 7\").\n\nReference [full][launch notes].\n\n[launch notes]: https://flark.dev/launch \"Launch notes\"\n";
        const ORIGINAL_LABEL: &str = "Flark architecture notes";
        const TARGET_LABEL: &str = "Flark design notes";
        let profile = SourceFactsScanProfile::new(4).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [716, 717, 718, 719],
            source_session_identity: 720,
            worker_generation: 1,
        };
        let mut runtime = DocumentRuntime::new(BASE_SOURCE, standard_document_runtime_config())
            .expect("revision-replacement runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start reference-bearing base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent revision-replacement host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes reference-bearing base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let direct_point = BASE_SOURCE
            .find(ORIGINAL_LABEL)
            .expect("direct-link label point");
        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 1,
                    source_version: base_delivery.ack.source_version,
                    base_ack: base_delivery.ack,
                    byte_offset: u32::try_from(direct_point).expect("bounded point"),
                    utf16_offset: u32::try_from(BASE_SOURCE[..direct_point].encode_utf16().count())
                        .expect("bounded UTF-16 point"),
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::Automatic,
                },
            )
            .expect("request active direct-link authority");
        let (base_inline, _) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            36_000,
        );
        assert!(matches!(
            base_inline.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative { fact_count: 1, .. }
        ));
        assert!(endpoint.hot_inline_has_poll_work());

        let label_start = BASE_SOURCE.find(ORIGINAL_LABEL).expect("edit label");
        let target_version = runtime
            .apply_edit(
                base_version,
                label_start..label_start + ORIGINAL_LABEL.len(),
                TARGET_LABEL,
            )
            .expect("length-changing direct-link edit")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan direct-link SourceFacts");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight direct-link exact base")
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("borrow direct-link target");
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        host.observe_source_version(source_version_for(binding, target_completion))
            .expect("host observes direct-link target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start direct-link revision replacement");
        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(target_delivery.offer.mode, PublicationMode::FullSnapshot);

        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(!endpoint.hot_inline_has_poll_work());
        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 2,
                    source_version: target_delivery.ack.source_version,
                    base_ack: target_delivery.ack,
                    byte_offset: u32::try_from(direct_point).expect("bounded point"),
                    utf16_offset: u32::try_from(BASE_SOURCE[..direct_point].encode_utf16().count())
                        .expect("bounded UTF-16 point"),
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::Automatic,
                },
            )
            .expect("request recertified direct-link authority");
        let (target_inline, _) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            37_000,
        );
        assert!(matches!(
            target_inline.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative { fact_count: 1, .. }
        ));
        assert_eq!(target_delivery.ack.source_version.revision, 1);
        assert_eq!(target_version.revision().get(), 1);

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn indented_code_request_reaches_typed_sidecar_and_reclaims_with_unit_fuel() {
        const SOURCE: &str = "\u{feff}\tα\0\r\n\n      \r    \tβ\r\tlast";
        const LINE_RECORD_BYTES: usize = 20;
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [706, 707, 708, 709],
            source_session_identity: 710,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start segmented candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(source_version_for(binding, completion))
            .expect("host observes source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let command = |generation: u32| InlineRefinementCommand {
            binding,
            refinement_generation: generation,
            source_version: delivery.ack.source_version,
            base_ack: delivery.ack,
            byte_offset: 0,
            utf16_offset: 0,
            affinity: InlinePointAffinity::After,
            target: InlineRefinementTarget::Automatic,
        };

        endpoint
            .request_hot_inline(&mut runtime, command(1))
            .expect("first indented-code demand");
        assert!(matches!(
            endpoint
                .poll(&mut runtime, 1)
                .expect("bounded indented-code projection"),
            CandidatePoll::Pending { transitions } if transitions <= 1
        ));
        assert!(
            endpoint.hot_inline_sidecar.is_none(),
            "one transition cannot publish a multi-line projection"
        );
        endpoint.cancel_hot_inline();
        for _ in 0..100_000 {
            if !endpoint.hot_inline_has_poll_work() {
                break;
            }
            assert!(matches!(
                endpoint
                    .poll(&mut runtime, 1)
                    .expect("fuelled indented-code cancellation"),
                CandidatePoll::Pending { transitions } if transitions <= 1
            ));
        }
        assert!(!endpoint.hot_inline_has_poll_work());

        endpoint
            .request_hot_inline(&mut runtime, command(2))
            .expect("replacement indented-code demand");
        let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            70_000,
        );
        let source_utf16 = u32::try_from(SOURCE.encode_utf16().count()).expect("bounded source");
        assert_eq!(begin.binding.refinement_generation, 2);
        assert_eq!(begin.binding.physical_start_utf8, 0);
        assert_eq!(begin.binding.physical_end_utf8, SOURCE.len() as u32);
        assert_eq!(begin.binding.visible_start_utf8, 0);
        assert_eq!(begin.binding.visible_end_utf8, SOURCE.len() as u32);
        assert_eq!(begin.binding.physical_start_utf16, 0);
        assert_eq!(begin.binding.physical_end_utf16, source_utf16);
        assert_eq!(begin.binding.visible_start_utf16, 0);
        assert_eq!(begin.binding.visible_end_utf16, source_utf16);
        assert!(matches!(
            begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative {
                fact_count: 5,
                logical_page_count: 1,
                ..
            }
        ));
        assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);
        assert_eq!(
            ack.transferred_node_count,
            begin.envelope.transferred_node_count
        );

        let mut encoded_lines = [0_u8; 5 * LINE_RECORD_BYTES];
        let query = host
            .query_inline_sidecar(begin.binding, &mut encoded_lines)
            .expect("query typed indented-code sidecar");
        assert!(matches!(
            query,
            HostInlineSidecarQueryOutcome::Authoritative {
                fact_count: 5,
                encoded_bytes: 100,
                ..
            }
        ));
        let observed = encoded_lines
            .chunks_exact(LINE_RECORD_BYTES)
            .map(|record| {
                std::array::from_fn::<u32, 5, _>(|field| {
                    let start = field * 4;
                    u32::from_le_bytes(
                        record[start..start + 4]
                            .try_into()
                            .expect("four-byte line field"),
                    )
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            observed,
            vec![
                [0, 9, 4, 3, 0],
                [9, 1, 0, 0, 1],
                [10, 7, 4, 2, 0],
                [17, 8, 4, 3, 0],
                [25, 5, 1, 4, 0],
            ]
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn block_quote_request_reaches_typed_sidecar_and_reclaims_with_unit_fuel() {
        const SOURCE: &str = "\u{feff}   > α😀\r\n> β\rlazy😀\0";
        const LINE_RECORD_BYTES: usize = 20;
        const VIEWPORT_HEADER_BYTES: usize = 32;
        const GREEN_RECORD_BYTES: usize = 80;
        const PROJECTION_RECORD_BYTES: usize = 56;
        const POINT_PATH_NODE_BYTES: usize = 40;
        const POINT_PATH_BYTES: usize = 2 * POINT_PATH_NODE_BYTES;
        const VIEWPORT_BYTES: usize = VIEWPORT_HEADER_BYTES
            + GREEN_RECORD_BYTES
            + PROJECTION_RECORD_BYTES
            + POINT_PATH_BYTES
            + 3 * LINE_RECORD_BYTES;
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [716, 717, 718, 719],
            source_session_identity: 720,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start segmented candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(source_version_for(binding, completion))
            .expect("host observes source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let command = |generation: u32| InlineRefinementCommand {
            binding,
            refinement_generation: generation,
            source_version: delivery.ack.source_version,
            base_ack: delivery.ack,
            byte_offset: 0,
            utf16_offset: 0,
            affinity: InlinePointAffinity::After,
            target: InlineRefinementTarget::Automatic,
        };

        endpoint
            .request_hot_inline(&mut runtime, command(1))
            .expect("first block-quote demand");
        assert!(matches!(
            endpoint
                .poll(&mut runtime, 1)
                .expect("bounded block-quote projection"),
            CandidatePoll::Pending { transitions } if transitions <= 1
        ));
        assert!(
            endpoint.hot_inline_sidecar.is_none(),
            "one transition cannot publish a multi-line projection"
        );
        endpoint.cancel_hot_inline();
        for _ in 0..100_000 {
            if !endpoint.hot_inline_has_poll_work() {
                break;
            }
            assert!(matches!(
                endpoint
                    .poll(&mut runtime, 1)
                    .expect("fuelled block-quote cancellation"),
                CandidatePoll::Pending { transitions } if transitions <= 1
            ));
        }
        assert!(!endpoint.hot_inline_has_poll_work());

        endpoint
            .request_hot_inline(&mut runtime, command(2))
            .expect("replacement block-quote demand");
        let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            80_000,
        );
        let source_utf16 = u32::try_from(SOURCE.encode_utf16().count()).expect("bounded source");
        assert_eq!(begin.binding.refinement_generation, 2);
        assert_eq!(begin.binding.physical_start_utf8, 0);
        assert_eq!(begin.binding.physical_end_utf8, SOURCE.len() as u32);
        assert_eq!(begin.binding.visible_start_utf8, 0);
        assert_eq!(begin.binding.visible_end_utf8, SOURCE.len() as u32);
        assert_eq!(begin.binding.physical_start_utf16, 0);
        assert_eq!(begin.binding.physical_end_utf16, source_utf16);
        assert_eq!(begin.binding.visible_start_utf16, 0);
        assert_eq!(begin.binding.visible_end_utf16, source_utf16);
        assert!(matches!(
            begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative {
                fact_count: 3,
                logical_page_count: 1,
                ..
            }
        ));
        assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);
        assert_eq!(
            ack.transferred_node_count,
            begin.envelope.transferred_node_count
        );

        let mut encoded_lines = [0_u8; 3 * LINE_RECORD_BYTES];
        let query = host
            .query_inline_sidecar(begin.binding, &mut encoded_lines)
            .expect("query typed block-quote sidecar");
        assert!(matches!(
            query,
            HostInlineSidecarQueryOutcome::Authoritative {
                fact_count: 3,
                encoded_bytes: 60,
                ..
            }
        ));
        let observed = encoded_lines
            .chunks_exact(LINE_RECORD_BYTES)
            .map(|record| {
                std::array::from_fn::<u32, 5, _>(|field| {
                    let start = field * 4;
                    u32::from_le_bytes(
                        record[start..start + 4]
                            .try_into()
                            .expect("four-byte line field"),
                    )
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            observed,
            vec![[0, 16, 8, 6, 1], [16, 5, 2, 2, 1], [21, 9, 0, 9, 2]]
        );

        let mut viewport = [0xa5_u8; VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: delivery.ack.source_version,
                    position: HostSourceMetric { bytes: 8, utf16: 6 },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: VIEWPORT_BYTES as u32,
                        maximum_open_depth: 2,
                        maximum_leaf_count: 4,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut viewport,
            )
            .expect("joined block-quote viewport");
        let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
            panic!("block quote and sidecar must author one schema-4 viewport: {outcome:?}");
        };
        assert_eq!(range.start, HostSourceMetric { bytes: 0, utf16: 0 });
        assert_eq!(
            range.end,
            HostSourceMetric {
                bytes: SOURCE.len() as u32,
                utf16: source_utf16,
            }
        );
        assert_eq!(receipt.encoded_bytes, VIEWPORT_BYTES as u32);
        assert_eq!(u32::from_le_bytes(viewport[8..12].try_into().unwrap()), 4);
        assert_eq!(
            u32::from_le_bytes(viewport[12..16].try_into().unwrap()),
            GREEN_RECORD_BYTES as u32
        );
        assert_eq!(
            u32::from_le_bytes(viewport[16..20].try_into().unwrap()),
            PROJECTION_RECORD_BYTES as u32
        );
        assert_eq!(u16::from_le_bytes(viewport[20..22].try_into().unwrap()), 2);
        assert_eq!(viewport[22], 3);
        assert_eq!(viewport[23], 0);
        assert_eq!(
            u32::from_le_bytes(viewport[24..28].try_into().unwrap()),
            POINT_PATH_BYTES as u32
        );
        assert_eq!(
            u32::from_le_bytes(viewport[28..32].try_into().unwrap()),
            (3 * LINE_RECORD_BYTES) as u32
        );

        let green = &viewport[VIEWPORT_HEADER_BYTES..VIEWPORT_HEADER_BYTES + GREEN_RECORD_BYTES];
        let projection_start = VIEWPORT_HEADER_BYTES + GREEN_RECORD_BYTES;
        let projection = &viewport[projection_start..projection_start + PROJECTION_RECORD_BYTES];
        assert_eq!(green[12], 8);
        assert_eq!(projection[12], 8);
        assert_eq!(u32::from_le_bytes(green[56..60].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(green[60..64].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(green[64..68].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(green[68..72].try_into().unwrap()), 20);
        assert_eq!(u32::from_le_bytes(green[72..76].try_into().unwrap()), 14);

        let path_start = projection_start + PROJECTION_RECORD_BYTES;
        let ancestor = &viewport[path_start..path_start + POINT_PATH_NODE_BYTES];
        let selected = &viewport[path_start + POINT_PATH_NODE_BYTES..path_start + POINT_PATH_BYTES];
        assert_eq!(ancestor[0], 1);
        assert_eq!(ancestor[1], 0);
        assert_eq!(u16::from_le_bytes(ancestor[2..4].try_into().unwrap()), 0);
        assert_eq!(
            u32::from_le_bytes(ancestor[4..8].try_into().unwrap()),
            u32::MAX
        );
        assert_eq!(u32::from_le_bytes(ancestor[28..32].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(ancestor[32..36].try_into().unwrap()), 20);
        assert_eq!(u32::from_le_bytes(ancestor[36..40].try_into().unwrap()), 14);
        assert_eq!(selected[0], 2);
        assert_eq!(selected[1], 3);
        assert_eq!(u16::from_le_bytes(selected[2..4].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(selected[4..8].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(selected[28..32].try_into().unwrap()), 3);

        let payload = &viewport[path_start + POINT_PATH_BYTES..];
        assert_eq!(payload, encoded_lines);

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn selected_bullet_item_inline_target_publishes_exact_nested_authority() {
        const SOURCE: &str = "- **bold** *em* `code`\r\n\
             - plain\r\n";
        const FACT_RECORD_BYTES: usize = 20;
        const ITEM_RECORD_BYTES: usize = 28;
        const COMPACT_METADATA_BYTES: usize = 8;
        const COMPACT_VIEWPORT_BYTES: usize = 300;
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [726, 727, 728, 729],
            source_session_identity: 730,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start segmented candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(source_version_for(binding, completion))
            .expect("host observes source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let point = SOURCE.find("bold").expect("selected item content") + 2;
        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 1,
                    source_version: delivery.ack.source_version,
                    base_ack: delivery.ack,
                    byte_offset: u32::try_from(point).expect("bounded point"),
                    utf16_offset: u32::try_from(point).expect("ASCII point"),
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::BulletListItemProjection,
                },
            )
            .expect("request selected-item structural authority");
        let (projection_begin, projection_ack) =
            deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
                &mut endpoint,
                &mut runtime,
                &mut host,
                75_000,
            );
        assert_eq!(projection_begin.binding.refinement_generation, 1);
        assert_eq!(projection_begin.binding.physical_start_utf8, 0);
        assert_eq!(
            projection_begin.binding.physical_end_utf8,
            SOURCE.len() as u32
        );
        assert_eq!(projection_begin.binding.visible_start_utf8, 0);
        assert_eq!(projection_begin.binding.visible_end_utf8, 24);
        assert!(matches!(
            projection_begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative {
                fact_count: 1,
                logical_page_count: 1,
                ..
            }
        ));
        assert_eq!(
            projection_ack.disposition,
            InlineSidecarAckDisposition::Authoritative
        );

        let mut compact = [0_u8; ITEM_RECORD_BYTES];
        let compact_query = host
            .query_inline_sidecar(projection_begin.binding, &mut compact)
            .expect("query compact selected-item sidecar");
        assert!(
            matches!(
                &compact_query,
                HostInlineSidecarQueryOutcome::Authoritative {
                    fact_count: 1,
                    encoded_bytes: 28,
                    ..
                }
            ),
            "unexpected compact query: {compact_query:?}"
        );
        assert_eq!(
            compact
                .chunks_exact(ITEM_RECORD_BYTES)
                .map(|record| std::array::from_fn::<u32, 7, _>(|field| {
                    let start = field * 4;
                    u32::from_le_bytes(record[start..start + 4].try_into().unwrap())
                }))
                .collect::<Vec<_>>(),
            vec![[0, 24, 2, 0, 2, 20, 20]]
        );

        let mut viewport = [0xa5_u8; COMPACT_VIEWPORT_BYTES];
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = host
            .query_structural(
                HostPointQuery {
                    source_version: delivery.ack.source_version,
                    position: HostSourceMetric {
                        bytes: point as u32,
                        utf16: point as u32,
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: COMPACT_VIEWPORT_BYTES as u32,
                        maximum_open_depth: 3,
                        maximum_leaf_count: 8,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut viewport,
            )
            .expect("query compact selected-item structural viewport")
        else {
            panic!("compact selected item must join the structural list");
        };
        assert_eq!(receipt.encoded_bytes, COMPACT_VIEWPORT_BYTES as u32);
        assert_eq!(u32::from_le_bytes(viewport[8..12].try_into().unwrap()), 6);
        assert_eq!(viewport[22], 5);
        let payload = COMPACT_VIEWPORT_BYTES - COMPACT_METADATA_BYTES - ITEM_RECORD_BYTES;
        assert_eq!(
            u32::from_le_bytes(viewport[payload..payload + 4].try_into().unwrap()),
            0
        );
        assert_eq!(
            viewport[payload + 4],
            2,
            "selected item preserves canonical CRLF"
        );
        assert_eq!(&viewport[payload + 5..payload + 8], &[0; 3]);
        assert_eq!(&viewport[payload + COMPACT_METADATA_BYTES..], &compact);

        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 2,
                    source_version: delivery.ack.source_version,
                    base_ack: delivery.ack,
                    byte_offset: u32::try_from(point).expect("bounded point"),
                    utf16_offset: u32::try_from(point).expect("ASCII point"),
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::BulletListItemInline,
                },
            )
            .expect("request selected-item inline authority");
        let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            75_000,
        );

        assert_eq!(begin.binding.refinement_generation, 2);
        assert_eq!(begin.binding.physical_start_utf8, 0);
        assert_eq!(begin.binding.physical_end_utf8, SOURCE.len() as u32);
        assert_eq!(begin.binding.visible_start_utf8, 2);
        assert_eq!(begin.binding.visible_end_utf8, 22);
        assert_eq!(begin.binding.physical_start_utf16, 0);
        assert_eq!(
            begin.binding.physical_end_utf16,
            SOURCE.encode_utf16().count() as u32
        );
        assert_eq!(begin.binding.visible_start_utf16, 2);
        assert_eq!(begin.binding.visible_end_utf16, 22);
        assert!(matches!(
            begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative { fact_count: 3, .. }
        ));
        assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);

        let mut facts = [0_u8; 3 * FACT_RECORD_BYTES];
        assert!(matches!(
            host.query_inline_sidecar(begin.binding, &mut facts)
                .expect("query selected-item inline sidecar"),
            HostInlineSidecarQueryOutcome::Authoritative {
                fact_count: 3,
                encoded_bytes: 60,
                ..
            }
        ));
        assert_eq!(
            facts
                .chunks_exact(FACT_RECORD_BYTES)
                .map(|record| (
                    record[0],
                    u32::from_le_bytes(record[4..8].try_into().unwrap())
                ))
                .collect::<Vec<_>>(),
            vec![(2, 0), (1, 9), (3, 14)]
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn ordered_item_targets_preserve_exact_metadata_and_fail_closed_across_lifecycle_edges() {
        const SOURCE: &str =
            "- bullet\r\n- tail\r\n\r\n7) first\r\n00042) **bold** 😀\r\n900) tail\r\n0)   ";
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [736, 737, 738, 739],
            source_session_identity: 740,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start ordered candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(source_version_for(binding, completion))
            .expect("host observes ordered source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let command = |generation: u32, byte_offset: usize, target: InlineRefinementTarget| {
            InlineRefinementCommand {
                binding,
                refinement_generation: generation,
                source_version: delivery.ack.source_version,
                base_ack: delivery.ack,
                byte_offset: u32::try_from(byte_offset).expect("bounded ordered point"),
                utf16_offset: u32::try_from(SOURCE[..byte_offset].encode_utf16().count())
                    .expect("bounded ordered UTF-16 point"),
                affinity: InlinePointAffinity::After,
                target,
            }
        };
        let middle = SOURCE.find("**bold**").expect("ordered middle content") + 2;

        assert!(matches!(
            endpoint.request_hot_inline(
                &mut runtime,
                command(1, middle, InlineRefinementTarget::BulletListItemProjection,),
            ),
            Err(CandidateEndpointError::Derive(
                M11CandidateDerivationError::PublishedBulletListLeafFenceNotBulletList
            ))
        ));
        assert!(endpoint.hot_inline.is_none());

        let bullet = SOURCE.find("bullet").expect("bullet-list item") + 2;
        assert!(matches!(
            endpoint.request_hot_inline(
                &mut runtime,
                command(1, bullet, InlineRefinementTarget::OrderedListItemProjection,),
            ),
            Err(CandidateEndpointError::Derive(
                M11CandidateDerivationError::PublishedOrderedListLeafFenceNotOrderedList
            ))
        ));
        assert!(endpoint.hot_inline.is_none());

        let mut stale = command(1, middle, InlineRefinementTarget::OrderedListItemProjection);
        stale.source_version.revision = stale
            .source_version
            .revision
            .checked_add(1)
            .expect("bounded stale revision");
        assert!(matches!(
            endpoint.request_hot_inline(&mut runtime, stale),
            Err(CandidateEndpointError::InvalidAuthority)
        ));
        assert!(endpoint.hot_inline.is_none());

        endpoint
            .request_hot_inline(
                &mut runtime,
                command(1, middle, InlineRefinementTarget::OrderedListItemProjection),
            )
            .expect("request cancellable ordered projection");
        assert!(matches!(
            endpoint
                .poll(&mut runtime, 1)
                .expect("bounded ordered projection"),
            CandidatePoll::Pending { transitions } if transitions <= 1
        ));
        endpoint.cancel_hot_inline();
        for _ in 0..100_000 {
            if !endpoint.hot_inline_has_poll_work() {
                break;
            }
            assert!(matches!(
                endpoint
                    .poll(&mut runtime, 1)
                    .expect("fuelled ordered cancellation"),
                CandidatePoll::Pending { transitions } if transitions <= 1
            ));
        }
        assert!(!endpoint.hot_inline_has_poll_work());

        endpoint
            .request_hot_inline(
                &mut runtime,
                command(2, middle, InlineRefinementTarget::OrderedListItemProjection),
            )
            .expect("request exact ordered projection");
        for _ in 0..100_000 {
            endpoint
                .poll_hot_inline(&mut runtime, 1)
                .expect("fuelled ordered projection");
            if matches!(endpoint.hot_inline, Some(HotInlineState::Ready(_))) {
                break;
            }
        }
        let Some(HotInlineState::Ready(ready)) = endpoint.hot_inline.as_ref() else {
            panic!("ordered projection did not become ready");
        };
        let HotInlineReadyPublication::Authoritative(root) = &ready.publication else {
            panic!("ordered projection must be authoritative");
        };
        let HotInlineProjectionRoot::OrderedListItem {
            root,
            selected_item_ordinal,
            canonical_line_ending,
            opening_marker_start,
            opening_marker_end,
            marker_value,
        } = root.as_ref()
        else {
            panic!("ordered demand must not become a bullet-list root");
        };
        assert_eq!(
            root.descriptor().projection_kind(),
            M11MarkedLineProjectionKind::OrderedList
        );
        assert_eq!(*selected_item_ordinal, 1);
        assert_eq!(
            *canonical_line_ending,
            M11HotInlineCanonicalLineEnding::CrLf
        );
        assert_eq!((*opening_marker_start, *opening_marker_end), (0, 6));
        assert_eq!(*marker_value, 42);

        let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            75_000,
        );
        assert_eq!(begin.binding.refinement_generation, 2);
        assert!(matches!(
            begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative {
                fact_count: 1,
                logical_page_count: 1,
                ..
            }
        ));
        assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);

        let terminal = SOURCE.rfind("0)").expect("terminal ordered item");
        endpoint
            .request_hot_inline(
                &mut runtime,
                command(
                    3,
                    terminal + 1,
                    InlineRefinementTarget::OrderedListItemProjection,
                ),
            )
            .expect("request terminal ordered projection");
        for _ in 0..100_000 {
            endpoint
                .poll_hot_inline(&mut runtime, 1)
                .expect("fuelled terminal ordered projection");
            if matches!(endpoint.hot_inline, Some(HotInlineState::Ready(_))) {
                break;
            }
        }
        let Some(HotInlineState::Ready(ready)) = endpoint.hot_inline.as_ref() else {
            panic!("terminal ordered projection did not become ready");
        };
        let HotInlineReadyPublication::Authoritative(root) = &ready.publication else {
            panic!("terminal ordered projection must remain authoritative");
        };
        let HotInlineProjectionRoot::OrderedListItem {
            root,
            selected_item_ordinal,
            canonical_line_ending,
            opening_marker_start,
            opening_marker_end,
            marker_value,
        } = root.as_ref()
        else {
            panic!("terminal ordered demand lost its typed root");
        };
        assert_eq!(
            root.descriptor().projection_kind(),
            M11MarkedLineProjectionKind::OrderedList
        );
        assert_eq!(root.descriptor().projected_utf8_length(), 0);
        assert_eq!(root.descriptor().projected_utf16_length(), 0);
        assert_eq!(*selected_item_ordinal, 3);
        assert_eq!(
            *canonical_line_ending,
            M11HotInlineCanonicalLineEnding::CrLf
        );
        assert_eq!((*opening_marker_start, *opening_marker_end), (0, 2));
        assert_eq!(*marker_value, 0);

        let (terminal_begin, terminal_ack) =
            deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
                &mut endpoint,
                &mut runtime,
                &mut host,
                85_000,
            );
        assert_eq!(terminal_begin.binding.refinement_generation, 3);
        assert!(matches!(
            terminal_begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative { fact_count: 1, .. }
        ));
        assert_eq!(
            terminal_ack.disposition,
            InlineSidecarAckDisposition::Authoritative
        );

        endpoint
            .request_hot_inline(
                &mut runtime,
                command(
                    4,
                    terminal + 1,
                    InlineRefinementTarget::OrderedListItemInline,
                ),
            )
            .expect("terminal ordered inline target fails closed as unsupported");
        let (unsupported_begin, unsupported_ack) =
            deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
                &mut endpoint,
                &mut runtime,
                &mut host,
                95_000,
            );
        assert!(matches!(
            unsupported_begin.envelope.disposition,
            HotInlineSidecarDisposition::Unsupported {
                reason: HOT_INLINE_UNSUPPORTED_NOT_INLINE_LEAF,
                ..
            }
        ));
        assert_eq!(
            unsupported_ack.disposition,
            InlineSidecarAckDisposition::Unsupported
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn terminal_empty_bullet_item_projection_preserves_physical_or_predecessor_ending() {
        const ITEM_RECORD_BYTES: usize = 28;
        const COMPACT_METADATA_BYTES: usize = 8;
        const TERMINAL_VIEWPORT_BYTES: usize = 268;
        for (case, source, expected_item_bytes, expected_ending) in [
            (0_u32, "- alpha\n-   \n", 5_u32, 1_u8),
            (1, "- alpha\n-   ", 4, 1),
            (2, "- alpha\r\n-   ", 4, 2),
        ] {
            let profile = SourceFactsScanProfile::new(8).expect("test profile");
            let parser_profile = ParserProfileId::new(1).expect("parser profile");
            let binding = SessionBinding {
                document_session: [746, 747, 748, 749 + case],
                source_session_identity: 750 + case,
                worker_generation: 1,
            };
            let mut runtime =
                DocumentRuntime::new(source, standard_document_runtime_config()).expect("runtime");
            let (certified, completion) =
                complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
            let mut endpoint = CandidateEndpoint::new();
            endpoint
                .start(certified, binding, completion)
                .expect("start segmented candidate");
            let mut host = NativeCandidateHost::new(HostConfig {
                document_session: binding.document_session,
                grammar_revision: GRAMMAR_REVISION,
                syntax_profile: 1,
                authority_mask: AUTHORITY_MASK_ALL_ROLES,
                maximum_query_bytes: 64 * 1024,
            })
            .expect("independent host");
            host.observe_source_version(source_version_for(binding, completion))
                .expect("host observes source");
            let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
                &mut endpoint,
                &mut runtime,
                &mut host,
            );
            drain_candidate_cleanup(&mut endpoint, &mut runtime);

            let item_start = source.rfind("-   ").expect("terminal marker-only item");
            endpoint
                .request_hot_inline(
                    &mut runtime,
                    InlineRefinementCommand {
                        binding,
                        refinement_generation: 1,
                        source_version: delivery.ack.source_version,
                        base_ack: delivery.ack,
                        byte_offset: (item_start + 1) as u32,
                        utf16_offset: (item_start + 1) as u32,
                        affinity: InlinePointAffinity::After,
                        target: InlineRefinementTarget::BulletListItemProjection,
                    },
                )
                .expect("request terminal selected-item projection");
            let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
                &mut endpoint,
                &mut runtime,
                &mut host,
                77_000 + case * 1_000,
            );
            assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);
            assert_eq!(begin.binding.visible_start_utf8, item_start as u32);
            assert_eq!(begin.binding.visible_end_utf8, source.len() as u32);

            let mut compact = [0_u8; ITEM_RECORD_BYTES];
            let compact_query = host
                .query_inline_sidecar(begin.binding, &mut compact)
                .expect("query terminal compact sidecar");
            assert!(
                matches!(
                    &compact_query,
                    HostInlineSidecarQueryOutcome::Authoritative {
                        fact_count: 1,
                        encoded_bytes,
                        ..
                    } if *encoded_bytes == ITEM_RECORD_BYTES as u32
                ),
                "unexpected terminal compact query: {compact_query:?}"
            );
            assert_eq!(
                u32::from_le_bytes(compact[4..8].try_into().unwrap()),
                expected_item_bytes
            );
            assert_eq!(u32::from_le_bytes(compact[20..24].try_into().unwrap()), 0);
            assert_eq!(u32::from_le_bytes(compact[24..28].try_into().unwrap()), 0);

            let mut viewport = [0xa5_u8; TERMINAL_VIEWPORT_BYTES];
            let HostStructuralQueryOutcome::Viewport { receipt, .. } = host
                .query_structural(
                    HostPointQuery {
                        source_version: delivery.ack.source_version,
                        position: HostSourceMetric {
                            bytes: source.len() as u32,
                            utf16: source.len() as u32,
                        },
                        affinity: HostMetricAffinity::Downstream,
                        budget: HostQueryBudget {
                            maximum_encoded_bytes: TERMINAL_VIEWPORT_BYTES as u32,
                            maximum_open_depth: 3,
                            maximum_leaf_count: 8,
                            maximum_tree_nodes_visited: 256,
                        },
                    },
                    &mut viewport,
                )
                .expect("query terminal compact viewport")
            else {
                panic!("terminal selected item must join the structural list");
            };
            assert_eq!(receipt.encoded_bytes, TERMINAL_VIEWPORT_BYTES as u32);
            assert_eq!(u32::from_le_bytes(viewport[8..12].try_into().unwrap()), 6);
            assert_eq!(u16::from_le_bytes(viewport[20..22].try_into().unwrap()), 2);
            assert_eq!(viewport[22], 5);
            let payload = TERMINAL_VIEWPORT_BYTES - COMPACT_METADATA_BYTES - ITEM_RECORD_BYTES;
            assert_eq!(
                u32::from_le_bytes(viewport[payload..payload + 4].try_into().unwrap()),
                1
            );
            assert_eq!(
                viewport[payload + 4],
                expected_ending,
                "EOF must inherit the immediate predecessor's authenticated ending"
            );
            assert_eq!(&viewport[payload + 5..payload + 8], &[0; 3]);
            assert_eq!(&viewport[payload + COMPACT_METADATA_BYTES..], &compact);

            close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
        }
    }

    #[test]
    fn bullet_list_request_reaches_typed_sidecar_and_selected_item_path() {
        const SOURCE: &str = "\u{feff}  -  α😀\r\n  - β\r-   ";
        const ITEM_RECORD_BYTES: usize = 28;
        const VIEWPORT_HEADER_BYTES: usize = 32;
        const GREEN_RECORD_BYTES: usize = 80;
        const PROJECTION_RECORD_BYTES: usize = 56;
        const POINT_PATH_NODE_BYTES: usize = 32;
        const POINT_PATH_BYTES: usize = 3 * POINT_PATH_NODE_BYTES;
        const VIEWPORT_BYTES: usize = VIEWPORT_HEADER_BYTES
            + GREEN_RECORD_BYTES
            + PROJECTION_RECORD_BYTES
            + POINT_PATH_BYTES
            + 3 * ITEM_RECORD_BYTES;
        const TERMINAL_POINT_PATH_BYTES: usize = 2 * POINT_PATH_NODE_BYTES;
        const TERMINAL_VIEWPORT_BYTES: usize = VIEWPORT_HEADER_BYTES
            + GREEN_RECORD_BYTES
            + PROJECTION_RECORD_BYTES
            + TERMINAL_POINT_PATH_BYTES
            + 3 * ITEM_RECORD_BYTES;
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [731, 732, 733, 734],
            source_session_identity: 735,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start segmented candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(source_version_for(binding, completion))
            .expect("host observes source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let command = |generation: u32| InlineRefinementCommand {
            binding,
            refinement_generation: generation,
            source_version: delivery.ack.source_version,
            base_ack: delivery.ack,
            byte_offset: 20,
            utf16_offset: 15,
            affinity: InlinePointAffinity::After,
            target: InlineRefinementTarget::Automatic,
        };
        endpoint
            .request_hot_inline(&mut runtime, command(1))
            .expect("first bullet-list demand");
        assert!(matches!(
            endpoint
                .poll(&mut runtime, 1)
                .expect("bounded bullet-list projection"),
            CandidatePoll::Pending { transitions } if transitions <= 1
        ));
        assert!(endpoint.hot_inline_sidecar.is_none());
        endpoint.cancel_hot_inline();
        for _ in 0..100_000 {
            if !endpoint.hot_inline_has_poll_work() {
                break;
            }
            assert!(matches!(
                endpoint
                    .poll(&mut runtime, 1)
                    .expect("fuelled bullet-list cancellation"),
                CandidatePoll::Pending { transitions } if transitions <= 1
            ));
        }
        assert!(!endpoint.hot_inline_has_poll_work());

        endpoint
            .request_hot_inline(&mut runtime, command(2))
            .expect("replacement bullet-list demand");
        let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            80_000,
        );
        assert_eq!(begin.binding.refinement_generation, 2);
        assert_eq!(begin.binding.physical_start_utf8, 0);
        assert_eq!(begin.binding.physical_end_utf8, SOURCE.len() as u32);
        assert!(matches!(
            begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative {
                fact_count: 3,
                logical_page_count: 1,
                ..
            }
        ));
        assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);

        let mut encoded_items = [0_u8; 3 * ITEM_RECORD_BYTES];
        let query = host
            .query_inline_sidecar(begin.binding, &mut encoded_items)
            .expect("query typed bullet-list sidecar");
        assert!(matches!(
            query,
            HostInlineSidecarQueryOutcome::Authoritative {
                fact_count: 3,
                encoded_bytes: 84,
                ..
            }
        ));
        let observed = encoded_items
            .chunks_exact(ITEM_RECORD_BYTES)
            .map(|record| {
                std::array::from_fn::<u32, 7, _>(|field| {
                    let start = field * 4;
                    u32::from_le_bytes(record[start..start + 4].try_into().unwrap())
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            observed,
            vec![
                [0, 16, 8, 3, 8, 6, 3],
                [16, 7, 4, 0, 4, 2, 1],
                [23, 4, 4, 0, 2, 0, 0],
            ]
        );

        let mut viewport = [0xa5_u8; VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: delivery.ack.source_version,
                    position: HostSourceMetric {
                        bytes: 20,
                        utf16: 15,
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: VIEWPORT_BYTES as u32,
                        maximum_open_depth: 3,
                        maximum_leaf_count: 8,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut viewport,
            )
            .expect("joined bullet-list viewport");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("list and sidecar must author one schema-5 viewport: {outcome:?}");
        };
        assert_eq!(receipt.encoded_bytes, VIEWPORT_BYTES as u32);
        assert_eq!(u32::from_le_bytes(viewport[8..12].try_into().unwrap()), 5);
        assert_eq!(u16::from_le_bytes(viewport[20..22].try_into().unwrap()), 3);
        assert_eq!(viewport[22], 4);
        assert_eq!(
            u32::from_le_bytes(viewport[24..28].try_into().unwrap()),
            POINT_PATH_BYTES as u32
        );
        assert_eq!(
            u32::from_le_bytes(viewport[28..32].try_into().unwrap()),
            (3 * ITEM_RECORD_BYTES) as u32
        );
        let path_start = VIEWPORT_HEADER_BYTES + GREEN_RECORD_BYTES + PROJECTION_RECORD_BYTES;
        let list = &viewport[path_start..path_start + POINT_PATH_NODE_BYTES];
        let item =
            &viewport[path_start + POINT_PATH_NODE_BYTES..path_start + 2 * POINT_PATH_NODE_BYTES];
        let paragraph =
            &viewport[path_start + 2 * POINT_PATH_NODE_BYTES..path_start + POINT_PATH_BYTES];
        assert_eq!((list[0], list[1]), (3, 1));
        assert_eq!((item[0], item[1]), (4, 1));
        assert_eq!((paragraph[0], paragraph[1]), (2, 2));
        assert_eq!(u32::from_le_bytes(item[16..20].try_into().unwrap()), 1);
        assert_eq!(&viewport[path_start + POINT_PATH_BYTES..], &encoded_items);

        let mut terminal_viewport = [0xa5_u8; TERMINAL_VIEWPORT_BYTES];
        let terminal_outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: delivery.ack.source_version,
                    position: HostSourceMetric {
                        bytes: 23,
                        utf16: 17,
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: TERMINAL_VIEWPORT_BYTES as u32,
                        maximum_open_depth: 3,
                        maximum_leaf_count: 8,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut terminal_viewport,
            )
            .expect("joined terminal-empty bullet-list viewport");
        let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = terminal_outcome else {
            panic!(
                "terminal list item must author a two-node schema-5 viewport: {terminal_outcome:?}"
            );
        };
        assert_eq!(range.start, HostSourceMetric { bytes: 0, utf16: 0 });
        assert_eq!(
            range.end,
            HostSourceMetric {
                bytes: SOURCE.len() as u32,
                utf16: 21,
            }
        );
        assert_eq!(receipt.encoded_bytes, TERMINAL_VIEWPORT_BYTES as u32);
        assert!(receipt.leaf_count <= 8);
        assert!(receipt.open_depth <= 3);
        assert!(receipt.tree_nodes_visited <= 256);
        assert_eq!(
            u32::from_le_bytes(terminal_viewport[8..12].try_into().unwrap()),
            5
        );
        assert_eq!(
            u16::from_le_bytes(terminal_viewport[20..22].try_into().unwrap()),
            2
        );
        assert_eq!(terminal_viewport[22], 4);
        assert_eq!(
            u32::from_le_bytes(terminal_viewport[24..28].try_into().unwrap()),
            TERMINAL_POINT_PATH_BYTES as u32
        );
        assert_eq!(
            u32::from_le_bytes(terminal_viewport[28..32].try_into().unwrap()),
            (3 * ITEM_RECORD_BYTES) as u32
        );
        let terminal_list = &terminal_viewport[path_start..path_start + POINT_PATH_NODE_BYTES];
        let terminal_item = &terminal_viewport
            [path_start + POINT_PATH_NODE_BYTES..path_start + TERMINAL_POINT_PATH_BYTES];
        assert_eq!((terminal_list[0], terminal_list[1]), (3, 1));
        assert_eq!(
            u16::from_le_bytes(terminal_list[2..4].try_into().unwrap()),
            0
        );
        assert_eq!(
            u32::from_le_bytes(terminal_list[4..8].try_into().unwrap()),
            u32::MAX
        );
        assert_eq!(
            u32::from_le_bytes(terminal_list[8..12].try_into().unwrap()),
            0
        );
        assert_eq!(
            u32::from_le_bytes(terminal_list[12..16].try_into().unwrap()),
            SOURCE.len() as u32
        );
        assert_eq!(
            u32::from_le_bytes(terminal_list[16..20].try_into().unwrap()),
            0
        );
        assert_eq!(
            u32::from_le_bytes(terminal_list[20..24].try_into().unwrap()),
            3
        );
        assert_eq!(
            u32::from_le_bytes(terminal_list[24..28].try_into().unwrap()),
            11
        );
        assert_eq!(
            u32::from_le_bytes(terminal_list[28..32].try_into().unwrap()),
            7
        );
        assert_eq!((terminal_item[0], terminal_item[1]), (4, 3));
        assert_eq!(
            u16::from_le_bytes(terminal_item[2..4].try_into().unwrap()),
            1
        );
        assert_eq!(
            u32::from_le_bytes(terminal_item[4..8].try_into().unwrap()),
            0
        );
        assert_eq!(
            u32::from_le_bytes(terminal_item[8..12].try_into().unwrap()),
            23
        );
        assert_eq!(
            u32::from_le_bytes(terminal_item[12..16].try_into().unwrap()),
            27
        );
        assert_eq!(
            u32::from_le_bytes(terminal_item[16..20].try_into().unwrap()),
            2
        );
        assert_eq!(
            u32::from_le_bytes(terminal_item[20..24].try_into().unwrap()),
            1
        );
        assert_eq!(
            u32::from_le_bytes(terminal_item[24..28].try_into().unwrap()),
            0
        );
        assert_eq!(
            u32::from_le_bytes(terminal_item[28..32].try_into().unwrap()),
            0
        );
        assert_eq!(
            &terminal_viewport[path_start + TERMINAL_POINT_PATH_BYTES..],
            &encoded_items
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn bullet_list_nonzero_leaf_root_joins_absolute_selected_item_path() {
        const SOURCE: &str = "before😀\n\n- α😀\n- beta";
        const LIST_START_BYTES: u32 = 12;
        const LIST_START_UTF16: u32 = 10;
        const ITEM_RECORD_BYTES: usize = 28;
        const VIEWPORT_HEADER_BYTES: usize = 32;
        const GREEN_RECORD_BYTES: usize = 80;
        const PROJECTION_RECORD_BYTES: usize = 56;
        const POINT_PATH_NODE_BYTES: usize = 32;
        const POINT_PATH_BYTES: usize = 3 * POINT_PATH_NODE_BYTES;
        const VIEWPORT_BYTES: usize = VIEWPORT_HEADER_BYTES
            + GREEN_RECORD_BYTES
            + PROJECTION_RECORD_BYTES
            + POINT_PATH_BYTES
            + 2 * ITEM_RECORD_BYTES;
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [736, 737, 738, 739],
            source_session_identity: 740,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start segmented candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(source_version_for(binding, completion))
            .expect("host observes source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 1,
                    source_version: delivery.ack.source_version,
                    base_ack: delivery.ack,
                    byte_offset: 23,
                    utf16_offset: 18,
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::Automatic,
                },
            )
            .expect("nonzero-root bullet-list demand");
        let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            80_000,
        );
        assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);
        assert_eq!(begin.binding.physical_start_utf8, LIST_START_BYTES);
        assert_eq!(begin.binding.physical_start_utf16, LIST_START_UTF16);
        assert_eq!(begin.binding.physical_end_utf8, SOURCE.len() as u32);
        assert_eq!(
            begin.binding.physical_end_utf16,
            SOURCE.encode_utf16().count() as u32
        );
        assert!(matches!(
            begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative {
                fact_count: 2,
                logical_page_count: 1,
                ..
            }
        ));

        let mut encoded_items = [0_u8; 2 * ITEM_RECORD_BYTES];
        let query = host
            .query_inline_sidecar(begin.binding, &mut encoded_items)
            .expect("query typed nonzero-root bullet-list sidecar");
        assert!(matches!(
            query,
            HostInlineSidecarQueryOutcome::Authoritative {
                fact_count: 2,
                encoded_bytes: 56,
                ..
            }
        ));
        let observed = encoded_items
            .chunks_exact(ITEM_RECORD_BYTES)
            .map(|record| {
                std::array::from_fn::<u32, 7, _>(|field| {
                    let start = field * 4;
                    u32::from_le_bytes(record[start..start + 4].try_into().unwrap())
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(observed, vec![[0, 9, 2, 0, 2, 6, 3], [9, 6, 2, 0, 2, 4, 4]]);

        let mut viewport = [0xa5_u8; VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: delivery.ack.source_version,
                    position: HostSourceMetric {
                        bytes: 23,
                        utf16: 18,
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: VIEWPORT_BYTES as u32,
                        maximum_open_depth: 3,
                        maximum_leaf_count: 8,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut viewport,
            )
            .expect("joined nonzero-root bullet-list viewport");
        let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
            panic!("nonzero-root list and sidecar must author schema 5: {outcome:?}");
        };
        assert_eq!(
            range.start,
            HostSourceMetric {
                bytes: LIST_START_BYTES,
                utf16: LIST_START_UTF16,
            }
        );
        assert_eq!(
            range.end,
            HostSourceMetric {
                bytes: SOURCE.len() as u32,
                utf16: SOURCE.encode_utf16().count() as u32,
            }
        );
        assert_eq!(receipt.encoded_bytes, VIEWPORT_BYTES as u32);
        assert_eq!(u32::from_le_bytes(viewport[8..12].try_into().unwrap()), 5);
        assert_eq!(u16::from_le_bytes(viewport[20..22].try_into().unwrap()), 3);
        assert_eq!(viewport[22], 4);

        let path_start = VIEWPORT_HEADER_BYTES + GREEN_RECORD_BYTES + PROJECTION_RECORD_BYTES;
        let list = &viewport[path_start..path_start + POINT_PATH_NODE_BYTES];
        let item =
            &viewport[path_start + POINT_PATH_NODE_BYTES..path_start + 2 * POINT_PATH_NODE_BYTES];
        let paragraph =
            &viewport[path_start + 2 * POINT_PATH_NODE_BYTES..path_start + POINT_PATH_BYTES];
        assert_eq!((list[0], list[1]), (3, 1));
        assert_eq!(
            (
                u32::from_le_bytes(list[8..12].try_into().unwrap()),
                u32::from_le_bytes(list[12..16].try_into().unwrap())
            ),
            (12, 27)
        );
        assert_eq!((item[0], item[1]), (4, 1));
        assert_eq!(
            (
                u32::from_le_bytes(item[8..12].try_into().unwrap()),
                u32::from_le_bytes(item[12..16].try_into().unwrap())
            ),
            (21, 27)
        );
        assert_eq!(u32::from_le_bytes(item[16..20].try_into().unwrap()), 1);
        assert_eq!((paragraph[0], paragraph[1]), (2, 2));
        assert_eq!(
            (
                u32::from_le_bytes(paragraph[8..12].try_into().unwrap()),
                u32::from_le_bytes(paragraph[12..16].try_into().unwrap())
            ),
            (23, 27)
        );
        assert_eq!(&viewport[path_start + POINT_PATH_BYTES..], &encoded_items);

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn close_after_sidecar_commit_before_delivery_receipt_reclaims_typed_roots() {
        let cases = [
            (
                "\u{feff}   > α😀\r\n> β\rlazy😀\0",
                [721, 722, 723, 724],
                725,
                "block quote",
            ),
            (
                "\u{feff}\tα\0\r\n\n      \r    \tβ\r\tlast",
                [726, 727, 728, 729],
                730,
                "indented code",
            ),
        ];

        for (source, document_session, source_session_identity, label) in cases {
            let profile = SourceFactsScanProfile::new(8).expect("test profile");
            let parser_profile = ParserProfileId::new(1).expect("parser profile");
            let binding = SessionBinding {
                document_session,
                source_session_identity,
                worker_generation: 1,
            };
            let mut runtime =
                DocumentRuntime::new(source, standard_document_runtime_config()).expect("runtime");
            let (certified, completion) =
                complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
            let mut endpoint = CandidateEndpoint::new();
            endpoint
                .start(certified, binding, completion)
                .expect("start segmented candidate");
            let mut host = NativeCandidateHost::new(HostConfig {
                document_session: binding.document_session,
                grammar_revision: GRAMMAR_REVISION,
                syntax_profile: 1,
                authority_mask: AUTHORITY_MASK_ALL_ROLES,
                maximum_query_bytes: 64 * 1024,
            })
            .expect("independent host");
            host.observe_source_version(source_version_for(binding, completion))
                .expect("host observes source");
            let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
                &mut endpoint,
                &mut runtime,
                &mut host,
            );
            drain_candidate_cleanup(&mut endpoint, &mut runtime);

            endpoint
                .request_hot_inline(
                    &mut runtime,
                    InlineRefinementCommand {
                        binding,
                        refinement_generation: 1,
                        source_version: delivery.ack.source_version,
                        base_ack: delivery.ack,
                        byte_offset: 0,
                        utf16_offset: 0,
                        affinity: InlinePointAffinity::After,
                        target: InlineRefinementTarget::Automatic,
                    },
                )
                .unwrap_or_else(|error| panic!("{label} demand failed: {error}"));
            let pending = commit_hot_inline_sidecar_to_independent_host_with_unit_fuel(
                &mut endpoint,
                &mut runtime,
                &mut host,
                90_000,
            );
            assert!(
                endpoint.hot_inline_sidecar.is_some(),
                "{label} delivery receipt must still own the producer sidecar"
            );
            host.acknowledge_inline_sidecar_delivery(pending.ack)
                .unwrap_or_else(|error| panic!("{label} host delivery failed: {error}"));

            // Match Endpoint::begin_close: cancel the producer first, then
            // latch the document runtime close before any delivery receipt is
            // returned to CandidateEndpoint.
            endpoint
                .begin_close()
                .unwrap_or_else(|error| panic!("{label} close failed: {error}"));
            runtime.cancel_source_facts();
            runtime
                .begin_close()
                .unwrap_or_else(|error| panic!("{label} runtime close failed: {error}"));

            for _ in 0..1_000_000 {
                if !endpoint.cleanup_pending() {
                    break;
                }
                endpoint
                    .poll_cleanup(&mut runtime, 1)
                    .unwrap_or_else(|error| panic!("{label} cleanup failed: {error}"));
            }
            assert!(
                !endpoint.cleanup_pending(),
                "{label} root remained live after close"
            );
            assert!(endpoint.hot_inline_sidecar.is_none());

            for _ in 0..1_000_000 {
                if runtime
                    .poll_close(1)
                    .unwrap_or_else(|error| panic!("{label} runtime drain failed: {error}"))
                    .complete
                {
                    break;
                }
            }
            assert_eq!(
                runtime.arena_metrics().resident_nodes,
                0,
                "{label} runtime did not reclaim to zero"
            );
            host.begin_close()
                .unwrap_or_else(|error| panic!("{label} host close failed: {error}"));
            for _ in 0..1_000_000 {
                match host
                    .poll(HostWorkGrant {
                        inspect_bytes: 0,
                        copy_bytes: 0,
                        transitions: 1,
                    })
                    .unwrap_or_else(|error| panic!("{label} host drain failed: {error}"))
                {
                    NativeHostPollOutcome::Pending => {}
                    NativeHostPollOutcome::Closed => break,
                    outcome => panic!("{label} unexpected host close outcome: {outcome:?}"),
                }
            }
            assert!(host.is_removable(), "{label} host did not close to zero");
        }
    }

    #[test]
    fn atx_heading_reaches_independent_host_and_refines_only_its_content() {
        const SOURCE: &str = "p\n\n  ### **β😀** ###  \r\n\n# before <tag>\n";
        const VIEWPORT_HEADER_BYTES: usize = 20;
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [711, 712, 713, 714],
            source_session_identity: 715,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start segmented candidate");
        let source_version = source_version_for(binding, completion);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(source_version)
            .expect("host observes source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let heading_start = SOURCE.find("  ###").expect("ATX Heading");
        let heading_end = heading_start + "  ### **β😀** ###  \r\n".len();
        let inline_start = SOURCE.find("**β😀**").expect("heading content");
        let inline_end = inline_start + "**β😀**".len();
        let mut output = [0_u8; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version,
                    position: HostSourceMetric {
                        bytes: u32::try_from(inline_start + 2).unwrap(),
                        utf16: u32::try_from(SOURCE[..inline_start + 2].encode_utf16().count())
                            .unwrap(),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                        maximum_open_depth: 64,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut output,
            )
            .expect("independent ATX Heading query");
        let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
            panic!("ATX Heading must author an independent viewport: {outcome:?}");
        };
        assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
        assert_eq!(range.start.bytes as usize, heading_start);
        assert_eq!(
            range.start.utf16 as usize,
            SOURCE[..heading_start].encode_utf16().count()
        );
        assert_eq!(range.end.bytes as usize, heading_end);
        assert_eq!(
            range.end.utf16 as usize,
            SOURCE[..heading_end].encode_utf16().count()
        );
        let green = &output[VIEWPORT_HEADER_BYTES..VIEWPORT_HEADER_BYTES + 80];
        let projection = &output[VIEWPORT_HEADER_BYTES + 80..];
        assert_eq!(green[12], 4);
        assert_eq!(projection[12], 4);
        assert_eq!(
            u64::from_le_bytes(green[16..24].try_into().unwrap()),
            heading_start as u64
        );
        assert_eq!(
            u64::from_le_bytes(green[24..32].try_into().unwrap()),
            heading_end as u64
        );
        assert_eq!(
            u64::from_le_bytes(green[32..40].try_into().unwrap()),
            inline_start as u64
        );
        assert_eq!(
            u64::from_le_bytes(green[40..48].try_into().unwrap()),
            inline_end as u64
        );
        assert_eq!(u64::from_le_bytes(green[48..56].try_into().unwrap()), 0x503);
        assert_eq!(u32::from_le_bytes(green[56..60].try_into().unwrap()), 5);
        assert_eq!(u32::from_le_bytes(green[60..64].try_into().unwrap()), 8);
        assert_eq!(
            u64::from_le_bytes(projection[32..40].try_into().unwrap()),
            inline_start as u64
        );
        assert_eq!(
            u64::from_le_bytes(projection[40..48].try_into().unwrap()),
            inline_end as u64
        );

        let command = |generation: u32, byte_offset: usize| InlineRefinementCommand {
            binding,
            refinement_generation: generation,
            source_version: delivery.ack.source_version,
            base_ack: delivery.ack,
            byte_offset: u32::try_from(byte_offset).expect("bounded point"),
            utf16_offset: u32::try_from(SOURCE[..byte_offset].encode_utf16().count())
                .expect("bounded UTF-16 point"),
            affinity: InlinePointAffinity::After,
            target: InlineRefinementTarget::Automatic,
        };
        endpoint
            .request_hot_inline(&mut runtime, command(1, inline_start + 2))
            .expect("ATX inline demand");
        let (authoritative_begin, authoritative_ack) =
            deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 40_000);
        assert_eq!(authoritative_begin.base_ack, delivery.ack);
        assert_eq!(
            authoritative_begin.binding.physical_start_utf8 as usize,
            heading_start
        );
        assert_eq!(
            authoritative_begin.binding.physical_end_utf8 as usize,
            heading_end
        );
        assert_eq!(
            authoritative_begin.binding.visible_start_utf8 as usize,
            inline_start
        );
        assert_eq!(
            authoritative_begin.binding.visible_end_utf8 as usize,
            inline_end
        );
        assert!(matches!(
            authoritative_begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative { fact_count, .. } if fact_count > 0
        ));
        assert_eq!(
            authoritative_ack.disposition,
            InlineSidecarAckDisposition::Authoritative
        );

        let hazard_start = SOURCE.find("before").expect("hazard heading content");
        let hazard_end = hazard_start + "before <tag>".len();
        endpoint
            .request_hot_inline(&mut runtime, command(2, hazard_start))
            .expect("hazard ATX inline demand");
        let (unsupported_begin, unsupported_ack) =
            deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 50_000);
        assert_eq!(
            unsupported_begin.binding.visible_start_utf8 as usize,
            hazard_start
        );
        assert_eq!(
            unsupported_begin.binding.visible_end_utf8 as usize,
            hazard_end
        );
        assert!(matches!(
            unsupported_begin.envelope.disposition,
            HotInlineSidecarDisposition::Unsupported {
                reason: HOT_INLINE_UNSUPPORTED_PARSER,
                ..
            }
        ));
        assert_eq!(
            unsupported_ack.disposition,
            InlineSidecarAckDisposition::Unsupported
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn setext_h1_h2_reach_independent_host_with_content_only_inline_fences() {
        const SOURCE: &str = "**H1 β😀**\r\n  ===  \r\n\n_H2_\n---\n";
        const VIEWPORT_HEADER_BYTES: usize = 20;
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [726, 727, 728, 729],
            source_session_identity: 730,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start segmented Setext candidate");
        let source_version = source_version_for(binding, completion);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent Setext host");
        host.observe_source_version(source_version)
            .expect("host observes Setext source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let h1_start = 0_usize;
        let h1_inline_end = SOURCE.find("\r\n  ===").expect("H1 content ending");
        let h1_marker_start = SOURCE.find("===").expect("H1 underline");
        let h1_marker_end = h1_marker_start + 3;
        let h1_line_ending_start = h1_marker_end + 2;
        let h1_end = h1_line_ending_start + 2;
        let h2_start = SOURCE.find("_H2_").expect("H2 content");
        let h2_inline_end = h2_start + "_H2_".len();
        let h2_marker_start = SOURCE[h2_start..]
            .find("---")
            .map(|offset| h2_start + offset)
            .expect("H2 underline");
        let h2_marker_end = h2_marker_start + 3;
        let h2_line_ending_start = h2_marker_end;
        let h2_end = h2_line_ending_start + 1;
        let headings = [
            (
                h1_start,
                h1_end,
                h1_start,
                h1_inline_end,
                h1_marker_start,
                h1_marker_end,
                h1_line_ending_start,
                h1_end,
                0x201_u64,
            ),
            (
                h2_start,
                h2_end,
                h2_start,
                h2_inline_end,
                h2_marker_start,
                h2_marker_end,
                h2_line_ending_start,
                h2_end,
                2_u64,
            ),
        ];
        for (
            source_start,
            source_end,
            inline_start,
            inline_end,
            marker_start,
            marker_end,
            line_ending_start,
            line_ending_end,
            metadata,
        ) in headings
        {
            let mut output = [0_u8; HOST_M11_VIEWPORT_BYTES];
            let point = inline_start + 1;
            let outcome = host
                .query_structural(
                    HostPointQuery {
                        source_version,
                        position: HostSourceMetric {
                            bytes: u32::try_from(point).expect("Setext point byte"),
                            utf16: u32::try_from(SOURCE[..point].encode_utf16().count())
                                .expect("Setext point UTF-16"),
                        },
                        affinity: HostMetricAffinity::Downstream,
                        budget: HostQueryBudget {
                            maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                            maximum_open_depth: 64,
                            maximum_leaf_count: 64,
                            maximum_tree_nodes_visited: 256,
                        },
                    },
                    &mut output,
                )
                .expect("independent Setext query");
            let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
                panic!("Setext Heading must author an independent viewport: {outcome:?}");
            };
            assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
            assert_eq!(range.start.bytes as usize, source_start);
            assert_eq!(
                range.start.utf16 as usize,
                SOURCE[..source_start].encode_utf16().count()
            );
            assert_eq!(range.end.bytes as usize, source_end);
            assert_eq!(
                range.end.utf16 as usize,
                SOURCE[..source_end].encode_utf16().count()
            );
            let green = &output[VIEWPORT_HEADER_BYTES..VIEWPORT_HEADER_BYTES + 80];
            let projection = &output[VIEWPORT_HEADER_BYTES + 80..];
            assert_eq!(green[12], 5);
            assert_eq!(projection[12], 5);
            assert_eq!(
                u64::from_le_bytes(green[16..24].try_into().unwrap()),
                source_start as u64
            );
            assert_eq!(
                u64::from_le_bytes(green[24..32].try_into().unwrap()),
                source_end as u64
            );
            assert_eq!(
                u64::from_le_bytes(green[32..40].try_into().unwrap()),
                inline_start as u64
            );
            assert_eq!(
                u64::from_le_bytes(green[40..48].try_into().unwrap()),
                inline_end as u64
            );
            assert_eq!(
                u64::from_le_bytes(green[48..56].try_into().unwrap()),
                metadata
            );
            assert_eq!(
                u32::from_le_bytes(green[56..60].try_into().unwrap()) as usize,
                marker_start
            );
            assert_eq!(
                u32::from_le_bytes(green[60..64].try_into().unwrap()) as usize,
                marker_end
            );
            assert_eq!(
                u32::from_le_bytes(green[64..68].try_into().unwrap()) as usize,
                line_ending_start
            );
            assert_eq!(
                u32::from_le_bytes(green[68..72].try_into().unwrap()) as usize,
                line_ending_end
            );
            assert_eq!(u64::from_le_bytes(green[72..80].try_into().unwrap()), 0);
            assert_eq!(
                u64::from_le_bytes(projection[32..40].try_into().unwrap()),
                inline_start as u64
            );
            assert_eq!(
                u64::from_le_bytes(projection[40..48].try_into().unwrap()),
                inline_end as u64
            );
        }

        let command = |generation: u32, byte_offset: usize| InlineRefinementCommand {
            binding,
            refinement_generation: generation,
            source_version: delivery.ack.source_version,
            base_ack: delivery.ack,
            byte_offset: u32::try_from(byte_offset).expect("bounded Setext point"),
            utf16_offset: u32::try_from(SOURCE[..byte_offset].encode_utf16().count())
                .expect("bounded Setext UTF-16 point"),
            affinity: InlinePointAffinity::After,
            target: InlineRefinementTarget::Automatic,
        };
        for (generation, physical_start, physical_end, visible_start, visible_end) in [
            (1, h1_start, h1_end, h1_start, h1_inline_end),
            (2, h2_start, h2_end, h2_start, h2_inline_end),
        ] {
            endpoint
                .request_hot_inline(&mut runtime, command(generation, visible_start + 1))
                .expect("Setext inline demand");
            let (begin, ack) =
                deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 40_000);
            assert_eq!(begin.base_ack, delivery.ack);
            assert_eq!(begin.binding.physical_start_utf8 as usize, physical_start);
            assert_eq!(begin.binding.physical_end_utf8 as usize, physical_end);
            assert_eq!(begin.binding.visible_start_utf8 as usize, visible_start);
            assert_eq!(begin.binding.visible_end_utf8 as usize, visible_end);
            assert_eq!(
                begin.binding.physical_start_utf16 as usize,
                SOURCE[..physical_start].encode_utf16().count()
            );
            assert_eq!(
                begin.binding.physical_end_utf16 as usize,
                SOURCE[..physical_end].encode_utf16().count()
            );
            assert_eq!(
                begin.binding.visible_start_utf16 as usize,
                SOURCE[..visible_start].encode_utf16().count()
            );
            assert_eq!(
                begin.binding.visible_end_utf16 as usize,
                SOURCE[..visible_end].encode_utf16().count()
            );
            assert!(matches!(
                begin.envelope.disposition,
                HotInlineSidecarDisposition::Authoritative { fact_count, .. } if fact_count > 0
            ));
            assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);
        }

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn thematic_break_reaches_independent_host_with_empty_projection_and_not_inline_sidecar() {
        const SOURCE: &str = "p\n\n  - - -  \r\n\nq";
        const VIEWPORT_HEADER_BYTES: usize = 20;
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [731, 732, 733, 734],
            source_session_identity: 735,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start segmented thematic-break candidate");
        let source_version = source_version_for(binding, completion);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent thematic-break host");
        host.observe_source_version(source_version)
            .expect("host observes thematic-break source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let thematic_start = SOURCE.find("  - - -").expect("thematic break");
        let marker_start = SOURCE[thematic_start..]
            .find('-')
            .map(|offset| thematic_start + offset)
            .expect("first thematic marker");
        let marker_end = thematic_start + "  - - -".len();
        let line_ending_start = SOURCE[marker_end..]
            .find("\r\n")
            .map(|offset| marker_end + offset)
            .expect("thematic line ending");
        let thematic_end = line_ending_start + 2;
        let mut output = [0_u8; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version,
                    position: HostSourceMetric {
                        bytes: u32::try_from(marker_start).expect("thematic query byte"),
                        utf16: u32::try_from(SOURCE[..marker_start].encode_utf16().count())
                            .expect("thematic query UTF-16"),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                        maximum_open_depth: 64,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut output,
            )
            .expect("independent thematic-break query");
        let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
            panic!("thematic break must author an independent viewport: {outcome:?}");
        };
        assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
        assert_eq!(range.start.bytes as usize, thematic_start);
        assert_eq!(
            range.start.utf16 as usize,
            SOURCE[..thematic_start].encode_utf16().count()
        );
        assert_eq!(range.end.bytes as usize, thematic_end);
        assert_eq!(
            range.end.utf16 as usize,
            SOURCE[..thematic_end].encode_utf16().count()
        );

        let green = &output[VIEWPORT_HEADER_BYTES..VIEWPORT_HEADER_BYTES + 80];
        let projection = &output[VIEWPORT_HEADER_BYTES + 80..];
        assert_eq!(green[12], 6);
        assert_eq!(projection[12], 6);
        for record in [green, projection] {
            assert_eq!(
                u64::from_le_bytes(record[16..24].try_into().unwrap()),
                thematic_start as u64
            );
            assert_eq!(
                u64::from_le_bytes(record[24..32].try_into().unwrap()),
                thematic_end as u64
            );
        }
        assert_eq!(
            u64::from_le_bytes(green[32..40].try_into().unwrap()),
            thematic_start as u64
        );
        assert_eq!(
            u64::from_le_bytes(green[40..48].try_into().unwrap()),
            thematic_start as u64
        );
        assert_eq!(u64::from_le_bytes(green[48..56].try_into().unwrap()), 0x22d);
        assert_eq!(
            u32::from_le_bytes(green[56..60].try_into().unwrap()) as usize,
            marker_start
        );
        assert_eq!(
            u32::from_le_bytes(green[60..64].try_into().unwrap()) as usize,
            marker_end
        );
        assert_eq!(
            u32::from_le_bytes(green[64..68].try_into().unwrap()) as usize,
            line_ending_start
        );
        assert_eq!(
            u32::from_le_bytes(green[68..72].try_into().unwrap()) as usize,
            thematic_end
        );
        assert_eq!(u64::from_le_bytes(green[72..80].try_into().unwrap()), 3);
        assert_eq!(
            u64::from_le_bytes(projection[32..40].try_into().unwrap()),
            thematic_start as u64
        );
        assert_eq!(
            u64::from_le_bytes(projection[40..48].try_into().unwrap()),
            thematic_start as u64
        );
        assert_eq!(
            u64::from_le_bytes(projection[48..56].try_into().unwrap()),
            0,
            "a thematic break has no inline projection runs"
        );

        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 1,
                    source_version: delivery.ack.source_version,
                    base_ack: delivery.ack,
                    byte_offset: u32::try_from(marker_start).expect("bounded thematic point"),
                    utf16_offset: u32::try_from(SOURCE[..marker_start].encode_utf16().count())
                        .expect("bounded thematic UTF-16 point"),
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::Automatic,
                },
            )
            .expect("thematic-break inline demand");
        let (begin, ack) =
            deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 60_000);
        assert_eq!(begin.base_ack, delivery.ack);
        assert_eq!(begin.binding.physical_start_utf8 as usize, thematic_start);
        assert_eq!(begin.binding.physical_end_utf8 as usize, thematic_end);
        assert!(matches!(
            begin.envelope.disposition,
            HotInlineSidecarDisposition::Unsupported {
                reason: HOT_INLINE_UNSUPPORTED_NOT_INLINE_LEAF,
                ..
            }
        ));
        assert_eq!(ack.disposition, InlineSidecarAckDisposition::Unsupported);

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn thematic_break_large_interior_paragraph_transition_stays_exact() {
        const PARAGRAPHS: usize = 4_096;
        const EDITED_PARAGRAPH: usize = PARAGRAPHS / 2;
        const MAXIMUM_TRANSFERRED_RECORDS: u32 = 64;
        const VIEWPORT_HEADER_BYTES: usize = 20;
        const THEMATIC_SOURCE: &str = "  - - -  \r\n";

        let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [736, 737, 738, 739],
            source_session_identity: 740,
            worker_generation: 1,
        };
        let base_source: String = (0..PARAGRAPHS)
            .map(|ordinal| format!("paragraph {ordinal:04} {}\n\n", "a".repeat(32)))
            .collect();
        let mut current_source = base_source;
        let mut runtime = DocumentRuntime::new(&current_source, standard_document_runtime_config())
            .expect("large thematic-break runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut current_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start large thematic-break base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("large thematic-break host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes large thematic-break base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        let mut current_ack = base_delivery.ack;
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        for phase in 0..2 {
            let middle_marker = format!("paragraph {EDITED_PARAGRAPH:04} ");
            let (edit_range, replacement, expected_middle_variant) = if phase == 0 {
                let middle_start = current_source
                    .find(&middle_marker)
                    .expect("middle Paragraph");
                let middle_end = current_source[middle_start..]
                    .find('\n')
                    .map(|offset| middle_start + offset + 1)
                    .expect("middle Paragraph line ending");
                (middle_start..middle_end, THEMATIC_SOURCE.to_owned(), 6_u8)
            } else {
                let middle_start = current_source
                    .find(THEMATIC_SOURCE)
                    .expect("middle thematic break");
                (
                    middle_start..middle_start + THEMATIC_SOURCE.len(),
                    format!(
                        "paragraph {EDITED_PARAGRAPH:04} replacement {}\n",
                        "z".repeat(24)
                    ),
                    1_u8,
                )
            };
            let mut target_source = current_source.clone();
            target_source.replace_range(edit_range.clone(), &replacement);
            let target_version = runtime
                .apply_edit(current_version, edit_range, &replacement)
                .expect("apply thematic-break transition")
                .source()
                .current();
            let plan = runtime
                .begin_incremental_source_facts(
                    profile,
                    parser_profile,
                    SourceFactsRootLimits::default(),
                )
                .expect("plan thematic-break transition");
            assert!(
                endpoint
                    .has_incremental_base_for_plan(&runtime, &plan)
                    .expect("preflight thematic-break transition"),
                "phase {phase} must retain authenticated crop authority"
            );
            let witness = complete_incremental_source_facts(&mut runtime);
            let ui_revision = u32::try_from(phase + 2).expect("UI revision");
            let base_ui_revision = u32::try_from(phase + 1).expect("base UI revision");
            let completion =
                completion_for_persistent_target(&runtime, ui_revision, base_ui_revision);
            let source_version = source_version_for(binding, completion);
            host.observe_source_version(source_version)
                .expect("host observes thematic-break transition");
            endpoint
                .start_incremental(
                    &runtime,
                    runtime
                        .snapshot_current_source()
                        .expect("borrow thematic-break target"),
                    witness,
                    binding,
                    completion,
                )
                .expect("start thematic-break crop");
            assert_eq!(
                active_candidate_phase(endpoint.active.as_ref()),
                "ParsingOrdinaryExact",
                "phase {phase} must use the bounded ordinary crop"
            );
            let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
                &mut endpoint,
                &mut runtime,
                &mut host,
            );
            assert_eq!(delivery.offer.mode, PublicationMode::ExactBaseDelta);
            assert_eq!(delivery.offer.base_ack, Some(current_ack));
            assert!(
                delivery.offer.transferred_record_count <= MAXIMUM_TRANSFERRED_RECORDS,
                "phase {phase} transferred {} of {} records",
                delivery.offer.transferred_record_count,
                delivery.offer.target_record_count
            );
            let block_replacement_records = delivery
                .packet_frames
                .iter()
                .flatten()
                .filter(|(kind, _)| {
                    *kind == CandidateSnapshotFrameKind::BlockSequenceReplacementPage
                })
                .map(|(_, records)| *records)
                .sum::<u32>();
            assert!(
                block_replacement_records > 0
                    && block_replacement_records <= MAXIMUM_TRANSFERRED_RECORDS,
                "phase {phase} must publish one bounded structural splice"
            );

            for ordinal in [0, EDITED_PARAGRAPH, PARAGRAPHS - 1] {
                let paragraph_marker = format!("paragraph {ordinal:04} ");
                let block_start = if ordinal == EDITED_PARAGRAPH && expected_middle_variant == 6 {
                    target_source
                        .find(THEMATIC_SOURCE)
                        .expect("target thematic break")
                } else {
                    target_source
                        .find(&paragraph_marker)
                        .expect("target Paragraph")
                };
                let block_end = target_source[block_start..]
                    .find('\n')
                    .map(|offset| block_start + offset + 1)
                    .expect("target block line ending");
                let point = if ordinal == EDITED_PARAGRAPH && expected_middle_variant == 6 {
                    block_start + 2
                } else {
                    block_start + paragraph_marker.len()
                };
                let mut output = [0_u8; HOST_M11_VIEWPORT_BYTES];
                let outcome = host
                    .query_structural(
                        HostPointQuery {
                            source_version,
                            position: HostSourceMetric {
                                bytes: u32::try_from(point).expect("query byte"),
                                utf16: u32::try_from(target_source[..point].encode_utf16().count())
                                    .expect("query UTF-16"),
                            },
                            affinity: HostMetricAffinity::Downstream,
                            budget: HostQueryBudget {
                                maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                                maximum_open_depth: 64,
                                maximum_leaf_count: 64,
                                maximum_tree_nodes_visited: 256,
                            },
                        },
                        &mut output,
                    )
                    .expect("query exact thematic-break target");
                let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
                    panic!("phase {phase} must expose block {ordinal} exactly: {outcome:?}");
                };
                assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
                assert_eq!(range.start.bytes as usize, block_start);
                assert_eq!(range.end.bytes as usize, block_end);
                assert_eq!(
                    range.start.utf16 as usize,
                    target_source[..block_start].encode_utf16().count()
                );
                assert_eq!(
                    range.end.utf16 as usize,
                    target_source[..block_end].encode_utf16().count()
                );
                let green = &output[VIEWPORT_HEADER_BYTES..VIEWPORT_HEADER_BYTES + 80];
                let projection = &output[VIEWPORT_HEADER_BYTES + 80..];
                let expected_variant = if ordinal == EDITED_PARAGRAPH {
                    expected_middle_variant
                } else {
                    1
                };
                assert_eq!(green[12], expected_variant);
                assert_eq!(projection[12], expected_variant);
                for record in [green, projection] {
                    assert_eq!(
                        u64::from_le_bytes(record[16..24].try_into().unwrap()),
                        block_start as u64
                    );
                    assert_eq!(
                        u64::from_le_bytes(record[24..32].try_into().unwrap()),
                        block_end as u64
                    );
                }
                if expected_variant == 6 {
                    assert_eq!(
                        u64::from_le_bytes(green[32..40].try_into().unwrap()),
                        block_start as u64
                    );
                    assert_eq!(
                        u64::from_le_bytes(green[40..48].try_into().unwrap()),
                        block_start as u64
                    );
                    assert_eq!(u64::from_le_bytes(green[48..56].try_into().unwrap()), 0x22d);
                    assert_eq!(
                        u32::from_le_bytes(green[56..60].try_into().unwrap()) as usize,
                        block_start + 2
                    );
                    assert_eq!(
                        u32::from_le_bytes(green[60..64].try_into().unwrap()) as usize,
                        block_start + 7
                    );
                    assert_eq!(
                        u32::from_le_bytes(green[64..68].try_into().unwrap()) as usize,
                        block_start + 9
                    );
                    assert_eq!(
                        u32::from_le_bytes(green[68..72].try_into().unwrap()) as usize,
                        block_end
                    );
                    assert_eq!(u64::from_le_bytes(green[72..80].try_into().unwrap()), 3);
                    assert_eq!(
                        u64::from_le_bytes(projection[32..40].try_into().unwrap()),
                        block_start as u64
                    );
                    assert_eq!(
                        u64::from_le_bytes(projection[40..48].try_into().unwrap()),
                        block_start as u64
                    );
                    assert_eq!(
                        u64::from_le_bytes(projection[48..56].try_into().unwrap()),
                        0
                    );
                }
            }

            let retained = endpoint
                .retained
                .as_ref()
                .expect("retained thematic-break target");
            let CandidateRestartAuthority::Ordinary(checkpoints) = retained
                .restart
                .as_ref()
                .expect("thematic-break restart authority")
            else {
                panic!("thematic-break phase must retain ordinary target checkpoints");
            };
            assert_eq!(checkpoints.source(), target_version);
            assert!(checkpoints.is_segmented_top_level());
            drain_candidate_cleanup(&mut endpoint, &mut runtime);
            assert!(
                endpoint
                    .has_exact_base_for(&runtime, target_version)
                    .expect("next thematic-break revision authority")
            );

            current_source = target_source;
            current_version = target_version;
            current_ack = delivery.ack;
        }

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn setext_large_interior_paragraph_h1_h2_paragraph_sequence_stays_exact() {
        const PARAGRAPHS: usize = 4_096;
        const EDITED_PARAGRAPH: usize = PARAGRAPHS / 2;
        const MAXIMUM_TRANSFERRED_RECORDS: u32 = 64;
        const VIEWPORT_HEADER_BYTES: usize = 20;

        let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [731, 732, 733, 734],
            source_session_identity: 735,
            worker_generation: 1,
        };
        let base_source: String = (0..PARAGRAPHS)
            .map(|ordinal| format!("paragraph {ordinal:04} {}\n\n", "a".repeat(32)))
            .collect();
        let mut current_source = base_source;
        let mut runtime = DocumentRuntime::new(&current_source, standard_document_runtime_config())
            .expect("large Setext runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut current_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start large Setext base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("large Setext host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes large Setext base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        let mut current_ack = base_delivery.ack;
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        for phase in 0..3 {
            let middle_marker = format!("paragraph {EDITED_PARAGRAPH:04} ");
            let middle_start = current_source
                .find(&middle_marker)
                .expect("middle Paragraph");
            let content_line_end = current_source[middle_start..]
                .find('\n')
                .map(|offset| middle_start + offset + 1)
                .expect("middle content line ending");
            let (edit_range, replacement, expected_variant, expected_level) = match phase {
                0 => (
                    content_line_end..content_line_end + 1,
                    "===\n\n",
                    5_u8,
                    1_u64,
                ),
                1 => {
                    let marker_start = current_source[middle_start..]
                        .find("===\n\n")
                        .map(|offset| middle_start + offset)
                        .expect("H1 underline");
                    (marker_start..marker_start + 3, "---", 5_u8, 2_u64)
                }
                2 => {
                    let marker_start = current_source[middle_start..]
                        .find("---\n\n")
                        .map(|offset| middle_start + offset)
                        .expect("H2 underline");
                    (marker_start..marker_start + 5, "\n", 1_u8, 0_u64)
                }
                _ => unreachable!(),
            };
            let mut target_source = current_source.clone();
            target_source.replace_range(edit_range.clone(), replacement);
            let target_version = runtime
                .apply_edit(current_version, edit_range, replacement)
                .expect("apply Setext phase")
                .source()
                .current();
            let plan = runtime
                .begin_incremental_source_facts(
                    profile,
                    parser_profile,
                    SourceFactsRootLimits::default(),
                )
                .expect("plan Setext phase");
            assert!(
                endpoint
                    .has_incremental_base_for_plan(&runtime, &plan)
                    .expect("preflight Setext phase"),
                "Setext phase {phase} must retain authenticated crop authority"
            );
            let witness = complete_incremental_source_facts(&mut runtime);
            let ui_revision = u32::try_from(phase + 2).expect("UI revision");
            let base_ui_revision = u32::try_from(phase + 1).expect("base UI revision");
            let completion =
                completion_for_persistent_target(&runtime, ui_revision, base_ui_revision);
            let source_version = source_version_for(binding, completion);
            host.observe_source_version(source_version)
                .expect("host observes Setext phase");
            endpoint
                .start_incremental(
                    &runtime,
                    runtime
                        .snapshot_current_source()
                        .expect("borrow Setext target"),
                    witness,
                    binding,
                    completion,
                )
                .expect("start Setext crop");
            assert_eq!(
                active_candidate_phase(endpoint.active.as_ref()),
                "ParsingOrdinaryExact",
                "Setext phase {phase} must use the bounded ordinary crop"
            );
            let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
                &mut endpoint,
                &mut runtime,
                &mut host,
            );
            assert_eq!(delivery.offer.mode, PublicationMode::ExactBaseDelta);
            assert_eq!(delivery.offer.base_ack, Some(current_ack));
            assert!(
                delivery.offer.transferred_record_count <= MAXIMUM_TRANSFERRED_RECORDS,
                "Setext phase {phase} transferred {} of {} records",
                delivery.offer.transferred_record_count,
                delivery.offer.target_record_count
            );
            let block_replacement_records = delivery
                .packet_frames
                .iter()
                .flatten()
                .filter(|(kind, _)| {
                    *kind == CandidateSnapshotFrameKind::BlockSequenceReplacementPage
                })
                .map(|(_, records)| *records)
                .sum::<u32>();
            assert!(
                block_replacement_records > 0
                    && block_replacement_records <= MAXIMUM_TRANSFERRED_RECORDS,
                "Setext phase {phase} must publish one bounded structural splice"
            );

            for ordinal in [0, EDITED_PARAGRAPH, PARAGRAPHS - 1] {
                let marker = format!("paragraph {ordinal:04} ");
                let paragraph_start = target_source
                    .find(&marker)
                    .expect("target Paragraph marker");
                let content_end = target_source[paragraph_start..]
                    .find('\n')
                    .map(|offset| paragraph_start + offset + 1)
                    .expect("target content line ending");
                let paragraph_end = if ordinal == EDITED_PARAGRAPH && expected_variant == 5 {
                    content_end + 4
                } else {
                    content_end
                };
                let mut output = [0_u8; HOST_M11_VIEWPORT_BYTES];
                let point = paragraph_start + marker.len();
                let outcome = host
                    .query_structural(
                        HostPointQuery {
                            source_version,
                            position: HostSourceMetric {
                                bytes: u32::try_from(point).expect("query byte"),
                                utf16: u32::try_from(target_source[..point].encode_utf16().count())
                                    .expect("query UTF-16"),
                            },
                            affinity: HostMetricAffinity::Downstream,
                            budget: HostQueryBudget {
                                maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                                maximum_open_depth: 64,
                                maximum_leaf_count: 64,
                                maximum_tree_nodes_visited: 256,
                            },
                        },
                        &mut output,
                    )
                    .expect("query exact Setext target");
                let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
                    panic!("Setext phase {phase} must expose block {ordinal} exactly: {outcome:?}");
                };
                assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
                assert_eq!(range.start.bytes as usize, paragraph_start);
                assert_eq!(range.end.bytes as usize, paragraph_end);
                assert_eq!(
                    range.start.utf16 as usize,
                    target_source[..paragraph_start].encode_utf16().count()
                );
                assert_eq!(
                    range.end.utf16 as usize,
                    target_source[..paragraph_end].encode_utf16().count()
                );
                let green = &output[VIEWPORT_HEADER_BYTES..VIEWPORT_HEADER_BYTES + 80];
                let projection = &output[VIEWPORT_HEADER_BYTES + 80..];
                let variant = if ordinal == EDITED_PARAGRAPH {
                    expected_variant
                } else {
                    1
                };
                assert_eq!(green[12], variant);
                assert_eq!(projection[12], variant);
                if ordinal == EDITED_PARAGRAPH && expected_variant == 5 {
                    assert_eq!(
                        u64::from_le_bytes(green[48..56].try_into().unwrap()),
                        expected_level
                    );
                    assert_eq!(
                        u64::from_le_bytes(green[32..40].try_into().unwrap()) as usize,
                        paragraph_start
                    );
                    assert_eq!(
                        u64::from_le_bytes(green[40..48].try_into().unwrap()) as usize,
                        content_end - 1
                    );
                }
            }

            let retained = endpoint.retained.as_ref().expect("retained Setext target");
            let CandidateRestartAuthority::Ordinary(checkpoints) =
                retained.restart.as_ref().expect("Setext restart authority")
            else {
                panic!("Setext phase must retain ordinary target checkpoints");
            };
            assert_eq!(checkpoints.source(), target_version);
            assert!(checkpoints.is_segmented_top_level());
            drain_candidate_cleanup(&mut endpoint, &mut runtime);
            assert!(
                endpoint
                    .has_exact_base_for(&runtime, target_version)
                    .expect("next Setext revision authority")
            );

            current_source = target_source;
            current_version = target_version;
            current_ack = delivery.ack;
        }

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn whole_paragraph_replacement_keeps_ready_candidate_polling_live() {
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [641, 642, 643, 644],
            source_session_identity: 645,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new("plain", standard_document_runtime_config()).expect("runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_source = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start base candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(source_version_for(binding, completion))
            .expect("host observes base source");
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        runtime
            .apply_edit(base_source, 0..5, "**plain**")
            .expect("replace the whole Paragraph");
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan replacement SourceFacts");
        let incremental = endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("inspect exact route");
        let target_completion;
        if incremental {
            let witness = complete_incremental_source_facts(&mut runtime);
            let target = runtime
                .snapshot_current_source()
                .expect("exact replacement target");
            target_completion = completion_for_persistent_target(&runtime, 2, 1);
            endpoint
                .start_incremental(&runtime, target, witness, binding, target_completion)
                .expect("start exact replacement candidate");
        } else {
            assert!(runtime.cancel_source_facts());
            let (certified, completion) =
                complete_clean_source_facts(&mut runtime, profile, parser_profile, 2, 1);
            target_completion = completion;
            endpoint
                .start(certified, binding, target_completion)
                .expect("start definitive clean replacement candidate");
        }
        host.observe_source_version(source_version_for(binding, target_completion))
            .expect("host observes replacement source");

        let mut zero_progress_phase = None;
        let mut reached_event = false;
        for _ in 0..1_000 {
            let phase = active_candidate_phase(endpoint.active.as_ref());
            match endpoint
                .poll(&mut runtime, 32)
                .expect("bounded replacement candidate poll")
            {
                CandidatePoll::Pending { transitions } => {
                    assert!(transitions <= 32);
                    if transitions == 0 && zero_progress_phase.is_none() {
                        zero_progress_phase = Some(phase);
                    }
                }
                CandidatePoll::Event { transitions, .. } => {
                    assert!(transitions <= 32);
                    reached_event = true;
                    break;
                }
                CandidatePoll::HotInlineEvent { .. } => {
                    panic!("replacement structural candidate emitted a hot-inline event")
                }
                CandidatePoll::ViewportPresentationEvent { .. } => {
                    panic!("replacement structural candidate emitted a viewport event")
                }
                CandidatePoll::ViewportPresentationUnavailable { .. } => {
                    panic!("replacement candidate emitted viewport unavailability")
                }
            }
        }
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
        assert!(
            reached_event,
            "the replacement candidate did not reach its first bounded offer"
        );
        assert_eq!(
            zero_progress_phase, None,
            "a ready candidate phase must keep the native isolate poll edge alive"
        );
    }

    #[test]
    fn fresh_canonical_publication_omits_inline_presentation_authority() {
        fn delivered_viewport(source: &str, document_seed: u32) -> Vec<u8> {
            let profile = SourceFactsScanProfile::new(8).expect("test profile");
            let parser_profile = ParserProfileId::new(1).expect("parser profile");
            let binding = SessionBinding {
                document_session: [
                    document_seed,
                    document_seed + 1,
                    document_seed + 2,
                    document_seed + 3,
                ],
                source_session_identity: document_seed + 4,
                worker_generation: 1,
            };
            let mut runtime = DocumentRuntime::new(source, standard_document_runtime_config())
                .expect("fresh inline runtime");
            let (certified, completion) =
                complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
            let mut endpoint = CandidateEndpoint::new();
            endpoint
                .start(certified, binding, completion)
                .expect("start fresh inline candidate");
            let mut host = NativeCandidateHost::new(HostConfig {
                document_session: binding.document_session,
                grammar_revision: GRAMMAR_REVISION,
                syntax_profile: 1,
                authority_mask: AUTHORITY_MASK_ALL_ROLES,
                maximum_query_bytes: 64 * 1024,
            })
            .expect("fresh inline host");
            let source_version = source_version_for(binding, completion);
            host.observe_source_version(source_version)
                .expect("host observes fresh inline source");
            let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
                &mut endpoint,
                &mut runtime,
                &mut host,
            );
            assert_eq!(delivery.offer.mode, PublicationMode::FullSnapshot);
            let mut output = vec![0_u8; 4096];
            let outcome = host
                .query_structural(
                    HostPointQuery {
                        source_version,
                        position: HostSourceMetric { bytes: 0, utf16: 0 },
                        affinity: HostMetricAffinity::Downstream,
                        budget: HostQueryBudget {
                            maximum_encoded_bytes: output.len() as u32,
                            maximum_open_depth: 64,
                            maximum_leaf_count: 64,
                            maximum_tree_nodes_visited: 256,
                        },
                    },
                    &mut output,
                )
                .expect("query fresh inline viewport");
            let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
                panic!("fresh inline candidate must author a viewport: {outcome:?}");
            };
            output.truncate(receipt.encoded_bytes as usize);
            drain_candidate_cleanup(&mut endpoint, &mut runtime);
            close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
            output
        }

        for output in [
            delivered_viewport("A **bold _em_** and `code`.", 641),
            delivered_viewport("plain @ blocker", 631),
        ] {
            assert_eq!(output.len(), HOST_M11_VIEWPORT_BYTES);
            assert_eq!(
                u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
                1,
                "fresh canonical publication must remain structure-only until inline demand"
            );
        }
    }

    #[test]
    fn ordinary_target_cut_validation_distinguishes_lf_crlf_and_unterminated_eof() {
        let runtime = DocumentRuntime::new("a\nb\r\nc", standard_document_runtime_config())
            .expect("boundary runtime");
        let target = runtime.snapshot_current_source().expect("target lease");
        assert!(target_physical_line_cut_is_exact(&target, 0, 0).expect("BOF cut"));
        assert!(target_physical_line_cut_is_exact(&target, 2, 2).expect("LF cut"));
        assert!(
            !target_physical_line_cut_is_exact(&target, 4, 4).expect("inside CRLF"),
            "a cut between CR and LF is never a physical-line start"
        );
        assert!(target_physical_line_cut_is_exact(&target, 5, 5).expect("CRLF cut"));
        assert!(
            !target_physical_line_cut_is_exact(&target, 6, 6).expect("unterminated EOF"),
            "EOF after unterminated content is not itself a new physical-line start"
        );
        drop(target);
        let mut runtime = runtime;
        runtime.begin_close().expect("begin close");
        while !runtime.poll_close(64).expect("close poll").complete {}
    }

    #[test]
    fn ordinary_paragraph_bof_edit_streams_exact_segmented_delta_and_keeps_late_inline_live() {
        let source = format!(
            "**bold** ordinary paragraph line\n{}",
            "ordinary paragraph line\n".repeat(219)
        );
        assert!(
            source.len()
                > usize::try_from(flark_parser::M11_ORDINARY_PARAGRAPH_CHECKPOINT_STRIDE_BYTES)
                    .expect("checkpoint stride"),
            "fixture must cross the sparse ordinary-Paragraph checkpoint stride"
        );
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [681, 682, 683, 684],
            source_session_identity: 685,
            worker_generation: 1,
        };
        let mut runtime = DocumentRuntime::new(&source, standard_document_runtime_config())
            .expect("ordinary-Paragraph runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let source_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start clean ordinary-Paragraph candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent candidate host");
        host.observe_source_version(source_version_for(binding, completion))
            .expect("host observes ordinary-Paragraph source");

        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );

        assert_eq!(delivery.offer.mode, PublicationMode::FullSnapshot);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, source_version)
                .expect("inspect retained ordinary-Paragraph base"),
            "the delivered publication must retain its source- and binding-authenticated \
             ordinary-Paragraph restart collection"
        );
        let target_version = runtime
            .apply_edit(
                source_version,
                source.find("bold").expect("bold source") + 1
                    ..source.find("bold").expect("bold source") + 2,
                "O",
            )
            .expect("edit ordinary Paragraph at BOF")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan incremental SourceFacts");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("inspect planned incremental routing"),
            "an early edit must select BOF-to-convergence parser authority"
        );
        assert!(
            endpoint
                .has_exact_base_for(&runtime, source_version)
                .expect("reinspect retained ordinary-Paragraph base"),
            "an eligibility probe must not consume the move-only restart collection"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("borrow exact target source");
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        host.observe_source_version(source_version_for(binding, target_completion))
            .expect("host observes BOF target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start authenticated BOF crop");
        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(delivery.ack));
        assert!(
            target_delivery.offer.transferred_record_count
                < target_delivery.offer.target_record_count,
            "exact BOF delta must omit authenticated reused records"
        );
        let mut output = vec![0_u8; 4096];
        let target_source_version = source_version_for(binding, target_completion);
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: target_source_version,
                    position: HostSourceMetric { bytes: 0, utf16: 0 },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: output.len() as u32,
                        maximum_open_depth: 64,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut output,
            )
            .expect("query exact BOF inline facts");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("exact BOF candidate must author a structural viewport: {outcome:?}");
        };
        output.truncate(receipt.encoded_bytes as usize);
        assert_eq!(output.len(), HOST_M11_VIEWPORT_BYTES);
        assert_eq!(
            u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
            1,
            "segmented exact crop must not embed inline presentation authority"
        );
        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 1,
                    source_version: target_source_version,
                    base_ack: target_delivery.ack,
                    byte_offset: 3,
                    utf16_offset: 3,
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::Automatic,
                },
            )
            .expect("late inline demand must queue while superseded-base cleanup is pending");
        let (inline_begin, inline_ack) =
            deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 15_000);
        assert!(matches!(
            inline_begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative { fact_count, .. } if fact_count > 0
        ));
        assert_eq!(
            inline_ack.disposition,
            InlineSidecarAckDisposition::Authoritative
        );
        assert!(
            endpoint
                .has_exact_base_for(&runtime, target_version)
                .expect("inspect retained BOF target base"),
            "BOF delivery must retain target checkpoint authority"
        );

        let unsupported_edit = source.find("ordinary").expect("ordinary source");
        let unsupported_version = runtime
            .apply_edit(target_version, unsupported_edit..unsupported_edit, "@")
            .expect("insert exact inline hazard")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan unsupported exact SourceFacts");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight unsupported exact crop")
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("unsupported exact target source");
        let unsupported_completion = completion_for_persistent_target(&runtime, 3, 2);
        let unsupported_source_version = source_version_for(binding, unsupported_completion);
        host.observe_source_version(unsupported_source_version)
            .expect("host observes unsupported target");
        endpoint
            .start_incremental(
                &runtime,
                target_lease,
                witness,
                binding,
                unsupported_completion,
            )
            .expect("start unsupported exact crop");
        let unsupported_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(
            unsupported_delivery.offer.mode,
            PublicationMode::ExactBaseDelta
        );
        assert_eq!(
            unsupported_delivery.offer.base_ack,
            Some(target_delivery.ack)
        );
        assert!(
            unsupported_delivery.offer.transferred_record_count
                < unsupported_delivery.offer.target_record_count,
            "the next exact crop must retain the immediately preceding base"
        );
        assert_installed_candidate_has_no_inline(&host, unsupported_source_version);
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, unsupported_version)
                .expect("unsupported target remains an exact base")
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn exact_base_survives_mid_parse_cancel_and_replacement_converges() {
        let mut fixture = OrdinaryCancellationFixture::new([721, 722, 723, 724]);
        let first_edit = fixture.edit_offset(512);
        fixture.start_target(first_edit, "Z", 2, 1);
        assert_eq!(
            active_candidate_phase(fixture.endpoint.active.as_ref()),
            "ParsingOrdinaryExact"
        );
        assert!(matches!(
            fixture
                .endpoint
                .poll(&mut fixture.runtime, 1)
                .expect("advance bounded ordinary crop"),
            CandidatePoll::Pending { transitions: 1 }
        ));
        assert_eq!(
            active_candidate_phase(fixture.endpoint.active.as_ref()),
            "ParsingOrdinaryExact",
            "fixture must cancel after parser work, not before the crop starts"
        );

        fixture.endpoint.cancel().expect("cancel target mid-parse");
        drain_candidate_cleanup(&mut fixture.endpoint, &mut fixture.runtime);
        fixture.assert_original_base_restored();

        let replacement_edit = fixture.edit_offset(513);
        let replacement = fixture.start_target(replacement_edit, "Y", 3, 2);
        fixture.deliver_replacement(replacement);
        close_exact_pair_to_zero(
            &mut fixture.endpoint,
            &mut fixture.runtime,
            &mut fixture.host,
        );
    }

    #[test]
    fn exact_base_survives_mid_stream_cancel_and_replacement_converges() {
        let mut fixture = OrdinaryCancellationFixture::new([731, 732, 733, 734]);
        let first_edit = fixture.edit_offset(512);
        fixture.start_target(first_edit, "Z", 2, 1);

        let mut saw_begin = false;
        let mut saw_packet = false;
        for event_id in 1..1_000_000_u32 {
            match fixture
                .endpoint
                .poll(&mut fixture.runtime, 1)
                .expect("advance exact target to stream")
            {
                CandidatePoll::Pending { transitions } => assert_eq!(transitions, 1),
                CandidatePoll::Event { transitions, event } => {
                    assert!(transitions <= 1);
                    let CandidateEvent { credit, body } = *event;
                    match body {
                        CandidateEventBody::Begin(_) => {
                            assert!(!saw_begin);
                            saw_begin = true;
                            fixture
                                .endpoint
                                .accept_credit(credit, event_id)
                                .expect("accept target Begin credit");
                        }
                        CandidateEventBody::Packet { encoded } => {
                            assert!(saw_begin);
                            let packet = decode_publication_packet(&encoded)
                                .expect("decode in-flight exact packet");
                            assert!(packet.frame_count > 0);
                            let offer_id = packet.offer_id;
                            let Some(ActiveCandidate::Streaming(streaming)) =
                                fixture.endpoint.active.as_ref()
                            else {
                                panic!("packet receipt must leave the exact target streaming");
                            };
                            assert!(streaming.stream.is_some());
                            assert!(streaming.sealed_publication.is_none());
                            assert!(streaming.superseded_exact_base.is_none());
                            assert!(streaming.exact_base_recovery.is_some());
                            assert!(matches!(
                                streaming.phase,
                                StreamPhase::AwaitPacketReceipt { .. }
                            ));
                            fixture
                                .endpoint
                                .accept_credit(credit, event_id)
                                .expect("accept target Packet credit");
                            assert!(fixture
                                .endpoint
                                .handle_host_poll(
                                    event_id,
                                    offer_id,
                                    HostPollPhase::PacketCredit,
                                    HostPollResult::Rejected(
                                        crate::v3_publication_wire::HostRejectReason::Superseded,
                                    ),
                                )
                                .expect("reject target mid-stream")
                                .is_none());
                            saw_packet = true;
                            break;
                        }
                        CandidateEventBody::Commit(_)
                        | CandidateEventBody::DeliveryAcknowledged(_) => {
                            panic!("fixture must cancel before exact target commit")
                        }
                    }
                }
                CandidatePoll::HotInlineEvent { .. } => {
                    panic!("structural cancellation fixture emitted hot-inline work")
                }
                CandidatePoll::ViewportPresentationEvent { .. } => {
                    panic!("structural cancellation fixture emitted viewport work")
                }
                CandidatePoll::ViewportPresentationUnavailable { .. } => {
                    panic!("structural fixture emitted viewport unavailability")
                }
            }
        }
        assert!(saw_packet, "exact target did not reach packet streaming");
        assert!(matches!(
            fixture.endpoint.cleanup.as_ref(),
            Some(CandidateCleanup::ExactStreamAndRestore {
                base: None,
                recovery: Some(_),
                ..
            })
        ));
        drain_candidate_cleanup(&mut fixture.endpoint, &mut fixture.runtime);
        fixture.assert_original_base_restored();

        let replacement_edit = fixture.edit_offset(513);
        let replacement = fixture.start_target(replacement_edit, "Y", 3, 2);
        fixture.deliver_replacement(replacement);
        close_exact_pair_to_zero(
            &mut fixture.endpoint,
            &mut fixture.runtime,
            &mut fixture.host,
        );
    }

    #[test]
    fn ordinary_paragraph_middle_edit_streams_exact_segmented_delta() {
        let profile = SourceFactsScanProfile::new(2).expect("dense test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [711, 712, 713, 714],
            source_session_identity: 715,
            worker_generation: 1,
        };
        let mut base_source: String = (0..1_024)
            .map(|ordinal| format!("ordinary prose line {ordinal:04} {}\n", "a".repeat(40)))
            .collect();
        base_source.push_str("\nLate **bold** and _live_.\n");
        let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
            .expect("ordinary exact-delta runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start clean ordinary base candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent candidate host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes ordinary base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

        let edit_start = base_source
            .find("ordinary prose line 0512 ")
            .expect("middle line")
            + "ordinary prose line 0512 ".len()
            + 20;
        let target_version = runtime
            .apply_edit(base_version, edit_start..edit_start + 1, "Z")
            .expect("middle ordinary edit")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan bounded SourceFacts replacement");
        assert!(
            plan.base_byte_range().start > 0
                && plan.base_byte_range().end < base_version.byte_len(),
            "fixture must leave exact parser authority on both sides of the changed pages"
        );
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight ordinary crop"),
            "middle edit must have an authenticated restart and convergence line"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("borrow exact target source");
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        host.observe_source_version(source_version_for(binding, target_completion))
            .expect("host observes exact ordinary target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start authenticated ordinary crop candidate");
        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );

        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
        let block_replacement_frames = target_delivery
            .packet_frames
            .iter()
            .flatten()
            .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::BlockSequenceReplacementPage)
            .collect::<Vec<_>>();
        assert_eq!(
            block_replacement_frames.len(),
            1,
            "one local ordinary-Paragraph edit must transfer one packed block page"
        );
        assert_eq!(
            block_replacement_frames[0].1, 1,
            "one BlockSequence replacement page is one canonical transport record"
        );
        let target_source_version = source_version_for(binding, target_completion);
        let mut output = vec![0_u8; 4096];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: target_source_version,
                    position: HostSourceMetric {
                        bytes: u32::try_from(edit_start).expect("test edit byte"),
                        utf16: u32::try_from(edit_start).expect("ASCII test edit UTF-16"),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: output.len() as u32,
                        maximum_open_depth: 64,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut output,
            )
            .expect("query independently replayed target");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("replayed target must expose the edited Paragraph: {outcome:?}");
        };
        assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
        assert_eq!(
            u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
            1,
            "persistent block reuse must not manufacture inline authority"
        );
        assert!(
            target_delivery.offer.transferred_record_count
                < target_delivery.offer.target_record_count,
            "exact middle delta must omit authenticated reused records"
        );
        let retained = endpoint.retained.as_ref().expect("retained target base");
        let CandidateRestartAuthority::Ordinary(checkpoints) =
            retained.restart.as_ref().expect("target restart authority")
        else {
            panic!("ordinary crop must retain ordinary target checkpoints");
        };
        assert_eq!(checkpoints.source(), target_version);
        assert!(
            checkpoints.len() > 2,
            "target must preserve sparse authority on both sides of the crop"
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, target_version)
                .expect("next-revision exact base")
        );

        let late_inline_point = base_source
            .find("bold")
            .expect("late recursive-Green inline point");
        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 1,
                    source_version: target_source_version,
                    base_ack: target_delivery.ack,
                    byte_offset: u32::try_from(late_inline_point).expect("late inline test byte"),
                    utf16_offset: u32::try_from(late_inline_point)
                        .expect("ASCII late inline test UTF-16"),
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::RecursiveGreenParagraph,
                },
            )
            .expect("retained recursive-Green Paragraph query above the old 64-KiB cap");
        let (inline_begin, inline_ack) =
            deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 25_000);
        assert!(matches!(
            inline_begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative { fact_count, .. }
                if fact_count > 0
        ));
        assert_eq!(
            inline_ack.disposition,
            InlineSidecarAckDisposition::Authoritative
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn independent_host_4096_paragraph_middle_edit_is_bounded_exact_delta() {
        const PARAGRAPHS: usize = 4_096;
        const VIEWPORT_HEADER_BYTES: usize = 20;
        const MAXIMUM_CROP_BYTES: usize = 16 * 1024;
        const MAXIMUM_CROP_PHYSICAL_LINES: usize = 512;
        const MAXIMUM_CROP_PARSER_TRANSITIONS: usize = 4_096;

        let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [901, 902, 903, 904],
            source_session_identity: 905,
            worker_generation: 1,
        };
        let base_source: String = (0..PARAGRAPHS)
            .map(|ordinal| format!("paragraph {ordinal:04} {}\n\n", "a".repeat(32)))
            .collect();
        let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
            .expect("4,096-Paragraph runtime");
        let base_started = std::time::Instant::now();
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start 4,096-Paragraph base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent 4,096-Paragraph host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes exact base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        let base_elapsed = base_started.elapsed();
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert!(matches!(
            endpoint
                .retained
                .as_ref()
                .and_then(|retained| retained.restart.as_ref()),
            Some(CandidateRestartAuthority::Ordinary(checkpoints))
                if checkpoints.source() == base_version
                    && checkpoints.is_segmented_top_level()
        ));

        let paragraph_start = base_source
            .find("paragraph 2048 ")
            .expect("middle Paragraph");
        let edit_start = paragraph_start + "paragraph 2048 ".len() + 16;
        let mut target_source = base_source.clone();
        target_source.replace_range(edit_start..edit_start + 1, "Z");
        let paragraph_end = target_source[paragraph_start..]
            .find('\n')
            .map(|offset| paragraph_start + offset + 1)
            .expect("middle Paragraph line ending");
        let incremental_started = std::time::Instant::now();
        let target_version = runtime
            .apply_edit(base_version, edit_start..edit_start + 1, "Z")
            .expect("shape-preserving middle Paragraph edit")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan bounded SourceFacts replacement");
        assert_eq!(
            plan.base_byte_range(),
            &(0..base_version.byte_len()),
            "the production SourceFacts page may span the complete fixture"
        );
        assert_eq!(
            plan.exact_parser_base_byte_range(),
            Some(&(edit_start..edit_start + 1)),
            "parser restart must use the exact edit envelope, not the storage page"
        );
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight bounded ordinary crop"),
            "the middle edit must select authenticated restart and convergence authority"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("borrow exact target");
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        let target_source_version = source_version_for(binding, target_completion);
        host.observe_source_version(target_source_version)
            .expect("host observes exact target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start bounded ordinary crop");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingOrdinaryExact",
            "the 4,096-Paragraph edit must enter the crop parser, not exact-clean fallback"
        );

        let Some(ActiveCandidate::ParsingOrdinaryExact(mut parsing)) = endpoint.active.take()
        else {
            unreachable!("phase asserted above")
        };
        let mut crop_poll_transitions = 0_usize;
        let ordinary_result = loop {
            match parsing
                .job
                .poll(1)
                .expect("poll authenticated ordinary crop")
            {
                OrdinaryExactPoll::Pending { transitions } => {
                    assert!(transitions <= 1);
                    crop_poll_transitions = crop_poll_transitions
                        .checked_add(transitions)
                        .expect("crop transition count");
                }
                OrdinaryExactPoll::Complete {
                    transitions,
                    result,
                } => {
                    assert!(transitions <= 1);
                    crop_poll_transitions = crop_poll_transitions
                        .checked_add(transitions)
                        .expect("crop transition count");
                    break result;
                }
            }
        };
        let OrdinaryExactResult::Interior(cropped) = &ordinary_result else {
            panic!("a middle edit must complete the interior crop route");
        };
        let work = cropped.work();
        let crop_range = work.target_crop_bytes();
        let crop_source_bytes = work.crop_source_bytes_discovered();
        let crop_physical_lines = work.crop_physical_lines_discovered();
        let crop_parser_transitions = work.crop_parser_transitions();
        let crop_merge_transitions = work.checkpoint_merge_transitions();
        let reused_prefix_checkpoints = work.reused_prefix_checkpoints();
        let fresh_crop_checkpoints = work.fresh_crop_checkpoints();
        let reused_suffix_checkpoints = work.reused_suffix_checkpoints();
        assert!(
            crop_range.start > 0 && crop_range.end < target_source.len(),
            "the parser crop must leave unchanged source on both sides"
        );
        assert!(
            crop_range.start <= edit_start && edit_start < crop_range.end,
            "the bounded crop must contain the edited byte"
        );
        assert_eq!(
            work.crop_source_bytes_discovered(),
            crop_range.len(),
            "the work receipt must charge the complete crop and only the crop"
        );
        assert_eq!(work.crop_source_bytes_read(), crop_range.len());
        assert!(
            work.crop_source_bytes_discovered() <= MAXIMUM_CROP_BYTES
                && work.crop_source_bytes_discovered() <= M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES,
            "crop discovered {} of {} document bytes",
            work.crop_source_bytes_discovered(),
            target_source.len()
        );
        assert!(
            work.crop_physical_lines_discovered() <= MAXIMUM_CROP_PHYSICAL_LINES,
            "crop discovered {} of {} physical lines",
            work.crop_physical_lines_discovered(),
            PARAGRAPHS * 2
        );
        assert!(
            work.crop_parser_transitions() <= MAXIMUM_CROP_PARSER_TRANSITIONS,
            "crop used {} parser transitions",
            work.crop_parser_transitions()
        );
        assert!(work.reused_prefix_checkpoints() > 0);
        assert!(work.fresh_crop_checkpoints() > 0);
        assert!(work.reused_suffix_checkpoints() > 0);
        assert_eq!(
            work.convergence_ordinal_delta(),
            0,
            "a shape-preserving edit must keep downstream block ordinals stable"
        );
        assert_eq!(
            crop_poll_transitions,
            work.crop_parser_transitions() + work.checkpoint_merge_transitions(),
            "endpoint polling must account for all exposed crop and merge work"
        );
        assert!(
            work.maximum_checkpoint_records_per_transition()
                <= flark_parser::M11_ORDINARY_CHECKPOINT_MERGE_RECORDS_PER_TRANSITION,
            "checkpoint merge must honor its exposed per-transition record bound"
        );

        let ParsingOrdinaryExactCandidate {
            context,
            base,
            witness,
            ..
        } = *parsing;
        endpoint.active = Some(
            match begin_exact_candidate_build_ordinary(
                &mut runtime,
                context,
                base,
                witness,
                ordinary_result,
            ) {
                Ok(active) => active,
                Err(failure) => panic!("start exact crop build: {}", failure.error),
            },
        );
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "BuildingExact",
            "the completed crop must stay on the incremental exact builder"
        );
        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        let incremental_elapsed = incremental_started.elapsed();

        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
        assert_eq!(target_delivery.ack.source_version, target_source_version);
        assert!(
            target_delivery.offer.transferred_record_count
                < target_delivery.offer.target_record_count,
            "the exact delta must omit authenticated unchanged records"
        );
        let block_replacement_records = target_delivery
            .packet_frames
            .iter()
            .flatten()
            .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::BlockSequenceReplacementPage)
            .map(|(_, records)| *records)
            .sum::<u32>();
        assert!(
            block_replacement_records > 0,
            "the exact delta must carry a nonempty middle block replacement window"
        );
        assert!(
            block_replacement_records < PARAGRAPHS as u32,
            "one middle edit must not transfer a document-sized block replacement"
        );
        eprintln!(
            "m11_4096_paragraph_bounded_exact_delta source_bytes={} base_ms={} \
             incremental_ms={} crop_bytes={} crop_lines={} crop_parser_transitions={} \
             crop_merge_transitions={} reused_prefix_checkpoints={} \
             fresh_crop_checkpoints={} reused_suffix_checkpoints={} target_records={} \
             transferred_records={} block_replacement_records={}",
            base_source.len(),
            base_elapsed.as_millis(),
            incremental_elapsed.as_millis(),
            crop_source_bytes,
            crop_physical_lines,
            crop_parser_transitions,
            crop_merge_transitions,
            reused_prefix_checkpoints,
            fresh_crop_checkpoints,
            reused_suffix_checkpoints,
            target_delivery.offer.target_record_count,
            target_delivery.offer.transferred_record_count,
            block_replacement_records,
        );

        let mut output = [0_u8; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: target_source_version,
                    position: HostSourceMetric {
                        bytes: u32::try_from(edit_start).expect("edit byte"),
                        utf16: u32::try_from(edit_start).expect("ASCII edit UTF-16"),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                        maximum_open_depth: 64,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut output,
            )
            .expect("query independently installed target");
        let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
            panic!("installed target must expose the edited Paragraph: {outcome:?}");
        };
        assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
        assert_eq!(range.start.bytes as usize, paragraph_start);
        assert_eq!(range.start.utf16 as usize, paragraph_start);
        assert_eq!(range.end.bytes as usize, paragraph_end);
        assert_eq!(range.end.utf16 as usize, paragraph_end);
        assert_eq!(
            u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
            1
        );
        let green = &output[VIEWPORT_HEADER_BYTES..VIEWPORT_HEADER_BYTES + 80];
        let projection = &output[VIEWPORT_HEADER_BYTES + 80..];
        assert_eq!(green[12], M11BlockSequenceEntryKind::Paragraph as u8);
        assert_eq!(projection[12], M11BlockSequenceEntryKind::Paragraph as u8);
        for record in [green, projection] {
            assert_eq!(
                u64::from_le_bytes(record[16..24].try_into().expect("source start")),
                paragraph_start as u64
            );
            assert_eq!(
                u64::from_le_bytes(record[24..32].try_into().expect("source end")),
                paragraph_end as u64
            );
        }
        let target_lease = runtime
            .snapshot_current_source()
            .expect("reborrow exact installed source");
        assert_eq!(target_lease.version(), target_version);
        let mut cursor = target_lease
            .cursor_in(paragraph_start..paragraph_end)
            .expect("bounded target Paragraph cursor");
        let mut copied = vec![0_u8; paragraph_end - paragraph_start];
        assert_eq!(cursor.read(&mut copied), copied.len());
        drop(cursor.finish().expect("finish target Paragraph cursor"));
        assert_eq!(
            copied,
            target_source.as_bytes()[paragraph_start..paragraph_end],
            "the independently installed semantic range must name exact canonical source"
        );

        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, target_version)
                .expect("target exact-base continuity")
        );
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn independent_host_4096_paragraph_first_edit_is_bounded_exact_delta() {
        const PARAGRAPHS: usize = 4_096;
        const MAXIMUM_TRANSFERRED_RECORDS: u32 = 64;

        let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [911, 912, 913, 914],
            source_session_identity: 915,
            worker_generation: 1,
        };
        let base_source: String = (0..PARAGRAPHS)
            .map(|ordinal| format!("paragraph {ordinal:04} {}\n\n", "a".repeat(32)))
            .collect();
        let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
            .expect("first-Paragraph runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start first-Paragraph base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent first-Paragraph host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes first-Paragraph base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert!(matches!(
            endpoint
                .retained
                .as_ref()
                .and_then(|retained| retained.restart.as_ref()),
            Some(CandidateRestartAuthority::Ordinary(checkpoints))
                if checkpoints.source() == base_version
                    && checkpoints.is_segmented_top_level()
        ));

        let edit_start = base_source
            .find("aaaaaaaa")
            .expect("first Paragraph payload")
            + 4;
        const REPLACEMENT: &str = "expanded";
        let mut target_source = base_source.clone();
        target_source.replace_range(edit_start..edit_start + 1, REPLACEMENT);
        let target_version = runtime
            .apply_edit(base_version, edit_start..edit_start + 1, REPLACEMENT)
            .expect("lengthen first Paragraph")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan first-Paragraph SourceFacts replacement");
        assert_eq!(
            plan.exact_parser_base_byte_range(),
            Some(&(edit_start..edit_start + 1)),
            "BOF selection must follow the exact edit envelope"
        );
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight segmented BOF crop"),
            "a first-block edit must select authenticated BOF-to-Paragraph convergence"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        let target_source_version = source_version_for(binding, target_completion);
        host.observe_source_version(target_source_version)
            .expect("host observes first-Paragraph target");
        endpoint
            .start_incremental(
                &runtime,
                runtime
                    .snapshot_current_source()
                    .expect("borrow first-Paragraph target"),
                witness,
                binding,
                target_completion,
            )
            .expect("start segmented BOF crop");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingOrdinaryExact",
            "the first-block edit must enter the boundary crop, not exact-clean fallback"
        );

        let Some(ActiveCandidate::ParsingOrdinaryExact(mut parsing)) = endpoint.active.take()
        else {
            unreachable!("phase asserted above")
        };
        let mut crop_poll_transitions = 0_usize;
        let ordinary_result = loop {
            match parsing.job.poll(1).expect("poll segmented BOF crop") {
                OrdinaryExactPoll::Pending { transitions } => {
                    assert!(transitions <= 1);
                    crop_poll_transitions = crop_poll_transitions
                        .checked_add(transitions)
                        .expect("BOF crop transition count");
                }
                OrdinaryExactPoll::Complete {
                    transitions,
                    result,
                } => {
                    assert!(transitions <= 1);
                    crop_poll_transitions = crop_poll_transitions
                        .checked_add(transitions)
                        .expect("BOF crop transition count");
                    break result;
                }
            }
        };
        let OrdinaryExactResult::Boundary(cropped) = &ordinary_result else {
            panic!("a first-block edit must complete the BOF boundary route");
        };
        let work = cropped.work();
        let crop_range = work.target_crop_bytes();
        assert_eq!(crop_range.start, 0);
        assert!(
            crop_range.end < target_source.len(),
            "BOF crop must retain a document-sized authenticated suffix"
        );
        assert!(edit_start < crop_range.end);
        assert_eq!(work.crop_source_bytes_discovered(), crop_range.len());
        assert_eq!(work.crop_source_bytes_read(), crop_range.len());
        assert!(
            work.crop_source_bytes_discovered() <= M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES,
            "BOF crop read {} of {} document bytes",
            work.crop_source_bytes_discovered(),
            target_source.len()
        );
        assert_eq!(work.reused_prefix_checkpoints(), 0);
        assert!(work.fresh_crop_checkpoints() > 0);
        assert!(
            work.reused_suffix_checkpoints() > 0,
            "BOF convergence must retain downstream restart authority"
        );
        assert_eq!(
            work.convergence_ordinal_delta(),
            Some(0),
            "a length-only first-block edit must preserve block ordinals"
        );
        assert!(
            crop_poll_transitions >= work.checkpoint_merge_transitions(),
            "the work receipt must not charge more merge work than endpoint polling performed"
        );

        let ParsingOrdinaryExactCandidate {
            context,
            base,
            witness,
            ..
        } = *parsing;
        endpoint.active = Some(
            match begin_exact_candidate_build_ordinary(
                &mut runtime,
                context,
                base,
                witness,
                ordinary_result,
            ) {
                Ok(active) => active,
                Err(failure) => panic!("start exact segmented BOF build: {}", failure.error),
            },
        );
        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
        assert_eq!(target_delivery.ack.source_version, target_source_version);
        assert!(
            target_delivery.offer.transferred_record_count <= MAXIMUM_TRANSFERRED_RECORDS,
            "one first-block edit transferred {} of {} records",
            target_delivery.offer.transferred_record_count,
            target_delivery.offer.target_record_count
        );
        assert!(
            target_delivery.offer.transferred_record_count
                < target_delivery.offer.target_record_count
        );
        let block_replacement_records = target_delivery
            .packet_frames
            .iter()
            .flatten()
            .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::BlockSequenceReplacementPage)
            .map(|(_, records)| *records)
            .sum::<u32>();
        assert!(
            block_replacement_records > 0
                && block_replacement_records <= MAXIMUM_TRANSFERRED_RECORDS,
            "the first-block splice must publish a bounded nonempty replacement"
        );

        for ordinal in [0, PARAGRAPHS / 2, PARAGRAPHS - 1] {
            let marker = format!("paragraph {ordinal:04} ");
            let paragraph_start = target_source
                .find(&marker)
                .expect("target Paragraph marker");
            let paragraph_end = target_source[paragraph_start..]
                .find('\n')
                .map(|offset| paragraph_start + offset + 1)
                .expect("target Paragraph line ending");
            let point = paragraph_start + marker.len();
            let mut output = [0_u8; HOST_M11_VIEWPORT_BYTES];
            let outcome = host
                .query_structural(
                    HostPointQuery {
                        source_version: target_source_version,
                        position: HostSourceMetric {
                            bytes: u32::try_from(point).expect("Paragraph point byte"),
                            utf16: u32::try_from(point).expect("ASCII Paragraph point UTF-16"),
                        },
                        affinity: HostMetricAffinity::Downstream,
                        budget: HostQueryBudget {
                            maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                            maximum_open_depth: 64,
                            maximum_leaf_count: 64,
                            maximum_tree_nodes_visited: 256,
                        },
                    },
                    &mut output,
                )
                .expect("query installed BOF target");
            let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
                panic!("installed target must expose Paragraph {ordinal}: {outcome:?}");
            };
            assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
            assert_eq!(range.start.bytes as usize, paragraph_start);
            assert_eq!(range.start.utf16 as usize, paragraph_start);
            assert_eq!(range.end.bytes as usize, paragraph_end);
            assert_eq!(range.end.utf16 as usize, paragraph_end);
        }

        let retained = endpoint.retained.as_ref().expect("retained BOF target");
        let CandidateRestartAuthority::Ordinary(checkpoints) =
            retained.restart.as_ref().expect("target restart authority")
        else {
            panic!("segmented BOF crop must retain ordinary target checkpoints");
        };
        assert_eq!(checkpoints.source(), target_version);
        assert!(checkpoints.is_segmented_top_level());
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let next_edit = target_source
            .find("paragraph 2048 ")
            .expect("next-edit Paragraph")
            + "paragraph 2048 ".len();
        let next_version = runtime
            .apply_edit(target_version, next_edit..next_edit + 1, "Q")
            .expect("apply next exact edit")
            .source()
            .current();
        let next_plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan next exact edit");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &next_plan)
                .expect("preflight next exact edit")
        );
        let next_witness = complete_incremental_source_facts(&mut runtime);
        let next_completion = completion_for_persistent_target(&runtime, 3, 2);
        host.observe_source_version(source_version_for(binding, next_completion))
            .expect("host observes next target");
        endpoint
            .start_incremental(
                &runtime,
                runtime
                    .snapshot_current_source()
                    .expect("borrow next target"),
                next_witness,
                binding,
                next_completion,
            )
            .expect("start next exact edit");
        let next_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(next_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(next_delivery.offer.base_ack, Some(target_delivery.ack));
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, next_version)
                .expect("next-revision exact base")
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn independent_host_4096_paragraph_final_edit_is_bounded_exact_delta() {
        const PARAGRAPHS: usize = 4_096;
        const MAXIMUM_TRANSFERRED_RECORDS: u32 = 64;

        let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [916, 917, 918, 919],
            source_session_identity: 920,
            worker_generation: 1,
        };
        let base_source: String = (0..PARAGRAPHS)
            .map(|ordinal| format!("paragraph {ordinal:04} {}\n\n", "a".repeat(32)))
            .collect();
        let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
            .expect("final-Paragraph runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start final-Paragraph base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent final-Paragraph host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes final-Paragraph base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert!(matches!(
            endpoint
                .retained
                .as_ref()
                .and_then(|retained| retained.restart.as_ref()),
            Some(CandidateRestartAuthority::Ordinary(checkpoints))
                if checkpoints.source() == base_version
                    && checkpoints.is_segmented_top_level()
        ));

        let edit_start = base_source
            .rfind("aaaaaaaa")
            .expect("final Paragraph payload")
            + 4;
        const REPLACEMENT: &str = "expanded";
        let mut target_source = base_source.clone();
        target_source.replace_range(edit_start..edit_start + 1, REPLACEMENT);
        let target_version = runtime
            .apply_edit(base_version, edit_start..edit_start + 1, REPLACEMENT)
            .expect("lengthen final Paragraph")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan final-Paragraph SourceFacts replacement");
        assert_eq!(
            plan.exact_parser_base_byte_range(),
            Some(&(edit_start..edit_start + 1)),
            "EOF selection must follow the exact edit envelope"
        );
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight segmented EOF crop"),
            "a final-block edit must select authenticated Paragraph-to-EOF authority"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        let target_source_version = source_version_for(binding, target_completion);
        host.observe_source_version(target_source_version)
            .expect("host observes final-Paragraph target");
        endpoint
            .start_incremental(
                &runtime,
                runtime
                    .snapshot_current_source()
                    .expect("borrow final-Paragraph target"),
                witness,
                binding,
                target_completion,
            )
            .expect("start segmented EOF crop");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingOrdinaryExact",
            "the final-block edit must enter the boundary crop, not exact-clean fallback"
        );

        let Some(ActiveCandidate::ParsingOrdinaryExact(mut parsing)) = endpoint.active.take()
        else {
            unreachable!("phase asserted above")
        };
        let mut crop_poll_transitions = 0_usize;
        let ordinary_result = loop {
            match parsing.job.poll(1).expect("poll segmented EOF crop") {
                OrdinaryExactPoll::Pending { transitions } => {
                    assert!(transitions <= 1);
                    crop_poll_transitions = crop_poll_transitions
                        .checked_add(transitions)
                        .expect("EOF crop transition count");
                }
                OrdinaryExactPoll::Complete {
                    transitions,
                    result,
                } => {
                    assert!(transitions <= 1);
                    crop_poll_transitions = crop_poll_transitions
                        .checked_add(transitions)
                        .expect("EOF crop transition count");
                    break result;
                }
            }
        };
        let OrdinaryExactResult::Boundary(cropped) = &ordinary_result else {
            panic!("a final-block edit must complete the EOF boundary route");
        };
        let work = cropped.work();
        let crop_range = work.target_crop_bytes();
        assert!(
            crop_range.start > 0,
            "EOF crop must retain an authenticated document prefix"
        );
        assert_eq!(crop_range.end, target_source.len());
        assert!(crop_range.start <= edit_start);
        assert_eq!(work.crop_source_bytes_discovered(), crop_range.len());
        assert_eq!(work.crop_source_bytes_read(), crop_range.len());
        assert!(
            work.crop_source_bytes_discovered() <= M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES,
            "EOF crop read {} of {} document bytes",
            work.crop_source_bytes_discovered(),
            target_source.len()
        );
        assert!(
            work.reused_prefix_checkpoints() > 0,
            "EOF restart must retain upstream restart authority"
        );
        assert_eq!(
            work.fresh_crop_checkpoints(),
            0,
            "a final-block crop need not mint a checkpoint beyond EOF"
        );
        assert_eq!(work.reused_suffix_checkpoints(), 0);
        assert_eq!(work.convergence_ordinal_delta(), None);
        assert!(
            crop_poll_transitions >= work.checkpoint_merge_transitions(),
            "the work receipt must not charge more merge work than endpoint polling performed"
        );

        let ParsingOrdinaryExactCandidate {
            context,
            base,
            witness,
            ..
        } = *parsing;
        endpoint.active = Some(
            match begin_exact_candidate_build_ordinary(
                &mut runtime,
                context,
                base,
                witness,
                ordinary_result,
            ) {
                Ok(active) => active,
                Err(failure) => panic!("start exact segmented EOF build: {}", failure.error),
            },
        );
        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
        assert_eq!(target_delivery.ack.source_version, target_source_version);
        assert!(
            target_delivery.offer.transferred_record_count <= MAXIMUM_TRANSFERRED_RECORDS,
            "one final-block edit transferred {} of {} records",
            target_delivery.offer.transferred_record_count,
            target_delivery.offer.target_record_count
        );
        assert!(
            target_delivery.offer.transferred_record_count
                < target_delivery.offer.target_record_count
        );
        let block_replacement_records = target_delivery
            .packet_frames
            .iter()
            .flatten()
            .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::BlockSequenceReplacementPage)
            .map(|(_, records)| *records)
            .sum::<u32>();
        assert!(
            block_replacement_records > 0
                && block_replacement_records <= MAXIMUM_TRANSFERRED_RECORDS,
            "the final-block splice must publish a bounded nonempty replacement"
        );

        for ordinal in [0, PARAGRAPHS / 2, PARAGRAPHS - 1] {
            let marker = format!("paragraph {ordinal:04} ");
            let paragraph_start = target_source
                .find(&marker)
                .expect("target Paragraph marker");
            let paragraph_end = target_source[paragraph_start..]
                .find('\n')
                .map(|offset| paragraph_start + offset + 1)
                .expect("target Paragraph line ending");
            let point = paragraph_start + marker.len();
            let mut output = [0_u8; HOST_M11_VIEWPORT_BYTES];
            let outcome = host
                .query_structural(
                    HostPointQuery {
                        source_version: target_source_version,
                        position: HostSourceMetric {
                            bytes: u32::try_from(point).expect("Paragraph point byte"),
                            utf16: u32::try_from(point).expect("ASCII Paragraph point UTF-16"),
                        },
                        affinity: HostMetricAffinity::Downstream,
                        budget: HostQueryBudget {
                            maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                            maximum_open_depth: 64,
                            maximum_leaf_count: 64,
                            maximum_tree_nodes_visited: 256,
                        },
                    },
                    &mut output,
                )
                .expect("query installed EOF target");
            let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
                panic!("installed target must expose Paragraph {ordinal}: {outcome:?}");
            };
            assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
            assert_eq!(range.start.bytes as usize, paragraph_start);
            assert_eq!(range.start.utf16 as usize, paragraph_start);
            assert_eq!(range.end.bytes as usize, paragraph_end);
            assert_eq!(range.end.utf16 as usize, paragraph_end);
        }

        let retained = endpoint.retained.as_ref().expect("retained EOF target");
        let CandidateRestartAuthority::Ordinary(checkpoints) =
            retained.restart.as_ref().expect("target restart authority")
        else {
            panic!("segmented EOF crop must retain ordinary target checkpoints");
        };
        assert_eq!(checkpoints.source(), target_version);
        assert!(checkpoints.is_segmented_top_level());
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let next_edit = target_source
            .find("paragraph 2048 ")
            .expect("next-edit Paragraph")
            + "paragraph 2048 ".len();
        let next_version = runtime
            .apply_edit(target_version, next_edit..next_edit + 1, "Q")
            .expect("apply next exact edit")
            .source()
            .current();
        let next_plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan next exact edit");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &next_plan)
                .expect("preflight next exact edit")
        );
        let next_witness = complete_incremental_source_facts(&mut runtime);
        let next_completion = completion_for_persistent_target(&runtime, 3, 2);
        host.observe_source_version(source_version_for(binding, next_completion))
            .expect("host observes next target");
        endpoint
            .start_incremental(
                &runtime,
                runtime
                    .snapshot_current_source()
                    .expect("borrow next target"),
                next_witness,
                binding,
                next_completion,
            )
            .expect("start next exact edit");
        let next_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(next_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(next_delivery.offer.base_ack, Some(target_delivery.ack));
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, next_version)
                .expect("next-revision exact base")
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn mixed_atx_and_fence_edits_crop_locally_while_unclosed_fence_falls_back() {
        const PARAGRAPHS: usize = 4_096;
        const VIEWPORT_HEADER_BYTES: usize = 20;

        let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [921, 922, 923, 924],
            source_session_identity: 925,
            worker_generation: 1,
        };
        let mut base_source = String::new();
        for ordinal in 0..PARAGRAPHS {
            base_source.push_str(&format!(
                "paragraph {ordinal:04} {}\ncontinuation {ordinal:04} {}\n\n",
                "a".repeat(64),
                "b".repeat(64),
            ));
            if ordinal == PARAGRAPHS / 2 - 1 {
                base_source.push_str(concat!(
                    "## mixed **heading**\n\n",
                    "```dart\nlet value = 1;\n```\n\n",
                    "    indented value = 1;\n",
                    "    indented continuation = 2;\n\n",
                    "> quoted value = 1\n",
                    "> quoted continuation = 2\n\n",
                ));
            }
        }
        base_source.push_str("> terminal quote value = 1\n> terminal quote continuation = 2\n");

        let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
            .expect("mixed-block runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start mixed-block base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("mixed-block independent host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes mixed-block base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

        let heading_edit = base_source.find("heading").expect("heading content");
        let heading_version = runtime
            .apply_edit(base_version, heading_edit..heading_edit + 1, "H")
            .expect("edit ATX content")
            .source()
            .current();
        let heading_plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan ATX SourceFacts replacement");
        assert_eq!(
            heading_plan.exact_parser_base_byte_range(),
            Some(&(heading_edit..heading_edit + 1))
        );
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &heading_plan)
                .expect("preflight ATX crop"),
            "an interior ATX edit must use the surrounding authenticated Paragraph checkpoints"
        );
        let heading_witness = complete_incremental_source_facts(&mut runtime);
        let heading_completion = completion_for_persistent_target(&runtime, 2, 1);
        host.observe_source_version(source_version_for(binding, heading_completion))
            .expect("host observes ATX target");
        endpoint
            .start_incremental(
                &runtime,
                runtime
                    .snapshot_current_source()
                    .expect("borrow exact ATX target"),
                heading_witness,
                binding,
                heading_completion,
            )
            .expect("start bounded ATX crop");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingOrdinaryExact"
        );
        let heading_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(heading_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(heading_delivery.offer.base_ack, Some(base_delivery.ack));
        assert!(
            heading_delivery.offer.transferred_record_count
                < heading_delivery.offer.target_record_count
        );
        let heading_source_version = source_version_for(binding, heading_completion);
        let mut heading_output = [0_u8; HOST_M11_VIEWPORT_BYTES];
        let heading_outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: heading_source_version,
                    position: HostSourceMetric {
                        bytes: u32::try_from(heading_edit + 1).expect("heading point"),
                        utf16: u32::try_from(heading_edit + 1).expect("ASCII heading point"),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                        maximum_open_depth: 64,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut heading_output,
            )
            .expect("query cropped ATX target");
        assert!(matches!(
            heading_outcome,
            HostStructuralQueryOutcome::Viewport { .. }
        ));
        assert_eq!(
            heading_output[VIEWPORT_HEADER_BYTES + 12],
            4,
            "independent host must retain the ATX structured kind"
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let fence_edit = base_source.find("value = 1").expect("fence body") + "value = ".len();
        let fence_version = runtime
            .apply_edit(heading_version, fence_edit..fence_edit + 1, "2")
            .expect("edit fenced-code body")
            .source()
            .current();
        let fence_plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan fenced-code SourceFacts replacement");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &fence_plan)
                .expect("preflight fenced-code crop"),
            "an interior fence-body edit must reuse the same mixed-block crop path"
        );
        let fence_witness = complete_incremental_source_facts(&mut runtime);
        let fence_completion = completion_for_persistent_target(&runtime, 3, 2);
        host.observe_source_version(source_version_for(binding, fence_completion))
            .expect("host observes fenced-code target");
        endpoint
            .start_incremental(
                &runtime,
                runtime
                    .snapshot_current_source()
                    .expect("borrow exact fenced-code target"),
                fence_witness,
                binding,
                fence_completion,
            )
            .expect("start bounded fenced-code crop");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingOrdinaryExact"
        );
        let fence_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(fence_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(fence_delivery.offer.base_ack, Some(heading_delivery.ack));
        let fence_source_version = source_version_for(binding, fence_completion);
        let mut fence_output = [0_u8; HOST_M11_VIEWPORT_BYTES];
        let fence_outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: fence_source_version,
                    position: HostSourceMetric {
                        bytes: u32::try_from(fence_edit).expect("fence point"),
                        utf16: u32::try_from(fence_edit).expect("ASCII fence point"),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                        maximum_open_depth: 64,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut fence_output,
            )
            .expect("query cropped fenced-code target");
        assert!(matches!(
            fence_outcome,
            HostStructuralQueryOutcome::Viewport { .. }
        ));
        assert_eq!(
            fence_output[VIEWPORT_HEADER_BYTES + 12],
            3,
            "independent host must retain the fenced-code structured kind"
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let indented_edit = base_source
            .find("indented value = 1")
            .expect("indented-code body")
            + "indented value = ".len();
        let indented_version = runtime
            .apply_edit(fence_version, indented_edit..indented_edit + 1, "2")
            .expect("edit indented-code body")
            .source()
            .current();
        let indented_plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan indented-code SourceFacts replacement");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &indented_plan)
                .expect("preflight indented-code crop"),
            "an interior indented-code edit must reuse the same mixed-block crop path"
        );
        let indented_witness = complete_incremental_source_facts(&mut runtime);
        let indented_completion = completion_for_persistent_target(&runtime, 4, 3);
        host.observe_source_version(source_version_for(binding, indented_completion))
            .expect("host observes indented-code target");
        endpoint
            .start_incremental(
                &runtime,
                runtime
                    .snapshot_current_source()
                    .expect("borrow exact indented-code target"),
                indented_witness,
                binding,
                indented_completion,
            )
            .expect("start bounded indented-code crop");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingOrdinaryExact"
        );
        let indented_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(
            indented_delivery.offer.mode,
            PublicationMode::ExactBaseDelta
        );
        assert_eq!(indented_delivery.offer.base_ack, Some(fence_delivery.ack));
        assert!(
            indented_delivery.offer.transferred_record_count
                < indented_delivery.offer.target_record_count / 4,
            "the middle indented-code delta must retain the large authenticated prefix and suffix"
        );
        let indented_source_version = source_version_for(binding, indented_completion);
        let mut indented_output = [0_u8; HOST_M11_VIEWPORT_BYTES];
        let indented_outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: indented_source_version,
                    position: HostSourceMetric {
                        bytes: u32::try_from(indented_edit).expect("indented-code point"),
                        utf16: u32::try_from(indented_edit).expect("ASCII indented-code point"),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                        maximum_open_depth: 64,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut indented_output,
            )
            .expect("query cropped indented-code target");
        assert!(matches!(
            indented_outcome,
            HostStructuralQueryOutcome::Viewport { .. }
        ));
        assert_eq!(
            indented_output[VIEWPORT_HEADER_BYTES + 12],
            7,
            "independent host must retain the indented-code structured kind"
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let quote_edit = base_source
            .find("quoted continuation = 2")
            .expect("multiline block-quote body")
            + "quoted continuation = ".len();
        let quote_version = runtime
            .apply_edit(indented_version, quote_edit..quote_edit + 1, "3")
            .expect("edit multiline block-quote body")
            .source()
            .current();
        let quote_plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan block-quote SourceFacts replacement");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &quote_plan)
                .expect("preflight block-quote crop"),
            "an interior exact single-Paragraph block-quote edit must reuse the mixed-block crop path"
        );
        let quote_witness = complete_incremental_source_facts(&mut runtime);
        let quote_completion = completion_for_persistent_target(&runtime, 5, 4);
        host.observe_source_version(source_version_for(binding, quote_completion))
            .expect("host observes block-quote target");
        endpoint
            .start_incremental(
                &runtime,
                runtime
                    .snapshot_current_source()
                    .expect("borrow exact block-quote target"),
                quote_witness,
                binding,
                quote_completion,
            )
            .expect("start bounded block-quote crop");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingOrdinaryExact"
        );
        let quote_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(quote_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(quote_delivery.offer.base_ack, Some(indented_delivery.ack));
        assert!(
            quote_delivery.offer.transferred_record_count
                < quote_delivery.offer.target_record_count / 4,
            "the middle block-quote delta must retain the large authenticated prefix and suffix"
        );
        let quote_source_version = source_version_for(binding, quote_completion);
        let mut quote_output = [0_u8; HOST_M11_VIEWPORT_BYTES];
        let quote_outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: quote_source_version,
                    position: HostSourceMetric {
                        bytes: u32::try_from(quote_edit).expect("block-quote point"),
                        utf16: u32::try_from(quote_edit).expect("ASCII block-quote point"),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                        maximum_open_depth: 64,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut quote_output,
            )
            .expect("query cropped block-quote target");
        assert!(matches!(
            quote_outcome,
            HostStructuralQueryOutcome::Viewport { .. }
        ));
        assert_eq!(
            quote_output[VIEWPORT_HEADER_BYTES + 12],
            8,
            "independent host must retain the block-quote structured kind"
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let terminal_quote_edit = base_source
            .rfind("terminal quote continuation = 2")
            .expect("terminal multiline block-quote body")
            + "terminal quote continuation = ".len();
        let terminal_quote_version = runtime
            .apply_edit(
                quote_version,
                terminal_quote_edit..terminal_quote_edit + 1,
                "3",
            )
            .expect("edit terminal multiline block-quote body")
            .source()
            .current();
        let terminal_quote_plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan terminal block-quote SourceFacts replacement");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &terminal_quote_plan)
                .expect("preflight terminal block-quote crop"),
            "a terminal exact single-Paragraph block quote must select authenticated EOF authority"
        );
        let terminal_quote_witness = complete_incremental_source_facts(&mut runtime);
        let terminal_quote_completion = completion_for_persistent_target(&runtime, 6, 5);
        host.observe_source_version(source_version_for(binding, terminal_quote_completion))
            .expect("host observes terminal block-quote target");
        endpoint
            .start_incremental(
                &runtime,
                runtime
                    .snapshot_current_source()
                    .expect("borrow exact terminal block-quote target"),
                terminal_quote_witness,
                binding,
                terminal_quote_completion,
            )
            .expect("start bounded terminal block-quote crop");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingOrdinaryExact"
        );
        let terminal_quote_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(
            terminal_quote_delivery.offer.mode,
            PublicationMode::ExactBaseDelta
        );
        assert_eq!(
            terminal_quote_delivery.offer.base_ack,
            Some(quote_delivery.ack)
        );
        assert!(
            terminal_quote_delivery.offer.transferred_record_count
                < terminal_quote_delivery.offer.target_record_count,
            "the terminal block-quote delta must retain its authenticated prefix"
        );
        let terminal_quote_source_version = source_version_for(binding, terminal_quote_completion);
        let mut terminal_quote_output = [0_u8; HOST_M11_VIEWPORT_BYTES];
        let terminal_quote_outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: terminal_quote_source_version,
                    position: HostSourceMetric {
                        bytes: u32::try_from(terminal_quote_edit)
                            .expect("terminal block-quote point"),
                        utf16: u32::try_from(terminal_quote_edit)
                            .expect("ASCII terminal block-quote point"),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                        maximum_open_depth: 64,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut terminal_quote_output,
            )
            .expect("query cropped terminal block-quote target");
        assert!(matches!(
            terminal_quote_outcome,
            HostStructuralQueryOutcome::Viewport { .. }
        ));
        assert_eq!(
            terminal_quote_output[VIEWPORT_HEADER_BYTES + 12],
            8,
            "independent host must retain the terminal block-quote structured kind"
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let terminal_nested_quote_edit = base_source
            .rfind("> terminal quote value = 1")
            .expect("terminal block-quote opener")
            + 1;
        let unsupported_quote_version = runtime
            .apply_edit(
                terminal_quote_version,
                terminal_nested_quote_edit..terminal_nested_quote_edit + 1,
                ">",
            )
            .expect("make the terminal quote nested")
            .source()
            .current();
        let unsupported_quote_plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan unsupported terminal block-quote replacement");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &unsupported_quote_plan)
                .expect("preflight unsupported terminal block-quote crop"),
            "preflight may begin from the exact quote before target semantics are known"
        );
        let unsupported_quote_witness = complete_incremental_source_facts(&mut runtime);
        let unsupported_quote_completion = completion_for_persistent_target(&runtime, 7, 6);
        host.observe_source_version(source_version_for(binding, unsupported_quote_completion))
            .expect("host observes unsupported terminal block-quote target");
        endpoint
            .start_incremental(
                &runtime,
                runtime
                    .snapshot_current_source()
                    .expect("borrow unsupported terminal block-quote target"),
                unsupported_quote_witness,
                binding,
                unsupported_quote_completion,
            )
            .expect("start unsupported terminal block-quote crop");
        let unsupported_quote_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(
            unsupported_quote_delivery.offer.mode,
            PublicationMode::FullSnapshot,
            "a nested quote at EOF must fail closed instead of publishing an exact local splice"
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        let closing_fence = base_source
            .find("\n```\n\n")
            .map(|offset| offset + 1)
            .expect("closing fence");
        runtime
            .apply_edit(
                unsupported_quote_version,
                closing_fence..closing_fence + 3,
                "",
            )
            .expect("make the middle fence consume its former convergence suffix");
        let divergent_plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan divergent fence SourceFacts replacement");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &divergent_plan)
                .expect("preflight divergent fence crop"),
            "preflight may start from exact authority before semantics are known"
        );
        let divergent_witness = complete_incremental_source_facts(&mut runtime);
        let divergent_completion = completion_for_persistent_target(&runtime, 8, 7);
        host.observe_source_version(source_version_for(binding, divergent_completion))
            .expect("host observes divergent fence target");
        endpoint
            .start_incremental(
                &runtime,
                runtime
                    .snapshot_current_source()
                    .expect("borrow divergent fence target"),
                divergent_witness,
                binding,
                divergent_completion,
            )
            .expect("start divergent fence crop");
        let divergent_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(
            divergent_delivery.offer.mode,
            PublicationMode::FullSnapshot,
            "an unclosed fence that crosses convergence must fail closed into a definitive parse"
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn segmented_over_cap_restart_falls_back_and_reuses_packed_block_pages() {
        let profile = SourceFactsScanProfile::new(64).expect("bounded test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [716, 717, 718, 719],
            source_session_identity: 720,
            worker_generation: 1,
        };
        let base_source: String = (0..4_096)
            .map(|ordinal| format!("paragraph {ordinal:04} {}\n\n", "a".repeat(32)))
            .collect();
        let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
            .expect("segmented exact-base runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start segmented clean base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent segmented host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes segmented base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert!(matches!(
            endpoint
                .retained
                .as_ref()
                .and_then(|retained| retained.restart.as_ref()),
            Some(CandidateRestartAuthority::Ordinary(checkpoints))
                if checkpoints.source() == base_version && checkpoints.is_segmented_top_level()
        ));

        let edit_start = base_source
            .find("paragraph 2048 ")
            .expect("middle segmented Paragraph")
            + "paragraph 2048 ".len()
            + 12;
        let oversized_replacement = "Z".repeat(M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES + 1_024);
        let target_version = runtime
            .apply_edit(
                base_version,
                edit_start..edit_start + 1,
                &oversized_replacement,
            )
            .expect("make the mapped restart window exceed its hard cap")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan segmented SourceFacts replacement");
        assert!(
            plan.base_byte_range().start > 0
                && plan.base_byte_range().end < base_version.byte_len(),
            "fixture must retain packed block pages on both sides"
        );
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight exact-clean fallback"),
            "the exact base remains eligible before target-window cap evaluation"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("borrow segmented target");
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        host.observe_source_version(source_version_for(binding, target_completion))
            .expect("host observes segmented target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start typed over-cap exact-clean fallback");
        assert!(
            matches!(
                endpoint.active,
                Some(ActiveCandidate::ParsingExactFallback(_))
            ),
            "an over-cap local restart must enter the definitive clean parser, not fault"
        );
        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );

        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
        let replacement_pages = target_delivery
            .packet_frames
            .iter()
            .flatten()
            .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::BlockSequenceReplacementPage)
            .map(|(_, records)| usize::try_from(*records).expect("record count"))
            .sum::<usize>();
        assert!(
            (1..16).contains(&replacement_pages),
            "one local edit should transfer only boundary-local packed pages, got \
             {replacement_pages}"
        );
        assert!(
            target_delivery.offer.transferred_record_count
                < target_delivery.offer.target_record_count / 4,
            "exact target must omit the large retained block/source-fact majority"
        );
        let target_source_version = source_version_for(binding, target_completion);
        let mut output = vec![0_u8; 4096];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: target_source_version,
                    position: HostSourceMetric {
                        bytes: u32::try_from(edit_start).expect("test edit byte"),
                        utf16: u32::try_from(edit_start).expect("ASCII test edit UTF-16"),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: output.len() as u32,
                        maximum_open_depth: 64,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut output,
            )
            .expect("query independently replayed target");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("replayed target must expose the edited Paragraph: {outcome:?}");
        };
        assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
        assert_eq!(
            u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
            1,
            "persistent block reuse must not manufacture inline authority"
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, target_version)
                .expect("segmented target exact-base continuity")
        );
        assert!(matches!(
            endpoint
                .retained
                .as_ref()
                .and_then(|retained| retained.restart.as_ref()),
            Some(CandidateRestartAuthority::Ordinary(checkpoints))
                if checkpoints.source() == target_version && checkpoints.is_segmented_top_level()
        ));

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn tight_bullet_item_edit_uses_authenticated_block_delta() {
        let profile = SourceFactsScanProfile::new(64).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [716, 717, 718, 719],
            source_session_identity: 720,
            worker_generation: 1,
        };
        let mut base_source = String::new();
        let mut list_start = 0;
        for ordinal in 0..512 {
            use std::fmt::Write as _;
            writeln!(
                &mut base_source,
                "paragraph {ordinal:04} {}\n",
                "a".repeat(32)
            )
            .expect("paragraph fixture write");
            if ordinal == 255 {
                list_start = base_source.len();
                base_source.push_str("  - α😀 first\r\n  - beta second\r\n\r\n");
            }
        }
        let edit_start = base_source
            .find("beta second")
            .expect("selected Bullet List item");
        let edit_start_utf16 = base_source[..edit_start].encode_utf16().count();

        let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
            .expect("Bullet List exact-delta runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start clean Bullet List base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent Bullet List host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes Bullet List base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

        let target_version = runtime
            .apply_edit(base_version, edit_start..edit_start + 1, "β")
            .expect("Unicode Bullet List item edit")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan Bullet List SourceFacts replacement");
        assert_eq!(
            plan.exact_parser_base_byte_range(),
            Some(&(edit_start..edit_start + 1)),
        );
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight Bullet List crop"),
            "ordinary Paragraph checkpoints must bracket the changed list"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("borrow Bullet List target");
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        let target_source_version = source_version_for(binding, target_completion);
        host.observe_source_version(target_source_version)
            .expect("host observes Bullet List target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start authenticated Bullet List crop");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingOrdinaryExact",
        );
        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );

        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
        assert!(
            target_delivery.offer.transferred_record_count <= 64,
            "one list-item edit transferred {} records",
            target_delivery.offer.transferred_record_count,
        );
        assert!(
            target_delivery.offer.transferred_record_count
                < target_delivery.offer.target_record_count,
            "the target must retain document records outside the changed list"
        );
        assert!(
            target_delivery
                .packet_frames
                .iter()
                .flatten()
                .any(|(kind, _)| *kind == CandidateSnapshotFrameKind::BlockSequenceReplacementPage)
        );

        let mut output = vec![0_u8; 4096];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: target_source_version,
                    position: HostSourceMetric {
                        bytes: u32::try_from(edit_start).expect("edit byte"),
                        utf16: u32::try_from(edit_start_utf16).expect("edit UTF-16"),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: output.len() as u32,
                        maximum_open_depth: 64,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut output,
            )
            .expect("query independently replayed Bullet List");
        let HostStructuralQueryOutcome::Viewport { range, .. } = outcome else {
            panic!("exact-delta target must expose the edited Bullet List: {outcome:?}");
        };
        assert_eq!(range.start.bytes as usize, list_start);
        assert!(range.end.bytes as usize > edit_start);
        assert_eq!(
            u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
            1,
        );

        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, target_version)
                .expect("Bullet List target exact-base continuity")
        );
        assert!(matches!(
            endpoint
                .retained
                .as_ref()
                .and_then(|retained| retained.restart.as_ref()),
            Some(CandidateRestartAuthority::Ordinary(checkpoints))
                if checkpoints.source() == target_version && checkpoints.is_segmented_top_level()
        ));

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn ordinary_paragraph_eof_edit_streams_delta_then_semantic_split_falls_back() {
        let profile = SourceFactsScanProfile::new(2).expect("dense test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [721, 722, 723, 724],
            source_session_identity: 725,
            worker_generation: 1,
        };
        let base_source: String = (0..1_024)
            .map(|ordinal| format!("ordinary prose line {ordinal:04} {}\n", "a".repeat(40)))
            .collect();
        let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
            .expect("ordinary EOF runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start clean ordinary base candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent candidate host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes ordinary base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

        let edit_start = base_source
            .find("ordinary prose line 1018 ")
            .expect("tail line");
        let replacement: String = (0..10)
            .map(|ordinal| format!("replacement tail {ordinal:02} 世界😀 {}\n", "b".repeat(40)))
            .collect();
        let first_target_source = format!("{}{}", &base_source[..edit_start], replacement);
        let target_version = runtime
            .apply_edit(base_version, edit_start..base_source.len(), &replacement)
            .expect("EOF ordinary edit")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan EOF SourceFacts replacement");
        assert!(
            plan.base_byte_range().start > 0
                && plan.base_byte_range().end == base_version.byte_len(),
            "fixture must leave only exact parser prefix authority"
        );
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight EOF crop"),
            "EOF edit must have an authenticated restart"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("borrow exact EOF target");
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        host.observe_source_version(source_version_for(binding, target_completion))
            .expect("host observes exact EOF target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start authenticated EOF crop");
        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );

        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
        assert!(
            target_delivery.offer.transferred_record_count
                < target_delivery.offer.target_record_count,
            "exact EOF delta must omit authenticated reused records"
        );
        assert_installed_candidate_has_no_inline(
            &host,
            source_version_for(binding, target_completion),
        );
        let retained = endpoint.retained.as_ref().expect("retained EOF target");
        let CandidateRestartAuthority::Ordinary(checkpoints) =
            retained.restart.as_ref().expect("EOF target authority")
        else {
            panic!("EOF crop must retain ordinary target checkpoints");
        };
        assert_eq!(checkpoints.source(), target_version);
        assert!(checkpoints.len() > 2);
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, target_version)
                .expect("next EOF exact base")
        );

        let second_paragraph_start = first_target_source.len() + 1;
        let second_paragraph_start_utf16 = first_target_source.encode_utf16().count() + 1;
        let split_target_version = runtime
            .apply_edit(
                target_version,
                first_target_source.len()..first_target_source.len(),
                "\nsecond paragraph\n",
            )
            .expect("turn the EOF Paragraph into a segmented target")
            .source()
            .current();
        let split_plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan EOF semantic split");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &split_plan)
                .expect("preflight EOF semantic split"),
            "the semantic split must initially select the authenticated EOF crop"
        );
        let split_witness = complete_incremental_source_facts(&mut runtime);
        let split_lease = runtime
            .snapshot_current_source()
            .expect("borrow segmented EOF target");
        let split_completion = completion_for_persistent_target(&runtime, 3, 2);
        let split_source_version = source_version_for(binding, split_completion);
        host.observe_source_version(split_source_version)
            .expect("host observes segmented EOF target");
        endpoint
            .start_incremental(
                &runtime,
                split_lease,
                split_witness,
                binding,
                split_completion,
            )
            .expect("start EOF crop before semantic decline");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingOrdinaryExact"
        );
        let split_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );

        assert_eq!(split_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(split_delivery.offer.base_ack, None);
        assert_eq!(
            split_delivery.offer.transferred_record_count,
            split_delivery.offer.target_record_count
        );
        let mut output = [0_u8; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version: split_source_version,
                    position: HostSourceMetric {
                        bytes: u32::try_from(second_paragraph_start + 1)
                            .expect("second Paragraph byte"),
                        utf16: u32::try_from(second_paragraph_start_utf16 + 1)
                            .expect("second Paragraph UTF-16"),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                        maximum_open_depth: 64,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut output,
            )
            .expect("query clean-fallback second Paragraph");
        let HostStructuralQueryOutcome::Viewport { range, .. } = outcome else {
            panic!("clean fallback must install the second Paragraph: {outcome:?}");
        };
        assert_eq!(range.start.bytes as usize, second_paragraph_start);
        assert_eq!(
            u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
            1
        );
        let retained = endpoint
            .retained
            .as_ref()
            .expect("retained segmented EOF target");
        let Some(CandidateRestartAuthority::Ordinary(checkpoints)) = retained.restart.as_ref()
        else {
            panic!("clean EOF fallback must retain segmented restart authority");
        };
        assert_eq!(checkpoints.source(), split_target_version);
        assert!(checkpoints.is_segmented_top_level());
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, split_target_version)
                .expect("segmented EOF target exact-base continuity")
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn ordinary_crop_blank_boundary_falls_back_to_fresh_full_snapshot() {
        let profile = SourceFactsScanProfile::new(2).expect("dense test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [731, 732, 733, 734],
            source_session_identity: 735,
            worker_generation: 1,
        };
        let base_source: String = (0..1_024)
            .map(|ordinal| format!("ordinary prose line {ordinal:04} {}\n", "a".repeat(40)))
            .collect();
        let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
            .expect("ordinary fallback runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start clean ordinary base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent fallback host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes ordinary base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

        let blank_start = base_source
            .find("ordinary prose line 0512 ")
            .expect("middle ordinary line");
        let blank_end = base_source[blank_start..]
            .find('\n')
            .map(|offset| blank_start + offset)
            .expect("middle line ending");
        let target_version = runtime
            .apply_edit(base_version, blank_start..blank_end, "")
            .expect("insert semantic blank boundary")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan blank-boundary SourceFacts");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight ordinary crop"),
            "fixture must initially select the bounded ordinary crop"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("borrow blank-boundary target");
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        host.observe_source_version(source_version_for(binding, target_completion))
            .expect("host observes blank-boundary target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start bounded crop before semantic decline");
        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );

        assert_eq!(target_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(target_delivery.offer.base_ack, None);
        assert_eq!(
            target_delivery.offer.transferred_record_count,
            target_delivery.offer.target_record_count
        );
        assert!(
            !target_delivery
                .packet_frames
                .iter()
                .flatten()
                .any(|(kind, _)| *kind == CandidateSnapshotFrameKind::SourceFactsReplacementPage)
        );
        let retained = endpoint
            .retained
            .as_ref()
            .expect("retained fallback target");
        assert_eq!(
            retained
                .publication
                .descriptor(&runtime)
                .expect("fallback descriptor")
                .source_revision,
            target_version.revision().get()
        );
        let Some(CandidateRestartAuthority::Ordinary(checkpoints)) = retained.restart.as_ref()
        else {
            panic!("clean fallback must retain the segmented target restart authority");
        };
        assert_eq!(checkpoints.source(), target_version);
        assert!(checkpoints.is_segmented_top_level());
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, target_version)
                .expect("segmented target eligibility"),
            "the segmented clean target remains eligible for exact-base discovery"
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn leading_crop_new_definition_falls_back_with_fresh_references() {
        const BASE_SOURCE: &str = "[base]: /base\n!x]: /new\nvisible\n";

        let profile = SourceFactsScanProfile::new(2).expect("dense test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [741, 742, 743, 744],
            source_session_identity: 745,
            worker_generation: 1,
        };
        let mut runtime = DocumentRuntime::new(BASE_SOURCE, standard_document_runtime_config())
            .expect("leading fallback runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start clean leading base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent leading fallback host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes leading base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::References)
                .expect("base References"),
            1
        );

        let edit_start = BASE_SOURCE.find('!').expect("definition edit marker");
        let target_version = runtime
            .apply_edit(base_version, edit_start..edit_start + 1, "[")
            .expect("turn paragraph line into a new definition")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan new-definition SourceFacts");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight leading crop"),
            "fixture must initially select the retained leading restart"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("borrow new-definition target");
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        host.observe_source_version(source_version_for(binding, target_completion))
            .expect("host observes new-definition target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start leading crop before semantic decline");
        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );

        assert_eq!(target_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(target_delivery.offer.base_ack, None);
        assert_eq!(
            target_delivery.offer.transferred_record_count,
            target_delivery.offer.target_record_count
        );
        assert!(
            target_delivery
                .packet_frames
                .iter()
                .flatten()
                .all(|(kind, _)| *kind != CandidateSnapshotFrameKind::SourceFactsReplacementPage)
        );
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::References)
                .expect("fresh target References"),
            2,
            "fallback must rebuild References from the definitive target parse"
        );
        let retained = endpoint.retained.as_ref().expect("retained target base");
        let CandidateRestartAuthority::Leading(restart) =
            retained.restart.as_ref().expect("supported target restart")
        else {
            panic!("fresh target must retain leading-reference authority");
        };
        assert_eq!(restart.source(), target_version);
        assert_eq!(
            restart.definition_count(),
            2,
            "fallback must install fresh target checkpoint semantics"
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, target_version)
                .expect("next target exact base")
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn length_changing_edit_before_late_definition_rebuilds_reference_coordinates() {
        const PARAGRAPHS: usize = 8_192;

        let mut base_source = String::new();
        for ordinal in 0..PARAGRAPHS {
            use std::fmt::Write as _;
            writeln!(
                &mut base_source,
                "Paragraph {ordinal:04} stays definition free.\n"
            )
            .expect("late-definition fixture write");
        }
        base_source.push_str("[late]: /target\n");

        let profile = SourceFactsScanProfile::new(64).expect("dense coordinate-shift profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [746, 747, 748, 749],
            source_session_identity: 750,
            worker_generation: 1,
        };
        let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
            .expect("late-definition runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start late-definition base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent late-definition host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes late-definition base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::References)
                .expect("base References"),
            1
        );
        assert!(matches!(
            endpoint
                .retained
                .as_ref()
                .and_then(|retained| retained.restart.as_ref()),
            Some(CandidateRestartAuthority::ExactBaseOnly { .. })
        ));

        let edit_start = base_source
            .find("Paragraph 0100")
            .expect("early Paragraph edit");
        let equal_length_version = runtime
            .apply_edit(base_version, edit_start..edit_start + 1, "p")
            .expect("equal-length early edit")
            .source()
            .current();
        let equal_length_plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan equal-length SourceFacts");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &equal_length_plan)
                .expect("preflight equal-length exact base"),
            "the fixture must exercise exact-base clean discovery"
        );
        let equal_length_witness = complete_incremental_source_facts(&mut runtime);
        let equal_length_lease = runtime
            .snapshot_current_source()
            .expect("borrow equal-length target");
        let equal_length_completion = completion_for_persistent_target(&runtime, 2, 1);
        host.observe_source_version(source_version_for(binding, equal_length_completion))
            .expect("host observes equal-length target");
        endpoint
            .start_incremental(
                &runtime,
                equal_length_lease,
                equal_length_witness,
                binding,
                equal_length_completion,
            )
            .expect("start equal-length exact-base edit");
        let equal_length_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(
            equal_length_delivery.offer.mode,
            PublicationMode::ExactBaseDelta,
            "unchanged absolute coordinates remain eligible for reference-root reuse"
        );
        assert_eq!(
            equal_length_delivery.offer.base_ack,
            Some(base_delivery.ack)
        );
        assert!(
            equal_length_delivery.offer.transferred_record_count
                < equal_length_delivery.offer.target_record_count
        );
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::References)
                .expect("reused equal-length References"),
            1
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, equal_length_version)
                .expect("equal-length exact-base continuity")
        );

        let target_version = runtime
            .apply_edit(
                equal_length_version,
                edit_start..edit_start + 1,
                "Expanded p",
            )
            .expect("length-changing early edit")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan late-definition SourceFacts");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight exact base"),
            "the fixture must exercise exact-base clean discovery"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("borrow late-definition target");
        let target_completion = completion_for_persistent_target(&runtime, 3, 2);
        host.observe_source_version(source_version_for(binding, target_completion))
            .expect("host observes late-definition target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start exact-base late-definition edit");
        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );

        assert_eq!(
            target_delivery.offer.mode,
            PublicationMode::FullSnapshot,
            "a reused reference record would retain its old absolute byte/UTF-16 ranges"
        );
        assert_eq!(target_delivery.offer.base_ack, None);
        assert_eq!(
            target_delivery.offer.transferred_record_count,
            target_delivery.offer.target_record_count
        );
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::References)
                .expect("rebuilt target References"),
            1
        );
        assert_eq!(
            endpoint
                .retained
                .as_ref()
                .and_then(|retained| retained.restart.as_ref())
                .expect("target exact-base authority")
                .source(),
            target_version
        );

        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    fn leading_references_use_the_production_exact_delta_path<
        const REFERENCES: usize,
        const FUEL: usize,
    >() {
        let mut base_source = String::new();
        base_source.reserve(REFERENCES * 24);
        for ordinal in 0..REFERENCES {
            use std::fmt::Write as _;
            writeln!(&mut base_source, "[ref-{ordinal}]: /target-{ordinal}")
                .expect("reference fixture write");
        }
        let tail_start = base_source.len();
        base_source.push_str("live **tail** stays editable\n");

        let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [751, 752, 753, 754],
            source_session_identity: 755,
            worker_generation: 1,
        };
        let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
            .expect("large-reference exact-delta runtime");

        let cold_started = std::time::Instant::now();
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start large-reference base candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent large-reference host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes large-reference base");
        let base_delivery = deliver_endpoint_to_independent_host_with_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            FUEL,
        );
        let cold_elapsed = cold_started.elapsed();
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::References)
                .expect("base References"),
            REFERENCES as u64
        );
        let retained = endpoint.retained.as_ref().expect("retained base");
        let CandidateRestartAuthority::Leading(restart) =
            retained.restart.as_ref().expect("leading restart")
        else {
            panic!("large-reference base must retain leading-reference authority");
        };
        assert_eq!(restart.definition_count(), REFERENCES);

        let edit_start = tail_start
            + base_source[tail_start..]
                .find("tail")
                .expect("editable tail");
        let target_version = runtime
            .apply_edit(base_version, edit_start..edit_start + 1, "T")
            .expect("bounded tail edit")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan large-reference SourceFacts delta");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight large-reference crop"),
            "the unchanged definition prefix must select exact crop authority"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("borrow large-reference target");
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        host.observe_source_version(source_version_for(binding, target_completion))
            .expect("host observes large-reference target");

        let exact_started = std::time::Instant::now();
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start production large-reference exact crop");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingExact",
            "the exact tail edit must enter the local leading-reference crop"
        );
        let target_delivery = deliver_endpoint_to_independent_host_with_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            FUEL,
        );
        let exact_elapsed = exact_started.elapsed();

        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
        let reused_records = target_delivery
            .offer
            .target_record_count
            .checked_sub(target_delivery.offer.transferred_record_count)
            .expect("exact delta cannot transfer more than its target");
        assert!(
            reused_records >= REFERENCES as u32,
            "all canonical reference records must come from the acknowledged base"
        );
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::References)
                .expect("target References"),
            REFERENCES as u64
        );
        assert_eq!(
            endpoint
                .retained
                .as_ref()
                .and_then(|retained| retained.restart.as_ref())
                .expect("next large-reference restart")
                .source(),
            target_version
        );
        eprintln!(
            "m11_{REFERENCES}_reference_exact_delta source_bytes={} cold_ms={} exact_ms={} \
             base_records={} target_records={} transferred_records={} reused_records={} \
             base_packets={} exact_packets={} mode={:?}",
            base_source.len(),
            cold_elapsed.as_millis(),
            exact_elapsed.as_millis(),
            base_delivery.offer.target_record_count,
            target_delivery.offer.target_record_count,
            target_delivery.offer.transferred_record_count,
            reused_records,
            base_delivery.packet_frames.len(),
            target_delivery.packet_frames.len(),
            target_delivery.offer.mode,
        );

        drain_candidate_cleanup_with_fuel(&mut endpoint, &mut runtime, FUEL);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, target_version)
                .expect("next large-reference exact base")
        );
        close_exact_pair_to_zero_with_fuel(&mut endpoint, &mut runtime, &mut host, FUEL);
    }

    #[test]
    fn four_thousand_ninety_six_leading_references_use_the_production_exact_delta_path() {
        leading_references_use_the_production_exact_delta_path::<4_096, 1>();
    }

    #[test]
    fn one_hundred_thousand_leading_references_use_the_production_exact_delta_path() {
        leading_references_use_the_production_exact_delta_path::<100_000, 64>();
    }

    #[test]
    fn frozen_leading_references_allow_bounded_middle_paragraph_exact_delta() {
        const REFERENCES: usize = 2_048;
        const PARAGRAPHS: usize = 2_048;
        const EDITED_PARAGRAPH: usize = PARAGRAPHS / 2;
        const MAXIMUM_TRANSFERRED_RECORDS: u32 = 64;

        let mut base_source = String::new();
        base_source.reserve((REFERENCES + PARAGRAPHS) * 56);
        for ordinal in 0..REFERENCES {
            use std::fmt::Write as _;
            writeln!(&mut base_source, "[ref-{ordinal}]: /target-{ordinal}")
                .expect("reference fixture write");
        }
        let tail_start = base_source.len();
        let mut paragraph_ranges = Vec::with_capacity(PARAGRAPHS);
        for ordinal in 0..PARAGRAPHS {
            let start = base_source.len();
            use std::fmt::Write as _;
            writeln!(
                &mut base_source,
                "tail paragraph {ordinal:04} {}\n",
                "a".repeat(32)
            )
            .expect("Paragraph fixture write");
            let end = base_source.len() - 1;
            paragraph_ranges.push(start..end);
        }
        assert_eq!(
            paragraph_ranges.first().expect("first tail").start,
            tail_start
        );

        let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [761, 762, 763, 764],
            source_session_identity: 765,
            worker_generation: 1,
        };
        let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
            .expect("reference-frozen Paragraph runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start reference-frozen base");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent reference-frozen host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes reference-frozen base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::References)
                .expect("base References"),
            REFERENCES as u64
        );
        assert!(matches!(
            endpoint
                .retained
                .as_ref()
                .and_then(|retained| retained.restart.as_ref()),
            Some(CandidateRestartAuthority::Ordinary(checkpoints))
                if checkpoints.source() == base_version
                    && checkpoints.is_segmented_top_level()
                    && checkpoints.frozen_reference_definition_count() == Some(REFERENCES)
        ));

        let edited_range = paragraph_ranges[EDITED_PARAGRAPH].clone();
        let edit_start = edited_range.start
            + base_source[edited_range.clone()]
                .find("aaaaaaaa")
                .expect("editable Paragraph payload")
            + 4;
        const REPLACEMENT: &str = "expanded";
        let coordinate_delta = REPLACEMENT.len() - 1;
        let target_version = runtime
            .apply_edit(base_version, edit_start..edit_start + 1, REPLACEMENT)
            .expect("length-changing middle tail edit")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan reference-frozen SourceFacts delta");
        assert_eq!(
            plan.exact_parser_base_byte_range(),
            Some(&(edit_start..edit_start + 1)),
            "ordinary parser authority must follow the exact edit, not a storage-page envelope"
        );
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight reference-frozen exact crop"),
            "an unchanged definition prefix must retain ordinary restart and convergence authority"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let target_lease = runtime
            .snapshot_current_source()
            .expect("borrow reference-frozen target");
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        let target_source_version = source_version_for(binding, target_completion);
        host.observe_source_version(target_source_version)
            .expect("host observes reference-frozen target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start reference-frozen ordinary crop");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "ParsingOrdinaryExact",
            "a middle tail Paragraph edit must use ordinary restart/convergence, not the \
             one-remainder leading-reference crop"
        );
        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );

        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
        assert_eq!(target_delivery.ack.source_version, target_source_version);
        assert!(
            target_delivery.offer.transferred_record_count <= MAXIMUM_TRANSFERRED_RECORDS,
            "one local Paragraph edit transferred {} records across {REFERENCES} frozen \
             definitions and {PARAGRAPHS} Paragraphs",
            target_delivery.offer.transferred_record_count
        );
        let reused_records = target_delivery
            .offer
            .target_record_count
            .checked_sub(target_delivery.offer.transferred_record_count)
            .expect("exact delta cannot transfer more than its target");
        assert!(
            reused_records >= REFERENCES as u32,
            "the acknowledged base must supply every frozen reference record"
        );
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::References)
                .expect("retained target References"),
            REFERENCES as u64
        );

        let edited_target_range = edited_range.start..edited_range.end + coordinate_delta;
        let last_base_range = paragraph_ranges[PARAGRAPHS - 1].clone();
        let last_target_range =
            last_base_range.start + coordinate_delta..last_base_range.end + coordinate_delta;
        for (name, paragraph_range, point) in [
            (
                "first tail Paragraph",
                0..paragraph_ranges[0].end,
                paragraph_ranges[0].start + 1,
            ),
            (
                "edited middle tail Paragraph",
                edited_target_range,
                edit_start + 1,
            ),
            (
                "last tail Paragraph",
                last_target_range.clone(),
                last_target_range.start + 1,
            ),
        ] {
            let mut output = [0_u8; HOST_M11_VIEWPORT_BYTES];
            let outcome = host
                .query_structural(
                    HostPointQuery {
                        source_version: target_source_version,
                        position: HostSourceMetric {
                            bytes: u32::try_from(point).expect("query byte"),
                            utf16: u32::try_from(point).expect("ASCII query UTF-16"),
                        },
                        affinity: HostMetricAffinity::Downstream,
                        budget: HostQueryBudget {
                            maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                            maximum_open_depth: 64,
                            maximum_leaf_count: 64,
                            maximum_tree_nodes_visited: 256,
                        },
                    },
                    &mut output,
                )
                .unwrap_or_else(|error| panic!("query {name}: {error}"));
            let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
                panic!("installed target must expose {name}: {outcome:?}");
            };
            assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
            assert_eq!(range.start.bytes as usize, paragraph_range.start, "{name}");
            assert_eq!(range.start.utf16 as usize, paragraph_range.start, "{name}");
            assert_eq!(range.end.bytes as usize, paragraph_range.end, "{name}");
            assert_eq!(range.end.utf16 as usize, paragraph_range.end, "{name}");
        }

        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, target_version)
                .expect("next-edit exact-base authority"),
            "the installed target must remain eligible for the next edit"
        );
        assert!(matches!(
            endpoint
                .retained
                .as_ref()
                .and_then(|retained| retained.restart.as_ref()),
            Some(CandidateRestartAuthority::Ordinary(checkpoints))
                if checkpoints.source() == target_version
                    && checkpoints.is_segmented_top_level()
                    && checkpoints.frozen_reference_definition_count() == Some(REFERENCES)
        ));

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn exact_base_delta_round_trips_at_the_sixteen_frame_replay_boundary() {
        // The crop grammar intentionally requires the visible remainder to
        // follow the leading definitions directly. A blank separator is an
        // explicit `BlankBoundary` and therefore cannot mint a restart.
        const PREFIX: &str = "[ref]: /target\n";
        const TAIL_BYTES: usize = 1_864;

        let profile = SourceFactsScanProfile::new(2).expect("dense test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [701, 702, 703, 704],
            source_session_identity: 705,
            worker_generation: 1,
        };
        let base_source = format!("{PREFIX}{}", "a".repeat(TAIL_BYTES));
        let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
            .expect("exact-delta runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start clean base candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent candidate host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes exact base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, base_version)
                .expect("inspect retained base")
        );

        let target_source = format!("{PREFIX}{}", "b".repeat(TAIL_BYTES));
        let target_version = runtime
            .apply_edit(
                base_version,
                PREFIX.len()..base_source.len(),
                &target_source[PREFIX.len()..],
            )
            .expect("replace paragraph tail")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan incremental SourceFacts");
        assert_eq!(plan.source(), target_version);
        let witness = complete_incremental_source_facts(&mut runtime);
        assert_eq!(
            witness.target_page_range().end - witness.target_page_range().start,
            15,
            "fixture must put E4 plus fifteen E5 pages on the 16-frame boundary"
        );
        let target_lease = runtime
            .snapshot_current_source()
            .expect("borrow exact target source");
        let target_completion = completion_for_persistent_target(&runtime, 2, 1);
        host.observe_source_version(source_version_for(binding, target_completion))
            .expect("host observes exact target");
        endpoint
            .start_incremental(&runtime, target_lease, witness, binding, target_completion)
            .expect("start exact-base candidate");
        let target_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );

        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
        assert!(
            target_delivery.offer.transferred_record_count
                < target_delivery.offer.target_record_count,
            "the exact delta must omit authenticated reused records"
        );
        let boundary_packet = target_delivery
            .packet_frames
            .first()
            .expect("exact delta packet");
        assert_eq!(boundary_packet.len(), 16);
        assert_eq!(boundary_packet[0].0, CandidateSnapshotFrameKind::Begin);
        assert!(
            boundary_packet[1..]
                .iter()
                .all(|(kind, _)| *kind == CandidateSnapshotFrameKind::SourceFactsReplacementPage)
        );
        assert!(
            target_delivery
                .packet_frames
                .iter()
                .skip(1)
                .flatten()
                .any(|(kind, _)| *kind == CandidateSnapshotFrameKind::Node),
            "producer must resume after the full replacement packet receives credit"
        );

        let retained = endpoint.retained.as_ref().expect("retained exact target");
        let descriptor = retained
            .publication
            .descriptor(&runtime)
            .expect("target descriptor");
        assert_eq!(
            u64::from(target_delivery.ack.record_count),
            descriptor.canonical_record_count
        );
        assert_eq!(
            target_delivery.ack.publication_session,
            digest_words(descriptor.publication)
        );
        assert_eq!(
            target_delivery.ack.source_root,
            split_u64(descriptor.source_root)
        );
        assert_eq!(
            target_delivery.ack.source_version,
            source_version_for(binding, target_completion)
        );
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::References)
                .expect("installed References"),
            1
        );
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::SourceFacts)
                .expect("installed SourceFacts"),
            runtime
                .persistent_source_facts()
                .expect("target persistent SourceFacts")
                .page_count()
        );
        assert_eq!(
            retained
                .restart
                .as_ref()
                .expect("next-revision parser restart")
                .source(),
            target_version
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(
            endpoint
                .has_exact_base_for(&runtime, target_version)
                .expect("next-revision exact base")
        );

        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }

    #[test]
    fn one_acknowledged_base_survives_rejection_replaces_without_a_chain_and_closes() {
        let mut runtime = DocumentRuntime::new(TEST_SOURCE, standard_document_runtime_config())
            .expect("test runtime");
        install_persistent_source_facts(&mut runtime);
        while !runtime.poll_retirement(256).complete {}
        let persistent_page = runtime
            .persistent_source_facts_page(0)
            .expect("persistent page lookup")
            .expect("persistent page")
            .id();
        let persistent_resident_nodes = runtime.arena_metrics().resident_nodes;
        let mut endpoint = CandidateEndpoint::new();

        let first_stream = streaming_for_runtime(&mut runtime, 4, 1);
        let first_ack = deliver_stream(&mut endpoint, &runtime, first_stream);
        assert!(!endpoint.cleanup_pending());
        while !runtime.poll_retirement(256).complete {}
        let retained_resident_nodes = runtime.arena_metrics().resident_nodes;
        assert!(retained_resident_nodes > persistent_resident_nodes);
        let first_publication = endpoint
            .retained
            .as_ref()
            .expect("first retained base")
            .publication
            .descriptor(&runtime)
            .expect("first retained descriptor")
            .publication;

        let mut rejected = streaming_for_runtime(&mut runtime, 4, 2);
        rejected.phase = StreamPhase::AwaitPacketHost {
            poll_ticket: 17,
            next_frame_ordinal: 0,
            end: false,
        };
        let rejected_offer = rejected.offer.offer_id;
        endpoint.active = Some(ActiveCandidate::Streaming(Box::new(rejected)));
        assert!(
            endpoint
                .handle_host_poll(
                    17,
                    rejected_offer,
                    HostPollPhase::PacketCredit,
                    HostPollResult::Rejected(
                        crate::v3_publication_wire::HostRejectReason::Superseded,
                    ),
                )
                .expect("reject newer candidate")
                .is_none()
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert_eq!(
            endpoint.retained.as_ref().map(|retained| retained.ack),
            Some(first_ack)
        );
        assert_eq!(
            runtime
                .persistent_source_facts_page(0)
                .expect("persistent page lookup")
                .expect("persistent page survives rejection")
                .id(),
            persistent_page
        );
        assert_eq!(
            runtime.arena_metrics().resident_nodes,
            retained_resident_nodes
        );

        let second_stream = streaming_for_runtime(&mut runtime, 4, 2);
        let second_ack = deliver_stream(&mut endpoint, &runtime, second_stream);
        assert!(second_ack.host_revision > first_ack.host_revision);
        assert!(endpoint.cleanup_pending());
        assert_eq!(
            endpoint.retained.as_ref().map(|retained| retained.ack),
            Some(second_ack)
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        let second_publication = endpoint
            .retained
            .as_ref()
            .expect("second retained base")
            .publication
            .descriptor(&runtime)
            .expect("second retained descriptor")
            .publication;
        assert_ne!(second_publication, first_publication);
        assert_eq!(
            runtime.arena_metrics().resident_nodes,
            retained_resident_nodes,
            "replacing a same-shaped base must not retain an old revision chain"
        );

        let cancelled = streaming_for_runtime(&mut runtime, 4, 3);
        endpoint.active = Some(ActiveCandidate::Streaming(Box::new(cancelled)));
        endpoint.cancel().expect("cancel newer candidate");
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert_eq!(
            endpoint.retained.as_ref().map(|retained| retained.ack),
            Some(second_ack)
        );
        assert_eq!(
            runtime.arena_metrics().resident_nodes,
            retained_resident_nodes
        );

        let stale_stream = streaming_for_runtime(&mut runtime, 4, 2);
        let mut stale_stream = stale_stream;
        seal_stream(&runtime, &mut stale_stream);
        stale_stream.phase = StreamPhase::AwaitDeliveryReceipt;
        endpoint.active = Some(ActiveCandidate::Streaming(Box::new(stale_stream)));
        assert!(matches!(
            endpoint.accept_credit(CandidateCredit::Delivery, 18),
            Err(CandidateEndpointError::InvalidAuthority)
        ));
        assert_eq!(
            endpoint.retained.as_ref().map(|retained| retained.ack),
            Some(second_ack)
        );
        endpoint.cancel().expect("cancel rejected stale delivery");
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        endpoint.begin_close().expect("begin endpoint close");
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(endpoint.retained.is_none());
        assert_eq!(
            runtime
                .persistent_source_facts_page(0)
                .expect("persistent page lookup")
                .expect("persistent page survives producer close")
                .id(),
            persistent_page
        );
        assert_eq!(
            runtime.arena_metrics().resident_nodes,
            persistent_resident_nodes,
            "producer shutdown must leave persistent SourceFacts resident"
        );

        runtime.begin_close().expect("begin runtime close");
        while !runtime
            .poll_close(256)
            .expect("poll runtime close")
            .complete
        {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    #[test]
    fn packet_builder_accumulates_across_small_poll_grants_and_flushes_on_end() {
        let (runtime, mut streaming) = test_streaming(16);
        let (pending_polls, event) = poll_to_packet_event(&runtime, &mut streaming, 1);
        assert!(pending_polls > 1);

        let CandidateEvent {
            credit:
                CandidateCredit::Packet {
                    first_frame_ordinal,
                    frame_count,
                    end,
                },
            body: CandidateEventBody::Packet { encoded },
        } = event
        else {
            panic!("expected one publication packet");
        };
        let packet = decode_publication_packet(&encoded).expect("decode produced packet");
        assert_eq!(first_frame_ordinal, 0);
        assert_eq!(frame_count, packet.frame_count);
        assert!(frame_count > 1);
        assert!(end);
        assert_eq!(
            streaming.phase,
            StreamPhase::AwaitPacketReceipt {
                first_frame_ordinal,
                frame_count,
                end,
            }
        );
        let commit = streaming.commit.expect("end frame seals commit");
        assert_eq!(commit.actual_frame_count, frame_count);
        assert_eq!(
            commit.actual_encoded_frame_bytes,
            packet.aggregate_frame_bytes
        );
        let canonical_digest256 = packet
            .frames()
            .find_map(|frame| {
                M11CandidateHost::classify_frame(frame.expect("validated packet frame").bytes)
                    .expect("classify packet frame")
                    .canonical_stream_digest256
            })
            .expect("end frame carries canonical stream digest");
        assert_eq!(
            commit.canonical_stream_digest,
            protocol_digest128_from_blake3(
                ProtocolDigestDomain::CandidateStream,
                canonical_digest256,
            )
        );
        let expected_ack = streaming.expected_ack.expect("sealed candidate ack");
        assert_eq!(
            expected_ack.sequence_digest,
            protocol_digest128_from_blake3(
                ProtocolDigestDomain::CandidateAckSequence,
                streaming.descriptor.manifest_digest256,
            )
        );
        assert_ne!(
            canonical_digest256, streaming.descriptor.manifest_digest256,
            "commit and ACK must bind different 256-bit proofs"
        );
        assert_ne!(
            commit.canonical_stream_digest, expected_ack.sequence_digest,
            "commit and ACK must use separate digest domains"
        );
        assert!(streaming.lookahead.is_none());
        cancel_streaming_to_zero(runtime, streaming);
    }

    #[test]
    fn packet_builder_enforces_exact_count_body_and_offer_caps() {
        let mut count_limited = PacketBuilder::default();
        for ordinal in 0..MAXIMUM_PACKET_FRAME_COUNT {
            push_test_frame(&mut count_limited, ordinal, 1);
        }
        assert!(
            count_limited
                .saturated(MAXIMUM_PACKET_ENCODED_BYTES)
                .expect("count saturation")
        );
        assert!(
            !count_limited
                .can_accept(1, MAXIMUM_PACKET_ENCODED_BYTES)
                .expect("count boundary")
        );

        let mut body_limited = PacketBuilder::default();
        for ordinal in 0..12 {
            push_test_frame(&mut body_limited, ordinal, 5_041);
        }
        push_test_frame(&mut body_limited, 12, 5_044);
        assert_eq!(
            body_limited.aggregate_frame_bytes,
            MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES as usize
        );
        assert!(
            body_limited
                .saturated(MAXIMUM_PACKET_ENCODED_BYTES)
                .expect("body saturation")
        );
        assert!(
            !body_limited
                .can_accept(1, MAXIMUM_PACKET_ENCODED_BYTES)
                .expect("body boundary")
        );

        let mut offer_limited = PacketBuilder::default();
        push_test_frame(&mut offer_limited, 0, 10);
        let exact_offer_cap = offer_limited.encoded_len().expect("encoded length");
        assert!(
            offer_limited
                .saturated(exact_offer_cap)
                .expect("offer saturation")
        );
        assert!(
            !offer_limited
                .can_accept(1, exact_offer_cap)
                .expect("offer boundary")
        );
    }

    #[test]
    fn non_fitting_frame_is_retained_as_single_lookahead() {
        let (runtime, mut streaming) = test_streaming(1);
        for ordinal in 0..13 {
            push_test_frame(&mut streaming.packet, ordinal, 5_000);
        }
        streaming.next_frame_ordinal = 13;
        streaming.lookahead = Some(M11SnapshotFrame {
            kind: M11SnapshotFrameKind::Node,
            node_ordinal: Some(12),
            canonical_record_count: 0,
            canonical_stream_digest256: None,
            bytes: vec![0; 1_000].into_boxed_slice(),
        });

        let (_, event) = poll_to_packet_event(&runtime, &mut streaming, 1);
        let CandidateEventBody::Packet { encoded } = event.body else {
            panic!("expected publication packet");
        };
        let packet = decode_publication_packet(&encoded).expect("decode full packet");
        assert_eq!(packet.frame_count, 13);
        assert!(streaming.lookahead.is_some());
        assert!(streaming.packet.frames.is_empty());
        cancel_streaming_to_zero(runtime, streaming);
    }

    #[test]
    fn packet_credit_requires_exact_frame_range_and_host_cursor() {
        let (runtime, mut streaming) = test_streaming(1);
        streaming.phase = StreamPhase::AwaitPacketReceipt {
            first_frame_ordinal: 4,
            frame_count: 3,
            end: false,
        };
        let offer_id = streaming.offer.offer_id;
        let mut endpoint = CandidateEndpoint {
            active: Some(ActiveCandidate::Streaming(Box::new(streaming))),
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
        };

        assert!(matches!(
            endpoint.accept_credit(
                CandidateCredit::Packet {
                    first_frame_ordinal: 4,
                    frame_count: 2,
                    end: false,
                },
                77,
            ),
            Err(CandidateEndpointError::InvalidState)
        ));
        endpoint
            .accept_credit(
                CandidateCredit::Packet {
                    first_frame_ordinal: 4,
                    frame_count: 3,
                    end: false,
                },
                77,
            )
            .expect("exact packet event credit");
        assert!(matches!(
            endpoint.handle_host_poll(
                77,
                offer_id,
                HostPollPhase::PacketCredit,
                HostPollResult::Completed(HostPollOutcome::PacketCredit {
                    offer_id,
                    next_frame_ordinal: 6,
                }),
            ),
            Err(CandidateEndpointError::InvalidState)
        ));
        assert!(
            endpoint
                .handle_host_poll(
                    77,
                    offer_id,
                    HostPollPhase::PacketCredit,
                    HostPollResult::Completed(HostPollOutcome::PacketCredit {
                        offer_id,
                        next_frame_ordinal: 7,
                    }),
                )
                .expect("exact host packet cursor")
                .is_none()
        );
        cancel_endpoint_to_zero(runtime, endpoint);
    }

    #[test]
    fn cancellation_reclaims_stream_with_buffered_packet() {
        let (runtime, mut streaming) = test_streaming(32);
        match streaming
            .poll_event(&runtime, 1)
            .expect("first bounded packet poll")
        {
            CandidatePoll::Pending { transitions } => assert!(transitions <= 1),
            CandidatePoll::Event { .. } => panic!("one transition unexpectedly finished stream"),
            CandidatePoll::HotInlineEvent { .. } => {
                panic!("structural stream emitted a hot-inline event")
            }
            CandidatePoll::ViewportPresentationEvent { .. } => {
                panic!("structural stream emitted a viewport event")
            }
            CandidatePoll::ViewportPresentationUnavailable { .. } => {
                panic!("structural stream emitted viewport unavailability")
            }
        }
        assert_eq!(streaming.phase, StreamPhase::NeedPacket);
        assert!(!streaming.packet.frames.is_empty());
        cancel_streaming_to_zero(runtime, streaming);
    }

    #[test]
    fn clean_parse_cancellation_drops_immediately_while_cleanup_drains() {
        let (mut runtime, mut prior) = test_streaming(32);
        let prior_stream = prior.stream.take().expect("prior stream");
        let profile = SourceFactsScanProfile::new(4).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let binding = SessionBinding {
            document_session: [801, 802, 803, 804],
            source_session_identity: 805,
            worker_generation: 1,
        };
        let mut endpoint = CandidateEndpoint::new();
        endpoint.cleanup = Some(CandidateCleanup::Stream {
            stream: Box::new(prior_stream),
            begun: false,
        });
        endpoint
            .start(certified, binding, completion)
            .expect("clean parse may overlap bounded prior cleanup");
        assert!(matches!(endpoint.active, Some(ActiveCandidate::Parsing(_))));

        endpoint.cancel().expect("cancel clean parse");
        assert!(endpoint.active.is_none());
        assert!(endpoint.cleanup.is_some());
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(256).expect("runtime close").complete {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    }

    #[test]
    fn candidate_commit_is_invariant_to_packet_regrouping() {
        let (runtime, mut streaming) = test_streaming(16);
        let (_, event) = poll_to_packet_event(&runtime, &mut streaming, 256);
        let CandidateEventBody::Packet { encoded } = event.body else {
            panic!("expected publication packet");
        };
        let original = decode_publication_packet(&encoded).expect("decode original packet");
        let frames: Vec<_> = original
            .frames()
            .map(|frame| frame.expect("validated frame"))
            .collect();
        let split = frames.len() / 2;
        assert!(split > 0 && split < frames.len());

        let inputs: Vec<_> = frames
            .iter()
            .map(|frame| PublicationPacketFrameInput {
                record_count: frame.record_count,
                digest: frame.digest,
                bytes: frame.bytes,
            })
            .collect();
        let mut first_bytes = vec![0; MAXIMUM_PACKET_ENCODED_BYTES];
        let first_len = encode_publication_packet_into(
            PublicationPacketInput {
                offer_id: original.offer_id,
                first_frame_ordinal: original.first_frame_ordinal,
                first_record_ordinal: original.first_record_ordinal,
                frames: &inputs[..split],
            },
            &mut first_bytes,
        )
        .expect("encode first regrouped packet");
        first_bytes.truncate(first_len);
        let mut second_bytes = vec![0; MAXIMUM_PACKET_ENCODED_BYTES];
        let second_len = encode_publication_packet_into(
            PublicationPacketInput {
                offer_id: original.offer_id,
                first_frame_ordinal: frames[split].ordinal,
                first_record_ordinal: frames[split].first_record_ordinal,
                frames: &inputs[split..],
            },
            &mut second_bytes,
        )
        .expect("encode second regrouped packet");
        second_bytes.truncate(second_len);

        let first = decode_publication_packet(&first_bytes).expect("decode first regrouped packet");
        let second =
            decode_publication_packet(&second_bytes).expect("decode second regrouped packet");
        let mut transport = CandidateTransportDigest::new();
        for packet in [first, second] {
            for frame in packet.frames() {
                let frame = frame.expect("validated regrouped frame");
                let metadata = M11CandidateHost::classify_frame(frame.bytes)
                    .expect("independent frame classification");
                assert_eq!(metadata.canonical_record_count, frame.record_count);
                let kind = match metadata.kind {
                    M11HostFrameKind::Begin => CandidateSnapshotFrameKind::Begin,
                    M11HostFrameKind::SourceFactsReplacementPage => {
                        CandidateSnapshotFrameKind::SourceFactsReplacementPage
                    }
                    M11HostFrameKind::BlockSequenceReplacementPage => {
                        CandidateSnapshotFrameKind::BlockSequenceReplacementPage
                    }
                    M11HostFrameKind::RecursiveGreenReplacementPage => {
                        CandidateSnapshotFrameKind::RecursiveGreenReplacementPage
                    }
                    M11HostFrameKind::Node => CandidateSnapshotFrameKind::Node,
                    M11HostFrameKind::End => CandidateSnapshotFrameKind::End,
                };
                let digest256 = transport
                    .push(
                        frame.ordinal,
                        frame.first_record_ordinal,
                        frame.record_count,
                        kind,
                        frame.bytes,
                    )
                    .expect("regrouped transport frame");
                assert_eq!(
                    protocol_digest128_from_blake3(ProtocolDigestDomain::CandidateFrame, digest256),
                    frame.digest
                );
            }
        }
        let receipt = transport.finish();
        let commit = streaming.commit.expect("sealed candidate commit");
        assert_eq!(receipt.frame_count, commit.actual_frame_count);
        assert_eq!(
            receipt.encoded_frame_bytes,
            commit.actual_encoded_frame_bytes
        );
        assert_eq!(
            protocol_digest128_from_blake3(
                ProtocolDigestDomain::CandidateTransport,
                receipt.digest256
            ),
            commit.rolling_transport_digest
        );
        cancel_streaming_to_zero(runtime, streaming);
    }
}

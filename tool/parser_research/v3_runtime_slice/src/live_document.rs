//! Worker-owned authority seam for one live Markdown document.
//!
//! This module deliberately does not expose the source store, coordinator, or
//! arena independently. Parser work is admitted only as one source-bound arena
//! build, and cancellation consumes that candidate into the arena's fuelled
//! build-abort state machine.

use std::fmt;
use std::num::NonZeroU64;
use std::ops::Range;

#[cfg(feature = "exact-parser")]
pub(crate) mod persisted_restart_activation;

use crate::{
    AcceptedEdit, AdmissionReceipt, ArenaBuildError, ArenaBuildId, ArenaBuildLifecycle,
    ArenaBuildTicket, ArenaError, ArenaIdentity, BlockId, CandidateLineReceipt,
    CandidateLogicalAction, CandidateOpenBinding, CandidateRecognitionBytePollError,
    CandidateRecognitionBytePollReceipt, CandidateRecognitionByteScanner,
    CandidateRecognitionByteSession, CandidateRecognitionByteSessionFinishReceipt,
    CandidateRecognitionCheckpoint, CandidateRecognitionLineReceipt, CandidateRecognitionPoll,
    CandidateRecognitionRangeKind, CandidateRecognitionRangeReceipt, CandidateRecognitionSink,
    CandidateRecognitionWindowError, CandidateRecognitionWindowReceipt, CandidateSourceAtom,
    CandidateSourceBoundary, CandidateSourceLedger, CandidateSourceLineBoundaryContinuation,
    CandidateSourcePoll, CandidateSourceSeal, CandidateTerminatorResolution, CandidateWriter,
    CandidateWriterBinding, CandidateWriterConfig, CandidateWriterError,
    CandidateWriterLogicalAction, CandidateWriterProgress, CandidateWriterSourceAtom,
    CandidateWriterSourcePoll, ConsumedSourcePiece, Coordinator, CoordinatorError, CoverageId,
    CoveragePart, CropSourceCursor, GreenAffinity, GreenCloseFacts, GreenFencedCodeOpenFacts,
    GreenHeadingOpenFacts, GreenItemOpenFacts, GreenKind, GreenListOpenFacts,
    GreenTableCellOpenFacts, GreenTableOpenFacts, GreenTableRowOpenFacts, OutputRootLease,
    PageArena, ParseGeneration, ParsePlan, ParseToken, PromotionReceipt, PublicationDelta,
    ReclaimPollError, ReclaimReceipt, ResumableSerializedGreenBuild, RetiredSourceRoot,
    SerializedGreenManifestDescriptor, SourceBoundLedgerError, SourceBoundProjectionComposer,
    SourceByte, SourcePhysicalLineDescriptor, SourceProjectionComposerError, SourceQueryView,
    SourceRevision, SourceRootId, SourceSnapshotDescriptor, SourceStore, SourceStoreError,
    ValidatedSourceClaim,
};

use crate::CandidateWriterLocalCommitFailure;

#[cfg(feature = "exact-parser")]
use crate::RetainedSetextDriverActivation;
#[cfg(feature = "exact-parser")]
use crate::committed_checkpoint_index::{ParentBoundDonorSuccessor, ParentBoundDonorSuccessorStep};
#[cfg(feature = "exact-parser")]
use crate::parent_selected_convergence::{
    ParentSelectedConvergenceMapError, ParentSelectedConvergenceMapJob,
    ParentSelectedConvergenceMapProgress, ParentSelectedConvergenceMapStart,
    ParentSelectedConvergenceTargetRelation, ParentSelectedMappedConvergence,
};
#[cfg(feature = "exact-parser")]
use crate::same_build_checkpoint::{
    JoinedParserDonorSample, ParserLineBoundaryCheckpointAuthority, SameBuildLineBoundaryCheckpoint,
};
#[cfg(feature = "exact-parser")]
use crate::setext_cross_build_restart::{
    InMemorySetextActivationError, InMemorySetextActivationJob, InMemorySetextActivationProgress,
    InMemorySetextCheckpointDraft, JoinedInMemorySetextRestart, ReadyInMemorySetextActivation,
};
#[cfg(all(test, feature = "exact-parser"))]
use crate::storage_only_composite_document::PublishedRestartCompositeDocumentView;
#[cfg(feature = "exact-parser")]
use crate::storage_only_composite_document::{
    PreparedRestartCompositePublication, PublishedRestartCompositeHandle, RestartCompositeDocument,
    RestartCompositeDocumentBuildReceipt, RestartCompositeDocumentError,
};
#[cfg(feature = "exact-parser")]
use crate::{
    CandidateLineBoundaryCheckpointAdmission, CandidateWriterLineBoundaryContinuation,
    CandidateWriterTailAdoptionReady, CandidateWriterTailAdoptionReceipt,
    CapturedDonorCheckpointSample, CapturedParentSelectedSuffixSample,
    DonorCheckpointSampleCaptureFailure, DonorCheckpointSampleCursor,
    GreenSourceTailAdoptionCapability, LineageAdoptionBundleJob, LineageAdoptionBundleProof,
    ParentSelectedCandidateAdoptionTail, RestartCheckpointSampleChain,
    SourceBoundGreenTailAdoption, TailAdoptionJoinError,
};

#[cfg(test)]
use crate::GreenFencedCodeCloseFacts;

const BOOTSTRAP_PAYLOAD: &[u8] = b"flark-v3-unparsed-bootstrap";

/// Maximum number of old or unpublished Crop roots retained until the host
/// moves them onto its native disposer or Web/Wasm idle lane.
///
/// This deliberately small fixed bound admits at most four outstanding
/// whole-document replacements. Further edits receive backpressure before
/// source preparation or candidate cancellation begins.
pub const SOURCE_RETIREMENT_QUEUE_CAPACITY: usize = 4;

/// Pessimistic bound on the sum of source bytes described by queued roots.
/// Crop sharing means physical retention is normally much smaller for local
/// edits, but charging every root at its full logical length gives admission a
/// simple safe policy. A 100 MiB whole-document root is explicitly supported;
/// the worker must drain before admitting work that would exceed 256 MiB.
pub const SOURCE_RETIREMENT_QUEUE_BYTE_CAPACITY: usize = 256 * 1024 * 1024;

/// Opaque identity of the one parser candidate currently owned by a live
/// document. Its fields are observations, not independently forgeable build
/// authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveCandidateEpoch {
    token: ParseToken,
    source: SourceSnapshotDescriptor,
    arena: ArenaIdentity,
    build: ArenaBuildId,
}

impl LiveCandidateEpoch {
    #[must_use]
    pub const fn parse_token(self) -> ParseToken {
        self.token
    }

    #[must_use]
    pub const fn source(self) -> SourceSnapshotDescriptor {
        self.source
    }

    #[must_use]
    pub const fn arena_identity(self) -> ArenaIdentity {
        self.arena
    }

    #[must_use]
    pub const fn build_id(self) -> ArenaBuildId {
        self.build
    }
}

/// Actor-joined coordinates for the untouched physical line at one candidate's
/// speculative-recognition cursor.
///
/// The physical extent comes from the persistent source index, while the
/// opaque checkpoint proves the active writer/build and exact line ordinal.
/// Neither component can authorize source consumption or green publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateRecognitionLineDescriptor {
    epoch: LiveCandidateEpoch,
    checkpoint: CandidateRecognitionCheckpoint,
    physical_line: SourcePhysicalLineDescriptor,
}

impl CandidateRecognitionLineDescriptor {
    #[must_use]
    pub const fn epoch(self) -> LiveCandidateEpoch {
        self.epoch
    }

    #[must_use]
    pub const fn checkpoint(self) -> CandidateRecognitionCheckpoint {
        self.checkpoint
    }

    #[must_use]
    pub const fn physical_line(self) -> SourcePhysicalLineDescriptor {
        self.physical_line
    }

    #[must_use]
    pub const fn source(self) -> SourceSnapshotDescriptor {
        self.physical_line.source()
    }

    #[must_use]
    pub const fn line_ordinal(self) -> u64 {
        self.checkpoint.line_ordinal()
    }

    #[must_use]
    pub const fn start(self) -> usize {
        self.physical_line.start()
    }

    #[must_use]
    pub const fn content_end(self) -> usize {
        self.physical_line.content_end()
    }

    #[must_use]
    pub const fn end(self) -> usize {
        self.physical_line.end()
    }

    #[must_use]
    pub const fn physical_utf16(self) -> usize {
        self.physical_line.physical_utf16()
    }

    pub(crate) const fn into_parts(
        self,
    ) -> (
        LiveCandidateEpoch,
        CandidateRecognitionCheckpoint,
        SourcePhysicalLineDescriptor,
    ) {
        (self.epoch, self.checkpoint, self.physical_line)
    }
}

/// Opaque query identity for a candidate whose source lease was dropped and
/// whose arena journal is being retired under explicit fuel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateAbort {
    epoch: LiveCandidateEpoch,
}

/// Read-only receipt for one atomically admitted source edit. The source and
/// coordinator components describe the same transition; a cancelled parser
/// job, if any, is already detached and aborting under explicit fuel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveEditReceipt {
    source: AcceptedEdit,
    admission: AdmissionReceipt,
    cancelled: Option<CandidateAbort>,
}

impl LiveEditReceipt {
    #[must_use]
    pub const fn source(&self) -> &AcceptedEdit {
        &self.source
    }

    #[must_use]
    pub const fn admission(&self) -> AdmissionReceipt {
        self.admission
    }

    #[must_use]
    pub const fn cancelled(&self) -> Option<CandidateAbort> {
        self.cancelled
    }
}

/// Copyable observation of both live-document clocks. It carries no source,
/// parser, arena, or publication authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveDocumentClockSnapshot {
    source: SourceSnapshotDescriptor,
    coordinator_revision: SourceRevision,
    coordinator_root: SourceRootId,
    parse_generation: ParseGeneration,
    active: Option<ParseToken>,
    queued: Option<ParseToken>,
}

impl LiveDocumentClockSnapshot {
    #[must_use]
    pub const fn source(self) -> SourceSnapshotDescriptor {
        self.source
    }

    #[must_use]
    pub const fn coordinator_source(self) -> (SourceRevision, SourceRootId) {
        (self.coordinator_revision, self.coordinator_root)
    }

    #[must_use]
    pub const fn parse_generation(self) -> ParseGeneration {
        self.parse_generation
    }

    #[must_use]
    pub const fn active(self) -> Option<ParseToken> {
        self.active
    }

    #[must_use]
    pub const fn queued(self) -> Option<ParseToken> {
        self.queued
    }

    #[must_use]
    pub const fn source_and_coordinator_are_aligned(self) -> bool {
        self.source.revision.0 == self.coordinator_revision.0
            && self.source.root.0 == self.coordinator_root.0
    }
}

impl CandidateAbort {
    #[must_use]
    pub const fn epoch(self) -> LiveCandidateEpoch {
        self.epoch
    }

    #[must_use]
    pub const fn build_id(self) -> ArenaBuildId {
        self.epoch.build
    }
}

/// A fresh document-wide block identity reserved for exactly one candidate
/// build. This value is linear: it is intentionally neither `Clone` nor
/// `Copy`, and dropping it burns the already advanced scalar ID.
#[must_use = "a fresh block identity is build-scoped and is burned if unused"]
#[derive(Debug, PartialEq, Eq)]
pub struct FreshBlockPermit {
    build: ArenaBuildId,
    id: BlockId,
}

impl FreshBlockPermit {
    #[must_use]
    pub const fn id(&self) -> BlockId {
        self.id
    }

    #[must_use]
    pub const fn build_id(&self) -> ArenaBuildId {
        self.build
    }
}

/// A fresh document-wide coverage identity reserved for exactly one candidate
/// build. Cancellation never rewinds the allocator that minted it.
#[must_use = "a fresh coverage identity is build-scoped and is burned if unused"]
#[derive(Debug, PartialEq, Eq)]
pub struct FreshCoveragePermit {
    build: ArenaBuildId,
    id: CoverageId,
}

impl FreshCoveragePermit {
    #[must_use]
    pub const fn id(&self) -> CoverageId {
        self.id
    }

    #[must_use]
    pub const fn build_id(&self) -> ArenaBuildId {
        self.build
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityIdentityKind {
    Block,
    Coverage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveDocumentError {
    CandidateAlreadyActive,
    NoCandidate,
    WrongCandidateEpoch,
    CandidateStale,
    /// One arena-committed restart output is still owned by the actor while
    /// coordinator publication is retried or explicitly retired. No second
    /// candidate or source transition may cross that linear hold.
    RestartPublicationHeld,
    UnknownAbort,
    CancellationCapacityExhausted,
    SourceRetirementBackpressure,
    SourceRetirementByteBackpressure {
        required: usize,
        available: usize,
    },
    IdentityExhausted(EntityIdentityKind),
    CandidateSourceLedgerAlreadyActive,
    CandidateSourceLedgerNotActive,
    CandidateSourceLedgerRequiresFreshCursor,
    Arena(ArenaError),
    ArenaBuild(ArenaBuildError),
    Coordinator(CoordinatorError),
    Source(SourceStoreError),
    SourceLedger(SourceBoundLedgerError),
    SourceProjectionComposer(SourceProjectionComposerError),
    Invariant(&'static str),
}

impl fmt::Display for LiveDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CandidateAlreadyActive => {
                formatter.write_str("the live document already owns a parser candidate")
            }
            Self::NoCandidate => formatter.write_str("the live document has no parser candidate"),
            Self::WrongCandidateEpoch => {
                formatter.write_str("candidate epoch does not identify current parser work")
            }
            Self::CandidateStale => {
                formatter.write_str("candidate no longer matches the live document epochs")
            }
            Self::RestartPublicationHeld => formatter.write_str(
                "an arena-committed restart output is awaiting publication or retirement",
            ),
            Self::UnknownAbort => {
                formatter.write_str("candidate abort is complete, stale, or belongs elsewhere")
            }
            Self::CancellationCapacityExhausted => {
                formatter.write_str("candidate cancellation tracking capacity exhausted")
            }
            Self::SourceRetirementBackpressure => write!(
                formatter,
                "source retirement queue is full (capacity {SOURCE_RETIREMENT_QUEUE_CAPACITY})"
            ),
            Self::SourceRetirementByteBackpressure {
                required,
                available,
            } => write!(
                formatter,
                "source retirement needs {required} logical bytes, only {available} remain"
            ),
            Self::IdentityExhausted(kind) => {
                write!(formatter, "document {kind:?} identity space exhausted")
            }
            Self::CandidateSourceLedgerAlreadyActive => {
                formatter.write_str("the candidate source ledger is already active")
            }
            Self::CandidateSourceLedgerNotActive => {
                formatter.write_str("the candidate source ledger is not active")
            }
            Self::CandidateSourceLedgerRequiresFreshCursor => formatter
                .write_str("the candidate source ledger requires an unconsumed candidate cursor"),
            Self::Arena(error) => error.fmt(formatter),
            Self::ArenaBuild(error) => error.fmt(formatter),
            Self::Coordinator(error) => error.fmt(formatter),
            Self::Source(error) => error.fmt(formatter),
            Self::SourceLedger(error) => error.fmt(formatter),
            Self::SourceProjectionComposer(error) => error.fmt(formatter),
            Self::Invariant(message) => {
                write!(formatter, "live-document invariant failed: {message}")
            }
        }
    }
}

impl std::error::Error for LiveDocumentError {}

impl From<ArenaError> for LiveDocumentError {
    fn from(error: ArenaError) -> Self {
        Self::Arena(error)
    }
}

impl From<ArenaBuildError> for LiveDocumentError {
    fn from(error: ArenaBuildError) -> Self {
        Self::ArenaBuild(error)
    }
}

impl From<CoordinatorError> for LiveDocumentError {
    fn from(error: CoordinatorError) -> Self {
        Self::Coordinator(error)
    }
}

impl From<SourceStoreError> for LiveDocumentError {
    fn from(error: SourceStoreError) -> Self {
        Self::Source(error)
    }
}

impl From<SourceBoundLedgerError> for LiveDocumentError {
    fn from(error: SourceBoundLedgerError) -> Self {
        Self::SourceLedger(error)
    }
}

impl From<SourceProjectionComposerError> for LiveDocumentError {
    fn from(error: SourceProjectionComposerError) -> Self {
        Self::SourceProjectionComposer(error)
    }
}

/// Exact source authority held by one candidate. Neither the descriptor nor
/// the cursor can be supplied by parser code.
#[derive(Debug)]
struct BoundSourceCursor {
    descriptor: SourceSnapshotDescriptor,
    total_utf16: usize,
    cursor: CropSourceCursor,
}

#[derive(Debug)]
enum CandidateWriterSlot {
    None,
    Active(Box<CandidateWriter>),
    #[cfg(feature = "exact-parser")]
    Paused(Box<CandidateWriterLineBoundaryContinuation>),
    #[cfg(feature = "exact-parser")]
    AdoptedTail(Box<CandidateWriterTailAdoptionReady>),
}

#[derive(Debug)]
struct CandidateJob {
    epoch: LiveCandidateEpoch,
    raw_source: Option<BoundSourceCursor>,
    ledger: Option<CandidateSourceLedger>,
    projection_composer_admitted: bool,
    ticket: Option<ArenaBuildTicket>,
    identities: Option<DocumentIdentityAllocator>,
    writer: CandidateWriterSlot,
    #[cfg(feature = "exact-parser")]
    retained_activation: Option<NonZeroU64>,
    #[cfg(feature = "exact-parser")]
    persisted_restart: persisted_restart_activation::PersistedRestartSourcePhase,
    /// Commit has consumed the writer/source domains, but arena abort
    /// admission returned the still-linear suspended ticket. This state is
    /// cancellation-only: it exists solely so no exceptional arena failure
    /// can orphan the build ticket or document identity allocator.
    commit_recovery: bool,
}

/// Actor-local identity of the last successfully committed mechanism green
/// document. This is not coordinator publication, but it prevents a retained
/// draft from another document/build with colliding low semantic IDs from
/// entering this actor's next candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MechanismDocumentBinding {
    epoch: LiveCandidateEpoch,
    green: SerializedGreenManifestDescriptor,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
struct RestartDocumentBinding {
    epoch: LiveCandidateEpoch,
    published: PublishedRestartCompositeHandle,
    receipt: RestartCompositeDocumentBuildReceipt,
}

/// Copyable scheduler observation for one arena-committed restart output
/// which has not crossed coordinator publication. The owning document or
/// prepared owner-plus-descriptor transaction never leaves `LiveDocumentStore`.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RestartCompositePublicationHold {
    epoch: LiveCandidateEpoch,
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
enum RestartCompositePublicationHoldOwner {
    Owning(RestartCompositeDocument),
    Prepared(PreparedRestartCompositePublication),
}

#[cfg(feature = "exact-parser")]
#[derive(Debug)]
struct PendingRestartCompositePublication {
    epoch: LiveCandidateEpoch,
    receipt: RestartCompositeDocumentBuildReceipt,
    last_error: RestartCompositeDocumentError,
    owner: RestartCompositePublicationHoldOwner,
}

/// Terminal local-build result. `Published` is the ordinary production path.
/// `Held` is not a failed candidate and therefore carries no arena-build abort:
/// the actor owns a committed output which must be retried or retired through
/// the explicit hold API before any next edit/candidate is admitted.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestartCompositeCommitProgress {
    Published {
        receipt: RestartCompositeDocumentBuildReceipt,
        publication: PublicationDelta,
    },
    Held {
        receipt: RestartCompositeDocumentBuildReceipt,
        error: RestartCompositeDocumentError,
        hold: RestartCompositePublicationHold,
    },
}

/// Actor-visible publication barrier following exact parser completion. A
/// caller must observe `Published` before offering rendered semantics. `Held`
/// means local parse/build work is complete but the exact owner transaction
/// still needs `retry_restart_composite_publication` or
/// `release_restart_composite_publication`; it is never silent terminal work.
#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestartCompositePublicationState {
    Unavailable,
    Published {
        epoch: LiveCandidateEpoch,
        receipt: RestartCompositeDocumentBuildReceipt,
        output: OutputRootLease,
    },
    Held {
        receipt: RestartCompositeDocumentBuildReceipt,
        error: RestartCompositeDocumentError,
        hold: RestartCompositePublicationHold,
    },
}

#[cfg(feature = "exact-parser")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RestartCompositePublicationReleaseFailure {
    pub(crate) error: RestartCompositeDocumentError,
    pub(crate) hold: RestartCompositePublicationHold,
}

impl CandidateJob {
    fn source_descriptor(&self) -> SourceSnapshotDescriptor {
        if self.commit_recovery {
            return self.epoch.source;
        }
        #[cfg(feature = "exact-parser")]
        if matches!(
            self.persisted_restart,
            persisted_restart_activation::PersistedRestartSourcePhase::AdoptionSplicing { .. }
        ) {
            return self.epoch.source;
        }
        if let Some(source) = &self.raw_source {
            source.descriptor
        } else if let Some(ledger) = &self.ledger {
            ledger.descriptor()
        } else {
            match &self.writer {
                CandidateWriterSlot::Active(writer) => writer.source_descriptor(),
                #[cfg(feature = "exact-parser")]
                CandidateWriterSlot::Paused(writer) => writer.source_descriptor(),
                #[cfg(feature = "exact-parser")]
                CandidateWriterSlot::AdoptedTail(writer) => writer.source_descriptor(),
                CandidateWriterSlot::None => panic!("candidate owns source"),
            }
        }
    }

    fn source_identity(&self) -> SourceRootId {
        if self.commit_recovery {
            return self.epoch.source.root;
        }
        #[cfg(feature = "exact-parser")]
        if matches!(
            self.persisted_restart,
            persisted_restart_activation::PersistedRestartSourcePhase::AdoptionSplicing { .. }
        ) {
            return self.epoch.source.root;
        }
        if let Some(source) = &self.raw_source {
            source.cursor.source_identity()
        } else if let Some(ledger) = &self.ledger {
            ledger.source_identity()
        } else {
            match &self.writer {
                CandidateWriterSlot::Active(writer) => writer.source_identity(),
                #[cfg(feature = "exact-parser")]
                CandidateWriterSlot::Paused(writer) => writer.source_identity(),
                #[cfg(feature = "exact-parser")]
                CandidateWriterSlot::AdoptedTail(writer) => writer.source_descriptor().root,
                CandidateWriterSlot::None => panic!("candidate owns source"),
            }
        }
    }

    fn cursor_offset(&self) -> usize {
        if self.commit_recovery {
            return self.epoch.source.bytes;
        }
        #[cfg(feature = "exact-parser")]
        if matches!(
            self.persisted_restart,
            persisted_restart_activation::PersistedRestartSourcePhase::AdoptionSplicing { .. }
        ) {
            return self.epoch.source.bytes;
        }
        #[cfg(feature = "exact-parser")]
        if let Some(offset) = self.persisted_restart.cursor_offset() {
            return offset;
        }
        if let Some(source) = &self.raw_source {
            source.cursor.offset()
        } else if let Some(ledger) = &self.ledger {
            ledger.cursor_offset()
        } else {
            match &self.writer {
                CandidateWriterSlot::Active(writer) => writer.cursor_offset(),
                #[cfg(feature = "exact-parser")]
                CandidateWriterSlot::Paused(writer) => writer
                    .cursor_offset()
                    .expect("validated checkpoint source offset fits usize"),
                #[cfg(feature = "exact-parser")]
                CandidateWriterSlot::AdoptedTail(writer) => writer
                    .cursor_offset()
                    .expect("validated adopted-tail prefix offset fits usize"),
                CandidateWriterSlot::None => panic!("candidate owns source"),
            }
        }
    }

    fn build_id(&self) -> Result<ArenaBuildId, CandidateWriterError> {
        if let Some(ticket) = &self.ticket {
            Ok(ticket.id())
        } else {
            match &self.writer {
                CandidateWriterSlot::Active(writer) => writer.build_id(),
                #[cfg(feature = "exact-parser")]
                CandidateWriterSlot::Paused(writer) => writer.build_id(),
                #[cfg(feature = "exact-parser")]
                CandidateWriterSlot::AdoptedTail(writer) => writer.build_id(),
                CandidateWriterSlot::None => Err(CandidateWriterError::Invariant(
                    "candidate build authority missing",
                )),
            }
        }
    }
}

#[derive(Debug)]
struct AbortingCandidate {
    epoch: LiveCandidateEpoch,
    arena_complete: bool,
    #[cfg(feature = "exact-parser")]
    heap: crate::candidate_writer::CandidateWriterHeapRetirement,
}

impl AbortingCandidate {
    fn empty(epoch: LiveCandidateEpoch) -> Self {
        Self {
            epoch,
            arena_complete: false,
            #[cfg(feature = "exact-parser")]
            heap: crate::candidate_writer::CandidateWriterHeapRetirement::empty(),
        }
    }

    #[cfg(feature = "exact-parser")]
    fn with_heap(
        epoch: LiveCandidateEpoch,
        heap: crate::candidate_writer::CandidateWriterHeapRetirement,
    ) -> Self {
        Self {
            epoch,
            arena_complete: false,
            heap,
        }
    }
}

/// Receipt for one cancellation slice spanning both arena owners and
/// candidate-local heap drafts. The sum of `owners_scheduled` and
/// `heap_transitions` never exceeds the caller's fuel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CandidateAbortPollReceipt {
    pub owners_scheduled: usize,
    pub owners_remaining: usize,
    pub arena_complete: bool,
    pub heap_transitions: usize,
    pub heap_complete: bool,
    pub complete: bool,
}

/// Actor-safe commit failure. The writer's linear assets have either entered
/// the document's fuelled abort queue (`abort` is present) or remain parked in
/// the document's cancellation-only recovery candidate (`abort` is absent).
/// No ticket or identity allocator escapes this boundary.
#[derive(Debug)]
pub(crate) struct CandidateWriterMechanismCommitFailure {
    pub(crate) error: CandidateWriterError,
    pub(crate) abort: Option<CandidateAbort>,
}

/// Activation failure with explicit cleanup state. Before the first green
/// poll `abort` is `None` and the untouched candidate may fall back to a full
/// parse. After any retained-page mutation the actor transitions the entire
/// fresh build to fuelled abort and returns its handle here.
#[cfg(feature = "exact-parser")]
#[derive(Debug)]
#[allow(dead_code)] // External-borrow feasibility result; production consumes it from a composite-root job.
pub(crate) struct InMemorySetextActivationFailure {
    pub(crate) error: InMemorySetextActivationError,
    pub(crate) abort: Option<CandidateAbort>,
    pub(crate) cleanup_error: Option<LiveDocumentError>,
}

/// Inline FIFO storage keeps edit preflight and enqueue allocation-free. Its
/// logical bound is admission policy, not an allocator-dependent `Vec`
/// capacity.
#[derive(Debug)]
struct RetiredSourceRootQueue {
    slots: [Option<RetiredSourceRoot>; SOURCE_RETIREMENT_QUEUE_CAPACITY],
    head: usize,
    len: usize,
    bytes: usize,
    byte_capacity: usize,
}

impl RetiredSourceRootQueue {
    fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
            head: 0,
            len: 0,
            bytes: 0,
            byte_capacity: SOURCE_RETIREMENT_QUEUE_BYTE_CAPACITY,
        }
    }

    const fn len(&self) -> usize {
        self.len
    }

    const fn is_full(&self) -> bool {
        self.len == SOURCE_RETIREMENT_QUEUE_CAPACITY
    }

    const fn bytes(&self) -> usize {
        self.bytes
    }

    const fn available_bytes(&self) -> usize {
        self.byte_capacity - self.bytes
    }

    fn push(&mut self, root: RetiredSourceRoot) {
        assert!(!self.is_full(), "source retirement preflight was skipped");
        let root_bytes = root.descriptor().bytes;
        assert!(
            root_bytes <= self.available_bytes(),
            "source retirement byte preflight was skipped"
        );
        let tail = (self.head + self.len) % SOURCE_RETIREMENT_QUEUE_CAPACITY;
        debug_assert!(self.slots[tail].is_none());
        self.slots[tail] = Some(root);
        self.len += 1;
        self.bytes += root_bytes;
    }

    fn pop(&mut self) -> Option<RetiredSourceRoot> {
        if self.len == 0 {
            return None;
        }
        let root = self.slots[self.head]
            .take()
            .expect("nonempty retirement queue owns its head");
        self.bytes -= root.descriptor().bytes;
        self.head = (self.head + 1) % SOURCE_RETIREMENT_QUEUE_CAPACITY;
        self.len -= 1;
        Some(root)
    }
}

/// Allocation-free FIFO drain of retired Crop-root ownership.
///
/// Each yielded item is non-cloneable and should be moved to the host-selected
/// destruction lane. Dropping this iterator early leaves unvisited roots in
/// the document queue.
#[derive(Debug)]
#[must_use = "drain items must be transferred to the host's source disposal lane"]
pub struct RetiredSourceRootDrain<'a> {
    queue: &'a mut RetiredSourceRootQueue,
}

impl Iterator for RetiredSourceRootDrain<'_> {
    type Item = RetiredSourceRoot;

    fn next(&mut self) -> Option<Self::Item> {
        self.queue.pop()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.queue.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for RetiredSourceRootDrain<'_> {}

#[derive(Debug)]
pub(crate) struct DocumentIdentityAllocator {
    next_block: Option<NonZeroU64>,
    next_coverage: Option<NonZeroU64>,
}

impl Default for DocumentIdentityAllocator {
    fn default() -> Self {
        Self {
            next_block: NonZeroU64::new(1),
            next_coverage: NonZeroU64::new(1),
        }
    }
}

impl DocumentIdentityAllocator {
    pub(crate) fn mint_block(
        &mut self,
        build: ArenaBuildId,
    ) -> Result<FreshBlockPermit, LiveDocumentError> {
        let id = take_monotonic(&mut self.next_block, EntityIdentityKind::Block)?;
        Ok(FreshBlockPermit {
            build,
            id: BlockId(id),
        })
    }

    pub(crate) fn mint_coverage(
        &mut self,
        build: ArenaBuildId,
    ) -> Result<FreshCoveragePermit, LiveDocumentError> {
        let id = take_monotonic(&mut self.next_coverage, EntityIdentityKind::Coverage)?;
        Ok(FreshCoveragePermit {
            build,
            id: CoverageId(id),
        })
    }
}

fn take_monotonic(
    next: &mut Option<NonZeroU64>,
    kind: EntityIdentityKind,
) -> Result<u64, LiveDocumentError> {
    let current = next.ok_or(LiveDocumentError::IdentityExhausted(kind))?;
    *next = current.get().checked_add(1).and_then(NonZeroU64::new);
    Ok(current.get())
}

/// Single-worker owner of source, parse, arena, and fresh-identity clocks for
/// one document.
///
/// No field is exposed independently. A caller can copy query epochs and
/// source snapshots, but cannot turn either into a parser lease or build
/// ticket.
#[derive(Debug)]
pub struct LiveDocumentStore {
    source: SourceStore,
    coordinator: Coordinator,
    arena: PageArena,
    identities: Option<DocumentIdentityAllocator>,
    candidate: Option<CandidateJob>,
    aborting: Vec<AbortingCandidate>,
    retired_source_roots: RetiredSourceRootQueue,
    latest_mechanism_document: Option<MechanismDocumentBinding>,
    #[cfg(feature = "exact-parser")]
    latest_restart_document: Option<RestartDocumentBinding>,
    #[cfg(feature = "exact-parser")]
    pending_restart_publication: Option<PendingRestartCompositePublication>,
    #[cfg(all(test, feature = "exact-parser"))]
    restart_publication_token_override: Option<ParseToken>,
    #[cfg(feature = "exact-parser")]
    #[allow(dead_code)]
    // Feasibility activation is integrated under tests before its production caller.
    next_retained_activation: NonZeroU64,
}

impl LiveDocumentStore {
    /// Creates an unparsed generation-zero bootstrap and immediately admits
    /// the exact generation-one parse of revision zero.
    pub fn new(text: &str, lineage_capacity: usize) -> Result<Self, LiveDocumentError> {
        let source = SourceStore::try_new(text, lineage_capacity)?;
        let mut arena = PageArena::new();
        let bootstrap = arena.allocate(BOOTSTRAP_PAYLOAD, &[])?.owner;
        let mut coordinator = Coordinator::new(source.root_id(), bootstrap);
        coordinator.begin_initial_parse()?;
        debug_assert_eq!(coordinator.arena_identity(), arena.identity());
        debug_assert_eq!(coordinator.source_revision(), source.revision());
        debug_assert_eq!(coordinator.source_root(), source.root_id());
        Ok(Self {
            source,
            coordinator,
            arena,
            identities: Some(DocumentIdentityAllocator::default()),
            candidate: None,
            aborting: Vec::new(),
            retired_source_roots: RetiredSourceRootQueue::new(),
            latest_mechanism_document: None,
            #[cfg(feature = "exact-parser")]
            latest_restart_document: None,
            #[cfg(feature = "exact-parser")]
            pending_restart_publication: None,
            #[cfg(all(test, feature = "exact-parser"))]
            restart_publication_token_override: None,
            #[cfg(feature = "exact-parser")]
            next_retained_activation: NonZeroU64::new(1).expect("one is nonzero"),
        })
    }

    #[must_use]
    pub fn active_parse_plan(&self) -> Option<ParsePlan> {
        self.coordinator.active_plan()
    }

    #[must_use]
    /// Returns only the coordinator's worker-current published output. An
    /// arena-committed output in `RestartCompositePublicationState::Held` is
    /// intentionally unreachable here and cannot be offered accidentally.
    pub fn current_output(&self) -> OutputRootLease {
        self.coordinator.current_output()
    }

    #[must_use]
    pub const fn source_revision(&self) -> SourceRevision {
        self.source.revision()
    }

    #[must_use]
    pub fn source_root(&self) -> SourceRootId {
        self.source.root_id()
    }

    #[must_use]
    pub fn source_descriptor(&self) -> SourceSnapshotDescriptor {
        self.source.descriptor()
    }

    #[must_use]
    pub fn clocks(&self) -> LiveDocumentClockSnapshot {
        LiveDocumentClockSnapshot {
            source: self.source.descriptor(),
            coordinator_revision: self.coordinator.source_revision(),
            coordinator_root: self.coordinator.source_root(),
            parse_generation: self.coordinator.parse_generation(),
            active: self.coordinator.active_plan().map(|plan| plan.token),
            queued: self.coordinator.queued_plan().map(|plan| plan.token),
        }
    }

    /// Returns a borrowed read-only view. It owns no Crop `Arc`, cannot mint an
    /// owning cursor, and its borrow prevents an edit from overlapping the
    /// observation.
    #[must_use]
    pub fn query_source(&self) -> SourceQueryView<'_> {
        self.source.query_view()
    }

    #[must_use]
    pub const fn retired_source_root_count(&self) -> usize {
        self.retired_source_roots.len()
    }

    #[must_use]
    pub const fn retired_source_root_capacity(&self) -> usize {
        SOURCE_RETIREMENT_QUEUE_CAPACITY
    }

    #[must_use]
    pub const fn retired_source_logical_bytes(&self) -> usize {
        self.retired_source_roots.bytes()
    }

    #[must_use]
    pub const fn retired_source_logical_byte_capacity(&self) -> usize {
        self.retired_source_roots.byte_capacity
    }

    /// Takes the oldest retired root without running its destructor. The
    /// caller receives the queue's linear ownership capability and chooses the
    /// native disposer or Web/Wasm idle lane where it is dropped.
    pub fn take_retired_source_root(&mut self) -> Option<RetiredSourceRoot> {
        self.retired_source_roots.pop()
    }

    /// Drains retired roots in edit order without allocating an intermediate
    /// collection or destroying any root inside this call.
    pub fn drain_retired_source_roots(&mut self) -> RetiredSourceRootDrain<'_> {
        RetiredSourceRootDrain {
            queue: &mut self.retired_source_roots,
        }
    }

    #[must_use]
    pub fn candidate_epoch(&self) -> Option<LiveCandidateEpoch> {
        self.candidate.as_ref().map(|candidate| candidate.epoch)
    }

    /// Issues exactly one parser source lease and one suspended arena build
    /// ticket after re-deriving every epoch from actor-owned state.
    pub fn begin_candidate(
        &mut self,
        token: ParseToken,
    ) -> Result<LiveCandidateEpoch, LiveDocumentError> {
        #[cfg(feature = "exact-parser")]
        if self.pending_restart_publication.is_some() {
            return Err(LiveDocumentError::RestartPublicationHeld);
        }
        self.require_current_token(token)?;
        if self.candidate.is_some() {
            return Err(LiveDocumentError::CandidateAlreadyActive);
        }
        // Reserve the candidate's eventual abort record before issuing either
        // linear source or arena authority. Cancellation and atomic edit
        // publication therefore contain no hidden Vec growth.
        self.aborting
            .try_reserve(1)
            .map_err(|_| LiveDocumentError::CancellationCapacityExhausted)?;

        let descriptor = self.current_source_descriptor();
        if token.source_revision != descriptor.revision || token.source_root != descriptor.root {
            return Err(LiveDocumentError::CandidateStale);
        }
        if self.coordinator.arena_identity() != self.arena.identity() {
            return Err(LiveDocumentError::Invariant(
                "coordinator and build arena identities diverged",
            ));
        }

        let lease = self.source.issue_parser_lease();
        let bound_descriptor = SourceSnapshotDescriptor {
            revision: lease.revision(),
            root: lease.identity(),
            bytes: lease.len_bytes(),
        };
        if bound_descriptor != descriptor {
            return Err(LiveDocumentError::Invariant(
                "parser lease changed during one actor turn",
            ));
        }
        let ticket = self.arena.begin_build()?;
        let build = ticket.id();
        let source = BoundSourceCursor {
            descriptor,
            total_utf16: lease.len_utf16(),
            cursor: lease.cursor(),
        };
        let epoch = LiveCandidateEpoch {
            token,
            source: descriptor,
            arena: self.arena.identity(),
            build,
        };
        let identities = self.identities.take().ok_or(LiveDocumentError::Invariant(
            "document identity allocator is already candidate-owned",
        ))?;
        self.candidate = Some(CandidateJob {
            epoch,
            raw_source: Some(source),
            ledger: None,
            projection_composer_admitted: false,
            ticket: Some(ticket),
            identities: Some(identities),
            writer: CandidateWriterSlot::None,
            #[cfg(feature = "exact-parser")]
            retained_activation: None,
            #[cfg(feature = "exact-parser")]
            persisted_restart: persisted_restart_activation::PersistedRestartSourcePhase::Inactive,
            commit_recovery: false,
        });
        self.require_live_epoch(epoch)?;
        Ok(epoch)
    }

    /// Streams one exact physical source byte from the candidate-owned cursor.
    /// Parser code never receives or clones the underlying source lease.
    pub fn poll_candidate_byte(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<Option<SourceByte>, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        self.require_no_retained_activation(epoch)?;
        let byte = self
            .candidate
            .as_mut()
            .expect("candidate was validated")
            .raw_source
            .as_mut()
            .ok_or(LiveDocumentError::CandidateSourceLedgerAlreadyActive)?
            .cursor
            .next_byte();
        self.require_live_epoch(epoch)?;
        Ok(byte)
    }

    pub fn candidate_cursor_offset(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<usize, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self
            .candidate
            .as_ref()
            .expect("candidate was validated")
            .cursor_offset())
    }

    /// Irreversibly selects the one-pass production source seam for this
    /// candidate. The raw-byte compatibility probe is unavailable afterward,
    /// so source bytes cannot flow through two parser-facing paths.
    pub fn activate_candidate_source_ledger(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        self.require_no_retained_activation(epoch)?;
        let (recognition_total_utf16, recognition_cursor) = self.source.issue_recognition_cursor();
        let candidate = self.candidate.as_mut().expect("candidate was validated");
        if candidate.ledger.is_some() {
            return Err(LiveDocumentError::CandidateSourceLedgerAlreadyActive);
        }
        let source = candidate
            .raw_source
            .take()
            .ok_or(LiveDocumentError::CandidateSourceLedgerAlreadyActive)?;
        if source.cursor.offset() != 0 {
            candidate.raw_source = Some(source);
            return Err(LiveDocumentError::CandidateSourceLedgerRequiresFreshCursor);
        }
        if recognition_total_utf16 != source.total_utf16 {
            candidate.raw_source = Some(source);
            return Err(LiveDocumentError::Invariant(
                "candidate source roles disagree on whole-root UTF-16 length",
            ));
        }
        candidate.ledger = Some(CandidateSourceLedger::new(
            epoch,
            source.descriptor,
            source.total_utf16,
            source.cursor,
            recognition_cursor,
        ));
        self.require_live_epoch(epoch)
    }

    /// Rebuilds the candidate source ledger from one acknowledged physical
    /// line boundary without retaining either decoder cursor or its chunk
    /// scratch.
    ///
    /// The continuation is intentionally never returned to parser-facing
    /// code. The actor validates the quiescent ledger while it is still live,
    /// verifies the Crop cut is a scalar-exact physical-line start, remints
    /// both cursor roles at that exact cut, and only then consumes/replaces
    /// the ledger. This is a same-build in-memory mechanism; durable restart
    /// still requires the composite writer/green checkpoint authority.
    #[allow(dead_code)] // The composite writer checkpoint will become the production caller.
    pub(crate) fn restart_candidate_source_ledger_at_line_boundary(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        let absolute_offset = {
            let ledger = self.candidate_ledger()?;
            ledger.validate_line_boundary_continuation(epoch)?;
            ledger
                .line_boundary_offset_for_actor(epoch)
                .map_err(LiveDocumentError::from)?
        };
        let offset = usize::try_from(absolute_offset).map_err(|_| {
            LiveDocumentError::SourceLedger(SourceBoundLedgerError::SourceLengthOverflow)
        })?;
        if self.source.descriptor() != epoch.source() {
            return Err(LiveDocumentError::CandidateStale);
        }
        let cursor_pair = self
            .source
            .issue_resume_cursor_pair(offset)
            .map_err(SourceStoreError::from)?;
        if cursor_pair.descriptor() != epoch.source() || cursor_pair.offset() != offset {
            return Err(LiveDocumentError::CandidateStale);
        }
        let authoritative_root_utf16 = cursor_pair.total_utf16();
        let physical_line_start = cursor_pair.is_physical_line_start();
        let (authoritative, recognition) = cursor_pair.into_cursors();

        let ledger = self
            .candidate
            .as_mut()
            .expect("candidate was validated")
            .ledger
            .take()
            .ok_or(LiveDocumentError::CandidateSourceLedgerNotActive)?;
        let continuation: CandidateSourceLineBoundaryContinuation = ledger
            .into_line_boundary_continuation(epoch)
            .expect("the unchanged ledger passed pre-take continuation validation");
        continuation.validate_resume_authority(
            epoch,
            authoritative_root_utf16,
            &authoritative,
            &recognition,
            physical_line_start,
        )?;
        let resumed = continuation.resume_with_validated_cursors(authoritative, recognition);
        self.candidate
            .as_mut()
            .expect("candidate was validated")
            .ledger = Some(resumed);
        self.require_live_epoch(epoch)
    }

    /// Performs at most `fuel` decoder work units and returns at most one
    /// complete UTF-8/source atom.
    pub fn poll_candidate_source(
        &mut self,
        epoch: LiveCandidateEpoch,
        fuel: usize,
    ) -> Result<CandidateSourcePoll, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self.candidate_ledger_mut()?.poll(epoch, fuel)?)
    }

    /// Polls the non-authoritative speculative cursor. Its atoms and
    /// checkpoints are recognition data only and cannot enter claim APIs.
    pub fn poll_candidate_recognition(
        &mut self,
        epoch: LiveCandidateEpoch,
        fuel: usize,
    ) -> Result<CandidateRecognitionPoll, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self.candidate_ledger_mut()?.poll_recognition(epoch, fuel)?)
    }

    pub fn candidate_recognition_checkpoint(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionCheckpoint, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self.candidate_ledger()?.recognition_checkpoint(epoch)?)
    }

    /// Seals one recognized line as the candidate's sole O(1) replay
    /// expectation. Recognition cannot advance again until authoritative
    /// replay finishes the exact same line.
    pub fn candidate_finish_recognition_line(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionLineReceipt, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self
            .candidate_ledger_mut()?
            .finish_recognition_line(epoch)?)
    }

    /// Begins one bounded multi-line recognition recipe. The range kind is
    /// scanner-family data only: rejected candidates still replay through the
    /// authoritative ledger as ordinary source/paragraph actions.
    pub fn candidate_begin_recognition_range(
        &mut self,
        epoch: LiveCandidateEpoch,
        kind: CandidateRecognitionRangeKind,
    ) -> Result<(), LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self
            .candidate_ledger_mut()?
            .begin_recognition_range(epoch, kind)?)
    }

    /// Advances an open recognition range by one complete physical line
    /// without retaining a queue of line receipts.
    pub fn candidate_continue_recognition_range_line(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionLineReceipt, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self
            .candidate_ledger_mut()?
            .continue_recognition_range_line(epoch)?)
    }

    /// Installs one O(1) summary as the sole replay expectation for the
    /// recognized range. The receipt is diagnostic output and cannot be fed
    /// back into any source, claim, or replay API.
    pub fn candidate_finish_recognition_range(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionRangeReceipt, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self
            .candidate_ledger_mut()?
            .finish_recognition_range(epoch)?)
    }

    /// Opens one semantic binding from a fresh ID minted inside the actor.
    /// Parser code supplies a kind, never a `BlockId`.
    pub fn candidate_open_binding(
        &mut self,
        epoch: LiveCandidateEpoch,
        kind: GreenKind,
    ) -> Result<CandidateOpenBinding, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        let permit = self.candidate_identities_mut()?.mint_block(epoch.build)?;
        Ok(self
            .candidate_ledger_mut()?
            .open_binding(epoch, permit, kind)?)
    }

    pub fn candidate_close_binding(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: &CandidateOpenBinding,
    ) -> Result<(), LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self.candidate_ledger_mut()?.close_binding(epoch, binding)?)
    }

    /// Claims the exact source interval from the current ordered cursor to a
    /// source-minted scalar boundary. Coverage identity is minted internally.
    #[allow(clippy::too_many_arguments)]
    pub fn candidate_claim_to(
        &mut self,
        epoch: LiveCandidateEpoch,
        boundary: CandidateSourceBoundary,
        owner: &CandidateOpenBinding,
        part: CoveragePart,
        logical: &CandidateLogicalAction,
        affinity: GreenAffinity,
    ) -> Result<ValidatedSourceClaim, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        let permit = self
            .candidate_identities_mut()?
            .mint_coverage(epoch.build)?;
        Ok(self
            .candidate_ledger_mut()?
            .claim_to(epoch, permit, boundary, owner, part, logical, affinity)?)
    }

    /// Production source consumption. Unlike the adjacent proof-harness API,
    /// this mints no coverage identity per decoder atom.
    #[allow(clippy::too_many_arguments)]
    pub fn candidate_consume_to(
        &mut self,
        epoch: LiveCandidateEpoch,
        boundary: CandidateSourceBoundary,
        owner: &CandidateOpenBinding,
        part: CoveragePart,
        logical: &CandidateLogicalAction,
    ) -> Result<ConsumedSourcePiece, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self
            .candidate_ledger_mut()?
            .consume_to(epoch, boundary, owner, part, logical)?)
    }

    /// Stages the exact typed line-ending atom. Its semantic policy can be
    /// resolved after parser lookahead, before any later claim is emitted.
    pub fn candidate_stage_terminator(
        &mut self,
        epoch: LiveCandidateEpoch,
        atom: &CandidateSourceAtom,
        terminal: &CandidateOpenBinding,
        affinity: GreenAffinity,
    ) -> Result<(), LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        let permit = self
            .candidate_identities_mut()?
            .mint_coverage(epoch.build)?;
        Ok(self
            .candidate_ledger_mut()?
            .stage_terminator(epoch, permit, atom, terminal, affinity)?)
    }

    pub fn candidate_stage_consumed_terminator(
        &mut self,
        epoch: LiveCandidateEpoch,
        atom: &CandidateSourceAtom,
        terminal: &CandidateOpenBinding,
    ) -> Result<(), LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self
            .candidate_ledger_mut()?
            .stage_consumed_terminator(epoch, atom, terminal)?)
    }

    pub fn candidate_resolve_terminator(
        &mut self,
        epoch: LiveCandidateEpoch,
        resolution: CandidateTerminatorResolution,
    ) -> Result<ValidatedSourceClaim, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self
            .candidate_ledger_mut()?
            .resolve_terminator(epoch, resolution)?)
    }

    pub fn candidate_resolve_consumed_terminator(
        &mut self,
        epoch: LiveCandidateEpoch,
        resolution: CandidateTerminatorResolution,
    ) -> Result<ConsumedSourcePiece, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self
            .candidate_ledger_mut()?
            .resolve_consumed_terminator(epoch, resolution)?)
    }

    /// Stages or O(1)-coalesces the exact current blank physical line. Only
    /// the first line mints a coverage ID; arbitrarily many adjacent blank
    /// lines extend the same pending range without retained line records.
    pub fn candidate_stage_blank_gap(
        &mut self,
        epoch: LiveCandidateEpoch,
        affinity: GreenAffinity,
    ) -> Result<(), LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        let needs_permit = !self.candidate_ledger()?.has_pending_gap();
        let permit = if needs_permit {
            Some(
                self.candidate_identities_mut()?
                    .mint_coverage(epoch.build)?,
            )
        } else {
            None
        };
        Ok(self
            .candidate_ledger_mut()?
            .stage_blank_gap(epoch, permit, affinity)?)
    }

    pub fn candidate_stage_consumed_blank_gap(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self
            .candidate_ledger_mut()?
            .stage_consumed_blank_gap(epoch)?)
    }

    pub fn candidate_resolve_blank_gap(
        &mut self,
        epoch: LiveCandidateEpoch,
        surviving_owner: &CandidateOpenBinding,
    ) -> Result<ValidatedSourceClaim, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self
            .candidate_ledger_mut()?
            .resolve_gap(epoch, surviving_owner)?)
    }

    pub fn candidate_resolve_consumed_blank_gap(
        &mut self,
        epoch: LiveCandidateEpoch,
        surviving_owner: &CandidateOpenBinding,
    ) -> Result<ConsumedSourcePiece, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self
            .candidate_ledger_mut()?
            .resolve_consumed_gap(epoch, surviving_owner)?)
    }

    /// Admits exactly one source-bound projection composer for this candidate.
    /// Its generation comes from the actor's parse clock, never caller input.
    pub fn begin_source_projection_composer(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<SourceBoundProjectionComposer, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        self.require_no_retained_activation(epoch)?;
        let candidate = self.candidate.as_mut().expect("candidate was validated");
        if candidate.ledger.is_none() {
            return Err(LiveDocumentError::CandidateSourceLedgerNotActive);
        }
        if candidate.projection_composer_admitted {
            return Err(LiveDocumentError::Invariant(
                "candidate projection composer already admitted",
            ));
        }
        let composer = SourceBoundProjectionComposer::begin(epoch);
        candidate.projection_composer_admitted = true;
        Ok(composer)
    }

    /// Irreversibly collapses the source ledger, projection composer, identity
    /// clocks, and resumable green builder behind the grammar-free writer
    /// boundary. Legacy claim/composer/ticket APIs become unavailable because
    /// their linear state is physically moved into the writer.
    pub fn activate_candidate_writer(
        &mut self,
        epoch: LiveCandidateEpoch,
        config: CandidateWriterConfig,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let candidate = self.candidate.as_mut().expect("candidate was validated");
        #[cfg(feature = "exact-parser")]
        if candidate.retained_activation.is_some() {
            return Err(CandidateWriterError::Busy);
        }
        if !matches!(candidate.writer, CandidateWriterSlot::None) {
            return Err(CandidateWriterError::Busy);
        }
        if candidate.projection_composer_admitted {
            return Err(CandidateWriterError::Invariant(
                "legacy projection composer was already admitted",
            ));
        }
        let ledger = candidate
            .ledger
            .as_ref()
            .ok_or(CandidateWriterError::Actor(
                LiveDocumentError::CandidateSourceLedgerNotActive,
            ))?;
        let ticket = candidate
            .ticket
            .as_ref()
            .ok_or(CandidateWriterError::Invariant("candidate ticket missing"))?;
        let spec = CandidateWriter::root_spec(epoch, ledger, config)?;
        let builder = ResumableSerializedGreenBuild::new(ticket, spec)?;

        let ledger = candidate.ledger.take().expect("ledger was checked");
        let ticket = candidate.ticket.take().expect("ticket was checked");
        let identities = candidate
            .identities
            .take()
            .ok_or(CandidateWriterError::Invariant(
                "candidate identity allocator missing",
            ))?;
        candidate.writer = CandidateWriterSlot::Active(Box::new(
            CandidateWriter::new(epoch, ledger, ticket, identities, builder)
                .expect("actor-derived writer parts share one exact epoch"),
        ));
        candidate.projection_composer_admitted = true;
        Ok(())
    }

    pub fn poll_candidate_writer_source(
        &mut self,
        epoch: LiveCandidateEpoch,
        fuel: usize,
    ) -> Result<CandidateWriterSourcePoll, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?.poll_source(epoch, fuel)
    }

    pub fn candidate_writer_recognition_checkpoint(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionCheckpoint, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        match &self
            .candidate
            .as_ref()
            .expect("candidate was validated")
            .writer
        {
            CandidateWriterSlot::Active(writer) => writer.recognition_checkpoint(epoch),
            CandidateWriterSlot::None => Err(CandidateWriterError::Invariant(
                "candidate writer is not active",
            )),
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::Paused(_) => Err(CandidateWriterError::Busy),
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::AdoptedTail(_) => Err(CandidateWriterError::Busy),
        }
    }

    /// Joins the active candidate's untouched recognition-line start with the
    /// current persistent source index in one non-interleavable actor read.
    ///
    /// A copied source descriptor or scalar offset is insufficient: the
    /// writer must still own the exact candidate/build at an untouched line
    /// start, and the source store must independently validate the complete
    /// `{revision, root, bytes}` snapshot before it resolves the physical end.
    pub fn candidate_writer_recognition_line_descriptor(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionLineDescriptor, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let checkpoint = match &self
            .candidate
            .as_ref()
            .expect("candidate was validated")
            .writer
        {
            CandidateWriterSlot::Active(writer) => {
                writer.recognition_line_start_checkpoint(epoch)?
            }
            CandidateWriterSlot::None => {
                return Err(CandidateWriterError::Invariant(
                    "candidate writer is not active",
                ));
            }
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::Paused(_) => return Err(CandidateWriterError::Busy),
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::AdoptedTail(_) => return Err(CandidateWriterError::Busy),
        };
        let start = usize::try_from(checkpoint.absolute_offset()).map_err(|_| {
            CandidateWriterError::SourceLedger(SourceBoundLedgerError::SourceLengthOverflow)
        })?;
        let physical_line = self
            .source
            .query_physical_line_descriptor(checkpoint.source(), start)
            .map_err(|error| CandidateWriterError::Actor(LiveDocumentError::Source(error)))?;
        if physical_line.start() != start
            || physical_line.source() != checkpoint.source()
            || checkpoint.build_id() != epoch.build_id()
        {
            return Err(CandidateWriterError::Invariant(
                "candidate recognition descriptor join changed identity",
            ));
        }
        Ok(CandidateRecognitionLineDescriptor {
            epoch,
            checkpoint,
            physical_line,
        })
    }

    /// Actor-joined EOF observation for a source-backed recognition client.
    ///
    /// This exposes no source byte and mints no claim authority. It exists so
    /// a byte-session client can distinguish the terminal empty cut from a
    /// nonempty indexed physical line without falling back to scalar parsing.
    pub fn candidate_writer_recognition_at_physical_eof(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<bool, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let checkpoint = match &self
            .candidate
            .as_ref()
            .expect("candidate was validated")
            .writer
        {
            CandidateWriterSlot::Active(writer) => {
                writer.recognition_line_start_checkpoint(epoch)?
            }
            CandidateWriterSlot::None => {
                return Err(CandidateWriterError::Invariant(
                    "candidate writer is not active",
                ));
            }
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::Paused(_) => return Err(CandidateWriterError::Busy),
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::AdoptedTail(_) => return Err(CandidateWriterError::Busy),
        };
        if checkpoint.source() != epoch.source() || checkpoint.build_id() != epoch.build_id() {
            return Err(CandidateWriterError::Invariant(
                "candidate recognition EOF join changed identity",
            ));
        }
        Ok(usize::try_from(checkpoint.absolute_offset()).ok() == Some(epoch.source().bytes))
    }

    /// Consumes one actor-joined untouched-line descriptor into the sole
    /// candidate-owned recognition-byte session. The descriptor is rejoined
    /// with the mutable writer state here, so a stale or crossed line fails
    /// before any byte is exposed.
    pub fn candidate_writer_begin_recognition_byte_session(
        &mut self,
        epoch: LiveCandidateEpoch,
        descriptor: CandidateRecognitionLineDescriptor,
    ) -> Result<CandidateRecognitionByteSession, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        if descriptor.epoch() != epoch {
            return Err(CandidateWriterError::Actor(
                LiveDocumentError::WrongCandidateEpoch,
            ));
        }
        let (bound_epoch, checkpoint, physical) = descriptor.into_parts();
        self.candidate_writer_mut()?.begin_recognition_byte_session(
            epoch,
            bound_epoch,
            checkpoint,
            physical,
        )
    }

    /// Borrows the active candidate's sequential byte source for one bounded
    /// scanner poll. Scanner state and its logical cursor remain caller-owned;
    /// the Crop cursor and physical high-water never escape the actor.
    pub fn poll_candidate_writer_recognition_byte_session<S: CandidateRecognitionByteScanner>(
        &mut self,
        epoch: LiveCandidateEpoch,
        session: CandidateRecognitionByteSession,
        fuel: usize,
        scanner: &mut S,
    ) -> Result<
        CandidateRecognitionBytePollReceipt,
        CandidateRecognitionBytePollError<CandidateWriterError, S::Error>,
    > {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)
            .map_err(CandidateRecognitionBytePollError::Infrastructure)?;
        if session.epoch() != epoch {
            return Err(CandidateRecognitionBytePollError::Infrastructure(
                CandidateWriterError::Actor(LiveDocumentError::WrongCandidateEpoch),
            ));
        }
        self.candidate_writer_mut()
            .map_err(CandidateRecognitionBytePollError::Infrastructure)?
            .poll_recognition_byte_session(epoch, session, fuel, scanner)
    }

    /// Finishes and advances exactly the actor-bound physical line, installing
    /// the normal authoritative replay expectation. There is intentionally no
    /// byte-session abandon operation; cancellation owns that transition.
    pub fn candidate_writer_finish_recognition_byte_session(
        &mut self,
        epoch: LiveCandidateEpoch,
        session: CandidateRecognitionByteSession,
    ) -> Result<CandidateRecognitionByteSessionFinishReceipt, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        if session.epoch() != epoch {
            return Err(CandidateWriterError::Actor(
                LiveDocumentError::WrongCandidateEpoch,
            ));
        }
        self.candidate_writer_mut()?
            .finish_recognition_byte_session(epoch, session)
    }

    pub fn poll_candidate_writer_recognition(
        &mut self,
        epoch: LiveCandidateEpoch,
        fuel: usize,
    ) -> Result<CandidateRecognitionPoll, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?.poll_recognition(epoch, fuel)
    }

    /// Pumps a bounded recognition window into parser-local scanner state.
    /// The sink sees no claim-capable boundary, and any sink failure poisons
    /// this unpublished writer because the speculative cursor may have moved.
    pub fn poll_candidate_writer_recognition_window<S: CandidateRecognitionSink>(
        &mut self,
        epoch: LiveCandidateEpoch,
        fuel: usize,
        sink: &mut S,
    ) -> Result<
        CandidateRecognitionWindowReceipt,
        CandidateRecognitionWindowError<CandidateWriterError, S::Error>,
    > {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)
            .map_err(CandidateRecognitionWindowError::Infrastructure)?;
        self.candidate_writer_mut()
            .map_err(CandidateRecognitionWindowError::Infrastructure)?
            .poll_recognition_window(epoch, fuel, sink)
    }

    pub fn candidate_writer_finish_recognition_line(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionLineReceipt, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?.finish_recognition_line(epoch)
    }

    pub fn candidate_writer_begin_recognition_range(
        &mut self,
        epoch: LiveCandidateEpoch,
        kind: CandidateRecognitionRangeKind,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .begin_recognition_range(epoch, kind)
    }

    pub fn candidate_writer_continue_recognition_range_line(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionLineReceipt, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .continue_recognition_range_line(epoch)
    }

    pub fn candidate_writer_finish_recognition_range(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateRecognitionRangeReceipt, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?.finish_recognition_range(epoch)
    }

    pub fn candidate_writer_finish_line(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateLineReceipt, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?.finish_line(epoch)
    }

    pub fn candidate_writer_start_open(
        &mut self,
        epoch: LiveCandidateEpoch,
        kind: GreenKind,
        facts: crate::FactsEnvelope,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?.start_open(epoch, kind, facts)
    }

    pub fn candidate_writer_start_open_list(
        &mut self,
        epoch: LiveCandidateEpoch,
        facts: GreenListOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?.start_open_list(epoch, facts)
    }

    pub fn candidate_writer_start_open_item(
        &mut self,
        epoch: LiveCandidateEpoch,
        facts: GreenItemOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?.start_open_item(epoch, facts)
    }

    pub fn candidate_writer_start_open_heading(
        &mut self,
        epoch: LiveCandidateEpoch,
        facts: GreenHeadingOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_open_heading(epoch, facts)
    }

    pub fn candidate_writer_start_open_table(
        &mut self,
        epoch: LiveCandidateEpoch,
        facts: GreenTableOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?.start_open_table(epoch, facts)
    }

    pub fn candidate_writer_start_open_table_row(
        &mut self,
        epoch: LiveCandidateEpoch,
        facts: GreenTableRowOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_open_table_row(epoch, facts)
    }

    pub fn candidate_writer_start_open_table_cell(
        &mut self,
        epoch: LiveCandidateEpoch,
        facts: GreenTableCellOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_open_table_cell(epoch, facts)
    }

    pub fn candidate_writer_start_open_fenced_code(
        &mut self,
        epoch: LiveCandidateEpoch,
        facts: GreenFencedCodeOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_open_fenced_code(epoch, facts)
    }

    pub(crate) fn candidate_writer_mark_fenced_code_boundary(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: &CandidateWriterBinding,
        boundary: crate::CandidateFencedCodeBoundary,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .mark_fenced_code_boundary(epoch, binding, boundary)
    }

    pub(crate) fn candidate_writer_start_promote_setext(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: CandidateWriterBinding,
        facts: GreenHeadingOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_promote_setext(epoch, binding, facts)
    }

    pub(crate) fn candidate_writer_start_promote_table_header(
        &mut self,
        epoch: LiveCandidateEpoch,
        paragraph: CandidateWriterBinding,
        facts: GreenTableOpenFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_promote_table_header(epoch, paragraph, facts)
    }

    pub(crate) fn candidate_writer_supply_table_header_input(
        &mut self,
        epoch: LiveCandidateEpoch,
        input: crate::CandidateTableHeaderInput,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .supply_table_header_input(epoch, input)
    }

    pub fn candidate_writer_start_consume(
        &mut self,
        epoch: LiveCandidateEpoch,
        atom: CandidateWriterSourceAtom,
        owner: &CandidateWriterBinding,
        part: CoveragePart,
        logical: CandidateWriterLogicalAction<'_>,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_consume(epoch, atom, owner, part, logical)
    }

    pub fn candidate_writer_start_identity_line_replay(
        &mut self,
        epoch: LiveCandidateEpoch,
        terminal: &CandidateWriterBinding,
        part: CoveragePart,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_identity_line_replay(epoch, terminal, part)
    }

    pub fn candidate_writer_start_range_replay(
        &mut self,
        epoch: LiveCandidateEpoch,
        physical_owner: &CandidateWriterBinding,
        part: CoveragePart,
        physical_bytes: u64,
        recipe: crate::CandidateWriterRangeRecipe,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?.start_range_replay(
            epoch,
            physical_owner,
            part,
            physical_bytes,
            recipe,
        )
    }

    pub fn candidate_writer_stage_terminator(
        &mut self,
        epoch: LiveCandidateEpoch,
        atom: CandidateWriterSourceAtom,
        terminal: &CandidateWriterBinding,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .stage_terminator(epoch, atom, terminal)
    }

    pub fn candidate_writer_start_resolve_terminator(
        &mut self,
        epoch: LiveCandidateEpoch,
        resolution: CandidateTerminatorResolution,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_resolve_terminator(epoch, resolution)
    }

    pub fn candidate_writer_defer_blank_gap_atom(
        &mut self,
        epoch: LiveCandidateEpoch,
        atom: CandidateWriterSourceAtom,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .defer_blank_gap_atom(epoch, atom)
    }

    pub fn candidate_writer_stage_blank_gap(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?.stage_blank_gap(epoch)
    }

    pub fn candidate_writer_start_resolve_blank_gap(
        &mut self,
        epoch: LiveCandidateEpoch,
        surviving_owner: &CandidateWriterBinding,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_resolve_blank_gap(epoch, surviving_owner)
    }

    pub fn candidate_writer_start_close(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: CandidateWriterBinding,
        closed: crate::ClosedChildAggregate,
        last_line_blank: bool,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_close(epoch, binding, closed, last_line_blank)
    }

    pub fn candidate_writer_start_close_with_facts(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: CandidateWriterBinding,
        closed: crate::ClosedChildAggregate,
        last_line_blank: bool,
        facts: GreenCloseFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?.start_close_with_facts(
            epoch,
            binding,
            closed,
            last_line_blank,
            facts,
        )
    }

    #[cfg(test)]
    pub(crate) fn candidate_writer_start_close_fenced_code_with_test_facts(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: CandidateWriterBinding,
        closed: crate::ClosedChildAggregate,
        last_line_blank: bool,
        facts: GreenFencedCodeCloseFacts,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_close_fenced_code_with_test_facts(epoch, binding, closed, last_line_blank, facts)
    }

    pub(crate) fn candidate_writer_start_close_fenced_code(
        &mut self,
        epoch: LiveCandidateEpoch,
        binding: CandidateWriterBinding,
        closed: crate::ClosedChildAggregate,
        last_line_blank: bool,
        fence_closed: bool,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?.start_close_fenced_code(
            epoch,
            binding,
            closed,
            last_line_blank,
            fence_closed,
        )
    }

    pub fn candidate_writer_start_finish(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?.start_finish(epoch)
    }

    #[cfg(feature = "exact-parser")]
    pub(crate) fn candidate_writer_start_line_boundary_checkpoint(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateLineBoundaryCheckpointAdmission, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_line_boundary_checkpoint(epoch)
    }

    #[cfg(feature = "exact-parser")]
    pub(crate) fn candidate_writer_start_convergence_line_boundary_checkpoint(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateLineBoundaryCheckpointAdmission, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_convergence_line_boundary_checkpoint(epoch)
    }

    /// Atomically moves the writer from Active to Paused only after the exact
    /// parser/source/composer/green/binding join succeeds. The returned token
    /// owns no arena or source capability; generic document cancellation
    /// remains authoritative while it is live.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn pause_candidate_writer_at_line_boundary(
        &mut self,
        epoch: LiveCandidateEpoch,
        parser: ParserLineBoundaryCheckpointAuthority,
        bindings: &[CandidateWriterBinding],
    ) -> Result<
        SameBuildLineBoundaryCheckpoint,
        Box<(ParserLineBoundaryCheckpointAuthority, CandidateWriterError)>,
    > {
        if let Err(error) = self.require_live_epoch(epoch) {
            return Err(Box::new((parser, CandidateWriterError::Actor(error))));
        }
        let writer = {
            let candidate = self.candidate.as_mut().expect("candidate was validated");
            match std::mem::replace(&mut candidate.writer, CandidateWriterSlot::None) {
                CandidateWriterSlot::Active(writer) => writer,
                CandidateWriterSlot::None => {
                    return Err(Box::new((
                        parser,
                        CandidateWriterError::Invariant("candidate writer is not active"),
                    )));
                }
                CandidateWriterSlot::Paused(writer) => {
                    candidate.writer = CandidateWriterSlot::Paused(writer);
                    return Err(Box::new((parser, CandidateWriterError::Busy)));
                }
                CandidateWriterSlot::AdoptedTail(writer) => {
                    candidate.writer = CandidateWriterSlot::AdoptedTail(writer);
                    return Err(Box::new((parser, CandidateWriterError::Busy)));
                }
            }
        };
        let continuation = match (*writer).into_line_boundary_continuation() {
            Ok(continuation) => continuation,
            Err(failure) => {
                self.candidate
                    .as_mut()
                    .expect("candidate remains active")
                    .writer = CandidateWriterSlot::Active(Box::new(failure.writer));
                return Err(Box::new((parser, failure.error)));
            }
        };
        match SameBuildLineBoundaryCheckpoint::join(parser, &continuation, bindings) {
            Ok(checkpoint) => {
                self.candidate
                    .as_mut()
                    .expect("candidate remains active")
                    .writer = CandidateWriterSlot::Paused(Box::new(continuation));
                Ok(checkpoint)
            }
            Err(failure) => {
                let crate::same_build_checkpoint::SameBuildLineBoundaryJoinFailure {
                    parser,
                    error: pairing_error,
                } = *failure;
                let offset = continuation.cursor_offset().unwrap_or(usize::MAX);
                let pair = match self.source.issue_resume_cursor_pair(offset) {
                    Ok(pair) => pair,
                    Err(error) => {
                        self.candidate
                            .as_mut()
                            .expect("candidate remains active")
                            .writer = CandidateWriterSlot::Paused(Box::new(continuation));
                        return Err(Box::new((
                            parser,
                            CandidateWriterError::Actor(LiveDocumentError::Source(
                                SourceStoreError::from(error),
                            )),
                        )));
                    }
                };
                match continuation.resume_with_cursor_pair(pair) {
                    Ok(writer) => {
                        self.candidate
                            .as_mut()
                            .expect("candidate remains active")
                            .writer = CandidateWriterSlot::Active(Box::new(writer));
                        Err(Box::new((parser, pairing_error)))
                    }
                    Err(resume) => {
                        self.candidate
                            .as_mut()
                            .expect("candidate remains active")
                            .writer = CandidateWriterSlot::Paused(Box::new(resume.continuation));
                        Err(Box::new((parser, resume.error)))
                    }
                }
            }
        }
    }

    /// Moves the already parser-joined paused writer into the distinct
    /// source/composer adopted-tail state. Ordinary resume and commit APIs are
    /// disabled afterward; only the future green/index splice or generic
    /// candidate cancellation may consume the state.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn begin_zero_restart_tail_lineage_mechanism_only(
        &self,
        tail: &GreenSourceTailAdoptionCapability,
    ) -> Result<LineageAdoptionBundleJob, TailAdoptionJoinError> {
        tail.begin_zero_restart_lineage(&self.source)
    }

    /// Mechanism-only actor join for the byte-0 proof. The future production
    /// entry point consumes the parent-selected restart/convergence authority
    /// instead of exposing separate lineage construction.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn join_zero_restart_tail_to_candidate_mechanism_only(
        &self,
        epoch: LiveCandidateEpoch,
        tail: GreenSourceTailAdoptionCapability,
        lineage: LineageAdoptionBundleProof,
    ) -> Result<SourceBoundGreenTailAdoption, TailAdoptionJoinError> {
        if self.require_live_epoch(epoch).is_err() {
            return Err(TailAdoptionJoinError::WrongCandidate);
        }
        tail.join_current_source(&self.source, epoch, lineage)
    }

    #[cfg(feature = "exact-parser")]
    pub(crate) fn adopt_candidate_writer_source_composer_tail(
        &mut self,
        epoch: LiveCandidateEpoch,
        tail: SourceBoundGreenTailAdoption,
    ) -> Result<CandidateWriterTailAdoptionReceipt, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let continuation = {
            let candidate = self.candidate.as_mut().expect("candidate was validated");
            match std::mem::replace(&mut candidate.writer, CandidateWriterSlot::None) {
                CandidateWriterSlot::Paused(writer) => writer,
                CandidateWriterSlot::Active(writer) => {
                    candidate.writer = CandidateWriterSlot::Active(writer);
                    return Err(CandidateWriterError::Busy);
                }
                CandidateWriterSlot::AdoptedTail(writer) => {
                    candidate.writer = CandidateWriterSlot::AdoptedTail(writer);
                    return Err(CandidateWriterError::Busy);
                }
                CandidateWriterSlot::None => {
                    return Err(CandidateWriterError::Invariant(
                        "candidate paused writer is missing",
                    ));
                }
            }
        };
        match (*continuation).seal_source_composer_adopted_tail(tail) {
            Ok(ready) => {
                let receipt = ready.receipt();
                self.candidate
                    .as_mut()
                    .expect("candidate remains active")
                    .writer = CandidateWriterSlot::AdoptedTail(Box::new(ready));
                Ok(receipt)
            }
            Err(failure) => {
                let failure = *failure;
                self.candidate
                    .as_mut()
                    .expect("candidate remains active")
                    .writer = CandidateWriterSlot::Paused(Box::new(failure.continuation));
                // The rejected tail is linear source/storage authority and is
                // burned here; the paused candidate remains resumable/cancellable.
                drop(failure.tail);
                Err(failure.error)
            }
        }
    }

    #[cfg(feature = "exact-parser")]
    pub(crate) fn resume_candidate_writer_at_line_boundary(
        &mut self,
        epoch: LiveCandidateEpoch,
        checkpoint: &SameBuildLineBoundaryCheckpoint,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        if checkpoint.epoch() != epoch {
            return Err(CandidateWriterError::WrongCandidate);
        }
        let offset = match &self
            .candidate
            .as_ref()
            .expect("candidate was validated")
            .writer
        {
            CandidateWriterSlot::Paused(writer) => writer.cursor_offset()?,
            CandidateWriterSlot::Active(_) => return Err(CandidateWriterError::Busy),
            CandidateWriterSlot::AdoptedTail(_) => return Err(CandidateWriterError::Busy),
            CandidateWriterSlot::None => {
                return Err(CandidateWriterError::Invariant(
                    "candidate paused writer is missing",
                ));
            }
        };
        let pair = self
            .source
            .issue_resume_cursor_pair(offset)
            .map_err(SourceStoreError::from)
            .map_err(LiveDocumentError::Source)
            .map_err(CandidateWriterError::Actor)?;
        let continuation = {
            let candidate = self.candidate.as_mut().expect("candidate was validated");
            match std::mem::replace(&mut candidate.writer, CandidateWriterSlot::None) {
                CandidateWriterSlot::Paused(writer) => writer,
                CandidateWriterSlot::Active(writer) => {
                    candidate.writer = CandidateWriterSlot::Active(writer);
                    return Err(CandidateWriterError::Busy);
                }
                CandidateWriterSlot::AdoptedTail(writer) => {
                    candidate.writer = CandidateWriterSlot::AdoptedTail(writer);
                    return Err(CandidateWriterError::Busy);
                }
                CandidateWriterSlot::None => {
                    return Err(CandidateWriterError::Invariant(
                        "candidate paused writer is missing",
                    ));
                }
            }
        };
        match (*continuation).resume_with_cursor_pair(pair) {
            Ok(writer) => {
                self.candidate
                    .as_mut()
                    .expect("candidate remains active")
                    .writer = CandidateWriterSlot::Active(Box::new(writer));
                Ok(())
            }
            Err(failure) => {
                self.candidate
                    .as_mut()
                    .expect("candidate remains active")
                    .writer = CandidateWriterSlot::Paused(Box::new(failure.continuation));
                Err(failure.error)
            }
        }
    }

    /// Read-only capture of the narrow in-memory Setext restart draft from the
    /// actor's already-paused writer. The paused continuation remains the sole
    /// same-build resume/cancel capability.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn capture_candidate_writer_in_memory_setext_checkpoint(
        &self,
        epoch: LiveCandidateEpoch,
        bindings: &[CandidateWriterBinding],
        joined_donor: JoinedParserDonorSample,
    ) -> Result<InMemorySetextCheckpointDraft, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let candidate = self.candidate.as_ref().expect("candidate was validated");
        let CandidateWriterSlot::Paused(writer) = &candidate.writer else {
            return Err(CandidateWriterError::Invariant(
                "retained Setext capture requires the paused writer",
            ));
        };
        writer.capture_in_memory_setext_checkpoint(bindings, joined_donor)
    }

    /// Starts the sole sparse donor-sample chain from the actor's paused,
    /// fully joined parser/writer checkpoint.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn capture_candidate_writer_first_donor_checkpoint_sample(
        &mut self,
        epoch: LiveCandidateEpoch,
        bindings: &[CandidateWriterBinding],
        joined_donor: JoinedParserDonorSample,
    ) -> Result<CapturedDonorCheckpointSample, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let candidate = self.candidate.as_mut().expect("candidate was validated");
        let CandidateWriterSlot::Paused(writer) = &mut candidate.writer else {
            return Err(CandidateWriterError::Invariant(
                "donor checkpoint capture requires the paused writer",
            ));
        };
        writer.capture_first_donor_checkpoint_sample(bindings, joined_donor)
    }

    /// Consumes and remints the sparse donor cursor at a later joined cut.
    /// Actor-slot failures return the unchanged cursor just like writer-side
    /// validation or allocation failures.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn capture_candidate_writer_successive_donor_checkpoint_sample(
        &mut self,
        epoch: LiveCandidateEpoch,
        bindings: &[CandidateWriterBinding],
        joined_donor: JoinedParserDonorSample,
        cursor: DonorCheckpointSampleCursor,
    ) -> Result<CapturedDonorCheckpointSample, Box<DonorCheckpointSampleCaptureFailure>> {
        if let Err(error) = self.require_live_epoch(epoch) {
            return Err(Box::new(DonorCheckpointSampleCaptureFailure {
                error: CandidateWriterError::Actor(error),
                cursor,
            }));
        }
        let candidate = self.candidate.as_mut().expect("candidate was validated");
        let CandidateWriterSlot::Paused(writer) = &mut candidate.writer else {
            return Err(Box::new(DonorCheckpointSampleCaptureFailure {
                error: CandidateWriterError::Invariant(
                    "donor checkpoint capture requires the paused writer",
                ),
                cursor,
            }));
        };
        writer.capture_successive_donor_checkpoint_sample(bindings, joined_donor, cursor)
    }

    /// Captures one live suffix sample from the parent-seeded chain. No cursor
    /// or coordinate is accepted from the scheduler; the paused writer owns
    /// both the retained origin and every continuation.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn capture_candidate_writer_parent_selected_suffix_sample(
        &mut self,
        epoch: LiveCandidateEpoch,
        bindings: &[CandidateWriterBinding],
        joined_donor: JoinedParserDonorSample,
    ) -> Result<CapturedParentSelectedSuffixSample, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let candidate = self.candidate.as_mut().expect("candidate was validated");
        let CandidateWriterSlot::Paused(writer) = &mut candidate.writer else {
            return Err(CandidateWriterError::Invariant(
                "parent-selected suffix capture requires the paused writer",
            ));
        };
        writer.capture_parent_selected_suffix_sample(bindings, joined_donor)
    }

    /// Consumes the opaque rejection half of one live donor mismatch and
    /// rewinds only the paused writer's sparse-sample transaction.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn reject_candidate_writer_parent_selected_suffix_sample(
        &mut self,
        epoch: LiveCandidateEpoch,
        rejected: crate::ParentSelectedRejectedSuffixSample,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let candidate = self.candidate.as_mut().expect("candidate was validated");
        let CandidateWriterSlot::Paused(writer) = &mut candidate.writer else {
            return Err(CandidateWriterError::Invariant(
                "suffix probe rejection requires the paused writer",
            ));
        };
        writer.reject_parent_selected_suffix_sample(rejected)
    }

    /// Selects the first old convergence checkpoint through the actor-owned
    /// paused writer. The exact suspended ticket stays inside the candidate;
    /// the scheduler receives only an opaque parent-bound step.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn begin_candidate_writer_parent_selected_old_convergence(
        &self,
        epoch: LiveCandidateEpoch,
        tail: &ParentSelectedCandidateAdoptionTail,
    ) -> Result<ParentBoundDonorSuccessorStep, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let candidate = self.candidate.as_ref().expect("candidate was validated");
        let CandidateWriterSlot::Paused(writer) = &candidate.writer else {
            return Err(CandidateWriterError::Invariant(
                "old convergence selection requires the paused writer",
            ));
        };
        writer.begin_parent_selected_old_convergence(&self.arena, tail)
    }

    /// Advances the old retained checkpoint chain after a candidate-observed
    /// mismatch. This cannot accept the fresh-sample cursor by construction.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn advance_candidate_writer_parent_selected_old_convergence(
        &self,
        epoch: LiveCandidateEpoch,
        tail: &ParentSelectedCandidateAdoptionTail,
        current: ParentBoundDonorSuccessor,
    ) -> Result<ParentBoundDonorSuccessorStep, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let candidate = self.candidate.as_ref().expect("candidate was validated");
        let CandidateWriterSlot::Paused(writer) = &candidate.writer else {
            return Err(CandidateWriterError::Invariant(
                "old convergence advance requires the paused writer",
            ));
        };
        writer.advance_parent_selected_old_convergence(&self.arena, tail, current)
    }

    /// Crosses one typed immediate donor-partition transition while the same
    /// parser/writer checkpoint remains paused at R.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn advance_candidate_writer_parent_selected_old_convergence_partition(
        &self,
        epoch: LiveCandidateEpoch,
        tail: &ParentSelectedCandidateAdoptionTail,
        transition: crate::committed_checkpoint_index::ParentBoundDonorPartitionTransition,
    ) -> Result<ParentBoundDonorSuccessorStep, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let candidate = self.candidate.as_ref().expect("candidate was validated");
        let CandidateWriterSlot::Paused(writer) = &candidate.writer else {
            return Err(CandidateWriterError::Invariant(
                "old convergence partition transition requires the paused writer",
            ));
        };
        writer.advance_parent_selected_old_convergence_partition(&self.arena, tail, transition)
    }

    /// Resolves semantic A from the retained green child and starts the
    /// immutable A-to-current mapping while the exact parser/writer remain
    /// jointly paused at R.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn begin_parent_selected_convergence_mapping(
        &self,
        epoch: LiveCandidateEpoch,
        tail: &ParentSelectedCandidateAdoptionTail,
        old_convergence: ParentBoundDonorSuccessor,
    ) -> Result<ParentSelectedConvergenceMapStart, ParentSelectedConvergenceMapError> {
        self.require_live_epoch(epoch)
            .map_err(|_| ParentSelectedConvergenceMapError::SourceAdvanced)?;
        let candidate = self.candidate.as_ref().expect("candidate was validated");
        let CandidateWriterSlot::Paused(writer) = &candidate.writer else {
            return Err(ParentSelectedConvergenceMapError::Invariant(
                "convergence mapping must begin at a joined paused R",
            ));
        };
        writer.begin_parent_selected_convergence_mapping(
            &self.arena,
            &self.source,
            tail,
            old_convergence,
        )
    }

    /// Advances at most `fuel` retained edit records. Actor freshness and the
    /// frozen current source are rechecked before mapped C can be returned.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn poll_parent_selected_convergence_mapping(
        &self,
        epoch: LiveCandidateEpoch,
        job: &mut ParentSelectedConvergenceMapJob,
        fuel: usize,
    ) -> Result<ParentSelectedConvergenceMapProgress, ParentSelectedConvergenceMapError> {
        self.require_live_epoch(epoch)
            .map_err(|_| ParentSelectedConvergenceMapError::SourceAdvanced)?;
        let candidate = self.candidate.as_ref().expect("candidate was validated");
        if !matches!(candidate.writer, CandidateWriterSlot::Paused(_)) {
            return Err(ParentSelectedConvergenceMapError::Invariant(
                "convergence mapping lost its paused R checkpoint",
            ));
        }
        job.poll(&self.source, fuel)
    }

    /// Actor-side opaque target comparison. The joined paused writer is used
    /// before resume; the active writer is used while replaying toward C.
    /// Mapped C never becomes a scheduler offset.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn candidate_writer_parent_selected_convergence_relation(
        &self,
        epoch: LiveCandidateEpoch,
        mapped: &ParentSelectedMappedConvergence,
    ) -> Result<ParentSelectedConvergenceTargetRelation, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let candidate = self.candidate.as_ref().expect("candidate was validated");
        match &candidate.writer {
            CandidateWriterSlot::Active(writer) => {
                writer.relation_to_parent_selected_convergence(mapped)
            }
            CandidateWriterSlot::Paused(writer) => {
                writer.relation_to_parent_selected_convergence(mapped)
            }
            CandidateWriterSlot::AdoptedTail(_) | CandidateWriterSlot::None => {
                Err(CandidateWriterError::Invariant(
                    "mapped convergence comparison requires an exact writer",
                ))
            }
        }
    }

    /// Preflights the first direct splice lane before source/composer tail
    /// adoption becomes one-way. An ineligible shape leaves the paused writer
    /// untouched so the exact driver can advance old C or parse the suffix.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn candidate_writer_parent_selected_tail_splice_is_eligible(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<bool, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let candidate = self.candidate.as_ref().expect("candidate was validated");
        let CandidateWriterSlot::Paused(writer) = &candidate.writer else {
            return Err(CandidateWriterError::Invariant(
                "tail splice preflight requires the paused convergence writer",
            ));
        };
        Ok(writer.parent_selected_direct_tail_splice_is_eligible())
    }

    /// Validates the exact retained-green suffix against the current writer's
    /// prefix snapshot before source/composer adoption becomes one-way. The
    /// paused writer and both linear authorities remain untouched.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn candidate_writer_parent_selected_green_suffix_preflight(
        &self,
        epoch: LiveCandidateEpoch,
        parent_tail: &ParentSelectedCandidateAdoptionTail,
        mapped_tail: &SourceBoundGreenTailAdoption,
    ) -> Result<crate::GreenJournalSuffixPreflight, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let candidate = self.candidate.as_ref().expect("candidate was validated");
        let CandidateWriterSlot::Paused(writer) = &candidate.writer else {
            return Err(CandidateWriterError::Invariant(
                "green suffix preflight requires the paused convergence writer",
            ));
        };
        writer.preflight_parent_selected_green_suffix(&self.arena, parent_tail, mapped_tail)
    }

    /// Begins the explicitly in-memory 4/5 Setext activation proof. The old
    /// mechanism document remains caller-borrowed; production will replace
    /// that borrow with an actor-owned composite-root lease.
    #[cfg(feature = "exact-parser")]
    #[allow(dead_code)] // Executed by the integrated feasibility gate before product-root wiring.
    pub(crate) fn begin_in_memory_setext_activation<'old>(
        &mut self,
        epoch: LiveCandidateEpoch,
        joined: JoinedInMemorySetextRestart,
        old_document: &'old crate::CandidateWriterBuiltDocument,
        config: CandidateWriterConfig,
    ) -> Result<InMemorySetextActivationJob<'old>, InMemorySetextActivationError> {
        self.validate_in_memory_setext_activation_provenance(
            epoch,
            joined.old_epoch(),
            joined.old_binding(),
        )?;
        let candidate = self.candidate.as_ref().expect("candidate was validated");
        if candidate.ledger.is_some()
            || candidate.projection_composer_admitted
            || !matches!(candidate.writer, CandidateWriterSlot::None)
            || candidate.retained_activation.is_some()
            || candidate.commit_recovery
        {
            return Err(InMemorySetextActivationError::Writer(
                CandidateWriterError::Busy,
            ));
        }
        let source = candidate
            .raw_source
            .as_ref()
            .ok_or(InMemorySetextActivationError::Writer(
                CandidateWriterError::Invariant(
                    "retained activation requires the untouched fresh source cursor",
                ),
            ))?;
        if source.cursor.offset() != 0 || source.descriptor != epoch.source() {
            return Err(InMemorySetextActivationError::Writer(
                CandidateWriterError::WrongCandidate,
            ));
        }
        let ticket = candidate
            .ticket
            .as_ref()
            .ok_or(InMemorySetextActivationError::Writer(
                CandidateWriterError::Invariant("retained activation candidate ticket is missing"),
            ))?;
        let source_utf16 = u64::try_from(source.total_utf16).map_err(|_| {
            InMemorySetextActivationError::Writer(CandidateWriterError::Invariant(
                "retained activation source UTF-16 length exceeds u64",
            ))
        })?;
        let spec = CandidateWriter::root_spec_from_source(epoch, source_utf16, config)?;
        let activation_id = self.next_retained_activation;
        let next_activation = activation_id
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or(InMemorySetextActivationError::InvalidState(
                "retained activation identity exhausted",
            ))?;
        let job = InMemorySetextActivationJob::try_new(
            &self.source,
            ticket,
            &self.arena,
            old_document,
            epoch,
            activation_id,
            joined,
            spec,
        )?;
        self.candidate
            .as_mut()
            .expect("candidate remains active")
            .retained_activation = Some(activation_id);
        self.next_retained_activation = next_activation;
        Ok(job)
    }

    /// Advances lineage first, then resumes and validates the exact donor,
    /// and only then permits one retained-green arena mutation. This ordering
    /// keeps all pre-donor failures safe for ordinary full-parse fallback.
    #[cfg(feature = "exact-parser")]
    #[allow(dead_code, clippy::too_many_lines)] // Phase ordering is kept explicit for the proof audit.
    pub(crate) fn poll_in_memory_setext_activation(
        &mut self,
        epoch: LiveCandidateEpoch,
        job: &mut InMemorySetextActivationJob<'_>,
        lineage_fuel: usize,
    ) -> Result<InMemorySetextActivationProgress, Box<InMemorySetextActivationFailure>> {
        if let Err(error) =
            self.require_in_memory_setext_activation_slot(epoch, job.activation_id())
        {
            return Err(Box::new(InMemorySetextActivationFailure {
                error,
                abort: None,
                cleanup_error: None,
            }));
        }
        if let Err(error) = self.validate_in_memory_setext_activation_provenance(
            epoch,
            job.old_epoch(),
            job.old_binding(),
        ) {
            if job.green_started() {
                return Err(self.abort_dirty_in_memory_setext_activation(epoch, error));
            }
            self.clear_pristine_in_memory_setext_activation(epoch, job.activation_id());
            return Err(Box::new(InMemorySetextActivationFailure {
                error,
                abort: None,
                cleanup_error: None,
            }));
        }
        match job.poll_coordinate(&self.source, lineage_fuel) {
            Ok(InMemorySetextActivationProgress::Pending) => {
                return Ok(InMemorySetextActivationProgress::Pending);
            }
            Ok(InMemorySetextActivationProgress::Ready) => {}
            Err(error) => {
                self.clear_pristine_in_memory_setext_activation(epoch, job.activation_id());
                return Err(Box::new(InMemorySetextActivationFailure {
                    error,
                    abort: None,
                    cleanup_error: None,
                }));
            }
        }
        let preparation = {
            let ticket = self
                .candidate
                .as_ref()
                .and_then(|candidate| candidate.ticket.as_ref())
                .ok_or_else(|| {
                    Box::new(InMemorySetextActivationFailure {
                        error: InMemorySetextActivationError::Writer(
                            CandidateWriterError::Invariant(
                                "retained activation candidate ticket is missing",
                            ),
                        ),
                        abort: None,
                        cleanup_error: None,
                    })
                })?;
            job.prepare_source_and_donor(ticket)
        };
        if let Err(error) = preparation {
            self.clear_pristine_in_memory_setext_activation(epoch, job.activation_id());
            return Err(Box::new(InMemorySetextActivationFailure {
                error,
                abort: None,
                cleanup_error: None,
            }));
        }

        let ticket = self
            .candidate
            .as_mut()
            .expect("candidate was validated")
            .ticket
            .take()
            .expect("retained activation ticket was preflighted");
        // Complete the arena-borrowing slice before any whole-actor cleanup
        // helper is called; an Err-shaped resume result still carries the
        // session lifetime in its Ok variant until the match is consumed.
        let green_slice = match self.arena.resume_build(ticket) {
            Ok(mut session) => {
                let green_result = job.poll_green(&mut session);
                let suspended = session.suspend();
                Ok((green_result, suspended))
            }
            Err(failure) => Err(failure),
        };
        let (green_result, suspended) = match green_slice {
            Ok(slice) => slice,
            Err(failure) => {
                self.candidate
                    .as_mut()
                    .expect("candidate remains active")
                    .ticket = Some(failure.ticket);
                if job.green_started() {
                    return Err(self.abort_dirty_in_memory_setext_activation(
                        epoch,
                        InMemorySetextActivationError::Writer(CandidateWriterError::ArenaBuild(
                            failure.error,
                        )),
                    ));
                }
                self.clear_pristine_in_memory_setext_activation(epoch, job.activation_id());
                return Err(Box::new(InMemorySetextActivationFailure {
                    error: InMemorySetextActivationError::Writer(CandidateWriterError::ArenaBuild(
                        failure.error,
                    )),
                    abort: None,
                    cleanup_error: None,
                }));
            }
        };
        match suspended {
            Ok(ticket) => {
                self.candidate
                    .as_mut()
                    .expect("candidate remains active")
                    .ticket = Some(ticket);
            }
            Err(error) => {
                // Failed suspension transitions the arena session to abort on
                // drop. Detach the candidate and return allocator ownership;
                // the caller receives the fuelled abort identity.
                let candidate = self.candidate.take().expect("candidate remains active");
                let identities = candidate
                    .identities
                    .expect("pre-writer candidate owns its identity allocator");
                debug_assert!(self.identities.is_none());
                self.identities = Some(identities);
                self.aborting.push(AbortingCandidate::empty(epoch));
                return Err(Box::new(InMemorySetextActivationFailure {
                    error: InMemorySetextActivationError::Writer(CandidateWriterError::ArenaBuild(
                        error,
                    )),
                    abort: Some(CandidateAbort { epoch }),
                    cleanup_error: None,
                }));
            }
        }
        match green_result {
            Ok(progress) => Ok(progress),
            Err(error) => Err(self.abort_dirty_in_memory_setext_activation(epoch, error)),
        }
    }

    /// Atomically installs the restored writer only after actor provenance,
    /// source/donor resume, and green inverse validation all succeeded.
    #[cfg(feature = "exact-parser")]
    #[allow(dead_code)] // Executed by the integrated feasibility gate before product-root wiring.
    pub(crate) fn activate_ready_in_memory_setext(
        &mut self,
        epoch: LiveCandidateEpoch,
        ready: ReadyInMemorySetextActivation,
    ) -> Result<RetainedSetextDriverActivation, Box<InMemorySetextActivationFailure>> {
        if let Err(error) =
            self.require_in_memory_setext_activation_slot(epoch, ready.activation_id())
        {
            return Err(Box::new(InMemorySetextActivationFailure {
                error,
                abort: None,
                cleanup_error: None,
            }));
        }
        if let Err(error) = self.validate_in_memory_setext_activation_provenance(
            epoch,
            ready.old_epoch(),
            ready.old_binding(),
        ) {
            return Err(self.abort_dirty_in_memory_setext_activation(epoch, error));
        }
        let (source, green, old_binding, expected_spec) = ready.into_parts();
        let activation = match CandidateWriter::join_retained_setext_green_activation(
            epoch,
            source,
            green,
            old_binding,
            &expected_spec,
        ) {
            Ok(activation) => activation,
            Err(error) => {
                return Err(self.abort_dirty_in_memory_setext_activation(
                    epoch,
                    InMemorySetextActivationError::Writer(error),
                ));
            }
        };
        {
            let candidate = self.candidate.as_ref().expect("candidate was validated");
            if candidate.ledger.is_some()
                || candidate.projection_composer_admitted
                || !matches!(candidate.writer, CandidateWriterSlot::None)
                || candidate.commit_recovery
                || candidate
                    .raw_source
                    .as_ref()
                    .map(|source| source.cursor.offset())
                    != Some(0)
                || candidate.ticket.is_none()
                || candidate.identities.is_none()
            {
                return Err(self.abort_dirty_in_memory_setext_activation(
                    epoch,
                    InMemorySetextActivationError::Writer(CandidateWriterError::Busy),
                ));
            }
        }
        let candidate = self.candidate.as_mut().expect("candidate was validated");
        let ticket = candidate.ticket.take().expect("ticket was preflighted");
        let identities = candidate
            .identities
            .take()
            .expect("identity allocator was preflighted");
        let (writer, driver) = CandidateWriter::install_validated_retained_setext(
            epoch, activation, ticket, identities,
        );
        drop(candidate.raw_source.take());
        candidate.writer = CandidateWriterSlot::Active(Box::new(writer));
        candidate.projection_composer_admitted = true;
        candidate.retained_activation = None;
        Ok(driver)
    }

    /// Explicitly releases the actor slot owned by an external-borrow
    /// activation job. Before any green poll this restores ordinary full-parse
    /// fallback. Once the build journal may contain retained pages, abandonment
    /// cancels the whole candidate and returns its fuelled abort handle.
    #[cfg(feature = "exact-parser")]
    #[allow(dead_code, clippy::needless_pass_by_value)] // Consuming the linear job closes its actor slot.
    pub(crate) fn abandon_in_memory_setext_activation(
        &mut self,
        epoch: LiveCandidateEpoch,
        job: InMemorySetextActivationJob<'_>,
    ) -> Result<Option<CandidateAbort>, LiveDocumentError> {
        self.require_in_memory_setext_activation_slot(epoch, job.activation_id())
            .map_err(|_| LiveDocumentError::WrongCandidateEpoch)?;
        if job.green_started() {
            self.cancel_candidate(epoch).map(Some)
        } else {
            self.clear_pristine_in_memory_setext_activation(epoch, job.activation_id());
            Ok(None)
        }
    }

    pub fn poll_candidate_writer(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateWriterProgress, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let candidate = self.candidate.as_mut().expect("candidate was validated");
        match &mut candidate.writer {
            CandidateWriterSlot::Active(writer) => writer.poll(epoch, &mut self.arena),
            CandidateWriterSlot::None => Err(CandidateWriterError::Invariant(
                "candidate writer is not active",
            )),
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::Paused(_) => Err(CandidateWriterError::Busy),
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::AdoptedTail(_) => Err(CandidateWriterError::Busy),
        }
    }

    #[cfg(feature = "exact-parser")]
    pub(crate) fn candidate_writer_start_reference_prefix(
        &mut self,
        epoch: LiveCandidateEpoch,
        paragraph: CandidateWriterBinding,
        request: flark_comrak_value_block_core::DirectReferencePrefixRequest,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .start_reference_prefix(epoch, paragraph, request)
    }

    #[cfg(feature = "exact-parser")]
    pub(crate) fn candidate_writer_install_reference_prefix_work(
        &mut self,
        epoch: LiveCandidateEpoch,
        identity: crate::serialized_green::active_paragraph_projection_cursor::ActiveParagraphProjectionIdentity,
        work: flark_comrak_value_block_core::DirectReferencePrefixWork<
            crate::serialized_green::active_paragraph_projection_cursor::ActiveParagraphProjectionIdentity,
        >,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        let source = &self.source;
        let arena = &mut self.arena;
        let candidate = self.candidate.as_mut().expect("candidate was validated");
        match &mut candidate.writer {
            CandidateWriterSlot::Active(writer) => {
                writer.install_reference_prefix_work(epoch, identity, work, arena, source)
            }
            CandidateWriterSlot::None => Err(CandidateWriterError::Invariant(
                "candidate writer is not active",
            )),
            CandidateWriterSlot::Paused(_) | CandidateWriterSlot::AdoptedTail(_) => {
                Err(CandidateWriterError::Busy)
            }
        }
    }

    pub fn candidate_writer_is_poisoned(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<bool, CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        match &self
            .candidate
            .as_ref()
            .expect("candidate was validated")
            .writer
        {
            CandidateWriterSlot::Active(writer) => Ok(writer.is_poisoned()),
            CandidateWriterSlot::None => Err(CandidateWriterError::Invariant(
                "candidate writer is not active",
            )),
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::Paused(_) => Ok(false),
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::AdoptedTail(_) => Ok(false),
        }
    }

    #[cfg(test)]
    pub(crate) fn inject_candidate_writer_close_failure_for_test(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) {
        self.require_live_epoch(epoch).unwrap();
        self.candidate_writer_mut()
            .unwrap()
            .inject_failure_after_green_ack_before_ledger_close();
    }

    #[cfg(test)]
    pub(crate) fn inject_candidate_writer_setext_failure_for_test(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) {
        self.require_live_epoch(epoch).unwrap();
        self.candidate_writer_mut()
            .unwrap()
            .inject_failure_after_setext_green_ack_before_ledger_retype();
    }

    #[cfg(test)]
    pub(crate) fn force_candidate_writer_setext_deferred_identity_for_test(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .force_active_setext_deferred_identity_for_test()
    }

    #[cfg(test)]
    pub(crate) fn cross_candidate_writer_deferred_setext_storage_for_test(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        self.candidate_writer_mut()?
            .cross_pending_deferred_setext_storage_for_test()
    }

    /// Grammar-free/direct-parser mechanism commit. This consumes the writer's
    /// joined source/composer/green/ticket seal, but it is not a publishable
    /// production manifest until checkpoint/reference/inline/Unknown roots are
    /// joined by the architecture-selection transaction. A local commit
    /// failure never returns the writer's linear abort/identity capabilities:
    /// the actor either admits them to its fuelled abort queue or parks them in
    /// a cancellation-only recovery candidate if arena abort admission itself
    /// fails.
    #[allow(dead_code)] // Consumed by the exact-parser feature's next slice.
    pub(crate) fn commit_candidate_writer_mechanism(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<crate::CandidateWriterBuiltDocument, Box<CandidateWriterMechanismCommitFailure>>
    {
        self.require_live_epoch(epoch).unwrap();
        let mut candidate = self.candidate.take().expect("candidate was validated");
        let writer = match std::mem::replace(&mut candidate.writer, CandidateWriterSlot::None) {
            CandidateWriterSlot::Active(writer) => writer,
            CandidateWriterSlot::None => panic!("candidate writer is active"),
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::Paused(_) => panic!("paused writer cannot commit"),
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::AdoptedTail(_) => {
                panic!("adopted-tail writer requires green/index splice before commit")
            }
        };
        match (*writer).commit_local(&mut self.arena) {
            Ok((built, identities)) => {
                debug_assert!(self.identities.is_none());
                self.identities = Some(identities);
                let green = built
                    .green_document()
                    .manifest_descriptor(&self.arena)
                    .expect("freshly committed green manifest re-decodes");
                self.latest_mechanism_document = Some(MechanismDocumentBinding { epoch, green });
                Ok(built)
            }
            Err(CandidateWriterLocalCommitFailure {
                error,
                abort,
                identities,
            }) => match abort {
                crate::CandidateWriterAbortLease::Suspended(ticket) => {
                    match self.arena.begin_build_abort(ticket) {
                        Ok(build) => {
                            debug_assert_eq!(build, epoch.build);
                            debug_assert!(self.identities.is_none());
                            self.identities = Some(identities);
                            self.aborting.push(AbortingCandidate::empty(epoch));
                            Err(Box::new(CandidateWriterMechanismCommitFailure {
                                error,
                                abort: Some(CandidateAbort { epoch }),
                            }))
                        }
                        Err(failure) => {
                            // The ticket transition is expected to be
                            // infallible for actor-derived authority. Preserve
                            // both linear capabilities even if an arena
                            // invariant says otherwise, so the failed exact
                            // job (or the next edit) can retry ordinary
                            // cancellation without an orphaned journal.
                            candidate.ticket = Some(failure.ticket);
                            candidate.identities = Some(identities);
                            candidate.commit_recovery = true;
                            self.candidate = Some(candidate);
                            Err(Box::new(CandidateWriterMechanismCommitFailure {
                                error,
                                abort: None,
                            }))
                        }
                    }
                }
                crate::CandidateWriterAbortLease::AlreadyAborting(build) => {
                    debug_assert_eq!(build, epoch.build);
                    debug_assert!(self.identities.is_none());
                    self.identities = Some(identities);
                    self.aborting.push(AbortingCandidate::empty(epoch));
                    Err(Box::new(CandidateWriterMechanismCommitFailure {
                        error,
                        abort: Some(CandidateAbort { epoch }),
                    }))
                }
            },
        }
    }

    /// First live-actor v2 commit path. It stores the one committed parent in
    /// this actor and restores the exact allocator moved out with the
    /// candidate. The prototype deliberately refuses replacement/reload until
    /// source/adoption/reference/inline publication is designed.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn commit_candidate_writer_restart_composite(
        &mut self,
        epoch: LiveCandidateEpoch,
        samples: RestartCheckpointSampleChain,
    ) -> Result<RestartCompositeCommitProgress, Box<CandidateWriterMechanismCommitFailure>> {
        self.require_live_epoch(epoch).unwrap();
        if self.latest_restart_document.is_some() || self.pending_restart_publication.is_some() {
            return Err(Box::new(CandidateWriterMechanismCommitFailure {
                error: CandidateWriterError::Invariant(
                    "restart parent replacement or overlapping publication is outside the first v2 authority slice",
                ),
                abort: None,
            }));
        }
        let mut candidate = self.candidate.take().expect("candidate was validated");
        let writer = match std::mem::replace(&mut candidate.writer, CandidateWriterSlot::None) {
            CandidateWriterSlot::Active(writer) => writer,
            CandidateWriterSlot::None => panic!("candidate writer is active"),
            CandidateWriterSlot::Paused(_) => panic!("paused writer cannot commit"),
            CandidateWriterSlot::AdoptedTail(_) => {
                panic!("adopted-tail writer requires green/index splice before commit")
            }
        };
        match (*writer).commit_restart_composite(&mut self.arena, samples) {
            Ok((document, receipt, identities)) => {
                debug_assert!(self.identities.is_none());
                self.identities = Some(identities);
                Ok(self.prepare_and_publish_restart_composite(epoch, receipt, document))
            }
            Err(CandidateWriterLocalCommitFailure {
                error,
                abort,
                identities,
            }) => match abort {
                crate::CandidateWriterAbortLease::Suspended(ticket) => {
                    match self.arena.begin_build_abort(ticket) {
                        Ok(build) => {
                            debug_assert_eq!(build, epoch.build);
                            debug_assert!(self.identities.is_none());
                            self.identities = Some(identities);
                            self.aborting.push(AbortingCandidate::empty(epoch));
                            Err(Box::new(CandidateWriterMechanismCommitFailure {
                                error,
                                abort: Some(CandidateAbort { epoch }),
                            }))
                        }
                        Err(failure) => {
                            candidate.ticket = Some(failure.ticket);
                            candidate.identities = Some(identities);
                            candidate.commit_recovery = true;
                            self.candidate = Some(candidate);
                            Err(Box::new(CandidateWriterMechanismCommitFailure {
                                error,
                                abort: None,
                            }))
                        }
                    }
                }
                crate::CandidateWriterAbortLease::AlreadyAborting(build) => {
                    debug_assert_eq!(build, epoch.build);
                    debug_assert!(self.identities.is_none());
                    self.identities = Some(identities);
                    self.aborting.push(AbortingCandidate::empty(epoch));
                    Err(Box::new(CandidateWriterMechanismCommitFailure {
                        error,
                        abort: Some(CandidateAbort { epoch }),
                    }))
                }
            },
        }
    }

    /// Converts the sole owning restart document into a root-bound candidate
    /// transaction, then publishes it atomically. Any post-commit failure is
    /// parked as a distinct actor-owned hold; it is never misrepresented as a
    /// cancellable arena build.
    #[cfg(feature = "exact-parser")]
    fn prepare_and_publish_restart_composite(
        &mut self,
        epoch: LiveCandidateEpoch,
        receipt: RestartCompositeDocumentBuildReceipt,
        document: RestartCompositeDocument,
    ) -> RestartCompositeCommitProgress {
        let publication = match document.prepare_publication(&self.arena) {
            Ok(publication) => publication,
            Err(failure) => {
                return self.park_restart_composite_publication(
                    epoch,
                    receipt,
                    failure.error,
                    RestartCompositePublicationHoldOwner::Owning(failure.document),
                );
            }
        };
        self.publish_prepared_restart_composite(epoch, receipt, publication)
    }

    #[cfg(feature = "exact-parser")]
    fn publish_prepared_restart_composite(
        &mut self,
        epoch: LiveCandidateEpoch,
        receipt: RestartCompositeDocumentBuildReceipt,
        publication: PreparedRestartCompositePublication,
    ) -> RestartCompositeCommitProgress {
        #[cfg(test)]
        let publication_token = self
            .restart_publication_token_override
            .take()
            .unwrap_or_else(|| epoch.parse_token());
        #[cfg(not(test))]
        let publication_token = epoch.parse_token();
        match publication.publish(&mut self.coordinator, publication_token, &mut self.arena) {
            Ok(publication) => {
                let delta = publication.delta();
                let published = publication.into_binding();
                debug_assert_eq!(published.output_lease(), delta.offered_output);
                debug_assert!(self.pending_restart_publication.is_none());
                // Assignment happens only after coordinator publication has
                // made this exact lease worker-current. This is deliberately
                // replacement-capable for the later suffix-splice entrypoint:
                // the old binding remains coordinator-owned until this point.
                self.latest_restart_document = Some(RestartDocumentBinding {
                    epoch,
                    published,
                    receipt,
                });
                RestartCompositeCommitProgress::Published {
                    receipt,
                    publication: delta,
                }
            }
            Err(failure) => self.park_restart_composite_publication(
                epoch,
                receipt,
                failure.error,
                RestartCompositePublicationHoldOwner::Prepared(failure.publication),
            ),
        }
    }

    #[cfg(all(test, feature = "exact-parser"))]
    pub(crate) fn reject_next_restart_publication_with_token_for_test(
        &mut self,
        token: ParseToken,
    ) {
        assert!(
            self.restart_publication_token_override
                .replace(token)
                .is_none()
        );
    }

    #[cfg(feature = "exact-parser")]
    fn park_restart_composite_publication(
        &mut self,
        epoch: LiveCandidateEpoch,
        receipt: RestartCompositeDocumentBuildReceipt,
        error: RestartCompositeDocumentError,
        owner: RestartCompositePublicationHoldOwner,
    ) -> RestartCompositeCommitProgress {
        debug_assert!(self.pending_restart_publication.is_none());
        let hold = RestartCompositePublicationHold { epoch };
        self.pending_restart_publication = Some(PendingRestartCompositePublication {
            epoch,
            receipt,
            last_error: error,
            owner,
        });
        RestartCompositeCommitProgress::Held {
            receipt,
            error,
            hold,
        }
    }

    /// Retries the exact owner/descriptor transaction parked after local
    /// commit. Preparation is repeated only when the owning document itself
    /// was returned; a coordinator rejection retries the already-prepared
    /// opaque bundle without reconstructing scalar authority.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn retry_restart_composite_publication(
        &mut self,
        hold: RestartCompositePublicationHold,
    ) -> Result<RestartCompositeCommitProgress, LiveDocumentError> {
        let pending =
            self.pending_restart_publication
                .take()
                .ok_or(LiveDocumentError::Invariant(
                    "restart publication hold is not active",
                ))?;
        if pending.epoch != hold.epoch {
            self.pending_restart_publication = Some(pending);
            return Err(LiveDocumentError::WrongCandidateEpoch);
        }
        Ok(match pending.owner {
            RestartCompositePublicationHoldOwner::Owning(document) => {
                self.prepare_and_publish_restart_composite(pending.epoch, pending.receipt, document)
            }
            RestartCompositePublicationHoldOwner::Prepared(publication) => {
                self.publish_prepared_restart_composite(pending.epoch, pending.receipt, publication)
            }
        })
    }

    /// Retires a held arena-committed output without ever manufacturing a
    /// build-abort handle. Release rejection restores the exact owning value
    /// to the actor and returns the same copyable hold for a later retry.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn release_restart_composite_publication(
        &mut self,
        hold: RestartCompositePublicationHold,
    ) -> Result<(), RestartCompositePublicationReleaseFailure> {
        let Some(pending) = self.pending_restart_publication.take() else {
            return Err(RestartCompositePublicationReleaseFailure {
                error: RestartCompositeDocumentError::Invalid(
                    "restart publication hold is not active",
                ),
                hold,
            });
        };
        if pending.epoch != hold.epoch {
            self.pending_restart_publication = Some(pending);
            return Err(RestartCompositePublicationReleaseFailure {
                error: RestartCompositeDocumentError::Invalid(
                    "restart publication hold belongs to another candidate epoch",
                ),
                hold,
            });
        }
        let PendingRestartCompositePublication {
            epoch,
            receipt,
            last_error,
            owner,
        } = pending;
        match owner {
            RestartCompositePublicationHoldOwner::Owning(document) => {
                match document.release_later(&mut self.arena) {
                    Ok(()) => Ok(()),
                    Err(failure) => {
                        let error = failure.error;
                        self.pending_restart_publication =
                            Some(PendingRestartCompositePublication {
                                epoch,
                                receipt,
                                last_error,
                                owner: RestartCompositePublicationHoldOwner::Owning(
                                    failure.document,
                                ),
                            });
                        Err(RestartCompositePublicationReleaseFailure { error, hold })
                    }
                }
            }
            RestartCompositePublicationHoldOwner::Prepared(publication) => {
                match publication.release_later(&mut self.arena) {
                    Ok(()) => Ok(()),
                    Err(failure) => {
                        let error = failure.error;
                        self.pending_restart_publication =
                            Some(PendingRestartCompositePublication {
                                epoch,
                                receipt,
                                last_error,
                                owner: RestartCompositePublicationHoldOwner::Prepared(
                                    failure.publication,
                                ),
                            });
                        Err(RestartCompositePublicationReleaseFailure { error, hold })
                    }
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn commit_candidate_writer_local_for_test(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<crate::CandidateWriterBuiltDocument, CandidateWriterLocalCommitFailure> {
        self.require_live_epoch(epoch).unwrap();
        let mut candidate = self.candidate.take().expect("candidate was validated");
        let writer = match std::mem::replace(&mut candidate.writer, CandidateWriterSlot::None) {
            CandidateWriterSlot::Active(writer) => writer,
            CandidateWriterSlot::None => panic!("candidate writer is active"),
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::Paused(_) => panic!("paused writer cannot commit"),
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::AdoptedTail(_) => {
                panic!("adopted-tail writer requires green/index splice before commit")
            }
        };
        match (*writer).commit_local(&mut self.arena) {
            Ok((built, identities)) => {
                debug_assert!(self.identities.is_none());
                self.identities = Some(identities);
                Ok(built)
            }
            Err(failure) => Err(failure),
        }
    }

    #[cfg(test)]
    pub(crate) const fn candidate_writer_test_arena(&self) -> &PageArena {
        &self.arena
    }

    #[cfg(all(test, feature = "exact-parser"))]
    pub(crate) const fn restart_publication_test_coordinator(&self) -> &Coordinator {
        &self.coordinator
    }

    #[cfg(test)]
    pub(crate) fn candidate_writer_test_arena_mut(&mut self) -> &mut PageArena {
        &mut self.arena
    }

    #[cfg(all(test, feature = "exact-parser"))]
    pub(crate) fn latest_mechanism_binding_for_test(
        &self,
    ) -> Option<(LiveCandidateEpoch, SerializedGreenManifestDescriptor)> {
        self.latest_mechanism_document
            .map(|binding| (binding.epoch, binding.green))
    }

    /// Publication barrier for the live exact-parser actor. Exact grammar
    /// driving may report local completion before a recoverable coordinator
    /// rejection is retried, but rendering/publication code must not treat
    /// that as authoritative output until this method returns `Published`.
    #[cfg(feature = "exact-parser")]
    pub(crate) fn restart_composite_publication_state(
        &self,
    ) -> Result<RestartCompositePublicationState, RestartCompositeDocumentError> {
        match (
            self.latest_restart_document.as_ref(),
            self.pending_restart_publication.as_ref(),
        ) {
            (None, None) => Ok(RestartCompositePublicationState::Unavailable),
            (Some(binding), None) => {
                binding.published.view(&self.coordinator, &self.arena)?;
                Ok(RestartCompositePublicationState::Published {
                    epoch: binding.epoch,
                    receipt: binding.receipt,
                    output: binding.published.output_lease(),
                })
            }
            (published, Some(pending)) => {
                // A future replacement may legitimately hold the next owner
                // while the previous published parent remains worker-current.
                // Revalidate that fallback before reporting the newer hold.
                if let Some(binding) = published {
                    binding.published.view(&self.coordinator, &self.arena)?;
                }
                Ok(RestartCompositePublicationState::Held {
                    receipt: pending.receipt,
                    error: pending.last_error,
                    hold: RestartCompositePublicationHold {
                        epoch: pending.epoch,
                    },
                })
            }
        }
    }

    #[cfg(all(test, feature = "exact-parser"))]
    pub(crate) fn latest_restart_view_for_test(
        &self,
    ) -> Option<(
        LiveCandidateEpoch,
        RestartCompositeDocumentBuildReceipt,
        PublishedRestartCompositeDocumentView<'_>,
    )> {
        self.latest_restart_document.as_ref().map(|binding| {
            (
                binding.epoch,
                binding.receipt,
                binding
                    .published
                    .view(&self.coordinator, &self.arena)
                    .expect("stored restart parent revalidates"),
            )
        })
    }

    #[cfg(all(test, feature = "exact-parser"))]
    pub(crate) fn candidate_writer_checkpoint_retention_for_test(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(usize, usize, usize), CandidateWriterError> {
        self.require_live_epoch(epoch)
            .map_err(CandidateWriterError::Actor)?;
        match &self
            .candidate
            .as_ref()
            .expect("candidate was validated")
            .writer
        {
            CandidateWriterSlot::Paused(continuation) => Ok((
                continuation.retained_source_bytes_for_test(),
                continuation.retained_source_heap_bytes_for_test(),
                continuation.retained_open_depth_for_test(),
            )),
            CandidateWriterSlot::Active(_) => Err(CandidateWriterError::Busy),
            CandidateWriterSlot::AdoptedTail(_) => Err(CandidateWriterError::TailAdoptionReady),
            CandidateWriterSlot::None => Err(CandidateWriterError::Invariant(
                "candidate paused writer is missing",
            )),
        }
    }

    pub fn candidate_finish_line(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateLineReceipt, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self.candidate_ledger_mut()?.finish_line(epoch)?)
    }

    pub fn seal_candidate_source(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateSourceSeal, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        Ok(self.candidate_ledger_mut()?.seal(epoch)?)
    }

    /// Reserves a document-wide block ID for the exact current build. The
    /// allocator advances before the permit is returned and never rolls back.
    pub fn mint_block_permit(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<FreshBlockPermit, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        self.require_no_retained_activation(epoch)?;
        self.candidate_identities_mut()?.mint_block(epoch.build)
    }

    /// Reserves a document-wide coverage ID for the exact current build.
    pub fn mint_coverage_permit(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<FreshCoveragePermit, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        self.require_no_retained_activation(epoch)?;
        self.candidate_identities_mut()?.mint_coverage(epoch.build)
    }

    /// Invalidates parser/source access immediately and transfers the linear
    /// arena ticket into its fuelled abort lifecycle without scanning owners.
    #[allow(clippy::too_many_lines)] // Cancellation keeps all linear recovery branches in one actor turn.
    pub fn cancel_candidate(
        &mut self,
        epoch: LiveCandidateEpoch,
    ) -> Result<CandidateAbort, LiveDocumentError> {
        self.require_live_epoch(epoch)?;
        let CandidateJob {
            epoch: owned_epoch,
            raw_source,
            ledger,
            projection_composer_admitted,
            ticket,
            identities,
            writer,
            #[cfg(feature = "exact-parser")]
            retained_activation,
            #[cfg(feature = "exact-parser")]
            persisted_restart,
            commit_recovery,
        } = self.candidate.take().expect("candidate was validated");
        debug_assert_eq!(owned_epoch, epoch);
        match writer {
            CandidateWriterSlot::Active(mut writer) => {
                return match writer.begin_abort(&mut self.arena) {
                    Ok(build) => {
                        debug_assert_eq!(build, epoch.build);
                        debug_assert!(self.identities.is_none());
                        #[cfg(feature = "exact-parser")]
                        let (identities, heap) = (*writer).into_identities_after_abort();
                        #[cfg(not(feature = "exact-parser"))]
                        let identities = (*writer).into_identities_after_abort();
                        self.identities = Some(identities);
                        drop(raw_source);
                        drop(ledger);
                        #[cfg(feature = "exact-parser")]
                        self.aborting
                            .push(AbortingCandidate::with_heap(epoch, heap));
                        #[cfg(not(feature = "exact-parser"))]
                        self.aborting.push(AbortingCandidate::empty(epoch));
                        Ok(CandidateAbort { epoch })
                    }
                    Err(error) => {
                        self.candidate = Some(CandidateJob {
                            epoch,
                            raw_source,
                            ledger,
                            projection_composer_admitted,
                            ticket: None,
                            identities: None,
                            writer: CandidateWriterSlot::Active(writer),
                            #[cfg(feature = "exact-parser")]
                            retained_activation,
                            #[cfg(feature = "exact-parser")]
                            persisted_restart,
                            commit_recovery,
                        });
                        Err(LiveDocumentError::ArenaBuild(error))
                    }
                };
            }
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::Paused(mut writer) => {
                return match writer.begin_abort(&mut self.arena) {
                    Ok(build) => {
                        debug_assert_eq!(build, epoch.build);
                        debug_assert!(self.identities.is_none());
                        let (identities, heap) = (*writer).into_identities_after_abort();
                        self.identities = Some(identities);
                        drop(raw_source);
                        drop(ledger);
                        self.aborting
                            .push(AbortingCandidate::with_heap(epoch, heap));
                        Ok(CandidateAbort { epoch })
                    }
                    Err(error) => {
                        self.candidate = Some(CandidateJob {
                            epoch,
                            raw_source,
                            ledger,
                            projection_composer_admitted,
                            ticket: None,
                            identities: None,
                            writer: CandidateWriterSlot::Paused(writer),
                            retained_activation,
                            persisted_restart,
                            commit_recovery,
                        });
                        Err(LiveDocumentError::ArenaBuild(error))
                    }
                };
            }
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::AdoptedTail(mut writer) => {
                return match writer.begin_abort(&mut self.arena) {
                    Ok(build) => {
                        debug_assert_eq!(build, epoch.build);
                        debug_assert!(self.identities.is_none());
                        let (identities, heap) = (*writer).into_identities_after_abort();
                        self.identities = Some(identities);
                        drop(raw_source);
                        drop(ledger);
                        self.aborting
                            .push(AbortingCandidate::with_heap(epoch, heap));
                        Ok(CandidateAbort { epoch })
                    }
                    Err(error) => {
                        self.candidate = Some(CandidateJob {
                            epoch,
                            raw_source,
                            ledger,
                            projection_composer_admitted,
                            ticket: None,
                            identities: None,
                            writer: CandidateWriterSlot::AdoptedTail(writer),
                            retained_activation,
                            persisted_restart,
                            commit_recovery,
                        });
                        Err(LiveDocumentError::ArenaBuild(error))
                    }
                };
            }
            CandidateWriterSlot::None => {}
        }
        let ticket = ticket.ok_or(LiveDocumentError::Invariant(
            "non-writer candidate ticket missing",
        ))?;
        let identities = identities.ok_or(LiveDocumentError::Invariant(
            "non-writer candidate identity allocator missing",
        ))?;
        match self.arena.begin_build_abort(ticket) {
            Ok(build) => {
                debug_assert_eq!(build, epoch.build);
                debug_assert!(self.identities.is_none());
                self.identities = Some(identities);
                drop(raw_source);
                drop(ledger);
                #[cfg(feature = "exact-parser")]
                let heap = persisted_restart.into_heap_retirement();
                #[cfg(feature = "exact-parser")]
                self.aborting
                    .push(AbortingCandidate::with_heap(epoch, heap));
                #[cfg(not(feature = "exact-parser"))]
                self.aborting.push(AbortingCandidate::empty(epoch));
                Ok(CandidateAbort { epoch })
            }
            Err(failure) => {
                self.candidate = Some(CandidateJob {
                    epoch,
                    raw_source,
                    ledger,
                    projection_composer_admitted,
                    ticket: Some(failure.ticket),
                    identities: Some(identities),
                    writer: CandidateWriterSlot::None,
                    #[cfg(feature = "exact-parser")]
                    retained_activation,
                    #[cfg(feature = "exact-parser")]
                    persisted_restart,
                    commit_recovery,
                });
                Err(LiveDocumentError::ArenaBuild(failure.error))
            }
        }
    }

    /// Performs at most `fuel` combined heap releases and journal-owner
    /// transfers for one cancelled candidate. Completing the poll makes the
    /// old build generation stale and proves no proportional draft chain was
    /// destroyed on edit ingress.
    pub fn poll_candidate_abort(
        &mut self,
        abort: CandidateAbort,
        fuel: usize,
    ) -> Result<CandidateAbortPollReceipt, LiveDocumentError> {
        let Some(index) = self
            .aborting
            .iter()
            .position(|candidate| candidate.epoch == abort.epoch)
        else {
            return Err(LiveDocumentError::UnknownAbort);
        };

        #[cfg(feature = "exact-parser")]
        let heap_transitions = self.aborting[index].heap.poll(fuel);
        #[cfg(not(feature = "exact-parser"))]
        let heap_transitions = 0;
        #[cfg(feature = "exact-parser")]
        let heap_complete = self.aborting[index].heap.is_complete();
        #[cfg(not(feature = "exact-parser"))]
        let heap_complete = true;

        let arena_fuel = fuel.saturating_sub(heap_transitions);
        let (owners_scheduled, owners_remaining, arena_complete) =
            if self.aborting[index].arena_complete {
                (0, 0, true)
            } else {
                let arena = self.arena.poll_build_abort(abort.build_id(), arena_fuel)?;
                if arena.complete {
                    self.aborting[index].arena_complete = true;
                }
                (
                    arena.owners_scheduled,
                    arena.owners_remaining,
                    arena.complete,
                )
            };
        let complete = arena_complete && heap_complete;
        let receipt = CandidateAbortPollReceipt {
            owners_scheduled,
            owners_remaining,
            arena_complete,
            heap_transitions,
            heap_complete,
            complete,
        };
        if complete {
            self.aborting.swap_remove(index);
        }
        Ok(receipt)
    }

    /// Advances ordinary arena reference reclamation separately from build
    /// journal cancellation.
    pub fn poll_reclaim(&mut self, fuel: usize) -> Result<ReclaimReceipt, ReclaimPollError> {
        self.arena.poll_reclaim(fuel)
    }

    /// Atomically admits one exact edit across the source and coordinator
    /// clocks. Preparing the next Crop root, lineage state, parse plan, and all
    /// overflow/range checks happens before the active candidate is detached.
    /// Once cancellation succeeds, both prepared states publish by assignment
    /// in this non-interleavable actor turn.
    pub fn accept_edit(
        &mut self,
        expected: SourceSnapshotDescriptor,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<LiveEditReceipt, LiveDocumentError> {
        #[cfg(feature = "exact-parser")]
        if self.pending_restart_publication.is_some() {
            return Err(LiveDocumentError::RestartPublicationHeld);
        }
        self.preflight_source_retirement(&range, replacement.len())?;
        let prepared_source = self.source.prepare_edit(expected, range, replacement)?;
        let prepared_coordinator = match self
            .coordinator
            .prepare_source_transition(prepared_source.transition())
        {
            Ok(prepared) => prepared,
            Err(error) => {
                self.retired_source_roots
                    .push(prepared_source.into_retired_root());
                return Err(error.into());
            }
        };

        let cancelled = if let Some(epoch) = self.candidate_epoch() {
            match self.cancel_candidate(epoch) {
                Ok(cancelled) => Some(cancelled),
                Err(error) => {
                    self.retired_source_roots
                        .push(prepared_source.into_retired_root());
                    return Err(error);
                }
            }
        } else {
            None
        };

        let (source, retired_root) = self.source.commit_prepared_edit(prepared_source);
        self.retired_source_roots.push(retired_root);
        let admission = self
            .coordinator
            .commit_prepared_source_transition(prepared_coordinator);
        assert!(
            self.clocks().source_and_coordinator_are_aligned(),
            "one actor edit transition must publish aligned source clocks"
        );
        Ok(LiveEditReceipt {
            source,
            admission,
            cancelled,
        })
    }

    /// Reserves enough logical-byte budget for either possible root produced
    /// by this admission: the old root on success or the unpublished next root
    /// on a later coordinator/cancellation rejection. Numeric-invalid ranges
    /// allocate no next root and are left for `SourceStore`'s typed validation.
    fn preflight_source_retirement(
        &self,
        range: &Range<usize>,
        replacement_bytes: usize,
    ) -> Result<(), LiveDocumentError> {
        if self.retired_source_roots.is_full() {
            return Err(LiveDocumentError::SourceRetirementBackpressure);
        }
        let old_bytes = self.source.descriptor().bytes;
        if range.start > range.end || range.end > old_bytes {
            return Ok(());
        }
        let removed = range.end - range.start;
        let next_bytes = old_bytes
            .checked_sub(removed)
            .and_then(|bytes| bytes.checked_add(replacement_bytes))
            .ok_or(LiveDocumentError::SourceRetirementByteBackpressure {
                required: usize::MAX,
                available: self.retired_source_roots.available_bytes(),
            })?;
        let required = old_bytes.max(next_bytes);
        let available = self.retired_source_roots.available_bytes();
        if required > available {
            return Err(LiveDocumentError::SourceRetirementByteBackpressure {
                required,
                available,
            });
        }
        Ok(())
    }

    /// Promotes the newest atomically admitted plan after stale candidate work
    /// has been detached. Repeated edits may replace the queued plan first.
    pub fn promote_latest_parse(&mut self) -> Result<PromotionReceipt, LiveDocumentError> {
        #[cfg(feature = "exact-parser")]
        if self.pending_restart_publication.is_some() {
            return Err(LiveDocumentError::RestartPublicationHeld);
        }
        if self.candidate.is_some() {
            return Err(LiveDocumentError::CandidateAlreadyActive);
        }
        Ok(self.coordinator.promote_latest(&mut self.arena)?)
    }

    fn current_source_descriptor(&self) -> SourceSnapshotDescriptor {
        self.source.descriptor()
    }

    fn candidate_ledger(&self) -> Result<&CandidateSourceLedger, LiveDocumentError> {
        self.candidate
            .as_ref()
            .and_then(|candidate| candidate.ledger.as_ref())
            .ok_or(LiveDocumentError::CandidateSourceLedgerNotActive)
    }

    fn candidate_ledger_mut(&mut self) -> Result<&mut CandidateSourceLedger, LiveDocumentError> {
        self.candidate
            .as_mut()
            .and_then(|candidate| candidate.ledger.as_mut())
            .ok_or(LiveDocumentError::CandidateSourceLedgerNotActive)
    }

    fn candidate_identities_mut(
        &mut self,
    ) -> Result<&mut DocumentIdentityAllocator, LiveDocumentError> {
        let candidate = self.candidate.as_mut().ok_or(LiveDocumentError::Invariant(
            "candidate identity allocator is not directly accessible",
        ))?;
        if candidate.commit_recovery {
            return Err(LiveDocumentError::Invariant(
                "failed-commit recovery candidate is cancellation-only",
            ));
        }
        candidate
            .identities
            .as_mut()
            .ok_or(LiveDocumentError::Invariant(
                "candidate identity allocator is not directly accessible",
            ))
    }

    fn candidate_writer_mut(&mut self) -> Result<&mut CandidateWriter, CandidateWriterError> {
        match &mut self
            .candidate
            .as_mut()
            .ok_or(CandidateWriterError::Invariant("candidate is not active"))?
            .writer
        {
            CandidateWriterSlot::Active(writer) => Ok(writer),
            CandidateWriterSlot::None => Err(CandidateWriterError::Invariant(
                "candidate writer is not active",
            )),
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::Paused(_) => Err(CandidateWriterError::Busy),
            #[cfg(feature = "exact-parser")]
            CandidateWriterSlot::AdoptedTail(_) => Err(CandidateWriterError::Busy),
        }
    }

    #[cfg(feature = "exact-parser")]
    fn require_no_retained_activation(
        &self,
        epoch: LiveCandidateEpoch,
    ) -> Result<(), LiveDocumentError> {
        let candidate = self
            .candidate
            .as_ref()
            .ok_or(LiveDocumentError::NoCandidate)?;
        if candidate.epoch != epoch {
            return Err(LiveDocumentError::WrongCandidateEpoch);
        }
        if candidate.retained_activation.is_some() {
            return Err(LiveDocumentError::Invariant(
                "retained Setext activation exclusively owns the fresh candidate",
            ));
        }
        Ok(())
    }

    #[cfg(not(feature = "exact-parser"))]
    const fn require_no_retained_activation(
        &self,
        _epoch: LiveCandidateEpoch,
    ) -> Result<(), LiveDocumentError> {
        Ok(())
    }

    fn require_current_token(&self, token: ParseToken) -> Result<(), LiveDocumentError> {
        let Some(active) = self.coordinator.active_plan() else {
            return Err(LiveDocumentError::Coordinator(
                CoordinatorError::NoActiveParse,
            ));
        };
        if active.token != token {
            if token.generation != self.coordinator.parse_generation() {
                return Err(LiveDocumentError::Coordinator(
                    CoordinatorError::StaleGeneration {
                        supplied: token.generation,
                        current: self.coordinator.parse_generation(),
                    },
                ));
            }
            return Err(LiveDocumentError::Coordinator(
                CoordinatorError::WrongParseToken,
            ));
        }
        if self.coordinator.queued_plan().is_some() {
            return Err(LiveDocumentError::CandidateStale);
        }
        Ok(())
    }

    #[cfg(feature = "exact-parser")]
    #[allow(dead_code)] // Reached from the feasibility activation before production root wiring.
    fn validate_in_memory_setext_activation_provenance(
        &self,
        epoch: LiveCandidateEpoch,
        old_epoch: LiveCandidateEpoch,
        old_binding: SerializedGreenManifestDescriptor,
    ) -> Result<(), InMemorySetextActivationError> {
        self.require_live_epoch(epoch)?;
        let expected = MechanismDocumentBinding {
            epoch: old_epoch,
            green: old_binding,
        };
        if self.latest_mechanism_document != Some(expected)
            || old_epoch.arena_identity() != self.arena.identity()
            || epoch.arena_identity() != self.arena.identity()
            || old_epoch.build_id() == epoch.build_id()
            || old_binding.source_revision != old_epoch.source().revision
            || old_binding.source_root != old_epoch.source().root
            || old_binding.parse_generation != old_epoch.parse_token().generation
        {
            return Err(InMemorySetextActivationError::Writer(
                CandidateWriterError::WrongCandidate,
            ));
        }
        Ok(())
    }

    #[cfg(feature = "exact-parser")]
    #[allow(dead_code)] // Reached from the feasibility activation before production root wiring.
    fn require_in_memory_setext_activation_slot(
        &self,
        epoch: LiveCandidateEpoch,
        activation_id: NonZeroU64,
    ) -> Result<(), InMemorySetextActivationError> {
        let candidate = self
            .candidate
            .as_ref()
            .ok_or(InMemorySetextActivationError::Actor(
                LiveDocumentError::NoCandidate,
            ))?;
        if candidate.epoch != epoch || candidate.retained_activation != Some(activation_id) {
            return Err(InMemorySetextActivationError::Writer(
                CandidateWriterError::WrongCandidate,
            ));
        }
        Ok(())
    }

    #[cfg(feature = "exact-parser")]
    #[allow(dead_code)] // Reached from the feasibility activation before production root wiring.
    fn clear_pristine_in_memory_setext_activation(
        &mut self,
        epoch: LiveCandidateEpoch,
        activation_id: NonZeroU64,
    ) {
        if let Some(candidate) = self.candidate.as_mut()
            && candidate.epoch == epoch
            && candidate.retained_activation == Some(activation_id)
        {
            candidate.retained_activation = None;
        }
    }

    #[cfg(feature = "exact-parser")]
    #[allow(dead_code)] // Reached from the feasibility activation before production root wiring.
    fn abort_dirty_in_memory_setext_activation(
        &mut self,
        epoch: LiveCandidateEpoch,
        error: InMemorySetextActivationError,
    ) -> Box<InMemorySetextActivationFailure> {
        match self.cancel_candidate(epoch) {
            Ok(abort) => Box::new(InMemorySetextActivationFailure {
                error,
                abort: Some(abort),
                cleanup_error: None,
            }),
            Err(cleanup_error) => {
                if let Some(candidate) = self.candidate.as_mut() {
                    candidate.commit_recovery = true;
                }
                Box::new(InMemorySetextActivationFailure {
                    error,
                    abort: None,
                    cleanup_error: Some(cleanup_error),
                })
            }
        }
    }

    fn require_live_epoch(&self, supplied: LiveCandidateEpoch) -> Result<(), LiveDocumentError> {
        let Some(candidate) = self.candidate.as_ref() else {
            return Err(LiveDocumentError::NoCandidate);
        };
        if candidate.epoch != supplied {
            return Err(LiveDocumentError::WrongCandidateEpoch);
        }
        self.require_current_token(supplied.token)?;
        if supplied.source != self.current_source_descriptor()
            || supplied.arena != self.arena.identity()
            || supplied.arena != self.coordinator.arena_identity()
            || supplied.build
                != candidate
                    .build_id()
                    .map_err(|_| LiveDocumentError::CandidateStale)?
            || supplied.source != candidate.source_descriptor()
            || candidate.source_identity() != supplied.source.root
            || self.arena.build_lifecycle(supplied.build)? != ArenaBuildLifecycle::Suspended
        {
            return Err(LiveDocumentError::CandidateStale);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "exact-parser")]
    use crate::exact_block_job::{
        ExactBlockCheckpointAdmission, ExactBlockCheckpointCapturePoll, ExactBlockJob,
        ExactBlockJobProgress,
    };
    use crate::{
        CandidateLineEnding, CandidateSourceAtomKind, PendingSourceKind, SourceLedgerMetric,
    };
    #[cfg(feature = "exact-parser")]
    use flark_comrak_value_block_core::{DirectPollStatus, DirectValueBlockParser, SyntaxProfile};

    #[cfg(feature = "exact-parser")]
    const RESTART_PUBLICATION_CONFIG: CandidateWriterConfig = CandidateWriterConfig {
        syntax_profile: 1,
        grammar_revision: crate::GrammarRevision(1),
        semantic_epoch: 1,
    };

    #[cfg(feature = "exact-parser")]
    const RETAINED_PUBLICATION_CONFIG: CandidateWriterConfig = CandidateWriterConfig {
        syntax_profile: 1,
        grammar_revision: crate::GrammarRevision(1),
        semantic_epoch: 2,
    };

    #[cfg(feature = "exact-parser")]
    #[test]
    fn dense_checkpoint_heap_retirement_obeys_the_actor_abort_fuel_budget() {
        const SAMPLES: usize = 10_000;
        let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
        assert!(parser.pending_command().is_some());
        parser.acknowledge_command().unwrap();
        parser.begin_line("x\n".to_owned()).unwrap();
        loop {
            match parser.poll_line(1).unwrap().status {
                DirectPollStatus::CommandReady => parser.acknowledge_command().unwrap(),
                DirectPollStatus::Pending => {}
                DirectPollStatus::ExternalWorkReady => {
                    panic!("non-reference donor fixture unexpectedly requested external work")
                }
                DirectPollStatus::Complete => break,
            }
        }
        let interval =
            crate::committed_checkpoint_index::RelativeCheckpointMeasure::new(1, 1, 1, 1, 1);
        let mut samples = Vec::new();
        samples.try_reserve_exact(SAMPLES).unwrap();
        for _ in 0..SAMPLES {
            samples.push(
                crate::committed_checkpoint_index::DonorCheckpointSampleDraft::try_new(
                    interval,
                    parser
                        .capture_durable_grammar_line_boundary_checkpoint()
                        .unwrap(),
                )
                .unwrap(),
            );
        }

        let mut document = LiveDocumentStore::new("", 4).unwrap();
        let epoch = document
            .begin_candidate(document.active_parse_plan().unwrap().token)
            .unwrap();
        let abort = document.cancel_candidate(epoch).unwrap();
        let retiring = document
            .aborting
            .iter_mut()
            .find(|candidate| candidate.epoch == epoch)
            .unwrap();
        retiring.heap = crate::candidate_writer::CandidateWriterHeapRetirement::from_donor(
            crate::committed_checkpoint_index::DonorCheckpointHeapRetirement::from_samples(samples),
        );

        let zero = document.poll_candidate_abort(abort, 0).unwrap();
        assert_eq!(zero.owners_scheduled + zero.heap_transitions, 0);
        assert!(zero.arena_complete);
        assert!(!zero.heap_complete);
        assert!(!zero.complete);

        for completed in 1..=SAMPLES {
            let receipt = document.poll_candidate_abort(abort, 1).unwrap();
            assert!(receipt.owners_scheduled + receipt.heap_transitions <= 1);
            assert_eq!(receipt.heap_transitions, 1);
            assert_eq!(receipt.complete, completed == SAMPLES);
        }
        assert!(matches!(
            document.poll_candidate_abort(abort, 0),
            Err(LiveDocumentError::UnknownAbort)
        ));
    }

    /// Drives the real parser/writer/checkpoint path through one two-line
    /// restart-composite commit. Supplying `reject_publication` changes only
    /// the final coordinator token, after the arena commit, so the actor's
    /// post-commit hold is exercised without forging storage authority.
    #[cfg(feature = "exact-parser")]
    fn drive_two_line_restart_publication(
        reject_publication: bool,
    ) -> (LiveDocumentStore, ParseToken, OutputRootLease) {
        let mut document = LiveDocumentStore::new("alpha\nbeta\n", 8).unwrap();
        let initial_plan = document.active_parse_plan().unwrap();
        let token = initial_plan.token;
        let epoch = document.begin_candidate(token).unwrap();
        document.activate_candidate_source_ledger(epoch).unwrap();
        document
            .activate_candidate_writer(epoch, RESTART_PUBLICATION_CONFIG)
            .unwrap();
        if reject_publication {
            document.reject_next_restart_publication_with_token_for_test(ParseToken {
                generation: ParseGeneration(token.generation.0 + 1),
                ..token
            });
        }
        let mut job = Some(ExactBlockJob::new(epoch).unwrap());
        let mut chain: Option<RestartCheckpointSampleChain> = None;
        let mut cursor = None;
        for _ in 0..2_000_000 {
            let progress = job.as_mut().unwrap().poll(&mut document).unwrap();
            if progress == ExactBlockJobProgress::Complete {
                return (document, token, initial_plan.base_output);
            }
            let pending = job.as_ref().unwrap();
            if !pending.is_line_boundary_checkpoint_seam() {
                continue;
            }
            let mut capture = match job
                .take()
                .unwrap()
                .start_line_boundary_checkpoint(&mut document)
                .unwrap()
            {
                ExactBlockCheckpointAdmission::Started(capture) => *capture,
                ExactBlockCheckpointAdmission::Skipped { reason, .. } => {
                    panic!("eligible publication checkpoint skipped: {reason:?}")
                }
            };
            let checkpoint = loop {
                match capture.poll(&mut document).unwrap() {
                    ExactBlockCheckpointCapturePoll::Pending(next) => capture = next,
                    ExactBlockCheckpointCapturePoll::Ready(checkpoint) => break checkpoint,
                }
            };
            let acknowledged_lines = checkpoint.acknowledged_lines();
            let captured = match cursor.take() {
                None => checkpoint
                    .capture_first_donor_checkpoint_sample(&mut document)
                    .unwrap(),
                Some(previous) => checkpoint
                    .capture_successive_donor_checkpoint_sample(&mut document, previous)
                    .unwrap(),
            };
            if let Some(samples) = chain.as_mut() {
                cursor = Some(
                    samples
                        .try_append(captured)
                        .unwrap_or_else(|_| panic!("two-line restart chain append failed")),
                );
            } else {
                let (samples, next) = captured
                    .try_start_restart_chain()
                    .unwrap_or_else(|_| panic!("two-line restart chain start failed"));
                chain = Some(samples);
                cursor = Some(next);
            }
            let mut resumed = checkpoint.resume(&mut document).unwrap();
            if acknowledged_lines == 2 {
                resumed
                    .install_restart_sample_chain(chain.take().unwrap())
                    .unwrap();
                cursor = None;
            }
            job = Some(resumed);
        }
        panic!("two-line restart publication did not converge")
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn restart_composite_commit_publishes_one_coordinator_current_binding() {
        let (mut document, _token, _bootstrap) = drive_two_line_restart_publication(false);
        let state = document.restart_composite_publication_state().unwrap();
        let RestartCompositePublicationState::Published { output, .. } = state else {
            panic!("completed restart output must cross the publication barrier: {state:?}");
        };
        assert_eq!(document.current_output(), output);
        assert!(document.active_parse_plan().is_none());
        assert!(document.latest_restart_view_for_test().is_some());

        let source = document.source_descriptor();
        let edit = document
            .accept_edit(source, source.bytes..source.bytes, "x")
            .unwrap();
        assert_eq!(edit.admission.active.base_output, output);
        assert_eq!(document.active_parse_plan().unwrap().base_output, output);
        let epoch = document
            .begin_candidate(document.active_parse_plan().unwrap().token)
            .unwrap();
        let abort = document.cancel_candidate(epoch).unwrap();
        while !document.poll_candidate_abort(abort, 1).unwrap().complete {}
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn rejected_restart_publication_is_observable_and_retries_the_same_committed_output() {
        let (mut document, token, _bootstrap) = drive_two_line_restart_publication(true);
        let state = document.restart_composite_publication_state().unwrap();
        let RestartCompositePublicationState::Held { hold, error, .. } = state else {
            panic!("rejected coordinator handoff must remain an observable hold: {state:?}");
        };
        assert!(matches!(
            error,
            RestartCompositeDocumentError::Invalid(
                "restart publication manifest and parse token differ"
            )
        ));
        assert_eq!(
            document.begin_candidate(token),
            Err(LiveDocumentError::RestartPublicationHeld)
        );
        let source = document.source_descriptor();
        assert_eq!(
            document.accept_edit(source, source.bytes..source.bytes, "x"),
            Err(LiveDocumentError::RestartPublicationHeld)
        );

        let progress = document.retry_restart_composite_publication(hold).unwrap();
        assert!(matches!(
            progress,
            RestartCompositeCommitProgress::Published { .. }
        ));
        assert!(matches!(
            document.restart_composite_publication_state().unwrap(),
            RestartCompositePublicationState::Published { .. }
        ));
        assert!(document.active_parse_plan().is_none());
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn held_restart_publication_releases_without_fabricating_a_candidate_abort() {
        let (mut document, token, _bootstrap) = drive_two_line_restart_publication(true);
        let RestartCompositePublicationState::Held { hold, .. } =
            document.restart_composite_publication_state().unwrap()
        else {
            panic!("publication rejection must be held")
        };
        assert_eq!(document.candidate_epoch(), None);
        document
            .release_restart_composite_publication(hold)
            .unwrap();
        assert_eq!(
            document.restart_composite_publication_state().unwrap(),
            RestartCompositePublicationState::Unavailable
        );
        let epoch = document.begin_candidate(token).unwrap();
        let abort = document.cancel_candidate(epoch).unwrap();
        while !document.poll_candidate_abort(abort, 1).unwrap().complete {}
    }

    #[cfg(feature = "exact-parser")]
    #[test]
    fn persisted_restart_rejects_wrong_or_noncurrent_published_parent_lease() {
        use crate::live_document::persisted_restart_activation::PersistedRestartActivationError;

        let (mut wrong, _token, _bootstrap) = drive_two_line_restart_publication(false);
        let source = wrong.source_descriptor();
        wrong
            .accept_edit(source, source.bytes..source.bytes, "x")
            .unwrap();
        let epoch = wrong
            .begin_candidate(wrong.active_parse_plan().unwrap().token)
            .unwrap();
        let mut forged = wrong.current_output();
        forged.grammar_revision = crate::GrammarRevision(forged.grammar_revision.0 + 1);
        wrong
            .latest_restart_document
            .as_mut()
            .unwrap()
            .published
            .replace_output_lease_for_test(forged);
        assert!(matches!(
            wrong.begin_persisted_restart_activation(epoch, 6, RETAINED_PUBLICATION_CONFIG),
            Err(PersistedRestartActivationError::Parent(
                RestartCompositeDocumentError::Coordinator(CoordinatorError::LeaseMismatch(_))
            ))
        ));
        let abort = wrong.cancel_candidate(epoch).unwrap();
        while !wrong.poll_candidate_abort(abort, 1).unwrap().complete {}

        let (mut noncurrent, _token, bootstrap) = drive_two_line_restart_publication(false);
        let source = noncurrent.source_descriptor();
        noncurrent
            .accept_edit(source, source.bytes..source.bytes, "x")
            .unwrap();
        let epoch = noncurrent
            .begin_candidate(noncurrent.active_parse_plan().unwrap().token)
            .unwrap();
        noncurrent
            .latest_restart_document
            .as_mut()
            .unwrap()
            .published
            .replace_output_lease_for_test(bootstrap);
        assert!(matches!(
            noncurrent.begin_persisted_restart_activation(epoch, 6, RETAINED_PUBLICATION_CONFIG),
            Err(PersistedRestartActivationError::Parent(
                RestartCompositeDocumentError::Coordinator(CoordinatorError::RootNotWorkerCurrent(
                    _
                ))
            ))
        ));
        let abort = noncurrent.cancel_candidate(epoch).unwrap();
        while !noncurrent.poll_candidate_abort(abort, 1).unwrap().complete {}
    }

    fn activate_ledger(source: &str) -> (LiveDocumentStore, LiveCandidateEpoch) {
        let mut document = LiveDocumentStore::new(source, 8).unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        document.activate_candidate_source_ledger(epoch).unwrap();
        (document, epoch)
    }

    fn recognize_current_line(
        document: &mut LiveDocumentStore,
        epoch: LiveCandidateEpoch,
    ) -> CandidateRecognitionLineReceipt {
        let mut saw_atom = false;
        loop {
            match document.poll_candidate_recognition(epoch, 1).unwrap() {
                CandidateRecognitionPoll::NeedFuel(receipt) => {
                    assert_eq!(receipt.work_units, 1);
                }
                CandidateRecognitionPoll::Atom { atom, .. } => {
                    saw_atom = true;
                    if matches!(atom.kind(), CandidateSourceAtomKind::LineEnding(_)) {
                        break;
                    }
                }
                CandidateRecognitionPoll::Eof(_) => break,
            }
        }
        assert!(
            saw_atom,
            "a continuation test cannot acknowledge a phantom line"
        );
        document.candidate_finish_recognition_line(epoch).unwrap()
    }

    fn consume_identity_line_to_pending_terminator(
        document: &mut LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        terminal: &CandidateOpenBinding,
    ) -> CandidateLineReceipt {
        let _ = recognize_current_line(document, epoch);
        consume_authoritative_identity_line(document, epoch, terminal)
    }

    fn consume_authoritative_identity_line(
        document: &mut LiveDocumentStore,
        epoch: LiveCandidateEpoch,
        terminal: &CandidateOpenBinding,
    ) -> CandidateLineReceipt {
        let identity = CandidateLogicalAction::identity(terminal).unwrap();
        loop {
            match document.poll_candidate_source(epoch, 1).unwrap() {
                CandidateSourcePoll::NeedFuel(receipt) => {
                    assert_eq!(receipt.work_units, 1);
                }
                CandidateSourcePoll::Atom { atom, .. } => {
                    if matches!(atom.kind(), CandidateSourceAtomKind::LineEnding(_)) {
                        document
                            .candidate_stage_terminator(
                                epoch,
                                &atom,
                                terminal,
                                GreenAffinity::Downstream,
                            )
                            .unwrap();
                        let receipt = document.candidate_finish_line(epoch).unwrap();
                        assert!(receipt.recognition_replay_matched());
                        return receipt;
                    }
                    document
                        .candidate_claim_to(
                            epoch,
                            atom.boundary(),
                            terminal,
                            CoveragePart::CONTENT,
                            &identity,
                            GreenAffinity::Downstream,
                        )
                        .unwrap();
                }
                CandidateSourcePoll::Eof(_) => {
                    let receipt = document.candidate_finish_line(epoch).unwrap();
                    assert!(receipt.recognition_replay_matched());
                    return receipt;
                }
            }
        }
    }

    fn open_document_and_terminal(
        document: &mut LiveDocumentStore,
        epoch: LiveCandidateEpoch,
    ) -> (CandidateOpenBinding, CandidateOpenBinding) {
        let root = document
            .candidate_open_binding(epoch, GreenKind::DOCUMENT)
            .unwrap();
        let paragraph = document
            .candidate_open_binding(epoch, GreenKind::PARAGRAPH)
            .unwrap();
        (root, paragraph)
    }

    #[test]
    fn line_boundary_restart_remints_all_endings_and_discards_lone_cr_lookahead() {
        for (source, ending, boundary, cursor_before_restart) in [
            ("a\nb\n", CandidateLineEnding::Lf, 2, 2),
            ("a\rb\n", CandidateLineEnding::LoneCr, 2, 3),
            ("a\r\nb\n", CandidateLineEnding::CrLf, 3, 3),
        ] {
            let (mut document, epoch) = activate_ledger(source);
            let (_root, paragraph) = open_document_and_terminal(&mut document, epoch);
            let receipt =
                consume_identity_line_to_pending_terminator(&mut document, epoch, &paragraph);
            assert_eq!(receipt.ending(), Some(ending));
            assert_eq!(receipt.pending(), Some(PendingSourceKind::Terminator));
            assert_eq!(
                document.candidate_cursor_offset(epoch).unwrap(),
                cursor_before_restart
            );

            let before = document
                .candidate_ledger()
                .unwrap()
                .test_top_binding_state()
                .unwrap();
            document
                .restart_candidate_source_ledger_at_line_boundary(epoch)
                .unwrap();
            let after = document
                .candidate_ledger()
                .unwrap()
                .test_top_binding_state()
                .unwrap();
            assert_eq!(document.candidate_cursor_offset(epoch).unwrap(), boundary);
            assert_eq!(
                after, before,
                "restart preserves the complete binding stamp"
            );

            document
                .candidate_resolve_terminator(
                    epoch,
                    CandidateTerminatorResolution::ContinueCanonicalNewline,
                )
                .unwrap();
            match document.poll_candidate_recognition(epoch, 1).unwrap() {
                CandidateRecognitionPoll::Atom { atom, receipt, .. } => {
                    assert_eq!(atom.kind(), CandidateSourceAtomKind::Scalar('b'));
                    assert_eq!(receipt.source_bytes_read, 1);
                }
                other => panic!("fresh recognition cursor must reread b: {other:?}"),
            }
        }
    }

    #[test]
    fn line_boundary_restart_preserves_a_pending_blank_gap() {
        let (mut document, epoch) = activate_ledger(" \t\r\nx\n");
        let root = document
            .candidate_open_binding(epoch, GreenKind::DOCUMENT)
            .unwrap();
        let _ = recognize_current_line(&mut document, epoch);
        loop {
            match document.poll_candidate_source(epoch, 1).unwrap() {
                CandidateSourcePoll::NeedFuel(_) => {}
                CandidateSourcePoll::Atom { atom, .. } => {
                    if matches!(atom.kind(), CandidateSourceAtomKind::LineEnding(_)) {
                        break;
                    }
                }
                CandidateSourcePoll::Eof(_) => panic!("blank line has an ending"),
            }
        }
        document
            .candidate_stage_blank_gap(epoch, GreenAffinity::Upstream)
            .unwrap();
        let receipt = document.candidate_finish_line(epoch).unwrap();
        assert_eq!(receipt.pending(), Some(PendingSourceKind::Gap));

        document
            .restart_candidate_source_ledger_at_line_boundary(epoch)
            .unwrap();
        let gap = document.candidate_resolve_blank_gap(epoch, &root).unwrap();
        assert_eq!(gap.absolute_range(), (0, 4));
        assert_eq!(gap.metric().bytes(), 4);
        assert_eq!(gap.part(), CoveragePart::GAP);
    }

    #[test]
    fn line_boundary_restart_supports_a_fully_acknowledged_bare_eof_line_and_exact_seal() {
        let source = "a\r\nb";
        let (mut document, epoch) = activate_ledger(source);
        let (root, paragraph) = open_document_and_terminal(&mut document, epoch);
        let _ = consume_identity_line_to_pending_terminator(&mut document, epoch, &paragraph);
        document
            .restart_candidate_source_ledger_at_line_boundary(epoch)
            .unwrap();
        document
            .candidate_resolve_terminator(
                epoch,
                CandidateTerminatorResolution::ContinueCanonicalNewline,
            )
            .unwrap();

        let recognized = recognize_current_line(&mut document, epoch);
        assert_eq!(recognized.ending(), None);
        let bare = consume_authoritative_identity_line(&mut document, epoch, &paragraph);
        assert_eq!(bare.ending(), None);
        assert_eq!(bare.pending(), None);
        document
            .restart_candidate_source_ledger_at_line_boundary(epoch)
            .unwrap();
        assert!(matches!(
            document.poll_candidate_source(epoch, 1).unwrap(),
            CandidateSourcePoll::Eof(_)
        ));

        document.candidate_close_binding(epoch, &paragraph).unwrap();
        document.candidate_close_binding(epoch, &root).unwrap();
        let seal = document.seal_candidate_source(epoch).unwrap();
        assert_eq!(seal.metric().bytes(), u64::try_from(source.len()).unwrap());
        assert_eq!(
            seal.metric().utf16(),
            u64::try_from(source.encode_utf16().count()).unwrap()
        );
        assert_eq!(seal.line_count(), 2);
        assert_eq!(seal.source_piece_count(), 3);
        assert!(
            seal.source_bytes_copied() > source.len(),
            "diagnostics include both reminted cursor loads"
        );
    }

    #[test]
    fn line_boundary_capture_rejects_live_recognition_replay_or_range_without_consuming_ledger() {
        let (mut document, epoch) = activate_ledger("a\nb\n");
        let (_root, _paragraph) = open_document_and_terminal(&mut document, epoch);
        let _ = recognize_current_line(&mut document, epoch);
        assert_eq!(
            document.restart_candidate_source_ledger_at_line_boundary(epoch),
            Err(LiveDocumentError::SourceLedger(
                SourceBoundLedgerError::RecognitionReplayPending
            ))
        );
        assert!(document.candidate_ledger().is_ok());

        // A fresh document isolates the independently forbidden open-range
        // shape; neither failed capture consumes its live ledger.
        let (mut ranged, ranged_epoch) = activate_ledger("a\nb\n");
        let (_root, _paragraph) = open_document_and_terminal(&mut ranged, ranged_epoch);
        ranged
            .candidate_begin_recognition_range(
                ranged_epoch,
                CandidateRecognitionRangeKind::ReferenceDefinitionPrefix,
            )
            .unwrap();
        assert_eq!(
            ranged.restart_candidate_source_ledger_at_line_boundary(ranged_epoch),
            Err(LiveDocumentError::SourceLedger(
                SourceBoundLedgerError::RecognitionRangeAlreadyOpen
            ))
        );
        assert!(ranged.candidate_ledger().is_ok());
    }

    #[test]
    fn line_boundary_resume_rejects_wrong_epoch_root_offset_scalar_and_crlf_cut() {
        let (mut document, epoch) = activate_ledger("a\r\nb\n");
        let (_root, paragraph) = open_document_and_terminal(&mut document, epoch);
        let _ = consume_identity_line_to_pending_terminator(&mut document, epoch, &paragraph);
        let ledger = document.candidate.as_mut().unwrap().ledger.take().unwrap();
        let continuation = ledger.into_line_boundary_continuation(epoch).unwrap();
        assert_eq!(continuation.absolute_offset(), 3);

        let valid_pair = document.source.issue_resume_cursor_pair(3).unwrap();
        let root_utf16 = valid_pair.total_utf16();
        let valid_physical_line_start = valid_pair.is_physical_line_start();
        let (valid_authoritative, valid_recognition) = valid_pair.into_cursors();

        let mut other_document = LiveDocumentStore::new("a\r\nb\n", 8).unwrap();
        let other_token = other_document.active_parse_plan().unwrap().token;
        let other_epoch = other_document.begin_candidate(other_token).unwrap();
        assert_eq!(
            continuation.validate_resume_authority(
                other_epoch,
                root_utf16,
                &valid_authoritative,
                &valid_recognition,
                valid_physical_line_start,
            ),
            Err(SourceBoundLedgerError::WrongEpoch)
        );

        let other_source = SourceStore::new("a\r\nb\n", 8);
        let (wrong_root, _) = other_source
            .issue_resume_cursor_pair(3)
            .unwrap()
            .into_cursors();
        assert_eq!(
            continuation.validate_resume_authority(
                epoch,
                root_utf16,
                &wrong_root,
                &valid_recognition,
                valid_physical_line_start,
            ),
            Err(SourceBoundLedgerError::WrongSourceRoot)
        );

        let (wrong_offset, _) = document
            .source
            .issue_resume_cursor_pair(0)
            .unwrap()
            .into_cursors();
        assert_eq!(
            continuation.validate_resume_authority(
                epoch,
                root_utf16,
                &wrong_offset,
                &valid_recognition,
                valid_physical_line_start,
            ),
            Err(SourceBoundLedgerError::WrongSourceOffset)
        );
        assert_eq!(
            continuation.validate_resume_authority(
                epoch,
                root_utf16,
                &valid_authoritative,
                &valid_recognition,
                false,
            ),
            Err(SourceBoundLedgerError::ResumeOffsetIsNotPhysicalLineStart)
        );

        let utf8 = SourceStore::new("😀\n", 8);
        assert_eq!(
            utf8.issue_resume_cursor_pair(1).unwrap_err(),
            crate::SourceError::NotCharBoundary(1)
        );
        assert_eq!(
            document.source.issue_resume_cursor_pair(2).unwrap_err(),
            crate::SourceError::InvalidRange,
            "middle of CRLF is forbidden"
        );
    }

    #[test]
    fn line_boundary_continuation_scales_only_with_open_depth_and_handles_deep_resume() {
        const QUOTE_DEPTH: usize = 256;
        let source = format!("x\n{}", "tail".repeat(256 * 1024));
        let (mut document, epoch) = activate_ledger(&source);
        let root = document
            .candidate_open_binding(epoch, GreenKind::DOCUMENT)
            .unwrap();
        let mut quotes = Vec::with_capacity(QUOTE_DEPTH);
        for _ in 0..QUOTE_DEPTH {
            quotes.push(
                document
                    .candidate_open_binding(epoch, GreenKind::BLOCK_QUOTE)
                    .unwrap(),
            );
        }
        let paragraph = document
            .candidate_open_binding(epoch, GreenKind::PARAGRAPH)
            .unwrap();
        let _ = consume_identity_line_to_pending_terminator(&mut document, epoch, &paragraph);
        let before = document
            .candidate_ledger()
            .unwrap()
            .test_top_binding_state()
            .unwrap();

        let ledger = document.candidate.as_mut().unwrap().ledger.take().unwrap();
        let continuation = ledger.into_line_boundary_continuation(epoch).unwrap();
        let open_depth = QUOTE_DEPTH + 2;
        eprintln!(
            "source line continuation: source_bytes={} inline_bytes={} open_depth={} heap_bytes={}",
            continuation.retained_source_bytes_for_test(),
            std::mem::size_of_val(&continuation),
            open_depth,
            continuation.retained_heap_bytes_for_test(),
        );
        assert_eq!(continuation.retained_source_bytes_for_test(), 0);
        assert_eq!(
            continuation.retained_heap_bytes_for_test(),
            open_depth
                * (std::mem::size_of::<crate::source_bound_ledger::BindingStamp>()
                    + std::mem::size_of::<SourceLedgerMetric>())
        );
        assert!(
            continuation.retained_heap_bytes_for_test() < source.len() / 16,
            "a 1 MiB untouched suffix cannot enter continuation storage"
        );

        let offset = usize::try_from(continuation.absolute_offset()).unwrap();
        let cursor_pair = document.source.issue_resume_cursor_pair(offset).unwrap();
        let root_utf16 = cursor_pair.total_utf16();
        let physical_line_start = cursor_pair.is_physical_line_start();
        let (authoritative, recognition) = cursor_pair.into_cursors();
        continuation
            .validate_resume_authority(
                epoch,
                root_utf16,
                &authoritative,
                &recognition,
                physical_line_start,
            )
            .unwrap();
        document.candidate.as_mut().unwrap().ledger =
            Some(continuation.resume_with_validated_cursors(authoritative, recognition));
        assert_eq!(
            document
                .candidate_ledger()
                .unwrap()
                .test_top_binding_state()
                .unwrap(),
            before
        );
        document
            .candidate_resolve_terminator(
                epoch,
                CandidateTerminatorResolution::ContinueCanonicalNewline,
            )
            .unwrap();
        document.candidate_close_binding(epoch, &paragraph).unwrap();
        for quote in quotes.iter().rev() {
            document.candidate_close_binding(epoch, quote).unwrap();
        }
        document.candidate_close_binding(epoch, &root).unwrap();
    }

    #[test]
    fn setext_ledger_retype_preserves_identity_path_and_logical_metric() {
        let mut document = LiveDocumentStore::new("alpha\n===\n", 8).unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();
        document.activate_candidate_source_ledger(epoch).unwrap();
        let root = document
            .candidate_open_binding(epoch, GreenKind::DOCUMENT)
            .unwrap();
        let paragraph = document
            .candidate_open_binding(epoch, GreenKind::PARAGRAPH)
            .unwrap();
        let block = paragraph.block_id();
        let before = document
            .candidate_ledger()
            .unwrap()
            .test_top_binding_state()
            .unwrap();

        let heading = document
            .candidate_ledger_mut()
            .unwrap()
            .promote_top_paragraph_to_setext_heading(epoch, paragraph)
            .unwrap();
        let after = document
            .candidate_ledger()
            .unwrap()
            .test_top_binding_state()
            .unwrap();

        assert_eq!(heading.block_id(), block);
        assert_eq!(heading.kind(), GreenKind::HEADING);
        assert_eq!(after.0, before.0, "BlockId is the semantic identity");
        assert_eq!(after.1, GreenKind::HEADING);
        assert_eq!(after.2, before.2, "open-path depth is unchanged");
        assert_eq!(after.3, before.3, "path generation is unchanged");
        assert_eq!(after.4, before.4, "logical metric is unchanged");
        assert_eq!(after.5, before.5 + 1, "structural generation advances");

        document.candidate_close_binding(epoch, &heading).unwrap();
        document.candidate_close_binding(epoch, &root).unwrap();
    }

    #[test]
    fn atomic_edit_detaches_a_real_journal_owner_under_exact_fuel() {
        let mut document = LiveDocumentStore::new("abc", 8).unwrap();
        let token = document.active_parse_plan().unwrap().token;
        document.begin_candidate(token).unwrap();

        let CandidateJob {
            epoch,
            raw_source,
            ledger,
            projection_composer_admitted,
            ticket,
            identities,
            writer,
            #[cfg(feature = "exact-parser")]
            retained_activation,
            #[cfg(feature = "exact-parser")]
            persisted_restart,
            commit_recovery,
        } = document.candidate.take().unwrap();
        let mut session = document
            .arena
            .resume_build(ticket.expect("non-writer test candidate owns ticket"))
            .unwrap();
        let (_staged_owner, _) = session.allocate(b"staged-green-page", &[]).unwrap();
        let ticket = session.suspend().unwrap();
        document.candidate = Some(CandidateJob {
            epoch,
            raw_source,
            ledger,
            projection_composer_admitted,
            ticket: Some(ticket),
            identities,
            writer,
            #[cfg(feature = "exact-parser")]
            retained_activation,
            #[cfg(feature = "exact-parser")]
            persisted_restart,
            commit_recovery,
        });

        let receipt = document
            .accept_edit(document.source_descriptor(), 3..3, "!")
            .unwrap();
        assert!(document.clocks().source_and_coordinator_are_aligned());
        let abort = receipt.cancelled().expect("candidate was detached");

        let without_fuel = document.poll_candidate_abort(abort, 0).unwrap();
        assert!(!without_fuel.complete);
        assert_eq!(without_fuel.owners_scheduled, 0);
        assert_eq!(without_fuel.owners_remaining, 1);

        let one_owner = document.poll_candidate_abort(abort, 1).unwrap();
        assert!(one_owner.complete);
        assert_eq!(one_owner.owners_scheduled, 1);
        assert_eq!(one_owner.owners_remaining, 0);
        assert_eq!(document.arena.metrics().pending_releases, 1);

        assert_eq!(document.poll_reclaim(0).unwrap().reference_transitions, 0);
        let reclaimed = document.poll_reclaim(1).unwrap();
        assert_eq!(reclaimed.reference_transitions, 1);
        assert_eq!(reclaimed.nodes_reclaimed, 1);
    }

    #[test]
    fn worker_query_owner_retires_before_host_destroys_the_last_root_owner() {
        fn assert_send<T: Send>() {}
        assert_send::<RetiredSourceRoot>();

        let mut document = LiveDocumentStore::new("old source", 8).unwrap();
        // SourceStore's owning query is worker-internal. LiveDocumentStore's
        // public query_source path now returns only a borrowed view.
        let old = document.source.query_snapshot();
        let old_descriptor = document.source_descriptor();
        let weak = old.weak_observer_for_testing();

        document.accept_edit(old_descriptor, 0..3, "new").unwrap();
        assert_eq!(document.retired_source_root_count(), 1);
        assert_eq!(
            document.retired_source_root_capacity(),
            SOURCE_RETIREMENT_QUEUE_CAPACITY
        );
        assert_eq!(
            document.retired_source_logical_bytes(),
            old_descriptor.bytes
        );
        assert_eq!(
            document.retired_source_logical_byte_capacity(),
            SOURCE_RETIREMENT_QUEUE_BYTE_CAPACITY
        );
        assert!(weak.upgrade().is_some());

        let retired = document
            .take_retired_source_root()
            .expect("accepted edit queued its previous root");
        assert_eq!(retired.descriptor(), old_descriptor);
        assert_eq!(document.retired_source_root_count(), 0);
        assert_eq!(document.retired_source_logical_bytes(), 0);
        assert!(weak.upgrade().is_some());

        // Retire every worker owner while the host capability still pins the
        // root, so no worker-side decrement can run the deep destructor.
        drop(old);
        assert!(weak.upgrade().is_some());

        // This explicit disposal represents the host's off-admission lane and
        // is the only operation that can release the final owner.
        retired.dispose();
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn retired_roots_leave_the_fixed_queue_in_edit_order() {
        let mut document = LiveDocumentStore::new("", 8).unwrap();
        let mut expected = Vec::new();
        for replacement in ["a", "b", "c"] {
            let snapshot = document.source.query_snapshot();
            expected.push((
                document.source_descriptor(),
                snapshot.weak_observer_for_testing(),
            ));
            drop(snapshot);
            let end = document.source_descriptor().bytes;
            document
                .accept_edit(document.source_descriptor(), end..end, replacement)
                .unwrap();
        }

        let mut drain = document.drain_retired_source_roots();
        assert_eq!(drain.len(), expected.len());
        for (descriptor, weak) in expected {
            let retired = drain.next().expect("one FIFO root per edit");
            assert_eq!(retired.descriptor(), descriptor);
            assert!(weak.upgrade().is_some());
            drop(retired);
            assert!(weak.upgrade().is_none());
        }
        assert!(drain.next().is_none());
    }

    #[test]
    fn full_retirement_queue_backpressures_before_clocks_or_candidate_change() {
        let mut document = LiveDocumentStore::new("", 8).unwrap();
        for _ in 0..SOURCE_RETIREMENT_QUEUE_CAPACITY {
            let descriptor = document.source_descriptor();
            document
                .accept_edit(descriptor, descriptor.bytes..descriptor.bytes, "x")
                .unwrap();
        }
        assert_eq!(
            document.retired_source_root_count(),
            SOURCE_RETIREMENT_QUEUE_CAPACITY
        );

        // Promote the latest queued parse so a real source-bound candidate can
        // be present when the backpressured edit arrives.
        document.promote_latest_parse().unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let candidate = document.begin_candidate(token).unwrap();
        let before = document.clocks();
        let before_text = document.query_source().materialize_for_testing();
        let descriptor = document.source_descriptor();

        let error = document
            .accept_edit(descriptor, descriptor.bytes..descriptor.bytes, "!")
            .unwrap_err();
        assert_eq!(error, LiveDocumentError::SourceRetirementBackpressure);
        assert_eq!(document.clocks(), before);
        assert_eq!(document.candidate_epoch(), Some(candidate));
        assert_eq!(
            document.query_source().materialize_for_testing(),
            before_text
        );
        assert_eq!(
            document.retired_source_root_count(),
            SOURCE_RETIREMENT_QUEUE_CAPACITY
        );
    }

    #[test]
    fn logical_byte_budget_backpressures_before_clocks_or_candidate_change() {
        let mut document = LiveDocumentStore::new("base", 8).unwrap();
        // A small injected limit proves the production 256 MiB policy without
        // allocating hundreds of MiB in the unit test.
        document.retired_source_roots.byte_capacity = 9;
        let initial = document.source_descriptor();
        document
            .accept_edit(initial, initial.bytes..initial.bytes, "x")
            .unwrap();
        assert_eq!(document.retired_source_logical_bytes(), 4);

        document.promote_latest_parse().unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let candidate = document.begin_candidate(token).unwrap();
        let before = document.clocks();
        let expected = document.source_descriptor();
        let error = document
            .accept_edit(expected, expected.bytes..expected.bytes, "y")
            .unwrap_err();
        assert_eq!(
            error,
            LiveDocumentError::SourceRetirementByteBackpressure {
                required: 6,
                available: 5,
            }
        );
        assert_eq!(document.clocks(), before);
        assert_eq!(document.candidate_epoch(), Some(candidate));
        assert_eq!(document.retired_source_logical_bytes(), 4);
    }

    #[test]
    fn coordinator_rejection_queues_the_unpublished_prepared_root() {
        let mut document = LiveDocumentStore::new("base", 8).unwrap();
        // Deliberately drift this unit-test actor so coordinator preparation
        // rejects the next otherwise-valid source transition.
        document
            .source
            .apply_edit(SourceRevision(0), 0..0, "drift-")
            .unwrap();
        let expected = document.source_descriptor();
        let before = document.clocks();
        let replacement = "x".repeat(2 * 1024 * 1024);

        let error = document
            .accept_edit(expected, expected.bytes..expected.bytes, &replacement)
            .unwrap_err();
        assert_eq!(
            error,
            LiveDocumentError::Coordinator(CoordinatorError::InvalidTransition)
        );
        assert_eq!(document.clocks(), before);
        assert_eq!(document.retired_source_root_count(), 1);

        let retired = document.take_retired_source_root().unwrap();
        assert_eq!(retired.descriptor().revision, SourceRevision(2));
        assert_eq!(
            retired.descriptor().bytes,
            expected.bytes + replacement.len()
        );
        let weak = retired.weak_observer_for_testing();
        assert!(weak.upgrade().is_some());
        drop(retired);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn candidate_cancellation_rejection_also_queues_unpublished_root() {
        let mut document = LiveDocumentStore::new("base", 8).unwrap();
        let token = document.active_parse_plan().unwrap().token;
        let epoch = document.begin_candidate(token).unwrap();

        // Install a different valid ticket temporarily. The actor's epoch
        // check rejects cancellation before taking the candidate, exercising
        // the post-prepare failure lane without forging a ticket.
        let replacement_ticket = document.arena.begin_build().unwrap();
        let original_ticket = document
            .candidate
            .as_mut()
            .unwrap()
            .ticket
            .replace(replacement_ticket);
        let before = document.clocks();
        let expected = document.source_descriptor();
        let error = document
            .accept_edit(expected, expected.bytes..expected.bytes, "!")
            .unwrap_err();
        assert_eq!(error, LiveDocumentError::CandidateStale);
        assert_eq!(document.clocks(), before);
        assert_eq!(document.candidate_epoch(), Some(epoch));
        assert_eq!(document.retired_source_root_count(), 1);

        // Restore the valid candidate and explicitly retire the temporary
        // empty build so the test itself leaves no orphan lifecycle.
        let replacement_ticket = std::mem::replace(
            &mut document.candidate.as_mut().unwrap().ticket,
            original_ticket,
        )
        .expect("temporary build ticket is present");
        let replacement_build = document
            .arena
            .begin_build_abort(replacement_ticket)
            .unwrap();
        assert!(
            document
                .arena
                .poll_build_abort(replacement_build, 0)
                .unwrap()
                .complete
        );
        document.require_live_epoch(epoch).unwrap();

        let retired = document.take_retired_source_root().unwrap();
        assert_eq!(retired.descriptor().revision, SourceRevision(1));
        let weak = retired.weak_observer_for_testing();
        drop(retired);
        assert!(weak.upgrade().is_none());
    }
}

//! Platform-neutral, source-only Flark v3 parser endpoint.
//!
//! This is the serialized state machine shared by the future native registry
//! and Web Worker slot. It deliberately stops before grammar, publication,
//! registry, and FFI integration. Source synchronization is installed in two
//! phases: an unpublished Crop root (or edit result) first produces a credited
//! wire acknowledgement, and only an accepted exact event receipt can make
//! that source eligible for fact scanning.

use std::{fmt, sync::Arc};

use flark_engine::m11_host::M11_CANDIDATE_ARENA_MAX_SLOTS;
use flark_engine::{
    ArenaLimits, DocumentRuntime, DocumentRuntimeConfig, DocumentRuntimeError, DocumentState,
    IncrementalSourceFactsPlan, ParserProfileId, PersistentSourceFactsDeltaWitness,
    PersistentSourceFactsInfo, RuntimeSourceFactsPoll, SourceFactCheckpoint, SourceFactRootPage,
    SourceFactsCompletion, SourceFactsRootLimits, SourceFactsScanProfile, SourceFactsWork,
    SourceRevision, SourceSeedBuilder, SourceSnapshotLease, SourceStore, SourceUtf16Operation,
    SourceVersion, PERSISTENT_SOURCE_FACTS_CHECKPOINT_ROOT_GUARD_ALGORITHM,
    SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX, SOURCE_FACT_SOURCE_BYTES_PER_POLL_MAX,
};

use crate::v3_session_wire::{
    decode_command, encode_event_into, Command, DecodeError, DrainGrant, DrainProgress,
    EditCommand, EncodeError, Event, EventBody, EventDisposition, EventReceiptCommand,
    InlineRefinementCommand, InlineRefinementUnavailableEvent, ObservedSourceReplicaVersion,
    OpenMode, SessionBinding, SnapshotCommand, SourceCertificationReceipt,
    SourceFactCheckpointWire, SourceFactsCompletionEvent, SourceFactsDeltaBeginEvent,
    SourceFactsDeltaCompletionEvent, SourceFactsDeltaPageEvent, SourceFactsPageEvent,
    SourceReceiptDisposition, SourceStamp, SupersedeCommand, ViewportPresentationCommand,
    ViewportPresentationUnavailableEvent, PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM,
};
use crate::{
    v3_candidate_endpoint::{
        CandidateCredit, CandidateEndpoint, CandidateEndpointError, CandidateEvent,
        CandidateEventBody, CandidatePoll, CandidateViewportPresentationEvent,
        CandidateViewportPresentationEventBody, HotInlineCredit, HotInlineEvent,
        HotInlineEventBody, ViewportInlineBatchCommand, ViewportInlineBatchLimits,
        ViewportPresentationCredit, ViewportPresentationUnavailableReason,
    },
    v3_publication_wire::{
        decode_host_poll_command, decode_inline_sidecar_host_poll_command,
        decode_viewport_presentation_host_poll_command,
        encode_event_into as encode_publication_event_into, encode_hot_inline_sidecar_event_into,
        encode_viewport_presentation_event_into, DecodeError as PublicationDecodeError,
        EncodeError as PublicationEncodeError, HostPollResult, HotInlineSidecarEvent,
        InlineSidecarHostPollResult, PublicationEvent, StructuralAck, ViewportPresentationEvent,
        ViewportPresentationHostPollResult,
    },
};

const DEFAULT_CHECKPOINT_SPACING_UTF16: usize = 4 * 1024;
const INLINE_REFINEMENT_UNAVAILABLE_LATE_QUERY: u32 = 1;
const INLINE_REFINEMENT_UNAVAILABLE_RETRYABLE_BUSY: u32 = 2;
const VIEWPORT_PRESENTATION_UNAVAILABLE_RETRYABLE_BUSY: u32 = 1;
const VIEWPORT_PRESENTATION_UNAVAILABLE_BUDGET_EXCEEDED: u32 = 2;
const VIEWPORT_PRESENTATION_UNAVAILABLE_DERIVATION: u32 = 3;
const VIEWPORT_PRESENTATION_UNAVAILABLE_HOST_REJECTED: u32 = 4;
pub(crate) const V3_PRODUCER_ARENA_MAX_LIVE_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;
pub const MAXIMUM_ENDPOINT_RETIREMENT_TRANSITIONS: usize = 256;

pub(crate) fn standard_document_runtime_config() -> DocumentRuntimeConfig {
    DocumentRuntimeConfig {
        arena_limits: ArenaLimits {
            max_slots: M11_CANDIDATE_ARENA_MAX_SLOTS,
            max_live_payload_bytes: V3_PRODUCER_ARENA_MAX_LIVE_PAYLOAD_BYTES,
            max_children_per_node: flark_engine::m11_host::M11_HOST_MAXIMUM_PROGRAM_CHILDREN,
        },
        ..DocumentRuntimeConfig::default()
    }
}

/// Bounded construction policy for one endpoint.
#[derive(Clone, Copy, Debug)]
pub struct EndpointConfig {
    runtime: DocumentRuntimeConfig,
    source_facts: SourceFactsScanProfile,
    source_facts_root_limits: SourceFactsRootLimits,
    parser_profile: ParserProfileId,
}

impl EndpointConfig {
    pub fn new(
        runtime: DocumentRuntimeConfig,
        source_facts: SourceFactsScanProfile,
        source_facts_root_limits: SourceFactsRootLimits,
        parser_profile: ParserProfileId,
    ) -> Result<Self, EndpointError> {
        if runtime.max_retired_sources == 0
            || runtime.max_retired_sources.checked_add(2).is_none()
            || runtime.max_retired_source_bytes == 0
            || runtime.max_retired_source_bytes > u32::MAX as usize
            || runtime.arena_limits.max_slots == 0
            || runtime.arena_limits.max_slots > u32::MAX as usize
            || runtime.arena_limits.max_live_payload_bytes == 0
            || runtime.arena_limits.max_children_per_node == 0
            || source_facts_root_limits.max_checkpoints() == 0
            || source_facts_root_limits.max_pages() == 0
            || source_facts_root_limits.max_resident_bytes() == 0
        {
            return Err(EndpointError::InvalidConfig);
        }
        Ok(Self {
            runtime,
            source_facts,
            source_facts_root_limits,
            parser_profile,
        })
    }

    /// Production-shaped defaults for the current syntax profile.
    pub fn standard() -> Result<Self, EndpointError> {
        Self::new(
            standard_document_runtime_config(),
            SourceFactsScanProfile::new(DEFAULT_CHECKPOINT_SPACING_UTF16)
                .map_err(|_| EndpointError::InvalidConfig)?,
            SourceFactsRootLimits::default(),
            ParserProfileId::new(1).ok_or(EndpointError::InvalidConfig)?,
        )
    }
}

/// Endpoint lifecycle and source authority are intentionally orthogonal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointLifecycle {
    AwaitingOpen,
    Opening,
    Open,
    Faulted,
    Closing,
    Closed,
    Removable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CertificationStatus {
    NotStarted,
    Scanning,
    Publishing,
    AwaitingPromotion,
    ExternallyEligible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointSourceStatus {
    Absent,
    Seeding {
        target: SourceStamp,
        next_utf16: u32,
        total_utf16: u32,
        intent_high_water: u32,
    },
    AwaitingInstallReceipt {
        target: SourceStamp,
        observed: ObservedSourceReplicaVersion,
    },
    Installed {
        target: SourceStamp,
        observed: ObservedSourceReplicaVersion,
        certification: CertificationStatus,
    },
    NeedsReseed,
    Closing,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointEventKind {
    Opened,
    SourceSynchronized,
    SourceFactsPage,
    SourceFactsCompleted,
    Failed,
    DrainProgress,
    Closed,
    PublicationBegin,
    PublicationPacket,
    PublicationCommit,
    PublicationDeliveryAcknowledged,
    InlinePublicationBegin,
    InlinePublicationPacket,
    InlinePublicationCommit,
    InlinePublicationDeliveryAcknowledged,
    ViewportPublicationBegin,
    ViewportPublicationPacket,
    ViewportPublicationCommit,
    ViewportPublicationDeliveryAcknowledged,
    InlineRefinementUnavailable,
    ViewportPresentationUnavailable,
    SourceFactsDeltaBegin,
    SourceFactsDeltaPage,
    SourceFactsDeltaCompleted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutstandingEventStatus {
    pub event_id: u32,
    pub kind: EndpointEventKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EndpointStatus {
    pub lifecycle: EndpointLifecycle,
    pub binding: Option<SessionBinding>,
    pub source: EndpointSourceStatus,
    pub outstanding_event: Option<OutstandingEventStatus>,
    pub close_latched: bool,
    pub failure_emitted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointCommandAction {
    Opened,
    SourcePageAccepted,
    SourceBatchAccepted,
    InlineRefinementAccepted,
    ViewportPresentationAccepted,
    Superseded,
    EventReceiptAccepted,
    CloseLatched,
    DrainAdvanced,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EndpointCommandReceipt {
    pub correlation_id: u32,
    pub action: EndpointCommandAction,
    pub outstanding_event: Option<OutstandingEventStatus>,
}

/// Explicit fuel for source scanning and faulted-runtime cleanup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EndpointPollFuel {
    pub maximum_source_bytes: usize,
    pub maximum_checkpoints: usize,
    pub maximum_retirement_transitions: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EndpointPollReceipt {
    pub source_bytes_examined: usize,
    pub source_bytes_buffered: usize,
    pub cursor_refills: usize,
    pub cursor_copy_bytes_upper_bound: usize,
    pub checkpoints_emitted: usize,
    pub source_fact_transitions: usize,
    pub released_source_leases: usize,
    pub released_source_bytes: usize,
    pub arena_transitions: usize,
    pub arena_nodes_reclaimed: usize,
    pub scan_complete: bool,
    pub certification: Option<CertificationStatus>,
    pub cleanup_complete: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EndpointCandidatePollReceipt {
    pub transitions: usize,
    pub cleanup_complete: bool,
    pub outstanding_event: Option<OutstandingEventStatus>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointHostPollAction {
    CreditAccepted,
    CandidateCancelled,
    DeliveryEmitted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EndpointHostPollReceipt {
    pub correlation_id: u32,
    pub action: EndpointHostPollAction,
    pub outstanding_event: Option<OutstandingEventStatus>,
}

impl EndpointPollReceipt {
    fn add_source_work(&mut self, work: SourceFactsWork) {
        self.source_bytes_examined = work.source_bytes_examined();
        self.source_bytes_buffered = work.source_bytes_buffered();
        self.cursor_refills = work.cursor_refills();
        self.cursor_copy_bytes_upper_bound = work.cursor_copy_bytes_upper_bound();
        self.checkpoints_emitted = work.checkpoints_emitted();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EndpointFailureCode {
    InvalidLifecycle = 1,
    InvalidSeed = 2,
    InvalidEdit = 3,
    SourceFacts = 4,
    SourceHashMismatch = 5,
    EventIdentityExhausted = 6,
    RejectedSourceReceipt = 7,
    RejectedCertificationReceipt = 8,
}

impl EndpointFailureCode {
    const fn code(self) -> u32 {
        self as u32
    }
}

#[derive(Debug)]
pub enum EndpointError {
    InvalidConfig,
    Decode(DecodeError),
    Encode(EncodeError),
    PublicationDecode(PublicationDecodeError),
    PublicationEncode(PublicationEncodeError),
    Candidate,
    EventCreditOccupied { event_id: u32 },
    NoOutstandingEvent,
    ReceiptMismatch { expected: u32, actual: u32 },
    InvalidLifecycle,
    InvalidSeed,
    InvalidEdit,
    InvalidReceipt,
    SourceFacts,
    EventIdentityExhausted,
    InvalidPollFuel,
}

impl fmt::Display for EndpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Flark v3 source endpoint failure: {self:?}")
    }
}

impl std::error::Error for EndpointError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            Self::Encode(error) => Some(error),
            Self::PublicationDecode(error) => Some(error),
            Self::PublicationEncode(error) => Some(error),
            _ => None,
        }
    }
}

impl From<DecodeError> for EndpointError {
    fn from(error: DecodeError) -> Self {
        Self::Decode(error)
    }
}

impl From<EncodeError> for EndpointError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl From<PublicationDecodeError> for EndpointError {
    fn from(error: PublicationDecodeError) -> Self {
        Self::PublicationDecode(error)
    }
}

impl From<PublicationEncodeError> for EndpointError {
    fn from(error: PublicationEncodeError) -> Self {
        Self::PublicationEncode(error)
    }
}

impl From<CandidateEndpointError> for EndpointError {
    fn from(_error: CandidateEndpointError) -> Self {
        Self::Candidate
    }
}

struct PendingSeed {
    builder: SourceSeedBuilder,
    target: SourceStamp,
    through_intent_sequence: u32,
}

struct PendingInstall {
    store: SourceStore,
    target: SourceStamp,
    observed: ObservedSourceReplicaVersion,
}

#[derive(Clone, Copy)]
struct PendingEditInstall {
    target: SourceStamp,
    observed: ObservedSourceReplicaVersion,
    dropped_intent_entries: u32,
    dropped_payload_utf16: u32,
    dropped_deleted_utf16: u32,
    dropped_operation_count: u32,
}

struct PendingCertificationPublication {
    completion: SourceFactsCompletionEvent,
    pages: Vec<Arc<SourceFactRootPage>>,
    next_page: usize,
}

/// Move-only local authority awaiting the external delta-certification gate.
///
/// This cannot be converted into a clean `CertifiedSource`: doing so would
/// either republish the whole document or silently weaken the exact splice
/// proof. The eventual candidate handoff consumes all three fields together.
#[allow(dead_code)] // Consumed by the delta-certification ACK seam in the next vertical slice.
struct PendingIncrementalCandidate {
    target_source: SourceSnapshotLease,
    target_facts: PersistentSourceFactsInfo,
    source_facts_delta: Box<PersistentSourceFactsDeltaWitness>,
}

struct PendingIncrementalPublication {
    candidate: PendingIncrementalCandidate,
    begin: SourceFactsDeltaBeginEvent,
    completion: SourceFactsCompletionEvent,
    next_page: u64,
    replacement_checkpoints_accepted: u64,
    replacement_checkpoint_hash128: [u32; 4],
    staged_page: Option<SourceFactsDeltaPageEvent>,
    terminal: Option<SourceFactsDeltaCompletionEvent>,
}

enum ActiveSourceFacts {
    Clean,
    Incremental(IncrementalSourceFactsPlan),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EventTransition {
    Opened(OpenMode),
    IntermediateSeed,
    InstallSeed,
    InstallEdit,
    CertificationPage {
        certification_id: u32,
        page_ordinal: u32,
    },
    CertificationCompletion {
        certification_id: u32,
    },
    DeltaBegin {
        certification_id: u32,
    },
    DeltaPage {
        certification_id: u32,
        replacement_page_ordinal: u32,
    },
    DeltaCompletion {
        certification_id: u32,
    },
    InlineRefinementUnavailable,
    ViewportPresentationUnavailable,
    Failed,
    DrainProgress {
        complete: bool,
    },
    Closed,
    Candidate(CandidateCredit),
    HotInline(HotInlineCredit),
    ViewportPresentation(ViewportPresentationCredit),
}

enum OutstandingPayload {
    Session(Box<EventBody>),
    Publication(Box<CandidateEventBody>),
    HotInline(Box<HotInlineEventBody>),
    ViewportPresentation(Box<CandidateViewportPresentationEventBody>),
}

struct OutstandingEvent {
    event_id: u32,
    payload: OutstandingPayload,
    kind: EndpointEventKind,
    transition: EventTransition,
    drain_grant: Option<DrainGrant>,
}

#[cfg(test)]
impl OutstandingEvent {
    const fn session_body(&self) -> Option<EventBody> {
        match &self.payload {
            OutstandingPayload::Session(body) => Some(**body),
            OutstandingPayload::Publication(_) => None,
            OutstandingPayload::HotInline(_) => None,
            OutstandingPayload::ViewportPresentation(_) => None,
        }
    }
}

/// One exclusively serialized source endpoint. It is `Send` through its
/// engine-owned capabilities and deliberately not an implicit shared actor.
pub struct Endpoint {
    config: EndpointConfig,
    lifecycle: EndpointLifecycle,
    /// Before open this is the previous generation accepted by recovery.
    decode_binding: Option<SessionBinding>,
    binding: Option<SessionBinding>,
    next_event_id: u32,
    next_certification_id: u32,
    outstanding: Option<OutstandingEvent>,
    close_latched: bool,
    failure_emitted: bool,
    needs_reseed: bool,

    pending_seed: Option<PendingSeed>,
    pending_install: Option<PendingInstall>,
    pending_edit_install: Option<PendingEditInstall>,
    pending_certification: Option<PendingCertificationPublication>,
    pending_incremental_publication: Option<PendingIncrementalPublication>,
    active_source_facts: Option<ActiveSourceFacts>,
    runtime: Option<DocumentRuntime>,
    runtime_poisoned: bool,
    installed_target: Option<SourceStamp>,
    observed: Option<ObservedSourceReplicaVersion>,
    certification: CertificationStatus,
    active_candidate_certification: Option<SourceCertificationReceipt>,
    last_external_certification: Option<SourceCertificationReceipt>,
    active_viewport_presentation_generation: Option<u32>,
    candidate: CandidateEndpoint,
}

impl fmt::Debug for Endpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Endpoint")
            .field("status", &self.status())
            .finish()
    }
}

impl Endpoint {
    /// Creates an endpoint that accepts exactly one fresh open.
    pub fn fresh(config: EndpointConfig) -> Self {
        Self::new(config, None)
    }

    /// Creates an endpoint that accepts only the exact next recovery binding.
    pub fn recovery(
        previous: SessionBinding,
        config: EndpointConfig,
    ) -> Result<Self, EndpointError> {
        if !previous.is_valid() || previous.worker_generation == u32::MAX {
            return Err(EndpointError::InvalidConfig);
        }
        Ok(Self::new(config, Some(previous)))
    }

    fn new(config: EndpointConfig, previous: Option<SessionBinding>) -> Self {
        Self {
            config,
            lifecycle: EndpointLifecycle::AwaitingOpen,
            decode_binding: previous,
            binding: None,
            next_event_id: 1,
            next_certification_id: 1,
            outstanding: None,
            close_latched: false,
            failure_emitted: false,
            needs_reseed: false,
            pending_seed: None,
            pending_install: None,
            pending_edit_install: None,
            pending_certification: None,
            pending_incremental_publication: None,
            active_source_facts: None,
            runtime: None,
            runtime_poisoned: false,
            installed_target: None,
            observed: None,
            certification: CertificationStatus::NotStarted,
            active_candidate_certification: None,
            last_external_certification: None,
            active_viewport_presentation_generation: None,
            candidate: CandidateEndpoint::new(),
        }
    }

    #[must_use]
    pub fn status(&self) -> EndpointStatus {
        EndpointStatus {
            lifecycle: self.lifecycle,
            binding: self.binding,
            source: self.source_status(),
            outstanding_event: self.outstanding_status(),
            close_latched: self.close_latched,
            failure_emitted: self.failure_emitted,
        }
    }

    fn source_status(&self) -> EndpointSourceStatus {
        if matches!(
            self.lifecycle,
            EndpointLifecycle::Closed | EndpointLifecycle::Removable
        ) {
            return EndpointSourceStatus::Closed;
        }
        if self.lifecycle == EndpointLifecycle::Closing {
            return EndpointSourceStatus::Closing;
        }
        if self.needs_reseed {
            return EndpointSourceStatus::NeedsReseed;
        }
        if let Some(pending) = &self.pending_install {
            return EndpointSourceStatus::AwaitingInstallReceipt {
                target: pending.target,
                observed: pending.observed,
            };
        }
        if let Some(pending) = self.pending_edit_install {
            return EndpointSourceStatus::AwaitingInstallReceipt {
                target: pending.target,
                observed: pending.observed,
            };
        }
        if let Some(seed) = &self.pending_seed {
            return EndpointSourceStatus::Seeding {
                target: seed.target,
                next_utf16: u32::try_from(seed.builder.observed_utf16_len())
                    .expect("wire-bounded seed coordinate must fit u32"),
                total_utf16: u32::try_from(seed.builder.expected_utf16_len())
                    .expect("wire-bounded seed coordinate must fit u32"),
                intent_high_water: seed.through_intent_sequence,
            };
        }
        match (self.installed_target, self.observed) {
            (Some(target), Some(observed)) => EndpointSourceStatus::Installed {
                target,
                observed,
                certification: self.certification,
            },
            _ => EndpointSourceStatus::Absent,
        }
    }

    #[must_use]
    pub fn outstanding_status(&self) -> Option<OutstandingEventStatus> {
        self.outstanding
            .as_ref()
            .map(|outstanding| OutstandingEventStatus {
                event_id: outstanding.event_id,
                kind: outstanding.kind,
            })
    }

    /// Encodes the same outstanding event on every retry until its exact
    /// receipt returns the one global credit.
    pub fn encode_outstanding_event(&self, output: &mut [u8]) -> Result<usize, EndpointError> {
        let outstanding = self
            .outstanding
            .as_ref()
            .ok_or(EndpointError::NoOutstandingEvent)?;
        let binding = self.binding.ok_or(EndpointError::InvalidLifecycle)?;
        match &outstanding.payload {
            OutstandingPayload::Session(body) => encode_event_into(
                Event {
                    binding,
                    event_id: outstanding.event_id,
                    body: **body,
                },
                binding,
                outstanding.drain_grant,
                output,
            )
            .map_err(EndpointError::Encode),
            OutstandingPayload::Publication(body) => {
                let body = body.borrowed().map_err(EndpointError::PublicationDecode)?;
                encode_publication_event_into(
                    PublicationEvent {
                        event_id: outstanding.event_id,
                        binding,
                        body,
                    },
                    binding,
                    output,
                )
                .map_err(EndpointError::PublicationEncode)
            }
            OutstandingPayload::HotInline(body) => {
                let body = body.borrowed().map_err(EndpointError::PublicationDecode)?;
                encode_hot_inline_sidecar_event_into(
                    HotInlineSidecarEvent {
                        event_id: outstanding.event_id,
                        binding,
                        body,
                    },
                    binding,
                    output,
                )
                .map_err(EndpointError::PublicationEncode)
            }
            OutstandingPayload::ViewportPresentation(body) => {
                let body = body.borrowed().map_err(EndpointError::PublicationDecode)?;
                encode_viewport_presentation_event_into(
                    ViewportPresentationEvent {
                        event_id: outstanding.event_id,
                        binding,
                        body,
                    },
                    binding,
                    output,
                )
                .map_err(EndpointError::PublicationEncode)
            }
        }
    }

    /// Decodes and applies one schema-3 command. No second wire parser exists
    /// in this layer.
    pub fn dispatch(&mut self, bytes: &[u8]) -> Result<EndpointCommandReceipt, EndpointError> {
        let established = self.binding.or(self.decode_binding);
        let decoded = match decode_command(bytes, established) {
            Ok(decoded) => decoded,
            Err(error) => {
                // A malformed frame cannot identify an exact source target.
                // Once open, fail the replica closed unless an outstanding
                // event credit forbids every non-receipt transition.
                if self.outstanding.is_none() && self.lifecycle == EndpointLifecycle::Open {
                    self.poison_runtime();
                    self.pending_seed = None;
                    self.pending_install = None;
                    self.pending_edit_install = None;
                    self.fail_closed(EndpointFailureCode::InvalidLifecycle);
                }
                return Err(EndpointError::Decode(error));
            }
        };

        if let Some(outstanding) = &self.outstanding {
            match decoded.command {
                Command::EventReceipt(receipt) if receipt.event_id == outstanding.event_id => {}
                Command::BeginClose { .. } => {
                    self.begin_close()?;
                    return Ok(self.command_receipt(
                        decoded.correlation_id,
                        EndpointCommandAction::CloseLatched,
                    ));
                }
                Command::EventReceipt(receipt) => {
                    return Err(EndpointError::ReceiptMismatch {
                        expected: outstanding.event_id,
                        actual: receipt.event_id,
                    });
                }
                _ => {
                    return Err(EndpointError::EventCreditOccupied {
                        event_id: outstanding.event_id,
                    });
                }
            }
        }

        let action = match decoded.command {
            Command::Open { binding, mode } => self.handle_open(binding, mode)?,
            Command::Snapshot(command) => self.handle_snapshot(command)?,
            Command::Edit(command) => self.handle_edit(command)?,
            Command::RefineInline(command) => self.handle_inline_refinement(command)?,
            Command::PresentViewport(command) => self.handle_viewport_presentation(command)?,
            Command::Supersede(command) => self.handle_supersede(command)?,
            Command::EventReceipt(receipt) => self.handle_receipt(receipt)?,
            Command::BeginClose { .. } => {
                self.begin_close()?;
                EndpointCommandAction::CloseLatched
            }
            Command::Drain(grant) => self.handle_drain(grant)?,
        };
        Ok(self.command_receipt(decoded.correlation_id, action))
    }

    fn command_receipt(
        &self,
        correlation_id: u32,
        action: EndpointCommandAction,
    ) -> EndpointCommandReceipt {
        EndpointCommandReceipt {
            correlation_id,
            action,
            outstanding_event: self.outstanding_status(),
        }
    }

    fn handle_open(
        &mut self,
        binding: SessionBinding,
        mode: OpenMode,
    ) -> Result<EndpointCommandAction, EndpointError> {
        let recovering_fault = self.lifecycle == EndpointLifecycle::Faulted
            && mode == OpenMode::Recovery
            && self.runtime.is_none();
        if self.lifecycle != EndpointLifecycle::AwaitingOpen && !recovering_fault {
            return Err(EndpointError::InvalidLifecycle);
        }
        if recovering_fault {
            self.failure_emitted = false;
            self.runtime_poisoned = false;
            self.needs_reseed = false;
            self.installed_target = None;
            self.observed = None;
            self.certification = CertificationStatus::NotStarted;
            self.active_candidate_certification = None;
            self.last_external_certification = None;
            self.active_viewport_presentation_generation = None;
        }
        self.binding = Some(binding);
        self.decode_binding = Some(binding);
        self.lifecycle = EndpointLifecycle::Opening;
        self.emit(
            EventBody::Opened(mode),
            EndpointEventKind::Opened,
            EventTransition::Opened(mode),
            None,
        )?;
        Ok(EndpointCommandAction::Opened)
    }

    fn ensure_open(&self) -> Result<(), EndpointError> {
        if self.lifecycle == EndpointLifecycle::Open && !self.close_latched {
            Ok(())
        } else {
            Err(EndpointError::InvalidLifecycle)
        }
    }

    fn handle_snapshot(
        &mut self,
        command: SnapshotCommand<'_>,
    ) -> Result<EndpointCommandAction, EndpointError> {
        self.ensure_open()?;
        if self.runtime.is_some() || self.runtime_poisoned || self.pending_install.is_some() {
            return self.reject_seed(EndpointFailureCode::InvalidSeed);
        }

        if command.is_seed() {
            if self.pending_seed.is_some() {
                return self.reject_seed(EndpointFailureCode::InvalidSeed);
            }
            self.pending_seed = Some(PendingSeed {
                builder: SourceStore::seed(
                    SourceRevision::new(u64::from(command.base_ui_revision)),
                    command.total_utf16_length as usize,
                ),
                target: command.target_stamp,
                through_intent_sequence: command.through_intent_sequence,
            });
        }

        let pending = match self.pending_seed.as_mut() {
            Some(pending)
                if pending.target == command.target_stamp
                    && pending.through_intent_sequence == command.through_intent_sequence
                    && pending.builder.revision().get() == u64::from(command.base_ui_revision)
                    && pending.builder.expected_utf16_len()
                        == command.total_utf16_length as usize
                    && pending.builder.observed_utf16_len() == command.start_utf16 as usize =>
            {
                pending
            }
            _ => return self.reject_seed(EndpointFailureCode::InvalidSeed),
        };

        if pending
            .builder
            .append_page(
                command.start_utf16 as usize..command.end_utf16 as usize,
                command.source,
            )
            .is_err()
        {
            return self.reject_seed(EndpointFailureCode::InvalidSeed);
        }

        if command.end_utf16 < command.total_utf16_length {
            let acknowledgement = command.acknowledgement(None);
            self.emit(
                EventBody::SourceSynchronized(acknowledgement),
                EndpointEventKind::SourceSynchronized,
                EventTransition::IntermediateSeed,
                None,
            )?;
            return Ok(EndpointCommandAction::SourcePageAccepted);
        }

        let pending = self
            .pending_seed
            .take()
            .expect("validated final page must own a pending seed");
        let store = match pending.builder.finalize() {
            Ok(store) => store,
            Err(_) => return self.reject_seed(EndpointFailureCode::InvalidSeed),
        };
        let version = store.version();
        if !version_matches_stamp(version, pending.target) {
            return self.reject_seed(EndpointFailureCode::InvalidSeed);
        }
        let observed = match observed(version, pending.through_intent_sequence) {
            Some(observed) => observed,
            None => return self.reject_seed(EndpointFailureCode::InvalidSeed),
        };
        self.pending_install = Some(PendingInstall {
            store,
            target: pending.target,
            observed,
        });
        let acknowledgement = command.acknowledgement(Some(observed));
        self.emit(
            EventBody::SourceSynchronized(acknowledgement),
            EndpointEventKind::SourceSynchronized,
            EventTransition::InstallSeed,
            None,
        )?;
        Ok(EndpointCommandAction::SourcePageAccepted)
    }

    fn reject_seed<T>(&mut self, code: EndpointFailureCode) -> Result<T, EndpointError> {
        self.pending_seed = None;
        self.pending_install = None;
        self.pending_edit_install = None;
        self.fail_closed(code);
        Err(EndpointError::InvalidSeed)
    }

    fn handle_edit(
        &mut self,
        command: EditCommand<'_>,
    ) -> Result<EndpointCommandAction, EndpointError> {
        self.ensure_open()?;
        let (Some(installed_target), Some(installed_observed), Some(runtime)) =
            (self.installed_target, self.observed, self.runtime.as_ref())
        else {
            return self.reject_edit();
        };
        if self.pending_seed.is_some()
            || self.pending_install.is_some()
            || self.pending_edit_install.is_some()
            || command.base_stamp() != installed_target
            || command.first_sequence
                != installed_observed
                    .intent_high_water
                    .checked_add(1)
                    .unwrap_or(0)
        {
            return self.reject_edit();
        }
        let current = runtime
            .current_source_version()
            .ok_or(EndpointError::InvalidEdit)?;
        if !version_matches_stamp(current, command.base_stamp()) {
            return self.reject_edit();
        }

        // Recover any exact parser base before the source root advances. This
        // is stronger than the later generic cancellation: a rapid edit may
        // arrive while an exact-delta candidate is parsing, building, or
        // streaming, and the next incremental SourceFacts transaction still
        // needs the retained structural base.
        if self
            .candidate
            .cancel_for_edit(
                self.runtime
                    .as_mut()
                    .expect("a validated edit owns a runtime"),
            )
            .is_err()
        {
            self.poison_runtime();
            self.fail_closed(EndpointFailureCode::SourceFacts);
            return Err(EndpointError::Candidate);
        }
        self.active_viewport_presentation_generation = None;

        // The first list-local production lane deliberately admits one intent
        // and one operation. Larger batches stay correct through the existing
        // definitive path and must not accidentally preserve a rolling edit
        // island across unrelated changes.
        let single_operation = if command.intent_count == 1 && command.operation_count == 1 {
            command
                .intents()
                .next()
                .and_then(|intent| intent.operations.into_iter().next())
        } else {
            None
        };
        if let Some(operation) = single_operation {
            let changed_bytes = self.runtime.as_ref().and_then(|runtime| {
                let source = runtime.snapshot_current_source().ok()?;
                if source.version() != current {
                    return None;
                }
                let start = source
                    .byte_offset_for_utf16(operation.start_utf16 as usize)
                    .ok()?;
                let end = source
                    .byte_offset_for_utf16(operation.end_utf16 as usize)
                    .ok()?;
                Some(start..end)
            });
            if let Some(changed_bytes) = changed_bytes {
                let _ = self.candidate.prepare_bullet_list_local_edit(
                    self.runtime
                        .as_ref()
                        .expect("a validated edit owns a runtime"),
                    changed_bytes,
                    operation.start_utf16 as usize..operation.end_utf16 as usize,
                );
            } else {
                self.candidate.discard_bullet_list_local_edit_plan();
            }
        } else {
            self.candidate.discard_bullet_list_local_edit_plan();
        }

        let mut expected_sequence = command.first_sequence;
        let mut final_target = installed_target;
        let mut final_sequence = installed_observed.intent_high_water;
        let mut deleted_utf16 = 0_u32;
        // Late inline work is revision-bound and must stop being observable
        // before the source root advances. Reclamation remains explicitly
        // fuelled after the edit acknowledgement returns.
        self.candidate.cancel_hot_inline();
        for intent in command.intents() {
            if intent.sequence != expected_sequence || intent.base_stamp != final_target {
                return self.reject_edit();
            }
            let current = self
                .runtime
                .as_ref()
                .and_then(DocumentRuntime::current_source_version)
                .ok_or(EndpointError::InvalidEdit)?;
            if current.revision().get() != u64::from(intent.base_ui_revision)
                || !version_matches_stamp(current, intent.base_stamp)
            {
                return self.reject_edit();
            }
            let mut operations = Vec::new();
            if operations
                .try_reserve_exact(intent.operations.len())
                .is_err()
            {
                return self.reject_edit();
            }
            for operation in intent.operations {
                deleted_utf16 =
                    match deleted_utf16.checked_add(operation.end_utf16 - operation.start_utf16) {
                        Some(value) => value,
                        None => return self.reject_edit(),
                    };
                operations.push(SourceUtf16Operation::new(
                    operation.start_utf16 as usize..operation.end_utf16 as usize,
                    operation.replacement,
                ));
            }
            let result = self
                .runtime
                .as_mut()
                .expect("validated edit owns a runtime")
                .apply_utf16_edit_intent(
                    current,
                    SourceRevision::new(u64::from(intent.ui_revision)),
                    &operations,
                );
            let receipt = match result {
                Ok(receipt) => receipt,
                Err(_) => return self.reject_edit(),
            };
            if !version_matches_stamp(receipt.source().current(), intent.target_stamp) {
                return self.reject_edit();
            }
            // The source-only endpoint has no active parser candidate. Retire
            // exactly the one predecessor admitted by this intent before the
            // next bounded intent, so a valid wire batch is not accidentally
            // capped by the runtime's cross-command retirement backlog.
            self.runtime
                .as_mut()
                .expect("a committed edit retains its runtime")
                .poll_retirement(1);
            final_target = intent.target_stamp;
            final_sequence = intent.sequence;
            expected_sequence = match expected_sequence.checked_add(1) {
                Some(value) => value,
                None => return self.reject_edit(),
            };
        }
        if final_target != command.target_stamp() || final_sequence != command.last_sequence {
            return self.reject_edit();
        }
        let final_version = self
            .runtime
            .as_ref()
            .and_then(DocumentRuntime::current_source_version)
            .ok_or(EndpointError::InvalidEdit)?;
        let observed = match observed(final_version, final_sequence) {
            Some(observed) => observed,
            None => return self.reject_edit(),
        };
        self.pending_edit_install = Some(PendingEditInstall {
            target: final_target,
            observed,
            dropped_intent_entries: command.intent_count,
            dropped_payload_utf16: command.payload_utf16,
            dropped_deleted_utf16: deleted_utf16,
            dropped_operation_count: command.operation_count,
        });
        self.certification = CertificationStatus::NotStarted;
        self.emit(
            EventBody::SourceSynchronized(command.acknowledgement(observed)),
            EndpointEventKind::SourceSynchronized,
            EventTransition::InstallEdit,
            None,
        )?;
        Ok(EndpointCommandAction::SourceBatchAccepted)
    }

    fn handle_inline_refinement(
        &mut self,
        command: InlineRefinementCommand,
    ) -> Result<EndpointCommandAction, EndpointError> {
        self.ensure_open()?;
        if self.certification != CertificationStatus::ExternallyEligible {
            return Err(EndpointError::InvalidLifecycle);
        }
        let runtime = self
            .runtime
            .as_mut()
            .ok_or(EndpointError::InvalidLifecycle)?;
        if let Err(error) = self.candidate.request_hot_inline(runtime, command) {
            let unavailable_reason = match &error {
                CandidateEndpointError::Derive(_)
                | CandidateEndpointError::RecursiveGreenParagraph(_)
                | CandidateEndpointError::InlineRefinementUnavailable => {
                    Some(INLINE_REFINEMENT_UNAVAILABLE_LATE_QUERY)
                }
                CandidateEndpointError::Busy => Some(INLINE_REFINEMENT_UNAVAILABLE_RETRYABLE_BUSY),
                _ => None,
            };
            if let Some(reason_code) = unavailable_reason {
                self.emit(
                    EventBody::InlineRefinementUnavailable(InlineRefinementUnavailableEvent {
                        refinement_generation: command.refinement_generation,
                        reason_code,
                    }),
                    EndpointEventKind::InlineRefinementUnavailable,
                    EventTransition::InlineRefinementUnavailable,
                    None,
                )?;
                return Ok(EndpointCommandAction::InlineRefinementAccepted);
            }
            return Err(error.into());
        }
        self.active_viewport_presentation_generation = None;
        Ok(EndpointCommandAction::InlineRefinementAccepted)
    }

    fn handle_viewport_presentation(
        &mut self,
        command: ViewportPresentationCommand,
    ) -> Result<EndpointCommandAction, EndpointError> {
        self.ensure_open()?;
        if self.certification != CertificationStatus::ExternallyEligible
            || self.binding != Some(command.binding)
        {
            return Err(EndpointError::InvalidLifecycle);
        }
        let runtime = self
            .runtime
            .as_ref()
            .ok_or(EndpointError::InvalidLifecycle)?;
        let request = self.candidate.request_viewport_inline_batch(
            runtime,
            ViewportInlineBatchCommand {
                binding: command.binding,
                viewport_generation: command.viewport_generation,
                source_version: command.source_version,
                base_ack: command.base_ack,
                start_entry_ordinal: command.start_block_ordinal,
                start_byte_offset: command.start_utf8,
                start_utf16_offset: command.start_utf16,
                end_byte_offset: command.requested_end_utf8,
                end_utf16_offset: command.requested_end_utf16,
                limits: ViewportInlineBatchLimits {
                    maximum_structural_entries: command.limits.maximum_structural_entries,
                    maximum_storage_pages: command.limits.maximum_storage_pages,
                    maximum_inline_leaves: command.limits.maximum_inline_leaves,
                    maximum_inline_leaf_source_bytes: command
                        .limits
                        .maximum_inline_leaf_source_bytes,
                    maximum_inline_source_bytes: u64::from(
                        command.limits.maximum_inline_source_bytes,
                    ),
                    maximum_fact_records: u64::from(command.limits.maximum_fact_records),
                    maximum_projection_bytes: u64::from(command.limits.maximum_encoded_frame_bytes),
                    maximum_parser_transitions: u64::from(
                        command.limits.maximum_parser_transitions,
                    ),
                },
            },
        );
        if let Err(error) = request {
            let reason_code = match &error {
                CandidateEndpointError::Busy => {
                    Some(VIEWPORT_PRESENTATION_UNAVAILABLE_RETRYABLE_BUSY)
                }
                CandidateEndpointError::ViewportInlineLimitExceeded(_) => {
                    Some(VIEWPORT_PRESENTATION_UNAVAILABLE_BUDGET_EXCEEDED)
                }
                CandidateEndpointError::Derive(_) => {
                    Some(VIEWPORT_PRESENTATION_UNAVAILABLE_DERIVATION)
                }
                _ => None,
            };
            if let Some(reason_code) = reason_code {
                self.emit(
                    EventBody::ViewportPresentationUnavailable(
                        ViewportPresentationUnavailableEvent {
                            viewport_generation: command.viewport_generation,
                            reason_code,
                        },
                    ),
                    EndpointEventKind::ViewportPresentationUnavailable,
                    EventTransition::ViewportPresentationUnavailable,
                    None,
                )?;
                return Ok(EndpointCommandAction::ViewportPresentationAccepted);
            }
            return Err(error.into());
        }
        self.active_viewport_presentation_generation = Some(command.viewport_generation);
        Ok(EndpointCommandAction::ViewportPresentationAccepted)
    }

    fn reject_edit<T>(&mut self) -> Result<T, EndpointError> {
        self.poison_runtime();
        self.fail_closed(EndpointFailureCode::InvalidEdit);
        Err(EndpointError::InvalidEdit)
    }

    fn handle_supersede(
        &mut self,
        command: SupersedeCommand,
    ) -> Result<EndpointCommandAction, EndpointError> {
        self.ensure_open()?;
        let current_revision = self.observed.map(|observed| observed.revision);
        if current_revision.is_some_and(|revision| command.target_ui_revision < revision) {
            return Err(EndpointError::InvalidLifecycle);
        }
        self.cancel_derived()?;
        self.certification = CertificationStatus::NotStarted;
        Ok(EndpointCommandAction::Superseded)
    }

    fn handle_receipt(
        &mut self,
        receipt: EventReceiptCommand,
    ) -> Result<EndpointCommandAction, EndpointError> {
        let outstanding = self
            .outstanding
            .take()
            .ok_or(EndpointError::NoOutstandingEvent)?;
        if receipt.event_id != outstanding.event_id {
            let expected = outstanding.event_id;
            self.outstanding = Some(outstanding);
            return Err(EndpointError::ReceiptMismatch {
                expected,
                actual: receipt.event_id,
            });
        }

        if self.close_latched
            && !matches!(
                outstanding.transition,
                EventTransition::DrainProgress { .. } | EventTransition::Closed
            )
        {
            // A close command invalidates every non-close transition but the
            // exact receipt is still the only way to return event credit.
            return Ok(EndpointCommandAction::EventReceiptAccepted);
        }

        match outstanding.transition {
            EventTransition::Opened(mode) => {
                if !plain_accepted_receipt(receipt) {
                    self.fail_closed(EndpointFailureCode::InvalidLifecycle);
                    return Err(EndpointError::InvalidReceipt);
                }
                let _ = mode;
                self.lifecycle = EndpointLifecycle::Open;
            }
            EventTransition::IntermediateSeed => {
                if !accepted_snapshot_receipt(receipt, 0) {
                    self.pending_seed = None;
                    self.fail_closed(EndpointFailureCode::RejectedSourceReceipt);
                    return Err(EndpointError::InvalidReceipt);
                }
            }
            EventTransition::InstallSeed => {
                let pending = self
                    .pending_install
                    .take()
                    .ok_or(EndpointError::InvalidReceipt)?;
                if !accepted_snapshot_receipt(receipt, pending.observed.revision) {
                    self.fail_closed(EndpointFailureCode::RejectedSourceReceipt);
                    return Err(EndpointError::InvalidReceipt);
                }
                let runtime =
                    match DocumentRuntime::from_source_store(pending.store, self.config.runtime) {
                        Ok(runtime) => runtime,
                        Err(_) => {
                            self.fail_closed(EndpointFailureCode::InvalidSeed);
                            return Err(EndpointError::InvalidSeed);
                        }
                    };
                self.runtime = Some(runtime);
                self.installed_target = Some(pending.target);
                self.observed = Some(pending.observed);
                self.needs_reseed = false;
                if self.start_source_facts(false).is_err() {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::SourceFacts);
                    return Err(EndpointError::SourceFacts);
                }
            }
            EventTransition::InstallEdit => {
                let pending = self
                    .pending_edit_install
                    .take()
                    .ok_or(EndpointError::InvalidReceipt)?;
                if !accepted_edit_receipt(receipt, pending) {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::RejectedSourceReceipt);
                    return Err(EndpointError::InvalidReceipt);
                }
                self.installed_target = Some(pending.target);
                self.observed = Some(pending.observed);
                self.needs_reseed = false;
                if self.start_source_facts(true).is_err() {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::SourceFacts);
                    return Err(EndpointError::SourceFacts);
                }
            }
            EventTransition::CertificationPage {
                certification_id,
                page_ordinal,
            } => {
                let valid_pending = self.pending_certification.as_ref().is_some_and(|pending| {
                    pending.completion.certification_id == certification_id
                        && u32::try_from(pending.next_page) == Ok(page_ordinal)
                        && pending.next_page < pending.pages.len()
                });
                if !valid_pending {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::RejectedCertificationReceipt);
                    return Err(EndpointError::InvalidReceipt);
                }
                match receipt.disposition {
                    EventDisposition::Accepted
                        if receipt.source.is_none() && receipt.certification.is_none() =>
                    {
                        let pending = self
                            .pending_certification
                            .as_mut()
                            .expect("validated page receipt retains publication");
                        pending.next_page += 1;
                        if self.emit_next_certification_event().is_err() {
                            self.poison_runtime();
                            self.fail_closed(EndpointFailureCode::SourceFacts);
                            return Err(EndpointError::SourceFacts);
                        }
                    }
                    EventDisposition::Stale
                        if receipt.source.is_none() && receipt.certification.is_none() =>
                    {
                        self.cancel_derived()?;
                        self.certification = CertificationStatus::NotStarted;
                    }
                    _ => {
                        self.poison_runtime();
                        self.fail_closed(EndpointFailureCode::RejectedCertificationReceipt);
                        return Err(EndpointError::InvalidReceipt);
                    }
                }
            }
            EventTransition::CertificationCompletion { certification_id } => {
                let Some(pending) = self.pending_certification.as_ref() else {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::RejectedCertificationReceipt);
                    return Err(EndpointError::InvalidReceipt);
                };
                let expected = pending.completion;
                let all_pages_accepted = pending.next_page == pending.pages.len();
                if expected.certification_id != certification_id || !all_pages_accepted {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::RejectedCertificationReceipt);
                    return Err(EndpointError::InvalidReceipt);
                }
                match receipt.disposition {
                    EventDisposition::Accepted
                        if receipt.source.is_none()
                            && receipt.certification == Some(expected.into()) =>
                    {
                        let certified = self
                            .runtime
                            .as_mut()
                            .and_then(DocumentRuntime::take_certified_source)
                            .ok_or(EndpointError::SourceFacts)?;
                        let binding = self.binding.ok_or(EndpointError::InvalidLifecycle)?;
                        if let Err(error) = self.candidate.start(certified, binding, expected) {
                            self.poison_runtime();
                            self.fail_closed(EndpointFailureCode::SourceFacts);
                            let _ = error;
                            return Err(EndpointError::Candidate);
                        }
                        self.active_candidate_certification = Some(expected.into());
                        self.installed_target = Some(SourceStamp::Known {
                            revision: expected.ui_revision,
                            utf16_length: expected.utf16_length,
                            utf8_length: expected.utf8_length,
                            content_hash128: expected.content_hash128,
                        });
                        self.pending_certification = None;
                        self.certification = CertificationStatus::ExternallyEligible;
                    }
                    EventDisposition::Stale
                        if receipt.source.is_none() && receipt.certification.is_none() =>
                    {
                        self.cancel_derived()?;
                        self.certification = CertificationStatus::NotStarted;
                    }
                    _ => {
                        self.poison_runtime();
                        self.fail_closed(EndpointFailureCode::RejectedCertificationReceipt);
                        return Err(EndpointError::InvalidReceipt);
                    }
                }
            }
            EventTransition::DeltaBegin { certification_id } => {
                let valid_pending =
                    self.pending_incremental_publication
                        .as_ref()
                        .is_some_and(|pending| {
                            pending.begin.certification_id == certification_id
                                && pending.next_page == 0
                                && pending.staged_page.is_none()
                        });
                if !valid_pending {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::RejectedCertificationReceipt);
                    return Err(EndpointError::InvalidReceipt);
                }
                match receipt.disposition {
                    EventDisposition::Accepted
                        if receipt.source.is_none() && receipt.certification.is_none() =>
                    {
                        if self.emit_next_incremental_certification_event().is_err() {
                            self.poison_runtime();
                            self.fail_closed(EndpointFailureCode::SourceFacts);
                            return Err(EndpointError::SourceFacts);
                        }
                    }
                    EventDisposition::Stale
                        if receipt.source.is_none() && receipt.certification.is_none() =>
                    {
                        self.cancel_derived()?;
                        self.certification = CertificationStatus::NotStarted;
                    }
                    _ => {
                        self.poison_runtime();
                        self.fail_closed(EndpointFailureCode::RejectedCertificationReceipt);
                        return Err(EndpointError::InvalidReceipt);
                    }
                }
            }
            EventTransition::DeltaPage {
                certification_id,
                replacement_page_ordinal,
            } => {
                let valid_pending =
                    self.pending_incremental_publication
                        .as_ref()
                        .is_some_and(|pending| {
                            pending.begin.certification_id == certification_id
                                && u32::try_from(pending.next_page) == Ok(replacement_page_ordinal)
                                && pending.staged_page.is_some_and(|page| {
                                    page.replacement_page_ordinal == replacement_page_ordinal
                                })
                        });
                if !valid_pending {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::RejectedCertificationReceipt);
                    return Err(EndpointError::InvalidReceipt);
                }
                match receipt.disposition {
                    EventDisposition::Accepted
                        if receipt.source.is_none() && receipt.certification.is_none() =>
                    {
                        let pending = self
                            .pending_incremental_publication
                            .as_mut()
                            .expect("validated delta page retains publication");
                        let page = pending
                            .staged_page
                            .take()
                            .expect("validated delta page remains staged");
                        for checkpoint in page
                            .checkpoints
                            .iter()
                            .copied()
                            .take(page.page_checkpoint_count as usize)
                        {
                            append_portable_checkpoint_hash_wire(
                                &mut pending.replacement_checkpoint_hash128,
                                checkpoint,
                            );
                        }
                        pending.replacement_checkpoints_accepted = pending
                            .replacement_checkpoints_accepted
                            .checked_add(u64::from(page.page_checkpoint_count))
                            .ok_or(EndpointError::SourceFacts)?;
                        if pending.replacement_checkpoints_accepted
                            > u64::from(pending.begin.replacement_checkpoint_count)
                        {
                            self.poison_runtime();
                            self.fail_closed(EndpointFailureCode::SourceFacts);
                            return Err(EndpointError::SourceFacts);
                        }
                        pending.next_page = pending
                            .next_page
                            .checked_add(1)
                            .ok_or(EndpointError::SourceFacts)?;
                        if self.emit_next_incremental_certification_event().is_err() {
                            self.poison_runtime();
                            self.fail_closed(EndpointFailureCode::SourceFacts);
                            return Err(EndpointError::SourceFacts);
                        }
                    }
                    EventDisposition::Stale
                        if receipt.source.is_none() && receipt.certification.is_none() =>
                    {
                        self.cancel_derived()?;
                        self.certification = CertificationStatus::NotStarted;
                    }
                    _ => {
                        self.poison_runtime();
                        self.fail_closed(EndpointFailureCode::RejectedCertificationReceipt);
                        return Err(EndpointError::InvalidReceipt);
                    }
                }
            }
            EventTransition::DeltaCompletion { certification_id } => {
                let Some(pending) = self.pending_incremental_publication.as_ref() else {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::RejectedCertificationReceipt);
                    return Err(EndpointError::InvalidReceipt);
                };
                let Some(terminal) = pending.terminal else {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::RejectedCertificationReceipt);
                    return Err(EndpointError::InvalidReceipt);
                };
                let expected = terminal.completion;
                let replacement_page_count =
                    u64::from(pending.begin.target_page_end - pending.begin.target_page_start);
                let all_pages_accepted = pending.next_page == replacement_page_count
                    && pending.staged_page.is_none()
                    && pending.replacement_checkpoints_accepted
                        == u64::from(pending.begin.replacement_checkpoint_count);
                if expected.certification_id != certification_id
                    || !all_pages_accepted
                    || !self.incremental_completion_matches_target(pending)
                {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::RejectedCertificationReceipt);
                    return Err(EndpointError::InvalidReceipt);
                }
                match receipt.disposition {
                    EventDisposition::Accepted
                        if receipt.source.is_none()
                            && receipt.certification == Some(expected.into()) =>
                    {
                        // The exact external proof and the still-live target
                        // root were rechecked above before this move-only
                        // handoff can be consumed.
                        let pending = self
                            .pending_incremental_publication
                            .take()
                            .expect("validated delta completion retains publication");
                        let binding = self.binding.ok_or(EndpointError::InvalidLifecycle)?;
                        let runtime = self.runtime.as_ref().ok_or(EndpointError::SourceFacts)?;
                        if self
                            .candidate
                            .start_incremental(
                                runtime,
                                pending.candidate.target_source,
                                pending.candidate.source_facts_delta,
                                binding,
                                expected,
                            )
                            .is_err()
                        {
                            self.poison_runtime();
                            self.fail_closed(EndpointFailureCode::SourceFacts);
                            return Err(EndpointError::Candidate);
                        }
                        self.active_candidate_certification = Some(expected.into());
                        self.installed_target = Some(SourceStamp::Known {
                            revision: expected.ui_revision,
                            utf16_length: expected.utf16_length,
                            utf8_length: expected.utf8_length,
                            content_hash128: expected.content_hash128,
                        });
                        self.certification = CertificationStatus::ExternallyEligible;
                    }
                    EventDisposition::Stale
                        if receipt.source.is_none() && receipt.certification.is_none() =>
                    {
                        self.cancel_derived()?;
                        self.certification = CertificationStatus::NotStarted;
                    }
                    _ => {
                        self.poison_runtime();
                        self.fail_closed(EndpointFailureCode::RejectedCertificationReceipt);
                        return Err(EndpointError::InvalidReceipt);
                    }
                }
            }
            EventTransition::Candidate(credit) => {
                if receipt.source.is_some() || receipt.certification.is_some() {
                    self.candidate.cancel()?;
                    self.active_candidate_certification = None;
                    return Err(EndpointError::InvalidReceipt);
                }
                match receipt.disposition {
                    EventDisposition::Accepted => {
                        let runtime = self.runtime.as_ref().ok_or(EndpointError::Candidate)?;
                        self.candidate
                            .accept_credit(runtime, credit, outstanding.event_id)?;
                    }
                    EventDisposition::Stale | EventDisposition::Rejected => {
                        self.candidate.cancel()?;
                        self.active_candidate_certification = None;
                    }
                }
            }
            EventTransition::HotInline(credit) => {
                if receipt.source.is_some() || receipt.certification.is_some() {
                    self.candidate.cancel_hot_inline();
                    return Err(EndpointError::InvalidReceipt);
                }
                match receipt.disposition {
                    EventDisposition::Accepted => {
                        self.candidate
                            .accept_hot_inline_credit(credit, outstanding.event_id)?;
                    }
                    EventDisposition::Stale | EventDisposition::Rejected => {
                        self.candidate.cancel_hot_inline();
                    }
                }
            }
            EventTransition::ViewportPresentation(credit) => {
                if receipt.source.is_some() || receipt.certification.is_some() {
                    self.candidate.cancel_viewport_presentation();
                    self.active_viewport_presentation_generation = None;
                    return Err(EndpointError::InvalidReceipt);
                }
                match receipt.disposition {
                    EventDisposition::Accepted => {
                        self.candidate
                            .accept_viewport_presentation_credit(credit, outstanding.event_id)?;
                        if credit == ViewportPresentationCredit::Delivery {
                            self.active_viewport_presentation_generation = None;
                        }
                    }
                    EventDisposition::Stale | EventDisposition::Rejected => {
                        self.candidate.cancel_viewport_presentation();
                        self.active_viewport_presentation_generation = None;
                    }
                }
            }
            EventTransition::InlineRefinementUnavailable
            | EventTransition::ViewportPresentationUnavailable => {
                if !plain_accepted_receipt(receipt) {
                    return Err(EndpointError::InvalidReceipt);
                }
            }
            EventTransition::Failed => {
                if !plain_accepted_receipt(receipt) {
                    return Err(EndpointError::InvalidReceipt);
                }
                self.lifecycle = EndpointLifecycle::Faulted;
            }
            EventTransition::DrainProgress { complete } => {
                if !plain_accepted_receipt(receipt) {
                    return Err(EndpointError::InvalidReceipt);
                }
                if complete {
                    self.lifecycle = EndpointLifecycle::Closed;
                    self.emit(
                        EventBody::Closed,
                        EndpointEventKind::Closed,
                        EventTransition::Closed,
                        None,
                    )?;
                }
            }
            EventTransition::Closed => {
                if !plain_accepted_receipt(receipt) {
                    self.lifecycle = EndpointLifecycle::Closing;
                    return Err(EndpointError::InvalidReceipt);
                }
                self.lifecycle = EndpointLifecycle::Removable;
            }
        }
        Ok(EndpointCommandAction::EventReceiptAccepted)
    }

    fn begin_close(&mut self) -> Result<(), EndpointError> {
        if self.binding.is_none() {
            return Err(EndpointError::InvalidLifecycle);
        }
        if matches!(
            self.lifecycle,
            EndpointLifecycle::Closed | EndpointLifecycle::Removable
        ) {
            return Ok(());
        }
        self.close_latched = true;
        self.lifecycle = EndpointLifecycle::Closing;
        self.pending_seed = None;
        self.pending_install = None;
        self.pending_edit_install = None;
        self.pending_certification = None;
        self.pending_incremental_publication = None;
        self.active_source_facts = None;
        self.candidate.begin_close()?;
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.cancel_source_facts();
        }
        self.certification = CertificationStatus::NotStarted;
        self.installed_target = None;
        self.observed = None;
        self.active_candidate_certification = None;
        self.last_external_certification = None;
        self.active_viewport_presentation_generation = None;
        if let Some(runtime) = self.runtime.as_mut() {
            runtime
                .begin_close()
                .map_err(|_| EndpointError::InvalidLifecycle)?;
        }
        Ok(())
    }

    fn handle_drain(&mut self, grant: DrainGrant) -> Result<EndpointCommandAction, EndpointError> {
        if self.lifecycle != EndpointLifecycle::Closing || self.outstanding.is_some() {
            return Err(EndpointError::InvalidLifecycle);
        }
        let poll = if self.candidate.cleanup_pending() {
            let transition_budget = grant.maximum_transitions as usize;
            let runtime = self
                .runtime
                .as_mut()
                .ok_or(EndpointError::InvalidLifecycle)?;
            let complete = self.candidate.poll_cleanup(runtime, transition_budget)?;
            flark_engine::DrainPoll {
                released_source_leases: 0,
                released_source_bytes: 0,
                arena_transitions: transition_budget,
                arena_nodes_reclaimed: 0,
                complete: complete && self.runtime.is_none(),
            }
        } else if let Some(runtime) = self.runtime.as_mut() {
            // The v1 DrainProgress byte counter is u32. Every admitted source
            // root is individually u32-bounded, so release at most one source
            // transition per event; once no source lease remains, the rest of
            // the exact grant can drain arena work without byte aggregation.
            let transition_budget = if runtime.retired_source_count() > 0 {
                1
            } else {
                grant.maximum_transitions as usize
            };
            runtime
                .poll_close(transition_budget)
                .map_err(|_| EndpointError::InvalidLifecycle)?
        } else {
            flark_engine::DrainPoll {
                released_source_leases: 0,
                released_source_bytes: 0,
                arena_transitions: 0,
                arena_nodes_reclaimed: 0,
                complete: true,
            }
        };
        if poll.complete {
            self.runtime = None;
            self.runtime_poisoned = false;
        }
        let progress = DrainProgress {
            drain_id: grant.drain_id,
            released_source_leases: bounded_u32(poll.released_source_leases)?,
            released_source_bytes: bounded_u32(poll.released_source_bytes)?,
            arena_transitions: bounded_u32(poll.arena_transitions)?,
            arena_nodes_reclaimed: bounded_u32(poll.arena_nodes_reclaimed)?,
            complete: poll.complete,
        };
        self.emit(
            EventBody::DrainProgress(progress),
            EndpointEventKind::DrainProgress,
            EventTransition::DrainProgress {
                complete: poll.complete,
            },
            Some(grant),
        )?;
        Ok(EndpointCommandAction::DrainAdvanced)
    }

    /// Advances source facts only under explicit caller fuel. A faulted
    /// partially-mutated runtime uses only the separately declared retirement
    /// transition budget.
    pub fn poll_source_facts(
        &mut self,
        fuel: EndpointPollFuel,
    ) -> Result<EndpointPollReceipt, EndpointError> {
        if fuel.maximum_retirement_transitions > MAXIMUM_ENDPOINT_RETIREMENT_TRANSITIONS {
            return Err(EndpointError::InvalidPollFuel);
        }
        let mut receipt = EndpointPollReceipt::default();
        if self.runtime_poisoned {
            if fuel.maximum_retirement_transitions == 0 {
                return Err(EndpointError::InvalidPollFuel);
            }
            if self.candidate.cleanup_pending() {
                let complete = self
                    .candidate
                    .poll_cleanup(
                        self.runtime.as_mut().ok_or(EndpointError::SourceFacts)?,
                        fuel.maximum_retirement_transitions,
                    )
                    .map_err(|_| EndpointError::Candidate)?;
                receipt.arena_transitions = fuel.maximum_retirement_transitions;
                receipt.cleanup_complete = false;
                if !complete {
                    return Ok(receipt);
                }
                return Ok(receipt);
            }
            let poll = self
                .runtime
                .as_mut()
                .ok_or(EndpointError::SourceFacts)?
                .poll_close(fuel.maximum_retirement_transitions)
                .map_err(|_| EndpointError::SourceFacts)?;
            receipt.released_source_leases = poll.released_source_leases;
            receipt.released_source_bytes = poll.released_source_bytes;
            receipt.arena_transitions = poll.arena_transitions;
            receipt.arena_nodes_reclaimed = poll.arena_nodes_reclaimed;
            receipt.cleanup_complete = poll.complete;
            if poll.complete {
                self.runtime = None;
                self.runtime_poisoned = false;
            }
            return Ok(receipt);
        }

        if self.lifecycle != EndpointLifecycle::Open {
            return Ok(receipt);
        }
        if self.certification != CertificationStatus::Scanning {
            return Ok(receipt);
        }
        // A superseding edit can leave the recursive-Green base temporarily
        // owned by candidate cleanup. Let that bounded actor restore or
        // release its authority before source facts make the new revision
        // eligible to start another candidate transaction.
        if self.candidate.cleanup_pending() {
            if fuel.maximum_retirement_transitions == 0 {
                return Err(EndpointError::InvalidPollFuel);
            }
            let _complete = self
                .candidate
                .poll_cleanup(
                    self.runtime.as_mut().ok_or(EndpointError::SourceFacts)?,
                    fuel.maximum_retirement_transitions,
                )
                .map_err(|_| EndpointError::Candidate)?;
            receipt.arena_transitions = fuel.maximum_retirement_transitions;
            // This quantum deliberately did no source-fact work. Keep the
            // source lane scheduled even when cleanup completed so the next
            // turn can resume the still-active scan.
            receipt.cleanup_complete = false;
            return Ok(receipt);
        }
        if fuel.maximum_source_bytes == 0
            || fuel.maximum_source_bytes > SOURCE_FACT_SOURCE_BYTES_PER_POLL_MAX
            || fuel.maximum_checkpoints == 0
            || fuel.maximum_checkpoints > SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX
        {
            return Err(EndpointError::InvalidPollFuel);
        }
        let polled = match self
            .runtime
            .as_mut()
            .expect("an active source-fact job requires its runtime")
            .poll_source_facts(fuel.maximum_source_bytes, fuel.maximum_checkpoints)
        {
            Ok(polled) => polled,
            Err(_) => {
                self.poison_runtime();
                self.fail_closed(EndpointFailureCode::SourceFacts);
                return Err(EndpointError::SourceFacts);
            }
        };
        match polled {
            RuntimeSourceFactsPoll::Pending(work) => receipt.add_source_work(work),
            RuntimeSourceFactsPoll::PromotionPending { transitions } => {
                receipt.source_fact_transitions = transitions;
            }
            RuntimeSourceFactsPoll::IncrementalScanComplete {
                source,
                byte_start,
                byte_end,
                work,
            } => {
                receipt.add_source_work(work);
                receipt.source_fact_transitions = 1;
                let valid = matches!(
                    self.active_source_facts.as_ref(),
                    Some(ActiveSourceFacts::Incremental(plan))
                        if plan.source() == source
                            && plan.target_byte_range().start == byte_start
                            && plan.target_byte_range().end == byte_end
                );
                if !valid || !self.source_version_matches_target(source) {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::SourceFacts);
                    return Err(EndpointError::SourceFacts);
                }
            }
            RuntimeSourceFactsPoll::IncrementalComplete {
                source,
                work: _,
                witness,
            } => {
                receipt.scan_complete = true;
                let valid_plan = matches!(
                    self.active_source_facts.as_ref(),
                    Some(ActiveSourceFacts::Incremental(plan))
                        if plan.source() == source
                            && plan.base() == witness.base()
                            && plan.source() == witness.target()
                            && plan.base_page_range() == witness.base_page_range()
                            && plan.base_byte_range() == witness.base_byte_range()
                            && plan.target_byte_range() == witness.target_byte_range()
                            && plan.lineage_transitions() == witness.lineage_transitions()
                );
                if !valid_plan || !self.source_version_matches_target(source) {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::SourceFacts);
                    return Err(EndpointError::SourceFacts);
                }
                let handoff = {
                    let runtime = self.runtime.as_ref().ok_or(EndpointError::SourceFacts)?;
                    let target_facts = runtime
                        .persistent_source_facts()
                        .ok_or(EndpointError::SourceFacts)?;
                    if target_facts.source() != source
                        || target_facts.profile() != self.config.source_facts
                        || target_facts.parser_profile() != self.config.parser_profile
                        || !persistent_source_facts_match_target(
                            target_facts,
                            self.installed_target,
                        )
                    {
                        self.poison_runtime();
                        self.fail_closed(EndpointFailureCode::SourceFacts);
                        return Err(EndpointError::SourceFacts);
                    }
                    let target_source = runtime
                        .snapshot_current_source()
                        .map_err(|_| EndpointError::SourceFacts)?;
                    PendingIncrementalCandidate {
                        target_source,
                        target_facts,
                        source_facts_delta: witness,
                    }
                };
                self.active_source_facts = None;
                if self
                    .prepare_incremental_certification_publication(handoff)
                    .is_err()
                {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::SourceFacts);
                    return Err(EndpointError::SourceFacts);
                }
                receipt.certification = Some(self.certification);
            }
            RuntimeSourceFactsPoll::ScanComplete { completion, work } => {
                receipt.add_source_work(work);
                receipt.source_fact_transitions = 1;
                if !matches!(
                    self.active_source_facts.as_ref(),
                    Some(ActiveSourceFacts::Clean)
                ) || !self.source_facts_completion_matches_target(completion)
                {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::SourceHashMismatch);
                    return Err(EndpointError::SourceFacts);
                }
            }
            RuntimeSourceFactsPoll::Complete { completion, work } => {
                receipt.add_source_work(work);
                receipt.scan_complete = true;
                if !matches!(
                    self.active_source_facts.as_ref(),
                    Some(ActiveSourceFacts::Clean)
                ) || !self.source_facts_completion_matches_target(completion)
                {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::SourceHashMismatch);
                    return Err(EndpointError::SourceFacts);
                }
                if self
                    .runtime
                    .as_ref()
                    .and_then(DocumentRuntime::certified_source)
                    .is_none()
                {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::SourceFacts);
                    return Err(EndpointError::SourceFacts);
                }
                if self.prepare_certification_publication(completion).is_err() {
                    self.poison_runtime();
                    self.fail_closed(EndpointFailureCode::SourceFacts);
                    return Err(EndpointError::SourceFacts);
                }
                self.active_source_facts = None;
                receipt.certification = Some(self.certification);
            }
        }
        Ok(receipt)
    }

    fn source_facts_completion_matches_target(&self, completion: SourceFactsCompletion) -> bool {
        match self.installed_target {
            Some(SourceStamp::Known {
                utf8_length,
                content_hash128,
                ..
            }) => {
                completion.fingerprint().byte_len() == u64::from(utf8_length)
                    && completion.fingerprint().rolling_hash().words() == content_hash128
            }
            Some(SourceStamp::Provisional { .. }) => true,
            None => false,
        }
    }

    fn source_version_matches_target(&self, source: SourceVersion) -> bool {
        let observed_matches = self.observed.is_some_and(|observed| {
            source.revision().get() == u64::from(observed.revision)
                && source.utf16_len() == observed.utf16_length as usize
                && source.byte_len() == observed.utf8_length as usize
        });
        if !observed_matches {
            return false;
        }
        match self.installed_target {
            Some(SourceStamp::Known {
                revision,
                utf16_length,
                utf8_length,
                ..
            }) => {
                source.revision().get() == u64::from(revision)
                    && source.utf16_len() == utf16_length as usize
                    && source.byte_len() == utf8_length as usize
            }
            Some(SourceStamp::Provisional {
                revision,
                utf16_length,
            }) => {
                source.revision().get() == u64::from(revision)
                    && source.utf16_len() == utf16_length as usize
            }
            None => false,
        }
    }

    /// Advances exact parsing, candidate sealing, snapshot emission, or
    /// producer reclamation under one explicit transition grant.
    pub fn poll_candidate(
        &mut self,
        maximum_transitions: usize,
    ) -> Result<EndpointCandidatePollReceipt, EndpointError> {
        if maximum_transitions == 0 || maximum_transitions > MAXIMUM_ENDPOINT_RETIREMENT_TRANSITIONS
        {
            return Err(EndpointError::InvalidPollFuel);
        }
        if self.lifecycle != EndpointLifecycle::Open
            || self.outstanding.is_some()
            || !self.candidate.has_poll_work()
        {
            return Ok(EndpointCandidatePollReceipt {
                transitions: 0,
                cleanup_complete: !self.candidate.cleanup_pending(),
                outstanding_event: self.outstanding_status(),
            });
        }
        let candidate_poll = match self.candidate.poll(
            self.runtime
                .as_mut()
                .ok_or(EndpointError::InvalidLifecycle)?,
            maximum_transitions,
        ) {
            Ok(poll) => poll,
            Err(error) => return Err(error.into()),
        };
        let transitions = match candidate_poll {
            CandidatePoll::Pending { transitions } => transitions,
            CandidatePoll::Event { transitions, event } => {
                self.emit_candidate(*event)?;
                transitions
            }
            CandidatePoll::HotInlineEvent { transitions, event } => {
                self.emit_hot_inline(*event)?;
                transitions
            }
            CandidatePoll::ViewportPresentationEvent { transitions, event } => {
                self.emit_viewport_presentation(*event)?;
                transitions
            }
            CandidatePoll::ViewportPresentationUnavailable {
                transitions,
                viewport_generation,
                reason,
            } => {
                let reason_code = match reason {
                    ViewportPresentationUnavailableReason::BudgetExceeded => {
                        VIEWPORT_PRESENTATION_UNAVAILABLE_BUDGET_EXCEEDED
                    }
                    ViewportPresentationUnavailableReason::DerivationFailed => {
                        VIEWPORT_PRESENTATION_UNAVAILABLE_DERIVATION
                    }
                };
                if self.active_viewport_presentation_generation != Some(viewport_generation) {
                    return Err(EndpointError::Candidate);
                }
                self.active_viewport_presentation_generation = None;
                self.emit(
                    EventBody::ViewportPresentationUnavailable(
                        ViewportPresentationUnavailableEvent {
                            viewport_generation,
                            reason_code,
                        },
                    ),
                    EndpointEventKind::ViewportPresentationUnavailable,
                    EventTransition::ViewportPresentationUnavailable,
                    None,
                )?;
                transitions
            }
        };
        Ok(EndpointCandidatePollReceipt {
            transitions,
            cleanup_complete: !self.candidate.cleanup_pending(),
            outstanding_event: self.outstanding_status(),
        })
    }

    /// Applies one terminal host-poll response. Event credit remains owned by
    /// the schema-3 session receipt path; this decoder accepts no receipts.
    pub fn dispatch_host_poll(
        &mut self,
        bytes: &[u8],
    ) -> Result<EndpointHostPollReceipt, EndpointError> {
        self.ensure_open()?;
        if let Some(outstanding) = &self.outstanding {
            return Err(EndpointError::EventCreditOccupied {
                event_id: outstanding.event_id,
            });
        }
        let binding = self.binding.ok_or(EndpointError::InvalidLifecycle)?;
        let decoded = decode_host_poll_command(bytes, binding)?;
        let rejected = matches!(decoded.result, HostPollResult::Rejected(_));
        let event = self.candidate.handle_host_poll(
            decoded.ticket.poll_ticket,
            decoded.ticket.offer_id,
            decoded.ticket.phase,
            decoded.result,
        )?;
        if let Some(CandidateEventBody::DeliveryAcknowledged(ack)) =
            event.as_ref().map(|event| &event.body)
        {
            let certification = self
                .active_candidate_certification
                .take()
                .ok_or(EndpointError::SourceFacts)?;
            if !certification_binds_structural_ack(certification, *ack) {
                self.poison_runtime();
                self.fail_closed(EndpointFailureCode::SourceFacts);
                return Err(EndpointError::SourceFacts);
            }
            let target = self
                .runtime
                .as_ref()
                .and_then(DocumentRuntime::current_source_version)
                .ok_or(EndpointError::SourceFacts)?;
            if self
                .runtime
                .as_mut()
                .ok_or(EndpointError::SourceFacts)?
                .commit_persistent_source_facts_delta(target)
                .is_err()
            {
                self.poison_runtime();
                self.fail_closed(EndpointFailureCode::SourceFacts);
                return Err(EndpointError::SourceFacts);
            }
            self.last_external_certification = Some(certification);
        } else if rejected {
            self.active_candidate_certification = None;
        }
        let action = if let Some(event) = event {
            self.emit_candidate(event)?;
            EndpointHostPollAction::DeliveryEmitted
        } else if rejected {
            EndpointHostPollAction::CandidateCancelled
        } else {
            EndpointHostPollAction::CreditAccepted
        };
        Ok(EndpointHostPollReceipt {
            correlation_id: decoded.correlation_id,
            action,
            outstanding_event: self.outstanding_status(),
        })
    }

    /// Applies one terminal hot-inline host-poll response. The sidecar shares
    /// the endpoint's single event credit but has distinct ticket phases and
    /// cannot advance structural publication.
    pub fn dispatch_inline_sidecar_host_poll(
        &mut self,
        bytes: &[u8],
    ) -> Result<EndpointHostPollReceipt, EndpointError> {
        self.ensure_open()?;
        if let Some(outstanding) = &self.outstanding {
            return Err(EndpointError::EventCreditOccupied {
                event_id: outstanding.event_id,
            });
        }
        let binding = self.binding.ok_or(EndpointError::InvalidLifecycle)?;
        let decoded = decode_inline_sidecar_host_poll_command(bytes, binding)?;
        let rejected = matches!(decoded.result, InlineSidecarHostPollResult::Rejected(_));
        let event = self.candidate.handle_hot_inline_host_poll(
            decoded.ticket.poll_ticket,
            decoded.ticket.offer_id,
            decoded.ticket.phase,
            decoded.result,
        )?;
        let action = if let Some(event) = event {
            self.emit_hot_inline(event)?;
            EndpointHostPollAction::DeliveryEmitted
        } else if rejected {
            EndpointHostPollAction::CandidateCancelled
        } else {
            EndpointHostPollAction::CreditAccepted
        };
        Ok(EndpointHostPollReceipt {
            correlation_id: decoded.correlation_id,
            action,
            outstanding_event: self.outstanding_status(),
        })
    }

    /// Applies one terminal viewport-presentation host-poll response. The
    /// aggregate shares the endpoint's single event credit, but its tickets
    /// and ACK cannot be replayed into either structural or point-sidecar
    /// publication.
    pub fn dispatch_viewport_presentation_host_poll(
        &mut self,
        bytes: &[u8],
    ) -> Result<EndpointHostPollReceipt, EndpointError> {
        self.ensure_open()?;
        if let Some(outstanding) = &self.outstanding {
            return Err(EndpointError::EventCreditOccupied {
                event_id: outstanding.event_id,
            });
        }
        let binding = self.binding.ok_or(EndpointError::InvalidLifecycle)?;
        let decoded = decode_viewport_presentation_host_poll_command(bytes, binding)?;
        let rejected = matches!(
            decoded.result,
            ViewportPresentationHostPollResult::Rejected(_)
        );
        let viewport_generation = self.active_viewport_presentation_generation;
        let event = self.candidate.handle_viewport_presentation_host_poll(
            decoded.ticket.poll_ticket,
            decoded.ticket.offer_id,
            decoded.ticket.phase,
            decoded.result,
        )?;
        let action = if let Some(event) = event {
            self.emit_viewport_presentation(event)?;
            EndpointHostPollAction::DeliveryEmitted
        } else if rejected {
            let viewport_generation = viewport_generation.ok_or(EndpointError::Candidate)?;
            self.active_viewport_presentation_generation = None;
            self.emit(
                EventBody::ViewportPresentationUnavailable(ViewportPresentationUnavailableEvent {
                    viewport_generation,
                    reason_code: VIEWPORT_PRESENTATION_UNAVAILABLE_HOST_REJECTED,
                }),
                EndpointEventKind::ViewportPresentationUnavailable,
                EventTransition::ViewportPresentationUnavailable,
                None,
            )?;
            EndpointHostPollAction::CandidateCancelled
        } else {
            EndpointHostPollAction::CreditAccepted
        };
        Ok(EndpointHostPollReceipt {
            correlation_id: decoded.correlation_id,
            action,
            outstanding_event: self.outstanding_status(),
        })
    }

    fn start_source_facts(&mut self, after_edit: bool) -> Result<(), EndpointError> {
        let no_unpublished_delta = self.pending_incremental_publication.is_none();
        if after_edit {
            self.cancel_derived_after_edit()?;
        } else {
            self.cancel_derived()?;
        }
        let has_exact_base = if after_edit && no_unpublished_delta {
            let runtime = self.runtime.as_ref().ok_or(EndpointError::SourceFacts)?;
            match runtime.persistent_source_facts() {
                Some(base) => self.candidate.has_exact_base_for(runtime, base.source())?,
                None => false,
            }
        } else {
            false
        };
        let runtime = self.runtime.as_mut().ok_or(EndpointError::SourceFacts)?;
        let active = if has_exact_base {
            match runtime.begin_incremental_source_facts(
                self.config.source_facts,
                self.config.parser_profile,
                self.config.source_facts_root_limits,
            ) {
                Ok(plan) => {
                    if self
                        .candidate
                        .has_incremental_base_for_plan(runtime, &plan)?
                    {
                        ActiveSourceFacts::Incremental(plan)
                    } else {
                        // Planning is borrow-only. If the exact parser cannot
                        // surround this SourceFacts crop with authenticated
                        // restart/convergence authority, restore the retained
                        // facts base and enter the definitive clean lane.
                        if !runtime.cancel_source_facts() {
                            return Err(EndpointError::SourceFacts);
                        }
                        self.last_external_certification = None;
                        runtime
                            .begin_source_facts(
                                self.config.source_facts,
                                self.config.parser_profile,
                                self.config.source_facts_root_limits,
                            )
                            .map_err(|_| EndpointError::SourceFacts)?;
                        ActiveSourceFacts::Clean
                    }
                }
                Err(
                    DocumentRuntimeError::NoPersistentSourceFactsBase
                    | DocumentRuntimeError::IncrementalSourceFactsProfileMismatch
                    | DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable,
                ) => {
                    // These are explicit eligibility outcomes, not corruption.
                    // The clean lane remains definitive and replaces the stale
                    // external base proof when its publication is accepted.
                    self.last_external_certification = None;
                    runtime
                        .begin_source_facts(
                            self.config.source_facts,
                            self.config.parser_profile,
                            self.config.source_facts_root_limits,
                        )
                        .map_err(|_| EndpointError::SourceFacts)?;
                    ActiveSourceFacts::Clean
                }
                Err(_) => return Err(EndpointError::SourceFacts),
            }
        } else {
            // Clean scanning is reserved for initial installation and the
            // explicit no-combined-base case (for example an edit racing the
            // first acknowledged candidate).
            runtime
                .begin_source_facts(
                    self.config.source_facts,
                    self.config.parser_profile,
                    self.config.source_facts_root_limits,
                )
                .map_err(|_| EndpointError::SourceFacts)?;
            if after_edit {
                self.last_external_certification = None;
            }
            ActiveSourceFacts::Clean
        };
        if matches!(active, ActiveSourceFacts::Clean) {
            self.candidate.discard_bullet_list_local_edit_plan();
        }
        self.active_source_facts = Some(active);
        self.certification = CertificationStatus::Scanning;
        Ok(())
    }

    fn prepare_certification_publication(
        &mut self,
        scan_completion: SourceFactsCompletion,
    ) -> Result<(), EndpointError> {
        let observed = self.observed.ok_or(EndpointError::SourceFacts)?;
        let target = self.installed_target.ok_or(EndpointError::SourceFacts)?;
        if target.revision() != observed.revision
            || target.utf16_length() != observed.utf16_length
            || scan_completion.source().revision().get() != u64::from(observed.revision)
            || scan_completion.source().utf16_len() != observed.utf16_length as usize
            || scan_completion.source().byte_len() != observed.utf8_length as usize
        {
            return Err(EndpointError::SourceFacts);
        }

        let (fingerprint, logical_line_breaks, checkpoint_spacing_utf16, checkpoint_count, pages) = {
            let facts = self
                .runtime
                .as_ref()
                .and_then(DocumentRuntime::certified_source)
                .ok_or(EndpointError::SourceFacts)?
                .facts();
            if facts.source() != scan_completion.source()
                || facts.fingerprint() != scan_completion.fingerprint()
                || facts.logical_line_breaks() != scan_completion.logical_line_breaks()
                || facts.profile().checkpoint_spacing_utf16()
                    != scan_completion.checkpoint_spacing_utf16()
                || facts.checkpoint_count() != scan_completion.checkpoint_count()
                || facts.pages().len()
                    != usize::try_from(facts.page_count())
                        .map_err(|_| EndpointError::SourceFacts)?
            {
                return Err(EndpointError::SourceFacts);
            }
            let page_count =
                usize::try_from(facts.page_count()).map_err(|_| EndpointError::SourceFacts)?;
            let mut pages = Vec::new();
            pages
                .try_reserve_exact(page_count)
                .map_err(|_| EndpointError::SourceFacts)?;
            pages.extend(facts.pages().cloned());
            (
                facts.fingerprint(),
                facts.logical_line_breaks(),
                facts.profile().checkpoint_spacing_utf16(),
                facts.checkpoint_count(),
                pages,
            )
        };

        let certification_id = self.take_certification_id()?;
        let checkpoint_hash128 = portable_checkpoint_hash(&pages)?;
        let completion = SourceFactsCompletionEvent {
            certification_id,
            worker_replica_revision: observed.revision,
            ui_revision: target.revision(),
            utf16_length: observed.utf16_length,
            intent_high_water: observed.intent_high_water,
            fingerprint_algorithm: fingerprint.algorithm(),
            utf8_length: u32::try_from(fingerprint.byte_len())
                .map_err(|_| EndpointError::SourceFacts)?,
            logical_line_breaks: u32::try_from(logical_line_breaks)
                .map_err(|_| EndpointError::SourceFacts)?,
            checkpoint_spacing_utf16: u32::try_from(checkpoint_spacing_utf16)
                .map_err(|_| EndpointError::SourceFacts)?,
            checkpoint_count: u32::try_from(checkpoint_count)
                .map_err(|_| EndpointError::SourceFacts)?,
            page_count: u32::try_from(pages.len()).map_err(|_| EndpointError::SourceFacts)?,
            content_hash128: fingerprint.rolling_hash().words(),
            checkpoint_hash128,
        };
        self.pending_certification = Some(PendingCertificationPublication {
            completion,
            pages,
            next_page: 0,
        });
        self.certification = CertificationStatus::Publishing;
        self.emit_next_certification_event()
    }

    fn take_certification_id(&mut self) -> Result<u32, EndpointError> {
        if self.next_certification_id == 0 {
            return Err(EndpointError::EventIdentityExhausted);
        }
        let certification_id = self.next_certification_id;
        self.next_certification_id = self.next_certification_id.checked_add(1).unwrap_or(0);
        Ok(certification_id)
    }

    fn emit_next_certification_event(&mut self) -> Result<(), EndpointError> {
        let pending = self
            .pending_certification
            .as_ref()
            .ok_or(EndpointError::SourceFacts)?;
        if let Some(page) = pending.pages.get(pending.next_page) {
            let completion = pending.completion;
            let page_ordinal =
                u32::try_from(pending.next_page).map_err(|_| EndpointError::SourceFacts)?;
            let page_checkpoint_count =
                u32::try_from(page.checkpoints().len()).map_err(|_| EndpointError::SourceFacts)?;
            let mut checkpoints =
                [SourceFactCheckpointWire::default(); SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX];
            for (output, checkpoint) in checkpoints.iter_mut().zip(page.checkpoints()) {
                *output = SourceFactCheckpointWire {
                    byte_offset: u32::try_from(checkpoint.byte_offset())
                        .map_err(|_| EndpointError::SourceFacts)?,
                    utf16_offset: u32::try_from(checkpoint.utf16_offset())
                        .map_err(|_| EndpointError::SourceFacts)?,
                    logical_line_breaks: u32::try_from(checkpoint.logical_line_breaks())
                        .map_err(|_| EndpointError::SourceFacts)?,
                    rolling_hash128: checkpoint.rolling_hash().words(),
                };
            }
            let event = SourceFactsPageEvent {
                certification_id: completion.certification_id,
                worker_replica_revision: completion.worker_replica_revision,
                ui_revision: completion.ui_revision,
                utf16_length: completion.utf16_length,
                intent_high_water: completion.intent_high_water,
                checkpoint_spacing_utf16: completion.checkpoint_spacing_utf16,
                page_ordinal,
                page_count: completion.page_count,
                checkpoint_count: completion.checkpoint_count,
                page_checkpoint_count,
                checkpoints,
            };
            self.certification = CertificationStatus::Publishing;
            self.emit(
                EventBody::SourceFactsPage(event),
                EndpointEventKind::SourceFactsPage,
                EventTransition::CertificationPage {
                    certification_id: completion.certification_id,
                    page_ordinal,
                },
                None,
            )
        } else {
            let completion = pending.completion;
            self.certification = CertificationStatus::AwaitingPromotion;
            self.emit(
                EventBody::SourceFactsCompleted(completion),
                EndpointEventKind::SourceFactsCompleted,
                EventTransition::CertificationCompletion {
                    certification_id: completion.certification_id,
                },
                None,
            )
        }
    }

    fn prepare_incremental_certification_publication(
        &mut self,
        candidate: PendingIncrementalCandidate,
    ) -> Result<(), EndpointError> {
        if PERSISTENT_SOURCE_FACTS_CHECKPOINT_ROOT_GUARD_ALGORITHM
            != PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM
        {
            return Err(EndpointError::SourceFacts);
        }
        let observed = self.observed.ok_or(EndpointError::SourceFacts)?;
        let target = self.installed_target.ok_or(EndpointError::SourceFacts)?;
        let base = self
            .last_external_certification
            .ok_or(EndpointError::SourceFacts)?;
        let witness = &candidate.source_facts_delta;
        let target_facts = candidate.target_facts;
        let target_source = target_facts.source();
        let summary = target_facts.summary();
        if witness.base().revision().get() != u64::from(base.ui_revision)
            || witness.base().utf16_len() != base.utf16_length as usize
            || witness.base().byte_len() != base.utf8_length as usize
            || witness.base_page_count() != u64::from(base.page_count)
            || witness.target() != target_source
            || target_source != candidate.target_source.version()
            || target_source.revision().get() != u64::from(observed.revision)
            || target_source.utf16_len() != observed.utf16_length as usize
            || target_source.byte_len() != observed.utf8_length as usize
            || target.revision() != observed.revision
            || target.utf16_length() != observed.utf16_length
            || target_facts.profile() != self.config.source_facts
            || target_facts.parser_profile() != self.config.parser_profile
            || summary.byte_len() != u64::from(observed.utf8_length)
            || summary.utf16_len() != u64::from(observed.utf16_length)
        {
            return Err(EndpointError::SourceFacts);
        }

        let base_page_start = witness.base_page_range().start;
        let base_page_end = witness.base_page_range().end;
        let target_page_start = witness.target_page_range().start;
        let target_page_end = witness.target_page_range().end;
        let target_page_count = target_facts.page_count();
        let target_checkpoint_count = target_facts.checkpoint_count();
        if base_page_start > base_page_end
            || base_page_end > u64::from(base.page_count)
            || target_page_start != base_page_start
            || target_page_start > target_page_end
            || target_page_end > target_page_count
        {
            return Err(EndpointError::SourceFacts);
        }

        let replacement_page_count = target_page_end - target_page_start;
        // Persistent splice pages are deliberately not repacked globally.
        // Their count therefore cannot be inferred from page ordinals times
        // the per-page maximum after the first large middle edit. The move-only
        // engine witness carries the exact removed/replacement checkpoint
        // counts authenticated by the atomic splice.
        let base_removed_checkpoint_count = witness.base_replacement_checkpoint_count();
        let base_removed_page_count = base_page_end - base_page_start;
        let retained_base_checkpoints = u64::from(base.checkpoint_count)
            .checked_sub(base_removed_checkpoint_count)
            .ok_or(EndpointError::SourceFacts)?;
        let replacement_checkpoint_count = witness.target_replacement_checkpoint_count();
        if base_removed_checkpoint_count > u64::from(base.checkpoint_count)
            || base_removed_checkpoint_count
                > base_removed_page_count
                    .checked_mul(SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX as u64)
                    .ok_or(EndpointError::SourceFacts)?
            || base_removed_page_count > base_removed_checkpoint_count
            || (base_removed_page_count == 0) != (base_removed_checkpoint_count == 0)
            || retained_base_checkpoints.checked_add(replacement_checkpoint_count)
                != Some(target_checkpoint_count)
            || replacement_checkpoint_count
                > replacement_page_count
                    .checked_mul(SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX as u64)
                    .ok_or(EndpointError::SourceFacts)?
            || replacement_page_count > replacement_checkpoint_count
            || (replacement_page_count == 0) != (replacement_checkpoint_count == 0)
        {
            return Err(EndpointError::SourceFacts);
        }

        let certification_id = self.take_certification_id()?;
        let target_guard128 = target_facts.checkpoint_root_guard128();
        let completion = SourceFactsCompletionEvent {
            certification_id,
            worker_replica_revision: observed.revision,
            ui_revision: target.revision(),
            utf16_length: observed.utf16_length,
            intent_high_water: observed.intent_high_water,
            fingerprint_algorithm: 1,
            utf8_length: observed.utf8_length,
            logical_line_breaks: u32::try_from(summary.logical_line_breaks())
                .map_err(|_| EndpointError::SourceFacts)?,
            checkpoint_spacing_utf16: u32::try_from(
                target_facts.profile().checkpoint_spacing_utf16(),
            )
            .map_err(|_| EndpointError::SourceFacts)?,
            checkpoint_count: u32::try_from(target_checkpoint_count)
                .map_err(|_| EndpointError::SourceFacts)?,
            page_count: u32::try_from(target_page_count).map_err(|_| EndpointError::SourceFacts)?,
            content_hash128: summary.rolling_hash().words(),
            checkpoint_hash128: target_guard128,
        };
        let begin = SourceFactsDeltaBeginEvent {
            certification_id,
            worker_replica_revision: completion.worker_replica_revision,
            ui_revision: completion.ui_revision,
            utf16_length: completion.utf16_length,
            intent_high_water: completion.intent_high_water,
            base_ui_revision: base.ui_revision,
            base_utf16_length: base.utf16_length,
            base_utf8_length: base.utf8_length,
            base_content_hash128: base.content_hash128,
            base_checkpoint_hash128: base.checkpoint_hash128,
            base_checkpoint_count: base.checkpoint_count,
            base_page_count: base.page_count,
            base_checkpoint_spacing_utf16: base.checkpoint_spacing_utf16,
            base_page_start: u32::try_from(base_page_start)
                .map_err(|_| EndpointError::SourceFacts)?,
            base_page_end: u32::try_from(base_page_end).map_err(|_| EndpointError::SourceFacts)?,
            target_page_start: u32::try_from(target_page_start)
                .map_err(|_| EndpointError::SourceFacts)?,
            target_page_end: u32::try_from(target_page_end)
                .map_err(|_| EndpointError::SourceFacts)?,
            target_checkpoint_count: completion.checkpoint_count,
            target_page_count: completion.page_count,
            target_checkpoint_root_guard_algorithm: PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM,
            target_checkpoint_root_guard128: target_guard128,
            replacement_checkpoint_count: u32::try_from(replacement_checkpoint_count)
                .map_err(|_| EndpointError::SourceFacts)?,
        };
        self.pending_incremental_publication = Some(PendingIncrementalPublication {
            candidate,
            begin,
            completion,
            next_page: 0,
            replacement_checkpoints_accepted: 0,
            replacement_checkpoint_hash128: [0; 4],
            staged_page: None,
            terminal: None,
        });
        self.certification = CertificationStatus::Publishing;
        self.emit(
            EventBody::SourceFactsDeltaBegin(begin),
            EndpointEventKind::SourceFactsDeltaBegin,
            EventTransition::DeltaBegin { certification_id },
            None,
        )
    }

    fn emit_next_incremental_certification_event(&mut self) -> Result<(), EndpointError> {
        let (begin, completion, next_page, accepted_checkpoints, replacement_hash) = {
            let pending = self
                .pending_incremental_publication
                .as_ref()
                .ok_or(EndpointError::SourceFacts)?;
            if pending.staged_page.is_some() || pending.terminal.is_some() {
                return Err(EndpointError::SourceFacts);
            }
            (
                pending.begin,
                pending.completion,
                pending.next_page,
                pending.replacement_checkpoints_accepted,
                pending.replacement_checkpoint_hash128,
            )
        };
        let replacement_page_count = u64::from(begin.target_page_end - begin.target_page_start);
        if next_page < replacement_page_count {
            let absolute_ordinal = u64::from(begin.target_page_start)
                .checked_add(next_page)
                .ok_or(EndpointError::SourceFacts)?;
            let page = self
                .runtime
                .as_ref()
                .ok_or(EndpointError::SourceFacts)?
                .persistent_source_facts_page(absolute_ordinal)
                .map_err(|_| EndpointError::SourceFacts)?
                .ok_or(EndpointError::SourceFacts)?;
            if page.ordinal() != absolute_ordinal
                || page.checkpoint_count() == 0
                || page.checkpoint_count() > SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX
                || accepted_checkpoints
                    .checked_add(
                        u64::try_from(page.checkpoint_count())
                            .map_err(|_| EndpointError::SourceFacts)?,
                    )
                    .is_none_or(|count| count > u64::from(begin.replacement_checkpoint_count))
            {
                return Err(EndpointError::SourceFacts);
            }
            let mut checkpoints =
                [SourceFactCheckpointWire::default(); SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX];
            for (output, checkpoint) in checkpoints.iter_mut().zip(page.checkpoints()) {
                *output = source_fact_checkpoint_wire(*checkpoint)?;
            }
            let page = SourceFactsDeltaPageEvent {
                certification_id: completion.certification_id,
                worker_replica_revision: completion.worker_replica_revision,
                ui_revision: completion.ui_revision,
                utf16_length: completion.utf16_length,
                intent_high_water: completion.intent_high_water,
                replacement_page_ordinal: u32::try_from(next_page)
                    .map_err(|_| EndpointError::SourceFacts)?,
                page_checkpoint_count: u32::try_from(page.checkpoint_count())
                    .map_err(|_| EndpointError::SourceFacts)?,
                checkpoints,
            };
            self.pending_incremental_publication
                .as_mut()
                .expect("delta publication was validated above")
                .staged_page = Some(page);
            self.certification = CertificationStatus::Publishing;
            self.emit(
                EventBody::SourceFactsDeltaPage(page),
                EndpointEventKind::SourceFactsDeltaPage,
                EventTransition::DeltaPage {
                    certification_id: page.certification_id,
                    replacement_page_ordinal: page.replacement_page_ordinal,
                },
                None,
            )
        } else {
            if next_page != replacement_page_count
                || accepted_checkpoints != u64::from(begin.replacement_checkpoint_count)
            {
                return Err(EndpointError::SourceFacts);
            }
            let terminal = SourceFactsDeltaCompletionEvent {
                completion,
                checkpoint_root_guard_algorithm: PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM,
                replacement_checkpoint_hash128: replacement_hash,
            };
            self.pending_incremental_publication
                .as_mut()
                .expect("delta publication was validated above")
                .terminal = Some(terminal);
            self.certification = CertificationStatus::AwaitingPromotion;
            self.emit(
                EventBody::SourceFactsDeltaCompleted(terminal),
                EndpointEventKind::SourceFactsDeltaCompleted,
                EventTransition::DeltaCompletion {
                    certification_id: completion.certification_id,
                },
                None,
            )
        }
    }

    fn incremental_completion_matches_target(
        &self,
        pending: &PendingIncrementalPublication,
    ) -> bool {
        let Some(terminal) = pending.terminal else {
            return false;
        };
        let completion = terminal.completion;
        let candidate = &pending.candidate;
        let facts = candidate.target_facts;
        let summary = facts.summary();
        let Some(runtime_facts) = self
            .runtime
            .as_ref()
            .and_then(DocumentRuntime::persistent_source_facts)
        else {
            return false;
        };
        let Some(observed) = self.observed else {
            return false;
        };
        runtime_facts == facts
            && candidate.target_source.version() == facts.source()
            && candidate.source_facts_delta.target() == facts.source()
            && candidate.source_facts_delta.profile() == facts.profile()
            && candidate.source_facts_delta.parser_profile() == facts.parser_profile()
            && completion.worker_replica_revision == observed.revision
            && completion.ui_revision == observed.revision
            && completion.utf16_length == observed.utf16_length
            && completion.intent_high_water == observed.intent_high_water
            && completion.fingerprint_algorithm == 1
            && completion.utf8_length == observed.utf8_length
            && u64::from(completion.logical_line_breaks) == summary.logical_line_breaks()
            && u64::from(completion.checkpoint_spacing_utf16)
                == facts.profile().checkpoint_spacing_utf16()
            && u64::from(completion.checkpoint_count) == facts.checkpoint_count()
            && u64::from(completion.page_count) == facts.page_count()
            && completion.content_hash128 == summary.rolling_hash().words()
            && completion.checkpoint_hash128 == facts.checkpoint_root_guard128()
            && pending.begin.target_checkpoint_root_guard_algorithm
                == PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM
            && pending.begin.target_checkpoint_root_guard128 == facts.checkpoint_root_guard128()
            && terminal.checkpoint_root_guard_algorithm
                == PERSISTENT_CHECKPOINT_ROOT_GUARD_ALGORITHM
    }

    fn cancel_derived(&mut self) -> Result<(), EndpointError> {
        if self.candidate.cancel().is_err() {
            self.poison_runtime();
            self.fail_closed(EndpointFailureCode::SourceFacts);
            return Err(EndpointError::Candidate);
        }
        self.clear_derived_source_state();
        Ok(())
    }

    fn cancel_derived_after_edit(&mut self) -> Result<(), EndpointError> {
        if self.candidate.cancel_for_source_facts_after_edit().is_err() {
            self.poison_runtime();
            self.fail_closed(EndpointFailureCode::SourceFacts);
            return Err(EndpointError::Candidate);
        }
        self.clear_derived_source_state();
        Ok(())
    }

    fn clear_derived_source_state(&mut self) {
        self.pending_certification = None;
        self.pending_incremental_publication = None;
        self.active_source_facts = None;
        self.active_candidate_certification = None;
        self.active_viewport_presentation_generation = None;
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.cancel_source_facts();
        }
    }

    fn poison_runtime(&mut self) {
        self.pending_certification = None;
        self.pending_incremental_publication = None;
        self.active_source_facts = None;
        let _ = self.candidate.begin_close();
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.cancel_source_facts();
        }
        self.pending_seed = None;
        self.pending_install = None;
        self.pending_edit_install = None;
        self.installed_target = None;
        self.observed = None;
        self.active_candidate_certification = None;
        self.last_external_certification = None;
        self.active_viewport_presentation_generation = None;
        self.certification = CertificationStatus::NotStarted;
        self.needs_reseed = true;
        if let Some(runtime) = self.runtime.as_mut() {
            let _ = runtime.begin_close();
            self.runtime_poisoned = true;
        }
    }

    fn fail_closed(&mut self, code: EndpointFailureCode) {
        if self.close_latched {
            return;
        }
        self.needs_reseed = true;
        self.lifecycle = EndpointLifecycle::Faulted;
        if self.failure_emitted || self.outstanding.is_some() {
            return;
        }
        self.failure_emitted = true;
        if self
            .emit(
                EventBody::Failed {
                    failure_code: code.code(),
                },
                EndpointEventKind::Failed,
                EventTransition::Failed,
                None,
            )
            .is_err()
        {
            self.lifecycle = EndpointLifecycle::Faulted;
        }
    }

    fn emit(
        &mut self,
        body: EventBody,
        kind: EndpointEventKind,
        transition: EventTransition,
        drain_grant: Option<DrainGrant>,
    ) -> Result<(), EndpointError> {
        if let Some(outstanding) = &self.outstanding {
            return Err(EndpointError::EventCreditOccupied {
                event_id: outstanding.event_id,
            });
        }
        if self.next_event_id == 0 {
            self.needs_reseed = true;
            self.lifecycle = EndpointLifecycle::Faulted;
            return Err(EndpointError::EventIdentityExhausted);
        }
        let event_id = self.next_event_id;
        self.next_event_id = self.next_event_id.checked_add(1).unwrap_or(0);
        let binding = self.binding.ok_or(EndpointError::InvalidLifecycle)?;
        let _ = binding;
        self.outstanding = Some(OutstandingEvent {
            event_id,
            payload: OutstandingPayload::Session(Box::new(body)),
            kind,
            transition,
            drain_grant,
        });
        Ok(())
    }

    fn emit_candidate(&mut self, event: CandidateEvent) -> Result<(), EndpointError> {
        if let Some(outstanding) = &self.outstanding {
            return Err(EndpointError::EventCreditOccupied {
                event_id: outstanding.event_id,
            });
        }
        if self.binding.is_none() || self.next_event_id == 0 {
            self.needs_reseed = true;
            self.lifecycle = EndpointLifecycle::Faulted;
            let _ = self.candidate.cancel();
            self.active_candidate_certification = None;
            return Err(EndpointError::EventIdentityExhausted);
        }
        let event_id = self.next_event_id;
        self.next_event_id = self.next_event_id.checked_add(1).unwrap_or(0);
        let kind = match &event.body {
            CandidateEventBody::Begin(_) => EndpointEventKind::PublicationBegin,
            CandidateEventBody::Packet { .. } => EndpointEventKind::PublicationPacket,
            CandidateEventBody::Commit(_) => EndpointEventKind::PublicationCommit,
            CandidateEventBody::DeliveryAcknowledged(_) => {
                EndpointEventKind::PublicationDeliveryAcknowledged
            }
        };
        self.outstanding = Some(OutstandingEvent {
            event_id,
            payload: OutstandingPayload::Publication(Box::new(event.body)),
            kind,
            transition: EventTransition::Candidate(event.credit),
            drain_grant: None,
        });
        Ok(())
    }

    fn emit_hot_inline(&mut self, event: HotInlineEvent) -> Result<(), EndpointError> {
        if let Some(outstanding) = &self.outstanding {
            return Err(EndpointError::EventCreditOccupied {
                event_id: outstanding.event_id,
            });
        }
        if self.binding.is_none() || self.next_event_id == 0 {
            self.needs_reseed = true;
            self.lifecycle = EndpointLifecycle::Faulted;
            self.candidate.cancel_hot_inline();
            return Err(EndpointError::EventIdentityExhausted);
        }
        let event_id = self.next_event_id;
        self.next_event_id = self.next_event_id.checked_add(1).unwrap_or(0);
        let kind = match &event.body {
            HotInlineEventBody::Begin(_) => EndpointEventKind::InlinePublicationBegin,
            HotInlineEventBody::Packet { .. } => EndpointEventKind::InlinePublicationPacket,
            HotInlineEventBody::Commit(_) => EndpointEventKind::InlinePublicationCommit,
            HotInlineEventBody::DeliveryAcknowledged(_) => {
                EndpointEventKind::InlinePublicationDeliveryAcknowledged
            }
        };
        self.outstanding = Some(OutstandingEvent {
            event_id,
            payload: OutstandingPayload::HotInline(Box::new(event.body)),
            kind,
            transition: EventTransition::HotInline(event.credit),
            drain_grant: None,
        });
        Ok(())
    }

    fn emit_viewport_presentation(
        &mut self,
        event: CandidateViewportPresentationEvent,
    ) -> Result<(), EndpointError> {
        if let Some(outstanding) = &self.outstanding {
            return Err(EndpointError::EventCreditOccupied {
                event_id: outstanding.event_id,
            });
        }
        if self.binding.is_none() || self.next_event_id == 0 {
            self.needs_reseed = true;
            self.lifecycle = EndpointLifecycle::Faulted;
            self.candidate.cancel_viewport_presentation();
            return Err(EndpointError::EventIdentityExhausted);
        }
        let event_id = self.next_event_id;
        self.next_event_id = self.next_event_id.checked_add(1).unwrap_or(0);
        let kind = match &event.body {
            CandidateViewportPresentationEventBody::Begin(_) => {
                EndpointEventKind::ViewportPublicationBegin
            }
            CandidateViewportPresentationEventBody::Packet { .. } => {
                EndpointEventKind::ViewportPublicationPacket
            }
            CandidateViewportPresentationEventBody::Commit(_) => {
                EndpointEventKind::ViewportPublicationCommit
            }
            CandidateViewportPresentationEventBody::DeliveryAcknowledged(_) => {
                EndpointEventKind::ViewportPublicationDeliveryAcknowledged
            }
        };
        self.outstanding = Some(OutstandingEvent {
            event_id,
            payload: OutstandingPayload::ViewportPresentation(Box::new(event.body)),
            kind,
            transition: EventTransition::ViewportPresentation(event.credit),
            drain_grant: None,
        });
        Ok(())
    }

    #[cfg(test)]
    fn set_next_event_id_for_test(&mut self, next: u32) {
        self.next_event_id = next;
    }

    #[cfg(test)]
    fn recursive_green_path_counts_for_test(&self) -> (u64, u64) {
        let receipt = self.candidate.recursive_green_path_receipt();
        (
            receipt.local_adoption_deliveries,
            receipt.clean_fallback_deliveries,
        )
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        // Emergency containment only. Normal owners must use credited close;
        // Drop cannot yield, so it performs the documented unmetered fallback
        // needed to keep DocumentRuntime's structural invariant intact.
        if let Some(runtime) = self.runtime.as_mut() {
            self.candidate.emergency_close(runtime);
            let _ = runtime.begin_close();
            while runtime.state() != DocumentState::Closed {
                if runtime.poll_close(usize::MAX).is_err() {
                    break;
                }
            }
        }
    }
}

fn accepted_snapshot_receipt(receipt: EventReceiptCommand, worker_revision: u32) -> bool {
    receipt.disposition == EventDisposition::Accepted
        && receipt.certification.is_none()
        && receipt.source.is_some_and(|source| {
            source.disposition == SourceReceiptDisposition::Acknowledged
                && source.dropped_intent_entries == 0
                && source.dropped_payload_utf16 == 0
                && source.dropped_deleted_utf16 == 0
                && source.dropped_operation_count == 0
                && source.worker_revision == worker_revision
        })
}

fn accepted_edit_receipt(receipt: EventReceiptCommand, pending: PendingEditInstall) -> bool {
    receipt.disposition == EventDisposition::Accepted
        && receipt.certification.is_none()
        && receipt.source.is_some_and(|source| {
            source.disposition == SourceReceiptDisposition::Acknowledged
                && source.dropped_intent_entries == pending.dropped_intent_entries
                && source.dropped_payload_utf16 == pending.dropped_payload_utf16
                && source.dropped_deleted_utf16 == pending.dropped_deleted_utf16
                && source.dropped_operation_count == pending.dropped_operation_count
                && source.worker_revision == pending.observed.revision
        })
}

fn plain_accepted_receipt(receipt: EventReceiptCommand) -> bool {
    receipt.disposition == EventDisposition::Accepted
        && receipt.source.is_none()
        && receipt.certification.is_none()
}

fn certification_binds_structural_ack(
    certification: SourceCertificationReceipt,
    ack: StructuralAck,
) -> bool {
    certification.worker_replica_revision == certification.ui_revision
        && certification.fingerprint_algorithm == 1
        && certification.ui_revision == ack.source_version.revision
        && certification.utf8_length == ack.source_version.utf8_length
        && certification.utf16_length == ack.source_version.utf16_length
        && certification.content_hash128 == ack.source_version.content_hash128
}

fn version_matches_stamp(version: SourceVersion, stamp: SourceStamp) -> bool {
    if version.revision().get() != u64::from(stamp.revision())
        || version.utf16_len() != stamp.utf16_length() as usize
    {
        return false;
    }
    match stamp {
        SourceStamp::Provisional { .. } => u32::try_from(version.byte_len()).is_ok(),
        SourceStamp::Known { utf8_length, .. } => version.byte_len() == utf8_length as usize,
    }
}

fn observed(
    version: SourceVersion,
    intent_high_water: u32,
) -> Option<ObservedSourceReplicaVersion> {
    Some(ObservedSourceReplicaVersion {
        revision: u32::try_from(version.revision().get()).ok()?,
        utf16_length: u32::try_from(version.utf16_len()).ok()?,
        utf8_length: u32::try_from(version.byte_len()).ok()?,
        intent_high_water,
    })
}

fn persistent_source_facts_match_target(
    facts: PersistentSourceFactsInfo,
    target: Option<SourceStamp>,
) -> bool {
    let source = facts.source();
    let summary = facts.summary();
    match target {
        Some(SourceStamp::Known {
            revision,
            utf16_length,
            utf8_length,
            content_hash128,
        }) => {
            source.revision().get() == u64::from(revision)
                && source.utf16_len() == utf16_length as usize
                && source.byte_len() == utf8_length as usize
                && summary.utf16_len() == u64::from(utf16_length)
                && summary.byte_len() == u64::from(utf8_length)
                && summary.rolling_hash().words() == content_hash128
        }
        Some(SourceStamp::Provisional {
            revision,
            utf16_length,
        }) => {
            source.revision().get() == u64::from(revision)
                && source.utf16_len() == utf16_length as usize
                && summary.byte_len() == source.byte_len() as u64
                && summary.utf16_len() == u64::from(utf16_length)
        }
        None => false,
    }
}

fn portable_checkpoint_hash(pages: &[Arc<SourceFactRootPage>]) -> Result<[u32; 4], EndpointError> {
    let mut hash = [0_u32; 4];
    for checkpoint in pages.iter().flat_map(|page| page.checkpoints()) {
        append_portable_checkpoint_hash(&mut hash, *checkpoint)?;
    }
    Ok(hash)
}

fn append_portable_checkpoint_hash(
    hash: &mut [u32; 4],
    checkpoint: SourceFactCheckpoint,
) -> Result<(), EndpointError> {
    let wire = source_fact_checkpoint_wire(checkpoint)?;
    append_portable_checkpoint_hash_wire(hash, wire);
    Ok(())
}

fn append_portable_checkpoint_hash_wire(hash: &mut [u32; 4], checkpoint: SourceFactCheckpointWire) {
    const BASES: [u32; 4] = [0x0010_0193, 0x9e37_79b1, 0x85eb_ca77, 0xc2b2_ae3d];
    for value in [
        checkpoint.utf16_offset,
        checkpoint.byte_offset,
        checkpoint.logical_line_breaks,
        checkpoint.rolling_hash128[0],
        checkpoint.rolling_hash128[1],
        checkpoint.rolling_hash128[2],
        checkpoint.rolling_hash128[3],
    ] {
        for byte in u64::from(value).to_le_bytes() {
            let term = u32::from(byte) + 1;
            for (lane, base) in hash.iter_mut().zip(BASES) {
                *lane = lane.wrapping_mul(base).wrapping_add(term);
            }
        }
    }
}

fn source_fact_checkpoint_wire(
    checkpoint: SourceFactCheckpoint,
) -> Result<SourceFactCheckpointWire, EndpointError> {
    Ok(SourceFactCheckpointWire {
        byte_offset: u32::try_from(checkpoint.byte_offset())
            .map_err(|_| EndpointError::SourceFacts)?,
        utf16_offset: u32::try_from(checkpoint.utf16_offset())
            .map_err(|_| EndpointError::SourceFacts)?,
        logical_line_breaks: u32::try_from(checkpoint.logical_line_breaks())
            .map_err(|_| EndpointError::SourceFacts)?,
        rolling_hash128: checkpoint.rolling_hash().words(),
    })
}

fn bounded_u32(value: usize) -> Result<u32, EndpointError> {
    u32::try_from(value).map_err(|_| EndpointError::InvalidLifecycle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v3_host_store::{
        HostConfig, HostMetricAffinity, HostPointQuery, HostPollOutcome as NativeHostPollOutcome,
        HostQueryBudget, HostSourceMetric, HostStructuralQueryOutcome, HostWorkGrant,
        NativeCandidateHost, HOST_M11_VIEWPORT_BYTES,
    };
    use crate::v3_publication_wire::{
        decode_event as decode_publication_event, HostPollPhase, OfferBegin, PublicationEventBody,
        PublicationMode, SourceVersion as PublicationSourceVersion, StructuralAck,
        MAXIMUM_PACKET_ENCODED_BYTES, PAYLOAD_PREFIX_BYTES as PUBLICATION_PAYLOAD_PREFIX_BYTES,
        PAYLOAD_SCHEMA as PUBLICATION_PAYLOAD_SCHEMA,
    };
    use crate::v3_session_wire::{
        SourceCertificationReceipt, SourceReceipt, ViewportPresentationLimits, PAYLOAD_SCHEMA,
    };
    use crate::v3_wire::{self, DecodeLimits, FrameKind, Header, Opcode, Status};

    const EVENT_BUFFER_BYTES: usize = 2_048;

    #[derive(Clone, Copy)]
    struct TestOperation<'a> {
        start: u32,
        end: u32,
        replacement: &'a str,
    }

    #[derive(Clone)]
    struct TestIntent<'a> {
        sequence: u32,
        base_revision: u32,
        revision: u32,
        base_stamp: SourceStamp,
        target_stamp: SourceStamp,
        operations: Vec<TestOperation<'a>>,
    }

    fn config(spacing: usize) -> EndpointConfig {
        EndpointConfig::new(
            standard_document_runtime_config(),
            SourceFactsScanProfile::new(spacing).unwrap(),
            SourceFactsRootLimits::default(),
            ParserProfileId::new(1).unwrap(),
        )
        .unwrap()
    }

    fn binding(generation: u32) -> SessionBinding {
        SessionBinding {
            document_session: [11, 22, 33, 44],
            source_session_identity: 55,
            worker_generation: generation,
        }
    }

    fn push_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn push_common(bytes: &mut Vec<u8>, variant: u16, binding: SessionBinding) {
        push_u16(bytes, PAYLOAD_SCHEMA);
        push_u16(bytes, variant);
        push_u32(bytes, binding.worker_generation);
        for word in binding.document_session {
            push_u32(bytes, word);
        }
        push_u32(bytes, binding.source_session_identity);
    }

    fn push_stamp(bytes: &mut Vec<u8>, stamp: SourceStamp) {
        match stamp {
            SourceStamp::Provisional {
                revision,
                utf16_length,
            } => {
                push_u32(bytes, 0);
                push_u32(bytes, revision);
                push_u32(bytes, utf16_length);
                for _ in 0..5 {
                    push_u32(bytes, 0);
                }
            }
            SourceStamp::Known {
                revision,
                utf16_length,
                utf8_length,
                content_hash128,
            } => {
                push_u32(bytes, 1);
                push_u32(bytes, revision);
                push_u32(bytes, utf16_length);
                push_u32(bytes, utf8_length);
                for word in content_hash128 {
                    push_u32(bytes, word);
                }
            }
        }
    }

    fn frame(opcode: Opcode, correlation_id: u32, payload: &[u8]) -> Vec<u8> {
        let mut output = vec![0; v3_wire::HEADER_BYTES + payload.len()];
        let written = v3_wire::encode_into(
            FrameKind::Request,
            Header {
                opcode,
                status: Status::Ok,
                flags: 0,
                correlation_id,
            },
            payload,
            &mut output,
        )
        .unwrap();
        output.truncate(written);
        output
    }

    fn open_frame(binding: SessionBinding, mode: OpenMode, correlation_id: u32) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(
            &mut payload,
            match mode {
                OpenMode::Fresh => 0,
                OpenMode::Recovery => 1,
            },
            binding,
        );
        frame(Opcode::ParserOpen, correlation_id, &payload)
    }

    #[allow(clippy::too_many_arguments)]
    fn snapshot_frame(
        binding: SessionBinding,
        lease_id: u32,
        start_utf16: u32,
        end_utf16: u32,
        total_utf16: u32,
        revision: u32,
        intent_high_water: u32,
        target: SourceStamp,
        source: &str,
    ) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, u16::from(start_utf16 != 0), binding);
        push_u32(&mut payload, lease_id);
        push_u32(&mut payload, revision);
        push_u32(&mut payload, start_utf16);
        push_u32(&mut payload, end_utf16);
        push_u32(&mut payload, total_utf16);
        push_u32(&mut payload, intent_high_water);
        push_stamp(&mut payload, target);
        push_u32(&mut payload, source.encode_utf16().count() as u32);
        push_u32(&mut payload, source.len() as u32);
        payload.extend_from_slice(source.as_bytes());
        frame(Opcode::SnapshotPage, lease_id, &payload)
    }

    fn edit_frame(binding: SessionBinding, lease_id: u32, intents: &[TestIntent<'_>]) -> Vec<u8> {
        let operation_count = intents
            .iter()
            .map(|intent| intent.operations.len() as u32)
            .sum();
        let payload_utf16 = intents
            .iter()
            .flat_map(|intent| &intent.operations)
            .map(|operation| operation.replacement.encode_utf16().count() as u32)
            .sum();
        let payload_utf8 = intents
            .iter()
            .flat_map(|intent| &intent.operations)
            .map(|operation| operation.replacement.len() as u32)
            .sum();
        let mut payload = Vec::new();
        push_common(&mut payload, 0, binding);
        push_u32(&mut payload, lease_id);
        push_u32(&mut payload, intents.first().unwrap().sequence);
        push_u32(&mut payload, intents.last().unwrap().sequence);
        push_u32(&mut payload, intents.len() as u32);
        push_u32(&mut payload, operation_count);
        push_u32(&mut payload, payload_utf16);
        push_u32(&mut payload, payload_utf8);
        for intent in intents {
            push_u32(&mut payload, intent.sequence);
            push_u32(&mut payload, intent.base_revision);
            push_u32(&mut payload, intent.revision);
            push_u32(&mut payload, intent.operations.len() as u32);
            push_stamp(&mut payload, intent.base_stamp);
            push_stamp(&mut payload, intent.target_stamp);
            for operation in &intent.operations {
                push_u32(&mut payload, operation.start);
                push_u32(&mut payload, operation.end);
                push_u32(
                    &mut payload,
                    operation.replacement.encode_utf16().count() as u32,
                );
                push_u32(&mut payload, operation.replacement.len() as u32);
                payload.extend_from_slice(operation.replacement.as_bytes());
            }
        }
        frame(Opcode::Edit, lease_id, &payload)
    }

    fn receipt_frame(
        binding: SessionBinding,
        event_id: u32,
        disposition: EventDisposition,
        source: Option<SourceReceipt>,
    ) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(
            &mut payload,
            match disposition {
                EventDisposition::Accepted => 0,
                EventDisposition::Stale => 1,
                EventDisposition::Rejected => 2,
            },
            binding,
        );
        push_u32(&mut payload, u32::from(source.is_some()));
        push_u32(&mut payload, 0);
        if let Some(source) = source {
            push_u32(&mut payload, source.disposition as u32);
            push_u32(&mut payload, source.dropped_intent_entries);
            push_u32(&mut payload, source.dropped_payload_utf16);
            push_u32(&mut payload, source.dropped_deleted_utf16);
            push_u32(&mut payload, source.dropped_operation_count);
            push_u32(&mut payload, source.worker_revision);
        }
        frame(Opcode::ParserAcknowledge, event_id, &payload)
    }

    fn push_publication_common(bytes: &mut Vec<u8>, variant: u16, binding: SessionBinding) {
        push_u16(bytes, PUBLICATION_PAYLOAD_SCHEMA);
        push_u16(bytes, variant);
        push_u32(bytes, binding.worker_generation);
        for word in binding.document_session {
            push_u32(bytes, word);
        }
        push_u32(bytes, binding.source_session_identity);
    }

    fn push_id128(bytes: &mut Vec<u8>, identity: [u32; 4]) {
        for word in identity {
            push_u32(bytes, word);
        }
    }

    fn push_publication_source(bytes: &mut Vec<u8>, source: PublicationSourceVersion) {
        push_id128(bytes, source.document_session);
        push_u32(bytes, source.revision);
        push_u32(bytes, source.utf8_length);
        push_u32(bytes, source.utf16_length);
        push_id128(bytes, source.content_hash128);
    }

    fn push_structural_ack(bytes: &mut Vec<u8>, ack: StructuralAck) {
        push_id128(bytes, ack.publication_session);
        push_u32(bytes, ack.host_revision);
        push_publication_source(bytes, ack.source_version);
        push_u32(bytes, ack.source_root[0]);
        push_u32(bytes, ack.source_root[1]);
        push_u32(bytes, ack.parse_generation);
        push_u32(bytes, ack.grammar_revision);
        push_u32(bytes, ack.syntax_profile);
        push_u32(bytes, ack.authority_mask);
        push_u32(bytes, ack.record_count);
        push_id128(bytes, ack.sequence_digest);
        push_id128(bytes, ack.manifest_digest);
    }

    fn host_poll_response(
        binding: SessionBinding,
        poll_ticket: u32,
        offer_id: [u32; 4],
        phase: HostPollPhase,
        outcome: NativeHostPollOutcome,
    ) -> Vec<u8> {
        let (variant, phase_code) = match phase {
            HostPollPhase::PacketCredit => (1, 0),
            HostPollPhase::Commit => (2, 1),
            HostPollPhase::Abort => (3, 2),
        };
        let mut payload = Vec::new();
        push_publication_common(&mut payload, variant, binding);
        push_u32(&mut payload, poll_ticket);
        push_id128(&mut payload, offer_id);
        push_u32(&mut payload, phase_code);
        match outcome {
            NativeHostPollOutcome::PacketCredit {
                offer_id: credited,
                next_frame_ordinal,
            } => {
                assert_eq!(phase, HostPollPhase::PacketCredit);
                push_id128(&mut payload, credited);
                push_u32(&mut payload, next_frame_ordinal);
            }
            NativeHostPollOutcome::Committed(ack) => {
                assert_eq!(phase, HostPollPhase::Commit);
                push_structural_ack(&mut payload, ack);
            }
            NativeHostPollOutcome::AbortComplete { offer_id: aborted } => {
                assert_eq!(phase, HostPollPhase::Abort);
                push_id128(&mut payload, aborted);
            }
            NativeHostPollOutcome::Pending | NativeHostPollOutcome::Closed => {
                panic!("terminal host-poll response required")
            }
        }
        let mut output = vec![0; v3_wire::HEADER_BYTES + payload.len()];
        let written = v3_wire::encode_into(
            FrameKind::Response,
            Header {
                opcode: Opcode::HostPoll,
                status: Status::Ok,
                flags: 0,
                correlation_id: poll_ticket,
            },
            &payload,
            &mut output,
        )
        .unwrap();
        output.truncate(written);
        output
    }

    fn certification_receipt_frame(
        binding: SessionBinding,
        event_id: u32,
        certification: SourceCertificationReceipt,
    ) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, EventDisposition::Accepted as u16, binding);
        push_u32(&mut payload, 0);
        push_u32(&mut payload, 1);
        for value in [
            certification.certification_id,
            certification.worker_replica_revision,
            certification.ui_revision,
            certification.utf16_length,
            certification.intent_high_water,
            certification.fingerprint_algorithm,
            certification.ui_revision,
            certification.utf16_length,
            certification.utf8_length,
            certification.logical_line_breaks,
            certification.checkpoint_spacing_utf16,
            certification.checkpoint_count,
            certification.page_count,
        ] {
            push_u32(&mut payload, value);
        }
        for word in certification
            .content_hash128
            .into_iter()
            .chain(certification.checkpoint_hash128)
        {
            push_u32(&mut payload, word);
        }
        frame(Opcode::ParserAcknowledge, event_id, &payload)
    }

    fn accepted_source(worker_revision: u32) -> SourceReceipt {
        SourceReceipt {
            disposition: SourceReceiptDisposition::Acknowledged,
            dropped_intent_entries: 0,
            dropped_payload_utf16: 0,
            dropped_deleted_utf16: 0,
            dropped_operation_count: 0,
            worker_revision,
        }
    }

    fn supersede_frame(
        binding: SessionBinding,
        correlation_id: u32,
        target_revision: u32,
    ) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, 0, binding);
        push_u32(&mut payload, target_revision);
        frame(Opcode::Supersede, correlation_id, &payload)
    }

    fn close_frame(binding: SessionBinding, correlation_id: u32) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, 0, binding);
        push_u32(&mut payload, binding.worker_generation);
        frame(Opcode::Close, correlation_id, &payload)
    }

    fn viewport_presentation_frame(
        command: ViewportPresentationCommand,
        correlation_id: u32,
    ) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, 0, command.binding);
        push_u32(&mut payload, command.viewport_generation);
        push_publication_source(&mut payload, command.source_version);
        push_structural_ack(&mut payload, command.base_ack);
        for value in [
            command.requested_start_utf8,
            command.requested_start_utf16,
            command.requested_end_utf8,
            command.requested_end_utf16,
            command.start_block_ordinal as u32,
            (command.start_block_ordinal >> 32) as u32,
            command.start_utf8,
            command.start_utf16,
            command.limits.maximum_structural_entries,
            command.limits.maximum_storage_pages,
            command.limits.maximum_inline_leaves,
            command.limits.maximum_inline_leaf_source_bytes,
            command.limits.maximum_inline_source_bytes,
            command.limits.maximum_fact_records,
            command.limits.maximum_encoded_frame_bytes,
            command.limits.maximum_parser_transitions,
        ] {
            push_u32(&mut payload, value);
        }
        frame(Opcode::ParserPresentViewport, correlation_id, &payload)
    }

    fn inline_refinement_frame(command: InlineRefinementCommand, correlation_id: u32) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, 0, command.binding);
        push_u32(&mut payload, command.refinement_generation);
        push_publication_source(&mut payload, command.source_version);
        push_structural_ack(&mut payload, command.base_ack);
        push_u32(&mut payload, command.byte_offset);
        push_u32(&mut payload, command.utf16_offset);
        push_u32(
            &mut payload,
            match command.affinity {
                crate::v3_session_wire::InlinePointAffinity::Before => 0,
                crate::v3_session_wire::InlinePointAffinity::After => 1,
            },
        );
        push_u32(
            &mut payload,
            match command.target {
                crate::v3_session_wire::InlineRefinementTarget::Automatic => 0,
                crate::v3_session_wire::InlineRefinementTarget::BulletListItemInline => 1,
                crate::v3_session_wire::InlineRefinementTarget::BulletListItemProjection => 2,
                crate::v3_session_wire::InlineRefinementTarget::OrderedListItemInline => 3,
                crate::v3_session_wire::InlineRefinementTarget::OrderedListItemProjection => 4,
                crate::v3_session_wire::InlineRefinementTarget::RecursiveGreenParagraph => 5,
                crate::v3_session_wire::InlineRefinementTarget::BlockQuoteProjection => 6,
                crate::v3_session_wire::InlineRefinementTarget::BlockQuoteInline => 7,
            },
        );
        frame(Opcode::ParserRefineInline, correlation_id, &payload)
    }

    fn drain_frame(binding: SessionBinding, drain_id: u32, transitions: u32) -> Vec<u8> {
        let mut payload = Vec::new();
        push_common(&mut payload, 0, binding);
        push_u32(&mut payload, drain_id);
        push_u32(&mut payload, transitions);
        frame(Opcode::Drain, drain_id, &payload)
    }

    fn open(endpoint: &mut Endpoint, binding: SessionBinding, mode: OpenMode) {
        endpoint.dispatch(&open_frame(binding, mode, 900)).unwrap();
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Opening);
        let outstanding = endpoint.outstanding_status().unwrap();
        assert_eq!(outstanding.kind, EndpointEventKind::Opened);
        endpoint
            .dispatch(&receipt_frame(
                binding,
                outstanding.event_id,
                EventDisposition::Accepted,
                None,
            ))
            .unwrap();
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Open);
    }

    fn acknowledge_source(endpoint: &mut Endpoint, binding: SessionBinding, revision: u32) {
        let event = endpoint.outstanding_status().unwrap();
        assert_eq!(event.kind, EndpointEventKind::SourceSynchronized);
        let source = endpoint.pending_edit_install.map_or_else(
            || accepted_source(revision),
            |pending| SourceReceipt {
                disposition: SourceReceiptDisposition::Acknowledged,
                dropped_intent_entries: pending.dropped_intent_entries,
                dropped_payload_utf16: pending.dropped_payload_utf16,
                dropped_deleted_utf16: pending.dropped_deleted_utf16,
                dropped_operation_count: pending.dropped_operation_count,
                worker_revision: pending.observed.revision,
            },
        );
        endpoint
            .dispatch(&receipt_frame(
                binding,
                event.event_id,
                EventDisposition::Accepted,
                Some(source),
            ))
            .unwrap();
    }

    fn seed_one_page(
        endpoint: &mut Endpoint,
        binding: SessionBinding,
        source: &str,
        target: SourceStamp,
        high_water: u32,
    ) {
        endpoint
            .dispatch(&snapshot_frame(
                binding,
                10,
                0,
                source.encode_utf16().count() as u32,
                source.encode_utf16().count() as u32,
                target.revision(),
                high_water,
                target,
                source,
            ))
            .unwrap();
        assert!(matches!(
            endpoint.status().source,
            EndpointSourceStatus::AwaitingInstallReceipt { .. }
        ));
        acknowledge_source(endpoint, binding, target.revision());
    }

    fn poll_to_certification(endpoint: &mut Endpoint) -> CertificationStatus {
        for _ in 0..10_000 {
            if endpoint.certification == CertificationStatus::Scanning {
                endpoint
                    .poll_source_facts(EndpointPollFuel {
                        maximum_source_bytes: 7,
                        maximum_checkpoints: 2,
                        maximum_retirement_transitions: 1,
                    })
                    .unwrap();
            }
            let Some(outstanding) = endpoint.outstanding.as_ref() else {
                continue;
            };
            let event_id = outstanding.event_id;
            match outstanding
                .session_body()
                .expect("source-fact session event")
            {
                EventBody::SourceFactsPage(_) => {
                    let binding = endpoint.binding.unwrap();
                    endpoint
                        .dispatch(&receipt_frame(
                            binding,
                            event_id,
                            EventDisposition::Accepted,
                            None,
                        ))
                        .unwrap();
                }
                EventBody::SourceFactsCompleted(completion) => {
                    let binding = endpoint.binding.unwrap();
                    endpoint
                        .dispatch(&certification_receipt_frame(
                            binding,
                            event_id,
                            completion.into(),
                        ))
                        .unwrap();
                    return endpoint.certification;
                }
                other => panic!("unexpected event while promoting source facts: {other:?}"),
            }
        }
        panic!("source facts did not complete under bounded polling");
    }

    fn poll_to_incremental_handoff(endpoint: &mut Endpoint) -> usize {
        let mut source_bytes_examined = 0_usize;
        for _ in 0..10_000 {
            let receipt = endpoint
                .poll_source_facts(EndpointPollFuel {
                    maximum_source_bytes: 37,
                    maximum_checkpoints: 5,
                    maximum_retirement_transitions: 1,
                })
                .expect("bounded incremental SourceFacts poll");
            source_bytes_examined += receipt.source_bytes_examined;
            assert!(
                receipt.source_bytes_examined <= 37,
                "one poll crossed its source-byte grant"
            );
            if endpoint.outstanding_status().is_some_and(|outstanding| {
                outstanding.kind == EndpointEventKind::SourceFactsDeltaBegin
            }) {
                assert!(receipt.scan_complete);
                assert_eq!(receipt.certification, Some(CertificationStatus::Publishing));
                return source_bytes_examined;
            }
            assert_eq!(endpoint.outstanding_status(), None);
        }
        panic!("incremental SourceFacts did not reach its typed handoff");
    }

    fn accept_incremental_certification(
        endpoint: &mut Endpoint,
        binding: SessionBinding,
    ) -> SourceCertificationReceipt {
        for _ in 0..10_000 {
            let outstanding = endpoint
                .outstanding
                .as_ref()
                .expect("delta publication retains one event credit");
            let event_id = outstanding.event_id;
            match outstanding
                .session_body()
                .expect("delta certification is a session event")
            {
                EventBody::SourceFactsDeltaBegin(_) | EventBody::SourceFactsDeltaPage(_) => {
                    endpoint
                        .dispatch(&receipt_frame(
                            binding,
                            event_id,
                            EventDisposition::Accepted,
                            None,
                        ))
                        .expect("delta event receipt");
                }
                EventBody::SourceFactsDeltaCompleted(completion) => {
                    let proof: SourceCertificationReceipt = completion.completion.into();
                    endpoint
                        .dispatch(&certification_receipt_frame(binding, event_id, proof))
                        .expect("delta promotion proof");
                    return proof;
                }
                _ => panic!("unexpected event while promoting SourceFacts delta"),
            }
        }
        panic!("SourceFacts delta did not complete under credited publication");
    }

    fn seed_ascii_pages_and_certify(
        endpoint: &mut Endpoint,
        binding: SessionBinding,
        source: &str,
    ) -> SourceStamp {
        const PAGE_BYTES: usize = crate::v3_session_wire::MAXIMUM_SNAPSHOT_UTF16 as usize;
        let target = known(0, source);
        for (ordinal, start) in (0..source.len()).step_by(PAGE_BYTES).enumerate() {
            let end = (start + PAGE_BYTES).min(source.len());
            endpoint
                .dispatch(&snapshot_frame(
                    binding,
                    10 + u32::try_from(ordinal).expect("page ordinal"),
                    u32::try_from(start).expect("page start"),
                    u32::try_from(end).expect("page end"),
                    u32::try_from(source.len()).expect("source length"),
                    0,
                    0,
                    target,
                    &source[start..end],
                ))
                .expect("bounded seed page");
            acknowledge_source(endpoint, binding, 0);
        }

        for _ in 0..10_000 {
            if endpoint.certification == CertificationStatus::Scanning {
                endpoint
                    .poll_source_facts(EndpointPollFuel {
                        maximum_source_bytes: 64 * 1024,
                        maximum_checkpoints: 64,
                        maximum_retirement_transitions: 32,
                    })
                    .expect("source-facts poll");
            }
            let Some(outstanding) = endpoint.outstanding.as_ref() else {
                continue;
            };
            let event_id = outstanding.event_id;
            match outstanding
                .session_body()
                .expect("source-fact session event")
            {
                EventBody::SourceFactsPage(_) => {
                    endpoint
                        .dispatch(&receipt_frame(
                            binding,
                            event_id,
                            EventDisposition::Accepted,
                            None,
                        ))
                        .expect("source-facts page receipt");
                }
                EventBody::SourceFactsCompleted(completion) => {
                    endpoint
                        .dispatch(&certification_receipt_frame(
                            binding,
                            event_id,
                            completion.into(),
                        ))
                        .expect("source certification receipt");
                    break;
                }
                _ => panic!("unexpected event while promoting source facts"),
            };
        }
        assert_eq!(
            endpoint.certification,
            CertificationStatus::ExternallyEligible
        );
        target
    }

    fn seed_utf8_pages_and_certify(
        endpoint: &mut Endpoint,
        binding: SessionBinding,
        source: &str,
    ) -> SourceStamp {
        const PAGE_UTF16: usize = crate::v3_session_wire::MAXIMUM_SNAPSHOT_UTF16 as usize;
        let target = known(0, source);
        let total_utf16 = source.encode_utf16().count();
        let mut start_byte = 0_usize;
        let mut start_utf16 = 0_usize;
        let mut ordinal = 0_u32;
        while start_byte < source.len() {
            let mut page_utf16 = 0_usize;
            let mut end_byte = start_byte;
            for (relative, character) in source[start_byte..].char_indices() {
                let width = character.len_utf16();
                if page_utf16 + width > PAGE_UTF16 {
                    break;
                }
                page_utf16 += width;
                end_byte = start_byte + relative + character.len_utf8();
            }
            assert!(
                end_byte > start_byte,
                "one UTF-8 scalar must fit a seed page"
            );
            let end_utf16 = start_utf16 + page_utf16;
            endpoint
                .dispatch(&snapshot_frame(
                    binding,
                    10 + ordinal,
                    u32::try_from(start_utf16).expect("page UTF-16 start"),
                    u32::try_from(end_utf16).expect("page UTF-16 end"),
                    u32::try_from(total_utf16).expect("source UTF-16 length"),
                    0,
                    0,
                    target,
                    &source[start_byte..end_byte],
                ))
                .expect("bounded UTF-8 seed page");
            acknowledge_source(endpoint, binding, 0);
            start_byte = end_byte;
            start_utf16 = end_utf16;
            ordinal += 1;
        }

        assert_eq!(start_utf16, total_utf16);
        for _ in 0..10_000 {
            if endpoint.certification == CertificationStatus::Scanning {
                endpoint
                    .poll_source_facts(EndpointPollFuel {
                        maximum_source_bytes: 64 * 1024,
                        maximum_checkpoints: 64,
                        maximum_retirement_transitions: 32,
                    })
                    .expect("source-facts poll");
            }
            let Some(outstanding) = endpoint.outstanding.as_ref() else {
                continue;
            };
            let event_id = outstanding.event_id;
            match outstanding
                .session_body()
                .expect("source-fact session event")
            {
                EventBody::SourceFactsPage(_) => {
                    endpoint
                        .dispatch(&receipt_frame(
                            binding,
                            event_id,
                            EventDisposition::Accepted,
                            None,
                        ))
                        .expect("source-facts page receipt");
                }
                EventBody::SourceFactsCompleted(completion) => {
                    endpoint
                        .dispatch(&certification_receipt_frame(
                            binding,
                            event_id,
                            completion.into(),
                        ))
                        .expect("source certification receipt");
                    break;
                }
                _ => panic!("unexpected event while promoting source facts"),
            }
        }
        assert_eq!(
            endpoint.certification,
            CertificationStatus::ExternallyEligible
        );
        target
    }

    fn poll_to_first_certification_event(endpoint: &mut Endpoint) -> OutstandingEventStatus {
        for _ in 0..10_000 {
            endpoint
                .poll_source_facts(EndpointPollFuel {
                    maximum_source_bytes: 7,
                    maximum_checkpoints: 2,
                    maximum_retirement_transitions: 1,
                })
                .unwrap();
            if let Some(outstanding) = endpoint.outstanding_status() {
                if matches!(
                    outstanding.kind,
                    EndpointEventKind::SourceFactsPage | EndpointEventKind::SourceFactsCompleted
                ) {
                    return outstanding;
                }
            }
        }
        panic!("source facts did not publish under bounded polling");
    }

    fn close_to_removable(endpoint: &mut Endpoint, binding: SessionBinding) {
        close_to_removable_with_grant(endpoint, binding, 1);
    }

    fn close_to_removable_with_grant(
        endpoint: &mut Endpoint,
        binding: SessionBinding,
        maximum_transitions: u32,
    ) {
        assert!((1..=MAXIMUM_ENDPOINT_RETIREMENT_TRANSITIONS as u32).contains(&maximum_transitions));
        endpoint.dispatch(&close_frame(binding, 700)).unwrap();
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Closing);
        if let Some(outstanding) = endpoint.outstanding_status() {
            endpoint
                .dispatch(&receipt_frame(
                    binding,
                    outstanding.event_id,
                    EventDisposition::Accepted,
                    None,
                ))
                .unwrap();
        }
        let mut drain_id = 800;
        loop {
            endpoint
                .dispatch(&drain_frame(binding, drain_id, maximum_transitions))
                .unwrap();
            let event = endpoint.outstanding_status().unwrap();
            assert_eq!(event.kind, EndpointEventKind::DrainProgress);
            let mut encoded = [0; EVENT_BUFFER_BYTES];
            let written = endpoint.encode_outstanding_event(&mut encoded).unwrap();
            let decoded = v3_wire::decode(
                &encoded[..written],
                FrameKind::Request,
                DecodeLimits::default(),
            )
            .unwrap();
            let complete = u32::from_le_bytes(decoded.payload[48..52].try_into().unwrap()) == 1;
            endpoint
                .dispatch(&receipt_frame(
                    binding,
                    event.event_id,
                    EventDisposition::Accepted,
                    None,
                ))
                .unwrap();
            if complete {
                break;
            }
            drain_id += 1;
        }
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Closed);
        let closed = endpoint.outstanding_status().unwrap();
        assert_eq!(closed.kind, EndpointEventKind::Closed);
        endpoint
            .dispatch(&receipt_frame(
                binding,
                closed.event_id,
                EventDisposition::Accepted,
                None,
            ))
            .unwrap();
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Removable);
    }

    #[derive(Debug)]
    struct CandidateDeliveryReceipt {
        offer: OfferBegin,
        ack: StructuralAck,
        source_root: [u32; 2],
        candidate_polls: usize,
        candidate_transitions: usize,
        candidate_before_begin_transitions: usize,
        candidate_after_begin_transitions: usize,
        candidate_phase_transitions: std::collections::BTreeMap<&'static str, usize>,
        packet_count: usize,
        source_facts_replacement_frames: usize,
        source_facts_replacement_records: u64,
        block_sequence_replacement_frames: usize,
        block_sequence_replacement_records: u64,
        maximum_encoded_event_bytes: usize,
        maximum_packet_bytes: usize,
        host_polls: usize,
        producer_cleanup_polls: usize,
    }

    fn publication_source_version(
        endpoint: &Endpoint,
        binding: SessionBinding,
    ) -> PublicationSourceVersion {
        let SourceStamp::Known {
            revision,
            utf16_length,
            utf8_length,
            content_hash128,
        } = endpoint
            .installed_target
            .expect("certification must refine the source stamp")
        else {
            panic!("certification must install exact source authority")
        };
        PublicationSourceVersion {
            document_session: binding.document_session,
            revision,
            utf8_length,
            utf16_length,
            content_hash128,
        }
    }

    fn deliver_current_candidate(
        endpoint: &mut Endpoint,
        binding: SessionBinding,
        host: &mut NativeCandidateHost,
    ) -> CandidateDeliveryReceipt {
        deliver_current_candidate_with_transition_grant(endpoint, binding, host, 31)
    }

    fn deliver_current_candidate_with_transition_grant(
        endpoint: &mut Endpoint,
        binding: SessionBinding,
        host: &mut NativeCandidateHost,
        transitions: u32,
    ) -> CandidateDeliveryReceipt {
        const EVENT_BYTES: usize =
            v3_wire::HEADER_BYTES + PUBLICATION_PAYLOAD_PREFIX_BYTES + MAXIMUM_PACKET_ENCODED_BYTES;
        const MAXIMUM_POLLS: usize = 1_000_000;
        assert!(transitions > 0);

        let mut source_root = None;
        let mut offer = None;
        let mut committed_ack = None;
        let mut candidate_polls = 0_usize;
        let mut candidate_transitions = 0_usize;
        let mut candidate_before_begin_transitions = 0_usize;
        let mut candidate_after_begin_transitions = 0_usize;
        let mut candidate_phase_transitions = std::collections::BTreeMap::new();
        let mut packet_count = 0_usize;
        let mut source_facts_replacement_frames = 0_usize;
        let mut source_facts_replacement_records = 0_u64;
        let mut block_sequence_replacement_frames = 0_usize;
        let mut block_sequence_replacement_records = 0_u64;
        let mut maximum_encoded_event_bytes = 0_usize;
        let mut maximum_packet_bytes = 0_usize;
        let mut host_polls = 0_usize;

        for _ in 0..MAXIMUM_POLLS {
            if endpoint.outstanding_status().is_none() {
                let before_begin = offer.is_none();
                let phase = endpoint.candidate.active_phase_for_test();
                let receipt = endpoint
                    .poll_candidate(transitions as usize)
                    .expect("bounded candidate poll");
                candidate_polls += 1;
                candidate_transitions += receipt.transitions;
                *candidate_phase_transitions.entry(phase).or_insert(0) += receipt.transitions;
                if before_begin {
                    candidate_before_begin_transitions += receipt.transitions;
                } else {
                    candidate_after_begin_transitions += receipt.transitions;
                }
                assert!(receipt.transitions <= transitions as usize);
            }
            let Some(status) = endpoint.outstanding_status() else {
                continue;
            };

            let mut encoded = vec![0_u8; EVENT_BYTES];
            let written = endpoint
                .encode_outstanding_event(&mut encoded)
                .expect("bounded candidate event");
            assert!(written <= EVENT_BYTES);
            maximum_encoded_event_bytes = maximum_encoded_event_bytes.max(written);
            encoded.truncate(written);
            let event = decode_publication_event(&encoded, binding)
                .expect("independently decoded publication event");
            assert_eq!(event.event_id, status.event_id);

            match event.body {
                PublicationEventBody::Begin(begin) => {
                    assert_eq!(status.kind, EndpointEventKind::PublicationBegin);
                    assert!(
                        begin.limits.maximum_packet_bytes as usize <= MAXIMUM_PACKET_ENCODED_BYTES
                    );
                    assert!(
                        offer.replace(begin).is_none(),
                        "candidate emitted two Begin events"
                    );
                    source_root = Some(begin.source_root);
                    host.begin_offer(begin).expect("host begins exact offer");
                    endpoint
                        .dispatch(&receipt_frame(
                            binding,
                            status.event_id,
                            EventDisposition::Accepted,
                            None,
                        ))
                        .expect("begin event receipt");
                }
                PublicationEventBody::Packet(packet) => {
                    assert_eq!(status.kind, EndpointEventKind::PublicationPacket);
                    assert!(packet.encoded().len() <= MAXIMUM_PACKET_ENCODED_BYTES);
                    maximum_packet_bytes = maximum_packet_bytes.max(packet.encoded().len());
                    packet_count += 1;
                    for frame in packet.frames() {
                        let frame = frame.expect("validated packet frame");
                        let metadata =
                            flark_engine::m11_host::M11CandidateHost::classify_frame(frame.bytes)
                                .expect("engine frame metadata");
                        assert_eq!(metadata.canonical_record_count, frame.record_count);
                        match metadata.kind {
                            flark_engine::m11_host::M11HostFrameKind::SourceFactsReplacementPage => {
                                source_facts_replacement_frames += 1;
                                source_facts_replacement_records += u64::from(frame.record_count);
                            }
                            flark_engine::m11_host::M11HostFrameKind::BlockSequenceReplacementPage => {
                                block_sequence_replacement_frames += 1;
                                block_sequence_replacement_records += u64::from(frame.record_count);
                            }
                            flark_engine::m11_host::M11HostFrameKind::Begin
                            | flark_engine::m11_host::M11HostFrameKind::Node
                            | flark_engine::m11_host::M11HostFrameKind::RecursiveGreenReplacementPage
                            | flark_engine::m11_host::M11HostFrameKind::End => {}
                        }
                    }
                    host.admit_packet(packet)
                        .expect("host copies one bounded packet");
                    endpoint
                        .dispatch(&receipt_frame(
                            binding,
                            status.event_id,
                            EventDisposition::Accepted,
                            None,
                        ))
                        .expect("packet event receipt");

                    let outcome = loop {
                        host_polls += 1;
                        assert!(host_polls <= MAXIMUM_POLLS);
                        let outcome = host
                            .poll(HostWorkGrant {
                                inspect_bytes: EVENT_BYTES as u32,
                                copy_bytes: EVENT_BYTES as u32,
                                transitions,
                            })
                            .expect("bounded host packet poll");
                        if !matches!(outcome, NativeHostPollOutcome::Pending) {
                            break outcome;
                        }
                    };
                    assert!(matches!(
                        outcome,
                        NativeHostPollOutcome::PacketCredit { .. }
                    ));
                    endpoint
                        .dispatch_host_poll(&host_poll_response(
                            binding,
                            status.event_id,
                            packet.offer_id,
                            HostPollPhase::PacketCredit,
                            outcome,
                        ))
                        .expect("exact host packet credit");
                }
                PublicationEventBody::Commit(commit) => {
                    assert_eq!(status.kind, EndpointEventKind::PublicationCommit);
                    host.request_commit(commit).expect("host commit request");
                    endpoint
                        .dispatch(&receipt_frame(
                            binding,
                            status.event_id,
                            EventDisposition::Accepted,
                            None,
                        ))
                        .expect("commit event receipt");

                    let outcome = loop {
                        host_polls += 1;
                        assert!(host_polls <= MAXIMUM_POLLS);
                        let outcome = host
                            .poll(HostWorkGrant {
                                inspect_bytes: EVENT_BYTES as u32,
                                copy_bytes: EVENT_BYTES as u32,
                                transitions,
                            })
                            .expect("bounded host install poll");
                        if !matches!(outcome, NativeHostPollOutcome::Pending) {
                            break outcome;
                        }
                    };
                    let NativeHostPollOutcome::Committed(ack) = outcome else {
                        panic!("commit must install the exact candidate")
                    };
                    committed_ack = Some(ack);
                    endpoint
                        .dispatch_host_poll(&host_poll_response(
                            binding,
                            status.event_id,
                            commit.offer_id,
                            HostPollPhase::Commit,
                            outcome,
                        ))
                        .expect("exact host commit ticket");
                }
                PublicationEventBody::DeliveryAcknowledged(ack) => {
                    assert_eq!(
                        status.kind,
                        EndpointEventKind::PublicationDeliveryAcknowledged
                    );
                    assert_eq!(Some(ack), committed_ack);
                    host.acknowledge_delivery(ack)
                        .expect("host accepts exact delivery proof");
                    endpoint
                        .dispatch(&receipt_frame(
                            binding,
                            status.event_id,
                            EventDisposition::Accepted,
                            None,
                        ))
                        .expect("delivery event receipt");

                    let mut producer_cleanup_polls = 0_usize;
                    while endpoint.candidate.cleanup_pending() {
                        producer_cleanup_polls += 1;
                        assert!(producer_cleanup_polls <= MAXIMUM_POLLS);
                        let receipt = endpoint
                            .poll_candidate(transitions as usize)
                            .expect("bounded producer reclamation");
                        assert!(receipt.transitions <= transitions as usize);
                        assert_eq!(receipt.outstanding_event, None);
                    }
                    assert_eq!(endpoint.outstanding_status(), None);
                    return CandidateDeliveryReceipt {
                        offer: offer.expect("candidate emitted Begin"),
                        ack,
                        source_root: source_root.expect("Begin source root"),
                        candidate_polls,
                        candidate_transitions,
                        candidate_before_begin_transitions,
                        candidate_after_begin_transitions,
                        candidate_phase_transitions,
                        packet_count,
                        source_facts_replacement_frames,
                        source_facts_replacement_records,
                        block_sequence_replacement_frames,
                        block_sequence_replacement_records,
                        maximum_encoded_event_bytes,
                        maximum_packet_bytes,
                        host_polls,
                        producer_cleanup_polls,
                    };
                }
                PublicationEventBody::AbortRequested { .. }
                | PublicationEventBody::Failed { .. } => {
                    panic!("exact clean candidate must not abort")
                }
            }
        }
        panic!("candidate publication did not complete under bounded polling")
    }

    fn close_host_to_zero(host: &mut NativeCandidateHost) -> usize {
        host.begin_close().expect("begin host close");
        for polls in 1..=1_000_000 {
            match host
                .poll(HostWorkGrant {
                    inspect_bytes: 0,
                    copy_bytes: 0,
                    transitions: 31,
                })
                .expect("bounded host close poll")
            {
                NativeHostPollOutcome::Closed => {
                    assert!(host.is_removable());
                    return polls;
                }
                NativeHostPollOutcome::Pending => {}
                other => panic!("unexpected host close outcome: {other:?}"),
            }
        }
        panic!("host did not reclaim to zero under bounded close polling")
    }

    #[derive(Debug, Eq, PartialEq)]
    struct TestStructuralViewport {
        start_bytes: u32,
        start_utf16: u32,
        end_bytes: u32,
        end_utf16: u32,
        encoded: Vec<u8>,
    }

    fn query_structural_viewport(
        host: &NativeCandidateHost,
        source_version: PublicationSourceVersion,
        position: usize,
    ) -> TestStructuralViewport {
        let position = u32::try_from(position).expect("ASCII fixture point");
        let mut output = vec![0_u8; 4_096];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version,
                    position: HostSourceMetric {
                        bytes: position,
                        utf16: position,
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
            .expect("query exact structural viewport");
        let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
            panic!("exact target must expose a structural viewport: {outcome:?}");
        };
        output.truncate(usize::try_from(receipt.encoded_bytes).expect("viewport byte count"));
        TestStructuralViewport {
            start_bytes: range.start.bytes,
            start_utf16: range.start.utf16,
            end_bytes: range.end.bytes,
            end_utf16: range.end.utf16,
            encoded: output,
        }
    }

    struct TightBulletListFixture {
        base: String,
        target: String,
        list_start: usize,
        target_list_end: usize,
        edit_start: usize,
        target_suffix_start: usize,
    }

    fn tight_bullet_list_fixture(item_count: usize) -> TightBulletListFixture {
        use std::fmt::Write as _;

        assert!(item_count >= 3);
        const PREFIX: &str = "ordinary prefix before list\n\n";
        const SUFFIX: &str = "ordinary suffix after list\n";
        const INSERTION: &str = "live ";
        let mut base = String::with_capacity(
            PREFIX.len()
                + item_count
                    .checked_mul(24)
                    .expect("bounded bullet-list fixture")
                + SUFFIX.len()
                + 1,
        );
        base.push_str(PREFIX);
        let list_start = base.len();
        for ordinal in 0..item_count {
            writeln!(&mut base, "- item {ordinal:06} payload").expect("bullet-list item");
        }
        let list_end = base.len();
        base.push('\n');
        let suffix_start = base.len();
        base.push_str(SUFFIX);

        let middle = item_count / 2;
        let edit_marker = format!("item {middle:06} payload");
        let edit_start = base.find(&edit_marker).expect("middle bullet-list item")
            + edit_marker.len()
            - "payload".len();
        let mut target = base.clone();
        target.insert_str(edit_start, INSERTION);
        TightBulletListFixture {
            base,
            target,
            list_start,
            target_list_end: list_end + INSERTION.len(),
            edit_start,
            target_suffix_start: suffix_start + INSERTION.len(),
        }
    }

    struct EndpointLocalListInsertionReceipt {
        source_version: PublicationSourceVersion,
        delivery: CandidateDeliveryReceipt,
        source_facts_bytes_examined: usize,
    }

    fn apply_endpoint_local_list_insertion(
        endpoint: &mut Endpoint,
        host: &mut NativeCandidateHost,
        binding: SessionBinding,
        correlation_id: u32,
        sequence: u32,
        base_stamp: SourceStamp,
        target_stamp: SourceStamp,
        edit_start: usize,
        insertion: &str,
        expected_base_ack: StructuralAck,
    ) -> EndpointLocalListInsertionReceipt {
        endpoint
            .dispatch(&edit_frame(
                binding,
                correlation_id,
                &[TestIntent {
                    sequence,
                    base_revision: base_stamp.revision(),
                    revision: target_stamp.revision(),
                    base_stamp,
                    target_stamp,
                    operations: vec![TestOperation {
                        start: u32::try_from(edit_start).expect("local insertion start"),
                        end: u32::try_from(edit_start).expect("local insertion end"),
                        replacement: insertion,
                    }],
                }],
            ))
            .expect("dispatch local list insertion");
        assert!(
            endpoint
                .candidate
                .has_bullet_list_local_edit_plan_for_test(),
            "edit admission must retain authenticated local-list authority"
        );
        acknowledge_source(endpoint, binding, target_stamp.revision());
        assert!(
            endpoint
                .candidate
                .has_bullet_list_local_edit_plan_for_test(),
            "source acknowledgement must preserve authenticated local-list authority"
        );
        let source_facts_plan = match endpoint
            .active_source_facts
            .as_ref()
            .expect("local insertion starts SourceFacts")
        {
            ActiveSourceFacts::Incremental(plan) => plan.clone(),
            ActiveSourceFacts::Clean => {
                panic!("local insertion must retain the exact SourceFacts base")
            }
        };
        assert_eq!(
            source_facts_plan.exact_parser_base_byte_range(),
            Some(&(edit_start..edit_start)),
            "the exact parser handoff must retain the pure-insertion envelope"
        );
        assert_eq!(
            source_facts_plan.base().revision().get(),
            u64::from(base_stamp.revision()),
            "incremental SourceFacts must replan from the acknowledged base revision"
        );
        let source_facts_bytes_examined = poll_to_incremental_handoff(endpoint);
        assert!(
            endpoint
                .candidate
                .has_bullet_list_local_edit_plan_for_test(),
            "SourceFacts polling must preserve authenticated local-list authority"
        );
        accept_incremental_certification(endpoint, binding);
        assert_eq!(
            endpoint.candidate.active_phase_for_test(),
            "ParsingBulletListLocal",
            "incremental certification must activate the local-list parser"
        );

        let source_version = publication_source_version(endpoint, binding);
        host.observe_source_version(source_version)
            .expect("host observes local-list target");
        let delivery = deliver_current_candidate_with_transition_grant(endpoint, binding, host, 1);
        assert_eq!(delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(delivery.offer.base_ack, Some(expected_base_ack));
        assert_eq!(delivery.ack.source_version, source_version);
        EndpointLocalListInsertionReceipt {
            source_version,
            delivery,
            source_facts_bytes_examined,
        }
    }

    fn assert_bounded_local_list_delivery(delivery: &CandidateDeliveryReceipt) {
        let local = delivery
            .candidate_phase_transitions
            .get("ParsingBulletListLocal")
            .copied()
            .unwrap_or(0);
        let build = delivery
            .candidate_phase_transitions
            .get("BuildingExact")
            .copied()
            .unwrap_or(0);
        let stream = delivery
            .candidate_phase_transitions
            .get("Streaming")
            .copied()
            .unwrap_or(0);
        assert_eq!(delivery.candidate_transitions, local + build + stream);
        assert!((1..=512).contains(&local));
        assert!((1..=64).contains(&build));
        assert!((1..=64).contains(&stream));
        assert!(delivery.offer.transferred_record_count <= 64);
        assert!(
            delivery.offer.transferred_record_count < delivery.offer.target_record_count,
            "a local delta must retain records outside its replacement"
        );
        assert!((1..=4).contains(&delivery.source_facts_replacement_frames));
        assert!((1..=4).contains(&delivery.source_facts_replacement_records));
        assert!((1..=4).contains(&delivery.block_sequence_replacement_frames));
        assert!((1..=4).contains(&delivery.block_sequence_replacement_records));
        assert!(delivery.packet_count <= 16);
    }

    #[derive(Debug)]
    struct BulletListLocalEndpointGateReceipt {
        item_count: usize,
        source_bytes: usize,
        source_facts_bytes_examined: usize,
        candidate_transitions: usize,
        candidate_before_begin_transitions: usize,
        candidate_after_begin_transitions: usize,
        bullet_list_local_transitions: usize,
        building_exact_transitions: usize,
        streaming_transitions: usize,
        transferred_records: u32,
        source_facts_replacement_records: u64,
        block_sequence_replacement_records: u64,
        packet_count: usize,
    }

    fn run_bullet_list_local_endpoint_gate(
        item_count: usize,
        worker_generation: u32,
    ) -> BulletListLocalEndpointGateReceipt {
        const INSERTION: &str = "live ";
        const MAXIMUM_LOCAL_TRANSFER_RECORDS: u32 = 64;
        // SourceFacts replaces at most one packed 64-checkpoint page. With
        // the production-shaped 4 KiB checkpoint spacing this is a fixed
        // 256 KiB storage window plus the inserted bytes, independent of the
        // list or document length. Parser-local work has its separate 64 KiB
        // island cap.
        const MAXIMUM_LOCAL_SOURCE_FACTS_BYTES: usize =
            4 * 1024 * SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX + INSERTION.len();
        const MAXIMUM_LOCAL_CANDIDATE_TRANSITIONS: usize = 200_000;
        const VIEWPORT_HEADER_BYTES: usize = 20;

        let fixture = tight_bullet_list_fixture(item_count);
        assert_eq!(fixture.base.len(), fixture.base.encode_utf16().count());
        assert_eq!(fixture.target.len(), fixture.target.encode_utf16().count());
        let binding = binding(worker_generation);
        let mut endpoint = Endpoint::fresh(config(4 * 1024));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let base_stamp = seed_ascii_pages_and_certify(&mut endpoint, binding, &fixture.base);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent bullet-list host");
        host.observe_source_version(publication_source_version(&endpoint, binding))
            .expect("host observes bullet-list base");
        let base_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(base_delivery.source_facts_replacement_frames, 0);
        assert_eq!(base_delivery.block_sequence_replacement_frames, 0);

        let target_stamp = known(1, &fixture.target);
        endpoint
            .dispatch(&edit_frame(
                binding,
                20,
                &[TestIntent {
                    sequence: 1,
                    base_revision: 0,
                    revision: 1,
                    base_stamp,
                    target_stamp,
                    operations: vec![TestOperation {
                        start: u32::try_from(fixture.edit_start).expect("middle insertion start"),
                        end: u32::try_from(fixture.edit_start).expect("middle insertion end"),
                        replacement: INSERTION,
                    }],
                }],
            ))
            .expect("middle pure insertion");
        assert!(
            endpoint
                .candidate
                .has_bullet_list_local_edit_plan_for_test(),
            "endpoint edit admission must retain the local-list plan"
        );
        acknowledge_source(&mut endpoint, binding, 1);
        assert!(
            endpoint
                .candidate
                .has_bullet_list_local_edit_plan_for_test(),
            "source acknowledgement must preserve the local-list plan"
        );
        let source_facts_plan = match endpoint
            .active_source_facts
            .as_ref()
            .expect("pure insertion starts SourceFacts")
        {
            ActiveSourceFacts::Incremental(plan) => plan.clone(),
            ActiveSourceFacts::Clean => {
                panic!("middle list insertion must retain the exact SourceFacts base")
            }
        };
        assert_eq!(
            source_facts_plan.exact_parser_base_byte_range(),
            Some(&(fixture.edit_start..fixture.edit_start)),
            "the parser handoff must retain the pure-insertion envelope"
        );
        let source_facts_bytes_examined = poll_to_incremental_handoff(&mut endpoint);
        assert!(
            source_facts_bytes_examined <= MAXIMUM_LOCAL_SOURCE_FACTS_BYTES,
            "incremental SourceFacts inspected {source_facts_bytes_examined} bytes for a \
             {}-item list",
            item_count
        );
        assert!(
            endpoint
                .candidate
                .has_bullet_list_local_edit_plan_for_test(),
            "SourceFacts polling must preserve the local-list plan"
        );
        accept_incremental_certification(&mut endpoint, binding);
        assert_eq!(
            endpoint.candidate.active_phase_for_test(),
            "ParsingBulletListLocal",
            "accepted incremental certification must activate the local-list parser"
        );

        let target_source_version = publication_source_version(&endpoint, binding);
        host.observe_source_version(target_source_version)
            .expect("host observes bullet-list target");
        let target_delivery =
            deliver_current_candidate_with_transition_grant(&mut endpoint, binding, &mut host, 1);
        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
        assert_eq!(target_delivery.ack.source_version, target_source_version);
        assert!(
            target_delivery.offer.transferred_record_count <= MAXIMUM_LOCAL_TRANSFER_RECORDS,
            "{}-item list insertion transferred {} records",
            item_count,
            target_delivery.offer.transferred_record_count
        );
        assert!(
            target_delivery.offer.transferred_record_count
                < target_delivery.offer.target_record_count,
            "the target must retain records outside the local list replacement"
        );
        assert!((1..=4).contains(&target_delivery.source_facts_replacement_frames));
        assert!((1..=4).contains(&target_delivery.block_sequence_replacement_frames));
        assert!((1..=4).contains(&target_delivery.block_sequence_replacement_records));
        assert!(
            target_delivery.candidate_transitions <= MAXIMUM_LOCAL_CANDIDATE_TRANSITIONS,
            "{}-item list insertion consumed {} candidate transitions",
            item_count,
            target_delivery.candidate_transitions
        );
        assert!(target_delivery.packet_count <= 16);

        let incremental_prefix = query_structural_viewport(&host, target_source_version, 1);
        let incremental_list = query_structural_viewport(
            &host,
            target_source_version,
            fixture.edit_start + INSERTION.len(),
        );
        let incremental_suffix = query_structural_viewport(
            &host,
            target_source_version,
            fixture.target_suffix_start + 1,
        );
        assert_eq!(incremental_list.start_bytes as usize, fixture.list_start);
        assert_eq!(incremental_list.end_bytes as usize, fixture.target_list_end);
        assert_eq!(
            incremental_list.encoded[VIEWPORT_HEADER_BYTES + 12],
            9,
            "the edited block must remain the exact variant-9 list"
        );
        assert_eq!(
            u32::from_le_bytes(
                incremental_list.encoded[VIEWPORT_HEADER_BYTES + 56..VIEWPORT_HEADER_BYTES + 60]
                    .try_into()
                    .expect("variant-9 item count"),
            ),
            u32::try_from(item_count).expect("bounded item count")
        );

        // A separately clean-parsed target is the semantic oracle. Compare
        // all three top-level regions so the delta cannot hide a corrupt
        // prefix or suffix behind one correct list record.
        let clean_binding = self::binding(
            worker_generation
                .checked_add(100)
                .expect("clean worker generation"),
        );
        let mut clean_endpoint = Endpoint::fresh(config(4 * 1024));
        open(&mut clean_endpoint, clean_binding, OpenMode::Fresh);
        seed_ascii_pages_and_certify(&mut clean_endpoint, clean_binding, &fixture.target);
        let clean_source_version = publication_source_version(&clean_endpoint, clean_binding);
        let mut clean_host = NativeCandidateHost::new(HostConfig {
            document_session: clean_binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent clean-oracle host");
        clean_host
            .observe_source_version(clean_source_version)
            .expect("clean host observes target");
        let clean_delivery =
            deliver_current_candidate(&mut clean_endpoint, clean_binding, &mut clean_host);
        assert_eq!(clean_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(
            incremental_prefix,
            query_structural_viewport(&clean_host, clean_source_version, 1)
        );
        assert_eq!(
            incremental_list,
            query_structural_viewport(
                &clean_host,
                clean_source_version,
                fixture.edit_start + INSERTION.len(),
            )
        );
        assert_eq!(
            incremental_suffix,
            query_structural_viewport(
                &clean_host,
                clean_source_version,
                fixture.target_suffix_start + 1,
            )
        );

        close_host_to_zero(&mut clean_host);
        close_to_removable(&mut clean_endpoint, clean_binding);
        close_host_to_zero(&mut host);
        close_to_removable(&mut endpoint, binding);

        BulletListLocalEndpointGateReceipt {
            item_count,
            source_bytes: fixture.target.len(),
            source_facts_bytes_examined,
            candidate_transitions: target_delivery.candidate_transitions,
            candidate_before_begin_transitions: target_delivery.candidate_before_begin_transitions,
            candidate_after_begin_transitions: target_delivery.candidate_after_begin_transitions,
            bullet_list_local_transitions: target_delivery
                .candidate_phase_transitions
                .get("ParsingBulletListLocal")
                .copied()
                .unwrap_or(0),
            building_exact_transitions: target_delivery
                .candidate_phase_transitions
                .get("BuildingExact")
                .copied()
                .unwrap_or(0),
            streaming_transitions: target_delivery
                .candidate_phase_transitions
                .get("Streaming")
                .copied()
                .unwrap_or(0),
            transferred_records: target_delivery.offer.transferred_record_count,
            source_facts_replacement_records: target_delivery.source_facts_replacement_records,
            block_sequence_replacement_records: target_delivery.block_sequence_replacement_records,
            packet_count: target_delivery.packet_count,
        }
    }

    #[test]
    fn viewport_presentation_requires_external_exact_base_and_maps_semantic_limits() {
        let binding = binding(40);
        let source = "**bold** and _emphasis_ and `code`\n";
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        seed_ascii_pages_and_certify(&mut endpoint, binding, source);
        let source_version = publication_source_version(&endpoint, binding);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent viewport host");
        host.observe_source_version(source_version)
            .expect("host observes exact viewport source");
        let delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);
        let command = ViewportPresentationCommand {
            binding,
            viewport_generation: 1,
            source_version,
            base_ack: delivery.ack,
            requested_start_utf8: 0,
            requested_start_utf16: 0,
            requested_end_utf8: source_version.utf8_length,
            requested_end_utf16: source_version.utf16_length,
            start_block_ordinal: 0,
            start_utf8: 0,
            start_utf16: 0,
            limits: ViewportPresentationLimits {
                maximum_structural_entries: 1,
                maximum_storage_pages: 1,
                maximum_inline_leaves: 1,
                maximum_inline_leaf_source_bytes: source_version.utf8_length,
                maximum_inline_source_bytes: source_version.utf8_length,
                maximum_fact_records: 16,
                maximum_encoded_frame_bytes: 256 * 1024,
                maximum_parser_transitions: 10_000,
            },
        };

        endpoint.certification = CertificationStatus::AwaitingPromotion;
        assert!(matches!(
            endpoint.handle_viewport_presentation(command),
            Err(EndpointError::InvalidLifecycle)
        ));
        endpoint.certification = CertificationStatus::ExternallyEligible;

        let mut wrong_binding = command;
        wrong_binding.binding.worker_generation += 1;
        assert!(matches!(
            endpoint.handle_viewport_presentation(wrong_binding),
            Err(EndpointError::InvalidLifecycle)
        ));

        let mut wrong_base = command;
        wrong_base.base_ack.host_revision += 1;
        assert!(matches!(
            endpoint.dispatch(&viewport_presentation_frame(wrong_base, 60)),
            Err(EndpointError::Candidate)
        ));

        let mut one_byte_leaf_limit = command;
        one_byte_leaf_limit.limits.maximum_inline_leaf_source_bytes = 1;
        let unavailable = endpoint
            .dispatch(&viewport_presentation_frame(one_byte_leaf_limit, 61))
            .expect("bounded viewport failure remains attempt-local");
        let unavailable_event = unavailable
            .outstanding_event
            .expect("attempt-local viewport failure emits one credited event");
        assert!(matches!(
            endpoint
                .outstanding
                .as_ref()
                .and_then(OutstandingEvent::session_body),
            Some(EventBody::ViewportPresentationUnavailable(
                ViewportPresentationUnavailableEvent {
                    viewport_generation: 1,
                    reason_code: VIEWPORT_PRESENTATION_UNAVAILABLE_BUDGET_EXCEEDED,
                }
            ))
        ));
        endpoint
            .dispatch(&receipt_frame(
                binding,
                unavailable_event.event_id,
                EventDisposition::Accepted,
                None,
            ))
            .expect("accept viewport unavailable receipt");

        let receipt = endpoint
            .dispatch(&viewport_presentation_frame(command, 63))
            .expect("accept exact viewport presentation command");
        assert_eq!(receipt.correlation_id, 63);
        assert_eq!(
            receipt.action,
            EndpointCommandAction::ViewportPresentationAccepted
        );
        assert_eq!(receipt.outstanding_event, None);

        let poll = endpoint
            .poll_candidate(1)
            .expect("bounded viewport derivation poll");
        assert!(poll.transitions <= 1);
        assert_eq!(poll.outstanding_event, None);

        endpoint.candidate.cancel_hot_inline();
        for _ in 0..10_000 {
            if !endpoint.candidate.cleanup_pending() {
                break;
            }
            let poll = endpoint
                .poll_candidate(31)
                .expect("bounded viewport cancellation");
            assert!(poll.transitions <= 31);
            assert_eq!(poll.outstanding_event, None);
        }
        assert!(!endpoint.candidate.cleanup_pending());
        close_host_to_zero(&mut host);
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn twenty_thousand_item_list_middle_insertion_is_a_bounded_exact_delta() {
        let receipt = run_bullet_list_local_endpoint_gate(20_000, 41);
        eprintln!("m11_bullet_list_local_endpoint_gate {receipt:?}");
        assert_eq!(receipt.item_count, 20_000);
        assert!(receipt.source_bytes > 400_000);
        assert!(
            receipt.source_facts_bytes_examined
                <= 4 * 1024 * SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX + "live ".len()
        );
        assert!(receipt.candidate_transitions <= 200_000);
        assert_eq!(
            receipt.candidate_transitions,
            receipt.candidate_before_begin_transitions + receipt.candidate_after_begin_transitions
        );
        assert_eq!(
            receipt.candidate_transitions,
            receipt.bullet_list_local_transitions
                + receipt.building_exact_transitions
                + receipt.streaming_transitions
        );
        assert!((1..=512).contains(&receipt.bullet_list_local_transitions));
        assert!((1..=64).contains(&receipt.building_exact_transitions));
        assert!((1..=64).contains(&receipt.streaming_transitions));
        assert!(receipt.transferred_records <= 64);
        assert!(receipt.source_facts_replacement_records <= 4);
        assert!(receipt.block_sequence_replacement_records <= 4);
        assert!(receipt.packet_count <= 16);
    }

    #[test]
    fn consecutive_middle_item_insertions_rebase_from_underfilled_committed_topology() {
        const ITEM_COUNT: usize = 20_000;
        const FIRST_INSERTION: &str = "live ";
        const SECOND_INSERTION: &str = "again ";
        const MAXIMUM_LOCAL_SOURCE_FACTS_BYTES: usize =
            4 * 1024 * SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX + SECOND_INSERTION.len();
        const VIEWPORT_HEADER_BYTES: usize = 20;

        let fixture = tight_bullet_list_fixture(ITEM_COUNT);
        let second_edit_start = fixture.edit_start + FIRST_INSERTION.len();
        let mut final_target = fixture.target.clone();
        final_target.insert_str(second_edit_start, SECOND_INSERTION);
        assert_eq!(final_target.len(), final_target.encode_utf16().count());

        let binding = binding(43);
        let mut endpoint = Endpoint::fresh(config(4 * 1024));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let base_stamp = seed_ascii_pages_and_certify(&mut endpoint, binding, &fixture.base);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent consecutive-edit host");
        host.observe_source_version(publication_source_version(&endpoint, binding))
            .expect("host observes consecutive-edit base");
        let base_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

        let first_stamp = known(1, &fixture.target);
        let first = apply_endpoint_local_list_insertion(
            &mut endpoint,
            &mut host,
            binding,
            20,
            1,
            base_stamp,
            first_stamp,
            fixture.edit_start,
            FIRST_INSERTION,
            base_delivery.ack,
        );
        assert_bounded_local_list_delivery(&first.delivery);
        assert!(
            first.source_facts_bytes_examined
                <= 4 * 1024 * SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX + FIRST_INSERTION.len()
        );

        let committed = endpoint
            .runtime
            .as_ref()
            .and_then(DocumentRuntime::persistent_source_facts)
            .expect("first delta commits persistent SourceFacts");
        let checkpoint_count = committed.checkpoint_count();
        let page_count = committed.page_count();
        let checkpoints_per_page =
            u64::try_from(SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX).expect("checkpoint page bound");
        let packed_page_count = checkpoint_count
            .checked_add(checkpoints_per_page - 1)
            .expect("bounded checkpoint count")
            / checkpoints_per_page;
        assert!(
            page_count > packed_page_count,
            "the second edit must replan from committed underfilled pages: \
             {checkpoint_count} checkpoints in {page_count} pages"
        );

        let final_stamp = known(2, &final_target);
        let second = apply_endpoint_local_list_insertion(
            &mut endpoint,
            &mut host,
            binding,
            21,
            2,
            first_stamp,
            final_stamp,
            second_edit_start,
            SECOND_INSERTION,
            first.delivery.ack,
        );
        assert_bounded_local_list_delivery(&second.delivery);
        assert!(second.source_facts_bytes_examined <= MAXIMUM_LOCAL_SOURCE_FACTS_BYTES);
        assert_eq!(second.delivery.offer.base_ack, Some(first.delivery.ack));
        eprintln!(
            "m11_consecutive_bullet_list_endpoint_gate checkpoints={checkpoint_count} \
             pages={page_count} first_source_facts_bytes={} second_source_facts_bytes={} \
             first_phases={:?} second_phases={:?} first_records={} second_records={}",
            first.source_facts_bytes_examined,
            second.source_facts_bytes_examined,
            first.delivery.candidate_phase_transitions,
            second.delivery.candidate_phase_transitions,
            first.delivery.offer.transferred_record_count,
            second.delivery.offer.transferred_record_count,
        );

        let final_list_end = fixture.target_list_end + SECOND_INSERTION.len();
        let final_suffix_start = fixture.target_suffix_start + SECOND_INSERTION.len();
        let incremental_prefix = query_structural_viewport(&host, second.source_version, 1);
        let incremental_list = query_structural_viewport(
            &host,
            second.source_version,
            second_edit_start + SECOND_INSERTION.len(),
        );
        let incremental_suffix =
            query_structural_viewport(&host, second.source_version, final_suffix_start + 1);
        assert_eq!(incremental_list.start_bytes as usize, fixture.list_start);
        assert_eq!(incremental_list.end_bytes as usize, final_list_end);
        assert_eq!(
            incremental_list.encoded[VIEWPORT_HEADER_BYTES + 12],
            9,
            "the twice-edited block must remain the exact variant-9 list"
        );
        assert_eq!(
            u32::from_le_bytes(
                incremental_list.encoded[VIEWPORT_HEADER_BYTES + 56..VIEWPORT_HEADER_BYTES + 60]
                    .try_into()
                    .expect("variant-9 item count"),
            ),
            u32::try_from(ITEM_COUNT).expect("bounded item count")
        );

        let clean_binding = self::binding(143);
        let mut clean_endpoint = Endpoint::fresh(config(4 * 1024));
        open(&mut clean_endpoint, clean_binding, OpenMode::Fresh);
        seed_ascii_pages_and_certify(&mut clean_endpoint, clean_binding, &final_target);
        let clean_source_version = publication_source_version(&clean_endpoint, clean_binding);
        let mut clean_host = NativeCandidateHost::new(HostConfig {
            document_session: clean_binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent consecutive-edit clean oracle");
        clean_host
            .observe_source_version(clean_source_version)
            .expect("clean host observes consecutive-edit target");
        let clean_delivery =
            deliver_current_candidate(&mut clean_endpoint, clean_binding, &mut clean_host);
        assert_eq!(clean_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(
            incremental_prefix,
            query_structural_viewport(&clean_host, clean_source_version, 1)
        );
        assert_eq!(
            incremental_list,
            query_structural_viewport(
                &clean_host,
                clean_source_version,
                second_edit_start + SECOND_INSERTION.len(),
            )
        );
        assert_eq!(
            incremental_suffix,
            query_structural_viewport(&clean_host, clean_source_version, final_suffix_start + 1)
        );

        close_host_to_zero(&mut clean_host);
        close_to_removable(&mut clean_endpoint, clean_binding);
        close_host_to_zero(&mut host);
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    #[ignore = "manual 100,000-item production-shape endpoint scaling gate"]
    fn hundred_thousand_item_list_middle_insertion_is_a_bounded_exact_delta() {
        let receipt = run_bullet_list_local_endpoint_gate(100_000, 42);
        eprintln!("m11_bullet_list_local_endpoint_100k_gate {receipt:?}");
        assert_eq!(receipt.item_count, 100_000);
        assert!(receipt.source_bytes > 2_000_000);
        assert!(
            receipt.source_facts_bytes_examined
                <= 4 * 1024 * SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX + "live ".len()
        );
        assert!(receipt.candidate_transitions <= 200_000);
        assert_eq!(
            receipt.candidate_transitions,
            receipt.candidate_before_begin_transitions + receipt.candidate_after_begin_transitions
        );
        assert_eq!(
            receipt.candidate_transitions,
            receipt.bullet_list_local_transitions
                + receipt.building_exact_transitions
                + receipt.streaming_transitions
        );
        assert!((1..=512).contains(&receipt.bullet_list_local_transitions));
        assert!((1..=64).contains(&receipt.building_exact_transitions));
        assert!((1..=64).contains(&receipt.streaming_transitions));
        assert!(receipt.transferred_records <= 64);
        assert!(receipt.source_facts_replacement_records <= 4);
        assert!(receipt.block_sequence_replacement_records <= 4);
        assert!(receipt.packet_count <= 16);
    }

    struct LargeEndpointHostReceipt {
        first: CandidateDeliveryReceipt,
        replacement: CandidateDeliveryReceipt,
        source_fact_records: u64,
        delete_frame_bytes: usize,
        host_close_polls: usize,
    }

    fn run_large_endpoint_host_replacement(
        source: &str,
        expected_reference_records: u64,
    ) -> LargeEndpointHostReceipt {
        assert_eq!(source.len(), source.encode_utf16().count(), "ASCII fixture");
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4 * 1024));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let base = seed_ascii_pages_and_certify(&mut endpoint, binding, source);
        let first_source = publication_source_version(&endpoint, binding);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(first_source)
            .expect("host observes exact large source");

        let first = deliver_current_candidate(&mut endpoint, binding, &mut host);
        let source_fact_records = host
            .role_record_count(flark_engine::m11_host::M11HostRole::SourceFacts)
            .expect("installed SourceFacts role");
        assert_eq!(first.ack.source_version, first_source);
        assert_eq!(
            first.ack.record_count as u64,
            source_fact_records + expected_reference_records + 3,
            "SourceFacts + Green + Projection + References + CleanEof"
        );
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::References)
                .expect("installed References role"),
            expected_reference_records
        );
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::Green)
                .expect("installed Green role"),
            1
        );
        assert!(first.packet_count > 0);
        assert!(
            first.maximum_encoded_event_bytes
                <= v3_wire::HEADER_BYTES
                    + PUBLICATION_PAYLOAD_PREFIX_BYTES
                    + MAXIMUM_PACKET_ENCODED_BYTES
        );
        assert!(first.maximum_packet_bytes <= MAXIMUM_PACKET_ENCODED_BYTES);
        assert!(first.candidate_transitions >= first.candidate_polls);

        let mut query_output = [0xa5; HOST_M11_VIEWPORT_BYTES];
        let query = host
            .query_structural(
                HostPointQuery {
                    source_version: first_source,
                    position: HostSourceMetric {
                        bytes: u32::try_from(source.len() / 2).expect("source position"),
                        utf16: u32::try_from(source.len() / 2).expect("ASCII source position"),
                    },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: 4 * 1024,
                        maximum_open_depth: 16,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut query_output,
            )
            .expect("bounded structural query");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = query else {
            panic!("installed large source must answer a bounded viewport: {query:?}")
        };
        assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
        assert_eq!(
            receipt.leaf_count, 1,
            "a point query returns only the exact selected structural leaf"
        );
        assert_eq!(
            receipt.tree_nodes_visited, 2,
            "the packed root plus selected leaf are the exact bounded route"
        );

        let empty = known(1, "");
        let edit = edit_frame(
            binding,
            20,
            &[TestIntent {
                sequence: 1,
                base_revision: 0,
                revision: 1,
                base_stamp: base,
                target_stamp: empty,
                operations: vec![TestOperation {
                    start: 0,
                    end: u32::try_from(source.len()).expect("large source UTF-16 length"),
                    replacement: "",
                }],
            }],
        );
        assert!(
            edit.len() < 512,
            "deleting the large source must not copy deleted bytes onto the bridge"
        );
        let delete_frame_bytes = edit.len();
        endpoint.dispatch(&edit).expect("bounded large-source edit");
        acknowledge_source(&mut endpoint, binding, 1);
        assert_eq!(
            poll_to_certification(&mut endpoint),
            CertificationStatus::ExternallyEligible
        );
        let runtime = endpoint.runtime.as_ref().expect("open source runtime");
        assert_eq!(runtime.retired_source_count(), 0);
        assert_eq!(runtime.retired_source_bytes(), 0);

        let second_source = publication_source_version(&endpoint, binding);
        assert_eq!(second_source.revision, 1);
        assert_eq!(second_source.utf8_length, 0);
        host.observe_source_version(second_source)
            .expect("host observes exact replacement source");
        let replacement = deliver_current_candidate(&mut endpoint, binding, &mut host);
        assert_eq!(replacement.ack.source_version, second_source);
        assert_eq!(
            replacement.ack.record_count, 1,
            "empty SourceFacts, block sequence, and References contribute no records; only CleanEof remains"
        );
        assert_ne!(first.source_root, replacement.source_root);
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::References)
                .expect("replacement References role"),
            0
        );

        let host_close_polls = close_host_to_zero(&mut host);
        close_to_removable(&mut endpoint, binding);
        LargeEndpointHostReceipt {
            first,
            replacement,
            source_fact_records,
            delete_frame_bytes,
            host_close_polls,
        }
    }

    fn provisional(revision: u32, utf16_length: u32) -> SourceStamp {
        SourceStamp::Provisional {
            revision,
            utf16_length,
        }
    }

    fn known(revision: u32, source: &str) -> SourceStamp {
        SourceStamp::Known {
            revision,
            utf16_length: source.encode_utf16().count() as u32,
            utf8_length: source.len() as u32,
            content_hash128: rolling_hash(source.as_bytes()),
        }
    }

    fn rolling_hash(source: &[u8]) -> [u32; 4] {
        const BASES: [u32; 4] = [0x0010_0193, 0x9e37_79b1, 0x85eb_ca77, 0xc2b2_ae3d];
        let mut words = [0_u32; 4];
        for byte in source {
            for (word, base) in words.iter_mut().zip(BASES) {
                *word = word.wrapping_mul(base).wrapping_add(u32::from(*byte) + 1);
            }
        }
        words
    }

    fn has_certified_source(endpoint: &Endpoint) -> bool {
        endpoint
            .runtime
            .as_ref()
            .and_then(DocumentRuntime::certified_source)
            .is_some()
    }

    #[test]
    fn invalid_work_profiles_cannot_construct_an_endpoint_that_can_open() {
        let invalid_runtime = DocumentRuntimeConfig {
            max_retired_sources: 0,
            ..DocumentRuntimeConfig::default()
        };
        assert!(matches!(
            EndpointConfig::new(
                invalid_runtime,
                SourceFactsScanProfile::new(4).unwrap(),
                SourceFactsRootLimits::default(),
                ParserProfileId::new(1).unwrap(),
            ),
            Err(EndpointError::InvalidConfig)
        ));
        assert!(SourceFactsScanProfile::new(1).is_err());
        assert!(SourceFactsRootLimits::new(0, 1, 1).is_none());
        assert!(ParserProfileId::new(0).is_none());
    }

    #[test]
    fn fresh_and_recovery_open_are_exact_and_event_credit_replays_one_event() {
        let mut endpoint = Endpoint::fresh(config(4));
        let current = binding(1);
        endpoint
            .dispatch(&open_frame(current, OpenMode::Fresh, 90))
            .unwrap();
        let event = endpoint.outstanding_status().unwrap();
        let mut first = [0; EVENT_BUFFER_BYTES];
        let mut replay = [0; EVENT_BUFFER_BYTES];
        let first_len = endpoint.encode_outstanding_event(&mut first).unwrap();
        let replay_len = endpoint.encode_outstanding_event(&mut replay).unwrap();
        assert_eq!(&first[..first_len], &replay[..replay_len]);
        assert!(matches!(
            endpoint.dispatch(&snapshot_frame(
                current,
                10,
                0,
                0,
                0,
                0,
                0,
                known(0, ""),
                ""
            )),
            Err(EndpointError::EventCreditOccupied { .. })
        ));
        assert!(matches!(
            endpoint.dispatch(&receipt_frame(
                current,
                event.event_id + 1,
                EventDisposition::Accepted,
                None
            )),
            Err(EndpointError::ReceiptMismatch { .. })
        ));
        endpoint
            .dispatch(&receipt_frame(
                current,
                event.event_id,
                EventDisposition::Accepted,
                None,
            ))
            .unwrap();
        assert!(matches!(
            endpoint.dispatch(&receipt_frame(
                current,
                event.event_id,
                EventDisposition::Accepted,
                None
            )),
            Err(EndpointError::NoOutstandingEvent)
        ));

        let mut recovery = Endpoint::recovery(current, config(4)).unwrap();
        assert!(recovery
            .dispatch(&open_frame(current, OpenMode::Fresh, 91))
            .is_err());
        open(&mut recovery, binding(2), OpenMode::Recovery);
    }

    #[test]
    fn unicode_multipage_seed_is_unpublished_until_final_receipt_then_certifies() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(3));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let target = known(7, "a🌍b");
        endpoint
            .dispatch(&snapshot_frame(binding, 10, 0, 1, 4, 7, 6, target, "a"))
            .unwrap();
        assert!(matches!(
            endpoint.status().source,
            EndpointSourceStatus::Seeding { next_utf16: 1, .. }
        ));
        acknowledge_source(&mut endpoint, binding, 0);
        endpoint
            .dispatch(&snapshot_frame(binding, 11, 1, 3, 4, 7, 6, target, "🌍"))
            .unwrap();
        acknowledge_source(&mut endpoint, binding, 0);
        endpoint
            .dispatch(&snapshot_frame(binding, 12, 3, 4, 4, 7, 6, target, "b"))
            .unwrap();
        assert!(matches!(
            endpoint.status().source,
            EndpointSourceStatus::AwaitingInstallReceipt { .. }
        ));
        assert_eq!(endpoint.certification, CertificationStatus::NotStarted);
        acknowledge_source(&mut endpoint, binding, 7);
        assert_eq!(
            poll_to_certification(&mut endpoint),
            CertificationStatus::ExternallyEligible
        );
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn empty_provisional_source_promotes_to_exact_external_authority() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        seed_one_page(&mut endpoint, binding, "", provisional(0, 0), 0);
        assert_eq!(
            poll_to_certification(&mut endpoint),
            CertificationStatus::ExternallyEligible
        );
        assert_eq!(endpoint.installed_target, Some(known(0, "")));
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn same_revision_promotion_refines_provisional_stamp_before_next_edit() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        seed_one_page(&mut endpoint, binding, "a", provisional(0, 1), 0);
        assert_eq!(
            poll_to_certification(&mut endpoint),
            CertificationStatus::ExternallyEligible
        );
        let refined_base = known(0, "a");
        assert_eq!(endpoint.installed_target, Some(refined_base));
        let target = known(1, "ab");
        endpoint
            .dispatch(&edit_frame(
                binding,
                20,
                &[TestIntent {
                    sequence: 1,
                    base_revision: 0,
                    revision: 1,
                    base_stamp: refined_base,
                    target_stamp: target,
                    operations: vec![TestOperation {
                        start: 1,
                        end: 1,
                        replacement: "b",
                    }],
                }],
            ))
            .unwrap();
        acknowledge_source(&mut endpoint, binding, 1);
        assert_eq!(
            poll_to_certification(&mut endpoint),
            CertificationStatus::ExternallyEligible
        );
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn acknowledged_exact_base_uses_a_bounded_incremental_source_facts_handoff() {
        let binding = binding(1);
        let mut source = String::from("[ref]: https://example.com\n");
        source.push_str(&"paragraph using [ref] with stable tail text ".repeat(320));

        let mut endpoint = Endpoint::fresh(config(16));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let base = seed_ascii_pages_and_certify(&mut endpoint, binding, &source);
        let initial_source = publication_source_version(&endpoint, binding);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(initial_source)
            .expect("host observes exact base source");
        let _ = deliver_current_candidate(&mut endpoint, binding, &mut host);
        let base_source = endpoint
            .runtime
            .as_ref()
            .and_then(DocumentRuntime::persistent_source_facts)
            .expect("persistent SourceFacts base")
            .source();
        assert!(endpoint
            .candidate
            .has_exact_base_for(
                endpoint.runtime.as_ref().expect("open runtime"),
                base_source
            )
            .expect("inspect exact base"));

        let edit_start = source
            .rfind("stable")
            .expect("tail edit marker uses an unchanged leading-reference prefix");
        let edit_end = edit_start + "stable".len();
        let mut target_source = source.clone();
        target_source.replace_range(edit_start..edit_end, "updated");
        let target = known(1, &target_source);
        endpoint
            .dispatch(&edit_frame(
                binding,
                20,
                &[TestIntent {
                    sequence: 1,
                    base_revision: 0,
                    revision: 1,
                    base_stamp: base,
                    target_stamp: target,
                    operations: vec![TestOperation {
                        start: u32::try_from(edit_start).expect("ASCII edit start"),
                        end: u32::try_from(edit_end).expect("ASCII edit end"),
                        replacement: "updated",
                    }],
                }],
            ))
            .expect("bounded tail edit");
        acknowledge_source(&mut endpoint, binding, 1);
        let plan = match endpoint
            .active_source_facts
            .as_ref()
            .expect("edit starts SourceFacts")
        {
            ActiveSourceFacts::Incremental(plan) => plan.clone(),
            ActiveSourceFacts::Clean => panic!("acknowledged exact base must not rescan clean"),
        };
        assert_eq!(
            plan.source(),
            endpoint
                .runtime
                .as_ref()
                .and_then(DocumentRuntime::current_source_version)
                .expect("target source")
        );
        assert!(
            plan.target_byte_range().end - plan.target_byte_range().start < target_source.len(),
            "the incremental scan must crop the exact target"
        );

        let scanned = poll_to_incremental_handoff(&mut endpoint);
        assert!(
            scanned < target_source.len(),
            "the live path must not inspect the whole target source"
        );
        let publication = endpoint
            .pending_incremental_publication
            .as_ref()
            .expect("move-only exact handoff");
        let handoff = &publication.candidate;
        assert_eq!(handoff.target_source.version(), plan.source());
        assert_eq!(handoff.target_facts.source(), plan.source());
        assert_eq!(handoff.source_facts_delta.base(), plan.base());
        assert_eq!(handoff.source_facts_delta.target(), plan.source());
        assert_eq!(
            handoff.source_facts_delta.base_page_range(),
            plan.base_page_range()
        );
        let SourceStamp::Known {
            content_hash128, ..
        } = target
        else {
            panic!("test target is exact")
        };
        assert_eq!(
            handoff.target_facts.summary().rolling_hash().words(),
            content_hash128
        );
        assert_eq!(
            endpoint.outstanding_status().unwrap().kind,
            EndpointEventKind::SourceFactsDeltaBegin
        );
        let proof = accept_incremental_certification(&mut endpoint, binding);
        assert_eq!(proof.ui_revision, 1);
        assert_eq!(
            endpoint.certification,
            CertificationStatus::ExternallyEligible
        );
        assert!(endpoint.pending_incremental_publication.is_none());

        host.observe_source_version(publication_source_version(&endpoint, binding))
            .expect("host observes exact target source");
        let delivered = deliver_current_candidate(&mut endpoint, binding, &mut host);
        assert_eq!(delivered.ack.source_version.revision, 1);
        let committed_target = endpoint
            .runtime
            .as_ref()
            .and_then(DocumentRuntime::current_source_version)
            .expect("committed target source");
        assert!(
            !endpoint
                .runtime
                .as_mut()
                .expect("open runtime")
                .commit_persistent_source_facts_delta(committed_target)
                .expect("delivery already committed the SourceFacts transaction"),
            "host commit dispatch must retire the old persistent SourceFacts base"
        );
        close_host_to_zero(&mut host);
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn ordinary_paragraph_middle_edit_uses_bounded_checkpoint_convergence() {
        let binding = binding(1);
        let source: String = (0..1_024)
            .map(|ordinal| format!("ordinary prose line {ordinal:04} {}\n", "a".repeat(40)))
            .collect();
        let mut endpoint = Endpoint::fresh(config(16));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let base = seed_ascii_pages_and_certify(&mut endpoint, binding, &source);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(publication_source_version(&endpoint, binding))
            .expect("host observes ordinary base source");
        let base_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);

        let edit_start = source
            .find("ordinary prose line 0512 ")
            .expect("middle line")
            + "ordinary prose line 0512 ".len()
            + 20;
        let mut target_source = source.clone();
        target_source.replace_range(edit_start..edit_start + 1, "Z");
        let target = known(1, &target_source);
        endpoint
            .dispatch(&edit_frame(
                binding,
                20,
                &[TestIntent {
                    sequence: 1,
                    base_revision: 0,
                    revision: 1,
                    base_stamp: base,
                    target_stamp: target,
                    operations: vec![TestOperation {
                        start: u32::try_from(edit_start).expect("ASCII edit start"),
                        end: u32::try_from(edit_start + 1).expect("ASCII edit end"),
                        replacement: "Z",
                    }],
                }],
            ))
            .expect("middle ordinary edit");
        acknowledge_source(&mut endpoint, binding, 1);
        let plan = match endpoint
            .active_source_facts
            .as_ref()
            .expect("edit starts SourceFacts")
        {
            ActiveSourceFacts::Incremental(plan) => plan.clone(),
            ActiveSourceFacts::Clean => {
                panic!("middle ordinary edit must select authenticated convergence")
            }
        };
        assert!(
            plan.base_byte_range().start > 0
                && plan.base_byte_range().end < source.len()
                && plan.target_byte_range().end - plan.target_byte_range().start
                    < target_source.len()
        );
        let scanned = poll_to_incremental_handoff(&mut endpoint);
        assert!(
            scanned < target_source.len(),
            "SourceFacts and parser work must remain cropped"
        );
        accept_incremental_certification(&mut endpoint, binding);

        host.observe_source_version(publication_source_version(&endpoint, binding))
            .expect("host observes ordinary target source");
        let target_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);
        assert_eq!(target_delivery.ack.source_version.revision, 1);
        assert_ne!(
            target_delivery.ack.publication_session,
            base_delivery.ack.publication_session
        );
        let (checkpoint_source, checkpoint_count) = endpoint
            .candidate
            .retained_ordinary_restart_receipt()
            .expect("ordinary target restart authority");
        assert_eq!(
            checkpoint_source,
            endpoint
                .runtime
                .as_ref()
                .and_then(DocumentRuntime::current_source_version)
                .expect("current target source")
        );
        assert!(checkpoint_count > 2);

        close_host_to_zero(&mut host);
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn ordinary_paragraph_bof_edit_uses_incremental_source_facts_and_delivers() {
        let binding = binding(1);
        let source: String = (0..1_024)
            .map(|ordinal| format!("ordinary prose line {ordinal:04} {}\n", "a".repeat(40)))
            .collect();
        let mut endpoint = Endpoint::fresh(config(16));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let base = seed_ascii_pages_and_certify(&mut endpoint, binding, &source);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(publication_source_version(&endpoint, binding))
            .expect("host observes ordinary BOF base source");
        let base_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);

        assert_eq!(&source[..1], "o");
        let mut target_source = source.clone();
        target_source.replace_range(0..1, "O");
        let target = known(1, &target_source);
        endpoint
            .dispatch(&edit_frame(
                binding,
                20,
                &[TestIntent {
                    sequence: 1,
                    base_revision: 0,
                    revision: 1,
                    base_stamp: base,
                    target_stamp: target,
                    operations: vec![TestOperation {
                        start: 0,
                        end: 1,
                        replacement: "O",
                    }],
                }],
            ))
            .expect("ordinary BOF edit");
        acknowledge_source(&mut endpoint, binding, 1);
        let plan = match endpoint
            .active_source_facts
            .as_ref()
            .expect("BOF edit starts SourceFacts")
        {
            ActiveSourceFacts::Incremental(plan) => plan.clone(),
            ActiveSourceFacts::Clean => {
                panic!("BOF ordinary edit must select incremental SourceFacts")
            }
        };
        assert_eq!(plan.base_byte_range().start, 0);
        assert!(plan.base_byte_range().end < source.len());
        assert_eq!(plan.target_byte_range().start, 0);
        assert!(plan.target_byte_range().end < target_source.len());

        let scanned = poll_to_incremental_handoff(&mut endpoint);
        assert!(
            scanned < target_source.len(),
            "BOF SourceFacts work must remain below whole-document work"
        );
        accept_incremental_certification(&mut endpoint, binding);

        host.observe_source_version(publication_source_version(&endpoint, binding))
            .expect("host observes ordinary BOF target source");
        let target_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);
        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
        assert!(
            target_delivery.offer.transferred_record_count
                < target_delivery.offer.target_record_count
        );
        assert_eq!(target_delivery.ack.source_version.revision, 1);
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Open);
        assert!(!endpoint.runtime_poisoned);
        assert!(!endpoint.needs_reseed);
        let (checkpoint_source, checkpoint_count) = endpoint
            .candidate
            .retained_ordinary_restart_receipt()
            .expect("BOF target restart authority");
        assert_eq!(
            checkpoint_source,
            endpoint
                .runtime
                .as_ref()
                .and_then(DocumentRuntime::current_source_version)
                .expect("ordinary BOF target")
        );
        assert!(checkpoint_count > 2);

        close_host_to_zero(&mut host);
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn ordinary_paragraph_eof_edit_uses_incremental_source_facts_and_delivers() {
        let binding = binding(1);
        let source: String = (0..1_024)
            .map(|ordinal| format!("ordinary prose line {ordinal:04} {}\n", "a".repeat(40)))
            .collect();
        let mut endpoint = Endpoint::fresh(config(16));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let base = seed_ascii_pages_and_certify(&mut endpoint, binding, &source);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(publication_source_version(&endpoint, binding))
            .expect("host observes ordinary EOF base source");
        let base_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);

        let edit_start = source.rfind('a').expect("last ordinary payload byte");
        let mut target_source = source.clone();
        target_source.replace_range(edit_start..edit_start + 1, "Z");
        let target = known(1, &target_source);
        endpoint
            .dispatch(&edit_frame(
                binding,
                20,
                &[TestIntent {
                    sequence: 1,
                    base_revision: 0,
                    revision: 1,
                    base_stamp: base,
                    target_stamp: target,
                    operations: vec![TestOperation {
                        start: u32::try_from(edit_start).expect("ASCII EOF edit start"),
                        end: u32::try_from(edit_start + 1).expect("ASCII EOF edit end"),
                        replacement: "Z",
                    }],
                }],
            ))
            .expect("ordinary EOF edit");
        acknowledge_source(&mut endpoint, binding, 1);
        let plan = match endpoint
            .active_source_facts
            .as_ref()
            .expect("EOF edit starts SourceFacts")
        {
            ActiveSourceFacts::Incremental(plan) => plan.clone(),
            ActiveSourceFacts::Clean => {
                panic!("EOF ordinary edit must select incremental SourceFacts")
            }
        };
        assert!(plan.base_byte_range().start > 0);
        assert_eq!(plan.base_byte_range().end, source.len());
        assert!(plan.target_byte_range().start > 0);
        assert_eq!(plan.target_byte_range().end, target_source.len());

        let scanned = poll_to_incremental_handoff(&mut endpoint);
        assert!(
            scanned < target_source.len(),
            "EOF SourceFacts work must remain below whole-document work"
        );
        accept_incremental_certification(&mut endpoint, binding);

        host.observe_source_version(publication_source_version(&endpoint, binding))
            .expect("host observes ordinary EOF target source");
        let target_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);
        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
        assert!(
            target_delivery.offer.transferred_record_count
                < target_delivery.offer.target_record_count
        );
        assert_eq!(target_delivery.ack.source_version.revision, 1);
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Open);
        assert!(!endpoint.runtime_poisoned);
        assert!(!endpoint.needs_reseed);
        let (checkpoint_source, checkpoint_count) = endpoint
            .candidate
            .retained_ordinary_restart_receipt()
            .expect("EOF target restart authority");
        assert_eq!(
            checkpoint_source,
            endpoint
                .runtime
                .as_ref()
                .and_then(DocumentRuntime::current_source_version)
                .expect("ordinary EOF target")
        );
        assert!(checkpoint_count > 2);

        close_host_to_zero(&mut host);
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn new_leading_definition_declines_crop_into_clean_full_snapshot() {
        let binding = binding(1);
        let source = "[base]: /base\n!x]: /new\nvisible\n";
        let mut endpoint = Endpoint::fresh(config(2));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let base = seed_ascii_pages_and_certify(&mut endpoint, binding, source);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(publication_source_version(&endpoint, binding))
            .expect("host observes leading-reference base source");
        let base_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::References)
                .expect("base References"),
            1
        );

        let edit_start = source.find("!x]: /new").expect("definition-shaped tail");
        let mut target_source = source.to_owned();
        target_source.replace_range(edit_start..edit_start + 1, "[");
        let target = known(1, &target_source);
        endpoint
            .dispatch(&edit_frame(
                binding,
                20,
                &[TestIntent {
                    sequence: 1,
                    base_revision: 0,
                    revision: 1,
                    base_stamp: base,
                    target_stamp: target,
                    operations: vec![TestOperation {
                        start: u32::try_from(edit_start).expect("ASCII definition edit start"),
                        end: u32::try_from(edit_start + 1).expect("ASCII definition edit end"),
                        replacement: "[",
                    }],
                }],
            ))
            .expect("one-byte leading-definition edit");
        acknowledge_source(&mut endpoint, binding, 1);
        assert!(
            matches!(
                endpoint.active_source_facts,
                Some(ActiveSourceFacts::Incremental(_))
            ),
            "the unchanged leading prefix must first select incremental SourceFacts"
        );

        poll_to_incremental_handoff(&mut endpoint);
        let proof = accept_incremental_certification(&mut endpoint, binding);
        assert_eq!(proof.ui_revision, 1);
        assert_eq!(
            endpoint.certification,
            CertificationStatus::ExternallyEligible
        );
        assert!(endpoint.active_source_facts.is_none());
        assert!(endpoint.pending_certification.is_none());
        assert!(endpoint.pending_incremental_publication.is_none());
        assert_eq!(endpoint.active_candidate_certification, Some(proof));
        assert_eq!(
            endpoint
                .last_external_certification
                .expect("committed base proof remains authoritative")
                .ui_revision,
            0
        );

        let target_source_version = publication_source_version(&endpoint, binding);
        host.observe_source_version(target_source_version)
            .expect("host observes clean-fallback target source");
        let target_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);
        assert_eq!(target_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(target_delivery.offer.base_ack, None);
        assert_eq!(
            target_delivery.offer.transferred_record_count,
            target_delivery.offer.target_record_count
        );
        assert_eq!(target_delivery.ack.source_version, target_source_version);
        assert_ne!(
            target_delivery.ack.publication_session,
            base_delivery.ack.publication_session
        );
        assert_eq!(
            host.role_record_count(flark_engine::m11_host::M11HostRole::References)
                .expect("fresh target References"),
            2
        );
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Open);
        assert!(!endpoint.runtime_poisoned);
        assert!(!endpoint.needs_reseed);
        assert!(!endpoint.failure_emitted);
        assert!(endpoint.active_source_facts.is_none());
        assert!(endpoint.pending_certification.is_none());
        assert!(endpoint.pending_incremental_publication.is_none());
        assert_eq!(endpoint.last_external_certification, Some(proof));
        let runtime = endpoint.runtime.as_ref().expect("open fallback runtime");
        let target_runtime_source = runtime
            .current_source_version()
            .expect("clean-fallback target source");
        assert!(endpoint
            .candidate
            .has_exact_base_for(runtime, target_runtime_source)
            .expect("clean fallback retained next-revision authority"));

        close_host_to_zero(&mut host);
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn whole_source_change_selects_clean_source_facts_before_any_delta() {
        let binding = binding(1);
        let source: String = (0..96)
            .map(|ordinal| format!("base ordinary line {ordinal:04} {}\n", "a".repeat(40)))
            .collect();
        let target_source: String = (0..96)
            .map(|ordinal| format!("replacement line {ordinal:04} {}\n", "b".repeat(44)))
            .collect();
        let mut endpoint = Endpoint::fresh(config(16));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let base = seed_ascii_pages_and_certify(&mut endpoint, binding, &source);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(publication_source_version(&endpoint, binding))
            .expect("host observes whole-change base source");
        let base_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);

        let target = known(1, &target_source);
        endpoint
            .dispatch(&edit_frame(
                binding,
                20,
                &[TestIntent {
                    sequence: 1,
                    base_revision: 0,
                    revision: 1,
                    base_stamp: base,
                    target_stamp: target,
                    operations: vec![TestOperation {
                        start: 0,
                        end: u32::try_from(source.len()).expect("ASCII whole-source end"),
                        replacement: &target_source,
                    }],
                }],
            ))
            .expect("whole-source edit");
        acknowledge_source(&mut endpoint, binding, 1);
        assert!(
            matches!(endpoint.active_source_facts, Some(ActiveSourceFacts::Clean)),
            "whole-source parser range must route clean before delta publication"
        );
        assert!(
            !endpoint
                .candidate
                .has_bullet_list_local_edit_plan_for_test(),
            "a clean SourceFacts lane must not retain local edit authority"
        );
        assert!(endpoint.pending_incremental_publication.is_none());
        assert_eq!(endpoint.outstanding_status(), None);

        assert_eq!(
            poll_to_certification(&mut endpoint),
            CertificationStatus::ExternallyEligible
        );
        assert!(endpoint.active_source_facts.is_none());
        assert!(endpoint.pending_incremental_publication.is_none());

        let target_source_version = publication_source_version(&endpoint, binding);
        host.observe_source_version(target_source_version)
            .expect("host observes whole-change clean target");
        let target_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);
        assert_eq!(target_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(target_delivery.offer.base_ack, None);
        assert_eq!(
            target_delivery.offer.transferred_record_count,
            target_delivery.offer.target_record_count
        );
        assert_eq!(target_delivery.ack.source_version, target_source_version);
        assert_ne!(
            target_delivery.ack.publication_session,
            base_delivery.ack.publication_session
        );
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Open);
        assert!(!endpoint.runtime_poisoned);
        assert!(!endpoint.needs_reseed);

        close_host_to_zero(&mut host);
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn edit_crossing_parser_restart_prefix_selects_clean_source_facts_before_delta() {
        let binding = binding(1);
        let source = "[shared]: /target\nstable\n";
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let base = seed_ascii_pages_and_certify(&mut endpoint, binding, source);
        let base_source = publication_source_version(&endpoint, binding);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(base_source)
            .expect("host observes exact base source");
        let _ = deliver_current_candidate(&mut endpoint, binding, &mut host);

        let target_source = "[changed]: /target\nstable\n";
        let target = known(1, target_source);
        endpoint
            .dispatch(&edit_frame(
                binding,
                20,
                &[TestIntent {
                    sequence: 1,
                    base_revision: 0,
                    revision: 1,
                    base_stamp: base,
                    target_stamp: target,
                    operations: vec![TestOperation {
                        start: 1,
                        end: 7,
                        replacement: "changed",
                    }],
                }],
            ))
            .expect("bounded prefix edit");
        acknowledge_source(&mut endpoint, binding, 1);
        assert!(
            matches!(endpoint.active_source_facts, Some(ActiveSourceFacts::Clean)),
            "a crossed parser prefix must route clean before producing a SourceFacts delta"
        );
        assert!(endpoint.pending_incremental_publication.is_none());
        assert_eq!(
            poll_to_certification(&mut endpoint),
            CertificationStatus::ExternallyEligible
        );

        let target_source_version = publication_source_version(&endpoint, binding);
        host.observe_source_version(target_source_version)
            .expect("host observes exact clean target");
        let delivered = deliver_current_candidate(&mut endpoint, binding, &mut host);
        assert_eq!(delivered.ack.source_version, target_source_version);
        close_host_to_zero(&mut host);
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn canonical_pages_replay_exactly_and_hold_the_single_event_credit() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(2));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let source = "x".repeat(130);
        let target = known(0, &source);
        seed_one_page(&mut endpoint, binding, &source, target, 0);

        let first = poll_to_first_certification_event(&mut endpoint);
        assert_eq!(first.kind, EndpointEventKind::SourceFactsPage);
        let mut encoded = [0; EVENT_BUFFER_BYTES];
        let mut replay = [0; EVENT_BUFFER_BYTES];
        let encoded_len = endpoint.encode_outstanding_event(&mut encoded).unwrap();
        let replay_len = endpoint.encode_outstanding_event(&mut replay).unwrap();
        assert_eq!(&encoded[..encoded_len], &replay[..replay_len]);
        assert!(matches!(
            endpoint.dispatch(&supersede_frame(binding, 99, 1)),
            Err(EndpointError::EventCreditOccupied { event_id }) if event_id == first.event_id
        ));

        endpoint
            .dispatch(&receipt_frame(
                binding,
                first.event_id,
                EventDisposition::Accepted,
                None,
            ))
            .unwrap();
        let second = endpoint.outstanding_status().unwrap();
        assert_eq!(second.kind, EndpointEventKind::SourceFactsPage);
        assert_ne!(second.event_id, first.event_id);
        endpoint
            .dispatch(&receipt_frame(
                binding,
                second.event_id,
                EventDisposition::Accepted,
                None,
            ))
            .unwrap();
        let terminal = endpoint.outstanding_status().unwrap();
        assert_eq!(terminal.kind, EndpointEventKind::SourceFactsCompleted);
        let EventBody::SourceFactsCompleted(completion) = endpoint
            .outstanding
            .as_ref()
            .expect("terminal event remains credited")
            .session_body()
            .expect("terminal session event")
        else {
            panic!("expected canonical completion event");
        };
        assert_eq!(completion.page_count, 2);
        endpoint
            .dispatch(&certification_receipt_frame(
                binding,
                terminal.event_id,
                completion.into(),
            ))
            .unwrap();
        assert_eq!(
            endpoint.certification,
            CertificationStatus::ExternallyEligible
        );
        assert_eq!(endpoint.installed_target, Some(target));
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn stale_page_discards_only_derived_facts_and_keeps_installed_source() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(2));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let source = "source facts";
        let target = known(0, source);
        seed_one_page(&mut endpoint, binding, source, target, 0);
        let page = poll_to_first_certification_event(&mut endpoint);
        assert_eq!(page.kind, EndpointEventKind::SourceFactsPage);
        endpoint
            .dispatch(&receipt_frame(
                binding,
                page.event_id,
                EventDisposition::Stale,
                None,
            ))
            .unwrap();
        assert_eq!(endpoint.certification, CertificationStatus::NotStarted);
        assert_eq!(endpoint.installed_target, Some(target));
        assert_eq!(endpoint.observed.unwrap().revision, 0);
        assert!(!has_certified_source(&endpoint));
        assert!(endpoint.outstanding.is_none());
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn crossed_terminal_promotion_proof_fails_closed_without_authority_refinement() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let provisional = provisional(0, 3);
        seed_one_page(&mut endpoint, binding, "abc", provisional, 0);
        let _ = poll_to_first_certification_event(&mut endpoint);
        while endpoint.outstanding_status().unwrap().kind == EndpointEventKind::SourceFactsPage {
            let page = endpoint.outstanding_status().unwrap();
            endpoint
                .dispatch(&receipt_frame(
                    binding,
                    page.event_id,
                    EventDisposition::Accepted,
                    None,
                ))
                .unwrap();
        }
        let terminal = endpoint.outstanding_status().unwrap();
        let EventBody::SourceFactsCompleted(completion) = endpoint
            .outstanding
            .as_ref()
            .expect("completion retains global credit")
            .session_body()
            .expect("completion session event")
        else {
            panic!("expected completion");
        };
        let mut crossed: SourceCertificationReceipt = completion.into();
        crossed.checkpoint_hash128[0] ^= 1;
        assert!(matches!(
            endpoint.dispatch(&certification_receipt_frame(
                binding,
                terminal.event_id,
                crossed,
            )),
            Err(EndpointError::InvalidReceipt)
        ));
        assert_ne!(endpoint.installed_target, Some(known(0, "abc")));
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Faulted);
        assert_eq!(
            endpoint.outstanding_status().unwrap().kind,
            EndpointEventKind::Failed
        );
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn invalid_scan_fuel_is_rejected_without_poisoning_or_advancing_the_job() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        seed_one_page(&mut endpoint, binding, "abcdef", known(0, "abcdef"), 0);
        for fuel in [
            EndpointPollFuel {
                maximum_source_bytes: 0,
                maximum_checkpoints: 1,
                maximum_retirement_transitions: 0,
            },
            EndpointPollFuel {
                maximum_source_bytes: SOURCE_FACT_SOURCE_BYTES_PER_POLL_MAX + 1,
                maximum_checkpoints: 1,
                maximum_retirement_transitions: 0,
            },
            EndpointPollFuel {
                maximum_source_bytes: 1,
                maximum_checkpoints: SOURCE_FACT_CHECKPOINTS_PER_PAGE_MAX + 1,
                maximum_retirement_transitions: 0,
            },
        ] {
            assert!(matches!(
                endpoint.poll_source_facts(fuel),
                Err(EndpointError::InvalidPollFuel)
            ));
            assert_eq!(endpoint.certification, CertificationStatus::Scanning);
            assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Open);
            assert!(!endpoint.runtime_poisoned);
        }
        assert_eq!(
            poll_to_certification(&mut endpoint),
            CertificationStatus::ExternallyEligible
        );
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn persistent_promotion_poll_reports_scheduler_progress() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        seed_one_page(
            &mut endpoint,
            binding,
            "promotion progress",
            known(0, "promotion progress"),
            0,
        );

        let mut transition_only_polls = Vec::new();
        for _ in 0..128 {
            let receipt = endpoint
                .poll_source_facts(EndpointPollFuel {
                    maximum_source_bytes: 64,
                    maximum_checkpoints: 8,
                    maximum_retirement_transitions: 0,
                })
                .expect("bounded source-facts poll");
            if receipt.source_fact_transitions != 0 {
                transition_only_polls.push(receipt);
                if transition_only_polls.len() == 2 {
                    break;
                }
            }
        }

        assert_eq!(
            transition_only_polls.len(),
            2,
            "scanner completion and the first promotion poll must both remain scheduler-visible"
        );
        for transition in transition_only_polls {
            assert_eq!(transition.source_fact_transitions, 1);
            assert_eq!(transition.source_bytes_examined, 0);
            assert_eq!(transition.source_bytes_buffered, 0);
            assert_eq!(transition.checkpoints_emitted, 0);
            assert!(!transition.scan_complete);
        }
        assert_eq!(endpoint.certification, CertificationStatus::Scanning);
        assert_eq!(
            poll_to_certification(&mut endpoint),
            CertificationStatus::ExternallyEligible
        );
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn known_hash_mismatch_never_becomes_certified_and_fails_once() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let mut bad = known(0, "abc");
        if let SourceStamp::Known {
            ref mut content_hash128,
            ..
        } = bad
        {
            content_hash128[0] ^= 1;
        }
        seed_one_page(&mut endpoint, binding, "abc", bad, 0);
        let mut mismatch = None;
        for _ in 0..3 {
            let result = endpoint.poll_source_facts(EndpointPollFuel {
                maximum_source_bytes: 64,
                maximum_checkpoints: 8,
                maximum_retirement_transitions: 1,
            });
            if result.is_err() {
                mismatch = Some(result);
                break;
            }
        }
        assert!(matches!(mismatch, Some(Err(EndpointError::SourceFacts))));
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Faulted);
        assert_eq!(
            endpoint.outstanding_status().unwrap().kind,
            EndpointEventKind::Failed
        );
        assert!(!has_certified_source(&endpoint));
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn reordered_seed_page_fails_closed_without_acknowledging_a_replica() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let target = known(0, "abcd");
        endpoint
            .dispatch(&snapshot_frame(binding, 10, 0, 2, 4, 0, 0, target, "ab"))
            .unwrap();
        acknowledge_source(&mut endpoint, binding, 0);
        assert!(matches!(
            endpoint.dispatch(&snapshot_frame(binding, 11, 3, 4, 4, 0, 0, target, "d")),
            Err(EndpointError::InvalidSeed)
        ));
        assert_eq!(endpoint.status().source, EndpointSourceStatus::NeedsReseed);
        assert_eq!(
            endpoint.outstanding_status().unwrap().kind,
            EndpointEventKind::Failed
        );
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn rejected_final_seed_receipt_discards_the_unpublished_root_and_requires_reseed() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        endpoint
            .dispatch(&snapshot_frame(
                binding,
                10,
                0,
                1,
                1,
                0,
                0,
                known(0, "a"),
                "a",
            ))
            .unwrap();
        let event = endpoint.outstanding_status().unwrap();
        assert!(endpoint
            .dispatch(&receipt_frame(
                binding,
                event.event_id,
                EventDisposition::Stale,
                Some(SourceReceipt {
                    disposition: SourceReceiptDisposition::Stale,
                    dropped_intent_entries: 0,
                    dropped_payload_utf16: 0,
                    dropped_deleted_utf16: 0,
                    dropped_operation_count: 0,
                    worker_revision: 0,
                })
            ))
            .is_err());
        assert_eq!(endpoint.status().source, EndpointSourceStatus::NeedsReseed);
        assert_eq!(
            endpoint.outstanding_status().unwrap().kind,
            EndpointEventKind::Failed
        );
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn malformed_seed_frame_poisoning_does_not_reparse_the_wire() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let mut malformed = snapshot_frame(binding, 10, 0, 1, 1, 0, 0, known(0, "a"), "a");
        malformed.pop();
        assert!(matches!(
            endpoint.dispatch(&malformed),
            Err(EndpointError::Decode(_))
        ));
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Faulted);
        assert_eq!(
            endpoint.outstanding_status().unwrap().kind,
            EndpointEventKind::Failed
        );
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn same_offset_edit_preserves_order_and_only_installs_after_ack() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(3));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let base = known(0, "x");
        seed_one_page(&mut endpoint, binding, "x", base, 0);
        let target = known(1, "ABx");
        endpoint
            .dispatch(&edit_frame(
                binding,
                20,
                &[TestIntent {
                    sequence: 1,
                    base_revision: 0,
                    revision: 1,
                    base_stamp: base,
                    target_stamp: target,
                    operations: vec![
                        TestOperation {
                            start: 0,
                            end: 0,
                            replacement: "A",
                        },
                        TestOperation {
                            start: 0,
                            end: 0,
                            replacement: "B",
                        },
                    ],
                }],
            ))
            .unwrap();
        assert!(matches!(
            endpoint.status().source,
            EndpointSourceStatus::AwaitingInstallReceipt { target: value, .. } if value == target
        ));
        acknowledge_source(&mut endpoint, binding, 1);
        assert_eq!(
            poll_to_certification(&mut endpoint),
            CertificationStatus::ExternallyEligible
        );
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn edit_ack_requires_the_exact_dart_journal_drop_receipt() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let base = known(0, "a");
        seed_one_page(&mut endpoint, binding, "a", base, 0);
        let target = known(1, "ab");
        endpoint
            .dispatch(&edit_frame(
                binding,
                20,
                &[TestIntent {
                    sequence: 1,
                    base_revision: 0,
                    revision: 1,
                    base_stamp: base,
                    target_stamp: target,
                    operations: vec![TestOperation {
                        start: 1,
                        end: 1,
                        replacement: "b",
                    }],
                }],
            ))
            .unwrap();
        let event = endpoint.outstanding_status().unwrap();
        assert!(endpoint
            .dispatch(&receipt_frame(
                binding,
                event.event_id,
                EventDisposition::Accepted,
                Some(accepted_source(1)),
            ))
            .is_err());
        assert_eq!(endpoint.status().source, EndpointSourceStatus::NeedsReseed);
        assert_eq!(
            endpoint.outstanding_status().unwrap().kind,
            EndpointEventKind::Failed
        );
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn bounded_batch_can_exceed_cross_command_retirement_backlog_without_leaking_credit() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let initial = known(0, "");
        seed_one_page(&mut endpoint, binding, "", initial, 0);

        let mut intents = Vec::new();
        let mut source = String::new();
        let mut base_stamp = initial;
        for revision in 1..=16 {
            let start = source.encode_utf16().count() as u32;
            source.push('x');
            let target_stamp = known(revision, &source);
            intents.push(TestIntent {
                sequence: revision,
                base_revision: revision - 1,
                revision,
                base_stamp,
                target_stamp,
                operations: vec![TestOperation {
                    start,
                    end: start,
                    replacement: "x",
                }],
            });
            base_stamp = target_stamp;
        }
        endpoint
            .dispatch(&edit_frame(binding, 20, &intents))
            .unwrap();
        assert!(matches!(
            endpoint.status().source,
            EndpointSourceStatus::AwaitingInstallReceipt {
                observed: ObservedSourceReplicaVersion {
                    revision: 16,
                    intent_high_water: 16,
                    ..
                },
                ..
            }
        ));
        acknowledge_source(&mut endpoint, binding, 16);
        assert_eq!(
            poll_to_certification(&mut endpoint),
            CertificationStatus::ExternallyEligible
        );
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn surrogate_split_and_stale_stamp_emit_no_source_ack_or_certification() {
        for stale in [false, true] {
            let binding = binding(1);
            let mut endpoint = Endpoint::fresh(config(4));
            open(&mut endpoint, binding, OpenMode::Fresh);
            let base = known(0, "🌍");
            seed_one_page(&mut endpoint, binding, "🌍", base, 0);
            let declared_base = if stale { provisional(0, 2) } else { base };
            let target = known(1, if stale { "🌍x" } else { "x🌍" });
            let operation = if stale {
                TestOperation {
                    start: 2,
                    end: 2,
                    replacement: "x",
                }
            } else {
                TestOperation {
                    start: 1,
                    end: 1,
                    replacement: "x",
                }
            };
            assert!(matches!(
                endpoint.dispatch(&edit_frame(
                    binding,
                    20,
                    &[TestIntent {
                        sequence: 1,
                        base_revision: 0,
                        revision: 1,
                        base_stamp: declared_base,
                        target_stamp: target,
                        operations: vec![operation],
                    }]
                )),
                Err(EndpointError::InvalidEdit)
            ));
            assert_eq!(
                endpoint.outstanding_status().unwrap().kind,
                EndpointEventKind::Failed
            );
            assert!(!has_certified_source(&endpoint));
            close_to_removable(&mut endpoint, binding);
        }
    }

    #[test]
    fn later_intent_failure_quarantines_partial_internal_commits_until_bounded_cleanup() {
        let current = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, current, OpenMode::Fresh);
        let base = known(0, "a");
        seed_one_page(&mut endpoint, current, "a", base, 0);
        let after_first = known(1, "ab");
        let after_second = known(2, "ab");
        let intents = [
            TestIntent {
                sequence: 1,
                base_revision: 0,
                revision: 1,
                base_stamp: base,
                target_stamp: after_first,
                operations: vec![TestOperation {
                    start: 1,
                    end: 1,
                    replacement: "b",
                }],
            },
            TestIntent {
                sequence: 2,
                base_revision: 1,
                revision: 2,
                base_stamp: after_first,
                target_stamp: after_second,
                operations: vec![TestOperation {
                    start: 3,
                    end: 3,
                    replacement: "",
                }],
            },
        ];
        assert!(matches!(
            endpoint.dispatch(&edit_frame(current, 20, &intents)),
            Err(EndpointError::InvalidEdit)
        ));
        assert_eq!(endpoint.status().source, EndpointSourceStatus::NeedsReseed);
        assert!(!has_certified_source(&endpoint));
        let event = endpoint.outstanding_status().unwrap();
        assert_eq!(event.kind, EndpointEventKind::Failed);
        endpoint
            .dispatch(&receipt_frame(
                current,
                event.event_id,
                EventDisposition::Accepted,
                None,
            ))
            .unwrap();
        let cleanup = endpoint
            .poll_source_facts(EndpointPollFuel {
                maximum_source_bytes: 1,
                maximum_checkpoints: 1,
                maximum_retirement_transitions: 1,
            })
            .unwrap();
        assert!(cleanup.released_source_leases <= 1);
        while endpoint.runtime.is_some() {
            endpoint
                .poll_source_facts(EndpointPollFuel {
                    maximum_source_bytes: 1,
                    maximum_checkpoints: 1,
                    maximum_retirement_transitions: 1,
                })
                .unwrap();
        }
        let next = binding(2);
        open(&mut endpoint, next, OpenMode::Recovery);
        close_to_removable(&mut endpoint, next);
    }

    #[test]
    fn supersede_cancels_certification_but_retains_exact_installed_source() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let target = known(0, "source");
        seed_one_page(&mut endpoint, binding, "source", target, 0);
        assert_eq!(
            poll_to_certification(&mut endpoint),
            CertificationStatus::ExternallyEligible
        );
        let observed_before = endpoint.observed;
        endpoint.dispatch(&supersede_frame(binding, 77, 1)).unwrap();
        assert_eq!(endpoint.observed, observed_before);
        assert!(!has_certified_source(&endpoint));
        assert!(matches!(
            endpoint.status().source,
            EndpointSourceStatus::Installed {
                target: value,
                certification: CertificationStatus::NotStarted,
                ..
            } if value == target
        ));
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn close_latches_over_open_or_source_credit_and_closed_receipt_gates_removal() {
        for phase in 0..2 {
            let binding = binding(1);
            let mut endpoint = Endpoint::fresh(config(4));
            endpoint
                .dispatch(&open_frame(binding, OpenMode::Fresh, 1))
                .unwrap();
            if phase == 1 {
                let opened = endpoint.outstanding_status().unwrap();
                endpoint
                    .dispatch(&receipt_frame(
                        binding,
                        opened.event_id,
                        EventDisposition::Accepted,
                        None,
                    ))
                    .unwrap();
                endpoint
                    .dispatch(&snapshot_frame(
                        binding,
                        10,
                        0,
                        1,
                        2,
                        0,
                        0,
                        known(0, "ab"),
                        "a",
                    ))
                    .unwrap();
            }
            let outstanding = endpoint.outstanding_status().unwrap();
            endpoint.dispatch(&close_frame(binding, 2)).unwrap();
            assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Closing);
            endpoint
                .dispatch(&receipt_frame(
                    binding,
                    outstanding.event_id,
                    EventDisposition::Stale,
                    if phase == 1 {
                        Some(SourceReceipt {
                            disposition: SourceReceiptDisposition::Stale,
                            dropped_intent_entries: 0,
                            dropped_payload_utf16: 0,
                            dropped_deleted_utf16: 0,
                            dropped_operation_count: 0,
                            worker_revision: 0,
                        })
                    } else {
                        None
                    },
                ))
                .unwrap();
            close_to_removable(&mut endpoint, binding);
        }
    }

    #[test]
    fn close_cancels_final_install_and_runtime_owned_scan_before_drain() {
        let binding = binding(1);

        let mut pending_install = Endpoint::fresh(config(4));
        open(&mut pending_install, binding, OpenMode::Fresh);
        pending_install
            .dispatch(&snapshot_frame(
                binding,
                10,
                0,
                1,
                1,
                0,
                0,
                known(0, "a"),
                "a",
            ))
            .unwrap();
        assert!(pending_install.pending_install.is_some());
        close_to_removable(&mut pending_install, binding);
        assert!(pending_install.pending_install.is_none());

        let mut scanning = Endpoint::fresh(config(4));
        open(&mut scanning, binding, OpenMode::Fresh);
        seed_one_page(&mut scanning, binding, "abcdef", known(0, "abcdef"), 0);
        assert_eq!(scanning.certification, CertificationStatus::Scanning);
        scanning.dispatch(&close_frame(binding, 700)).unwrap();
        assert_eq!(scanning.certification, CertificationStatus::NotStarted);
        assert!(scanning
            .runtime
            .as_ref()
            .and_then(DocumentRuntime::certified_source)
            .is_none());
        close_to_removable(&mut scanning, binding);
    }

    #[test]
    fn drain_progress_receipt_must_be_accepted_before_closed_is_emitted() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        endpoint.dispatch(&close_frame(binding, 2)).unwrap();
        endpoint.dispatch(&drain_frame(binding, 3, 1)).unwrap();
        let progress = endpoint.outstanding_status().unwrap();
        assert_eq!(progress.kind, EndpointEventKind::DrainProgress);
        assert!(endpoint
            .dispatch(&receipt_frame(
                binding,
                progress.event_id,
                EventDisposition::Rejected,
                None
            ))
            .is_err());
        assert_eq!(endpoint.outstanding_status(), None);
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Closing);
        endpoint.dispatch(&drain_frame(binding, 4, 1)).unwrap();
        let progress = endpoint.outstanding_status().unwrap();
        endpoint
            .dispatch(&receipt_frame(
                binding,
                progress.event_id,
                EventDisposition::Accepted,
                None,
            ))
            .unwrap();
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Closed);
        assert_eq!(
            endpoint.outstanding_status().unwrap().kind,
            EndpointEventKind::Closed
        );
        let closed = endpoint.outstanding_status().unwrap();
        assert!(endpoint
            .dispatch(&receipt_frame(
                binding,
                closed.event_id,
                EventDisposition::Rejected,
                None
            ))
            .is_err());
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Closing);
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn one_mib_single_line_reaches_publication_under_window_bounded_candidate_polls() {
        const SOURCE_BYTES: usize = 1024 * 1024;
        const CANDIDATE_FUEL: usize = 32;
        let binding = binding(1);
        let source = "x".repeat(SOURCE_BYTES);
        let mut endpoint = Endpoint::fresh(config(4 * 1024));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let _target = seed_ascii_pages_and_certify(&mut endpoint, binding, &source);

        let mut polls = 0_usize;
        let mut transitions = 0_usize;
        loop {
            polls += 1;
            let receipt = endpoint
                .poll_candidate(CANDIDATE_FUEL)
                .expect("candidate poll");
            transitions += receipt.transitions;
            if let Some(event) = receipt.outstanding_event {
                assert_eq!(event.kind, EndpointEventKind::PublicationBegin);
                break;
            }
            assert!(receipt.transitions > 0, "candidate must make progress");
        }
        assert!(
            polls <= 18,
            "1 MiB exact parse plus candidate sealing needed {polls} scheduled polls"
        );
        assert!(
            transitions <= 18 * CANDIDATE_FUEL,
            "1 MiB candidate exceeded the window-granular transition envelope: {transitions}"
        );

        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn mixed_524k_late_paragraph_burst_commits_by_recursive_green_adoption() {
        use std::fmt::Write as _;

        const MINIMUM_UTF16: usize = 512 * 1024;
        const LATE_PARAGRAPH: &str = "Late active **β😀bold** stays _fluid_ with `code`.";
        const INSERTION: &str = "swift";

        let mut source = String::from("Opening sentinel remains canonical.\n\n");
        let mut cycle = 0_usize;
        while source.len() < MINIMUM_UTF16 {
            writeln!(
                &mut source,
                "Paragraph {cycle} has **strong** and _emphasis_ content.\n\n\
                 ## Heading {cycle}\n\n\
                 > Quote {cycle} remains source-backed.\n\
                 > Its continuation stays in the same container.\n\n\
                 - list item {cycle}\n\
                 - second item {cycle}\n\n\
                 ```dart\n\
                 final value{cycle} = {cycle};\n\
                 ```\n\n\
                 ---"
            )
            .expect("append mixed fixture cycle");
            cycle += 1;
        }
        let target_start_utf16 = source.encode_utf16().count();
        source.push_str(LATE_PARAGRAPH);
        source.push('\n');
        assert!(target_start_utf16 > MINIMUM_UTF16);

        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4 * 1024));
        open(&mut endpoint, binding, OpenMode::Fresh);
        let mut base_stamp = seed_utf8_pages_and_certify(&mut endpoint, binding, &source);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent mixed-document host");
        host.observe_source_version(publication_source_version(&endpoint, binding))
            .expect("host observes mixed-document base");
        let base_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        assert_eq!(endpoint.recursive_green_path_counts_for_test(), (0, 0));

        let bold_end_in_paragraph = LATE_PARAGRAPH.find("bold").expect("bold marker") + 4;
        let mut insertion_utf16 = target_start_utf16
            + LATE_PARAGRAPH[..bold_end_in_paragraph]
                .encode_utf16()
                .count();
        let mut insertion_byte = source.rfind("bold").expect("late bold") + 4;
        let mut target_source = source;
        for (index, character) in INSERTION.char_indices() {
            let replacement = &INSERTION[index..index + character.len_utf8()];
            let mut next_source = target_source.clone();
            next_source.insert_str(insertion_byte, replacement);
            let revision = u32::try_from(index + 1).expect("bounded ASCII insertion revision");
            let target_stamp = known(revision, &next_source);
            endpoint
                .dispatch(&edit_frame(
                    binding,
                    100 + revision,
                    &[TestIntent {
                        sequence: revision,
                        base_revision: base_stamp.revision(),
                        revision,
                        base_stamp,
                        target_stamp,
                        operations: vec![TestOperation {
                            start: u32::try_from(insertion_utf16).expect("insertion UTF-16"),
                            end: u32::try_from(insertion_utf16).expect("insertion UTF-16"),
                            replacement,
                        }],
                    }],
                ))
                .expect("dispatch zero-cadence insertion");
            acknowledge_source(&mut endpoint, binding, revision);
            base_stamp = target_stamp;
            target_source = next_source;
            insertion_byte += replacement.len();
            insertion_utf16 += replacement.encode_utf16().count();
        }

        assert!(matches!(
            endpoint.active_source_facts,
            Some(ActiveSourceFacts::Incremental(_))
        ));
        let _ = poll_to_incremental_handoff(&mut endpoint);
        let final_certification = accept_incremental_certification(&mut endpoint, binding);
        assert_eq!(final_certification.ui_revision, INSERTION.len() as u32);
        host.observe_source_version(publication_source_version(&endpoint, binding))
            .expect("host observes final burst target");
        let target_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);
        assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(
            endpoint.recursive_green_path_counts_for_test(),
            (1, 0),
            "the committed burst target must use local Adoption -> ReadyUpdate, not a clean Green rebuild"
        );

        let mut next_source = target_source.clone();
        next_source.insert(insertion_byte, '!');
        let revision = u32::try_from(INSERTION.len() + 1).expect("follow-up revision");
        let target_stamp = known(revision, &next_source);
        endpoint
            .dispatch(&edit_frame(
                binding,
                200 + revision,
                &[TestIntent {
                    sequence: revision,
                    base_revision: base_stamp.revision(),
                    revision,
                    base_stamp,
                    target_stamp,
                    operations: vec![TestOperation {
                        start: u32::try_from(insertion_utf16).expect("follow-up UTF-16"),
                        end: u32::try_from(insertion_utf16).expect("follow-up UTF-16"),
                        replacement: "!",
                    }],
                }],
            ))
            .expect("dispatch independently certified follow-up insertion");
        acknowledge_source(&mut endpoint, binding, revision);
        assert!(matches!(
            endpoint.active_source_facts,
            Some(ActiveSourceFacts::Incremental(_))
        ));
        let _ = poll_to_incremental_handoff(&mut endpoint);
        let follow_up_certification = accept_incremental_certification(&mut endpoint, binding);
        assert_eq!(follow_up_certification.ui_revision, revision);
        host.observe_source_version(publication_source_version(&endpoint, binding))
            .expect("host observes independently certified follow-up target");
        let follow_up_delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);
        assert_eq!(
            follow_up_delivery.offer.mode,
            PublicationMode::ExactBaseDelta
        );
        assert_eq!(
            endpoint.recursive_green_path_counts_for_test(),
            (2, 0),
            "a second certified EOF edit must reuse the terminal convergence authority"
        );

        close_host_to_zero(&mut host);
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn one_hundred_thousand_physical_lines_with_one_reference_cross_endpoint_host_and_close_to_zero(
    ) {
        const PHYSICAL_LINES: usize = 100_000;
        let mut source = String::from("[shared]: /target \"title\"\n");
        source.reserve((PHYSICAL_LINES - 1) * 2);
        for _ in 1..PHYSICAL_LINES {
            source.push_str("x\n");
        }
        assert_eq!(source.lines().count(), PHYSICAL_LINES);
        assert!(source.len() > crate::v3_session_wire::MAXIMUM_SNAPSHOT_UTF16 as usize);
        let receipt = run_large_endpoint_host_replacement(&source, 1);
        assert_eq!(receipt.source_fact_records, 1);
        assert_eq!(receipt.first.ack.record_count, 5);
        eprintln!(
            "m11_100k_line_endpoint lines={PHYSICAL_LINES} source_bytes={} source_fact_records={} first_records={} first_packets={} first_candidate_polls={} first_candidate_transitions={} first_host_polls={} first_producer_cleanup_polls={} max_event_bytes={} max_packet_bytes={} delete_frame_bytes={} replacement_records={} replacement_packets={} replacement_host_polls={} replacement_cleanup_polls={} host_close_polls={}",
            source.len(),
            receipt.source_fact_records,
            receipt.first.ack.record_count,
            receipt.first.packet_count,
            receipt.first.candidate_polls,
            receipt.first.candidate_transitions,
            receipt.first.host_polls,
            receipt.first.producer_cleanup_polls,
            receipt.first.maximum_encoded_event_bytes,
            receipt.first.maximum_packet_bytes,
            receipt.delete_frame_bytes,
            receipt.replacement.ack.record_count,
            receipt.replacement.packet_count,
            receipt.replacement.host_polls,
            receipt.replacement.producer_cleanup_polls,
            receipt.host_close_polls,
        );
    }

    #[test]
    #[ignore = "manual 10 MiB production endpoint-to-host resource and reclamation receipt"]
    fn ten_mib_single_line_crosses_endpoint_host_replacement_and_close_to_zero() {
        const SOURCE_BYTES: usize = 10 * 1024 * 1024;
        let source = "x".repeat(SOURCE_BYTES);
        let receipt = run_large_endpoint_host_replacement(&source, 0);
        assert!(receipt.source_fact_records > 1);
        assert!(
            receipt.source_fact_records
                <= flark_engine::parser_internal::M11_MAX_ROLE_RECORDS as u64
        );
        eprintln!(
            "m11_10mib_endpoint source_bytes={SOURCE_BYTES} source_fact_records={} first_records={} first_packets={} first_candidate_polls={} first_candidate_transitions={} first_host_polls={} first_producer_cleanup_polls={} max_event_bytes={} max_packet_bytes={} delete_frame_bytes={} replacement_records={} replacement_packets={} replacement_host_polls={} replacement_cleanup_polls={} host_close_polls={}",
            receipt.source_fact_records,
            receipt.first.ack.record_count,
            receipt.first.packet_count,
            receipt.first.candidate_polls,
            receipt.first.candidate_transitions,
            receipt.first.host_polls,
            receipt.first.producer_cleanup_polls,
            receipt.first.maximum_encoded_event_bytes,
            receipt.first.maximum_packet_bytes,
            receipt.delete_frame_bytes,
            receipt.replacement.ack.record_count,
            receipt.replacement.packet_count,
            receipt.replacement.host_polls,
            receipt.replacement.producer_cleanup_polls,
            receipt.host_close_polls,
        );
    }

    #[test]
    fn one_hundred_thousand_reference_definitions_cross_endpoint_host_replacement_and_close_to_zero(
    ) {
        const DEFINITIONS: usize = 100_000;
        let mut source = String::new();
        source.reserve(DEFINITIONS * 24);
        for ordinal in 0..DEFINITIONS {
            use std::fmt::Write as _;
            writeln!(&mut source, "[label-{ordinal}]: /u").expect("string write");
        }
        let receipt = run_large_endpoint_host_replacement(&source, DEFINITIONS as u64);
        assert!(receipt.source_fact_records > 0);
        assert_eq!(
            receipt.first.ack.record_count as u64,
            receipt.source_fact_records + DEFINITIONS as u64 + 3
        );
        assert!(receipt.first.packet_count > 1);
        assert!(
            receipt.first.maximum_encoded_event_bytes
                <= v3_wire::HEADER_BYTES
                    + PUBLICATION_PAYLOAD_PREFIX_BYTES
                    + MAXIMUM_PACKET_ENCODED_BYTES
        );
        assert!(receipt.first.maximum_packet_bytes <= MAXIMUM_PACKET_ENCODED_BYTES);
        assert!(receipt.delete_frame_bytes < 512);
        assert_eq!(receipt.replacement.ack.record_count, 1);
        assert!(receipt.replacement.packet_count > 0);
        assert!(receipt.host_close_polls > 0);
        eprintln!(
            "m11_100k_reference_endpoint definitions={DEFINITIONS} source_bytes={} source_fact_records={} first_records={} first_packets={} first_candidate_polls={} first_candidate_transitions={} first_host_polls={} first_producer_cleanup_polls={} max_event_bytes={} max_packet_bytes={} delete_frame_bytes={} replacement_records={} replacement_packets={} replacement_host_polls={} replacement_cleanup_polls={} host_close_polls={}",
            source.len(),
            receipt.source_fact_records,
            receipt.first.ack.record_count,
            receipt.first.packet_count,
            receipt.first.candidate_polls,
            receipt.first.candidate_transitions,
            receipt.first.host_polls,
            receipt.first.producer_cleanup_polls,
            receipt.first.maximum_encoded_event_bytes,
            receipt.first.maximum_packet_bytes,
            receipt.delete_frame_bytes,
            receipt.replacement.ack.record_count,
            receipt.replacement.packet_count,
            receipt.replacement.host_polls,
            receipt.replacement.producer_cleanup_polls,
            receipt.host_close_polls,
        );
    }

    #[test]
    fn supersede_cancels_discovery_or_lexical_quantum_without_partial_candidate() {
        let binding = binding(1);
        let source = "x".repeat(5_000);

        for polls_before_supersede in [1_usize, 2] {
            let mut endpoint = Endpoint::fresh(config(4 * 1024));
            open(&mut endpoint, binding, OpenMode::Fresh);
            let target = seed_ascii_pages_and_certify(&mut endpoint, binding, &source);

            for _ in 0..polls_before_supersede {
                let receipt = endpoint.poll_candidate(1).expect("candidate quantum");
                assert_eq!(receipt.transitions, 1);
                assert_eq!(receipt.outstanding_event, None);
                assert!(receipt.cleanup_complete);
            }

            let supersede = endpoint
                .dispatch(&supersede_frame(binding, 701, 0))
                .expect("supersede active exact parse");
            assert_eq!(supersede.correlation_id, 701);
            assert_eq!(supersede.action, EndpointCommandAction::Superseded);
            assert_eq!(endpoint.outstanding_status(), None);
            assert_eq!(endpoint.installed_target, Some(target));
            assert_eq!(endpoint.certification, CertificationStatus::NotStarted);

            let after = endpoint
                .poll_candidate(1)
                .expect("cancelled candidate poll");
            assert_eq!(after.transitions, 0);
            assert!(after.cleanup_complete);
            assert_eq!(after.outstanding_event, None);

            close_to_removable(&mut endpoint, binding);
        }
    }

    #[test]
    fn positive_u32_event_exhaustion_fails_closed() {
        let binding = binding(1);
        let mut endpoint = Endpoint::fresh(config(4));
        endpoint.set_next_event_id_for_test(u32::MAX);
        endpoint
            .dispatch(&open_frame(binding, OpenMode::Fresh, 1))
            .unwrap();
        let opened = endpoint.outstanding_status().unwrap();
        assert_eq!(opened.event_id, u32::MAX);
        endpoint
            .dispatch(&receipt_frame(
                binding,
                opened.event_id,
                EventDisposition::Accepted,
                None,
            ))
            .unwrap();
        assert!(matches!(
            endpoint.dispatch(&snapshot_frame(
                binding,
                10,
                0,
                0,
                0,
                0,
                0,
                known(0, ""),
                ""
            )),
            Err(EndpointError::EventIdentityExhausted)
        ));
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Faulted);
    }

    #[test]
    fn nested_block_quote_projection_emits_unavailable_without_faulting() {
        const SOURCE: &str = "> > nested quote\n";
        let binding = binding(29);
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, binding, OpenMode::Fresh);
        seed_ascii_pages_and_certify(&mut endpoint, binding, SOURCE);

        let source_version = publication_source_version(&endpoint, binding);
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(source_version)
            .expect("host exact source");
        let delivery = deliver_current_candidate(&mut endpoint, binding, &mut host);

        let point = SOURCE.find("nested").expect("nested quote content") + 1;
        let command = InlineRefinementCommand {
            binding,
            refinement_generation: 1,
            source_version,
            base_ack: delivery.ack,
            byte_offset: u32::try_from(point).expect("bounded byte point"),
            utf16_offset: u32::try_from(SOURCE[..point].encode_utf16().count())
                .expect("bounded UTF-16 point"),
            affinity: crate::v3_session_wire::InlinePointAffinity::After,
            target: crate::v3_session_wire::InlineRefinementTarget::BlockQuoteProjection,
        };
        let receipt = endpoint
            .dispatch(&inline_refinement_frame(command, 901))
            .expect("unsupported nested quote remains a valid late query");
        assert_eq!(
            receipt.action,
            EndpointCommandAction::InlineRefinementAccepted
        );
        let outstanding = endpoint
            .outstanding_status()
            .expect("unsupported shape emits a terminal refinement event");
        assert_eq!(
            outstanding.kind,
            EndpointEventKind::InlineRefinementUnavailable
        );
        assert!(matches!(
            endpoint
                .outstanding
                .as_ref()
                .and_then(OutstandingEvent::session_body),
            Some(EventBody::InlineRefinementUnavailable(
                InlineRefinementUnavailableEvent {
                    refinement_generation: 1,
                    reason_code: INLINE_REFINEMENT_UNAVAILABLE_LATE_QUERY,
                }
            ))
        ));
        assert_eq!(endpoint.status().lifecycle, EndpointLifecycle::Open);
        assert!(!endpoint.status().failure_emitted);

        endpoint
            .dispatch(&receipt_frame(
                binding,
                outstanding.event_id,
                EventDisposition::Accepted,
                None,
            ))
            .expect("acknowledge unavailable event");
        close_host_to_zero(&mut host);
        close_to_removable(&mut endpoint, binding);
    }

    #[test]
    fn certified_source_streams_through_exact_credit_into_independent_host() {
        let first_binding = binding(7);
        let source = "plain paragraph\n";
        let mut endpoint = Endpoint::fresh(config(4));
        open(&mut endpoint, first_binding, OpenMode::Fresh);
        seed_one_page(&mut endpoint, first_binding, source, provisional(0, 16), 0);
        assert_eq!(
            poll_to_certification(&mut endpoint),
            CertificationStatus::ExternallyEligible
        );

        let SourceStamp::Known {
            revision,
            utf16_length,
            utf8_length,
            content_hash128,
        } = endpoint.installed_target.expect("promoted exact source")
        else {
            panic!("certification must refine the source stamp")
        };
        let source_version = PublicationSourceVersion {
            document_session: first_binding.document_session,
            revision,
            utf8_length,
            utf16_length,
            content_hash128,
        };
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: first_binding.document_session,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(source_version)
            .expect("host exact source");

        let drive = |endpoint: &mut Endpoint,
                     binding: SessionBinding,
                     host: &mut NativeCandidateHost,
                     expect_prior_install: bool|
         -> (StructuralAck, [u32; 2]) {
            let runtime = endpoint.runtime.as_mut().expect("open document runtime");
            while !runtime.poll_retirement(256).complete {}
            let persistent_page = runtime
                .persistent_source_facts_page(0)
                .expect("persistent SourceFacts page")
                .expect("certification retains one SourceFacts page")
                .id();
            let persistent_resident_nodes = runtime.arena_metrics().resident_nodes;
            assert!(persistent_resident_nodes > 0);

            let mut offer_id = None;
            let mut source_root = None;
            let mut record_total = 0_u32;
            let mut saw_begin_frame = false;
            let mut saw_end_frame = false;
            let mut committed_ack = None;
            for _ in 0..20_000 {
                if endpoint.outstanding_status().is_none() {
                    endpoint.poll_candidate(31).expect("candidate poll");
                }
                let Some(status) = endpoint.outstanding_status() else {
                    continue;
                };
                let mut encoded = vec![
                    0_u8;
                    v3_wire::HEADER_BYTES
                        + PUBLICATION_PAYLOAD_PREFIX_BYTES
                        + MAXIMUM_PACKET_ENCODED_BYTES
                ];
                let written = endpoint
                    .encode_outstanding_event(&mut encoded)
                    .expect("encode candidate event");
                encoded.truncate(written);
                let event = decode_publication_event(&encoded, binding).expect("publication event");
                assert_eq!(event.event_id, status.event_id);
                match event.body {
                    PublicationEventBody::Begin(begin) => {
                        assert_eq!(status.kind, EndpointEventKind::PublicationBegin);
                        assert_eq!(begin.target_host_revision, begin.parse_generation);
                        assert_eq!(
                            begin.parse_generation,
                            binding.worker_generation + begin.source_version.revision
                        );
                        offer_id = Some(begin.offer_id);
                        source_root = Some(begin.source_root);
                        host.begin_offer(begin).expect("host begin offer");
                        endpoint
                            .dispatch(&receipt_frame(
                                binding,
                                status.event_id,
                                EventDisposition::Accepted,
                                None,
                            ))
                            .expect("schema-3 begin receipt");
                    }
                    PublicationEventBody::Packet(packet) => {
                        assert_eq!(status.kind, EndpointEventKind::PublicationPacket);
                        assert_eq!(Some(packet.offer_id), offer_id);
                        for frame in packet.frames() {
                            let frame = frame.expect("validated packet frame");
                            let metadata =
                                flark_engine::m11_host::M11CandidateHost::classify_frame(
                                    frame.bytes,
                                )
                                .expect("engine frame metadata");
                            assert_eq!(metadata.canonical_record_count, frame.record_count);
                            assert_eq!(frame.first_record_ordinal, record_total);
                            match metadata.kind {
                                flark_engine::m11_host::M11HostFrameKind::Begin => {
                                    assert_eq!(frame.ordinal, 0);
                                    assert_eq!(frame.record_count, 0);
                                    saw_begin_frame = true;
                                }
                                flark_engine::m11_host::M11HostFrameKind::Node => {}
                                flark_engine::m11_host::M11HostFrameKind::SourceFactsReplacementPage => {
                                    panic!("clean recovery publication cannot contain an exact-base SourceFacts page")
                                }
                                flark_engine::m11_host::M11HostFrameKind::BlockSequenceReplacementPage => {
                                    panic!("clean recovery publication cannot contain an exact-base BlockSequence page")
                                }
                                flark_engine::m11_host::M11HostFrameKind::RecursiveGreenReplacementPage => {
                                    panic!("clean recovery publication cannot contain an exact-base recursive-Green page")
                                }
                                flark_engine::m11_host::M11HostFrameKind::End => {
                                    assert_eq!(frame.record_count, 0);
                                    saw_end_frame = true;
                                }
                            }
                            record_total += frame.record_count;
                        }
                        host.admit_packet(packet).expect("host copies packet");
                        endpoint
                            .dispatch(&receipt_frame(
                                binding,
                                status.event_id,
                                EventDisposition::Accepted,
                                None,
                            ))
                            .expect("schema-3 packet receipt");
                        let outcome = loop {
                            let outcome = host
                                .poll(HostWorkGrant {
                                    inspect_bytes: 16 * 1024,
                                    copy_bytes: 16 * 1024,
                                    transitions: 31,
                                })
                                .expect("host packet poll");
                            if !matches!(outcome, NativeHostPollOutcome::Pending) {
                                break outcome;
                            }
                        };
                        let NativeHostPollOutcome::PacketCredit { .. } = outcome else {
                            panic!("one closed packet must return exact packet credit")
                        };
                        endpoint
                            .dispatch_host_poll(&host_poll_response(
                                binding,
                                status.event_id,
                                packet.offer_id,
                                HostPollPhase::PacketCredit,
                                outcome,
                            ))
                            .expect("exact host packet ticket");
                    }
                    PublicationEventBody::Commit(commit) => {
                        assert_eq!(status.kind, EndpointEventKind::PublicationCommit);
                        assert!(saw_begin_frame && saw_end_frame);
                        let installed_before_commit =
                            host.role_record_count(flark_engine::m11_host::M11HostRole::Green);
                        if expect_prior_install {
                            assert_eq!(
                                installed_before_commit.expect("prior root remains queryable"),
                                1
                            );
                        } else {
                            assert!(
                                installed_before_commit.is_err(),
                                "End admission must not install before Commit"
                            );
                        }
                        host.request_commit(commit).expect("host commit request");
                        endpoint
                            .dispatch(&receipt_frame(
                                binding,
                                status.event_id,
                                EventDisposition::Accepted,
                                None,
                            ))
                            .expect("schema-3 commit receipt");
                        let outcome = loop {
                            let outcome = host
                                .poll(HostWorkGrant {
                                    inspect_bytes: 16 * 1024,
                                    copy_bytes: 16 * 1024,
                                    transitions: 31,
                                })
                                .expect("fuelled host install");
                            if !matches!(outcome, NativeHostPollOutcome::Pending) {
                                break outcome;
                            }
                        };
                        let NativeHostPollOutcome::Committed(ack) = outcome else {
                            panic!("commit must install exactly one candidate")
                        };
                        assert_eq!(ack.record_count, record_total);
                        committed_ack = Some(ack);
                        endpoint
                            .dispatch_host_poll(&host_poll_response(
                                binding,
                                status.event_id,
                                commit.offer_id,
                                HostPollPhase::Commit,
                                outcome,
                            ))
                            .expect("exact commit host ticket");
                        assert!(!endpoint.candidate.cleanup_pending());
                    }
                    PublicationEventBody::DeliveryAcknowledged(ack) => {
                        assert_eq!(
                            status.kind,
                            EndpointEventKind::PublicationDeliveryAcknowledged
                        );
                        assert_eq!(Some(ack), committed_ack);
                        host.acknowledge_delivery(ack)
                            .expect("host delivery retirement");
                        assert!(!endpoint.candidate.cleanup_pending());
                        endpoint
                            .dispatch(&receipt_frame(
                                binding,
                                status.event_id,
                                EventDisposition::Accepted,
                                None,
                            ))
                            .expect("schema-3 delivery receipt");
                        assert!(!endpoint.candidate.cleanup_pending());
                        let runtime = endpoint.runtime.as_ref().expect("open document runtime");
                        assert_eq!(
                            runtime
                                .persistent_source_facts_page(0)
                                .expect("surviving SourceFacts page")
                                .expect("persistent SourceFacts remains installed")
                                .id(),
                            persistent_page
                        );
                        assert!(
                            runtime.arena_metrics().resident_nodes > persistent_resident_nodes,
                            "delivery must retain one producer publication alongside persistent SourceFacts"
                        );
                        assert_eq!(
                            host.role_record_count(flark_engine::m11_host::M11HostRole::Green)
                                .expect("installed Green role"),
                            1
                        );
                        assert_eq!(endpoint.outstanding_status(), None);
                        return (ack, source_root.expect("Begin source root"));
                    }
                    PublicationEventBody::AbortRequested { .. }
                    | PublicationEventBody::Failed { .. } => {
                        panic!("clean no-reference vertical must not abort")
                    }
                }
            }
            panic!("candidate publication did not complete under bounded polling");
        };

        let (first_ack, first_root) = drive(&mut endpoint, first_binding, &mut host, false);
        drop(endpoint);

        let recovery_binding = binding(8);
        let mut recovered =
            Endpoint::recovery(first_binding, config(4)).expect("recovery endpoint");
        open(&mut recovered, recovery_binding, OpenMode::Recovery);
        seed_one_page(
            &mut recovered,
            recovery_binding,
            source,
            known(0, source),
            0,
        );
        assert_eq!(
            poll_to_certification(&mut recovered),
            CertificationStatus::ExternallyEligible
        );
        host.observe_source_version(source_version)
            .expect("same exact source observed by replacement worker");
        let (recovered_ack, recovered_root) =
            drive(&mut recovered, recovery_binding, &mut host, true);
        assert_ne!(
            first_root, recovered_root,
            "recovery must allocate a new root"
        );
        assert_eq!(first_ack.source_version, recovered_ack.source_version);
        assert!(recovered_ack.parse_generation > first_ack.parse_generation);
        assert_eq!(recovered_ack.host_revision, recovered_ack.parse_generation);
    }
}

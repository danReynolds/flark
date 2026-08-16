//! Source-bound fuelled construction for persistent recursive Green roots.

use std::fmt;

use crate::candidate_manifest::StrongIdentity;
use crate::document::DocumentRuntime;
use crate::measured_sequence::{
    begin_measured_sequence_seal, concat_measured_sequence_build_roots_atomic,
    BeginMeasuredSequenceSealFailure, CommittedMeasuredSequenceRoot, MeasuredSequenceBuildRoot,
    MeasuredSequenceSeal, ResumableMeasuredSequenceBuilder, ResumableSequenceProgress,
    SequenceInspectionReceipt, SequenceMutationReceipt,
};
use crate::source::{SourceCursor, SourceSnapshotLease, SourceVersion};
use crate::storage::{ArenaBuildSession, CandidateBuild, ARENA_PAGE_BYTES};
use crate::ReclaimReceipt;

use super::codec::{
    encode_leaf_header, encode_packed_event, packed_event_len, packed_event_summary, LogicalAtom,
    M11RecursiveGreenError, M11RecursiveGreenEvent, M11RecursiveGreenFrameId,
    M11RecursiveGreenKind, M11RecursiveGreenLogicalAction, M11RecursiveGreenSourceMetric,
    PackedGreenEvent, RecursiveGreenSpec, RecursiveGreenSummary, GREEN_EVENTS_PER_PAGE_MAX,
    GREEN_LEAF_HEADER_BYTES,
};

pub const M11_RECURSIVE_GREEN_MAX_POLL_TRANSITIONS: usize = 4096;
const CANONICAL_SCAN_BYTES_PER_TRANSITION: usize = 512;

pub(super) fn allocate_recursive_green_identity() -> Result<StrongIdentity, M11RecursiveGreenError>
{
    StrongIdentity::allocate(b"recursive-green").map_err(|_| M11RecursiveGreenError::InvalidState)
}

pub(super) type GreenSequenceBuilder = ResumableMeasuredSequenceBuilder<RecursiveGreenSpec>;
pub(super) type GreenSequenceBuildRoot = MeasuredSequenceBuildRoot<RecursiveGreenSpec>;
type GreenSequenceSeal = MeasuredSequenceSeal<RecursiveGreenSpec>;
pub(super) type GreenSequenceTree = CommittedMeasuredSequenceRoot<RecursiveGreenSpec>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11RecursiveGreenBuildReceipt {
    transitions: usize,
    events: u64,
    renderable_rows: u64,
    source_bytes: u64,
    source_utf16: u64,
    logical_bytes: u64,
    logical_utf16: u64,
    storage_pages: usize,
    branches_allocated: usize,
    node_headers_decoded: u64,
    payload_bytes_inspected: u64,
    events_authenticated: u64,
    maximum_live_bins: usize,
    reserved_scratch_bytes: usize,
    seal_transitions: usize,
}

impl M11RecursiveGreenBuildReceipt {
    pub(super) fn from_mutation(
        transitions: usize,
        summary: RecursiveGreenSummary,
        mutation: SequenceMutationReceipt,
        seal_transitions: usize,
    ) -> Self {
        Self {
            transitions,
            events: summary.events,
            renderable_rows: summary.renderable_row_exits,
            source_bytes: summary.physical_bytes,
            source_utf16: summary.physical_utf16,
            logical_bytes: summary.logical_bytes,
            logical_utf16: summary.logical_utf16,
            storage_pages: mutation.leaves_adopted,
            branches_allocated: mutation.branches_allocated,
            node_headers_decoded: mutation.inspection.node_headers_decoded,
            payload_bytes_inspected: mutation.inspection.spec.payload_bytes_inspected,
            events_authenticated: mutation.inspection.spec.spec_items_hashed,
            maximum_live_bins: mutation.maximum_live_bins,
            reserved_scratch_bytes: mutation.reserved_scratch_bytes,
            seal_transitions,
        }
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
    #[must_use]
    pub const fn events(self) -> u64 {
        self.events
    }
    #[must_use]
    pub const fn renderable_rows(self) -> u64 {
        self.renderable_rows
    }
    #[must_use]
    pub const fn source_bytes(self) -> u64 {
        self.source_bytes
    }
    #[must_use]
    pub const fn source_utf16(self) -> u64 {
        self.source_utf16
    }
    #[must_use]
    pub const fn logical_bytes(self) -> u64 {
        self.logical_bytes
    }
    #[must_use]
    pub const fn logical_utf16(self) -> u64 {
        self.logical_utf16
    }
    #[must_use]
    pub const fn storage_pages(self) -> usize {
        self.storage_pages
    }
    #[must_use]
    pub const fn branches_allocated(self) -> usize {
        self.branches_allocated
    }
    #[must_use]
    pub const fn node_headers_decoded(self) -> u64 {
        self.node_headers_decoded
    }
    #[must_use]
    pub const fn payload_bytes_inspected(self) -> u64 {
        self.payload_bytes_inspected
    }
    #[must_use]
    pub const fn events_authenticated(self) -> u64 {
        self.events_authenticated
    }
    #[must_use]
    pub const fn maximum_live_bins(self) -> usize {
        self.maximum_live_bins
    }
    #[must_use]
    pub const fn reserved_scratch_bytes(self) -> usize {
        self.reserved_scratch_bytes
    }
    #[must_use]
    pub const fn seal_transitions(self) -> usize {
        self.seal_transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenBuildStatus {
    NeedsInput,
    Pending,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenBuildPoll {
    status: M11RecursiveGreenBuildStatus,
    transitions: usize,
}

impl M11RecursiveGreenBuildPoll {
    #[must_use]
    pub const fn status(self) -> M11RecursiveGreenBuildStatus {
        self.status
    }
    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BuildPhase {
    Accepting,
    Pushing,
    ReadyForFinish,
    Finishing,
    ReadyForRoot,
    ReadyForSeal,
    Sealing,
    FragmentBarrierFlush,
    FragmentBarrierFinishing,
    FragmentBarrierReadyForRoot,
    FragmentReady,
    FragmentFrozen,
    FragmentRewriting,
    Complete,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct OpenFrame {
    pub(super) frame: M11RecursiveGreenFrameId,
    pub(super) kind: M11RecursiveGreenKind,
    pub(super) event_ordinal: u64,
    pub(super) source_before: M11RecursiveGreenSourceMetric,
    pub(super) logical_before: M11RecursiveGreenSourceMetric,
    pub(super) fragment_issued: bool,
}

/// Linear authority for the active terminal frame which may enter one
/// unpublished normalization transaction.
///
/// Fields are deliberately private: callers can move this capability back to
/// the build but cannot manufacture event or projection coordinates.
#[must_use = "terminal-fragment authority must be consumed by a barrier or discarded"]
pub struct M11RecursiveGreenTerminalFragment {
    runtime_identity: StrongIdentity,
    source: SourceVersion,
    frame: M11RecursiveGreenFrameId,
    kind: M11RecursiveGreenKind,
    generation: u64,
    event_ordinal: u64,
    source_before: M11RecursiveGreenSourceMetric,
    logical_before: M11RecursiveGreenSourceMetric,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenTerminalFragmentBarrierStatus {
    Pending,
    Ready,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenTerminalFragmentBarrierPoll {
    status: M11RecursiveGreenTerminalFragmentBarrierStatus,
    transitions: usize,
}

impl M11RecursiveGreenTerminalFragmentBarrierPoll {
    #[must_use]
    pub const fn status(self) -> M11RecursiveGreenTerminalFragmentBarrierStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct M11RecursiveGreenTerminalFragmentStamp {
    pub(super) runtime_identity: StrongIdentity,
    pub(super) source: SourceVersion,
    pub(super) frame: M11RecursiveGreenFrameId,
    pub(super) kind: M11RecursiveGreenKind,
    pub(super) generation: u64,
    pub(super) barrier_generation: u64,
    pub(super) event_ordinal: u64,
    pub(super) source_before: M11RecursiveGreenSourceMetric,
    pub(super) logical_before: M11RecursiveGreenSourceMetric,
    pub(super) source_end: M11RecursiveGreenSourceMetric,
    pub(super) logical_end: M11RecursiveGreenSourceMetric,
    pub(super) events_end: u64,
}

/// Non-forgeable binding between one active terminal frame and one immutable
/// build-local packed-Green prefix.
#[must_use = "terminal-fragment bindings must be consumed by rewrite or resume"]
pub struct M11RecursiveGreenTerminalFragmentBinding {
    pub(super) stamp: M11RecursiveGreenTerminalFragmentStamp,
}

/// Copyable provenance identity for source adapters borrowing one live
/// terminal-fragment binding. It carries no rewrite authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenTerminalFragmentIdentity {
    stamp: M11RecursiveGreenTerminalFragmentStamp,
}

impl M11RecursiveGreenTerminalFragmentBinding {
    #[must_use]
    pub const fn identity(&self) -> M11RecursiveGreenTerminalFragmentIdentity {
        M11RecursiveGreenTerminalFragmentIdentity { stamp: self.stamp }
    }
}

impl fmt::Debug for M11RecursiveGreenTerminalFragmentBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11RecursiveGreenTerminalFragmentBinding")
            .field("frame", &self.stamp.frame)
            .field("generation", &self.stamp.generation)
            .field("barrier_generation", &self.stamp.barrier_generation)
            .field("event_ordinal", &self.stamp.event_ordinal)
            .finish_non_exhaustive()
    }
}

struct CanonicalScan {
    cursor: Option<SourceCursor>,
    run_start: usize,
    end: usize,
    owner_depth: u32,
    part: super::codec::M11RecursiveGreenCoveragePart,
    pending_nul: bool,
}

enum CanonicalScanStep {
    Pending,
    Event(PackedGreenEvent),
    Complete,
}

impl CanonicalScan {
    fn poll(
        &mut self,
        lease: &SourceSnapshotLease,
    ) -> Result<CanonicalScanStep, M11RecursiveGreenError> {
        if self.pending_nul {
            self.pending_nul = false;
            return Ok(CanonicalScanStep::Event(PackedGreenEvent::Coverage {
                physical: M11RecursiveGreenSourceMetric::from_validated(1, 1),
                owner_depth: self.owner_depth,
                part: self.part,
                atom: LogicalAtom::NulToReplacement,
            }));
        }
        let cursor = self
            .cursor
            .as_mut()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let scan_start = self.run_start;
        for _ in 0..CANONICAL_SCAN_BYTES_PER_TRANSITION {
            let position = cursor.position();
            if position == self.end {
                break;
            }
            let byte = cursor
                .next_byte()
                .ok_or(M11RecursiveGreenError::IncompleteCoverage)?;
            if byte == 0 {
                self.run_start = position
                    .checked_add(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                if position > scan_start {
                    self.pending_nul = true;
                    return Ok(CanonicalScanStep::Event(PackedGreenEvent::Coverage {
                        physical: metric_between(lease, scan_start, position)?,
                        owner_depth: self.owner_depth,
                        part: self.part,
                        atom: LogicalAtom::Identity,
                    }));
                }
                return Ok(CanonicalScanStep::Event(PackedGreenEvent::Coverage {
                    physical: M11RecursiveGreenSourceMetric::from_validated(1, 1),
                    owner_depth: self.owner_depth,
                    part: self.part,
                    atom: LogicalAtom::NulToReplacement,
                }));
            }
        }
        let position = cursor.position();
        if position == self.end {
            if position > self.run_start {
                let start = self.run_start;
                self.run_start = position;
                return Ok(CanonicalScanStep::Event(PackedGreenEvent::Coverage {
                    physical: metric_between(lease, start, position)?,
                    owner_depth: self.owner_depth,
                    part: self.part,
                    atom: LogicalAtom::Identity,
                }));
            }
            let cursor = self
                .cursor
                .take()
                .ok_or(M11RecursiveGreenError::InvalidState)?;
            drop(cursor.finish()?);
            return Ok(CanonicalScanStep::Complete);
        }
        Ok(CanonicalScanStep::Pending)
    }
}

/// Fuelled, source-authenticated builder for one packed recursive Green root.
#[must_use = "recursive-green builds require root transfer or explicit cancellation"]
pub struct M11RecursiveGreenBuild {
    pub(super) runtime_identity: StrongIdentity,
    pub(super) green_identity: StrongIdentity,
    pub(super) lease: Option<SourceSnapshotLease>,
    pub(super) source: SourceVersion,
    pub(super) phase: BuildPhase,
    input_closed: bool,
    pending_input: Option<M11RecursiveGreenEvent>,
    pending_packed: Option<PackedGreenEvent>,
    canonical_scan: Option<CanonicalScan>,
    pub(super) open: Vec<OpenFrame>,
    last_frame: u64,
    root_closed: bool,
    property_adjacent: bool,
    page: [u8; ARENA_PAGE_BYTES],
    page_len: usize,
    page_events: u16,
    page_summary: RecursiveGreenSummary,
    pub(super) builder: Option<GreenSequenceBuilder>,
    pub(super) build: Option<CandidateBuild>,
    build_root: Option<GreenSequenceBuildRoot>,
    pub(super) working_prefix: Option<GreenSequenceBuildRoot>,
    seal: Option<GreenSequenceSeal>,
    failed_tree: Option<GreenSequenceTree>,
    output: Option<M11RecursiveGreenRoot>,
    pub(super) mutation: SequenceMutationReceipt,
    transitions: usize,
    pub(super) expected_summary: RecursiveGreenSummary,
    seal_transitions: usize,
    sealed_storage_pages: Option<usize>,
    next_fragment_generation: u64,
    next_barrier_generation: u64,
    pending_fragment: Option<M11RecursiveGreenTerminalFragment>,
    pub(super) active_fragment: Option<M11RecursiveGreenTerminalFragmentStamp>,
}

impl fmt::Debug for M11RecursiveGreenBuild {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11RecursiveGreenBuild")
            .field("source", &self.source)
            .field("phase", &self.phase)
            .field("receipt", &self.receipt())
            .finish_non_exhaustive()
    }
}

impl M11RecursiveGreenBuild {
    pub fn new(
        runtime: &DocumentRuntime,
        lease: SourceSnapshotLease,
    ) -> Result<Self, M11RecursiveGreenError> {
        let source = lease.version();
        if runtime.current_source_version() != Some(source) {
            return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
        }
        let mut open = Vec::new();
        open.try_reserve_exact(64)
            .map_err(|_| M11RecursiveGreenError::InvalidState)?;
        let green_identity = allocate_recursive_green_identity()?;
        Ok(Self {
            runtime_identity: runtime.producer_identity(),
            green_identity,
            lease: Some(lease),
            source,
            phase: BuildPhase::Accepting,
            input_closed: false,
            pending_input: None,
            pending_packed: None,
            canonical_scan: None,
            open,
            last_frame: 0,
            root_closed: false,
            property_adjacent: false,
            page: [0; ARENA_PAGE_BYTES],
            page_len: GREEN_LEAF_HEADER_BYTES,
            page_events: 0,
            page_summary: RecursiveGreenSummary::empty(),
            builder: None,
            build: None,
            build_root: None,
            working_prefix: None,
            seal: None,
            failed_tree: None,
            output: None,
            mutation: SequenceMutationReceipt::default(),
            transitions: 0,
            expected_summary: RecursiveGreenSummary::empty(),
            seal_transitions: 0,
            sealed_storage_pages: None,
            next_fragment_generation: 1,
            next_barrier_generation: 1,
            pending_fragment: None,
            active_fragment: None,
        })
    }

    /// Captures the exact Green half of a parser/writer restart boundary.
    ///
    /// The build may have an unflushed terminal page, but no event or source
    /// recipe may still be pending. The returned move-only capability is bound
    /// to the identity inherited by this build's eventual committed root.
    #[doc(hidden)]
    pub fn capture_structural_boundary(
        &self,
    ) -> Result<super::adopt::M11RecursiveGreenStructuralBoundary, M11RecursiveGreenError> {
        if self.phase != BuildPhase::Accepting
            || self.input_closed
            || self.pending_input.is_some()
            || self.pending_packed.is_some()
            || self.canonical_scan.is_some()
            || self.root_closed
            || self.pending_fragment.is_some()
            || self.active_fragment.is_some()
            || self.open.is_empty()
            || usize::try_from(self.expected_summary.balance).ok() != Some(self.open.len())
        {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        super::adopt::M11RecursiveGreenStructuralBoundary::from_build(
            self.runtime_identity,
            self.green_identity,
            self.source,
            self.expected_summary.events,
            M11RecursiveGreenSourceMetric::from_validated(
                self.expected_summary.physical_bytes,
                self.expected_summary.physical_utf16,
            ),
            M11RecursiveGreenSourceMetric::from_validated(
                self.expected_summary.logical_bytes,
                self.expected_summary.logical_utf16,
            ),
            self.open.iter().map(|frame| (frame.frame, frame.kind)),
        )
    }

    pub fn offer_event(
        &mut self,
        event: M11RecursiveGreenEvent,
    ) -> Result<(), M11RecursiveGreenError> {
        if self.input_closed {
            return Err(M11RecursiveGreenError::InputClosed);
        }
        if self.phase != BuildPhase::Accepting
            || self.pending_input.is_some()
            || self.pending_packed.is_some()
            || self.canonical_scan.is_some()
        {
            return Err(if self.pending_input.is_some() {
                M11RecursiveGreenError::EventAlreadyPending
            } else {
                M11RecursiveGreenError::InvalidState
            });
        }
        self.pending_input = Some(event);
        Ok(())
    }

    /// Mints the sole linear transaction authority for the currently active
    /// terminal frame. The Enter and every accepted projection coordinate are
    /// derived from builder state; the caller supplies only the frame it
    /// already owns through the scalar event protocol.
    pub fn mint_terminal_fragment(
        &mut self,
        frame: M11RecursiveGreenFrameId,
    ) -> Result<M11RecursiveGreenTerminalFragment, M11RecursiveGreenError> {
        if self.phase != BuildPhase::Accepting || !self.is_waiting_for_input() {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        let open = self
            .open
            .last_mut()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        if open.frame != frame || open.fragment_issued {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        let generation = self.next_fragment_generation;
        self.next_fragment_generation = generation
            .checked_add(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        open.fragment_issued = true;
        Ok(M11RecursiveGreenTerminalFragment {
            runtime_identity: self.runtime_identity,
            source: self.source,
            frame: open.frame,
            kind: open.kind,
            generation,
            event_ordinal: open.event_ordinal,
            source_before: open.source_before,
            logical_before: open.logical_before,
        })
    }

    /// Freezes the active event suffix at a force-sealed leaf barrier. Polling
    /// is separate so page allocation and AVL reduction remain caller-fuelled.
    pub fn begin_terminal_fragment_barrier(
        &mut self,
        fragment: M11RecursiveGreenTerminalFragment,
    ) -> Result<(), M11RecursiveGreenError> {
        if self.phase != BuildPhase::Accepting
            || !self.is_waiting_for_input()
            || self.pending_fragment.is_some()
            || self.active_fragment.is_some()
            || fragment.runtime_identity != self.runtime_identity
            || fragment.source != self.source
        {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        let open = self
            .open
            .last()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        if open.frame != fragment.frame
            || open.kind != fragment.kind
            || open.event_ordinal != fragment.event_ordinal
            || open.source_before != fragment.source_before
            || open.logical_before != fragment.logical_before
            || !open.fragment_issued
        {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        self.pending_fragment = Some(fragment);
        self.phase = if self.page_events > 0 {
            BuildPhase::FragmentBarrierFlush
        } else if self.builder.is_some() {
            BuildPhase::FragmentBarrierFinishing
        } else if self.working_prefix.is_some() {
            self.complete_fragment_barrier()?;
            BuildPhase::FragmentReady
        } else {
            return Err(M11RecursiveGreenError::InvalidState);
        };
        Ok(())
    }

    pub fn poll_terminal_fragment_barrier(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11RecursiveGreenTerminalFragmentBarrierPoll, M11RecursiveGreenError> {
        if self.pending_fragment.is_none()
            || !matches!(
                self.phase,
                BuildPhase::FragmentBarrierFlush
                    | BuildPhase::Pushing
                    | BuildPhase::FragmentBarrierFinishing
                    | BuildPhase::FragmentBarrierReadyForRoot
                    | BuildPhase::FragmentReady
            )
        {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        let poll = self.poll(runtime, fuel)?;
        Ok(M11RecursiveGreenTerminalFragmentBarrierPoll {
            status: if self.phase == BuildPhase::FragmentReady {
                M11RecursiveGreenTerminalFragmentBarrierStatus::Ready
            } else {
                M11RecursiveGreenTerminalFragmentBarrierStatus::Pending
            },
            transitions: poll.transitions(),
        })
    }

    /// Transfers the frozen projection binding to the transaction actor. The
    /// build remains paused until that exact binding is resumed or rewritten.
    pub fn take_terminal_fragment_binding(
        &mut self,
    ) -> Result<M11RecursiveGreenTerminalFragmentBinding, M11RecursiveGreenError> {
        if self.phase != BuildPhase::FragmentReady {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        let stamp = self
            .active_fragment
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        self.phase = BuildPhase::FragmentFrozen;
        Ok(M11RecursiveGreenTerminalFragmentBinding { stamp })
    }

    pub fn finish_input(&mut self) -> Result<(), M11RecursiveGreenError> {
        if self.input_closed {
            return Err(M11RecursiveGreenError::InputClosed);
        }
        if self.phase != BuildPhase::Accepting
            || self.pending_input.is_some()
            || self.pending_packed.is_some()
            || self.canonical_scan.is_some()
        {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        if !self.open.is_empty()
            || !self.root_closed
            || self.expected_summary.balance != 0
            || self.expected_summary.minimum_prefix != 0
            || self.expected_summary.oldest_open.is_some()
            || self.expected_summary.physical_bytes
                != u64::try_from(self.source.byte_len())
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
            || self.expected_summary.physical_utf16
                != u64::try_from(self.source.utf16_len())
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        {
            return Err(M11RecursiveGreenError::IncompleteCoverage);
        }
        self.input_closed = true;
        Ok(())
    }

    pub fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11RecursiveGreenBuildPoll, M11RecursiveGreenError> {
        self.ensure_runtime(runtime)?;
        validate_fuel(fuel)?;
        let before = self.transitions;
        while self.transitions - before < fuel {
            if self.phase == BuildPhase::Accepting && self.is_waiting_for_input() {
                return Ok(self.poll_receipt(M11RecursiveGreenBuildStatus::NeedsInput, before));
            }
            match self.phase {
                BuildPhase::Complete => {
                    return Ok(self.poll_receipt(M11RecursiveGreenBuildStatus::Complete, before));
                }
                BuildPhase::Cancelled => {
                    return Ok(self.poll_receipt(M11RecursiveGreenBuildStatus::Cancelled, before));
                }
                BuildPhase::FragmentReady
                | BuildPhase::FragmentFrozen
                | BuildPhase::FragmentRewriting => {
                    return Ok(self.poll_receipt(M11RecursiveGreenBuildStatus::Pending, before));
                }
                BuildPhase::Failed => return Err(M11RecursiveGreenError::InvalidState),
                _ => self.step(runtime)?,
            }
            self.transitions = self
                .transitions
                .checked_add(1)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        }
        let status = match self.phase {
            BuildPhase::Complete => M11RecursiveGreenBuildStatus::Complete,
            BuildPhase::Cancelled => M11RecursiveGreenBuildStatus::Cancelled,
            BuildPhase::Accepting if self.is_waiting_for_input() => {
                M11RecursiveGreenBuildStatus::NeedsInput
            }
            _ => M11RecursiveGreenBuildStatus::Pending,
        };
        Ok(self.poll_receipt(status, before))
    }

    fn poll_receipt(
        &self,
        status: M11RecursiveGreenBuildStatus,
        before: usize,
    ) -> M11RecursiveGreenBuildPoll {
        M11RecursiveGreenBuildPoll {
            status,
            transitions: self.transitions - before,
        }
    }

    fn is_waiting_for_input(&self) -> bool {
        !self.input_closed
            && self.pending_input.is_none()
            && self.pending_packed.is_none()
            && self.canonical_scan.is_none()
    }

    fn step(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11RecursiveGreenError> {
        match self.phase {
            BuildPhase::Accepting => self.step_accepting(runtime),
            BuildPhase::Pushing => self.poll_push(runtime),
            BuildPhase::ReadyForFinish => self.begin_finish(runtime),
            BuildPhase::Finishing => self.poll_finish(runtime),
            BuildPhase::ReadyForRoot => self.take_build_root(runtime),
            BuildPhase::ReadyForSeal => self.begin_seal(runtime),
            BuildPhase::Sealing => self.poll_seal(runtime),
            BuildPhase::FragmentBarrierFlush => self.begin_page(runtime),
            BuildPhase::FragmentBarrierFinishing => self.poll_fragment_finish(runtime),
            BuildPhase::FragmentBarrierReadyForRoot => self.take_fragment_root(runtime),
            BuildPhase::FragmentReady
            | BuildPhase::FragmentFrozen
            | BuildPhase::FragmentRewriting => Err(M11RecursiveGreenError::InvalidState),
            BuildPhase::Complete | BuildPhase::Cancelled | BuildPhase::Failed => {
                Err(M11RecursiveGreenError::InvalidState)
            }
        }
    }

    fn step_accepting(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11RecursiveGreenError> {
        if self.pending_packed.is_some() {
            return self.append_pending_packed(runtime);
        }
        if self.canonical_scan.is_some() {
            let step = self
                .canonical_scan
                .as_mut()
                .ok_or(M11RecursiveGreenError::InvalidState)?
                .poll(
                    self.lease
                        .as_ref()
                        .ok_or(M11RecursiveGreenError::InvalidState)?,
                )?;
            match step {
                CanonicalScanStep::Pending => return Ok(()),
                CanonicalScanStep::Event(event) => self.pending_packed = Some(event),
                CanonicalScanStep::Complete => {
                    self.canonical_scan = None;
                }
            }
            return Ok(());
        }
        if let Some(event) = self.pending_input.take() {
            return self.prepare_input(event);
        }
        if !self.input_closed {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        if self.page_events > 0 {
            return self.begin_page(runtime);
        }
        self.phase = BuildPhase::ReadyForFinish;
        Ok(())
    }

    fn prepare_input(
        &mut self,
        event: M11RecursiveGreenEvent,
    ) -> Result<(), M11RecursiveGreenError> {
        if self.root_closed {
            return self.fail(M11RecursiveGreenError::InvalidEvent);
        }
        let packed = match event {
            M11RecursiveGreenEvent::Enter { frame, kind } => {
                if frame.get() <= self.last_frame
                    || (self.open.is_empty() && self.expected_summary.events != 0)
                {
                    return self.fail(M11RecursiveGreenError::InvalidEvent);
                }
                self.last_frame = frame.get();
                self.open.push(OpenFrame {
                    frame,
                    kind,
                    event_ordinal: self.expected_summary.events,
                    source_before: M11RecursiveGreenSourceMetric::from_validated(
                        self.expected_summary.physical_bytes,
                        self.expected_summary.physical_utf16,
                    ),
                    logical_before: M11RecursiveGreenSourceMetric::from_validated(
                        self.expected_summary.logical_bytes,
                        self.expected_summary.logical_utf16,
                    ),
                    fragment_issued: false,
                });
                self.property_adjacent = true;
                Some(PackedGreenEvent::Enter { frame, kind })
            }
            M11RecursiveGreenEvent::Property(property) => {
                if !self.property_adjacent || self.open.is_empty() {
                    return self.fail(M11RecursiveGreenError::InvalidEvent);
                }
                Some(PackedGreenEvent::Property(property))
            }
            M11RecursiveGreenEvent::Coverage {
                physical,
                owner_depth,
                part,
                logical,
            } => {
                if physical.is_empty()
                    || usize::try_from(owner_depth)
                        .ok()
                        .is_none_or(|depth| depth >= self.open.len())
                {
                    return self.fail(M11RecursiveGreenError::InvalidEvent);
                }
                logical.validate(owner_depth)?;
                self.property_adjacent = false;
                self.prepare_coverage(physical, owner_depth, part, logical)?;
                None
            }
            M11RecursiveGreenEvent::RetypeOpen {
                frame,
                kind,
                property,
            } => {
                let Some(open) = self.open.last_mut() else {
                    return self.fail(M11RecursiveGreenError::InvalidEvent);
                };
                if open.frame != frame {
                    return self.fail(M11RecursiveGreenError::InvalidEvent);
                }
                open.kind = kind;
                self.property_adjacent = false;
                Some(PackedGreenEvent::RetypeOpen {
                    frame,
                    kind,
                    property,
                })
            }
            M11RecursiveGreenEvent::Exit {
                frame,
                final_kind,
                close,
                last_line_blank,
                child,
            } => {
                let Some(open) = self.open.pop() else {
                    return self.fail(M11RecursiveGreenError::InvalidEvent);
                };
                if open.frame != frame || open.kind != final_kind {
                    return self.fail(M11RecursiveGreenError::InvalidEvent);
                }
                if self.open.is_empty() {
                    self.root_closed = true;
                }
                self.property_adjacent = false;
                Some(PackedGreenEvent::Exit {
                    frame,
                    final_kind,
                    close,
                    last_line_blank,
                    child,
                })
            }
        };
        if let Some(packed) = packed {
            self.pending_packed = Some(packed);
        }
        Ok(())
    }

    fn prepare_coverage(
        &mut self,
        physical: M11RecursiveGreenSourceMetric,
        owner_depth: u32,
        part: super::codec::M11RecursiveGreenCoveragePart,
        logical: M11RecursiveGreenLogicalAction,
    ) -> Result<(), M11RecursiveGreenError> {
        let start = usize::try_from(self.expected_summary.physical_bytes)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        let end_u64 = self
            .expected_summary
            .physical_bytes
            .checked_add(physical.bytes())
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let end = usize::try_from(end_u64).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        let expected_utf16 = self
            .expected_summary
            .physical_utf16
            .checked_add(physical.utf16())
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        if end > self.source.byte_len()
            || expected_utf16
                > u64::try_from(self.source.utf16_len())
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
            || u64::try_from(
                self.lease
                    .as_ref()
                    .ok_or(M11RecursiveGreenError::InvalidState)?
                    .utf16_offset_for_byte(end)?,
            )
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
                != expected_utf16
        {
            return self.fail(M11RecursiveGreenError::IncompleteCoverage);
        }
        let atom = match logical {
            M11RecursiveGreenLogicalAction::None => Some(LogicalAtom::None),
            M11RecursiveGreenLogicalAction::Identity => Some(LogicalAtom::Identity),
            M11RecursiveGreenLogicalAction::HiddenUpstream => Some(LogicalAtom::HiddenUpstream),
            M11RecursiveGreenLogicalAction::PartialTab {
                target_owner_depth,
                remaining_spaces,
            } => {
                if physical != M11RecursiveGreenSourceMetric::from_validated(1, 1)
                    || read_small(
                        self.lease
                            .as_ref()
                            .ok_or(M11RecursiveGreenError::InvalidState)?,
                        start,
                        end,
                    )? != [b'\t'].as_slice()
                {
                    return self.fail(M11RecursiveGreenError::InvalidEvent);
                }
                Some(LogicalAtom::TabToSpaces {
                    target_owner_depth,
                    spaces: remaining_spaces,
                })
            }
            M11RecursiveGreenLogicalAction::CanonicalNewline => {
                let bytes = read_small(
                    self.lease
                        .as_ref()
                        .ok_or(M11RecursiveGreenError::InvalidState)?,
                    start,
                    end,
                )?;
                Some(match bytes.as_slice() {
                    b"\n" => LogicalAtom::LfToLf,
                    b"\r" => LogicalAtom::LoneCrToLf,
                    b"\r\n" => LogicalAtom::CrLfToLf,
                    _ => return self.fail(M11RecursiveGreenError::InvalidEvent),
                })
            }
            M11RecursiveGreenLogicalAction::CanonicalText => {
                let cursor = self
                    .lease
                    .as_ref()
                    .ok_or(M11RecursiveGreenError::InvalidState)?
                    .duplicate()
                    .cursor_in(start..end)?;
                self.canonical_scan = Some(CanonicalScan {
                    cursor: Some(cursor),
                    run_start: start,
                    end,
                    owner_depth,
                    part,
                    pending_nul: false,
                });
                None
            }
        };
        if let Some(atom) = atom {
            self.pending_packed = Some(PackedGreenEvent::Coverage {
                physical,
                owner_depth,
                part,
                atom,
            });
        }
        Ok(())
    }

    fn append_pending_packed(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11RecursiveGreenError> {
        let event = self
            .pending_packed
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let encoded_len = packed_event_len(event);
        if self.page_events > 0
            && (usize::from(self.page_events) >= GREEN_EVENTS_PER_PAGE_MAX
                || self.page_len + encoded_len > ARENA_PAGE_BYTES)
        {
            return self.begin_page(runtime);
        }
        if self.page_len + encoded_len > ARENA_PAGE_BYTES {
            return self.fail(M11RecursiveGreenError::Corrupt(
                "recursive-green event exceeds a page",
            ));
        }
        let event_summary = packed_event_summary(event)?;
        let next_page = self.page_summary.checked_followed_by(event_summary)?;
        let next_expected = self.expected_summary.checked_followed_by(event_summary)?;
        encode_packed_event(event, &mut self.page, &mut self.page_len)?;
        self.page_events = self
            .page_events
            .checked_add(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        self.page_summary = next_page;
        self.expected_summary = next_expected;
        self.pending_packed = None;
        Ok(())
    }

    fn begin_page(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11RecursiveGreenError> {
        encode_leaf_header(
            &mut self.page,
            self.page_events,
            self.page_len - GREEN_LEAF_HEADER_BYTES,
            self.page_summary,
        )?;
        let payload_len = self.page_len;
        if self.builder.is_none() {
            let result = (|| {
                let mut session = match self.build.take() {
                    Some(build) => runtime.producer_arena_mut().resume_build(build)?,
                    None => runtime.producer_arena_mut().begin_build()?,
                };
                let mut builder = GreenSequenceBuilder::try_new(&mut session, &mut self.mutation)?;
                let leaf = session.allocate(&self.page[..payload_len], &[])?;
                builder.begin_push(&session, leaf, &mut self.mutation)?;
                Ok::<_, M11RecursiveGreenError>((builder, session.suspend()?))
            })();
            match result {
                Ok((builder, build)) => {
                    self.builder = Some(builder);
                    self.build = Some(build);
                }
                Err(error) => return self.fail(error),
            }
        } else {
            let build = self
                .build
                .take()
                .ok_or(M11RecursiveGreenError::InvalidState)?;
            let result = (|| {
                let mut session = runtime.producer_arena_mut().resume_build(build)?;
                let leaf = session.allocate(&self.page[..payload_len], &[])?;
                self.builder
                    .as_mut()
                    .ok_or(M11RecursiveGreenError::InvalidState)?
                    .begin_push(&session, leaf, &mut self.mutation)?;
                Ok::<_, M11RecursiveGreenError>(session.suspend()?)
            })();
            match result {
                Ok(build) => self.build = Some(build),
                Err(error) => return self.fail(error),
            }
        }
        self.reset_page();
        self.phase = BuildPhase::Pushing;
        Ok(())
    }

    fn poll_push(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11RecursiveGreenError> {
        let progress = self.with_resumed_build(runtime, |builder, session, mutation| {
            builder.poll_push(session, mutation)
        })?;
        self.phase = match progress {
            ResumableSequenceProgress::Pending => BuildPhase::Pushing,
            ResumableSequenceProgress::Complete if self.pending_fragment.is_some() => {
                BuildPhase::FragmentBarrierFinishing
            }
            ResumableSequenceProgress::Complete
                if self.input_closed
                    && self.pending_packed.is_none()
                    && self.canonical_scan.is_none() =>
            {
                BuildPhase::ReadyForFinish
            }
            ResumableSequenceProgress::Complete => BuildPhase::Accepting,
        };
        Ok(())
    }

    fn begin_finish(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11RecursiveGreenError> {
        if self.builder.is_none() {
            self.build_root = self.working_prefix.take();
            if self.build_root.is_none() {
                return Err(M11RecursiveGreenError::InvalidState);
            }
            self.phase = BuildPhase::ReadyForSeal;
            return Ok(());
        }
        self.with_resumed_build(runtime, |builder, session, mutation| {
            builder.begin_finish(session, mutation)
        })?;
        self.phase = BuildPhase::Finishing;
        Ok(())
    }

    fn poll_finish(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11RecursiveGreenError> {
        let progress = self.with_resumed_build(runtime, |builder, session, mutation| {
            builder.poll_finish(session, mutation)
        })?;
        self.phase = match progress {
            ResumableSequenceProgress::Pending => BuildPhase::Finishing,
            ResumableSequenceProgress::Complete => BuildPhase::ReadyForRoot,
        };
        Ok(())
    }

    fn poll_fragment_finish(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11RecursiveGreenError> {
        if self.builder.is_none() {
            self.complete_fragment_barrier()?;
            self.phase = BuildPhase::FragmentReady;
            return Ok(());
        }
        let reduction_started = self.build_root.is_some();
        if !reduction_started {
            self.with_resumed_build(runtime, |builder, session, mutation| {
                builder.begin_finish(session, mutation)
            })?;
            // A private sentinel distinguishes "begin reduction" from the
            // root-ready phase without exposing arena ownership.
            self.build_root = None;
            self.phase = BuildPhase::FragmentBarrierReadyForRoot;
            return Ok(());
        }
        Err(M11RecursiveGreenError::InvalidState)
    }

    fn take_fragment_root(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11RecursiveGreenError> {
        let progress = self.with_resumed_build(runtime, |builder, session, mutation| {
            builder.poll_finish(session, mutation)
        })?;
        if progress == ResumableSequenceProgress::Pending {
            return Ok(());
        }
        let build = self
            .build
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let mut session = runtime.producer_arena_mut().resume_build(build)?;
        let suffix = self
            .builder
            .as_mut()
            .ok_or(M11RecursiveGreenError::InvalidState)?
            .take_root(&session)?;
        let root = concat_measured_sequence_build_roots_atomic::<RecursiveGreenSpec>(
            &mut session,
            self.working_prefix.take(),
            Some(suffix),
            &mut self.mutation,
        )?
        .ok_or(M11RecursiveGreenError::InvalidState)?;
        self.build = Some(session.suspend()?);
        self.builder = None;
        self.working_prefix = Some(root);
        self.complete_fragment_barrier()?;
        self.phase = BuildPhase::FragmentReady;
        Ok(())
    }

    fn complete_fragment_barrier(&mut self) -> Result<(), M11RecursiveGreenError> {
        if self.working_prefix.is_none()
            || self.page_events != 0
            || self.builder.is_some()
            || self.build_root.is_some()
        {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        let fragment = self
            .pending_fragment
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let barrier_generation = self.next_barrier_generation;
        self.next_barrier_generation = barrier_generation
            .checked_add(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        self.active_fragment = Some(M11RecursiveGreenTerminalFragmentStamp {
            runtime_identity: fragment.runtime_identity,
            source: fragment.source,
            frame: fragment.frame,
            kind: fragment.kind,
            generation: fragment.generation,
            barrier_generation,
            event_ordinal: fragment.event_ordinal,
            source_before: fragment.source_before,
            logical_before: fragment.logical_before,
            source_end: M11RecursiveGreenSourceMetric::from_validated(
                self.expected_summary.physical_bytes,
                self.expected_summary.physical_utf16,
            ),
            logical_end: M11RecursiveGreenSourceMetric::from_validated(
                self.expected_summary.logical_bytes,
                self.expected_summary.logical_utf16,
            ),
            events_end: self.expected_summary.events,
        });
        Ok(())
    }

    fn take_build_root(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11RecursiveGreenError> {
        let build = self
            .build
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let mut session = runtime.producer_arena_mut().resume_build(build)?;
        match self
            .builder
            .as_mut()
            .ok_or(M11RecursiveGreenError::InvalidState)?
            .take_root(&session)
        {
            Ok(root) => {
                let root = concat_measured_sequence_build_roots_atomic::<RecursiveGreenSpec>(
                    &mut session,
                    self.working_prefix.take(),
                    Some(root),
                    &mut self.mutation,
                )?
                .ok_or(M11RecursiveGreenError::InvalidState)?;
                self.build = Some(session.suspend()?);
                self.build_root = Some(root);
                self.phase = BuildPhase::ReadyForSeal;
                Ok(())
            }
            Err(error) => {
                drop(session);
                self.fail(error)
            }
        }
    }

    fn begin_seal(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11RecursiveGreenError> {
        let build = self
            .build
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let root = self
            .build_root
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        match begin_measured_sequence_seal(runtime.producer_arena_mut(), build, root) {
            Ok(seal) => {
                self.builder = None;
                self.seal = Some(seal);
                self.phase = BuildPhase::Sealing;
                Ok(())
            }
            Err(BeginMeasuredSequenceSealFailure { error, build, root }) => {
                self.build = Some(build);
                self.build_root = Some(root);
                Err(error.into())
            }
        }
    }

    fn poll_seal(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11RecursiveGreenError> {
        let poll = self
            .seal
            .as_mut()
            .ok_or(M11RecursiveGreenError::InvalidState)?
            .poll(runtime.producer_arena_mut(), 1)?;
        self.seal_transitions = self
            .seal_transitions
            .checked_add(poll.transitions)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let Some(tree) = poll.root else {
            return Ok(());
        };
        self.seal = None;
        let mut inspection = SequenceInspectionReceipt::default();
        let Some(measure) = tree
            .as_ref()
            .summary(runtime.producer_arena(), &mut inspection)?
        else {
            return self.reject_tree(
                runtime,
                tree,
                M11RecursiveGreenError::Corrupt("sealed recursive-green root is empty"),
            );
        };
        add_inspection(&mut self.mutation.inspection, inspection)?;
        if measure.summary() != self.expected_summary
            || measure.summary().balance != 0
            || measure.summary().minimum_prefix != 0
            || measure.summary().oldest_open.is_some()
        {
            return self.reject_tree(
                runtime,
                tree,
                M11RecursiveGreenError::Corrupt("sealed recursive-green summary changed"),
            );
        }
        let lease = self
            .lease
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        self.sealed_storage_pages = Some(
            usize::try_from(measure.leaves())
                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
        );
        self.output = Some(M11RecursiveGreenRoot {
            runtime_identity: self.runtime_identity,
            green_identity: self.green_identity,
            lease: Some(lease),
            source: self.source,
            summary: measure.summary(),
            page_count: measure.leaves(),
            tree_height: measure.height(),
            tree: Some(tree),
            receipt: self.receipt(),
            released: false,
        });
        self.phase = BuildPhase::Complete;
        Ok(())
    }

    fn reject_tree(
        &mut self,
        runtime: &mut DocumentRuntime,
        tree: GreenSequenceTree,
        error: M11RecursiveGreenError,
    ) -> Result<(), M11RecursiveGreenError> {
        self.phase = BuildPhase::Failed;
        match tree.release(runtime.producer_arena_mut()) {
            Ok(()) => Err(error),
            Err(failure) => {
                let release_error = failure.error;
                self.failed_tree = Some(failure.root);
                Err(release_error.into())
            }
        }
    }

    fn with_resumed_build<T>(
        &mut self,
        runtime: &mut DocumentRuntime,
        operation: impl FnOnce(
            &mut GreenSequenceBuilder,
            &mut ArenaBuildSession<'_>,
            &mut SequenceMutationReceipt,
        ) -> Result<T, M11RecursiveGreenError>,
    ) -> Result<T, M11RecursiveGreenError> {
        let build = self
            .build
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let mut session = runtime.producer_arena_mut().resume_build(build)?;
        match operation(
            self.builder
                .as_mut()
                .ok_or(M11RecursiveGreenError::InvalidState)?,
            &mut session,
            &mut self.mutation,
        ) {
            Ok(value) => {
                self.build = Some(session.suspend()?);
                Ok(value)
            }
            Err(error) => {
                drop(session);
                self.fail(error)
            }
        }
    }

    fn reset_page(&mut self) {
        self.page.fill(0);
        self.page_len = GREEN_LEAF_HEADER_BYTES;
        self.page_events = 0;
        self.page_summary = RecursiveGreenSummary::empty();
    }

    fn fail<T>(&mut self, error: M11RecursiveGreenError) -> Result<T, M11RecursiveGreenError> {
        self.phase = BuildPhase::Failed;
        Err(error)
    }

    fn ensure_runtime(&self, runtime: &DocumentRuntime) -> Result<(), M11RecursiveGreenError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11RecursiveGreenError::WrongRuntime);
        }
        if runtime.current_source_version() != Some(self.source) {
            return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
        }
        Ok(())
    }

    pub fn begin_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11RecursiveGreenError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11RecursiveGreenError::WrongRuntime);
        }
        if let Some(mut output) = self.output.take() {
            output.begin_release(runtime)?;
        }
        if let Some(tree) = self.failed_tree.take() {
            if let Err(failure) = tree.release(runtime.producer_arena_mut()) {
                let error = failure.error;
                self.failed_tree = Some(failure.root);
                return Err(error.into());
            }
        }
        if let Some(seal) = self.seal.take() {
            if let Err(failure) = seal.abort(runtime.producer_arena_mut()) {
                self.seal = Some(failure.seal);
                return Err(failure.error.into());
            }
        }
        if let Some(build) = self.build.take() {
            runtime.producer_arena_mut().abort_build(build)?;
        }
        self.builder = None;
        self.build_root = None;
        self.working_prefix = None;
        self.pending_input = None;
        self.pending_packed = None;
        self.canonical_scan = None;
        self.pending_fragment = None;
        self.active_fragment = None;
        self.lease.take();
        self.phase = BuildPhase::Cancelled;
        Ok(())
    }

    pub fn poll_cancel(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11RecursiveGreenReclaimPoll, M11RecursiveGreenError> {
        if self.phase != BuildPhase::Cancelled {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        poll_reclaim(runtime, fuel)
    }

    #[must_use]
    pub fn take_root(&mut self) -> Option<M11RecursiveGreenRoot> {
        if self.phase != BuildPhase::Complete {
            return None;
        }
        let receipt = self.receipt();
        if let Some(root) = self.output.as_mut() {
            root.receipt = receipt;
        }
        self.output.take()
    }

    #[must_use]
    pub fn receipt(&self) -> M11RecursiveGreenBuildReceipt {
        let mut receipt = M11RecursiveGreenBuildReceipt::from_mutation(
            self.transitions,
            self.expected_summary,
            self.mutation,
            self.seal_transitions,
        );
        if let Some(storage_pages) = self.sealed_storage_pages {
            receipt.storage_pages = storage_pages;
        }
        receipt
    }
}

impl Drop for M11RecursiveGreenBuild {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.build.is_none()
                    && self.seal.is_none()
                    && self.failed_tree.is_none()
                    && self.output.is_none()
                    && self.lease.is_none(),
                "recursive-green builds require root transfer or explicit cancellation"
            );
        }
    }
}

#[must_use = "recursive-green roots require explicit release"]
pub struct M11RecursiveGreenRoot {
    pub(super) runtime_identity: StrongIdentity,
    pub(super) green_identity: StrongIdentity,
    pub(super) lease: Option<SourceSnapshotLease>,
    pub(super) source: SourceVersion,
    pub(super) summary: RecursiveGreenSummary,
    pub(super) page_count: u64,
    pub(super) tree_height: u16,
    pub(super) tree: Option<GreenSequenceTree>,
    receipt: M11RecursiveGreenBuildReceipt,
    pub(super) released: bool,
}

impl fmt::Debug for M11RecursiveGreenRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11RecursiveGreenRoot")
            .field("source", &self.source)
            .field("events", &self.summary.events)
            .field("storage_pages", &self.page_count)
            .field("tree_height", &self.tree_height)
            .finish_non_exhaustive()
    }
}

impl M11RecursiveGreenRoot {
    pub(super) fn from_splice(
        runtime_identity: StrongIdentity,
        green_identity: StrongIdentity,
        lease: SourceSnapshotLease,
        summary: RecursiveGreenSummary,
        page_count: u64,
        tree_height: u16,
        tree: GreenSequenceTree,
        receipt: M11RecursiveGreenBuildReceipt,
    ) -> Self {
        Self {
            runtime_identity,
            green_identity,
            source: lease.version(),
            lease: Some(lease),
            summary,
            page_count,
            tree_height,
            tree: Some(tree),
            receipt,
            released: false,
        }
    }

    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }
    #[must_use]
    pub const fn event_count(&self) -> u64 {
        self.summary.events
    }
    /// Total bytes in the canonical packed event stream, excluding storage
    /// page and measured-tree headers.
    #[must_use]
    pub const fn canonical_event_byte_len(&self) -> u64 {
        self.summary.canonical_event_bytes
    }
    /// Shape-independent commitment to the exact ordered packed event stream.
    #[must_use]
    pub fn canonical_event_commitment256(&self) -> [u8; 32] {
        self.summary.canonical_commitment.checksum()
    }
    /// Greatest frame identity already present in this persistent root.
    ///
    /// Incremental writers seed newly inserted frames above this value so an
    /// unchanged suffix can retain its original identities and storage pages.
    #[doc(hidden)]
    #[must_use]
    pub const fn maximum_frame_id(&self) -> u64 {
        self.summary.max_frame_id
    }
    #[must_use]
    pub const fn source_byte_len(&self) -> u64 {
        self.summary.physical_bytes
    }
    #[must_use]
    pub const fn source_utf16_len(&self) -> u64 {
        self.summary.physical_utf16
    }
    #[must_use]
    pub const fn logical_byte_len(&self) -> u64 {
        self.summary.logical_bytes
    }
    #[must_use]
    pub const fn logical_utf16_len(&self) -> u64 {
        self.summary.logical_utf16
    }
    #[must_use]
    pub const fn storage_page_count(&self) -> u64 {
        self.page_count
    }
    #[must_use]
    pub const fn tree_height(&self) -> u16 {
        self.tree_height
    }
    #[must_use]
    pub const fn minimum_closed_depth(&self) -> Option<i64> {
        self.summary.minimum_closed_depth
    }
    #[must_use]
    pub const fn outermost_child_fold(&self) -> super::codec::M11RecursiveGreenChildFold {
        self.summary.outermost_children
    }
    #[must_use]
    pub const fn build_receipt(&self) -> M11RecursiveGreenBuildReceipt {
        self.receipt
    }

    pub(super) fn ensure_runtime(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<(), M11RecursiveGreenError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11RecursiveGreenError::WrongRuntime);
        }
        if runtime.current_source_version() != Some(self.source) {
            return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
        }
        if self.released || self.lease.is_none() || self.tree.is_none() {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        Ok(())
    }

    pub(super) fn ensure_storage_live(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<(), M11RecursiveGreenError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11RecursiveGreenError::WrongRuntime);
        }
        if self.released || self.lease.is_none() || self.tree.is_none() {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        Ok(())
    }

    pub(super) fn lease(&self) -> Result<&SourceSnapshotLease, M11RecursiveGreenError> {
        self.lease
            .as_ref()
            .ok_or(M11RecursiveGreenError::InvalidState)
    }

    #[cfg(test)]
    pub(super) fn tree_root_id_for_test(&self) -> Option<crate::ArenaId> {
        self.tree
            .as_ref()
            .and_then(GreenSequenceTree::root_id_for_test)
    }

    pub fn begin_release(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11RecursiveGreenError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11RecursiveGreenError::WrongRuntime);
        }
        if self.released {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        if let Some(tree) = self.tree.take() {
            match tree.release(runtime.producer_arena_mut()) {
                Ok(()) => {}
                Err(failure) => {
                    self.tree = Some(failure.root);
                    return Err(failure.error.into());
                }
            }
        }
        self.lease.take();
        self.released = true;
        Ok(())
    }

    pub fn poll_release(
        &self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11RecursiveGreenReclaimPoll, M11RecursiveGreenError> {
        if !self.released {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        poll_reclaim(runtime, fuel)
    }
}

impl Drop for M11RecursiveGreenRoot {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.released,
                "recursive-green roots require explicit release"
            );
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenReclaimPoll {
    receipt: ReclaimReceipt,
    complete: bool,
}

impl M11RecursiveGreenReclaimPoll {
    #[must_use]
    pub const fn receipt(self) -> ReclaimReceipt {
        self.receipt
    }
    #[must_use]
    pub const fn complete(self) -> bool {
        self.complete
    }
}

fn metric_between(
    lease: &SourceSnapshotLease,
    start: usize,
    end: usize,
) -> Result<M11RecursiveGreenSourceMetric, M11RecursiveGreenError> {
    let start_utf16 = lease.utf16_offset_for_byte(start)?;
    let end_utf16 = lease.utf16_offset_for_byte(end)?;
    M11RecursiveGreenSourceMetric::new(
        u64::try_from(
            end.checked_sub(start)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?,
        )
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
        u64::try_from(
            end_utf16
                .checked_sub(start_utf16)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?,
        )
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
    )
    .ok_or(M11RecursiveGreenError::InvalidEvent)
}

fn read_small(
    lease: &SourceSnapshotLease,
    start: usize,
    end: usize,
) -> Result<Vec<u8>, M11RecursiveGreenError> {
    if end.checked_sub(start).is_none_or(|len| len > 2) {
        return Err(M11RecursiveGreenError::InvalidEvent);
    }
    let mut cursor = lease.duplicate().cursor_in(start..end)?;
    let mut output = [0; 2];
    let read = cursor.read(&mut output);
    drop(cursor.finish()?);
    Ok(output[..read].to_vec())
}

fn validate_fuel(fuel: usize) -> Result<(), M11RecursiveGreenError> {
    if fuel == 0 {
        return Err(M11RecursiveGreenError::ZeroFuel);
    }
    if fuel > M11_RECURSIVE_GREEN_MAX_POLL_TRANSITIONS {
        return Err(M11RecursiveGreenError::PollLimitExceeded);
    }
    Ok(())
}

fn poll_reclaim(
    runtime: &mut DocumentRuntime,
    fuel: usize,
) -> Result<M11RecursiveGreenReclaimPoll, M11RecursiveGreenError> {
    validate_fuel(fuel)?;
    let receipt = runtime.producer_arena_mut().poll_reclaim(fuel);
    let metrics = runtime.arena_metrics();
    Ok(M11RecursiveGreenReclaimPoll {
        receipt,
        complete: metrics.pending_build_aborts == 0 && metrics.pending_reclaims == 0,
    })
}

fn add_inspection(
    total: &mut SequenceInspectionReceipt,
    added: SequenceInspectionReceipt,
) -> Result<(), M11RecursiveGreenError> {
    total.node_headers_decoded = total
        .node_headers_decoded
        .checked_add(added.node_headers_decoded)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    total.summary_combinations = total
        .summary_combinations
        .checked_add(added.summary_combinations)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    total.spec.payload_bytes_inspected = total
        .spec
        .payload_bytes_inspected
        .checked_add(added.spec.payload_bytes_inspected)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    total.spec.spec_items_hashed = total
        .spec
        .spec_items_hashed
        .checked_add(added.spec.spec_items_hashed)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    Ok(())
}

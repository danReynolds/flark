//! Source-backed, fuel-one cooking of exact reference destination/title cuts.

use std::collections::VecDeque;
use std::fmt;
use std::ops::Range;

use flark_engine::parser_internal::{
    M11CandidateBuild, M11CandidateBuildPoll, M11PublicationError, M11ReferenceRange,
    M11ReferenceRecordStart, M11ReferenceValueKind,
};
use flark_engine::{DocumentRuntime, SourceCursor, SourceEditError, SourceSnapshotLease};

use crate::reference_value::{
    clean_title_body_range, CleanReferenceValueChunk, DestinationTrimProbe,
    ReferenceValueBodyCleaner, ReferenceValueCleanerError, ReferenceValueCleanerReceipt,
    ReferenceValueCleanerStatus,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11ReferenceCookReceipt {
    pub transitions: u64,
    pub source_bytes_read: u64,
    pub probe_bytes: u64,
    pub count_input_bytes: u64,
    pub emit_input_bytes: u64,
    pub cooked_bytes_emitted: u64,
    pub completed_definitions: u64,
    pub maximum_source_window_bytes: usize,
    pub maximum_entity_candidate_bytes: usize,
    pub maximum_clean_output_chunk_bytes: usize,
    pub maximum_engine_retained_bytes: usize,
    pub maximum_plan_label_bytes: usize,
    pub cancelled: bool,
}

impl M11ReferenceCookReceipt {
    /// Conservative upper bound over simultaneously retained source-derived
    /// bytes. Fixed scalar/counter state is intentionally excluded.
    #[must_use]
    pub const fn maximum_retained_bytes(self) -> usize {
        self.maximum_source_window_bytes
            .saturating_add(self.maximum_entity_candidate_bytes)
            .saturating_add(self.maximum_clean_output_chunk_bytes)
            .saturating_add(self.maximum_engine_retained_bytes)
            .saturating_add(self.maximum_plan_label_bytes)
    }
}

#[derive(Debug)]
pub enum ReferenceCookError {
    Source(SourceEditError),
    Cleaner(&'static str),
    Publication(M11PublicationError),
    MetricOverflow,
    InvalidState(&'static str),
}

impl fmt::Display for ReferenceCookError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => error.fmt(formatter),
            Self::Cleaner(message) | Self::InvalidState(message) => formatter.write_str(message),
            Self::Publication(error) => error.fmt(formatter),
            Self::MetricOverflow => formatter.write_str("reference cooker metric overflow"),
        }
    }
}

impl std::error::Error for ReferenceCookError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Publication(error) => Some(error),
            Self::Cleaner(_) | Self::MetricOverflow | Self::InvalidState(_) => None,
        }
    }
}

impl From<SourceEditError> for ReferenceCookError {
    fn from(error: SourceEditError) -> Self {
        Self::Source(error)
    }
}

impl From<ReferenceValueCleanerError> for ReferenceCookError {
    fn from(_: ReferenceValueCleanerError) -> Self {
        Self::Cleaner("reference value cleaner rejected its bounded transition")
    }
}

impl From<M11PublicationError> for ReferenceCookError {
    fn from(error: M11PublicationError) -> Self {
        Self::Publication(error)
    }
}

pub(crate) struct CookReferencePlan {
    pub(crate) source: M11ReferenceRange,
    pub(crate) label_source: M11ReferenceRange,
    pub(crate) destination_source: M11ReferenceRange,
    pub(crate) title_source: Option<M11ReferenceRange>,
    pub(crate) destination_bytes: Range<usize>,
    pub(crate) title_bytes: Option<Range<usize>>,
    pub(crate) normalized_label: Box<[u8]>,
}

struct ActiveReference {
    header: Option<CookReferenceHeader>,
    destination_bytes: Range<usize>,
    title_bytes: Option<Range<usize>>,
    destination_selected: Option<Range<usize>>,
    title_selected: Option<Range<usize>>,
    destination_len: Option<usize>,
    title_len: Option<usize>,
}

struct CookReferenceHeader {
    source: M11ReferenceRange,
    label_source: M11ReferenceRange,
    destination_source: M11ReferenceRange,
    title_source: Option<M11ReferenceRange>,
    normalized_label: Box<[u8]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CookValueKind {
    Destination,
    Title,
}

impl CookValueKind {
    const fn engine(self) -> M11ReferenceValueKind {
        match self {
            Self::Destination => M11ReferenceValueKind::Destination,
            Self::Title => M11ReferenceValueKind::Title,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CookPhase {
    StartDefinition,
    Probe(CookValueKind),
    Count(CookValueKind),
    BeginStream,
    Emit(CookValueKind),
    AwaitFact,
    Complete,
    Cancelled,
    Failed,
}

#[derive(Default)]
struct ValueProbe {
    destination: DestinationTrimProbe,
    len: usize,
    first: Option<u8>,
    last: Option<u8>,
}

impl ValueProbe {
    fn push(&mut self, kind: CookValueKind, byte: u8) -> Result<(), ReferenceCookError> {
        self.first.get_or_insert(byte);
        self.last = Some(byte);
        self.len = self
            .len
            .checked_add(1)
            .ok_or(ReferenceCookError::MetricOverflow)?;
        if kind == CookValueKind::Destination {
            self.destination.push(byte)?;
        }
        Ok(())
    }

    fn finish(self, kind: CookValueKind) -> Range<usize> {
        match kind {
            CookValueKind::Destination => self.destination.finish(),
            CookValueKind::Title => clean_title_body_range(self.len, self.first, self.last),
        }
    }
}

pub(crate) enum ReferenceCookPoll {
    Progress,
    Complete,
}

/// Owns the one certified source lease while exact cuts are probed, counted,
/// and emitted into the candidate arena.
pub(crate) struct ReferenceCooker {
    lease: Option<SourceSnapshotLease>,
    cursor: Option<SourceCursor>,
    plans: VecDeque<CookReferencePlan>,
    active: Option<ActiveReference>,
    phase: CookPhase,
    probe: ValueProbe,
    cleaner: Option<ReferenceValueBodyCleaner>,
    cleaner_needs_input: bool,
    pending_output: Option<CleanReferenceValueChunk>,
    pending_output_offset: usize,
    receipt: M11ReferenceCookReceipt,
}

impl ReferenceCooker {
    pub(crate) fn new(lease: SourceSnapshotLease, plans: VecDeque<CookReferencePlan>) -> Self {
        let maximum_plan_label_bytes = plans
            .iter()
            .map(|plan| plan.normalized_label.len())
            .fold(0_usize, usize::saturating_add);
        Self {
            lease: Some(lease),
            cursor: None,
            plans,
            active: None,
            phase: CookPhase::StartDefinition,
            probe: ValueProbe::default(),
            cleaner: None,
            cleaner_needs_input: false,
            pending_output: None,
            pending_output_offset: 0,
            receipt: M11ReferenceCookReceipt {
                maximum_plan_label_bytes,
                ..M11ReferenceCookReceipt::default()
            },
        }
    }

    #[must_use]
    pub(crate) const fn receipt(&self) -> M11ReferenceCookReceipt {
        self.receipt
    }

    pub(crate) fn poll_one(
        &mut self,
        runtime: &mut DocumentRuntime,
        build: &mut M11CandidateBuild,
    ) -> Result<ReferenceCookPoll, ReferenceCookError> {
        if matches!(self.phase, CookPhase::Complete) {
            return Ok(ReferenceCookPoll::Complete);
        }
        if matches!(self.phase, CookPhase::Cancelled | CookPhase::Failed) {
            return Err(ReferenceCookError::InvalidState(
                "reference cooker is cancelled or failed",
            ));
        }
        self.receipt.transitions = self
            .receipt
            .transitions
            .checked_add(1)
            .ok_or(ReferenceCookError::MetricOverflow)?;
        let result = self.drive_one(runtime, build);
        if result.is_err() {
            self.phase = CookPhase::Failed;
        }
        result
    }

    fn drive_one(
        &mut self,
        runtime: &mut DocumentRuntime,
        build: &mut M11CandidateBuild,
    ) -> Result<ReferenceCookPoll, ReferenceCookError> {
        match self.phase {
            CookPhase::StartDefinition => Ok(self.start_definition()),
            CookPhase::Probe(kind) => self.poll_probe(kind),
            CookPhase::Count(kind) => self.poll_clean(kind, false, runtime, build),
            CookPhase::BeginStream => self.begin_stream(runtime, build),
            CookPhase::Emit(kind) => self.poll_clean(kind, true, runtime, build),
            CookPhase::AwaitFact => self.await_fact(runtime, build),
            CookPhase::Complete => Ok(ReferenceCookPoll::Complete),
            CookPhase::Cancelled | CookPhase::Failed => Err(ReferenceCookError::InvalidState(
                "reference cooker is not active",
            )),
        }
    }

    fn start_definition(&mut self) -> ReferenceCookPoll {
        let Some(plan) = self.plans.pop_front() else {
            self.lease.take();
            self.phase = CookPhase::Complete;
            return ReferenceCookPoll::Complete;
        };
        self.active = Some(ActiveReference {
            header: Some(CookReferenceHeader {
                source: plan.source,
                label_source: plan.label_source,
                destination_source: plan.destination_source,
                title_source: plan.title_source,
                normalized_label: plan.normalized_label,
            }),
            destination_bytes: plan.destination_bytes,
            title_bytes: plan.title_bytes,
            destination_selected: None,
            title_selected: None,
            destination_len: None,
            title_len: None,
        });
        self.probe = ValueProbe::default();
        self.phase = CookPhase::Probe(CookValueKind::Destination);
        ReferenceCookPoll::Progress
    }

    fn poll_probe(&mut self, kind: CookValueKind) -> Result<ReferenceCookPoll, ReferenceCookError> {
        if self.cursor.is_none() {
            let range = self.raw_range(kind)?;
            self.open_cursor(range)?;
            return Ok(ReferenceCookPoll::Progress);
        }
        let mut byte = [0_u8; 1];
        let read = self
            .cursor
            .as_mut()
            .ok_or(ReferenceCookError::InvalidState("probe cursor disappeared"))?
            .read(&mut byte);
        self.observe_cursor();
        if read == 1 {
            self.probe.push(kind, byte[0])?;
            self.bump_source_read()?;
            self.receipt.probe_bytes = self
                .receipt
                .probe_bytes
                .checked_add(1)
                .ok_or(ReferenceCookError::MetricOverflow)?;
            return Ok(ReferenceCookPoll::Progress);
        }
        self.finish_cursor()?;
        let raw = self.raw_range(kind)?;
        let local = std::mem::take(&mut self.probe).finish(kind);
        let selected_start = raw
            .start
            .checked_add(local.start)
            .ok_or(ReferenceCookError::MetricOverflow)?;
        let selected_end = raw
            .start
            .checked_add(local.end)
            .ok_or(ReferenceCookError::MetricOverflow)?;
        let selected = selected_start..selected_end;
        self.set_selected(kind, selected);
        self.phase = CookPhase::Count(kind);
        Ok(ReferenceCookPoll::Progress)
    }

    fn poll_clean(
        &mut self,
        kind: CookValueKind,
        emit: bool,
        runtime: &mut DocumentRuntime,
        build: &mut M11CandidateBuild,
    ) -> Result<ReferenceCookPoll, ReferenceCookError> {
        if self.cleaner.is_none() {
            self.open_cursor(self.selected_range(kind)?)?;
            self.cleaner = Some(ReferenceValueBodyCleaner::new());
            self.cleaner_needs_input = true;
            return Ok(ReferenceCookPoll::Progress);
        }
        if emit && self.flush_pending_output(kind, runtime, build)? {
            return Ok(ReferenceCookPoll::Progress);
        }
        if self.advance_clean_input(emit)? {
            return Ok(ReferenceCookPoll::Progress);
        }
        self.advance_cleaner(kind, emit)?;
        Ok(ReferenceCookPoll::Progress)
    }

    fn flush_pending_output(
        &mut self,
        kind: CookValueKind,
        runtime: &mut DocumentRuntime,
        build: &mut M11CandidateBuild,
    ) -> Result<bool, ReferenceCookError> {
        if self.pending_output.is_none() {
            return Ok(false);
        }
        let capacity = build.reference_stream_capacity(kind.engine())?;
        if capacity == 0 {
            self.poll_build_one(runtime, build)?;
            return Ok(true);
        }
        let (consumed, output_len) = {
            let bytes = self
                .pending_output
                .as_ref()
                .ok_or(ReferenceCookError::InvalidState(
                    "reference cleaner output disappeared",
                ))?
                .bytes();
            let end = self
                .pending_output_offset
                .saturating_add(capacity)
                .min(bytes.len());
            (
                build.offer_reference_stream_bytes(
                    kind.engine(),
                    &bytes[self.pending_output_offset..end],
                )?,
                bytes.len(),
            )
        };
        if consumed == 0 {
            return Err(ReferenceCookError::InvalidState(
                "reference sink accepted zero bytes with positive capacity",
            ));
        }
        self.pending_output_offset = self
            .pending_output_offset
            .checked_add(consumed)
            .ok_or(ReferenceCookError::MetricOverflow)?;
        self.receipt.cooked_bytes_emitted = self
            .receipt
            .cooked_bytes_emitted
            .checked_add(u64::try_from(consumed).map_err(|_| ReferenceCookError::MetricOverflow)?)
            .ok_or(ReferenceCookError::MetricOverflow)?;
        if self.pending_output_offset == output_len {
            self.pending_output = None;
            self.pending_output_offset = 0;
        }
        self.observe_engine(build);
        Ok(true)
    }

    fn advance_clean_input(&mut self, emit: bool) -> Result<bool, ReferenceCookError> {
        if !self.cleaner_needs_input {
            return Ok(false);
        }
        let mut byte = [0_u8; 1];
        let read = self
            .cursor
            .as_mut()
            .ok_or(ReferenceCookError::InvalidState("clean cursor disappeared"))?
            .read(&mut byte);
        self.observe_cursor();
        if read == 1 {
            self.cleaner
                .as_mut()
                .ok_or(ReferenceCookError::InvalidState("cleaner disappeared"))?
                .offer_byte(byte[0])?;
            self.cleaner_needs_input = false;
            self.bump_source_read()?;
            let counter = if emit {
                &mut self.receipt.emit_input_bytes
            } else {
                &mut self.receipt.count_input_bytes
            };
            *counter = counter
                .checked_add(1)
                .ok_or(ReferenceCookError::MetricOverflow)?;
            return Ok(true);
        }
        self.cleaner
            .as_mut()
            .ok_or(ReferenceCookError::InvalidState("cleaner disappeared"))?
            .finish_input()?;
        self.cleaner_needs_input = false;
        self.finish_cursor()?;
        Ok(true)
    }

    fn advance_cleaner(
        &mut self,
        kind: CookValueKind,
        emit: bool,
    ) -> Result<(), ReferenceCookError> {
        let status = self
            .cleaner
            .as_mut()
            .ok_or(ReferenceCookError::InvalidState("cleaner disappeared"))?
            .poll()?;
        match status {
            ReferenceValueCleanerStatus::Progress => {}
            ReferenceValueCleanerStatus::NeedInput => self.cleaner_needs_input = true,
            ReferenceValueCleanerStatus::OutputReady => {
                let (output, receipt) = {
                    let cleaner = self
                        .cleaner
                        .as_mut()
                        .ok_or(ReferenceCookError::InvalidState("cleaner disappeared"))?;
                    let output = cleaner.take_output()?;
                    (output, cleaner.receipt())
                };
                self.observe_cleaner(receipt);
                if emit {
                    self.pending_output = Some(output);
                    self.pending_output_offset = 0;
                }
            }
            ReferenceValueCleanerStatus::Complete => {
                let receipt = self
                    .cleaner
                    .as_ref()
                    .ok_or(ReferenceCookError::InvalidState("cleaner disappeared"))?
                    .receipt();
                self.observe_cleaner(receipt);
                let cooked_len = usize::try_from(receipt.output_bytes)
                    .map_err(|_| ReferenceCookError::MetricOverflow)?;
                self.cleaner = None;
                if emit {
                    if cooked_len != self.declared_len(kind)? {
                        return Err(ReferenceCookError::InvalidState(
                            "emit pass diverged from counted cooked length",
                        ));
                    }
                    self.phase = if kind == CookValueKind::Destination
                        && self
                            .active
                            .as_ref()
                            .is_some_and(|active| active.title_bytes.is_some())
                    {
                        CookPhase::Emit(CookValueKind::Title)
                    } else {
                        CookPhase::AwaitFact
                    };
                } else {
                    self.set_declared_len(kind, cooked_len);
                    self.phase = if kind == CookValueKind::Destination
                        && self
                            .active
                            .as_ref()
                            .is_some_and(|active| active.title_bytes.is_some())
                    {
                        self.probe = ValueProbe::default();
                        CookPhase::Probe(CookValueKind::Title)
                    } else {
                        CookPhase::BeginStream
                    };
                }
            }
        }
        Ok(())
    }

    fn begin_stream(
        &mut self,
        runtime: &mut DocumentRuntime,
        build: &mut M11CandidateBuild,
    ) -> Result<ReferenceCookPoll, ReferenceCookError> {
        if !build.references_idle() {
            self.poll_build_one(runtime, build)?;
            return Ok(ReferenceCookPoll::Progress);
        }
        let active = self
            .active
            .as_mut()
            .ok_or(ReferenceCookError::InvalidState(
                "reference plan disappeared",
            ))?;
        let header = active
            .header
            .take()
            .ok_or(ReferenceCookError::InvalidState(
                "reference header already consumed",
            ))?;
        build.begin_reference_stream(
            runtime,
            M11ReferenceRecordStart::new(
                header.source,
                header.label_source,
                header.destination_source,
                header.title_source,
                header.normalized_label,
                active
                    .destination_len
                    .ok_or(ReferenceCookError::InvalidState(
                        "destination length missing",
                    ))?,
                active.title_len,
            ),
        )?;
        self.observe_engine(build);
        self.phase = CookPhase::Emit(CookValueKind::Destination);
        Ok(ReferenceCookPoll::Progress)
    }

    fn await_fact(
        &mut self,
        runtime: &mut DocumentRuntime,
        build: &mut M11CandidateBuild,
    ) -> Result<ReferenceCookPoll, ReferenceCookError> {
        if !build.references_idle() {
            self.poll_build_one(runtime, build)?;
            return Ok(ReferenceCookPoll::Progress);
        }
        self.receipt.completed_definitions = self
            .receipt
            .completed_definitions
            .checked_add(1)
            .ok_or(ReferenceCookError::MetricOverflow)?;
        self.active = None;
        self.phase = CookPhase::StartDefinition;
        Ok(ReferenceCookPoll::Progress)
    }

    fn poll_build_one(
        &mut self,
        runtime: &mut DocumentRuntime,
        build: &mut M11CandidateBuild,
    ) -> Result<(), ReferenceCookError> {
        match build.poll(runtime, 1)? {
            M11CandidateBuildPoll::Pending { transitions: 1 } => {
                self.observe_engine(build);
                Ok(())
            }
            M11CandidateBuildPoll::Pending { transitions: 0 } => Err(
                ReferenceCookError::InvalidState("reference builder made no requested progress"),
            ),
            M11CandidateBuildPoll::Pending { .. } => Err(ReferenceCookError::InvalidState(
                "reference builder exceeded fuel one",
            )),
            M11CandidateBuildPoll::Published { .. } => Err(ReferenceCookError::InvalidState(
                "candidate published before reference cooking completed",
            )),
        }
    }

    fn open_cursor(&mut self, range: Range<usize>) -> Result<(), ReferenceCookError> {
        let lease = self.lease.take().ok_or(ReferenceCookError::InvalidState(
            "reference source lease disappeared",
        ))?;
        self.cursor = Some(lease.cursor_in(range)?);
        Ok(())
    }

    fn finish_cursor(&mut self) -> Result<(), ReferenceCookError> {
        let cursor = self.cursor.take().ok_or(ReferenceCookError::InvalidState(
            "reference cursor disappeared",
        ))?;
        self.receipt.maximum_source_window_bytes = self
            .receipt
            .maximum_source_window_bytes
            .max(cursor.max_refill_bytes());
        self.lease = Some(cursor.finish()?);
        Ok(())
    }

    fn observe_cursor(&mut self) {
        if let Some(cursor) = &self.cursor {
            self.receipt.maximum_source_window_bytes = self
                .receipt
                .maximum_source_window_bytes
                .max(cursor.max_refill_bytes());
        }
    }

    fn observe_cleaner(&mut self, receipt: ReferenceValueCleanerReceipt) {
        self.receipt.maximum_entity_candidate_bytes = self
            .receipt
            .maximum_entity_candidate_bytes
            .max(receipt.maximum_entity_candidate_bytes);
        self.receipt.maximum_clean_output_chunk_bytes = self
            .receipt
            .maximum_clean_output_chunk_bytes
            .max(receipt.maximum_output_chunk_bytes);
    }

    fn observe_engine(&mut self, build: &M11CandidateBuild) {
        self.receipt.maximum_engine_retained_bytes = self
            .receipt
            .maximum_engine_retained_bytes
            .max(build.reference_stream_retained_bytes());
    }

    fn bump_source_read(&mut self) -> Result<(), ReferenceCookError> {
        self.receipt.source_bytes_read = self
            .receipt
            .source_bytes_read
            .checked_add(1)
            .ok_or(ReferenceCookError::MetricOverflow)?;
        Ok(())
    }

    fn raw_range(&self, kind: CookValueKind) -> Result<Range<usize>, ReferenceCookError> {
        let active = self
            .active
            .as_ref()
            .ok_or(ReferenceCookError::InvalidState(
                "reference plan disappeared",
            ))?;
        match kind {
            CookValueKind::Destination => Ok(active.destination_bytes.clone()),
            CookValueKind::Title => {
                active
                    .title_bytes
                    .clone()
                    .ok_or(ReferenceCookError::InvalidState(
                        "title source range disappeared",
                    ))
            }
        }
    }

    fn selected_range(&self, kind: CookValueKind) -> Result<Range<usize>, ReferenceCookError> {
        let active = self
            .active
            .as_ref()
            .ok_or(ReferenceCookError::InvalidState(
                "reference plan disappeared",
            ))?;
        match kind {
            CookValueKind::Destination => active.destination_selected.clone(),
            CookValueKind::Title => active.title_selected.clone(),
        }
        .ok_or(ReferenceCookError::InvalidState(
            "reference selected range disappeared",
        ))
    }

    fn set_selected(&mut self, kind: CookValueKind, selected: Range<usize>) {
        if let Some(active) = &mut self.active {
            match kind {
                CookValueKind::Destination => active.destination_selected = Some(selected),
                CookValueKind::Title => active.title_selected = Some(selected),
            }
        }
    }

    fn declared_len(&self, kind: CookValueKind) -> Result<usize, ReferenceCookError> {
        let active = self
            .active
            .as_ref()
            .ok_or(ReferenceCookError::InvalidState(
                "reference plan disappeared",
            ))?;
        match kind {
            CookValueKind::Destination => active.destination_len,
            CookValueKind::Title => active.title_len,
        }
        .ok_or(ReferenceCookError::InvalidState(
            "reference cooked length disappeared",
        ))
    }

    fn set_declared_len(&mut self, kind: CookValueKind, len: usize) {
        if let Some(active) = &mut self.active {
            match kind {
                CookValueKind::Destination => active.destination_len = Some(len),
                CookValueKind::Title => active.title_len = Some(len),
            }
        }
    }

    pub(crate) fn cancel(&mut self) {
        if let Some(cursor) = self.cursor.take() {
            self.lease = Some(cursor.cancel());
        }
        self.pending_output = None;
        self.cleaner = None;
        self.plans.clear();
        self.active = None;
        self.lease.take();
        self.phase = CookPhase::Cancelled;
        self.receipt.cancelled = true;
    }
}

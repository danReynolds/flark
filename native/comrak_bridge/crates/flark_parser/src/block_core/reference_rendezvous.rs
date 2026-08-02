//! Atomic join between reference recognition, persistent semantics, and Green.
//!
//! The donor owns CommonMark recognition, the frozen terminal Green fragment
//! owns projected source, and `M11ReferenceJournal` owns ordered first-winner
//! semantics.  This actor is the only place those three linear capabilities
//! meet.  It never materializes a Paragraph or an unbounded destination/title.

use std::fmt;
use std::ops::Range;

use flark_block_core_donor as donor;
use flark_engine::parser_internal::{
    M11RecursiveGreenBuildStatus, M11RecursiveGreenError, M11RecursiveGreenFrameId,
    M11RecursiveGreenLogicalPosition, M11RecursiveGreenLogicalRange,
    M11RecursiveGreenTerminalFragmentBarrierStatus, M11RecursiveGreenTerminalFragmentBinding,
    M11RecursiveGreenTerminalFragmentCursor, M11RecursiveGreenTerminalFragmentCursorStatus,
    M11RecursiveGreenTerminalFragmentDisposition, M11RecursiveGreenTerminalFragmentIdentity,
    M11RecursiveGreenTerminalFragmentRewrite, M11RecursiveGreenTerminalFragmentRewritePoll,
    M11RecursiveGreenTerminalFragmentRewriteWork, M11ReferenceJournal, M11ReferenceJournalError,
    M11ReferenceJournalOccurrenceStart, M11ReferenceJournalRange, M11ReferenceJournalValueKind,
};
use flark_engine::DocumentRuntime;

use super::writer::M11ReferenceStagedTerminator;
use super::{M11BlockWriter, M11BlockWriterError, M11DirectBlockController, M11DirectBlockError};
use crate::reference_value::{
    clean_title_body_range, CleanReferenceValueChunk, DestinationTrimProbe,
    ReferenceValueBodyCleaner, ReferenceValueCleanerError, ReferenceValueCleanerStatus,
};

type Identity = M11RecursiveGreenTerminalFragmentIdentity;
type Work = donor::DirectReferencePrefixWork<Identity>;
type OutputAck = donor::DirectReferencePrefixOutputAck<Identity>;
type TerminalOutput = donor::DirectReferencePrefixTerminalOutput<Identity>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11ReferenceRendezvousStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11ReferenceRendezvousPoll {
    pub transitions: usize,
    pub status: M11ReferenceRendezvousStatus,
}

#[derive(Debug)]
pub enum M11ReferenceRendezvousError {
    Controller(M11DirectBlockError),
    Writer(M11BlockWriterError),
    Green(M11RecursiveGreenError),
    Journal(M11ReferenceJournalError),
    Cleaner(ReferenceValueCleanerError),
    InvalidState(&'static str),
    CounterOverflow,
    ZeroFuel,
}

impl fmt::Display for M11ReferenceRendezvousError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Controller(error) => write!(formatter, "{error:?}"),
            Self::Writer(error) => error.fmt(formatter),
            Self::Green(error) => error.fmt(formatter),
            Self::Journal(error) => error.fmt(formatter),
            Self::Cleaner(error) => error.fmt(formatter),
            Self::InvalidState(message) => formatter.write_str(message),
            Self::CounterOverflow => formatter.write_str("reference rendezvous counter overflow"),
            Self::ZeroFuel => formatter.write_str("reference rendezvous requires nonzero fuel"),
        }
    }
}

impl std::error::Error for M11ReferenceRendezvousError {}

impl From<M11DirectBlockError> for M11ReferenceRendezvousError {
    fn from(error: M11DirectBlockError) -> Self {
        Self::Controller(error)
    }
}

impl From<M11BlockWriterError> for M11ReferenceRendezvousError {
    fn from(error: M11BlockWriterError) -> Self {
        Self::Writer(error)
    }
}

impl From<M11RecursiveGreenError> for M11ReferenceRendezvousError {
    fn from(error: M11RecursiveGreenError) -> Self {
        Self::Green(error)
    }
}

impl From<M11ReferenceJournalError> for M11ReferenceRendezvousError {
    fn from(error: M11ReferenceJournalError) -> Self {
        Self::Journal(error)
    }
}

impl From<ReferenceValueCleanerError> for M11ReferenceRendezvousError {
    fn from(error: ReferenceValueCleanerError) -> Self {
        Self::Cleaner(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Barrier,
    Scan,
    Occurrence,
    TerminalRange,
    Rewrite,
    Gap,
    Commit,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OccurrencePhase {
    Source,
    Label,
    ProbeDestination,
    ProbeTitle,
    CountDestination,
    CountTitle,
    BeginJournal,
    EmitDestination,
    EmitTitle,
    AwaitJournal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ValueKind {
    Destination,
    Title,
}

impl ValueKind {
    const fn journal(self) -> M11ReferenceJournalValueKind {
        match self {
            Self::Destination => M11ReferenceJournalValueKind::Destination,
            Self::Title => M11ReferenceJournalValueKind::Title,
        }
    }
}

#[derive(Clone, Debug)]
struct LogicalSpan {
    bytes: Range<u64>,
    utf16: Range<u64>,
}

impl LogicalSpan {
    fn from_direct(
        range: &donor::DirectReferenceLogicalRange,
        base: donor::DirectReferenceLogicalPosition,
    ) -> Result<Self, M11ReferenceRendezvousError> {
        let start_bytes = range.bytes.start.checked_sub(base.bytes).ok_or(
            M11ReferenceRendezvousError::InvalidState("reference range precedes its logical base"),
        )?;
        let end_bytes = range.bytes.end.checked_sub(base.bytes).ok_or(
            M11ReferenceRendezvousError::InvalidState("reference range precedes its logical base"),
        )?;
        let start_utf16 = range.utf16.start.checked_sub(base.utf16).ok_or(
            M11ReferenceRendezvousError::InvalidState("reference range precedes its UTF-16 base"),
        )?;
        let end_utf16 = range.utf16.end.checked_sub(base.utf16).ok_or(
            M11ReferenceRendezvousError::InvalidState("reference range precedes its UTF-16 base"),
        )?;
        if start_bytes > end_bytes || start_utf16 > end_utf16 {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference range is reversed",
            ));
        }
        Ok(Self {
            bytes: start_bytes..end_bytes,
            utf16: start_utf16..end_utf16,
        })
    }

    fn green(&self) -> Result<M11RecursiveGreenLogicalRange, M11ReferenceRendezvousError> {
        let start = M11RecursiveGreenLogicalPosition::new(self.bytes.start, self.utf16.start)
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference start is not a valid logical point",
            ))?;
        let end = M11RecursiveGreenLogicalPosition::new(self.bytes.end, self.utf16.end).ok_or(
            M11ReferenceRendezvousError::InvalidState("reference end is not a valid logical point"),
        )?;
        M11RecursiveGreenLogicalRange::new(start, end).ok_or(
            M11ReferenceRendezvousError::InvalidState("reference range is not monotonic"),
        )
    }

    fn select_ascii(&self, selected: Range<usize>) -> Result<Self, M11ReferenceRendezvousError> {
        let raw_len = usize::try_from(self.bytes.end - self.bytes.start)
            .map_err(|_| M11ReferenceRendezvousError::CounterOverflow)?;
        if selected.start > selected.end || selected.end > raw_len {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference value selection left its source cut",
            ));
        }
        let start = u64::try_from(selected.start)
            .map_err(|_| M11ReferenceRendezvousError::CounterOverflow)?;
        let end = u64::try_from(selected.end)
            .map_err(|_| M11ReferenceRendezvousError::CounterOverflow)?;
        let trailing = u64::try_from(raw_len - selected.end)
            .map_err(|_| M11ReferenceRendezvousError::CounterOverflow)?;
        Ok(Self {
            bytes: (self.bytes.start + start)..(self.bytes.start + end),
            // Destination trimming and title delimiters are ASCII, therefore
            // every removed byte is exactly one logical UTF-16 unit.
            utf16: (self.utf16.start + start)..(self.utf16.end - trailing),
        })
    }
}

#[derive(Default)]
struct ValueProbe {
    destination: DestinationTrimProbe,
    len: usize,
    first: Option<u8>,
    last: Option<u8>,
}

impl ValueProbe {
    fn push(&mut self, kind: ValueKind, byte: u8) -> Result<(), M11ReferenceRendezvousError> {
        self.first.get_or_insert(byte);
        self.last = Some(byte);
        self.len = self
            .len
            .checked_add(1)
            .ok_or(M11ReferenceRendezvousError::CounterOverflow)?;
        if kind == ValueKind::Destination {
            self.destination.push(byte)?;
        }
        Ok(())
    }

    fn finish(self, kind: ValueKind) -> Range<usize> {
        match kind {
            ValueKind::Destination => self.destination.finish(),
            ValueKind::Title => clean_title_body_range(self.len, self.first, self.last),
        }
    }
}

struct CleanPass {
    cursor: M11RecursiveGreenTerminalFragmentCursor,
    cleaner: ReferenceValueBodyCleaner,
    needs_input: bool,
    pending: Option<CleanReferenceValueChunk>,
    pending_offset: usize,
}

struct ActiveOccurrence {
    definition: donor::DirectReferenceDefinition,
    ack: Option<OutputAck>,
    phase: OccurrencePhase,
    replay: Option<M11RecursiveGreenTerminalFragmentCursor>,
    probe: ValueProbe,
    clean: Option<CleanPass>,
    source: Option<M11ReferenceJournalRange>,
    label_source: Option<M11ReferenceJournalRange>,
    destination_source: Option<M11ReferenceJournalRange>,
    title_source: Option<M11ReferenceJournalRange>,
    destination_selected: Option<LogicalSpan>,
    title_selected: Option<LogicalSpan>,
    destination_len: Option<usize>,
    title_len: Option<usize>,
}

impl ActiveOccurrence {
    fn new(definition: donor::DirectReferenceDefinition, ack: OutputAck) -> Self {
        Self {
            definition,
            ack: Some(ack),
            phase: OccurrencePhase::Source,
            replay: None,
            probe: ValueProbe::default(),
            clean: None,
            source: None,
            label_source: None,
            destination_source: None,
            title_source: None,
            destination_selected: None,
            title_selected: None,
            destination_len: None,
            title_len: None,
        }
    }
}

/// One fuelled reference-prefix transaction for the active Paragraph.
#[must_use = "reference rendezvous must be polled to completion"]
pub struct M11ReferenceRendezvous {
    request: donor::DirectReferencePrefixRequest,
    frame: M11RecursiveGreenFrameId,
    staged: Option<M11ReferenceStagedTerminator>,
    phase: Phase,
    binding: Option<M11RecursiveGreenTerminalFragmentBinding>,
    identity: Option<Identity>,
    scan: Option<M11RecursiveGreenTerminalFragmentCursor>,
    work: Option<Work>,
    active: Option<ActiveOccurrence>,
    terminal: Option<TerminalOutput>,
    terminal_replay: Option<M11RecursiveGreenTerminalFragmentCursor>,
    rewrite: Option<M11RecursiveGreenTerminalFragmentRewriteWork>,
}

impl M11ReferenceRendezvous {
    pub fn begin(
        controller: &mut M11DirectBlockController,
        writer: &mut M11BlockWriter,
    ) -> Result<Self, M11ReferenceRendezvousError> {
        let request = controller.pending_reference_prefix_request()?;
        let frame = writer.reference_paragraph_frame()?;
        let staged = writer.reference_staged_terminator(frame)?;
        if request.include_pending_terminator() != staged.is_some() {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "parser and writer disagree about the staged Paragraph terminator",
            ));
        }
        let build = writer.reference_green_build_mut()?;
        let fragment = build.mint_terminal_fragment(frame)?;
        build.begin_terminal_fragment_barrier(fragment)?;
        Ok(Self {
            request,
            frame,
            staged,
            phase: Phase::Barrier,
            binding: None,
            identity: None,
            scan: None,
            work: None,
            active: None,
            terminal: None,
            terminal_replay: None,
            rewrite: None,
        })
    }

    pub fn poll(
        &mut self,
        controller: &mut M11DirectBlockController,
        writer: &mut M11BlockWriter,
        journal: &mut M11ReferenceJournal,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11ReferenceRendezvousPoll, M11ReferenceRendezvousError> {
        if fuel == 0 {
            return Err(M11ReferenceRendezvousError::ZeroFuel);
        }
        if self.phase == Phase::Complete {
            return Ok(M11ReferenceRendezvousPoll {
                transitions: 0,
                status: M11ReferenceRendezvousStatus::Complete,
            });
        }
        if self.phase == Phase::Failed {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference rendezvous is failed",
            ));
        }
        let mut transitions = 0;
        while transitions < fuel && self.phase != Phase::Complete {
            let result = self.drive_one(controller, writer, journal, runtime);
            if let Err(error) = result {
                self.phase = Phase::Failed;
                return Err(error);
            }
            transitions += 1;
        }
        Ok(M11ReferenceRendezvousPoll {
            transitions,
            status: if self.phase == Phase::Complete {
                M11ReferenceRendezvousStatus::Complete
            } else {
                M11ReferenceRendezvousStatus::Pending
            },
        })
    }

    fn drive_one(
        &mut self,
        controller: &mut M11DirectBlockController,
        writer: &mut M11BlockWriter,
        journal: &mut M11ReferenceJournal,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        match self.phase {
            Phase::Barrier => self.poll_barrier(controller, writer, runtime),
            Phase::Scan => self.poll_scan(writer, runtime),
            Phase::Occurrence => self.poll_occurrence(writer, journal, runtime),
            Phase::TerminalRange => self.poll_terminal_range(writer, runtime),
            Phase::Rewrite => self.poll_rewrite(writer, runtime),
            Phase::Gap => self.poll_gap(writer, runtime),
            Phase::Commit => self.commit_terminal(controller),
            Phase::Complete => Ok(()),
            Phase::Failed => Err(M11ReferenceRendezvousError::InvalidState(
                "reference rendezvous is failed",
            )),
        }
    }

    fn poll_barrier(
        &mut self,
        controller: &mut M11DirectBlockController,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let build = writer.reference_green_build_mut()?;
        let poll = build.poll_terminal_fragment_barrier(runtime, 1)?;
        if poll.status() != M11RecursiveGreenTerminalFragmentBarrierStatus::Ready {
            return Ok(());
        }
        let binding = build.take_terminal_fragment_binding()?;
        let identity = binding.identity();
        let scan = build.open_terminal_fragment_cursor(&binding)?;
        let work = controller.begin_reference_prefix_work(self.request, identity)?;
        self.binding = Some(binding);
        self.identity = Some(identity);
        self.scan = Some(scan);
        self.work = Some(work);
        self.phase = Phase::Scan;
        Ok(())
    }

    fn poll_scan(
        &mut self,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let scan = self
            .scan
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference scan cursor disappeared",
            ))?;
        if scan.ready_byte().is_none() && !scan.is_final() {
            let _ = writer
                .reference_green_build_mut()?
                .poll_terminal_fragment_cursor(runtime, scan, 1)?;
        }
        let identity = self
            .identity
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference projection identity disappeared",
            ))?;
        let staged = self.staged;
        let mut source = ProjectedReferenceSource {
            identity,
            cursor: scan,
            virtual_lf: staged.is_some(),
            virtual_raw: staged.map_or(0, |value| value.raw_codepoint_contribution),
        };
        let receipt = self
            .work
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference scanner work disappeared",
            ))?
            .poll_source(&mut source, 1, false)
            .map_err(map_donor_poll_error)?;
        match receipt.status {
            donor::DirectReferencePrefixPollStatus::NeedMore => {}
            donor::DirectReferencePrefixPollStatus::OutputReady => {
                let output = self
                    .work
                    .as_mut()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference output lost its scanner work",
                    ))?
                    .take_output()
                    .map_err(map_infallible_donor_error)?;
                if output.source_identity() != identity {
                    return Err(M11ReferenceRendezvousError::InvalidState(
                        "reference output crossed its projected source",
                    ));
                }
                let (definition, ack) = output.acknowledge();
                if definition.destination_transform
                    != donor::DirectReferenceValueTransform::CleanDestination
                    || definition.title_transform
                        != definition
                            .logical_title
                            .as_ref()
                            .map(|_| donor::DirectReferenceValueTransform::CleanTitle)
                {
                    return Err(M11ReferenceRendezvousError::InvalidState(
                        "reference output selected an unsupported value transform",
                    ));
                }
                self.active = Some(ActiveOccurrence::new(definition, ack));
                self.phase = Phase::Occurrence;
            }
            donor::DirectReferencePrefixPollStatus::Complete => {
                let work = self
                    .work
                    .take()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "completed reference scan lost its work",
                    ))?;
                let terminal = work.take_terminal().map_err(|_| {
                    M11ReferenceRendezvousError::InvalidState(
                        "completed reference scan lost its terminal",
                    )
                })?;
                if terminal.source_identity() != identity {
                    return Err(M11ReferenceRendezvousError::InvalidState(
                        "reference terminal crossed its projected source",
                    ));
                }
                self.terminal = Some(terminal);
                self.begin_terminal_rewrite(writer, runtime)?;
            }
            donor::DirectReferencePrefixPollStatus::Cancelled => {
                return Err(M11ReferenceRendezvousError::InvalidState(
                    "reference scanner was unexpectedly cancelled",
                ));
            }
        }
        Ok(())
    }

    fn poll_occurrence(
        &mut self,
        writer: &mut M11BlockWriter,
        journal: &mut M11ReferenceJournal,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let phase = self
            .active
            .as_ref()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference occurrence disappeared",
            ))?
            .phase;
        match phase {
            OccurrencePhase::Source
            | OccurrencePhase::Label
            | OccurrencePhase::ProbeDestination
            | OccurrencePhase::ProbeTitle => self.poll_occurrence_range(writer, runtime),
            OccurrencePhase::CountDestination | OccurrencePhase::CountTitle => {
                self.poll_occurrence_clean(writer, journal, runtime, false)
            }
            OccurrencePhase::BeginJournal => self.begin_journal(journal, runtime),
            OccurrencePhase::EmitDestination | OccurrencePhase::EmitTitle => {
                self.poll_occurrence_clean(writer, journal, runtime, true)
            }
            OccurrencePhase::AwaitJournal => {
                if !journal.is_idle() {
                    let _ = journal.poll(runtime, 1)?;
                    return Ok(());
                }
                let mut active =
                    self.active
                        .take()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "durable reference occurrence disappeared",
                        ))?;
                let ack = active
                    .ack
                    .take()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "durable reference occurrence lost its parser acknowledgement",
                    ))?;
                let ack_status = self
                    .work
                    .as_mut()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference scanner work disappeared after publication",
                    ))?
                    .acknowledge_output(ack)
                    .map_err(map_infallible_donor_error)?;
                if ack_status == donor::DirectReferencePrefixOutputAckStatus::Complete {
                    let work =
                        self.work
                            .take()
                            .ok_or(M11ReferenceRendezvousError::InvalidState(
                                "completed reference occurrence lost its scanner work",
                            ))?;
                    let terminal = work.take_terminal().map_err(|_| {
                        M11ReferenceRendezvousError::InvalidState(
                            "completed reference occurrence lost its terminal",
                        )
                    })?;
                    self.terminal = Some(terminal);
                    self.begin_terminal_rewrite(writer, runtime)?;
                } else {
                    self.phase = Phase::Scan;
                }
                Ok(())
            }
        }
    }

    fn poll_occurrence_range(
        &mut self,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let base = self.request.logical_base();
        let fragment_end = self.fragment_logical_end()?;
        let staged = self.staged;
        let active = self
            .active
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference occurrence disappeared",
            ))?;
        let (direct, probe_kind) = match active.phase {
            OccurrencePhase::Source => (&active.definition.logical_source, None),
            OccurrencePhase::Label => (&active.definition.logical_label, None),
            OccurrencePhase::ProbeDestination => (
                &active.definition.logical_destination,
                Some(ValueKind::Destination),
            ),
            OccurrencePhase::ProbeTitle => (
                active.definition.logical_title.as_ref().ok_or(
                    M11ReferenceRendezvousError::InvalidState(
                        "reference title phase has no title range",
                    ),
                )?,
                Some(ValueKind::Title),
            ),
            _ => {
                return Err(M11ReferenceRendezvousError::InvalidState(
                    "reference range poll entered a non-range phase",
                ));
            }
        };
        let span = LogicalSpan::from_direct(direct, base)?;
        if active.replay.is_none() {
            let clipped = clip_to_fragment(&span, fragment_end, staged.is_some())?;
            let binding =
                self.binding
                    .as_ref()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference range lost its fragment binding",
                    ))?;
            let build = writer.reference_green_build_mut()?;
            let range = build.bind_terminal_fragment_logical_range(binding, clipped.green()?)?;
            active.replay = Some(build.open_terminal_fragment_range_replay(binding, range)?);
            active.probe = ValueProbe::default();
            return Ok(());
        }
        let replay = active
            .replay
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference replay disappeared",
            ))?;
        let polled = writer
            .reference_green_build_mut()?
            .poll_terminal_fragment_cursor(runtime, replay, 1)?;
        match polled.status() {
            M11RecursiveGreenTerminalFragmentCursorStatus::Pending => Ok(()),
            M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady => {
                let ready =
                    replay
                        .ready_byte()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "reference replay reported no ready byte",
                        ))?;
                let byte = replay.read_byte(ready.relative_offset())?;
                if let Some(kind) = probe_kind {
                    active.probe.push(kind, byte)?;
                }
                Ok(())
            }
            M11RecursiveGreenTerminalFragmentCursorStatus::Complete => {
                let mut completed = active
                    .replay
                    .take()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "completed reference replay disappeared",
                    ))?
                    .take_completed_range()?;
                let physical =
                    completed
                        .physical_range()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "nonempty reference range has no physical envelope",
                        ))?;
                let mut byte_range = physical.byte_range();
                let mut utf16_range = physical.utf16_range();
                let includes_virtual =
                    span.bytes.end > fragment_end.bytes() || span.utf16.end > fragment_end.utf16();
                if includes_virtual {
                    let staged = staged.ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference range escaped Green without a staged terminator",
                    ))?;
                    if byte_range.end != staged.start.bytes()
                        || utf16_range.end != staged.start.utf16()
                    {
                        return Err(M11ReferenceRendezvousError::InvalidState(
                            "reference source did not join its staged terminator",
                        ));
                    }
                    byte_range.end = staged.end.bytes();
                    utf16_range.end = staged.end.utf16();
                }
                let journal_range = M11ReferenceJournalRange::new(byte_range, utf16_range);
                match active.phase {
                    OccurrencePhase::Source => {
                        active.source = Some(journal_range);
                        active.phase = OccurrencePhase::Label;
                    }
                    OccurrencePhase::Label => {
                        active.label_source = Some(journal_range);
                        active.phase = OccurrencePhase::ProbeDestination;
                    }
                    OccurrencePhase::ProbeDestination => {
                        let selected =
                            std::mem::take(&mut active.probe).finish(ValueKind::Destination);
                        active.destination_selected = Some(span.select_ascii(selected)?);
                        active.destination_source = Some(journal_range);
                        active.phase = if active.definition.logical_title.is_some() {
                            OccurrencePhase::ProbeTitle
                        } else {
                            OccurrencePhase::CountDestination
                        };
                    }
                    OccurrencePhase::ProbeTitle => {
                        let selected = std::mem::take(&mut active.probe).finish(ValueKind::Title);
                        active.title_selected = Some(span.select_ascii(selected)?);
                        active.title_source = Some(journal_range);
                        active.phase = OccurrencePhase::CountDestination;
                    }
                    _ => unreachable!("range phase was checked above"),
                }
                // Keep the authenticated authority's lifetime explicit until
                // its physical envelope has been consumed.
                let _ = &mut completed;
                Ok(())
            }
        }
    }

    fn poll_occurrence_clean(
        &mut self,
        writer: &mut M11BlockWriter,
        journal: &mut M11ReferenceJournal,
        runtime: &mut DocumentRuntime,
        emit: bool,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let active = self
            .active
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference occurrence disappeared",
            ))?;
        let kind = match active.phase {
            OccurrencePhase::CountDestination | OccurrencePhase::EmitDestination => {
                ValueKind::Destination
            }
            OccurrencePhase::CountTitle | OccurrencePhase::EmitTitle => ValueKind::Title,
            _ => {
                return Err(M11ReferenceRendezvousError::InvalidState(
                    "reference cleaner entered a non-value phase",
                ));
            }
        };
        if active.clean.is_none() {
            let span = match kind {
                ValueKind::Destination => active.destination_selected.as_ref(),
                ValueKind::Title => active.title_selected.as_ref(),
            }
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference cleaner lost its selected range",
            ))?;
            let binding =
                self.binding
                    .as_ref()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference cleaner lost its fragment binding",
                    ))?;
            let build = writer.reference_green_build_mut()?;
            let range = build.bind_terminal_fragment_logical_range(binding, span.green()?)?;
            let cursor = build.open_terminal_fragment_range_replay(binding, range)?;
            active.clean = Some(CleanPass {
                cursor,
                cleaner: ReferenceValueBodyCleaner::new(),
                needs_input: true,
                pending: None,
                pending_offset: 0,
            });
            return Ok(());
        }
        let clean = active
            .clean
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference cleaner disappeared",
            ))?;
        if emit && clean.pending.is_some() {
            let capacity = journal.stream_capacity(kind.journal())?;
            if capacity == 0 {
                let _ = journal.poll(runtime, 1)?;
                return Ok(());
            }
            let (consumed, output_len) = {
                let bytes = clean
                    .pending
                    .as_ref()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference cleaner output disappeared",
                    ))?
                    .bytes();
                let end = clean
                    .pending_offset
                    .saturating_add(capacity)
                    .min(bytes.len());
                (
                    journal
                        .offer_stream_bytes(kind.journal(), &bytes[clean.pending_offset..end])?,
                    bytes.len(),
                )
            };
            if consumed == 0 {
                return Err(M11ReferenceRendezvousError::InvalidState(
                    "reference journal accepted zero bytes with positive capacity",
                ));
            }
            clean.pending_offset = clean
                .pending_offset
                .checked_add(consumed)
                .ok_or(M11ReferenceRendezvousError::CounterOverflow)?;
            if clean.pending_offset == output_len {
                clean.pending = None;
                clean.pending_offset = 0;
            }
            return Ok(());
        }
        if clean.needs_input {
            let polled = writer
                .reference_green_build_mut()?
                .poll_terminal_fragment_cursor(runtime, &mut clean.cursor, 1)?;
            match polled.status() {
                M11RecursiveGreenTerminalFragmentCursorStatus::Pending => return Ok(()),
                M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady => {
                    let ready = clean.cursor.ready_byte().ok_or(
                        M11ReferenceRendezvousError::InvalidState(
                            "reference cleaner source reported no byte",
                        ),
                    )?;
                    let byte = clean.cursor.read_byte(ready.relative_offset())?;
                    clean.cleaner.offer_byte(byte)?;
                    clean.needs_input = false;
                    return Ok(());
                }
                M11RecursiveGreenTerminalFragmentCursorStatus::Complete => {
                    let _ = clean.cursor.take_completed_range()?;
                    clean.cleaner.finish_input()?;
                    clean.needs_input = false;
                    return Ok(());
                }
            }
        }
        match clean.cleaner.poll()? {
            ReferenceValueCleanerStatus::Progress => Ok(()),
            ReferenceValueCleanerStatus::NeedInput => {
                clean.needs_input = true;
                Ok(())
            }
            ReferenceValueCleanerStatus::OutputReady => {
                let output = clean.cleaner.take_output()?;
                if emit {
                    clean.pending = Some(output);
                    clean.pending_offset = 0;
                }
                Ok(())
            }
            ReferenceValueCleanerStatus::Complete => {
                let cooked_len = usize::try_from(clean.cleaner.receipt().output_bytes)
                    .map_err(|_| M11ReferenceRendezvousError::CounterOverflow)?;
                active.clean = None;
                if emit {
                    let declared = match kind {
                        ValueKind::Destination => active.destination_len,
                        ValueKind::Title => active.title_len,
                    }
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference emit pass lost its counted length",
                    ))?;
                    if declared != cooked_len {
                        return Err(M11ReferenceRendezvousError::InvalidState(
                            "reference count and emit passes diverged",
                        ));
                    }
                    active.phase = if kind == ValueKind::Destination
                        && active.definition.logical_title.is_some()
                    {
                        OccurrencePhase::EmitTitle
                    } else {
                        OccurrencePhase::AwaitJournal
                    };
                } else {
                    match kind {
                        ValueKind::Destination => active.destination_len = Some(cooked_len),
                        ValueKind::Title => active.title_len = Some(cooked_len),
                    }
                    active.phase = if kind == ValueKind::Destination
                        && active.definition.logical_title.is_some()
                    {
                        OccurrencePhase::CountTitle
                    } else {
                        OccurrencePhase::BeginJournal
                    };
                }
                Ok(())
            }
        }
    }

    fn begin_journal(
        &mut self,
        journal: &mut M11ReferenceJournal,
        runtime: &DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        if !journal.is_idle() {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference journal was not idle at the occurrence boundary",
            ));
        }
        let active = self
            .active
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference occurrence disappeared",
            ))?;
        let normalized = std::mem::take(&mut active.definition.normalized_label)
            .into_bytes()
            .into_boxed_slice();
        journal.begin_occurrence_stream(
            runtime,
            M11ReferenceJournalOccurrenceStart::new(
                active
                    .source
                    .take()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference occurrence lost its source range",
                    ))?,
                active
                    .label_source
                    .take()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference occurrence lost its label range",
                    ))?,
                active.destination_source.take().ok_or(
                    M11ReferenceRendezvousError::InvalidState(
                        "reference occurrence lost its destination range",
                    ),
                )?,
                active.title_source.take(),
                normalized,
                active
                    .destination_len
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference occurrence lost its destination length",
                    ))?,
                active.title_len,
            ),
        )?;
        active.phase = OccurrencePhase::EmitDestination;
        Ok(())
    }

    fn begin_terminal_rewrite(
        &mut self,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let terminal = self
            .terminal
            .as_ref()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference terminal disappeared",
            ))?
            .terminal()
            .clone();
        if terminal.disposition == donor::DirectReferencePrefixDisposition::NoDefinitions {
            let binding = self
                .binding
                .take()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "unchanged reference terminal lost its binding",
                ))?;
            self.rewrite = Some(
                writer
                    .reference_green_build_mut()?
                    .begin_terminal_fragment_rewrite(
                        runtime,
                        binding,
                        M11RecursiveGreenTerminalFragmentRewrite::Unchanged,
                    )?,
            );
            self.phase = Phase::Rewrite;
            return Ok(());
        }
        let span =
            if terminal.disposition == donor::DirectReferencePrefixDisposition::VisibleRemainder {
                LogicalSpan::from_direct(
                    &terminal.logical_reference_prefix,
                    self.request.logical_base(),
                )?
            } else {
                let end = self.fragment_logical_end()?;
                LogicalSpan {
                    bytes: 0..end.bytes(),
                    utf16: 0..end.utf16(),
                }
            };
        let binding = self
            .binding
            .as_ref()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference terminal lost its fragment binding",
            ))?;
        let build = writer.reference_green_build_mut()?;
        let range = build.bind_terminal_fragment_logical_range(binding, span.green()?)?;
        self.terminal_replay = Some(build.open_terminal_fragment_range_replay(binding, range)?);
        self.phase = Phase::TerminalRange;
        Ok(())
    }

    fn poll_terminal_range(
        &mut self,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let replay =
            self.terminal_replay
                .as_mut()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "terminal range replay disappeared",
                ))?;
        match writer
            .reference_green_build_mut()?
            .poll_terminal_fragment_cursor(runtime, replay, 1)?
            .status()
        {
            M11RecursiveGreenTerminalFragmentCursorStatus::Pending => Ok(()),
            M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady => {
                let ready =
                    replay
                        .ready_byte()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "terminal range replay reported no byte",
                        ))?;
                let _ = replay.read_byte(ready.relative_offset())?;
                Ok(())
            }
            M11RecursiveGreenTerminalFragmentCursorStatus::Complete => {
                let range = self
                    .terminal_replay
                    .take()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "completed terminal range disappeared",
                    ))?
                    .take_completed_range()?;
                let terminal = self
                    .terminal
                    .as_ref()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "terminal rewrite lost its parser disposition",
                    ))?
                    .terminal();
                let rewrite = if terminal.disposition
                    == donor::DirectReferencePrefixDisposition::VisibleRemainder
                    || terminal.disposition
                        == donor::DirectReferencePrefixDisposition::ReferenceOnly
                        && self.request.context()
                            == donor::DirectReferencePrefixContext::SetextCandidate
                {
                    M11RecursiveGreenTerminalFragmentRewrite::RetainVisibleSuffix {
                        removed_prefix: range,
                    }
                } else {
                    M11RecursiveGreenTerminalFragmentRewrite::RemoveWrapper {
                        whole_fragment: range,
                    }
                };
                let binding =
                    self.binding
                        .take()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "terminal rewrite lost its fragment binding",
                        ))?;
                self.rewrite = Some(
                    writer
                        .reference_green_build_mut()?
                        .begin_terminal_fragment_rewrite(runtime, binding, rewrite)?,
                );
                self.phase = Phase::Rewrite;
                Ok(())
            }
        }
    }

    fn poll_rewrite(
        &mut self,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let rewrite = self
            .rewrite
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference rewrite work disappeared",
            ))?;
        let poll = writer
            .reference_green_build_mut()?
            .poll_terminal_fragment_rewrite(runtime, rewrite, 1)?;
        let M11RecursiveGreenTerminalFragmentRewritePoll::Complete { authority, .. } = poll else {
            return Ok(());
        };
        if authority.frame() != self.frame {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference rewrite returned the wrong Paragraph frame",
            ));
        }
        self.rewrite = None;
        let terminal = self
            .terminal
            .as_ref()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference rewrite lost its parser terminal",
            ))?
            .terminal();
        let reference_only =
            terminal.disposition == donor::DirectReferencePrefixDisposition::ReferenceOnly;
        let remove = reference_only
            && self.request.context() == donor::DirectReferencePrefixContext::ParagraphFinalization;
        let expected = if remove {
            M11RecursiveGreenTerminalFragmentDisposition::Removed
        } else {
            M11RecursiveGreenTerminalFragmentDisposition::Surviving
        };
        if authority.disposition() != expected {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference rewrite disposition disagrees with parser chronology",
            ));
        }
        let gap = writer.complete_reference_fragment(
            self.frame,
            remove,
            reference_only && self.staged.is_some(),
        )?;
        if let Some(gap) = gap {
            writer.reference_green_build_mut()?.offer_event(gap)?;
            self.phase = Phase::Gap;
        } else {
            self.phase = Phase::Commit;
        }
        Ok(())
    }

    fn poll_gap(
        &mut self,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let poll = writer.reference_green_build_mut()?.poll(runtime, 1)?;
        if poll.status() == M11RecursiveGreenBuildStatus::NeedsInput {
            self.phase = Phase::Commit;
        }
        Ok(())
    }

    fn commit_terminal(
        &mut self,
        controller: &mut M11DirectBlockController,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let identity = self
            .identity
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference commit lost its projection identity",
            ))?;
        let terminal = self
            .terminal
            .take()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference commit lost its terminal",
            ))?;
        let disposition = terminal.terminal().disposition;
        let status =
            controller.commit_reference_prefix_terminal(terminal.acknowledge(), identity)?;
        let valid = matches!(
            (disposition, status),
            (
                donor::DirectReferencePrefixDisposition::NoDefinitions,
                donor::DirectReferencePrefixCommitStatus::ParagraphUnchangedArmed
            ) | (
                donor::DirectReferencePrefixDisposition::VisibleRemainder,
                donor::DirectReferencePrefixCommitStatus::VisibleRemainderArmed
            ) | (
                donor::DirectReferencePrefixDisposition::ReferenceOnly,
                donor::DirectReferencePrefixCommitStatus::ReferenceOnlyArmed
            )
        );
        if !valid {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference parser commit disagrees with its terminal",
            ));
        }
        self.phase = Phase::Complete;
        Ok(())
    }

    fn fragment_logical_end(
        &self,
    ) -> Result<M11RecursiveGreenLogicalPosition, M11ReferenceRendezvousError> {
        self.scan
            .as_ref()
            .map(M11RecursiveGreenTerminalFragmentCursor::logical_position)
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference fragment lost its scan cursor",
            ))
    }
}

struct ProjectedReferenceSource<'a> {
    identity: Identity,
    cursor: &'a mut M11RecursiveGreenTerminalFragmentCursor,
    virtual_lf: bool,
    virtual_raw: u8,
}

fn clip_to_fragment(
    span: &LogicalSpan,
    physical_end: M11RecursiveGreenLogicalPosition,
    has_staged_terminator: bool,
) -> Result<LogicalSpan, M11ReferenceRendezvousError> {
    if span.bytes.end <= physical_end.bytes() && span.utf16.end <= physical_end.utf16() {
        return Ok(span.clone());
    }
    if !has_staged_terminator
        || span.bytes.end != physical_end.bytes().saturating_add(1)
        || span.utf16.end != physical_end.utf16().saturating_add(1)
        || span.bytes.start > physical_end.bytes()
        || span.utf16.start > physical_end.utf16()
    {
        return Err(M11ReferenceRendezvousError::InvalidState(
            "reference range escaped the frozen fragment",
        ));
    }
    Ok(LogicalSpan {
        bytes: span.bytes.start..physical_end.bytes(),
        utf16: span.utf16.start..physical_end.utf16(),
    })
}

impl donor::DirectReferencePrefixSource for ProjectedReferenceSource<'_> {
    type Identity = Identity;
    type Error = M11RecursiveGreenError;

    fn identity(&self) -> Self::Identity {
        self.identity
    }

    fn available_len(&self) -> usize {
        let physical = usize::try_from(self.cursor.available_len()).unwrap_or(usize::MAX);
        physical.saturating_add(usize::from(self.cursor.is_final() && self.virtual_lf))
    }

    fn is_final(&self) -> bool {
        self.cursor.is_final()
    }

    fn access_budget(&self) -> usize {
        usize::from(self.cursor.ready_byte().is_some() || self.cursor.is_final() && self.virtual_lf)
    }

    fn read_byte(&mut self, relative_offset: usize) -> Result<u8, Self::Error> {
        let physical = usize::try_from(self.cursor.available_len())
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        if self.cursor.is_final() && self.virtual_lf && relative_offset == physical {
            return Ok(b'\n');
        }
        self.cursor.read_byte(
            u64::try_from(relative_offset).map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
        )
    }

    fn raw_codepoint_contribution(&self, logical_scalar_end_offset: usize) -> u8 {
        let physical = usize::try_from(self.cursor.available_len()).unwrap_or(usize::MAX);
        if self.cursor.is_final() && self.virtual_lf && logical_scalar_end_offset == physical {
            self.virtual_raw
        } else {
            u64::try_from(logical_scalar_end_offset)
                .ok()
                .map_or(0, |offset| self.cursor.raw_codepoint_contribution(offset))
        }
    }
}

fn map_donor_poll_error(
    error: donor::DirectReferencePrefixPollError<M11RecursiveGreenError>,
) -> M11ReferenceRendezvousError {
    match error {
        donor::DirectReferencePrefixPollError::Source(error) => error.into(),
        donor::DirectReferencePrefixPollError::ZeroFuel => {
            M11ReferenceRendezvousError::InvalidState("reference scanner received zero fuel")
        }
        donor::DirectReferencePrefixPollError::WrongSource => {
            M11ReferenceRendezvousError::InvalidState("reference scanner crossed source identity")
        }
        donor::DirectReferencePrefixPollError::SourceBudgetContractViolated => {
            M11ReferenceRendezvousError::InvalidState("reference source exceeded its access grant")
        }
        donor::DirectReferencePrefixPollError::NonSequentialSource => {
            M11ReferenceRendezvousError::InvalidState("reference source was not sequential")
        }
        donor::DirectReferencePrefixPollError::InvalidUtf8 { .. } => {
            M11ReferenceRendezvousError::InvalidState("reference projection is invalid UTF-8")
        }
        donor::DirectReferencePrefixPollError::InvalidRawCodepointContribution { .. } => {
            M11ReferenceRendezvousError::InvalidState(
                "reference projection has an invalid raw-codepoint contribution",
            )
        }
        donor::DirectReferencePrefixPollError::PollAfterComplete => {
            M11ReferenceRendezvousError::InvalidState(
                "reference scanner was polled after completion",
            )
        }
        donor::DirectReferencePrefixPollError::PollAfterCancelled => {
            M11ReferenceRendezvousError::InvalidState(
                "reference scanner was polled after cancellation",
            )
        }
        donor::DirectReferencePrefixPollError::OutputNotAcknowledged => {
            M11ReferenceRendezvousError::InvalidState("reference output was not acknowledged")
        }
        donor::DirectReferencePrefixPollError::OutputNotReady => {
            M11ReferenceRendezvousError::InvalidState("reference output was not ready")
        }
        donor::DirectReferencePrefixPollError::WrongOutput => {
            M11ReferenceRendezvousError::InvalidState("reference output acknowledgement was wrong")
        }
        donor::DirectReferencePrefixPollError::CounterOverflow => {
            M11ReferenceRendezvousError::CounterOverflow
        }
    }
}

fn map_infallible_donor_error(
    _error: donor::DirectReferencePrefixPollError<std::convert::Infallible>,
) -> M11ReferenceRendezvousError {
    M11ReferenceRendezvousError::InvalidState(
        "reference scanner rejected its linear acknowledgement",
    )
}

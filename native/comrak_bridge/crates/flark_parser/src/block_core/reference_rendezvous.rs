//! Atomic join between reference recognition, persistent semantics, and Green.
//!
//! The donor owns CommonMark recognition, the frozen terminal Green fragment
//! owns projected source, and `M11ReferenceJournal` owns ordered first-winner
//! semantics.  This actor is the only place those three linear capabilities
//! meet.  It never materializes a Paragraph or an unbounded destination/title.

use std::fmt;
use std::ops::Range;

use flark_block_core_donor as donor;
use flark_block_core_donor::DirectReferencePrefixSource;
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
    SourcePrefix,
    Label,
    LabelDestinationGap,
    Destination,
    DestinationTitleGap,
    Title,
    SourceSuffix,
    BeginJournal,
    EmitDestination,
    EmitTitle,
    AwaitJournal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SegmentKind {
    SourcePrefix,
    Label,
    Gap,
    Destination,
    Title,
    SourceSuffix,
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
}

const COOKED_SCRATCH_PAGE_BYTES: usize = 4 * 1024;
// This is the exact production `ReferenceRootLimits` per-fact bound. The
// rendezvous enforces it before retaining cooked scratch, then the journal
// independently preflights the same bound before accepting the occurrence.
const MAX_COOKED_REFERENCE_FACT_BYTES: usize = 16 * 1024 * 1024;

struct CookedScratch {
    pages: Vec<Box<[u8]>>,
    len: usize,
    maximum: usize,
}

impl CookedScratch {
    fn new(maximum: usize) -> Self {
        Self {
            pages: Vec::new(),
            len: 0,
            maximum,
        }
    }

    fn append(&mut self, mut bytes: &[u8]) -> Result<(), M11ReferenceRendezvousError> {
        let target = self
            .len
            .checked_add(bytes.len())
            .ok_or(M11ReferenceRendezvousError::CounterOverflow)?;
        if target > self.maximum {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference cooked value exceeds its hard per-fact bound",
            ));
        }
        while !bytes.is_empty() {
            let page_offset = self.len % COOKED_SCRATCH_PAGE_BYTES;
            if page_offset == 0 {
                self.pages.try_reserve(1).map_err(|_| {
                    M11ReferenceRendezvousError::InvalidState(
                        "reference cooked scratch allocation failed",
                    )
                })?;
                let mut page = Vec::new();
                page.try_reserve_exact(COOKED_SCRATCH_PAGE_BYTES)
                    .map_err(|_| {
                        M11ReferenceRendezvousError::InvalidState(
                            "reference cooked scratch allocation failed",
                        )
                    })?;
                page.resize(COOKED_SCRATCH_PAGE_BYTES, 0);
                self.pages.push(page.into_boxed_slice());
            }
            let take = bytes.len().min(COOKED_SCRATCH_PAGE_BYTES - page_offset);
            let page = self
                .pages
                .last_mut()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "reference cooked scratch lost its page",
                ))?;
            page[page_offset..page_offset + take].copy_from_slice(&bytes[..take]);
            self.len += take;
            bytes = &bytes[take..];
        }
        Ok(())
    }

    fn remaining_from(&self, offset: usize, maximum: usize) -> &[u8] {
        debug_assert!(offset < self.len);
        let page_index = offset / COOKED_SCRATCH_PAGE_BYTES;
        let page_offset = offset % COOKED_SCRATCH_PAGE_BYTES;
        let available = (self.len - offset)
            .min(COOKED_SCRATCH_PAGE_BYTES - page_offset)
            .min(maximum);
        &self.pages[page_index][page_offset..page_offset + available]
    }

    const fn len(&self) -> usize {
        self.len
    }
}

enum StreamingValueMode {
    Destination {
        saw_non_space: bool,
        pending_spaces: usize,
        pending_non_space: Option<u8>,
    },
    Title {
        saw_first: bool,
        expected_close: Option<u8>,
        held_last: Option<u8>,
        pending_feed: Option<u8>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StreamingValuePoll {
    NeedsSource,
    Progress,
    Complete,
}

struct StreamingValueCook {
    mode: StreamingValueMode,
    cleaner: ReferenceValueBodyCleaner,
    cleaner_needs_input: bool,
    source_finished: bool,
    finish_sent: bool,
    complete: bool,
    output: CookedScratch,
}

impl StreamingValueCook {
    fn new(kind: ValueKind, maximum: usize) -> Self {
        Self {
            mode: match kind {
                ValueKind::Destination => StreamingValueMode::Destination {
                    saw_non_space: false,
                    pending_spaces: 0,
                    pending_non_space: None,
                },
                ValueKind::Title => StreamingValueMode::Title {
                    saw_first: false,
                    expected_close: None,
                    held_last: None,
                    pending_feed: None,
                },
            },
            cleaner: ReferenceValueBodyCleaner::new(),
            cleaner_needs_input: true,
            source_finished: false,
            finish_sent: false,
            complete: false,
            output: CookedScratch::new(maximum),
        }
    }

    fn can_accept_source(&self) -> bool {
        if self.source_finished {
            return false;
        }
        match &self.mode {
            StreamingValueMode::Destination {
                pending_non_space, ..
            } => pending_non_space.is_none(),
            StreamingValueMode::Title { pending_feed, .. } => pending_feed.is_none(),
        }
    }

    fn offer_source_byte(&mut self, byte: u8) -> Result<(), M11ReferenceRendezvousError> {
        if !self.can_accept_source() {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference value source advanced before its cleaner",
            ));
        }
        match &mut self.mode {
            StreamingValueMode::Destination {
                saw_non_space,
                pending_spaces,
                pending_non_space,
            } => {
                if is_comrak_space(byte) {
                    if *saw_non_space {
                        *pending_spaces = pending_spaces
                            .checked_add(1)
                            .ok_or(M11ReferenceRendezvousError::CounterOverflow)?;
                    }
                } else {
                    *saw_non_space = true;
                    *pending_non_space = Some(byte);
                }
            }
            StreamingValueMode::Title {
                saw_first,
                expected_close,
                held_last,
                pending_feed,
            } => {
                if !*saw_first {
                    *saw_first = true;
                    *expected_close = match byte {
                        b'\'' | b'"' => Some(byte),
                        b'(' => Some(b')'),
                        _ => {
                            *pending_feed = Some(byte);
                            None
                        }
                    };
                } else if expected_close.is_some() {
                    if let Some(previous) = held_last.replace(byte) {
                        *pending_feed = Some(previous);
                    }
                } else {
                    *pending_feed = Some(byte);
                }
            }
        }
        Ok(())
    }

    fn finish_source(&mut self) -> Result<(), M11ReferenceRendezvousError> {
        if self.source_finished {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference value source finished twice",
            ));
        }
        match &mut self.mode {
            StreamingValueMode::Destination { pending_spaces, .. } => {
                // These are trailing spaces. Internal runs were retained until
                // the next non-space proved that they belong to the body.
                *pending_spaces = 0;
            }
            StreamingValueMode::Title {
                saw_first,
                expected_close,
                held_last,
                ..
            } => {
                if !*saw_first {
                    return Err(M11ReferenceRendezvousError::InvalidState(
                        "reference title source is empty",
                    ));
                }
                if let Some(expected) = expected_close {
                    if held_last.take() != Some(*expected) {
                        return Err(M11ReferenceRendezvousError::InvalidState(
                            "reference title delimiters changed after recognition",
                        ));
                    }
                }
            }
        }
        self.source_finished = true;
        Ok(())
    }

    fn poll_one(&mut self) -> Result<StreamingValuePoll, M11ReferenceRendezvousError> {
        if self.complete {
            return Ok(StreamingValuePoll::Complete);
        }
        if !self.cleaner_needs_input {
            return match self.cleaner.poll()? {
                ReferenceValueCleanerStatus::Progress => Ok(StreamingValuePoll::Progress),
                ReferenceValueCleanerStatus::NeedInput => {
                    self.cleaner_needs_input = true;
                    Ok(StreamingValuePoll::Progress)
                }
                ReferenceValueCleanerStatus::OutputReady => {
                    let chunk = self.cleaner.take_output()?;
                    self.output.append(chunk.bytes())?;
                    Ok(StreamingValuePoll::Progress)
                }
                ReferenceValueCleanerStatus::Complete => {
                    self.complete = true;
                    Ok(StreamingValuePoll::Complete)
                }
            };
        }

        let next = match &mut self.mode {
            StreamingValueMode::Destination {
                pending_spaces,
                pending_non_space,
                ..
            } => {
                if pending_non_space.is_some() && *pending_spaces > 0 {
                    *pending_spaces -= 1;
                    Some(b' ')
                } else {
                    pending_non_space.take()
                }
            }
            StreamingValueMode::Title { pending_feed, .. } => pending_feed.take(),
        };
        if let Some(byte) = next {
            self.cleaner.offer_byte(byte)?;
            self.cleaner_needs_input = false;
            return Ok(StreamingValuePoll::Progress);
        }
        if !self.source_finished {
            return Ok(StreamingValuePoll::NeedsSource);
        }
        if !self.finish_sent {
            self.cleaner.finish_input()?;
            self.cleaner_needs_input = false;
            self.finish_sent = true;
            return Ok(StreamingValuePoll::Progress);
        }
        Err(M11ReferenceRendezvousError::InvalidState(
            "reference cleaner requested input after source completion",
        ))
    }

    fn take_output(&mut self) -> Result<CookedScratch, M11ReferenceRendezvousError> {
        if !self.complete {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference cooked scratch was taken before completion",
            ));
        }
        Ok(std::mem::replace(&mut self.output, CookedScratch::new(0)))
    }
}

fn is_comrak_space(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\r' | b' ')
}

#[derive(Clone, Debug)]
struct PhysicalEnvelope {
    bytes: Range<u64>,
    utf16: Range<u64>,
}

impl PhysicalEnvelope {
    fn include(&mut self, bytes: Range<u64>, utf16: Range<u64>) {
        self.bytes.end = bytes.end;
        self.utf16.end = utf16.end;
    }

    fn into_journal(self) -> M11ReferenceJournalRange {
        M11ReferenceJournalRange::new(self.bytes, self.utf16)
    }
}

struct ActiveOccurrence {
    definition: donor::DirectReferenceDefinition,
    ack: Option<OutputAck>,
    phase: OccurrencePhase,
    segment_started: bool,
    value_cook: Option<StreamingValueCook>,
    source_envelope: Option<PhysicalEnvelope>,
    source: Option<M11ReferenceJournalRange>,
    label_source: Option<M11ReferenceJournalRange>,
    destination_source: Option<M11ReferenceJournalRange>,
    title_source: Option<M11ReferenceJournalRange>,
    cooked_destination: Option<CookedScratch>,
    cooked_title: Option<CookedScratch>,
    emit_offset: usize,
}

impl ActiveOccurrence {
    fn new(definition: donor::DirectReferenceDefinition, ack: OutputAck) -> Self {
        Self {
            definition,
            ack: Some(ack),
            phase: OccurrencePhase::SourcePrefix,
            segment_started: false,
            value_cook: None,
            source_envelope: None,
            source: None,
            label_source: None,
            destination_source: None,
            title_source: None,
            cooked_destination: None,
            cooked_title: None,
            emit_offset: 0,
        }
    }
}

fn logical_span(byte_start: u64, byte_end: u64, utf16_start: u64, utf16_end: u64) -> LogicalSpan {
    LogicalSpan {
        bytes: byte_start..byte_end,
        utf16: utf16_start..utf16_end,
    }
}

fn advance_occurrence_segment(
    active: &mut ActiveOccurrence,
    has_title: bool,
) -> Result<(), M11ReferenceRendezvousError> {
    active.segment_started = false;
    active.phase = match active.phase {
        OccurrencePhase::SourcePrefix => OccurrencePhase::Label,
        OccurrencePhase::Label => OccurrencePhase::LabelDestinationGap,
        OccurrencePhase::LabelDestinationGap => OccurrencePhase::Destination,
        OccurrencePhase::Destination => {
            if has_title {
                OccurrencePhase::DestinationTitleGap
            } else {
                OccurrencePhase::SourceSuffix
            }
        }
        OccurrencePhase::DestinationTitleGap => OccurrencePhase::Title,
        OccurrencePhase::Title => OccurrencePhase::SourceSuffix,
        OccurrencePhase::SourceSuffix => OccurrencePhase::BeginJournal,
        _ => {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference segment completed outside the segment transaction",
            ));
        }
    };
    Ok(())
}

fn finalize_occurrence_source(
    active: &mut ActiveOccurrence,
    base: donor::DirectReferenceLogicalPosition,
    fragment_end: M11RecursiveGreenLogicalPosition,
    staged: Option<M11ReferenceStagedTerminator>,
) -> Result<(), M11ReferenceRendezvousError> {
    let source = LogicalSpan::from_direct(&active.definition.logical_source, base)?;
    let mut envelope =
        active
            .source_envelope
            .take()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference source traversal produced no physical envelope",
            ))?;
    if source.bytes.end > fragment_end.bytes() || source.utf16.end > fragment_end.utf16() {
        let staged = staged.ok_or(M11ReferenceRendezvousError::InvalidState(
            "reference source escaped Green without a staged terminator",
        ))?;
        if envelope.bytes.end != staged.start.bytes() || envelope.utf16.end != staged.start.utf16()
        {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference source did not join its staged terminator",
            ));
        }
        envelope.bytes.end = staged.end.bytes();
        envelope.utf16.end = staged.end.utf16();
    }
    active.source = Some(envelope.into_journal());
    Ok(())
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
    range_replay: Option<M11RecursiveGreenTerminalFragmentCursor>,
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
            range_replay: None,
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
        if scan.ready_chunk().is_empty() && !scan.is_final() {
            let _ = writer
                .reference_green_build_mut()?
                .poll_terminal_fragment_cursor_chunk(runtime, scan, 1)?;
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
        let scan_fuel = source
            .access_budget()
            .min(flark_engine::SOURCE_CURSOR_WINDOW_BYTES)
            .max(1);
        let receipt = self
            .work
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference scanner work disappeared",
            ))?
            .poll_source(&mut source, scan_fuel, false)
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
            OccurrencePhase::SourcePrefix
            | OccurrencePhase::Label
            | OccurrencePhase::LabelDestinationGap
            | OccurrencePhase::Destination
            | OccurrencePhase::DestinationTitleGap
            | OccurrencePhase::Title
            | OccurrencePhase::SourceSuffix => self.poll_occurrence_segment(writer, runtime),
            OccurrencePhase::BeginJournal => self.begin_journal(journal, runtime),
            OccurrencePhase::EmitDestination | OccurrencePhase::EmitTitle => {
                self.poll_occurrence_scratch(journal, runtime)
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

    fn poll_occurrence_segment(
        &mut self,
        writer: &mut M11BlockWriter,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let base = self.request.logical_base();
        let fragment_end = self.fragment_logical_end()?;
        let (kind, span, has_title) = {
            let active = self
                .active
                .as_ref()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "reference occurrence disappeared",
                ))?;
            let source = LogicalSpan::from_direct(&active.definition.logical_source, base)?;
            let label = LogicalSpan::from_direct(&active.definition.logical_label, base)?;
            let destination =
                LogicalSpan::from_direct(&active.definition.logical_destination, base)?;
            let title = active
                .definition
                .logical_title
                .as_ref()
                .map(|range| LogicalSpan::from_direct(range, base))
                .transpose()?;
            let selected = match active.phase {
                OccurrencePhase::SourcePrefix => (
                    SegmentKind::SourcePrefix,
                    logical_span(
                        source.bytes.start,
                        label.bytes.start,
                        source.utf16.start,
                        label.utf16.start,
                    ),
                ),
                OccurrencePhase::Label => (SegmentKind::Label, label.clone()),
                OccurrencePhase::LabelDestinationGap => (
                    SegmentKind::Gap,
                    logical_span(
                        label.bytes.end,
                        destination.bytes.start,
                        label.utf16.end,
                        destination.utf16.start,
                    ),
                ),
                OccurrencePhase::Destination => (SegmentKind::Destination, destination.clone()),
                OccurrencePhase::DestinationTitleGap => {
                    let title = title
                        .as_ref()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "reference title gap has no title",
                        ))?;
                    (
                        SegmentKind::Gap,
                        logical_span(
                            destination.bytes.end,
                            title.bytes.start,
                            destination.utf16.end,
                            title.utf16.start,
                        ),
                    )
                }
                OccurrencePhase::Title => (
                    SegmentKind::Title,
                    title
                        .clone()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "reference title phase has no title",
                        ))?,
                ),
                OccurrencePhase::SourceSuffix => {
                    let start = title.as_ref().unwrap_or(&destination);
                    (
                        SegmentKind::SourceSuffix,
                        logical_span(
                            start.bytes.end,
                            source.bytes.end,
                            start.utf16.end,
                            source.utf16.end,
                        ),
                    )
                }
                _ => {
                    return Err(M11ReferenceRendezvousError::InvalidState(
                        "reference segment entered a non-segment phase",
                    ));
                }
            };
            (selected.0, selected.1, title.is_some())
        };
        let clipped = clip_to_fragment(&span, fragment_end, self.staged.is_some())?;
        let empty =
            clipped.bytes.start == clipped.bytes.end && clipped.utf16.start == clipped.utf16.end;

        if matches!(kind, SegmentKind::Destination | SegmentKind::Title) {
            let active = self
                .active
                .as_mut()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "reference occurrence disappeared",
                ))?;
            if active.value_cook.is_none() {
                let normalized_len = active.definition.normalized_label.as_bytes().len();
                let already_cooked = active
                    .cooked_destination
                    .as_ref()
                    .map_or(0, CookedScratch::len);
                let maximum = MAX_COOKED_REFERENCE_FACT_BYTES
                    .checked_sub(normalized_len)
                    .and_then(|remaining| remaining.checked_sub(already_cooked))
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference cooked values exceed their hard per-fact bound",
                    ))?;
                active.value_cook = Some(StreamingValueCook::new(
                    if kind == SegmentKind::Destination {
                        ValueKind::Destination
                    } else {
                        ValueKind::Title
                    },
                    maximum,
                ));
            }
            match active
                .value_cook
                .as_mut()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "reference value cleaner disappeared",
                ))?
                .poll_one()?
            {
                StreamingValuePoll::Progress => return Ok(()),
                StreamingValuePoll::Complete => {
                    let output = active
                        .value_cook
                        .as_mut()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "reference value cleaner disappeared",
                        ))?
                        .take_output()?;
                    active.value_cook = None;
                    if kind == SegmentKind::Destination {
                        active.cooked_destination = Some(output);
                    } else {
                        active.cooked_title = Some(output);
                    }
                    advance_occurrence_segment(active, has_title)?;
                    return Ok(());
                }
                StreamingValuePoll::NeedsSource => {}
            }
        }

        let segment_started = self
            .active
            .as_ref()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference occurrence disappeared",
            ))?
            .segment_started;
        if !segment_started {
            if empty {
                let active =
                    self.active
                        .as_mut()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "reference occurrence disappeared",
                        ))?;
                active.segment_started = true;
                if let Some(cook) = active.value_cook.as_mut() {
                    cook.finish_source()?;
                } else {
                    if active.phase == OccurrencePhase::SourceSuffix {
                        finalize_occurrence_source(active, base, fragment_end, self.staged)?;
                    }
                    advance_occurrence_segment(active, has_title)?;
                }
                return Ok(());
            }
            let binding =
                self.binding
                    .as_ref()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference segment lost its fragment binding",
                    ))?;
            let build = writer.reference_green_build_mut()?;
            let range = build.bind_terminal_fragment_logical_range(binding, clipped.green()?)?;
            if let Some(replay) = self.range_replay.as_mut() {
                build.retarget_terminal_fragment_range_replay_forward(binding, replay, range)?;
            } else {
                self.range_replay =
                    Some(build.open_terminal_fragment_range_replay(binding, range)?);
            }
            self.active
                .as_mut()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "reference occurrence disappeared",
                ))?
                .segment_started = true;
            return Ok(());
        }

        let replay =
            self.range_replay
                .as_mut()
                .ok_or(M11ReferenceRendezvousError::InvalidState(
                    "reference forward replay disappeared",
                ))?;
        let build = writer.reference_green_build_mut()?;
        let polled = if matches!(kind, SegmentKind::Destination | SegmentKind::Title) {
            build.poll_terminal_fragment_cursor(runtime, replay, 1)?
        } else {
            build.poll_terminal_fragment_cursor_chunk(runtime, replay, 1)?
        };
        match polled.status() {
            M11RecursiveGreenTerminalFragmentCursorStatus::Pending => Ok(()),
            M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady => {
                if let Some(cook) = self
                    .active
                    .as_mut()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference occurrence disappeared",
                    ))?
                    .value_cook
                    .as_mut()
                {
                    let ready = replay.ready_byte().ok_or(
                        M11ReferenceRendezvousError::InvalidState(
                            "reference value replay reported no ready byte",
                        ),
                    )?;
                    let byte = replay.read_byte(ready.relative_offset())?;
                    cook.offer_source_byte(byte)?;
                } else {
                    let ready = replay.ready_chunk().len();
                    if ready == 0 {
                        return Err(M11ReferenceRendezvousError::InvalidState(
                            "reference range replay reported an empty ready chunk",
                        ));
                    }
                    replay.consume_ready_prefix(ready)?;
                }
                Ok(())
            }
            M11RecursiveGreenTerminalFragmentCursorStatus::Complete => {
                let completed = replay.take_completed_range()?;
                let physical =
                    completed
                        .physical_range()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "nonempty reference segment has no physical envelope",
                        ))?;
                let bytes = physical.byte_range();
                let utf16 = physical.utf16_range();
                let active =
                    self.active
                        .as_mut()
                        .ok_or(M11ReferenceRendezvousError::InvalidState(
                            "reference occurrence disappeared",
                        ))?;
                match active.source_envelope.as_mut() {
                    Some(envelope) => envelope.include(bytes.clone(), utf16.clone()),
                    None => {
                        active.source_envelope = Some(PhysicalEnvelope {
                            bytes: bytes.clone(),
                            utf16: utf16.clone(),
                        });
                    }
                }
                match kind {
                    SegmentKind::Label => {
                        active.label_source = Some(M11ReferenceJournalRange::new(bytes, utf16));
                    }
                    SegmentKind::Destination => {
                        active.destination_source =
                            Some(M11ReferenceJournalRange::new(bytes, utf16));
                    }
                    SegmentKind::Title => {
                        active.title_source = Some(M11ReferenceJournalRange::new(bytes, utf16));
                    }
                    SegmentKind::SourcePrefix | SegmentKind::Gap | SegmentKind::SourceSuffix => {}
                }
                if let Some(cook) = active.value_cook.as_mut() {
                    cook.finish_source()?;
                } else {
                    if active.phase == OccurrencePhase::SourceSuffix {
                        finalize_occurrence_source(active, base, fragment_end, self.staged)?;
                    }
                    advance_occurrence_segment(active, has_title)?;
                }
                Ok(())
            }
        }
    }

    fn poll_occurrence_scratch(
        &mut self,
        journal: &mut M11ReferenceJournal,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11ReferenceRendezvousError> {
        let active = self
            .active
            .as_mut()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference occurrence disappeared",
            ))?;
        let (kind, scratch) = match active.phase {
            OccurrencePhase::EmitDestination => (
                ValueKind::Destination,
                active.cooked_destination.as_ref().ok_or(
                    M11ReferenceRendezvousError::InvalidState(
                        "reference occurrence lost its cooked destination",
                    ),
                )?,
            ),
            OccurrencePhase::EmitTitle => (
                ValueKind::Title,
                active
                    .cooked_title
                    .as_ref()
                    .ok_or(M11ReferenceRendezvousError::InvalidState(
                        "reference occurrence lost its cooked title",
                    ))?,
            ),
            _ => {
                return Err(M11ReferenceRendezvousError::InvalidState(
                    "reference scratch entered a non-emission phase",
                ));
            }
        };
        if active.emit_offset == scratch.len() {
            active.emit_offset = 0;
            active.phase = if kind == ValueKind::Destination && active.cooked_title.is_some() {
                OccurrencePhase::EmitTitle
            } else {
                OccurrencePhase::AwaitJournal
            };
            return Ok(());
        }
        let capacity = journal.stream_capacity(kind.journal())?;
        if capacity == 0 {
            let _ = journal.poll(runtime, 1)?;
            return Ok(());
        }
        let bytes = scratch.remaining_from(active.emit_offset, capacity);
        let consumed = journal.offer_stream_bytes(kind.journal(), bytes)?;
        if consumed == 0 {
            return Err(M11ReferenceRendezvousError::InvalidState(
                "reference journal accepted zero retained bytes with positive capacity",
            ));
        }
        active.emit_offset = active
            .emit_offset
            .checked_add(consumed)
            .ok_or(M11ReferenceRendezvousError::CounterOverflow)?;
        Ok(())
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
        let destination_len = active
            .cooked_destination
            .as_ref()
            .ok_or(M11ReferenceRendezvousError::InvalidState(
                "reference occurrence lost its cooked destination",
            ))?
            .len();
        let title_len = active.cooked_title.as_ref().map(CookedScratch::len);
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
                destination_len,
                title_len,
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
        // Every occurrence range has already been consumed monotonically.
        // The final structural rewrite performs one independent linear prefix
        // validation, never one replay per occurrence.
        self.range_replay = None;
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
            .poll_terminal_fragment_cursor_chunk(runtime, replay, 1)?
            .status()
        {
            M11RecursiveGreenTerminalFragmentCursorStatus::Pending => Ok(()),
            M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady => {
                let ready = replay.ready_chunk().len();
                if ready == 0 {
                    return Err(M11ReferenceRendezvousError::InvalidState(
                        "terminal range replay reported an empty ready chunk",
                    ));
                }
                replay.consume_ready_prefix(ready)?;
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
        self.cursor.ready_chunk().len().saturating_add(usize::from(
            self.cursor.is_final() && self.virtual_lf,
        ))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistent_recursive_green_session::{
        M11PersistentRecursiveGreenBuildStatus, M11PersistentRecursiveGreenCleanPlan,
    };
    use flark_engine::DocumentRuntimeConfig;

    fn cook_once(kind: ValueKind, source: &[u8]) -> Vec<u8> {
        let mut cook = StreamingValueCook::new(kind, 1024);
        let mut source_offset = 0;
        loop {
            match cook.poll_one().expect("single-pass value cook") {
                StreamingValuePoll::NeedsSource if source_offset < source.len() => {
                    cook.offer_source_byte(source[source_offset])
                        .expect("offer value source");
                    source_offset += 1;
                }
                StreamingValuePoll::NeedsSource => {
                    cook.finish_source().expect("finish value source");
                }
                StreamingValuePoll::Progress => {}
                StreamingValuePoll::Complete => break,
            }
        }
        let scratch = cook.take_output().expect("take cooked scratch");
        let mut output = Vec::with_capacity(scratch.len());
        let mut offset = 0;
        while offset < scratch.len() {
            let bytes = scratch.remaining_from(offset, usize::MAX);
            output.extend_from_slice(bytes);
            offset += bytes.len();
        }
        output
    }

    fn same_paragraph_reference_transitions(definitions: usize) -> usize {
        let mut source = String::new();
        for ordinal in 0..definitions {
            use std::fmt::Write as _;
            writeln!(&mut source, "[ref-{ordinal}]: /target-{ordinal}")
                .expect("reference fixture write");
        }
        let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
            .expect("reference slope runtime");
        let plan = M11PersistentRecursiveGreenCleanPlan::new(
            runtime.snapshot_current_source().expect("scanner lease"),
            runtime.snapshot_current_source().expect("writer lease"),
            1,
        )
        .expect("reference slope plan");
        let mut build = plan.begin(&mut runtime).expect("reference slope build");
        let mut transitions = 0_usize;
        loop {
            let poll = build.poll(&mut runtime, 1).expect("reference slope poll");
            transitions = transitions
                .checked_add(poll.transitions())
                .expect("reference slope transition count");
            if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
                break;
            }
        }
        let mut session = build.take_session().expect("reference slope session");
        assert_eq!(session.reference_occurrence_count(), definitions as u64);
        session
            .begin_release(&mut runtime)
            .expect("begin session release");
        while !session
            .poll_release(&mut runtime, 64)
            .expect("poll session release")
        {}
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(64).expect("poll runtime close").complete {}
        transitions
    }

    #[test]
    fn same_paragraph_reference_work_has_linear_doubling_slope() {
        let small = same_paragraph_reference_transitions(32);
        let doubled = same_paragraph_reference_transitions(64);
        eprintln!(
            "same_paragraph_reference_slope definitions=32 transitions={small} \
             definitions=64 transitions={doubled} ratio={:.3}",
            doubled as f64 / small as f64,
        );
        assert!(
            doubled < small * 3,
            "doubling same-Paragraph definitions grew from {small} to {doubled} transitions"
        );
    }

    #[test]
    fn single_pass_value_cooking_preserves_trim_title_entity_and_escape_semantics() {
        assert_eq!(
            cook_once(ValueKind::Destination, b" \t/a&amp;b\\* \r"),
            b"/a&b*"
        );
        assert_eq!(cook_once(ValueKind::Title, b"\"a&amp;b\\*\""), b"a&b*");
    }
}

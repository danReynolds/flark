//! Parser-owned resumable recognition of a leading reference-definition run.
//!
//! The DFA is a streaming correspondent of the pinned donor's
//! `link_label + spnl + manual_scan_link_url + link_title` sequence.  It owns
//! no paragraph `String`, URL, title, line queue, or definition vector.  One
//! spec-bounded label is retained so the pinned donor can normalize it; large
//! destination and title values remain exact source ranges with transform
//! recipes.

use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

use flark_reference_label_service::{
    MAX_NORMALIZED_REFERENCE_LABEL_BYTES, ReferenceLabelAccumulator, is_reference_label_whitespace,
};

/// Maximum raw source bytes retained while one UTF-8 scalar straddles refills.
/// Complete label text is never retained.
pub const DIRECT_REFERENCE_LABEL_MAX_RETAINED_BYTES: usize = 4;
/// Complete normalized-output allocation admitted before scanner polling.
pub const DIRECT_REFERENCE_LABEL_MAX_NORMALIZED_BYTES: usize = MAX_NORMALIZED_REFERENCE_LABEL_BYTES;
static DIRECT_REFERENCE_WORK_IDS: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirectReferenceLogicalPosition {
    pub bytes: u64,
    pub utf16: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectReferenceLogicalRange {
    pub bytes: Range<u64>,
    pub utf16: Range<u64>,
}

impl DirectReferenceLogicalRange {
    fn new(start: DirectReferenceLogicalPosition, end: DirectReferenceLogicalPosition) -> Self {
        Self {
            bytes: start.bytes..end.bytes,
            utf16: start.utf16..end.utf16,
        }
    }
}

/// Deferred exact donor value transform for a source-backed field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectReferenceValueTransform {
    CleanDestination,
    CleanTitle,
}

/// One ordered reference occurrence.  The normalized label is bounded by the
/// CommonMark limit; destination/title payloads are deliberately absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectReferenceDefinition {
    /// Parser-authenticated logical cuts.  The active writer resolves these
    /// through its source/projection capability; they are not physical ranges.
    pub logical_source: DirectReferenceLogicalRange,
    pub logical_label: DirectReferenceLogicalRange,
    pub logical_destination: DirectReferenceLogicalRange,
    pub logical_title: Option<DirectReferenceLogicalRange>,
    pub normalized_label: String,
    pub destination_transform: DirectReferenceValueTransform,
    pub title_transform: Option<DirectReferenceValueTransform>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectReferencePrefixDisposition {
    NoDefinitions,
    ReferenceOnly,
    VisibleRemainder,
}

/// Terminal summary for the recognized logical run.  `recognition` can extend
/// beyond `reference_prefix` only when donor lookahead disproved a title; that
/// suffix must replay ordinarily and is never retained here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectReferencePrefixTerminal {
    pub disposition: DirectReferencePrefixDisposition,
    pub definition_count: u64,
    pub logical_reference_prefix: DirectReferenceLogicalRange,
    pub logical_recognition: DirectReferenceLogicalRange,
}

pub trait DirectReferencePrefixSource {
    type Identity: Copy + Eq;
    type Error;

    fn identity(&self) -> Self::Identity;
    /// Bytes currently readable from the work's source-relative byte zero.
    fn available_len(&self) -> usize;
    /// Whether `available_len` is a definitive logical-run EOF.
    fn is_final(&self) -> bool;
    fn access_budget(&self) -> usize;
    /// Reads exactly the requested source-relative byte.  Work requests are
    /// monotonic; sources may reject any rewind.
    fn read_byte(&mut self, relative_offset: usize) -> Result<u8, Self::Error>;
    /// Number of raw source Unicode scalar values represented by the logical
    /// scalar ending at this byte. Identity text contributes one, a canonical
    /// LF projected from physical CRLF contributes two, continuation units of
    /// one projected tab contribute zero, and true synthetic units contribute
    /// zero. The source/writer capability, not the DFA, authenticates this
    /// cumulative-boundary delta.
    fn raw_codepoint_contribution(&self, logical_scalar_end_offset: usize) -> u8;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DirectReferencePrefixPollStatus {
    #[default]
    NeedMore,
    OutputReady,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirectReferencePrefixPollReceipt {
    pub status: DirectReferencePrefixPollStatus,
    pub inspected_bytes: usize,
    pub source_first_reads: usize,
    pub logical_high_water: u64,
    pub retained_source_bytes: usize,
    pub source_budget_exhausted: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DirectReferencePrefixPollError<SourceError> {
    ZeroFuel,
    WrongSource,
    Source(SourceError),
    SourceBudgetContractViolated,
    NonSequentialSource,
    InvalidUtf8 {
        relative_offset: usize,
    },
    InvalidRawCodepointContribution {
        relative_offset: usize,
        contribution: u8,
    },
    PollAfterComplete,
    PollAfterCancelled,
    OutputNotAcknowledged,
    OutputNotReady,
    WrongOutput,
    CounterOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Label,
    Colon,
    BeforeDestination,
    AngleDestination,
    AngleClosed,
    BareDestination,
    AfterDestination,
    QuotedTitle,
    ParenTitle,
    AfterTitle,
    AfterTitleCr,
}

#[derive(Debug)]
struct Scanner {
    start: DirectReferenceLogicalPosition,
    phase: Phase,
    label_trimmed_start: Option<DirectReferenceLogicalPosition>,
    label_trimmed_end: DirectReferenceLogicalPosition,
    label_accumulator: ReferenceLabelAccumulator,
    label_pending: [u8; 4],
    label_pending_len: u8,
    label_pending_start: DirectReferenceLogicalPosition,
    label_backslash: bool,
    destination_start: DirectReferenceLogicalPosition,
    destination_end: DirectReferenceLogicalPosition,
    destination_bytes: u64,
    bare_depth: u8,
    bare_backslash: bool,
    angle_backslash: bool,
    before_destination_newline: bool,
    before_destination_cr: bool,
    after_destination_separator: bool,
    after_destination_newline: bool,
    after_destination_cr: bool,
    fallback_source_end: Option<DirectReferenceLogicalPosition>,
    title_start: Option<DirectReferenceLogicalPosition>,
    title_end: Option<DirectReferenceLogicalPosition>,
    title_closer: u8,
    title_backslash: bool,
}

impl Scanner {
    fn new(start: DirectReferenceLogicalPosition) -> Self {
        Self {
            start,
            phase: Phase::Label,
            label_trimmed_start: None,
            label_trimmed_end: DirectReferenceLogicalPosition {
                bytes: start.bytes + 1,
                utf16: start.utf16 + 1,
            },
            label_accumulator: ReferenceLabelAccumulator::new_preflighted(),
            label_pending: [0; 4],
            label_pending_len: 0,
            label_pending_start: start,
            label_backslash: false,
            destination_start: start,
            destination_end: start,
            destination_bytes: 0,
            bare_depth: 0,
            bare_backslash: false,
            angle_backslash: false,
            before_destination_newline: false,
            before_destination_cr: false,
            after_destination_separator: false,
            after_destination_newline: false,
            after_destination_cr: false,
            fallback_source_end: None,
            title_start: None,
            title_end: None,
            title_closer: 0,
            title_backslash: false,
        }
    }

    fn retained_source_bytes(&self) -> usize {
        usize::from(self.label_pending_len)
    }
}

#[derive(Debug)]
enum Stage {
    Scanning(Box<Scanner>),
    OutputReady {
        definition: DirectReferenceDefinition,
        source_end: DirectReferenceLogicalPosition,
        resume: DirectReferencePrefixOutputResume,
    },
    AwaitingOutputAck {
        key: DirectReferencePrefixOutputKey,
        source_end: DirectReferenceLogicalPosition,
        resume: DirectReferencePrefixOutputResume,
    },
    Complete(DirectReferencePrefixTerminal),
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug)]
struct DirectReferencePrefixReplayUnit {
    before: DirectReferenceLogicalPosition,
    after: DirectReferenceLogicalPosition,
    byte: u8,
    raw_codepoint_contribution: u8,
}

#[derive(Clone, Copy, Debug)]
enum DirectReferencePrefixOutputResume {
    ScanAtSourceEnd,
    Replay(DirectReferencePrefixReplayUnit),
    VisibleRemainder(DirectReferenceLogicalPosition),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DirectReferencePrefixOutputKey {
    parser_instance_id: u64,
    rendezvous_id: u64,
    work_id: u64,
    sequence: u64,
    source_end: DirectReferenceLogicalPosition,
    recognition_high_water: DirectReferenceLogicalPosition,
}

/// Non-cloneable one-occurrence capability.  The active writer consumes this
/// value while publishing the occurrence and returns its acknowledgement;
/// the work cannot poll again in between.
pub struct DirectReferencePrefixOutput<I: Copy + Eq> {
    key: DirectReferencePrefixOutputKey,
    source_identity: I,
    definition: DirectReferenceDefinition,
}

impl<I: Copy + Eq> DirectReferencePrefixOutput<I> {
    /// Read-only provenance check for the writer rendezvous. The output stays
    /// linear; observing its source identity cannot acknowledge or replay it.
    #[must_use]
    pub const fn source_identity(&self) -> I {
        self.source_identity
    }

    #[must_use]
    pub const fn definition(&self) -> &DirectReferenceDefinition {
        &self.definition
    }

    /// Model the consuming writer acknowledgement boundary.  Production
    /// callers invoke this only after durable occurrence publication succeeds.
    #[must_use]
    pub fn acknowledge(self) -> (DirectReferenceDefinition, DirectReferencePrefixOutputAck<I>) {
        (
            self.definition,
            DirectReferencePrefixOutputAck {
                key: self.key,
                source_identity: self.source_identity,
            },
        )
    }
}

/// Non-cloneable writer receipt that rearms exactly the work which minted it.
pub struct DirectReferencePrefixOutputAck<I: Copy + Eq> {
    key: DirectReferencePrefixOutputKey,
    source_identity: I,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectReferencePrefixOutputAckStatus {
    Rearmed,
    Complete,
}

/// Terminal capability consumed after the writer has applied prefix removal,
/// visible-remainder retention, or reference-only wrapper removal.
pub struct DirectReferencePrefixTerminalOutput<I: Copy + Eq> {
    parser_instance_id: u64,
    rendezvous_id: u64,
    work_id: u64,
    source_identity: I,
    terminal: DirectReferencePrefixTerminal,
}

impl<I: Copy + Eq> DirectReferencePrefixTerminalOutput<I> {
    #[must_use]
    pub const fn source_identity(&self) -> I {
        self.source_identity
    }

    #[must_use]
    pub const fn terminal(&self) -> &DirectReferencePrefixTerminal {
        &self.terminal
    }

    #[must_use]
    pub fn acknowledge(self) -> DirectReferencePrefixTerminalAck<I> {
        DirectReferencePrefixTerminalAck {
            parser_instance_id: self.parser_instance_id,
            rendezvous_id: self.rendezvous_id,
            work_id: self.work_id,
            source_identity: self.source_identity,
            terminal: self.terminal,
        }
    }
}

/// Writer-authenticated terminal join consumed by the parser.
pub struct DirectReferencePrefixTerminalAck<I: Copy + Eq> {
    pub(crate) parser_instance_id: u64,
    pub(crate) rendezvous_id: u64,
    pub(crate) work_id: u64,
    pub(crate) source_identity: I,
    pub(crate) terminal: DirectReferencePrefixTerminal,
}

#[derive(Clone, Copy, Debug, Default)]
struct Utf8Metric {
    utf16: u64,
    remaining: u8,
    codepoint: u32,
    minimum: u32,
}

impl Utf8Metric {
    fn consume(&mut self, byte: u8) -> Result<(), ()> {
        if self.remaining == 0 {
            match byte {
                0x00..=0x7f => {
                    self.utf16 = self.utf16.checked_add(1).ok_or(())?;
                }
                0xc2..=0xdf => {
                    self.remaining = 1;
                    self.codepoint = u32::from(byte & 0x1f);
                    self.minimum = 0x80;
                }
                0xe0..=0xef => {
                    self.remaining = 2;
                    self.codepoint = u32::from(byte & 0x0f);
                    self.minimum = 0x800;
                }
                0xf0..=0xf4 => {
                    self.remaining = 3;
                    self.codepoint = u32::from(byte & 0x07);
                    self.minimum = 0x1_0000;
                }
                _ => return Err(()),
            }
            return Ok(());
        }
        if !(0x80..=0xbf).contains(&byte) {
            return Err(());
        }
        self.codepoint = (self.codepoint << 6) | u32::from(byte & 0x3f);
        self.remaining -= 1;
        if self.remaining == 0 {
            if self.codepoint < self.minimum
                || (0xd800..=0xdfff).contains(&self.codepoint)
                || self.codepoint > 0x10_ffff
            {
                return Err(());
            }
            self.utf16 = self
                .utf16
                .checked_add(if self.codepoint > 0xffff { 2 } else { 1 })
                .ok_or(())?;
        }
        Ok(())
    }

    const fn is_complete(self) -> bool {
        self.remaining == 0
    }
}

/// Non-cloneable parser-owned source cursor for a leading definition run.
pub struct DirectReferencePrefixWork<I: Copy + Eq> {
    pub(crate) parser_instance_id: u64,
    pub(crate) rendezvous_id: u64,
    work_id: u64,
    source_identity: I,
    logical_base: DirectReferenceLogicalPosition,
    cursor: usize,
    metric: Utf8Metric,
    prefix_end: DirectReferenceLogicalPosition,
    definition_count: u64,
    pending_replay: Option<DirectReferencePrefixReplayUnit>,
    last_consumed: Option<DirectReferencePrefixReplayUnit>,
    stage: Stage,
}

impl<I: Copy + Eq> DirectReferencePrefixWork<I> {
    pub(crate) fn new(
        parser_instance_id: u64,
        rendezvous_id: u64,
        source_identity: I,
        logical_base: DirectReferenceLogicalPosition,
    ) -> Self {
        Self {
            parser_instance_id,
            rendezvous_id,
            work_id: DIRECT_REFERENCE_WORK_IDS.fetch_add(1, Ordering::Relaxed),
            source_identity,
            logical_base,
            cursor: 0,
            metric: Utf8Metric {
                utf16: logical_base.utf16,
                ..Utf8Metric::default()
            },
            prefix_end: logical_base,
            definition_count: 0,
            pending_replay: None,
            last_consumed: None,
            stage: Stage::Scanning(Box::new(Scanner::new(logical_base))),
        }
    }

    #[must_use]
    pub const fn source_identity(&self) -> I {
        self.source_identity
    }

    pub(crate) const fn work_id(&self) -> u64 {
        self.work_id
    }

    #[must_use]
    pub fn retained_source_bytes(&self) -> usize {
        match &self.stage {
            Stage::Scanning(scanner) => scanner.retained_source_bytes(),
            Stage::OutputReady { .. } => 0,
            Stage::AwaitingOutputAck { .. } => 0,
            Stage::Complete(_) | Stage::Cancelled | Stage::Failed => 0,
        }
    }

    #[must_use]
    pub fn terminal(&self) -> Option<&DirectReferencePrefixTerminal> {
        match &self.stage {
            Stage::Complete(terminal) => Some(terminal),
            _ => None,
        }
    }

    /// Move one occurrence into a non-cloneable writer capability.  Work is
    /// frozen until the matching acknowledgement returns.
    pub fn take_output(
        &mut self,
    ) -> Result<
        DirectReferencePrefixOutput<I>,
        DirectReferencePrefixPollError<std::convert::Infallible>,
    > {
        let stage = std::mem::replace(&mut self.stage, Stage::Failed);
        match stage {
            Stage::OutputReady {
                definition,
                source_end,
                resume,
            } => {
                let current = DirectReferenceLogicalPosition {
                    bytes: self.logical_base.bytes + self.cursor as u64,
                    utf16: self.metric.utf16,
                };
                let key = DirectReferencePrefixOutputKey {
                    parser_instance_id: self.parser_instance_id,
                    rendezvous_id: self.rendezvous_id,
                    work_id: self.work_id,
                    sequence: self.definition_count,
                    source_end,
                    recognition_high_water: current,
                };
                self.stage = Stage::AwaitingOutputAck {
                    key,
                    source_end,
                    resume,
                };
                Ok(DirectReferencePrefixOutput {
                    key,
                    source_identity: self.source_identity,
                    definition,
                })
            }
            other => {
                self.stage = other;
                Err(DirectReferencePrefixPollError::OutputNotReady)
            }
        }
    }

    /// Rearm this work after the active writer consumed its exact output.
    pub fn acknowledge_output(
        &mut self,
        ack: DirectReferencePrefixOutputAck<I>,
    ) -> Result<
        DirectReferencePrefixOutputAckStatus,
        DirectReferencePrefixPollError<std::convert::Infallible>,
    > {
        let (key, source_end, resume) = match self.stage {
            Stage::AwaitingOutputAck {
                key,
                source_end,
                resume,
            } => (key, source_end, resume),
            _ => return Err(DirectReferencePrefixPollError::WrongOutput),
        };
        if ack.key != key || ack.source_identity != self.source_identity {
            return Err(DirectReferencePrefixPollError::WrongOutput);
        }
        self.prefix_end = source_end;
        self.definition_count = self
            .definition_count
            .checked_add(1)
            .ok_or(DirectReferencePrefixPollError::CounterOverflow)?;
        match resume {
            DirectReferencePrefixOutputResume::ScanAtSourceEnd => {
                self.stage = Stage::Scanning(Box::new(Scanner::new(source_end)));
                return Ok(DirectReferencePrefixOutputAckStatus::Rearmed);
            }
            DirectReferencePrefixOutputResume::Replay(replay) => {
                self.pending_replay = Some(replay);
                self.stage = Stage::Scanning(Box::new(Scanner::new(source_end)));
                return Ok(DirectReferencePrefixOutputAckStatus::Rearmed);
            }
            DirectReferencePrefixOutputResume::VisibleRemainder(recognition_high_water) => {
                self.stage = Stage::Complete(DirectReferencePrefixTerminal {
                    disposition: DirectReferencePrefixDisposition::VisibleRemainder,
                    definition_count: self.definition_count,
                    logical_reference_prefix: DirectReferenceLogicalRange::new(
                        self.logical_base,
                        source_end,
                    ),
                    logical_recognition: DirectReferenceLogicalRange::new(
                        self.logical_base,
                        recognition_high_water,
                    ),
                });
            }
        }
        Ok(DirectReferencePrefixOutputAckStatus::Complete)
    }

    /// Consume a completed work into its writer terminal capability.
    pub fn take_terminal(self) -> Result<DirectReferencePrefixTerminalOutput<I>, Self> {
        if !matches!(&self.stage, Stage::Complete(_)) {
            return Err(self);
        }
        let Self {
            parser_instance_id,
            rendezvous_id,
            work_id,
            source_identity,
            stage,
            ..
        } = self;
        let Stage::Complete(terminal) = stage else {
            unreachable!("completed stage was checked before consuming work")
        };
        Ok(DirectReferencePrefixTerminalOutput {
            parser_instance_id,
            rendezvous_id,
            work_id,
            source_identity,
            terminal,
        })
    }

    /// Permanently cancel actor-owned work, invalidating any outstanding ack.
    pub fn cancel(&mut self) {
        self.stage = Stage::Cancelled;
    }

    /// Advance within caller and source budgets.  `cancelled` is sampled at
    /// entry and after bounded work so cancellation latency is at most one
    /// poll grant.
    pub fn poll_source<S>(
        &mut self,
        source: &mut S,
        fuel: usize,
        cancelled: bool,
    ) -> Result<DirectReferencePrefixPollReceipt, DirectReferencePrefixPollError<S::Error>>
    where
        S: DirectReferencePrefixSource<Identity = I>,
    {
        if fuel == 0 {
            return Err(DirectReferencePrefixPollError::ZeroFuel);
        }
        if source.identity() != self.source_identity {
            return Err(DirectReferencePrefixPollError::WrongSource);
        }
        match self.stage {
            Stage::OutputReady { .. } | Stage::AwaitingOutputAck { .. } => {
                return Err(DirectReferencePrefixPollError::OutputNotAcknowledged);
            }
            Stage::Complete(_) => return Err(DirectReferencePrefixPollError::PollAfterComplete),
            Stage::Cancelled => return Err(DirectReferencePrefixPollError::PollAfterCancelled),
            Stage::Failed => return Err(DirectReferencePrefixPollError::PollAfterComplete),
            Stage::Scanning(_) => {}
        }
        if cancelled {
            self.stage = Stage::Cancelled;
            return Ok(self.receipt(DirectReferencePrefixPollStatus::Cancelled, 0, 0, false));
        }

        let access_grant = source.access_budget();
        let mut inspected = 0;
        let mut source_reads = 0;
        while inspected < fuel {
            if let Some(replay) = self.pending_replay.take() {
                inspected += 1;
                self.consume_byte(
                    replay.before,
                    replay.after,
                    replay.byte,
                    replay.raw_codepoint_contribution,
                )?;
                if !matches!(self.stage, Stage::Scanning(_)) {
                    break;
                }
                continue;
            }
            let available = source.available_len();
            if self.cursor > available {
                self.stage = Stage::Failed;
                return Err(DirectReferencePrefixPollError::NonSequentialSource);
            }
            if self.cursor == available {
                if source.is_final() {
                    self.finish_at_eof::<S::Error>()?;
                }
                break;
            }
            if source_reads == access_grant {
                break;
            }
            let relative = self.cursor;
            let byte = source
                .read_byte(relative)
                .map_err(DirectReferencePrefixPollError::Source)?;
            let before = self.position()?;
            self.metric.consume(byte).map_err(|()| {
                self.stage = Stage::Failed;
                DirectReferencePrefixPollError::InvalidUtf8 {
                    relative_offset: relative,
                }
            })?;
            self.cursor += 1;
            inspected += 1;
            source_reads += 1;
            let after = self.position()?;
            let raw_codepoint_contribution = if after.utf16 != before.utf16 {
                source.raw_codepoint_contribution(relative)
            } else {
                0
            };
            if after.utf16 != before.utf16 && raw_codepoint_contribution > 2 {
                self.stage = Stage::Failed;
                return Err(
                    DirectReferencePrefixPollError::InvalidRawCodepointContribution {
                        relative_offset: relative,
                        contribution: raw_codepoint_contribution,
                    },
                );
            }
            self.consume_byte(before, after, byte, raw_codepoint_contribution)?;
            if !matches!(self.stage, Stage::Scanning(_)) {
                break;
            }
        }
        if source_reads > access_grant {
            self.stage = Stage::Failed;
            return Err(DirectReferencePrefixPollError::SourceBudgetContractViolated);
        }
        if cancelled {
            self.stage = Stage::Cancelled;
            return Ok(self.receipt(
                DirectReferencePrefixPollStatus::Cancelled,
                inspected,
                source_reads,
                false,
            ));
        }
        let status = match self.stage {
            Stage::OutputReady { .. } => DirectReferencePrefixPollStatus::OutputReady,
            Stage::AwaitingOutputAck { .. } => {
                return Err(DirectReferencePrefixPollError::OutputNotAcknowledged);
            }
            Stage::Complete(_) => DirectReferencePrefixPollStatus::Complete,
            Stage::Cancelled => DirectReferencePrefixPollStatus::Cancelled,
            Stage::Scanning(_) | Stage::Failed => DirectReferencePrefixPollStatus::NeedMore,
        };
        let exhausted = source_reads == access_grant && inspected < fuel;
        Ok(self.receipt(status, inspected, source_reads, exhausted))
    }

    fn receipt(
        &self,
        status: DirectReferencePrefixPollStatus,
        inspected: usize,
        source_first_reads: usize,
        source_budget_exhausted: bool,
    ) -> DirectReferencePrefixPollReceipt {
        DirectReferencePrefixPollReceipt {
            status,
            inspected_bytes: inspected,
            source_first_reads,
            logical_high_water: self.logical_base.bytes + self.cursor as u64,
            retained_source_bytes: self.retained_source_bytes(),
            source_budget_exhausted,
        }
    }

    fn position<SourceError>(
        &self,
    ) -> Result<DirectReferenceLogicalPosition, DirectReferencePrefixPollError<SourceError>> {
        Ok(DirectReferenceLogicalPosition {
            bytes: self
                .logical_base
                .bytes
                .checked_add(
                    u64::try_from(self.cursor)
                        .map_err(|_| DirectReferencePrefixPollError::CounterOverflow)?,
                )
                .ok_or(DirectReferencePrefixPollError::CounterOverflow)?,
            utf16: self.metric.utf16,
        })
    }

    fn consume_byte<SourceError>(
        &mut self,
        before: DirectReferenceLogicalPosition,
        after: DirectReferenceLogicalPosition,
        byte: u8,
        raw_codepoint_contribution: u8,
    ) -> Result<(), DirectReferencePrefixPollError<SourceError>> {
        self.last_consumed = Some(DirectReferencePrefixReplayUnit {
            before,
            after,
            byte,
            raw_codepoint_contribution,
        });
        let mut scanner = match std::mem::replace(&mut self.stage, Stage::Failed) {
            Stage::Scanning(scanner) => scanner,
            _ => return Err(DirectReferencePrefixPollError::PollAfterComplete),
        };
        let decision = match scanner.phase {
            Phase::Label => self.consume_label(
                &mut scanner,
                before,
                after,
                byte,
                raw_codepoint_contribution,
            ),
            Phase::Colon => {
                if byte == b':' {
                    scanner.phase = Phase::BeforeDestination;
                    Decision::Continue
                } else {
                    Decision::Reject
                }
            }
            Phase::BeforeDestination => {
                self.consume_before_destination(&mut scanner, before, after, byte)
            }
            Phase::AngleDestination => self.consume_angle_destination(&mut scanner, before, byte),
            Phase::AngleClosed => {
                scanner.phase = Phase::AfterDestination;
                self.consume_after_destination(&mut scanner, before, after, byte)
            }
            Phase::BareDestination => {
                self.consume_bare_destination(&mut scanner, before, after, byte)
            }
            Phase::AfterDestination => {
                self.consume_after_destination(&mut scanner, before, after, byte)
            }
            Phase::QuotedTitle => self.consume_quoted_title(&mut scanner, after, byte),
            Phase::ParenTitle => self.consume_paren_title(&mut scanner, after, byte),
            Phase::AfterTitle => self.consume_after_title(&mut scanner, before, after, byte),
            Phase::AfterTitleCr => {
                let end = if byte == b'\n' { after } else { before };
                Decision::Accept(end)
            }
        };
        self.apply_decision(scanner, decision)
    }

    fn apply_decision<SourceError>(
        &mut self,
        scanner: Box<Scanner>,
        decision: Decision,
    ) -> Result<(), DirectReferencePrefixPollError<SourceError>> {
        match decision {
            Decision::Continue => self.stage = Stage::Scanning(scanner),
            Decision::Reject => self.complete_visible_or_empty()?,
            Decision::Accept(end) => {
                if !self.metric.is_complete() {
                    self.stage = Stage::Failed;
                    return Err(DirectReferencePrefixPollError::InvalidUtf8 {
                        relative_offset: self.cursor,
                    });
                }
                let definition = self.build_definition(scanner, end)?;
                let resume = self.output_resume(end)?;
                self.stage = Stage::OutputReady {
                    definition,
                    source_end: end,
                    resume,
                };
            }
            Decision::Fallback => {
                if let Some(end) = scanner.fallback_source_end {
                    let definition = self.build_definition_without_title(scanner, end)?;
                    let resume = self.output_resume(end)?;
                    self.stage = Stage::OutputReady {
                        definition,
                        source_end: end,
                        resume,
                    };
                } else {
                    self.complete_visible_or_empty()?;
                }
            }
            Decision::FallbackPreservingTitle => {
                if let Some(end) = scanner.fallback_source_end {
                    let definition = self.build_definition_without_title(scanner, end)?;
                    let resume = self.output_resume(end)?;
                    self.stage = Stage::OutputReady {
                        definition,
                        source_end: end,
                        resume,
                    };
                } else {
                    self.complete_visible_or_empty()?;
                }
            }
        }
        Ok(())
    }

    fn output_resume<SourceError>(
        &self,
        source_end: DirectReferenceLogicalPosition,
    ) -> Result<DirectReferencePrefixOutputResume, DirectReferencePrefixPollError<SourceError>>
    {
        let recognition_high_water = self.position()?;
        if recognition_high_water == source_end {
            return Ok(DirectReferencePrefixOutputResume::ScanAtSourceEnd);
        }
        if let Some(replay) = self.last_consumed
            && replay.before == source_end
            && replay.after == recognition_high_water
            && replay.byte == b'['
        {
            return Ok(DirectReferencePrefixOutputResume::Replay(replay));
        }
        Ok(DirectReferencePrefixOutputResume::VisibleRemainder(
            recognition_high_water,
        ))
    }

    fn consume_label(
        &self,
        scanner: &mut Scanner,
        before: DirectReferenceLogicalPosition,
        after: DirectReferenceLogicalPosition,
        byte: u8,
        raw_codepoint_contribution: u8,
    ) -> Decision {
        if before.bytes == scanner.start.bytes {
            return if byte == b'[' {
                Decision::Continue
            } else {
                Decision::Reject
            };
        }

        let pending_len = usize::from(scanner.label_pending_len);
        if pending_len == scanner.label_pending.len() {
            return Decision::Reject;
        }
        if pending_len == 0 {
            scanner.label_pending_start = before;
        }
        scanner.label_pending[pending_len] = byte;
        scanner.label_pending_len += 1;
        if after.utf16 == before.utf16 {
            return Decision::Continue;
        }
        let pending_len = usize::from(scanner.label_pending_len);
        let mut pending = [0; 4];
        pending[..pending_len].copy_from_slice(&scanner.label_pending[..pending_len]);
        let scalar_start = scanner.label_pending_start;
        scanner.label_pending_len = 0;
        let pending = &pending[..pending_len];
        let ascii = (pending_len == 1).then_some(pending[0]);

        if scanner.label_backslash {
            scanner.label_backslash = false;
            if ascii.is_some_and(is_ascii_punctuation) {
                return append_label_scalar(
                    scanner,
                    pending,
                    scalar_start,
                    after,
                    raw_codepoint_contribution,
                );
            }
        }
        match ascii {
            Some(b']') => {
                if scanner.label_trimmed_start.is_none() {
                    Decision::Reject
                } else {
                    scanner.phase = Phase::Colon;
                    Decision::Continue
                }
            }
            Some(b'[') => Decision::Reject,
            Some(b'\\') => {
                scanner.label_backslash = true;
                append_label_scalar(
                    scanner,
                    pending,
                    scalar_start,
                    after,
                    raw_codepoint_contribution,
                )
            }
            _ => append_label_scalar(
                scanner,
                pending,
                scalar_start,
                after,
                raw_codepoint_contribution,
            ),
        }
    }

    fn consume_before_destination(
        &self,
        scanner: &mut Scanner,
        before: DirectReferenceLogicalPosition,
        after: DirectReferenceLogicalPosition,
        byte: u8,
    ) -> Decision {
        if scanner.before_destination_cr {
            scanner.before_destination_cr = false;
            if byte == b'\n' {
                return Decision::Continue;
            }
            return self.start_destination(scanner, before, after, byte);
        }
        if is_space_or_tab(byte) {
            return Decision::Continue;
        }
        if !scanner.before_destination_newline && matches!(byte, b'\r' | b'\n') {
            scanner.before_destination_newline = true;
            scanner.before_destination_cr = byte == b'\r';
            return Decision::Continue;
        }
        self.start_destination(scanner, before, after, byte)
    }

    fn start_destination(
        &self,
        scanner: &mut Scanner,
        before: DirectReferenceLogicalPosition,
        after: DirectReferenceLogicalPosition,
        byte: u8,
    ) -> Decision {
        scanner.destination_start = before;
        if byte == b'<' {
            scanner.destination_start = after;
            scanner.phase = Phase::AngleDestination;
            Decision::Continue
        } else {
            scanner.phase = Phase::BareDestination;
            self.consume_bare_destination(scanner, before, after, byte)
        }
    }

    fn consume_angle_destination(
        &self,
        scanner: &mut Scanner,
        before: DirectReferenceLogicalPosition,
        byte: u8,
    ) -> Decision {
        if matches!(byte, b'\r' | b'\n' | b'<') {
            return Decision::Reject;
        }
        if scanner.angle_backslash {
            scanner.angle_backslash = false;
            if is_ascii_punctuation(byte) {
                return Decision::Continue;
            }
        }
        match byte {
            b'\\' => scanner.angle_backslash = true,
            b'>' => {
                scanner.destination_end = before;
                scanner.phase = Phase::AngleClosed;
            }
            _ => {}
        }
        Decision::Continue
    }

    fn consume_bare_destination(
        &self,
        scanner: &mut Scanner,
        before: DirectReferenceLogicalPosition,
        after: DirectReferenceLogicalPosition,
        byte: u8,
    ) -> Decision {
        if scanner.bare_backslash {
            scanner.bare_backslash = false;
            if is_ascii_punctuation(byte) {
                scanner.destination_bytes += 1;
                return Decision::Continue;
            }
        }
        match byte {
            b'\\' => {
                scanner.destination_bytes += 1;
                scanner.bare_backslash = true;
            }
            b'(' => {
                scanner.destination_bytes += 1;
                scanner.bare_depth = scanner.bare_depth.saturating_add(1);
                if scanner.bare_depth > 32 {
                    return Decision::Reject;
                }
            }
            b')' if scanner.bare_depth > 0 => {
                scanner.destination_bytes += 1;
                scanner.bare_depth -= 1;
            }
            _ if is_url_space(byte) || (byte.is_ascii_control() && byte != 0) => {
                return self.end_bare_destination(scanner, before, after, byte);
            }
            _ => scanner.destination_bytes += 1,
        }
        Decision::Continue
    }

    fn end_bare_destination(
        &self,
        scanner: &mut Scanner,
        before: DirectReferenceLogicalPosition,
        after: DirectReferenceLogicalPosition,
        delimiter: u8,
    ) -> Decision {
        if scanner.destination_bytes == 0 || scanner.bare_depth != 0 {
            return Decision::Reject;
        }
        scanner.destination_end = before;
        scanner.phase = Phase::AfterDestination;
        self.consume_after_destination(scanner, before, after, delimiter)
    }

    fn consume_after_destination(
        &self,
        scanner: &mut Scanner,
        before: DirectReferenceLogicalPosition,
        after: DirectReferenceLogicalPosition,
        byte: u8,
    ) -> Decision {
        if scanner.after_destination_cr {
            scanner.after_destination_cr = false;
            if byte == b'\n' {
                scanner.fallback_source_end = Some(after);
                return Decision::Continue;
            }
            scanner.fallback_source_end = Some(before);
            return self.consume_after_destination(scanner, before, after, byte);
        }
        if is_space_or_tab(byte) {
            scanner.after_destination_separator = true;
            return Decision::Continue;
        }
        if !scanner.after_destination_newline && matches!(byte, b'\r' | b'\n') {
            scanner.after_destination_separator = true;
            scanner.after_destination_newline = true;
            if byte == b'\r' {
                scanner.after_destination_cr = true;
            } else {
                scanner.fallback_source_end = Some(after);
            }
            return Decision::Continue;
        }
        if !scanner.after_destination_separator {
            return Decision::Reject;
        }
        match byte {
            b'"' | b'\'' => {
                scanner.title_start = Some(before);
                scanner.title_closer = byte;
                scanner.phase = Phase::QuotedTitle;
                Decision::Continue
            }
            b'(' => {
                scanner.title_start = Some(before);
                scanner.title_closer = b')';
                scanner.phase = Phase::ParenTitle;
                Decision::Continue
            }
            _ => Decision::Fallback,
        }
    }

    fn consume_quoted_title(
        &self,
        scanner: &mut Scanner,
        after: DirectReferenceLogicalPosition,
        byte: u8,
    ) -> Decision {
        if consume_title_escape(scanner, byte) {
            return Decision::Continue;
        }
        if byte == scanner.title_closer {
            scanner.title_end = Some(after);
            scanner.phase = Phase::AfterTitle;
        }
        Decision::Continue
    }

    fn consume_paren_title(
        &self,
        scanner: &mut Scanner,
        after: DirectReferenceLogicalPosition,
        byte: u8,
    ) -> Decision {
        if consume_title_escape(scanner, byte) {
            return Decision::Continue;
        }
        match byte {
            b')' => {
                scanner.title_end = Some(after);
                scanner.phase = Phase::AfterTitle;
                Decision::Continue
            }
            b'(' => Decision::Fallback,
            _ => Decision::Continue,
        }
    }

    fn consume_after_title(
        &self,
        _scanner: &mut Scanner,
        _before: DirectReferenceLogicalPosition,
        after: DirectReferenceLogicalPosition,
        byte: u8,
    ) -> Decision {
        if is_space_or_tab(byte) {
            return Decision::Continue;
        }
        match byte {
            b'\n' => Decision::Accept(after),
            b'\r' => {
                _scanner.phase = Phase::AfterTitleCr;
                Decision::Continue
            }
            _ => Decision::FallbackPreservingTitle,
        }
    }

    fn finish_at_eof<SourceError>(
        &mut self,
    ) -> Result<(), DirectReferencePrefixPollError<SourceError>> {
        if !self.metric.is_complete() {
            self.stage = Stage::Failed;
            return Err(DirectReferencePrefixPollError::InvalidUtf8 {
                relative_offset: self.cursor,
            });
        }
        let scanner = match std::mem::replace(&mut self.stage, Stage::Failed) {
            Stage::Scanning(scanner) => scanner,
            _ => return Err(DirectReferencePrefixPollError::PollAfterComplete),
        };
        let end = self.position()?;
        let decision = match scanner.phase {
            Phase::BareDestination if scanner.destination_bytes > 0 && scanner.bare_depth == 0 => {
                Decision::Accept(end)
            }
            Phase::AfterDestination | Phase::AfterTitle | Phase::AfterTitleCr => {
                Decision::Accept(end)
            }
            Phase::QuotedTitle | Phase::ParenTitle => Decision::Fallback,
            Phase::Label
            | Phase::Colon
            | Phase::BeforeDestination
            | Phase::AngleDestination
            | Phase::AngleClosed
            | Phase::BareDestination => Decision::Reject,
        };
        let mut scanner = scanner;
        if matches!(scanner.phase, Phase::BareDestination)
            && matches!(decision, Decision::Accept(_))
        {
            scanner.destination_end = end;
        }
        self.apply_decision(scanner, decision)
    }

    fn build_definition<SourceError>(
        &self,
        scanner: Box<Scanner>,
        source_end: DirectReferenceLogicalPosition,
    ) -> Result<DirectReferenceDefinition, DirectReferencePrefixPollError<SourceError>> {
        self.build_definition_inner(scanner, source_end, true)
    }

    fn build_definition_without_title<SourceError>(
        &self,
        scanner: Box<Scanner>,
        source_end: DirectReferenceLogicalPosition,
    ) -> Result<DirectReferenceDefinition, DirectReferencePrefixPollError<SourceError>> {
        self.build_definition_inner(scanner, source_end, false)
    }

    fn build_definition_inner<SourceError>(
        &self,
        scanner: Box<Scanner>,
        source_end: DirectReferenceLogicalPosition,
        retain_title: bool,
    ) -> Result<DirectReferenceDefinition, DirectReferencePrefixPollError<SourceError>> {
        let label_start = scanner
            .label_trimmed_start
            .ok_or(DirectReferencePrefixPollError::CounterOverflow)?;
        let label_end = scanner.label_trimmed_end;
        let title = retain_title
            .then(|| scanner.title_start.zip(scanner.title_end))
            .flatten()
            .map(|(start, end)| DirectReferenceLogicalRange::new(start, end));
        let logical_source = DirectReferenceLogicalRange::new(scanner.start, source_end);
        let logical_destination =
            DirectReferenceLogicalRange::new(scanner.destination_start, scanner.destination_end);
        let normalized_label = scanner.label_accumulator.into_normalized();
        if normalized_label.is_empty() {
            return Err(DirectReferencePrefixPollError::CounterOverflow);
        }
        Ok(DirectReferenceDefinition {
            logical_source,
            logical_label: DirectReferenceLogicalRange::new(label_start, label_end),
            logical_destination,
            title_transform: title
                .as_ref()
                .map(|_| DirectReferenceValueTransform::CleanTitle),
            logical_title: title,
            normalized_label,
            destination_transform: DirectReferenceValueTransform::CleanDestination,
        })
    }

    fn complete_visible_or_empty<SourceError>(
        &mut self,
    ) -> Result<(), DirectReferencePrefixPollError<SourceError>> {
        let recognition_end = self.position()?;
        let disposition = if self.definition_count == 0 {
            DirectReferencePrefixDisposition::NoDefinitions
        } else {
            if recognition_end == self.prefix_end {
                DirectReferencePrefixDisposition::ReferenceOnly
            } else {
                DirectReferencePrefixDisposition::VisibleRemainder
            }
        };
        self.stage = Stage::Complete(DirectReferencePrefixTerminal {
            disposition,
            definition_count: self.definition_count,
            logical_reference_prefix: DirectReferenceLogicalRange::new(
                self.logical_base,
                self.prefix_end,
            ),
            logical_recognition: DirectReferenceLogicalRange::new(
                self.logical_base,
                recognition_end,
            ),
        });
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
enum Decision {
    Continue,
    Reject,
    Accept(DirectReferenceLogicalPosition),
    Fallback,
    FallbackPreservingTitle,
}

fn append_label_scalar(
    scanner: &mut Scanner,
    scalar: &[u8],
    scalar_start: DirectReferenceLogicalPosition,
    scalar_end: DirectReferenceLogicalPosition,
    raw_codepoints: u8,
) -> Decision {
    let Ok(text) = std::str::from_utf8(scalar) else {
        return Decision::Reject;
    };
    let Some(ch) = text.chars().next() else {
        return Decision::Reject;
    };
    if scanner.label_accumulator.push(ch, raw_codepoints).is_err() {
        return Decision::Reject;
    }
    if !is_reference_label_whitespace(ch) {
        scanner.label_trimmed_start.get_or_insert(scalar_start);
        scanner.label_trimmed_end = scalar_end;
    }
    Decision::Continue
}

fn consume_title_escape(scanner: &mut Scanner, byte: u8) -> bool {
    if scanner.title_backslash {
        scanner.title_backslash = false;
        if is_ascii_punctuation(byte) {
            return true;
        }
    }
    if byte == b'\\' {
        scanner.title_backslash = true;
    }
    false
}

const fn is_space_or_tab(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

const fn is_url_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

const fn is_ascii_punctuation(byte: u8) -> bool {
    matches!(byte, b'!'..=b'/' | b':'..=b'@' | b'['..=b'`' | b'{'..=b'~')
}

#[cfg(test)]
mod tests {
    use super::*;
    use comrak::block_spine_facade::reference_definitions;

    #[derive(Debug, PartialEq, Eq)]
    enum SourceError {
        NonSequential,
        PastGrant,
    }

    struct SliceSource<'a> {
        identity: u64,
        bytes: &'a [u8],
        available: usize,
        final_input: bool,
        next: usize,
        grant: usize,
        used: usize,
    }

    impl SliceSource<'_> {
        fn replenish(&mut self, grant: usize) {
            self.grant = grant;
            self.used = 0;
        }
    }

    impl DirectReferencePrefixSource for SliceSource<'_> {
        type Identity = u64;
        type Error = SourceError;

        fn identity(&self) -> Self::Identity {
            self.identity
        }

        fn available_len(&self) -> usize {
            self.available
        }

        fn is_final(&self) -> bool {
            self.final_input
        }

        fn access_budget(&self) -> usize {
            self.grant - self.used
        }

        fn read_byte(&mut self, relative_offset: usize) -> Result<u8, Self::Error> {
            if relative_offset != self.next {
                return Err(SourceError::NonSequential);
            }
            if self.used == self.grant || relative_offset >= self.available {
                return Err(SourceError::PastGrant);
            }
            let byte = self.bytes[relative_offset];
            self.next += 1;
            self.used += 1;
            Ok(byte)
        }

        fn raw_codepoint_contribution(&self, _logical_scalar_end_offset: usize) -> u8 {
            1
        }
    }

    struct CrlfMetricSource<'a> {
        inner: SliceSource<'a>,
        canonical_lf_offset: usize,
    }

    struct ProjectedMetricSource<'a> {
        inner: SliceSource<'a>,
        zero_contribution_offsets: Vec<usize>,
    }

    impl DirectReferencePrefixSource for ProjectedMetricSource<'_> {
        type Identity = u64;
        type Error = SourceError;

        fn identity(&self) -> Self::Identity {
            self.inner.identity()
        }

        fn available_len(&self) -> usize {
            self.inner.available_len()
        }

        fn is_final(&self) -> bool {
            self.inner.is_final()
        }

        fn access_budget(&self) -> usize {
            self.inner.access_budget()
        }

        fn read_byte(&mut self, relative_offset: usize) -> Result<u8, Self::Error> {
            self.inner.read_byte(relative_offset)
        }

        fn raw_codepoint_contribution(&self, logical_scalar_end_offset: usize) -> u8 {
            u8::from(
                !self
                    .zero_contribution_offsets
                    .contains(&logical_scalar_end_offset),
            )
        }
    }

    impl DirectReferencePrefixSource for CrlfMetricSource<'_> {
        type Identity = u64;
        type Error = SourceError;

        fn identity(&self) -> Self::Identity {
            self.inner.identity()
        }

        fn available_len(&self) -> usize {
            self.inner.available_len()
        }

        fn is_final(&self) -> bool {
            self.inner.is_final()
        }

        fn access_budget(&self) -> usize {
            self.inner.access_budget()
        }

        fn read_byte(&mut self, relative_offset: usize) -> Result<u8, Self::Error> {
            self.inner.read_byte(relative_offset)
        }

        fn raw_codepoint_contribution(&self, logical_scalar_end_offset: usize) -> u8 {
            if logical_scalar_end_offset == self.canonical_lf_offset {
                2
            } else {
                1
            }
        }
    }

    fn first_definition(
        input: &str,
        fuel: usize,
    ) -> (Option<DirectReferenceDefinition>, usize, usize) {
        let mut source = SliceSource {
            identity: 7,
            bytes: input.as_bytes(),
            available: input.len(),
            final_input: true,
            next: 0,
            grant: fuel,
            used: 0,
        };
        let mut work =
            DirectReferencePrefixWork::new(11, 13, 7, DirectReferenceLogicalPosition::default());
        let mut maximum_retained = 0;
        let mut polls = 0;
        loop {
            source.replenish(fuel);
            let receipt = work.poll_source(&mut source, fuel, false).unwrap();
            polls += 1;
            maximum_retained = maximum_retained.max(receipt.retained_source_bytes);
            assert!(receipt.inspected_bytes <= fuel);
            assert!(receipt.source_first_reads <= fuel);
            match receipt.status {
                DirectReferencePrefixPollStatus::NeedMore => {}
                DirectReferencePrefixPollStatus::OutputReady => {
                    let output = work.take_output().unwrap();
                    let (definition, _) = output.acknowledge();
                    return (Some(definition), maximum_retained, polls);
                }
                DirectReferencePrefixPollStatus::Complete => {
                    return (None, maximum_retained, polls);
                }
                DirectReferencePrefixPollStatus::Cancelled => panic!("unexpected cancellation"),
            }
        }
    }

    fn utf16_at(input: &str, bytes: usize) -> u64 {
        input[..bytes].encode_utf16().count() as u64
    }

    fn assert_first_matches_donor(input: &str, fuel: usize) {
        let expected = reference_definitions(input).unwrap().into_iter().next();
        let (actual, retained, _) = first_definition(input, fuel);
        assert_eq!(actual.is_some(), expected.is_some(), "input={input:?}");
        assert!(retained <= DIRECT_REFERENCE_LABEL_MAX_RETAINED_BYTES);
        if let (Some(actual), Some(expected)) = (actual, expected) {
            let expected_source = expected.source.start as u64..expected.source.end as u64;
            let expected_label =
                expected.label_source.start as u64..expected.label_source.end as u64;
            let expected_destination =
                expected.url_source.start as u64..expected.url_source.end as u64;
            assert_eq!(actual.logical_source.bytes, expected_source);
            assert_eq!(actual.logical_label.bytes, expected_label);
            assert_eq!(actual.logical_destination.bytes, expected_destination);
            assert_eq!(
                actual
                    .logical_title
                    .as_ref()
                    .map(|range| range.bytes.clone()),
                expected
                    .title_source
                    .map(|range| range.start as u64..range.end as u64)
            );
            assert_eq!(actual.normalized_label, expected.normalized_label);
            for range in [
                &actual.logical_source,
                &actual.logical_label,
                &actual.logical_destination,
            ]
            .into_iter()
            .chain(actual.logical_title.iter())
            {
                assert_eq!(
                    range.utf16.start,
                    utf16_at(input, range.bytes.start as usize),
                    "UTF-16 start input={input:?} range={range:?}"
                );
                assert_eq!(
                    range.utf16.end,
                    utf16_at(input, range.bytes.end as usize),
                    "UTF-16 end input={input:?} range={range:?}"
                );
            }
        }
    }

    #[test]
    fn fuel_one_fixed_shapes_match_pinned_donor() {
        for input in [
            "[x]: /url\n",
            "[ x ]:\t<url> \"title\"\r\n",
            "[x]: a(b)c 'title'\n",
            "[x]: a\\(b\\) (title)\n",
            "[x]: <u>\n  (title)\n",
            "[x]: u\n[next]: v\n",
            "[x\\]]: /url\n",
            "[x\\[]: /url\n",
            "[Straẞe]: /世界\r\n",
            "[]: /url\n",
            "[x]: <>\n",
            "[x]: <u>",
            "[x]: u",
        ] {
            assert_first_matches_donor(input, 1);
        }
    }

    #[test]
    fn invalid_title_suffix_is_discarded_from_destination_only_definition() {
        for input in ["[foo]: /url\n\"title\" ok\n"] {
            let (actual, _, _) = first_definition(input, 1);
            let actual = actual
                .unwrap_or_else(|| panic!("destination-only definition remains valid: {input:?}"));
            assert!(actual.logical_title.is_none(), "input={input:?}");

            let donor = reference_definitions(input).unwrap().remove(0);
            assert!(
                donor.title_source.is_some(),
                "pinned Comrak disagreement remains an explicit oracle divergence"
            );
        }
    }

    #[test]
    fn escaped_line_ending_in_angle_destination_is_invalid() {
        let input = "[x]: <u\\\n  v>\n";
        assert!(first_definition(input, 1).0.is_none());
        assert!(
            reference_definitions(input).unwrap().first().is_some(),
            "pinned Comrak incorrectly lets backslash hide the line ending"
        );
    }

    #[test]
    fn non_spec_unicode_and_control_whitespace_remain_label_content() {
        for label in ["\u{a0}", "\u{2003}", "\u{b}", "\u{c}"] {
            let input = format!("[{label}]: /u\n");
            let (definition, _, _) = first_definition(&input, 1);
            let definition = definition.expect("non-spec whitespace is nonempty label content");
            assert_eq!(definition.normalized_label, label);
            assert_eq!(definition.logical_label.bytes, 1..1 + label.len() as u64);
        }
    }

    #[derive(Clone, Copy)]
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            self.0
        }

        fn usize(&mut self, ceiling: usize) -> usize {
            (self.next() as usize) % ceiling
        }
    }

    #[test]
    fn randomized_ascii_presence_and_ranges_match_pinned_donor() {
        let alphabet = b"[]:<>/()\\'\" abcXYZ09-_\t\r\n";
        let mut rng = Lcg(0x5e_fe_12);
        for _ in 0..10_000 {
            let len = rng.usize(128);
            let input: String = (0..len)
                .map(|_| alphabet[rng.usize(alphabet.len())] as char)
                .collect();
            assert_first_matches_donor(&input, 7);
        }
    }

    #[test]
    fn label_limit_is_normatively_unicode_scalar_bounded() {
        assert!(
            first_definition(&format!("[{}]: /u\n", "a".repeat(999)), 1)
                .0
                .is_some()
        );
        assert!(
            first_definition(&format!("[{}]: /u\n", "a".repeat(1000)), 1)
                .0
                .is_none()
        );
        assert!(
            first_definition(&format!("[{}]: /u\n", "é".repeat(999)), 1)
                .0
                .is_some()
        );
        assert!(
            first_definition(&format!("[{}]: /u\n", "é".repeat(1000)), 1)
                .0
                .is_none()
        );

        // Pinned Comrak is retained as a syntax oracle below the boundary,
        // but its byte counter is deliberately not normative for this gate.
        assert!(
            reference_definitions(&format!("[{}]: /u\n", "a".repeat(1000)))
                .unwrap()
                .first()
                .is_some()
        );
        assert!(
            reference_definitions(&format!("[{}]: /u\n", "é".repeat(999)))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn projected_crlf_contributes_two_raw_codepoints_to_label_limit() {
        for (ascii, accepted) in [(997, true), (998, false)] {
            let input = format!("[{}\n]: /u\n", "a".repeat(ascii));
            let lf = ascii + 1;
            let mut source = CrlfMetricSource {
                inner: SliceSource {
                    identity: 73,
                    bytes: input.as_bytes(),
                    available: input.len(),
                    final_input: true,
                    next: 0,
                    grant: 1,
                    used: 0,
                },
                canonical_lf_offset: lf,
            };
            let mut work = DirectReferencePrefixWork::new(
                79,
                83,
                73,
                DirectReferenceLogicalPosition::default(),
            );
            let found = loop {
                source.inner.replenish(1);
                match work.poll_source(&mut source, 1, false).unwrap().status {
                    DirectReferencePrefixPollStatus::NeedMore => {}
                    DirectReferencePrefixPollStatus::OutputReady => break true,
                    DirectReferencePrefixPollStatus::Complete => break false,
                    DirectReferencePrefixPollStatus::Cancelled => {
                        panic!("unexpected cancellation")
                    }
                }
            };
            assert_eq!(found, accepted, "ASCII scalars before CRLF={ascii}");
        }
    }

    #[test]
    fn projected_tab_continuations_do_not_overcount_label_limit() {
        for (ascii, accepted) in [(997, true), (998, false)] {
            let input = format!("[{}   b]: /u\n", "a".repeat(ascii));
            let first_space = ascii + 1;
            let mut source = ProjectedMetricSource {
                inner: SliceSource {
                    identity: 137,
                    bytes: input.as_bytes(),
                    available: input.len(),
                    final_input: true,
                    next: 0,
                    grant: 1,
                    used: 0,
                },
                zero_contribution_offsets: vec![first_space + 1, first_space + 2],
            };
            let mut work = DirectReferencePrefixWork::new(
                139,
                149,
                137,
                DirectReferenceLogicalPosition::default(),
            );
            let found = loop {
                source.inner.replenish(1);
                match work.poll_source(&mut source, 1, false).unwrap().status {
                    DirectReferencePrefixPollStatus::NeedMore => {}
                    DirectReferencePrefixPollStatus::OutputReady => break true,
                    DirectReferencePrefixPollStatus::Complete => break false,
                    DirectReferencePrefixPollStatus::Cancelled => {
                        panic!("unexpected cancellation")
                    }
                }
            };
            assert_eq!(
                found, accepted,
                "ASCII scalars before projected tab={ascii}"
            );
        }
    }

    #[test]
    fn one_byte_refills_preserve_unicode_crlf_and_exact_cuts() {
        let input = "[Straẞe]: </世界> \"títle\"\r\n";
        let mut source = SliceSource {
            identity: 19,
            bytes: input.as_bytes(),
            available: 0,
            final_input: false,
            next: 0,
            grant: 1,
            used: 0,
        };
        let mut work =
            DirectReferencePrefixWork::new(23, 29, 19, DirectReferenceLogicalPosition::default());
        loop {
            source.replenish(1);
            let receipt = work.poll_source(&mut source, 1, false).unwrap();
            assert!(receipt.inspected_bytes <= 1);
            match receipt.status {
                DirectReferencePrefixPollStatus::NeedMore => {
                    if source.available < input.len() {
                        source.available += 1;
                    } else {
                        source.final_input = true;
                    }
                }
                DirectReferencePrefixPollStatus::OutputReady => {
                    let (actual, _) = work.take_output().unwrap().acknowledge();
                    let expected = reference_definitions(input).unwrap().remove(0);
                    assert_eq!(
                        actual.logical_source.bytes,
                        expected.source.start as u64..expected.source.end as u64
                    );
                    assert_eq!(actual.normalized_label, expected.normalized_label);
                    break;
                }
                DirectReferencePrefixPollStatus::Complete => panic!("definition rejected"),
                DirectReferencePrefixPollStatus::Cancelled => panic!("unexpected cancellation"),
            }
        }
    }

    #[test]
    fn giant_values_are_ranges_and_retention_stays_label_bounded() {
        let giant_destination = format!("[x]: /{}\n", "u".repeat(1024 * 1024));
        let (definition, retained, polls) = first_definition(&giant_destination, 4096);
        let definition = definition.expect("giant destination remains valid");
        assert_eq!(
            definition.logical_destination.bytes.end - definition.logical_destination.bytes.start,
            1024 * 1024 + 1
        );
        assert!(retained <= 1);
        assert!(polls > 1);

        let giant_bad_title = format!("[x]: /u\n\"{}", "t".repeat(1024 * 1024));
        let (definition, retained, polls) = first_definition(&giant_bad_title, 4096);
        let definition = definition.expect("donor falls back to destination-only definition");
        assert_eq!(definition.logical_source.bytes, 0..8);
        assert!(definition.logical_title.is_none());
        assert!(retained <= 1);
        assert!(polls > 1);
    }

    #[test]
    fn one_actor_work_publishes_ordered_occurrences_one_at_a_time() {
        let input = "[x]: /one\n[x]: /two\nvisible\n";
        let mut source = SliceSource {
            identity: 89,
            bytes: input.as_bytes(),
            available: input.len(),
            final_input: true,
            next: 0,
            grant: 1,
            used: 0,
        };
        let mut work =
            DirectReferencePrefixWork::new(97, 101, 89, DirectReferenceLogicalPosition::default());
        let mut definitions = Vec::new();
        'poll: loop {
            source.replenish(1);
            let receipt = work.poll_source(&mut source, 1, false).unwrap();
            match receipt.status {
                DirectReferencePrefixPollStatus::NeedMore => {}
                DirectReferencePrefixPollStatus::OutputReady => {
                    let output = work.take_output().unwrap();
                    source.replenish(1);
                    assert_eq!(
                        work.poll_source(&mut source, 1, false),
                        Err(DirectReferencePrefixPollError::OutputNotAcknowledged)
                    );
                    let (definition, ack) = output.acknowledge();
                    definitions.push(definition.logical_destination.bytes.clone());
                    if work.acknowledge_output(ack).unwrap()
                        == DirectReferencePrefixOutputAckStatus::Complete
                    {
                        break 'poll;
                    }
                }
                DirectReferencePrefixPollStatus::Complete => break,
                DirectReferencePrefixPollStatus::Cancelled => panic!("unexpected cancellation"),
            }
        }
        assert_eq!(definitions, [5..9, 15..19]);
        let terminal = work.terminal().unwrap();
        assert_eq!(terminal.definition_count, 2);
        assert_eq!(
            terminal.disposition,
            DirectReferencePrefixDisposition::VisibleRemainder
        );
        assert_eq!(terminal.logical_reference_prefix.bytes, 0..20);
        assert_eq!(terminal.logical_recognition.bytes, 0..21);
    }

    #[test]
    fn cancellation_invalidates_an_outstanding_writer_ack() {
        let input = "[x]: /u\n";
        let mut source = SliceSource {
            identity: 103,
            bytes: input.as_bytes(),
            available: input.len(),
            final_input: true,
            next: 0,
            grant: input.len(),
            used: 0,
        };
        let mut work = DirectReferencePrefixWork::new(
            107,
            109,
            103,
            DirectReferenceLogicalPosition::default(),
        );
        let receipt = work.poll_source(&mut source, input.len(), false).unwrap();
        assert_eq!(receipt.status, DirectReferencePrefixPollStatus::NeedMore);
        source.replenish(1);
        let receipt = work.poll_source(&mut source, 1, false).unwrap();
        assert_eq!(receipt.status, DirectReferencePrefixPollStatus::OutputReady);
        let (_, ack) = work.take_output().unwrap().acknowledge();
        work.cancel();
        assert_eq!(
            work.acknowledge_output(ack),
            Err(DirectReferencePrefixPollError::WrongOutput)
        );
        source.replenish(1);
        assert_eq!(
            work.poll_source(&mut source, 1, false),
            Err(DirectReferencePrefixPollError::PollAfterCancelled)
        );
    }

    #[test]
    fn crossed_work_acknowledgement_is_rejected() {
        let input = "[x]: /u\n";
        let mut source_a = SliceSource {
            identity: 151,
            bytes: input.as_bytes(),
            available: input.len(),
            final_input: true,
            next: 0,
            grant: 1,
            used: 0,
        };
        let mut source_b = SliceSource {
            identity: 151,
            bytes: input.as_bytes(),
            available: input.len(),
            final_input: true,
            next: 0,
            grant: 1,
            used: 0,
        };
        let mut work_a = DirectReferencePrefixWork::new(
            157,
            163,
            151,
            DirectReferenceLogicalPosition::default(),
        );
        let mut work_b = DirectReferencePrefixWork::new(
            157,
            163,
            151,
            DirectReferenceLogicalPosition::default(),
        );
        let output_a = loop {
            source_a.replenish(1);
            if work_a.poll_source(&mut source_a, 1, false).unwrap().status
                == DirectReferencePrefixPollStatus::OutputReady
            {
                break work_a.take_output().unwrap();
            }
        };
        let output_b = loop {
            source_b.replenish(1);
            if work_b.poll_source(&mut source_b, 1, false).unwrap().status
                == DirectReferencePrefixPollStatus::OutputReady
            {
                break work_b.take_output().unwrap();
            }
        };
        let (_, ack_b) = output_b.acknowledge();
        assert_eq!(
            work_a.acknowledge_output(ack_b),
            Err(DirectReferencePrefixPollError::WrongOutput)
        );
        drop(output_a);
        work_a.cancel();
        work_b.cancel();
    }

    #[test]
    fn work_moves_only_bounded_handles_during_fuel_one_polling() {
        let work_bytes = std::mem::size_of::<DirectReferencePrefixWork<u64>>();
        let scanner_bytes = std::mem::size_of::<Scanner>();
        eprintln!("reference work bytes={work_bytes}; boxed scanner bytes={scanner_bytes}");
        assert!(
            work_bytes <= 512,
            "work keeps scanner payload behind one stable box; the bounded inline terminal/output union remains in-place"
        );
        assert!(scanner_bytes >= DIRECT_REFERENCE_LABEL_MAX_RETAINED_BYTES);
        let scanner = Scanner::new(DirectReferenceLogicalPosition::default());
        assert_eq!(
            scanner.label_accumulator.allocated_bytes(),
            DIRECT_REFERENCE_LABEL_MAX_NORMALIZED_BYTES,
            "normalized output envelope is allocated before polling"
        );

        let input = "[x]: /u\n";
        let mut source = SliceSource {
            identity: 113,
            bytes: input.as_bytes(),
            available: input.len(),
            final_input: true,
            next: 0,
            grant: 1,
            used: 0,
        };
        let mut work = DirectReferencePrefixWork::new(
            127,
            131,
            113,
            DirectReferenceLogicalPosition::default(),
        );
        loop {
            source.replenish(1);
            let receipt = work.poll_source(&mut source, 1, false).unwrap();
            assert!(receipt.inspected_bytes <= 1);
            assert!(receipt.source_first_reads <= 1);
            assert!(receipt.retained_source_bytes <= DIRECT_REFERENCE_LABEL_MAX_RETAINED_BYTES);
            if receipt.status == DirectReferencePrefixPollStatus::OutputReady {
                break;
            }
        }
    }

    #[test]
    fn cancellation_is_observed_before_source_access() {
        let input = "[x]: /u\n";
        let mut source = SliceSource {
            identity: 31,
            bytes: input.as_bytes(),
            available: input.len(),
            final_input: true,
            next: 0,
            grant: input.len(),
            used: 0,
        };
        let mut work =
            DirectReferencePrefixWork::new(37, 41, 31, DirectReferenceLogicalPosition::default());
        let receipt = work.poll_source(&mut source, 1, true).unwrap();
        assert_eq!(receipt.status, DirectReferencePrefixPollStatus::Cancelled);
        assert_eq!(receipt.inspected_bytes, 0);
        assert_eq!(source.next, 0);
    }

    #[test]
    fn logical_cuts_do_not_claim_nested_physical_contiguity() {
        // A writer projection can expose this nested, CRLF source as the
        // logical paragraph below.  Container prefixes and the second-line
        // indentation have no logical bytes; CRLF maps to one logical LF.
        let physical = "> [x]:\r\n>   /u\r\n";
        let logical = "[x]:\n/u\n";
        let (definition, _, _) = first_definition(logical, 1);
        let definition = definition.expect("multiline logical definition");
        assert_eq!(definition.logical_source.bytes, 0..logical.len() as u64);

        let projected_physical_runs = [2..6, 6..8, 12..14, 14..16];
        assert_eq!(&physical[projected_physical_runs[0].clone()], "[x]:");
        assert_eq!(&physical[projected_physical_runs[1].clone()], "\r\n");
        assert_eq!(&physical[projected_physical_runs[2].clone()], "/u");
        assert_eq!(&physical[projected_physical_runs[3].clone()], "\r\n");
        assert!(
            projected_physical_runs
                .windows(2)
                .any(|pair| pair[0].end != pair[1].start)
        );
    }
}

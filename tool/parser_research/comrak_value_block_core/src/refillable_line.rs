//! Bounded-window feasibility slice for oversized `CommonMark` physical lines.
//!
//! This module deliberately separates two jobs that a whole-line `&str`
//! hides: block-prefix recognition and source-body coverage. It admits the
//! mechanically simple document prefixes (plain paragraphs and fenced-code
//! openers) plus every tab/NUL-free line inside an already-open fenced block.
//! Other document prefixes fail closed for the donor grammar to decide later.
//!
//! The job owns only fixed-size recognizer state and one bounded refill buffer.
//! The source remains behind [`RefillableLineSource`], and completed output is
//! ranges, metrics, and grammar facts rather than copied text.
//!
//! # Donor authority
//!
//! These standalone `RefillableLineKind` results remain feasibility facts, not
//! parser authority. Oversized production admission now enters
//! [`crate::parser::DirectValueBlockParser::begin_source_line_work`], whose
//! bounded scan supplies only physical metrics and a controller window before
//! the existing donor `LineTransition` decides the block. No result from this
//! module may be used to bypass that controller.

use std::cmp::min;
use std::ops::Range;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::parser::{
    DirectCoveragePart, DirectFenceCharacter, DirectFencedCodeFacts, DirectLineEnding,
    DirectLogicalAction,
};
use crate::source_ledger::{
    BoundaryAffinity, RefillableSourceLine, RefillableSourceLineKey, SourceMetric, SourceRevision,
};

pub const DEFAULT_REFILL_WINDOW_BYTES: usize = 4 * 1024;
pub const MAX_REFILL_WINDOW_BYTES: usize = 64 * 1024;

const CANCELLATION_CHECK_INTERVAL: usize = 64;
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Source-owned bounded reader for one exact line descriptor.
///
/// `read_window` receives a line-relative byte offset and a destination whose
/// length is the maximum permitted synchronous read. A successful non-final
/// read must return at least one byte and may never report more bytes than the
/// destination length. Implementations need not align windows to UTF-8 scalar
/// boundaries; the recognizer carries the at-most-three-byte partial scalar.
pub trait RefillableLineSource {
    fn line_key(&self) -> RefillableSourceLineKey;

    /// Pull at most `destination.len()` bytes beginning at `relative_start`.
    ///
    /// # Errors
    ///
    /// Returns a backend-specific, intentionally non-stringly error category.
    fn read_window(
        &self,
        relative_start: u64,
        destination: &mut [u8],
    ) -> Result<usize, RefillableSourceReadError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefillableSourceReadError {
    Unavailable,
    RevisionChanged,
}

#[derive(Clone, Debug, Default)]
pub struct RefillableCancellationToken(Arc<AtomicBool>);

impl RefillableCancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefillableLineContext {
    Document,
    FencedCode(DirectFencedCodeFacts),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefillableLineKind {
    Paragraph,
    FencedCodeOpening(DirectFencedCodeFacts),
    FencedCodeLiteral,
    FencedCodeClosing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefillableClaimOwner {
    Document,
    Paragraph,
    FencedCode,
}

/// Writer action corresponding to one exact physical claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefillableClaimAction {
    Consume(DirectLogicalAction),
    StageParagraphTerminator { ending: DirectLineEnding },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefillableCoverageClaim {
    pub owner: RefillableClaimOwner,
    pub part: DirectCoveragePart,
    pub relative_range: Range<u64>,
    pub absolute_range: Range<u64>,
    pub metric: SourceMetric,
    pub action: RefillableClaimAction,
    pub affinity: BoundaryAffinity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefillableLineResult {
    pub revision: SourceRevision,
    pub line_ordinal: u64,
    pub absolute_start: u64,
    pub metric: SourceMetric,
    pub ending: Option<DirectLineEnding>,
    pub kind: RefillableLineKind,
    claims: Box<[RefillableCoverageClaim]>,
    pub provenance_digest: u64,
}

impl RefillableLineResult {
    #[must_use]
    pub fn claims(&self) -> &[RefillableCoverageClaim] {
        &self.claims
    }

    /// Rechecks that the fixed claim recipe is one exact physical partition.
    #[must_use]
    pub fn coverage_is_complete(&self) -> bool {
        let mut next = 0_u64;
        let mut metric = SourceMetric::default();
        for claim in &self.claims {
            if claim.relative_range.start != next
                || claim.relative_range.end <= claim.relative_range.start
            {
                return false;
            }
            let Some(bytes) = metric.bytes.checked_add(claim.metric.bytes) else {
                return false;
            };
            let Some(utf16) = metric.utf16.checked_add(claim.metric.utf16) else {
                return false;
            };
            metric = SourceMetric { bytes, utf16 };
            next = claim.relative_range.end;
        }
        next == self.metric.bytes && metric == self.metric
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefillablePhase {
    PrefixRecognition,
    BodyCoverage,
    Complete,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefillablePollStatus {
    Pending,
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefillablePollReceipt {
    pub status: RefillablePollStatus,
    pub phase: RefillablePhase,
    pub bytes_inspected: usize,
    pub source_reads: usize,
    pub cancellation_checks: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefillableScanReceipt {
    pub polls: usize,
    pub source_reads: usize,
    pub source_bytes_read: u64,
    pub bytes_inspected: u64,
    pub maximum_source_bytes_per_poll: usize,
    pub maximum_bytes_inspected_per_poll: usize,
    pub prefix_bytes_inspected: u64,
    pub body_bytes_covered: u64,
    pub terminal_bytes_inspected: u64,
    pub cancellation_checks: usize,
    pub scratch_capacity_bytes: usize,
    /// Always zero: source bytes live only in the bounded scratch window.
    pub retained_source_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefillableCancellationReceipt {
    pub abandoned_phase: RefillablePhase,
    pub bytes_inspected: u64,
    pub prefix_bytes_inspected: u64,
    pub body_bytes_covered: u64,
    pub scratch_capacity_bytes: usize,
    pub retained_source_bytes: usize,
    pub completed_claims: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefillableLineError {
    InvalidWindowBytes {
        requested: usize,
    },
    InvalidFenceFacts,
    ZeroFuel,
    WrongSourceLine,
    SourceRead(RefillableSourceReadError),
    SourceStalled {
        relative_start: u64,
    },
    SourceOverrun {
        reported: usize,
        permitted: usize,
    },
    InvalidUtf8 {
        relative_offset: u64,
    },
    MetricMismatch {
        source: SourceMetric,
        derived: SourceMetric,
    },
    EmbeddedLineEnding {
        relative_offset: u64,
    },
    TabOrNul {
        relative_offset: u64,
    },
    UnsupportedDocumentPrefix {
        relative_offset: u64,
        byte: Option<u8>,
    },
    MetricOverflow,
    AbsoluteRangeOverflow,
    PollAfterFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalState {
    Running,
    Complete,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecognitionDecision {
    Paragraph {
        content_start: u64,
    },
    FenceOpening {
        facts: DirectFencedCodeFacts,
        marker_end: u64,
    },
    FenceLiteral {
        deindent_end: u64,
    },
    FenceClosing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DocumentPrefix {
    Leading,
    FenceRun { marker: u8, run: u64 },
    BacktickInfo { run: u64, marker_end: u64 },
    Decided,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DocumentRecognition {
    leading_spaces: u8,
    prefix: DocumentPrefix,
    decision: Option<RecognitionDecision>,
}

impl DocumentRecognition {
    const fn new() -> Self {
        Self {
            leading_spaces: 0,
            prefix: DocumentPrefix::Leading,
            decision: None,
        }
    }

    fn consume(&mut self, byte: u8, offset: u64) -> Result<(), RefillableLineError> {
        match self.prefix {
            DocumentPrefix::Leading => self.consume_leading(byte, offset),
            DocumentPrefix::FenceRun { marker, run } => {
                self.consume_fence_run(byte, offset, marker, run)
            }
            DocumentPrefix::BacktickInfo { run, marker_end } => {
                if byte == b'`' {
                    self.decide_paragraph();
                } else {
                    self.prefix = DocumentPrefix::BacktickInfo { run, marker_end };
                }
                Ok(())
            }
            DocumentPrefix::Decided => Ok(()),
        }
    }

    fn consume_leading(&mut self, byte: u8, offset: u64) -> Result<(), RefillableLineError> {
        if byte == b' ' {
            self.leading_spaces = self
                .leading_spaces
                .checked_add(1)
                .ok_or(RefillableLineError::MetricOverflow)?;
            if self.leading_spaces > 3 {
                return Err(RefillableLineError::UnsupportedDocumentPrefix {
                    relative_offset: offset,
                    byte: Some(byte),
                });
            }
            return Ok(());
        }
        if matches!(byte, b'`' | b'~') {
            self.prefix = DocumentPrefix::FenceRun {
                marker: byte,
                run: 1,
            };
            return Ok(());
        }
        if is_potential_block_start(byte) {
            return Err(RefillableLineError::UnsupportedDocumentPrefix {
                relative_offset: offset,
                byte: Some(byte),
            });
        }
        self.decide_paragraph();
        Ok(())
    }

    fn consume_fence_run(
        &mut self,
        byte: u8,
        offset: u64,
        marker: u8,
        run: u64,
    ) -> Result<(), RefillableLineError> {
        if byte == marker {
            self.prefix = DocumentPrefix::FenceRun {
                marker,
                run: run
                    .checked_add(1)
                    .ok_or(RefillableLineError::MetricOverflow)?,
            };
            return Ok(());
        }
        if run < 3 {
            self.decide_paragraph();
            return Ok(());
        }
        if marker == b'~' {
            self.decide(RecognitionDecision::FenceOpening {
                facts: DirectFencedCodeFacts {
                    fence: DirectFenceCharacter::Tilde,
                    minimum_closing_length: run,
                    fence_offset_columns: self.leading_spaces,
                },
                marker_end: offset,
            });
        } else {
            self.prefix = DocumentPrefix::BacktickInfo {
                run,
                marker_end: offset,
            };
        }
        Ok(())
    }

    fn finish(&mut self, content_end: u64) -> Result<RecognitionDecision, RefillableLineError> {
        match self.prefix {
            DocumentPrefix::Leading => {
                return Err(RefillableLineError::UnsupportedDocumentPrefix {
                    relative_offset: content_end,
                    byte: None,
                });
            }
            DocumentPrefix::FenceRun { marker, run } if run >= 3 => {
                let fence = if marker == b'`' {
                    DirectFenceCharacter::Backtick
                } else {
                    DirectFenceCharacter::Tilde
                };
                self.decide(RecognitionDecision::FenceOpening {
                    facts: DirectFencedCodeFacts {
                        fence,
                        minimum_closing_length: run,
                        fence_offset_columns: self.leading_spaces,
                    },
                    marker_end: content_end,
                });
            }
            DocumentPrefix::FenceRun { .. } => self.decide_paragraph(),
            DocumentPrefix::BacktickInfo { run, marker_end } => {
                self.decide(RecognitionDecision::FenceOpening {
                    facts: DirectFencedCodeFacts {
                        fence: DirectFenceCharacter::Backtick,
                        minimum_closing_length: run,
                        fence_offset_columns: self.leading_spaces,
                    },
                    marker_end,
                });
            }
            DocumentPrefix::Decided => {}
        }
        self.decision.ok_or(RefillableLineError::MetricOverflow)
    }

    fn decide(&mut self, decision: RecognitionDecision) {
        self.decision = Some(decision);
        self.prefix = DocumentPrefix::Decided;
    }

    fn decide_paragraph(&mut self) {
        self.decide(RecognitionDecision::Paragraph {
            content_start: u64::from(self.leading_spaces),
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FencePrefix {
    Leading,
    MarkerRun { run: u64 },
    ClosingWhitespace,
    Decided,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FenceRecognition {
    facts: DirectFencedCodeFacts,
    leading_spaces: u64,
    prefix: FencePrefix,
    decision: Option<RecognitionDecision>,
}

impl FenceRecognition {
    const fn new(facts: DirectFencedCodeFacts) -> Self {
        Self {
            facts,
            leading_spaces: 0,
            prefix: FencePrefix::Leading,
            decision: None,
        }
    }

    fn consume(&mut self, byte: u8) -> Result<(), RefillableLineError> {
        match self.prefix {
            FencePrefix::Leading => {
                if byte == b' ' {
                    self.leading_spaces = self
                        .leading_spaces
                        .checked_add(1)
                        .ok_or(RefillableLineError::MetricOverflow)?;
                    if self.leading_spaces > 3 {
                        self.decide_literal();
                    }
                } else if byte == self.facts.fence.marker() {
                    self.prefix = FencePrefix::MarkerRun { run: 1 };
                } else {
                    self.decide_literal();
                }
            }
            FencePrefix::MarkerRun { run } => {
                if byte == self.facts.fence.marker() {
                    self.prefix = FencePrefix::MarkerRun {
                        run: run
                            .checked_add(1)
                            .ok_or(RefillableLineError::MetricOverflow)?,
                    };
                } else if run < self.facts.minimum_closing_length {
                    self.decide_literal();
                } else if byte == b' ' {
                    self.prefix = FencePrefix::ClosingWhitespace;
                } else {
                    self.decide_literal();
                }
            }
            FencePrefix::ClosingWhitespace => {
                if byte != b' ' {
                    self.decide_literal();
                }
            }
            FencePrefix::Decided => {}
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<RecognitionDecision, RefillableLineError> {
        match self.prefix {
            FencePrefix::MarkerRun { run } if run >= self.facts.minimum_closing_length => {
                self.decision = Some(RecognitionDecision::FenceClosing);
                self.prefix = FencePrefix::Decided;
            }
            FencePrefix::Leading | FencePrefix::MarkerRun { .. } => self.decide_literal(),
            FencePrefix::ClosingWhitespace => {
                self.decision = Some(RecognitionDecision::FenceClosing);
                self.prefix = FencePrefix::Decided;
            }
            FencePrefix::Decided => {}
        }
        self.decision.ok_or(RefillableLineError::MetricOverflow)
    }

    fn decide_literal(&mut self) {
        self.decision = Some(RecognitionDecision::FenceLiteral {
            deindent_end: min(
                self.leading_spaces,
                u64::from(self.facts.fence_offset_columns),
            ),
        });
        self.prefix = FencePrefix::Decided;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Recognition {
    Document(DocumentRecognition),
    Fence(FenceRecognition),
}

impl Recognition {
    const fn new(context: RefillableLineContext) -> Self {
        match context {
            RefillableLineContext::Document => Self::Document(DocumentRecognition::new()),
            RefillableLineContext::FencedCode(facts) => Self::Fence(FenceRecognition::new(facts)),
        }
    }

    fn consume(&mut self, byte: u8, offset: u64) -> Result<(), RefillableLineError> {
        match self {
            Self::Document(recognition) => recognition.consume(byte, offset),
            Self::Fence(recognition) => recognition.consume(byte),
        }
    }

    fn decision(self) -> Option<RecognitionDecision> {
        match self {
            Self::Document(recognition) => recognition.decision,
            Self::Fence(recognition) => recognition.decision,
        }
    }

    fn finish(&mut self, content_end: u64) -> Result<RecognitionDecision, RefillableLineError> {
        match self {
            Self::Document(recognition) => recognition.finish(content_end),
            Self::Fence(recognition) => recognition.finish(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Utf8Fold {
    remaining: u8,
    code_point: u32,
    minimum: u32,
    utf16: u64,
}

impl Utf8Fold {
    fn push(&mut self, byte: u8, offset: u64) -> Result<(), RefillableLineError> {
        if self.remaining == 0 {
            match byte {
                0x00..=0x7f => self.add_utf16(1),
                0xc2..=0xdf => {
                    self.remaining = 1;
                    self.code_point = u32::from(byte & 0x1f);
                    self.minimum = 0x80;
                    Ok(())
                }
                0xe0..=0xef => {
                    self.remaining = 2;
                    self.code_point = u32::from(byte & 0x0f);
                    self.minimum = 0x800;
                    Ok(())
                }
                0xf0..=0xf4 => {
                    self.remaining = 3;
                    self.code_point = u32::from(byte & 0x07);
                    self.minimum = 0x1_0000;
                    Ok(())
                }
                _ => Err(RefillableLineError::InvalidUtf8 {
                    relative_offset: offset,
                }),
            }
        } else {
            if byte & 0xc0 != 0x80 {
                return Err(RefillableLineError::InvalidUtf8 {
                    relative_offset: offset,
                });
            }
            self.code_point = (self.code_point << 6) | u32::from(byte & 0x3f);
            self.remaining -= 1;
            if self.remaining != 0 {
                return Ok(());
            }
            if self.code_point < self.minimum
                || self.code_point > 0x10_ffff
                || (0xd800..=0xdfff).contains(&self.code_point)
            {
                return Err(RefillableLineError::InvalidUtf8 {
                    relative_offset: offset,
                });
            }
            self.add_utf16(if self.code_point <= 0xffff { 1 } else { 2 })
        }
    }

    fn finish(self, offset: u64) -> Result<u64, RefillableLineError> {
        if self.remaining == 0 {
            Ok(self.utf16)
        } else {
            Err(RefillableLineError::InvalidUtf8 {
                relative_offset: offset,
            })
        }
    }

    fn add_utf16(&mut self, units: u64) -> Result<(), RefillableLineError> {
        self.utf16 = self
            .utf16
            .checked_add(units)
            .ok_or(RefillableLineError::MetricOverflow)?;
        Ok(())
    }
}

/// Resumable recognizer/coverage job for one certified physical line.
pub struct RefillableLineJob {
    line: RefillableSourceLine,
    expected_key: RefillableSourceLineKey,
    recognition: Recognition,
    scratch: Vec<u8>,
    read_offset: u64,
    ending_tail: [u8; 2],
    ending_tail_len: usize,
    utf8: Utf8Fold,
    phase: RefillablePhase,
    terminal: TerminalState,
    scan: RefillableScanReceipt,
    result: Option<RefillableLineResult>,
    cancellation: Option<RefillableCancellationReceipt>,
}

impl RefillableLineJob {
    /// Admits a certified line in O(1) with respect to its physical length.
    ///
    /// # Errors
    ///
    /// Returns an error for an unbounded/zero refill window or invalid fenced
    /// context facts.
    pub fn new(
        line: RefillableSourceLine,
        context: RefillableLineContext,
        window_bytes: usize,
    ) -> Result<Self, RefillableLineError> {
        if window_bytes == 0 || window_bytes > MAX_REFILL_WINDOW_BYTES {
            return Err(RefillableLineError::InvalidWindowBytes {
                requested: window_bytes,
            });
        }
        if let RefillableLineContext::FencedCode(facts) = context
            && (facts.minimum_closing_length < 3 || facts.fence_offset_columns > 3)
        {
            return Err(RefillableLineError::InvalidFenceFacts);
        }
        let expected_key = line.key();
        let scratch = vec![0; window_bytes];
        let scratch_capacity_bytes = scratch.capacity();
        Ok(Self {
            line,
            expected_key,
            recognition: Recognition::new(context),
            scratch,
            read_offset: 0,
            ending_tail: [0; 2],
            ending_tail_len: 0,
            utf8: Utf8Fold::default(),
            phase: RefillablePhase::PrefixRecognition,
            terminal: TerminalState::Running,
            scan: RefillableScanReceipt {
                scratch_capacity_bytes,
                ..RefillableScanReceipt::default()
            },
            result: None,
            cancellation: None,
        })
    }

    #[must_use]
    pub const fn phase(&self) -> RefillablePhase {
        self.phase
    }

    #[must_use]
    pub const fn scan_receipt(&self) -> RefillableScanReceipt {
        self.scan
    }

    #[must_use]
    pub fn result(&self) -> Option<&RefillableLineResult> {
        self.result.as_ref()
    }

    #[must_use]
    pub const fn cancellation_receipt(&self) -> Option<RefillableCancellationReceipt> {
        self.cancellation
    }

    /// The job never owns the source line or a logical payload copy.
    #[must_use]
    pub const fn retained_source_bytes(&self) -> usize {
        0
    }

    /// Pull and inspect at most `min(fuel, configured_window)` source bytes.
    ///
    /// # Errors
    ///
    /// Fails closed on source identity/protocol violations, invalid UTF-8,
    /// metric mismatch, unsupported direct-parser input, or an ambiguous
    /// document prefix outside this feasibility slice.
    pub fn poll<S: RefillableLineSource>(
        &mut self,
        source: &S,
        fuel: usize,
        cancellation: &RefillableCancellationToken,
    ) -> Result<RefillablePollReceipt, RefillableLineError> {
        if fuel == 0 {
            return Err(RefillableLineError::ZeroFuel);
        }
        match self.terminal {
            TerminalState::Complete => return Ok(self.idle_receipt(RefillablePollStatus::Complete)),
            TerminalState::Cancelled => {
                return Ok(self.idle_receipt(RefillablePollStatus::Cancelled));
            }
            TerminalState::Failed => return Err(RefillableLineError::PollAfterFailure),
            TerminalState::Running => {}
        }
        if source.line_key() != self.expected_key {
            self.fail();
            return Err(RefillableLineError::WrongSourceLine);
        }

        self.scan.polls = self.scan.polls.saturating_add(1);
        let mut poll = RefillablePollReceipt {
            status: RefillablePollStatus::Pending,
            phase: self.phase,
            bytes_inspected: 0,
            source_reads: 0,
            cancellation_checks: 0,
        };
        self.check_cancellation(cancellation, &mut poll);
        if self.terminal == TerminalState::Cancelled {
            poll.status = RefillablePollStatus::Cancelled;
            poll.phase = self.phase;
            self.record_poll_maxima(&poll, 0);
            return Ok(poll);
        }

        if self.read_offset == self.line.metric().bytes {
            self.complete_line()?;
            poll.status = RefillablePollStatus::Complete;
            poll.phase = self.phase;
            self.record_poll_maxima(&poll, 0);
            return Ok(poll);
        }

        let source_bytes = match self.pull_window(source, fuel, &mut poll) {
            Ok(source_bytes) => source_bytes,
            Err(error) => {
                self.fail();
                return Err(error);
            }
        };
        let process_result = self.process_window(source_bytes, cancellation, &mut poll);
        if let Err(error) = process_result {
            self.fail();
            return Err(error);
        }
        if self.terminal == TerminalState::Cancelled {
            poll.status = RefillablePollStatus::Cancelled;
        } else if self.read_offset == self.line.metric().bytes {
            if let Err(error) = self.complete_line() {
                self.fail();
                return Err(error);
            }
            poll.status = RefillablePollStatus::Complete;
        }
        poll.phase = self.phase;
        self.record_poll_maxima(&poll, source_bytes);
        Ok(poll)
    }

    fn pull_window<S: RefillableLineSource>(
        &mut self,
        source: &S,
        fuel: usize,
        poll: &mut RefillablePollReceipt,
    ) -> Result<usize, RefillableLineError> {
        let remaining = self.line.metric().bytes - self.read_offset;
        let remaining = usize::try_from(min(
            remaining,
            u64::try_from(usize::MAX).unwrap_or(u64::MAX),
        ))
        .map_err(|_| RefillableLineError::MetricOverflow)?;
        let permitted = min(min(fuel, self.scratch.len()), remaining);
        let read = source
            .read_window(self.read_offset, &mut self.scratch[..permitted])
            .map_err(RefillableLineError::SourceRead)?;
        poll.source_reads = 1;
        self.scan.source_reads = self.scan.source_reads.saturating_add(1);
        if read > permitted {
            return Err(RefillableLineError::SourceOverrun {
                reported: read,
                permitted,
            });
        }
        if read == 0 {
            return Err(RefillableLineError::SourceStalled {
                relative_start: self.read_offset,
            });
        }
        self.scan.source_bytes_read = self
            .scan
            .source_bytes_read
            .checked_add(u64::try_from(read).map_err(|_| RefillableLineError::MetricOverflow)?)
            .ok_or(RefillableLineError::MetricOverflow)?;
        Ok(read)
    }

    fn process_window(
        &mut self,
        source_bytes: usize,
        cancellation: &RefillableCancellationToken,
        poll: &mut RefillablePollReceipt,
    ) -> Result<(), RefillableLineError> {
        for index in 0..source_bytes {
            if index > 0 && index % CANCELLATION_CHECK_INTERVAL == 0 {
                self.check_cancellation(cancellation, poll);
                if self.terminal == TerminalState::Cancelled {
                    break;
                }
            }
            let byte = self.scratch[index];
            self.push_physical(byte)?;
            self.read_offset = self
                .read_offset
                .checked_add(1)
                .ok_or(RefillableLineError::MetricOverflow)?;
            poll.bytes_inspected = poll.bytes_inspected.saturating_add(1);
            self.scan.bytes_inspected = self
                .scan
                .bytes_inspected
                .checked_add(1)
                .ok_or(RefillableLineError::MetricOverflow)?;
        }
        self.check_cancellation(cancellation, poll);
        Ok(())
    }

    fn push_physical(&mut self, byte: u8) -> Result<(), RefillableLineError> {
        let offset = self.read_offset;
        if matches!(byte, b'\t' | b'\0') {
            return Err(RefillableLineError::TabOrNul {
                relative_offset: offset,
            });
        }
        self.utf8.push(byte, offset)?;
        if self.ending_tail_len < self.ending_tail.len() {
            self.ending_tail[self.ending_tail_len] = byte;
            self.ending_tail_len += 1;
            return Ok(());
        }
        let content_byte = self.ending_tail[0];
        self.ending_tail[0] = self.ending_tail[1];
        self.ending_tail[1] = byte;
        self.feed_content(content_byte, offset - 2)
    }

    fn feed_content(&mut self, byte: u8, offset: u64) -> Result<(), RefillableLineError> {
        if matches!(byte, b'\r' | b'\n') {
            return Err(RefillableLineError::EmbeddedLineEnding {
                relative_offset: offset,
            });
        }
        let was_prefix = self.recognition.decision().is_none();
        self.recognition.consume(byte, offset)?;
        if was_prefix {
            self.scan.prefix_bytes_inspected = self
                .scan
                .prefix_bytes_inspected
                .checked_add(1)
                .ok_or(RefillableLineError::MetricOverflow)?;
        } else {
            self.scan.body_bytes_covered = self
                .scan
                .body_bytes_covered
                .checked_add(1)
                .ok_or(RefillableLineError::MetricOverflow)?;
        }
        if self.recognition.decision().is_some() {
            self.phase = RefillablePhase::BodyCoverage;
        }
        Ok(())
    }

    fn complete_line(&mut self) -> Result<(), RefillableLineError> {
        let derived = SourceMetric {
            bytes: self.read_offset,
            utf16: self.utf8.finish(self.read_offset)?,
        };
        if derived != self.line.metric() {
            return Err(RefillableLineError::MetricMismatch {
                source: self.line.metric(),
                derived,
            });
        }
        let (content_end, ending) = self.finish_ending_tail()?;
        let decision = self.recognition.finish(content_end)?;
        let result = build_result(&self.line, decision, content_end, ending)?;
        debug_assert!(result.coverage_is_complete());
        self.result = Some(result);
        self.terminal = TerminalState::Complete;
        self.phase = RefillablePhase::Complete;
        Ok(())
    }

    fn finish_ending_tail(
        &mut self,
    ) -> Result<(u64, Option<DirectLineEnding>), RefillableLineError> {
        let tail = self.ending_tail;
        let len = self.ending_tail_len;
        self.ending_tail_len = 0;
        let (body_len, ending) = if len == 2 && tail == [b'\r', b'\n'] {
            (0, Some(DirectLineEnding::CrLf))
        } else if len > 0 && tail[len - 1] == b'\n' {
            (len - 1, Some(DirectLineEnding::Lf))
        } else if len > 0 && tail[len - 1] == b'\r' {
            (len - 1, Some(DirectLineEnding::Cr))
        } else {
            (len, None)
        };
        let tail_start = self
            .read_offset
            .checked_sub(u64::try_from(len).map_err(|_| RefillableLineError::MetricOverflow)?)
            .ok_or(RefillableLineError::MetricOverflow)?;
        for (index, byte) in tail[..body_len].iter().copied().enumerate() {
            let offset = tail_start
                .checked_add(u64::try_from(index).map_err(|_| RefillableLineError::MetricOverflow)?)
                .ok_or(RefillableLineError::MetricOverflow)?;
            self.feed_content(byte, offset)?;
        }
        let ending_bytes =
            u64::try_from(len - body_len).map_err(|_| RefillableLineError::MetricOverflow)?;
        self.scan.terminal_bytes_inspected = self
            .scan
            .terminal_bytes_inspected
            .checked_add(ending_bytes)
            .ok_or(RefillableLineError::MetricOverflow)?;
        let content_end = self
            .read_offset
            .checked_sub(ending_bytes)
            .ok_or(RefillableLineError::MetricOverflow)?;
        Ok((content_end, ending))
    }

    fn check_cancellation(
        &mut self,
        token: &RefillableCancellationToken,
        poll: &mut RefillablePollReceipt,
    ) {
        poll.cancellation_checks = poll.cancellation_checks.saturating_add(1);
        self.scan.cancellation_checks = self.scan.cancellation_checks.saturating_add(1);
        if !token.is_cancelled() || self.terminal != TerminalState::Running {
            return;
        }
        let abandoned_phase = self.phase;
        self.cancellation = Some(RefillableCancellationReceipt {
            abandoned_phase,
            bytes_inspected: self.scan.bytes_inspected,
            prefix_bytes_inspected: self.scan.prefix_bytes_inspected,
            body_bytes_covered: self.scan.body_bytes_covered,
            scratch_capacity_bytes: self.scan.scratch_capacity_bytes,
            retained_source_bytes: 0,
            completed_claims: 0,
        });
        self.terminal = TerminalState::Cancelled;
        self.phase = RefillablePhase::Cancelled;
    }

    fn record_poll_maxima(&mut self, poll: &RefillablePollReceipt, source_bytes: usize) {
        self.scan.maximum_source_bytes_per_poll =
            self.scan.maximum_source_bytes_per_poll.max(source_bytes);
        self.scan.maximum_bytes_inspected_per_poll = self
            .scan
            .maximum_bytes_inspected_per_poll
            .max(poll.bytes_inspected);
    }

    fn idle_receipt(&self, status: RefillablePollStatus) -> RefillablePollReceipt {
        RefillablePollReceipt {
            status,
            phase: self.phase,
            bytes_inspected: 0,
            source_reads: 0,
            cancellation_checks: 0,
        }
    }

    fn fail(&mut self) {
        self.terminal = TerminalState::Failed;
        self.phase = RefillablePhase::Failed;
    }
}

fn build_result(
    line: &RefillableSourceLine,
    decision: RecognitionDecision,
    content_end: u64,
    ending: Option<DirectLineEnding>,
) -> Result<RefillableLineResult, RefillableLineError> {
    let ending_bytes = line
        .metric()
        .bytes
        .checked_sub(content_end)
        .ok_or(RefillableLineError::MetricOverflow)?;
    let (kind, claims) = match decision {
        RecognitionDecision::Paragraph { content_start } => {
            let claims = paragraph_claims(line, content_start, content_end, ending)?;
            (RefillableLineKind::Paragraph, claims)
        }
        RecognitionDecision::FenceOpening { facts, marker_end } => {
            let claims = fence_opening_claims(line, marker_end, content_end, ending_bytes)?;
            (RefillableLineKind::FencedCodeOpening(facts), claims)
        }
        RecognitionDecision::FenceLiteral { deindent_end } => {
            let claims = fence_literal_claims(line, deindent_end, content_end, ending_bytes)?;
            (RefillableLineKind::FencedCodeLiteral, claims)
        }
        RecognitionDecision::FenceClosing => {
            let claims = fence_closing_claims(line, content_end, ending_bytes)?;
            (RefillableLineKind::FencedCodeClosing, claims)
        }
    };
    finish_result(line, ending, kind, claims)
}

fn paragraph_claims(
    line: &RefillableSourceLine,
    content_start: u64,
    content_end: u64,
    ending: Option<DirectLineEnding>,
) -> Result<Vec<RefillableCoverageClaim>, RefillableLineError> {
    let mut claims = Vec::with_capacity(3);
    push_claim(
        &mut claims,
        line,
        RefillableClaimOwner::Document,
        DirectCoveragePart::Gap,
        0..content_start,
        RefillableClaimAction::Consume(DirectLogicalAction::None),
        BoundaryAffinity::Downstream,
    )?;
    push_claim(
        &mut claims,
        line,
        RefillableClaimOwner::Paragraph,
        DirectCoveragePart::Content,
        content_start..content_end,
        RefillableClaimAction::Consume(DirectLogicalAction::CanonicalText),
        BoundaryAffinity::Downstream,
    )?;
    if let Some(ending) = ending {
        push_claim(
            &mut claims,
            line,
            RefillableClaimOwner::Paragraph,
            DirectCoveragePart::Terminal,
            content_end..line.metric().bytes,
            RefillableClaimAction::StageParagraphTerminator { ending },
            BoundaryAffinity::Upstream,
        )?;
    }
    Ok(claims)
}

fn fence_opening_claims(
    line: &RefillableSourceLine,
    marker_end: u64,
    content_end: u64,
    ending_bytes: u64,
) -> Result<Vec<RefillableCoverageClaim>, RefillableLineError> {
    let mut claims = Vec::with_capacity(3);
    push_claim(
        &mut claims,
        line,
        RefillableClaimOwner::FencedCode,
        DirectCoveragePart::BlockMarker,
        0..marker_end,
        RefillableClaimAction::Consume(DirectLogicalAction::None),
        BoundaryAffinity::Downstream,
    )?;
    push_claim(
        &mut claims,
        line,
        RefillableClaimOwner::FencedCode,
        DirectCoveragePart::Content,
        marker_end..content_end,
        RefillableClaimAction::Consume(DirectLogicalAction::CanonicalText),
        BoundaryAffinity::Downstream,
    )?;
    push_canonical_ending(&mut claims, line, content_end, ending_bytes)?;
    Ok(claims)
}

fn fence_literal_claims(
    line: &RefillableSourceLine,
    deindent_end: u64,
    content_end: u64,
    ending_bytes: u64,
) -> Result<Vec<RefillableCoverageClaim>, RefillableLineError> {
    let mut claims = Vec::with_capacity(3);
    push_claim(
        &mut claims,
        line,
        RefillableClaimOwner::FencedCode,
        DirectCoveragePart::ContainerMarker,
        0..deindent_end,
        RefillableClaimAction::Consume(DirectLogicalAction::None),
        BoundaryAffinity::Downstream,
    )?;
    push_claim(
        &mut claims,
        line,
        RefillableClaimOwner::FencedCode,
        DirectCoveragePart::Content,
        deindent_end..content_end,
        RefillableClaimAction::Consume(DirectLogicalAction::CanonicalText),
        BoundaryAffinity::Downstream,
    )?;
    push_canonical_ending(&mut claims, line, content_end, ending_bytes)?;
    Ok(claims)
}

fn push_canonical_ending(
    claims: &mut Vec<RefillableCoverageClaim>,
    line: &RefillableSourceLine,
    content_end: u64,
    ending_bytes: u64,
) -> Result<(), RefillableLineError> {
    if ending_bytes == 0 {
        return Ok(());
    }
    push_claim(
        claims,
        line,
        RefillableClaimOwner::FencedCode,
        DirectCoveragePart::Content,
        content_end..line.metric().bytes,
        RefillableClaimAction::Consume(DirectLogicalAction::CanonicalNewline),
        BoundaryAffinity::Downstream,
    )
}

fn fence_closing_claims(
    line: &RefillableSourceLine,
    content_end: u64,
    ending_bytes: u64,
) -> Result<Vec<RefillableCoverageClaim>, RefillableLineError> {
    let mut claims = Vec::with_capacity(2);
    push_claim(
        &mut claims,
        line,
        RefillableClaimOwner::FencedCode,
        DirectCoveragePart::BlockMarker,
        0..content_end,
        RefillableClaimAction::Consume(DirectLogicalAction::None),
        BoundaryAffinity::Downstream,
    )?;
    if ending_bytes > 0 {
        push_claim(
            &mut claims,
            line,
            RefillableClaimOwner::FencedCode,
            DirectCoveragePart::Terminal,
            content_end..line.metric().bytes,
            RefillableClaimAction::Consume(DirectLogicalAction::None),
            BoundaryAffinity::Downstream,
        )?;
    }
    Ok(claims)
}

fn push_claim(
    claims: &mut Vec<RefillableCoverageClaim>,
    line: &RefillableSourceLine,
    owner: RefillableClaimOwner,
    part: DirectCoveragePart,
    relative_range: Range<u64>,
    action: RefillableClaimAction,
    affinity: BoundaryAffinity,
) -> Result<(), RefillableLineError> {
    if relative_range.start == relative_range.end {
        return Ok(());
    }
    let bytes = relative_range
        .end
        .checked_sub(relative_range.start)
        .ok_or(RefillableLineError::MetricOverflow)?;
    let terminal_bytes = match action {
        RefillableClaimAction::StageParagraphTerminator { .. }
        | RefillableClaimAction::Consume(DirectLogicalAction::CanonicalNewline) => bytes,
        RefillableClaimAction::Consume(_) => 0,
    };
    let known_ascii = matches!(
        part,
        DirectCoveragePart::BlockMarker
            | DirectCoveragePart::ContainerMarker
            | DirectCoveragePart::Gap
    ) || terminal_bytes > 0;
    let utf16 = if known_ascii {
        bytes
    } else {
        let before_bytes: u64 = claims.iter().map(|claim| claim.metric.bytes).sum();
        let before_utf16: u64 = claims.iter().map(|claim| claim.metric.utf16).sum();
        let bytes_after = line
            .metric()
            .bytes
            .checked_sub(before_bytes)
            .and_then(|value| value.checked_sub(bytes))
            .ok_or(RefillableLineError::MetricOverflow)?;
        let utf16_after = if relative_range.end == line.metric().bytes {
            0
        } else if bytes_after <= 2 {
            bytes_after
        } else {
            return Err(RefillableLineError::MetricOverflow);
        };
        line.metric()
            .utf16
            .checked_sub(before_utf16)
            .and_then(|value| value.checked_sub(utf16_after))
            .ok_or(RefillableLineError::MetricOverflow)?
    };
    let absolute_start = line
        .absolute_start()
        .checked_add(relative_range.start)
        .ok_or(RefillableLineError::AbsoluteRangeOverflow)?;
    let absolute_end = line
        .absolute_start()
        .checked_add(relative_range.end)
        .ok_or(RefillableLineError::AbsoluteRangeOverflow)?;
    claims.push(RefillableCoverageClaim {
        owner,
        part,
        relative_range,
        absolute_range: absolute_start..absolute_end,
        metric: SourceMetric { bytes, utf16 },
        action,
        affinity,
    });
    Ok(())
}

fn finish_result(
    line: &RefillableSourceLine,
    ending: Option<DirectLineEnding>,
    kind: RefillableLineKind,
    claims: Vec<RefillableCoverageClaim>,
) -> Result<RefillableLineResult, RefillableLineError> {
    let provenance_digest = claims
        .iter()
        .fold(result_digest_seed(line, ending, kind), fold_claim);
    let result = RefillableLineResult {
        revision: line.revision(),
        line_ordinal: line.line_ordinal(),
        absolute_start: line.absolute_start(),
        metric: line.metric(),
        ending,
        kind,
        claims: claims.into_boxed_slice(),
        provenance_digest,
    };
    if result.coverage_is_complete() {
        Ok(result)
    } else {
        Err(RefillableLineError::MetricOverflow)
    }
}

fn result_digest_seed(
    line: &RefillableSourceLine,
    ending: Option<DirectLineEnding>,
    kind: RefillableLineKind,
) -> u64 {
    let mut digest = FNV_OFFSET_BASIS;
    for value in [
        line.revision().0,
        line.line_ordinal(),
        line.absolute_start(),
        line.metric().bytes,
        line.metric().utf16,
        u64::from(ending_tag(ending)),
        u64::from(line_kind_tag(kind)),
    ] {
        digest = fold_u64(digest, value);
    }
    if let RefillableLineKind::FencedCodeOpening(facts) = kind {
        digest = fold_u64(digest, u64::from(facts.fence.marker()));
        digest = fold_u64(digest, facts.minimum_closing_length);
        digest = fold_u64(digest, u64::from(facts.fence_offset_columns));
    }
    digest
}

fn fold_claim(mut digest: u64, claim: &RefillableCoverageClaim) -> u64 {
    for value in [
        claim.relative_range.start,
        claim.relative_range.end,
        claim.metric.bytes,
        claim.metric.utf16,
        u64::from(claim_owner_tag(claim.owner)),
        u64::from(coverage_part_tag(claim.part)),
        u64::from(claim_action_tag(claim.action)),
        u64::from(affinity_tag(claim.affinity)),
    ] {
        digest = fold_u64(digest, value);
    }
    digest
}

fn fold_u64(mut digest: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        digest ^= u64::from(byte);
        digest = digest.wrapping_mul(FNV_PRIME);
    }
    digest
}

const fn ending_tag(ending: Option<DirectLineEnding>) -> u8 {
    match ending {
        None => 0,
        Some(DirectLineEnding::Lf) => 1,
        Some(DirectLineEnding::Cr) => 2,
        Some(DirectLineEnding::CrLf) => 3,
    }
}

const fn line_kind_tag(kind: RefillableLineKind) -> u8 {
    match kind {
        RefillableLineKind::Paragraph => 1,
        RefillableLineKind::FencedCodeOpening(_) => 2,
        RefillableLineKind::FencedCodeLiteral => 3,
        RefillableLineKind::FencedCodeClosing => 4,
    }
}

const fn claim_owner_tag(owner: RefillableClaimOwner) -> u8 {
    match owner {
        RefillableClaimOwner::Document => 1,
        RefillableClaimOwner::Paragraph => 2,
        RefillableClaimOwner::FencedCode => 3,
    }
}

const fn coverage_part_tag(part: DirectCoveragePart) -> u8 {
    match part {
        DirectCoveragePart::Content => 1,
        DirectCoveragePart::ContainerMarker => 2,
        DirectCoveragePart::BlockMarker => 3,
        DirectCoveragePart::Gap => 4,
        DirectCoveragePart::Terminal => 5,
    }
}

const fn claim_action_tag(action: RefillableClaimAction) -> u8 {
    match action {
        RefillableClaimAction::Consume(DirectLogicalAction::Identity) => 1,
        RefillableClaimAction::Consume(DirectLogicalAction::CanonicalNewline) => 2,
        RefillableClaimAction::Consume(DirectLogicalAction::None) => 3,
        RefillableClaimAction::StageParagraphTerminator {
            ending: DirectLineEnding::Lf,
        } => 4,
        RefillableClaimAction::StageParagraphTerminator {
            ending: DirectLineEnding::Cr,
        } => 5,
        RefillableClaimAction::StageParagraphTerminator {
            ending: DirectLineEnding::CrLf,
        } => 6,
        RefillableClaimAction::Consume(DirectLogicalAction::HiddenUpstream) => 7,
        RefillableClaimAction::Consume(DirectLogicalAction::CanonicalText) => 8,
        RefillableClaimAction::Consume(DirectLogicalAction::PartialTab(_)) => 9,
    }
}

const fn affinity_tag(affinity: BoundaryAffinity) -> u8 {
    match affinity {
        BoundaryAffinity::Upstream => 1,
        BoundaryAffinity::Downstream => 2,
    }
}

const fn is_potential_block_start(byte: u8) -> bool {
    matches!(
        byte,
        b'#' | b'>' | b'-' | b'+' | b'*' | b'_' | b'=' | b'<' | b'[' | b'0'..=b'9'
    )
}

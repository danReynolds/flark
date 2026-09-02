//! Repeatable, allocation-free lexical scanning for one exact inline range.
//!
//! The production inline machine deliberately does not retain a lexical event
//! root. Every grammar phase creates a fresh scanner over the same immutable
//! source range. Event ordinals are assigned only when a complete logical
//! event is emitted, so they are independent of source-page, UTF-8, and poll
//! boundaries and remain stable across all passes.

use std::fmt;

use comrak::block_spine_facade;
use finl_unicode::categories::CharacterCategories;
use flark_engine::parser_internal::{
    M11ParserRangeCursor, M11ParserRangeError, M11ParserRangeStatus,
    M11_PARSER_RANGE_MAX_POLL_BYTES,
};

/// Largest cooperative scanner slice.
pub const M11_INLINE_LEX_MAX_POLL_TRANSITIONS: usize = M11_PARSER_RANGE_MAX_POLL_BYTES;

/// A lexical candidate requiring precedence-aware grammar resolution.
///
/// Candidate kinds are deliberately not `M11UnsupportedInlineKind`. A raw
/// `[`, `]`, `<`, or `&` is often literal, and an `@` or URL prefix can be
/// shielded by code. The inline job must establish a competing construct, or
/// deliberately apply its documented conservative profile fence, before
/// publishing a whole-leaf `Unsupported` disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11InlineLexHazardKind {
    LinkOrImageCandidate,
    HtmlCandidate,
    HardBreakCandidate,
    BareAutolinkCandidate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11InlineLexEventKind {
    /// A CommonMark backslash escape of one ASCII punctuation byte.
    BackslashEscape,
    /// A marker-backed physical line ending that CommonMark renders as a hard
    /// line break.
    ///
    /// `content_start` is the exact LF/CR/CRLF start within this event's
    /// source range. The event is held until the first scalar of the following
    /// content is available so the current narrow projection can fail closed
    /// when Comrak would discard continuation indentation that Flark cannot yet
    /// project exactly.
    HardLineBreak {
        content_start: u32,
        continuation_indented: bool,
    },
    /// One complete CommonMark character reference and its parser-cooked
    /// replacement. The exact source range remains on the enclosing event.
    ///
    /// Comrak's pinned entity table emits one or two Unicode scalars. Keeping
    /// those cooked values on the parser event lets downstream projection
    /// materialize text without recognizing entity spelling a second time.
    CharacterReference {
        first: char,
        second: Option<char>,
    },
    BacktickRun {
        len: u32,
        /// The first backtick is escaped in top-level text context.
        ///
        /// Code-span resolution may still use the complete raw run as a
        /// closer because backslashes have no escaping role inside code.
        escaped_prefix: bool,
    },
    EmphasisRun {
        marker: u8,
        len: u32,
        can_open: bool,
        can_close: bool,
    },
    /// One source-complete tilde run for the shared inline delimiter walk.
    ///
    /// The resolver admits only the run lengths selected by the pinned GFM
    /// profile. Keeping every run as an event preserves neighboring delimiter
    /// order without treating a literal long run as a whole-leaf hazard.
    StrikethroughRun {
        len: u32,
        can_open: bool,
        can_close: bool,
    },
    Hazard(M11InlineLexHazardKind),
}

/// One source-ordered candidate produced by every repeatable lexical pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11InlineLexEvent {
    ordinal: u32,
    start: u32,
    end: u32,
    kind: M11InlineLexEventKind,
}

impl M11InlineLexEvent {
    #[must_use]
    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }

    #[must_use]
    pub const fn start(self) -> u32 {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> u32 {
        self.end
    }

    #[must_use]
    pub const fn kind(self) -> M11InlineLexEventKind {
        self.kind
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11InlineLexReceipt {
    transitions: u64,
    source_bytes: u64,
    scalars: u64,
    events: u64,
    maximum_poll_transitions: usize,
}

impl M11InlineLexReceipt {
    #[must_use]
    pub const fn transitions(self) -> u64 {
        self.transitions
    }

    #[must_use]
    pub const fn source_bytes(self) -> u64 {
        self.source_bytes
    }

    #[must_use]
    pub const fn scalars(self) -> u64 {
        self.scalars
    }

    #[must_use]
    pub const fn events(self) -> u64 {
        self.events
    }

    #[must_use]
    pub const fn maximum_poll_transitions(self) -> usize {
        self.maximum_poll_transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11InlineLexPollStatus {
    Pending,
    Event(M11InlineLexEvent),
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11InlineLexPoll {
    status: M11InlineLexPollStatus,
    transitions: usize,
    source_bytes: usize,
}

impl M11InlineLexPoll {
    #[must_use]
    pub const fn status(self) -> M11InlineLexPollStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }

    #[must_use]
    pub const fn source_bytes(self) -> usize {
        self.source_bytes
    }
}

#[derive(Debug)]
pub enum M11InlineLexError {
    Source(M11ParserRangeError),
    ZeroFuel,
    PollLimitExceeded,
    InvalidUtf8,
    CoordinateOverflow,
    OrdinalExhausted,
    InvalidState,
}

impl fmt::Display for M11InlineLexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "inline source scan failed: {error}"),
            Self::ZeroFuel => formatter.write_str("inline lexical poll requires nonzero fuel"),
            Self::PollLimitExceeded => {
                formatter.write_str("inline lexical poll exceeds its bounded transition limit")
            }
            Self::InvalidUtf8 => {
                formatter.write_str("exact inline source range contains invalid UTF-8")
            }
            Self::CoordinateOverflow => {
                formatter.write_str("inline lexical coordinate exceeds the u32 schema")
            }
            Self::OrdinalExhausted => formatter.write_str("inline lexical event ordinal exhausted"),
            Self::InvalidState => formatter.write_str("inline lexical scanner state is invalid"),
        }
    }
}

impl std::error::Error for M11InlineLexError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            _ => None,
        }
    }
}

impl From<M11ParserRangeError> for M11InlineLexError {
    fn from(value: M11ParserRangeError) -> Self {
        Self::Source(value)
    }
}

#[derive(Clone, Copy, Debug)]
struct Scalar {
    value: char,
    start: u32,
    end: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunKind {
    Backtick,
    Emphasis(u8),
    Tilde,
}

#[derive(Clone, Copy, Debug)]
struct PendingRun {
    kind: RunKind,
    start: u32,
    end: u32,
    before: Option<char>,
    escaped_prefix: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntityPhase {
    Start,
    Named,
    NumericStart,
    Decimal,
    HexStart,
    Hex,
}

#[derive(Clone, Copy, Debug)]
struct PendingEntity {
    start: u32,
    len: usize,
    phase: EntityPhase,
}

const RECENT_SCALARS: usize = 8;
const ENTITY_CANDIDATE_BYTES: usize = 32;
const ENTITY_REPLACEMENT_BYTES: usize = 8;

enum Pump {
    Progress {
        transitions: usize,
        source_bytes: usize,
    },
    Eof,
}

/// Fixed-storage, repeatable scanner over one exact parser range cursor.
pub struct M11InlineLexScanner {
    cursor: M11ParserRangeCursor,
    window: [u8; M11_PARSER_RANGE_MAX_POLL_BYTES],
    window_position: usize,
    window_len: usize,
    source_eof: bool,
    scalar_eof: bool,
    utf8: [u8; 4],
    utf8_len: usize,
    utf8_expected: usize,
    utf8_start: u32,
    next_byte_offset: u32,
    current: Option<Scalar>,
    lookahead: Option<Scalar>,
    previous: Option<char>,
    previous_non_tilde: Option<char>,
    trailing_backslashes: u32,
    trailing_spaces: u32,
    trailing_space_start: u32,
    recent: [Option<Scalar>; RECENT_SCALARS],
    pending_bare_autolink: Option<(u32, u32)>,
    pending_hard_break: Option<(u32, u32, u32)>,
    pending_entity: Option<PendingEntity>,
    entity_candidate: [u8; ENTITY_CANDIDATE_BYTES],
    pending_run: Option<PendingRun>,
    deferred_run_before_tildes: Option<PendingRun>,
    next_event_ordinal: u32,
    receipt: M11InlineLexReceipt,
    complete: bool,
}

impl fmt::Debug for M11InlineLexScanner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11InlineLexScanner")
            .field("next_byte_offset", &self.next_byte_offset)
            .field("next_event_ordinal", &self.next_event_ordinal)
            .field("pending_run", &self.pending_run)
            .field("receipt", &self.receipt)
            .field("complete", &self.complete)
            .finish_non_exhaustive()
    }
}

impl M11InlineLexScanner {
    #[must_use]
    pub fn new(cursor: M11ParserRangeCursor) -> Self {
        Self {
            cursor,
            window: [0; M11_PARSER_RANGE_MAX_POLL_BYTES],
            window_position: 0,
            window_len: 0,
            source_eof: false,
            scalar_eof: false,
            utf8: [0; 4],
            utf8_len: 0,
            utf8_expected: 0,
            utf8_start: 0,
            next_byte_offset: 0,
            current: None,
            lookahead: None,
            previous: None,
            previous_non_tilde: None,
            trailing_backslashes: 0,
            trailing_spaces: 0,
            trailing_space_start: 0,
            recent: [None; RECENT_SCALARS],
            pending_bare_autolink: None,
            pending_hard_break: None,
            pending_entity: None,
            entity_candidate: [0; ENTITY_CANDIDATE_BYTES],
            pending_run: None,
            deferred_run_before_tildes: None,
            next_event_ordinal: 0,
            receipt: M11InlineLexReceipt::default(),
            complete: false,
        }
    }

    /// Advances by at most `fuel` source-copy, UTF-8, or lexical transitions.
    ///
    /// At most one complete event is returned per poll. Refills, scalar
    /// decoding, and event decisions are separate charged transitions; no
    /// source-sized loop is hidden behind one event.
    pub fn poll(&mut self, fuel: usize) -> Result<M11InlineLexPoll, M11InlineLexError> {
        if fuel == 0 {
            return Err(M11InlineLexError::ZeroFuel);
        }
        if fuel > M11_INLINE_LEX_MAX_POLL_TRANSITIONS {
            return Err(M11InlineLexError::PollLimitExceeded);
        }
        if self.complete {
            return Ok(M11InlineLexPoll {
                status: M11InlineLexPollStatus::Complete,
                transitions: 0,
                source_bytes: 0,
            });
        }

        let result = self.poll_inner(fuel);
        if result.is_err() {
            // Every error after argument validation is terminal for this
            // scanner. Release the owned range cursor before propagating it so
            // direct `?` use cannot leave a lifecycle obligation behind.
            self.cancel();
        }
        result
    }

    fn poll_inner(&mut self, fuel: usize) -> Result<M11InlineLexPoll, M11InlineLexError> {
        let mut transitions = 0;
        let mut source_bytes = 0;
        while transitions < fuel {
            if let Some((start, content_start, end)) = self.pending_hard_break {
                if let Some(current) = self.current {
                    self.pending_hard_break = None;
                    transitions += 1;
                    let event = self.emit(
                        start,
                        end,
                        M11InlineLexEventKind::HardLineBreak {
                            content_start,
                            continuation_indented: matches!(current.value, ' ' | '\t'),
                        },
                    )?;
                    return self.finish_poll(
                        M11InlineLexPollStatus::Event(event),
                        transitions,
                        source_bytes,
                    );
                }
                if self.scalar_eof {
                    // CommonMark does not produce a hard line break for a
                    // marker followed only by a terminal line ending.
                    self.pending_hard_break = None;
                }
            }

            if self.current.is_none() && self.scalar_eof {
                if self.pending_entity.take().is_some() {
                    // Missing-semicolon and truncated candidates are literal.
                    transitions += 1;
                    continue;
                }
                if let Some(run) = self.deferred_run_before_tildes.take() {
                    transitions += 1;
                    if let Some(event) = self.finish_run(run, None)? {
                        return self.finish_poll(
                            M11InlineLexPollStatus::Event(event),
                            transitions,
                            source_bytes,
                        );
                    }
                    continue;
                }
                if let Some(run) = self.pending_run.take() {
                    transitions += 1;
                    if let Some(event) = self.finish_run(run, None)? {
                        return self.finish_poll(
                            M11InlineLexPollStatus::Event(event),
                            transitions,
                            source_bytes,
                        );
                    }
                    continue;
                }
                self.complete = true;
                transitions += 1;
                return self.finish_poll(
                    M11InlineLexPollStatus::Complete,
                    transitions,
                    source_bytes,
                );
            }

            if self.current.is_none() || (self.lookahead.is_none() && !self.scalar_eof) {
                match self.pump_scalar(fuel - transitions)? {
                    Pump::Progress {
                        transitions: used,
                        source_bytes: read,
                    } => {
                        transitions += used;
                        source_bytes += read;
                    }
                    Pump::Eof => {
                        transitions += 1;
                    }
                }
                continue;
            }

            transitions += 1;
            if let Some(event) = self.process_current()? {
                return self.finish_poll(
                    M11InlineLexPollStatus::Event(event),
                    transitions,
                    source_bytes,
                );
            }
        }

        self.finish_poll(M11InlineLexPollStatus::Pending, transitions, source_bytes)
    }

    pub fn cancel(&mut self) {
        if !self.complete {
            self.cursor.cancel();
        }
        self.window_position = 0;
        self.window_len = 0;
        self.current = None;
        self.lookahead = None;
        self.pending_bare_autolink = None;
        self.pending_hard_break = None;
        self.pending_entity = None;
        self.pending_run = None;
        self.deferred_run_before_tildes = None;
        self.complete = true;
        self.source_eof = true;
        self.scalar_eof = true;
    }

    #[must_use]
    pub const fn receipt(&self) -> M11InlineLexReceipt {
        self.receipt
    }

    fn finish_poll(
        &mut self,
        status: M11InlineLexPollStatus,
        transitions: usize,
        source_bytes: usize,
    ) -> Result<M11InlineLexPoll, M11InlineLexError> {
        self.receipt.transitions = self
            .receipt
            .transitions
            .checked_add(
                u64::try_from(transitions).map_err(|_| M11InlineLexError::CoordinateOverflow)?,
            )
            .ok_or(M11InlineLexError::CoordinateOverflow)?;
        self.receipt.source_bytes = self
            .receipt
            .source_bytes
            .checked_add(
                u64::try_from(source_bytes).map_err(|_| M11InlineLexError::CoordinateOverflow)?,
            )
            .ok_or(M11InlineLexError::CoordinateOverflow)?;
        self.receipt.maximum_poll_transitions =
            self.receipt.maximum_poll_transitions.max(transitions);
        Ok(M11InlineLexPoll {
            status,
            transitions,
            source_bytes,
        })
    }

    fn pump_scalar(&mut self, fuel: usize) -> Result<Pump, M11InlineLexError> {
        if self.window_position == self.window_len {
            if self.source_eof {
                if self.utf8_len != 0 {
                    return Err(M11InlineLexError::InvalidUtf8);
                }
                self.scalar_eof = true;
                return Ok(Pump::Eof);
            }
            let poll = self.cursor.poll(fuel, &mut self.window)?;
            self.window_position = 0;
            self.window_len = poll.bytes_read();
            self.source_eof = poll.status() == M11ParserRangeStatus::Complete;
            if self.window_len == 0 {
                if self.utf8_len != 0 {
                    return Err(M11InlineLexError::InvalidUtf8);
                }
                self.scalar_eof = true;
                return Ok(Pump::Eof);
            }
            return Ok(Pump::Progress {
                transitions: poll.transitions(),
                source_bytes: poll.bytes_read(),
            });
        }

        let byte = self.window[self.window_position];
        self.window_position += 1;
        if self.utf8_len == 0 {
            self.utf8_start = self.next_byte_offset;
            self.utf8_expected = utf8_sequence_len(byte).ok_or(M11InlineLexError::InvalidUtf8)?;
        }
        if self.utf8_len >= self.utf8.len() {
            return Err(M11InlineLexError::InvalidUtf8);
        }
        self.utf8[self.utf8_len] = byte;
        self.utf8_len += 1;
        self.next_byte_offset = self
            .next_byte_offset
            .checked_add(1)
            .ok_or(M11InlineLexError::CoordinateOverflow)?;
        if self.utf8_len == self.utf8_expected {
            let source = std::str::from_utf8(&self.utf8[..self.utf8_len])
                .map_err(|_| M11InlineLexError::InvalidUtf8)?;
            let mut chars = source.chars();
            let value = chars.next().ok_or(M11InlineLexError::InvalidUtf8)?;
            if chars.next().is_some() {
                return Err(M11InlineLexError::InvalidUtf8);
            }
            let scalar = Scalar {
                value,
                start: self.utf8_start,
                end: self.next_byte_offset,
            };
            self.utf8_len = 0;
            self.utf8_expected = 0;
            if self.current.is_none() {
                self.current = Some(scalar);
            } else if self.lookahead.is_none() {
                self.lookahead = Some(scalar);
            } else {
                return Err(M11InlineLexError::InvalidState);
            }
            self.receipt.scalars = self
                .receipt
                .scalars
                .checked_add(1)
                .ok_or(M11InlineLexError::CoordinateOverflow)?;
        }
        Ok(Pump::Progress {
            transitions: 1,
            source_bytes: 0,
        })
    }

    fn process_current(&mut self) -> Result<Option<M11InlineLexEvent>, M11InlineLexError> {
        let current = self.current.ok_or(M11InlineLexError::InvalidState)?;
        if self.pending_entity.is_some() {
            return self.process_pending_entity(current);
        }
        if let Some(mut run) = self.pending_run {
            if run_accepts(run.kind, current.value) {
                run.end = current.end;
                self.pending_run = Some(run);
                self.take_current_without_context()?;
                return Ok(None);
            }
            if matches!(run.kind, RunKind::Emphasis(_))
                && current.value == '~'
                && self.deferred_run_before_tildes.is_none()
            {
                self.deferred_run_before_tildes = Some(run);
                self.pending_run = Some(PendingRun {
                    kind: RunKind::Tilde,
                    start: current.start,
                    end: current.end,
                    before: Some(run_marker(run.kind)),
                    escaped_prefix: false,
                });
                self.take_current_without_context()?;
                return Ok(None);
            }
            if run.kind == RunKind::Tilde {
                if let Some(deferred) = self.deferred_run_before_tildes.take() {
                    return self.finish_run(deferred, Some(current.value));
                }
            }
            self.pending_run = None;
            return self.finish_run(run, Some(current.value));
        }

        let next = self.lookahead.map(|scalar| scalar.value);
        match current.value {
            '\\' if next == Some('`') => {
                let escaped = self.lookahead.ok_or(M11InlineLexError::InvalidState)?;
                let start = current.start;
                // Keep the backtick in the lexical stream. At top level this
                // backslash escapes its first byte, but inside an already-open
                // code span the same raw run remains an eligible closer.
                self.consume_current()?;
                self.emit(start, escaped.end, M11InlineLexEventKind::BackslashEscape)
                    .map(Some)
            }
            '\\' if next.is_some_and(|value| value.is_ascii_punctuation()) => {
                let escaped = self.lookahead.ok_or(M11InlineLexError::InvalidState)?;
                let start = current.start;
                self.consume_current()?;
                self.consume_current()?;
                self.emit(start, escaped.end, M11InlineLexEventKind::BackslashEscape)
                    .map(Some)
            }
            '!' if next == Some('[') => {
                let bracket = self.lookahead.ok_or(M11InlineLexError::InvalidState)?;
                let start = current.start;
                self.consume_current()?;
                self.consume_current()?;
                self.emit(
                    start,
                    bracket.end,
                    M11InlineLexEventKind::Hazard(M11InlineLexHazardKind::LinkOrImageCandidate),
                )
                .map(Some)
            }
            '`' => {
                self.start_run(RunKind::Backtick, current)?;
                Ok(None)
            }
            marker @ ('*' | '_') => {
                self.start_run(RunKind::Emphasis(marker as u8), current)?;
                Ok(None)
            }
            '~' => {
                self.start_run(RunKind::Tilde, current)?;
                Ok(None)
            }
            '[' | ']' => {
                self.consume_current()?;
                self.emit(
                    current.start,
                    current.end,
                    M11InlineLexEventKind::Hazard(M11InlineLexHazardKind::LinkOrImageCandidate),
                )
                .map(Some)
            }
            '<' => {
                self.consume_current()?;
                self.emit(
                    current.start,
                    current.end,
                    M11InlineLexEventKind::Hazard(M11InlineLexHazardKind::HtmlCandidate),
                )
                .map(Some)
            }
            '&' => {
                self.consume_current()?;
                self.pending_entity = Some(PendingEntity {
                    start: current.start,
                    len: 0,
                    phase: EntityPhase::Start,
                });
                Ok(None)
            }
            '@' => {
                self.consume_current()?;
                self.emit(
                    current.start,
                    current.end,
                    M11InlineLexEventKind::Hazard(M11InlineLexHazardKind::BareAutolinkCandidate),
                )
                .map(Some)
            }
            '\r' | '\n' => self.process_line_ending(current),
            _ => {
                self.consume_current()?;
                if let Some((start, end)) = self.pending_bare_autolink.take() {
                    return self
                        .emit(
                            start,
                            end,
                            M11InlineLexEventKind::Hazard(
                                M11InlineLexHazardKind::BareAutolinkCandidate,
                            ),
                        )
                        .map(Some);
                }
                Ok(None)
            }
        }
    }

    fn process_pending_entity(
        &mut self,
        current: Scalar,
    ) -> Result<Option<M11InlineLexEvent>, M11InlineLexError> {
        let mut pending = self.pending_entity.ok_or(M11InlineLexError::InvalidState)?;
        let accept = match pending.phase {
            EntityPhase::Start => match current.value {
                '#' => Some(EntityPhase::NumericStart),
                value if value.is_ascii_alphanumeric() => Some(EntityPhase::Named),
                ';' => Some(EntityPhase::Start),
                _ => None,
            },
            EntityPhase::Named => match current.value {
                value if value.is_ascii_alphanumeric() => Some(EntityPhase::Named),
                ';' => Some(EntityPhase::Named),
                _ => None,
            },
            EntityPhase::NumericStart => match current.value {
                'x' | 'X' => Some(EntityPhase::HexStart),
                value if value.is_ascii_digit() => Some(EntityPhase::Decimal),
                ';' => Some(EntityPhase::NumericStart),
                _ => None,
            },
            EntityPhase::Decimal => match current.value {
                value if value.is_ascii_digit() => Some(EntityPhase::Decimal),
                ';' => Some(EntityPhase::Decimal),
                _ => None,
            },
            EntityPhase::HexStart => match current.value {
                value if value.is_ascii_hexdigit() => Some(EntityPhase::Hex),
                ';' => Some(EntityPhase::HexStart),
                _ => None,
            },
            EntityPhase::Hex => match current.value {
                value if value.is_ascii_hexdigit() => Some(EntityPhase::Hex),
                ';' => Some(EntityPhase::Hex),
                _ => None,
            },
        };
        let Some(next_phase) = accept else {
            // The leading ampersand and any accepted ASCII body are literal.
            // Leave the first non-candidate scalar for the normal grammar
            // walk, because it may itself be a delimiter or hazard.
            self.pending_entity = None;
            return Ok(None);
        };
        if pending.len == ENTITY_CANDIDATE_BYTES {
            self.pending_entity = None;
            return Ok(None);
        }
        self.entity_candidate[pending.len] =
            u8::try_from(current.value).map_err(|_| M11InlineLexError::InvalidState)?;
        pending.len += 1;
        pending.phase = next_phase;
        let end = current.end;
        self.consume_current()?;

        if current.value != ';' {
            self.pending_entity = (pending.len < ENTITY_CANDIDATE_BYTES).then_some(pending);
            return Ok(None);
        }
        self.pending_entity = None;
        let candidate = std::str::from_utf8(&self.entity_candidate[..pending.len])
            .map_err(|_| M11InlineLexError::InvalidState)?;
        let mut decoded = [0_u8; ENTITY_REPLACEMENT_BYTES];
        let Some(decoded_len) =
            block_spine_facade::decode_reference_entity(candidate, &mut decoded)
        else {
            return Ok(None);
        };
        let decoded = std::str::from_utf8(&decoded[..decoded_len])
            .map_err(|_| M11InlineLexError::InvalidState)?;
        let mut scalars = decoded.chars();
        let first = scalars.next().ok_or(M11InlineLexError::InvalidState)?;
        let second = scalars.next();
        if scalars.next().is_some() {
            return Err(M11InlineLexError::InvalidState);
        }
        self.emit(
            pending.start,
            end,
            M11InlineLexEventKind::CharacterReference { first, second },
        )
        .map(Some)
    }

    fn start_run(&mut self, kind: RunKind, current: Scalar) -> Result<(), M11InlineLexError> {
        let before = if matches!(kind, RunKind::Emphasis(_)) && self.previous == Some('~') {
            self.previous_non_tilde
        } else {
            self.previous
        };
        self.pending_run = Some(PendingRun {
            kind,
            start: current.start,
            end: current.end,
            before,
            escaped_prefix: kind == RunKind::Backtick && self.trailing_backslashes % 2 == 1,
        });
        self.take_current_without_context()
    }

    fn finish_run(
        &mut self,
        run: PendingRun,
        after: Option<char>,
    ) -> Result<Option<M11InlineLexEvent>, M11InlineLexError> {
        let len = run
            .end
            .checked_sub(run.start)
            .ok_or(M11InlineLexError::CoordinateOverflow)?;
        let marker = match run.kind {
            RunKind::Backtick => '`',
            RunKind::Emphasis(marker) => char::from(marker),
            RunKind::Tilde => '~',
        };
        self.previous = Some(marker);
        if marker != '~' {
            self.previous_non_tilde = Some(marker);
        }
        self.trailing_backslashes = 0;
        self.trailing_spaces = 0;
        let kind = match run.kind {
            RunKind::Backtick => M11InlineLexEventKind::BacktickRun {
                len,
                escaped_prefix: run.escaped_prefix,
            },
            RunKind::Emphasis(marker) => {
                let (can_open, can_close) = classify_emphasis(marker, run.before, after);
                M11InlineLexEventKind::EmphasisRun {
                    marker,
                    len,
                    can_open,
                    can_close,
                }
            }
            RunKind::Tilde => {
                let (can_open, can_close) = classify_emphasis(b'~', run.before, after);
                M11InlineLexEventKind::StrikethroughRun {
                    len,
                    can_open,
                    can_close,
                }
            }
        };
        self.emit(run.start, run.end, kind).map(Some)
    }

    fn process_line_ending(
        &mut self,
        current: Scalar,
    ) -> Result<Option<M11InlineLexEvent>, M11InlineLexError> {
        let marker_start = if self.trailing_backslashes % 2 == 1 {
            Some(
                current
                    .start
                    .checked_sub(1)
                    .ok_or(M11InlineLexError::CoordinateOverflow)?,
            )
        } else if self.trailing_spaces >= 2 {
            Some(self.trailing_space_start)
        } else {
            None
        };
        let mut end = current.end;
        if current.value == '\r' && self.lookahead.map(|scalar| scalar.value) == Some('\n') {
            end = self.lookahead.ok_or(M11InlineLexError::InvalidState)?.end;
            self.consume_current()?;
        }
        self.consume_current()?;
        if let Some(start) = marker_start {
            self.pending_hard_break = Some((start, current.start, end));
        }
        Ok(None)
    }

    fn emit(
        &mut self,
        start: u32,
        end: u32,
        kind: M11InlineLexEventKind,
    ) -> Result<M11InlineLexEvent, M11InlineLexError> {
        let ordinal = self.next_event_ordinal;
        self.next_event_ordinal = self
            .next_event_ordinal
            .checked_add(1)
            .ok_or(M11InlineLexError::OrdinalExhausted)?;
        self.receipt.events = self
            .receipt
            .events
            .checked_add(1)
            .ok_or(M11InlineLexError::OrdinalExhausted)?;
        Ok(M11InlineLexEvent {
            ordinal,
            start,
            end,
            kind,
        })
    }

    fn take_current_without_context(&mut self) -> Result<(), M11InlineLexError> {
        let scalar = self.current.take().ok_or(M11InlineLexError::InvalidState)?;
        self.record_scalar(scalar);
        self.current = self.lookahead.take();
        Ok(())
    }

    fn record_scalar(&mut self, scalar: Scalar) {
        self.recent.rotate_left(1);
        self.recent[RECENT_SCALARS - 1] = Some(scalar);
        for pattern in [b"http://".as_slice(), b"https://", b"ftp://", b"www."] {
            if let Some((start, end)) = recent_ascii_suffix(&self.recent, pattern) {
                self.pending_bare_autolink = Some((start, end));
                break;
            }
        }
    }

    fn consume_current(&mut self) -> Result<Scalar, M11InlineLexError> {
        let scalar = self.current.ok_or(M11InlineLexError::InvalidState)?;
        self.take_current_without_context()?;
        self.previous = Some(scalar.value);
        if scalar.value != '~' {
            self.previous_non_tilde = Some(scalar.value);
        }
        self.trailing_backslashes = if scalar.value == '\\' {
            self.trailing_backslashes
                .checked_add(1)
                .ok_or(M11InlineLexError::CoordinateOverflow)?
        } else {
            0
        };
        self.trailing_spaces = if scalar.value == ' ' {
            if self.trailing_spaces == 0 {
                self.trailing_space_start = scalar.start;
            }
            self.trailing_spaces
                .checked_add(1)
                .ok_or(M11InlineLexError::CoordinateOverflow)?
        } else {
            0
        };
        Ok(scalar)
    }
}

const fn utf8_sequence_len(first: u8) -> Option<usize> {
    match first {
        0x00..=0x7f => Some(1),
        0xc2..=0xdf => Some(2),
        0xe0..=0xef => Some(3),
        0xf0..=0xf4 => Some(4),
        _ => None,
    }
}

const fn run_accepts(kind: RunKind, value: char) -> bool {
    match kind {
        RunKind::Backtick => value == '`',
        RunKind::Emphasis(marker) => value as u32 == marker as u32,
        RunKind::Tilde => value == '~',
    }
}

const fn run_marker(kind: RunKind) -> char {
    match kind {
        RunKind::Backtick => '`',
        RunKind::Emphasis(marker) => marker as char,
        RunKind::Tilde => '~',
    }
}

fn classify_emphasis(marker: u8, before: Option<char>, after: Option<char>) -> (bool, bool) {
    let before_whitespace = before.is_none_or(char::is_whitespace);
    let after_whitespace = after.is_none_or(char::is_whitespace);
    let before_punctuation = before.is_some_and(m11_is_markdown_punctuation);
    let after_punctuation = after.is_some_and(m11_is_markdown_punctuation);
    let left_flanking =
        !after_whitespace && (!after_punctuation || before_whitespace || before_punctuation);
    let right_flanking =
        !before_whitespace && (!before_punctuation || after_whitespace || after_punctuation);
    let can_open = left_flanking && (marker != b'_' || !right_flanking || before_punctuation);
    let can_close = right_flanking && (marker != b'_' || !left_flanking || after_punctuation);
    (can_open, can_close)
}

/// CommonMark punctuation classification shared with consumers that must
/// conservatively preserve an already parser-authenticated delimiter run.
pub fn m11_is_markdown_punctuation(value: char) -> bool {
    value.is_punctuation() || value.is_symbol()
}

fn recent_ascii_suffix(
    recent: &[Option<Scalar>; RECENT_SCALARS],
    pattern: &[u8],
) -> Option<(u32, u32)> {
    let start = RECENT_SCALARS.checked_sub(pattern.len())?;
    let scalars = &recent[start..];
    for (scalar, expected) in scalars.iter().zip(pattern) {
        let value = scalar.as_ref()?.value;
        if !value.is_ascii() || !u8::try_from(value).ok()?.eq_ignore_ascii_case(expected) {
            return None;
        }
    }
    Some((
        scalars.first()?.as_ref()?.start,
        scalars.last()?.as_ref()?.end,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flark_engine::parser_internal::M11ParserSourceRangeAuthority;
    use flark_engine::{DocumentRuntime, DocumentRuntimeConfig};

    fn source_authority(
        runtime: &DocumentRuntime,
        range: std::ops::Range<usize>,
    ) -> M11ParserSourceRangeAuthority {
        M11ParserSourceRangeAuthority::new(
            runtime,
            runtime.snapshot_current_source().expect("source"),
            range,
        )
        .expect("source authority")
    }

    fn scan(
        runtime: &DocumentRuntime,
        authority: &M11ParserSourceRangeAuthority,
        fuel: usize,
    ) -> Vec<M11InlineLexEvent> {
        let cursor = authority.cursor(runtime).expect("source cursor");
        let mut scanner = M11InlineLexScanner::new(cursor);
        let mut events = Vec::new();
        loop {
            let poll = scanner.poll(fuel).expect("lexical poll");
            assert!(poll.transitions() <= fuel);
            match poll.status() {
                M11InlineLexPollStatus::Pending => {}
                M11InlineLexPollStatus::Event(event) => events.push(event),
                M11InlineLexPollStatus::Complete => break,
            }
        }
        drop(scanner);
        events
    }

    fn close_runtime(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin close");
        while !runtime.poll_close(64).expect("close").complete {}
    }

    #[test]
    fn events_and_ordinals_are_independent_of_utf8_refill_and_fuel_boundaries() {
        let source = format!(
            "{}é_*β*_ `γ` \\\\* [x] & < ~~ name@example.test  \r\n",
            "x".repeat(M11_PARSER_RANGE_MAX_POLL_BYTES - 1)
        );
        let runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let authority = source_authority(&runtime, 0..source.len());

        let expected = scan(&runtime, &authority, M11_INLINE_LEX_MAX_POLL_TRANSITIONS);
        for fuel in [1, 2, 7, 31, 257] {
            assert_eq!(scan(&runtime, &authority, fuel), expected, "fuel={fuel}");
        }
        for (ordinal, event) in expected.iter().copied().enumerate() {
            assert_eq!(event.ordinal(), u32::try_from(ordinal).expect("ordinal"));
        }

        drop(authority);
        close_runtime(runtime);
    }

    #[test]
    fn unicode_punctuation_and_symbols_participate_in_commonmark_flanking() {
        let source = "α_β_γ α—_β_ α★_β_ α *β*";
        let runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let authority = source_authority(&runtime, 0..source.len());
        let events = scan(&runtime, &authority, 3);
        let emphasis: Vec<_> = events
            .into_iter()
            .filter_map(|event| match event.kind() {
                M11InlineLexEventKind::EmphasisRun {
                    marker,
                    can_open,
                    can_close,
                    ..
                } => Some((marker, can_open, can_close)),
                _ => None,
            })
            .collect();
        assert_eq!(
            emphasis,
            vec![
                (b'_', false, false),
                (b'_', false, false),
                (b'_', true, false),
                (b'_', false, true),
                (b'_', true, false),
                (b'_', false, true),
                (b'*', true, false),
                (b'*', false, true),
            ]
        );

        drop(authority);
        close_runtime(runtime);
    }

    #[test]
    fn tilde_runs_are_real_delimiters_and_neighboring_emphasis_skips_them() {
        let source = "a~*?x*";
        let runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let authority = source_authority(&runtime, 0..source.len());
        let events = scan(&runtime, &authority, 1);
        assert_eq!(
            events
                .iter()
                .map(|event| (event.ordinal(), event.start(), event.end(), event.kind()))
                .collect::<Vec<_>>(),
            vec![
                (
                    0,
                    1,
                    2,
                    M11InlineLexEventKind::StrikethroughRun {
                        len: 1,
                        can_open: false,
                        can_close: true,
                    },
                ),
                (
                    1,
                    2,
                    3,
                    M11InlineLexEventKind::EmphasisRun {
                        marker: b'*',
                        len: 1,
                        can_open: false,
                        can_close: true,
                    },
                ),
                (
                    2,
                    5,
                    6,
                    M11InlineLexEventKind::EmphasisRun {
                        marker: b'*',
                        len: 1,
                        can_open: false,
                        can_close: true,
                    },
                ),
            ]
        );

        drop(authority);
        close_runtime(runtime);
    }

    #[test]
    fn escapes_and_competing_candidates_emit_one_stable_hazard_each() {
        let source = "\\*literal* ![x] [y] &amp; <i> ~~ z@x  \nb";
        let runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let authority = source_authority(&runtime, 0..source.len());
        let events = scan(&runtime, &authority, 5);
        let actual: Vec<_> = events
            .iter()
            .map(|event| (event.ordinal(), event.start(), event.end(), event.kind()))
            .collect();
        assert_eq!(
            actual,
            vec![
                (0, 0, 2, M11InlineLexEventKind::BackslashEscape,),
                (
                    1,
                    9,
                    10,
                    M11InlineLexEventKind::EmphasisRun {
                        marker: b'*',
                        len: 1,
                        can_open: false,
                        can_close: true,
                    },
                ),
                (
                    2,
                    11,
                    13,
                    M11InlineLexEventKind::Hazard(M11InlineLexHazardKind::LinkOrImageCandidate),
                ),
                (
                    3,
                    14,
                    15,
                    M11InlineLexEventKind::Hazard(M11InlineLexHazardKind::LinkOrImageCandidate),
                ),
                (
                    4,
                    16,
                    17,
                    M11InlineLexEventKind::Hazard(M11InlineLexHazardKind::LinkOrImageCandidate),
                ),
                (
                    5,
                    18,
                    19,
                    M11InlineLexEventKind::Hazard(M11InlineLexHazardKind::LinkOrImageCandidate),
                ),
                (
                    6,
                    20,
                    25,
                    M11InlineLexEventKind::CharacterReference {
                        first: '&',
                        second: None,
                    },
                ),
                (
                    7,
                    26,
                    27,
                    M11InlineLexEventKind::Hazard(M11InlineLexHazardKind::HtmlCandidate),
                ),
                (
                    8,
                    30,
                    32,
                    M11InlineLexEventKind::StrikethroughRun {
                        len: 2,
                        can_open: false,
                        can_close: false,
                    },
                ),
                (
                    9,
                    34,
                    35,
                    M11InlineLexEventKind::Hazard(M11InlineLexHazardKind::BareAutolinkCandidate),
                ),
                (
                    10,
                    36,
                    39,
                    M11InlineLexEventKind::HardLineBreak {
                        content_start: 38,
                        continuation_indented: false,
                    },
                ),
            ]
        );

        drop(authority);
        close_runtime(runtime);
    }

    #[test]
    fn character_references_use_the_pinned_comrak_decoder_and_are_fuel_invariant() {
        let cases = [
            (
                "&amp;",
                Some(M11InlineLexEventKind::CharacterReference {
                    first: '&',
                    second: None,
                }),
            ),
            (
                "&#35;",
                Some(M11InlineLexEventKind::CharacterReference {
                    first: '#',
                    second: None,
                }),
            ),
            (
                "&#x1F600;",
                Some(M11InlineLexEventKind::CharacterReference {
                    first: '😀',
                    second: None,
                }),
            ),
            (
                "&ngE;",
                Some(M11InlineLexEventKind::CharacterReference {
                    first: '≧',
                    second: Some('\u{338}'),
                }),
            ),
            (
                "&#0;",
                Some(M11InlineLexEventKind::CharacterReference {
                    first: '\u{FFFD}',
                    second: None,
                }),
            ),
            // Preserve the pinned Comrak 0.54 donor exactly. Its current
            // scalar guard includes U+E000 in the replacement range.
            (
                "&#xE000;",
                Some(M11InlineLexEventKind::CharacterReference {
                    first: '\u{FFFD}',
                    second: None,
                }),
            ),
            ("&nbsp", None),
            ("&x;", None),
            ("&#;", None),
            ("&#x;", None),
            ("&#12345678;", None),
            ("&#x0000041;", None),
        ];
        for (source, expected_kind) in cases {
            let runtime =
                DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
            let authority = source_authority(&runtime, 0..source.len());
            let expected = expected_kind.map_or_else(Vec::new, |kind| {
                vec![M11InlineLexEvent {
                    ordinal: 0,
                    start: 0,
                    end: u32::try_from(source.len()).expect("source length"),
                    kind,
                }]
            });
            for fuel in [1, 2, 7, 31, M11_INLINE_LEX_MAX_POLL_TRANSITIONS] {
                assert_eq!(
                    scan(&runtime, &authority, fuel),
                    expected,
                    "source={source:?}, fuel={fuel}"
                );
            }
            drop(authority);
            close_runtime(runtime);
        }
    }

    #[test]
    fn invalid_entity_body_leaves_grammar_punctuation_for_the_normal_walk() {
        let source = "&amp*;";
        let runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let authority = source_authority(&runtime, 0..source.len());
        let events = scan(&runtime, &authority, 1);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].start(), 4);
        assert_eq!(events[0].end(), 5);
        assert!(matches!(
            events[0].kind(),
            M11InlineLexEventKind::EmphasisRun { marker: b'*', .. }
        ));

        drop(authority);
        close_runtime(runtime);
    }

    #[test]
    fn hard_break_candidates_have_exact_marker_ranges_and_backslash_parity() {
        for ending in ["\n", "\r", "\r\n"] {
            type ExpectedEvent = (u32, u32, u32, M11InlineLexEventKind);

            let ending_len = u32::try_from(ending.len()).expect("line ending length");
            let cases: [(&str, Vec<ExpectedEvent>); 7] = [
                ("", vec![]),
                (" ", vec![]),
                (
                    "  ",
                    vec![(
                        0,
                        0,
                        2 + ending_len,
                        M11InlineLexEventKind::HardLineBreak {
                            content_start: 2,
                            continuation_indented: false,
                        },
                    )],
                ),
                (
                    "   ",
                    vec![(
                        0,
                        0,
                        3 + ending_len,
                        M11InlineLexEventKind::HardLineBreak {
                            content_start: 3,
                            continuation_indented: false,
                        },
                    )],
                ),
                (
                    "\\",
                    vec![(
                        0,
                        0,
                        1 + ending_len,
                        M11InlineLexEventKind::HardLineBreak {
                            content_start: 1,
                            continuation_indented: false,
                        },
                    )],
                ),
                (
                    "\\\\",
                    vec![(0, 0, 2, M11InlineLexEventKind::BackslashEscape)],
                ),
                (
                    "\\\\\\",
                    vec![
                        (0, 0, 2, M11InlineLexEventKind::BackslashEscape),
                        (
                            1,
                            2,
                            3 + ending_len,
                            M11InlineLexEventKind::HardLineBreak {
                                content_start: 3,
                                continuation_indented: false,
                            },
                        ),
                    ],
                ),
            ];

            for (prefix, expected) in cases {
                let source = format!("{prefix}{ending}x");
                let runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
                    .expect("runtime");
                let authority = source_authority(&runtime, 0..source.len());
                for fuel in [1, 2, 7, 31] {
                    let actual = scan(&runtime, &authority, fuel)
                        .iter()
                        .map(|event| (event.ordinal(), event.start(), event.end(), event.kind()))
                        .collect::<Vec<_>>();
                    assert_eq!(
                        actual, expected,
                        "prefix={prefix:?}, ending={ending:?}, fuel={fuel}"
                    );
                }
                drop(authority);
                close_runtime(runtime);
            }
        }
    }

    #[test]
    fn hard_break_candidates_stamp_indented_continuations_for_fail_closed_resolution() {
        for ending in ["\n", "\r", "\r\n"] {
            for indent in [" ", "  ", "\t", "\t "] {
                let source = format!("x\\{ending}{indent}y");
                let content_start = 2_u32;
                let ending_extra = u32::try_from(ending.len() - 1).expect("ending width");
                let expected = M11InlineLexEventKind::HardLineBreak {
                    content_start,
                    continuation_indented: true,
                };
                let runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
                    .expect("runtime");
                let authority = source_authority(&runtime, 0..source.len());
                for fuel in [1, 2, 7, 31] {
                    let hard_break = scan(&runtime, &authority, fuel)
                        .into_iter()
                        .find(|event| {
                            matches!(event.kind(), M11InlineLexEventKind::HardLineBreak { .. })
                        })
                        .expect("hard-break event");
                    assert_eq!(
                        hard_break.start(),
                        1,
                        "ending={ending:?}, indent={indent:?}"
                    );
                    assert_eq!(
                        hard_break.end(),
                        3 + ending_extra,
                        "ending={ending:?}, indent={indent:?}"
                    );
                    assert_eq!(
                        hard_break.kind(),
                        expected,
                        "ending={ending:?}, indent={indent:?}, fuel={fuel}"
                    );
                }
                drop(authority);
                close_runtime(runtime);
            }
        }
    }

    #[test]
    fn terminal_line_ending_does_not_create_a_hard_break_candidate() {
        for source in ["\\\n", "\\\r", "\\\r\n", "  \n", "  \r", "  \r\n"] {
            let runtime =
                DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
            let authority = source_authority(&runtime, 0..source.len());
            for fuel in [1, 2, 7] {
                assert!(
                    scan(&runtime, &authority, fuel).iter().all(|event| {
                        !matches!(event.kind(), M11InlineLexEventKind::HardLineBreak { .. })
                    }),
                    "source={source:?}, fuel={fuel}"
                );
            }
            drop(authority);
            close_runtime(runtime);
        }
    }

    #[test]
    fn all_existing_bare_autolink_prefixes_are_case_insensitive_and_code_shieldable() {
        let source = "`HTTP://inside` https://outside FTP://x Www.example";
        let runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let authority = source_authority(&runtime, 0..source.len());
        let events = scan(&runtime, &authority, 7);
        assert_eq!(
            events
                .iter()
                .map(|event| (event.start(), event.end(), event.kind()))
                .collect::<Vec<_>>(),
            vec![
                (
                    0,
                    1,
                    M11InlineLexEventKind::BacktickRun {
                        len: 1,
                        escaped_prefix: false,
                    },
                ),
                (
                    1,
                    8,
                    M11InlineLexEventKind::Hazard(M11InlineLexHazardKind::BareAutolinkCandidate),
                ),
                (
                    14,
                    15,
                    M11InlineLexEventKind::BacktickRun {
                        len: 1,
                        escaped_prefix: false,
                    },
                ),
                (
                    16,
                    24,
                    M11InlineLexEventKind::Hazard(M11InlineLexHazardKind::BareAutolinkCandidate),
                ),
                (
                    32,
                    38,
                    M11InlineLexEventKind::Hazard(M11InlineLexHazardKind::BareAutolinkCandidate),
                ),
                (
                    40,
                    44,
                    M11InlineLexEventKind::Hazard(M11InlineLexHazardKind::BareAutolinkCandidate),
                ),
            ]
        );
        // The first URL candidate intentionally remains in the lexical stream.
        // Code-span resolution, not this scanner, must shield it before a
        // candidate becomes a whole-leaf unsupported disposition.

        drop(authority);
        close_runtime(runtime);
    }

    #[test]
    fn middle_source_range_is_scalar_aligned_and_uses_range_relative_coordinates() {
        let prefix = "OUTé";
        let visible = "α *β*";
        let source = format!("{prefix}{visible}終OUT");
        let range = prefix.len()..prefix.len() + visible.len();
        let runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let authority = source_authority(&runtime, range);
        let events = scan(&runtime, &authority, 2);
        assert_eq!(
            events
                .iter()
                .map(|event| (event.ordinal(), event.start(), event.end(), event.kind()))
                .collect::<Vec<_>>(),
            vec![
                (
                    0,
                    3,
                    4,
                    M11InlineLexEventKind::EmphasisRun {
                        marker: b'*',
                        len: 1,
                        can_open: true,
                        can_close: false,
                    },
                ),
                (
                    1,
                    6,
                    7,
                    M11InlineLexEventKind::EmphasisRun {
                        marker: b'*',
                        len: 1,
                        can_open: false,
                        can_close: true,
                    },
                ),
            ]
        );

        drop(authority);
        close_runtime(runtime);
    }

    #[test]
    fn cancellation_closes_the_owned_source_cursor() {
        let source = "α".repeat(10_000);
        let runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let authority = source_authority(&runtime, 0..source.len());
        let mut scanner =
            M11InlineLexScanner::new(authority.cursor(&runtime).expect("source cursor"));
        assert_eq!(
            scanner.poll(1).expect("first poll").status(),
            M11InlineLexPollStatus::Pending
        );
        scanner.cancel();
        assert_eq!(
            scanner.poll(1).expect("cancelled poll").status(),
            M11InlineLexPollStatus::Complete
        );
        drop(scanner);

        drop(authority);
        close_runtime(runtime);
    }

    #[test]
    fn terminal_poll_error_cancels_the_owned_source_cursor_before_propagation() {
        let source = "x";
        let runtime =
            DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
        let authority = source_authority(&runtime, 0..source.len());
        let mut scanner =
            M11InlineLexScanner::new(authority.cursor(&runtime).expect("source cursor"));

        // Inject a corrupt byte window to exercise the terminal error path;
        // public source ranges are guaranteed scalar-aligned and valid UTF-8.
        scanner.window[0] = 0xff;
        scanner.window_len = 1;
        assert!(matches!(
            scanner.poll(1),
            Err(M11InlineLexError::InvalidUtf8)
        ));
        assert_eq!(
            scanner.poll(1).expect("failed scanner is closed").status(),
            M11InlineLexPollStatus::Complete
        );
        // Dropping here proves the error path already discharged the cursor.
        drop(scanner);

        drop(authority);
        close_runtime(runtime);
    }
}

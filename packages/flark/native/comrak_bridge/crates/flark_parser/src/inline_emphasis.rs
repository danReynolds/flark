//! Resumable CommonMark/GFM paired-inline delimiter resolution.
//!
//! This stage consumes the move-only, source-ordered opaque stream, repeats
//! the exact lexical pass, and resolves `*`, `_`, and the pinned GFM `~`
//! delimiter ownership in one shared stack. Delimiters wholly inside resolved
//! code or angle-autolink ranges are excluded before the walk runs. The output
//! remains candidate-only until the end-to-end projection job promotes it.

use std::fmt;
use std::ops::Range;

use flark_engine::parser_internal::{M11ParserRangeCursor, M11ParserSourceRangeAuthority};
use flark_engine::{DocumentRuntime, SourceVersion};

use crate::inline_autolink::{
    M11InlineAutolinkError, M11InlineOpaqueCandidate, M11InlineOpaqueCandidates,
};
use crate::inline_direct::{M11InlineDirectCandidates, M11InlineDirectError};
use crate::inline_lex::{
    M11InlineLexError, M11InlineLexEvent, M11InlineLexEventKind, M11InlineLexPollStatus,
    M11InlineLexScanner,
};
use crate::inline_radix::{
    M11InlineRadixError, M11InlineRadixPages, M11InlineRadixReclaimPoll,
    M11_INLINE_RADIX_MAX_POLL_TRANSITIONS,
};

pub(crate) const M11_INLINE_EMPHASIS_MAX_POLL_TRANSITIONS: usize =
    M11_INLINE_RADIX_MAX_POLL_TRANSITIONS;

const DELIMITER_PAGE_RECORDS: usize = 128;
const CANDIDATE_PAGE_RECORDS: usize = 128;
const REMAINDER_PAGE_RECORDS: usize = 256;

const DELIMITER_CAN_OPEN: u8 = 1;
const DELIMITER_CAN_CLOSE: u8 = 2;
const DELIMITER_ACTIVE: u8 = 4;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Delimiter {
    start: u32,
    len: u32,
    /// Source run length used by CommonMark's mod-three rule.
    ///
    /// The donor retains this value after consuming part of a run; [len]
    /// tracks only the remaining marker bytes available for a later match.
    original_len: u32,
    previous_plus_one: u32,
    next_plus_one: u32,
    candidate_head_plus_one: u32,
    consumed: u32,
    marker: u8,
    flags: u8,
}

impl Delimiter {
    const fn can_open(self) -> bool {
        self.flags & DELIMITER_CAN_OPEN != 0
    }

    const fn can_close(self) -> bool {
        self.flags & DELIMITER_CAN_CLOSE != 0
    }

    const fn active(self) -> bool {
        self.flags & DELIMITER_ACTIVE != 0
    }

    const fn previous(self) -> Option<u32> {
        self.previous_plus_one.checked_sub(1)
    }

    const fn next(self) -> Option<u32> {
        self.next_plus_one.checked_sub(1)
    }

    fn end(self) -> Result<u32, M11InlineEmphasisError> {
        self.start
            .checked_add(self.len)
            .ok_or(M11InlineEmphasisError::CoordinateOverflow)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum M11EmphasisCandidateKind {
    #[default]
    Emphasis,
    Strong,
    Strikethrough,
}

/// Exact marker ownership established by the CommonMark delimiter walk.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct M11EmphasisCandidate {
    range_start: u32,
    range_end: u32,
    content_start: u32,
    content_end: u32,
    marker: u8,
    kind: M11EmphasisCandidateKind,
    reserved: u16,
    next_same_opener_plus_one: u32,
}

impl M11EmphasisCandidate {
    pub(crate) const fn kind(self) -> M11EmphasisCandidateKind {
        self.kind
    }

    pub(crate) const fn marker(self) -> u8 {
        self.marker
    }

    pub(crate) fn relative_range(self) -> Range<u32> {
        self.range_start..self.range_end
    }

    pub(crate) fn relative_opener_range(self) -> Range<u32> {
        self.range_start..self.content_start
    }

    pub(crate) fn relative_content_range(self) -> Range<u32> {
        self.content_start..self.content_end
    }

    pub(crate) fn relative_closer_range(self) -> Range<u32> {
        self.content_end..self.range_end
    }

    pub(crate) const fn next_same_opener(self) -> Option<u32> {
        self.next_same_opener_plus_one.checked_sub(1)
    }
}

/// Unconsumed bytes in a delimiter run that contributed to a match.
///
/// These are not automatically unsupported. The later whole-inline validator
/// owns the fail-closed policy, including the learned ambiguous-remainder
/// boundary (`**wow*`, `*wow**`, and related partial runs).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct M11EmphasisRemainderCandidate {
    start: u32,
    end: u32,
    marker: u8,
    reserved: [u8; 3],
}

impl M11EmphasisRemainderCandidate {
    pub(crate) fn relative_range(self) -> Range<u32> {
        self.start..self.end
    }

    pub(crate) const fn marker(self) -> u8 {
        self.marker
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineEmphasisPollStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineEmphasisPoll {
    status: M11InlineEmphasisPollStatus,
    transitions: usize,
}

impl M11InlineEmphasisPoll {
    pub(crate) const fn status(self) -> M11InlineEmphasisPollStatus {
        self.status
    }

    pub(crate) const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineEmphasisReleasePoll {
    transitions: usize,
    complete: bool,
}

impl M11InlineEmphasisReleasePoll {
    pub(crate) const fn transitions(self) -> usize {
        self.transitions
    }

    pub(crate) const fn complete(self) -> bool {
        self.complete
    }
}

#[derive(Debug)]
pub(crate) enum M11InlineEmphasisError {
    Opaque(M11InlineAutolinkError),
    Direct(M11InlineDirectError),
    Lex(M11InlineLexError),
    Scratch(M11InlineRadixError),
    ZeroFuel,
    PollLimitExceeded,
    CoordinateOverflow,
    InvalidState,
}

impl fmt::Display for M11InlineEmphasisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Opaque(error) => write!(formatter, "inline opaque baton failed: {error}"),
            Self::Direct(error) => write!(formatter, "inline direct baton failed: {error}"),
            Self::Lex(error) => write!(formatter, "inline emphasis scan failed: {error}"),
            Self::Scratch(error) => write!(formatter, "inline emphasis scratch failed: {error}"),
            Self::ZeroFuel => formatter.write_str("inline emphasis poll requires nonzero fuel"),
            Self::PollLimitExceeded => {
                formatter.write_str("inline emphasis poll exceeds its transition limit")
            }
            Self::CoordinateOverflow => {
                formatter.write_str("inline emphasis coordinate or counter overflow")
            }
            Self::InvalidState => formatter.write_str("inline emphasis job is in an invalid state"),
        }
    }
}

impl std::error::Error for M11InlineEmphasisError {}

impl From<M11InlineAutolinkError> for M11InlineEmphasisError {
    fn from(value: M11InlineAutolinkError) -> Self {
        Self::Opaque(value)
    }
}

impl From<M11InlineDirectError> for M11InlineEmphasisError {
    fn from(value: M11InlineDirectError) -> Self {
        Self::Direct(value)
    }
}

impl From<M11InlineLexError> for M11InlineEmphasisError {
    fn from(value: M11InlineLexError) -> Self {
        Self::Lex(value)
    }
}

impl From<M11InlineRadixError> for M11InlineEmphasisError {
    fn from(value: M11InlineRadixError) -> Self {
        Self::Scratch(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EmphasisPhase {
    Initializing,
    Scanning,
    Processing,
    Remainders,
    Complete,
    Faulted,
    Aborting,
    Aborted,
    Transferred,
}

#[derive(Clone, Copy, Debug)]
struct Search {
    closer: u32,
    opener: Option<u32>,
    category: usize,
    mod_three_rule_invoked: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MatchPhase {
    WriteCandidate,
    RemoveInterior,
    ConsumeOpener,
    ConsumeCloser,
}

#[derive(Clone, Copy, Debug)]
struct Match {
    opener: u32,
    closer: u32,
    interior: Option<u32>,
    use_delimiters: u32,
    phase: MatchPhase,
}

/// One exact-source emphasis candidate derivation.
///
/// Construction is infallible and does not open another source lease. The
/// first poll validates and borrows the move-only authority baton carried by
/// the code candidate output. Every terminal poll fault leaves all admitted
/// state owned by this job for explicit fuelled abort.
pub(crate) struct M11InlineEmphasisJob {
    source: SourceVersion,
    source_range: Range<u32>,
    opaque: Option<M11InlineOpaqueCandidates>,
    scanner: Option<M11InlineLexScanner>,
    delimiters: Option<M11InlineRadixPages<Delimiter, DELIMITER_PAGE_RECORDS>>,
    candidates: Option<M11InlineRadixPages<M11EmphasisCandidate, CANDIDATE_PAGE_RECORDS>>,
    remainders: Option<M11InlineRadixPages<M11EmphasisRemainderCandidate, REMAINDER_PAGE_RECORDS>>,
    delimiter_reclaim_started: bool,
    candidate_reclaim_started: bool,
    remainder_reclaim_started: bool,
    code_release_started: bool,
    pending_event: Option<M11InlineLexEvent>,
    opaque_index: u32,
    direct_syntax: Vec<Range<u32>>,
    direct_index: usize,
    delimiter_count: u32,
    candidate_count: u32,
    remainder_count: u32,
    first_delimiter: Option<u32>,
    last_delimiter: Option<u32>,
    closer: Option<u32>,
    search: Option<Search>,
    pending_match: Option<Match>,
    openers_bottom: [u32; 8],
    remainder_index: u32,
    phase: EmphasisPhase,
}

impl fmt::Debug for M11InlineEmphasisJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11InlineEmphasisJob")
            .field("source", &self.source)
            .field("source_range", &self.source_range)
            .field("delimiter_count", &self.delimiter_count)
            .field("candidate_count", &self.candidate_count)
            .field("remainder_count", &self.remainder_count)
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

impl M11InlineEmphasisJob {
    pub(crate) fn new(opaque: M11InlineOpaqueCandidates) -> Result<Self, M11InlineEmphasisError> {
        Self::new_with_syntax(opaque, Vec::new())
    }

    pub(crate) fn new_with_direct(
        opaque: M11InlineOpaqueCandidates,
        direct: &M11InlineDirectCandidates,
    ) -> Result<Self, M11InlineEmphasisError> {
        if opaque.source() != direct.source() || opaque.source_range() != direct.source_range() {
            return Err(M11InlineEmphasisError::InvalidState);
        }
        Self::new_with_syntax(opaque, direct.syntax_ranges().collect())
    }

    fn new_with_syntax(
        opaque: M11InlineOpaqueCandidates,
        direct_syntax: Vec<Range<u32>>,
    ) -> Result<Self, M11InlineEmphasisError> {
        let source = opaque.source();
        let source_range = opaque.source_range();
        Ok(Self {
            source,
            source_range,
            opaque: Some(opaque),
            scanner: None,
            delimiters: None,
            candidates: None,
            remainders: None,
            delimiter_reclaim_started: false,
            candidate_reclaim_started: false,
            remainder_reclaim_started: false,
            code_release_started: false,
            pending_event: None,
            opaque_index: 0,
            direct_syntax,
            direct_index: 0,
            delimiter_count: 0,
            candidate_count: 0,
            remainder_count: 0,
            first_delimiter: None,
            last_delimiter: None,
            closer: None,
            search: None,
            pending_match: None,
            openers_bottom: [0; 8],
            remainder_index: 0,
            phase: EmphasisPhase::Initializing,
        })
    }

    pub(crate) fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineEmphasisPoll, M11InlineEmphasisError> {
        validate_fuel(fuel)?;
        if matches!(
            self.phase,
            EmphasisPhase::Faulted
                | EmphasisPhase::Aborting
                | EmphasisPhase::Aborted
                | EmphasisPhase::Transferred
        ) {
            return Err(M11InlineEmphasisError::InvalidState);
        }
        if self.phase == EmphasisPhase::Complete {
            return Ok(M11InlineEmphasisPoll {
                status: M11InlineEmphasisPollStatus::Complete,
                transitions: 0,
            });
        }

        if let Err(error) = self.opaque_ref().and_then(|opaque| {
            opaque
                .validate_source(runtime)
                .map_err(M11InlineEmphasisError::from)
        }) {
            self.fault();
            return Err(error);
        }

        let mut transitions = 0;
        while transitions < fuel {
            let step = match self.phase {
                EmphasisPhase::Initializing => self.poll_initializing(runtime, &mut transitions),
                EmphasisPhase::Scanning => self.poll_scanning(runtime, fuel, &mut transitions),
                EmphasisPhase::Processing => self.poll_processing(runtime, &mut transitions),
                EmphasisPhase::Remainders => self.poll_remainders(runtime, &mut transitions),
                EmphasisPhase::Complete => {
                    return Ok(M11InlineEmphasisPoll {
                        status: M11InlineEmphasisPollStatus::Complete,
                        transitions,
                    });
                }
                EmphasisPhase::Faulted
                | EmphasisPhase::Aborting
                | EmphasisPhase::Aborted
                | EmphasisPhase::Transferred => Err(M11InlineEmphasisError::InvalidState),
            };
            if let Err(error) = step {
                self.fault();
                return Err(error);
            }
        }

        Ok(M11InlineEmphasisPoll {
            status: if self.phase == EmphasisPhase::Complete {
                M11InlineEmphasisPollStatus::Complete
            } else {
                M11InlineEmphasisPollStatus::Pending
            },
            transitions,
        })
    }

    fn poll_initializing(
        &mut self,
        runtime: &DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11InlineEmphasisError> {
        let cursor = self.opaque_ref()?.source_cursor(runtime)?;
        self.scanner = Some(M11InlineLexScanner::new(cursor));
        self.delimiters = Some(M11InlineRadixPages::new(self.source)?);
        self.candidates = Some(M11InlineRadixPages::new(self.source)?);
        self.remainders = Some(M11InlineRadixPages::new(self.source)?);
        self.phase = EmphasisPhase::Scanning;
        *transitions += 1;
        Ok(())
    }

    fn poll_scanning(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineEmphasisError> {
        if self.pending_event.is_some() {
            self.poll_pending_event(runtime)?;
            *transitions += 1;
            return Ok(());
        }

        let poll = self
            .scanner
            .as_mut()
            .ok_or(M11InlineEmphasisError::InvalidState)?
            .poll(fuel - *transitions)?;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
        match poll.status() {
            M11InlineLexPollStatus::Pending => {}
            M11InlineLexPollStatus::Event(event) => {
                if matches!(
                    event.kind(),
                    M11InlineLexEventKind::EmphasisRun { .. }
                        | M11InlineLexEventKind::StrikethroughRun { .. }
                ) {
                    self.pending_event = Some(event);
                }
            }
            M11InlineLexPollStatus::Complete => {
                self.closer = self.first_delimiter;
                self.phase = EmphasisPhase::Processing;
            }
        }
        Ok(())
    }

    fn poll_pending_event(
        &mut self,
        runtime: &mut DocumentRuntime,
    ) -> Result<(), M11InlineEmphasisError> {
        let event = self
            .pending_event
            .ok_or(M11InlineEmphasisError::InvalidState)?;
        if let Some(opaque) = self.opaque_ref()?.candidate(self.opaque_index)? {
            let opaque_range = opaque.relative_range();
            if event.start() >= opaque_range.end {
                self.opaque_index = self
                    .opaque_index
                    .checked_add(1)
                    .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
                return Ok(());
            }
        }
        if self
            .direct_syntax
            .get(self.direct_index)
            .is_some_and(|range| event.start() >= range.end)
        {
            self.direct_index = self
                .direct_index
                .checked_add(1)
                .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
            return Ok(());
        }

        let opaque_range = self
            .opaque_ref()?
            .candidate(self.opaque_index)?
            .map(M11InlineOpaqueCandidate::relative_range);
        let direct_range = self.direct_syntax.get(self.direct_index);
        let opaque_shielded = opaque_range
            .as_ref()
            .is_some_and(|range| range.start <= event.start() && event.end() <= range.end);
        let direct_shielded = direct_range
            .is_some_and(|range| range.start <= event.start() && event.end() <= range.end);
        if opaque_shielded || direct_shielded {
            self.pending_event = None;
            return Ok(());
        }
        if opaque_range
            .as_ref()
            .is_some_and(|range| range.start < event.end() && event.start() < range.end)
        {
            return Err(M11InlineEmphasisError::InvalidState);
        }
        if direct_range.is_some_and(|range| range.start < event.end() && event.start() < range.end)
        {
            return Err(M11InlineEmphasisError::InvalidState);
        }

        let (marker, len, can_open, can_close) = match event.kind() {
            M11InlineLexEventKind::EmphasisRun {
                marker,
                len,
                can_open,
                can_close,
            } => (marker, len, can_open, can_close),
            M11InlineLexEventKind::StrikethroughRun {
                len,
                can_open,
                can_close,
            } => (b'~', len, can_open, can_close),
            _ => return Err(M11InlineEmphasisError::InvalidState),
        };
        self.pending_event = None;
        // The selected Comrak-compatible GFM profile admits one- and
        // two-tilde runs. Longer runs remain literal and do not participate in
        // delimiter ownership.
        if (!can_open && !can_close) || (marker == b'~' && len > 2) {
            return Ok(());
        }
        self.append_delimiter(runtime, event.start(), len, marker, can_open, can_close)
    }

    fn append_delimiter(
        &mut self,
        runtime: &mut DocumentRuntime,
        start: u32,
        len: u32,
        marker: u8,
        can_open: bool,
        can_close: bool,
    ) -> Result<(), M11InlineEmphasisError> {
        let index = self.delimiter_count;
        let previous = self.last_delimiter;
        let delimiter = Delimiter {
            start,
            len,
            original_len: len,
            previous_plus_one: plus_one(previous)?,
            next_plus_one: 0,
            candidate_head_plus_one: 0,
            consumed: 0,
            marker,
            flags: DELIMITER_ACTIVE
                | (u8::from(can_open) * DELIMITER_CAN_OPEN)
                | (u8::from(can_close) * DELIMITER_CAN_CLOSE),
        };
        self.delimiters_mut()?.set(
            runtime,
            usize::try_from(index).map_err(|_| M11InlineEmphasisError::CoordinateOverflow)?,
            delimiter,
        )?;
        if let Some(previous) = previous {
            let mut record = self.delimiter(previous)?;
            record.next_plus_one = index
                .checked_add(1)
                .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
            self.set_delimiter(runtime, previous, record)?;
        } else {
            self.first_delimiter = Some(index);
        }
        self.last_delimiter = Some(index);
        self.delimiter_count = index
            .checked_add(1)
            .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
        Ok(())
    }

    fn poll_processing(
        &mut self,
        runtime: &mut DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11InlineEmphasisError> {
        if self.pending_match.is_some() {
            self.poll_match(runtime)?;
            *transitions += 1;
            return Ok(());
        }
        if self.search.is_some() {
            self.poll_search(runtime)?;
            *transitions += 1;
            return Ok(());
        }

        let Some(closer_index) = self.closer else {
            self.remainder_index = 0;
            self.phase = EmphasisPhase::Remainders;
            return Ok(());
        };
        let closer = self.delimiter(closer_index)?;
        if !closer.active() {
            return Err(M11InlineEmphasisError::InvalidState);
        }
        if !closer.can_close() {
            self.closer = closer.next();
            *transitions += 1;
            return Ok(());
        }
        let category = opener_bottom_category(closer)?;
        self.search = Some(Search {
            closer: closer_index,
            opener: closer.previous(),
            category,
            mod_three_rule_invoked: false,
        });
        *transitions += 1;
        Ok(())
    }

    fn poll_search(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11InlineEmphasisError> {
        let mut search = self
            .search
            .take()
            .ok_or(M11InlineEmphasisError::InvalidState)?;
        let closer = self.delimiter(search.closer)?;
        let Some(opener_index) = search
            .opener
            .filter(|index| *index >= self.openers_bottom[search.category])
        else {
            if !search.mod_three_rule_invoked {
                self.openers_bottom[search.category] = search.closer;
            }
            let next = closer.next();
            if !closer.can_open() {
                self.remove_delimiter(runtime, search.closer, closer)?;
            }
            self.closer = next;
            return Ok(());
        };

        let opener = self.delimiter(opener_index)?;
        if !opener.active() {
            return Err(M11InlineEmphasisError::InvalidState);
        }
        if opener.can_open() && opener.marker == closer.marker {
            let sum = opener
                .original_len
                .checked_add(closer.original_len)
                .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
            let odd_match = (closer.can_open() || opener.can_close())
                && sum % 3 == 0
                && !(opener.original_len % 3 == 0 && closer.original_len % 3 == 0);
            if !odd_match {
                if opener.marker == b'~' && opener.len != closer.len {
                    // Comrak admits both one- and two-tilde delimiters but
                    // never partially consumes a mismatched pair. Its shared
                    // walk terminates at this attempted match, preserving all
                    // already-established inner candidates as literal-aware
                    // output while preventing a later crossing match.
                    self.closer = None;
                    return Ok(());
                }
                self.pending_match = Some(Match {
                    opener: opener_index,
                    closer: search.closer,
                    interior: closer.previous(),
                    use_delimiters: if opener.marker == b'~' {
                        opener.len
                    } else if opener.len >= 2 && closer.len >= 2 {
                        2
                    } else {
                        1
                    },
                    phase: MatchPhase::WriteCandidate,
                });
                return Ok(());
            }
            search.mod_three_rule_invoked = true;
        }
        search.opener = opener.previous();
        self.search = Some(search);
        Ok(())
    }

    fn poll_match(&mut self, runtime: &mut DocumentRuntime) -> Result<(), M11InlineEmphasisError> {
        let mut matched = self
            .pending_match
            .take()
            .ok_or(M11InlineEmphasisError::InvalidState)?;
        match matched.phase {
            MatchPhase::WriteCandidate => {
                let mut opener = self.delimiter(matched.opener)?;
                let closer = self.delimiter(matched.closer)?;
                let opener_end = opener.end()?;
                let opener_start = opener_end
                    .checked_sub(matched.use_delimiters)
                    .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
                let closer_end = closer
                    .start
                    .checked_add(matched.use_delimiters)
                    .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
                if opener_end > closer.start {
                    return Err(M11InlineEmphasisError::InvalidState);
                }
                let candidate = M11EmphasisCandidate {
                    range_start: opener_start,
                    range_end: closer_end,
                    content_start: opener_end,
                    content_end: closer.start,
                    marker: opener.marker,
                    kind: if opener.marker == b'~' {
                        M11EmphasisCandidateKind::Strikethrough
                    } else if matched.use_delimiters == 2 {
                        M11EmphasisCandidateKind::Strong
                    } else {
                        M11EmphasisCandidateKind::Emphasis
                    },
                    reserved: 0,
                    next_same_opener_plus_one: opener.candidate_head_plus_one,
                };
                let index = self.candidate_count;
                self.candidates_mut()?.set(
                    runtime,
                    usize::try_from(index)
                        .map_err(|_| M11InlineEmphasisError::CoordinateOverflow)?,
                    candidate,
                )?;
                opener.candidate_head_plus_one = index
                    .checked_add(1)
                    .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
                self.set_delimiter(runtime, matched.opener, opener)?;
                self.candidate_count = index
                    .checked_add(1)
                    .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
                matched.phase = MatchPhase::RemoveInterior;
                self.pending_match = Some(matched);
            }
            MatchPhase::RemoveInterior => {
                let Some(interior) = matched.interior else {
                    return Err(M11InlineEmphasisError::InvalidState);
                };
                if interior == matched.opener {
                    matched.phase = MatchPhase::ConsumeOpener;
                } else {
                    let record = self.delimiter(interior)?;
                    matched.interior = record.previous();
                    self.remove_delimiter(runtime, interior, record)?;
                }
                self.pending_match = Some(matched);
            }
            MatchPhase::ConsumeOpener => {
                let mut opener = self.delimiter(matched.opener)?;
                opener.len = opener
                    .len
                    .checked_sub(matched.use_delimiters)
                    .ok_or(M11InlineEmphasisError::InvalidState)?;
                opener.consumed = opener
                    .consumed
                    .checked_add(matched.use_delimiters)
                    .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
                self.set_or_remove_delimiter(runtime, matched.opener, opener)?;
                matched.phase = MatchPhase::ConsumeCloser;
                self.pending_match = Some(matched);
            }
            MatchPhase::ConsumeCloser => {
                let mut closer = self.delimiter(matched.closer)?;
                let next = closer.next();
                closer.start = closer
                    .start
                    .checked_add(matched.use_delimiters)
                    .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
                closer.len = closer
                    .len
                    .checked_sub(matched.use_delimiters)
                    .ok_or(M11InlineEmphasisError::InvalidState)?;
                closer.consumed = closer
                    .consumed
                    .checked_add(matched.use_delimiters)
                    .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
                let remains = closer.len != 0;
                self.set_or_remove_delimiter(runtime, matched.closer, closer)?;
                self.closer = if remains { Some(matched.closer) } else { next };
            }
        }
        Ok(())
    }

    fn poll_remainders(
        &mut self,
        runtime: &mut DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11InlineEmphasisError> {
        if self.remainder_index >= self.delimiter_count {
            self.phase = EmphasisPhase::Complete;
            return Ok(());
        }
        let record = self.delimiter(self.remainder_index)?;
        if record.consumed != 0 && record.len != 0 {
            let remainder = M11EmphasisRemainderCandidate {
                start: record.start,
                end: record.end()?,
                marker: record.marker,
                reserved: [0; 3],
            };
            let index = self.remainder_count;
            self.remainders_mut()?.set(
                runtime,
                usize::try_from(index).map_err(|_| M11InlineEmphasisError::CoordinateOverflow)?,
                remainder,
            )?;
            self.remainder_count = index
                .checked_add(1)
                .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
        }
        self.remainder_index = self
            .remainder_index
            .checked_add(1)
            .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
        *transitions += 1;
        Ok(())
    }

    fn set_or_remove_delimiter(
        &mut self,
        runtime: &mut DocumentRuntime,
        index: u32,
        record: Delimiter,
    ) -> Result<(), M11InlineEmphasisError> {
        if record.len == 0 {
            self.remove_delimiter(runtime, index, record)
        } else {
            self.set_delimiter(runtime, index, record)
        }
    }

    fn remove_delimiter(
        &mut self,
        runtime: &mut DocumentRuntime,
        index: u32,
        mut record: Delimiter,
    ) -> Result<(), M11InlineEmphasisError> {
        if !record.active() {
            return Err(M11InlineEmphasisError::InvalidState);
        }
        let previous = record.previous();
        let next = record.next();
        if let Some(previous) = previous {
            let mut neighbor = self.delimiter(previous)?;
            neighbor.next_plus_one = plus_one(next)?;
            self.set_delimiter(runtime, previous, neighbor)?;
        } else {
            self.first_delimiter = next;
        }
        if let Some(next) = next {
            let mut neighbor = self.delimiter(next)?;
            neighbor.previous_plus_one = plus_one(previous)?;
            self.set_delimiter(runtime, next, neighbor)?;
        } else {
            self.last_delimiter = previous;
        }
        record.flags &= !DELIMITER_ACTIVE;
        self.set_delimiter(runtime, index, record)
    }

    fn delimiter(&self, index: u32) -> Result<Delimiter, M11InlineEmphasisError> {
        self.delimiters_ref()?
            .get(usize::try_from(index).map_err(|_| M11InlineEmphasisError::CoordinateOverflow)?)?
            .ok_or(M11InlineEmphasisError::InvalidState)
    }

    fn set_delimiter(
        &mut self,
        runtime: &mut DocumentRuntime,
        index: u32,
        record: Delimiter,
    ) -> Result<(), M11InlineEmphasisError> {
        self.delimiters_mut()?.set(
            runtime,
            usize::try_from(index).map_err(|_| M11InlineEmphasisError::CoordinateOverflow)?,
            record,
        )?;
        Ok(())
    }

    fn opaque_ref(&self) -> Result<&M11InlineOpaqueCandidates, M11InlineEmphasisError> {
        self.opaque
            .as_ref()
            .ok_or(M11InlineEmphasisError::InvalidState)
    }

    fn delimiters_ref(
        &self,
    ) -> Result<&M11InlineRadixPages<Delimiter, DELIMITER_PAGE_RECORDS>, M11InlineEmphasisError>
    {
        self.delimiters
            .as_ref()
            .ok_or(M11InlineEmphasisError::InvalidState)
    }

    fn delimiters_mut(
        &mut self,
    ) -> Result<&mut M11InlineRadixPages<Delimiter, DELIMITER_PAGE_RECORDS>, M11InlineEmphasisError>
    {
        self.delimiters
            .as_mut()
            .ok_or(M11InlineEmphasisError::InvalidState)
    }

    fn candidates_mut(
        &mut self,
    ) -> Result<
        &mut M11InlineRadixPages<M11EmphasisCandidate, CANDIDATE_PAGE_RECORDS>,
        M11InlineEmphasisError,
    > {
        self.candidates
            .as_mut()
            .ok_or(M11InlineEmphasisError::InvalidState)
    }

    fn remainders_mut(
        &mut self,
    ) -> Result<
        &mut M11InlineRadixPages<M11EmphasisRemainderCandidate, REMAINDER_PAGE_RECORDS>,
        M11InlineEmphasisError,
    > {
        self.remainders
            .as_mut()
            .ok_or(M11InlineEmphasisError::InvalidState)
    }

    fn fault(&mut self) {
        if let Some(scanner) = self.scanner.as_mut() {
            scanner.cancel();
        }
        self.phase = EmphasisPhase::Faulted;
    }

    pub(crate) fn take_output(&mut self) -> Option<M11InlineCandidates> {
        if self.phase != EmphasisPhase::Complete {
            return None;
        }
        let opaque_count = self.opaque_ref().ok()?.len();
        let opaque = self.opaque.take()?;
        let delimiters = self.delimiters.take()?;
        let candidates = self.candidates.take()?;
        let remainders = self.remainders.take()?;
        self.phase = EmphasisPhase::Transferred;
        Some(M11InlineCandidates {
            source: self.source,
            source_range: self.source_range.clone(),
            opaque: Some(opaque),
            delimiters: Some(delimiters),
            emphasis: Some(candidates),
            remainders: Some(remainders),
            delimiter_count: self.delimiter_count,
            emphasis_count: self.candidate_count,
            remainder_count: self.remainder_count,
            opaque_count,
            delimiter_reclaim_started: false,
            emphasis_reclaim_started: false,
            remainder_reclaim_started: false,
            code_release_started: false,
            release_complete: false,
        })
    }

    pub(crate) fn begin_abort(&mut self) -> Result<(), M11InlineEmphasisError> {
        if matches!(
            self.phase,
            EmphasisPhase::Transferred | EmphasisPhase::Aborted
        ) {
            return Err(M11InlineEmphasisError::InvalidState);
        }
        if self.phase == EmphasisPhase::Aborting {
            return Ok(());
        }
        if let Some(scanner) = self.scanner.as_mut() {
            scanner.cancel();
        }
        if !self.code_release_started {
            self.opaque
                .as_mut()
                .ok_or(M11InlineEmphasisError::InvalidState)?
                .begin_release()?;
            self.code_release_started = true;
        }
        begin_pages_reclaim(&mut self.delimiters, &mut self.delimiter_reclaim_started)?;
        begin_pages_reclaim(&mut self.candidates, &mut self.candidate_reclaim_started)?;
        begin_pages_reclaim(&mut self.remainders, &mut self.remainder_reclaim_started)?;
        self.phase = EmphasisPhase::Aborting;
        Ok(())
    }

    pub(crate) fn poll_abort(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineEmphasisReleasePoll, M11InlineEmphasisError> {
        validate_fuel(fuel)?;
        if self.phase != EmphasisPhase::Aborting {
            return Err(M11InlineEmphasisError::InvalidState);
        }
        let mut transitions = 0;
        while transitions < fuel {
            if poll_and_drop_pages(
                &mut self.delimiters,
                runtime,
                fuel - transitions,
                &mut transitions,
            )? {
                continue;
            }
            if poll_and_drop_pages(
                &mut self.candidates,
                runtime,
                fuel - transitions,
                &mut transitions,
            )? {
                continue;
            }
            if poll_and_drop_pages(
                &mut self.remainders,
                runtime,
                fuel - transitions,
                &mut transitions,
            )? {
                continue;
            }
            if let Some(opaque) = self.opaque.as_mut() {
                let poll = opaque.poll_release(runtime, fuel - transitions)?;
                transitions = transitions
                    .checked_add(poll.transitions())
                    .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
                if poll.complete() {
                    drop(self.opaque.take());
                    continue;
                }
                return Ok(M11InlineEmphasisReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            self.phase = EmphasisPhase::Aborted;
            return Ok(M11InlineEmphasisReleasePoll {
                transitions,
                complete: true,
            });
        }
        Ok(M11InlineEmphasisReleasePoll {
            transitions,
            complete: false,
        })
    }
}

impl Drop for M11InlineEmphasisJob {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                matches!(
                    self.phase,
                    EmphasisPhase::Aborted | EmphasisPhase::Transferred
                ),
                "inline emphasis jobs require output transfer or explicit fuelled abort"
            );
        }
    }
}

/// Move-only opaque and emphasis candidates for the later precedence stage.
pub(crate) struct M11InlineCandidates {
    source: SourceVersion,
    source_range: Range<u32>,
    opaque: Option<M11InlineOpaqueCandidates>,
    delimiters: Option<M11InlineRadixPages<Delimiter, DELIMITER_PAGE_RECORDS>>,
    emphasis: Option<M11InlineRadixPages<M11EmphasisCandidate, CANDIDATE_PAGE_RECORDS>>,
    remainders: Option<M11InlineRadixPages<M11EmphasisRemainderCandidate, REMAINDER_PAGE_RECORDS>>,
    delimiter_count: u32,
    emphasis_count: u32,
    remainder_count: u32,
    opaque_count: u32,
    delimiter_reclaim_started: bool,
    emphasis_reclaim_started: bool,
    remainder_reclaim_started: bool,
    code_release_started: bool,
    release_complete: bool,
}

impl M11InlineCandidates {
    pub(crate) const fn source(&self) -> SourceVersion {
        self.source
    }

    pub(crate) fn source_range(&self) -> Range<u32> {
        self.source_range.clone()
    }

    pub(crate) fn validate_source(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<(), M11InlineEmphasisError> {
        self.opaque
            .as_ref()
            .ok_or(M11InlineEmphasisError::InvalidState)?
            .validate_source(runtime)?;
        Ok(())
    }

    pub(crate) fn source_authority(
        &self,
    ) -> Result<&M11ParserSourceRangeAuthority, M11InlineEmphasisError> {
        Ok(self
            .opaque
            .as_ref()
            .ok_or(M11InlineEmphasisError::InvalidState)?
            .source_authority()?)
    }

    pub(crate) fn source_cursor(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<M11ParserRangeCursor, M11InlineEmphasisError> {
        Ok(self
            .opaque
            .as_ref()
            .ok_or(M11InlineEmphasisError::InvalidState)?
            .source_cursor(runtime)?)
    }

    pub(crate) const fn opaque_len(&self) -> u32 {
        self.opaque_count
    }

    pub(crate) fn opaque_candidate(
        &self,
        index: u32,
    ) -> Result<Option<M11InlineOpaqueCandidate>, M11InlineEmphasisError> {
        Ok(self
            .opaque
            .as_ref()
            .ok_or(M11InlineEmphasisError::InvalidState)?
            .candidate(index)?)
    }

    pub(crate) const fn delimiter_len(&self) -> u32 {
        self.delimiter_count
    }

    /// Head of the outer-to-inner candidate chain for one source-ordered
    /// delimiter. Iterating delimiters by index and following this link emits
    /// CommonMark source/preorder without a global sort.
    pub(crate) fn delimiter_candidate_head(
        &self,
        delimiter_index: u32,
    ) -> Result<Option<u32>, M11InlineEmphasisError> {
        if delimiter_index >= self.delimiter_count {
            return Ok(None);
        }
        let delimiter = self
            .delimiters
            .as_ref()
            .ok_or(M11InlineEmphasisError::InvalidState)?
            .get(
                usize::try_from(delimiter_index)
                    .map_err(|_| M11InlineEmphasisError::CoordinateOverflow)?,
            )?
            .ok_or(M11InlineEmphasisError::InvalidState)?;
        Ok(delimiter.candidate_head_plus_one.checked_sub(1))
    }

    pub(crate) const fn emphasis_len(&self) -> u32 {
        self.emphasis_count
    }

    pub(crate) fn emphasis_candidate(
        &self,
        index: u32,
    ) -> Result<Option<M11EmphasisCandidate>, M11InlineEmphasisError> {
        if index >= self.emphasis_count {
            return Ok(None);
        }
        Ok(Some(
            self.emphasis
                .as_ref()
                .ok_or(M11InlineEmphasisError::InvalidState)?
                .get(
                    usize::try_from(index)
                        .map_err(|_| M11InlineEmphasisError::CoordinateOverflow)?,
                )?
                .ok_or(M11InlineEmphasisError::InvalidState)?,
        ))
    }

    pub(crate) const fn remainder_len(&self) -> u32 {
        self.remainder_count
    }

    pub(crate) fn remainder_candidate(
        &self,
        index: u32,
    ) -> Result<Option<M11EmphasisRemainderCandidate>, M11InlineEmphasisError> {
        if index >= self.remainder_count {
            return Ok(None);
        }
        Ok(Some(
            self.remainders
                .as_ref()
                .ok_or(M11InlineEmphasisError::InvalidState)?
                .get(
                    usize::try_from(index)
                        .map_err(|_| M11InlineEmphasisError::CoordinateOverflow)?,
                )?
                .ok_or(M11InlineEmphasisError::InvalidState)?,
        ))
    }

    pub(crate) fn begin_release(&mut self) -> Result<(), M11InlineEmphasisError> {
        if self.release_complete {
            return Err(M11InlineEmphasisError::InvalidState);
        }
        if !self.code_release_started {
            self.opaque
                .as_mut()
                .ok_or(M11InlineEmphasisError::InvalidState)?
                .begin_release()?;
            self.code_release_started = true;
        }
        begin_pages_reclaim(&mut self.delimiters, &mut self.delimiter_reclaim_started)?;
        begin_pages_reclaim(&mut self.emphasis, &mut self.emphasis_reclaim_started)?;
        begin_pages_reclaim(&mut self.remainders, &mut self.remainder_reclaim_started)?;
        Ok(())
    }

    pub(crate) fn poll_release(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineEmphasisReleasePoll, M11InlineEmphasisError> {
        validate_fuel(fuel)?;
        if !self.delimiter_reclaim_started
            || !self.emphasis_reclaim_started
            || !self.remainder_reclaim_started
            || !self.code_release_started
        {
            return Err(M11InlineEmphasisError::InvalidState);
        }
        if self.release_complete {
            return Ok(M11InlineEmphasisReleasePoll {
                transitions: 0,
                complete: true,
            });
        }
        let mut transitions = 0;
        while transitions < fuel {
            if poll_and_drop_pages(
                &mut self.delimiters,
                runtime,
                fuel - transitions,
                &mut transitions,
            )? {
                continue;
            }
            if poll_and_drop_pages(
                &mut self.emphasis,
                runtime,
                fuel - transitions,
                &mut transitions,
            )? {
                continue;
            }
            if poll_and_drop_pages(
                &mut self.remainders,
                runtime,
                fuel - transitions,
                &mut transitions,
            )? {
                continue;
            }
            let opaque = self
                .opaque
                .as_mut()
                .ok_or(M11InlineEmphasisError::InvalidState)?;
            let poll = opaque.poll_release(runtime, fuel - transitions)?;
            transitions = transitions
                .checked_add(poll.transitions())
                .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
            if poll.complete() {
                self.release_complete = true;
                return Ok(M11InlineEmphasisReleasePoll {
                    transitions,
                    complete: true,
                });
            }
            return Ok(M11InlineEmphasisReleasePoll {
                transitions,
                complete: false,
            });
        }
        Ok(M11InlineEmphasisReleasePoll {
            transitions,
            complete: false,
        })
    }

    pub(crate) fn take_source_authority(&mut self) -> Option<M11ParserSourceRangeAuthority> {
        if !self.release_complete
            || self.delimiters.is_some()
            || self.emphasis.is_some()
            || self.remainders.is_some()
        {
            return None;
        }
        let authority = self.opaque.as_mut()?.take_source_authority()?;
        drop(self.opaque.take());
        Some(authority)
    }
}

impl Drop for M11InlineCandidates {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.delimiters.is_none()
                    && self.emphasis.is_none()
                    && self.remainders.is_none()
                    && (self.opaque.is_none() || self.release_complete),
                "inline candidates require explicit fuelled release"
            );
        }
    }
}

fn opener_bottom_category(closer: Delimiter) -> Result<usize, M11InlineEmphasisError> {
    match closer.marker {
        b'_' => Ok(0),
        b'*' => {
            let offset = usize::from(closer.can_open()) * 3
                + usize::try_from(closer.original_len % 3)
                    .map_err(|_| M11InlineEmphasisError::CoordinateOverflow)?;
            1usize
                .checked_add(offset)
                .ok_or(M11InlineEmphasisError::CoordinateOverflow)
        }
        b'~' => Ok(7),
        _ => Err(M11InlineEmphasisError::InvalidState),
    }
}

fn plus_one(index: Option<u32>) -> Result<u32, M11InlineEmphasisError> {
    index.map_or(Ok(0), |index| {
        index
            .checked_add(1)
            .ok_or(M11InlineEmphasisError::CoordinateOverflow)
    })
}

fn begin_pages_reclaim<T: Copy + Default, const PAGE_RECORDS: usize>(
    pages: &mut Option<M11InlineRadixPages<T, PAGE_RECORDS>>,
    started: &mut bool,
) -> Result<(), M11InlineEmphasisError> {
    if pages.is_some() && !*started {
        pages
            .as_mut()
            .ok_or(M11InlineEmphasisError::InvalidState)?
            .begin_reclaim()?;
        *started = true;
    }
    Ok(())
}

fn poll_and_drop_pages<T: Copy + Default, const PAGE_RECORDS: usize>(
    pages: &mut Option<M11InlineRadixPages<T, PAGE_RECORDS>>,
    runtime: &mut DocumentRuntime,
    fuel: usize,
    transitions: &mut usize,
) -> Result<bool, M11InlineEmphasisError> {
    let Some(owner) = pages.as_mut() else {
        return Ok(false);
    };
    let poll: M11InlineRadixReclaimPoll = owner.poll_reclaim(runtime, fuel)?;
    *transitions = transitions
        .checked_add(poll.transitions())
        .ok_or(M11InlineEmphasisError::CoordinateOverflow)?;
    if poll.complete() {
        drop(pages.take());
    }
    Ok(true)
}

fn validate_fuel(fuel: usize) -> Result<(), M11InlineEmphasisError> {
    if fuel == 0 {
        return Err(M11InlineEmphasisError::ZeroFuel);
    }
    if fuel > M11_INLINE_EMPHASIS_MAX_POLL_TRANSITIONS {
        return Err(M11InlineEmphasisError::PollLimitExceeded);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline_autolink::{
        M11InlineAutolinkJob, M11InlineAutolinkPollStatus, M11InlineOpaqueCandidate,
        M11InlineOpaqueCandidates, M11InlineOpaqueKind, M11InlineOpaquePollStatus,
        M11InlineOpaqueResolveJob,
    };
    use crate::inline_code::{M11InlineCodeJob, M11InlineCodePollStatus, M11InlineCodeRuns};
    use comrak::{markdown_to_html, Options as ComrakOptions};
    use flark_engine::parser_internal::{
        M11ParserSourceRangeAuthority, M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS,
        M11_INLINE_PROJECTION_FLAG_CODE_TRIM_ONE_SPACE,
    };
    use flark_engine::{ArenaLimits, DocumentRuntimeConfig};
    use std::cmp::Reverse;

    #[derive(Debug, Eq, PartialEq)]
    struct Resolution {
        source: SourceVersion,
        source_range: Range<u32>,
        code: Vec<M11InlineOpaqueCandidate>,
        emphasis: Vec<M11EmphasisCandidate>,
        emphasis_preorder: Vec<M11EmphasisCandidate>,
        remainders: Vec<M11EmphasisRemainderCandidate>,
        maximum_poll_transitions: usize,
        maximum_retained_scratch_bytes: usize,
    }

    type CanonicalCandidate = (M11EmphasisCandidateKind, Range<u32>, Range<u32>, Range<u32>);

    fn close_runtime(mut runtime: DocumentRuntime) {
        runtime.begin_close().expect("begin close");
        while !runtime.poll_close(64).expect("close").complete {}
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
    }

    fn resolve_code(
        runtime: &mut DocumentRuntime,
        source_range: Range<usize>,
        fuel: usize,
    ) -> (M11InlineCodeJob, M11InlineCodeRuns) {
        let authority = M11ParserSourceRangeAuthority::new(
            runtime,
            runtime.snapshot_current_source().expect("lease"),
            source_range,
        )
        .expect("authority");
        let mut job = M11InlineCodeJob::new(runtime, authority).expect("code job");
        loop {
            let poll = job.poll(runtime, fuel).expect("code poll");
            assert!(poll.transitions() <= fuel);
            if poll.status() == M11InlineCodePollStatus::Complete {
                break;
            }
        }
        let output = job.take_output().expect("code output");
        (job, output)
    }

    fn resolve_opaque(
        runtime: &mut DocumentRuntime,
        source_range: Range<usize>,
        fuel: usize,
    ) -> (
        M11InlineCodeJob,
        M11InlineAutolinkJob,
        M11InlineOpaqueCandidates,
    ) {
        let (code_job, code) = resolve_code(runtime, source_range, fuel);
        let mut autolink_job = M11InlineAutolinkJob::new(runtime, &code).expect("autolink job");
        loop {
            let poll = autolink_job.poll(runtime, fuel).expect("autolink poll");
            assert!(poll.transitions() <= fuel);
            if poll.status() == M11InlineAutolinkPollStatus::Complete {
                break;
            }
        }
        let mut code = Some(code);
        let mut opaque_job =
            M11InlineOpaqueResolveJob::take_new(runtime, &mut code, &mut autolink_job)
                .expect("opaque job");
        assert!(code.is_none());
        loop {
            let poll = opaque_job.poll(runtime, fuel).expect("opaque poll");
            assert!(poll.transitions() <= fuel);
            if poll.status() == M11InlineOpaquePollStatus::Complete {
                break;
            }
        }
        let opaque = opaque_job.take_output().expect("opaque candidates");
        drop(opaque_job);
        (code_job, autolink_job, opaque)
    }

    fn resolve(source_text: &str, fuel: usize) -> Resolution {
        resolve_in(source_text, 0..source_text.len(), fuel)
    }

    fn resolve_in(source_text: &str, source_range: Range<usize>, fuel: usize) -> Resolution {
        let mut runtime =
            DocumentRuntime::new(source_text, DocumentRuntimeConfig::default()).expect("runtime");
        let source = runtime.current_source_version().expect("source");
        let (code_job, autolink_job, opaque) =
            resolve_opaque(&mut runtime, source_range.clone(), fuel);

        let mut job = M11InlineEmphasisJob::new(opaque).expect("emphasis job");
        let mut maximum_poll_transitions = 0;
        let mut maximum_retained_scratch_bytes =
            runtime.arena_metrics().reserved_external_payload_bytes;
        loop {
            let poll = job.poll(&mut runtime, fuel).expect("emphasis poll");
            maximum_poll_transitions = maximum_poll_transitions.max(poll.transitions());
            maximum_retained_scratch_bytes = maximum_retained_scratch_bytes
                .max(runtime.arena_metrics().reserved_external_payload_bytes);
            assert!(poll.transitions() <= fuel);
            if poll.status() == M11InlineEmphasisPollStatus::Complete {
                break;
            }
        }
        let mut output = job.take_output().expect("inline candidates");
        assert_eq!(output.source(), source);
        output.validate_source(&runtime).expect("source authority");

        let mut code = Vec::new();
        for index in 0..output.opaque_len() {
            let candidate = output
                .opaque_candidate(index)
                .expect("opaque candidate")
                .expect("present");
            if candidate.kind() == M11InlineOpaqueKind::Code {
                code.push(candidate);
            }
        }
        let mut emphasis = Vec::new();
        for index in 0..output.emphasis_len() {
            emphasis.push(
                output
                    .emphasis_candidate(index)
                    .expect("emphasis candidate")
                    .expect("present"),
            );
        }
        let mut emphasis_preorder = Vec::new();
        for delimiter_index in 0..output.delimiter_len() {
            let mut candidate_index = output
                .delimiter_candidate_head(delimiter_index)
                .expect("delimiter candidate head");
            while let Some(index) = candidate_index {
                let candidate = output
                    .emphasis_candidate(index)
                    .expect("linked emphasis candidate")
                    .expect("present");
                emphasis_preorder.push(candidate);
                candidate_index = candidate.next_same_opener();
            }
        }
        let mut remainders = Vec::new();
        for index in 0..output.remainder_len() {
            remainders.push(
                output
                    .remainder_candidate(index)
                    .expect("remainder candidate")
                    .expect("present"),
            );
        }

        output.begin_release().expect("begin output release");
        loop {
            let poll = output.poll_release(&mut runtime, 1).expect("release");
            assert!(poll.transitions() <= 1);
            if poll.complete() {
                break;
            }
        }
        let authority = output
            .take_source_authority()
            .expect("source authority baton");
        let mut cursor = authority.cursor(&runtime).expect("baton cursor");
        cursor.cancel();
        drop(cursor);
        drop(authority);
        drop(output);
        drop(job);
        drop(autolink_job);
        drop(code_job);
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
        close_runtime(runtime);

        Resolution {
            source,
            source_range: u32::try_from(source_range.start).expect("start")
                ..u32::try_from(source_range.end).expect("end"),
            code,
            emphasis,
            emphasis_preorder,
            remainders,
            maximum_poll_transitions,
            maximum_retained_scratch_bytes,
        }
    }

    fn canonical(candidates: &[M11EmphasisCandidate]) -> Vec<CanonicalCandidate> {
        let mut candidates = candidates.to_vec();
        candidates.sort_by_key(|candidate| {
            (
                candidate.relative_range().start,
                Reverse(candidate.relative_range().end),
            )
        });
        candidates
            .into_iter()
            .map(|candidate| {
                (
                    candidate.kind(),
                    candidate.relative_range(),
                    candidate.relative_opener_range(),
                    candidate.relative_closer_range(),
                )
            })
            .collect()
    }

    #[test]
    fn learned_nested_adjacent_and_intraword_ownership_is_exact() {
        type Expected = (M11EmphasisCandidateKind, Range<u32>, Range<u32>, Range<u32>);
        let cases: &[(&str, &[Expected])] = &[
            (
                "***x***",
                &[
                    (M11EmphasisCandidateKind::Emphasis, 0..7, 0..1, 6..7),
                    (M11EmphasisCandidateKind::Strong, 1..6, 1..3, 4..6),
                ],
            ),
            (
                "**a *b* c**",
                &[
                    (M11EmphasisCandidateKind::Strong, 0..11, 0..2, 9..11),
                    (M11EmphasisCandidateKind::Emphasis, 4..7, 4..5, 6..7),
                ],
            ),
            (
                "_a_",
                &[(M11EmphasisCandidateKind::Emphasis, 0..3, 0..1, 2..3)],
            ),
            (
                "__a__",
                &[(M11EmphasisCandidateKind::Strong, 0..5, 0..2, 3..5)],
            ),
            (
                "*a***b**",
                &[
                    (M11EmphasisCandidateKind::Emphasis, 0..3, 0..1, 2..3),
                    (M11EmphasisCandidateKind::Strong, 3..8, 3..5, 6..8),
                ],
            ),
        ];

        for (source, expected) in cases {
            let result = resolve(source, 1);
            assert_eq!(canonical(&result.emphasis), *expected, "source={source:?}");
            assert!(result.remainders.is_empty(), "source={source:?}");
        }

        let triple = resolve("***x***", 1);
        assert_eq!(
            triple
                .emphasis_preorder
                .iter()
                .map(|candidate| candidate.kind())
                .collect::<Vec<_>>(),
            vec![
                M11EmphasisCandidateKind::Emphasis,
                M11EmphasisCandidateKind::Strong,
            ]
        );
        assert_eq!(
            triple
                .emphasis_preorder
                .iter()
                .map(|candidate| candidate.relative_range())
                .collect::<Vec<_>>(),
            vec![0..7, 1..6]
        );
        for source in ["a_b_c", "a__b__c", "word_with_many_parts"] {
            let result = resolve(source, 1);
            assert!(result.emphasis.is_empty(), "source={source:?}");
            assert!(result.remainders.is_empty(), "source={source:?}");
        }
    }

    #[test]
    fn accepted_code_ranges_shield_delimiters_before_matching() {
        let source = "before `**not strong* and _not em_ and ~~not strike~~` after **live**";
        let result = resolve(source, 3);
        assert_eq!(result.code.len(), 1);
        assert_eq!(result.emphasis.len(), 1);
        let live = result.emphasis[0];
        assert_eq!(live.kind(), M11EmphasisCandidateKind::Strong);
        assert_eq!(
            &source.as_bytes()[live.relative_content_range().start as usize
                ..live.relative_content_range().end as usize],
            b"live"
        );
        assert!(result.remainders.is_empty());
    }

    #[test]
    fn resolved_code_flags_follow_commonmark_edge_space_and_line_ending_rules() {
        let both = M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS
            | M11_INLINE_PROJECTION_FLAG_CODE_TRIM_ONE_SPACE;
        for (content, expected_flags) in [
            (" a ", M11_INLINE_PROJECTION_FLAG_CODE_TRIM_ONE_SPACE),
            ("   ", 0),
            ("\na\n", both),
            ("\ra\r", both),
            ("\r\na\r\n", both),
            (
                "\n\n",
                M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS,
            ),
            (
                "\r\n",
                M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS,
            ),
            (
                "a\n",
                M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS,
            ),
            (
                "\na",
                M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS,
            ),
            ("\ta\t", 0),
            ("\u{a0}a\u{a0}", 0),
        ] {
            let source = format!("`{content}`");
            for fuel in [1, 31] {
                let result = resolve(&source, fuel);
                assert_eq!(result.code.len(), 1, "content={content:?}, fuel={fuel}");
                assert_eq!(
                    result.code[0].flags(),
                    expected_flags,
                    "content={content:?}, fuel={fuel}"
                );
            }
        }
    }

    #[test]
    fn gfm_strikethrough_uses_the_shared_source_ordered_delimiter_stack() {
        type Expected = (M11EmphasisCandidateKind, Range<u32>, Range<u32>, Range<u32>);
        let cases: &[(&str, &[Expected])] = &[
            (
                "~x~",
                &[(M11EmphasisCandidateKind::Strikethrough, 0..3, 0..1, 2..3)],
            ),
            (
                "~~x~~",
                &[(M11EmphasisCandidateKind::Strikethrough, 0..5, 0..2, 3..5)],
            ),
            (
                "~~*x*~~",
                &[
                    (M11EmphasisCandidateKind::Strikethrough, 0..7, 0..2, 5..7),
                    (M11EmphasisCandidateKind::Emphasis, 2..5, 2..3, 4..5),
                ],
            ),
            (
                "*~~x~~*",
                &[
                    (M11EmphasisCandidateKind::Emphasis, 0..7, 0..1, 6..7),
                    (M11EmphasisCandidateKind::Strikethrough, 1..6, 1..3, 4..6),
                ],
            ),
            (
                "~~*x~~*",
                &[(M11EmphasisCandidateKind::Strikethrough, 0..6, 0..2, 4..6)],
            ),
        ];
        for (source, expected) in cases {
            let result = resolve(source, 1);
            assert_eq!(canonical(&result.emphasis), *expected, "source={source:?}");
            assert!(result.remainders.is_empty(), "source={source:?}");
        }

        for source in ["~a~~", "~~a~", "~~~a~~~", "~~~~a~~~~"] {
            let result = resolve(source, 1);
            assert!(result.emphasis.is_empty(), "source={source:?}");
            assert!(result.remainders.is_empty(), "source={source:?}");
        }

        let curious = "This ~text~~~~ is ~~~~curious~.";
        let result = resolve(curious, 1);
        assert_eq!(
            canonical(&result.emphasis),
            vec![(M11EmphasisCandidateKind::Strikethrough, 5..30, 5..6, 29..30,)]
        );

        let soft_break = "hello ~~world~~ between ~~wo\nrld~~ after";
        let result = resolve(soft_break, 1);
        assert_eq!(
            canonical(&result.emphasis)
                .into_iter()
                .map(|candidate| candidate.1)
                .collect::<Vec<_>>(),
            vec![6..15, 24..34]
        );
    }

    #[test]
    fn partial_matched_runs_remain_explicit_for_later_fail_closed_policy() {
        for (source, expected_remainder) in [
            ("**wow*", 0..1),
            ("*wow**", 5..6),
            ("__wow_", 0..1),
            ("_wow__", 5..6),
            ("***wow*", 0..2),
        ] {
            let result = resolve(source, 2);
            assert_eq!(result.emphasis.len(), 1, "source={source:?}");
            assert_eq!(result.remainders.len(), 1, "source={source:?}");
            assert_eq!(
                result.remainders[0].relative_range(),
                expected_remainder,
                "source={source:?}"
            );
        }

        for source in ["*", "**", "***", "ordinary * literal"] {
            let result = resolve(source, 2);
            assert!(result.emphasis.is_empty(), "source={source:?}");
            assert!(result.remainders.is_empty(), "source={source:?}");
        }
    }

    #[test]
    fn unicode_flanking_and_cross_line_spans_follow_commonmark() {
        let unicode = resolve("α—_β_ α★_γ_ α_δ_γ", 2);
        assert_eq!(unicode.emphasis.len(), 2);
        assert!(unicode
            .emphasis
            .iter()
            .all(|candidate| candidate.kind() == M11EmphasisCandidateKind::Emphasis));

        let source = "**bold\ncontinues** and *em\ncontinues*";
        let cross_line = resolve(source, 2);
        let actual = canonical(&cross_line.emphasis);
        assert_eq!(
            actual,
            vec![
                (M11EmphasisCandidateKind::Strong, 0..18, 0..2, 16..18,),
                (M11EmphasisCandidateKind::Emphasis, 23..37, 23..24, 36..37,),
            ]
        );
    }

    #[test]
    fn source_range_authority_and_poll_partition_are_invariant() {
        let prefix = "OUTé:";
        let visible = "***α*** `*shielded*` **β**";
        let source = format!("{prefix}{visible}:終OUT");
        let range = prefix.len()..prefix.len() + visible.len();
        let expected = resolve_in(
            &source,
            range.clone(),
            M11_INLINE_EMPHASIS_MAX_POLL_TRANSITIONS,
        );
        assert_eq!(
            expected.source_range,
            u32::try_from(range.start).expect("start")..u32::try_from(range.end).expect("end")
        );
        for fuel in [1, 2, 7, 31, 257] {
            let actual = resolve_in(&source, range.clone(), fuel);
            assert_eq!(actual.code, expected.code, "fuel={fuel}");
            assert_eq!(actual.emphasis, expected.emphasis, "fuel={fuel}");
            assert_eq!(
                actual.emphasis_preorder, expected.emphasis_preorder,
                "fuel={fuel}"
            );
            assert_eq!(actual.remainders, expected.remainders, "fuel={fuel}");
            assert!(actual.maximum_poll_transitions <= fuel);
        }
    }

    #[test]
    fn partial_work_aborts_and_reclaims_every_admitted_page_with_fuel_one() {
        let source = "*x* ".repeat(5_000);
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let (code_job, autolink_job, opaque) = resolve_opaque(&mut runtime, 0..source.len(), 257);
        let mut job = M11InlineEmphasisJob::new(opaque).expect("emphasis job");
        while runtime.arena_metrics().reserved_external_payload_bytes == 0 {
            let poll = job.poll(&mut runtime, 257).expect("partial poll");
            assert_eq!(poll.status(), M11InlineEmphasisPollStatus::Pending);
        }
        job.begin_abort().expect("begin abort");
        loop {
            let poll = job.poll_abort(&mut runtime, 1).expect("abort poll");
            assert!(poll.transitions() <= 1);
            if poll.complete() {
                break;
            }
        }
        drop(job);
        drop(autolink_job);
        drop(code_job);
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
        close_runtime(runtime);
    }

    #[test]
    fn scratch_mutation_failure_is_terminal_and_remains_abortable() {
        let source = "*x*";
        let config = DocumentRuntimeConfig {
            arena_limits: ArenaLimits {
                max_live_payload_bytes: 1_024,
                ..ArenaLimits::default()
            },
            ..DocumentRuntimeConfig::default()
        };
        let mut runtime = DocumentRuntime::new(source, config).expect("runtime");
        let (code_job, autolink_job, opaque) = resolve_opaque(&mut runtime, 0..source.len(), 31);
        let mut job = M11InlineEmphasisJob::new(opaque).expect("emphasis job");
        let error = loop {
            match job.poll(&mut runtime, 31) {
                Ok(poll) => assert_eq!(poll.status(), M11InlineEmphasisPollStatus::Pending),
                Err(error) => break error,
            }
        };
        assert!(matches!(error, M11InlineEmphasisError::Scratch(_)));
        assert!(matches!(
            job.poll(&mut runtime, 1),
            Err(M11InlineEmphasisError::InvalidState)
        ));
        job.begin_abort().expect("begin abort");
        loop {
            if job.poll_abort(&mut runtime, 1).expect("abort").complete() {
                break;
            }
        }
        drop(job);
        drop(autolink_job);
        drop(code_job);
        assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
        close_runtime(runtime);
    }

    #[test]
    fn deterministic_paired_delimiter_corpus_matches_local_comrak_oracle() {
        let alphabet = b"*_~ab .";
        let mut state = 0x5eed_cafe_u64;
        for case in 0..1_000 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let len = usize::try_from(state % 48).expect("fits") + 1;
            let mut source = String::with_capacity(len + 2);
            source.push('a');
            for _ in 0..len {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                let index = usize::try_from(state % alphabet.len() as u64).expect("fits");
                source.push(char::from(alphabet[index]));
            }
            source.push('b');

            let result = resolve(&source, 31);
            let actual_candidates = canonical(&result.emphasis);
            let actual = (
                result
                    .emphasis
                    .iter()
                    .filter(|candidate| candidate.kind() == M11EmphasisCandidateKind::Emphasis)
                    .count(),
                result
                    .emphasis
                    .iter()
                    .filter(|candidate| candidate.kind() == M11EmphasisCandidateKind::Strong)
                    .count(),
                result
                    .emphasis
                    .iter()
                    .filter(|candidate| candidate.kind() == M11EmphasisCandidateKind::Strikethrough)
                    .count(),
            );
            let mut options = ComrakOptions::default();
            options.extension.strikethrough = true;
            let html = markdown_to_html(&source, &options);
            let default_html = markdown_to_html(&source, &ComrakOptions::default());
            let expected = (
                html.matches("<em>").count(),
                html.matches("<strong>").count(),
                html.matches("<del>").count(),
            );
            assert_eq!(
                actual, expected,
                "case={case}, source={source:?}, candidates={actual_candidates:?}, html={html:?}, default_html={default_html:?}"
            );
        }
    }

    #[test]
    fn one_mib_dense_delimiters_stay_inside_shared_scratch_ceiling() {
        let source = "*x* ".repeat(256 * 1024);
        assert_eq!(source.len(), 1024 * 1024);
        let result = resolve(&source, M11_INLINE_EMPHASIS_MAX_POLL_TRANSITIONS);
        assert_eq!(result.emphasis.len(), 256 * 1024);
        assert_eq!(result.emphasis_preorder.len(), 256 * 1024);
        assert!(result.remainders.is_empty());
        assert_eq!(
            result
                .emphasis_preorder
                .first()
                .expect("first")
                .relative_range(),
            0..3
        );
        assert_eq!(
            result
                .emphasis_preorder
                .last()
                .expect("last")
                .relative_range(),
            (source.len() as u32 - 4)..(source.len() as u32 - 1)
        );
        assert!(
            result.maximum_retained_scratch_bytes < 64 * 1024 * 1024,
            "retained {} bytes",
            result.maximum_retained_scratch_bytes
        );
    }
}

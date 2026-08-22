//! Fuel-bounded CommonMark angle-autolink resolution.
//!
//! The scanner first retains syntactic angle candidates independently from
//! raw backtick pairing. A second fuelled resolver then walks both opener
//! streams left-to-right: whichever opener occurs first consumes its complete
//! extent, and backtick pairing is repaired after runs swallowed by an earlier
//! angle candidate are skipped. The resulting code/autolink sequence is the
//! sole shielding map used by the whole-leaf hazard and delimiter passes.
//!
//! The resolver is deliberately streaming. It uses one exact range cursor,
//! a fixed-size source window, and engine-admitted radix pages for the
//! retained candidates. It never invokes the full Comrak parser. Angle facts
//! preserve the exact semantic source destination; escaping required by an
//! output format is a consumer concern. Tests use pinned Comrak output only as
//! a differential oracle.

use std::fmt;
use std::ops::Range;

use flark_engine::parser_internal::{
    M11ParserPageError, M11ParserRangeStatus,
    M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS,
    M11_INLINE_PROJECTION_FLAG_CODE_TRIM_ONE_SPACE,
};
use flark_engine::{DocumentRuntime, SourceVersion};

use crate::inline_code::{M11InlineCodeError, M11InlineCodeRuns};
use crate::inline_radix::{
    M11InlineRadixError, M11InlineRadixPages, M11InlineRadixReclaimPoll,
    M11_INLINE_RADIX_MAX_POLL_TRANSITIONS,
};

pub(crate) const M11_INLINE_AUTOLINK_MAX_POLL_TRANSITIONS: usize =
    M11_INLINE_RADIX_MAX_POLL_TRANSITIONS;

const AUTOLINK_PAGE_RECORDS: usize = 128;
const OPAQUE_PAGE_RECORDS: usize = 128;
pub(crate) const M11_INLINE_AUTOLINK_SOURCE_WINDOW_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum M11AngleAutolinkKind {
    #[default]
    Uri,
    Email,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct M11AngleAutolinkCandidate {
    range_start: u32,
    range_end: u32,
    content_start: u32,
    content_end: u32,
    kind: M11AngleAutolinkKind,
    reserved: [u8; 3],
}

impl M11AngleAutolinkCandidate {
    pub(crate) const fn kind(self) -> M11AngleAutolinkKind {
        self.kind
    }

    pub(crate) fn relative_range(self) -> Range<u32> {
        self.range_start..self.range_end
    }

    pub(crate) fn relative_content_range(self) -> Range<u32> {
        self.content_start..self.content_end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineAutolinkPollStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineAutolinkPoll {
    status: M11InlineAutolinkPollStatus,
    transitions: usize,
}

impl M11InlineAutolinkPoll {
    pub(crate) const fn status(self) -> M11InlineAutolinkPollStatus {
        self.status
    }

    pub(crate) const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineAutolinkReleasePoll {
    transitions: usize,
    complete: bool,
}

impl M11InlineAutolinkReleasePoll {
    pub(crate) const fn transitions(self) -> usize {
        self.transitions
    }

    pub(crate) const fn complete(self) -> bool {
        self.complete
    }
}

#[derive(Debug)]
pub(crate) enum M11InlineAutolinkError {
    Source(M11ParserPageError),
    Code(M11InlineCodeError),
    Scratch(M11InlineRadixError),
    ZeroFuel,
    PollLimitExceeded,
    CoordinateOverflow,
    InvalidState,
}

impl fmt::Display for M11InlineAutolinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "inline autolink source failed: {error}"),
            Self::Code(error) => write!(formatter, "inline autolink code fence failed: {error}"),
            Self::Scratch(error) => write!(formatter, "inline autolink scratch failed: {error}"),
            Self::ZeroFuel => formatter.write_str("inline autolink poll requires nonzero fuel"),
            Self::PollLimitExceeded => {
                formatter.write_str("inline autolink poll exceeds its transition limit")
            }
            Self::CoordinateOverflow => {
                formatter.write_str("inline autolink coordinate or counter overflow")
            }
            Self::InvalidState => formatter.write_str("inline autolink job is in an invalid state"),
        }
    }
}

impl std::error::Error for M11InlineAutolinkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Code(error) => Some(error),
            Self::Scratch(error) => Some(error),
            _ => None,
        }
    }
}

impl From<M11ParserPageError> for M11InlineAutolinkError {
    fn from(value: M11ParserPageError) -> Self {
        Self::Source(value)
    }
}

impl From<M11InlineCodeError> for M11InlineAutolinkError {
    fn from(value: M11InlineCodeError) -> Self {
        Self::Code(value)
    }
}

impl From<M11InlineRadixError> for M11InlineAutolinkError {
    fn from(value: M11InlineRadixError) -> Self {
        Self::Scratch(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EmailPhase {
    Local { len: u32 },
    DomainStart,
    Domain { label_len: u8, ends_alnum: bool },
    Invalid,
}

#[derive(Clone, Copy, Debug)]
struct CandidateState {
    start: u32,
    uri_possible: bool,
    uri_scheme_len: u8,
    uri_colon_seen: bool,
    email_phase: EmailPhase,
}

impl CandidateState {
    const fn new(start: u32) -> Self {
        Self {
            start,
            uri_possible: true,
            uri_scheme_len: 0,
            uri_colon_seen: false,
            email_phase: EmailPhase::Local { len: 0 },
        }
    }

    fn observe(&mut self, byte: u8) {
        self.observe_uri(byte);
        self.observe_email(byte);
    }

    fn observe_uri(&mut self, byte: u8) {
        if !self.uri_possible {
            return;
        }
        if self.uri_colon_seen {
            if byte <= 0x20 || matches!(byte, b'<' | b'>') || byte == 0xff {
                self.uri_possible = false;
            }
            return;
        }
        if self.uri_scheme_len == 0 {
            if byte.is_ascii_alphabetic() {
                self.uri_scheme_len = 1;
            } else {
                self.uri_possible = false;
            }
            return;
        }
        if byte == b':' {
            if self.uri_scheme_len >= 2 {
                self.uri_colon_seen = true;
            } else {
                self.uri_possible = false;
            }
            return;
        }
        if byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.') {
            if self.uri_scheme_len == 32 {
                self.uri_possible = false;
            } else {
                self.uri_scheme_len += 1;
            }
        } else {
            self.uri_possible = false;
        }
    }

    fn observe_email(&mut self, byte: u8) {
        self.email_phase = match self.email_phase {
            EmailPhase::Local { len } => {
                if byte == b'@' && len != 0 {
                    EmailPhase::DomainStart
                } else if email_local_byte(byte) {
                    EmailPhase::Local {
                        len: len.saturating_add(1),
                    }
                } else {
                    EmailPhase::Invalid
                }
            }
            EmailPhase::DomainStart => {
                if byte.is_ascii_alphanumeric() {
                    EmailPhase::Domain {
                        label_len: 1,
                        ends_alnum: true,
                    }
                } else {
                    EmailPhase::Invalid
                }
            }
            EmailPhase::Domain {
                label_len,
                ends_alnum,
            } => {
                if byte == b'.' && ends_alnum {
                    EmailPhase::DomainStart
                } else if byte.is_ascii_alphanumeric() && label_len < 63 {
                    EmailPhase::Domain {
                        label_len: label_len + 1,
                        ends_alnum: true,
                    }
                } else if byte == b'-' && label_len < 63 {
                    EmailPhase::Domain {
                        label_len: label_len + 1,
                        ends_alnum: false,
                    }
                } else {
                    EmailPhase::Invalid
                }
            }
            EmailPhase::Invalid => EmailPhase::Invalid,
        };
    }

    const fn finish(self) -> Option<M11AngleAutolinkKind> {
        if self.uri_possible && self.uri_colon_seen {
            return Some(M11AngleAutolinkKind::Uri);
        }
        if matches!(
            self.email_phase,
            EmailPhase::Domain {
                ends_alnum: true,
                ..
            }
        ) {
            return Some(M11AngleAutolinkKind::Email);
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AutolinkPhase {
    Scanning,
    Complete,
    Faulted,
    Aborting,
    Aborted,
    Transferred,
}

/// Streaming resolver over one exact inline range.
pub(crate) struct M11InlineAutolinkJob {
    source: SourceVersion,
    source_range: Range<u32>,
    cursor: flark_engine::parser_internal::M11ParserRangeCursor,
    pages: Option<M11InlineRadixPages<M11AngleAutolinkCandidate, AUTOLINK_PAGE_RECORDS>>,
    reclaim_started: bool,
    window: [u8; M11_INLINE_AUTOLINK_SOURCE_WINDOW_BYTES],
    window_position: usize,
    window_len: usize,
    source_offset: u32,
    source_eof: bool,
    candidate: Option<CandidateState>,
    pending_candidate: Option<M11AngleAutolinkCandidate>,
    candidate_count: u32,
    phase: AutolinkPhase,
}

impl fmt::Debug for M11InlineAutolinkJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11InlineAutolinkJob")
            .field("source", &self.source)
            .field("source_range", &self.source_range)
            .field("source_offset", &self.source_offset)
            .field("candidate_count", &self.candidate_count)
            .field("phase", &self.phase)
            .finish_non_exhaustive()
    }
}

impl M11InlineAutolinkJob {
    pub(crate) fn new(
        runtime: &DocumentRuntime,
        code: &M11InlineCodeRuns,
    ) -> Result<Self, M11InlineAutolinkError> {
        code.validate_source(runtime)?;
        let source = code.source();
        let source_range = code.source_range();
        let pages = M11InlineRadixPages::new(source)?;
        let cursor = code.source_cursor(runtime)?;
        Ok(Self {
            source,
            source_range,
            cursor,
            pages: Some(pages),
            reclaim_started: false,
            window: [0; M11_INLINE_AUTOLINK_SOURCE_WINDOW_BYTES],
            window_position: 0,
            window_len: 0,
            source_offset: 0,
            source_eof: false,
            candidate: None,
            pending_candidate: None,
            candidate_count: 0,
            phase: AutolinkPhase::Scanning,
        })
    }

    pub(crate) fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineAutolinkPoll, M11InlineAutolinkError> {
        validate_fuel(fuel)?;
        if self.phase == AutolinkPhase::Complete {
            return Ok(M11InlineAutolinkPoll {
                status: M11InlineAutolinkPollStatus::Complete,
                transitions: 0,
            });
        }
        if self.phase != AutolinkPhase::Scanning {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        let mut transitions = 0;
        while transitions < fuel {
            let step = if self.pending_candidate.is_some() {
                self.poll_pending_candidate(runtime, &mut transitions)
            } else if self.window_position < self.window_len {
                self.poll_byte(runtime, &mut transitions)
            } else if self.source_eof {
                self.candidate = None;
                self.phase = AutolinkPhase::Complete;
                transitions += 1;
                Ok(())
            } else {
                self.poll_source(fuel, &mut transitions)
            };
            if let Err(error) = step {
                self.cursor.cancel();
                self.phase = AutolinkPhase::Faulted;
                return Err(error);
            }
            if self.phase == AutolinkPhase::Complete {
                return Ok(M11InlineAutolinkPoll {
                    status: M11InlineAutolinkPollStatus::Complete,
                    transitions,
                });
            }
        }
        Ok(M11InlineAutolinkPoll {
            status: M11InlineAutolinkPollStatus::Pending,
            transitions,
        })
    }

    fn poll_source(
        &mut self,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineAutolinkError> {
        let poll = self.cursor.poll(fuel - *transitions, &mut self.window)?;
        self.window_position = 0;
        self.window_len = poll.bytes_read();
        self.source_eof = poll.status() == M11ParserRangeStatus::Complete;
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
        if self.window_len == 0 && !self.source_eof {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        Ok(())
    }

    fn poll_byte(
        &mut self,
        _runtime: &mut DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11InlineAutolinkError> {
        let byte = self.window[self.window_position];
        self.window_position += 1;
        let offset = self.source_offset;
        self.source_offset = self
            .source_offset
            .checked_add(1)
            .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
        *transitions += 1;

        if byte == b'<' {
            self.candidate = Some(CandidateState::new(offset));
            return Ok(());
        }
        let Some(mut candidate) = self.candidate else {
            return Ok(());
        };
        if byte == b'>' {
            self.candidate = None;
            if let Some(kind) = candidate.finish() {
                let content_start = candidate
                    .start
                    .checked_add(1)
                    .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
                self.pending_candidate = Some(M11AngleAutolinkCandidate {
                    range_start: candidate.start,
                    range_end: self.source_offset,
                    content_start,
                    content_end: offset,
                    kind,
                    reserved: [0; 3],
                });
            }
            return Ok(());
        }

        candidate.observe(byte);
        self.candidate = Some(candidate);
        Ok(())
    }

    fn poll_pending_candidate(
        &mut self,
        runtime: &mut DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11InlineAutolinkError> {
        let candidate = self
            .pending_candidate
            .ok_or(M11InlineAutolinkError::InvalidState)?;
        self.pages
            .as_mut()
            .ok_or(M11InlineAutolinkError::InvalidState)?
            .set(
                runtime,
                usize::try_from(self.candidate_count)
                    .map_err(|_| M11InlineAutolinkError::CoordinateOverflow)?,
                candidate,
            )?;
        self.candidate_count = self
            .candidate_count
            .checked_add(1)
            .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
        self.pending_candidate = None;
        *transitions += 1;
        Ok(())
    }

    pub(crate) fn take_output(&mut self) -> Option<M11AngleAutolinkCandidates> {
        if self.phase != AutolinkPhase::Complete {
            return None;
        }
        let pages = self.pages.take()?;
        self.phase = AutolinkPhase::Transferred;
        Some(M11AngleAutolinkCandidates {
            source: self.source,
            source_range: self.source_range.clone(),
            count: self.candidate_count,
            pages: Some(pages),
            reclaim_started: false,
        })
    }

    pub(crate) fn begin_abort(&mut self) -> Result<(), M11InlineAutolinkError> {
        if matches!(
            self.phase,
            AutolinkPhase::Transferred | AutolinkPhase::Aborted
        ) {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        if self.phase == AutolinkPhase::Aborting {
            return Ok(());
        }
        self.cursor.cancel();
        if self.pages.is_some() && !self.reclaim_started {
            self.pages
                .as_mut()
                .ok_or(M11InlineAutolinkError::InvalidState)?
                .begin_reclaim()?;
            self.reclaim_started = true;
        }
        self.phase = AutolinkPhase::Aborting;
        Ok(())
    }

    pub(crate) fn poll_abort(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineAutolinkReleasePoll, M11InlineAutolinkError> {
        validate_fuel(fuel)?;
        if self.phase != AutolinkPhase::Aborting || !self.reclaim_started {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        let Some(pages) = self.pages.as_mut() else {
            self.phase = AutolinkPhase::Aborted;
            return Ok(M11InlineAutolinkReleasePoll {
                transitions: 0,
                complete: true,
            });
        };
        let poll = pages.poll_reclaim(runtime, fuel)?;
        if poll.complete() {
            drop(self.pages.take());
            self.phase = AutolinkPhase::Aborted;
        }
        Ok(M11InlineAutolinkReleasePoll {
            transitions: poll.transitions(),
            complete: poll.complete(),
        })
    }
}

impl Drop for M11InlineAutolinkJob {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                matches!(
                    self.phase,
                    AutolinkPhase::Aborted | AutolinkPhase::Transferred
                ),
                "inline autolink jobs require output transfer or explicit fuelled abort"
            );
        }
    }
}

pub(crate) struct M11AngleAutolinkCandidates {
    source: SourceVersion,
    source_range: Range<u32>,
    count: u32,
    pages: Option<M11InlineRadixPages<M11AngleAutolinkCandidate, AUTOLINK_PAGE_RECORDS>>,
    reclaim_started: bool,
}

impl M11AngleAutolinkCandidates {
    pub(crate) const fn source(&self) -> SourceVersion {
        self.source
    }

    pub(crate) fn source_range(&self) -> Range<u32> {
        self.source_range.clone()
    }

    pub(crate) const fn len(&self) -> u32 {
        self.count
    }

    pub(crate) fn candidate(
        &self,
        index: u32,
    ) -> Result<Option<M11AngleAutolinkCandidate>, M11InlineAutolinkError> {
        if index >= self.count {
            return Ok(None);
        }
        Ok(Some(
            self.pages
                .as_ref()
                .ok_or(M11InlineAutolinkError::InvalidState)?
                .get(
                    usize::try_from(index)
                        .map_err(|_| M11InlineAutolinkError::CoordinateOverflow)?,
                )?
                .ok_or(M11InlineAutolinkError::InvalidState)?,
        ))
    }

    pub(crate) fn begin_release(&mut self) -> Result<(), M11InlineAutolinkError> {
        if self.reclaim_started {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        self.pages
            .as_mut()
            .ok_or(M11InlineAutolinkError::InvalidState)?
            .begin_reclaim()?;
        self.reclaim_started = true;
        Ok(())
    }

    pub(crate) fn poll_release(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineAutolinkReleasePoll, M11InlineAutolinkError> {
        validate_fuel(fuel)?;
        if !self.reclaim_started {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        let poll: M11InlineRadixReclaimPoll = self
            .pages
            .as_mut()
            .ok_or(M11InlineAutolinkError::InvalidState)?
            .poll_reclaim(runtime, fuel)?;
        if poll.complete() {
            drop(self.pages.take());
        }
        Ok(M11InlineAutolinkReleasePoll {
            transitions: poll.transitions(),
            complete: poll.complete(),
        })
    }
}

impl Drop for M11AngleAutolinkCandidates {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.pages.is_none(),
                "angle-autolink candidates require explicit fuelled release"
            );
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum M11InlineOpaqueKind {
    #[default]
    Code,
    AutolinkUri,
    AutolinkEmail,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct M11InlineOpaqueCandidate {
    range_start: u32,
    range_end: u32,
    content_start: u32,
    content_end: u32,
    kind: M11InlineOpaqueKind,
    flags: u8,
    reserved: [u8; 2],
}

impl M11InlineOpaqueCandidate {
    pub(crate) fn new_bare_autolink(
        kind: M11InlineOpaqueKind,
        flags: u8,
        relative_range: Range<u32>,
    ) -> Result<Self, M11InlineAutolinkError> {
        if !matches!(
            kind,
            M11InlineOpaqueKind::AutolinkUri | M11InlineOpaqueKind::AutolinkEmail
        ) || relative_range.start >= relative_range.end
        {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        Ok(Self {
            range_start: relative_range.start,
            range_end: relative_range.end,
            content_start: relative_range.start,
            content_end: relative_range.end,
            kind,
            flags,
            reserved: [0; 2],
        })
    }

    pub(crate) const fn kind(self) -> M11InlineOpaqueKind {
        self.kind
    }

    pub(crate) const fn flags(self) -> u8 {
        self.flags
    }

    pub(crate) fn relative_range(self) -> Range<u32> {
        self.range_start..self.range_end
    }

    pub(crate) fn relative_content_range(self) -> Range<u32> {
        self.content_start..self.content_end
    }
}

#[derive(Clone, Copy, Debug)]
struct OpaqueFlagAccumulator {
    record: M11InlineOpaqueCandidate,
    observed: bool,
    first_is_space: bool,
    last_is_space: bool,
    has_non_space: bool,
    normalize_line_endings: bool,
}

impl OpaqueFlagAccumulator {
    const fn new(record: M11InlineOpaqueCandidate) -> Self {
        Self {
            record,
            observed: false,
            first_is_space: false,
            last_is_space: false,
            has_non_space: false,
            normalize_line_endings: false,
        }
    }

    fn observe(&mut self, byte: u8) {
        let is_space = matches!(byte, b' ' | b'\r' | b'\n');
        if !self.observed {
            self.first_is_space = is_space;
            self.observed = true;
        }
        self.last_is_space = is_space;
        self.has_non_space |= !is_space;
        self.normalize_line_endings |= matches!(byte, b'\r' | b'\n');
    }

    fn finish(mut self) -> M11InlineOpaqueCandidate {
        self.record.flags = (u8::from(self.normalize_line_endings)
            * M11_INLINE_PROJECTION_FLAG_CODE_NORMALIZE_LINE_ENDINGS)
            | (u8::from(self.first_is_space && self.last_is_space && self.has_non_space)
                * M11_INLINE_PROJECTION_FLAG_CODE_TRIM_ONE_SPACE);
        self.record
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineOpaquePollStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineOpaquePoll {
    status: M11InlineOpaquePollStatus,
    transitions: usize,
}

impl M11InlineOpaquePoll {
    pub(crate) const fn status(self) -> M11InlineOpaquePollStatus {
        self.status
    }

    pub(crate) const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OpaqueResolvePhase {
    Merge,
    Flags,
    Complete,
    Faulted,
    Aborting,
    Aborted,
    Transferred,
}

/// Fuelled left-to-right ownership over raw backtick runs and syntactic
/// angle-autolink candidates.
///
/// The winner at the earlier opener consumes its complete extent. Pairing
/// code from raw runs here, after skipping runs consumed by an earlier
/// autolink, is what makes later code spans reflect the integrated grammar
/// rather than an independently resolved code-only pass.
pub(crate) struct M11InlineOpaqueResolveJob {
    source: SourceVersion,
    source_range: Range<u32>,
    code: Option<M11InlineCodeRuns>,
    autolinks: Option<M11AngleAutolinkCandidates>,
    resolved: Option<M11InlineRadixPages<M11InlineOpaqueCandidate, OPAQUE_PAGE_RECORDS>>,
    flags_cursor: flark_engine::parser_internal::M11ParserRangeCursor,
    run_index: u32,
    autolink_index: u32,
    resolved_count: u32,
    consumed_end: u32,
    flag_index: u32,
    flag_current: Option<OpaqueFlagAccumulator>,
    flag_window: [u8; M11_INLINE_AUTOLINK_SOURCE_WINDOW_BYTES],
    flag_window_position: usize,
    flag_window_len: usize,
    flag_source_offset: u32,
    flag_source_eof: bool,
    code_release_started: bool,
    autolink_release_started: bool,
    resolved_reclaim_started: bool,
    phase: OpaqueResolvePhase,
}

impl M11InlineOpaqueResolveJob {
    /// Validates and prepares every fallible resource before transferring
    /// either move-only owner out of its current container.
    ///
    /// On error, `code` and `autolink_job` remain untouched and can still be
    /// explicitly released by their controller.
    pub(crate) fn take_new(
        runtime: &DocumentRuntime,
        code: &mut Option<M11InlineCodeRuns>,
        autolink_job: &mut M11InlineAutolinkJob,
    ) -> Result<Self, M11InlineAutolinkError> {
        let code_ref = code.as_ref().ok_or(M11InlineAutolinkError::InvalidState)?;
        if autolink_job.phase != AutolinkPhase::Complete || autolink_job.pages.is_none() {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        code_ref.validate_source(runtime)?;
        if code_ref.source() != autolink_job.source
            || code_ref.source_range() != autolink_job.source_range
        {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        let source = code_ref.source();
        let source_range = code_ref.source_range();
        let resolved = M11InlineRadixPages::new(source)?;
        let flags_cursor = code_ref.source_cursor(runtime)?;

        // Every fallible operation is complete. These transfers cannot fail
        // after the state checks above.
        let code = code
            .take()
            .expect("validated code owner must remain present");
        let autolinks = autolink_job
            .take_output()
            .expect("complete autolink job must transfer its output");
        Ok(Self {
            source,
            source_range,
            code: Some(code),
            autolinks: Some(autolinks),
            resolved: Some(resolved),
            flags_cursor,
            run_index: 0,
            autolink_index: 0,
            resolved_count: 0,
            consumed_end: 0,
            flag_index: 0,
            flag_current: None,
            flag_window: [0; M11_INLINE_AUTOLINK_SOURCE_WINDOW_BYTES],
            flag_window_position: 0,
            flag_window_len: 0,
            flag_source_offset: 0,
            flag_source_eof: false,
            code_release_started: false,
            autolink_release_started: false,
            resolved_reclaim_started: false,
            phase: OpaqueResolvePhase::Merge,
        })
    }

    pub(crate) fn poll(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineOpaquePoll, M11InlineAutolinkError> {
        validate_fuel(fuel)?;
        if self.phase == OpaqueResolvePhase::Complete {
            return Ok(M11InlineOpaquePoll {
                status: M11InlineOpaquePollStatus::Complete,
                transitions: 0,
            });
        }
        if !matches!(
            self.phase,
            OpaqueResolvePhase::Merge | OpaqueResolvePhase::Flags
        ) {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        self.code_ref()?.validate_source(runtime)?;
        let mut transitions = 0;
        while transitions < fuel {
            let result = match self.phase {
                OpaqueResolvePhase::Merge => self.poll_merge(runtime, &mut transitions),
                OpaqueResolvePhase::Flags => self.poll_flags(runtime, fuel, &mut transitions),
                OpaqueResolvePhase::Complete => break,
                _ => Err(M11InlineAutolinkError::InvalidState),
            };
            if let Err(error) = result {
                self.flags_cursor.cancel();
                self.phase = OpaqueResolvePhase::Faulted;
                return Err(error);
            }
        }
        Ok(M11InlineOpaquePoll {
            status: if self.phase == OpaqueResolvePhase::Complete {
                M11InlineOpaquePollStatus::Complete
            } else {
                M11InlineOpaquePollStatus::Pending
            },
            transitions,
        })
    }

    fn poll_merge(
        &mut self,
        runtime: &mut DocumentRuntime,
        transitions: &mut usize,
    ) -> Result<(), M11InlineAutolinkError> {
        let run = self.code_ref()?.raw_run(self.run_index)?;
        if let Some(run) = run {
            if run.raw_start() < self.consumed_end {
                self.run_index = self
                    .run_index
                    .checked_add(1)
                    .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
                *transitions += 1;
                return Ok(());
            }
            if run.top_level_opener()?.is_none() {
                self.run_index = self
                    .run_index
                    .checked_add(1)
                    .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
                *transitions += 1;
                return Ok(());
            }
        }
        let autolink = self.autolinks_ref()?.candidate(self.autolink_index)?;
        if let Some(autolink) = autolink {
            if autolink.range_start < self.consumed_end {
                self.autolink_index = self
                    .autolink_index
                    .checked_add(1)
                    .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
                *transitions += 1;
                return Ok(());
            }
        }
        let Some(choice_is_angle) = (match (run, autolink) {
            (Some(run), Some(angle)) => {
                let (run_start, _) = run
                    .top_level_opener()?
                    .ok_or(M11InlineAutolinkError::InvalidState)?;
                if run_start == angle.range_start {
                    return Err(M11InlineAutolinkError::InvalidState);
                }
                Some(angle.range_start < run_start)
            }
            (Some(_), None) => Some(false),
            (None, Some(_)) => Some(true),
            (None, None) => None,
        }) else {
            self.phase = OpaqueResolvePhase::Flags;
            *transitions += 1;
            return Ok(());
        };

        if choice_is_angle {
            let angle = autolink.ok_or(M11InlineAutolinkError::InvalidState)?;
            let kind = match angle.kind {
                M11AngleAutolinkKind::Uri => M11InlineOpaqueKind::AutolinkUri,
                M11AngleAutolinkKind::Email => M11InlineOpaqueKind::AutolinkEmail,
            };
            self.push_resolved(
                runtime,
                M11InlineOpaqueCandidate {
                    range_start: angle.range_start,
                    range_end: angle.range_end,
                    content_start: angle.content_start,
                    content_end: angle.content_end,
                    kind,
                    flags: 0,
                    reserved: [0; 2],
                },
            )?;
            self.autolink_index = self
                .autolink_index
                .checked_add(1)
                .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
            self.consumed_end = angle.range_end;
            *transitions += 1;
            return Ok(());
        }

        let opener = run.ok_or(M11InlineAutolinkError::InvalidState)?;
        let (range_start, opener_len) = opener
            .top_level_opener()?
            .ok_or(M11InlineAutolinkError::InvalidState)?;
        let Some(closer_index) = opener.next_closer() else {
            self.run_index = self
                .run_index
                .checked_add(1)
                .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
            *transitions += 1;
            return Ok(());
        };
        if closer_index <= self.run_index {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        let closer = self
            .code_ref()?
            .raw_run(closer_index)?
            .ok_or(M11InlineAutolinkError::InvalidState)?;
        let content_start = opener.raw_end()?;
        if closer.len() != opener_len || content_start > closer.raw_start() {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        let range_end = closer.raw_end()?;
        self.push_resolved(
            runtime,
            M11InlineOpaqueCandidate {
                range_start,
                range_end,
                content_start,
                content_end: closer.raw_start(),
                kind: M11InlineOpaqueKind::Code,
                flags: 0,
                reserved: [0; 2],
            },
        )?;
        self.run_index = closer_index
            .checked_add(1)
            .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
        self.consumed_end = range_end;
        *transitions += 1;
        Ok(())
    }

    fn poll_flags(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineAutolinkError> {
        if self.flag_current.is_none() {
            if self.flag_index == self.resolved_count {
                self.flags_cursor.cancel();
                self.phase = OpaqueResolvePhase::Complete;
                *transitions += 1;
                return Ok(());
            }
            let record = self.resolved_candidate(self.flag_index)?;
            if record.kind != M11InlineOpaqueKind::Code {
                self.flag_index = self
                    .flag_index
                    .checked_add(1)
                    .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
                *transitions += 1;
                return Ok(());
            }
            if self.flag_source_offset > record.content_start {
                return Err(M11InlineAutolinkError::InvalidState);
            }
            self.flag_current = Some(OpaqueFlagAccumulator::new(record));
            *transitions += 1;
            return Ok(());
        }

        let content_end = self
            .flag_current
            .as_ref()
            .ok_or(M11InlineAutolinkError::InvalidState)?
            .record
            .content_end;
        if self.flag_source_offset == content_end {
            let record = self
                .flag_current
                .take()
                .ok_or(M11InlineAutolinkError::InvalidState)?
                .finish();
            let index = self.flag_index;
            self.resolved_mut()?.set(
                runtime,
                usize::try_from(index).map_err(|_| M11InlineAutolinkError::CoordinateOverflow)?,
                record,
            )?;
            self.flag_index = index
                .checked_add(1)
                .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
            *transitions += 1;
            return Ok(());
        }
        if self.flag_source_offset > content_end {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        if self.flag_window_position == self.flag_window_len {
            if self.flag_source_eof {
                return Err(M11InlineAutolinkError::InvalidState);
            }
            let poll = self
                .flags_cursor
                .poll(fuel - *transitions, &mut self.flag_window)?;
            self.flag_window_position = 0;
            self.flag_window_len = poll.bytes_read();
            self.flag_source_eof = poll.status() == M11ParserRangeStatus::Complete;
            *transitions = transitions
                .checked_add(poll.transitions())
                .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
            if self.flag_window_len == 0 && self.flag_source_eof {
                return Err(M11InlineAutolinkError::InvalidState);
            }
            return Ok(());
        }
        let byte_offset = self.flag_source_offset;
        let byte = self.flag_window[self.flag_window_position];
        self.flag_window_position += 1;
        self.flag_source_offset = self
            .flag_source_offset
            .checked_add(1)
            .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
        let accumulator = self
            .flag_current
            .as_mut()
            .ok_or(M11InlineAutolinkError::InvalidState)?;
        if byte_offset >= accumulator.record.content_start {
            accumulator.observe(byte);
        }
        *transitions += 1;
        Ok(())
    }

    fn push_resolved(
        &mut self,
        runtime: &mut DocumentRuntime,
        candidate: M11InlineOpaqueCandidate,
    ) -> Result<(), M11InlineAutolinkError> {
        let index = self.resolved_count;
        self.resolved_mut()?.set(
            runtime,
            usize::try_from(index).map_err(|_| M11InlineAutolinkError::CoordinateOverflow)?,
            candidate,
        )?;
        self.resolved_count = index
            .checked_add(1)
            .ok_or(M11InlineAutolinkError::CoordinateOverflow)?;
        Ok(())
    }

    fn resolved_candidate(
        &self,
        index: u32,
    ) -> Result<M11InlineOpaqueCandidate, M11InlineAutolinkError> {
        self.resolved
            .as_ref()
            .ok_or(M11InlineAutolinkError::InvalidState)?
            .get(usize::try_from(index).map_err(|_| M11InlineAutolinkError::CoordinateOverflow)?)?
            .ok_or(M11InlineAutolinkError::InvalidState)
    }

    fn code_ref(&self) -> Result<&M11InlineCodeRuns, M11InlineAutolinkError> {
        self.code
            .as_ref()
            .ok_or(M11InlineAutolinkError::InvalidState)
    }

    fn autolinks_ref(&self) -> Result<&M11AngleAutolinkCandidates, M11InlineAutolinkError> {
        self.autolinks
            .as_ref()
            .ok_or(M11InlineAutolinkError::InvalidState)
    }

    fn resolved_mut(
        &mut self,
    ) -> Result<
        &mut M11InlineRadixPages<M11InlineOpaqueCandidate, OPAQUE_PAGE_RECORDS>,
        M11InlineAutolinkError,
    > {
        self.resolved
            .as_mut()
            .ok_or(M11InlineAutolinkError::InvalidState)
    }

    pub(crate) fn take_output(&mut self) -> Option<M11InlineOpaqueCandidates> {
        if self.phase != OpaqueResolvePhase::Complete {
            return None;
        }
        let code = self.code.take()?;
        let autolinks = self.autolinks.take()?;
        let resolved = self.resolved.take()?;
        self.phase = OpaqueResolvePhase::Transferred;
        Some(M11InlineOpaqueCandidates {
            source: self.source,
            source_range: self.source_range.clone(),
            count: self.resolved_count,
            code: Some(code),
            autolinks: Some(autolinks),
            resolved: Some(resolved),
            augmented: None,
            code_release_started: false,
            autolink_release_started: false,
            resolved_reclaim_started: false,
            release_complete: false,
        })
    }

    pub(crate) fn begin_abort(&mut self) -> Result<(), M11InlineAutolinkError> {
        if matches!(
            self.phase,
            OpaqueResolvePhase::Transferred | OpaqueResolvePhase::Aborted
        ) {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        if self.phase == OpaqueResolvePhase::Aborting {
            return Ok(());
        }
        self.flags_cursor.cancel();
        self.code
            .as_mut()
            .ok_or(M11InlineAutolinkError::InvalidState)?
            .begin_release()?;
        self.code_release_started = true;
        self.autolinks
            .as_mut()
            .ok_or(M11InlineAutolinkError::InvalidState)?
            .begin_release()?;
        self.autolink_release_started = true;
        self.resolved_mut()?.begin_reclaim()?;
        self.resolved_reclaim_started = true;
        self.phase = OpaqueResolvePhase::Aborting;
        Ok(())
    }

    pub(crate) fn poll_abort(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineAutolinkReleasePoll, M11InlineAutolinkError> {
        validate_fuel(fuel)?;
        if self.phase != OpaqueResolvePhase::Aborting
            || !self.code_release_started
            || !self.autolink_release_started
            || !self.resolved_reclaim_started
        {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        let mut transitions = 0;
        while transitions < fuel {
            if let Some(resolved) = self.resolved.as_mut() {
                let poll = resolved.poll_reclaim(runtime, fuel - transitions)?;
                transitions += poll.transitions();
                if poll.complete() {
                    drop(self.resolved.take());
                    continue;
                }
                return Ok(M11InlineAutolinkReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            if let Some(autolinks) = self.autolinks.as_mut() {
                let poll = autolinks.poll_release(runtime, fuel - transitions)?;
                transitions += poll.transitions();
                if poll.complete() {
                    drop(self.autolinks.take());
                    continue;
                }
                return Ok(M11InlineAutolinkReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            if let Some(code) = self.code.as_mut() {
                let poll = code.poll_release(runtime, fuel - transitions)?;
                transitions += poll.transitions();
                if poll.complete() {
                    drop(self.code.take());
                    continue;
                }
                return Ok(M11InlineAutolinkReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            self.phase = OpaqueResolvePhase::Aborted;
            return Ok(M11InlineAutolinkReleasePoll {
                transitions,
                complete: true,
            });
        }
        Ok(M11InlineAutolinkReleasePoll {
            transitions,
            complete: false,
        })
    }
}

impl Drop for M11InlineOpaqueResolveJob {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                matches!(
                    self.phase,
                    OpaqueResolvePhase::Aborted | OpaqueResolvePhase::Transferred
                ),
                "inline opaque resolver requires output transfer or explicit fuelled abort"
            );
        }
    }
}

/// Move-only, source-ordered opaque ownership shared by hazard, emphasis, and
/// final fact emission.
pub(crate) struct M11InlineOpaqueCandidates {
    source: SourceVersion,
    source_range: Range<u32>,
    count: u32,
    code: Option<M11InlineCodeRuns>,
    autolinks: Option<M11AngleAutolinkCandidates>,
    resolved: Option<M11InlineRadixPages<M11InlineOpaqueCandidate, OPAQUE_PAGE_RECORDS>>,
    augmented: Option<Vec<M11InlineOpaqueCandidate>>,
    code_release_started: bool,
    autolink_release_started: bool,
    resolved_reclaim_started: bool,
    release_complete: bool,
}

impl M11InlineOpaqueCandidates {
    pub(crate) const fn source(&self) -> SourceVersion {
        self.source
    }

    pub(crate) fn source_range(&self) -> Range<u32> {
        self.source_range.clone()
    }

    pub(crate) const fn len(&self) -> u32 {
        self.count
    }

    pub(crate) fn candidate(
        &self,
        index: u32,
    ) -> Result<Option<M11InlineOpaqueCandidate>, M11InlineAutolinkError> {
        if index >= self.count {
            return Ok(None);
        }
        if let Some(augmented) = self.augmented.as_ref() {
            return Ok(Some(
                *augmented
                    .get(
                        usize::try_from(index)
                            .map_err(|_| M11InlineAutolinkError::CoordinateOverflow)?,
                    )
                    .ok_or(M11InlineAutolinkError::InvalidState)?,
            ));
        }
        Ok(Some(
            self.resolved
                .as_ref()
                .ok_or(M11InlineAutolinkError::InvalidState)?
                .get(
                    usize::try_from(index)
                        .map_err(|_| M11InlineAutolinkError::CoordinateOverflow)?,
                )?
                .ok_or(M11InlineAutolinkError::InvalidState)?,
        ))
    }

    pub(crate) fn install_augmented(
        &mut self,
        candidates: Vec<M11InlineOpaqueCandidate>,
    ) -> Result<(), M11InlineAutolinkError> {
        if self.augmented.is_some() || self.release_complete {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        let mut previous: Option<Range<u32>> = None;
        for candidate in &candidates {
            let range = candidate.relative_range();
            if range.start >= range.end
                || previous.as_ref().is_some_and(|prior| {
                    prior.start > range.start
                        || (prior.start == range.start && prior.end < range.end)
                        || prior.end > range.start
                })
            {
                return Err(M11InlineAutolinkError::InvalidState);
            }
            previous = Some(range);
        }
        self.count = u32::try_from(candidates.len())
            .map_err(|_| M11InlineAutolinkError::CoordinateOverflow)?;
        self.augmented = Some(candidates);
        Ok(())
    }

    pub(crate) fn validate_source(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<(), M11InlineAutolinkError> {
        self.code
            .as_ref()
            .ok_or(M11InlineAutolinkError::InvalidState)?
            .validate_source(runtime)?;
        Ok(())
    }

    pub(crate) fn source_cursor(
        &self,
        runtime: &DocumentRuntime,
    ) -> Result<flark_engine::parser_internal::M11ParserRangeCursor, M11InlineAutolinkError> {
        Ok(self
            .code
            .as_ref()
            .ok_or(M11InlineAutolinkError::InvalidState)?
            .source_cursor(runtime)?)
    }

    pub(crate) fn source_authority(
        &self,
    ) -> Result<&flark_engine::parser_internal::M11ParserSourceRangeAuthority, M11InlineAutolinkError>
    {
        Ok(self
            .code
            .as_ref()
            .ok_or(M11InlineAutolinkError::InvalidState)?
            .source_authority()?)
    }

    pub(crate) fn begin_release(&mut self) -> Result<(), M11InlineAutolinkError> {
        if self.release_complete
            || self.code_release_started
            || self.autolink_release_started
            || self.resolved_reclaim_started
        {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        drop(self.augmented.take());
        self.resolved
            .as_mut()
            .ok_or(M11InlineAutolinkError::InvalidState)?
            .begin_reclaim()?;
        self.resolved_reclaim_started = true;
        self.autolinks
            .as_mut()
            .ok_or(M11InlineAutolinkError::InvalidState)?
            .begin_release()?;
        self.autolink_release_started = true;
        self.code
            .as_mut()
            .ok_or(M11InlineAutolinkError::InvalidState)?
            .begin_release()?;
        self.code_release_started = true;
        Ok(())
    }

    pub(crate) fn poll_release(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<M11InlineAutolinkReleasePoll, M11InlineAutolinkError> {
        validate_fuel(fuel)?;
        if !self.code_release_started
            || !self.autolink_release_started
            || !self.resolved_reclaim_started
        {
            return Err(M11InlineAutolinkError::InvalidState);
        }
        if self.release_complete {
            return Ok(M11InlineAutolinkReleasePoll {
                transitions: 0,
                complete: true,
            });
        }
        let mut transitions = 0;
        while transitions < fuel {
            if let Some(resolved) = self.resolved.as_mut() {
                let poll = resolved.poll_reclaim(runtime, fuel - transitions)?;
                transitions += poll.transitions();
                if poll.complete() {
                    drop(self.resolved.take());
                    continue;
                }
                return Ok(M11InlineAutolinkReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            if let Some(autolinks) = self.autolinks.as_mut() {
                let poll = autolinks.poll_release(runtime, fuel - transitions)?;
                transitions += poll.transitions();
                if poll.complete() {
                    drop(self.autolinks.take());
                    continue;
                }
                return Ok(M11InlineAutolinkReleasePoll {
                    transitions,
                    complete: false,
                });
            }
            let code = self
                .code
                .as_mut()
                .ok_or(M11InlineAutolinkError::InvalidState)?;
            let poll = code.poll_release(runtime, fuel - transitions)?;
            transitions += poll.transitions();
            if poll.complete() {
                self.release_complete = true;
                return Ok(M11InlineAutolinkReleasePoll {
                    transitions,
                    complete: true,
                });
            }
            return Ok(M11InlineAutolinkReleasePoll {
                transitions,
                complete: false,
            });
        }
        Ok(M11InlineAutolinkReleasePoll {
            transitions,
            complete: false,
        })
    }

    pub(crate) fn take_source_authority(
        &mut self,
    ) -> Option<flark_engine::parser_internal::M11ParserSourceRangeAuthority> {
        if !self.release_complete || self.resolved.is_some() || self.autolinks.is_some() {
            return None;
        }
        let authority = self.code.as_mut()?.take_source_authority()?;
        drop(self.code.take());
        Some(authority)
    }
}

impl Drop for M11InlineOpaqueCandidates {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.resolved.is_none()
                    && self.autolinks.is_none()
                    && (self.code.is_none() || self.release_complete),
                "opaque inline candidates require explicit fuelled release"
            );
        }
    }
}

const fn email_local_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'.' | b'!'
                | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'/'
                | b'='
                | b'?'
                | b'^'
                | b'_'
                | b'`'
                | b'{'
                | b'|'
                | b'}'
                | b'~'
                | b'-'
        )
}

fn validate_fuel(fuel: usize) -> Result<(), M11InlineAutolinkError> {
    if fuel == 0 {
        return Err(M11InlineAutolinkError::ZeroFuel);
    }
    if fuel > M11_INLINE_AUTOLINK_MAX_POLL_TRANSITIONS {
        return Err(M11InlineAutolinkError::PollLimitExceeded);
    }
    Ok(())
}

//! Strict GFM bare-autolink recognition over a bounded UTF-8 token.
//!
//! This module owns the grammar decisions mirrored from cmark-gfm/Comrak's
//! non-relaxed autolink extension. It intentionally has no parser-runtime or
//! storage dependencies: the resumable pipeline feeds it bounded tokens and
//! applies code, angle-autolink, direct-link, and bracket-context precedence.

use std::collections::VecDeque;
use std::fmt;
use std::ops::Range;

use finl_unicode::categories::CharacterCategories;
use flark_engine::parser_internal::{
    M11ParserRangeCursor, M11ParserRangeStatus, M11_PARSER_RANGE_MAX_POLL_BYTES,
};
use flark_engine::{DocumentRuntime, SourceVersion};

use crate::inline_autolink::{
    M11InlineAutolinkError, M11InlineOpaqueCandidate, M11InlineOpaqueCandidates,
    M11InlineOpaqueKind,
};
use crate::inline_direct::{M11InlineDirectCandidates, M11InlineDirectError};

pub(crate) const M11_BARE_AUTOLINK_MAX_TOKEN_BYTES: usize = 8 * 1024;
pub(crate) const M11_BARE_AUTOLINK_WWW_FLAG: u8 = 0x01;
pub(crate) const M11_INLINE_BARE_AUTOLINK_MAX_POLL_TRANSITIONS: usize =
    M11_PARSER_RANGE_MAX_POLL_BYTES;
const SOURCE_READ_CHUNK_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11BareAutolinkKind {
    Uri,
    Email,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct M11BareAutolinkCandidate {
    range: Range<u32>,
    kind: M11BareAutolinkKind,
    flags: u8,
}

impl M11BareAutolinkCandidate {
    pub(crate) fn range(&self) -> Range<u32> {
        self.range.clone()
    }

    pub(crate) const fn kind(&self) -> M11BareAutolinkKind {
        self.kind
    }

    pub(crate) const fn flags(&self) -> u8 {
        self.flags
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11BareAutolinkError {
    TokenTooLong { actual: usize, maximum: usize },
    CoordinateOverflow,
}

impl fmt::Display for M11BareAutolinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TokenTooLong { actual, maximum } => write!(
                formatter,
                "bare-autolink token has {actual} bytes; maximum is {maximum}"
            ),
            Self::CoordinateOverflow => formatter.write_str("bare-autolink coordinate exceeds u32"),
        }
    }
}

impl std::error::Error for M11BareAutolinkError {}

/// Classifies strict GFM bare autolinks in one bounded source token.
///
/// URI and `www.` candidates are resolved first, matching inline parser
/// precedence. Email candidates are then found only in the remaining text
/// gaps. Returned ranges are byte offsets relative to `input`.
pub(crate) fn classify_bare_autolinks(
    input: &str,
) -> Result<Vec<M11BareAutolinkCandidate>, M11BareAutolinkError> {
    if input.len() > M11_BARE_AUTOLINK_MAX_TOKEN_BYTES {
        return Err(M11BareAutolinkError::TokenTooLong {
            actual: input.len(),
            maximum: M11_BARE_AUTOLINK_MAX_TOKEN_BYTES,
        });
    }

    let mut uris = classify_uris(input)?;
    let mut candidates = Vec::with_capacity(uris.len());
    let mut gap_start = 0usize;
    for uri in &uris {
        let uri_start = usize::try_from(uri.range.start)
            .map_err(|_| M11BareAutolinkError::CoordinateOverflow)?;
        classify_emails_in(input, gap_start..uri_start, &mut candidates)?;
        gap_start =
            usize::try_from(uri.range.end).map_err(|_| M11BareAutolinkError::CoordinateOverflow)?;
    }
    classify_emails_in(input, gap_start..input.len(), &mut candidates)?;
    candidates.append(&mut uris);
    candidates.sort_unstable_by(|left, right| {
        left.range
            .start
            .cmp(&right.range.start)
            .then_with(|| right.range.end.cmp(&left.range.end))
    });
    Ok(candidates)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum M11InlineBareAutolinkPollStatus {
    Pending,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct M11InlineBareAutolinkPoll {
    status: M11InlineBareAutolinkPollStatus,
    transitions: usize,
}

impl M11InlineBareAutolinkPoll {
    pub(crate) const fn status(self) -> M11InlineBareAutolinkPollStatus {
        self.status
    }

    pub(crate) const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Debug)]
pub(crate) enum M11InlineBareAutolinkJobError {
    Opaque(M11InlineAutolinkError),
    Direct(M11InlineDirectError),
    Grammar(M11BareAutolinkError),
    ZeroFuel,
    PollLimitExceeded,
    InvalidUtf8,
    CoordinateOverflow,
    InvalidState,
}

impl fmt::Display for M11InlineBareAutolinkJobError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Opaque(error) => write!(formatter, "bare-autolink opaque map failed: {error}"),
            Self::Direct(error) => write!(formatter, "bare-autolink direct map failed: {error}"),
            Self::Grammar(error) => write!(formatter, "bare-autolink grammar failed: {error}"),
            Self::ZeroFuel => formatter.write_str("bare-autolink poll requires nonzero fuel"),
            Self::PollLimitExceeded => {
                formatter.write_str("bare-autolink poll exceeds its transition limit")
            }
            Self::InvalidUtf8 => formatter.write_str("bare-autolink token is invalid UTF-8"),
            Self::CoordinateOverflow => {
                formatter.write_str("bare-autolink coordinate or counter overflow")
            }
            Self::InvalidState => formatter.write_str("bare-autolink job is in an invalid state"),
        }
    }
}

impl std::error::Error for M11InlineBareAutolinkJobError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Opaque(error) => Some(error),
            Self::Direct(error) => Some(error),
            Self::Grammar(error) => Some(error),
            _ => None,
        }
    }
}

impl From<M11InlineAutolinkError> for M11InlineBareAutolinkJobError {
    fn from(value: M11InlineAutolinkError) -> Self {
        Self::Opaque(value)
    }
}

impl From<M11InlineDirectError> for M11InlineBareAutolinkJobError {
    fn from(value: M11InlineDirectError) -> Self {
        Self::Direct(value)
    }
}

impl From<M11BareAutolinkError> for M11InlineBareAutolinkJobError {
    fn from(value: M11BareAutolinkError) -> Self {
        Self::Grammar(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BareJobPhase {
    Scan,
    Classify,
    Filter,
    Merge,
    Complete,
    Cancelled,
    Transferred,
}

/// Resumable whole-leaf bare-autolink stage.
///
/// Source copying, token processing, bracket-context filtering, and the final
/// source-order merge are fuelled. The one synchronous grammar call is capped
/// to an 8 KiB token and preceded by one charged transition per token byte.
pub(crate) struct M11InlineBareAutolinkJob {
    source: SourceVersion,
    source_range: Range<u32>,
    cursor: Option<M11ParserRangeCursor>,
    pending_source: VecDeque<u8>,
    read_complete: bool,
    relative_offset: u32,
    phase: BareJobPhase,
    token_start: u32,
    token: Vec<u8>,
    token_overflow: bool,
    classification_credit: usize,
    classified: Vec<M11BareAutolinkCandidate>,
    classified_index: usize,
    filter_offset: usize,
    accepted_end: Option<u32>,
    bare: Vec<M11InlineOpaqueCandidate>,
    bracket_depth: u32,
    bracket_context_unknown: bool,
    raw_backslashes: u32,
    opaque_filter_index: u32,
    direct_filter_index: u32,
    merge_opaque_index: u32,
    merge_bare_index: usize,
    merged: Vec<M11InlineOpaqueCandidate>,
    output: Option<Vec<M11InlineOpaqueCandidate>>,
}

impl M11InlineBareAutolinkJob {
    pub(crate) fn new(
        runtime: &DocumentRuntime,
        opaque: &M11InlineOpaqueCandidates,
        direct: &M11InlineDirectCandidates,
    ) -> Result<Self, M11InlineBareAutolinkJobError> {
        direct.validate_source(runtime, opaque)?;
        let capacity = usize::try_from(opaque.len())
            .map_err(|_| M11InlineBareAutolinkJobError::CoordinateOverflow)?;
        Ok(Self {
            source: opaque.source(),
            source_range: opaque.source_range(),
            cursor: Some(opaque.source_cursor(runtime)?),
            pending_source: VecDeque::with_capacity(SOURCE_READ_CHUNK_BYTES),
            read_complete: false,
            relative_offset: 0,
            phase: BareJobPhase::Scan,
            token_start: 0,
            token: Vec::new(),
            token_overflow: false,
            classification_credit: 0,
            classified: Vec::new(),
            classified_index: 0,
            filter_offset: 0,
            accepted_end: None,
            bare: Vec::new(),
            bracket_depth: 0,
            bracket_context_unknown: false,
            raw_backslashes: 0,
            opaque_filter_index: 0,
            direct_filter_index: 0,
            merge_opaque_index: 0,
            merge_bare_index: 0,
            merged: Vec::with_capacity(capacity),
            output: None,
        })
    }

    pub(crate) fn poll(
        &mut self,
        runtime: &DocumentRuntime,
        opaque: &M11InlineOpaqueCandidates,
        direct: &M11InlineDirectCandidates,
        fuel: usize,
    ) -> Result<M11InlineBareAutolinkPoll, M11InlineBareAutolinkJobError> {
        validate_job_fuel(fuel)?;
        if self.phase == BareJobPhase::Complete {
            return Ok(M11InlineBareAutolinkPoll {
                status: M11InlineBareAutolinkPollStatus::Complete,
                transitions: 0,
            });
        }
        if matches!(
            self.phase,
            BareJobPhase::Cancelled | BareJobPhase::Transferred
        ) {
            return Err(M11InlineBareAutolinkJobError::InvalidState);
        }
        direct.validate_source(runtime, opaque)?;
        if opaque.source() != self.source || opaque.source_range() != self.source_range {
            return Err(M11InlineBareAutolinkJobError::InvalidState);
        }

        let mut transitions = 0usize;
        while transitions < fuel {
            match self.phase {
                BareJobPhase::Scan => self.poll_scan(fuel, &mut transitions)?,
                BareJobPhase::Classify => self.poll_classify(fuel, &mut transitions)?,
                BareJobPhase::Filter => {
                    self.poll_filter(opaque, direct, &mut transitions)?;
                }
                BareJobPhase::Merge => self.poll_merge(opaque, &mut transitions)?,
                BareJobPhase::Complete => {
                    return Ok(M11InlineBareAutolinkPoll {
                        status: M11InlineBareAutolinkPollStatus::Complete,
                        transitions,
                    });
                }
                BareJobPhase::Cancelled | BareJobPhase::Transferred => {
                    return Err(M11InlineBareAutolinkJobError::InvalidState);
                }
            }
        }
        Ok(M11InlineBareAutolinkPoll {
            status: if self.phase == BareJobPhase::Complete {
                M11InlineBareAutolinkPollStatus::Complete
            } else {
                M11InlineBareAutolinkPollStatus::Pending
            },
            transitions,
        })
    }

    fn poll_scan(
        &mut self,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineBareAutolinkJobError> {
        if let Some(byte) = self.pending_source.pop_front() {
            let offset = self.relative_offset;
            self.relative_offset = self
                .relative_offset
                .checked_add(1)
                .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?;
            if is_ascii_space(byte) {
                self.finish_token();
            } else {
                if self.token.is_empty() && !self.token_overflow {
                    self.token_start = offset;
                }
                if self.token.len() < M11_BARE_AUTOLINK_MAX_TOKEN_BYTES {
                    self.token.push(byte);
                } else {
                    self.token_overflow = true;
                }
            }
            *transitions += 1;
            return Ok(());
        }
        if self.read_complete {
            let expected_length = self
                .source_range
                .end
                .checked_sub(self.source_range.start)
                .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?;
            if self.relative_offset != expected_length {
                return Err(M11InlineBareAutolinkJobError::InvalidState);
            }
            if self.token.is_empty() && !self.token_overflow {
                self.begin_merge()?;
            } else {
                self.finish_token();
            }
            *transitions += 1;
            return Ok(());
        }

        let remaining = fuel - *transitions;
        let read_fuel = remaining.min(SOURCE_READ_CHUNK_BYTES);
        let mut buffer = [0u8; SOURCE_READ_CHUNK_BYTES];
        let poll = self
            .cursor
            .as_mut()
            .ok_or(M11InlineBareAutolinkJobError::InvalidState)?
            .poll(read_fuel, &mut buffer[..read_fuel])
            .map_err(M11InlineAutolinkError::from)?;
        self.pending_source.extend(&buffer[..poll.bytes_read()]);
        *transitions = transitions
            .checked_add(poll.transitions())
            .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?;
        if poll.status() == M11ParserRangeStatus::Complete {
            self.read_complete = true;
            drop(self.cursor.take());
        }
        Ok(())
    }

    fn finish_token(&mut self) {
        if self.token_overflow {
            self.bracket_context_unknown = true;
            self.token.clear();
            self.token_overflow = false;
            self.classification_credit = 0;
            return;
        }
        if self.token.is_empty() {
            return;
        }
        self.classification_credit = 0;
        self.phase = BareJobPhase::Classify;
    }

    fn poll_classify(
        &mut self,
        fuel: usize,
        transitions: &mut usize,
    ) -> Result<(), M11InlineBareAutolinkJobError> {
        if self.classification_credit < self.token.len() {
            let charged = (fuel - *transitions).min(
                self.token
                    .len()
                    .checked_sub(self.classification_credit)
                    .ok_or(M11InlineBareAutolinkJobError::InvalidState)?,
            );
            self.classification_credit = self
                .classification_credit
                .checked_add(charged)
                .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?;
            *transitions += charged;
            return Ok(());
        }
        let token = std::str::from_utf8(&self.token)
            .map_err(|_| M11InlineBareAutolinkJobError::InvalidUtf8)?;
        self.classified = classify_bare_autolinks(token)?;
        for candidate in &mut self.classified {
            candidate.range = add_base(candidate.range(), self.token_start)?;
        }
        self.classified_index = 0;
        self.filter_offset = 0;
        self.accepted_end = None;
        self.phase = BareJobPhase::Filter;
        *transitions += 1;
        Ok(())
    }

    fn poll_filter(
        &mut self,
        opaque: &M11InlineOpaqueCandidates,
        direct: &M11InlineDirectCandidates,
        transitions: &mut usize,
    ) -> Result<(), M11InlineBareAutolinkJobError> {
        if self.filter_offset == self.token.len() {
            let token_end = self
                .token_start
                .checked_add(
                    u32::try_from(self.token.len())
                        .map_err(|_| M11InlineBareAutolinkJobError::CoordinateOverflow)?,
                )
                .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?;
            if self.accepted_end == Some(token_end) {
                self.accepted_end = None;
            }
            if self.classified_index != self.classified.len() || self.accepted_end.is_some() {
                return Err(M11InlineBareAutolinkJobError::InvalidState);
            }
            self.token.clear();
            self.classified.clear();
            self.classification_credit = 0;
            self.filter_offset = 0;
            self.raw_backslashes = 0;
            if self.read_complete && self.pending_source.is_empty() {
                self.begin_merge()?;
            } else {
                self.phase = BareJobPhase::Scan;
            }
            *transitions += 1;
            return Ok(());
        }

        let relative_filter = u32::try_from(self.filter_offset)
            .map_err(|_| M11InlineBareAutolinkJobError::CoordinateOverflow)?;
        let offset = self
            .token_start
            .checked_add(relative_filter)
            .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?;
        if self.accepted_end == Some(offset) {
            self.accepted_end = None;
        }
        if self.accepted_end.is_none() {
            if let Some(candidate) = self.classified.get(self.classified_index) {
                if candidate.range.start == offset {
                    let range = candidate.range();
                    let accepted = !self.bracket_context_unknown
                        && self.bracket_depth == 0
                        && !range_intersects_opaque(opaque, self.opaque_filter_index, &range)?
                        && !range_intersects_direct(direct, self.direct_filter_index, &range);
                    if accepted {
                        let kind = match candidate.kind {
                            M11BareAutolinkKind::Uri => M11InlineOpaqueKind::AutolinkUri,
                            M11BareAutolinkKind::Email => M11InlineOpaqueKind::AutolinkEmail,
                        };
                        self.bare.push(M11InlineOpaqueCandidate::new_bare_autolink(
                            kind,
                            candidate.flags,
                            range.clone(),
                        )?);
                        self.accepted_end = Some(range.end);
                    }
                    self.classified_index += 1;
                } else if candidate.range.start < offset {
                    return Err(M11InlineBareAutolinkJobError::InvalidState);
                }
            }
        }

        let inside_accepted = self.accepted_end.is_some_and(|end| offset < end);
        if !inside_accepted && !self.byte_is_blocked(opaque, direct, offset)? {
            let byte = self.token[self.filter_offset];
            match byte {
                b'[' if self.raw_backslashes.is_multiple_of(2) => {
                    self.bracket_depth = self
                        .bracket_depth
                        .checked_add(1)
                        .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?;
                }
                b']' if self.raw_backslashes.is_multiple_of(2) && self.bracket_depth > 0 => {
                    self.bracket_depth -= 1;
                }
                _ => {}
            }
            self.raw_backslashes = if byte == b'\\' {
                self.raw_backslashes
                    .checked_add(1)
                    .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?
            } else {
                0
            };
        } else {
            self.raw_backslashes = 0;
        }
        self.filter_offset += 1;
        *transitions += 1;
        Ok(())
    }

    fn byte_is_blocked(
        &mut self,
        opaque: &M11InlineOpaqueCandidates,
        direct: &M11InlineDirectCandidates,
        offset: u32,
    ) -> Result<bool, M11InlineBareAutolinkJobError> {
        while let Some(range) = opaque
            .candidate(self.opaque_filter_index)?
            .map(|candidate| candidate.relative_range())
        {
            if range.end > offset {
                break;
            }
            self.opaque_filter_index = self
                .opaque_filter_index
                .checked_add(1)
                .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?;
        }
        while let Some(range) = direct
            .fact(self.direct_filter_index)
            .map(|candidate| candidate.source())
        {
            if range.end > offset {
                break;
            }
            self.direct_filter_index = self
                .direct_filter_index
                .checked_add(1)
                .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?;
        }
        let opaque_blocked = opaque
            .candidate(self.opaque_filter_index)?
            .is_some_and(|candidate| {
                let range = candidate.relative_range();
                range.start <= offset && offset < range.end
            });
        let direct_blocked = direct
            .fact(self.direct_filter_index)
            .is_some_and(|candidate| {
                let range = candidate.source();
                range.start <= offset && offset < range.end
            });
        Ok(opaque_blocked || direct_blocked)
    }

    fn begin_merge(&mut self) -> Result<(), M11InlineBareAutolinkJobError> {
        if !self.token.is_empty()
            || self.token_overflow
            || !self.classified.is_empty()
            || self.accepted_end.is_some()
        {
            return Err(M11InlineBareAutolinkJobError::InvalidState);
        }
        let capacity = usize::try_from(self.merge_opaque_index)
            .map_err(|_| M11InlineBareAutolinkJobError::CoordinateOverflow)?
            .checked_add(self.bare.len())
            .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?;
        self.merged.reserve(capacity);
        self.phase = BareJobPhase::Merge;
        Ok(())
    }

    fn poll_merge(
        &mut self,
        opaque: &M11InlineOpaqueCandidates,
        transitions: &mut usize,
    ) -> Result<(), M11InlineBareAutolinkJobError> {
        let existing = opaque.candidate(self.merge_opaque_index)?;
        let bare = self.bare.get(self.merge_bare_index).copied();
        let next = match (existing, bare) {
            (Some(existing), Some(bare)) => {
                let existing_range = existing.relative_range();
                let bare_range = bare.relative_range();
                if ranges_overlap(&existing_range, &bare_range) {
                    return Err(M11InlineBareAutolinkJobError::InvalidState);
                }
                if compare_ranges(&existing_range, &bare_range).is_le() {
                    self.merge_opaque_index = self
                        .merge_opaque_index
                        .checked_add(1)
                        .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?;
                    existing
                } else {
                    self.merge_bare_index += 1;
                    bare
                }
            }
            (Some(existing), None) => {
                self.merge_opaque_index = self
                    .merge_opaque_index
                    .checked_add(1)
                    .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?;
                existing
            }
            (None, Some(bare)) => {
                self.merge_bare_index += 1;
                bare
            }
            (None, None) => {
                if self.merge_opaque_index != opaque.len()
                    || self.merge_bare_index != self.bare.len()
                {
                    return Err(M11InlineBareAutolinkJobError::InvalidState);
                }
                self.output = Some(std::mem::take(&mut self.merged));
                self.phase = BareJobPhase::Complete;
                *transitions += 1;
                return Ok(());
            }
        };
        self.merged.push(next);
        *transitions += 1;
        Ok(())
    }

    pub(crate) fn take_output(&mut self) -> Option<Vec<M11InlineOpaqueCandidate>> {
        if self.phase != BareJobPhase::Complete {
            return None;
        }
        let output = self.output.take()?;
        self.phase = BareJobPhase::Transferred;
        Some(output)
    }

    pub(crate) fn cancel(&mut self) {
        if let Some(cursor) = self.cursor.as_mut() {
            cursor.cancel();
        }
        drop(self.cursor.take());
        self.pending_source.clear();
        self.token.clear();
        self.classified.clear();
        self.bare.clear();
        self.merged.clear();
        drop(self.output.take());
        self.phase = BareJobPhase::Cancelled;
    }
}

impl Drop for M11InlineBareAutolinkJob {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                matches!(
                    self.phase,
                    BareJobPhase::Cancelled | BareJobPhase::Transferred
                ),
                "bare-autolink jobs require output transfer or explicit cancellation"
            );
        }
    }
}

fn range_intersects_opaque(
    opaque: &M11InlineOpaqueCandidates,
    start_index: u32,
    range: &Range<u32>,
) -> Result<bool, M11InlineBareAutolinkJobError> {
    let mut index = start_index;
    while let Some(candidate) = opaque.candidate(index)? {
        let candidate_range = candidate.relative_range();
        if candidate_range.start >= range.end {
            return Ok(false);
        }
        if ranges_overlap(&candidate_range, range) {
            return Ok(true);
        }
        index = index
            .checked_add(1)
            .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?;
    }
    Ok(false)
}

fn range_intersects_direct(
    direct: &M11InlineDirectCandidates,
    start_index: u32,
    range: &Range<u32>,
) -> bool {
    let mut index = start_index;
    while let Some(candidate) = direct.fact(index) {
        let candidate_range = candidate.source();
        if candidate_range.start >= range.end {
            return false;
        }
        if ranges_overlap(&candidate_range, range) {
            return true;
        }
        let Some(next) = index.checked_add(1) else {
            return true;
        };
        index = next;
    }
    false
}

fn ranges_overlap(left: &Range<u32>, right: &Range<u32>) -> bool {
    left.start < right.end && right.start < left.end
}

fn compare_ranges(left: &Range<u32>, right: &Range<u32>) -> std::cmp::Ordering {
    left.start
        .cmp(&right.start)
        .then_with(|| right.end.cmp(&left.end))
}

fn add_base(range: Range<u32>, base: u32) -> Result<Range<u32>, M11InlineBareAutolinkJobError> {
    Ok(range
        .start
        .checked_add(base)
        .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?
        ..range
            .end
            .checked_add(base)
            .ok_or(M11InlineBareAutolinkJobError::CoordinateOverflow)?)
}

fn validate_job_fuel(fuel: usize) -> Result<(), M11InlineBareAutolinkJobError> {
    if fuel == 0 {
        return Err(M11InlineBareAutolinkJobError::ZeroFuel);
    }
    if fuel > M11_INLINE_BARE_AUTOLINK_MAX_POLL_TRANSITIONS {
        return Err(M11InlineBareAutolinkJobError::PollLimitExceeded);
    }
    Ok(())
}

fn classify_uris(input: &str) -> Result<Vec<M11BareAutolinkCandidate>, M11BareAutolinkError> {
    let bytes = input.as_bytes();
    let mut candidates = Vec::new();
    let mut position = 0usize;
    while position < bytes.len() {
        let (prefix_len, host_offset, flags) = if starts_scheme(input, position, "https://") {
            (8usize, 8usize, 0)
        } else if starts_scheme(input, position, "http://") {
            (7, 7, 0)
        } else if starts_scheme(input, position, "ftp://") {
            (6, 6, 0)
        } else if starts_www(input, position) {
            (4, 4, M11_BARE_AUTOLINK_WWW_FLAG)
        } else {
            position += 1;
            continue;
        };

        let host_start = position
            .checked_add(host_offset)
            .ok_or(M11BareAutolinkError::CoordinateOverflow)?;
        let Some(domain_len) = input.get(host_start..).and_then(check_domain) else {
            position += 1;
            continue;
        };
        let mut end = host_start
            .checked_add(domain_len)
            .ok_or(M11BareAutolinkError::CoordinateOverflow)?;
        while end < bytes.len() && !is_ascii_space(bytes[end]) {
            end += 1;
        }
        let relative_end = autolink_delim(
            input
                .get(position..end)
                .ok_or(M11BareAutolinkError::CoordinateOverflow)?,
        );
        end = position
            .checked_add(relative_end)
            .ok_or(M11BareAutolinkError::CoordinateOverflow)?;
        if end <= position + prefix_len {
            position += 1;
            continue;
        }
        candidates.push(M11BareAutolinkCandidate {
            range: checked_range(position, end)?,
            kind: M11BareAutolinkKind::Uri,
            flags,
        });
        position = end;
    }
    Ok(candidates)
}

fn starts_scheme(input: &str, position: usize, scheme: &str) -> bool {
    input
        .get(position..)
        .is_some_and(|suffix| suffix.starts_with(scheme))
        && (position == 0 || !input.as_bytes()[position - 1].is_ascii_alphabetic())
}

fn starts_www(input: &str, position: usize) -> bool {
    if !input
        .get(position..)
        .is_some_and(|suffix| suffix.starts_with("www."))
    {
        return false;
    }
    position == 0
        || is_ascii_space(input.as_bytes()[position - 1])
        || matches!(
            input.as_bytes()[position - 1],
            b'*' | b'_' | b'~' | b'(' | b'['
        )
}

const fn is_ascii_space(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\r' | b' ')
}

fn check_domain(data: &str) -> Option<usize> {
    let mut periods = 0usize;
    let mut underscores_before_period = 0usize;
    let mut underscores_after_period = 0usize;

    for (index, character) in data.char_indices() {
        if character == '\\' && index < data.len().saturating_sub(1) {
            // Match cmark-gfm: an escape byte does not itself terminate domain
            // recognition; the escaped byte is still evaluated normally.
        } else if character == '_' {
            underscores_after_period += 1;
        } else if character == '.' {
            underscores_before_period = underscores_after_period;
            underscores_after_period = 0;
            periods += 1;
        } else if !is_valid_host_character(character) && character != '-' {
            if underscores_before_period == 0 && underscores_after_period == 0 && periods > 0 {
                return Some(index);
            }
            return None;
        }
    }

    if (underscores_before_period > 0 || underscores_after_period > 0) && periods <= 10 {
        None
    } else if periods > 0 {
        Some(data.len())
    } else {
        None
    }
}

fn is_valid_host_character(character: char) -> bool {
    !(character.is_whitespace() || character.is_punctuation() || character.is_symbol())
}

fn classify_emails_in(
    input: &str,
    range: Range<usize>,
    output: &mut Vec<M11BareAutolinkCandidate>,
) -> Result<(), M11BareAutolinkError> {
    let bytes = input.as_bytes();
    let mut at = range.start;
    while at < range.end {
        if bytes[at] != b'@' {
            at += 1;
            continue;
        }
        let mut rewind = 0usize;
        while rewind < at - range.start && is_email_local_byte(bytes[at - rewind - 1]) {
            rewind += 1;
        }
        if rewind == 0 {
            at += 1;
            continue;
        }
        let start = at - rewind;
        if starts_explicit_mail_protocol(input, start) {
            at += 1;
            continue;
        }

        let mut link_end = 1usize;
        let mut periods = 0usize;
        while link_end < range.end - at {
            let byte = bytes[at + link_end];
            if byte.is_ascii_alphanumeric() {
                // Accepted.
            } else if byte == b'@' {
                link_end = 0;
                break;
            } else if byte == b'.'
                && link_end < range.end - at - 1
                && bytes[at + link_end + 1].is_ascii_alphanumeric()
            {
                periods += 1;
            } else if !matches!(byte, b'-' | b'_') {
                break;
            }
            link_end += 1;
        }
        if link_end < 2
            || periods == 0
            || (!bytes[at + link_end - 1].is_ascii_alphabetic() && bytes[at + link_end - 1] != b'.')
        {
            at += 1;
            continue;
        }
        let trimmed = autolink_delim(
            input
                .get(at..at + link_end)
                .ok_or(M11BareAutolinkError::CoordinateOverflow)?,
        );
        if trimmed == 0 {
            at += 1;
            continue;
        }
        let end = at
            .checked_add(trimmed)
            .ok_or(M11BareAutolinkError::CoordinateOverflow)?;
        output.push(M11BareAutolinkCandidate {
            range: checked_range(start, end)?,
            kind: M11BareAutolinkKind::Email,
            flags: 0,
        });
        at = end;
    }
    Ok(())
}

fn starts_explicit_mail_protocol(input: &str, local_start: usize) -> bool {
    let Some(prefix) = input.get(..local_start) else {
        return false;
    };
    ["mailto:", "xmpp:"].into_iter().any(|protocol| {
        if !prefix.ends_with(protocol) {
            return false;
        }
        let protocol_start = prefix.len() - protocol.len();
        protocol_start == 0 || !prefix.as_bytes()[protocol_start - 1].is_ascii_alphabetic()
    })
}

const fn is_email_local_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-' | b'_')
}

fn autolink_delim(data: &str) -> usize {
    let bytes = data.as_bytes();
    let mut link_end = bytes
        .iter()
        .position(|byte| *byte == b'<')
        .unwrap_or(bytes.len());

    while link_end > 0 {
        let closing = bytes[link_end - 1];
        if matches!(
            closing,
            b'?' | b'!' | b'.' | b',' | b':' | b'*' | b'_' | b'~' | b'\'' | b'"'
        ) {
            link_end -= 1;
        } else if closing == b';' {
            let mut new_end = link_end.saturating_sub(2);
            while new_end > 0 && bytes[new_end].is_ascii_alphabetic() {
                new_end -= 1;
            }
            if new_end < link_end.saturating_sub(2) && bytes[new_end] == b'&' {
                link_end = new_end;
            } else {
                link_end -= 1;
            }
        } else if closing == b')' {
            let mut opening_count = 0usize;
            let mut closing_count = 0usize;
            for byte in &bytes[..link_end] {
                if *byte == b'(' {
                    opening_count += 1;
                } else if *byte == b')' {
                    closing_count += 1;
                }
            }
            if closing_count <= opening_count {
                break;
            }
            link_end -= 1;
        } else if link_end >= 3 && bytes[link_end - 3..link_end] == [0xe2, 0x81, 0xa9] {
            link_end -= 3;
            break;
        } else {
            break;
        }
    }
    link_end
}

fn checked_range(start: usize, end: usize) -> Result<Range<u32>, M11BareAutolinkError> {
    Ok(
        u32::try_from(start).map_err(|_| M11BareAutolinkError::CoordinateOverflow)?
            ..u32::try_from(end).map_err(|_| M11BareAutolinkError::CoordinateOverflow)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classified(source: &str) -> Vec<(Range<u32>, M11BareAutolinkKind, u8)> {
        classify_bare_autolinks(source)
            .expect("classification")
            .into_iter()
            .map(|candidate| (candidate.range(), candidate.kind(), candidate.flags()))
            .collect()
    }

    fn expected(
        source: &str,
        text: &str,
        kind: M11BareAutolinkKind,
        flags: u8,
    ) -> (Range<u32>, M11BareAutolinkKind, u8) {
        let start = source.find(text).expect("expected text");
        (
            u32::try_from(start).expect("start")..u32::try_from(start + text.len()).expect("end"),
            kind,
            flags,
        )
    }

    #[test]
    fn gfm_examples_621_through_623_cover_www_path_and_period_trimming() {
        for (source, target) in [
            ("www.commonmark.org", "www.commonmark.org"),
            (
                "Visit www.commonmark.org/help for more information.",
                "www.commonmark.org/help",
            ),
            ("Visit www.commonmark.org.", "www.commonmark.org"),
            ("Visit www.commonmark.org/a.b.", "www.commonmark.org/a.b"),
        ] {
            assert_eq!(
                classified(source),
                vec![expected(
                    source,
                    target,
                    M11BareAutolinkKind::Uri,
                    M11_BARE_AUTOLINK_WWW_FLAG,
                )]
            );
        }
    }

    #[test]
    fn gfm_examples_624_and_625_match_parenthesis_rules() {
        for (source, target) in [
            (
                "www.google.com/search?q=Markup+(business)",
                "www.google.com/search?q=Markup+(business)",
            ),
            (
                "www.google.com/search?q=Markup+(business)))",
                "www.google.com/search?q=Markup+(business)",
            ),
            (
                "(www.google.com/search?q=Markup+(business))",
                "www.google.com/search?q=Markup+(business)",
            ),
            (
                "(www.google.com/search?q=Markup+(business)",
                "www.google.com/search?q=Markup+(business)",
            ),
            (
                "www.google.com/search?q=(business))+ok",
                "www.google.com/search?q=(business))+ok",
            ),
        ] {
            assert_eq!(
                classified(source),
                vec![expected(
                    source,
                    target,
                    M11BareAutolinkKind::Uri,
                    M11_BARE_AUTOLINK_WWW_FLAG,
                )]
            );
        }
    }

    #[test]
    fn gfm_examples_626_and_627_match_entity_and_angle_trimming() {
        for (source, target) in [
            (
                "www.google.com/search?q=commonmark&hl=en",
                "www.google.com/search?q=commonmark&hl=en",
            ),
            (
                "www.google.com/search?q=commonmark&hl;",
                "www.google.com/search?q=commonmark",
            ),
            ("www.commonmark.org/he<lp", "www.commonmark.org/he"),
        ] {
            assert_eq!(
                classified(source),
                vec![expected(
                    source,
                    target,
                    M11BareAutolinkKind::Uri,
                    M11_BARE_AUTOLINK_WWW_FLAG,
                )]
            );
        }
    }

    #[test]
    fn gfm_example_628_accepts_only_lowercase_supported_schemes() {
        let source = "http://commonmark.org https://encrypted.google.com/search?q=Markup+(business) ftp://foo.bar.baz.";
        assert_eq!(
            classified(source),
            vec![
                expected(source, "http://commonmark.org", M11BareAutolinkKind::Uri, 0,),
                expected(
                    source,
                    "https://encrypted.google.com/search?q=Markup+(business)",
                    M11BareAutolinkKind::Uri,
                    0,
                ),
                expected(source, "ftp://foo.bar.baz", M11BareAutolinkKind::Uri, 0,),
            ]
        );
        assert!(classified("HTTP://commonmark.org Www.commonmark.org").is_empty());
    }

    #[test]
    fn gfm_examples_629_through_631_match_email_rules() {
        for (source, targets) in [
            ("foo@bar.baz", vec!["foo@bar.baz"]),
            (
                "hello@mail+xyz.example isn't valid, but hello+xyz@mail.example is.",
                vec!["hello+xyz@mail.example"],
            ),
            ("a.b-c_d@a.b", vec!["a.b-c_d@a.b"]),
            ("a.b-c_d@a.b.", vec!["a.b-c_d@a.b"]),
            ("a.b-c_d@a.b-", vec![]),
            ("a.b-c_d@a.b_", vec![]),
        ] {
            assert_eq!(
                classified(source),
                targets
                    .into_iter()
                    .map(|target| expected(source, target, M11BareAutolinkKind::Email, 0))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn uri_precedence_and_boundaries_are_deterministic() {
        let source = "xhttp://x.y 1http://a.b _www.a.b http://a.b/foo@bar.com";
        assert_eq!(
            classified(source),
            vec![
                expected(source, "http://a.b", M11BareAutolinkKind::Uri, 0),
                expected(
                    source,
                    "www.a.b",
                    M11BareAutolinkKind::Uri,
                    M11_BARE_AUTOLINK_WWW_FLAG,
                ),
                expected(
                    source,
                    "http://a.b/foo@bar.com",
                    M11BareAutolinkKind::Uri,
                    0,
                ),
            ]
        );
    }

    #[test]
    fn explicit_mail_protocols_decline_until_the_wire_has_an_exact_target_recipe() {
        assert!(classified("mailto:foo@bar.baz xmpp:foo@bar.baz").is_empty());
        let source = "notmailto:foo@bar.baz";
        assert_eq!(
            classified(source),
            vec![expected(
                source,
                "foo@bar.baz",
                M11BareAutolinkKind::Email,
                0,
            )]
        );
    }

    #[test]
    fn token_cap_is_explicit_and_allocation_bounded() {
        let source = "x".repeat(M11_BARE_AUTOLINK_MAX_TOKEN_BYTES + 1);
        assert_eq!(
            classify_bare_autolinks(&source),
            Err(M11BareAutolinkError::TokenTooLong {
                actual: M11_BARE_AUTOLINK_MAX_TOKEN_BYTES + 1,
                maximum: M11_BARE_AUTOLINK_MAX_TOKEN_BYTES,
            })
        );
    }
}

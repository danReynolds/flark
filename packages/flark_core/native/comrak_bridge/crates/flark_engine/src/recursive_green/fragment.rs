//! Build-local logical projection and canonical terminal-fragment rewrite.
//!
//! The cursor reads the exact packed events owned by the unpublished Green
//! build. It never materializes a terminal leaf's text and never turns a
//! logical offset into an assumed physical offset.

use std::ops::Range;

use crate::document::DocumentRuntime;
use crate::measured_sequence::{
    splice_measured_sequence_build_root_atomic, MeasuredSequenceBuildRoot,
    ResumableMeasuredSequenceBuilder, ResumableSequenceProgress, SequenceInspectionReceipt,
    SequenceSpecInspection,
};
use crate::source::{SourceCursor, SOURCE_CURSOR_WINDOW_BYTES};
use crate::storage::ARENA_PAGE_BYTES;

use super::build::{
    BuildPhase, GreenSequenceBuildRoot, M11RecursiveGreenBuild,
    M11RecursiveGreenTerminalFragmentBinding, M11RecursiveGreenTerminalFragmentStamp,
};
use super::codec::{
    decode_leaf, decode_packed_event, encode_leaf_header, encode_packed_event, packed_event_len,
    packed_event_summary, LogicalAtom, M11RecursiveGreenCoveragePart, M11RecursiveGreenError,
    M11RecursiveGreenFrameId, M11RecursiveGreenSourceMetric, PackedGreenEvent, RecursiveGreenSpec,
    RecursiveGreenSummary, GREEN_EVENTS_PER_PAGE_MAX, GREEN_LEAF_HEADER_BYTES,
};

const REPLACEMENT_BYTES: [u8; 3] = [0xef, 0xbf, 0xbd];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11RecursiveGreenLogicalPosition {
    bytes: u64,
    utf16: u64,
}

impl M11RecursiveGreenLogicalPosition {
    #[must_use]
    pub const fn new(bytes: u64, utf16: u64) -> Option<Self> {
        if bytes < utf16 {
            None
        } else {
            Some(Self { bytes, utf16 })
        }
    }

    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.bytes
    }

    #[must_use]
    pub const fn utf16(self) -> u64 {
        self.utf16
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenLogicalRange {
    bytes: Range<u64>,
    utf16: Range<u64>,
}

impl M11RecursiveGreenLogicalRange {
    #[must_use]
    pub fn new(
        start: M11RecursiveGreenLogicalPosition,
        end: M11RecursiveGreenLogicalPosition,
    ) -> Option<Self> {
        (start.bytes <= end.bytes && start.utf16 <= end.utf16).then_some(Self {
            bytes: start.bytes..end.bytes,
            utf16: start.utf16..end.utf16,
        })
    }

    #[must_use]
    pub fn byte_range(&self) -> Range<u64> {
        self.bytes.clone()
    }

    #[must_use]
    pub fn utf16_range(&self) -> Range<u64> {
        self.utf16.clone()
    }
}

/// Linear range authority bound to one exact frozen projection generation.
/// Endpoint metrics are revalidated by range replay before completion.
#[must_use = "logical ranges must be replayed or discarded with their fragment"]
pub struct M11RecursiveGreenTerminalFragmentRange {
    stamp: M11RecursiveGreenTerminalFragmentStamp,
    range: M11RecursiveGreenLogicalRange,
    physical: Option<M11RecursiveGreenTerminalFragmentPhysicalRange>,
    replay_validated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenTerminalFragmentPhysicalRange {
    bytes: Range<u64>,
    utf16: Range<u64>,
}

impl M11RecursiveGreenTerminalFragmentPhysicalRange {
    #[must_use]
    pub fn byte_range(&self) -> Range<u64> {
        self.bytes.clone()
    }

    #[must_use]
    pub fn utf16_range(&self) -> Range<u64> {
        self.utf16.clone()
    }
}

impl M11RecursiveGreenTerminalFragmentRange {
    /// Returns the physical source envelope authenticated by completed replay.
    #[must_use]
    pub fn physical_range(&self) -> Option<&M11RecursiveGreenTerminalFragmentPhysicalRange> {
        self.replay_validated.then_some(())?;
        self.physical.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenProjectedByte {
    relative_offset: u64,
    byte: u8,
}

impl M11RecursiveGreenProjectedByte {
    #[must_use]
    pub const fn relative_offset(self) -> u64 {
        self.relative_offset
    }

    #[must_use]
    pub const fn byte(self) -> u8 {
        self.byte
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenTerminalFragmentCursorStatus {
    Pending,
    ByteReady,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenTerminalFragmentCursorPoll {
    status: M11RecursiveGreenTerminalFragmentCursorStatus,
    transitions: usize,
}

impl M11RecursiveGreenTerminalFragmentCursorPoll {
    #[must_use]
    pub const fn status(self) -> M11RecursiveGreenTerminalFragmentCursorStatus {
        self.status
    }

    #[must_use]
    pub const fn transitions(self) -> usize {
        self.transitions
    }
}

#[derive(Debug)]
enum ProjectedAtom {
    Identity {
        source: SourceCursor,
        physical_bytes: u64,
        physical_utf16: u64,
        scalar_remaining: u8,
        scalar_utf16: u8,
    },
    Static {
        bytes: [u8; 3],
        len: u8,
        next: u8,
        scalar_utf16_at_end: u8,
        raw_contribution_at_end: u8,
        physical: PhysicalSpan,
    },
    Spaces {
        remaining: u8,
        first: bool,
        physical: PhysicalSpan,
    },
}

#[derive(Clone, Copy, Debug)]
struct PhysicalSpan {
    byte_start: u64,
    byte_end: u64,
    utf16_start: u64,
    utf16_end: u64,
}

/// Fuelled sequential decoder over one active unpublished terminal fragment.
///
/// A cursor caches at most one packed leaf, one source window and one bounded
/// ready chunk. Range replay uses the same type with a private yield interval.
#[must_use = "terminal-fragment cursors must reach completion or be discarded with the build"]
pub struct M11RecursiveGreenTerminalFragmentCursor {
    stamp: M11RecursiveGreenTerminalFragmentStamp,
    next_event: u64,
    physical_position: u64,
    physical_utf16_position: u64,
    open: Vec<M11RecursiveGreenFrameId>,
    leaf: Vec<u8>,
    leaf_event_cursor: usize,
    leaf_events_remaining: u16,
    atom: Option<ProjectedAtom>,
    ready_bytes: Vec<u8>,
    ready_raw_contributions: Vec<u8>,
    ready_start: usize,
    ready_base_offset: u64,
    available_bytes: u64,
    logical_utf16: u64,
    last_raw_contribution: Option<(u64, u8)>,
    yield_bytes: Range<u64>,
    expected_yield_utf16: Option<Range<u64>>,
    range_authority: Option<M11RecursiveGreenTerminalFragmentRange>,
    yielded_bytes: u64,
    yielded_physical: Option<PhysicalSpan>,
    complete: bool,
}

impl M11RecursiveGreenTerminalFragmentCursor {
    fn new(
        stamp: M11RecursiveGreenTerminalFragmentStamp,
        yield_bytes: Range<u64>,
        expected_yield_utf16: Option<Range<u64>>,
        range_authority: Option<M11RecursiveGreenTerminalFragmentRange>,
    ) -> Result<Self, M11RecursiveGreenError> {
        let mut leaf = Vec::new();
        leaf.try_reserve_exact(ARENA_PAGE_BYTES)
            .map_err(|_| M11RecursiveGreenError::InvalidState)?;
        let mut open = Vec::new();
        open.try_reserve_exact(16)
            .map_err(|_| M11RecursiveGreenError::InvalidState)?;
        let mut ready_bytes = Vec::new();
        ready_bytes
            .try_reserve_exact(SOURCE_CURSOR_WINDOW_BYTES)
            .map_err(|_| M11RecursiveGreenError::InvalidState)?;
        let mut ready_raw_contributions = Vec::new();
        ready_raw_contributions
            .try_reserve_exact(SOURCE_CURSOR_WINDOW_BYTES)
            .map_err(|_| M11RecursiveGreenError::InvalidState)?;
        open.push(stamp.frame);
        let empty_at_origin = yield_bytes.start == 0 && yield_bytes.end == 0;
        Ok(Self {
            stamp,
            next_event: stamp
                .event_ordinal
                .checked_add(1)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?,
            physical_position: stamp.source_before.bytes(),
            physical_utf16_position: stamp.source_before.utf16(),
            open,
            leaf,
            leaf_event_cursor: 0,
            leaf_events_remaining: 0,
            atom: None,
            ready_bytes,
            ready_raw_contributions,
            ready_start: 0,
            ready_base_offset: 0,
            available_bytes: 0,
            logical_utf16: 0,
            last_raw_contribution: None,
            yield_bytes,
            expected_yield_utf16,
            range_authority,
            yielded_bytes: 0,
            yielded_physical: empty_at_origin.then_some(PhysicalSpan {
                byte_start: stamp.source_before.bytes(),
                byte_end: stamp.source_before.bytes(),
                utf16_start: stamp.source_before.utf16(),
                utf16_end: stamp.source_before.utf16(),
            }),
            complete: empty_at_origin,
        })
    }

    #[must_use]
    pub const fn available_len(&self) -> u64 {
        self.yielded_bytes
    }

    #[must_use]
    pub const fn is_final(&self) -> bool {
        self.complete
    }

    #[must_use]
    pub fn ready_byte(&self) -> Option<M11RecursiveGreenProjectedByte> {
        let index = self.ready_start;
        Some(M11RecursiveGreenProjectedByte {
            relative_offset: self
                .ready_base_offset
                .checked_add(u64::try_from(index).ok()?)?,
            byte: *self.ready_bytes.get(index)?,
        })
    }

    /// The next sequential projected bytes, bounded by one source window.
    #[must_use]
    pub fn ready_chunk(&self) -> &[u8] {
        &self.ready_bytes[self.ready_start..]
    }

    /// Consumes one ready byte at its exact sequential offset.
    pub fn read_byte(&mut self, relative_offset: u64) -> Result<u8, M11RecursiveGreenError> {
        let index = self.ready_start;
        let expected = self
            .ready_base_offset
            .checked_add(u64::try_from(index).map_err(|_| M11RecursiveGreenError::CounterOverflow)?)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        if expected != relative_offset {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        let byte = *self
            .ready_bytes
            .get(index)
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        self.consume_ready_prefix(1)?;
        Ok(byte)
    }

    /// Consumes a sequential prefix of the bounded ready chunk.
    pub fn consume_ready_prefix(&mut self, len: usize) -> Result<(), M11RecursiveGreenError> {
        if len == 0 || len > self.ready_chunk().len() {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        let last = self
            .ready_start
            .checked_add(len - 1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let offset = self
            .ready_base_offset
            .checked_add(u64::try_from(last).map_err(|_| M11RecursiveGreenError::CounterOverflow)?)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let contribution = *self
            .ready_raw_contributions
            .get(last)
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        self.last_raw_contribution = Some((offset, contribution));
        self.ready_start = self
            .ready_start
            .checked_add(len)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        if self.ready_start == self.ready_bytes.len() {
            self.ready_bytes.clear();
            self.ready_raw_contributions.clear();
            self.ready_start = 0;
            self.ready_base_offset = self.yielded_bytes;
        }
        Ok(())
    }

    /// Returns the physical-codepoint contribution associated with the last
    /// byte of one completed logical scalar. Intermediate UTF-8 bytes and tab
    /// continuation spaces deliberately return zero.
    #[must_use]
    pub fn raw_codepoint_contribution(&self, relative_offset: u64) -> u8 {
        self.last_raw_contribution
            .filter(|(offset, _)| *offset == relative_offset)
            .map_or(0, |(_, contribution)| contribution)
    }

    #[must_use]
    pub const fn logical_position(&self) -> M11RecursiveGreenLogicalPosition {
        M11RecursiveGreenLogicalPosition {
            bytes: self.available_bytes,
            utf16: self.logical_utf16,
        }
    }

    /// Returns the revalidated linear range only after replay crossed both
    /// metric endpoints exactly.
    pub fn take_completed_range(
        &mut self,
    ) -> Result<M11RecursiveGreenTerminalFragmentRange, M11RecursiveGreenError> {
        if !self.complete || !self.ready_chunk().is_empty() {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        let mut authority = self
            .range_authority
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        authority.physical =
            self.yielded_physical
                .map(|physical| M11RecursiveGreenTerminalFragmentPhysicalRange {
                    bytes: physical.byte_start..physical.byte_end,
                    utf16: physical.utf16_start..physical.utf16_end,
                });
        authority.replay_validated = true;
        Ok(authority)
    }

    fn should_yield(&self, logical_offset: u64) -> bool {
        self.yield_bytes.contains(&logical_offset)
    }
}

impl M11RecursiveGreenBuild {
    fn validate_fragment_binding(
        &self,
        binding: &M11RecursiveGreenTerminalFragmentBinding,
    ) -> Result<M11RecursiveGreenTerminalFragmentStamp, M11RecursiveGreenError> {
        if self.phase != BuildPhase::FragmentFrozen
            || self.active_fragment != Some(binding.stamp)
            || binding.stamp.runtime_identity != self.runtime_identity
            || binding.stamp.source != self.source
            || self.working_prefix.is_none()
            || self.builder.is_some()
        {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        Ok(binding.stamp)
    }

    pub fn open_terminal_fragment_cursor(
        &self,
        binding: &M11RecursiveGreenTerminalFragmentBinding,
    ) -> Result<M11RecursiveGreenTerminalFragmentCursor, M11RecursiveGreenError> {
        let stamp = self.validate_fragment_binding(binding)?;
        M11RecursiveGreenTerminalFragmentCursor::new(stamp, 0..u64::MAX, None, None)
    }

    /// Mints a range capability bound to this exact barrier. A later replay
    /// revalidates both UTF-8 and UTF-16 endpoints while crossing them.
    pub fn bind_terminal_fragment_logical_range(
        &self,
        binding: &M11RecursiveGreenTerminalFragmentBinding,
        range: M11RecursiveGreenLogicalRange,
    ) -> Result<M11RecursiveGreenTerminalFragmentRange, M11RecursiveGreenError> {
        let stamp = self.validate_fragment_binding(binding)?;
        let maximum = stamp
            .logical_end
            .bytes()
            .checked_sub(stamp.logical_before.bytes())
            .ok_or(M11RecursiveGreenError::Corrupt(
                "fragment logical summary moved backwards",
            ))?;
        if range.bytes.start > range.bytes.end
            || range.utf16.start > range.utf16.end
            || range.bytes.end > maximum
        {
            return Err(M11RecursiveGreenError::InvalidPoint);
        }
        Ok(M11RecursiveGreenTerminalFragmentRange {
            stamp,
            range,
            physical: None,
            replay_validated: false,
        })
    }

    pub fn open_terminal_fragment_range_replay(
        &self,
        binding: &M11RecursiveGreenTerminalFragmentBinding,
        range: M11RecursiveGreenTerminalFragmentRange,
    ) -> Result<M11RecursiveGreenTerminalFragmentCursor, M11RecursiveGreenError> {
        let stamp = self.validate_fragment_binding(binding)?;
        if range.stamp != stamp {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        let bytes = range.range.bytes.clone();
        let utf16 = range.range.utf16.clone();
        M11RecursiveGreenTerminalFragmentCursor::new(stamp, bytes, Some(utf16), Some(range))
    }

    /// Retargets a completed replay to a later range without discarding its
    /// decoded event, projection, or open-frame position.
    ///
    /// Reference definitions are emitted in source order. Keeping one cursor
    /// at that monotonic boundary avoids replaying the already authenticated
    /// Paragraph prefix for every nested label and value range.
    pub fn retarget_terminal_fragment_range_replay_forward(
        &self,
        binding: &M11RecursiveGreenTerminalFragmentBinding,
        cursor: &mut M11RecursiveGreenTerminalFragmentCursor,
        range: M11RecursiveGreenTerminalFragmentRange,
    ) -> Result<(), M11RecursiveGreenError> {
        let stamp = self.validate_fragment_binding(binding)?;
        let empty_at_cursor = range.range.bytes.start == range.range.bytes.end
            && range.range.utf16.start == range.range.utf16.end
            && range.range.bytes.start == cursor.available_bytes
            && range.range.utf16.start == cursor.logical_utf16;
        let empty_physical = if empty_at_cursor {
            let endpoint = match cursor.yielded_physical {
                Some(physical) => (physical.byte_end, physical.utf16_end),
                None if cursor.available_bytes == 0 && cursor.logical_utf16 == 0 => {
                    (stamp.source_before.bytes(), stamp.source_before.utf16())
                }
                None => return Err(M11RecursiveGreenError::InvalidState),
            };
            Some(PhysicalSpan {
                byte_start: endpoint.0,
                byte_end: endpoint.0,
                utf16_start: endpoint.1,
                utf16_end: endpoint.1,
            })
        } else {
            None
        };
        if range.stamp != stamp
            || cursor.stamp != stamp
            || !cursor.complete
            || !cursor.ready_chunk().is_empty()
            || cursor.range_authority.is_some()
            || range.range.bytes.start < cursor.available_bytes
            || range.range.utf16.start < cursor.logical_utf16
            || range.range.bytes.start > range.range.bytes.end
            || range.range.utf16.start > range.range.utf16.end
        {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        cursor.yield_bytes = range.range.bytes.clone();
        cursor.expected_yield_utf16 = Some(range.range.utf16.clone());
        cursor.range_authority = Some(range);
        cursor.yielded_bytes = 0;
        cursor.yielded_physical = empty_physical;
        cursor.last_raw_contribution = None;
        cursor.ready_base_offset = 0;
        cursor.complete = empty_at_cursor;
        Ok(())
    }

    pub fn poll_terminal_fragment_cursor(
        &mut self,
        runtime: &mut DocumentRuntime,
        cursor: &mut M11RecursiveGreenTerminalFragmentCursor,
        fuel: usize,
    ) -> Result<M11RecursiveGreenTerminalFragmentCursorPoll, M11RecursiveGreenError> {
        if fuel == 0 {
            return Err(M11RecursiveGreenError::ZeroFuel);
        }
        self.ensure_fragment_cursor(runtime, cursor)?;
        if !cursor.ready_chunk().is_empty() {
            return Ok(cursor_poll(
                M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady,
                0,
            ));
        }
        if cursor.complete {
            return Ok(cursor_poll(
                M11RecursiveGreenTerminalFragmentCursorStatus::Complete,
                0,
            ));
        }

        let mut transitions = 0;
        while transitions < fuel {
            if cursor.atom.is_some() {
                self.step_fragment_atom(cursor)?;
                transitions += 1;
                if !cursor.ready_chunk().is_empty() {
                    return Ok(cursor_poll(
                        M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady,
                        transitions,
                    ));
                }
                continue;
            }
            if cursor.next_event == cursor.stamp.events_end {
                if cursor.open != [cursor.stamp.frame]
                    || cursor.physical_position != cursor.stamp.source_end.bytes()
                {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "terminal-fragment cursor changed its frozen boundary",
                    ));
                }
                if let Some(expected) = &cursor.expected_yield_utf16 {
                    let actual_end = cursor.logical_utf16;
                    if cursor.available_bytes < cursor.yield_bytes.end || actual_end < expected.end
                    {
                        return Err(M11RecursiveGreenError::InvalidPoint);
                    }
                }
                cursor.complete = true;
                return Ok(cursor_poll(
                    M11RecursiveGreenTerminalFragmentCursorStatus::Complete,
                    transitions,
                ));
            }
            if cursor.leaf_events_remaining == 0 {
                self.load_fragment_leaf(runtime, cursor)?;
                transitions += 1;
                continue;
            }
            self.step_fragment_event(cursor)?;
            transitions += 1;
        }
        Ok(cursor_poll(
            M11RecursiveGreenTerminalFragmentCursorStatus::Pending,
            transitions,
        ))
    }

    /// Advances by bounded projection quanta and exposes up to one 4 KiB
    /// source window as a sequential ready chunk. One unit of caller fuel
    /// performs at most `SOURCE_CURSOR_WINDOW_BYTES` projection/event steps.
    pub fn poll_terminal_fragment_cursor_chunk(
        &mut self,
        runtime: &mut DocumentRuntime,
        cursor: &mut M11RecursiveGreenTerminalFragmentCursor,
        fuel: usize,
    ) -> Result<M11RecursiveGreenTerminalFragmentCursorPoll, M11RecursiveGreenError> {
        if fuel == 0 {
            return Err(M11RecursiveGreenError::ZeroFuel);
        }
        self.ensure_fragment_cursor(runtime, cursor)?;
        if !cursor.ready_chunk().is_empty() {
            return Ok(cursor_poll(
                M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady,
                0,
            ));
        }
        if cursor.complete {
            return Ok(cursor_poll(
                M11RecursiveGreenTerminalFragmentCursorStatus::Complete,
                0,
            ));
        }

        let mut transitions = 0;
        while transitions < fuel {
            let mut quantum_steps = 0;
            while cursor.ready_chunk().len() < SOURCE_CURSOR_WINDOW_BYTES
                && quantum_steps < SOURCE_CURSOR_WINDOW_BYTES
                && !cursor.complete
            {
                if cursor.atom.is_some() {
                    self.step_fragment_atom(cursor)?;
                    quantum_steps += 1;
                    continue;
                }
                if cursor.next_event == cursor.stamp.events_end {
                    if cursor.open != [cursor.stamp.frame]
                        || cursor.physical_position != cursor.stamp.source_end.bytes()
                    {
                        return Err(M11RecursiveGreenError::Corrupt(
                            "terminal-fragment cursor changed its frozen boundary",
                        ));
                    }
                    if let Some(expected) = &cursor.expected_yield_utf16 {
                        let actual_end = cursor.logical_utf16;
                        if cursor.available_bytes < cursor.yield_bytes.end
                            || actual_end < expected.end
                        {
                            return Err(M11RecursiveGreenError::InvalidPoint);
                        }
                    }
                    cursor.complete = true;
                    break;
                }
                if cursor.leaf_events_remaining == 0 {
                    self.load_fragment_leaf(runtime, cursor)?;
                    quantum_steps += 1;
                    continue;
                }
                self.step_fragment_event(cursor)?;
                quantum_steps += 1;
            }
            if quantum_steps > 0 {
                transitions += 1;
            }
            if !cursor.ready_chunk().is_empty() {
                return Ok(cursor_poll(
                    M11RecursiveGreenTerminalFragmentCursorStatus::ByteReady,
                    transitions,
                ));
            }
            if cursor.complete {
                return Ok(cursor_poll(
                    M11RecursiveGreenTerminalFragmentCursorStatus::Complete,
                    transitions,
                ));
            }
        }
        Ok(cursor_poll(
            M11RecursiveGreenTerminalFragmentCursorStatus::Pending,
            transitions,
        ))
    }

    fn ensure_fragment_cursor(
        &self,
        runtime: &DocumentRuntime,
        cursor: &M11RecursiveGreenTerminalFragmentCursor,
    ) -> Result<(), M11RecursiveGreenError> {
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11RecursiveGreenError::WrongRuntime);
        }
        if runtime.current_source_version() != Some(self.source) {
            return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
        }
        if self.phase != BuildPhase::FragmentFrozen
            || self.active_fragment != Some(cursor.stamp)
            || self.working_prefix.is_none()
        {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        Ok(())
    }

    fn load_fragment_leaf(
        &mut self,
        runtime: &mut DocumentRuntime,
        cursor: &mut M11RecursiveGreenTerminalFragmentCursor,
    ) -> Result<(), M11RecursiveGreenError> {
        let build = self
            .build
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let session = runtime.producer_arena_mut().resume_build(build)?;
        let result: Result<(u64, u64), M11RecursiveGreenError> = (|| {
            let root = self
                .working_prefix
                .as_ref()
                .ok_or(M11RecursiveGreenError::InvalidState)?
                .as_ref(&session)?;
            let mut inspection = SequenceInspectionReceipt::default();
            let located = root
                .locate_leaf_containing_metric(
                    session.arena(),
                    cursor.next_event,
                    |summary| summary.events,
                    &mut inspection,
                )?
                .ok_or(M11RecursiveGreenError::Corrupt(
                    "fragment event has no packed leaf",
                ))?;
            let payload = session.arena().payload(located.id)?;
            if payload.len() > ARENA_PAGE_BYTES {
                return Err(M11RecursiveGreenError::Corrupt(
                    "fragment leaf exceeds the arena page bound",
                ));
            }
            cursor.leaf.clear();
            cursor.leaf.extend_from_slice(payload);
            let prefix_events = located.prefix.map_or(0, |summary| summary.events);
            Ok((prefix_events, located.ordinal))
        })();
        self.build = Some(session.suspend()?);
        let (prefix_events, _leaf_ordinal) = result?;

        let mut inspection = SequenceSpecInspection::default();
        let decoded = decode_leaf(&cursor.leaf, &mut inspection)?.ok_or(
            M11RecursiveGreenError::Corrupt("fragment route reached a non-Green leaf"),
        )?;
        let skip = cursor
            .next_event
            .checked_sub(prefix_events)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        if skip >= usize::from(decoded.events) {
            return Err(M11RecursiveGreenError::Corrupt(
                "fragment leaf event offset is outside the leaf",
            ));
        }
        let mut event_cursor = 0;
        for _ in 0..skip {
            let _ = decode_packed_event(decoded.event_bytes, &mut event_cursor)?;
        }
        cursor.leaf_event_cursor = event_cursor;
        cursor.leaf_events_remaining = decoded.events
            - u16::try_from(skip).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        Ok(())
    }

    fn step_fragment_event(
        &self,
        cursor: &mut M11RecursiveGreenTerminalFragmentCursor,
    ) -> Result<(), M11RecursiveGreenError> {
        let mut inspection = SequenceSpecInspection::default();
        let decoded = decode_leaf(&cursor.leaf, &mut inspection)?.ok_or(
            M11RecursiveGreenError::Corrupt("fragment cache is not a Green leaf"),
        )?;
        let event = decode_packed_event(decoded.event_bytes, &mut cursor.leaf_event_cursor)?;
        cursor.leaf_events_remaining = cursor
            .leaf_events_remaining
            .checked_sub(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        cursor.next_event = cursor
            .next_event
            .checked_add(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        match event {
            PackedGreenEvent::Enter { frame, .. } => cursor.open.push(frame),
            PackedGreenEvent::RetypeOpen { frame, .. } => {
                if cursor.open.last().copied() != Some(frame) {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "fragment retype crossed its open path",
                    ));
                }
            }
            PackedGreenEvent::Exit { frame, .. } => {
                if cursor.open.pop() != Some(frame) || frame == cursor.stamp.frame {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "frozen terminal fragment closed before its barrier",
                    ));
                }
            }
            PackedGreenEvent::Coverage {
                physical,
                owner_depth,
                atom,
                ..
            } => {
                let start = cursor.physical_position;
                let start_utf16 = cursor.physical_utf16_position;
                cursor.physical_position = start
                    .checked_add(physical.bytes())
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                cursor.physical_utf16_position = start_utf16
                    .checked_add(physical.utf16())
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                let target_depth = u32::try_from(cursor.open.len() - 1)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
                let logical_targets_fragment = match atom {
                    LogicalAtom::TabToSpaces {
                        target_owner_depth, ..
                    } => target_owner_depth == target_depth,
                    LogicalAtom::None | LogicalAtom::HiddenUpstream => false,
                    _ => owner_depth == target_depth,
                };
                if logical_targets_fragment {
                    cursor.atom = projection_atom(
                        self.lease
                            .as_ref()
                            .ok_or(M11RecursiveGreenError::InvalidState)?,
                        start,
                        cursor.physical_position,
                        start_utf16,
                        cursor.physical_utf16_position,
                        atom,
                    )?;
                }
            }
            PackedGreenEvent::Property(_) => {}
        }
        Ok(())
    }

    fn step_fragment_atom(
        &self,
        cursor: &mut M11RecursiveGreenTerminalFragmentCursor,
    ) -> Result<(), M11RecursiveGreenError> {
        let logical_offset = cursor.available_bytes;
        if logical_offset == cursor.yield_bytes.start {
            if let Some(expected) = &cursor.expected_yield_utf16 {
                if cursor.logical_utf16 != expected.start {
                    return Err(M11RecursiveGreenError::InvalidPoint);
                }
            }
        }
        let (byte, utf16_increment, raw_contribution, finished, physical) = match cursor
            .atom
            .as_mut()
            .ok_or(M11RecursiveGreenError::InvalidState)?
        {
            ProjectedAtom::Identity {
                source,
                physical_bytes,
                physical_utf16,
                scalar_remaining,
                scalar_utf16,
            } => {
                let byte_start = *physical_bytes;
                let utf16_start = *physical_utf16;
                let byte = source.next_byte().ok_or(M11RecursiveGreenError::Corrupt(
                    "identity projection ended before its physical metric",
                ))?;
                if *scalar_remaining == 0 {
                    *scalar_remaining = utf8_width(byte).ok_or(M11RecursiveGreenError::Corrupt(
                        "identity projection is invalid UTF-8",
                    ))?;
                    *scalar_utf16 = if *scalar_remaining == 4 { 2 } else { 1 };
                }
                *scalar_remaining -= 1;
                let scalar_end = *scalar_remaining == 0;
                let finished = source.position() == source.end();
                *physical_bytes = physical_bytes
                    .checked_add(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                if scalar_end {
                    *physical_utf16 = physical_utf16
                        .checked_add(u64::from(*scalar_utf16))
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                }
                (
                    byte,
                    if scalar_end { *scalar_utf16 } else { 0 },
                    u8::from(scalar_end),
                    finished,
                    PhysicalSpan {
                        byte_start,
                        byte_end: *physical_bytes,
                        utf16_start,
                        utf16_end: *physical_utf16,
                    },
                )
            }
            ProjectedAtom::Static {
                bytes,
                len,
                next,
                scalar_utf16_at_end,
                raw_contribution_at_end,
                physical,
            } => {
                let byte = bytes[usize::from(*next)];
                *next += 1;
                let finished = *next == *len;
                (
                    byte,
                    if finished { *scalar_utf16_at_end } else { 0 },
                    if finished {
                        *raw_contribution_at_end
                    } else {
                        0
                    },
                    finished,
                    *physical,
                )
            }
            ProjectedAtom::Spaces {
                remaining,
                first,
                physical,
            } => {
                let contribution = u8::from(*first);
                *first = false;
                *remaining -= 1;
                (b' ', 1, contribution, *remaining == 0, *physical)
            }
        };
        cursor.available_bytes = cursor
            .available_bytes
            .checked_add(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        cursor.logical_utf16 = cursor
            .logical_utf16
            .checked_add(u64::from(utf16_increment))
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        if cursor.should_yield(logical_offset) {
            cursor.yielded_physical = Some(match cursor.yielded_physical {
                Some(existing) => PhysicalSpan {
                    byte_start: existing.byte_start,
                    utf16_start: existing.utf16_start,
                    byte_end: physical.byte_end,
                    utf16_end: physical.utf16_end,
                },
                None => physical,
            });
            if cursor.ready_bytes.is_empty() {
                cursor.ready_base_offset = cursor.yielded_bytes;
            }
            cursor.ready_bytes.push(byte);
            cursor.ready_raw_contributions.push(raw_contribution);
            cursor.yielded_bytes = cursor
                .yielded_bytes
                .checked_add(1)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        }
        if cursor.yield_bytes.start == cursor.yield_bytes.end
            && cursor.available_bytes == cursor.yield_bytes.end
            && cursor.yielded_physical.is_none()
        {
            cursor.yielded_physical = Some(PhysicalSpan {
                byte_start: physical.byte_end,
                byte_end: physical.byte_end,
                utf16_start: physical.utf16_end,
                utf16_end: physical.utf16_end,
            });
        }
        if finished {
            cursor.atom = None;
        }
        if cursor.available_bytes == cursor.yield_bytes.end {
            if let Some(expected) = &cursor.expected_yield_utf16 {
                if cursor.logical_utf16 != expected.end {
                    return Err(M11RecursiveGreenError::InvalidPoint);
                }
            }
            cursor.complete = true;
        }
        Ok(())
    }
}

/// Canonical terminal-fragment outcomes selected by an upstream grammar.
/// Storage sees only an authenticated logical cut and the structural policy.
pub enum M11RecursiveGreenTerminalFragmentRewrite {
    /// The recognizer found no semantic prefix. Packed Green is unchanged.
    Unchanged,
    /// Removes the terminal wrapper and rehomes its complete physical source
    /// under the parent as non-logical Gap coverage.
    RemoveWrapper {
        whole_fragment: M11RecursiveGreenTerminalFragmentRange,
    },
    /// Keeps the terminal wrapper while removing one accepted logical prefix.
    RetainVisibleSuffix {
        removed_prefix: M11RecursiveGreenTerminalFragmentRange,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenTerminalFragmentDisposition {
    Surviving,
    Removed,
}

/// Linear receipt proving which frame authority survived the canonical splice.
#[must_use = "rewrite authority must be joined by the writer"]
pub struct M11RecursiveGreenTerminalFragmentRewriteAuthority {
    stamp: M11RecursiveGreenTerminalFragmentStamp,
    disposition: M11RecursiveGreenTerminalFragmentDisposition,
    visible_remainder_boundary: Option<super::adopt::M11RecursiveGreenStructuralBoundary>,
}

impl M11RecursiveGreenTerminalFragmentRewriteAuthority {
    #[must_use]
    pub const fn frame(&self) -> M11RecursiveGreenFrameId {
        self.stamp.frame
    }

    #[must_use]
    pub const fn disposition(&self) -> M11RecursiveGreenTerminalFragmentDisposition {
        self.disposition
    }

    /// Takes the authenticated structural cut between a removed logical
    /// prefix and its surviving terminal-frame suffix, when this authority
    /// completed a `RetainVisibleSuffix` rewrite.
    #[doc(hidden)]
    pub fn take_visible_remainder_boundary(
        &mut self,
    ) -> Option<super::adopt::M11RecursiveGreenStructuralBoundary> {
        self.visible_remainder_boundary.take()
    }
}

pub enum M11RecursiveGreenTerminalFragmentRewritePoll {
    Pending {
        transitions: usize,
    },
    Complete {
        transitions: usize,
        authority: M11RecursiveGreenTerminalFragmentRewriteAuthority,
    },
}

impl M11RecursiveGreenTerminalFragmentRewritePoll {
    #[must_use]
    pub const fn transitions(&self) -> usize {
        match self {
            Self::Pending { transitions } | Self::Complete { transitions, .. } => *transitions,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RewriteMode {
    Unchanged,
    Remove { cut_bytes: u64, cut_utf16: u64 },
    Retain { cut_bytes: u64, cut_utf16: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RewritePhase {
    Unchanged,
    LoadLeaf,
    ScanEvent,
    PushPage,
    BeginFinish,
    Reduce,
    TakeReplacement,
    Splice,
    Complete,
}

/// One bounded, fuelled canonical replacement of the frozen Green suffix.
#[must_use = "terminal-fragment rewrites must be polled to completion or followed by build cancellation"]
pub struct M11RecursiveGreenTerminalFragmentRewriteWork {
    stamp: M11RecursiveGreenTerminalFragmentStamp,
    mode: RewriteMode,
    phase: RewritePhase,
    base: Option<GreenSequenceBuildRoot>,
    replacement: Option<ResumableMeasuredSequenceBuilder<RecursiveGreenSpec>>,
    replacement_root: Option<MeasuredSequenceBuildRoot<RecursiveGreenSpec>>,
    first_leaf: u64,
    first_event: u64,
    next_leaf: u64,
    end_leaf: u64,
    next_original_event: u64,
    leaf: Vec<u8>,
    leaf_event_cursor: usize,
    leaf_events_remaining: u16,
    pending_output: [Option<PackedGreenEvent>; 2],
    pending_output_next: u8,
    pending_output_len: u8,
    boundary_after_pending_output: Option<u8>,
    emitted_events: u64,
    visible_remainder_event_cut: Option<u64>,
    visible_remainder_physical: Option<M11RecursiveGreenSourceMetric>,
    page: [u8; ARENA_PAGE_BYTES],
    page_len: usize,
    page_events: u16,
    page_summary: RecursiveGreenSummary,
    fragment_started: bool,
    physical_position: u64,
    logical_bytes: u64,
    logical_utf16: u64,
}

impl M11RecursiveGreenBuild {
    pub fn begin_terminal_fragment_rewrite(
        &mut self,
        runtime: &mut DocumentRuntime,
        binding: M11RecursiveGreenTerminalFragmentBinding,
        rewrite: M11RecursiveGreenTerminalFragmentRewrite,
    ) -> Result<M11RecursiveGreenTerminalFragmentRewriteWork, M11RecursiveGreenError> {
        let stamp = self.validate_fragment_binding(&binding)?;
        let mut visible_remainder_physical = None;
        let mode = match rewrite {
            M11RecursiveGreenTerminalFragmentRewrite::Unchanged => RewriteMode::Unchanged,
            M11RecursiveGreenTerminalFragmentRewrite::RemoveWrapper { whole_fragment } => {
                validate_rewrite_range(stamp, &whole_fragment)?;
                if whole_fragment.range.bytes.start != 0 || whole_fragment.range.utf16.start != 0 {
                    return Err(M11RecursiveGreenError::InvalidPoint);
                }
                RewriteMode::Remove {
                    cut_bytes: whole_fragment.range.bytes.end,
                    cut_utf16: whole_fragment.range.utf16.end,
                }
            }
            M11RecursiveGreenTerminalFragmentRewrite::RetainVisibleSuffix { removed_prefix } => {
                validate_rewrite_range(stamp, &removed_prefix)?;
                if removed_prefix.range.bytes.start != 0 || removed_prefix.range.utf16.start != 0 {
                    return Err(M11RecursiveGreenError::InvalidPoint);
                }
                let physical = removed_prefix
                    .physical_range()
                    .ok_or(M11RecursiveGreenError::InvalidState)?;
                let bytes = physical.byte_range();
                let utf16 = physical.utf16_range();
                visible_remainder_physical = Some(M11RecursiveGreenSourceMetric::from_validated(
                    bytes.end, utf16.end,
                ));
                RewriteMode::Retain {
                    cut_bytes: removed_prefix.range.bytes.end,
                    cut_utf16: removed_prefix.range.utf16.end,
                }
            }
        };
        let base = self
            .working_prefix
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let (first_leaf, end_leaf, first_event) =
            self.fragment_rewrite_bounds(runtime, &base, stamp.event_ordinal)?;
        let mut leaf = Vec::new();
        leaf.try_reserve_exact(ARENA_PAGE_BYTES)
            .map_err(|_| M11RecursiveGreenError::InvalidState)?;
        self.phase = BuildPhase::FragmentRewriting;
        Ok(M11RecursiveGreenTerminalFragmentRewriteWork {
            stamp,
            mode,
            phase: if mode == RewriteMode::Unchanged {
                RewritePhase::Unchanged
            } else {
                RewritePhase::LoadLeaf
            },
            base: Some(base),
            replacement: None,
            replacement_root: None,
            first_leaf,
            first_event,
            next_leaf: first_leaf,
            end_leaf,
            next_original_event: first_event,
            leaf,
            leaf_event_cursor: 0,
            leaf_events_remaining: 0,
            pending_output: [None, None],
            pending_output_next: 0,
            pending_output_len: 0,
            boundary_after_pending_output: None,
            emitted_events: 0,
            visible_remainder_event_cut: None,
            visible_remainder_physical,
            page: [0; ARENA_PAGE_BYTES],
            page_len: GREEN_LEAF_HEADER_BYTES,
            page_events: 0,
            page_summary: RecursiveGreenSummary::empty(),
            fragment_started: false,
            physical_position: stamp.source_before.bytes(),
            logical_bytes: 0,
            logical_utf16: 0,
        })
    }

    pub fn poll_terminal_fragment_rewrite(
        &mut self,
        runtime: &mut DocumentRuntime,
        work: &mut M11RecursiveGreenTerminalFragmentRewriteWork,
        fuel: usize,
    ) -> Result<M11RecursiveGreenTerminalFragmentRewritePoll, M11RecursiveGreenError> {
        if fuel == 0 {
            return Err(M11RecursiveGreenError::ZeroFuel);
        }
        if runtime.producer_identity() != self.runtime_identity {
            return Err(M11RecursiveGreenError::WrongRuntime);
        }
        if runtime.current_source_version() != Some(self.source)
            || self.phase != BuildPhase::FragmentRewriting
            || self.active_fragment != Some(work.stamp)
        {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        let mut transitions = 0;
        while transitions < fuel {
            match work.phase {
                RewritePhase::Unchanged => {
                    self.working_prefix = work.base.take();
                    return self.complete_fragment_rewrite(work, transitions + 1);
                }
                RewritePhase::LoadLeaf => {
                    self.load_rewrite_leaf(runtime, work)?;
                    work.phase = RewritePhase::ScanEvent;
                }
                RewritePhase::ScanEvent => {
                    if work.pending_output_next < work.pending_output_len {
                        let event = work.pending_output[usize::from(work.pending_output_next)]
                            .ok_or(M11RecursiveGreenError::InvalidState)?;
                        if work.page_events > 0
                            && (usize::from(work.page_events) >= GREEN_EVENTS_PER_PAGE_MAX
                                || work.page_len + packed_event_len(event) > ARENA_PAGE_BYTES)
                        {
                            self.begin_rewrite_page_push(runtime, work)?;
                            work.phase = RewritePhase::PushPage;
                        } else {
                            let output_index = work.pending_output_next;
                            append_rewrite_event(work, event)?;
                            work.emitted_events = work
                                .emitted_events
                                .checked_add(1)
                                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                            if work.boundary_after_pending_output == Some(output_index) {
                                work.visible_remainder_event_cut = Some(
                                    work.first_event
                                        .checked_add(work.emitted_events)
                                        .ok_or(M11RecursiveGreenError::CounterOverflow)?,
                                );
                                work.boundary_after_pending_output = None;
                            }
                            work.pending_output_next += 1;
                            if work.pending_output_next == work.pending_output_len {
                                work.pending_output = [None, None];
                                work.pending_output_next = 0;
                                work.pending_output_len = 0;
                            }
                        }
                    } else if work.leaf_events_remaining > 0 {
                        self.rewrite_next_event(work)?;
                    } else if work.next_leaf < work.end_leaf {
                        work.phase = RewritePhase::LoadLeaf;
                    } else if work.page_events > 0 {
                        self.begin_rewrite_page_push(runtime, work)?;
                        work.phase = RewritePhase::PushPage;
                    } else {
                        work.phase = RewritePhase::BeginFinish;
                    }
                }
                RewritePhase::PushPage => {
                    let progress =
                        self.with_rewrite_session(runtime, work, |work, session, mutation| {
                            work.replacement
                                .as_mut()
                                .ok_or(M11RecursiveGreenError::InvalidState)?
                                .poll_push(session, mutation)
                        })?;
                    if progress == ResumableSequenceProgress::Complete {
                        work.phase = if work.next_leaf < work.end_leaf
                            || work.leaf_events_remaining > 0
                            || work.pending_output_len > 0
                        {
                            RewritePhase::ScanEvent
                        } else {
                            RewritePhase::BeginFinish
                        };
                    }
                }
                RewritePhase::BeginFinish => {
                    self.with_rewrite_session(runtime, work, |work, session, mutation| {
                        work.replacement
                            .as_mut()
                            .ok_or(M11RecursiveGreenError::InvalidState)?
                            .begin_finish(session, mutation)
                    })?;
                    work.phase = RewritePhase::Reduce;
                }
                RewritePhase::Reduce => {
                    let progress =
                        self.with_rewrite_session(runtime, work, |work, session, mutation| {
                            work.replacement
                                .as_mut()
                                .ok_or(M11RecursiveGreenError::InvalidState)?
                                .poll_finish(session, mutation)
                        })?;
                    if progress == ResumableSequenceProgress::Complete {
                        work.phase = RewritePhase::TakeReplacement;
                    }
                }
                RewritePhase::TakeReplacement => {
                    let build = self
                        .build
                        .take()
                        .ok_or(M11RecursiveGreenError::InvalidState)?;
                    let session = runtime.producer_arena_mut().resume_build(build)?;
                    let result = work
                        .replacement
                        .as_mut()
                        .ok_or(M11RecursiveGreenError::InvalidState)?
                        .take_root(&session);
                    self.build = Some(session.suspend()?);
                    work.replacement_root = Some(result?);
                    work.replacement = None;
                    work.phase = RewritePhase::Splice;
                }
                RewritePhase::Splice => {
                    self.finish_fragment_splice(runtime, work)?;
                    return self.complete_fragment_rewrite(work, transitions + 1);
                }
                RewritePhase::Complete => return Err(M11RecursiveGreenError::InvalidState),
            }
            transitions += 1;
        }
        Ok(M11RecursiveGreenTerminalFragmentRewritePoll::Pending { transitions })
    }

    fn fragment_rewrite_bounds(
        &mut self,
        runtime: &mut DocumentRuntime,
        base: &GreenSequenceBuildRoot,
        event: u64,
    ) -> Result<(u64, u64, u64), M11RecursiveGreenError> {
        let build = self
            .build
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let session = runtime.producer_arena_mut().resume_build(build)?;
        let result = (|| {
            let root = base.as_ref(&session)?;
            let mut inspection = SequenceInspectionReceipt::default();
            let measure = root
                .summary(session.arena(), &mut inspection)?
                .ok_or(M11RecursiveGreenError::InvalidState)?;
            let located = root
                .locate_leaf_containing_metric(
                    session.arena(),
                    event,
                    |summary| summary.events,
                    &mut inspection,
                )?
                .ok_or(M11RecursiveGreenError::InvalidPoint)?;
            Ok((
                located.ordinal,
                measure.leaves(),
                located.prefix.map_or(0, |summary| summary.events),
            ))
        })();
        self.build = Some(session.suspend()?);
        result
    }

    fn load_rewrite_leaf(
        &mut self,
        runtime: &mut DocumentRuntime,
        work: &mut M11RecursiveGreenTerminalFragmentRewriteWork,
    ) -> Result<(), M11RecursiveGreenError> {
        let build = self
            .build
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let session = runtime.producer_arena_mut().resume_build(build)?;
        let result: Result<(), M11RecursiveGreenError> = (|| {
            let root = work
                .base
                .as_ref()
                .ok_or(M11RecursiveGreenError::InvalidState)?
                .as_ref(&session)?;
            let mut inspection = SequenceInspectionReceipt::default();
            let located = root
                .locate_leaf_with_prefix(session.arena(), work.next_leaf, &mut inspection)?
                .ok_or(M11RecursiveGreenError::Corrupt(
                    "rewrite leaf ordinal is absent",
                ))?;
            let payload = session.arena().payload(located.id)?;
            work.leaf.clear();
            work.leaf.extend_from_slice(payload);
            Ok(())
        })();
        self.build = Some(session.suspend()?);
        result?;
        let mut inspection = SequenceSpecInspection::default();
        let decoded = decode_leaf(&work.leaf, &mut inspection)?.ok_or(
            M11RecursiveGreenError::Corrupt("rewrite route reached a non-Green leaf"),
        )?;
        work.leaf_event_cursor = 0;
        work.leaf_events_remaining = decoded.events;
        work.next_leaf = work
            .next_leaf
            .checked_add(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        Ok(())
    }

    fn rewrite_next_event(
        &self,
        work: &mut M11RecursiveGreenTerminalFragmentRewriteWork,
    ) -> Result<(), M11RecursiveGreenError> {
        let mut inspection = SequenceSpecInspection::default();
        let decoded = decode_leaf(&work.leaf, &mut inspection)?.ok_or(
            M11RecursiveGreenError::Corrupt("rewrite cache is not a Green leaf"),
        )?;
        let event = decode_packed_event(decoded.event_bytes, &mut work.leaf_event_cursor)?;
        work.leaf_events_remaining -= 1;
        let ordinal = work.next_original_event;
        work.next_original_event = ordinal
            .checked_add(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        if ordinal < work.stamp.event_ordinal {
            queue_rewrite_events(work, [Some(event), None]);
            return Ok(());
        }
        if ordinal == work.stamp.event_ordinal {
            if !matches!(
                event,
                PackedGreenEvent::Enter { frame, kind }
                    if frame == work.stamp.frame && kind == work.stamp.kind
            ) {
                return Err(M11RecursiveGreenError::Corrupt(
                    "fragment rewrite target is not its authenticated Enter",
                ));
            }
            work.fragment_started = true;
            if !matches!(work.mode, RewriteMode::Remove { .. }) {
                queue_rewrite_events(work, [Some(event), None]);
            }
            return Ok(());
        }
        if !work.fragment_started || ordinal >= work.stamp.events_end {
            return Err(M11RecursiveGreenError::Corrupt(
                "fragment rewrite crossed its event barrier",
            ));
        }
        let output = self.transform_fragment_event(work, event)?;
        queue_rewrite_events(work, output);
        Ok(())
    }

    fn transform_fragment_event(
        &self,
        work: &mut M11RecursiveGreenTerminalFragmentRewriteWork,
        event: PackedGreenEvent,
    ) -> Result<[Option<PackedGreenEvent>; 2], M11RecursiveGreenError> {
        match event {
            PackedGreenEvent::Enter { .. } | PackedGreenEvent::Exit { .. } => {
                Err(M11RecursiveGreenError::InvalidEvent)
            }
            PackedGreenEvent::Property(_) | PackedGreenEvent::RetypeOpen { .. }
                if matches!(work.mode, RewriteMode::Remove { .. }) =>
            {
                Ok([None, None])
            }
            PackedGreenEvent::Property(_) | PackedGreenEvent::RetypeOpen { .. } => {
                Ok([Some(event), None])
            }
            PackedGreenEvent::Coverage {
                physical,
                owner_depth,
                part,
                atom,
            } => self.transform_fragment_coverage(work, physical, owner_depth, part, atom),
        }
    }

    fn transform_fragment_coverage(
        &self,
        work: &mut M11RecursiveGreenTerminalFragmentRewriteWork,
        physical: M11RecursiveGreenSourceMetric,
        owner_depth: u32,
        part: M11RecursiveGreenCoveragePart,
        atom: LogicalAtom,
    ) -> Result<[Option<PackedGreenEvent>; 2], M11RecursiveGreenError> {
        let physical_start = work.physical_position;
        work.physical_position = physical_start
            .checked_add(physical.bytes())
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let logical_targets_fragment = match atom {
            LogicalAtom::TabToSpaces {
                target_owner_depth, ..
            } => target_owner_depth == 0,
            LogicalAtom::None | LogicalAtom::HiddenUpstream => false,
            _ => owner_depth == 0,
        };
        let logical = atom.logical_metric(physical);
        let logical_start = work.logical_bytes;
        let utf16_start = work.logical_utf16;
        if logical_targets_fragment {
            work.logical_bytes = logical_start
                .checked_add(logical.bytes())
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            work.logical_utf16 = utf16_start
                .checked_add(logical.utf16())
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        }

        match work.mode {
            RewriteMode::Unchanged => unreachable!("unchanged rewrite bypasses event scanning"),
            RewriteMode::Remove { cut_bytes, .. } => {
                let next_owner = owner_depth.saturating_sub(1);
                let next_part = if owner_depth == 0 {
                    M11RecursiveGreenCoveragePart::Gap
                } else {
                    part
                };
                let next_atom = if logical_targets_fragment {
                    LogicalAtom::None
                } else {
                    rebase_removed_atom(atom)?
                };
                if logical_targets_fragment && work.logical_bytes > cut_bytes {
                    return Err(M11RecursiveGreenError::InvalidPoint);
                }
                Ok([
                    Some(PackedGreenEvent::Coverage {
                        physical,
                        owner_depth: next_owner,
                        part: next_part,
                        atom: next_atom,
                    }),
                    None,
                ])
            }
            RewriteMode::Retain {
                cut_bytes,
                cut_utf16,
            } if logical_targets_fragment && logical_start < cut_bytes => {
                if work.logical_bytes <= cut_bytes {
                    let (next_owner, next_part) = if owner_depth == 0 {
                        (1, M11RecursiveGreenCoveragePart::Gap)
                    } else {
                        (owner_depth, part)
                    };
                    if work.logical_bytes == cut_bytes {
                        work.boundary_after_pending_output = Some(0);
                    }
                    return Ok([
                        Some(PackedGreenEvent::Coverage {
                            physical,
                            owner_depth: next_owner,
                            part: next_part,
                            atom: LogicalAtom::None,
                        }),
                        None,
                    ]);
                }
                let LogicalAtom::Identity = atom else {
                    return Err(M11RecursiveGreenError::InvalidPoint);
                };
                let prefix_bytes = cut_bytes - logical_start;
                let prefix_utf16 = cut_utf16
                    .checked_sub(utf16_start)
                    .ok_or(M11RecursiveGreenError::InvalidPoint)?;
                if prefix_bytes == 0
                    || prefix_bytes >= physical.bytes()
                    || prefix_utf16 == 0
                    || prefix_utf16 >= physical.utf16()
                {
                    return Err(M11RecursiveGreenError::InvalidPoint);
                }
                let physical_cut = physical_start
                    .checked_add(prefix_bytes)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                let lease = self
                    .lease
                    .as_ref()
                    .ok_or(M11RecursiveGreenError::InvalidState)?;
                let physical_cut = usize::try_from(physical_cut)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
                let physical_start_usize = usize::try_from(physical_start)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
                let observed_utf16 = lease
                    .utf16_offset_for_byte(physical_cut)?
                    .checked_sub(lease.utf16_offset_for_byte(physical_start_usize)?)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                if u64::try_from(observed_utf16)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
                    != prefix_utf16
                {
                    return Err(M11RecursiveGreenError::InvalidPoint);
                }
                let prefix = metric(prefix_bytes, prefix_utf16)?;
                let suffix = metric(
                    physical.bytes() - prefix_bytes,
                    physical.utf16() - prefix_utf16,
                )?;
                work.boundary_after_pending_output = Some(0);
                Ok([
                    Some(PackedGreenEvent::Coverage {
                        physical: prefix,
                        owner_depth: 1,
                        part: M11RecursiveGreenCoveragePart::Gap,
                        atom: LogicalAtom::None,
                    }),
                    Some(PackedGreenEvent::Coverage {
                        physical: suffix,
                        owner_depth,
                        part,
                        atom,
                    }),
                ])
            }
            RewriteMode::Retain { .. } => Ok([
                Some(PackedGreenEvent::Coverage {
                    physical,
                    owner_depth,
                    part,
                    atom,
                }),
                None,
            ]),
        }
    }

    fn begin_rewrite_page_push(
        &mut self,
        runtime: &mut DocumentRuntime,
        work: &mut M11RecursiveGreenTerminalFragmentRewriteWork,
    ) -> Result<(), M11RecursiveGreenError> {
        encode_leaf_header(
            &mut work.page,
            work.page_events,
            work.page_len - GREEN_LEAF_HEADER_BYTES,
            work.page_summary,
        )?;
        let build = self
            .build
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let mut session = runtime.producer_arena_mut().resume_build(build)?;
        let result = (|| {
            if work.replacement.is_none() {
                work.replacement = Some(ResumableMeasuredSequenceBuilder::try_new(
                    &mut session,
                    &mut self.mutation,
                )?);
            }
            let leaf = session.allocate(&work.page[..work.page_len], &[])?;
            work.replacement
                .as_mut()
                .ok_or(M11RecursiveGreenError::InvalidState)?
                .begin_push(&session, leaf, &mut self.mutation)
        })();
        self.build = Some(session.suspend()?);
        result?;
        work.page.fill(0);
        work.page_len = GREEN_LEAF_HEADER_BYTES;
        work.page_events = 0;
        work.page_summary = RecursiveGreenSummary::empty();
        Ok(())
    }

    fn with_rewrite_session<T>(
        &mut self,
        runtime: &mut DocumentRuntime,
        work: &mut M11RecursiveGreenTerminalFragmentRewriteWork,
        operation: impl FnOnce(
            &mut M11RecursiveGreenTerminalFragmentRewriteWork,
            &mut crate::storage::ArenaBuildSession<'_>,
            &mut crate::measured_sequence::SequenceMutationReceipt,
        ) -> Result<T, M11RecursiveGreenError>,
    ) -> Result<T, M11RecursiveGreenError> {
        let build = self
            .build
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let mut session = runtime.producer_arena_mut().resume_build(build)?;
        let result = operation(work, &mut session, &mut self.mutation);
        self.build = Some(session.suspend()?);
        result
    }

    fn finish_fragment_splice(
        &mut self,
        runtime: &mut DocumentRuntime,
        work: &mut M11RecursiveGreenTerminalFragmentRewriteWork,
    ) -> Result<(), M11RecursiveGreenError> {
        let expected_cut = match work.mode {
            RewriteMode::Remove {
                cut_bytes,
                cut_utf16,
            }
            | RewriteMode::Retain {
                cut_bytes,
                cut_utf16,
            } => (cut_bytes, cut_utf16),
            RewriteMode::Unchanged => return Err(M11RecursiveGreenError::InvalidState),
        };
        if work.next_original_event != work.stamp.events_end
            || work.physical_position != work.stamp.source_end.bytes()
            || work.logical_bytes < expected_cut.0
            || work.logical_utf16 < expected_cut.1
            || (matches!(work.mode, RewriteMode::Remove { .. })
                && (work.logical_bytes, work.logical_utf16) != expected_cut)
        {
            return Err(M11RecursiveGreenError::InvalidPoint);
        }
        let build = self
            .build
            .take()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let mut session = runtime.producer_arena_mut().resume_build(build)?;
        let result = (|| {
            let root = splice_measured_sequence_build_root_atomic::<RecursiveGreenSpec>(
                &mut session,
                work.base
                    .take()
                    .ok_or(M11RecursiveGreenError::InvalidState)?,
                work.first_leaf..work.end_leaf,
                work.replacement_root.take(),
                &mut self.mutation,
            )?
            .ok_or(M11RecursiveGreenError::InvalidState)?;
            let mut inspection = SequenceInspectionReceipt::default();
            let measure = root
                .as_ref(&session)?
                .summary(session.arena(), &mut inspection)?
                .ok_or(M11RecursiveGreenError::InvalidState)?;
            if measure.summary().physical_bytes != self.expected_summary.physical_bytes
                || measure.summary().physical_utf16 != self.expected_summary.physical_utf16
            {
                return Err(M11RecursiveGreenError::IncompleteCoverage);
            }
            Ok((root, measure.summary()))
        })();
        self.build = Some(session.suspend()?);
        let (root, summary) = result?;
        self.expected_summary = summary;
        self.working_prefix = Some(root);
        if matches!(work.mode, RewriteMode::Remove { .. }) {
            let open = self
                .open
                .pop()
                .ok_or(M11RecursiveGreenError::InvalidState)?;
            if open.frame != work.stamp.frame {
                return Err(M11RecursiveGreenError::InvalidState);
            }
        }
        Ok(())
    }

    fn complete_fragment_rewrite(
        &mut self,
        work: &mut M11RecursiveGreenTerminalFragmentRewriteWork,
        transitions: usize,
    ) -> Result<M11RecursiveGreenTerminalFragmentRewritePoll, M11RecursiveGreenError> {
        let disposition = if matches!(work.mode, RewriteMode::Remove { .. }) {
            M11RecursiveGreenTerminalFragmentDisposition::Removed
        } else {
            M11RecursiveGreenTerminalFragmentDisposition::Surviving
        };
        let visible_remainder_boundary = if matches!(work.mode, RewriteMode::Retain { .. }) {
            let event_cut = work
                .visible_remainder_event_cut
                .ok_or(M11RecursiveGreenError::InvalidState)?;
            let physical = work
                .visible_remainder_physical
                .ok_or(M11RecursiveGreenError::InvalidState)?;
            Some(
                super::adopt::M11RecursiveGreenStructuralBoundary::from_build(
                    self.runtime_identity,
                    self.green_identity,
                    self.source,
                    event_cut,
                    physical,
                    work.stamp.logical_before,
                    self.open.iter().map(|frame| (frame.frame, frame.kind)),
                )?,
            )
        } else {
            None
        };
        self.active_fragment = None;
        self.phase = BuildPhase::Accepting;
        work.phase = RewritePhase::Complete;
        Ok(M11RecursiveGreenTerminalFragmentRewritePoll::Complete {
            transitions,
            authority: M11RecursiveGreenTerminalFragmentRewriteAuthority {
                stamp: work.stamp,
                disposition,
                visible_remainder_boundary,
            },
        })
    }
}

fn validate_rewrite_range(
    stamp: M11RecursiveGreenTerminalFragmentStamp,
    range: &M11RecursiveGreenTerminalFragmentRange,
) -> Result<(), M11RecursiveGreenError> {
    if range.stamp != stamp {
        return Err(M11RecursiveGreenError::InvalidState);
    }
    if !range.replay_validated {
        return Err(M11RecursiveGreenError::InvalidState);
    }
    Ok(())
}

fn queue_rewrite_events(
    work: &mut M11RecursiveGreenTerminalFragmentRewriteWork,
    events: [Option<PackedGreenEvent>; 2],
) {
    work.pending_output = events;
    work.pending_output_next = 0;
    work.pending_output_len = u8::from(events[0].is_some()) + u8::from(events[1].is_some());
}

fn append_rewrite_event(
    work: &mut M11RecursiveGreenTerminalFragmentRewriteWork,
    event: PackedGreenEvent,
) -> Result<(), M11RecursiveGreenError> {
    let summary = packed_event_summary(event)?;
    let next = work.page_summary.checked_followed_by(summary)?;
    encode_packed_event(event, &mut work.page, &mut work.page_len)?;
    work.page_events = work
        .page_events
        .checked_add(1)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    work.page_summary = next;
    Ok(())
}

fn rebase_removed_atom(atom: LogicalAtom) -> Result<LogicalAtom, M11RecursiveGreenError> {
    Ok(match atom {
        LogicalAtom::TabToSpaces {
            target_owner_depth: 0,
            ..
        } => LogicalAtom::None,
        LogicalAtom::TabToSpaces {
            target_owner_depth,
            spaces,
        } => LogicalAtom::TabToSpaces {
            target_owner_depth: target_owner_depth - 1,
            spaces,
        },
        LogicalAtom::Identity
        | LogicalAtom::LfToLf
        | LogicalAtom::CrLfToLf
        | LogicalAtom::LoneCrToLf
        | LogicalAtom::NulToReplacement
        | LogicalAtom::None
        | LogicalAtom::HiddenUpstream => atom,
    })
}

fn metric(bytes: u64, utf16: u64) -> Result<M11RecursiveGreenSourceMetric, M11RecursiveGreenError> {
    M11RecursiveGreenSourceMetric::new(bytes, utf16).ok_or(M11RecursiveGreenError::InvalidPoint)
}

fn projection_atom(
    lease: &crate::SourceSnapshotLease,
    start: u64,
    end: u64,
    start_utf16: u64,
    end_utf16: u64,
    atom: LogicalAtom,
) -> Result<Option<ProjectedAtom>, M11RecursiveGreenError> {
    let source_start =
        usize::try_from(start).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let source_end = usize::try_from(end).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let physical = PhysicalSpan {
        byte_start: start,
        byte_end: end,
        utf16_start: start_utf16,
        utf16_end: end_utf16,
    };
    Ok(match atom {
        LogicalAtom::None | LogicalAtom::HiddenUpstream => None,
        LogicalAtom::Identity => Some(ProjectedAtom::Identity {
            source: lease.duplicate().cursor_in(source_start..source_end)?,
            physical_bytes: start,
            physical_utf16: start_utf16,
            scalar_remaining: 0,
            scalar_utf16: 0,
        }),
        LogicalAtom::TabToSpaces { spaces, .. } => Some(ProjectedAtom::Spaces {
            remaining: spaces,
            first: true,
            physical,
        }),
        LogicalAtom::LfToLf => Some(ProjectedAtom::Static {
            bytes: [b'\n', 0, 0],
            len: 1,
            next: 0,
            scalar_utf16_at_end: 1,
            raw_contribution_at_end: 1,
            physical,
        }),
        LogicalAtom::CrLfToLf => Some(ProjectedAtom::Static {
            bytes: [b'\n', 0, 0],
            len: 1,
            next: 0,
            scalar_utf16_at_end: 1,
            raw_contribution_at_end: 2,
            physical,
        }),
        LogicalAtom::LoneCrToLf => Some(ProjectedAtom::Static {
            bytes: [b'\n', 0, 0],
            len: 1,
            next: 0,
            scalar_utf16_at_end: 1,
            raw_contribution_at_end: 1,
            physical,
        }),
        LogicalAtom::NulToReplacement => Some(ProjectedAtom::Static {
            bytes: REPLACEMENT_BYTES,
            len: 3,
            next: 0,
            scalar_utf16_at_end: 1,
            raw_contribution_at_end: 1,
            physical,
        }),
    })
}

fn utf8_width(first: u8) -> Option<u8> {
    match first {
        0x00..=0x7f => Some(1),
        0xc2..=0xdf => Some(2),
        0xe0..=0xef => Some(3),
        0xf0..=0xf4 => Some(4),
        _ => None,
    }
}

const fn cursor_poll(
    status: M11RecursiveGreenTerminalFragmentCursorStatus,
    transitions: usize,
) -> M11RecursiveGreenTerminalFragmentCursorPoll {
    M11RecursiveGreenTerminalFragmentCursorPoll {
        status,
        transitions,
    }
}

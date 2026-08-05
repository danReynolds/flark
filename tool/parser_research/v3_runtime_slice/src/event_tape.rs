//! Grammar-free packed structural event persistence.
//!
//! Event pages contain only stable scalar identities and coverage-relative
//! coordinates. They are leaves in a persistent prefix-sum sequence and own a
//! real arena edge to a separately persistent leading projection checkpoint.

use std::fmt;
use std::ops::Range;

use crate::arena::ArenaBuildTransaction;
use crate::persistent_sequence::{
    PersistentSequence, SealedSequenceLeaf, SequenceMutationReceipt,
    SequenceNodeKind as CoreSequenceNodeKind, SequenceSpec,
    sequence_node as persistent_sequence_node,
};
use crate::{ARENA_PAGE_BYTES, ArenaError, ArenaId, OwnedArenaRef, OwnerTransferError, PageArena};

const FORMAT_VERSION: u8 = 1;
const EVENT_PAGE_TAG: u8 = 0x51;
const SEQUENCE_BRANCH_TAG: u8 = 0x52;
const PROJECTION_FRAME_TAG: u8 = 0x53;
const OUTPUT_MANIFEST_TAG: u8 = 0x54;
const EVENT_PAGE_HEADER_BYTES: usize = 24;
const COVERAGE_RECORD_BYTES: usize = 24;
const SEQUENCE_BRANCH_BYTES: usize = 52;
const PROJECTION_FRAME_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CoverageId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OccurrenceId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunRangeId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueCursorId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalRecordId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Position {
    pub bytes: u64,
    pub utf16: u64,
}

impl Position {
    fn checked_add(self, other: Self) -> Result<Self, EventTapeError> {
        Ok(Self {
            bytes: self
                .bytes
                .checked_add(other.bytes)
                .ok_or(EventTapeError::Overflow("byte coverage"))?,
            utf16: self
                .utf16
                .checked_add(other.utf16)
                .ok_or(EventTapeError::Overflow("UTF-16 coverage"))?,
        })
    }

    fn coordinate(self, coordinate: Coordinate) -> u64 {
        match coordinate {
            Coordinate::Byte => self.bytes,
            Coordinate::Utf16 => self.utf16,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coordinate {
    Byte,
    Utf16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceAnchor {
    pub coverage: CoverageId,
    pub local_bytes: u32,
    pub local_utf16: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoverageRecord {
    pub id: CoverageId,
    pub length: Position,
}

/// A physical event-page identity. Slot reuse changes its generation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutputPageId(pub ArenaId);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectionRootId(pub ArenaId);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutputRootId(pub ArenaId);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventStamp {
    pub page: OutputPageId,
    pub local_event: u16,
}

/// Scalar-only structural emission. Growing source or transformed payloads are
/// represented by external cursor IDs, never copied into an event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuralEvent {
    Open {
        block: BlockId,
        parent: Option<BlockId>,
        kind_tag: u16,
        start: SourceAnchor,
    },
    Promote {
        block: BlockId,
        kind_tag: u16,
        context: u64,
    },
    AppendRuns {
        block: BlockId,
        runs: RunRangeId,
    },
    DrainRunPrefix {
        block: BlockId,
        logical_bytes: u64,
    },
    WriteEnd {
        block: BlockId,
        end: SourceAnchor,
    },
    RepairListEnds {
        list: BlockId,
        first: BlockId,
        last: BlockId,
    },
    Definition {
        occurrence: OccurrenceId,
        symbol: SymbolId,
        value: ValueCursorId,
        origin: SourceAnchor,
    },
    Finalize {
        block: BlockId,
        terminal: TerminalRecordId,
    },
    Close {
        block: BlockId,
    },
}

/// Every event has a stable self-relative stamp. `WriteEnd` and list repair use
/// this containing stamp rather than embedding an absolute event ordinal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StampedEvent {
    pub stamp: EventStamp,
    pub event: StructuralEvent,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PushResult<T> {
    Accepted,
    PageFull {
        item: T,
        continuation: Option<CoverageContinuation>,
    },
}

/// Non-copyable permission to continue emitting the physical line that filled
/// the previous page.
#[derive(Debug, PartialEq, Eq)]
pub struct CoverageContinuation {
    coverage: CoverageId,
}

impl CoverageContinuation {
    #[must_use]
    pub const fn coverage(&self) -> CoverageId {
        self.coverage
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventTapeReceipt {
    pub event_pages_allocated: usize,
    pub event_payload_bytes_copied: usize,
    pub projection_nodes_allocated: usize,
    pub projection_path_nodes_visited: usize,
    pub sequence_leaf_nodes_adopted: usize,
    pub sequence_branch_nodes_allocated: usize,
    pub sequence_nodes_visited: usize,
    pub child_references_added: usize,
    pub pages_reused: usize,
    pub maximum_temporary_event_bytes: usize,
    pub maximum_streaming_sequence_roots: usize,
    pub maximum_streaming_sequence_bin_slots: usize,
    pub maximum_streaming_sequence_bin_bytes: usize,
}

impl EventTapeReceipt {
    #[must_use]
    pub const fn sequence_nodes_allocated(self) -> usize {
        self.sequence_branch_nodes_allocated
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventTapeError {
    Arena(ArenaError),
    Corrupt(&'static str),
    EmptyPage,
    EventTooLarge,
    CoverageTooLarge,
    Overflow(&'static str),
    InvalidPageRange,
    ProjectionMismatch(&'static str),
    ProjectionFuelExhausted,
}

impl From<ArenaError> for EventTapeError {
    fn from(error: ArenaError) -> Self {
        Self::Arena(error)
    }
}

fn legacy_owner_transfer_error(failure: OwnerTransferError) -> EventTapeError {
    // This retained event-tape bakeoff predates recoverable owner-transfer
    // errors and is not the selected storage path. Keep the lossy conversion
    // local and explicit; production authority code must return `failure.owner`.
    let OwnerTransferError { error, owner } = failure;
    drop(owner);
    EventTapeError::Arena(error)
}

impl fmt::Display for EventTapeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arena(error) => error.fmt(formatter),
            Self::Corrupt(message) => write!(formatter, "corrupt event tape: {message}"),
            Self::EmptyPage => formatter.write_str("an event page cannot be empty"),
            Self::EventTooLarge => formatter.write_str("one structural event exceeds a page"),
            Self::CoverageTooLarge => {
                formatter.write_str("one coverage record exceeds the remaining page")
            }
            Self::Overflow(field) => write!(formatter, "event-tape {field} overflow"),
            Self::InvalidPageRange => formatter.write_str("invalid event-page splice range"),
            Self::ProjectionMismatch(message) => {
                write!(formatter, "projection invariant failed: {message}")
            }
            Self::ProjectionFuelExhausted => {
                formatter.write_str("projection traversal exhausted its fuel")
            }
        }
    }
}

impl std::error::Error for EventTapeError {}

#[derive(Debug)]
pub struct EventPageBuilder {
    leading_projection: ProjectionCheckpoint,
    events: Vec<StructuralEvent>,
    encoded_event_bytes: usize,
    coverage: Vec<CoverageRecord>,
    active_coverage: Option<CoverageId>,
    dedicated_continuation: bool,
}

impl EventPageBuilder {
    #[must_use]
    pub fn new(leading_projection: ProjectionCheckpoint) -> Self {
        Self {
            leading_projection,
            events: Vec::new(),
            encoded_event_bytes: 0,
            coverage: Vec::new(),
            active_coverage: None,
            dedicated_continuation: false,
        }
    }

    /// Starts a page dedicated to the overflowing line authorized by `token`.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)] // The token cannot authorize two continuations.
    pub fn continuing(
        leading_projection: ProjectionCheckpoint,
        token: CoverageContinuation,
    ) -> Self {
        let CoverageContinuation { coverage } = token;
        Self {
            leading_projection,
            events: Vec::new(),
            encoded_event_bytes: 0,
            coverage: Vec::new(),
            active_coverage: Some(coverage),
            dedicated_continuation: true,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.coverage.is_empty()
    }

    #[must_use]
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub const fn temporary_event_bytes(&self) -> usize {
        self.encoded_event_bytes
    }

    pub fn push_event(
        &mut self,
        event: StructuralEvent,
    ) -> Result<PushResult<StructuralEvent>, EventTapeError> {
        let event_bytes = encoded_event_len(&event);
        let next_count = self
            .events
            .len()
            .checked_add(1)
            .ok_or(EventTapeError::Overflow("event count"))?;
        if packed_page_len(
            next_count,
            self.coverage.len(),
            self.encoded_event_bytes + event_bytes,
        ) > ARENA_PAGE_BYTES
        {
            if self.is_empty() {
                return Err(EventTapeError::EventTooLarge);
            }
            return Ok(PushResult::PageFull {
                item: event,
                continuation: self
                    .active_coverage
                    .map(|coverage| CoverageContinuation { coverage }),
            });
        }
        self.encoded_event_bytes += event_bytes;
        self.events.push(event);
        Ok(PushResult::Accepted)
    }

    pub fn push_coverage(
        &mut self,
        coverage: CoverageRecord,
    ) -> Result<PushResult<CoverageRecord>, EventTapeError> {
        if self.dedicated_continuation {
            return Ok(PushResult::PageFull {
                item: coverage,
                continuation: None,
            });
        }
        let next_count = self
            .coverage
            .len()
            .checked_add(1)
            .ok_or(EventTapeError::Overflow("coverage count"))?;
        if packed_page_len(self.events.len(), next_count, self.encoded_event_bytes)
            > ARENA_PAGE_BYTES
        {
            if self.is_empty() {
                return Err(EventTapeError::CoverageTooLarge);
            }
            return Ok(PushResult::PageFull {
                item: coverage,
                continuation: None,
            });
        }
        self.active_coverage = Some(coverage.id);
        self.coverage.push(coverage);
        Ok(PushResult::Accepted)
    }

    pub fn seal_page(
        mut self,
        arena: &mut PageArena,
        receipt: &mut EventTapeReceipt,
    ) -> Result<SealedEventPage, EventTapeError> {
        if self.is_empty() {
            return Err(EventTapeError::EmptyPage);
        }
        let event_count = u16::try_from(self.events.len())
            .map_err(|_| EventTapeError::Overflow("page-local event count"))?;
        let coverage_count = u16::try_from(self.coverage.len())
            .map_err(|_| EventTapeError::Overflow("page-local coverage count"))?;
        let mut total = Position::default();
        for coverage in &self.coverage {
            total = total.checked_add(coverage.length)?;
        }

        let mut payload = Vec::with_capacity(packed_page_len(
            self.events.len(),
            self.coverage.len(),
            self.encoded_event_bytes,
        ));
        payload.push(EVENT_PAGE_TAG);
        payload.push(FORMAT_VERSION);
        push_u16(&mut payload, event_count);
        push_u16(&mut payload, coverage_count);
        push_u16(
            &mut payload,
            event_count
                .checked_add(1)
                .ok_or(EventTapeError::Overflow("event offsets"))?,
        );
        push_u64(&mut payload, total.bytes);
        push_u64(&mut payload, total.utf16);
        debug_assert_eq!(payload.len(), EVENT_PAGE_HEADER_BYTES);
        for coverage in &self.coverage {
            push_u64(&mut payload, coverage.id.0);
            push_u64(&mut payload, coverage.length.bytes);
            push_u64(&mut payload, coverage.length.utf16);
        }
        let mut offset = 0_usize;
        for event in &self.events {
            push_u16(
                &mut payload,
                u16::try_from(offset).map_err(|_| EventTapeError::Overflow("event offset"))?,
            );
            offset += encoded_event_len(event);
        }
        push_u16(
            &mut payload,
            u16::try_from(offset).map_err(|_| EventTapeError::Overflow("event payload"))?,
        );
        for event in &self.events {
            encode_event(event, &mut payload);
        }
        if payload.len() > ARENA_PAGE_BYTES {
            return Err(EventTapeError::Corrupt("builder exceeded page cap"));
        }
        let children = self
            .leading_projection
            .owner
            .as_ref()
            .map_or_else(Vec::new, |owner| vec![owner.id()]);
        let allocation = match arena.allocate(&payload, &children) {
            Ok(allocation) => allocation,
            Err(error) => {
                if let Some(owner) = self.leading_projection.owner.take() {
                    arena
                        .release_later(owner)
                        .map_err(legacy_owner_transfer_error)?;
                }
                return Err(error.into());
            }
        };
        if let Some(owner) = self.leading_projection.owner.take() {
            arena
                .release_later(owner)
                .map_err(legacy_owner_transfer_error)?;
        }
        receipt.event_pages_allocated += 1;
        receipt.event_payload_bytes_copied += allocation.receipt.payload_bytes_copied;
        receipt.child_references_added += allocation.receipt.child_references_added;
        receipt.maximum_temporary_event_bytes = receipt
            .maximum_temporary_event_bytes
            .max(self.encoded_event_bytes);
        let id = OutputPageId(allocation.owner.id());
        Ok(SealedEventPage {
            id,
            owner: allocation.owner,
        })
    }

    pub fn cancel(mut self, arena: &mut PageArena) -> Result<(), EventTapeError> {
        if let Some(owner) = self.leading_projection.owner.take() {
            arena
                .release_later(owner)
                .map_err(legacy_owner_transfer_error)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct SealedEventPage {
    id: OutputPageId,
    owner: OwnedArenaRef,
}

impl SealedEventPage {
    #[must_use]
    pub const fn id(&self) -> OutputPageId {
        self.id
    }

    #[must_use]
    pub const fn arena_id(&self) -> ArenaId {
        self.id.0
    }
}

fn packed_page_len(events: usize, coverage: usize, event_bytes: usize) -> usize {
    EVENT_PAGE_HEADER_BYTES
        .saturating_add(coverage.saturating_mul(COVERAGE_RECORD_BYTES))
        .saturating_add(events.saturating_add(1).saturating_mul(2))
        .saturating_add(event_bytes)
}

fn encoded_event_len(event: &StructuralEvent) -> usize {
    match event {
        StructuralEvent::Open { parent, .. } => {
            1 + 8 + 1 + usize::from(parent.is_some()) * 8 + 2 + 16
        }
        StructuralEvent::Promote { .. } => 1 + 8 + 2 + 8,
        StructuralEvent::AppendRuns { .. }
        | StructuralEvent::DrainRunPrefix { .. }
        | StructuralEvent::Finalize { .. } => 1 + 8 + 8,
        StructuralEvent::WriteEnd { .. } | StructuralEvent::RepairListEnds { .. } => 1 + 8 + 16,
        StructuralEvent::Definition { .. } => 1 + 8 + 8 + 8 + 16,
        StructuralEvent::Close { .. } => 1 + 8,
    }
}

fn encode_event(event: &StructuralEvent, output: &mut Vec<u8>) {
    match event {
        StructuralEvent::Open {
            block,
            parent,
            kind_tag,
            start,
        } => {
            output.push(1);
            push_u64(output, block.0);
            output.push(u8::from(parent.is_some()));
            if let Some(parent) = parent {
                push_u64(output, parent.0);
            }
            push_u16(output, *kind_tag);
            encode_anchor(*start, output);
        }
        StructuralEvent::Promote {
            block,
            kind_tag,
            context,
        } => {
            output.push(2);
            push_u64(output, block.0);
            push_u16(output, *kind_tag);
            push_u64(output, *context);
        }
        StructuralEvent::AppendRuns { block, runs } => {
            output.push(3);
            push_u64(output, block.0);
            push_u64(output, runs.0);
        }
        StructuralEvent::DrainRunPrefix {
            block,
            logical_bytes,
        } => {
            output.push(4);
            push_u64(output, block.0);
            push_u64(output, *logical_bytes);
        }
        StructuralEvent::WriteEnd { block, end } => {
            output.push(5);
            push_u64(output, block.0);
            encode_anchor(*end, output);
        }
        StructuralEvent::RepairListEnds { list, first, last } => {
            output.push(6);
            push_u64(output, list.0);
            push_u64(output, first.0);
            push_u64(output, last.0);
        }
        StructuralEvent::Definition {
            occurrence,
            symbol,
            value,
            origin,
        } => {
            output.push(7);
            push_u64(output, occurrence.0);
            push_u64(output, symbol.0);
            push_u64(output, value.0);
            encode_anchor(*origin, output);
        }
        StructuralEvent::Finalize { block, terminal } => {
            output.push(8);
            push_u64(output, block.0);
            push_u64(output, terminal.0);
        }
        StructuralEvent::Close { block } => {
            output.push(9);
            push_u64(output, block.0);
        }
    }
}

fn decode_event(bytes: &[u8]) -> Result<StructuralEvent, EventTapeError> {
    let mut decoder = Decoder::new(bytes);
    let tag = decoder.u8()?;
    let event = match tag {
        1 => {
            let block = BlockId(decoder.u64()?);
            let parent = match decoder.u8()? {
                0 => None,
                1 => Some(BlockId(decoder.u64()?)),
                _ => return Err(EventTapeError::Corrupt("invalid parent flag")),
            };
            StructuralEvent::Open {
                block,
                parent,
                kind_tag: decoder.u16()?,
                start: decoder.anchor()?,
            }
        }
        2 => StructuralEvent::Promote {
            block: BlockId(decoder.u64()?),
            kind_tag: decoder.u16()?,
            context: decoder.u64()?,
        },
        3 => StructuralEvent::AppendRuns {
            block: BlockId(decoder.u64()?),
            runs: RunRangeId(decoder.u64()?),
        },
        4 => StructuralEvent::DrainRunPrefix {
            block: BlockId(decoder.u64()?),
            logical_bytes: decoder.u64()?,
        },
        5 => StructuralEvent::WriteEnd {
            block: BlockId(decoder.u64()?),
            end: decoder.anchor()?,
        },
        6 => StructuralEvent::RepairListEnds {
            list: BlockId(decoder.u64()?),
            first: BlockId(decoder.u64()?),
            last: BlockId(decoder.u64()?),
        },
        7 => StructuralEvent::Definition {
            occurrence: OccurrenceId(decoder.u64()?),
            symbol: SymbolId(decoder.u64()?),
            value: ValueCursorId(decoder.u64()?),
            origin: decoder.anchor()?,
        },
        8 => StructuralEvent::Finalize {
            block: BlockId(decoder.u64()?),
            terminal: TerminalRecordId(decoder.u64()?),
        },
        9 => StructuralEvent::Close {
            block: BlockId(decoder.u64()?),
        },
        _ => return Err(EventTapeError::Corrupt("unknown structural event")),
    };
    if !decoder.is_empty() {
        return Err(EventTapeError::Corrupt("trailing structural event bytes"));
    }
    Ok(event)
}

fn encode_anchor(anchor: SourceAnchor, output: &mut Vec<u8>) {
    push_u64(output, anchor.coverage.0);
    push_u32(output, anchor.local_bytes);
    push_u32(output, anchor.local_utf16);
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

struct Decoder<'a> {
    remaining: &'a [u8],
}

impl<'a> Decoder<'a> {
    const fn new(remaining: &'a [u8]) -> Self {
        Self { remaining }
    }

    const fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], EventTapeError> {
        let (value, remaining) = self
            .remaining
            .split_at_checked(count)
            .ok_or(EventTapeError::Corrupt("truncated scalar"))?;
        self.remaining = remaining;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, EventTapeError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, EventTapeError> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .expect("decoder requested exactly two bytes"),
        ))
    }

    fn u32(&mut self) -> Result<u32, EventTapeError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .expect("decoder requested exactly four bytes"),
        ))
    }

    fn u64(&mut self) -> Result<u64, EventTapeError> {
        Ok(u64::from_le_bytes(
            self.take(8)?
                .try_into()
                .expect("decoder requested exactly eight bytes"),
        ))
    }

    fn anchor(&mut self) -> Result<SourceAnchor, EventTapeError> {
        Ok(SourceAnchor {
            coverage: CoverageId(self.u64()?),
            local_bytes: self.u32()?,
            local_utf16: self.u32()?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SequenceSummary {
    pub pages: u64,
    pub coverage: Position,
    pub events: u64,
    pub height: u16,
    pub leading_zero_coverage_pages: u64,
    pub trailing_zero_coverage_pages: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventPageView {
    pub id: OutputPageId,
    pub leading_projection: Option<ProjectionRootId>,
    pub coverage: Vec<CoverageRecord>,
    pub events: Vec<StampedEvent>,
    pub packed_bytes: usize,
}

pub fn read_event_page(
    arena: &PageArena,
    page: OutputPageId,
) -> Result<EventPageView, EventTapeError> {
    let payload = arena.payload(page.0)?;
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != EVENT_PAGE_TAG || decoder.u8()? != FORMAT_VERSION {
        return Err(EventTapeError::Corrupt("wrong event-page header"));
    }
    let event_count = usize::from(decoder.u16()?);
    let coverage_count = usize::from(decoder.u16()?);
    let offset_count = usize::from(decoder.u16()?);
    if offset_count != event_count + 1 {
        return Err(EventTapeError::Corrupt("wrong event-offset count"));
    }
    let declared_coverage = Position {
        bytes: decoder.u64()?,
        utf16: decoder.u64()?,
    };
    let mut coverage = Vec::with_capacity(coverage_count);
    let mut actual_coverage = Position::default();
    for _ in 0..coverage_count {
        let record = CoverageRecord {
            id: CoverageId(decoder.u64()?),
            length: Position {
                bytes: decoder.u64()?,
                utf16: decoder.u64()?,
            },
        };
        actual_coverage = actual_coverage.checked_add(record.length)?;
        coverage.push(record);
    }
    if actual_coverage != declared_coverage {
        return Err(EventTapeError::Corrupt("coverage summary mismatch"));
    }
    let mut offsets = Vec::with_capacity(offset_count);
    for _ in 0..offset_count {
        offsets.push(usize::from(decoder.u16()?));
    }
    let event_bytes = decoder.remaining;
    if offsets.first() != Some(&0) || offsets.last() != Some(&event_bytes.len()) {
        return Err(EventTapeError::Corrupt("event offsets do not span payload"));
    }
    if offsets.windows(2).any(|pair| pair[0] > pair[1]) {
        return Err(EventTapeError::Corrupt("event offsets are not ordered"));
    }
    let mut events = Vec::with_capacity(event_count);
    for (index, pair) in offsets.windows(2).enumerate() {
        let local_event =
            u16::try_from(index).map_err(|_| EventTapeError::Overflow("event stamp"))?;
        events.push(StampedEvent {
            stamp: EventStamp { page, local_event },
            event: decode_event(&event_bytes[pair[0]..pair[1]])?,
        });
    }
    let children = arena.children(page.0)?;
    if children[1].is_some() {
        return Err(EventTapeError::Corrupt("event page has extra child"));
    }
    let leading_projection = children[0].map(ProjectionRootId);
    Ok(EventPageView {
        id: page,
        leading_projection,
        coverage,
        events,
        packed_bytes: payload.len(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectionFrame {
    pub block: BlockId,
    pub kind_tag: u16,
    pub context: u64,
    pub start: SourceAnchor,
    pub current: SourceAnchor,
}

/// Mutable owner of one persistent projection-stack root. Each page retains a
/// separate arena edge to the checkpoint that led it.
#[derive(Debug, Default)]
pub struct ProjectionState {
    owner: Option<OwnedArenaRef>,
    depth: u32,
}

impl ProjectionState {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            owner: None,
            depth: 0,
        }
    }

    #[must_use]
    pub fn root(&self) -> Option<ProjectionRootId> {
        self.owner
            .as_ref()
            .map(|owner| ProjectionRootId(owner.id()))
    }

    #[must_use]
    pub const fn depth(&self) -> u32 {
        self.depth
    }

    pub fn checkpoint(
        &self,
        arena: &mut PageArena,
    ) -> Result<ProjectionCheckpoint, EventTapeError> {
        Ok(ProjectionCheckpoint {
            owner: self
                .owner
                .as_ref()
                .map(|owner| arena.retain(owner.id()))
                .transpose()?,
        })
    }

    pub fn apply(
        &mut self,
        arena: &mut PageArena,
        event: &StructuralEvent,
        receipt: &mut EventTapeReceipt,
    ) -> Result<(), EventTapeError> {
        match event {
            StructuralEvent::Open {
                block,
                parent,
                kind_tag,
                start,
            } => self.open(arena, *block, *parent, *kind_tag, *start, receipt),
            StructuralEvent::Promote {
                block,
                kind_tag,
                context,
            } => self.rewrite(
                arena,
                *block,
                |frame| {
                    frame.kind_tag = *kind_tag;
                    frame.context = *context;
                },
                receipt,
            ),
            StructuralEvent::WriteEnd { block, end } => {
                self.rewrite(arena, *block, |frame| frame.current = *end, receipt)
            }
            StructuralEvent::Close { block } => self.close(arena, *block),
            StructuralEvent::AppendRuns { .. }
            | StructuralEvent::DrainRunPrefix { .. }
            | StructuralEvent::RepairListEnds { .. }
            | StructuralEvent::Definition { .. }
            | StructuralEvent::Finalize { .. } => Ok(()),
        }
    }

    pub fn release_later(mut self, arena: &mut PageArena) -> Result<(), EventTapeError> {
        if let Some(owner) = self.owner.take() {
            arena
                .release_later(owner)
                .map_err(legacy_owner_transfer_error)?;
        }
        Ok(())
    }

    fn open(
        &mut self,
        arena: &mut PageArena,
        block: BlockId,
        parent: Option<BlockId>,
        kind_tag: u16,
        start: SourceAnchor,
        receipt: &mut EventTapeReceipt,
    ) -> Result<(), EventTapeError> {
        let actual_parent = self
            .owner
            .as_ref()
            .map(|owner| decode_projection_frame(arena, owner.id()).map(|value| value.0.block))
            .transpose()?;
        if parent != actual_parent {
            return Err(EventTapeError::ProjectionMismatch(
                "open parent is not the current projection top",
            ));
        }
        let depth = self
            .depth
            .checked_add(1)
            .ok_or(EventTapeError::Overflow("projection depth"))?;
        let frame = ProjectionFrame {
            block,
            kind_tag,
            context: 0,
            start,
            current: start,
        };
        let payload = encode_projection_frame(frame, depth);
        let children = self
            .owner
            .as_ref()
            .map_or_else(Vec::new, |owner| vec![owner.id()]);
        let allocation = arena.allocate(&payload, &children)?;
        if let Some(old) = self.owner.replace(allocation.owner) {
            arena
                .release_later(old)
                .map_err(legacy_owner_transfer_error)?;
        }
        self.depth = depth;
        receipt.projection_nodes_allocated += 1;
        receipt.child_references_added += allocation.receipt.child_references_added;
        Ok(())
    }

    fn close(&mut self, arena: &mut PageArena, block: BlockId) -> Result<(), EventTapeError> {
        let current = self
            .owner
            .take()
            .ok_or(EventTapeError::ProjectionMismatch("close on empty stack"))?;
        let (frame, declared_depth) = decode_projection_frame(arena, current.id())?;
        if frame.block != block || declared_depth != self.depth {
            self.owner = Some(current);
            return Err(EventTapeError::ProjectionMismatch(
                "close does not match projection top",
            ));
        }
        let parent = arena.children(current.id())?[0];
        let parent_owner = parent.map(|id| arena.retain(id)).transpose()?;
        arena
            .release_later(current)
            .map_err(legacy_owner_transfer_error)?;
        self.owner = parent_owner;
        self.depth -= 1;
        Ok(())
    }

    fn rewrite(
        &mut self,
        arena: &mut PageArena,
        block: BlockId,
        update: impl FnOnce(&mut ProjectionFrame),
        receipt: &mut EventTapeReceipt,
    ) -> Result<(), EventTapeError> {
        let root = self
            .owner
            .as_ref()
            .ok_or(EventTapeError::ProjectionMismatch("update on empty stack"))?
            .id();
        let mut chain = Vec::new();
        let mut cursor = Some(root);
        let target_index = loop {
            let id = cursor.ok_or(EventTapeError::ProjectionMismatch(
                "projection update target is not open",
            ))?;
            let (frame, depth) = decode_projection_frame(arena, id)?;
            receipt.projection_path_nodes_visited += 1;
            chain.push((id, frame, depth));
            if frame.block == block {
                break chain.len() - 1;
            }
            cursor = arena.children(id)?[0];
        };
        let (_, mut target, target_depth) = chain[target_index];
        update(&mut target);
        let target_parent = arena.children(chain[target_index].0)?[0];
        let target_children = target_parent.map_or_else(Vec::new, |id| vec![id]);
        let allocation = arena.allocate(
            &encode_projection_frame(target, target_depth),
            &target_children,
        )?;
        receipt.projection_nodes_allocated += 1;
        receipt.child_references_added += allocation.receipt.child_references_added;
        let mut replacement = allocation.owner;
        for (_, frame, depth) in chain[..target_index].iter().rev() {
            let allocation = arena.allocate(
                &encode_projection_frame(*frame, *depth),
                &[replacement.id()],
            )?;
            arena
                .release_later(replacement)
                .map_err(legacy_owner_transfer_error)?;
            replacement = allocation.owner;
            receipt.projection_nodes_allocated += 1;
            receipt.child_references_added += allocation.receipt.child_references_added;
        }
        let old = self
            .owner
            .replace(replacement)
            .expect("rewrite started with a root");
        arena
            .release_later(old)
            .map_err(legacy_owner_transfer_error)?;
        Ok(())
    }
}

/// One builder-owned lease for a leading projection root. Sealing transfers
/// reachability into the event page; cancellation queues this owner directly.
#[derive(Debug, Default)]
pub struct ProjectionCheckpoint {
    owner: Option<OwnedArenaRef>,
}

impl ProjectionCheckpoint {
    #[must_use]
    pub const fn empty() -> Self {
        Self { owner: None }
    }

    #[must_use]
    pub fn root(&self) -> Option<ProjectionRootId> {
        self.owner
            .as_ref()
            .map(|owner| ProjectionRootId(owner.id()))
    }
}

fn encode_projection_frame(frame: ProjectionFrame, depth: u32) -> [u8; PROJECTION_FRAME_BYTES] {
    let mut payload = Vec::with_capacity(PROJECTION_FRAME_BYTES);
    payload.push(PROJECTION_FRAME_TAG);
    payload.push(FORMAT_VERSION);
    push_u32(&mut payload, depth);
    push_u64(&mut payload, frame.block.0);
    push_u16(&mut payload, frame.kind_tag);
    push_u64(&mut payload, frame.context);
    encode_anchor(frame.start, &mut payload);
    encode_anchor(frame.current, &mut payload);
    push_u64(&mut payload, 0);
    payload
        .try_into()
        .expect("projection frame has one fixed encoding")
}

fn decode_projection_frame(
    arena: &PageArena,
    id: ArenaId,
) -> Result<(ProjectionFrame, u32), EventTapeError> {
    let payload = arena.payload(id)?;
    if payload.len() != PROJECTION_FRAME_BYTES {
        return Err(EventTapeError::Corrupt("wrong projection-frame length"));
    }
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != PROJECTION_FRAME_TAG || decoder.u8()? != FORMAT_VERSION {
        return Err(EventTapeError::Corrupt("wrong projection-frame header"));
    }
    let depth = decoder.u32()?;
    let frame = ProjectionFrame {
        block: BlockId(decoder.u64()?),
        kind_tag: decoder.u16()?,
        context: decoder.u64()?,
        start: decoder.anchor()?,
        current: decoder.anchor()?,
    };
    if decoder.u64()? != 0 || !decoder.is_empty() {
        return Err(EventTapeError::Corrupt("projection-frame padding"));
    }
    Ok((frame, depth))
}

fn load_projection(
    arena: &PageArena,
    root: Option<ProjectionRootId>,
    fuel: usize,
) -> Result<(Vec<ProjectionFrame>, usize), EventTapeError> {
    let mut frames = Vec::new();
    let mut cursor = root.map(|root| root.0);
    let mut visited = 0_usize;
    let mut expected_depth = None;
    while let Some(id) = cursor {
        if visited == fuel {
            return Err(EventTapeError::ProjectionFuelExhausted);
        }
        let (frame, depth) = decode_projection_frame(arena, id)?;
        if let Some(expected) = expected_depth
            && depth != expected
        {
            return Err(EventTapeError::Corrupt("projection depth discontinuity"));
        }
        expected_depth = Some(
            depth
                .checked_sub(1)
                .ok_or(EventTapeError::Corrupt("zero-depth projection frame"))?,
        );
        frames.push(frame);
        visited += 1;
        cursor = arena.children(id)?[0];
    }
    if expected_depth.is_some_and(|depth| depth != 0) {
        return Err(EventTapeError::Corrupt("truncated projection stack"));
    }
    frames.reverse();
    Ok((frames, visited))
}

#[derive(Clone, Copy, Debug)]
enum SequenceNodeKind {
    Page(OutputPageId),
    Branch { left: ArenaId, right: ArenaId },
}

fn event_page_summary(payload: &[u8]) -> Result<SequenceSummary, EventTapeError> {
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != EVENT_PAGE_TAG || decoder.u8()? != FORMAT_VERSION {
        return Err(EventTapeError::Corrupt("wrong event-page summary header"));
    }
    let events = u64::from(decoder.u16()?);
    let _coverage_records = decoder.u16()?;
    let _offsets = decoder.u16()?;
    let coverage = Position {
        bytes: decoder.u64()?,
        utf16: decoder.u64()?,
    };
    let zero = u64::from(coverage == Position::default());
    Ok(SequenceSummary {
        pages: 1,
        coverage,
        events,
        height: 1,
        leading_zero_coverage_pages: zero,
        trailing_zero_coverage_pages: zero,
    })
}

fn combined_summary(
    left: SequenceSummary,
    right: SequenceSummary,
) -> Result<SequenceSummary, EventTapeError> {
    let pages = left
        .pages
        .checked_add(right.pages)
        .ok_or(EventTapeError::Overflow("sequence page count"))?;
    let leading_zero_coverage_pages = if left.leading_zero_coverage_pages == left.pages {
        left.pages
            .checked_add(right.leading_zero_coverage_pages)
            .ok_or(EventTapeError::Overflow("leading zero pages"))?
    } else {
        left.leading_zero_coverage_pages
    };
    let trailing_zero_coverage_pages = if right.trailing_zero_coverage_pages == right.pages {
        right
            .pages
            .checked_add(left.trailing_zero_coverage_pages)
            .ok_or(EventTapeError::Overflow("trailing zero pages"))?
    } else {
        right.trailing_zero_coverage_pages
    };
    Ok(SequenceSummary {
        pages,
        coverage: left.coverage.checked_add(right.coverage)?,
        events: left
            .events
            .checked_add(right.events)
            .ok_or(EventTapeError::Overflow("sequence event count"))?,
        height: left
            .height
            .max(right.height)
            .checked_add(1)
            .ok_or(EventTapeError::Overflow("sequence height"))?,
        leading_zero_coverage_pages,
        trailing_zero_coverage_pages,
    })
}

fn encode_branch_summary(summary: SequenceSummary) -> [u8; SEQUENCE_BRANCH_BYTES] {
    let mut payload = Vec::with_capacity(SEQUENCE_BRANCH_BYTES);
    payload.push(SEQUENCE_BRANCH_TAG);
    payload.push(FORMAT_VERSION);
    push_u16(&mut payload, summary.height);
    push_u64(&mut payload, summary.pages);
    push_u64(&mut payload, summary.coverage.bytes);
    push_u64(&mut payload, summary.coverage.utf16);
    push_u64(&mut payload, summary.events);
    push_u64(&mut payload, summary.leading_zero_coverage_pages);
    push_u64(&mut payload, summary.trailing_zero_coverage_pages);
    payload
        .try_into()
        .expect("sequence branch has one fixed encoding")
}

fn decode_branch_summary(payload: &[u8]) -> Result<SequenceSummary, EventTapeError> {
    if payload.len() != SEQUENCE_BRANCH_BYTES {
        return Err(EventTapeError::Corrupt("wrong sequence-branch length"));
    }
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != SEQUENCE_BRANCH_TAG || decoder.u8()? != FORMAT_VERSION {
        return Err(EventTapeError::Corrupt("wrong sequence-branch header"));
    }
    let summary = SequenceSummary {
        height: decoder.u16()?,
        pages: decoder.u64()?,
        coverage: Position {
            bytes: decoder.u64()?,
            utf16: decoder.u64()?,
        },
        events: decoder.u64()?,
        leading_zero_coverage_pages: decoder.u64()?,
        trailing_zero_coverage_pages: decoder.u64()?,
    };
    if !decoder.is_empty()
        || summary.pages < 2
        || summary.height < 2
        || summary.leading_zero_coverage_pages > summary.pages
        || summary.trailing_zero_coverage_pages > summary.pages
    {
        return Err(EventTapeError::Corrupt("invalid sequence-branch summary"));
    }
    Ok(summary)
}

#[derive(Debug)]
struct EventSequenceSpec;

impl SequenceSpec for EventSequenceSpec {
    type Summary = SequenceSummary;
    type Error = EventTapeError;
    type BranchPayload = [u8; SEQUENCE_BRANCH_BYTES];

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() == Some(EVENT_PAGE_TAG) {
            event_page_summary(payload).map(Some)
        } else {
            Ok(None)
        }
    }

    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() == Some(SEQUENCE_BRANCH_TAG) {
            decode_branch_summary(payload).map(Some)
        } else {
            Ok(None)
        }
    }

    fn encode_branch(summary: Self::Summary) -> Self::BranchPayload {
        encode_branch_summary(summary)
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        combined_summary(left, right)
    }

    fn leaves(summary: Self::Summary) -> u64 {
        summary.pages
    }

    fn height(summary: Self::Summary) -> u16 {
        summary.height
    }

    fn invalid(message: &'static str) -> Self::Error {
        if message == "sequence splice is out of range" {
            EventTapeError::InvalidPageRange
        } else {
            EventTapeError::Corrupt(message)
        }
    }
}

fn sequence_node(
    arena: &PageArena,
    id: ArenaId,
) -> Result<(SequenceSummary, SequenceNodeKind), EventTapeError> {
    let (summary, kind) = persistent_sequence_node::<EventSequenceSpec>(arena, id)?;
    Ok((
        summary,
        match kind {
            CoreSequenceNodeKind::Leaf => SequenceNodeKind::Page(OutputPageId(id)),
            CoreSequenceNodeKind::Branch { left, right } => {
                SequenceNodeKind::Branch { left, right }
            }
        },
    ))
}

#[derive(Debug, Default)]
pub struct OutputSequence {
    inner: PersistentSequence<EventSequenceSpec>,
}

impl OutputSequence {
    pub fn from_pages(
        arena: &mut PageArena,
        pages: Vec<SealedEventPage>,
        receipt: &mut EventTapeReceipt,
    ) -> Result<Self, EventTapeError> {
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let inner = PersistentSequence::from_leaves(
            arena,
            pages
                .into_iter()
                .map(|page| SealedSequenceLeaf::new(page.owner))
                .collect(),
            &mut sequence_receipt,
        )?;
        merge_sequence_receipt(receipt, sequence_receipt);
        Ok(Self { inner })
    }

    #[must_use]
    pub fn as_ref(&self) -> OutputSequenceRef {
        OutputSequenceRef {
            root: self.inner.root_id(),
        }
    }

    #[must_use]
    pub fn root_id(&self) -> Option<ArenaId> {
        self.inner.root_id()
    }

    pub fn splice_pages(
        &self,
        arena: &mut PageArena,
        range: Range<usize>,
        replacements: Vec<SealedEventPage>,
        receipt: &mut EventTapeReceipt,
    ) -> Result<Self, EventTapeError> {
        let start = u64::try_from(range.start).expect("usize fits u64 on supported targets");
        let end = u64::try_from(range.end).expect("usize fits u64 on supported targets");
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let inner = self.inner.splice_leaves(
            arena,
            start..end,
            replacements
                .into_iter()
                .map(|page| SealedSequenceLeaf::new(page.owner))
                .collect(),
            &mut sequence_receipt,
        )?;
        merge_sequence_receipt(receipt, sequence_receipt);
        Ok(Self { inner })
    }

    pub fn release_later(self, arena: &mut PageArena) -> Result<(), EventTapeError> {
        self.inner.release_later(arena)
    }
}

fn merge_sequence_receipt(receipt: &mut EventTapeReceipt, sequence: SequenceMutationReceipt) {
    receipt.sequence_leaf_nodes_adopted += sequence.leaves_adopted;
    receipt.sequence_branch_nodes_allocated += sequence.branches_allocated;
    receipt.sequence_nodes_visited += sequence.nodes_visited;
    receipt.child_references_added += sequence.child_references_added;
    receipt.pages_reused += sequence.leaves_reused;
    receipt.maximum_streaming_sequence_roots = receipt
        .maximum_streaming_sequence_roots
        .max(sequence.maximum_streaming_roots);
    receipt.maximum_streaming_sequence_bin_slots = receipt
        .maximum_streaming_sequence_bin_slots
        .max(sequence.maximum_streaming_bin_slots);
    receipt.maximum_streaming_sequence_bin_bytes = receipt
        .maximum_streaming_sequence_bin_bytes
        .max(sequence.maximum_streaming_bin_bytes);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OutputSequenceRef {
    root: Option<ArenaId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageLocation {
    pub page: OutputPageId,
    pub page_index: u64,
    pub coverage_prefix: Position,
    pub nodes_visited: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoverageLocation {
    pub coverage: CoverageId,
    pub page: OutputPageId,
    pub page_range: PageRange,
    pub absolute_prefix: Position,
    pub local_offset: u64,
    pub at_document_end: bool,
    pub nodes_visited: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageRange {
    pub start: u64,
    pub end: u64,
}

impl OutputSequenceRef {
    pub fn summary(self, arena: &PageArena) -> Result<SequenceSummary, EventTapeError> {
        self.root.map_or_else(
            || Ok(SequenceSummary::default()),
            |root| sequence_node(arena, root).map(|value| value.0),
        )
    }

    pub fn locate_page(
        self,
        arena: &PageArena,
        page_index: u64,
    ) -> Result<Option<PageLocation>, EventTapeError> {
        let Some(mut node) = self.root else {
            return Ok(None);
        };
        let summary = sequence_node(arena, node)?.0;
        if page_index >= summary.pages {
            return Ok(None);
        }
        let mut index = page_index;
        let mut page_prefix = 0_u64;
        let mut coverage_prefix = Position::default();
        let mut nodes_visited = 0_usize;
        loop {
            nodes_visited += 1;
            let (_, kind) = sequence_node(arena, node)?;
            match kind {
                SequenceNodeKind::Page(page) => {
                    return Ok(Some(PageLocation {
                        page,
                        page_index: page_prefix,
                        coverage_prefix,
                        nodes_visited,
                    }));
                }
                SequenceNodeKind::Branch { left, right } => {
                    let left_summary = sequence_node(arena, left)?.0;
                    if index < left_summary.pages {
                        node = left;
                    } else {
                        index -= left_summary.pages;
                        page_prefix = page_prefix
                            .checked_add(left_summary.pages)
                            .ok_or(EventTapeError::Overflow("page prefix"))?;
                        coverage_prefix = coverage_prefix.checked_add(left_summary.coverage)?;
                        node = right;
                    }
                }
            }
        }
    }

    pub fn right_partition_root(
        self,
        arena: &PageArena,
    ) -> Result<Option<ArenaId>, EventTapeError> {
        let Some(root) = self.root else {
            return Ok(None);
        };
        Ok(match sequence_node(arena, root)?.1 {
            SequenceNodeKind::Page(_) => None,
            SequenceNodeKind::Branch { right, .. } => Some(right),
        })
    }

    pub fn locate_coordinate(
        self,
        arena: &PageArena,
        coordinate: Coordinate,
        offset: u64,
    ) -> Result<Option<CoverageLocation>, EventTapeError> {
        let summary = self.summary(arena)?;
        let total = summary.coverage.coordinate(coordinate);
        if total == 0 || offset > total {
            return Ok(None);
        }
        let at_document_end = offset == total;
        let target = if at_document_end { total - 1 } else { offset };
        let mut node = self.root.expect("nonzero summary has a root");
        let mut page_index = 0_u64;
        let mut absolute_prefix = Position::default();
        let mut nodes_visited = 0_usize;
        loop {
            nodes_visited += 1;
            let (_, kind) = sequence_node(arena, node)?;
            match kind {
                SequenceNodeKind::Page(page) => {
                    let view = read_event_page(arena, page)?;
                    let mut record_prefix = absolute_prefix;
                    for (index, record) in view.coverage.iter().enumerate() {
                        let start = record_prefix.coordinate(coordinate);
                        let end = start
                            .checked_add(record.length.coordinate(coordinate))
                            .ok_or(EventTapeError::Overflow("coverage record end"))?;
                        let is_last_document_record =
                            at_document_end && index + 1 == view.coverage.len() && end == total;
                        if target < end || is_last_document_record {
                            let trailing = if index + 1 == view.coverage.len() {
                                self.trailing_zero_pages_after(arena, page_index)?
                            } else {
                                0
                            };
                            return Ok(Some(CoverageLocation {
                                coverage: record.id,
                                page,
                                page_range: PageRange {
                                    start: page_index,
                                    end: page_index
                                        .checked_add(1)
                                        .and_then(|value| value.checked_add(trailing))
                                        .ok_or(EventTapeError::Overflow("coverage page range"))?,
                                },
                                absolute_prefix: record_prefix,
                                local_offset: if at_document_end {
                                    record.length.coordinate(coordinate)
                                } else {
                                    offset - start
                                },
                                at_document_end,
                                nodes_visited,
                            }));
                        }
                        record_prefix = record_prefix.checked_add(record.length)?;
                    }
                    return Err(EventTapeError::Corrupt(
                        "coordinate descended into a zero-coverage page",
                    ));
                }
                SequenceNodeKind::Branch { left, right } => {
                    let left_summary = sequence_node(arena, left)?.0;
                    let left_end = absolute_prefix
                        .coordinate(coordinate)
                        .checked_add(left_summary.coverage.coordinate(coordinate))
                        .ok_or(EventTapeError::Overflow("coordinate branch end"))?;
                    if target < left_end {
                        node = left;
                    } else {
                        page_index = page_index
                            .checked_add(left_summary.pages)
                            .ok_or(EventTapeError::Overflow("coordinate page index"))?;
                        absolute_prefix = absolute_prefix.checked_add(left_summary.coverage)?;
                        node = right;
                    }
                }
            }
        }
    }

    fn trailing_zero_pages_after(
        self,
        arena: &PageArena,
        page_index: u64,
    ) -> Result<u64, EventTapeError> {
        let Some(mut node) = self.root else {
            return Ok(0);
        };
        let mut index = page_index;
        let mut right_siblings = Vec::new();
        loop {
            match sequence_node(arena, node)?.1 {
                SequenceNodeKind::Page(_) => break,
                SequenceNodeKind::Branch { left, right } => {
                    let left_pages = sequence_node(arena, left)?.0.pages;
                    if index < left_pages {
                        right_siblings.push(right);
                        node = left;
                    } else {
                        index -= left_pages;
                        node = right;
                    }
                }
            }
        }
        let mut trailing = 0_u64;
        for sibling in right_siblings.into_iter().rev() {
            let summary = sequence_node(arena, sibling)?.0;
            trailing = trailing
                .checked_add(summary.leading_zero_coverage_pages)
                .ok_or(EventTapeError::Overflow("trailing zero pages"))?;
            if summary.leading_zero_coverage_pages != summary.pages {
                break;
            }
        }
        Ok(trailing)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectionView {
    pub frames: Vec<ProjectionFrame>,
    pub checkpoint_nodes_visited: usize,
    pub pages_folded: usize,
    pub events_folded: usize,
    pub earlier_pages_replayed: usize,
}

impl OutputSequenceRef {
    pub fn fold_viewport(
        self,
        arena: &PageArena,
        pages: PageRange,
        projection_fuel: usize,
    ) -> Result<ProjectionView, EventTapeError> {
        let summary = self.summary(arena)?;
        if pages.start > pages.end || pages.end > summary.pages {
            return Err(EventTapeError::InvalidPageRange);
        }
        if pages.start == pages.end {
            return Ok(ProjectionView::default());
        }
        let first = self
            .locate_page(arena, pages.start)?
            .ok_or(EventTapeError::InvalidPageRange)?;
        let first_page = read_event_page(arena, first.page)?;
        let (frames, checkpoint_nodes_visited) =
            load_projection(arena, first_page.leading_projection, projection_fuel)?;
        let mut view = ProjectionView {
            frames,
            checkpoint_nodes_visited,
            pages_folded: 0,
            events_folded: 0,
            earlier_pages_replayed: 0,
        };
        for page_index in pages.start..pages.end {
            let location = self
                .locate_page(arena, page_index)?
                .ok_or(EventTapeError::InvalidPageRange)?;
            let page = if page_index == pages.start {
                first_page.clone()
            } else {
                read_event_page(arena, location.page)?
            };
            for event in &page.events {
                fold_projection_event(&mut view.frames, &event.event)?;
                view.events_folded += 1;
            }
            view.pages_folded += 1;
        }
        Ok(view)
    }

    pub fn fold_coverage_location(
        self,
        arena: &PageArena,
        location: CoverageLocation,
        projection_fuel: usize,
    ) -> Result<ProjectionView, EventTapeError> {
        self.fold_viewport(arena, location.page_range, projection_fuel)
    }
}

fn fold_projection_event(
    frames: &mut Vec<ProjectionFrame>,
    event: &StructuralEvent,
) -> Result<(), EventTapeError> {
    match event {
        StructuralEvent::Open {
            block,
            parent,
            kind_tag,
            start,
        } => {
            if frames.last().map(|frame| frame.block) != *parent {
                return Err(EventTapeError::ProjectionMismatch(
                    "folded open parent is not projection top",
                ));
            }
            frames.push(ProjectionFrame {
                block: *block,
                kind_tag: *kind_tag,
                context: 0,
                start: *start,
                current: *start,
            });
        }
        StructuralEvent::Promote {
            block,
            kind_tag,
            context,
        } => {
            let frame = frames
                .iter_mut()
                .rev()
                .find(|frame| frame.block == *block)
                .ok_or(EventTapeError::ProjectionMismatch(
                    "folded promotion target is not open",
                ))?;
            frame.kind_tag = *kind_tag;
            frame.context = *context;
        }
        StructuralEvent::WriteEnd { block, end } => {
            let frame = frames
                .iter_mut()
                .rev()
                .find(|frame| frame.block == *block)
                .ok_or(EventTapeError::ProjectionMismatch(
                    "folded end target is not open",
                ))?;
            frame.current = *end;
        }
        StructuralEvent::Close { block } => {
            if frames.last().map(|frame| frame.block) != Some(*block) {
                return Err(EventTapeError::ProjectionMismatch(
                    "folded close does not match projection top",
                ));
            }
            frames.pop();
        }
        StructuralEvent::AppendRuns { .. }
        | StructuralEvent::DrainRunPrefix { .. }
        | StructuralEvent::RepairListEnds { .. }
        | StructuralEvent::Definition { .. }
        | StructuralEvent::Finalize { .. } => {}
    }
    Ok(())
}

/// One acyclic lifetime root. The event sequence is retained through an arena
/// child edge; its ownership token is consumed during construction.
#[derive(Debug)]
pub struct OutputRootManifest {
    owner: OwnedArenaRef,
}

impl OutputRootManifest {
    pub fn build(
        arena: &mut PageArena,
        sequence: OutputSequence,
        receipt: &mut EventTapeReceipt,
    ) -> Result<Self, EventTapeError> {
        let mut payload = Vec::with_capacity(8);
        payload.push(OUTPUT_MANIFEST_TAG);
        payload.push(FORMAT_VERSION);
        payload.extend_from_slice(&[0; 6]);
        let root = sequence.inner.into_owner();
        let mut transaction = ArenaBuildTransaction::new(arena);
        let root = root.map(|owner| transaction.track(owner)).transpose()?;
        let children = root
            .as_ref()
            .map_or_else(Vec::new, |owner| vec![transaction.id(owner)]);
        let (manifest, allocation) = transaction.allocate(&payload, &children)?;
        receipt.child_references_added += allocation.child_references_added;
        if let Some(owner) = root {
            transaction.release(owner)?;
        }
        Ok(Self {
            owner: transaction.take(manifest),
        })
    }

    #[must_use]
    pub const fn id(&self) -> OutputRootId {
        OutputRootId(self.owner.id())
    }

    pub fn events(&self, arena: &PageArena) -> Result<OutputSequenceRef, EventTapeError> {
        let payload = arena.payload(self.owner.id())?;
        if payload != [OUTPUT_MANIFEST_TAG, FORMAT_VERSION, 0, 0, 0, 0, 0, 0] {
            return Err(EventTapeError::Corrupt("wrong output manifest"));
        }
        let children = arena.children(self.owner.id())?;
        if children[1].is_some() {
            return Err(EventTapeError::Corrupt("manifest has extra component"));
        }
        Ok(OutputSequenceRef { root: children[0] })
    }

    pub fn release_later(self, arena: &mut PageArena) -> Result<(), EventTapeError> {
        arena
            .release_later(self.owner)
            .map_err(legacy_owner_transfer_error)?;
        Ok(())
    }
}

//! Falsification slice for a genuinely resumable inline resolver.
//!
//! This module deliberately replaces the atomic, region-sized resolver in
//! [`crate::grammar`] for the narrow ASCII `CommonMark` seam made from code
//! spans, emphasis, and strong emphasis. It consumes the one immutable lexical
//! root produced by [`crate::frontier::SharedLexer`]. There is no leaf-size or
//! event-count cutoff and unsupported input is left literal by the delimiter
//! algorithm rather than represented by a semantic fallback record.
//!
//! The emphasis stack is a fixed-page radix arena (not a document-sized
//! `Vec`). Opener searches advance one stack entry per transition. Resolution
//! first writes a temporary role overlay, then a third resumable pass replays
//! lexical events in source order and appends a delta/varint span stream.
//! Allocation, copying, hashing, replay, and page reclamation are separate
//! charged dimensions.
//!
//! This is still a falsification experiment, not the final inline core:
//!
//! * flanking classification is the ASCII subset already claimed by the
//!   integrated grammar slice;
//! * code spans use a deterministic linear two-pass equivalent of
//!   pulldown-cmark's run-length cache: a compact run table is indexed in
//!   reverse by a sparse fixed-page last-occurrence map, then resolved in one
//!   forward pass;
//! * the upstream shared lexer and logical-cursor constructors still have
//!   setup/accounting gaps documented elsewhere in the crate.
//! * resolution writes compact role/match facts plus a lexical-event-ordinal
//!   head overlay. A final bounded replay consumes that overlay in source
//!   order, reclaims temporary pages, and emits the permanent compact stream.
//!   No whole-result sort or flattening pass exists.

use std::array;
use std::mem::size_of;

use crate::frontier::{
    CursorMetrics, CursorStep, InlineLexicalInput, LexicalCursorMetrics, LexicalCursorStep,
    LexicalEvent, LexicalEventKind, LogicalByte, LogicalCursor, MeteredLexicalCursor,
    SegmentedLeafIdentity,
};

const RADIX_BITS: usize = 6;
const RADIX: usize = 1 << RADIX_BITS;
const RADIX_MASK: usize = RADIX - 1;
const RADIX_LEVELS: usize = 4;
const STORAGE_PAGE_BYTES: usize = 4096;
const PACKED_INLINE_EL_BYTES: usize = 16;
const PACKED_CODE_RUN_BYTES: usize = 16;
const PACKED_CODE_SPAN_BYTES: usize = 12;
const PACKED_FACT_BYTES: usize = 8;
const PACKED_ROLE_HEAD_BYTES: usize = 3;
const MAX_ENCODED_SPAN_BYTES: usize = 16;
const INLINE_ELS_PER_PAGE: usize = STORAGE_PAGE_BYTES / PACKED_INLINE_EL_BYTES;
const CODE_RUNS_PER_PAGE: usize = STORAGE_PAGE_BYTES / PACKED_CODE_RUN_BYTES;
const CODE_SPANS_PER_PAGE: usize = STORAGE_PAGE_BYTES / PACKED_CODE_SPAN_BYTES;
const FACTS_PER_PAGE: usize = STORAGE_PAGE_BYTES / PACKED_FACT_BYTES;
const ROLE_HEADS_PER_PAGE: usize = STORAGE_PAGE_BYTES / PACKED_ROLE_HEAD_BYTES;
/// Allocation size transferred by [`InlineOutputPageDrain`].
pub const INLINE_OUTPUT_PAGE_BYTES: usize = STORAGE_PAGE_BYTES;
const STREAM_BYTES_PER_PAGE: usize = INLINE_OUTPUT_PAGE_BYTES;
const FACT_PAGE_COUNTS_PER_PAGE: usize = STORAGE_PAGE_BYTES / size_of::<u16>();
const MAP_ENTRIES_PER_PAGE: usize = STORAGE_PAGE_BYTES / size_of::<u32>();
const MAX_PACKED_OFFSET: usize = (1 << 30) - 1;
const PACKED_LINK_MASK: u32 = (1 << 30) - 1;
const ROLE_HEAD_OVERFLOW_SENTINEL: u32 = (1 << 24) - 1;
const MAX_EVENT_DECODE_BYTES: usize = 12;
const MAX_SOURCE_INDEX_NODES_PER_STEP: usize = usize::BITS as usize * 2;
const HASH_BASE: u64 = 0x0000_0100_0000_01b3;

/// Largest cooperative scheduler slice accepted by [`InlineMachine::poll_cooperative`].
///
/// The scalar path bounds wall-clock work by a transition count and relies on
/// the fixed atomic limits below. It avoids predicting and subtracting every
/// [`InlineWork`] dimension before every state-machine transition; the exact
/// multidimensional [`InlineMachine::poll`] remains available for validation
/// and callers that need independently chosen permits.
pub const MAX_INLINE_COOPERATIVE_TRANSITIONS: usize = 4096;

/// Largest explicit cancellation-reclamation slice. Cancellation transitions
/// can release a 4 KiB allocation, so this lower cap avoids turning a parser
/// slice into a multi-megabyte deallocation burst.
pub const MAX_INLINE_CANCEL_TRANSITIONS: usize = 256;

/// No inline transition installs or releases more than one fixed-depth radix
/// path (one 4 KiB page plus at most three directory nodes).
pub const MAX_INLINE_ATOMIC_PAGE_OPERATIONS: usize = RADIX_LEVELS;

/// Maximum allocation or reclamation bytes caused by one inline transition.
/// All radix payload pages are at most 4 KiB and directory node sizes do not
/// depend on their record type.
pub const MAX_INLINE_ATOMIC_PAGE_BYTES: usize = size_of::<RadixTop<u8, STREAM_BYTES_PER_PAGE>>()
    + size_of::<RadixMiddle<u8, STREAM_BYTES_PER_PAGE>>()
    + size_of::<RadixLeaf<u8, STREAM_BYTES_PER_PAGE>>()
    + STORAGE_PAGE_BYTES;

/// Maximum bytes explicitly copied by one inline transition.
pub const MAX_INLINE_ATOMIC_COPY_BYTES: usize = PACKED_CODE_RUN_BYTES + size_of::<u32>();

/// [`InlineMachine::poll_cancel`] now reclaims the machine's radix pages in
/// fixed-depth transitions. The remaining ownership gap is the last-owner
/// shared lexical/source `Arc` (and callers directly dropping a standalone
/// [`InlineOutput`] instead of consuming its page drain); these still require
/// transfer to the crate's integer-ID page arena before production use.
pub const REMAINING_RECLAMATION_GAP: &str =
    "machine radix cancellation is resumable, but last-owner lexical/source Arc destruction and direct standalone-output Drop still need PageArena reclaim tickets";

/// Cursor construction is not yet fully admitted by this machine's receipt.
/// `LogicalCursor` creates a depth-bounded `SourceCursor` traversal `Vec`; the
/// source API reports index nodes visited but not the allocation/capacity, and
/// the shared segmented/lexical roots are accounted by their owning layers.
pub const REMAINING_CURSOR_ACCOUNTING_GAP: &str =
    "LogicalCursor/SourceCursor setup still allocates an uncharged depth-bounded traversal Vec; replace it with fixed storage or expose exact allocation admission";

/// One semantic inline range. Text is implicit outside these ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InlineSpanKind {
    Code,
    Emphasis,
    Strong,
}

impl InlineSpanKind {
    const fn tag(self) -> u8 {
        match self {
            Self::Code => 1,
            Self::Emphasis => 2,
            Self::Strong => 3,
        }
    }

    fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Code),
            2 => Some(Self::Emphasis),
            3 => Some(Self::Strong),
            _ => None,
        }
    }
}

/// Exact delimiter and content ranges for one resolved inline span.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InlineSpan {
    pub kind: InlineSpanKind,
    pub opener_start: usize,
    pub opener_end: usize,
    pub closer_start: usize,
    pub closer_end: usize,
    pub content_start: usize,
    pub content_end: usize,
}

impl InlineSpan {
    /// Complete source range occupied by the construct.
    #[must_use]
    pub const fn start(self) -> usize {
        self.opener_start
    }

    /// Complete source range occupied by the construct.
    #[must_use]
    pub const fn end(self) -> usize {
        self.closer_end
    }
}

#[derive(Clone, Copy, Debug)]
struct PackedInlineEl([u8; PACKED_INLINE_EL_BYTES]);

impl Default for PackedInlineEl {
    fn default() -> Self {
        Self([0; PACKED_INLINE_EL_BYTES])
    }
}

impl PackedInlineEl {
    fn encode(value: InlineEl) -> Self {
        assert!(value.start <= MAX_PACKED_OFFSET);
        assert!(value.count <= MAX_PACKED_OFFSET);
        assert!(value.run_length <= MAX_PACKED_OFFSET);
        let mut bytes = [0u8; PACKED_INLINE_EL_BYTES];
        bytes[..4].copy_from_slice(&as_u32(value.start).to_le_bytes());
        bytes[4..8].copy_from_slice(&as_u32(value.count).to_le_bytes());
        let flags = as_u32(value.run_length)
            | if value.marker == b'_' { 1 << 30 } else { 0 }
            | if value.both { 1 << 31 } else { 0 };
        bytes[8..12].copy_from_slice(&flags.to_le_bytes());
        bytes[12..].copy_from_slice(&as_u32(value.event_ordinal).to_le_bytes());
        Self(bytes)
    }

    fn decode(self) -> InlineEl {
        let start = read_u32(&self.0[..4]);
        let count = read_u32(&self.0[4..8]);
        let flags = read_u32(&self.0[8..12]);
        InlineEl {
            start: start as usize,
            count: count as usize,
            run_length: (flags & ((1 << 30) - 1)) as usize,
            marker: if flags & (1 << 30) == 0 { b'*' } else { b'_' },
            both: flags & (1 << 31) != 0,
            event_ordinal: read_u32(&self.0[12..]) as usize,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PackedCodeRun([u8; PACKED_CODE_RUN_BYTES]);

impl Default for PackedCodeRun {
    fn default() -> Self {
        Self([0; PACKED_CODE_RUN_BYTES])
    }
}

impl PackedCodeRun {
    fn encode(start: usize, len: usize, next: Option<usize>, event_ordinal: usize) -> Self {
        assert!(u32::try_from(start).is_ok());
        assert!(u32::try_from(len).is_ok());
        let next = next.map_or(0, |value| as_u32(value + 1));
        let mut bytes = [0u8; PACKED_CODE_RUN_BYTES];
        bytes[..4].copy_from_slice(&as_u32(start).to_le_bytes());
        bytes[4..8].copy_from_slice(&as_u32(len).to_le_bytes());
        bytes[8..12].copy_from_slice(&next.to_le_bytes());
        bytes[12..].copy_from_slice(&as_u32(event_ordinal).to_le_bytes());
        Self(bytes)
    }

    fn decode(self) -> CodeRun {
        let next = read_u32(&self.0[8..12]);
        CodeRun {
            start: read_u32(&self.0[..4]) as usize,
            len: read_u32(&self.0[4..8]) as usize,
            next: (next != 0).then(|| next as usize - 1),
            event_ordinal: read_u32(&self.0[12..]) as usize,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct CodeRun {
    start: usize,
    len: usize,
    next: Option<usize>,
    event_ordinal: usize,
}

#[derive(Clone, Copy, Debug)]
struct PackedCodeSpan([u8; PACKED_CODE_SPAN_BYTES]);

impl Default for PackedCodeSpan {
    fn default() -> Self {
        Self([0; PACKED_CODE_SPAN_BYTES])
    }
}

impl PackedCodeSpan {
    fn encode(span: InlineSpan) -> Self {
        debug_assert_eq!(span.kind, InlineSpanKind::Code);
        let width = span.opener_end - span.opener_start;
        debug_assert_eq!(width, span.closer_end - span.closer_start);
        let mut bytes = [0u8; PACKED_CODE_SPAN_BYTES];
        bytes[..4].copy_from_slice(&as_u32(span.opener_start).to_le_bytes());
        bytes[4..8].copy_from_slice(&as_u32(span.closer_start).to_le_bytes());
        bytes[8..].copy_from_slice(&as_u32(width).to_le_bytes());
        Self(bytes)
    }

    fn decode(self) -> InlineSpan {
        let opener_start = read_u32(&self.0[..4]) as usize;
        let closer_start = read_u32(&self.0[4..8]) as usize;
        let width = read_u32(&self.0[8..]) as usize;
        let opener_end = opener_start + width;
        InlineSpan {
            kind: InlineSpanKind::Code,
            opener_start,
            opener_end,
            closer_start,
            closer_end: closer_start + width,
            content_start: opener_end,
            content_end: closer_start,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PackedFact([u8; PACKED_FACT_BYTES]);

impl Default for PackedFact {
    fn default() -> Self {
        Self([0; PACKED_FACT_BYTES])
    }
}

impl PackedFact {
    fn encode(span: InlineSpan, next: Option<usize>) -> Self {
        let next = next.map_or(0, |index| {
            assert!(index < MAX_PACKED_OFFSET);
            as_u32(index + 1)
        });
        let link_and_kind = next | (u32::from(span.kind.tag()) << 30);
        let mut bytes = [0u8; PACKED_FACT_BYTES];
        bytes[..4].copy_from_slice(&as_u32(span.closer_start).to_le_bytes());
        bytes[4..].copy_from_slice(&link_and_kind.to_le_bytes());
        Self(bytes)
    }

    fn decode(self) -> Option<InlineFact> {
        let closer_start = read_u32(&self.0[..4]) as usize;
        let link_and_kind = read_u32(&self.0[4..]);
        let kind = InlineSpanKind::from_tag((link_and_kind >> 30) as u8)?;
        let next = link_and_kind & PACKED_LINK_MASK;
        Some(InlineFact {
            closer_start,
            kind,
            next: (next != 0)
                .then(|| usize::try_from(next).expect("30-bit fact ordinal fits usize") - 1),
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct InlineFact {
    closer_start: usize,
    kind: InlineSpanKind,
    next: Option<usize>,
}

impl InlineFact {
    fn width(self, event: LexicalEvent) -> Option<usize> {
        match self.kind {
            InlineSpanKind::Code => match event.kind {
                LexicalEventKind::BacktickRun { len } => Some(len),
                _ => None,
            },
            InlineSpanKind::Emphasis => Some(1),
            InlineSpanKind::Strong => Some(2),
        }
    }

    fn span(self, opener_start: usize, event: LexicalEvent) -> Option<InlineSpan> {
        let width = self.width(event)?;
        let opener_end = opener_start.checked_add(width)?;
        let closer_end = self.closer_start.checked_add(width)?;
        Some(InlineSpan {
            kind: self.kind,
            opener_start,
            opener_end,
            closer_start: self.closer_start,
            closer_end,
            content_start: opener_end,
            content_end: self.closer_start,
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct PackedRoleHead([u8; PACKED_ROLE_HEAD_BYTES]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoleHead {
    None,
    Direct(usize),
    Overflow,
}

impl PackedRoleHead {
    fn direct(index: usize) -> Option<Self> {
        let encoded = index.checked_add(1)?;
        if encoded >= ROLE_HEAD_OVERFLOW_SENTINEL as usize {
            return None;
        }
        Some(Self::encode_raw(as_u32(encoded)))
    }

    const fn overflow() -> Self {
        Self([0xff; PACKED_ROLE_HEAD_BYTES])
    }

    fn decode(self) -> RoleHead {
        let raw = u32::from(self.0[0]) | (u32::from(self.0[1]) << 8) | (u32::from(self.0[2]) << 16);
        match raw {
            0 => RoleHead::None,
            ROLE_HEAD_OVERFLOW_SENTINEL => RoleHead::Overflow,
            value => {
                RoleHead::Direct(usize::try_from(value).expect("24-bit role head fits usize") - 1)
            }
        }
    }

    fn encode_raw(raw: u32) -> Self {
        debug_assert!(raw < ROLE_HEAD_OVERFLOW_SENTINEL);
        let bytes = raw.to_le_bytes();
        Self([bytes[0], bytes[1], bytes[2]])
    }
}

fn as_u32(value: usize) -> u32 {
    u32::try_from(value).expect("prototype packed offset fits u32")
}

fn read_u32(bytes: &[u8]) -> u32 {
    let mut value = [0u8; 4];
    value.copy_from_slice(&bytes[..4]);
    u32::from_le_bytes(value)
}

#[derive(Clone, Copy, Debug, Default)]
struct InlineEl {
    start: usize,
    count: usize,
    run_length: usize,
    marker: u8,
    both: bool,
    event_ordinal: usize,
}

#[derive(Debug)]
struct RadixLeaf<T: Copy + Default, const PAGE_RECORDS: usize> {
    pages: [Option<Box<[T; PAGE_RECORDS]>>; RADIX],
    live_pages: u8,
}

impl<T: Copy + Default, const PAGE_RECORDS: usize> RadixLeaf<T, PAGE_RECORDS> {
    fn new() -> Self {
        Self {
            pages: array::from_fn(|_| None),
            live_pages: 0,
        }
    }
}

#[derive(Debug)]
struct RadixMiddle<T: Copy + Default, const PAGE_RECORDS: usize> {
    children: [Option<Box<RadixLeaf<T, PAGE_RECORDS>>>; RADIX],
    live_children: u8,
}

impl<T: Copy + Default, const PAGE_RECORDS: usize> RadixMiddle<T, PAGE_RECORDS> {
    fn new() -> Self {
        Self {
            children: array::from_fn(|_| None),
            live_children: 0,
        }
    }
}

#[derive(Debug)]
struct RadixTop<T: Copy + Default, const PAGE_RECORDS: usize> {
    children: [Option<Box<RadixMiddle<T, PAGE_RECORDS>>>; RADIX],
    live_children: u8,
}

impl<T: Copy + Default, const PAGE_RECORDS: usize> RadixTop<T, PAGE_RECORDS> {
    fn new() -> Self {
        Self {
            children: array::from_fn(|_| None),
            live_children: 0,
        }
    }
}

/// Four-level, fixed-fanout page directory. The root is inline; all other
/// allocations are counted exactly when first installed.
#[derive(Debug)]
struct RadixPages<T: Copy + Default, const PAGE_RECORDS: usize> {
    roots: [Option<Box<RadixTop<T, PAGE_RECORDS>>>; RADIX],
    retained_allocations: usize,
    retained_bytes: usize,
}

impl<T: Copy + Default, const PAGE_RECORDS: usize> Default for RadixPages<T, PAGE_RECORDS> {
    fn default() -> Self {
        Self {
            roots: array::from_fn(|_| None),
            retained_allocations: 0,
            retained_bytes: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct AllocationNeed {
    allocations: usize,
    bytes: usize,
}

#[derive(Debug)]
struct TakenRadixPage<T: Copy + Default, const PAGE_RECORDS: usize> {
    page: Box<[T; PAGE_RECORDS]>,
    leaf: Option<Box<RadixLeaf<T, PAGE_RECORDS>>>,
    middle: Option<Box<RadixMiddle<T, PAGE_RECORDS>>>,
    top: Option<Box<RadixTop<T, PAGE_RECORDS>>>,
}

impl<T: Copy + Default, const PAGE_RECORDS: usize> TakenRadixPage<T, PAGE_RECORDS> {
    const fn allocation_need(&self) -> AllocationNeed {
        AllocationNeed {
            allocations: 1
                + self.leaf.is_some() as usize
                + self.middle.is_some() as usize
                + self.top.is_some() as usize,
            bytes: size_of::<[T; PAGE_RECORDS]>()
                + self.leaf.is_some() as usize * size_of::<RadixLeaf<T, PAGE_RECORDS>>()
                + self.middle.is_some() as usize * size_of::<RadixMiddle<T, PAGE_RECORDS>>()
                + self.top.is_some() as usize * size_of::<RadixTop<T, PAGE_RECORDS>>(),
        }
    }
}

impl<T: Copy + Default, const PAGE_RECORDS: usize> RadixPages<T, PAGE_RECORDS> {
    fn page_coordinates(page: usize) -> [usize; RADIX_LEVELS] {
        let capacity = 1usize << (RADIX_BITS * RADIX_LEVELS);
        assert!(page < capacity, "radix page address space exhausted");
        [
            (page >> (RADIX_BITS * 3)) & RADIX_MASK,
            (page >> (RADIX_BITS * 2)) & RADIX_MASK,
            (page >> RADIX_BITS) & RADIX_MASK,
            page & RADIX_MASK,
        ]
    }

    fn allocation_need(&self, record: usize) -> AllocationNeed {
        let page = record / PAGE_RECORDS;
        let [a, b, c, d] = Self::page_coordinates(page);
        let mut need = AllocationNeed::default();
        let Some(top) = &self.roots[a] else {
            return AllocationNeed {
                allocations: 4,
                bytes: size_of::<RadixTop<T, PAGE_RECORDS>>()
                    + size_of::<RadixMiddle<T, PAGE_RECORDS>>()
                    + size_of::<RadixLeaf<T, PAGE_RECORDS>>()
                    + size_of::<[T; PAGE_RECORDS]>(),
            };
        };
        let Some(middle) = &top.children[b] else {
            return AllocationNeed {
                allocations: 3,
                bytes: size_of::<RadixMiddle<T, PAGE_RECORDS>>()
                    + size_of::<RadixLeaf<T, PAGE_RECORDS>>()
                    + size_of::<[T; PAGE_RECORDS]>(),
            };
        };
        let Some(leaf) = &middle.children[c] else {
            return AllocationNeed {
                allocations: 2,
                bytes: size_of::<RadixLeaf<T, PAGE_RECORDS>>() + size_of::<[T; PAGE_RECORDS]>(),
            };
        };
        if leaf.pages[d].is_none() {
            need.allocations = 1;
            need.bytes = size_of::<[T; PAGE_RECORDS]>();
        }
        need
    }

    fn ensure_page(&mut self, record: usize) -> AllocationNeed {
        let page = record / PAGE_RECORDS;
        let [a, b, c, d] = Self::page_coordinates(page);
        let mut actual = AllocationNeed::default();
        if self.roots[a].is_none() {
            self.roots[a] = Some(Box::new(RadixTop::new()));
            actual.allocations += 1;
            actual.bytes += size_of::<RadixTop<T, PAGE_RECORDS>>();
        }
        let top = self.roots[a].as_mut().expect("top was installed");
        if top.children[b].is_none() {
            top.children[b] = Some(Box::new(RadixMiddle::new()));
            top.live_children += 1;
            actual.allocations += 1;
            actual.bytes += size_of::<RadixMiddle<T, PAGE_RECORDS>>();
        }
        let middle = top.children[b].as_mut().expect("middle was installed");
        if middle.children[c].is_none() {
            middle.children[c] = Some(Box::new(RadixLeaf::new()));
            middle.live_children += 1;
            actual.allocations += 1;
            actual.bytes += size_of::<RadixLeaf<T, PAGE_RECORDS>>();
        }
        let leaf = middle.children[c].as_mut().expect("leaf was installed");
        if leaf.pages[d].is_none() {
            leaf.pages[d] = Some(Box::new([T::default(); PAGE_RECORDS]));
            leaf.live_pages += 1;
            actual.allocations += 1;
            actual.bytes += size_of::<[T; PAGE_RECORDS]>();
        }
        self.retained_allocations += actual.allocations;
        self.retained_bytes += actual.bytes;
        actual
    }

    fn set(&mut self, record: usize, value: T) {
        let page = record / PAGE_RECORDS;
        let slot = record % PAGE_RECORDS;
        let [a, b, c, d] = Self::page_coordinates(page);
        self.roots[a]
            .as_mut()
            .expect("record page was ensured")
            .children[b]
            .as_mut()
            .expect("record page was ensured")
            .children[c]
            .as_mut()
            .expect("record page was ensured")
            .pages[d]
            .as_mut()
            .expect("record page was ensured")[slot] = value;
    }

    fn get(&self, record: usize) -> Option<T> {
        let page = record / PAGE_RECORDS;
        let slot = record % PAGE_RECORDS;
        let [a, b, c, d] = Self::page_coordinates(page);
        Some(
            self.roots[a].as_ref()?.children[b].as_ref()?.children[c]
                .as_ref()?
                .pages[d]
                .as_ref()?[slot],
        )
    }

    fn reclaim_need(&self, page: usize) -> AllocationNeed {
        let [a, b, c, d] = Self::page_coordinates(page);
        let Some(top) = self.roots[a].as_ref() else {
            return AllocationNeed::default();
        };
        let Some(middle) = top.children[b].as_ref() else {
            return AllocationNeed::default();
        };
        let Some(leaf) = middle.children[c].as_ref() else {
            return AllocationNeed::default();
        };
        if leaf.pages[d].is_none() {
            return AllocationNeed::default();
        }
        let mut need = AllocationNeed {
            allocations: 1,
            bytes: size_of::<[T; PAGE_RECORDS]>(),
        };
        if leaf.live_pages == 1 {
            need.allocations += 1;
            need.bytes += size_of::<RadixLeaf<T, PAGE_RECORDS>>();
            if middle.live_children == 1 {
                need.allocations += 1;
                need.bytes += size_of::<RadixMiddle<T, PAGE_RECORDS>>();
                if top.live_children == 1 {
                    need.allocations += 1;
                    need.bytes += size_of::<RadixTop<T, PAGE_RECORDS>>();
                }
            }
        }
        need
    }

    fn remove_page(&mut self, page: usize) -> AllocationNeed {
        let removed = self.take_page(page);
        let actual = removed
            .as_ref()
            .map_or_else(AllocationNeed::default, TakenRadixPage::allocation_need);
        drop(removed);
        actual
    }

    fn take_page(&mut self, page: usize) -> Option<TakenRadixPage<T, PAGE_RECORDS>> {
        let [a, b, c, d] = Self::page_coordinates(page);
        let (page, remove_leaf) = {
            let top = self.roots[a].as_mut()?;
            let middle = top.children[b].as_mut()?;
            let leaf = middle.children[c].as_mut()?;
            let removed = leaf.pages[d].take()?;
            leaf.live_pages -= 1;
            (removed, leaf.live_pages == 0)
        };
        let mut leaf = None;
        let mut middle = None;
        let mut top = None;
        let mut remove_middle = false;
        if remove_leaf {
            let top_ref = self.roots[a].as_mut().expect("live top exists");
            let middle_ref = top_ref.children[b].as_mut().expect("live middle exists");
            leaf = middle_ref.children[c].take();
            middle_ref.live_children -= 1;
            remove_middle = middle_ref.live_children == 0;
        }
        let mut remove_top = false;
        if remove_middle {
            let top_ref = self.roots[a].as_mut().expect("live top exists");
            middle = top_ref.children[b].take();
            top_ref.live_children -= 1;
            remove_top = top_ref.live_children == 0;
        }
        if remove_top {
            top = self.roots[a].take();
        }
        let removed = TakenRadixPage {
            page,
            leaf,
            middle,
            top,
        };
        let actual = removed.allocation_need();
        self.retained_allocations -= actual.allocations;
        self.retained_bytes -= actual.bytes;
        Some(removed)
    }
}

#[derive(Clone, Copy, Debug)]
struct EncodedSpan {
    bytes: [u8; MAX_ENCODED_SPAN_BYTES],
    len: u8,
}

impl EncodedSpan {
    fn new(span: InlineSpan, previous_opener_start: usize) -> Self {
        assert!(span.opener_start >= previous_opener_start);
        let mut bytes = [0u8; MAX_ENCODED_SPAN_BYTES];
        bytes[0] = span.kind.tag();
        let mut cursor = 1;
        cursor = write_u32_varint(
            &mut bytes,
            cursor,
            as_u32(span.opener_start - previous_opener_start),
        );
        cursor = write_u32_varint(
            &mut bytes,
            cursor,
            as_u32(span.opener_end - span.opener_start),
        );
        cursor = write_u32_varint(
            &mut bytes,
            cursor,
            as_u32(span.closer_start - span.opener_end),
        );
        Self {
            bytes,
            len: u8::try_from(cursor).expect("encoded span fits fixed scratch"),
        }
    }
}

fn write_u32_varint(output: &mut [u8], mut cursor: usize, mut value: u32) -> usize {
    loop {
        let low = (value & 0x7f) as u8;
        value >>= 7;
        output[cursor] = if value == 0 { low } else { low | 0x80 };
        cursor += 1;
        if value == 0 {
            return cursor;
        }
    }
}

#[derive(Debug, Default)]
struct SpanStream {
    bytes: RadixPages<u8, STREAM_BYTES_PER_PAGE>,
    byte_len: usize,
    span_count: usize,
}

impl SpanStream {
    fn get(&self, index: usize) -> Option<u8> {
        (index < self.byte_len)
            .then(|| self.bytes.get(index))
            .flatten()
    }
}

/// Multidimensional caller permit and per-poll measured delta.
///
/// Every field is a hard upper bound. A poll stops before an operation whose
/// conservative reservation would cross any dimension.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InlineWork {
    pub transitions: usize,
    pub lexical_root_clones: usize,
    pub lexical_tree_nodes: usize,
    pub lexical_pages_entered: usize,
    pub lexical_events: usize,
    pub lexical_decode_bytes: usize,
    pub source_cursor_starts: usize,
    pub source_cursor_steps: usize,
    pub source_logical_bytes: usize,
    pub source_descriptor_entries: usize,
    pub source_excluded_bytes: usize,
    pub source_seek_operations: usize,
    pub source_index_nodes: usize,
    pub code_search_events: usize,
    pub code_index_steps: usize,
    pub code_state_reads: usize,
    pub code_state_writes: usize,
    pub delimiter_classifications: usize,
    pub delimiter_search_entries: usize,
    pub delimiter_index_steps: usize,
    pub delimiter_stack_writes: usize,
    pub output_index_steps: usize,
    pub output_reads: usize,
    pub role_facts: usize,
    pub emit_prepares: usize,
    pub emits: usize,
    pub page_allocations: usize,
    pub allocated_bytes: usize,
    pub page_reclaims: usize,
    pub reclaimed_bytes: usize,
    pub copy_bytes: usize,
    pub hash_bytes: usize,
    pub hash_operations: usize,
}

impl InlineWork {
    /// Whether `actual` stays within every independent dimension of this
    /// permit.
    #[must_use]
    pub fn allows(self, actual: Self) -> bool {
        actual.fits(self)
    }

    fn fits(self, remaining: Self) -> bool {
        macro_rules! all {
            ($($field:ident),+ $(,)?) => {
                $(self.$field <= remaining.$field)&&+
            };
        }
        all!(
            transitions,
            lexical_root_clones,
            lexical_tree_nodes,
            lexical_pages_entered,
            lexical_events,
            lexical_decode_bytes,
            source_cursor_starts,
            source_cursor_steps,
            source_logical_bytes,
            source_descriptor_entries,
            source_excluded_bytes,
            source_seek_operations,
            source_index_nodes,
            code_search_events,
            code_index_steps,
            code_state_reads,
            code_state_writes,
            delimiter_classifications,
            delimiter_search_entries,
            delimiter_index_steps,
            delimiter_stack_writes,
            output_index_steps,
            output_reads,
            role_facts,
            emit_prepares,
            emits,
            page_allocations,
            allocated_bytes,
            page_reclaims,
            reclaimed_bytes,
            copy_bytes,
            hash_bytes,
            hash_operations,
        )
    }

    fn add_assign(&mut self, value: Self) {
        macro_rules! add {
            ($($field:ident),+ $(,)?) => {
                $(self.$field += value.$field;)+
            };
        }
        add!(
            transitions,
            lexical_root_clones,
            lexical_tree_nodes,
            lexical_pages_entered,
            lexical_events,
            lexical_decode_bytes,
            source_cursor_starts,
            source_cursor_steps,
            source_logical_bytes,
            source_descriptor_entries,
            source_excluded_bytes,
            source_seek_operations,
            source_index_nodes,
            code_search_events,
            code_index_steps,
            code_state_reads,
            code_state_writes,
            delimiter_classifications,
            delimiter_search_entries,
            delimiter_index_steps,
            delimiter_stack_writes,
            output_index_steps,
            output_reads,
            role_facts,
            emit_prepares,
            emits,
            page_allocations,
            allocated_bytes,
            page_reclaims,
            reclaimed_bytes,
            copy_bytes,
            hash_bytes,
            hash_operations,
        );
    }

    fn subtract(self, used: Self) -> Self {
        let mut remaining = self;
        macro_rules! sub {
            ($($field:ident),+ $(,)?) => {
                $(remaining.$field -= used.$field;)+
            };
        }
        sub!(
            transitions,
            lexical_root_clones,
            lexical_tree_nodes,
            lexical_pages_entered,
            lexical_events,
            lexical_decode_bytes,
            source_cursor_starts,
            source_cursor_steps,
            source_logical_bytes,
            source_descriptor_entries,
            source_excluded_bytes,
            source_seek_operations,
            source_index_nodes,
            code_search_events,
            code_index_steps,
            code_state_reads,
            code_state_writes,
            delimiter_classifications,
            delimiter_search_entries,
            delimiter_index_steps,
            delimiter_stack_writes,
            output_index_steps,
            output_reads,
            role_facts,
            emit_prepares,
            emits,
            page_allocations,
            allocated_bytes,
            page_reclaims,
            reclaimed_bytes,
            copy_bytes,
            hash_bytes,
            hash_operations,
        );
        remaining
    }

    /// A practical small-slice permit used by tests. It is intentionally not a
    /// magic scalar fuel conversion: every dimension remains independently
    /// bounded and visible in returned receipts.
    #[must_use]
    pub const fn uniform(transitions: usize) -> Self {
        Self {
            transitions,
            lexical_root_clones: transitions,
            lexical_tree_nodes: transitions,
            lexical_pages_entered: transitions,
            lexical_events: transitions,
            lexical_decode_bytes: transitions * MAX_EVENT_DECODE_BYTES,
            source_cursor_starts: transitions,
            source_cursor_steps: transitions,
            source_logical_bytes: transitions,
            source_descriptor_entries: transitions,
            source_excluded_bytes: transitions,
            source_seek_operations: transitions,
            source_index_nodes: transitions * MAX_SOURCE_INDEX_NODES_PER_STEP,
            code_search_events: transitions,
            code_index_steps: transitions * RADIX_LEVELS * 4,
            code_state_reads: transitions * 2,
            code_state_writes: transitions * 2,
            delimiter_classifications: transitions,
            delimiter_search_entries: transitions,
            delimiter_index_steps: transitions * RADIX_LEVELS * 2,
            delimiter_stack_writes: transitions,
            output_index_steps: transitions * RADIX_LEVELS * 4,
            output_reads: transitions * 2,
            role_facts: transitions,
            emit_prepares: transitions,
            emits: transitions,
            page_allocations: transitions * RADIX_LEVELS,
            allocated_bytes: usize::MAX,
            page_reclaims: transitions * RADIX_LEVELS,
            reclaimed_bytes: usize::MAX,
            copy_bytes: transitions * (PACKED_CODE_RUN_BYTES + size_of::<u32>()),
            hash_bytes: transitions * PACKED_FACT_BYTES,
            hash_operations: transitions * PACKED_FACT_BYTES,
        }
    }
}

/// Non-work observations returned beside a poll receipt.
///
/// A large persistent-source gap can advance this distance by an arbitrary
/// amount in one bounded indexed seek. It is therefore useful telemetry, but
/// treating it as caller fuel would require a bogus `usize::MAX` permit and
/// make the predictive contract self-defeating.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InlineTelemetry {
    pub source_skipped_bytes: usize,
    pub source_chunk_loads: usize,
    pub source_chunk_bytes_copied: usize,
}

impl InlineTelemetry {
    const fn delta_since(self, before: Self) -> Self {
        Self {
            source_skipped_bytes: self.source_skipped_bytes - before.source_skipped_bytes,
            source_chunk_loads: self.source_chunk_loads - before.source_chunk_loads,
            source_chunk_bytes_copied: self.source_chunk_bytes_copied
                - before.source_chunk_bytes_copied,
        }
    }
}

/// Lower-bound retained storage owned by this machine, excluding shared
/// lexical/source roots and the upstream `SourceCursor` traversal capacity
/// called out by [`REMAINING_CURSOR_ACCOUNTING_GAP`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InlineRetention {
    pub allocations: usize,
    pub bytes: usize,
    pub peak_bytes: usize,
    pub fixed_machine_bytes: usize,
    pub code_run_bytes: usize,
    pub code_span_bytes: usize,
    pub code_index_bytes: usize,
    pub delimiter_bytes: usize,
    pub temporary_overlay_bytes: usize,
    pub fact_counter_bytes: usize,
    pub output_bytes: usize,
    pub output_payload_bytes: usize,
    pub delimiter_high_water: usize,
    pub output_spans: usize,
}

/// Status after one bounded poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineStatus {
    Pending,
    Ready,
}

/// Receipt for one bounded poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlinePoll {
    pub status: InlineStatus,
    pub delta: InlineWork,
    pub telemetry_delta: InlineTelemetry,
}

/// Status of explicit bounded cancellation reclamation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineCancelStatus {
    Pending,
    Ready,
}

/// Exact work performed by one [`InlineMachine::poll_cancel`] call.
#[derive(Debug)]
pub struct InlineCancelPoll {
    pub status: InlineCancelStatus,
    pub delta: InlineWork,
    /// At most one original page allocation and its emptied directory path.
    /// Move this opaque, `Send` value to the reclaim worker; dropping it on the
    /// latency-sensitive caller defeats the ownership-transfer boundary.
    pub page: Option<InlineCancelPage>,
}

#[derive(Debug)]
enum InlineCancelAllocation {
    CodeRun(TakenRadixPage<PackedCodeRun, CODE_RUNS_PER_PAGE>),
    CodeSpan(TakenRadixPage<PackedCodeSpan, CODE_SPANS_PER_PAGE>),
    CodeIndex(TakenRadixPage<u32, MAP_ENTRIES_PER_PAGE>),
    Delimiter(TakenRadixPage<PackedInlineEl, INLINE_ELS_PER_PAGE>),
    Fact(TakenRadixPage<PackedFact, FACTS_PER_PAGE>),
    RoleHead(TakenRadixPage<PackedRoleHead, ROLE_HEADS_PER_PAGE>),
    OverflowRoleHead(TakenRadixPage<u32, MAP_ENTRIES_PER_PAGE>),
    FactCounter(TakenRadixPage<u16, FACT_PAGE_COUNTS_PER_PAGE>),
    Stream(TakenRadixPage<u8, STREAM_BYTES_PER_PAGE>),
}

impl InlineCancelAllocation {
    fn allocation_need(&self) -> AllocationNeed {
        match self {
            Self::CodeRun(page) => page.allocation_need(),
            Self::CodeSpan(page) => page.allocation_need(),
            Self::CodeIndex(page) | Self::OverflowRoleHead(page) => page.allocation_need(),
            Self::Delimiter(page) => page.allocation_need(),
            Self::Fact(page) => page.allocation_need(),
            Self::RoleHead(page) => page.allocation_need(),
            Self::FactCounter(page) => page.allocation_need(),
            Self::Stream(page) => page.allocation_need(),
        }
    }
}

/// Opaque ownership transfer produced by bounded cancellation.
///
/// It is intentionally not flattened or copied. The allocation can be queued
/// to another thread or a page-arena reclaim ticket and destroyed there.
#[derive(Debug)]
pub struct InlineCancelPage {
    allocation: InlineCancelAllocation,
}

impl InlineCancelPage {
    /// Number of original page/directory allocations transferred together.
    #[must_use]
    pub fn allocations(&self) -> usize {
        self.allocation.allocation_need().allocations
    }

    /// Exact owned bytes transferred out of the parser machine.
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.allocation.allocation_need().bytes
    }
}

/// Immutable result of the narrow inline machine.
#[derive(Debug)]
pub struct InlineOutput {
    input_identity: SegmentedLeafIdentity,
    stream: SpanStream,
    digest: u64,
    total_work: InlineWork,
    total_telemetry: InlineTelemetry,
}

impl InlineOutput {
    #[must_use]
    pub const fn input_identity(&self) -> SegmentedLeafIdentity {
        self.input_identity
    }

    #[must_use]
    pub const fn span_count(&self) -> usize {
        self.stream.span_count
    }

    #[must_use]
    pub const fn digest(&self) -> u64 {
        self.digest
    }

    #[must_use]
    pub const fn total_work(&self) -> InlineWork {
        self.total_work
    }

    #[must_use]
    pub const fn total_telemetry(&self) -> InlineTelemetry {
        self.total_telemetry
    }

    /// Decodes the permanent compact stream, which was emitted in canonical
    /// lexical source order without sorting.
    #[must_use]
    pub fn spans(&self) -> InlineSpans<'_> {
        InlineSpans {
            stream: &self.stream,
            byte_index: 0,
            previous_opener_start: 0,
        }
    }

    /// Creates a no-allocation decoder that consumes at most one encoded byte
    /// per step. Consumers needing scheduler integration should use this
    /// instead of the convenience iterator.
    #[must_use]
    pub fn projection_cursor(&self) -> InlineProjectionCursor {
        InlineProjectionCursor {
            input_identity: self.input_identity,
            byte_index: 0,
            previous_opener_start: 0,
            stage: ProjectionDecodeStage::Kind,
            kind: InlineSpanKind::Code,
            opener_start: 0,
            width: 0,
            varint_value: 0,
            varint_shift: 0,
            metrics: ProjectionMetrics::default(),
        }
    }

    /// Lower-bound retained bytes for the output page directory and pages.
    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        size_of::<Self>() + self.stream.bytes.retained_bytes
    }

    /// Exact encoded payload bytes, excluding sparse directory structure.
    #[must_use]
    pub const fn payload_bytes(&self) -> usize {
        self.stream.byte_len
    }

    /// Consumes this result and transfers its existing canonical 4 KiB page
    /// allocations in source order. No span decode, payload copy, or page
    /// allocation occurs while draining.
    #[must_use]
    pub fn into_page_drain(self) -> InlineOutputPageDrain {
        let Self {
            input_identity,
            stream,
            digest,
            total_work,
            total_telemetry,
        } = self;
        InlineOutputPageDrain {
            input_identity,
            digest,
            span_count: stream.span_count,
            payload_bytes: stream.byte_len,
            total_work,
            total_telemetry,
            pages: stream.bytes,
            next_page: 0,
            page_count: stream.byte_len.div_ceil(INLINE_OUTPUT_PAGE_BYTES),
            metrics: InlineOutputPageDrainMetrics::default(),
        }
    }
}

/// One original allocation removed from the canonical span stream.
#[derive(Debug)]
pub struct InlineOutputPage {
    allocation: Box<[u8; INLINE_OUTPUT_PAGE_BYTES]>,
    _leaf: Option<Box<RadixLeaf<u8, STREAM_BYTES_PER_PAGE>>>,
    _middle: Option<Box<RadixMiddle<u8, STREAM_BYTES_PER_PAGE>>>,
    _top: Option<Box<RadixTop<u8, STREAM_BYTES_PER_PAGE>>>,
    used_len: usize,
}

impl InlineOutputPage {
    /// Number of canonical payload bytes in this page. Only the final page can
    /// be shorter than [`INLINE_OUTPUT_PAGE_BYTES`].
    #[must_use]
    pub const fn used_len(&self) -> usize {
        self.used_len
    }

    /// Used canonical bytes, excluding zero-filled tail capacity.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.allocation[..self.used_len]
    }

    /// Transfers the exact original fixed-size allocation to another owner.
    #[must_use]
    pub fn into_allocation(self) -> Box<[u8; INLINE_OUTPUT_PAGE_BYTES]> {
        self.allocation
    }
}

/// Exact cumulative work of [`InlineOutputPageDrain`]. Page allocations are
/// transferred, while empty fixed-depth radix directory nodes are reclaimed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InlineOutputPageDrainMetrics {
    pub transitions: usize,
    pub output_index_steps: usize,
    pub page_transfers: usize,
    pub transferred_payload_bytes: usize,
    pub directory_reclaims: usize,
    pub directory_reclaimed_bytes: usize,
}

/// Consuming, zero-copy transfer cursor over canonical output pages.
#[derive(Debug)]
pub struct InlineOutputPageDrain {
    input_identity: SegmentedLeafIdentity,
    digest: u64,
    span_count: usize,
    payload_bytes: usize,
    total_work: InlineWork,
    total_telemetry: InlineTelemetry,
    pages: RadixPages<u8, STREAM_BYTES_PER_PAGE>,
    next_page: usize,
    page_count: usize,
    metrics: InlineOutputPageDrainMetrics,
}

/// One bounded page-drain transition.
#[derive(Debug)]
pub enum InlineOutputPageDrainStep {
    Page(InlineOutputPage),
    Done,
}

impl InlineOutputPageDrain {
    #[must_use]
    pub const fn input_identity(&self) -> SegmentedLeafIdentity {
        self.input_identity
    }

    #[must_use]
    pub const fn digest(&self) -> u64 {
        self.digest
    }

    #[must_use]
    pub const fn span_count(&self) -> usize {
        self.span_count
    }

    #[must_use]
    pub const fn payload_bytes(&self) -> usize {
        self.payload_bytes
    }

    #[must_use]
    pub const fn total_work(&self) -> InlineWork {
        self.total_work
    }

    #[must_use]
    pub const fn total_telemetry(&self) -> InlineTelemetry {
        self.total_telemetry
    }

    /// Transfers at most one existing page and prunes at most the three fixed
    /// radix directory levels behind it.
    ///
    /// # Panics
    ///
    /// Panics only if an internally completed span stream is missing a page.
    #[must_use]
    pub fn step(&mut self) -> InlineOutputPageDrainStep {
        self.metrics.transitions += 1;
        if self.next_page == self.page_count {
            return InlineOutputPageDrainStep::Done;
        }
        let page_index = self.next_page;
        let removed = self
            .pages
            .take_page(page_index)
            .expect("completed canonical page exists");
        let reclaim = removed.allocation_need();
        let TakenRadixPage {
            page: allocation,
            leaf,
            middle,
            top,
        } = removed;
        self.next_page += 1;
        let used_len = if self.next_page == self.page_count {
            self.payload_bytes - page_index * INLINE_OUTPUT_PAGE_BYTES
        } else {
            INLINE_OUTPUT_PAGE_BYTES
        };
        self.metrics.output_index_steps += RADIX_LEVELS;
        self.metrics.page_transfers += 1;
        self.metrics.transferred_payload_bytes += used_len;
        self.metrics.directory_reclaims += reclaim.allocations - 1;
        self.metrics.directory_reclaimed_bytes +=
            reclaim.bytes - size_of::<[u8; INLINE_OUTPUT_PAGE_BYTES]>();
        InlineOutputPageDrainStep::Page(InlineOutputPage {
            allocation,
            _leaf: leaf,
            _middle: middle,
            _top: top,
            used_len,
        })
    }

    #[must_use]
    pub const fn metrics(&self) -> InlineOutputPageDrainMetrics {
        self.metrics
    }
}

/// Sequential decoder over the immutable completed output.
pub struct InlineSpans<'a> {
    stream: &'a SpanStream,
    byte_index: usize,
    previous_opener_start: usize,
}

impl Iterator for InlineSpans<'_> {
    type Item = InlineSpan;

    fn next(&mut self) -> Option<Self::Item> {
        let kind = InlineSpanKind::from_tag(self.read_byte()?)?;
        let opener_delta = self.read_varint()? as usize;
        let width = self.read_varint()? as usize;
        let content_len = self.read_varint()? as usize;
        let opener_start = self.previous_opener_start.checked_add(opener_delta)?;
        let opener_end = opener_start.checked_add(width)?;
        let closer_start = opener_end.checked_add(content_len)?;
        let closer_end = closer_start.checked_add(width)?;
        self.previous_opener_start = opener_start;
        Some(InlineSpan {
            kind,
            opener_start,
            opener_end,
            closer_start,
            closer_end,
            content_start: opener_end,
            content_end: closer_start,
        })
    }
}

impl InlineSpans<'_> {
    fn read_byte(&mut self) -> Option<u8> {
        let byte = self.stream.get(self.byte_index)?;
        self.byte_index += 1;
        Some(byte)
    }

    fn read_varint(&mut self) -> Option<u32> {
        let mut value = 0u32;
        for shift in (0..=28).step_by(7) {
            let byte = self.read_byte()?;
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }
}

/// One bounded source-order projection transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionStep {
    Progress,
    Span(InlineSpan),
    Done,
}

/// Exact cumulative work of [`InlineProjectionCursor`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectionMetrics {
    pub transitions: usize,
    pub output_reads: usize,
    pub output_index_steps: usize,
    pub decoded_bytes: usize,
    pub spans: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectionDecodeStage {
    Kind,
    OpenerDelta,
    Width,
    ContentLen,
}

/// Fixed-storage canonical projection cursor. Each step reads at most one byte
/// from the permanent stream.
#[derive(Debug)]
pub struct InlineProjectionCursor {
    input_identity: SegmentedLeafIdentity,
    byte_index: usize,
    previous_opener_start: usize,
    stage: ProjectionDecodeStage,
    kind: InlineSpanKind,
    opener_start: usize,
    width: usize,
    varint_value: u32,
    varint_shift: u8,
    metrics: ProjectionMetrics,
}

impl InlineProjectionCursor {
    /// Advances one bounded transition against `output`.
    ///
    /// # Panics
    ///
    /// Panics if `output` is not the output that created this cursor, or if
    /// the internally produced stream is corrupt.
    #[must_use]
    pub fn step(&mut self, output: &InlineOutput) -> ProjectionStep {
        assert_eq!(
            self.input_identity, output.input_identity,
            "projection cursor must be used with its originating output"
        );
        self.metrics.transitions += 1;
        let Some(byte) = output.stream.get(self.byte_index) else {
            assert_eq!(self.stage, ProjectionDecodeStage::Kind);
            return ProjectionStep::Done;
        };
        self.byte_index += 1;
        self.metrics.output_reads += 1;
        self.metrics.output_index_steps += RADIX_LEVELS;
        self.metrics.decoded_bytes += 1;
        if self.stage == ProjectionDecodeStage::Kind {
            self.kind = InlineSpanKind::from_tag(byte).expect("valid stream kind tag");
            self.stage = ProjectionDecodeStage::OpenerDelta;
            return ProjectionStep::Progress;
        }
        self.varint_value |= u32::from(byte & 0x7f) << self.varint_shift;
        if byte & 0x80 != 0 {
            self.varint_shift += 7;
            assert!(self.varint_shift <= 28, "u32 varint exceeds five bytes");
            return ProjectionStep::Progress;
        }
        let value = std::mem::take(&mut self.varint_value) as usize;
        self.varint_shift = 0;
        match self.stage {
            ProjectionDecodeStage::Kind => unreachable!(),
            ProjectionDecodeStage::OpenerDelta => {
                self.opener_start = self
                    .previous_opener_start
                    .checked_add(value)
                    .expect("valid opener delta");
                self.stage = ProjectionDecodeStage::Width;
                ProjectionStep::Progress
            }
            ProjectionDecodeStage::Width => {
                self.width = value;
                self.stage = ProjectionDecodeStage::ContentLen;
                ProjectionStep::Progress
            }
            ProjectionDecodeStage::ContentLen => {
                let opener_end = self.opener_start + self.width;
                let closer_start = opener_end + value;
                let span = InlineSpan {
                    kind: self.kind,
                    opener_start: self.opener_start,
                    opener_end,
                    closer_start,
                    closer_end: closer_start + self.width,
                    content_start: opener_end,
                    content_end: closer_start,
                };
                self.previous_opener_start = self.opener_start;
                self.stage = ProjectionDecodeStage::Kind;
                self.metrics.spans += 1;
                ProjectionStep::Span(span)
            }
        }
    }

    #[must_use]
    pub const fn metrics(&self) -> ProjectionMetrics {
        self.metrics
    }
}

#[derive(Clone, Copy, Debug)]
struct DelimiterRun {
    start: usize,
    len: usize,
    run_length: usize,
    marker: u8,
    can_open: bool,
    can_close: bool,
    both: bool,
    consumed: usize,
    event_ordinal: usize,
}

#[derive(Clone, Copy, Debug)]
struct MatchState {
    opener: InlineEl,
    matched_count: usize,
    remaining: usize,
    open_cursor: usize,
    close_cursor: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmitResume {
    Code,
    Match,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmitStage {
    Prepare,
    PrepareOverflow,
    StoreFact,
    StoreHead,
    StoreOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    StartCode,
    ScanCode,
    StoreCodeRun,
    CodeIndexRead,
    CodeIndexWrite,
    ResolveCodeRead,
    ResolveCodeCloser,
    StoreCodeSpan,
    ReclaimCodePage,
    StartEmphasis,
    LoadCodeSpan,
    ScanEmphasisEvent,
    SeekDelimiterStart,
    ConsumeDelimiter,
    ReadDelimiterAfter,
    ClassifyDelimiter,
    BeginCloserSearch,
    SearchOpener,
    ClampLowerBounds,
    StartMatch,
    EmitMatch,
    PushResidualOpener,
    FinishDelimiter,
    PushDelimiter,
    Emit,
    StartCanonicalReplay,
    ReplayEvent,
    ReplayRole,
    ReplayOverflowRole,
    ScanReplayFactWidths,
    ReplayFact,
    ReclaimReplayFactPage,
    PrepareCanonicalSpan,
    AppendCanonicalByte,
    MaybeReclaimRolePage,
    ReclaimRolePage,
    ReclaimOverflowRolePage,
    CleanupFactCounterPage,
    CleanupCodeSpanPage,
    CleanupDelimiterPage,
    CleanupCodeRunPage,
    CleanupCodeIndexPage,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CancelPhase {
    ReleaseCursors,
    CodeRuns,
    CodeSpans,
    CodeIndex,
    Delimiters,
    Facts,
    RoleHeads,
    OverflowRoleHeads,
    FactCounters,
    Stream,
    Done,
}

impl CancelPhase {
    const fn next(self) -> Self {
        match self {
            Self::ReleaseCursors => Self::CodeRuns,
            Self::CodeRuns => Self::CodeSpans,
            Self::CodeSpans => Self::CodeIndex,
            Self::CodeIndex => Self::Delimiters,
            Self::Delimiters => Self::Facts,
            Self::Facts => Self::RoleHeads,
            Self::RoleHeads => Self::OverflowRoleHeads,
            Self::OverflowRoleHeads => Self::FactCounters,
            Self::FactCounters => Self::Stream,
            Self::Stream | Self::Done => Self::Done,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CancelState {
    phase: CancelPhase,
    page: usize,
}

/// Resumable inline resolver over one immutable shared lexical root.
#[derive(Debug)]
pub struct InlineMachine {
    input: InlineLexicalInput,
    input_identity: SegmentedLeafIdentity,
    phase: Phase,
    lexical: Option<MeteredLexicalCursor>,
    logical: Option<LogicalCursor>,
    held_logical_byte: Option<LogicalByte>,
    before_byte: Option<u8>,
    delimiter_before: Option<u8>,
    delimiter_after: Option<u8>,
    pending_event: Option<LexicalEvent>,
    pending_event_ordinal: usize,
    event_ordinal: usize,
    code_runs: RadixPages<PackedCodeRun, CODE_RUNS_PER_PAGE>,
    code_spans: RadixPages<PackedCodeSpan, CODE_SPANS_PER_PAGE>,
    code_last_by_length: RadixPages<u32, MAP_ENTRIES_PER_PAGE>,
    code_run_count: usize,
    code_index_page_limit: usize,
    code_index: usize,
    code_resolve_index: usize,
    code_reclaimed_pages: usize,
    pending_code_run: Option<CodeRun>,
    code_span_count: usize,
    code_fact_index: usize,
    active_code_span: Option<InlineSpan>,
    delimiter_run: Option<DelimiterRun>,
    delimiter_stack: RadixPages<PackedInlineEl, INLINE_ELS_PER_PAGE>,
    stack_len: usize,
    stack_high_water: usize,
    lower_bounds: [usize; 9],
    search_index: usize,
    search_lower_bound: usize,
    matched_opener: Option<InlineEl>,
    clamp_index: usize,
    match_state: Option<MatchState>,
    facts: RadixPages<PackedFact, FACTS_PER_PAGE>,
    role_heads: RadixPages<PackedRoleHead, ROLE_HEADS_PER_PAGE>,
    role_overflow_heads: RadixPages<u32, MAP_ENTRIES_PER_PAGE>,
    fact_page_consumed: RadixPages<u16, FACT_PAGE_COUNTS_PER_PAGE>,
    fact_count: usize,
    digest: u64,
    pending_span: Option<InlineSpan>,
    pending_opener_event: usize,
    pending_fact: Option<PackedFact>,
    emit_resume: EmitResume,
    emit_stage: EmitStage,
    replay_event_ordinal: usize,
    replay_event: Option<LexicalEvent>,
    replay_next_fact: Option<usize>,
    replay_head_fact: Option<usize>,
    replay_opener_cursor: usize,
    replay_width_sum: usize,
    replay_reclaim_fact_page: Option<usize>,
    replay_role_pages_reclaimed: usize,
    replay_overflow_pages_reclaimed: usize,
    replay_complete: bool,
    pending_encoded_span: Option<EncodedSpan>,
    pending_encoded_byte: usize,
    stream_previous_opener_start: usize,
    stream: SpanStream,
    cleanup_fact_counter_page: usize,
    cleanup_code_span_page: usize,
    cleanup_delimiter_page: usize,
    cleanup_code_run_page: usize,
    cleanup_code_index_page: usize,
    peak_owned_bytes: usize,
    total_work: InlineWork,
    total_telemetry: InlineTelemetry,
    result: Option<InlineOutput>,
    cancellation: Option<CancelState>,
}

impl InlineMachine {
    /// Creates a dormant machine. Cursor creation is itself a charged poll
    /// transition, so construction does not hide root traversal work.
    ///
    /// # Panics
    ///
    /// Panics if the lexical view has no segmented origin root.
    #[must_use]
    pub fn new(input: InlineLexicalInput) -> Self {
        let input_identity = input
            .view()
            .input()
            .expect("shared lexer output retains segmented input")
            .identity();
        Self {
            input,
            input_identity,
            phase: Phase::StartCode,
            lexical: None,
            logical: None,
            held_logical_byte: None,
            before_byte: None,
            delimiter_before: None,
            delimiter_after: None,
            pending_event: None,
            pending_event_ordinal: 0,
            event_ordinal: 0,
            code_runs: RadixPages::default(),
            code_spans: RadixPages::default(),
            code_last_by_length: RadixPages::default(),
            code_run_count: 0,
            code_index_page_limit: 0,
            code_index: 0,
            code_resolve_index: 0,
            code_reclaimed_pages: 0,
            pending_code_run: None,
            code_span_count: 0,
            code_fact_index: 0,
            active_code_span: None,
            delimiter_run: None,
            delimiter_stack: RadixPages::default(),
            stack_len: 0,
            stack_high_water: 0,
            lower_bounds: [0; 9],
            search_index: 0,
            search_lower_bound: 0,
            matched_opener: None,
            clamp_index: 0,
            match_state: None,
            facts: RadixPages::default(),
            role_heads: RadixPages::default(),
            role_overflow_heads: RadixPages::default(),
            fact_page_consumed: RadixPages::default(),
            fact_count: 0,
            digest: 0,
            pending_span: None,
            pending_opener_event: 0,
            pending_fact: None,
            emit_resume: EmitResume::Code,
            emit_stage: EmitStage::Prepare,
            replay_event_ordinal: 0,
            replay_event: None,
            replay_next_fact: None,
            replay_head_fact: None,
            replay_opener_cursor: 0,
            replay_width_sum: 0,
            replay_reclaim_fact_page: None,
            replay_role_pages_reclaimed: 0,
            replay_overflow_pages_reclaimed: 0,
            replay_complete: false,
            pending_encoded_span: None,
            pending_encoded_byte: 0,
            stream_previous_opener_start: 0,
            stream: SpanStream::default(),
            cleanup_fact_counter_page: 0,
            cleanup_code_span_page: 0,
            cleanup_delimiter_page: 0,
            cleanup_code_run_page: 0,
            cleanup_code_index_page: 0,
            peak_owned_bytes: 0,
            total_work: InlineWork::default(),
            total_telemetry: InlineTelemetry::default(),
            result: None,
            cancellation: None,
        }
    }

    /// Advances while every next operation fits the remaining caller permit.
    /// No dimension may exceed the permit returned in `delta`.
    ///
    /// # Panics
    ///
    /// Panics if cancellation has already begun.
    #[must_use]
    pub fn poll(&mut self, permit: InlineWork) -> InlinePoll {
        assert!(
            self.cancellation.is_none(),
            "a cancelled inline machine cannot resume parsing"
        );
        let mut delta = InlineWork::default();
        let telemetry_before = self.total_telemetry;
        while self.phase != Phase::Done {
            let remaining = permit.subtract(delta);
            let requirement = self.next_requirement();
            if !requirement.fits(remaining) {
                break;
            }
            let phase = self.phase;
            let actual = self.tick();
            debug_assert!(
                actual.fits(requirement),
                "phase={phase:?} actual={actual:?} requirement={requirement:?}"
            );
            delta.add_assign(actual);
            self.peak_owned_bytes = self.peak_owned_bytes.max(self.current_owned_bytes());
        }
        self.finish_poll(&delta, telemetry_before)
    }

    /// Advances by at most `transitions` fixed-size state-machine ticks.
    ///
    /// This is the intended latency-oriented scheduler path. It records the
    /// same exact [`InlineWork`] receipt as [`Self::poll`] after each operation,
    /// but does not construct, subtract, and compare a 33-field predictive
    /// permit on every transition. Each transition is structurally bounded to
    /// one cursor/event step, one radix lookup, or one radix-page operation;
    /// allocations, reclamations, and copies are guarded by the exported
    /// atomic caps.
    ///
    /// # Panics
    ///
    /// Panics if cancellation has already begun or if `transitions` exceeds
    /// [`MAX_INLINE_COOPERATIVE_TRANSITIONS`]. A zero budget is valid and
    /// returns an empty pending receipt.
    #[must_use]
    pub fn poll_cooperative(&mut self, transitions: usize) -> InlinePoll {
        assert!(
            self.cancellation.is_none(),
            "a cancelled inline machine cannot resume parsing"
        );
        assert!(
            transitions <= MAX_INLINE_COOPERATIVE_TRANSITIONS,
            "cooperative inline slice exceeds the static transition cap"
        );
        let mut delta = InlineWork::default();
        let telemetry_before = self.total_telemetry;
        for _ in 0..transitions {
            if self.phase == Phase::Done {
                break;
            }
            let phase = self.phase;
            let actual = self.tick();
            assert_eq!(
                actual.transitions, 1,
                "phase {phase:?} violated the atomic transition contract"
            );
            assert!(
                actual.page_allocations <= MAX_INLINE_ATOMIC_PAGE_OPERATIONS
                    && actual.page_reclaims <= MAX_INLINE_ATOMIC_PAGE_OPERATIONS,
                "phase {phase:?} exceeded the atomic radix-operation cap: {actual:?}"
            );
            assert!(
                actual.allocated_bytes <= MAX_INLINE_ATOMIC_PAGE_BYTES
                    && actual.reclaimed_bytes <= MAX_INLINE_ATOMIC_PAGE_BYTES,
                "phase {phase:?} exceeded the atomic radix-byte cap: {actual:?}"
            );
            assert!(
                actual.copy_bytes <= MAX_INLINE_ATOMIC_COPY_BYTES,
                "phase {phase:?} exceeded the atomic copy cap: {actual:?}"
            );
            delta.add_assign(actual);
            self.peak_owned_bytes = self.peak_owned_bytes.max(self.current_owned_bytes());
        }
        self.finish_poll(&delta, telemetry_before)
    }

    /// Reclaims machine-owned parser and output radix pages cooperatively.
    ///
    /// The first transition releases active cursors and transfers a completed
    /// result's stream back into the machine. Every later transition either
    /// advances one cleanup phase or removes at most one 4 KiB radix page plus
    /// its now-empty fixed-depth directory path. Once `Ready`, dropping this
    /// machine cannot recursively release parser/output page trees.
    ///
    /// This deliberately does not claim to bound destruction of the final
    /// shared lexical/source owner; see [`REMAINING_RECLAMATION_GAP`].
    ///
    /// # Panics
    ///
    /// Panics if `transitions` is zero or exceeds
    /// [`MAX_INLINE_CANCEL_TRANSITIONS`].
    #[must_use]
    pub fn poll_cancel(&mut self, transitions: usize) -> InlineCancelPoll {
        assert!(
            (1..=MAX_INLINE_CANCEL_TRANSITIONS).contains(&transitions),
            "cancellation slice must fit the static transition cap"
        );
        self.cancellation.get_or_insert(CancelState {
            phase: CancelPhase::ReleaseCursors,
            page: 0,
        });
        let mut delta = InlineWork::default();
        let mut page = None;
        for _ in 0..transitions {
            if self.cancel_phase() == CancelPhase::Done {
                break;
            }
            let phase = self.cancel_phase();
            let (actual, transferred) = self.tick_cancel();
            assert_eq!(
                actual.transitions, 1,
                "cancel phase {phase:?} violated the atomic transition contract"
            );
            assert!(
                actual.page_reclaims <= MAX_INLINE_ATOMIC_PAGE_OPERATIONS
                    && actual.reclaimed_bytes <= MAX_INLINE_ATOMIC_PAGE_BYTES,
                "cancel phase {phase:?} exceeded the atomic reclaim cap: {actual:?}"
            );
            delta.add_assign(actual);
            if transferred.is_some() {
                page = transferred;
                break;
            }
        }
        InlineCancelPoll {
            status: if self.cancel_phase() == CancelPhase::Done {
                InlineCancelStatus::Ready
            } else {
                InlineCancelStatus::Pending
            },
            delta,
            page,
        }
    }

    fn cancel_phase(&self) -> CancelPhase {
        self.cancellation
            .expect("cancellation state was initialized")
            .phase
    }

    fn tick_cancel(&mut self) -> (InlineWork, Option<InlineCancelPage>) {
        let state = self
            .cancellation
            .expect("cancellation state was initialized");
        let mut work = InlineWork {
            transitions: 1,
            ..InlineWork::default()
        };
        if state.phase == CancelPhase::ReleaseCursors {
            self.lexical = None;
            self.logical = None;
            self.held_logical_byte = None;
            if let Some(output) = self.result.take() {
                debug_assert_eq!(self.stream.bytes.retained_allocations, 0);
                let InlineOutput { stream, .. } = output;
                self.stream = stream;
            }
            self.advance_cancel_phase();
            return (work, None);
        }

        let page_limit = self.cancel_page_limit(state.phase);
        if state.page >= page_limit {
            self.advance_cancel_phase();
            if self.cancel_phase() == CancelPhase::Done {
                debug_assert_eq!(self.machine_radix_allocations(), 0);
            }
            return (work, None);
        }

        let transferred = match state.phase {
            CancelPhase::CodeRuns => {
                work.code_index_steps = RADIX_LEVELS;
                self.code_runs
                    .take_page(state.page)
                    .map(InlineCancelAllocation::CodeRun)
            }
            CancelPhase::CodeSpans => {
                work.output_index_steps = RADIX_LEVELS;
                self.code_spans
                    .take_page(state.page)
                    .map(InlineCancelAllocation::CodeSpan)
            }
            CancelPhase::CodeIndex => {
                work.code_index_steps = RADIX_LEVELS;
                self.code_last_by_length
                    .take_page(state.page)
                    .map(InlineCancelAllocation::CodeIndex)
            }
            CancelPhase::Delimiters => {
                work.delimiter_index_steps = RADIX_LEVELS;
                self.delimiter_stack
                    .take_page(state.page)
                    .map(InlineCancelAllocation::Delimiter)
            }
            CancelPhase::Facts => {
                work.output_index_steps = RADIX_LEVELS;
                self.facts
                    .take_page(state.page)
                    .map(InlineCancelAllocation::Fact)
            }
            CancelPhase::RoleHeads => {
                work.output_index_steps = RADIX_LEVELS;
                self.role_heads
                    .take_page(state.page)
                    .map(InlineCancelAllocation::RoleHead)
            }
            CancelPhase::OverflowRoleHeads => {
                work.output_index_steps = RADIX_LEVELS;
                self.role_overflow_heads
                    .take_page(state.page)
                    .map(InlineCancelAllocation::OverflowRoleHead)
            }
            CancelPhase::FactCounters => {
                work.output_index_steps = RADIX_LEVELS;
                self.fact_page_consumed
                    .take_page(state.page)
                    .map(InlineCancelAllocation::FactCounter)
            }
            CancelPhase::Stream => {
                work.output_index_steps = RADIX_LEVELS;
                self.stream
                    .bytes
                    .take_page(state.page)
                    .map(InlineCancelAllocation::Stream)
            }
            CancelPhase::ReleaseCursors | CancelPhase::Done => {
                unreachable!("non-page cancellation phase was handled")
            }
        };
        self.cancellation
            .as_mut()
            .expect("cancellation state remains live")
            .page += 1;
        let page = transferred.map(|allocation| {
            let reclaimed = allocation.allocation_need();
            work.page_reclaims = reclaimed.allocations;
            work.reclaimed_bytes = reclaimed.bytes;
            InlineCancelPage { allocation }
        });
        (work, page)
    }

    fn advance_cancel_phase(&mut self) {
        let state = self
            .cancellation
            .as_mut()
            .expect("cancellation state remains live");
        state.phase = state.phase.next();
        state.page = 0;
    }

    fn cancel_page_limit(&self, phase: CancelPhase) -> usize {
        if self.cancel_phase_allocations(phase) == 0 {
            return 0;
        }
        match phase {
            CancelPhase::CodeRuns => self.code_run_count.div_ceil(CODE_RUNS_PER_PAGE),
            CancelPhase::CodeSpans => self.code_span_count.div_ceil(CODE_SPANS_PER_PAGE),
            CancelPhase::CodeIndex => self.code_index_page_limit,
            CancelPhase::Delimiters => self.stack_high_water.div_ceil(INLINE_ELS_PER_PAGE),
            CancelPhase::Facts => self.fact_count.div_ceil(FACTS_PER_PAGE),
            CancelPhase::RoleHeads => self
                .input
                .view()
                .event_count()
                .div_ceil(ROLE_HEADS_PER_PAGE),
            CancelPhase::OverflowRoleHeads => self
                .input
                .view()
                .event_count()
                .div_ceil(MAP_ENTRIES_PER_PAGE),
            CancelPhase::FactCounters => self
                .fact_count
                .div_ceil(FACTS_PER_PAGE)
                .div_ceil(FACT_PAGE_COUNTS_PER_PAGE),
            CancelPhase::Stream => self.stream.byte_len.div_ceil(STREAM_BYTES_PER_PAGE),
            CancelPhase::ReleaseCursors | CancelPhase::Done => 0,
        }
    }

    const fn cancel_phase_allocations(&self, phase: CancelPhase) -> usize {
        match phase {
            CancelPhase::CodeRuns => self.code_runs.retained_allocations,
            CancelPhase::CodeSpans => self.code_spans.retained_allocations,
            CancelPhase::CodeIndex => self.code_last_by_length.retained_allocations,
            CancelPhase::Delimiters => self.delimiter_stack.retained_allocations,
            CancelPhase::Facts => self.facts.retained_allocations,
            CancelPhase::RoleHeads => self.role_heads.retained_allocations,
            CancelPhase::OverflowRoleHeads => self.role_overflow_heads.retained_allocations,
            CancelPhase::FactCounters => self.fact_page_consumed.retained_allocations,
            CancelPhase::Stream => self.stream.bytes.retained_allocations,
            CancelPhase::ReleaseCursors | CancelPhase::Done => 0,
        }
    }

    const fn machine_radix_allocations(&self) -> usize {
        self.code_runs.retained_allocations
            + self.code_spans.retained_allocations
            + self.code_last_by_length.retained_allocations
            + self.delimiter_stack.retained_allocations
            + self.facts.retained_allocations
            + self.role_heads.retained_allocations
            + self.role_overflow_heads.retained_allocations
            + self.fact_page_consumed.retained_allocations
            + self.stream.bytes.retained_allocations
    }

    fn finish_poll(&mut self, delta: &InlineWork, telemetry_before: InlineTelemetry) -> InlinePoll {
        self.total_work.add_assign(*delta);
        if self.phase == Phase::Done && self.result.is_none() {
            debug_assert_eq!(self.facts.retained_allocations, 0);
            debug_assert_eq!(self.role_heads.retained_allocations, 0);
            debug_assert_eq!(self.role_overflow_heads.retained_allocations, 0);
            self.result = Some(InlineOutput {
                input_identity: self.input_identity,
                stream: std::mem::take(&mut self.stream),
                digest: self.digest,
                total_work: self.total_work,
                total_telemetry: self.total_telemetry,
            });
            self.peak_owned_bytes = self.peak_owned_bytes.max(self.current_owned_bytes());
        }
        InlinePoll {
            status: if self.phase == Phase::Done {
                InlineStatus::Ready
            } else {
                InlineStatus::Pending
            },
            delta: *delta,
            telemetry_delta: self.total_telemetry.delta_since(telemetry_before),
        }
    }

    fn next_requirement(&self) -> InlineWork {
        let mut work = InlineWork {
            transitions: 1,
            ..InlineWork::default()
        };
        match self.phase {
            Phase::StartCode => work.lexical_root_clones = 1,
            Phase::ScanCode | Phase::ScanEmphasisEvent => {
                work.lexical_tree_nodes = 1;
                work.lexical_pages_entered = 1;
                work.lexical_events = 1;
                work.lexical_decode_bytes = MAX_EVENT_DECODE_BYTES;
                if self.phase == Phase::ScanCode {
                    work.code_search_events = 1;
                }
            }
            Phase::StoreCodeRun => self.code_store_requirement(&mut work, false),
            Phase::CodeIndexRead | Phase::ResolveCodeRead | Phase::ResolveCodeCloser => {
                work.code_index_steps = RADIX_LEVELS;
                work.code_state_reads = 1;
            }
            Phase::StoreCodeSpan => self.code_store_requirement(&mut work, true),
            Phase::ReclaimCodePage => {
                let need = self.code_runs.reclaim_need(self.code_reclaimed_pages);
                work.code_index_steps = RADIX_LEVELS;
                work.page_reclaims = need.allocations;
                work.reclaimed_bytes = need.bytes;
            }
            Phase::CodeIndexWrite => {
                let run = self.pending_code_run.expect("index write has run");
                let need = self.code_last_by_length.allocation_need(run.len);
                work.code_index_steps = RADIX_LEVELS * 4;
                work.code_state_reads = 1;
                work.code_state_writes = 2;
                work.page_allocations = need.allocations;
                work.allocated_bytes = need.bytes;
                work.copy_bytes = PACKED_CODE_RUN_BYTES + size_of::<u32>();
            }
            Phase::StartEmphasis => {
                work.lexical_root_clones = 1;
                work.source_cursor_starts = 1;
                work.source_index_nodes = MAX_SOURCE_INDEX_NODES_PER_STEP;
            }
            Phase::LoadCodeSpan => {
                work.output_reads = 1;
                work.output_index_steps = RADIX_LEVELS;
            }
            Phase::SeekDelimiterStart | Phase::ConsumeDelimiter | Phase::ReadDelimiterAfter => {
                if self.held_logical_byte.is_none() {
                    work.source_cursor_steps = 1;
                    work.source_logical_bytes = 1;
                    work.source_descriptor_entries = 1;
                    work.source_excluded_bytes = 1;
                    work.source_seek_operations = 1;
                    work.source_index_nodes = MAX_SOURCE_INDEX_NODES_PER_STEP;
                }
            }
            Phase::ClassifyDelimiter => work.delimiter_classifications = 1,
            Phase::SearchOpener => {
                if self.search_index > self.search_lower_bound {
                    work.delimiter_search_entries = 1;
                    work.delimiter_index_steps = RADIX_LEVELS;
                }
            }
            Phase::ClampLowerBounds => work.delimiter_index_steps = 1,
            Phase::PushResidualOpener | Phase::PushDelimiter => {
                let need = self.delimiter_stack.allocation_need(self.stack_len);
                work.delimiter_index_steps = RADIX_LEVELS * 2;
                work.delimiter_stack_writes = 1;
                work.page_allocations = need.allocations;
                work.allocated_bytes = need.bytes;
                work.copy_bytes = PACKED_INLINE_EL_BYTES;
            }
            Phase::Emit => self.emit_requirement(&mut work),
            Phase::StartCanonicalReplay
            | Phase::ReplayEvent
            | Phase::ReplayRole
            | Phase::ReplayOverflowRole
            | Phase::ScanReplayFactWidths
            | Phase::ReplayFact
            | Phase::ReclaimReplayFactPage
            | Phase::PrepareCanonicalSpan
            | Phase::AppendCanonicalByte
            | Phase::ReclaimRolePage
            | Phase::ReclaimOverflowRolePage
            | Phase::CleanupFactCounterPage
            | Phase::CleanupCodeSpanPage
            | Phase::CleanupDelimiterPage
            | Phase::CleanupCodeRunPage
            | Phase::CleanupCodeIndexPage => self.finalization_requirement(&mut work),
            Phase::BeginCloserSearch
            | Phase::StartMatch
            | Phase::EmitMatch
            | Phase::FinishDelimiter
            | Phase::MaybeReclaimRolePage
            | Phase::Done => {}
        }
        work
    }

    fn code_store_requirement(&self, work: &mut InlineWork, span: bool) {
        let need = if span {
            self.code_spans.allocation_need(self.code_span_count)
        } else {
            self.code_runs.allocation_need(self.code_run_count)
        };
        if span {
            work.output_index_steps = RADIX_LEVELS * 2;
            work.copy_bytes = PACKED_CODE_SPAN_BYTES;
        } else {
            work.code_index_steps = RADIX_LEVELS * 2;
            work.copy_bytes = PACKED_CODE_RUN_BYTES;
        }
        work.code_state_writes = 1;
        work.page_allocations = need.allocations;
        work.allocated_bytes = need.bytes;
    }

    fn finalization_requirement(&self, work: &mut InlineWork) {
        match self.phase {
            Phase::StartCanonicalReplay => work.lexical_root_clones = 1,
            Phase::ReplayEvent => {
                work.lexical_tree_nodes = 1;
                work.lexical_pages_entered = 1;
                work.lexical_events = 1;
                work.lexical_decode_bytes = MAX_EVENT_DECODE_BYTES;
            }
            Phase::ReplayRole | Phase::ReplayOverflowRole | Phase::ScanReplayFactWidths => {
                work.output_reads = 1;
                work.output_index_steps = RADIX_LEVELS;
            }
            Phase::ReplayFact => {
                let fact_index = self.replay_next_fact.expect("replay fact exists");
                let page = fact_index / FACTS_PER_PAGE;
                let need = self.fact_page_consumed.allocation_need(page);
                work.output_reads = 2;
                work.output_index_steps = RADIX_LEVELS * 4;
                work.page_allocations = need.allocations;
                work.allocated_bytes = need.bytes;
                work.copy_bytes = size_of::<u16>();
            }
            Phase::ReclaimReplayFactPage => {
                let page = self
                    .replay_reclaim_fact_page
                    .expect("completed fact page is pending reclaim");
                let need = self.facts.reclaim_need(page);
                work.output_index_steps = RADIX_LEVELS;
                work.page_reclaims = need.allocations;
                work.reclaimed_bytes = need.bytes;
            }
            Phase::PrepareCanonicalSpan => {
                work.emit_prepares = 1;
                work.copy_bytes = MAX_ENCODED_SPAN_BYTES;
            }
            Phase::AppendCanonicalByte => self.append_requirement(work),
            Phase::ReclaimRolePage
            | Phase::ReclaimOverflowRolePage
            | Phase::CleanupFactCounterPage
            | Phase::CleanupCodeSpanPage
            | Phase::CleanupDelimiterPage
            | Phase::CleanupCodeRunPage
            | Phase::CleanupCodeIndexPage => self.cleanup_requirement(work),
            _ => unreachable!("only finalization phases are delegated"),
        }
    }

    fn append_requirement(&self, work: &mut InlineWork) {
        let need = self.stream.bytes.allocation_need(self.stream.byte_len);
        work.output_index_steps = RADIX_LEVELS * 2;
        work.page_allocations = need.allocations;
        work.allocated_bytes = need.bytes;
        work.copy_bytes = 1;
        work.hash_bytes = 1;
        work.hash_operations = 1;
        if self.pending_encoded_byte + 1
            == usize::from(
                self.pending_encoded_span
                    .expect("encoded span is pending")
                    .len,
            )
        {
            work.emits = 1;
        }
    }

    fn cleanup_requirement(&self, work: &mut InlineWork) {
        let (need, output_index, delimiter_index, code_index) = match self.phase {
            Phase::ReclaimRolePage => (
                self.role_heads
                    .reclaim_need(self.replay_role_pages_reclaimed),
                RADIX_LEVELS,
                0,
                0,
            ),
            Phase::ReclaimOverflowRolePage => (
                self.role_overflow_heads
                    .reclaim_need(self.replay_overflow_pages_reclaimed),
                RADIX_LEVELS,
                0,
                0,
            ),
            Phase::CleanupFactCounterPage => (
                self.fact_page_consumed
                    .reclaim_need(self.cleanup_fact_counter_page),
                RADIX_LEVELS,
                0,
                0,
            ),
            Phase::CleanupCodeSpanPage => (
                self.code_spans.reclaim_need(self.cleanup_code_span_page),
                RADIX_LEVELS,
                0,
                0,
            ),
            Phase::CleanupDelimiterPage => (
                self.delimiter_stack
                    .reclaim_need(self.cleanup_delimiter_page),
                0,
                RADIX_LEVELS,
                0,
            ),
            Phase::CleanupCodeRunPage => (
                self.code_runs.reclaim_need(self.cleanup_code_run_page),
                0,
                0,
                RADIX_LEVELS,
            ),
            Phase::CleanupCodeIndexPage => (
                self.code_last_by_length
                    .reclaim_need(self.cleanup_code_index_page),
                0,
                0,
                RADIX_LEVELS,
            ),
            _ => unreachable!("only cleanup phases are delegated"),
        };
        work.output_index_steps = output_index;
        work.delimiter_index_steps = delimiter_index;
        work.code_index_steps = code_index;
        work.page_reclaims = need.allocations;
        work.reclaimed_bytes = need.bytes;
    }

    fn emit_requirement(&self, work: &mut InlineWork) {
        match self.emit_stage {
            EmitStage::Prepare | EmitStage::PrepareOverflow => {
                work.emit_prepares = 1;
                work.output_reads = 1;
                work.output_index_steps = RADIX_LEVELS;
                work.copy_bytes = PACKED_FACT_BYTES;
            }
            EmitStage::StoreFact => {
                let need = self.facts.allocation_need(self.fact_count);
                work.output_index_steps = RADIX_LEVELS * 2;
                work.page_allocations = need.allocations;
                work.allocated_bytes = need.bytes;
                work.copy_bytes = PACKED_FACT_BYTES;
            }
            EmitStage::StoreHead => {
                let need = self.role_heads.allocation_need(self.pending_opener_event);
                work.output_index_steps = RADIX_LEVELS * 2;
                work.role_facts = usize::from(PackedRoleHead::direct(self.fact_count).is_some());
                work.page_allocations = need.allocations;
                work.allocated_bytes = need.bytes;
                work.copy_bytes = PACKED_ROLE_HEAD_BYTES;
            }
            EmitStage::StoreOverflow => {
                let need = self
                    .role_overflow_heads
                    .allocation_need(self.pending_opener_event);
                work.output_index_steps = RADIX_LEVELS * 2;
                work.role_facts = 1;
                work.page_allocations = need.allocations;
                work.allocated_bytes = need.bytes;
                work.copy_bytes = size_of::<u32>();
            }
        }
    }

    fn tick(&mut self) -> InlineWork {
        let mut work = InlineWork {
            transitions: 1,
            ..InlineWork::default()
        };
        match self.phase {
            Phase::StartCode => self.tick_start_code(&mut work),
            Phase::ScanCode => self.tick_code_scan(&mut work),
            Phase::StoreCodeRun => self.tick_store_code_run(&mut work),
            Phase::CodeIndexRead => self.tick_code_index_read(&mut work),
            Phase::CodeIndexWrite => self.tick_code_index_write(&mut work),
            Phase::ResolveCodeRead => self.tick_resolve_code_read(&mut work),
            Phase::ResolveCodeCloser => self.tick_resolve_code_closer(&mut work),
            Phase::StoreCodeSpan => self.tick_store_code_span(&mut work),
            Phase::ReclaimCodePage => self.tick_reclaim_code_page(&mut work),
            Phase::StartEmphasis => self.tick_start_emphasis(&mut work),
            Phase::LoadCodeSpan => self.tick_load_code_span(&mut work),
            Phase::ScanEmphasisEvent => self.tick_emphasis_event(&mut work),
            Phase::SeekDelimiterStart => self.tick_seek_delimiter(&mut work),
            Phase::ConsumeDelimiter => self.tick_consume_delimiter(&mut work),
            Phase::ReadDelimiterAfter => self.tick_delimiter_after(&mut work),
            Phase::ClassifyDelimiter => {
                self.classify_delimiter();
                self.phase = if self.delimiter_run.expect("classified run").can_close {
                    Phase::BeginCloserSearch
                } else {
                    Phase::FinishDelimiter
                };
                work.delimiter_classifications = 1;
            }
            Phase::BeginCloserSearch => self.begin_closer_search(),
            Phase::SearchOpener => self.tick_search_opener(&mut work),
            Phase::ClampLowerBounds => {
                if self.clamp_index < self.lower_bounds.len() {
                    self.lower_bounds[self.clamp_index] =
                        self.lower_bounds[self.clamp_index].min(self.stack_len);
                    self.clamp_index += 1;
                    work.delimiter_index_steps = 1;
                } else {
                    self.phase = Phase::StartMatch;
                }
            }
            Phase::StartMatch => self.start_match(),
            Phase::EmitMatch => self.emit_match(),
            Phase::PushResidualOpener => {
                let state = self.match_state.expect("match state remains available");
                let resolved = self
                    .matched_opener
                    .expect("matched opener remains available");
                let residual = InlineEl {
                    count: resolved.count - state.matched_count,
                    ..resolved
                };
                self.push_stack(residual, &mut work);
                self.phase = Phase::BeginCloserSearch;
            }
            Phase::FinishDelimiter => self.finish_delimiter(),
            Phase::PushDelimiter => {
                let run = self.delimiter_run.expect("delimiter is available");
                let value = InlineEl {
                    start: run.start + run.consumed,
                    count: run.len - run.consumed,
                    run_length: run.run_length,
                    marker: run.marker,
                    both: run.both,
                    event_ordinal: run.event_ordinal,
                };
                self.push_stack(value, &mut work);
                self.delimiter_run = None;
                self.phase = Phase::ScanEmphasisEvent;
            }
            Phase::Emit => self.tick_emit(&mut work),
            Phase::StartCanonicalReplay => self.tick_start_canonical_replay(&mut work),
            Phase::ReplayEvent => self.tick_replay_event(&mut work),
            Phase::ReplayRole => self.tick_replay_role(&mut work),
            Phase::ReplayOverflowRole => self.tick_replay_overflow_role(&mut work),
            Phase::ScanReplayFactWidths => self.tick_scan_replay_fact_widths(&mut work),
            Phase::ReplayFact => self.tick_replay_fact(&mut work),
            Phase::ReclaimReplayFactPage => self.tick_reclaim_replay_fact_page(&mut work),
            Phase::PrepareCanonicalSpan => self.tick_prepare_canonical_span(&mut work),
            Phase::AppendCanonicalByte => self.tick_append_canonical_byte(&mut work),
            Phase::MaybeReclaimRolePage => self.maybe_reclaim_role_page(),
            Phase::ReclaimRolePage => self.tick_reclaim_role_page(&mut work),
            Phase::ReclaimOverflowRolePage => {
                self.tick_reclaim_overflow_role_page(&mut work);
            }
            Phase::CleanupFactCounterPage => self.tick_cleanup_fact_counter_page(&mut work),
            Phase::CleanupCodeSpanPage => self.tick_cleanup_code_span_page(&mut work),
            Phase::CleanupDelimiterPage => self.tick_cleanup_delimiter_page(&mut work),
            Phase::CleanupCodeRunPage => self.tick_cleanup_code_run_page(&mut work),
            Phase::CleanupCodeIndexPage => self.tick_cleanup_code_index_page(&mut work),
            Phase::Done => {}
        }
        work
    }

    fn tick_start_code(&mut self, work: &mut InlineWork) {
        self.lexical = Some(self.input.metered_cursor());
        self.event_ordinal = 0;
        self.phase = Phase::ScanCode;
        work.lexical_root_clones = 1;
    }

    fn tick_store_code_run(&mut self, work: &mut InlineWork) {
        let event = self.pending_event.take().expect("backtick event retained");
        let LexicalEventKind::BacktickRun { len } = event.kind else {
            unreachable!("only backtick runs are stored")
        };
        let need = self.code_runs.ensure_page(self.code_run_count);
        self.code_runs.set(
            self.code_run_count,
            PackedCodeRun::encode(event.start.offset, len, None, self.pending_event_ordinal),
        );
        self.code_run_count += 1;
        self.phase = Phase::ScanCode;
        work.code_index_steps = RADIX_LEVELS * 2;
        work.code_state_writes = 1;
        work.page_allocations = need.allocations;
        work.allocated_bytes = need.bytes;
        work.copy_bytes = PACKED_CODE_RUN_BYTES;
    }

    fn tick_code_index_read(&mut self, work: &mut InlineWork) {
        if self.code_index == 0 {
            self.code_resolve_index = 0;
            self.phase = Phase::ResolveCodeRead;
            return;
        }
        self.code_index -= 1;
        self.pending_code_run = Some(
            self.code_runs
                .get(self.code_index)
                .expect("code run ordinal exists")
                .decode(),
        );
        self.phase = Phase::CodeIndexWrite;
        work.code_index_steps = RADIX_LEVELS;
        work.code_state_reads = 1;
    }

    fn tick_code_index_write(&mut self, work: &mut InlineWork) {
        let run = self.pending_code_run.take().expect("index run retained");
        let prior = self.code_last_by_length.get(run.len).unwrap_or(0);
        let next = (prior != 0).then(|| prior as usize - 1);
        let need = self.code_last_by_length.ensure_page(run.len);
        self.code_index_page_limit = self
            .code_index_page_limit
            .max(run.len / MAP_ENTRIES_PER_PAGE + 1);
        self.code_last_by_length
            .set(run.len, as_u32(self.code_index + 1));
        self.code_runs.set(
            self.code_index,
            PackedCodeRun::encode(run.start, run.len, next, run.event_ordinal),
        );
        self.phase = Phase::CodeIndexRead;
        work.code_index_steps = RADIX_LEVELS * 4;
        work.code_state_reads = 1;
        work.code_state_writes = 2;
        work.page_allocations = need.allocations;
        work.allocated_bytes = need.bytes;
        work.copy_bytes = PACKED_CODE_RUN_BYTES + size_of::<u32>();
    }

    fn tick_resolve_code_read(&mut self, work: &mut InlineWork) {
        if self.code_reclaimed_pages < self.code_resolve_index / CODE_RUNS_PER_PAGE {
            self.phase = Phase::ReclaimCodePage;
            return;
        }
        if self.code_resolve_index == self.code_run_count {
            self.phase = Phase::StartEmphasis;
            return;
        }
        let run = self
            .code_runs
            .get(self.code_resolve_index)
            .expect("code run ordinal exists")
            .decode();
        self.pending_code_run = Some(run);
        work.code_index_steps = RADIX_LEVELS;
        work.code_state_reads = 1;
        if run.next.is_some() {
            self.phase = Phase::ResolveCodeCloser;
        } else {
            self.code_resolve_index += 1;
        }
    }

    fn tick_reclaim_code_page(&mut self, work: &mut InlineWork) {
        let reclaimed = self.code_runs.remove_page(self.code_reclaimed_pages);
        assert!(
            reclaimed.allocations > 0,
            "only fully consumed code pages are reclaimed"
        );
        self.code_reclaimed_pages += 1;
        self.phase = Phase::ResolveCodeRead;
        work.code_index_steps = RADIX_LEVELS;
        work.page_reclaims = reclaimed.allocations;
        work.reclaimed_bytes = reclaimed.bytes;
    }

    fn tick_resolve_code_closer(&mut self, work: &mut InlineWork) {
        let opener = self.pending_code_run.take().expect("code opener retained");
        let closer_index = opener.next.expect("matched code run has closer");
        let closer = self
            .code_runs
            .get(closer_index)
            .expect("code closer ordinal exists")
            .decode();
        debug_assert_eq!(opener.len, closer.len);
        self.pending_span = Some(InlineSpan {
            kind: InlineSpanKind::Code,
            opener_start: opener.start,
            opener_end: opener.start + opener.len,
            closer_start: closer.start,
            closer_end: closer.start + closer.len,
            content_start: opener.start + opener.len,
            content_end: closer.start,
        });
        self.pending_opener_event = opener.event_ordinal;
        self.code_resolve_index = closer_index + 1;
        self.emit_resume = EmitResume::Code;
        self.emit_stage = EmitStage::Prepare;
        self.phase = Phase::StoreCodeSpan;
        work.code_index_steps = RADIX_LEVELS;
        work.code_state_reads = 1;
    }

    fn tick_store_code_span(&mut self, work: &mut InlineWork) {
        let span = self.pending_span.expect("resolved code span is pending");
        let need = self.code_spans.ensure_page(self.code_span_count);
        self.code_spans
            .set(self.code_span_count, PackedCodeSpan::encode(span));
        self.code_span_count += 1;
        self.phase = Phase::Emit;
        work.output_index_steps = RADIX_LEVELS * 2;
        work.code_state_writes = 1;
        work.page_allocations = need.allocations;
        work.allocated_bytes = need.bytes;
        work.copy_bytes = PACKED_CODE_SPAN_BYTES;
    }

    fn tick_start_emphasis(&mut self, work: &mut InlineWork) {
        self.lexical = Some(self.input.metered_cursor());
        let leaf = self
            .input
            .view()
            .input()
            .expect("shared lexer retains segmented input");
        let logical = leaf.cursor();
        let source = logical.metrics();
        self.total_telemetry.source_chunk_loads += source.source_chunk_loads;
        self.total_telemetry.source_chunk_bytes_copied += source.source_chunk_bytes_copied;
        self.logical = Some(logical);
        self.event_ordinal = 0;
        self.code_fact_index = 0;
        self.active_code_span = None;
        self.phase = if self.code_span_count == 0 {
            Phase::ScanEmphasisEvent
        } else {
            Phase::LoadCodeSpan
        };
        work.lexical_root_clones = 1;
        work.source_cursor_starts = 1;
        // `LogicalCursor::new` currently does not expose setup metrics. The
        // caller reserves the architecture-wide bound; we charge no invented
        // measurement and retain this as an explicit upstream gap.
    }

    fn tick_load_code_span(&mut self, work: &mut InlineWork) {
        let span = self
            .code_spans
            .get(self.code_fact_index)
            .expect("code span ordinal exists")
            .decode();
        self.code_fact_index += 1;
        debug_assert_eq!(span.kind, InlineSpanKind::Code);
        self.active_code_span = Some(span);
        if let Some(event) = self.pending_event {
            if event.start.offset >= span.end() && self.code_fact_index < self.code_span_count {
                self.phase = Phase::LoadCodeSpan;
            } else {
                self.pending_event = None;
                self.consider_emphasis_event(event);
            }
        } else {
            self.phase = Phase::ScanEmphasisEvent;
        }
        work.output_reads = 1;
        work.output_index_steps = RADIX_LEVELS;
    }

    fn tick_code_scan(&mut self, work: &mut InlineWork) {
        let before = self
            .lexical
            .as_ref()
            .expect("code scan has cursor")
            .metrics();
        let step = self.lexical.as_mut().expect("code scan has cursor").step();
        let after = self
            .lexical
            .as_ref()
            .expect("code scan has cursor")
            .metrics();
        add_lexical_delta(work, before, after);
        match step {
            LexicalCursorStep::Progress => {}
            LexicalCursorStep::Event(event) => {
                let ordinal = self.event_ordinal;
                self.event_ordinal += 1;
                work.code_search_events = 1;
                if matches!(event.kind, LexicalEventKind::BacktickRun { .. }) {
                    self.pending_event = Some(event);
                    self.pending_event_ordinal = ordinal;
                    self.phase = Phase::StoreCodeRun;
                }
            }
            LexicalCursorStep::Done => {
                self.code_index = self.code_run_count;
                self.phase = Phase::CodeIndexRead;
            }
        }
    }

    fn tick_emphasis_event(&mut self, work: &mut InlineWork) {
        let before = self
            .lexical
            .as_ref()
            .expect("emphasis scan has cursor")
            .metrics();
        let step = self
            .lexical
            .as_mut()
            .expect("emphasis scan has cursor")
            .step();
        let after = self
            .lexical
            .as_ref()
            .expect("emphasis scan has cursor")
            .metrics();
        add_lexical_delta(work, before, after);
        match step {
            LexicalCursorStep::Progress => {}
            LexicalCursorStep::Done => {
                self.cleanup_code_span_page = 0;
                self.phase = Phase::CleanupCodeSpanPage;
            }
            LexicalCursorStep::Event(event) => {
                self.pending_event_ordinal = self.event_ordinal;
                self.event_ordinal += 1;
                self.consider_emphasis_event(event);
            }
        }
    }

    fn consider_emphasis_event(&mut self, event: LexicalEvent) {
        if let Some(code) = self.active_code_span {
            if event.start.offset >= code.end() && self.code_fact_index < self.code_span_count {
                self.pending_event = Some(event);
                self.phase = Phase::LoadCodeSpan;
                return;
            }
            if event.start.offset >= code.start() && event.start.offset < code.end() {
                self.phase = Phase::ScanEmphasisEvent;
                return;
            }
        }
        if matches!(event.kind, LexicalEventKind::EmphasisRun { .. }) {
            self.pending_event = Some(event);
            self.phase = Phase::SeekDelimiterStart;
        } else {
            self.phase = Phase::ScanEmphasisEvent;
        }
    }

    fn tick_seek_delimiter(&mut self, work: &mut InlineWork) {
        let event = self.pending_event.expect("delimiter event retained");
        let Some(byte) = self.next_logical(work) else {
            return;
        };
        if byte.logical_offset < event.start.offset {
            self.before_byte = Some(byte.byte);
            return;
        }
        assert_eq!(byte.logical_offset, event.start.offset);
        self.delimiter_before = self.before_byte;
        self.before_byte = Some(byte.byte);
        self.phase = if event.end == event.start.offset + 1 {
            Phase::ReadDelimiterAfter
        } else {
            Phase::ConsumeDelimiter
        };
    }

    fn tick_consume_delimiter(&mut self, work: &mut InlineWork) {
        let event = self.pending_event.expect("delimiter event retained");
        let Some(byte) = self.next_logical(work) else {
            return;
        };
        assert!(byte.logical_offset < event.end);
        self.before_byte = Some(byte.byte);
        if byte.logical_offset + 1 == event.end {
            self.phase = Phase::ReadDelimiterAfter;
        }
    }

    fn tick_delimiter_after(&mut self, work: &mut InlineWork) {
        match self.next_logical_step(work) {
            NextLogical::Progress => {}
            NextLogical::Done => {
                self.delimiter_after = None;
                self.phase = Phase::ClassifyDelimiter;
            }
            NextLogical::Byte(byte) => {
                self.delimiter_after = Some(byte.byte);
                self.held_logical_byte = Some(byte);
                self.phase = Phase::ClassifyDelimiter;
            }
        }
    }

    fn classify_delimiter(&mut self) {
        let event = self.pending_event.take().expect("delimiter event retained");
        let LexicalEventKind::EmphasisRun { marker, len } = event.kind else {
            unreachable!("only emphasis events are classified")
        };
        let before_ws = self.delimiter_before.is_none_or(is_ascii_whitespace);
        let after_ws = self.delimiter_after.is_none_or(is_ascii_whitespace);
        let before_punct = self.delimiter_before.is_some_and(is_ascii_punctuation);
        let after_punct = self.delimiter_after.is_some_and(is_ascii_punctuation);
        let left_flanking = !after_ws && (!after_punct || before_ws || before_punct);
        let right_flanking = !before_ws && (!before_punct || after_ws || after_punct);
        let can_open = left_flanking && (marker == b'*' || !right_flanking || before_punct);
        let can_close = right_flanking && (marker == b'*' || !left_flanking || after_punct);
        self.delimiter_run = Some(DelimiterRun {
            start: event.start.offset,
            len,
            run_length: len,
            marker,
            can_open,
            can_close,
            both: can_open && can_close,
            consumed: 0,
            event_ordinal: self.pending_event_ordinal,
        });
    }

    fn begin_closer_search(&mut self) {
        let run = self.delimiter_run.expect("closer run exists");
        if run.consumed == run.len {
            self.phase = Phase::FinishDelimiter;
            return;
        }
        let count = run.len - run.consumed;
        self.search_index = self.stack_len;
        self.search_lower_bound = self.get_lower_bound(run.marker, count, run.both);
        self.phase = Phase::SearchOpener;
    }

    fn tick_search_opener(&mut self, work: &mut InlineWork) {
        let run = self.delimiter_run.expect("closer run exists");
        let count = run.len - run.consumed;
        if self.search_index == self.search_lower_bound {
            self.set_lower_bound(run.marker, count, run.both, self.stack_len);
            self.phase = Phase::FinishDelimiter;
            return;
        }
        self.search_index -= 1;
        let opener = self
            .delimiter_stack
            .get(self.search_index)
            .expect("live delimiter stack entry exists")
            .decode();
        work.delimiter_search_entries = 1;
        work.delimiter_index_steps = RADIX_LEVELS;
        let odd_match = (run.both || opener.both)
            && (run.run_length + opener.run_length).is_multiple_of(3)
            && !run.run_length.is_multiple_of(3);
        if opener.marker == run.marker && !odd_match {
            self.stack_len = self.search_index;
            self.matched_opener = Some(opener);
            self.clamp_index = 0;
            self.phase = Phase::ClampLowerBounds;
        }
    }

    fn start_match(&mut self) {
        let opener = self.matched_opener.expect("matched opener exists");
        let run = self.delimiter_run.expect("closer run exists");
        let count = (run.len - run.consumed).min(opener.count);
        self.match_state = Some(MatchState {
            opener,
            matched_count: count,
            remaining: count,
            open_cursor: opener.start + opener.count,
            close_cursor: run.start + run.consumed,
        });
        self.phase = Phase::EmitMatch;
    }

    fn emit_match(&mut self) {
        let mut state = self.match_state.expect("match state exists");
        if state.remaining == 0 {
            let mut run = self.delimiter_run.expect("closer run exists");
            let matched = state.opener.count.min(run.len.saturating_sub(run.consumed));
            run.consumed += matched;
            self.delimiter_run = Some(run);
            self.phase = if state.opener.count > matched {
                Phase::PushResidualOpener
            } else {
                Phase::BeginCloserSearch
            };
            return;
        }
        let width = if state.remaining > 1 { 2 } else { 1 };
        let opener_start = state.open_cursor - width;
        let closer_start = state.close_cursor;
        let span = InlineSpan {
            kind: if width == 2 {
                InlineSpanKind::Strong
            } else {
                InlineSpanKind::Emphasis
            },
            opener_start,
            opener_end: state.open_cursor,
            closer_start,
            closer_end: closer_start + width,
            content_start: state.open_cursor,
            content_end: closer_start,
        };
        state.open_cursor -= width;
        state.close_cursor += width;
        state.remaining -= width;
        self.match_state = Some(state);
        self.pending_span = Some(span);
        self.pending_opener_event = state.opener.event_ordinal;
        self.emit_resume = EmitResume::Match;
        self.emit_stage = EmitStage::Prepare;
        self.phase = Phase::Emit;
    }

    fn finish_delimiter(&mut self) {
        let run = self.delimiter_run.expect("delimiter run exists");
        if run.consumed < run.len && run.can_open {
            self.phase = Phase::PushDelimiter;
        } else {
            self.delimiter_run = None;
            self.phase = Phase::ScanEmphasisEvent;
        }
    }

    fn push_stack(&mut self, value: InlineEl, work: &mut InlineWork) {
        let need = self.delimiter_stack.ensure_page(self.stack_len);
        self.delimiter_stack
            .set(self.stack_len, PackedInlineEl::encode(value));
        self.stack_len += 1;
        self.stack_high_water = self.stack_high_water.max(self.stack_len);
        work.delimiter_index_steps = RADIX_LEVELS * 2;
        work.delimiter_stack_writes = 1;
        work.page_allocations = need.allocations;
        work.allocated_bytes = need.bytes;
        work.copy_bytes = PACKED_INLINE_EL_BYTES;
    }

    fn tick_emit(&mut self, work: &mut InlineWork) {
        match self.emit_stage {
            EmitStage::Prepare => {
                let head = self
                    .role_heads
                    .get(self.pending_opener_event)
                    .unwrap_or_default()
                    .decode();
                match head {
                    RoleHead::None => self.prepare_pending_fact(None),
                    RoleHead::Direct(index) => self.prepare_pending_fact(Some(index)),
                    RoleHead::Overflow => self.emit_stage = EmitStage::PrepareOverflow,
                }
                work.emit_prepares = 1;
                work.output_reads = 1;
                work.output_index_steps = RADIX_LEVELS;
                if head != RoleHead::Overflow {
                    work.copy_bytes = PACKED_FACT_BYTES;
                }
            }
            EmitStage::PrepareOverflow => {
                let head = self
                    .role_overflow_heads
                    .get(self.pending_opener_event)
                    .expect("overflow sentinel has a head");
                assert!(head != 0, "overflow head encodes a fact ordinal");
                self.prepare_pending_fact(Some(
                    usize::try_from(head).expect("u32 fact ordinal fits usize") - 1,
                ));
                work.emit_prepares = 1;
                work.output_reads = 1;
                work.output_index_steps = RADIX_LEVELS;
                work.copy_bytes = PACKED_FACT_BYTES;
            }
            EmitStage::StoreFact => {
                let fact = self.pending_fact.expect("prepared fact");
                let need = self.facts.ensure_page(self.fact_count);
                self.facts.set(self.fact_count, fact);
                work.output_index_steps = RADIX_LEVELS * 2;
                work.page_allocations = need.allocations;
                work.allocated_bytes = need.bytes;
                work.copy_bytes = PACKED_FACT_BYTES;
                self.emit_stage = EmitStage::StoreHead;
            }
            EmitStage::StoreHead => {
                let need = self.role_heads.ensure_page(self.pending_opener_event);
                let direct = PackedRoleHead::direct(self.fact_count);
                self.role_heads.set(
                    self.pending_opener_event,
                    direct.unwrap_or_else(PackedRoleHead::overflow),
                );
                work.output_index_steps = RADIX_LEVELS * 2;
                work.page_allocations = need.allocations;
                work.allocated_bytes = need.bytes;
                work.copy_bytes = PACKED_ROLE_HEAD_BYTES;
                if direct.is_some() {
                    work.role_facts = 1;
                    self.finish_role_fact();
                } else {
                    self.emit_stage = EmitStage::StoreOverflow;
                }
            }
            EmitStage::StoreOverflow => {
                let need = self
                    .role_overflow_heads
                    .ensure_page(self.pending_opener_event);
                self.role_overflow_heads
                    .set(self.pending_opener_event, as_u32(self.fact_count + 1));
                work.output_index_steps = RADIX_LEVELS * 2;
                work.page_allocations = need.allocations;
                work.allocated_bytes = need.bytes;
                work.copy_bytes = size_of::<u32>();
                work.role_facts = 1;
                self.finish_role_fact();
            }
        }
    }

    fn prepare_pending_fact(&mut self, next: Option<usize>) {
        self.pending_fact = Some(PackedFact::encode(
            self.pending_span.expect("pending semantic span"),
            next,
        ));
        self.emit_stage = EmitStage::StoreFact;
    }

    fn finish_role_fact(&mut self) {
        self.fact_count += 1;
        self.pending_span.take().expect("pending semantic span");
        self.pending_fact = None;
        self.phase = match self.emit_resume {
            EmitResume::Code => Phase::ResolveCodeRead,
            EmitResume::Match => Phase::EmitMatch,
        };
    }

    fn tick_start_canonical_replay(&mut self, work: &mut InlineWork) {
        self.lexical = Some(self.input.metered_cursor());
        self.replay_event_ordinal = 0;
        self.replay_event = None;
        self.replay_next_fact = None;
        self.replay_head_fact = None;
        self.replay_opener_cursor = 0;
        self.replay_width_sum = 0;
        self.replay_reclaim_fact_page = None;
        self.replay_role_pages_reclaimed = 0;
        self.replay_overflow_pages_reclaimed = 0;
        self.replay_complete = false;
        self.phase = Phase::ReplayEvent;
        work.lexical_root_clones = 1;
    }

    fn tick_replay_event(&mut self, work: &mut InlineWork) {
        let before = self
            .lexical
            .as_ref()
            .expect("canonical replay has lexical cursor")
            .metrics();
        let step = self
            .lexical
            .as_mut()
            .expect("canonical replay has lexical cursor")
            .step();
        let after = self
            .lexical
            .as_ref()
            .expect("canonical replay has lexical cursor")
            .metrics();
        add_lexical_delta(work, before, after);
        match step {
            LexicalCursorStep::Progress => {}
            LexicalCursorStep::Event(event) => {
                self.replay_event = Some(event);
                self.phase = Phase::ReplayRole;
            }
            LexicalCursorStep::Done => {
                self.replay_complete = true;
                self.phase = Phase::MaybeReclaimRolePage;
            }
        }
    }

    fn tick_replay_role(&mut self, work: &mut InlineWork) {
        let head = self
            .role_heads
            .get(self.replay_event_ordinal)
            .unwrap_or_default()
            .decode();
        self.replay_event_ordinal += 1;
        match head {
            RoleHead::None => self.finish_replay_role(None),
            RoleHead::Direct(index) => self.finish_replay_role(Some(index)),
            RoleHead::Overflow => self.phase = Phase::ReplayOverflowRole,
        }
        work.output_reads = 1;
        work.output_index_steps = RADIX_LEVELS;
    }

    fn tick_replay_overflow_role(&mut self, work: &mut InlineWork) {
        let event_ordinal = self.replay_event_ordinal - 1;
        let head = self
            .role_overflow_heads
            .get(event_ordinal)
            .expect("overflow sentinel has a replay head");
        assert!(head != 0, "overflow replay head encodes a fact ordinal");
        self.finish_replay_role(Some(
            usize::try_from(head).expect("u32 fact ordinal fits usize") - 1,
        ));
        work.output_reads = 1;
        work.output_index_steps = RADIX_LEVELS;
    }

    fn finish_replay_role(&mut self, head: Option<usize>) {
        self.replay_next_fact = head;
        self.replay_head_fact = head;
        self.replay_width_sum = 0;
        self.phase = if head.is_some() {
            Phase::ScanReplayFactWidths
        } else {
            Phase::MaybeReclaimRolePage
        };
    }

    fn tick_scan_replay_fact_widths(&mut self, work: &mut InlineWork) {
        let fact_index = self
            .replay_next_fact
            .expect("fact-width scan has a fact ordinal");
        let fact = self
            .facts
            .get(fact_index)
            .and_then(PackedFact::decode)
            .expect("role head references a fact during width scan");
        let event = self.replay_event.expect("replay event remains live");
        self.replay_width_sum = self
            .replay_width_sum
            .checked_add(fact.width(event).expect("fact kind matches opener event"))
            .expect("opener width sum fits usize");
        self.replay_next_fact = fact.next;
        if fact.next.is_none() {
            self.replay_opener_cursor = event
                .end
                .checked_sub(self.replay_width_sum)
                .expect("resolved openers are a suffix of their lexical run");
            self.replay_next_fact = self.replay_head_fact;
            self.phase = Phase::ReplayFact;
        }
        work.output_reads = 1;
        work.output_index_steps = RADIX_LEVELS;
    }

    fn tick_replay_fact(&mut self, work: &mut InlineWork) {
        let fact_index = self.replay_next_fact.expect("replay fact exists");
        let event = self.replay_event.expect("replay event remains live");
        let fact = self
            .facts
            .get(fact_index)
            .and_then(PackedFact::decode)
            .expect("role head references a live fact");
        let span = fact
            .span(self.replay_opener_cursor, event)
            .expect("fact kind matches replay event");
        self.replay_opener_cursor = span.opener_end;
        self.replay_next_fact = fact.next;
        self.pending_span = Some(span);

        let fact_page = fact_index / FACTS_PER_PAGE;
        let consumed = self.fact_page_consumed.get(fact_page).unwrap_or(0);
        let need = self.fact_page_consumed.ensure_page(fact_page);
        let consumed = consumed.checked_add(1).expect("fact page count fits u16");
        self.fact_page_consumed.set(fact_page, consumed);
        let page_start = fact_page * FACTS_PER_PAGE;
        let expected = (self.fact_count - page_start).min(FACTS_PER_PAGE);
        if usize::from(consumed) == expected {
            self.replay_reclaim_fact_page = Some(fact_page);
            self.phase = Phase::ReclaimReplayFactPage;
        } else {
            self.phase = Phase::PrepareCanonicalSpan;
        }
        work.output_reads = 2;
        work.output_index_steps = RADIX_LEVELS * 4;
        work.page_allocations = need.allocations;
        work.allocated_bytes = need.bytes;
        work.copy_bytes = size_of::<u16>();
    }

    fn tick_reclaim_replay_fact_page(&mut self, work: &mut InlineWork) {
        let page = self
            .replay_reclaim_fact_page
            .take()
            .expect("completed fact page is pending reclaim");
        let reclaimed = self.facts.remove_page(page);
        assert!(reclaimed.allocations > 0, "completed fact page is live");
        self.phase = Phase::PrepareCanonicalSpan;
        work.output_index_steps = RADIX_LEVELS;
        work.page_reclaims = reclaimed.allocations;
        work.reclaimed_bytes = reclaimed.bytes;
    }

    fn tick_prepare_canonical_span(&mut self, work: &mut InlineWork) {
        let encoded = EncodedSpan::new(
            self.pending_span.expect("replay span is pending"),
            self.stream_previous_opener_start,
        );
        self.pending_encoded_span = Some(encoded);
        self.pending_encoded_byte = 0;
        self.phase = Phase::AppendCanonicalByte;
        work.emit_prepares = 1;
        work.copy_bytes = usize::from(encoded.len);
    }

    fn tick_append_canonical_byte(&mut self, work: &mut InlineWork) {
        let encoded = self
            .pending_encoded_span
            .expect("canonical bytes are pending");
        let byte = encoded.bytes[self.pending_encoded_byte];
        let need = self.stream.bytes.ensure_page(self.stream.byte_len);
        self.stream.bytes.set(self.stream.byte_len, byte);
        self.stream.byte_len += 1;
        self.digest = self
            .digest
            .wrapping_mul(HASH_BASE)
            .wrapping_add(u64::from(byte) + 1);
        self.pending_encoded_byte += 1;
        work.output_index_steps = RADIX_LEVELS * 2;
        work.page_allocations = need.allocations;
        work.allocated_bytes = need.bytes;
        work.copy_bytes = 1;
        work.hash_bytes = 1;
        work.hash_operations = 1;
        if self.pending_encoded_byte == usize::from(encoded.len) {
            let span = self.pending_span.take().expect("encoded span remains live");
            self.stream_previous_opener_start = span.opener_start;
            self.stream.span_count += 1;
            self.pending_encoded_span = None;
            self.pending_encoded_byte = 0;
            work.emits = 1;
            self.phase = if self.replay_next_fact.is_some() {
                Phase::ReplayFact
            } else {
                Phase::MaybeReclaimRolePage
            };
        }
    }

    fn maybe_reclaim_role_page(&mut self) {
        let completed_role_pages = if self.replay_complete {
            self.replay_event_ordinal.div_ceil(ROLE_HEADS_PER_PAGE)
        } else {
            self.replay_event_ordinal / ROLE_HEADS_PER_PAGE
        };
        let completed_overflow_pages = if self.replay_complete {
            self.replay_event_ordinal.div_ceil(MAP_ENTRIES_PER_PAGE)
        } else {
            self.replay_event_ordinal / MAP_ENTRIES_PER_PAGE
        };
        if self.replay_role_pages_reclaimed < completed_role_pages {
            self.phase = Phase::ReclaimRolePage;
        } else if self.role_overflow_heads.retained_allocations > 0
            && self.replay_overflow_pages_reclaimed < completed_overflow_pages
        {
            self.phase = Phase::ReclaimOverflowRolePage;
        } else if self.replay_complete {
            debug_assert_eq!(self.facts.retained_allocations, 0);
            debug_assert_eq!(self.role_heads.retained_allocations, 0);
            debug_assert_eq!(self.role_overflow_heads.retained_allocations, 0);
            self.phase = Phase::CleanupFactCounterPage;
        } else {
            self.phase = Phase::ReplayEvent;
        }
    }

    fn tick_reclaim_role_page(&mut self, work: &mut InlineWork) {
        let reclaimed = self
            .role_heads
            .remove_page(self.replay_role_pages_reclaimed);
        self.replay_role_pages_reclaimed += 1;
        self.phase = Phase::MaybeReclaimRolePage;
        work.output_index_steps = RADIX_LEVELS;
        work.page_reclaims = reclaimed.allocations;
        work.reclaimed_bytes = reclaimed.bytes;
    }

    fn tick_reclaim_overflow_role_page(&mut self, work: &mut InlineWork) {
        let reclaimed = self
            .role_overflow_heads
            .remove_page(self.replay_overflow_pages_reclaimed);
        self.replay_overflow_pages_reclaimed += 1;
        self.phase = Phase::MaybeReclaimRolePage;
        work.output_index_steps = RADIX_LEVELS;
        work.page_reclaims = reclaimed.allocations;
        work.reclaimed_bytes = reclaimed.bytes;
    }

    fn tick_cleanup_fact_counter_page(&mut self, work: &mut InlineWork) {
        let fact_pages = self.fact_count.div_ceil(FACTS_PER_PAGE);
        let page_limit = fact_pages.div_ceil(FACT_PAGE_COUNTS_PER_PAGE);
        if self.cleanup_fact_counter_page == page_limit {
            self.phase = Phase::CleanupDelimiterPage;
            return;
        }
        let reclaimed = self
            .fact_page_consumed
            .remove_page(self.cleanup_fact_counter_page);
        self.cleanup_fact_counter_page += 1;
        work.output_index_steps = RADIX_LEVELS;
        work.page_reclaims = reclaimed.allocations;
        work.reclaimed_bytes = reclaimed.bytes;
    }

    fn tick_cleanup_code_span_page(&mut self, work: &mut InlineWork) {
        let page_limit = self.code_span_count.div_ceil(CODE_SPANS_PER_PAGE);
        if self.cleanup_code_span_page == page_limit {
            self.phase = Phase::StartCanonicalReplay;
            return;
        }
        let reclaimed = self.code_spans.remove_page(self.cleanup_code_span_page);
        self.cleanup_code_span_page += 1;
        work.output_index_steps = RADIX_LEVELS;
        work.page_reclaims = reclaimed.allocations;
        work.reclaimed_bytes = reclaimed.bytes;
    }

    fn tick_cleanup_delimiter_page(&mut self, work: &mut InlineWork) {
        let page_limit = self.stack_high_water.div_ceil(INLINE_ELS_PER_PAGE);
        if self.cleanup_delimiter_page == page_limit {
            self.cleanup_code_run_page = self.code_reclaimed_pages;
            self.phase = Phase::CleanupCodeRunPage;
            return;
        }
        let reclaimed = self
            .delimiter_stack
            .remove_page(self.cleanup_delimiter_page);
        self.cleanup_delimiter_page += 1;
        work.delimiter_index_steps = RADIX_LEVELS;
        work.page_reclaims = reclaimed.allocations;
        work.reclaimed_bytes = reclaimed.bytes;
    }

    fn tick_cleanup_code_run_page(&mut self, work: &mut InlineWork) {
        let page_limit = self.code_run_count.div_ceil(CODE_RUNS_PER_PAGE);
        if self.cleanup_code_run_page == page_limit {
            self.phase = Phase::CleanupCodeIndexPage;
            return;
        }
        let reclaimed = self.code_runs.remove_page(self.cleanup_code_run_page);
        self.cleanup_code_run_page += 1;
        work.code_index_steps = RADIX_LEVELS;
        work.page_reclaims = reclaimed.allocations;
        work.reclaimed_bytes = reclaimed.bytes;
    }

    fn tick_cleanup_code_index_page(&mut self, work: &mut InlineWork) {
        if self.cleanup_code_index_page == self.code_index_page_limit {
            self.phase = Phase::Done;
            return;
        }
        let reclaimed = self
            .code_last_by_length
            .remove_page(self.cleanup_code_index_page);
        self.cleanup_code_index_page += 1;
        work.code_index_steps = RADIX_LEVELS;
        work.page_reclaims = reclaimed.allocations;
        work.reclaimed_bytes = reclaimed.bytes;
    }

    fn next_logical(&mut self, work: &mut InlineWork) -> Option<LogicalByte> {
        match self.next_logical_step(work) {
            NextLogical::Byte(byte) => Some(byte),
            NextLogical::Progress => None,
            NextLogical::Done => panic!("validated delimiter offset exists in logical input"),
        }
    }

    fn next_logical_step(&mut self, work: &mut InlineWork) -> NextLogical {
        if let Some(byte) = self.held_logical_byte.take() {
            return NextLogical::Byte(byte);
        }
        let before = self
            .logical
            .as_ref()
            .expect("emphasis scan has logical cursor")
            .metrics();
        let step = self
            .logical
            .as_mut()
            .expect("emphasis scan has logical cursor")
            .step();
        let after = self
            .logical
            .as_ref()
            .expect("emphasis scan has logical cursor")
            .metrics();
        self.total_telemetry.source_skipped_bytes +=
            after.skipped_source_bytes - before.skipped_source_bytes;
        self.total_telemetry.source_chunk_loads +=
            after.source_chunk_loads - before.source_chunk_loads;
        self.total_telemetry.source_chunk_bytes_copied +=
            after.source_chunk_bytes_copied - before.source_chunk_bytes_copied;
        add_cursor_delta(work, before, after);
        match step {
            CursorStep::Progress => NextLogical::Progress,
            CursorStep::Byte(byte) => NextLogical::Byte(byte),
            CursorStep::Done => NextLogical::Done,
        }
    }

    fn get_lower_bound(&self, marker: u8, count: usize, both: bool) -> usize {
        let bound = if marker == b'_' {
            let mod_three = self.lower_bounds[6 + count % 3];
            if both {
                mod_three
            } else {
                mod_three.min(self.lower_bounds[0])
            }
        } else {
            let mod_three = self.lower_bounds[2 + count % 3];
            if both {
                mod_three
            } else {
                mod_three.min(self.lower_bounds[1])
            }
        };
        bound.min(self.stack_len)
    }

    fn set_lower_bound(&mut self, marker: u8, count: usize, both: bool, bound: usize) {
        if marker == b'_' {
            if both {
                self.lower_bounds[6 + count % 3] = bound;
            } else {
                self.lower_bounds[0] = bound;
            }
        } else {
            self.lower_bounds[2 + count % 3] = bound;
            if !both {
                self.lower_bounds[1] = bound;
            }
        }
    }

    /// Returns the completed immutable result without flattening its packed
    /// pages.
    #[must_use]
    pub fn output(&self) -> Option<&InlineOutput> {
        self.result.as_ref()
    }

    /// Moves the completed compact output out of the resolver. The output has
    /// no shared lexical/source owner, so callers may then drop the machine
    /// without invalidating stream iteration.
    #[must_use]
    pub fn take_output(&mut self) -> Option<InlineOutput> {
        self.result.take()
    }

    /// Cumulative measured work performed so far.
    #[must_use]
    pub const fn total_work(&self) -> InlineWork {
        self.total_work
    }

    /// Cumulative semantic-distance telemetry, excluded from caller fuel.
    #[must_use]
    pub const fn total_telemetry(&self) -> InlineTelemetry {
        self.total_telemetry
    }

    /// Lower-bound owned memory receipt. Shared source and lexical roots are
    /// deliberately excluded and can be measured through their own APIs.
    #[must_use]
    pub fn retention(&self) -> InlineRetention {
        let (output_allocations, output_bytes, output_payload_bytes, output_spans) =
            if let Some(output) = &self.result {
                (
                    output.stream.bytes.retained_allocations,
                    output.stream.bytes.retained_bytes,
                    output.stream.byte_len,
                    output.stream.span_count,
                )
            } else {
                (
                    self.stream.bytes.retained_allocations,
                    self.stream.bytes.retained_bytes,
                    self.stream.byte_len,
                    self.stream.span_count,
                )
            };
        let temporary_overlay_bytes = self.facts.retained_bytes
            + self.role_heads.retained_bytes
            + self.role_overflow_heads.retained_bytes;
        InlineRetention {
            allocations: self.delimiter_stack.retained_allocations
                + self.code_runs.retained_allocations
                + self.code_spans.retained_allocations
                + self.code_last_by_length.retained_allocations
                + self.facts.retained_allocations
                + self.role_heads.retained_allocations
                + self.role_overflow_heads.retained_allocations
                + self.fact_page_consumed.retained_allocations
                + output_allocations,
            bytes: self.current_owned_bytes(),
            peak_bytes: self.peak_owned_bytes.max(self.current_owned_bytes()),
            fixed_machine_bytes: size_of::<Self>(),
            code_run_bytes: self.code_runs.retained_bytes,
            code_span_bytes: self.code_spans.retained_bytes,
            code_index_bytes: self.code_last_by_length.retained_bytes,
            delimiter_bytes: self.delimiter_stack.retained_bytes,
            temporary_overlay_bytes,
            fact_counter_bytes: self.fact_page_consumed.retained_bytes,
            output_bytes,
            output_payload_bytes,
            delimiter_high_water: self.stack_high_water,
            output_spans,
        }
    }

    const fn current_owned_bytes(&self) -> usize {
        let output_bytes = if let Some(output) = &self.result {
            output.stream.bytes.retained_bytes
        } else {
            self.stream.bytes.retained_bytes
        };
        size_of::<Self>()
            + self.delimiter_stack.retained_bytes
            + self.code_runs.retained_bytes
            + self.code_spans.retained_bytes
            + self.code_last_by_length.retained_bytes
            + self.facts.retained_bytes
            + self.role_heads.retained_bytes
            + self.role_overflow_heads.retained_bytes
            + self.fact_page_consumed.retained_bytes
            + output_bytes
    }

    /// Number of compact code-run records indexed by the linear donor seam.
    #[must_use]
    pub const fn code_run_count(&self) -> usize {
        self.code_run_count
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NextLogical {
    Progress,
    Byte(LogicalByte),
    Done,
}

fn add_lexical_delta(
    work: &mut InlineWork,
    before: LexicalCursorMetrics,
    after: LexicalCursorMetrics,
) {
    work.lexical_tree_nodes += after.tree_nodes - before.tree_nodes;
    work.lexical_pages_entered += after.pages_entered - before.pages_entered;
    work.lexical_events += after.events - before.events;
    work.lexical_decode_bytes += after.decoded_bytes - before.decoded_bytes;
}

fn add_cursor_delta(work: &mut InlineWork, before: CursorMetrics, after: CursorMetrics) {
    work.source_cursor_steps += after.operations - before.operations;
    work.source_logical_bytes += after.logical_bytes - before.logical_bytes;
    work.source_descriptor_entries += after.descriptor_entries - before.descriptor_entries;
    work.source_excluded_bytes += after.excluded_source_bytes - before.excluded_source_bytes;
    work.source_seek_operations += after.source_seek_operations - before.source_seek_operations;
    work.source_index_nodes += after.source_seek_index_nodes - before.source_seek_index_nodes;
}

const fn is_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

const fn is_ascii_punctuation(byte: u8) -> bool {
    byte.is_ascii_punctuation()
}

#[cfg(test)]
mod tests {
    use super::{PackedRoleHead, RoleHead, ROLE_HEAD_OVERFLOW_SENTINEL};

    #[test]
    fn packed_role_head_boundary_routes_large_fact_ordinals_to_overflow() {
        let largest_direct = ROLE_HEAD_OVERFLOW_SENTINEL as usize - 2;
        assert_eq!(
            PackedRoleHead::direct(largest_direct).map(PackedRoleHead::decode),
            Some(RoleHead::Direct(largest_direct))
        );
        assert!(PackedRoleHead::direct(largest_direct + 1).is_none());
        assert_eq!(PackedRoleHead::overflow().decode(), RoleHead::Overflow);
        assert_eq!(PackedRoleHead::default().decode(), RoleHead::None);
    }
}

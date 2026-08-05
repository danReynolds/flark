//! Representation/checkpoint spike for a Flark-owned inline service.
//!
//! This deliberately implements only a toy pairing grammar. Its purpose is to
//! make memory representation, fuel, cancellation, page sealing, and immutable
//! suffix reuse executable without smuggling in a donor AST.

use std::mem::size_of;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const LEX_SHORT_DELTA: u64 = 31;
const FACT_PAGE_EVENTS: u32 = 4096;
const BYTE_CHUNK_BYTES: usize = 4096;
const MAX_PROTOTYPE_SOURCE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ENCODED_BYTES_PER_SOURCE_BYTE: usize = 10;

pub const PRODUCTION_GAPS: &[&str] = &[
    "toy delimiter pairing is not CommonMark/GFM inline semantics",
    "candidate parsing is still clean-from-start; reused pages are adopted only after convergence",
    "state fingerprints are non-cryptographic and need collision-safe confirmation",
    "reference labels, code spans, escapes, entities, autolinks, HTML, and table-cell rules are absent",
    "source segments are an Arc-backed stand-in, not the production rope/piece tree",
    "the allocator accounting is conservative bookkeeping, not a replacement for process RSS",
    "the spike has an explicit 16 MiB source ceiling; production needs paged indexes without a fixed ceiling",
    "checkpoints retain packed open-stack roots, but restart, prefix sharing, and early suffix attachment remain unproved",
];

#[derive(Debug)]
struct ChunkedBytes {
    chunks: Vec<Box<[u8; BYTE_CHUNK_BYTES]>>,
    len: usize,
    digest: u64,
}

impl ChunkedBytes {
    fn with_max_payload(max_payload: usize) -> Self {
        let max_chunks = max_payload.div_ceil(BYTE_CHUNK_BYTES).max(1);
        Self {
            chunks: Vec::with_capacity(max_chunks),
            len: 0,
            digest: FNV_OFFSET,
        }
    }

    fn push(&mut self, byte: u8) {
        let chunk = self.len / BYTE_CHUNK_BYTES;
        let offset = self.len % BYTE_CHUNK_BYTES;
        if offset == 0 {
            assert!(
                self.chunks.len() < self.chunks.capacity(),
                "prototype packed-byte ceiling exceeded"
            );
            self.chunks.push(Box::new([0; BYTE_CHUNK_BYTES]));
        }
        self.chunks[chunk][offset] = byte;
        self.len += 1;
        self.digest ^= u64::from(byte);
        self.digest = self.digest.wrapping_mul(FNV_PRIME);
    }

    fn get(&self, index: usize) -> u8 {
        assert!(index < self.len);
        self.chunks[index / BYTE_CHUNK_BYTES][index % BYTE_CHUNK_BYTES]
    }

    fn len(&self) -> usize {
        self.len
    }

    fn truncate(&mut self, len: usize) {
        assert!(len <= self.len);
        self.len = len;
        self.chunks.truncate(len.div_ceil(BYTE_CHUNK_BYTES));
        // Stack tapes are the only callers of truncate and do not consume the
        // append-only digest. Page/tape digests remain incrementally exact.
    }

    fn payload_eq(&self, other: &Self) -> bool {
        self.len == other.len && (0..self.len).all(|index| self.get(index) == other.get(index))
    }

    fn allocated_bytes(&self) -> usize {
        self.chunks.len() * BYTE_CHUNK_BYTES
            + self.chunks.capacity() * size_of::<Box<[u8; BYTE_CHUNK_BYTES]>>()
    }
}

#[derive(Clone, Debug)]
pub struct SegmentedSource {
    segments: Arc<[Arc<str>]>,
    starts: Arc<[u64]>,
    len: u64,
}

impl SegmentedSource {
    pub fn from_text_chunked(text: &str, target_bytes: usize) -> Self {
        assert!(target_bytes > 0);
        let mut segments = Vec::new();
        let mut start = 0;
        while start < text.len() {
            let mut end = (start + target_bytes).min(text.len());
            while end > start && !text.is_char_boundary(end) {
                end -= 1;
            }
            if end == start {
                end = text[start..]
                    .char_indices()
                    .nth(1)
                    .map_or(text.len(), |(offset, _)| start + offset);
            }
            segments.push(text[start..end].to_owned());
            start = end;
        }
        Self::from_owned_segments(segments)
    }

    pub fn from_owned_segments(segments: Vec<String>) -> Self {
        Self::from_arcs(
            segments
                .into_iter()
                .filter(|segment| !segment.is_empty())
                .map(Arc::<str>::from)
                .collect(),
        )
    }

    fn from_arcs(segments: Vec<Arc<str>>) -> Self {
        let mut starts = Vec::with_capacity(segments.len());
        let mut len = 0u64;
        for segment in &segments {
            starts.push(len);
            len += segment.len() as u64;
        }
        Self {
            segments: segments.into(),
            starts: starts.into(),
            len,
        }
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    pub fn edit(&self, range: Range<u64>, insert: &str) -> Self {
        assert!(range.start <= range.end && range.end <= self.len);
        assert!(self.is_char_boundary(range.start));
        assert!(self.is_char_boundary(range.end));

        let mut output = Vec::with_capacity(self.segments.len() + 3);
        let mut inserted = false;
        for (index, segment) in self.segments.iter().enumerate() {
            let base = self.starts[index];
            let end = base + segment.len() as u64;
            if end <= range.start {
                output.push(Arc::clone(segment));
                continue;
            }
            if base >= range.end {
                if !inserted && !insert.is_empty() {
                    output.push(Arc::<str>::from(insert));
                    inserted = true;
                }
                output.push(Arc::clone(segment));
                continue;
            }
            if range.start > base {
                output.push(Arc::<str>::from(&segment[..(range.start - base) as usize]));
            }
            if !inserted && !insert.is_empty() {
                output.push(Arc::<str>::from(insert));
                inserted = true;
            }
            if range.end < end {
                output.push(Arc::<str>::from(&segment[(range.end - base) as usize..]));
            }
        }

        // The overlap loop cannot place an insertion at EOF or into an empty
        // source. Handle that case explicitly without flattening the source.
        if !inserted && !insert.is_empty() {
            output.push(Arc::<str>::from(insert));
        }
        Self::from_arcs(output)
    }

    pub fn shared_segment_count(&self, other: &Self) -> usize {
        self.segments
            .iter()
            .filter(|left| other.segments.iter().any(|right| Arc::ptr_eq(left, right)))
            .count()
    }

    fn is_char_boundary(&self, position: u64) -> bool {
        if position == self.len {
            return true;
        }
        for (index, start) in self.starts.iter().enumerate() {
            let end = *start + self.segments[index].len() as u64;
            if position >= *start && position < end {
                return self.segments[index].is_char_boundary((position - *start) as usize);
            }
        }
        false
    }

    fn cursor(&self) -> SegmentedCursor {
        SegmentedCursor {
            source: self.clone(),
            segment: 0,
            offset: 0,
            absolute: 0,
        }
    }

    fn segment(&self, index: usize) -> &str {
        &self.segments[index]
    }

    fn segment_start(&self, index: usize) -> u64 {
        self.starts[index]
    }

    fn metadata_bytes(&self) -> usize {
        self.segments.len() * (size_of::<Arc<str>>() + size_of::<u64>())
    }

    fn max_segment_len(&self) -> usize {
        self.segments
            .iter()
            .map(|segment| segment.len())
            .max()
            .unwrap_or(0)
    }

    fn conservative_retained_bytes(&self) -> usize {
        self.len as usize + self.metadata_bytes() + self.segments.len() * 2 * size_of::<usize>()
    }
}

#[derive(Clone, Debug)]
struct SegmentedCursor {
    source: SegmentedSource,
    segment: usize,
    offset: usize,
    absolute: u64,
}

#[derive(Clone, Copy, Debug)]
struct CursorByte {
    byte: u8,
    absolute: u64,
    segment: usize,
    ends_segment: bool,
}

impl SegmentedCursor {
    fn next_byte(&mut self) -> Option<CursorByte> {
        while self.segment < self.source.segment_count() {
            let bytes = self.source.segment(self.segment).as_bytes();
            if self.offset == bytes.len() {
                self.segment += 1;
                self.offset = 0;
                continue;
            }
            let result = CursorByte {
                byte: bytes[self.offset],
                absolute: self.absolute,
                segment: self.segment,
                ends_segment: self.offset + 1 == bytes.len(),
            };
            self.offset += 1;
            self.absolute += 1;
            return Some(result);
        }
        None
    }

    fn peek_byte(&self) -> Option<u8> {
        let mut segment = self.segment;
        let mut offset = self.offset;
        while segment < self.source.segment_count() {
            let bytes = self.source.segment(segment).as_bytes();
            if offset < bytes.len() {
                return Some(bytes[offset]);
            }
            segment += 1;
            offset = 0;
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum LexKind {
    Star = 0,
    Underscore = 1,
    OpenBracket = 2,
    CloseBracket = 3,
    StarRun = 4,
    UnderscoreRun = 5,
}

impl LexKind {
    fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Star,
            1 => Self::Underscore,
            2 => Self::OpenBracket,
            3 => Self::CloseBracket,
            4 => Self::StarRun,
            5 => Self::UnderscoreRun,
            _ => unreachable!("invalid lexical record kind"),
        }
    }
}

#[derive(Debug)]
struct LexPage {
    source_len: u64,
    source_digest: u64,
    bytes: ChunkedBytes,
    events: u32,
    scan_in: u64,
    scan_out: u64,
    digest: u64,
}

#[derive(Clone, Debug)]
struct LexPageRef {
    base: u64,
    page: Arc<LexPage>,
}

#[derive(Debug)]
struct LexBuilder {
    base: u64,
    source_len: u64,
    source_digest: u64,
    bytes: ChunkedBytes,
    events: u32,
    previous_position: Option<u64>,
}

impl LexBuilder {
    fn new(base: u64, source: &str, max_payload: usize) -> Self {
        let mut builder = Self {
            base,
            source_len: 0,
            source_digest: FNV_OFFSET,
            // A page can contain one cross-segment run plus at most one dense
            // source segment. The fixed chunk-pointer index prevents a flat
            // Vec growth copy; payload chunks remain lazy and fixed-size.
            bytes: ChunkedBytes::with_max_payload(max_payload),
            events: 0,
            previous_position: None,
        };
        builder.extend_source(source);
        builder
    }

    fn extend_source(&mut self, source: &str) {
        self.source_len += source.len() as u64;
    }

    fn observe_source_byte(&mut self, byte: u8) {
        self.source_digest ^= u64::from(byte);
        self.source_digest = self.source_digest.wrapping_mul(FNV_PRIME);
    }

    fn push(&mut self, kind: LexKind, position: u64, run_len: u64) {
        let previous = self.previous_position.unwrap_or(self.base);
        let delta = position - previous;
        let short = delta.min(LEX_SHORT_DELTA) as u8;
        self.bytes.push(((kind as u8) << 5) | short);
        if delta >= LEX_SHORT_DELTA {
            push_varint(&mut self.bytes, delta - LEX_SHORT_DELTA);
        }
        if matches!(kind, LexKind::StarRun | LexKind::UnderscoreRun) {
            debug_assert!(run_len >= 2);
            push_varint(&mut self.bytes, run_len - 2);
        }
        self.events += 1;
        self.previous_position = Some(position);
    }

    fn seal(self) -> LexPageRef {
        let digest = mix(self.bytes.digest, self.source_digest);
        LexPageRef {
            base: self.base,
            page: Arc::new(LexPage {
                source_len: self.source_len,
                source_digest: self.source_digest,
                bytes: self.bytes,
                events: self.events,
                scan_in: 0,
                scan_out: 0,
                digest,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Run {
    byte: u8,
    start: u64,
    len: u64,
}

#[derive(Clone, Copy, Debug)]
struct LexEvent {
    kind: LexKind,
    position: u64,
    run_len: u64,
    ordinal: u64,
}

#[derive(Clone, Copy, Debug)]
enum DecodeAction {
    BeginPage(usize),
    Event(LexEvent),
    EndPage(usize),
    Done,
}

#[derive(Debug, Default)]
struct LexDecoder {
    page: usize,
    offset: usize,
    previous_position: u64,
    ordinal: u64,
    begun: bool,
}

impl LexDecoder {
    fn next(&mut self, pages: &[LexPageRef]) -> DecodeAction {
        if self.page == pages.len() {
            return DecodeAction::Done;
        }
        let page = &pages[self.page];
        if !self.begun {
            self.begun = true;
            self.previous_position = page.base;
            return DecodeAction::BeginPage(self.page);
        }
        if self.offset == page.page.bytes.len() {
            let completed = self.page;
            self.page += 1;
            self.offset = 0;
            self.begun = false;
            return DecodeAction::EndPage(completed);
        }

        let header = page.page.bytes.get(self.offset);
        self.offset += 1;
        let kind = LexKind::from_code(header >> 5);
        let mut delta = u64::from(header & 0x1f);
        if delta == LEX_SHORT_DELTA {
            delta += read_varint(&page.page.bytes, &mut self.offset);
        }
        let run_len = if matches!(kind, LexKind::StarRun | LexKind::UnderscoreRun) {
            read_varint(&page.page.bytes, &mut self.offset) + 2
        } else {
            1
        };
        let position = self.previous_position + delta;
        let event = LexEvent {
            kind,
            position,
            run_len,
            ordinal: self.ordinal,
        };
        self.previous_position = position;
        self.ordinal += 1;
        DecodeAction::Event(event)
    }
}

#[derive(Debug)]
struct ReverseVarintStack {
    bytes: ChunkedBytes,
    len: usize,
    top: Option<u64>,
}

impl ReverseVarintStack {
    fn new(max_payload: usize) -> Self {
        Self {
            bytes: ChunkedBytes::with_max_payload(max_payload),
            len: 0,
            top: None,
        }
    }

    fn push(&mut self, value: u64) {
        let delta = self.top.map_or(value + 1, |top| value - top);
        push_reverse_varint(&mut self.bytes, delta);
        self.top = Some(value);
        self.len += 1;
    }

    fn pop(&mut self) -> Option<u64> {
        let current = self.top?;
        let (start, delta) = pop_reverse_varint(&self.bytes);
        self.bytes.truncate(start);
        self.len -= 1;
        self.top = if self.len == 0 {
            None
        } else {
            Some(current - delta)
        };
        Some(current)
    }

    fn payload_bytes(&self) -> usize {
        self.bytes.len()
    }

    fn allocated_bytes(&self) -> usize {
        self.bytes.allocated_bytes()
    }
}

#[derive(Debug)]
struct PackedOpenStack {
    ordinals: ReverseVarintStack,
    positions: ReverseVarintStack,
    fingerprint: u64,
}

impl PackedOpenStack {
    fn new(max_payload_per_lane: usize) -> Self {
        Self {
            ordinals: ReverseVarintStack::new(max_payload_per_lane),
            positions: ReverseVarintStack::new(max_payload_per_lane),
            fingerprint: 0,
        }
    }

    fn push(&mut self, ordinal: u64, position: u64) {
        self.ordinals.push(ordinal);
        self.positions.push(position);
        self.fingerprint ^= stack_item_hash(ordinal, position);
    }

    fn pop(&mut self) -> Option<(u64, u64)> {
        let ordinal = self.ordinals.pop()?;
        let position = self.positions.pop().expect("parallel packed stacks");
        self.fingerprint ^= stack_item_hash(ordinal, position);
        Some((ordinal, position))
    }

    fn is_empty(&self) -> bool {
        self.ordinals.len == 0
    }

    fn state_fingerprint(&self) -> u64 {
        mix(
            self.fingerprint,
            self.ordinals.top.unwrap_or(u64::MAX) ^ self.ordinals.len as u64,
        )
    }

    fn payload_bytes(&self) -> usize {
        self.ordinals.payload_bytes() + self.positions.payload_bytes()
    }

    fn allocated_bytes(&self) -> usize {
        self.ordinals.allocated_bytes() + self.positions.allocated_bytes()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FactKind {
    StarPair = 0,
    UnderscorePair = 1,
    BracketPair = 2,
}

impl FactKind {
    fn from_code(code: u8) -> Self {
        match code {
            0 => Self::StarPair,
            1 => Self::UnderscorePair,
            2 => Self::BracketPair,
            _ => unreachable!("invalid fact kind"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fact {
    pub kind: FactKind,
    pub start: u64,
    pub len: u64,
}

#[derive(Debug)]
struct PendingTape {
    bytes: ChunkedBytes,
    count: u64,
    previous_start: i64,
}

impl PendingTape {
    fn new(max_payload: usize) -> Self {
        Self {
            bytes: ChunkedBytes::with_max_payload(max_payload),
            count: 0,
            previous_start: 0,
        }
    }

    fn push(&mut self, fact: Fact) {
        let start = fact.start as i64;
        let delta = start - self.previous_start;
        let header = (zigzag(delta) << 2) | fact.kind as u64;
        push_varint(&mut self.bytes, header);
        push_varint(&mut self.bytes, fact.len);
        self.previous_start = start;
        self.count += 1;
    }
}

#[derive(Debug, Default)]
struct PendingDecoder {
    offset: usize,
    previous_start: i64,
}

impl PendingDecoder {
    fn next(&mut self, tape: &PendingTape) -> Option<Fact> {
        if self.offset == tape.bytes.len() {
            return None;
        }
        let header = read_varint(&tape.bytes, &mut self.offset);
        let delta = unzigzag(header >> 2);
        let start = self.previous_start + delta;
        let len = read_varint(&tape.bytes, &mut self.offset);
        self.previous_start = start;
        Some(Fact {
            kind: FactKind::from_code((header & 3) as u8),
            start: start as u64,
            len,
        })
    }
}

#[derive(Debug)]
struct FactPage {
    bytes: ChunkedBytes,
    count: u32,
    digest: u64,
}

#[derive(Clone, Debug)]
struct FactPageRef {
    base: u64,
    page: Arc<FactPage>,
}

#[derive(Debug)]
struct FactBuilder {
    base: Option<u64>,
    previous_start: i64,
    bytes: ChunkedBytes,
    count: u32,
}

impl FactBuilder {
    fn new() -> Self {
        Self {
            base: None,
            previous_start: 0,
            bytes: ChunkedBytes::with_max_payload(FACT_PAGE_EVENTS as usize * 20),
            count: 0,
        }
    }

    fn push(&mut self, fact: Fact) {
        let base = *self.base.get_or_insert(fact.start);
        let relative = fact.start as i64 - base as i64;
        let delta = relative - self.previous_start;
        push_varint(&mut self.bytes, (zigzag(delta) << 2) | fact.kind as u64);
        push_varint(&mut self.bytes, fact.len);
        self.previous_start = relative;
        self.count += 1;
    }

    fn is_full(&self) -> bool {
        self.count == FACT_PAGE_EVENTS
    }

    fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn seal(&mut self) -> FactPageRef {
        let base = self.base.take().expect("non-empty fact page");
        let bytes = std::mem::replace(&mut self.bytes, Self::new().bytes);
        let count = std::mem::take(&mut self.count);
        self.previous_start = 0;
        let digest = bytes.digest;
        FactPageRef {
            base,
            page: Arc::new(FactPage {
                bytes,
                count,
                digest,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Scan,
    SealLexPage,
    Resolve,
    Emit,
    SealFactPage,
    Eof,
    Done,
    Cancelled,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PhaseTicks {
    pub scan: u64,
    pub seal_lex_page: u64,
    pub resolve: u64,
    pub emit: u64,
    pub seal_fact_page: u64,
    pub eof: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub source_bytes: u64,
    pub lexical_events: u64,
    pub lexical_payload_bytes: usize,
    pub pending_facts: u64,
    pub pending_payload_bytes: usize,
    pub fact_payload_bytes: usize,
    pub fact_count: u64,
    pub max_stack_payload_bytes: usize,
    pub max_stack_allocated_bytes: usize,
    pub retained_old_bytes: usize,
    pub peak_accounted_bytes: usize,
    pub phase_ticks: PhaseTicks,
}

impl Metrics {
    pub fn lexical_bytes_per_event(&self) -> f64 {
        if self.lexical_events == 0 {
            0.0
        } else {
            self.lexical_payload_bytes as f64 / self.lexical_events as f64
        }
    }

    pub fn fact_bytes_per_event(&self) -> f64 {
        if self.fact_count == 0 {
            0.0
        } else {
            self.fact_payload_bytes as f64 / self.fact_count as f64
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Progress {
    pub phase: Phase,
    pub fuel_used: usize,
}

#[derive(Debug)]
pub struct Engine {
    source: SegmentedSource,
    cursor: SegmentedCursor,
    cancellation: Arc<AtomicBool>,
    phase: Phase,
    active_segment: Option<usize>,
    lex_builder: Option<LexBuilder>,
    run: Option<Run>,
    lex_pages: Vec<LexPageRef>,
    lex_decoder: LexDecoder,
    star_stack: PackedOpenStack,
    underscore_stack: PackedOpenStack,
    bracket_stack: PackedOpenStack,
    resolution_states: Vec<(u64, u64)>,
    pending: PendingTape,
    pending_decoder: PendingDecoder,
    fact_builder: FactBuilder,
    fact_pages: Vec<FactPageRef>,
    seal_then_eof: bool,
    final_state_fingerprint: Option<u64>,
    document_fingerprint: Option<u64>,
    source_digest: u64,
    lex_page_max_payload: usize,
    metrics: Metrics,
}

impl Engine {
    pub fn new(
        source: SegmentedSource,
        cancellation: Arc<AtomicBool>,
        retained_old_bytes: usize,
    ) -> Self {
        assert!(
            source.len() <= MAX_PROTOTYPE_SOURCE_BYTES,
            "packed-state spike is intentionally capped at 16 MiB"
        );
        let cursor = source.cursor();
        let max_encoded_tape = source.len() as usize * MAX_ENCODED_BYTES_PER_SOURCE_BYTE + 32;
        let max_lex_pages = source.segment_count().max(1);
        // A delimiter run can bridge one page boundary after both surrounding
        // segments have already contributed dense records. Longer bridges are
        // themselves compressed runs, so two maximum segments plus a small
        // record allowance is the conservative toy-grammar bound.
        let lex_page_max_payload = source.max_segment_len() * 2 + 64;
        let max_fact_pages = (source.len() as usize / 2)
            .div_ceil(FACT_PAGE_EVENTS as usize)
            .max(1);
        let mut engine = Self {
            metrics: Metrics {
                source_bytes: source.len(),
                retained_old_bytes,
                ..Metrics::default()
            },
            source,
            cursor,
            cancellation,
            phase: Phase::Scan,
            active_segment: None,
            lex_builder: None,
            run: None,
            lex_pages: Vec::with_capacity(max_lex_pages),
            lex_decoder: LexDecoder::default(),
            star_stack: PackedOpenStack::new(max_encoded_tape),
            underscore_stack: PackedOpenStack::new(max_encoded_tape),
            bracket_stack: PackedOpenStack::new(max_encoded_tape),
            resolution_states: Vec::with_capacity(max_lex_pages),
            pending: PendingTape::new(max_encoded_tape),
            pending_decoder: PendingDecoder::default(),
            fact_builder: FactBuilder::new(),
            fact_pages: Vec::with_capacity(max_fact_pages),
            seal_then_eof: false,
            final_state_fingerprint: None,
            document_fingerprint: None,
            source_digest: FNV_OFFSET,
            lex_page_max_payload,
        };
        engine.observe_memory();
        engine
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    pub fn advance(&mut self, fuel: usize) -> Progress {
        assert!(fuel > 0);
        let mut used = 0;
        while used < fuel && !matches!(self.phase, Phase::Done | Phase::Cancelled) {
            if self.cancellation.load(Ordering::Relaxed) {
                self.phase = Phase::Cancelled;
                break;
            }
            self.tick();
            used += 1;
        }
        self.observe_memory();
        Progress {
            phase: self.phase,
            fuel_used: used,
        }
    }

    fn tick(&mut self) {
        match self.phase {
            Phase::Scan => {
                self.metrics.phase_ticks.scan += 1;
                self.tick_scan();
            }
            Phase::SealLexPage => {
                self.metrics.phase_ticks.seal_lex_page += 1;
                let builder = self.lex_builder.take().expect("lex page to seal");
                self.lex_pages.push(builder.seal());
                self.resolution_states.push((0, 0));
                self.active_segment = None;
                self.phase = Phase::Scan;
            }
            Phase::Resolve => {
                self.metrics.phase_ticks.resolve += 1;
                self.tick_resolve();
            }
            Phase::Emit => {
                self.metrics.phase_ticks.emit += 1;
                self.tick_emit();
            }
            Phase::SealFactPage => {
                self.metrics.phase_ticks.seal_fact_page += 1;
                self.fact_pages.push(self.fact_builder.seal());
                self.phase = if self.seal_then_eof {
                    Phase::Eof
                } else {
                    Phase::Emit
                };
            }
            Phase::Eof => {
                self.metrics.phase_ticks.eof += 1;
                let state = self.resolver_fingerprint();
                // Source and packed-output digests were accumulated a byte at
                // a time in earlier fuelled transitions. EOF is constant work.
                let mut document = mix(self.source_digest, state);
                document = mix(document, self.pending.bytes.digest);
                document = mix(document, self.pending.count);
                self.final_state_fingerprint = Some(state);
                self.document_fingerprint = Some(document);
                self.phase = Phase::Done;
            }
            Phase::Done | Phase::Cancelled => unreachable!(),
        }
    }

    fn tick_scan(&mut self) {
        let Some(next) = self.cursor.next_byte() else {
            self.flush_run();
            self.phase = Phase::Resolve;
            return;
        };
        if self.active_segment != Some(next.segment) {
            self.active_segment = Some(next.segment);
            if let Some(builder) = &mut self.lex_builder {
                builder.extend_source(self.source.segment(next.segment));
            } else {
                self.lex_builder = Some(LexBuilder::new(
                    self.source.segment_start(next.segment),
                    self.source.segment(next.segment),
                    self.lex_page_max_payload,
                ));
            }
        }
        self.source_digest ^= u64::from(next.byte);
        self.source_digest = self.source_digest.wrapping_mul(FNV_PRIME);
        self.lex_builder
            .as_mut()
            .expect("active lex page")
            .observe_source_byte(next.byte);

        match next.byte {
            b'*' | b'_' => match &mut self.run {
                Some(run) if run.byte == next.byte && run.start + run.len == next.absolute => {
                    run.len += 1;
                }
                _ => {
                    self.flush_run();
                    self.run = Some(Run {
                        byte: next.byte,
                        start: next.absolute,
                        len: 1,
                    });
                }
            },
            b'[' | b']' => {
                self.flush_run();
                let kind = if next.byte == b'[' {
                    LexKind::OpenBracket
                } else {
                    LexKind::CloseBracket
                };
                self.lex_builder
                    .as_mut()
                    .expect("active lex page")
                    .push(kind, next.absolute, 1);
            }
            _ => self.flush_run(),
        }

        if next.ends_segment {
            let continues_run = self
                .run
                .is_some_and(|run| self.cursor.peek_byte() == Some(run.byte));
            if !continues_run {
                self.flush_run();
                self.phase = Phase::SealLexPage;
            }
        }
    }

    fn flush_run(&mut self) {
        let Some(run) = self.run.take() else {
            return;
        };
        let kind = match (run.byte, run.len) {
            (b'*', 1) => LexKind::Star,
            (b'_', 1) => LexKind::Underscore,
            (b'*', _) => LexKind::StarRun,
            (b'_', _) => LexKind::UnderscoreRun,
            _ => unreachable!(),
        };
        self.lex_builder
            .as_mut()
            .expect("run belongs to active lex page")
            .push(kind, run.start, run.len);
    }

    fn tick_resolve(&mut self) {
        match self.lex_decoder.next(&self.lex_pages) {
            DecodeAction::BeginPage(index) => {
                self.resolution_states[index].0 = self.resolver_fingerprint();
            }
            DecodeAction::EndPage(index) => {
                self.resolution_states[index].1 = self.resolver_fingerprint();
            }
            DecodeAction::Event(event) => self.resolve_event(event),
            DecodeAction::Done => self.phase = Phase::Emit,
        }
    }

    fn resolve_event(&mut self, event: LexEvent) {
        match event.kind {
            LexKind::Star | LexKind::StarRun => {
                if self.star_stack.is_empty() {
                    self.star_stack.push(event.ordinal, event.position);
                } else {
                    let (_, start) = self.star_stack.pop().expect("checked non-empty");
                    self.pending.push(Fact {
                        kind: FactKind::StarPair,
                        start,
                        len: event.position + event.run_len - start,
                    });
                }
            }
            LexKind::Underscore | LexKind::UnderscoreRun => {
                if self.underscore_stack.is_empty() {
                    self.underscore_stack.push(event.ordinal, event.position);
                } else {
                    let (_, start) = self.underscore_stack.pop().expect("checked non-empty");
                    self.pending.push(Fact {
                        kind: FactKind::UnderscorePair,
                        start,
                        len: event.position + event.run_len - start,
                    });
                }
            }
            LexKind::OpenBracket => self.bracket_stack.push(event.ordinal, event.position),
            LexKind::CloseBracket => {
                if let Some((_, start)) = self.bracket_stack.pop() {
                    self.pending.push(Fact {
                        kind: FactKind::BracketPair,
                        start,
                        len: event.position + 1 - start,
                    });
                }
            }
        }
        let stack_payload = self.star_stack.payload_bytes()
            + self.underscore_stack.payload_bytes()
            + self.bracket_stack.payload_bytes();
        let stack_allocated = self.star_stack.allocated_bytes()
            + self.underscore_stack.allocated_bytes()
            + self.bracket_stack.allocated_bytes();
        self.metrics.max_stack_payload_bytes =
            self.metrics.max_stack_payload_bytes.max(stack_payload);
        self.metrics.max_stack_allocated_bytes =
            self.metrics.max_stack_allocated_bytes.max(stack_allocated);
    }

    fn tick_emit(&mut self) {
        if let Some(fact) = self.pending_decoder.next(&self.pending) {
            self.fact_builder.push(fact);
            if self.fact_builder.is_full() {
                self.seal_then_eof = false;
                self.phase = Phase::SealFactPage;
            }
        } else if self.fact_builder.is_empty() {
            self.phase = Phase::Eof;
        } else {
            self.seal_then_eof = true;
            self.phase = Phase::SealFactPage;
        }
    }

    fn resolver_fingerprint(&self) -> u64 {
        mix(
            mix(
                self.star_stack.state_fingerprint(),
                self.underscore_stack.state_fingerprint(),
            ),
            self.bracket_stack.state_fingerprint(),
        )
    }

    fn observe_memory(&mut self) {
        let lex_payload = self
            .lex_pages
            .iter()
            .map(|page| page.page.bytes.len())
            .sum::<usize>();
        let fact_payload = self
            .fact_pages
            .iter()
            .map(|page| page.page.bytes.len())
            .sum::<usize>();
        self.metrics.lexical_events = self
            .lex_pages
            .iter()
            .map(|page| u64::from(page.page.events))
            .sum();
        self.metrics.lexical_payload_bytes = lex_payload;
        self.metrics.pending_facts = self.pending.count;
        self.metrics.pending_payload_bytes = self.pending.bytes.len();
        self.metrics.fact_payload_bytes = fact_payload + self.fact_builder.bytes.len();
        self.metrics.fact_count = self
            .fact_pages
            .iter()
            .map(|page| u64::from(page.page.count))
            .sum::<u64>()
            + u64::from(self.fact_builder.count);

        let lex_allocated = self
            .lex_pages
            .iter()
            .map(|page| page.page.bytes.allocated_bytes())
            .sum::<usize>()
            + self.lex_pages.capacity() * size_of::<LexPageRef>()
            + self.lex_pages.len() * size_of::<LexPage>()
            + self
                .lex_builder
                .as_ref()
                .map_or(0, |builder| builder.bytes.allocated_bytes());
        let fact_allocated = self
            .fact_pages
            .iter()
            .map(|page| page.page.bytes.allocated_bytes())
            .sum::<usize>()
            + self.fact_pages.capacity() * size_of::<FactPageRef>()
            + self.fact_pages.len() * size_of::<FactPage>()
            + self.fact_builder.bytes.allocated_bytes();
        let live = self.source.conservative_retained_bytes()
            + lex_allocated
            + self.pending.bytes.allocated_bytes()
            + fact_allocated
            + self.metrics.max_stack_allocated_bytes
            + self.resolution_states.capacity() * size_of::<(u64, u64)>();
        self.metrics.peak_accounted_bytes = self
            .metrics
            .peak_accounted_bytes
            .max(live + self.metrics.retained_old_bytes);
    }

    pub fn finish(mut self) -> Result<(Checkpoint, Metrics), Phase> {
        if self.phase != Phase::Done {
            return Err(self.phase);
        }
        self.observe_memory();
        let checkpoint = Checkpoint {
            source: self.source,
            lex_pages: self.lex_pages,
            fact_pages: self.fact_pages,
            resolution_states: self.resolution_states,
            state_fingerprint: self.final_state_fingerprint.expect("EOF finalized state"),
            document_fingerprint: self.document_fingerprint.expect("EOF finalized document"),
            lexical_events: self.metrics.lexical_events,
            fact_count: self.metrics.fact_count,
            star_stack: self.star_stack,
            underscore_stack: self.underscore_stack,
            bracket_stack: self.bracket_stack,
        };
        Ok((checkpoint, self.metrics))
    }
}

#[derive(Debug)]
pub struct Checkpoint {
    source: SegmentedSource,
    lex_pages: Vec<LexPageRef>,
    fact_pages: Vec<FactPageRef>,
    resolution_states: Vec<(u64, u64)>,
    pub state_fingerprint: u64,
    pub document_fingerprint: u64,
    pub lexical_events: u64,
    pub fact_count: u64,
    star_stack: PackedOpenStack,
    underscore_stack: PackedOpenStack,
    bracket_stack: PackedOpenStack,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReuseStats {
    pub shared_source_segments: usize,
    pub reused_lex_suffix_pages: usize,
    pub reused_fact_suffix_pages: usize,
    pub reused_lex_payload_bytes: usize,
    pub reused_fact_payload_bytes: usize,
}

impl Checkpoint {
    pub fn retained_bytes(&self) -> usize {
        self.source.conservative_retained_bytes()
            + self
                .lex_pages
                .iter()
                .map(|page| {
                    page.page.bytes.allocated_bytes()
                        + size_of::<LexPageRef>()
                        + size_of::<LexPage>()
                })
                .sum::<usize>()
            + self
                .fact_pages
                .iter()
                .map(|page| {
                    page.page.bytes.allocated_bytes()
                        + size_of::<FactPageRef>()
                        + size_of::<FactPage>()
                })
                .sum::<usize>()
            + self.resolution_states.len() * size_of::<(u64, u64)>()
            + self.state_root_allocated_bytes()
    }

    pub fn lexical_payload_bytes(&self) -> usize {
        self.lex_pages
            .iter()
            .map(|page| page.page.bytes.len())
            .sum()
    }

    pub fn fact_payload_bytes(&self) -> usize {
        self.fact_pages
            .iter()
            .map(|page| page.page.bytes.len())
            .sum()
    }

    pub fn source(&self) -> &SegmentedSource {
        &self.source
    }

    pub fn state_root_payload_bytes(&self) -> usize {
        self.star_stack.payload_bytes()
            + self.underscore_stack.payload_bytes()
            + self.bracket_stack.payload_bytes()
    }

    pub fn state_root_allocated_bytes(&self) -> usize {
        self.star_stack.allocated_bytes()
            + self.underscore_stack.allocated_bytes()
            + self.bracket_stack.allocated_bytes()
    }

    pub fn equivalent_output(&self, other: &Self) -> bool {
        self.state_fingerprint == other.state_fingerprint
            && self.document_fingerprint == other.document_fingerprint
            && self.source.len() == other.source.len()
            && self.lexical_events == other.lexical_events
            && self.fact_count == other.fact_count
            && self.facts() == other.facts()
    }

    pub fn facts(&self) -> Vec<Fact> {
        let mut facts = Vec::with_capacity(self.fact_count as usize);
        for page in &self.fact_pages {
            let mut offset = 0;
            let mut previous = 0i64;
            while offset < page.page.bytes.len() {
                let header = read_varint(&page.page.bytes, &mut offset);
                let relative = previous + unzigzag(header >> 2);
                let len = read_varint(&page.page.bytes, &mut offset);
                facts.push(Fact {
                    kind: FactKind::from_code((header & 3) as u8),
                    start: (page.base as i64 + relative) as u64,
                    len,
                });
                previous = relative;
            }
        }
        facts
    }

    pub fn with_reusable_suffix(mut self, old: &Self) -> (Self, ReuseStats) {
        let mut stats = ReuseStats {
            shared_source_segments: self.source.shared_segment_count(&old.source),
            ..ReuseStats::default()
        };

        let mut new_index = self.lex_pages.len();
        let mut old_index = old.lex_pages.len();
        while new_index > 0 && old_index > 0 {
            let new_page = &self.lex_pages[new_index - 1];
            let old_page = &old.lex_pages[old_index - 1];
            let new_state = self.resolution_states[new_index - 1];
            let old_state = old.resolution_states[old_index - 1];
            if new_page.page.source_len != old_page.page.source_len
                || new_page.page.source_digest != old_page.page.source_digest
                || new_page.page.digest != old_page.page.digest
                || !new_page.page.bytes.payload_eq(&old_page.page.bytes)
                || new_page.page.scan_in != old_page.page.scan_in
                || new_page.page.scan_out != old_page.page.scan_out
                || new_state != old_state
            {
                break;
            }
            stats.reused_lex_suffix_pages += 1;
            stats.reused_lex_payload_bytes += new_page.page.bytes.len();
            self.lex_pages[new_index - 1].page = Arc::clone(&old_page.page);
            new_index -= 1;
            old_index -= 1;
        }

        let mut new_index = self.fact_pages.len();
        let mut old_index = old.fact_pages.len();
        while new_index > 0 && old_index > 0 {
            let new_page = &self.fact_pages[new_index - 1];
            let old_page = &old.fact_pages[old_index - 1];
            if new_page.page.count != old_page.page.count
                || new_page.page.digest != old_page.page.digest
                || !new_page.page.bytes.payload_eq(&old_page.page.bytes)
            {
                break;
            }
            stats.reused_fact_suffix_pages += 1;
            stats.reused_fact_payload_bytes += new_page.page.bytes.len();
            self.fact_pages[new_index - 1].page = Arc::clone(&old_page.page);
            new_index -= 1;
            old_index -= 1;
        }
        (self, stats)
    }
}

pub fn parse_to_checkpoint(source: SegmentedSource, fuel: usize) -> (Checkpoint, Metrics) {
    let cancellation = Arc::new(AtomicBool::new(false));
    let mut engine = Engine::new(source, cancellation, 0);
    while engine.phase() != Phase::Done {
        engine.advance(fuel);
    }
    engine.finish().expect("completed engine")
}

fn push_varint(output: &mut ChunkedBytes, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        output.push(byte);
        if value == 0 {
            return;
        }
    }
}

fn read_varint(input: &ChunkedBytes, offset: &mut usize) -> u64 {
    let mut result = 0u64;
    let mut shift = 0;
    loop {
        let byte = input.get(*offset);
        *offset += 1;
        result |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return result;
        }
        shift += 7;
        debug_assert!(shift < 64);
    }
}

fn push_reverse_varint(output: &mut ChunkedBytes, value: u64) {
    let groups = (64 - value.leading_zeros() as usize).max(1).div_ceil(7);
    for group in (0..groups).rev() {
        let mut byte = ((value >> (group * 7)) & 0x7f) as u8;
        if group == groups - 1 {
            byte |= 0x80;
        }
        output.push(byte);
    }
}

fn pop_reverse_varint(input: &ChunkedBytes) -> (usize, u64) {
    let mut start = input.len() - 1;
    while input.get(start) & 0x80 == 0 {
        start -= 1;
    }
    let mut value = 0u64;
    for index in start..input.len() {
        value = (value << 7) | u64::from(input.get(index) & 0x7f);
    }
    (start, value)
}

fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

fn unzigzag(value: u64) -> i64 {
    ((value >> 1) as i64) ^ (-((value & 1) as i64))
}

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

fn mix(left: u64, right: u64) -> u64 {
    let mut value = left ^ right.wrapping_add(0x9e3779b97f4a7c15).rotate_left(17);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58476d1ce4e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

fn stack_item_hash(ordinal: u64, position: u64) -> u64 {
    mix(ordinal, position)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: SegmentedSource, fuel: usize, retained: usize) -> (Checkpoint, Metrics) {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut engine = Engine::new(source, cancel, retained);
        while engine.phase() != Phase::Done {
            engine.advance(fuel);
        }
        engine.finish().unwrap()
    }

    #[test]
    fn segmented_cursor_preserves_utf8_bytes_and_edit_shares_suffix() {
        let source = SegmentedSource::from_text_chunked("aé🙂z", 2);
        let bytes = source.cursor().map_for_test();
        assert_eq!(bytes, "aé🙂z".as_bytes());
        let edited = source.edit(1..3, "E");
        assert_eq!(edited.len(), source.len() - 1);
        assert!(edited.shared_segment_count(&source) > 0);
    }

    #[test]
    fn segmented_edit_inserts_exactly_once_at_boundaries_and_eof() {
        let source = SegmentedSource::from_owned_segments(vec!["ab".into(), "cd".into()]);
        assert_eq!(flatten(&source.edit(2..2, "X")), "abXcd");
        assert_eq!(flatten(&source.edit(4..4, "X")), "abcdX");
        assert_eq!(flatten(&source.edit(1..1, "X")), "aXbcd");
        assert_eq!(flatten(&source.edit(1..3, "X")), "aXd");
    }

    #[test]
    fn reverse_varint_stack_round_trips_dense_ordinals() {
        let mut stack = ReverseVarintStack::new(1_000_000);
        for value in 0..100_000 {
            stack.push(value);
        }
        assert_eq!(stack.payload_bytes(), 100_000);
        for expected in (0..100_000).rev() {
            assert_eq!(stack.pop(), Some(expected));
        }
        assert_eq!(stack.pop(), None);
    }

    #[test]
    fn fuel_one_and_4096_have_identical_output() {
        let text = "start *a* [_b_] *** tail\n".repeat(128);
        let source = SegmentedSource::from_text_chunked(&text, 37);
        let (one, one_metrics) = run(source.clone(), 1, 0);
        let (wide, wide_metrics) = run(source, 4096, 0);
        assert!(one.equivalent_output(&wide));
        assert_eq!(one.facts(), wide.facts());
        assert_eq!(one_metrics.phase_ticks, wide_metrics.phase_ticks);
        assert!(one_metrics.phase_ticks.seal_lex_page > 0);
        assert!(one_metrics.phase_ticks.resolve > 0);
        assert!(one_metrics.phase_ticks.emit > 0);
        assert!(one_metrics.phase_ticks.seal_fact_page > 0);
        assert_eq!(one_metrics.phase_ticks.eof, 1);
    }

    #[test]
    fn cancellation_is_observed_during_resolution() {
        let source = SegmentedSource::from_text_chunked(&"*_".repeat(20_000), 1024);
        let cancel = Arc::new(AtomicBool::new(false));
        let mut engine = Engine::new(source, Arc::clone(&cancel), 0);
        while engine.phase() != Phase::Resolve {
            engine.advance(1);
        }
        engine.advance(100);
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(engine.advance(4096).phase, Phase::Cancelled);
        assert_eq!(engine.finish().unwrap_err(), Phase::Cancelled);
    }

    #[test]
    fn cancellation_is_observed_during_fact_page_sealing() {
        let source = SegmentedSource::from_text_chunked(&"*a*".repeat(10_000), 2048);
        let cancel = Arc::new(AtomicBool::new(false));
        let mut engine = Engine::new(source, Arc::clone(&cancel), 0);
        while engine.phase() != Phase::SealFactPage {
            engine.advance(1);
        }
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(engine.advance(1).phase, Phase::Cancelled);
    }

    #[test]
    fn cancellation_is_observed_before_eof_commit() {
        let source = SegmentedSource::from_text_chunked("plain text", 4);
        let cancel = Arc::new(AtomicBool::new(false));
        let mut engine = Engine::new(source, Arc::clone(&cancel), 0);
        while engine.phase() != Phase::Eof {
            engine.advance(1);
        }
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(engine.advance(1).phase, Phase::Cancelled);
    }

    #[test]
    fn dense_tape_and_fact_pages_are_compact() {
        let source = SegmentedSource::from_text_chunked(&"*_".repeat(250_000), 64 * 1024);
        let (checkpoint, metrics) = run(source, 4096, 0);
        assert_eq!(checkpoint.lexical_events, 500_000);
        assert!(metrics.lexical_bytes_per_event() <= 1.01);
        assert!(metrics.fact_bytes_per_event() <= 2.01);
    }

    #[test]
    fn all_open_brackets_use_two_packed_payload_bytes_each() {
        let count = 250_000usize;
        let source = SegmentedSource::from_text_chunked(&"[".repeat(count), 64 * 1024);
        let (_, metrics) = run(source, 4096, 0);
        assert_eq!(metrics.lexical_events, count as u64);
        assert!(metrics.max_stack_payload_bytes <= count * 2 + 16);
    }

    #[test]
    fn giant_delimiter_run_is_one_event_across_source_segments() {
        let source = SegmentedSource::from_text_chunked(&"*".repeat(1_000_000), 64 * 1024);
        let (_, metrics) = run(source, 4096, 0);
        assert_eq!(metrics.lexical_events, 1);
        assert!(metrics.lexical_payload_bytes <= 5);
    }

    #[test]
    fn source_segmentation_does_not_change_tokens_or_facts() {
        let text = "***a*** [_x_] plain *_ tail".repeat(100);
        let (single_bytes, _) = run(SegmentedSource::from_text_chunked(&text, 1), 4096, 0);
        let (wide, _) = run(SegmentedSource::from_text_chunked(&text, 137), 4096, 0);
        assert!(single_bytes.equivalent_output(&wide));
    }

    #[test]
    fn local_plain_edit_converges_and_reuses_immutable_suffix_pages() {
        let text = "*a* plain [_b_] tail\n".repeat(8_000);
        let source = SegmentedSource::from_text_chunked(&text, 4096);
        let (old, _) = run(source.clone(), 4096, 0);
        let edit_at = (text.len() / 3) as u64 + 1;
        let edited = source.edit(edit_at..edit_at, "x");
        let (candidate, metrics) = run(edited, 4096, old.retained_bytes());
        let (candidate, reuse) = candidate.with_reusable_suffix(&old);
        assert!(reuse.shared_source_segments > 0);
        assert!(reuse.reused_lex_suffix_pages > 1);
        assert!(reuse.reused_fact_suffix_pages > 1);
        assert!(metrics.peak_accounted_bytes > old.retained_bytes());
        assert!(candidate.fact_count > 0);
    }

    #[test]
    fn production_gaps_remain_explicit() {
        assert!(PRODUCTION_GAPS.len() >= 6);
        assert!(PRODUCTION_GAPS
            .iter()
            .any(|gap| gap.contains("not CommonMark")));
    }

    impl SegmentedCursor {
        fn map_for_test(mut self) -> Vec<u8> {
            let mut bytes = Vec::new();
            while let Some(next) = self.next_byte() {
                bytes.push(next.byte);
            }
            bytes
        }
    }

    fn flatten(source: &SegmentedSource) -> String {
        source.segments.iter().map(AsRef::as_ref).collect()
    }
}

//! A deliberately limited, executable Markdown grammar commitment slice.
//!
//! This is not a `CommonMark` implementation.  It exists to test one proposed
//! ownership boundary end to end: one [`SegmentedLeaf`] is lexed once by
//! [`SharedLexer`](crate::frontier::SharedLexer), and both table recognition
//! and inline resolution consume that exact immutable lexical root.  Grammar
//! passes may revisit logical source bytes, but they never run a second
//! punctuation lexer and never flatten the logical leaf.
//!
//! The supported subset is intentionally explicit:
//!
//! * one paragraph leaf;
//! * matching backtick code spans of any run length;
//! * ASCII `CommonMark` flanking for `*`, `_`, `**`, and `__`;
//! * one GFM table header, delimiter row, and optional single body row;
//! * at most [`MAX_TABLE_COLUMNS`] table columns and
//!   [`MAX_INLINE_EVENTS_PER_REGION`] lexical events per inline region.
//!
//! Larger or more complicated input produces a conservative literal record.
//! That fallback is part of the API: callers must not confuse this research
//! slice with full CommonMark/GFM coverage.

use std::fmt;
use std::sync::Arc;

use crate::frontier::{
    CursorStep, LexicalConsumers, LexicalEvent, LexicalEventKind, LexicalEvents, LogicalCursor,
    LogicalOrigin, SegmentedLeaf,
};
use crate::packed::PACKED_PAGE_BYTES;

/// Largest logical leaf accepted by this local grammar slice.
pub const MAX_GRAMMAR_LEAF_BYTES: usize = 64 * 1024;
/// Maximum columns recognized by the limited GFM table path.
pub const MAX_TABLE_COLUMNS: usize = 16;
/// Maximum lexical candidates retained for one paragraph or table cell.
pub const MAX_INLINE_EVENTS_PER_REGION: usize = 128;
/// Maximum poll fuel accepted by [`GrammarJob::poll`].
pub const MAX_GRAMMAR_POLL_WORK: usize = 4 * 1024;

const MAX_TABLE_LINES: usize = 3;
const MAX_TABLE_REGIONS: usize = MAX_TABLE_COLUMNS + 2;
const MAX_INLINE_REGIONS: usize = MAX_TABLE_COLUMNS * 2;
const MAX_OUTPUT_PAGES: usize = 64;
const MAX_RECORD_FIELDS: usize = 7;
const HASH_BASE: u64 = 0x0000_0100_0000_01b3;
const MAX_ENCODED_RECORD_BYTES: usize = 18;
const MAX_OUTPUT_RECORDS: usize = MAX_INLINE_REGIONS * (2 * MAX_INLINE_EVENTS_PER_REGION + 3) + 6;

// Each fact can produce at most one semantic record and one preceding text
// record. Per-region and document wrappers add the remaining terms above.
// Therefore accepted input cannot reach `PackedOutputBuilder` capacity.
const _: () =
    assert!(MAX_OUTPUT_RECORDS * MAX_ENCODED_RECORD_BYTES <= MAX_OUTPUT_PAGES * PACKED_PAGE_BYTES);

/// Explicit ceiling for one local, atomic region-resolution transition.
///
/// Code-span matching is currently a bounded quadratic search.  The bound is
/// small enough to falsify the architecture without allowing a pathological
/// document-sized atomic task.  Exceeding the event bound selects literal
/// fallback before this work occurs.
pub const MAX_ATOMIC_REGION_INDEX_UNITS: usize =
    MAX_INLINE_EVENTS_PER_REGION * MAX_INLINE_EVENTS_PER_REGION * 2
        + 8 * MAX_INLINE_EVENTS_PER_REGION
        + 32;

/// Alignment parsed from one GFM delimiter cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Alignment {
    #[default]
    None = 0,
    Left = 1,
    Center = 2,
    Right = 3,
}

impl Alignment {
    fn from_u64(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Left),
            2 => Some(Self::Center),
            3 => Some(Self::Right),
            _ => None,
        }
    }
}

/// Why exact grammar resolution deliberately fell back to literal text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FallbackReason {
    LeafTooLarge = 1,
    TooManyInlineEvents = 2,
    UnsupportedEmphasisRun = 3,
    UnsupportedEmphasisInteraction = 4,
}

impl FallbackReason {
    fn from_u64(value: u64) -> Option<Self> {
        match value {
            1 => Some(Self::LeafTooLarge),
            2 => Some(Self::TooManyInlineEvents),
            3 => Some(Self::UnsupportedEmphasisRun),
            4 => Some(Self::UnsupportedEmphasisInteraction),
            _ => None,
        }
    }
}

/// Typed schema decoded from the compact output pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrammarRecord {
    ParagraphStart {
        start: usize,
        end: usize,
    },
    ParagraphEnd,
    Text {
        start: usize,
        end: usize,
    },
    Escaped {
        start: usize,
        end: usize,
        byte: u8,
    },
    Code {
        start: usize,
        end: usize,
        content_start: usize,
        content_end: usize,
        delimiter_len: usize,
        table_cell: bool,
    },
    EmphasisStart {
        start: usize,
        end: usize,
    },
    EmphasisEnd {
        start: usize,
        end: usize,
    },
    StrongStart {
        start: usize,
        end: usize,
    },
    StrongEnd {
        start: usize,
        end: usize,
    },
    TableStart {
        start: usize,
        end: usize,
        columns: usize,
    },
    TableEnd,
    TableHeadStart,
    TableHeadEnd,
    TableRowStart,
    TableRowEnd,
    TableCellStart {
        start: usize,
        end: usize,
        column: usize,
        alignment: Alignment,
        header: bool,
    },
    TableCellEnd,
    LiteralFallback {
        start: usize,
        end: usize,
        reason: FallbackReason,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum RecordTag {
    ParagraphStart = 1,
    ParagraphEnd = 2,
    Text = 3,
    Escaped = 4,
    Code = 5,
    EmphasisStart = 6,
    EmphasisEnd = 7,
    StrongStart = 8,
    StrongEnd = 9,
    TableStart = 10,
    TableEnd = 11,
    TableHeadStart = 12,
    TableHeadEnd = 13,
    TableRowStart = 14,
    TableRowEnd = 15,
    TableCellStart = 16,
    TableCellEnd = 17,
    LiteralFallback = 18,
}

impl GrammarRecord {
    fn encode(self, target: &mut [u8; 96]) -> usize {
        let (tag, fields): (RecordTag, [u64; MAX_RECORD_FIELDS]) = match self {
            Self::ParagraphStart { start, end } => (
                RecordTag::ParagraphStart,
                [as_u64(start), as_u64(end), 0, 0, 0, 0, 0],
            ),
            Self::ParagraphEnd => (RecordTag::ParagraphEnd, [0; MAX_RECORD_FIELDS]),
            Self::Text { start, end } => {
                (RecordTag::Text, [as_u64(start), as_u64(end), 0, 0, 0, 0, 0])
            }
            Self::Escaped { start, end, byte } => (
                RecordTag::Escaped,
                [as_u64(start), as_u64(end), u64::from(byte), 0, 0, 0, 0],
            ),
            Self::Code {
                start,
                end,
                content_start,
                content_end,
                delimiter_len,
                table_cell,
            } => (
                RecordTag::Code,
                [
                    as_u64(start),
                    as_u64(end),
                    as_u64(content_start),
                    as_u64(content_end),
                    as_u64(delimiter_len),
                    u64::from(table_cell),
                    0,
                ],
            ),
            Self::EmphasisStart { start, end } => (
                RecordTag::EmphasisStart,
                [as_u64(start), as_u64(end), 0, 0, 0, 0, 0],
            ),
            Self::EmphasisEnd { start, end } => (
                RecordTag::EmphasisEnd,
                [as_u64(start), as_u64(end), 0, 0, 0, 0, 0],
            ),
            Self::StrongStart { start, end } => (
                RecordTag::StrongStart,
                [as_u64(start), as_u64(end), 0, 0, 0, 0, 0],
            ),
            Self::StrongEnd { start, end } => (
                RecordTag::StrongEnd,
                [as_u64(start), as_u64(end), 0, 0, 0, 0, 0],
            ),
            Self::TableStart {
                start,
                end,
                columns,
            } => (
                RecordTag::TableStart,
                [as_u64(start), as_u64(end), as_u64(columns), 0, 0, 0, 0],
            ),
            Self::TableEnd => (RecordTag::TableEnd, [0; MAX_RECORD_FIELDS]),
            Self::TableHeadStart => (RecordTag::TableHeadStart, [0; MAX_RECORD_FIELDS]),
            Self::TableHeadEnd => (RecordTag::TableHeadEnd, [0; MAX_RECORD_FIELDS]),
            Self::TableRowStart => (RecordTag::TableRowStart, [0; MAX_RECORD_FIELDS]),
            Self::TableRowEnd => (RecordTag::TableRowEnd, [0; MAX_RECORD_FIELDS]),
            Self::TableCellStart {
                start,
                end,
                column,
                alignment,
                header,
            } => (
                RecordTag::TableCellStart,
                [
                    as_u64(start),
                    as_u64(end),
                    as_u64(column),
                    alignment as u64,
                    u64::from(header),
                    0,
                    0,
                ],
            ),
            Self::TableCellEnd => (RecordTag::TableCellEnd, [0; MAX_RECORD_FIELDS]),
            Self::LiteralFallback { start, end, reason } => (
                RecordTag::LiteralFallback,
                [as_u64(start), as_u64(end), reason as u64, 0, 0, 0, 0],
            ),
        };
        let field_count = record_field_count(tag);
        let header = (u64::from(tag as u8) << 4) | as_u64(field_count);
        let mut len = encode_varint(header, target);
        for field in &fields[..field_count] {
            len += encode_varint(*field, &mut target[len..]);
        }
        len
    }
}

fn record_field_count(tag: RecordTag) -> usize {
    match tag {
        RecordTag::ParagraphStart
        | RecordTag::Text
        | RecordTag::EmphasisStart
        | RecordTag::EmphasisEnd
        | RecordTag::StrongStart
        | RecordTag::StrongEnd => 2,
        RecordTag::Escaped | RecordTag::TableStart | RecordTag::LiteralFallback => 3,
        RecordTag::Code => 6,
        RecordTag::TableCellStart => 5,
        RecordTag::ParagraphEnd
        | RecordTag::TableEnd
        | RecordTag::TableHeadStart
        | RecordTag::TableHeadEnd
        | RecordTag::TableRowStart
        | RecordTag::TableRowEnd
        | RecordTag::TableCellEnd => 0,
    }
}

fn as_u64(value: usize) -> u64 {
    u64::try_from(value).expect("logical offsets fit u64")
}

fn as_usize(value: u64) -> Option<usize> {
    usize::try_from(value).ok()
}

fn encode_varint(mut value: u64, target: &mut [u8]) -> usize {
    let mut written = 0;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        target[written] = byte;
        written += 1;
        if value == 0 {
            return written;
        }
    }
}

fn decode_varint(bytes: &[u8], cursor: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*cursor)?;
        *cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

#[derive(Debug)]
struct GrammarPage {
    bytes: Box<[u8]>,
    records: usize,
}

/// Immutable compact grammar output with a typed decoder.
#[derive(Clone, Debug)]
pub struct PackedGrammarOutput {
    pages: Arc<[Arc<GrammarPage>]>,
    records: usize,
    payload_bytes: usize,
    digest: u64,
}

impl PackedGrammarOutput {
    #[must_use]
    pub const fn record_count(&self) -> usize {
        self.records
    }

    #[must_use]
    pub const fn payload_bytes(&self) -> usize {
        self.payload_bytes
    }

    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    #[must_use]
    pub const fn digest(&self) -> u64 {
        self.digest
    }

    #[must_use]
    pub fn records(&self) -> GrammarRecords<'_> {
        GrammarRecords {
            output: self,
            page: 0,
            byte: 0,
            remaining: self.records,
            page_records_remaining: self.pages.first().map_or(0, |page| page.records),
        }
    }
}

/// Typed iterator over packed records. Malformed pages stop decoding.
pub struct GrammarRecords<'a> {
    output: &'a PackedGrammarOutput,
    page: usize,
    byte: usize,
    remaining: usize,
    page_records_remaining: usize,
}

impl Iterator for GrammarRecords<'_> {
    type Item = GrammarRecord;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        while self.page_records_remaining == 0 {
            self.page += 1;
            self.byte = 0;
            self.page_records_remaining = self.output.pages.get(self.page)?.records;
        }
        let page = self.output.pages.get(self.page)?;
        let header = decode_varint(&page.bytes, &mut self.byte)?;
        let field_count = usize::try_from(header & 0x0f).ok()?;
        if field_count > MAX_RECORD_FIELDS {
            return None;
        }
        let tag = u8::try_from(header >> 4).ok()?;
        let mut fields = [0u64; MAX_RECORD_FIELDS];
        for field in &mut fields[..field_count] {
            *field = decode_varint(&page.bytes, &mut self.byte)?;
        }
        let decoded = decode_record(tag, &fields[..field_count])?;
        self.remaining -= 1;
        self.page_records_remaining -= 1;
        Some(decoded)
    }
}

fn decode_record(tag: u8, fields: &[u64]) -> Option<GrammarRecord> {
    let range = |fields: &[u64]| Some((as_usize(*fields.first()?)?, as_usize(*fields.get(1)?)?));
    Some(match tag {
        1 if fields.len() == 2 => {
            let (start, end) = range(fields)?;
            GrammarRecord::ParagraphStart { start, end }
        }
        2 if fields.is_empty() => GrammarRecord::ParagraphEnd,
        3 if fields.len() == 2 => {
            let (start, end) = range(fields)?;
            GrammarRecord::Text { start, end }
        }
        4 if fields.len() == 3 => {
            let (start, end) = range(fields)?;
            GrammarRecord::Escaped {
                start,
                end,
                byte: u8::try_from(fields[2]).ok()?,
            }
        }
        5 if fields.len() == 6 => GrammarRecord::Code {
            start: as_usize(fields[0])?,
            end: as_usize(fields[1])?,
            content_start: as_usize(fields[2])?,
            content_end: as_usize(fields[3])?,
            delimiter_len: as_usize(fields[4])?,
            table_cell: match fields[5] {
                0 => false,
                1 => true,
                _ => return None,
            },
        },
        6 if fields.len() == 2 => {
            let (start, end) = range(fields)?;
            GrammarRecord::EmphasisStart { start, end }
        }
        7 if fields.len() == 2 => {
            let (start, end) = range(fields)?;
            GrammarRecord::EmphasisEnd { start, end }
        }
        8 if fields.len() == 2 => {
            let (start, end) = range(fields)?;
            GrammarRecord::StrongStart { start, end }
        }
        9 if fields.len() == 2 => {
            let (start, end) = range(fields)?;
            GrammarRecord::StrongEnd { start, end }
        }
        10 if fields.len() == 3 => GrammarRecord::TableStart {
            start: as_usize(fields[0])?,
            end: as_usize(fields[1])?,
            columns: as_usize(fields[2])?,
        },
        11 if fields.is_empty() => GrammarRecord::TableEnd,
        12 if fields.is_empty() => GrammarRecord::TableHeadStart,
        13 if fields.is_empty() => GrammarRecord::TableHeadEnd,
        14 if fields.is_empty() => GrammarRecord::TableRowStart,
        15 if fields.is_empty() => GrammarRecord::TableRowEnd,
        16 if fields.len() == 5 => GrammarRecord::TableCellStart {
            start: as_usize(fields[0])?,
            end: as_usize(fields[1])?,
            column: as_usize(fields[2])?,
            alignment: Alignment::from_u64(fields[3])?,
            header: match fields[4] {
                0 => false,
                1 => true,
                _ => return None,
            },
        },
        17 if fields.is_empty() => GrammarRecord::TableCellEnd,
        18 if fields.len() == 3 => GrammarRecord::LiteralFallback {
            start: as_usize(fields[0])?,
            end: as_usize(fields[1])?,
            reason: FallbackReason::from_u64(fields[2])?,
        },
        _ => return None,
    })
}

#[derive(Debug)]
struct PackedOutputBuilder {
    page: Box<[u8; PACKED_PAGE_BYTES]>,
    page_len: usize,
    page_records: usize,
    pages: Vec<Arc<GrammarPage>>,
    records: usize,
    payload_bytes: usize,
    digest: u64,
    allocation_units: usize,
    copy_units: usize,
    hash_units: usize,
    index_units: usize,
    capacity_exceeded: bool,
}

impl PackedOutputBuilder {
    fn new() -> Self {
        Self {
            page: Box::new([0; PACKED_PAGE_BYTES]),
            page_len: 0,
            page_records: 0,
            pages: Vec::with_capacity(MAX_OUTPUT_PAGES),
            records: 0,
            payload_bytes: 0,
            digest: 0,
            // One fixed page and one non-zero-capacity page-index allocation.
            allocation_units: 2,
            copy_units: 0,
            hash_units: 0,
            index_units: 0,
            capacity_exceeded: false,
        }
    }

    fn push(&mut self, record: GrammarRecord) {
        if self.capacity_exceeded {
            return;
        }
        let mut encoded = [0u8; 96];
        let len = record.encode(&mut encoded);
        debug_assert!(len < PACKED_PAGE_BYTES);
        if self.page_len + len > PACKED_PAGE_BYTES {
            self.seal_page();
        }
        if self.pages.len() == MAX_OUTPUT_PAGES {
            self.capacity_exceeded = true;
            return;
        }
        let destination = &mut self.page[self.page_len..self.page_len + len];
        destination.copy_from_slice(&encoded[..len]);
        self.copy_units += len;
        for byte in &encoded[..len] {
            self.digest = self
                .digest
                .wrapping_mul(HASH_BASE)
                .wrapping_add(u64::from(*byte) + 1);
            self.hash_units += 1;
        }
        self.page_len += len;
        self.page_records += 1;
        self.records += 1;
        self.payload_bytes += len;
        self.index_units += 1;
    }

    fn seal_page(&mut self) {
        if self.page_len == 0 || self.pages.len() == MAX_OUTPUT_PAGES {
            if self.pages.len() == MAX_OUTPUT_PAGES && self.page_len != 0 {
                self.capacity_exceeded = true;
            }
            return;
        }
        let bytes = self.page[..self.page_len].to_vec().into_boxed_slice();
        self.copy_units += self.page_len;
        self.allocation_units += 1;
        let page = Arc::new(GrammarPage {
            bytes,
            records: self.page_records,
        });
        self.allocation_units += 1;
        self.pages.push(page);
        self.index_units += 1;
        self.page_len = 0;
        self.page_records = 0;
    }

    fn finish(mut self) -> (PackedGrammarOutput, OutputWork) {
        self.seal_page();
        // Converting Vec into Arc<[T]> copies the page index into one exact
        // allocation.  The Vec allocation is then released; both requests are
        // charged because this is work accounting, not retained accounting.
        let page_index_bytes = self.pages.len() * std::mem::size_of::<Arc<GrammarPage>>();
        self.copy_units += page_index_bytes;
        self.index_units += self.pages.len();
        let pages: Arc<[Arc<GrammarPage>]> = self.pages.into();
        self.allocation_units += 1;
        let output = PackedGrammarOutput {
            pages,
            records: self.records,
            payload_bytes: self.payload_bytes,
            digest: self.digest,
        };
        (
            output,
            OutputWork {
                allocation_units: self.allocation_units,
                copy_units: self.copy_units,
                hash_units: self.hash_units,
                index_units: self.index_units,
                capacity_exceeded: self.capacity_exceeded,
            },
        )
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct OutputWork {
    allocation_units: usize,
    copy_units: usize,
    hash_units: usize,
    index_units: usize,
    capacity_exceeded: bool,
}

/// Auditable work dimensions for the complete grammar job.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GrammarWorkReceipt {
    /// Logical bytes examined across both grammar passes. Re-reading is charged.
    pub logical_bytes_inspected: usize,
    /// Physical logical bytes plus excluded physical bytes traversed by cursors.
    pub source_bytes_inspected: usize,
    pub virtual_bytes_inspected: usize,
    pub excluded_source_bytes_inspected: usize,
    /// Excluded source bytes bypassed through persistent-index seeks and never
    /// inspected by grammar.
    pub source_bytes_skipped: usize,
    /// Persistent source-tree nodes inspected to perform those seeks.
    pub source_index_nodes_examined: usize,
    /// Calls to `LogicalCursor::step`, including progress and EOF transitions.
    pub cursor_transitions: usize,
    /// Candidate events pulled from the shared immutable lexical root.
    pub lexical_events_examined: usize,
    /// Grammar state-machine transitions, which are the poll-fuel dimension.
    pub parser_transitions: usize,
    /// Explicit array lookup, comparison, stack, and page-index operations.
    pub index_units: usize,
    /// Exact allocation requests owned by this module's packed output builder.
    pub grammar_allocation_units: usize,
    /// Upstream allocation sites invoked but not observable through current APIs.
    ///
    /// The two `LogicalCursor`s and two `LexicalEvents` iterators internally own
    /// `Vec` traversal stacks.  Frontier APIs expose neither allocation counts
    /// nor allocation-free cursors, so this field is a count of unmeterable
    /// sites, not a fabricated allocator-call total.
    pub unmetered_upstream_allocation_sites: usize,
    /// Bytes copied into mutable output pages and then immutable page payloads.
    pub copy_units: usize,
    /// Bytes first encoded into the bounded stack record buffer.
    pub encode_units: usize,
    /// Bytes mixed into the packed-output digest.
    pub hash_units: usize,
    /// Encoded packed bytes retained by the result.
    pub output_payload_bytes: usize,
    pub output_records: usize,
    /// Largest bounded atomic resolver transition actually observed.
    pub max_atomic_index_units: usize,
}

/// Result classification, including conservative fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrammarClassification {
    Empty,
    Paragraph,
    Table { columns: usize, body_rows: usize },
    LiteralFallback(FallbackReason),
}

/// Complete immutable grammar result.
#[derive(Clone, Debug)]
pub struct GrammarOutput {
    pub records: PackedGrammarOutput,
    pub classification: GrammarClassification,
    pub receipt: GrammarWorkReceipt,
    /// Exact logical-to-physical origin root for every record offset.
    ///
    /// Production transport will replace this in-process root with an arena
    /// child handle; dropping it would make virtual/container leaf ranges
    /// impossible to project back to stable source anchors.
    pub input: SegmentedLeaf,
}

/// Construction errors that indicate an invalid ownership boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrammarError {
    LexicalRootsDiffer,
    MissingLogicalInput,
}

impl fmt::Display for GrammarError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LexicalRootsDiffer => {
                formatter.write_str("table and inline consumers do not share one lexical root")
            }
            Self::MissingLogicalInput => formatter.write_str("lexical root has no logical input"),
        }
    }
}

impl std::error::Error for GrammarError {}

#[derive(Clone, Copy, Debug, Default)]
struct CellScan {
    raw_start: usize,
    raw_end: usize,
    trimmed_start: usize,
    trimmed_end: usize,
    has_non_space: bool,
    first: u8,
    last: u8,
    dash_count: usize,
    colon_count: usize,
    invalid_delimiter_byte: bool,
}

impl CellScan {
    const EMPTY: Self = Self {
        raw_start: 0,
        raw_end: 0,
        trimmed_start: 0,
        trimmed_end: 0,
        has_non_space: false,
        first: 0,
        last: 0,
        dash_count: 0,
        colon_count: 0,
        invalid_delimiter_byte: false,
    };

    fn begin(offset: usize) -> Self {
        Self {
            raw_start: offset,
            raw_end: offset,
            trimmed_start: offset,
            trimmed_end: offset,
            ..Self::EMPTY
        }
    }

    fn observe(&mut self, offset: usize, byte: u8) {
        self.raw_end = offset + 1;
        if matches!(byte, b' ' | b'\t' | b'\r') {
            return;
        }
        if !self.has_non_space {
            self.has_non_space = true;
            self.trimmed_start = offset;
            self.first = byte;
        }
        self.trimmed_end = offset + 1;
        self.last = byte;
        match byte {
            b'-' => self.dash_count += 1,
            b':' => self.colon_count += 1,
            _ => self.invalid_delimiter_byte = true,
        }
    }

    fn finish(&mut self, end: usize) {
        self.raw_end = end;
        if !self.has_non_space {
            self.trimmed_start = self.raw_start;
            self.trimmed_end = self.raw_start;
        }
    }

    fn is_blank(self) -> bool {
        !self.has_non_space
    }

    fn content_range(self) -> InlineRange {
        InlineRange {
            start: self.trimmed_start,
            end: self.trimmed_end,
            column: 0,
            alignment: Alignment::None,
            header: false,
        }
    }

    fn delimiter_alignment(self) -> Option<Alignment> {
        if self.invalid_delimiter_byte || self.dash_count == 0 || self.colon_count > 2 {
            return None;
        }
        let left = self.first == b':';
        let right = self.last == b':';
        if self.colon_count != usize::from(left) + usize::from(right) {
            return None;
        }
        Some(match (left, right) {
            (false, false) => Alignment::None,
            (true, false) => Alignment::Left,
            (false, true) => Alignment::Right,
            (true, true) => Alignment::Center,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct LineScan {
    start: usize,
    end: usize,
    regions: [CellScan; MAX_TABLE_REGIONS],
    region_count: usize,
    pipe_count: usize,
    overflow: bool,
}

impl LineScan {
    const EMPTY: Self = Self {
        start: 0,
        end: 0,
        regions: [CellScan::EMPTY; MAX_TABLE_REGIONS],
        region_count: 0,
        pipe_count: 0,
        overflow: false,
    };

    fn begin(offset: usize) -> Self {
        let mut line = Self {
            start: offset,
            end: offset,
            ..Self::EMPTY
        };
        line.regions[0] = CellScan::begin(offset);
        line.region_count = 1;
        line
    }

    fn observe(&mut self, offset: usize, byte: u8, is_pipe: bool) {
        if is_pipe {
            let current = self.region_count.saturating_sub(1);
            self.regions[current].finish(offset);
            self.pipe_count += 1;
            if self.region_count == MAX_TABLE_REGIONS {
                self.overflow = true;
                return;
            }
            self.regions[self.region_count] = CellScan::begin(offset + 1);
            self.region_count += 1;
        } else {
            let current = self.region_count.saturating_sub(1);
            self.regions[current].observe(offset, byte);
        }
    }

    fn finish(&mut self, end: usize) {
        self.end = end;
        let current = self.region_count.saturating_sub(1);
        self.regions[current].finish(end);
    }

    fn cell_bounds(self) -> Option<(usize, usize)> {
        if self.overflow || self.region_count == 0 {
            return None;
        }
        let mut start = 0;
        let mut end = self.region_count;
        if self.pipe_count > 0 && self.regions[0].is_blank() {
            start += 1;
        }
        if self.pipe_count > 0 && end > start && self.regions[end - 1].is_blank() {
            end -= 1;
        }
        (start < end).then_some((start, end))
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct InlineRange {
    start: usize,
    end: usize,
    column: usize,
    alignment: Alignment,
    header: bool,
}

#[derive(Clone, Copy, Debug)]
struct TableModel {
    regions: [InlineRange; MAX_INLINE_REGIONS],
    region_count: usize,
    columns: usize,
    has_body: bool,
    start: usize,
    end: usize,
}

#[derive(Clone, Copy, Debug)]
struct TableScanner {
    lines: [LineScan; MAX_TABLE_LINES],
    line_count: usize,
    current: LineScan,
    too_many_lines: bool,
    saw_byte_on_line: bool,
    ended_with_newline: bool,
}

impl TableScanner {
    fn new() -> Self {
        Self {
            lines: [LineScan::EMPTY; MAX_TABLE_LINES],
            line_count: 0,
            current: LineScan::begin(0),
            too_many_lines: false,
            saw_byte_on_line: false,
            ended_with_newline: false,
        }
    }

    fn observe(&mut self, offset: usize, byte: u8, is_pipe: bool) {
        if byte == b'\n' {
            self.finish_line(offset);
            self.current = LineScan::begin(offset + 1);
            self.saw_byte_on_line = false;
            self.ended_with_newline = true;
        } else {
            self.current.observe(offset, byte, is_pipe);
            self.saw_byte_on_line = true;
            self.ended_with_newline = false;
        }
    }

    fn finish_line(&mut self, end: usize) {
        self.current.finish(end);
        if self.line_count < MAX_TABLE_LINES {
            self.lines[self.line_count] = self.current;
            self.line_count += 1;
        } else {
            self.too_many_lines = true;
        }
    }

    fn finish(
        mut self,
        logical_len: usize,
        receipt: &mut GrammarWorkReceipt,
    ) -> Option<TableModel> {
        if !self.ended_with_newline || self.saw_byte_on_line || logical_len == 0 {
            self.finish_line(logical_len);
        }
        receipt.index_units += 1;
        if self.too_many_lines || !(2..=3).contains(&self.line_count) {
            return None;
        }
        let header = self.lines[0];
        let delimiter = self.lines[1];
        let (header_start, header_end) = header.cell_bounds()?;
        let (delimiter_start, delimiter_end) = delimiter.cell_bounds()?;
        let columns = header_end - header_start;
        receipt.index_units += 4;
        if columns == 0
            || columns > MAX_TABLE_COLUMNS
            || delimiter_end - delimiter_start != columns
            || header.pipe_count + delimiter.pipe_count == 0
        {
            return None;
        }

        let body_bounds = if self.line_count == 3 {
            let body = self.lines[2];
            let bounds = body.cell_bounds()?;
            if bounds.1 - bounds.0 != columns {
                return None;
            }
            Some(bounds)
        } else {
            None
        };

        let mut alignments = [Alignment::None; MAX_TABLE_COLUMNS];
        for (column, alignment) in alignments[..columns].iter_mut().enumerate() {
            receipt.index_units += 1;
            *alignment = delimiter.regions[delimiter_start + column].delimiter_alignment()?;
        }

        let mut regions = [InlineRange::default(); MAX_INLINE_REGIONS];
        for column in 0..columns {
            receipt.index_units += 1;
            let mut range = header.regions[header_start + column].content_range();
            range.column = column;
            range.alignment = alignments[column];
            range.header = true;
            regions[column] = range;
        }
        let mut region_count = columns;
        if let Some((body_start, _)) = body_bounds {
            let body = self.lines[2];
            for (column, alignment) in alignments[..columns].iter().copied().enumerate() {
                receipt.index_units += 1;
                let mut range = body.regions[body_start + column].content_range();
                range.column = column;
                range.alignment = alignment;
                range.header = false;
                regions[region_count] = range;
                region_count += 1;
            }
        }
        Some(TableModel {
            regions,
            region_count,
            columns,
            has_body: body_bounds.is_some(),
            start: header.start,
            end: self.lines[self.line_count - 1].end,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FactRole {
    None,
    Escape,
    CodeOpen { closer: usize },
    CodeClose,
    EmphasisOpen { strong: bool },
    EmphasisClose { strong: bool },
    Suppressed,
}

#[derive(Clone, Copy, Debug)]
struct EventFact {
    event: LexicalEvent,
    previous: Option<u8>,
    next: Option<u8>,
    role: FactRole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    OversizedFallback,
    TableScan,
    PrepareInline,
    InlineScan,
    Finish,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventFeedState {
    Available,
    Exhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BodyState {
    NotStarted,
    Started,
}

/// Poll completion status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrammarStatus {
    Pending,
    Ready,
}

/// One bounded parser poll. `work` is exactly the number of grammar state
/// transitions and never exceeds the supplied fuel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrammarPoll {
    pub status: GrammarStatus,
    pub work: usize,
}

/// Resumable limited-subset grammar job.
///
/// The job owns resumable lexical cursors, so it can outlive the short-lived
/// consumer handle without copying candidate events into another token array.
pub struct GrammarJob {
    leaf: SegmentedLeaf,
    table_cursor: Option<LogicalCursor>,
    inline_cursor: Option<LogicalCursor>,
    table_events: Option<LexicalEvents>,
    inline_events: Option<LexicalEvents>,
    table_next: Option<LexicalEvent>,
    inline_next: Option<LexicalEvent>,
    table_event_state: EventFeedState,
    inline_event_state: EventFeedState,
    phase: Phase,
    table_scanner: Option<TableScanner>,
    table_model: Option<TableModel>,
    regions: [InlineRange; MAX_INLINE_REGIONS],
    region_count: usize,
    region_index: usize,
    facts: [Option<EventFact>; MAX_INLINE_EVENTS_PER_REGION],
    fact_count: usize,
    region_fallback: Option<FallbackReason>,
    pending_next_fact: Option<usize>,
    previous_region_byte: Option<u8>,
    body_state: BodyState,
    output: Option<PackedOutputBuilder>,
    result: Option<GrammarOutput>,
    receipt: GrammarWorkReceipt,
    classification: GrammarClassification,
}

impl GrammarJob {
    /// Creates a job only when table and inline consumers prove they share the
    /// exact lexical root produced by one shared lexer.
    ///
    /// # Errors
    ///
    /// Returns an error for mismatched lexical roots or a view lacking its
    /// segmented logical input.
    pub fn new(consumers: &LexicalConsumers) -> Result<Self, GrammarError> {
        let table_view = consumers.table.view();
        let inline_view = consumers.inline.view();
        if !table_view.shares_root_with(inline_view) {
            return Err(GrammarError::LexicalRootsDiffer);
        }
        let leaf = inline_view
            .input()
            .cloned()
            .ok_or(GrammarError::MissingLogicalInput)?;
        let oversized = leaf.len() > MAX_GRAMMAR_LEAF_BYTES;
        let (table_cursor, inline_cursor, table_events, inline_events) = if oversized {
            (None, None, None, None)
        } else {
            (
                Some(leaf.cursor()),
                Some(leaf.cursor()),
                Some(table_view.events()),
                Some(inline_view.events()),
            )
        };
        Ok(Self {
            leaf,
            table_cursor,
            inline_cursor,
            table_events,
            inline_events,
            table_next: None,
            inline_next: None,
            table_event_state: EventFeedState::Available,
            inline_event_state: EventFeedState::Available,
            phase: if oversized {
                Phase::OversizedFallback
            } else {
                Phase::TableScan
            },
            table_scanner: (!oversized).then(TableScanner::new),
            table_model: None,
            regions: [InlineRange::default(); MAX_INLINE_REGIONS],
            region_count: 0,
            region_index: 0,
            facts: [None; MAX_INLINE_EVENTS_PER_REGION],
            fact_count: 0,
            region_fallback: None,
            pending_next_fact: None,
            previous_region_byte: None,
            body_state: BodyState::NotStarted,
            output: Some(PackedOutputBuilder::new()),
            result: None,
            receipt: GrammarWorkReceipt {
                // For normal jobs, two frontier cursors and two event iterators
                // each invoke a private Vec-backed traversal stack.  Their
                // allocator behavior is intentionally not guessed.
                unmetered_upstream_allocation_sites: if oversized { 0 } else { 4 },
                ..GrammarWorkReceipt::default()
            },
            classification: if oversized {
                GrammarClassification::LiteralFallback(FallbackReason::LeafTooLarge)
            } else {
                GrammarClassification::Paragraph
            },
        })
    }

    /// Advances at most `fuel` grammar transitions.
    ///
    /// # Panics
    ///
    /// Panics when fuel is zero or exceeds [`MAX_GRAMMAR_POLL_WORK`].
    pub fn poll(&mut self, fuel: usize) -> GrammarPoll {
        assert!(fuel > 0 && fuel <= MAX_GRAMMAR_POLL_WORK);
        let mut work = 0;
        while work < fuel && self.phase != Phase::Done {
            self.tick();
            work += 1;
            self.receipt.parser_transitions += 1;
        }
        GrammarPoll {
            status: if self.phase == Phase::Done {
                GrammarStatus::Ready
            } else {
                GrammarStatus::Pending
            },
            work,
        }
    }

    fn tick(&mut self) {
        match self.phase {
            Phase::OversizedFallback => self.emit_oversized_fallback(),
            Phase::TableScan => self.scan_table_transition(),
            Phase::PrepareInline => self.prepare_inline(),
            Phase::InlineScan => self.scan_inline_transition(),
            Phase::Finish => self.finish_output(),
            Phase::Done => {}
        }
    }

    fn emit_oversized_fallback(&mut self) {
        let len = self.leaf.len();
        self.push_record(GrammarRecord::ParagraphStart { start: 0, end: len });
        self.push_record(GrammarRecord::LiteralFallback {
            start: 0,
            end: len,
            reason: FallbackReason::LeafTooLarge,
        });
        self.push_text(0, len);
        self.push_record(GrammarRecord::ParagraphEnd);
        self.phase = Phase::Finish;
    }

    fn scan_table_transition(&mut self) {
        let cursor = self
            .table_cursor
            .as_mut()
            .expect("normal grammar job has a table cursor");
        let before = cursor.metrics();
        let step = cursor.step();
        let after = cursor.metrics();
        self.receipt.cursor_transitions += 1;
        let excluded = after.excluded_source_bytes - before.excluded_source_bytes;
        let skipped = after.skipped_source_bytes - before.skipped_source_bytes;
        let index_nodes = after.source_seek_index_nodes - before.source_seek_index_nodes;
        self.receipt.excluded_source_bytes_inspected += excluded;
        self.receipt.source_bytes_inspected += excluded;
        self.receipt.source_bytes_skipped += skipped;
        self.receipt.source_index_nodes_examined += index_nodes;
        self.receipt.index_units += index_nodes;
        match step {
            CursorStep::Progress => {}
            CursorStep::Byte(logical) => {
                self.charge_logical_byte(logical.origin);
                let event = event_at_offset(
                    &mut self.table_events,
                    &mut self.table_next,
                    &mut self.table_event_state,
                    logical.logical_offset,
                    &mut self.receipt,
                );
                let is_pipe = event.is_some_and(|candidate| {
                    candidate.kind == LexicalEventKind::TablePipe
                        && candidate.start.offset == logical.logical_offset
                });
                self.table_scanner
                    .as_mut()
                    .expect("table scan owns a scanner")
                    .observe(logical.logical_offset, logical.byte, is_pipe);
            }
            CursorStep::Done => {
                let scanner = self.table_scanner.take().expect("table scanner exists");
                self.table_model = scanner.finish(self.leaf.len(), &mut self.receipt);
                self.phase = Phase::PrepareInline;
            }
        }
    }

    fn prepare_inline(&mut self) {
        if self.leaf.is_empty() {
            self.classification = GrammarClassification::Empty;
            self.phase = Phase::Finish;
            return;
        }
        if let Some(table) = self.table_model {
            self.regions[..table.region_count]
                .copy_from_slice(&table.regions[..table.region_count]);
            self.region_count = table.region_count;
            self.classification = GrammarClassification::Table {
                columns: table.columns,
                body_rows: usize::from(table.has_body),
            };
            self.push_record(GrammarRecord::TableStart {
                start: table.start,
                end: table.end,
                columns: table.columns,
            });
            self.push_record(GrammarRecord::TableHeadStart);
        } else {
            self.regions[0] = InlineRange {
                start: 0,
                end: self.leaf.len(),
                ..InlineRange::default()
            };
            self.region_count = 1;
            self.classification = GrammarClassification::Paragraph;
            self.push_record(GrammarRecord::ParagraphStart {
                start: 0,
                end: self.leaf.len(),
            });
        }
        self.phase = Phase::InlineScan;
    }

    fn scan_inline_transition(&mut self) {
        if self.region_index == self.region_count {
            self.phase = Phase::Finish;
            return;
        }
        let range = self.regions[self.region_index];
        if range.start == range.end {
            self.resolve_and_emit_region(range);
            self.advance_region();
            return;
        }

        let cursor = self
            .inline_cursor
            .as_mut()
            .expect("normal grammar job has an inline cursor");
        let before = cursor.metrics();
        let step = cursor.step();
        let after = cursor.metrics();
        self.receipt.cursor_transitions += 1;
        let excluded = after.excluded_source_bytes - before.excluded_source_bytes;
        let skipped = after.skipped_source_bytes - before.skipped_source_bytes;
        let index_nodes = after.source_seek_index_nodes - before.source_seek_index_nodes;
        self.receipt.excluded_source_bytes_inspected += excluded;
        self.receipt.source_bytes_inspected += excluded;
        self.receipt.source_bytes_skipped += skipped;
        self.receipt.source_index_nodes_examined += index_nodes;
        self.receipt.index_units += index_nodes;
        match step {
            CursorStep::Progress => {}
            CursorStep::Byte(logical) => {
                self.charge_logical_byte(logical.origin);
                let event = event_at_offset(
                    &mut self.inline_events,
                    &mut self.inline_next,
                    &mut self.inline_event_state,
                    logical.logical_offset,
                    &mut self.receipt,
                );
                if logical.logical_offset >= range.start && logical.logical_offset < range.end {
                    self.observe_inline_byte(range, logical.logical_offset, logical.byte, event);
                    if logical.logical_offset + 1 == range.end {
                        if let Some(index) = self.pending_next_fact.take() {
                            if let Some(fact) = &mut self.facts[index] {
                                fact.next = None;
                            }
                        }
                        self.resolve_and_emit_region(range);
                        self.advance_region();
                    }
                }
            }
            CursorStep::Done => {
                // Validated ranges never extend past EOF. Empty trailing ranges
                // are handled before stepping; reaching EOF here means all work
                // is complete or an upstream invariant was violated.
                if self.region_index < self.region_count {
                    self.resolve_and_emit_region(range);
                    self.advance_region();
                }
            }
        }
    }

    fn observe_inline_byte(
        &mut self,
        range: InlineRange,
        offset: usize,
        byte: u8,
        event: Option<LexicalEvent>,
    ) {
        if let Some(index) = self.pending_next_fact {
            if self.facts[index].is_some_and(|fact| fact.event.end == offset) {
                if let Some(fact) = &mut self.facts[index] {
                    fact.next = Some(byte);
                }
                self.pending_next_fact = None;
            }
        }
        if let Some(event) = event {
            if event.start.offset >= range.start && event.end <= range.end {
                if self.fact_count == MAX_INLINE_EVENTS_PER_REGION {
                    self.region_fallback = Some(FallbackReason::TooManyInlineEvents);
                } else {
                    let fact = EventFact {
                        event,
                        previous: self.previous_region_byte,
                        next: None,
                        role: FactRole::None,
                    };
                    if matches!(
                        event.kind,
                        LexicalEventKind::EmphasisRun { len, .. } if len > 2
                    ) {
                        self.region_fallback = Some(FallbackReason::UnsupportedEmphasisRun);
                    }
                    self.facts[self.fact_count] = Some(fact);
                    self.pending_next_fact = Some(self.fact_count);
                    self.fact_count += 1;
                }
            }
        }
        self.previous_region_byte = Some(byte);
    }

    fn resolve_and_emit_region(&mut self, range: InlineRange) {
        let fallback = self.region_fallback;

        let mut atomic_units = 0;
        if fallback.is_none() {
            self.resolve_code_spans(&mut atomic_units);
            let crossing = self.resolve_emphasis(&mut atomic_units);
            if crossing || self.has_unsupported_emphasis_interaction(&mut atomic_units) {
                self.region_fallback = Some(FallbackReason::UnsupportedEmphasisInteraction);
            } else {
                self.mark_escapes(&mut atomic_units);
            }
        }
        let fallback = fallback.or(self.region_fallback);
        debug_assert!(
            atomic_units <= MAX_ATOMIC_REGION_INDEX_UNITS,
            "bounded local resolver exceeded its documented ceiling"
        );
        self.receipt.index_units += atomic_units;
        self.receipt.max_atomic_index_units = self.receipt.max_atomic_index_units.max(atomic_units);

        self.emit_region_start(range);
        if let Some(reason) = fallback {
            self.push_record(GrammarRecord::LiteralFallback {
                start: range.start,
                end: range.end,
                reason,
            });
            self.push_text(range.start, range.end);
        } else {
            self.emit_resolved_content(range);
        }
        self.emit_region_end(range);
    }

    fn resolve_code_spans(&mut self, units: &mut usize) {
        let mut index = 0;
        while index < self.fact_count {
            *units += 1;
            let fact = self.fact(index);
            let LexicalEventKind::BacktickRun { len } = fact.event.kind else {
                index += 1;
                continue;
            };
            let mut closer = index + 1;
            while closer < self.fact_count {
                *units += 1;
                if matches!(
                    self.fact(closer).event.kind,
                    LexicalEventKind::BacktickRun { len: candidate } if candidate == len
                ) {
                    break;
                }
                closer += 1;
            }
            if closer == self.fact_count {
                index += 1;
                continue;
            }
            self.fact_mut(index).role = FactRole::CodeOpen { closer };
            self.fact_mut(closer).role = FactRole::CodeClose;
            for suppressed in index + 1..closer {
                *units += 1;
                self.fact_mut(suppressed).role = FactRole::Suppressed;
            }
            index = closer + 1;
        }
    }

    fn resolve_emphasis(&mut self, units: &mut usize) -> bool {
        let mut openers = [0usize; MAX_INLINE_EVENTS_PER_REGION];
        let mut opener_count = 0;
        let mut crossing = false;
        for index in 0..self.fact_count {
            *units += 1;
            let fact = self.fact(index);
            if fact.role != FactRole::None {
                continue;
            }
            let LexicalEventKind::EmphasisRun { marker, len } = fact.event.kind else {
                continue;
            };
            debug_assert!(matches!(len, 1 | 2));
            let (can_open, can_close) = delimiter_flanking(marker, fact.previous, fact.next);
            let mut matched = None;
            if can_close {
                for stack_index in (0..opener_count).rev() {
                    *units += 1;
                    let opener = self.fact(openers[stack_index]);
                    if matches!(
                        opener.event.kind,
                        LexicalEventKind::EmphasisRun {
                            marker: opener_marker,
                            len: opener_len,
                        } if opener_marker == marker && opener_len == len
                    ) {
                        matched = Some(stack_index);
                        break;
                    }
                }
            }
            if let Some(stack_index) = matched {
                let opener_index = openers[stack_index];
                let strong = len == 2;
                self.fact_mut(opener_index).role = FactRole::EmphasisOpen { strong };
                self.fact_mut(index).role = FactRole::EmphasisClose { strong };
                // Discard crossing openers above the match.
                crossing |= opener_count > stack_index + 1;
                opener_count = stack_index;
            } else if can_open {
                openers[opener_count] = index;
                opener_count += 1;
                *units += 1;
            }
        }
        crossing
    }

    fn has_unsupported_emphasis_interaction(&self, units: &mut usize) -> bool {
        for left_index in 0..self.fact_count {
            let left = self.fact(left_index);
            if left.role != FactRole::None {
                continue;
            }
            let LexicalEventKind::EmphasisRun {
                marker: left_marker,
                len: left_len,
            } = left.event.kind
            else {
                continue;
            };
            let (left_opens, _) = delimiter_flanking(left_marker, left.previous, left.next);
            if !left_opens {
                continue;
            }
            for right_index in left_index + 1..self.fact_count {
                *units += 1;
                let right = self.fact(right_index);
                if right.role != FactRole::None {
                    continue;
                }
                let LexicalEventKind::EmphasisRun {
                    marker: right_marker,
                    len: right_len,
                } = right.event.kind
                else {
                    continue;
                };
                let (_, right_closes) =
                    delimiter_flanking(right_marker, right.previous, right.next);
                if left_marker == right_marker && left_len != right_len && right_closes {
                    return true;
                }
            }
        }
        false
    }

    fn mark_escapes(&mut self, units: &mut usize) {
        for index in 0..self.fact_count {
            *units += 1;
            let fact = self.fact(index);
            if fact.role == FactRole::None
                && matches!(fact.event.kind, LexicalEventKind::BackslashEscape { .. })
            {
                self.fact_mut(index).role = FactRole::Escape;
            }
        }
    }

    fn emit_region_start(&mut self, range: InlineRange) {
        if self.table_model.is_none() {
            return;
        }
        if !range.header && self.body_state == BodyState::NotStarted {
            self.push_record(GrammarRecord::TableHeadEnd);
            self.push_record(GrammarRecord::TableRowStart);
            self.body_state = BodyState::Started;
        }
        self.push_record(GrammarRecord::TableCellStart {
            start: range.start,
            end: range.end,
            column: range.column,
            alignment: range.alignment,
            header: range.header,
        });
    }

    fn emit_region_end(&mut self, range: InlineRange) {
        if self.table_model.is_some() {
            self.push_record(GrammarRecord::TableCellEnd);
            if self.region_index + 1 == self.region_count {
                if range.header {
                    self.push_record(GrammarRecord::TableHeadEnd);
                } else {
                    self.push_record(GrammarRecord::TableRowEnd);
                }
                self.push_record(GrammarRecord::TableEnd);
            }
        } else {
            self.push_record(GrammarRecord::ParagraphEnd);
        }
    }

    fn emit_resolved_content(&mut self, range: InlineRange) {
        let mut raw_cursor = range.start;
        let mut index = 0;
        while index < self.fact_count {
            let fact = self.fact(index);
            if fact.event.start.offset < raw_cursor {
                index += 1;
                continue;
            }
            match fact.role {
                FactRole::CodeOpen { closer } => {
                    self.push_text(raw_cursor, fact.event.start.offset);
                    let close = self.fact(closer);
                    let LexicalEventKind::BacktickRun { len: delimiter_len } = fact.event.kind
                    else {
                        unreachable!("code opener is a backtick run")
                    };
                    self.push_record(GrammarRecord::Code {
                        start: fact.event.start.offset,
                        end: close.event.end,
                        content_start: fact.event.end,
                        content_end: close.event.start.offset,
                        delimiter_len,
                        table_cell: self.table_model.is_some(),
                    });
                    raw_cursor = close.event.end;
                    index = closer + 1;
                }
                FactRole::EmphasisOpen { strong } => {
                    self.push_text(raw_cursor, fact.event.start.offset);
                    self.push_record(if strong {
                        GrammarRecord::StrongStart {
                            start: fact.event.start.offset,
                            end: fact.event.end,
                        }
                    } else {
                        GrammarRecord::EmphasisStart {
                            start: fact.event.start.offset,
                            end: fact.event.end,
                        }
                    });
                    raw_cursor = fact.event.end;
                    index += 1;
                }
                FactRole::EmphasisClose { strong } => {
                    self.push_text(raw_cursor, fact.event.start.offset);
                    self.push_record(if strong {
                        GrammarRecord::StrongEnd {
                            start: fact.event.start.offset,
                            end: fact.event.end,
                        }
                    } else {
                        GrammarRecord::EmphasisEnd {
                            start: fact.event.start.offset,
                            end: fact.event.end,
                        }
                    });
                    raw_cursor = fact.event.end;
                    index += 1;
                }
                FactRole::Escape => {
                    self.push_text(raw_cursor, fact.event.start.offset);
                    let LexicalEventKind::BackslashEscape { escaped } = fact.event.kind else {
                        unreachable!("escape role is a backslash event")
                    };
                    self.push_record(GrammarRecord::Escaped {
                        start: fact.event.start.offset,
                        end: fact.event.end,
                        byte: escaped,
                    });
                    raw_cursor = fact.event.end;
                    index += 1;
                }
                FactRole::None | FactRole::CodeClose | FactRole::Suppressed => index += 1,
            }
        }
        self.push_text(raw_cursor, range.end);
    }

    fn fact(&self, index: usize) -> EventFact {
        self.facts[index].expect("fact index is initialized")
    }

    fn fact_mut(&mut self, index: usize) -> &mut EventFact {
        self.facts[index]
            .as_mut()
            .expect("fact index is initialized")
    }

    fn advance_region(&mut self) {
        for fact in &mut self.facts[..self.fact_count] {
            *fact = None;
        }
        self.fact_count = 0;
        self.region_fallback = None;
        self.pending_next_fact = None;
        self.previous_region_byte = None;
        self.region_index += 1;
    }

    fn charge_logical_byte(&mut self, origin: LogicalOrigin) {
        self.receipt.logical_bytes_inspected += 1;
        match origin {
            LogicalOrigin::Source(_) => self.receipt.source_bytes_inspected += 1,
            LogicalOrigin::Virtual { .. } => self.receipt.virtual_bytes_inspected += 1,
        }
    }

    fn push_text(&mut self, start: usize, end: usize) {
        if start < end {
            self.push_record(GrammarRecord::Text { start, end });
        }
    }

    fn push_record(&mut self, record: GrammarRecord) {
        self.output
            .as_mut()
            .expect("output builder exists before finish")
            .push(record);
    }

    fn finish_output(&mut self) {
        let builder = self.output.take().expect("finish owns output builder");
        let (records, work) = builder.finish();
        self.receipt.grammar_allocation_units = work.allocation_units;
        self.receipt.copy_units = work.copy_units;
        self.receipt.encode_units = records.payload_bytes();
        self.receipt.hash_units = work.hash_units;
        self.receipt.index_units += work.index_units;
        self.receipt.output_payload_bytes = records.payload_bytes();
        self.receipt.output_records = records.record_count();
        assert!(
            !work.capacity_exceeded,
            "accepted grammar bounds violated the compile-time output-capacity proof"
        );
        self.result = Some(GrammarOutput {
            records,
            classification: self.classification,
            receipt: self.receipt,
            input: self.leaf.clone(),
        });
        self.phase = Phase::Done;
    }

    /// Returns the immutable result after a ready poll.
    #[must_use]
    pub const fn result(&self) -> Option<&GrammarOutput> {
        self.result.as_ref()
    }
}

fn event_at_offset(
    events: &mut Option<LexicalEvents>,
    next: &mut Option<LexicalEvent>,
    state: &mut EventFeedState,
    offset: usize,
    receipt: &mut GrammarWorkReceipt,
) -> Option<LexicalEvent> {
    loop {
        if next.is_none() && *state == EventFeedState::Available {
            *next = events.as_mut().and_then(Iterator::next);
            if next.is_some() {
                receipt.lexical_events_examined += 1;
            } else {
                *state = EventFeedState::Exhausted;
            }
        }
        let candidate = (*next)?;
        receipt.index_units += 1;
        if candidate.start.offset < offset {
            *next = None;
            continue;
        }
        if candidate.start.offset == offset {
            *next = None;
            return Some(candidate);
        }
        return None;
    }
}

fn delimiter_flanking(marker: u8, previous: Option<u8>, next: Option<u8>) -> (bool, bool) {
    let previous_whitespace = previous.is_none_or(|byte| byte.is_ascii_whitespace());
    let next_whitespace = next.is_none_or(|byte| byte.is_ascii_whitespace());
    let previous_punctuation = previous.is_some_and(|byte| byte.is_ascii_punctuation());
    let next_punctuation = next.is_some_and(|byte| byte.is_ascii_punctuation());

    let left_flanking =
        !next_whitespace && (!next_punctuation || previous_whitespace || previous_punctuation);
    let right_flanking =
        !previous_whitespace && (!previous_punctuation || next_whitespace || next_punctuation);
    if marker == b'_' {
        (
            left_flanking && (!right_flanking || previous_punctuation),
            right_flanking && (!left_flanking || next_punctuation),
        )
    } else {
        (left_flanking, right_flanking)
    }
}

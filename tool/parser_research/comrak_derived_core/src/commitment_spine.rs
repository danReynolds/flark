//! Exact-under-cap commitment spine using Comrak's donor scanners.
//!
//! This is the maintainable alternative to translating generated scanner
//! DFAs into a second parser. Runtime state, source ranges, restart semantics,
//! and outputs are Flark-owned. Lexical classification delegates to the pinned
//! Comrak facade. Every delegated slice is capped; oversized physical lines or
//! paragraphs become explicit source-visible regions.

use std::ops::Range;
use std::sync::Arc;

use comrak::block_spine_facade::{
    self as donor, FacadeAlignment, FacadeReferenceDefinition, FacadeTableRow,
    MAX_CLASSIFICATION_BYTES,
};
use comrak::ResolvedReference;
use im::OrdMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitmentStatus {
    Yielded,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommitmentReport {
    pub status: CommitmentStatus,
    pub work_units: usize,
    pub source_bytes_inspected: usize,
    pub completed_lines: usize,
    pub maximum_atomic_classification_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpaqueReason {
    PhysicalLineOverCap,
    ParagraphOverCap,
    TableAutocompletionCap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

impl From<FacadeAlignment> for Alignment {
    fn from(value: FacadeAlignment) -> Self {
        match value {
            FacadeAlignment::None => Self::None,
            FacadeAlignment::Left => Self::Left,
            FacadeAlignment::Center => Self::Center,
            FacadeAlignment::Right => Self::Right,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellDescriptor {
    pub source: Range<usize>,
    pub internal_offset: usize,
    pub escaped_pipe: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitmentFact {
    List {
        id: u64,
        source: Range<usize>,
        depth: usize,
        tight: bool,
    },
    TableStart {
        id: u64,
        source: Range<usize>,
        alignments: Arc<[Alignment]>,
        header: Arc<[CellDescriptor]>,
    },
    TableRow {
        table_id: u64,
        source: Range<usize>,
        cells: Arc<[CellDescriptor]>,
        autocompleted: usize,
    },
    HtmlBlock {
        id: u64,
        block_type: u8,
        source: Range<usize>,
    },
    ReferenceDefinition {
        generation: u64,
        normalized_label: Arc<str>,
        source: Range<usize>,
        label_source: Range<usize>,
        url_source: Range<usize>,
        title_source: Option<Range<usize>>,
        first_definition: bool,
    },
    InlineDependency {
        source: Range<usize>,
        symbol_generation: u64,
    },
    Opaque {
        source: Range<usize>,
        reason: OpaqueReason,
    },
}

#[derive(Clone, Debug)]
pub struct ReferenceSymbol {
    pub generation: u64,
    pub normalized_label: Arc<str>,
    pub source: Range<usize>,
    pub url_source: Range<usize>,
    pub title_source: Option<Range<usize>>,
    pub url: Arc<str>,
    pub title: Arc<str>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ItemAggregate {
    has_child: bool,
    any_child_ends_blank: bool,
    previous_child_ends_blank: bool,
    loose_between_children: bool,
    last_line_blank: bool,
}

impl ItemAggregate {
    fn add_nonblank_line(&mut self) {
        if self.last_line_blank && self.has_child {
            self.loose_between_children = true;
            self.previous_child_ends_blank = true;
            self.any_child_ends_blank = true;
        }
        self.has_child = true;
        self.last_line_blank = false;
    }

    fn add_blank_line(&mut self) {
        if self.has_child {
            self.last_line_blank = true;
            self.previous_child_ends_blank = true;
            self.any_child_ends_blank = true;
        }
    }

    fn child_list_closed(&mut self, ends_blank: bool) {
        if self.previous_child_ends_blank {
            self.loose_between_children = true;
        }
        self.has_child = true;
        self.previous_child_ends_blank = ends_blank;
        self.any_child_ends_blank |= ends_blank;
        self.last_line_blank = ends_blank;
    }

    fn makes_list_loose(self, has_next_item: bool) -> bool {
        self.loose_between_children
            || (has_next_item && (self.last_line_blank || self.any_child_ends_blank))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ListFrame {
    id: u64,
    marker_indent: usize,
    content_indent: usize,
    marker: u8,
    start: usize,
    end: usize,
    depth: usize,
    loose_closed_prefix: bool,
    item: ItemAggregate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TableState {
    id: u64,
    start: usize,
    end: usize,
    alignments: Arc<[Alignment]>,
    columns: usize,
    rows: usize,
    nonempty_cells: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HtmlState {
    id: u64,
    block_type: u8,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingParagraph {
    source: Range<usize>,
    last_line: Range<usize>,
}

#[derive(Clone, Debug)]
pub struct CommitmentRestartState {
    lists: Arc<Vec<ListFrame>>,
    table: Option<TableState>,
    html: Option<HtmlState>,
    paragraph: Option<PendingParagraph>,
    symbols: OrdMap<Arc<str>, ReferenceSymbol>,
    symbol_generation: u64,
}

impl Default for CommitmentRestartState {
    fn default() -> Self {
        Self {
            lists: Arc::new(Vec::new()),
            table: None,
            html: None,
            paragraph: None,
            symbols: OrdMap::new(),
            symbol_generation: 0,
        }
    }
}

impl CommitmentRestartState {
    pub fn symbol_generation(&self) -> u64 {
        self.symbol_generation
    }

    pub fn symbols(&self) -> impl Iterator<Item = (&Arc<str>, &ReferenceSymbol)> {
        self.symbols.iter()
    }

    pub fn semantic_eq(&self, other: &Self) -> bool {
        self.lists.as_ref() == other.lists.as_ref()
            && self.table == other.table
            && self.html == other.html
            && self.paragraph == other.paragraph
            && self.symbol_generation == other.symbol_generation
            && self.symbols.len() == other.symbols.len()
            && self.symbols.iter().all(|(label, left)| {
                other.symbols.get(label).is_some_and(|right| {
                    left.generation == right.generation
                        && left.source == right.source
                        && left.url_source == right.url_source
                        && left.title_source == right.title_source
                        && left.url == right.url
                        && left.title == right.title
                })
            })
    }
}

#[derive(Clone, Debug)]
pub struct CommitmentCheckpoint {
    pub offset: usize,
    pub line_number: usize,
    pub state: CommitmentRestartState,
    next_id: u64,
}

#[derive(Clone, Debug)]
struct LineWork {
    start: usize,
    cursor: usize,
    saw_cr: bool,
}

impl LineWork {
    fn new(start: usize) -> Self {
        Self {
            start,
            cursor: start,
            saw_cr: false,
        }
    }
}

pub struct CommitmentSpine {
    source: Arc<str>,
    offset: usize,
    line_number: usize,
    state: CommitmentRestartState,
    line: Option<LineWork>,
    facts: Vec<CommitmentFact>,
    next_id: u64,
    maximum_atomic_classification_bytes: usize,
}

impl CommitmentSpine {
    pub fn new(source: impl Into<Arc<str>>) -> Self {
        Self {
            source: source.into(),
            offset: 0,
            line_number: 0,
            state: CommitmentRestartState::default(),
            line: None,
            facts: Vec::new(),
            next_id: 1,
            maximum_atomic_classification_bytes: 0,
        }
    }

    pub fn resume(source: impl Into<Arc<str>>, checkpoint: CommitmentCheckpoint) -> Self {
        let source = source.into();
        assert!(checkpoint.offset <= source.len());
        assert!(source.is_char_boundary(checkpoint.offset));
        Self {
            source,
            offset: checkpoint.offset,
            line_number: checkpoint.line_number,
            state: checkpoint.state,
            line: None,
            facts: Vec::new(),
            next_id: checkpoint.next_id,
            maximum_atomic_classification_bytes: 0,
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn state(&self) -> &CommitmentRestartState {
        &self.state
    }

    pub fn facts(&self) -> &[CommitmentFact] {
        &self.facts
    }

    pub fn take_facts(&mut self) -> Vec<CommitmentFact> {
        std::mem::take(&mut self.facts)
    }

    pub fn checkpoint(&self) -> Option<CommitmentCheckpoint> {
        self.line.is_none().then(|| CommitmentCheckpoint {
            offset: self.offset,
            line_number: self.line_number,
            state: self.state.clone(),
            next_id: self.next_id,
        })
    }

    pub fn is_complete(&self) -> bool {
        self.offset == self.source.len() && self.line.is_none()
    }

    pub fn maximum_atomic_classification_bytes(&self) -> usize {
        self.maximum_atomic_classification_bytes
    }

    pub fn advance(&mut self, fuel: usize) -> CommitmentReport {
        let mut report = CommitmentReport {
            status: CommitmentStatus::Yielded,
            work_units: 0,
            source_bytes_inspected: 0,
            completed_lines: 0,
            maximum_atomic_classification_bytes: self.maximum_atomic_classification_bytes,
        };

        while report.work_units < fuel && !self.is_complete() {
            if self.line.is_none() {
                self.line = Some(LineWork::new(self.offset));
            }
            let line = self.line.as_mut().expect("line initialized");
            let bytes = self.source.as_bytes();
            let byte = bytes.get(line.cursor).copied();
            report.work_units += 1;
            if byte.is_some() {
                report.source_bytes_inspected += 1;
            }
            match byte {
                Some(b'\n') => {
                    line.cursor += 1;
                    let finished = self.line.take().expect("finished line");
                    self.publish_line(finished.start..finished.cursor);
                    report.completed_lines += 1;
                }
                Some(b'\r') => {
                    line.cursor += 1;
                    line.saw_cr = true;
                }
                Some(_) if line.saw_cr => {
                    let finished = self.line.take().expect("finished CR line");
                    self.publish_line(finished.start..finished.cursor - 1);
                    report.completed_lines += 1;
                }
                Some(_) => line.cursor += 1,
                None => {
                    let finished = self.line.take().expect("finished final line");
                    self.publish_line(finished.start..finished.cursor);
                    report.completed_lines += 1;
                }
            }
        }
        report.maximum_atomic_classification_bytes = self.maximum_atomic_classification_bytes;
        if self.is_complete() {
            self.finish_document();
            report.status = CommitmentStatus::Complete;
        }
        report
    }

    pub fn materialize_reference_snapshot(&self) -> Vec<(String, String, String)> {
        self.state
            .symbols
            .iter()
            .map(|(label, symbol)| {
                (
                    label.to_string(),
                    symbol.url.to_string(),
                    symbol.title.to_string(),
                )
            })
            .collect()
    }

    fn publish_line(&mut self, line: Range<usize>) {
        self.offset = line.end;
        self.line_number += 1;
        let source = self.source.clone();
        let content = trim_line_ending(&source[line.clone()]);
        let content_end = line.start + content.len();
        let content_range = line.start..content_end;

        if content.len() > MAX_CLASSIFICATION_BYTES {
            self.close_html(content_range.start);
            self.close_table(content_range.start);
            self.close_paragraph(content_range.start);
            self.close_all_lists(content_range.start);
            self.facts.push(CommitmentFact::Opaque {
                source: line,
                reason: OpaqueReason::PhysicalLineOverCap,
            });
            return;
        }
        self.maximum_atomic_classification_bytes =
            self.maximum_atomic_classification_bytes.max(content.len());
        let blank = content
            .as_bytes()
            .iter()
            .all(|byte| matches!(byte, b' ' | b'\t'));

        if let Some(html) = self.state.html.clone() {
            if matches!(html.block_type, 6 | 7) && blank {
                self.close_html(content_range.start);
            } else {
                let first = first_nonspace(content);
                let tail = &content[first..];
                let ended = donor::html_block_end(html.block_type, tail)
                    .expect("bounded HTML classification");
                let state = self.state.html.as_mut().expect("open HTML");
                state.end = line.end;
                if ended {
                    self.close_html(line.end);
                }
                return;
            }
        }

        if blank {
            self.close_table(content_range.start);
            self.close_paragraph(content_range.start);
            self.note_list_blank(line.end);
            return;
        }

        if let Some(table) = self.state.table.clone() {
            let first = first_nonspace(content);
            let logical = &content[first..];
            if let Some(row) = donor::table_row(logical, false).expect("bounded table row") {
                self.emit_table_row(table, line.clone(), first, row);
                return;
            }
            self.close_table(content_range.start);
        }

        let first = first_nonspace(content);
        let logical = &content[first..];
        let allow_type_7 = self.state.paragraph.is_none();
        if let Some(block_type) =
            donor::html_block_start(logical, allow_type_7).expect("bounded HTML start")
        {
            self.close_paragraph(content_range.start);
            self.close_all_lists(content_range.start);
            let id = self.take_id();
            self.state.html = Some(HtmlState {
                id,
                block_type,
                start: line.start + first,
                end: line.end,
            });
            if donor::html_block_end(block_type, logical).expect("bounded HTML end") {
                self.close_html(line.end);
            }
            return;
        }

        if self.try_activate_table(line.clone(), first, logical) {
            return;
        }

        let paragraph_start = self.process_list_line(line.clone(), content, first);
        self.extend_paragraph(line, paragraph_start);
    }

    fn try_activate_table(&mut self, line: Range<usize>, first: usize, logical: &str) -> bool {
        let Some(alignments) =
            donor::table_delimiter_alignments(logical, false).expect("bounded table delimiter")
        else {
            return false;
        };
        let Some(paragraph) = self.state.paragraph.clone() else {
            return false;
        };
        let source = self.source.clone();
        let header_text = trim_line_ending(&source[paragraph.last_line.clone()]);
        let header_first = first_nonspace(header_text);
        let Some(header) =
            donor::table_row(&header_text[header_first..], false).expect("bounded table header")
        else {
            return false;
        };
        if header.cells.len() != alignments.len() {
            return false;
        }

        if paragraph.source.start < paragraph.last_line.start {
            let prefix = paragraph.source.start..paragraph.last_line.start;
            self.facts.push(CommitmentFact::InlineDependency {
                source: prefix,
                symbol_generation: self.state.symbol_generation,
            });
        }
        self.state.paragraph = None;
        let id = self.take_id();
        let alignments: Arc<[Alignment]> = alignments
            .into_iter()
            .map(Alignment::from)
            .collect::<Vec<_>>()
            .into();
        let header_cells = descriptors(&header, paragraph.last_line.start + header_first);
        self.facts.push(CommitmentFact::TableStart {
            id,
            source: paragraph.last_line.start..line.end,
            alignments: alignments.clone(),
            header: header_cells,
        });
        self.state.table = Some(TableState {
            id,
            start: paragraph.last_line.start,
            end: line.end,
            columns: alignments.len(),
            alignments,
            rows: 1,
            nonempty_cells: header.cells.len(),
        });
        self.close_all_lists(line.start + first);
        true
    }

    fn emit_table_row(
        &mut self,
        mut table: TableState,
        line: Range<usize>,
        first: usize,
        row: FacadeTableRow,
    ) {
        let populated = row.cells.len().min(table.columns);
        let autocompleted = table.columns.saturating_sub(populated);
        let projected = table
            .columns
            .saturating_mul(table.rows.saturating_add(1))
            .saturating_sub(table.nonempty_cells.saturating_add(populated));
        if projected > 500_000 {
            self.close_table(line.start);
            self.facts.push(CommitmentFact::Opaque {
                source: line,
                reason: OpaqueReason::TableAutocompletionCap,
            });
            return;
        }
        let cells = descriptors(&row, line.start + first);
        self.facts.push(CommitmentFact::TableRow {
            table_id: table.id,
            source: line.clone(),
            cells,
            autocompleted,
        });
        table.end = line.end;
        table.rows += 1;
        table.nonempty_cells += populated;
        self.state.table = Some(table);
    }

    fn extend_paragraph(&mut self, line: Range<usize>, nonspace_start: usize) {
        if let Some(paragraph) = &mut self.state.paragraph {
            paragraph.source.end = line.end;
            paragraph.last_line = nonspace_start..line.end;
        } else {
            self.state.paragraph = Some(PendingParagraph {
                source: nonspace_start..line.end,
                last_line: nonspace_start..line.end,
            });
        }
    }

    fn close_paragraph(&mut self, _at: usize) {
        let Some(paragraph) = self.state.paragraph.take() else {
            return;
        };
        let source = self.source.clone();
        let logical = &source[paragraph.source.clone()];
        if logical.len() > MAX_CLASSIFICATION_BYTES {
            self.facts.push(CommitmentFact::Opaque {
                source: paragraph.source,
                reason: OpaqueReason::ParagraphOverCap,
            });
            return;
        }
        self.maximum_atomic_classification_bytes =
            self.maximum_atomic_classification_bytes.max(logical.len());
        let definitions =
            donor::reference_definitions(logical).expect("bounded reference paragraph");
        let mut consumed = 0;
        for definition in definitions {
            consumed = consumed.max(definition.source.end);
            self.insert_reference(paragraph.source.start, definition);
        }
        let remainder_start = paragraph.source.start + consumed;
        if remainder_start < paragraph.source.end
            && !self.source[remainder_start..paragraph.source.end]
                .as_bytes()
                .iter()
                .all(|byte| byte.is_ascii_whitespace())
        {
            self.facts.push(CommitmentFact::InlineDependency {
                source: remainder_start..paragraph.source.end,
                symbol_generation: self.state.symbol_generation,
            });
        }
    }

    fn insert_reference(&mut self, base: usize, definition: FacadeReferenceDefinition) {
        let label: Arc<str> = Arc::from(definition.normalized_label);
        let first = !self.state.symbols.contains_key(&label);
        let generation = if first {
            self.state.symbol_generation += 1;
            self.state.symbol_generation
        } else {
            self.state.symbol_generation
        };
        let source = offset_range(base, definition.source);
        let label_source = offset_range(base, definition.label_source);
        let url_source = offset_range(base, definition.url_source);
        let title_source = definition
            .title_source
            .map(|range| offset_range(base, range));
        if first {
            let ResolvedReference { url, title } = definition.resolved;
            self.state.symbols.insert(
                label.clone(),
                ReferenceSymbol {
                    generation,
                    normalized_label: label.clone(),
                    source: source.clone(),
                    url_source: url_source.clone(),
                    title_source: title_source.clone(),
                    url: Arc::from(url),
                    title: Arc::from(title),
                },
            );
        }
        self.facts.push(CommitmentFact::ReferenceDefinition {
            generation,
            normalized_label: label,
            source,
            label_source,
            url_source,
            title_source,
            first_definition: first,
        });
    }

    fn process_list_line(&mut self, line: Range<usize>, content: &str, first: usize) -> usize {
        let maximum_indent = self
            .state
            .lists
            .last()
            .map_or(3, |frame| frame.content_indent.saturating_add(3));
        let marker = parse_list_marker(content, first, maximum_indent)
            .filter(|marker| {
                !self.state.lists.is_empty()
                    || self.state.paragraph.is_none()
                    || marker.interrupts_paragraph
            })
            .map(|mut marker| {
                marker.absolute_start += line.start;
                marker
            });
        if let Some(marker) = marker {
            self.close_paragraph(marker.absolute_start);
            let paragraph_start = (line.start + marker.content_indent).min(line.end);
            let lists = Arc::make_mut(&mut self.state.lists);
            while lists
                .last()
                .is_some_and(|frame| marker.indent < frame.marker_indent)
            {
                close_last_list(lists, &mut self.facts, marker.absolute_start, false);
            }
            let sibling = lists.last().is_some_and(|frame| {
                marker.indent == frame.marker_indent && marker.marker == frame.marker
            });
            if sibling {
                let frame = lists.last_mut().expect("sibling list");
                frame.loose_closed_prefix |= frame.item.makes_list_loose(true);
                frame.item = ItemAggregate::default();
                frame.item.add_nonblank_line();
                frame.end = line.end;
                return paragraph_start;
            }
            while lists
                .last()
                .is_some_and(|frame| marker.indent <= frame.marker_indent)
            {
                close_last_list(lists, &mut self.facts, marker.absolute_start, false);
            }
            let depth = lists.len() + 1;
            let mut item = ItemAggregate::default();
            item.add_nonblank_line();
            let id = self.next_id;
            self.next_id += 1;
            lists.push(ListFrame {
                id,
                marker_indent: marker.indent,
                content_indent: marker.content_indent,
                marker: marker.marker,
                start: marker.absolute_start,
                end: line.end,
                depth,
                loose_closed_prefix: false,
                item,
            });
            return paragraph_start;
        }
        let lists = Arc::make_mut(&mut self.state.lists);
        if let Some(frame) = lists.last_mut() {
            if first >= frame.content_indent {
                frame.item.add_nonblank_line();
                frame.end = line.end;
            } else {
                close_all_lists(lists, &mut self.facts, line.start + first);
            }
        }
        if first >= 4 && lists.is_empty() {
            line.start
        } else {
            line.start + first
        }
    }

    fn note_list_blank(&mut self, end: usize) {
        if let Some(frame) = Arc::make_mut(&mut self.state.lists).last_mut() {
            frame.item.add_blank_line();
            frame.end = end;
        }
    }

    fn close_all_lists(&mut self, at: usize) {
        close_all_lists(Arc::make_mut(&mut self.state.lists), &mut self.facts, at);
    }

    fn close_table(&mut self, _at: usize) {
        self.state.table = None;
    }

    fn close_html(&mut self, _at: usize) {
        if let Some(html) = self.state.html.take() {
            self.facts.push(CommitmentFact::HtmlBlock {
                id: html.id,
                block_type: html.block_type,
                source: html.start..html.end,
            });
        }
    }

    fn finish_document(&mut self) {
        if self.line.is_some() {
            return;
        }
        self.close_html(self.offset);
        self.close_table(self.offset);
        self.close_paragraph(self.offset);
        self.close_all_lists(self.offset);
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("commitment id exhaustion");
        id
    }
}

fn descriptors(row: &FacadeTableRow, base: usize) -> Arc<[CellDescriptor]> {
    row.cells
        .iter()
        .map(|cell| CellDescriptor {
            source: offset_range(base, cell.source.clone()),
            internal_offset: cell.internal_offset,
            escaped_pipe: cell.had_escaped_pipe,
        })
        .collect::<Vec<_>>()
        .into()
}

fn offset_range(base: usize, range: Range<usize>) -> Range<usize> {
    base + range.start..base + range.end
}

fn trim_line_ending(line: &str) -> &str {
    line.strip_suffix("\r\n")
        .or_else(|| line.strip_suffix('\n'))
        .or_else(|| line.strip_suffix('\r'))
        .unwrap_or(line)
}

fn first_nonspace(line: &str) -> usize {
    line.as_bytes()
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\t'))
        .unwrap_or(line.len())
}

#[derive(Clone, Copy, Debug)]
struct ParsedMarker {
    indent: usize,
    content_indent: usize,
    marker: u8,
    absolute_start: usize,
    interrupts_paragraph: bool,
}

fn parse_list_marker(line: &str, first: usize, maximum_indent: usize) -> Option<ParsedMarker> {
    if first > maximum_indent {
        return None;
    }
    let bytes = line.as_bytes();
    let marker = *bytes.get(first)?;
    let (marker_end, normalized_marker, interrupts_paragraph) =
        if matches!(marker, b'-' | b'+' | b'*') {
            (first + 1, marker, true)
        } else if marker.is_ascii_digit() {
            let mut cursor = first;
            let mut value = 0usize;
            while cursor < bytes.len() && bytes[cursor].is_ascii_digit() && cursor - first < 9 {
                value = value * 10 + usize::from(bytes[cursor] - b'0');
                cursor += 1;
            }
            let delimiter = *bytes.get(cursor)?;
            if !matches!(delimiter, b'.' | b')') {
                return None;
            }
            (cursor + 1, delimiter, value == 1)
        } else {
            return None;
        };
    if !matches!(bytes.get(marker_end), None | Some(b' ' | b'\t')) {
        return None;
    }
    let mut content = marker_end;
    let mut padding = 0;
    while padding < 4 && matches!(bytes.get(content), Some(b' ')) {
        padding += 1;
        content += 1;
    }
    if padding == 0 {
        padding = 1;
    }
    Some(ParsedMarker {
        indent: first,
        content_indent: marker_end + padding,
        marker: normalized_marker,
        absolute_start: first,
        interrupts_paragraph: interrupts_paragraph && content < bytes.len(),
    })
}

fn close_last_list(
    lists: &mut Vec<ListFrame>,
    facts: &mut Vec<CommitmentFact>,
    _at: usize,
    parent_ends_blank: bool,
) {
    let Some(frame) = lists.pop() else {
        return;
    };
    let tight = !(frame.loose_closed_prefix || frame.item.makes_list_loose(false));
    facts.push(CommitmentFact::List {
        id: frame.id,
        source: frame.start..frame.end,
        depth: frame.depth,
        tight,
    });
    if let Some(parent) = lists.last_mut() {
        parent
            .item
            .child_list_closed(parent_ends_blank || frame.item.last_line_blank);
    }
}

fn close_all_lists(lists: &mut Vec<ListFrame>, facts: &mut Vec<CommitmentFact>, at: usize) {
    while !lists.is_empty() {
        close_last_list(lists, facts, at, false);
    }
}

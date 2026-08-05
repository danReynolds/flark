use crate::{
    BlockOutput, BlockRecord, ContainerKind, HtmlClass, LeafBlock, LeafKind, LeafLine, OriginSpan,
    ReferenceOccurrence, SegmentedSource, SignatureEvent, SignatureKind,
};
use comrak::block_spine_facade::{
    self as donor, FacadeAlignment, FacadeError, MAX_CLASSIFICATION_BYTES,
};
use im::Vector;
use std::fmt;

const CODE_INDENT: u32 = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockError {
    Donor(FacadeError),
    OversizedOrdinaryLine { bytes: usize },
    InvalidState(&'static str),
}

impl fmt::Display for BlockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for BlockError {}

impl From<FacadeError> for BlockError {
    fn from(value: FacadeError) -> Self {
        Self::Donor(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineType {
    Blank,
    Hr,
    AtxHeader,
    SetextUnderline,
    IndentedCode,
    FencedCode,
    Html,
    Text,
    Table,
    TableUnderline,
}

#[derive(Clone, Debug)]
struct LineAnalysis {
    kind: LineType,
    data: u32,
    enforce_new_block: bool,
    beg: usize,
    end: usize,
    indent: u32,
    alignments: Vec<FacadeAlignment>,
}

impl LineAnalysis {
    fn dummy() -> Self {
        Self {
            kind: LineType::Blank,
            data: 0,
            enforce_new_block: false,
            beg: 0,
            end: 0,
            indent: 0,
            alignments: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
struct Container {
    ch: u8,
    start: u64,
    mark_indent: u32,
    contents_indent: u32,
    opener_index: usize,
    marker: OriginSpan,
    task: Option<u8>,
}

impl Default for Container {
    fn default() -> Self {
        Self {
            ch: 0,
            start: 0,
            mark_indent: 0,
            contents_indent: 0,
            opener_index: 0,
            marker: OriginSpan::default(),
            task: None,
        }
    }
}

impl Container {
    fn is_quote(&self) -> bool {
        self.ch == b'>'
    }

    fn is_ordered(&self) -> bool {
        matches!(self.ch, b'.' | b')')
    }

    fn list_kind(&self) -> ContainerKind {
        if self.is_ordered() {
            ContainerKind::Ordered {
                delimiter: self.ch,
                start: self.start,
            }
        } else {
            ContainerKind::Unordered { marker: self.ch }
        }
    }
}

#[derive(Clone, Debug)]
struct LeafBuilder {
    kind: LeafKind,
    lines: Vec<LeafLine>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EndOutcome {
    Ended,
    RetainedSetextUnderline,
}

#[derive(Clone, Debug)]
pub struct BlockCheckpoint {
    containers: Vector<Container>,
    current: Option<LeafBuilder>,
    pivot: Option<LineAnalysis>,
    code_fence_length: usize,
    code_fence_char: u8,
    html_class: Option<HtmlClass>,
    last_line_has_list_loosening_effect: bool,
    last_list_item_starts_with_two_blank_lines: bool,
    output: BlockOutput,
}

impl PartialEq for BlockCheckpoint {
    fn eq(&self, other: &Self) -> bool {
        self.containers == other.containers
            && self.current == other.current
            && self.pivot == other.pivot
            && self.code_fence_length == other.code_fence_length
            && self.code_fence_char == other.code_fence_char
            && self.html_class == other.html_class
            && self.last_line_has_list_loosening_effect
                == other.last_line_has_list_loosening_effect
            && self.last_list_item_starts_with_two_blank_lines
                == other.last_list_item_starts_with_two_blank_lines
            && self.output.records == other.output.records
        // Reference occurrences and first-definition values are output
        // aggregates, deliberately excluded from structural convergence.
    }
}

impl Eq for BlockCheckpoint {}

#[derive(Clone, Debug)]
pub struct Md4cDerivedBlockCore {
    source: SegmentedSource,
    next_absolute: usize,
    checkpoint: BlockCheckpoint,
}

impl Md4cDerivedBlockCore {
    pub fn parse(source: SegmentedSource) -> Result<Self, BlockError> {
        let mut parser = Self::new(source);
        while parser.step_line()? {}
        parser.finish()?;
        Ok(parser)
    }

    pub fn new(source: SegmentedSource) -> Self {
        Self {
            source,
            next_absolute: 0,
            checkpoint: BlockCheckpoint {
                containers: Vector::new(),
                current: None,
                pivot: None,
                code_fence_length: 0,
                code_fence_char: 0,
                html_class: None,
                last_line_has_list_loosening_effect: false,
                last_list_item_starts_with_two_blank_lines: false,
                output: BlockOutput::default(),
            },
        }
    }

    pub fn source(&self) -> &SegmentedSource {
        &self.source
    }

    pub fn output(&self) -> &BlockOutput {
        &self.checkpoint.output
    }

    pub fn checkpoint(&self) -> BlockCheckpoint {
        self.checkpoint.clone()
    }

    pub fn next_absolute(&self) -> usize {
        self.next_absolute
    }

    pub fn step_line(&mut self) -> Result<bool, BlockError> {
        let Some(line) = self.source.line_at(self.next_absolute) else {
            return Ok(false);
        };
        if line.scratch.len() > MAX_CLASSIFICATION_BYTES {
            return Err(BlockError::OversizedOrdinaryLine {
                bytes: line.scratch.len(),
            });
        }
        let analysis = self.analyze_line(&line)?;
        self.process_line(&line, analysis)?;
        self.next_absolute = line.next_absolute();
        Ok(true)
    }

    pub fn finish(&mut self) -> Result<(), BlockError> {
        self.end_current()?;
        self.leave_child_containers(0)?;
        Ok(())
    }

    fn records_len(&self) -> usize {
        self.checkpoint.output.records.len()
    }

    fn push_record(&mut self, record: BlockRecord) {
        self.checkpoint.output.records.push_back(record);
    }

    fn mark_list_loose(&mut self, opener_index: usize) -> Result<(), BlockError> {
        let Some(record) = self.checkpoint.output.records.get(opener_index).cloned() else {
            return Err(BlockError::InvalidState("list opener index"));
        };
        let BlockRecord::Enter { kind, marker, .. } = record else {
            return Err(BlockError::InvalidState("list opener record"));
        };
        self.checkpoint.output.records.set(
            opener_index,
            BlockRecord::Enter {
                kind,
                marker,
                tight: false,
            },
        );
        Ok(())
    }

    fn start_new_block(&mut self, analysis: &LineAnalysis) -> Result<(), BlockError> {
        if self.checkpoint.current.is_some() {
            return Err(BlockError::InvalidState("start with current leaf"));
        }
        let kind = match analysis.kind {
            LineType::Hr => LeafKind::ThematicBreak,
            LineType::AtxHeader => LeafKind::Heading {
                level: analysis.data as u8,
                setext: false,
            },
            LineType::FencedCode => LeafKind::Code { fenced: true },
            LineType::IndentedCode => LeafKind::Code { fenced: false },
            LineType::Text => LeafKind::Paragraph,
            LineType::Html => LeafKind::Html {
                class: HtmlClass::from_u8(analysis.data as u8)
                    .ok_or(BlockError::InvalidState("html class"))?,
            },
            LineType::Table => LeafKind::Table {
                alignments: analysis.alignments.clone(),
            },
            _ => return Err(BlockError::InvalidState("non-leaf line start")),
        };
        self.checkpoint.current = Some(LeafBuilder {
            kind,
            lines: Vec::new(),
        });
        Ok(())
    }

    fn add_line(&mut self, line: &crate::LineWindow, analysis: &LineAnalysis) {
        let content_end = match analysis.kind {
            LineType::AtxHeader => analysis.end,
            _ => line.content_len,
        };
        let logical_end = match analysis.kind {
            LineType::AtxHeader | LineType::Hr | LineType::SetextUnderline => content_end,
            _ => line.scratch.len(),
        };
        let leaf_line = LeafLine {
            logical: line.origin_slice(analysis.beg..logical_end),
            content: line.origin_slice(analysis.beg..content_end),
            hidden_prefix: line.origin_slice(0..analysis.beg),
            indent: analysis.indent,
        };
        self.checkpoint
            .current
            .as_mut()
            .expect("current leaf")
            .lines
            .push(leaf_line);
    }

    fn current_logical_origin(&self, current: &LeafBuilder) -> OriginSpan {
        let mut origin = OriginSpan::default();
        for line in &current.lines {
            origin.append(&line.logical);
        }
        origin
    }

    fn consume_reference_definitions(
        &mut self,
        current: &mut LeafBuilder,
    ) -> Result<usize, BlockError> {
        if !matches!(current.kind, LeafKind::Paragraph | LeafKind::Heading { setext: true, .. }) {
            return Ok(0);
        }
        let logical = self.current_logical_origin(current);
        let text = self.source.materialize_origin(&logical);
        if text.len() > MAX_CLASSIFICATION_BYTES || !text.starts_with('[') {
            return Ok(0);
        }
        let definitions = donor::reference_definitions(&text)?;
        let consumed = definitions.last().map_or(0, |definition| definition.source.end);
        for definition in definitions {
            let occurrence = ReferenceOccurrence {
                normalized_label: definition.normalized_label.clone(),
                source: logical.slice(definition.source.clone()),
                label: logical.slice(definition.label_source),
                destination: logical.slice(definition.url_source),
                title: definition.title_source.map(|range| logical.slice(range)),
                url: definition.resolved.url,
                clean_title: definition.resolved.title,
            };
            self.checkpoint
                .output
                .reference_occurrences
                .push_back(occurrence.clone());
            if !self
                .checkpoint
                .output
                .first_definitions
                .contains_key(&occurrence.normalized_label)
            {
                self.checkpoint
                    .output
                    .first_definitions
                    .insert(occurrence.normalized_label.clone(), occurrence);
            }
        }
        if consumed > 0 {
            let mut remaining = consumed;
            while remaining > 0 && !current.lines.is_empty() {
                let len = current.lines[0].logical.len();
                if remaining >= len {
                    remaining -= len;
                    current.lines.remove(0);
                } else {
                    let line = &mut current.lines[0];
                    line.logical = line.logical.slice(remaining..line.logical.len());
                    line.content = line.logical.clone();
                    line.hidden_prefix = OriginSpan::default();
                    remaining = 0;
                }
            }
        }
        Ok(consumed)
    }

    fn end_current(&mut self) -> Result<EndOutcome, BlockError> {
        let Some(mut current) = self.checkpoint.current.take() else {
            return Ok(EndOutcome::Ended);
        };
        self.consume_reference_definitions(&mut current)?;
        if let LeafKind::Heading { setext: true, .. } = current.kind {
            if current.lines.len() == 1 {
                current.kind = LeafKind::Paragraph;
                self.checkpoint.current = Some(current);
                return Ok(EndOutcome::RetainedSetextUnderline);
            }
        }
        if current.lines.is_empty() {
            return Ok(EndOutcome::Ended);
        }
        self.push_record(BlockRecord::Leaf(LeafBlock {
            kind: current.kind,
            lines: current.lines,
        }));
        Ok(EndOutcome::Ended)
    }

    fn enter_child_containers(&mut self, n_children: usize) -> Result<(), BlockError> {
        let start = self.checkpoint.containers.len() - n_children;
        for index in start..self.checkpoint.containers.len() {
            let container = self.checkpoint.containers[index].clone();
            if container.is_quote() {
                self.end_current()?;
                self.push_record(BlockRecord::Enter {
                    kind: ContainerKind::Quote,
                    marker: container.marker,
                    tight: true,
                });
            } else {
                self.end_current()?;
                let opener_index = self.records_len();
                self.checkpoint.containers[index].opener_index = opener_index;
                self.push_record(BlockRecord::Enter {
                    kind: container.list_kind(),
                    marker: container.marker.clone(),
                    tight: true,
                });
                self.push_record(BlockRecord::Enter {
                    kind: ContainerKind::Item {
                        task: container.task,
                    },
                    marker: container.marker,
                    tight: true,
                });
            }
        }
        Ok(())
    }

    fn leave_child_containers(&mut self, n_keep: usize) -> Result<(), BlockError> {
        while self.checkpoint.containers.len() > n_keep {
            let container = self
                .checkpoint
                .containers
                .back()
                .cloned()
                .ok_or(BlockError::InvalidState("container close"))?;
            self.end_current()?;
            if container.is_quote() {
                self.push_record(BlockRecord::Exit {
                    kind: ContainerKind::Quote,
                });
            } else {
                self.push_record(BlockRecord::Exit {
                    kind: ContainerKind::Item {
                        task: container.task,
                    },
                });
                self.push_record(BlockRecord::Exit {
                    kind: container.list_kind(),
                });
            }
            self.checkpoint.containers.pop_back();
        }
        Ok(())
    }

    fn line_indentation(bytes: &[u8], mut offset: usize, total: u32) -> (u32, usize) {
        let mut column = total;
        while offset < bytes.len() && matches!(bytes[offset], b' ' | b'\t') {
            if bytes[offset] == b'\t' {
                column = (column + 4) & !3;
            } else {
                column += 1;
            }
            offset += 1;
        }
        (column - total, offset)
    }

    fn is_hr_line(bytes: &[u8], beg: usize) -> bool {
        let marker = bytes[beg];
        let mut count = 0;
        for byte in &bytes[beg..] {
            if *byte == marker {
                count += 1;
            } else if !matches!(*byte, b' ' | b'\t') {
                return false;
            }
        }
        count >= 3
    }

    fn is_setext_underline(bytes: &[u8], beg: usize) -> Option<u32> {
        let marker = *bytes.get(beg)?;
        if !matches!(marker, b'=' | b'-') {
            return None;
        }
        let mut offset = beg + 1;
        while bytes.get(offset) == Some(&marker) {
            offset += 1;
        }
        while matches!(bytes.get(offset), Some(b' ' | b'\t')) {
            offset += 1;
        }
        (offset == bytes.len()).then_some(if marker == b'=' { 1 } else { 2 })
    }

    fn atx_header(bytes: &[u8], beg: usize) -> Option<(usize, u32)> {
        let mut offset = beg;
        while bytes.get(offset) == Some(&b'#') && offset - beg < 7 {
            offset += 1;
        }
        let level = offset - beg;
        if level == 0
            || level > 6
            || (offset < bytes.len() && !matches!(bytes[offset], b' ' | b'\t'))
        {
            return None;
        }
        while matches!(bytes.get(offset), Some(b' ' | b'\t')) {
            offset += 1;
        }
        Some((offset, level as u32))
    }

    fn opening_fence(&mut self, bytes: &[u8], beg: usize) -> bool {
        let marker = bytes[beg];
        let mut offset = beg;
        while bytes.get(offset) == Some(&marker) {
            offset += 1;
        }
        if offset - beg < 3 {
            return false;
        }
        if marker == b'`' && bytes[offset..].contains(&b'`') {
            return false;
        }
        self.checkpoint.code_fence_char = marker;
        self.checkpoint.code_fence_length = offset - beg;
        true
    }

    fn closing_fence(&self, bytes: &[u8], beg: usize) -> bool {
        let mut offset = beg;
        while bytes.get(offset) == Some(&self.checkpoint.code_fence_char) {
            offset += 1;
        }
        offset - beg >= self.checkpoint.code_fence_length
            && bytes[offset..]
                .iter()
                .all(|byte| matches!(*byte, b' ' | b'\t'))
    }

    fn container_mark(
        &self,
        line: &crate::LineWindow,
        bytes: &[u8],
        indent: u32,
        beg: usize,
    ) -> Option<(usize, Container)> {
        if beg >= bytes.len() || indent >= CODE_INDENT {
            return None;
        }
        if bytes[beg] == b'>' {
            return Some((
                beg + 1,
                Container {
                    ch: b'>',
                    mark_indent: indent,
                    contents_indent: indent + 1,
                    marker: line.origin_slice(beg..beg + 1),
                    ..Container::default()
                },
            ));
        }
        if matches!(bytes[beg], b'-' | b'+' | b'*')
            && (beg + 1 == bytes.len() || matches!(bytes[beg + 1], b' ' | b'\t'))
        {
            return Some((
                beg + 1,
                Container {
                    ch: bytes[beg],
                    mark_indent: indent,
                    contents_indent: indent + 1,
                    marker: line.origin_slice(beg..beg + 1),
                    ..Container::default()
                },
            ));
        }
        let mut offset = beg;
        let max_end = (beg + 9).min(bytes.len());
        let mut start = 0_u64;
        while offset < max_end && bytes[offset].is_ascii_digit() {
            start = start * 10 + u64::from(bytes[offset] - b'0');
            offset += 1;
        }
        if offset > beg
            && offset < bytes.len()
            && matches!(bytes[offset], b'.' | b')')
            && (offset + 1 == bytes.len() || matches!(bytes[offset + 1], b' ' | b'\t'))
        {
            return Some((
                offset + 1,
                Container {
                    ch: bytes[offset],
                    start,
                    mark_indent: indent,
                    contents_indent: indent + (offset - beg + 1) as u32,
                    marker: line.origin_slice(beg..offset + 1),
                    ..Container::default()
                },
            ));
        }
        None
    }

    fn containers_compatible(pivot: &Container, candidate: &Container) -> bool {
        !candidate.is_quote()
            && candidate.ch == pivot.ch
            && candidate.mark_indent <= pivot.contents_indent
    }

    fn last_record_is_item(&self) -> bool {
        matches!(
            self.checkpoint.output.records.back(),
            Some(BlockRecord::Enter {
                kind: ContainerKind::Item { .. },
                ..
            })
        )
    }

    fn analyze_line(&mut self, line: &crate::LineWindow) -> Result<LineAnalysis, BlockError> {
        let bytes = line.content().as_bytes();
        let pivot_original = self
            .checkpoint
            .pivot
            .clone()
            .unwrap_or_else(LineAnalysis::dummy);
        let mut pivot_kind = pivot_original.kind;
        let mut total_indent = 0_u32;
        let mut n_parents = 0_usize;
        let mut n_brothers = 0_usize;
        let mut n_children = 0_usize;
        let mut container = Container::default();
        let previous_loosen = self.checkpoint.last_line_has_list_loosening_effect;
        let (mut indent, mut offset) = Self::line_indentation(bytes, 0, total_indent);
        total_indent += indent;
        let mut analysis = LineAnalysis {
            kind: LineType::Text,
            data: 0,
            enforce_new_block: false,
            beg: offset,
            end: bytes.len(),
            indent,
            alignments: Vec::new(),
        };

        while n_parents < self.checkpoint.containers.len() {
            let parent = self.checkpoint.containers[n_parents].clone();
            if parent.is_quote() && indent < CODE_INDENT && bytes.get(offset) == Some(&b'>') {
                offset += 1;
                total_indent += 1;
                (indent, offset) = Self::line_indentation(bytes, offset, total_indent);
                total_indent += indent;
                if indent > 0 {
                    indent -= 1;
                }
                analysis.beg = offset;
            } else if !parent.is_quote() && indent >= parent.contents_indent {
                indent -= parent.contents_indent;
            } else {
                break;
            }
            n_parents += 1;
        }

        if offset >= bytes.len() {
            while n_parents < self.checkpoint.containers.len()
                && !self.checkpoint.containers[n_parents].is_quote()
            {
                n_parents += 1;
            }
        }

        loop {
            if pivot_kind == LineType::FencedCode {
                analysis.beg = offset;
                if indent < CODE_INDENT && self.closing_fence(bytes, offset) {
                    analysis.kind = LineType::Blank;
                    self.checkpoint.last_line_has_list_loosening_effect = false;
                    break;
                }
                if n_parents == self.checkpoint.containers.len() {
                    indent = indent.saturating_sub(pivot_original.indent);
                    analysis.kind = LineType::FencedCode;
                    break;
                }
            }

            if pivot_kind == LineType::Html {
                if let Some(class) = self.checkpoint.html_class {
                    if n_parents < self.checkpoint.containers.len() {
                        self.checkpoint.html_class = None;
                    } else {
                        let ended = if matches!(class, HtmlClass::BlockTag | HtmlClass::CompleteTag) {
                            bytes[offset..].is_empty()
                        } else {
                            donor::html_block_end(class as u8, &line.scratch[offset..])?
                        };
                        if ended {
                            self.checkpoint.html_class = None;
                            if matches!(class, HtmlClass::BlockTag | HtmlClass::CompleteTag) {
                                analysis.kind = LineType::Blank;
                                analysis.indent = 0;
                                break;
                            }
                        }
                        analysis.kind = LineType::Html;
                        n_parents = self.checkpoint.containers.len();
                        break;
                    }
                }
            }

            if offset >= bytes.len() {
                if pivot_kind == LineType::IndentedCode
                    && n_parents == self.checkpoint.containers.len()
                {
                    analysis.kind = LineType::IndentedCode;
                    indent = indent.saturating_sub(CODE_INDENT);
                    self.checkpoint.last_line_has_list_loosening_effect = false;
                } else {
                    analysis.kind = LineType::Blank;
                    self.checkpoint.last_line_has_list_loosening_effect = n_parents > 0
                        && n_brothers + n_children == 0
                        && !self.checkpoint.containers[n_parents - 1].is_quote();
                    if n_parents > 0
                        && !self.checkpoint.containers[n_parents - 1].is_quote()
                        && n_brothers + n_children == 0
                        && self.checkpoint.current.is_none()
                        && self.last_record_is_item()
                    {
                        self.checkpoint.last_list_item_starts_with_two_blank_lines = true;
                    }
                }
                break;
            }

            if self.checkpoint.last_list_item_starts_with_two_blank_lines {
                if n_parents > 0
                    && n_parents == self.checkpoint.containers.len()
                    && !self.checkpoint.containers[n_parents - 1].is_quote()
                    && n_brothers + n_children == 0
                    && self.checkpoint.current.is_none()
                    && self.last_record_is_item()
                {
                    n_parents -= 1;
                    indent = total_indent;
                    if n_parents > 0 {
                        indent = indent.min(
                            indent.saturating_sub(
                                self.checkpoint.containers[n_parents - 1].contents_indent,
                            ),
                        );
                    }
                }
                self.checkpoint.last_list_item_starts_with_two_blank_lines = false;
            }
            self.checkpoint.last_line_has_list_loosening_effect = false;

            if indent < CODE_INDENT
                && pivot_kind == LineType::Text
                && matches!(bytes[offset], b'=' | b'-')
                && n_parents == self.checkpoint.containers.len()
            {
                if let Some(level) = Self::is_setext_underline(bytes, offset) {
                    analysis.kind = LineType::SetextUnderline;
                    analysis.data = level;
                    break;
                }
            }

            if indent < CODE_INDENT
                && matches!(bytes[offset], b'-' | b'_' | b'*')
                && Self::is_hr_line(bytes, offset)
            {
                analysis.kind = LineType::Hr;
                break;
            }

            if n_parents < self.checkpoint.containers.len()
                && n_brothers + n_children == 0
            {
                if let Some((after_mark, candidate)) =
                    self.container_mark(line, bytes, indent, offset)
                {
                    if Self::containers_compatible(
                        &self.checkpoint.containers[n_parents],
                        &candidate,
                    ) {
                        pivot_kind = LineType::Blank;
                        offset = after_mark;
                        total_indent += candidate.contents_indent - candidate.mark_indent;
                        (indent, offset) = Self::line_indentation(bytes, offset, total_indent);
                        total_indent += indent;
                        analysis.beg = offset;
                        let mut adjusted = candidate;
                        if offset >= bytes.len() {
                            adjusted.contents_indent += 1;
                        } else if indent <= CODE_INDENT {
                            adjusted.contents_indent += indent;
                            indent = 0;
                        } else {
                            adjusted.contents_indent += 1;
                            indent -= 1;
                        }
                        self.checkpoint.containers[n_parents].mark_indent = adjusted.mark_indent;
                        self.checkpoint.containers[n_parents].contents_indent =
                            adjusted.contents_indent;
                        container = adjusted;
                        n_brothers += 1;
                        continue;
                    }
                }
            }

            if indent >= CODE_INDENT && pivot_kind != LineType::Text {
                analysis.kind = LineType::IndentedCode;
                indent -= CODE_INDENT;
                break;
            }

            if indent < CODE_INDENT {
                if let Some((after_mark, mut candidate)) =
                    self.container_mark(line, bytes, indent, offset)
                {
                    let blank_mark_interrupt = pivot_kind == LineType::Text
                        && n_parents == self.checkpoint.containers.len()
                        && after_mark >= bytes.len()
                        && !candidate.is_quote();
                    let ordered_interrupt = pivot_kind == LineType::Text
                        && n_parents == self.checkpoint.containers.len()
                        && candidate.is_ordered()
                        && candidate.start != 1;
                    if !blank_mark_interrupt && !ordered_interrupt {
                        offset = after_mark;
                        total_indent += candidate.contents_indent - candidate.mark_indent;
                        (indent, offset) = Self::line_indentation(bytes, offset, total_indent);
                        total_indent += indent;
                        analysis.beg = offset;
                        analysis.data = u32::from(candidate.ch);
                        if offset >= bytes.len() {
                            candidate.contents_indent += 1;
                        } else if indent <= CODE_INDENT {
                            candidate.contents_indent += indent;
                            indent = 0;
                        } else {
                            candidate.contents_indent += 1;
                            indent -= 1;
                        }
                        if n_brothers + n_children == 0 {
                            pivot_kind = LineType::Blank;
                        }
                        if n_children == 0 {
                            self.leave_child_containers(n_parents + n_brothers)?;
                        }
                        n_children += 1;
                        self.checkpoint.containers.push_back(candidate.clone());
                        container = candidate;
                        continue;
                    }
                }
            }

            if pivot_kind == LineType::Table
                && n_parents == self.checkpoint.containers.len()
            {
                analysis.kind = LineType::Table;
                break;
            }

            if indent < CODE_INDENT && bytes[offset] == b'#' {
                if let Some((content_beg, level)) = Self::atx_header(bytes, offset) {
                    analysis.kind = LineType::AtxHeader;
                    analysis.data = level;
                    analysis.beg = content_beg;
                    break;
                }
            }

            if indent < CODE_INDENT
                && matches!(bytes[offset], b'`' | b'~')
                && self.opening_fence(bytes, offset)
            {
                analysis.kind = LineType::FencedCode;
                analysis.data = 1;
                analysis.enforce_new_block = true;
                break;
            }

            if bytes[offset] == b'<' {
                let allow_type_7 = pivot_kind != LineType::Text;
                if let Some(class) = donor::html_block_start(&line.scratch[offset..], allow_type_7)?
                    .and_then(HtmlClass::from_u8)
                {
                    let ended = if matches!(class, HtmlClass::BlockTag | HtmlClass::CompleteTag) {
                        false
                    } else {
                        donor::html_block_end(class as u8, &line.scratch[offset..])?
                    };
                    self.checkpoint.html_class = (!ended).then_some(class);
                    analysis.enforce_new_block = true;
                    analysis.kind = LineType::Html;
                    analysis.data = class as u32;
                    break;
                }
            }

            if pivot_kind == LineType::Text
                && matches!(bytes[offset], b'|' | b'-' | b':')
                && n_parents == self.checkpoint.containers.len()
            {
                if let Some(current) = &self.checkpoint.current {
                    if current.lines.len() == 1 {
                        let header = self.source.materialize_origin(&current.lines[0].logical);
                        let delimiter = &line.scratch[offset..];
                        if let (Some(header), Some(alignments)) = (
                            donor::table_row(&header, false)?,
                            donor::table_delimiter_alignments(delimiter, false)?,
                        ) {
                            if header.cells.len() == alignments.len() {
                                analysis.kind = LineType::TableUnderline;
                                analysis.data = alignments.len() as u32;
                                analysis.alignments = alignments;
                                break;
                            }
                        }
                    }
                }
            }

            analysis.kind = LineType::Text;
            if pivot_kind == LineType::Text && n_brothers + n_children == 0 {
                n_parents = self.checkpoint.containers.len();
            }

            if n_brothers + n_children > 0
                && self
                    .checkpoint
                    .containers
                    .back()
                    .is_some_and(|current| !current.is_quote())
            {
                let mut task = offset;
                while task < bytes.len() && task < offset + 3 && matches!(bytes[task], b' ' | b'\t') {
                    task += 1;
                }
                if task + 2 < bytes.len()
                    && bytes[task] == b'['
                    && matches!(bytes[task + 1], b'x' | b'X' | b' ')
                    && bytes[task + 2] == b']'
                    && (task + 3 == bytes.len()
                        || matches!(bytes[task + 3], b' ' | b'\t'))
                {
                    let symbol = bytes[task + 1];
                    if n_children > 0 {
                        self.checkpoint.containers.back_mut().expect("child").task = Some(symbol);
                    } else {
                        container.task = Some(symbol);
                    }
                    offset = task + 3;
                    while matches!(bytes.get(offset), Some(b' ' | b'\t')) {
                        offset += 1;
                    }
                    analysis.beg = offset;
                }
            }
            break;
        }

        analysis.indent = indent;
        analysis.end = bytes.len();
        if analysis.kind == LineType::AtxHeader {
            let mut end = analysis.end;
            while end > analysis.beg && matches!(bytes[end - 1], b' ' | b'\t') {
                end -= 1;
            }
            let trailing = end;
            while end > analysis.beg && bytes[end - 1] == b'#' {
                end -= 1;
            }
            if end == analysis.beg || (end < trailing && matches!(bytes[end - 1], b' ' | b'\t')) {
                analysis.end = end;
            }
        }

        if previous_loosen && analysis.kind != LineType::Blank && n_parents + n_brothers > 0 {
            let parent = self.checkpoint.containers[n_parents + n_brothers - 1].clone();
            if !parent.is_quote() {
                self.mark_list_loose(parent.opener_index)?;
            }
        }

        if n_children == 0 && n_parents + n_brothers < self.checkpoint.containers.len() {
            self.leave_child_containers(n_parents + n_brothers)?;
        }

        if n_brothers > 0 {
            let old = self.checkpoint.containers[n_parents].clone();
            self.end_current()?;
            self.push_record(BlockRecord::Exit {
                kind: ContainerKind::Item { task: old.task },
            });
            self.push_record(BlockRecord::Enter {
                kind: ContainerKind::Item {
                    task: container.task,
                },
                marker: container.marker.clone(),
                tight: true,
            });
            self.checkpoint.containers[n_parents].task = container.task;
            self.checkpoint.containers[n_parents].marker = container.marker;
        }

        if n_children > 0 {
            self.enter_child_containers(n_children)?;
        }
        Ok(analysis)
    }

    fn process_line(
        &mut self,
        line: &crate::LineWindow,
        mut analysis: LineAnalysis,
    ) -> Result<(), BlockError> {
        if analysis.kind == LineType::Blank {
            self.end_current()?;
            self.checkpoint.pivot = None;
            return Ok(());
        }

        if analysis.enforce_new_block {
            self.end_current()?;
        }

        if matches!(analysis.kind, LineType::Hr | LineType::AtxHeader) {
            self.end_current()?;
            self.start_new_block(&analysis)?;
            self.add_line(line, &analysis);
            self.end_current()?;
            self.checkpoint.pivot = None;
            return Ok(());
        }

        if analysis.kind == LineType::SetextUnderline {
            let Some(current) = self.checkpoint.current.as_mut() else {
                return Err(BlockError::InvalidState("setext without paragraph"));
            };
            current.kind = LeafKind::Heading {
                level: analysis.data as u8,
                setext: true,
            };
            self.add_line(line, &analysis);
            let outcome = self.end_current()?;
            if outcome == EndOutcome::RetainedSetextUnderline {
                analysis.kind = LineType::Text;
                self.checkpoint.pivot = Some(analysis);
            } else {
                self.checkpoint.pivot = None;
            }
            return Ok(());
        }

        if analysis.kind == LineType::TableUnderline {
            let Some(current) = self.checkpoint.current.as_mut() else {
                return Err(BlockError::InvalidState("table without header"));
            };
            current.kind = LeafKind::Table {
                alignments: analysis.alignments.clone(),
            };
            self.add_line(line, &analysis);
            analysis.kind = LineType::Table;
            self.checkpoint.pivot = Some(analysis);
            return Ok(());
        }

        if self
            .checkpoint
            .pivot
            .as_ref()
            .is_some_and(|pivot| pivot.kind != analysis.kind)
        {
            self.end_current()?;
        }
        if self.checkpoint.current.is_none() {
            self.start_new_block(&analysis)?;
            self.checkpoint.pivot = Some(analysis.clone());
        }
        self.add_line(line, &analysis);
        Ok(())
    }

    pub fn signatures(&self) -> Result<Vec<SignatureEvent>, BlockError> {
        let mut events = Vec::new();
        for record in &self.checkpoint.output.records {
            match record {
                BlockRecord::Enter { kind, tight, .. } => {
                    let signature = match kind {
                        ContainerKind::Quote => SignatureKind::Quote,
                        ContainerKind::Unordered { .. } => SignatureKind::List {
                            ordered: false,
                            start: 1,
                            tight: *tight,
                        },
                        ContainerKind::Ordered { start, .. } => SignatureKind::List {
                            ordered: true,
                            start: *start,
                            tight: *tight,
                        },
                        ContainerKind::Item { .. } => SignatureKind::Item,
                    };
                    events.push(SignatureEvent::Enter(signature));
                }
                BlockRecord::Exit { kind } => {
                    let signature = match kind {
                        ContainerKind::Quote => SignatureKind::Quote,
                        ContainerKind::Unordered { .. } => SignatureKind::List {
                            ordered: false,
                            start: 1,
                            tight: true,
                        },
                        ContainerKind::Ordered { start, .. } => SignatureKind::List {
                            ordered: true,
                            start: *start,
                            tight: true,
                        },
                        ContainerKind::Item { .. } => SignatureKind::Item,
                    };
                    // Exit list metadata is normalized from the matching
                    // opener by the test stack; tight=true is a placeholder.
                    events.push(SignatureEvent::Exit(signature));
                }
                BlockRecord::Leaf(leaf) => self.leaf_signatures(leaf, &mut events)?,
            }
        }
        normalize_exit_metadata(&mut events);
        Ok(events)
    }

    fn leaf_signatures(
        &self,
        leaf: &LeafBlock,
        events: &mut Vec<SignatureEvent>,
    ) -> Result<(), BlockError> {
        let kind = match &leaf.kind {
            LeafKind::ThematicBreak => SignatureKind::ThematicBreak,
            LeafKind::Heading { level, setext } => SignatureKind::Heading {
                level: *level,
                setext: *setext,
            },
            LeafKind::Code { fenced } => SignatureKind::Code { fenced: *fenced },
            LeafKind::Html { class } => SignatureKind::Html { class: *class },
            LeafKind::Paragraph => SignatureKind::Paragraph,
            LeafKind::Table { alignments } => {
                let table = SignatureKind::Table {
                    columns: alignments.len(),
                };
                events.push(SignatureEvent::Enter(table.clone()));
                for (row_index, line) in leaf.lines.iter().enumerate() {
                    if row_index == 1 {
                        continue;
                    }
                    let row = SignatureKind::TableRow {
                        header: row_index == 0,
                    };
                    events.push(SignatureEvent::Enter(row.clone()));
                    let logical = self.source.materialize_origin(&line.logical);
                    let parsed = donor::table_row(&logical, false)?;
                    for column in 0..alignments.len() {
                        let cell = SignatureKind::TableCell { column };
                        events.push(SignatureEvent::Enter(cell.clone()));
                        events.push(SignatureEvent::Exit(cell));
                    }
                    let _ = parsed;
                    events.push(SignatureEvent::Exit(row));
                }
                events.push(SignatureEvent::Exit(table));
                return Ok(());
            }
        };
        events.push(SignatureEvent::Enter(kind.clone()));
        events.push(SignatureEvent::Exit(kind));
        Ok(())
    }
}

fn normalize_exit_metadata(events: &mut [SignatureEvent]) {
    let mut stack: Vec<SignatureKind> = Vec::new();
    for event in events {
        match event {
            SignatureEvent::Enter(kind) => stack.push(kind.clone()),
            SignatureEvent::Exit(kind) => {
                if let Some(open) = stack.pop() {
                    *kind = open;
                }
            }
        }
    }
}

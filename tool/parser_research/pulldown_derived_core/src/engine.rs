use std::collections::{HashMap, HashSet};
use std::mem::size_of;
use std::ops::Range;
use std::sync::Arc;

use crate::model::{
    AdvanceReceipt, Ancestry, Chunk, ChunkKind, Container, EditReceipt, Fuel, MarkerFact,
    MarkerKind, MemoryReceipt, OutputDelta, ParseError, SemanticChunk, SemanticFact,
    SemanticSnapshot, Span,
};
use crate::scanners::{
    scan_closing_fence, scan_opening_fence, scan_setext, LineCursor, LineWork, ListScan,
    MAX_CONTAINER_DEPTH,
};

const CHECKPOINT_STRIDE: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FenceState {
    marker: u8,
    len: u32,
    indent: u8,
    container_depth: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingParagraph {
    chunk_index: u32,
    digest: u64,
    lines: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParserState {
    containers: Arc<[Container]>,
    fence: Option<FenceState>,
    pending: Option<PendingParagraph>,
}

impl Default for ParserState {
    fn default() -> Self {
        Self {
            containers: Arc::from([]),
            fence: None,
            pending: None,
        }
    }
}

impl ParserState {
    fn convergence_eq(&self, other: &Self) -> bool {
        self.containers == other.containers
            && self.fence == other.fence
            && match (self.pending, other.pending) {
                (None, None) => true,
                (Some(left), Some(right)) => {
                    left.digest == right.digest && left.lines == right.lines
                }
                _ => false,
            }
    }

    fn shifted_chunk_indexes(mut self, delta: isize) -> Self {
        let delta = i32::try_from(delta).expect("32-bit spike output shift");
        if let Some(pending) = &mut self.pending {
            pending.chunk_index = pending
                .chunk_index
                .checked_add_signed(delta)
                .expect("validated output splice");
        }
        self
    }
}

#[derive(Clone, Debug)]
struct Checkpoint {
    offset: u32,
    chunks: u32,
    facts: u32,
    state: ParserState,
}

impl Checkpoint {
    fn offset(&self) -> usize {
        self.offset as usize
    }
}

#[derive(Clone, Debug)]
struct PendingMarker {
    kind: MarkerKind,
    span: Span,
}

#[derive(Clone, Debug)]
struct AncestryPool {
    values: Vec<Ancestry>,
}

impl Default for AncestryPool {
    fn default() -> Self {
        Self {
            values: vec![Ancestry(Vec::new())],
        }
    }
}

impl AncestryPool {
    fn intern(&mut self, containers: &[Container]) -> u32 {
        if let Some((index, _)) = self
            .values
            .iter()
            .enumerate()
            .find(|(_, ancestry)| ancestry.0 == containers)
        {
            return index as u32;
        }
        let index = self.values.len();
        self.values.push(Ancestry(containers.to_vec()));
        u32::try_from(index).expect("bounded research ancestry pool")
    }
}

/// A cooperatively resumable parse task. `advance` never examines more source
/// bytes than its supplied fuel, even when the current physical line is 10 MB.
pub struct ParserTask {
    source: String,
    cursor: usize,
    line: Option<LineWork>,
    pending_cr: bool,
    state: ParserState,
    chunks: Vec<Chunk>,
    facts: Vec<MarkerFact>,
    ancestries: AncestryPool,
    checkpoints: Vec<Checkpoint>,
    next_checkpoint: usize,
    next_id: u64,
    complete: bool,
    max_advance_source_bytes: usize,
    max_transient_state_bytes: usize,
}

impl ParserTask {
    pub fn new(source: impl Into<String>) -> Result<Self, ParseError> {
        let source = source.into();
        if source.len() > u32::MAX as usize {
            return Err(ParseError::DocumentTooLarge);
        }
        let mut task = Self {
            source,
            cursor: 0,
            line: None,
            pending_cr: false,
            state: ParserState::default(),
            chunks: Vec::new(),
            facts: Vec::new(),
            ancestries: AncestryPool::default(),
            checkpoints: Vec::new(),
            next_checkpoint: CHECKPOINT_STRIDE,
            next_id: 1,
            complete: false,
            max_advance_source_bytes: 0,
            max_transient_state_bytes: 0,
        };
        task.record_checkpoint(true)?;
        Ok(task)
    }

    fn from_checkpoint(
        source: String,
        checkpoint: &Checkpoint,
        chunks: Vec<Chunk>,
        facts: Vec<MarkerFact>,
        ancestries: AncestryPool,
        checkpoints: Vec<Checkpoint>,
        next_id: u64,
    ) -> Self {
        let cursor = checkpoint.offset();
        Self {
            source,
            cursor,
            line: None,
            pending_cr: false,
            state: checkpoint.state.clone(),
            chunks,
            facts,
            ancestries,
            checkpoints,
            next_checkpoint: ((cursor / CHECKPOINT_STRIDE) + 1) * CHECKPOINT_STRIDE,
            next_id,
            complete: false,
            max_advance_source_bytes: 0,
            max_transient_state_bytes: 0,
        }
    }

    pub fn advance(&mut self, fuel: Fuel) -> Result<AdvanceReceipt, ParseError> {
        self.advance_inner(fuel, false)
    }

    fn advance_one_line(&mut self, fuel: Fuel) -> Result<AdvanceReceipt, ParseError> {
        self.advance_inner(fuel, true)
    }

    fn advance_inner(
        &mut self,
        fuel: Fuel,
        stop_after_line: bool,
    ) -> Result<AdvanceReceipt, ParseError> {
        if fuel.bytes == 0 {
            return Err(ParseError::ZeroFuel);
        }
        if self.complete {
            return Ok(AdvanceReceipt {
                complete: true,
                ..AdvanceReceipt::default()
            });
        }

        let before_chunks = self.chunks.len();
        let before_facts = self.facts.len();
        let mut remaining = fuel.bytes;
        let mut source_bytes = 0;
        let mut lines_completed = 0;

        while remaining > 0 && !self.complete {
            if self.pending_cr {
                if self.source.as_bytes().get(self.cursor) == Some(&b'\n') {
                    self.cursor += 1;
                    remaining -= 1;
                    source_bytes += 1;
                }
                self.pending_cr = false;
                self.finish_line(self.cursor)?;
                lines_completed += 1;
                if stop_after_line {
                    break;
                }
                continue;
            }

            if self.cursor == self.source.len() {
                if self.line.as_ref().is_some_and(|line| line.len() > 0) {
                    self.finish_line(self.cursor)?;
                    lines_completed += 1;
                }
                self.finish_document()?;
                break;
            }

            if self.line.is_none() {
                self.line = Some(LineWork::new(self.cursor));
            }
            let byte = self.source.as_bytes()[self.cursor];
            let absolute = self.cursor;
            self.cursor += 1;
            remaining -= 1;
            source_bytes += 1;
            match byte {
                b'\n' => {
                    self.finish_line(self.cursor)?;
                    lines_completed += 1;
                    if stop_after_line {
                        break;
                    }
                }
                b'\r' => self.pending_cr = true,
                _ => self
                    .line
                    .as_mut()
                    .expect("line initialized")
                    .observe(absolute, byte),
            }
        }

        self.max_advance_source_bytes = self.max_advance_source_bytes.max(source_bytes);
        let transient = size_of::<ParserState>()
            + self.state.containers.len() * size_of::<Container>()
            + self.line.as_ref().map_or(0, LineWork::transient_bytes);
        self.max_transient_state_bytes = self.max_transient_state_bytes.max(transient);

        Ok(AdvanceReceipt {
            source_bytes,
            lines_completed,
            chunks_emitted: self.chunks.len() - before_chunks,
            facts_emitted: self.facts.len() - before_facts,
            complete: self.complete,
        })
    }

    fn finish_line(&mut self, line_end: usize) -> Result<(), ParseError> {
        let line = self.line.take().unwrap_or_else(|| LineWork::new(line_end));
        let content_end = line.start + line.len();
        self.parse_complete_line(&line, content_end, line_end)?;
        self.record_checkpoint(false)
    }

    fn finish_document(&mut self) -> Result<(), ParseError> {
        self.state.pending = None;
        self.state.fence = None;
        self.state.containers = Arc::from([]);
        self.complete = true;
        self.record_checkpoint(true)
    }

    fn scan_existing_containers<'a>(
        &self,
        line: &'a LineWork,
    ) -> Result<(usize, LineCursor<'a>, Vec<PendingMarker>), ParseError> {
        let mut cursor = LineCursor::new(line);
        let mut markers = Vec::new();
        let mut matched = 0;
        for container in self.state.containers.iter() {
            let saved = cursor.clone();
            match container {
                Container::BlockQuote => {
                    let _ = cursor.scan_space(3)?;
                    let Some(marker) = cursor.scan_blockquote_marker()? else {
                        cursor = saved;
                        break;
                    };
                    markers.push(PendingMarker {
                        kind: MarkerKind::BlockQuote,
                        span: Span::new(line.start + marker.start, line.start + marker.end)?,
                    });
                }
                Container::BulletItem { indent, .. } | Container::OrderedItem { indent, .. } => {
                    if !cursor.scan_space(*indent as usize)? && !cursor.is_at_eol()? {
                        cursor = saved;
                        break;
                    }
                }
            }
            matched += 1;
        }
        Ok((matched, cursor, markers))
    }

    fn parse_complete_line(
        &mut self,
        line: &LineWork,
        content_end: usize,
        line_end: usize,
    ) -> Result<(), ParseError> {
        let (matched, mut cursor, mut markers) = self.scan_existing_containers(line)?;

        if let Some(fence) = self.state.fence {
            if matched == fence.container_depth as usize {
                let mut content_cursor = cursor.clone();
                let _ = content_cursor.scan_space(fence.indent as usize)?;
                let mut close_cursor = content_cursor.clone();
                let _ = close_cursor.scan_space(4usize.saturating_sub(fence.indent as usize))?;
                if let Some(close_len) = scan_closing_fence(
                    line,
                    close_cursor.position(),
                    fence.marker,
                    fence.len as usize,
                )? {
                    markers.push(PendingMarker {
                        kind: MarkerKind::FenceClose(fence.marker),
                        span: Span::new(
                            line.start + close_cursor.position(),
                            line.start + close_cursor.position() + close_len,
                        )?,
                    });
                    self.emit_chunk(
                        ChunkKind::FenceClose {
                            marker: fence.marker,
                            len: close_len as u32,
                        },
                        Span::new(line.start, line_end)?,
                        Span::new(
                            line.start + close_cursor.position(),
                            line.start + close_cursor.position() + close_len,
                        )?,
                        &self.state.containers.clone(),
                        markers,
                    )?;
                    self.state.fence = None;
                    self.state.pending = None;
                    return Ok(());
                }

                self.emit_chunk(
                    ChunkKind::CodeLine,
                    Span::new(line.start, line_end)?,
                    Span::new(line.start + content_cursor.position(), content_end)?,
                    &self.state.containers.clone(),
                    markers,
                )?;
                return Ok(());
            }
            // Pulldown closes the fence when an enclosing container can no
            // longer be continued, then parses this line again outside it.
            self.state.fence = None;
            self.state.containers = Arc::from(&self.state.containers[..matched]);
        }

        if let Some(pending) = self.state.pending {
            if matched == self.state.containers.len() {
                let mut setext_cursor = cursor.clone();
                let indent = setext_cursor.scan_space_upto(4)?;
                if indent < 4 {
                    if let Some((level, marker_len)) = scan_setext(line, setext_cursor.position())?
                    {
                        let chunk_index = pending.chunk_index as usize;
                        let chunk_id = self.chunks[chunk_index].id;
                        self.chunks[chunk_index].kind = ChunkKind::Heading(level);
                        self.chunks[chunk_index].source.end =
                            u32::try_from(line_end).map_err(|_| ParseError::DocumentTooLarge)?;
                        for marker in markers {
                            self.facts.push(MarkerFact {
                                chunk_id,
                                kind: marker.kind,
                                span: marker.span,
                            });
                        }
                        self.facts.push(MarkerFact {
                            chunk_id,
                            kind: MarkerKind::Setext(level),
                            span: Span::new(
                                line.start + setext_cursor.position(),
                                line.start + setext_cursor.position() + marker_len,
                            )?,
                        });
                        self.state.pending = None;
                        return Ok(());
                    }
                }
            }

            if line.is_blank_from(cursor.position())? {
                self.state.pending = None;
                self.state.containers = Arc::from(&self.state.containers[..matched]);
                return self.emit_blank(line, line_end, markers);
            }

            if !self.detect_interrupt(line, cursor.clone())? {
                // Pulldown's paragraph loop permits lazy continuation when an
                // enclosing quote/list marker is absent and no block starts.
                cursor.scan_all_space()?;
                let chunk = &mut self.chunks[pending.chunk_index as usize];
                chunk.source.end =
                    u32::try_from(line_end).map_err(|_| ParseError::DocumentTooLarge)?;
                chunk.content.end =
                    u32::try_from(content_end).map_err(|_| ParseError::DocumentTooLarge)?;
                for marker in markers {
                    self.facts.push(MarkerFact {
                        chunk_id: chunk.id,
                        kind: marker.kind,
                        span: marker.span,
                    });
                }
                self.state.pending = Some(PendingParagraph {
                    chunk_index: pending.chunk_index,
                    digest: pending.digest.rotate_left(7) ^ line.digest,
                    lines: pending.lines + 1,
                });
                return Ok(());
            }
            self.state.pending = None;
            self.state.containers = Arc::from(&self.state.containers[..matched]);
        } else {
            self.state.containers = Arc::from(&self.state.containers[..matched]);
        }

        let mut containers = self.state.containers.to_vec();
        self.scan_new_containers(line, &mut cursor, &mut containers, &mut markers)?;
        self.state.containers = Arc::from(containers);

        if line.is_blank_from(cursor.position())? {
            return self.emit_blank(line, line_end, markers);
        }

        let indent = cursor.scan_space_upto(4)?;
        if indent == 4 {
            return Err(ParseError::UnsupportedSyntax {
                offset: line.start + cursor.position(),
                feature: "indented code block",
            });
        }
        let content_start = cursor.position();

        for marker in [b'-', b'*', b'_'] {
            if line.is_thematic_break_from(content_start, marker)? {
                return Err(ParseError::UnsupportedSyntax {
                    offset: line.start + content_start,
                    feature: "thematic break",
                });
            }
        }

        if let Some(fence) = scan_opening_fence(line, content_start)? {
            markers.push(PendingMarker {
                kind: MarkerKind::FenceOpen(fence.marker),
                span: Span::new(
                    line.start + content_start,
                    line.start + content_start + fence.len,
                )?,
            });
            let containers = self.state.containers.clone();
            self.emit_chunk(
                ChunkKind::FenceOpen {
                    marker: fence.marker,
                    len: fence.len as u32,
                },
                Span::new(line.start, line_end)?,
                Span::new(line.start + fence.info_start, content_end)?,
                &containers,
                markers,
            )?;
            self.state.fence = Some(FenceState {
                marker: fence.marker,
                len: fence.len as u32,
                indent: indent as u8,
                container_depth: self.state.containers.len() as u8,
            });
            return Ok(());
        }

        if line.prefix().get(content_start) == Some(&b'#') {
            return Err(ParseError::UnsupportedSyntax {
                offset: line.start + content_start,
                feature: "ATX heading",
            });
        }

        let containers = self.state.containers.clone();
        let chunk_index = self.emit_chunk(
            ChunkKind::Paragraph,
            Span::new(line.start, line_end)?,
            Span::new(line.start + content_start, content_end)?,
            &containers,
            markers,
        )?;
        self.state.pending = Some(PendingParagraph {
            chunk_index: chunk_index as u32,
            digest: line.digest,
            lines: 1,
        });
        Ok(())
    }

    fn detect_interrupt(
        &self,
        line: &LineWork,
        mut cursor: LineCursor<'_>,
    ) -> Result<bool, ParseError> {
        let mut saw_container = false;
        loop {
            let saved = cursor.clone();
            let outer_indent = cursor.scan_space_upto(4)?;
            if outer_indent == 4 {
                cursor = saved;
                break;
            }
            if let Some(list) = cursor.scan_list_marker_with_indent(outer_indent)? {
                if !matches!(list.marker, b'.' | b')') || list.start == 1 {
                    return Ok(true);
                }
                cursor = saved;
                break;
            }
            if cursor.scan_blockquote_marker()?.is_some() {
                saw_container = true;
                continue;
            }
            cursor = saved;
            break;
        }
        if saw_container {
            return Ok(true);
        }
        let indent = cursor.scan_space_upto(4)?;
        if indent == 4 {
            return Ok(false);
        }
        let position = cursor.position();
        if scan_opening_fence(line, position)?.is_some() {
            return Ok(true);
        }
        for marker in [b'-', b'*', b'_'] {
            if line.is_thematic_break_from(position, marker)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn scan_new_containers(
        &self,
        line: &LineWork,
        cursor: &mut LineCursor<'_>,
        containers: &mut Vec<Container>,
        markers: &mut Vec<PendingMarker>,
    ) -> Result<(), ParseError> {
        loop {
            let saved = cursor.clone();
            let outer_indent = cursor.scan_space_upto(4)?;
            if outer_indent == 4 {
                *cursor = saved;
                return Ok(());
            }
            if let Some(list) = cursor.scan_list_marker_with_indent(outer_indent)? {
                self.push_list_container(line, containers, markers, list)?;
            } else if let Some(marker) = cursor.scan_blockquote_marker()? {
                containers.push(Container::BlockQuote);
                markers.push(PendingMarker {
                    kind: MarkerKind::BlockQuote,
                    span: Span::new(line.start + marker.start, line.start + marker.end)?,
                });
            } else {
                *cursor = saved;
                return Ok(());
            }
            if containers.len() > MAX_CONTAINER_DEPTH {
                return Err(ParseError::UnsupportedSyntax {
                    offset: line.start + cursor.position(),
                    feature: "container nesting beyond the spike bound",
                });
            }
        }
    }

    fn push_list_container(
        &self,
        line: &LineWork,
        containers: &mut Vec<Container>,
        markers: &mut Vec<PendingMarker>,
        list: ListScan,
    ) -> Result<(), ParseError> {
        let indent = u8::try_from(list.indent).map_err(|_| ParseError::UnsupportedSyntax {
            offset: line.start + list.marker_start,
            feature: "list indentation beyond the spike bound",
        })?;
        let (container, kind) = if matches!(list.marker, b'.' | b')') {
            (
                Container::OrderedItem {
                    delimiter: list.marker,
                    start: list.start,
                    indent,
                },
                MarkerKind::Ordered {
                    delimiter: list.marker,
                    start: list.start,
                },
            )
        } else {
            (
                Container::BulletItem {
                    marker: list.marker,
                    indent,
                },
                MarkerKind::Bullet(list.marker),
            )
        };
        containers.push(container);
        markers.push(PendingMarker {
            kind,
            span: Span::new(line.start + list.marker_start, line.start + list.marker_end)?,
        });
        Ok(())
    }

    fn emit_blank(
        &mut self,
        line: &LineWork,
        line_end: usize,
        markers: Vec<PendingMarker>,
    ) -> Result<(), ParseError> {
        let containers = self.state.containers.clone();
        self.emit_chunk(
            ChunkKind::Blank,
            Span::new(line.start, line_end)?,
            Span::new(line.start, line.start + line.len())?,
            &containers,
            markers,
        )?;
        Ok(())
    }

    fn emit_chunk(
        &mut self,
        kind: ChunkKind,
        source: Span,
        content: Span,
        containers: &[Container],
        markers: Vec<PendingMarker>,
    ) -> Result<usize, ParseError> {
        let ancestry = self.ancestries.intern(containers);
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).expect("research ID overflow");
        let index = self.chunks.len();
        self.chunks.push(Chunk {
            id,
            kind,
            source,
            content,
            ancestry,
        });
        self.facts
            .extend(markers.into_iter().map(|marker| MarkerFact {
                chunk_id: id,
                kind: marker.kind,
                span: marker.span,
            }));
        Ok(index)
    }

    fn record_checkpoint(&mut self, force: bool) -> Result<(), ParseError> {
        // A setext underline can still rewrite a pending paragraph. Retained
        // edit checkpoints therefore wait for a closed output boundary; the
        // transient task may still yield at any byte inside that paragraph.
        if !force && (self.cursor < self.next_checkpoint || self.state.pending.is_some()) {
            return Ok(());
        }
        let checkpoint = Checkpoint {
            offset: u32::try_from(self.cursor).map_err(|_| ParseError::DocumentTooLarge)?,
            chunks: u32::try_from(self.chunks.len()).expect("bounded research chunks"),
            facts: u32::try_from(self.facts.len()).expect("bounded research facts"),
            state: self.state.clone(),
        };
        if self
            .checkpoints
            .last()
            .is_some_and(|existing| existing.offset == checkpoint.offset)
        {
            *self.checkpoints.last_mut().expect("checked") = checkpoint;
        } else {
            self.checkpoints.push(checkpoint);
        }
        while self.next_checkpoint <= self.cursor {
            self.next_checkpoint += CHECKPOINT_STRIDE;
        }
        Ok(())
    }

    fn into_document(self) -> Result<Document, ParseError> {
        if !self.complete {
            return Err(ParseError::UnsupportedSyntax {
                offset: self.cursor,
                feature: "conversion of an incomplete parser task",
            });
        }
        Ok(Document {
            source: self.source,
            chunks: self.chunks,
            facts: self.facts,
            ancestries: self.ancestries,
            checkpoints: self.checkpoints,
            next_id: self.next_id,
            max_advance_source_bytes: self.max_advance_source_bytes,
            max_transient_state_bytes: self.max_transient_state_bytes,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Document {
    source: String,
    chunks: Vec<Chunk>,
    facts: Vec<MarkerFact>,
    ancestries: AncestryPool,
    checkpoints: Vec<Checkpoint>,
    next_id: u64,
    max_advance_source_bytes: usize,
    max_transient_state_bytes: usize,
}

impl Document {
    pub fn parse(source: impl Into<String>, fuel: Fuel) -> Result<Self, ParseError> {
        let mut task = ParserTask::new(source)?;
        while !task.complete {
            task.advance(fuel)?;
        }
        task.into_document()
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    pub fn facts(&self) -> &[MarkerFact] {
        &self.facts
    }

    pub fn ancestries(&self) -> &[Ancestry] {
        &self.ancestries.values
    }

    pub fn semantic_snapshot(&self) -> SemanticSnapshot {
        let id_to_index: HashMap<_, _> = self
            .chunks
            .iter()
            .enumerate()
            .map(|(index, chunk)| (chunk.id, index))
            .collect();
        SemanticSnapshot {
            chunks: self
                .chunks
                .iter()
                .map(|chunk| SemanticChunk {
                    kind: chunk.kind,
                    source: chunk.source,
                    content: chunk.content,
                    ancestry: self.ancestries.values[chunk.ancestry as usize].clone(),
                })
                .collect(),
            facts: self
                .facts
                .iter()
                .map(|fact| SemanticFact {
                    kind: fact.kind,
                    span: fact.span,
                    chunk: id_to_index[&fact.chunk_id],
                })
                .collect(),
        }
    }

    pub fn apply_edit(
        &mut self,
        old_range: Range<usize>,
        replacement: &str,
        fuel: Fuel,
    ) -> Result<EditReceipt, ParseError> {
        if fuel.bytes == 0 {
            return Err(ParseError::ZeroFuel);
        }
        if old_range.start > old_range.end
            || old_range.end > self.source.len()
            || !self.source.is_char_boundary(old_range.start)
            || !self.source.is_char_boundary(old_range.end)
        {
            return Err(ParseError::InvalidEditRange);
        }

        let old_source_len = self.source.len();
        let old_chunks = self.chunks.clone();
        let old_facts = self.facts.clone();
        let old_checkpoints = self.checkpoints.clone();
        let old_ancestries = self.ancestries.clone();
        let old_next_id = self.next_id;

        let mut source = self.source.clone();
        source.replace_range(old_range.clone(), replacement);
        if source.len() > u32::MAX as usize {
            return Err(ParseError::DocumentTooLarge);
        }
        let new_end = old_range.start + replacement.len();
        let source_delta = source.len() as isize - old_source_len as isize;

        let restart_index = old_checkpoints
            .partition_point(|checkpoint| checkpoint.offset() <= old_range.start)
            .saturating_sub(1);
        let restart_checkpoint = &old_checkpoints[restart_index];
        let restart = restart_checkpoint.offset();
        let prefix_chunks = old_chunks[..restart_checkpoint.chunks as usize].to_vec();
        let prefix_facts = old_facts[..restart_checkpoint.facts as usize].to_vec();
        let prefix_checkpoints = old_checkpoints[..=restart_index].to_vec();
        let mut task = ParserTask::from_checkpoint(
            source,
            restart_checkpoint,
            prefix_chunks,
            prefix_facts,
            old_ancestries,
            prefix_checkpoints,
            old_next_id,
        );

        let old_by_offset: HashMap<_, _> = old_checkpoints
            .iter()
            .enumerate()
            .map(|(index, checkpoint)| (checkpoint.offset(), index))
            .collect();
        let mut advance_calls = 0;
        let mut converged_at = None;
        while !task.complete {
            task.advance_one_line(fuel)?;
            advance_calls += 1;
            if task.cursor < new_end || task.pending_cr || task.line.is_some() {
                continue;
            }
            let mapped = task.cursor as isize - source_delta;
            if mapped < old_range.end as isize {
                continue;
            }
            let Some(&old_checkpoint_index) = old_by_offset.get(&(mapped as usize)) else {
                continue;
            };
            let old_checkpoint = &old_checkpoints[old_checkpoint_index];
            // A pending paragraph can be retroactively promoted by the first
            // reused line. Waiting for a closed output boundary avoids a
            // false splice without inventing a fix-up pass.
            if task.state.pending.is_none() && task.state.convergence_eq(&old_checkpoint.state) {
                converged_at = Some(old_checkpoint_index);
                break;
            }
        }

        let reparsed_end = task.cursor;
        let mut reused_suffix_chunks = 0;
        if let Some(old_checkpoint_index) = converged_at {
            let old_checkpoint = &old_checkpoints[old_checkpoint_index];
            let chunk_index_delta = task.chunks.len() as isize - old_checkpoint.chunks as isize;
            let fact_index_delta = task.facts.len() as isize - old_checkpoint.facts as isize;
            reused_suffix_chunks = old_chunks.len() - old_checkpoint.chunks as usize;
            // Capture the actual convergence boundary before attaching the
            // reused suffix. Recording it afterwards would associate an early
            // source offset with end-of-document output counts.
            task.record_checkpoint(true)?;
            task.chunks.extend(
                old_chunks[old_checkpoint.chunks as usize..]
                    .iter()
                    .cloned()
                    .map(|chunk| chunk.shifted(source_delta)),
            );
            task.facts.extend(
                old_facts[old_checkpoint.facts as usize..]
                    .iter()
                    .cloned()
                    .map(|fact| fact.shifted(source_delta)),
            );
            let source_delta_i32 = i32::try_from(source_delta).expect("32-bit spike source shift");
            let chunk_index_delta_i32 =
                i32::try_from(chunk_index_delta).expect("32-bit spike chunk shift");
            let fact_index_delta_i32 =
                i32::try_from(fact_index_delta).expect("32-bit spike fact shift");
            for checkpoint in old_checkpoints.iter().skip(old_checkpoint_index + 1) {
                task.checkpoints.push(Checkpoint {
                    offset: checkpoint
                        .offset
                        .checked_add_signed(source_delta_i32)
                        .expect("validated source splice"),
                    chunks: checkpoint
                        .chunks
                        .checked_add_signed(chunk_index_delta_i32)
                        .expect("validated chunk splice"),
                    facts: checkpoint
                        .facts
                        .checked_add_signed(fact_index_delta_i32)
                        .expect("validated fact splice"),
                    state: checkpoint
                        .state
                        .clone()
                        .shifted_chunk_indexes(chunk_index_delta),
                });
            }
            task.cursor = task.source.len();
            task.line = None;
            task.pending_cr = false;
            task.state = ParserState::default();
            task.complete = true;
        }

        if !task.complete {
            task.finish_document()?;
        }
        let new_document = task.into_document()?;
        let delta = calculate_delta(
            &old_chunks,
            &new_document.chunks,
            &old_facts,
            &new_document.facts,
            source_delta,
        );
        let receipt = EditReceipt {
            restart,
            reparsed_end,
            reparsed_bytes: reparsed_end.saturating_sub(restart),
            advance_calls,
            converged: converged_at.is_some(),
            reused_suffix_chunks,
            delta,
        };
        *self = new_document;
        Ok(receipt)
    }

    pub fn memory_receipt(&self) -> MemoryReceipt {
        let mut unique_container_allocations = HashSet::new();
        let checkpoint_container_bytes = self
            .checkpoints
            .iter()
            .filter_map(|checkpoint| {
                let pointer =
                    Arc::as_ptr(&checkpoint.state.containers) as *const Container as usize;
                unique_container_allocations
                    .insert(pointer)
                    .then_some(checkpoint.state.containers.len() * size_of::<Container>())
            })
            .sum();
        MemoryReceipt {
            source_bytes: self.source.capacity(),
            chunk_bytes: self.chunks.capacity() * size_of::<Chunk>(),
            fact_bytes: self.facts.capacity() * size_of::<MarkerFact>(),
            checkpoint_bytes: self.checkpoints.capacity() * size_of::<Checkpoint>(),
            checkpoint_container_bytes,
            ancestry_bytes: self.ancestries.values.capacity() * size_of::<Ancestry>()
                + self
                    .ancestries
                    .values
                    .iter()
                    .map(|ancestry| ancestry.0.capacity() * size_of::<Container>())
                    .sum::<usize>(),
            transient_state_bytes: self.max_transient_state_bytes,
            checkpoints: self.checkpoints.len(),
            chunks: self.chunks.len(),
            facts: self.facts.len(),
            max_advance_source_bytes: self.max_advance_source_bytes,
        }
    }
}

fn calculate_delta(
    old_chunks: &[Chunk],
    new_chunks: &[Chunk],
    old_facts: &[MarkerFact],
    new_facts: &[MarkerFact],
    suffix_shift: isize,
) -> OutputDelta {
    let chunk_prefix = old_chunks
        .iter()
        .zip(new_chunks)
        .take_while(|(old, new)| old == new)
        .count();
    let mut chunk_suffix = 0;
    while chunk_suffix < old_chunks.len().saturating_sub(chunk_prefix)
        && chunk_suffix < new_chunks.len().saturating_sub(chunk_prefix)
    {
        let old = &old_chunks[old_chunks.len() - 1 - chunk_suffix];
        let new = &new_chunks[new_chunks.len() - 1 - chunk_suffix];
        if old.clone().shifted(suffix_shift) != *new {
            break;
        }
        chunk_suffix += 1;
    }

    let fact_prefix = old_facts
        .iter()
        .zip(new_facts)
        .take_while(|(old, new)| old == new)
        .count();
    let mut fact_suffix = 0;
    while fact_suffix < old_facts.len().saturating_sub(fact_prefix)
        && fact_suffix < new_facts.len().saturating_sub(fact_prefix)
    {
        let old = &old_facts[old_facts.len() - 1 - fact_suffix];
        let new = &new_facts[new_facts.len() - 1 - fact_suffix];
        if old.clone().shifted(suffix_shift) != *new {
            break;
        }
        fact_suffix += 1;
    }

    OutputDelta {
        old_chunks: chunk_prefix..old_chunks.len() - chunk_suffix,
        new_chunks: chunk_prefix..new_chunks.len() - chunk_suffix,
        old_facts: fact_prefix..old_facts.len() - fact_suffix,
        new_facts: fact_prefix..new_facts.len() - fact_suffix,
        reused_suffix_shift: suffix_shift,
    }
}

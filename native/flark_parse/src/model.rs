//! Flat render-model extraction over an unmodified comrak AST.
//!
//! See `schema/render_model_v1.json` and `SCHEMA.md` for the layout. The
//! extraction walks the tree once, derives what comrak does not expose
//! (per-line content ranges, reference definitions), and validates every
//! derivation against comrak's own output in report mode.

use crate::lines::LineIndex;
use crate::reference_definitions::{self, CoverBlock, Definition};
use crate::schema::{self, block, block_kind, content, definition, header, run, run_kind, table_alignment};
use comrak::nodes::{AstNode, ListType, NodeValue, Sourcepos, TableAlignment};
use comrak::{parse_document, Arena, Options};

pub type BlockRec = [u32; block::WORDS];
pub type ContentRec = [u32; content::WORDS];
pub type RunRec = [u32; run::WORDS];
pub type DefinitionRec = [u32; definition::WORDS];

/// A validation finding from report mode. `rule` is a stable identifier.
#[derive(Clone, Debug)]
pub struct Deviation { pub rule: &'static str, pub detail: String }

pub fn options() -> Options<'static> {
    let mut o = Options::default();
    o.extension.table = true;
    o.extension.strikethrough = true;
    o.extension.tasklist = true;
    o.extension.autolink = true;
    o.extension.footnotes = true;
    o.render.sourcepos = true;
    o.render.escaped_char_spans = true;
    o
}

/// comrak sourcepos → [start, end) byte range. Columns are 1-based bytes
/// within the line; `end.column` is inclusive.
pub fn sourcepos_range(sp: Sourcepos, li: &LineIndex, src_len: usize) -> Option<(usize, usize)> {
    if sp.start.line == 0 || sp.end.line == 0 { return None; }
    let start = li.line_start(sp.start.line - 1) + sp.start.column.saturating_sub(1);
    let end = li.line_start(sp.end.line - 1) + sp.end.column;
    let start = start.min(src_len);
    let end = end.min(src_len).max(start);
    Some((start, end))
}

#[derive(Clone, Copy)]
struct Container { kind: u8, offset: usize, first_line: usize } // 1 quote, 2 item, 3 footnote

/// Column cursor over one physical line: spaces and tabs consumed by column,
/// with the remainder of a partially consumed tab carried as virtual spaces.
struct ColCursor<'a> { line: &'a [u8], pos: usize, col: usize, virt: usize }

impl<'a> ColCursor<'a> {
    fn new(line: &'a [u8]) -> Self { ColCursor { line, pos: 0, col: 0, virt: 0 } }
    fn peek(&self) -> Option<u8> { if self.virt > 0 { Some(b' ') } else { self.line.get(self.pos).copied() } }
    /// Consume up to `n` columns of whitespace. Returns the number consumed.
    fn consume_columns(&mut self, n: usize) -> usize {
        let mut got = 0usize;
        while got < n {
            if self.virt > 0 { self.virt -= 1; got += 1; continue; }
            match self.line.get(self.pos) {
                Some(b' ') => { self.pos += 1; self.col += 1; got += 1; }
                Some(b'\t') => {
                    let stop = (self.col / 4 + 1) * 4;
                    let width = stop - self.col;
                    self.pos += 1; self.col = stop;
                    let take = width.min(n - got);
                    got += take; self.virt = width - take;
                }
                _ => break,
            }
        }
        got
    }
    /// Consume all leading whitespace, virtual included.
    fn skip_whitespace(&mut self) { self.virt = 0; while matches!(self.line.get(self.pos), Some(b' ') | Some(b'\t')) { if self.line[self.pos] == b'\t' { self.col = (self.col / 4 + 1) * 4; } else { self.col += 1; } self.pos += 1; } }
    fn advance(&mut self, n: usize) { for _ in 0..n { if let Some(b) = self.line.get(self.pos) { if *b == b'\t' { self.col = (self.col / 4 + 1) * 4; } else { self.col += 1; } self.pos += 1; } } }
}

struct BlockNode<'b> { node: &'b AstNode<'b>, idx: usize, containers: Vec<Container> }

/// Correction context for a leaf whose leading definitions comrak stripped.
pub struct Shift { lines: Vec<(usize, usize, usize, usize)>, def_lines: usize }

pub struct Extractor<'a> {
    src: &'a str,
    li: LineIndex,
    blocks: Vec<BlockRec>,
    content: Vec<ContentRec>,
    runs: Vec<RunRec>,
    definitions: Vec<Definition>,
    strings: Vec<u8>,
    collect: bool,
    pub deviations: Vec<Deviation>,
}

impl<'a> Extractor<'a> {
    pub fn extract(src: &'a str) -> Vec<u8> { Self::run(src, false).0 }
    pub fn extract_with_report(src: &'a str) -> (Vec<u8>, Vec<Deviation>) { Self::run(src, true) }

    fn run(src: &'a str, collect: bool) -> (Vec<u8>, Vec<Deviation>) {
        let mut ex = Extractor { src, li: LineIndex::new(src), blocks: Vec::new(), content: Vec::new(), runs: Vec::new(), definitions: Vec::new(), strings: Vec::new(), collect, deviations: Vec::new() };
        let arena = Arena::new();
        let root = parse_document(&arena, src, &options());
        // Pass 1: block records and container chains.
        let mut leaves: Vec<BlockNode<'_>> = Vec::new();
        ex.walk_block(root, u32::MAX, &mut Vec::new(), &mut leaves);
        // Pass 2: definitions, validated against the block output.
        let cover: Vec<CoverBlock> = ex.blocks.iter().map(|b| CoverBlock { start: b[block::START_BYTE] as usize, end: b[block::END_BYTE] as usize + 1, is_container_list: b[block::KIND] == block_kind::LIST, is_document: b[block::KIND] == block_kind::DOCUMENT }).collect();
        ex.definitions = reference_definitions::collect(src, &cover);
        ex.definitions.sort_by_key(|d| d.start);
        // Pass 3: per-line content and inline runs for every leaf, in order.
        for leaf in &leaves { ex.leaf(leaf); }
        ex.definitions.sort_by_key(|d| d.start);
        ex.definitions.dedup_by_key(|d| d.start);
        let buf = ex.encode();
        (buf, ex.deviations)
    }

    fn dev(&mut self, rule: &'static str, detail: String) { if self.collect { self.deviations.push(Deviation { rule, detail }); } }

    fn push_string(&mut self, s: &str) -> (u32, u32) { let off = self.strings.len() as u32; self.strings.extend_from_slice(s.as_bytes()); (off, s.len() as u32) }

    // ---------------------------------------------------------------- blocks

    fn walk_block<'b>(&mut self, node: &'b AstNode<'b>, parent: u32, containers: &mut Vec<Container>, leaves: &mut Vec<BlockNode<'b>>) {
        let data = node.data.borrow();
        let sp = data.sourcepos;
        let first_line = sp.start.line.saturating_sub(1);
        let (start, end) = sourcepos_range(sp, &self.li, self.src.len()).unwrap_or((0, 0));
        let mut rec: BlockRec = [0; block::WORDS];
        rec[block::PARENT] = parent;
        rec[block::START_BYTE] = start as u32; rec[block::END_BYTE] = end as u32;
        rec[block::START_UTF16] = self.li.u16(start); rec[block::END_UTF16] = self.li.u16(end);
        rec[block::FIRST_LINE] = first_line as u32;
        rec[block::LINE_COUNT] = if sp.end.line >= sp.start.line && sp.start.line > 0 { (sp.end.line - sp.start.line + 1) as u32 } else { 0 };
        let mut container: Option<Container> = None;
        let mut is_leaf = false;
        match &data.value {
            NodeValue::Document => rec[block::KIND] = block_kind::DOCUMENT,
            NodeValue::Paragraph => { rec[block::KIND] = block_kind::PARAGRAPH; is_leaf = true; }
            NodeValue::Heading(h) => { rec[block::KIND] = block_kind::HEADING; rec[block::ATTR0] = h.level as u32; rec[block::FLAGS] = h.setext as u32; is_leaf = true; }
            NodeValue::CodeBlock(c) => { rec[block::KIND] = block_kind::CODE_BLOCK; rec[block::ATTR0] = if c.fenced { c.fence_length as u32 } else { 0 }; rec[block::FLAGS] = c.fenced as u32; is_leaf = true; }
            NodeValue::HtmlBlock(_) => { rec[block::KIND] = block_kind::HTML_BLOCK; is_leaf = true; }
            NodeValue::BlockQuote => { rec[block::KIND] = block_kind::BLOCK_QUOTE; container = Some(Container { kind: 1, offset: 0, first_line }); }
            NodeValue::List(l) => { rec[block::KIND] = block_kind::LIST; rec[block::ATTR0] = matches!(l.list_type, ListType::Ordered) as u32; rec[block::ATTR1] = l.start as u32; rec[block::FLAGS] = l.tight as u32; }
            NodeValue::Item(l) => { rec[block::KIND] = block_kind::ITEM; rec[block::ATTR0] = (l.marker_offset + l.padding) as u32; container = Some(Container { kind: 2, offset: l.marker_offset + l.padding, first_line }); }
            NodeValue::TaskItem(t) => {
                rec[block::KIND] = block_kind::ITEM;
                rec[block::FLAGS] = 1 | ((t.symbol.is_some() as u32) << 1);
                if let Some((ss, se)) = sourcepos_range(t.symbol_sourcepos, &self.li, self.src.len()) { rec[block::ATTR1] = ss as u32; rec[block::ATTR2] = se as u32; }
                // comrak reports no NodeList for task items; recover the content offset from the first child.
                let off = node.children().next().and_then(|c| sourcepos_range(c.data.borrow().sourcepos, &self.li, self.src.len())).map(|(cs, _)| cs.saturating_sub(self.li.line_start(first_line))).unwrap_or(2);
                rec[block::ATTR0] = off as u32;
                container = Some(Container { kind: 2, offset: off, first_line });
            }
            NodeValue::ThematicBreak => { rec[block::KIND] = block_kind::THEMATIC_BREAK; is_leaf = true; }
            NodeValue::Table(t) => {
                rec[block::KIND] = block_kind::TABLE; rec[block::ATTR0] = t.num_columns as u32;
                let mut packed = 0u32;
                for (i, a) in t.alignments.iter().enumerate().take(16) { let v = match a { TableAlignment::None => table_alignment::NONE, TableAlignment::Left => table_alignment::LEFT, TableAlignment::Center => table_alignment::CENTER, TableAlignment::Right => table_alignment::RIGHT }; packed |= v << (2 * i); }
                rec[block::ATTR1] = packed;
            }
            NodeValue::TableRow(h) => { rec[block::KIND] = block_kind::TABLE_ROW; rec[block::FLAGS] = *h as u32; }
            NodeValue::TableCell => { rec[block::KIND] = block_kind::TABLE_CELL; is_leaf = true; }
            NodeValue::FootnoteDefinition(_) => {
                rec[block::KIND] = block_kind::FOOTNOTE_DEFINITION;
                // label range on the first line: `[^label]:`
                let ls = self.li.line_start(first_line); let le = self.li.line_end(first_line, self.src.len());
                let line = &self.src.as_bytes()[ls..le];
                if let Some(o) = line.iter().position(|b| *b == b'[') { if line.get(o + 1) == Some(&b'^') { if let Some(c) = line[o..].iter().position(|b| *b == b']') { rec[block::ATTR1] = (ls + o + 2) as u32; rec[block::ATTR2] = (ls + o + c) as u32; } } }
                container = Some(Container { kind: 3, offset: 4, first_line });
            }
            _ => rec[block::KIND] = block_kind::OTHER,
        }
        drop(data);
        let idx = self.blocks.len();
        self.blocks.push(rec);
        if let Some(c) = container { containers.push(c); }
        if is_leaf {
            leaves.push(BlockNode { node, idx, containers: containers.clone() });
        } else {
            for child in node.children() {
                if is_block_value(&child.data.borrow().value) { self.walk_block(child, idx as u32, containers, leaves); }
                else { leaves.push(BlockNode { node: child, idx, containers: containers.clone() }); }
            }
        }
        if container.is_some() { containers.pop(); }
    }

    // ------------------------------------------------------------- per line

    /// Consume container prefixes on one physical line. Returns the cursor
    /// positioned at the innermost content, plus whether a prefix was absent
    /// (lazy continuation).
    fn prefix_cursor<'l>(&self, line0: usize, line: &'l [u8], containers: &[Container]) -> (ColCursor<'l>, bool) {
        let mut cur = ColCursor::new(line);
        let mut lazy = false;
        for c in containers {
            match c.kind {
                1 => {
                    let save = (cur.pos, cur.col, cur.virt);
                    cur.consume_columns(3);
                    if cur.peek() == Some(b'>') && cur.virt == 0 {
                        cur.advance(1);
                        // one optional space; a tab counts one column and carries the rest
                        match cur.line.get(cur.pos) { Some(b' ') => cur.advance(1), Some(b'\t') => { let stop = (cur.col / 4 + 1) * 4; let width = stop - cur.col; cur.pos += 1; cur.col = stop; cur.virt = width - 1; } _ => {} }
                    } else { cur.pos = save.0; cur.col = save.1; cur.virt = save.2; lazy = true; break; }
                }
                2 => {
                    if c.first_line == line0 {
                        // marker line: marker_offset spaces, the marker, then padding
                        cur.skip_whitespace_limited(c.offset);
                        while matches!(cur.line.get(cur.pos), Some(b) if b.is_ascii_digit()) { cur.advance(1); }
                        if matches!(cur.line.get(cur.pos), Some(b'-') | Some(b'+') | Some(b'*') | Some(b'.') | Some(b')')) { cur.advance(1); }
                        let target = c.offset.saturating_sub(cur.col);
                        cur.consume_columns(target);
                    } else {
                        let got = cur.consume_columns(c.offset);
                        if got < c.offset { lazy = true; break; }
                    }
                }
                3 => {
                    if c.first_line == line0 {
                        cur.skip_whitespace_limited(3);
                        if cur.peek() == Some(b'[') { while let Some(b) = cur.line.get(cur.pos) { let done = *b == b']'; cur.advance(1); if done { break; } } if cur.peek() == Some(b':') { cur.advance(1); } cur.skip_whitespace(); }
                    } else {
                        let got = cur.consume_columns(4);
                        if got < 4 { lazy = true; break; }
                    }
                }
                _ => {}
            }
        }
        (cur, lazy)
    }

    fn push_content(&mut self, line0: usize, cs: usize, ce: usize, virt: usize) {
        let ce = ce.max(cs);
        self.content.push([line0 as u32, cs as u32, self.li.u16(cs), ce as u32, self.li.u16(ce), virt as u32]);
    }

    fn leaf(&mut self, leaf: &BlockNode<'_>) {
        let idx = leaf.idx;
        let data = leaf.node.data.borrow();
        let sp = data.sourcepos;
        let _kind = self.blocks[idx][block::KIND];
        let content_start = self.content.len() as u32;
        self.blocks[idx][block::CONTENT_OFFSET] = content_start;
        if sp.start.line == 0 { drop(data); return; }
        let (l0, l1) = (sp.start.line - 1, sp.end.line - 1);
        match &data.value {
            NodeValue::CodeBlock(c) => { let c = c.clone(); drop(data); self.code_block_lines(idx, &c, l0, l1, &leaf.containers); }
            NodeValue::HtmlBlock(_) => {
                drop(data);
                let (bs, be) = (self.blocks[idx][block::START_BYTE] as usize, self.blocks[idx][block::END_BYTE] as usize);
                for line0 in l0..=l1 {
                    let (cur, _) = self.prefix_cursor(line0, self.line_bytes(line0), &leaf.containers);
                    let ls = self.li.line_start(line0);
                    let cs = (ls + cur.pos).max(if line0 == l0 { bs } else { 0 });
                    let le = self.li.line_end(line0, self.src.len());
                    if cs > be { break; }
                    self.push_content(line0, cs, le.min(be.max(cs)), cur.virt);
                }
            }
            NodeValue::ThematicBreak => { drop(data); }
            NodeValue::TableCell => {
                drop(data);
                let (cs, ce) = sourcepos_range(sp, &self.li, self.src.len()).unwrap_or((0, 0));
                let bytes = self.src.as_bytes();
                let (mut a, mut b) = (cs, ce);
                while a < b && bytes[a] == b' ' { a += 1; }
                while b > a && bytes[b - 1] == b' ' { b -= 1; }
                self.push_content(l0, a, b, 0);
                let cell = (cs, ce);
                for child in leaf.node.children() { self.walk_inline(child, idx as u32, u32::MAX, Some(cell), None); }
            }
            NodeValue::Heading(h) => {
                let setext = h.setext; drop(data);
                let last_content_line = if setext { l1.saturating_sub(1).max(l0) } else { l1 };
                if setext {
                    // A paragraph that became a setext heading may also have had
                    // definitions stripped from its start; same rule as paragraphs.
                    let (shift, records) = self.strip_definitions(l0, last_content_line, &leaf.containers);
                    for (line0, cs, ce, virt) in records { self.push_content(line0, cs, ce, virt); }
                    for child in leaf.node.children() { self.walk_inline(child, idx as u32, u32::MAX, None, shift.as_ref()); }
                    self.blocks[idx][block::CONTENT_COUNT] = self.content.len() as u32 - content_start;
                    return;
                }
                let (span_s, span_e) = self.children_span(leaf.node);
                for line0 in l0..=last_content_line {
                    let (mut cur, _) = self.prefix_cursor(line0, self.line_bytes(line0), &leaf.containers);
                    cur.skip_whitespace();
                    let ls = self.li.line_start(line0);
                    let mut cs = ls + cur.pos;
                    let mut ce = self.trimmed_end(line0, cs);
                    if !setext && line0 == l0 {
                        // ATX: skip the opening sequence and drop the closing one
                        let bytes = self.src.as_bytes();
                        let mut p = cs; while p < ce && bytes[p] == b'#' { p += 1; }
                        while p < ce && (bytes[p] == b' ' || bytes[p] == b'\t') { p += 1; }
                        cs = p;
                        let mut q = ce; while q > cs && bytes[q - 1] == b'#' { q -= 1; }
                        if q < ce && (q == cs || bytes[q - 1] == b' ' || bytes[q - 1] == b'\t') { ce = q; while ce > cs && (bytes[ce - 1] == b' ' || bytes[ce - 1] == b'\t') { ce -= 1; } }
                    }
                    if span_s < span_e { if line0 == l0 && span_s > cs && span_s < ce { cs = span_s; } if line0 == last_content_line && span_e < ce && span_e > cs { ce = span_e; } }
                    self.push_content(line0, cs, ce, 0);
                }
                for child in leaf.node.children() { self.walk_inline(child, idx as u32, u32::MAX, None, None); }
            }
            NodeValue::Paragraph => {
                drop(data);
                let (shift, records) = self.strip_definitions(l0, l1, &leaf.containers);
                for (line0, cs, ce, virt) in records { self.push_content(line0, cs, ce, virt); }
                for child in leaf.node.children() { self.walk_inline(child, idx as u32, u32::MAX, None, shift.as_ref()); }
            }
            _ => { drop(data); for child in leaf.node.children() { self.walk_inline(child, idx as u32, u32::MAX, None, None); } }
        }
        self.blocks[idx][block::CONTENT_COUNT] = self.content.len() as u32 - content_start;
        self.widen_block_to_content(idx);
    }

    /// comrak's sourcepos for an indented code block inside a container is a
    /// single byte; the derived content is validated against the literal, so
    /// the block range follows the content. Ancestors widen with it.
    fn widen_block_to_content(&mut self, idx: usize) {
        let (co, cn) = (self.blocks[idx][block::CONTENT_OFFSET] as usize, self.blocks[idx][block::CONTENT_COUNT] as usize);
        if cn == 0 { return; }
        let first = self.content[co][content::START_BYTE] as usize;
        let last = self.content[co + cn - 1][content::END_BYTE] as usize;
        let mut i = idx;
        loop {
            let b = &mut self.blocks[i];
            if (b[block::START_BYTE] as usize) > first { b[block::START_BYTE] = first as u32; b[block::START_UTF16] = self.li.u16(first); }
            if (b[block::END_BYTE] as usize) < last { b[block::END_BYTE] = last as u32; b[block::END_UTF16] = self.li.u16(last); }
            if b[block::PARENT] == u32::MAX { break; }
            i = b[block::PARENT] as usize;
        }
    }

    /// Per-line content for a paragraph-like leaf over lines l0..=l1, with
    /// definitions comrak consumed from the start removed (mirroring
    /// `resolve_reference_link_definitions`). Returns the correction context
    /// for inline positions and the content records to push.
    fn strip_definitions(&mut self, l0: usize, l1: usize, containers: &[Container]) -> (Option<Shift>, Vec<(usize, usize, usize, usize)>) {
        let mut lines_all: Vec<(usize, usize, usize, usize)> = Vec::new();
        for line0 in l0..=l1 {
            let (mut cur, _) = self.prefix_cursor(line0, self.line_bytes(line0), containers);
            cur.skip_whitespace();
            let ls = self.li.line_start(line0);
            let cs = ls + cur.pos;
            let ce = self.trimmed_end(line0, cs);
            lines_all.push((line0, cs, ce, 0));
        }
        let mut buffer = String::new();
        for (i, (_, cs, ce, _)) in lines_all.iter().enumerate() { if i > 0 { buffer.push('\n'); } buffer.push_str(&self.src[*cs..*ce]); }
        let defs = reference_definitions::paragraph_definitions(&buffer);
        if defs.is_empty() { return (None, lines_all); }
        let to_byte = |off: usize| -> usize { let mut acc = 0usize; for (_, cs, ce, _) in lines_all.iter() { let len = ce - cs; if off <= acc + len { return cs + (off - acc); } acc += len + 1; } lines_all.last().map(|l| l.2).unwrap_or(0) };
        for d in &defs {
            let (a, b) = (to_byte(d.start), to_byte(d.end));
            let end = if self.src.as_bytes().get(b) == Some(&b'\n') && b > a && b == self.li.line_end(self.li.line_of(a.max(b.saturating_sub(1))), self.src.len()) { b + 1 } else { b };
            self.definitions.push(Definition { start: a, end, label: (to_byte(d.label.0), to_byte(d.label.1)), dest: (to_byte(d.dest.0), to_byte(d.dest.1)) });
        }
        // Definitions end at a line end, so whole lines were consumed.
        let consumed_to = to_byte(defs.last().map(|d| d.end).unwrap_or(0));
        let def_lines = lines_all.iter().filter(|(_, _, ce, _)| *ce <= consumed_to).count();
        let records: Vec<_> = lines_all.iter().skip(def_lines).copied().collect();
        (Some(Shift { lines: lines_all, def_lines }), records)
    }

    fn line_bytes(&self, line0: usize) -> &'a [u8] { let ls = self.li.line_start(line0); let le = self.li.line_end(line0, self.src.len()); &self.src.as_bytes()[ls..le] }

    fn trimmed_end(&self, line0: usize, cs: usize) -> usize {
        let le = self.li.line_end(line0, self.src.len());
        let bytes = self.src.as_bytes();
        let mut ce = le; while ce > cs && (bytes[ce - 1] == b' ' || bytes[ce - 1] == b'\t' || bytes[ce - 1] == b'\r') { ce -= 1; }
        ce
    }

    fn code_block_lines(&mut self, idx: usize, c: &comrak::nodes::NodeCodeBlock, l0: usize, l1: usize, containers: &[Container]) {
        let bytes = self.src.as_bytes();
        let mut derived = String::new();
        let mut closed = false;
        // info string on the opening fence line
        if c.fenced {
            let (cur, _) = self.prefix_cursor(l0, self.line_bytes(l0), containers);
            let ls = self.li.line_start(l0); let le = self.li.line_end(l0, self.src.len());
            let mut p = ls + cur.pos; while p < le && (bytes[p] == b' ' || bytes[p] == b'\t') { p += 1; }
            while p < le && bytes[p] == c.fence_char { p += 1; }
            while p < le && (bytes[p] == b' ' || bytes[p] == b'\t') { p += 1; }
            let mut q = le; while q > p && (bytes[q - 1] == b' ' || bytes[q - 1] == b'\t') { q -= 1; }
            self.blocks[idx][block::ATTR1] = p as u32; self.blocks[idx][block::ATTR2] = q as u32;
        }
        for line0 in l0..=l1 {
            if c.fenced && line0 == l0 { continue; }
            let (mut cur, _) = self.prefix_cursor(line0, self.line_bytes(line0), containers);
            let ls = self.li.line_start(line0); let le = self.li.line_end(line0, self.src.len());
            if c.fenced && line0 == l1 && l1 > l0 {
                // closing fence: at most three spaces, fence_char × ≥ fence_length, spaces only after
                let mut probe = ColCursor { line: cur.line, pos: cur.pos, col: cur.col, virt: cur.virt };
                probe.consume_columns(3);
                let rest = &cur.line[probe.pos..];
                let n = rest.iter().take_while(|b| **b == c.fence_char).count();
                if probe.virt == 0 && n >= c.fence_length && rest[n..].iter().all(|b| *b == b' ' || *b == b'\t') { closed = true; continue; }
            }
            let remove = if c.fenced { c.fence_offset } else { 4 };
            cur.consume_columns(remove);
            let cs = ls + cur.pos;
            self.push_content(line0, cs, le, cur.virt);
            for _ in 0..cur.virt { derived.push(' '); }
            derived.push_str(&self.src[cs..le]); derived.push('\n');
        }
        if closed { self.blocks[idx][block::FLAGS] |= 2; }
        if self.collect && derived.trim_end_matches('\n') != c.literal.trim_end_matches('\n') {
            let d = format!("block {idx}: derived {:?} vs literal {:?}", derived, c.literal);
            self.dev("code-content", d);
        }
    }

    fn children_span<'b>(&self, node: &'b AstNode<'b>) -> (usize, usize) {
        let mut first = None; let mut last = None;
        for ch in node.children() {
            if let Some((cs, ce)) = sourcepos_range(ch.data.borrow().sourcepos, &self.li, self.src.len()) { if first.is_none() { first = Some(cs); } last = Some(ce); }
        }
        match (first, last) { (Some(a), Some(b)) => (a, b.max(a)), _ => (0, 0) }
    }

    // --------------------------------------------------------------- inlines

    /// A reported (1-based line, 0-based column) through the stripped-
    /// definition shift: comrak computes inline positions from the stripped
    /// content buffer but maps them through the leaf's original line table,
    /// so a position on original line `n` really lies on line `n + d`.
    fn shifted(&self, line1: usize, col0: usize, sh: &Shift) -> usize {
        let raw = (self.li.line_start(line1.saturating_sub(1)) + col0).min(self.src.len());
        if line1 == 0 { return raw; }
        let first_line = sh.lines[0].0;
        let n = (line1 - 1).saturating_sub(first_line);
        if n + sh.def_lines >= sh.lines.len() { return raw; }
        let prefix = sh.lines[n].1 - self.li.line_start(sh.lines[n].0);
        let content_col = col0.saturating_sub(prefix);
        let (target_line, cs, _, _) = sh.lines[n + sh.def_lines];
        (cs + content_col).min(self.li.line_end_with_break(target_line, self.src.len()))
    }

    /// Escaped-pipe shift inside a table cell: comrak positions inline nodes
    /// on the unescaped cell text, so every `\|` before a position pushes it
    /// one byte to the right in the source.
    fn pipe_shift(&self, byte: usize, cell: Option<(usize, usize)>) -> usize {
        let Some((cell_start, _)) = cell else { return byte; };
        let raw = self.src.as_bytes();
        let mut b = byte; let mut k0 = 0usize;
        loop {
            let end = b.min(raw.len()).max(cell_start);
            let k = raw[cell_start..end].windows(2).filter(|w| w == b"\\|").count();
            if k == k0 { break; }
            k0 = k; b = byte + k;
        }
        b
    }

    /// Source range of an inline node after both corrections.
    fn corrected_range(&self, sp: Sourcepos, cell: Option<(usize, usize)>, shift: Option<&Shift>) -> Option<(usize, usize)> {
        if sp.start.line == 0 || sp.end.line == 0 { return None; }
        let (s, e) = match shift {
            Some(sh) => (self.shifted(sp.start.line, sp.start.column.saturating_sub(1), sh), self.shifted(sp.end.line, sp.end.column, sh)),
            None => sourcepos_range(sp, &self.li, self.src.len())?,
        };
        let s = self.pipe_shift(s, cell);
        let e = self.pipe_shift(e, cell).max(s);
        Some((s.min(self.src.len()), e.min(self.src.len())))
    }

    fn walk_inline<'b>(&mut self, node: &'b AstNode<'b>, blk: u32, parent: u32, cell: Option<(usize, usize)>, shift: Option<&Shift>) {
        let data = node.data.borrow();
        let sp = data.sourcepos;
        let Some((s, e)) = self.corrected_range(sp, cell, shift) else { return; };
        let src = self.src; let bytes = src.as_bytes();
        let slice = &src[s..e];
        let mut rec: RunRec = [0; run::WORDS];
        rec[run::BLOCK] = blk; rec[run::PARENT] = parent;
        let (kind, cs, ce): (u32, usize, usize) = match &data.value {
            NodeValue::Text(t) => {
                let t: &str = &**t;
                if t == slice { (run_kind::TEXT, s, e) } else {
                    let (off, len) = self.push_string(t);
                    rec[run::AUX0] = off; rec[run::AUX1] = len;
                    if self.collect && !slice.contains('&') && !slice.contains('\t') && !slice.contains('\r') && cell.is_none() && !slice.contains('\\') {
                        self.dev("text-mismatch", format!("block {blk} {:?} vs literal {:?}", slice, t));
                    }
                    (run_kind::REPLACEMENT, s, e)
                }
            }
            NodeValue::Emph => { let ok = e > s + 1 && matches!(bytes[s], b'*' | b'_') && bytes[e - 1] == bytes[s]; if !ok { self.dev("emph-delims", format!("block {blk} {:?}", slice)); } (run_kind::EMPH, s + 1, e.saturating_sub(1).max(s + 1)) }
            NodeValue::Strong => { let ok = e >= s + 4 && matches!(bytes[s], b'*' | b'_') && bytes[s + 1] == bytes[s] && bytes[e - 1] == bytes[s] && bytes[e - 2] == bytes[s]; if !ok { self.dev("strong-delims", format!("block {blk} {:?}", slice)); } (run_kind::STRONG, s + 2, e.saturating_sub(2).max(s + 2)) }
            NodeValue::Strikethrough => { let n = if e >= s + 4 && bytes[s] == b'~' && bytes[s + 1] == b'~' { 2 } else { 1 }; let ok = e >= s + 2 * n && bytes[s] == b'~' && bytes[e - 1] == b'~'; if !ok { self.dev("strike-delims", format!("block {blk} {:?}", slice)); } (run_kind::STRIKE, s + n, e.saturating_sub(n).max(s + n)) }
            NodeValue::Code(c) => {
                let n = c.num_backticks.max(1);
                rec[run::AUX0] = n as u32;
                let ok = e >= s + 2 * n && slice.starts_with(&"`".repeat(n)) && slice.ends_with(&"`".repeat(n));
                if !ok { self.dev("code-delims", format!("block {blk} {:?}", slice)); (run_kind::CODE, s, e) } else {
                    let (mut cs, mut ce) = (s + n, e - n);
                    let raw = &src[cs..ce];
                    if raw != c.literal {
                        let stripped = if raw.len() >= 2 && raw.starts_with(' ') && raw.ends_with(' ') && !raw.trim().is_empty() { &raw[1..raw.len() - 1] } else { raw };
                        if stripped == c.literal { cs += 1; ce -= 1; }
                        else if raw.replace('\n', " ") == c.literal || raw.replace('\n', " ").trim_matches(' ') == c.literal.trim_matches(' ') {}
                        else if cell.is_some() && raw.replace("\\|", "|") == c.literal { let (off, len) = self.push_string(&c.literal); rec[run::AUX2] = off; rec[run::AUX3] = len; rec[run::FLAGS] |= 2; }
                        else { let d = format!("block {blk} {:?} vs {:?}", raw, c.literal); self.dev("code-literal", d); }
                    }
                    (run_kind::CODE, cs, ce)
                }
            }
            NodeValue::Link(_) => {
                if s < e && bytes[s] == b'[' {
                    let (cs, ce) = { let (a, b) = self.children_span_corrected(node, cell, shift); if a == 0 && b == 0 { (s + 1, s + 1) } else { (a, b) } };
                    self.link_aux(&mut rec, ce, e);
                    (run_kind::LINK, cs, ce)
                } else {
                    let (cs, ce) = (if s < e && bytes[s] == b'<' { s + 1 } else { s }, if s < e && bytes[e - 1] == b'>' { e - 1 } else { e });
                    rec[run::AUX0] = cs as u32; rec[run::AUX1] = ce as u32;
                    (run_kind::AUTOLINK, cs, ce)
                }
            }
            NodeValue::Image(_) => {
                if s < e && bytes[s] == b'!' {
                    let (cs, ce) = { let (a, b) = self.children_span_corrected(node, cell, shift); if a == 0 && b == 0 { (s + 2, s + 2) } else { (a, b) } };
                    self.link_aux(&mut rec, ce, e);
                    (run_kind::IMAGE, cs, ce)
                } else { self.dev("image-delims", format!("block {blk} {:?}", slice)); (run_kind::IMAGE, s, e) }
            }
            NodeValue::SoftBreak => (run_kind::SOFT_BREAK, e, e),
            NodeValue::LineBreak => (run_kind::HARD_BREAK, e, e),
            NodeValue::HtmlInline(_) => (run_kind::HTML_INLINE, s, e),
            NodeValue::FootnoteReference(_) => { if e > s + 3 { rec[run::AUX0] = (s + 2) as u32; rec[run::AUX1] = (e - 1) as u32; } (run_kind::FOOTNOTE_REF, s, e) }
            NodeValue::Escaped => { let (a, b) = self.children_span_corrected(node, cell, shift); if a < b { (run_kind::ESCAPE, a, b) } else { (run_kind::ESCAPE, (s + 1).min(e), e) } }
            _ => (run_kind::OTHER, s, e),
        };
        drop(data);
        let (cs, ce) = (cs.max(s).min(e), ce.max(s).min(e));
        let (cs, ce) = (cs.min(ce), ce);
        rec[run::KIND] = kind;
        rec[run::START_BYTE] = s as u32; rec[run::END_BYTE] = e as u32;
        rec[run::CONTENT_START_BYTE] = cs as u32; rec[run::CONTENT_END_BYTE] = ce as u32;
        rec[run::START_UTF16] = self.li.u16(s); rec[run::END_UTF16] = self.li.u16(e);
        rec[run::CONTENT_START_UTF16] = self.li.u16(cs); rec[run::CONTENT_END_UTF16] = self.li.u16(ce);
        if self.li.line_of(s) != self.li.line_of(e.saturating_sub(1).max(s)) { rec[run::FLAGS] |= 1 << 8; }
        let ri = self.runs.len() as u32;
        self.runs.push(rec);
        for ch in node.children() { self.walk_inline(ch, blk, ri, cell, shift); }
    }

    fn children_span_corrected<'b>(&self, node: &'b AstNode<'b>, cell: Option<(usize, usize)>, shift: Option<&Shift>) -> (usize, usize) {
        let mut first = None; let mut last = None;
        for ch in node.children() {
            if let Some((cs, ce)) = self.corrected_range(ch.data.borrow().sourcepos, cell, shift) { if first.is_none() { first = Some(cs); } last = Some(ce); }
        }
        match (first, last) { (Some(a), Some(b)) => (a, b.max(a)), _ => (0, 0) }
    }

    /// Destination and title ranges after a link's `]`, or the reference label.
    fn link_aux(&self, rec: &mut RunRec, ce: usize, e: usize) {
        let bytes = self.src.as_bytes();
        let mut p = ce;
        if p < e && bytes[p] == b']' { p += 1; }
        if p < e && bytes[p] == b'(' {
            p += 1;
            while p < e && (bytes[p] == b' ' || bytes[p] == b'\n') { p += 1; }
            let ds = p;
            if p < e && bytes[p] == b'<' { p += 1; while p < e && bytes[p] != b'>' { p += 1; } if p < e { p += 1; } }
            else { let mut depth = 0i32; while p < e && !(bytes[p] == b' ' || bytes[p] == b'\n') { if bytes[p] == b'(' { depth += 1; } if bytes[p] == b')' { if depth == 0 { break; } depth -= 1; } p += 1; } }
            let de = p;
            rec[run::AUX0] = ds as u32; rec[run::AUX1] = de as u32;
            while p < e && (bytes[p] == b' ' || bytes[p] == b'\n') { p += 1; }
            if p < e && matches!(bytes[p], b'"' | b'\'' | b'(') {
                let closer = if bytes[p] == b'(' { b')' } else { bytes[p] };
                let ts = p + 1; let mut q = ts; while q < e && bytes[q] != closer { q += 1; }
                if q < e { rec[run::AUX2] = ts as u32; rec[run::AUX3] = q as u32; rec[run::FLAGS] |= 2; }
            }
        } else {
            rec[run::FLAGS] |= 1;
            if p < e && bytes[p] == b'[' { let ls = p + 1; let mut q = ls; while q < e && bytes[q] != b']' { q += 1; } rec[run::AUX0] = ls as u32; rec[run::AUX1] = q as u32; }
        }
    }

    // ---------------------------------------------------------------- encode

    fn encode(&self) -> Vec<u8> {
        let words = schema::HEADER_WORDS + self.li.line_count() * 2 + self.blocks.len() * block::WORDS + self.content.len() * content::WORDS + self.runs.len() * run::WORDS + self.definitions.len() * definition::WORDS;
        let mut out: Vec<u8> = Vec::with_capacity(words * 4 + self.strings.len() + 4);
        let push = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
        let mut hdr = [0u32; schema::HEADER_WORDS];
        hdr[header::MAGIC] = schema::MAGIC; hdr[header::VERSION] = schema::VERSION;
        hdr[header::SRC_BYTES] = self.src.len() as u32; hdr[header::SRC_UTF16] = self.li.u16(self.src.len());
        hdr[header::LINE_COUNT] = self.li.line_count() as u32; hdr[header::BLOCK_COUNT] = self.blocks.len() as u32;
        hdr[header::CONTENT_COUNT] = self.content.len() as u32; hdr[header::RUN_COUNT] = self.runs.len() as u32;
        hdr[header::DEFINITION_COUNT] = self.definitions.len() as u32; hdr[header::STRING_BYTES] = self.strings.len() as u32;
        for v in hdr { push(&mut out, v); }
        for l in 0..self.li.line_count() { let s = self.li.line_start(l); push(&mut out, s as u32); push(&mut out, self.li.u16(s)); }
        for b in &self.blocks { for v in b { push(&mut out, *v); } }
        for c in &self.content { for v in c { push(&mut out, *v); } }
        for r in &self.runs { for v in r { push(&mut out, *v); } }
        for d in &self.definitions { for v in [d.start as u32, d.end as u32, self.li.u16(d.start), self.li.u16(d.end), d.label.0 as u32, d.label.1 as u32, d.dest.0 as u32, d.dest.1 as u32] { push(&mut out, v); } }
        out.extend_from_slice(&self.strings);
        while out.len() % 4 != 0 { out.push(0); }
        out
    }
}

impl<'a> ColCursor<'a> {
    /// Skip up to `limit` columns of leading whitespace (used for marker lines).
    fn skip_whitespace_limited(&mut self, limit: usize) { let _ = self.consume_columns(limit); }
}

fn is_block_value(v: &NodeValue) -> bool {
    !matches!(v, NodeValue::Text(_) | NodeValue::Emph | NodeValue::Strong | NodeValue::Code(_) | NodeValue::SoftBreak | NodeValue::LineBreak | NodeValue::Link(_) | NodeValue::Image(_) | NodeValue::Strikethrough | NodeValue::HtmlInline(_) | NodeValue::FootnoteReference(_) | NodeValue::Escaped | NodeValue::Superscript | NodeValue::Subscript | NodeValue::Underline | NodeValue::SpoileredText | NodeValue::Math(_) | NodeValue::WikiLink(_) | NodeValue::EscapedTag(_) | NodeValue::Raw(_))
}

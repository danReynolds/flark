//! Flat render-model extraction over an unmodified comrak AST.
//!
//! See `schema/render_model_v1.json` and `SCHEMA.md` for the layout. The
//! extraction walks the tree once, iteratively, derives what comrak does not
//! expose (per-line content ranges, reference definitions), corrects the two
//! situations where comrak's inline positions are known to be off, and in
//! report mode validates every derivation against comrak's own output.

use crate::text_pieces;
use crate::lines::LineIndex;
use crate::reference_definitions::{self, Definition};
use crate::schema::{self, block, block_kind, content, definition, header, run, run_kind, table_alignment};
use comrak::nodes::{AstNode, ListType, NodeCodeBlock, NodeValue, Sourcepos, TableAlignment};
use comrak::{parse_document, Arena, Options};

pub type BlockRec = [u32; block::WORDS];
pub type ContentRec = [u32; content::WORDS];
pub type RunRec = [u32; run::WORDS];

/// A validation finding from report mode. `rule` is a stable identifier.
#[derive(Clone, Debug)]
pub struct Deviation { pub rule: &'static str, pub detail: String }

/// Options for the render-model parse. Footnote definitions stay where they
/// are written so every source byte keeps an owner and blocks stay in
/// document order; the spec-conformance test uses `spec_options` instead.
pub fn options() -> Options<'static> {
    let mut o = Options::default();
    o.extension.table = true;
    o.extension.strikethrough = true;
    o.extension.tasklist = true;
    o.extension.autolink = true;
    o.extension.footnotes = true;
    o.parse.leave_footnote_definitions = true;
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContainerKind { Quote, Item, Footnote }

/// One container in the chain above a line. `offset` is the content column
/// relative to the enclosing container's content column.
#[derive(Clone, Copy)]
struct Container { kind: ContainerKind, offset: usize, first_line: usize, checkbox: Option<(usize, usize)> }

/// Column cursor over one physical line: spaces and tabs consumed by column,
/// with the remainder of a partially consumed tab carried as virtual spaces.
#[derive(Clone, Copy)]
struct ColCursor<'a> { line: &'a [u8], pos: usize, col: usize, virt: usize }

impl<'a> ColCursor<'a> {
    fn new(line: &'a [u8]) -> Self { ColCursor { line, pos: 0, col: 0, virt: 0 } }
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
    fn skip_whitespace(&mut self) {
        self.virt = 0;
        while matches!(self.line.get(self.pos), Some(b' ') | Some(b'\t')) { if self.line[self.pos] == b'\t' { self.col = (self.col / 4 + 1) * 4; } else { self.col += 1; } self.pos += 1; }
    }
    fn advance(&mut self, n: usize) {
        for _ in 0..n { if let Some(b) = self.line.get(self.pos) { if *b == b'\t' { self.col = (self.col / 4 + 1) * 4; } else { self.col += 1; } self.pos += 1; } }
    }
}

/// Result of consuming the container prefixes on one line.
struct Prefix<'a> { cur: ColCursor<'a>, lazy: bool, after_checkbox: bool, /// byte offset within the line where the innermost matched container's prefix begins
    prefix_start: usize }

struct Leaf<'b> { node: &'b AstNode<'b>, idx: usize, container: Option<usize> }

/// A container in the arena of containers: its data and the container above it.
struct ContainerNode { c: Container, parent: Option<usize>, block: usize }

/// Correction context for a leaf whose leading definitions comrak stripped:
/// comrak computes inline positions from the stripped content buffer but maps
/// them through the leaf's original per-line offsets, so a position reported
/// on original line `n` really lies on line `n + def_lines`.
pub struct Shift { lines: Vec<LineSpan>, def_lines: usize }

/// One line of a paragraph-like leaf as comrak buffered it: `virt` is the
/// number of virtual spaces a partially consumed tab contributed on a lazy
/// line, which exist in comrak's buffer but not in the source.
#[derive(Clone, Copy)]
struct LineSpan { line0: usize, start: usize, end: usize, virt: usize, prefix_start: usize }

pub struct Extractor<'a> {
    src: &'a str,
    li: LineIndex,
    blocks: Vec<BlockRec>,
    content: Vec<ContentRec>,
    runs: Vec<RunRec>,
    definitions: Vec<Definition>,
    strings: Vec<u8>,
    /// Arena of container blocks with parent links; chains are rebuilt on
    /// demand so deep nesting costs depth, not depth squared.
    containers: Vec<ContainerNode>,
    collect: bool,
    /// Per-leaf inline repair state: a byte offset comrak's positions are off
    /// by after a bare CR (its inline line counter does not advance there),
    /// discovered from the first mismatching Text literal and carried forward.
    run_delta: isize,
    last_text_end: usize,
    pub deviations: Vec<Deviation>,
}

impl<'a> Extractor<'a> {
    /// The render model as little-endian u32 words; the string table is
    /// packed into the tail and padded to a whole word.
    pub fn extract(src: &'a str) -> Vec<u32> { Self::run(src, false).0 }
    pub fn extract_with_report(src: &'a str) -> (Vec<u32>, Vec<Deviation>) { Self::run(src, true) }

    fn run(src: &'a str, collect: bool) -> (Vec<u32>, Vec<Deviation>) {
        let mut ex = Extractor { src, li: LineIndex::new(src), blocks: Vec::new(), content: Vec::new(), runs: Vec::new(), definitions: Vec::new(), strings: Vec::new(), containers: Vec::new(), collect, run_delta: 0, last_text_end: 0, deviations: Vec::new() };
        let arena = Arena::new();
        let root = parse_document(&arena, src, &options());
        let leaves = ex.walk_blocks(root);
        for leaf in &leaves { let chain = ex.chain(leaf.container); ex.leaf(leaf, &chain); }
        ex.widen_parents();
        ex.gap_definitions(&leaves);
        ex.definitions.sort_by_key(|d| d.start);
        ex.definitions.dedup_by_key(|d| d.start);
        let buf = ex.encode();
        (buf, ex.deviations)
    }

    fn dev(&mut self, rule: &'static str, detail: impl FnOnce() -> String) { if self.collect { let d = detail(); self.deviations.push(Deviation { rule, detail: d }); } }

    fn push_string(&mut self, s: &str) -> (u32, u32) { let off = self.strings.len() as u32; self.strings.extend_from_slice(s.as_bytes()); (off, s.len() as u32) }

    /// A source range whose ends are snapped to scalar boundaries. comrak's
    /// columns can land inside a scalar (its end column drifts across CR line
    /// endings); the kind-specific checks below, delimiters and literals, are
    /// the oracle for the resulting position, so snapping itself is silent.
    fn slice(&mut self, s: usize, e: usize) -> (usize, usize) {
        let (mut a, mut b) = (s.min(self.src.len()), e.min(self.src.len()));
        while a > 0 && !self.src.is_char_boundary(a) { a -= 1; }
        while b < self.src.len() && !self.src.is_char_boundary(b) { b += 1; }
        (a, b.max(a))
    }

    // ---------------------------------------------------------------- blocks

    /// Pass 1: block records and container chains, iteratively. Returns the
    /// leaves in document order with the container chain above each.
    fn walk_blocks<'b>(&mut self, root: &'b AstNode<'b>) -> Vec<Leaf<'b>> {
        let mut leaves = Vec::new();
        // (node, parent block, innermost container above it)
        let mut stack: Vec<(&'b AstNode<'b>, u32, Option<usize>)> = vec![(root, u32::MAX, None)];
        while let Some((node, parent, container_id)) = stack.pop() {
            let (idx, container, is_leaf) = self.block_record(node, parent, container_id);
            let inner = match container { Some(c) => { self.containers.push(ContainerNode { c, parent: container_id, block: idx }); Some(self.containers.len() - 1) } None => container_id };
            if is_leaf {
                leaves.push(Leaf { node, idx, container: inner });
            } else {
                let children: Vec<_> = node.children().collect();
                for child in children.into_iter().rev() {
                    if child.data.borrow().value.block() { stack.push((child, idx as u32, inner)); }
                    else { leaves.push(Leaf { node: child, idx, container: inner }); }
                }
            }
        }
        leaves.sort_by_key(|l| l.idx);
        leaves
    }

    /// The containers above `id`, outermost first.
    fn chain(&self, id: Option<usize>) -> Vec<Container> { let mut v = Vec::new(); self.chain_into(id, &mut v); v }

    fn chain_into(&self, id: Option<usize>, out: &mut Vec<Container>) {
        out.clear();
        let mut cur = id;
        while let Some(i) = cur { out.push(self.containers[i].c); cur = self.containers[i].parent; }
        out.reverse();
    }

    /// comrak's container ranges can stop short of a lazily continued child
    /// (and a row short of its last cell); the schema requires containment,
    /// so parents grow to cover their children, innermost first.
    fn widen_parents(&mut self) {
        for i in (1..self.blocks.len()).rev() {
            let p = self.blocks[i][block::PARENT];
            if p == u32::MAX { continue; }
            let (s, e) = (self.blocks[i][block::START_BYTE], self.blocks[i][block::END_BYTE]);
            let (s16, e16) = (self.blocks[i][block::START_UTF16], self.blocks[i][block::END_UTF16]);
            let parent = &mut self.blocks[p as usize];
            if s < parent[block::START_BYTE] { parent[block::START_BYTE] = s; parent[block::START_UTF16] = s16; }
            if e > parent[block::END_BYTE] { parent[block::END_BYTE] = e; parent[block::END_UTF16] = e16; }
        }
    }

    fn block_record<'b>(&mut self, node: &'b AstNode<'b>, parent: u32, container_id: Option<usize>) -> (usize, Option<Container>, bool) {
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
        let is_leaf = data.value.contains_inlines() || matches!(data.value, NodeValue::CodeBlock(_) | NodeValue::HtmlBlock(_) | NodeValue::ThematicBreak);
        match &data.value {
            NodeValue::Document => rec[block::KIND] = block_kind::DOCUMENT,
            NodeValue::Paragraph => rec[block::KIND] = block_kind::PARAGRAPH,
            NodeValue::Heading(h) => { rec[block::KIND] = block_kind::HEADING; rec[block::ATTR0] = h.level as u32; rec[block::FLAGS] = h.setext as u32; }
            NodeValue::CodeBlock(c) => { rec[block::KIND] = block_kind::CODE_BLOCK; rec[block::ATTR0] = if c.fenced { c.fence_length as u32 } else { 0 }; rec[block::FLAGS] = (c.fenced as u32) | ((c.fenced && c.closed) as u32) << 1; }
            NodeValue::HtmlBlock(_) => rec[block::KIND] = block_kind::HTML_BLOCK,
            NodeValue::BlockQuote => { rec[block::KIND] = block_kind::BLOCK_QUOTE; container = Some(Container { kind: ContainerKind::Quote, offset: 0, first_line, checkbox: None }); }
            NodeValue::List(l) => { rec[block::KIND] = block_kind::LIST; rec[block::ATTR0] = matches!(l.list_type, ListType::Ordered) as u32; rec[block::ATTR1] = l.start as u32; rec[block::FLAGS] = l.tight as u32; }
            NodeValue::Item(l) => { rec[block::KIND] = block_kind::ITEM; rec[block::ATTR0] = (l.marker_offset + l.padding) as u32; container = Some(Container { kind: ContainerKind::Item, offset: l.marker_offset + l.padding, first_line, checkbox: None }); }
            NodeValue::TaskItem(t) => {
                rec[block::KIND] = block_kind::ITEM;
                rec[block::FLAGS] = 1 | ((t.symbol.is_some() as u32) << 1);
                // comrak's symbol position is subject to its stripped-definition
                // drift; trust it only when the bytes there are the checkbox, else
                // find the checkbox in the item's source.
                let symbol = t.symbol.unwrap_or(' ');
                let bytes = self.src.as_bytes();
                let looks_like_checkbox = |a: usize| a >= 1 && a + 2 <= bytes.len() && bytes[a - 1] == b'[' && bytes[a + 1] == b']' && (bytes[a] as char == symbol || (symbol == ' ' && bytes[a] == b' '));
                let claimed = sourcepos_range(t.symbol_sourcepos, &self.li, self.src.len()).map(|(ss, _)| ss).filter(|ss| looks_like_checkbox(*ss));
                let found = claimed.or_else(|| (start..end.min(bytes.len())).find(|a| looks_like_checkbox(*a)));
                let mut checkbox = None;
                if let Some(ss) = found { rec[block::ATTR1] = ss as u32; rec[block::ATTR2] = (ss + 1) as u32; checkbox = Some((ss - 1, ss + 2)); }
                // comrak reports no NodeList for task items: the content column
                // is derived from the marker line.
                let chain = self.chain(container_id);
                let offset = self.marker_content_offset(first_line, &chain);
                rec[block::ATTR0] = offset as u32;
                container = Some(Container { kind: ContainerKind::Item, offset, first_line, checkbox });
            }
            NodeValue::ThematicBreak => rec[block::KIND] = block_kind::THEMATIC_BREAK,
            NodeValue::Table(t) => {
                rec[block::KIND] = block_kind::TABLE; rec[block::ATTR0] = t.num_columns as u32;
                let mut packed = 0u32;
                for (i, a) in t.alignments.iter().enumerate().take(16) { let v = match a { TableAlignment::None => table_alignment::NONE, TableAlignment::Left => table_alignment::LEFT, TableAlignment::Center => table_alignment::CENTER, TableAlignment::Right => table_alignment::RIGHT }; packed |= v << (2 * i); }
                rec[block::ATTR1] = packed;
                if t.alignments.len() > 16 && t.alignments[16..].iter().any(|a| !matches!(a, TableAlignment::None)) { self.dev("table-alignment-cap", || format!("{} columns; alignments beyond 16 dropped", t.alignments.len())); }
            }
            NodeValue::TableRow(h) => { rec[block::KIND] = block_kind::TABLE_ROW; rec[block::FLAGS] = *h as u32; }
            NodeValue::TableCell => rec[block::KIND] = block_kind::TABLE_CELL,
            NodeValue::FootnoteDefinition(_) => {
                rec[block::KIND] = block_kind::FOOTNOTE_DEFINITION;
                let ls = self.li.line_start(first_line); let le = self.li.line_end(first_line, self.src.len());
                let line = &self.src.as_bytes()[ls..le];
                if let Some(o) = line.iter().position(|b| *b == b'[') { if line.get(o + 1) == Some(&b'^') { if let Some(c) = line[o..].iter().position(|b| *b == b']') { rec[block::ATTR1] = (ls + o + 2) as u32; rec[block::ATTR2] = (ls + o + c) as u32; } } }
                container = Some(Container { kind: ContainerKind::Footnote, offset: 4, first_line, checkbox: None });
            }
            _ => rec[block::KIND] = block_kind::OTHER,
        }
        drop(data);
        let idx = self.blocks.len();
        self.blocks.push(rec);
        (idx, container, is_leaf)
    }

    /// Content column of a task item relative to its enclosing container: the
    /// column of the checkbox on the marker line.
    fn marker_content_offset(&self, line0: usize, containers: &[Container]) -> usize {
        let line = self.line_bytes(line0);
        let p = self.prefix_cursor(line0, line, containers);
        let base = p.cur.col;
        let mut cur = p.cur;
        cur.consume_columns(3);
        while matches!(cur.line.get(cur.pos), Some(b) if b.is_ascii_digit()) { cur.advance(1); }
        if matches!(cur.line.get(cur.pos), Some(b'-') | Some(b'+') | Some(b'*') | Some(b'.') | Some(b')')) { cur.advance(1); }
        cur.consume_columns(usize::MAX.min(5));
        cur.col.saturating_sub(base).max(2)
    }

    // ------------------------------------------------------------- per line

    /// Consume container prefixes on one physical line.
    fn prefix_cursor<'l>(&self, line0: usize, line: &'l [u8], containers: &[Container]) -> Prefix<'l> {
        let mut cur = ColCursor::new(line);
        let mut lazy = false;
        let mut after_checkbox = false;
        let mut base = 0usize;
        let mut prefix_start = 0usize;
        for c in containers {
            let at = cur.pos;
            match c.kind {
                ContainerKind::Quote => {
                    let save = cur;
                    cur.consume_columns(3);
                    if cur.virt == 0 && cur.line.get(cur.pos) == Some(&b'>') {
                        cur.advance(1);
                        match cur.line.get(cur.pos) {
                            Some(b' ') => cur.advance(1),
                            Some(b'\t') => { let stop = (cur.col / 4 + 1) * 4; let width = stop - cur.col; cur.pos += 1; cur.col = stop; cur.virt = width - 1; }
                            _ => {}
                        }
                        base = cur.col - cur.virt;
                        prefix_start = at;
                    } else { cur = save; lazy = true; break; }
                }
                ContainerKind::Item => {
                    let target = base + c.offset;
                    if c.first_line == line0 {
                        cur.consume_columns(3);
                        while matches!(cur.line.get(cur.pos), Some(b) if b.is_ascii_digit()) { cur.advance(1); }
                        if matches!(cur.line.get(cur.pos), Some(b'-') | Some(b'+') | Some(b'*') | Some(b'.') | Some(b')')) { cur.advance(1); }
                        let need = target.saturating_sub(cur.col - cur.virt);
                        cur.consume_columns(need);
                    } else {
                        // cmark advances past the indentation only when all of
                        // it is there; a lazy line keeps its partial spaces.
                        let save = cur;
                        let need = target.saturating_sub(cur.col - cur.virt);
                        let got = cur.consume_columns(need);
                        if got < need { cur = save; lazy = true; break; }
                    }
                    prefix_start = at;
                    // The task checkbox is skipped on whichever line comrak found it
                    // (the item's first paragraph line), plus one space or tab.
                    if let Some((cb_start, cb_end)) = c.checkbox {
                        if self.li.line_start(line0) + cur.pos == cb_start {
                            cur.advance(cb_end - cb_start);
                            if matches!(cur.line.get(cur.pos), Some(b' ') | Some(b'\t')) { cur.advance(1); }
                            after_checkbox = true;
                        }
                    }
                    base = target;
                }
                ContainerKind::Footnote => {
                    if c.first_line == line0 {
                        cur.consume_columns(3);
                        if cur.virt == 0 && cur.line.get(cur.pos) == Some(&b'[') {
                            while let Some(b) = cur.line.get(cur.pos) { let done = *b == b']'; cur.advance(1); if done { break; } }
                            if cur.line.get(cur.pos) == Some(&b':') { cur.advance(1); }
                            cur.skip_whitespace();
                        }
                        base = cur.col;
                        prefix_start = at;
                    } else {
                        let target = base + 4;
                        let save = cur;
                        let need = target.saturating_sub(cur.col - cur.virt);
                        let got = cur.consume_columns(need);
                        if got < need { cur = save; lazy = true; break; }
                        base = target;
                        prefix_start = at;
                    }
                }
            }
        }
        if lazy { prefix_start = cur.pos; }
        Prefix { cur, lazy, after_checkbox, prefix_start }
    }

    fn line_bytes(&self, line0: usize) -> &'a [u8] { let ls = self.li.line_start(line0); let le = self.li.line_end(line0, self.src.len()); &self.src.as_bytes()[ls..le] }

    fn trimmed_end(&self, line0: usize, cs: usize) -> usize {
        let le = self.li.line_end(line0, self.src.len());
        let bytes = self.src.as_bytes();
        let mut ce = le; while ce > cs && (bytes[ce - 1] == b' ' || bytes[ce - 1] == b'\t') { ce -= 1; }
        ce
    }

    /// Content span of a paragraph-like line as comrak's buffer sees it:
    /// container prefixes consumed, then leading whitespace skipped for a
    /// line whose prefixes all matched (cmark advances to the first non-space
    /// before adding it), but kept on a lazy continuation line and after a
    /// task checkbox, where cmark adds the line as-is. Trailing whitespace is
    /// trimmed.
    fn paragraph_line(&self, line0: usize, containers: &[Container]) -> LineSpan {
        let mut p = self.prefix_cursor(line0, self.line_bytes(line0), containers);
        if !p.lazy && !p.after_checkbox { p.cur.skip_whitespace(); }
        let start = self.li.line_start(line0) + p.cur.pos;
        LineSpan { line0, start, end: self.trimmed_end(line0, start), virt: if p.lazy { p.cur.virt } else { 0 }, prefix_start: self.li.line_start(line0) + p.prefix_start }
    }

    fn push_content(&mut self, line0: usize, cs: usize, ce: usize, virt: usize, prefix_start: usize) {
        let ce = ce.max(cs);
        let ps = prefix_start.min(cs);
        self.content.push([line0 as u32, cs as u32, self.li.u16(cs), ce as u32, self.li.u16(ce), virt as u32, ps as u32, self.li.u16(ps)]);
    }

    fn leaf(&mut self, leaf: &Leaf<'_>, chain: &[Container]) {
        let idx = leaf.idx;
        let data = leaf.node.data.borrow();
        let sp = data.sourcepos;
        let content_start = self.content.len() as u32;
        let first_run = self.runs.len();
        self.blocks[idx][block::CONTENT_OFFSET] = content_start;
        if sp.start.line == 0 { return; }
        let (l0, l1) = (sp.start.line - 1, sp.end.line - 1);
        let kind = self.blocks[idx][block::KIND];
        match &data.value {
            NodeValue::CodeBlock(c) => { self.code_block_lines(idx, c, l0, l1, chain); }
            NodeValue::HtmlBlock(_) => {
                let (bs, be) = (self.blocks[idx][block::START_BYTE] as usize, self.blocks[idx][block::END_BYTE] as usize);
                for line0 in l0..=l1 {
                    let p = self.prefix_cursor(line0, self.line_bytes(line0), chain);
                    let ls = self.li.line_start(line0);
                    let cs = (ls + p.cur.pos).max(if line0 == l0 { bs } else { 0 });
                    let le = self.li.line_end(line0, self.src.len());
                    if cs > be { break; }
                    self.push_content(line0, cs, le.min(be.max(cs)), p.cur.virt, ls + p.prefix_start);
                }
            }
            NodeValue::ThematicBreak => {}
            NodeValue::TableCell => {
                let (cs, ce) = sourcepos_range(sp, &self.li, self.src.len()).unwrap_or((0, 0));
                let bytes = self.src.as_bytes();
                let (mut a, mut b) = (cs, ce);
                while a < b && bytes[a] == b' ' { a += 1; }
                while b > a && bytes[b - 1] == b' ' { b -= 1; }
                let rec_index = self.content.len();
                self.push_content(l0, a, b, 0, a);
                self.walk_inlines(leaf.node, idx as u32, Some((cs, ce)), None);
                // In a pipeless table comrak offsets a body row's cells by the
                // header row's indentation; the cell's runs, repaired by their
                // literals, are the reliable extent of its content.
                if self.runs.len() > first_run {
                    let (mut lo, mut hi) = (usize::MAX, 0usize);
                    for r in &self.runs[first_run..] { lo = lo.min(r[run::START_BYTE] as usize); hi = hi.max(r[run::END_BYTE] as usize); }
                    let rec = &mut self.content[rec_index];
                    if lo < rec[content::START_BYTE] as usize || hi > rec[content::END_BYTE] as usize || (rec[content::START_BYTE] as usize) < lo && lo - (rec[content::START_BYTE] as usize) <= 2 {
                        rec[content::START_BYTE] = lo as u32; rec[content::START_UTF16] = self.li.u16(lo);
                        rec[content::END_BYTE] = hi as u32; rec[content::END_UTF16] = self.li.u16(hi);
                    }
                }
            }
            NodeValue::Heading(h) if !h.setext => {
                let (span_s, span_e) = self.children_span(leaf.node, None, None);
                let mut span = self.paragraph_line(l0, chain);
                let bytes = self.src.as_bytes();
                let (mut cs, mut ce) = (span.start, span.end);
                let mut p = cs; while p < ce && bytes[p] == b'#' { p += 1; }
                while p < ce && (bytes[p] == b' ' || bytes[p] == b'\t') { p += 1; }
                cs = p;
                let mut q = ce; while q > cs && bytes[q - 1] == b'#' { q -= 1; }
                if q < ce && (q == cs || bytes[q - 1] == b' ' || bytes[q - 1] == b'\t') { ce = q; while ce > cs && (bytes[ce - 1] == b' ' || bytes[ce - 1] == b'\t') { ce -= 1; } }
                if span_s < span_e { if span_s < cs || span_e > ce { self.dev("heading-content", || format!("block {idx}: children {span_s}..{span_e} outside derived {cs}..{ce}")); } }
                span.start = cs; span.end = ce;
                self.push_content(l0, span.start, span.end, 0, span.prefix_start);
                self.walk_inlines(leaf.node, idx as u32, None, None);
            }
            NodeValue::Heading(_) | NodeValue::Paragraph => {
                // A setext heading's underline is hidden: no content record.
                let last = if kind == block_kind::HEADING { l1.saturating_sub(1).max(l0) } else { l1 };
                // comrak does not resolve definitions in a paragraph it split to
                // make a table header (the remainder is re-created as a new node).
                let split_by_table = kind == block_kind::PARAGRAPH && self.blocks.get(idx + 1).map_or(false, |b| b[block::KIND] == block_kind::TABLE && b[block::FIRST_LINE] as usize == l1 + 1);
                let (shift, records) = self.strip_definitions(l0, last, chain, !split_by_table);
                for r in records { self.push_content(r.line0, r.start, r.end, 0, r.prefix_start); }
                // That split paragraph also had its pipes unescaped, so its inline
                // positions carry the same shift as a table cell's.
                let pipes = if split_by_table { Some((self.blocks[idx][block::START_BYTE] as usize, self.blocks[idx][block::END_BYTE] as usize)) } else { None };
                self.walk_inlines(leaf.node, idx as u32, pipes, shift.as_ref());
            }
            _ => { self.walk_inlines(leaf.node, idx as u32, None, None); }
        }
        drop(data);
        self.blocks[idx][block::CONTENT_COUNT] = self.content.len() as u32 - content_start;
        self.fit_block_to_content(idx);
        self.fit_block_to_runs(idx, first_run);
    }

    /// comrak's block end column can fall short of its last inline across CR
    /// line endings; the schema requires containment, so the block follows
    /// its runs. Outside CR input that is a derivation bug and is reported.
    fn fit_block_to_runs(&mut self, idx: usize, first_run: usize) {
        if first_run >= self.runs.len() { return; }
        let (mut lo, mut hi) = (usize::MAX, 0usize);
        for r in &self.runs[first_run..] { lo = lo.min(r[run::START_BYTE] as usize); hi = hi.max(r[run::END_BYTE] as usize); }
        let (bs, be) = (self.blocks[idx][block::START_BYTE] as usize, self.blocks[idx][block::END_BYTE] as usize);
        if lo >= bs && hi <= be { return; }
        if !self.src.contains('\r') { self.dev("block-range", || format!("block {idx} {bs}..{be} narrower than its runs {lo}..{hi}")); }
        let b = &mut self.blocks[idx];
        if lo < bs { b[block::START_BYTE] = lo as u32; b[block::START_UTF16] = self.li.u16(lo); }
        if hi > be { b[block::END_BYTE] = hi as u32; b[block::END_UTF16] = self.li.u16(hi); }
    }

    /// comrak reports a one-byte sourcepos for indented code blocks inside
    /// containers; the derived content is validated against the literal, so
    /// the block range follows it there. Anywhere else, content outside the
    /// block range is a derivation bug: reported, then widened so the schema
    /// invariant holds.
    fn fit_block_to_content(&mut self, idx: usize) {
        let (co, cn) = (self.blocks[idx][block::CONTENT_OFFSET] as usize, self.blocks[idx][block::CONTENT_COUNT] as usize);
        if cn == 0 { return; }
        let first = self.content[co][content::START_BYTE] as usize;
        let last = self.content[co + cn - 1][content::END_BYTE] as usize;
        // Line coverage follows the content records too (an unclosed fence's
        // literal can run past comrak's end line).
        let last_line = self.content[co + cn - 1][content::LINE];
        let fl = self.blocks[idx][block::FIRST_LINE];
        if last_line >= fl && last_line + 1 - fl > self.blocks[idx][block::LINE_COUNT] { self.blocks[idx][block::LINE_COUNT] = last_line + 1 - fl; }
        let (bs, be) = (self.blocks[idx][block::START_BYTE] as usize, self.blocks[idx][block::END_BYTE] as usize);
        if first >= bs && last <= be { return; }
        let kind = self.blocks[idx][block::KIND];
        // Code blocks follow their validated literal; CR line endings make
        // comrak's block columns unreliable (registered). Anything else is a
        // derivation bug worth reporting.
        if kind != block_kind::CODE_BLOCK && !self.src.contains('\r') {
            self.dev("block-range", || format!("block {idx} kind {kind} {bs}..{be} narrower than content {first}..{last}"));
        }
        let mut i = idx;
        loop {
            let b = &mut self.blocks[i];
            if (b[block::START_BYTE] as usize) > first { b[block::START_BYTE] = first as u32; b[block::START_UTF16] = self.li.u16(first); }
            if (b[block::END_BYTE] as usize) < last { b[block::END_BYTE] = last as u32; b[block::END_UTF16] = self.li.u16(last); }
            if b[block::PARENT] == u32::MAX { break; }
            i = b[block::PARENT] as usize;
        }
    }

    fn code_block_lines(&mut self, idx: usize, c: &NodeCodeBlock, l0: usize, l1: usize, containers: &[Container]) {
        let bytes = self.src.as_bytes();
        let mut derived = String::new();
        if c.fenced {
            let p = self.prefix_cursor(l0, self.line_bytes(l0), containers);
            let ls = self.li.line_start(l0); let le = self.li.line_end(l0, self.src.len());
            let mut q = ls + p.cur.pos; while q < le && (bytes[q] == b' ' || bytes[q] == b'\t') { q += 1; }
            while q < le && bytes[q] == c.fence_char { q += 1; }
            while q < le && (bytes[q] == b' ' || bytes[q] == b'\t') { q += 1; }
            let mut r = le; while r > q && (bytes[r - 1] == b' ' || bytes[r - 1] == b'\t') { r -= 1; }
            self.blocks[idx][block::ATTR1] = q as u32; self.blocks[idx][block::ATTR2] = r as u32;
        }
        // comrak places the block's end line on the closing fence exactly when
        // the block is fenced and closed; the opening fence is always line l0.
        // Its end line can also fall short of the literal for an unclosed fence
        // inside a container; the literal's line count is authoritative.
        let lit = c.literal.replace("\r\n", "\n").replace('\r', "\n");
        let literal_lines = if lit.is_empty() { 0 } else { lit.matches('\n').count() + usize::from(!lit.ends_with('\n')) };
        let first = if c.fenced { l0 + 1 } else { l0 };
        // For a closed fence the content is exactly the lines between the
        // fences. Otherwise comrak's end line can run short or long (trailing
        // blank lines of a container); the literal's line count is exact.
        let last = if c.fenced && c.closed && l1 > l0 { l1 - 1 } else if literal_lines == 0 { first.wrapping_sub(1) } else { (first + literal_lines - 1).min(self.li.line_count() - 1) };
        let l1 = l1.max(last);
        for line0 in first..=last {
            if last < first || line0 > l1 { break; }
            let mut p = self.prefix_cursor(line0, self.line_bytes(line0), containers);
            let ls = self.li.line_start(line0); let le = self.li.line_end(line0, self.src.len());
            let remove = if c.fenced { c.fence_offset } else { 4 };
            p.cur.consume_columns(remove);
            let cs = ls + p.cur.pos;
            self.push_content(line0, cs, le, p.cur.virt, ls + p.prefix_start);
            if self.collect { for _ in 0..p.cur.virt { derived.push(' '); } derived.push_str(&self.src[cs..le]); derived.push('\n'); }
        }
        // comrak keeps CR line endings inside code literals; content records end
        // before any terminator, so compare with line endings normalized.
        let literal_normalized = c.literal.replace("\r\n", "\n").replace('\r', "\n");
        if self.collect && derived.trim_end_matches('\n') != literal_normalized.trim_end_matches('\n') {
            let literal = c.literal.clone();
            self.dev("code-content", || format!("block {idx}: derived {:?} vs literal {:?}", derived, literal));
        }
    }

    /// Per-line content for a paragraph-like leaf over lines l0..=l1, with the
    /// definitions comrak consumed from its start removed (mirroring
    /// `resolve_reference_link_definitions`).
    fn strip_definitions(&mut self, l0: usize, l1: usize, containers: &[Container], allow: bool) -> (Option<Shift>, Vec<LineSpan>) {
        let lines: Vec<LineSpan> = (l0..=l1).map(|line0| self.paragraph_line(line0, containers)).collect();
        let first_byte = lines.first().and_then(|l| self.src.as_bytes().get(l.start)).copied();
        let defs = if allow && first_byte == Some(b'[') { reference_definitions::paragraph_definitions(&lines_text(self.src, &lines)) } else { Vec::new() };
        let def_lines = if defs.is_empty() { 0 } else { self.record_buffer_definitions(&lines, &defs, false) };
        let records = lines[def_lines..].to_vec();
        let needs_geometry = def_lines > 0 || lines.iter().any(|l| l.virt > 0);
        (if needs_geometry { Some(Shift { lines, def_lines }) } else { None }, records)
    }

    /// Map definitions found in a line buffer back to source and record them.
    /// Returns how many whole lines the definitions consumed.
    fn record_buffer_definitions(&mut self, lines: &[LineSpan], defs: &[reference_definitions::BufferDefinition], exact: bool) -> usize {
        let to_byte = |off: usize| -> usize {
            let mut acc = 0usize;
            for l in lines { let len = l.end - l.start; if off <= acc + len { return l.start + (off - acc); } acc += len + 1; }
            lines.last().map(|l| l.end).unwrap_or(0)
        };
        let buffer_len: usize = lines.iter().map(|l| l.end - l.start + 1).sum::<usize>();
        let mut consumed_lines = 0usize;
        for d in defs {
            // A definition ends at a line end; its source range runs through
            // that line's terminator, never into the next line's prefix.
            let mut acc = 0usize; let mut last_line = 0usize;
            for (i, l) in lines.iter().enumerate() { let len = l.end - l.start; if d.end <= acc + len + 1 { last_line = i; break; } acc += len + 1; last_line = i; }
            let start = to_byte(d.start);
            let end = self.li.line_end_with_break(lines[last_line].line0, self.src.len());
            self.definitions.push(Definition { start, end, label: (to_byte(d.label.0), to_byte(d.label.1)), dest: (to_byte(d.dest.0), to_byte(d.dest.1)) });
            consumed_lines = last_line + 1;
        }
        if exact {
            let tail = defs.last().map(|d| d.end).unwrap_or(0);
            let rest = &lines_text(self.src, lines)[tail.min(buffer_len)..];
            if !rest.bytes().all(|b| b == b' ' || b == b'\t' || b == b'\n') {
                let shown = rest.to_string();
                self.dev("definition-gap", || format!("lines without a block only partly parse as definitions; remainder {:?}", shown));
            }
        }
        consumed_lines
    }

    /// Pass 3: definitions comrak consumed whole. A paragraph made only of
    /// definitions leaves no node, so its lines belong to no leaf block; every
    /// such run inside a container is parsed with comrak's own definition rule.
    fn gap_definitions(&mut self, leaves: &[Leaf<'_>]) {
        let line_count = self.li.line_count();
        let mut covered = vec![false; line_count];
        let mark = |b: &BlockRec, covered: &mut Vec<bool>| { let (fl, n) = (b[block::FIRST_LINE] as usize, b[block::LINE_COUNT] as usize); for l in fl..(fl + n).min(line_count) { covered[l] = true; } };
        for leaf in leaves { mark(&self.blocks[leaf.idx], &mut covered); }
        for b in &self.blocks { if b[block::KIND] == block_kind::TABLE { mark(b, &mut covered); } }
        if covered.iter().all(|c| *c) { return; }
        // Innermost container per line: later containers are deeper.
        let mut owner: Vec<Option<usize>> = vec![None; line_count];
        for (i, cn) in self.containers.iter().enumerate() {
            let b = &self.blocks[cn.block];
            let (fl, n) = (b[block::FIRST_LINE] as usize, b[block::LINE_COUNT] as usize);
            for l in fl..(fl + n).min(line_count) { owner[l] = Some(i); }
        }
        let mut line0 = 0usize;
        while line0 < line_count {
            if covered[line0] { line0 += 1; continue; }
            let scope = owner[line0];
            let chain = self.chain(scope);
            let span = self.paragraph_line(line0, &chain);
            if span.start >= span.end || self.prefix_cursor(line0, self.line_bytes(line0), &chain).lazy { line0 += 1; continue; }
            // A removed paragraph continues through lazy lines like any other,
            // so the run takes every following uncovered non-blank line.
            let mut lines = vec![span];
            let mut l = line0 + 1;
            while l < line_count && !covered[l] {
                // A lazy line's innermost container is the scope itself or one
                // above it; a line owned by another container (an empty item, say)
                // starts something else.
                let mut related = owner[l] == scope;
                let mut up = scope;
                while !related { match up { Some(i) => { up = self.containers[i].parent; related = owner[l] == up; } None => break } }
                if !related { break; }
                let sp = self.paragraph_line(l, &chain);
                if sp.start >= sp.end { break; }
                lines.push(sp); l += 1;
            }
            let buffer = lines_text(self.src, &lines);
            let defs = reference_definitions::paragraph_definitions(&buffer);
            if defs.is_empty() {
                self.dev("uncovered-lines", || format!("lines {line0}..{l} belong to no block and are not definitions: {:?}", buffer));
            } else {
                self.record_buffer_definitions(&lines, &defs, true);
            }
            line0 = l;
        }
    }

    fn children_span<'b>(&mut self, node: &'b AstNode<'b>, cell: Option<(usize, usize)>, shift: Option<&Shift>) -> (usize, usize) {
        let mut first = None; let mut last = None;
        for ch in node.children() {
            if let Some((cs, ce)) = self.corrected_range(ch.data.borrow().sourcepos, cell, shift) { if first.is_none() { first = Some(cs); } last = Some(ce); }
        }
        match (first, last) { (Some(a), Some(b)) => (a, b.max(a)), _ => (0, 0) }
    }

    // --------------------------------------------------------------- inlines

    /// A reported (1-based line, 0-based column) mapped to a source byte:
    /// comrak's column is the leaf's original per-line offset plus the offset
    /// into its buffered line, which starts with any virtual spaces of a
    /// partially consumed tab; the buffered line lives `def_lines` further
    /// down when definitions were stripped.
    fn shifted(&self, line1: usize, col0: usize, sh: &Shift) -> usize {
        let raw = (self.li.line_start(line1.saturating_sub(1)) + col0).min(self.src.len());
        if line1 == 0 { return raw; }
        let first_line = sh.lines[0].line0;
        let n = (line1 - 1).saturating_sub(first_line);
        if n + sh.def_lines >= sh.lines.len() { return raw; }
        let prefix = sh.lines[n].start - self.li.line_start(sh.lines[n].line0);
        let content_col = col0.saturating_sub(prefix);
        let target = sh.lines[n + sh.def_lines];
        (target.start + content_col.saturating_sub(target.virt)).min(self.li.line_end_with_break(target.line0, self.src.len()))
    }

    /// Bytes comrak removed from a table cell's raw text before `upto`: each
    /// `\|` whose backslash is not itself escaped loses the backslash.
    fn pipes_removed_before(&self, cell_start: usize, upto: usize) -> usize {
        let raw = &self.src.as_bytes()[cell_start..upto.min(self.src.len()).max(cell_start)];
        let mut removed = 0; let mut last_was_backslash = false;
        for &b in raw {
            if last_was_backslash { if b == b'|' { removed += 1; } last_was_backslash = false; }
            else if b == b'\\' { last_was_backslash = true; }
        }
        removed
    }

    fn pipe_shift(&self, byte: usize, cell: Option<(usize, usize)>) -> usize {
        let Some((cell_start, _)) = cell else { return byte; };
        let mut b = byte; let mut k0 = 0usize;
        loop {
            // A backslash at b-1 whose pipe sits at b was removed too: count through b+1.
            let k = self.pipes_removed_before(cell_start, b + 1);
            if k == k0 { break; }
            k0 = k; b = byte + k;
        }
        b
    }

    fn corrected_range(&mut self, sp: Sourcepos, cell: Option<(usize, usize)>, shift: Option<&Shift>) -> Option<(usize, usize)> {
        if sp.start.line == 0 || sp.end.line == 0 { return None; }
        let (s, e) = match shift {
            Some(sh) => (self.shifted(sp.start.line, sp.start.column.saturating_sub(1), sh), self.shifted(sp.end.line, sp.end.column, sh)),
            None => sourcepos_range(sp, &self.li, self.src.len())?,
        };
        let s = self.pipe_shift(s, cell);
        let e = self.pipe_shift(e, cell).max(s);
        let (s, e) = ((s as isize + self.run_delta).max(0) as usize, (e as isize + self.run_delta).max(0) as usize);
        Some(self.slice(s, e))
    }

    /// Pass 2 inline walk, iterative: runs in document order, contiguous per block.
    fn walk_inlines<'b>(&mut self, leaf: &'b AstNode<'b>, blk: u32, cell: Option<(usize, usize)>, shift: Option<&Shift>) {
        let content_from = self.blocks[blk as usize][block::CONTENT_OFFSET] as usize;
        self.run_delta = 0;
        // Allow a small backward window: comrak can also place a run too far right.
        self.last_text_end = (self.blocks[blk as usize][block::START_BYTE] as usize).saturating_sub(8);
        let first_run = self.runs.len();
        let mut stack: Vec<(&'b AstNode<'b>, u32)> = leaf.children().collect::<Vec<_>>().into_iter().rev().map(|n| (n, u32::MAX)).collect();
        while let Some((node, parent)) = stack.pop() {
            if let Some(ri) = self.inline_record(node, blk, parent, cell, shift, content_from) {
                let children: Vec<_> = node.children().collect();
                for ch in children.into_iter().rev() { stack.push((ch, ri)); }
            }
        }
        if self.run_delta != 0 { self.refit_containers(first_run); }
    }

    /// After a repair shifted positions mid-leaf, containers recorded before
    /// the shift was known are re-derived from their children: delimiters
    /// around the children span for emphasis kinds, bracket syntax for links.
    fn refit_containers(&mut self, first_run: usize) {
        let bytes = self.src.as_bytes();
        for i in first_run..self.runs.len() {
            let kind = self.runs[i][run::KIND];
            let (mut cs, mut ce) = (usize::MAX, 0usize);
            for j in (i + 1)..self.runs.len() { if self.runs[j][run::PARENT] == i as u32 { cs = cs.min(self.runs[j][run::START_BYTE] as usize); ce = ce.max(self.runs[j][run::END_BYTE] as usize); } }
            if cs == usize::MAX { continue; }
            let (s0, e0) = (self.runs[i][run::START_BYTE] as usize, self.runs[i][run::END_BYTE] as usize);
            if s0 <= cs && e0 >= ce && (kind != run_kind::LINK && kind != run_kind::IMAGE || bytes.get(s0) == Some(&b'[') || bytes.get(s0) == Some(&b'!')) { continue; }
            let (ns, ne, ncs, nce) = match kind {
                run_kind::EMPH | run_kind::STRONG | run_kind::STRIKE => {
                    let n = if kind == run_kind::EMPH { 1 } else if kind == run_kind::STRONG { 2 } else if cs >= 2 && bytes[cs - 2] == b'~' && bytes.get(ce + 1) == Some(&b'~') { 2 } else { 1 };
                    if cs < n || ce + n > bytes.len() { continue; }
                    (cs - n, ce + n, cs, ce)
                }
                run_kind::LINK | run_kind::IMAGE => {
                    let open = if kind == run_kind::IMAGE { 2 } else { 1 };
                    if cs < open { continue; }
                    let mut p = ce;
                    if bytes.get(p) == Some(&b']') { p += 1; } else { continue; }
                    if bytes.get(p) == Some(&b'(') { let mut depth = 0i32; while p < bytes.len() { match bytes[p] { b'\\' => p += 1, b'(' => depth += 1, b')' => { depth -= 1; if depth == 0 { p += 1; break; } } _ => {} } p += 1; } }
                    else if bytes.get(p) == Some(&b'[') { while p < bytes.len() && bytes[p] != b']' { p += 1; } if p < bytes.len() { p += 1; } }
                    (cs - open, p.min(bytes.len()), cs, ce)
                }
                run_kind::ESCAPE => { if cs < 1 { continue; } (cs - 1, ce, cs, ce) }
                _ => continue,
            };
            let r = &mut self.runs[i];
            r[run::START_BYTE] = ns as u32; r[run::END_BYTE] = ne as u32; r[run::CONTENT_START_BYTE] = ncs as u32; r[run::CONTENT_END_BYTE] = nce as u32;
            r[run::START_UTF16] = self.li.u16(ns); r[run::END_UTF16] = self.li.u16(ne); r[run::CONTENT_START_UTF16] = self.li.u16(ncs); r[run::CONTENT_END_UTF16] = self.li.u16(nce);
            if kind == run_kind::LINK || kind == run_kind::IMAGE { let mut rec = self.runs[i]; rec[run::AUX0] = 0; rec[run::AUX1] = 0; rec[run::AUX2] = 0; rec[run::AUX3] = 0; rec[run::FLAGS] &= !3; self.link_aux(&mut rec, nce, ne); self.runs[i] = rec; }
        }
    }

    fn inline_record<'b>(&mut self, node: &'b AstNode<'b>, blk: u32, parent: u32, cell: Option<(usize, usize)>, shift: Option<&Shift>, content_from: usize) -> Option<u32> {
        let data = node.data.borrow();
        let sp = data.sourcepos;
        let (s, e) = self.corrected_range(sp, cell, shift)?;
        let src = self.src; let bytes = src.as_bytes();
        let slice = &src[s..e];
        let mut rec: RunRec = [0; run::WORDS];
        rec[run::BLOCK] = blk; rec[run::PARENT] = parent;
        let (kind, cs, ce): (u32, usize, usize) = match &data.value {
            NodeValue::Text(t) => {
                let t: &str = &**t;
                // Repair: comrak's inline line counter does not advance across a bare
                // CR, so positions after one are short by the line-ending bytes.
                // Find the literal forward of the last text run and carry the offset.
                let replacement_like = slice.contains('&') || slice.contains('\t') || slice.contains("\\|") || (t.len() > slice.len() && t.trim_start_matches(' ') == slice);
                let (s, e, slice) = if t != slice && !t.is_empty() && t != "\n" && !replacement_like {
                    let block_end = self.blocks[blk as usize][block::END_BYTE] as usize;
                    let mut from = self.last_text_end.max(s.saturating_sub(4)).min(src.len());
                    while from > 0 && !src.is_char_boundary(from) { from -= 1; }
                    let mut to = block_end.max(from).min(src.len());
                    while to < src.len() && !src.is_char_boundary(to) { to += 1; }
                    match src[from..to].find(t) {
                        Some(off) if off <= 8 + (e - s) => { let q = from + off; self.run_delta += q as isize - s as isize; (q, q + t.len(), &src[q..q + t.len()]) }
                        _ => (s, e, slice),
                    }
                } else { (s, e, slice) };
                if t == slice { (run_kind::TEXT, s, e) } else {
                    // Known limit: after a bare CR, text that also carries an entity
                    // cannot be relocated by literal search (the literal is decoded),
                    // so it keeps comrak's shifted range with the literal as display.
                    let cr_leaf = { let (bs, be) = (self.blocks[blk as usize][block::START_BYTE] as usize, self.blocks[blk as usize][block::END_BYTE] as usize); src.as_bytes()[bs.min(src.len())..be.min(src.len())].contains(&b'\r') };
                    // Virtual spaces of a partially consumed tab exist in comrak's buffer
                    // but not in the source; the literal then carries up to three
                    // leading spaces the slice cannot, so it displays as a replacement.
                    let virtual_spaces = t.len() > slice.len() && t.len() - slice.len() <= 3 && t.trim_start_matches(' ') == slice;
                    // comrak also unescapes pipes in a paragraph it examined as a table
                    // header candidate, not only inside cells.
                    let unescaped_pipes = slice.contains("\\|") && slice.replace("\\|", "|") == t;
                    let explained = slice.contains('&') || slice.contains('\t') || unescaped_pipes || cr_leaf || virtual_spaces;
                    if !explained { let (sl, lit) = (slice.to_string(), t.to_string()); self.dev("text-mismatch", || format!("block {blk} {:?} vs literal {:?}", sl, lit)); }
                    // Keep exact ranges around the bytes that differ (an entity, an
                    // escaped pipe, a CR, virtual spaces). The pieces are validated to
                    // rebuild the literal; otherwise the node stays one replacement run.
                    match text_pieces::split_pieces(slice, t) {
                        Some(pieces) if pieces.len() > 1 => {
                            let last = pieces.len() - 1;
                            for p in &pieces[..last] { let d = p.display.map(|(a, b)| &t[a..b]); self.push_piece(blk, parent, s + p.start, s + p.end, d); }
                            let p = &pieces[last];
                            match p.display {
                                None => (run_kind::TEXT, s + p.start, s + p.end),
                                Some((a, b)) => { let (off, len) = self.push_string(&t[a..b]); rec[run::AUX0] = off; rec[run::AUX1] = len; (run_kind::REPLACEMENT, s + p.start, s + p.end) }
                            }
                        }
                        _ => { let (off, len) = self.push_string(t); rec[run::AUX0] = off; rec[run::AUX1] = len; (run_kind::REPLACEMENT, s, e) }
                    }
                }
            }
            NodeValue::Emph => { let ok = e > s + 1 && matches!(bytes[s], b'*' | b'_') && bytes[e - 1] == bytes[s]; if !ok { let sl = slice.to_string(); self.dev("emph-delims", || format!("block {blk} {:?}", sl)); } (run_kind::EMPH, s + 1, e.saturating_sub(1).max(s + 1)) }
            NodeValue::Strong => { let ok = e >= s + 4 && matches!(bytes[s], b'*' | b'_') && bytes[s + 1] == bytes[s] && bytes[e - 1] == bytes[s] && bytes[e - 2] == bytes[s]; if !ok { let sl = slice.to_string(); self.dev("strong-delims", || format!("block {blk} {:?}", sl)); } (run_kind::STRONG, s + 2, e.saturating_sub(2).max(s + 2)) }
            NodeValue::Strikethrough => { let n = if e >= s + 4 && bytes[s] == b'~' && bytes[s + 1] == b'~' { 2 } else { 1 }; let ok = e >= s + 2 * n && bytes[s] == b'~' && bytes[e - 1] == b'~'; if !ok { let sl = slice.to_string(); self.dev("strike-delims", || format!("block {blk} {:?}", sl)); } (run_kind::STRIKE, s + n, e.saturating_sub(n).max(s + n)) }
            NodeValue::Code(c) => {
                let n = c.num_backticks.max(1);
                rec[run::AUX0] = n as u32;
                // comrak's end column drifts when a span crosses CR line endings;
                // the closing run is the next backtick run of exactly n after the
                // opener (CommonMark), validated below against the literal.
                let mut e = e;
                if s + n <= bytes.len() && bytes[s..s + n].iter().all(|b| *b == b'`') && !(e >= s + 2 * n && bytes[e - n..e].iter().all(|b| *b == b'`') && bytes.get(e) != Some(&b'`')) {
                    let mut i = s + n;
                    while i < bytes.len() {
                        if bytes[i] == b'`' { let start = i; while i < bytes.len() && bytes[i] == b'`' { i += 1; } if i - start == n { e = i; break; } } else { i += 1; }
                    }
                }
                let slice = &src[s..e];
                let ok = e >= s + 2 * n && bytes[s..s + n].iter().all(|b| *b == b'`') && bytes[e - n..e].iter().all(|b| *b == b'`');
                if !ok { let sl = slice.to_string(); self.dev("code-delims", || format!("block {blk} {:?}", sl)); (run_kind::CODE, s, e) } else {
                    let (mut cs, mut ce) = (s + n, e - n);
                    let raw = &src[cs..ce];
                    if raw != c.literal {
                        // A span crossing lines includes container prefixes in the source;
                        // comrak's buffer has the per-line content only, which the block's
                        // content records reproduce. Validate that text with whitespace
                        // collapsed (comrak also drops continuation-line indentation and
                        // keeps CR quirks of its own).
                        let multiline = raw.contains(['\n', '\r']);
                        let buffered: String = if multiline {
                            let mut parts = Vec::new();
                            for rec in &self.content[content_from..] {
                                let (a, b) = ((rec[content::START_BYTE] as usize).max(cs), (rec[content::END_BYTE] as usize).min(ce));
                                if a < b { parts.push(&src[a..b]); }
                            }
                            parts.join(" ")
                        } else { raw.to_string() };
                        let norm = |t: &str| if multiline { t.split_whitespace().collect::<Vec<_>>().join(" ") } else { t.to_string() };
                        let raw = buffered.as_str();
                        let (nraw, nlit) = (norm(raw), norm(&c.literal));
                        let stripped = if nraw.len() >= 2 && nraw.starts_with(' ') && nraw.ends_with(' ') && !nraw.trim().is_empty() { &nraw[1..nraw.len() - 1] } else { &nraw[..] };
                        let unescaped_pipes = cell.is_some() && nraw.replace("\\|", "|") == nlit;
                        if stripped == nlit && nraw != nlit { cs += 1; ce -= 1; }
                        else if nraw == nlit {}
                        else if unescaped_pipes { let (off, len) = self.push_string(&c.literal); rec[run::AUX2] = off; rec[run::AUX3] = len; rec[run::FLAGS] |= 2; }
                        else { let (r, l) = (raw.to_string(), c.literal.clone()); self.dev("code-literal", || format!("block {blk} {:?} vs {:?}", r, l)); }
                    }
                    (run_kind::CODE, cs, ce)
                }
            }
            NodeValue::Link(_) => {
                if s < e && bytes[s] == b'[' {
                    let (cs, ce) = { let (a, b) = self.children_span(node, cell, shift); if a == 0 && b == 0 { (s + 1, s + 1) } else { (a, b) } };
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
                    let (cs, ce) = { let (a, b) = self.children_span(node, cell, shift); if a == 0 && b == 0 { (s + 2, s + 2) } else { (a, b) } };
                    self.link_aux(&mut rec, ce, e);
                    (run_kind::IMAGE, cs, ce)
                } else { let sl = slice.to_string(); self.dev("image-delims", || format!("block {blk} {:?}", sl)); (run_kind::IMAGE, s, e) }
            }
            NodeValue::SoftBreak => (run_kind::SOFT_BREAK, e, e),
            NodeValue::LineBreak => (run_kind::HARD_BREAK, e, e),
            NodeValue::HtmlInline(_) => (run_kind::HTML_INLINE, s, e),
            NodeValue::FootnoteReference(_) => { if e > s + 3 { rec[run::AUX0] = (s + 2) as u32; rec[run::AUX1] = (e - 1) as u32; } (run_kind::FOOTNOTE_REF, s, e) }
            NodeValue::Escaped => { let (a, b) = self.children_span(node, cell, shift); if a < b { (run_kind::ESCAPE, a, b) } else { (run_kind::ESCAPE, (s + 1).min(e), e) } }
            _ => (run_kind::OTHER, s, e),
        };
        drop(data);
        // The Text arm may have moved the run; take its range from the content when it is a text run.
        let (s, e) = if kind == run_kind::TEXT || kind == run_kind::REPLACEMENT { (cs, ce) } else { (s, e) };
        if matches!(kind, run_kind::TEXT | run_kind::REPLACEMENT | run_kind::CODE | run_kind::AUTOLINK | run_kind::SOFT_BREAK | run_kind::HARD_BREAK) { self.last_text_end = e; }
        let (cs, ce) = (cs.max(s).min(e), ce.max(s).min(e));
        let (cs, ce) = (cs.min(ce), ce);
        rec[run::KIND] = kind;
        rec[run::START_BYTE] = s as u32; rec[run::END_BYTE] = e as u32;
        rec[run::CONTENT_START_BYTE] = cs as u32; rec[run::CONTENT_END_BYTE] = ce as u32;
        rec[run::START_UTF16] = self.li.u16(s); rec[run::END_UTF16] = self.li.u16(e);
        rec[run::CONTENT_START_UTF16] = self.li.u16(cs); rec[run::CONTENT_END_UTF16] = self.li.u16(ce);
        if sp.start.line != sp.end.line { rec[run::FLAGS] |= 1 << 8; }
        let ri = self.runs.len() as u32;
        self.runs.push(rec);
        Some(ri)
    }

    /// One piece of a split text node: an exact text run, or a replacement
    /// run displaying [display] (empty for hidden bytes).
    fn push_piece(&mut self, blk: u32, parent: u32, s: usize, e: usize, display: Option<&str>) {
        let mut rec: RunRec = [0; run::WORDS];
        rec[run::BLOCK] = blk; rec[run::PARENT] = parent;
        rec[run::KIND] = match display { None => run_kind::TEXT, Some(d) => { let (off, len) = self.push_string(d); rec[run::AUX0] = off; rec[run::AUX1] = len; run_kind::REPLACEMENT } };
        rec[run::START_BYTE] = s as u32; rec[run::END_BYTE] = e as u32;
        rec[run::CONTENT_START_BYTE] = s as u32; rec[run::CONTENT_END_BYTE] = e as u32;
        rec[run::START_UTF16] = self.li.u16(s); rec[run::END_UTF16] = self.li.u16(e);
        rec[run::CONTENT_START_UTF16] = self.li.u16(s); rec[run::CONTENT_END_UTF16] = self.li.u16(e);
        if self.src[s..e].contains('\n') { rec[run::FLAGS] |= 1 << 8; }
        self.runs.push(rec);
    }

    /// Destination and title ranges after a link's `]`, using comrak's own
    /// destination and title scanners; angle brackets are excluded from the
    /// destination range everywhere. Reference-style links carry the label.
    fn link_aux(&self, rec: &mut RunRec, ce: usize, e: usize) {
        let bytes = self.src.as_bytes();
        let mut p = ce;
        if p < e && bytes[p] == b']' { p += 1; }
        if p < e && bytes[p] == b'(' {
            p += 1;
            while p < e && reference_definitions::isspace(bytes[p]) { p += 1; }
            if let Some((ds, de, consumed)) = reference_definitions::scan_link_url(&bytes[p..e]) {
                rec[run::AUX0] = (p + ds) as u32; rec[run::AUX1] = (p + de) as u32;
                p += consumed;
            }
            while p < e && reference_definitions::isspace(bytes[p]) { p += 1; }
            if p < e { if let Some(n) = reference_definitions::scan_link_title(&bytes[p..e]) { if n >= 2 { rec[run::AUX2] = (p + 1) as u32; rec[run::AUX3] = (p + n - 1) as u32; rec[run::FLAGS] |= 2; } } }
        } else {
            rec[run::FLAGS] |= 1;
            if p < e && bytes[p] == b'[' { let ls = p + 1; let mut q = ls; while q < e && bytes[q] != b']' { q += 1; } rec[run::AUX0] = ls as u32; rec[run::AUX1] = q as u32; }
        }
    }

    // ---------------------------------------------------------------- encode

    fn encode(&self) -> Vec<u32> {
        let words = schema::HEADER_WORDS + self.li.line_count() * 2 + self.blocks.len() * block::WORDS + self.content.len() * content::WORDS + self.runs.len() * run::WORDS + self.definitions.len() * definition::WORDS;
        let string_words = (self.strings.len() + 3) / 4;
        let mut out: Vec<u32> = Vec::with_capacity(words + string_words);
        let mut hdr = [0u32; schema::HEADER_WORDS];
        hdr[header::MAGIC] = schema::MAGIC; hdr[header::VERSION] = schema::VERSION;
        hdr[header::SRC_BYTES] = self.src.len() as u32; hdr[header::SRC_UTF16] = self.li.u16(self.src.len());
        hdr[header::LINE_COUNT] = self.li.line_count() as u32; hdr[header::BLOCK_COUNT] = self.blocks.len() as u32;
        hdr[header::CONTENT_COUNT] = self.content.len() as u32; hdr[header::RUN_COUNT] = self.runs.len() as u32;
        hdr[header::DEFINITION_COUNT] = self.definitions.len() as u32; hdr[header::STRING_BYTES] = self.strings.len() as u32;
        out.extend_from_slice(&hdr);
        for l in 0..self.li.line_count() { let s = self.li.line_start(l); out.push(s as u32); out.push(self.li.u16(s)); }
        for r in &self.blocks { out.extend_from_slice(r); }
        for r in &self.content { out.extend_from_slice(r); }
        for r in &self.runs { out.extend_from_slice(r); }
        for d in &self.definitions { out.extend_from_slice(&[d.start as u32, d.end as u32, self.li.u16(d.start), self.li.u16(d.end), d.label.0 as u32, d.label.1 as u32, d.dest.0 as u32, d.dest.1 as u32]); }
        for chunk in self.strings.chunks(4) { let mut b = [0u8; 4]; b[..chunk.len()].copy_from_slice(chunk); out.push(u32::from_le_bytes(b)); }
        debug_assert_eq!(out.len(), words + string_words);
        out
    }
}

/// The model as the byte stream a host receives: each word little-endian.
pub fn to_bytes(words: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(words.len() * 4);
    for w in words { out.extend_from_slice(&w.to_le_bytes()); }
    out
}

/// The content buffer comrak sees for these lines: prefix-stripped lines
/// joined by '\n' and terminated by '\n' (cmark always ends a line).
fn lines_text(src: &str, lines: &[LineSpan]) -> String {
    let mut buffer = String::new();
    for l in lines { buffer.push_str(&src[l.start..l.end]); buffer.push('\n'); }
    buffer
}

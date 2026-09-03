//! Flat render-model extraction over an unmodified comrak AST.
//!
//! Layout (all u32 little-endian):
//!   header  : magic 'FLK5', version, src_bytes, src_utf16, line_count,
//!             block_count, run_count, content_count, string_bytes
//!   lines   : line_count × [start_byte, start_utf16]
//!   blocks  : block_count × BLOCK_WORDS
//!   content : content_count × [line_index, start_byte, start_utf16, end_byte, end_utf16]
//!   runs    : run_count × RUN_WORDS
//!   strings : string_bytes of UTF-8 (replacement display text)
use comrak::nodes::{AstNode, ListType, NodeValue, Sourcepos};
use comrak::{parse_document, Arena, Options};

pub const MAGIC: u32 = 0x354B4C46; // 'FLK5'
pub const VERSION: u32 = 1;
pub const HEADER_WORDS: usize = 9;
pub const BLOCK_WORDS: usize = 12;
pub const CONTENT_WORDS: usize = 5;
pub const RUN_WORDS: usize = 13;

pub mod block_kind {
    pub const DOCUMENT: u32 = 0;
    pub const PARAGRAPH: u32 = 1;
    pub const HEADING: u32 = 2;
    pub const CODE_BLOCK: u32 = 3;
    pub const HTML_BLOCK: u32 = 4;
    pub const BLOCK_QUOTE: u32 = 5;
    pub const LIST: u32 = 6;
    pub const ITEM: u32 = 7;
    pub const THEMATIC_BREAK: u32 = 8;
    pub const TABLE: u32 = 9;
    pub const TABLE_ROW: u32 = 10;
    pub const TABLE_CELL: u32 = 11;
    pub const FOOTNOTE_DEF: u32 = 12;
    pub const OTHER: u32 = 14;
}

pub mod run_kind {
    pub const TEXT: u32 = 1;
    pub const EMPH: u32 = 2;
    pub const STRONG: u32 = 3;
    pub const CODE: u32 = 4;
    pub const STRIKE: u32 = 5;
    pub const LINK: u32 = 6;
    pub const IMAGE: u32 = 7;
    pub const AUTOLINK: u32 = 8;
    pub const ESCAPE: u32 = 9;
    pub const REPLACEMENT: u32 = 10;
    pub const HARD_BREAK: u32 = 11;
    pub const SOFT_BREAK: u32 = 12;
    pub const HTML_INLINE: u32 = 13;
    pub const FOOTNOTE_REF: u32 = 14;
    pub const TASK_MARKER: u32 = 15;
    pub const OTHER: u32 = 16;
}

/// Byte offset of every line start plus a byte→UTF-16 prefix table.
pub struct LineIndex {
    pub line_starts: Vec<usize>,
    /// utf16[i] = number of UTF-16 code units in src[..i] (len = src.len()+1)
    pub utf16: Vec<u32>,
}

impl LineIndex {
    pub fn new(src: &str) -> Self {
        let bytes = src.as_bytes();
        let mut line_starts = vec![0usize];
        let mut utf16 = Vec::with_capacity(bytes.len() + 1);
        let mut count: u32 = 0;
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            let width = if b < 0x80 { 1 } else if b < 0xE0 { 2 } else if b < 0xF0 { 3 } else { 4 };
            let units = if width == 4 { 2 } else { 1 };
            for _ in 0..width { utf16.push(count); }
            count += units;
            if b == b'\n' {
                line_starts.push(i + 1);
            }
            i += width;
        }
        utf16.push(count);
        // trailing partial line without newline still counts as a line; a final
        // "\n" produces an empty trailing line which comrak also counts.
        LineIndex { line_starts, utf16 }
    }
    pub fn line_count(&self) -> usize { self.line_starts.len() }
    pub fn line_start(&self, line0: usize) -> usize { self.line_starts[line0.min(self.line_starts.len() - 1)] }
    pub fn line_end(&self, line0: usize, src_len: usize) -> usize {
        if line0 + 1 < self.line_starts.len() { self.line_starts[line0 + 1] - 1 } else { src_len }
    }
    pub fn u16(&self, byte: usize) -> u32 { self.utf16[byte.min(self.utf16.len() - 1)] }
}

/// comrak sourcepos → [start, end) byte range. Columns are 1-based bytes
/// within the line; end.column is inclusive.
pub fn sourcepos_range(sp: Sourcepos, li: &LineIndex, src_len: usize) -> Option<(usize, usize)> {
    if sp.start.line == 0 || sp.end.line == 0 { return None; }
    let start = li.line_start(sp.start.line - 1) + sp.start.column.saturating_sub(1);
    let end = li.line_start(sp.end.line - 1) + sp.end.column;
    let start = start.min(src_len);
    let end = end.min(src_len).max(start);
    Some((start, end))
}

#[derive(Clone, Copy)]
struct Container { kind: u8, offset: usize, first_line: usize } // kind: 1 quote, 2 item(offset), 3 footnote(4)

pub struct Extractor<'a> {
    src: &'a str,
    li: LineIndex,
    blocks: Vec<[u32; BLOCK_WORDS]>,
    content: Vec<[u32; CONTENT_WORDS]>,
    runs: Vec<[u32; RUN_WORDS]>,
    strings: Vec<u8>,
    pub deviations: Vec<String>,
}

impl<'a> Extractor<'a> {
    pub fn extract(src: &'a str, collect_deviations: bool) -> Vec<u8> {
        let mut ex = Extractor { src, li: LineIndex::new(src), blocks: Vec::new(), content: Vec::new(), runs: Vec::new(), strings: Vec::new(), deviations: Vec::new() };
        let arena = Arena::new();
        let root = parse_document(&arena, src, &options());
        ex.walk_block(root, u32::MAX, &mut Vec::new(), collect_deviations);
        ex.encode()
    }

    pub fn extract_with_report(src: &'a str) -> (Vec<u8>, Vec<String>) {
        let mut ex = Extractor { src, li: LineIndex::new(src), blocks: Vec::new(), content: Vec::new(), runs: Vec::new(), strings: Vec::new(), deviations: Vec::new() };
        let arena = Arena::new();
        let root = parse_document(&arena, src, &options());
        ex.walk_block(root, u32::MAX, &mut Vec::new(), true);
        let buf = ex.encode();
        (buf, ex.deviations)
    }

    fn encode(&self) -> Vec<u8> {
        let words = HEADER_WORDS + self.li.line_count() * 2 + self.blocks.len() * BLOCK_WORDS + self.content.len() * CONTENT_WORDS + self.runs.len() * RUN_WORDS;
        let mut out: Vec<u8> = Vec::with_capacity(words * 4 + self.strings.len() + 4);
        let push = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
        for v in [MAGIC, VERSION, self.src.len() as u32, self.li.u16(self.src.len()), self.li.line_count() as u32, self.blocks.len() as u32, self.runs.len() as u32, self.content.len() as u32, self.strings.len() as u32] { push(&mut out, v); }
        for l in 0..self.li.line_count() { let s = self.li.line_start(l); push(&mut out, s as u32); push(&mut out, self.li.u16(s)); }
        for b in &self.blocks { for v in b { push(&mut out, *v); } }
        for c in &self.content { for v in c { push(&mut out, *v); } }
        for r in &self.runs { for v in r { push(&mut out, *v); } }
        out.extend_from_slice(&self.strings);
        while out.len() % 4 != 0 { out.push(0); }
        out
    }

    fn dev(&mut self, collect: bool, msg: String) { if collect { self.deviations.push(msg); } }

    fn walk_block<'b>(&mut self, node: &'b AstNode<'b>, parent: u32, containers: &mut Vec<Container>, collect: bool) {
        let data = node.data.borrow();
        let sp = data.sourcepos;
        let (kind, attr0, attr1, flags, is_leaf_inline, container): (u32, u32, u32, u32, bool, Option<Container>) = match &data.value {
            NodeValue::Document => (block_kind::DOCUMENT, 0, 0, 0, false, None),
            NodeValue::Paragraph => (block_kind::PARAGRAPH, 0, 0, 0, true, None),
            NodeValue::Heading(h) => (block_kind::HEADING, h.level as u32, 0, h.setext as u32, true, None),
            NodeValue::CodeBlock(c) => (block_kind::CODE_BLOCK, c.fence_length as u32, c.fence_offset as u32, c.fenced as u32, false, None),
            NodeValue::HtmlBlock(_) => (block_kind::HTML_BLOCK, 0, 0, 0, false, None),
            NodeValue::BlockQuote => (block_kind::BLOCK_QUOTE, 0, 0, 0, false, Some(Container { kind: 1, offset: 0, first_line: sp.start.line.saturating_sub(1) })),
            NodeValue::List(l) => (block_kind::LIST, matches!(l.list_type, ListType::Ordered) as u32, l.start as u32, l.tight as u32, false, None),
            NodeValue::Item(l) => (block_kind::ITEM, (l.marker_offset + l.padding) as u32, l.marker_offset as u32, 0, false, Some(Container { kind: 2, offset: l.marker_offset + l.padding, first_line: sp.start.line.saturating_sub(1) })),
            NodeValue::TaskItem(t) => (block_kind::ITEM, 0, 0, 0x10 | (t.symbol.is_some() as u32), false, None),
            NodeValue::ThematicBreak => (block_kind::THEMATIC_BREAK, 0, 0, 0, false, None),
            NodeValue::Table(_) => (block_kind::TABLE, 0, 0, 0, false, None),
            NodeValue::TableRow(h) => (block_kind::TABLE_ROW, *h as u32, 0, 0, false, None),
            NodeValue::TableCell => (block_kind::TABLE_CELL, 0, 0, 0, true, None),
            NodeValue::FootnoteDefinition(_) => (block_kind::FOOTNOTE_DEF, 0, 0, 0, false, Some(Container { kind: 3, offset: 4, first_line: sp.start.line.saturating_sub(1) })),
            _ => (block_kind::OTHER, 0, 0, 0, false, None),
        };
        let mut attr0 = attr0;
        // TaskItem in comrak 0.54 is an Item-like node whose own sourcepos covers the item; keep ITEM kind.
        if let NodeValue::TaskItem(_) = &data.value { attr0 = 0; }
        let (start, end) = sourcepos_range(sp, &self.li, self.src.len()).unwrap_or((0, 0));
        let first_line = sp.start.line.saturating_sub(1) as u32;
        let line_count = if sp.end.line >= sp.start.line { (sp.end.line - sp.start.line + 1) as u32 } else { 0 };
        let idx = self.blocks.len() as u32;
        let content_off = self.content.len() as u32;
        self.blocks.push([kind, parent, start as u32, end as u32, self.li.u16(start), self.li.u16(end), first_line, line_count, content_off, attr0, attr1, flags]);
        drop(data);

        if let Some(c) = container { containers.push(c); }

        // Per-line content ranges for leaf blocks.
        let data = node.data.borrow();
        match &data.value {
            NodeValue::CodeBlock(c) => { let c = c.clone(); drop(data); self.code_block_content(node, idx, &c, containers, collect); }
            _ if is_leaf_inline => { drop(data); self.inline_leaf_content(node, idx, containers, collect); }
            _ => { drop(data); }
        }
        // fix content count
        let n = (self.content.len() as u32) - content_off;
        self.blocks[idx as usize][7] = if is_leaf_inline || matches!(node.data.borrow().value, NodeValue::CodeBlock(_)) { n } else { line_count };

        if is_leaf_inline {
            for child in node.children() { self.walk_inline(child, idx, u32::MAX, collect); }
        } else {
            for child in node.children() {
                let is_block = !matches!(child.data.borrow().value, NodeValue::Text(_) | NodeValue::Emph | NodeValue::Strong | NodeValue::Code(_) | NodeValue::SoftBreak | NodeValue::LineBreak | NodeValue::Link(_) | NodeValue::Image(_) | NodeValue::Strikethrough | NodeValue::HtmlInline(_) | NodeValue::FootnoteReference(_) | NodeValue::Escaped);
                if is_block { self.walk_block(child, idx, containers, collect); } else { self.walk_inline(child, idx, u32::MAX, collect); }
            }
        }
        if container.is_some() { containers.pop(); }
    }

    /// Scanner derivation: consume container prefixes on one physical line.
    /// Returns (content_start_byte, lazy) where lazy means a container prefix was absent.
    fn prefix_scan(&self, line0: usize, containers: &[Container], _first_line_of_item: Option<usize>) -> (usize, bool) {
        let ls = self.li.line_start(line0);
        let le = self.li.line_end(line0, self.src.len());
        let line = &self.src.as_bytes()[ls..le];
        let mut pos = 0usize; // byte pos within line
        let mut col = 0usize;  // visual column (tabs to 4)
        let mut lazy = false;
        for c in containers {
            match c.kind {
                1 => {
                    // up to 3 spaces, then '>' then optional one space
                    let mut p = pos; let mut k = col; let mut spaces = 0;
                    while p < line.len() && line[p] == b' ' && spaces < 3 { p += 1; k += 1; spaces += 1; }
                    if p < line.len() && line[p] == b'>' {
                        p += 1; k += 1;
                        if p < line.len() && line[p] == b' ' { p += 1; k += 1; }
                        else if p < line.len() && line[p] == b'\t' { p += 1; k = (k / 4 + 1) * 4; }
                        pos = p; col = k;
                    } else { lazy = true; break; }
                }
                2 | 3 => {
                    // need `offset` columns of indentation (or the marker itself on the item's first line)
                    let target = col + c.offset;
                    if c.first_line == line0 && c.kind == 2 {
                        // consume marker_offset spaces, the marker, and padding
                        let mut p = pos; let mut k = col;
                        while p < line.len() && (line[p] == b' ' || line[p] == b'\t') && k < target { if line[p] == b'\t' { k = (k / 4 + 1) * 4; } else { k += 1; } p += 1; }
                        // marker: -, +, * or digits followed by . or )
                        while p < line.len() && (line[p].is_ascii_digit()) { p += 1; k += 1; }
                        if p < line.len() && matches!(line[p], b'-' | b'+' | b'*' | b'.' | b')') { p += 1; k += 1; }
                        while p < line.len() && (line[p] == b' ' || line[p] == b'\t') && k < target { if line[p] == b'\t' { k = (k / 4 + 1) * 4; } else { k += 1; } p += 1; }
                        pos = p; col = k;
                    } else {
                        let mut p = pos; let mut k = col;
                        while p < line.len() && (line[p] == b' ' || line[p] == b'\t') && k < target { if line[p] == b'\t' { k = (k / 4 + 1) * 4; } else { k += 1; } p += 1; }
                        if k >= target { pos = p; col = k; } else { lazy = true; break; }
                    }
                }
                _ => {}
            }
        }
        (ls + pos, lazy)
    }

    fn inline_leaf_content<'b>(&mut self, node: &'b AstNode<'b>, idx: u32, containers: &[Container], collect: bool) {
        let sp = node.data.borrow().sourcepos;
        if sp.start.line == 0 { return; }
        // Parser-authored derivation: first inline node starting on each line.
        let mut starts: Vec<Option<usize>> = vec![None; (sp.end.line - sp.start.line + 1).max(1)];
        fn visit<'b>(n: &'b AstNode<'b>, base: usize, starts: &mut Vec<Option<usize>>, li: &LineIndex, src_len: usize) {
            for ch in n.children() {
                let d = ch.data.borrow();
                let s = d.sourcepos;
                if s.start.line >= base + 1 {
                    let l = s.start.line - 1 - base;
                    if l < starts.len() && starts[l].is_none() && !matches!(d.value, NodeValue::SoftBreak | NodeValue::LineBreak) {
                        if let Some((st, _)) = sourcepos_range(s, li, src_len) { starts[l] = Some(st); }
                    }
                }
                drop(d);
                visit(ch, base, starts, li, src_len);
            }
        }
        visit(node, sp.start.line - 1, &mut starts, &self.li, self.src.len());
        let is_table_cell = matches!(node.data.borrow().value, NodeValue::TableCell);
        let first_item_line = containers.last().and_then(|c| if c.kind == 2 { self.item_first_line(idx) } else { None });
        for (k, s) in starts.iter().enumerate() {
            let line0 = sp.start.line - 1 + k;
            let ls = self.li.line_start(line0);
            let le = self.li.line_end(line0, self.src.len());
            // scanner derivation
            let (mut scan, _lazy) = if is_table_cell { (ls, false) } else { self.prefix_scan(line0, containers, first_item_line) };
            // leaf-specific: paragraphs and headings strip leading whitespace; ATX markers
            let bytes = self.src.as_bytes();
            while scan < le && (bytes[scan] == b' ' || bytes[scan] == b'\t') { scan += 1; }
            if k == 0 { if let NodeValue::Heading(h) = &node.data.borrow().value { if !h.setext { let mut p = scan; let mut n = 0; while p < le && bytes[p] == b'#' { p += 1; n += 1; } if n > 0 { while p < le && (bytes[p] == b' ' || bytes[p] == b'\t') { p += 1; } scan = p; } } } }
            let cs = scan;
            if is_table_cell {
                // cell: sourcepos of the cell itself is the content span
                let (cst, cen) = sourcepos_range(sp, &self.li, self.src.len()).unwrap_or((ls, ls));
                self.content.push([line0 as u32, cst as u32, self.li.u16(cst), cen as u32, self.li.u16(cen)]);
                continue;
            }
            if let Some(v) = s { if *v < scan && collect { self.dev(collect, format!("line-content: block {idx} line {line0}: parser {v} before scanner {scan}: {:?}", &self.src[ls..le])); } }
            // content end: line end minus trailing whitespace (and ATX closing sequence handled by inline span)
            let mut ce = le; while ce > cs && (bytes[ce - 1] == b' ' || bytes[ce - 1] == b'\t') { ce -= 1; }
            self.content.push([line0 as u32, cs as u32, self.li.u16(cs), ce as u32, self.li.u16(ce)]);
        }
    }

    fn item_first_line(&self, idx: u32) -> Option<usize> {
        // walk up to the nearest ITEM block and return its first line
        let mut i = idx as usize;
        loop {
            let b = &self.blocks[i];
            if b[0] == block_kind::ITEM { return Some(b[6] as usize); }
            if b[1] == u32::MAX { return None; }
            i = b[1] as usize;
        }
    }

    fn code_block_content<'b>(&mut self, node: &'b AstNode<'b>, idx: u32, c: &comrak::nodes::NodeCodeBlock, containers: &[Container], collect: bool) {
        let sp = node.data.borrow().sourcepos;
        if sp.start.line == 0 { return; }
        let first_item_line = containers.last().and_then(|cc| if cc.kind == 2 { self.item_first_line(idx) } else { None });
        let mut derived = String::new();
        let bytes = self.src.as_bytes();
        let (l0, l1) = (sp.start.line - 1, sp.end.line - 1);
        for line0 in l0..=l1 {
            let is_fence_line = c.fenced && (line0 == l0 || (line0 == l1 && l1 > l0 && { let (p0, _) = self.prefix_scan(line0, containers, first_item_line); self.closing_fence_at(p0, line0, c) }));
            if is_fence_line { continue; }
            let (mut p, _lazy) = self.prefix_scan(line0, containers, first_item_line);
            let le = self.li.line_end(line0, self.src.len());
            let remove = if c.fenced { c.fence_offset } else { 4 };
            let mut n = 0; while p < le && n < remove { if bytes[p] == b' ' { p += 1; n += 1; } else if bytes[p] == b'\t' { p += 1; n = (n / 4 + 1) * 4; } else { break; } }
            self.content.push([line0 as u32, p as u32, self.li.u16(p), le as u32, self.li.u16(le)]);
            derived.push_str(&self.src[p..le]); derived.push('\n');
        }
        if collect && derived != c.literal {
            // indented code blocks may end with blank lines trimmed; compare after trimming trailing newlines
            if derived.trim_end_matches('\n') != c.literal.trim_end_matches('\n') {
                self.dev(collect, format!("code-content: block {idx}: derived {:?} vs literal {:?}", derived, c.literal));
            }
        }
    }

    fn closing_fence_at(&self, ls: usize, line0: usize, c: &comrak::nodes::NodeCodeBlock) -> bool {
        let le = self.li.line_end(line0, self.src.len());
        let raw = &self.src[ls..le];
        let lead = raw.len() - raw.trim_start_matches(' ').len();
        let t = raw.trim();
        let ch = c.fence_char as char;
        lead < 4 && t.len() >= c.fence_length && t.chars().all(|x| x == ch)
    }

    fn walk_inline<'b>(&mut self, node: &'b AstNode<'b>, block: u32, parent: u32, collect: bool) {
        let data = node.data.borrow();
        let sp = data.sourcepos;
        let Some((s, e)) = sourcepos_range(sp, &self.li, self.src.len()) else { return; };
        let src = self.src;
        let bytes = src.as_bytes();
        let slice = &src[s..e];
        let mut aux0 = 0u32; let mut aux1 = 0u32;
        let (kind, cs, ce): (u32, usize, usize) = match &data.value {
            NodeValue::Text(t) => {
                let t: &str = &**t;
                if t == slice { (run_kind::TEXT, s, e) }
                else {
                    // entity / escape / replacement: display text differs from source
                    aux0 = self.strings.len() as u32; self.strings.extend_from_slice(t.as_bytes()); aux1 = t.len() as u32;
                    if collect && !slice.contains('&') && !slice.contains('\\') && !slice.contains('\t') && !slice.contains("\r") { self.dev(collect, format!("text-mismatch: block {block} {:?} vs literal {:?}", slice, t)); }
                    (run_kind::REPLACEMENT, s, e)
                }
            }
            NodeValue::Emph => {
                let ok = e > s + 1 && matches!(bytes[s], b'*' | b'_') && bytes[e - 1] == bytes[s];
                if !ok { self.dev(collect, format!("emph-delims: block {block} {:?}", slice)); }
                (run_kind::EMPH, s + 1, e.saturating_sub(1).max(s + 1))
            }
            NodeValue::Strong => {
                let ok = e >= s + 4 && matches!(bytes[s], b'*' | b'_') && bytes[s + 1] == bytes[s] && bytes[e - 1] == bytes[s] && bytes[e - 2] == bytes[s];
                if !ok { self.dev(collect, format!("strong-delims: block {block} {:?}", slice)); }
                (run_kind::STRONG, s + 2, e.saturating_sub(2).max(s + 2))
            }
            NodeValue::Strikethrough => {
                let n = if e >= s + 4 && bytes[s] == b'~' && bytes[s + 1] == b'~' { 2 } else { 1 };
                let ok = e >= s + 2 * n && bytes[s] == b'~' && bytes[e - 1] == b'~';
                if !ok { self.dev(collect, format!("strike-delims: block {block} {:?}", slice)); }
                (run_kind::STRIKE, s + n, e.saturating_sub(n).max(s + n))
            }
            NodeValue::Code(c) => {
                let n = c.num_backticks.max(1);
                let ok = e >= s + 2 * n && slice.starts_with(&"`".repeat(n)) && slice.ends_with(&"`".repeat(n));
                if !ok { self.dev(collect, format!("code-delims: block {block} {:?}", slice)); (run_kind::CODE, s, e) }
                else {
                    let (mut cs, mut ce) = (s + n, e - n);
                    let raw = &src[cs..ce];
                    if raw != c.literal {
                        let stripped = if raw.len() >= 2 && raw.starts_with(' ') && raw.ends_with(' ') && !raw.trim().is_empty() { &raw[1..raw.len() - 1] } else { raw };
                        if stripped == c.literal { cs += 1; ce -= 1; }
                        else if raw.replace('\n', " ") == c.literal || raw.replace('\n', " ").trim_matches(' ') == c.literal.trim_matches(' ') { /* newline→space, fine for display */ }
                        else { self.dev(collect, format!("code-literal: block {block} {:?} vs {:?}", raw, c.literal)); }
                    }
                    (run_kind::CODE, cs, ce)
                }
            }
            NodeValue::Link(l) => {
                if bytes[s] == b'[' {
                    let (cs, ce) = self.children_span(node, s + 1);
                    // destination range: inside "(...)" or a reference label; record dest start/end when inline form
                    if bytes[e - 1] == b')' { if let Some(p) = src[ce..e].find('(') { aux0 = (ce + p + 1) as u32; aux1 = (e - 1) as u32; } }
                    let _ = l;
                    (run_kind::LINK, cs, ce)
                } else {
                    (run_kind::AUTOLINK, if bytes[s] == b'<' { s + 1 } else { s }, if bytes[e - 1] == b'>' { e - 1 } else { e })
                }
            }
            NodeValue::Image(_) => {
                if bytes[s] == b'!' { let (cs, ce) = self.children_span(node, s + 2); (run_kind::IMAGE, cs, ce) }
                else { self.dev(collect, format!("image-delims: block {block} {:?}", slice)); (run_kind::IMAGE, s, e) }
            }
            NodeValue::SoftBreak => (run_kind::SOFT_BREAK, e, e),
            NodeValue::LineBreak => (run_kind::HARD_BREAK, e, e),
            NodeValue::HtmlInline(_) => (run_kind::HTML_INLINE, s, e),
            NodeValue::FootnoteReference(_) => (run_kind::FOOTNOTE_REF, s, e),
            NodeValue::Escaped => (run_kind::ESCAPE, (s + 1).min(e), e),
            _ => (run_kind::OTHER, s, e),
        };
        drop(data);
        if collect && (cs < s || ce > e || cs > ce) { self.dev(collect, format!("range-order: block {block} kind {kind} {s}..{e} content {cs}..{ce}")); }
        let ri = self.runs.len() as u32;
        self.runs.push([kind, block, parent, s as u32, e as u32, cs as u32, ce as u32, self.li.u16(s), self.li.u16(e), self.li.u16(cs), self.li.u16(ce), aux0, aux1]);
        for ch in node.children() { self.walk_inline(ch, block, ri, collect); }
    }

    fn children_span<'b>(&self, node: &'b AstNode<'b>, fallback: usize) -> (usize, usize) {
        let mut first = None; let mut last = None;
        for ch in node.children() {
            if let Some((cs, ce)) = sourcepos_range(ch.data.borrow().sourcepos, &self.li, self.src.len()) {
                if first.is_none() { first = Some(cs); }
                last = Some(ce);
            }
        }
        match (first, last) { (Some(a), Some(b)) => (a, b.max(a)), _ => (fallback, fallback) }
    }
}

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

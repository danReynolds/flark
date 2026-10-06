//! Flat render-model extraction over an unmodified comrak AST.
//!
//! See `schema/render_model_v5.json` and `SCHEMA.md` for the published
//! layout and `records` for the richer internal one. The extraction walks the
//! tree once, iteratively, derives what comrak does not
//! expose (per-line content ranges, reference definitions), corrects the two
//! situations where comrak's inline positions are known to be off, and in
//! report mode validates every derivation against comrak's own output.

use crate::text_pieces;
use crate::lines::LineIndex;
use crate::reference_definitions::{self, Definition};
use crate::records::{block, content, run};
use crate::schema::{self, block_kind, run_kind, table_alignment};
use comrak::nodes::{AstNode, ListType, NodeCodeBlock, NodeValue, Sourcepos, TableAlignment};
use comrak::{parse_document, Arena, Options};

pub type BlockRec = [u32; block::WORDS];
pub type ContentRec = [u32; content::WORDS];
pub type RunRec = [u32; run::WORDS];

/// A validation finding from report mode. `rule` is a stable identifier.
/// `leaf` names the paragraph, heading or table cell whose inline runs were
/// dropped because of it, so the leaf shows its source as plain text; a
/// finding without one refuses the whole model.
#[derive(Clone, Debug)]
pub struct Deviation { pub rule: &'static str, pub detail: String, pub leaf: Option<u32> }

/// Deviations confined to one leaf's inline runs. Dropping the leaf's runs
/// leaves a model that is exactly right, only less rendered.
/// Block flags bit 23 (SCHEMA.md, `any`): a leaf published without runs.
pub const SOURCE_ONLY: u32 = 1 << 23;

/// comrak's GFM email autolinking recurses once for every address it links
/// in one text node (`process_email_autolinks` calls itself on the rest), at
/// about 330 bytes of stack a level in a release build. A stack overflow
/// aborts the host process instead of unwinding, so no `catch_unwind`
/// contains it. Measured largest depths: 802 on a 256 KiB thread, 1,582 on
/// 512 KiB, 3,143 on 1 MiB, 3,127 in a Dart VM isolate, 3,244 for wasm32 in
/// V8 at its default stack and 1,874 at a 500 KB one. A text node never
/// crosses a line, so a line that could hold more addresses than this is
/// refused before comrak parses it. 1,024 levels take about 330 KiB. An
/// address takes at least four ASCII bytes: comrak links `c@.r`, and packs
/// them with no gap as `a@.b+@.c`. The editor's widest line, 4,096 code
/// units, therefore holds at most 1,024, exactly this cap, so no line the
/// editor parses by default is refused.
pub const MAX_EMAIL_AUTOLINKS_PER_LINE: usize = 1024;

/// comrak gives every body row of a table as many cells as its header has
/// columns, creating the ones the row lacks (`try_opening_row`). Its own cap
/// on created cells (`MAX_AUTOCOMPLETED_CELLS`, 500,000) never applies: it
/// counts a row's cells after adding the missing ones, so none counts as
/// missing. A header of 2,048 columns over 2,046 one-character rows, 12 KB
/// that the editor parses live even on a phone, made 4.2 million cells: 1.7 s
/// of parse and 2.2 s with the extraction on an M1 Pro, at 2 GB resident.
/// Past the editor's limits more rows only add cells, until an allocation
/// fails, which aborts the process. A document whose tables could gain more
/// cells than this, or than half its bytes when that is more, is refused
/// before comrak parses it. 16,384 is as many cells as a table of
/// one-character cells (`a|`) holds in the editor's 32 KiB live limit, so
/// created cells at most double what such a table costs: 8 ms for that
/// table, 3 ms for 16,384 created cells. A larger document may gain as many
/// as its bytes could hold.
pub const MAX_FILLED_TABLE_CELLS: usize = 16_384;

const INLINE_RULES: &[&str] = &["text-mismatch", "emph-delims", "strong-delims", "strike-delims", "escape-delims", "link-delims", "footnote-ref-delims", "code-delims", "code-literal", "image-delims", "html-inline-end", "html-inline-literal", "heading-content", "run-structure"];

/// Extraction refused to publish a model because a derived range or value did
/// not agree with comrak's source positions or literal output, or because the
/// model broke the schema's structural invariants. A deviation confined to
/// one leaf's inlines does not refuse: that leaf publishes without runs.
#[derive(Debug)]
pub struct ExtractionError { pub deviations: Vec<Deviation> }

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

/// The first line that could link more than [MAX_EMAIL_AUTOLINKS_PER_LINE]
/// addresses ([email_autolinks_bound]).
fn email_autolink_overflow(src: &str) -> Option<usize> {
    let b = src.as_bytes();
    let (mut line, mut start, mut i) = (0usize, 0usize, 0usize);
    loop {
        if i < b.len() && !matches!(b[i], b'\n' | b'\r') { i += 1; continue; }
        if email_autolinks_bound(&src[start..i]) > MAX_EMAIL_AUTOLINKS_PER_LINE { return Some(line); }
        if i == b.len() { return None; }
        i += if b[i] == b'\r' && b.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
        (start, line) = (i, line + 1);
    }
}

/// At least as many addresses as comrak can link in one text node of
/// [line] (no line ending). Each needs its own `@`, counting an entity
/// reference that decodes to one (`&#64;`, `&#x40;`, `&commat;`) since comrak
/// links addresses in decoded text, and at least four ASCII bytes, which
/// decoding never adds: a character before the `@` and a domain of a dot and
/// a letter (`c@.r`), and the next address starts after the last one's
/// domain. The smaller count bounds the addresses one text node of the line
/// can link: a line of `@` signs alone holds a quarter of its bytes.
fn email_autolinks_bound(line: &str) -> usize {
    let b = line.as_bytes();
    let (mut ats, mut ascii) = (0usize, 0usize);
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'@' => { ats += 1; ascii += 1; }
            b'&' => {
                ascii += 1;
                if let Some(l) = text_pieces::entity_len(&line[i..]) {
                    let body = &line[i + 1..i + l - 1];
                    let value = match body.strip_prefix('#') { Some(n) => match n.strip_prefix(['x', 'X']) { Some(h) => u32::from_str_radix(h, 16).ok(), None => n.parse().ok() }, None => (body == "commat").then_some(64) };
                    if value == Some(64) { ats += 1; }
                }
            }
            c => if c < 0x80 { ascii += 1; },
        }
    }
    ats.min(ascii / 4)
}

/// At least as many cells as comrak creates for the tables of [src] because
/// a body row lacks them (see [MAX_FILLED_TABLE_CELLS]). comrak ends a table
/// at a line blank inside its container, and a line of quote markers alone
/// is blank inside a quote and opens one outside it, so the rows of a table
/// follow its delimiter row within one run of lines that are neither. A
/// table has as many columns as its delimiter row has cells, so a line after
/// a possible delimiter row lacks at most the most cells of any possible
/// delimiter row above it in its run, less its own. A possible delimiter row
/// follows another line of its run (the header) and holds only pipes,
/// colons, dashes, spaces and tabs, a dash among them (`table_start` is
/// stricter). Lines are read without leading spaces, tabs and quote markers,
/// the prefixes a row's containers can have: a row's own content cannot
/// begin with `>`, which opens a quote, and dropping that from a line that is
/// no row only lowers its cells. A byte order mark opening the document is
/// skipped, as comrak skips it. One pass, linear in the source.
pub fn filled_table_cells(src: &str) -> usize {
    let b = src.as_bytes();
    let (mut filled, mut columns, mut run) = (0usize, 0usize, 0usize);
    let mut start = if b.starts_with("\u{feff}".as_bytes()) { 3 } else { 0 };
    loop {
        let end = b[start..].iter().position(|&c| c == b'\n' || c == b'\r').map_or(b.len(), |p| start + p);
        let mut i = start;
        while i < end && matches!(b[i], b' ' | b'\t' | b'>') { i += 1; }
        let line = &b[i..end];
        if line.is_empty() { columns = 0; run = 0; } else {
            let cells = row_cells(line);
            filled = filled.saturating_add(columns.saturating_sub(cells));
            if run > 0 && line.contains(&b'-') && line.iter().all(|&c| matches!(c, b'|' | b':' | b'-' | b' ' | b'\t' | 0x0b | 0x0c)) { columns = columns.max(cells); }
            run += 1;
        }
        if end == b.len() { return filled; }
        start = end + if b[end] == b'\r' && b.get(end + 1) == Some(&b'\n') { 2 } else { 1 };
    }
}

/// The cells comrak's table `row` splits one line's content into: a pipe
/// ends a cell unless a backslash precedes it (the cell scanner's longest
/// match reads `\|` as an escaped character whatever comes before the
/// backslash), a leading pipe and the spaces, tabs, vertical tabs and form
/// feeds after any pipe belong to the pipe, and content after the last pipe
/// is one more cell. A lone pipe holds none: comrak takes no row from it.
fn row_cells(line: &[u8]) -> usize {
    let space = |c: u8| matches!(c, b' ' | b'\t' | 0x0b | 0x0c);
    let (mut i, mut cells) = (0usize, 0usize);
    if line.first() == Some(&b'|') { i = 1; while i < line.len() && space(line[i]) { i += 1; } }
    while i < line.len() {
        let start = i;
        while i < line.len() && !(line[i] == b'|' && (i == 0 || line[i - 1] != b'\\')) { i += 1; }
        if i == line.len() { if i > start { cells += 1; } break; }
        cells += 1;
        i += 1;
        while i < line.len() && space(line[i]) { i += 1; }
    }
    cells
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContainerKind { Quote, Item, Footnote }

/// One container in the chain above a line. `offset` is the content column
/// relative to the enclosing container's content column. `checkbox` is a
/// task item's [Checkbox] `start..end`, all a prefix cursor needs: chains are
/// copied per line, so the container stays as small as it was.
#[derive(Clone, Copy)]
struct Container { kind: ContainerKind, offset: usize, first_line: usize, checkbox: Option<(usize, usize)> }

/// A task item's checkbox as comrak's tasklist scanner took it, in source
/// bytes. `start` is the first byte the scanner read that is not a literal
/// space or tab (those are skipped before any checkbox is looked for): the
/// opening bracket, or whitespace written another way. `end` follows the
/// closing bracket and the one whitespace character after it, when there is
/// one and more follows it on its line. `symbol` is the character between
/// the brackets, which a reference may spell.
#[derive(Clone, Copy)]
struct Checkbox { start: usize, end: usize, symbol: (usize, usize) }

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
    prefix_start: usize,
    /// where the innermost prefix holding at least one byte begins: the one
    /// before [prefix_start] when that container has no bytes on the line
    held: usize }

/// `setext_definitions` marks a paragraph whose leading definitions comrak
/// resolved at a setext underline before a table split it (see
/// [Extractor::reattach_setext_definitions]). `split_end` is the end line
/// (1-based, in comrak's numbering) derived for a paragraph a table split
/// that comrak reports ending on its first line (see
/// [Extractor::reattach_split_paragraph_end]).
struct Leaf<'b> { node: &'b AstNode<'b>, idx: usize, container: Option<usize>, setext_definitions: bool, split_end: Option<usize> }

/// A container in the arena of containers: its data and the container above it.
struct ContainerNode { c: Container, parent: Option<usize>, block: usize }

/// Correction context for a leaf whose leading definitions comrak stripped:
/// comrak computes inline positions from the stripped content buffer but maps
/// them through the leaf's original per-line offsets, so a position reported
/// on original line `n` really lies on line `n + def_lines`.
pub struct Shift { lines: Vec<LineSpan>, def_lines: usize }

/// A cell buffer has an inherited column origin and unescaped pipes.
#[derive(Clone, Copy)]
struct Cell { start: usize, column_delta: isize }

fn shift_columns(mut sp: Sourcepos, delta: isize) -> Sourcepos {
    sp.start.column = sp.start.column.saturating_add_signed(delta);
    sp.end.column = sp.end.column.saturating_add_signed(delta);
    sp
}

/// One line of a paragraph-like leaf as comrak buffered it: `virt` is the
/// number of virtual spaces a partially consumed tab contributed on a lazy
/// line, which exist in comrak's buffer but not in the source. `held` is
/// [Prefix::held] in the source.
#[derive(Clone, Copy)]
struct LineSpan { line0: usize, start: usize, end: usize, virt: usize, prefix_start: usize, held: usize }

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
    /// Per-leaf memo for [Extractor::leaf_has_bare_cr].
    bare_cr_leaf: Option<bool>,
    pub deviations: Vec<Deviation>,
    /// Scratch for [leaf_run_problem], reused across leaves.
    sibling_end: Vec<usize>,
    /// comrak's decodings of the entity references text pieces display.
    entities: text_pieces::Entities,
    /// Escaped pipes before each position of the current cell or split
    /// paragraph ([Extractor::pipe_shift]).
    pipes: PipeCounts,
    /// The current leaf's text for link repairs.
    leaf_text: LeafText,
}

impl<'a> Extractor<'a> {
    /// The render model as little-endian u32 words; the string table is
    /// packed into the tail and padded to a whole word.
    pub fn extract(src: &'a str) -> Result<Vec<u32>, ExtractionError> {
        let (model, deviations) = Self::run(src, true);
        if deviations.iter().all(|d| d.leaf.is_some()) { Ok(model) } else { Err(ExtractionError { deviations }) }
    }
    pub fn extract_with_report(src: &'a str) -> (Vec<u32>, Vec<Deviation>) { Self::run(src, true) }

    fn new(src: &'a str, collect: bool) -> Self {
        Extractor { src, li: LineIndex::new(src), blocks: Vec::new(), content: Vec::new(), runs: Vec::new(), definitions: Vec::new(), strings: Vec::new(), containers: Vec::new(), collect, run_delta: 0, last_text_end: 0, bare_cr_leaf: None, deviations: Vec::new(), sibling_end: Vec::new(), entities: text_pieces::Entities::default(), pipes: PipeCounts::default(), leaf_text: LeafText::default() }
    }

    fn run(src: &'a str, collect: bool) -> (Vec<u32>, Vec<Deviation>) {
        let mut ex = Extractor::new(src, collect);
        let refusal = email_autolink_overflow(src)
            .map(|line| ("autolink-depth", format!("line {line} holds more than {MAX_EMAIL_AUTOLINKS_PER_LINE} possible email addresses")))
            .or_else(|| {
                let (filled, cap) = (filled_table_cells(src), MAX_FILLED_TABLE_CELLS.max(src.len() / 2));
                (filled > cap).then(|| ("table-cells", format!("tables could gain {filled} cells their rows lack, more than {cap}")))
            });
        if let Some((rule, detail)) = refusal {
            // Publish nothing past the document block: comrak is not run.
            let mut doc: BlockRec = [0; block::WORDS];
            doc[block::PARENT] = u32::MAX;
            doc[block::END_BYTE] = src.len() as u32; doc[block::END_UTF16] = ex.li.u16(src.len());
            doc[block::LINE_COUNT] = ex.li.line_count() as u32;
            ex.blocks.push(doc);
            ex.deviations.push(Deviation { rule, detail, leaf: None });
            let buf = ex.encode();
            return (buf, ex.deviations);
        }
        let arena = Arena::new();
        let root = parse_document(&arena, src, &options());
        let leaves = ex.walk_blocks(root);
        for leaf in &leaves { let chain = ex.chain(leaf.container); ex.leaf(leaf, &chain); }
        ex.widen_parents();
        ex.gap_definitions(&leaves);
        ex.definitions.sort_by_key(|d| d.start);
        ex.definitions.dedup_by_key(|d| d.start);
        ex.empty_container_lines();
        ex.fit_overlapping_containers();
        ex.check_structure();
        let buf = ex.encode();
        (buf, ex.deviations)
    }

    fn dev(&mut self, rule: &'static str, detail: impl FnOnce() -> String) { if self.collect { let d = detail(); self.deviations.push(Deviation { rule, detail: d, leaf: None }); } }

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
            // The split paragraph's end first: it restores the numbering the
            // setext reattachment reads.
            let split_end = self.reattach_split_paragraph_end(node, container_id);
            let setext_definitions = self.reattach_setext_definitions(node, container_id);
            let (idx, container, is_leaf) = self.block_record(node, parent, container_id);
            let inner = match container { Some(c) => { self.containers.push(ContainerNode { c, parent: container_id, block: idx }); Some(self.containers.len() - 1) } None => container_id };
            if is_leaf {
                leaves.push(Leaf { node, idx, container: inner, setext_definitions, split_end });
            } else {
                let children: Vec<_> = node.children().collect();
                for child in children.into_iter().rev() {
                    if child.data.borrow().value.block() { stack.push((child, idx as u32, inner)); }
                    else { leaves.push(Leaf { node: child, idx, container: inner, setext_definitions: false, split_end: None }); }
                }
            }
        }
        leaves.sort_by_key(|l| l.idx);
        leaves
    }

    /// comrak ends a paragraph a table header split
    /// (`try_inserting_table_header_paragraph`) at its last line's offset
    /// plus the bytes before that line's ending, and counts none before the
    /// `\r` of a CRLF. A last line added at offset 0, a lazy one, so ends at
    /// column 0, and a list or footnote definition finalized around it
    /// (`fix_zero_end_columns`) moves that end to the paragraph's start: the
    /// paragraph seems to end on its first line, short of the table that
    /// split it, its later lines in no block and its leading definitions,
    /// which a split paragraph keeps as text, taken for definitions. A
    /// paragraph reported to end on its first line, followed by its table
    /// with only its own continuation lines between, ends on the line before
    /// the table, at that line's end, as comrak numbers it without a CRLF; it
    /// is moved there before any position is read. Returns the derived end
    /// line (1-based), which the paragraph's own inlines must reach
    /// ([Extractor::check_split_paragraph_end]).
    fn reattach_split_paragraph_end<'b>(&self, node: &'b AstNode<'b>, container_id: Option<usize>) -> Option<usize> {
        if !matches!(node.data.borrow().value, NodeValue::Paragraph) { return None; }
        let table = node.next_sibling().filter(|t| matches!(t.data.borrow().value, NodeValue::Table(_)))?;
        let (psp, tsp) = (node.data.borrow().sourcepos, table.data.borrow().sourcepos);
        // `fix_zero_end_columns` falls back to the start itself; a task
        // checkbox taken afterwards moves the start's column, not the end.
        if psp.start.line == 0 || psp.end.line != psp.start.line || tsp.start.line <= psp.start.line + 1 { return None; }
        let last = tsp.start.line - 2;
        let chain = self.chain(container_id);
        if !(psp.start.line..=last).all(|l| l < self.li.line_count() && { let s = self.paragraph_line(l, &chain); s.start < s.end }) { return None; }
        let mut d = node.data.borrow_mut();
        d.sourcepos.end.line = last + 1;
        d.sourcepos.end.column = self.li.line_end(last, self.src.len()) - self.li.line_start(last);
        Some(last + 1)
    }

    /// comrak resolves a paragraph's leading definitions when a setext
    /// underline follows them (`handle_setext_heading`); with nothing left the
    /// underline becomes the paragraph's first line, but the paragraph keeps
    /// the definitions' start line. A table header that later splits the
    /// paragraph (`try_inserting_table_header_paragraph`) then numbers the
    /// split paragraph's end and the whole header row from that stale start:
    /// all of them sit as many lines early as the definitions take, and the
    /// header's columns take the offset of the line that many lines up. The
    /// positions are moved where comrak parsed them before any is read;
    /// `leaf` strips the definitions as it does for an unsplit paragraph, and
    /// the paragraph's and the header cells' literals validate the result.
    /// Only a table that split the paragraph is moved: the split numbers the
    /// paragraph's end and the header on consecutive lines. A table after a
    /// blank line came from a paragraph of its own and is numbered correctly;
    /// moving it left its header line in no block.
    fn reattach_setext_definitions<'b>(&self, node: &'b AstNode<'b>, container_id: Option<usize>) -> bool {
        let Some(table) = node.next_sibling().filter(|t| matches!(t.data.borrow().value, NodeValue::Table(_))) else { return false };
        if !matches!(node.data.borrow().value, NodeValue::Paragraph) { return false; }
        let (psp, tsp) = (node.data.borrow().sourcepos, table.data.borrow().sourcepos);
        if psp.start.line == 0 || tsp.start.line <= psp.start.line || tsp.start.line != psp.end.line + 1 { return false; }
        let chain = self.chain(container_id);
        let l0 = psp.start.line - 1;
        if self.src.as_bytes().get(self.paragraph_line(l0, &chain).start) != Some(&b'[') { return false; }
        // The definitions as resolved from the paragraph's first line. Lines
        // through the table's end, which comrak numbers correctly, hold them.
        let lines: Vec<LineSpan> = (l0..tsp.end.line.min(self.li.line_count())).map(|l| self.paragraph_line(l, &chain)).collect();
        let buffer = LineBuffer::new(self.src, &self.li, &lines).text;
        let Some(last) = reference_definitions::paragraph_definitions(&buffer).last().map(|d| d.end) else { return false };
        let k = buffer[..last].matches('\n').count();
        if k == 0 || !buffer[..last].ends_with('\n') || !self.setext_underline(l0 + k, &chain) { return false; }
        // comrak's split numbered the paragraph's last line and the header line
        // `newlines` and `newlines + 1` lines from the stale start.
        let newlines = tsp.start.line - psp.start.line;
        let column = |line0: usize| (self.paragraph_line(line0, &chain).start - self.li.line_start(line0)) as isize;
        let (end_delta, header_delta) = (column(l0 + newlines - 1 + k) - column(l0 + newlines - 1), column(l0 + newlines + k) - column(l0 + newlines));
        { let mut d = node.data.borrow_mut(); d.sourcepos.end.line += k; d.sourcepos.end.column = d.sourcepos.end.column.saturating_add_signed(end_delta); }
        { let mut d = table.data.borrow_mut(); d.sourcepos.start.line += k; d.sourcepos.start.column = d.sourcepos.start.column.saturating_add_signed(header_delta); }
        if let Some(header) = table.first_child() {
            for n in header.descendants() {
                let mut d = n.data.borrow_mut();
                if d.sourcepos.start.line == 0 { continue; }
                d.sourcepos = shift_columns(d.sourcepos, header_delta);
                d.sourcepos.start.line += k; d.sourcepos.end.line += k;
            }
        }
        true
    }

    /// Whether comrak reads [line0] as a setext underline under [containers]:
    /// every container matched, at most three columns of indentation, then a
    /// run of `=` or of `-` and only spaces or tabs (`setext_heading_line`).
    fn setext_underline(&self, line0: usize, containers: &[Container]) -> bool {
        if line0 >= self.li.line_count() { return false; }
        let p = self.prefix_cursor(line0, self.line_bytes(line0), containers);
        let mut cur = p.cur;
        cur.consume_columns(3);
        let rest = &cur.line[cur.pos..];
        let Some(&marker) = rest.first() else { return false };
        let run = rest.iter().take_while(|&&b| b == marker).count();
        !p.lazy && cur.virt == 0 && matches!(marker, b'=' | b'-') && rest[run..].iter().all(|b| matches!(b, b' ' | b'\t'))
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
                // comrak reports no NodeList for task items: the content column
                // is derived from the marker line.
                let chain = self.chain(container_id);
                let offset = self.marker_content_offset(first_line, &chain);
                rec[block::ATTR0] = offset as u32;
                let item = Container { kind: ContainerKind::Item, offset, first_line, checkbox: None };
                let symbol = t.symbol.unwrap_or(' ');
                let last_line = sp.end.line.saturating_sub(1).max(first_line);
                let checkbox = match self.task_checkbox(node, symbol, t.symbol_sourcepos, item, &chain, last_line) {
                    Ok(cb) => { rec[block::ATTR1] = cb.symbol.0 as u32; rec[block::ATTR2] = cb.symbol.1 as u32; Some(cb) }
                    // A task item without its checkbox's range cannot be
                    // toggled, nor its checkbox hidden: refuse rather than
                    // publish it.
                    Err(why) => { self.dev("task-checkbox", || format!("item at line {first_line}, symbol {symbol:?}: {why}")); None }
                };
                container = Some(Container { checkbox: checkbox.map(|cb| (cb.start, cb.end)), ..item });
            }
            NodeValue::ThematicBreak => rec[block::KIND] = block_kind::THEMATIC_BREAK,
            NodeValue::Table(t) => {
                rec[block::KIND] = block_kind::TABLE; rec[block::ATTR0] = t.num_columns as u32;
                let mut packed = 0u32;
                for (i, a) in t.alignments.iter().enumerate().take(16) { let v = match a { TableAlignment::None => table_alignment::NONE, TableAlignment::Left => table_alignment::LEFT, TableAlignment::Center => table_alignment::CENTER, TableAlignment::Right => table_alignment::RIGHT }; packed |= v << (2 * i); }
                rec[block::ATTR1] = packed;
                if t.alignments.len() > 16 && t.alignments[16..].iter().any(|a| !matches!(a, TableAlignment::None)) { self.dev("table-alignment-cap", || format!("{} columns; alignments beyond 16 dropped", t.alignments.len())); }
            }
            NodeValue::TableRow(h) => {
                rec[block::KIND] = block_kind::TABLE_ROW; rec[block::FLAGS] = *h as u32;
                // comrak starts a body row at the header's column rather than at
                // its own first nonspace, as it does the row's cells (translated
                // in `leaf`): a row indented unlike its header, or any row under a
                // header that follows a byte order mark, would start inside its
                // first cell or before its own line.
                if !*h {
                    let mut p = self.prefix_cursor(first_line, self.line_bytes(first_line), &self.chain(container_id));
                    p.cur.skip_whitespace();
                    let s = (self.li.line_start(first_line) + p.cur.pos).min(end);
                    rec[block::START_BYTE] = s as u32; rec[block::START_UTF16] = self.li.u16(s);
                }
            }
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
        if let Some(c) = container.filter(|c| c.kind == ContainerKind::Item) {
            // The column offset can consume only part of a tab. Export the
            // resulting source position; callers must never add columns to a
            // byte/UTF-16 offset. Leave a task checkbox outside this marker.
            let mut chain = self.chain(container_id);
            chain.push(Container { checkbox: None, ..c });
            let prefix = self.prefix_cursor(first_line, self.line_bytes(first_line), &chain);
            let marker_end = self.li.line_start(first_line) + prefix.cur.pos;
            rec[block::MARKER_END_BYTE] = marker_end as u32;
            rec[block::MARKER_END_UTF16] = self.li.u16(marker_end);
        }
        let idx = self.blocks.len();
        self.blocks.push(rec);
        (idx, container, is_leaf)
    }

    /// Content column of a task item relative to its enclosing container's
    /// content column, by cmark's padding rule: the marker plus the one to
    /// four columns of space after it, or plus one when the rest of the line
    /// is blank or five or more columns of space follow (indented code). A
    /// partially consumed tab before the marker counts from where the
    /// container's content begins, not from the tab stop.
    fn marker_content_offset(&self, line0: usize, containers: &[Container]) -> usize {
        let line = self.line_bytes(line0);
        let p = self.prefix_cursor(line0, line, containers);
        let base = p.cur.col - p.cur.virt;
        let mut cur = p.cur;
        cur.consume_columns(3);
        while matches!(cur.line.get(cur.pos), Some(b) if b.is_ascii_digit()) { cur.advance(1); }
        if matches!(cur.line.get(cur.pos), Some(b'-') | Some(b'+') | Some(b'*') | Some(b'.') | Some(b')')) { cur.advance(1); }
        let marker_end = cur.col;
        let spaces = cur.consume_columns(5);
        let rest_blank = cur.line[cur.pos..].iter().all(|b| matches!(b, b' ' | b'\t'));
        let padding = if rest_blank || spaces >= 5 || spaces == 0 { 1 } else { spaces };
        (marker_end + padding).saturating_sub(base).max(2)
    }

    /// The checkbox comrak's tasklist scanner took from task item [node]:
    /// `spacechar* '[' symbol ']' (spacechar | end)` over the text of the
    /// item's first paragraph once its leading definitions are resolved.
    /// comrak resolves them first, from the lines as written, and scans only
    /// after joining the text nodes entity references decode into. So the
    /// checkbox can sit on a line after definitions, a vertical tab or form
    /// feed is whitespace to it, and any of its characters can be a reference
    /// (`[ ]&#9;x`, `&#91;x]`). comrak's own symbol position is the text's
    /// reported start plus the symbol's offset in decoded text: stripped
    /// definitions leave that start on the paragraph's first line, and a
    /// reference before the symbol is longer than what it decodes to. The
    /// item's first checkbox-shaped bytes were no better: they can be a
    /// definition's destination (`1. [a]:[x]\n[x]`). Where neither applies,
    /// comrak's position must be the derived one.
    fn task_checkbox(&self, node: &AstNode<'_>, symbol: char, claimed: Sourcepos, item: Container, chain: &[Container], last_line: usize) -> Result<Checkbox, String> {
        let mut chain = chain.to_vec();
        chain.push(item);
        // The scanned paragraph was the item's first child when comrak
        // scanned: paragraphs of definitions alone were removed before. When
        // taking the checkbox emptied it, comrak removed it too, so it lies
        // before the item's first remaining child, after any such paragraphs
        // and blank lines; otherwise it is that child.
        let child = node.first_child().map(|c| { let d = c.data.borrow(); (matches!(d.value, NodeValue::Paragraph), d.sourcepos) });
        let region_end = child.map_or(last_line + 1, |(_, sp)| sp.start.line.saturating_sub(1)).min(last_line + 1);
        let mut l = item.first_line;
        while l < region_end {
            let lines: Vec<LineSpan> = (l..region_end).map(|k| self.paragraph_line(k, &chain)).take_while(|s| s.start < s.end).collect();
            l += lines.len().max(1);
            if let Some(found) = self.checkbox_after_definitions(&lines, symbol, claimed) { return found; }
        }
        match child {
            Some((true, sp)) => {
                let lines: Vec<LineSpan> = (sp.start.line.saturating_sub(1)..sp.end.line).map(|k| self.paragraph_line(k, &chain)).collect();
                self.checkbox_after_definitions(&lines, symbol, claimed).unwrap_or_else(|| Err(format!("the paragraph at line {} holds only definitions", sp.start.line)))
            }
            _ => Err(format!("no paragraph before line {region_end}")),
        }
    }

    /// The checkbox opening what is left of paragraph [lines] once its
    /// definitions are resolved, or `None` when nothing is left: comrak
    /// removed such a paragraph before it scanned for checkboxes.
    fn checkbox_after_definitions(&self, lines: &[LineSpan], symbol: char, claimed: Sourcepos) -> Option<Result<Checkbox, String>> {
        let buffer = LineBuffer::new(self.src, &self.li, lines);
        let defs = reference_definitions::paragraph_definitions(&buffer.text);
        // The definitions end with a line, so the text begins at one.
        let after = defs.last().map_or(0, |d| d.end);
        if after >= buffer.text.len() { return None; }
        let line = &lines[buffer.starts.iter().position(|&s| s == after)?];
        let line_end = self.li.line_end(line.line0, self.src.len());
        let Some(checkbox) = self.scan_checkbox(line.start, line_end, symbol) else {
            return Some(Err(format!("no checkbox at {:?}", &self.src[line.start..line_end])));
        };
        // Without definitions and references before the symbol, comrak's
        // position is exact, and the derivation must agree with it.
        if defs.is_empty() && !self.src[checkbox.start..checkbox.symbol.0].contains('&') {
            let at = sourcepos_range(claimed, &self.li, self.src.len()).map(|(s, _)| s);
            if at != Some(checkbox.symbol.0) { return Some(Err(format!("symbol derived at {} where comrak reports {at:?}", checkbox.symbol.0))); }
        }
        Some(Ok(checkbox))
    }

    /// comrak's tasklist pattern matched from [from] over the text comrak
    /// decodes the source to, each character written as itself or as an
    /// entity reference comrak decodes to it. The literal spaces and tabs
    /// leading it are not the checkbox's: a prefix cursor skips them first.
    fn scan_checkbox(&self, from: usize, line_end: usize, symbol: char) -> Option<Checkbox> {
        // The scanner's `spacechar`, the bytes 0x09 to 0x0D and the space.
        let spacechar = |c: char| matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ');
        let bytes = self.src.as_bytes();
        let mut i = from;
        while i < line_end && matches!(bytes[i], b' ' | b'\t') { i += 1; }
        let start = i;
        while let Some((_, next)) = self.decoded_char(i, line_end).filter(|&(c, _)| spacechar(c)) { i = next; }
        let mut take = |c: char| {
            let (_, next) = self.decoded_char(i, line_end).filter(|&(d, _)| d == c)?;
            let range = (i, next);
            i = next;
            Some(range)
        };
        take('[')?;
        let symbol = take(symbol)?;
        take(']')?;
        // comrak trims the spaces and tabs a text ends with before a line
        // ending, and those ending the paragraph: followed by nothing else on
        // its line, the checkbox takes no whitespace.
        if !bytes[i..line_end].iter().all(|b| matches!(b, b' ' | b'\t')) {
            if let Some((_, next)) = self.decoded_char(i, line_end).filter(|&(c, _)| spacechar(c)) { i = next; }
        }
        Some(Checkbox { start, end: i, symbol })
    }

    /// The character comrak's text holds for the source at [i], and where
    /// the source after it begins. An entity reference comrak decodes stands
    /// for its decoding, and for no single character when that is longer.
    fn decoded_char(&self, i: usize, end: usize) -> Option<(char, usize)> {
        let rest = self.src.get(i..end)?;
        if let Some(l) = text_pieces::entity_len(rest) {
            let shown = self.entities.decode(&rest[..l]);
            if shown != rest[..l] {
                let mut chars = shown.chars();
                return match (chars.next(), chars.next()) { (Some(c), None) => Some((c, i + l)), _ => None };
            }
        }
        rest.chars().next().map(|c| (c, i + c.len_utf8()))
    }

    // ------------------------------------------------------------- per line

    /// Consume container prefixes on one physical line.
    fn prefix_cursor<'l>(&self, line0: usize, line: &'l [u8], containers: &[Container]) -> Prefix<'l> {
        // comrak skips a byte order mark that begins the first line without
        // counting a column (`process_line`), so every derivation on that line
        // starts after it: the mark is neither content nor a liftable prefix.
        let bom = if line0 == 0 && line.starts_with("\u{feff}".as_bytes()) { 3 } else { 0 };
        let mut cur = ColCursor { pos: bom, ..ColCursor::new(line) };
        let mut lazy = false;
        let mut after_checkbox = false;
        let mut base = 0usize;
        let mut prefix_start = bom;
        // A blank line short of an item's indentation keeps the prefix start a
        // lazy line would have (where the indentation fell short) for editing,
        // while its content follows cmark past the whitespace.
        let mut prefix_frozen = false;
        let mut held = bom;
        for c in containers {
            let at = cur.pos;
            let frozen = prefix_frozen;
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
                        if !prefix_frozen { prefix_start = at; }
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
                        // it is there; a lazy line keeps its partial spaces. A
                        // blank line still continues an item that holds a
                        // block, advanced to its first non-space: the lines of
                        // a leaf after its first always have one.
                        let save = cur;
                        let need = target.saturating_sub(cur.col - cur.virt);
                        let got = cur.consume_columns(need);
                        if got < need {
                            cur = save;
                            if cur.line[cur.pos..].iter().all(|b| matches!(b, b' ' | b'\t')) {
                                if !prefix_frozen { prefix_start = at; prefix_frozen = true; }
                                cur.skip_whitespace();
                            } else { lazy = true; break; }
                        }
                    }
                    if !prefix_frozen { prefix_start = at; }
                    // The task checkbox is skipped on whichever line comrak found it
                    // (the item's first paragraph line), with the whitespace
                    // character after it. The checkbox leads the paragraph's
                    // first line, which cmark strips of whitespace first: it
                    // can sit past extra spaces or a partially consumed tab.
                    if let Some((start, end)) = c.checkbox {
                        let mut probe = cur;
                        probe.skip_whitespace();
                        if self.li.line_start(line0) + probe.pos == start {
                            cur = probe;
                            cur.advance(end - start);
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
                        if !prefix_frozen { prefix_start = at; }
                    } else {
                        let target = base + 4;
                        let save = cur;
                        let need = target.saturating_sub(cur.col - cur.virt);
                        let got = cur.consume_columns(need);
                        if got < need { cur = save; lazy = true; break; }
                        base = target;
                        if !prefix_frozen { prefix_start = at; }
                    }
                }
            }
            if !frozen && cur.pos > at { held = at; }
        }
        if lazy {
            // A lazy line still holds a task item's checkbox when definitions
            // stripped before it left it leading the paragraph, whichever
            // container the line falls short of: the item's own indentation,
            // or a quote or list item it is nested in. The checkbox's
            // paragraph is its item's first child, so only the innermost
            // container can own it; looking further cost every lazy line the
            // chain's depth.
            if let Some((start, end)) = containers.last().and_then(|c| c.checkbox) {
                let mut probe = cur;
                probe.skip_whitespace();
                if self.li.line_start(line0) + probe.pos == start {
                    cur = probe;
                    cur.advance(end - start);
                    after_checkbox = true;
                }
            }
            prefix_start = cur.pos;
            held = cur.pos;
        }
        Prefix { cur, lazy, after_checkbox, prefix_start, held }
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
        let ls = self.li.line_start(line0);
        let start = ls + p.cur.pos;
        LineSpan { line0, start, end: self.trimmed_end(line0, start), virt: if p.lazy { p.cur.virt } else { 0 }, prefix_start: ls + p.prefix_start, held: ls + p.held }
    }

    fn push_content(&mut self, line0: usize, cs: usize, ce: usize, virt: usize, prefix_start: usize) {
        let ce = ce.max(cs);
        let ps = prefix_start.min(cs);
        self.content.push([line0 as u32, cs as u32, self.li.u16(cs), ce as u32, self.li.u16(ce), virt as u32, ps as u32, self.li.u16(ps)]);
    }

    /// Empty container lines have no leaf AST node. Publish their authenticated
    /// prefix range too, so hosts can exit one quote/item without scanning
    /// markers or accidentally removing all enclosing containers.
    fn empty_container_lines(&mut self) {
        let lines = self.li.line_of(self.src.len()) + 1;
        let mut claimed = vec![false; lines];
        for b in &self.blocks {
            if matches!(b[block::KIND], block_kind::PARAGRAPH | block_kind::HEADING | block_kind::CODE_BLOCK | block_kind::HTML_BLOCK | block_kind::THEMATIC_BREAK | block_kind::TABLE) {
                let first = b[block::FIRST_LINE] as usize;
                let end = (first + b[block::LINE_COUNT] as usize).min(lines);
                for line in first..end { claimed[line] = true; }
            }
        }
        let spans: Vec<(usize, usize)> = self.containers.iter().map(|c| {
            let b = &self.blocks[c.block];
            (self.li.line_of(b[block::START_BYTE] as usize), self.li.line_of((b[block::END_BYTE] as usize).saturating_sub(1)))
        }).collect();
        let inner = innermost_per_line(lines, &claimed, &spans);
        let mut by_container = vec![Vec::new(); self.containers.len()];
        for (line, owner) in inner.into_iter().enumerate() {
            if let Some(i) = owner { by_container[i].push(line); }
        }
        for (i, owned) in by_container.into_iter().enumerate() {
            if owned.is_empty() { continue; }
            let block_index = self.containers[i].block;
            let chain = self.chain(Some(i));
            self.blocks[block_index][block::CONTENT_OFFSET] = self.content.len() as u32;
            // comrak continues a list item over its container's blank lines
            // without the item's indentation (`parse_node_item_prefix`), so
            // the item has no prefix there: the line's innermost prefix is the
            // enclosing one. comrak matches every enclosing prefix before it
            // continues the item, so that one is taken only on lines inside
            // comrak's own line range for the container, which the extraction
            // refits for leaves alone.
            let (first, count) = (self.blocks[block_index][block::FIRST_LINE] as usize, self.blocks[block_index][block::LINE_COUNT] as usize);
            for line in owned {
                let p = self.paragraph_line(line, &chain);
                if p.start != p.end { continue; }
                let prefix = if p.prefix_start == p.start && (first..first + count).contains(&line) { p.held } else { p.prefix_start };
                self.push_content(line, p.start, p.end, 0, prefix);
            }
            self.blocks[block_index][block::CONTENT_COUNT] = self.content.len() as u32 - self.blocks[block_index][block::CONTENT_OFFSET];
            // comrak can end a container short of its own empty lines: an
            // empty footnote definition followed by a blank line inside an
            // item ends at its label's first byte. The block covers them.
            let first = self.blocks[block_index][block::CONTENT_OFFSET] as usize;
            for c in first..self.content.len() {
                let (cs, ce) = (self.content[c][content::START_BYTE], self.content[c][content::END_BYTE]);
                let b = &mut self.blocks[block_index];
                if cs < b[block::START_BYTE] { b[block::START_BYTE] = cs; b[block::START_UTF16] = self.li.u16(cs as usize); }
                if ce > b[block::END_BYTE] { b[block::END_BYTE] = ce; b[block::END_UTF16] = self.li.u16(ce as usize); }
            }
        }
        self.widen_parents();
    }

    fn leaf(&mut self, leaf: &Leaf<'_>, chain: &[Container]) {
        let idx = leaf.idx;
        let data = leaf.node.data.borrow();
        let sp = data.sourcepos;
        let content_start = self.content.len() as u32;
        let first_run = self.runs.len();
        let first_deviation = self.deviations.len();
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
                // comrak's end column counts a partially consumed tab's virtual
                // spaces, so it can run past the block's last line into the next
                // block. The block ends with its last line.
                if let Some(last) = self.content.get(content_start as usize..).and_then(|c| c.last()) {
                    let le = self.li.line_end(last[content::LINE] as usize, self.src.len());
                    if be > le { self.blocks[idx][block::END_BYTE] = le as u32; self.blocks[idx][block::END_UTF16] = self.li.u16(le); }
                }
            }
            NodeValue::ThematicBreak => {
                // comrak's rule sourcepos is one column inside a container and
                // runs past the line at a document's end. A rule is exactly its
                // line's content, and a host that deletes it needs that range.
                let mut p = self.prefix_cursor(l0, self.line_bytes(l0), chain);
                p.cur.skip_whitespace();
                let ls = self.li.line_start(l0);
                let (cs, ce) = (ls + p.cur.pos, self.trimmed_end(l0, ls + p.cur.pos));
                let rec = &mut self.blocks[idx];
                rec[block::START_BYTE] = cs as u32; rec[block::START_UTF16] = self.li.u16(cs);
                rec[block::END_BYTE] = ce as u32; rec[block::END_UTF16] = self.li.u16(ce);
            }
            NodeValue::TableCell => {
                // Comrak parses a body row from first_nonspace, but records its
                // cells and inline line_offsets relative to the HEADER's column.
                // Translate that origin before reading any literal or unescaping
                // pipes; repeated text cannot authenticate a shifted position.
                let parent = leaf.node.parent().unwrap().data.borrow();
                let column_delta = if matches!(parent.value, NodeValue::TableRow(false)) {
                    let mut p = self.prefix_cursor(l0, self.line_bytes(l0), chain);
                    p.cur.skip_whitespace();
                    (p.cur.pos + 1) as isize - parent.sourcepos.start.column as isize
                } else { 0 };
                let corrected = shift_columns(sp, column_delta);
                let (mut cs, mut ce) = sourcepos_range(corrected, &self.li, self.src.len()).unwrap_or((0, 0));
                let bytes = self.src.as_bytes();
                // A row short of the header's columns is filled with cells whose
                // sourcepos is the row's closing delimiter, or its line break
                // when the row has none; several missing cells repeat the one
                // position. A cell's content can never begin at an unescaped
                // pipe or cross the line end, so a childless cell there is the
                // implicit empty cell, not the delimiter it points at.
                let le = self.li.line_end(l0, self.src.len());
                ce = ce.min(le);
                cs = cs.min(ce);
                if cs < ce && bytes[cs] == b'|' && leaf.node.first_child().is_none() { ce = cs; }
                let rec = &mut self.blocks[idx];
                rec[block::START_BYTE] = cs as u32; rec[block::START_UTF16] = self.li.u16(cs);
                rec[block::END_BYTE] = ce as u32; rec[block::END_UTF16] = self.li.u16(ce);
                let mut a = cs;
                while a < ce && bytes[a] == b' ' { a += 1; }
                // One space or tab before a closing pipe is the pipe's
                // separator, as a closing heading sequence keeps its own: the
                // cell's text, and a caret at its end, stop after its last
                // character. Whitespace before that is the cell's text, so
                // editing gives whitespace typed against the pipe a
                // separator after it, and what was typed stays the cell's.
                let mut e = ce;
                if e > a && e < le && bytes[e] == b'|' && matches!(bytes[e - 1], b' ' | b'\t') {
                    e -= 1;
                }
                self.push_content(l0, a, e, 0, a);
                self.walk_inlines(leaf.node, idx as u32, Some(Cell { start: cs, column_delta }), None);
            }
            NodeValue::Heading(h) if !h.setext => {
                let (span_s, span_e) = self.children_span(leaf.node, None, None);
                let mut span = self.paragraph_line(l0, chain);
                let bytes = self.src.as_bytes();
                let (mut cs, mut ce) = (span.start, span.end);
                let mut p = cs; while p < ce && bytes[p] == b'#' { p += 1; }
                // paragraph_line trims trailing whitespace. In an empty ATX
                // heading that includes the required opening separator, so
                // consume it against the physical line end. It is prefix,
                // not an editable leading space before the first character.
                let line_end = self.li.line_end(l0, self.src.len());
                while p < line_end && (bytes[p] == b' ' || bytes[p] == b'\t') { p += 1; }
                cs = p;
                let mut q = ce; while q > cs && bytes[q - 1] == b'#' { q -= 1; }
                if q < ce && (q == cs || bytes[q - 1] == b' ' || bytes[q - 1] == b'\t') {
                    // Keep the required separator with the closing marker;
                    // additional spaces remain editable content.
                    ce = if q > cs { q - 1 } else { q };
                } else {
                    ce = self.li.line_end(l0, self.src.len());
                }
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
                // comrak resolves definitions before it removes a task checkbox,
                // and `[x] ` cannot begin one: a paragraph led by its item's
                // checkbox strips no definitions.
                let led_by_checkbox = self.prefix_cursor(l0, self.line_bytes(l0), chain).after_checkbox;
                // A split paragraph keeps its definitions as text, unless a setext
                // underline had resolved them first: then they left the paragraph
                // and only their lines remain in it.
                let first_definition = self.definitions.len();
                let (shift, records) = self.strip_definitions(l0, last, chain, (!split_by_table || leaf.setext_definitions) && !led_by_checkbox);
                for r in records {
                    // The parser buffer trims trailing whitespace, but an
                    // editing content span must retain it. Otherwise typing a
                    // space legalizes the caret backwards and the next letter
                    // is inserted before that space. Break-marker runs still
                    // authenticate any whitespace hidden by the projection.
                    let end = self.li.line_end(r.line0, self.src.len());
                    self.push_content(r.line0, r.start, end, 0, r.prefix_start);
                }
                // That split paragraph also had its pipes unescaped, so its inline
                // positions carry the same shift as a table cell's.
                let pipes = if split_by_table { Some(Cell { start: self.blocks[idx][block::START_BYTE] as usize, column_delta: 0 }) } else { None };
                self.walk_inlines(leaf.node, idx as u32, pipes, shift.as_ref());
                self.check_stripped_definitions(idx, first_run, first_definition);
                if let Some(end) = leaf.split_end { self.check_split_paragraph_end(idx, leaf.node, end); }
            }
            _ => { self.walk_inlines(leaf.node, idx as u32, None, None); }
        }
        let inline_leaf = !matches!(data.value, NodeValue::CodeBlock(_) | NodeValue::HtmlBlock(_) | NodeValue::ThematicBreak);
        drop(data);
        if inline_leaf { self.settle_inlines(idx, first_run, first_deviation, content_start as usize); }
        self.blocks[idx][block::CONTENT_COUNT] = self.content.len() as u32 - content_start;
        self.fit_block_to_content(idx);
        self.trim_trailing_blank_lines(idx, chain);
        self.fit_block_to_runs(idx, first_run);
    }

    /// A leaf whose inline extraction deviated, or whose runs do not nest and
    /// follow each other or leave content outside every childless run and
    /// delimiter, publishes without runs: it shows its source as plain text
    /// and stays editable, while the rest of the document renders. Its
    /// deviations are scoped to it; any other deviation still refuses.
    fn settle_inlines(&mut self, idx: usize, first_run: usize, first_deviation: usize, content_from: usize) {
        if let Some(problem) = self.leaf_run_problem(first_run) { self.dev("run-structure", || format!("block {idx}: {problem}")); }
        else if let Some(x) = self.uncovered_content(first_run, content_from) { self.dev("run-structure", || format!("block {idx}: content byte {x} lies in no childless run or delimiter")); }
        let mut degrade = false;
        for d in &mut self.deviations[first_deviation..] {
            if INLINE_RULES.contains(&d.rule) { d.leaf = Some(idx as u32); degrade = true; }
        }
        if degrade {
            self.runs.truncate(first_run);
            self.blocks[idx][block::FLAGS] |= SOURCE_ONLY;
        }
    }

    /// The first content byte of a leaf with runs that none of them covers,
    /// other than editable whitespace (cell padding, spaces before a break).
    /// A run without children covers its whole range; a run with children
    /// covers only its delimiters, the source outside its content range,
    /// since a host paints its children and shows any byte between them
    /// verbatim. Such a byte would show text a replacement run already
    /// displays twice, or an entity cut short undecoded. Content records are
    /// in line order, so one sweep does.
    fn uncovered_content(&self, first_run: usize, content_from: usize) -> Option<usize> {
        let runs = &self.runs[first_run..];
        if runs.is_empty() { return None; }
        let mut has_children = vec![false; runs.len()];
        for r in runs {
            let p = r[run::PARENT] as usize;
            if r[run::PARENT] != u32::MAX && (first_run..self.runs.len()).contains(&p) { has_children[p - first_run] = true; }
        }
        let mut spans: Vec<(usize, usize)> = Vec::with_capacity(runs.len() + 1);
        for (r, &parent) in runs.iter().zip(&has_children) {
            let (s, e) = (r[run::START_BYTE] as usize, r[run::END_BYTE] as usize);
            if parent { spans.push((s, r[run::CONTENT_START_BYTE] as usize)); spans.push((r[run::CONTENT_END_BYTE] as usize, e)); } else { spans.push((s, e)); }
        }
        spans.sort_unstable();
        let bytes = self.src.as_bytes();
        let (mut k, mut reach) = (0, 0);
        for c in &self.content[content_from..] {
            for x in c[content::START_BYTE] as usize..c[content::END_BYTE] as usize {
                while k < spans.len() && spans[k].0 <= x { reach = reach.max(spans[k].1); k += 1; }
                if x >= reach && !matches!(bytes[x], b' ' | b'\t') { return Some(x); }
            }
        }
        None
    }

    /// Why the runs from [first_run], one leaf's, cannot be painted: a run
    /// whose content lies outside it, whose parent is not an earlier run of
    /// the leaf or does not contain it, or which starts before its previous
    /// sibling ends. The projection walks runs in order, so an overlap would
    /// show the shared source twice.
    fn leaf_run_problem(&mut self, first_run: usize) -> Option<String> {
        let mut sibling_end = std::mem::take(&mut self.sibling_end);
        let problem = self.run_problem_with(first_run, &mut sibling_end);
        self.sibling_end = sibling_end;
        problem
    }

    fn run_problem_with(&self, first_run: usize, sibling_end: &mut Vec<usize>) -> Option<String> {
        let runs = &self.runs[first_run..];
        // Slot zero is the leaf itself; slot i + 1 holds run i's children.
        sibling_end.clear();
        sibling_end.resize(runs.len() + 1, 0);
        for (i, r) in runs.iter().enumerate() {
            let at = first_run + i;
            let (s, e, cs, ce) = (r[run::START_BYTE] as usize, r[run::END_BYTE] as usize, r[run::CONTENT_START_BYTE] as usize, r[run::CONTENT_END_BYTE] as usize);
            if !(s <= cs && cs <= ce && ce <= e) { return Some(format!("run {at} order {s} {cs} {ce} {e}")); }
            let p = r[run::PARENT];
            let slot = if p == u32::MAX { 0 } else {
                let p = p as usize;
                if p < first_run || p >= at { return Some(format!("run {at} parent {p} outside the leaf")); }
                let (ps, pe) = (self.runs[p][run::START_BYTE] as usize, self.runs[p][run::END_BYTE] as usize);
                if s < ps || e > pe { return Some(format!("run {at} {s}..{e} outside its parent {p} {ps}..{pe}")); }
                p - first_run + 1
            };
            if s < sibling_end[slot] { return Some(format!("run {at} {s}..{e} overlaps its previous sibling, which ends at {}", sibling_end[slot])); }
            sibling_end[slot] = e;
        }
        None
    }

    /// A container that runs into the block after it ends with its own
    /// children and empty lines. comrak can end one past them: a list or item
    /// whose last child is an HTML block after a partial tab (a column too
    /// far), or an item after a tab-indented continuation line (a line too
    /// far). Only a container that overlaps its next sibling is refitted.
    fn fit_overlapping_containers(&mut self) {
        let n = self.blocks.len();
        let mut next_start: Vec<Option<u32>> = vec![None; n];
        let mut last_child: Vec<Option<usize>> = vec![None; n + 1];
        for i in 1..n {
            let p = self.blocks[i][block::PARENT];
            if p == u32::MAX { continue; }
            if let Some(previous) = last_child[p as usize] { next_start[previous] = Some(self.blocks[i][block::START_BYTE]); }
            last_child[p as usize] = Some(i);
        }
        let container = |k: u32| matches!(k, block_kind::BLOCK_QUOTE | block_kind::LIST | block_kind::ITEM | block_kind::FOOTNOTE_DEFINITION);
        // How far each block's own content and children reach, whatever
        // comrak said a container's end was. Children follow their parent,
        // so a reverse walk sees every child first.
        let mut reach: Vec<u32> = vec![0; n];
        for i in (0..n).rev() {
            let (co, cn) = (self.blocks[i][block::CONTENT_OFFSET] as usize, self.blocks[i][block::CONTENT_COUNT] as usize);
            for c in self.content.get(co..co + cn).unwrap_or(&[]) { reach[i] = reach[i].max(c[content::END_BYTE]); }
            if !container(self.blocks[i][block::KIND]) { reach[i] = reach[i].max(self.blocks[i][block::END_BYTE]); }
            let p = self.blocks[i][block::PARENT];
            if p != u32::MAX { reach[p as usize] = reach[p as usize].max(reach[i]); }
        }
        // Parents first: a container overlapping its next sibling ends at its
        // reach, and every container stays inside its parent.
        for i in 1..n {
            if !container(self.blocks[i][block::KIND]) { continue; }
            let p = self.blocks[i][block::PARENT];
            let parent_end = if p == u32::MAX { u32::MAX } else { self.blocks[p as usize][block::END_BYTE] };
            let (start, end) = (self.blocks[i][block::START_BYTE], self.blocks[i][block::END_BYTE]);
            if next_start[i].is_some_and(|ns| end > ns) || end > parent_end {
                let e = reach[i].max(start).min(parent_end.max(start));
                if e < end { self.blocks[i][block::END_BYTE] = e; self.blocks[i][block::END_UTF16] = self.li.u16(e as usize); }
            }
        }
    }

    /// The model's structural invariants, checked before publishing: blocks
    /// in document order, each inside its parent and clear of its previous
    /// sibling; content records on their own line and inside their block;
    /// runs in block order and inside their block. A host trusts all of these
    /// to map carets and paint, so a model that breaks one is refused.
    fn check_structure(&mut self) {
        let src_len = self.src.len();
        let mut problem: Option<(&'static str, String)> = None;
        // The previous sibling under each parent, as the test invariant compares neighbours.
        let mut sibling = vec![(0usize, 0usize); self.blocks.len() + 1];
        let mut previous_start = 0usize;
        for (i, b) in self.blocks.iter().enumerate() {
            let (s, e) = (b[block::START_BYTE] as usize, b[block::END_BYTE] as usize);
            if s > e || e > src_len { problem = Some(("block-tree", format!("block {i} range {s}..{e}"))); break; }
            if i == 0 { continue; }
            let p = b[block::PARENT] as usize;
            if p >= i { problem = Some(("block-tree", format!("block {i} parent {p}"))); break; }
            let (ps, pe) = (self.blocks[p][block::START_BYTE] as usize, self.blocks[p][block::END_BYTE] as usize);
            if s < ps || e > pe { problem = Some(("block-tree", format!("block {i} {s}..{e} outside its parent {p} {ps}..{pe}"))); break; }
            if s < previous_start { problem = Some(("block-tree", format!("block {i} starts before the block before it"))); break; }
            previous_start = s;
            let (qs, qe) = sibling[p];
            if s < qe && e > qs { problem = Some(("block-tree", format!("block {i} {s}..{e} overlaps its previous sibling {qs}..{qe}"))); break; }
            sibling[p] = (s, e);
        }
        if problem.is_none() {
            'blocks: for (i, b) in self.blocks.iter().enumerate() {
                let (s, e) = (b[block::START_BYTE] as usize, b[block::END_BYTE] as usize);
                let (co, cn) = (b[block::CONTENT_OFFSET] as usize, b[block::CONTENT_COUNT] as usize);
                let mut last_line: Option<usize> = None;
                for c in self.content.get(co..co + cn).unwrap_or(&[]) {
                    let (cs, ce, line, ps) = (c[content::START_BYTE] as usize, c[content::END_BYTE] as usize, c[content::LINE] as usize, c[content::PREFIX_START_BYTE] as usize);
                    let (ls, le) = (self.li.line_start(line), self.li.line_end_with_break(line, src_len));
                    if cs > ce || cs < s || ce > e + 1 || cs < ls || ce > le || ps < ls || ps > cs || last_line.is_some_and(|l| line <= l) {
                        problem = Some(("content-structure", format!("block {i} content {cs}..{ce} on line {line}")));
                        break 'blocks;
                    }
                    last_line = Some(line);
                }
            }
        }
        if problem.is_none() {
            let mut previous_block = 0u32;
            for (i, r) in self.runs.iter().enumerate() {
                let b = r[run::BLOCK];
                if b < previous_block || b as usize >= self.blocks.len() { problem = Some(("run-block", format!("run {i} block {b}"))); break; }
                previous_block = b;
                let (s, e) = (r[run::START_BYTE] as usize, r[run::END_BYTE] as usize);
                let (bs, be) = (self.blocks[b as usize][block::START_BYTE] as usize, self.blocks[b as usize][block::END_BYTE] as usize);
                if s < bs || e > be + 1 { problem = Some(("run-block", format!("run {i} {s}..{e} outside block {b} {bs}..{be}"))); break; }
            }
        }
        if let Some((rule, detail)) = problem { self.dev(rule, || detail); }
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
    /// comrak's leaf sourcepos can run past the last line the leaf has content
    /// for, onto blank lines that follow: a thematic break or an unclosed fence
    /// ending a document, an indented code block before a blank separator. A
    /// blank line is document structure, not the leaf's markup — leaving it
    /// inside makes it a line the projected row owns and can never show, so its
    /// caret would paint at the end of the line above. Lines that carry markup
    /// the row hides (a closing fence, a setext underline, a container prefix)
    /// are not blank and stay with the leaf.
    fn trim_trailing_blank_lines(&mut self, idx: usize, chain: &[Container]) {
        let (co, cn) = (self.blocks[idx][block::CONTENT_OFFSET] as usize, self.blocks[idx][block::CONTENT_COUNT] as usize);
        let first_line = self.blocks[idx][block::FIRST_LINE] as usize;
        let content_line = if cn == 0 { first_line } else { self.content[co + cn - 1][content::LINE] as usize };
        let mut lines = self.blocks[idx][block::LINE_COUNT] as usize;
        let bytes = self.src.as_bytes();
        while lines > 1 && first_line + lines - 1 > content_line {
            let line = first_line + lines - 1;
            let mut p = self.prefix_cursor(line, self.line_bytes(line), chain);
            p.cur.skip_whitespace();
            let (ls, le) = (self.li.line_start(line), self.li.line_end(line, self.src.len()));
            if ls + p.cur.pos < le && !bytes[ls + p.cur.pos..le].iter().all(|b| matches!(b, b' ' | b'\t')) { break; }
            lines -= 1;
        }
        if lines == self.blocks[idx][block::LINE_COUNT] as usize { return; }
        let end = self.li.line_end(first_line + lines - 1, self.src.len());
        let rec = &mut self.blocks[idx];
        rec[block::LINE_COUNT] = lines as u32;
        if (rec[block::END_BYTE] as usize) > end {
            rec[block::END_BYTE] = end as u32;
            rec[block::END_UTF16] = self.li.u16(end);
        }
    }

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

    /// The end derived for a paragraph a table split
    /// ([Extractor::reattach_split_paragraph_end]) is where comrak's own
    /// inlines end: comrak numbers them by the line endings they cross, which
    /// the moved end does not touch. Its last inline ends on line [end]
    /// (1-based), or the derivation is wrong and the model is refused. The
    /// text literal checks then place each inline on those lines.
    fn check_split_paragraph_end<'b>(&mut self, idx: usize, node: &'b AstNode<'b>, end: usize) {
        let reach = node.descendants().skip(1).map(|n| n.data.borrow().sourcepos.end.line).max();
        if reach != Some(end) { self.dev("split-paragraph-end", || format!("block {idx}: comrak's inlines end on line {reach:?}, the derived end is line {end}")); }
    }

    /// comrak removes a leaf's leading definitions before it parses inlines,
    /// so none of the leaf's inlines, the runs from [first_run], lies among
    /// the definitions recorded for it from [first_definition]. One that
    /// does marks text comrak kept and the mirror took for a definition: the
    /// leaf has no content there and a host would show a definition's row
    /// over it. Checked before an inline deviation can drop the runs, and
    /// refused. The definitions open the leaf, so their span from the first
    /// one's start to the last one's end is all a run must stay out of.
    fn check_stripped_definitions(&mut self, idx: usize, first_run: usize, first_definition: usize) {
        let (Some(first), Some(last)) = (self.definitions.get(first_definition), self.definitions.last()) else { return };
        let (from, to) = (first.start, last.end);
        let Some(r) = self.runs[first_run..].iter().find(|r| (r[run::START_BYTE] as usize) < to && r[run::END_BYTE] as usize > from) else { return };
        let (s, e) = (r[run::START_BYTE], r[run::END_BYTE]);
        self.dev("definition-inline", || format!("block {idx}: an inline at {s}..{e} lies among its definitions {from}..{to}"));
    }

    /// Per-line content for a paragraph-like leaf over lines l0..=l1, with the
    /// definitions comrak consumed from its start removed (mirroring
    /// `resolve_reference_link_definitions`).
    fn strip_definitions(&mut self, l0: usize, l1: usize, containers: &[Container], allow: bool) -> (Option<Shift>, Vec<LineSpan>) {
        let lines: Vec<LineSpan> = (l0..=l1).map(|line0| self.paragraph_line(line0, containers)).collect();
        // comrak resolves definitions, and places inlines, in the lines as
        // written: a task checkbox leaves the text only afterwards. Read
        // without it, the line after a definition let the definition run on
        // (`- [a]: /u\n  [x] "t"` took `"t"` for its title), and every inline
        // after the checkbox was placed its width too far right.
        let written: Vec<LineSpan> = if containers.iter().any(|c| c.checkbox.is_some()) {
            let as_written: Vec<Container> = containers.iter().map(|c| Container { checkbox: None, ..*c }).collect();
            (l0..=l1).map(|line0| self.paragraph_line(line0, &as_written)).collect()
        } else { lines.clone() };
        let first_byte = written.first().and_then(|l| self.src.as_bytes().get(l.start)).copied();
        let mut def_lines = 0;
        if allow && first_byte == Some(b'[') {
            let buffer = LineBuffer::new(self.src, &self.li, &written);
            let defs = reference_definitions::paragraph_definitions(&buffer.text);
            if !defs.is_empty() { def_lines = self.record_buffer_definitions(&written, &buffer, &defs, false, None); }
        }
        let records = lines[def_lines..].to_vec();
        let needs_geometry = def_lines > 0 || written.iter().any(|l| l.virt > 0);
        (if needs_geometry { Some(Shift { lines: written, def_lines }) } else { None }, records)
    }

    /// Map definitions found in a line buffer back to source and record them.
    /// Returns how many whole lines the definitions consumed.
    /// With [exact], the lines must hold nothing else, save [checkbox]: the
    /// checkbox of the task item they belong to, which comrak takes from what
    /// the definitions left (a checkbox alone on the line after them leaves no
    /// paragraph at all).
    fn record_buffer_definitions(&mut self, lines: &[LineSpan], buffer: &LineBuffer, defs: &[reference_definitions::BufferDefinition], exact: bool, checkbox: Option<(usize, usize)>) -> usize {
        // Definitions have several independently addressed endpoints. The
        // buffer indexes its lines once; rescanning it for every endpoint made
        // a paragraph of definitions quadratic in its line count.
        let to_byte = |off: usize| buffer.byte(lines, off);
        let mut consumed_lines = 0usize;
        for d in defs {
            // A definition ends at a line end; its source range runs through
            // that line's terminator, never into the next line's prefix.
            let last_line = buffer.starts.partition_point(|&start| start < d.end).saturating_sub(1).min(lines.len() - 1);
            let start = to_byte(d.start);
            let end = self.li.line_end_with_break(lines[last_line].line0, self.src.len());
            self.definitions.push(Definition { start, end, label: (to_byte(d.label.0), to_byte(d.label.1)), dest: (to_byte(d.dest.0), to_byte(d.dest.1)) });
            consumed_lines = last_line + 1;
        }
        if exact {
            let tail = defs.last().map(|d| d.end).unwrap_or(0);
            let rest = &buffer.text[tail.min(buffer.text.len())..];
            let blank = [' ', '\t', '\r', '\n'];
            let lead = rest.len() - rest.trim_start_matches(blank).len();
            let left = rest.trim_matches(blank);
            // The checkbox ends past the space or tab it takes, which the
            // remainder leaves out at the end of its line.
            let src = self.src;
            let is_checkbox = !left.contains('\n') && checkbox.is_some_and(|(start, end)| {
                let (at, to) = (to_byte(tail + lead), to_byte(tail + lead) + left.len());
                at == start && to <= end && src.as_bytes()[to..end].iter().all(|b| matches!(b, b' ' | b'\t'))
            });
            if !left.is_empty() && !is_checkbox {
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
        let spans: Vec<(usize, usize)> = self.containers.iter().map(|cn| {
            let b = &self.blocks[cn.block];
            let (fl, n) = (b[block::FIRST_LINE] as usize, b[block::LINE_COUNT] as usize);
            (fl, (fl + n).wrapping_sub(1))
        }).collect();
        let owner = innermost_per_line(line_count, &[], &spans);
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
            let buffer = LineBuffer::new(self.src, &self.li, &lines);
            let defs = reference_definitions::paragraph_definitions(&buffer.text);
            if defs.is_empty() {
                self.dev("uncovered-lines", || format!("lines {line0}..{l} belong to no block and are not definitions: {:?}", buffer.text));
            } else {
                let checkbox = scope.and_then(|i| self.containers[i].c.checkbox);
                self.record_buffer_definitions(&lines, &buffer, &defs, true, checkbox);
            }
            line0 = l;
        }
    }

    fn children_span<'b>(&mut self, node: &'b AstNode<'b>, cell: Option<Cell>, shift: Option<&Shift>) -> (usize, usize) {
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

    /// The end of an inline literal that crosses lines and starts at `s`.
    /// comrak's end column adds the prefix width of the paragraph line
    /// numbered by the lines the literal crosses, not of the line it ends on
    /// (`adjust_node_newlines` counts from the node's line, `parse_inline`
    /// from the paragraph's). The literal's last line instead lies at the
    /// content start of the line it ends on, after that line's virtual
    /// spaces. `None` when the source does not hold the literal there.
    fn crossing_literal_end(&self, s: usize, literal: &str, content_from: usize, shift: Option<&Shift>) -> Option<usize> {
        let (first, last) = (literal.find('\n')?, literal.rfind('\n')?);
        let (bytes, lit) = (self.src.as_bytes(), literal.as_bytes());
        if bytes.get(s..s + first + 1)? != &lit[..first + 1] { return None; }
        let line = self.li.line_of(s) + lit.iter().filter(|&&b| b == b'\n').count();
        // A leaf's content records and buffered lines are in line order: each
        // tag of a paragraph of them scanned the leaf's lines for its own.
        let records = &self.content[content_from..];
        let start = records.get(records.partition_point(|r| (r[content::LINE] as usize) < line)).filter(|r| r[content::LINE] as usize == line)?[content::START_BYTE] as usize;
        let virt = shift.and_then(|sh| sh.lines.get(sh.lines.partition_point(|l| l.line0 < line)).filter(|l| l.line0 == line)).map_or(0, |l| l.virt);
        let tail = &lit[last + 1..];
        if tail.len() < virt || tail[..virt].iter().any(|&b| b != b' ') { return None; }
        let end = start + tail.len() - virt;
        (bytes.get(start..end)? == &tail[virt..]).then_some(end)
    }

    fn pipe_shift(&mut self, byte: usize, cell: Option<Cell>) -> usize {
        let Some(Cell { start: cell_start, .. }) = cell else { return byte; };
        // comrak's columns restart on each line: in a paragraph split to make
        // a table header, only the escapes earlier on the byte's own line move it.
        let from = cell_start.max(self.li.line_start(self.li.line_of(byte.min(self.src.len()))));
        let mut b = byte; let mut k0 = 0usize;
        loop {
            // A backslash at b-1 whose pipe sits at b was removed too: count through b+1.
            let k = self.pipes.removed(self.src.as_bytes(), cell_start, from, b + 1);
            if k == k0 { break; }
            k0 = k; b = byte + k;
        }
        b
    }

    fn corrected_range(&mut self, sp: Sourcepos, cell: Option<Cell>, shift: Option<&Shift>) -> Option<(usize, usize)> {
        if sp.start.line == 0 || sp.end.line == 0 { return None; }
        let sp = shift_columns(sp, cell.map_or(0, |c| c.column_delta));
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
    fn walk_inlines<'b>(&mut self, leaf: &'b AstNode<'b>, blk: u32, cell: Option<Cell>, shift: Option<&Shift>) {
        let content_from = self.blocks[blk as usize][block::CONTENT_OFFSET] as usize;
        self.run_delta = 0;
        self.bare_cr_leaf = None;
        // Allow a small backward window: comrak can also place a run too far right.
        self.last_text_end = (self.blocks[blk as usize][block::START_BYTE] as usize).saturating_sub(8);
        let first_run = self.runs.len();
        // Slot zero is the leaf's root children; every emitted run gets a
        // child slot. A first child has no preceding sibling, so searching
        // backward through all earlier runs for it is quadratic in flat markup.
        let mut previous_siblings: Vec<Option<usize>> = vec![None];
        let mut stack: Vec<(&'b AstNode<'b>, u32)> = leaf.children().collect::<Vec<_>>().into_iter().rev().map(|n| (n, u32::MAX)).collect();
        while let Some((node, parent)) = stack.pop() {
            let slot = if parent == u32::MAX { 0 } else { parent as usize - first_run + 1 };
            let sibling_end = previous_siblings[slot].map_or(0, |i| self.runs[i][run::END_BYTE] as usize);
            if let Some(ri) = self.inline_record(node, blk, parent, cell, shift, content_from, sibling_end) {
                previous_siblings.resize(self.runs.len() - first_run + 1, None);
                previous_siblings[slot] = Some(self.runs.len() - 1);
                let children: Vec<_> = node.children().collect();
                for ch in children.into_iter().rev() { stack.push((ch, ri)); }
            }
        }
        if self.run_delta != 0 { self.refit_containers(first_run); }
        self.check_delimiters(blk, first_run);
    }

    /// Whether leaf [blk] holds a bare CR, which excuses a text literal that
    /// nothing else explains. Scanned at most once per leaf and only when
    /// asked: scanning the leaf for every text whose literal differs from its
    /// source, every text with an entity, made a paragraph of them quadratic.
    fn leaf_has_bare_cr(&mut self, blk: u32) -> bool {
        let (bs, be) = (self.blocks[blk as usize][block::START_BYTE] as usize, self.blocks[blk as usize][block::END_BYTE] as usize);
        let src = self.src;
        *self.bare_cr_leaf.get_or_insert_with(|| has_bare_cr(src.as_bytes(), bs..be))
    }

    /// The literal of an inline node comrak placed at [s]..[e] where it does
    /// not appear: found from the end of the previous text and sibling, close
    /// to where comrak put it, or `None`.
    fn find_literal(&self, literal: &str, s: usize, e: usize, sibling_end: usize, blk: u32) -> Option<usize> {
        let src = self.src;
        let block_end = self.blocks[blk as usize][block::END_BYTE] as usize;
        let mut from = self.last_text_end.max(sibling_end).max(s.saturating_sub(4)).min(src.len());
        while from > 0 && !src.is_char_boundary(from) { from -= 1; }
        // A match is taken only when it starts within 8 + (e - s) bytes, so
        // the search stops there: searched to the block's end, a paragraph
        // of distinct misplaced tags was quadratic in its length.
        let reach = from + 8 + (e - s) + literal.len();
        let mut to = block_end.max(from).min(reach).min(src.len());
        while to < src.len() && !src.is_char_boundary(to) { to += 1; }
        match src[from..to].find(literal) {
            Some(off) if off <= 8 + (e - s) => Some(from + off),
            _ => None,
        }
    }

    /// The nearest source window, from the end of the previous text and
    /// sibling and close to where comrak put [s], whose pieces explain the
    /// text literal [t] (entities decoded), or `None`. Only short literals are
    /// searched, and only windows whose pieces could begin and end them: an
    /// explained window begins with an exact piece, an entity, an escaped
    /// pipe's backslash, a CR or virtual spaces before the literal's spaces,
    /// and ends with an exact piece, an entity or a CR. Every start and length
    /// with a piece walk each was cubic in the cell or line, seconds for a
    /// short table of entities before escaped pipes; the walks are also
    /// capped, after which the text is left where comrak put it. The ends a
    /// window can have, and the references a window could cut, are found once
    /// for the whole search: testing every end of every start, and scanning
    /// back for references at each, cost each text of a leaf comrak misplaced
    /// up to tens of microseconds.
    fn find_explained(&self, t: &str, s: usize, e: usize, sibling_end: usize, blk: u32, pipes: bool) -> Option<(usize, usize)> {
        const MAX_WALKS: usize = 256;
        let (Some(&first), Some(&last)) = (t.as_bytes().first(), t.as_bytes().last()) else { return None };
        if t.len() > 64 { return None; }
        let (src, bytes) = (self.src, self.src.as_bytes());
        let block_end = self.blocks[blk as usize][block::END_BYTE] as usize;
        let from = self.last_text_end.max(sibling_end).max(s.saturating_sub(4)).min(src.len());
        let to = block_end.max(from).min(src.len());
        let last_start = (from + 8 + (e - s)).min(to);
        let reach = (last_start + t.len() * 12 + 8).min(to);
        let ends: Vec<usize> = (from + 1..=reach).filter(|&r| matches!(bytes[r - 1], b if b == last || matches!(b, b';' | b'\r')) && src.is_char_boundary(r)).collect();
        let cover = text_pieces::EntityCover::new(src, from, reach);
        let mut walks = 0;
        for q in from..=last_start {
            if !src.is_char_boundary(q) || (first != b' ' && !matches!(bytes.get(q), Some(&b) if b == first || matches!(b, b'&' | b'\\' | b'\r'))) { continue; }
            let longest = (t.len() * 12 + 8).min(to - q);
            // The windows from q that could explain the literal, read once:
            // only they are worth a piece walk, though every window counts.
            let mut readable: Option<Option<Vec<usize>>> = None;
            for &r in ends[ends.partition_point(|&r| r <= q)..].iter().take_while(|&&r| r <= q + longest) {
                if cover.cuts(src, q, r) { continue; }
                if walks == MAX_WALKS { return None; }
                walks += 1;
                let readable = readable.get_or_insert_with(|| text_pieces::read_ends(src, q, q + longest, t, &self.entities));
                if readable.as_ref().is_none_or(|ends| ends.contains(&r)) && text_pieces::explains(&src[q..r], t, &self.entities, pipes) { return Some((q, r)); }
            }
        }
        None
    }

    /// Whether [s]..[e] is bracketed by the delimiters of a run of [kind].
    fn delimited(bytes: &[u8], kind: u32, s: usize, e: usize) -> bool {
        if e > bytes.len() || s >= e { return false; }
        match kind {
            run_kind::EMPH => e > s + 1 && matches!(bytes[s], b'*' | b'_') && bytes[e - 1] == bytes[s],
            run_kind::STRONG => e >= s + 4 && matches!(bytes[s], b'*' | b'_') && bytes[s + 1] == bytes[s] && bytes[e - 1] == bytes[s] && bytes[e - 2] == bytes[s],
            run_kind::STRIKE => { let n = if e >= s + 4 && bytes[s] == b'~' && bytes[s + 1] == b'~' { 2 } else { 1 }; e >= s + 2 * n && bytes[s] == b'~' && bytes[e - 1] == b'~' }
            _ => true,
        }
    }

    /// Emphasis, strong and strikethrough runs must be bracketed by their own
    /// delimiters. One that is not, because comrak's end drifted inside a
    /// multiline link the container encloses, is re-derived around its
    /// children; only one that still is not counts as a deviation. An escape
    /// is exactly a backslash and the character its child shows: after a
    /// drift the child text of `\\` matches the escaping backslash itself,
    /// which leaves the escaped one in no run. A link or image opens with its
    /// bracket, closes its text with `]` and ends with `)` or `]`, and an
    /// autolink holds its text: a drifted link that met none of these was
    /// published as an autolink over `]()`, or ended inside `[][a]`. A
    /// footnote reference is `[^` through `]`: a one-character text found by
    /// literal search can match inside one.
    fn check_delimiters(&mut self, blk: u32, first_run: usize) {
        let bytes = self.src.as_bytes();
        let spans = self.child_spans(first_run);
        for i in first_run..self.runs.len() {
            let kind = self.runs[i][run::KIND];
            let (s, e) = (self.runs[i][run::START_BYTE] as usize, self.runs[i][run::END_BYTE] as usize);
            if kind == run_kind::ESCAPE {
                let (cs, ce) = spans[i - first_run];
                let escapes = |a: usize, z: usize| bytes.get(a) == Some(&b'\\') && z == a + 2 && (cs == usize::MAX || (cs, ce) == (a + 1, z));
                if escapes(s, e) { continue; }
                if cs != usize::MAX && cs >= 1 && escapes(cs - 1, ce) {
                    let r = &mut self.runs[i];
                    r[run::START_BYTE] = (cs - 1) as u32; r[run::END_BYTE] = ce as u32; r[run::CONTENT_START_BYTE] = cs as u32; r[run::CONTENT_END_BYTE] = ce as u32;
                    r[run::START_UTF16] = self.li.u16(cs - 1); r[run::END_UTF16] = self.li.u16(ce); r[run::CONTENT_START_UTF16] = self.li.u16(cs); r[run::CONTENT_END_UTF16] = self.li.u16(ce);
                } else {
                    let shown = self.src.get(s..e.min(self.src.len())).unwrap_or("").to_string();
                    self.dev("escape-delims", || format!("block {blk} {:?}", shown));
                }
                continue;
            }
            if kind == run_kind::FOOTNOTE_REF {
                if !(bytes[s.min(bytes.len())..].starts_with(b"[^") && e > s + 3 && bytes.get(e - 1) == Some(&b']')) {
                    let shown = self.src.get(s..e.min(self.src.len())).unwrap_or("").to_string();
                    self.dev("footnote-ref-delims", || format!("block {blk} {:?}", shown));
                }
                continue;
            }
            if matches!(kind, run_kind::LINK | run_kind::IMAGE | run_kind::AUTOLINK) {
                let (cs, ce) = spans[i - first_run];
                let ok = if kind == run_kind::AUTOLINK { cs != usize::MAX } else {
                    let open: &[u8] = if kind == run_kind::IMAGE { b"![" } else { b"[" };
                    let framed = bytes[s.min(bytes.len())..].starts_with(open) && matches!(e.checked_sub(1).and_then(|z| bytes.get(z)), Some(b')' | b']'));
                    let text_end = self.runs[i][run::CONTENT_END_BYTE] as usize;
                    // The text was taken from comrak's child positions before
                    // the children's own repairs (a destination continued on
                    // the next line ends a line early): it follows the runs.
                    if framed && bytes.get(text_end) != Some(&b']') && cs != usize::MAX && cs >= s + open.len() && bytes.get(ce) == Some(&b']') {
                        let r = &mut self.runs[i];
                        r[run::CONTENT_START_BYTE] = cs as u32; r[run::CONTENT_END_BYTE] = ce as u32;
                        r[run::CONTENT_START_UTF16] = self.li.u16(cs); r[run::CONTENT_END_UTF16] = self.li.u16(ce);
                    }
                    framed && bytes.get(self.runs[i][run::CONTENT_END_BYTE] as usize) == Some(&b']')
                };
                if !ok {
                    let shown = self.src.get(s..e.min(self.src.len())).unwrap_or("").to_string();
                    self.dev(if kind == run_kind::IMAGE { "image-delims" } else { "link-delims" }, || format!("block {blk} {:?}", shown));
                }
                continue;
            }
            if !matches!(kind, run_kind::EMPH | run_kind::STRONG | run_kind::STRIKE) { continue; }
            if Self::delimited(bytes, kind, s, e) { continue; }
            let (cs, ce) = spans[i - first_run];
            let n = match kind {
                run_kind::EMPH => 1,
                run_kind::STRONG => 2,
                _ => if cs != usize::MAX && cs >= 2 && bytes[cs - 2] == b'~' && bytes.get(ce + 1) == Some(&b'~') { 2 } else { 1 },
            };
            if cs != usize::MAX && cs >= n && Self::delimited(bytes, kind, cs - n, ce + n) {
                let (ns, ne) = (cs - n, ce + n);
                let r = &mut self.runs[i];
                r[run::START_BYTE] = ns as u32; r[run::END_BYTE] = ne as u32; r[run::CONTENT_START_BYTE] = cs as u32; r[run::CONTENT_END_BYTE] = ce as u32;
                r[run::START_UTF16] = self.li.u16(ns); r[run::END_UTF16] = self.li.u16(ne); r[run::CONTENT_START_UTF16] = self.li.u16(cs); r[run::CONTENT_END_UTF16] = self.li.u16(ce);
            } else {
                let rule = match kind { run_kind::EMPH => "emph-delims", run_kind::STRONG => "strong-delims", _ => "strike-delims" };
                let shown = self.src.get(s..e.min(self.src.len())).unwrap_or("").to_string();
                self.dev(rule, || format!("block {blk} {:?}", shown));
            }
        }
    }

    /// The source span of each run's children, from [first_run]: `(usize::MAX,
    /// 0)` for a run without any. Runs are re-derived parents first and a
    /// child always follows its parent, so spans taken before any is
    /// re-derived are the ones each parent would see; scanning every later
    /// run for each parent was quadratic in a leaf's runs.
    fn child_spans(&self, first_run: usize) -> Vec<(usize, usize)> {
        let mut spans = vec![(usize::MAX, 0usize); self.runs.len() - first_run];
        for r in &self.runs[first_run..] {
            let p = r[run::PARENT];
            if p == u32::MAX || (p as usize) < first_run { continue; }
            let span = &mut spans[p as usize - first_run];
            span.0 = span.0.min(r[run::START_BYTE] as usize); span.1 = span.1.max(r[run::END_BYTE] as usize);
        }
        spans
    }

    /// After a repair shifted positions mid-leaf, containers recorded before
    /// the shift was known are re-derived from their children: delimiters
    /// around the children span for emphasis kinds, bracket syntax for links.
    fn refit_containers(&mut self, first_run: usize) {
        let bytes = self.src.as_bytes();
        let spans = self.child_spans(first_run);
        for i in first_run..self.runs.len() {
            let kind = self.runs[i][run::KIND];
            let (cs, ce) = spans[i - first_run];
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

    fn inline_record<'b>(&mut self, node: &'b AstNode<'b>, blk: u32, parent: u32, cell: Option<Cell>, shift: Option<&Shift>, content_from: usize, sibling_end: usize) -> Option<u32> {
        let data = node.data.borrow();
        let sp = data.sourcepos;
        let (mut s, mut e) = self.corrected_range(sp, cell, shift)?;
        if matches!(data.value, NodeValue::SoftBreak | NodeValue::LineBreak) && !self.src[s..e].contains(['\n', '\r']) {
            // comrak can leave a break's sourcepos inside a multiline link's
            // title. Its preceding sibling owns the true closing boundary.
            if sibling_end > 0 {
                let end = sibling_end;
                let line = self.li.line_of(end);
                let line_end = self.li.line_end(line, self.src.len());
                // A drifted sibling can end between the CR and LF of a CRLF,
                // past the line's content end: nothing to repair from there.
                if end >= s && self.src.as_bytes().get(end..line_end).is_some_and(|rest| rest.iter().all(|b| matches!(b, b' ' | b'\t'))) {
                    s = end;
                    e = self.li.line_end_with_break(line, self.src.len());
                }
            }
        }
        let src = self.src; let bytes = src.as_bytes();
        let slice = &src[s..e];
        let mut rec: RunRec = [0; run::WORDS];
        rec[run::BLOCK] = blk; rec[run::PARENT] = parent;
        let (kind, cs, ce): (u32, usize, usize) = match &data.value {
            NodeValue::Text(t) => {
                let t: &str = &**t;
                // A repeated literal can match the closing syntax of a preceding
                // multiline link even though comrak positioned it too early.
                // Siblings cannot overlap: authenticate relocation after the
                // preceding sibling's complete range, not only its text children.
                // Repair: comrak's inline line counter does not advance across a bare
                // CR, so positions after one are short by the line-ending bytes.
                // Find the literal forward of the last text run and carry the offset.
                // A slice with an entity, tab or escaped pipe can differ from its
                // literal legitimately, so it is searched for only when the piece
                // walk cannot explain the literal from it: then comrak placed it
                // (after a multiline link's syntax, say) where it is not.
                let pipes = cell.is_some();
                let replacement_like = slice.contains('&') || slice.contains('\t') || slice.contains("\\|") || (t.len() > slice.len() && t.trim_start_matches(' ') == slice);
                let drifted = !replacement_like || !text_pieces::explains(slice, t, &self.entities, pipes);
                let (s, e, slice) = if (t != slice || s < sibling_end) && !t.is_empty() && t != "\n" && drifted {
                    let block_end = self.blocks[blk as usize][block::END_BYTE] as usize;
                    let mut from = self.last_text_end.max(sibling_end).max(s.saturating_sub(4)).min(src.len());
                    while from > 0 && !src.is_char_boundary(from) { from -= 1; }
                    // Only a match starting within `8 + (e - s)` of `from` is taken,
                    // so nothing past one that could is searched: scanning on to the
                    // block's end cost a whole block per drifted text.
                    let mut to = block_end.max(from).min(src.len()).min(from + 8 + (e - s) + t.len());
                    while to < src.len() && !src.is_char_boundary(to) { to += 1; }
                    // A literal ending in `&` also matches the first byte of `&amp;`;
                    // comrak never ends a text node inside a reference, so such a
                    // match is the wrong text, unless the reference there decodes to
                    // the literal: the entity span below then takes the reference.
                    let matches_text = |q: usize| !text_pieces::cuts_entity(src, q, q + t.len()) || text_pieces::entity_len(&src[q..]).is_some_and(|l| self.entities.decode(&src[q..q + l]) == t);
                    match src[from..to].match_indices(t).map(|(off, _)| off).find(|&off| matches_text(from + off)) {
                        Some(off) if off <= 8 + (e - s) => { let q = from + off; self.run_delta += q as isize - s as isize; (q, q + t.len(), &src[q..q + t.len()]) }
                        // A literal with a decoded entity, or a cell's escaped pipe,
                        // is not in the source as written: take the nearest window
                        // its pieces explain.
                        _ => match self.find_explained(t, s, e, sibling_end, blk, pipes) {
                            Some((q, r)) => { self.run_delta += q as isize - s as isize; (q, r, &src[q..r]) }
                            None => (s, e, slice),
                        },
                    }
                } else { (s, e, slice) };
                // A drifted range can cover just the first bytes of an entity that
                // decode to the literal (`&` of `&amp;`): the text is the entity.
                // An escaped ampersand (`\&amp;`) is not one.
                let escaped = (s > 0 && bytes[s - 1] == b'\\') || (parent != u32::MAX && self.runs[parent as usize][run::KIND] == run_kind::ESCAPE);
                let (s, e, slice) = match (t == slice && !escaped).then(|| text_pieces::entity_len(&src[s..])).flatten() {
                    Some(l) if s + l > e && text_pieces::explains(&src[s..s + l], t, &self.entities, pipes) => (s, s + l, &src[s..s + l]),
                    _ => (s, e, slice),
                };
                if t == slice { (run_kind::TEXT, s, e) } else {
                    // Virtual spaces of a partially consumed tab exist in comrak's buffer
                    // but not in the source; the literal then carries up to three
                    // leading spaces the slice cannot, so it displays as a replacement.
                    let virtual_spaces = t.len() > slice.len() && t.len() - slice.len() <= 3 && t.trim_start_matches(' ') == slice;
                    // comrak also unescapes pipes in a paragraph it examined as a table
                    // header candidate, not only inside cells.
                    let unescaped_pipes = slice.contains("\\|") && slice.replace("\\|", "|") == t;
                    // Containing an entity or a tab explains nothing by itself: a slice
                    // explains the literal only piece by piece, or a drifted range
                    // would publish a replacement over text it does not hold.
                    // Known limit: after a bare CR, text that also carries an entity
                    // cannot be relocated by literal search (the literal is decoded),
                    // so it keeps comrak's shifted range with the literal as display.
                    // A CRLF line ending is no reason: it exempted every multi-line
                    // CRLF paragraph from this check. The leaf is scanned for a bare
                    // CR last, and once (see [Extractor::leaf_has_bare_cr]).
                    let explained = text_pieces::explains(slice, t, &self.entities, pipes) || unescaped_pipes || virtual_spaces || self.leaf_has_bare_cr(blk);
                    if !explained { let (sl, lit) = (slice.to_string(), t.to_string()); self.dev("text-mismatch", || format!("block {blk} {:?} vs literal {:?}", sl, lit)); }
                    // Keep exact ranges around the bytes that differ (an entity, an
                    // escaped pipe, a CR, virtual spaces). The pieces are validated to
                    // rebuild the literal; otherwise the node stays one replacement run.
                    match text_pieces::split_pieces(slice, t, &self.entities, pipes) {
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
            // Delimiters are checked once the leaf's repairs are done
            // (check_delimiters): comrak's end can drift inside a multiline
            // link's syntax that the container encloses.
            NodeValue::Emph => (run_kind::EMPH, s + 1, e.saturating_sub(1).max(s + 1)),
            NodeValue::Strong => (run_kind::STRONG, s + 2, e.saturating_sub(2).max(s + 2)),
            NodeValue::Strikethrough => { let n = if e >= s + 4 && bytes[s] == b'~' && bytes[s + 1] == b'~' { 2 } else { 1 }; (run_kind::STRIKE, s + n, e.saturating_sub(n).max(s + n)) }
            NodeValue::Code(c) => {
                let n = c.num_backticks.max(1);
                rec[run::AUX0] = n as u32;
                // comrak's end column drifts when a span crosses CR line endings,
                // and a span crossing lines can take another line's prefix width
                // (see crossing_literal_end), which can land just after an
                // unrelated backtick run. The closing run is the next backtick
                // run of exactly n after the opener (CommonMark), validated below
                // against the literal.
                let crosses = sp.start.line != sp.end.line;
                // Placed away from any opener (after a multiline link's syntax):
                // the span starts at the nearest run of exactly n backticks.
                let backticks = |a: usize| a + n <= bytes.len() && bytes[a..a + n].iter().all(|b| *b == b'`');
                let opener = |a: usize| backticks(a) && (a == 0 || bytes[a - 1] != b'`') && bytes.get(a + n) != Some(&b'`');
                if !backticks(s) {
                    let from = self.last_text_end.max(sibling_end).max(s.saturating_sub(4));
                    if let Some(q) = (from..(from + 8 + (e - s)).min(bytes.len())).find(|&a| opener(a)) {
                        self.run_delta += q as isize - s as isize;
                        e = (q + (e - s)).min(bytes.len()); s = q;
                        while e < bytes.len() && !src.is_char_boundary(e) { e += 1; }
                    }
                }
                if s + n <= bytes.len() && bytes[s..s + n].iter().all(|b| *b == b'`') && (crosses || !(e >= s + 2 * n && e <= bytes.len() && bytes[e - n..e].iter().all(|b| *b == b'`') && bytes.get(e) != Some(&b'`'))) {
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
                            // Only the records the span overlaps, which follow
                            // each other: every span of a paragraph of them
                            // walked all of its lines.
                            let records = &self.content[content_from..];
                            let first = records.partition_point(|r| (r[content::END_BYTE] as usize) <= cs);
                            let mut parts = Vec::new();
                            for rec in records[first..].iter().take_while(|r| (r[content::START_BYTE] as usize) < ce) {
                                let (a, b) = ((rec[content::START_BYTE] as usize).max(cs), (rec[content::END_BYTE] as usize).min(ce));
                                if a < b { parts.push(&src[a..b]); }
                            }
                            parts.join(" ")
                        } else { raw.to_string() };
                        let norm = |t: &str| if multiline { t.split_whitespace().collect::<Vec<_>>().join(" ") } else { t.to_string() };
                        let raw = buffered.as_str();
                        let (nraw, nlit) = (norm(raw), norm(&c.literal));
                        // comrak unescapes the pipes of a cell, and of a paragraph split
                        // to make a table header, before it parses the span. CommonMark
                        // then strips one space from each side unless the content is all
                        // spaces; a tab is not a space there. Stripped from the escaped
                        // text instead, `` ` \| ` `` in a cell matched neither.
                        let unescaped = if cell.is_some() { text_pieces::unescape_pipes(&nraw) } else { nraw.clone() };
                        let strip = usize::from(unescaped.len() >= 2 && unescaped.starts_with(' ') && unescaped.ends_with(' ') && !unescaped.trim_matches(' ').is_empty());
                        if unescaped[strip..unescaped.len() - strip] != nlit {
                            let (r, l) = (raw.to_string(), c.literal.clone());
                            self.dev("code-literal", || format!("block {blk} {:?} vs {:?}", r, l));
                        } else {
                            if strip == 1 { cs += 1; ce -= 1; }
                            if unescaped != nraw {
                                // The literal lost an escaped pipe's backslash, so the span
                                // displays its literal. A host places display text within
                                // one line, as in a cell: a span that crosses lines, in a
                                // paragraph split to make a table header, showed its
                                // backslash, and its leaf shows its source instead.
                                if multiline {
                                    let (r, l) = (raw.to_string(), c.literal.clone());
                                    self.dev("code-literal", || format!("block {blk} {:?} displays {:?} across lines", r, l));
                                } else {
                                    let (off, len) = self.push_string(&c.literal);
                                    rec[run::AUX2] = off; rec[run::AUX3] = len; rec[run::FLAGS] |= 2;
                                }
                            }
                        }
                    }
                    (run_kind::CODE, cs, ce)
                }
            }
            NodeValue::Link(link) => {
                self.resolved_resource(&mut rec, &link.url, &link.title);
                if s < e && bytes[s] == b'[' {
                    let (cs, ce) = { let (a, b) = self.children_span(node, cell, shift); if a == 0 && b == 0 { (s + 1, s + 1) } else { (a, b) } };
                    self.link_aux(&mut rec, ce, e);
                    if bytes.get(e.saturating_sub(1)) != Some(&b')') {
                        e = self.repair_inline_link(&mut rec, ce, blk).unwrap_or(e);
                    }
                    (run_kind::LINK, cs, ce)
                } else {
                    let bracketed = s < e && bytes[s] == b'<' && bytes[e - 1] == b'>';
                    let (cs, ce) = if bracketed { (s + 1, e - 1) } else { (s, e) };
                    rec[run::AUX0] = cs as u32; rec[run::AUX1] = ce as u32;
                    (run_kind::AUTOLINK, cs, ce)
                }
            }
            NodeValue::Image(link) => {
                self.resolved_resource(&mut rec, &link.url, &link.title);
                if s < e && bytes[s] == b'!' {
                    let (cs, ce) = { let (a, b) = self.children_span(node, cell, shift); if a == 0 && b == 0 { (s + 2, s + 2) } else { (a, b) } };
                    self.link_aux(&mut rec, ce, e);
                    if bytes.get(e.saturating_sub(1)) != Some(&b')') {
                        e = self.repair_inline_link(&mut rec, ce, blk).unwrap_or(e);
                    }
                    (run_kind::IMAGE, cs, ce)
                } else { let sl = slice.to_string(); self.dev("image-delims", || format!("block {blk} {:?}", sl)); (run_kind::IMAGE, s, e) }
            }
            NodeValue::SoftBreak => (run_kind::SOFT_BREAK, e, e),
            NodeValue::LineBreak => {
                let line_end = self.li.line_end(self.li.line_of(s), self.src.len()).min(e);
                // Spaces before a hard break stay editable. The full run
                // still owns the marker and newline for atomic break deletion.
                if line_end > s && self.src.as_bytes()[s..line_end].iter().all(|b| matches!(b, b' ' | b'\t')) {
                    (run_kind::HARD_BREAK, s, line_end)
                } else { (run_kind::HARD_BREAK, e, e) }
            }
            NodeValue::HtmlInline(literal) => {
                if literal.contains('\n') {
                    match self.crossing_literal_end(s, literal, content_from, shift) {
                        Some(end) => e = end,
                        None => { let lit = literal.clone(); self.dev("html-inline-end", || format!("block {blk} {:?} at {s}", lit)); }
                    }
                } else if cell.is_none() && slice != literal.as_str() {
                    // Placed where it is not, like text after a multiline link's
                    // syntax: the tag is its own literal, so find it nearby.
                    match self.find_literal(literal, s, e, sibling_end, blk) {
                        Some(q) => { self.run_delta += q as isize - s as isize; s = q; e = q + literal.len(); }
                        None => {
                            let (sl, lit) = (slice.to_string(), literal.clone());
                            self.dev("html-inline-literal", || format!("block {blk} {:?} vs literal {:?}", sl, lit));
                        }
                    }
                }
                (run_kind::HTML_INLINE, s, e)
            }
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
    /// Complete an AST-authenticated inline link whose sourcepos ends before
    /// its closing parenthesis (notably a title on another physical line).
    fn repair_inline_link(&mut self, rec: &mut RunRec, ce: usize, blk: u32) -> Option<usize> {
        if self.src.as_bytes().get(ce..ce + 2)? != b"](" { return None; }
        // Use the already derived content records so quote/list prefixes do
        // not become part of the link grammar. This rare sourcepos repair is
        // bounded by the current leaf, and maps scanner offsets back to source.
        // The leaf's text is buffered once: rebuilding it from each link on
        // made a paragraph of such links quadratic.
        let from = self.blocks[blk as usize][block::CONTENT_OFFSET] as usize;
        if self.leaf_text.key != (from, self.content.len()) { self.leaf_text = LeafText::new(self.src, &self.content, from); }
        let text = &self.leaf_text;
        // The buffer from ce: the first line ending at or after it, from ce.
        let records = &self.content[from..];
        let k = records.partition_point(|r| (r[content::END_BYTE] as usize) < ce);
        let base = records.get(k).map_or(text.bytes.len(), |r| text.line_starts[k] + ce.saturating_sub(r[content::START_BYTE] as usize));
        let (bytes, source_offsets) = (&text.bytes[base..], &text.offsets[base..]);
        let mut p = 2;
        while bytes.get(p).is_some_and(|b| reference_definitions::spacechar(*b)) { p += 1; }
        let (ds, de, consumed) = reference_definitions::scan_link_url(bytes.get(p..)?)?;
        let destination = (*source_offsets.get(p + ds)?, *source_offsets.get(p + de)?);
        p += consumed;
        while bytes.get(p).is_some_and(|b| reference_definitions::spacechar(*b)) { p += 1; }
        let mut title = None;
        if bytes.get(p) != Some(&b')') {
            let n = reference_definitions::scan_link_title(bytes.get(p..)?)?;
            title = Some((*source_offsets.get(p + 1)?, *source_offsets.get(p + n - 1)?));
            p += n;
            while bytes.get(p).is_some_and(|b| reference_definitions::spacechar(*b)) { p += 1; }
        }
        if bytes.get(p) != Some(&b')') { return None; }
        rec[run::AUX0] = destination.0 as u32; rec[run::AUX1] = destination.1 as u32;
        rec[run::FLAGS] &= !3;
        if let Some((s, e)) = title {
            rec[run::AUX2] = s as u32; rec[run::AUX3] = e as u32; rec[run::FLAGS] |= 2;
        }
        Some(source_offsets[p] + 1)
    }

    fn resolved_resource(&mut self, rec: &mut RunRec, destination: &str, title: &str) {
        let (offset, length) = self.push_string(destination);
        rec[run::DESTINATION_OFFSET] = offset; rec[run::DESTINATION_LENGTH] = length;
        let (offset, length) = self.push_string(title);
        rec[run::TITLE_OFFSET] = offset; rec[run::TITLE_LENGTH] = length;
    }

    fn link_aux(&self, rec: &mut RunRec, ce: usize, e: usize) {
        let bytes = self.src.as_bytes();
        let mut p = ce;
        if p < e && bytes[p] == b']' { p += 1; }
        if p < e && bytes[p] == b'(' {
            p += 1;
            while p < e && reference_definitions::spacechar(bytes[p]) { p += 1; }
            if let Some((ds, de, consumed)) = reference_definitions::scan_link_url(&bytes[p..e]) {
                rec[run::AUX0] = (p + ds) as u32; rec[run::AUX1] = (p + de) as u32;
                p += consumed;
            }
            while p < e && reference_definitions::spacechar(bytes[p]) { p += 1; }
            if p < e { if let Some(n) = reference_definitions::scan_link_title(&bytes[p..e]) { if n >= 2 { rec[run::AUX2] = (p + 1) as u32; rec[run::AUX3] = (p + n - 1) as u32; rec[run::FLAGS] |= 2; } } }
        } else {
            rec[run::FLAGS] |= 1;
            if p < e && bytes[p] == b'[' { let ls = p + 1; let mut q = ls; while q < e && bytes[q] != b']' { q += 1; } rec[run::AUX0] = ls as u32; rec[run::AUX1] = q as u32; }
        }
    }

    // ---------------------------------------------------------------- encode

    /// Pack the internal records into the published model: UTF-16 offsets
    /// only, fixed-width records, and one extras section for values only some
    /// kinds carry. Content records are emitted in block order, so a block's
    /// records end where the next block's begin.
    fn encode(&mut self) -> Vec<u32> {
        use schema::{block as wb, content as wc, extra, header as wh, run as wr};
        let u16 = |li: &LineIndex, b: u32| li.u16(b as usize);
        let (nb, nr) = (self.blocks.len(), self.runs.len());
        // Block b owns runs [first_run[b], first_run[b + 1]).
        let mut first_run = vec![0u32; nb + 1];
        let mut r = 0usize;
        for (b, slot) in first_run.iter_mut().enumerate() {
            while r < nr && (self.runs[r][run::BLOCK] as usize) < b { r += 1; }
            *slot = r as u32;
        }
        let mut extras: Vec<u32> = Vec::new();
        let mut blocks: Vec<u32> = Vec::with_capacity(nb * wb::WORDS);
        let mut content: Vec<u32> = Vec::with_capacity(self.content.len() * wc::WORDS);
        let mut packing_overflow = None;
        for (b, rec) in self.blocks.iter().enumerate() {
            let kind = rec[block::KIND];
            let mut flags = rec[block::FLAGS];
            let mut attr = rec[block::ATTR0];
            let mut extra = extra::NONE;
            match kind {
                block_kind::CODE_BLOCK if flags & 1 != 0 => {
                    extra = extras.len() as u32;
                    extras.extend([u16(&self.li, rec[block::ATTR1]), u16(&self.li, rec[block::ATTR2])]);
                }
                block_kind::LIST => { flags |= (rec[block::ATTR0] & 1) << 1; attr = rec[block::ATTR1]; }
                block_kind::ITEM => {
                    extra = extras.len() as u32;
                    extras.extend([rec[block::MARKER_END_UTF16], u16(&self.li, rec[block::ATTR1]), u16(&self.li, rec[block::ATTR2])]);
                }
                block_kind::TABLE => { extra = extras.len() as u32; extras.push(rec[block::ATTR1]); }
                block_kind::FOOTNOTE_DEFINITION => {
                    extra = extras.len() as u32;
                    extras.extend([u16(&self.li, rec[block::ATTR1]), u16(&self.li, rec[block::ATTR2])]);
                }
                _ => {}
            }
            blocks.extend([
                kind | flags << wb::kind_flags::FLAGS_SHIFT,
                rec[block::PARENT], rec[block::START_UTF16], rec[block::END_UTF16],
                rec[block::FIRST_LINE], rec[block::LINE_COUNT],
                (content.len() / wc::WORDS) as u32, first_run[b], attr, extra,
            ]);
            let (co, cn) = (rec[block::CONTENT_OFFSET] as usize, rec[block::CONTENT_COUNT] as usize);
            for c in &self.content[co..co + cn] {
                let (line, virt) = (c[content::LINE], c[content::VIRTUAL_LEADING_SPACES]);
                if line > wc::line_virtual::LINE_MASK || virt > wc::line_virtual::VIRTUAL_LEADING_SPACES_MASK {
                    packing_overflow = Some(format!("content line {line} with {virt} virtual spaces"));
                }
                content.extend([c[content::START_UTF16], c[content::END_UTF16], c[content::PREFIX_START_UTF16], line | virt << wc::line_virtual::VIRTUAL_LEADING_SPACES_SHIFT]);
            }
        }
        if let Some(detail) = packing_overflow { self.dev("content-packing", || detail); }
        if content.len() / wc::WORDS != self.content.len() {
            let (kept, all) = (content.len() / wc::WORDS, self.content.len());
            self.dev("content-owner", || format!("{kept} of {all} content records belong to a block"));
        }
        let mut runs: Vec<u32> = Vec::with_capacity(nr * wr::WORDS);
        let mut run_extras: Vec<u32> = Vec::new();
        const WIDE: u32 = 0xFFFF;
        for (i, rec) in self.runs.iter().enumerate() {
            let kind = rec[run::KIND];
            let (s, e, cs, ce) = (rec[run::START_UTF16], rec[run::END_UTF16], rec[run::CONTENT_START_UTF16], rec[run::CONTENT_END_UTF16]);
            let (before, after) = (cs - s, e - ce);
            let parent = rec[run::PARENT];
            let distance = if parent == u32::MAX { 0 } else { i as u32 - parent };
            let wide = before >= WIDE || after >= WIDE || distance >= WIDE;
            let mut flags = rec[run::FLAGS] & 3 | (rec[run::FLAGS] >> 8 & 1) << 2;
            if wide { flags |= 1 << 3; }
            let offset = extras.len() as u32;
            if wide { extras.extend([cs, ce, parent]); }
            match kind {
                run_kind::LINK | run_kind::IMAGE | run_kind::AUTOLINK => extras.extend([
                    u16(&self.li, rec[run::AUX0]), u16(&self.li, rec[run::AUX1]), u16(&self.li, rec[run::AUX2]), u16(&self.li, rec[run::AUX3]),
                    rec[run::DESTINATION_OFFSET], rec[run::DESTINATION_LENGTH], rec[run::TITLE_OFFSET], rec[run::TITLE_LENGTH],
                ]),
                run_kind::REPLACEMENT => extras.extend([rec[run::AUX0], rec[run::AUX1]]),
                run_kind::CODE if rec[run::FLAGS] & 2 != 0 => extras.extend([rec[run::AUX2], rec[run::AUX3]]),
                run_kind::FOOTNOTE_REF => extras.extend([u16(&self.li, rec[run::AUX0]), u16(&self.li, rec[run::AUX1])]),
                _ => {}
            }
            if extras.len() as u32 > offset { run_extras.extend([i as u32, offset]); }
            let (before, after, distance) = if wide { (WIDE, WIDE, WIDE) } else { (before, after, distance) };
            runs.extend([
                s, e,
                before | after << wr::hidden::AFTER_SHIFT,
                kind | flags << wr::kind_flags_parent::FLAGS_SHIFT | distance << wr::kind_flags_parent::PARENT_DISTANCE_SHIFT,
            ]);
        }
        let nl = self.li.line_count();
        let nd = self.definitions.len();
        let string_words = self.strings.len().div_ceil(4);
        let words = schema::HEADER_WORDS + nl * schema::line::WORDS + blocks.len() + content.len() + runs.len() + nd * schema::definition::WORDS + run_extras.len() + extras.len() + string_words;
        let mut out: Vec<u32> = Vec::with_capacity(words);
        let mut hdr = [0u32; schema::HEADER_WORDS];
        hdr[wh::MAGIC] = schema::MAGIC; hdr[wh::VERSION] = schema::VERSION;
        hdr[wh::SRC_BYTES] = self.src.len() as u32; hdr[wh::SRC_UTF16] = self.li.u16(self.src.len());
        hdr[wh::LINE_COUNT] = nl as u32; hdr[wh::BLOCK_COUNT] = nb as u32;
        hdr[wh::CONTENT_COUNT] = (content.len() / wc::WORDS) as u32; hdr[wh::RUN_COUNT] = nr as u32;
        hdr[wh::DEFINITION_COUNT] = nd as u32; hdr[wh::RUN_EXTRA_COUNT] = (run_extras.len() / schema::run_extra::WORDS) as u32;
        hdr[wh::EXTRA_WORDS] = extras.len() as u32; hdr[wh::STRING_BYTES] = self.strings.len() as u32;
        out.extend_from_slice(&hdr);
        for l in 0..nl { out.push(self.li.u16(self.li.line_start(l))); }
        out.extend_from_slice(&blocks);
        out.extend_from_slice(&content);
        out.extend_from_slice(&runs);
        for d in &self.definitions {
            out.extend([d.start, d.end, d.label.0, d.label.1, d.dest.0, d.dest.1].map(|b| self.li.u16(b)));
        }
        out.extend_from_slice(&run_extras);
        out.extend_from_slice(&extras);
        for chunk in self.strings.chunks(4) { let mut b = [0u8; 4]; b[..chunk.len()].copy_from_slice(chunk); out.push(u32::from_le_bytes(b)); }
        debug_assert_eq!(out.len(), words);
        out
    }
}

/// The model as the byte stream a host receives: each word little-endian.
pub fn to_bytes(words: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(words.len() * 4);
    for w in words { out.extend_from_slice(&w.to_le_bytes()); }
    out
}

/// Whether [range] of [bytes] holds a CR that does not begin a CRLF: the
/// line ending outside the source contract (comrak's inline line counter
/// does not advance across it). CRLF is in the contract and earns no
/// exemption.
fn has_bare_cr(bytes: &[u8], range: std::ops::Range<usize>) -> bool {
    range.into_iter().any(|i| bytes.get(i) == Some(&b'\r') && bytes.get(i + 1) != Some(&b'\n'))
}

/// For each of [lines] lines, the last of [spans] (inclusive line ranges in
/// walk order, so the last that covers a line is the innermost container)
/// covering it, leaving lines [taken] marks unowned. Visiting spans from the
/// last and jumping over lines already given away, with a union-find over the
/// next open line, touches each line once: filling every span whole was
/// depth times lines, quadratic for nested quotes continued by lazy lines.
fn innermost_per_line(lines: usize, taken: &[bool], spans: &[(usize, usize)]) -> Vec<Option<usize>> {
    fn next_open(open: &mut [usize], mut l: usize) -> usize {
        let mut root = l;
        while open[root] != root { root = open[root]; }
        while open[l] != root { let up = open[l]; open[l] = root; l = up; }
        root
    }
    let mut owner = vec![None; lines];
    let mut open: Vec<usize> = (0..=lines).map(|l| if taken.get(l) == Some(&true) { l + 1 } else { l }).collect();
    for (i, &(first, last)) in spans.iter().enumerate().rev() {
        if last == usize::MAX { continue; }
        let mut line = next_open(&mut open, first.min(lines));
        while line <= last && line < lines { owner[line] = Some(i); open[line] = line + 1; line = next_open(&mut open, line + 1); }
    }
    owner
}

/// A leaf's content records as one buffer, each line's content followed by
/// a line feed, with every byte's source offset and where each line begins:
/// the text an inline link's repair scans ([Extractor::repair_inline_link]).
/// `key` names the leaf: its first content record and the record count.
#[derive(Default)]
struct LeafText { key: (usize, usize), bytes: Vec<u8>, offsets: Vec<usize>, line_starts: Vec<usize> }

impl LeafText {
    fn new(src: &str, content: &[ContentRec], from: usize) -> Self {
        let mut text = LeafText { key: (from, content.len()), ..LeafText::default() };
        for r in &content[from..] {
            let (start, end) = (r[content::START_BYTE] as usize, r[content::END_BYTE] as usize);
            text.line_starts.push(text.bytes.len());
            text.bytes.extend_from_slice(&src.as_bytes()[start..end]);
            text.offsets.extend(start..end);
            text.bytes.push(b'\n'); text.offsets.push(end);
        }
        text
    }
}

/// The backslashes comrak removes from a table cell's raw text, or from a
/// paragraph it split to make a table header, before it parses inlines: each
/// `\|` whose backslash is not itself escaped loses the backslash, counted
/// from the leaf's start with the toggle a line ending resets. `counts[i]`
/// holds those in `origin..origin + i`, extended on demand. Every inline
/// position of the leaf is shifted by the count before it on its own line,
/// some several times over (see [Extractor::pipe_shift]); rescanning the line
/// for each made a cell or split paragraph quadratic in its length: a third
/// of a second for 36 KB of `\|*a*` cells.
#[derive(Default)]
struct PipeCounts { origin: usize, last_was_backslash: bool, counts: Vec<u32> }

impl PipeCounts {
    /// The backslashes removed in `from..upto` (clamped to `from..` the
    /// source's end), counting from a leaf whose escapes start at [origin].
    /// [from] is [origin] or a line start after it, where the toggle is reset.
    fn removed(&mut self, src: &[u8], origin: usize, from: usize, upto: usize) -> usize {
        if self.origin != origin || self.counts.is_empty() {
            *self = PipeCounts { origin, last_was_backslash: false, counts: vec![0] };
        }
        let upto = upto.min(src.len()).max(from);
        while self.origin + self.counts.len() <= upto {
            let b = src[self.origin + self.counts.len() - 1];
            let mut n = self.counts[self.counts.len() - 1];
            if self.last_was_backslash { if b == b'|' { n += 1; } self.last_was_backslash = false; }
            else if b == b'\\' { self.last_was_backslash = true; }
            self.counts.push(n);
        }
        (self.counts[upto - origin] - self.counts[from - origin]) as usize
    }
}

/// The content buffer comrak builds from some lines of a paragraph, and
/// where each of them begins in it (one more entry: the buffer's length).
struct LineBuffer { text: String, starts: Vec<usize> }

impl LineBuffer {
    /// comrak adds a paragraph line from its content start through its own
    /// ending (`add_line`), after the virtual spaces of a partially consumed
    /// tab on a lazy line. So a line keeps its trailing whitespace and a
    /// CRLF, and a document's last line without an ending enters
    /// unterminated. The definition rule reads all of that: inside angle
    /// brackets a backslash takes whatever byte follows it, so after `<b\` a
    /// trailing space or the CR of a CRLF leaves the line ending to end the
    /// destination, and a destination reaching the end of the input is none
    /// (`manual_scan_link_url`). Trimmed and joined by '\n', the lines gave
    /// definitions comrak refuses (`[a]: <b>` ending a document).
    fn new(src: &str, li: &LineIndex, lines: &[LineSpan]) -> Self {
        let (mut text, mut starts) = (String::new(), Vec::with_capacity(lines.len() + 1));
        for l in lines {
            starts.push(text.len());
            for _ in 0..l.virt { text.push(' '); }
            text.push_str(&src[l.start..li.line_end_with_break(l.line0, src.len())]);
        }
        starts.push(text.len());
        LineBuffer { text, starts }
    }

    /// The line holding buffer offset [off]; the buffer's end is its last
    /// line's.
    fn line_at(&self, off: usize) -> usize { self.starts.partition_point(|&s| s <= off).saturating_sub(1).min(self.starts.len().saturating_sub(2)) }

    /// The source byte at buffer offset [off] of [lines]. Virtual spaces
    /// hold no source byte: they map to their line's first.
    fn byte(&self, lines: &[LineSpan], off: usize) -> usize {
        let i = self.line_at(off);
        lines[i].start + (off - self.starts[i]).saturating_sub(lines[i].virt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_rec(s: usize, e: usize, parent: u32) -> RunRec {
        let mut r = [0; run::WORDS];
        r[run::START_BYTE] = s as u32; r[run::END_BYTE] = e as u32;
        r[run::CONTENT_START_BYTE] = s as u32; r[run::CONTENT_END_BYTE] = e as u32;
        r[run::PARENT] = parent;
        r
    }

    fn block_rec(kind: u32, parent: u32, s: usize, e: usize) -> BlockRec {
        let mut b = [0; block::WORDS];
        b[block::KIND] = kind; b[block::PARENT] = parent;
        b[block::START_BYTE] = s as u32; b[block::END_BYTE] = e as u32;
        b
    }

    #[test]
    fn a_leaf_whose_runs_overlap_publishes_only_its_source() {
        let mut ex = Extractor::new("abc def ghi", true);
        ex.blocks.push(block_rec(block_kind::PARAGRAPH, u32::MAX, 0, 11));
        ex.runs.push(run_rec(0, 5, u32::MAX));
        ex.runs.push(run_rec(3, 11, u32::MAX));
        ex.settle_inlines(0, 0, 0, 0);
        assert!(ex.runs.is_empty());
        assert_ne!(ex.blocks[0][block::FLAGS] & SOURCE_ONLY, 0);
        assert_eq!((ex.deviations[0].rule, ex.deviations[0].leaf), ("run-structure", Some(0)));
    }

    #[test]
    fn a_leaf_whose_runs_leave_content_uncovered_publishes_only_its_source() {
        let mut ex = Extractor::new("abc &amp; d", true);
        ex.blocks.push(block_rec(block_kind::PARAGRAPH, u32::MAX, 0, 11));
        ex.content.push([0, 0, 0, 11, 11, 0, 0, 0]);
        ex.runs.push(run_rec(0, 4, u32::MAX));
        ex.runs.push(run_rec(4, 9, u32::MAX));
        ex.settle_inlines(0, 0, 0, 0);
        assert!(ex.runs.is_empty(), "` d` lies in no run");
        assert_eq!((ex.deviations[0].rule, ex.deviations[0].leaf), ("run-structure", Some(0)));
        let mut ex = Extractor::new("abc  ", true);
        ex.blocks.push(block_rec(block_kind::PARAGRAPH, u32::MAX, 0, 5));
        ex.content.push([0, 0, 0, 5, 5, 0, 0, 0]);
        ex.runs.push(run_rec(0, 3, u32::MAX));
        ex.settle_inlines(0, 0, 0, 0);
        assert_eq!(ex.runs.len(), 1, "trailing spaces are editable whitespace");
    }

    #[test]
    fn a_container_covers_only_its_delimiters() {
        // A host paints a container's children and shows any byte between
        // them verbatim: `\` inside `*\|*` that no child holds is uncovered,
        // although the emphasis's range spans it.
        let emph = |content: (usize, usize)| { let mut r = run_rec(0, 5, u32::MAX); r[run::CONTENT_START_BYTE] = content.0 as u32; r[run::CONTENT_END_BYTE] = content.1 as u32; r };
        let mut ex = Extractor::new("*\\|x*", true);
        ex.blocks.push(block_rec(block_kind::PARAGRAPH, u32::MAX, 0, 5));
        ex.content.push([0, 0, 0, 5, 5, 0, 0, 0]);
        ex.runs.push(emph((1, 4)));
        ex.runs.push(run_rec(2, 4, 0));
        ex.settle_inlines(0, 0, 0, 0);
        assert!(ex.runs.is_empty(), "`\\` lies between the delimiters and the child");
        assert_eq!((ex.deviations[0].rule, ex.deviations[0].leaf), ("run-structure", Some(0)));
        let mut ex = Extractor::new("*\\|x*", true);
        ex.blocks.push(block_rec(block_kind::PARAGRAPH, u32::MAX, 0, 5));
        ex.content.push([0, 0, 0, 5, 5, 0, 0, 0]);
        ex.runs.push(emph((1, 4)));
        ex.runs.push(run_rec(1, 2, 0));
        ex.runs.push(run_rec(2, 4, 0));
        ex.settle_inlines(0, 0, 0, 0);
        assert_eq!(ex.runs.len(), 3, "the delimiters and the children cover the leaf");
    }

    #[test]
    fn a_run_outside_its_parent_or_before_its_sibling_is_a_problem() {
        let mut ex = Extractor::new("abc def ghi", true);
        ex.runs = vec![run_rec(0, 4, u32::MAX), run_rec(2, 6, 0)];
        assert!(ex.leaf_run_problem(0).unwrap().contains("outside its parent"));
        ex.runs = vec![run_rec(0, 11, u32::MAX), run_rec(0, 4, 0), run_rec(3, 6, 0)];
        assert!(ex.leaf_run_problem(0).unwrap().contains("overlaps its previous sibling"));
        ex.runs = vec![run_rec(0, 11, u32::MAX), run_rec(0, 4, 0), run_rec(4, 6, 0), run_rec(6, 11, u32::MAX)];
        assert!(ex.leaf_run_problem(0).is_some(), "a root run after a root run that covers it");
        ex.runs = vec![run_rec(0, 4, u32::MAX), run_rec(0, 2, 0), run_rec(4, 11, u32::MAX)];
        assert_eq!(ex.leaf_run_problem(0), None);
    }

    #[test]
    fn inline_deviations_are_scoped_to_their_leaf_and_others_refuse() {
        let mut ex = Extractor::new("abc", true);
        ex.blocks.push(block_rec(block_kind::PARAGRAPH, u32::MAX, 0, 3));
        ex.runs.push(run_rec(0, 3, u32::MAX));
        ex.dev("text-mismatch", || "synthetic".into());
        ex.dev("code-content", || "synthetic".into());
        ex.settle_inlines(0, 0, 0, 0);
        assert!(ex.runs.is_empty());
        assert_eq!(ex.deviations.iter().map(|d| (d.rule, d.leaf)).collect::<Vec<_>>(), [("text-mismatch", Some(0)), ("code-content", None)]);
    }

    #[test]
    fn an_inline_among_a_leafs_definitions_refuses_the_model() {
        // comrak parses a leaf's inlines only after removing its definitions:
        // a run inside them is text the mirror took for a definition.
        let definition = Definition { start: 0, end: 9, label: (1, 2), dest: (6, 7) };
        let mut ex = Extractor::new("[a]: <b>\nc", true);
        ex.definitions.push(definition);
        ex.runs.extend([run_rec(5, 8, u32::MAX), run_rec(9, 10, u32::MAX)]);
        ex.check_stripped_definitions(1, 0, 0);
        assert_eq!(ex.deviations.iter().map(|d| (d.rule, d.leaf)).collect::<Vec<_>>(), [("definition-inline", None)]);
        // Runs from the definitions' end on, a break at it included, are the
        // leaf's text; a leaf without definitions is not checked.
        let mut ex = Extractor::new("[a]: <b>\nc", true);
        ex.definitions.push(definition);
        ex.runs.extend([run_rec(9, 9, u32::MAX), run_rec(9, 10, u32::MAX)]);
        ex.check_stripped_definitions(1, 0, 0);
        ex.check_stripped_definitions(1, 0, 1);
        assert!(ex.deviations.is_empty());
    }

    #[test]
    fn a_split_paragraph_end_its_inlines_do_not_reach_refuses_the_model() {
        // The end derived for a paragraph a table split must be the line its
        // last inline ends on, by comrak's own numbering.
        let src = "a\nb\n";
        let arena = Arena::new();
        let root = parse_document(&arena, src, &options());
        let paragraph = root.first_child().expect("a paragraph");
        let mut ex = Extractor::new(src, true);
        ex.check_split_paragraph_end(1, paragraph, 2);
        assert!(ex.deviations.is_empty());
        for end in [1, 3] { ex.check_split_paragraph_end(1, paragraph, end); }
        assert_eq!(ex.deviations.iter().map(|d| (d.rule, d.leaf)).collect::<Vec<_>>(), [("split-paragraph-end", None), ("split-paragraph-end", None)]);
    }

    /// A seeded xorshift and a token picker for the generated shapes below.
    struct Shapes(u64);
    impl Shapes {
        fn below(&mut self, n: usize) -> usize { let mut x = self.0; x ^= x << 13; x ^= x >> 7; x ^= x << 17; self.0 = x; (x % n.max(1) as u64) as usize }
        fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str { xs[self.below(xs.len())] }
    }

    /// The lines of [src] as comrak splits them.
    fn source_lines(src: &str) -> Vec<&str> {
        let li = LineIndex::new(src);
        (0..li.line_count()).map(|l| &src[li.line_start(l)..li.line_end(l, src.len())]).collect()
    }

    #[test]
    fn no_text_node_links_more_addresses_than_its_line_bounds() {
        // comrak links the addresses of one text node recursively; the guard
        // bounds them per line. Every run of a text node's pieces, its texts
        // and the address links cut from it, stays within its line's bound,
        // whatever leads the line, wraps the addresses or spells their `@`.
        let tokens = ["a@b.c", "a@.b", "+@.c", "x@y.zz", "&#64;", "&commat;", "&#x40;", "&#X40;", "&#0064;", "a&#64;b.c", "mailto:a@b.c", "xmpp:a@b.c/r", " ", "\t", ".", "-", "_", "+", "*", "**", "~~", "\\@", "\\", "[", "]", "a", "b1", "é", "http://x.y", "www.a.b", "@", "@@", "`", "&amp;", "&nope;", "\u{b}"];
        let prefixes = ["", "> ", "- ", "1. ", "> - ", "[^n]: ", "    ", "# ", "|", "- [ ] "];
        let mut shapes = Shapes(0x5EED_A7);
        let iterations = std::env::var("FLARK_FUZZ_ITERATIONS").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(2_000) / 4;
        for _ in 0..iterations {
            let prefix = shapes.pick(&prefixes);
            let mut src = if prefix == "|" { "|a|\n|-|\n".to_string() } else { String::new() };
            for _ in 0..1 + shapes.below(3) {
                src.push_str(prefix);
                for _ in 0..shapes.below(120) { src.push_str(shapes.pick(&tokens)); }
                src.push_str(if shapes.below(3) == 0 { "\r\n" } else { "\n" });
            }
            let lines = source_lines(&src);
            let arena = Arena::new();
            let root = parse_document(&arena, &src, &options());
            // Text nodes next to each other were merged before comrak
            // linked them, so the texts and address links between two other
            // nodes are one text node's.
            let check = |run: usize, line: usize| if run > 0 {
                let bound = email_autolinks_bound(lines[line - 1]);
                assert!(run <= bound, "{run} addresses linked in one text node of a line bounded at {bound}: {:?}", lines[line - 1]);
            };
            for parent in root.descendants() {
                let (mut run, mut line) = (0usize, 0usize);
                for child in parent.children() {
                    let d = child.data.borrow();
                    match &d.value {
                        NodeValue::Link(l) if l.url.starts_with("mailto:") || l.url.starts_with("xmpp:") => { run += 1; line = d.sourcepos.start.line; }
                        NodeValue::Text(_) => {}
                        _ => { check(run, line); run = 0; }
                    }
                }
                check(run, line);
            }
        }
    }

    /// The cells comrak fills in the body rows of [src]'s tables, and its
    /// tables and their body rows. A row comrak fills has its own cells,
    /// asserted to be as many as `row_cells` splits its line into, then
    /// childless ones at its end: each filler sits one column past the last
    /// own cell's end, while an own cell starts past the pipe after the one
    /// before it.
    fn comrak_fills(src: &str) -> (usize, usize, usize) {
        let arena = Arena::new();
        let root = parse_document(&arena, src, &options());
        let source = source_lines(src);
        let (mut filled, mut tables, mut rows) = (0, 0, 0);
        for table in root.descendants() {
            let NodeValue::Table(t) = &table.data.borrow().value else { continue };
            tables += 1;
            for r in table.children().skip(1) {
                rows += 1;
                let line = r.data.borrow().sourcepos.start.line;
                let cells: Vec<(Sourcepos, bool)> = r.children().map(|c| (c.data.borrow().sourcepos, c.first_child().is_none())).collect();
                let own = (1..cells.len()).find(|&k| cells[k].1 && cells[k].0.start == cells[k].0.end && cells[k].0.start.column == cells[k - 1].0.end.column + 1).unwrap_or(cells.len());
                assert!(cells[own..].iter().all(|c| c.0.start == cells[own].0.start), "fillers share one position in {src:?}");
                let content = source[line - 1].trim_start_matches(['\u{feff}', ' ', '\t', '>']);
                assert_eq!(t.num_columns - (cells.len() - own), t.num_columns.min(row_cells(content.as_bytes())), "row {content:?} of {src:?}");
                filled += cells.len() - own;
            }
        }
        (filled, tables, rows)
    }

    #[test]
    fn filled_table_cells_bound_the_cells_comrak_fills() {
        // `filled_table_cells` counts at least every cell comrak fills, under
        // quotes, list items and footnotes, after a byte order mark, with
        // escaped pipes, ragged rows, rows that open other blocks and CRLF;
        // exactly as many when every line after the delimiter row is a row
        // of the table and none could be a wider delimiter row.
        let cell = ["a", " ", "\\|", "\\", "\\\\|", "`", "*", "&#124;", "&amp;", "-", ":", "\t", "\u{b}", "é", ">", "x"];
        let mut shapes = Shapes(0x7AB1E5);
        let iterations = std::env::var("FLARK_FUZZ_ITERATIONS").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(2_000) / 2;
        let mut exact = 0;
        for _ in 0..iterations {
            let (first, rest) = shapes.pick(&["|", "> |> ", ">|>", "- |  ", "[^n]: |    ", "> - |>   ", "1. |   ", "\u{feff}|"]).split_once('|').unwrap();
            let row = |shapes: &mut Shapes, cells: usize| {
                let mut s = String::new();
                if shapes.below(2) == 0 { s.push('|'); }
                for i in 0..cells {
                    for _ in 0..shapes.below(4) { s.push_str(shapes.pick(&cell)); }
                    if i + 1 < cells || shapes.below(2) == 0 { s.push('|'); }
                }
                s
            };
            let columns = 1 + shapes.below(6);
            let mut lines = vec![row(&mut shapes, columns)];
            let mut delimiter = String::from(if shapes.below(2) == 0 { "|" } else { "" });
            for i in 0..columns { delimiter.push_str(shapes.pick(&["-", "---", ":-:", " -: "])); if i + 1 < columns || shapes.below(2) == 0 { delimiter.push('|'); } }
            lines.push(delimiter);
            for _ in 0..shapes.below(8) {
                let line = match shapes.below(8) { 0 => shapes.pick(&["", "- item", "# h", ">", "    code", "---"]).to_string(), _ => { let n = shapes.below(columns + 3); row(&mut shapes, n) } };
                lines.push(line);
            }
            let newline = if shapes.below(3) == 0 { "\r\n" } else { "\n" };
            let src: String = lines.iter().enumerate().map(|(i, l)| format!("{}{l}{newline}", if i == 0 { first } else { rest })).collect();
            let (filled, tables, rows) = comrak_fills(&src);
            let bound = filled_table_cells(&src);
            assert!(filled <= bound, "{filled} filled cells, bound {bound} for {src:?}");
            let delimiter_like = |l: &str| l.contains('-') && l.bytes().all(|c| matches!(c, b'|' | b':' | b'-' | b' ' | b'\t' | 0x0b | 0x0c));
            if tables == 1 && rows == lines.len() - 2 && !lines[2..].iter().map(|l| l.trim_start_matches([' ', '\t', '>'])).any(|l| delimiter_like(l) && row_cells(l.as_bytes()) > columns) {
                assert_eq!(bound, filled, "for {src:?}");
                exact += 1;
            }
        }
        assert!(exact > iterations / 10, "only {exact} exact cases");
    }

    #[test]
    fn a_document_whose_tables_could_gain_too_many_cells_is_refused_before_parsing() {
        // A header of 2,048 columns over 2,046 one-character rows: 12 KB in
        // the editor's live limits, 4.2 million cells.
        let wide = |columns: usize, rows: usize| "a|".repeat(columns) + "\n" + &"-|".repeat(columns) + &"\nx".repeat(rows);
        let src = wide(2048, 2046);
        assert_eq!(filled_table_cells(&src), 2047 * 2046);
        let (model, deviations) = Extractor::extract_with_report(&src);
        assert_eq!(deviations.iter().map(|d| (d.rule, d.leaf)).collect::<Vec<_>>(), [("table-cells", None)]);
        assert_eq!(model[schema::header::BLOCK_COUNT], 1);
        // As many cells as the cap fill, or a larger document's half its bytes.
        assert_eq!(filled_table_cells(&wide(129, 128)), MAX_FILLED_TABLE_CELLS);
        assert!(Extractor::extract(&wide(129, 128)).is_ok());
        assert!(Extractor::extract(&wide(129, 129)).is_err());
        let padded = wide(130, 130) + "\n\n" + &"word ".repeat(7_000);
        assert!(filled_table_cells(&padded) <= padded.len() / 2 && Extractor::extract(&padded).is_ok());
        // Rows short of their columns by comrak's split: an escaped pipe, even
        // after an escaped backslash, delimits nothing.
        for (src, filled) in [("|a|b|c|\n|-|-|-|\n|x|\n|x|y|\n|x|y|z|\nx\n", 5), ("|a|b|\n|-|-|\n|x\\|y|\n", 1), ("|a|b|\n|-|-|\n|x\\\\|y|\n", 1), ("> a|b\n> -|-\n> x\n", 1), ("\u{feff}a|b\n-|-\nx\n", 1)] {
            assert_eq!((filled_table_cells(src), comrak_fills(src).0), (filled, filled), "{src:?}");
        }
        // Rows that fill their columns, and a blank line or a quote marker
        // ending the table, add nothing.
        let full = "| a | b | c |\n|---|---|---|\n".to_string() + &"| x | y | z |\n".repeat(5_000);
        assert_eq!(filled_table_cells(&full), 0);
        assert_eq!(filled_table_cells(&(wide(500, 0) + "\n\n" + &"x\n".repeat(500))), 0);
        assert_eq!(filled_table_cells(&(wide(500, 1) + "\n>\n" + &"x\n".repeat(500))), 499);
    }

    #[test]
    fn overlapping_sibling_blocks_refuse_the_model() {
        let mut ex = Extractor::new("abc def ghi", true);
        ex.blocks = vec![block_rec(block_kind::DOCUMENT, u32::MAX, 0, 11), block_rec(block_kind::PARAGRAPH, 0, 0, 6), block_rec(block_kind::PARAGRAPH, 0, 4, 11)];
        ex.check_structure();
        assert_eq!((ex.deviations[0].rule, ex.deviations[0].leaf), ("block-tree", None));
    }
}

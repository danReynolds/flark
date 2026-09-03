//! Link reference definitions, derived textually and validated against the
//! parser's own block output (salvaged from the v2 bridge, RFC 022 Phase 2b).
//!
//! comrak consumes definitions during the block phase and exposes no node or
//! sourcepos for them. A definition-shaped line run is proposed here and
//! accepted only when no non-container block covers it: a real definition
//! leaves no block behind, while a definition-shaped line comrak kept as
//! content (inside a fence, lazily continuing a paragraph) is covered by a
//! block and refuted. Classification proposes; the parser decides.

/// One accepted definition. Ranges are byte offsets; `end` includes the
/// trailing newline when present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Definition {
    pub start: usize,
    pub end: usize,
    pub label: (usize, usize),
    pub dest: (usize, usize),
}

/// A block as the coverage check sees it.
#[derive(Clone, Copy)]
pub struct CoverBlock { pub start: usize, pub end: usize, pub is_container_list: bool, pub is_document: bool }

pub fn collect(text: &str, blocks: &[CoverBlock]) -> Vec<Definition> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    while start <= bytes.len() {
        let end = match bytes[start..].iter().position(|b| *b == b'\n') { Some(o) => start + o, None => bytes.len() };
        let end_with_break = if end < bytes.len() { end + 1 } else { end };
        lines.push((start, end, end_with_break));
        if end >= bytes.len() { break; }
        start = end + 1;
    }
    let mut out = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let (ls, le, lbe) = lines[index];
        let line = &bytes[ls..le];
        let Some(parts) = definition_parts(line) else { index += 1; continue; };
        let mut range_end = lbe;
        if !definition_line_has_title(line) && index + 1 < lines.len() {
            let (ns, ne, nbe) = lines[index + 1];
            if is_standalone_title_line(&bytes[ns..ne]) { range_end = nbe; index += 1; }
        }
        let covered = blocks.iter().any(|b| !b.is_container_list && !b.is_document && b.start < range_end && b.end > ls);
        if !covered {
            out.push(Definition { start: ls, end: range_end, label: (ls + parts.0, ls + parts.1), dest: (ls + parts.2, ls + parts.3) });
        }
        index += 1;
    }
    out
}

/// `[label]: destination…` with up to three leading spaces or tabs, a
/// non-empty single-line label, and a non-space destination. Returns
/// (label_start, label_end, dest_start, dest_end) relative to the line.
fn definition_parts(line: &[u8]) -> Option<(usize, usize, usize, usize)> {
    let after_indent = skip_indent(line);
    let rest = &line[after_indent..];
    if rest.first() != Some(&b'[') { return None; }
    if rest.get(1) == Some(&b'^') { return None; }
    let label_len = label_length(&rest[1..])?;
    let mut cursor = 1 + label_len + 1;
    if rest.get(cursor) != Some(&b':') { return None; }
    cursor += 1;
    while matches!(rest.get(cursor), Some(b' ') | Some(b'\t')) { cursor += 1; }
    let dest_start = cursor;
    match rest.get(cursor) {
        Some(b'<') => {
            cursor += 1;
            while let Some(b) = rest.get(cursor) { if *b == b'>' { break; } if *b == b'\n' { return None; } cursor += 1; }
            if rest.get(cursor) != Some(&b'>') { return None; }
            cursor += 1;
        }
        Some(b) if !b.is_ascii_whitespace() => { while matches!(rest.get(cursor), Some(b) if !b.is_ascii_whitespace()) { cursor += 1; } }
        _ => return None,
    }
    Some((after_indent + 1, after_indent + 1 + label_len, after_indent + dest_start, after_indent + cursor))
}

fn definition_line_has_title(line: &[u8]) -> bool {
    let Some((_, _, _, dest_end)) = definition_parts(line) else { return false; };
    let mut cursor = dest_end;
    let mut saw_gap = false;
    while matches!(line.get(cursor), Some(b' ') | Some(b'\t')) { saw_gap = true; cursor += 1; }
    saw_gap && matches!(line.get(cursor), Some(b'"') | Some(b'\'') | Some(b'('))
}

fn is_standalone_title_line(line: &[u8]) -> bool {
    let mut cursor = 0usize;
    while matches!(line.get(cursor), Some(b' ') | Some(b'\t')) { cursor += 1; }
    let closer = match line.get(cursor) { Some(b'"') => b'"', Some(b'\'') => b'\'', Some(b'(') => b')', _ => return false };
    cursor += 1;
    while let Some(b) = line.get(cursor) {
        if *b == closer {
            cursor += 1;
            while matches!(line.get(cursor), Some(b' ') | Some(b'\t')) { cursor += 1; }
            return cursor == line.len();
        }
        cursor += 1;
    }
    false
}

fn skip_indent(line: &[u8]) -> usize {
    let mut c = 0usize;
    while c < 3 && matches!(line.get(c), Some(b' ') | Some(b'\t')) { c += 1; }
    c
}

fn label_length(rest: &[u8]) -> Option<usize> {
    let mut n = 0usize;
    for b in rest { match b { b']' => return if n == 0 { None } else { Some(n) }, b'\n' => return None, _ => n += 1 } }
    None
}

// ---------------------------------------------------------------------------
// Mirror of comrak's `resolve_reference_link_definitions`: definitions are
// parsed from the start of a paragraph's content buffer (container prefixes
// stripped, lines joined by '\n') while the buffer begins with '['. Offsets
// returned are into that buffer.

/// One definition consumed from the start of a paragraph buffer. Offsets are
/// buffer offsets: `end` is where the next definition or the paragraph text
/// begins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferDefinition { pub start: usize, pub end: usize, pub label: (usize, usize), pub dest: (usize, usize) }

pub fn paragraph_definitions(buffer: &str) -> Vec<BufferDefinition> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    let bytes = buffer.as_bytes();
    while pos < buffer.len() && bytes[pos] == b'[' {
        match parse_reference_inline(&buffer[pos..]) {
            Some((len, label, dest)) => { out.push(BufferDefinition { start: pos, end: pos + len, label: (pos + label.0, pos + label.1), dest: (pos + dest.0, pos + dest.1) }); pos += len; }
            None => break,
        }
    }
    out
}

const MAX_LINK_LABEL_LENGTH: usize = 1000;

fn ispunct(b: u8) -> bool { b.is_ascii_punctuation() }
fn isspace(b: u8) -> bool { matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) }

struct Sc { pos: usize }
impl Sc {
    fn peek(&self, s: &[u8]) -> Option<u8> { s.get(self.pos).copied() }
    fn skip_spaces(&mut self, s: &[u8]) { while matches!(self.peek(s), Some(b' ') | Some(b'\t')) { self.pos += 1; } }
    fn skip_line_end(&mut self, s: &[u8]) -> bool { let old = self.pos; if self.peek(s) == Some(b'\r') { self.pos += 1; } if self.peek(s) == Some(b'\n') { self.pos += 1; } self.pos > old || self.pos >= s.len() }
    fn spnl(&mut self, s: &[u8]) { self.skip_spaces(s); if self.skip_line_end(s) { self.skip_spaces(s); } }
    /// comrak `Scanner::link_label`: returns the trimmed label range.
    fn link_label(&mut self, s: &[u8]) -> Option<(usize, usize)> {
        let start = self.pos;
        if self.peek(s) != Some(b'[') { return None; }
        self.pos += 1;
        let mut length = 0usize;
        while let Some(b) = self.peek(s) {
            if b == b']' {
                let (mut a, mut z) = (start + 1, self.pos);
                while a < z && isspace(s[a]) { a += 1; }
                while z > a && isspace(s[z - 1]) { z -= 1; }
                self.pos += 1;
                return Some((a, z));
            }
            if b == b'[' { break; }
            if b == b'\\' { self.pos += 1; length += 1; if self.peek(s).is_some_and(ispunct) { self.pos += 1; length += 1; } }
            else { self.pos += 1; length += 1; }
            if length > MAX_LINK_LABEL_LENGTH { self.pos = start; return None; }
        }
        self.pos = start;
        None
    }
}

/// comrak `manual_scan_link_url`: returns (dest_start, dest_end, consumed).
fn scan_link_url(s: &[u8]) -> Option<(usize, usize, usize)> {
    let len = s.len();
    if len > 0 && s[0] == b'<' {
        let mut i = 1;
        while i < len {
            let b = s[i];
            if b == b'>' { i += 1; break; }
            else if b == b'\\' { i += 2; }
            else if b == b'\n' || b == b'\r' || b == b'<' { return None; }
            else { i += 1; }
        }
        if i > len || i < 2 || s.get(i - 1) != Some(&b'>') { return None; }
        return Some((1, i - 1, i));
    }
    let mut i = 0; let mut nb_p = 0i32;
    while i < len {
        if s[i] == b'\\' && i + 1 < len && ispunct(s[i + 1]) { i += 2; }
        else if s[i] == b'(' { nb_p += 1; i += 1; if nb_p > 32 { return None; } }
        else if s[i] == b')' { if nb_p == 0 { break; } nb_p -= 1; i += 1; }
        else if isspace(s[i]) || (s[i].is_ascii_control() && s[i] != 0) { if i == 0 { return None; } break; }
        else { i += 1; }
    }
    if len == 0 || nb_p != 0 { None } else { Some((0, i, i)) }
}

/// cmark `link_title`: "…", '…' or (…) with backslash escapes; may span lines.
fn scan_link_title(s: &[u8]) -> Option<usize> {
    let open = *s.first()?;
    let close = match open { b'"' => b'"', b'\'' => b'\'', b'(' => b')', _ => return None };
    let mut i = 1;
    while i < s.len() {
        let b = s[i];
        if b == b'\\' && i + 1 < s.len() && ispunct(s[i + 1]) { i += 2; continue; }
        if b == close { return Some(i + 1); }
        if open == b'(' && b == b'(' { return None; }
        if b == 0 { return None; }
        i += 1;
    }
    None
}

/// comrak `parse_reference_inline`, returning (consumed, label range, dest range).
fn parse_reference_inline(content: &str) -> Option<(usize, (usize, usize), (usize, usize))> {
    let s = content.as_bytes();
    let mut sc = Sc { pos: 0 };
    let label = sc.link_label(s)?;
    if label.0 == label.1 { return None; }
    if sc.peek(s) != Some(b':') { return None; }
    sc.pos += 1;
    sc.spnl(s);
    let (ds, de, matchlen) = scan_link_url(&s[sc.pos..])?;
    let dest = (sc.pos + ds, sc.pos + de);
    sc.pos += matchlen;
    let beforetitle = sc.pos;
    sc.spnl(s);
    let title_len = if sc.pos == beforetitle { None } else { scan_link_title(&s[sc.pos..]) };
    let has_title = match title_len { Some(n) => { sc.pos += n; true } None => { sc.pos = beforetitle; false } };
    sc.skip_spaces(s);
    if !sc.skip_line_end(s) {
        if has_title {
            sc.pos = beforetitle; sc.skip_spaces(s);
            if !sc.skip_line_end(s) { return None; }
        } else { return None; }
    }
    Some((sc.pos, label, dest))
}

#[cfg(test)]
mod mirror_tests {
    use super::*;
    #[test]
    fn strips_definition_then_paragraph_text() {
        let d = paragraph_definitions("[foo]: /url\n\"title\" ok");
        assert_eq!(d.len(), 1); assert_eq!(d[0].end, 12);
    }
    #[test]
    fn rejects_title_followed_by_text_on_same_line() { assert!(paragraph_definitions("[foo]: /url \"title\" ok").is_empty()); }
    #[test]
    fn multiline_label() { let d = paragraph_definitions("[\nfoo]: /url\nbar"); assert_eq!(d.len(), 1); assert_eq!(d[0].end, 13); }
    #[test]
    fn several_in_a_row() { assert_eq!(paragraph_definitions("[a]: /a\n[b]: /b\ntext").len(), 2); }
}

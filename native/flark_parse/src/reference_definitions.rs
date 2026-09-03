//! Link reference definitions, derived by mirroring comrak's own rule.
//!
//! comrak consumes definitions during the block phase and exposes no node or
//! sourcepos for them. `resolve_reference_link_definitions` strips them from
//! the start of a paragraph's content buffer while it begins with `[`, and a
//! paragraph that becomes blank is removed. The extraction rebuilds that
//! buffer for every paragraph-like leaf and for every run of lines no leaf
//! block covers, and runs the same parser over it. Every Text literal comrak
//! reports is then checked against its corrected source slice, which is the
//! validation for this derivation.

/// One definition in source coordinates. `end` includes the trailing line
/// terminator when present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Definition {
    pub start: usize,
    pub end: usize,
    pub label: (usize, usize),
    pub dest: (usize, usize),
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
pub(crate) fn isspace(b: u8) -> bool { matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) }

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
pub(crate) fn scan_link_url(s: &[u8]) -> Option<(usize, usize, usize)> {
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
        if i >= len || i < 2 || s.get(i - 1) != Some(&b'>') { return None; }
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
pub(crate) fn scan_link_title(s: &[u8]) -> Option<usize> {
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

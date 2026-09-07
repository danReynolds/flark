//! Splitting a text node whose literal differs from its source slice into
//! exact pieces and replacement pieces, so a host keeps exact ranges around
//! an entity, an escaped pipe, or a stray CR instead of one opaque run.
//!
//! The split is derived by a lockstep walk and validated by construction:
//! the pieces cover the slice exactly and their displays concatenate to the
//! literal. When no resync is found the caller keeps the single run.

/// A piece of a text node: source byte range within the slice, and the
/// literal byte range it displays as (`None` when the piece is exact).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Piece { pub start: usize, pub end: usize, pub display: Option<(usize, usize)> }

const MAX_SOURCE: usize = 40;
const MAX_DISPLAY_CHARS: usize = 4;

pub(crate) fn split_pieces(slice: &str, literal: &str) -> Option<Vec<Piece>> {
    let sb = slice.as_bytes();
    let mut pieces = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    let mut exact_start: Option<(usize, usize)> = None;
    loop {
        // An entity is one unit even when its decoded text begins with the
        // same character (`&amp;` decodes to `&`).
        if let Some(l) = entity_len(&slice[i..]) {
            if let Some(jj) = resync_after(&slice[i + l..], literal, j) {
                if let Some((es, _)) = exact_start.take() { pieces.push(Piece { start: es, end: i, display: None }); }
                pieces.push(Piece { start: i, end: i + l, display: Some((j, jj)) });
                i += l; j = jj;
                continue;
            }
        }
        let sc = slice[i..].chars().next();
        let lc = literal[j..].chars().next();
        match (sc, lc) {
            (None, None) => break,
            (Some(a), Some(b)) if a == b => {
                if exact_start.is_none() { exact_start = Some((i, j)); }
                i += a.len_utf8(); j += b.len_utf8();
                continue;
            }
            _ => {}
        }
        if let Some((es, _)) = exact_start.take() { pieces.push(Piece { start: es, end: i, display: None }); }
        // Resync: consume L source bytes and K literal chars, the cheapest
        // pair after which the next few characters agree again.
        let mut found = None;
        'outer: for cost in 1..=(MAX_SOURCE + MAX_DISPLAY_CHARS) {
            for l in 0..=cost.min(MAX_SOURCE) {
                let k = cost - l;
                if k > MAX_DISPLAY_CHARS || i + l > sb.len() || !slice.is_char_boundary(i + l) { continue; }
                let mut jj = j;
                let mut ok = true;
                for _ in 0..k { match literal[jj..].chars().next() { Some(c) => jj += c.len_utf8(), None => { ok = false; break; } } }
                if !ok { continue; }
                if agree(&slice[i + l..], &literal[jj..]) { found = Some((l, jj)); break 'outer; }
            }
        }
        let (l, jj) = found?;
        pieces.push(Piece { start: i, end: i + l, display: Some((j, jj)) });
        i += l; j = jj;
    }
    if let Some((es, _)) = exact_start.take() { pieces.push(Piece { start: es, end: i, display: None }); }
    // Validate by construction.
    let mut rebuilt = String::new();
    let mut cursor = 0;
    for p in &pieces {
        if p.start != cursor || p.end < p.start { return None; }
        cursor = p.end;
        match p.display { None => rebuilt.push_str(&slice[p.start..p.end]), Some((a, b)) => rebuilt.push_str(&literal[a..b]) }
    }
    if cursor != slice.len() || rebuilt != literal { return None; }
    Some(pieces)
}

/// Byte length of a well-formed entity reference at the start of `s`.
fn entity_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    if b.first() != Some(&b'&') { return None; }
    let mut i = 1;
    if b.get(1) == Some(&b'#') {
        i = 2;
        let hex = matches!(b.get(2), Some(b'x' | b'X'));
        if hex { i = 3; }
        let start = i;
        while i < b.len() && i - start < 8 && (b[i].is_ascii_digit() || (hex && b[i].is_ascii_hexdigit())) { i += 1; }
        if i == start { return None; }
    } else {
        while i < b.len() && i < 33 && b[i].is_ascii_alphanumeric() { i += 1; }
        if i == 1 { return None; }
    }
    if b.get(i) == Some(&b';') { Some(i + 1) } else { None }
}

/// After consuming source up to `rest`, the literal position from `j` (up
/// to four characters on) where the texts agree again.
fn resync_after(rest: &str, literal: &str, j: usize) -> Option<usize> {
    let mut jj = j;
    for _ in 0..=MAX_DISPLAY_CHARS {
        if agree(rest, &literal[jj..]) { return Some(jj); }
        match literal[jj..].chars().next() { Some(c) => jj += c.len_utf8(), None => return None }
    }
    None
}

/// The next three characters (or the ends) agree; a source entity counts as
/// the end of what must agree, since it decodes on its own.
fn agree(a: &str, b: &str) -> bool {
    let (mut ai, mut bi) = (a.char_indices(), b.chars());
    for _ in 0..3 {
        match (ai.next(), bi.next()) {
            (None, None) => return true,
            (Some((o, x)), y) => {
                if entity_len(&a[o..]).is_some() { return true; }
                match y { Some(y) if x == y => continue, _ => return false }
            }
            (None, Some(_)) => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(slice: &str, literal: &str) -> Vec<(String, Option<String>)> {
        split_pieces(slice, literal).unwrap().into_iter().map(|p| (slice[p.start..p.end].to_string(), p.display.map(|(a, b)| literal[a..b].to_string()))).collect()
    }

    #[test]
    fn entity_in_the_middle() {
        assert_eq!(shown("x &amp; 😀y", "x & 😀y"), vec![("x ".into(), None), ("&amp;".into(), Some("&".into())), (" 😀y".into(), None)]);
    }

    #[test]
    fn numeric_entity_at_the_edges() {
        assert_eq!(shown("&#35;a&#x1F600;", "#a😀"), vec![("&#35;".into(), Some("#".into())), ("a".into(), None), ("&#x1F600;".into(), Some("😀".into()))]);
    }

    #[test]
    fn escaped_pipe_hides_the_backslash() {
        assert_eq!(shown("a \\| b", "a | b"), vec![("a ".into(), None), ("\\".into(), Some("".into())), ("| b".into(), None)]);
    }

    #[test]
    fn stray_cr_is_hidden() {
        assert_eq!(shown("a\rb", "ab"), vec![("a".into(), None), ("\r".into(), Some("".into())), ("b".into(), None)]);
    }

    #[test]
    fn virtual_leading_spaces_are_a_zero_width_piece() {
        assert_eq!(shown("bar", "  bar"), vec![("".into(), Some("  ".into())), ("bar".into(), None)]);
    }

    #[test]
    fn double_encoded_entity_keeps_the_literal_tail_exact() {
        assert_eq!(shown("&amp;amp;", "&amp;"), vec![("&amp;".into(), Some("&".into())), ("amp;".into(), None)]);
    }

    #[test]
    fn unknown_entities_stay_exact() {
        assert_eq!(shown("a &foo; b&amp;", "a &foo; b&"), vec![("a &foo; b".into(), None), ("&amp;".into(), Some("&".into()))]);
    }

    #[test]
    fn unrelated_text_does_not_split() {
        assert_eq!(split_pieces("completely", "different words here"), None);
    }
}

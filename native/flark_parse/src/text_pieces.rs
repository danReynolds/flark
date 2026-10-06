//! Splitting a text node whose literal differs from its source slice into
//! exact pieces and replacement pieces, so a host keeps exact ranges around
//! an entity, an escaped pipe, or a stray CR instead of one opaque run.
//!
//! The split is derived by a lockstep walk and validated by construction:
//! the pieces cover the slice exactly and their displays concatenate to the
//! literal. An entity reference's piece displays exactly comrak's decoding of
//! it. When no resync is found the caller keeps the single run.

use std::cell::RefCell;
use std::collections::HashMap;

use comrak::nodes::NodeValue;

/// A piece of a text node: source byte range within the slice, and the
/// literal byte range it displays as (`None` when the piece is exact).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Piece { pub start: usize, pub end: usize, pub display: Option<(usize, usize)> }

const MAX_SOURCE: usize = 40;
const MAX_DISPLAY_CHARS: usize = 4;

/// The longest entity reference `entity_len` accepts: `&`, a 32-byte name
/// and `;`.
pub(crate) const MAX_ENTITY_LEN: usize = 34;

/// comrak's decoding of entity references, cached for one extraction.
/// comrak's entity table is private, so a reference is decoded by parsing it
/// alone; what comrak makes of it is exactly what the reference's piece must
/// display. A reference CommonMark does not decode (`&nope;`, a numeric one
/// of eight digits) comes back as itself.
#[derive(Default)]
pub(crate) struct Entities { decoded: RefCell<HashMap<String, String>> }

impl Entities {
    pub(crate) fn decode(&self, reference: &str) -> String {
        if let Some(shown) = self.decoded.borrow().get(reference) { return shown.clone(); }
        let arena = comrak::Arena::new();
        let root = comrak::parse_document(&arena, reference, &comrak::Options::default());
        let text = root.first_child().filter(|p| p.next_sibling().is_none()).and_then(|p| p.first_child()).filter(|t| t.next_sibling().is_none());
        let shown = match text.map(|t| t.data.borrow().value.clone()) {
            Some(NodeValue::Text(t)) => { let t: &str = &t; t.to_string() }
            _ => reference.to_string(),
        };
        self.decoded.borrow_mut().insert(reference.to_string(), shown.clone());
        shown
    }

    /// The length of [reference]'s decoding when [rest] begins with it.
    fn decoded_len(&self, reference: &str, rest: &[u8]) -> Option<usize> {
        if let Some(shown) = self.decoded.borrow().get(reference) { return rest.starts_with(shown.as_bytes()).then_some(shown.len()); }
        let shown = self.decode(reference);
        rest.starts_with(shown.as_bytes()).then_some(shown.len())
    }
}

/// Whether [slice] holds [literal]: equal, or split into pieces whose every
/// replacement has a known reason (an entity showing its decoding, the
/// backslash of an escaped pipe where comrak unescapes pipes, a stray CR, a
/// partial tab's leading virtual spaces). A replacement for anything else
/// means comrak placed the text where it is not. [pipes] is set for a table
/// cell and for a paragraph split to make a table header, the only leaves
/// whose pipes comrak unescapes before parsing inlines.
pub(crate) fn explains(slice: &str, literal: &str, entities: &Entities, pipes: bool) -> bool {
    if slice == literal || unescapes_to(slice, literal) { return true; }
    if !could_explain(slice, literal, entities) { return false; }
    let Some(pieces) = split_pieces(slice, literal, entities, pipes) else { return false };
    pieces.iter().all(|p| match p.display {
        None => true,
        Some((a, b)) => {
            let (source, shown) = (&slice[p.start..p.end], &literal[a..b]);
            (source.is_empty() && p.start == 0 && shown.bytes().all(|c| c == b' '))
                // A reference that displays anything but its decoding would
                // hide the text after it in its own display, leaving that
                // text's source in no run (shown twice by a host).
                || (entity_len(source) == Some(source.len()) && entities.decode(source) == shown)
                || (pipes && source == "\\" && shown.is_empty() && slice[p.end..].starts_with('|'))
                // A stray CR is dropped from the literal; it displays nothing.
                || (!source.is_empty() && source.bytes().all(|c| c == b'\r') && shown.is_empty())
        }
    })
}

/// Whether [literal] can be [slice] read through the replacements `explains`
/// accepts: a few leading spaces the slice lacks, then the slice with each
/// entity reference shown decoded or as written, and each CR and backslash
/// before a pipe dropped or kept. Every exact piece of an explained slice is
/// read as written and every replacement is one of those, so a slice that
/// cannot be read so explains nothing, and the piece walk, which tries every
/// resync at its first mismatch, is spared it. A relocation search walks
/// hundreds of windows for each text of a leaf whose texts comrak misplaced:
/// 158 ms for 8 KB. The read branches only at references, CRs and escaped
/// pipes; past its step budget it answers yes and leaves the slice to the walk.
fn could_explain(slice: &str, literal: &str, entities: &Entities) -> bool {
    let (s, l) = (slice.as_bytes(), literal.as_bytes());
    let lead = l.iter().take_while(|&&c| c == b' ').count();
    let mut budget = 4 * (s.len() + l.len()) + 64;
    // The reads still to try, as source and literal offsets; with no room
    // left for another, the answer is yes. No leading spaces is tried first.
    let mut reads = [(0usize, 0usize); 16];
    let mut pending = 0;
    for j in (0..=lead).rev() {
        if pending == reads.len() { return true; }
        reads[pending] = (0, j);
        pending += 1;
    }
    while pending > 0 {
        pending -= 1;
        let (mut i, mut j) = reads[pending];
        loop {
            if budget == 0 { return true; }
            budget -= 1;
            let Some(&c) = s.get(i) else { if j == l.len() { return true; } break };
            let decoded = if c == b'&' { entity_len(&slice[i..]).and_then(|n| entities.decoded_len(&slice[i..i + n], &l[j..]).map(|d| (i + n, j + d))) } else { None };
            let dropped = (c == b'\r' || (c == b'\\' && s.get(i + 1) == Some(&b'|'))).then_some((i + 1, j));
            for read in [decoded, dropped].into_iter().flatten() {
                if pending == reads.len() { return true; }
                reads[pending] = read;
                pending += 1;
            }
            if l.get(j) != Some(&c) { break; }
            i += 1; j += 1;
        }
    }
    false
}

/// The ends of the windows of [src] from [q] up to [limit] that
/// [could_explain] [literal], or `None` when the read runs past its budget
/// and every window has to be walked. One read from [q] answers for every
/// window it starts: a window takes a reference whole or as written (a
/// window ending inside a reference is never walked), and drops a backslash
/// only before a pipe inside the window, read with it here.
pub(crate) fn read_ends(src: &str, q: usize, limit: usize, literal: &str, entities: &Entities) -> Option<Vec<usize>> {
    let (s, l) = (src.as_bytes(), literal.as_bytes());
    let lead = l.iter().take_while(|&&c| c == b' ').count();
    let mut budget = 4 * (limit - q + l.len()) + 64;
    let mut reads: Vec<(usize, usize)> = (0..=lead).rev().map(|j| (q, j)).collect();
    let mut ends = Vec::new();
    while let Some((mut i, mut j)) = reads.pop() {
        loop {
            if budget == 0 { return None; }
            budget -= 1;
            if j == l.len() && !ends.contains(&i) { ends.push(i); }
            if i == limit { break; }
            let c = s[i];
            if c == b'&' && j < l.len() {
                if let Some(n) = entity_len(&src[i..]).filter(|&n| i + n <= limit) {
                    if let Some(d) = entities.decoded_len(&src[i..i + n], &l[j..]) { reads.push((i + n, j + d)); }
                }
            }
            if c == b'\r' { reads.push((i + 1, j)); }
            if c == b'\\' && i + 2 <= limit && s[i + 1] == b'|' && l.get(j) == Some(&b'|') { reads.push((i + 2, j + 1)); }
            if l.get(j) != Some(&c) { break; }
            i += 1; j += 1;
        }
    }
    Some(ends)
}

/// Whether [unescape_pipes] of [slice] is [literal], without building it.
fn unescapes_to(slice: &str, literal: &str) -> bool {
    let (s, l) = (slice.as_bytes(), literal.as_bytes());
    let (mut i, mut j) = (0usize, 0usize);
    while i < s.len() {
        // A backslash before a pipe is dropped; before anything else it is
        // kept together with the character it escapes.
        let take = match (s[i], s.get(i + 1)) {
            (b'\\', Some(b'|')) => { i += 1; continue; }
            (b'\\', Some(_)) => 1 + slice[i + 1..].chars().next().map_or(0, char::len_utf8),
            _ => 1,
        };
        if l.get(j..j + take) != Some(&s[i..i + take]) { return false; }
        i += take; j += take;
    }
    j == l.len()
}

/// [slice] with each escaped pipe's backslash removed, by comrak's toggle
/// rule: a backslash escapes the byte after it, so `\\\\|` keeps its pipe.
pub(crate) fn unescape_pipes(slice: &str) -> String {
    let mut out = String::with_capacity(slice.len());
    let mut chars = slice.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('|') => {}
                Some(_) => { out.push(c); out.push(chars.next().unwrap()); continue; }
                None => out.push(c),
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub(crate) fn split_pieces(slice: &str, literal: &str, entities: &Entities, pipes: bool) -> Option<Vec<Piece>> {
    let sb = slice.as_bytes();
    let mut pieces = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    let mut exact_start: Option<(usize, usize)> = None;
    loop {
        // An entity is one unit that displays exactly its decoding, even when
        // the decoding begins with the same character (`&amp;` is `&`) or the
        // next source bytes differ from the literal (`&amp;\|` in a cell).
        // A reference comrak does not decode is compared as ordinary text.
        if let Some(l) = entity_len(&slice[i..]) {
            let shown = entities.decode(&slice[i..i + l]);
            if shown != slice[i..i + l] && literal[j..].starts_with(shown.as_str()) {
                if let Some((es, _)) = exact_start.take() { pieces.push(Piece { start: es, end: i, display: None }); }
                pieces.push(Piece { start: i, end: i + l, display: Some((j, j + shown.len())) });
                i += l; j += shown.len();
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
        // With the source used up, a resync would need a piece of no source
        // bytes, which only the leading piece may be.
        if i == sb.len() && i > 0 { return None; }
        // Resync: consume L source bytes and K literal chars, the cheapest
        // pair after which the next few characters agree again.
        let mut found = None;
        // Only the pairs within reach are tried, cheapest first: at most
        // MAX_DISPLAY_CHARS literal chars, no source past its end. Every
        // pair of every cost was visited, about a thousand at a mismatch, in
        // each text of a leaf comrak misplaced.
        let rest = sb.len() - i;
        'outer: for cost in 1..=(MAX_SOURCE + MAX_DISPLAY_CHARS).min(rest + MAX_DISPLAY_CHARS) {
            for l in cost.saturating_sub(MAX_DISPLAY_CHARS)..=cost.min(MAX_SOURCE).min(rest) {
                let k = cost - l;
                // Only a leading piece may display text the source does not
                // hold, and only spaces: a partially consumed tab's virtual
                // spaces, all `explains` accepts. Anywhere else a zero-length
                // source piece would put display text at an offset no caret
                // can reach. A leading `|` won where a cell's text opens with
                // `\|&#124;` (`agree` reads `\|` as `|`), so the backslash's
                // own piece was never tried.
                if l == 0 && i > 0 { continue; }
                if k > MAX_DISPLAY_CHARS || i + l > sb.len() || !slice.is_char_boundary(i + l) { continue; }
                let mut jj = j;
                let mut ok = true;
                for _ in 0..k { match literal[jj..].chars().next() { Some(c) => jj += c.len_utf8(), None => { ok = false; break; } } }
                if !ok || (l == 0 && literal[j..jj].bytes().any(|c| c != b' ')) { continue; }
                if agree(&slice[i + l..], &literal[jj..], pipes) { found = Some((l, jj)); break 'outer; }
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

/// Length of the entity reference at the start of [s], if there is one.
pub(crate) fn entity_len(s: &str) -> Option<usize> {
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

/// Whether the source range [q, r) cuts through an entity reference. comrak
/// decodes a whole reference into one text node, so a text run neither ends
/// nor begins inside one, unless its `&` is escaped: `\&amp;` is an escape
/// whose text is the `&` alone, then the text `amp;`. A relocation candidate
/// that cuts a reference matched the reference's first bytes, not the text.
pub(crate) fn cuts_entity(src: &str, q: usize, r: usize) -> bool {
    let b = src.as_bytes();
    let reaches = |a: usize, past: usize| b[a] == b'&' && entity_len(&src[a..]).is_some_and(|l| a + l > past);
    let escaped = |a: usize| b[..a].iter().rev().take_while(|&&c| c == b'\\').count() % 2 == 1;
    (r.saturating_sub(MAX_ENTITY_LEN).max(q)..r).any(|a| reaches(a, r) && !(a == q && r == a + 1 && escaped(a)))
        || (q.saturating_sub(MAX_ENTITY_LEN)..q).any(|a| reaches(a, q) && !escaped(a))
}

/// [cuts_entity] for the windows of one span of the source, answered from
/// one pass over the references that reach into it: `inside[x - from]` is one
/// more than the start of the reference that byte `x` lies strictly inside
/// (references never overlap: none holds a `&`), or zero.
pub(crate) struct EntityCover { from: usize, inside: Vec<u32> }

impl EntityCover {
    /// The references reaching any of `from..=to`.
    pub(crate) fn new(src: &str, from: usize, to: usize) -> Self {
        let b = src.as_bytes();
        let mut inside = vec![0u32; to + 1 - from];
        for a in from.saturating_sub(MAX_ENTITY_LEN)..to {
            if b[a] != b'&' { continue; }
            let Some(l) = entity_len(&src[a..]) else { continue };
            for x in (a + 1).max(from)..(a + l).min(to + 1) { inside[x - from] = a as u32 + 1; }
        }
        EntityCover { from, inside }
    }

    fn covering(&self, x: usize) -> Option<usize> { self.inside.get(x - self.from).and_then(|&a| a.checked_sub(1)).map(|a| a as usize) }

    /// [cuts_entity] of `q..r`, both in the covered span.
    pub(crate) fn cuts(&self, src: &str, q: usize, r: usize) -> bool {
        let b = src.as_bytes();
        let escaped = |a: usize| b[..a].iter().rev().take_while(|&&c| c == b'\\').count() % 2 == 1;
        self.covering(r).is_some_and(|a| a >= q && !(a == q && r == a + 1 && escaped(a))) || self.covering(q).is_some_and(|a| !escaped(a))
    }
}

/// The next three characters (or the ends) agree; a source entity counts as
/// the end of what must agree, since it decodes on its own. Where comrak
/// unescapes pipes, the backslash of `\|` is not in the literal.
fn agree(a: &str, b: &str, pipes: bool) -> bool {
    let (mut ai, mut bi) = (a.char_indices(), b.chars());
    for _ in 0..3 {
        match (ai.next(), bi.next()) {
            (None, None) => return true,
            (Some((o, x)), y) => {
                if entity_len(&a[o..]).is_some() { return true; }
                let x = if pipes && x == '\\' && a[o + 1..].starts_with('|') { ai.next(); '|' } else { x };
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

    fn split(slice: &str, literal: &str, pipes: bool) -> Vec<(String, Option<String>)> {
        split_pieces(slice, literal, &Entities::default(), pipes).unwrap().into_iter().map(|p| (slice[p.start..p.end].to_string(), p.display.map(|(a, b)| literal[a..b].to_string()))).collect()
    }

    fn shown(slice: &str, literal: &str) -> Vec<(String, Option<String>)> { split(slice, literal, false) }

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
        assert_eq!(split("a \\| b", "a | b", true), vec![("a ".into(), None), ("\\".into(), Some("".into())), ("| b".into(), None)]);
    }

    #[test]
    fn an_entity_before_an_escaped_pipe_displays_only_itself() {
        let pieces = vec![("&amp;".into(), Some("&".into())), ("\\".into(), Some("".into())), ("|".into(), None)];
        assert_eq!(split("&amp;\\|", "&|", true), pieces);
        assert!(explains("&amp;\\|", "&|", &Entities::default(), true));
        // An entity piece that displayed the following pipe would leave the
        // pipe's source in no run.
        assert!(!explains("&amp;", "&|", &Entities::default(), true));
        assert!(!explains("&nbsp;", "\u{a0} |", &Entities::default(), true));
    }

    #[test]
    fn an_escaped_pipe_opening_a_text_keeps_its_own_piece() {
        // `agree` reads `\|` as `|`, so a zero-length leading piece showing
        // `|` agreed before the backslash's own piece was tried, and
        // `explains` rejects a leading piece that shows anything but spaces.
        let pieces = vec![("\\".into(), Some("".into())), ("|".into(), None), ("&#124;".into(), Some("|".into()))];
        assert_eq!(split("\\|&#124;", "||", true), pieces);
        assert!(explains("\\|&#124;", "||", &Entities::default(), true));
        assert!(explains("\\|&verbar;x", "||x", &Entities::default(), true));
    }

    #[test]
    fn only_a_leaf_whose_pipes_comrak_unescapes_hides_a_backslash() {
        assert!(explains("&amp;\\|", "&|", &Entities::default(), true));
        assert!(!explains("&amp;\\|", "&|", &Entities::default(), false));
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
        assert_eq!(split_pieces("completely", "different words here", &Entities::default(), false), None);
    }

    #[test]
    fn entities_decode_as_comrak_decodes_them() {
        let e = Entities::default();
        for (reference, shown) in [("&amp;", "&"), ("&#35;", "#"), ("&#x1F600;", "😀"), ("&nbsp;", "\u{a0}"), ("&#32;", " "), ("&#10;", "\n"), ("&#0;", "\u{fffd}"), ("&#x110000;", "\u{fffd}"), ("&nope;", "&nope;"), ("&#12345678;", "&#12345678;"), ("&#64;", "@")] {
            assert_eq!(e.decode(reference), shown, "{reference}");
        }
    }

    #[test]
    fn a_range_may_not_cut_an_entity() {
        let src = "a &amp; b";
        assert!(cuts_entity(src, 0, 3), "ends one byte into the reference");
        assert!(!cuts_entity(src, 0, 7));
        assert!(cuts_entity(src, 3, 9), "begins inside the reference");
        assert!(!cuts_entity("\\&amp; b", 2, 8), "an escaped `&` leaves the text `amp;`");
        assert!(!cuts_entity("\\&amp; b", 1, 2), "an escaped `&` is the escape's whole text");
        assert!(cuts_entity("\\&amp; b", 1, 3), "no text holds an escaped `&` and what follows it");
        assert!(cuts_entity("\\\\&amp; b", 3, 9), "an escaped backslash does not escape the `&`");
        assert!(cuts_entity("\\\\&amp; b", 2, 3), "nor does it end a text at the `&`");
    }

    #[test]
    fn adjacent_entities_each_display_their_own_text() {
        assert_eq!(
            shown("a&amp;&amp;b", "a&&b"),
            vec![
                ("a".into(), None),
                ("&amp;".into(), Some("&".into())),
                ("&amp;".into(), Some("&".into())),
                ("b".into(), None),
            ]
        );
        assert_eq!(
            shown("&lt;&gt;", "<>"),
            vec![("&lt;".into(), Some("<".into())), ("&gt;".into(), Some(">".into()))]
        );
        assert_eq!(
            shown("a&#10;&#10;b", "a\n\nb"),
            vec![
                ("a".into(), None),
                ("&#10;".into(), Some("\n".into())),
                ("&#10;".into(), Some("\n".into())),
                ("b".into(), None),
            ]
        );
        // A reference CommonMark does not decode is told apart by comrak's
        // own decoding of it: it stays exact text, and the next reference
        // displays its own decoding.
        assert_eq!(shown("&nope;&amp;", "&nope;&"), vec![("&nope;".into(), None), ("&amp;".into(), Some("&".into()))]);
        assert_eq!(
            shown("&amp;amp;", "&amp;"),
            vec![("&amp;".into(), Some("&".into())), ("amp;".into(), None)]
        );
    }
}

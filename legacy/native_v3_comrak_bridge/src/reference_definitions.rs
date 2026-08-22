//! Reference-definition ranges, derived in the bridge (RFC 022 Phase 2b).
//!
//! Comrak consumes link reference definitions during the block phase and
//! exposes neither nodes nor sourcepos for them, so their spans cannot be
//! read off the AST. This module classifies definition-shaped line runs
//! textually — the one sanctioned derivation the parser's API forces — and
//! then VALIDATES each candidate against comrak's own verdict: a real
//! definition was consumed and therefore no emitted block covers its lines,
//! while a definition-shaped line comrak kept as content (inside a code
//! fence, lazily continuing a paragraph) is covered by a block and rejected.
//! Classification proposes; the parser's block output decides.

use crate::payload::{JsonBlock, JsonRange};

/// Byte ranges (line runs, trailing newline included) of link reference
/// definitions that comrak consumed. Footnote-shaped definitions
/// (`[^label]: …`) are excluded: flark keeps unsupported GitHub footnote
/// syntax source-visible.
pub(crate) fn collect_reference_definition_ranges(
    text: &str,
    blocks: &[JsonBlock],
) -> Vec<JsonRange> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    while start <= bytes.len() {
        let end = match bytes[start..].iter().position(|byte| *byte == b'\n') {
            Some(offset) => start + offset,
            None => bytes.len(),
        };
        let end_with_break = if end < bytes.len() { end + 1 } else { end };
        lines.push((start, end, end_with_break));
        if end >= bytes.len() {
            break;
        }
        start = end + 1;
    }

    let mut ranges = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let (line_start, line_end, line_break_end) = lines[index];
        let line = &bytes[line_start..line_end];
        if !is_reference_definition_line(line) {
            index += 1;
            continue;
        }
        let mut range_end = line_break_end;
        // CommonMark lets the title sit alone on the following line; comrak
        // folds it into the definition, so consume it too or it renders as a
        // stray visible line. Rarer wrapped shapes stay visible.
        if !definition_line_has_title(line) && index + 1 < lines.len() {
            let (next_start, next_end, next_break_end) = lines[index + 1];
            if is_standalone_title_line(&bytes[next_start..next_end]) {
                range_end = next_break_end;
                index += 1;
            }
        }
        let candidate = JsonRange {
            start_byte: line_start as u32,
            end_byte: range_end as u32,
        };
        if !covered_by_any_block(&candidate, blocks) {
            ranges.push(candidate);
        }
        index += 1;
    }
    ranges
}

/// The parser's verdict: a consumed definition produces no block over its
/// lines, so any intersecting block token refutes the classification.
///
/// Pure list CONTAINER spans are exempt: their line-expanded range brackets
/// every line between the first and last item — including a consumed
/// definition sandwiched between items — so counting them would refute real
/// definitions. `list_item` spans still refute: a definition-shaped line
/// inside a fenced code block inside an item is covered only by the item
/// (nested code blocks are suppressed from the block vec), and it must stay
/// visible as code.
fn covered_by_any_block(range: &JsonRange, blocks: &[JsonBlock]) -> bool {
    blocks.iter().any(|block| {
        !matches!(block.kind, "unordered_list" | "ordered_list")
            && block.start_byte < range.end_byte
            && block.end_byte > range.start_byte
    })
}

/// `[label]: destination…` with up to three leading spaces/tabs, a
/// non-empty single-line label, and a non-space destination present.
/// Footnote-shaped labels (`[^…]`) are excluded.
fn is_reference_definition_line(line: &[u8]) -> bool {
    let Some(after_indent) = skip_indent(line) else {
        return false;
    };
    let rest = &line[after_indent..];
    if rest.first() != Some(&b'[') {
        return false;
    }
    if rest.get(1) == Some(&b'^') {
        return false;
    }
    let Some(label_len) = label_length(&rest[1..]) else {
        return false;
    };
    let mut cursor = 1 + label_len + 1; // past `[label]`
    if rest.get(cursor) != Some(&b':') {
        return false;
    }
    cursor += 1;
    while matches!(rest.get(cursor), Some(b' ') | Some(b'\t')) {
        cursor += 1;
    }
    matches!(rest.get(cursor), Some(byte) if !byte.is_ascii_whitespace())
}

/// Whether the opening line already carries its title: a destination token
/// (bare, or `<…>` which may contain spaces) followed by whitespace and a
/// title opener (`"`, `'`, `(`).
fn definition_line_has_title(line: &[u8]) -> bool {
    let Some(after_indent) = skip_indent(line) else {
        return false;
    };
    let rest = &line[after_indent..];
    if rest.first() != Some(&b'[') {
        return false;
    }
    let Some(label_len) = label_length(&rest[1..]) else {
        return false;
    };
    let mut cursor = 1 + label_len + 1;
    if rest.get(cursor) != Some(&b':') {
        return false;
    }
    cursor += 1;
    while matches!(rest.get(cursor), Some(b' ') | Some(b'\t')) {
        cursor += 1;
    }
    // Destination: angle form may contain spaces; bare form is one \S+ run.
    match rest.get(cursor) {
        Some(b'<') => {
            cursor += 1;
            while let Some(byte) = rest.get(cursor) {
                if *byte == b'>' {
                    break;
                }
                if *byte == b'\n' {
                    return false;
                }
                cursor += 1;
            }
            if rest.get(cursor) != Some(&b'>') {
                return false;
            }
            cursor += 1;
        }
        Some(byte) if !byte.is_ascii_whitespace() => {
            while matches!(rest.get(cursor), Some(byte) if !byte.is_ascii_whitespace()) {
                cursor += 1;
            }
        }
        _ => return false,
    }
    let mut saw_gap = false;
    while matches!(rest.get(cursor), Some(b' ') | Some(b'\t')) {
        saw_gap = true;
        cursor += 1;
    }
    saw_gap && matches!(rest.get(cursor), Some(b'"') | Some(b'\'') | Some(b'('))
}

/// Whether the whole line is a single title token (`"…"`, `'…'`, `(…)`)
/// surrounded only by spaces/tabs — the shape of a title wrapped onto the
/// line after the destination.
fn is_standalone_title_line(line: &[u8]) -> bool {
    let mut cursor = 0usize;
    while matches!(line.get(cursor), Some(b' ') | Some(b'\t')) {
        cursor += 1;
    }
    let closer = match line.get(cursor) {
        Some(b'"') => b'"',
        Some(b'\'') => b'\'',
        Some(b'(') => b')',
        _ => return false,
    };
    cursor += 1;
    while let Some(byte) = line.get(cursor) {
        if *byte == closer {
            cursor += 1;
            while matches!(line.get(cursor), Some(b' ') | Some(b'\t')) {
                cursor += 1;
            }
            return cursor == line.len();
        }
        cursor += 1;
    }
    false
}

/// Up to three leading spaces/tabs (mirrors the pre-existing Dart
/// classification exactly); more indentation is an indented code block.
fn skip_indent(line: &[u8]) -> Option<usize> {
    let mut cursor = 0usize;
    while cursor < 3 && matches!(line.get(cursor), Some(b' ') | Some(b'\t')) {
        cursor += 1;
    }
    Some(cursor)
}

/// Length of a non-empty label run terminated by `]`, containing neither
/// `]` nor a newline. Returns None when no such label exists.
fn label_length(rest: &[u8]) -> Option<usize> {
    let mut length = 0usize;
    for byte in rest {
        match byte {
            b']' => return if length == 0 { None } else { Some(length) },
            b'\n' => return None,
            _ => length += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::parser::parse_to_payload;

    fn def_ranges(md: &str) -> Vec<(u64, u64)> {
        let payload = parse_to_payload(md, 1).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        match json["referenceDefinitionRanges"].as_array() {
            None => Vec::new(),
            Some(ranges) => ranges
                .iter()
                .map(|range| {
                    (
                        range["startByte"].as_u64().unwrap(),
                        range["endByte"].as_u64().unwrap(),
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn basic_definition_is_collected() {
        assert_eq!(def_ranges("[foo]: /url\n\nSee [foo].\n"), vec![(0, 12)]);
    }

    #[test]
    fn inline_title_definition_is_one_line() {
        assert_eq!(def_ranges("[foo]: /url \"title\"\n\nx\n"), vec![(0, 20)],);
    }

    #[test]
    fn next_line_title_is_folded() {
        assert_eq!(def_ranges("[foo]: /url\n\"title\"\n\nx\n"), vec![(0, 20)],);
    }

    #[test]
    fn angle_destination_with_space_is_refuted_by_comrak() {
        // The classifier proposes this shape, but comrak (cmark-gfm lineage)
        // does not accept a space inside an angle destination for a
        // definition — the line stays a paragraph, and the block-coverage
        // check refutes the candidate. The pre-bridge Dart scanner would
        // have hidden a line the parser kept as content; the validation is
        // the point of deriving this in the bridge.
        assert_eq!(def_ranges("[foo]: <a b> \"t\"\n\"decoy\"\n\nx\n"), vec![]);
    }

    #[test]
    fn footnote_definition_stays_source_visible() {
        assert_eq!(def_ranges("[^1]: note\n\nx\n"), vec![]);
    }

    #[test]
    fn definition_shape_inside_code_fence_is_refuted_by_blocks() {
        assert_eq!(def_ranges("```\n[foo]: /url\n```\n"), vec![]);
    }

    #[test]
    fn definition_shape_continuing_a_paragraph_is_refuted_by_blocks() {
        // A definition cannot interrupt a paragraph; comrak keeps the line as
        // paragraph content, and the covering block refutes the candidate.
        assert_eq!(def_ranges("text\n[foo]: /url\n\nx\n"), vec![]);
    }

    #[test]
    fn consecutive_definitions_collect_individually() {
        assert_eq!(def_ranges("[a]: /a\n[b]: /b\n\nx\n"), vec![(0, 8), (8, 16)],);
    }

    /// A consumed definition sandwiched between list items is bracketed by
    /// the list CONTAINER's line-expanded span but covered by no item or
    /// leaf block; container spans must not refute it (regression pin).
    #[test]
    fn definition_between_list_items_is_still_collected() {
        assert_eq!(
            def_ranges("- a\n\n  [foo]: /url\n\n- b\n\nsee [foo]\n"),
            vec![(5, 19)],
        );
    }

    /// A definition-shaped line inside a fenced code block inside a list
    /// item is covered by the ITEM span (the nested code block is suppressed
    /// from the block vec) and must stay refuted — visible as code.
    #[test]
    fn definition_shape_in_code_inside_list_item_stays_refuted() {
        assert_eq!(def_ranges("- a\n\n  ```\n  [foo]: /url\n  ```\n"), vec![]);
    }

    /// An error payload from a CURRENT artifact must not read as stale.
    #[test]
    fn diagnostic_payload_carries_the_current_protocol_version() {
        let payload = crate::payload::diagnostic_payload("boom");
        let json: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(
            json["protocolVersion"].as_u64(),
            Some(crate::payload::PAYLOAD_PROTOCOL_VERSION as u64),
        );
    }
}

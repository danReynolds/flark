//! Bounded parser-owned facts for the first live-edit command profile.
//!
//! This is not a second Markdown parser. It exposes the result of the same
//! segmented Comrak-donor scan used by the block controller, narrowed to the
//! top-level plain/list cases that `flark-edit-v1` can prove locally.

use std::ops::Range;

use crate::{
    block_core::{BulletMarker, ListDelimiter},
    segmented_lexical::{SegmentedLineScanner, SegmentedListMarker},
    M11LineEnding,
};

/// The maximum exact line prefix an interactive resolver may inspect.
///
/// This matches the source runtime's bounded cursor window. A caller may pass
/// a prefix of a longer line: all supported opener decisions are already
/// final once this classifier returns a supported case.
pub const M11_SIMPLE_EDIT_LINE_MAX_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11SimpleEditListMarker {
    Bullet(BulletMarker),
    Ordered {
        value: u32,
        delimiter: ListDelimiter,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum M11SimpleEditLineKind {
    Plain,
    ListItem {
        marker: M11SimpleEditListMarker,
        prefix: Range<usize>,
        content: Range<usize>,
        marker_offset: u8,
        task_checked: Option<bool>,
        empty: bool,
    },
    AtxHeading {
        prefix: Range<usize>,
        content: Range<usize>,
        empty: bool,
    },
    BlockQuote {
        prefix: Range<usize>,
        content: Range<usize>,
        empty: bool,
    },
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11SimpleEditLine {
    pub kind: M11SimpleEditLineKind,
    pub ending: M11LineEnding,
    pub content_end: usize,
}

/// Classifies one exact physical line, or a bounded prefix of a longer line.
///
/// `strip_bom` is true only for the first physical line. Invalid UTF-8,
/// multiple physical lines, contextual constructs, and grammar failures all
/// fail closed as `Unsupported`.
#[must_use]
pub fn classify_m11_simple_edit_line(source: &[u8], strip_bom: bool) -> M11SimpleEditLine {
    let (content_end, ending) = physical_line_ending(source);
    if source.len() > M11_SIMPLE_EDIT_LINE_MAX_BYTES
        || std::str::from_utf8(source).is_err()
        || source[..content_end]
            .iter()
            .any(|byte| matches!(byte, b'\r' | b'\n'))
    {
        return unsupported(ending, content_end);
    }

    let mut scanner = SegmentedLineScanner::new(strip_bom);
    for byte in source {
        scanner.push(*byte);
    }
    let Ok(facts) = scanner.finish() else {
        return unsupported(ending, content_end);
    };

    if facts.indent < 4 {
        if let Some(item) = facts.list_item {
            let child = item.child;
            let simple_child = !child.block_quote
                && !child.atx_heading
                && !child.fence
                && !child.html_block_1_to_6
                && !child.html_block_7
                && !child.setext
                && !child.thematic_break
                && !child.list
                && !child.table_delimiter_candidate
                && (!child.potential_reference_definition || child.task);
            if item.opening_indent <= 3 && !item.tab_padded && simple_child {
                let marker = match item.marker {
                    SegmentedListMarker::Bullet(byte) => {
                        M11SimpleEditListMarker::Bullet(match byte {
                            b'-' => BulletMarker::Hyphen,
                            b'+' => BulletMarker::Plus,
                            b'*' => BulletMarker::Asterisk,
                            _ => return unsupported(ending, content_end),
                        })
                    }
                    SegmentedListMarker::Ordered { start, delimiter } => {
                        let Ok(value) = u32::try_from(start) else {
                            return unsupported(ending, content_end);
                        };
                        let delimiter = match delimiter {
                            b'.' => ListDelimiter::Period,
                            b')' => ListDelimiter::Parenthesis,
                            _ => return unsupported(ending, content_end),
                        };
                        M11SimpleEditListMarker::Ordered { value, delimiter }
                    }
                };
                let (prefix_end, content_start, task_checked) = if child.task {
                    let task_start = item.content.start;
                    let Some(marker) = source.get(task_start..task_start.saturating_add(3)) else {
                        return unsupported(ending, content_end);
                    };
                    let checked = match marker {
                        [b'[', b' ', b']'] => false,
                        [b'[', b'x' | b'X', b']'] => true,
                        _ => return unsupported(ending, content_end),
                    };
                    let marker_end = task_start + 3;
                    let task_end = if source
                        .get(marker_end)
                        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
                    {
                        marker_end + 1
                    } else {
                        marker_end
                    };
                    (task_end, task_end, Some(checked))
                } else {
                    (item.hidden_prefix.end, item.content.start, None)
                };
                let empty = if task_checked.is_some() {
                    source[content_start..item.content.end]
                        .iter()
                        .all(|byte| matches!(byte, b' ' | b'\t'))
                } else {
                    item.empty
                };
                return M11SimpleEditLine {
                    kind: M11SimpleEditLineKind::ListItem {
                        marker,
                        prefix: item.hidden_prefix.start..prefix_end,
                        content: content_start..item.content.end,
                        marker_offset: u8::try_from(item.opening_indent).unwrap_or(u8::MAX),
                        task_checked,
                        empty,
                    },
                    ending,
                    content_end,
                };
            }
        }

        if let Some(heading) = facts.atx_heading {
            let empty = source[heading.content.start..heading.content.end]
                .iter()
                .all(|byte| matches!(byte, b' ' | b'\t'));
            return M11SimpleEditLine {
                kind: M11SimpleEditLineKind::AtxHeading {
                    prefix: heading.opening_marker.start..heading.content.start,
                    content: heading.content.start..heading.content.end,
                    empty,
                },
                ending,
                content_end,
            };
        }

        if let Some(quote) = facts.block_quote_source {
            let residual = quote.residual;
            let simple_child = !residual.block_quote
                && !residual.atx_heading
                && !residual.fence
                && !residual.html_block_1_to_6
                && !residual.html_block_7
                && !residual.setext
                && !residual.thematic_break
                && !residual.indented_code
                && !residual.list
                && !residual.interrupting_list
                && !residual.table_delimiter_candidate
                && !residual.potential_reference_definition;
            if simple_child {
                let content = quote.content.start..quote.line_ending.start;
                let empty = residual.blank
                    || source[content.clone()]
                        .iter()
                        .all(|byte| matches!(byte, b' ' | b'\t'));
                return M11SimpleEditLine {
                    kind: M11SimpleEditLineKind::BlockQuote {
                        prefix: quote.hidden_prefix.start..quote.hidden_prefix.end,
                        content,
                        empty,
                    },
                    ending,
                    content_end,
                };
            }
        }
    }

    let starts_contextual_block = facts.blank
        || facts.indent >= 4
        || facts.block_quote
        || facts.atx_heading.is_some()
        || facts.fence.opener_valid
        || facts.html_block_1_to_6.is_some()
        || facts.html_block_7
        || facts.setext.is_some()
        || facts.thematic_break.is_some()
        || facts.list
        || facts.table_delimiter_candidate
        || facts.first_significant_byte == Some(b'[');
    M11SimpleEditLine {
        kind: if starts_contextual_block {
            M11SimpleEditLineKind::Unsupported
        } else {
            M11SimpleEditLineKind::Plain
        },
        ending,
        content_end,
    }
}

fn physical_line_ending(source: &[u8]) -> (usize, M11LineEnding) {
    if source.ends_with(b"\r\n") {
        (source.len() - 2, M11LineEnding::CrLf)
    } else if source.ends_with(b"\n") {
        (source.len() - 1, M11LineEnding::Lf)
    } else if source.ends_with(b"\r") {
        (source.len() - 1, M11LineEnding::Cr)
    } else {
        (source.len(), M11LineEnding::Eof)
    }
}

fn unsupported(ending: M11LineEnding, content_end: usize) -> M11SimpleEditLine {
    M11SimpleEditLine {
        kind: M11SimpleEditLineKind::Unsupported,
        ending,
        content_end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_donor_list_rules_for_the_e1_subset() {
        let cases = [
            ("- alpha\n", false),
            ("  + alpha\r\n", false),
            ("9) alpha", false),
            ("42. \n", true),
            ("- [ ] task\n", false),
            ("- [x] \n", true),
            ("- [ ]   \n", true),
        ];
        for (source, empty) in cases {
            let line = classify_m11_simple_edit_line(source.as_bytes(), false);
            let M11SimpleEditLineKind::ListItem { empty: actual, .. } = line.kind else {
                panic!("expected simple List Item for {source:?}: {line:?}");
            };
            assert_eq!(actual, empty, "{source:?}");
        }
    }

    #[test]
    fn contextual_and_complex_constructs_fail_closed() {
        for source in [
            "> > nested\n",
            "> # child heading\n",
            "- # child heading\n",
            "```\n",
            "---\n",
            "[label]: /url\n",
            "| --- |\n",
            "    code\n",
        ] {
            assert_eq!(
                classify_m11_simple_edit_line(source.as_bytes(), false).kind,
                M11SimpleEditLineKind::Unsupported,
                "{source:?}"
            );
        }
    }

    #[test]
    fn exposes_atx_and_simple_quote_prefix_geometry() {
        assert_eq!(
            classify_m11_simple_edit_line(b"  ## heading\n", false).kind,
            M11SimpleEditLineKind::AtxHeading {
                prefix: 2..5,
                content: 5..12,
                empty: false,
            }
        );
        assert_eq!(
            classify_m11_simple_edit_line(b"> quote\n", false).kind,
            M11SimpleEditLineKind::BlockQuote {
                prefix: 0..2,
                content: 2..7,
                empty: false,
            }
        );
        assert!(matches!(
            classify_m11_simple_edit_line(b"> \n", false).kind,
            M11SimpleEditLineKind::BlockQuote { empty: true, .. }
        ));
    }

    #[test]
    fn ordinary_text_and_line_endings_remain_exact() {
        for (source, ending) in [
            ("alpha\n", M11LineEnding::Lf),
            ("alpha\r\n", M11LineEnding::CrLf),
            ("alpha\r", M11LineEnding::Cr),
            ("alpha", M11LineEnding::Eof),
        ] {
            let line = classify_m11_simple_edit_line(source.as_bytes(), false);
            assert_eq!(line.kind, M11SimpleEditLineKind::Plain, "{source:?}");
            assert_eq!(line.ending, ending, "{source:?}");
        }
    }
}

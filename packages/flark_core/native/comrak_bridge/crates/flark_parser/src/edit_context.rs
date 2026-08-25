//! Bounded parser-owned facts for the first live-edit command profile.
//!
//! This is not a second Markdown parser. It exposes the result of the same
//! segmented Comrak-donor scan used by the block controller, narrowed to the
//! bounded top-level cases that `flark-edit-v1` can prove locally.

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
    Blank,
    ListItem {
        marker: M11SimpleEditListMarker,
        prefix: Range<usize>,
        content: Range<usize>,
        marker_offset: u8,
        item_padding: u8,
        task_checked: Option<bool>,
        empty: bool,
    },
    AtxHeading {
        level: u8,
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

/// Parser-authored block presentation produced by one exact bounded edit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11SimpleBlockTransitionPresentation {
    Plain,
    AtxHeading { level: u8 },
    BlockQuote { depth: u8 },
    ListItem { marker: M11SimpleEditListMarker },
}

/// One pre-edit proof that an exact scalar insertion or deletion changes only
/// the current physical line's block shell in the declared way.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11SimpleBlockTransition {
    affected: Range<usize>,
    trigger: Range<usize>,
    replacement: Option<char>,
    result_prefix_utf16_len: u8,
    presentation: M11SimpleBlockTransitionPresentation,
}

/// One parser-authored, bounded prefix sequence that constructs a supported
/// simple block shell from a Plain physical line.
///
/// The host receives the exact sequence and the point at which the donor
/// classifier says the result shell becomes active. It does not infer marker
/// grammar from source text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11SimpleBlockPrefixPlan {
    affected: Range<usize>,
    prefix: Vec<u8>,
    activation_prefix_utf16_len: u8,
    result_prefix_utf16_len: u8,
    presentation: M11SimpleBlockTransitionPresentation,
}

impl M11SimpleBlockPrefixPlan {
    #[must_use]
    pub fn affected(&self) -> Range<usize> {
        self.affected.clone()
    }

    #[must_use]
    pub fn prefix(&self) -> &[u8] {
        &self.prefix
    }

    #[must_use]
    pub const fn activation_prefix_utf16_len(&self) -> u8 {
        self.activation_prefix_utf16_len
    }

    #[must_use]
    pub const fn result_prefix_utf16_len(&self) -> u8 {
        self.result_prefix_utf16_len
    }

    #[must_use]
    pub const fn presentation(&self) -> M11SimpleBlockTransitionPresentation {
        self.presentation
    }
}

impl M11SimpleBlockTransition {
    #[must_use]
    pub fn affected(&self) -> Range<usize> {
        self.affected.clone()
    }

    #[must_use]
    pub fn trigger(&self) -> Range<usize> {
        self.trigger.clone()
    }

    #[must_use]
    pub const fn replacement(&self) -> Option<char> {
        self.replacement
    }

    #[must_use]
    pub const fn result_prefix_utf16_len(&self) -> u8 {
        self.result_prefix_utf16_len
    }

    #[must_use]
    pub const fn presentation(&self) -> M11SimpleBlockTransitionPresentation {
        self.presentation
    }
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
                let item_padding = item
                    .continuation_prefix
                    .end
                    .saturating_sub(item.opening_indent);
                let Ok(item_padding) = u8::try_from(item_padding) else {
                    return unsupported(ending, content_end);
                };
                if !(2..=14).contains(&item_padding) {
                    return unsupported(ending, content_end);
                }
                return M11SimpleEditLine {
                    kind: M11SimpleEditLineKind::ListItem {
                        marker,
                        prefix: item.hidden_prefix.start..prefix_end,
                        content: content_start..item.content.end,
                        marker_offset: u8::try_from(item.opening_indent).unwrap_or(u8::MAX),
                        item_padding,
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
                    level: heading.level,
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

    let starts_contextual_block = facts.indent >= 4
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
        kind: if facts.blank {
            M11SimpleEditLineKind::Blank
        } else if starts_contextual_block {
            M11SimpleEditLineKind::Unsupported
        } else {
            M11SimpleEditLineKind::Plain
        },
        ending,
        content_end,
    }
}

/// Derives exact one-scalar block-shell transitions for a supported simple
/// physical line.
///
/// The bounded donor-backed classifier is run on each possible opener point,
/// so this function does not maintain a second Markdown marker table. Only a
/// top-level prefix replacement whose edited line has one supported simple
/// shell is published. Hosts receive the result classification, never enough
/// source grammar to infer a different one.
#[must_use]
pub fn derive_m11_simple_block_transitions(
    source: &[u8],
    strip_bom: bool,
) -> Vec<M11SimpleBlockTransition> {
    let base = classify_m11_simple_edit_line(source, strip_bom);
    let Some((base_presentation, base_prefix_end)) =
        simple_block_transition_presentation(&base.kind)
    else {
        return Vec::new();
    };
    if base.content_end == 0 || base.content_end > M11_SIMPLE_EDIT_LINE_MAX_BYTES {
        return Vec::new();
    }
    let Ok(text) = std::str::from_utf8(source) else {
        return Vec::new();
    };
    let search_end = base.content_end.min(14);
    let mut transitions = Vec::new();
    let mut candidates = Vec::new();
    for trigger in 0..=search_end {
        if !text.is_char_boundary(trigger) {
            continue;
        }
        candidates.push((trigger..trigger, Some(' ')));
    }
    if text.is_char_boundary(0) {
        candidates.push((0..0, Some('>')));
    }
    for (trigger, character) in text
        .char_indices()
        .take_while(|(start, _)| *start < search_end)
    {
        candidates.push((trigger..trigger + character.len_utf8(), None));
    }
    for (trigger, replacement) in candidates {
        let mut edited =
            Vec::with_capacity(source.len() + usize::from(replacement.is_some()) - trigger.len());
        edited.extend_from_slice(&source[..trigger.start]);
        if let Some(replacement) = replacement {
            let mut encoded = [0_u8; 4];
            edited.extend_from_slice(replacement.encode_utf8(&mut encoded).as_bytes());
        }
        edited.extend_from_slice(&source[trigger.end..]);
        let result = classify_m11_simple_edit_line(&edited, strip_bom);
        let delta = isize::try_from(edited.len()).unwrap_or(isize::MAX)
            - isize::try_from(source.len()).unwrap_or(isize::MIN);
        if isize::try_from(result.content_end).ok()
            != isize::try_from(base.content_end)
                .ok()
                .and_then(|value| value.checked_add(delta))
        {
            continue;
        }
        let Some((presentation, prefix_end)) = simple_block_transition_presentation(&result.kind)
        else {
            continue;
        };
        let changes_shell = presentation != base_presentation
            && (base_presentation == M11SimpleBlockTransitionPresentation::Plain
                || presentation == M11SimpleBlockTransitionPresentation::Plain);
        let changes_optional_quote_space = matches!(
            (base_presentation, presentation, base_prefix_end, prefix_end),
            (
                M11SimpleBlockTransitionPresentation::BlockQuote { depth: 1 },
                M11SimpleBlockTransitionPresentation::BlockQuote { depth: 1 },
                1,
                2
            ) | (
                M11SimpleBlockTransitionPresentation::BlockQuote { depth: 1 },
                M11SimpleBlockTransitionPresentation::BlockQuote { depth: 1 },
                2,
                1
            )
        );
        if !changes_shell && !changes_optional_quote_space {
            continue;
        }
        let prefix_utf16_len = edited
            .get(..prefix_end)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(|value| value.encode_utf16().count());
        let Some(prefix_utf16_len) = prefix_utf16_len.and_then(|len| u8::try_from(len).ok()) else {
            continue;
        };
        transitions.push(M11SimpleBlockTransition {
            affected: 0..base.content_end,
            trigger,
            replacement,
            result_prefix_utf16_len: prefix_utf16_len,
            presentation,
        });
    }
    transitions
}

/// Derives the finite D0 prefix-construction plans for a supported Plain line.
///
/// Candidate sequences are data owned by the parser layer, and every
/// intermediate and final state is reclassified by the donor-backed block
/// classifier. Any unexpected or contextual state suppresses the plan.
#[must_use]
pub fn derive_m11_simple_block_prefix_plans(
    source: &[u8],
    strip_bom: bool,
) -> Vec<M11SimpleBlockPrefixPlan> {
    let base = classify_m11_simple_edit_line(source, strip_bom);
    if base.kind != M11SimpleEditLineKind::Plain
        || base.content_end == 0
        || base.content_end > M11_SIMPLE_EDIT_LINE_MAX_BYTES
    {
        return Vec::new();
    }

    const CANDIDATES: [&[u8]; 4] = [b"# ", b"> ", b"- ", b"1. "];
    let mut plans = Vec::with_capacity(CANDIDATES.len());
    for prefix in CANDIDATES {
        let mut edited = Vec::with_capacity(source.len() + prefix.len());
        edited.extend_from_slice(prefix);
        edited.extend_from_slice(source);
        let result = classify_m11_simple_edit_line(&edited, strip_bom);
        if result.content_end != base.content_end + prefix.len() {
            continue;
        }
        let Some((presentation, result_prefix_end)) =
            simple_block_transition_presentation(&result.kind)
        else {
            continue;
        };
        if presentation == M11SimpleBlockTransitionPresentation::Plain
            || result_prefix_end != prefix.len()
        {
            continue;
        }

        let mut activation = None;
        let mut valid = true;
        for step in 1..=prefix.len() {
            let mut intermediate = Vec::with_capacity(source.len() + step);
            intermediate.extend_from_slice(&prefix[..step]);
            intermediate.extend_from_slice(source);
            let classified = classify_m11_simple_edit_line(&intermediate, strip_bom);
            if classified.content_end != base.content_end + step {
                valid = false;
                break;
            }
            let Some((step_presentation, step_prefix_end)) =
                simple_block_transition_presentation(&classified.kind)
            else {
                valid = false;
                break;
            };
            if step_presentation == presentation && step_prefix_end == step {
                activation.get_or_insert(step);
            } else if step_presentation != M11SimpleBlockTransitionPresentation::Plain {
                valid = false;
                break;
            }
        }
        let Some(activation) = activation.filter(|_| valid) else {
            continue;
        };
        let (Ok(activation_prefix_utf16_len), Ok(result_prefix_utf16_len)) =
            (u8::try_from(activation), u8::try_from(result_prefix_end))
        else {
            continue;
        };
        plans.push(M11SimpleBlockPrefixPlan {
            affected: 0..base.content_end,
            prefix: prefix.to_vec(),
            activation_prefix_utf16_len,
            result_prefix_utf16_len,
            presentation,
        });
    }
    plans
}

fn simple_block_transition_presentation(
    kind: &M11SimpleEditLineKind,
) -> Option<(M11SimpleBlockTransitionPresentation, usize)> {
    match kind {
        M11SimpleEditLineKind::Plain => Some((M11SimpleBlockTransitionPresentation::Plain, 0)),
        M11SimpleEditLineKind::Blank => None,
        M11SimpleEditLineKind::AtxHeading {
            level,
            prefix,
            content,
            ..
        } if prefix.start == 0 && prefix.end == content.start => Some((
            M11SimpleBlockTransitionPresentation::AtxHeading { level: *level },
            prefix.end,
        )),
        M11SimpleEditLineKind::BlockQuote {
            prefix, content, ..
        } if prefix.start == 0 && prefix.end == content.start => Some((
            M11SimpleBlockTransitionPresentation::BlockQuote { depth: 1 },
            prefix.end,
        )),
        M11SimpleEditLineKind::ListItem {
            marker,
            prefix,
            content,
            marker_offset: 0,
            task_checked: None,
            ..
        } if prefix.start == 0 && prefix.end == content.start => Some((
            M11SimpleBlockTransitionPresentation::ListItem { marker: *marker },
            prefix.end,
        )),
        _ => None,
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
                level: 2,
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
    fn derives_exact_simple_block_shell_transitions_from_plain_lines() {
        let cases = [
            (
                "#change\n",
                1,
                2,
                M11SimpleBlockTransitionPresentation::AtxHeading { level: 1 },
            ),
            (
                ">change\n",
                1,
                2,
                M11SimpleBlockTransitionPresentation::BlockQuote { depth: 1 },
            ),
            (
                "-change\n",
                1,
                2,
                M11SimpleBlockTransitionPresentation::ListItem {
                    marker: M11SimpleEditListMarker::Bullet(BulletMarker::Hyphen),
                },
            ),
            (
                "1.change\n",
                2,
                3,
                M11SimpleBlockTransitionPresentation::ListItem {
                    marker: M11SimpleEditListMarker::Ordered {
                        value: 1,
                        delimiter: ListDelimiter::Period,
                    },
                },
            ),
        ];
        for (source, trigger, prefix_len, presentation) in cases {
            let transitions = derive_m11_simple_block_transitions(source.as_bytes(), false);
            assert!(
                transitions.contains(&M11SimpleBlockTransition {
                    affected: 0..source.len() - 1,
                    trigger: trigger..trigger,
                    replacement: Some(' '),
                    result_prefix_utf16_len: prefix_len,
                    presentation,
                }),
                "{source:?}",
            );
        }
        assert!(
            derive_m11_simple_block_transitions(b"change\n", false).contains(
                &M11SimpleBlockTransition {
                    affected: 0..6,
                    trigger: 0..0,
                    replacement: Some('>'),
                    result_prefix_utf16_len: 1,
                    presentation: M11SimpleBlockTransitionPresentation::BlockQuote { depth: 1 },
                },
            )
        );
    }

    #[test]
    fn derives_donor_checked_simple_block_prefix_plans() {
        let plans = derive_m11_simple_block_prefix_plans(b"change\n", false);
        let expected = [
            (
                b"# ".as_slice(),
                2,
                2,
                M11SimpleBlockTransitionPresentation::AtxHeading { level: 1 },
            ),
            (
                b"> ".as_slice(),
                1,
                2,
                M11SimpleBlockTransitionPresentation::BlockQuote { depth: 1 },
            ),
            (
                b"- ".as_slice(),
                2,
                2,
                M11SimpleBlockTransitionPresentation::ListItem {
                    marker: M11SimpleEditListMarker::Bullet(BulletMarker::Hyphen),
                },
            ),
            (
                b"1. ".as_slice(),
                3,
                3,
                M11SimpleBlockTransitionPresentation::ListItem {
                    marker: M11SimpleEditListMarker::Ordered {
                        value: 1,
                        delimiter: ListDelimiter::Period,
                    },
                },
            ),
        ];
        for (prefix, activation, result_prefix, presentation) in expected {
            assert!(
                plans.iter().any(|plan| {
                    plan.affected == (0..6)
                        && plan.prefix == prefix
                        && usize::from(plan.activation_prefix_utf16_len) == activation
                        && usize::from(plan.result_prefix_utf16_len) == result_prefix
                        && plan.presentation == presentation
                }),
                "missing plan for {prefix:?}: {plans:?}",
            );
        }
    }

    #[test]
    fn simple_block_transition_proof_fails_closed_outside_plain_bounded_rows() {
        for source in ["---\n", "```code\n"] {
            assert!(
                derive_m11_simple_block_transitions(source.as_bytes(), false).is_empty(),
                "{source:?}",
            );
        }
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

    #[test]
    fn blank_lines_are_explicit_parser_facts() {
        for source in ["", "\n", "\r\n", "  \n"] {
            let line = classify_m11_simple_edit_line(source.as_bytes(), false);
            assert_eq!(line.kind, M11SimpleEditLineKind::Blank, "{source:?}");
        }
    }
}

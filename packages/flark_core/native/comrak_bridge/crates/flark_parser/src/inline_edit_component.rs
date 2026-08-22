//! Parser-authored dependency components for pending inline presentation.
//!
//! These records carry only grammar proof already established by the inline
//! parser. Hosts may match the declared edit predicate and transform the
//! declared ranges, but they must not infer additional Markdown semantics.

use std::ops::Range;

use flark_engine::parser_internal::{M11InlineProjectionFact, M11InlineProjectionKind};

pub const M11_INLINE_EDIT_COMPONENTS_MAX: usize = 128;
pub const M11_INLINE_EDIT_COMPONENT_SOURCE_MAX_BYTES: usize = 4 * 1024;

const M11_GUARDED_PROSE_EXACT_SCALARS: [char; 13] = [
    '.', ',', ';', ':', '!', '?', '\'', '"', '(', ')', '-', '–', '—',
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11InlineEditComponentMatcher {
    /// One non-empty ASCII-alphanumeric/space replacement wholly inside a
    /// parser-declared literal trigger. The wire maps this to the existing
    /// guarded ASCII-literal matcher; Core never derives the trigger itself.
    AsciiProseSplice,
    InsertExactScalarAtPoint {
        scalar: char,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11InlineEditComponent {
    affected: Range<u32>,
    trigger: Range<u32>,
    matcher: M11InlineEditComponentMatcher,
}

impl M11InlineEditComponent {
    #[must_use]
    pub fn affected(&self) -> Range<u32> {
        self.affected.clone()
    }

    #[must_use]
    pub fn trigger(&self) -> Range<u32> {
        self.trigger.clone()
    }

    #[must_use]
    pub const fn matcher(&self) -> M11InlineEditComponentMatcher {
        self.matcher
    }
}

/// Derives bounded parser-owned inline edit components.
///
/// A newly inserted `[` cannot form direct/reference syntax when the current
/// authoritative leaf has no bracket byte at all. Requiring one sole Strong
/// fact and unchanged ASCII-alphanumeric guards on both sides also keeps the
/// insertion away from delimiter flanking boundaries. The complete Strong
/// source is therefore the affected dependency closure; plain source outside
/// it remains independent. The record is one-shot and grants no successor
/// authority before a fresh parse. Guarded prose components are lower-priority
/// optional authority: bounded syntax components are retained first when the
/// per-leaf record cap is reached.
pub(crate) fn derive_inline_edit_components(
    source: &[u8],
    facts: &[M11InlineProjectionFact],
    exhaustive_bracket_classification: bool,
) -> Vec<M11InlineEditComponent> {
    if source.is_empty() || source.len() > M11_INLINE_EDIT_COMPONENT_SOURCE_MAX_BYTES {
        return Vec::new();
    }

    let prose_components = derive_ascii_prose_components(source, facts);
    let prose_exact_components = derive_guarded_prose_exact_scalar_components(source, facts);
    if !exhaustive_bracket_classification
        || source
            .iter()
            .any(|byte| matches!(byte, b'[' | b']' | b'\r' | b'\n'))
        || facts.len() != 1
        || facts[0].kind() != M11InlineProjectionKind::Strong
    {
        return bounded_components(prose_components, prose_exact_components);
    }

    let fact = facts[0];
    let affected = fact.relative_range();
    let content = fact.relative_content_range();
    let Ok(affected_start) = usize::try_from(affected.start) else {
        return bounded_components(prose_components, prose_exact_components);
    };
    let Ok(affected_end) = usize::try_from(affected.end) else {
        return bounded_components(prose_components, prose_exact_components);
    };
    let Ok(content_start) = usize::try_from(content.start) else {
        return bounded_components(prose_components, prose_exact_components);
    };
    let Ok(content_end) = usize::try_from(content.end) else {
        return bounded_components(prose_components, prose_exact_components);
    };
    if affected_start >= affected_end
        || affected_end > source.len()
        || content_start <= affected_start
        || content_end >= affected_end
        || content_start >= content_end
    {
        return prose_components;
    }

    let mut components = Vec::new();
    for point in content_start.saturating_add(1)..content_end {
        if !source[point - 1].is_ascii_alphanumeric() || !source[point].is_ascii_alphanumeric() {
            continue;
        }
        let Ok(point) = u32::try_from(point) else {
            return bounded_components(prose_components, prose_exact_components);
        };
        components.push(M11InlineEditComponent {
            affected: affected.clone(),
            trigger: point..point,
            matcher: M11InlineEditComponentMatcher::InsertExactScalarAtPoint { scalar: '[' },
        });
        if components.len() == M11_INLINE_EDIT_COMPONENTS_MAX {
            return components;
        }
    }
    components.extend(
        prose_components
            .into_iter()
            .take(M11_INLINE_EDIT_COMPONENTS_MAX - components.len()),
    );
    components.extend(
        prose_exact_components
            .into_iter()
            .take(M11_INLINE_EDIT_COMPONENTS_MAX - components.len()),
    );
    components
}

fn bounded_components(
    prose_components: Vec<M11InlineEditComponent>,
    prose_exact_components: Vec<M11InlineEditComponent>,
) -> Vec<M11InlineEditComponent> {
    prose_components
        .into_iter()
        .chain(prose_exact_components)
        .take(M11_INLINE_EDIT_COMPONENTS_MAX)
        .collect()
}

/// Emits one-shot punctuation authority only at an ASCII-alphanumeric guard
/// pair inside the fact-free prefix before one authoritative Strong fact. The
/// complete prefix is painted exact, so punctuation inside it cannot leak
/// parser dependencies into the retained Strong fact. The scalar vocabulary
/// is parser-owned protocol data; Core compares only the declared scalar.
fn derive_guarded_prose_exact_scalar_components(
    source: &[u8],
    facts: &[M11InlineProjectionFact],
) -> Vec<M11InlineEditComponent> {
    let [fact] = facts else {
        return Vec::new();
    };
    if fact.kind() != M11InlineProjectionKind::Strong {
        return Vec::new();
    }
    let Ok(prefix_end) = usize::try_from(fact.relative_range().start) else {
        return Vec::new();
    };
    if prefix_end < 2 || prefix_end > source.len() {
        return Vec::new();
    }
    let prefix = &source[..prefix_end];
    if prefix
        .iter()
        .any(|byte| !byte.is_ascii_alphanumeric() && *byte != b' ')
    {
        return Vec::new();
    }
    let Ok(affected_end) = u32::try_from(prefix_end) else {
        return Vec::new();
    };
    let mut components = Vec::new();
    'points: for point in 1..prefix_end {
        if !source[point - 1].is_ascii_alphanumeric() || !source[point].is_ascii_alphanumeric() {
            continue;
        }
        let Ok(point) = u32::try_from(point) else {
            return Vec::new();
        };
        for scalar in M11_GUARDED_PROSE_EXACT_SCALARS {
            components.push(M11InlineEditComponent {
                affected: 0..affected_end,
                trigger: point..point,
                matcher: M11InlineEditComponentMatcher::InsertExactScalarAtPoint { scalar },
            });
            if components.len() == M11_INLINE_EDIT_COMPONENTS_MAX {
                break 'points;
            }
        }
    }
    components
}

/// Captures punctuation-bearing plain gaps that the older whole-line literal
/// emitter intentionally rejects. Each affected range is the complete
/// fact-free physical-line gap, while each trigger is one maximal guarded
/// ASCII prose run inside it. Core admits a multiword replacement only
/// strictly inside that trigger, leaving stable alphanumeric guards on both
/// sides. A fragment containing only the older ASCII word/space vocabulary is
/// omitted because the runtime already emits the same authority for it.
fn derive_ascii_prose_components(
    source: &[u8],
    facts: &[M11InlineProjectionFact],
) -> Vec<M11InlineEditComponent> {
    let occupied = facts
        .iter()
        .map(|fact| fact.relative_range())
        .map(|range| {
            let start = usize::try_from(range.start).ok()?;
            let end = usize::try_from(range.end).ok()?;
            (start <= end && end <= source.len()).then_some(start..end)
        })
        .collect::<Option<Vec<_>>>();
    let Some(mut occupied) = occupied else {
        return Vec::new();
    };
    occupied.sort_by_key(|range| (range.start, range.end));
    if occupied.windows(2).any(|pair| pair[1].start < pair[0].end) {
        return Vec::new();
    }

    let mut components = Vec::new();
    let mut gap_start = 0usize;
    for range in occupied
        .into_iter()
        .chain(std::iter::once(source.len()..source.len()))
    {
        if gap_start < range.start {
            append_ascii_prose_gap_components(source, gap_start..range.start, &mut components);
            if components.len() == M11_INLINE_EDIT_COMPONENTS_MAX {
                break;
            }
        }
        gap_start = gap_start.max(range.end);
    }
    components
}

fn append_ascii_prose_gap_components(
    source: &[u8],
    gap: Range<usize>,
    components: &mut Vec<M11InlineEditComponent>,
) {
    let mut line_start = gap.start;
    while line_start < gap.end {
        let line_end = source[line_start..gap.end]
            .iter()
            .position(|byte| matches!(byte, b'\r' | b'\n'))
            .map_or(gap.end, |offset| line_start + offset);
        append_ascii_prose_line_components(source, line_start..line_end, components);
        if components.len() == M11_INLINE_EDIT_COMPONENTS_MAX || line_end == gap.end {
            return;
        }
        line_start = line_end + 1;
        if source[line_end] == b'\r' && line_start < gap.end && source[line_start] == b'\n' {
            line_start += 1;
        }
    }
}

fn append_ascii_prose_line_components(
    source: &[u8],
    line: Range<usize>,
    components: &mut Vec<M11InlineEditComponent>,
) {
    if line.is_empty()
        || source[line.clone()]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b' ')
    {
        return;
    }
    let mut cursor = line.start;
    while cursor < line.end {
        while cursor < line.end && !source[cursor].is_ascii_alphanumeric() && source[cursor] != b' '
        {
            cursor += 1;
        }
        let run_start = cursor;
        while cursor < line.end
            && (source[cursor].is_ascii_alphanumeric() || source[cursor] == b' ')
        {
            cursor += 1;
        }
        let mut literal_start = run_start;
        let mut literal_end = cursor;
        while literal_start < literal_end && source[literal_start] == b' ' {
            literal_start += 1;
        }
        while literal_start < literal_end && source[literal_end - 1] == b' ' {
            literal_end -= 1;
        }
        if literal_end.saturating_sub(literal_start) < 3
            || !source[literal_start].is_ascii_alphanumeric()
            || !source[literal_end - 1].is_ascii_alphanumeric()
        {
            continue;
        }
        let (Ok(affected_start), Ok(affected_end), Ok(trigger_start), Ok(trigger_end)) = (
            u32::try_from(line.start),
            u32::try_from(line.end),
            u32::try_from(literal_start + 1),
            u32::try_from(literal_end - 1),
        ) else {
            return;
        };
        components.push(M11InlineEditComponent {
            affected: affected_start..affected_end,
            trigger: trigger_start..trigger_end,
            matcher: M11InlineEditComponentMatcher::AsciiProseSplice,
        });
        if components.len() == M11_INLINE_EDIT_COMPONENTS_MAX {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strong(source: Range<u32>, content: Range<u32>) -> M11InlineProjectionFact {
        M11InlineProjectionFact::new(M11InlineProjectionKind::Strong, 0, source, content)
            .expect("strong fact")
    }

    #[test]
    fn bracket_component_is_bounded_to_guarded_strong_content() {
        let components =
            derive_inline_edit_components(b"Before **bold** after.", &[strong(7..15, 9..13)], true)
                .into_iter()
                .filter(|component| {
                    matches!(
                        component.matcher(),
                        M11InlineEditComponentMatcher::InsertExactScalarAtPoint { scalar: '[' }
                    )
                })
                .collect::<Vec<_>>();
        assert_eq!(components.len(), 3);
        assert_eq!(components[0].affected(), 7..15);
        assert_eq!(components[0].trigger(), 10..10);
        assert_eq!(
            components[0].matcher(),
            M11InlineEditComponentMatcher::InsertExactScalarAtPoint { scalar: '[' }
        );
    }

    #[test]
    fn punctuation_bearing_plain_gap_gets_a_guarded_prose_trigger() {
        let source = b"This is **bold** editor,\nonly incomplete or temporarily pending syntax.\n";
        let components = derive_inline_edit_components(source, &[strong(8..16, 10..14)], true);
        let phrase_start = source
            .windows(b"temporarily pending".len())
            .position(|window| window == b"temporarily pending")
            .expect("phrase start");
        let phrase_end = phrase_start + b"temporarily pending".len();
        let component = components
            .iter()
            .find(|component| {
                component.matcher() == M11InlineEditComponentMatcher::AsciiProseSplice
                    && u32::try_from(phrase_start).is_ok_and(|start| {
                        component.trigger().start < start
                            && u32::try_from(phrase_end)
                                .is_ok_and(|end| end < component.trigger().end)
                    })
            })
            .expect("guarded phrase component");
        let prose_start = source
            .windows(b"only incomplete".len())
            .position(|window| window == b"only incomplete")
            .expect("prose start");
        let prose_end = source
            .iter()
            .rposition(|byte| *byte == b'.')
            .expect("prose end");
        assert_eq!(
            component.affected(),
            prose_start as u32..prose_end as u32 + 1
        );
    }

    #[test]
    fn guarded_plain_prefix_declares_each_exact_prose_scalar() {
        let source = b"AlphaBeta and **bold**.";
        let point = 5;
        let components = derive_inline_edit_components(source, &[strong(14..22, 16..20)], true);
        let declared = components
            .iter()
            .filter_map(|component| {
                (component.affected() == (0..14) && component.trigger() == (point..point))
                    .then(|| match component.matcher() {
                        M11InlineEditComponentMatcher::InsertExactScalarAtPoint { scalar } => {
                            Some(scalar)
                        }
                        M11InlineEditComponentMatcher::AsciiProseSplice => None,
                    })
                    .flatten()
            })
            .collect::<Vec<_>>();
        assert_eq!(declared, M11_GUARDED_PROSE_EXACT_SCALARS);
    }

    #[test]
    fn bounded_syntax_components_have_priority_at_the_record_cap() {
        let prefix = "abc-".repeat(M11_INLINE_EDIT_COMPONENTS_MAX);
        let source = format!("{prefix}**bold**");
        let strong_start = u32::try_from(prefix.len()).expect("Strong start");
        let components = derive_inline_edit_components(
            source.as_bytes(),
            &[strong(
                strong_start..strong_start + 8,
                strong_start + 2..strong_start + 6,
            )],
            true,
        );
        assert_eq!(components.len(), M11_INLINE_EDIT_COMPONENTS_MAX);
        assert!(components.iter().any(|component| matches!(
            component.matcher(),
            M11InlineEditComponentMatcher::InsertExactScalarAtPoint { scalar: '[' }
        )));
    }

    #[test]
    fn bracket_component_fails_closed_for_dependency_or_nested_fact() {
        for components in [
            derive_inline_edit_components(
                b"Before **bold** after ]",
                &[strong(7..15, 9..13)],
                true,
            ),
            derive_inline_edit_components(
                b"Before **bold** after.",
                &[strong(7..15, 9..13), strong(8..14, 10..12)],
                true,
            ),
            derive_inline_edit_components(
                b"Before **bold** after.",
                &[strong(7..15, 9..13)],
                false,
            ),
            derive_inline_edit_components(
                b"Before **bold**\nafter.",
                &[strong(7..15, 9..13)],
                true,
            ),
        ] {
            assert!(components.iter().all(|component| !matches!(
                component.matcher(),
                M11InlineEditComponentMatcher::InsertExactScalarAtPoint { scalar: '[' }
            )));
        }
    }
}

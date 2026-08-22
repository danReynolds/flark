//! Parser-authored dependency components for pending inline presentation.
//!
//! These records carry only grammar proof already established by the inline
//! parser. Hosts may match the declared edit predicate and transform the
//! declared ranges, but they must not infer additional Markdown semantics.

use std::ops::Range;

use flark_engine::parser_internal::{M11InlineProjectionFact, M11InlineProjectionKind};

pub const M11_INLINE_EDIT_COMPONENTS_MAX: usize = 128;
pub const M11_INLINE_EDIT_COMPONENT_SOURCE_MAX_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11InlineEditComponentMatcher {
    InsertExactScalarAtPoint { scalar: char },
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

/// Derives the first bounded syntax-punctuation component.
///
/// A newly inserted `[` cannot form direct/reference syntax when the current
/// authoritative leaf has no bracket byte at all. Requiring one sole Strong
/// fact and unchanged ASCII-alphanumeric guards on both sides also keeps the
/// insertion away from delimiter flanking boundaries. The complete Strong
/// source is therefore the affected dependency closure; plain source outside
/// it remains independent. The record is one-shot and grants no successor
/// authority before a fresh parse.
pub(crate) fn derive_inline_edit_components(
    source: &[u8],
    facts: &[M11InlineProjectionFact],
    exhaustive_bracket_classification: bool,
) -> Vec<M11InlineEditComponent> {
    if source.is_empty()
        || source.len() > M11_INLINE_EDIT_COMPONENT_SOURCE_MAX_BYTES
        || !exhaustive_bracket_classification
        || source
            .iter()
            .any(|byte| matches!(byte, b'[' | b']' | b'\r' | b'\n'))
        || facts.len() != 1
        || facts[0].kind() != M11InlineProjectionKind::Strong
    {
        return Vec::new();
    }

    let fact = facts[0];
    let affected = fact.relative_range();
    let content = fact.relative_content_range();
    let Ok(affected_start) = usize::try_from(affected.start) else {
        return Vec::new();
    };
    let Ok(affected_end) = usize::try_from(affected.end) else {
        return Vec::new();
    };
    let Ok(content_start) = usize::try_from(content.start) else {
        return Vec::new();
    };
    let Ok(content_end) = usize::try_from(content.end) else {
        return Vec::new();
    };
    if affected_start >= affected_end
        || affected_end > source.len()
        || content_start <= affected_start
        || content_end >= affected_end
        || content_start >= content_end
    {
        return Vec::new();
    }

    let mut components = Vec::new();
    for point in content_start.saturating_add(1)..content_end {
        if !source[point - 1].is_ascii_alphanumeric() || !source[point].is_ascii_alphanumeric() {
            continue;
        }
        let Ok(point) = u32::try_from(point) else {
            return Vec::new();
        };
        components.push(M11InlineEditComponent {
            affected: affected.clone(),
            trigger: point..point,
            matcher: M11InlineEditComponentMatcher::InsertExactScalarAtPoint { scalar: '[' },
        });
        if components.len() == M11_INLINE_EDIT_COMPONENTS_MAX {
            break;
        }
    }
    components
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
            derive_inline_edit_components(b"Before **bold** after.", &[strong(7..15, 9..13)], true);
        assert_eq!(components.len(), 3);
        assert_eq!(components[0].affected(), 7..15);
        assert_eq!(components[0].trigger(), 10..10);
        assert_eq!(
            components[0].matcher(),
            M11InlineEditComponentMatcher::InsertExactScalarAtPoint { scalar: '[' }
        );
    }

    #[test]
    fn bracket_component_fails_closed_for_dependency_or_nested_fact() {
        assert!(derive_inline_edit_components(
            b"Before **bold** after ]",
            &[strong(7..15, 9..13)],
            true,
        )
        .is_empty());
        assert!(derive_inline_edit_components(
            b"Before **bold** after.",
            &[strong(7..15, 9..13), strong(8..14, 10..12)],
            true,
        )
        .is_empty());
        assert!(derive_inline_edit_components(
            b"Before **bold** after.",
            &[strong(7..15, 9..13)],
            false,
        )
        .is_empty());
        assert!(derive_inline_edit_components(
            b"Before **bold**\nafter.",
            &[strong(7..15, 9..13)],
            true,
        )
        .is_empty());
    }
}

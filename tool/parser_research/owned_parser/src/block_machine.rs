//! Canonical block-line recognition and restart state.
//!
//! Both clean semantic parsing and incremental checkpointing consume these
//! rules. Keeping them here is an explicit architecture constraint: the
//! checkpoint layer must not grow a predictive Markdown grammar of its own.

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BlockState {
    pub quote_depth: u16,
    pub leaf: LeafState,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum LeafState {
    #[default]
    None,
    /// Inline parsing is deferred until the paragraph closes, so the rolling
    /// digest is part of the convergence state. Without it, an edit can appear
    /// converged on the next unchanged line while the paragraph's inline tree
    /// is still different.
    Paragraph {
        digest: u64,
    },
    /// A setext underline closes a paragraph and is semantically part of the
    /// resulting heading. Retain the digest for one checkpoint so convergence
    /// cannot incorrectly reuse the old underline after changed heading text.
    SetextHeading {
        digest: u64,
    },
    IndentedCode,
    Fence {
        marker: u8,
        length: u16,
        indent: u8,
    },
    HtmlComment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FenceOpen<'a> {
    pub indent: usize,
    pub marker: u8,
    pub length: usize,
    pub info: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct QuotePrefix {
    pub marker_offset: usize,
    pub content_offset: usize,
}

/// Advance the minimal restart state after consuming one physical line.
///
/// This is intentionally not a second parser: it uses the same recognizers as
/// the semantic block builder. The stress milestone will extend this state to
/// a full container stack rather than adding independent heuristics.
pub(crate) fn advance_state(mut state: BlockState, line_with_ending: &str) -> BlockState {
    let line = line_with_ending
        .strip_suffix('\n')
        .unwrap_or(line_with_ending);
    let (quote_depth, inner) = strip_quote_prefixes(line);

    if state.quote_depth > quote_depth {
        let lazy = matches!(state.leaf, LeafState::Paragraph { .. })
            && !is_blank(inner)
            && !starts_interrupting_block(inner);
        if !lazy {
            state = BlockState::default();
        }
    }
    if state.quote_depth <= quote_depth || matches!(state.leaf, LeafState::None) {
        state.quote_depth = quote_depth;
    }

    state.leaf = match state.leaf {
        LeafState::Fence {
            marker,
            length,
            indent,
        } => {
            if state.quote_depth != quote_depth {
                classify_leaf(inner)
            } else if is_fence_close(inner, marker, length as usize) {
                LeafState::None
            } else {
                LeafState::Fence {
                    marker,
                    length,
                    indent,
                }
            }
        }
        LeafState::HtmlComment => {
            if inner.contains("-->") {
                LeafState::None
            } else {
                LeafState::HtmlComment
            }
        }
        LeafState::Paragraph { digest } => {
            if is_blank(inner) {
                LeafState::None
            } else if setext_level(inner).is_some() {
                LeafState::SetextHeading {
                    digest: extend_digest(digest, line_with_ending.as_bytes()),
                }
            } else if starts_interrupting_block(inner) {
                classify_leaf(inner)
            } else {
                LeafState::Paragraph {
                    digest: extend_digest(digest, line_with_ending.as_bytes()),
                }
            }
        }
        LeafState::SetextHeading { .. } => classify_leaf(inner),
        LeafState::IndentedCode => {
            if is_blank(inner) || leading_spaces(inner) >= 4 {
                LeafState::IndentedCode
            } else {
                classify_leaf(inner)
            }
        }
        LeafState::None => classify_leaf(inner),
    };
    if quote_depth == 0 && is_blank(line) {
        state.quote_depth = 0;
    }
    state
}

fn classify_leaf(line: &str) -> LeafState {
    if is_blank(line)
        || atx_heading(line)
        || is_thematic_break(line)
        || setext_level(line).is_some()
    {
        LeafState::None
    } else if let Some(open) = fence_open(line) {
        LeafState::Fence {
            marker: open.marker,
            length: open.length as u16,
            indent: open.indent as u8,
        }
    } else if line.trim_start().starts_with("<!--") && !line.contains("-->") {
        LeafState::HtmlComment
    } else if leading_spaces(line) >= 4 {
        LeafState::IndentedCode
    } else {
        LeafState::Paragraph {
            digest: extend_digest(DIGEST_OFFSET, line.as_bytes()),
        }
    }
}

pub(crate) fn quote_prefix(line: &str) -> Option<QuotePrefix> {
    let spaces = leading_spaces(line);
    if spaces > 3 || line.as_bytes().get(spaces) != Some(&b'>') {
        return None;
    }
    let mut content_offset = spaces + 1;
    if matches!(line.as_bytes().get(content_offset), Some(b' ' | b'\t')) {
        content_offset += 1;
    }
    Some(QuotePrefix {
        marker_offset: spaces,
        content_offset,
    })
}

pub(crate) fn strip_quote_prefixes(mut line: &str) -> (u16, &str) {
    let mut depth = 0u16;
    while let Some(prefix) = quote_prefix(line) {
        line = &line[prefix.content_offset..];
        depth = depth.saturating_add(1);
    }
    (depth, line)
}

pub(crate) fn starts_interrupting_block(line: &str) -> bool {
    fence_open(line).is_some()
        || atx_heading(line)
        || is_thematic_break(line)
        || quote_prefix(line).is_some()
}

pub(crate) fn fence_open(line: &str) -> Option<FenceOpen<'_>> {
    let indent = leading_spaces(line);
    if indent > 3 {
        return None;
    }
    let rest = &line.as_bytes()[indent..];
    let marker = *rest.first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let length = rest.iter().take_while(|byte| **byte == marker).count();
    if length < 3 {
        return None;
    }
    let info = line[indent + length..].trim();
    if marker == b'`' && info.contains('`') {
        return None;
    }
    Some(FenceOpen {
        indent,
        marker,
        length,
        info,
    })
}

pub(crate) fn is_fence_close(line: &str, marker: u8, minimum: usize) -> bool {
    let indent = leading_spaces(line);
    if indent > 3 {
        return false;
    }
    let rest = &line.as_bytes()[indent..];
    let length = rest.iter().take_while(|byte| **byte == marker).count();
    length >= minimum
        && rest[length..]
            .iter()
            .all(|byte| matches!(byte, b' ' | b'\t'))
}

pub(crate) fn atx_heading(line: &str) -> bool {
    let indent = leading_spaces(line);
    if indent > 3 {
        return false;
    }
    let rest = &line.as_bytes()[indent..];
    let count = rest.iter().take_while(|byte| **byte == b'#').count();
    (1..=6).contains(&count)
        && rest
            .get(count)
            .is_none_or(|byte| matches!(byte, b' ' | b'\t'))
}

pub(crate) fn setext_level(line: &str) -> Option<u8> {
    let indent = leading_spaces(line);
    if indent > 3 {
        return None;
    }
    let trimmed = line[indent..].trim_ascii_end();
    if !trimmed.is_empty() && trimmed.bytes().all(|byte| byte == b'=') {
        Some(1)
    } else if !trimmed.is_empty() && trimmed.bytes().all(|byte| byte == b'-') {
        Some(2)
    } else {
        None
    }
}

pub(crate) fn is_thematic_break(line: &str) -> bool {
    let indent = leading_spaces(line);
    if indent > 3 {
        return false;
    }
    let mut marker = None;
    let mut count = 0;
    for byte in line.as_bytes()[indent..].iter().copied() {
        if matches!(byte, b' ' | b'\t') {
            continue;
        }
        if !matches!(byte, b'*' | b'-' | b'_') || marker.is_some_and(|old| old != byte) {
            return false;
        }
        marker = Some(byte);
        count += 1;
    }
    count >= 3
}

pub(crate) fn is_blank(line: &str) -> bool {
    line.bytes().all(|byte| matches!(byte, b' ' | b'\t'))
}

pub(crate) fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

const DIGEST_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

fn extend_digest(mut digest: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        digest ^= *byte as u64;
        digest = digest.wrapping_mul(0x100_0000_01b3);
    }
    digest
}

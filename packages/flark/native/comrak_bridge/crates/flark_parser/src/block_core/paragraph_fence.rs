// SPDX-License-Identifier: MIT

//! Parser-semantic inline-leaf selection over generic recursive Green storage.

use std::{fmt, ops::Range};

use flark_engine::parser_internal::{
    M11ParserSourceRangeAuthority, M11RecursiveGreenError, M11RecursiveGreenFrameFence,
    M11RecursiveGreenFrameId, M11RecursiveGreenFrameQueryError, M11RecursiveGreenFrameQueryLimits,
    M11RecursiveGreenKind, M11RecursiveGreenPoint, M11RecursiveGreenQueryReceipt,
    M11RecursiveGreenRenderableRow, M11RecursiveGreenRoot, M11RecursiveGreenRowQueryLimits,
    M11RecursiveGreenRowWindow, M11RecursiveGreenSliceRoot,
};
use flark_engine::{DocumentRuntime, SourceVersion};

use super::{
    writer::{
        FACT_CODE, FACT_HEADING, FACT_ITEM, FACT_LIST, KIND_BLOCK_QUOTE,
        KIND_EMPTY_BLOCK_QUOTE_ROW, KIND_EMPTY_ITEM_ROW, KIND_FENCED_CODE, KIND_HEADING,
        KIND_INDENTED_CODE, KIND_ITEM, KIND_LIST, KIND_PARAGRAPH, KIND_THEMATIC_BREAK,
    },
    BulletMarker, FenceCharacter, HeadingStyle, ListDelimiter,
};

const MAX_ROW_LIST_PREFIX_BYTES: usize = 64;
const MAX_ROW_BLOCK_QUOTE_PREFIX_BYTES: usize = 64;
const MAX_SIMPLE_BLOCK_QUOTE_SOURCE_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenListMarker {
    Bullet(BulletMarker),
    Ordered {
        value: u32,
        delimiter: ListDelimiter,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenCodeBlockStyle {
    Indented,
    Fenced {
        fence: FenceCharacter,
        minimum_closing_length: u32,
        fence_offset: u8,
        closed: bool,
    },
}

/// Parser-owned presentation facts for one renderable row.
///
/// This is intentionally semantic rather than a renderer style. Hosts may
/// choose typography, but they never re-parse source to discover the heading
/// level or syntax family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenRowPresentation {
    Plain,
    Heading {
        level: u8,
        style: HeadingStyle,
    },
    ListItem {
        marker: M11RecursiveGreenListMarker,
        prefix_start_byte: u64,
        prefix_end_byte: u64,
        prefix_start_utf16: u64,
        prefix_end_utf16: u64,
        /// Exact end of the complete parser-owned item, including nested
        /// descendants. Indentation may use a one-line splice only when this
        /// equals the active renderable row end.
        item_end_byte: u64,
        item_end_utf16: u64,
        nesting_depth: u8,
        marker_offset: u8,
        /// Parser-authored CommonMark width of this item marker plus its
        /// following padding. A following sibling must use this exact width
        /// when it becomes this item's child.
        item_padding: u8,
        container_widths: u64,
        container_count: u8,
        marker_column: u8,
        simple_continuation: bool,
        starts_list: bool,
        task_checked: Option<bool>,
    },
    BlockQuote {
        prefix_start_byte: u64,
        prefix_end_byte: u64,
        prefix_start_utf16: u64,
        prefix_end_utf16: u64,
        nesting_depth: u8,
        /// Exact parser-certified physical widths of each quote marker,
        /// packed root-first as four-bit values.
        container_widths: u64,
        container_count: u8,
        simple_continuation: bool,
    },
    CodeBlock {
        style: M11RecursiveGreenCodeBlockStyle,
    },
    ThematicBreak,
}

/// Decodes final grammar facts already carried by the row's Green frame.
pub fn m11_recursive_green_row_presentation(
    runtime: &DocumentRuntime,
    row: &M11RecursiveGreenRenderableRow,
) -> Result<M11RecursiveGreenRowPresentation, M11RecursiveGreenError> {
    if row.kind().get() == KIND_HEADING {
        return heading_row_presentation(row);
    }
    if matches!(
        row.kind().get(),
        KIND_PARAGRAPH | KIND_EMPTY_ITEM_ROW | KIND_EMPTY_BLOCK_QUOTE_ROW
    ) {
        if let Some(presentation) = list_item_row_presentation(runtime, row)? {
            return Ok(presentation);
        }
        if let Some(presentation) = block_quote_row_presentation(runtime, row)? {
            return Ok(presentation);
        }
    }
    if row.kind().get() == KIND_INDENTED_CODE {
        return Ok(M11RecursiveGreenRowPresentation::CodeBlock {
            style: M11RecursiveGreenCodeBlockStyle::Indented,
        });
    }
    if row.kind().get() == KIND_FENCED_CODE {
        return fenced_code_row_presentation(row);
    }
    if row.kind().get() == KIND_THEMATIC_BREAK {
        return Ok(M11RecursiveGreenRowPresentation::ThematicBreak);
    }
    Ok(M11RecursiveGreenRowPresentation::Plain)
}

fn heading_row_presentation(
    row: &M11RecursiveGreenRenderableRow,
) -> Result<M11RecursiveGreenRowPresentation, M11RecursiveGreenError> {
    let frame = row.path().last().ok_or(M11RecursiveGreenError::Corrupt(
        "Heading row omitted its final path frame",
    ))?;
    let property = frame.property().ok_or(M11RecursiveGreenError::Corrupt(
        "Heading row omitted parser-authored facts",
    ))?;
    let bytes = property.as_bytes();
    if frame.kind() != row.kind()
        || property.tag().get() != FACT_HEADING
        || bytes.len() != 2
        || !(1..=6).contains(&bytes[0])
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "Heading row carried invalid parser-authored facts",
        ));
    }
    let style = match bytes[1] {
        0 => HeadingStyle::Atx,
        1 => HeadingStyle::Setext,
        _ => {
            return Err(M11RecursiveGreenError::Corrupt(
                "Heading row carried an invalid syntax family",
            ))
        }
    };
    Ok(M11RecursiveGreenRowPresentation::Heading {
        level: bytes[0],
        style,
    })
}

fn list_item_row_presentation(
    runtime: &DocumentRuntime,
    row: &M11RecursiveGreenRenderableRow,
) -> Result<Option<M11RecursiveGreenRowPresentation>, M11RecursiveGreenError> {
    let path = row.path();
    let Some(item_index) = path
        .iter()
        .rposition(|frame| frame.kind().get() == KIND_ITEM)
    else {
        return Ok(None);
    };
    let Some(list_index) = path[..item_index]
        .iter()
        .rposition(|frame| frame.kind().get() == KIND_LIST)
    else {
        return Err(M11RecursiveGreenError::Corrupt(
            "List Item row omitted its List ancestor",
        ));
    };
    let list = &path[list_index];
    let item = &path[item_index];
    if item_index + 1 != path.len() - 1 {
        return Ok(None);
    }
    let list_fact = list.property().ok_or(M11RecursiveGreenError::Corrupt(
        "List Item row omitted parser-authored List facts",
    ))?;
    let item_fact = item.property().ok_or(M11RecursiveGreenError::Corrupt(
        "List Item row omitted parser-authored Item facts",
    ))?;
    let list_bytes = list_fact.as_bytes();
    let item_bytes = item_fact.as_bytes();
    if list_fact.tag().get() != FACT_LIST
        || list_bytes.len() != 8
        || item_fact.tag().get() != FACT_ITEM
        || !matches!(item_bytes.len(), 4 | 5)
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "List Item row carried invalid parser-authored facts",
        ));
    }
    let marker_offset = u16::from_le_bytes([item_bytes[0], item_bytes[1]]);
    let padding = u16::from_le_bytes([item_bytes[2], item_bytes[3]]);
    let task_checked = match item_bytes.get(4).copied().unwrap_or(0) {
        0 => None,
        1 => Some(false),
        2 => Some(true),
        _ => {
            return Err(M11RecursiveGreenError::Corrupt(
                "List Item row carried an invalid task marker fact",
            ));
        }
    };
    if marker_offset > 3 || !(2..=14).contains(&padding) {
        return Err(M11RecursiveGreenError::Corrupt(
            "List Item row carried invalid indentation facts",
        ));
    }

    let row_start = row.physical_range().start;
    let row_start_utf16 = row.physical_utf16_range().start;
    let prefix_start = item.physical_range().start;
    let prefix_start_utf16 = item.physical_utf16_range().start;
    let prefix_len = row_start
        .checked_sub(prefix_start)
        .ok_or(M11RecursiveGreenError::Corrupt(
            "List Item prefix follows its row",
        ))?;
    let _prefix_utf16_len =
        row_start_utf16
            .checked_sub(prefix_start_utf16)
            .ok_or(M11RecursiveGreenError::Corrupt(
                "List Item UTF-16 prefix follows its row",
            ))?;
    let Ok(prefix_len) = usize::try_from(prefix_len) else {
        return Ok(None);
    };
    if prefix_len == 0 || prefix_len > MAX_ROW_LIST_PREFIX_BYTES {
        return Ok(None);
    }
    let start =
        usize::try_from(prefix_start).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let end = usize::try_from(row_start).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let mut prefix = [0_u8; MAX_ROW_LIST_PREFIX_BYTES];
    let read = runtime.read_current_source_window(start..end, &mut prefix[..prefix_len])?;
    if read != prefix_len {
        return Err(M11RecursiveGreenError::Corrupt(
            "List Item prefix source stopped early",
        ));
    }
    let prefix = &prefix[..prefix_len];
    let trailing_line_ending_len = if row.kind().get() == KIND_EMPTY_ITEM_ROW {
        if prefix.ends_with(b"\r\n") {
            2
        } else if prefix.ends_with(b"\n") || prefix.ends_with(b"\r") {
            1
        } else {
            0
        }
    } else {
        0
    };
    let marker_prefix_len = prefix_len.saturating_sub(trailing_line_ending_len);
    let prefix = &prefix[..marker_prefix_len];
    if prefix.is_empty() || prefix.iter().any(|byte| matches!(byte, b'\r' | b'\n')) {
        return Ok(None);
    }
    let prefix_end = row_start
        .checked_sub(trailing_line_ending_len as u64)
        .ok_or(M11RecursiveGreenError::Corrupt(
            "List Item prefix line ending exceeds its row start",
        ))?;
    let prefix_end_utf16 = row_start_utf16
        .checked_sub(trailing_line_ending_len as u64)
        .ok_or(M11RecursiveGreenError::Corrupt(
            "List Item UTF-16 prefix line ending exceeds its row start",
        ))?;
    let list_marker_prefix = if let Some(checked) = task_checked {
        task_list_marker_prefix(prefix, checked)?
    } else {
        prefix
    };
    let marker = decode_list_marker(list_marker_prefix, list_bytes)?;
    let nesting_depth = u8::try_from(
        path.iter()
            .filter(|frame| frame.kind().get() == KIND_LIST)
            .count(),
    )
    .map_err(|_| M11RecursiveGreenError::Corrupt("List nesting exceeds its parser bound"))?;
    let root_document = path
        .first()
        .is_some_and(|frame| frame.kind().get() == super::writer::KIND_DOCUMENT);
    let pure_list_path = root_document
        && item_index == usize::from(nesting_depth) * 2
        && list_index + 1 == item_index
        && item_index + 1 == path.len() - 1
        && path[1..=item_index]
            .chunks_exact(2)
            .all(|pair| pair[0].kind().get() == KIND_LIST && pair[1].kind().get() == KIND_ITEM);
    let mut container_widths = 0_u64;
    let mut container_count = 0_u8;
    let mut marker_column =
        u8::try_from(marker_offset).expect("validated Item marker offsets fit in u8");
    let mut bounded_lineage = pure_list_path;
    if pure_list_path {
        for ancestor_item_index in (2..item_index).step_by(2) {
            if container_count >= 16 {
                bounded_lineage = false;
                break;
            }
            let ancestor_fact =
                path[ancestor_item_index]
                    .property()
                    .ok_or(M11RecursiveGreenError::Corrupt(
                        "List ancestor omitted parser-authored Item facts",
                    ))?;
            let ancestor_bytes = ancestor_fact.as_bytes();
            if ancestor_fact.tag().get() != FACT_ITEM || !matches!(ancestor_bytes.len(), 4 | 5) {
                return Err(M11RecursiveGreenError::Corrupt(
                    "List ancestor carried invalid parser-authored Item facts",
                ));
            }
            let width = u16::from_le_bytes([ancestor_bytes[2], ancestor_bytes[3]]);
            if !(2..=14).contains(&width) {
                return Err(M11RecursiveGreenError::Corrupt(
                    "List ancestor carried invalid container width",
                ));
            }
            let Some(next_column) = u16::from(marker_column).checked_add(width) else {
                bounded_lineage = false;
                break;
            };
            let Ok(next_column) = u8::try_from(next_column) else {
                bounded_lineage = false;
                break;
            };
            container_widths |= u64::from(width) << (u32::from(container_count) * 4);
            container_count += 1;
            marker_column = next_column;
        }
    }
    let simple_continuation =
        pure_list_path && bounded_lineage && container_count == nesting_depth.saturating_sub(1);
    let starts_list = item.physical_range().start == list.physical_range().start;
    Ok(Some(M11RecursiveGreenRowPresentation::ListItem {
        marker,
        prefix_start_byte: prefix_start,
        prefix_end_byte: prefix_end,
        prefix_start_utf16,
        prefix_end_utf16,
        item_end_byte: item.physical_range().end,
        item_end_utf16: item.physical_utf16_range().end,
        nesting_depth,
        marker_offset: u8::try_from(marker_offset)
            .expect("validated Item marker offsets fit in u8"),
        item_padding: u8::try_from(padding).expect("validated Item padding fits in u8"),
        container_widths,
        container_count,
        marker_column,
        simple_continuation,
        starts_list,
        task_checked,
    }))
}

fn block_quote_row_presentation(
    runtime: &DocumentRuntime,
    row: &M11RecursiveGreenRenderableRow,
) -> Result<Option<M11RecursiveGreenRowPresentation>, M11RecursiveGreenError> {
    let path = row.path();
    let Some(first_quote_index) = path
        .iter()
        .position(|frame| frame.kind().get() == KIND_BLOCK_QUOTE)
    else {
        return Ok(None);
    };
    let nesting_depth = u8::try_from(
        path.iter()
            .filter(|frame| frame.kind().get() == KIND_BLOCK_QUOTE)
            .count(),
    )
    .map_err(|_| M11RecursiveGreenError::Corrupt("BlockQuote nesting exceeds its parser bound"))?;
    let quote = &path[first_quote_index];
    let row_start = row.physical_range().start;
    let quote_start = quote.physical_range().start;
    let empty_quote_row = row.kind().get() == KIND_EMPTY_BLOCK_QUOTE_ROW;
    let read_start = if empty_quote_row {
        row_start
            .saturating_sub(MAX_ROW_BLOCK_QUOTE_PREFIX_BYTES as u64)
            .max(quote_start)
    } else {
        quote_start
    };
    let read_len = row_start
        .checked_sub(read_start)
        .ok_or(M11RecursiveGreenError::Corrupt(
            "BlockQuote prefix follows its row",
        ))?;
    let Ok(read_len) = usize::try_from(read_len) else {
        return Ok(None);
    };
    if read_len == 0 || read_len > MAX_ROW_BLOCK_QUOTE_PREFIX_BYTES {
        return Ok(None);
    }
    let start = usize::try_from(read_start).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let end = usize::try_from(row_start).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let mut prefix = [0_u8; MAX_ROW_BLOCK_QUOTE_PREFIX_BYTES];
    let read = runtime.read_current_source_window(start..end, &mut prefix[..read_len])?;
    if read != read_len {
        return Err(M11RecursiveGreenError::Corrupt(
            "BlockQuote prefix source stopped early",
        ));
    }
    let trailing_line_ending_len = if empty_quote_row {
        let prefix = &prefix[..read_len];
        if prefix.ends_with(b"\r\n") {
            2
        } else if prefix.ends_with(b"\n") || prefix.ends_with(b"\r") {
            1
        } else {
            0
        }
    } else {
        0
    };
    let marker_end = read_len.saturating_sub(trailing_line_ending_len);
    // A quote container may begin with an empty physical line before this
    // paragraph. In that case `quote_start..row_start` contains both that
    // line and this row's prefix; only the final physical-line prefix belongs
    // to the row presentation.
    let marker_start = prefix[..marker_end]
        .iter()
        .rposition(|byte| matches!(byte, b'\r' | b'\n'))
        .map_or(0, |index| index + 1);
    if empty_quote_row && marker_start == 0 && read_start != quote_start {
        return Ok(None);
    }
    if marker_start == marker_end
        || prefix[marker_start..marker_end]
            .iter()
            .any(|byte| matches!(byte, b'\r' | b'\n'))
    {
        return Ok(None);
    }
    let prefix_start =
        read_start
            .checked_add(marker_start as u64)
            .ok_or(M11RecursiveGreenError::Corrupt(
                "BlockQuote prefix start exceeds its row",
            ))?;
    let prefix_end =
        read_start
            .checked_add(marker_end as u64)
            .ok_or(M11RecursiveGreenError::Corrupt(
                "BlockQuote prefix end exceeds its row",
            ))?;
    let lease = runtime.snapshot_current_source()?;
    let prefix_start_utf16 = u64::try_from(lease.utf16_offset_for_byte(
        usize::try_from(prefix_start).map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
    )?)
    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let prefix_end_utf16 = u64::try_from(lease.utf16_offset_for_byte(
        usize::try_from(prefix_end).map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
    )?)
    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let pure_quote_path = first_quote_index == 1
        && path.len() == usize::from(nesting_depth) + 2
        && path[0].kind().get() == super::writer::KIND_DOCUMENT
        && path[1..=usize::from(nesting_depth)]
            .iter()
            .all(|frame| frame.kind().get() == KIND_BLOCK_QUOTE);
    let (container_widths, container_count) = if pure_quote_path {
        m11_block_quote_prefix_lineage(&prefix[marker_start..marker_end], nesting_depth)
            .unwrap_or((0, 0))
    } else {
        (0, 0)
    };
    let bounded_lineage = container_count == nesting_depth;
    let simple_continuation = pure_quote_path
        && bounded_lineage
        && row.editable_range().is_some()
        && row_has_one_physical_line(runtime, row)?;
    Ok(Some(M11RecursiveGreenRowPresentation::BlockQuote {
        prefix_start_byte: prefix_start,
        prefix_end_byte: prefix_end,
        prefix_start_utf16,
        prefix_end_utf16,
        nesting_depth,
        container_widths,
        container_count,
        simple_continuation,
    }))
}

/// Splits an already parser-certified pure block-quote prefix into exact
/// container contributions. This does not classify arbitrary Markdown: the
/// caller supplies the certified nesting depth, and the complete prefix must
/// be consumed by exactly that many markers.
pub fn m11_block_quote_prefix_lineage(prefix: &[u8], nesting_depth: u8) -> Option<(u64, u8)> {
    if nesting_depth == 0 || nesting_depth > 16 {
        return None;
    }
    let mut cursor = 0_usize;
    let mut widths = 0_u64;
    for index in 0..nesting_depth {
        let marker = prefix[cursor..].iter().position(|byte| *byte == b'>')? + cursor;
        if prefix[cursor..marker]
            .iter()
            .any(|byte| !matches!(byte, b' ' | b'\t'))
        {
            return None;
        }
        let mut end = marker.checked_add(1)?;
        if prefix
            .get(end)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
        {
            end += 1;
        }
        let width = end.checked_sub(cursor)?;
        if !(1..=15).contains(&width) {
            return None;
        }
        widths |= u64::try_from(width).ok()? << (u32::from(index) * 4);
        cursor = end;
    }
    (cursor == prefix.len()).then_some((widths, nesting_depth))
}

#[cfg(test)]
mod block_quote_prefix_lineage_tests {
    use super::m11_block_quote_prefix_lineage;

    #[test]
    fn accepts_exact_physical_marker_widths_and_rejects_partial_prefixes() {
        assert_eq!(m11_block_quote_prefix_lineage(b"> > ", 2), Some((0x22, 2)));
        assert_eq!(m11_block_quote_prefix_lineage(b">>", 2), Some((0x11, 2)));
        assert_eq!(
            m11_block_quote_prefix_lineage(b" >\t> ", 2),
            Some((0x23, 2))
        );
        assert_eq!(m11_block_quote_prefix_lineage(b"> > tail", 2), None);
        assert_eq!(m11_block_quote_prefix_lineage(b"- > ", 1), None);
        assert_eq!(m11_block_quote_prefix_lineage(b"> ", 0), None);
    }
}

fn row_has_one_physical_line(
    runtime: &DocumentRuntime,
    row: &M11RecursiveGreenRenderableRow,
) -> Result<bool, M11RecursiveGreenError> {
    let range = row.physical_range();
    let length = range.end.saturating_sub(range.start);
    let Ok(length) = usize::try_from(length) else {
        return Ok(false);
    };
    if length > MAX_SIMPLE_BLOCK_QUOTE_SOURCE_BYTES {
        return Ok(false);
    }
    let start =
        usize::try_from(range.start).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let end = usize::try_from(range.end).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let mut source = [0_u8; MAX_SIMPLE_BLOCK_QUOTE_SOURCE_BYTES];
    let read = runtime.read_current_source_window(start..end, &mut source[..length])?;
    if read != length {
        return Err(M11RecursiveGreenError::Corrupt(
            "BlockQuote row source stopped early",
        ));
    }
    let source = &source[..length];
    let content_end = if source.ends_with(b"\r\n") {
        source.len() - 2
    } else if source.ends_with(b"\n") || source.ends_with(b"\r") {
        source.len() - 1
    } else {
        source.len()
    };
    Ok(!source[..content_end]
        .iter()
        .any(|byte| matches!(byte, b'\r' | b'\n')))
}

fn fenced_code_row_presentation(
    row: &M11RecursiveGreenRenderableRow,
) -> Result<M11RecursiveGreenRowPresentation, M11RecursiveGreenError> {
    let frame = row.path().last().ok_or(M11RecursiveGreenError::Corrupt(
        "FencedCode row omitted its final path frame",
    ))?;
    let property = frame.property().ok_or(M11RecursiveGreenError::Corrupt(
        "FencedCode row omitted parser-authored facts",
    ))?;
    let close = frame.close().ok_or(M11RecursiveGreenError::Corrupt(
        "FencedCode row omitted parser-authored close facts",
    ))?;
    let bytes = property.as_bytes();
    let close_bytes = close.as_bytes();
    if frame.kind() != row.kind()
        || property.tag().get() != FACT_CODE
        || bytes.len() != 10
        || close.tag().get() != FACT_CODE
        || close_bytes.len() < 33
        || bytes[1] > 3
        || !matches!(close_bytes[0], 0 | 1)
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "FencedCode row carried invalid parser-authored facts",
        ));
    }
    let fence = match bytes[0] {
        b'`' => FenceCharacter::Backtick,
        b'~' => FenceCharacter::Tilde,
        _ => {
            return Err(M11RecursiveGreenError::Corrupt(
                "FencedCode row carried an invalid fence character",
            ))
        }
    };
    let minimum_closing_length = u64::from_le_bytes(
        bytes[2..10]
            .try_into()
            .expect("fixed FencedCode fact width"),
    );
    let minimum_closing_length = u32::try_from(minimum_closing_length).map_err(|_| {
        M11RecursiveGreenError::Corrupt("FencedCode fence length exceeds the ABI bound")
    })?;
    if minimum_closing_length < 3 {
        return Err(M11RecursiveGreenError::Corrupt(
            "FencedCode row carried an invalid fence length",
        ));
    }
    Ok(M11RecursiveGreenRowPresentation::CodeBlock {
        style: M11RecursiveGreenCodeBlockStyle::Fenced {
            fence,
            minimum_closing_length,
            fence_offset: bytes[1],
            closed: close_bytes[0] == 1,
        },
    })
}

fn decode_list_marker(
    prefix: &[u8],
    list_fact: &[u8],
) -> Result<M11RecursiveGreenListMarker, M11RecursiveGreenError> {
    let marker_end = prefix
        .iter()
        .rposition(|byte| !matches!(byte, b' ' | b'\t'))
        .map(|index| index + 1)
        .ok_or(M11RecursiveGreenError::Corrupt(
            "List Item prefix omitted its marker",
        ))?;
    match list_fact[0] {
        1 => {
            let marker = match list_fact[1] {
                b'-' => BulletMarker::Hyphen,
                b'+' => BulletMarker::Plus,
                b'*' => BulletMarker::Asterisk,
                _ => {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "Bullet List carried an invalid marker",
                    ))
                }
            };
            if prefix[marker_end - 1] != marker.byte() {
                return Err(M11RecursiveGreenError::Corrupt(
                    "List Item prefix disagrees with its Bullet List",
                ));
            }
            Ok(M11RecursiveGreenListMarker::Bullet(marker))
        }
        2 => {
            let delimiter = match list_fact[2] {
                b'.' => ListDelimiter::Period,
                b')' => ListDelimiter::Parenthesis,
                _ => {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "Ordered List carried an invalid delimiter",
                    ))
                }
            };
            let delimiter_byte = match delimiter {
                ListDelimiter::Period => b'.',
                ListDelimiter::Parenthesis => b')',
            };
            if prefix[marker_end - 1] != delimiter_byte {
                return Err(M11RecursiveGreenError::Corrupt(
                    "List Item prefix disagrees with its Ordered List",
                ));
            }
            let mut digit_start = marker_end - 1;
            while digit_start > 0 && prefix[digit_start - 1].is_ascii_digit() {
                digit_start -= 1;
            }
            let digits = &prefix[digit_start..marker_end - 1];
            if digits.is_empty() || digits.len() > 9 {
                return Err(M11RecursiveGreenError::Corrupt(
                    "Ordered List Item carried an invalid marker value",
                ));
            }
            let value = digits.iter().try_fold(0_u32, |value, digit| {
                value.checked_mul(10)?.checked_add(u32::from(digit - b'0'))
            });
            let Some(value) = value.filter(|value| *value <= 999_999_999) else {
                return Err(M11RecursiveGreenError::Corrupt(
                    "Ordered List Item marker exceeds the parser bound",
                ));
            };
            Ok(M11RecursiveGreenListMarker::Ordered { value, delimiter })
        }
        _ => Err(M11RecursiveGreenError::Corrupt(
            "List Item row carried an invalid List style",
        )),
    }
}

fn task_list_marker_prefix(prefix: &[u8], checked: bool) -> Result<&[u8], M11RecursiveGreenError> {
    let Some(marker_start) = prefix.iter().rposition(|byte| *byte == b'[') else {
        return Err(M11RecursiveGreenError::Corrupt(
            "Task List Item prefix omitted its task marker",
        ));
    };
    let Some(marker) = prefix.get(marker_start..marker_start + 3) else {
        return Err(M11RecursiveGreenError::Corrupt(
            "Task List Item prefix truncated its task marker",
        ));
    };
    let expected_symbol = if checked { b'x' } else { b' ' };
    let symbol_matches = marker[1].to_ascii_lowercase() == expected_symbol;
    if marker[0] != b'['
        || marker[2] != b']'
        || !symbol_matches
        || !prefix[marker_start + 3..]
            .iter()
            .all(|byte| matches!(byte, b' ' | b'\t'))
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "Task List Item prefix disagrees with its parser-authored task fact",
        ));
    }
    Ok(&prefix[..marker_start])
}

/// Parser-owned inline-bearing recursive-Green leaf kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenInlineLeafKind {
    Paragraph,
    Heading,
}

impl M11RecursiveGreenInlineLeafKind {
    #[must_use]
    pub const fn from_green_kind(kind: M11RecursiveGreenKind) -> Option<Self> {
        match kind.get() {
            KIND_PARAGRAPH => Some(Self::Paragraph),
            KIND_HEADING => Some(Self::Heading),
            _ => None,
        }
    }

    const fn green_kind(self) -> M11RecursiveGreenKind {
        let value = match self {
            Self::Paragraph => KIND_PARAGRAPH,
            Self::Heading => KIND_HEADING,
        };
        match M11RecursiveGreenKind::new(value) {
            Some(kind) => kind,
            None => unreachable!(),
        }
    }
}

/// Move-only proof that a point belongs to one final inline-bearing leaf.
#[must_use = "inline-leaf fences must be consumed by exact inline work or deliberately dropped"]
pub struct M11RecursiveGreenInlineLeafFence {
    inner: M11RecursiveGreenFrameFence,
    kind: M11RecursiveGreenInlineLeafKind,
}

impl fmt::Debug for M11RecursiveGreenInlineLeafFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11RecursiveGreenInlineLeafFence")
            .field("kind", &self.kind)
            .field("source", &self.source())
            .field("frame", &self.frame())
            .field("block_source", &self.block_source_range())
            .field("inline_source", &self.inline_source_range())
            .field("receipt", &self.receipt())
            .finish_non_exhaustive()
    }
}

impl M11RecursiveGreenInlineLeafFence {
    #[must_use]
    pub const fn kind(&self) -> M11RecursiveGreenInlineLeafKind {
        self.kind
    }

    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.inner.source()
    }

    #[must_use]
    pub const fn frame(&self) -> M11RecursiveGreenFrameId {
        self.inner.frame()
    }

    #[must_use]
    pub fn block_source_range(&self) -> Range<u64> {
        self.inner.block_source_range()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> Range<u64> {
        self.inner.block_source_utf16_range()
    }

    #[must_use]
    pub fn inline_source_range(&self) -> Range<u64> {
        self.inner.inline_source_range()
    }

    #[must_use]
    pub fn inline_source_utf16_range(&self) -> Range<u64> {
        self.inner.inline_source_utf16_range()
    }

    #[must_use]
    pub const fn receipt(&self) -> M11RecursiveGreenQueryReceipt {
        self.inner.receipt()
    }

    pub(crate) fn into_inline_authority(self) -> (M11ParserSourceRangeAuthority, Range<u64>) {
        self.inner.into_inline_authority()
    }

    pub(crate) fn into_paragraph(self) -> Option<M11RecursiveGreenParagraphFence> {
        (self.kind == M11RecursiveGreenInlineLeafKind::Paragraph)
            .then_some(M11RecursiveGreenParagraphFence { inner: self })
    }
}

/// Move-only proof that a point belongs to one final Paragraph frame.
///
/// The generic storage query mints every range and the exact source authority;
/// this parser wrapper contributes only the grammar-owned Paragraph kind.
#[must_use = "Paragraph fences must be consumed by exact inline work or deliberately dropped"]
pub struct M11RecursiveGreenParagraphFence {
    inner: M11RecursiveGreenInlineLeafFence,
}

impl fmt::Debug for M11RecursiveGreenParagraphFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11RecursiveGreenParagraphFence")
            .field("source", &self.source())
            .field("frame", &self.frame())
            .field("block_source", &self.block_source_range())
            .field("block_source_utf16", &self.block_source_utf16_range())
            .field("inline_source", &self.inline_source_range())
            .field("inline_source_utf16", &self.inline_source_utf16_range())
            .field("receipt", &self.receipt())
            .finish_non_exhaustive()
    }
}

impl M11RecursiveGreenParagraphFence {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.inner.source()
    }

    #[must_use]
    pub const fn frame(&self) -> M11RecursiveGreenFrameId {
        self.inner.frame()
    }

    #[must_use]
    pub fn block_source_range(&self) -> Range<u64> {
        self.inner.block_source_range()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> Range<u64> {
        self.inner.block_source_utf16_range()
    }

    #[must_use]
    pub fn inline_source_range(&self) -> Range<u64> {
        self.inner.inline_source_range()
    }

    #[must_use]
    pub fn inline_source_utf16_range(&self) -> Range<u64> {
        self.inner.inline_source_utf16_range()
    }

    #[must_use]
    pub const fn receipt(&self) -> M11RecursiveGreenQueryReceipt {
        self.inner.receipt()
    }

    pub(crate) fn into_inline_leaf(self) -> M11RecursiveGreenInlineLeafFence {
        self.inner
    }
}

/// Resolves an exact final Paragraph or Heading owner and mints its contiguous
/// parser-authored inline range. No source range is caller supplied.
pub fn resolve_m11_recursive_green_inline_leaf_fence(
    runtime: &DocumentRuntime,
    root: &M11RecursiveGreenRoot,
    point: M11RecursiveGreenPoint,
    limits: M11RecursiveGreenFrameQueryLimits,
) -> Result<Option<M11RecursiveGreenInlineLeafFence>, M11RecursiveGreenFrameQueryError> {
    let expected = [
        M11RecursiveGreenInlineLeafKind::Paragraph.green_kind(),
        M11RecursiveGreenInlineLeafKind::Heading.green_kind(),
    ];
    root.locate_frame_fence_for_kinds(runtime, point, &expected, limits)
        .map(|fence| {
            fence.map(|inner| M11RecursiveGreenInlineLeafFence {
                kind: M11RecursiveGreenInlineLeafKind::from_green_kind(inner.kind())
                    .expect("accepted Green kind is inline-bearing"),
                inner,
            })
        })
}

/// Resolves an inline-bearing row through cached parser-authored close
/// geometry, avoiding replay from a potentially distant frame `Enter`.
pub fn resolve_m11_recursive_green_inline_leaf_row_fence(
    runtime: &DocumentRuntime,
    root: &M11RecursiveGreenRoot,
    point: M11RecursiveGreenPoint,
    limits: M11RecursiveGreenRowQueryLimits,
    maximum_inline_source_bytes: u64,
) -> Result<Option<M11RecursiveGreenInlineLeafFence>, M11RecursiveGreenFrameQueryError> {
    let expected = [
        M11RecursiveGreenInlineLeafKind::Paragraph.green_kind(),
        M11RecursiveGreenInlineLeafKind::Heading.green_kind(),
    ];
    root.locate_renderable_row_fence_for_kinds(
        runtime,
        point,
        &expected,
        limits,
        maximum_inline_source_bytes,
    )
    .map(|fence| {
        fence.map(|inner| M11RecursiveGreenInlineLeafFence {
            kind: M11RecursiveGreenInlineLeafKind::from_green_kind(inner.kind())
                .expect("accepted Green kind is inline-bearing"),
            inner,
        })
    })
}

/// Slice-root counterpart of
/// [`resolve_m11_recursive_green_inline_leaf_row_fence`]. The distinct root
/// type prevents bounded prefix authority from entering whole-document paths.
pub fn resolve_m11_recursive_green_slice_inline_leaf_row_fence(
    runtime: &DocumentRuntime,
    root: &M11RecursiveGreenSliceRoot,
    point: M11RecursiveGreenPoint,
    limits: M11RecursiveGreenRowQueryLimits,
    maximum_inline_source_bytes: u64,
) -> Result<Option<M11RecursiveGreenInlineLeafFence>, M11RecursiveGreenFrameQueryError> {
    let expected = [
        M11RecursiveGreenInlineLeafKind::Paragraph.green_kind(),
        M11RecursiveGreenInlineLeafKind::Heading.green_kind(),
    ];
    root.locate_renderable_row_fence_for_kinds(
        runtime,
        point,
        &expected,
        limits,
        maximum_inline_source_bytes,
    )
    .map(|fence| {
        fence.map(|inner| M11RecursiveGreenInlineLeafFence {
            kind: M11RecursiveGreenInlineLeafKind::from_green_kind(inner.kind())
                .expect("accepted Green kind is inline-bearing"),
            inner,
        })
    })
}

/// Batch counterpart of
/// [`resolve_m11_recursive_green_slice_inline_leaf_row_fence`]: one shared
/// walk locates the bounded row window and mints every qualifying row's
/// inline-leaf fence. The returned fences are index-aligned with the window's
/// rows; a non-inline-bearing row is `None` with the same admission the
/// per-point resolver applies at that row's own start.
pub fn resolve_m11_recursive_green_slice_inline_leaf_row_fences(
    runtime: &DocumentRuntime,
    root: &M11RecursiveGreenSliceRoot,
    point: M11RecursiveGreenPoint,
    limits: M11RecursiveGreenRowQueryLimits,
    maximum_inline_source_bytes: u64,
) -> Result<
    (
        M11RecursiveGreenRowWindow,
        Vec<Option<M11RecursiveGreenInlineLeafFence>>,
    ),
    M11RecursiveGreenFrameQueryError,
> {
    let expected = [
        M11RecursiveGreenInlineLeafKind::Paragraph.green_kind(),
        M11RecursiveGreenInlineLeafKind::Heading.green_kind(),
    ];
    let (window, fences) = root
        .locate_renderable_row_fences_for_kinds(
            runtime,
            point,
            &expected,
            limits,
            maximum_inline_source_bytes,
        )?
        .into_parts();
    let fences = fences
        .into_iter()
        .map(|fence| {
            fence.map(|inner| M11RecursiveGreenInlineLeafFence {
                kind: M11RecursiveGreenInlineLeafKind::from_green_kind(inner.kind())
                    .expect("accepted Green kind is inline-bearing"),
                inner,
            })
        })
        .collect();
    Ok((window, fences))
}

/// Resolves the physical coverage owner at one authenticated source point and
/// returns it only when its final kind is Paragraph. The caller cannot provide
/// or widen either returned range.
pub fn resolve_m11_recursive_green_paragraph_fence(
    runtime: &DocumentRuntime,
    root: &M11RecursiveGreenRoot,
    point: M11RecursiveGreenPoint,
    limits: M11RecursiveGreenFrameQueryLimits,
) -> Result<Option<M11RecursiveGreenParagraphFence>, M11RecursiveGreenFrameQueryError> {
    resolve_m11_recursive_green_inline_leaf_fence(runtime, root, point, limits).map(|fence| {
        fence.and_then(|inner| {
            (inner.kind() == M11RecursiveGreenInlineLeafKind::Paragraph)
                .then_some(M11RecursiveGreenParagraphFence { inner })
        })
    })
}

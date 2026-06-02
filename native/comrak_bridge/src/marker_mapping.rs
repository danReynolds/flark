use crate::payload::{JsonBlock, JsonInlineToken, JsonRange};
use crate::source_ranges::{
    end_of_line, for_each_line_in_range, leading_indent, line_content_end, line_start_for_offset,
    normalize_ranges, push_range,
};

#[derive(Clone, Copy)]
struct FenceOpener {
    marker_start: usize,
    marker_length: usize,
    marker_byte: u8,
}

#[derive(Clone, Copy)]
struct ListMarker {
    marker_start: usize,
    marker_end: usize,
}

pub(crate) fn collect_block_marker_ranges(
    text: &str,
    blocks: &[JsonBlock],
    is_gfm: bool,
) -> Vec<JsonRange> {
    let text_len = text.len();
    if text_len == 0 || blocks.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    for block in blocks {
        let start = (block.start_byte as usize).min(text_len);
        let end = (block.end_byte as usize).min(text_len);
        if end <= start {
            continue;
        }

        match block.kind {
            "header" => {
                let open_line_end = end_of_line(text, start).min(end);
                let open_line = &text[start..open_line_end];
                if let Some((marker_start, marker_end)) = match_atx_heading_marker(open_line) {
                    push_range(
                        &mut ranges,
                        start + marker_start,
                        start + marker_end,
                        text_len,
                    );
                } else {
                    let underline_start = open_line_end;
                    if underline_start < end {
                        let underline_end = line_content_end(text, underline_start).min(end);
                        let underline_line = &text[underline_start..underline_end];
                        if is_setext_underline(underline_line) {
                            push_range(&mut ranges, underline_start, underline_end, text_len);
                        }
                    }
                }
            }
            "blockquote" => {
                for_each_line_in_range(text, start, end, |line_start, line| {
                    if let Some((marker_start, marker_end)) = match_blockquote_marker(line) {
                        push_range(
                            &mut ranges,
                            line_start + marker_start,
                            line_start + marker_end,
                            text_len,
                        );
                        if let Some((list_start, list_end)) =
                            list_marker_range_allowing_quote_prefix(line, marker_end)
                        {
                            push_range(
                                &mut ranges,
                                line_start + list_start,
                                line_start + list_end,
                                text_len,
                            );
                        }
                    }
                });
            }
            "unordered_list" | "ordered_list" => {
                for_each_line_in_range(text, start, end, |line_start, line| {
                    if let Some(marker) = match_list_marker(line) {
                        push_range(
                            &mut ranges,
                            line_start + marker.marker_start,
                            line_start + marker.marker_end,
                            text_len,
                        );
                        if is_gfm {
                            if let Some((task_start, task_end)) =
                                match_task_checkbox_range(line, marker.marker_end)
                            {
                                push_range(
                                    &mut ranges,
                                    line_start + task_start,
                                    line_start + task_end,
                                    text_len,
                                );
                            }
                        }
                    }
                });
            }
            "fenced_code" => {
                let open_line_end = line_content_end(text, start);
                let open_line = &text[start..open_line_end];
                let opener = match_fence_opener(open_line);
                if let Some(open) = opener {
                    push_range(
                        &mut ranges,
                        start + open.marker_start,
                        start + open.marker_start + open.marker_length,
                        text_len,
                    );

                    let info_start = start + open.marker_start + open.marker_length;
                    if info_start < open_line_end {
                        let info = &text[info_start..open_line_end];
                        if let Some(token) = first_fence_token(info) {
                            if is_supported_fence_tag(token) {
                                push_range(&mut ranges, info_start, open_line_end, text_len);
                            }
                        }
                    }
                }

                if end > start {
                    let close_line_start = line_start_for_offset(text, end.saturating_sub(1));
                    if close_line_start > start {
                        let close_line_end = line_content_end(text, close_line_start);
                        let close_line = &text[close_line_start..close_line_end];
                        if let Some(closer) = match_fence_opener(close_line) {
                            let reference = opener.unwrap_or(closer);
                            if is_fence_closer(close_line, reference) {
                                push_range(
                                    &mut ranges,
                                    close_line_start + closer.marker_start,
                                    close_line_start + closer.marker_start + closer.marker_length,
                                    text_len,
                                );
                                let close_info_start =
                                    close_line_start + closer.marker_start + closer.marker_length;
                                if close_info_start < close_line_end {
                                    push_range(
                                        &mut ranges,
                                        close_info_start,
                                        close_line_end,
                                        text_len,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    normalize_ranges(ranges, text_len)
}

pub(crate) fn collect_inline_marker_ranges(
    text: &str,
    exclusions: &[JsonRange],
    inline_tokens: &[JsonInlineToken],
) -> Vec<JsonRange> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    if len == 0 {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut cursor = 0usize;
    let mut exclusion_index = 0usize;

    while cursor < len {
        while exclusion_index < exclusions.len()
            && (exclusions[exclusion_index].end_byte as usize) <= cursor
        {
            exclusion_index += 1;
        }
        if exclusion_index < exclusions.len() {
            let exclusion = exclusions[exclusion_index];
            let exclusion_start = exclusion.start_byte as usize;
            let exclusion_end = exclusion.end_byte as usize;
            if cursor >= exclusion_start && cursor < exclusion_end {
                cursor = exclusion_end.min(len);
                continue;
            }
        }

        let byte = bytes[cursor];
        if byte.is_ascii_punctuation() && is_escaped_at(bytes, cursor) {
            push_range(&mut ranges, cursor.saturating_sub(1), cursor, len);
        }

        cursor += 1;
    }

    for token in inline_tokens {
        collect_token_marker_ranges(&mut ranges, bytes, token);
    }

    normalize_ranges(ranges, len)
}

fn collect_token_marker_ranges(ranges: &mut Vec<JsonRange>, bytes: &[u8], token: &JsonInlineToken) {
    let len = bytes.len();
    let start = (token.start_byte as usize).min(len);
    let end = (token.end_byte as usize).min(len);
    if end <= start {
        return;
    }

    if token.styles.iter().any(|style| *style == "bold") {
        if has_wrapping_marker(bytes, start, end, b"**") {
            push_wrapping_marker_ranges(ranges, start, end, 2, len);
        } else if has_wrapping_marker(bytes, start, end, b"__") {
            push_wrapping_marker_ranges(ranges, start, end, 2, len);
        }
    }
    if token.styles.iter().any(|style| *style == "italic") {
        if has_wrapping_marker(bytes, start, end, b"*") {
            push_wrapping_marker_ranges(ranges, start, end, 1, len);
        } else if has_wrapping_marker(bytes, start, end, b"_") {
            push_wrapping_marker_ranges(ranges, start, end, 1, len);
        }
    }
    if token.styles.iter().any(|style| *style == "code") {
        let leading = count_edge_byte(bytes, start, end, b'`', true);
        let trailing = count_edge_byte(bytes, start, end, b'`', false);
        let marker_len = leading.min(trailing);
        if marker_len > 0 && end.saturating_sub(start) > marker_len * 2 {
            push_wrapping_marker_ranges(ranges, start, end, marker_len, len);
        }
    }
    if token.styles.iter().any(|style| *style == "strikethrough")
        && has_wrapping_marker(bytes, start, end, b"~~")
    {
        push_wrapping_marker_ranges(ranges, start, end, 2, len);
    }
}

fn has_wrapping_marker(bytes: &[u8], start: usize, end: usize, marker: &[u8]) -> bool {
    let marker_len = marker.len();
    end.saturating_sub(start) > marker_len * 2
        && bytes[start..end].starts_with(marker)
        && bytes[start..end].ends_with(marker)
}

fn push_wrapping_marker_ranges(
    ranges: &mut Vec<JsonRange>,
    start: usize,
    end: usize,
    marker_len: usize,
    text_len: usize,
) {
    push_range(ranges, start, start + marker_len, text_len);
    push_range(ranges, end.saturating_sub(marker_len), end, text_len);
}

fn count_edge_byte(bytes: &[u8], start: usize, end: usize, byte: u8, leading: bool) -> usize {
    let mut count = 0usize;
    if leading {
        let mut cursor = start;
        while cursor < end && bytes[cursor] == byte {
            count += 1;
            cursor += 1;
        }
    } else {
        let mut cursor = end;
        while cursor > start && bytes[cursor - 1] == byte {
            count += 1;
            cursor -= 1;
        }
    }
    count
}

fn is_escaped_at(bytes: &[u8], offset: usize) -> bool {
    if offset == 0 || offset > bytes.len() {
        return false;
    }

    let mut backslashes = 0usize;
    let mut cursor = offset;
    while cursor > 0 {
        cursor -= 1;
        if bytes[cursor] != b'\\' {
            break;
        }
        backslashes += 1;
    }
    backslashes % 2 == 1
}

fn match_atx_heading_marker(line: &str) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let (columns, index) = leading_indent(line);
    if columns > 3 || index >= bytes.len() {
        return None;
    }

    let mut level = 0usize;
    let mut cursor = index;
    while cursor < bytes.len() && bytes[cursor] == b'#' && level < 6 {
        level += 1;
        cursor += 1;
    }
    if level == 0 {
        return None;
    }
    // Keep parity with the existing markdown-package adapter: only treat
    // headings as marker-hide candidates when an explicit space/tab follows the
    // marker run.
    if cursor >= bytes.len() {
        return None;
    }
    if bytes[cursor] != b' ' && bytes[cursor] != b'\t' {
        return None;
    }

    let marker_end = if cursor < bytes.len() {
        cursor + 1
    } else {
        cursor
    };
    Some((index, marker_end.min(bytes.len())))
}

fn is_setext_underline(line: &str) -> bool {
    let bytes = line.as_bytes();
    let (columns, index) = leading_indent(line);
    if columns > 3 || index >= bytes.len() {
        return false;
    }
    let marker = bytes[index];
    if marker != b'-' && marker != b'=' {
        return false;
    }
    for byte in &bytes[index..] {
        if *byte == b' ' || *byte == b'\t' {
            continue;
        }
        if *byte != marker {
            return false;
        }
    }
    true
}

fn match_blockquote_marker(line: &str) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let (columns, index) = leading_indent(line);
    if columns > 3 || index >= bytes.len() || bytes[index] != b'>' {
        return None;
    }

    let mut end = index + 1;
    if end < bytes.len() && (bytes[end] == b' ' || bytes[end] == b'\t') {
        end += 1;
    }
    Some((index, end))
}

fn match_fence_opener(line: &str) -> Option<FenceOpener> {
    let bytes = line.as_bytes();
    let (columns, index) = leading_indent(line);
    if columns > 3 || index >= bytes.len() {
        return None;
    }

    let marker = bytes[index];
    if marker != b'`' && marker != b'~' {
        return None;
    }

    let mut marker_length = 0usize;
    while index + marker_length < bytes.len() && bytes[index + marker_length] == marker {
        marker_length += 1;
    }
    if marker_length < 3 {
        return None;
    }
    if marker == b'`' && bytes[index + marker_length..].contains(&b'`') {
        return None;
    }

    Some(FenceOpener {
        marker_start: index,
        marker_length,
        marker_byte: marker,
    })
}

fn is_fence_closer(line: &str, opener: FenceOpener) -> bool {
    let bytes = line.as_bytes();
    let (columns, index) = leading_indent(line);
    if columns > 3 || index >= bytes.len() || bytes[index] != opener.marker_byte {
        return false;
    }

    let mut marker_length = 0usize;
    while index + marker_length < bytes.len() && bytes[index + marker_length] == opener.marker_byte
    {
        marker_length += 1;
    }
    if marker_length < opener.marker_length {
        return false;
    }

    for byte in &bytes[index + marker_length..] {
        if *byte != b' ' && *byte != b'\t' {
            return false;
        }
    }
    true
}

fn first_fence_token(info: &str) -> Option<&str> {
    info.split_whitespace()
        .next()
        .filter(|token| !token.is_empty())
}

fn is_supported_fence_tag(tag: &str) -> bool {
    let normalized = tag.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "dart"
            | "json"
            | "yaml"
            | "bash"
            | "python"
            | "javascript"
            | "typescript"
            | "xml"
            | "css"
            | "sql"
            | "markdown"
            | "yml"
            | "sh"
            | "shell"
            | "py"
            | "js"
            | "ts"
            | "html"
            | "md"
    )
}

fn match_list_marker(line: &str) -> Option<ListMarker> {
    let bytes = line.as_bytes();
    let (indent_columns, marker_start) = leading_indent(line);
    if indent_columns > 3 || marker_start >= bytes.len() {
        return None;
    }

    let first = bytes[marker_start];
    if first == b'-' || first == b'+' || first == b'*' {
        let marker_end = marker_start + 1;
        if marker_end < bytes.len() && (bytes[marker_end] == b' ' || bytes[marker_end] == b'\t') {
            return Some(ListMarker {
                marker_start,
                marker_end: marker_end + 1,
            });
        }
        return None;
    }

    let mut cursor = marker_start;
    let mut digits = 0usize;
    while cursor < bytes.len() && bytes[cursor].is_ascii_digit() && digits < 9 {
        cursor += 1;
        digits += 1;
    }
    if digits == 0 || cursor >= bytes.len() {
        return None;
    }
    if bytes[cursor] != b'.' && bytes[cursor] != b')' {
        return None;
    }
    cursor += 1;
    if cursor >= bytes.len() || (bytes[cursor] != b' ' && bytes[cursor] != b'\t') {
        return None;
    }

    Some(ListMarker {
        marker_start,
        marker_end: cursor + 1,
    })
}

fn match_task_checkbox_range(line: &str, marker_end: usize) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut cursor = marker_end;
    while cursor < bytes.len() && (bytes[cursor] == b' ' || bytes[cursor] == b'\t') {
        cursor += 1;
    }

    if cursor + 2 >= bytes.len() || bytes[cursor] != b'[' {
        return None;
    }
    let state = bytes[cursor + 1];
    if state != b' ' && state != b'x' && state != b'X' {
        return None;
    }
    if bytes[cursor + 2] != b']' {
        return None;
    }

    let mut end = cursor + 3;
    if end < bytes.len() && (bytes[end] == b' ' || bytes[end] == b'\t') {
        end += 1;
    }
    Some((cursor, end))
}

fn list_marker_range_allowing_quote_prefix(line: &str, from: usize) -> Option<(usize, usize)> {
    fn direct_at(line: &str, start: usize) -> Option<(usize, usize)> {
        if start >= line.len() {
            return None;
        }
        let marker = match_list_marker(&line[start..])?;
        Some((start + marker.marker_start, start + marker.marker_end))
    }

    if from >= line.len() {
        return None;
    }

    if let Some(direct) = direct_at(line, from) {
        return Some(direct);
    }

    let bytes = line.as_bytes();
    let mut cursor = from;
    while cursor < bytes.len() {
        if bytes[cursor] != b'>' {
            break;
        }
        cursor += 1;
        if cursor < bytes.len() && bytes[cursor] == b' ' {
            cursor += 1;
        }
        while cursor < bytes.len() && bytes[cursor] == b' ' {
            cursor += 1;
        }
        if let Some(nested) = direct_at(line, cursor) {
            return Some(nested);
        }
    }

    None
}

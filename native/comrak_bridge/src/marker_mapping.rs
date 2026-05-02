use crate::payload::{JsonBlock, JsonRange};
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

pub(crate) fn collect_inline_marker_ranges(text: &str, exclusions: &[JsonRange]) -> Vec<JsonRange> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    if len == 0 {
        return Vec::new();
    }

    let mut runs = Vec::new();
    let mut cursor = 0usize;
    let mut exclusion_index = 0usize;

    let mut bold_start: Option<usize> = None;
    let mut italic_start: Option<usize> = None;
    let mut code_start: Option<usize> = None;

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
                // Exclusions terminate in-progress inline wrappers.
                bold_start = None;
                italic_start = None;
                code_start = None;
                cursor = exclusion_end.min(len);
                continue;
            }
        }

        let byte = bytes[cursor];
        let escaped_inline_delimiter = code_start.is_none()
            && matches!(byte, b'`' | b'*' | b'_')
            && is_escaped_at(bytes, cursor);
        if escaped_inline_delimiter {
            cursor += 1;
            continue;
        }

        if byte == b'`' {
            if code_start.is_none() {
                code_start = Some(cursor);
            } else {
                let start = code_start.expect("code_start just checked for Some");
                add_style_run(&mut runs, start, cursor + 1, InlineMarkerStyle::Code);
                code_start = None;
            }
        }
        // Bold / italic with `*` (mirrors SovereignStyleScanner)
        else if code_start.is_none() && byte == b'*' {
            if cursor + 1 < len && bytes[cursor + 1] == b'*' {
                if bold_start.is_none() {
                    bold_start = Some(cursor);
                    cursor += 1;
                } else {
                    let start = bold_start.expect("bold_start just checked for Some");
                    add_style_run(&mut runs, start, cursor + 2, InlineMarkerStyle::Bold);
                    bold_start = None;
                    cursor += 1;
                }
            } else {
                if italic_start.is_none() {
                    italic_start = Some(cursor);
                } else if let Some(start) = italic_start {
                    if bytes.get(start) == Some(&b'*') {
                        add_style_run(&mut runs, start, cursor + 1, InlineMarkerStyle::Italic);
                        italic_start = None;
                    }
                }
            }
        }
        // Italic with `_` (mirrors SovereignStyleScanner)
        else if code_start.is_none() && byte == b'_' {
            if italic_start.is_none() {
                italic_start = Some(cursor);
            } else if let Some(start) = italic_start {
                if bytes.get(start) == Some(&b'_') {
                    add_style_run(&mut runs, start, cursor + 1, InlineMarkerStyle::Italic);
                    italic_start = None;
                }
            }
        }

        cursor += 1;
    }

    let mut ranges = Vec::new();
    for run in runs {
        match run.style {
            InlineMarkerStyle::Bold => {
                if run.end.saturating_sub(run.start) > 4 {
                    push_range(&mut ranges, run.start, run.start + 2, len);
                    push_range(&mut ranges, run.end.saturating_sub(2), run.end, len);
                }
            }
            InlineMarkerStyle::Italic | InlineMarkerStyle::Code => {
                if run.end.saturating_sub(run.start) > 2 {
                    push_range(&mut ranges, run.start, run.start + 1, len);
                    push_range(&mut ranges, run.end.saturating_sub(1), run.end, len);
                }
            }
        }
    }

    normalize_ranges(ranges, len)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InlineMarkerStyle {
    Bold,
    Italic,
    Code,
}

#[derive(Clone, Copy)]
struct InlineStyleRun {
    start: usize,
    end: usize,
    style: InlineMarkerStyle,
}

fn add_style_run(
    runs: &mut Vec<InlineStyleRun>,
    start: usize,
    end: usize,
    style: InlineMarkerStyle,
) {
    if end <= start {
        return;
    }
    if let Some(last) = runs.last().copied() {
        if last.end == start && last.style == style {
            let last_index = runs.len() - 1;
            runs[last_index] = InlineStyleRun {
                start: last.start,
                end,
                style,
            };
            return;
        }
    }
    runs.push(InlineStyleRun { start, end, style });
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

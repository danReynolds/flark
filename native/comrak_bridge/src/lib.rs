use comrak::nodes::{AstNode, ListType, NodeHeading, NodeList, NodeValue, Sourcepos, TableAlignment};
use comrak::{parse_document, Arena, Options};
use serde::Serialize;

const ABI_VERSION: u32 = 1;
const STATUS_OK: u16 = 0;
const STATUS_ERROR: u16 = 1;

#[repr(C)]
pub struct SovereignComrakResponse {
    pub abi_version: u32,
    pub revision: u32,
    pub status_code: u16,
    pub reserved: u16,
    pub payload_ptr: *mut u8,
    pub payload_len: u32,
}

#[no_mangle]
pub extern "C" fn sovereign_comrak_bridge_version() -> u32 {
    ABI_VERSION
}

#[no_mangle]
pub extern "C" fn sovereign_comrak_parse(
    revision: u32,
    profile: u8,
    text_ptr: *const u8,
    text_len: u32,
) -> *mut SovereignComrakResponse {
    if profile > 1 {
        return allocate_response(
            revision,
            STATUS_ERROR,
            diagnostic_payload("Unsupported profile in comrak bridge."),
        );
    }

    if text_len > 0 && text_ptr.is_null() {
        return allocate_response(
            revision,
            STATUS_ERROR,
            diagnostic_payload("Received null text pointer with non-zero length."),
        );
    }

    let input_bytes = if text_ptr.is_null() || text_len == 0 {
        &[][..]
    } else {
        // SAFETY: pointer/length are validated above and only read for this call.
        unsafe { std::slice::from_raw_parts(text_ptr, text_len as usize) }
    };

    let text = match std::str::from_utf8(input_bytes) {
        Ok(text) => text,
        Err(_) => {
            return allocate_response(
                revision,
                STATUS_ERROR,
                diagnostic_payload("Invalid UTF-8 input."),
            )
        }
    };

    let parse = parse_to_payload(text, profile);
    match parse {
        Ok(payload) => allocate_response(revision, STATUS_OK, payload),
        Err(message) => allocate_response(revision, STATUS_ERROR, diagnostic_payload(&message)),
    }
}

#[no_mangle]
pub extern "C" fn sovereign_comrak_response_free(response_ptr: *mut SovereignComrakResponse) {
    if response_ptr.is_null() {
        return;
    }

    // SAFETY: Caller guarantees pointer originates from `sovereign_comrak_parse`.
    let response = unsafe { Box::from_raw(response_ptr) };

    if !response.payload_ptr.is_null() && response.payload_len > 0 {
        // SAFETY: payload pointer/len originate from `allocate_response`.
        unsafe {
            let _ = Vec::from_raw_parts(
                response.payload_ptr,
                response.payload_len as usize,
                response.payload_len as usize,
            );
        }
    }
}

fn allocate_response(
    revision: u32,
    status_code: u16,
    mut payload: Vec<u8>,
) -> *mut SovereignComrakResponse {
    let payload_len = payload.len() as u32;
    let payload_ptr = if payload.is_empty() {
        std::ptr::null_mut()
    } else {
        let ptr = payload.as_mut_ptr();
        std::mem::forget(payload);
        ptr
    };
    let payload_len = if payload_ptr.is_null() {
        0
    } else {
        payload_len
    };

    let response = SovereignComrakResponse {
        abi_version: ABI_VERSION,
        revision,
        status_code,
        reserved: 0,
        payload_ptr,
        payload_len,
    };
    Box::into_raw(Box::new(response))
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonRange {
    start_byte: u32,
    end_byte: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonBlock {
    #[serde(rename = "type")]
    kind: &'static str,
    start_byte: u32,
    end_byte: u32,
    payload: serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonInlineToken {
    styles: Vec<&'static str>,
    start_byte: u32,
    end_byte: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonDiagnostic {
    start_byte: u32,
    end_byte: u32,
    message: String,
    code: &'static str,
    is_error: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonParsePayload {
    blocks: Vec<JsonBlock>,
    inline_tokens: Vec<JsonInlineToken>,
    marker_ranges: Vec<JsonRange>,
    exclusion_ranges: Vec<JsonRange>,
    diagnostics: Vec<JsonDiagnostic>,
}

struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    fn from_text(text: &str) -> Self {
        let mut line_starts = vec![0];
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(offset + 1);
            }
        }
        Self { line_starts }
    }

    fn line_start(&self, line: usize, text_len: usize) -> usize {
        if line == 0 {
            return 0;
        }
        self.line_starts
            .get(line - 1)
            .copied()
            .unwrap_or(text_len)
            .min(text_len)
    }

    fn line_end_content(&self, line: usize, text: &str) -> usize {
        let text_len = text.len();
        let start = self.line_start(line, text_len);
        let next = self
            .line_starts
            .get(line)
            .copied()
            .unwrap_or(text_len)
            .min(text_len);
        let line_slice = &text[start..next];
        if line_slice.ends_with('\n') {
            next.saturating_sub(1)
        } else {
            next
        }
    }

    fn line_end_with_break(&self, line: usize, text: &str) -> usize {
        self.line_starts
            .get(line)
            .copied()
            .unwrap_or(text.len())
            .min(text.len())
    }

    fn column_start_offset(&self, line: usize, column: usize, text: &str) -> usize {
        let text_len = text.len();
        let start = self.line_start(line, text_len);
        let content_end = self.line_end_content(line, text);
        if column <= 1 || start >= content_end {
            return start.min(content_end);
        }

        let mut byte = start;
        let mut remaining = column.saturating_sub(1);
        for ch in text[start..content_end].chars() {
            if remaining == 0 {
                break;
            }
            byte += ch.len_utf8();
            remaining -= 1;
        }
        byte.min(content_end)
    }

    fn column_end_offset(&self, line: usize, column: usize, text: &str) -> usize {
        let content_end = self.line_end_content(line, text);
        let start = self.column_start_offset(line, column, text);
        if start >= content_end {
            return content_end;
        }
        let mut iter = text[start..content_end].chars();
        match iter.next() {
            Some(ch) => (start + ch.len_utf8()).min(content_end),
            None => content_end,
        }
    }
}

fn parse_to_payload(text: &str, profile: u8) -> Result<Vec<u8>, String> {
    let mut options = Options::default();
    if profile == 1 {
        options.extension.autolink = true;
        options.extension.strikethrough = true;
        options.extension.table = true;
        options.extension.tagfilter = true;
        options.extension.tasklist = true;
    }

    let arena = Arena::new();
    let root = parse_document(&arena, text, &options);
    let line_index = LineIndex::from_text(text);

    let mut blocks = Vec::new();
    let mut inline_tokens = Vec::new();
    let mut exclusion_ranges = Vec::new();

    for node in root.descendants() {
        collect_node(
            node,
            text,
            &line_index,
            &mut blocks,
            &mut inline_tokens,
            &mut exclusion_ranges,
        );
    }
    let exclusion_ranges = normalize_ranges(exclusion_ranges, text.len());
    let mut marker_ranges = collect_block_marker_ranges(text, &blocks, profile == 1);
    marker_ranges.extend(collect_inline_marker_ranges(text, &exclusion_ranges));
    let marker_ranges = normalize_ranges(marker_ranges, text.len());

    let payload = JsonParsePayload {
        blocks,
        inline_tokens,
        marker_ranges,
        exclusion_ranges,
        diagnostics: Vec::new(),
    };
    serde_json::to_vec(&payload).map_err(|error| error.to_string())
}

fn collect_node<'a>(
    node: &'a AstNode<'a>,
    text: &str,
    line_index: &LineIndex,
    blocks: &mut Vec<JsonBlock>,
    inline_tokens: &mut Vec<JsonInlineToken>,
    exclusion_ranges: &mut Vec<JsonRange>,
) {
    let data = node.data.borrow();
    let sourcepos = data.sourcepos;
    let Some(inline_range) = sourcepos_to_range(sourcepos, text, line_index) else {
        return;
    };
    if inline_range.end_byte <= inline_range.start_byte {
        return;
    }
    let line_range = expand_range_to_full_lines(text, inline_range);
    let inside_list_or_quote = has_list_or_quote_ancestor(node);

    match &data.value {
        NodeValue::Paragraph => blocks.push(JsonBlock {
            kind: "paragraph",
            start_byte: line_range.start_byte,
            end_byte: line_range.end_byte,
            payload: serde_json::Value::Object(Default::default()),
        }),
        NodeValue::Heading(NodeHeading { level, .. }) => {
            if inside_list_or_quote {
                return;
            }
            let heading_start = trim_heading_leading_reference_defs(
                text,
                line_range.start_byte,
                line_range.end_byte,
            );
            blocks.push(JsonBlock {
                kind: "header",
                start_byte: heading_start,
                end_byte: line_range.end_byte,
                payload: serde_json::json!({ "level": level }),
            });
        }
        NodeValue::ThematicBreak => {
            if inside_list_or_quote {
                return;
            }
            blocks.push(JsonBlock {
                kind: "thematic_break",
                start_byte: line_range.start_byte,
                end_byte: line_range.end_byte,
                payload: serde_json::Value::Object(Default::default()),
            });
        }
        NodeValue::CodeBlock(code_block) => {
            if inside_list_or_quote {
                return;
            }

            let code_line_range = if code_block.fenced {
                line_range
            } else {
                trim_blank_code_block_lines(text, line_range)
            };
            if code_line_range.end_byte <= code_line_range.start_byte {
                return;
            }

            blocks.push(JsonBlock {
                kind: "fenced_code",
                start_byte: code_line_range.start_byte,
                end_byte: code_line_range.end_byte,
                payload: extract_code_payload(&code_block.info),
            });
            exclusion_ranges.push(JsonRange {
                start_byte: code_line_range.start_byte,
                end_byte: code_line_range.end_byte,
            });
        }
        NodeValue::BlockQuote => {
            if inside_list_or_quote {
                return;
            }
            blocks.push(JsonBlock {
                kind: "blockquote",
                start_byte: line_range.start_byte,
                end_byte: line_range.end_byte,
                payload: serde_json::Value::Object(Default::default()),
            });
        }
        NodeValue::List(NodeList { list_type, .. }) => {
            let kind = match list_type {
                ListType::Bullet => "unordered_list",
                ListType::Ordered => "ordered_list",
            };
            blocks.push(JsonBlock {
                kind,
                start_byte: line_range.start_byte,
                end_byte: line_range.end_byte,
                payload: serde_json::Value::Object(Default::default()),
            });
        }
        NodeValue::Table(table) => {
            if inside_list_or_quote {
                return;
            }
            let alignments: Vec<&'static str> = table
                .alignments
                .iter()
                .map(|alignment| match alignment {
                    TableAlignment::None => "none",
                    TableAlignment::Left => "left",
                    TableAlignment::Center => "center",
                    TableAlignment::Right => "right",
                })
                .collect();
            blocks.push(JsonBlock {
                kind: "table",
                start_byte: line_range.start_byte,
                end_byte: line_range.end_byte,
                payload: serde_json::json!({
                    "columns": table.num_columns,
                    "rows": table.num_rows,
                    "alignments": alignments,
                }),
            });
        }
        NodeValue::Strong => inline_tokens.push(JsonInlineToken {
            styles: vec!["bold"],
            start_byte: inline_range.start_byte,
            end_byte: inline_range.end_byte,
        }),
        NodeValue::Emph => inline_tokens.push(JsonInlineToken {
            styles: vec!["italic"],
            start_byte: inline_range.start_byte,
            end_byte: inline_range.end_byte,
        }),
        NodeValue::Code(_) => inline_tokens.push(JsonInlineToken {
            styles: vec!["code"],
            start_byte: inline_range.start_byte,
            end_byte: inline_range.end_byte,
        }),
        NodeValue::Image(_) => inline_tokens.push(JsonInlineToken {
            styles: vec!["image"],
            start_byte: inline_range.start_byte,
            end_byte: inline_range.end_byte,
        }),
        _ => {}
    }
}

fn has_list_or_quote_ancestor<'a>(node: &'a AstNode<'a>) -> bool {
    let mut parent = node.parent();
    while let Some(ancestor) = parent {
        let is_list_or_quote = {
            let value = &ancestor.data.borrow().value;
            matches!(
                value,
                NodeValue::List(_) | NodeValue::Item(_) | NodeValue::BlockQuote
            )
        };
        if is_list_or_quote {
            return true;
        }
        parent = ancestor.parent();
    }
    false
}

fn trim_blank_code_block_lines(text: &str, range: JsonRange) -> JsonRange {
    let text_len = text.len();
    let mut start = (range.start_byte as usize).min(text_len);
    let mut end = (range.end_byte as usize).min(text_len);

    while start < end {
        let line_end = end_of_line(text, start).min(end);
        let content_end = line_content_end(text, start).min(line_end);
        let line = &text[start..content_end];
        if !line.trim_matches(|ch| ch == ' ' || ch == '\t').is_empty() {
            break;
        }
        if line_end <= start {
            break;
        }
        start = line_end;
    }

    while start < end {
        let line_start = line_start_for_range_end(text, end);
        if line_start < start {
            break;
        }
        let content_end = line_content_end(text, line_start).min(end);
        let line = &text[line_start..content_end];
        if !line.trim_matches(|ch| ch == ' ' || ch == '\t').is_empty() {
            break;
        }
        if line_start >= end {
            break;
        }
        end = line_start;
    }

    JsonRange {
        start_byte: start as u32,
        end_byte: end as u32,
    }
}

fn line_start_for_range_end(text: &str, end: usize) -> usize {
    if end == 0 || text.is_empty() {
        return 0;
    }

    let bytes = text.as_bytes();
    let mut idx = end.min(bytes.len()).saturating_sub(1);
    if bytes[idx] == b'\n' {
        if idx == 0 {
            return 0;
        }
        idx -= 1;
    }

    while idx > 0 {
        if bytes[idx - 1] == b'\n' {
            return idx;
        }
        idx -= 1;
    }
    0
}

fn trim_heading_leading_reference_defs(text: &str, start_byte: u32, end_byte: u32) -> u32 {
    let mut start = start_byte as usize;
    let end = end_byte as usize;
    while start < end {
        let content_end = line_content_end(text, start).min(end);
        let line = &text[start..content_end];
        if !is_link_reference_definition_line(line) {
            break;
        }
        let next = end_of_line(text, start).min(end);
        if next <= start || next >= end {
            break;
        }
        start = next;
    }
    start as u32
}

fn is_link_reference_definition_line(line: &str) -> bool {
    let (indent_columns, marker_start) = leading_indent(line);
    if indent_columns > 3 || marker_start >= line.len() {
        return false;
    }
    let remainder = &line[marker_start..];
    if !remainder.starts_with('[') {
        return false;
    }
    let Some(close) = remainder.find("]:") else {
        return false;
    };
    if close <= 1 {
        return false;
    }
    let after = remainder[close + 2..].trim_start_matches(|ch| ch == ' ' || ch == '\t');
    !after.is_empty()
}

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

fn collect_block_marker_ranges(text: &str, blocks: &[JsonBlock], is_gfm: bool) -> Vec<JsonRange> {
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

fn collect_inline_marker_ranges(text: &str, exclusions: &[JsonRange]) -> Vec<JsonRange> {
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

fn push_range(ranges: &mut Vec<JsonRange>, start: usize, end: usize, text_len: usize) {
    if end <= start || start >= text_len {
        return;
    }
    let clamped_end = end.min(text_len);
    if clamped_end <= start {
        return;
    }
    ranges.push(JsonRange {
        start_byte: start as u32,
        end_byte: clamped_end as u32,
    });
}

fn normalize_ranges(mut ranges: Vec<JsonRange>, text_len: usize) -> Vec<JsonRange> {
    let max = text_len as u32;
    for range in &mut ranges {
        if range.start_byte > max {
            range.start_byte = max;
        }
        if range.end_byte > max {
            range.end_byte = max;
        }
    }
    ranges.retain(|range| range.end_byte > range.start_byte);
    ranges.sort_by(|a, b| {
        a.start_byte
            .cmp(&b.start_byte)
            .then(a.end_byte.cmp(&b.end_byte))
    });

    let mut deduped: Vec<JsonRange> = Vec::new();
    for range in ranges {
        if let Some(last) = deduped.last() {
            if range.start_byte < last.end_byte {
                continue;
            }
            if range.start_byte == last.start_byte && range.end_byte == last.end_byte {
                continue;
            }
        }
        deduped.push(range);
    }
    deduped
}

fn for_each_line_in_range<F>(text: &str, start: usize, end: usize, mut visitor: F)
where
    F: FnMut(usize, &str),
{
    let mut cursor = start.min(text.len());
    let bound = end.min(text.len());
    while cursor < bound {
        let line_end = end_of_line(text, cursor).min(bound);
        let content_end = line_content_end(text, cursor).min(line_end);
        visitor(cursor, &text[cursor..content_end]);
        if line_end <= cursor {
            break;
        }
        cursor = line_end;
    }
}

fn end_of_line(text: &str, start: usize) -> usize {
    if start >= text.len() {
        return text.len();
    }
    match text[start..].find('\n') {
        Some(index) => start + index + 1,
        None => text.len(),
    }
}

fn line_content_end(text: &str, line_start: usize) -> usize {
    let mut end = end_of_line(text, line_start);
    if end > line_start && text.as_bytes()[end - 1] == b'\n' {
        end -= 1;
        if end > line_start && text.as_bytes()[end - 1] == b'\r' {
            end -= 1;
        }
    }
    end
}

fn line_start_for_offset(text: &str, offset: usize) -> usize {
    if text.is_empty() {
        return 0;
    }

    let bytes = text.as_bytes();
    let mut safe = offset.min(bytes.len() - 1);
    if bytes[safe] == b'\n' && safe > 0 {
        safe -= 1;
    }

    while safe > 0 {
        if bytes[safe - 1] == b'\n' {
            return safe;
        }
        safe -= 1;
    }
    0
}

fn leading_indent(line: &str) -> (usize, usize) {
    let bytes = line.as_bytes();
    let mut columns = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b' ' => {
                columns += 1;
                index += 1;
            }
            b'\t' => {
                columns += 4 - (columns % 4);
                index += 1;
            }
            _ => break,
        }
    }
    (columns, index)
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

fn sourcepos_to_range(
    sourcepos: Sourcepos,
    text: &str,
    line_index: &LineIndex,
) -> Option<JsonRange> {
    if sourcepos.start.line == 0 || sourcepos.end.line == 0 {
        return None;
    }

    let start = line_index.column_start_offset(
        sourcepos.start.line as usize,
        sourcepos.start.column as usize,
        text,
    );
    let mut end = line_index.column_end_offset(
        sourcepos.end.line as usize,
        sourcepos.end.column as usize,
        text,
    );

    if end < start {
        end = start;
    }

    if sourcepos.start.line != sourcepos.end.line {
        let line_end = line_index.line_end_with_break(sourcepos.end.line as usize, text);
        if line_end > end {
            end = line_end;
        }
    }

    Some(JsonRange {
        start_byte: start as u32,
        end_byte: end as u32,
    })
}

fn expand_range_to_full_lines(text: &str, range: JsonRange) -> JsonRange {
    let text_len = text.len();
    if text_len == 0 {
        return JsonRange {
            start_byte: 0,
            end_byte: 0,
        };
    }

    let start = line_start_for_offset(text, (range.start_byte as usize).min(text_len - 1));
    let end_anchor = if range.end_byte == 0 {
        start
    } else {
        (range.end_byte as usize)
            .saturating_sub(1)
            .min(text_len.saturating_sub(1))
    };
    let end_line_start = line_start_for_offset(text, end_anchor);
    let end = end_of_line(text, end_line_start).min(text_len);

    JsonRange {
        start_byte: start as u32,
        end_byte: end as u32,
    }
}

fn extract_code_payload(info: &str) -> serde_json::Value {
    let language = info.split_whitespace().next().unwrap_or("");
    if language.is_empty() {
        serde_json::Value::Object(Default::default())
    } else {
        serde_json::json!({ "language": language })
    }
}

fn diagnostic_payload(message: &str) -> Vec<u8> {
    let escaped = message
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!(
        "{{\"blocks\":[],\"inlineTokens\":[],\"markerRanges\":[],\"exclusionRanges\":[],\"diagnostics\":[{{\"startByte\":0,\"endByte\":0,\"message\":\"{}\",\"code\":\"COMRAK_BRIDGE_ERROR\",\"isError\":true}}]}}",
        escaped
    )
    .into_bytes()
}

use crate::marker_mapping::{collect_block_marker_ranges, collect_inline_marker_ranges};
use crate::payload::{JsonBlock, JsonInlineToken, JsonParsePayload, JsonRange};
use crate::source_ranges::{
    end_of_line, leading_indent, line_content_end, line_start_for_offset, normalize_ranges,
};
use comrak::nodes::{
    AstNode, ListType, NodeHeading, NodeList, NodeValue, Sourcepos, TableAlignment,
};
use comrak::{parse_document, Arena, Options};

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

pub(crate) fn parse_to_payload(text: &str, profile: u8) -> Result<Vec<u8>, String> {
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

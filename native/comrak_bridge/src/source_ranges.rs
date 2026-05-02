use crate::payload::JsonRange;

pub(crate) fn push_range(ranges: &mut Vec<JsonRange>, start: usize, end: usize, text_len: usize) {
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

pub(crate) fn normalize_ranges(mut ranges: Vec<JsonRange>, text_len: usize) -> Vec<JsonRange> {
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

pub(crate) fn for_each_line_in_range<F>(text: &str, start: usize, end: usize, mut visitor: F)
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

pub(crate) fn end_of_line(text: &str, start: usize) -> usize {
    if start >= text.len() {
        return text.len();
    }
    match text[start..].find('\n') {
        Some(index) => start + index + 1,
        None => text.len(),
    }
}

pub(crate) fn line_content_end(text: &str, line_start: usize) -> usize {
    let mut end = end_of_line(text, line_start);
    if end > line_start && text.as_bytes()[end - 1] == b'\n' {
        end -= 1;
        if end > line_start && text.as_bytes()[end - 1] == b'\r' {
            end -= 1;
        }
    }
    end
}

pub(crate) fn line_start_for_offset(text: &str, offset: usize) -> usize {
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

pub(crate) fn leading_indent(line: &str) -> (usize, usize) {
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

use std::collections::HashMap;

use crate::block_machine::{
    atx_heading as parse_atx_heading_probe, fence_open, is_blank, is_fence_close as fence_close,
    is_thematic_break, leading_spaces, quote_prefix, setext_level,
};
use crate::{
    Block, BlockKind, CoverageLeaf, Document, Inline, InlineKind, Marker, MarkerKind, SourceRange,
};

#[derive(Clone, Copy, Debug)]
struct Line<'a> {
    start: usize,
    content_end: usize,
    end: usize,
    content: &'a str,
}

#[derive(Clone, Debug)]
struct LogicalText {
    text: String,
    /// Original source byte offset for every logical byte boundary.
    map: Vec<usize>,
}

impl LogicalText {
    fn new() -> Self {
        Self {
            text: String::new(),
            map: vec![0],
        }
    }

    fn push_slice(&mut self, text: &str, original_start: usize) {
        if self.text.is_empty() {
            self.map[0] = original_start;
        }
        self.text.push_str(text);
        self.map
            .extend((1..=text.len()).map(|offset| original_start + offset));
    }

    fn push_mapped_char(&mut self, character: char, original_start: usize, original_end: usize) {
        if self.text.is_empty() {
            self.map[0] = original_start;
        }
        let encoded = character.len_utf8();
        self.text.push(character);
        for byte in 1..=encoded {
            self.map.push(if byte == encoded {
                original_end
            } else {
                original_start
            });
        }
    }

    fn range(&self, start: usize, end: usize) -> SourceRange {
        SourceRange::new(self.map[start], self.map[end])
    }
}

pub fn parse(source: &str) -> Document {
    assert!(!source.contains('\r'), "parser input must be LF-normalized");
    let lines = lines(source);
    let mut next_id = 1;
    let blocks = parse_lines(source, &lines, &mut next_id);
    let coverage = build_coverage(source.len(), &blocks);
    Document {
        source_len: source.len(),
        blocks,
        coverage,
    }
}

fn lines(source: &str) -> Vec<Line<'_>> {
    let mut output = Vec::new();
    let mut start = 0;
    while start < source.len() {
        let end = source[start..]
            .find('\n')
            .map_or(source.len(), |relative| start + relative + 1);
        let content_end = if end > start && source.as_bytes()[end - 1] == b'\n' {
            end - 1
        } else {
            end
        };
        output.push(Line {
            start,
            content_end,
            end,
            content: &source[start..content_end],
        });
        start = end;
    }
    output
}

fn parse_lines(source: &str, lines: &[Line<'_>], next_id: &mut u64) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if is_blank(line.content) {
            index += 1;
            continue;
        }

        if let Some((block, next)) = parse_block_quote(source, lines, index, next_id) {
            blocks.push(block);
            index = next;
            continue;
        }
        if let Some((block, next)) = parse_fenced_code(lines, index, next_id) {
            blocks.push(block);
            index = next;
            continue;
        }
        if let Some(block) = parse_atx_heading(line, next_id) {
            blocks.push(block);
            index += 1;
            continue;
        }
        if is_thematic_break(line.content) {
            blocks.push(Block {
                id: take_id(next_id),
                kind: BlockKind::ThematicBreak,
                range: SourceRange::new(line.start, line.content_end),
                content_range: SourceRange::new(line.start, line.content_end),
                markers: Vec::new(),
                inlines: Vec::new(),
                children: Vec::new(),
                literal: String::new(),
            });
            index += 1;
            continue;
        }
        if leading_spaces(line.content) >= 4 {
            let (block, next) = parse_indented_code(lines, index, next_id);
            blocks.push(block);
            index = next;
            continue;
        }

        let (block, next) = parse_paragraph(lines, index, next_id);
        blocks.push(block);
        index = next;
    }
    blocks
}

fn parse_block_quote(
    _source: &str,
    lines: &[Line<'_>],
    start: usize,
    next_id: &mut u64,
) -> Option<(Block, usize)> {
    let first_prefix = quote_prefix(lines[start].content)?;
    let mut index = start;
    let mut inner = String::new();
    let mut map = Vec::new();
    let mut markers = Vec::new();
    while index < lines.len() {
        let line = lines[index];
        let Some(prefix) = quote_prefix(line.content) else {
            break;
        };
        let marker_start = line.start + prefix.marker_offset;
        markers.push(Marker {
            kind: MarkerKind::Quote,
            range: SourceRange::new(marker_start, marker_start + 1),
        });
        let content_start = line.start + prefix.content_offset;
        let content = &line.content[prefix.content_offset..];
        let inner_start = inner.len();
        inner.push_str(content);
        if line.end > line.content_end {
            inner.push('\n');
        }
        map.push((
            inner_start,
            content_start,
            content.len(),
            line.content_end,
            line.end,
        ));
        index += 1;
    }
    if index == start {
        return None;
    }

    let mut children = parse(&inner).blocks;
    for child in &mut children {
        remap_block(child, &map);
    }
    let last = lines[index - 1];
    Some((
        Block {
            id: take_id(next_id),
            kind: BlockKind::BlockQuote,
            range: SourceRange::new(lines[start].start, last.content_end),
            content_range: SourceRange::new(
                lines[start].start + first_prefix.content_offset,
                last.content_end,
            ),
            markers,
            inlines: Vec::new(),
            children,
            literal: String::new(),
        },
        index,
    ))
}

fn remap_block(block: &mut Block, map: &[(usize, usize, usize, usize, usize)]) {
    block.range = remap_range(block.range, map);
    block.content_range = remap_range(block.content_range, map);
    for marker in &mut block.markers {
        marker.range = remap_range(marker.range, map);
    }
    for inline in &mut block.inlines {
        remap_inline(inline, map);
    }
    for child in &mut block.children {
        remap_block(child, map);
    }
}

fn remap_inline(inline: &mut Inline, map: &[(usize, usize, usize, usize, usize)]) {
    inline.range = remap_range(inline.range, map);
    for child in &mut inline.children {
        remap_inline(child, map);
    }
}

fn remap_range(range: SourceRange, map: &[(usize, usize, usize, usize, usize)]) -> SourceRange {
    SourceRange::new(remap_offset(range.start, map), remap_offset(range.end, map))
}

fn remap_offset(offset: usize, map: &[(usize, usize, usize, usize, usize)]) -> usize {
    for &(inner_start, original_start, content_len, original_content_end, original_end) in map {
        if offset <= inner_start + content_len {
            return original_start + offset.saturating_sub(inner_start).min(content_len);
        }
        if offset < inner_start + content_len + (original_end > original_content_end) as usize {
            return original_content_end;
        }
    }
    map.last().map_or(0, |entry| entry.4)
}

fn parse_fenced_code(
    lines: &[Line<'_>],
    start: usize,
    next_id: &mut u64,
) -> Option<(Block, usize)> {
    let opening = fence_open(lines[start].content)?;
    let mut literal = String::new();
    let mut index = start + 1;
    let mut close = None;
    let mut content_start = lines[start].end;
    let mut content_end = content_start;
    while index < lines.len() {
        let line = lines[index];
        if fence_close(line.content, opening.marker, opening.length) {
            close = Some(index);
            break;
        }
        let remove = removable_indent(line.content, opening.indent);
        let content = &line.content[remove..];
        if literal.is_empty() {
            content_start = line.start + remove;
        }
        literal.push_str(content);
        if line.end > line.content_end {
            literal.push('\n');
        }
        content_end = line.content_end;
        index += 1;
    }
    let end_index = close.map_or(lines.len(), |close| close + 1);
    let last = if end_index == 0 {
        lines[start]
    } else {
        lines[end_index - 1]
    };
    let mut markers = vec![Marker {
        kind: MarkerKind::FenceOpen,
        range: SourceRange::new(
            lines[start].start + opening.indent,
            lines[start].start + opening.indent + opening.length,
        ),
    }];
    if let Some(close) = close {
        let indent = leading_spaces(lines[close].content);
        let length = lines[close].content[indent..]
            .bytes()
            .take_while(|byte| *byte == opening.marker)
            .count();
        markers.push(Marker {
            kind: MarkerKind::FenceClose,
            range: SourceRange::new(
                lines[close].start + indent,
                lines[close].start + indent + length,
            ),
        });
    }
    Some((
        Block {
            id: take_id(next_id),
            kind: BlockKind::FencedCode {
                info: opening.info.to_owned(),
            },
            range: SourceRange::new(lines[start].start, last.content_end),
            content_range: SourceRange::new(content_start, content_end),
            markers,
            inlines: Vec::new(),
            children: Vec::new(),
            literal,
        },
        end_index,
    ))
}

fn parse_atx_heading(line: Line<'_>, next_id: &mut u64) -> Option<Block> {
    let indent = leading_spaces(line.content);
    if indent > 3 {
        return None;
    }
    let rest = &line.content[indent..];
    let level = rest.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    if rest
        .as_bytes()
        .get(level)
        .is_some_and(|byte| !matches!(byte, b' ' | b'\t'))
    {
        return None;
    }
    let after_open = indent + level;
    let mut content_start = after_open;
    while matches!(
        line.content.as_bytes().get(content_start),
        Some(b' ' | b'\t')
    ) {
        content_start += 1;
    }
    let mut content_end = line.content.len();
    while matches!(
        line.content.as_bytes().get(content_end.saturating_sub(1)),
        Some(b' ' | b'\t')
    ) {
        content_end = content_end.saturating_sub(1);
    }
    content_end = content_end.max(content_start);
    let mut close_start = None;
    let mut cursor = content_end;
    while cursor > content_start && line.content.as_bytes()[cursor - 1] == b'#' {
        cursor -= 1;
    }
    if cursor < content_end
        && (cursor == content_start
            || matches!(line.content.as_bytes().get(cursor - 1), Some(b' ' | b'\t')))
    {
        close_start = Some(cursor);
        content_end = cursor;
        while content_end > content_start
            && matches!(
                line.content.as_bytes().get(content_end.saturating_sub(1)),
                Some(b' ' | b'\t')
            )
        {
            content_end = content_end.saturating_sub(1);
        }
    }

    let logical = logical_from_slice(
        &line.content[content_start..content_end],
        line.start + content_start,
    );
    let mut markers = vec![Marker {
        kind: MarkerKind::HeadingOpen,
        range: SourceRange::new(line.start + indent, line.start + indent + level),
    }];
    if let Some(close_start) = close_start {
        markers.push(Marker {
            kind: MarkerKind::HeadingClose,
            range: SourceRange::new(
                line.start + close_start,
                line.start + line.content.trim_ascii_end().len(),
            ),
        });
    }
    Some(Block {
        id: take_id(next_id),
        kind: BlockKind::Heading { level: level as u8 },
        range: SourceRange::new(line.start, line.content_end),
        content_range: SourceRange::new(line.start + content_start, line.start + content_end),
        markers,
        inlines: parse_inlines(&logical),
        children: Vec::new(),
        literal: String::new(),
    })
}

fn parse_indented_code(lines: &[Line<'_>], start: usize, next_id: &mut u64) -> (Block, usize) {
    let mut index = start;
    let mut literal = String::new();
    let mut last_nonblank_len = 0;
    while index < lines.len() {
        let line = lines[index];
        if is_blank(line.content) {
            literal.push('\n');
            index += 1;
            continue;
        }
        if leading_spaces(line.content) < 4 {
            break;
        }
        literal.push_str(&line.content[4..]);
        if line.end > line.content_end {
            literal.push('\n');
        }
        last_nonblank_len = literal.len();
        index += 1;
    }
    literal.truncate(last_nonblank_len);
    let last = lines[index.saturating_sub(1).max(start)];
    (
        Block {
            id: take_id(next_id),
            kind: BlockKind::IndentedCode,
            range: SourceRange::new(lines[start].start, last.content_end),
            content_range: SourceRange::new(lines[start].start + 4, last.content_end),
            markers: Vec::new(),
            inlines: Vec::new(),
            children: Vec::new(),
            literal,
        },
        index,
    )
}

fn parse_paragraph(lines: &[Line<'_>], start: usize, next_id: &mut u64) -> (Block, usize) {
    let mut index = start;
    let mut logical = LogicalText::new();
    let mut last = lines[start];
    let mut heading_level = None;
    while index < lines.len() {
        let line = lines[index];
        if is_blank(line.content) {
            break;
        }
        if index > start {
            if let Some(level) = setext_level(line.content) {
                heading_level = Some(level);
                last = line;
                index += 1;
                break;
            }
            if quote_prefix(line.content).is_some()
                || fence_open(line.content).is_some()
                || parse_atx_heading_probe(line.content)
                || is_thematic_break(line.content)
            {
                break;
            }
        }
        let leading = leading_spaces(line.content);
        let content_start = leading.min(line.content.len());
        let content = &line.content[content_start..];
        if !logical.text.is_empty() {
            logical.push_mapped_char('\n', last.content_end, last.end);
        }
        logical.push_slice(content, line.start + content_start);
        last = line;
        index += 1;
    }
    let first_content_start = lines[start].start + leading_spaces(lines[start].content);
    let content_end = if heading_level.is_some() {
        logical.map.last().copied().unwrap_or(first_content_start)
    } else {
        last.content_end
    };
    let kind = heading_level.map_or(BlockKind::Paragraph, |level| BlockKind::Heading { level });
    (
        Block {
            id: take_id(next_id),
            kind,
            range: SourceRange::new(lines[start].start, last.content_end),
            content_range: SourceRange::new(first_content_start, content_end),
            markers: Vec::new(),
            inlines: parse_inlines(&logical),
            children: Vec::new(),
            literal: String::new(),
        },
        index,
    )
}

fn logical_from_slice(text: &str, start: usize) -> LogicalText {
    let mut logical = LogicalText::new();
    logical.push_slice(text, start);
    logical
}

fn parse_inlines(logical: &LogicalText) -> Vec<Inline> {
    let end = logical.text.trim_end_matches([' ', '\t']).len();
    parse_inline_range(logical, 0, end)
}

fn parse_inline_range(logical: &LogicalText, start: usize, end: usize) -> Vec<Inline> {
    let bytes = logical.text.as_bytes();
    let backtick_runs = DelimiterRunIndex::new(bytes, start, end, b'`');
    let mut tokens = Vec::new();
    let mut text = String::new();
    let mut text_start = start;
    let mut cursor = start;

    let flush_text = |tokens: &mut Vec<InlineToken>, text: &mut String, from: usize, to: usize| {
        if text.is_empty() {
            return;
        }
        tokens.push(InlineToken::Node(Inline {
            kind: InlineKind::Text(std::mem::take(text)),
            range: logical.range(from, to),
            children: Vec::new(),
        }));
    };

    while cursor < end {
        if bytes[cursor] == b'\\' && cursor + 1 < end {
            if bytes[cursor + 1] == b'\n' {
                flush_text(&mut tokens, &mut text, text_start, cursor);
                tokens.push(InlineToken::Node(Inline {
                    kind: InlineKind::HardBreak,
                    range: logical.range(cursor, cursor + 2),
                    children: Vec::new(),
                }));
                cursor += 2;
                text_start = cursor;
                continue;
            }
            if is_ascii_punctuation(bytes[cursor + 1]) {
                if text.is_empty() {
                    text_start = cursor;
                }
                text.push(bytes[cursor + 1] as char);
                cursor += 2;
                continue;
            }
        }
        if bytes[cursor] == b'`' {
            let run = count_run(bytes, cursor, b'`', end);
            if let Some(close) = backtick_runs.next_start(run, cursor + run) {
                flush_text(&mut tokens, &mut text, text_start, cursor);
                let raw = &logical.text[cursor + run..close];
                let mut cooked = raw.replace('\n', " ");
                if cooked.starts_with(' ')
                    && cooked.ends_with(' ')
                    && cooked.bytes().any(|byte| byte != b' ')
                {
                    cooked.remove(0);
                    cooked.pop();
                }
                tokens.push(InlineToken::Node(Inline {
                    kind: InlineKind::Code(cooked),
                    range: logical.range(cursor, close + run),
                    children: Vec::new(),
                }));
                cursor = close + run;
                text_start = cursor;
                continue;
            }
            if text.is_empty() {
                text_start = cursor;
            }
            text.push_str(&logical.text[cursor..cursor + run]);
            cursor += run;
            continue;
        }
        if matches!(bytes[cursor], b'*' | b'_') {
            flush_text(&mut tokens, &mut text, text_start, cursor);
            let marker = bytes[cursor];
            let run = count_run(bytes, cursor, marker, end);
            let before = logical.text[..cursor].chars().next_back();
            let after = logical.text[cursor + run..end].chars().next();
            let (can_open, can_close) = delimiter_flanking(marker, before, after);
            tokens.push(InlineToken::Delimiter {
                marker,
                start: cursor,
                end: cursor + run,
                can_open,
                can_close,
            });
            cursor += run;
            text_start = cursor;
            continue;
        }
        if bytes[cursor] == b'\n' {
            let spaces = text.bytes().rev().take_while(|byte| *byte == b' ').count();
            if spaces >= 2 {
                text.truncate(text.len() - spaces);
                flush_text(&mut tokens, &mut text, text_start, cursor - spaces);
                tokens.push(InlineToken::Node(Inline {
                    kind: InlineKind::HardBreak,
                    range: logical.range(cursor - spaces, cursor + 1),
                    children: Vec::new(),
                }));
            } else {
                if spaces == 1 {
                    text.pop();
                }
                flush_text(&mut tokens, &mut text, text_start, cursor - spaces);
                tokens.push(InlineToken::Node(Inline {
                    kind: InlineKind::SoftBreak,
                    range: logical.range(cursor, cursor + 1),
                    children: Vec::new(),
                }));
            }
            cursor += 1;
            text_start = cursor;
            continue;
        }
        if text.is_empty() {
            text_start = cursor;
        }
        let character = logical.text[cursor..end].chars().next().unwrap();
        text.push(character);
        cursor += character.len_utf8();
    }
    flush_text(&mut tokens, &mut text, text_start, end);
    process_emphasis(logical, tokens)
}

#[derive(Debug)]
enum InlineToken {
    Node(Inline),
    Delimiter {
        marker: u8,
        start: usize,
        end: usize,
        can_open: bool,
        can_close: bool,
    },
}

#[derive(Clone, Debug)]
struct DelimiterEntry {
    marker: u8,
    start: usize,
    end: usize,
    can_open: bool,
    can_close: bool,
    previous: Option<usize>,
    previous_same: Option<usize>,
    next: Option<usize>,
    next_same: Option<usize>,
    active: bool,
}

#[derive(Clone, Copy, Debug)]
struct EmphasisMatch {
    id: usize,
    kind: EmphasisKind,
    open_start: usize,
    open_end: usize,
    close_start: usize,
    close_end: usize,
}

#[derive(Clone, Copy, Debug)]
enum EmphasisKind {
    Emphasis,
    Strong,
}

#[derive(Clone, Copy, Debug)]
enum EmphasisEventKind {
    Open,
    Close,
}

#[derive(Clone, Copy, Debug)]
struct EmphasisEvent {
    offset: usize,
    match_id: usize,
    kind: EmphasisEventKind,
}

fn process_emphasis(logical: &LogicalText, tokens: Vec<InlineToken>) -> Vec<Inline> {
    let (matches, consumed) = match_delimiters(&tokens);
    if matches.is_empty() {
        return unresolved_tokens(logical, tokens);
    }

    let mut events = Vec::with_capacity(matches.len() * 2);
    for matched in &matches {
        events.push(EmphasisEvent {
            offset: matched.open_start,
            match_id: matched.id,
            kind: EmphasisEventKind::Open,
        });
        events.push(EmphasisEvent {
            offset: matched.close_end,
            match_id: matched.id,
            kind: EmphasisEventKind::Close,
        });
    }
    // At a shared byte boundary an existing wrapper closes before a new one
    // opens. Within a delimiter run, distinct consumed bytes establish the
    // outer-to-inner order without special casing.
    events.sort_by_key(|event| {
        (
            event.offset,
            matches!(event.kind, EmphasisEventKind::Open) as u8,
        )
    });

    #[derive(Debug)]
    struct Builder {
        match_id: Option<usize>,
        children: Vec<Inline>,
    }

    fn apply_events(
        logical: &LogicalText,
        matches: &[EmphasisMatch],
        events: &[EmphasisEvent],
        event_index: &mut usize,
        offset: usize,
        stack: &mut Vec<Builder>,
    ) {
        while events
            .get(*event_index)
            .is_some_and(|event| event.offset == offset)
        {
            let event = events[*event_index];
            *event_index += 1;
            let matched = matches[event.match_id];
            match event.kind {
                EmphasisEventKind::Open => stack.push(Builder {
                    match_id: Some(event.match_id),
                    children: Vec::new(),
                }),
                EmphasisEventKind::Close => {
                    let builder = stack.pop().expect("emphasis builder");
                    assert_eq!(builder.match_id, Some(event.match_id));
                    push_inline(
                        &mut stack.last_mut().unwrap().children,
                        Inline {
                            kind: match matched.kind {
                                EmphasisKind::Emphasis => InlineKind::Emphasis,
                                EmphasisKind::Strong => InlineKind::Strong,
                            },
                            range: logical.range(matched.open_start, matched.close_end),
                            children: builder.children,
                        },
                    );
                }
            }
        }
    }

    let mut stack = vec![Builder {
        match_id: None,
        children: Vec::new(),
    }];
    let mut event_index = 0;
    let mut consumed_index = 0;
    for token in tokens {
        match token {
            InlineToken::Node(inline) => {
                push_inline(&mut stack.last_mut().unwrap().children, inline)
            }
            InlineToken::Delimiter {
                marker, start, end, ..
            } => {
                let mut cursor = start;
                while cursor < end {
                    apply_events(
                        logical,
                        &matches,
                        &events,
                        &mut event_index,
                        cursor,
                        &mut stack,
                    );
                    while consumed
                        .get(consumed_index)
                        .is_some_and(|range| range.end <= cursor)
                    {
                        consumed_index += 1;
                    }
                    let is_consumed = consumed
                        .get(consumed_index)
                        .is_some_and(|range| range.start <= cursor && cursor < range.end);
                    if !is_consumed {
                        push_inline(
                            &mut stack.last_mut().unwrap().children,
                            Inline {
                                kind: InlineKind::Text((marker as char).to_string()),
                                range: logical.range(cursor, cursor + 1),
                                children: Vec::new(),
                            },
                        );
                    }
                    cursor += 1;
                }
                apply_events(
                    logical,
                    &matches,
                    &events,
                    &mut event_index,
                    end,
                    &mut stack,
                );
            }
        }
    }
    assert_eq!(event_index, events.len());
    assert_eq!(stack.len(), 1);
    stack.pop().unwrap().children
}

fn match_delimiters(tokens: &[InlineToken]) -> (Vec<EmphasisMatch>, Vec<SourceRange>) {
    let mut entries = Vec::<DelimiterEntry>::new();
    let mut previous = None;
    let mut last_same = [None, None];
    for token in tokens {
        let InlineToken::Delimiter {
            marker,
            start,
            end,
            can_open,
            can_close,
        } = token
        else {
            continue;
        };
        let index = entries.len();
        let marker_index = (*marker == b'_') as usize;
        entries.push(DelimiterEntry {
            marker: *marker,
            start: *start,
            end: *end,
            can_open: *can_open,
            can_close: *can_close,
            previous,
            previous_same: last_same[marker_index],
            next: None,
            next_same: None,
            active: true,
        });
        if let Some(previous) = previous {
            entries[previous].next = Some(index);
        }
        if let Some(previous_same) = last_same[marker_index] {
            entries[previous_same].next_same = Some(index);
        }
        last_same[marker_index] = Some(index);
        previous = Some(index);
    }
    let mut head = (!entries.is_empty()).then_some(0);
    let mut matches = Vec::new();
    // CommonMark's opener-bottom optimization. A failed search establishes a
    // lower bound for this marker / closer-can-open / run-length-mod-3 class,
    // preventing repeated scans of delimiter prefixes already proved unable
    // to match.
    let mut openers_bottom = [0usize; 12];
    let mut closer = head;
    while let Some(closer_index) = closer {
        if !entries[closer_index].active || !entries[closer_index].can_close {
            closer = entries[closer_index].next;
            continue;
        }
        loop {
            let closer_len = entries[closer_index].end - entries[closer_index].start;
            let marker_index = (entries[closer_index].marker == b'_') as usize;
            let opener_bottom_index =
                marker_index * 6 + (entries[closer_index].can_open as usize) * 3 + closer_len % 3;
            // Search the active chain for this marker. Removal maintains this
            // linkage as well as the all-delimiter chain, so nested input is
            // traversed once instead of repeatedly walking stale entries.
            let mut opener = entries[closer_index].previous_same;
            let mut mod_three_rule_invoked = false;
            while let Some(opener_index) = opener {
                if opener_index < openers_bottom[opener_bottom_index] {
                    opener = None;
                    break;
                }
                let candidate = &entries[opener_index];
                if candidate.active && candidate.can_open {
                    if delimiter_pair_allowed(candidate, &entries[closer_index]) {
                        break;
                    }
                    mod_three_rule_invoked = true;
                }
                opener = candidate.previous_same;
            }
            let Some(opener_index) = opener else {
                if !mod_three_rule_invoked {
                    openers_bottom[opener_bottom_index] = closer_index;
                }
                if !entries[closer_index].can_open {
                    let next = entries[closer_index].next;
                    remove_delimiter(&mut entries, &mut head, closer_index);
                    closer = next;
                }
                break;
            };

            let use_count = if entries[opener_index].end - entries[opener_index].start >= 2
                && entries[closer_index].end - entries[closer_index].start >= 2
            {
                2
            } else {
                1
            };
            let open_end = entries[opener_index].end;
            let open_start = open_end - use_count;
            let close_start = entries[closer_index].start;
            let close_end = close_start + use_count;
            matches.push(EmphasisMatch {
                id: matches.len(),
                kind: if use_count == 2 {
                    EmphasisKind::Strong
                } else {
                    EmphasisKind::Emphasis
                },
                open_start,
                open_end,
                close_start,
                close_end,
            });
            entries[opener_index].end = open_start;
            entries[closer_index].start = close_end;

            let mut between = entries[opener_index].next;
            while let Some(index) = between {
                if index == closer_index {
                    break;
                }
                between = entries[index].next;
                remove_delimiter(&mut entries, &mut head, index);
            }
            if entries[opener_index].start == entries[opener_index].end {
                remove_delimiter(&mut entries, &mut head, opener_index);
            }
            if entries[closer_index].start == entries[closer_index].end {
                let next = entries[closer_index].next;
                remove_delimiter(&mut entries, &mut head, closer_index);
                closer = next;
                break;
            }
        }
        if entries[closer_index].active {
            closer = entries[closer_index].next;
        }
    }

    let mut consumed = matches
        .iter()
        .flat_map(|matched| {
            [
                SourceRange::new(matched.open_start, matched.open_end),
                SourceRange::new(matched.close_start, matched.close_end),
            ]
        })
        .collect::<Vec<_>>();
    consumed.sort_by_key(|range| range.start);
    (matches, consumed)
}

fn delimiter_pair_allowed(opener: &DelimiterEntry, closer: &DelimiterEntry) -> bool {
    let opener_len = opener.end - opener.start;
    let closer_len = closer.end - closer.start;
    !((opener.can_close || closer.can_open)
        && (opener_len + closer_len).is_multiple_of(3)
        && (!opener_len.is_multiple_of(3) || !closer_len.is_multiple_of(3)))
}

fn remove_delimiter(entries: &mut [DelimiterEntry], head: &mut Option<usize>, index: usize) {
    if !entries[index].active {
        return;
    }
    let previous = entries[index].previous;
    let next = entries[index].next;
    let previous_same = entries[index].previous_same;
    let next_same = entries[index].next_same;
    if let Some(previous) = previous {
        entries[previous].next = next;
    } else {
        *head = next;
    }
    if let Some(next) = next {
        entries[next].previous = previous;
    }
    if let Some(previous_same) = previous_same {
        entries[previous_same].next_same = next_same;
    }
    if let Some(next_same) = next_same {
        entries[next_same].previous_same = previous_same;
    }
    entries[index].active = false;
}

fn unresolved_tokens(logical: &LogicalText, tokens: Vec<InlineToken>) -> Vec<Inline> {
    let mut output = Vec::new();
    for token in tokens {
        let inline = match token {
            InlineToken::Node(inline) => inline,
            InlineToken::Delimiter {
                marker, start, end, ..
            } => Inline {
                kind: InlineKind::Text(std::iter::repeat_n(marker as char, end - start).collect()),
                range: logical.range(start, end),
                children: Vec::new(),
            },
        };
        push_inline(&mut output, inline);
    }
    output
}

fn push_inline(output: &mut Vec<Inline>, inline: Inline) {
    if let Some(last) = output.last_mut() {
        if let (InlineKind::Text(last_text), InlineKind::Text(next_text)) =
            (&mut last.kind, &inline.kind)
        {
            if last.range.end == inline.range.start {
                last_text.push_str(next_text);
                last.range.end = inline.range.end;
                return;
            }
        }
    }
    output.push(inline);
}

fn delimiter_flanking(marker: u8, before: Option<char>, after: Option<char>) -> (bool, bool) {
    let before_whitespace = before.is_none_or(char::is_whitespace);
    let after_whitespace = after.is_none_or(char::is_whitespace);
    let before_punctuation = before.is_some_and(is_punctuation);
    let after_punctuation = after.is_some_and(is_punctuation);
    let left_flanking =
        !after_whitespace && (!after_punctuation || before_whitespace || before_punctuation);
    let right_flanking =
        !before_whitespace && (!before_punctuation || after_whitespace || after_punctuation);
    if marker == b'_' {
        (
            left_flanking && (!right_flanking || before_punctuation),
            right_flanking && (!left_flanking || after_punctuation),
        )
    } else {
        (left_flanking, right_flanking)
    }
}

fn is_punctuation(character: char) -> bool {
    // The production parser needs generated Unicode general-category tables.
    // ASCII is sufficient to exercise the delimiter-stack architecture, and
    // the scorecard makes the remaining Unicode conformance debt visible.
    character.is_ascii_punctuation()
}

fn count_run(bytes: &[u8], start: usize, marker: u8, end: usize) -> usize {
    bytes[start..end]
        .iter()
        .take_while(|byte| **byte == marker)
        .count()
}

/// One-pass delimiter-run index for code-span lookup.
///
/// cmark's inline parser caches the last observed position for each backtick
/// run length so unmatched openers do not repeatedly scan the remaining leaf.
/// This trial uses an explicit position index instead; a production resumable
/// inline machine would build equivalent state incrementally under its budget.
struct DelimiterRunIndex {
    starts_by_length: HashMap<usize, Vec<usize>>,
}

impl DelimiterRunIndex {
    fn new(bytes: &[u8], start: usize, end: usize, marker: u8) -> Self {
        let mut starts_by_length = HashMap::<usize, Vec<usize>>::new();
        let mut cursor = start;
        while cursor < end {
            if bytes[cursor] != marker {
                cursor += 1;
                continue;
            }
            let length = count_run(bytes, cursor, marker, end);
            starts_by_length.entry(length).or_default().push(cursor);
            cursor += length;
        }
        Self { starts_by_length }
    }

    fn next_start(&self, length: usize, from: usize) -> Option<usize> {
        let starts = self.starts_by_length.get(&length)?;
        starts
            .get(starts.partition_point(|start| *start < from))
            .copied()
    }
}

fn is_ascii_punctuation(byte: u8) -> bool {
    byte.is_ascii_punctuation()
}

fn removable_indent(line: &str, maximum: usize) -> usize {
    leading_spaces(line).min(maximum)
}

fn take_id(next_id: &mut u64) -> u64 {
    let id = *next_id;
    *next_id += 1;
    id
}

fn build_coverage(source_len: usize, blocks: &[Block]) -> Vec<CoverageLeaf> {
    let mut output = Vec::new();
    let mut cursor = 0;
    for block in blocks {
        if cursor < block.range.start {
            output.push(CoverageLeaf {
                range: SourceRange::new(cursor, block.range.start),
                owner: None,
            });
        }
        if cursor < block.range.end {
            output.push(CoverageLeaf {
                range: SourceRange::new(cursor.max(block.range.start), block.range.end),
                owner: Some(block.id),
            });
            cursor = block.range.end;
        }
    }
    if cursor < source_len {
        output.push(CoverageLeaf {
            range: SourceRange::new(cursor, source_len),
            owner: None,
        });
    }
    output
}

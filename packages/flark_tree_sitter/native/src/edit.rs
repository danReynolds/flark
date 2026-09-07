//! A snippet edit proposal. Language differences live in validated queries.
use serde::{Deserialize, Serialize};
use tree_sitter::{QueryCursor, StreamingIterator};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub source: String,
    pub base: usize,
    pub extent: usize,
    pub action: Action,
    #[serde(default)]
    pub text: String,
    pub unit: String,
    pub newline: String,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Insert,
    Newline,
    Indent,
    Outdent,
}

#[derive(Debug, Serialize)]
pub struct Edit {
    pub version: u32,
    pub language: u32,
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub base: usize,
    pub extent: usize,
}

struct Capture {
    name: String,
    start: usize,
    end: usize,
}
struct Syntax {
    captures: Vec<Capture>,
    literal: Vec<bool>,
}

fn byte_at(s: &str, units: usize) -> Result<usize, String> {
    let mut n = 0;
    for (i, c) in s.char_indices() {
        if n == units {
            return Ok(i);
        }
        n += c.len_utf16();
    }
    if n == units {
        Ok(s.len())
    } else {
        Err("invalid UTF-16 position".into())
    }
}
fn units(s: &str) -> usize {
    s.encode_utf16().count()
}
fn line_start(s: &str, p: usize) -> usize {
    s[..p].rfind('\n').map_or(0, |i| i + 1)
}
fn line_end(s: &str, p: usize) -> usize {
    s[p..].find(['\r', '\n']).map_or(s.len(), |i| p + i)
}
fn leading(s: &str) -> &str {
    &s[..s.find(|c| c != ' ' && c != '\t').unwrap_or(s.len())]
}
fn indent(s: &str, p: usize) -> &str {
    leading(&s[line_start(s, p)..line_end(s, p)])
}

impl Syntax {
    fn parse(source: &str, language: u32) -> Result<Self, String> {
        if language == 0 {
            return Ok(Self {
                captures: vec![],
                literal: vec![],
            });
        }
        let grammar = crate::languages::get(language)?;
        let prepared = crate::syntax::parse(source, &grammar)?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(
            &grammar.edit,
            prepared.tree.root_node(),
            prepared.source.as_bytes(),
        );
        let names = grammar.edit.capture_names();
        let mut captures = vec![];
        while let Some(m) = matches.next() {
            for c in m.captures() {
                if c.node.is_missing() {
                    continue;
                }
                let name = names[c.index as usize];
                if c.node.end_byte() <= prepared.offset
                    || c.node.start_byte() >= prepared.offset + source.len()
                {
                    continue;
                }
                let mut start = c.node.start_byte().max(prepared.offset) - prepared.offset;
                let mut end =
                    c.node.end_byte().min(prepared.offset + source.len()) - prepared.offset;
                if name == "opaque.tail" {
                    end = source.len() + 1;
                }
                if name == "opaque.body" {
                    start = source[start..end]
                        .find('\n')
                        .map_or(end + 1, |i| start + i + 1);
                    end += 1;
                }
                captures.push(Capture {
                    name: name.into(),
                    start,
                    end,
                });
            }
        }
        captures.sort_by_key(|c| (c.start, c.end));
        captures.dedup_by(|a, b| a.name == b.name && a.start == b.start && a.end == b.end);
        // The bounded snippet permits a compact byte-position mask. Paint
        // outer regions first so interpolation and nested strings override
        // their containers. Avoid rescanning every region for every delimiter.
        let mut regions: Vec<_> = captures
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                c.name.starts_with("opaque") || c.name == "code" || c.name == "comment"
            })
            .collect();
        regions.sort_by_key(|(i, c)| (std::cmp::Reverse(c.end - c.start), std::cmp::Reverse(*i)));
        let mut literal = vec![false; source.len() + 1];
        for (_, c) in regions {
            literal[c.start.min(source.len() + 1)..c.end.min(source.len() + 1)]
                .fill(c.name != "code");
        }
        Ok(Self { captures, literal })
    }
    fn literal(&self, p: usize) -> bool {
        self.literal.get(p).copied().unwrap_or(false)
    }
    fn structural(&self, c: &Capture) -> bool {
        !self.literal(c.start)
    }
    fn stack(&self, p: usize) -> Vec<&Capture> {
        let mut stack: Vec<&Capture> = vec![];
        for c in &self.captures {
            if c.end > p || !self.structural(c) {
                continue;
            }
            if c.name.starts_with("open.") {
                stack.push(c);
            } else if let Some(kind) = c.name.strip_prefix("close.") {
                if stack
                    .last()
                    .is_some_and(|o| o.name.strip_prefix("open.") == Some(kind))
                {
                    stack.pop();
                } else {
                    stack.clear();
                }
            }
        }
        stack
    }
    fn last_code(&self, s: &str, p: usize) -> usize {
        let mut end = p;
        loop {
            end = s[..end].trim_end_matches([' ', '\t']).len();
            if let Some(c) = self
                .captures
                .iter()
                .find(|c| c.name == "comment" && c.end == end)
            {
                end = c.start;
            } else {
                return end;
            }
        }
    }
    fn branch_target<'a>(&self, s: &'a str, at: usize) -> Option<&'a str> {
        let start = line_start(s, at);
        let first = start + indent(s, at).len();
        let branch = self
            .captures
            .iter()
            .find(|c| c.start == first && c.name.starts_with("branch.") && self.structural(c))?;
        let families: Vec<_> = branch.name[7..].split('.').collect();
        let mut stack: Vec<(usize, &str, &str)> = vec![];
        let mut line = 0;
        while line < start {
            let end = line_end(s, line);
            let ws = leading(&s[line..end]);
            let first = line + ws.len();
            if first < end && !self.literal(first) {
                let is_branch = self
                    .captures
                    .iter()
                    .any(|c| c.start == first && c.name.starts_with("branch."));
                while stack
                    .last()
                    .is_some_and(|(col, _, _)| *col > ws.len() || (*col == ws.len() && !is_branch))
                {
                    stack.pop();
                }
                if let Some(c) = self
                    .captures
                    .iter()
                    .find(|c| c.start == first && c.name.starts_with("anchor."))
                {
                    stack.push((ws.len(), &c.name[7..], ws));
                }
            }
            line = s[end..].find('\n').map_or(start, |i| end + i + 1);
        }
        stack
            .iter()
            .rev()
            .find(|(_, family, _)| families.contains(family))
            .map(|(_, _, ws)| *ws)
    }
}

pub fn propose(r: Request, language: u32) -> Result<Edit, String> {
    if language > crate::languages::COUNT || units(&r.source) > crate::MAX_UTF16 {
        return Err("source or language limit".into());
    }
    if !(r.unit == "\t"
        || (!r.unit.is_empty() && r.unit.len() <= 8 && r.unit.bytes().all(|b| b == b' ')))
        || (r.newline != "\n" && r.newline != "\r\n")
    {
        return Err("invalid indentation options".into());
    }
    if r.action != Action::Insert && !r.text.is_empty() {
        return Err("text only valid for insert".into());
    }
    let base = byte_at(&r.source, r.base)?;
    let extent = byte_at(&r.source, r.extent)?;
    let (start, end) = (base.min(extent), base.max(extent));
    if [start, end]
        .iter()
        .any(|p| *p > 0 && *p < r.source.len() && &r.source.as_bytes()[p - 1..=(*p)] == b"\r\n")
    {
        return Err("position splits CRLF".into());
    }
    if r.action == Action::Indent || r.action == Action::Outdent {
        return shift(&r, language, base, extent);
    }
    let mut candidate = r.source.clone();
    candidate.replace_range(
        start..end,
        if r.action == Action::Insert {
            &r.text
        } else {
            ""
        },
    );
    if units(&candidate) > crate::MAX_UTF16 {
        return Err("candidate limit".into());
    }
    let at = start
        + if r.action == Action::Insert {
            r.text.len()
        } else {
            0
        };
    if r.action == Action::Insert
        && (r.text.chars().count() != 1
            || !crate::languages::reindent_triggers(language).contains(&r.text)
            || (!crate::languages::branch_triggers(language).contains(&r.text)
                && !candidate[line_start(&candidate, start)..start]
                    .bytes()
                    .all(|c| c == b' ' || c == b'\t')))
    {
        let caret = units(&candidate[..at]);
        return Ok(Edit {
            version: crate::VERSION,
            language,
            start: units(&r.source[..start]),
            end: units(&r.source[..end]),
            text: r.text,
            base: caret,
            extent: caret,
        });
    }
    let syntax = Syntax::parse(&candidate, language)?;
    let line = line_start(&candidate, at);
    let current = leading(&candidate[line..at]);
    if r.action == Action::Insert {
        let mut from = start;
        let mut to = end;
        let mut inserted = r.text.clone();
        let mut caret = at;
        if r.text.chars().count() == 1 && !syntax.literal(at.saturating_sub(1)) {
            let target = if let Some(close) = syntax.captures.iter().find(|c| {
                c.end == at && c.start == line + current.len() && c.name.starts_with("close.")
            }) {
                syntax
                    .stack(close.start)
                    .last()
                    .filter(|o| o.name[5..] == close.name[6..])
                    .map(|o| indent(&candidate, o.start))
            } else if let Some(middle) = syntax.captures.iter().find(|c| {
                c.end == at && c.start == line + current.len() && c.name.starts_with("middle.")
            }) {
                syntax
                    .stack(middle.start)
                    .last()
                    .filter(|o| o.name.strip_prefix("open.") == middle.name.strip_prefix("middle."))
                    .map(|o| indent(&candidate, o.start))
            } else if crate::languages::branch_triggers(language).contains(&r.text)
                && syntax.stack(at).is_empty()
            {
                syntax.branch_target(&candidate, at)
            } else {
                None
            };
            if let Some(target) =
                target.filter(|t| t.len() < current.len() && current.starts_with(*t))
            {
                let length = current.len();
                let target = target.to_string();
                candidate.replace_range(line..line + length, &target);
                caret -= length - target.len();
                // One replacement includes the typed character and outdent.
                from = line;
                to = end;
                inserted = candidate[line..caret].to_string();
            }
        }
        return Ok(Edit {
            version: crate::VERSION,
            language,
            start: units(&r.source[..from]),
            end: units(&r.source[..to]),
            text: inserted,
            base: units(&candidate[..caret]),
            extent: units(&candidate[..caret]),
        });
    }
    let mut target = current.to_string();
    let mut consume = end;
    let mut tail = String::new();
    // A caret immediately before a literal belongs to the preceding code.
    // Looking only at the next byte would suppress Enter after `{` in JSON.
    if !syntax.literal(at) || at == 0 || !syntax.literal(at - 1) {
        let last = syntax.last_code(&candidate, at);
        let stack = syntax.stack(at);
        let opener = stack.last().filter(|c| c.start >= line).copied();
        let header = syntax.captures.iter().find(|c| {
            c.end == last
                && c.start >= line
                && c.name.starts_with("indent.")
                && c.name != "indent.prefix"
                && syntax.structural(c)
        });
        let middle = syntax.captures.iter().any(|c| {
            c.name.starts_with("middle.")
                && c.start >= line
                && c.end <= last
                && syntax.structural(c)
                && stack
                    .last()
                    .is_some_and(|o| o.name.strip_prefix("open.") == c.name.strip_prefix("middle."))
        });
        if opener.is_some() || header.is_some() || middle {
            if let Some(prefix) = syntax
                .captures
                .iter()
                .find(|c| c.name == "indent.prefix" && c.start >= line && c.end <= at)
            {
                let width = prefix.end - prefix.start + leading(&candidate[prefix.end..at]).len();
                target.push_str(&" ".repeat(width));
            }
            let scalar_width = header
                .filter(|h| h.name == "indent.scalar")
                .and_then(|h| {
                    candidate[h.start..h.end]
                        .chars()
                        .find_map(|c| c.to_digit(10))
                })
                .filter(|n| *n > 0);
            if let Some(width) = scalar_width {
                target.push_str(&" ".repeat(width as usize));
            } else {
                target.push_str(&r.unit);
            }
        }
        if let Some(open) = opener {
            let right_ws = leading(&candidate[at..line_end(&candidate, at)]);
            if syntax.captures.iter().any(|c| {
                c.start == at + right_ws.len()
                    && c.name.strip_prefix("close.") == open.name.strip_prefix("open.")
            }) {
                consume += right_ws.len();
                tail = format!("{}{}", r.newline, current);
            }
        }
    }
    let first = format!("{}{}", r.newline, target);
    let caret = units(&r.source[..start]) + units(&first);
    Ok(Edit {
        version: crate::VERSION,
        language,
        start: units(&r.source[..start]),
        end: units(&r.source[..consume]),
        text: first + &tail,
        base: caret,
        extent: caret,
    })
}

fn shift(r: &Request, language: u32, base: usize, extent: usize) -> Result<Edit, String> {
    let start = base.min(extent);
    let end = base.max(extent);
    if base == extent && r.action == Action::Indent {
        return Ok(Edit {
            version: crate::VERSION,
            language,
            start: r.base,
            end: r.base,
            text: r.unit.clone(),
            base: r.base + units(&r.unit),
            extent: r.base + units(&r.unit),
        });
    }
    let first = line_start(&r.source, start);
    let last = if end > start && line_start(&r.source, end) == end {
        line_start(&r.source, end - 1)
    } else {
        line_start(&r.source, end)
    };
    let stop = line_end(&r.source, last);
    let mut line = first;
    let mut changes = vec![];
    while line <= last {
        let ws = indent(&r.source, line);
        let removed = if r.action == Action::Indent {
            0
        } else if ws.starts_with('\t') {
            1
        } else {
            ws.len().min(if r.unit == "\t" { 4 } else { r.unit.len() })
        };
        let inserted = if r.action == Action::Indent {
            r.unit.as_str()
        } else {
            ""
        };
        changes.push((line, removed, inserted));
        line = r.source[line..]
            .find('\n')
            .map_or(last + 1, |i| line + i + 1);
    }
    let mut text = r.source[first..stop].to_string();
    for (at, removed, inserted) in changes.iter().rev() {
        text.replace_range(at - first..at - first + removed, inserted);
    }
    let mapped = |pos: usize| -> usize {
        let mut delta: isize = 0;
        for (at, removed, inserted) in &changes {
            if pos < *at {
                break;
            }
            if pos <= at + removed {
                return (*at as isize + delta) as usize + inserted.len();
            }
            delta += inserted.len() as isize - *removed as isize;
        }
        (pos as isize + delta) as usize
    };
    let mut result = r.source.clone();
    result.replace_range(first..stop, &text);
    Ok(Edit {
        version: crate::VERSION,
        language,
        start: units(&r.source[..first]),
        end: units(&r.source[..stop]),
        text,
        base: units(&result[..mapped(base)]),
        extent: units(&result[..mapped(extent)]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_requests_are_rejected_before_syntax_work() {
        for (source, position) in [("😀", 1), ("\r\n", 1), ("x", 2)] {
            let r = Request {
                source: source.into(),
                base: position,
                extent: position,
                action: Action::Newline,
                text: String::new(),
                unit: "  ".into(),
                newline: "\n".into(),
            };
            assert!(propose(r, 1).is_err());
        }
        assert!(serde_json::from_str::<Request>(r#"{"source":"","base":0,"extent":0,"action":"newline","unit":"  ","newline":"\n","unknown":true}"#).is_err());
    }
}

//! Snippet-local language services. Markdown and document edits belong to Flark.

mod abi;
pub mod detect;
pub mod edit;
mod languages;
mod syntax;

use serde::Serialize;
use std::cell::RefCell;
use tree_sitter_highlight::{HighlightEvent, Highlighter};

thread_local! {
    // Upstream recommends one reusable highlighter per executing thread. This
    // retains parser/cursor scratch storage, never a document or analysis.
    static HIGHLIGHTER: RefCell<Highlighter> = RefCell::new(Highlighter::new());
}

pub const VERSION: u32 = 4;
pub const MAX_UTF16: usize = 8192;

/// Both coordinate systems describe the exact original source, including CRLF.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Span {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start: usize,
    pub end: usize,
    pub scopes: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Analysis {
    pub version: u32,
    pub language: u32,
    pub status: &'static str,
    pub spans: Vec<Span>,
}

impl Analysis {
    /// Wire ranges are contiguous, so starts are the previous ends. Intern the
    /// scope stacks once per response instead of repeating strings per token.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        use std::collections::HashMap;
        #[derive(Serialize)]
        struct Wire<'a> {
            version: u32,
            language: u32,
            status: &'a str,
            scope_sets: Vec<&'a [String]>,
            spans: Vec<[usize; 3]>,
        }
        let mut ids = HashMap::new();
        let mut scope_sets = Vec::new();
        let mut spans = Vec::with_capacity(self.spans.len());
        for span in &self.spans {
            let id = *ids.entry(span.scopes.as_slice()).or_insert_with(|| {
                scope_sets.push(span.scopes.as_slice());
                scope_sets.len() - 1
            });
            spans.push([span.end, span.end_byte, id]);
        }
        serde_json::to_vec(&Wire {
            version: self.version,
            language: self.language,
            status: self.status,
            scope_sets,
            spans,
        })
        .map_err(|e| e.to_string())
    }
}

/// Complete-snippet analysis is intentional for the first integration gate.
/// It is bounded by source size, not yet qualified as a keystroke-time budget.
pub fn analyze(source: &str, language: u32) -> Result<Analysis, String> {
    HIGHLIGHTER.with_borrow_mut(|highlighter| analyze_with(source, language, highlighter))
}

fn analyze_with(
    source: &str,
    language: u32,
    highlighter: &mut Highlighter,
) -> Result<Analysis, String> {
    let units = source.encode_utf16().count();
    if language > crate::languages::COUNT {
        return Err("unsupported language".into());
    }
    if units > MAX_UTF16 || language == 0 {
        return Ok(Analysis {
            version: VERSION,
            language,
            status: if units > MAX_UTF16 { "limit" } else { "plain" },
            spans: if source.is_empty() {
                vec![]
            } else {
                vec![Span {
                    start_byte: 0,
                    end_byte: source.len(),
                    start: 0,
                    end: units,
                    scopes: vec![],
                }]
            },
        });
    }
    let grammar = languages::get(language)?;
    let prepared = grammar
        .statement_context
        .map(|_| syntax::parse(source, &grammar))
        .transpose()?;
    let (input, offset) = prepared
        .as_ref()
        .map_or((source, 0), |p| (p.source.as_str(), p.offset));
    let highlights = grammar.highlight()?;
    let events = highlighter
        .highlight(&highlights.config, input.as_bytes(), None, None, |_| None)
        .map_err(|e| e.to_string())?;
    let mut scopes = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut consumed = 0;
    let mut utf16 = 0;
    for event in events {
        match event.map_err(|e| e.to_string())? {
            HighlightEvent::HighlightStart(id) => scopes.push(highlights.scopes[id.0].clone()),
            HighlightEvent::HighlightEnd => {
                scopes.pop().ok_or("unbalanced highlight scopes")?;
            }
            HighlightEvent::Source { start, end } => {
                let start = start.max(offset).min(offset + source.len()) - offset;
                let end = end.max(offset).min(offset + source.len()) - offset;
                if start == end {
                    continue;
                }
                if start != consumed
                    || end < start
                    || !source.is_char_boundary(start)
                    || !source.is_char_boundary(end)
                {
                    return Err("noncontiguous or invalid source range".into());
                }
                let length = source[start..end].encode_utf16().count();
                if end > start {
                    if let Some(previous) = spans.last_mut().filter(|span| span.scopes == scopes) {
                        previous.end_byte = end;
                        previous.end = utf16 + length;
                    } else {
                        spans.push(Span {
                            start_byte: start,
                            end_byte: end,
                            start: utf16,
                            end: utf16 + length,
                            scopes: scopes.clone(),
                        });
                    }
                }
                consumed = end;
                utf16 += length;
            }
        }
    }
    if consumed != source.len() || !scopes.is_empty() {
        return Err("incomplete highlight result".into());
    }
    Ok(Analysis {
        version: VERSION,
        language,
        status: "highlighted",
        spans,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_unicode_crlf_and_incomplete_source_in_every_grammar() {
        for language in 1..=crate::languages::COUNT {
            for source in ["", "\r\n", "\n\n", "😀 café 中文\r\n  x", "{\n  '\"", "\0x"] {
                let analysis = analyze(source, language).unwrap();
                let mut end = (0, 0);
                for span in &analysis.spans {
                    assert_eq!((span.start_byte, span.start), end);
                    assert_eq!(
                        source[span.start_byte..span.end_byte]
                            .encode_utf16()
                            .count(),
                        span.end - span.start
                    );
                    end = (span.end_byte, span.end);
                }
                assert_eq!(end, (source.len(), source.encode_utf16().count()));
            }
        }
    }

    #[test]
    fn every_grammar_highlights_real_code_and_distinguishes_strings() {
        for (language, source) in [
            (1, "void main() { print('hello'); }"),
            (2, "const x = 'hello';"),
            (3, "def f():\n    return 'hello'"),
            (4, "name: \"hello\"\n"),
            (5, "def hello\n  print 'hello'\nend"),
        ] {
            let analysis = analyze(source, language).unwrap();
            assert!(
                analysis
                    .spans
                    .iter()
                    .any(|s| s.scopes.iter().any(|s| s.starts_with("string"))),
                "{language}: {analysis:?}"
            );
            assert!(analysis.spans.iter().any(|s| !s.scopes.is_empty()));
        }
    }

    #[test]
    fn unknown_language_errors_and_limit_preserves_source() {
        assert!(analyze("x", 99).is_err());
        let source = "😀".repeat(MAX_UTF16 / 2 + 1);
        let analysis = analyze(&source, 1).unwrap();
        assert_eq!(analysis.status, "limit");
        assert_eq!(analysis.spans[0].end, MAX_UTF16 + 2);
        assert_eq!(analyze("", 0).unwrap().status, "plain");
    }

    #[test]
    fn repeated_analyses_do_not_share_document_state() {
        let expected = analyze("const x = 42;", 2).unwrap();
        for _ in 0..20 {
            analyze("/* unclosed", 2).unwrap();
            analyze("name: |\n  contents", 4).unwrap();
            assert_eq!(analyze("const x = 42;", 2).unwrap(), expected);
        }
    }

    #[test]
    fn reused_highlighter_matches_fresh_after_language_and_context_changes() {
        for source in [
            "for (final x in [1, 2]) {\n  print('😀');\n}",
            "/* unterminated",
            "void f() {",
            "name: |\n  text\n",
            "if ready:\n    print(f'{value}')\nelse:",
            "",
            "const x = `a ${value}`;",
        ] {
            for language in [4, 1, 5, 3, 2, 0, 1] {
                assert_eq!(
                    analyze(source, language).unwrap(),
                    analyze_with(source, language, &mut Highlighter::new()).unwrap()
                );
            }
        }
    }
}

//! Some language grammars require a compilation unit, while fences contain
//! statements. Compare parser error coverage with a grammar-specific context.
use std::cell::RefCell;
use tree_sitter::{InputEdit, Language, Parser, Point, Tree};

thread_local! {
    static PARSER: RefCell<ParseScratch> = RefCell::new(ParseScratch::default());
}

// At most the last admitted snippet in its two possible grammar contexts.
// Every call still supplies the complete source and language. A caller does
// not own a mutable parser session, and switching documents is just an edit.
#[derive(Default)]
struct ParseScratch {
    parser: Parser,
    language: Option<Language>,
    normal: Option<(String, Tree)>,
    wrapped: Option<(String, Tree)>,
}

fn point(source: &str, byte: usize) -> Point {
    let prefix = &source.as_bytes()[..byte];
    Point::new(
        prefix.iter().filter(|&&c| c == b'\n').count(),
        prefix
            .iter()
            .rposition(|&c| c == b'\n')
            .map_or(byte, |i| byte - i - 1),
    )
}

fn edit_between(old: &str, new: &str) -> InputEdit {
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let common_end = old.as_bytes()[start..]
        .iter()
        .rev()
        .zip(new.as_bytes()[start..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (mut old_end, mut new_end) = (old.len() - common_end, new.len() - common_end);
    while !old.is_char_boundary(old_end) || !new.is_char_boundary(new_end) {
        old_end += 1;
        new_end += 1;
    }
    InputEdit {
        start_byte: start,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start_position: point(old, start),
        old_end_position: point(old, old_end),
        new_end_position: point(new, new_end),
    }
}

fn parse_cached(
    parser: &mut Parser,
    source: &str,
    previous: &mut Option<(String, Tree)>,
) -> Result<Tree, String> {
    if let Some((old, tree)) = previous {
        if old == source {
            return Ok(tree.clone());
        }
        tree.edit(&edit_between(old, source));
    }
    let result = parser.parse(source, previous.as_ref().map(|(_, tree)| tree));
    match result {
        Some(tree) => {
            *previous = Some((source.into(), tree.clone()));
            Ok(tree)
        }
        None => {
            *previous = None;
            Err("parse cancelled".into())
        }
    }
}
pub struct Parsed {
    pub source: String,
    pub offset: usize,
    pub tree: Tree,
}
pub fn parse(source: &str, grammar: &crate::languages::Grammar) -> Result<Parsed, String> {
    PARSER.with_borrow_mut(|parser| parse_with(source, grammar, parser))
}
pub fn parse_fresh(source: &str, grammar: &crate::languages::Grammar) -> Result<Parsed, String> {
    parse_with(source, grammar, &mut ParseScratch::default())
}
fn parse_with(
    source: &str,
    grammar: &crate::languages::Grammar,
    scratch: &mut ParseScratch,
) -> Result<Parsed, String> {
    if scratch.language.as_ref() != Some(&grammar.language) {
        scratch
            .parser
            .set_language(&grammar.language)
            .map_err(|e| e.to_string())?;
        scratch.language = Some(grammar.language.clone());
        scratch.normal = None;
        scratch.wrapped = None;
    }
    let tree = parse_cached(&mut scratch.parser, source, &mut scratch.normal)?;
    if tree.root_node().has_error() {
        let errors = error_weight(&tree, 0, source.len());
        // Missing closing punctuation is normal during typing and does not
        // indicate that the grammar chose the wrong enclosing context.
        if let Some((prefix, suffix)) = grammar.statement_context.filter(|_| errors.0 > 0) {
            let wrapped = format!("{prefix}{source}{suffix}");
            let alternative = parse_cached(&mut scratch.parser, &wrapped, &mut scratch.wrapped)?;
            if error_weight(&alternative, prefix.len(), source.len()) < errors {
                return Ok(Parsed {
                    source: wrapped,
                    offset: prefix.len(),
                    tree: alternative,
                });
            }
        }
    }
    Ok(Parsed {
        source: source.into(),
        offset: 0,
        tree,
    })
}
pub fn error_weight(tree: &Tree, offset: usize, length: usize) -> (usize, usize) {
    let mut pending = vec![tree.root_node()];
    let mut weight = 0;
    let mut missing = 0;
    while let Some(node) = pending.pop() {
        if node.is_error() {
            weight += node
                .end_byte()
                .min(offset + length)
                .saturating_sub(node.start_byte().max(offset))
                + 1;
        } else if node.is_missing() {
            missing += 1;
        } else if node.has_error() {
            for i in 0..node.child_count() {
                pending.push(node.child(i).unwrap());
            }
        }
    }
    (weight, missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signature(tree: &Tree) -> String {
        let mut nodes = vec![tree.root_node()];
        let mut result = String::new();
        while let Some(node) = nodes.pop() {
            result.push_str(&format!(
                "{}:{:?}:{}:{};",
                node.kind(),
                node.range(),
                node.is_missing(),
                node.child_count()
            ));
            for i in (0..node.child_count()).rev() {
                nodes.push(node.child(i).unwrap());
            }
        }
        result
    }

    #[test]
    fn incremental_context_matches_fresh_during_typing_and_replacements() {
        let mut scratch = ParseScratch::default();
        for (language, seed) in [
            (1, "for (final x in [1, 2]) {\r\n  print('😀 café');\r\n}"),
            (2, "const x = `😀 ${name}`;\nif (x) {\n  f();\n}"),
            (
                3,
                "if ready:\r\n    print(f'😀 {name}')\r\nelse:\r\n    pass",
            ),
            (4, "settings: |2-\n  😀 café\nitems: [one, two]"),
            (
                5,
                "class Hello\r\n  def hello\r\n    puts '😀 café'\r\n  end\r\nend",
            ),
            (1, "void f() {}\nfor (final x in []) {\nprint(x);\n}"),
        ] {
            let grammar = crate::languages::get(language).unwrap();
            let check = |source: &str, scratch: &mut ParseScratch| {
                let actual = parse_with(source, &grammar, scratch).unwrap();
                let fresh = parse_with(source, &grammar, &mut ParseScratch::default()).unwrap();
                assert_eq!(
                    actual.offset, fresh.offset,
                    "context {language}: {source:?}"
                );
                assert_eq!(
                    signature(&actual.tree),
                    signature(&fresh.tree),
                    "tree {language}: {source:?}"
                );
            };
            // Includes missing delimiters, shared UTF-8 byte prefixes, CRLF,
            // deletion, replacement earlier in a document and document swaps.
            for (end, _) in seed.char_indices().chain([(seed.len(), '\0')]) {
                check(&seed[..end], &mut scratch);
            }
            let mut source = seed.to_string();
            let mut rng = 0x51eed_u64;
            for i in 0..160 {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                let boundaries: Vec<_> = source
                    .char_indices()
                    .map(|(i, _)| i)
                    .chain([source.len()])
                    .collect();
                let a = (rng as usize) % boundaries.len();
                let b = (a + (i % 4)).min(boundaries.len() - 1);
                let inserted = ["", "x", "😀", "😁", "\r\n", "'", "}", ":", "/*"][i % 9];
                source.replace_range(boundaries[a]..boundaries[b], inserted);
                check(&source, &mut scratch);
            }
            check(seed, &mut scratch);
            check("", &mut scratch);
        }
    }
}

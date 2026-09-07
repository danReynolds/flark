//! Diagnostic decomposition; uses the package's exact grammar and query setup.
#[allow(dead_code)]
#[path = "../src/languages.rs"]
mod languages;
#[path = "../src/syntax.rs"]
mod syntax;
use std::time::Instant;
use tree_sitter::{QueryCursor, StreamingIterator};
use tree_sitter_highlight::Highlighter;

fn median(mut f: impl FnMut()) -> u128 {
    f();
    let mut samples = Vec::new();
    for _ in 0..12 {
        let start = Instant::now();
        f();
        samples.push(start.elapsed().as_micros());
    }
    samples.sort();
    samples[samples.len() / 2]
}
fn main() {
    for (id, body, suffix) in [
        (1, "void f() { print(\"😀\"); }\n", "void g() {"),
        (2, "const x = {value: \"😀\"};\n", "if (ready) {"),
        (3, "if ready:\n    print(\"😀\")\n", "if ready:"),
        (4, "name: \"😀\"\nitems: [one, two]\n", "settings: |"),
    ] {
        let source = body.repeat((8190 - suffix.len()) / body.encode_utf16().count()) + suffix;
        let grammar = languages::get(id).unwrap();
        let mut changed = source.clone();
        let parse = median(|| {
            changed.push(' ');
            std::hint::black_box(syntax::parse(&changed, &grammar).unwrap());
        });
        let prepared = syntax::parse(&source, &grammar).unwrap();
        assert_eq!(
            &prepared.source[prepared.offset..prepared.offset + source.len()],
            source
        );
        let query = median(|| {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(
                &grammar.edit,
                prepared.tree.root_node(),
                prepared.source.as_bytes(),
            );
            let mut n = 0;
            while let Some(m) = matches.next() {
                n += m.captures().len();
            }
            std::hint::black_box(n);
        });
        let mut highlighter = Highlighter::new();
        let highlights = grammar.highlight().unwrap();
        let highlight = median(|| {
            for e in highlighter
                .highlight(
                    &highlights.config,
                    prepared.source.as_bytes(),
                    None,
                    None,
                    |_| None,
                )
                .unwrap()
            {
                std::hint::black_box(e.unwrap());
            }
        });
        let analysis = flark_tree_sitter::analyze(&source, id).unwrap();
        let json = median(|| {
            std::hint::black_box(analysis.encode().unwrap());
        });
        println!(
            "{}",
            serde_json::json!({"language":id,"units":source.encode_utf16().count(),"incrementalParseMicros":parse,"editQueryMicros":query,"highlightEventsMicros":highlight,"analysisJsonEncodeMicros":json})
        );
    }
}

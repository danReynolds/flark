use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use flark_owned_parser_trial::{parse, render_html};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Example {
    markdown: String,
    html: String,
    example: usize,
    section: String,
}

fn examples() -> Vec<Example> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../test/fixtures/commonmark/upstream/common_mark_tests.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn complete_foundation_sections_match_commonmark_0312() {
    let sections = BTreeSet::from([
        "ATX headings",
        "Blank lines",
        "Fenced code blocks",
        "Paragraphs",
        "Soft line breaks",
        "Textual content",
    ]);
    let cases: Vec<_> = examples()
        .into_iter()
        .filter(|case| sections.contains(case.section.as_str()))
        .collect();
    assert_eq!(cases.len(), 61);

    let mut failures = Vec::new();
    for case in cases {
        let actual = render_html(&parse(&case.markdown));
        if actual != case.html {
            failures.push(format!(
                "example {} ({})\nmarkdown={:?}\nexpected={:?}\nactual  ={:?}",
                case.example, case.section, case.markdown, case.html, actual
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

#[test]
fn source_coverage_is_total_and_nonoverlapping() {
    for case in examples() {
        let document = parse(&case.markdown);
        let mut cursor = 0;
        for leaf in &document.coverage {
            assert_eq!(leaf.range.start, cursor, "example {}", case.example);
            assert!(
                leaf.range.end > leaf.range.start,
                "example {}",
                case.example
            );
            cursor = leaf.range.end;
        }
        assert_eq!(cursor, case.markdown.len(), "example {}", case.example);
    }
}

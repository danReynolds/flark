use std::collections::BTreeMap;
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

/// A deliberately non-gating receipt. The exact fixture tests are the gates;
/// this exposes how much of CommonMark the current architecture slice gets by
/// accident and prevents a small green subset from looking more complete.
#[test]
fn print_commonmark_0312_scorecard() {
    let mut sections: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut section_failures: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut passed = 0;
    let cases = examples();
    let mut first_failures = Vec::new();
    for case in &cases {
        let actual = render_html(&parse(&case.markdown));
        let matched = actual == case.html;
        let score = sections.entry(case.section.clone()).or_default();
        score.1 += 1;
        if matched {
            score.0 += 1;
            passed += 1;
        } else if first_failures.len() < 12 {
            first_failures.push(case.example);
        }
        if !matched {
            section_failures
                .entry(case.section.clone())
                .or_default()
                .push(case.example);
        }
    }

    eprintln!("COMMONMARK_SCORE {passed}/{}", cases.len());
    for (section, (section_passed, total)) in sections {
        eprintln!("{section_passed:>3}/{total:<3} {section}");
    }
    eprintln!("FIRST_FAILURES {first_failures:?}");
    for (section, failures) in section_failures {
        if failures.len() <= 20 {
            eprintln!("FAILURES {section}: {failures:?}");
        }
    }
    assert!(passed >= 61);
}

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use flark_owned_parser_trial::{parse, render_html};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: usize,
    cases: Vec<ManifestCase>,
}

#[derive(Debug, Deserialize)]
struct ManifestCase {
    source: String,
    example: usize,
    mechanism: String,
}

#[derive(Debug, Deserialize)]
struct Example {
    markdown: String,
    html: String,
    example: usize,
}

fn fixtures(path: &Path) -> BTreeMap<usize, Example> {
    serde_json::from_str::<Vec<Example>>(&fs::read_to_string(path).unwrap())
        .unwrap()
        .into_iter()
        .map(|case| (case.example, case))
        .collect()
}

/// Non-gating until individual stress mechanisms are implemented. This is a
/// visible receipt, not permission to call the architecture milestone done.
#[test]
fn print_pinned_architecture_stress_scorecard() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest: Manifest =
        serde_json::from_str(&fs::read_to_string(root.join("stress_manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest.schema_version, 1);
    let fixture_root = root.join("../../../test/fixtures/commonmark/upstream");
    let commonmark = fixtures(&fixture_root.join("common_mark_tests.json"));
    let gfm = fixtures(&fixture_root.join("gfm_tests.json"));

    let mut passed = 0;
    for case in &manifest.cases {
        let fixture = match case.source.as_str() {
            "commonmark" => &commonmark[&case.example],
            "gfm" => &gfm[&case.example],
            unknown => panic!("unknown fixture source {unknown}"),
        };
        let actual = render_html(&parse(&fixture.markdown));
        let matched = actual == fixture.html;
        passed += matched as usize;
        eprintln!(
            "STRESS {}:{} {} — {}",
            case.source,
            case.example,
            if matched { "PASS" } else { "FAIL" },
            case.mechanism
        );
    }
    eprintln!("STRESS_SCORE {passed}/{}", manifest.cases.len());
}

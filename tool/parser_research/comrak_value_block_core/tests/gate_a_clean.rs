use std::collections::BTreeMap;
use std::path::PathBuf;

use flark_comrak_value_block_core::{SyntaxProfile, normalized_html, parse_document};
use flark_gate_a_harness::{FixtureAuthority, load_gate_a_fixtures};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[test]
fn clean_gate_a_is_exact() {
    let fixtures = load_gate_a_fixtures(&repo_root()).expect("load Gate A fixtures");
    assert_eq!(fixtures.len(), 189);
    let mut failures = Vec::new();
    let mut passes = BTreeMap::<String, usize>::new();
    for gate in fixtures {
        let profile = match gate.authority {
            FixtureAuthority::CommonMark0312 => SyntaxProfile::CommonMark,
            FixtureAuthority::Gfm029 => SyntaxProfile::Gfm,
        };
        let result = parse_document(&gate.fixture.markdown, profile)
            .map_err(|error| format!("parse: {error:?}"))
            .and_then(|document| {
                normalized_html(&document).map_err(|error| format!("render: {error:?}"))
            });
        match result {
            Ok(actual) if actual == gate.fixture.html => {
                *passes.entry(gate.fixture.section.clone()).or_default() += 1;
            }
            Ok(actual) => failures.push(format!(
                "{:?} example {} ({}):\nexpected: {:?}\nactual:   {:?}",
                gate.authority,
                gate.fixture.example,
                gate.fixture.section,
                gate.fixture.html,
                actual,
            )),
            Err(error) => failures.push(format!(
                "{:?} example {} ({}): {error}",
                gate.authority, gate.fixture.example, gate.fixture.section,
            )),
        }
    }
    eprintln!("GATE_A_CLEAN passes={passes:?} failures={}", failures.len());
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

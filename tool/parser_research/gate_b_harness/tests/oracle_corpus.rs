use std::collections::BTreeMap;
use std::path::PathBuf;

use flark_gate_b_harness::{
    gate_b_histories, load_gate_b_fixtures, minimal_edit, FixtureAuthority,
    COMMONMARK_GATE_B_SECTIONS, GFM_GATE_B_SECTIONS,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[test]
fn normative_inline_fixture_profile_is_exactly_pinned() {
    let fixtures = load_gate_b_fixtures(&repo_root()).unwrap();
    assert_eq!(fixtures.len(), 398);
    let counts = fixtures.iter().fold(BTreeMap::new(), |mut counts, gate| {
        *counts
            .entry((gate.authority, gate.fixture.section.clone()))
            .or_insert(0usize) += 1;
        counts
    });

    let expected_commonmark = [
        ("Backslash escapes", 13),
        ("Entity and numeric character references", 17),
        ("Code spans", 22),
        ("Emphasis and strong emphasis", 132),
        ("Links", 90),
        ("Images", 22),
        ("Autolinks", 19),
        ("Raw HTML", 20),
        ("Hard line breaks", 15),
        ("Soft line breaks", 2),
        ("Textual content", 3),
        ("Link reference definitions", 27),
        ("Inlines", 1),
        ("Precedence", 1),
    ];
    for (section, expected) in expected_commonmark {
        assert_eq!(
            counts[&(FixtureAuthority::CommonMark0312, section.to_owned())],
            expected,
            "{section}"
        );
    }
    assert_eq!(COMMONMARK_GATE_B_SECTIONS.len(), expected_commonmark.len());
    assert_eq!(
        counts[&(FixtureAuthority::Gfm029, "Autolinks (extension)".to_owned())],
        11
    );
    assert_eq!(
        counts[&(
            FixtureAuthority::Gfm029,
            "Strikethrough (extension)".to_owned()
        )],
        2
    );
    assert_eq!(
        counts[&(
            FixtureAuthority::Gfm029,
            "Disallowed Raw HTML (extension)".to_owned()
        )],
        1
    );
    assert_eq!(GFM_GATE_B_SECTIONS.len(), 3);
}

#[test]
fn every_revision_histories_are_scalar_safe_real_edits() {
    let histories = gate_b_histories();
    assert_eq!(histories.len(), 11);
    let mut revisions = 0;
    for history in histories {
        assert!(history.revisions.len() >= 4, "{}", history.name);
        for (index, pair) in history.revisions.windows(2).enumerate() {
            let edit = minimal_edit(index as u64, &pair[0].source, &pair[1].source);
            assert_eq!(edit.apply(&pair[0].source).unwrap(), pair[1].source);
            revisions += 1;
        }
    }
    assert!(revisions >= 650, "only {revisions} intermediate revisions");
}

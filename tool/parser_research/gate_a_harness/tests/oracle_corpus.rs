use std::collections::BTreeMap;
use std::path::PathBuf;

use flark_gate_a_harness::{
    gate_a_histories, load_gate_a_fixtures, minimal_edit, oracle_html, oracle_html_for,
    pulldown_oracle_html_for, FixtureAuthority,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[test]
fn exact_gate_a_fixture_corpus_is_complete_and_matches_pinned_comrak() {
    let fixtures = load_gate_a_fixtures(&repo_root()).unwrap();
    let mut pulldown_serialization_differences = 0;
    let counts = fixtures.iter().fold(BTreeMap::new(), |mut counts, gate| {
        *counts
            .entry((gate.authority, gate.fixture.section.clone()))
            .or_insert(0usize) += 1;
        counts
    });
    assert_eq!(fixtures.len(), 189);
    assert_eq!(
        counts[&(FixtureAuthority::CommonMark0312, "Tabs".into())],
        11
    );
    assert_eq!(
        counts[&(FixtureAuthority::CommonMark0312, "Setext headings".into())],
        27
    );
    assert_eq!(
        counts[&(FixtureAuthority::CommonMark0312, "HTML blocks".into())],
        44
    );
    assert_eq!(
        counts[&(FixtureAuthority::CommonMark0312, "Block quotes".into())],
        25
    );
    assert_eq!(
        counts[&(FixtureAuthority::CommonMark0312, "List items".into())],
        48
    );
    assert_eq!(
        counts[&(FixtureAuthority::CommonMark0312, "Lists".into())],
        26
    );
    assert_eq!(
        counts[&(FixtureAuthority::Gfm029, "Tables (extension)".into())],
        8
    );

    for gate in fixtures {
        let actual = oracle_html_for(gate.authority.profile(), &gate.fixture.markdown).unwrap();
        assert_eq!(
            actual, gate.fixture.html,
            "{:?} example {} ({})",
            gate.authority, gate.fixture.example, gate.fixture.section
        );
        if pulldown_oracle_html_for(gate.authority.profile(), &gate.fixture.markdown)
            != gate.fixture.html
        {
            pulldown_serialization_differences += 1;
        }
    }
    eprintln!(
        "PULLDOWN_GATE_A_EXACT_SERIALIZATION differences={pulldown_serialization_differences}"
    );
    assert!(pulldown_serialization_differences > 0);
}

#[test]
fn every_revision_histories_are_valid_scalar_safe_splices() {
    let histories = gate_a_histories();
    assert_eq!(histories.len(), 10);
    assert_eq!(
        histories
            .iter()
            .flat_map(|history| &history.revisions)
            .map(|revision| revision.facts.len())
            .sum::<usize>(),
        9
    );
    let mut revisions = 0;
    let mut independent_oracle_disagreements = 0;
    for history in histories {
        assert!(history.revisions.len() >= 3, "{}", history.name);
        for (index, pair) in history.revisions.windows(2).enumerate() {
            let edit = minimal_edit(index as u64, &pair[0].source, &pair[1].source);
            assert_eq!(edit.apply(&pair[0].source).unwrap(), pair[1].source);
            // Oracle every intermediate revision, including incomplete syntax.
            let comrak = oracle_html(&pair[1].source).unwrap();
            let pulldown = pulldown_oracle_html_for(history.profile, &pair[1].source);
            independent_oracle_disagreements += usize::from(comrak != pulldown);
            revisions += 1;
        }
    }
    assert!(revisions >= 400, "only {revisions} ambiguity revisions");
    eprintln!("PULLDOWN_PARTIAL_REVISION_DISAGREEMENTS count={independent_oracle_disagreements}");
    assert!(independent_oracle_disagreements > 0);
}

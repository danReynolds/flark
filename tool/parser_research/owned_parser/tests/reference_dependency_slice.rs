use flark_owned_parser_trial::{
    AdvanceStatus, CancelFlag, EditRequest, IncrementalDelta, RevisionedDocument, WorkBudget,
};

fn apply(
    document: &mut RevisionedDocument,
    start: usize,
    end: usize,
    replacement: &str,
) -> IncrementalDelta {
    let mut session = document
        .begin_edit(EditRequest {
            base_revision: document.revision(),
            before_hash32: document.hash32(),
            start_utf8: start,
            end_utf8: end,
            replacement: replacement.to_owned(),
        })
        .unwrap();
    loop {
        match session.advance(WorkBudget::new(4, 256), &CancelFlag::default()) {
            AdvanceStatus::Pending => {}
            AdvanceStatus::Converged => break,
            AdvanceStatus::Cancelled => unreachable!(),
        }
    }
    let delta = document.adopt(session.into_result().unwrap()).unwrap();
    document.assert_checkpoint_oracle();
    delta
}

#[test]
fn forward_and_container_definitions_feed_a_global_first_wins_index() {
    let source = "[Foo]\n\n> [FOO]: /first \"title\"\n\n[foo]: /second\n";
    let document = RevisionedDocument::new(source);
    assert_eq!(document.reference_target("foo"), Some(("/first", "title")));
    assert_eq!(document.reference_lookup_count("fOo"), 1);

    let inert = RevisionedDocument::new("```\n[x]: /not-a-definition\n```\n\n[x]\n");
    assert_eq!(inert.reference_target("x"), None);
    assert_eq!(inert.reference_lookup_count("x"), 1);
}

#[test]
fn winner_changes_invalidate_distant_lookups_without_reparsing_them() {
    let mut source = String::from("[shared]: /first\n\n");
    for index in 0..20_000 {
        source.push_str(&format!("paragraph {index} uses [shared]\n\n"));
    }
    source.push_str("[shared]: /second\n");
    let mut document = RevisionedDocument::new(&source);
    assert_eq!(document.reference_lookup_count("shared"), 20_000);

    let first = source.find("/first").unwrap();
    let delta = apply(&mut document, first, first + 6, "/prime");
    assert_eq!(delta.references.presence_changed, Vec::<String>::new());
    assert_eq!(delta.references.value_changed, ["shared"]);
    assert_eq!(delta.references.invalidated_lookup_records, 20_000);
    assert_eq!(document.reference_target("shared"), Some(("/prime", "")));
    assert!(delta.reparsed_lines <= 3, "{delta:?}");
    assert!(delta.reparsed_bytes < 80, "{delta:?}");

    let second = document.materialize().rfind("/second").unwrap();
    let duplicate = apply(&mut document, second, second + 7, "/third");
    assert!(duplicate.references.value_changed.is_empty());
    assert_eq!(duplicate.references.invalidated_lookup_records, 0);
    assert_eq!(document.reference_target("shared"), Some(("/prime", "")));
}

#[test]
fn adding_a_missing_definition_reports_presence_dependencies() {
    let source = "[missing]\n\nplain text\n\nafter\n";
    let mut document = RevisionedDocument::new(source);
    assert_eq!(document.reference_target("missing"), None);
    let start = source.find("plain text").unwrap();
    let delta = apply(
        &mut document,
        start,
        start + "plain text".len(),
        "[missing]: /now",
    );
    assert_eq!(delta.references.presence_changed, ["missing"]);
    assert_eq!(delta.references.invalidated_lookup_records, 1);
    assert_eq!(document.reference_target("missing"), Some(("/now", "")));
}

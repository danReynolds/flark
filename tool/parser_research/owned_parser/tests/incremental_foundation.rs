use flark_owned_parser_trial::{
    AdvanceStatus, CancelFlag, EditRequest, IncrementalError, RevisionedDocument, WorkBudget,
};

fn apply(
    document: &mut RevisionedDocument,
    start: usize,
    end: usize,
    replacement: &str,
    budget: WorkBudget,
) -> flark_owned_parser_trial::IncrementalDelta {
    let mut session = document
        .begin_edit(EditRequest {
            base_revision: document.revision(),
            before_hash32: document.hash32(),
            start_utf8: start,
            end_utf8: end,
            replacement: replacement.to_owned(),
        })
        .unwrap();
    let cancel = CancelFlag::default();
    loop {
        match session.advance(budget, &cancel) {
            AdvanceStatus::Pending => {}
            AdvanceStatus::Converged => break,
            AdvanceStatus::Cancelled => panic!("unexpected cancellation"),
        }
    }
    let delta = document.adopt(session.into_result().unwrap()).unwrap();
    document.assert_checkpoint_oracle();
    delta
}

#[test]
fn localized_edit_converges_and_reuses_checkpoint_suffix() {
    let source = (0..20_000)
        .map(|index| format!("paragraph line {index}\n\n"))
        .collect::<String>();
    let mut document = RevisionedDocument::new(&source);
    let needle = "paragraph line 10000";
    let start = source.find(needle).unwrap();
    let delta = apply(
        &mut document,
        start,
        start + needle.len(),
        "changed paragraph 10000",
        WorkBudget::new(2, 256),
    );
    assert!(delta.reparsed_lines <= 4, "{delta:?}");
    assert!(delta.reused_checkpoints > 30_000, "{delta:?}");
    assert!(document.checkpoint_height() < 32);
}

#[test]
fn giant_fence_edit_is_local_but_opener_change_propagates_to_eof() {
    let mut source = String::from("```rust\n");
    while source.len() < 1_000_000 {
        source.push_str("payload has *literal* markdown\n");
    }
    source.push_str("```\nafter\n");
    let mut document = RevisionedDocument::new(&source);
    let middle = source.len() / 2;
    let payload = source[middle..].find("payload").unwrap() + middle;
    let local = apply(
        &mut document,
        payload,
        payload + 7,
        "changed",
        WorkBudget::new(1, 128),
    );
    assert!(local.reparsed_lines <= 4, "{local:?}");
    assert!(local.reparsed_bytes < 160, "{local:?}");

    let opener = document.materialize().find("```").unwrap();
    let propagated = apply(
        &mut document,
        opener,
        opener + 3,
        "~~~",
        WorkBudget::new(64, 4096),
    );
    assert!(
        propagated.reparsed_bytes > 900_000,
        "changed fence kind must invalidate through the old closer: {propagated:?}"
    );
}

#[test]
fn work_is_resumable_cancelable_and_revision_safe() {
    let source = (0..20_000)
        .map(|index| format!("line {index}\n"))
        .collect::<String>();
    let document = RevisionedDocument::new(&source);
    let mut session = document
        .begin_edit(EditRequest {
            base_revision: 0,
            before_hash32: document.hash32(),
            start_utf8: 0,
            end_utf8: 0,
            replacement: "```\n".to_owned(),
        })
        .unwrap();
    let cancel = CancelFlag::default();
    assert_eq!(
        session.advance(WorkBudget::new(1, 32), &cancel),
        AdvanceStatus::Pending
    );
    cancel.cancel();
    assert_eq!(
        session.advance(WorkBudget::new(1, 32), &cancel),
        AdvanceStatus::Cancelled
    );
    assert!(matches!(
        session.into_result(),
        Err(IncrementalError::NotConverged)
    ));
}

#[test]
fn edit_does_not_false_converge_inside_multiline_paragraph_or_setext_heading() {
    let mut document = RevisionedDocument::new("first\nsecond\nthird\n\nafter\n");
    let paragraph = apply(
        &mut document,
        0,
        "first".len(),
        "changed",
        WorkBudget::new(1, 64),
    );
    assert!(
        paragraph.convergence_utf8 >= "changed\nsecond\nthird\n\n".len(),
        "must invalidate through the paragraph boundary: {paragraph:?}"
    );

    let mut document = RevisionedDocument::new("title\n=====\nafter\n");
    let heading = apply(
        &mut document,
        0,
        "title".len(),
        "changed",
        WorkBudget::new(1, 64),
    );
    assert!(
        heading.convergence_utf8 > "changed\n=====\n".len(),
        "must not retain an old setext underline checkpoint whose semantic text changed: {heading:?}"
    );
}

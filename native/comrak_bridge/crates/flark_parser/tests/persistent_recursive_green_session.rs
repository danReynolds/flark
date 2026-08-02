use flark_engine::parser_internal::M11RecursiveGreenPoint;
use flark_engine::{DocumentRuntime, DocumentRuntimeConfig, SourceBoundaryAffinity};
use flark_parser::{
    M11PersistentRecursiveGreenAdoptionStatus, M11PersistentRecursiveGreenBuildStatus,
    M11PersistentRecursiveGreenCleanPlan, M11PersistentRecursiveGreenSession,
};

fn build_session(runtime: &mut DocumentRuntime) -> M11PersistentRecursiveGreenSession {
    let plan = M11PersistentRecursiveGreenCleanPlan::new(
        runtime.snapshot_current_source().expect("scanner lease"),
        runtime.snapshot_current_source().expect("writer lease"),
        1,
    )
    .expect("clean plan");
    let mut build = plan.begin(runtime).expect("clean build");
    loop {
        let poll = build.poll(runtime, 64).expect("poll clean build");
        if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
            return build.take_session().expect("persistent session");
        }
    }
}

fn release_session(
    runtime: &mut DocumentRuntime,
    session: &mut M11PersistentRecursiveGreenSession,
) {
    session.begin_release(runtime).expect("begin release");
    while !session.poll_release(runtime, 64).expect("poll release") {}
}

#[test]
fn clean_session_retains_green_and_reference_authority_for_late_queries() {
    let source = "[a]: /target\n\nbefore\n\nLate **bold** [a].\n";
    let mut runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("document runtime");
    let mut session = build_session(&mut runtime);
    assert_eq!(session.reference_occurrence_count(), 1);
    assert!(session.checkpoint_count() >= 1);

    let point = source.find("bold").expect("late paragraph point");
    let prepared = session
        .prepare_paragraph_inline(
            &runtime,
            M11RecursiveGreenPoint::new(point, point, SourceBoundaryAffinity::After),
        )
        .expect("retained Green query");
    assert_eq!(
        &source[prepared.block_source_range().start as usize
            ..prepared.block_source_range().end as usize],
        "Late **bold** [a].\n"
    );
    assert_eq!(prepared.driver_work(), 0);

    release_session(&mut runtime, &mut session);
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("poll close").complete {}
}

#[test]
fn large_session_keeps_two_late_edits_local_and_reference_rebind_constant() {
    let mut source = String::from("[a]: /target\n\n");
    for ordinal in 0..9_000 {
        source.push_str(&format!(
            "Paragraph {ordinal:05} carries enough ordinary source for sparse restart spacing.\n\n"
        ));
    }
    source.push_str("Late **bold** and _live_ [a].\n\n");
    for ordinal in 0..100 {
        source.push_str(&format!(
            "Trailing convergence paragraph {ordinal:03} keeps an unchanged suffix available.\n\n"
        ));
    }
    assert!(source.len() > 512 * 1024);

    let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
        .expect("large document runtime");
    let session0 = build_session(&mut runtime);
    assert!(session0.checkpoint_count() < source.len() / 2048);

    let first_start = source.rfind("bold").expect("first edit");
    let base0 = session0.source();
    runtime
        .apply_edit(base0, first_start..first_start + 4, "strong")
        .expect("first edit");
    let target1_lease = runtime
        .snapshot_current_source()
        .expect("first target lease");
    let mut adoption1 = session0
        .begin_local_adoption(&runtime, target1_lease, first_start..first_start + 4)
        .unwrap_or_else(|failure| panic!("first adoption start: {}", failure.error()));
    loop {
        let poll = adoption1.poll(&mut runtime, 64).expect("first adoption");
        match poll.status() {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::Complete => break,
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                panic!("first edit unexpectedly required clean fallback")
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("first edit adoption was not cancelled")
            }
        }
    }
    let mut update1 = adoption1.take_update().expect("first update");
    let work1 = update1.work();
    assert!(work1.source_bytes_read() < 16 * 1024);
    assert!(work1.green_tree_nodes_rebuilt() < 256);
    assert!(work1.reference_rebind_transitions() <= 4);
    let mut superseded0 = update1.take_base().expect("first base");
    let session1 = update1.take_target().expect("first target");
    release_session(&mut runtime, &mut superseded0);

    let target1_source = runtime.current_source_version().expect("target1 source");
    let target1_text = "Late **strong** and _live_ [a].\n";
    let target1_start = source.rfind("Late **bold**").expect("late island start");
    let second_start = target1_start + target1_text.find("live").expect("second edit");
    runtime
        .apply_edit(target1_source, second_start..second_start + 4, "fluid")
        .expect("second edit");
    let target2_lease = runtime
        .snapshot_current_source()
        .expect("second target lease");
    let mut adoption2 = session1
        .begin_local_adoption(&runtime, target2_lease, second_start..second_start + 4)
        .unwrap_or_else(|failure| panic!("second adoption start: {}", failure.error()));
    loop {
        let poll = adoption2.poll(&mut runtime, 64).expect("second adoption");
        match poll.status() {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::Complete => break,
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                panic!("second edit unexpectedly required clean fallback")
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("second edit adoption was not cancelled")
            }
        }
    }
    let mut update2 = adoption2.take_update().expect("second update");
    let work2 = update2.work();
    assert!(work2.source_bytes_read() < 16 * 1024);
    assert!(work2.green_tree_nodes_rebuilt() < 256);
    assert!(work2.reference_rebind_transitions() <= 4);
    let mut superseded1 = update2.take_base().expect("second base");
    let mut session2 = update2.take_target().expect("second target");
    release_session(&mut runtime, &mut superseded1);

    let target2_text = "Late **strong** and _fluid_ [a].\n";
    let target2_start = target1_start;
    let prepared = session2
        .prepare_paragraph_inline(
            &runtime,
            M11RecursiveGreenPoint::new(
                target2_start + 10,
                target2_start + 10,
                SourceBoundaryAffinity::After,
            ),
        )
        .expect("late retained query");
    assert_eq!(
        prepared.block_source_range(),
        target2_start as u32..(target2_start + target2_text.len()) as u32
    );
    assert_eq!(prepared.driver_work(), 0);

    release_session(&mut runtime, &mut session2);
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(64).expect("poll close").complete {}
}

#[test]
fn commonmark_321_and_325_stay_generic_exact_and_local_inside_a_large_session() {
    const CM321: &str = "- a\n  > b\n  ```\n  c\n  ```\n- d\n";
    const CM325: &str = "* foo\n  * bar\n\n  baz\n";

    let mut source = String::new();
    for ordinal in 0..9_000 {
        source.push_str(&format!(
            "Prefix paragraph {ordinal:05} carries enough ordinary source for sparse restart spacing.\n\n"
        ));
    }
    let cm321_start = source.len();
    source.push_str(CM321);
    source.push('\n');
    source.push_str(CM325);
    source.push('\n');
    for ordinal in 0..1_000 {
        source.push_str(&format!(
            "Trailing paragraph {ordinal:04} remains an unchanged serialized-Green sibling.\n\n"
        ));
    }
    assert!(source.len() > 512 * 1024);

    let edit_start = cm321_start + CM321.find("> b").expect("quote child") + 2;
    let prefix_probe = source.find("Prefix paragraph 00000").expect("prefix probe");
    let suffix_probe = source
        .rfind("Trailing paragraph 0999")
        .expect("suffix probe");
    let mut target_source = source.clone();
    target_source.replace_range(edit_start..edit_start + 1, "beta");
    const LENGTH_DELTA: usize = "beta".len() - 1;

    let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
        .expect("large recursive-container runtime");
    let session0 = build_session(&mut runtime);
    let base_prefix_page = session0
        .storage_page_identity_at_source_byte_for_diagnostics(&runtime, prefix_probe)
        .expect("base prefix page");
    let base_suffix_page = session0
        .storage_page_identity_at_source_byte_for_diagnostics(&runtime, suffix_probe)
        .expect("base suffix page");

    let base = session0.source();
    runtime
        .apply_edit(base, edit_start..edit_start + 1, "beta")
        .expect("edit nested quote child");
    let target_lease = runtime
        .snapshot_current_source()
        .expect("recursive-container target lease");
    let mut adoption = session0
        .begin_local_adoption(&runtime, target_lease, edit_start..edit_start + 1)
        .unwrap_or_else(|failure| panic!("container adoption start: {}", failure.error()));
    loop {
        let poll = adoption.poll(&mut runtime, 64).expect("container adoption");
        match poll.status() {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::Complete => break,
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                panic!("nested container edit unexpectedly required clean fallback")
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("nested container adoption was cancelled")
            }
        }
    }

    let mut update = adoption.take_update().expect("container update");
    let work = update.work();
    assert!(work.source_bytes_read() < 16 * 1024);
    assert!(work.green_tree_nodes_rebuilt() < 256);
    let mut superseded = update.take_base().expect("container base");
    let mut target = update.take_target().expect("container target");

    assert_eq!(
        target
            .storage_page_identity_at_source_byte_for_diagnostics(&runtime, prefix_probe)
            .expect("target prefix page"),
        base_prefix_page,
    );
    assert_eq!(
        target
            .storage_page_identity_at_source_byte_for_diagnostics(
                &runtime,
                suffix_probe + LENGTH_DELTA,
            )
            .expect("target suffix page"),
        base_suffix_page,
    );
    let prepared = target
        .prepare_paragraph_inline(
            &runtime,
            M11RecursiveGreenPoint::new(
                edit_start + 1,
                edit_start + 1,
                SourceBoundaryAffinity::After,
            ),
        )
        .expect("nested quote Paragraph query");
    assert!(target_source[prepared.inline_source_range().start as usize
        ..prepared.inline_source_range().end as usize]
        .contains("beta"));

    let incremental_digest = target
        .semantic_digest_for_diagnostics(&runtime)
        .expect("incremental semantic digest");
    let mut clean_runtime = DocumentRuntime::new(&target_source, DocumentRuntimeConfig::default())
        .expect("clean target runtime");
    let mut clean_target = build_session(&mut clean_runtime);
    assert_eq!(
        clean_target
            .semantic_digest_for_diagnostics(&clean_runtime)
            .expect("clean semantic digest"),
        incremental_digest,
    );

    release_session(&mut runtime, &mut superseded);
    release_session(&mut runtime, &mut target);
    runtime
        .begin_close()
        .expect("begin container runtime close");
    while !runtime
        .poll_close(64)
        .expect("poll container runtime close")
        .complete
    {}
    release_session(&mut clean_runtime, &mut clean_target);
    clean_runtime
        .begin_close()
        .expect("begin clean runtime close");
    while !clean_runtime
        .poll_close(64)
        .expect("poll clean runtime close")
        .complete
    {}
}

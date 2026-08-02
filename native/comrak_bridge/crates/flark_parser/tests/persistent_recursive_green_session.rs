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

fn cancellation_fixture() -> (String, usize) {
    let mut source = String::from("[a]: /target\n\n");
    for ordinal in 0..240 {
        source.push_str(&format!(
            "Paragraph {ordinal:03} carries enough ordinary source for sparse restart spacing.\n\n"
        ));
    }
    let edit_start = source
        .find("Paragraph 140")
        .and_then(|paragraph| {
            source[paragraph..]
                .find("ordinary")
                .map(|word| paragraph + word)
        })
        .expect("late cancellation edit");
    (source, edit_start)
}

fn assert_original_base(
    base: &M11PersistentRecursiveGreenSession,
    source: flark_engine::SourceVersion,
    checkpoints: usize,
    references: u64,
) {
    assert_eq!(base.source(), source);
    assert_eq!(base.checkpoint_count(), checkpoints);
    assert_eq!(base.reference_occurrence_count(), references);
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
fn leading_reference_visible_remainder_edit_uses_local_recursive_green_adoption() {
    const REFERENCES: usize = 128;
    let mut source = String::new();
    for ordinal in 0..REFERENCES {
        source.push_str(&format!("[r{ordinal}]: /target/{ordinal}\n"));
    }
    source.push_str("visible **bold** tail\n");
    let edit_start = source.rfind("bold").expect("visible tail edit");
    let mut target_source = source.clone();
    target_source.replace_range(edit_start..edit_start + 4, "strong");

    let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
        .expect("leading-reference runtime");
    let session = build_session(&mut runtime);
    assert_eq!(session.reference_occurrence_count(), REFERENCES as u64);
    assert!(session.checkpoint_count() >= 2);
    let base = session.source();
    runtime
        .apply_edit(base, edit_start..edit_start + 4, "strong")
        .expect("visible tail edit");
    let target_lease = runtime.snapshot_current_source().expect("target lease");
    let mut adoption = session
        .begin_local_adoption(&runtime, target_lease, edit_start..edit_start + 4)
        .unwrap_or_else(|failure| panic!("remainder adoption start: {}", failure.error()));
    loop {
        match adoption
            .poll(&mut runtime, 64)
            .expect("poll remainder adoption")
            .status()
        {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::Complete => break,
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                panic!("visible remainder edit required clean fallback")
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("visible remainder edit was cancelled")
            }
        }
    }
    let mut update = adoption.take_update().expect("local remainder update");
    assert!(update.work().source_bytes_read() < 256);
    assert!(update.work().green_tree_nodes_rebuilt() < 64);
    assert!(update.work().reference_rebind_transitions() <= 4);
    let mut superseded = update.take_base().expect("superseded base");
    let mut target = update.take_target().expect("target session");
    assert_eq!(target.reference_occurrence_count(), REFERENCES as u64);

    let incremental_digest = target
        .semantic_digest_for_diagnostics(&runtime)
        .expect("incremental digest");
    let mut clean_runtime = DocumentRuntime::new(&target_source, DocumentRuntimeConfig::default())
        .expect("clean oracle runtime");
    let mut clean = build_session(&mut clean_runtime);
    assert_eq!(
        incremental_digest,
        clean
            .semantic_digest_for_diagnostics(&clean_runtime)
            .expect("clean digest")
    );

    release_session(&mut clean_runtime, &mut clean);
    clean_runtime.begin_close().expect("begin clean close");
    while !clean_runtime
        .poll_close(64)
        .expect("poll clean close")
        .complete
    {}
    release_session(&mut runtime, &mut superseded);
    release_session(&mut runtime, &mut target);
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(64).expect("poll close").complete {}
}

#[test]
fn local_adoption_cancel_before_splice_returns_complete_original_base() {
    let (source, edit_start) = cancellation_fixture();
    let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
        .expect("cancellation runtime");
    let session = build_session(&mut runtime);
    let base_source = session.source();
    let base_checkpoints = session.checkpoint_count();
    let base_references = session.reference_occurrence_count();
    let base_resident_nodes = runtime.arena_metrics().resident_nodes;
    assert!(base_checkpoints >= 3);

    runtime
        .apply_edit(
            base_source,
            edit_start..edit_start + "ordinary".len(),
            "precise_",
        )
        .expect("cancellation edit");
    let target_lease = runtime
        .snapshot_current_source()
        .expect("cancellation target lease");
    let mut adoption = session
        .begin_local_adoption(
            &runtime,
            target_lease,
            edit_start..edit_start + "ordinary".len(),
        )
        .unwrap_or_else(|failure| panic!("cancellation adoption start: {}", failure.error()));

    adoption
        .begin_cancel(&mut runtime)
        .expect("begin pre-splice cancellation");
    while !adoption
        .poll_cancel(&mut runtime, 1)
        .expect("poll pre-splice cancellation")
    {}
    let mut base = adoption
        .take_base_after_cancel()
        .expect("complete base after pre-splice cancellation");
    assert_original_base(&base, base_source, base_checkpoints, base_references);
    assert_eq!(runtime.arena_metrics().resident_nodes, base_resident_nodes);

    release_session(&mut runtime, &mut base);
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("poll runtime close").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
}

#[test]
fn local_adoption_cancel_after_splice_releases_target_and_returns_complete_original_base() {
    let (source, edit_start) = cancellation_fixture();
    let mut runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default())
        .expect("cancellation runtime");
    let session = build_session(&mut runtime);
    while !runtime.poll_retirement(1).complete {}
    let base_source = session.source();
    let base_checkpoints = session.checkpoint_count();
    let base_references = session.reference_occurrence_count();
    let base_arena_metrics = runtime.arena_metrics();
    let base_resident_nodes = base_arena_metrics.resident_nodes;
    assert!(base_checkpoints >= 3);
    let base_page_probe_offsets = [
        source.find("Paragraph 010").expect("prefix page probe"),
        source.find("Paragraph 140").expect("edited page probe"),
        source.find("Paragraph 230").expect("suffix page probe"),
    ];
    let base_page_probes = base_page_probe_offsets.map(|probe| {
        session
            .storage_page_identity_at_source_byte_for_diagnostics(&runtime, probe)
            .expect("base page identity")
    });

    runtime
        .apply_edit(
            base_source,
            edit_start..edit_start + "ordinary".len(),
            "precise_",
        )
        .expect("cancellation edit");
    let target_lease = runtime
        .snapshot_current_source()
        .expect("cancellation target lease");
    let mut adoption = session
        .begin_local_adoption(
            &runtime,
            target_lease,
            edit_start..edit_start + "ordinary".len(),
        )
        .unwrap_or_else(|failure| panic!("cancellation adoption start: {}", failure.error()));

    let mut target_resident_nodes = None;
    for _ in 0..20_000 {
        let poll = adoption
            .poll(&mut runtime, 1)
            .expect("poll adoption through structural splice");
        match poll.status() {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::Complete => {
                panic!("adoption completed before the post-splice cancellation checkpoint")
            }
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                panic!("post-splice cancellation fixture required clean fallback")
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("adoption cancelled before cancellation was requested")
            }
        }
        if runtime.arena_metrics().resident_nodes > base_resident_nodes {
            target_resident_nodes = Some(runtime.arena_metrics().resident_nodes);
            break;
        }
    }
    let target_resident_nodes =
        target_resident_nodes.expect("target Green root was never installed");

    adoption
        .begin_cancel(&mut runtime)
        .expect("begin post-splice cancellation");
    while !adoption
        .poll_cancel(&mut runtime, 1)
        .expect("poll post-splice cancellation")
    {}
    let mut base = adoption
        .take_base_after_cancel()
        .expect("complete base after post-splice cancellation");
    assert_original_base(&base, base_source, base_checkpoints, base_references);
    assert_eq!(
        base_page_probes,
        base_page_probe_offsets.map(|probe| {
            base.storage_page_identity_at_source_byte_for_diagnostics(&runtime, probe)
                .expect("restored base page identity")
        })
    );
    assert!(target_resident_nodes > base_resident_nodes);
    let restored_arena_metrics = runtime.arena_metrics();
    assert_eq!(
        (
            restored_arena_metrics.resident_nodes,
            restored_arena_metrics.live_payload_bytes,
            restored_arena_metrics.reserved_external_payload_bytes,
            restored_arena_metrics.pending_reclaims,
            restored_arena_metrics.live_builds,
            restored_arena_metrics.pending_build_aborts,
        ),
        (
            base_arena_metrics.resident_nodes,
            base_arena_metrics.live_payload_bytes,
            base_arena_metrics.reserved_external_payload_bytes,
            base_arena_metrics.pending_reclaims,
            base_arena_metrics.live_builds,
            base_arena_metrics.pending_build_aborts,
        ),
        "post-splice cancellation must restore every live arena metric",
    );
    assert!(
        restored_arena_metrics.allocated_slots >= base_arena_metrics.allocated_slots,
        "allocator slot high-water must remain monotonic",
    );

    release_session(&mut runtime, &mut base);
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("poll runtime close").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
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

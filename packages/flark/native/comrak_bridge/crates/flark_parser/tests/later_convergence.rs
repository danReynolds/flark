// SPDX-License-Identifier: MIT

use flark_engine::{DocumentRuntime, DocumentRuntimeConfig};
use flark_parser::{
    M11PersistentRecursiveGreenAdoptionStatus, M11PersistentRecursiveGreenBuildStatus,
    M11PersistentRecursiveGreenCleanPlan, M11PersistentRecursiveGreenSession,
};

const FUEL: usize = 64;

fn ordinary_padding(label: &str) -> String {
    ordinary_padding_count(label, 96)
}

fn ordinary_padding_count(label: &str, count: usize) -> String {
    let mut padding = String::new();
    for ordinal in 0..count {
        padding.push_str(&format!(
            "{label} ordinary paragraph {ordinal:03} keeps distant document structure stable.\n\n"
        ));
    }
    padding
}

fn build_session(runtime: &mut DocumentRuntime) -> M11PersistentRecursiveGreenSession {
    let plan = M11PersistentRecursiveGreenCleanPlan::new(
        runtime.snapshot_current_source().expect("scanner lease"),
        runtime.snapshot_current_source().expect("writer lease"),
        1,
    )
    .expect("clean plan");
    let mut build = plan.begin(runtime).expect("clean build");
    loop {
        let poll = build.poll(runtime, FUEL).expect("poll clean build");
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
    while !session.poll_release(runtime, FUEL).expect("poll release") {}
}

fn close_runtime(mut runtime: DocumentRuntime) {
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(FUEL).expect("poll close").complete {}
    let metrics = runtime.arena_metrics();
    assert_eq!(metrics.resident_nodes, 0);
    assert_eq!(metrics.live_payload_bytes, 0);
    assert_eq!(metrics.live_builds, 0);
}

#[test]
fn commonmark_178_advances_past_a_non_resumable_fresh_convergence_boundary() {
    let prefix = ordinary_padding("Prefix");
    let base_source = format!(
        "{prefix}<script>\nfoo\n</script>1. *bar*\n\n\n{}",
        ordinary_padding("Suffix")
    );
    let closer = base_source.find("</script>").expect("HTML closer");
    let edit = closer + 2..closer + 3;
    let mut target_source = base_source.clone();
    target_source.replace_range(edit.clone(), "A");
    assert_eq!(base_source.len(), target_source.len());

    let mut runtime =
        DocumentRuntime::new(&base_source, DocumentRuntimeConfig::default()).expect("runtime");
    let session = build_session(&mut runtime);
    let mut clean_runtime = DocumentRuntime::new(&target_source, DocumentRuntimeConfig::default())
        .expect("clean runtime");
    let mut clean = build_session(&mut clean_runtime);
    assert!(
        clean.checkpoint_count() < session.checkpoint_count(),
        "the broken HTML closer must make at least one base sample unavailable in the target"
    );
    let clean_digest = clean
        .semantic_digest_for_diagnostics(&clean_runtime)
        .expect("clean digest");
    let base = session.source();
    runtime
        .apply_edit(base, edit.clone(), "A")
        .expect("broken closer edit");
    let target_lease = runtime.snapshot_current_source().expect("target lease");
    let mut adoption = session
        .begin_local_adoption(&runtime, target_lease, edit)
        .unwrap_or_else(|failure| panic!("begin local adoption: {}", failure.error()));

    const FUEL_PATTERN: [usize; 4] = [1, 7, 3, FUEL];
    let mut fuel_index = 0_usize;
    loop {
        let fuel = FUEL_PATTERN[fuel_index % FUEL_PATTERN.len()];
        fuel_index += 1;
        let poll = adoption
            .poll(&mut runtime, fuel)
            .expect("poll later-convergence adoption");
        assert!(poll.transitions() <= fuel);
        match poll.status() {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::Complete => break,
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                panic!("bounded later convergence unexpectedly required a clean fallback")
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("later-convergence adoption cancelled without a request")
            }
        }
    }

    let mut update = adoption.take_update().expect("completed update");
    assert!(
        update.work().source_bytes_read() < 16 * 1024,
        "the next authenticated boundary remains inside the local work envelope"
    );
    let mut superseded = update.take_base().expect("superseded base");
    let mut target = update.take_target().expect("target session");
    let incremental_digest = target
        .semantic_digest_for_diagnostics(&runtime)
        .expect("incremental digest");

    assert_eq!(incremental_digest, clean_digest);

    release_session(&mut clean_runtime, &mut clean);
    close_runtime(clean_runtime);
    release_session(&mut runtime, &mut superseded);
    release_session(&mut runtime, &mut target);
    close_runtime(runtime);
}

#[test]
fn lost_html_convergence_fails_clean_before_consuming_an_unbounded_suffix() {
    let prefix = ordinary_padding("Prefix");
    let base_source = format!(
        "{prefix}<script>\nfoo\n</script>1. *bar*\n\n\n{}",
        ordinary_padding_count("Long suffix", 4_096)
    );
    assert!(base_source.len() > 256 * 1024);
    let closer = base_source.find("</script>").expect("HTML closer");
    let edit = closer + 2..closer + 3;

    let mut runtime =
        DocumentRuntime::new(&base_source, DocumentRuntimeConfig::default()).expect("runtime");
    let session = build_session(&mut runtime);
    let base_source_version = session.source();
    let base_checkpoints = session.checkpoint_count();
    runtime
        .apply_edit(base_source_version, edit.clone(), "A")
        .expect("broken closer edit");
    let target_lease = runtime.snapshot_current_source().expect("target lease");
    let mut adoption = session
        .begin_local_adoption(&runtime, target_lease, edit)
        .unwrap_or_else(|failure| panic!("begin local adoption: {}", failure.error()));

    for _ in 0..20_000 {
        let poll = adoption
            .poll(&mut runtime, 7)
            .expect("poll bounded later-convergence search");
        assert!(poll.transitions() <= 7);
        match poll.status() {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => break,
            M11PersistentRecursiveGreenAdoptionStatus::Complete => {
                panic!("lost convergence consumed the unbounded target suffix")
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("later-convergence search cancelled without a request")
            }
        }
    }
    assert_eq!(
        adoption
            .poll(&mut runtime, 1)
            .expect("stable fallback status")
            .status(),
        M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired,
        "later-convergence search did not reach its fixed work envelope"
    );

    adoption
        .begin_cancel(&mut runtime)
        .expect("begin fallback cancellation");
    while !adoption
        .poll_cancel(&mut runtime, FUEL)
        .expect("poll fallback cancellation")
    {}
    let mut base = adoption
        .take_base_after_cancel()
        .expect("original base after fallback cancellation");
    assert_eq!(base.source(), base_source_version);
    assert_eq!(base.checkpoint_count(), base_checkpoints);
    release_session(&mut runtime, &mut base);
    close_runtime(runtime);
}

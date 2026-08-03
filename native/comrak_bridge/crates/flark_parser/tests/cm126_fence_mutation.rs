// SPDX-License-Identifier: MIT

use flark_engine::{DocumentRuntime, DocumentRuntimeConfig};
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

fn close_runtime(mut runtime: DocumentRuntime) {
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(64).expect("poll close").complete {}
    let metrics = runtime.arena_metrics();
    assert_eq!(metrics.resident_nodes, 0);
    assert_eq!(metrics.live_payload_bytes, 0);
}

#[test]
fn commonmark_126_fence_break_never_publishes_a_divergent_local_target() {
    let prefix = String::from("Before\n\n");
    let suffix = String::from("After\n\n");
    let fixture = "```\n";
    let source = format!("{prefix}{fixture}\n\n{suffix}");
    let edit_start = prefix.len() + 2;
    let edit = edit_start..edit_start + 1;
    let mut target_source = source.clone();
    target_source.replace_range(edit.clone(), "~");

    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("base runtime");
    let base = build_session(&mut runtime);
    runtime
        .apply_edit(base.source(), edit.clone(), "~")
        .expect("fence mutation");
    let target_lease = runtime.snapshot_current_source().expect("target lease");
    let mut adoption = match base.begin_local_adoption(&runtime, target_lease, edit) {
        Ok(adoption) => adoption,
        Err(failure) => {
            let mut base = failure.into_base();
            release_session(&mut runtime, &mut base);
            close_runtime(runtime);
            return;
        }
    };
    loop {
        match adoption
            .poll(&mut runtime, 64)
            .expect("poll adoption")
            .status()
        {
            M11PersistentRecursiveGreenAdoptionStatus::Pending => {}
            M11PersistentRecursiveGreenAdoptionStatus::Complete => break,
            M11PersistentRecursiveGreenAdoptionStatus::CleanFallbackRequired => {
                adoption.begin_cancel(&mut runtime).expect("begin cancel");
                while !adoption.poll_cancel(&mut runtime, 64).expect("poll cancel") {}
                let mut base = adoption.take_base_after_cancel().expect("cancelled base");
                release_session(&mut runtime, &mut base);
                close_runtime(runtime);
                return;
            }
            M11PersistentRecursiveGreenAdoptionStatus::Cancelled => {
                panic!("adoption cancelled without request")
            }
        }
    }

    let mut update = adoption.take_update().expect("completed update");
    let mut base = update.take_base().expect("base session");
    let mut target = update.take_target().expect("target session");

    let mut clean_runtime = DocumentRuntime::new(&target_source, DocumentRuntimeConfig::default())
        .expect("clean runtime");
    let mut clean = build_session(&mut clean_runtime);
    assert_eq!(
        target
            .semantic_digest_for_diagnostics(&runtime)
            .expect("incremental digest"),
        clean
            .semantic_digest_for_diagnostics(&clean_runtime)
            .expect("clean digest")
    );

    release_session(&mut runtime, &mut base);
    release_session(&mut runtime, &mut target);
    close_runtime(runtime);
    release_session(&mut clean_runtime, &mut clean);
    close_runtime(clean_runtime);
}

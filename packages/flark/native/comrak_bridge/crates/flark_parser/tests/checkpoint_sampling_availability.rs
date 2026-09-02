// SPDX-License-Identifier: MIT

use flark_engine::{DocumentRuntime, DocumentRuntimeConfig};
use flark_parser::{
    M11PersistentRecursiveGreenBuildStatus, M11PersistentRecursiveGreenCleanPlan,
    M11PersistentRecursiveGreenSession,
};

const FUEL: usize = 64;

fn ordinary_padding(label: &str) -> String {
    let mut padding = String::new();
    for ordinal in 0..96 {
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
fn commonmark_21_open_html_boundary_skips_only_the_non_resumable_sample() {
    let source = format!(
        "{}<a href=\"/bar\\/)\">\n\n\n{}",
        ordinary_padding("Prefix"),
        ordinary_padding("Suffix"),
    );
    assert!(source.len() > 8 * 1024, "fixture crosses sparse samples");

    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut session = build_session(&mut runtime);
    assert!(
        session.checkpoint_count() >= 2,
        "later resumable boundaries remain indexed after the HTML sample is skipped",
    );

    release_session(&mut runtime, &mut session);
    close_runtime(runtime);
}

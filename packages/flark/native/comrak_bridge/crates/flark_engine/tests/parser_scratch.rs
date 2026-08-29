use flark_engine::parser_internal::M11ParserScratchError;
use flark_engine::{DocumentRuntime, DocumentRuntimeConfig, DocumentState};

fn close_runtime(mut runtime: DocumentRuntime) {
    if runtime.state() == DocumentState::Open {
        runtime.begin_close().expect("begin close");
    }
    while runtime.state() != DocumentState::Closed {
        runtime.poll_close(64).expect("poll close");
    }
    let metrics = runtime.arena_metrics();
    assert_eq!(metrics.resident_nodes, 0);
    assert_eq!(metrics.reserved_external_payload_bytes, 0);
    assert_eq!(metrics.live_builds, 0);
}

#[test]
fn admission_is_exact_source_and_runtime_bound_and_wrong_release_is_recoverable() {
    let mut owner =
        DocumentRuntime::new("owner", DocumentRuntimeConfig::default()).expect("owner runtime");
    let mut foreign =
        DocumentRuntime::new("owner", DocumentRuntimeConfig::default()).expect("foreign runtime");
    let owner_source = owner.current_source_version().expect("owner source");
    let foreign_source = foreign.current_source_version().expect("foreign source");
    assert_ne!(owner_source, foreign_source);
    assert_eq!(
        owner
            .try_admit_parser_scratch(foreign_source, 4096)
            .expect_err("foreign source"),
        M11ParserScratchError::SourceAuthorityMismatch
    );

    let admission = owner
        .try_admit_parser_scratch(owner_source, 4096)
        .expect("admission");
    assert_eq!(admission.source(), owner_source);
    assert_eq!(admission.bytes(), 4096);
    assert_eq!(owner.arena_metrics().reserved_external_payload_bytes, 4096);

    let failure = foreign
        .release_parser_scratch(admission)
        .expect_err("wrong runtime");
    assert_eq!(failure.error(), M11ParserScratchError::WrongRuntime);
    assert_eq!(owner.arena_metrics().reserved_external_payload_bytes, 4096);
    assert_eq!(foreign.arena_metrics().reserved_external_payload_bytes, 0);
    owner
        .release_parser_scratch(failure.into_admission())
        .expect("owner releases recovered admission");
    close_runtime(owner);
    close_runtime(foreign);
}

#[test]
fn split_release_updates_exact_aggregate_bytes() {
    let mut runtime =
        DocumentRuntime::new("split", DocumentRuntimeConfig::default()).expect("runtime");
    let source = runtime.current_source_version().expect("source");
    let mut tail = runtime
        .try_admit_parser_scratch(source, 5656)
        .expect("bundle admission");
    assert_eq!(
        tail.split_prefix(0).expect_err("empty prefix"),
        M11ParserScratchError::InvalidSplit {
            prefix_bytes: 0,
            available_bytes: 5656,
        }
    );
    let prefix = tail.split_prefix(4096).expect("page prefix");
    assert_eq!(prefix.bytes(), 4096);
    assert_eq!(tail.bytes(), 1560);
    assert_eq!(
        runtime.arena_metrics().reserved_external_payload_bytes,
        5656
    );

    runtime
        .release_parser_scratch(prefix)
        .expect("release prefix");
    assert_eq!(
        runtime.arena_metrics().reserved_external_payload_bytes,
        1560
    );
    runtime.release_parser_scratch(tail).expect("release tail");
    assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
    close_runtime(runtime);
}

#[test]
fn hard_budget_rejection_is_typed_and_non_mutating() {
    let mut config = DocumentRuntimeConfig::default();
    config.arena_limits.max_live_payload_bytes = 4096;
    let mut runtime = DocumentRuntime::new("budget", config).expect("runtime");
    let source = runtime.current_source_version().expect("source");
    let admission = runtime
        .try_admit_parser_scratch(source, 4096)
        .expect("exact budget");
    let error = runtime
        .try_admit_parser_scratch(source, 1)
        .expect_err("over budget");
    assert!(error.is_resource_limit());
    assert_eq!(
        runtime.arena_metrics().reserved_external_payload_bytes,
        4096
    );
    runtime
        .release_parser_scratch(admission)
        .expect("release admission");
    close_runtime(runtime);
}

#[test]
fn closing_waits_for_scratch_and_release_does_not_require_current_source() {
    let mut runtime =
        DocumentRuntime::new("closing", DocumentRuntimeConfig::default()).expect("runtime");
    let source = runtime.current_source_version().expect("source");
    let admission = runtime
        .try_admit_parser_scratch(source, 1024)
        .expect("admission");
    runtime.begin_close().expect("begin close");

    let blocked = runtime.poll_close(64).expect("blocked close");
    assert!(!blocked.complete);
    assert_eq!(runtime.state(), DocumentState::Closing);
    assert_eq!(
        runtime.arena_metrics().reserved_external_payload_bytes,
        1024
    );

    runtime
        .release_parser_scratch(admission)
        .expect("release while closing");
    while runtime.state() != DocumentState::Closed {
        runtime.poll_close(64).expect("finish close");
    }
    assert_eq!(runtime.arena_metrics().reserved_external_payload_bytes, 0);
}

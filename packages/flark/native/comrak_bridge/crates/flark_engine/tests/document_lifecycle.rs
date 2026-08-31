use flark_engine::{
    DocumentRuntime, DocumentRuntimeConfig, DocumentRuntimeError, DocumentState, SourceEditError,
    SourceRevision, SourceStore, SourceUtf16Operation,
};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

fn close(mut runtime: DocumentRuntime) {
    if runtime.state() == DocumentState::Open {
        runtime.begin_close().expect("begin close");
    }
    while runtime.state() != DocumentState::Closed {
        runtime.poll_close(2).expect("close poll");
    }
}

fn read_runtime_source(runtime: &DocumentRuntime) -> Vec<u8> {
    let length = runtime
        .current_source_version()
        .expect("open runtime source")
        .byte_len();
    let mut bytes = vec![0_u8; length];
    let read = runtime
        .read_current_source_window(0..length, &mut bytes)
        .expect("bounded runtime source window");
    assert_eq!(read, length);
    bytes
}

#[test]
fn runtime_and_source_capabilities_are_send() {
    fn assert_send<T: Send>() {}

    assert_send::<DocumentRuntime>();
    assert_send::<flark_engine::SourceStore>();
    assert_send::<flark_engine::SourceSnapshotLease>();
    assert_send::<flark_engine::PreparedSourceEdit>();
    assert_send::<flark_engine::SourceCommit>();
    assert_send::<flark_engine::SourceCursor>();
    assert_send::<flark_engine::PhysicalLineCursor>();
    assert_send::<flark_engine::Utf16EditReceipt>();
}

#[test]
fn externally_seeded_runtime_preserves_revision_root_and_exact_snapshot() {
    let mut seed = SourceStore::seed(SourceRevision::new(41), 4);
    seed.append_page(0..4, "a😀b").expect("seed page");
    let store = seed.finalize().expect("seed source");
    let seeded_version = store.version();

    let runtime = DocumentRuntime::from_source_store(store, DocumentRuntimeConfig::default())
        .expect("seeded runtime");
    assert_eq!(runtime.current_source_version(), Some(seeded_version));
    assert_eq!(read_runtime_source(&runtime), "a😀b".as_bytes());
    close(runtime);
}
#[test]
fn serialized_runtime_migrates_across_host_threads_and_still_drains_to_zero() {
    let runtime = DocumentRuntime::new("alpha", DocumentRuntimeConfig::default()).expect("runtime");
    let origin_thread = thread::current().id();

    let (runtime, source, first_thread) = thread::spawn(move || {
        let runtime = runtime;
        let source = runtime.current_source_version().expect("source");
        (runtime, source, thread::current().id())
    })
    .join()
    .expect("first host thread");

    let (runtime, second_thread) = thread::spawn(move || {
        let mut runtime = runtime;
        runtime
            .apply_edit(source, 5..5, " beta")
            .expect("edit after migration");
        while !runtime.poll_retirement(1).complete {}
        (runtime, thread::current().id())
    })
    .join()
    .expect("second host thread");

    let (runtime, third_thread) = thread::spawn(move || {
        let mut runtime = runtime;
        runtime.begin_close().expect("begin close");
        while !runtime.poll_close(1).expect("close poll").complete {}
        assert_eq!(runtime.arena_metrics().resident_nodes, 0);
        assert_eq!(runtime.arena_metrics().live_builds, 0);
        (runtime, thread::current().id())
    })
    .join()
    .expect("third host thread");

    assert_ne!(origin_thread, first_thread);
    assert_ne!(first_thread, second_thread);
    assert_ne!(second_thread, third_thread);
    assert_eq!(runtime.state(), DocumentState::Closed);
    assert_eq!(runtime.retired_source_count(), 0);
    assert_eq!(runtime.retired_source_bytes(), 0);
}

#[test]
fn concurrent_callers_require_explicit_external_serialization() {
    let runtime =
        DocumentRuntime::new("serialized", DocumentRuntimeConfig::default()).expect("runtime");
    let shared = Arc::new(Mutex::new(runtime));
    let barrier = Arc::new(Barrier::new(3));
    let mut callers = Vec::new();

    for _ in 0..2 {
        let shared = Arc::clone(&shared);
        let barrier = Arc::clone(&barrier);
        callers.push(thread::spawn(move || {
            barrier.wait();
            let runtime = shared.lock().expect("serialized runtime lock");
            assert_eq!(runtime.state(), DocumentState::Open);
        }));
    }
    barrier.wait();
    for caller in callers {
        caller.join().expect("serialized caller");
    }

    let runtime = Arc::try_unwrap(shared)
        .expect("all shared owners joined")
        .into_inner()
        .expect("serialized runtime");
    close(runtime);
}

#[cfg(debug_assertions)]
#[test]
fn abandoning_an_open_runtime_is_loud_in_debug_builds() {
    let dropped = std::panic::catch_unwind(|| {
        let _runtime = DocumentRuntime::new("worker local", DocumentRuntimeConfig::default())
            .expect("runtime");
    });
    assert!(dropped.is_err());
}

#[test]
fn runtime_keeps_one_current_source_and_fuel_drains_superseded_roots() {
    let mut runtime =
        DocumentRuntime::new("alpha", DocumentRuntimeConfig::default()).expect("document runtime");
    let initial = runtime.current_source_version().expect("initial source");

    let first_edit = runtime
        .apply_edit(initial, 5..5, " beta")
        .expect("first edit");
    assert_eq!(
        runtime.current_source_version(),
        Some(first_edit.source().current())
    );
    assert_eq!(runtime.retired_source_count(), 1);

    let drained = runtime.poll_retirement(1);
    assert_eq!(drained.released_source_leases, 1);
    assert_eq!(drained.released_source_bytes, "alpha".len());
    assert_eq!(drained.arena_transitions, 0);
    assert!(drained.complete);

    let second_edit = runtime
        .apply_edit(first_edit.source().current(), 0..0, "!")
        .expect("second edit");
    let third_edit = runtime
        .apply_edit(second_edit.source().current(), 0..1, "?")
        .expect("third edit");
    assert_eq!(
        runtime.current_source_version(),
        Some(third_edit.source().current())
    );
    assert_eq!(runtime.retired_source_count(), 2);
    close(runtime);
}

#[test]
fn close_is_explicit_fuelled_and_waits_for_source_storage() {
    let mut runtime =
        DocumentRuntime::new("source", DocumentRuntimeConfig::default()).expect("document runtime");
    let source = runtime.current_source_version().expect("source");
    assert!(runtime.begin_close().expect("begin close"));
    assert!(!runtime.begin_close().expect("idempotent close"));
    assert_eq!(runtime.state(), DocumentState::Closing);
    assert!(matches!(
        runtime.apply_edit(source, 0..0, "x"),
        Err(DocumentRuntimeError::NotOpen {
            state: DocumentState::Closing
        })
    ));
    let first = runtime.poll_close(1).expect("first close poll");
    assert_eq!(first.released_source_leases, 1);
    assert!(first.complete);
    assert_eq!(runtime.state(), DocumentState::Closed);
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);

    let after = runtime.poll_close(1).expect("closed poll");
    assert_eq!(after.released_source_leases, 0);
    assert_eq!(after.arena_transitions, 0);
    assert!(after.complete);
}

#[test]
fn retirement_backpressure_rejects_without_partial_transition() {
    let mut runtime = DocumentRuntime::new(
        "text",
        DocumentRuntimeConfig {
            max_retired_sources: 1,
            ..DocumentRuntimeConfig::default()
        },
    )
    .expect("document runtime");
    let source = runtime.current_source_version().unwrap();
    let first = runtime.apply_edit(source, 0..0, "x").expect("first edit");
    let current = first.source().current();

    let edit_error = runtime
        .apply_edit(current, 0..0, "y")
        .expect_err("one pending retirement lease fills capacity");
    assert!(matches!(
        edit_error,
        DocumentRuntimeError::RetirementBackpressure {
            needed_leases: 1,
            available_leases: 0,
            ..
        }
    ));
    assert_eq!(runtime.current_source_version(), Some(current));
    assert_eq!(runtime.retired_source_count(), 1);

    let drain = runtime.poll_retirement(1);
    assert!(drain.complete);
    assert_eq!(runtime.retired_source_count(), 0);

    assert!(runtime.begin_close().expect("close after drain"));
    assert!(runtime.poll_close(1).expect("final close poll").complete);
    assert_eq!(runtime.state(), DocumentState::Closed);
}

#[test]
fn retired_source_byte_backpressure_rejects_without_partial_transition() {
    let mut runtime = DocumentRuntime::new(
        "text",
        DocumentRuntimeConfig {
            max_retired_source_bytes: 6,
            ..DocumentRuntimeConfig::default()
        },
    )
    .expect("document runtime");
    let initial = runtime.current_source_version().expect("initial source");
    let first = runtime
        .apply_edit(initial, 3..4, "!")
        .expect("first edit fills most of the byte budget");
    let current = first.source().current();

    let error = runtime
        .apply_edit(current, 3..4, "?")
        .expect_err("retired logical bytes fill the remaining budget");
    assert!(matches!(
        error,
        DocumentRuntimeError::RetirementBackpressure {
            needed_leases: 1,
            available_leases: 7,
            needed_bytes: 4,
            available_bytes: 2,
        }
    ));
    assert_eq!(runtime.current_source_version(), Some(current));
    assert_eq!(runtime.retired_source_count(), 1);
    assert_eq!(runtime.retired_source_bytes(), 4);
    assert_eq!(read_runtime_source(&runtime), b"tex!");

    assert!(runtime.poll_retirement(1).complete);
    close(runtime);
}

#[test]
fn edit_and_drain_receipts_report_conservative_source_bytes() {
    let mut runtime =
        DocumentRuntime::new("alpha", DocumentRuntimeConfig::default()).expect("runtime");
    let source = runtime.current_source_version().expect("source");
    let receipt = runtime
        .apply_edit(source, 5..5, "!")
        .expect("admitted edit");
    assert_eq!(receipt.retired_source_leases(), 1);
    assert_eq!(receipt.retired_source_bytes(), 5);
    assert_eq!(runtime.retired_source_bytes(), 5);

    let drain = runtime.poll_retirement(1);
    assert_eq!(drain.released_source_leases, 1);
    assert_eq!(drain.released_source_bytes, 5);
    assert_eq!(runtime.retired_source_bytes(), 0);
    close(runtime);
}

#[test]
fn utf16_intent_is_atomic_and_unicode_exact() {
    let mut seed = SourceStore::seed(SourceRevision::new(41), 4);
    seed.append_page(0..4, "A😀z").expect("seed page");
    let mut runtime = DocumentRuntime::from_source_store(
        seed.finalize().expect("seed source"),
        DocumentRuntimeConfig::default(),
    )
    .expect("runtime");
    let before = runtime.current_source_version().expect("source version");
    let operations = [
        SourceUtf16Operation::new(0..0, "<"),
        SourceUtf16Operation::new(1..1, "x"),
        SourceUtf16Operation::new(1..1, "y"),
        SourceUtf16Operation::new(1..3, "🙂"),
        SourceUtf16Operation::new(3..4, "Z"),
    ];

    let receipt = runtime
        .apply_utf16_edit_intent(before, SourceRevision::new(42), &operations)
        .expect("atomic intent");
    assert_eq!(receipt.source().previous(), before);
    assert_eq!(
        receipt.source().current().revision(),
        SourceRevision::new(42)
    );
    assert_ne!(receipt.source().current().root(), before.root());
    assert_eq!(receipt.source().operation_count(), 5);
    assert_eq!(receipt.source().replacement_byte_len(), 8);
    assert_eq!(receipt.source().replacement_utf16_len(), 6);
    assert_eq!(receipt.retired_source_leases(), 1);
    assert_eq!(receipt.retired_source_bytes(), before.byte_len());
    assert_eq!(
        runtime.current_source_version(),
        Some(receipt.source().current())
    );
    assert_eq!(runtime.retired_source_count(), 1);
    assert_eq!(read_runtime_source(&runtime), "<Axy🙂Z".as_bytes());

    runtime.begin_close().expect("begin close");
    assert!(matches!(
        runtime.read_current_source_window(0..0, &mut []),
        Err(DocumentRuntimeError::NotOpen {
            state: DocumentState::Closing
        })
    ));
    assert!(matches!(
        runtime.apply_utf16_edit_intent(
            receipt.source().current(),
            SourceRevision::new(43),
            &[SourceUtf16Operation::new(0..0, "!")],
        ),
        Err(DocumentRuntimeError::NotOpen {
            state: DocumentState::Closing
        })
    ));
    while runtime.state() != DocumentState::Closed {
        runtime.poll_close(1).expect("close poll");
    }
    assert!(matches!(
        runtime.read_current_source_window(0..0, &mut []),
        Err(DocumentRuntimeError::NotOpen {
            state: DocumentState::Closed
        })
    ));
}

#[test]
fn malformed_utf16_intents_leave_source_untouched() {
    let mut runtime =
        DocumentRuntime::new("a😀b", DocumentRuntimeConfig::default()).expect("runtime");
    let before = runtime.current_source_version().expect("source");
    let foreign = SourceStore::new("a😀b").expect("foreign source").version();
    let valid = [SourceUtf16Operation::new(0..0, "!")];

    assert!(matches!(
        runtime.apply_utf16_edit_intent(foreign, SourceRevision::new(1), &valid),
        Err(DocumentRuntimeError::Source(SourceEditError::StaleVersion {
            expected,
            actual,
        })) if expected == foreign && actual == before
    ));
    assert!(matches!(
        runtime.apply_utf16_edit_intent(before, SourceRevision::new(2), &valid),
        Err(DocumentRuntimeError::Source(
            SourceEditError::InvalidRevisionTransition { .. }
        ))
    ));
    let split_surrogate = [SourceUtf16Operation::new(2..2, "split")];
    assert!(matches!(
        runtime.apply_utf16_edit_intent(before, SourceRevision::new(1), &split_surrogate),
        Err(DocumentRuntimeError::Source(
            SourceEditError::SplitUtf16Scalar { offset: 2 }
        ))
    ));
    let overlap = [
        SourceUtf16Operation::new(0..3, "first"),
        SourceUtf16Operation::new(1..3, "second"),
    ];
    assert!(matches!(
        runtime.apply_utf16_edit_intent(before, SourceRevision::new(1), &overlap),
        Err(DocumentRuntimeError::Source(
            SourceEditError::InvalidOperationOrder { .. }
        ))
    ));

    assert_eq!(runtime.current_source_version(), Some(before));
    assert_eq!(runtime.retired_source_count(), 0);
    assert_eq!(read_runtime_source(&runtime), "a😀b".as_bytes());
    close(runtime);
}

#[test]
fn utf16_target_size_budget_rejects_before_authority_changes() {
    let mut runtime = DocumentRuntime::new(
        "abc",
        DocumentRuntimeConfig {
            max_retired_source_bytes: 5,
            ..DocumentRuntimeConfig::default()
        },
    )
    .expect("runtime");
    let before = runtime.current_source_version().expect("source");

    let error = runtime
        .apply_utf16_edit_intent(
            before,
            SourceRevision::new(1),
            &[SourceUtf16Operation::new(3..3, "xyz")],
        )
        .expect_err("target exceeds source budget");
    assert!(matches!(
        error,
        DocumentRuntimeError::SourceExceedsRetirementBudget {
            source_bytes: 6,
            limit: 5,
        }
    ));
    assert_eq!(runtime.current_source_version(), Some(before));
    assert_eq!(runtime.retired_source_count(), 0);
    assert_eq!(read_runtime_source(&runtime), b"abc");
    close(runtime);
}

#[test]
fn utf16_retirement_backpressure_rejects_before_authority_changes() {
    let mut runtime = DocumentRuntime::new(
        "text",
        DocumentRuntimeConfig {
            max_retired_sources: 1,
            ..DocumentRuntimeConfig::default()
        },
    )
    .expect("runtime");
    let before = runtime.current_source_version().expect("source");
    let first = runtime
        .apply_edit(before, 4..4, "!")
        .expect("fill retirement capacity");
    let current = first.source().current();

    let error = runtime
        .apply_utf16_edit_intent(
            current,
            SourceRevision::new(2),
            &[SourceUtf16Operation::new(5..5, "?")],
        )
        .expect_err("pending source fills retirement capacity");
    assert!(matches!(
        error,
        DocumentRuntimeError::RetirementBackpressure {
            needed_leases: 1,
            available_leases: 0,
            needed_bytes: 5,
            ..
        }
    ));
    assert_eq!(runtime.current_source_version(), Some(current));
    assert_eq!(runtime.retired_source_count(), 1);
    assert_eq!(runtime.retired_source_bytes(), before.byte_len());
    assert_eq!(read_runtime_source(&runtime), b"text!");

    while !runtime.poll_retirement(1).complete {}
    close(runtime);
}

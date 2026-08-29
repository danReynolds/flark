use flark_engine::{
    DocumentRuntime, DocumentRuntimeConfig, DocumentRuntimeError, DocumentState,
    IncrementalSourceFactsPlan, ParserProfileId, PersistentSourceFactsDeltaWitness,
    RuntimeSourceFactsPoll, SourceEditError, SourceFactsCoverage, SourceFactsRootLimits,
    SourceFactsScanProfile, SourceRevision, SourceStore, SourceUtf16Operation,
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

fn complete_clean_source_facts(
    runtime: &mut DocumentRuntime,
    profile: SourceFactsScanProfile,
    parser_profile: ParserProfileId,
    limits: SourceFactsRootLimits,
) {
    runtime
        .begin_source_facts(profile, parser_profile, limits)
        .expect("begin clean SourceFacts");
    loop {
        match runtime
            .poll_source_facts(128, 64)
            .expect("bounded clean SourceFacts poll")
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. }
            | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
            RuntimeSourceFactsPoll::Complete { .. } => break,
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
            | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                panic!("clean SourceFacts job reported incremental progress")
            }
        }
    }
}

fn complete_incremental_source_facts(
    runtime: &mut DocumentRuntime,
) -> Box<PersistentSourceFactsDeltaWitness> {
    loop {
        match runtime
            .poll_source_facts(17, 3)
            .expect("bounded incremental SourceFacts poll")
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. }
            | RuntimeSourceFactsPoll::IncrementalScanComplete { .. } => {}
            RuntimeSourceFactsPoll::IncrementalComplete { witness, .. } => return witness,
            RuntimeSourceFactsPoll::ScanComplete { .. }
            | RuntimeSourceFactsPoll::Complete { .. } => {
                panic!("incremental SourceFacts job reported clean progress")
            }
        }
    }
}

fn mint_middle_source_facts_delta(
    unit_count: usize,
) -> (
    DocumentRuntime,
    IncrementalSourceFactsPlan,
    Box<PersistentSourceFactsDeltaWitness>,
    u64,
) {
    let unit = "alpha **bold** 😀\r\nbeta\n";
    let source = unit.repeat(unit_count);
    let profile = SourceFactsScanProfile::new(4).expect("source-fact profile");
    let parser_profile = ParserProfileId::new(73).expect("parser profile");
    let limits = SourceFactsRootLimits::default();
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    complete_clean_source_facts(&mut runtime, profile, parser_profile, limits);
    let base = runtime
        .persistent_source_facts()
        .expect("persistent clean base");
    let base_page_count = base.page_count();
    drop(
        runtime
            .take_certified_source()
            .expect("release clean certification projection"),
    );

    let edit_start = source
        .match_indices("**bold**")
        .nth(unit_count / 2)
        .expect("middle markdown occurrence")
        .0;
    let edit_end = edit_start + "**bold**".len();
    let target = runtime
        .apply_edit(base.source(), edit_start..edit_end, "**stronger**")
        .expect("middle edit")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, limits)
        .expect("incremental SourceFacts plan");
    assert_eq!(plan.source(), target);
    let witness = complete_incremental_source_facts(&mut runtime);
    (runtime, plan, witness, base_page_count)
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
    assert_send::<PersistentSourceFactsDeltaWitness>();
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
    let initial_plan = runtime.latest_plan().expect("initial plan");
    assert_eq!(initial_plan.generation().get(), 1);
    assert_eq!(initial_plan.source(), seeded_version);

    assert_eq!(read_runtime_source(&runtime), "a😀b".as_bytes());
    close(runtime);
}

#[test]
fn serialized_runtime_migrates_across_host_threads_and_still_drains_to_zero() {
    let runtime = DocumentRuntime::new("alpha", DocumentRuntimeConfig::default()).expect("runtime");
    let origin_thread = thread::current().id();

    let (runtime, source, first_thread) = thread::spawn(move || {
        let mut runtime = runtime;
        let source = runtime.current_source_version().expect("source");
        runtime.begin_candidate().expect("candidate");
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
fn runtime_keeps_one_current_source_one_candidate_and_one_latest_plan() {
    let mut runtime =
        DocumentRuntime::new("alpha", DocumentRuntimeConfig::default()).expect("document runtime");
    let initial = runtime.latest_plan().expect("initial plan");
    assert_eq!(initial.generation().get(), 1);
    assert_eq!(initial.source(), runtime.current_source_version().unwrap());

    let active = runtime.begin_candidate().expect("begin candidate");
    assert_eq!(active.generation(), initial.generation());
    assert!(runtime.latest_plan().is_none());
    assert_eq!(runtime.arena_metrics().resident_nodes, 1);

    let first_edit = runtime
        .apply_edit(initial.source(), 5..5, " beta")
        .expect("first edit");
    assert!(first_edit.superseded_active_candidate());
    assert!(runtime.active_candidate().is_none());
    assert_eq!(runtime.latest_plan(), Some(first_edit.latest_plan()));
    assert_eq!(runtime.retired_source_count(), 2);

    let drained = runtime.poll_retirement(4);
    assert_eq!(drained.released_source_leases, 2);
    assert_eq!(drained.released_source_bytes, "alpha".len() * 2);
    assert_eq!(drained.arena_transitions, 2);
    assert_eq!(drained.arena_nodes_reclaimed, 1);
    assert!(drained.complete);
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);

    let second_edit = runtime
        .apply_edit(first_edit.source().current(), 0..0, "!")
        .expect("second edit");
    let third_edit = runtime
        .apply_edit(second_edit.source().current(), 0..1, "?")
        .expect("third edit");
    assert_eq!(runtime.latest_plan(), Some(third_edit.latest_plan()));
    assert_eq!(third_edit.latest_plan().generation().get(), 4);
    assert_eq!(runtime.retired_source_count(), 2);
    close(runtime);
}

#[test]
fn close_is_explicit_fuelled_and_waits_for_source_and_arena() {
    let mut runtime =
        DocumentRuntime::new("source", DocumentRuntimeConfig::default()).expect("document runtime");
    let source = runtime.begin_candidate().expect("candidate").source();
    assert!(runtime.begin_close().expect("begin close"));
    assert!(!runtime.begin_close().expect("idempotent close"));
    assert_eq!(runtime.state(), DocumentState::Closing);
    assert!(matches!(
        runtime.apply_edit(source, 0..0, "x"),
        Err(DocumentRuntimeError::NotOpen {
            state: DocumentState::Closing
        })
    ));
    assert!(matches!(
        runtime.begin_candidate(),
        Err(DocumentRuntimeError::NotOpen {
            state: DocumentState::Closing
        })
    ));

    let first = runtime.poll_close(1).expect("first close poll");
    assert_eq!(first.released_source_leases, 1);
    assert!(!first.complete);
    let second = runtime.poll_close(1).expect("second close poll");
    assert_eq!(second.arena_transitions, 1);
    assert!(!second.complete);
    let third = runtime.poll_close(1).expect("third close poll");
    assert_eq!(third.released_source_leases, 1);
    assert!(!third.complete);
    let fourth = runtime.poll_close(1).expect("fourth close poll");
    assert_eq!(fourth.arena_transitions, 1);
    assert_eq!(fourth.arena_nodes_reclaimed, 1);
    assert!(fourth.complete);
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
    let active = runtime.begin_candidate().expect("candidate");

    let edit_error = runtime
        .apply_edit(source, 0..0, "x")
        .expect_err("two retirement leases exceed capacity");
    assert!(matches!(
        edit_error,
        DocumentRuntimeError::RetirementBackpressure {
            needed_leases: 2,
            available_leases: 1,
            needed_bytes: 8,
            ..
        }
    ));
    assert_eq!(runtime.current_source_version(), Some(source));
    assert_eq!(runtime.active_candidate(), Some(active));
    assert!(runtime.latest_plan().is_none());
    assert_eq!(runtime.retired_source_count(), 0);
    assert_eq!(runtime.arena_metrics().resident_nodes, 1);

    runtime
        .complete_candidate(active.generation())
        .expect("retire candidate");
    let drain = runtime.poll_retirement(3);
    assert!(drain.complete);
    assert_eq!(runtime.retired_source_count(), 0);
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);

    assert!(runtime.begin_close().expect("close after drain"));
    assert!(runtime.poll_close(1).expect("final close poll").complete);
    assert_eq!(runtime.state(), DocumentState::Closed);
}

#[test]
fn stale_candidate_completion_preserves_the_new_attempt() {
    let mut runtime =
        DocumentRuntime::new("a", DocumentRuntimeConfig::default()).expect("document runtime");
    let first = runtime.begin_candidate().expect("first candidate");
    runtime
        .apply_edit(first.source(), 1..1, "b")
        .expect("edit supersedes first candidate");
    runtime.poll_retirement(3);
    let second = runtime.begin_candidate().expect("second candidate");

    let error = runtime
        .complete_candidate(first.generation())
        .expect_err("old generation must not complete new candidate");
    assert!(matches!(
        error,
        DocumentRuntimeError::StaleCandidate { expected, actual }
            if expected == first.generation() && actual == second.generation()
    ));
    assert_eq!(runtime.active_candidate(), Some(second));
    assert_eq!(runtime.retired_source_count(), 0);
    close(runtime);
}

#[test]
fn logical_source_byte_cap_rejects_before_any_authority_changes() {
    let mut runtime = DocumentRuntime::new(
        "text",
        DocumentRuntimeConfig {
            max_retired_sources: 8,
            max_retired_source_bytes: 6,
            ..DocumentRuntimeConfig::default()
        },
    )
    .expect("document runtime");
    let source = runtime.current_source_version().expect("source");
    let active = runtime.begin_candidate().expect("candidate");

    let error = runtime
        .apply_edit(source, 3..4, "t")
        .expect_err("two logical source leases exceed byte cap");
    assert!(matches!(
        error,
        DocumentRuntimeError::RetirementBackpressure {
            needed_leases: 2,
            available_leases: 8,
            needed_bytes: 8,
            available_bytes: 6,
        }
    ));
    assert_eq!(runtime.current_source_version(), Some(source));
    assert_eq!(runtime.active_candidate(), Some(active));
    assert_eq!(runtime.retired_source_count(), 0);
    assert_eq!(runtime.retired_source_bytes(), 0);

    runtime
        .complete_candidate(active.generation())
        .expect("retire candidate");
    runtime.poll_retirement(3);
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
fn sustained_fuel_two_edits_cannot_starve_arena_retirement() {
    let mut runtime = DocumentRuntime::new("a", DocumentRuntimeConfig::default()).expect("runtime");
    runtime.begin_candidate().expect("initial candidate");

    for iteration in 0..64 {
        let source = runtime.current_source_version().expect("source");
        runtime
            .apply_edit(source, 0..0, "x")
            .expect("admitted edit");
        let receipt = runtime.poll_retirement(2);
        assert!(receipt.released_source_leases >= 1);
        if iteration < 2 {
            assert_eq!(receipt.arena_transitions, 1);
        }
        if iteration == 1 {
            assert_eq!(runtime.arena_metrics().resident_nodes, 0);
        }
    }

    while !runtime.poll_retirement(2).complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    close(runtime);
}

#[test]
fn utf16_intent_is_atomic_unicode_exact_and_supersedes_the_active_candidate() {
    let mut seed = SourceStore::seed(SourceRevision::new(41), 4);
    seed.append_page(0..4, "A😀z").expect("seed page");
    let mut runtime = DocumentRuntime::from_source_store(
        seed.finalize().expect("seed source"),
        DocumentRuntimeConfig::default(),
    )
    .expect("runtime");
    let before = runtime.current_source_version().expect("source version");
    let active = runtime.begin_candidate().expect("active candidate");
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
    assert!(receipt.superseded_active_candidate());
    assert_eq!(receipt.retired_source_leases(), 2);
    assert_eq!(receipt.retired_source_bytes(), before.byte_len() * 2);
    assert_eq!(receipt.latest_plan().generation().get(), 2);
    assert_eq!(receipt.latest_plan().source(), receipt.source().current());
    assert_eq!(runtime.latest_plan(), Some(receipt.latest_plan()));
    assert_eq!(runtime.active_candidate(), None);
    assert_eq!(runtime.retired_source_count(), 2);
    assert_eq!(active.source(), before);
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
fn malformed_utf16_intents_leave_source_and_plan_untouched() {
    let mut runtime =
        DocumentRuntime::new("a😀b", DocumentRuntimeConfig::default()).expect("runtime");
    let before = runtime.current_source_version().expect("source");
    let initial_plan = runtime.latest_plan();
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
    assert_eq!(runtime.latest_plan(), initial_plan);
    assert_eq!(runtime.active_candidate(), None);
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
    let initial_plan = runtime.latest_plan();

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
    assert_eq!(runtime.latest_plan(), initial_plan);
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
    let active = runtime.begin_candidate().expect("candidate");

    let error = runtime
        .apply_utf16_edit_intent(
            before,
            SourceRevision::new(1),
            &[SourceUtf16Operation::new(4..4, "!")],
        )
        .expect_err("candidate and source exceed retirement capacity");
    assert!(matches!(
        error,
        DocumentRuntimeError::RetirementBackpressure {
            needed_leases: 2,
            available_leases: 1,
            needed_bytes: 8,
            ..
        }
    ));
    assert_eq!(runtime.current_source_version(), Some(before));
    assert_eq!(runtime.active_candidate(), Some(active));
    assert_eq!(runtime.latest_plan(), None);
    assert_eq!(runtime.retired_source_count(), 0);
    assert_eq!(runtime.retired_source_bytes(), 0);
    assert_eq!(read_runtime_source(&runtime), b"text");

    runtime
        .complete_candidate(active.generation())
        .expect("retire candidate");
    while !runtime.poll_retirement(1).complete {}
    close(runtime);
}

#[test]
fn source_facts_stay_runtime_owned_across_rejection_edit_and_close() {
    let mut runtime = DocumentRuntime::new(
        "abc",
        DocumentRuntimeConfig {
            max_retired_source_bytes: 5,
            ..DocumentRuntimeConfig::default()
        },
    )
    .expect("runtime");
    let profile = SourceFactsScanProfile::new(2).expect("source-fact profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let before = runtime.current_source_version().expect("source");
    assert_eq!(
        runtime
            .begin_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
            .expect("begin facts"),
        before
    );
    assert!(matches!(
        runtime.begin_source_facts(profile, parser_profile, SourceFactsRootLimits::default()),
        Err(DocumentRuntimeError::SourceFactsAlreadyActive)
    ));
    assert!(matches!(
        runtime.poll_source_facts(1, 1).expect("first fact poll"),
        RuntimeSourceFactsPoll::Pending(_)
    ));

    let rejected = runtime.apply_utf16_edit_intent(
        before,
        SourceRevision::new(1),
        &[SourceUtf16Operation::new(3..3, "xyz")],
    );
    assert!(matches!(
        rejected,
        Err(DocumentRuntimeError::SourceExceedsRetirementBudget {
            source_bytes: 6,
            limit: 5,
        })
    ));
    assert!(runtime.poll_source_facts(1, 1).is_ok());

    let edit = runtime
        .apply_utf16_edit_intent(
            before,
            SourceRevision::new(1),
            &[SourceUtf16Operation::new(3..3, "!")],
        )
        .expect("admitted edit");
    assert!(matches!(
        runtime.poll_source_facts(1, 1),
        Err(DocumentRuntimeError::NoSourceFactsJob)
    ));

    runtime
        .begin_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("restart facts");
    while let RuntimeSourceFactsPoll::Pending(_)
    | RuntimeSourceFactsPoll::PromotionPending { .. }
    | RuntimeSourceFactsPoll::ScanComplete { .. } = runtime
        .poll_source_facts(64, 64)
        .expect("bounded fact poll")
    {}
    assert_eq!(
        runtime.certified_source().map(|source| source.source()),
        Some(edit.source().current())
    );

    runtime.begin_close().expect("begin close");
    assert!(runtime.certified_source().is_none());
    while runtime.state() != DocumentState::Closed {
        runtime.poll_close(1).expect("close poll");
    }
}

#[test]
fn clean_source_facts_promote_to_a_persistent_actor_owned_base() {
    let source = "a".repeat(400);
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let profile = SourceFactsScanProfile::new(2).expect("source-fact profile");
    let parser_profile = ParserProfileId::new(9).expect("parser profile");
    let before = runtime.current_source_version().expect("source");
    runtime
        .begin_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("begin facts");

    let mut promotion_only_polls = 0;
    loop {
        match runtime
            .poll_source_facts(64, 64)
            .expect("bounded source-fact poll")
        {
            RuntimeSourceFactsPoll::Pending(_) => {}
            RuntimeSourceFactsPoll::PromotionPending { transitions } => {
                assert_eq!(transitions, 1);
                promotion_only_polls += 1;
            }
            RuntimeSourceFactsPoll::ScanComplete { completion, .. } => {
                assert_eq!(completion.source(), before);
            }
            RuntimeSourceFactsPoll::Complete { completion, .. } => {
                assert_eq!(completion.source(), before);
                break;
            }
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
            | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                panic!("clean SourceFacts job reported incremental progress")
            }
        }
    }

    let info = runtime
        .persistent_source_facts()
        .expect("persistent SourceFacts base");
    assert_eq!(info.source(), before);
    assert_eq!(info.parser_profile(), parser_profile);
    assert_eq!(info.profile(), profile);
    assert!(info.page_count() >= 3);
    assert_eq!(
        info.work().leaves_adopted(),
        usize::try_from(info.page_count()).expect("page count")
    );
    assert!(info.work().branches_allocated() >= info.work().leaves_adopted() - 1);
    assert!(info.work().node_headers_decoded() > 0);
    assert!(info.work().payload_bytes_inspected() > 0);
    assert!(info.work().checkpoints_hashed() >= info.checkpoint_count());
    assert!(promotion_only_polls > info.work().branches_allocated());

    let certified = runtime
        .take_certified_source()
        .expect("legacy publication projection");
    assert_eq!(certified.source(), before);
    assert_eq!(
        runtime
            .persistent_source_facts()
            .expect("base survives projection transfer")
            .source(),
        before
    );
    drop(certified);

    let edit = runtime
        .apply_edit(before, 200..200, "!")
        .expect("admitted edit");
    assert_ne!(edit.source().current(), before);
    assert_eq!(
        runtime
            .persistent_source_facts()
            .expect("prior base remains available for incremental adoption")
            .source(),
        before
    );
    close(runtime);
}

#[test]
fn persistent_source_certification_requires_a_clean_current_promoted_root() {
    let source = "persistent certification ".repeat(32);
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let profile = SourceFactsScanProfile::new(3).expect("source-fact profile");
    let parser_profile = ParserProfileId::new(17).expect("parser profile");
    let current = runtime.current_source_version().expect("current source");

    assert!(matches!(
        runtime.certify_current_persistent_source(),
        Err(DocumentRuntimeError::NoPersistentSourceFactsBase)
    ));

    runtime
        .begin_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("begin SourceFacts");
    let mut observed_scan_complete = false;
    loop {
        match runtime
            .poll_source_facts(8, 2)
            .expect("bounded SourceFacts poll")
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. } => {}
            RuntimeSourceFactsPoll::ScanComplete { completion, .. } => {
                observed_scan_complete = true;
                assert_eq!(completion.source(), current);
                assert!(matches!(
                    runtime.certify_current_persistent_source(),
                    Err(DocumentRuntimeError::NoPersistentSourceFactsBase)
                ));
            }
            RuntimeSourceFactsPoll::Complete { completion, .. } => {
                assert_eq!(completion.source(), current);
                break;
            }
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
            | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                panic!("clean SourceFacts job reported incremental progress")
            }
        }
    }
    assert!(observed_scan_complete);

    let certified = runtime
        .certify_current_persistent_source()
        .expect("clean current persistent certification");
    assert_eq!(certified.source(), current);
    assert_eq!(certified.parser_profile(), parser_profile);
    assert_eq!(certified.source_facts_profile(), profile);
    assert_eq!(certified.exact_parse_lease().version(), current);
    drop(certified);

    let edited = runtime
        .apply_edit(current, current.byte_len()..current.byte_len(), "!")
        .expect("edit current source")
        .source()
        .current();
    assert_ne!(edited, current);
    assert!(matches!(
        runtime.certify_current_persistent_source(),
        Err(DocumentRuntimeError::PersistentSourceFactsDeltaAuthorityMismatch)
    ));
    close(runtime);
}

#[test]
fn cancelling_persistent_source_facts_promotion_reclaims_the_build() {
    let source = "ab".repeat(300);
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let profile = SourceFactsScanProfile::new(2).expect("source-fact profile");
    let parser_profile = ParserProfileId::new(11).expect("parser profile");
    let before = runtime.current_source_version().expect("source");
    runtime
        .begin_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("begin facts");

    loop {
        match runtime
            .poll_source_facts(64, 64)
            .expect("bounded source-fact poll")
        {
            RuntimeSourceFactsPoll::PromotionPending { transitions } => {
                assert_eq!(transitions, 1);
                break;
            }
            RuntimeSourceFactsPoll::Pending(_) => {}
            RuntimeSourceFactsPoll::ScanComplete { .. } => {}
            RuntimeSourceFactsPoll::Complete { .. } => {
                panic!("multi-page promotion completed before cancellation point")
            }
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
            | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                panic!("clean SourceFacts job reported incremental progress")
            }
        }
    }
    assert!(runtime.persistent_source_facts().is_none());

    runtime
        .apply_edit(before, 1..1, "!")
        .expect("edit cancels persistent promotion");
    while !runtime.poll_retirement(1).complete {}
    close(runtime);
}

#[test]
fn incremental_source_facts_crop_and_splice_match_a_clean_oracle() {
    let unit = "# heading\r\nalpha **bold** 😀 beta\r\n- item\n\n";
    let source = unit.repeat(160);
    let profile = SourceFactsScanProfile::new(4).expect("source-fact profile");
    let parser_profile = ParserProfileId::new(17).expect("parser profile");
    let limits = SourceFactsRootLimits::default();
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    runtime
        .begin_source_facts(profile, parser_profile, limits)
        .expect("begin clean facts");
    loop {
        match runtime
            .poll_source_facts(128, 64)
            .expect("bounded clean SourceFacts poll")
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. }
            | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
            RuntimeSourceFactsPoll::Complete { .. } => break,
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
            | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                panic!("clean SourceFacts job reported incremental progress")
            }
        }
    }
    let base = runtime
        .persistent_source_facts()
        .expect("persistent clean base");
    let base_pages = base.page_count();
    assert!(base_pages > 8);
    drop(
        runtime
            .take_certified_source()
            .expect("release legacy clean projection"),
    );

    let edit_start = source
        .match_indices("**bold**")
        .nth(80)
        .expect("middle markdown occurrence")
        .0;
    let edit_end = edit_start + "**bold**".len();
    let replacement = "**stronger 😀 markdown**\r\n> inserted";
    let mut expected = source.clone();
    expected.replace_range(edit_start..edit_end, replacement);
    let target = runtime
        .apply_edit(base.source(), edit_start..edit_end, replacement)
        .expect("variable-length edit")
        .source()
        .current();

    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, limits)
        .expect("plan exact incremental crop");
    assert_eq!(plan.base(), base.source());
    assert_eq!(plan.source(), target);
    assert!(plan.base_page_range().end - plan.base_page_range().start <= 2);
    assert!(plan.target_byte_range().end - plan.target_byte_range().start < target.byte_len() / 4);

    let mut scanned_bytes = 0_usize;
    let (incremental_work, witness) = loop {
        match runtime
            .poll_source_facts(17, 3)
            .expect("bounded incremental SourceFacts poll")
        {
            RuntimeSourceFactsPoll::Pending(work) => {
                scanned_bytes = scanned_bytes
                    .checked_add(work.source_bytes_examined())
                    .expect("scan accounting");
            }
            RuntimeSourceFactsPoll::PromotionPending { transitions } => {
                assert_eq!(transitions, 1);
            }
            RuntimeSourceFactsPoll::IncrementalScanComplete {
                source,
                byte_start,
                byte_end,
                work,
            } => {
                assert_eq!(source, target);
                assert_eq!(byte_start..byte_end, plan.target_byte_range().clone());
                scanned_bytes = scanned_bytes
                    .checked_add(work.source_bytes_examined())
                    .expect("scan accounting");
            }
            RuntimeSourceFactsPoll::IncrementalComplete {
                source,
                work,
                witness,
            } => {
                assert_eq!(source, target);
                break (work, witness);
            }
            RuntimeSourceFactsPoll::ScanComplete { .. }
            | RuntimeSourceFactsPoll::Complete { .. } => {
                panic!("incremental SourceFacts job reported clean progress")
            }
        }
    };
    let witness = runtime
        .take_persistent_source_facts_delta(witness)
        .expect("consume exact delta witness");
    assert_eq!(
        scanned_bytes,
        plan.target_byte_range().end - plan.target_byte_range().start
    );
    let deleted_pages = usize::try_from(plan.base_page_range().end - plan.base_page_range().start)
        .expect("deleted page count");
    assert_eq!(incremental_work.leaves_deleted(), deleted_pages);
    assert_eq!(
        incremental_work.leaves_reused(),
        usize::try_from(base_pages).expect("base page count") - deleted_pages
    );
    let replacement_pages =
        usize::try_from(witness.target_page_range().end - witness.target_page_range().start)
            .expect("replacement page count");
    assert_eq!(incremental_work.leaves_adopted(), replacement_pages);
    assert_eq!(
        incremental_work.committed_leaves_retained(),
        usize::try_from(base_pages).expect("base page count") + replacement_pages
    );
    assert!(incremental_work.maximum_atomic_height() > 0);

    let incremental = runtime
        .persistent_source_facts()
        .expect("incrementally updated persistent facts");
    assert_eq!(incremental.source(), target);
    assert_eq!(incremental.coverage(), SourceFactsCoverage::CleanEof);

    let mut oracle =
        DocumentRuntime::new(&expected, DocumentRuntimeConfig::default()).expect("clean oracle");
    oracle
        .begin_source_facts(profile, parser_profile, limits)
        .expect("begin oracle facts");
    loop {
        match oracle
            .poll_source_facts(128, 64)
            .expect("bounded oracle SourceFacts poll")
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. }
            | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
            RuntimeSourceFactsPoll::Complete { .. } => break,
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
            | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                panic!("clean oracle reported incremental progress")
            }
        }
    }
    let clean = oracle
        .persistent_source_facts()
        .expect("clean oracle persistent facts");
    assert_eq!(incremental.summary(), clean.summary());
    assert_eq!(
        incremental.summary().byte_len(),
        u64::try_from(expected.len()).expect("expected byte length")
    );

    close(runtime);
    close(oracle);
}

#[test]
fn production_spacing_keeps_exact_parser_envelope_narrow_inside_one_wide_page() {
    let source = "alpha beta\n\n".repeat(4_096);
    let profile = SourceFactsScanProfile::new(4_096).expect("production source-fact profile");
    let parser_profile = ParserProfileId::new(101).expect("parser profile");
    let limits = SourceFactsRootLimits::default();
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    complete_clean_source_facts(&mut runtime, profile, parser_profile, limits);
    let base = runtime
        .persistent_source_facts()
        .expect("persistent clean base");
    assert_eq!(base.page_count(), 1);
    drop(
        runtime
            .take_certified_source()
            .expect("release clean certification projection"),
    );

    let edit_start = source
        .match_indices("alpha")
        .nth(2_048)
        .expect("middle paragraph")
        .0;
    let edit_end = edit_start + "alpha".len();
    let target = runtime
        .apply_edit(base.source(), edit_start..edit_end, "omega")
        .expect("middle same-length edit")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, limits)
        .expect("page-wide SourceFacts plan");

    assert_eq!(plan.base_byte_range(), &(0..source.len()));
    assert_eq!(plan.target_byte_range(), &(0..target.byte_len()));
    assert_eq!(
        plan.exact_parser_base_byte_range(),
        Some(&(edit_start..edit_end))
    );
    assert_eq!(
        plan.exact_parser_target_byte_range(),
        Some(&(edit_start..edit_end))
    );
    assert!(
        plan.exact_parser_target_byte_range()
            .expect("exact parser target")
            .len()
            < plan.target_byte_range().len()
    );

    let witness = complete_incremental_source_facts(&mut runtime);
    assert_eq!(
        witness.exact_parser_base_byte_range(),
        plan.exact_parser_base_byte_range()
    );
    assert_eq!(
        witness.exact_parser_target_byte_range(),
        plan.exact_parser_target_byte_range()
    );
    drop(
        runtime
            .take_persistent_source_facts_delta(witness)
            .expect("revalidate exact parser envelope"),
    );
    close(runtime);
}

#[test]
fn adjacent_rapid_insertions_compose_one_exact_parser_envelope() {
    let source = "0123456789\n";
    let profile = SourceFactsScanProfile::new(4_096).expect("production source-fact profile");
    let parser_profile = ParserProfileId::new(103).expect("parser profile");
    let limits = SourceFactsRootLimits::default();
    let mut runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
    complete_clean_source_facts(&mut runtime, profile, parser_profile, limits);
    let base = runtime
        .persistent_source_facts()
        .expect("persistent clean base")
        .source();
    drop(
        runtime
            .take_certified_source()
            .expect("release clean certification projection"),
    );

    let first = runtime
        .apply_edit(base, 4..4, "X")
        .expect("first insertion")
        .source()
        .current();
    let target = runtime
        .apply_edit(first, 5..5, "Y")
        .expect("adjacent insertion touching envelope end")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, limits)
        .expect("adjacent rapid edits remain exact");

    assert_eq!(plan.source(), target);
    assert_eq!(plan.lineage_transitions(), 2);
    assert_eq!(plan.exact_parser_base_byte_range(), Some(&(4..4)));
    assert_eq!(plan.exact_parser_target_byte_range(), Some(&(4..6)));

    let witness = complete_incremental_source_facts(&mut runtime);
    assert_eq!(witness.exact_parser_base_byte_range(), Some(&(4..4)));
    assert_eq!(witness.exact_parser_target_byte_range(), Some(&(4..6)));
    drop(
        runtime
            .take_persistent_source_facts_delta(witness)
            .expect("revalidate composed insertion envelope"),
    );
    close(runtime);
}

#[test]
fn distant_later_edit_discards_only_the_exact_parser_envelope() {
    let source = "x".repeat(512);
    let profile = SourceFactsScanProfile::new(4_096).expect("production source-fact profile");
    let parser_profile = ParserProfileId::new(107).expect("parser profile");
    let limits = SourceFactsRootLimits::default();
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    complete_clean_source_facts(&mut runtime, profile, parser_profile, limits);
    let base = runtime
        .persistent_source_facts()
        .expect("persistent clean base")
        .source();
    drop(
        runtime
            .take_certified_source()
            .expect("release clean certification projection"),
    );

    let first = runtime
        .apply_edit(base, 16..16, "left")
        .expect("first local insertion")
        .source()
        .current();
    let target = runtime
        .apply_edit(first, 400..400, "right")
        .expect("distant later insertion")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, limits)
        .expect("wide SourceFacts page still admits incremental work");

    assert_eq!(plan.source(), target);
    assert_eq!(plan.lineage_transitions(), 2);
    assert_eq!(plan.base_byte_range(), &(0..source.len()));
    assert!(plan.exact_parser_base_byte_range().is_none());
    assert!(plan.exact_parser_target_byte_range().is_none());

    let witness = complete_incremental_source_facts(&mut runtime);
    assert!(witness.exact_parser_base_byte_range().is_none());
    assert!(witness.exact_parser_target_byte_range().is_none());
    drop(
        runtime
            .take_persistent_source_facts_delta(witness)
            .expect("wider SourceFacts authority remains valid"),
    );
    close(runtime);
}

#[test]
fn exact_parser_envelope_survives_cancellation_and_one_use_revalidation() {
    let source = "alpha beta gamma\n".repeat(32);
    let profile = SourceFactsScanProfile::new(4_096).expect("production source-fact profile");
    let parser_profile = ParserProfileId::new(109).expect("parser profile");
    let limits = SourceFactsRootLimits::default();
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    complete_clean_source_facts(&mut runtime, profile, parser_profile, limits);
    let base = runtime
        .persistent_source_facts()
        .expect("persistent clean base")
        .source();
    drop(
        runtime
            .take_certified_source()
            .expect("release clean certification projection"),
    );

    let edit_start = source.find("beta").expect("editable word");
    let edit_end = edit_start + "beta".len();
    let target = runtime
        .apply_edit(base, edit_start..edit_end, "delta")
        .expect("local replacement")
        .source()
        .current();
    let first_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, limits)
        .expect("first exact plan");
    assert_eq!(
        first_plan.exact_parser_base_byte_range(),
        Some(&(edit_start..edit_end))
    );
    assert!(runtime.cancel_source_facts());
    assert_eq!(
        runtime
            .persistent_source_facts()
            .expect("cancellation restores persistent base")
            .source(),
        base
    );

    let second_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, limits)
        .expect("restarted exact plan");
    assert_eq!(second_plan.source(), target);
    assert_eq!(
        second_plan.exact_parser_base_byte_range(),
        first_plan.exact_parser_base_byte_range()
    );
    assert_eq!(
        second_plan.exact_parser_target_byte_range(),
        first_plan.exact_parser_target_byte_range()
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    assert_eq!(
        witness.exact_parser_target_byte_range(),
        second_plan.exact_parser_target_byte_range()
    );
    let witness = runtime
        .take_persistent_source_facts_delta(witness)
        .expect("one-use envelope authority revalidates");
    assert!(matches!(
        runtime.take_persistent_source_facts_delta(witness),
        Err(DocumentRuntimeError::PersistentSourceFactsDeltaStale)
    ));
    close(runtime);
}

#[test]
fn cancelled_incremental_promotion_restores_base_for_nearby_rapid_edits() {
    let unit = "alpha **bold** 😀\r\nbeta\n";
    let source = unit.repeat(120);
    let profile = SourceFactsScanProfile::new(4).expect("source-fact profile");
    let parser_profile = ParserProfileId::new(29).expect("parser profile");
    let limits = SourceFactsRootLimits::default();
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    runtime
        .begin_source_facts(profile, parser_profile, limits)
        .expect("begin clean facts");
    loop {
        match runtime
            .poll_source_facts(128, 64)
            .expect("bounded clean SourceFacts poll")
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. }
            | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
            RuntimeSourceFactsPoll::Complete { .. } => break,
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
            | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                panic!("clean SourceFacts job reported incremental progress")
            }
        }
    }
    let base = runtime
        .persistent_source_facts()
        .expect("persistent clean base");
    let baseline_resident_nodes = runtime.arena_metrics().resident_nodes;
    drop(
        runtime
            .take_certified_source()
            .expect("release legacy clean projection"),
    );

    let first_start = source
        .match_indices("**bold**")
        .nth(60)
        .expect("middle markdown occurrence")
        .0;
    let first_end = first_start + "**bold**".len();
    let first_replacement = "**stronger markdown**";
    let mut after_first = source.clone();
    after_first.replace_range(first_start..first_end, first_replacement);
    let first_target = runtime
        .apply_edit(base.source(), first_start..first_end, first_replacement)
        .expect("first edit")
        .source()
        .current();
    let first_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, limits)
        .expect("first incremental plan");
    assert_eq!(first_plan.lineage_transitions(), 1);
    assert!(first_plan.planning_work().node_headers_decoded() > 0);

    let mut scan_complete = false;
    loop {
        match runtime
            .poll_source_facts(19, 5)
            .expect("bounded first incremental poll")
        {
            RuntimeSourceFactsPoll::PromotionPending { transitions } if scan_complete => {
                assert_eq!(transitions, 1);
                break;
            }
            RuntimeSourceFactsPoll::PromotionPending { .. } => {}
            RuntimeSourceFactsPoll::Pending(_) => {}
            RuntimeSourceFactsPoll::IncrementalScanComplete { source, .. } => {
                assert_eq!(source, first_target);
                scan_complete = true;
            }
            RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                panic!("promotion completed before the cancellation point")
            }
            RuntimeSourceFactsPoll::ScanComplete { .. }
            | RuntimeSourceFactsPoll::Complete { .. } => {
                panic!("incremental SourceFacts job reported clean progress")
            }
        }
    }
    assert!(
        runtime.arena_metrics().live_builds > 0
            || runtime.arena_metrics().resident_nodes > baseline_resident_nodes
    );

    let second_start = after_first
        .find("stronger")
        .expect("text introduced by first edit")
        + "strong".len();
    let mut after_second = after_first.clone();
    after_second.insert_str(second_start, "_live_");
    let second_target = runtime
        .apply_edit(first_target, second_start..second_start, "_live_")
        .expect("second edit cancels first incremental promotion")
        .source()
        .current();
    assert_eq!(
        runtime
            .persistent_source_facts()
            .expect("cancel restored the prior base")
            .source(),
        base.source()
    );
    while !runtime.poll_retirement(1).complete {}
    assert_eq!(
        runtime.arena_metrics().resident_nodes,
        baseline_resident_nodes
    );
    assert_eq!(runtime.arena_metrics().live_builds, 0);

    let second_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, limits)
        .expect("nearby rapid edits remain incrementally admissible");
    assert_eq!(second_plan.base(), base.source());
    assert_eq!(second_plan.source(), second_target);
    assert_eq!(second_plan.lineage_transitions(), 2);
    loop {
        match runtime
            .poll_source_facts(19, 5)
            .expect("bounded second incremental poll")
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. }
            | RuntimeSourceFactsPoll::IncrementalScanComplete { .. } => {}
            RuntimeSourceFactsPoll::IncrementalComplete { source, .. } => {
                assert_eq!(source, second_target);
                break;
            }
            RuntimeSourceFactsPoll::ScanComplete { .. }
            | RuntimeSourceFactsPoll::Complete { .. } => {
                panic!("incremental SourceFacts job reported clean progress")
            }
        }
    }

    let mut oracle = DocumentRuntime::new(&after_second, DocumentRuntimeConfig::default())
        .expect("clean oracle");
    oracle
        .begin_source_facts(profile, parser_profile, limits)
        .expect("begin oracle facts");
    loop {
        match oracle
            .poll_source_facts(128, 64)
            .expect("bounded oracle SourceFacts poll")
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. }
            | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
            RuntimeSourceFactsPoll::Complete { .. } => break,
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
            | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                panic!("clean oracle reported incremental progress")
            }
        }
    }
    assert_eq!(
        runtime
            .persistent_source_facts()
            .expect("incremental facts")
            .summary(),
        oracle
            .persistent_source_facts()
            .expect("oracle facts")
            .summary()
    );

    let third_target = runtime
        .apply_edit(second_target, 0..0, "prefix ")
        .expect("rapid prefix edit")
        .source()
        .current();
    let fourth_target = runtime
        .apply_edit(
            third_target,
            third_target.byte_len()..third_target.byte_len(),
            " suffix",
        )
        .expect("rapid distant tail edit")
        .source()
        .current();
    assert!(matches!(
        runtime.begin_incremental_source_facts(profile, parser_profile, limits),
        Err(DocumentRuntimeError::IncrementalSourceFactsLineageUnavailable)
    ));
    assert_eq!(
        runtime
            .persistent_source_facts()
            .expect("failed reuse keeps the last acknowledged exact base")
            .source(),
        base.source()
    );
    runtime
        .begin_source_facts(profile, parser_profile, limits)
        .expect("explicit clean fallback");
    loop {
        match runtime
            .poll_source_facts(128, 64)
            .expect("bounded clean fallback poll")
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. }
            | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
            RuntimeSourceFactsPoll::Complete { completion, .. } => {
                assert_eq!(completion.source(), fourth_target);
                break;
            }
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
            | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                panic!("clean fallback reported incremental progress")
            }
        }
    }

    close(runtime);
    close(oracle);
}

#[test]
fn incremental_splice_mints_an_exact_one_use_delta_witness() {
    let (mut runtime, plan, witness, base_page_count) = mint_middle_source_facts_delta(320);
    let replacement_pages = witness.target_page_range().end - witness.target_page_range().start;
    let replaced_base_pages = witness.base_page_range().end - witness.base_page_range().start;

    assert_eq!(witness.base(), plan.base());
    assert_eq!(witness.target(), plan.source());
    assert_eq!(
        witness.parser_profile(),
        ParserProfileId::new(73).expect("parser profile")
    );
    assert_eq!(
        witness.profile(),
        SourceFactsScanProfile::new(4).expect("source-fact profile")
    );
    assert_eq!(witness.base_page_range(), plan.base_page_range());
    assert_eq!(witness.base_byte_range(), plan.base_byte_range());
    assert_eq!(witness.target_byte_range(), plan.target_byte_range());
    assert_eq!(witness.base_page_count(), base_page_count);
    assert_eq!(
        witness.target_page_range().start,
        witness.base_page_range().start
    );
    assert_eq!(witness.lineage_transitions(), plan.lineage_transitions());
    assert_eq!(witness.planning_work(), plan.planning_work());
    assert_eq!(
        witness.scan_work().source_bytes_examined(),
        plan.target_byte_range().end - plan.target_byte_range().start
    );
    assert_eq!(
        witness.replacement_work().leaves_adopted(),
        usize::try_from(replacement_pages).expect("replacement page count")
    );
    assert_eq!(
        witness.splice_work().leaves_deleted(),
        usize::try_from(replaced_base_pages).expect("replaced base page count")
    );
    assert_eq!(
        witness.splice_work().leaves_reused(),
        usize::try_from(base_page_count - replaced_base_pages).expect("reused base page count")
    );
    assert!(witness.splice_work().maximum_atomic_height() > 0);
    assert!(witness.splice_work().seal_transitions() > 0);
    assert_eq!(
        format!("{:?}", witness.target_root_authority()),
        "PersistentSourceFactsDeltaRootAuthority(..)"
    );

    let witness = runtime
        .take_persistent_source_facts_delta(witness)
        .expect("exact target root revalidates the witness");
    assert_eq!(witness.target(), plan.source());
    assert!(matches!(
        runtime.take_persistent_source_facts_delta(witness),
        Err(DocumentRuntimeError::PersistentSourceFactsDeltaStale)
    ));
    close(runtime);
}

#[test]
fn foreign_runtime_cannot_adopt_a_delta_witness() {
    let (runtime, _, witness, _) = mint_middle_source_facts_delta(192);
    let mut foreign =
        DocumentRuntime::new("foreign", DocumentRuntimeConfig::default()).expect("foreign runtime");
    assert!(matches!(
        foreign.take_persistent_source_facts_delta(witness),
        Err(DocumentRuntimeError::PersistentSourceFactsDeltaForeignRuntime)
    ));
    close(foreign);
    close(runtime);
}

#[test]
fn structural_commit_advances_the_persistent_source_facts_base() {
    let (mut runtime, first_plan, witness, _) = mint_middle_source_facts_delta(256);
    let first_target = witness.target();
    let witness = runtime
        .take_persistent_source_facts_delta(witness)
        .expect("reserve exact SourceFacts transaction");
    assert_eq!(witness.target(), first_target);
    drop(witness);

    assert!(runtime
        .commit_persistent_source_facts_delta(first_target)
        .expect("commit structurally acknowledged target"));
    assert!(!runtime
        .commit_persistent_source_facts_delta(first_target)
        .expect("commit is idempotent when no transaction remains"));

    let second_edit = first_plan.target_byte_range().start;
    let second_target = runtime
        .apply_edit(first_target, second_edit..second_edit, "!")
        .expect("edit committed target")
        .source()
        .current();
    let successor = runtime
        .begin_incremental_source_facts(
            SourceFactsScanProfile::new(4).expect("source-fact profile"),
            ParserProfileId::new(73).expect("parser profile"),
            SourceFactsRootLimits::default(),
        )
        .expect("committed target is next exact base");
    assert_eq!(successor.base(), first_target);
    assert_eq!(successor.source(), second_target);
    assert_eq!(successor.lineage_transitions(), 1);
    assert!(runtime.cancel_source_facts());
    close(runtime);
}

#[test]
fn edit_candidate_supersession_and_close_each_invalidate_a_delta_witness() {
    let (mut edited, _, edit_witness, _) = mint_middle_source_facts_delta(160);
    let target = edit_witness.target();
    edited
        .apply_edit(target, 0..0, "!")
        .expect("later edit supersedes delta target");
    assert!(matches!(
        edited.take_persistent_source_facts_delta(edit_witness),
        Err(DocumentRuntimeError::PersistentSourceFactsDeltaStale)
    ));
    close(edited);

    let (mut superseded, _, superseded_witness, _) = mint_middle_source_facts_delta(160);
    superseded.begin_candidate().expect("target candidate");
    superseded
        .supersede_candidate()
        .expect("candidate supersession");
    assert!(matches!(
        superseded.take_persistent_source_facts_delta(superseded_witness),
        Err(DocumentRuntimeError::PersistentSourceFactsDeltaStale)
    ));
    close(superseded);

    let (mut closing, _, close_witness, _) = mint_middle_source_facts_delta(160);
    closing.begin_close().expect("begin close");
    assert!(matches!(
        closing.take_persistent_source_facts_delta(close_witness),
        Err(DocumentRuntimeError::NotOpen {
            state: DocumentState::Closing
        })
    ));
    close(closing);
}

#[test]
fn dropping_a_delta_or_cancelling_its_successor_leaves_no_extra_owner() {
    let (mut runtime, first_plan, witness, _) = mint_middle_source_facts_delta(256);
    let acknowledged_base = witness.base();
    let first_target = witness.target();
    while !runtime.poll_retirement(1).complete {}
    drop(witness);

    // Keep the superseding edit inside the first delta's authenticated crop so
    // the runtime can compose base -> first target -> second target exactly.
    let second_edit = first_plan.target_byte_range().start;
    let second_target = runtime
        .apply_edit(first_target, second_edit..second_edit, "!")
        .expect("successor edit")
        .source()
        .current();
    assert_eq!(
        runtime
            .persistent_source_facts()
            .expect("uncommitted target rolls back to acknowledged base")
            .source(),
        acknowledged_base
    );
    while !runtime.poll_retirement(1).complete {}
    let resident_after_rollback = runtime.arena_metrics().resident_nodes;
    let profile = SourceFactsScanProfile::new(4).expect("source-fact profile");
    let parser_profile = ParserProfileId::new(73).expect("parser profile");
    let successor = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("successor incremental plan");
    assert_eq!(successor.base(), acknowledged_base);
    assert_eq!(successor.source(), second_target);
    assert_eq!(successor.lineage_transitions(), 2);
    assert!(matches!(
        runtime
            .poll_source_facts(1, 1)
            .expect("bounded successor poll"),
        RuntimeSourceFactsPoll::Pending(_) | RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
    ));
    assert!(runtime.cancel_source_facts());
    assert_eq!(
        runtime
            .persistent_source_facts()
            .expect("cancel restores exact prior base")
            .source(),
        acknowledged_base
    );
    assert_eq!(runtime.current_source_version(), Some(second_target));
    while !runtime.poll_retirement(1).complete {}
    assert_eq!(
        runtime.arena_metrics().resident_nodes,
        resident_after_rollback
    );
    close(runtime);
}

#[test]
fn delta_work_scales_with_changed_pages_and_tree_height_not_document_pages() {
    fn observe(unit_count: usize) -> (u64, u64, usize, usize, u16, usize) {
        let (runtime, plan, witness, base_pages) = mint_middle_source_facts_delta(unit_count);
        let changed_pages =
            usize::try_from(witness.target_page_range().end - witness.target_page_range().start)
                .expect("changed pages");
        let observed = (
            base_pages,
            plan.planning_work().node_headers_decoded(),
            witness.scan_work().source_bytes_examined(),
            witness.splice_work().nodes_visited(),
            witness.splice_work().maximum_atomic_height(),
            changed_pages,
        );
        drop(witness);
        close(runtime);
        observed
    }

    let small = observe(128);
    let large = observe(8192);
    assert!(
        large.0 > small.0 * 32,
        "fixture must materially scale pages"
    );
    assert!(
        small.5 <= 2 && large.5 <= 2,
        "edit changes at most two pages"
    );
    assert!(
        large.2 <= small.2 + 1024,
        "cropped scan must remain page-local"
    );

    let large_envelope = 16 * (large.5 + usize::from(large.4) + 1);
    assert!(
        usize::try_from(large.1).expect("planning headers") <= large_envelope,
        "planning inspection must fit changed pages plus height"
    );
    assert!(
        large.3 <= large_envelope,
        "splice visits must fit changed pages plus height"
    );
    assert!(
        u64::try_from(large.3).expect("visited nodes") * 8 < large.0,
        "changed-path visits must remain sublinear in unchanged page count"
    );
}

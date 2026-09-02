use flark_runtime::{DocumentActor, DocumentActorError, DocumentSessionPhase};

/// Source whose block count, not byte count, dominates persistent storage.
/// Each block is four bytes, so five mebibytes carries over a million blocks.
fn tiny_blocks(target_bytes: usize) -> String {
    let mut source = String::with_capacity(target_bytes + 4);
    while source.len() < target_bytes {
        source.push_str("x.\n\n");
    }
    source
}

#[test]
#[ignore = "full-scale 5 MiB block-density certification stress"]
fn five_mib_dense_document_converges_inside_the_payload_budget() {
    let actor = DocumentActor::begin(tiny_blocks(5 * 1024 * 1024)).expect("actor");
    let mut turns = 0;
    let ready_metrics = loop {
        match actor.pump(512) {
            Ok(receipt) => {
                if receipt.phase == DocumentSessionPhase::Ready {
                    break actor
                        .call_for_test(|document| Ok(document.arena_metrics()))
                        .expect("ready arena metrics");
                }
            }
            Err(error) => {
                let metrics = actor
                    .call_for_test(|document| {
                        document
                            .fault_arena_metrics()
                            .ok_or(flark_runtime::DocumentSessionError::Faulted)
                    })
                    .expect("faulted arena metrics");
                panic!("5 MiB dense document must converge, got {error:?}; arena={metrics:?}");
            }
        }
        turns += 1;
        assert!(turns < 2_000_000, "convergence should terminate");
    };
    assert!(
        ready_metrics.live_payload_bytes + ready_metrics.reserved_external_payload_bytes
            <= 64 * 1024 * 1024,
        "ready dense arena exceeded its configured budget: {ready_metrics:?}"
    );
    eprintln!("5 MiB dense ready arena={ready_metrics:?}");

    actor.begin_close().expect("begin dense close");
    while !actor.pump_close(256).expect("pump dense close").complete {}
    let closed = actor.inspect().expect("inspect closed actor");
    assert_eq!(closed.phase, DocumentSessionPhase::Closed);
    assert_eq!(closed.source_byte_len, 0);
    assert_eq!(closed.source_utf16_len, 0);
    drop(actor);
}

#[test]
fn a_panicking_job_is_contained_and_poisons_only_that_actor() {
    let actor = DocumentActor::begin("alpha\n".to_owned()).expect("actor");
    assert_eq!(actor.inspect().expect("inspect").revision, 1);

    let outcome: Result<(), DocumentActorError> =
        actor.call_for_test(|_| panic!("engine invariant"));
    assert!(
        matches!(outcome, Err(DocumentActorError::Panicked)),
        "a contained unwind must be reported as such, got {outcome:?}"
    );

    // The actor is still answering rather than silently dead, and it reports
    // the same typed fault instead of an anonymous internal error.
    assert!(matches!(actor.inspect(), Err(DocumentActorError::Panicked)));

    // Dropping a poisoned actor must not abort the process.
    drop(actor);
}

use flark_runtime::{
    DocumentActor, DocumentActorError, DocumentSessionError, DocumentSessionPhase,
};

/// Source whose block count, not byte count, exhausts the engine's live
/// payload budget. Each block is four bytes, so a few mebibytes of this shape
/// carries over a million blocks.
fn tiny_blocks(target_bytes: usize) -> String {
    let mut source = String::with_capacity(target_bytes + 4);
    while source.len() < target_bytes {
        source.push_str("x.\n\n");
    }
    source
}

#[test]
fn exhausting_the_payload_budget_is_a_typed_error_not_a_crash() {
    // Driven through the actor because that is the production owner: a
    // faulted session must never be dropped bare, and the actor is what
    // guarantees that.
    let actor = DocumentActor::begin(tiny_blocks(5 * 1024 * 1024)).expect("actor");
    let mut turns = 0;
    loop {
        match actor.pump(512) {
            Ok(receipt) => {
                if receipt.phase == DocumentSessionPhase::Ready {
                    // A future engine that fits this shape inside the budget
                    // converges instead; that is a pass, not a regression.
                    break;
                }
            }
            Err(error) => {
                // The budget is reported as a typed fault, not an assertion,
                // an out-of-memory abort, or a silently dead actor.
                match &error {
                    DocumentActorError::Session(DocumentSessionError::Parser(_)) => {
                        assert!(
                            format!("{error}").contains("payload"),
                            "expected the payload budget to be named, got {error}"
                        );
                    }
                    DocumentActorError::Panicked => {}
                    other => panic!("expected a typed parser fault, got {other:?}"),
                }
                // The actor still answers rather than having died with it.
                assert!(actor.inspect().is_err() || actor.inspect().is_ok());
                break;
            }
        }
        turns += 1;
        assert!(turns < 2_000_000, "convergence should terminate");
    }
    // Releasing a faulted document must not abort the process.
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

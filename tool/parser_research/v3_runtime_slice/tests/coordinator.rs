use flark_v3_runtime_slice::{
    ArenaError, ArenaScopedId, Coordinator, CoordinatorError, GrammarRevision, OutputRootLease,
    OwnedArenaRef, PageArena, ParseGeneration, SourceRevision, SourceRootId, SourceStore,
};

fn allocate(arena: &mut PageArena, payload: impl AsRef<[u8]>) -> OwnedArenaRef {
    arena
        .allocate(payload.as_ref(), &[])
        .expect("bounded root allocation")
        .owner
}

#[test]
fn revision_zero_has_one_generation_one_initial_parse() {
    let source = SourceStore::new("", 8);
    let mut arena = PageArena::new();
    let initial = allocate(&mut arena, "bootstrap-unparsed");
    let mut coordinator = Coordinator::new(source.root_id(), initial);

    let plan = coordinator
        .begin_initial_parse()
        .expect("admit exact revision-zero parse");
    assert_eq!(plan.token.source_revision, SourceRevision(0));
    assert_eq!(plan.token.source_root, source.root_id());
    assert_eq!(plan.token.generation, ParseGeneration(1));
    assert_eq!(coordinator.active_plan(), Some(plan));
    assert_eq!(
        coordinator.begin_initial_parse(),
        Err(CoordinatorError::InitialParseAlreadyAdmitted)
    );
}

#[test]
fn source_transition_chain_and_latest_queued_plan_are_exact() {
    let mut source = SourceStore::new("a", 8);
    let mut arena = PageArena::new();
    let initial = allocate(&mut arena, "initial");
    let mut coordinator = Coordinator::new(source.root_id(), initial);

    let first = source
        .apply_edit(SourceRevision(0), 1..1, "b")
        .expect("first edit");
    let first_plan = coordinator
        .accept_source_transition(first.transition)
        .expect("admit first")
        .active;
    let second = source
        .apply_edit(SourceRevision(1), 2..2, "c")
        .expect("second edit");
    let second_receipt = coordinator
        .accept_source_transition(second.transition)
        .expect("queue second");
    assert_eq!(second_receipt.active, first_plan);
    assert_eq!(
        second_receipt.queued.expect("queued").token.generation,
        ParseGeneration(2)
    );

    let third = source
        .apply_edit(SourceRevision(2), 3..3, "d")
        .expect("third edit");
    let third_receipt = coordinator
        .accept_source_transition(third.transition)
        .expect("replace queued");
    assert_eq!(
        third_receipt
            .replaced_queued
            .expect("second replaced")
            .generation,
        ParseGeneration(2)
    );
    assert_eq!(
        third_receipt
            .queued
            .expect("latest queued")
            .token
            .generation,
        ParseGeneration(3)
    );

    let promotion = coordinator
        .promote_latest(&mut arena)
        .expect("promote latest");
    assert_eq!(promotion.cancelled, first_plan.token);
    assert_eq!(promotion.promoted.token.generation, ParseGeneration(3));
    assert!(coordinator.queued_plan().is_none());

    let invalid = flark_v3_runtime_slice::SourceTransition {
        base_revision: SourceRevision(1),
        target_revision: SourceRevision(2),
        base_root: SourceRootId(999),
        result_root: SourceRootId(1_000),
    };
    assert_eq!(
        coordinator.accept_source_transition(invalid),
        Err(CoordinatorError::InvalidTransition)
    );
    assert_eq!(coordinator.source_revision(), SourceRevision(3));

    let before_zero_root = format!("{coordinator:?}");
    let zero_result_root = flark_v3_runtime_slice::SourceTransition {
        base_revision: SourceRevision(3),
        target_revision: SourceRevision(4),
        base_root: coordinator.source_root(),
        result_root: SourceRootId(0),
    };
    assert_eq!(
        coordinator.accept_source_transition(zero_result_root),
        Err(CoordinatorError::InvalidTransition)
    );
    assert_eq!(format!("{coordinator:?}"), before_zero_root);
}

#[test]
fn stale_generation_cannot_attach_or_commit_and_candidate_retires_with_fuel() {
    let mut source = SourceStore::new("a", 8);
    let mut arena = PageArena::new();
    let initial = allocate(&mut arena, "initial");
    let mut coordinator = Coordinator::new(source.root_id(), initial);

    let first = source
        .apply_edit(SourceRevision(0), 1..1, "b")
        .expect("first edit");
    let first_token = coordinator
        .accept_source_transition(first.transition)
        .expect("admit first")
        .active
        .token;
    let stale_candidate = allocate(&mut arena, "stale");
    let stale_candidate_id = stale_candidate.scoped_id();
    let stale_candidate_local = stale_candidate.id();
    coordinator
        .attach_candidate(first_token, stale_candidate, &mut arena)
        .expect("attach while current");

    let second = source
        .apply_edit(SourceRevision(1), 2..2, "c")
        .expect("second edit");
    let second_token = coordinator
        .accept_source_transition(second.transition)
        .expect("queue second")
        .queued
        .expect("queued")
        .token;
    assert!(matches!(
        coordinator.commit(first_token, &mut arena),
        Err(CoordinatorError::StaleGeneration { .. })
    ));

    let unattached = allocate(&mut arena, "caller-still-owns");
    let unattached_id = unattached.id();
    let rejected = coordinator
        .attach_candidate(first_token, unattached, &mut arena)
        .expect_err("stale generation cannot consume candidate authority");
    assert!(matches!(
        rejected.error,
        CoordinatorError::StaleGeneration { .. }
    ));
    assert!(arena.contains(unattached_id));
    arena
        .release_later(rejected.candidate)
        .expect("caller deliberately retires rejected candidate");

    let promotion = coordinator
        .promote_latest(&mut arena)
        .expect("cancel stale");
    assert_eq!(promotion.promoted.token, second_token);
    assert_eq!(promotion.retired_candidate, Some(stale_candidate_id));
    let receipt = arena.poll_reclaim(2).expect("bounded retire");
    assert_eq!(receipt.reference_transitions, 2);
    assert!(!arena.contains(stale_candidate_local));
    assert!(!arena.contains(unattached_id));

    let current = allocate(&mut arena, "current");
    coordinator
        .attach_candidate(second_token, current, &mut arena)
        .expect("attach current");
    let delta = coordinator
        .commit(second_token, &mut arena)
        .expect("commit current");
    assert_eq!(delta.target_source_revision, SourceRevision(2));
    assert_eq!(coordinator.grammar_revision(), GrammarRevision(2));
    assert_eq!(
        coordinator.query_payload(delta.offered_output, &arena),
        Ok(&b"current"[..])
    );
}

#[test]
fn stalled_ui_ack_retires_the_prior_offer_before_publishing_for_1000_commits() {
    let mut source = SourceStore::new("", 4);
    let mut arena = PageArena::new();
    let initial = allocate(&mut arena, "initial");
    let mut coordinator = Coordinator::new(source.root_id(), initial);
    let stale_initial_lease = coordinator
        .acknowledged_output()
        .expect("initial root is acknowledged");
    let mut final_offer = None;

    for revision in 0..1_000_usize {
        let revision_u64 = u64::try_from(revision).expect("test revision fits u64");
        let edit = source
            .apply_edit(SourceRevision(revision_u64), revision..revision, "x")
            .expect("append source");
        let token = coordinator
            .accept_source_transition(edit.transition)
            .expect("admit latest")
            .active
            .token;
        let candidate = allocate(&mut arena, revision_u64.to_le_bytes());
        coordinator
            .attach_candidate(token, candidate, &mut arena)
            .expect("attach candidate");
        let delta = coordinator
            .commit(token, &mut arena)
            .expect("commit exact latest");
        final_offer = Some(delta.offered_output);
        assert!(coordinator.metrics().published_roots <= 3);
        let receipt = arena.poll_reclaim(1).expect("bounded background reclaim");
        assert!(receipt.reference_transitions <= 1);
    }

    let metrics = coordinator.metrics();
    // The acknowledged bootstrap root and newest worker-current offer remain
    // published. Every superseded unacknowledged offer is transferred to the
    // arena's bounded reclaim queue before its replacement is installed, so
    // the coordinator itself never holds a third published root on this path.
    assert_eq!(metrics.published_roots, 2);
    assert_eq!(metrics.maximum_published_roots, 2);
    assert!(arena.metrics().live_nodes <= 3);

    let final_offer = final_offer.expect("one offer");
    assert_eq!(
        coordinator.query_payload(final_offer, &arena),
        Ok(&999_u64.to_le_bytes()[..])
    );
    coordinator
        .acknowledge(final_offer, &mut arena)
        .expect("ack final offer");
    assert_eq!(coordinator.metrics().published_roots, 1);
    assert_eq!(coordinator.acknowledged_output(), Some(final_offer));
    assert!(matches!(
        coordinator.resolve_root(stale_initial_lease, &arena),
        Err(CoordinatorError::UnknownRoot(_))
    ));
    while arena.metrics().pending_releases > 0 {
        arena.poll_reclaim(1).expect("fuelled final release");
    }
    assert_eq!(arena.metrics().live_nodes, 1);
}

#[test]
fn releasing_remote_access_does_not_release_the_worker_current_root() {
    let mut source = SourceStore::new("a", 4);
    let mut arena = PageArena::new();
    let initial = allocate(&mut arena, "initial");
    let initial_id = initial.id();
    let mut coordinator = Coordinator::new(source.root_id(), initial);
    let initial_lease = coordinator
        .acknowledged_output()
        .expect("initial root acknowledged");

    coordinator
        .release_root(initial_lease, &mut arena)
        .expect("release remote access");
    assert_eq!(coordinator.acknowledged_output(), None);
    assert!(arena.contains(initial_id));
    assert!(matches!(
        coordinator.resolve_root(initial_lease, &arena),
        Err(CoordinatorError::UnknownRoot(_))
    ));
    assert!(matches!(
        coordinator.release_root(initial_lease, &mut arena),
        Err(CoordinatorError::UnknownRoot(_))
    ));

    let edit = source
        .apply_edit(SourceRevision(0), 1..1, "b")
        .expect("edit source");
    let token = coordinator
        .accept_source_transition(edit.transition)
        .expect("admit parse")
        .active
        .token;
    let candidate = allocate(&mut arena, "replacement");
    coordinator
        .attach_candidate(token, candidate, &mut arena)
        .expect("attach candidate");
    let offer = coordinator
        .commit(token, &mut arena)
        .expect("commit replacement")
        .offered_output;
    assert_eq!(
        coordinator.query_payload(offer, &arena),
        Ok(&b"replacement"[..])
    );
    arena.poll_reclaim(1).expect("retire old worker root");
    assert!(!arena.contains(initial_id));
}

#[test]
fn exact_root_lease_rejects_revision_or_generation_substitution() {
    let source = SourceStore::new("a", 4);
    let mut arena = PageArena::new();
    let initial = allocate(&mut arena, "initial");
    let coordinator = Coordinator::new(source.root_id(), initial);
    let lease = coordinator.current_output();
    let forged = OutputRootLease {
        parse_generation: ParseGeneration(99),
        ..lease
    };
    assert_eq!(
        coordinator.resolve_root(forged, &arena),
        Err(CoordinatorError::LeaseMismatch(lease.remote_root))
    );
}

#[test]
fn output_lease_cannot_alias_the_same_local_slot_in_another_arena() {
    let source = SourceStore::new("a", 4);
    let mut first = PageArena::new();
    let mut second = PageArena::new();
    let first_root = allocate(&mut first, "first");
    let second_root = allocate(&mut second, "second");
    assert_eq!(first_root.id(), second_root.id());

    let coordinator = Coordinator::new(source.root_id(), first_root);
    let lease = coordinator.current_output();
    assert_eq!(coordinator.query_payload(lease, &first), Ok(&b"first"[..]));
    assert!(matches!(
        coordinator.resolve_root(lease, &second),
        Err(CoordinatorError::Arena(ArenaError::WrongArena {
            expected,
            actual,
        })) if expected == first.identity() && actual == second.identity()
    ));
    assert_eq!(
        second
            .payload(second_root.id())
            .expect("second root untouched"),
        b"second"
    );

    second.release_later(second_root).expect("release second");
    second.poll_reclaim(1).expect("reclaim second");
}

#[test]
fn rejected_duplicate_candidate_returns_its_distinct_owner() {
    let mut source = SourceStore::new("a", 4);
    let mut arena = PageArena::new();
    let edge_only = allocate(&mut arena, "edge-only");
    let edge_only_id = edge_only.id();
    let initial = arena
        .allocate(b"initial", &[edge_only_id])
        .expect("parent")
        .owner;
    let initial_id = initial.id();
    let initial_root = initial.scoped_id();
    arena
        .release_later(edge_only)
        .expect("release caller child ownership");
    arena.poll_reclaim(1).expect("leave only parent edge");
    let mut coordinator = Coordinator::new(source.root_id(), initial);
    let edit = source
        .apply_edit(SourceRevision(0), 1..1, "b")
        .expect("source edit");
    let token = coordinator
        .accept_source_transition(edit.transition)
        .expect("admit parse")
        .active
        .token;

    assert!(!arena.contains(edge_only_id) || arena.payload(edge_only_id).is_ok());
    let duplicate_owner = arena.retain(initial_id).expect("distinct retained owner");
    let rejected = coordinator
        .attach_candidate(token, duplicate_owner, &mut arena)
        .expect_err("published root cannot also become candidate");
    assert_eq!(
        rejected.error,
        CoordinatorError::DuplicateArenaRoot(initial_root)
    );
    assert_eq!(rejected.candidate.scoped_id(), initial_root);
    arena
        .release_later(rejected.candidate)
        .expect("caller retains authority to retire rejected duplicate");
    arena.poll_reclaim(1).expect("retire rejected transfer");
    assert!(arena.contains(initial_id));
}

#[test]
fn foreign_candidate_or_supplied_arena_returns_authority_without_mutation() {
    let mut source = SourceStore::new("a", 4);
    let mut first = PageArena::new();
    let mut second = PageArena::new();
    let initial = allocate(&mut first, "initial");
    let mut coordinator = Coordinator::new(source.root_id(), initial);
    assert_eq!(coordinator.arena_identity(), first.identity());

    let edit = source
        .apply_edit(SourceRevision(0), 1..1, "b")
        .expect("source edit");
    let token = coordinator
        .accept_source_transition(edit.transition)
        .expect("admit parse")
        .active
        .token;

    let retryable = allocate(&mut first, "retryable");
    let retryable_id = retryable.scoped_id();
    let coordinator_before = format!("{coordinator:?}");
    let first_before = first.metrics();
    let second_before = second.metrics();
    let rejected = coordinator
        .attach_candidate(token, retryable, &mut second)
        .expect_err("foreign supplied arena must be rejected first");
    assert!(matches!(
        rejected.error,
        CoordinatorError::Arena(ArenaError::WrongArena { expected, actual })
            if expected == first.identity() && actual == second.identity()
    ));
    assert_eq!(rejected.candidate.scoped_id(), retryable_id);
    assert_eq!(format!("{coordinator:?}"), coordinator_before);
    assert_eq!(first.metrics(), first_before);
    assert_eq!(second.metrics(), second_before);

    coordinator
        .attach_candidate(token, rejected.candidate, &mut first)
        .expect("returned authority can be retried in the bound arena");

    let foreign = allocate(&mut second, "foreign");
    let foreign_id = foreign.scoped_id();
    let coordinator_before = format!("{coordinator:?}");
    let first_before = first.metrics();
    let second_before = second.metrics();
    let rejected = coordinator
        .attach_candidate(token, foreign, &mut first)
        .expect_err("foreign candidate cannot enter bound arena");
    assert!(matches!(
        rejected.error,
        CoordinatorError::Arena(ArenaError::WrongArena { expected, actual })
            if expected == first.identity() && actual == second.identity()
    ));
    assert_eq!(rejected.candidate.scoped_id(), foreign_id);
    assert_eq!(format!("{coordinator:?}"), coordinator_before);
    assert_eq!(first.metrics(), first_before);
    assert_eq!(second.metrics(), second_before);
    second
        .release_later(rejected.candidate)
        .expect("foreign arena still accepts returned authority");
}

#[test]
fn wrong_arena_preflight_leaves_all_coordinator_state_and_owners_unchanged() {
    let mut source = SourceStore::new("a", 8);
    let mut arena = PageArena::new();
    let mut foreign = PageArena::new();
    let foreign_root = allocate(&mut foreign, "foreign");
    let initial = allocate(&mut arena, "initial");
    let mut coordinator = Coordinator::new(source.root_id(), initial);
    let initial_lease = coordinator
        .acknowledged_output()
        .expect("initial output is acknowledged");

    let first = source
        .apply_edit(SourceRevision(0), 1..1, "b")
        .expect("first edit");
    let first_token = coordinator
        .accept_source_transition(first.transition)
        .expect("admit first")
        .active
        .token;
    let candidate = allocate(&mut arena, "candidate");
    let candidate_id = candidate.id();
    coordinator
        .attach_candidate(first_token, candidate, &mut arena)
        .expect("attach candidate");
    let second = source
        .apply_edit(SourceRevision(1), 2..2, "c")
        .expect("second edit");
    let promoted_token = coordinator
        .accept_source_transition(second.transition)
        .expect("queue second")
        .queued
        .expect("queued plan")
        .token;

    let coordinator_before = format!("{coordinator:?}");
    let arena_before = arena.metrics();
    let foreign_before = foreign.metrics();
    let arena_identity = arena.identity();
    let foreign_identity = foreign.identity();
    let assert_foreign = |error: CoordinatorError| {
        assert!(matches!(
            error,
            CoordinatorError::Arena(ArenaError::WrongArena { expected, actual })
                if expected == arena_identity && actual == foreign_identity
        ));
    };

    assert_foreign(
        coordinator
            .promote_latest(&mut foreign)
            .expect_err("foreign arena cannot promote"),
    );
    assert_foreign(
        coordinator
            .commit(first_token, &mut foreign)
            .expect_err("arena check precedes stale-token check"),
    );
    assert_foreign(
        coordinator
            .acknowledge(initial_lease, &mut foreign)
            .expect_err("foreign arena cannot acknowledge"),
    );
    assert_foreign(
        coordinator
            .release_root(initial_lease, &mut foreign)
            .expect_err("foreign arena cannot release"),
    );
    assert_foreign(
        coordinator
            .resolve_root(initial_lease, &foreign)
            .expect_err("foreign arena cannot resolve"),
    );
    assert_foreign(
        coordinator
            .query_payload(initial_lease, &foreign)
            .expect_err("foreign arena cannot query"),
    );

    assert_eq!(format!("{coordinator:?}"), coordinator_before);
    assert_eq!(arena.metrics(), arena_before);
    assert_eq!(foreign.metrics(), foreign_before);
    assert!(arena.contains(candidate_id));
    assert_eq!(
        foreign
            .payload(foreign_root.id())
            .expect("foreign owner is untouched"),
        b"foreign"
    );

    let promotion = coordinator
        .promote_latest(&mut arena)
        .expect("same state can still promote in bound arena");
    assert_eq!(promotion.promoted.token, promoted_token);
    assert_eq!(
        promotion.retired_candidate.map(ArenaScopedId::local),
        Some(candidate_id)
    );

    foreign
        .release_later(foreign_root)
        .expect("release foreign test owner");
}

use flark_v3_runtime_slice::{ArenaBuildError, ArenaError, ArenaLimits, PageArena};

fn limits(
    max_slots: u32,
    max_storage: usize,
    max_builds: u32,
    max_journal_slots: usize,
) -> ArenaLimits {
    ArenaLimits::new(max_slots, max_storage, max_builds, max_journal_slots)
}

fn settle(arena: &mut PageArena) {
    while arena.metrics().pending_releases != 0 {
        arena.poll_reclaim(64).expect("settle arena releases");
    }
}

#[test]
fn slot_segments_cross_boundaries_without_moving_prior_metadata() {
    let mut arena =
        PageArena::try_with_limits(limits(130, 1024 * 1024, 2, 64)).expect("admit bounded arena");
    let mut owners = Vec::new();
    for index in 0_u32..130 {
        let allocation = arena
            .allocate(&index.to_le_bytes(), &[])
            .expect("allocate across slot segments");
        let boundary = matches!(index, 0 | 64 | 128);
        assert_eq!(allocation.receipt.slot_metadata_segment_added, boundary);
        assert_eq!(
            allocation.receipt.slot_metadata_entries_initialized,
            if boundary { 64 } else { 0 }
        );
        assert_eq!(allocation.receipt.slot_metadata_prior_entries_moved, 0);
        if boundary {
            assert!(allocation.receipt.slot_metadata_segment_actual_capacity >= 64);
        }
        owners.push(allocation.owner);
    }

    for index in [0_usize, 63, 64, 127, 128, 129] {
        assert_eq!(
            arena
                .payload(owners[index].id())
                .expect("old page remains live"),
            &u32::try_from(index).unwrap().to_le_bytes()
        );
    }
    let metrics = arena.metrics();
    assert_eq!(metrics.slots, 130);
    assert_eq!(metrics.slot_capacity, 192);
    assert_eq!(metrics.slot_segments, 3);
    assert_eq!(metrics.slot_directory_logical_segments, 3);
    assert_eq!(metrics.slot_hard_limit, 130);
    assert_eq!(metrics.slot_metadata_prior_entries_moved, 0);
    assert_eq!(metrics.maximum_slot_segment_entries_initialized, 64);

    let before_failure = arena.metrics();
    assert_eq!(
        arena.allocate(b"over-limit", &[]),
        Err(ArenaError::SlotLimitReached { limit: 130 })
    );
    assert_eq!(arena.metrics(), before_failure);

    let recycled_id = owners[64].id();
    arena
        .release_later(owners.swap_remove(64))
        .expect("release one middle slot");
    arena.poll_reclaim(1).expect("reclaim middle slot");
    let replacement = arena.allocate(b"reuse", &[]).expect("reuse middle slot");
    assert!(replacement.receipt.slot_reused);
    assert!(!replacement.receipt.slot_metadata_segment_added);
    assert_eq!(replacement.receipt.slot_metadata_prior_entries_moved, 0);
    assert_eq!(replacement.owner.id().index, recycled_id.index);
    assert_ne!(replacement.owner.id().generation, recycled_id.generation);

    arena
        .release_later(replacement.owner)
        .expect("release replacement");
    for owner in owners {
        arena.release_later(owner).expect("release retained page");
    }
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn storage_budget_rejection_precedes_slot_or_reference_mutation() {
    let mut arena = PageArena::try_with_limits(limits(64, 1, 1, 16)).expect("admit tiny budget");
    let before = arena.metrics();
    assert!(matches!(
        arena.allocate(b"xx", &[]),
        Err(ArenaError::StorageBudgetExceeded {
            requested,
            limit: 1
        }) if requested >= 2
    ));
    assert_eq!(arena.metrics(), before);
    assert_eq!(arena.metrics().slot_segments, 0);

    let mut arena = PageArena::try_with_limits(limits(64, 1, 1, 16))
        .expect("admit child-reference budget probe");
    let child = arena.allocate(b"", &[]).expect("zero-byte child").owner;
    let child_id = child.id();
    let before = arena.metrics();
    assert!(matches!(
        arena.allocate(b"xx", &[child_id]),
        Err(ArenaError::StorageBudgetExceeded { limit: 1, .. })
    ));
    assert_eq!(arena.metrics(), before);
    arena
        .release_later(child)
        .expect("release only child owner");
    let reclaim = arena
        .poll_reclaim(1)
        .expect("no failed parent edge remains");
    assert_eq!(reclaim.nodes_reclaimed, 1);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn build_directory_is_preflighted_and_saturates_without_moving_slots() {
    let mut arena =
        PageArena::try_with_limits(limits(64, 1024, 2, 16)).expect("admit two-build arena");
    let (first, first_receipt) = arena.begin_build_with_receipt().expect("admit first build");
    let (second, second_receipt) = arena
        .begin_build_with_receipt()
        .expect("admit second build");
    for receipt in [first_receipt, second_receipt] {
        assert!(receipt.build_slot_added);
        assert_eq!(receipt.build_slots_initialized, 1);
        assert_eq!(receipt.prior_build_slots_moved, 0);
        assert_eq!(receipt.build_directory_logical_limit, 2);
        assert!(receipt.build_directory_actual_capacity >= 2);
    }
    let before = arena.metrics();
    assert!(matches!(
        arena.begin_build(),
        Err(ArenaBuildError::BuildLimitReached { limit: 2 })
    ));
    assert_eq!(arena.metrics(), before);

    let first_id = arena.begin_build_abort(first).expect("abort first");
    assert!(arena.poll_build_abort(first_id, 0).unwrap().complete);
    let (reused, receipt) = arena
        .begin_build_with_receipt()
        .expect("reuse first build slot");
    assert!(receipt.build_slot_reused);
    assert!(!receipt.build_slot_added);
    assert_eq!(receipt.prior_build_slots_moved, 0);
    let reused_id = arena.begin_build_abort(reused).expect("abort reused");
    assert!(arena.poll_build_abort(reused_id, 0).unwrap().complete);
    let second_id = arena.begin_build_abort(second).expect("abort second");
    assert!(arena.poll_build_abort(second_id, 0).unwrap().complete);
}

#[test]
fn journal_segments_and_saturation_are_bounded_before_page_mutation() {
    let mut arena = PageArena::try_with_limits(limits(1_100, 1024 * 1024, 1, 1_025))
        .expect("admit bounded journal");
    let ticket = arena.begin_build().expect("begin build");
    let build = ticket.id();
    let mut session = arena.resume_build(ticket).expect("resume build");
    for index in 0_u32..1_025 {
        let (_, receipt) = session
            .allocate(&index.to_le_bytes(), &[])
            .expect("allocate owner");
        let boundary = index.is_multiple_of(16);
        assert_eq!(receipt.owner_journal_segment_added, boundary);
        assert_eq!(
            receipt.owner_journal_entries_initialized,
            if boundary { 16 } else { 0 }
        );
        assert_eq!(receipt.owner_journal_prior_entries_moved, 0);
        if boundary {
            assert!(receipt.owner_journal_segment_actual_capacity >= 16);
        }
        let directory_boundary = matches!(index, 0 | 1_024);
        assert_eq!(
            receipt.owner_journal_directory_block_added,
            directory_boundary
        );
        assert_eq!(
            receipt.owner_journal_directory_descriptors_preflighted,
            if directory_boundary { 64 } else { 0 }
        );
        if directory_boundary {
            assert!(receipt.owner_journal_directory_actual_capacity >= 64);
        } else {
            assert_eq!(receipt.owner_journal_directory_actual_capacity, 0);
        }
        assert_eq!(receipt.owner_journal_prior_directory_descriptors_moved, 0);
    }
    let before = session.arena().metrics();
    assert!(matches!(
        session.allocate(b"journal-over-limit", &[]),
        Err(ArenaBuildError::Arena(
            ArenaError::OwnerJournalLimitReached { limit: 1_025 }
        ))
    ));
    assert_eq!(session.arena().metrics(), before);
    let metrics = session
        .arena()
        .build_journal_metrics(build)
        .expect("journal receipt");
    assert_eq!(metrics.slots, 1_025);
    assert_eq!(metrics.initialized_slot_capacity, 1_040);
    assert!(metrics.slot_capacity >= 1_040);
    assert_eq!(metrics.segments, 65);
    assert_eq!(metrics.directory_blocks, 2);
    assert_eq!(metrics.hard_slot_limit, 1_025);
    assert_eq!(metrics.prior_entries_moved, 0);
    assert_eq!(metrics.prior_directory_descriptors_moved, 0);
    assert_eq!(metrics.maximum_segment_entries_initialized, 16);
    assert_eq!(metrics.maximum_directory_descriptors_preflighted, 64);

    drop(session);
    assert_eq!(arena.metrics().pending_releases, 0);
    let mut scheduled = 0;
    loop {
        let abort = arena
            .poll_build_abort(build, 31)
            .expect("bounded journal abort");
        assert!(abort.owners_scheduled <= 31);
        scheduled += abort.owners_scheduled;
        if abort.complete {
            break;
        }
    }
    assert_eq!(scheduled, 1_025);
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn saturated_candidate_cleans_up_without_disturbing_committed_root() {
    let mut arena =
        PageArena::try_with_limits(limits(64, 1024 * 1024, 2, 2)).expect("admit saturation arena");
    let committed_ticket = arena.begin_build().expect("begin committed build");
    let mut committed_session = arena
        .resume_build(committed_ticket)
        .expect("resume committed build");
    let (committed, _) = committed_session
        .allocate(b"committed", &[])
        .expect("allocate committed root");
    let committed_id = committed_session.owner_id(&committed).unwrap();
    let committed = committed_session.commit(committed).expect("commit root");

    let candidate_ticket = arena.begin_build().expect("begin candidate");
    let candidate_id = candidate_ticket.id();
    let mut candidate = arena
        .resume_build(candidate_ticket)
        .expect("resume candidate");
    let (_first, _) = candidate.allocate(b"first", &[]).expect("first owner");
    let (_second, _) = candidate.allocate(b"second", &[]).expect("second owner");
    let before = candidate.arena().metrics();
    assert!(matches!(
        candidate.allocate(b"saturated", &[]),
        Err(ArenaBuildError::Arena(
            ArenaError::OwnerJournalLimitReached { limit: 2 }
        ))
    ));
    assert_eq!(candidate.arena().metrics(), before);
    assert_eq!(
        candidate.arena().payload(committed_id).unwrap(),
        b"committed"
    );

    drop(candidate);
    let receipt = arena
        .poll_build_abort(candidate_id, 2)
        .expect("clean candidate owners exactly once");
    assert!(receipt.complete);
    assert_eq!(receipt.owners_scheduled, 2);
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 1);
    assert_eq!(arena.payload(committed_id).unwrap(), b"committed");
    arena
        .release_later(committed)
        .expect("release committed root");
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

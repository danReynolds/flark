use flark_v3_runtime_slice::{ArenaBuildError, ArenaBuildLifecycle, PageArena};

fn settle(arena: &mut PageArena) {
    while arena.metrics().pending_releases != 0 {
        arena.poll_reclaim(64).expect("settle arena releases");
    }
}

#[test]
fn a_yielded_job_retains_no_arena_borrow_and_commits_one_owner() {
    let mut arena = PageArena::new();
    let ticket = arena.begin_build().expect("begin build");
    let build = ticket.id();

    let mut session = arena.resume_build(ticket).expect("resume first slice");
    let (leaf, _) = session.allocate(b"leaf", &[]).expect("allocate leaf");
    let leaf_id = session.owner_id(&leaf).expect("leaf ID");
    let ticket = session.suspend().expect("suspend first slice");

    // The parser job now owns only its ticket and linear owner handles. The
    // arena is independently available for queries and reclaim coordination.
    assert_eq!(arena.payload(leaf_id).unwrap(), b"leaf");
    assert_eq!(
        arena.build_lifecycle(build).unwrap(),
        ArenaBuildLifecycle::Suspended
    );

    let mut session = arena.resume_build(ticket).expect("resume second slice");
    let (root, _) = session
        .allocate(b"root", &[leaf_id])
        .expect("allocate root");
    let root_id = session.owner_id(&root).expect("root ID");
    session.release(leaf).expect("transfer leaf owner");
    let root_owner = session.commit(root).expect("commit sole root owner");

    assert!(matches!(
        arena.build_lifecycle(build),
        Err(ArenaBuildError::StaleBuild(id)) if id == build
    ));
    arena
        .poll_reclaim(1)
        .expect("release transferred leaf owner");
    assert_eq!(arena.payload(root_id).unwrap(), b"root");
    assert_eq!(arena.children(root_id).unwrap()[0], Some(leaf_id));

    arena
        .release_later(root_owner)
        .expect("release committed root");
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn session_drop_is_constant_time_and_abort_is_strictly_fuelled() {
    const OWNERS: usize = 1_000;
    let mut arena = PageArena::new();
    let ticket = arena.begin_build().expect("begin build");
    let build = ticket.id();
    {
        let mut session = arena.resume_build(ticket).expect("resume build");
        for index in 0..OWNERS {
            let payload = u64::try_from(index).unwrap().to_le_bytes();
            let _owner = session.allocate(&payload, &[]).expect("allocate owner").0;
        }
        assert_eq!(session.live_owners().unwrap(), OWNERS);
        // Deliberately drop without suspend/commit/begin_abort.
    }

    assert_eq!(
        arena.build_lifecycle(build).unwrap(),
        ArenaBuildLifecycle::Aborting
    );
    assert_eq!(arena.metrics().pending_releases, 0);
    assert_eq!(arena.metrics().live_nodes, OWNERS);

    let zero = arena.poll_build_abort(build, 0).expect("zero-fuel poll");
    assert_eq!(zero.owners_scheduled, 0);
    assert_eq!(zero.owners_remaining, OWNERS);
    assert!(!zero.complete);

    let first = arena.poll_build_abort(build, 7).expect("fuelled poll");
    assert_eq!(first.owners_scheduled, 7);
    assert_eq!(first.owners_remaining, OWNERS - 7);
    assert!(!first.complete);
    assert_eq!(arena.metrics().pending_releases, 7);
    assert_eq!(arena.metrics().live_nodes, OWNERS);

    let second = arena.poll_build_abort(build, OWNERS).expect("finish abort");
    assert_eq!(second.owners_scheduled, OWNERS - 7);
    assert_eq!(second.owners_remaining, 0);
    assert!(second.complete);
    assert_eq!(arena.metrics().pending_releases, OWNERS);

    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn explicit_abort_starts_without_cleanup_and_sparse_holes_hide_no_scan() {
    let mut arena = PageArena::new();
    let ticket = arena.begin_build().expect("begin build");
    let mut session = arena.resume_build(ticket).expect("resume build");
    let mut owners = Vec::new();
    for index in 0_u32..128 {
        owners.push(
            session
                .allocate(&index.to_le_bytes(), &[])
                .expect("allocate owner")
                .0,
        );
    }
    // Leave live owners scattered across a journal full of transferred holes.
    for owner in owners.drain(..120) {
        session.release(owner).expect("release owner");
    }
    let ticket = session.suspend().expect("suspend build");
    let journal = arena
        .build_journal_metrics(ticket.id())
        .expect("journal metrics");
    assert_eq!(journal.live_owners, 8);
    assert_eq!(journal.maximum_live_owners, 128);
    assert_eq!(journal.slots, 128);
    let pending_before = arena.metrics().pending_releases;
    let build = arena
        .begin_build_abort(ticket)
        .expect("constant-time begin abort");
    assert_eq!(arena.metrics().pending_releases, pending_before);

    for remaining in (1..=8).rev() {
        let receipt = arena.poll_build_abort(build, 1).expect("one owner poll");
        assert_eq!(receipt.owners_scheduled, 1);
        assert_eq!(receipt.owners_remaining, remaining - 1);
        assert_eq!(receipt.complete, remaining == 1);
    }
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn wrong_arena_returns_ticket_and_owner_handles_are_build_bound() {
    let mut first_arena = PageArena::new();
    let mut second_arena = PageArena::new();
    let first_ticket = first_arena.begin_build().expect("first build");
    let error = second_arena
        .resume_build(first_ticket)
        .expect_err("wrong arena must reject ticket");
    assert!(matches!(error.error, ArenaBuildError::WrongArena(_)));
    let first_ticket = error.ticket;

    let mut first_session = first_arena
        .resume_build(first_ticket)
        .expect("ticket remains usable in original arena");
    let (first_owner, _) = first_session.allocate(b"first", &[]).expect("first owner");
    let first_ticket = first_session.suspend().expect("suspend first");

    let second_ticket = first_arena.begin_build().expect("second build");
    let second_build = second_ticket.id();
    let second_session = first_arena
        .resume_build(second_ticket)
        .expect("resume second build");
    assert!(matches!(
        second_session.owner_id(&first_owner),
        Err(ArenaBuildError::CrossBuildOwner { expected, actual })
            if expected == second_build && actual == first_ticket.id()
    ));
    let second_build = second_session.begin_abort().expect("abort second");
    assert!(
        first_arena
            .poll_build_abort(second_build, 0)
            .unwrap()
            .complete
    );

    let first_session = first_arena
        .resume_build(first_ticket)
        .expect("resume first build");
    let root = first_session
        .commit(first_owner)
        .expect("commit first owner");
    first_arena.release_later(root).expect("release root");
    settle(&mut first_arena);
}

#[test]
fn completed_generation_rejects_stale_build_and_old_owner_capability() {
    let mut arena = PageArena::new();
    let ticket = arena.begin_build().expect("old build");
    let old_build = ticket.id();
    let mut session = arena.resume_build(ticket).expect("resume old build");
    let (old_owner, _) = session.allocate(b"old", &[]).expect("old owner");
    let ticket = session.suspend().expect("suspend old build");
    arena.begin_build_abort(ticket).expect("abort old build");
    assert!(arena.poll_build_abort(old_build, 1).unwrap().complete);

    assert!(matches!(
        arena.build_lifecycle(old_build),
        Err(ArenaBuildError::StaleBuild(id)) if id == old_build
    ));
    let ticket = arena.begin_build().expect("reused build slot");
    let new_build = ticket.id();
    assert_ne!(new_build, old_build);
    assert_eq!(
        arena
            .build_journal_metrics(new_build)
            .unwrap()
            .maximum_live_owners,
        0
    );
    let session = arena.resume_build(ticket).expect("resume new build");
    assert!(matches!(
        session.owner_id(&old_owner),
        Err(ArenaBuildError::CrossBuildOwner { expected, actual })
            if expected == new_build && actual == old_build
    ));
    let new_build = session.begin_abort().expect("abort new build");
    assert!(arena.poll_build_abort(new_build, 0).unwrap().complete);
    settle(&mut arena);
}

#[test]
fn commit_rejects_more_than_one_owner_and_drop_defers_all_cleanup() {
    let mut arena = PageArena::new();
    let ticket = arena.begin_build().expect("begin build");
    let build = ticket.id();
    let mut session = arena.resume_build(ticket).expect("resume build");
    let (candidate, _) = session.allocate(b"candidate", &[]).expect("candidate");
    let (_extra, _) = session.allocate(b"extra", &[]).expect("extra");
    let error = session
        .commit(candidate)
        .expect_err("two-owner commit must fail");
    assert_eq!(
        error,
        ArenaBuildError::ExpectedExactlyOneOwner { build, actual: 2 }
    );
    assert_eq!(arena.metrics().pending_releases, 0);
    assert_eq!(
        arena.build_lifecycle(build).unwrap(),
        ArenaBuildLifecycle::Aborting
    );
    let receipt = arena.poll_build_abort(build, 1).expect("partial abort");
    assert_eq!(receipt.owners_scheduled, 1);
    assert_eq!(receipt.owners_remaining, 1);
    assert!(!receipt.complete);
    let receipt = arena.poll_build_abort(build, 1).expect("finish abort");
    assert!(receipt.complete);
    settle(&mut arena);
}

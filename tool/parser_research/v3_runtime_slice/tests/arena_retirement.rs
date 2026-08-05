use flark_v3_runtime_slice::{ARENA_PAGE_BYTES, ArenaError, PageArena};

#[test]
fn stale_ids_cannot_alias_reused_slots() {
    let mut arena = PageArena::new();
    let old_owner = arena.allocate(b"old", &[]).expect("allocate old").owner;
    let old = old_owner.id();
    arena.release_later(old_owner).expect("release old");
    assert_eq!(arena.poll_reclaim(1).expect("reclaim").nodes_reclaimed, 1);

    let new_owner = arena.allocate(b"new", &[]).expect("reuse slot").owner;
    let new = new_owner.id();
    assert_eq!(old.index, new.index);
    assert_ne!(old.generation, new.generation);
    assert_eq!(arena.payload(old), Err(ArenaError::StaleId(old)));
    assert_eq!(arena.payload(new).expect("new is live"), b"new");
}

#[test]
fn deep_retirement_is_iterative_and_strictly_fuelled() {
    const DEPTH: usize = 20_000;
    const FUEL: usize = 7;

    let mut arena = PageArena::new();
    let mut root = arena.allocate(b"x", &[]).expect("leaf").owner;
    for _ in 1..DEPTH {
        let parent = arena.allocate(b"x", &[root.id()]).expect("parent").owner;
        arena.release_later(root).expect("transfer child ownership");
        let receipt = arena.poll_reclaim(1).expect("consume ownership release");
        assert_eq!(receipt.reference_transitions, 1);
        assert_eq!(receipt.nodes_reclaimed, 0);
        root = parent;
    }
    assert_eq!(arena.metrics().live_nodes, DEPTH);
    arena.release_later(root).expect("release chain root");

    let mut transitions = 0;
    while arena.metrics().pending_releases > 0 {
        let receipt = arena.poll_reclaim(FUEL).expect("fuelled reclaim");
        assert!(receipt.reference_transitions <= FUEL);
        assert!(receipt.nodes_reclaimed <= FUEL);
        assert!(receipt.payload_bytes_reclaimed <= FUEL * ARENA_PAGE_BYTES);
        transitions += receipt.reference_transitions;
    }
    assert_eq!(transitions, DEPTH);
    assert_eq!(arena.metrics().live_nodes, 0);
    assert_eq!(arena.metrics().live_payload_bytes, 0);
}

#[test]
fn shared_and_duplicate_child_edges_release_exactly_once_each() {
    let mut arena = PageArena::new();
    let child = arena.allocate(b"child", &[]).expect("child").owner;
    let child_id = child.id();
    let left = arena.allocate(b"left", &[child_id]).expect("left").owner;
    let right = arena
        .allocate(b"right", &[child_id, child_id])
        .expect("duplicate edges")
        .owner;
    arena
        .release_later(child)
        .expect("release caller child ref");
    arena.poll_reclaim(1).expect("leave three edge refs");

    arena.release_later(left).expect("release left");
    arena.poll_reclaim(2).expect("left and its child edge");
    assert!(arena.contains(child_id));
    arena.release_later(right).expect("release right");
    let receipt = arena.poll_reclaim(3).expect("right and duplicate edges");
    assert_eq!(receipt.nodes_reclaimed, 2);
    assert!(!arena.contains(child_id));
}

#[test]
fn caller_ownership_payload_cap_and_slot_reuse_are_enforced() {
    let mut arena = PageArena::new();
    assert_eq!(
        arena.allocate(&vec![0; ARENA_PAGE_BYTES + 1], &[]),
        Err(ArenaError::PayloadTooLarge(ARENA_PAGE_BYTES + 1))
    );

    for _ in 0..1_000 {
        let root = arena.allocate(b"bounded", &[]).expect("allocate").owner;
        arena.release_later(root).expect("first release");
        arena.poll_reclaim(1).expect("reclaim root");
    }
    let metrics = arena.metrics();
    assert_eq!(metrics.slots, 1);
    assert_eq!(metrics.live_nodes, 0);
    assert_eq!(metrics.reusable_slots, 1);
}

#[test]
fn retained_owners_are_distinct_transfers_while_query_ids_are_copyable() {
    let mut arena = PageArena::new();
    let first = arena.allocate(b"shared", &[]).expect("allocate").owner;
    let id = first.id();
    let second = arena.retain(id).expect("retain distinct owner");

    arena.release_later(first).expect("release first owner");
    arena.poll_reclaim(1).expect("consume first release");
    assert!(arena.contains(id));
    assert_eq!(arena.payload(id).expect("second owner remains"), b"shared");

    arena.release_later(second).expect("release second owner");
    arena.poll_reclaim(1).expect("consume second release");
    assert_eq!(arena.payload(id), Err(ArenaError::StaleId(id)));
}

#[test]
fn external_owners_and_scoped_ids_reject_real_two_arena_slot_collisions() {
    let mut first = PageArena::new();
    let mut second = PageArena::new();
    let first_owner = first.allocate(b"first", &[]).expect("first root").owner;
    let second_owner = second.allocate(b"second", &[]).expect("second root").owner;

    // Fresh arenas intentionally collide in their compact local edge space.
    assert_eq!(first_owner.id(), second_owner.id());
    assert_ne!(first.identity(), second.identity());

    let first_scoped = first_owner.scoped_id();
    assert_eq!(first.local_id(first_scoped), Ok(first_owner.id()));
    assert!(matches!(
        second.local_id(first_scoped),
        Err(ArenaError::WrongArena { expected, actual })
            if expected == second.identity() && actual == first.identity()
    ));

    let first_id = first_owner.id();
    let error = second
        .release_later(first_owner)
        .expect_err("wrong arena must reject the linear owner");
    assert!(matches!(
        error.error,
        ArenaError::WrongArena { expected, actual }
            if expected == second.identity() && actual == first.identity()
    ));
    assert_eq!(
        first.payload(first_id).expect("owner remains live"),
        b"first"
    );
    assert_eq!(
        second
            .payload(second_owner.id())
            .expect("collision target remains live"),
        b"second"
    );

    first
        .release_later(error.owner)
        .expect("returned owner remains releasable in its original arena");
    first.poll_reclaim(1).expect("reclaim returned owner");
    assert!(!first.contains(first_id));
    second.release_later(second_owner).expect("release second");
    second.poll_reclaim(1).expect("reclaim second");
}

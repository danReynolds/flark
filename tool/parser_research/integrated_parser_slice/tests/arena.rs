use flark_integrated_parser_slice::arena::{
    ArenaError, ArenaId, PageArena, ARENA_PAGE_BYTES, ARENA_SLAB_SLOTS, MAX_RECLAIM_FRONTIER,
};

fn release_fully(arena: &mut PageArena, id: ArenaId, fuel: usize) -> usize {
    arena.release_later(id).unwrap();
    let mut polls = 0;
    while arena.metrics().pending_releases > 0 {
        let receipt = arena.poll_reclaim(fuel).unwrap();
        assert!(receipt.reference_transitions <= fuel);
        polls += 1;
    }
    polls
}

#[test]
fn stale_ids_fail_closed_after_slot_reuse() {
    let mut arena = PageArena::new();
    let first = arena.allocate(b"first", &[]).unwrap().id;
    release_fully(&mut arena, first, 1);
    assert_eq!(arena.payload(first), Err(ArenaError::StaleId(first)));

    let second = arena.allocate(b"second", &[]).unwrap().id;
    assert_eq!(second.index, first.index);
    assert_ne!(second.generation, first.generation);
    assert_eq!(arena.payload(second).unwrap(), b"second");
}

#[test]
fn fixed_page_adoption_transfers_payload_without_copying_or_reallocating_it() {
    let mut arena = PageArena::new();
    let child = arena.allocate(b"anchor", &[]).unwrap().id;
    let mut allocation = Box::new([0_u8; ARENA_PAGE_BYTES]);
    allocation[..7].copy_from_slice(b"payload");
    let allocation_address = allocation.as_ptr();
    let preview = arena
        .preview_adopt_owned_page_transferring_owned_children(7, &[child])
        .unwrap();
    assert_eq!(preview.payload_bytes_copied, 0);
    assert_eq!(preview.payload_allocation_bytes_adopted, ARENA_PAGE_BYTES);

    let receipt = arena
        .adopt_owned_page_transferring_owned_children_preflighted(preview, allocation, 7, &[child])
        .unwrap();
    assert_eq!(receipt.payload_bytes_copied, 0);
    assert_eq!(receipt.payload_allocation_bytes_adopted, ARENA_PAGE_BYTES);
    assert_eq!(arena.payload(receipt.id).unwrap(), b"payload");
    assert_eq!(
        arena.payload(receipt.id).unwrap().as_ptr(),
        allocation_address
    );
    assert_eq!(arena.metrics().live_payload_bytes, ARENA_PAGE_BYTES + 6);

    arena.release_later(receipt.id).unwrap();
    let mut reclaimed_payload = 0;
    while arena.metrics().pending_releases != 0 {
        reclaimed_payload += arena.poll_reclaim(1).unwrap().payload_bytes_reclaimed;
    }
    assert_eq!(reclaimed_payload, ARENA_PAGE_BYTES + 6);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn shared_children_are_reclaimed_only_after_every_parent_edge() {
    let mut arena = PageArena::new();
    let leaf = arena.allocate(b"leaf", &[]).unwrap().id;
    let left = arena.allocate(b"left", &[leaf]).unwrap().id;
    let right = arena.allocate(b"right", &[leaf]).unwrap().id;
    // Transfer the caller's leaf handle; two parent edges remain.
    release_fully(&mut arena, leaf, 1);
    assert_eq!(arena.payload(leaf).unwrap(), b"leaf");

    release_fully(&mut arena, left, 1);
    assert_eq!(arena.payload(leaf).unwrap(), b"leaf");
    release_fully(&mut arena, right, 1);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn deep_chain_reclamation_is_iterative_and_one_transition_per_poll() {
    const NODES: usize = 100_000;
    let mut arena = PageArena::new();
    let mut root = arena.allocate(&[], &[]).unwrap().id;
    for _ in 1..NODES {
        let parent = arena.allocate(&[], &[root]).unwrap().id;
        arena.release_later(root).unwrap();
        let transfer = arena.poll_reclaim(1).unwrap();
        assert_eq!(transfer.reference_transitions, 1);
        assert_eq!(transfer.nodes_reclaimed, 0);
        root = parent;
    }
    assert_eq!(arena.metrics().live_nodes, NODES);

    arena.release_later(root).unwrap();
    let mut reclaimed = 0;
    while arena.metrics().pending_releases > 0 {
        let receipt = arena.poll_reclaim(1).unwrap();
        assert_eq!(receipt.reference_transitions, 1);
        assert!(receipt.nodes_reclaimed <= 1);
        assert!(receipt.payload_bytes_reclaimed <= ARENA_PAGE_BYTES);
        reclaimed += receipt.nodes_reclaimed;
    }
    assert_eq!(reclaimed, NODES);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn balanced_tree_reclamation_stays_within_fixed_frontier_and_fuel() {
    const LEVELS: usize = 17;
    let mut arena = PageArena::new();
    let mut level = (0..(1usize << LEVELS))
        .map(|_| arena.allocate(&[1], &[]).unwrap().id)
        .collect::<Vec<_>>();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len() / 2);
        for pair in level.chunks_exact(2) {
            let parent = arena.allocate(&[2], pair).unwrap().id;
            for &child in pair {
                arena.release_later(child).unwrap();
            }
            let receipt = arena.poll_reclaim(2).unwrap();
            assert_eq!(receipt.nodes_reclaimed, 0);
            next.push(parent);
        }
        level = next;
    }
    let root = level[0];
    let expected = (1usize << (LEVELS + 1)) - 1;
    assert_eq!(arena.metrics().live_nodes, expected);

    arena.release_later(root).unwrap();
    let mut reclaimed = 0;
    while arena.metrics().pending_releases > 0 {
        let receipt = arena.poll_reclaim(31).unwrap();
        assert!(receipt.reference_transitions <= 31);
        assert_eq!(receipt.pending_after, arena.metrics().pending_releases);
        assert!(arena.metrics().queued_release_nodes <= arena.metrics().live_nodes);
        reclaimed += receipt.nodes_reclaimed;
    }
    assert_eq!(reclaimed, expected);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn reclaimed_slabs_are_reused_and_high_water_plateaus() {
    const NODES: usize = 10_000;
    let mut arena = PageArena::new();
    let first = (0..NODES)
        .map(|_| arena.allocate(&[7; 32], &[]).unwrap().id)
        .collect::<Vec<_>>();
    for chunk in first.chunks(MAX_RECLAIM_FRONTIER) {
        for &id in chunk {
            arena.release_later(id).unwrap();
        }
        while arena.metrics().pending_releases > 0 {
            arena.poll_reclaim(47).unwrap();
        }
    }
    let plateau = arena.metrics();
    assert_eq!(plateau.live_nodes, 0);

    let second = (0..NODES)
        .map(|_| arena.allocate(&[9; 32], &[]).unwrap().id)
        .collect::<Vec<_>>();
    let reused = arena.metrics();
    assert_eq!(reused.slabs, plateau.slabs);
    assert_eq!(reused.high_water_live_nodes, NODES);
    assert_eq!(reused.high_water_payload_bytes, NODES * 32);
    for chunk in second.chunks(MAX_RECLAIM_FRONTIER) {
        for &id in chunk {
            arena.release_later(id).unwrap();
        }
        while arena.metrics().pending_releases > 0 {
            arena.poll_reclaim(47).unwrap();
        }
    }
}

#[test]
fn double_release_is_rejected_before_queue_mutation() {
    let mut arena = PageArena::new();
    let id = arena.allocate(b"owned", &[]).unwrap().id;
    arena.release_later(id).unwrap();
    let before = arena.metrics();

    assert_eq!(
        arena.release_later(id),
        Err(ArenaError::NoOwnedReference(id))
    );
    assert_eq!(arena.metrics(), before);
    let receipt = arena.poll_reclaim(1).unwrap();
    assert_eq!(receipt.reference_transitions, 1);
    assert_eq!(receipt.nodes_reclaimed, 1);
    assert_eq!(receipt.pending_after, 0);
}

#[test]
fn legitimate_duplicate_child_edges_release_exactly_twice() {
    let mut arena = PageArena::new();
    let child = arena.allocate(b"child", &[]).unwrap().id;
    let parent = arena.allocate(b"parent", &[child, child]).unwrap().id;
    arena.release_later(child).unwrap();
    arena.release_later(parent).unwrap();

    let receipt = arena.poll_reclaim(4).unwrap();
    assert_eq!(receipt.reference_transitions, 4);
    assert_eq!(receipt.child_releases_enqueued, 2);
    assert_eq!(receipt.nodes_reclaimed, 2);
    assert_eq!(receipt.pending_after, 0);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn full_frontier_binary_parent_cannot_wedge_intrusive_queue() {
    let mut arena = PageArena::new();
    let left = arena.allocate(b"left", &[]).unwrap().id;
    let right = arena.allocate(b"right", &[]).unwrap().id;
    let parent = arena.allocate(b"parent", &[left, right]).unwrap().id;
    let fillers = (0..MAX_RECLAIM_FRONTIER)
        .map(|_| arena.allocate(&[], &[]).unwrap().id)
        .collect::<Vec<_>>();

    // Put the binary parent at the head, then exceed the historical fixed
    // frontier width. Its two child transitions must remain admissible.
    arena.release_later(parent).unwrap();
    for &filler in &fillers {
        arena.release_later(filler).unwrap();
    }
    assert_eq!(arena.metrics().pending_releases, MAX_RECLAIM_FRONTIER + 1);

    let first = arena.poll_reclaim(1).unwrap();
    assert_eq!(first.nodes_reclaimed, 1);
    assert_eq!(first.child_releases_enqueued, 2);
    assert_eq!(first.pending_after, MAX_RECLAIM_FRONTIER + 2);
    while arena.metrics().pending_releases > 0 {
        arena.poll_reclaim(31).unwrap();
    }
    // Parent edges are gone; caller-owned child handles remain.
    assert_eq!(arena.metrics().live_nodes, 2);
    arena.release_later(left).unwrap();
    arena.release_later(right).unwrap();
    arena.poll_reclaim(2).unwrap();
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn hot_slot_generation_exhaustion_retires_instead_of_aliasing() {
    let mut arena = PageArena::with_generation_ceiling_for_tests(2);
    let first = arena.allocate(b"first", &[]).unwrap().id;
    arena.release_later(first).unwrap();
    arena.poll_reclaim(1).unwrap();

    let hot = arena.allocate(b"hot", &[]).unwrap().id;
    assert_eq!(hot.index, first.index);
    assert_eq!(hot.generation, 2);
    arena.release_later(hot).unwrap();
    let failure = arena.poll_reclaim(1).unwrap_err();
    assert_eq!(failure.error, ArenaError::GenerationExhausted(hot));
    assert_eq!(failure.receipt.reference_transitions, 1);
    assert_eq!(failure.receipt.nodes_reclaimed, 1);
    assert_eq!(failure.receipt.slots_retired, 1);
    assert_eq!(failure.receipt.pending_after, 0);

    let replacement = arena.allocate(b"replacement", &[]).unwrap().id;
    assert_ne!(replacement.index, hot.index);
    assert_eq!(arena.payload(hot), Err(ArenaError::StaleId(hot)));
    assert_eq!(arena.metrics().retired_slots, 1);
}

#[test]
fn partial_progress_error_preserves_complete_receipt() {
    let mut arena = PageArena::with_generation_ceiling_for_tests(2);
    let first = arena.allocate(&[], &[]).unwrap().id;
    arena.release_later(first).unwrap();
    arena.poll_reclaim(1).unwrap();
    let exhausted_next = arena.allocate(b"exhaust", &[]).unwrap().id;
    let ordinary = arena.allocate(b"ordinary", &[]).unwrap().id;

    // FIFO order performs one ordinary reclaim before hitting exhaustion.
    arena.release_later(ordinary).unwrap();
    arena.release_later(exhausted_next).unwrap();
    let failure = arena.poll_reclaim(2).unwrap_err();
    assert_eq!(
        failure.error,
        ArenaError::GenerationExhausted(exhausted_next)
    );
    assert_eq!(failure.receipt.reference_transitions, 2);
    assert_eq!(failure.receipt.nodes_reclaimed, 2);
    assert_eq!(failure.receipt.payload_bytes_reclaimed, 15);
    assert_eq!(failure.receipt.slots_retired, 1);
    assert_eq!(failure.receipt.pending_after, 0);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn payload_and_many_legitimate_releases_are_accounted() {
    let mut arena = PageArena::new();
    assert_eq!(
        arena.allocate(&vec![0; ARENA_PAGE_BYTES + 1], &[]),
        Err(ArenaError::PayloadTooLarge(ARENA_PAGE_BYTES + 1))
    );
    let id = arena.allocate(&[], &[]).unwrap().id;
    for _ in 0..MAX_RECLAIM_FRONTIER {
        arena.retain(id).unwrap();
        arena.release_later(id).unwrap();
    }
    assert_eq!(arena.metrics().queued_release_nodes, 1);
    assert_eq!(arena.metrics().pending_releases, MAX_RECLAIM_FRONTIER);
    arena.release_later(id).unwrap();
    assert_eq!(
        arena.release_later(id),
        Err(ArenaError::NoOwnedReference(id))
    );
    let receipt = arena.poll_reclaim(MAX_RECLAIM_FRONTIER + 1).unwrap();
    assert_eq!(receipt.reference_transitions, MAX_RECLAIM_FRONTIER + 1);
    assert_eq!(receipt.nodes_reclaimed, 1);
    assert_eq!(receipt.pending_after, 0);
}

#[test]
fn transferred_child_ownership_builds_without_housekeeping_reclaim_work() {
    let mut arena = PageArena::new();
    let mut root = arena.allocate(b"leaf", &[]).unwrap().id;
    for ordinal in 0..4_096_u64 {
        let receipt = arena
            .allocate_transferring_owned_children(&ordinal.to_le_bytes(), &[root])
            .unwrap();
        assert_eq!(receipt.child_references_added, 0);
        assert_eq!(receipt.child_owned_references_transferred, 1);
        root = receipt.id;
    }
    assert_eq!(arena.metrics().pending_releases, 0);
    assert_eq!(arena.metrics().live_nodes, 4_097);

    arena.release_later(root).unwrap();
    while arena.metrics().pending_releases > 0 {
        let receipt = arena.poll_reclaim(17).unwrap();
        assert!(receipt.reference_transitions <= 17);
    }
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn duplicate_transferred_edges_consume_two_real_owned_references() {
    let mut arena = PageArena::new();
    let child = arena.allocate(b"child", &[]).unwrap().id;
    arena.retain(child).unwrap();
    let parent = arena
        .allocate_transferring_owned_children(b"parent", &[child, child])
        .unwrap();
    assert_eq!(parent.child_owned_references_transferred, 2);
    assert_eq!(
        arena.release_later(child),
        Err(ArenaError::NoOwnedReference(child))
    );

    arena.release_later(parent.id).unwrap();
    let receipt = arena.poll_reclaim(3).unwrap();
    assert_eq!(receipt.nodes_reclaimed, 2);
    assert_eq!(receipt.child_releases_enqueued, 2);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn rejected_transfer_is_atomic_when_duplicate_ownership_is_insufficient() {
    let mut arena = PageArena::new();
    let child = arena.allocate(b"child", &[]).unwrap().id;
    let before = arena.metrics();
    assert_eq!(
        arena.allocate_transferring_owned_children(b"parent", &[child, child]),
        Err(ArenaError::NoOwnedReference(child))
    );
    assert_eq!(arena.metrics(), before);
    assert_eq!(arena.payload(child).unwrap(), b"child");
    arena.release_later(child).unwrap();
    arena.poll_reclaim(1).unwrap();
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn unlinked_anchor_rollback_does_not_join_an_older_reclaim_fifo() {
    let mut arena = PageArena::new();
    let leaf = arena.allocate(b"old leaf", &[]).unwrap().id;
    let old_root = arena
        .allocate_transferring_owned_children(b"old root", &[leaf])
        .unwrap()
        .id;
    arena.release_later(old_root).unwrap();
    assert_eq!(arena.metrics().pending_releases, 1);

    let anchor = arena.allocate(b"new unlinked anchor", &[]).unwrap().id;
    let pending_before = arena.metrics().pending_releases;
    let rollback = arena.discard_unlinked_owned(anchor).unwrap();
    assert_eq!(rollback.nodes_reclaimed, 1);
    assert_eq!(rollback.payload_bytes_reclaimed, 19);
    assert_eq!(rollback.pending_after, pending_before);
    assert_eq!(arena.metrics().pending_releases, pending_before);

    while arena.metrics().pending_releases > 0 {
        arena.poll_reclaim(1).unwrap();
    }
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn immediate_rollback_rejects_linked_or_shared_nodes_without_mutation() {
    let mut arena = PageArena::new();
    let child = arena.allocate(b"child", &[]).unwrap().id;
    let parent = arena.allocate(b"parent", &[child]).unwrap().id;
    let before = arena.metrics();
    assert_eq!(
        arena.discard_unlinked_owned(parent),
        Err(ArenaError::InvalidReferenceState(parent))
    );
    assert_eq!(arena.metrics(), before);
    assert_eq!(arena.payload(parent).unwrap(), b"parent");

    arena.release_later(parent).unwrap();
    arena.release_later(child).unwrap();
    while arena.metrics().pending_releases > 0 {
        arena.poll_reclaim(3).unwrap();
    }
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn slab_and_directory_growth_preflight_exactly_matches_the_transition() {
    let mut arena = PageArena::new();
    let first_preview = arena.preview_allocate(1, &[]).unwrap();
    assert_eq!(first_preview.slabs_added, 1);
    assert_eq!(first_preview.slots_initialized, ARENA_SLAB_SLOTS);
    assert_eq!(first_preview.directory_blocks_added, 2);
    assert_eq!(
        first_preview.directory_entries_initialized,
        2 * ARENA_SLAB_SLOTS
    );
    assert!(first_preview.slot_bytes_initialized > 0);
    assert!(first_preview.directory_bytes_initialized > 0);
    let first = arena.allocate(b"a", &[]).unwrap();
    assert_eq!(first.preview(), first_preview);

    for _ in 1..ARENA_SLAB_SLOTS {
        let preview = arena.preview_allocate(0, &[]).unwrap();
        assert_eq!(preview.slabs_added, 0);
        let actual = arena.allocate(&[], &[]).unwrap();
        assert_eq!(actual.preview(), preview);
    }
    assert_eq!(arena.metrics().slabs, 1);

    let boundary_preview = arena.preview_allocate(7, &[]).unwrap();
    assert_eq!(boundary_preview.slabs_added, 1);
    assert_eq!(boundary_preview.slots_initialized, ARENA_SLAB_SLOTS);
    assert_eq!(boundary_preview.directory_blocks_added, 0);
    assert_eq!(boundary_preview.directory_entries_initialized, 0);
    let boundary = arena.allocate(b"1234567", &[]).unwrap();
    assert_eq!(boundary.preview(), boundary_preview);
    assert_eq!(arena.metrics().slabs, 2);
}

#[test]
fn bound_allocation_preview_rejects_intervening_mutation_before_work() {
    let mut arena = PageArena::new();
    let preview = arena.preview_allocate(4, &[]).unwrap();
    arena.allocate(b"intervening", &[]).unwrap();
    let before = arena.metrics();
    assert_eq!(
        arena.allocate_preflighted(preview, b"next", &[]),
        Err(ArenaError::StaleAllocationPreview)
    );
    assert_eq!(arena.metrics(), before);

    let child = arena.allocate(b"child", &[]).unwrap().id;
    let transfer = arena
        .preview_allocate_transferring_owned_children(0, &[child])
        .unwrap();
    assert_eq!(
        arena.allocate_preflighted(transfer, &[], &[child]),
        Err(ArenaError::StaleAllocationPreview)
    );
    assert_eq!(arena.metrics().live_nodes, before.live_nodes + 1);
}

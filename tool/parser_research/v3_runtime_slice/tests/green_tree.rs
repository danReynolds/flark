#![allow(clippy::too_many_lines)]

use flark_v3_runtime_slice::{
    ArenaError, ClosedChildAggregate, ForestBlockId, GreenContainerKind, GreenContainerSpec,
    GreenGapOwner, GreenItemParagraph, GreenMetric, GreenMutationReceipt, GreenSourcePart,
    GreenTree, PageArena, prove_three_external_children_are_unrepresentable,
};

fn metric(bytes: u64, utf16: u64) -> GreenMetric {
    GreenMetric { bytes, utf16 }
}

fn settle(arena: &mut PageArena) {
    while arena.metrics().pending_releases != 0 {
        arena.poll_reclaim(16_384).expect("settle green arena");
    }
}

fn item(index: u64) -> GreenItemParagraph {
    GreenItemParagraph {
        item: ForestBlockId(10 + index * 2),
        paragraph: ForestBlockId(11 + index * 2),
        marker: metric(2, 2),
        content: metric(10, 8),
        trailing_gap: metric(1, 1),
        trailing_gap_owner: if index.is_multiple_of(2) {
            GreenGapOwner::Item
        } else {
            GreenGapOwner::List
        },
        contribution: ClosedChildAggregate::default(),
    }
}

fn ordinary_containers() -> [GreenContainerSpec; 2] {
    [
        GreenContainerSpec {
            block: ForestBlockId(1),
            kind: GreenContainerKind::Document,
            prefix: GreenMetric::default(),
            suffix: GreenMetric::default(),
        },
        GreenContainerSpec {
            block: ForestBlockId(2),
            kind: GreenContainerKind::List,
            prefix: GreenMetric::default(),
            suffix: GreenMetric::default(),
        },
    ]
}

#[test]
fn hundred_thousand_item_microtrees_are_compact_and_local_but_block_lookup_is_linear() {
    const ITEMS: u64 = 100_000;
    const SEMANTIC_BLOCKS: usize = 200_002;
    let mut arena = PageArena::new();
    let mut build = GreenMutationReceipt::default();
    let tree = GreenTree::build_list(
        &mut arena,
        &ordinary_containers(),
        (0..ITEMS).map(item),
        &mut build,
    )
    .expect("packed hierarchical list");
    settle(&mut arena);
    let retained = arena.metrics();
    assert_eq!(tree.metric(&arena).unwrap(), metric(1_300_000, 1_100_000));
    assert!(tree.list_is_tight(&arena).unwrap());

    let first_marker = tree.source_lookup_bytes(&arena, 0).unwrap().unwrap();
    assert_eq!(first_marker.owner, ForestBlockId(10));
    assert_eq!(first_marker.part, GreenSourcePart::ItemMarker);
    assert_eq!(
        first_marker.enclosing,
        [ForestBlockId(1), ForestBlockId(2), ForestBlockId(10)]
    );
    let first_content = tree.source_lookup_bytes(&arena, 2).unwrap().unwrap();
    assert_eq!(first_content.owner, ForestBlockId(11));
    assert_eq!(first_content.part, GreenSourcePart::ParagraphContent);
    let first_gap = tree.source_lookup_bytes(&arena, 12).unwrap().unwrap();
    assert_eq!(first_gap.owner, ForestBlockId(10));
    assert_eq!(first_gap.part, GreenSourcePart::Gap);
    let second_gap = tree.source_lookup_bytes(&arena, 25).unwrap().unwrap();
    assert_eq!(second_gap.owner, ForestBlockId(2));
    assert_eq!(second_gap.enclosing, [ForestBlockId(1), ForestBlockId(2)]);
    let last_content = tree
        .source_lookup_utf16(&arena, 1_100_000 - 2)
        .unwrap()
        .unwrap();
    assert!(last_content.receipt.arena_nodes_visited < 32);
    assert!(last_content.receipt.packed_entries_examined <= 84);

    let pages = tree.child_page_count(&arena).unwrap();
    let far_page = tree.child_page_at(&arena, pages - 1).unwrap().unwrap();
    let mut mutation = GreenMutationReceipt::default();
    let changed_index = 50_001;
    let expected = item(changed_index);
    let replacement = GreenItemParagraph {
        content: metric(12, 9),
        contribution: ClosedChildAggregate {
            item_loose_if_nonlast: true,
            ..ClosedChildAggregate::default()
        },
        ..expected
    };
    let edited = tree
        .replace_item(
            &mut arena,
            changed_index,
            expected.item,
            replacement,
            &mut mutation,
        )
        .expect("one local item replacement");
    assert!(!edited.list_is_tight(&arena).unwrap());
    assert_eq!(edited.metric(&arena).unwrap(), metric(1_300_002, 1_100_001));
    assert_eq!(
        edited.child_page_at(&arena, pages - 1).unwrap(),
        Some(far_page)
    );
    assert!(mutation.suffix_pages_reused >= usize::try_from(pages - 1).unwrap());
    assert!(mutation.sequence_nodes_visited < 128);
    assert!(mutation.maximum_live_owner_handles < 32);

    let last_paragraph = item(ITEMS - 1).paragraph;
    let (found, linear) = edited.find_block_linear(&arena, last_paragraph).unwrap();
    assert!(found);
    assert_eq!(
        linear.packed_entries_examined,
        usize::try_from(ITEMS).unwrap()
    );
    assert!(linear.arena_nodes_visited > 2_000);

    let retained_accounted = retained.live_payload_bytes + retained.slot_storage_bytes;
    let hundredths = retained_accounted * 100 / SEMANTIC_BLOCKS;
    eprintln!(
        "green_tree_list items={ITEMS} semantic_blocks={SEMANTIC_BLOCKS} live_nodes={} payload_bytes={} slot_capacity={} slot_bytes={} accounted_retained={} bytes_per_block={}.{:02} bytes_per_source_segment={}.{:02} typed_page_buffer={} encoded_page_buffer={} stream_roots={} stream_bin_bytes={} live_owner_handles={} journal_capacity={} journal_bytes={} edit_nodes={} edit_payload={} reused_pages={} linear_block_nodes={} linear_entries={}",
        retained.live_nodes,
        retained.live_payload_bytes,
        retained.slot_capacity,
        retained.slot_storage_bytes,
        retained_accounted,
        hundredths / 100,
        hundredths % 100,
        retained_accounted * 100 / (usize::try_from(ITEMS).unwrap() * 3) / 100,
        retained_accounted * 100 / (usize::try_from(ITEMS).unwrap() * 3) % 100,
        build.maximum_typed_page_buffer_bytes,
        build.maximum_leaf_buffer_bytes,
        build.maximum_streaming_roots,
        build.maximum_streaming_bin_bytes,
        build.maximum_live_owner_handles,
        build.owner_journal_capacity,
        build.owner_journal_bytes,
        mutation.sequence_nodes_visited,
        mutation.payload_bytes_copied,
        mutation.suffix_pages_reused,
        linear.arena_nodes_visited,
        linear.packed_entries_examined,
    );
    assert!(hundredths / 100 < 30);

    tree.release_later(&mut arena).unwrap();
    edited.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn nested_quote_list_markers_and_gaps_have_exact_semantic_owners() {
    let containers = [
        GreenContainerSpec {
            block: ForestBlockId(1),
            kind: GreenContainerKind::Document,
            prefix: GreenMetric::default(),
            suffix: GreenMetric::default(),
        },
        GreenContainerSpec {
            block: ForestBlockId(2),
            kind: GreenContainerKind::BlockQuote,
            prefix: metric(2, 2),
            suffix: metric(1, 1),
        },
        GreenContainerSpec {
            block: ForestBlockId(3),
            kind: GreenContainerKind::List,
            prefix: GreenMetric::default(),
            suffix: GreenMetric::default(),
        },
    ];
    let entry = GreenItemParagraph {
        item: ForestBlockId(4),
        paragraph: ForestBlockId(5),
        marker: metric(2, 2),
        content: metric(3, 3),
        trailing_gap: metric(2, 2),
        trailing_gap_owner: GreenGapOwner::List,
        contribution: ClosedChildAggregate::default(),
    };
    let mut arena = PageArena::new();
    let tree = GreenTree::build_list(
        &mut arena,
        &containers,
        [entry],
        &mut GreenMutationReceipt::default(),
    )
    .unwrap();
    assert_eq!(tree.metric(&arena).unwrap(), metric(10, 10));
    let quote_marker = tree.source_lookup_bytes(&arena, 0).unwrap().unwrap();
    assert_eq!(quote_marker.owner, ForestBlockId(2));
    assert_eq!(quote_marker.part, GreenSourcePart::ContainerMarker);
    assert_eq!(quote_marker.enclosing, [ForestBlockId(1), ForestBlockId(2)]);
    let item_marker = tree.source_lookup_bytes(&arena, 2).unwrap().unwrap();
    assert_eq!(item_marker.owner, ForestBlockId(4));
    let content = tree.source_lookup_bytes(&arena, 4).unwrap().unwrap();
    assert_eq!(content.owner, ForestBlockId(5));
    let list_gap = tree.source_lookup_bytes(&arena, 7).unwrap().unwrap();
    assert_eq!(list_gap.owner, ForestBlockId(3));
    assert_eq!(
        list_gap.enclosing,
        [ForestBlockId(1), ForestBlockId(2), ForestBlockId(3)]
    );
    let quote_gap = tree.source_lookup_bytes(&arena, 9).unwrap().unwrap();
    assert_eq!(quote_gap.owner, ForestBlockId(2));
    assert_eq!(quote_gap.enclosing, [ForestBlockId(1), ForestBlockId(2)]);
    tree.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

#[test]
fn packed_page_with_three_out_of_line_children_hits_the_arena_fanout_wall() {
    let mut arena = PageArena::new();
    let error = prove_three_external_children_are_unrepresentable(&mut arena).unwrap();
    assert_eq!(error, ArenaError::TooManyChildren(3));
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn malformed_item_after_flushed_pages_cancels_the_whole_build() {
    let mut arena = PageArena::new();
    let mut entries = (0..200).map(item).collect::<Vec<_>>();
    entries.push(GreenItemParagraph {
        item: ForestBlockId(0),
        ..item(201)
    });
    let error = GreenTree::build_list(
        &mut arena,
        &ordinary_containers(),
        entries,
        &mut GreenMutationReceipt::default(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        flark_v3_runtime_slice::GreenTreeError::Invalid("invalid item microtree identity")
    );
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
    assert_eq!(arena.metrics().live_payload_bytes, 0);
}

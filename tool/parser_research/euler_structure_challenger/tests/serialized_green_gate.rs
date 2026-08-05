use std::iter;

use euler_structure_challenger::serialized_green::{
    Affinity, CoverageAtom, CoveragePart, GreenError, GreenKind, GreenMutationReceipt,
    GreenProperty, GreenToken, PropertyTag, SerializedGreenSequence, SourceCoordinate,
};
use euler_structure_challenger::{BlockId, ClosedChildSummary, ContainerSemantics};

fn enter(id: u64, kind: GreenKind) -> GreenToken {
    GreenToken::enter(BlockId(id), kind, ClosedChildSummary::default())
}

fn coverage(bytes: u64, utf16: u64, owner_relative_depth: u32, part: CoveragePart) -> GreenToken {
    GreenToken::Coverage(CoverageAtom::new(bytes, utf16, owner_relative_depth, part).unwrap())
}

fn property(tag: PropertyTag, bytes: &[u8]) -> GreenToken {
    GreenToken::Property(GreenProperty::new(tag, bytes).unwrap())
}

#[test]
fn source_descent_recovers_owner_enclosing_path_affinity_and_hull() {
    // Logical source: quote marker, emoji line, continuation marker, second
    // content line, quote-owned gap, document-owned gap. The continuation
    // marker occurs while Paragraph is structurally open but belongs to Quote.
    let tokens = [
        enter(1, GreenKind::DOCUMENT),
        enter(2, GreenKind::BLOCK_QUOTE),
        coverage(2, 2, 0, CoveragePart::CONTAINER_MARKER),
        enter(3, GreenKind::PARAGRAPH),
        coverage(5, 3, 0, CoveragePart::CONTENT),
        coverage(2, 2, 1, CoveragePart::CONTAINER_MARKER),
        coverage(2, 2, 0, CoveragePart::CONTENT),
        GreenToken::Exit,
        coverage(1, 1, 0, CoveragePart::GAP),
        GreenToken::Exit,
        coverage(1, 1, 0, CoveragePart::GAP),
        GreenToken::Exit,
    ];
    let mut build = GreenMutationReceipt::default();
    let sequence = SerializedGreenSequence::from_tokens(tokens, &mut build).unwrap();
    assert_eq!(sequence.metric().bytes, 13);
    assert_eq!(sequence.metric().utf16, 11);

    let before_marker = sequence
        .source_lookup(SourceCoordinate::Bytes, 7, Affinity::Upstream)
        .unwrap();
    assert_eq!(before_marker.owner, BlockId(3));
    assert_eq!(before_marker.atom.part, CoveragePart::CONTENT);
    let marker = sequence
        .source_lookup(SourceCoordinate::Bytes, 7, Affinity::Downstream)
        .unwrap();
    assert_eq!(marker.owner, BlockId(2));
    assert_eq!(marker.enclosing, [BlockId(1), BlockId(2)]);
    assert_eq!(marker.open_path, [BlockId(1), BlockId(2), BlockId(3)]);
    assert_eq!(marker.atom.part, CoveragePart::CONTAINER_MARKER);
    assert!(marker.receipt.summary_nodes_skipped <= marker.receipt.tree_nodes_visited);

    let utf16_marker = sequence
        .source_lookup(SourceCoordinate::Utf16, 5, Affinity::Downstream)
        .unwrap();
    assert_eq!(utf16_marker.owner, BlockId(2));
    assert_eq!(utf16_marker.byte_range, 7..9);
    assert_eq!(utf16_marker.utf16_range, 5..7);

    let paragraph = sequence.block_span_from_hit(&marker, 2).unwrap();
    assert_eq!(paragraph.block, BlockId(3));
    assert_eq!(paragraph.byte_range, 2..11);
    assert_eq!(paragraph.utf16_range, 2..9);
    let quote = sequence.block_span_from_hit(&marker, 1).unwrap();
    assert_eq!(quote.byte_range, 0..12);
    assert_eq!(
        sequence.subtree_blocks(&quote).unwrap(),
        [BlockId(2), BlockId(3)]
    );

    let quote_gap = sequence
        .source_lookup(SourceCoordinate::Bytes, 12, Affinity::Upstream)
        .unwrap();
    let document_gap = sequence
        .source_lookup(SourceCoordinate::Bytes, 12, Affinity::Downstream)
        .unwrap();
    assert_eq!(quote_gap.owner, BlockId(2));
    assert_eq!(document_gap.owner, BlockId(1));
    let end_upstream = sequence
        .source_lookup(SourceCoordinate::Bytes, 13, Affinity::Upstream)
        .unwrap();
    let end_downstream = sequence
        .source_lookup(SourceCoordinate::Bytes, 13, Affinity::Downstream)
        .unwrap();
    assert_eq!(end_upstream.owner, BlockId(1));
    assert_eq!(end_downstream.owner, BlockId(1));
    assert_eq!(end_downstream.byte_range, 12..13);
}

#[test]
fn owner_depth_is_validated_against_the_exact_structural_stack() {
    let tokens = [
        enter(1, GreenKind::DOCUMENT),
        enter(2, GreenKind::PARAGRAPH),
        coverage(1, 1, 2, CoveragePart::CONTENT),
        GreenToken::Exit,
        GreenToken::Exit,
    ];
    let sequence =
        SerializedGreenSequence::from_tokens(tokens, &mut GreenMutationReceipt::default()).unwrap();
    assert_eq!(
        sequence.source_lookup(SourceCoordinate::Bytes, 0, Affinity::Downstream),
        Err(GreenError::Invalid(
            "coverage owner depth escapes open path"
        ))
    );
}

#[test]
fn shallow_witness_and_deep_fallback_return_the_same_exact_path() {
    for depth in [2_u64, 4, 5, 32] {
        let tokens = (0..depth)
            .map(|index| enter(index + 1, GreenKind::BLOCK_QUOTE))
            .chain(iter::once(coverage(
                1,
                1,
                0,
                CoveragePart::CONTAINER_MARKER,
            )))
            .chain((0..depth).map(|_| GreenToken::Exit));
        let sequence =
            SerializedGreenSequence::from_tokens(tokens, &mut GreenMutationReceipt::default())
                .unwrap();
        let full = sequence
            .source_lookup(SourceCoordinate::Bytes, 0, Affinity::Downstream)
            .unwrap();
        let owner = sequence
            .source_owner_lookup(SourceCoordinate::Bytes, 0, Affinity::Downstream)
            .unwrap();
        assert_eq!(owner.owner, full.owner);
        assert_eq!(owner.enclosing, full.enclosing);
        assert_eq!(owner.open_path, full.open_path);
        assert_eq!(owner.open_path.len(), usize::try_from(depth).unwrap());
        if depth <= 4 {
            assert!(owner.receipt.witness_fragments_used > 0);
        } else {
            assert!(owner.receipt.leaf_tokens_scanned >= usize::try_from(depth).unwrap());
        }
    }
}

#[test]
fn heterogeneous_structural_properties_are_adjacent_and_source_addressable() {
    let tokens = [
        enter(1, GreenKind::DOCUMENT),
        enter(2, GreenKind::HEADING),
        property(PropertyTag::HEADING, &[2]),
        coverage(3, 3, 0, CoveragePart::CONTENT),
        GreenToken::Exit,
        enter(3, GreenKind::LIST),
        property(PropertyTag::LIST, &[1, 1, b'.', 1]),
        enter(4, GreenKind::ITEM),
        property(PropertyTag::ITEM, &[0, 3]),
        enter(5, GreenKind::PARAGRAPH),
        coverage(3, 3, 0, CoveragePart::CONTENT),
        GreenToken::Exit,
        coverage(1, 1, 0, CoveragePart::GAP),
        GreenToken::Exit,
        GreenToken::Exit,
        enter(6, GreenKind::CODE_BLOCK),
        property(PropertyTag::FENCE, &[b'`', 3, 0, 0]),
        coverage(3, 3, 0, CoveragePart::BLOCK_MARKER),
        coverage(4, 4, 0, CoveragePart::CONTENT),
        GreenToken::Exit,
        enter(7, GreenKind::HTML_BLOCK),
        property(PropertyTag::HTML, &[6]),
        coverage(5, 5, 0, CoveragePart::CONTENT),
        GreenToken::Exit,
        enter(8, GreenKind::TABLE),
        property(PropertyTag::TABLE_ALIGNMENTS, &[1, 2, 3]),
        enter(9, GreenKind::TABLE_ROW),
        enter(10, GreenKind::TABLE_CELL),
        coverage(1, 1, 0, CoveragePart::CONTENT),
        GreenToken::Exit,
        enter(11, GreenKind::TABLE_CELL),
        coverage(1, 1, 0, CoveragePart::CONTENT),
        GreenToken::Exit,
        enter(12, GreenKind::TABLE_CELL),
        coverage(1, 1, 0, CoveragePart::CONTENT),
        GreenToken::Exit,
        GreenToken::Exit,
        GreenToken::Exit,
        GreenToken::Exit,
    ];
    let sequence =
        SerializedGreenSequence::from_tokens(tokens, &mut GreenMutationReceipt::default()).unwrap();
    let heading = sequence
        .source_lookup(SourceCoordinate::Bytes, 0, Affinity::Downstream)
        .unwrap();
    assert_eq!(
        sequence.properties_from_hit(&heading, 1).unwrap()[0].bytes(),
        [2]
    );
    let list_content = sequence
        .source_lookup(SourceCoordinate::Bytes, 3, Affinity::Downstream)
        .unwrap();
    assert_eq!(
        sequence.properties_from_hit(&list_content, 1).unwrap()[0].tag,
        PropertyTag::LIST
    );
    assert_eq!(
        sequence.properties_from_hit(&list_content, 2).unwrap()[0].tag,
        PropertyTag::ITEM
    );
    let fence = sequence
        .source_lookup(SourceCoordinate::Bytes, 7, Affinity::Downstream)
        .unwrap();
    assert_eq!(fence.owner, BlockId(6));
    assert_eq!(
        sequence.properties_from_hit(&fence, 1).unwrap()[0].bytes(),
        [b'`', 3, 0, 0]
    );
    let html = sequence
        .source_lookup(SourceCoordinate::Bytes, 14, Affinity::Downstream)
        .unwrap();
    assert_eq!(
        sequence.properties_from_hit(&html, 1).unwrap()[0].tag,
        PropertyTag::HTML
    );
    let table_cell = sequence
        .source_lookup(SourceCoordinate::Bytes, 19, Affinity::Downstream)
        .unwrap();
    assert_eq!(table_cell.owner, BlockId(10));
    assert_eq!(
        sequence.properties_from_hit(&table_cell, 1).unwrap()[0].bytes(),
        [1, 2, 3]
    );
    let stats = sequence.memory_stats();
    assert_eq!(stats.property_records, 6);
}

#[test]
fn nested_list_fold_change_propagates_by_source_routes_without_sibling_scan() {
    let tokens = [
        enter(1, GreenKind::DOCUMENT),
        enter(2, GreenKind::LIST),
        property(PropertyTag::LIST, &[0, b'-', 1, 1]),
        enter(3, GreenKind::ITEM),
        property(PropertyTag::ITEM, &[0, 2]),
        enter(4, GreenKind::LIST),
        property(PropertyTag::LIST, &[0, b'-', 1, 1]),
        enter(5, GreenKind::ITEM),
        property(PropertyTag::ITEM, &[0, 2]),
        enter(6, GreenKind::PARAGRAPH),
        coverage(1, 1, 0, CoveragePart::CONTENT),
        GreenToken::Exit,
        GreenToken::Exit,
        GreenToken::Exit,
        GreenToken::Exit,
        enter(7, GreenKind::ITEM),
        property(PropertyTag::ITEM, &[0, 2]),
        enter(8, GreenKind::PARAGRAPH),
        coverage(1, 1, 0, CoveragePart::CONTENT),
        GreenToken::Exit,
        GreenToken::Exit,
        GreenToken::Exit,
        GreenToken::Exit,
    ];
    let mut sequence =
        SerializedGreenSequence::from_tokens(tokens, &mut GreenMutationReceipt::default()).unwrap();
    let initial = sequence
        .source_lookup(SourceCoordinate::Bytes, 0, Affinity::Downstream)
        .unwrap();
    let initial_outer_list = sequence.block_span_from_hit(&initial, 1).unwrap();
    assert!(
        sequence
            .direct_child_summary(&initial_outer_list, &mut Default::default(),)
            .unwrap()
            .list_is_tight()
    );

    let item_semantics = ContainerSemantics {
        descends_through_last_child: true,
        is_item: true,
        last_line_blank: false,
    };
    let list_semantics = ContainerSemantics {
        descends_through_last_child: true,
        is_item: false,
        last_line_blank: false,
    };
    let mut mutation = GreenMutationReceipt::default();
    let mut hit = sequence
        .source_lookup(SourceCoordinate::Bytes, 0, Affinity::Downstream)
        .unwrap();
    sequence = sequence
        .replace_token(
            &hit.ancestors[5].enter,
            GreenToken::enter(
                BlockId(6),
                GreenKind::PARAGRAPH,
                ClosedChildSummary {
                    ends_blank: true,
                    ..ClosedChildSummary::default()
                },
            ),
            &mut mutation,
        )
        .unwrap();

    hit = sequence
        .source_lookup(SourceCoordinate::Bytes, 0, Affinity::Downstream)
        .unwrap();
    let inner_item_span = sequence.block_span_from_hit(&hit, 4).unwrap();
    let inner_item_children = sequence
        .direct_child_summary(&inner_item_span, &mut Default::default())
        .unwrap();
    let inner_item_closed = item_semantics.closed_summary(inner_item_children);
    sequence = sequence
        .replace_token(
            &hit.ancestors[4].enter,
            GreenToken::enter(BlockId(5), GreenKind::ITEM, inner_item_closed),
            &mut mutation,
        )
        .unwrap();

    hit = sequence
        .source_lookup(SourceCoordinate::Bytes, 0, Affinity::Downstream)
        .unwrap();
    let inner_list_span = sequence.block_span_from_hit(&hit, 3).unwrap();
    let inner_list_children = sequence
        .direct_child_summary(&inner_list_span, &mut Default::default())
        .unwrap();
    let inner_list_closed = list_semantics.closed_summary(inner_list_children);
    sequence = sequence
        .replace_token(
            &hit.ancestors[3].enter,
            GreenToken::enter(BlockId(4), GreenKind::LIST, inner_list_closed),
            &mut mutation,
        )
        .unwrap();

    hit = sequence
        .source_lookup(SourceCoordinate::Bytes, 0, Affinity::Downstream)
        .unwrap();
    let outer_item_span = sequence.block_span_from_hit(&hit, 2).unwrap();
    let outer_item_children = sequence
        .direct_child_summary(&outer_item_span, &mut Default::default())
        .unwrap();
    let outer_item_closed = item_semantics.closed_summary(outer_item_children);
    sequence = sequence
        .replace_token(
            &hit.ancestors[2].enter,
            GreenToken::enter(BlockId(3), GreenKind::ITEM, outer_item_closed),
            &mut mutation,
        )
        .unwrap();

    hit = sequence
        .source_lookup(SourceCoordinate::Bytes, 0, Affinity::Downstream)
        .unwrap();
    let outer_list_span = sequence.block_span_from_hit(&hit, 1).unwrap();
    let final_fold = sequence
        .direct_child_summary(&outer_list_span, &mut Default::default())
        .unwrap();
    assert!(!final_fold.list_is_tight());
    assert!(mutation.nodes_visited < 64, "{mutation:?}");
    assert!(mutation.nodes_allocated < 64, "{mutation:?}");
}

fn list_tokens(items: u64) -> impl Iterator<Item = GreenToken> {
    iter::once(enter(1, GreenKind::DOCUMENT))
        .chain(iter::once(enter(2, GreenKind::LIST)))
        .chain(iter::once(property(PropertyTag::LIST, &[0, b'-', 1, 1])))
        .chain((0..items).flat_map(|index| {
            let item = 10 + index * 2;
            [
                enter(item, GreenKind::ITEM),
                property(PropertyTag::ITEM, &[0, 2]),
                coverage(2, 2, 0, CoveragePart::BLOCK_MARKER),
                enter(item + 1, GreenKind::PARAGRAPH),
                coverage(10, 8, 0, CoveragePart::CONTENT),
                GreenToken::Exit,
                coverage(1, 1, 0, CoveragePart::GAP),
                GreenToken::Exit,
            ]
        }))
        .chain([GreenToken::Exit, GreenToken::Exit])
}

#[test]
fn hundred_thousand_item_source_lookup_and_prefix_edit_are_indexless_and_local() {
    const ITEMS: u64 = 100_000;
    const BYTES_PER_ITEM: u64 = 13;
    const PREFIX_BYTES: u64 = 6;
    let mut build = GreenMutationReceipt::default();
    let sequence = SerializedGreenSequence::from_tokens(list_tokens(ITEMS), &mut build).unwrap();
    let retained = sequence.memory_stats();
    assert_eq!(
        retained.semantic_blocks,
        usize::try_from(ITEMS * 2 + 2).unwrap()
    );
    assert_eq!(
        retained.property_records,
        usize::try_from(ITEMS + 1).unwrap()
    );
    assert_eq!(retained.coverage_atoms, usize::try_from(ITEMS * 3).unwrap());
    assert_eq!(sequence.metric().bytes, ITEMS * BYTES_PER_ITEM);

    let far_item = ITEMS / 2 + 123;
    let far_content_offset = far_item * BYTES_PER_ITEM + 2;
    let far = sequence
        .source_lookup(
            SourceCoordinate::Bytes,
            far_content_offset,
            Affinity::Downstream,
        )
        .unwrap();
    let far_block = BlockId(11 + far_item * 2);
    assert_eq!(far.owner, far_block);
    assert_eq!(far.open_path.len(), 4);
    assert!(far.receipt.tree_nodes_visited < 128, "{:?}", far.receipt);
    // Reverse ancestry may decode the current variable-width 4 KiB leaf and
    // one boundary leaf on either side; it must not grow with document size.
    assert!(far.receipt.leaf_tokens_scanned < 5_000, "{:?}", far.receipt);
    let fast_far = sequence
        .source_owner_lookup(
            SourceCoordinate::Bytes,
            far_content_offset,
            Affinity::Downstream,
        )
        .unwrap();
    assert_eq!(fast_far.owner, far_block);
    assert!(fast_far.receipt.witness_fragments_used > 0);
    assert!(
        fast_far.receipt.leaf_tokens_scanned < far.receipt.leaf_tokens_scanned,
        "full={:?} fast={:?}",
        far.receipt,
        fast_far.receipt
    );
    let viewport = sequence.coverage_window(&fast_far, 256).unwrap();
    assert_eq!(viewport.owners.len(), 256);
    assert!(viewport.receipt.leaf_pages_visited <= 4, "{viewport:?}");
    assert!(viewport.receipt.leaf_tokens_scanned < 2_000, "{viewport:?}");
    let far_page = far.cursor.page_id();

    let first = sequence
        .source_lookup(SourceCoordinate::Bytes, 0, Affinity::Downstream)
        .unwrap();
    assert_eq!(first.open_path, [BlockId(1), BlockId(2), BlockId(10)]);
    assert_eq!(
        sequence.properties_from_hit(&first, 1).unwrap()[0].tag,
        PropertyTag::LIST
    );
    assert_eq!(
        sequence.properties_from_hit(&first, 2).unwrap()[0].bytes(),
        [0, 2]
    );
    let first_item_enter = first.ancestors[2].enter.clone();
    let replacement = [
        enter(9_000_000, GreenKind::ITEM),
        property(PropertyTag::ITEM, &[0, 2]),
        coverage(2, 2, 0, CoveragePart::BLOCK_MARKER),
        enter(9_000_001, GreenKind::PARAGRAPH),
        coverage(3, 3, 0, CoveragePart::CONTENT),
        GreenToken::Exit,
        coverage(1, 1, 0, CoveragePart::GAP),
        GreenToken::Exit,
    ];
    let mut edit = GreenMutationReceipt::default();
    let changed = sequence
        .splice_before(&first_item_enter, replacement, &mut edit)
        .unwrap();
    assert_eq!(
        changed.metric().bytes,
        ITEMS * BYTES_PER_ITEM + PREFIX_BYTES
    );
    assert_eq!(
        changed.validate_cursor(&far.cursor),
        Err(GreenError::StaleCursor)
    );
    let current_far = changed
        .source_lookup(
            SourceCoordinate::Bytes,
            far_content_offset + PREFIX_BYTES,
            Affinity::Downstream,
        )
        .unwrap();
    assert_eq!(current_far.owner, far_block);
    assert_eq!(current_far.cursor.page_id(), far_page);
    assert!(edit.nodes_visited < 256, "{edit:?}");
    assert!(edit.nodes_allocated < 256, "{edit:?}");
    let changed_first = changed
        .source_lookup(SourceCoordinate::Bytes, 0, Affinity::Downstream)
        .unwrap();
    let second_replacement = [
        enter(9_000_010, GreenKind::ITEM),
        property(PropertyTag::ITEM, &[0, 2]),
        coverage(2, 2, 0, CoveragePart::BLOCK_MARKER),
        enter(9_000_011, GreenKind::PARAGRAPH),
        coverage(3, 3, 0, CoveragePart::CONTENT),
        GreenToken::Exit,
        coverage(1, 1, 0, CoveragePart::GAP),
        GreenToken::Exit,
    ];
    let changed_twice = changed
        .splice_before(
            &changed_first.ancestors[2].enter,
            second_replacement,
            &mut GreenMutationReceipt::default(),
        )
        .unwrap();
    let shared_three =
        SerializedGreenSequence::shared_memory_stats(&[&sequence, &changed, &changed_twice]);
    assert!(
        shared_three.accounted_retained_bytes < retained.accounted_retained_bytes + 50_000,
        "single={retained:?} shared={shared_three:?}"
    );

    let hundredths = retained.accounted_retained_bytes * 100 / retained.semantic_blocks;
    eprintln!(
        "serialized_green_list items={ITEMS} blocks={} property_records={} coverage_atoms={} leaf_pages={} branch_nodes={} packed_tokens={} payload={} slots={} slot_bytes={} accounted={} bytes_per_block={}.{:02} bytes_per_segment={}.{:02} shared_three_roots_accounted={} shared_three_root_slots={} prototype_typed_mirror={} prototype_heap_lower_bound={} build_nodes={} typed_page_buffer={} encoded_page_buffer={} stream_roots={} stream_bin_bytes={} full_query_nodes={} full_query_tokens={} full_query_skips={} owner_query_nodes={} owner_query_tokens={} owner_query_witnesses={} viewport_pages={} viewport_tokens={} viewport_atoms={} edit_nodes_visited={} edit_nodes_allocated={} edit_payload_allocated={} far_page_reused={}",
        retained.semantic_blocks,
        retained.property_records,
        retained.coverage_atoms,
        retained.leaf_pages,
        retained.branch_nodes,
        retained.packed_token_bytes,
        retained.retained_payload_bytes,
        retained.arena_slots,
        retained.arena_slot_bytes,
        retained.accounted_retained_bytes,
        hundredths / 100,
        hundredths % 100,
        retained.accounted_retained_bytes * 100 / retained.coverage_atoms / 100,
        retained.accounted_retained_bytes * 100 / retained.coverage_atoms % 100,
        shared_three.accounted_retained_bytes,
        shared_three.arena_slots,
        retained.prototype_typed_token_bytes,
        retained.prototype_heap_lower_bound,
        build.nodes_allocated,
        build.maximum_typed_page_buffer_bytes,
        build.maximum_encoded_page_buffer_bytes,
        build.maximum_streaming_roots,
        build.maximum_streaming_bin_bytes,
        far.receipt.tree_nodes_visited,
        far.receipt.leaf_tokens_scanned,
        far.receipt.summary_nodes_skipped,
        fast_far.receipt.tree_nodes_visited,
        fast_far.receipt.leaf_tokens_scanned,
        fast_far.receipt.witness_fragments_used,
        viewport.receipt.leaf_pages_visited,
        viewport.receipt.leaf_tokens_scanned,
        viewport.receipt.coverage_atoms,
        edit.nodes_visited,
        edit.nodes_allocated,
        edit.packed_payload_bytes_allocated,
        current_far.cursor.page_id() == far_page,
    );
}

#[test]
fn ten_mib_plain_paragraph_is_one_coalesced_coverage_atom() {
    const BYTES: u64 = 10 * 1_024 * 1_024;
    let tokens = [
        enter(1, GreenKind::DOCUMENT),
        enter(2, GreenKind::PARAGRAPH),
        coverage(BYTES, BYTES, 0, CoveragePart::CONTENT),
        GreenToken::Exit,
        GreenToken::Exit,
    ];
    let mut build = GreenMutationReceipt::default();
    let sequence = SerializedGreenSequence::from_tokens(tokens, &mut build).unwrap();
    let retained = sequence.memory_stats();
    assert_eq!(retained.semantic_blocks, 2);
    assert_eq!(retained.coverage_atoms, 1);
    assert_eq!(retained.leaf_pages, 1);
    assert_eq!(sequence.metric().bytes, BYTES);
    let last = sequence
        .source_lookup(SourceCoordinate::Bytes, BYTES, Affinity::Downstream)
        .unwrap();
    assert_eq!(last.owner, BlockId(2));
    assert_eq!(last.byte_range, 0..BYTES);
    eprintln!(
        "serialized_green_plain_10mib source_bytes={BYTES} blocks={} coverage_atoms={} leaf_pages={} packed_tokens={} payload={} slot_bytes={} accounted={} prototype_typed_mirror={} prototype_heap_lower_bound={} typed_page_buffer={} encoded_page_buffer={} query_nodes={} query_tokens={}",
        retained.semantic_blocks,
        retained.coverage_atoms,
        retained.leaf_pages,
        retained.packed_token_bytes,
        retained.retained_payload_bytes,
        retained.arena_slot_bytes,
        retained.accounted_retained_bytes,
        retained.prototype_typed_token_bytes,
        retained.prototype_heap_lower_bound,
        build.maximum_typed_page_buffer_bytes,
        build.maximum_encoded_page_buffer_bytes,
        last.receipt.tree_nodes_visited,
        last.receipt.leaf_tokens_scanned,
    );
    assert!(retained.accounted_retained_bytes < 256);
}

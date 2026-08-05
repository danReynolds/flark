#![allow(clippy::too_many_lines)]

use flark_v3_runtime_slice::{
    ClosedChildAggregate, CompactBlockFacts, GenericAffinity, GenericBlockKind, GenericCoordinate,
    GenericGreenMetric, GenericSourceKind, HierarchicalBuildReceipt, HierarchicalGreenDocument,
    HierarchicalRootSpec, HierarchicalSiblingEntry, HierarchicalViewportReceipt, PageArena,
    hierarchical_retained_receipt,
};

const BLOCKS: u64 = 100_000;
const OUTER_ITEMS: u64 = 1_000;
const NESTED_AT: u64 = 500;

fn metric(bytes: u64, utf16: u64) -> GenericGreenMetric {
    GenericGreenMetric { bytes, utf16 }
}

fn facts_for(kind: GenericBlockKind, index: u64) -> CompactBlockFacts {
    match kind {
        GenericBlockKind::Heading => CompactBlockFacts::heading(
            u8::try_from(index % 6 + 1).unwrap(),
            index.is_multiple_of(2),
        ),
        GenericBlockKind::List => CompactBlockFacts::list(
            u32::try_from(index + 1).unwrap(),
            if index.is_multiple_of(2) { b'-' } else { b'1' },
            if index.is_multiple_of(2) { 0 } else { b'.' },
            true,
        ),
        GenericBlockKind::Item => CompactBlockFacts::item(2, 2),
        GenericBlockKind::FencedCode => CompactBlockFacts::fence(b'`', 3, 0),
        GenericBlockKind::Html => CompactBlockFacts::html(u8::try_from(index % 7 + 1).unwrap()),
        GenericBlockKind::Table => {
            CompactBlockFacts::table(u16::try_from(index % 8 + 1).unwrap(), 0b10_01_00)
        }
        GenericBlockKind::TableRow | GenericBlockKind::TableCell => CompactBlockFacts(index),
        _ => CompactBlockFacts::paragraph(),
    }
}

fn heterogeneous_kind(index: u64) -> GenericBlockKind {
    match index % 12 {
        0 => GenericBlockKind::Paragraph,
        1 => GenericBlockKind::Heading,
        2 => GenericBlockKind::FencedCode,
        3 => GenericBlockKind::IndentedCode,
        4 => GenericBlockKind::Html,
        5 => GenericBlockKind::Table,
        6 => GenericBlockKind::TableRow,
        7 => GenericBlockKind::TableCell,
        8 => GenericBlockKind::ThematicBreak,
        9 => GenericBlockKind::BlockQuote,
        10 => GenericBlockKind::List,
        _ => GenericBlockKind::Item,
    }
}

fn heterogeneous_entry(index: u64) -> HierarchicalSiblingEntry {
    let kind = heterogeneous_kind(index);
    let bytes = 5 + index % 7;
    let utf16 = bytes - index % 3;
    HierarchicalSiblingEntry {
        block: 10 + index,
        coverage: 1_000_000 + index,
        kind,
        source_kind: match index % 3 {
            0 => GenericSourceKind::Terminal,
            1 => GenericSourceKind::Gap,
            _ => GenericSourceKind::ContainerMarker,
        },
        local_metric: metric(bytes, utf16),
        facts: facts_for(kind, index),
        contribution: ClosedChildAggregate::default(),
        child: None,
    }
}

fn prefix_metric(entries: u64) -> GenericGreenMetric {
    (0..entries).fold(metric(0, 0), |sum, index| {
        let entry = heterogeneous_entry(index);
        metric(
            sum.bytes + entry.local_metric.bytes,
            sum.utf16 + entry.local_metric.utf16,
        )
    })
}

fn settle(arena: &mut PageArena) {
    while arena.metrics().pending_releases != 0 {
        let receipt = arena.poll_reclaim(16_384).expect("settle shared arena");
        assert!(receipt.reference_transitions <= 16_384);
    }
}

#[test]
fn hundred_thousand_heterogeneous_blocks_keep_three_roots_compact_and_source_queryable() {
    let mut arena = PageArena::new();
    let mut build = HierarchicalBuildReceipt::default();
    let old = HierarchicalGreenDocument::build(
        &mut arena,
        HierarchicalRootSpec {
            block: 1,
            kind: GenericBlockKind::Document,
            facts: CompactBlockFacts::paragraph(),
            epoch: 1,
            source_revision: 1,
        },
        (0..BLOCKS).map(heterogeneous_entry),
        &mut build,
    )
    .expect("generic 100k hierarchical document");
    assert_eq!(old.block_count(&arena).unwrap(), BLOCKS + 1);
    assert_eq!(old.metric(&arena).unwrap(), prefix_metric(BLOCKS));
    let leaf_count = old.leaf_count(&arena).unwrap();
    assert!(leaf_count > 900 && leaf_count < 1_100);
    let far_page = old.leaf_at(&arena, leaf_count - 1).unwrap().unwrap();

    let last = old
        .source_lookup(
            &arena,
            old.metric(&arena).unwrap().bytes - 1,
            GenericCoordinate::Bytes,
            GenericAffinity::Downstream,
        )
        .unwrap()
        .unwrap();
    assert_eq!(last.owner, heterogeneous_entry(BLOCKS - 1).block);
    assert_eq!(last.facts, heterogeneous_entry(BLOCKS - 1).facts);
    assert!(last.receipt.sequence_nodes_visited < 128);
    assert!(last.receipt.entries_examined <= 101);
    let last_utf16 = old
        .source_lookup(
            &arena,
            old.metric(&arena).unwrap().utf16 - 1,
            GenericCoordinate::Utf16,
            GenericAffinity::Downstream,
        )
        .unwrap()
        .unwrap();
    assert_eq!(last_utf16.owner, last.owner);

    let middle = prefix_metric(50_000);
    let mut viewport_receipt = HierarchicalViewportReceipt::default();
    let viewport = old
        .viewport(
            &arena,
            middle.bytes..middle.bytes + 160,
            GenericCoordinate::Bytes,
            &mut viewport_receipt,
        )
        .unwrap();
    assert!(viewport.len() >= 18 && viewport.len() <= 24);
    assert!(viewport_receipt.sequence_nodes_visited < 128);
    assert!(viewport_receipt.leaves_visited <= 2);
    assert!(viewport_receipt.entries_examined <= 202);
    assert!(viewport.iter().all(|entry| {
        let index = entry.owner - 10;
        entry.facts == facts_for(entry.kind, index)
    }));

    let first = old
        .source_lookup(
            &arena,
            0,
            GenericCoordinate::Bytes,
            GenericAffinity::Downstream,
        )
        .unwrap()
        .unwrap();
    let mut prefix_edit = HierarchicalBuildReceipt::default();
    let candidate = old
        .replace_at_cursor(
            &mut arena,
            first.cursor,
            HierarchicalSiblingEntry {
                local_metric: metric(
                    heterogeneous_entry(0).local_metric.bytes + 3,
                    heterogeneous_entry(0).local_metric.utf16 + 3,
                ),
                ..heterogeneous_entry(0)
            },
            &mut prefix_edit,
        )
        .expect("source-derived prefix edit");
    assert_eq!(
        candidate.leaf_at(&arena, leaf_count - 1).unwrap(),
        Some(far_page)
    );
    let current_far = candidate
        .source_lookup(
            &arena,
            candidate.metric(&arena).unwrap().bytes - 1,
            GenericCoordinate::Bytes,
            GenericAffinity::Downstream,
        )
        .unwrap()
        .unwrap();
    assert_eq!(current_far.owner, last.owner);
    assert_eq!(current_far.cursor.page_id(), far_page);

    let interior_offset = middle.bytes + 3;
    let interior = candidate
        .source_lookup(
            &arena,
            interior_offset,
            GenericCoordinate::Bytes,
            GenericAffinity::Downstream,
        )
        .unwrap()
        .unwrap();
    assert_eq!(interior.owner, heterogeneous_entry(50_000).block);
    let mut interior_edit = HierarchicalBuildReceipt::default();
    let current = candidate
        .replace_at_cursor(
            &mut arena,
            interior.cursor,
            HierarchicalSiblingEntry {
                local_metric: metric(
                    heterogeneous_entry(50_000).local_metric.bytes + 1,
                    heterogeneous_entry(50_000).local_metric.utf16 + 1,
                ),
                facts: CompactBlockFacts(0xfeed_beef),
                ..heterogeneous_entry(50_000)
            },
            &mut interior_edit,
        )
        .expect("source-derived interior edit");
    assert_eq!(
        current.leaf_at(&arena, leaf_count - 1).unwrap(),
        Some(far_page)
    );
    assert!(prefix_edit.sequence_nodes_visited < 128);
    assert!(interior_edit.sequence_nodes_visited < 128);
    assert!(prefix_edit.entries_decoded <= 101);
    assert!(interior_edit.entries_decoded <= 101);

    settle(&mut arena);
    let retained = hierarchical_retained_receipt(&arena, 3);
    let hundredths = retained.accounted_retained_bytes * 100
        / (usize::try_from(BLOCKS).expect("100k block count fits usize") + 1);
    eprintln!(
        "hierarchical_green_100k blocks={} leaf_pages={} initial_branches={} initial_roots={} initial_manifests={} live_nodes={} payload={} edge_bytes={} storage={} slot_capacity={} slot_bytes={} allocator_model={} root_handles={} accounted_three_roots={} bytes_per_block={}.{:02} heap_allocations={} high_water_storage={} build_payload={} build_edges={} typed_page_buffer={} encoded_page_buffer={} edge_buffer={} stream_roots={} stream_bin_bytes={} journal_capacity={} journal_bytes={} last_query_nodes={} last_query_entries={} viewport_nodes={} viewport_leaves={} viewport_entries={} prefix_edit_nodes={} prefix_edit_payload={} prefix_edit_roots={} prefix_edit_manifests={} interior_edit_nodes={} interior_edit_payload={} interior_edit_roots={} interior_edit_manifests={} far_page_reused=true",
        BLOCKS + 1,
        leaf_count,
        build.branch_nodes_allocated,
        build.root_nodes_allocated,
        build.manifest_nodes_allocated,
        retained.live_nodes,
        retained.live_payload_bytes,
        retained.live_edge_bytes,
        retained.live_storage_bytes,
        retained.slot_capacity,
        retained.slot_storage_bytes,
        retained.modeled_allocator_overhead_bytes,
        retained.root_handle_bytes,
        retained.accounted_retained_bytes,
        hundredths / 100,
        hundredths % 100,
        retained.heap_page_allocations,
        retained.high_water_storage_bytes,
        build.payload_bytes_copied,
        build.edge_bytes_copied,
        build.maximum_typed_page_buffer_bytes,
        build.maximum_encoded_page_buffer_bytes,
        build.maximum_edge_buffer_bytes,
        build.maximum_streaming_roots,
        build.maximum_streaming_bin_bytes,
        build.owner_journal_capacity,
        build.owner_journal_bytes,
        last.receipt.sequence_nodes_visited,
        last.receipt.entries_examined,
        viewport_receipt.sequence_nodes_visited,
        viewport_receipt.leaves_visited,
        viewport_receipt.entries_examined,
        prefix_edit.sequence_nodes_visited,
        prefix_edit.payload_bytes_copied,
        prefix_edit.root_nodes_allocated,
        prefix_edit.manifest_nodes_allocated,
        interior_edit.sequence_nodes_visited,
        interior_edit.payload_bytes_copied,
        interior_edit.root_nodes_allocated,
        interior_edit.manifest_nodes_allocated,
    );
    assert!(hundredths / 100 < 50);

    old.release_later(&mut arena).unwrap();
    candidate.release_later(&mut arena).unwrap();
    current.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

fn item_entry(
    index: u64,
    child: Option<flark_v3_runtime_slice::ArenaId>,
) -> HierarchicalSiblingEntry {
    HierarchicalSiblingEntry {
        block: 20_000_000 + index,
        coverage: 30_000_000 + index,
        kind: GenericBlockKind::Item,
        source_kind: GenericSourceKind::Terminal,
        local_metric: metric(13, 11),
        facts: CompactBlockFacts::item(2, 2),
        contribution: ClosedChildAggregate::default(),
        child,
    }
}

#[test]
fn nested_list_fold_change_propagates_through_two_source_derived_local_splices() {
    let mut arena = PageArena::new();
    let mut inner_build = HierarchicalBuildReceipt::default();
    let inner_old = HierarchicalGreenDocument::build(
        &mut arena,
        HierarchicalRootSpec {
            block: 2,
            kind: GenericBlockKind::List,
            facts: CompactBlockFacts::list(1, b'-', 0, true),
            epoch: 1,
            source_revision: 1,
        },
        (0..BLOCKS).map(|index| item_entry(index, None)),
        &mut inner_build,
    )
    .unwrap();
    assert!(inner_old.list_is_tight(&arena).unwrap());
    let inner_leaves = inner_old.leaf_count(&arena).unwrap();
    let inner_far = inner_old
        .leaf_at(&arena, inner_leaves - 1)
        .unwrap()
        .unwrap();
    let changed_index = 50_000;
    let inner_hit = inner_old
        .source_lookup(
            &arena,
            changed_index * 13,
            GenericCoordinate::Bytes,
            GenericAffinity::Downstream,
        )
        .unwrap()
        .unwrap();
    let mut inner_edit = HierarchicalBuildReceipt::default();
    let inner_changed = inner_old
        .replace_at_cursor(
            &mut arena,
            inner_hit.cursor,
            HierarchicalSiblingEntry {
                contribution: ClosedChildAggregate {
                    item_loose_if_nonlast: true,
                    ..ClosedChildAggregate::default()
                },
                ..item_entry(changed_index, None)
            },
            &mut inner_edit,
        )
        .unwrap();
    assert!(!inner_changed.list_is_tight(&arena).unwrap());
    assert_eq!(
        inner_changed.leaf_at(&arena, inner_leaves - 1).unwrap(),
        Some(inner_far)
    );

    let mut outer_build = HierarchicalBuildReceipt::default();
    let outer_old = HierarchicalGreenDocument::build(
        &mut arena,
        HierarchicalRootSpec {
            block: 3,
            kind: GenericBlockKind::List,
            facts: CompactBlockFacts::list(1, b'-', 0, true),
            epoch: 1,
            source_revision: 1,
        },
        (0..OUTER_ITEMS).map(|index| {
            let mut entry = item_entry(1_000_000 + index, None);
            entry.local_metric = metric(2, 2);
            if index == NESTED_AT {
                entry.child = Some(inner_old.root_id());
            }
            entry
        }),
        &mut outer_build,
    )
    .unwrap();
    assert!(outer_old.list_is_tight(&arena).unwrap());
    let outer_leaves = outer_old.leaf_count(&arena).unwrap();
    let outer_far = outer_old
        .leaf_at(&arena, outer_leaves - 1)
        .unwrap()
        .unwrap();
    let outer_hit = outer_old
        .source_lookup(
            &arena,
            NESTED_AT * 2,
            GenericCoordinate::Bytes,
            GenericAffinity::Downstream,
        )
        .unwrap()
        .unwrap();
    let mut replacement = item_entry(1_000_000 + NESTED_AT, Some(inner_changed.root_id()));
    replacement.local_metric = metric(2, 2);
    replacement.contribution = ClosedChildAggregate {
        item_loose_if_nonlast: true,
        ..ClosedChildAggregate::default()
    };
    let mut outer_edit = HierarchicalBuildReceipt::default();
    let outer_changed = outer_old
        .replace_at_cursor(&mut arena, outer_hit.cursor, replacement, &mut outer_edit)
        .unwrap();
    assert!(!outer_changed.list_is_tight(&arena).unwrap());
    assert_eq!(
        outer_changed.leaf_at(&arena, outer_leaves - 1).unwrap(),
        Some(outer_far)
    );
    assert!(inner_edit.sequence_nodes_visited < 128);
    assert!(outer_edit.sequence_nodes_visited < 128);
    assert!(inner_edit.entries_decoded <= 101);
    assert!(outer_edit.entries_decoded <= 101);
    eprintln!(
        "hierarchical_green_nested_fold inner_items={BLOCKS} outer_items={OUTER_ITEMS} inner_edit_nodes={} inner_entries_decoded={} outer_edit_nodes={} outer_entries_decoded={} inner_far_reused=true outer_far_reused=true inner_tight_before=true inner_tight_after=false outer_tight_before=true outer_tight_after=false",
        inner_edit.sequence_nodes_visited,
        inner_edit.entries_decoded,
        outer_edit.sequence_nodes_visited,
        outer_edit.entries_decoded,
    );

    inner_old.release_later(&mut arena).unwrap();
    inner_changed.release_later(&mut arena).unwrap();
    outer_old.release_later(&mut arena).unwrap();
    outer_changed.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

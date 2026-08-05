#![allow(clippy::items_after_statements, clippy::too_many_lines)]

use flark_v3_runtime_slice::{
    BlockId, BlockOrder, BlockRecord, BlockRecordTable, ClosedChildAggregate,
    ContainerChildFoldIndex, ContainerFoldInput, ContainerFoldSemantics, CoverageAffinity,
    CoverageId, CoverageOrderOracle, CoveragePartition, CoverageRecord, CoverageSegment,
    CoverageSegmentKind, DirectChildAggregate, EventPageBuilder, EventTapeReceipt,
    ExplicitCoverageOrder, ForestAnchor, ForestBlockId, ForestCoverageId, ForestRunCursorId,
    GrammarRevision, OpenFrame, OpenOverlay, OutputSequence, PageArena, ParseGeneration, Position,
    PresentationAuthority, PresentationBudget, PresentationBuildOutcome, PresentationEpoch,
    PresentationFact, PresentationFactBuilder, PresentationLookup, PresentationRange,
    PresentationRequest, PresentationRequestId, PresentationRequestScope, PresentationStyleTag,
    ProjectionState, PushResult, RecordForestError, RecordForestManifest, RecordForestReceipt,
    RunRangeId, SemanticRootGeneration, SourceAnchor, SourceRevision, StructuralBlock,
    StructuralEvent,
};

fn anchor(coverage: u64, local: u32) -> ForestAnchor {
    ForestAnchor {
        coverage: ForestCoverageId(coverage),
        local_bytes: local,
        local_utf16: local,
    }
}

fn event_anchor(coverage: u64, local: u32) -> SourceAnchor {
    SourceAnchor {
        coverage: CoverageId(coverage),
        local_bytes: local,
        local_utf16: local,
    }
}

fn record(
    id: u64,
    parent: Option<u64>,
    start: ForestAnchor,
    end: ForestAnchor,
    terminal: bool,
) -> BlockRecord {
    BlockRecord {
        id: ForestBlockId(id),
        parent: parent.map(ForestBlockId),
        kind_tag: u16::try_from(id % 17).expect("small kind"),
        context: id.wrapping_mul(19),
        property: None,
        start,
        end,
        content: terminal.then_some(ForestRunCursorId(id)),
        subtree_last: ForestBlockId(id),
        terminal,
    }
}

fn order(ids: impl IntoIterator<Item = u64>) -> ExplicitCoverageOrder {
    ExplicitCoverageOrder::from_ids(ids.into_iter().map(ForestCoverageId))
        .expect("unique coverage order")
}

struct NumericCoverageOrder;

impl CoverageOrderOracle for NumericCoverageOrder {
    fn rank(&self, coverage: ForestCoverageId) -> Result<u64, RecordForestError> {
        Ok(coverage.0)
    }
}

fn settle(arena: &mut PageArena) {
    while arena.metrics().pending_releases != 0 {
        arena.poll_reclaim(16_384).expect("settle arena transfers");
    }
}

fn presentation_epoch() -> PresentationEpoch {
    PresentationEpoch {
        source: SourceRevision(7),
        grammar: GrammarRevision(3),
        generation: ParseGeneration(11),
        semantic_root: SemanticRootGeneration(13),
    }
}

fn presentation_request(start: ForestAnchor, end: ForestAnchor) -> PresentationRequest {
    PresentationRequest {
        id: PresentationRequestId(17),
        scope: PresentationRequestScope::ActiveEdit,
        range: PresentationRange { start, end },
        required_authority: PresentationAuthority::INLINE_PROJECTION,
    }
}

fn empty_presentation(
    arena: &mut PageArena,
    order: &ExplicitCoverageOrder,
    start: ForestAnchor,
    end: ForestAnchor,
) -> flark_v3_runtime_slice::PresentationFactLease {
    let request = presentation_request(start, end);
    let builder = PresentationFactBuilder::new(
        presentation_epoch(),
        request,
        PresentationAuthority::INLINE_PROJECTION,
        PresentationBudget::hard_max(),
    );
    let PresentationBuildOutcome::Exact { lease, .. } = builder
        .finish(arena, order)
        .expect("empty exact presentation")
    else {
        panic!("empty presentation fits the bounded lease");
    };
    lease
}

#[test]
fn total_partition_recovers_nested_blank_owners_dense_lines_and_affinity() {
    let coverage_order = order(1..=4);
    let mut arena = PageArena::new();
    let mut receipt = RecordForestReceipt::default();
    let records = BlockRecordTable::from_records(
        &mut arena,
        &[
            record(1, None, anchor(1, 0), anchor(4, 0), false),
            record(2, Some(1), anchor(1, 0), anchor(4, 0), false),
            record(3, Some(2), anchor(1, 0), anchor(2, 0), true),
            record(4, Some(2), anchor(3, 0), anchor(4, 0), true),
        ],
        &mut receipt,
    )
    .expect("nested records");
    let partition = CoveragePartition::from_segments(
        &mut arena,
        &[
            CoverageSegment {
                owner: ForestBlockId(3),
                kind: CoverageSegmentKind::Terminal,
                start: anchor(1, 0),
                end: anchor(2, 0),
            },
            CoverageSegment {
                owner: ForestBlockId(2),
                kind: CoverageSegmentKind::Gap,
                start: anchor(2, 0),
                end: anchor(3, 0),
            },
            CoverageSegment {
                owner: ForestBlockId(4),
                kind: CoverageSegmentKind::Terminal,
                start: anchor(3, 0),
                end: anchor(4, 0),
            },
        ],
        anchor(1, 0),
        anchor(4, 0),
        &coverage_order,
        &mut receipt,
    )
    .expect("total blank-safe partition");

    let downstream = partition
        .as_ref()
        .enclosing_blocks(
            &arena,
            anchor(2, 0),
            CoverageAffinity::Downstream,
            &coverage_order,
            records.as_ref(),
            None,
            8,
        )
        .expect("blank lookup")
        .expect("covered blank");
    assert_eq!(downstream.segment.kind, CoverageSegmentKind::Gap);
    assert_eq!(downstream.segment.owner, ForestBlockId(2));
    assert_eq!(downstream.blocks.len(), 2);
    assert!(
        matches!(downstream.blocks[0], StructuralBlock::Finalized(value) if value.id == ForestBlockId(2))
    );
    assert!(
        matches!(downstream.blocks[1], StructuralBlock::Finalized(value) if value.id == ForestBlockId(1))
    );

    let upstream = partition
        .as_ref()
        .enclosing_blocks(
            &arena,
            anchor(2, 0),
            CoverageAffinity::Upstream,
            &coverage_order,
            records.as_ref(),
            None,
            8,
        )
        .expect("upstream lookup")
        .expect("preceding terminal");
    assert_eq!(upstream.segment.owner, ForestBlockId(3));
    assert_eq!(upstream.blocks.len(), 3);
    assert_eq!(
        partition
            .as_ref()
            .lookup(
                &arena,
                anchor(4, 0),
                CoverageAffinity::Upstream,
                &coverage_order,
            )
            .unwrap()
            .0
            .unwrap()
            .owner,
        ForestBlockId(4)
    );
    assert!(
        partition
            .as_ref()
            .lookup(
                &arena,
                anchor(4, 0),
                CoverageAffinity::Downstream,
                &coverage_order,
            )
            .unwrap()
            .0
            .is_none()
    );

    records.release_later(&mut arena).unwrap();
    partition.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);

    let mut dense_arena = PageArena::new();
    let dense_order = order([1]);
    let dense_segments = (0_u32..1_202)
        .map(|offset| CoverageSegment {
            owner: ForestBlockId(u64::from(offset) + 1),
            kind: CoverageSegmentKind::ContainerMarker,
            start: anchor(1, offset),
            end: anchor(1, offset + 1),
        })
        .collect::<Vec<_>>();
    let dense = CoveragePartition::from_segments(
        &mut dense_arena,
        &dense_segments,
        anchor(1, 0),
        anchor(1, 1_202),
        &dense_order,
        &mut RecordForestReceipt::default(),
    )
    .expect("same-line partition uses composite keys");
    let (found, dense_receipt) = dense
        .as_ref()
        .lookup(
            &dense_arena,
            anchor(1, 1_201),
            CoverageAffinity::Downstream,
            &dense_order,
        )
        .expect("dense lookup");
    assert_eq!(found.unwrap().owner, ForestBlockId(1_202));
    assert!(dense_receipt.nodes_visited <= 8);
    assert!(dense_receipt.segments_examined <= 84);
    dense.release_later(&mut dense_arena).unwrap();
    settle(&mut dense_arena);

    let mut empty_arena = PageArena::new();
    let empty = CoveragePartition::from_segments(
        &mut empty_arena,
        &[],
        anchor(1, 0),
        anchor(1, 0),
        &dense_order,
        &mut RecordForestReceipt::default(),
    )
    .expect("empty document has an explicit empty partition");
    assert!(
        empty
            .as_ref()
            .lookup(
                &empty_arena,
                anchor(1, 0),
                CoverageAffinity::Downstream,
                &dense_order,
            )
            .unwrap()
            .0
            .is_none()
    );
    empty.release_later(&mut empty_arena).unwrap();
}

#[test]
fn coverage_prefix_insert_reuses_stable_suffix_pages_under_new_rank_oracle() {
    let old_order = order(2..=202);
    let segments = (2..=201)
        .map(|coverage| CoverageSegment {
            owner: ForestBlockId(coverage),
            kind: CoverageSegmentKind::Terminal,
            start: anchor(coverage, 0),
            end: anchor(coverage + 1, 0),
        })
        .collect::<Vec<_>>();
    let mut arena = PageArena::new();
    let original = CoveragePartition::from_segments(
        &mut arena,
        &segments,
        anchor(2, 0),
        anchor(202, 0),
        &old_order,
        &mut RecordForestReceipt::default(),
    )
    .unwrap();
    assert!(original.as_ref().page_count(&arena).unwrap() > 2);
    let old_suffix_page = original.as_ref().page_at(&arena, 2).unwrap().unwrap();

    let new_order = order(1..=202);
    let mut mutation = RecordForestReceipt::default();
    let edited = original
        .prepend_segment(
            &mut arena,
            CoverageSegment {
                owner: ForestBlockId(1),
                kind: CoverageSegmentKind::Terminal,
                start: anchor(1, 0),
                end: anchor(2, 0),
            },
            &new_order,
            &mut mutation,
        )
        .expect("stable-anchor prefix splice");
    assert_eq!(
        edited.as_ref().page_at(&arena, 3).unwrap(),
        Some(old_suffix_page)
    );
    let (found, query) = edited
        .as_ref()
        .lookup(
            &arena,
            anchor(150, 0),
            CoverageAffinity::Downstream,
            &new_order,
        )
        .unwrap();
    assert_eq!(found.unwrap().owner, ForestBlockId(150));
    assert!(query.nodes_visited <= 8);
    assert_eq!(
        mutation.pages_reused,
        usize::try_from(original.as_ref().page_count(&arena).unwrap() - 1).unwrap()
    );

    original.release_later(&mut arena).unwrap();
    edited.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn block_order_boundary_splice_and_large_suffix_identity_are_exact() {
    let mut small_arena = PageArena::new();
    let small = BlockOrder::from_entries(
        &mut small_arena,
        &[
            ForestBlockId(1),
            ForestBlockId(2),
            ForestBlockId(3),
            ForestBlockId(4),
        ],
        2,
        &mut RecordForestReceipt::default(),
    )
    .unwrap();
    let inserted = small
        .splice(
            &mut small_arena,
            2..2,
            &[ForestBlockId(9)],
            &mut RecordForestReceipt::default(),
        )
        .expect("exact leaf-boundary insertion");
    let values = (0..inserted.as_ref().len(&small_arena).unwrap())
        .map(|index| {
            inserted
                .as_ref()
                .get(&small_arena, index)
                .unwrap()
                .0
                .unwrap()
                .0
        })
        .collect::<Vec<_>>();
    assert_eq!(values, [1, 2, 9, 3, 4]);
    small.release_later(&mut small_arena).unwrap();
    inserted.release_later(&mut small_arena).unwrap();
    settle(&mut small_arena);

    const PAGES: u64 = 65_536;
    let mut arena = PageArena::new();
    let entries = (1..=PAGES).map(ForestBlockId).collect::<Vec<_>>();
    let original =
        BlockOrder::from_entries(&mut arena, &entries, 1, &mut RecordForestReceipt::default())
            .expect("forced-page preorder");
    let far_page = original.as_ref().page_at(&arena, 60_000).unwrap().unwrap();
    let suffix_root = original
        .as_ref()
        .right_partition_root(&arena)
        .unwrap()
        .unwrap();
    let mut mutation = RecordForestReceipt::default();
    let edited = original
        .splice(&mut arena, 0..0, &[ForestBlockId(PAGES + 1)], &mut mutation)
        .expect("prefix splice");
    assert_eq!(
        edited.as_ref().page_at(&arena, 60_001).unwrap(),
        Some(far_page)
    );
    assert!(edited.as_ref().contains_node(&arena, suffix_root).unwrap());
    assert!(mutation.branch_nodes_allocated < 128);
    assert!(mutation.nodes_visited < 256);
    assert_eq!(mutation.pages_reused, usize::try_from(PAGES - 1).unwrap());

    original.release_later(&mut arena).unwrap();
    edited.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn repeated_same_gap_inserts_keep_pages_packed_and_distant_suffix_shared() {
    const INITIAL: u64 = 20_000;
    const INSERTS: u64 = 10_000;
    const ORDER_CAPACITY: usize = 128;
    let mut order_arena = PageArena::new();
    let entries = (1..=INITIAL).map(ForestBlockId).collect::<Vec<_>>();
    let mut block_order = BlockOrder::from_entries(
        &mut order_arena,
        &entries,
        ORDER_CAPACITY,
        &mut RecordForestReceipt::default(),
    )
    .unwrap();
    let far_order_page = block_order
        .as_ref()
        .page_at(
            &order_arena,
            block_order.as_ref().page_count(&order_arena).unwrap() - 1,
        )
        .unwrap()
        .unwrap();
    for insertion in 0..INSERTS {
        let next = block_order
            .splice(
                &mut order_arena,
                0..0,
                &[ForestBlockId(1_000_000 + insertion)],
                &mut RecordForestReceipt::default(),
            )
            .unwrap();
        block_order.release_later(&mut order_arena).unwrap();
        block_order = next;
        order_arena.poll_reclaim(1_024).unwrap();
    }
    settle(&mut order_arena);
    let expected_order_pages = (INITIAL + INSERTS).div_ceil(ORDER_CAPACITY as u64);
    assert!(block_order.as_ref().page_count(&order_arena).unwrap() <= expected_order_pages + 1);
    assert!(block_order.as_ref().height(&order_arena).unwrap() <= 12);
    assert!(
        block_order
            .as_ref()
            .contains_node(&order_arena, far_order_page)
            .unwrap()
    );
    assert!(order_arena.metrics().live_payload_bytes < 400_000);
    block_order.release_later(&mut order_arena).unwrap();
    settle(&mut order_arena);

    const INITIAL_SEGMENTS: u64 = 400;
    let initial_start = INSERTS + 1;
    let segments = (initial_start..initial_start + INITIAL_SEGMENTS)
        .map(|coverage| CoverageSegment {
            owner: ForestBlockId(coverage),
            kind: CoverageSegmentKind::Terminal,
            start: anchor(coverage, 0),
            end: anchor(coverage + 1, 0),
        })
        .collect::<Vec<_>>();
    let mut coverage_arena = PageArena::new();
    let mut coverage = CoveragePartition::from_segments(
        &mut coverage_arena,
        &segments,
        anchor(initial_start, 0),
        anchor(initial_start + INITIAL_SEGMENTS, 0),
        &NumericCoverageOrder,
        &mut RecordForestReceipt::default(),
    )
    .unwrap();
    let far_coverage_page = coverage
        .as_ref()
        .page_at(
            &coverage_arena,
            coverage.as_ref().page_count(&coverage_arena).unwrap() - 1,
        )
        .unwrap()
        .unwrap();
    for coverage_id in (1..=INSERTS).rev() {
        let next = coverage
            .prepend_segment(
                &mut coverage_arena,
                CoverageSegment {
                    owner: ForestBlockId(coverage_id),
                    kind: CoverageSegmentKind::Terminal,
                    start: anchor(coverage_id, 0),
                    end: anchor(coverage_id + 1, 0),
                },
                &NumericCoverageOrder,
                &mut RecordForestReceipt::default(),
            )
            .unwrap();
        coverage.release_later(&mut coverage_arena).unwrap();
        coverage = next;
        coverage_arena.poll_reclaim(1_024).unwrap();
    }
    settle(&mut coverage_arena);
    let coverage_capacity = 84_u64;
    let expected_coverage_pages = (INITIAL_SEGMENTS + INSERTS).div_ceil(coverage_capacity);
    assert!(coverage.as_ref().page_count(&coverage_arena).unwrap() <= expected_coverage_pages + 1);
    assert!(coverage.as_ref().height(&coverage_arena).unwrap() <= 12);
    assert!(
        coverage
            .as_ref()
            .contains_node(&coverage_arena, far_coverage_page)
            .unwrap()
    );
    assert!(coverage_arena.metrics().live_payload_bytes < 700_000);
    coverage.release_later(&mut coverage_arena).unwrap();
    settle(&mut coverage_arena);
}

#[test]
fn detach_reparent_and_spanning_list_insert_do_not_duplicate_or_rewrite_suffix() {
    let mut arena = PageArena::new();
    let mut receipt = RecordForestReceipt::default();
    let initial_records = BlockRecordTable::from_records(
        &mut arena,
        &[
            record(1, None, anchor(1, 0), anchor(5, 0), false),
            record(2, Some(1), anchor(1, 0), anchor(2, 0), true),
            record(3, None, anchor(2, 0), anchor(3, 0), true),
            record(4, None, anchor(3, 0), anchor(5, 0), false),
        ],
        &mut receipt,
    )
    .unwrap();
    let reparented = initial_records
        .upsert(
            &mut arena,
            record(2, Some(4), anchor(1, 0), anchor(2, 0), true),
            &mut receipt,
        )
        .expect("promotion reparent");
    let without_definition = reparented
        .remove(&mut arena, ForestBlockId(3), &mut receipt)
        .expect("reference-definition paragraph detach");
    assert_eq!(
        without_definition
            .as_ref()
            .get(&arena, ForestBlockId(2))
            .unwrap()
            .0
            .unwrap()
            .parent,
        Some(ForestBlockId(4))
    );
    assert!(
        without_definition
            .as_ref()
            .get(&arena, ForestBlockId(3))
            .unwrap()
            .0
            .is_none()
    );

    const CHILDREN: u64 = 100_000;
    let ids = std::iter::once(ForestBlockId(1))
        .chain((0..CHILDREN).map(|index| ForestBlockId(index + 10)))
        .chain(std::iter::once(ForestBlockId(2)))
        .collect::<Vec<_>>();
    let list_order =
        BlockOrder::from_entries(&mut arena, &ids, 128, &mut RecordForestReceipt::default())
            .expect("large spanning list order");
    let far_leaf_index = list_order.as_ref().page_count(&arena).unwrap() - 1;
    let far_page = list_order
        .as_ref()
        .page_at(&arena, far_leaf_index)
        .unwrap()
        .unwrap();
    let mut insertion_receipt = RecordForestReceipt::default();
    let inserted = list_order
        .splice(
            &mut arena,
            50_001..50_001,
            &[ForestBlockId(1_000_001)],
            &mut insertion_receipt,
        )
        .expect("local child insertion");
    let new_far_leaf = inserted.as_ref().page_count(&arena).unwrap() - 1;
    assert_eq!(
        inserted.as_ref().page_at(&arena, new_far_leaf).unwrap(),
        Some(far_page)
    );
    assert_eq!(inserted.as_ref().len(&arena).unwrap(), CHILDREN + 3);
    assert!(insertion_receipt.pages_reused > 780);
    assert!(insertion_receipt.nodes_visited < 128);

    initial_records.release_later(&mut arena).unwrap();
    reparented.release_later(&mut arena).unwrap();
    without_definition.release_later(&mut arena).unwrap();
    list_order.release_later(&mut arena).unwrap();
    inserted.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn direct_child_fold_index_updates_spanning_and_nested_list_properties_locally() {
    const ITEMS: u64 = 100_000;
    let list_semantics = ContainerFoldSemantics {
        descends_through_last_child: true,
        is_item: false,
        last_line_blank: false,
    };
    let item_semantics = ContainerFoldSemantics {
        descends_through_last_child: true,
        is_item: true,
        last_line_blank: false,
    };
    let empty = ClosedChildAggregate::default();
    let large_children = (0..ITEMS)
        .map(|index| DirectChildAggregate {
            child: ForestBlockId(1_000_000 + index),
            summary: empty,
        })
        .collect::<Vec<_>>();
    let containers = vec![
        ContainerFoldInput {
            container: ForestBlockId(100),
            semantics: list_semantics,
            children: vec![
                DirectChildAggregate {
                    child: ForestBlockId(101),
                    summary: empty,
                },
                DirectChildAggregate {
                    child: ForestBlockId(105),
                    summary: empty,
                },
            ],
        },
        ContainerFoldInput {
            container: ForestBlockId(101),
            semantics: item_semantics,
            children: vec![DirectChildAggregate {
                child: ForestBlockId(102),
                summary: empty,
            }],
        },
        ContainerFoldInput {
            container: ForestBlockId(102),
            semantics: list_semantics,
            children: vec![DirectChildAggregate {
                child: ForestBlockId(103),
                summary: empty,
            }],
        },
        ContainerFoldInput {
            container: ForestBlockId(200),
            semantics: list_semantics,
            children: large_children,
        },
    ];
    let mut arena = PageArena::new();
    let mut build = RecordForestReceipt::default();
    let index = ContainerChildFoldIndex::from_containers(&mut arena, &containers, &mut build)
        .expect("packed direct-child fold index");
    assert!(
        index
            .as_ref()
            .get(&arena, ForestBlockId(100))
            .unwrap()
            .0
            .unwrap()
            .list_is_tight()
    );
    assert!(
        index
            .as_ref()
            .get(&arena, ForestBlockId(102))
            .unwrap()
            .0
            .unwrap()
            .list_is_tight()
    );

    let large_pages = index
        .as_ref()
        .direct_child_page_count(&arena, ForestBlockId(200))
        .unwrap();
    let far_page = index
        .as_ref()
        .direct_child_page_at(&arena, ForestBlockId(200), large_pages - 1)
        .unwrap()
        .unwrap();
    let mut large_mutation = RecordForestReceipt::default();
    let large_changed = index
        .replace_child(
            &mut arena,
            ForestBlockId(200),
            50_001,
            ForestBlockId(1_050_001),
            ClosedChildAggregate {
                item_loose_if_nonlast: true,
                ..empty
            },
            &mut large_mutation,
        )
        .expect("one spanning-list child replacement");
    assert!(
        !large_changed
            .as_ref()
            .get(&arena, ForestBlockId(200))
            .unwrap()
            .0
            .unwrap()
            .list_is_tight()
    );
    assert_eq!(
        large_changed
            .as_ref()
            .direct_child_page_at(&arena, ForestBlockId(200), large_pages - 1)
            .unwrap(),
        Some(far_page)
    );
    assert!(large_mutation.pages_reused >= usize::try_from(large_pages).unwrap() + 2);
    assert!(large_mutation.nodes_visited < 128);
    assert!(large_mutation.branch_nodes_allocated < 128);

    let changed_item = ClosedChildAggregate {
        ends_blank: true,
        item_loose_if_nonlast: true,
        item_loose_if_last: true,
    };
    let mut nested_receipt = RecordForestReceipt::default();
    let inner_list = large_changed
        .replace_child(
            &mut arena,
            ForestBlockId(102),
            0,
            ForestBlockId(103),
            changed_item,
            &mut nested_receipt,
        )
        .expect("inner list contribution");
    let inner_view = inner_list
        .as_ref()
        .get(&arena, ForestBlockId(102))
        .unwrap()
        .0
        .unwrap();
    assert!(!inner_view.list_is_tight());
    let outer_item = inner_list
        .replace_child(
            &mut arena,
            ForestBlockId(101),
            0,
            ForestBlockId(102),
            inner_view.closed_summary(),
            &mut nested_receipt,
        )
        .expect("propagate inner list summary to outer item");
    let outer_item_view = outer_item
        .as_ref()
        .get(&arena, ForestBlockId(101))
        .unwrap()
        .0
        .unwrap();
    let outer_list = outer_item
        .replace_child(
            &mut arena,
            ForestBlockId(100),
            0,
            ForestBlockId(101),
            outer_item_view.closed_summary(),
            &mut nested_receipt,
        )
        .expect("propagate outer item summary to outer list");
    assert!(
        !outer_list
            .as_ref()
            .get(&arena, ForestBlockId(100))
            .unwrap()
            .0
            .unwrap()
            .list_is_tight()
    );
    assert!(nested_receipt.nodes_visited < 128);
    let metrics = arena.metrics();
    let direct_hundredths = build.payload_bytes_copied * 100 / usize::try_from(ITEMS).unwrap();
    eprintln!(
        "direct_child_index items={ITEMS} build_payload={} live_payload={} bytes_per_large_item={}.{:02} large_pages={} large_edit_nodes={} nested_edit_nodes={}",
        build.payload_bytes_copied,
        metrics.live_payload_bytes,
        direct_hundredths / 100,
        direct_hundredths % 100,
        large_pages,
        large_mutation.nodes_visited,
        nested_receipt.nodes_visited,
    );

    index.release_later(&mut arena).unwrap();
    large_changed.release_later(&mut arena).unwrap();
    inner_list.release_later(&mut arena).unwrap();
    outer_item.release_later(&mut arena).unwrap();
    outer_list.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn partial_snapshot_uses_overlay_first_and_revision_bound_presentation() {
    let coverage_order = order(1..=3);
    let mut arena = PageArena::new();
    let mut receipt = RecordForestReceipt::default();
    let records = BlockRecordTable::from_records(
        &mut arena,
        &[record(1, None, anchor(1, 0), anchor(2, 0), false)],
        &mut receipt,
    )
    .unwrap();
    let block_order = BlockOrder::from_entries(
        &mut arena,
        &[ForestBlockId(1), ForestBlockId(2), ForestBlockId(3)],
        64,
        &mut receipt,
    )
    .unwrap();
    let coverage = CoveragePartition::from_segments(
        &mut arena,
        &[CoverageSegment {
            owner: ForestBlockId(3),
            kind: CoverageSegmentKind::Gap,
            start: anchor(1, 0),
            end: anchor(2, 0),
        }],
        anchor(1, 0),
        anchor(2, 0),
        &coverage_order,
        &mut receipt,
    )
    .unwrap();
    let mut frontier = OpenOverlay::new();
    for frame in [
        OpenFrame {
            block: ForestBlockId(1),
            parent: None,
            kind_tag: 1,
            context: 10,
            start: anchor(1, 0),
            current: anchor(2, 0),
            pending: None,
        },
        OpenFrame {
            block: ForestBlockId(2),
            parent: Some(ForestBlockId(1)),
            kind_tag: 2,
            context: 20,
            start: anchor(1, 0),
            current: anchor(2, 0),
            pending: None,
        },
        OpenFrame {
            block: ForestBlockId(3),
            parent: Some(ForestBlockId(2)),
            kind_tag: 3,
            context: 30,
            start: anchor(1, 0),
            current: anchor(2, 0),
            pending: Some(ForestRunCursorId(44)),
        },
    ] {
        frontier.push(&mut arena, frame, &mut receipt).unwrap();
    }
    let unknown = flark_v3_runtime_slice::UnknownRange {
        start: Some(anchor(2, 0)),
        end: Some(anchor(3, 0)),
    };
    let snapshot = frontier.snapshot(&mut arena, unknown).unwrap();
    let enclosing = coverage
        .as_ref()
        .enclosing_blocks(
            &arena,
            anchor(1, 0),
            CoverageAffinity::Downstream,
            &coverage_order,
            records.as_ref(),
            Some(&snapshot),
            16,
        )
        .unwrap()
        .unwrap();
    assert_eq!(enclosing.blocks.len(), 3);
    assert!(
        matches!(enclosing.blocks[0], StructuralBlock::Open(value) if value.block == ForestBlockId(3))
    );
    assert!(
        matches!(enclosing.blocks[2], StructuralBlock::Open(value) if value.block == ForestBlockId(1))
    );
    assert_eq!(enclosing.receipt.frontier_nodes_visited, 3);

    let epoch = presentation_epoch();
    let request = presentation_request(anchor(1, 0), anchor(2, 0));
    let mut builder = PresentationFactBuilder::new(
        epoch,
        request,
        PresentationAuthority::INLINE_PROJECTION,
        PresentationBudget::hard_max(),
    );
    assert!(matches!(
        builder.push(PresentationFact::Style {
            range: request.range,
            style: PresentationStyleTag(7),
            layer: 1,
        }),
        flark_v3_runtime_slice::PresentationPushResult::Accepted
    ));
    let PresentationBuildOutcome::Exact { lease, .. } =
        builder.finish(&mut arena, &coverage_order).unwrap()
    else {
        panic!("one active fact fits");
    };
    frontier.release_later(&mut arena).unwrap();
    let manifest = RecordForestManifest::build(
        &mut arena,
        epoch,
        request.range,
        records,
        block_order,
        coverage,
        snapshot,
        Some(lease),
        &coverage_order,
        &mut receipt,
    )
    .expect("atomic composite root");
    settle(&mut arena);
    let components = manifest.components(&arena).unwrap();
    assert!(components.records.is_some());
    assert!(components.order.is_some());
    assert!(components.coverage.is_some());
    assert!(components.frontier.is_some());
    assert!(components.presentation.is_some());
    assert_eq!(manifest.unknown(&arena).unwrap(), unknown);
    assert!(matches!(
        manifest
            .query_presentation(&arena, epoch, request, &coverage_order)
            .unwrap(),
        PresentationLookup::Exact(exact) if exact.facts.len() == 1
    ));

    manifest.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn composite_rejects_cross_epoch_and_cross_range_presentation_atomically() {
    fn components(
        arena: &mut PageArena,
        coverage_order: &ExplicitCoverageOrder,
        presentation_epoch: PresentationEpoch,
        presentation_range: PresentationRange,
    ) -> (
        BlockRecordTable,
        BlockOrder,
        CoveragePartition,
        flark_v3_runtime_slice::OpenOverlaySnapshot,
        flark_v3_runtime_slice::PresentationFactLease,
    ) {
        let mut receipt = RecordForestReceipt::default();
        let records = BlockRecordTable::from_records(
            arena,
            &[record(1, None, anchor(1, 0), anchor(2, 0), false)],
            &mut receipt,
        )
        .unwrap();
        let block_order =
            BlockOrder::from_entries(arena, &[ForestBlockId(1)], 64, &mut receipt).unwrap();
        let coverage = CoveragePartition::from_segments(
            arena,
            &[CoverageSegment {
                owner: ForestBlockId(1),
                kind: CoverageSegmentKind::Gap,
                start: anchor(1, 0),
                end: anchor(2, 0),
            }],
            anchor(1, 0),
            anchor(2, 0),
            coverage_order,
            &mut receipt,
        )
        .unwrap();
        let snapshot = OpenOverlay::new()
            .snapshot(arena, flark_v3_runtime_slice::UnknownRange::default())
            .unwrap();
        let request = PresentationRequest {
            id: PresentationRequestId(99),
            scope: PresentationRequestScope::Viewport,
            range: presentation_range,
            required_authority: PresentationAuthority::INLINE_PROJECTION,
        };
        let PresentationBuildOutcome::Exact { lease, .. } = PresentationFactBuilder::new(
            presentation_epoch,
            request,
            PresentationAuthority::INLINE_PROJECTION,
            PresentationBudget::hard_max(),
        )
        .finish(arena, coverage_order)
        .unwrap() else {
            panic!("empty bounded presentation");
        };
        (records, block_order, coverage, snapshot, lease)
    }

    let coverage_order = order(1..=3);
    let forest_epoch = presentation_epoch();
    let structural_range = PresentationRange {
        start: anchor(1, 0),
        end: anchor(2, 0),
    };

    let mut epoch_arena = PageArena::new();
    let wrong_epoch = PresentationEpoch {
        source: SourceRevision(forest_epoch.source.0 + 1),
        ..forest_epoch
    };
    let (records, block_order, coverage, snapshot, lease) = components(
        &mut epoch_arena,
        &coverage_order,
        wrong_epoch,
        structural_range,
    );
    assert!(matches!(
        RecordForestManifest::build(
            &mut epoch_arena,
            forest_epoch,
            structural_range,
            records,
            block_order,
            coverage,
            snapshot,
            Some(lease),
            &coverage_order,
            &mut RecordForestReceipt::default(),
        ),
        Err(flark_v3_runtime_slice::RecordForestError::Invalid(
            "presentation epoch differs from forest epoch"
        ))
    ));
    settle(&mut epoch_arena);
    assert_eq!(epoch_arena.metrics().live_nodes, 0);

    let mut range_arena = PageArena::new();
    let outside_range = PresentationRange {
        start: anchor(1, 0),
        end: anchor(3, 0),
    };
    let (records, block_order, coverage, snapshot, lease) = components(
        &mut range_arena,
        &coverage_order,
        forest_epoch,
        outside_range,
    );
    assert!(matches!(
        RecordForestManifest::build(
            &mut range_arena,
            forest_epoch,
            structural_range,
            records,
            block_order,
            coverage,
            snapshot,
            Some(lease),
            &coverage_order,
            &mut RecordForestReceipt::default(),
        ),
        Err(flark_v3_runtime_slice::RecordForestError::Invalid(
            "presentation range exceeds structural authority"
        ))
    ));
    settle(&mut range_arena);
    assert_eq!(range_arena.metrics().live_nodes, 0);
}

#[test]
#[allow(clippy::too_many_lines)]
fn long_open_leaf_retains_bounded_forest_but_event_tape_retains_line_history() {
    const LINES: u64 = 100_000;
    const BYTES_PER_LINE: u64 = 100;
    let coverage_order = order(1..=LINES + 1);

    let mut forest_arena = PageArena::new();
    let mut forest_receipt = RecordForestReceipt::default();
    let records = BlockRecordTable::from_records(&mut forest_arena, &[], &mut forest_receipt)
        .expect("no finalized open leaf record");
    let block_order = BlockOrder::from_entries(
        &mut forest_arena,
        &[ForestBlockId(1)],
        64,
        &mut forest_receipt,
    )
    .unwrap();
    let coverage = CoveragePartition::from_segments(
        &mut forest_arena,
        &[CoverageSegment {
            owner: ForestBlockId(1),
            kind: CoverageSegmentKind::Gap,
            start: anchor(1, 0),
            end: anchor(LINES + 1, 0),
        }],
        anchor(1, 0),
        anchor(LINES + 1, 0),
        &coverage_order,
        &mut forest_receipt,
    )
    .unwrap();
    let mut frontier = OpenOverlay::new();
    frontier
        .push(
            &mut forest_arena,
            OpenFrame {
                block: ForestBlockId(1),
                parent: None,
                kind_tag: 3,
                context: 0,
                start: anchor(1, 0),
                current: anchor(LINES + 1, 0),
                pending: Some(ForestRunCursorId(1)),
            },
            &mut forest_receipt,
        )
        .unwrap();
    let snapshot = frontier
        .snapshot(
            &mut forest_arena,
            flark_v3_runtime_slice::UnknownRange::default(),
        )
        .unwrap();
    let presentation = empty_presentation(
        &mut forest_arena,
        &coverage_order,
        anchor(LINES, 0),
        anchor(LINES + 1, 0),
    );
    frontier.release_later(&mut forest_arena).unwrap();
    let manifest = RecordForestManifest::build(
        &mut forest_arena,
        presentation_epoch(),
        PresentationRange {
            start: anchor(1, 0),
            end: anchor(LINES + 1, 0),
        },
        records,
        block_order,
        coverage,
        snapshot,
        Some(presentation),
        &coverage_order,
        &mut forest_receipt,
    )
    .unwrap();
    settle(&mut forest_arena);
    let forest_retained = forest_arena.metrics();

    let mut tape_arena = PageArena::new();
    let mut tape_receipt = EventTapeReceipt::default();
    let mut projection = ProjectionState::new();
    let mut pages = Vec::new();
    let mut builder = EventPageBuilder::new(projection.checkpoint(&mut tape_arena).unwrap());
    for line in 0..LINES {
        let mut coverage_record = CoverageRecord {
            id: CoverageId(line + 1),
            length: Position {
                bytes: BYTES_PER_LINE,
                utf16: BYTES_PER_LINE,
            },
        };
        loop {
            match builder.push_coverage(coverage_record).unwrap() {
                PushResult::Accepted => break,
                PushResult::PageFull {
                    item,
                    continuation: None,
                } => {
                    pages.push(
                        builder
                            .seal_page(&mut tape_arena, &mut tape_receipt)
                            .unwrap(),
                    );
                    builder =
                        EventPageBuilder::new(projection.checkpoint(&mut tape_arena).unwrap());
                    coverage_record = item;
                }
                PushResult::PageFull { .. } => panic!("new line cannot continue old line"),
            }
        }
        if line == 0 {
            let event = StructuralEvent::Open {
                block: BlockId(1),
                parent: None,
                kind_tag: 3,
                start: event_anchor(1, 0),
            };
            assert_eq!(builder.push_event(event.clone()), Ok(PushResult::Accepted));
            projection
                .apply(&mut tape_arena, &event, &mut tape_receipt)
                .unwrap();
        }
        for (mut event, updates_projection) in [
            (
                StructuralEvent::AppendRuns {
                    block: BlockId(1),
                    runs: RunRangeId(line + 1),
                },
                false,
            ),
            (
                StructuralEvent::WriteEnd {
                    block: BlockId(1),
                    end: event_anchor(line + 1, 100),
                },
                true,
            ),
        ] {
            loop {
                match builder.push_event(event).unwrap() {
                    PushResult::Accepted => {
                        if updates_projection {
                            projection
                                .apply(
                                    &mut tape_arena,
                                    &StructuralEvent::WriteEnd {
                                        block: BlockId(1),
                                        end: event_anchor(line + 1, 100),
                                    },
                                    &mut tape_receipt,
                                )
                                .unwrap();
                        }
                        break;
                    }
                    PushResult::PageFull {
                        item,
                        continuation: Some(token),
                    } => {
                        pages.push(
                            builder
                                .seal_page(&mut tape_arena, &mut tape_receipt)
                                .unwrap(),
                        );
                        builder = EventPageBuilder::continuing(
                            projection.checkpoint(&mut tape_arena).unwrap(),
                            token,
                        );
                        event = item;
                    }
                    PushResult::PageFull { .. } => panic!("line event requires continuation"),
                }
            }
        }
        if line % 256 == 0 {
            tape_arena.poll_reclaim(512).unwrap();
        }
    }
    pages.push(
        builder
            .seal_page(&mut tape_arena, &mut tape_receipt)
            .unwrap(),
    );
    projection.release_later(&mut tape_arena).unwrap();
    let tape = OutputSequence::from_pages(&mut tape_arena, pages, &mut tape_receipt).unwrap();
    let tape_coverage = CoveragePartition::from_segments(
        &mut tape_arena,
        &[CoverageSegment {
            owner: ForestBlockId(1),
            kind: CoverageSegmentKind::Gap,
            start: anchor(1, 0),
            end: anchor(LINES + 1, 0),
        }],
        anchor(1, 0),
        anchor(LINES + 1, 0),
        &coverage_order,
        &mut RecordForestReceipt::default(),
    )
    .unwrap();
    let tape_presentation = empty_presentation(
        &mut tape_arena,
        &coverage_order,
        anchor(LINES, 0),
        anchor(LINES + 1, 0),
    );
    settle(&mut tape_arena);
    let tape_retained = tape_arena.metrics();
    // Output-layer discriminator only. Both sides include the same compact
    // coverage/presentation fixture; neither includes Crop/source lineage,
    // run directories, allocator metadata, or the heap-backed order oracle.
    // This receipt must not be presented as total runtime memory.
    assert!(forest_retained.live_payload_bytes < 2_048);
    assert!(tape_retained.live_payload_bytes > 7_000_000);
    assert!(tape_retained.live_payload_bytes > forest_retained.live_payload_bytes * 1_000);
    eprintln!(
        "long_open_leaf_output_layer forest_nodes={} forest_bytes={} tape_nodes={} tape_bytes={} tape_events={} tape_pages={}",
        forest_retained.live_nodes,
        forest_retained.live_payload_bytes,
        tape_retained.live_nodes,
        tape_retained.live_payload_bytes,
        tape.as_ref().summary(&tape_arena).unwrap().events,
        tape.as_ref().summary(&tape_arena).unwrap().pages,
    );

    manifest.release_later(&mut forest_arena).unwrap();
    settle(&mut forest_arena);
    tape.release_later(&mut tape_arena).unwrap();
    tape_coverage.release_later(&mut tape_arena).unwrap();
    tape_presentation.release_later(&mut tape_arena).unwrap();
    settle(&mut tape_arena);
    assert_eq!(forest_arena.metrics().live_nodes, 0);
    assert_eq!(tape_arena.metrics().live_nodes, 0);
}

#[test]
fn one_hundred_thousand_small_blocks_reports_exact_representation_cost() {
    const BLOCKS: u64 = 100_000;
    let coverage_order = order(1..=BLOCKS + 1);
    let records = (1..=BLOCKS)
        .map(|id| record(id, None, anchor(id, 0), anchor(id + 1, 0), true))
        .collect::<Vec<_>>();
    let ids = (1..=BLOCKS).map(ForestBlockId).collect::<Vec<_>>();
    let segments = (1..=BLOCKS)
        .map(|id| CoverageSegment {
            owner: ForestBlockId(id),
            kind: CoverageSegmentKind::Terminal,
            start: anchor(id, 0),
            end: anchor(id + 1, 0),
        })
        .collect::<Vec<_>>();
    let mut arena = PageArena::new();
    let mut receipt = RecordForestReceipt::default();
    let table = BlockRecordTable::from_records(&mut arena, &records, &mut receipt).unwrap();
    let block_order = BlockOrder::from_entries(&mut arena, &ids, 511, &mut receipt).unwrap();
    let coverage = CoveragePartition::from_segments(
        &mut arena,
        &segments,
        anchor(1, 0),
        anchor(BLOCKS + 1, 0),
        &coverage_order,
        &mut receipt,
    )
    .unwrap();
    let frontier = OpenOverlay::new();
    let snapshot = frontier
        .snapshot(&mut arena, flark_v3_runtime_slice::UnknownRange::default())
        .unwrap();
    let manifest = RecordForestManifest::build(
        &mut arena,
        presentation_epoch(),
        PresentationRange {
            start: anchor(1, 0),
            end: anchor(BLOCKS + 1, 0),
        },
        table,
        block_order,
        coverage,
        snapshot,
        None,
        &coverage_order,
        &mut receipt,
    )
    .unwrap();
    settle(&mut arena);
    let retained = arena.metrics();
    let hundredths = retained.live_payload_bytes * 100 / usize::try_from(BLOCKS).unwrap();
    eprintln!(
        "small_blocks blocks={BLOCKS} nodes={} bytes={} bytes_per_block={}.{:02}",
        retained.live_nodes,
        retained.live_payload_bytes,
        hundredths / 100,
        hundredths % 100,
    );
    // Algorithmically bounded and substantially below event history, but the
    // fixed AoS proof is intentionally a representation HOLD pending SoA/
    // delta-packed production pages.
    assert!((140..=145).contains(&(hundredths / 100)));
    manifest.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

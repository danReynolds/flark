#![allow(clippy::too_many_lines)]

use flark_v3_runtime_slice::{
    GenericAffinity, GenericBlockKind, GenericCoordinate, GenericGreenError, GenericGreenMetric,
    GenericNodeSpec, GenericPageReceipt, GenericPageSpec, GenericPieceSpec, GenericSourceKind,
    GenericViewportReceipt, VariableEdgeArena, build_generic_green_page,
    generic_page_external_children, generic_page_metric, generic_source_lookup,
    generic_viewport_atoms, splice_generic_page_pieces,
};

fn metric(bytes: u64, utf16: u64) -> GenericGreenMetric {
    GenericGreenMetric { bytes, utf16 }
}

fn node(block: u64, kind: GenericBlockKind, parent: Option<u16>) -> GenericNodeSpec {
    GenericNodeSpec {
        block,
        kind,
        parent,
    }
}

fn source(
    owner: u16,
    kind: GenericSourceKind,
    coverage: u64,
    bytes: u64,
    utf16: u64,
) -> GenericPieceSpec {
    GenericPieceSpec::Source {
        owner,
        kind,
        coverage,
        metric: metric(bytes, utf16),
    }
}

fn leaf(
    arena: &mut VariableEdgeArena,
    block: u64,
    kind: GenericBlockKind,
    coverage: u64,
    bytes: u64,
    utf16: u64,
) -> flark_v3_runtime_slice::GenericGreenRoot {
    build_generic_green_page(
        arena,
        &GenericPageSpec {
            nodes: vec![node(block, kind, None)],
            pieces: vec![source(
                0,
                GenericSourceKind::Terminal,
                coverage,
                bytes,
                utf16,
            )],
        },
        &mut GenericPageReceipt::default(),
    )
    .expect("generic leaf page")
}

fn settle(arena: &mut VariableEdgeArena) -> (usize, usize, usize) {
    let mut transitions = 0;
    let mut edges = 0;
    let mut pages = 0;
    while arena.metrics().pending_pages != 0 {
        let receipt = arena.poll_reclaim(1).expect("fuelled variable reclaim");
        assert!(receipt.transitions <= 1);
        transitions += receipt.transitions;
        edges += receipt.child_edges_released;
        pages += receipt.pages_reclaimed;
    }
    (transitions, edges, pages)
}

#[test]
fn one_page_owns_101_external_subtrees_without_proxy_nodes_and_retires_by_edge() {
    const CHILDREN: usize = 101;
    let mut arena = VariableEdgeArena::new();
    let mut children = Vec::new();
    for index in 0..CHILDREN {
        children.push(leaf(
            &mut arena,
            10_000 + index as u64,
            if index.is_multiple_of(3) {
                GenericBlockKind::Paragraph
            } else if index % 3 == 1 {
                GenericBlockKind::Heading
            } else {
                GenericBlockKind::FencedCode
            },
            20_000 + index as u64,
            1,
            1,
        ));
    }
    let child_ids = children
        .iter()
        .map(flark_v3_runtime_slice::GenericGreenRoot::id)
        .collect::<Vec<_>>();
    let mut parent_receipt = GenericPageReceipt::default();
    let parent = build_generic_green_page(
        &mut arena,
        &GenericPageSpec {
            nodes: vec![node(1, GenericBlockKind::Document, None)],
            pieces: child_ids
                .iter()
                .copied()
                .map(|child| GenericPieceSpec::External { owner: 0, child })
                .collect(),
        },
        &mut parent_receipt,
    )
    .expect("maximally dense external-edge page");
    assert_eq!(parent_receipt.payload_bytes, 3_280);
    assert_eq!(parent_receipt.edge_bytes, 808);
    assert_eq!(parent_receipt.total_storage_bytes, 4_088);
    assert_eq!(parent_receipt.external_edges, CHILDREN);
    assert_eq!(arena.metrics().live_nodes, CHILDREN + 1);
    assert_eq!(arena.metrics().live_edges, CHILDREN);
    assert_eq!(
        generic_page_external_children(&arena, parent.id()).unwrap(),
        child_ids
    );

    let replacement = leaf(&mut arena, 99_999, GenericBlockKind::Html, 99_999, 2, 2);
    let far_suffix = child_ids[CHILDREN - 1];
    let mut edit_receipt = GenericPageReceipt::default();
    let edited = splice_generic_page_pieces(
        &mut arena,
        parent.id(),
        50..51,
        &[GenericPieceSpec::External {
            owner: 0,
            child: replacement.id(),
        }],
        &mut edit_receipt,
    )
    .expect("one bounded parent-page rewrite");
    assert_eq!(edit_receipt.total_storage_bytes, 4_088);
    assert_eq!(
        generic_page_external_children(&arena, edited.id()).unwrap()[CHILDREN - 1],
        far_suffix
    );
    assert_eq!(
        generic_page_metric(&arena, edited.id()).unwrap(),
        metric(102, 102)
    );

    let overflow = leaf(
        &mut arena,
        100_000,
        GenericBlockKind::ThematicBreak,
        100_000,
        1,
        1,
    );
    let mut too_many = child_ids.clone();
    too_many.push(overflow.id());
    let error = build_generic_green_page(
        &mut arena,
        &GenericPageSpec {
            nodes: vec![node(2, GenericBlockKind::Document, None)],
            pieces: too_many
                .iter()
                .copied()
                .map(|child| GenericPieceSpec::External { owner: 0, child })
                .collect(),
        },
        &mut GenericPageReceipt::default(),
    )
    .unwrap_err();
    assert_eq!(error, GenericGreenError::StorageTooLarge(4_128));

    let before_release = arena.metrics();
    let accounted = before_release.live_storage_bytes + before_release.slot_storage_bytes;
    eprintln!(
        "generic_green_density children={CHILDREN} live_nodes={} live_edges={} storage={} payload={} edge_bytes={} slot_capacity={} slot_bytes={} accounted={} heap_allocations={} parent_storage={} rewrite_storage={} high_water_nodes={} high_water_storage={}",
        before_release.live_nodes,
        before_release.live_edges,
        before_release.live_storage_bytes,
        before_release.live_payload_bytes,
        before_release.live_edge_bytes,
        before_release.slot_capacity,
        before_release.slot_storage_bytes,
        accounted,
        before_release.heap_page_allocations,
        parent_receipt.total_storage_bytes,
        edit_receipt.total_storage_bytes,
        before_release.high_water_live_nodes,
        before_release.high_water_storage_bytes,
    );

    parent.release_later(&mut arena).unwrap();
    edited.release_later(&mut arena).unwrap();
    replacement.release_later(&mut arena).unwrap();
    overflow.release_later(&mut arena).unwrap();
    for child in children {
        child.release_later(&mut arena).unwrap();
    }
    let (_, released_edges, reclaimed_pages) = settle(&mut arena);
    assert_eq!(released_edges, CHILDREN * 2);
    assert_eq!(reclaimed_pages, CHILDREN + 4);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn interleaved_ancestor_markers_have_exact_byte_utf16_affinity_and_viewport_paths() {
    let mut arena = VariableEdgeArena::new();
    let root = build_generic_green_page(
        &mut arena,
        &GenericPageSpec {
            nodes: vec![
                node(1, GenericBlockKind::BlockQuote, None),
                node(2, GenericBlockKind::Paragraph, Some(0)),
            ],
            pieces: vec![
                source(0, GenericSourceKind::ContainerMarker, 11, 2, 2),
                source(1, GenericSourceKind::Terminal, 12, 3, 3),
                source(0, GenericSourceKind::ContainerMarker, 13, 2, 2),
                source(1, GenericSourceKind::Terminal, 14, 4, 2),
                source(0, GenericSourceKind::Gap, 15, 1, 1),
            ],
        },
        &mut GenericPageReceipt::default(),
    )
    .unwrap();
    assert_eq!(
        generic_page_metric(&arena, root.id()).unwrap(),
        metric(12, 10)
    );

    let upstream = generic_source_lookup(
        &arena,
        root.id(),
        2,
        GenericCoordinate::Bytes,
        GenericAffinity::Upstream,
    )
    .unwrap()
    .unwrap();
    assert_eq!(upstream.coverage, 11);
    assert_eq!(upstream.owner, 1);
    let downstream = generic_source_lookup(
        &arena,
        root.id(),
        2,
        GenericCoordinate::Bytes,
        GenericAffinity::Downstream,
    )
    .unwrap()
    .unwrap();
    assert_eq!(downstream.coverage, 12);
    assert_eq!(downstream.enclosing, [1, 2]);
    let continuation = generic_source_lookup(
        &arena,
        root.id(),
        5,
        GenericCoordinate::Bytes,
        GenericAffinity::Downstream,
    )
    .unwrap()
    .unwrap();
    assert_eq!(continuation.coverage, 13);
    assert_eq!(continuation.kind, GenericSourceKind::ContainerMarker);
    assert_eq!(continuation.owner, 1);
    assert_eq!(continuation.enclosing, [1]);
    let utf16_content = generic_source_lookup(
        &arena,
        root.id(),
        7,
        GenericCoordinate::Utf16,
        GenericAffinity::Downstream,
    )
    .unwrap()
    .unwrap();
    assert_eq!(utf16_content.coverage, 14);
    assert_eq!(utf16_content.enclosing, [1, 2]);

    let mut viewport_receipt = GenericViewportReceipt::default();
    let viewport = generic_viewport_atoms(
        &arena,
        root.id(),
        1..11,
        GenericCoordinate::Bytes,
        &mut viewport_receipt,
    )
    .unwrap();
    assert_eq!(
        viewport
            .iter()
            .map(|atom| atom.coverage)
            .collect::<Vec<_>>(),
        [11, 12, 13, 14]
    );
    assert_eq!(viewport[3].byte_start, 7);
    assert_eq!(viewport[3].byte_end, 11);
    assert_eq!(viewport[3].utf16_start, 7);
    assert_eq!(viewport[3].utf16_end, 9);
    assert_eq!(viewport_receipt.pages_visited, 1);
    assert_eq!(viewport_receipt.pieces_examined, 5);

    root.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn heterogeneous_tree_reparents_a_subtree_and_prefix_edit_keeps_distant_page_identity() {
    let mut arena = VariableEdgeArena::new();
    let a = leaf(&mut arena, 10, GenericBlockKind::Paragraph, 110, 3, 3);
    let b = leaf(&mut arena, 11, GenericBlockKind::FencedCode, 111, 4, 4);
    let c = leaf(&mut arena, 12, GenericBlockKind::Html, 112, 5, 5);
    let suffix = leaf(&mut arena, 13, GenericBlockKind::Heading, 113, 6, 6);
    let table = build_generic_green_page(
        &mut arena,
        &GenericPageSpec {
            nodes: vec![
                node(20, GenericBlockKind::Table, None),
                node(21, GenericBlockKind::TableRow, Some(0)),
                node(22, GenericBlockKind::TableCell, Some(1)),
                node(23, GenericBlockKind::Paragraph, Some(2)),
                node(24, GenericBlockKind::TableCell, Some(1)),
                node(25, GenericBlockKind::Paragraph, Some(4)),
            ],
            pieces: vec![
                source(3, GenericSourceKind::Terminal, 120, 2, 2),
                source(0, GenericSourceKind::ContainerMarker, 121, 1, 1),
                source(5, GenericSourceKind::Terminal, 122, 2, 2),
            ],
        },
        &mut GenericPageReceipt::default(),
    )
    .unwrap();
    let quote = build_generic_green_page(
        &mut arena,
        &GenericPageSpec {
            nodes: vec![node(30, GenericBlockKind::BlockQuote, None)],
            pieces: vec![
                GenericPieceSpec::External {
                    owner: 0,
                    child: a.id(),
                },
                GenericPieceSpec::External {
                    owner: 0,
                    child: b.id(),
                },
            ],
        },
        &mut GenericPageReceipt::default(),
    )
    .unwrap();
    let list = build_generic_green_page(
        &mut arena,
        &GenericPageSpec {
            nodes: vec![
                node(40, GenericBlockKind::List, None),
                node(41, GenericBlockKind::Item, Some(0)),
            ],
            pieces: vec![GenericPieceSpec::External {
                owner: 1,
                child: c.id(),
            }],
        },
        &mut GenericPageReceipt::default(),
    )
    .unwrap();
    let document = build_generic_green_page(
        &mut arena,
        &GenericPageSpec {
            nodes: vec![node(1, GenericBlockKind::Document, None)],
            pieces: vec![
                GenericPieceSpec::External {
                    owner: 0,
                    child: quote.id(),
                },
                GenericPieceSpec::External {
                    owner: 0,
                    child: list.id(),
                },
                GenericPieceSpec::External {
                    owner: 0,
                    child: table.id(),
                },
                GenericPieceSpec::External {
                    owner: 0,
                    child: suffix.id(),
                },
            ],
        },
        &mut GenericPageReceipt::default(),
    )
    .unwrap();

    let mut receipt = GenericPageReceipt::default();
    let quote_without_b =
        splice_generic_page_pieces(&mut arena, quote.id(), 1..2, &[], &mut receipt).unwrap();
    let list_with_b = splice_generic_page_pieces(
        &mut arena,
        list.id(),
        1..1,
        &[GenericPieceSpec::External {
            owner: 1,
            child: b.id(),
        }],
        &mut receipt,
    )
    .unwrap();
    let reparented = splice_generic_page_pieces(
        &mut arena,
        document.id(),
        0..2,
        &[
            GenericPieceSpec::External {
                owner: 0,
                child: quote_without_b.id(),
            },
            GenericPieceSpec::External {
                owner: 0,
                child: list_with_b.id(),
            },
        ],
        &mut receipt,
    )
    .unwrap();
    assert_eq!(
        generic_page_external_children(&arena, quote_without_b.id()).unwrap(),
        [a.id()]
    );
    assert_eq!(
        generic_page_external_children(&arena, list_with_b.id()).unwrap(),
        [c.id(), b.id()]
    );
    let reparented_children = generic_page_external_children(&arena, reparented.id()).unwrap();
    assert_eq!(reparented_children[2], table.id());
    assert_eq!(reparented_children[3], suffix.id());

    let a_edited = splice_generic_page_pieces(
        &mut arena,
        a.id(),
        0..1,
        &[source(0, GenericSourceKind::Terminal, 110, 7, 7)],
        &mut receipt,
    )
    .unwrap();
    let quote_prefix_edited = splice_generic_page_pieces(
        &mut arena,
        quote_without_b.id(),
        0..1,
        &[GenericPieceSpec::External {
            owner: 0,
            child: a_edited.id(),
        }],
        &mut receipt,
    )
    .unwrap();
    let prefix_edited = splice_generic_page_pieces(
        &mut arena,
        reparented.id(),
        0..1,
        &[GenericPieceSpec::External {
            owner: 0,
            child: quote_prefix_edited.id(),
        }],
        &mut receipt,
    )
    .unwrap();
    let prefix_children = generic_page_external_children(&arena, prefix_edited.id()).unwrap();
    assert_eq!(prefix_children[1], list_with_b.id());
    assert_eq!(prefix_children[2], table.id());
    assert_eq!(prefix_children[3], suffix.id());

    let table_hit = generic_source_lookup(
        &arena,
        prefix_edited.id(),
        16,
        GenericCoordinate::Bytes,
        GenericAffinity::Downstream,
    )
    .unwrap()
    .unwrap();
    assert_eq!(table_hit.coverage, 120);
    assert_eq!(table_hit.enclosing, [1, 20, 21, 22, 23]);
    let suffix_hit = generic_source_lookup(
        &arena,
        prefix_edited.id(),
        generic_page_metric(&arena, prefix_edited.id())
            .unwrap()
            .bytes,
        GenericCoordinate::Bytes,
        GenericAffinity::Upstream,
    )
    .unwrap()
    .unwrap();
    assert_eq!(suffix_hit.owner, 13);
    assert_eq!(suffix_hit.enclosing, [1, 13]);

    document.release_later(&mut arena).unwrap();
    reparented.release_later(&mut arena).unwrap();
    prefix_edited.release_later(&mut arena).unwrap();
    quote.release_later(&mut arena).unwrap();
    quote_without_b.release_later(&mut arena).unwrap();
    quote_prefix_edited.release_later(&mut arena).unwrap();
    list.release_later(&mut arena).unwrap();
    list_with_b.release_later(&mut arena).unwrap();
    table.release_later(&mut arena).unwrap();
    a.release_later(&mut arena).unwrap();
    a_edited.release_later(&mut arena).unwrap();
    b.release_later(&mut arena).unwrap();
    c.release_later(&mut arena).unwrap();
    suffix.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

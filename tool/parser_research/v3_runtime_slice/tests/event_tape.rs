use flark_v3_runtime_slice::{
    ARENA_PAGE_BYTES, BlockId, Coordinate, CoverageId, CoverageRecord, EventPageBuilder,
    EventTapeError, EventTapeReceipt, OccurrenceId, OutputRootManifest, OutputSequence, PageArena,
    PageRange, Position, ProjectionCheckpoint, ProjectionState, PushResult, RunRangeId,
    SourceAnchor, SourceStore, StructuralEvent, SymbolId, TerminalRecordId, ValueCursorId,
    read_event_page,
};

fn anchor(coverage: u64, bytes: u32, utf16: u32) -> SourceAnchor {
    SourceAnchor {
        coverage: CoverageId(coverage),
        local_bytes: bytes,
        local_utf16: utf16,
    }
}

fn one_page(
    arena: &mut PageArena,
    coverage: CoverageRecord,
    event: StructuralEvent,
    receipt: &mut EventTapeReceipt,
) -> flark_v3_runtime_slice::SealedEventPage {
    let mut builder = EventPageBuilder::new(ProjectionCheckpoint::empty());
    assert_eq!(builder.push_coverage(coverage), Ok(PushResult::Accepted));
    assert_eq!(builder.push_event(event), Ok(PushResult::Accepted));
    builder.seal_page(arena, receipt).expect("seal page")
}

#[test]
fn every_scalar_event_round_trips_with_page_relative_generation_safe_stamps() {
    let events = vec![
        StructuralEvent::Open {
            block: BlockId(1),
            parent: None,
            kind_tag: 7,
            start: anchor(11, 0, 0),
        },
        StructuralEvent::Promote {
            block: BlockId(1),
            kind_tag: 8,
            context: 91,
        },
        StructuralEvent::AppendRuns {
            block: BlockId(1),
            runs: RunRangeId(12),
        },
        StructuralEvent::DrainRunPrefix {
            block: BlockId(1),
            logical_bytes: 13,
        },
        StructuralEvent::WriteEnd {
            block: BlockId(1),
            end: anchor(11, 14, 12),
        },
        StructuralEvent::RepairListEnds {
            list: BlockId(1),
            first: BlockId(2),
            last: BlockId(3),
        },
        StructuralEvent::Definition {
            occurrence: OccurrenceId(15),
            symbol: SymbolId(16),
            value: ValueCursorId(17),
            origin: anchor(11, 3, 3),
        },
        StructuralEvent::Finalize {
            block: BlockId(1),
            terminal: TerminalRecordId(18),
        },
        StructuralEvent::Close { block: BlockId(1) },
    ];
    let mut arena = PageArena::new();
    let mut receipt = EventTapeReceipt::default();
    let mut builder = EventPageBuilder::new(ProjectionCheckpoint::empty());
    assert_eq!(
        builder.push_coverage(CoverageRecord {
            id: CoverageId(11),
            length: Position {
                bytes: 14,
                utf16: 12,
            },
        }),
        Ok(PushResult::Accepted)
    );
    for event in &events {
        assert_eq!(builder.push_event(event.clone()), Ok(PushResult::Accepted));
    }
    let page = builder
        .seal_page(&mut arena, &mut receipt)
        .expect("all scalar events fit");
    let page_id = page.id();
    let view = read_event_page(&arena, page_id).expect("decode packed page");
    assert_eq!(view.packed_bytes, receipt.event_payload_bytes_copied);
    assert!(view.packed_bytes <= ARENA_PAGE_BYTES);
    assert_eq!(
        view.events
            .iter()
            .map(|stamped| stamped.event.clone())
            .collect::<Vec<_>>(),
        events
    );
    for (index, stamped) in view.events.iter().enumerate() {
        assert_eq!(stamped.stamp.page, page_id);
        assert_eq!(usize::from(stamped.stamp.local_event), index);
    }

    let sequence =
        OutputSequence::from_pages(&mut arena, vec![page], &mut receipt).expect("adopt page owner");
    let manifest = OutputRootManifest::build(&mut arena, sequence, &mut receipt)
        .expect("manifest adopts sequence owner");
    while arena.metrics().pending_releases > 0 {
        arena.poll_reclaim(8).expect("settle ownership transfers");
    }
    let output = manifest.events(&arena).expect("manifest sequence");
    assert_eq!(output.summary(&arena).expect("summary").pages, 1);
    assert_eq!(
        output
            .locate_page(&arena, 0)
            .expect("query")
            .expect("page")
            .page,
        page_id
    );
    manifest
        .release_later(&mut arena)
        .expect("release manifest owner");
    while arena.metrics().pending_releases > 0 {
        arena.poll_reclaim(8).expect("retire manifest graph");
    }
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
#[allow(clippy::too_many_lines)] // One history proves packing, projection, and query composition.
fn one_physical_line_spans_many_pages_and_next_line_cannot_share_a_continuation() {
    const APPENDS: u64 = 1_200;
    let mut arena = PageArena::new();
    let mut receipt = EventTapeReceipt::default();
    let mut projection = ProjectionState::new();
    let mut pages = Vec::new();
    let mut builder = EventPageBuilder::new(
        projection
            .checkpoint(&mut arena)
            .expect("empty projection checkpoint"),
    );
    let line_a = CoverageRecord {
        id: CoverageId(1),
        length: Position { bytes: 9, utf16: 7 },
    };
    assert_eq!(builder.push_coverage(line_a), Ok(PushResult::Accepted));

    let open = StructuralEvent::Open {
        block: BlockId(7),
        parent: None,
        kind_tag: 3,
        start: anchor(1, 0, 0),
    };
    match builder.push_event(open.clone()).expect("bounded event") {
        PushResult::Accepted => projection
            .apply(&mut arena, &open, &mut receipt)
            .expect("open projection"),
        PushResult::PageFull { .. } => panic!("open fits the first page"),
    }

    for run in 0..APPENDS {
        let mut pending = StructuralEvent::AppendRuns {
            block: BlockId(7),
            runs: RunRangeId(run),
        };
        loop {
            match builder.push_event(pending).expect("bounded append") {
                PushResult::Accepted => break,
                PushResult::PageFull {
                    item,
                    continuation: Some(token),
                } => {
                    pages.push(
                        builder
                            .seal_page(&mut arena, &mut receipt)
                            .expect("seal full line page"),
                    );
                    builder = EventPageBuilder::continuing(
                        projection
                            .checkpoint(&mut arena)
                            .expect("open projection checkpoint"),
                        token,
                    );
                    while arena.metrics().pending_releases > 0 {
                        arena
                            .poll_reclaim(8)
                            .expect("builder checkpoint survives background reclaim");
                    }
                    pending = item;
                }
                PushResult::PageFull {
                    continuation: None, ..
                } => panic!("a line event must carry continuation authority"),
            }
        }
    }
    assert!(pages.len() >= 2, "the still-open line already filled pages");

    let line_b = CoverageRecord {
        id: CoverageId(2),
        length: Position { bytes: 2, utf16: 2 },
    };
    assert_eq!(
        builder.push_coverage(line_b),
        Ok(PushResult::PageFull {
            item: line_b,
            continuation: None,
        }),
        "a new line makes a dedicated continuation seal first"
    );
    pages.push(
        builder
            .seal_page(&mut arena, &mut receipt)
            .expect("seal final continuation"),
    );
    let line_a_pages = u64::try_from(pages.len()).expect("small test");
    assert!(line_a_pages >= 3);

    let continuation_roots = pages[1..]
        .iter()
        .map(|page| {
            read_event_page(&arena, page.id())
                .expect("continuation page")
                .leading_projection
        })
        .collect::<Vec<_>>();
    assert!(
        continuation_roots
            .iter()
            .all(|root| *root == projection.root())
    );
    assert_eq!(receipt.projection_nodes_allocated, 1);

    builder = EventPageBuilder::new(
        projection
            .checkpoint(&mut arena)
            .expect("next-line projection checkpoint"),
    );
    assert_eq!(builder.push_coverage(line_b), Ok(PushResult::Accepted));
    let close = StructuralEvent::Close { block: BlockId(7) };
    assert_eq!(builder.push_event(close.clone()), Ok(PushResult::Accepted));
    projection
        .apply(&mut arena, &close, &mut receipt)
        .expect("close projection");
    while arena.metrics().pending_releases > 0 {
        arena
            .poll_reclaim(8)
            .expect("unsealed page retains its leading checkpoint");
    }
    pages.push(
        builder
            .seal_page(&mut arena, &mut receipt)
            .expect("seal next line"),
    );
    projection
        .release_later(&mut arena)
        .expect("pages own their checkpoints");

    let sequence = OutputSequence::from_pages(&mut arena, pages, &mut receipt)
        .expect("build persistent sequence");
    while arena.metrics().pending_releases > 0 {
        arena.poll_reclaim(64).expect("settle transfers");
    }
    let output = sequence.as_ref();
    eprintln!(
        "mid_line pages={line_a_pages} events={} max_temporary_event_bytes={} projection_nodes={} projection_path_visits={}",
        output.summary(&arena).expect("mid-line summary").events,
        receipt.maximum_temporary_event_bytes,
        receipt.projection_nodes_allocated,
        receipt.projection_path_nodes_visited,
    );
    let start = output
        .locate_coordinate(&arena, Coordinate::Byte, 0)
        .expect("coordinate lookup")
        .expect("line A");
    assert_eq!(start.coverage, CoverageId(1));
    assert_eq!(
        start.page_range,
        PageRange {
            start: 0,
            end: line_a_pages
        }
    );
    let line_b_start = output
        .locate_coordinate(&arena, Coordinate::Byte, 9)
        .expect("boundary lookup")
        .expect("line B");
    assert_eq!(line_b_start.coverage, CoverageId(2));
    assert_eq!(line_b_start.page_range.start, line_a_pages);
    assert_eq!(line_b_start.page_range.end, line_a_pages + 1);
    let document_end = output
        .locate_coordinate(&arena, Coordinate::Byte, 11)
        .expect("end lookup")
        .expect("document end");
    assert_eq!(document_end.coverage, CoverageId(2));
    assert!(document_end.at_document_end);
    assert_eq!(document_end.local_offset, 2);
    let utf16_boundary = output
        .locate_coordinate(&arena, Coordinate::Utf16, 7)
        .expect("UTF-16 boundary lookup")
        .expect("line B in UTF-16");
    assert_eq!(utf16_boundary.coverage, CoverageId(2));
    assert_eq!(utf16_boundary.page_range, line_b_start.page_range);

    let a_projection = output
        .fold_coverage_location(&arena, start, 8)
        .expect("fold all pages of line A");
    assert_eq!(a_projection.frames.len(), 1);
    assert_eq!(a_projection.frames[0].block, BlockId(7));
    assert_eq!(
        a_projection.pages_folded,
        usize::try_from(line_a_pages).unwrap()
    );
    assert_eq!(a_projection.earlier_pages_replayed, 0);
    let distant_projection = output
        .fold_viewport(
            &arena,
            PageRange {
                start: line_a_pages - 1,
                end: line_a_pages,
            },
            8,
        )
        .expect("last continuation starts from its checkpoint");
    assert_eq!(distant_projection.frames.len(), 1);
    assert_eq!(distant_projection.frames[0].block, BlockId(7));
    assert_eq!(distant_projection.checkpoint_nodes_visited, 1);
    assert_eq!(distant_projection.pages_folded, 1);
    assert_eq!(distant_projection.earlier_pages_replayed, 0);
    let b_projection = output
        .fold_coverage_location(&arena, line_b_start, 8)
        .expect("fold line B from its checkpoint");
    assert!(b_projection.frames.is_empty());
    assert_eq!(b_projection.checkpoint_nodes_visited, 1);
    assert_eq!(b_projection.earlier_pages_replayed, 0);

    sequence
        .release_later(&mut arena)
        .expect("release event sequence");
    while arena.metrics().pending_releases > 0 {
        let reclaim = arena.poll_reclaim(7).expect("fuelled graph retirement");
        assert!(reclaim.reference_transitions <= 7);
        assert!(reclaim.payload_bytes_reclaimed <= 7 * ARENA_PAGE_BYTES);
    }
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
#[allow(clippy::too_many_lines)] // Keep the 65,536-page receipt assertions together.
fn unicode_prefix_insertion_reuses_65536_suffix_pages_and_a_large_subtree() {
    const PAGE_COUNT: usize = 65_536;
    let mut arena = PageArena::new();
    let mut build = EventTapeReceipt::default();
    let mut pages = Vec::with_capacity(PAGE_COUNT);
    for index in 0..PAGE_COUNT {
        pages.push(one_page(
            &mut arena,
            CoverageRecord {
                id: CoverageId(u64::try_from(index + 1).unwrap()),
                length: Position { bytes: 1, utf16: 1 },
            },
            StructuralEvent::Finalize {
                block: BlockId(u64::try_from(index + 1).unwrap()),
                terminal: TerminalRecordId(u64::try_from(index + 1).unwrap()),
            },
            &mut build,
        ));
    }
    let original =
        OutputSequence::from_pages(&mut arena, pages, &mut build).expect("balanced original");
    while arena.metrics().pending_releases > 0 {
        arena.poll_reclaim(4096).expect("settle original build");
    }
    let original_ref = original.as_ref();
    let original_summary = original_ref.summary(&arena).expect("summary");
    assert_eq!(original_summary.pages, PAGE_COUNT as u64);
    assert_eq!(original_summary.height, 17);
    let large_suffix = original_ref
        .right_partition_root(&arena)
        .expect("partition")
        .expect("branch");
    let suffix_page = original_ref
        .locate_page(&arena, 60_000)
        .expect("query")
        .expect("suffix page")
        .page;
    let suffix_stamp = read_event_page(&arena, suffix_page)
        .expect("suffix events")
        .events[0]
        .stamp;

    let mut mutation = EventTapeReceipt::default();
    let inserted = one_page(
        &mut arena,
        CoverageRecord {
            id: CoverageId(100_000),
            length: Position { bytes: 4, utf16: 2 },
        },
        StructuralEvent::Finalize {
            block: BlockId(100_000),
            terminal: TerminalRecordId(100_000),
        },
        &mut mutation,
    );
    let edited = original
        .splice_pages(&mut arena, 0..0, vec![inserted], &mut mutation)
        .expect("persistent prefix insert");
    let edited_ref = edited.as_ref();
    assert_eq!(edited_ref.summary(&arena).expect("summary").pages, 65_537);
    assert_eq!(
        edited_ref
            .right_partition_root(&arena)
            .expect("edited partition"),
        Some(large_suffix),
        "the unchanged 32,768-page suffix partition keeps exact identity"
    );
    let reused_suffix = edited_ref
        .locate_page(&arena, 60_001)
        .expect("query")
        .expect("shifted suffix");
    assert_eq!(reused_suffix.page, suffix_page);
    assert_eq!(
        read_event_page(&arena, reused_suffix.page)
            .expect("reused events")
            .events[0]
            .stamp,
        suffix_stamp,
        "prefix event count cannot rebase a suffix stamp"
    );
    assert_eq!(
        reused_suffix.coverage_prefix,
        Position {
            bytes: 60_004,
            utf16: 60_002,
        }
    );
    assert_eq!(mutation.event_pages_allocated, 1);
    assert_eq!(mutation.pages_reused, PAGE_COUNT);
    assert!(
        mutation.sequence_branch_nodes_allocated <= 4 * usize::from(original_summary.height) + 4,
        "receipt={mutation:?}"
    );
    assert!(
        mutation.sequence_nodes_visited <= 8 * usize::from(original_summary.height) + 16,
        "receipt={mutation:?}"
    );
    eprintln!(
        "prefix_insert pages={} old_height={} new_height={} mutation={mutation:?}",
        PAGE_COUNT,
        original_summary.height,
        edited_ref.summary(&arena).expect("edited summary").height,
    );

    original
        .release_later(&mut arena)
        .expect("release original");
    edited.release_later(&mut arena).expect("release edited");
    while arena.metrics().pending_releases > 0 {
        arena.poll_reclaim(4096).expect("retire shared trees");
    }
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
#[allow(clippy::too_many_lines)] // Retention is measured through the real page protocol.
fn ten_megabyte_open_leaf_reports_full_event_history_retention() {
    const LINES: u64 = 100_000;
    const BYTES_PER_LINE: u64 = 100;
    let mut arena = PageArena::new();
    let mut receipt = EventTapeReceipt::default();
    let mut projection = ProjectionState::new();
    let mut pages = Vec::new();
    let mut builder =
        EventPageBuilder::new(projection.checkpoint(&mut arena).expect("empty checkpoint"));

    for line in 0..LINES {
        let mut coverage = CoverageRecord {
            id: CoverageId(line + 1),
            length: Position {
                bytes: BYTES_PER_LINE,
                utf16: BYTES_PER_LINE,
            },
        };
        loop {
            match builder
                .push_coverage(coverage)
                .expect("coverage fits empty page")
            {
                PushResult::Accepted => break,
                PushResult::PageFull {
                    item,
                    continuation: None,
                } => {
                    pages.push(
                        builder
                            .seal_page(&mut arena, &mut receipt)
                            .expect("seal page"),
                    );
                    builder = EventPageBuilder::new(
                        projection.checkpoint(&mut arena).expect("page checkpoint"),
                    );
                    coverage = item;
                }
                PushResult::PageFull {
                    continuation: Some(_),
                    ..
                } => panic!("a new line never continues the previous line"),
            }
        }
        if line == 0 {
            let open = StructuralEvent::Open {
                block: BlockId(1),
                parent: None,
                kind_tag: 3,
                start: anchor(1, 0, 0),
            };
            assert_eq!(builder.push_event(open.clone()), Ok(PushResult::Accepted));
            projection
                .apply(&mut arena, &open, &mut receipt)
                .expect("open long leaf");
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
                    end: anchor(line + 1, 100, 100),
                },
                true,
            ),
        ] {
            loop {
                match builder.push_event(event).expect("scalar event") {
                    PushResult::Accepted => {
                        if updates_projection {
                            let write = StructuralEvent::WriteEnd {
                                block: BlockId(1),
                                end: anchor(line + 1, 100, 100),
                            };
                            projection
                                .apply(&mut arena, &write, &mut receipt)
                                .expect("advance projection end");
                        }
                        break;
                    }
                    PushResult::PageFull {
                        item,
                        continuation: Some(token),
                    } => {
                        pages.push(
                            builder
                                .seal_page(&mut arena, &mut receipt)
                                .expect("seal line split"),
                        );
                        builder = EventPageBuilder::continuing(
                            projection
                                .checkpoint(&mut arena)
                                .expect("continuation checkpoint"),
                            token,
                        );
                        event = item;
                    }
                    PushResult::PageFull {
                        continuation: None, ..
                    } => panic!("line event carries continuation authority"),
                }
            }
        }
        if line % 256 == 0 {
            arena.poll_reclaim(512).expect("bounded projection cleanup");
        }
    }
    pages.push(
        builder
            .seal_page(&mut arena, &mut receipt)
            .expect("seal final long-leaf page"),
    );
    projection
        .release_later(&mut arena)
        .expect("release active projection");
    let sequence = OutputSequence::from_pages(&mut arena, pages, &mut receipt)
        .expect("persist long event tape");
    while arena.metrics().pending_releases > 0 {
        arena.poll_reclaim(4096).expect("settle long tape");
    }
    let summary = sequence.as_ref().summary(&arena).expect("long summary");
    let retained = arena.metrics();
    assert_eq!(summary.coverage.bytes, 10_000_000);
    assert_eq!(summary.events, 2 * LINES + 1);
    let bytes_per_line_hundredths = retained
        .live_payload_bytes
        .checked_mul(100)
        .expect("small retention receipt")
        / usize::try_from(LINES).expect("line count fits usize");
    eprintln!(
        "long_open_leaf source_bytes={} lines={LINES} events={} event_pages={} event_payload_bytes={} arena_live_nodes={} arena_live_payload_bytes={} payload_bytes_per_line={}.{:02}",
        summary.coverage.bytes,
        summary.events,
        summary.pages,
        receipt.event_payload_bytes_copied,
        retained.live_nodes,
        retained.live_payload_bytes,
        bytes_per_line_hundredths / 100,
        bytes_per_line_hundredths % 100,
    );
    sequence
        .release_later(&mut arena)
        .expect("release long tape");
    while arena.metrics().pending_releases > 0 {
        let reclaim = arena.poll_reclaim(1024).expect("fuelled long retirement");
        assert!(reclaim.reference_transitions <= 1024);
        assert!(reclaim.payload_bytes_reclaimed <= 1024 * ARENA_PAGE_BYTES);
    }
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn output_outlives_no_strong_or_weak_crop_root_and_reclaims_with_fuel() {
    let store = SourceStore::new("😀 source\r\n", 8);
    let root = store.query_snapshot();
    let weak = root.weak_observer_for_testing();
    drop(root);

    let mut arena = PageArena::new();
    let mut receipt = EventTapeReceipt::default();
    let page = one_page(
        &mut arena,
        CoverageRecord {
            id: CoverageId(store.root_id().0),
            length: Position {
                bytes: 13,
                utf16: 11,
            },
        },
        StructuralEvent::AppendRuns {
            block: BlockId(1),
            runs: RunRangeId(1),
        },
        &mut receipt,
    );
    let sequence =
        OutputSequence::from_pages(&mut arena, vec![page], &mut receipt).expect("sequence");
    let manifest = OutputRootManifest::build(&mut arena, sequence, &mut receipt).expect("manifest");
    let manifest_id = manifest.id();
    while arena.metrics().pending_releases > 0 {
        arena.poll_reclaim(4).expect("settle adoption");
    }

    drop(store);
    assert!(weak.upgrade().is_none());
    let output = manifest.events(&arena).expect("output remains queryable");
    assert_eq!(output.summary(&arena).expect("summary").pages, 1);
    assert_eq!(output.summary(&arena).expect("summary").coverage.bytes, 13);

    manifest
        .release_later(&mut arena)
        .expect("release manifest");
    while arena.metrics().pending_releases > 0 {
        let reclaim = arena.poll_reclaim(1).expect("one transition at a time");
        assert!(reclaim.reference_transitions <= 1);
        assert!(reclaim.nodes_reclaimed <= 1);
        assert!(reclaim.payload_bytes_reclaimed <= ARENA_PAGE_BYTES);
    }
    assert_eq!(arena.metrics().live_nodes, 0);
    assert!(matches!(
        arena.payload(manifest_id.0),
        Err(flark_v3_runtime_slice::ArenaError::StaleId(_))
    ));
}

#[test]
fn rejected_projection_and_splice_inputs_do_not_leak_ownership() {
    let mut arena = PageArena::new();
    let mut receipt = EventTapeReceipt::default();
    let mut projection = ProjectionState::new();
    let wrong_parent = StructuralEvent::Open {
        block: BlockId(2),
        parent: Some(BlockId(99)),
        kind_tag: 1,
        start: anchor(1, 0, 0),
    };
    let before = arena.metrics();
    assert!(matches!(
        projection.apply(&mut arena, &wrong_parent, &mut receipt),
        Err(EventTapeError::ProjectionMismatch(_))
    ));
    assert_eq!(projection.depth(), 0);
    assert_eq!(arena.metrics(), before);

    let page = one_page(
        &mut arena,
        CoverageRecord {
            id: CoverageId(1),
            length: Position { bytes: 1, utf16: 1 },
        },
        StructuralEvent::Finalize {
            block: BlockId(1),
            terminal: TerminalRecordId(1),
        },
        &mut receipt,
    );
    let sequence =
        OutputSequence::from_pages(&mut arena, vec![page], &mut receipt).expect("sequence");
    let before = arena.metrics();
    let mut failed = EventTapeReceipt::default();
    assert!(matches!(
        sequence.splice_pages(&mut arena, 2..3, Vec::new(), &mut failed),
        Err(EventTapeError::InvalidPageRange)
    ));
    assert_eq!(failed, EventTapeReceipt::default());
    assert_eq!(arena.metrics(), before);
    assert_eq!(
        sequence.as_ref().summary(&arena).expect("still live").pages,
        1
    );
    sequence
        .release_later(&mut arena)
        .expect("release sequence");
    while arena.metrics().pending_releases > 0 {
        arena.poll_reclaim(4).expect("retire");
    }
    assert_eq!(arena.metrics().live_nodes, 0);
}

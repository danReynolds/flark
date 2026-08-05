use flark_v3_runtime_slice::{
    AtomicProjection, AtomicProjectionKind, BlockId, ClosedChildAggregate, CoverageId,
    CoveragePart, FactField, FactId, FactsEnvelope, GrammarRevision, GreenAffinity,
    GreenCoordinate, GreenEvent, GreenKind, GreenProjectionPosition, GreenTableCellOpenFacts,
    GreenTableOpenFacts, LogicalContribution, LogicalSegmentMapping, PageArena, ParseGeneration,
    ProjectionPiece, ProjectionProgram, SerializedGreenBuildReceipt, SerializedGreenDocument,
    SerializedGreenError, SerializedGreenRootSpec, SerializedMetric, SourceProjectionRun,
    SourceRevision, SourceRootId, VirtualProjectionKind,
};

fn root_spec(bytes: u64) -> SerializedGreenRootSpec {
    root_spec_with_utf16(bytes, bytes)
}

fn root_spec_with_utf16(bytes: u64, utf16: u64) -> SerializedGreenRootSpec {
    SerializedGreenRootSpec {
        syntax_profile: 1,
        source_revision: SourceRevision(1),
        source_root: SourceRootId(1),
        source_bytes: bytes,
        source_utf16: utf16,
        grammar_revision: GrammarRevision(1),
        parse_generation: ParseGeneration(1),
        semantic_epoch: 1,
        known_bytes: 0..bytes,
    }
}

fn enter(block: u64, kind: GreenKind, facts: FactsEnvelope) -> GreenEvent {
    GreenEvent::enter(BlockId(block), kind, facts)
}

fn exit() -> GreenEvent {
    GreenEvent::exit(ClosedChildAggregate::default())
}

fn physical(
    id: u64,
    bytes: u64,
    utf16: u64,
    owner_relative_depth: u32,
    part: CoveragePart,
) -> GreenEvent {
    GreenEvent::Coverage(
        SourceProjectionRun::new(CoverageId(id), bytes, utf16, owner_relative_depth, part).unwrap(),
    )
}

fn projected(
    id: u64,
    metric: SerializedMetric,
    owner_relative_depth: u32,
    part: CoveragePart,
    logical_target: u64,
    contribution: LogicalContribution,
) -> GreenEvent {
    GreenEvent::Coverage(
        SourceProjectionRun::with_logical(
            CoverageId(id),
            metric.bytes,
            metric.utf16,
            owner_relative_depth,
            part,
            BlockId(logical_target),
            contribution,
        )
        .unwrap(),
    )
}

fn settle(arena: &mut PageArena) {
    while arena.metrics().pending_releases != 0 {
        arena.poll_reclaim(10_000).unwrap();
    }
}

#[test]
#[allow(clippy::too_many_lines)] // Full cursor receipt pins one partial-tab ownership history.
fn continuation_line_partial_tab_is_terminal_owned_and_consumer_is_derived_from_structure() {
    let events = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::BLOCK_QUOTE, FactsEnvelope::empty()),
        enter(3, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        physical(1, 2, 2, 1, CoveragePart::CONTAINER_MARKER),
        projected(
            2,
            SerializedMetric { bytes: 1, utf16: 1 },
            0,
            CoveragePart::CONTENT,
            3,
            LogicalContribution::Atomic(AtomicProjection::tab_to_spaces(3).unwrap()),
        ),
        projected(
            3,
            SerializedMetric { bytes: 6, utf16: 3 },
            0,
            CoveragePart::CONTENT,
            3,
            LogicalContribution::Identity,
        ),
        projected(
            4,
            SerializedMetric { bytes: 2, utf16: 2 },
            0,
            CoveragePart::CONTENT,
            3,
            LogicalContribution::Atomic(AtomicProjection::crlf_to_lf()),
        ),
        projected(
            5,
            SerializedMetric { bytes: 1, utf16: 1 },
            0,
            CoveragePart::CONTENT,
            3,
            LogicalContribution::Identity,
        ),
        physical(6, 1, 1, 0, CoveragePart::TERMINAL),
        exit(),
        exit(),
        exit(),
    ];
    let mut arena = PageArena::new();
    let document = SerializedGreenDocument::build(
        &mut arena,
        root_spec_with_utf16(13, 10),
        events,
        &mut SerializedGreenBuildReceipt::default(),
    )
    .unwrap();

    let mut source = document
        .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
        .unwrap();
    let marker = source.next_coverage(&document, &arena).unwrap().unwrap();
    assert_eq!(marker.owner.block, BlockId(2));
    assert!(marker.logical_consumer.is_none());
    let paragraph = source
        .open_path()
        .iter()
        .find(|frame| frame.kind == GreenKind::PARAGRAPH)
        .unwrap()
        .enter;
    let tab = source.next_coverage(&document, &arena).unwrap().unwrap();
    assert_eq!(tab.owner.block, BlockId(3));
    assert_eq!(tab.logical_consumer.as_ref().unwrap().block, BlockId(3));

    let mut logical = document.logical_cursor(&arena, paragraph).unwrap();
    let tab = logical.next_segment(&document, &arena).unwrap().unwrap();
    assert_eq!(
        tab.mapping,
        LogicalSegmentMapping::AtomicAmbiguity {
            transform: AtomicProjectionKind::TabToSpaces { spaces: 3 }
        }
    );
    assert_eq!(tab.byte_range, 2..3);
    assert_eq!(tab.logical_byte_range, 0..3);
    assert!(matches!(
        tab.map_logical(GreenCoordinate::Bytes, 1),
        Some(GreenProjectionPosition::AtomicAmbiguity {
            physical,
            logical,
            transform: AtomicProjectionKind::TabToSpaces { spaces: 3 },
        }) if physical == (2..3) && logical == (0..3)
    ));
    let unicode = logical.next_segment(&document, &arena).unwrap().unwrap();
    assert_eq!(unicode.byte_range, 3..9);
    assert_eq!(unicode.utf16_range, 3..6);
    assert_eq!(unicode.logical_byte_range, 3..9);
    assert_eq!(unicode.logical_utf16_range, 3..6);
    let crlf = logical.next_segment(&document, &arena).unwrap().unwrap();
    assert_eq!(crlf.logical_byte_range, 9..10);
    assert_eq!(
        crlf.mapping,
        LogicalSegmentMapping::AtomicAmbiguity {
            transform: AtomicProjectionKind::CrLfToLf
        }
    );
    let tail = logical.next_segment(&document, &arena).unwrap().unwrap();
    assert_eq!(tail.logical_byte_range, 10..11);
    assert!(logical.next_segment(&document, &arena).unwrap().is_none());

    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn initial_line_quote_marker_precedes_terminal_and_partial_tab_content() {
    let events = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::BLOCK_QUOTE, FactsEnvelope::empty()),
        physical(1, 2, 2, 0, CoveragePart::CONTAINER_MARKER),
        enter(3, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        projected(
            2,
            SerializedMetric { bytes: 1, utf16: 1 },
            0,
            CoveragePart::CONTENT,
            3,
            LogicalContribution::Atomic(AtomicProjection::tab_to_spaces(3).unwrap()),
        ),
        projected(
            3,
            SerializedMetric { bytes: 2, utf16: 1 },
            0,
            CoveragePart::CONTENT,
            3,
            LogicalContribution::Identity,
        ),
        physical(4, 2, 2, 0, CoveragePart::TERMINAL),
        exit(),
        exit(),
        exit(),
    ];
    let mut arena = PageArena::new();
    let document = SerializedGreenDocument::build(
        &mut arena,
        root_spec_with_utf16(7, 6),
        events,
        &mut SerializedGreenBuildReceipt::default(),
    )
    .unwrap();

    let mut source = document
        .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
        .unwrap();
    assert_eq!(
        source.open_path().last().unwrap().kind,
        GreenKind::BLOCK_QUOTE
    );
    let marker = source.next_coverage(&document, &arena).unwrap().unwrap();
    assert_eq!(marker.owner.block, BlockId(2));
    assert!(marker.logical_consumer.is_none());
    let tab = source.next_coverage(&document, &arena).unwrap().unwrap();
    assert_eq!(tab.owner.block, BlockId(3));
    assert_eq!(tab.logical_consumer.unwrap().block, BlockId(3));
    let paragraph = source.open_path().last().unwrap().enter;
    let mut logical = document.logical_cursor(&arena, paragraph).unwrap();
    assert!(matches!(
        logical
            .next_segment(&document, &arena)
            .unwrap()
            .unwrap()
            .mapping,
        LogicalSegmentMapping::AtomicAmbiguity {
            transform: AtomicProjectionKind::TabToSpaces { spaces: 3 }
        }
    ));
    assert_eq!(
        logical
            .next_segment(&document, &arena)
            .unwrap()
            .unwrap()
            .logical_utf16_range,
        3..4
    );
    assert!(logical.next_segment(&document, &arena).unwrap().is_none());

    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

#[test]
#[allow(clippy::too_many_lines)] // The fixture checks every physical/logical mapping boundary.
fn table_trim_unescape_program_maps_hidden_identity_and_unicode_exactly() {
    // Physical spelling is " é\\|🙂 ": trim both spaces and hide the
    // escape backslash while borrowing every retained code point.
    let program = ProjectionProgram::new(vec![
        ProjectionPiece::Hidden {
            metric: SerializedMetric { bytes: 1, utf16: 1 },
            affinity: GreenAffinity::Downstream,
        },
        ProjectionPiece::Identity {
            metric: SerializedMetric { bytes: 2, utf16: 1 },
        },
        ProjectionPiece::Hidden {
            metric: SerializedMetric { bytes: 1, utf16: 1 },
            affinity: GreenAffinity::Downstream,
        },
        ProjectionPiece::Identity {
            metric: SerializedMetric { bytes: 5, utf16: 3 },
        },
        ProjectionPiece::Hidden {
            metric: SerializedMetric { bytes: 1, utf16: 1 },
            affinity: GreenAffinity::Upstream,
        },
    ])
    .unwrap();
    let table_facts = GreenTableOpenFacts::new(1).unwrap().into_envelope();
    let row_facts = FactsEnvelope::new(vec![FactField::critical(FactId::TABLE_ROW, [0])]).unwrap();
    let cell_facts = GreenTableCellOpenFacts::body(0).into_envelope();
    let events = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::TABLE, table_facts),
        enter(3, GreenKind::TABLE_ROW, row_facts),
        enter(4, GreenKind::TABLE_CELL, cell_facts),
        projected(
            1,
            SerializedMetric {
                bytes: 10,
                utf16: 7,
            },
            0,
            CoveragePart::CONTENT,
            4,
            LogicalContribution::Program(program),
        ),
        exit(),
        exit(),
        exit(),
        exit(),
    ];
    let mut arena = PageArena::new();
    let mut build = SerializedGreenBuildReceipt::default();
    let document =
        SerializedGreenDocument::build(&mut arena, root_spec_with_utf16(10, 7), events, &mut build)
            .unwrap();
    assert_eq!(build.projection_program_pages_allocated, 1);

    let source = document
        .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
        .unwrap();
    let cell = source
        .open_path()
        .iter()
        .find(|frame| frame.kind == GreenKind::TABLE_CELL)
        .unwrap()
        .enter;
    let mut logical = document.logical_cursor(&arena, cell).unwrap();
    let mut segments = Vec::new();
    while let Some(segment) = logical.next_segment(&document, &arena).unwrap() {
        segments.push(segment);
    }
    assert_eq!(segments.len(), 5);
    assert!(matches!(
        segments[0].mapping,
        LogicalSegmentMapping::Hidden {
            affinity: GreenAffinity::Downstream
        }
    ));
    assert_eq!(segments[1].byte_range, 1..3);
    assert_eq!(segments[1].utf16_range, 1..2);
    assert_eq!(segments[1].logical_byte_range, 0..2);
    assert_eq!(segments[3].byte_range, 4..9);
    assert_eq!(segments[3].utf16_range, 3..6);
    assert_eq!(segments[3].logical_byte_range, 2..7);
    assert_eq!(segments[3].logical_utf16_range, 1..4);
    assert_eq!(
        segments[2].map_physical(GreenCoordinate::Bytes, 3),
        Some(GreenProjectionPosition::Exact {
            physical: 3,
            logical: 2,
        })
    );
    assert_eq!(
        segments[2].map_physical(GreenCoordinate::Bytes, 4),
        Some(GreenProjectionPosition::Exact {
            physical: 4,
            logical: 2,
        })
    );
    assert!(matches!(
        segments[2].map_logical(GreenCoordinate::Bytes, 2),
        Some(GreenProjectionPosition::Hidden {
            physical,
            logical_boundary: 2,
            affinity: GreenAffinity::Downstream,
        }) if physical == (3..4)
    ));
    let receipt = logical.receipt();
    assert_eq!(receipt.projection_program_pages_decoded, 1);
    assert!(receipt.projection_program_bytes_validated > 0);
    assert!(receipt.projection_program_bytes_validated <= 4096);
    assert_eq!(receipt.projection_pieces_yielded, 5);
    assert!(receipt.maximum_program_scratch_bytes < 1024);

    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

#[test]
fn hidden_only_table_cell_is_inline_and_distinct_from_none_without_a_program_page() {
    let cell_facts = GreenTableCellOpenFacts::body(0).into_envelope();
    let events = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::TABLE_CELL, cell_facts),
        physical(1, 1, 1, 0, CoveragePart::BLOCK_MARKER),
        projected(
            2,
            SerializedMetric { bytes: 2, utf16: 2 },
            0,
            CoveragePart::CONTENT,
            2,
            LogicalContribution::Hidden {
                affinity: GreenAffinity::Downstream,
            },
        ),
        exit(),
        exit(),
    ];
    let mut arena = PageArena::new();
    let mut build = SerializedGreenBuildReceipt::default();
    let document =
        SerializedGreenDocument::build(&mut arena, root_spec(3), events, &mut build).unwrap();
    assert_eq!(build.projection_program_pages_allocated, 0);

    let mut source = document
        .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
        .unwrap();
    let cell = source.open_path().last().unwrap().enter;
    let syntax = source.next_coverage(&document, &arena).unwrap().unwrap();
    assert!(matches!(
        syntax.logical_contribution,
        flark_v3_runtime_slice::LogicalContributionView::None
    ));
    assert!(syntax.logical_consumer.is_none());
    let hidden = source.next_coverage(&document, &arena).unwrap().unwrap();
    assert!(matches!(
        hidden.logical_contribution,
        flark_v3_runtime_slice::LogicalContributionView::Hidden {
            affinity: GreenAffinity::Downstream
        }
    ));
    assert_eq!(hidden.logical_consumer.unwrap().block, BlockId(2));

    let mut logical = document.logical_cursor(&arena, cell).unwrap();
    let hidden = logical.next_segment(&document, &arena).unwrap().unwrap();
    assert_eq!(hidden.logical_byte_range, 0..0);
    assert!(hidden.program.is_none());
    assert_eq!(
        hidden.map_physical(GreenCoordinate::Bytes, 1),
        Some(GreenProjectionPosition::Exact {
            physical: 1,
            logical: 0,
        })
    );
    assert!(matches!(
        hidden.map_physical(GreenCoordinate::Bytes, 2),
        Some(GreenProjectionPosition::Hidden {
            physical,
            logical_boundary: 0,
            affinity: GreenAffinity::Downstream,
        }) if physical == (1..3)
    ));
    assert_eq!(
        hidden.map_physical(GreenCoordinate::Bytes, 3),
        Some(GreenProjectionPosition::Exact {
            physical: 3,
            logical: 0,
        })
    );
    assert!(matches!(
        hidden.map_logical(GreenCoordinate::Bytes, 0),
        Some(GreenProjectionPosition::Hidden { physical, .. }) if physical == (1..3)
    ));
    assert!(logical.next_segment(&document, &arena).unwrap().is_none());

    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

#[test]
fn dense_program_build_seals_before_pending_child_payloads_exceed_one_page() {
    const PIECES: u64 = 900;
    const RUNS: u64 = 8;
    let program = ProjectionProgram::new(
        (0..PIECES)
            .map(|index| {
                if index % 2 == 0 {
                    ProjectionPiece::Hidden {
                        metric: SerializedMetric { bytes: 1, utf16: 1 },
                        affinity: GreenAffinity::Downstream,
                    }
                } else {
                    ProjectionPiece::Identity {
                        metric: SerializedMetric { bytes: 1, utf16: 1 },
                    }
                }
            })
            .collect(),
    )
    .unwrap();
    let events = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
    ]
    .into_iter()
    .chain((0..RUNS).map(|index| {
        projected(
            index + 1,
            SerializedMetric {
                bytes: PIECES,
                utf16: PIECES,
            },
            0,
            CoveragePart::CONTENT,
            2,
            LogicalContribution::Program(program.clone()),
        )
    }))
    .chain([exit(), exit()]);
    let mut arena = PageArena::new();
    let mut receipt = SerializedGreenBuildReceipt::default();
    let document =
        SerializedGreenDocument::build(&mut arena, root_spec(PIECES * RUNS), events, &mut receipt)
            .unwrap();
    assert_eq!(
        receipt.projection_program_pages_allocated,
        usize::try_from(RUNS).unwrap()
    );
    assert!(receipt.maximum_pending_program_payload_bytes <= 4096);
    assert!(document.leaf_count(&arena).unwrap() > 1);

    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

#[test]
fn synthetic_newline_is_virtual_inside_a_physically_anchored_program() {
    let program = ProjectionProgram::new(vec![
        ProjectionPiece::Identity {
            metric: SerializedMetric { bytes: 4, utf16: 4 },
        },
        ProjectionPiece::Virtual {
            kind: VirtualProjectionKind::LineFeed,
        },
    ])
    .unwrap();
    let events = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::INDENTED_CODE, FactsEnvelope::empty()),
        projected(
            1,
            SerializedMetric { bytes: 4, utf16: 4 },
            0,
            CoveragePart::CONTENT,
            2,
            LogicalContribution::Program(program),
        ),
        exit(),
        exit(),
    ];
    let mut arena = PageArena::new();
    let document = SerializedGreenDocument::build(
        &mut arena,
        root_spec(4),
        events,
        &mut SerializedGreenBuildReceipt::default(),
    )
    .unwrap();
    let source = document
        .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
        .unwrap();
    let code = source.open_path().last().unwrap().enter;
    let mut logical = document.logical_cursor(&arena, code).unwrap();
    let identity = logical.next_segment(&document, &arena).unwrap().unwrap();
    let newline = logical.next_segment(&document, &arena).unwrap().unwrap();
    assert_eq!(identity.logical_byte_range, 0..4);
    assert_eq!(newline.byte_range, 4..4);
    assert_eq!(newline.logical_byte_range, 4..5);
    assert_eq!(
        newline.mapping,
        LogicalSegmentMapping::Virtual {
            kind: VirtualProjectionKind::LineFeed
        }
    );
    assert!(matches!(
        newline.map_physical(GreenCoordinate::Bytes, 4),
        Some(GreenProjectionPosition::Virtual {
            physical_boundary: 4,
            logical,
            kind: VirtualProjectionKind::LineFeed,
        }) if logical == (4..5)
    ));
    assert_eq!(
        newline.map_logical(GreenCoordinate::Bytes, 4),
        Some(GreenProjectionPosition::Exact {
            physical: 4,
            logical: 4,
        })
    );
    assert_eq!(
        newline.map_logical(GreenCoordinate::Bytes, 5),
        Some(GreenProjectionPosition::Exact {
            physical: 4,
            logical: 5,
        })
    );
    assert!(logical.next_segment(&document, &arena).unwrap().is_none());

    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

#[test]
fn program_partition_and_ambiguous_terminal_paths_fail_closed() {
    let program = ProjectionProgram::new(vec![ProjectionPiece::Identity {
        metric: SerializedMetric { bytes: 1, utf16: 1 },
    }])
    .unwrap();
    assert_eq!(
        SourceProjectionRun::with_logical(
            CoverageId(1),
            2,
            2,
            0,
            CoveragePart::CONTENT,
            BlockId(2),
            LogicalContribution::Program(program),
        ),
        Err(SerializedGreenError::Invalid(
            "projection program does not partition its physical run"
        ))
    );
    assert_eq!(
        ProjectionProgram::new(vec![ProjectionPiece::Virtual {
            kind: VirtualProjectionKind::LineFeed,
        }]),
        Err(SerializedGreenError::Invalid(
            "projection program must anchor physical input and a valid logical metric"
        ))
    );

    let nested_terminals = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        enter(3, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        projected(
            1,
            SerializedMetric { bytes: 1, utf16: 1 },
            0,
            CoveragePart::CONTENT,
            3,
            LogicalContribution::Identity,
        ),
        exit(),
        exit(),
        exit(),
    ];
    let mut arena = PageArena::new();
    assert_eq!(
        SerializedGreenDocument::build(
            &mut arena,
            root_spec(1),
            nested_terminals,
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap_err(),
        SerializedGreenError::Invalid("terminal block cannot contain another block")
    );
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);

    let terminal_contains_container = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        enter(3, GreenKind::BLOCK_QUOTE, FactsEnvelope::empty()),
        physical(1, 1, 1, 0, CoveragePart::GAP),
        exit(),
        exit(),
        exit(),
    ];
    assert_eq!(
        SerializedGreenDocument::build(
            &mut arena,
            root_spec(1),
            terminal_contains_container,
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap_err(),
        SerializedGreenError::Invalid("terminal block cannot contain another block")
    );
    settle(&mut arena);
}

#[test]
#[allow(clippy::too_many_lines)] // The identity receipt spans base, failed, and successful rewrites.
fn enter_rewrite_reuses_program_page_and_distant_leaf_by_exact_identity() {
    const TAIL: u64 = 2_000;
    let program = ProjectionProgram::new(vec![
        ProjectionPiece::Hidden {
            metric: SerializedMetric { bytes: 1, utf16: 1 },
            affinity: GreenAffinity::Downstream,
        },
        ProjectionPiece::Identity {
            metric: SerializedMetric { bytes: 1, utf16: 1 },
        },
    ])
    .unwrap();
    let events = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        projected(
            1,
            SerializedMetric { bytes: 2, utf16: 2 },
            0,
            CoveragePart::CONTENT,
            2,
            LogicalContribution::Program(program),
        ),
    ]
    .into_iter()
    .chain((0..TAIL).map(|index| {
        projected(
            index + 2,
            SerializedMetric { bytes: 1, utf16: 1 },
            0,
            CoveragePart::CONTENT,
            2,
            LogicalContribution::Identity,
        )
    }))
    .chain([exit(), exit()]);
    let mut arena = PageArena::new();
    let document = SerializedGreenDocument::build(
        &mut arena,
        root_spec(TAIL + 2),
        events,
        &mut SerializedGreenBuildReceipt::default(),
    )
    .unwrap();
    let far_index = document.leaf_count(&arena).unwrap() - 1;
    let far_leaf = document.leaf_at(&arena, far_index).unwrap().unwrap();
    let mut source = document
        .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
        .unwrap();
    let paragraph = source.open_path().last().unwrap().clone();
    let program_page = match source
        .next_coverage(&document, &arena)
        .unwrap()
        .unwrap()
        .logical_contribution
    {
        flark_v3_runtime_slice::LogicalContributionView::Program { program, .. } => program.page,
        other => panic!("expected Program, got {other:?}"),
    };
    let facts = FactsEnvelope::new(vec![FactField::optional(FactId(100), [7])]).unwrap();
    assert_eq!(
        document
            .rewrite_enters(
                &mut arena,
                ParseGeneration(2),
                2,
                vec![flark_v3_runtime_slice::GreenEnterRewrite {
                    target: paragraph.enter,
                    kind: GreenKind::HEADING,
                    facts: FactsEnvelope::empty(),
                }],
                &mut SerializedGreenBuildReceipt::default(),
            )
            .unwrap_err(),
        SerializedGreenError::Invalid(
            "Enter rewrite may only replace facts for the same block kind"
        )
    );
    let next = document
        .rewrite_enters(
            &mut arena,
            ParseGeneration(2),
            2,
            vec![flark_v3_runtime_slice::GreenEnterRewrite {
                target: paragraph.enter,
                kind: GreenKind::PARAGRAPH,
                facts,
            }],
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap();
    let next_far_index = next.leaf_count(&arena).unwrap() - 1;
    assert_eq!(
        next.leaf_at(&arena, next_far_index).unwrap(),
        Some(far_leaf)
    );
    let mut next_source = next
        .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
        .unwrap();
    let next_program_page = match next_source
        .next_coverage(&next, &arena)
        .unwrap()
        .unwrap()
        .logical_contribution
    {
        flark_v3_runtime_slice::LogicalContributionView::Program { program, .. } => program.page,
        other => panic!("expected Program, got {other:?}"),
    };
    assert_eq!(next_program_page, program_page);

    next.release_later(&mut arena).unwrap();
    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn lone_cr_has_a_typed_canonical_lf_transform() {
    let events = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        projected(
            1,
            SerializedMetric { bytes: 1, utf16: 1 },
            0,
            CoveragePart::CONTENT,
            2,
            LogicalContribution::Identity,
        ),
        projected(
            2,
            SerializedMetric { bytes: 1, utf16: 1 },
            0,
            CoveragePart::CONTENT,
            2,
            LogicalContribution::Atomic(AtomicProjection::lone_cr_to_lf()),
        ),
        projected(
            3,
            SerializedMetric { bytes: 1, utf16: 1 },
            0,
            CoveragePart::CONTENT,
            2,
            LogicalContribution::Identity,
        ),
        physical(4, 1, 1, 0, CoveragePart::TERMINAL),
        exit(),
        exit(),
    ];
    let mut arena = PageArena::new();
    let document = SerializedGreenDocument::build(
        &mut arena,
        root_spec(4),
        events,
        &mut SerializedGreenBuildReceipt::default(),
    )
    .unwrap();
    let source = document
        .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
        .unwrap();
    let paragraph = source.open_path().last().unwrap().enter;
    let mut logical = document.logical_cursor(&arena, paragraph).unwrap();
    let first = logical.next_segment(&document, &arena).unwrap().unwrap();
    assert_eq!(first.logical_byte_range, 0..1);
    let segment = logical.next_segment(&document, &arena).unwrap().unwrap();
    assert_eq!(
        segment.mapping,
        LogicalSegmentMapping::AtomicAmbiguity {
            transform: AtomicProjectionKind::LoneCrToLf
        }
    );
    let tail = logical.next_segment(&document, &arena).unwrap().unwrap();
    assert_eq!(tail.logical_byte_range, 2..3);
    assert!(logical.next_segment(&document, &arena).unwrap().is_none());

    let mut source = document
        .seek(&arena, GreenCoordinate::Bytes, 3, GreenAffinity::Downstream)
        .unwrap();
    let terminal = source.next_coverage(&document, &arena).unwrap().unwrap();
    assert!(matches!(
        terminal.logical_contribution,
        flark_v3_runtime_slice::LogicalContributionView::None
    ));
    assert!(terminal.logical_consumer.is_none());
    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

#[test]
fn revision_zero_nul_is_a_typed_u_fffd_atomic_projection() {
    let events = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        projected(
            1,
            SerializedMetric { bytes: 1, utf16: 1 },
            0,
            CoveragePart::CONTENT,
            2,
            LogicalContribution::Identity,
        ),
        projected(
            2,
            SerializedMetric { bytes: 1, utf16: 1 },
            0,
            CoveragePart::CONTENT,
            2,
            LogicalContribution::Atomic(AtomicProjection::nul_to_replacement()),
        ),
        projected(
            3,
            SerializedMetric { bytes: 1, utf16: 1 },
            0,
            CoveragePart::CONTENT,
            2,
            LogicalContribution::Identity,
        ),
        exit(),
        exit(),
    ];
    let mut spec = root_spec(3);
    spec.source_revision = SourceRevision(0);
    let mut arena = PageArena::new();
    let document = SerializedGreenDocument::build(
        &mut arena,
        spec,
        events,
        &mut SerializedGreenBuildReceipt::default(),
    )
    .unwrap();
    let source = document
        .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
        .unwrap();
    let paragraph = source.open_path().last().unwrap().enter;
    let mut logical = document.logical_cursor(&arena, paragraph).unwrap();
    let prefix = logical.next_segment(&document, &arena).unwrap().unwrap();
    assert_eq!(prefix.logical_byte_range, 0..1);
    let nul = logical.next_segment(&document, &arena).unwrap().unwrap();
    assert_eq!(nul.byte_range, 1..2);
    assert_eq!(nul.logical_byte_range, 1..4);
    assert_eq!(nul.logical_utf16_range, 1..2);
    assert_eq!(
        nul.mapping,
        LogicalSegmentMapping::AtomicAmbiguity {
            transform: AtomicProjectionKind::NulToReplacement,
        }
    );
    assert_eq!(
        nul.map_physical(GreenCoordinate::Bytes, 1),
        Some(GreenProjectionPosition::Exact {
            physical: 1,
            logical: 1,
        })
    );
    assert_eq!(
        nul.map_physical(GreenCoordinate::Bytes, 2),
        Some(GreenProjectionPosition::Exact {
            physical: 2,
            logical: 4,
        })
    );
    let suffix = logical.next_segment(&document, &arena).unwrap().unwrap();
    assert_eq!(suffix.logical_byte_range, 4..5);
    assert!(logical.next_segment(&document, &arena).unwrap().is_none());

    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

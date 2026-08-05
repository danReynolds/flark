use flark_v3_runtime_slice::{
    AtomicProjection, BlockId, ClosedChildAggregate, CoverageId, CoveragePart, FactsEnvelope,
    GrammarRevision, GreenAffinity, GreenEvent, GreenKind, GreenProjectionPosition,
    LogicalContribution, LogicalSegmentMapping, PROJECTION_PROGRAM_PAGE_BYTES, PageArena,
    ParseGeneration, ProjectionChunk, ProjectionChunkerFinish, ProjectionPiece,
    ProjectionProgramChunker, SerializedGreenBuildReceipt, SerializedGreenDocument,
    SerializedGreenError, SerializedGreenRootSpec, SerializedMetric, SourceProjectionRun,
    SourceRevision, SourceRootId, VirtualProjectionKind,
};

fn root_spec(bytes: u64, utf16: u64) -> SerializedGreenRootSpec {
    SerializedGreenRootSpec {
        syntax_profile: 1,
        source_revision: SourceRevision(0),
        source_root: SourceRootId(1),
        source_bytes: bytes,
        source_utf16: utf16,
        grammar_revision: GrammarRevision(1),
        parse_generation: ParseGeneration(1),
        semantic_epoch: 1,
        known_bytes: 0..bytes,
    }
}

fn settle(arena: &mut PageArena) {
    while arena.metrics().pending_releases != 0 {
        arena.poll_reclaim(10_000).unwrap();
    }
}

fn push_piece(
    chunker: &mut ProjectionProgramChunker,
    chunks: &mut Vec<ProjectionChunk>,
    piece: ProjectionPiece,
) {
    if let Some(chunk) = chunker.push(piece).unwrap() {
        chunks.push(chunk);
    }
}

fn finish_chunks(chunker: &mut ProjectionProgramChunker, chunks: &mut Vec<ProjectionChunk>) {
    loop {
        let (chunk, status) = chunker.finish().unwrap();
        if let Some(chunk) = chunk {
            chunks.push(chunk);
        }
        if matches!(status, ProjectionChunkerFinish::Complete(_)) {
            break;
        }
    }
}

fn alternating_piece(index: u64) -> ProjectionPiece {
    if index.is_multiple_of(2) {
        ProjectionPiece::Hidden {
            metric: SerializedMetric { bytes: 1, utf16: 1 },
            affinity: if index.is_multiple_of(4) {
                GreenAffinity::Upstream
            } else {
                GreenAffinity::Downstream
            },
        }
    } else {
        ProjectionPiece::Identity {
            metric: SerializedMetric { bytes: 1, utf16: 1 },
        }
    }
}

fn document_from_chunks(
    arena: &mut PageArena,
    chunks: Vec<ProjectionChunk>,
    total_bytes: u64,
) -> SerializedGreenDocument {
    let total_utf16 = chunks.iter().map(|chunk| chunk.physical_metric.utf16).sum();
    let events = [
        GreenEvent::enter(BlockId(1), GreenKind::DOCUMENT, FactsEnvelope::empty()),
        GreenEvent::enter(BlockId(2), GreenKind::PARAGRAPH, FactsEnvelope::empty()),
    ]
    .into_iter()
    .chain(chunks.into_iter().enumerate().map(|(index, chunk)| {
        SourceProjectionRun::with_logical(
            CoverageId(u64::try_from(index).unwrap() + 1),
            chunk.physical_metric.bytes,
            chunk.physical_metric.utf16,
            0,
            CoveragePart::CONTENT,
            BlockId(2),
            chunk.logical_contribution,
        )
        .map(GreenEvent::Coverage)
        .unwrap()
    }))
    .chain([
        GreenEvent::exit(ClosedChildAggregate::default()),
        GreenEvent::exit(ClosedChildAggregate::default()),
    ]);
    SerializedGreenDocument::build(
        arena,
        root_spec(total_bytes, total_utf16),
        events,
        &mut SerializedGreenBuildReceipt::default(),
    )
    .unwrap()
}

#[test]
fn dense_escaped_pipe_shape_streams_into_deterministic_bounded_chunks() {
    const PIECES: u64 = 20_000;
    let mut chunker = ProjectionProgramChunker::new(SerializedMetric {
        bytes: PIECES,
        utf16: PIECES,
    })
    .unwrap();
    let mut chunks = Vec::new();
    for index in 0..PIECES {
        push_piece(&mut chunker, &mut chunks, alternating_piece(index));
    }
    finish_chunks(&mut chunker, &mut chunks);
    let receipt = chunker.receipt();
    assert!(chunks.len() > 4);
    assert_eq!(receipt.pieces_accepted, PIECES);
    assert_eq!(receipt.chunks_emitted, chunks.len() as u64);
    assert!(receipt.maximum_buffered_payload_bytes <= PROJECTION_PROGRAM_PAGE_BYTES);
    assert_eq!(
        receipt.maximum_buffer_capacity_bytes,
        PROJECTION_PROGRAM_PAGE_BYTES
    );
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.physical_metric.bytes)
            .sum::<u64>(),
        PIECES
    );
    assert!(
        chunks
            .iter()
            .enumerate()
            .all(|(index, chunk)| chunk.fragment_ordinal == index as u64)
    );
    assert!(
        chunks
            .iter()
            .all(|chunk| match &chunk.logical_contribution {
                LogicalContribution::Program(program) => {
                    program.encoded_bytes() <= PROJECTION_PROGRAM_PAGE_BYTES
                }
                _ => true,
            })
    );
}

#[test]
fn alternating_hidden_affinities_can_form_a_zero_logical_program() {
    const PIECES: u64 = 5_000;
    let metric = SerializedMetric {
        bytes: PIECES,
        utf16: PIECES,
    };
    let mut chunker = ProjectionProgramChunker::new(metric).unwrap();
    let mut chunks = Vec::new();
    for index in 0..PIECES {
        push_piece(
            &mut chunker,
            &mut chunks,
            ProjectionPiece::Hidden {
                metric: SerializedMetric { bytes: 1, utf16: 1 },
                affinity: if index.is_multiple_of(2) {
                    GreenAffinity::Upstream
                } else {
                    GreenAffinity::Downstream
                },
            },
        );
    }
    finish_chunks(&mut chunker, &mut chunks);
    assert!(chunks.len() > 1);
    assert!(
        chunks
            .iter()
            .all(|chunk| match &chunk.logical_contribution {
                LogicalContribution::Program(program) => {
                    program.logical_metric() == SerializedMetric::default()
                }
                LogicalContribution::Hidden { .. } => true,
                _ => false,
            })
    );

    let mut arena = PageArena::new();
    let document = document_from_chunks(&mut arena, chunks, PIECES);
    let source = document
        .seek(
            &arena,
            flark_v3_runtime_slice::GreenCoordinate::Bytes,
            0,
            GreenAffinity::Downstream,
        )
        .unwrap();
    let paragraph = source.open_path().last().unwrap().enter;
    let mut logical = document.logical_cursor(&arena, paragraph).unwrap();
    let mut segments = 0_u64;
    while let Some(segment) = logical.next_segment(&document, &arena).unwrap() {
        assert!(matches!(
            segment.mapping,
            LogicalSegmentMapping::Hidden { .. }
        ));
        assert!(segment.logical_byte_range.is_empty());
        segments += 1;
    }
    assert_eq!(segments, PIECES);
    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

#[test]
#[allow(clippy::too_many_lines)] // One test compares both sides of the same virtual-anchor rule.
fn virtual_ownership_is_right_biased_inside_and_left_biased_at_eof() {
    const PREFIX_PIECES: u64 = 2_043;

    let mut interior = ProjectionProgramChunker::new(SerializedMetric {
        bytes: PREFIX_PIECES + 1,
        utf16: PREFIX_PIECES + 1,
    })
    .unwrap();
    let mut interior_chunks = Vec::new();
    for index in 0..PREFIX_PIECES {
        push_piece(
            &mut interior,
            &mut interior_chunks,
            alternating_piece(index),
        );
    }
    push_piece(
        &mut interior,
        &mut interior_chunks,
        ProjectionPiece::Virtual {
            kind: VirtualProjectionKind::LineFeed,
        },
    );
    push_piece(
        &mut interior,
        &mut interior_chunks,
        ProjectionPiece::Identity {
            metric: SerializedMetric { bytes: 1, utf16: 1 },
        },
    );
    finish_chunks(&mut interior, &mut interior_chunks);
    assert!(interior_chunks.len() >= 2);

    let mut arena = PageArena::new();
    let document = document_from_chunks(&mut arena, interior_chunks, PREFIX_PIECES + 1);
    let source = document
        .seek(
            &arena,
            flark_v3_runtime_slice::GreenCoordinate::Bytes,
            0,
            GreenAffinity::Downstream,
        )
        .unwrap();
    let paragraph = source.open_path().last().unwrap().enter;
    let mut logical = document.logical_cursor(&arena, paragraph).unwrap();
    let mut previous = None;
    let mut saw_interior = false;
    while let Some(segment) = logical.next_segment(&document, &arena).unwrap() {
        if segment.mapping
            == (LogicalSegmentMapping::Virtual {
                kind: VirtualProjectionKind::LineFeed,
            })
        {
            let virtual_coverage = segment.coverage;
            let next = logical.next_segment(&document, &arena).unwrap().unwrap();
            assert_eq!(virtual_coverage, next.coverage);
            assert_ne!(previous, Some(virtual_coverage));
            saw_interior = true;
            break;
        }
        previous = Some(segment.coverage);
    }
    assert!(saw_interior);
    document.release_later(&mut arena).unwrap();
    settle(&mut arena);

    let mut terminal = ProjectionProgramChunker::new(SerializedMetric {
        bytes: PREFIX_PIECES,
        utf16: PREFIX_PIECES,
    })
    .unwrap();
    let mut terminal_chunks = Vec::new();
    for index in 0..PREFIX_PIECES {
        push_piece(
            &mut terminal,
            &mut terminal_chunks,
            alternating_piece(index),
        );
    }
    push_piece(
        &mut terminal,
        &mut terminal_chunks,
        ProjectionPiece::Virtual {
            kind: VirtualProjectionKind::LineFeed,
        },
    );
    finish_chunks(&mut terminal, &mut terminal_chunks);
    assert!(terminal_chunks.len() >= 2);

    let mut arena = PageArena::new();
    let document = document_from_chunks(&mut arena, terminal_chunks, PREFIX_PIECES);
    let source = document
        .seek(
            &arena,
            flark_v3_runtime_slice::GreenCoordinate::Bytes,
            0,
            GreenAffinity::Downstream,
        )
        .unwrap();
    let paragraph = source.open_path().last().unwrap().enter;
    let mut logical = document.logical_cursor(&arena, paragraph).unwrap();
    let mut previous = None;
    let mut saw_terminal = false;
    while let Some(segment) = logical.next_segment(&document, &arena).unwrap() {
        if segment.mapping
            == (LogicalSegmentMapping::Virtual {
                kind: VirtualProjectionKind::LineFeed,
            })
        {
            assert_eq!(previous, Some(segment.coverage));
            assert_eq!(
                segment.map_physical(
                    flark_v3_runtime_slice::GreenCoordinate::Bytes,
                    segment.byte_range.start,
                ),
                Some(GreenProjectionPosition::Virtual {
                    physical_boundary: segment.byte_range.start,
                    logical: segment.logical_byte_range.clone(),
                    kind: VirtualProjectionKind::LineFeed,
                })
            );
            assert!(logical.next_segment(&document, &arena).unwrap().is_none());
            saw_terminal = true;
            break;
        }
        previous = Some(segment.coverage);
    }
    assert!(saw_terminal);
    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

#[test]
fn atomics_are_never_split_and_invalid_envelopes_fail_closed() {
    const ATOMICS: u64 = 3_000;
    let mut chunker = ProjectionProgramChunker::new(SerializedMetric {
        bytes: 4_000,
        utf16: 4_000,
    })
    .unwrap();
    let mut chunks = Vec::new();
    for index in 0..ATOMICS {
        let (physical_metric, projection) = match index % 3 {
            0 => (
                SerializedMetric { bytes: 1, utf16: 1 },
                AtomicProjection::tab_to_spaces(4).unwrap(),
            ),
            1 => (
                SerializedMetric { bytes: 2, utf16: 2 },
                AtomicProjection::crlf_to_lf(),
            ),
            _ => (
                SerializedMetric { bytes: 1, utf16: 1 },
                AtomicProjection::nul_to_replacement(),
            ),
        };
        push_piece(
            &mut chunker,
            &mut chunks,
            ProjectionPiece::Atomic {
                physical_metric,
                projection,
            },
        );
    }
    finish_chunks(&mut chunker, &mut chunks);
    assert!(chunks.len() > 1);
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.physical_metric.bytes)
            .sum::<u64>(),
        4_000
    );

    let mut short = ProjectionProgramChunker::new(SerializedMetric { bytes: 2, utf16: 2 }).unwrap();
    short
        .push(ProjectionPiece::Identity {
            metric: SerializedMetric { bytes: 1, utf16: 1 },
        })
        .unwrap();
    assert_eq!(
        short.finish().unwrap_err(),
        SerializedGreenError::Invalid("projection pieces do not partition their source envelope")
    );

    let mut virtual_only =
        ProjectionProgramChunker::new(SerializedMetric { bytes: 1, utf16: 1 }).unwrap();
    virtual_only
        .push(ProjectionPiece::Virtual {
            kind: VirtualProjectionKind::LineFeed,
        })
        .unwrap();
    assert_eq!(
        virtual_only.finish().unwrap_err(),
        SerializedGreenError::Invalid("virtual-only projection envelope has no physical anchor")
    );

    let mut repeated_virtual =
        ProjectionProgramChunker::new(SerializedMetric { bytes: 1, utf16: 1 }).unwrap();
    repeated_virtual
        .push(ProjectionPiece::Virtual {
            kind: VirtualProjectionKind::LineFeed,
        })
        .unwrap();
    assert_eq!(
        repeated_virtual
            .push(ProjectionPiece::Virtual {
                kind: VirtualProjectionKind::LineFeed,
            })
            .unwrap_err(),
        SerializedGreenError::Invalid(
            "adjacent virtual pieces require a bounded typed repeat transform"
        )
    );
}

fn build_unicode_varint_chunks(observe_every_piece: bool) -> Vec<ProjectionChunk> {
    const PIECES: u64 = 20_000;
    let mut chunker = ProjectionProgramChunker::new(SerializedMetric {
        bytes: PIECES * 4,
        utf16: PIECES * 2,
    })
    .unwrap();
    let mut chunks = Vec::new();
    for index in 0..PIECES {
        let piece = if index.is_multiple_of(2) {
            ProjectionPiece::Hidden {
                metric: SerializedMetric { bytes: 4, utf16: 2 },
                affinity: GreenAffinity::Upstream,
            }
        } else {
            ProjectionPiece::Identity {
                metric: SerializedMetric { bytes: 4, utf16: 2 },
            }
        };
        push_piece(&mut chunker, &mut chunks, piece);
        if observe_every_piece {
            let receipt = chunker.receipt();
            assert_eq!(receipt.pieces_accepted, index + 1);
        }
    }
    finish_chunks(&mut chunker, &mut chunks);
    chunks
}

#[test]
fn leading_virtual_and_unicode_varint_cliffs_are_schedule_independent() {
    let mut leading =
        ProjectionProgramChunker::new(SerializedMetric { bytes: 1, utf16: 1 }).unwrap();
    let mut leading_chunks = Vec::new();
    push_piece(
        &mut leading,
        &mut leading_chunks,
        ProjectionPiece::Virtual {
            kind: VirtualProjectionKind::LineFeed,
        },
    );
    push_piece(
        &mut leading,
        &mut leading_chunks,
        ProjectionPiece::Identity {
            metric: SerializedMetric { bytes: 1, utf16: 1 },
        },
    );
    finish_chunks(&mut leading, &mut leading_chunks);
    assert_eq!(leading_chunks.len(), 1);
    assert!(matches!(
        leading_chunks[0].logical_contribution,
        LogicalContribution::Program(_)
    ));

    let uninterrupted = build_unicode_varint_chunks(false);
    let observed_after_every_piece = build_unicode_varint_chunks(true);
    assert_eq!(uninterrupted, observed_after_every_piece);
    assert!(uninterrupted.len() > 4);
    assert!(
        uninterrupted
            .iter()
            .all(|chunk| match &chunk.logical_contribution {
                LogicalContribution::Program(program) => {
                    program.encoded_bytes() <= PROJECTION_PROGRAM_PAGE_BYTES
                }
                _ => true,
            })
    );
}

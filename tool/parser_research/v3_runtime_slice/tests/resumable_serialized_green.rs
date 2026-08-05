use flark_v3_runtime_slice::{
    ArenaBuildLifecycle, BlockId, ClosedChildAggregate, CoverageId, CoveragePart, FactField,
    FactId, FactsEnvelope, GrammarRevision, GreenAffinity, GreenCoordinate, GreenEnterRewrite,
    GreenEvent, GreenKind, LogicalContribution, PageArena, ParseGeneration, ProjectionPiece,
    ProjectionProgram, ResumableSerializedGreenBuild, SerializedGreenBuildManifest,
    SerializedGreenBuildReceipt, SerializedGreenError, SerializedGreenRootSpec,
    SerializedGreenStreamProgress, SerializedMetric, SourceProjectionRun, SourceRevision,
    SourceRootId,
};

fn root_spec(bytes: u64) -> SerializedGreenRootSpec {
    SerializedGreenRootSpec {
        syntax_profile: 1,
        source_revision: SourceRevision(0),
        source_root: SourceRootId(1),
        source_bytes: bytes,
        source_utf16: bytes,
        grammar_revision: GrammarRevision(1),
        parse_generation: ParseGeneration(1),
        semantic_epoch: 1,
        known_bytes: 0..bytes,
    }
}

fn enter(block: u64, kind: GreenKind) -> GreenEvent {
    GreenEvent::enter(BlockId(block), kind, FactsEnvelope::empty())
}

fn coverage(id: u64, bytes: u64, target: u64) -> GreenEvent {
    coverage_metric(id, bytes, bytes, target)
}

fn coverage_metric(id: u64, bytes: u64, utf16: u64, target: u64) -> GreenEvent {
    GreenEvent::Coverage(
        SourceProjectionRun::with_logical(
            CoverageId(id),
            bytes,
            utf16,
            0,
            CoveragePart::CONTENT,
            BlockId(target),
            LogicalContribution::Identity,
        )
        .unwrap(),
    )
}

#[test]
fn equal_byte_length_with_wrong_bound_utf16_fails_legacy_and_resumable_builds() {
    let events = || {
        [
            enter(1, GreenKind::DOCUMENT),
            enter(2, GreenKind::PARAGRAPH),
            coverage_metric(1, 4, 2, 2),
            exit(),
            exit(),
        ]
    };
    let mut wrong = root_spec(4);
    wrong.source_utf16 = 3;

    let mut arena = PageArena::new();
    assert_eq!(
        flark_v3_runtime_slice::SerializedGreenDocument::build(
            &mut arena,
            wrong.clone(),
            events(),
            &mut flark_v3_runtime_slice::SerializedGreenBuildReceipt::default(),
        )
        .unwrap_err(),
        SerializedGreenError::Invalid("green coverage does not match bound source length")
    );
    settle(&mut arena);

    let ticket = arena.begin_build().unwrap();
    let mut build = ResumableSerializedGreenBuild::new(&ticket, wrong).unwrap();
    let mut session = arena.resume_build(ticket).unwrap();
    for event in events() {
        build.offer_event(&mut session, event).unwrap();
        assert_eq!(
            build.poll(&mut session).unwrap(),
            SerializedGreenStreamProgress::ReadyForEvent
        );
    }
    build.finish_input(&mut session).unwrap();
    let error = loop {
        match build.poll(&mut session) {
            Ok(SerializedGreenStreamProgress::Pending) => {}
            Err(error) => break error,
            other => panic!("wrong UTF-16 manifest unexpectedly advanced: {other:?}"),
        }
    };
    assert_eq!(
        error,
        SerializedGreenError::Invalid("green coverage does not match bound source length")
    );
    let abort = session.begin_abort().unwrap();
    while !arena.poll_build_abort(abort, 1).unwrap().complete {}
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

fn exit() -> GreenEvent {
    GreenEvent::exit(ClosedChildAggregate::default())
}

fn settle(arena: &mut PageArena) {
    while arena.metrics().pending_releases != 0 {
        arena.poll_reclaim(10_000).unwrap();
    }
}

fn maximum_avl_height(leaves: u64) -> u16 {
    let mut height = 1_u16;
    let mut minimum_at_height = 1_u64;
    let mut minimum_at_next_height = 2_u64;
    while minimum_at_next_height <= leaves {
        height += 1;
        let next = minimum_at_height.saturating_add(minimum_at_next_height);
        minimum_at_height = minimum_at_next_height;
        minimum_at_next_height = next;
    }
    height
}

fn offer_with_suspend(
    arena: &mut PageArena,
    ticket: flark_v3_runtime_slice::ArenaBuildTicket,
    build: &mut ResumableSerializedGreenBuild,
    event: GreenEvent,
) -> flark_v3_runtime_slice::ArenaBuildTicket {
    let mut session = arena.resume_build(ticket).unwrap();
    build.offer_event(&mut session, event).unwrap();
    session.suspend().unwrap()
}

fn poll_with_suspend(
    arena: &mut PageArena,
    ticket: flark_v3_runtime_slice::ArenaBuildTicket,
    build: &mut ResumableSerializedGreenBuild,
) -> (
    flark_v3_runtime_slice::ArenaBuildTicket,
    SerializedGreenStreamProgress,
) {
    let mut session = arena.resume_build(ticket).unwrap();
    let before = build.receipt().resumable_arena_allocations;
    let progress = build.poll(&mut session).unwrap();
    let after = build.receipt().resumable_arena_allocations;
    assert!(after - before <= 1, "one poll crossed multiple allocations");
    (session.suspend().unwrap(), progress)
}

fn drain_event(
    arena: &mut PageArena,
    mut ticket: flark_v3_runtime_slice::ArenaBuildTicket,
    build: &mut ResumableSerializedGreenBuild,
) -> flark_v3_runtime_slice::ArenaBuildTicket {
    loop {
        let (next, progress) = poll_with_suspend(arena, ticket, build);
        ticket = next;
        if progress == SerializedGreenStreamProgress::ReadyForEvent {
            return ticket;
        }
        assert_eq!(progress, SerializedGreenStreamProgress::Pending);
    }
}

fn finish_with_suspend(
    arena: &mut PageArena,
    ticket: flark_v3_runtime_slice::ArenaBuildTicket,
    build: &mut ResumableSerializedGreenBuild,
) -> flark_v3_runtime_slice::ArenaBuildTicket {
    let mut session = arena.resume_build(ticket).unwrap();
    build.finish_input(&mut session).unwrap();
    let mut ticket = session.suspend().unwrap();
    loop {
        let (next, progress) = poll_with_suspend(arena, ticket, build);
        ticket = next;
        match progress {
            SerializedGreenStreamProgress::Pending => {}
            SerializedGreenStreamProgress::ManifestReady => return ticket,
            SerializedGreenStreamProgress::ReadyForEvent => {
                panic!("finished stream became writable")
            }
        }
    }
}

fn commit(
    arena: &mut PageArena,
    ticket: flark_v3_runtime_slice::ArenaBuildTicket,
    manifest: SerializedGreenBuildManifest,
) -> flark_v3_runtime_slice::SerializedGreenDocument {
    let session = arena.resume_build(ticket).unwrap();
    manifest.commit(session).unwrap().0
}

#[test]
fn sparse_leaf_barrier_mints_one_exact_nonforgeable_sequence_cut() {
    let mut arena = PageArena::new();
    let mut ticket = arena.begin_build().unwrap();
    let build_id = ticket.id();
    let mut build = ResumableSerializedGreenBuild::new(&ticket, root_spec(2)).unwrap();

    for event in [
        enter(1, GreenKind::DOCUMENT),
        enter(2, GreenKind::PARAGRAPH),
        coverage(1, 1, 2),
    ] {
        ticket = offer_with_suspend(&mut arena, ticket, &mut build, event);
        ticket = drain_event(&mut arena, ticket, &mut build);
    }

    let mut session = arena.resume_build(ticket).unwrap();
    build.begin_leaf_barrier(&mut session).unwrap();
    ticket = session.suspend().unwrap();
    loop {
        let (next, progress) = poll_with_suspend(&mut arena, ticket, &mut build);
        ticket = next;
        match progress {
            SerializedGreenStreamProgress::Pending => {}
            SerializedGreenStreamProgress::ReadyForEvent => break,
            SerializedGreenStreamProgress::ManifestReady => {
                panic!("leaf barrier unexpectedly finalized the document")
            }
        }
    }

    let session = arena.resume_build(ticket).unwrap();
    let cut = build.take_leaf_barrier_cut(&session).unwrap();
    assert_eq!(cut.build_id(), build_id);
    assert_eq!(cut.leaves_before(), 1);
    assert_eq!(cut.events_before(), 3);
    assert_eq!(cut.source_before(), SerializedMetric { bytes: 1, utf16: 1 });
    assert_eq!(build.receipt().leaf_barriers_completed, 1);
    assert_eq!(
        build.take_leaf_barrier_cut(&session).unwrap_err(),
        SerializedGreenError::Invalid("green leaf barrier has no unconsumed cut")
    );
    ticket = session.suspend().unwrap();

    for event in [coverage(2, 1, 2), exit(), exit()] {
        ticket = offer_with_suspend(&mut arena, ticket, &mut build, event);
        ticket = drain_event(&mut arena, ticket, &mut build);
    }
    ticket = finish_with_suspend(&mut arena, ticket, &mut build);
    let manifest = build.take_manifest().unwrap();
    let document = commit(&mut arena, ticket, manifest);
    assert_eq!(document.leaf_count(&arena).unwrap(), 2);
    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

#[test]
fn zero_metric_structural_barriers_have_distinct_exact_event_and_leaf_ranks() {
    let mut arena = PageArena::new();
    let mut ticket = arena.begin_build().unwrap();
    let mut build = ResumableSerializedGreenBuild::new(&ticket, root_spec(0)).unwrap();

    ticket = offer_with_suspend(
        &mut arena,
        ticket,
        &mut build,
        enter(1, GreenKind::DOCUMENT),
    );
    ticket = drain_event(&mut arena, ticket, &mut build);
    let mut session = arena.resume_build(ticket).unwrap();
    build.begin_leaf_barrier(&mut session).unwrap();
    ticket = session.suspend().unwrap();
    loop {
        let (next, progress) = poll_with_suspend(&mut arena, ticket, &mut build);
        ticket = next;
        if progress == SerializedGreenStreamProgress::ReadyForEvent {
            break;
        }
        assert_eq!(progress, SerializedGreenStreamProgress::Pending);
    }
    let session = arena.resume_build(ticket).unwrap();
    let document_cut = build.take_leaf_barrier_cut(&session).unwrap();
    ticket = session.suspend().unwrap();

    ticket = offer_with_suspend(
        &mut arena,
        ticket,
        &mut build,
        enter(2, GreenKind::PARAGRAPH),
    );
    ticket = drain_event(&mut arena, ticket, &mut build);
    let mut session = arena.resume_build(ticket).unwrap();
    build.begin_leaf_barrier(&mut session).unwrap();
    ticket = session.suspend().unwrap();
    loop {
        let (next, progress) = poll_with_suspend(&mut arena, ticket, &mut build);
        ticket = next;
        if progress == SerializedGreenStreamProgress::ReadyForEvent {
            break;
        }
        assert_eq!(progress, SerializedGreenStreamProgress::Pending);
    }
    let session = arena.resume_build(ticket).unwrap();
    let paragraph_cut = build.take_leaf_barrier_cut(&session).unwrap();
    ticket = session.suspend().unwrap();

    assert_eq!(document_cut.source_before(), SerializedMetric::default());
    assert_eq!(paragraph_cut.source_before(), SerializedMetric::default());
    assert_eq!(
        (document_cut.leaves_before(), document_cut.events_before()),
        (1, 1)
    );
    assert_eq!(
        (paragraph_cut.leaves_before(), paragraph_cut.events_before()),
        (2, 2)
    );

    for event in [exit(), exit()] {
        ticket = offer_with_suspend(&mut arena, ticket, &mut build, event);
        ticket = drain_event(&mut arena, ticket, &mut build);
    }
    ticket = finish_with_suspend(&mut arena, ticket, &mut build);
    let manifest = build.take_manifest().unwrap();
    let document = commit(&mut arena, ticket, manifest);
    assert_eq!(document.leaf_count(&arena).unwrap(), 3);
    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

#[test]
fn ready_leaf_cut_cannot_escape_through_a_wrong_or_recycled_build_session() {
    let mut arena = PageArena::new();
    let mut ticket = arena.begin_build().unwrap();
    let original_build = ticket.id();
    let mut build = ResumableSerializedGreenBuild::new(&ticket, root_spec(0)).unwrap();
    ticket = offer_with_suspend(
        &mut arena,
        ticket,
        &mut build,
        enter(1, GreenKind::DOCUMENT),
    );
    ticket = drain_event(&mut arena, ticket, &mut build);
    let mut session = arena.resume_build(ticket).unwrap();
    build.begin_leaf_barrier(&mut session).unwrap();
    ticket = session.suspend().unwrap();
    loop {
        let (next, progress) = poll_with_suspend(&mut arena, ticket, &mut build);
        ticket = next;
        if progress == SerializedGreenStreamProgress::ReadyForEvent {
            break;
        }
        assert_eq!(progress, SerializedGreenStreamProgress::Pending);
    }

    let wrong_ticket = arena.begin_build().unwrap();
    let wrong_build = wrong_ticket.id();
    let wrong_session = arena.resume_build(wrong_ticket).unwrap();
    assert_ne!(wrong_build, original_build);
    assert_eq!(
        build.take_leaf_barrier_cut(&wrong_session).unwrap_err(),
        SerializedGreenError::Invalid("arena session belongs to another build generation")
    );
    let wrong_abort = wrong_session.begin_abort().unwrap();
    assert!(arena.poll_build_abort(wrong_abort, 0).unwrap().complete);

    let abort = arena.begin_build_abort(ticket).unwrap();
    while !arena.poll_build_abort(abort, 1).unwrap().complete {}
    let recycled_ticket = arena.begin_build().unwrap();
    let recycled_session = arena.resume_build(recycled_ticket).unwrap();
    assert_eq!(
        build.take_leaf_barrier_cut(&recycled_session).unwrap_err(),
        SerializedGreenError::Invalid("arena session belongs to another build generation")
    );
    let recycled_abort = recycled_session.begin_abort().unwrap();
    assert!(arena.poll_build_abort(recycled_abort, 0).unwrap().complete);
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn cancellation_during_leaf_barrier_reclaims_the_forced_leaf_with_build_fuel() {
    let mut arena = PageArena::new();
    let ticket = arena.begin_build().unwrap();
    let mut build = ResumableSerializedGreenBuild::new(&ticket, root_spec(1)).unwrap();
    let mut ticket = ticket;
    for event in [
        enter(1, GreenKind::DOCUMENT),
        enter(2, GreenKind::PARAGRAPH),
        coverage(1, 1, 2),
        exit(),
        exit(),
    ] {
        ticket = offer_with_suspend(&mut arena, ticket, &mut build, event);
        ticket = drain_event(&mut arena, ticket, &mut build);
    }
    let mut session = arena.resume_build(ticket).unwrap();
    build.begin_leaf_barrier(&mut session).unwrap();
    assert_eq!(
        build.poll(&mut session).unwrap(),
        SerializedGreenStreamProgress::Pending
    );
    let abort = session.begin_abort().unwrap();
    let mut polls = 0;
    while !arena.poll_build_abort(abort, 1).unwrap().complete {
        polls += 1;
    }
    assert!(polls <= 2, "one forced leaf should require bounded cleanup");
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
#[allow(clippy::too_many_lines)] // Deliberately exercises every resumable builder boundary.
fn every_event_leaf_branch_and_manifest_boundary_can_suspend_and_resume() {
    const RUNS: u64 = 12_000;
    let mut arena = PageArena::new();
    let mut ticket = arena.begin_build().unwrap();
    let mut build = ResumableSerializedGreenBuild::new(&ticket, root_spec(RUNS)).unwrap();

    for event in [
        enter(1, GreenKind::DOCUMENT),
        enter(2, GreenKind::PARAGRAPH),
    ] {
        ticket = offer_with_suspend(&mut arena, ticket, &mut build, event);
        ticket = drain_event(&mut arena, ticket, &mut build);
    }
    for id in 1..=RUNS {
        ticket = offer_with_suspend(&mut arena, ticket, &mut build, coverage(id, 1, 2));
        ticket = drain_event(&mut arena, ticket, &mut build);
    }
    for event in [exit(), exit()] {
        ticket = offer_with_suspend(&mut arena, ticket, &mut build, event);
        ticket = drain_event(&mut arena, ticket, &mut build);
    }

    ticket = finish_with_suspend(&mut arena, ticket, &mut build);
    let receipt = build.receipt();
    assert!(receipt.leaf_pages_allocated > 16);
    assert!(receipt.branch_nodes_allocated > 8);
    assert!(receipt.maximum_streaming_roots <= 64);
    assert_eq!(receipt.maximum_sequence_bin_logical_slots, 64);
    assert!(receipt.maximum_streaming_bin_bytes >= receipt.maximum_sequence_bin_requested_bytes);
    let leaves = receipt.leaf_pages_allocated as u64;
    assert!(receipt.final_sequence_height > 0);
    assert!(receipt.final_sequence_height <= maximum_avl_height(leaves));
    assert!(receipt.maximum_sequence_join_tasks <= 64);
    assert!(receipt.maximum_sequence_join_values <= 2);
    assert!(
        receipt.maximum_sequence_join_task_capacity_bytes
            >= receipt.maximum_sequence_join_task_requested_bytes
    );
    assert!(
        receipt.maximum_sequence_join_value_capacity_bytes
            >= receipt.maximum_sequence_join_value_requested_bytes
    );
    assert!(receipt.maximum_sequence_join_task_capacity_bytes < 8 * 1024);
    assert!(receipt.maximum_sequence_join_value_capacity_bytes < 1024);
    assert!(receipt.maximum_partial_leaf_payload_capacity <= 4 * 1024);
    assert!(
        receipt.maximum_partial_leaf_payload_capacity
            >= receipt.partial_leaf_payload_requested_bytes
    );
    assert_eq!(receipt.partial_leaf_program_owner_logical_slots, 128);
    assert!(
        receipt.maximum_partial_leaf_program_owner_capacity_bytes
            >= receipt.partial_leaf_program_owner_requested_bytes
    );
    assert!(receipt.maximum_pending_event_payload_capacity < 512);
    assert_eq!(
        receipt.offer_event_descriptor_buffers_created,
        usize::try_from(RUNS).unwrap() + 4
    );
    assert_eq!(receipt.offer_event_facts_buffers_created, 0);
    assert!(receipt.maximum_live_owner_handles <= receipt.owner_journal_capacity);
    assert!(receipt.owner_journal_bytes > 0);

    let manifest = build.take_manifest().unwrap();
    assert_eq!(manifest.receipt(), receipt);
    let document = commit(&mut arena, ticket, manifest);
    assert_eq!(document.metric(&arena).unwrap().bytes, RUNS);
    assert_eq!(document.block_count(&arena).unwrap(), 2);
    let mut first = document
        .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
        .unwrap();
    assert_eq!(
        first
            .next_coverage(&document, &arena)
            .unwrap()
            .unwrap()
            .coverage,
        CoverageId(1)
    );
    let mut last = document
        .seek(
            &arena,
            GreenCoordinate::Bytes,
            RUNS - 1,
            GreenAffinity::Downstream,
        )
        .unwrap();
    assert_eq!(
        last.next_coverage(&document, &arena)
            .unwrap()
            .unwrap()
            .coverage,
        CoverageId(RUNS)
    );
    let far_index = document.leaf_count(&arena).unwrap() - 1;
    let far_leaf = document.leaf_at(&arena, far_index).unwrap().unwrap();
    let paragraph = first
        .open_path()
        .iter()
        .find(|frame| frame.kind == GreenKind::PARAGRAPH)
        .unwrap()
        .enter;
    let next = document
        .rewrite_enters(
            &mut arena,
            ParseGeneration(2),
            2,
            vec![GreenEnterRewrite {
                target: paragraph,
                kind: GreenKind::PARAGRAPH,
                facts: FactsEnvelope::new(vec![FactField::optional(FactId(100), [1])]).unwrap(),
            }],
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap();
    let next_far_index = next.leaf_count(&arena).unwrap() - 1;
    assert_eq!(
        next.leaf_at(&arena, next_far_index).unwrap(),
        Some(far_leaf)
    );
    assert_eq!(
        next.metric(&arena).unwrap(),
        document.metric(&arena).unwrap()
    );
    next.release_later(&mut arena).unwrap();
    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn program_payload_is_immediately_journaled_and_leaf_retains_only_its_owner() {
    let program = ProjectionProgram::new(vec![
        ProjectionPiece::Hidden {
            metric: SerializedMetric { bytes: 1, utf16: 1 },
            affinity: flark_v3_runtime_slice::GreenAffinity::Upstream,
        },
        ProjectionPiece::Identity {
            metric: SerializedMetric { bytes: 1, utf16: 1 },
        },
    ])
    .unwrap();
    let event = GreenEvent::Coverage(
        SourceProjectionRun::with_logical(
            CoverageId(1),
            2,
            2,
            0,
            CoveragePart::CONTENT,
            BlockId(2),
            LogicalContribution::Program(program),
        )
        .unwrap(),
    );
    let mut arena = PageArena::new();
    let ticket = arena.begin_build().unwrap();
    let build_id = ticket.id();
    let mut build = ResumableSerializedGreenBuild::new(&ticket, root_spec(2)).unwrap();
    let mut session = arena.resume_build(ticket).unwrap();

    for structural in [
        enter(1, GreenKind::DOCUMENT),
        enter(2, GreenKind::PARAGRAPH),
    ] {
        build.offer_event(&mut session, structural).unwrap();
        assert_eq!(
            build.poll(&mut session).unwrap(),
            SerializedGreenStreamProgress::ReadyForEvent
        );
    }
    build.offer_event(&mut session, event).unwrap();
    let after_offer = build.receipt();
    assert_eq!(after_offer.projection_program_pages_allocated, 1);
    assert_eq!(session.live_owners().unwrap(), 1);
    assert_eq!(after_offer.maximum_partial_leaf_program_owners, 0);
    assert!(after_offer.maximum_projection_program_payload_len > 0);
    assert!(after_offer.maximum_projection_program_scratch_capacity <= 4 * 1024);

    assert_eq!(
        build.poll(&mut session).unwrap(),
        SerializedGreenStreamProgress::ReadyForEvent
    );
    let after_append = build.receipt();
    assert_eq!(after_append.maximum_partial_leaf_program_owners, 1);
    assert!(after_append.maximum_partial_leaf_program_owner_capacity_bytes > 0);
    assert_eq!(
        session
            .arena()
            .build_journal_metrics(build_id)
            .unwrap()
            .live_owners,
        1
    );

    for structural in [exit(), exit()] {
        build.offer_event(&mut session, structural).unwrap();
        assert_eq!(
            build.poll(&mut session).unwrap(),
            SerializedGreenStreamProgress::ReadyForEvent
        );
    }
    build.finish_input(&mut session).unwrap();
    while build.poll(&mut session).unwrap() != SerializedGreenStreamProgress::ManifestReady {}
    assert_eq!(session.live_owners().unwrap(), 1);
    let manifest = build.take_manifest().unwrap();
    let document = manifest.commit(session).unwrap().0;
    assert_eq!(document.metric(&arena).unwrap().bytes, 2);
    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

#[test]
fn wrong_or_recycled_build_generation_cannot_resume_builder_state() {
    let mut arena = PageArena::new();
    let first = arena.begin_build().unwrap();
    let mut build = ResumableSerializedGreenBuild::new(&first, root_spec(1)).unwrap();
    let first_id = arena.begin_build_abort(first).unwrap();
    assert!(arena.poll_build_abort(first_id, 0).unwrap().complete);

    let recycled = arena.begin_build().unwrap();
    assert_ne!(recycled.id(), first_id);
    let mut wrong_session = arena.resume_build(recycled).unwrap();
    assert_eq!(
        build.offer_event(&mut wrong_session, enter(1, GreenKind::DOCUMENT)),
        Err(SerializedGreenError::Invalid(
            "arena session belongs to another build generation"
        ))
    );
    let abort = wrong_session.begin_abort().unwrap();
    assert!(arena.poll_build_abort(abort, 0).unwrap().complete);
}

#[test]
fn manifest_commit_with_wrong_build_fails_closed_without_crossing_journals() {
    let mut arena = PageArena::new();
    let first_ticket = arena.begin_build().unwrap();
    let first_id = first_ticket.id();
    let mut build = ResumableSerializedGreenBuild::new(&first_ticket, root_spec(1)).unwrap();
    let mut first_session = arena.resume_build(first_ticket).unwrap();
    for event in [
        enter(1, GreenKind::DOCUMENT),
        enter(2, GreenKind::PARAGRAPH),
        coverage(1, 1, 2),
        exit(),
        exit(),
    ] {
        build.offer_event(&mut first_session, event).unwrap();
        assert_eq!(
            build.poll(&mut first_session).unwrap(),
            SerializedGreenStreamProgress::ReadyForEvent
        );
    }
    build.finish_input(&mut first_session).unwrap();
    while build.poll(&mut first_session).unwrap() != SerializedGreenStreamProgress::ManifestReady {}
    assert_eq!(first_session.live_owners().unwrap(), 1);
    let first_ticket = first_session.suspend().unwrap();
    let manifest = build.take_manifest().unwrap();

    let wrong_ticket = arena.begin_build().unwrap();
    let wrong_id = wrong_ticket.id();
    let wrong_session = arena.resume_build(wrong_ticket).unwrap();
    assert_eq!(
        manifest.commit(wrong_session).unwrap_err(),
        SerializedGreenError::Invalid("manifest and arena session build generations differ")
    );

    // Consuming the wrong session is deliberately fail-closed. Its Drop only
    // marks that empty journal aborting; the manifest's journal remains
    // suspended, independently owned, and explicitly cancellable.
    assert_eq!(
        arena.build_lifecycle(wrong_id).unwrap(),
        ArenaBuildLifecycle::Aborting
    );
    assert!(arena.poll_build_abort(wrong_id, 0).unwrap().complete);
    assert_eq!(
        arena.build_lifecycle(first_id).unwrap(),
        ArenaBuildLifecycle::Suspended
    );
    assert_eq!(
        arena.build_journal_metrics(first_id).unwrap().live_owners,
        1
    );
    let abort = arena.begin_build_abort(first_ticket).unwrap();
    assert_eq!(abort, first_id);
    assert!(arena.poll_build_abort(abort, 1).unwrap().complete);
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn late_source_mismatch_aborts_in_constant_time_and_fuelled_cleanup_preserves_base() {
    let mut arena = PageArena::new();

    let base_ticket = arena.begin_build().unwrap();
    let mut base = ResumableSerializedGreenBuild::new(&base_ticket, root_spec(1)).unwrap();
    let mut session = arena.resume_build(base_ticket).unwrap();
    for event in [
        enter(1, GreenKind::DOCUMENT),
        enter(2, GreenKind::PARAGRAPH),
        coverage(1, 1, 2),
        exit(),
        exit(),
    ] {
        base.offer_event(&mut session, event).unwrap();
        assert_eq!(
            base.poll(&mut session).unwrap(),
            SerializedGreenStreamProgress::ReadyForEvent
        );
    }
    base.finish_input(&mut session).unwrap();
    while base.poll(&mut session).unwrap() != SerializedGreenStreamProgress::ManifestReady {}
    let base_document = base.take_manifest().unwrap().commit(session).unwrap().0;
    assert_eq!(base_document.metric(&arena).unwrap().bytes, 1);

    let ticket = arena.begin_build().unwrap();
    let mut candidate = ResumableSerializedGreenBuild::new(&ticket, root_spec(2)).unwrap();
    let mut session = arena.resume_build(ticket).unwrap();
    for event in [
        enter(10, GreenKind::DOCUMENT),
        enter(11, GreenKind::PARAGRAPH),
        coverage(10, 1, 11),
        exit(),
        exit(),
    ] {
        candidate.offer_event(&mut session, event).unwrap();
        assert_eq!(
            candidate.poll(&mut session).unwrap(),
            SerializedGreenStreamProgress::ReadyForEvent
        );
    }
    candidate.finish_input(&mut session).unwrap();
    let error = loop {
        match candidate.poll(&mut session) {
            Ok(SerializedGreenStreamProgress::Pending) => {}
            Err(error) => break error,
            other => panic!("unexpected late-validation result: {other:?}"),
        }
    };
    assert_eq!(
        error,
        SerializedGreenError::Invalid("green coverage does not match bound source length")
    );
    assert_eq!(base_document.metric(session.arena()).unwrap().bytes, 1);
    let build_id = session.id();
    let owners = session.live_owners().unwrap();
    assert!(owners > 0);
    let abort = session.begin_abort().unwrap();
    assert_eq!(abort, build_id);
    assert_eq!(
        arena.build_lifecycle(abort).unwrap(),
        ArenaBuildLifecycle::Aborting
    );
    assert_eq!(base_document.metric(&arena).unwrap().bytes, 1);

    let zero = arena.poll_build_abort(abort, 0).unwrap();
    assert_eq!(zero.owners_scheduled, 0);
    assert_eq!(zero.owners_remaining, owners);
    let mut remaining = owners;
    while remaining != 0 {
        let receipt = arena.poll_build_abort(abort, 1).unwrap();
        assert!(receipt.owners_scheduled <= 1);
        assert!(receipt.owners_remaining < remaining);
        remaining = receipt.owners_remaining;
    }
    settle(&mut arena);
    assert_eq!(base_document.metric(&arena).unwrap().bytes, 1);
    base_document.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

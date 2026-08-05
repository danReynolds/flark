use flark_v3_runtime_slice::{
    BlockId, ClosedChildAggregate, CoverageId, CoveragePart, CoverageRun, FactField, FactId,
    FactsEnvelope, GrammarRevision, GreenAffinity, GreenCloseFacts, GreenCoordinate,
    GreenEnterRewrite, GreenEvent, GreenFenceCharacter, GreenFencedCodeCloseFacts,
    GreenFencedCodeOpenFacts, GreenHeadingOpenFacts, GreenHeadingStyle, GreenItemOpenFacts,
    GreenKind, GreenListBullet, GreenListDelimiter, GreenListOpenFacts, GreenRelativeLogicalSlice,
    LogicalContribution, PageArena, ParseGeneration, SerializedGreenBuildReceipt,
    SerializedGreenDocument, SerializedGreenError, SerializedGreenRootSpec, SourceRevision,
    SourceRootId, serialized_green_retained_receipt,
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

fn coverage(
    id: u64,
    bytes: u64,
    utf16: u64,
    owner_relative_depth: u32,
    part: CoveragePart,
) -> GreenEvent {
    GreenEvent::Coverage(
        CoverageRun::new(CoverageId(id), bytes, utf16, owner_relative_depth, part).unwrap(),
    )
}

fn exit() -> GreenEvent {
    GreenEvent::exit(ClosedChildAggregate::default())
}

fn exit_list(tight: bool) -> GreenEvent {
    GreenEvent::exit_with_facts(
        ClosedChildAggregate::default(),
        GreenCloseFacts::List { tight },
    )
}

fn settle(arena: &mut PageArena) {
    while arena.metrics().pending_releases != 0 {
        arena.poll_reclaim(10_000).unwrap();
    }
}

#[test]
fn packed_seek_recovers_interleaved_owner_open_path_and_exact_utf16_ranges() {
    let events = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::BLOCK_QUOTE, FactsEnvelope::empty()),
        coverage(1, 2, 2, 0, CoveragePart::CONTAINER_MARKER),
        enter(3, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        coverage(2, 5, 3, 0, CoveragePart::CONTENT),
        coverage(3, 2, 2, 1, CoveragePart::CONTAINER_MARKER),
        coverage(4, 2, 2, 0, CoveragePart::CONTENT),
        exit(),
        coverage(5, 1, 1, 0, CoveragePart::GAP),
        exit(),
        coverage(6, 1, 1, 0, CoveragePart::GAP),
        exit(),
    ];
    let mut arena = PageArena::new();
    let document = SerializedGreenDocument::build(
        &mut arena,
        root_spec_with_utf16(13, 11),
        events,
        &mut SerializedGreenBuildReceipt::default(),
    )
    .unwrap();
    assert_eq!(document.metric(&arena).unwrap().bytes, 13);
    assert_eq!(document.metric(&arena).unwrap().utf16, 11);

    let mut cursor = document
        .seek(&arena, GreenCoordinate::Bytes, 7, GreenAffinity::Downstream)
        .unwrap();
    let marker = cursor.next_coverage(&document, &arena).unwrap().unwrap();
    assert_eq!(marker.coverage, CoverageId(3));
    assert_eq!(marker.owner.block, BlockId(2));
    assert_eq!(
        cursor
            .open_path()
            .iter()
            .map(|frame| frame.block)
            .collect::<Vec<_>>(),
        [BlockId(1), BlockId(2), BlockId(3)]
    );
    assert_eq!(marker.byte_range, 7..9);
    assert_eq!(marker.utf16_range, 5..7);

    let content = cursor.next_coverage(&document, &arena).unwrap().unwrap();
    assert_eq!(content.coverage, CoverageId(4));
    assert_eq!(content.owner.block, BlockId(3));
    let quote_gap = cursor.next_coverage(&document, &arena).unwrap().unwrap();
    assert_eq!(quote_gap.coverage, CoverageId(5));
    assert_eq!(quote_gap.owner.block, BlockId(2));
    let document_gap = cursor.next_coverage(&document, &arena).unwrap().unwrap();
    assert_eq!(document_gap.owner.block, BlockId(1));
    assert!(cursor.next_coverage(&document, &arena).unwrap().is_none());
    assert_eq!(cursor.receipt().root_descents, 1);
    assert_eq!(cursor.receipt().successor_root_descents, 0);

    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn facts_envelope_is_atomic_canonical_and_kind_checked() {
    let typed_list = GreenListOpenFacts::ordered(1, GreenListDelimiter::Period).unwrap();
    let list = typed_list.into_envelope();
    let typed_item = GreenItemOpenFacts::new(0, 3).unwrap();
    let item = typed_item.into_envelope();
    assert_eq!(GreenListOpenFacts::try_from_envelope(&list), Ok(typed_list));
    assert_eq!(GreenItemOpenFacts::try_from_envelope(&item), Ok(typed_item));
    let events = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::LIST, list),
        enter(3, GreenKind::ITEM, item),
        enter(4, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        coverage(1, 1, 1, 0, CoveragePart::CONTENT),
        exit(),
        exit(),
        exit_list(true),
        exit(),
    ];
    let mut arena = PageArena::new();
    let document = SerializedGreenDocument::build(
        &mut arena,
        root_spec(1),
        events,
        &mut SerializedGreenBuildReceipt::default(),
    )
    .unwrap();
    document.release_later(&mut arena).unwrap();
    settle(&mut arena);

    assert_eq!(
        FactsEnvelope::new(vec![
            FactField::critical(FactId::ITEM, [0, 0, 1, 0]),
            FactField::critical(FactId::LIST, [0; 8]),
        ]),
        Err(SerializedGreenError::Invalid(
            "facts must be strictly ordered and unique"
        ))
    );

    let invalid = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::HEADING, FactsEnvelope::empty()),
        coverage(1, 1, 1, 0, CoveragePart::CONTENT),
        exit(),
        exit(),
    ];
    assert_eq!(
        SerializedGreenDocument::build(
            &mut arena,
            root_spec(1),
            invalid,
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap_err(),
        SerializedGreenError::Invalid("required kind fact is missing")
    );
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn typed_list_and_item_facts_have_one_canonical_storage_form() {
    let bullet = GreenListOpenFacts::bullet(GreenListBullet::Dash);
    let bullet_envelope = bullet.into_envelope();
    assert_eq!(bullet_envelope.fields[0].value, [1, b'-', 0, 0, 1, 0, 0, 0]);
    assert_eq!(
        GreenListOpenFacts::try_from_envelope(&bullet_envelope),
        Ok(bullet)
    );

    let ordered = GreenListOpenFacts::ordered(42, GreenListDelimiter::Parenthesis).unwrap();
    let ordered_envelope = ordered.into_envelope();
    assert_eq!(
        ordered_envelope.fields[0].value,
        [2, 0, b')', 0, 42, 0, 0, 0]
    );
    assert_eq!(
        GreenListOpenFacts::try_from_envelope(&ordered_envelope),
        Ok(ordered)
    );

    let item = GreenItemOpenFacts::new(3, 14).unwrap();
    let item_envelope = item.into_envelope();
    assert_eq!(item_envelope.fields[0].value, [3, 0, 14, 0]);
    assert_eq!(
        GreenItemOpenFacts::try_from_envelope(&item_envelope),
        Ok(item)
    );

    for payload in [
        [2, b'.', b'1', 0, 1, 0, 0, 0],
        [1, b'-', 0, 1, 1, 0, 0, 0],
        [1, b'?', 0, 0, 1, 0, 0, 0],
        [2, 0, b'.', 0, 0, 202, 154, 59],
    ] {
        let raw = FactsEnvelope::new(vec![FactField::critical(FactId::LIST, payload)]).unwrap();
        assert!(GreenListOpenFacts::try_from_envelope(&raw).is_err());
    }
    for payload in [[4, 0, 2, 0], [0, 0, 1, 0], [0, 0, 15, 0]] {
        let raw = FactsEnvelope::new(vec![FactField::critical(FactId::ITEM, payload)]).unwrap();
        assert!(GreenItemOpenFacts::try_from_envelope(&raw).is_err());
    }
    assert!(GreenListOpenFacts::ordered(1_000_000_000, GreenListDelimiter::Period).is_err());
}

#[test]
fn typed_heading_and_fenced_code_facts_round_trip_without_length_truncation() {
    for heading in [
        GreenHeadingOpenFacts::atx(6).unwrap(),
        GreenHeadingOpenFacts::setext(2).unwrap(),
    ] {
        assert_eq!(
            GreenHeadingOpenFacts::try_from_envelope(&heading.into_envelope()),
            Ok(heading)
        );
    }
    assert_eq!(
        GreenHeadingOpenFacts::setext(2).unwrap().style(),
        GreenHeadingStyle::Setext
    );
    assert!(GreenHeadingOpenFacts::atx(0).is_err());
    assert!(GreenHeadingOpenFacts::setext(3).is_err());

    let giant_length = u64::MAX - 17;
    let fenced =
        GreenFencedCodeOpenFacts::new(GreenFenceCharacter::Tilde, giant_length, 3).unwrap();
    let envelope = fenced.into_envelope();
    assert_eq!(envelope.fields[0].value.len(), 10);
    assert_eq!(
        GreenFencedCodeOpenFacts::try_from_envelope(&envelope),
        Ok(fenced)
    );
    assert_eq!(
        GreenFencedCodeOpenFacts::try_from_envelope(&envelope)
            .unwrap()
            .minimum_closing_length(),
        giant_length
    );

    let invalid_heading =
        FactsEnvelope::new(vec![FactField::critical(FactId::HEADING, [3, 1])]).unwrap();
    assert!(GreenHeadingOpenFacts::try_from_envelope(&invalid_heading).is_err());
    for payload in [
        vec![b'?', 0, 3, 0, 0, 0, 0, 0, 0, 0],
        vec![b'`', 4, 3, 0, 0, 0, 0, 0, 0, 0],
        vec![b'~', 0, 2, 0, 0, 0, 0, 0, 0, 0],
    ] {
        let raw = FactsEnvelope::new(vec![FactField::critical(FactId::CODE, payload)]).unwrap();
        assert!(GreenFencedCodeOpenFacts::try_from_envelope(&raw).is_err());
    }
}

#[test]
fn heading_and_fenced_code_typed_facts_survive_the_packed_trace() {
    let heading = GreenHeadingOpenFacts::setext(1).unwrap();
    let fenced = GreenFencedCodeOpenFacts::new(GreenFenceCharacter::Backtick, 300, 0).unwrap();
    let fenced_close = GreenFencedCodeCloseFacts::new(
        false,
        GreenRelativeLogicalSlice::new(0..0, 0..0).unwrap(),
        GreenRelativeLogicalSlice::new(0..1, 0..1).unwrap(),
    )
    .unwrap();
    let events = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::HEADING, heading.into_envelope()),
        GreenEvent::Coverage(
            CoverageRun::with_logical(
                CoverageId(1),
                1,
                1,
                0,
                CoveragePart::CONTENT,
                BlockId(2),
                LogicalContribution::Identity,
            )
            .unwrap(),
        ),
        exit(),
        enter(3, GreenKind::FENCED_CODE, fenced.into_envelope()),
        GreenEvent::Coverage(
            CoverageRun::with_logical(
                CoverageId(2),
                1,
                1,
                0,
                CoveragePart::CONTENT,
                BlockId(3),
                LogicalContribution::Identity,
            )
            .unwrap(),
        ),
        GreenEvent::exit_with_facts(
            ClosedChildAggregate::default(),
            GreenCloseFacts::FencedCode(fenced_close),
        ),
        exit(),
    ];
    let mut arena = PageArena::new();
    let document = SerializedGreenDocument::build(
        &mut arena,
        root_spec(2),
        events,
        &mut SerializedGreenBuildReceipt::default(),
    )
    .unwrap();

    let heading_cursor = document
        .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
        .unwrap();
    let heading_frame = heading_cursor.open_path().last().unwrap();
    assert_eq!(
        GreenHeadingOpenFacts::try_from_envelope(&heading_frame.facts),
        Ok(heading)
    );
    let fenced_cursor = document
        .seek(&arena, GreenCoordinate::Bytes, 1, GreenAffinity::Downstream)
        .unwrap();
    let fenced_frame = fenced_cursor.open_path().last().unwrap();
    assert_eq!(
        GreenFencedCodeOpenFacts::try_from_envelope(&fenced_frame.facts),
        Ok(fenced)
    );
    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

#[test]
fn list_close_facts_are_required_and_rejected_on_other_kinds() {
    let list = GreenListOpenFacts::bullet(GreenListBullet::Asterisk).into_envelope();
    let missing = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::LIST, list),
        exit(),
        exit(),
    ];
    let mut arena = PageArena::new();
    assert_eq!(
        SerializedGreenDocument::build(
            &mut arena,
            root_spec(0),
            missing,
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap_err(),
        SerializedGreenError::Invalid("List Exit is missing its close-time tightness fact")
    );

    let wrong_kind = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        exit_list(false),
        exit(),
    ];
    assert_eq!(
        SerializedGreenDocument::build(
            &mut arena,
            root_spec(0),
            wrong_kind,
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap_err(),
        SerializedGreenError::Invalid("List close-time facts require a List binding")
    );
    settle(&mut arena);
}

#[test]
fn far_viewport_uses_one_root_descent_and_a_leaf_zipper() {
    const RUNS: u64 = 2_000;
    let events = std::iter::once(enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()))
        .chain(std::iter::once(enter(
            2,
            GreenKind::PARAGRAPH,
            FactsEnvelope::empty(),
        )))
        .chain((0..RUNS).map(|index| coverage(index + 1, 1, 1, 0, CoveragePart::CONTENT)))
        .chain([exit(), exit()]);
    let mut arena = PageArena::new();
    let document = SerializedGreenDocument::build(
        &mut arena,
        root_spec(RUNS),
        events,
        &mut SerializedGreenBuildReceipt::default(),
    )
    .unwrap();
    assert!(document.leaf_count(&arena).unwrap() > 2);
    let mut cursor = document
        .seek(
            &arena,
            GreenCoordinate::Bytes,
            1_500,
            GreenAffinity::Downstream,
        )
        .unwrap();
    for expected in 1_501..=1_900 {
        let view = cursor.next_coverage(&document, &arena).unwrap().unwrap();
        assert_eq!(view.coverage, CoverageId(expected));
        assert_eq!(view.owner.block, BlockId(2));
    }
    assert_eq!(cursor.receipt().root_descents, 1);
    assert_eq!(cursor.receipt().successor_root_descents, 0);
    assert!(cursor.receipt().leaf_pages_decoded > 1);

    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
}

#[test]
fn depth_hundred_enter_rewrite_is_one_base_root_batch_and_keeps_far_pages_exact() {
    const DEPTH: u64 = 100;
    const TAIL_RUNS: u64 = 2_000;
    let events = std::iter::once(enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()))
        .chain(
            (0..DEPTH)
                .map(|index| enter(index + 2, GreenKind::BLOCK_QUOTE, FactsEnvelope::empty())),
        )
        .chain(std::iter::once(coverage(1, 1, 1, 0, CoveragePart::CONTENT)))
        .chain((0..DEPTH).map(|_| exit()))
        .chain(std::iter::once(enter(
            DEPTH + 2,
            GreenKind::PARAGRAPH,
            FactsEnvelope::empty(),
        )))
        .chain((0..TAIL_RUNS).map(|index| coverage(index + 2, 1, 1, 0, CoveragePart::CONTENT)))
        .chain([exit(), exit()]);
    let mut arena = PageArena::new();
    let document = SerializedGreenDocument::build(
        &mut arena,
        root_spec(TAIL_RUNS + 1),
        events,
        &mut SerializedGreenBuildReceipt::default(),
    )
    .unwrap();
    let far_index = document.leaf_count(&arena).unwrap() - 1;
    let far_leaf = document.leaf_at(&arena, far_index).unwrap().unwrap();
    let cursor = document
        .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
        .unwrap();
    assert_eq!(
        cursor.open_path().len(),
        usize::try_from(DEPTH + 1).unwrap()
    );
    let facts = FactsEnvelope::new(vec![FactField::optional(FactId(100), [7])]).unwrap();
    let rewrites = cursor
        .open_path()
        .iter()
        .skip(1)
        .map(|frame| GreenEnterRewrite {
            target: frame.enter,
            kind: frame.kind,
            facts: facts.clone(),
        })
        .collect();
    let mut receipt = SerializedGreenBuildReceipt::default();
    let next = document
        .rewrite_enters(&mut arena, ParseGeneration(2), 2, rewrites, &mut receipt)
        .unwrap();
    let next_far_index = next.leaf_count(&arena).unwrap() - 1;
    assert_eq!(
        next.leaf_at(&arena, next_far_index).unwrap(),
        Some(far_leaf)
    );
    assert_eq!(
        receipt.sequence_leaves_reused,
        usize::try_from(far_index).unwrap()
    );
    assert!(receipt.maximum_decoded_page_buffer_bytes < 64 * 1024);

    let next_cursor = next
        .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
        .unwrap();
    assert!(
        next_cursor
            .open_path()
            .iter()
            .skip(1)
            .all(|frame| frame.facts == facts)
    );
    let old_cursor = document
        .seek(&arena, GreenCoordinate::Bytes, 0, GreenAffinity::Downstream)
        .unwrap();
    assert!(
        old_cursor
            .open_path()
            .iter()
            .skip(1)
            .all(|frame| frame.facts.fields.is_empty())
    );

    next.release_later(&mut arena).unwrap();
    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn fenced_code_close_facts_are_required_and_kind_checked() {
    let open = GreenFencedCodeOpenFacts::new(GreenFenceCharacter::Backtick, 3, 0)
        .unwrap()
        .into_envelope();
    let close = GreenFencedCodeCloseFacts::new(
        true,
        GreenRelativeLogicalSlice::new(0..0, 0..0).unwrap(),
        GreenRelativeLogicalSlice::new(0..0, 0..0).unwrap(),
    )
    .unwrap();
    let mut arena = PageArena::new();

    let missing = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::FENCED_CODE, open),
        exit(),
        exit(),
    ];
    assert_eq!(
        SerializedGreenDocument::build(
            &mut arena,
            root_spec(0),
            missing,
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap_err(),
        SerializedGreenError::Invalid("FencedCode Exit is missing its close-time projection facts")
    );

    let wrong_kind = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
        GreenEvent::exit_with_facts(
            ClosedChildAggregate::default(),
            GreenCloseFacts::FencedCode(close),
        ),
        exit(),
    ];
    assert_eq!(
        SerializedGreenDocument::build(
            &mut arena,
            root_spec(0),
            wrong_kind,
            &mut SerializedGreenBuildReceipt::default(),
        )
        .unwrap_err(),
        SerializedGreenError::Invalid("FencedCode close-time facts require a FencedCode binding")
    );
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
fn hundred_thousand_item_document_is_packed_only_and_fully_accounted() {
    const ITEMS: u64 = 100_000;
    let list_facts = GreenListOpenFacts::ordered(1, GreenListDelimiter::Period)
        .unwrap()
        .into_envelope();
    let prefix = [
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(2, GreenKind::LIST, list_facts),
    ];
    let body = (0..ITEMS).flat_map(|index| {
        let item = BlockId(3 + index * 2);
        let paragraph = BlockId(item.0 + 1);
        let coverage = 1 + index * 3;
        [
            GreenEvent::enter(
                item,
                GreenKind::ITEM,
                GreenItemOpenFacts::new(0, 3).unwrap().into_envelope(),
            ),
            GreenEvent::Coverage(
                CoverageRun::new(
                    CoverageId(coverage),
                    1,
                    1,
                    0,
                    CoveragePart::CONTAINER_MARKER,
                )
                .unwrap(),
            ),
            GreenEvent::enter(paragraph, GreenKind::PARAGRAPH, FactsEnvelope::empty()),
            GreenEvent::Coverage(
                CoverageRun::with_logical(
                    CoverageId(coverage + 1),
                    1,
                    1,
                    0,
                    CoveragePart::CONTENT,
                    paragraph,
                    LogicalContribution::Identity,
                )
                .unwrap(),
            ),
            exit(),
            GreenEvent::Coverage(
                CoverageRun::new(CoverageId(coverage + 2), 1, 1, 0, CoveragePart::GAP).unwrap(),
            ),
            GreenEvent::exit(ClosedChildAggregate {
                ends_blank: false,
                item_loose_if_nonlast: false,
                item_loose_if_last: false,
            }),
        ]
    });
    let events = prefix
        .into_iter()
        .chain(body)
        .chain([exit_list(true), exit()]);
    let mut arena = PageArena::new();
    let mut build = SerializedGreenBuildReceipt::default();
    let document =
        SerializedGreenDocument::build(&mut arena, root_spec(ITEMS * 3), events, &mut build)
            .unwrap();
    assert_eq!(document.block_count(&arena).unwrap(), ITEMS * 2 + 2);
    assert!(build.maximum_encoded_page_buffer_bytes <= 4_096);
    let retained = serialized_green_retained_receipt(&arena, 1);
    let blocks = usize::try_from(ITEMS * 2 + 2).unwrap();
    let hundredths_per_block = retained.accounted_retained_bytes * 100 / blocks;
    eprintln!(
        "packed serialized green: blocks={} leaves={} live_nodes={} payload={} edges={} slot_capacity={} slot_bytes={} allocator={} total={} bytes_per_block={}.{:02} page_scratch={} journal={}",
        ITEMS * 2 + 2,
        document.leaf_count(&arena).unwrap(),
        retained.live_nodes,
        retained.live_payload_bytes,
        retained.live_edge_bytes,
        retained.slot_capacity,
        retained.slot_storage_bytes,
        retained.modeled_allocator_bytes,
        retained.accounted_retained_bytes,
        hundredths_per_block / 100,
        hundredths_per_block % 100,
        build.maximum_encoded_page_buffer_bytes,
        build.owner_journal_bytes,
    );
    assert!(retained.accounted_retained_bytes < blocks * 64);

    document.release_later(&mut arena).unwrap();
    settle(&mut arena);
    assert_eq!(arena.metrics().live_nodes, 0);
}

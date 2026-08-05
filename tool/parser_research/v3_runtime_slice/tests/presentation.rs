use flark_v3_runtime_slice::{
    ARENA_PAGE_BYTES, AmbiguityTag, CommandCapabilities, CommandScopeTag, ExplicitCoverageOrder,
    FenceTargetRole, ForestAnchor, ForestBlockId, ForestCoverageId, ForestPropertyId,
    ForestRunCursorId, GrammarRevision, HostLayoutLease, InlineSyntaxTag, LayoutGeneration,
    PRESENTATION_FACTS_PER_PAGE, PRESENTATION_MANIFEST_BYTES, PRESENTATION_PACKED_FACT_BYTES,
    PageArena, ParseGeneration, PresentationAuthority, PresentationBudget,
    PresentationBuildOutcome, PresentationCap, PresentationEpoch, PresentationFact,
    PresentationFactBuilder, PresentationFactClass, PresentationHostId, PresentationLookup,
    PresentationPushResult, PresentationRange, PresentationRequest, PresentationRequestId,
    PresentationRequestScope, PresentationStyleTag, PresentationUnknownRange,
    PresentationUnknownReason, ReplacementSymbolId, ReplacementTag, RunEdgeKind,
    SemanticRootGeneration, SourceRevision, TableTargetRole, TaskTargetState,
    query_optional_presentation,
};

fn anchor(coverage: u64, offset: u32) -> ForestAnchor {
    ForestAnchor {
        coverage: ForestCoverageId(coverage),
        local_bytes: offset,
        local_utf16: offset,
    }
}

fn range(coverage: u64, start: u32, end: u32) -> PresentationRange {
    PresentationRange {
        start: anchor(coverage, start),
        end: anchor(coverage, end),
    }
}

fn epoch(source: u64, generation: u64) -> PresentationEpoch {
    PresentationEpoch {
        source: SourceRevision(source),
        grammar: GrammarRevision(7),
        generation: ParseGeneration(generation),
        semantic_root: SemanticRootGeneration(generation),
    }
}

fn request(scope: PresentationRequestScope, end: u32) -> PresentationRequest {
    PresentationRequest {
        id: PresentationRequestId(41),
        scope,
        range: range(1, 0, end),
        required_authority: PresentationAuthority::NONE,
    }
}

fn requiring(
    mut request: PresentationRequest,
    authority: PresentationAuthority,
) -> PresentationRequest {
    request.required_authority = authority;
    request
}

fn order() -> ExplicitCoverageOrder {
    ExplicitCoverageOrder::from_ids([ForestCoverageId(1), ForestCoverageId(2)])
        .expect("unique coverage order")
}

fn generous_budget() -> PresentationBudget {
    PresentationBudget::new(8, 512, 64 * 1024)
}

fn settle_transfers(arena: &mut PageArena) {
    while arena.metrics().pending_releases > 0 {
        arena.poll_reclaim(8).expect("settle ownership transfers");
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn ordinary_active_facts_round_trip_as_one_exact_revision_bound_snapshot() {
    let epoch = epoch(11, 3);
    let authority = PresentationAuthority::INLINE_PROJECTION
        .union(PresentationAuthority::REFERENCE_RESOLUTION)
        .union(PresentationAuthority::INTERACTION_TARGETS)
        .union(PresentationAuthority::COMMAND_CAPABILITIES);
    let request = requiring(request(PresentationRequestScope::ActiveEdit, 40), authority);
    let capabilities = CommandCapabilities::TOGGLE.union(CommandCapabilities::NAVIGATE);
    let facts = vec![
        PresentationFact::InlineHidden {
            range: range(1, 0, 2),
            syntax: InlineSyntaxTag(1),
            nesting: 2,
        },
        PresentationFact::Replacement {
            range: range(1, 2, 4),
            replacement: ReplacementTag(2),
            symbol: ReplacementSymbolId(99),
        },
        PresentationFact::Style {
            range: range(1, 4, 8),
            style: PresentationStyleTag(3),
            layer: 4,
        },
        PresentationFact::Ambiguity {
            range: range(1, 8, 10),
            ambiguity: AmbiguityTag(5),
            alternatives: 2,
        },
        PresentationFact::RunEdge {
            at: anchor(1, 10),
            run: ForestRunCursorId(101),
            edge: RunEdgeKind::SplitAfter,
            ordinal: 6,
        },
        PresentationFact::TableTarget {
            range: range(1, 10, 14),
            table: ForestBlockId(102),
            role: TableTargetRole::Cell,
            row: 7,
            column: 8,
        },
        PresentationFact::FenceTarget {
            range: range(1, 14, 18),
            fence: ForestBlockId(103),
            role: FenceTargetRole::Info,
            property: Some(ForestPropertyId(104)),
        },
        PresentationFact::TaskTarget {
            range: range(1, 18, 21),
            item: ForestBlockId(105),
            state: TaskTargetState::Checked,
        },
        PresentationFact::CommandCapabilities {
            range: range(1, 18, 21),
            target: ForestBlockId(105),
            scope: CommandScopeTag(9),
            capabilities,
        },
    ];
    assert!(capabilities.contains(CommandCapabilities::TOGGLE));
    assert!(capabilities.contains(CommandCapabilities::NAVIGATE));

    let mut builder = PresentationFactBuilder::new(epoch, request, authority, generous_budget());
    for fact in &facts {
        assert_eq!(builder.push(*fact), PresentationPushResult::Accepted);
    }
    let mut arena = PageArena::new();
    let PresentationBuildOutcome::Exact { lease, receipt } = builder
        .finish(&mut arena, &order())
        .expect("build exact snapshot")
    else {
        panic!("ordinary active facts fit the explicit budget");
    };
    settle_transfers(&mut arena);
    assert_eq!(receipt.facts_packed, facts.len());
    assert_eq!(receipt.fact_pages_allocated, 1);
    assert!(receipt.maximum_page_payload_bytes <= ARENA_PAGE_BYTES);

    assert_eq!(
        lease
            .query(&arena, epoch, request, &order())
            .expect("epoch-bound query"),
        PresentationLookup::Exact(flark_v3_runtime_slice::ExactPresentationFacts {
            epoch,
            request,
            facts,
        })
    );
    lease
        .release_later(&mut arena)
        .expect("schedule immutable manifest retirement");
    while arena.metrics().pending_releases > 0 {
        let reclaimed = arena.poll_reclaim(1).expect("strictly fuelled retirement");
        assert!(reclaimed.reference_transitions <= 1);
        assert!(reclaimed.nodes_reclaimed <= 1);
        assert!(reclaimed.payload_bytes_reclaimed <= ARENA_PAGE_BYTES);
    }
    assert_eq!(arena.metrics().live_nodes, 0);
}

#[test]
#[allow(clippy::too_many_lines)] // One table proves every atomic epoch/dimension rejection.
fn stale_epochs_and_wrong_requests_never_expose_even_one_fact_dimension() {
    let published_epoch = epoch(20, 4);
    let request = requiring(
        request(PresentationRequestScope::Viewport, 20),
        PresentationAuthority::INLINE_PROJECTION,
    );
    let fact = PresentationFact::Style {
        range: range(1, 1, 5),
        style: PresentationStyleTag(9),
        layer: 1,
    };
    let mut builder = PresentationFactBuilder::new(
        published_epoch,
        request,
        PresentationAuthority::INLINE_PROJECTION,
        generous_budget(),
    );
    assert_eq!(builder.push(fact), PresentationPushResult::Accepted);
    let mut arena = PageArena::new();
    let PresentationBuildOutcome::Exact { lease, .. } = builder
        .finish(&mut arena, &order())
        .expect("build snapshot")
    else {
        panic!("fact fits");
    };
    settle_transfers(&mut arena);

    let cases = [
        (
            PresentationEpoch {
                source: SourceRevision(21),
                ..published_epoch
            },
            request,
            PresentationUnknownReason::StaleSourceRevision,
        ),
        (
            PresentationEpoch {
                grammar: GrammarRevision(8),
                ..published_epoch
            },
            request,
            PresentationUnknownReason::StaleGrammarRevision,
        ),
        (
            PresentationEpoch {
                generation: ParseGeneration(5),
                ..published_epoch
            },
            request,
            PresentationUnknownReason::StaleParseGeneration,
        ),
        (
            PresentationEpoch {
                semantic_root: SemanticRootGeneration(5),
                ..published_epoch
            },
            request,
            PresentationUnknownReason::StaleSemanticRoot,
        ),
        (
            published_epoch,
            PresentationRequest {
                id: PresentationRequestId(42),
                ..request
            },
            PresentationUnknownReason::WrongRequest,
        ),
    ];
    for (expected_epoch, requested, reason) in cases {
        assert_eq!(
            lease
                .query_required_class(
                    &arena,
                    expected_epoch,
                    requested,
                    PresentationFactClass::Style,
                    &order(),
                )
                .expect("mismatch is an explicit unknown, not an error"),
            PresentationLookup::Unknown(PresentationUnknownRange {
                range: requested.range,
                reason,
            })
        );
    }
    let atomic_required =
        PresentationAuthority::INLINE_PROJECTION.union(PresentationAuthority::COMMAND_CAPABILITIES);
    let atomic_request = requiring(request, atomic_required);
    assert_eq!(
        lease
            .query(&arena, published_epoch, atomic_request, &order())
            .expect("partial authority is an explicit unknown"),
        PresentationLookup::Unknown(PresentationUnknownRange {
            range: request.range,
            reason: PresentationUnknownReason::IncompleteAuthority {
                required: atomic_required,
                certified: PresentationAuthority::INLINE_PROJECTION,
            },
        }),
        "the available inline dimension is not exposed when the atomic query also needs commands"
    );
    assert_eq!(
        lease
            .query_required_class(
                &arena,
                published_epoch,
                request,
                PresentationFactClass::Replacement,
                &order(),
            )
            .expect("certified exact-empty dimension"),
        PresentationLookup::Exact(flark_v3_runtime_slice::ExactPresentationFacts {
            epoch: published_epoch,
            request,
            facts: Vec::new(),
        })
    );
    lease.release_later(&mut arena).expect("release");
}

#[test]
fn record_page_and_total_payload_caps_fail_closed_before_arena_allocation() {
    let epoch = epoch(1, 1);
    let request = requiring(
        request(PresentationRequestScope::ActiveEdit, 200),
        PresentationAuthority::INLINE_PROJECTION,
    );
    let fact = |start| PresentationFact::Style {
        range: range(1, start, start + 1),
        style: PresentationStyleTag(1),
        layer: 0,
    };
    let cases = [
        (
            PresentationBudget::new(8, 1, 64 * 1024),
            2,
            PresentationCap::Records,
        ),
        (
            PresentationBudget::new(1, 200, 64 * 1024),
            PRESENTATION_FACTS_PER_PAGE + 1,
            PresentationCap::Pages,
        ),
        (
            PresentationBudget::new(
                8,
                200,
                u32::try_from(PRESENTATION_MANIFEST_BYTES).expect("small manifest"),
            ),
            1,
            PresentationCap::ArenaPayloadBytes,
        ),
    ];
    for (budget, pushes, cap) in cases {
        let mut builder = PresentationFactBuilder::new(
            epoch,
            request,
            PresentationAuthority::INLINE_PROJECTION,
            budget,
        );
        let mut terminal = None;
        for index in 0..pushes {
            let result = builder.push(fact(u32::try_from(index).expect("small test")));
            if matches!(result, PresentationPushResult::BecameUnknown(_)) {
                terminal = Some(result);
            }
        }
        let expected = PresentationUnknownRange {
            range: request.range,
            reason: PresentationUnknownReason::CapExceeded(cap),
        };
        assert_eq!(
            terminal,
            Some(PresentationPushResult::BecameUnknown(expected))
        );
        let mut arena = PageArena::new();
        assert!(matches!(
            builder.finish(&mut arena, &order()).expect("unknown outcome"),
            PresentationBuildOutcome::Unknown(unknown) if unknown == expected
        ));
        assert_eq!(arena.metrics().live_nodes, 0);
        assert_eq!(arena.metrics().live_payload_bytes, 0);
    }
}

#[test]
fn authority_completeness_distinguishes_exact_empty_from_not_produced() {
    let epoch = epoch(9, 2);
    let required =
        PresentationAuthority::INLINE_PROJECTION.union(PresentationAuthority::REFERENCE_RESOLUTION);
    let incomplete_request = requiring(request(PresentationRequestScope::Viewport, 20), required);
    let incomplete = PresentationFactBuilder::new(
        epoch,
        incomplete_request,
        PresentationAuthority::INLINE_PROJECTION,
        generous_budget(),
    );
    let mut arena = PageArena::new();
    assert!(matches!(
        incomplete
            .finish(&mut arena, &order())
            .expect("incomplete output is a normal unknown"),
        PresentationBuildOutcome::Unknown(PresentationUnknownRange {
            reason: PresentationUnknownReason::IncompleteAuthority {
                required: seen_required,
                certified: PresentationAuthority::INLINE_PROJECTION,
            },
            ..
        }) if seen_required == required
    ));
    assert_eq!(arena.metrics().live_nodes, 0);

    let target_request = requiring(
        request(PresentationRequestScope::Viewport, 20),
        PresentationAuthority::INTERACTION_TARGETS,
    );
    let complete_empty = PresentationFactBuilder::new(
        epoch,
        target_request,
        PresentationAuthority::INTERACTION_TARGETS,
        generous_budget(),
    );
    let PresentationBuildOutcome::Exact { lease, receipt } = complete_empty
        .finish(&mut arena, &order())
        .expect("certified empty target set")
    else {
        panic!("a certified empty dimension is exact");
    };
    assert_eq!(receipt.fact_pages_allocated, 0);
    assert_eq!(receipt.manifest_pages_allocated, 1);
    assert_eq!(
        lease
            .query_required_class(
                &arena,
                epoch,
                target_request,
                PresentationFactClass::TableTarget,
                &order(),
            )
            .expect("certified exact-empty query"),
        PresentationLookup::Exact(flark_v3_runtime_slice::ExactPresentationFacts {
            epoch,
            request: target_request,
            facts: Vec::new(),
        })
    );
    lease
        .release_later(&mut arena)
        .expect("release empty manifest");
}

#[test]
fn host_identity_survives_semantic_absence_and_layout_renewal_without_authority() {
    let host = HostLayoutLease::new(PresentationHostId(70), LayoutGeneration(1));
    let renewed = host.renew(LayoutGeneration(2));
    assert_eq!(host.host(), renewed.host());
    assert_ne!(host.layout_generation(), renewed.layout_generation());

    let arena = PageArena::new();
    let requested = request(PresentationRequestScope::Viewport, 20);
    assert_eq!(
        query_optional_presentation(None, &arena, epoch(1, 1), requested, &order())
            .expect("absence is explicit"),
        PresentationLookup::Unknown(PresentationUnknownRange {
            range: requested.range,
            reason: PresentationUnknownReason::MissingLease,
        })
    );
    assert_eq!(renewed.host(), PresentationHostId(70));
}

#[test]
fn two_dense_pages_are_leaf_complete_and_adopted_only_through_the_manifest() {
    const TOKENS: usize = PRESENTATION_FACTS_PER_PAGE;
    const FACTS: usize = TOKENS * 2;
    let epoch = epoch(31, 8);
    let authority =
        PresentationAuthority::INLINE_PROJECTION.union(PresentationAuthority::COMMAND_CAPABILITIES);
    let request = requiring(
        request(
            PresentationRequestScope::ActiveEdit,
            u32::try_from(TOKENS).expect("small token count"),
        ),
        authority,
    );
    let mut expected = Vec::with_capacity(FACTS);
    for token in 0..TOKENS {
        let start = u32::try_from(token).expect("small token index");
        let token_range = range(1, start, start + 1);
        expected.push(PresentationFact::InlineHidden {
            range: token_range,
            syntax: InlineSyntaxTag(1),
            nesting: 1,
        });
        expected.push(PresentationFact::CommandCapabilities {
            range: token_range,
            target: ForestBlockId(u64::try_from(token).expect("small token index")),
            scope: CommandScopeTag(2),
            capabilities: CommandCapabilities::OPEN,
        });
    }
    // The first token fact of the middle pair is the last record on page zero;
    // its style partner starts page one. Only the root manifest can expose the
    // pair, so no renderable leaf is published page-by-page.
    assert_eq!(
        expected[PRESENTATION_FACTS_PER_PAGE - 1].class(),
        PresentationFactClass::InlineHidden
    );
    assert_eq!(
        expected[PRESENTATION_FACTS_PER_PAGE].class(),
        PresentationFactClass::CommandCapabilities
    );

    let expected_fact_payload = FACTS * PRESENTATION_PACKED_FACT_BYTES
        + 2 * flark_v3_runtime_slice::PRESENTATION_FACT_PAGE_HEADER_BYTES;
    assert_eq!(expected_fact_payload, 2 * ARENA_PAGE_BYTES);
    let expected_total = expected_fact_payload + PRESENTATION_MANIFEST_BYTES;
    let mut builder = PresentationFactBuilder::new(
        epoch,
        request,
        authority,
        PresentationBudget::new(
            2,
            u32::try_from(FACTS).expect("small fact count"),
            u32::try_from(expected_total).expect("small retained payload"),
        ),
    );
    for fact in &expected {
        assert_eq!(builder.push(*fact), PresentationPushResult::Accepted);
    }
    let mut arena = PageArena::new();
    let PresentationBuildOutcome::Exact { lease, receipt } = builder
        .finish(&mut arena, &order())
        .expect("exactly two dense pages")
    else {
        panic!("dense pages exactly fit their declared cap");
    };
    settle_transfers(&mut arena);
    assert_eq!(receipt.fact_pages_allocated, 2);
    assert_eq!(receipt.facts_packed, FACTS);
    assert_eq!(receipt.maximum_page_payload_bytes, ARENA_PAGE_BYTES);
    assert_eq!(receipt.arena_payload_bytes, expected_total);
    assert_eq!(arena.metrics().live_payload_bytes, expected_total);
    assert_eq!(
        receipt.arena_payload_bytes * 1_000 / FACTS,
        expected_total * 1_000 / FACTS,
        "retained bytes/fact is explicit, including the manifest"
    );
    eprintln!(
        "presentation_dense facts={FACTS} fact_page_bytes={expected_fact_payload} total_payload_bytes={} retained_millibytes_per_fact={}",
        receipt.arena_payload_bytes,
        receipt.arena_payload_bytes * 1_000 / FACTS,
    );

    let PresentationLookup::Exact(exact) = lease
        .query(&arena, epoch, request, &order())
        .expect("atomic manifest query")
    else {
        panic!("matching epoch adopts the complete fact set");
    };
    assert_eq!(exact.facts, expected);
    lease.release_later(&mut arena).expect("release dense root");
    while arena.metrics().pending_releases > 0 {
        let receipt = arena.poll_reclaim(1).expect("fuelled retirement");
        assert!(receipt.reference_transitions <= 1);
        assert!(receipt.nodes_reclaimed <= 1);
        assert!(receipt.payload_bytes_reclaimed <= ARENA_PAGE_BYTES);
    }
    assert_eq!(arena.metrics().live_payload_bytes, 0);
}

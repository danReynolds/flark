#![cfg(feature = "exact-parser")]

use std::ops::Range;

use flark_comrak_inline_fragment_gate::{
    EMPTY_REFERENCE_SNAPSHOT, INLINE_FACT_FLAG_SOURCE_BACKED, InlineFactKind, InlineInputKind,
    InlineReferenceSnapshot, InlineReferenceTarget, MAX_INLINE_FRAGMENT_BYTES, OriginRunKind,
};
use flark_v3_runtime_slice::{
    BlockId, ClosedChildAggregate, CoverageId, CoveragePart, FactsEnvelope, GrammarRevision,
    GreenAffinity, GreenCoordinate, GreenEvent, GreenHeadingOpenFacts, GreenKind,
    InlineLeafMaterializationFuel, InlineLeafMaterializationJob, InlineLeafMaterializationPhase,
    InlineLeafMaterializationProgress, InlineLeafOutcome, InlineLeafUnknownReason,
    LiveDocumentStore, LogicalContribution, PageArena, ParseGeneration,
    SerializedGreenBuildReceipt, SerializedGreenDocument, SerializedGreenRootSpec,
    SourceProjectionRun,
};

#[derive(Debug)]
struct FixedReferenceSnapshot {
    identity: u64,
    generation: u64,
    target: InlineReferenceTarget,
}

impl InlineReferenceSnapshot for FixedReferenceSnapshot {
    fn identity(&self) -> u64 {
        self.identity
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn resolve(&self, normalized: &str, _original: &str) -> InlineReferenceTarget {
        assert_eq!(normalized, "ref");
        self.target.clone()
    }
}

fn enter(block: u64, kind: GreenKind, facts: FactsEnvelope) -> GreenEvent {
    GreenEvent::enter(BlockId(block), kind, facts)
}

fn exit() -> GreenEvent {
    GreenEvent::exit(ClosedChildAggregate::default())
}

fn physical(id: u64, bytes: u64, utf16: u64, part: CoveragePart) -> GreenEvent {
    GreenEvent::Coverage(SourceProjectionRun::new(CoverageId(id), bytes, utf16, 0, part).unwrap())
}

fn identity(id: u64, bytes: u64, utf16: u64, target: u64, part: CoveragePart) -> GreenEvent {
    GreenEvent::Coverage(
        SourceProjectionRun::with_logical(
            CoverageId(id),
            bytes,
            utf16,
            0,
            part,
            BlockId(target),
            LogicalContribution::Identity,
        )
        .unwrap(),
    )
}

fn build_atx(
    arena: &mut PageArena,
    live: &LiveDocumentStore,
    content: Range<usize>,
    content_utf16: u64,
    marker_tail: Range<usize>,
) -> SerializedGreenDocument {
    let source = live.query_source();
    let source_bytes = source.len_bytes();
    let source_utf16 = source.len_utf16();
    let mut events = vec![
        enter(1, GreenKind::DOCUMENT, FactsEnvelope::empty()),
        enter(
            2,
            GreenKind::HEADING,
            GreenHeadingOpenFacts::atx(1).unwrap().into_envelope(),
        ),
        physical(
            1,
            u64::try_from(content.start).unwrap(),
            u64::try_from(content.start).unwrap(),
            CoveragePart::BLOCK_MARKER,
        ),
        identity(
            2,
            u64::try_from(content.len()).unwrap(),
            content_utf16,
            2,
            CoveragePart::CONTENT,
        ),
    ];
    if !marker_tail.is_empty() {
        events.push(physical(
            3,
            u64::try_from(marker_tail.len()).unwrap(),
            u64::try_from(marker_tail.len()).unwrap(),
            CoveragePart::BLOCK_MARKER,
        ));
    }
    let terminal_start = marker_tail.end.max(content.end);
    if terminal_start < source_bytes {
        events.push(physical(
            4,
            u64::try_from(source_bytes - terminal_start).unwrap(),
            u64::try_from(source_utf16)
                .unwrap()
                .checked_sub(
                    u64::try_from(content.start).unwrap()
                        + content_utf16
                        + u64::try_from(marker_tail.len()).unwrap(),
                )
                .unwrap(),
            CoveragePart::TERMINAL,
        ));
    }
    events.extend([exit(), exit()]);
    SerializedGreenDocument::build(
        arena,
        SerializedGreenRootSpec {
            syntax_profile: 1,
            source_revision: source.revision(),
            source_root: source.identity(),
            source_bytes: u64::try_from(source_bytes).unwrap(),
            source_utf16: u64::try_from(source_utf16).unwrap(),
            grammar_revision: GrammarRevision(7),
            parse_generation: ParseGeneration(11),
            semantic_epoch: 13,
            known_bytes: 0..u64::try_from(source_bytes).unwrap(),
        },
        events,
        &mut SerializedGreenBuildReceipt::default(),
    )
    .unwrap()
}

fn heading_target(
    document: &SerializedGreenDocument,
    arena: &PageArena,
    content_start: u64,
) -> flark_v3_runtime_slice::GreenEnterCapability {
    document
        .seek(
            arena,
            GreenCoordinate::Bytes,
            content_start,
            GreenAffinity::Downstream,
        )
        .unwrap()
        .open_path()
        .last()
        .unwrap()
        .enter
}

fn drive_parser_worker_job(
    document: &SerializedGreenDocument,
    arena: &PageArena,
    live: &LiveDocumentStore,
    target: flark_v3_runtime_slice::GreenEnterCapability,
    references: &dyn InlineReferenceSnapshot,
) -> InlineLeafOutcome {
    let mut job = InlineLeafMaterializationJob::new_on_parser_worker(
        document,
        arena,
        live.query_source(),
        target,
    )
    .unwrap();
    let fuel = InlineLeafMaterializationFuel::new(usize::MAX).unwrap();
    loop {
        match job
            .poll(document, arena, live.query_source(), references, fuel)
            .unwrap()
        {
            InlineLeafMaterializationProgress::Pending { .. } => {}
            InlineLeafMaterializationProgress::Complete(outcome) => return outcome,
        }
    }
}

#[test]
fn packed_unicode_atx_materializes_through_real_comrak_and_exact_origin_map() {
    let source = "# **β😀** ###\r\n";
    let live = LiveDocumentStore::new(source, 8).unwrap();
    let mut arena = PageArena::new();
    let document = build_atx(&mut arena, &live, 2..12, 7, 12..16);
    let target = heading_target(&document, &arena, 2);

    let outcome =
        drive_parser_worker_job(&document, &arena, &live, target, &EMPTY_REFERENCE_SNAPSHOT);
    let InlineLeafOutcome::Ready(ready) = outcome else {
        panic!("source-backed packed heading must produce exact inline presentation");
    };

    assert_eq!(ready.logical(), "**β😀**");
    assert_eq!(ready.logical().len(), 10);
    assert_eq!(ready.logical().encode_utf16().count(), 7);
    assert_eq!(
        ready.input_kind(),
        InlineInputKind::Heading {
            level: 1,
            setext: false,
        }
    );
    assert_eq!(ready.binding().manifest(), document.manifest_id());
    assert_eq!(ready.binding().target(), target);
    assert_eq!(ready.binding().grammar(), GrammarRevision(7));
    assert_eq!(ready.binding().generation(), ParseGeneration(11));
    assert_eq!(ready.binding().semantic_epoch(), 13);
    assert_eq!(ready.origin_map().logical_len, 10);
    assert_eq!(ready.origin_map().runs.len(), 1);
    assert_eq!(ready.origin_map().runs[0].logical, 0..10);
    assert_eq!(ready.origin_map().runs[0].physical, 2..12);
    assert_eq!(ready.origin_map().runs[0].kind, OriginRunKind::Identity);

    let strong = ready
        .fragment()
        .facts
        .iter()
        .find(|fact| fact.kind == InlineFactKind::Strong as u8)
        .expect("real Comrak emits Strong");
    assert_eq!(strong.logical_start, 0);
    assert_eq!(strong.logical_len, 10);
    let text = ready
        .fragment()
        .facts
        .iter()
        .find(|fact| fact.kind == InlineFactKind::Text as u8)
        .expect("real Comrak emits source-backed Text");
    assert_eq!(text.flags, INLINE_FACT_FLAG_SOURCE_BACKED);
    assert_eq!(text.logical_start, 2);
    assert_eq!(text.logical_len, 6);

    let composed_strong = ready
        .composed()
        .semantic_facts
        .iter()
        .find(|mapped| mapped.fact.kind == InlineFactKind::Strong as u8)
        .unwrap();
    assert_eq!(composed_strong.physical_parts, vec![2..12]);
    let composed_text = ready
        .composed()
        .semantic_facts
        .iter()
        .find(|mapped| mapped.fact.kind == InlineFactKind::Text as u8)
        .unwrap();
    assert_eq!(composed_text.physical_parts, vec![4..10]);
    assert_eq!(ready.receipt().source_bytes_copied, 10);
    assert_eq!(ready.receipt().inline_service_calls, 1);
}

#[test]
fn packed_over_cap_atx_is_unknown_before_source_copy_or_comrak() {
    let logical_bytes = MAX_INLINE_FRAGMENT_BYTES + 1;
    let source = format!("# {}\n", "a".repeat(logical_bytes));
    let live = LiveDocumentStore::new(&source, 8).unwrap();
    let mut arena = PageArena::new();
    let document = build_atx(
        &mut arena,
        &live,
        2..2 + logical_bytes,
        u64::try_from(logical_bytes).unwrap(),
        2 + logical_bytes..2 + logical_bytes,
    );
    let target = heading_target(&document, &arena, 2);
    assert_eq!(target.kind, GreenKind::HEADING, "structure completes first");

    let outcome =
        drive_parser_worker_job(&document, &arena, &live, target, &EMPTY_REFERENCE_SNAPSHOT);
    let InlineLeafOutcome::Unknown(unknown) = outcome else {
        panic!("over-cap inline input must source-paint as explicit Unknown");
    };
    assert_eq!(
        unknown.reason(),
        &InlineLeafUnknownReason::OverInputCap {
            observed_logical_end: u64::try_from(logical_bytes).unwrap(),
            cap: MAX_INLINE_FRAGMENT_BYTES,
        }
    );
    assert_eq!(unknown.binding().target(), target);
    assert_eq!(unknown.receipt().logical_segments_visited, 1);
    assert_eq!(unknown.receipt().source_ranges_read, 0);
    assert_eq!(unknown.receipt().source_bytes_copied, 0);
    assert_eq!(unknown.receipt().inline_service_calls, 0);
}

#[test]
fn phased_job_revalidates_each_reference_dependency_field_before_ready() {
    let source = "# [x][ref]\n";
    let live = LiveDocumentStore::new(source, 8).unwrap();
    let mut arena = PageArena::new();
    let document = build_atx(&mut arena, &live, 2..10, 8, 10..10);
    let target = heading_target(&document, &arena, 2);
    let parsed = FixedReferenceSnapshot {
        identity: 1,
        generation: 7,
        target: InlineReferenceTarget {
            symbol_id: 41,
            presence_generation: 5,
            defined: true,
        },
    };
    let fuel = InlineLeafMaterializationFuel::new(64).unwrap();

    for actual in [
        InlineReferenceTarget {
            symbol_id: 42,
            presence_generation: 5,
            defined: true,
        },
        InlineReferenceTarget {
            symbol_id: 41,
            presence_generation: 6,
            defined: true,
        },
        InlineReferenceTarget {
            symbol_id: 41,
            presence_generation: 5,
            defined: false,
        },
    ] {
        let mut job = InlineLeafMaterializationJob::new_on_parser_worker(
            &document,
            &arena,
            live.query_source(),
            target,
        )
        .unwrap();
        assert!(matches!(
            job.poll(&document, &arena, live.query_source(), &parsed, fuel,)
                .unwrap(),
            InlineLeafMaterializationProgress::Pending {
                phase: InlineLeafMaterializationPhase::InlineService,
                ..
            }
        ));
        assert!(matches!(
            job.poll(&document, &arena, live.query_source(), &parsed, fuel,)
                .unwrap(),
            InlineLeafMaterializationProgress::Pending {
                phase: InlineLeafMaterializationPhase::ReferenceValidation,
                ..
            }
        ));
        assert_eq!(job.supersession_checks(), 4);

        let current = FixedReferenceSnapshot {
            identity: 2,
            generation: 8,
            target: actual.clone(),
        };
        let InlineLeafMaterializationProgress::Complete(InlineLeafOutcome::Unknown(unknown)) = job
            .poll(&document, &arena, live.query_source(), &current, fuel)
            .unwrap()
        else {
            panic!("a changed reference dependency must fail closed before Ready");
        };
        assert_eq!(
            unknown.reason(),
            &InlineLeafUnknownReason::StaleReferenceDependency {
                normalized_label: "ref".to_owned(),
                expected_symbol_id: 41,
                actual_symbol_id: actual.symbol_id,
                expected_presence_generation: 5,
                actual_presence_generation: actual.presence_generation,
                expected_resolved: true,
                actual_resolved: actual.defined,
            }
        );
    }
}

#[test]
fn phased_job_allows_snapshot_metadata_change_when_dependency_state_is_current() {
    let source = "# [x][ref]\n";
    let live = LiveDocumentStore::new(source, 8).unwrap();
    let mut arena = PageArena::new();
    let document = build_atx(&mut arena, &live, 2..10, 8, 10..10);
    let target = heading_target(&document, &arena, 2);
    let parsed = FixedReferenceSnapshot {
        identity: 1,
        generation: 7,
        target: InlineReferenceTarget {
            symbol_id: 41,
            presence_generation: 5,
            defined: true,
        },
    };
    let current = FixedReferenceSnapshot {
        identity: 99,
        generation: 100,
        target: parsed.target.clone(),
    };
    let fuel = InlineLeafMaterializationFuel::new(64).unwrap();
    let mut job = InlineLeafMaterializationJob::new_on_parser_worker(
        &document,
        &arena,
        live.query_source(),
        target,
    )
    .unwrap();
    assert!(matches!(
        job.poll(&document, &arena, live.query_source(), &parsed, fuel,)
            .unwrap(),
        InlineLeafMaterializationProgress::Pending { .. }
    ));
    assert!(matches!(
        job.poll(&document, &arena, live.query_source(), &parsed, fuel,)
            .unwrap(),
        InlineLeafMaterializationProgress::Pending { .. }
    ));
    assert!(matches!(
        job.poll(&document, &arena, live.query_source(), &current, fuel,)
            .unwrap(),
        InlineLeafMaterializationProgress::Pending {
            phase: InlineLeafMaterializationPhase::OriginComposition,
            ..
        }
    ));
    assert!(matches!(
        job.poll(&document, &arena, live.query_source(), &current, fuel,)
            .unwrap(),
        InlineLeafMaterializationProgress::Complete(InlineLeafOutcome::Ready(_))
    ));
}

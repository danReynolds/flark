use flark_gate_a_harness::{
    apply_order_operation, cold_giant_line_histories, dense_line_source, gate_a_histories,
    giant_construct_locality_cases, giant_line_cases, global_reclassification_cases, hash64,
    randomized_order_operations, same_boundary_insertions, validate_fact_probes,
    validate_local_delta, validate_non_poll_phase, validate_poll, validate_resources,
    validate_snapshot, CoverageChunk, Edit, FactKind, FactProbe, PhaseReceipt, PollReceipt,
    PollStatus, ProtectedRange, ResourceCaps, ResourceMetrics, RevisionDelta, Snapshot, StableId,
    SyntaxFact, SyntaxProfile, WorkFuel, MAX_COVERAGE_CHUNK_BYTES,
};

#[test]
fn gfm_table_code_pipe_fixture_requires_a_source_escape() {
    let history = gate_a_histories()
        .into_iter()
        .find(|history| history.name == "table-every-revision")
        .expect("table history exists");
    let final_source = &history.revisions[history.revisions.len() / 2].source;

    assert!(final_source.contains("`b\\|c`"));
    assert!(!final_source.contains("`b|c`"));
}

fn valid_snapshot(source: &str) -> Snapshot {
    Snapshot {
        profile: SyntaxProfile::FlarkGfm,
        revision: 0,
        source_len: source.len(),
        source_hash64: hash64(source.as_bytes()),
        normalized_html: flark_gate_a_harness::oracle_html(source).unwrap(),
        coverage: if source.is_empty() {
            Vec::new()
        } else {
            vec![CoverageChunk {
                id: StableId(1),
                source: 0..source.len(),
                kind: "source",
            }]
        },
        facts: Vec::new(),
        resources: ResourceMetrics::default(),
    }
}

#[test]
fn poll_contract_rejects_the_old_whole_line_budget_escape_hatch() {
    let fuel = WorkFuel::new(64, 64);
    let receipt = PollReceipt {
        status: PollStatus::Ready,
        source_bytes_examined: 10 * 1024 * 1024,
        transitions: 1,
        cancellation_checks: 1,
        max_uninterrupted_transitions: 1,
        peak_scratch_bytes: 10 * 1024 * 1024,
    };
    let error = validate_poll(receipt, fuel).unwrap_err();
    assert!(error.contains("examined 10485760 bytes"), "{error}");
}

#[test]
fn phase_contract_rejects_hidden_grammar_work_before_poll() {
    let receipt = PhaseReceipt {
        source_bytes_examined: 10 * 1024 * 1024,
        grammar_transitions: 1_000,
        structural_steps: 1,
        newly_allocated_bytes: 10 * 1024 * 1024,
        batch_tree_materializations: 1,
        grammar_side_scans: 0,
    };
    let error = validate_non_poll_phase(receipt, "begin_edit").unwrap_err();
    assert!(error.contains("begin_edit examined"), "{error}");
}

#[test]
fn coverage_contract_rejects_gaps_and_duplicate_ids() {
    let source = "alpha\nbeta\n";
    let mut snapshot = valid_snapshot(source);
    snapshot.coverage = vec![
        CoverageChunk {
            id: StableId(1),
            source: 0..6,
            kind: "source",
        },
        CoverageChunk {
            id: StableId(1),
            source: 7..source.len(),
            kind: "source",
        },
    ];
    let error = validate_snapshot(source, &snapshot).unwrap_err();
    assert!(
        error.contains("duplicate coverage ID") || error.contains("coverage gap"),
        "{error}"
    );
}

#[test]
fn coverage_contract_rejects_unbounded_whole_document_chunks() {
    let source = "a".repeat(MAX_COVERAGE_CHUNK_BYTES + 1);
    let snapshot = valid_snapshot(&source);
    let error = validate_snapshot(&source, &snapshot).unwrap_err();
    assert!(error.contains("coverage chunk"), "{error}");
}

#[test]
fn memory_contract_rejects_the_measured_dense_line_prototype_shapes() {
    let mut snapshot = valid_snapshot("a\n");
    snapshot.resources.checkpoint_bytes = 209_387_520;
    let error = validate_resources(&snapshot, None, ResourceCaps::GATE_A).unwrap_err();
    assert!(error.contains("persistent auxiliary state"), "{error}");
}

#[test]
fn expensive_fixture_generators_have_the_required_scale() {
    let giant = giant_line_cases(10 * 1024 * 1024);
    assert_eq!(giant.len(), 3);
    for case in giant {
        assert!(case.source.len() >= 10 * 1024 * 1024, "{}", case.name);
        assert_eq!(case.fuel.max_source_bytes, 4096);
        assert_eq!(
            case.edit.apply(&case.source).unwrap().len(),
            case.source.len()
        );
    }
    let cold = cold_giant_line_histories(10 * 1024 * 1024);
    assert_eq!(cold.len(), 3);
    assert!(cold.iter().all(|history| history.require_pending));
    assert!(cold
        .iter()
        .all(|history| history.revisions[1].source.len() >= 10 * 1024 * 1024));
    let locality = giant_construct_locality_cases();
    assert_eq!(locality.len(), 3);
    for case in locality {
        assert!(case.source.len() >= 900_000, "{}", case.name);
        assert_eq!(case.protected.len(), 2);
        assert_eq!(
            case.edit.apply(&case.source).unwrap().len(),
            case.source.len()
        );
    }
    let global = global_reclassification_cases();
    assert_eq!(global.len(), 1);
    assert!(global[0].source.len() >= 1_000_000);
    assert_eq!(global[0].protected.len(), 2);
    let edited = global[0].edit.apply(&global[0].source).unwrap();
    assert!(edited.contains("<!--\nordinary paragraph 00000"));
    assert_eq!(dense_line_source(1_000_000).len(), 2_000_000);
}

#[test]
fn stable_order_generators_reach_the_full_gate_without_unbounded_source_growth() {
    let same_boundary = same_boundary_insertions(10_000);
    assert_eq!(same_boundary.len(), 10_000);
    assert!(same_boundary.iter().all(|operation| matches!(
        operation,
        flark_gate_a_harness::OrderOperation::Insert { slot: 0, .. }
    )));

    let operations = randomized_order_operations(100_000, 256);
    assert_eq!(operations.len(), 100_000);
    let mut items = Vec::new();
    let mut max_items = 0;
    for operation in &operations {
        let before = flark_gate_a_harness::order_source(&items);
        let edit = apply_order_operation(&mut items, operation);
        assert_eq!(
            edit.apply(&before).unwrap(),
            flark_gate_a_harness::order_source(&items)
        );
        max_items = max_items.max(items.len());
    }
    assert!(max_items <= 256);
}

#[test]
fn source_fact_probe_checks_exact_range_and_markers() {
    let source = "title\n=====\n";
    let mut snapshot = valid_snapshot(source);
    snapshot.facts.push(SyntaxFact {
        id: StableId(2),
        parent: None,
        kind: FactKind::Heading {
            level: 1,
            setext: true,
        },
        source: 0..11,
        markers: std::iter::once(6..11).collect(),
    });
    let probe = FactProbe {
        kind: FactKind::Heading {
            level: 1,
            setext: true,
        },
        source_text: "title\n=====".into(),
        marker_texts: vec!["=====".into()],
    };
    validate_fact_probes(source, &snapshot, std::slice::from_ref(&probe)).unwrap();
    snapshot.facts[0].markers.clear();
    assert!(validate_fact_probes(source, &snapshot, &[probe]).is_err());
}

#[test]
fn local_delta_contract_preserves_shifted_suffix_identity() {
    let before = Snapshot {
        profile: SyntaxProfile::FlarkGfm,
        revision: 4,
        source_len: 12,
        source_hash64: 0,
        normalized_html: String::new(),
        coverage: vec![
            CoverageChunk {
                id: StableId(1),
                source: 0..4,
                kind: "prefix",
            },
            CoverageChunk {
                id: StableId(2),
                source: 4..8,
                kind: "edited",
            },
            CoverageChunk {
                id: StableId(3),
                source: 8..12,
                kind: "suffix",
            },
        ],
        facts: Vec::new(),
        resources: ResourceMetrics::default(),
    };
    let edit = Edit {
        base_revision: 4,
        start_utf8: 4,
        end_utf8: 8,
        replacement: "longer".into(),
    };
    let inserted = CoverageChunk {
        id: StableId(4),
        source: 4..10,
        kind: "edited",
    };
    let mut after = Snapshot {
        profile: SyntaxProfile::FlarkGfm,
        revision: 5,
        source_len: 14,
        source_hash64: 0,
        normalized_html: String::new(),
        coverage: vec![
            before.coverage[0].clone(),
            inserted.clone(),
            CoverageChunk {
                id: StableId(3),
                source: 10..14,
                kind: "suffix",
            },
        ],
        facts: Vec::new(),
        resources: ResourceMetrics::default(),
    };
    let delta = RevisionDelta {
        base_revision: 4,
        revision: 5,
        removed_coverage_ids: vec![StableId(2)],
        inserted_coverage: vec![inserted],
        removed_fact_ids: Vec::new(),
        upserted_facts: Vec::new(),
        encoded_bytes: 128,
        batch_tree_materializations: 0,
        grammar_side_scans: 0,
    };
    let protected = [ProtectedRange { old: 0..4 }, ProtectedRange { old: 8..12 }];
    validate_local_delta(&before, &after, &edit, &delta, &protected).unwrap();

    after.coverage[2].id = StableId(5);
    let error = validate_local_delta(&before, &after, &edit, &delta, &protected).unwrap_err();
    assert!(
        error.contains("protected coverage ID") || error.contains("coverage removals differ"),
        "{error}"
    );
}

#[test]
fn local_delta_rejects_a_mutated_whole_document_chunk() {
    let before = Snapshot {
        profile: SyntaxProfile::FlarkGfm,
        revision: 1,
        source_len: 8,
        source_hash64: 0,
        normalized_html: String::new(),
        coverage: vec![CoverageChunk {
            id: StableId(1),
            source: 0..8,
            kind: "whole-document",
        }],
        facts: Vec::new(),
        resources: ResourceMetrics::default(),
    };
    let edit = Edit {
        base_revision: 1,
        start_utf8: 4,
        end_utf8: 4,
        replacement: "x".into(),
    };
    let mut after = before.clone();
    after.revision = 2;
    after.source_len = 9;
    after.coverage[0].source = 0..9;
    let delta = RevisionDelta {
        base_revision: 1,
        revision: 2,
        removed_coverage_ids: Vec::new(),
        inserted_coverage: Vec::new(),
        removed_fact_ids: Vec::new(),
        upserted_facts: Vec::new(),
        encoded_bytes: 0,
        batch_tree_materializations: 0,
        grammar_side_scans: 0,
    };
    let error = validate_local_delta(&before, &after, &edit, &delta, &[]).unwrap_err();
    assert!(error.contains("intersects the edit"), "{error}");
}

#[test]
fn clean_equivalence_includes_semantic_parentage() {
    let source = "> item\n";
    let mut incremental = valid_snapshot(source);
    incremental.facts = vec![
        SyntaxFact {
            id: StableId(10),
            parent: None,
            kind: FactKind::BlockQuote,
            source: 0..source.len() - 1,
            markers: std::iter::once(0..1).collect(),
        },
        SyntaxFact {
            id: StableId(11),
            parent: Some(StableId(10)),
            kind: FactKind::Paragraph,
            source: 0..source.len() - 1,
            markers: Vec::new(),
        },
    ];
    let mut clean = incremental.clone();
    clean.facts[1].parent = None;
    let error = flark_gate_a_harness::validate_clean_equivalence(&incremental, &clean).unwrap_err();
    assert!(error.contains("fact mismatch"), "{error}");
}

#[test]
fn snapshot_rejects_cyclic_or_non_containing_parentage() {
    let source = "> item\n";
    let mut snapshot = valid_snapshot(source);
    snapshot.facts = vec![
        SyntaxFact {
            id: StableId(10),
            parent: Some(StableId(11)),
            kind: FactKind::BlockQuote,
            source: 0..source.len() - 1,
            markers: std::iter::once(0..1).collect(),
        },
        SyntaxFact {
            id: StableId(11),
            parent: Some(StableId(10)),
            kind: FactKind::Paragraph,
            source: 0..source.len() - 1,
            markers: Vec::new(),
        },
    ];
    let error = validate_snapshot(source, &snapshot).unwrap_err();
    assert!(error.contains("cyclic ancestry"), "{error}");

    snapshot.facts[0].parent = None;
    snapshot.facts[0].source = 0..1;
    snapshot.facts[1].source = 2..source.len() - 1;
    let error = validate_snapshot(source, &snapshot).unwrap_err();
    assert!(error.contains("escapes ancestor"), "{error}");
}

#[test]
fn protected_fact_identity_cannot_churn() {
    let mut before = valid_snapshot("prefix\nedit\nsuffix\n");
    before.revision = 3;
    before.coverage = vec![
        CoverageChunk {
            id: StableId(1),
            source: 0..7,
            kind: "prefix",
        },
        CoverageChunk {
            id: StableId(2),
            source: 7..12,
            kind: "edit",
        },
        CoverageChunk {
            id: StableId(3),
            source: 12..19,
            kind: "suffix",
        },
    ];
    before.facts = vec![SyntaxFact {
        id: StableId(20),
        parent: None,
        kind: FactKind::Paragraph,
        source: 0..6,
        markers: Vec::new(),
    }];
    let edit = Edit {
        base_revision: 3,
        start_utf8: 7,
        end_utf8: 12,
        replacement: "EDIT!".into(),
    };
    let mut after = before.clone();
    after.revision = 4;
    after.coverage[1].id = StableId(4);
    after.facts[0].id = StableId(21);
    let delta = RevisionDelta {
        base_revision: 3,
        revision: 4,
        removed_coverage_ids: vec![StableId(2)],
        inserted_coverage: vec![after.coverage[1].clone()],
        removed_fact_ids: vec![StableId(20)],
        upserted_facts: after.facts.clone(),
        encoded_bytes: 128,
        batch_tree_materializations: 0,
        grammar_side_scans: 0,
    };
    let error = validate_local_delta(
        &before,
        &after,
        &edit,
        &delta,
        &[ProtectedRange { old: 0..7 }],
    )
    .unwrap_err();
    assert!(error.contains("protected fact ID"), "{error}");
}

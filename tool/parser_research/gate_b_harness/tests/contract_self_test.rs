use flark_gate_b_harness::{
    alternating_special_source, delimiter_run_source, gate_b_histories, giant_inline_cases, hash64,
    plain_softbreak_source, reference_fanout_source, reference_multileaf_fanout_source,
    token_dense_leaf, validate_clean_equivalence, validate_delta_receipt, validate_flark_profile,
    validate_phase_evidence, validate_plain_run_coalescing, validate_poll_evidence,
    validate_presence_delta, validate_protected_identity, validate_replayable_delta,
    validate_resources, validate_snapshot, AuditEvent, AutolinkForm, AutolinkKind,
    DependencyIndexRefresh, DependencyIndexState, DisplayPolicy, Edit, InlineFact, InlineKind,
    InlineLeaf, LabelDependency, LeafOutput, LinkForm, MapPiece, OrderSplice,
    OutputSequenceRefresh, PendingToken, PhaseEvidence, PhaseReceipt, PollEvidence, PollReceipt,
    PollStatus, ProtectedRange, ReferenceDefinition, ReferenceUse, ResourceCaps, ResourceMetrics,
    RevisionDelta, SegmentedText, Snapshot, StableId, SyntaxProfile, TransitionKind, VirtualReason,
    WorkFuel, POLL_FUEL_BYTES,
};

#[test]
fn gfm_table_code_pipe_fixture_requires_a_source_escape() {
    let history = gate_b_histories()
        .into_iter()
        .find(|history| history.name == "escaped-and-code-table-pipes-every-revision")
        .expect("table history exists");
    let final_source = &history.revisions[history.revisions.len() / 2].source;

    assert!(final_source.contains("`c\\|d`"));
    assert!(!final_source.contains("`c|d`"));
}

fn mapped(source: &str, range: std::ops::Range<usize>) -> SegmentedText {
    SegmentedText {
        text: source[range.clone()].to_owned(),
        pieces: vec![MapPiece::Source(range)],
    }
}

fn valid_snapshot(source: &str) -> Snapshot {
    if source.is_empty() {
        return Snapshot {
            profile: SyntaxProfile::FlarkGfm,
            revision: 0,
            source_len: 0,
            source_hash64: hash64(&[]),
            test_html: String::new(),
            leaves: Vec::new(),
            facts: Vec::new(),
            definitions: Vec::new(),
            reference_uses: Vec::new(),
            leaf_outputs: Vec::new(),
            output_sequence_root: StableId(100),
            label_dependencies: Vec::new(),
            dependency_indexes: Vec::new(),
            resources: ResourceMetrics::default(),
        };
    }
    Snapshot {
        profile: SyntaxProfile::FlarkGfm,
        revision: 0,
        source_len: source.len(),
        source_hash64: hash64(source.as_bytes()),
        test_html: source.to_owned(),
        leaves: vec![InlineLeaf {
            id: StableId(1),
            source: 0..source.len(),
            input: mapped(source, 0..source.len()),
        }],
        facts: vec![InlineFact {
            id: StableId(2),
            leaf: StableId(1),
            parent: None,
            kind: InlineKind::Text,
            source: 0..source.len(),
            markers: Vec::new(),
            content: mapped(source, 0..source.len()),
        }],
        definitions: Vec::new(),
        reference_uses: Vec::new(),
        leaf_outputs: vec![LeafOutput {
            leaf: StableId(1),
            root: StableId(100),
            fact_count: 1,
            reference_use_count: 0,
        }],
        output_sequence_root: StableId(101),
        label_dependencies: Vec::new(),
        dependency_indexes: Vec::new(),
        resources: ResourceMetrics::default(),
    }
}

struct PresenceShape {
    revision: u64,
    defined: bool,
    symbol: StableId,
    generation: u64,
    output_sequence_root: StableId,
    leaf_root_base: u128,
    fact_base: u128,
}

fn presence_snapshot(source: &str, shape: PresenceShape) -> Snapshot {
    let starts = source
        .match_indices("[label]")
        .take(2)
        .map(|(start, _)| start)
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 2);
    let leaves = starts
        .iter()
        .enumerate()
        .map(|(index, start)| InlineLeaf {
            id: StableId(1 + index as u128),
            source: *start..*start + 7,
            input: mapped(source, *start..*start + 7),
        })
        .collect::<Vec<_>>();
    let facts = starts
        .iter()
        .enumerate()
        .map(|(index, start)| InlineFact {
            id: StableId(shape.fact_base + index as u128),
            leaf: leaves[index].id,
            parent: None,
            kind: if shape.defined {
                InlineKind::Link {
                    form: LinkForm::ShortcutReference,
                }
            } else {
                InlineKind::Text
            },
            source: *start..*start + 7,
            markers: if shape.defined {
                vec![*start..*start + 1, *start + 6..*start + 7]
            } else {
                Vec::new()
            },
            content: if shape.defined {
                mapped(source, *start + 1..*start + 6)
            } else {
                mapped(source, *start..*start + 7)
            },
        })
        .collect::<Vec<_>>();
    let definition_start = source.rfind("[label]: /winner\n");
    let definitions = definition_start
        .map(|start| ReferenceDefinition {
            symbol: shape.symbol,
            normalized_label: "label".to_owned(),
            destination: "/winner".to_owned(),
            title: None,
            source: start..source.len(),
            markers: std::iter::once(start..start + 1).collect(),
        })
        .into_iter()
        .collect::<Vec<_>>();
    let reference_uses = if shape.defined {
        starts
            .iter()
            .enumerate()
            .map(|(index, start)| ReferenceUse {
                fact: facts[index].id,
                symbol: shape.symbol,
                label_source: *start + 1..*start + 6,
            })
            .collect()
    } else {
        Vec::new()
    };
    let label_dependencies = leaves
        .iter()
        .map(|leaf| LabelDependency {
            leaf: leaf.id,
            normalized_label: "label".to_owned(),
            occurrences: 1,
            resolved_symbol: shape.defined.then_some(shape.symbol),
        })
        .collect();
    Snapshot {
        profile: SyntaxProfile::FlarkGfm,
        revision: shape.revision,
        source_len: source.len(),
        source_hash64: hash64(source.as_bytes()),
        test_html: source.to_owned(),
        leaves,
        facts,
        definitions,
        reference_uses,
        leaf_outputs: (0..2)
            .map(|index| LeafOutput {
                leaf: StableId(1 + index),
                root: StableId(shape.leaf_root_base + index),
                fact_count: 1,
                reference_use_count: usize::from(shape.defined),
            })
            .collect(),
        output_sequence_root: shape.output_sequence_root,
        label_dependencies,
        dependency_indexes: vec![DependencyIndexState {
            normalized_label: "label".to_owned(),
            generation: shape.generation,
            dependent_leaf_count: 2,
            occurrences: 2,
            resolved_symbol: shape.defined.then_some(shape.symbol),
        }],
        resources: ResourceMetrics::default(),
    }
}

fn valid_poll(token: PendingToken) -> PollEvidence {
    PollEvidence {
        receipt: PollReceipt {
            token,
            status: PollStatus::Pending,
            source_bytes_examined: 64,
            transitions: 5,
            cancellation_checks: 2,
            max_uninterrupted_transitions: 3,
            newly_allocated_bytes: 32,
            peak_scratch_bytes: 32,
            general_batch_trees: 0,
            grammar_side_scans: 0,
        },
        events: vec![
            AuditEvent::CancellationCheck,
            AuditEvent::SourceSpanExamined(0..64),
            AuditEvent::GrammarTransitions {
                kind: TransitionKind::Scan,
                count: 3,
            },
            AuditEvent::CancellationCheck,
            AuditEvent::GrammarTransitions {
                kind: TransitionKind::DelimiterResolution,
                count: 2,
            },
            AuditEvent::Allocation(32),
        ],
    }
}

#[test]
fn ordered_trace_accounts_for_scan_and_resolution_fuel() {
    let token = PendingToken {
        revision: 1,
        generation: 7,
    };
    validate_poll_evidence(&valid_poll(token), WorkFuel::GATE_B, token).unwrap();

    let mut dishonest = valid_poll(token);
    dishonest.events.push(AuditEvent::GrammarTransitions {
        kind: TransitionKind::Finalize,
        count: 10,
    });
    assert!(validate_poll_evidence(&dishonest, WorkFuel::GATE_B, token)
        .unwrap_err()
        .contains("audit trace"));
}

#[test]
fn ready_poll_must_charge_finalize_and_fact_sealing() {
    let token = PendingToken {
        revision: 1,
        generation: 7,
    };
    let mut evidence = valid_poll(token);
    evidence.receipt.status = PollStatus::Ready;
    assert!(validate_poll_evidence(&evidence, WorkFuel::GATE_B, token)
        .unwrap_err()
        .contains("finalization"));

    evidence.receipt.transitions += 1;
    evidence.receipt.max_uninterrupted_transitions = 3;
    evidence.events.push(AuditEvent::GrammarTransitions {
        kind: TransitionKind::Finalize,
        count: 1,
    });
    validate_poll_evidence(&evidence, WorkFuel::GATE_B, token).unwrap();
}

#[test]
fn poll_rejects_unfuelled_giant_scan_and_stale_generation() {
    let token = PendingToken {
        revision: 1,
        generation: 1,
    };
    let mut evidence = valid_poll(token);
    evidence.receipt.source_bytes_examined = 10 * 1024 * 1024;
    evidence.events[1] = AuditEvent::SourceSpanExamined(0..10 * 1024 * 1024);
    assert!(validate_poll_evidence(&evidence, WorkFuel::GATE_B, token)
        .unwrap_err()
        .contains("exceeded"));

    let other = PendingToken {
        revision: 1,
        generation: 2,
    };
    assert!(
        validate_poll_evidence(&valid_poll(token), WorkFuel::GATE_B, other)
            .unwrap_err()
            .contains("token")
    );
}

#[test]
fn begin_and_commit_reject_hidden_finalize_or_batch_work() {
    let evidence = PhaseEvidence {
        receipt: PhaseReceipt {
            source_bytes_examined: 0,
            grammar_transitions: 1,
            structural_steps: 1,
            newly_allocated_bytes: 0,
            general_batch_trees: 0,
            grammar_side_scans: 0,
        },
        events: vec![AuditEvent::GrammarTransitions {
            kind: TransitionKind::Finalize,
            count: 1,
        }],
    };
    assert!(validate_phase_evidence(&evidence, "commit")
        .unwrap_err()
        .contains("outside poll"));

    let batch = PhaseEvidence {
        receipt: PhaseReceipt {
            general_batch_trees: 1,
            ..PhaseReceipt::default()
        },
        events: vec![AuditEvent::GeneralBatchTreeMaterialization],
    };
    assert!(validate_phase_evidence(&batch, "begin_edit")
        .unwrap_err()
        .contains("batch tree"));
}

#[test]
fn segmented_mapping_rejects_backward_virtual_anchors_and_fake_empty_markers() {
    let source = "alpha\n> beta";
    let mut snapshot = Snapshot {
        profile: SyntaxProfile::FlarkGfm,
        revision: 0,
        source_len: source.len(),
        source_hash64: hash64(source.as_bytes()),
        test_html: String::new(),
        leaves: vec![InlineLeaf {
            id: StableId(1),
            source: 0..source.len(),
            input: SegmentedText {
                text: "alpha\nbeta".to_owned(),
                pieces: vec![
                    MapPiece::Source(0..5),
                    MapPiece::Virtual {
                        anchor_utf8: 5,
                        text: "\n".to_owned(),
                        reason: VirtualReason::ContainerLineJoin,
                    },
                    MapPiece::Source(8..12),
                ],
            },
        }],
        facts: vec![InlineFact {
            id: StableId(2),
            leaf: StableId(1),
            parent: None,
            kind: InlineKind::Strong,
            source: 0..source.len(),
            markers: std::iter::once(0..1).collect(),
            content: mapped(source, 0..5),
        }],
        definitions: Vec::new(),
        reference_uses: Vec::new(),
        leaf_outputs: vec![LeafOutput {
            leaf: StableId(1),
            root: StableId(100),
            fact_count: 1,
            reference_use_count: 0,
        }],
        output_sequence_root: StableId(101),
        label_dependencies: Vec::new(),
        dependency_indexes: Vec::new(),
        resources: ResourceMetrics::default(),
    };
    validate_snapshot(source, &snapshot).unwrap();
    if let MapPiece::Virtual { anchor_utf8, .. } = &mut snapshot.leaves[0].input.pieces[1] {
        *anchor_utf8 = 3;
    }
    assert!(validate_snapshot(source, &snapshot)
        .unwrap_err()
        .contains("backwards"));

    if let MapPiece::Virtual { anchor_utf8, .. } = &mut snapshot.leaves[0].input.pieces[1] {
        *anchor_utf8 = 5;
    }
    snapshot.facts[0].markers = std::iter::once(5..5).collect();
    assert!(validate_snapshot(source, &snapshot).is_err());
}

#[test]
fn source_policy_rejects_bare_autolink_markers_and_executable_raw_html() {
    let source = "www.example.com";
    let mut snapshot = valid_snapshot(source);
    snapshot.facts[0].kind = InlineKind::Autolink {
        kind: AutolinkKind::Uri,
        form: AutolinkForm::Bare,
    };
    snapshot.facts[0].markers = std::iter::once(0..3).collect();
    assert!(validate_snapshot(source, &snapshot)
        .unwrap_err()
        .contains("bare autolink"));

    let source = "<script>";
    let mut snapshot = valid_snapshot(source);
    snapshot.facts[0].kind = InlineKind::RawHtml {
        display: DisplayPolicy::Rendered,
    };
    assert!(validate_snapshot(source, &snapshot)
        .unwrap_err()
        .contains("source-visible"));
}

#[test]
fn flark_profile_rejects_footnote_reference_table_entries() {
    let source = "[^1]\n\n[^1]: note\n";
    let mut snapshot = valid_snapshot(source);
    snapshot.definitions.push(ReferenceDefinition {
        symbol: StableId(3),
        normalized_label: "^1".to_owned(),
        destination: "note".to_owned(),
        title: None,
        source: 6..16,
        markers: std::iter::once(6..7).collect(),
    });
    snapshot.reference_uses.push(ReferenceUse {
        fact: StableId(2),
        symbol: StableId(3),
        label_source: 1..3,
    });
    assert!(validate_flark_profile(source, &snapshot).is_err());
    assert!(validate_snapshot(source, &snapshot).is_err());
}

#[test]
fn memory_contract_counts_committed_pending_tape_and_resolution_stacks() {
    let mut metrics = ResourceMetrics::default();
    metrics.source_backing_bytes = 20 * 1024 * 1024;
    metrics.lexical_tape_bytes = 160 * 1024 * 1024;
    metrics.resolution_stack_bytes = 8 * 1024 * 1024;
    metrics.total_peak_live_bytes = metrics.accounted_peak_live_bytes();
    assert!(validate_resources(&metrics, ResourceCaps::TOKEN_DENSE)
        .unwrap_err()
        .contains("total peak live"));
    metrics.total_peak_live_bytes = 1;
    assert!(validate_resources(&metrics, ResourceCaps::TOKEN_DENSE)
        .unwrap_err()
        .contains("categorized accounting"));

    let mut tape_cheat = ResourceMetrics::default();
    tape_cheat.source_backing_bytes = 20 * 1024 * 1024;
    tape_cheat.lexical_tape_bytes = 25 * 1024 * 1024;
    tape_cheat.total_peak_live_bytes = tape_cheat.accounted_peak_live_bytes();
    assert!(validate_resources(&tape_cheat, ResourceCaps::TOKEN_DENSE)
        .unwrap_err()
        .contains("lexical tape"));
}

#[test]
fn plain_run_contract_rejects_record_per_byte_or_softbreak() {
    let source = plain_softbreak_source(100_000);
    let mut snapshot = valid_snapshot(&source);
    let leaf = snapshot.leaves[0].id;
    snapshot.facts = (0..source.len())
        .map(|index| InlineFact {
            id: StableId(10 + index as u128),
            leaf,
            parent: None,
            kind: InlineKind::Text,
            source: index..index + 1,
            markers: Vec::new(),
            content: mapped(&source, index..index + 1),
        })
        .collect();
    assert!(validate_plain_run_coalescing(&snapshot)
        .unwrap_err()
        .contains("text/break records"));
}

#[test]
fn replay_contract_rejects_implicit_global_snapshot_replacement() {
    let before_source = "abcdef";
    let after_source = "abcXdef";
    let before = valid_snapshot(before_source);
    let mut after = valid_snapshot(after_source);
    after.revision = 1;
    let edit = Edit {
        base_revision: 0,
        start_utf8: 3,
        end_utf8: 3,
        replacement: "X".to_owned(),
    };
    let delta = RevisionDelta {
        base_revision: 0,
        revision: 1,
        removed_leaf_ids: Vec::new(),
        upserted_leaves: Vec::new(),
        leaf_order_splices: Vec::new(),
        removed_fact_ids: Vec::new(),
        upserted_facts: Vec::new(),
        fact_order_splices: Vec::new(),
        output_sequence_refresh: None,
        removed_definition_symbols: Vec::new(),
        upserted_definitions: Vec::new(),
        removed_reference_use_facts: Vec::new(),
        upserted_reference_uses: Vec::new(),
        removed_label_dependencies: Vec::new(),
        upserted_label_dependencies: Vec::new(),
        dependency_index_refreshes: Vec::new(),
        encoded_bytes: 0,
        general_batch_trees: 0,
        grammar_side_scans: 0,
    };
    assert!(validate_replayable_delta(&before, &after, &edit, &delta)
        .unwrap_err()
        .contains("intersects edit"));
}

#[test]
fn protected_locality_rejects_identity_churn_outside_the_edit() {
    let before_source = "aaaBBBccc";
    let after_source = "aaaXXXccc";
    let mut before = valid_snapshot(before_source);
    before.leaves = vec![
        InlineLeaf {
            id: StableId(1),
            source: 0..3,
            input: mapped(before_source, 0..3),
        },
        InlineLeaf {
            id: StableId(2),
            source: 6..9,
            input: mapped(before_source, 6..9),
        },
    ];
    before.facts = vec![
        InlineFact {
            id: StableId(3),
            leaf: StableId(1),
            parent: None,
            kind: InlineKind::Text,
            source: 0..3,
            markers: Vec::new(),
            content: mapped(before_source, 0..3),
        },
        InlineFact {
            id: StableId(4),
            leaf: StableId(2),
            parent: None,
            kind: InlineKind::Text,
            source: 6..9,
            markers: Vec::new(),
            content: mapped(before_source, 6..9),
        },
    ];
    before.leaf_outputs = vec![
        LeafOutput {
            leaf: StableId(1),
            root: StableId(100),
            fact_count: 1,
            reference_use_count: 0,
        },
        LeafOutput {
            leaf: StableId(2),
            root: StableId(200),
            fact_count: 1,
            reference_use_count: 0,
        },
    ];
    before.output_sequence_root = StableId(300);
    let mut after = before.clone();
    after.revision = 1;
    after.source_hash64 = hash64(after_source.as_bytes());
    after.test_html = after_source.to_owned();
    after.leaves[1].id = StableId(20);
    after.leaves[1].input = mapped(after_source, 6..9);
    after.facts[1].leaf = StableId(20);
    after.facts[1].content = mapped(after_source, 6..9);
    after.leaf_outputs[1].leaf = StableId(20);
    after.leaf_outputs[1].root = StableId(201);
    after.output_sequence_root = StableId(301);
    let edit = Edit {
        base_revision: 0,
        start_utf8: 3,
        end_utf8: 6,
        replacement: "XXX".to_owned(),
    };
    let delta = RevisionDelta {
        base_revision: 0,
        revision: 1,
        removed_leaf_ids: vec![StableId(2)],
        upserted_leaves: vec![after.leaves[1].clone()],
        leaf_order_splices: vec![OrderSplice {
            start: 1,
            removed: vec![StableId(2)],
            inserted: vec![StableId(20)],
        }],
        removed_fact_ids: Vec::new(),
        upserted_facts: Vec::new(),
        fact_order_splices: Vec::new(),
        output_sequence_refresh: Some(OutputSequenceRefresh {
            removed_root: StableId(300),
            inserted_root: StableId(301),
            affected_leaf_count: 2,
        }),
        removed_definition_symbols: Vec::new(),
        upserted_definitions: Vec::new(),
        removed_reference_use_facts: Vec::new(),
        upserted_reference_uses: Vec::new(),
        removed_label_dependencies: Vec::new(),
        upserted_label_dependencies: Vec::new(),
        dependency_index_refreshes: Vec::new(),
        encoded_bytes: 256,
        general_batch_trees: 0,
        grammar_side_scans: 0,
    };
    validate_replayable_delta(&before, &after, &edit, &delta).unwrap();
    let mut missing_order = delta.clone();
    missing_order.leaf_order_splices.clear();
    assert!(
        validate_replayable_delta(&before, &after, &edit, &missing_order)
            .unwrap_err()
            .contains("order splices")
    );
    let error = validate_protected_identity(
        &before,
        &after,
        &edit,
        &delta,
        &[ProtectedRange { old: 6..9 }],
    )
    .unwrap_err();
    assert!(error.contains("protected leaf ID"), "{error}");
}

#[test]
fn clean_equivalence_includes_segment_maps_and_semantic_parentage() {
    let source = "**a**";
    let incremental = valid_snapshot(source);
    let mut clean = incremental.clone();
    clean.facts[0].content.text = "wrong".to_owned();
    assert!(validate_clean_equivalence(&incremental, &clean).is_err());
}

#[test]
fn stress_generators_have_required_scale_and_adversarial_density() {
    let cases = giant_inline_cases();
    assert_eq!(cases.len(), 3);
    for case in &cases {
        assert!(case.source.len() >= 10 * 1024 * 1024, "{}", case.name);
        assert_eq!(case.first_edit.base_revision, 0);
        assert!(case.require_pending);
        assert_eq!(
            case.first_edit.apply(&case.source).unwrap().len(),
            case.source.len()
        );
    }
    let dense = token_dense_leaf(10 * 1024 * 1024);
    assert!(dense.contains("[c](https://e.test)"));
    let delimiters = delimiter_run_source(10 * 1024 * 1024);
    assert!(delimiters.starts_with("********"));
    let alternating = alternating_special_source(10 * 1024 * 1024);
    assert!(alternating.contains("*_[`~<\\&]"));
    assert_eq!(WorkFuel::GATE_B.max_source_bytes, POLL_FUEL_BYTES);
}

#[test]
fn reference_fanout_generator_has_thousands_of_consumers_and_one_symbol() {
    let source = reference_fanout_source(5_000, "/old");
    assert_eq!(source.matches("[label] ").count(), 5_000);
    assert_eq!(source.matches("[label]:").count(), 1);
    assert!(source.ends_with("[label]: /old\n"));

    let multileaf = reference_multileaf_fanout_source(5_000, Some("/winner"));
    assert_eq!(multileaf.matches("[label]\n\n").count(), 5_000);
    assert_eq!(multileaf.matches("[label]:").count(), 1);
    assert!(multileaf.ends_with("[label]: /winner\n"));
}

#[test]
fn multileaf_presence_refresh_retains_misses_and_replays_compactly() {
    let defined_source = reference_multileaf_fanout_source(2, Some("/winner"));
    let undefined_source = reference_multileaf_fanout_source(2, None);
    assert_eq!(defined_source.matches("[label]\n\n").count(), 2);
    assert!(defined_source.ends_with("[label]: /winner\n"));

    let defined = presence_snapshot(
        &defined_source,
        PresenceShape {
            revision: 0,
            defined: true,
            symbol: StableId(50),
            generation: 1,
            output_sequence_root: StableId(200),
            leaf_root_base: 100,
            fact_base: 10,
        },
    );
    let undefined = presence_snapshot(
        &undefined_source,
        PresenceShape {
            revision: 1,
            defined: false,
            symbol: StableId(50),
            generation: 2,
            output_sequence_root: StableId(201),
            leaf_root_base: 110,
            fact_base: 20,
        },
    );
    validate_snapshot(&defined_source, &defined).unwrap();
    validate_snapshot(&undefined_source, &undefined).unwrap();
    assert_eq!(undefined.label_dependencies.len(), 2);
    assert!(undefined
        .label_dependencies
        .iter()
        .all(|dependency| dependency.resolved_symbol.is_none()));

    let removal = Edit {
        base_revision: 0,
        start_utf8: undefined_source.len(),
        end_utf8: defined_source.len(),
        replacement: String::new(),
    };
    let removal_delta = RevisionDelta {
        base_revision: 0,
        revision: 1,
        removed_leaf_ids: Vec::new(),
        upserted_leaves: Vec::new(),
        leaf_order_splices: Vec::new(),
        removed_fact_ids: Vec::new(),
        upserted_facts: Vec::new(),
        fact_order_splices: Vec::new(),
        output_sequence_refresh: Some(OutputSequenceRefresh {
            removed_root: StableId(200),
            inserted_root: StableId(201),
            affected_leaf_count: 2,
        }),
        removed_definition_symbols: vec![StableId(50)],
        upserted_definitions: Vec::new(),
        removed_reference_use_facts: Vec::new(),
        upserted_reference_uses: Vec::new(),
        removed_label_dependencies: Vec::new(),
        upserted_label_dependencies: Vec::new(),
        dependency_index_refreshes: vec![DependencyIndexRefresh {
            normalized_label: "label".to_owned(),
            removed_generation: Some(1),
            inserted_generation: Some(2),
        }],
        encoded_bytes: 256,
        general_batch_trees: 0,
        grammar_side_scans: 0,
    };
    validate_replayable_delta(&defined, &undefined, &removal, &removal_delta).unwrap();
    validate_presence_delta(&defined, &undefined, &removal, &removal_delta, 2).unwrap();

    let mut missing_root = removal_delta.clone();
    missing_root.output_sequence_refresh = None;
    assert!(
        validate_replayable_delta(&defined, &undefined, &removal, &missing_root)
            .unwrap_err()
            .contains("output-sequence refresh")
    );
    let mut enumerated_misses = removal_delta.clone();
    enumerated_misses
        .upserted_label_dependencies
        .push(undefined.label_dependencies[0].clone());
    assert!(
        validate_replayable_delta(&defined, &undefined, &removal, &enumerated_misses)
            .unwrap_err()
            .contains("enumerated per-leaf")
    );
    let mut enumerated_leaves = removal_delta.clone();
    enumerated_leaves.upserted_leaves = undefined.leaves.clone();
    assert!(
        validate_presence_delta(&defined, &undefined, &removal, &enumerated_leaves, 2)
            .unwrap_err()
            .contains("enumerated thousands")
    );

    let redefined = presence_snapshot(
        &defined_source,
        PresenceShape {
            revision: 2,
            defined: true,
            symbol: StableId(60),
            generation: 3,
            output_sequence_root: StableId(202),
            leaf_root_base: 120,
            fact_base: 30,
        },
    );
    let addition = Edit {
        base_revision: 1,
        start_utf8: undefined_source.len(),
        end_utf8: undefined_source.len(),
        replacement: "[label]: /winner\n".to_owned(),
    };
    let addition_delta = RevisionDelta {
        base_revision: 1,
        revision: 2,
        removed_leaf_ids: Vec::new(),
        upserted_leaves: Vec::new(),
        leaf_order_splices: Vec::new(),
        removed_fact_ids: Vec::new(),
        upserted_facts: Vec::new(),
        fact_order_splices: Vec::new(),
        output_sequence_refresh: Some(OutputSequenceRefresh {
            removed_root: StableId(201),
            inserted_root: StableId(202),
            affected_leaf_count: 2,
        }),
        removed_definition_symbols: Vec::new(),
        upserted_definitions: redefined.definitions.clone(),
        removed_reference_use_facts: Vec::new(),
        upserted_reference_uses: Vec::new(),
        removed_label_dependencies: Vec::new(),
        upserted_label_dependencies: Vec::new(),
        dependency_index_refreshes: vec![DependencyIndexRefresh {
            normalized_label: "label".to_owned(),
            removed_generation: Some(2),
            inserted_generation: Some(3),
        }],
        encoded_bytes: 256,
        general_batch_trees: 0,
        grammar_side_scans: 0,
    };
    validate_snapshot(&defined_source, &redefined).unwrap();
    validate_replayable_delta(&undefined, &redefined, &addition, &addition_delta).unwrap();
    validate_presence_delta(&undefined, &redefined, &addition, &addition_delta, 2).unwrap();
}

#[test]
fn delta_receipt_rejects_side_scans_and_large_global_payloads() {
    let delta = RevisionDelta {
        base_revision: 0,
        revision: 1,
        removed_leaf_ids: Vec::new(),
        upserted_leaves: Vec::new(),
        leaf_order_splices: Vec::new(),
        removed_fact_ids: Vec::new(),
        upserted_facts: Vec::new(),
        fact_order_splices: Vec::new(),
        output_sequence_refresh: None,
        removed_definition_symbols: Vec::new(),
        upserted_definitions: Vec::new(),
        removed_reference_use_facts: Vec::new(),
        upserted_reference_uses: Vec::new(),
        removed_label_dependencies: Vec::new(),
        upserted_label_dependencies: Vec::new(),
        dependency_index_refreshes: Vec::new(),
        encoded_bytes: 1_000_000,
        general_batch_trees: 0,
        grammar_side_scans: 1,
    };
    assert!(validate_delta_receipt(&delta, 64 * 1024).is_err());
}

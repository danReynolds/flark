use std::sync::Arc;

use flark_integrated_parser_slice::convergence::{
    BackwardMapStatus, BoundaryAffinity, ChangeHistory, CheckpointIdentity, ConvergenceJob,
    ConvergenceRejection, ConvergenceStatus, DependencyGeneration, DependencySnapshot,
    ExactCheckpoint, HistoryAdvance, LogicalInputSnapshot, MapFailure, OutputSuffixRoot,
    ParserStateSnapshot, RevisionId, MAX_CONVERGENCE_POLL_WORK, MAX_RECORDED_CHANGES,
    REMAINING_INTEGRATION_GAP,
};
use flark_integrated_parser_slice::frontier::SegmentedLeafBuilder;
use flark_integrated_parser_slice::source::{PersistentSource, MAX_PIECE_BYTES};

fn dependencies(entries: &[(u64, u64)]) -> DependencySnapshot {
    let entries: Vec<_> = entries
        .iter()
        .map(|&(dependency, generation)| DependencyGeneration {
            dependency,
            generation,
        })
        .collect();
    DependencySnapshot::from_sorted(&entries).unwrap()
}

fn checkpoint_identity(
    parser_atoms: &[u64],
    logical_input: LogicalInputSnapshot,
    dependency_entries: &[(u64, u64)],
) -> CheckpointIdentity {
    CheckpointIdentity {
        parser_state: ParserStateSnapshot::from_atoms(parser_atoms),
        logical_input,
        dependencies: dependencies(dependency_entries),
    }
}

fn finish(job: &mut ConvergenceJob<'_>, fuel: usize) -> ConvergenceStatus {
    for _ in 0..100_000 {
        let poll = job.poll(fuel);
        if !matches!(poll.status, ConvergenceStatus::Pending) {
            return poll.status;
        }
    }
    panic!("convergence job did not terminate");
}

#[test]
fn same_length_interior_replacement_is_changed_not_equal_by_shape() {
    let mut history = ChangeHistory::new(RevisionId(1), PersistentSource::from_text("abcdefghij"));
    let logical = LogicalInputSnapshot::proof_root(4);
    let output = OutputSuffixRoot::fresh();
    let candidate = ExactCheckpoint::capture(
        RevisionId(1),
        &history.latest_source(),
        3,
        checkpoint_identity(&[10, 20], logical.clone(), &[(7, 9)]),
    )
    .unwrap()
    .with_output_suffix(output);
    history.apply_edit(RevisionId(2), 2..6, "WXYZ").unwrap();
    let current = ExactCheckpoint::capture(
        RevisionId(2),
        &history.latest_source(),
        3,
        checkpoint_identity(&[10, 20], logical, &[(7, 9)]),
    )
    .unwrap();

    let mut job =
        ConvergenceJob::new(&history, candidate, current, BoundaryAffinity::Suffix).unwrap();
    assert_eq!(
        finish(&mut job, 1),
        ConvergenceStatus::Rejected(ConvergenceRejection::Map(
            MapFailure::BoundaryInsideChangedBytes {
                to_revision: RevisionId(2),
                boundary: 3,
                inserted_new: 2..6,
            }
        ))
    );
}

#[test]
fn prefix_insertion_maps_shifted_stable_suffix_and_converges() {
    let text = format!("{}suffix", "a".repeat(5_000));
    let mut history = ChangeHistory::new(RevisionId(10), PersistentSource::from_text(&text));
    let logical = LogicalInputSnapshot::proof_root(12);
    let output = OutputSuffixRoot::fresh();
    let candidate = ExactCheckpoint::capture(
        RevisionId(10),
        &history.latest_source(),
        4_500,
        checkpoint_identity(&[1, 3, 5, 8], logical.clone(), &[(11, 2), (19, 7)]),
    )
    .unwrap()
    .with_output_suffix(output.clone());
    history.apply_edit(RevisionId(20), 0..0, ">>>").unwrap();
    let current = ExactCheckpoint::capture(
        RevisionId(20),
        &history.latest_source(),
        4_503,
        checkpoint_identity(&[1, 3, 5, 8], logical, &[(11, 2), (19, 7)]),
    )
    .unwrap();
    assert_eq!(candidate.witness(), current.witness());

    let mut job =
        ConvergenceJob::new(&history, candidate, current, BoundaryAffinity::Suffix).unwrap();
    let ConvergenceStatus::Converged(proof) = finish(&mut job, 2) else {
        panic!("exact unchanged suffix should converge")
    };
    assert_eq!(proof.candidate_boundary, 4_500);
    assert_eq!(proof.current_boundary, 4_503);
    assert_eq!(proof.composed_change_steps, 1);
    assert_eq!(proof.adopted_output_suffix, output);
}

#[test]
fn source_compaction_uses_exact_copy_lineage_instead_of_false_negative() {
    let mut history =
        ChangeHistory::new(RevisionId(1), PersistentSource::from_text("small suffix"));
    let logical = LogicalInputSnapshot::proof_root(1);
    let output = OutputSuffixRoot::fresh();
    let candidate = ExactCheckpoint::capture(
        RevisionId(1),
        &history.latest_source(),
        6,
        checkpoint_identity(&[4], logical.clone(), &[]),
    )
    .unwrap()
    .with_output_suffix(output);
    history.apply_edit(RevisionId(2), 1..1, "x").unwrap();
    let current = ExactCheckpoint::capture(
        RevisionId(2),
        &history.latest_source(),
        7,
        checkpoint_identity(&[4], logical, &[]),
    )
    .unwrap();
    assert_ne!(candidate.witness(), current.witness());

    let mut map_job = history
        .backward_map(RevisionId(1), 7, BoundaryAffinity::Suffix)
        .unwrap();
    let BackwardMapStatus::Mapped(mapped) = map_job.poll(1).status else {
        panic!("compacted unchanged byte should still map")
    };
    assert_eq!(mapped.old_boundary, 6);
    assert_eq!(mapped.copied_next_byte_steps, 1);

    let mut job =
        ConvergenceJob::new(&history, candidate, current, BoundaryAffinity::Suffix).unwrap();
    assert!(matches!(
        finish(&mut job, 64),
        ConvergenceStatus::Converged(_)
    ));
}

#[test]
fn two_queued_edits_compose_without_materializing_a_forward_map() {
    let text = format!("{}stable suffix", "a".repeat(12_000));
    let mut history = ChangeHistory::new(RevisionId(100), PersistentSource::from_text(&text));
    let logical = LogicalInputSnapshot::proof_root(31);
    let output = OutputSuffixRoot::fresh();
    let candidate = ExactCheckpoint::capture(
        RevisionId(100),
        &history.latest_source(),
        10_000,
        checkpoint_identity(&[1, 2, 3], logical.clone(), &[(3, 5), (9, 11)]),
    )
    .unwrap()
    .with_output_suffix(output.clone());
    history.apply_edit(RevisionId(400), 0..0, ">>").unwrap();
    history
        .apply_edit(RevisionId(900), 5_000..5_004, "WXYZ")
        .unwrap();
    let current = ExactCheckpoint::capture(
        RevisionId(900),
        &history.latest_source(),
        10_002,
        checkpoint_identity(&[1, 2, 3], logical, &[(3, 5), (9, 11)]),
    )
    .unwrap();
    assert_eq!(candidate.witness(), current.witness());

    let mut job =
        ConvergenceJob::new(&history, candidate, current, BoundaryAffinity::Suffix).unwrap();
    let ConvergenceStatus::Converged(proof) = finish(&mut job, 1) else {
        panic!("two exact change records should compose")
    };
    assert_eq!(proof.composed_change_steps, 2);
    assert_eq!(proof.adopted_output_suffix, output);
    assert_eq!(job.audit().change_steps, 2);
}

#[test]
fn eof_edit_maps_exactly_but_changed_parser_state_prevents_reuse() {
    let mut history = ChangeHistory::new(RevisionId(1), PersistentSource::from_text("hello"));
    let logical = LogicalInputSnapshot::proof_root(1);
    let output = OutputSuffixRoot::fresh();
    let candidate = ExactCheckpoint::capture(
        RevisionId(1),
        &history.latest_source(),
        5,
        checkpoint_identity(&[1, 0], logical.clone(), &[]),
    )
    .unwrap()
    .with_output_suffix(output);
    history.apply_edit(RevisionId(2), 5..5, "!").unwrap();
    let current = ExactCheckpoint::capture(
        RevisionId(2),
        &history.latest_source(),
        6,
        checkpoint_identity(&[2, 0], logical, &[]),
    )
    .unwrap();
    assert_eq!(candidate.witness(), current.witness());

    let mut map_job = history
        .backward_map(RevisionId(1), 6, BoundaryAffinity::Suffix)
        .unwrap();
    let BackwardMapStatus::Mapped(mapped) = map_job.poll(1).status else {
        panic!("EOF should map to the old EOF")
    };
    assert_eq!(mapped.old_boundary, 5);

    let mut job =
        ConvergenceJob::new(&history, candidate, current, BoundaryAffinity::Suffix).unwrap();
    assert_eq!(
        finish(&mut job, 64),
        ConvergenceStatus::Rejected(ConvergenceRejection::ParserStateMismatch { atom: 0 })
    );
}

#[test]
fn empty_deletion_boundary_respects_explicit_affinity() {
    let mut history = ChangeHistory::new(RevisionId(1), PersistentSource::from_text("hello!"));
    history.apply_edit(RevisionId(2), 5..6, "").unwrap();
    let mut suffix = history
        .backward_map(RevisionId(1), 5, BoundaryAffinity::Suffix)
        .unwrap();
    let mut prefix = history
        .backward_map(RevisionId(1), 5, BoundaryAffinity::Prefix)
        .unwrap();
    let BackwardMapStatus::Mapped(suffix) = suffix.poll(1).status else {
        panic!("suffix-affine deletion boundary should map")
    };
    let BackwardMapStatus::Mapped(prefix) = prefix.poll(1).status else {
        panic!("prefix-affine deletion boundary should map")
    };
    assert_eq!(suffix.old_boundary, 6);
    assert_eq!(prefix.old_boundary, 5);
}

#[test]
fn candidate_output_is_adopted_only_after_common_checkpoint_proof() {
    let history = ChangeHistory::new(RevisionId(7), PersistentSource::from_text("same"));
    let source = history.latest_source();
    let logical_one = LogicalInputSnapshot::proof_root(9);
    let logical_two = LogicalInputSnapshot::proof_root(9);
    let output = OutputSuffixRoot::fresh();

    let candidate = ExactCheckpoint::capture(
        RevisionId(7),
        &source,
        0,
        checkpoint_identity(&[], logical_one.clone(), &[]),
    )
    .unwrap()
    .with_output_suffix(output.clone());
    let current = ExactCheckpoint::capture(
        RevisionId(7),
        &source,
        0,
        checkpoint_identity(&[], logical_one, &[]),
    )
    .unwrap();
    let mut adoption_job =
        ConvergenceJob::new(&history, candidate, current, BoundaryAffinity::Suffix).unwrap();
    let ConvergenceStatus::Converged(proof) = finish(&mut adoption_job, 64) else {
        panic!("matching current state should authorize old output adoption")
    };
    assert_eq!(proof.adopted_output_suffix, output);

    let candidate = ExactCheckpoint::capture(
        RevisionId(7),
        &source,
        0,
        checkpoint_identity(&[], logical_two, &[]),
    )
    .unwrap()
    .with_output_suffix(OutputSuffixRoot::fresh());
    let current = ExactCheckpoint::capture(
        RevisionId(7),
        &source,
        0,
        checkpoint_identity(&[], LogicalInputSnapshot::proof_root(9), &[]),
    )
    .unwrap();
    let mut logical_job =
        ConvergenceJob::new(&history, candidate, current, BoundaryAffinity::Suffix).unwrap();
    assert_eq!(
        finish(&mut logical_job, 64),
        ConvergenceStatus::Rejected(ConvergenceRejection::LogicalInputRootMismatch)
    );
}

#[test]
fn edit_later_in_suffix_invalidates_logical_suffix_root_even_at_equal_length() {
    let mut history = ChangeHistory::new(
        RevisionId(1),
        PersistentSource::from_text(&"a".repeat(8_000)),
    );
    let logical = LogicalInputSnapshot::proof_root(8);
    let old_output = OutputSuffixRoot::fresh();
    let candidate = ExactCheckpoint::capture(
        RevisionId(1),
        &history.latest_source(),
        4_500,
        checkpoint_identity(&[0], logical, &[(4, 1)]),
    )
    .unwrap()
    .with_output_suffix(old_output);
    history
        .apply_edit(RevisionId(2), 6_000..6_004, "WXYZ")
        .unwrap();
    let current = ExactCheckpoint::capture(
        RevisionId(2),
        &history.latest_source(),
        4_500,
        checkpoint_identity(&[0], LogicalInputSnapshot::proof_root(8), &[(4, 1)]),
    )
    .unwrap();
    assert_ne!(candidate.witness(), current.witness());
    assert_eq!(8_000, history.latest_source().len_bytes());

    let mut map_job = history
        .backward_map(RevisionId(1), 4_500, BoundaryAffinity::Suffix)
        .unwrap();
    let BackwardMapStatus::Mapped(mapped) = map_job.poll(1).status else {
        panic!("unchanged checkpoint should map")
    };
    assert_eq!(mapped.copied_next_byte_steps, 1);

    let mut job =
        ConvergenceJob::new(&history, candidate, current, BoundaryAffinity::Suffix).unwrap();
    assert_eq!(
        finish(&mut job, 64),
        ConvergenceStatus::Rejected(ConvergenceRejection::LogicalInputRootMismatch)
    );
}

#[test]
fn dependency_generation_change_rejects_otherwise_identical_checkpoint() {
    let history = ChangeHistory::new(RevisionId(1), PersistentSource::from_text("reference"));
    let logical = LogicalInputSnapshot::proof_root(1);
    let output = OutputSuffixRoot::fresh();
    let candidate = ExactCheckpoint::capture(
        RevisionId(1),
        &history.latest_source(),
        0,
        checkpoint_identity(&[1], logical.clone(), &[(42, 7)]),
    )
    .unwrap()
    .with_output_suffix(output);
    let current = ExactCheckpoint::capture(
        RevisionId(1),
        &history.latest_source(),
        0,
        checkpoint_identity(&[1], logical, &[(42, 8)]),
    )
    .unwrap();
    let mut job =
        ConvergenceJob::new(&history, candidate, current, BoundaryAffinity::Suffix).unwrap();
    assert_eq!(
        finish(&mut job, 64),
        ConvergenceStatus::Rejected(ConvergenceRejection::DependencyGenerationMismatch {
            entry: 0,
        })
    );
}

#[test]
fn parser_and_dependency_comparison_is_fuelled_and_audited() {
    let text = "a".repeat(8_000);
    let mut current_history = ChangeHistory::new(RevisionId(0), PersistentSource::from_text(&text));
    let parser_atoms: Vec<_> = (0..1_025_u64).collect();
    let dependency_entries: Vec<_> = (0..257_u64).map(|value| (value, value * 3)).collect();
    let logical = LogicalInputSnapshot::proof_root(22);
    let output = OutputSuffixRoot::fresh();
    let candidate = ExactCheckpoint::capture(
        RevisionId(0),
        &current_history.latest_source(),
        6_000,
        checkpoint_identity(&parser_atoms, logical.clone(), &dependency_entries),
    )
    .unwrap()
    .with_output_suffix(output.clone());
    let mut current_boundary = 6_000;
    for revision in 1..=5 {
        current_history
            .apply_edit(RevisionId(revision), 0..0, "x")
            .unwrap();
        current_boundary += 1;
    }
    let current = ExactCheckpoint::capture(
        RevisionId(5),
        &current_history.latest_source(),
        current_boundary,
        checkpoint_identity(&parser_atoms, logical, &dependency_entries),
    )
    .unwrap();
    let mut job = ConvergenceJob::new(
        &current_history,
        candidate,
        current,
        BoundaryAffinity::Suffix,
    )
    .unwrap();
    assert!(matches!(
        finish(&mut job, 7),
        ConvergenceStatus::Converged(_)
    ));
    let audit = job.audit();
    assert_eq!(audit.change_steps, 5);
    assert_eq!(audit.fixed_checks, 9);
    assert_eq!(audit.parser_state_atoms, 1_025);
    assert_eq!(audit.dependency_entries, 257);
    assert_eq!(
        audit.total_work,
        audit.change_steps
            + audit.fixed_checks
            + audit.parser_state_atoms
            + audit.dependency_entries
    );
    assert!(audit.max_poll_work <= 7);
    assert!(audit.max_poll_work <= MAX_CONVERGENCE_POLL_WORK);
}

#[test]
fn hundred_thousand_edits_retain_one_source_and_overflow_fails_closed() {
    let mut history = ChangeHistory::new(RevisionId(0), PersistentSource::default());
    let initial_source = history.latest_source();
    let initial_weak = Arc::downgrade(&initial_source);
    let logical = LogicalInputSnapshot::proof_root(1);
    let output = OutputSuffixRoot::fresh();
    let candidate = ExactCheckpoint::capture(
        RevisionId(0),
        &initial_source,
        0,
        checkpoint_identity(&[0], logical.clone(), &[]),
    )
    .unwrap()
    .with_output_suffix(output);
    drop(initial_source);

    let mut observed_resets = 0;
    for revision in 1..=100_000_u64 {
        let end = history.latest_source().len_bytes();
        let advance = history
            .apply_edit(RevisionId(revision), end..end, "x")
            .unwrap();
        if matches!(advance, HistoryAdvance::ContinuityReset) {
            observed_resets += 1;
        }
    }

    assert!(initial_weak.upgrade().is_none());
    let retention = history.retention();
    assert_eq!(retention.retained_source_roots, 1);
    assert!(retention.recorded_changes <= MAX_RECORDED_CHANGES);
    assert!(retention.record_capacity >= MAX_RECORDED_CHANGES);
    assert!(retention.record_storage_bytes <= 128 * 1_024);
    assert_eq!(retention.continuity_resets, observed_resets);
    assert!(observed_resets > 0);
    assert!(
        retention.latest_source_buffers.unique_buffers
            <= 100_000_usize.div_ceil(MAX_PIECE_BYTES) + 2
    );

    let current = ExactCheckpoint::capture(
        RevisionId(100_000),
        &history.latest_source(),
        100_000,
        checkpoint_identity(&[0], logical, &[]),
    )
    .unwrap();
    let mut job =
        ConvergenceJob::new(&history, candidate, current, BoundaryAffinity::Suffix).unwrap();
    assert_eq!(
        finish(&mut job, 17),
        ConvergenceStatus::Rejected(ConvergenceRejection::Map(MapFailure::RevisionNotInHistory(
            RevisionId(0)
        )))
    );
    assert!(job.audit().change_steps <= MAX_RECORDED_CHANGES);
}

#[test]
fn real_frontier_identity_survives_leaf_and_arc_clones() {
    let source = Arc::new(PersistentSource::from_text("frontier"));
    let mut builder = SegmentedLeafBuilder::new(source.clone());
    builder.push_source(0..source.len_bytes()).unwrap();
    let leaf = Arc::new(builder.finish());
    let logical = LogicalInputSnapshot::from_frontier(3, leaf.clone());
    let cloned_logical = LogicalInputSnapshot::from_frontier(3, Arc::new((*leaf).clone()));
    let history = ChangeHistory::new(RevisionId(1), (*source).clone());
    let output = OutputSuffixRoot::fresh();
    let candidate = ExactCheckpoint::capture(
        RevisionId(1),
        &history.latest_source(),
        0,
        checkpoint_identity(&[1], logical.clone(), &[]),
    )
    .unwrap()
    .with_output_suffix(output);
    let current = ExactCheckpoint::capture(
        RevisionId(1),
        &history.latest_source(),
        0,
        checkpoint_identity(&[1], cloned_logical, &[]),
    )
    .unwrap();
    let mut job =
        ConvergenceJob::new(&history, candidate, current, BoundaryAffinity::Suffix).unwrap();
    assert!(matches!(
        finish(&mut job, 64),
        ConvergenceStatus::Converged(_)
    ));
    assert!(REMAINING_INTEGRATION_GAP.contains("splice-persistent"));
}

#[test]
fn dependency_order_must_be_canonical() {
    let error = DependencySnapshot::from_sorted(&[
        DependencyGeneration {
            dependency: 7,
            generation: 1,
        },
        DependencyGeneration {
            dependency: 7,
            generation: 2,
        },
    ])
    .unwrap_err();
    assert_eq!(error.previous, 7);
    assert_eq!(error.next, 7);
}

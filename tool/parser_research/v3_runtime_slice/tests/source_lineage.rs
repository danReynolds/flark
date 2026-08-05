use flark_v3_runtime_slice::{
    BoundaryAffinity, LineageError, MapStatus, SOURCE_CURSOR_COPY_CAP_BYTES, SourceRevision,
    SourceRootId, SourceStore, SourceStoreError,
};
use std::time::Instant;

#[test]
fn crop_edits_and_cursor_match_unicode_crlf_oracle() {
    let mut oracle = String::from("a\r\n😀z\rq\n");
    let mut store = SourceStore::new(&oracle, 16);

    let edits = [(0..0, "β"), (5..9, "é"), (8..9, "\r\n"), (2..3, "")];
    for (range, replacement) in edits {
        let revision = store.revision();
        oracle.replace_range(range.clone(), replacement);
        store
            .apply_edit(revision, range, replacement)
            .expect("valid scalar-aligned edit");

        let root = store.query_snapshot();
        assert_eq!(root.materialize_for_testing(), oracle);
        let root_id = root.identity();
        let root_len = root.len_bytes();
        assert_eq!(root.len_utf16(), oracle.encode_utf16().count());
        let mut cursor = root.cursor();
        let mut streamed = Vec::new();
        while let Some(source_byte) = cursor.next_byte() {
            assert_eq!(source_byte.root, root_id);
            assert_eq!(source_byte.offset, streamed.len());
            streamed.push(source_byte.byte);
        }
        assert_eq!(streamed, oracle.as_bytes());
        assert!(cursor.metrics().maximum_chunk_bytes <= root_len);
    }
}

#[test]
fn edit_provenance_maps_only_operation_proven_unchanged_regions() {
    let root = flark_v3_runtime_slice::CropSnapshotLease::from_text("abc😀def");
    assert_eq!(root.len_utf16(), "abc😀def".encode_utf16().count());
    let old_id = root.identity();
    let (next, provenance) = root.edit(3..7, "XY").expect("valid emoji range");

    assert_eq!(provenance.map_unchanged(old_id, 0..3), Some(0..3));
    assert_eq!(provenance.map_unchanged(old_id, 7..10), Some(5..8));
    assert_eq!(provenance.map_unchanged(old_id, 2..8), None);
    assert_eq!(provenance.map_unchanged(old_id, 3..3), None);
    assert_eq!(
        provenance.map_unchanged(SourceRootId(old_id.0 + 99), 0..3),
        None
    );
    let descriptor = root.descriptor(7..10).expect("valid descriptor");
    let mapped = provenance
        .map_descriptor(descriptor)
        .expect("suffix is operation-proven unchanged");
    assert_eq!(mapped.root, next.identity());
    assert_eq!(mapped.start..mapped.end, 5..8);
}

#[test]
fn deterministic_edit_history_never_claims_a_changed_byte_sequence() {
    let initial = String::from("aβ\r\n😀-tail");
    let mut store = SourceStore::new(&initial, 64);
    let mut oracle = initial.clone();
    let mut next_tag = u64::try_from(initial.len()).expect("small fixture");
    let mut tags = (0..next_tag).collect::<Vec<_>>();
    let mut histories = vec![(initial, tags.clone())];
    let replacements = ["", "x", "β", "\r\n", "😀"];
    let mut random = 0xD1CE_BA5E_F00D_u64;

    for revision in 0..32_u64 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let boundaries = oracle
            .char_indices()
            .map(|(offset, _)| offset)
            .chain(std::iter::once(oracle.len()))
            .collect::<Vec<_>>();
        let left = usize::try_from(random).expect("64-bit test host") % boundaries.len();
        random = random.rotate_left(19) ^ 0x9E37_79B9_7F4A_7C15;
        let right = usize::try_from(random).expect("64-bit test host") % boundaries.len();
        let (start_index, end_index) = if left <= right {
            (left, right)
        } else {
            (right, left)
        };
        let range = boundaries[start_index]..boundaries[end_index];
        let replacement =
            replacements[usize::try_from(random >> 32).expect("fits usize") % replacements.len()];
        let inserted_tags = (0..replacement.len())
            .map(|_| {
                let tag = next_tag;
                next_tag += 1;
                tag
            })
            .collect::<Vec<_>>();
        oracle.replace_range(range.clone(), replacement);
        tags.splice(range.clone(), inserted_tags);
        store
            .apply_edit(SourceRevision(revision), range, replacement)
            .expect("generated edit is scalar aligned");
        assert_eq!(store.query_snapshot().materialize_for_testing(), oracle);
        histories.push((oracle.clone(), tags.clone()));
    }

    for (revision, (text, historical_tags)) in histories.iter().enumerate() {
        let boundaries = text
            .char_indices()
            .map(|(offset, _)| offset)
            .chain(std::iter::once(text.len()))
            .collect::<Vec<_>>();
        for (start_position, &start) in boundaries.iter().enumerate() {
            for &end in &boundaries[start_position + 1..] {
                let needle = &historical_tags[start..end];
                let expected = tags
                    .windows(needle.len())
                    .position(|candidate| candidate == needle)
                    .map(|mapped_start| mapped_start..mapped_start + needle.len());
                let revision = SourceRevision(u64::try_from(revision).expect("small history"));
                let mut job = store
                    .map_range_from(revision, start..end)
                    .expect("full history retained");
                let actual = loop {
                    match job.poll(1) {
                        MapStatus::Pending { .. } => {}
                        terminal => break terminal,
                    }
                };
                match expected {
                    Some(range) => assert_eq!(actual, MapStatus::ProvenRange(range)),
                    None => assert!(matches!(actual, MapStatus::Changed { .. })),
                }
            }
        }
    }
}

#[test]
fn lineage_mapping_is_exact_and_fuelled_by_edit_record() {
    let mut store = SourceStore::new("abcdefghij", 16);
    store
        .apply_edit(SourceRevision(0), 1..1, "XX")
        .expect("insert before target");
    store
        .apply_edit(SourceRevision(1), 3..5, "")
        .expect("delete before target");
    store
        .apply_edit(SourceRevision(2), 2..2, "!")
        .expect("second insert before target");

    let mut job = store
        .map_range_from(SourceRevision(0), 6..10)
        .expect("history retained");
    assert_eq!(
        job.poll(1),
        MapStatus::Pending {
            processed_records: 1,
            remaining_records: 2,
        }
    );
    assert_eq!(
        job.poll(1),
        MapStatus::Pending {
            processed_records: 2,
            remaining_records: 1,
        }
    );
    assert_eq!(job.poll(1), MapStatus::ProvenRange(7..11));
    assert_eq!(
        &store.query_snapshot().materialize_for_testing()[7..11],
        "ghij"
    );
    assert_eq!(job.poll(0), MapStatus::ProvenRange(7..11));
}

#[test]
fn overlap_is_changed_and_empty_ranges_require_boundary_mapping() {
    let mut store = SourceStore::new("abcdefgh", 8);
    store
        .apply_edit(SourceRevision(0), 3..5, "XYZ")
        .expect("valid replacement");

    let mut overlap = store
        .map_range_from(SourceRevision(0), 2..6)
        .expect("history retained");
    assert_eq!(
        overlap.poll(1),
        MapStatus::Changed {
            at_revision: SourceRevision(1),
        }
    );
    assert!(matches!(
        store.map_range_from(SourceRevision(0), 3..3),
        Err(LineageError::InvalidRange)
    ));
}

#[test]
fn boundary_affinity_is_exact_at_insertions() {
    let mut store = SourceStore::new("abcd", 8);
    store
        .apply_edit(SourceRevision(0), 2..2, "XYZ")
        .expect("valid insertion");

    let mut before = store
        .map_boundary_from(SourceRevision(0), 2, BoundaryAffinity::Before)
        .expect("history retained");
    let mut after = store
        .map_boundary_from(SourceRevision(0), 2, BoundaryAffinity::After)
        .expect("history retained");
    assert_eq!(before.poll(1), MapStatus::ProvenBoundary(2));
    assert_eq!(after.poll(1), MapStatus::ProvenBoundary(5));
}

#[test]
fn bounded_lineage_expires_without_retaining_old_crop_roots() {
    let mut store = SourceStore::new("abc", 2);
    let old = store.query_snapshot();

    store
        .apply_edit(SourceRevision(0), 0..0, "x")
        .expect("valid insertion");
    let mut scalar_only_job = store
        .map_boundary_from(SourceRevision(0), 0, BoundaryAffinity::Before)
        .expect("first record retained");
    drop(old);
    assert_eq!(scalar_only_job.poll(1), MapStatus::ProvenBoundary(0));

    for revision in 1..3 {
        store
            .apply_edit(SourceRevision(revision), 0..0, "x")
            .expect("valid insertion");
    }
    assert_eq!(store.lineage().retention().records, 2);
    assert_eq!(store.lineage().retention().retained_source_roots, 0);
    assert!(matches!(
        store.map_boundary_from(SourceRevision(0), 0, BoundaryAffinity::Before),
        Err(LineageError::HistoryExpired)
    ));

    assert_eq!(store.query_snapshot().materialize_for_testing(), "xxxabc");
}

#[test]
fn stale_revision_is_rejected_without_mutating_source() {
    let mut store = SourceStore::new("abc", 4);
    store
        .apply_edit(SourceRevision(0), 0..0, "x")
        .expect("first edit");
    let before = store.query_snapshot();
    let error = store
        .apply_edit(SourceRevision(0), 0..0, "stale")
        .expect_err("stale edit must fail");
    assert_eq!(
        error,
        SourceStoreError::StaleRevision {
            expected: SourceRevision(0),
            actual: SourceRevision(1),
        }
    );
    assert_eq!(store.root_id(), before.identity());
    assert_eq!(store.query_snapshot().materialize_for_testing(), "xabc");
}

#[test]
fn cursor_refills_have_an_explicit_copy_cap() {
    let text = "😀β".repeat(5_000);
    let store = SourceStore::new(&text, 4);
    let mut cursor = store.query_snapshot().cursor();
    let mut streamed = Vec::with_capacity(text.len());
    while let Some(source_byte) = cursor.next_byte() {
        streamed.push(source_byte.byte);
    }

    assert_eq!(streamed, text.as_bytes());
    assert!(cursor.metrics().chunk_loads > 1);
    assert!(cursor.metrics().maximum_chunk_bytes <= SOURCE_CURSOR_COPY_CAP_BYTES);
    assert_eq!(cursor.metrics().chunk_bytes_copied, text.len());
}

#[test]
fn thousand_record_job_construction_and_polls_are_strictly_bounded() {
    const CAPACITY: usize = 1_000;
    const POLL_FUEL: usize = 13;
    let mut store = SourceStore::new("stable", CAPACITY);
    for revision in 0..u64::try_from(CAPACITY).expect("small capacity") {
        store
            .apply_edit(SourceRevision(revision), 0..0, "x")
            .expect("valid prefix insertion");
    }

    let retention = store.lineage().retention();
    assert_eq!(retention.records, CAPACITY);
    assert!(retention.tree_nodes <= retention.maximum_tree_nodes);
    assert!(retention.last_update_tree_nodes <= retention.maximum_update_tree_nodes);

    let mut job = store
        .map_range_from(SourceRevision(0), 0..6)
        .expect("oldest retained revision");
    let construction = job.metrics();
    assert_eq!(construction.constructor_records_examined, 1);
    assert_eq!(construction.constructor_records_validated, 1);
    assert_eq!(construction.records_copied, 0);
    assert!(
        construction.constructor_tree_nodes_read <= retention.maximum_update_tree_nodes,
        "constructor lookup exceeded fixed tree depth"
    );
    assert_eq!(
        job.poll(0),
        MapStatus::Pending {
            processed_records: 0,
            remaining_records: CAPACITY,
        }
    );
    assert_eq!(job.metrics(), construction, "zero fuel did hidden work");

    loop {
        let before = job.metrics();
        let status = job.poll(POLL_FUEL);
        let after = job.metrics();
        let examined = after.poll_records_examined - before.poll_records_examined;
        assert!(examined <= POLL_FUEL);
        assert_eq!(
            after.poll_records_validated - before.poll_records_validated,
            examined
        );
        assert!(
            after.poll_tree_nodes_read - before.poll_tree_nodes_read
                <= examined * retention.maximum_update_tree_nodes
        );
        if !matches!(status, MapStatus::Pending { .. }) {
            assert_eq!(status, MapStatus::ProvenRange(CAPACITY..CAPACITY + 6));
            break;
        }
    }
}

#[test]
fn ten_thousand_record_snapshot_survives_ring_overwrite_with_bounded_scalars() {
    const CAPACITY: usize = 10_000;
    const POLL_FUEL: usize = 37;
    let capacity_revision = u64::try_from(CAPACITY).expect("small capacity");
    let mut store = SourceStore::new("stable", CAPACITY);
    for revision in 0..capacity_revision {
        store
            .apply_edit(SourceRevision(revision), 0..0, "x")
            .expect("valid prefix insertion");
    }

    let mut old_snapshot = store
        .map_range_from(SourceRevision(0), 0..6)
        .expect("oldest retained revision");
    let snapshot_retention = old_snapshot.retention();
    assert_eq!(snapshot_retention.records, CAPACITY);
    assert_eq!(snapshot_retention.retained_source_roots, 0);
    assert!(snapshot_retention.tree_nodes <= snapshot_retention.maximum_tree_nodes);

    for revision in capacity_revision..capacity_revision * 2 {
        store
            .apply_edit(SourceRevision(revision), 0..0, "y")
            .expect("live source keeps editing while the old scalar job waits");
    }
    let current_retention = store.lineage().retention();
    assert_eq!(current_retention.records, CAPACITY);
    assert!(current_retention.tree_nodes <= current_retention.maximum_tree_nodes);
    assert!(
        snapshot_retention.tree_nodes + current_retention.tree_nodes
            <= snapshot_retention.maximum_tree_nodes + current_retention.maximum_tree_nodes
    );
    assert!(matches!(
        store.map_range_from(SourceRevision(0), 0..6),
        Err(LineageError::HistoryExpired)
    ));

    loop {
        let before = old_snapshot.metrics();
        let status = old_snapshot.poll(POLL_FUEL);
        let after = old_snapshot.metrics();
        let examined = after.poll_records_examined - before.poll_records_examined;
        assert!(examined <= POLL_FUEL);
        assert!(
            after.poll_tree_nodes_read - before.poll_tree_nodes_read
                <= examined * current_retention.maximum_update_tree_nodes
        );
        if !matches!(status, MapStatus::Pending { .. }) {
            assert_eq!(status, MapStatus::ProvenRange(CAPACITY..CAPACITY + 6));
            break;
        }
    }
    assert_eq!(old_snapshot.metrics().poll_records_examined, CAPACITY);
    assert_eq!(old_snapshot.metrics().poll_records_validated, CAPACITY);
    assert_eq!(old_snapshot.metrics().records_copied, 0);
    let poll_metrics = old_snapshot.metrics();
    let drop_started = Instant::now();
    drop(old_snapshot);
    let drop_elapsed = drop_started.elapsed();
    eprintln!(
        "lineage_snapshot_receipt capacity={CAPACITY} snapshot_records={} snapshot_nodes={} \
         edit_path_nodes_last={} edit_path_nodes_max={} constructor_tree_nodes={} \
         poll_records={} poll_tree_nodes={} poll_max_lookup_nodes={} drop_ns={}",
        snapshot_retention.records,
        snapshot_retention.tree_nodes,
        current_retention.last_update_tree_nodes,
        current_retention.maximum_update_tree_nodes,
        poll_metrics.constructor_tree_nodes_read,
        poll_metrics.poll_records_examined,
        poll_metrics.poll_tree_nodes_read,
        poll_metrics.maximum_tree_nodes_per_lookup,
        drop_elapsed.as_nanos(),
    );
}

use flark_engine::{
    LineDescriptor, LineEnding, LinePoll, SourceEditError, SourceRevision, SourceSnapshotLease,
    SourceStore, SourceUtf16Operation, SOURCE_CURSOR_WINDOW_BYTES,
    SOURCE_EDIT_MAX_REPLACEMENT_UTF16, SOURCE_SEED_PAGE_MAX_UTF16,
};

fn read_source(lease: SourceSnapshotLease) -> Vec<u8> {
    let mut cursor = lease.cursor().expect("cursor allocation");
    let mut output = Vec::new();
    let mut buffer = [0_u8; 257];
    loop {
        let count = cursor.read(&mut buffer);
        if count == 0 {
            break;
        }
        output.extend_from_slice(&buffer[..count]);
    }
    output
}

fn collect_lines(lease: SourceSnapshotLease, fuel: usize) -> Vec<LineDescriptor> {
    let mut cursor = lease.lines().expect("line cursor allocation");
    let mut lines = Vec::new();
    loop {
        match cursor.poll(fuel) {
            LinePoll::Pending => {}
            LinePoll::Line(line) => lines.push(line),
            LinePoll::Complete => return lines,
        }
    }
}

#[test]
fn cursor_copies_no_more_than_four_kib_and_preserves_unicode() {
    let text = format!("{}😀{}", "a".repeat(5_000), "β".repeat(3_000));
    let source = SourceStore::new(&text).expect("source");
    let lease = source.snapshot();
    let mut cursor = lease.cursor().expect("cursor");
    let mut actual = Vec::new();
    let mut buffer = [0_u8; 701];
    loop {
        let count = cursor.read(&mut buffer);
        if count == 0 {
            break;
        }
        actual.extend_from_slice(&buffer[..count]);
    }

    assert_eq!(actual, text.as_bytes());
    assert!(cursor.refill_count() > 1);
    assert!(cursor.max_refill_bytes() <= SOURCE_CURSOR_WINDOW_BYTES);
}

#[test]
fn physical_lines_are_authoritative_for_crlf_cr_lf_and_utf16() {
    let source = SourceStore::new("A😀\r\nβ\r末\nz").expect("source");
    let lines = collect_lines(source.snapshot(), 1);

    assert_eq!(lines.len(), 4);
    assert_line(lines[0], (0, 5, 7, 3, 5, LineEnding::CrLf));
    assert_line(lines[1], (7, 9, 10, 1, 2, LineEnding::Cr));
    assert_line(lines[2], (10, 13, 14, 1, 2, LineEnding::Lf));
    assert_line(lines[3], (14, 15, 15, 1, 1, LineEnding::Eof));
}

#[test]
fn empty_source_has_one_line_and_trailing_newline_has_no_phantom_line() {
    let empty = SourceStore::new("").expect("empty source");
    let empty_lines = collect_lines(empty.snapshot(), 1);
    assert_eq!(empty_lines.len(), 1);
    assert_line(empty_lines[0], (0, 0, 0, 0, 0, LineEnding::Eof));

    let trailing = SourceStore::new("a\n").expect("trailing source");
    let trailing_lines = collect_lines(trailing.snapshot(), 1);
    assert_eq!(trailing_lines.len(), 1);
    assert_line(trailing_lines[0], (0, 1, 2, 1, 2, LineEnding::Lf));
}

#[test]
fn prepared_edit_is_unpublished_until_commit() {
    let mut source = SourceStore::new("alpha β").expect("source");
    let before = source.version();
    let prepared = source
        .prepare_edit(before, 0..5, "omega")
        .expect("prepare edit");

    assert_eq!(source.version(), before);
    assert_eq!(read_source(source.snapshot()), "alpha β".as_bytes());

    let commit = source.commit_prepared_edit(prepared).expect("commit edit");
    let (receipt, retired) = commit.into_parts();
    assert_eq!(receipt.previous(), before);
    assert_ne!(receipt.current().root(), before.root());
    assert_eq!(read_source(source.snapshot()), "omega β".as_bytes());
    assert_eq!(read_source(retired), "alpha β".as_bytes());
}

#[test]
fn stale_versions_and_split_scalars_do_not_mutate_source() {
    let mut source = SourceStore::new("éx").expect("source");
    let original = source.version();

    let split = source
        .prepare_edit(original, 1..1, "!")
        .expect_err("split scalar must fail");
    assert_eq!(split, SourceEditError::SplitUtf8Scalar { offset: 1 });
    assert_eq!(source.version(), original);

    let prepared = source
        .prepare_edit(original, 2..3, "y")
        .expect("valid edit");
    source
        .commit_prepared_edit(prepared)
        .expect("commit valid edit");
    let current = source.version();

    let stale = source
        .prepare_edit(original, 0..0, "!")
        .expect_err("stale edit must fail");
    assert_eq!(
        stale,
        SourceEditError::StaleVersion {
            expected: original,
            actual: current,
        }
    );
    assert_eq!(source.version(), current);
    assert_eq!(read_source(source.snapshot()), "éy".as_bytes());
}

#[test]
fn paged_seed_binds_external_revision_and_observed_unicode_metrics() {
    let mut seed = SourceStore::seed(SourceRevision::new(41), 10);
    seed.append_page(0..5, "A😀\r\n").expect("first page");
    assert_eq!(seed.observed_byte_len(), 7);
    assert_eq!(seed.observed_utf16_len(), 5);
    seed.append_page(5..10, "β\r末\nz").expect("second page");
    assert_eq!(seed.observed_byte_len(), 15);
    assert_eq!(seed.observed_utf16_len(), 10);

    let source = seed.finalize().expect("complete seed");
    assert_eq!(source.version().revision(), SourceRevision::new(41));
    assert_eq!(source.version().byte_len(), 15);
    assert_eq!(source.version().utf16_len(), 10);
    assert_eq!(read_source(source.snapshot()), "A😀\r\nβ\r末\nz".as_bytes());

    let lines = collect_lines(source.snapshot(), 1);
    assert_eq!(lines.len(), 4);
    assert_line(lines[0], (0, 5, 7, 3, 5, LineEnding::CrLf));
    assert_line(lines[1], (7, 9, 10, 1, 2, LineEnding::Cr));
    assert_line(lines[2], (10, 13, 14, 1, 2, LineEnding::Lf));
    assert_line(lines[3], (14, 15, 15, 1, 1, LineEnding::Eof));
}

#[test]
fn empty_partial_and_malformed_seeds_never_publish_a_store() {
    let empty = SourceStore::seed(SourceRevision::new(7), 0)
        .finalize()
        .expect("empty seed");
    assert_eq!(empty.version().revision(), SourceRevision::new(7));
    assert_eq!(empty.version().byte_len(), 0);
    assert_eq!(empty.version().utf16_len(), 0);

    let mut explicit_empty = SourceStore::seed(SourceRevision::new(8), 0);
    explicit_empty
        .append_page(0..0, "")
        .expect("one explicit empty page");
    assert_eq!(
        explicit_empty
            .finalize()
            .expect("empty store")
            .version()
            .utf16_len(),
        0
    );

    let mut partial = SourceStore::seed(SourceRevision::new(9), 3);
    partial.append_page(0..1, "a").expect("partial page");
    assert_eq!(
        partial.finalize().expect_err("partial seed must fail"),
        SourceEditError::IncompleteSeed {
            expected: 3,
            observed: 1,
        }
    );

    let mut reordered = SourceStore::seed(SourceRevision::new(10), 3);
    reordered.append_page(0..1, "a").expect("first page");
    assert_eq!(
        reordered
            .append_page(2..3, "b")
            .expect_err("gap must poison seed"),
        SourceEditError::InvalidSeedPage {
            expected_start: 1,
            start: 2,
            end: 3,
            page_utf16_len: 1,
            expected_total: 3,
        }
    );
    assert_eq!(
        reordered
            .append_page(1..3, "bc")
            .expect_err("poisoned seed cannot resume"),
        SourceEditError::SeedPoisoned
    );
    assert_eq!(
        reordered.finalize().expect_err("poisoned seed must fail"),
        SourceEditError::SeedPoisoned
    );

    let mut wrong_metric = SourceStore::seed(SourceRevision::new(11), 2);
    assert!(matches!(
        wrong_metric.append_page(0..1, "😀"),
        Err(SourceEditError::InvalidSeedPage {
            page_utf16_len: 2,
            ..
        })
    ));

    let oversized_text = "x".repeat(SOURCE_SEED_PAGE_MAX_UTF16 + 1);
    let mut oversized = SourceStore::seed(SourceRevision::new(12), oversized_text.len());
    assert_eq!(
        oversized
            .append_page(0..oversized_text.len(), &oversized_text)
            .expect_err("oversized page"),
        SourceEditError::SeedPageTooLarge {
            observed: SOURCE_SEED_PAGE_MAX_UTF16 + 1,
            limit: SOURCE_SEED_PAGE_MAX_UTF16,
        }
    );
}

#[test]
fn checked_utf16_lookup_rejects_split_surrogates_without_panicking() {
    let source = SourceStore::new("a😀é\r\n末").expect("source");
    let lease = source.snapshot();

    for (utf16, byte) in [(0, 0), (1, 1), (3, 5), (4, 7), (5, 8), (6, 9), (7, 12)] {
        assert_eq!(lease.byte_offset_for_utf16(utf16), Ok(byte));
        assert_eq!(source.byte_offset_for_utf16(utf16), Ok(byte));
    }
    assert_eq!(
        lease.byte_offset_for_utf16(2),
        Err(SourceEditError::SplitUtf16Scalar { offset: 2 })
    );
    assert_eq!(
        lease.byte_offset_for_utf16(8),
        Err(SourceEditError::InvalidUtf16Offset { offset: 8, len: 7 })
    );
}

#[test]
fn checked_utf16_lookup_matches_every_scalar_boundary_across_rope_chunks() {
    let text = "aβ😀\r\n末".repeat(1_500);
    let source = SourceStore::new(&text).expect("chunked source");
    let lease = source.snapshot();
    let mut utf16_offset = 0_usize;

    assert_eq!(lease.byte_offset_for_utf16(0), Ok(0));
    for (byte_offset, scalar) in text.char_indices() {
        assert_eq!(lease.byte_offset_for_utf16(utf16_offset), Ok(byte_offset));
        if scalar.len_utf16() == 2 {
            assert_eq!(
                lease.byte_offset_for_utf16(utf16_offset + 1),
                Err(SourceEditError::SplitUtf16Scalar {
                    offset: utf16_offset + 1,
                })
            );
        }
        utf16_offset += scalar.len_utf16();
    }
    assert_eq!(lease.byte_offset_for_utf16(utf16_offset), Ok(text.len()));
}

#[test]
fn utf16_intent_is_atomic_and_preserves_same_offset_insertion_order() {
    let mut source = SourceStore::seed(SourceRevision::new(41), 7);
    source.append_page(0..7, "A😀\r\nβz").expect("seed page");
    let mut source = source.finalize().expect("seed source");
    let before = source.version();
    let operations = [
        SourceUtf16Operation::new(0..0, "<"),
        SourceUtf16Operation::new(1..1, "x"),
        SourceUtf16Operation::new(1..1, "y"),
        SourceUtf16Operation::new(1..3, "🙂"),
        SourceUtf16Operation::new(3..5, "\n"),
        SourceUtf16Operation::new(6..7, "Z"),
    ];
    let prepared = source
        .prepare_utf16_edit_intent(before, SourceRevision::new(42), &operations)
        .expect("prepare intent");

    assert_eq!(source.version(), before);
    assert_eq!(read_source(source.snapshot()), "A😀\r\nβz".as_bytes());

    let (receipt, retired) = source
        .commit_prepared_utf16_edit_intent(prepared)
        .expect("commit intent")
        .into_parts();
    assert_eq!(receipt.previous(), before);
    assert_eq!(receipt.current().revision(), SourceRevision::new(42));
    assert_eq!(receipt.operation_count(), operations.len());
    assert_eq!(receipt.replacement_byte_len(), 9);
    assert_eq!(receipt.replacement_utf16_len(), 7);
    assert_eq!(read_source(source.snapshot()), "<Axy🙂\nβZ".as_bytes());
    assert_eq!(read_source(retired), "A😀\r\nβz".as_bytes());
}

#[test]
fn utf16_plan_preflights_exact_metrics_before_target_materialization() {
    let mut source = SourceStore::new("A😀z").expect("source");
    let before = source.version();
    let operations = [
        SourceUtf16Operation::new(0..0, "<"),
        SourceUtf16Operation::new(1..3, "🙂"),
        SourceUtf16Operation::new(3..4, "tail"),
    ];

    let plan = source
        .plan_utf16_edit_intent(before, SourceRevision::new(1), &operations)
        .expect("metric plan");
    assert_eq!(plan.expected(), before);
    assert_eq!(plan.declared_revision(), SourceRevision::new(1));
    assert_eq!(plan.operation_count(), 3);
    assert_eq!(plan.target_byte_len(), "<A🙂tail".len());
    assert_eq!(plan.target_utf16_len(), "<A🙂tail".encode_utf16().count());
    assert_eq!(
        source.version(),
        before,
        "planning must not allocate a root"
    );

    let prepared = source
        .materialize_utf16_edit_intent(plan)
        .expect("materialize admitted plan");
    assert_eq!(
        source.version(),
        before,
        "materialization remains unpublished"
    );
    let commit = source
        .commit_prepared_utf16_edit_intent(prepared)
        .expect("commit materialized plan");
    let (receipt, retired) = commit.into_parts();
    assert_ne!(receipt.current().root(), before.root());
    assert_eq!(read_source(source.snapshot()), "<A🙂tail".as_bytes());
    assert_eq!(read_source(retired), "A😀z".as_bytes());
}

#[test]
fn malformed_or_stale_utf16_intents_roll_back_as_one_unit() {
    let mut source = SourceStore::new("a😀b").expect("source");
    let before = source.version();

    assert_eq!(
        source
            .prepare_utf16_edit_intent(before, SourceRevision::new(1), &[])
            .expect_err("empty intent"),
        SourceEditError::EmptyEditIntent
    );
    assert_eq!(
        source
            .prepare_utf16_edit_intent(
                before,
                SourceRevision::new(2),
                &[SourceUtf16Operation::new(0..0, "!")],
            )
            .expect_err("skipped revision"),
        SourceEditError::InvalidRevisionTransition {
            current: SourceRevision::new(0),
            declared: SourceRevision::new(2),
        }
    );

    let partial_then_split = [
        SourceUtf16Operation::new(0..0, "first"),
        SourceUtf16Operation::new(2..2, "split"),
    ];
    assert_eq!(
        source
            .prepare_utf16_edit_intent(before, SourceRevision::new(1), &partial_then_split)
            .expect_err("later split boundary rolls back all operations"),
        SourceEditError::SplitUtf16Scalar { offset: 2 }
    );

    let unordered = [
        SourceUtf16Operation::new(3..3, "later"),
        SourceUtf16Operation::new(0..0, "earlier"),
    ];
    assert!(matches!(
        source.prepare_utf16_edit_intent(before, SourceRevision::new(1), &unordered),
        Err(SourceEditError::InvalidOperationOrder { .. })
    ));
    let overlapping = [
        SourceUtf16Operation::new(0..3, "first"),
        SourceUtf16Operation::new(1..3, "second"),
    ];
    assert!(matches!(
        source.prepare_utf16_edit_intent(before, SourceRevision::new(1), &overlapping),
        Err(SourceEditError::InvalidOperationOrder { .. })
    ));
    assert_eq!(source.version(), before);
    assert_eq!(read_source(source.snapshot()), "a😀b".as_bytes());

    let prepared = source
        .prepare_utf16_edit_intent(
            before,
            SourceRevision::new(1),
            &[SourceUtf16Operation::new(4..4, "!")],
        )
        .expect("prepared intent");
    let competing = source
        .prepare_edit(before, 0..1, "A")
        .expect("competing byte edit");
    source
        .commit_prepared_edit(competing)
        .expect("commit competing edit");
    let after_competing = source.version();
    let stale_error = match source.commit_prepared_utf16_edit_intent(prepared) {
        Ok(_) => panic!("stale prepared intent must not commit"),
        Err(error) => error,
    };
    assert_eq!(
        stale_error,
        SourceEditError::StaleVersion {
            expected: before,
            actual: after_competing,
        }
    );
    assert_eq!(source.version(), after_competing);
    assert_eq!(read_source(source.snapshot()), "A😀b".as_bytes());
}

#[test]
fn externally_declared_revisions_advance_sequentially() {
    let mut seed = SourceStore::seed(SourceRevision::new(9), 1);
    seed.append_page(0..1, "a").expect("seed page");
    let mut source = seed.finalize().expect("source");

    for (revision, insertion) in [(10, "b"), (11, "c"), (12, "d")] {
        let expected = source.version();
        let end = expected.utf16_len();
        let prepared = source
            .prepare_utf16_edit_intent(
                expected,
                SourceRevision::new(revision),
                &[SourceUtf16Operation::new(end..end, insertion)],
            )
            .expect("sequential prepare");
        source
            .commit_prepared_utf16_edit_intent(prepared)
            .expect("sequential commit");
    }
    assert_eq!(source.version().revision(), SourceRevision::new(12));
    assert_eq!(read_source(source.snapshot()), b"abcd");

    let current = source.version();
    assert!(matches!(
        source.prepare_utf16_edit_intent(
            current,
            SourceRevision::new(14),
            &[SourceUtf16Operation::new(0..0, "!")],
        ),
        Err(SourceEditError::InvalidRevisionTransition { .. })
    ));
    assert_eq!(source.version(), current);
}

#[test]
fn bounded_seed_and_intent_capabilities_are_send() {
    fn assert_send<T: Send>() {}

    assert_send::<flark_engine::SourceSeedBuilder>();
    assert_send::<flark_engine::PreparedSourceEditIntent>();
    assert_send::<flark_engine::SourceEditIntentCommit>();

    let oversized_replacement = "x".repeat(SOURCE_EDIT_MAX_REPLACEMENT_UTF16 + 1);
    let source = SourceStore::new("a").expect("source");
    let before = source.version();
    assert_eq!(
        source
            .prepare_utf16_edit_intent(
                before,
                SourceRevision::new(1),
                &[SourceUtf16Operation::new(0..0, &oversized_replacement)],
            )
            .expect_err("replacement bound"),
        SourceEditError::EditReplacementTooLarge {
            observed: SOURCE_EDIT_MAX_REPLACEMENT_UTF16 + 1,
            limit: SOURCE_EDIT_MAX_REPLACEMENT_UTF16,
        }
    );
    assert_eq!(source.version(), before);
}

fn assert_line(actual: LineDescriptor, expected: (usize, usize, usize, usize, usize, LineEnding)) {
    assert_eq!(actual.start_byte(), expected.0);
    assert_eq!(actual.content_end_byte(), expected.1);
    assert_eq!(actual.end_byte(), expected.2);
    assert_eq!(actual.content_utf16(), expected.3);
    assert_eq!(actual.physical_utf16(), expected.4);
    assert_eq!(actual.ending(), expected.5);
}

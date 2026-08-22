use flark_engine::{
    DocumentRuntime, DocumentRuntimeConfig, OpeningSourceError, OpeningSourceStore,
    SourceEditError, SourceRevision, SourceSnapshotLease,
};

fn read_source(lease: SourceSnapshotLease) -> Vec<u8> {
    let mut cursor = lease.cursor().expect("cursor allocation");
    let mut output = Vec::new();
    let mut buffer = [0_u8; 257];
    loop {
        let count = cursor.read(&mut buffer);
        if count == 0 {
            return output;
        }
        output.extend_from_slice(&buffer[..count]);
    }
}

fn close_runtime(mut runtime: DocumentRuntime) {
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("runtime close").complete {}
}

#[test]
fn append_publishes_immutable_prefixes_and_seal_reuses_the_last_root() {
    let mut opening = OpeningSourceStore::new(SourceRevision::new(41), Some(10)).expect("opening");
    let initial = opening.version();
    let initial_authority = opening.authority();
    let first = opening
        .append_page(initial, 0..5, "A😀\r\n")
        .expect("first page");
    let first_snapshot = opening.snapshot();

    assert_eq!(first.load(), initial.load());
    assert_eq!(first.generation(), 1);
    assert_eq!(first.revision(), SourceRevision::new(41));
    assert_eq!(first_snapshot.authority(), initial_authority);
    assert_eq!(first.admitted_input_bytes(), 7);
    assert_eq!(first.admitted_input_utf16(), 5);
    assert_eq!(first.current_bytes(), 7);
    assert_eq!(first.current_utf16(), 5);
    assert!(!first.input_complete());
    assert_eq!(
        read_source(first_snapshot.into_source_lease()),
        "A😀\r\n".as_bytes()
    );

    let second = opening
        .append_page(first, 5..10, "β\r末\nz")
        .expect("second page");
    let final_snapshot = opening.snapshot();
    assert_eq!(second.load(), initial.load());
    assert_eq!(second.generation(), 2);
    assert!(second.input_complete());
    assert_ne!(second.root(), first.root());
    assert_eq!(final_snapshot.authority(), initial_authority);
    assert_eq!(
        read_source(final_snapshot.into_source_lease()),
        "A😀\r\nβ\r末\nz".as_bytes()
    );
    let append_proof = opening
        .prove_append_since(first)
        .expect("same-revision generations prove append lineage");
    assert_eq!(append_proof.previous(), first);
    assert_eq!(append_proof.current(), second);
    assert_eq!(append_proof.authority(), initial_authority);
    assert_eq!(append_proof.previous_source_version().byte_len(), 7);
    assert_eq!(append_proof.current_source_version().byte_len(), 15);

    let sealed_root = second.root();
    let source = opening.seal().expect("seal");
    assert_eq!(source.version().root(), sealed_root);
    assert_eq!(source.version().revision(), SourceRevision::new(41));
    assert_eq!(source.authority(), initial_authority);
    assert_eq!(read_source(source.snapshot()), "A😀\r\nβ\r末\nz".as_bytes());
}

#[test]
fn admitted_prefix_edit_and_later_stream_append_are_separate_axes() {
    let mut opening = OpeningSourceStore::new(SourceRevision::new(8), Some(11)).expect("opening");
    let first = opening
        .append_page(opening.version(), 0..6, "alpha\n")
        .expect("first page");
    let before_edit = opening.snapshot();
    let before_edit_authority = before_edit.authority();
    let edited = opening
        .apply_utf16_edit(first, 0..5, "omega!")
        .expect("prefix edit");

    assert_eq!(edited.load(), first.load());
    assert_eq!(edited.revision(), SourceRevision::new(9));
    assert_eq!(opening.authority().document(), before_edit_authority.document());
    assert_ne!(opening.authority(), before_edit_authority);
    assert_eq!(edited.generation(), 2);
    assert_eq!(edited.admitted_input_utf16(), 6);
    assert_eq!(edited.current_utf16(), 7);
    assert_eq!(
        opening
            .prove_append_since(first)
            .expect_err("an edit cannot masquerade as append-only lineage"),
        OpeningSourceError::NotAppendLineage {
            previous: first,
            current: edited,
        }
    );
    assert_eq!(read_source(before_edit.into_source_lease()), b"alpha\n");
    assert_eq!(
        read_source(opening.snapshot().into_source_lease()),
        b"omega!\n"
    );

    let complete = opening
        .append_page(edited, 6..11, "tail\n")
        .expect("later stream page");
    assert_eq!(complete.admitted_input_utf16(), 11);
    assert_eq!(complete.current_utf16(), 12);
    assert_eq!(complete.revision(), SourceRevision::new(9));

    let source = opening.seal().expect("seal edited opening source");
    assert_eq!(source.version().root(), complete.root());
    assert_eq!(read_source(source.snapshot()), b"omega!\ntail\n");
}

#[test]
fn stale_or_malformed_opening_operations_never_mutate_silently() {
    let mut opening = OpeningSourceStore::new(SourceRevision::new(1), Some(3)).expect("opening");
    let initial = opening.version();
    let first = opening.append_page(initial, 0..1, "a").expect("first page");

    assert_eq!(
        opening
            .append_page(initial, 1..2, "b")
            .expect_err("stale generation"),
        OpeningSourceError::StaleVersion {
            expected: initial,
            actual: first,
        }
    );
    assert_eq!(opening.version(), first);
    assert_eq!(read_source(opening.snapshot().into_source_lease()), b"a");

    assert_eq!(
        opening
            .append_page(first, 2..3, "b")
            .expect_err("gap poisons input"),
        OpeningSourceError::Source(SourceEditError::InvalidSeedPage {
            expected_start: 1,
            start: 2,
            end: 3,
            page_utf16_len: 1,
            expected_total: 3,
        })
    );
    assert_eq!(
        opening
            .apply_utf16_edit(opening.version(), 0..1, "z")
            .expect_err("poisoned input cannot be edited"),
        OpeningSourceError::Source(SourceEditError::SeedPoisoned)
    );
    assert_eq!(
        opening.seal().expect_err("poisoned input cannot seal"),
        OpeningSourceError::Source(SourceEditError::SeedPoisoned)
    );
}

#[test]
fn a_trailing_bare_cr_is_not_an_unsealed_frontier() {
    let mut opening = OpeningSourceStore::new(SourceRevision::new(2), Some(9)).expect("opening");
    let initial = opening.version();
    let first = opening
        .append_page(initial, 0..7, "alpha\r\r")
        .expect("first page");
    let lease = opening.snapshot().into_source_lease();

    // An interior bare CR is a complete ending under both interpretations;
    // the trailing CR is a line start only for a sealed snapshot, because a
    // later LF may still join it into one CRLF ending.
    assert!(lease.is_physical_line_start(6).expect("interior sealed"));
    assert!(lease
        .is_unsealed_physical_line_frontier(6)
        .expect("interior unsealed"));
    assert!(lease.is_physical_line_start(7).expect("trailing sealed"));
    assert!(!lease
        .is_unsealed_physical_line_frontier(7)
        .expect("trailing unsealed"));

    // The append-lineage proof exposes the same rule to the parser seam.
    let proof = opening.prove_append_since(initial).expect("proof");
    assert!(!proof
        .current_admits_unsealed_line_frontier()
        .expect("proof frontier"));

    let _ = opening.append_page(first, 7..9, "\nz").expect("second page");
    let joined = opening.snapshot().into_source_lease();
    // The LF arrived: the pair is one CRLF ending, and the old frontier is
    // now interior to it under both interpretations.
    assert!(!joined.is_physical_line_start(7).expect("joined sealed"));
    assert!(!joined
        .is_unsealed_physical_line_frontier(7)
        .expect("joined unsealed"));
    assert!(joined
        .is_unsealed_physical_line_frontier(8)
        .expect("after crlf"));
    let proof = opening.prove_append_since(first).expect("later proof");
    assert!(!proof
        .current_admits_unsealed_line_frontier()
        .expect("mid-line tail"));
}

#[test]
fn the_last_unsealed_frontier_skips_ambiguous_tails_and_unknown_lengths_seal() {
    let mut opening = OpeningSourceStore::new(SourceRevision::new(4), None).expect("opening");
    let version = opening
        .append_page(opening.version(), 0..10, "a\nb\rc\r\nd\r\r")
        .expect("page");
    let lease = opening.snapshot().into_source_lease();

    // Admissible boundaries walk backward past the ambiguous trailing CR to
    // the interior bare CR whose successor is known not to be LF.
    assert_eq!(
        lease.last_unsealed_physical_line_frontier(0).expect("scan"),
        Some(9)
    );
    assert_eq!(
        lease.last_unsealed_physical_line_frontier(9).expect("floor"),
        None
    );

    // An unknown-length stream is never "input complete"; only the seal ends
    // it, at exactly the admitted text.
    assert_eq!(version.declared_input_complete(), None);
    assert!(!version.input_complete());
    let sealed = opening.seal().expect("unknown-length seal");
    assert_eq!(sealed.version().byte_len(), 10);
}

#[test]
fn empty_source_seals_without_a_phantom_page_and_opening_store_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<OpeningSourceStore>();

    let opening = OpeningSourceStore::new(SourceRevision::new(3), Some(0)).expect("empty opening");
    assert!(opening.version().input_complete());
    let root = opening.version().root();
    let source = opening.seal().expect("seal empty source");
    assert_eq!(source.version().root(), root);
    assert_eq!(read_source(source.snapshot()), b"");
}

#[test]
fn runtime_replica_advances_only_through_store_minted_append_lineage() {
    let mut opening = OpeningSourceStore::new(SourceRevision::new(5), Some(12)).expect("opening");
    let first = opening
        .append_page(opening.version(), 0..6, "first\n")
        .expect("first page");
    let authority = opening.authority();
    let mut runtime = DocumentRuntime::from_opening_snapshot(
        opening.snapshot(),
        DocumentRuntimeConfig::default(),
    )
    .expect("opening runtime");
    let initial_runtime_source = runtime.current_source_version().expect("runtime source");

    let second = opening
        .append_page(first, 6..12, "second")
        .expect("second page");
    let receipt = runtime
        .adopt_opening_append(
            opening
                .prove_append_since(first)
                .expect("append proof"),
        )
        .expect("adopt append");

    assert_eq!(receipt.authority(), authority);
    assert_eq!(receipt.previous(), initial_runtime_source);
    assert_eq!(receipt.current().root(), second.root());
    assert_eq!(receipt.unchanged_prefix_bytes(), 6);
    assert_eq!(receipt.current_generation(), second.generation());
    assert_eq!(runtime.current_source_version(), Some(receipt.current()));
    assert_eq!(read_source(runtime.snapshot_current_source().unwrap()), b"first\nsecond");
    assert_eq!(runtime.poll_retirement(1).released_source_leases, 1);
    close_runtime(runtime);
}

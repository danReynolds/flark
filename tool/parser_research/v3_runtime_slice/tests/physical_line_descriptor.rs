use flark_v3_runtime_slice::{
    SourcePhysicalLineDescriptor, SourcePhysicalLineEnding, SourceRevision, SourceRootId,
    SourceSnapshotDescriptor, SourceStore, SourceStoreError,
};

const TEN_MIB: usize = 10 * 1024 * 1024;
const INDEX_LEAF_BYTES: usize = 4 * 1024;

fn assert_line(
    line: SourcePhysicalLineDescriptor,
    source: SourceSnapshotDescriptor,
    start: usize,
    content_end: usize,
    end: usize,
    ending: SourcePhysicalLineEnding,
) {
    assert_eq!(line.source(), source);
    assert_eq!(line.start(), start);
    assert_eq!(line.content_end(), content_end);
    assert_eq!(line.end(), end);
    assert_eq!(line.ending(), ending);
    assert_eq!(line.end() - line.content_end(), line.ending().bytes());
    assert_eq!(
        line.physical_utf16(),
        line.content_utf16() + line.ending().bytes()
    );

    let receipt = line.receipt();
    assert!(receipt.tree_nodes_visited <= receipt.index_height);
    assert!(receipt.boundary_bytes_scanned <= INDEX_LEAF_BYTES);
    assert!(receipt.maximum_boundary_scratch_bytes <= INDEX_LEAF_BYTES);
    assert!(receipt.adjacent_bytes_read <= 4);
    assert_eq!(receipt.retained_source_roots, 0);
    assert_eq!(receipt.retained_source_bytes, 0);
}

fn naive_lines(text: &str) -> Vec<(usize, usize, usize, SourcePhysicalLineEnding)> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    loop {
        let mut content_end = start;
        while content_end < bytes.len() && !matches!(bytes[content_end], b'\r' | b'\n') {
            let scalar = text[content_end..]
                .chars()
                .next()
                .expect("a nonempty valid UTF-8 suffix has one scalar");
            content_end += scalar.len_utf8();
        }
        let (end, ending) = match bytes.get(content_end) {
            Some(b'\n') => (content_end + 1, SourcePhysicalLineEnding::Lf),
            Some(b'\r') if bytes.get(content_end + 1) == Some(&b'\n') => {
                (content_end + 2, SourcePhysicalLineEnding::CrLf)
            }
            Some(b'\r') => (content_end + 1, SourcePhysicalLineEnding::LoneCr),
            None => (content_end, SourcePhysicalLineEnding::BareEof),
            Some(_) => unreachable!("the content loop stops only at EOF or an ending"),
        };
        lines.push((start, content_end, end, ending));
        if ending == SourcePhysicalLineEnding::BareEof {
            break;
        }
        start = end;
    }
    lines
}

fn deterministic_index(state: u64, len: usize) -> usize {
    let len = u64::try_from(len).expect("test collection length fits u64");
    usize::try_from(state % len).expect("modulo result fits usize")
}

#[test]
fn exact_descriptors_cover_lf_cr_crlf_unicode_and_bare_eof() {
    let text = "α😀\nβ\rγ尾\r\nlast😀";
    let store = SourceStore::new(text, 8);
    let source = store.descriptor();

    assert_line(
        store.query_physical_line_descriptor(source, 0).unwrap(),
        source,
        0,
        6,
        7,
        SourcePhysicalLineEnding::Lf,
    );
    assert_line(
        store.query_physical_line_descriptor(source, 7).unwrap(),
        source,
        7,
        9,
        10,
        SourcePhysicalLineEnding::LoneCr,
    );
    assert_line(
        store.query_physical_line_descriptor(source, 10).unwrap(),
        source,
        10,
        15,
        17,
        SourcePhysicalLineEnding::CrLf,
    );
    assert_line(
        store.query_physical_line_descriptor(source, 17).unwrap(),
        source,
        17,
        text.len(),
        text.len(),
        SourcePhysicalLineEnding::BareEof,
    );

    for (text, ending, content_end, end) in [
        ("", SourcePhysicalLineEnding::BareEof, 0, 0),
        ("\n", SourcePhysicalLineEnding::Lf, 0, 1),
        ("\r", SourcePhysicalLineEnding::LoneCr, 0, 1),
        ("\r\n", SourcePhysicalLineEnding::CrLf, 0, 2),
        ("bare", SourcePhysicalLineEnding::BareEof, 4, 4),
    ] {
        let store = SourceStore::new(text, 8);
        assert_line(
            store
                .query_physical_line_descriptor(store.descriptor(), 0)
                .unwrap(),
            store.descriptor(),
            0,
            content_end,
            end,
            ending,
        );
    }

    let terminated = SourceStore::new("a\r\n", 8);
    assert_line(
        terminated
            .query_physical_line_descriptor(terminated.descriptor(), 3)
            .unwrap(),
        terminated.descriptor(),
        3,
        3,
        3,
        SourcePhysicalLineEnding::BareEof,
    );
}

#[test]
fn ten_mib_single_lines_resolve_from_summaries_not_line_scans() {
    let ascii = format!("{}\r\nnext", "a".repeat(TEN_MIB));
    let store = SourceStore::new(&ascii, 8);
    let source = store.descriptor();
    let first = store.query_physical_line_descriptor(source, 0).unwrap();
    assert_line(
        first,
        source,
        0,
        TEN_MIB,
        TEN_MIB + 2,
        SourcePhysicalLineEnding::CrLf,
    );
    assert_eq!(first.receipt().tree_nodes_visited, 0);
    assert_eq!(first.receipt().boundary_bytes_scanned, 0);

    let tail = store
        .query_physical_line_descriptor(source, TEN_MIB + 2)
        .unwrap();
    assert_line(
        tail,
        source,
        TEN_MIB + 2,
        ascii.len(),
        ascii.len(),
        SourcePhysicalLineEnding::BareEof,
    );

    let unicode = format!("{}\n", "😀".repeat(TEN_MIB / 4));
    let store = SourceStore::new(&unicode, 8);
    let line = store
        .query_physical_line_descriptor(store.descriptor(), 0)
        .unwrap();
    assert_line(
        line,
        store.descriptor(),
        0,
        TEN_MIB,
        TEN_MIB + 1,
        SourcePhysicalLineEnding::Lf,
    );
    assert_eq!(line.receipt().tree_nodes_visited, 0);
    assert_eq!(line.receipt().boundary_bytes_scanned, 0);
}

#[test]
fn crlf_split_across_index_leaves_is_still_one_exact_ending() {
    let text = format!("{}\r\n😀tail", "a".repeat(INDEX_LEAF_BYTES - 1));
    let store = SourceStore::new(&text, 8);
    let source = store.descriptor();
    let first = store.query_physical_line_descriptor(source, 0).unwrap();
    assert_line(
        first,
        source,
        0,
        INDEX_LEAF_BYTES - 1,
        INDEX_LEAF_BYTES + 1,
        SourcePhysicalLineEnding::CrLf,
    );
    let second = store
        .query_physical_line_descriptor(source, INDEX_LEAF_BYTES + 1)
        .unwrap();
    assert_line(
        second,
        source,
        INDEX_LEAF_BYTES + 1,
        text.len(),
        text.len(),
        SourcePhysicalLineEnding::BareEof,
    );
}

#[test]
fn edits_update_descriptors_locally_and_invalidate_old_snapshots() {
    let mut store = SourceStore::new("alpha\nβeta\r\nomega", 8);
    let old = store.descriptor();
    let old_second = store.query_physical_line_descriptor(old, 6).unwrap();
    assert_eq!(old_second.content_end(), 11);
    assert_eq!(old_second.end(), 13);

    store
        .apply_edit(SourceRevision(0), 0..5, "longer😀")
        .unwrap();
    let current = store.descriptor();
    assert!(matches!(
        store.query_physical_line_descriptor(old, 6),
        Err(SourceStoreError::SnapshotMismatch { expected, actual })
            if expected == old && actual == current
    ));

    let second_start = "longer😀\n".len();
    let second = store
        .query_physical_line_descriptor(current, second_start)
        .unwrap();
    assert_line(
        second,
        current,
        second_start,
        second_start + "βeta".len(),
        second_start + "βeta\r\n".len(),
        SourcePhysicalLineEnding::CrLf,
    );
    let receipt = second.receipt();
    assert!(receipt.tree_nodes_visited <= receipt.index_height);
    assert!(receipt.boundary_bytes_scanned <= INDEX_LEAF_BYTES);
}

#[test]
fn deterministic_unicode_and_line_ending_edits_match_a_naive_oracle() {
    let mut text = "seed😀\r\nalpha\rbeta\n尾".repeat(8);
    let mut store = SourceStore::new(&text, 32);
    let replacements = ["", "x", "😀", "\n", "\r", "\r\n", "β\r😀\n"];
    let mut state = 0xD1B5_4A32_D192_ED03u64;

    for _ in 0..250 {
        let boundaries: Vec<_> = (0..=text.len())
            .filter(|offset| text.is_char_boundary(*offset))
            .collect();
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let first = boundaries[deterministic_index(state, boundaries.len())];
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let second = boundaries[deterministic_index(state, boundaries.len())];
        let range = first.min(second)..first.max(second);
        let replacement =
            replacements[deterministic_index(state.rotate_left(17), replacements.len())];
        store
            .apply_edit(store.revision(), range.clone(), replacement)
            .unwrap();
        text.replace_range(range, replacement);

        let source = store.descriptor();
        for (start, content_end, end, ending) in naive_lines(&text) {
            assert_line(
                store.query_physical_line_descriptor(source, start).unwrap(),
                source,
                start,
                content_end,
                end,
                ending,
            );
        }
    }
}

#[test]
fn wrong_snapshot_non_line_start_and_invalid_utf8_cut_are_rejected() {
    let store = SourceStore::new("alpha\r\n😀tail", 8);
    let source = store.descriptor();
    let wrong_root = SourceSnapshotDescriptor {
        root: SourceRootId(source.root.0 + 1),
        ..source
    };
    let wrong_revision = SourceSnapshotDescriptor {
        revision: SourceRevision(source.revision.0 + 1),
        ..source
    };
    assert!(matches!(
        store.query_physical_line_descriptor(wrong_root, 0),
        Err(SourceStoreError::SnapshotMismatch { .. })
    ));
    assert!(matches!(
        store.query_physical_line_descriptor(wrong_revision, 0),
        Err(SourceStoreError::SnapshotMismatch { .. })
    ));
    assert!(matches!(
        store.query_physical_line_descriptor(source, 2),
        Err(SourceStoreError::NotPhysicalLineStart { offset: 2 })
    ));
    assert!(matches!(
        store.query_physical_line_descriptor(source, 6),
        Err(SourceStoreError::NotPhysicalLineStart { offset: 6 })
    ));
    assert!(matches!(
        store.query_physical_line_descriptor(source, 8),
        Err(SourceStoreError::Source(
            flark_v3_runtime_slice::SourceError::NotCharBoundary(8)
        ))
    ));
}

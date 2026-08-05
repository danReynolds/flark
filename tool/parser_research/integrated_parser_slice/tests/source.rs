use flark_integrated_parser_slice::source::{
    AllocationMetrics, PersistentSource, SourceCapture, SourceError, MAX_PIECE_BYTES,
};

fn cursor_bytes(source: &PersistentSource) -> Vec<u8> {
    source.cursor().map(|item| item.byte).collect()
}

fn assert_copy_ledger(allocations: AllocationMetrics, rewritten_bytes: usize) {
    assert_eq!(allocations.staged_bytes_copied, rewritten_bytes);
    assert_eq!(allocations.immutable_bytes_copied, rewritten_bytes);
    assert_eq!(allocations.copied_bytes, rewritten_bytes * 2);
}

#[test]
fn unicode_boundaries_and_anchored_cursor_are_exact() {
    let text = "aé🙂中\nβ";
    let source = PersistentSource::from_text(text);
    source.validate().unwrap();
    assert_eq!(cursor_bytes(&source), text.as_bytes());
    assert_eq!(source.line_breaks(), 1);

    let emoji = text.find('🙂').unwrap();
    for split in emoji + 1..emoji + '🙂'.len_utf8() {
        assert_eq!(
            source.edit(split..split, "x").unwrap_err(),
            SourceError::NotCharBoundary(split)
        );
    }
    let outcome = source.edit(emoji..emoji + '🙂'.len_utf8(), "🦀").unwrap();
    assert_eq!(outcome.source.materialize(), "aé🦀中\nβ");
    assert_eq!(outcome.metrics.copied_replacement_bytes, '🦀'.len_utf8());
    assert!(outcome.metrics.copied_existing_source_bytes <= 2 * MAX_PIECE_BYTES);
    assert_copy_ledger(
        outcome.metrics.allocations,
        outcome.metrics.copied_replacement_bytes + outcome.metrics.copied_existing_source_bytes,
    );
    outcome.source.validate().unwrap();
}

#[test]
fn metered_cursor_position_reports_logarithmic_index_work() {
    let text = "x".repeat(10 * 1024 * 1024);
    let source = PersistentSource::from_text(&text);
    let (mut cursor, receipt) = source.cursor_at_metered(text.len() - 1).unwrap();
    assert_eq!(cursor.next().map(|item| item.byte), Some(b'x'));
    assert!(receipt.index_nodes_visited > 0);
    assert!(receipt.index_nodes_visited <= source.metrics().depth);
}

#[test]
fn sequential_capture_reuses_the_existing_scan_without_payload_copy_or_tree_reseek() {
    let text = format!("prefix-{}-suffix", "🙂abcdef".repeat(2_000));
    let source = PersistentSource::from_text(&text);
    let start = text.find('🙂').unwrap();
    let end = text.rfind("-suffix").unwrap();
    let mut cursor = source.cursor_at(start).unwrap();
    let start_boundary = cursor.certified_boundary().unwrap();
    let mut capture = SourceCapture::new(start_boundary);
    for expected in &text.as_bytes()[start..end] {
        let actual = cursor.next_captured(&mut capture).unwrap().unwrap();
        assert_eq!(actual.byte, *expected);
    }
    let end_boundary = cursor.certified_boundary().unwrap();
    let captured = capture.finish(end_boundary).unwrap();

    assert_eq!(captured.document_range(), start..end);
    assert_eq!(captured.fragment().materialize(), &text[start..end]);
    assert_eq!(captured.metrics().bytes_observed, end - start);
    assert_eq!(captured.metrics().payload_bytes_copied, 0);
    assert!(captured.metrics().piece_runs > 1);
    assert!(captured.metrics().nodes_allocated <= captured.metrics().piece_runs * 2);
    assert_eq!(captured.fragment().first_anchor(), source.anchor_at(start));
}

#[test]
fn bounded_checkpoint_merge_and_trim_transfer_pending_runs_without_temp_nodes() {
    let source = PersistentSource::from_text("ab> x");
    let mut cursor = source.cursor();
    let mut prefix = SourceCapture::new(cursor.certified_boundary().unwrap());
    for _ in 0..2 {
        cursor.next_captured(&mut prefix).unwrap().unwrap();
    }
    let mut checkpoint = SourceCapture::new(cursor.certified_boundary().unwrap());
    for _ in 0..3 {
        cursor.next_captured(&mut checkpoint).unwrap().unwrap();
    }
    assert_eq!(prefix.metrics().nodes_allocated, 0);
    assert_eq!(checkpoint.metrics().nodes_allocated, 0);

    prefix.append_bounded(checkpoint, 3).unwrap();
    assert_eq!(prefix.metrics().nodes_allocated, 0);
    let end = prefix.certified_end();
    let captured = prefix.finish(end).unwrap();
    assert_eq!(captured.fragment().materialize(), "ab> x");
    assert_eq!(captured.metrics().nodes_allocated, 1);
    assert_eq!(captured.metrics().payload_bytes_copied, 0);

    // Drop the marker and space without looking them up again. The surviving
    // `x` is still a pending immutable-buffer range, not a temporary tree leaf.
    let mut trim_cursor = source.cursor_at(2).unwrap();
    let mut checkpoint = SourceCapture::new(trim_cursor.certified_boundary().unwrap());
    for _ in 0..2 {
        trim_cursor.next_captured(&mut checkpoint).unwrap().unwrap();
    }
    let content = trim_cursor.certified_boundary().unwrap();
    trim_cursor.next_captured(&mut checkpoint).unwrap().unwrap();
    let checkpoint = checkpoint.retain_suffix_bounded(content, 2).unwrap();
    assert_eq!(checkpoint.len(), 1);
    assert_eq!(checkpoint.metrics().nodes_allocated, 0);
    assert_eq!(checkpoint.metrics().checkpoint_prefix_bytes_discarded, 2);
}

#[test]
fn randomized_edits_match_clean_string_and_preserve_tree_invariants() {
    let mut random = XorShift64(0x6c8e_9cf5_7a23_410d);
    let mut clean = "start αβ🙂\nsecond line\n尾".repeat(16);
    let mut source = PersistentSource::from_text(&clean);
    let replacements = ["", "x", "\n", "é", "🙂", "**value**", "中\nβ"];

    for _ in 0..750 {
        let boundaries = scalar_boundaries(&clean);
        let left = random.usize(boundaries.len());
        let right = random.usize(boundaries.len());
        let (start_index, end_index) = if left <= right {
            (left, right)
        } else {
            (right, left)
        };
        let range = boundaries[start_index]..boundaries[end_index];
        let replacement = replacements[random.usize(replacements.len())];
        let outcome = source.edit(range.clone(), replacement).unwrap();
        clean.replace_range(range, replacement);
        assert_eq!(outcome.metrics.copied_replacement_bytes, replacement.len());
        assert!(outcome.metrics.copied_existing_source_bytes <= 2 * MAX_PIECE_BYTES);
        assert_copy_ledger(
            outcome.metrics.allocations,
            replacement.len() + outcome.metrics.copied_existing_source_bytes,
        );
        assert!(outcome.metrics.result.max_piece_bytes <= MAX_PIECE_BYTES);
        outcome.source.validate().unwrap();
        assert_eq!(outcome.source.materialize(), clean);
        assert_eq!(cursor_bytes(&outcome.source), clean.as_bytes());
        source = outcome.source;
    }
}

#[test]
fn prefix_edit_keeps_suffix_byte_anchors_stable() {
    let text = "α header\nmiddle\nstable suffix 🙂\n";
    let source = PersistentSource::from_text(text);
    let suffix = text.find("stable suffix").unwrap();
    let before = (suffix..text.len())
        .map(|offset| source.anchor_at(offset).unwrap())
        .collect::<Vec<_>>();

    let replacement = "inserted 🦀\n";
    let outcome = source.edit(0..0, replacement).unwrap();
    let shifted = suffix + replacement.len();
    let after = (shifted..outcome.source.len_bytes())
        .map(|offset| outcome.source.anchor_at(offset).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(before, after);
    assert_eq!(outcome.provenance.suffix.old, 0..text.len());
    assert_eq!(
        outcome.provenance.suffix.new,
        replacement.len()..replacement.len() + text.len()
    );
    assert_eq!(outcome.provenance.map_old_byte(suffix), Some(shifted));
    assert_eq!(
        outcome.provenance.suffix.fragment().first_anchor(),
        source.anchor_at(0)
    );
    outcome.source.validate().unwrap();
}

#[test]
fn ten_mib_middle_edit_copies_only_replacement_and_allocates_logarithmically() {
    let text = "a".repeat(10 * 1024 * 1024);
    let source = PersistentSource::from_text(&text);
    source.validate().unwrap();
    let before_metrics = source.metrics();
    let middle = text.len() / 2;
    let suffix_probe = middle + MAX_PIECE_BYTES * 3;
    let old_anchor = source.anchor_at(suffix_probe).unwrap();

    let outcome = source.edit(middle..middle + 1, "Z").unwrap();
    outcome.source.validate().unwrap();
    let allocations = outcome.metrics.allocations;

    assert_eq!(outcome.source.len_bytes(), text.len());
    assert_eq!(outcome.source.byte_at(middle).unwrap().byte, b'Z');
    assert_eq!(outcome.source.anchor_at(suffix_probe), Some(old_anchor));
    assert_eq!(allocations.new_buffers, 1);
    assert_eq!(outcome.metrics.copied_replacement_bytes, 1);
    assert_eq!(
        outcome.metrics.copied_existing_source_bytes,
        MAX_PIECE_BYTES - 1
    );
    assert_copy_ledger(allocations, MAX_PIECE_BYTES);
    assert_eq!(
        outcome.metrics.unchanged_suffix_bytes,
        text.len() - middle - 1
    );
    assert_eq!(
        outcome.provenance.suffix.fragment().len_bytes(),
        text.len() - middle - MAX_PIECE_BYTES
    );

    // Two splits and two joins should allocate in proportion to tree depth,
    // not the 2,560-piece source. Keep generous rotation headroom so the test
    // asserts complexity shape rather than one balancing implementation.
    assert!(
        allocations.new_nodes <= before_metrics.depth * 16 + 16,
        "edit allocated {allocations:?} from {before_metrics:?}"
    );
    assert!(outcome.metrics.result.depth <= before_metrics.depth + 2);
    // The replacement and compacted suffix fragment refill one 4 KiB page.
    assert_eq!(outcome.metrics.result.pieces, before_metrics.pieces);
}

#[test]
fn edit_path_does_not_materialize_or_copy_the_old_document() {
    let text = "paragraph with utf8 🙂\n".repeat(200_000);
    let source = PersistentSource::from_text(&text);
    let start = text.len() / 3;
    let start = previous_boundary(&text, start);
    let end = next_boundary(&text, start + 7);
    let replacement = "small";
    let outcome = source.edit(start..end, replacement).unwrap();

    assert_eq!(outcome.metrics.copied_replacement_bytes, replacement.len());
    assert!(outcome.metrics.copied_existing_source_bytes <= 2 * MAX_PIECE_BYTES);
    assert_copy_ledger(
        outcome.metrics.allocations,
        replacement.len() + outcome.metrics.copied_existing_source_bytes,
    );
    assert!(
        outcome.metrics.allocations.copied_bytes * 500 < text.len(),
        "edit copied a document-sized payload: {:?}",
        outcome.metrics
    );
    assert_eq!(
        outcome.metrics.unchanged_prefix_bytes + outcome.metrics.unchanged_suffix_bytes,
        text.len() - (end - start)
    );
    outcome.source.validate().unwrap();
}

#[test]
fn source_root_identity_is_exact_monotonic_and_content_independent() {
    let first = PersistentSource::from_text("same");
    let clone = first.clone();
    let equal_content = PersistentSource::from_text("same");
    assert_eq!(first.identity(), clone.identity());
    assert_ne!(first.identity(), equal_content.identity());
    assert!(first.identity().0 < equal_content.identity().0);

    let no_op = first.edit(0..0, "").unwrap();
    assert_eq!(no_op.source.materialize(), first.materialize());
    assert_ne!(no_op.source.identity(), first.identity());
    assert!(equal_content.identity().0 < no_op.source.identity().0);
    assert_eq!(first.identity(), clone.identity());
}

#[test]
fn large_ingest_and_replacement_use_independent_buffers_and_delete_without_pinning() {
    const BYTES: usize = 10 * 1024 * 1024;
    let text = "a".repeat(BYTES);
    let (initial, initial_allocations) = PersistentSource::from_text_with_metrics(&text);
    let initial_retention = initial.buffer_retention();
    assert_copy_ledger(initial_allocations, BYTES);
    assert_eq!(
        initial_allocations.new_buffers,
        BYTES.div_ceil(MAX_PIECE_BYTES)
    );
    assert_eq!(initial_retention.retained_buffer_bytes, BYTES);
    assert_eq!(initial_retention.max_buffer_bytes, MAX_PIECE_BYTES);
    assert_eq!(
        initial_retention.unique_buffers,
        BYTES.div_ceil(MAX_PIECE_BYTES)
    );

    let deletion = initial.edit(1..BYTES, "").unwrap();
    drop(initial);
    let surviving = deletion.source.buffer_retention();
    assert_eq!(deletion.source.materialize(), "a");
    assert_eq!(surviving.unique_buffers, 1);
    assert_eq!(surviving.retained_buffer_bytes, 1);
    assert_eq!(surviving.referenced_piece_bytes, 1);
    assert_eq!(surviving.unreferenced_retained_bytes, 0);
    assert!(surviving.max_buffer_bytes <= MAX_PIECE_BYTES);

    let empty = PersistentSource::default();
    let replacement = empty.edit(0..0, &text).unwrap();
    let replacement_retention = replacement.source.buffer_retention();
    assert_eq!(replacement.metrics.copied_replacement_bytes, BYTES);
    assert_eq!(replacement.metrics.copied_existing_source_bytes, 0);
    assert_eq!(
        replacement.metrics.allocations.new_buffers,
        BYTES.div_ceil(MAX_PIECE_BYTES)
    );
    assert_eq!(replacement_retention.retained_buffer_bytes, BYTES);
    assert_eq!(replacement_retention.max_buffer_bytes, MAX_PIECE_BYTES);
}

#[test]
fn one_hundred_thousand_single_byte_edits_compact_boundary_buffers() {
    const EDITS: usize = 100_000;
    let mut source = PersistentSource::default();
    let mut max_new_nodes = 0;
    let mut max_copied_existing = 0;
    let mut stable_prefix_anchor = None;

    for edit_index in 0..EDITS {
        let offset = source.len_bytes();
        let outcome = source.edit(offset..offset, "x").unwrap();
        assert_eq!(outcome.metrics.copied_replacement_bytes, 1);
        assert!(outcome.metrics.copied_existing_source_bytes <= MAX_PIECE_BYTES);
        assert!(outcome.metrics.allocations.copied_bytes <= 2 * (MAX_PIECE_BYTES + 1));
        assert_copy_ledger(
            outcome.metrics.allocations,
            outcome.metrics.copied_existing_source_bytes + 1,
        );
        assert!(outcome.metrics.allocations.new_buffers <= 2);
        max_new_nodes = max_new_nodes.max(outcome.metrics.allocations.new_nodes);
        max_copied_existing = max_copied_existing.max(outcome.metrics.copied_existing_source_bytes);
        source = outcome.source;

        if edit_index == 10_000 {
            stable_prefix_anchor = source.anchor_at(0);
        }
    }

    source.validate().unwrap();
    assert_eq!(source.len_bytes(), EDITS);
    assert_eq!(source.anchor_at(0), stable_prefix_anchor);
    let expected_pages = EDITS.div_ceil(MAX_PIECE_BYTES);
    let tree = source.metrics();
    let retention = source.buffer_retention();
    assert!(tree.pieces <= expected_pages + 1, "{tree:?}");
    assert!(
        retention.unique_buffers <= expected_pages + 1,
        "{retention:?}"
    );
    assert_eq!(retention.retained_buffer_bytes, EDITS);
    assert_eq!(retention.unreferenced_retained_bytes, 0);
    assert!(retention.max_buffer_bytes <= MAX_PIECE_BYTES);
    assert!(max_new_nodes <= 128, "max new nodes was {max_new_nodes}");
    assert!(max_copied_existing <= MAX_PIECE_BYTES);
    eprintln!(
        "typing-stress edits={EDITS} pieces={} buffers={} retained_bytes={} max_new_nodes_per_edit={max_new_nodes} max_existing_copy_per_edit={max_copied_existing}",
        tree.pieces,
        retention.unique_buffers,
        retention.retained_buffer_bytes,
    );
}

fn scalar_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = text
        .char_indices()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    boundaries.push(text.len());
    boundaries
}

fn previous_boundary(text: &str, mut offset: usize) -> usize {
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn next_boundary(text: &str, mut offset: usize) -> usize {
    offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset += 1;
    }
    offset
}

struct XorShift64(u64);

impl XorShift64 {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn usize(&mut self, upper: usize) -> usize {
        let upper = u64::try_from(upper).expect("test collections fit in u64");
        usize::try_from(self.next() % upper).expect("modulo result fits in usize")
    }
}

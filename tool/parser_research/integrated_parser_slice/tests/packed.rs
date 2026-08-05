use flark_integrated_parser_slice::packed::{
    ExactComparisonPoll, PackedOrdinalStackBuilder, PackedOrdinalStackRoot, PackedPageBuilder,
    PackedPageSequence, PackedRecordSink, PACKED_PAGE_BYTES,
};

#[test]
fn fixed_page_builder_never_grows_or_partially_pushes() {
    let mut builder = PackedPageBuilder::new();
    assert!(builder.try_push_bytes(&vec![7; PACKED_PAGE_BYTES - 1]));
    assert!(!builder.try_push_varint(300));
    assert_eq!(builder.len(), PACKED_PAGE_BYTES - 1);
    assert!(builder.try_push_byte(9));
    assert!(!builder.try_push_byte(10));
    let page = builder.seal();
    assert_eq!(page.payload().len(), PACKED_PAGE_BYTES);
}

#[test]
fn persistent_page_sequence_splices_without_enumerating_pages() {
    let mut sequence = PackedPageSequence::default();
    for value in 0..10_000u64 {
        let mut builder = PackedPageBuilder::new();
        assert!(builder.try_push_varint(value));
        sequence = sequence.push_back(builder.seal());
    }
    assert_eq!(sequence.page_count(), 10_000);
    assert!(sequence.height() < 32);

    let (prefix, suffix) = sequence.split_pages(5_000);
    let replacement = {
        let mut builder = PackedPageBuilder::new();
        assert!(builder.try_push_byte(0xff));
        PackedPageSequence::from_page(builder.seal())
    };
    let edited = prefix.concat(&replacement).concat(&suffix);
    assert_eq!(edited.page_count(), 10_001);
    assert!(edited.height() < 32);
    assert_eq!(suffix.page_count(), 5_000);
}

fn one_byte_page(byte: u8) -> PackedPageSequence {
    let mut builder = PackedPageBuilder::new();
    assert!(builder.try_push_byte(byte));
    PackedPageSequence::from_page(builder.seal())
}

#[test]
fn sequence_digest_depends_on_bytes_not_tree_association() {
    let a = one_byte_page(b'a');
    let b = one_byte_page(b'b');
    let c = one_byte_page(b'c');
    let d = one_byte_page(b'd');
    let left_associated = a.concat(&b).concat(&c).concat(&d);
    let right_associated = a.concat(&b.concat(&c.concat(&d)));

    assert_eq!(left_associated.digest(), right_associated.digest());
    assert_eq!(left_associated.payload_bytes(), 4);
    assert_eq!(right_associated.payload_bytes(), 4);
    assert_eq!(left_associated.allocated_sequence_nodes(), 7);
    assert_eq!(right_associated.allocated_sequence_nodes(), 7);
    assert!(left_associated.accounted_structural_bytes() > 4);
}

fn stack(values: impl IntoIterator<Item = u64>) -> PackedOrdinalStackRoot {
    let mut builder = PackedOrdinalStackBuilder::from_root(&PackedOrdinalStackRoot::default());
    for value in values {
        builder.push(value);
    }
    builder.checkpoint()
}

#[test]
fn dense_monotonic_stack_uses_two_bytes_per_entry_and_branches_persistently() {
    let root = stack(0..1_000_000);
    assert_eq!(root.len(), 1_000_000);
    assert!(
        root.payload_bytes() <= 2_010_000,
        "{}",
        root.payload_bytes()
    );

    let mut branch = PackedOrdinalStackBuilder::from_root(&root);
    for expected in (999_990..1_000_000).rev() {
        assert_eq!(branch.pop(), Some(expected));
    }
    branch.push(1_000_001);
    let branch = branch.checkpoint();
    assert_eq!(root.len(), 1_000_000);
    assert_eq!(branch.len(), 999_991);
    assert_eq!(branch.top(), Some(1_000_001));
    let clean_branch = stack((0..999_990).chain([1_000_001]));
    assert_eq!(branch.digest(), clean_branch.digest());
    let mut comparison = branch.exact_comparison(&clean_branch);
    while comparison.poll(4096) == ExactComparisonPoll::Pending {}
    assert_eq!(comparison.poll(1), ExactComparisonPoll::Equal);
}

#[test]
fn checkpoints_compact_the_mutable_tail_instead_of_retaining_sparse_pages() {
    let mut root = PackedOrdinalStackRoot::default();
    let mut maximum_tail_copy = 0;
    for value in 0..10_000 {
        let mut builder = PackedOrdinalStackBuilder::from_root(&root);
        maximum_tail_copy = maximum_tail_copy.max(builder.tail_bytes_copied());
        builder.push(value);
        root = builder.checkpoint();
    }

    assert_eq!(root.len(), 10_000);
    assert!(root.payload_bytes() <= 20_100, "{}", root.payload_bytes());
    assert!(root.page_count() <= 5, "{}", root.page_count());
    assert!(maximum_tail_copy < PACKED_PAGE_BYTES);
    assert!(root.accounted_retained_bytes() < 22_000);
}

#[test]
fn first_stack_ordinal_may_be_u64_max() {
    let mut builder = PackedOrdinalStackBuilder::from_root(&PackedOrdinalStackRoot::default());
    builder.push(u64::MAX);
    let root = builder.checkpoint();
    assert_eq!(root.top(), Some(u64::MAX));

    let mut builder = PackedOrdinalStackBuilder::from_root(&root);
    assert_eq!(builder.pop(), Some(u64::MAX));
    assert!(builder.checkpoint().is_empty());
}

#[test]
fn exact_stack_comparison_is_fuelled_and_does_not_trust_digest_only() {
    let left = stack(0..20_000);
    let right = stack(0..20_000);
    assert!(!left.fast_identity_eq(&right));
    let mut comparison = left.exact_comparison(&right);
    let mut pending = 0;
    loop {
        match comparison.poll(31) {
            ExactComparisonPoll::Pending => pending += 1,
            ExactComparisonPoll::Equal => break,
            ExactComparisonPoll::NotEqual => panic!("equal stacks differed"),
        }
    }
    assert!(pending > 100);

    let different = stack((0..19_999).chain([21_000]));
    let mut comparison = right.exact_comparison(&different);
    assert_eq!(comparison.poll(1), ExactComparisonPoll::NotEqual);
}

#[test]
fn record_sink_seals_fixed_pages_and_keeps_dense_payload_compact() {
    let mut sink = PackedRecordSink::new();
    for index in 0..1_000_000u64 {
        sink.push(1, &[index & 0x7f]);
    }
    let root = sink.finish();
    assert_eq!(root.records(), 1_000_000);
    assert!(root.payload_bytes() <= 2_010_000);
    assert!(root.pages().page_count() > 1);

    let mut records = root.iter();
    assert_eq!(
        records
            .next()
            .map(flark_integrated_parser_slice::packed::PackedRecord::tag),
        Some(1)
    );
    assert_eq!(
        records.next().map(|record| record.fields().to_vec()),
        Some(vec![1])
    );
    assert_eq!(records.count(), 999_998);
}

#[test]
fn record_header_carries_arity_and_tag_across_page_boundaries() {
    let mut sink = PackedRecordSink::new();
    for index in 0..10_000u64 {
        match index % 3 {
            0 => sink.push(2, &[]),
            1 => sink.push(17, &[index]),
            _ => sink.push(255, &[index, u64::MAX - index, 0]),
        }
    }
    let root = sink.finish();
    let decoded: Vec<_> = root.iter().collect();
    assert_eq!(decoded.len(), 10_000);
    assert_eq!(decoded[0].tag(), 2);
    assert!(decoded[0].fields().is_empty());
    assert_eq!(decoded[1].tag(), 17);
    assert_eq!(decoded[1].fields(), &[1]);
    assert_eq!(decoded[2].tag(), 255);
    assert_eq!(decoded[2].fields(), &[2, u64::MAX - 2, 0]);
}

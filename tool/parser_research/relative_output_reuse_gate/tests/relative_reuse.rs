use std::sync::Arc;

use flark_integrated_parser_slice::crop_source::CropSnapshotLease;
use flark_relative_output_reuse_gate::{
    assert_detached_output_type, detach_crop_blocks, AllocationReceipt, BlockFact, BlockKind,
    LocalRange, OutputPage, OutputTree, PageId, Position, PropertyId, PropertyTable, PropertyValue,
    ReferenceOccurrence, ReferenceRole, SymbolId, SymbolTable, SymbolValue,
};

fn whole_page(
    id: u64,
    text: &str,
    property: Option<PropertyId>,
    references: Vec<ReferenceOccurrence>,
    receipt: &mut AllocationReceipt,
) -> Arc<OutputPage> {
    let end = Position::of(text);
    OutputPage::from_fragment(
        PageId(id),
        text,
        vec![BlockFact {
            kind: BlockKind::Paragraph,
            range: LocalRange {
                start: Position::default(),
                end,
            },
            container_depth: 0,
            list_property: property,
        }],
        references,
        receipt,
    )
    .unwrap()
}

#[test]
fn prefix_edit_reuses_suffix_page_and_subtree_and_derives_both_coordinate_metrics() {
    assert_detached_output_type();
    let mut build = AllocationReceipt::default();
    let pages = [
        whole_page(1, "a\n\n", None, Vec::new(), &mut build),
        whole_page(2, "second λ\n\n", None, Vec::new(), &mut build),
        whole_page(3, "third\n\n", None, Vec::new(), &mut build),
        whole_page(4, "fourth\n", None, Vec::new(), &mut build),
    ];
    let original = OutputTree::from_pages(&pages, &mut build);
    let original_suffix_root = original.right_partition().unwrap();
    let suffix_page = original.locate_page(2).unwrap().page;
    let original_fact = original.absolute_fact(2, 0).unwrap();

    let mut mutation = AllocationReceipt::default();
    let replacement = whole_page(5, "🦀 expanded prefix\n\n", None, Vec::new(), &mut mutation);
    let edited = original.replace_prefix_pages(&[replacement], &mut mutation);

    assert!(original_suffix_root.shares_root_with(&edited.right_partition().unwrap()));
    assert!(Arc::ptr_eq(
        &suffix_page,
        &edited.locate_page(2).unwrap().page
    ));
    assert_eq!(suffix_page.facts()[0].range.start, Position::default());

    let edited_fact = edited.absolute_fact(2, 0).unwrap();
    let old_prefix = Position::of("a\n\nsecond λ\n\n");
    let new_prefix = Position::of("🦀 expanded prefix\n\nsecond λ\n\n");
    assert_eq!(original_fact.range.start, old_prefix);
    assert_eq!(edited_fact.range.start, new_prefix);
    assert_ne!(
        edited_fact.range.start.byte - original_fact.range.start.byte,
        edited_fact.range.start.utf16 - original_fact.range.start.utf16,
        "astral UTF-8/UTF-16 shifts must not be conflated"
    );
    assert!(edited_fact.nodes_visited <= edited.height());

    assert_eq!(mutation.output_pages_allocated, 1);
    assert_eq!(mutation.fact_records_allocated, 1);
    assert_eq!(mutation.reference_records_allocated, 0);
    assert_eq!(mutation.leaf_nodes_allocated, 1);
    assert!(mutation.branch_nodes_allocated <= original.height());
    assert!(mutation.tree_nodes_visited <= original.height());
}

#[test]
fn general_prefix_insertion_is_logarithmic_and_keeps_a_large_suffix_root() {
    const PAGE_COUNT: usize = 65_536;
    let mut build = AllocationReceipt::default();
    let mut pages = Vec::with_capacity(PAGE_COUNT);
    for index in 0..PAGE_COUNT {
        pages.push(
            OutputPage::from_metrics(
                PageId(index as u64),
                Position { byte: 1, utf16: 1 },
                vec![BlockFact {
                    kind: BlockKind::Paragraph,
                    range: LocalRange {
                        start: Position::default(),
                        end: Position { byte: 1, utf16: 1 },
                    },
                    container_depth: 0,
                    list_property: None,
                }],
                Vec::new(),
                &mut build,
            )
            .unwrap(),
        );
    }
    let original = OutputTree::from_pages(&pages, &mut build);
    assert_eq!(original.height(), 17);
    let large_suffix_root = original.right_partition().unwrap();
    assert_eq!(large_suffix_root.page_count(), PAGE_COUNT / 2);
    let suffix_probe = original.locate_page(60_000).unwrap().page;

    let mut mutation = AllocationReceipt::default();
    let inserted = whole_page(100_000, "🦀\n", None, Vec::new(), &mut mutation);
    let edited = original.splice_pages(0..0, &[inserted], &mut mutation);

    assert_eq!(edited.page_count(), PAGE_COUNT + 1);
    assert!(large_suffix_root.shares_root_with(&edited.right_partition().unwrap()));
    assert!(Arc::ptr_eq(
        &suffix_probe,
        &edited.locate_page(60_001).unwrap().page
    ));
    assert_eq!(
        edited.absolute_fact(60_001, 0).unwrap().range.start,
        Position {
            byte: 60_005,
            utf16: 60_003,
        }
    );

    assert_eq!(mutation.output_pages_allocated, 1);
    assert_eq!(mutation.fact_records_allocated, 1);
    assert_eq!(mutation.leaf_nodes_allocated, 1);
    assert_eq!(mutation.reference_records_allocated, 0);
    assert!(mutation.tree_nodes_allocated() <= 4 * original.height() + 1);
    assert!(mutation.tree_nodes_visited <= 3 * original.height() + 4);
}

#[test]
fn crop_parse_detaches_revision_and_clean_parse_confirms_relative_suffix_reuse() {
    let old_text = "old λ\r\n\r\nsecond 🦀\r\n\r\nthird tail";
    let old_source = CropSnapshotLease::from_text(old_text);
    let (old_output, old_receipt) = detach_crop_blocks(Arc::clone(&old_source)).unwrap();
    assert_eq!(old_receipt.parsed_block_leaves, 3);
    assert_eq!(old_receipt.retained_strong_source_leases, 0);
    assert_eq!(old_receipt.retained_weak_source_leases, 0);
    assert_eq!(Arc::strong_count(&old_source), 1);
    assert_eq!(Arc::weak_count(&old_source), 0);

    let edit_end = old_text.find("\r\n\r\n").unwrap();
    let (new_source, provenance) = old_source
        .edit(0..edit_end, "new expanded 🦀 prefix")
        .unwrap();
    assert_eq!(provenance.suffix.old.start, edit_end);
    let (clean_output, clean_receipt) = detach_crop_blocks(Arc::clone(&new_source)).unwrap();
    assert_eq!(clean_receipt.retained_strong_source_leases, 0);
    assert_eq!(clean_receipt.retained_weak_source_leases, 0);
    assert_eq!(clean_output.page_count(), old_output.page_count());

    let old_suffix_root = old_output.right_partition().unwrap();
    let old_suffix_page = old_output.locate_page(1).unwrap().page;
    let clean_prefix_page = clean_output.locate_page(0).unwrap().page;
    let mut mutation = AllocationReceipt::default();
    let candidate = old_output.replace_prefix_pages(&[clean_prefix_page], &mut mutation);

    assert!(candidate.semantically_eq(&clean_output));
    assert!(old_suffix_root.shares_root_with(&candidate.right_partition().unwrap()));
    assert!(Arc::ptr_eq(
        &old_suffix_page,
        &candidate.locate_page(1).unwrap().page
    ));
    assert_eq!(mutation.output_pages_allocated, 0);
    assert_eq!(mutation.fact_records_allocated, 0);
    assert!(mutation.tree_nodes_allocated() <= old_output.height());
    assert_eq!(
        candidate.absolute_fact(2, 0).unwrap().range,
        clean_output.absolute_fact(2, 0).unwrap().range
    );

    let old_observer = Arc::downgrade(&old_source);
    drop(old_source);
    assert!(old_observer.upgrade().is_none());
    assert_eq!(
        candidate.absolute_fact(1, 0).unwrap().kind,
        BlockKind::Paragraph
    );

    let new_observer = Arc::downgrade(&new_source);
    drop(new_source);
    assert!(new_observer.upgrade().is_none());
    assert!(candidate.semantically_eq(&clean_output));
}

#[test]
fn reference_values_and_list_tightness_are_indirect_properties_not_page_payload() {
    let symbol = SymbolId(7);
    let property = PropertyId(11);
    let mut allocations = AllocationReceipt::default();
    let page = whole_page(
        1,
        "- [label]\n",
        Some(property),
        vec![ReferenceOccurrence {
            symbol,
            local: Position { byte: 3, utf16: 3 },
            role: ReferenceRole::Consumer,
        }],
        &mut allocations,
    );
    let output = OutputTree::from_pages(&[Arc::clone(&page)], &mut allocations);
    let unchanged_output = output.clone();

    let symbols = SymbolTable::default().with_value(
        symbol,
        SymbolValue {
            destination: Arc::from("/one"),
            title: None,
            presence_generation: 1,
        },
    );
    let symbols = symbols.with_value(
        symbol,
        SymbolValue {
            destination: Arc::from("/two"),
            title: Some(Arc::from("changed winner value")),
            presence_generation: 1,
        },
    );
    let properties = PropertyTable::default()
        .with_value(property, PropertyValue::ListTight(true))
        .with_value(property, PropertyValue::ListTight(false));

    assert!(output.shares_root_with(&unchanged_output));
    assert!(Arc::ptr_eq(&page, &output.locate_page(0).unwrap().page));
    assert_eq!(output.summary().references.occurrences, 1);
    assert_ne!(output.summary().references.digest, 0);
    assert_eq!(page.facts()[0].list_property, Some(property));
    assert_eq!(symbols.value(symbol).unwrap().destination.as_ref(), "/two");
    assert_eq!(symbols.value(symbol).unwrap().presence_generation, 1);
    assert_eq!(
        properties.value(property),
        Some(PropertyValue::ListTight(false))
    );
}

#[test]
fn repeated_prefix_edits_remain_logarithmic_without_suffix_rebase() {
    const PAGE_COUNT: usize = 1024;
    let mut build = AllocationReceipt::default();
    let mut pages = Vec::with_capacity(PAGE_COUNT);
    pages.push(whole_page(0, "start\n", None, Vec::new(), &mut build));
    for index in 1..PAGE_COUNT {
        pages.push(whole_page(index as u64, "x", None, Vec::new(), &mut build));
    }
    let mut output = OutputTree::from_pages(&pages, &mut build);
    let permanent_suffix_root = output.right_partition().unwrap();
    let permanent_last_page = output.locate_page(PAGE_COUNT - 1).unwrap().page;

    for edit in 0..128_u64 {
        let text = if edit % 2 == 0 {
            format!("🦀-{edit}\n")
        } else {
            format!("λ-{edit}\n")
        };
        let mut mutation = AllocationReceipt::default();
        let replacement = whole_page(10_000 + edit, &text, None, Vec::new(), &mut mutation);
        output = output.replace_prefix_pages(&[replacement], &mut mutation);
        assert!(permanent_suffix_root.shares_root_with(&output.right_partition().unwrap()));
        assert!(Arc::ptr_eq(
            &permanent_last_page,
            &output.locate_page(PAGE_COUNT - 1).unwrap().page
        ));
        assert_eq!(mutation.output_pages_allocated, 1);
        assert_eq!(mutation.fact_records_allocated, 1);
        assert_eq!(mutation.leaf_nodes_allocated, 1);
        assert!(mutation.branch_nodes_allocated <= output.height());
        assert!(mutation.tree_nodes_visited <= output.height());
        assert_eq!(
            output.absolute_fact(PAGE_COUNT - 1, 0).unwrap().range.start,
            Position {
                byte: text.len() + PAGE_COUNT - 2,
                utf16: text.encode_utf16().count() + PAGE_COUNT - 2,
            }
        );
    }
}

#[test]
fn deterministic_mixed_splices_match_a_flat_oracle_without_losing_balance() {
    let mut build = AllocationReceipt::default();
    let mut oracle = Vec::new();
    for index in 0..257_u64 {
        oracle.push(
            OutputPage::from_metrics(
                PageId(index),
                Position {
                    byte: 1 + index as usize % 7,
                    utf16: 1 + index as usize % 3,
                },
                vec![BlockFact {
                    kind: BlockKind::Paragraph,
                    range: LocalRange {
                        start: Position::default(),
                        end: Position {
                            byte: 1 + index as usize % 7,
                            utf16: 1 + index as usize % 3,
                        },
                    },
                    container_depth: 0,
                    list_property: None,
                }],
                Vec::new(),
                &mut build,
            )
            .unwrap(),
        );
    }
    let mut output = OutputTree::from_pages(&oracle, &mut build);
    let mut seed = 0x6a09_e667_f3bc_c909_u64;
    let mut next_id = 10_000_u64;

    for step in 0..1000 {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let start = (seed as usize) % (oracle.len() + 1);
        seed = seed.rotate_left(29).wrapping_add(step as u64);
        let deleted = ((seed as usize) % 5).min(oracle.len() - start);
        seed = seed.rotate_left(31).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let inserted = (seed as usize) % 5;

        let previous_height = output.height().max(1);
        let mut mutation = AllocationReceipt::default();
        let mut replacements = Vec::with_capacity(inserted);
        for _ in 0..inserted {
            seed = seed.rotate_left(17).wrapping_add(next_id);
            let byte = 1 + seed as usize % 11;
            let utf16 = 1 + seed as usize % byte;
            replacements.push(
                OutputPage::from_metrics(
                    PageId(next_id),
                    Position { byte, utf16 },
                    vec![BlockFact {
                        kind: BlockKind::Paragraph,
                        range: LocalRange {
                            start: Position::default(),
                            end: Position { byte, utf16 },
                        },
                        container_depth: 0,
                        list_property: None,
                    }],
                    Vec::new(),
                    &mut mutation,
                )
                .unwrap(),
            );
            next_id += 1;
        }

        output = output.splice_pages(start..start + deleted, &replacements, &mut mutation);
        oracle.splice(start..start + deleted, replacements);
        assert_eq!(mutation.output_pages_allocated, inserted);
        assert_eq!(mutation.fact_records_allocated, inserted);
        assert!(
            mutation.tree_nodes_allocated() <= 12 * previous_height + 2 * inserted + 8,
            "step={step} receipt={mutation:?} previous_height={previous_height}"
        );
        assert_eq!(output.page_count(), oracle.len());

        let logarithmic_height_bound = if oracle.is_empty() {
            0
        } else {
            2 * (usize::BITS as usize - oracle.len().leading_zeros() as usize) + 2
        };
        assert!(
            output.height() <= logarithmic_height_bound,
            "step={step} len={} height={} bound={logarithmic_height_bound}",
            oracle.len(),
            output.height()
        );

        if step % 37 == 0 && !oracle.is_empty() {
            let expected_coverage =
                oracle
                    .iter()
                    .fold(Position::default(), |total, page| Position {
                        byte: total.byte + page.coverage().byte,
                        utf16: total.utf16 + page.coverage().utf16,
                    });
            assert_eq!(output.summary().coverage, expected_coverage);
            assert!(output
                .pages()
                .zip(&oracle)
                .all(|(actual, expected)| Arc::ptr_eq(&actual, expected)));

            let probe = (seed as usize) % oracle.len();
            let expected_prefix =
                oracle[..probe]
                    .iter()
                    .fold(Position::default(), |total, page| Position {
                        byte: total.byte + page.coverage().byte,
                        utf16: total.utf16 + page.coverage().utf16,
                    });
            let location = output.locate_page(probe).unwrap();
            assert_eq!(location.prefix, expected_prefix);
            assert!(Arc::ptr_eq(&location.page, &oracle[probe]));
            assert!(location.nodes_visited <= output.height());
        }
    }
}

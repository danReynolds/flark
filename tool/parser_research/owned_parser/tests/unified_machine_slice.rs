use flark_owned_parser_trial::{ContainerShape, LeafShape, TableAlignment, UnifiedSliceDocument};

fn shapes(document: &UnifiedSliceDocument) -> Vec<(String, Vec<ContainerShape>, LeafShape, bool)> {
    document
        .chunks()
        .into_iter()
        .map(|chunk| {
            (
                chunk.text,
                chunk.path.into_iter().map(|(_, shape)| shape).collect(),
                chunk.leaf_shape,
                chunk.continues_leaf,
            )
        })
        .collect()
}

fn quote_depth(path: &[ContainerShape]) -> usize {
    path.iter()
        .filter(|shape| matches!(shape, ContainerShape::Quote))
        .count()
}

fn list_depth(path: &[ContainerShape]) -> usize {
    path.iter()
        .filter(|shape| {
            matches!(
                shape,
                ContainerShape::BulletList { .. } | ContainerShape::OrderedList { .. }
            )
        })
        .count()
}

#[test]
fn commonmark_232_and_250_keep_lazy_lines_in_authoritative_ancestry() {
    let case_232 = UnifiedSliceDocument::new("> # Foo\n> bar\nbaz\n");
    let chunks = shapes(&case_232);
    assert_eq!(chunks[0].0, "Foo");
    assert_eq!(chunks[0].2, LeafShape::Heading(1));
    assert_eq!(quote_depth(&chunks[0].1), 1);
    assert_eq!(chunks[1].0, "bar");
    assert_eq!(chunks[2].0, "baz");
    assert_eq!(quote_depth(&chunks[2].1), 1);
    assert!(chunks[2].3);
    assert_eq!(case_232.chunks()[1].leaf_id, case_232.chunks()[2].leaf_id);

    let case_250 = UnifiedSliceDocument::new("> > > foo\nbar\n");
    let chunks = shapes(&case_250);
    assert_eq!(quote_depth(&chunks[0].1), 3);
    assert_eq!(quote_depth(&chunks[1].1), 3);
    assert!(chunks[1].3);
    case_250.assert_clean_oracle();
}

#[test]
fn commonmark_242_blank_line_closes_unmarked_quote_path() {
    let document = UnifiedSliceDocument::new("> foo\n\n> bar\n");
    let chunks = shapes(&document);
    assert_eq!(quote_depth(&chunks[0].1), 1);
    assert_eq!(chunks[1].2, LeafShape::Blank);
    assert_eq!(quote_depth(&chunks[1].1), 0);
    assert_eq!(quote_depth(&chunks[2].1), 1);
    assert_ne!(
        document.chunks()[0].path[0].0,
        document.chunks()[2].path[0].0
    );
}

#[test]
fn commonmark_259_carries_nested_quote_list_state_and_emits_looseness_fact() {
    let document = UnifiedSliceDocument::new("   > > 1.  one\n>>\n>>     two\n");
    let chunks = document.chunks();
    for chunk in &chunks {
        let path = chunk
            .path
            .iter()
            .map(|(_, shape)| shape.clone())
            .collect::<Vec<_>>();
        assert_eq!(quote_depth(&path), 2, "{chunk:?}");
        assert_eq!(list_depth(&path), 1, "{chunk:?}");
    }
    assert_eq!(chunks[0].text, "one");
    assert_eq!(chunks[1].leaf_shape, LeafShape::Blank);
    assert_eq!(chunks[2].text, "two");
    assert_ne!(chunks[0].leaf_id, chunks[2].leaf_id);
    let list_id = chunks[0]
        .path
        .iter()
        .find(|(_, shape)| matches!(shape, ContainerShape::OrderedList { .. }))
        .unwrap()
        .0;
    assert!(document.is_list_loose(list_id));
    document.assert_clean_oracle();
}

#[test]
fn commonmark_294_builds_four_nested_list_paths_without_recursive_reparse() {
    let document = UnifiedSliceDocument::new("- foo\n  - bar\n    - baz\n      - boo\n");
    let chunks = document.chunks();
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<Vec<_>>(),
        ["foo", "bar", "baz", "boo"]
    );
    let depths = chunks
        .iter()
        .map(|chunk| {
            list_depth(
                &chunk
                    .path
                    .iter()
                    .map(|(_, shape)| shape.clone())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(depths, [1, 2, 3, 4]);
    document.assert_clean_oracle();
}

#[test]
fn edit_inside_nested_list_reuses_suffix_and_preserves_relative_chunk_ranges() {
    let source = "- top\n  - child\n  - sibling\n\nafter\n\ntail\n";
    let mut document = UnifiedSliceDocument::new(source);
    let before = document.chunks();
    let sibling_before = before.iter().find(|chunk| chunk.text == "sibling").unwrap();
    let tail_before = before.iter().find(|chunk| chunk.text == "tail").unwrap();
    let child = source.find("child").unwrap();
    let delta = document.apply_edit(child, child + 5, "CHILD-LONGER");
    document.assert_clean_oracle();

    let after = document.chunks();
    let sibling_after = after.iter().find(|chunk| chunk.text == "sibling").unwrap();
    let tail_after = after.iter().find(|chunk| chunk.text == "tail").unwrap();
    assert_eq!(sibling_before.id, sibling_after.id);
    assert_eq!(tail_before.id, tail_after.id);
    assert_eq!(tail_after.source.start, tail_before.source.start + 7);
    assert!(delta.reparsed_lines <= 2, "{delta:?}");
    assert_eq!(delta.removed_chunk_ids.len(), 1, "{delta:?}");
    assert_eq!(delta.inserted_chunk_ids.len(), 1, "{delta:?}");
}

#[test]
fn editing_container_opener_adopts_old_container_ids_at_convergence() {
    let source = "- top\n  - child\n  - sibling\n\nafter\n";
    let mut document = UnifiedSliceDocument::new(source);
    let before = document.chunks();
    let old_list_id = before[0].path[0].0;
    let child_id = before[1].id;
    let top = source.find("top").unwrap();
    let delta = document.apply_edit(top, top + 3, "TOP!");
    document.assert_clean_oracle();

    let after = document.chunks();
    assert_eq!(after[0].path[0].0, old_list_id);
    assert_eq!(after[1].id, child_id);
    assert!(delta.reparsed_lines <= 2, "{delta:?}");
    assert_eq!(delta.removed_chunk_ids.len(), 1, "{delta:?}");
}

#[test]
fn one_megabyte_list_edit_is_parser_local_inside_the_container() {
    let mut source = String::new();
    let mut edit = 0;
    for index in 0..70_000 {
        if index == 35_000 {
            edit = source.len() + 2;
        }
        source.push_str(&format!("- item {index:05}\n"));
    }
    let mut document = UnifiedSliceDocument::new(&source);
    let started = std::time::Instant::now();
    let delta = document.apply_edit(edit, edit + 4, "EDIT");
    let elapsed = started.elapsed();
    document.assert_clean_oracle();
    eprintln!(
        "UNIFIED_LIST bytes={} apply_us={} reparsed_lines={} reused_chunks={}",
        source.len(),
        elapsed.as_micros(),
        delta.reparsed_lines,
        delta.reused_chunks
    );
    assert!(source.len() >= 900_000);
    assert!(delta.reparsed_lines <= 2, "{delta:?}");
    assert!(delta.reused_chunks >= 69_999, "{delta:?}");
}

#[test]
fn setext_transition_promotes_existing_source_chunk_and_carries_restart_digest() {
    let document = UnifiedSliceDocument::new("title\n=====\nafter\n");
    let chunks = document.chunks();
    assert_eq!(chunks[0].text, "title");
    assert_eq!(chunks[0].leaf_shape, LeafShape::Heading(1));
    assert_eq!(chunks[1].text, "");
    assert_eq!(chunks[1].leaf_shape, LeafShape::Heading(1));
    assert!(chunks[1].continues_leaf);
    assert_eq!(chunks[0].leaf_id, chunks[1].leaf_id);
    assert_eq!(chunks[1].markers.len(), 1);
    assert_eq!(chunks[1].markers[0], 6..11);
    assert_eq!(chunks[2].leaf_shape, LeafShape::Paragraph);
    document.assert_clean_oracle();
}

#[test]
fn setext_validity_toggle_reuses_header_and_suffix_ids_without_stale_promotion() {
    let source = "title\n=====\n\nafter\n";
    let mut document = UnifiedSliceDocument::new(source);
    let before = document.chunks();
    let title_chunk_id = before[0].id;
    let title_leaf_id = before[0].leaf_id;
    let after_chunk_id = before[3].id;

    let marker_middle = source.find("=====").unwrap() + 2;
    let invalid = document.apply_edit(marker_middle, marker_middle + 1, "x");
    document.assert_clean_oracle();
    let chunks = document.chunks();
    assert_eq!(chunks[0].id, title_chunk_id);
    assert_eq!(chunks[0].leaf_id, title_leaf_id);
    assert_eq!(chunks[0].leaf_shape, LeafShape::Paragraph);
    assert_eq!(chunks[1].leaf_shape, LeafShape::Paragraph);
    assert_eq!(chunks[3].id, after_chunk_id);
    assert!(invalid.reparsed_lines <= 3, "{invalid:?}");

    let valid = document.apply_edit(marker_middle, marker_middle + 1, "=");
    document.assert_clean_oracle();
    let chunks = document.chunks();
    assert_eq!(chunks[0].id, title_chunk_id);
    assert_eq!(chunks[0].leaf_id, title_leaf_id);
    assert_eq!(chunks[0].leaf_shape, LeafShape::Heading(1));
    assert_eq!(chunks[3].id, after_chunk_id);
    assert!(valid.reparsed_lines <= 3, "{valid:?}");
}

#[test]
fn editing_setext_text_cannot_converge_on_unchanged_underline() {
    let source = "title\n=====\nafter\n";
    let mut document = UnifiedSliceDocument::new(source);
    let after_id = document.chunks()[2].id;
    let delta = document.apply_edit(0, "title".len(), "changed");
    document.assert_clean_oracle();
    let chunks = document.chunks();
    assert_eq!(chunks[0].text, "changed");
    assert_eq!(chunks[0].leaf_shape, LeafShape::Heading(1));
    assert_eq!(chunks[2].id, after_id);
    assert!(delta.reparsed_lines >= 3, "{delta:?}");
}

#[test]
fn bounded_gfm_table_transition_emits_header_cells_alignment_and_body_rows() {
    let source = "| a | b |\n| :- | -: |\n| c | d |\n\nafter\n";
    let document = UnifiedSliceDocument::new(source);
    let chunks = document.chunks();
    assert_eq!(chunks[0].leaf_shape, LeafShape::Table { columns: 2 });
    assert_eq!(
        chunks[0]
            .cells
            .iter()
            .map(|range| &source[range.clone()])
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(
        chunks[0].alignments,
        [TableAlignment::Left, TableAlignment::Right]
    );
    assert_eq!(
        chunks[1].leaf_shape,
        LeafShape::TableDelimiter { columns: 2 }
    );
    assert_eq!(chunks[2].leaf_shape, LeafShape::TableRow { columns: 2 });
    assert_eq!(chunks[0].leaf_id, chunks[1].leaf_id);
    assert_eq!(chunks[1].leaf_id, chunks[2].leaf_id);
    assert_eq!(chunks[3].leaf_shape, LeafShape::Blank);
    assert_eq!(chunks[4].leaf_shape, LeafShape::Paragraph);
    document.assert_clean_oracle();
}

#[test]
fn table_delimiter_toggle_reinterprets_following_rows_and_preserves_suffix_id() {
    let source = "| a | b |\n| -- | -- |\n| c | d |\n\nafter\n";
    let mut document = UnifiedSliceDocument::new(source);
    let before = document.chunks();
    let header_chunk_id = before[0].id;
    let header_leaf_id = before[0].leaf_id;
    let after_chunk_id = before[4].id;
    let edit = source.find("--").unwrap();

    let invalid = document.apply_edit(edit, edit + 1, "x");
    document.assert_clean_oracle();
    let chunks = document.chunks();
    assert_eq!(chunks[0].id, header_chunk_id);
    assert_eq!(chunks[0].leaf_id, header_leaf_id);
    assert_eq!(chunks[0].leaf_shape, LeafShape::Paragraph);
    assert_eq!(chunks[1].leaf_shape, LeafShape::Paragraph);
    assert_eq!(chunks[2].leaf_shape, LeafShape::Paragraph);
    assert_eq!(chunks[4].id, after_chunk_id);
    assert!(invalid.reparsed_lines <= 4, "{invalid:?}");

    let valid = document.apply_edit(edit, edit + 1, "-");
    document.assert_clean_oracle();
    let chunks = document.chunks();
    assert_eq!(chunks[0].id, header_chunk_id);
    assert_eq!(chunks[0].leaf_id, header_leaf_id);
    assert_eq!(chunks[0].leaf_shape, LeafShape::Table { columns: 2 });
    assert_eq!(chunks[2].leaf_shape, LeafShape::TableRow { columns: 2 });
    assert_eq!(chunks[4].id, after_chunk_id);
    assert!(valid.reparsed_lines <= 4, "{valid:?}");
}

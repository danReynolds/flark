use comrak::block_spine_facade::{
    html_block_end, html_block_start, reference_definitions, table_delimiter_alignments, table_row,
    FacadeAlignment, FacadeError, MAX_CLASSIFICATION_BYTES,
};
use comrak::nodes::{Node, NodeValue};
use comrak::{parse_document, Arena, Options};

fn gfm() -> Options<'static> {
    let mut options = Options::default();
    options.extension.autolink = true;
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.tagfilter = true;
    options.extension.tasklist = true;
    options
}

fn first_list(root: Node<'_>) -> Node<'_> {
    root.descendants()
        .find(|node| matches!(node.data().value, NodeValue::List(_)))
        .expect("fixture has list")
}

fn list_is_tight(markdown: &str) -> bool {
    let arena = Arena::new();
    let root = parse_document(&arena, markdown, &gfm());
    let tight = match first_list(root).data().value {
        NodeValue::List(list) => list.tight,
        _ => unreachable!(),
    };
    tight
}

#[test]
fn list_tightness_depends_on_closed_prefix_and_future_siblings() {
    assert!(list_is_tight("- first\n- second\n"));
    assert!(!list_is_tight("- first\n\n- second\n"));
    assert!(!list_is_tight("- first\n\n  continuation\n- second\n"));

    // A checkpoint that retains only the open final item cannot distinguish
    // these prefixes. Exact restart state needs a list-prefix looseness fold.
    let old_prefix = "- first\n\n";
    let new_prefix = "- first\n";
    let suffix = "- second\n";
    assert_ne!(
        list_is_tight(&(old_prefix.to_owned() + suffix)),
        list_is_tight(&(new_prefix.to_owned() + suffix)),
    );
}

#[test]
fn table_lexer_is_reusable_but_table_activation_is_not_leaf_local() {
    let header = table_row("| a\\|b | c |\n", false)
        .expect("bounded call")
        .expect("table row");
    assert_eq!(header.cells.len(), 2);
    assert_eq!(header.cells[0].content, "a|b");
    assert!(header.cells[0].had_escaped_pipe);

    let alignments = table_delimiter_alignments("| :--- | ---: |\n", false)
        .expect("bounded call")
        .expect("delimiter row");
    assert_eq!(alignments, [FacadeAlignment::Left, FacadeAlignment::Right]);

    // The same delimiter-looking line is a table only when it can promote a
    // preceding paragraph with the same number of cells.
    let options = gfm();
    let arena = Arena::new();
    let table = parse_document(&arena, "a | b\n-- | --\n", &options);
    assert!(table
        .descendants()
        .any(|node| matches!(node.data().value, NodeValue::Table(_))));
    let arena = Arena::new();
    let paragraph = parse_document(&arena, "a | b | c\n-- | --\n", &options);
    assert!(!paragraph
        .descendants()
        .any(|node| matches!(node.data().value, NodeValue::Table(_))));
}

#[test]
fn html_classes_need_persistent_terminator_state() {
    assert_eq!(html_block_start("<script>open\n", true).unwrap(), Some(1));
    assert_eq!(html_block_start("<!-- open\n", true).unwrap(), Some(2));
    assert_eq!(html_block_start("<?open\n", true).unwrap(), Some(3));
    assert_eq!(html_block_start("<!A\n", true).unwrap(), Some(4));
    assert_eq!(html_block_start("<![CDATA[open\n", true).unwrap(), Some(5));
    assert_eq!(html_block_start("<table>\n", true).unwrap(), Some(6));
    assert_eq!(html_block_start("<x attr=v>\n", true).unwrap(), Some(7));
    assert_eq!(html_block_start("<x attr=v>\n", false).unwrap(), None);

    assert!(!html_block_end(1, "body\n").unwrap());
    assert!(html_block_end(1, "body</script>suffix\n").unwrap());
    assert!(html_block_end(2, "body-->suffix\n").unwrap());
    assert!(html_block_end(3, "body?>suffix\n").unwrap());
    assert!(html_block_end(4, "body>suffix\n").unwrap());
    assert!(html_block_end(5, "body]]>suffix\n").unwrap());
    assert!(!html_block_end(6, "anything\n").unwrap());
    assert!(!html_block_end(7, "anything\n").unwrap());
}

#[test]
fn reference_lexer_emits_ordered_occurrences_not_document_authority() {
    let definitions =
        reference_definitions("[same]: /first \"one\"\n[same]: /second\n[other]: /third\nbody\n")
            .expect("bounded call");
    assert_eq!(definitions.len(), 3);
    assert_eq!(definitions[0].normalized_label, "same");
    assert_eq!(definitions[0].resolved.url, "/first");
    assert_eq!(definitions[1].normalized_label, "same");
    assert_eq!(definitions[2].normalized_label, "other");
    // First-definition-wins and dependent-leaf invalidation remain document
    // state owned by the caller, not by this reusable scanner seam.
}

#[test]
fn bounded_facade_fails_closed_on_every_oversized_lexical_shape() {
    let huge = "a".repeat(MAX_CLASSIFICATION_BYTES + 1);
    for error in [
        html_block_start(&huge, true).unwrap_err(),
        table_row(&huge, false).unwrap_err(),
        reference_definitions(&huge).unwrap_err(),
    ] {
        assert_eq!(
            error,
            FacadeError::OverCap {
                bytes: MAX_CLASSIFICATION_BYTES + 1,
                cap: MAX_CLASSIFICATION_BYTES,
            }
        );
    }
}

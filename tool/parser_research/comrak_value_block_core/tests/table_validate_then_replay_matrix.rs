use std::sync::Arc;

use flark_comrak_value_block_core::render::normalized_html;
use flark_comrak_value_block_core::{BlockKind, SyntaxProfile, parse_document};
use flark_oversized_block_line_gate::{
    CancellationToken, TableHeaderDisposition, TableHeaderPassOneJob, TableHeaderPassOnePoll,
    TableHeaderRejectReason,
};

#[derive(Debug, PartialEq, Eq)]
struct Binding(u64);

fn classify(header: &[u8], delimiter: &[u8]) -> TableHeaderDisposition<Binding> {
    let cancellation = CancellationToken::default();
    let mut job = TableHeaderPassOneJob::new(Binding(1), Arc::from(header), Arc::from(delimiter));
    loop {
        match job.poll(1, &cancellation) {
            TableHeaderPassOnePoll::Pending { inspected } => assert!(inspected <= 1),
            TableHeaderPassOnePoll::Complete { value, inspected } => {
                assert!(inspected <= 1);
                return value;
            }
            TableHeaderPassOnePoll::Cancelled { .. } => panic!("uncancelled scan cancelled"),
        }
    }
}

fn root_kinds(source: &str) -> Vec<BlockKind> {
    let document = parse_document(source, SyntaxProfile::Gfm).expect("GFM parse");
    document
        .tree
        .node(document.tree.root)
        .children
        .iter()
        .map(|child| document.tree.node(*child).kind.clone())
        .collect()
}

#[test]
fn successful_table_preface_stays_literal_and_never_installs_a_reference() {
    let source = "[x]: /u\na | b\n--- | ---\n\n[x]\n";
    let document = parse_document(source, SyntaxProfile::Gfm).expect("GFM Table parse");
    assert!(matches!(
        classify(b"a | b\n", b"--- | ---\n"),
        TableHeaderDisposition::Ready(_)
    ));
    assert!(document.references.is_empty());

    let root = document.tree.node(document.tree.root);
    assert_eq!(root.children.len(), 3);
    assert!(matches!(
        document.tree.node(root.children[0]).kind,
        BlockKind::Paragraph
    ));
    assert!(matches!(
        document.tree.node(root.children[1]).kind,
        BlockKind::Table(_)
    ));
    assert!(matches!(
        document.tree.node(root.children[2]).kind,
        BlockKind::Paragraph
    ));

    let html = normalized_html(&document).expect("render");
    assert!(html.contains("<p>[x]: /u</p>"));
    assert!(html.ends_with("<p>[x]</p>\n"));
    assert!(!html.contains("href=\"/u\""));
}

#[test]
fn count_mismatch_rejects_table_then_ordinary_finalization_installs_the_reference() {
    let source = "[x]: /u\nh | c\n--- | --- | ---\n\n[x]\n";
    assert!(matches!(
        classify(b"h | c\n", b"--- | --- | ---\n"),
        TableHeaderDisposition::Rejected {
            reason: TableHeaderRejectReason::ColumnCountMismatch,
            ..
        }
    ));

    let document = parse_document(source, SyntaxProfile::Gfm).expect("rejected Table parse");
    assert_eq!(document.references.len(), 1);
    assert!(
        document
            .tree
            .nodes
            .iter()
            .all(|node| !matches!(node.kind, BlockKind::Table(_)))
    );
    let html = normalized_html(&document).expect("render");
    assert!(html.contains("<a href=\"/u\">x</a>"));
}

#[test]
fn setext_list_and_thematic_precedence_remain_ahead_of_table_activation() {
    let setext = root_kinds("title\n---\n");
    assert!(matches!(
        setext.as_slice(),
        [BlockKind::Heading {
            setext: true,
            level: 2,
            ..
        }]
    ));

    let list = root_kinds("a | b\n- | -\n");
    assert!(matches!(list.first(), Some(BlockKind::Paragraph)));
    assert!(matches!(list.get(1), Some(BlockKind::List(_))));
    assert!(list.iter().all(|kind| !matches!(kind, BlockKind::Table(_))));

    let thematic = root_kinds("---\n");
    assert!(matches!(thematic.as_slice(), [BlockKind::ThematicBreak]));

    let table = root_kinds("a | b\n--- | ---\n");
    assert!(matches!(table.as_slice(), [BlockKind::Table(_)]));
}

#[test]
fn noncandidate_is_retryable_but_one_rejection_sets_table_visited_permanently() {
    let retryable = parse_document(
        "first | header\nordinary words\nnext | header\n--- | ---\n",
        SyntaxProfile::Gfm,
    )
    .expect("retryable Table parse");
    assert!(
        retryable
            .tree
            .nodes
            .iter()
            .any(|node| matches!(node.kind, BlockKind::Table(_)))
    );

    let rejected = parse_document(
        "first | header\n--- | --- | ---\nnext | header\n--- | ---\n",
        SyntaxProfile::Gfm,
    )
    .expect("table_visited parse");
    assert!(
        rejected
            .tree
            .nodes
            .iter()
            .all(|node| !matches!(node.kind, BlockKind::Table(_)))
    );
    assert!(
        rejected
            .tree
            .nodes
            .iter()
            .any(|node| matches!(node.kind, BlockKind::Paragraph) && node.table_visited)
    );
}

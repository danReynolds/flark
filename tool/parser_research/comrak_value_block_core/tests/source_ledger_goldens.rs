use std::ops::Range;

use comrak::block_spine_facade::chop_trailing_hashes;
use flark_comrak_value_block_core::{
    BlockDocument, BlockKind, NodeId, OriginTransform, SyntaxProfile, parse_document,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GoldenPart {
    Gap,
    Content,
    BlockMarker,
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GoldenLogicalAction {
    None,
    HiddenUpstream,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GoldenClaim {
    owner: NodeId,
    part: GoldenPart,
    logical: GoldenLogicalAction,
    source: Range<usize>,
}

fn one_node(document: &BlockDocument, predicate: impl Fn(&BlockKind) -> bool) -> NodeId {
    let matches = document
        .tree
        .nodes
        .iter()
        .filter(|node| node.parent.is_some() && predicate(&node.kind))
        .map(|node| node.id)
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "expected one matching live node");
    matches[0]
}

fn content_end(text: &str) -> usize {
    if text.ends_with("\r\n") {
        text.len() - 2
    } else if text.ends_with(['\r', '\n']) {
        text.len() - 1
    } else {
        text.len()
    }
}

fn terminator_range(absolute_start: usize, text: &str) -> Range<usize> {
    absolute_start + content_end(text)..absolute_start + text.len()
}

#[test]
fn generic_stripped_indent_is_a_gap_of_the_surviving_nonterminal_parent() {
    struct Case {
        source: &'static str,
        marker_end: usize,
        content_start: usize,
        expected_parent: fn(&BlockKind) -> bool,
    }

    let cases = [
        Case {
            source: "alpha\n   beta\n",
            marker_end: 0,
            content_start: 3,
            expected_parent: |kind| matches!(kind, BlockKind::Document),
        },
        Case {
            source: "> alpha\n>   beta\n",
            // The quote prefix consumes `> `; the two additional spaces are
            // the generic accepts-lines indentation under test.
            marker_end: 2,
            content_start: 4,
            expected_parent: |kind| matches!(kind, BlockKind::BlockQuote),
        },
    ];

    for case in cases {
        let document =
            parse_document(case.source, SyntaxProfile::CommonMark).expect("generic-indent fixture");
        let paragraph = one_node(&document, |kind| matches!(kind, BlockKind::Paragraph));
        let parent = document
            .tree
            .parent(paragraph)
            .expect("paragraph has structural parent");
        assert!(
            (case.expected_parent)(&document.tree.node(parent).kind),
            "unexpected surviving parent for {:?}",
            case.source
        );
        assert_eq!(
            document.tree.node(paragraph).content.logical,
            "alpha\nbeta\n"
        );

        let second = &document.source.leaves[1];
        let second_origin = document
            .tree
            .node(paragraph)
            .content
            .origins
            .iter()
            .find(|origin| {
                origin
                    .source
                    .as_ref()
                    .is_some_and(|range| range.leaf_id == second.id)
            })
            .expect("second physical line contributes paragraph content");
        let source = second_origin.source.as_ref().expect("source-backed origin");
        assert_eq!(source.start as usize, case.content_start);
        assert_eq!(second_origin.transform, OriginTransform::Identity);

        let claim = GoldenClaim {
            owner: parent,
            part: GoldenPart::Gap,
            logical: GoldenLogicalAction::None,
            source: second.absolute_start + case.marker_end
                ..second.absolute_start + case.content_start,
        };
        assert_eq!(
            &case.source[claim.source.clone()],
            &second.text[case.marker_end..case.content_start]
        );
        assert!(
            case.source[claim.source.clone()]
                .bytes()
                .all(|byte| matches!(byte, b' ' | b'\t'))
        );
        assert_eq!(claim.part, GoldenPart::Gap);
        assert_eq!(claim.logical, GoldenLogicalAction::None);
        assert_eq!(claim.owner, parent);
    }
}

#[test]
fn atx_tail_golden_distinguishes_hidden_whitespace_from_a_closing_marker() {
    struct Case {
        source: &'static str,
        expected_logical: &'static str,
        closed: bool,
        tail_part: GoldenPart,
        tail_logical: GoldenLogicalAction,
    }

    let cases = [
        Case {
            source: "# alpha   \r\n",
            expected_logical: "alpha",
            closed: false,
            tail_part: GoldenPart::Content,
            tail_logical: GoldenLogicalAction::HiddenUpstream,
        },
        Case {
            source: "# alpha#   \n",
            expected_logical: "alpha#",
            closed: false,
            tail_part: GoldenPart::Content,
            tail_logical: GoldenLogicalAction::HiddenUpstream,
        },
        Case {
            source: "# alpha ###   \r\n",
            expected_logical: "alpha",
            closed: true,
            tail_part: GoldenPart::BlockMarker,
            tail_logical: GoldenLogicalAction::None,
        },
    ];

    for case in cases {
        let (chopped, donor_closed) = chop_trailing_hashes(case.source).expect("ATX tail");
        assert_eq!(donor_closed, case.closed);
        let document =
            parse_document(case.source, SyntaxProfile::CommonMark).expect("ATX heading fixture");
        let heading = one_node(&document, |kind| matches!(kind, BlockKind::Heading { .. }));
        let node = document.tree.node(heading);
        let BlockKind::Heading { closed, .. } = node.kind else {
            unreachable!("selected heading")
        };
        assert_eq!(closed, case.closed);
        assert_eq!(node.content.logical, case.expected_logical);
        let origin = node
            .content
            .origins
            .first()
            .expect("visible heading origin");
        let source = origin
            .source
            .as_ref()
            .expect("source-backed heading origin");
        assert_eq!(source.start, 2);
        assert_eq!(source.end as usize, chopped.len());
        assert_eq!(origin.transform, OriginTransform::Identity);

        let line_content_end = content_end(case.source);
        let tail = GoldenClaim {
            owner: heading,
            part: case.tail_part,
            logical: case.tail_logical,
            source: chopped.len()..line_content_end,
        };
        assert!(!tail.source.is_empty());
        assert_eq!(tail.part, case.tail_part);
        assert_eq!(tail.logical, case.tail_logical);

        let terminator = GoldenClaim {
            owner: heading,
            part: GoldenPart::Terminal,
            logical: GoldenLogicalAction::None,
            source: line_content_end..case.source.len(),
        };
        assert!(matches!(
            &case.source[terminator.source.clone()],
            "\n" | "\r" | "\r\n"
        ));
    }
}

#[test]
fn split_table_preface_only_occurs_under_paragraph_capable_parents_in_pinned_profile() {
    #[derive(Clone, Copy)]
    enum ParentKind {
        Document,
        Quote,
        Item,
    }

    impl ParentKind {
        fn matches(self, kind: &BlockKind) -> bool {
            match self {
                Self::Document => matches!(kind, BlockKind::Document),
                Self::Quote => matches!(kind, BlockKind::BlockQuote),
                Self::Item => matches!(kind, BlockKind::Item(_)),
            }
        }
    }

    let cases = [
        (
            ParentKind::Document,
            "preface\n| a | b |\n",
            "preface\n| a | b |\n| --- | --- |\n",
        ),
        (
            ParentKind::Quote,
            "> preface\n> | a | b |\n",
            "> preface\n> | a | b |\n> | --- | --- |\n",
        ),
        (
            ParentKind::Item,
            "- preface\n  | a | b |\n",
            "- preface\n  | a | b |\n  | --- | --- |\n",
        ),
    ];

    for (expected_parent, before_delimiter, complete) in cases {
        let before = parse_document(before_delimiter, SyntaxProfile::Gfm)
            .expect("open split-preface paragraph");
        let original = one_node(&before, |kind| matches!(kind, BlockKind::Paragraph));
        let original_parent = before
            .tree
            .parent(original)
            .expect("original paragraph parent");
        let original_parent_kind = &before.tree.node(original_parent).kind;
        assert!(expected_parent.matches(original_parent_kind));
        assert!(
            original_parent_kind.can_contain(&BlockKind::Paragraph),
            "the parser can only open the source paragraph below a paragraph-capable parent"
        );

        let after = parse_document(complete, SyntaxProfile::Gfm).expect("split GFM table");
        let table = one_node(&after, |kind| matches!(kind, BlockKind::Table(_)));
        let table_parent = after.tree.parent(table).expect("table parent");
        assert!(expected_parent.matches(&after.tree.node(table_parent).kind));
        let siblings = &after.tree.node(table_parent).children;
        let table_index = siblings
            .iter()
            .position(|candidate| *candidate == table)
            .expect("table is a child");
        assert!(table_index > 0, "split table retains a visible preface");
        let preface = siblings[table_index - 1];
        assert!(matches!(
            after.tree.node(preface).kind,
            BlockKind::Paragraph
        ));
        assert_eq!(after.tree.node(preface).content.logical, "preface");
        assert!(
            after
                .tree
                .node(table_parent)
                .kind
                .can_contain(&BlockKind::Paragraph)
        );
    }
}

#[test]
fn table_line_terminators_follow_their_structural_boundary_owner() {
    for ending in ["\n", "\r\n", "\r"] {
        let source = format!("| h1 | h2 |{ending}| --- | --- |{ending}| c1 | c2 |{ending}");
        let document = parse_document(&source, SyntaxProfile::Gfm).expect("GFM table fixture");
        let table = one_node(&document, |kind| matches!(kind, BlockKind::Table(_)));
        let rows = document
            .tree
            .node(table)
            .children
            .iter()
            .copied()
            .filter(|row| matches!(document.tree.node(*row).kind, BlockKind::TableRow { .. }))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2, "header and one body row");
        let header = rows[0];
        let body = rows[1];
        assert!(matches!(
            document.tree.node(header).kind,
            BlockKind::TableRow { header: true }
        ));
        assert!(matches!(
            document.tree.node(body).kind,
            BlockKind::TableRow { header: false }
        ));
        assert_eq!(document.tree.node(header).source_start.line, 1);
        assert_eq!(document.tree.node(header).source_end.line, 1);
        assert_eq!(document.tree.node(body).source_start.line, 3);
        assert_eq!(document.tree.node(body).source_end.line, 3);
        assert!(
            rows.iter().all(|row| {
                let node = document.tree.node(*row);
                node.source_start.line != 2 && node.source_end.line != 2
            }),
            "the delimiter is table syntax, not a row"
        );

        let leaves = &document.source.leaves;
        assert_eq!(leaves.len(), 3);
        let claims = [
            GoldenClaim {
                owner: header,
                part: GoldenPart::Terminal,
                logical: GoldenLogicalAction::None,
                source: terminator_range(leaves[0].absolute_start, &leaves[0].text),
            },
            GoldenClaim {
                owner: table,
                part: GoldenPart::Terminal,
                logical: GoldenLogicalAction::None,
                source: terminator_range(leaves[1].absolute_start, &leaves[1].text),
            },
            GoldenClaim {
                owner: body,
                part: GoldenPart::Terminal,
                logical: GoldenLogicalAction::None,
                source: terminator_range(leaves[2].absolute_start, &leaves[2].text),
            },
        ];
        assert_eq!(&source[claims[0].source.clone()], ending);
        assert_eq!(&source[claims[1].source.clone()], ending);
        assert_eq!(&source[claims[2].source.clone()], ending);
        assert!(
            claims
                .windows(2)
                .all(|pair| pair[0].source.end < pair[1].source.start)
        );
        assert!(claims.iter().all(|claim| {
            claim.part == GoldenPart::Terminal && claim.logical == GoldenLogicalAction::None
        }));
        assert_eq!(claims[0].owner, header);
        assert_eq!(claims[1].owner, table);
        assert_eq!(claims[2].owner, body);
    }
}

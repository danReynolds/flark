use std::fs;
use std::path::{Path, PathBuf};

use comrak::block_spine_facade::oracle_block_projection;
use flark_comrak_value_block_core::render::bounded_code_info;
use flark_comrak_value_block_core::source::LogicalProjection;
use flark_comrak_value_block_core::{
    Alignment, BlockDocument, BlockKind, NodeId, OriginTransform, SyntaxProfile, parse_document,
};
use flark_gate_a_harness::{FixtureAuthority, load_gate_a_fixtures};
use serde::Deserialize;

#[derive(Deserialize)]
struct CorpusFixture {
    markdown: String,
    example: usize,
    section: String,
}

#[derive(Debug, PartialEq, Eq)]
struct Projection {
    parent: Option<usize>,
    kind: String,
    source: [usize; 4],
    logical: String,
    line_offsets: Vec<usize>,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn load_corpus(path: &Path) -> Vec<CorpusFixture> {
    serde_json::from_slice(&fs::read(path).expect("read corpus")).expect("decode corpus")
}

#[test]
fn gate_a_block_structure_source_parentage_and_logical_leaves_match_donor() {
    let fixtures = load_gate_a_fixtures(&repo_root()).expect("Gate A fixture corpus");
    for gate in fixtures {
        let gfm = gate.authority == FixtureAuthority::Gfm029;
        let document = parse_document(
            &gate.fixture.markdown,
            if gfm {
                SyntaxProfile::Gfm
            } else {
                SyntaxProfile::CommonMark
            },
        )
        .expect("candidate parse");
        assert_origin_integrity(&document);
        let actual = candidate_projection(&document);
        let expected = oracle_block_projection(&gate.fixture.markdown, gfm)
            .into_iter()
            .map(|record| Projection {
                parent: record.parent,
                kind: record.kind,
                source: record.source,
                logical: record.logical,
                line_offsets: record.line_offsets,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual, expected,
            "{:?} example {} ({})",
            gate.authority, gate.fixture.example, gate.fixture.section
        );
    }
}

#[test]
fn full_corpus_block_structure_source_parentage_logical_leaves_and_origins_match_donor() {
    let root = repo_root();
    let fixtures = [
        (
            root.join("test/fixtures/commonmark/upstream/common_mark_tests.json"),
            SyntaxProfile::CommonMark,
        ),
        (
            root.join("test/fixtures/commonmark/upstream/gfm_tests.json"),
            SyntaxProfile::Gfm,
        ),
    ];
    let mut compared = 0;
    let mut failures = Vec::new();
    for (path, profile) in fixtures {
        for fixture in load_corpus(&path) {
            let document = parse_document(&fixture.markdown, profile).unwrap_or_else(|error| {
                panic!(
                    "candidate parse failed for {} example {}: {error:?}",
                    fixture.section, fixture.example
                )
            });
            assert_origin_integrity(&document);
            let mut actual = candidate_projection(&document);
            let mut expected =
                oracle_block_projection(&fixture.markdown, profile == SyntaxProfile::Gfm)
                    .into_iter()
                    .map(|record| Projection {
                        parent: record.parent,
                        kind: record.kind,
                        source: record.source,
                        logical: record.logical,
                        line_offsets: record.line_offsets,
                    })
                    .collect::<Vec<_>>();
            // `line_offsets` is Comrak parser scratch, not an output fact. In
            // particular, Comrak deliberately leaves zero entries behind for
            // physical reference-definition lines removed from a paragraph.
            // This value core maps surviving logical bytes through exact
            // coverage-relative origin runs instead; requiring the redundant
            // donor vector would couple persistence to an irrelevant internal.
            for projection in &mut actual {
                projection.line_offsets.clear();
            }
            for projection in &mut expected {
                projection.line_offsets.clear();
            }
            if actual != expected && failures.len() < 40 {
                failures.push(format!(
                    "{} example {}:\nactual: {actual:#?}\nexpected: {expected:#?}",
                    fixture.section, fixture.example
                ));
            }
            compared += 1;
        }
    }
    assert_eq!(compared, 1_322);
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn reference_definition_prefix_removal_keeps_survivors_on_exact_coverage_leaves() {
    let cases = [
        ("[\nfoo\n]: /url\nbar\n", vec![("bar\n", vec![4_u64])]),
        (
            "[foo]: /url\n\"title\" ok\n",
            vec![("\"title\" ok\n", vec![2_u64])],
        ),
        (
            "[foo]: /url\nbar\n===\n[foo]\n",
            vec![("bar\n", vec![2_u64]), ("[foo]\n", vec![4_u64])],
        ),
        (
            "[foo]: /url\n===\n[foo]\n",
            vec![("===\n[foo]\n", vec![2_u64, 3_u64])],
        ),
    ];
    for (source, expected) in cases {
        let document = parse_document(source, SyntaxProfile::CommonMark).expect("candidate parse");
        assert_origin_integrity(&document);
        let mut actual = Vec::new();
        for node in &document.tree.nodes {
            if node.kind.contains_inlines() && !node.content.logical.is_empty() {
                let leaf_ids = node
                    .content
                    .origins
                    .iter()
                    .filter_map(|origin| origin.source.as_ref().map(|range| range.leaf_id))
                    .collect::<Vec<_>>();
                actual.push((node.content.logical.as_str(), leaf_ids));
            }
        }
        assert_eq!(actual, expected, "{source:?}");
    }
}

fn candidate_projection(document: &BlockDocument) -> Vec<Projection> {
    let mut ids = Vec::new();
    collect_preorder(document, document.tree.root, &mut ids);
    ids.iter()
        .map(|id| {
            let node = document.tree.node(*id);
            Projection {
                parent: node
                    .parent
                    .and_then(|parent| ids.iter().position(|candidate| *candidate == parent)),
                kind: candidate_kind(document, *id),
                source: [
                    node.source_start.line,
                    node.source_start.column,
                    node.source_end.line,
                    node.source_end.column,
                ],
                logical: if matches!(
                    node.kind,
                    BlockKind::CodeBlock { .. } | BlockKind::HtmlBlock { .. }
                ) {
                    // Comrak moves raw block content into its literal field at
                    // finalization. That literal is already part of `kind`;
                    // the candidate deliberately retains the source-backed
                    // builder as an additional origin receipt.
                    String::new()
                } else {
                    node.content.logical.clone()
                },
                line_offsets: node.content.line_offsets.clone(),
            }
        })
        .collect()
}

fn collect_preorder(document: &BlockDocument, node: NodeId, output: &mut Vec<NodeId>) {
    output.push(node);
    for child in &document.tree.node(node).children {
        collect_preorder(document, *child, output);
    }
}

fn candidate_kind(document: &BlockDocument, node: NodeId) -> String {
    match &document.tree.node(node).kind {
        BlockKind::Document => "document".to_owned(),
        BlockKind::BlockQuote => "block_quote".to_owned(),
        BlockKind::List(list) => format!(
            "list:{:?}:{}:{:?}:{}:{}:{}:{}",
            list.list_type,
            list.start,
            list.delimiter,
            list.bullet_char,
            list.marker_offset,
            list.padding,
            list.tight
        ),
        BlockKind::Item(list) => format!(
            "item:{:?}:{}:{:?}:{}:{}:{}",
            list.list_type,
            list.start,
            list.delimiter,
            list.bullet_char,
            list.marker_offset,
            list.padding
        ),
        BlockKind::CodeBlock {
            fenced,
            fence_char,
            fence_length,
            fence_offset,
            info,
            literal,
            ..
        } => {
            let info = bounded_code_info(document, node, *info)
                .expect("bounded code info")
                .value;
            let literal = document
                .materialize_projection(node, *literal)
                .expect("code literal projection");
            format!("code:{fenced}:{fence_char}:{fence_length}:{fence_offset}:{info:?}:{literal:?}")
        }
        BlockKind::HtmlBlock {
            block_type,
            literal,
        } => {
            let literal = document
                .materialize_projection(node, *literal)
                .expect("HTML literal projection");
            format!("html:{block_type}:{literal:?}")
        }
        BlockKind::Paragraph => "paragraph".to_owned(),
        BlockKind::Heading {
            level,
            setext,
            closed,
        } => format!("heading:{level}:{setext}:{closed}"),
        BlockKind::ThematicBreak => "thematic_break".to_owned(),
        BlockKind::Table(table) => format!(
            "table:{:?}:{}:{}:{}",
            table
                .alignments
                .iter()
                .map(|alignment| match alignment {
                    Alignment::None => "none",
                    Alignment::Left => "left",
                    Alignment::Center => "center",
                    Alignment::Right => "right",
                })
                .collect::<Vec<_>>(),
            table.num_columns,
            table.num_rows,
            table.num_nonempty_cells
        ),
        BlockKind::TableRow { header } => format!("table_row:{header}"),
        BlockKind::TableCell => "table_cell".to_owned(),
    }
}

fn assert_origin_integrity(document: &BlockDocument) {
    for node in &document.tree.nodes {
        let logical = if node.content.is_source_backed() {
            document
                .materialize_projection(
                    node.id,
                    LogicalProjection::new(
                        0,
                        u32::try_from(node.content.logical_len()).expect("logical below u32"),
                    ),
                )
                .expect("raw source-backed projection")
        } else {
            node.content.logical.clone()
        };
        let mut cursor = 0_u32;
        for origin in &node.content.origins {
            assert_eq!(
                origin.logical_start, cursor,
                "origin gap at node {:?}",
                node.id
            );
            cursor += origin.logical_len;
            if let Some(source) = &origin.source {
                let leaf = document
                    .source
                    .leaves
                    .iter()
                    .find(|leaf| leaf.id == source.leaf_id)
                    .expect("origin leaf exists");
                assert!((source.end as usize) <= leaf.text.len());
                assert!((source.start as usize) <= (source.end as usize));
                if origin.transform == OriginTransform::Identity {
                    let logical_start = origin.logical_start as usize;
                    let logical_end = logical_start + origin.logical_len as usize;
                    assert_eq!(
                        &logical[logical_start..logical_end],
                        &leaf.text[source.start as usize..source.end as usize]
                    );
                }
            }
        }
        assert_eq!(cursor as usize, node.content.logical_len());
        assert_eq!(logical.len(), node.content.logical_len());
    }
}

#[test]
fn crlf_lone_cr_and_duplicate_reference_interactions_keep_relative_origins() {
    let source = "[x]: /first\r\n[x]: /second\rFoo\r\n---\r\n\r\n| [x] | b |\r\n| --- | --- |\r\n| [x] | c |\r";
    let document = parse_document(source, SyntaxProfile::Gfm).expect("candidate parse");
    assert_origin_integrity(&document);
    assert_eq!(document.references.len(), 2);
    assert_eq!(document.references[0].url, "/first");
    assert_eq!(document.references[1].url, "/second");
    assert!(
        document
            .source
            .leaves
            .iter()
            .any(|leaf| leaf.text.ends_with("\r\n"))
    );
    assert!(
        document
            .source
            .leaves
            .iter()
            .any(|leaf| leaf.text.ends_with('\r') && !leaf.text.ends_with("\r\n"))
    );
}

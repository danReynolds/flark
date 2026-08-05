use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use comrak::{Arena, Options, format_html, parse_document as parse_comrak_document};
use flark_canonical_range_gate::{
    CanonicalCoverageKind, donor_extent, parse_canonical, validate_canonical,
};
use flark_comrak_value_block_core::{BlockKind, SyntaxProfile, normalized_html, parse_document};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
struct CorpusFixture {
    markdown: String,
    example: usize,
    section: String,
}

fn oracle_html(source: &str, profile: SyntaxProfile) -> String {
    let arena = Arena::new();
    let mut options = Options::default();
    options.render.r#unsafe = true;
    if profile == SyntaxProfile::Gfm {
        options.extension.table = true;
        options.extension.strikethrough = true;
        options.extension.autolink = true;
        options.extension.tagfilter = true;
        options.extension.tasklist = true;
    }
    let root = parse_comrak_document(&arena, source, &options);
    let mut html = String::new();
    format_html(root, &options, &mut html).expect("oracle HTML");
    html
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn load(path: &Path) -> Vec<CorpusFixture> {
    serde_json::from_slice(&fs::read(path).expect("read corpus")).expect("decode corpus")
}

fn donor_rows(
    source: &str,
    profile: SyntaxProfile,
) -> Vec<(Option<usize>, BlockKind, std::ops::Range<usize>)> {
    let document = parse_document(source, profile).expect("clean parse");
    let mut ids = Vec::new();
    let mut stack = vec![document.tree.root];
    while let Some(node) = stack.pop() {
        ids.push(node);
        stack.extend(document.tree.node(node).children.iter().rev().copied());
    }
    ids.iter()
        .map(|node| {
            let value = document.tree.node(*node);
            (
                value
                    .parent
                    .and_then(|parent| ids.iter().position(|candidate| *candidate == parent)),
                value.kind.clone(),
                donor_extent(
                    source,
                    [
                        value.source_start.line,
                        value.source_start.column,
                        value.source_end.line,
                        value.source_end.column,
                    ],
                ),
            )
        })
        .collect()
}

#[test]
fn fixtures_255_and_257_expose_donor_chronology_but_have_one_canonical_rule() {
    let cases = [
        (255, "- one\n\n two\n", "- one"),
        (257, " -    one\n\n     two\n", "-    one"),
    ];
    for (number, source, expected_item) in cases {
        let canonical = parse_canonical(source, SyntaxProfile::CommonMark).expect("canonical");
        validate_canonical(source, &canonical).expect("valid canonical ranges");
        let list = canonical
            .nodes
            .iter()
            .find(|node| matches!(node.kind, BlockKind::List(_)))
            .expect("list");
        let item = canonical
            .nodes
            .iter()
            .find(|node| matches!(node.kind, BlockKind::Item(_)))
            .expect("item");
        assert_eq!(
            &source[list.source.clone()],
            expected_item,
            "fixture {number}"
        );
        assert_eq!(
            &source[item.source.clone()],
            expected_item,
            "fixture {number}"
        );
        assert!(
            list.source.start <= item.source.start && item.source.end <= list.source.end,
            "fixture {number} canonical containment"
        );
        assert_eq!(canonical.ignored_repair_events, 1);

        let blank_start = source.find("\n\n").expect("blank line") + 1;
        let blank = canonical
            .coverage
            .iter()
            .find(|segment| segment.source.start <= blank_start && blank_start < segment.source.end)
            .expect("blank coverage");
        assert_eq!(blank.kind, CanonicalCoverageKind::Gap);
        assert_eq!(
            blank.owner, 0,
            "trailing inter-block blank belongs to root gap"
        );
    }

    let donor_255 = donor_rows("- one\n\n two\n", SyntaxProfile::CommonMark);
    let donor_257 = donor_rows(" -    one\n\n     two\n", SyntaxProfile::CommonMark);
    let donor_255_list = &donor_255[1].2;
    let donor_255_item = &donor_255[2].2;
    assert!(
        donor_255_list.start <= donor_255_item.start && donor_255_item.end <= donor_255_list.end
    );
    let donor_257_list = &donor_257[1].2;
    let donor_257_item = &donor_257[2].2;
    assert!(
        donor_257_item.end > donor_257_list.end,
        "Comrak 257 child item must demonstrate the containment failure"
    );
}

fn assert_unicode_crlf_coordinates() {
    let source = "🎨\r\n- α\n\n  > β\r\n";
    let document = parse_canonical(source, SyntaxProfile::CommonMark).expect("unicode canonical");
    validate_canonical(source, &document).expect("unicode canonical ranges");
    assert_eq!(document.source_utf16_len, source.encode_utf16().count());
    assert_eq!(
        document
            .coverage
            .last()
            .expect("Unicode coverage")
            .utf16
            .end,
        source.encode_utf16().count()
    );
    let paragraphs = document
        .nodes
        .iter()
        .filter(|node| matches!(node.kind, BlockKind::Paragraph))
        .collect::<Vec<_>>();
    assert_eq!(&source[paragraphs[0].source.clone()], "🎨");
    assert_eq!(
        &source[paragraphs.last().expect("nested paragraph").source.clone()],
        "β"
    );
}

#[test]
fn focused_promotion_detach_nested_list_and_table_ranges_are_product_coherent() {
    let cases = [
        (
            SyntaxProfile::CommonMark,
            "- a\n\n  - b\n\n    c\n- d\n",
            "nested-list",
        ),
        (
            SyntaxProfile::CommonMark,
            "[foo]: /url\n\nbar\n===\n\nafter\n",
            "reference-detach-setext",
        ),
        (
            SyntaxProfile::Gfm,
            "before\n\n| a | b |\n| :- | -: |\n| c | d |\n\nafter\n",
            "table-promotion",
        ),
    ];
    for (profile, source, label) in cases {
        let canonical = parse_canonical(source, profile).expect("canonical");
        validate_canonical(source, &canonical).unwrap_or_else(|error| panic!("{label}: {error}"));
    }

    assert_unicode_crlf_coordinates();

    let nested_source = "- a\n\n  - b\n\n    c\n- d\n";
    let nested = parse_canonical(nested_source, SyntaxProfile::CommonMark).expect("nested");
    let first_internal_blank = nested_source.find("\n\n").expect("blank") + 1;
    let internal_blank = nested
        .coverage
        .iter()
        .find(|segment| {
            segment.source.start <= first_internal_blank
                && first_internal_blank < segment.source.end
        })
        .expect("internal blank coverage");
    assert_eq!(internal_blank.kind, CanonicalCoverageKind::Gap);
    assert_ne!(
        internal_blank.owner, 0,
        "a blank inside a continuing list item stays owned by that container"
    );

    let detached_source = "[foo]: /url\n\nbar\n===\n\nafter\n";
    let detached = parse_canonical(detached_source, SyntaxProfile::CommonMark).expect("detach");
    assert!(detached.detached_nodes >= 1);
    let heading = detached
        .nodes
        .iter()
        .find(|node| matches!(node.kind, BlockKind::Heading { setext: true, .. }))
        .expect("setext heading");
    assert_eq!(&detached_source[heading.source.clone()], "bar\n===");
    let definition = detached
        .coverage
        .iter()
        .find(|segment| segment.source.start == 0)
        .expect("definition coverage");
    assert_eq!(definition.kind, CanonicalCoverageKind::Gap);
    assert_eq!(definition.owner, 0);

    let table_source = "before\n\n| a | b |\n| :- | -: |\n| c | d |\n\nafter\n";
    let table = parse_canonical(table_source, SyntaxProfile::Gfm).expect("table");
    let table_node = table
        .nodes
        .iter()
        .find(|node| matches!(node.kind, BlockKind::Table(_)))
        .expect("table node");
    for child in table
        .nodes
        .iter()
        .filter(|node| node.parent == Some(table_node.handle))
    {
        assert!(table_node.source.start <= child.source.start);
        assert!(child.source.end <= table_node.source.end);
    }
    let first_pipe = table_source.find('|').expect("table pipe");
    let pipe = table
        .coverage
        .iter()
        .find(|segment| segment.source.contains(&first_pipe))
        .expect("table pipe coverage");
    assert_eq!(pipe.kind, CanonicalCoverageKind::ContainerMarker);
    assert_ne!(pipe.owner, 0);
}

#[test]
#[allow(clippy::too_many_lines)] // One corpus loop keeps every aggregate receipt in one audit.
fn full_commonmark_and_gfm_corpora_keep_exact_tree_html_and_canonical_range_invariants() {
    let root = repo_root();
    let corpora = [
        (
            root.join("test/fixtures/commonmark/upstream/common_mark_tests.json"),
            SyntaxProfile::CommonMark,
            "commonmark",
        ),
        (
            root.join("test/fixtures/commonmark/upstream/gfm_tests.json"),
            SyntaxProfile::Gfm,
            "gfm",
        ),
    ];
    let mut fixture_count = 0;
    let mut node_count = 0;
    let mut donor_range_deltas = 0;
    let mut non_document_range_deltas = 0;
    let mut repair_scope_range_deltas = 0;
    let mut donor_parent_containment_failures = 0;
    let mut repair_events = 0;
    let mut detached_nodes = 0;
    let mut delta_by_kind = BTreeMap::<String, usize>::new();
    let mut containment_samples = Vec::new();
    let mut delta_samples = Vec::new();

    for (path, profile, corpus) in corpora {
        for fixture in load(&path) {
            let clean = parse_document(&fixture.markdown, profile).unwrap_or_else(|error| {
                panic!(
                    "{corpus} {} {} parse failed: {error:?}",
                    fixture.example, fixture.section
                )
            });
            // Canonicalization is output-only: exact grammar/tree rendering is
            // still the same clean parser result.
            let clean_html = normalized_html(&clean).unwrap_or_else(|error| {
                panic!(
                    "{corpus} {} {} HTML failed: {error:?}",
                    fixture.example, fixture.section
                )
            });
            assert_eq!(
                clean_html,
                oracle_html(&fixture.markdown, profile),
                "{corpus} {} {} normative HTML",
                fixture.example,
                fixture.section
            );
            let canonical = parse_canonical(&fixture.markdown, profile).unwrap_or_else(|error| {
                panic!(
                    "{corpus} {} {} canonical parse failed: {error:?}",
                    fixture.example, fixture.section
                )
            });
            if let Err(error) = validate_canonical(&fixture.markdown, &canonical) {
                panic!(
                    "{corpus} {} {} canonical invalid: {error}",
                    fixture.example, fixture.section
                );
            }
            let donor = donor_rows(&fixture.markdown, profile);
            assert_eq!(
                donor.len(),
                canonical.nodes.len(),
                "{corpus} {} {} node count",
                fixture.example,
                fixture.section
            );
            for (index, (donor_node, canonical_node)) in
                donor.iter().zip(&canonical.nodes).enumerate()
            {
                assert_eq!(
                    donor_node.1, canonical_node.kind,
                    "{corpus} {} {} kind {index}",
                    fixture.example, fixture.section
                );
                if donor_node.2 != canonical_node.source {
                    donor_range_deltas += 1;
                    if !matches!(canonical_node.kind, BlockKind::Document) {
                        non_document_range_deltas += 1;
                        if has_list_ancestor(&canonical, canonical_node.handle) {
                            repair_scope_range_deltas += 1;
                        }
                    }
                    *delta_by_kind
                        .entry(kind_name(&canonical_node.kind).to_owned())
                        .or_default() += 1;
                    if !matches!(canonical_node.kind, BlockKind::Document)
                        && delta_samples.len() < 80
                    {
                        delta_samples.push(format!(
                            "{corpus}#{} {} node={index} kind={} donor={:?} canonical={:?}",
                            fixture.example,
                            fixture.section,
                            kind_name(&canonical_node.kind),
                            donor_node.2,
                            canonical_node.source
                        ));
                    }
                }
                if let Some(parent_index) = donor_node.0 {
                    let parent = &donor[parent_index].2;
                    if parent.start > donor_node.2.start || donor_node.2.end > parent.end {
                        donor_parent_containment_failures += 1;
                        if containment_samples.len() < 24 {
                            containment_samples.push(format!(
                                "{corpus}#{} {} child={index}:{:?} parent={parent_index}:{parent:?}",
                                fixture.example, fixture.section, donor_node.2
                            ));
                        }
                    }
                }
            }
            fixture_count += 1;
            node_count += canonical.nodes.len();
            repair_events += canonical.ignored_repair_events;
            detached_nodes += canonical.detached_nodes;
        }
    }

    eprintln!(
        "CANONICAL_RANGE_CORPUS fixtures={fixture_count} nodes={node_count} \
         donor_range_deltas={donor_range_deltas} \
         non_document_range_deltas={non_document_range_deltas} \
         repair_scope_range_deltas={repair_scope_range_deltas} \
         donor_parent_containment_failures={donor_parent_containment_failures} \
         canonical_parent_containment_failures=0 \
         ignored_repair_events={repair_events} detached_nodes={detached_nodes} \
         delta_by_kind={delta_by_kind:?}"
    );
    eprintln!(
        "DONOR_CONTAINMENT_SAMPLES\n{}",
        containment_samples.join("\n")
    );
    eprintln!("RANGE_DELTA_SAMPLES\n{}", delta_samples.join("\n"));
    assert_eq!(fixture_count, 1_322);
    assert!(donor_parent_containment_failures > 0);
    assert!(donor_range_deltas > 0);
    assert!(repair_events > 0);
}

fn has_list_ancestor(
    document: &flark_canonical_range_gate::CanonicalDocument,
    handle: u64,
) -> bool {
    let by_id = document
        .nodes
        .iter()
        .map(|node| (node.handle, node))
        .collect::<BTreeMap<_, _>>();
    let mut cursor = Some(handle);
    while let Some(current) = cursor {
        let node = by_id[&current];
        if matches!(node.kind, BlockKind::List(_)) {
            return true;
        }
        cursor = node.parent;
    }
    false
}

fn kind_name(kind: &BlockKind) -> &'static str {
    match kind {
        BlockKind::Document => "document",
        BlockKind::BlockQuote => "block_quote",
        BlockKind::List(_) => "list",
        BlockKind::Item(_) => "item",
        BlockKind::CodeBlock { .. } => "code",
        BlockKind::HtmlBlock { .. } => "html",
        BlockKind::Paragraph => "paragraph",
        BlockKind::Heading { .. } => "heading",
        BlockKind::ThematicBreak => "thematic_break",
        BlockKind::Table(_) => "table",
        BlockKind::TableRow { .. } => "table_row",
        BlockKind::TableCell => "table_cell",
    }
}

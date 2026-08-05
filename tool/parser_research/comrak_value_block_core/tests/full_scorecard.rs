use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use comrak::block_spine_facade::oracle_block_projection;
use flark_comrak_value_block_core::{
    BlockDocument, BlockKind, NodeId, SyntaxProfile, normalized_html, parse_document,
};
use flark_gate_a_harness::{SyntaxProfile as HarnessProfile, oracle_html_for};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    markdown: String,
    html: String,
    example: usize,
    section: String,
}

#[derive(Default)]
struct Score {
    exact: usize,
    authority_version_difference: usize,
    block_divergence: usize,
    output_or_inline_divergence: usize,
    candidate_error: usize,
    by_section: BTreeMap<String, [usize; 5]>,
    examples: Vec<String>,
    authority_fixture_ids: Vec<String>,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn load(path: &Path) -> Vec<Fixture> {
    serde_json::from_slice(&fs::read(path).expect("read corpus")).expect("decode corpus")
}

#[test]
fn full_commonmark_and_pinned_gfm_scorecard() {
    let root = repo_root();
    let commonmark = load(&root.join("test/fixtures/commonmark/upstream/common_mark_tests.json"));
    let gfm = load(&root.join("test/fixtures/commonmark/upstream/gfm_tests.json"));
    assert_eq!(commonmark.len(), 652);
    assert_eq!(gfm.len(), 670);

    let commonmark_score = score(
        &commonmark,
        SyntaxProfile::CommonMark,
        HarnessProfile::CommonMark0312,
    );
    let gfm_score = score(&gfm, SyntaxProfile::Gfm, HarnessProfile::FlarkGfm);
    print_score("COMMONMARK_0_31_2", &commonmark_score, commonmark.len());
    print_score("PINNED_GFM_0_29", &gfm_score, gfm.len());

    assert_eq!(
        commonmark_score.exact, 652,
        "CommonMark candidate divergences: {:?}",
        commonmark_score.examples
    );
    assert_eq!(
        commonmark_score.block_divergence
            + commonmark_score.authority_version_difference
            + commonmark_score.output_or_inline_divergence
            + commonmark_score.candidate_error,
        0
    );
    assert_eq!(gfm_score.exact, 661);
    assert_eq!(gfm_score.authority_version_difference, 9);
    assert_eq!(gfm_score.block_divergence, 0);
    assert_eq!(gfm_score.output_or_inline_divergence, 0);
    assert_eq!(gfm_score.candidate_error, 0);
    assert_eq!(
        gfm_score.authority_fixture_ids,
        [
            "HTML blocks:140",
            "HTML blocks:141",
            "HTML blocks:142",
            "HTML blocks:145",
            "HTML blocks:147",
            "Autolinks:610",
            "Autolinks:616",
            "Autolinks:619",
            "Autolinks:620",
        ]
    );
}

fn score(fixtures: &[Fixture], profile: SyntaxProfile, oracle_profile: HarnessProfile) -> Score {
    let mut score = Score::default();
    for fixture in fixtures {
        let result = parse_document(&fixture.markdown, profile)
            .map_err(|error| format!("parse {error:?}"))
            .and_then(|document| {
                normalized_html(&document)
                    .map(|html| (document, html))
                    .map_err(|error| format!("render {error:?}"))
            });
        let bucket = match result {
            Err(error) => {
                score.candidate_error += 1;
                push_example(&mut score, fixture, format!("candidate error: {error}"));
                4
            }
            Ok((_document, html)) if html == fixture.html => {
                score.exact += 1;
                0
            }
            Ok((document, html)) => {
                let oracle = oracle_html_for(oracle_profile, &fixture.markdown)
                    .expect("pinned Comrak oracle render");
                if html == oracle {
                    score.authority_version_difference += 1;
                    score
                        .authority_fixture_ids
                        .push(format!("{}:{}", fixture.section, fixture.example));
                    push_example(
                        &mut score,
                        fixture,
                        "candidate equals selected Comrak 0.54 profile, not older fixture".into(),
                    );
                    1
                } else if block_projection_matches(&document, &fixture.markdown, profile) {
                    score.output_or_inline_divergence += 1;
                    push_example(
                        &mut score,
                        fixture,
                        format!("block exact; candidate={html:?}; selected oracle={oracle:?}"),
                    );
                    3
                } else {
                    score.block_divergence += 1;
                    push_example(
                        &mut score,
                        fixture,
                        "candidate block projection differs from selected oracle".into(),
                    );
                    2
                }
            }
        };
        score.by_section.entry(fixture.section.clone()).or_default()[bucket] += 1;
    }
    score
}

fn push_example(score: &mut Score, fixture: &Fixture, reason: String) {
    if score.examples.len() < 40 {
        score.examples.push(format!(
            "{} example {}: {reason}",
            fixture.section, fixture.example
        ));
    }
}

fn block_projection_matches(
    document: &BlockDocument,
    markdown: &str,
    profile: SyntaxProfile,
) -> bool {
    let oracle = oracle_block_projection(markdown, profile == SyntaxProfile::Gfm);
    let mut ids = Vec::new();
    collect(document, document.tree.root, &mut ids);
    if ids.len() != oracle.len() {
        return false;
    }
    ids.iter().zip(oracle).all(|(id, expected)| {
        let node = document.tree.node(*id);
        let parent = node
            .parent
            .and_then(|parent| ids.iter().position(|candidate| *candidate == parent));
        let source = [
            node.source_start.line,
            node.source_start.column,
            node.source_end.line,
            node.source_end.column,
        ];
        parent == expected.parent
            && source == expected.source
            && coarse_kind(&node.kind) == coarse_oracle_kind(&expected.kind)
    })
}

fn collect(document: &BlockDocument, node: NodeId, output: &mut Vec<NodeId>) {
    output.push(node);
    for child in &document.tree.node(node).children {
        collect(document, *child, output);
    }
}

fn coarse_kind(kind: &BlockKind) -> &str {
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

fn coarse_oracle_kind(kind: &str) -> &str {
    kind.split(':').next().unwrap_or(kind)
}

fn print_score(label: &str, score: &Score, total: usize) {
    eprintln!(
        "{label} total={total} exact={} authority_version_difference={} block_divergence={} output_or_inline_divergence={} candidate_error={}",
        score.exact,
        score.authority_version_difference,
        score.block_divergence,
        score.output_or_inline_divergence,
        score.candidate_error
    );
    for (section, counts) in &score.by_section {
        if counts[1..].iter().any(|count| *count > 0) {
            eprintln!(
                "{label}_SECTION {section:?} exact={} authority={} block={} output_inline={} error={}",
                counts[0], counts[1], counts[2], counts[3], counts[4]
            );
        }
    }
    for example in &score.examples {
        eprintln!("{label}_EXAMPLE {example}");
    }
}

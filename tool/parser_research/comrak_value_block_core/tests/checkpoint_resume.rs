use flark_comrak_value_block_core::checkpoint::{
    LiveContinuationReceipt, MaterializerReceipt, PhysicalLine, ResumableValueBlockParser,
    TreeMaterializer,
};
use flark_comrak_value_block_core::source::SourceDocument;
use flark_comrak_value_block_core::{
    BlockDocument, BlockKind, NodeId, SyntaxProfile, normalized_html, parse_document,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct CorpusFixture {
    markdown: String,
    example: usize,
    section: String,
}

#[derive(Debug, Default)]
struct ScaleReceipt {
    source_bytes: usize,
    pending_bytes_copied: usize,
    logical_bytes_copied: usize,
    kind_bytes_copied: usize,
    events: usize,
    max_transient_nodes: usize,
    max_repair_entries: usize,
}

impl ScaleReceipt {
    fn add(&mut self, receipt: LiveContinuationReceipt) {
        self.source_bytes += receipt.source_leaf_bytes_copied;
        self.pending_bytes_copied += receipt.pending_logical_bytes_copied;
        self.logical_bytes_copied += receipt.materialized_logical_bytes_copied;
        self.kind_bytes_copied += receipt.materialized_kind_bytes_copied;
        self.events += receipt.structural_events_emitted;
        self.max_transient_nodes = self
            .max_transient_nodes
            .max(receipt.transient_nodes_before_compaction);
        self.max_repair_entries = self.max_repair_entries.max(receipt.repair_position_entries);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct NodeProjection {
    parent: Option<usize>,
    kind: BlockKind,
    source: [usize; 4],
    logical: String,
    origins: Vec<flark_comrak_value_block_core::OriginRun>,
    line_offsets: Vec<usize>,
}

fn projection(document: &BlockDocument) -> Vec<NodeProjection> {
    let mut ids = Vec::new();
    collect_preorder(document, document.tree.root, &mut ids);
    ids.iter()
        .map(|id| {
            let node = document.tree.node(*id);
            NodeProjection {
                parent: node
                    .parent
                    .and_then(|parent| ids.iter().position(|candidate| *candidate == parent)),
                kind: node.kind.clone(),
                source: [
                    node.source_start.line,
                    node.source_start.column,
                    node.source_end.line,
                    node.source_end.column,
                ],
                logical: node.content.logical.clone(),
                origins: node.content.origins.clone(),
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

fn resumed_with_materializer(
    source: &str,
    profile: SyntaxProfile,
    aggregate_positions: bool,
) -> (BlockDocument, MaterializerReceipt) {
    let source_document = SourceDocument::new(source);
    let mut sink = if aggregate_positions {
        TreeMaterializer::new_aggregate(profile)
    } else {
        TreeMaterializer::new(profile)
    };
    let mut parser = ResumableValueBlockParser::begin(profile);
    for leaf in &source_document.leaves {
        parser
            .push_line(
                PhysicalLine {
                    coverage_leaf_id: leaf.id,
                    absolute_start: leaf.absolute_start,
                    text: &leaf.text,
                },
                &mut sink,
            )
            .unwrap_or_else(|error| panic!("push physical line for {source:?}: {error:?}"));
        let (checkpoint, bindings, cursor) = parser.pause(&mut sink).expect("pause");
        let json = serde_json::to_string(&checkpoint).expect("serialize checkpoint");
        for forbidden in [
            "NodeId",
            "BlockTree",
            "Position",
            "absolute_start",
            "source_start",
            "source_end",
            "line_number",
            "revision",
        ] {
            assert!(
                !json.contains(forbidden),
                "canonical checkpoint leaked {forbidden}: {json}"
            );
        }
        let runtime = bindings.receipt();
        assert_eq!(runtime.frame_count, checkpoint.frames.len());
        assert!(runtime.contains_revision_positions);
        let checkpoint = serde_json::from_str(&json).expect("deserialize checkpoint");
        parser = ResumableValueBlockParser::resume(checkpoint, bindings, cursor).expect("resume");
    }
    parser.finish(&mut sink).expect("finish");
    sink.into_document_with_receipt()
}

fn resumed(source: &str, profile: SyntaxProfile) -> BlockDocument {
    resumed_with_materializer(source, profile, false).0
}

fn push_live_without_persisted_pause(
    source: &str,
    profile: SyntaxProfile,
) -> (ResumableValueBlockParser, TreeMaterializer, ScaleReceipt) {
    push_live_without_persisted_pause_with_materializer(source, profile, false)
}

fn push_live_without_persisted_pause_with_materializer(
    source: &str,
    profile: SyntaxProfile,
    aggregate_positions: bool,
) -> (ResumableValueBlockParser, TreeMaterializer, ScaleReceipt) {
    let source_document = SourceDocument::new(source);
    let mut sink = if aggregate_positions {
        TreeMaterializer::new_aggregate(profile)
    } else {
        TreeMaterializer::new(profile)
    };
    let mut parser = ResumableValueBlockParser::begin(profile);
    let mut receipt = ScaleReceipt::default();
    for leaf in &source_document.leaves {
        receipt.add(
            parser
                .push_line(
                    PhysicalLine {
                        coverage_leaf_id: leaf.id,
                        absolute_start: leaf.absolute_start,
                        text: &leaf.text,
                    },
                    &mut sink,
                )
                .expect("live scale line"),
        );
    }
    (parser, sink, receipt)
}

fn finish_live_scale(
    source: &str,
    profile: SyntaxProfile,
) -> (BlockDocument, ScaleReceipt, MaterializerReceipt) {
    let (parser, mut sink, mut receipt) = push_live_without_persisted_pause(source, profile);
    receipt.add(parser.finish(&mut sink).expect("finish live scale"));
    let (document, materializer) = sink.into_document_with_receipt();
    (document, receipt, materializer)
}

fn finish_live_scale_aggregate(
    source: &str,
    profile: SyntaxProfile,
) -> (BlockDocument, ScaleReceipt, MaterializerReceipt) {
    let (parser, mut sink, mut receipt) =
        push_live_without_persisted_pause_with_materializer(source, profile, true);
    receipt.add(
        parser
            .finish(&mut sink)
            .expect("finish aggregate live scale"),
    );
    let visible_positions = sink.resolve_position_page(0, 32);
    assert!(visible_positions.len() <= 32);
    let (document, materializer) = sink.into_document_with_receipt();
    (document, receipt, materializer)
}

fn assert_resume_exact(source: &str, profile: SyntaxProfile) {
    let expected = parse_document(source, profile).expect("one-shot parser");
    let actual = resumed(source, profile);
    assert_eq!(
        actual.source.leaves, expected.source.leaves,
        "source: {source:?}"
    );
    assert_eq!(actual.references, expected.references, "refs: {source:?}");
    assert_eq!(
        projection(&actual),
        projection(&expected),
        "blocks: {source:?}"
    );
    assert_eq!(
        normalized_html(&actual).expect("resumed html"),
        normalized_html(&expected).expect("one-shot html"),
        "html: {source:?}"
    );
}

fn assert_aggregate_resume_exact(source: &str, profile: SyntaxProfile) {
    let expected = resumed(source, profile);
    let (actual, receipt) = resumed_with_materializer(source, profile, true);
    assert_eq!(
        actual.source.leaves, expected.source.leaves,
        "source: {source:?}"
    );
    assert_eq!(actual.references, expected.references, "refs: {source:?}");
    assert_eq!(
        projection(&actual),
        projection(&expected),
        "blocks: {source:?}"
    );
    assert_eq!(
        normalized_html(&actual).expect("aggregate html"),
        normalized_html(&expected).expect("eager html"),
        "html: {source:?}"
    );
    assert_eq!(receipt.repair_nodes_scanned, 0);
    assert_eq!(receipt.final_list_nodes_scanned, 0);
    assert_eq!(receipt.lazy_repair_descendant_touches, 0);
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn load_corpus(path: &Path) -> Vec<CorpusFixture> {
    serde_json::from_slice(&fs::read(path).expect("read corpus")).expect("decode corpus")
}

#[test]
fn every_line_resume_covers_adversarial_block_transitions() {
    let cases = [
        (SyntaxProfile::CommonMark, "- a\n\n  - b\n\n    c\n- d\n"),
        (SyntaxProfile::CommonMark, "alpha\n===\n\nbeta\n---\n"),
        (
            SyntaxProfile::Gfm,
            "before\n\n| a | b |\n| :- | -: |\n| c | d |\n\nafter\n",
        ),
        (SyntaxProfile::Gfm, "  \n\naaa\n  \n\n# aaa\n\n  \n"),
        (
            SyntaxProfile::Gfm,
            "| foo | bar |\n| --- | --- |\n| baz | bim |\n",
        ),
        (
            SyntaxProfile::CommonMark,
            "[\nfoo\n]: /url\n\"title\" ok\n\n[foo]\n",
        ),
        (SyntaxProfile::CommonMark, "> a\nlazy\n> b\n"),
        (
            SyntaxProfile::CommonMark,
            "``` rust\nlet x = 1;\n```\n\n<script>\nx\n</script>\n",
        ),
        (
            SyntaxProfile::Gfm,
            "\u{feff}>\talpha\r\n> beta\r\n\r- gamma\r",
        ),
    ];
    for (profile, source) in cases {
        assert_resume_exact(source, profile);
    }
}

#[test]
fn aggregate_position_overlay_matches_eager_oracle_for_repair_and_promotion_edges() {
    let cases = [
        (SyntaxProfile::CommonMark, "- a\n  > b\n\n    c\n"),
        (SyntaxProfile::CommonMark, "- a\n\n  - b\n\n    c\n- d\n"),
        (SyntaxProfile::CommonMark, "- a\n\n  continuation\n\n- b\n"),
        (SyntaxProfile::CommonMark, "alpha\n===\n\nbeta\n---\n"),
        (
            SyntaxProfile::CommonMark,
            "[\nfoo\n]: /url\n\"title\" ok\n\n[foo]\n",
        ),
        (
            SyntaxProfile::Gfm,
            "before\n\n| a | b |\n| :- | -: |\n| c | d |\n\nafter\n",
        ),
    ];
    for (profile, source) in cases {
        assert_aggregate_resume_exact(source, profile);
    }
}

#[test]
fn aggregate_position_overlay_covers_gfm_blank_table_transition_window() {
    let path = repo_root().join("test/fixtures/commonmark/upstream/gfm_tests.json");
    for fixture in load_corpus(&path)
        .into_iter()
        .filter(|fixture| (197..=220).contains(&fixture.example))
    {
        assert_aggregate_resume_exact(&fixture.markdown, SyntaxProfile::Gfm);
    }
}

#[test]
fn aggregate_position_overlay_challenges_many_detached_reference_paragraphs() {
    let mut source = String::new();
    for index in 0..256 {
        if index == 0 {
            source.push_str(&format!("- [r{index}]: /{index}\n\n"));
        } else {
            source.push_str(&format!("  [r{index}]: /{index}\n\n"));
        }
    }
    let eager = resumed(&source, SyntaxProfile::CommonMark);
    let (aggregate, receipt) = resumed_with_materializer(&source, SyntaxProfile::CommonMark, true);
    assert_eq!(projection(&aggregate), projection(&eager));
    assert_eq!(aggregate.references, eager.references);
    eprintln!("DETACH_CHALLENGE {receipt:?}");
    assert!(receipt.lazy_detach_nodes_touched <= 256);
    assert!(
        receipt.lazy_max_position_resolution_steps <= 64,
        "{receipt:?}"
    );
}

#[test]
fn aggregate_position_overlay_bounds_deep_quote_and_capped_nested_list_reads() {
    let deep_quote = format!("{}x\n", "> ".repeat(512));
    let eager_quote = resumed(&deep_quote, SyntaxProfile::CommonMark);
    let (aggregate_quote, quote_receipt) =
        resumed_with_materializer(&deep_quote, SyntaxProfile::CommonMark, true);
    assert_eq!(projection(&aggregate_quote), projection(&eager_quote));
    assert!(quote_receipt.lazy_candidate_updates <= 2_048);
    assert!(
        quote_receipt.lazy_max_position_resolution_steps <= 8,
        "{quote_receipt:?}"
    );

    let nested_lists = format!("{}x\n", "- ".repeat(100));
    let eager_lists = resumed(&nested_lists, SyntaxProfile::CommonMark);
    let (aggregate_lists, list_receipt) =
        resumed_with_materializer(&nested_lists, SyntaxProfile::CommonMark, true);
    assert_eq!(projection(&aggregate_lists), projection(&eager_lists));
    assert_eq!(aggregate_lists.references, eager_lists.references);
    eprintln!("DEPTH_CHALLENGE quote={quote_receipt:?} list={list_receipt:?}");
    assert!(
        list_receipt.lazy_max_position_resolution_steps <= 2_500,
        "{list_receipt:?}"
    );
}

#[test]
fn deep_quote_receipts_separate_transition_finish_point_query_and_full_read() {
    for depth in [1_000_usize, 5_000, 20_000] {
        let source = format!("{}x\n", "> ".repeat(depth));
        let source_document = SourceDocument::new(&source);
        assert_eq!(source_document.leaves.len(), 1);
        let leaf = &source_document.leaves[0];
        let mut sink = TreeMaterializer::new_aggregate(SyntaxProfile::CommonMark);
        let mut parser = ResumableValueBlockParser::begin(SyntaxProfile::CommonMark);

        let started = Instant::now();
        let push = parser
            .push_line(
                PhysicalLine {
                    coverage_leaf_id: leaf.id,
                    absolute_start: leaf.absolute_start,
                    text: &leaf.text,
                },
                &mut sink,
            )
            .expect("deep quote transition");
        let push_elapsed = started.elapsed();
        let after_push = sink.receipt();
        let nodes = sink.output_node_count();

        let started = Instant::now();
        let finish = parser.finish(&mut sink).expect("deep quote finish");
        let finish_elapsed = started.elapsed();
        let after_finish = sink.receipt();

        let started = Instant::now();
        let point = sink.resolve_position_page(nodes - 1, 1);
        let point_elapsed = started.elapsed();
        let after_point = sink.receipt();
        assert_eq!(point.len(), 1);

        let started = Instant::now();
        let (_, full_read) = sink.into_document_with_receipt();
        let full_read_elapsed = started.elapsed();

        eprintln!(
            "DEEP_QUOTE depth={depth} bytes={} nodes={nodes} \
             push_us={} push={push:?}/{after_push:?} \
             finish_us={} finish={finish:?}/{after_finish:?} \
             point_us={} point={after_point:?} full_us={} full={full_read:?}",
            source.len(),
            push_elapsed.as_micros(),
            finish_elapsed.as_micros(),
            point_elapsed.as_micros(),
            full_read_elapsed.as_micros(),
        );

        assert_eq!(nodes, depth + 2);
        assert!(push.structural_events_emitted <= nodes * 8);
        assert!(finish.structural_events_emitted <= nodes * 8);
        assert!(after_finish.lazy_candidate_updates <= nodes * 4);
        assert!(
            after_finish.lazy_position_index_steps
                <= after_finish.lazy_candidate_updates.saturating_mul(32)
        );
        assert!(after_finish.lazy_position_index_resize_nodes_rebuilt <= nodes * 5);
        assert_eq!(after_finish.lazy_position_resolution_steps, 0);
        assert_eq!(after_point.lazy_position_page_queries, 1);
        assert_eq!(after_point.lazy_max_position_page_nodes, 1);
        assert_eq!(after_point.lazy_max_position_resolution_steps, 2);
        assert_eq!(full_read.lazy_max_position_resolution_steps, 2);
        assert_eq!(full_read.lazy_repair_scope_records, 0);
    }
}

#[test]
fn every_physical_line_resume_is_exact_over_full_1322_fixture_corpus() {
    let root = repo_root();
    let corpora = [
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
    for (path, profile) in corpora {
        for fixture in load_corpus(&path) {
            let expected = parse_document(&fixture.markdown, profile).unwrap_or_else(|error| {
                panic!(
                    "one-shot parse failed for {} example {}: {error:?}",
                    fixture.section, fixture.example
                )
            });
            let actual = resumed(&fixture.markdown, profile);
            let (aggregate, aggregate_receipt) =
                resumed_with_materializer(&fixture.markdown, profile, true);
            assert_eq!(
                actual.source.leaves, expected.source.leaves,
                "source: {} example {}",
                fixture.section, fixture.example
            );
            assert_eq!(
                actual.references, expected.references,
                "references: {} example {}",
                fixture.section, fixture.example
            );
            assert_eq!(
                projection(&actual),
                projection(&expected),
                "blocks/origins: {} example {}",
                fixture.section,
                fixture.example
            );
            assert_eq!(
                normalized_html(&actual).expect("resumed html"),
                normalized_html(&expected).expect("one-shot html"),
                "html: {} example {}",
                fixture.section,
                fixture.example
            );
            assert_eq!(
                projection(&aggregate),
                projection(&actual),
                "aggregate positions: {} example {}",
                fixture.section,
                fixture.example
            );
            assert_eq!(aggregate.references, actual.references);
            assert_eq!(aggregate.source.leaves, actual.source.leaves);
            assert_eq!(aggregate_receipt.repair_nodes_scanned, 0);
            assert_eq!(aggregate_receipt.final_list_nodes_scanned, 0);
            assert_eq!(aggregate_receipt.lazy_repair_descendant_touches, 0);
            compared += 1;
        }
    }
    assert_eq!(compared, 1_322);
}

#[test]
fn live_compaction_is_bounded_without_persisted_json_pause_each_line() {
    let mut source = String::new();
    for index in 0..500 {
        source.push_str(&format!("- item {index}\n\ncontinuation {index}\n\n"));
    }
    let source_document = SourceDocument::new(&source);
    let mut sink = TreeMaterializer::new_aggregate(SyntaxProfile::CommonMark);
    let mut parser = ResumableValueBlockParser::begin(SyntaxProfile::CommonMark);
    let mut max_transient_nodes = 0;
    let mut max_repair_entries = 0;
    let mut max_open_frames = 0;
    for leaf in &source_document.leaves {
        let receipt = parser
            .push_line(
                PhysicalLine {
                    coverage_leaf_id: leaf.id,
                    absolute_start: leaf.absolute_start,
                    text: &leaf.text,
                },
                &mut sink,
            )
            .expect("live line continuation");
        max_transient_nodes = max_transient_nodes.max(receipt.transient_nodes_before_compaction);
        max_repair_entries = max_repair_entries.max(receipt.repair_position_entries);
        max_open_frames = max_open_frames.max(receipt.retained_open_frames);
    }

    // Persist only once after 2,000 physical lines. The live parser still
    // seals/rebuilds scratch at every line boundary without JSON copying.
    let (checkpoint, bindings, cursor) = parser.pause(&mut sink).expect("single persisted pause");
    let json = serde_json::to_string(&checkpoint).expect("serialize final live checkpoint");
    let checkpoint = serde_json::from_str(&json).expect("deserialize final live checkpoint");
    let parser = ResumableValueBlockParser::resume(checkpoint, bindings, cursor).expect("resume");
    parser.finish(&mut sink).expect("finish");
    let actual = sink.into_document();
    let expected = parse_document(&source, SyntaxProfile::CommonMark).expect("one shot");

    assert_eq!(projection(&actual), projection(&expected));
    assert_eq!(actual.references, expected.references);
    assert_eq!(normalized_html(&actual), normalized_html(&expected));
    assert!(
        max_open_frames <= 4,
        "open frames grew to {max_open_frames}"
    );
    assert!(
        max_transient_nodes <= 7,
        "transient scratch grew with document: {max_transient_nodes} nodes"
    );
    assert!(
        max_repair_entries <= max_transient_nodes,
        "repair payload exceeded transient scratch: {max_repair_entries} > {max_transient_nodes}"
    );
}

#[test]
fn live_move_and_delta_receipt_is_linear_for_a_growing_open_paragraph() {
    let source = "x\n".repeat(256);
    let source_document = SourceDocument::new(&source);
    let mut sink = TreeMaterializer::new_aggregate(SyntaxProfile::CommonMark);
    let mut parser = ResumableValueBlockParser::begin(SyntaxProfile::CommonMark);
    let mut total_pending_bytes_copied = 0;
    let mut final_pending_bytes_copied = 0;
    let mut total_materialized_bytes_copied = 0;
    for leaf in &source_document.leaves {
        let receipt = parser
            .push_line(
                PhysicalLine {
                    coverage_leaf_id: leaf.id,
                    absolute_start: leaf.absolute_start,
                    text: &leaf.text,
                },
                &mut sink,
            )
            .expect("live line continuation");
        total_pending_bytes_copied += receipt.pending_logical_bytes_copied;
        final_pending_bytes_copied = receipt.pending_logical_bytes_copied;
        total_materialized_bytes_copied += receipt.materialized_logical_bytes_copied;
    }
    assert_eq!(final_pending_bytes_copied, 0);
    eprintln!(
        "COPY_GATE source_bytes={} pending_bytes_copied={} materialized_bytes_copied={}",
        source.len(),
        total_pending_bytes_copied,
        total_materialized_bytes_copied
    );
    assert_eq!(total_pending_bytes_copied, 0);
    assert_eq!(total_materialized_bytes_copied, source.len());
    assert_eq!(sink.receipt().lazy_sparse_position_snapshots, 0);

    // The grammar decision is now an ephemeral physical-line fact, not a read
    // of runtime-only source Positions restored outside checkpoint equality.
    let parser_source = include_str!("../src/parser.rs");
    assert!(!parser_source.contains("source_start.line != self.line_number"));
    assert!(parser_source.contains("opened_this_line.contains(&container)"));
}

#[test]
fn one_mib_live_continuation_receipts_are_linear_and_meter_list_output_scan() {
    let payload_line = format!("{}\n", "a".repeat(1023));
    let paragraph = payload_line.repeat(1024);
    assert_eq!(paragraph.len(), 1_048_576);
    let (paragraph_parser, mut paragraph_sink, paragraph_receipt) =
        push_live_without_persisted_pause(&paragraph, SyntaxProfile::CommonMark);
    assert_eq!(paragraph_receipt.source_bytes, paragraph.len());
    assert_eq!(paragraph_receipt.pending_bytes_copied, 0);
    assert_eq!(paragraph_receipt.logical_bytes_copied, paragraph.len());
    assert_eq!(paragraph_receipt.kind_bytes_copied, 0);
    assert!(paragraph_receipt.max_transient_nodes <= 2);
    let over_cap = paragraph_parser
        .finish(&mut paragraph_sink)
        .expect_err("giant paragraph finalization remains facade-capped");
    assert!(format!("{over_cap:?}").contains("OverCap"));

    let fence = format!("```\n{paragraph}```\n");
    let (fence_document, fence_receipt, fence_materializer) =
        finish_live_scale(&fence, SyntaxProfile::CommonMark);
    assert_large_projection_exact(&fence, SyntaxProfile::CommonMark, &fence_document);
    assert_linear_live_receipt("fence", &fence, &fence_receipt, 3);
    assert!(fence_receipt.kind_bytes_copied <= fence.len() + 16);
    assert_eq!(fence_materializer.repair_nodes_scanned, 0);

    let html = format!("<script>\n{paragraph}</script>\n");
    let (html_document, html_receipt, html_materializer) =
        finish_live_scale(&html, SyntaxProfile::CommonMark);
    assert_large_projection_exact(&html, SyntaxProfile::CommonMark, &html_document);
    assert_linear_live_receipt("html", &html, &html_receipt, 3);
    assert!(html_receipt.kind_bytes_copied <= html.len() + 16);
    assert_eq!(html_materializer.repair_nodes_scanned, 0);

    let list_line = format!("- {}\n", "l".repeat(125));
    assert_eq!(list_line.len(), 128);
    let list = list_line.repeat(8192);
    assert_eq!(list.len(), 1_048_576);
    let (list_document, list_receipt, list_materializer) =
        finish_live_scale(&list, SyntaxProfile::CommonMark);
    assert_large_projection_exact(&list, SyntaxProfile::CommonMark, &list_document);
    assert_linear_live_receipt("list", &list, &list_receipt, 8);
    assert_eq!(list_receipt.kind_bytes_copied, 0);
    assert!(list_materializer.repair_events >= 1);
    assert!(list_materializer.max_repair_nodes_scanned >= 16_385);
    assert!(list_materializer.final_list_nodes_scanned >= 16_384);

    let (aggregate_list_document, aggregate_list_receipt, aggregate_list_materializer) =
        finish_live_scale_aggregate(&list, SyntaxProfile::CommonMark);
    assert_eq!(
        projection(&aggregate_list_document),
        projection(&list_document)
    );
    assert_eq!(
        aggregate_list_receipt.source_bytes,
        list_receipt.source_bytes
    );
    assert_eq!(aggregate_list_receipt.events, list_receipt.events);
    assert!(aggregate_list_materializer.repair_events >= 1);
    assert_eq!(aggregate_list_materializer.repair_nodes_scanned, 0);
    assert_eq!(aggregate_list_materializer.max_repair_nodes_scanned, 0);
    assert_eq!(aggregate_list_materializer.final_list_nodes_scanned, 0);
    assert!(aggregate_list_materializer.lazy_repair_scope_records >= 1);
    assert_eq!(
        aggregate_list_materializer.lazy_repair_descendant_touches,
        0
    );
    assert!(aggregate_list_materializer.lazy_final_list_aggregate_reads >= 1);
    assert_eq!(aggregate_list_materializer.lazy_repair_open_depth_steps, 0);
    assert!(aggregate_list_materializer.lazy_candidate_updates <= list_receipt.events);
    assert!(
        aggregate_list_materializer.lazy_position_index_steps
            <= aggregate_list_materializer
                .lazy_candidate_updates
                .saturating_mul(32)
    );
    assert!(aggregate_list_materializer.lazy_position_index_resize_nodes_rebuilt <= 8192 * 16);
    assert!(aggregate_list_materializer.lazy_max_position_resolution_steps <= 16);
    assert_eq!(
        aggregate_list_materializer.lazy_sparse_position_snapshots,
        0
    );
    assert_eq!(aggregate_list_materializer.lazy_position_page_queries, 1);
    assert_eq!(aggregate_list_materializer.lazy_max_position_page_nodes, 32);

    let table_row = format!("|{}|{}|\n", "x".repeat(510), "y".repeat(510));
    assert_eq!(table_row.len(), 1024);
    let table = format!("| a | b |\n| --- | --- |\n{}", table_row.repeat(1024));
    let (table_document, table_receipt, table_materializer) =
        finish_live_scale(&table, SyntaxProfile::Gfm);
    assert_large_projection_exact(&table, SyntaxProfile::Gfm, &table_document);
    assert_linear_live_receipt("table", &table, &table_receipt, 8);
    assert!(table_receipt.kind_bytes_copied < table.len() / 100);
    assert_eq!(table_materializer.repair_nodes_scanned, 0);

    eprintln!(
        "ONE_MIB_RECEIPTS paragraph={paragraph_receipt:?} fence={fence_receipt:?} \
         html={html_receipt:?} list={list_receipt:?}/{list_materializer:?} \
         aggregate_list={aggregate_list_receipt:?}/{aggregate_list_materializer:?} \
         table={table_receipt:?}"
    );
}

fn assert_linear_live_receipt(
    label: &str,
    source: &str,
    receipt: &ScaleReceipt,
    max_transient_nodes: usize,
) {
    assert_eq!(receipt.source_bytes, source.len(), "{label}");
    assert_eq!(receipt.pending_bytes_copied, 0, "{label}");
    assert!(
        receipt.logical_bytes_copied <= source.len() + 32,
        "{label}: {receipt:?}"
    );
    assert!(
        receipt.events <= source.lines().count() * 20 + 32,
        "{label}: {receipt:?}"
    );
    assert!(
        receipt.max_transient_nodes <= max_transient_nodes,
        "{label}: {receipt:?}"
    );
}

fn assert_large_projection_exact(source: &str, profile: SyntaxProfile, actual: &BlockDocument) {
    let expected = parse_document(source, profile).expect("one-shot large parse");
    assert_eq!(actual.source.leaves, expected.source.leaves);
    assert_eq!(actual.references, expected.references);
    assert_eq!(projection(actual), projection(&expected));
}
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

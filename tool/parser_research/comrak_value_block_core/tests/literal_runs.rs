use comrak::block_spine_facade::MAX_CLASSIFICATION_BYTES;
use flark_comrak_value_block_core::checkpoint::{
    LiveContinuationReceipt, PhysicalLine, ResumableValueBlockParser, TreeMaterializer,
};
use flark_comrak_value_block_core::render::bounded_code_info;
use flark_comrak_value_block_core::source::SourceDocument;
use flark_comrak_value_block_core::{
    BlockDocument, BlockKind, FuelledValueBlockParser, NodeId, SyntaxProfile, WorkBudget,
    WorkStatus, normalized_html, parse_document,
};
use flark_gate_a_harness::{SyntaxProfile as HarnessProfile, oracle_html_for};

const MIB: usize = 1024 * 1024;
const LINE_BYTES: usize = 1024;
const POLL: WorkBudget = WorkBudget::new(64, 64, 64);

#[derive(Clone, Copy, Debug, Default)]
struct CopyTotals {
    source_leaf_bytes: usize,
    logical_bytes: usize,
    origin_runs: usize,
    line_offsets: usize,
    kind_bytes: usize,
    max_delta_logical_bytes: usize,
    max_delta_origin_runs: usize,
    max_delta_line_offsets: usize,
}

impl CopyTotals {
    fn add(&mut self, receipt: LiveContinuationReceipt) {
        assert_eq!(receipt.pending_logical_bytes_copied, 0);
        self.source_leaf_bytes += receipt.source_leaf_bytes_copied;
        self.logical_bytes += receipt.materialized_logical_bytes_copied;
        self.origin_runs += receipt.materialized_origin_runs_copied;
        self.line_offsets += receipt.materialized_line_offsets_copied;
        self.kind_bytes += receipt.materialized_kind_bytes_copied;
        self.max_delta_logical_bytes = self
            .max_delta_logical_bytes
            .max(receipt.max_delta_logical_bytes_copied);
        self.max_delta_origin_runs = self
            .max_delta_origin_runs
            .max(receipt.max_delta_origin_runs_copied);
        self.max_delta_line_offsets = self
            .max_delta_line_offsets
            .max(receipt.max_delta_line_offsets_copied);
    }
}

fn multiline_payload(bytes: usize) -> String {
    assert_eq!(bytes % LINE_BYTES, 0);
    let mut output = String::with_capacity(bytes);
    let line = format!("{}\n", "x".repeat(LINE_BYTES - 1));
    for _ in 0..bytes / LINE_BYTES {
        output.push_str(&line);
    }
    output
}

fn only_raw_block(document: &BlockDocument) -> (NodeId, &BlockKind) {
    let root = document.tree.root;
    let children = &document.tree.node(root).children;
    assert_eq!(children.len(), 1);
    let node = children[0];
    (node, &document.tree.node(node).kind)
}

fn resumed(source: &str) -> (BlockDocument, CopyTotals) {
    let source_document = SourceDocument::new(source);
    let mut parser = ResumableValueBlockParser::begin(SyntaxProfile::CommonMark);
    let mut materializer = TreeMaterializer::new_aggregate(SyntaxProfile::CommonMark);
    let mut totals = CopyTotals::default();
    for leaf in &source_document.leaves {
        totals.add(
            parser
                .push_line(
                    PhysicalLine {
                        coverage_leaf_id: leaf.id,
                        absolute_start: leaf.absolute_start,
                        text: &leaf.text,
                    },
                    &mut materializer,
                )
                .expect("live raw-block line"),
        );
    }
    totals.add(parser.finish(&mut materializer).expect("finish raw block"));
    (materializer.into_document(), totals)
}

fn assert_copy_contract(source: &str, expected_literal: &str, code: bool) {
    let direct = parse_document(source, SyntaxProfile::CommonMark).expect("direct raw block");
    let ownership = direct.literal_ownership_receipt();
    assert_eq!(ownership.blocks, 1);
    assert_eq!(ownership.owned_aggregate_literal_bytes, 0, "{ownership:?}");
    assert!(ownership.referenced_logical_bytes >= expected_literal.len());
    assert!(ownership.origin_runs > 1);

    let (node, kind) = only_raw_block(&direct);
    let literal = match kind {
        BlockKind::CodeBlock { info, literal, .. } => {
            assert!(code);
            let info = bounded_code_info(&direct, node, *info).expect("bounded info transform");
            assert_eq!(info.value, "text");
            assert_eq!(info.receipt.raw_bytes_copied, 4);
            assert!(info.receipt.normalized_bytes_owned <= info.receipt.input_cap);
            *literal
        }
        BlockKind::HtmlBlock { literal, .. } => {
            assert!(!code);
            *literal
        }
        other => panic!("unexpected raw block: {other:?}"),
    };
    assert_eq!(
        direct
            .materialize_projection(node, literal)
            .expect("direct literal projection"),
        expected_literal
    );

    let html = normalized_html(&direct).expect("streamed raw-block render");
    let oracle =
        oracle_html_for(HarnessProfile::CommonMark0312, source).expect("selected Comrak oracle");
    assert_eq!(html, oracle);

    let (live, totals) = resumed(source);
    assert_eq!(
        live.literal_ownership_receipt()
            .owned_aggregate_literal_bytes,
        0
    );
    assert_eq!(totals.source_leaf_bytes, source.len());
    assert_eq!(totals.logical_bytes, 0, "{totals:?}");
    assert_eq!(totals.kind_bytes, 0, "{totals:?}");
    assert_eq!(totals.max_delta_logical_bytes, 0, "{totals:?}");
    assert!(totals.origin_runs > 1, "{totals:?}");
    assert!(totals.line_offsets > 1, "{totals:?}");
    assert!(totals.max_delta_origin_runs <= 1, "{totals:?}");
    assert!(totals.max_delta_line_offsets <= 1, "{totals:?}");
    assert_eq!(normalized_html(&live).expect("live render"), html);
}

#[test]
fn one_and_ten_mib_fence_and_html_are_source_backed_and_oracle_exact() {
    for bytes in [MIB, 10 * MIB] {
        let payload = multiline_payload(bytes);

        let code = format!("```text\n{payload}```\n");
        assert_copy_contract(&code, &payload, true);
        drop(code);

        let html = format!("<script>\n{payload}</script>\n");
        assert_copy_contract(&html, &html, false);
    }
}

#[test]
fn cancellation_of_a_one_mib_open_fence_copies_no_aggregate_literal() {
    let source = format!("```text\n{}", multiline_payload(MIB));
    let lines = SourceDocument::new(&source);
    let mut parser = FuelledValueBlockParser::new(&source, SyntaxProfile::CommonMark);
    for leaf in lines.leaves {
        parser.begin_line(leaf.id, leaf.text).expect("begin line");
        loop {
            let receipt = parser.poll_line(POLL).expect("poll line");
            if receipt.status == WorkStatus::Complete {
                break;
            }
            assert_eq!(receipt.status, WorkStatus::Pending);
        }
    }
    let receipt = parser.cancel();
    assert_eq!(receipt.open_frames_copied, 0);
    assert_eq!(receipt.owned_aggregate_literal_bytes_awaiting_reclaim, 0);
    assert!(receipt.source_backed_logical_bytes_awaiting_reclaim >= MIB);
    assert!(receipt.raw_block_origin_runs_awaiting_reclaim > 1);
    assert!(!receipt.tree_reclaim_is_fuelled);
}

#[test]
fn fenced_info_normalization_is_exact_and_preallocation_bounded() {
    let source = "```  ru&amp;by\\!  \nbody\n```\n";
    let document = parse_document(source, SyntaxProfile::CommonMark).expect("code info parse");
    let (node, kind) = only_raw_block(&document);
    let BlockKind::CodeBlock { info, .. } = kind else {
        panic!("expected code block")
    };
    let normalized = bounded_code_info(&document, node, *info).expect("normalize info");
    assert_eq!(normalized.value, "ru&by!");
    assert_eq!(normalized.receipt.raw_bytes_copied, info.len() as usize);
    assert!(normalized.receipt.raw_bytes_copied <= normalized.receipt.input_cap);
    assert_eq!(
        normalized_html(&document).expect("render"),
        oracle_html_for(HarnessProfile::CommonMark0312, source).expect("oracle")
    );

    let near_cap_info = "a".repeat(MAX_CLASSIFICATION_BYTES - 4);
    let near_cap = format!("```{near_cap_info}\n```\n");
    let document = parse_document(&near_cap, SyntaxProfile::CommonMark).expect("near-cap info");
    let (node, kind) = only_raw_block(&document);
    let BlockKind::CodeBlock { info, .. } = kind else {
        panic!("expected near-cap code block")
    };
    let normalized = bounded_code_info(&document, node, *info).expect("bounded near-cap info");
    assert_eq!(normalized.value, near_cap_info);
    assert_eq!(
        normalized.receipt.raw_bytes_copied,
        MAX_CLASSIFICATION_BYTES - 4
    );
    assert_eq!(normalized.receipt.input_cap, MAX_CLASSIFICATION_BYTES);

    let over_cap = format!("```{}\n```\n", "a".repeat(MAX_CLASSIFICATION_BYTES - 3));
    assert!(
        parse_document(&over_cap, SyntaxProfile::CommonMark).is_err(),
        "an oversized opener must be rejected before an info allocation"
    );
}

#[test]
fn source_backed_cursor_accepts_stable_nonordinal_coverage_ids() {
    let source = "<script>\none\ntwo\n</script>\n";
    let mut source_document = SourceDocument::new(source);
    let stable_ids = [91_u64, 7, 4_000_000, 18];
    assert_eq!(source_document.leaves.len(), stable_ids.len());
    for (leaf, id) in source_document.leaves.iter_mut().zip(stable_ids) {
        leaf.id = id;
    }
    let source_document = SourceDocument::from_leaves(source_document.leaves);
    let mut parser = ResumableValueBlockParser::begin(SyntaxProfile::CommonMark);
    let mut materializer = TreeMaterializer::new_aggregate(SyntaxProfile::CommonMark);
    for leaf in &source_document.leaves {
        parser
            .push_line(
                PhysicalLine {
                    coverage_leaf_id: leaf.id,
                    absolute_start: leaf.absolute_start,
                    text: &leaf.text,
                },
                &mut materializer,
            )
            .expect("stable-id line");
    }
    parser.finish(&mut materializer).expect("stable-id finish");
    let document = materializer.into_document();
    assert_eq!(
        normalized_html(&document).expect("stable-id render"),
        source
    );
    let (node, _) = only_raw_block(&document);
    let actual_ids = document
        .tree
        .node(node)
        .content
        .origins
        .iter()
        .filter_map(|run| run.source.as_ref().map(|range| range.leaf_id))
        .collect::<Vec<_>>();
    assert_eq!(actual_ids, stable_ids);
}

#[test]
fn bare_fence_at_end_of_input_has_a_zero_length_source_backed_projection() {
    for source in ["```", "~~~", "```text"] {
        let direct = parse_document(source, SyntaxProfile::CommonMark).expect("bare fence");
        assert_eq!(
            direct
                .literal_ownership_receipt()
                .owned_aggregate_literal_bytes,
            0
        );
        assert_eq!(
            normalized_html(&direct).expect("bare-fence render"),
            oracle_html_for(HarnessProfile::CommonMark0312, source).expect("bare-fence oracle")
        );
        let (live, totals) = resumed(source);
        assert_eq!(totals.logical_bytes, 0);
        assert_eq!(totals.kind_bytes, 0);
        assert_eq!(
            normalized_html(&live).expect("live bare-fence render"),
            normalized_html(&direct).expect("direct bare-fence render")
        );
    }
}

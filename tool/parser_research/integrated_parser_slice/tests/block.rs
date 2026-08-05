use std::collections::BTreeSet;
use std::sync::{Arc, Weak};

use flark_integrated_parser_slice::block::{
    BlockContainer, BlockError, BlockJob, BlockOutput, BlockStatus, UnsupportedFeature,
    BLOCK_POLL_IS_MEASURED_SCHEDULER_ADMISSIBLE, MAX_BLOCK_CAPTURE_CHECKPOINT_BYTES,
    MAX_BLOCK_POLL_WORK, MAX_BLOCK_PREFIX_BYTES,
};
use flark_integrated_parser_slice::frontier::{
    CursorStep, LexerStatus, LogicalOrigin, SegmentDescriptor, SharedLexer, VirtualReason,
    MAX_LEXER_POLL_WORK,
};
use flark_integrated_parser_slice::grammar::{
    GrammarClassification, GrammarJob, GrammarRecord, GrammarStatus, MAX_GRAMMAR_POLL_WORK,
};
use flark_integrated_parser_slice::source::{PersistentSource, MAX_PIECE_BYTES};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

const _: () = assert!(!BLOCK_POLL_IS_MEASURED_SCHEDULER_ADMISSIBLE);

fn parse_blocks(source: Arc<PersistentSource>, fuel: usize) -> BlockOutput {
    let mut job = BlockJob::new(source);
    loop {
        let poll = job.poll(fuel);
        assert!(poll.work <= fuel);
        assert!(poll.work <= MAX_BLOCK_POLL_WORK);
        match poll.status {
            BlockStatus::Pending => {}
            BlockStatus::Ready => return job.result().unwrap().clone(),
            BlockStatus::Failed => panic!("block parse failed: {:?}", job.error()),
        }
    }
}

fn logical_bytes(leaf: &flark_integrated_parser_slice::block::BlockLeaf) -> Vec<u8> {
    let mut cursor = leaf.input.cursor();
    let mut result = Vec::new();
    loop {
        match cursor.step() {
            CursorStep::Byte(byte) => result.push(byte.byte),
            CursorStep::Progress => {}
            CursorStep::Done => return result,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PathFrame {
    Quote,
    BulletItem,
}

fn own_paragraph_paths(output: &BlockOutput) -> Vec<Vec<PathFrame>> {
    output
        .leaves()
        .map(|leaf| {
            leaf.context
                .frames()
                .iter()
                .map(|frame| match frame {
                    BlockContainer::BlockQuote => PathFrame::Quote,
                    BlockContainer::BulletItem { .. } => PathFrame::BulletItem,
                })
                .collect()
        })
        .collect()
}

fn pulldown_paragraph_paths(text: &str) -> Vec<Vec<PathFrame>> {
    let mut path = Vec::new();
    let mut result = Vec::new();
    let mut tight_item_pending = false;
    for event in Parser::new(text) {
        match event {
            Event::Start(Tag::BlockQuote(_)) => path.push(PathFrame::Quote),
            Event::End(TagEnd::BlockQuote(_)) => {
                assert_eq!(path.pop(), Some(PathFrame::Quote));
            }
            Event::Start(Tag::Item) => {
                path.push(PathFrame::BulletItem);
                tight_item_pending = true;
            }
            Event::End(TagEnd::Item) => {
                assert_eq!(path.pop(), Some(PathFrame::BulletItem));
                tight_item_pending = false;
            }
            Event::Start(Tag::Paragraph) => {
                result.push(path.clone());
                tight_item_pending = false;
            }
            Event::Text(_) | Event::Code(_) if tight_item_pending => {
                result.push(path.clone());
                tight_item_pending = false;
            }
            _ => {}
        }
    }
    result
}

#[test]
fn supported_quote_list_lazy_and_blank_paragraph_structure_matches_pulldown() {
    let cases = [
        "alpha\nbeta\n\nend\n",
        "> paragraph\ncontinuation\n\nafter\n",
        "> - **alpha**\n>   beta and `gamma`\n",
        "- a\n- b\n\nafter\n",
        "> - item\n>\tcontinued\nlazy\n\nend\n",
    ];
    for text in cases {
        let output = parse_blocks(Arc::new(PersistentSource::from_text(text)), 1);
        assert_eq!(
            own_paragraph_paths(&output),
            pulldown_paragraph_paths(text),
            "paragraph path differential failed for {text:?}"
        );
    }
}

#[test]
fn gate_b_quote_list_leaf_excludes_markers_and_inserts_virtual_line_join() {
    let text = "> - **alpha**\n>   beta and `gamma`\n";
    let source = Arc::new(PersistentSource::from_text(text));
    let output = parse_blocks(source.clone(), 3);
    let leaf = output.leaves().next().unwrap();
    assert_eq!(output.len(), 1);
    assert_eq!(
        leaf.context.frames(),
        &[
            BlockContainer::BlockQuote,
            BlockContainer::BulletItem {
                marker: b'-',
                continuation_indent: 2,
            },
        ]
    );
    assert_eq!(
        logical_bytes(&leaf),
        b"**alpha**\nbeta and `gamma`".to_vec()
    );

    let descriptors = leaf.input.descriptors().collect::<Vec<_>>();
    assert_eq!(descriptors.len(), 3);
    let SegmentDescriptor::Source(first) = &descriptors[0] else {
        panic!("first descriptor must be physical")
    };
    assert_eq!(first.document.start, text.find("**alpha**").unwrap());
    assert_eq!(
        first.first,
        source.anchor_at(text.find("**alpha**").unwrap()).unwrap()
    );
    assert!(matches!(
        descriptors[1],
        SegmentDescriptor::Virtual {
            byte: b'\n',
            count: 1,
            reason: VirtualReason::ContainerLineJoin,
            ..
        }
    ));
    assert!(!logical_bytes(&leaf).windows(2).any(|pair| pair == b"> "));
    assert!(!logical_bytes(&leaf).windows(2).any(|pair| pair == b"- "));
}

#[test]
fn source_to_block_to_shared_lexer_to_real_table_and_emphasis_grammar() {
    let text = "| left *em* | `c\\|d` |\n| :--- | ---: |\n| body | **strong** |";
    let output = parse_blocks(Arc::new(PersistentSource::from_text(text)), 7);
    let leaf = output.leaves().next().unwrap();
    assert_eq!(output.len(), 1);

    let mut lexer = SharedLexer::new(&leaf.input);
    while lexer.poll(MAX_LEXER_POLL_WORK).status != LexerStatus::Ready {}
    let consumers = lexer.consumers().unwrap();
    assert!(consumers
        .table
        .view()
        .shares_root_with(consumers.inline.view()));
    let mut grammar = GrammarJob::new(&consumers).unwrap();
    while grammar.poll(MAX_GRAMMAR_POLL_WORK).status != GrammarStatus::Ready {}
    let result = grammar.result().unwrap();
    assert_eq!(
        result.classification,
        GrammarClassification::Table {
            columns: 2,
            body_rows: 1,
        }
    );
    let records = result.records.records().collect::<Vec<_>>();
    assert!(records
        .iter()
        .any(|record| matches!(record, GrammarRecord::EmphasisStart { .. })));
    assert!(records
        .iter()
        .any(|record| matches!(record, GrammarRecord::StrongStart { .. })));
    assert!(records
        .iter()
        .any(|record| matches!(record, GrammarRecord::TableStart { .. })));
}

#[test]
fn giant_physical_line_yields_and_never_becomes_atomic_line_work() {
    let text = "a".repeat(1024 * 1024);
    let source = Arc::new(PersistentSource::from_text(&text));
    let source_metrics = source.metrics();
    let mut job = BlockJob::new(source);
    let mut polls = 0;
    loop {
        let poll = job.poll(MAX_BLOCK_POLL_WORK);
        polls += 1;
        assert!(poll.work <= MAX_BLOCK_POLL_WORK);
        if poll.status == BlockStatus::Ready {
            break;
        }
        assert_eq!(poll.status, BlockStatus::Pending);
    }
    let receipt = job.result().unwrap().receipt();
    assert!(polls > 250);
    assert_eq!(receipt.source_bytes_inspected, text.len());
    assert_eq!(receipt.prefix_bytes_copied, 1);
    assert!(receipt.prefix_bytes_examined > 0);
    assert!(receipt.max_atomic_prefix_units < 64, "{receipt:?}");
    assert!(receipt.max_atomic_leaf_handles_copied <= 32);
    assert_eq!(receipt.source_piece_transitions, source_metrics.pieces);
    assert!(receipt.source_cursor_tree_nodes_descended > 0);
    assert!(
        receipt.max_atomic_source_cursor_tree_nodes <= source_metrics.depth,
        "{receipt:?} / {source_metrics:?}"
    );
}

#[test]
fn scaled_million_softbreak_shape_allocates_capture_nodes_per_piece_not_per_line() {
    // 100k lines is a scaled receipt for the million-softbreak adversary. The
    // checkpoint count is intentionally O(lines), but direct pending-run
    // transfer means capture tree allocation follows immutable source pieces.
    const LINES: usize = 100_000;
    let text = "a\n".repeat(LINES);
    let source = Arc::new(PersistentSource::from_text(&text));
    let source_metrics = source.metrics();
    let output = parse_blocks(source, MAX_BLOCK_POLL_WORK);
    assert_eq!(output.len(), 1);
    let receipt = output.receipt();
    assert_eq!(receipt.source_capture_bytes_observed, text.len());
    assert_eq!(receipt.source_fragment_payload_bytes_copied, 0);
    assert!(receipt.source_capture_checkpoint_bytes_merged >= text.len() - 2);
    assert!(
        receipt.source_fragment_nodes_allocated <= source_metrics.pieces * 3 + 2,
        "capture allocations must scale with source pieces, not {LINES} lines: {receipt:?}"
    );
    assert!(
        receipt.source_fragment_nodes_allocated < LINES / 100,
        "a throwaway checkpoint node per line has regressed: {receipt:?}"
    );
    assert!(
        receipt.max_atomic_source_capture_nodes_allocated <= usize::BITS as usize,
        "{receipt:?}"
    );
    assert_eq!(
        receipt.source_capture_checkpoint_tree_nodes_examined, 0,
        "single-run line checkpoints must transfer directly without a tree walk"
    );
    assert!(
        receipt.source_index_nodes_examined <= source_metrics.pieces * 3 + source_metrics.depth,
        "sequential source traversal must scale with pieces, not lines: {receipt:?}"
    );
}

#[test]
fn unknown_and_oversized_block_profiles_fail_explicitly_without_fallback() {
    let cases = [
        ("# heading", UnsupportedFeature::AtxHeading),
        ("```code", UnsupportedFeature::FenceOrThematicBreak),
        ("1. ordered", UnsupportedFeature::OrderedList),
        ("    code", UnsupportedFeature::IndentedCode),
        // The tab leaves two virtual indentation columns after the quote's
        // one-column consumption; together with two spaces this is code.
        (">\t  code", UnsupportedFeature::IndentedCode),
        ("bare\rreturn", UnsupportedFeature::BareCarriageReturn),
    ];
    for (text, feature) in cases {
        let mut job = BlockJob::new(Arc::new(PersistentSource::from_text(text)));
        while job.poll(7).status == BlockStatus::Pending {}
        assert!(matches!(
            job.error(),
            Some(BlockError::Unsupported(value)) if value.feature == feature
        ));
        assert!(job.result().is_none());
    }

    let later_indented_code = format!("{}x", " ".repeat(MAX_BLOCK_PREFIX_BYTES + 17));
    let mut job = BlockJob::new(Arc::new(PersistentSource::from_text(&later_indented_code)));
    while job.poll(31).status == BlockStatus::Pending {}
    assert!(matches!(
        job.error(),
        Some(BlockError::Unsupported(value))
            if value.feature == UnsupportedFeature::IndentedCode
    ));
}

#[test]
fn giant_blank_line_discards_its_checkpoint_then_streams_to_newline() {
    let whitespace = " ".repeat(1024 * 1024);
    let text = format!("{whitespace}\nnext");
    let output = parse_blocks(Arc::new(PersistentSource::from_text(&text)), 3);
    let leaf = output.leaves().next().unwrap();
    assert_eq!(output.len(), 1);
    assert_eq!(logical_bytes(&leaf), b"next");
    let next = text.find("next").unwrap();
    assert_eq!(leaf.input.document_window(), next..text.len());
    let receipt = output.receipt();
    assert_eq!(receipt.prefix_bytes_copied, MAX_BLOCK_PREFIX_BYTES + 1);
    assert_eq!(
        receipt.streamed_blank_candidate_bytes,
        whitespace.len() - MAX_BLOCK_PREFIX_BYTES
    );
    assert_eq!(receipt.source_bytes_inspected, text.len());
    assert_eq!(
        receipt.source_capture_bytes_observed,
        MAX_BLOCK_PREFIX_BYTES + 1 + "next".len(),
        "only the bounded blank checkpoint, its LF separator, and real leaf content are captured"
    );
    assert_eq!(receipt.source_boundary_bytes_examined, 0);
    assert_eq!(receipt.source_fragment_payload_bytes_copied, 0);
}

#[test]
fn leaf_ids_are_unique_and_output_pages_do_not_require_a_document_vec() {
    let mut text = String::new();
    for _ in 0..10_000 {
        text.push_str("x\n\n");
    }
    let output = parse_blocks(Arc::new(PersistentSource::from_text(&text)), 31);
    let ids = output.leaves().map(|leaf| leaf.id).collect::<BTreeSet<_>>();
    assert_eq!(output.len(), 10_000);
    assert_eq!(ids.len(), output.len());
    let receipt = output.receipt();
    assert_eq!(receipt.output_pages_sealed, 313);
    assert!(receipt.output_tree_nodes < receipt.leaves_sealed / 8);
    assert_eq!(receipt.max_atomic_leaf_handles_copied, 32);
    assert_eq!(receipt.source_fragment_handles_retained, output.len());
    assert!(
        receipt.retained_block_structure_bytes >= output.len() * 64,
        "Arc<BlockLeaf> remains a heavyweight commitment representation: {receipt:?}"
    );
    assert!(receipt.retained_block_allocations > output.len());
    assert_eq!(receipt.source_fragment_payload_bytes_copied, 0);
    assert!(
        receipt.source_fragment_nodes_allocated > 0,
        "sequential capture must retain bounded source-fragment nodes"
    );
    assert_eq!(
        receipt.source_capture_bytes_observed,
        text.len(),
        "every physical byte is observed by exactly one retained or discarded checkpoint"
    );
    assert_eq!(receipt.source_boundary_bytes_examined, 0, "{receipt:?}");
    assert!(receipt.source_index_nodes_examined < 64, "{receipt:?}");
    assert!(
        receipt.max_atomic_source_capture_checkpoint_bytes <= MAX_BLOCK_CAPTURE_CHECKPOINT_BYTES,
        "{receipt:?}"
    );
    assert_eq!(
        output
            .leaves()
            .map(|leaf| leaf.input.document_window().len())
            .sum::<usize>(),
        10_000,
        "blank separators and terminal line endings must not be retained by leaves"
    );
}

#[test]
fn context_change_uses_exact_content_windows_and_releases_stripped_prefix_buffer() {
    // Place the new line's stripped quote prefix at the end of one immutable
    // source page and its content at the beginning of the next. A block parser
    // that merely retained its whole undecided prefix checkpoint would pin the
    // prefix buffer in the new leaf.
    let first = "a".repeat(MAX_PIECE_BYTES - 3);
    let text = format!("{first}\n> x\n");
    let quote_offset = text.find('>').unwrap();
    let content_offset = text.find('x').unwrap();
    assert_eq!(content_offset, MAX_PIECE_BYTES);
    let source = Arc::new(PersistentSource::from_text(&text));
    let prefix_buffer = source.anchor_at(quote_offset).unwrap().buffer_id;
    let content_buffer = source.anchor_at(content_offset).unwrap().buffer_id;
    assert_ne!(prefix_buffer, content_buffer);

    let output = parse_blocks(source, 3);
    let leaves = output.leaves().collect::<Vec<_>>();
    assert_eq!(leaves.len(), 2);
    assert_eq!(leaves[0].input.document_window(), 0..first.len());
    assert_eq!(
        leaves[1].input.document_window(),
        content_offset..content_offset + 1
    );
    assert_eq!(logical_bytes(&leaves[1]), b"x");
    assert_eq!(
        leaves[1].input.retained_source_buffer_ids(),
        vec![content_buffer],
        "the stripped quote prefix's buffer must not leak into the new leaf"
    );
    let receipt = output.receipt();
    assert_eq!(receipt.source_boundary_bytes_examined, 0, "{receipt:?}");
    assert_eq!(receipt.source_fragment_payload_bytes_copied, 0);
    assert!(receipt.source_capture_prefix_bytes_discarded >= 2);
    assert!(receipt.source_capture_checkpoint_tree_nodes_examined > 0);
}

#[test]
fn adjacent_context_changes_do_not_cross_pin_neighboring_line_prefixes() {
    let text = "one\n> two\n- three\n";
    let output = parse_blocks(Arc::new(PersistentSource::from_text(text)), 1);
    let leaves = output.leaves().collect::<Vec<_>>();
    assert_eq!(leaves.len(), 3);
    for (leaf, expected) in leaves.iter().zip(["one", "two", "three"]) {
        let start = text.find(expected).unwrap();
        assert_eq!(leaf.input.document_window(), start..start + expected.len());
        assert_eq!(logical_bytes(leaf), expected.as_bytes());
    }
    let receipt = output.receipt();
    assert_eq!(receipt.source_capture_bytes_observed, text.len());
    assert_eq!(receipt.source_boundary_bytes_examined, 0);
    assert_eq!(receipt.source_fragment_payload_bytes_copied, 0);
    assert!(receipt.source_capture_prefix_bytes_discarded >= 4);
}

#[test]
fn crlf_container_join_is_captured_only_after_continuation_is_proven() {
    let text = "> - one\r\n>   two\r\n\r\nafter\r\n";
    let content_start = text.find("one").unwrap();
    let content_end = text.find("two").unwrap() + 3;
    let after_start = text.find("after").unwrap();
    let output = parse_blocks(Arc::new(PersistentSource::from_text(text)), 1);
    let leaves = output.leaves().collect::<Vec<_>>();
    assert_eq!(leaves.len(), 2);
    assert_eq!(logical_bytes(&leaves[0]), b"one\ntwo");
    assert_eq!(
        leaves[0].input.document_window(),
        content_start..content_end
    );
    assert_eq!(
        leaves[1].input.document_window(),
        after_start..after_start + 5
    );
    let receipt = output.receipt();
    assert_eq!(receipt.source_capture_bytes_observed, text.len());
    assert_eq!(receipt.source_boundary_bytes_examined, 0);
    assert_eq!(receipt.source_fragment_payload_bytes_copied, 0);
    assert!(receipt.source_capture_checkpoint_bytes_merged >= 2 + 4);
    assert!(
        receipt.max_atomic_source_capture_checkpoint_bytes <= MAX_BLOCK_CAPTURE_CHECKPOINT_BYTES
    );
}

#[test]
fn utf8_continuation_bytes_need_no_invented_boundary_certificates() {
    let text = "> café 🙂\ncontinued β\n";
    let content_start = text.find("café").unwrap();
    let content_end = text.find('β').unwrap() + 'β'.len_utf8();
    let output = parse_blocks(Arc::new(PersistentSource::from_text(text)), 1);
    let leaf = output.leaves().next().unwrap();
    assert_eq!(output.len(), 1);
    assert_eq!(logical_bytes(&leaf), "café 🙂\ncontinued β".as_bytes());
    assert_eq!(leaf.input.document_window(), content_start..content_end);
    let receipt = output.receipt();
    assert_eq!(receipt.source_boundary_bytes_examined, 0, "{receipt:?}");
    assert_eq!(receipt.source_fragment_payload_bytes_copied, 0);
}

#[test]
fn poll_exposes_multidimensional_delta_and_is_not_yet_scheduler_admissible() {
    let source = Arc::new(PersistentSource::from_text("> - alpha"));
    let mut job = BlockJob::new(source);
    let poll = job.poll(7);
    assert_eq!(poll.work, 7);
    assert_eq!(poll.receipt_delta.parser_transitions, 7);
    assert_eq!(poll.receipt_delta.source_bytes_inspected, 7);
    assert!(poll.receipt_delta.prefix_bytes_examined > poll.work);
    assert!(poll.receipt_delta.max_atomic_prefix_units > 0);
}

#[test]
fn retained_leaf_drops_exact_source_root_but_keeps_bounded_source_bytes() {
    let source = Arc::new(PersistentSource::from_text("one\n\ntwo\n"));
    let weak: Weak<PersistentSource> = Arc::downgrade(&source);
    let output = parse_blocks(source.clone(), 2);
    let retained_leaf = output.leaves().next().unwrap();
    drop(output);
    drop(source);
    assert!(
        weak.upgrade().is_none(),
        "SegmentedLeaf must not pin the complete source root"
    );
    assert_eq!(logical_bytes(&retained_leaf), b"one".to_vec());
    assert_eq!(
        retained_leaf
            .input
            .retained_source_metrics()
            .referenced_piece_bytes,
        3
    );
    drop(retained_leaf);
    assert!(weak.upgrade().is_none());
}

#[test]
fn unchanged_far_suffix_descriptor_recovers_the_same_stable_anchor_after_edit() {
    let text = format!("{} tail", "a".repeat(8 * 1024));
    let old = PersistentSource::from_text(&text);
    let old_output = parse_blocks(Arc::new(old.clone()), 17);
    let old_leaf = old_output.leaves().next().unwrap();
    let old_last = match old_leaf.input.descriptors().last().unwrap() {
        SegmentDescriptor::Source(span) => span.last,
        SegmentDescriptor::Virtual { .. } => panic!("last descriptor is physical"),
    };

    let edited = old.edit(0..1, "b").unwrap().source;
    let new_output = parse_blocks(Arc::new(edited), 17);
    let new_leaf = new_output.leaves().next().unwrap();
    let new_last = match new_leaf.input.descriptors().last().unwrap() {
        SegmentDescriptor::Source(span) => span.last,
        SegmentDescriptor::Virtual { .. } => panic!("last descriptor is physical"),
    };
    assert_eq!(old_last, new_last);
    assert_ne!(old_output.source_identity(), new_output.source_identity());
}

#[test]
fn tab_bytes_inside_inline_content_remain_physical_source_origins() {
    let text = "> - alpha\tbeta\n>   gamma";
    let source = Arc::new(PersistentSource::from_text(text));
    let output = parse_blocks(source.clone(), 5);
    let leaf = output.leaves().next().unwrap();
    let mut cursor = leaf.input.cursor();
    let mut tab_origin = None;
    loop {
        match cursor.step() {
            CursorStep::Byte(byte) if byte.byte == b'\t' => {
                tab_origin = Some(byte.origin);
                break;
            }
            CursorStep::Byte(_) | CursorStep::Progress => {}
            CursorStep::Done => break,
        }
    }
    assert_eq!(
        tab_origin,
        Some(LogicalOrigin::Source(
            source.anchor_at(text.find('\t').unwrap()).unwrap()
        ))
    );
    let receipt = output.receipt();
    assert_eq!(receipt.source_boundary_bytes_examined, 0, "{receipt:?}");
    assert_eq!(receipt.source_fragment_payload_bytes_copied, 0);
}

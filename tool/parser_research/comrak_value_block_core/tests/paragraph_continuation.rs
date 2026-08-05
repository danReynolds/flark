use std::fs;
use std::path::{Path, PathBuf};

use comrak::block_spine_facade::{reference_definitions, table_row};
use flark_comrak_value_block_core::checkpoint::{
    BlockCheckpoint, BlockTransitionKind, MaterializationCursor, OpenOutputBindings,
    ParagraphSetextState, ParagraphTableCertification, ParagraphTableState, PhysicalLine,
    ReferencePrefixOutputState, ResumableValueBlockParser, StructuralEvent, WriteOnlyBlockSink,
    reconstruct_checkpoint,
};
use flark_comrak_value_block_core::source::SourceDocument;
use flark_comrak_value_block_core::{BlockKind, SyntaxProfile, parse_document};
use flark_oversized_block_line_gate::{CancellationToken, Poll, ReferencePrefixJob, TableRowJob};
use serde::Deserialize;

const MIB: usize = 1024 * 1024;
const POLL_BYTES: usize = 4096;

#[derive(Default)]
struct DropSink;

impl WriteOnlyBlockSink for DropSink {
    fn emit(&mut self, _event: StructuralEvent) {}
}

#[derive(Clone)]
struct Paused {
    checkpoint: BlockCheckpoint,
    bindings: OpenOutputBindings,
    cursor: MaterializationCursor,
}

fn pause(source: &str, profile: SyntaxProfile) -> Paused {
    let source = SourceDocument::new(source);
    let mut parser = ResumableValueBlockParser::begin(profile);
    let mut sink = DropSink;
    for leaf in &source.leaves {
        parser
            .push_line(
                PhysicalLine {
                    coverage_leaf_id: leaf.id,
                    absolute_start: leaf.absolute_start,
                    text: &leaf.text,
                },
                &mut sink,
            )
            .expect("prefix line");
    }
    let (checkpoint, bindings, cursor) = parser.pause(&mut sink).expect("pause");
    Paused {
        checkpoint,
        bindings,
        cursor,
    }
}

fn paragraph_state(
    checkpoint: &BlockCheckpoint,
) -> (
    usize,
    flark_comrak_value_block_core::checkpoint::ParagraphTransitionState,
) {
    checkpoint
        .transition_checkpoint()
        .frames
        .into_iter()
        .enumerate()
        .find_map(|(index, frame)| match frame.kind {
            BlockTransitionKind::Paragraph(state) => Some((index, state)),
            _ => None,
        })
        .expect("open paragraph")
}

fn proof_checkpoint_with_paragraph_logical(
    seed: &str,
    logical: String,
    profile: SyntaxProfile,
) -> BlockCheckpoint {
    let mut checkpoint = pause(seed, profile).checkpoint;
    let frame = checkpoint
        .frames
        .iter_mut()
        .find(|frame| matches!(frame.kind, BlockKind::Paragraph))
        .expect("seed paragraph");
    frame.pending.logical = logical;
    // This is a transition/output-partition witness, not a resumable parser
    // receipt: the current atomic facade rejects oversized physical lines
    // before it can create exact origin runs for them.
    frame.pending.origins.clear();
    frame.pending.line_offsets.clear();
    checkpoint
}

fn complete<T>(mut poll: impl FnMut() -> Poll<T>) -> T {
    loop {
        match poll() {
            Poll::Pending { .. } => {}
            Poll::Ready { value, .. } => return value,
            Poll::Cancelled { .. } => panic!("unexpected cancellation"),
        }
    }
}

#[test]
fn equal_gfm_header_grammar_keeps_preface_as_composed_output() {
    let left = pause(
        "old preface one\nold preface two\n| left | right |\n",
        SyntaxProfile::Gfm,
    );
    let right = pause(
        "new preface A\nnew preface B\n| left | right |\n",
        SyntaxProfile::Gfm,
    );
    let no_preface = pause("| left | right |\n", SyntaxProfile::Gfm);
    assert_ne!(left.checkpoint, right.checkpoint);
    assert!(
        left.checkpoint
            .transition_checkpoint()
            .is_grammar_compatible_for_suffix_reuse(&right.checkpoint.transition_checkpoint()),
        "preface bytes and origins must not pin the table transition"
    );
    assert!(
        left.checkpoint
            .transition_checkpoint()
            .is_grammar_compatible_for_suffix_reuse(&no_preface.checkpoint.transition_checkpoint()),
        "preface presence is a composed output splice, not a grammar branch"
    );

    let (grammar, output, receipt) = left.checkpoint.clone().into_reuse_parts();
    assert_eq!(receipt.retained_paragraph_payload_bytes, 0);
    let paragraph = output
        .frames
        .iter()
        .find_map(|frame| frame.paragraph.as_ref())
        .expect("paragraph output accumulator");
    assert!(paragraph.preface.is_some());
    assert!(paragraph.last_line.is_some());
    assert!(matches!(
        paragraph.reference_prefix,
        ReferencePrefixOutputState::Certified {
            consumed_prefix: 0,
            visible_remainder: true
        }
    ));
    assert_eq!(
        reconstruct_checkpoint(&grammar, output).expect("reconstruct"),
        left.checkpoint
    );

    for source in [
        "old preface one\nold preface two\n| left | right |\n| --- | --- |\n",
        "new preface A\nnew preface B\n| left | right |\n| --- | --- |\n",
    ] {
        let document = parse_document(source, SyntaxProfile::Gfm).expect("GFM table parse");
        let root = document.tree.node(document.tree.root);
        assert_eq!(root.children.len(), 2, "preface and table remain siblings");
        assert!(matches!(
            document.tree.node(root.children[0]).kind,
            BlockKind::Paragraph
        ));
        assert!(matches!(
            document.tree.node(root.children[1]).kind,
            BlockKind::Table(_)
        ));
        assert_eq!(
            document
                .tree
                .node(root.children[0])
                .content
                .logical
                .lines()
                .count(),
            2
        );
    }

    let document = parse_document("| left | right |\n| --- | --- |\n", SyntaxProfile::Gfm)
        .expect("GFM table without preface");
    let root = document.tree.node(document.tree.root);
    assert_eq!(root.children.len(), 1);
    assert!(matches!(
        document.tree.node(root.children[0]).kind,
        BlockKind::Table(_)
    ));
}

#[test]
fn giant_paragraph_projection_is_bounded_and_oversized_last_line_is_refillable() {
    let mut left = String::with_capacity(MIB + 64);
    let mut right = String::with_capacity(MIB + 64);
    for index in 0..1024 {
        left.push_str(&format!(
            "{}\n",
            if index == 3 {
                "a".repeat(1022)
            } else {
                "x".repeat(1023)
            }
        ));
        right.push_str(&format!(
            "{}\n",
            if index == 3 {
                "b".repeat(1022)
            } else {
                "x".repeat(1023)
            }
        ));
    }
    left.push_str("| left | right |\n");
    right.push_str("| left | right |\n");
    assert!(left.len() >= MIB && right.len() >= MIB);

    let left = pause(&left, SyntaxProfile::Gfm);
    let right = pause(&right, SyntaxProfile::Gfm);
    let (left_key, receipt) = left.checkpoint.transition_checkpoint_with_receipt();
    let right_key = right.checkpoint.transition_checkpoint();
    assert_eq!(receipt.retained_paragraph_payload_bytes, 0);
    assert_eq!(receipt.uncertified_paragraphs, 0);
    assert!(receipt.maximum_paragraph_bytes_inspected <= 2 * 8192 + 1);
    assert!(left_key.is_grammar_compatible_for_suffix_reuse(&right_key));
    let (_, state) = paragraph_state(&left.checkpoint);
    assert_eq!(state.setext, ParagraphSetextState::VisibleContent);
    assert_eq!(
        state.table_header,
        ParagraphTableState::Eligible { columns: 2 }
    );

    let giant_line = format!("| {} | right |\n", "z".repeat(MIB));
    let first = proof_checkpoint_with_paragraph_logical(
        "| seed | right |\n",
        giant_line.clone(),
        SyntaxProfile::Gfm,
    );
    let second = first.clone();
    let (frame, state) = paragraph_state(&first);
    assert_eq!(
        state.table_header,
        ParagraphTableState::UnknownOversizedLine
    );
    let mut first_key = first.transition_checkpoint();
    let mut second_key = second.transition_checkpoint();
    assert!(!first_key.is_grammar_compatible_for_suffix_reuse(&second_key));

    let token = CancellationToken::default();
    let mut job = TableRowJob::new(giant_line.as_bytes());
    let summary = complete(|| job.poll(giant_line.as_bytes(), POLL_BYTES, &token))
        .expect("giant table-shaped line");
    assert_eq!(summary.cells.len(), 2);
    assert!(job.receipt().maximum_bytes_per_poll <= POLL_BYTES);
    let certification = ParagraphTableCertification::Eligible { columns: 2 };
    first_key
        .certify_paragraph_table_header(frame, certification)
        .expect("first certification");
    second_key
        .certify_paragraph_table_header(frame, certification)
        .expect("second certification");
    assert!(first_key.is_grammar_compatible_for_suffix_reuse(&second_key));
}

#[test]
fn giant_leading_reference_is_source_visible_but_conservatively_nonconvergent() {
    let source = format!("[label]: /{}\n", "u".repeat(MIB));
    let checkpoint = proof_checkpoint_with_paragraph_logical(
        "[label]: /u\n",
        source.clone(),
        SyntaxProfile::CommonMark,
    );
    let (key, receipt) = checkpoint.transition_checkpoint_with_receipt();
    let (_, state) = paragraph_state(&checkpoint);
    assert_eq!(
        state.setext,
        ParagraphSetextState::UnknownLeadingReferencePrefix
    );
    assert_eq!(receipt.uncertified_paragraphs, 1);
    assert_eq!(receipt.retained_paragraph_payload_bytes, 0);
    assert!(!key.is_grammar_compatible_for_suffix_reuse(&key));

    let (_, output, _) = checkpoint.into_reuse_parts();
    let paragraph = output
        .frames
        .iter()
        .find_map(|frame| frame.paragraph.as_ref())
        .expect("paragraph output");
    assert!(matches!(
        paragraph.reference_prefix,
        ReferencePrefixOutputState::Unknown { .. }
    ));

    let token = CancellationToken::default();
    let mut job = ReferencePrefixJob::new();
    assert!(complete(|| job.poll(source.as_bytes(), POLL_BYTES, &token)).is_some());
    assert!(job.receipt().maximum_bytes_per_poll <= POLL_BYTES);
    // One recognized definition is not yet a production cursor for repeated
    // definitions and provisional EOF/title fallback, so no unsafe scalar
    // certification API exists for this state.
}

#[test]
fn grammar_plus_changed_output_reconstructs_the_new_list_prefix_only() {
    let old = pause("1. old payload\n", SyntaxProfile::CommonMark);
    let new = pause("7. new payload\n", SyntaxProfile::CommonMark);
    let old_grammar = old.checkpoint.transition_checkpoint();
    let expected_new = new.checkpoint.clone();
    let (new_grammar, new_output, _) = new.checkpoint.into_reuse_parts();
    assert!(old_grammar.is_grammar_compatible_for_suffix_reuse(&new_grammar));

    let rebuilt = reconstruct_checkpoint(&old_grammar, new_output).expect("compatible splice");
    assert_eq!(rebuilt, expected_new);
    let list = rebuilt
        .frames
        .iter()
        .find_map(|frame| match &frame.kind {
            BlockKind::List(list) => Some(list),
            _ => None,
        })
        .expect("list frame");
    assert_eq!(list.start, 7);
    let paragraph = rebuilt
        .frames
        .iter()
        .find(|frame| matches!(frame.kind, BlockKind::Paragraph))
        .expect("paragraph frame");
    assert_eq!(paragraph.pending.logical, "new payload\n");

    let bullet = pause("- incompatible\n", SyntaxProfile::CommonMark);
    let (_, bullet_output, _) = bullet.checkpoint.into_reuse_parts();
    assert!(reconstruct_checkpoint(&old_grammar, bullet_output).is_err());

    let (_, mut stale_cursor_output, _) = expected_new.clone().into_reuse_parts();
    stale_cursor_output
        .frames
        .iter_mut()
        .find_map(|frame| frame.paragraph.as_mut())
        .expect("paragraph cursor")
        .logical
        .end -= 1;
    assert!(reconstruct_checkpoint(&old_grammar, stale_cursor_output).is_err());

    // Runtime bindings/cursor remain revision-local and come from the selected
    // output revision, never from the old grammar root.
    let resumed = ResumableValueBlockParser::resume(rebuilt, new.bindings, new.cursor)
        .expect("resume rebuilt new prefix");
    let mut sink = DropSink;
    resumed.finish(&mut sink).expect("finish rebuilt prefix");
}

#[derive(Deserialize)]
struct CorpusFixture {
    markdown: String,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn load_corpus(path: &Path) -> Vec<CorpusFixture> {
    serde_json::from_slice(&fs::read(path).expect("read corpus")).expect("decode corpus")
}

#[test]
fn all_1322_spec_fixtures_have_exact_bounded_paragraph_decisions() {
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
    let mut fixtures = 0;
    let mut paragraph_boundaries = 0;
    let mut multiline_paragraphs = 0;
    let mut eligible_headers = 0;

    for (path, profile) in corpora {
        for fixture in load_corpus(&path) {
            let source = SourceDocument::new(&fixture.markdown);
            let mut parser = ResumableValueBlockParser::begin(profile);
            let mut sink = DropSink;
            for leaf in &source.leaves {
                parser
                    .push_line(
                        PhysicalLine {
                            coverage_leaf_id: leaf.id,
                            absolute_start: leaf.absolute_start,
                            text: &leaf.text,
                        },
                        &mut sink,
                    )
                    .expect("corpus line");
                let (checkpoint, bindings, cursor) = parser.pause(&mut sink).expect("pause");
                let key = checkpoint.transition_checkpoint();
                assert!(key.is_fully_certified(), "spec paragraph must be bounded");
                for (frame, projected) in checkpoint.frames.iter().zip(&key.frames) {
                    let BlockTransitionKind::Paragraph(actual) = projected.kind else {
                        continue;
                    };
                    paragraph_boundaries += 1;
                    multiline_paragraphs += usize::from(frame.pending.logical.lines().count() > 1);

                    let definitions = reference_definitions(&frame.pending.logical)
                        .expect("bounded spec reference scan");
                    let consumed = definitions
                        .last()
                        .map_or(0, |definition| definition.source.end);
                    let expected_setext = if frame.pending.logical[consumed..]
                        .bytes()
                        .any(|byte| !byte.is_ascii_whitespace())
                    {
                        ParagraphSetextState::VisibleContent
                    } else {
                        ParagraphSetextState::DefinitionsOnlyOrBlank
                    };
                    assert_eq!(actual.setext, expected_setext);

                    let expected_table =
                        if profile == SyntaxProfile::CommonMark || frame.table_visited {
                            ParagraphTableState::NotApplicable
                        } else {
                            match table_row(&frame.pending.logical, false)
                                .expect("bounded spec table scan")
                            {
                                Some(row) => ParagraphTableState::Eligible {
                                    columns: u32::try_from(row.cells.len()).expect("cell count"),
                                },
                                None => ParagraphTableState::Ineligible,
                            }
                        };
                    eligible_headers += usize::from(matches!(
                        expected_table,
                        ParagraphTableState::Eligible { .. }
                    ));
                    assert_eq!(actual.table_header, expected_table);
                }
                parser = ResumableValueBlockParser::resume(checkpoint, bindings, cursor)
                    .expect("resume corpus");
            }
            parser.finish(&mut sink).expect("finish corpus");
            fixtures += 1;
        }
    }

    assert_eq!(fixtures, 1322);
    assert!(paragraph_boundaries > 1000);
    assert!(multiline_paragraphs > 100);
    assert!(eligible_headers > 20);
}

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use flark_comrak_value_block_core::source::SourceDocument;
use flark_comrak_value_block_core::{
    BlockDocument, FuelledValueBlockParser, SyntaxProfile, WorkBudget, WorkPollReceipt, WorkStatus,
    normalized_html, parse_document,
};
use serde::Deserialize;

const POLL_BUDGET: WorkBudget = WorkBudget::new(256, 256, 256);

#[derive(Deserialize)]
struct CorpusFixture {
    markdown: String,
    example: usize,
    section: String,
}

#[derive(Clone, Copy, Debug, Default)]
struct AggregateReceipt {
    polls: usize,
    max_transitions: usize,
    max_events: usize,
    max_generated_events: usize,
    max_transition_event_fanout: usize,
    max_index_operations: usize,
    max_poll: Duration,
}

impl AggregateReceipt {
    fn add(&mut self, receipt: WorkPollReceipt, elapsed: Duration) {
        assert_eq!(receipt.open_frames_copied, 0);
        self.polls += 1;
        self.max_transitions = self.max_transitions.max(receipt.transitions);
        self.max_events = self.max_events.max(receipt.output_events);
        self.max_generated_events = self
            .max_generated_events
            .max(receipt.generated_output_events);
        self.max_transition_event_fanout = self
            .max_transition_event_fanout
            .max(receipt.max_transition_event_fanout);
        self.max_index_operations = self.max_index_operations.max(receipt.index_operations);
        self.max_poll = self.max_poll.max(elapsed);
    }

    fn assert_bounded(&self) {
        assert!(self.max_transitions <= POLL_BUDGET.transitions, "{self:?}");
        assert!(self.max_events <= POLL_BUDGET.output_events, "{self:?}");
        assert!(
            self.max_generated_events <= POLL_BUDGET.output_events,
            "atomic event producer exceeded the measured grant: {self:?}"
        );
        assert!(
            self.max_index_operations <= POLL_BUDGET.index_operations,
            "{self:?}"
        );
    }
}

fn fuelled(
    source: &str,
    profile: SyntaxProfile,
) -> (BlockDocument, AggregateReceipt, AggregateReceipt) {
    let source_document = SourceDocument::new(source);
    let mut parser = FuelledValueBlockParser::new(source, profile);
    let mut line_receipt = AggregateReceipt::default();
    for leaf in source_document.leaves {
        parser
            .begin_line(leaf.id, leaf.text)
            .expect("begin physical line");
        loop {
            let started = Instant::now();
            let receipt = parser.poll_line(POLL_BUDGET).expect("poll line");
            line_receipt.add(receipt, started.elapsed());
            if receipt.status == WorkStatus::Complete {
                break;
            }
            assert_eq!(receipt.status, WorkStatus::Pending);
        }
    }

    parser.begin_finish().expect("begin finish");
    let mut finish_receipt = AggregateReceipt::default();
    loop {
        let started = Instant::now();
        let receipt = parser.poll_finish(POLL_BUDGET).expect("poll finish");
        finish_receipt.add(receipt, started.elapsed());
        if receipt.status == WorkStatus::Complete {
            break;
        }
        assert_eq!(receipt.status, WorkStatus::Pending);
    }
    (
        parser.into_document().expect("completed document"),
        line_receipt,
        finish_receipt,
    )
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn load_corpus(path: &Path) -> Vec<CorpusFixture> {
    serde_json::from_slice(&fs::read(path).expect("read corpus")).expect("decode corpus")
}

fn assert_exact(source: &str, profile: SyntaxProfile) {
    let expected = parse_document(source, profile).expect("atomic parse");
    let (actual, line, finish) = fuelled(source, profile);
    assert_eq!(actual, expected, "structural output for {source:?}");
    assert_eq!(
        normalized_html(&actual).expect("fuelled html"),
        normalized_html(&expected).expect("atomic html"),
        "rendered output for {source:?}"
    );
    line.assert_bounded();
    finish.assert_bounded();
}

#[test]
fn ordinary_lines_complete_in_one_poll() {
    let source = "# heading\n\nparagraph *with inline text*\n\n- one\n- two\n";
    let source_document = SourceDocument::new(source);
    let line_count = source_document.leaves.len();
    let mut parser = FuelledValueBlockParser::new(source, SyntaxProfile::CommonMark);
    for leaf in source_document.leaves {
        parser.begin_line(leaf.id, leaf.text).expect("begin line");
        let receipt = parser.poll_line(POLL_BUDGET).expect("one poll");
        assert_eq!(receipt.status, WorkStatus::Complete, "{receipt:?}");
        assert_eq!(receipt.open_frames_copied, 0);
    }
    parser.begin_finish().expect("begin finish");
    while parser.poll_finish(POLL_BUDGET).expect("finish").status != WorkStatus::Complete {}
    let actual = parser.into_document().expect("document");
    let expected = parse_document(source, SyntaxProfile::CommonMark).expect("atomic");
    assert_eq!(actual, expected);
    assert_eq!(line_count, 6);
}

#[test]
fn document_cannot_escape_before_finish_completes() {
    let source = "paragraph\n";
    let source_document = SourceDocument::new(source);
    let leaf = source_document.leaves.into_iter().next().expect("line");
    let mut parser = FuelledValueBlockParser::new(source, SyntaxProfile::CommonMark);
    parser.begin_line(leaf.id, leaf.text).expect("begin");
    while parser.poll_line(POLL_BUDGET).expect("line").status != WorkStatus::Complete {}
    let error = parser
        .into_document()
        .expect_err("unfinished document must remain owned by the parser");
    assert_eq!(
        error,
        flark_comrak_value_block_core::ParseError::Invariant("fuelled parser is not complete")
    );
}

#[test]
fn fuelled_control_is_exact_for_adversarial_cross_construct_transitions() {
    let cases = [
        (SyntaxProfile::CommonMark, "- a\n\n  - b\n\n    c\n- d\n"),
        (SyntaxProfile::CommonMark, "alpha\n===\n\nbeta\n---\n"),
        (
            SyntaxProfile::Gfm,
            "| foo | bar |\n| --- | --- |\n| baz | bim |\n",
        ),
        (
            SyntaxProfile::Gfm,
            "- [x] task\n  > quote\n  > lazy\n\n  continuation\n",
        ),
        (
            SyntaxProfile::CommonMark,
            "> para\n> lazy\n\n    code\n\n[ref]: /url \"title\"\n",
        ),
    ];
    for (profile, source) in cases {
        assert_exact(source, profile);
    }
}

#[test]
fn dense_table_event_delivery_is_capped_and_atomic_generation_is_visible() {
    let columns = 300;
    let header = (0..columns)
        .map(|index| format!("h{index}"))
        .collect::<Vec<_>>()
        .join(" | ");
    let delimiter = std::iter::repeat_n("---", columns)
        .collect::<Vec<_>>()
        .join(" | ");
    let row = std::iter::repeat_n("x", columns)
        .collect::<Vec<_>>()
        .join(" | ");
    let source = format!("{header}\n{delimiter}\n{row}\n");
    let source_document = SourceDocument::new(&source);
    let mut parser = FuelledValueBlockParser::new(&source, SyntaxProfile::Gfm);
    let mut max_delivered = 0;
    let mut max_generated = 0;
    let mut max_fanout = 0;
    for leaf in source_document.leaves {
        parser.begin_line(leaf.id, leaf.text).expect("begin");
        loop {
            let receipt = parser.poll_line(POLL_BUDGET).expect("poll");
            max_delivered = max_delivered.max(receipt.output_events);
            max_generated = max_generated.max(receipt.generated_output_events);
            max_fanout = max_fanout.max(receipt.max_transition_event_fanout);
            if receipt.status == WorkStatus::Complete {
                break;
            }
        }
    }
    parser.begin_finish().expect("finish");
    loop {
        let receipt = parser.poll_finish(POLL_BUDGET).expect("finish poll");
        max_delivered = max_delivered.max(receipt.output_events);
        max_generated = max_generated.max(receipt.generated_output_events);
        max_fanout = max_fanout.max(receipt.max_transition_event_fanout);
        if receipt.status == WorkStatus::Complete {
            break;
        }
    }
    let actual = parser.into_document().expect("document");
    let expected = parse_document(&source, SyntaxProfile::Gfm).expect("atomic");
    assert_eq!(actual, expected);
    assert!(max_delivered <= POLL_BUDGET.output_events);
    assert!(max_generated > POLL_BUDGET.output_events);
    assert!(max_fanout > POLL_BUDGET.output_events);
    eprintln!(
        "DENSE_TABLE columns={columns} max_delivered={max_delivered} max_generated={max_generated} max_transition_fanout={max_fanout}"
    );
}

#[test]
fn fuelled_control_is_exact_over_full_1322_fixture_corpus() {
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
                    "atomic parse failed for {} example {}: {error:?}",
                    fixture.section, fixture.example
                )
            });
            let (actual, line, finish) = fuelled(&fixture.markdown, profile);
            assert_eq!(
                actual, expected,
                "{} example {}",
                fixture.section, fixture.example
            );
            line.assert_bounded();
            finish.assert_bounded();
            compared += 1;
        }
    }
    assert_eq!(compared, 1_322);
}

#[test]
fn deep_quote_and_list_work_is_split_into_bounded_polls() {
    for depth in [1_000_usize, 5_000, 20_000] {
        let source = format!("{}x\n", "> ".repeat(depth));
        {
            let label = "quote";
            let expected = parse_document(&source, SyntaxProfile::CommonMark).expect("atomic");
            let (actual, line, finish) = fuelled(&source, SyntaxProfile::CommonMark);
            assert_eq!(actual, expected, "{label} depth={depth}");
            line.assert_bounded();
            finish.assert_bounded();
            eprintln!(
                "FUELLED_DEPTH kind={label} depth={depth} bytes={} line={line:?} finish={finish:?}",
                source.len()
            );
            assert!(line.polls > depth / POLL_BUDGET.transitions);
            assert!(finish.polls > depth / POLL_BUDGET.transitions);
        }

        let list_source = format!("{}x\n", "- ".repeat(depth));
        if depth == 1_000 {
            let expected =
                parse_document(&list_source, SyntaxProfile::CommonMark).expect("atomic list");
            let (actual, line, finish) = fuelled(&list_source, SyntaxProfile::CommonMark);
            assert_eq!(actual, expected, "list depth={depth}");
            line.assert_bounded();
            finish.assert_bounded();
            eprintln!(
                "FUELLED_DEPTH kind=list requested_depth={depth} parser_depth_cap=100 bytes={} line={line:?} finish={finish:?}",
                list_source.len()
            );
        } else {
            let started = Instant::now();
            let error = parse_document(&list_source, SyntaxProfile::CommonMark)
                .expect_err("current fixed scanner must reject oversized list line");
            let source_document = SourceDocument::new(&list_source);
            let leaf = source_document.leaves.into_iter().next().expect("one line");
            let mut parser = FuelledValueBlockParser::new(&list_source, SyntaxProfile::CommonMark);
            parser.begin_line(leaf.id, leaf.text).expect("begin list");
            let fuelled_error = parser
                .poll_line(POLL_BUDGET)
                .expect_err("fuelled path must preserve the fixed scanner error");
            assert_eq!(fuelled_error, error);
            eprintln!(
                "FUELLED_DEPTH kind=list requested_depth={depth} parser_depth_cap=100 bytes={} rejected_us={} error={error:?}",
                list_source.len(),
                started.elapsed().as_micros()
            );
        }
    }
}

#[test]
fn deep_open_path_matching_and_deindent_closing_are_also_cooperative() {
    let depth = 20_000;
    let prefix = "> ".repeat(depth);
    let source = format!("{prefix}first\n{prefix}second\noutside\n");
    let expected = parse_document(&source, SyntaxProfile::CommonMark).expect("atomic");
    let (actual, line, finish) = fuelled(&source, SyntaxProfile::CommonMark);
    assert_eq!(actual, expected);
    line.assert_bounded();
    finish.assert_bounded();
    eprintln!(
        "FUELLED_CONTINUE depth={depth} bytes={} line={line:?} finish={finish:?}",
        source.len()
    );
    // Opening, matching the retained path, clearing its ancestor flags, and
    // closing it on deindent must all have yielded rather than hiding in one
    // physical-line call.
    assert!(line.polls > (depth * 3) / POLL_BUDGET.transitions);
}

#[test]
fn cancellation_abandons_scheduling_without_copy_but_defers_tree_reclaim() {
    let source = format!("{}x\n", "> ".repeat(20_000));
    let source_document = SourceDocument::new(&source);
    let leaf = source_document.leaves.into_iter().next().expect("one line");
    let mut parser = FuelledValueBlockParser::new(&source, SyntaxProfile::CommonMark);
    parser.begin_line(leaf.id, leaf.text).expect("begin");
    let first = parser.poll_line(POLL_BUDGET).expect("first poll");
    assert_eq!(first.status, WorkStatus::Pending);
    let cancelled = parser.cancel();
    assert!(cancelled.abandoned_line);
    assert!(!cancelled.abandoned_finish);
    assert_eq!(cancelled.open_frames_copied, 0);
    assert!(cancelled.tree_nodes_awaiting_reclaim > 1);
    assert!(!cancelled.tree_reclaim_is_fuelled);
    let after = parser.poll_line(POLL_BUDGET).expect("cancelled poll");
    assert_eq!(after.status, WorkStatus::Cancelled);
    assert_eq!(after.transitions, 0);
    assert_eq!(after.output_events, 0);
    assert_eq!(after.index_operations, 0);
}

#[test]
fn cancelled_tree_reclaim_is_measured_as_a_separate_owner_kernel() {
    let source = format!("{}x\n", "> ".repeat(20_000));
    let source_document = SourceDocument::new(&source);
    let leaf = source_document.leaves.into_iter().next().expect("one line");
    let mut parser = FuelledValueBlockParser::new(&source, SyntaxProfile::CommonMark);
    parser.begin_line(leaf.id, leaf.text).expect("begin");
    while parser.poll_line(POLL_BUDGET).expect("line").status != WorkStatus::Complete {}
    parser.begin_finish().expect("begin finish");
    let first_finish = parser.poll_finish(POLL_BUDGET).expect("finish poll");
    assert_eq!(first_finish.status, WorkStatus::Pending);

    let started = Instant::now();
    let receipt = parser.cancel();
    let cancel_elapsed = started.elapsed();
    assert_eq!(receipt.open_frames_copied, 0);
    assert_eq!(receipt.tree_nodes_awaiting_reclaim, 20_002);
    assert!(!receipt.tree_reclaim_is_fuelled);

    let started = Instant::now();
    drop(parser);
    let reclaim_elapsed = started.elapsed();
    eprintln!(
        "CANCEL_RECLAIM depth=20000 nodes={} cancel_us={} unmetered_drop_us={}",
        receipt.tree_nodes_awaiting_reclaim,
        cancel_elapsed.as_micros(),
        reclaim_elapsed.as_micros()
    );
}

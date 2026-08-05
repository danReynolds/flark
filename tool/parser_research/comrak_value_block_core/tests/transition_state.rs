use flark_comrak_value_block_core::checkpoint::{
    BlockCheckpoint, BlockTransitionKind, MaterializationCursor, OpenOutputBindings, PhysicalLine,
    ResumableValueBlockParser, StructuralEvent, WriteOnlyBlockSink,
};
use flark_comrak_value_block_core::tree::ChildSequenceFold;
use flark_comrak_value_block_core::{BlockKind, SyntaxProfile};

#[derive(Default)]
struct DropSink;

impl WriteOnlyBlockSink for DropSink {
    fn emit(&mut self, _event: StructuralEvent) {}
}

#[derive(Default)]
struct EventSink(Vec<StructuralEvent>);

impl WriteOnlyBlockSink for EventSink {
    fn emit(&mut self, event: StructuralEvent) {
        self.0.push(event);
    }
}

#[derive(Clone)]
struct Paused {
    checkpoint: BlockCheckpoint,
    bindings: OpenOutputBindings,
    cursor: MaterializationCursor,
    absolute_start: usize,
    next_leaf_id: u64,
}

fn pause_lines(profile: SyntaxProfile, lines: &[String], first_leaf_id: u64) -> Paused {
    let mut parser = ResumableValueBlockParser::begin(profile);
    let mut sink = DropSink;
    let mut absolute_start = 0;
    for (index, line) in lines.iter().enumerate() {
        parser
            .push_line(
                PhysicalLine {
                    coverage_leaf_id: first_leaf_id
                        + u64::try_from(index).expect("test line count fits u64"),
                    absolute_start,
                    text: line,
                },
                &mut sink,
            )
            .expect("prefix line");
        absolute_start += line.len();
    }
    let (checkpoint, bindings, cursor) = parser.pause(&mut sink).expect("pause");
    Paused {
        checkpoint,
        bindings,
        cursor,
        absolute_start,
        next_leaf_id: first_leaf_id + u64::try_from(lines.len()).expect("test lines fit u64"),
    }
}

fn pause(profile: SyntaxProfile, lines: &[&str]) -> Paused {
    pause_lines(
        profile,
        &lines
            .iter()
            .map(|line| (*line).to_owned())
            .collect::<Vec<_>>(),
        1,
    )
}

fn run_suffix(paused: Paused, suffix: &[&str]) -> Vec<StructuralEvent> {
    let mut parser =
        ResumableValueBlockParser::resume(paused.checkpoint, paused.bindings, paused.cursor)
            .expect("resume");
    let mut sink = EventSink::default();
    let mut absolute_start = paused.absolute_start;
    for (index, line) in suffix.iter().enumerate() {
        parser
            .push_line(
                PhysicalLine {
                    coverage_leaf_id: paused.next_leaf_id
                        + u64::try_from(index).expect("test suffix fits u64"),
                    absolute_start,
                    text: line,
                },
                &mut sink,
            )
            .expect("suffix line");
        absolute_start += line.len();
    }
    parser.finish(&mut sink).expect("finish");
    sink.0
}

fn kind_tag(kind: &BlockKind) -> &'static str {
    match kind {
        BlockKind::Document => "document",
        BlockKind::BlockQuote => "quote",
        BlockKind::List(_) => "list",
        BlockKind::Item(_) => "item",
        BlockKind::CodeBlock { .. } => "code",
        BlockKind::HtmlBlock { .. } => "html",
        BlockKind::Paragraph => "paragraph",
        BlockKind::Heading { .. } => "heading",
        BlockKind::ThematicBreak => "thematic",
        BlockKind::Table(_) => "table",
        BlockKind::TableRow { .. } => "row",
        BlockKind::TableCell => "cell",
    }
}

/// Structural trace with revision-local handles, positions, source payload,
/// and output-only properties erased. This is deliberately weaker than a full
/// semantic comparison: it answers only whether a mutated boundary state caused
/// a different later block transition. Fresh-parse/output equality belongs to
/// the composed record-forest gate.
fn transition_trace(events: &[StructuralEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            StructuralEvent::SourceLeaf(_) => None,
            StructuralEvent::Open { state, .. } => Some(format!("open:{}", kind_tag(&state.kind))),
            StructuralEvent::Update { state, .. } => {
                Some(format!("update:{}", kind_tag(&state.kind)))
            }
            StructuralEvent::Close { .. } => Some("close".to_owned()),
            StructuralEvent::Detach { .. } => Some("detach".to_owned()),
            StructuralEvent::RepairListSourcePositions { .. } => Some("repair-list".to_owned()),
            StructuralEvent::UpdateSourcePositions { .. } => Some("positions".to_owned()),
            StructuralEvent::AppendContent { .. } => Some("append".to_owned()),
            StructuralEvent::DrainContentPrefix { .. } => Some("drain".to_owned()),
            StructuralEvent::Reference(_) => Some("reference".to_owned()),
        })
        .collect()
}

fn item_kind_mut(checkpoint: &mut BlockCheckpoint) -> &mut flark_comrak_value_block_core::ListData {
    checkpoint
        .frames
        .iter_mut()
        .rev()
        .find_map(|frame| match &mut frame.kind {
            BlockKind::Item(item) => Some(item),
            _ => None,
        })
        .expect("open item frame")
}

fn set_table_transition_state(
    checkpoint: &mut BlockCheckpoint,
    columns: usize,
    donor_rows: usize,
    donor_nonempty_cells: usize,
    autocompleted_cells: usize,
) {
    let frame = checkpoint
        .frames
        .iter_mut()
        .find(|frame| matches!(frame.kind, BlockKind::Table(_)))
        .expect("open table frame");
    let BlockKind::Table(table) = &mut frame.kind else {
        unreachable!("selected table frame")
    };
    table.num_columns = columns;
    table.num_rows = donor_rows;
    table.num_nonempty_cells = donor_nonempty_cells;
    frame.table_autocompleted_cells = autocompleted_cells;
}

#[test]
fn item_transition_observes_only_effective_content_indent() {
    let paused = pause(SyntaxProfile::CommonMark, &["  - body\n"]);
    let mut same_indent = paused.clone();
    let item = item_kind_mut(&mut same_indent.checkpoint);
    assert!(item.padding > 0);
    item.marker_offset += 1;
    item.padding -= 1;

    assert_ne!(paused.checkpoint, same_indent.checkpoint);
    assert_eq!(
        paused.checkpoint.transition_checkpoint(),
        same_indent.checkpoint.transition_checkpoint(),
        "separately comparing marker offset and padding would create a false boundary"
    );
    assert_eq!(
        transition_trace(&run_suffix(
            paused.clone(),
            &["    continuation\n", "outside\n"]
        )),
        transition_trace(&run_suffix(
            same_indent,
            &["    continuation\n", "outside\n"]
        ))
    );

    let mut different_indent = paused.clone();
    item_kind_mut(&mut different_indent.checkpoint).padding += 1;
    assert_ne!(
        paused.checkpoint.transition_checkpoint(),
        different_indent.checkpoint.transition_checkpoint()
    );
    assert_ne!(
        transition_trace(&run_suffix(paused, &["    > nested quote\n"])),
        transition_trace(&run_suffix(different_indent, &["    > nested quote\n"])),
        "effective item indentation has a concrete continuation witness"
    );
}

#[test]
fn child_presence_enters_grammar_equality_only_for_an_item() {
    let root_only = pause(SyntaxProfile::CommonMark, &["paragraph\n", "\n"]);
    assert_eq!(root_only.checkpoint.frames.len(), 1);
    assert!(root_only.checkpoint.frames[0].closed_children.had_child);
    let mut root_without_history = root_only.clone();
    root_without_history.checkpoint.frames[0].closed_children = ChildSequenceFold::default();
    assert_eq!(
        root_only.checkpoint.transition_checkpoint(),
        root_without_history.checkpoint.transition_checkpoint(),
        "document child history is output state, not later grammar"
    );
    assert_eq!(
        transition_trace(&run_suffix(root_only, &["outside\n"])),
        transition_trace(&run_suffix(root_without_history, &["outside\n"]))
    );

    let item_with_history = pause(SyntaxProfile::CommonMark, &["- body\n", "\n"]);
    let item_index = item_with_history
        .checkpoint
        .frames
        .iter()
        .position(|frame| matches!(frame.kind, BlockKind::Item(_)))
        .expect("open item");
    assert_eq!(item_index + 1, item_with_history.checkpoint.frames.len());
    assert!(
        item_with_history.checkpoint.frames[item_index]
            .closed_children
            .had_child
    );
    let mut empty_item = item_with_history.clone();
    empty_item.checkpoint.frames[item_index].closed_children = ChildSequenceFold::default();
    assert_ne!(
        item_with_history.checkpoint.transition_checkpoint(),
        empty_item.checkpoint.transition_checkpoint(),
        "a blank line continues only an Item that already has a child"
    );
    assert_ne!(
        transition_trace(&run_suffix(item_with_history, &["\n"])),
        transition_trace(&run_suffix(empty_item, &["\n"]))
    );
}

fn giant_raw_lines(opener: &str, changed_early_line: bool) -> Vec<String> {
    let mut lines = Vec::with_capacity(1_025);
    lines.push(opener.to_owned());
    for index in 0..1_024 {
        let width = if changed_early_line && index == 3 {
            1_022
        } else {
            1_023
        };
        lines.push(format!("{}\n", "x".repeat(width)));
    }
    lines
}

fn assert_giant_raw_pending_is_output_only(opener: &str, closer: &str) {
    let baseline = pause_lines(
        SyntaxProfile::CommonMark,
        &giant_raw_lines(opener, false),
        1,
    );
    let changed = pause_lines(SyntaxProfile::CommonMark, &giant_raw_lines(opener, true), 1);
    assert!(baseline.absolute_start >= 1024 * 1024);
    assert_ne!(baseline.checkpoint, changed.checkpoint);
    assert_eq!(
        baseline.checkpoint.transition_checkpoint(),
        changed.checkpoint.transition_checkpoint(),
        "aggregate raw payload must not pin convergence to the block close"
    );
    assert_eq!(
        transition_trace(&run_suffix(baseline, &[closer, "outside\n"])),
        transition_trace(&run_suffix(changed, &[closer, "outside\n"]))
    );
}

#[test]
fn giant_fence_and_html_payloads_do_not_enter_transition_equality() {
    assert_giant_raw_pending_is_output_only("```\n", "```\n");
    assert_giant_raw_pending_is_output_only("<script>\n", "</script>\n");
}

#[test]
fn paragraph_key_retains_exact_future_decisions_not_payload_or_provenance() {
    let baseline = pause_lines(SyntaxProfile::Gfm, &["alpha | beta\n".to_owned()], 1);
    let different_provenance =
        pause_lines(SyntaxProfile::Gfm, &["alpha | beta\n".to_owned()], 10_000);
    assert_ne!(baseline.checkpoint, different_provenance.checkpoint);
    assert_eq!(
        baseline.checkpoint.transition_checkpoint(),
        different_provenance.checkpoint.transition_checkpoint(),
        "origin runs and physical leaf IDs are output state"
    );

    let different_payload_same_decisions = pause(SyntaxProfile::Gfm, &["gamma | delta\n"]);
    assert_ne!(
        baseline.checkpoint,
        different_payload_same_decisions.checkpoint
    );
    assert_eq!(
        baseline.checkpoint.transition_checkpoint(),
        different_payload_same_decisions
            .checkpoint
            .transition_checkpoint(),
        "equal setext visibility and table-header shape do not retain paragraph bytes"
    );

    let different_logical = pause(SyntaxProfile::Gfm, &["one cell\n"]);
    assert_ne!(
        baseline.checkpoint.transition_checkpoint(),
        different_logical.checkpoint.transition_checkpoint(),
        "different table-header column counts remain future grammar"
    );

    let mut table_disqualified = baseline.clone();
    table_disqualified.checkpoint.frames[1].table_visited = true;
    assert_ne!(
        baseline.checkpoint.transition_checkpoint(),
        table_disqualified.checkpoint.transition_checkpoint()
    );
    assert_ne!(
        transition_trace(&run_suffix(baseline, &["--- | ---\n"])),
        transition_trace(&run_suffix(table_disqualified, &["--- | ---\n"])),
        "table_visited has an exact GFM table-activation witness"
    );

    let text = pause(SyntaxProfile::CommonMark, &["ordinary\n"]);
    let definition = pause(SyntaxProfile::CommonMark, &["[r]: /url\n"]);
    assert_ne!(
        text.checkpoint.transition_checkpoint(),
        definition.checkpoint.transition_checkpoint()
    );
    assert_ne!(
        transition_trace(&run_suffix(text, &["\n"])),
        transition_trace(&run_suffix(definition, &["\n"])),
        "setext visibility retains the exact reference-prefix outcome"
    );
}

#[test]
fn full_width_table_row_history_does_not_block_suffix_convergence() {
    let one_row = pause(
        SyntaxProfile::Gfm,
        &["| a | b |\n", "| --- | --- |\n", "| one | two |\n"],
    );
    let two_rows = pause(
        SyntaxProfile::Gfm,
        &[
            "| a | b |\n",
            "| --- | --- |\n",
            "| zero | row |\n",
            "| one | two |\n",
        ],
    );
    assert_ne!(one_row.checkpoint, two_rows.checkpoint);
    assert_eq!(
        one_row.checkpoint.transition_checkpoint(),
        two_rows.checkpoint.transition_checkpoint(),
        "raw row counters would force a document-spanning table to its close"
    );
    assert_eq!(
        transition_trace(&run_suffix(
            one_row,
            &["| next | row |\n", "\n", "outside\n"]
        )),
        transition_trace(&run_suffix(
            two_rows,
            &["| next | row |\n", "\n", "outside\n"]
        ))
    );
}

#[test]
fn table_autocomplete_state_is_counted_and_saturated_at_the_observable_cap() {
    let short_row = pause(
        SyntaxProfile::Gfm,
        &["| a | b | c |\n", "| --- | --- | --- |\n", "| only |\n"],
    );
    let table_frame = short_row
        .checkpoint
        .frames
        .iter()
        .find(|frame| matches!(frame.kind, BlockKind::Table(_)))
        .expect("open table");
    assert!(
        table_frame.table_autocompleted_cells > 0,
        "short source row must count synthesized cells"
    );

    let base = pause(
        SyntaxProfile::Gfm,
        &["| a | b |\n", "| --- | --- |\n", "| x | y |\n"],
    );
    let mut at_cap_a = base.clone();
    set_table_transition_state(&mut at_cap_a.checkpoint, 2, 250_001, 500_002, 500_000);

    let mut at_cap_b = base.clone();
    set_table_transition_state(&mut at_cap_b.checkpoint, 2, 250_002, 500_004, 500_000);
    assert_eq!(
        at_cap_a.checkpoint.transition_checkpoint(),
        at_cap_b.checkpoint.transition_checkpoint()
    );

    let mut over_cap_a = base.clone();
    set_table_transition_state(&mut over_cap_a.checkpoint, 2, 250_001, 500_002, 500_001);

    let mut over_cap_b = base.clone();
    set_table_transition_state(
        &mut over_cap_b.checkpoint,
        2,
        usize::MAX,
        usize::MAX,
        usize::MAX,
    );
    assert_eq!(
        over_cap_a.checkpoint.transition_checkpoint(),
        over_cap_b.checkpoint.transition_checkpoint(),
        "all already-over-cap histories have identical future behavior"
    );
    assert_ne!(
        at_cap_a.checkpoint.transition_checkpoint(),
        over_cap_a.checkpoint.transition_checkpoint(),
        "the exact cap still accepts one later row; over-cap does not"
    );
    assert_ne!(
        transition_trace(&run_suffix(at_cap_a, &["| next | row |\n"])),
        transition_trace(&run_suffix(over_cap_a, &["| next | row |\n"]))
    );
}

#[test]
fn document_start_profile_and_current_frame_remain_in_transition_equality() {
    let mut sink = DropSink;
    let parser = ResumableValueBlockParser::begin(SyntaxProfile::CommonMark);
    let (initial, _, _) = parser.pause(&mut sink).expect("initial pause");
    assert!(initial.transition_checkpoint().at_document_start);

    let after_line = pause(SyntaxProfile::CommonMark, &["\n"]);
    assert!(
        !after_line
            .checkpoint
            .transition_checkpoint()
            .at_document_start
    );
    assert_ne!(
        initial.transition_checkpoint(),
        after_line.checkpoint.transition_checkpoint()
    );

    let gfm = pause(SyntaxProfile::Gfm, &["\n"]);
    assert_ne!(
        after_line.checkpoint.transition_checkpoint(),
        gfm.checkpoint.transition_checkpoint()
    );

    let table = pause(
        SyntaxProfile::Gfm,
        &["| a | b |\n", "| --- | --- |\n", "| x | y |\n"],
    );
    assert!(table.checkpoint.current_frame < table.checkpoint.frames.len());
    let mut different_current = table.clone();
    different_current.checkpoint.current_frame = (different_current.checkpoint.current_frame + 1)
        % different_current.checkpoint.frames.len();
    assert_ne!(
        table.checkpoint.transition_checkpoint(),
        different_current.checkpoint.transition_checkpoint(),
        "current is not always the deepest materialized table frame"
    );
}

#[test]
fn raw_and_heading_output_fields_are_absent_but_future_read_fields_are_present() {
    let fence = pause(SyntaxProfile::CommonMark, &["``` lang\n", "body\n"]);
    let key = fence.checkpoint.transition_checkpoint();
    assert!(key.frames.iter().any(|frame| matches!(
        frame.kind,
        BlockTransitionKind::FencedCode {
            fence_char: b'`',
            fence_length: 3,
            ..
        }
    )));

    let mut changed_fence = fence.clone();
    let code = changed_fence
        .checkpoint
        .frames
        .iter_mut()
        .find(|frame| matches!(frame.kind, BlockKind::CodeBlock { .. }))
        .expect("open code");
    if let BlockKind::CodeBlock {
        info,
        literal,
        closed,
        ..
    } = &mut code.kind
    {
        info.start = 7;
        literal.end = 9;
        *closed = !*closed;
    }
    assert_eq!(
        fence.checkpoint.transition_checkpoint(),
        changed_fence.checkpoint.transition_checkpoint()
    );

    let mut changed_delimiter = fence;
    let code = changed_delimiter
        .checkpoint
        .frames
        .iter_mut()
        .find(|frame| matches!(frame.kind, BlockKind::CodeBlock { .. }))
        .expect("open code");
    if let BlockKind::CodeBlock { fence_length, .. } = &mut code.kind {
        *fence_length += 1;
    }
    assert_ne!(key, changed_delimiter.checkpoint.transition_checkpoint());

    let heading = pause(SyntaxProfile::CommonMark, &["## heading ##\n"]);
    let mut changed_heading = heading.clone();
    let frame = changed_heading
        .checkpoint
        .frames
        .iter_mut()
        .find(|frame| matches!(frame.kind, BlockKind::Heading { .. }))
        .expect("open heading");
    if let BlockKind::Heading {
        level,
        setext,
        closed,
    } = &mut frame.kind
    {
        *level = 6;
        *setext = !*setext;
        *closed = !*closed;
    }
    frame.pending.logical.push_str("changed output only");
    assert_eq!(
        heading.checkpoint.transition_checkpoint(),
        changed_heading.checkpoint.transition_checkpoint()
    );
}

use std::collections::BTreeSet;

use flark_comrak_value_block_core::checkpoint::{
    BlockCheckpoint, MaterializationCursor, OpenOutputBindings, PhysicalLine,
    ResumableValueBlockParser, StructuralEvent, WriteOnlyBlockSink,
};
use flark_comrak_value_block_core::tree::{
    Alignment, BlockKind, ChildSequenceFold, ClosedChildSummary, ListDelimiter, ListType,
    SyntaxProfile,
};

#[derive(Default)]
struct EventSink(Vec<StructuralEvent>);

impl WriteOnlyBlockSink for EventSink {
    fn emit(&mut self, event: StructuralEvent) {
        self.0.push(event);
    }
}

fn fold(sequence: &[ClosedChildSummary]) -> ChildSequenceFold {
    let mut result = ChildSequenceFold::default();
    for child in sequence {
        result.push(*child);
    }
    result
}

fn visit_sequences(
    atoms: &[ClosedChildSummary],
    remaining: usize,
    sequence: &mut Vec<ClosedChildSummary>,
    visit: &mut impl FnMut(&[ClosedChildSummary]),
) {
    visit(sequence);
    if remaining == 0 {
        return;
    }
    for atom in atoms {
        sequence.push(*atom);
        visit_sequences(atoms, remaining - 1, sequence, visit);
        sequence.pop();
    }
}

#[test]
fn child_output_fold_is_an_exact_associative_range_summary() {
    let mut atoms = Vec::new();
    for ends_blank in [false, true] {
        for item_loose_if_nonlast in [false, true] {
            for item_loose_if_last in [false, true] {
                atoms.push(ClosedChildSummary {
                    ends_blank,
                    item_loose_if_nonlast,
                    item_loose_if_last,
                });
            }
        }
    }

    let mut compared = 0_usize;
    let mut reachable = BTreeSet::new();
    visit_sequences(
        &atoms,
        4,
        &mut Vec::new(),
        &mut |sequence: &[ClosedChildSummary]| {
            let expected = fold(sequence);
            reachable.insert(expected);
            for split in 0..=sequence.len() {
                let prefix = fold(&sequence[..split]);
                let suffix = fold(&sequence[split..]);
                assert_eq!(prefix.followed_by(suffix), expected, "split {split}");
                compared += 1;
            }
        },
    );
    assert_eq!(compared, 22_737);
    assert_eq!(reachable.len(), 33, "finite fold state closure changed");
    for state in &reachable {
        for atom in &atoms {
            let mut next = *state;
            next.push(*atom);
            assert!(
                reachable.contains(&next),
                "reachable state set is not closed under push: {state:?} + {atom:?}"
            );
        }
    }
    let identity = ChildSequenceFold::default();
    for left in &reachable {
        assert_eq!(identity.followed_by(*left), *left);
        assert_eq!(left.followed_by(identity), *left);
        for middle in &reachable {
            for right in &reachable {
                assert_eq!(
                    left.followed_by(*middle).followed_by(*right),
                    left.followed_by(middle.followed_by(*right))
                );
            }
        }
    }
}

fn normalized_transition_trace(events: &[StructuralEvent]) -> Vec<StructuralEvent> {
    events
        .iter()
        .cloned()
        .map(|mut event| {
            // `ListData.tight` is the one expected output difference. All
            // handles, structural events, source positions, and other state
            // remain under typed equality.
            let state = match &mut event {
                StructuralEvent::Open { state, .. } | StructuralEvent::Update { state, .. } => {
                    Some(state)
                }
                _ => None,
            };
            if let Some(state) = state {
                state.last_line_blank = false;
                match &mut state.kind {
                    BlockKind::List(list) => {
                        list.marker_offset = 0;
                        list.padding = 0;
                        list.start = 0;
                        list.tight = false;
                    }
                    BlockKind::Item(item) => {
                        item.list_type = ListType::Bullet;
                        item.start = 0;
                        item.delimiter = ListDelimiter::Period;
                        item.bullet_char = 0;
                        item.tight = false;
                    }
                    BlockKind::CodeBlock {
                        info,
                        literal,
                        closed,
                        ..
                    } => {
                        *info = Default::default();
                        *literal = Default::default();
                        *closed = false;
                    }
                    BlockKind::HtmlBlock { literal, .. } => {
                        *literal = Default::default();
                    }
                    BlockKind::Heading {
                        level,
                        setext,
                        closed,
                    } => {
                        *level = 0;
                        *setext = false;
                        *closed = false;
                    }
                    BlockKind::Table(table) => table.alignments.fill(Alignment::None),
                    BlockKind::TableRow { header } => *header = false,
                    _ => {}
                }
            }
            event
        })
        .collect()
}

fn run_suffix(
    checkpoint: BlockCheckpoint,
    bindings: OpenOutputBindings,
    cursor: MaterializationCursor,
    mut absolute_start: usize,
    suffix: &[&str],
) -> Vec<StructuralEvent> {
    let mut parser =
        ResumableValueBlockParser::resume(checkpoint, bindings, cursor).expect("resume checkpoint");
    let mut events = EventSink::default();
    for (index, line) in suffix.iter().enumerate() {
        parser
            .push_line(
                PhysicalLine {
                    coverage_leaf_id: 10_000 + u64::try_from(index).expect("small suffix index"),
                    absolute_start,
                    text: line,
                },
                &mut events,
            )
            .expect("suffix line");
        absolute_start += line.len();
    }
    parser.finish(&mut events).expect("suffix finish");
    events.0
}

fn flip_any_nonlast_child_ends_blank(fold: &mut ChildSequenceFold) {
    fold.any_nonlast_child_ends_blank = !fold.any_nonlast_child_ends_blank;
}

fn flip_last_child_ends_blank(fold: &mut ChildSequenceFold) {
    fold.last_child_ends_blank = !fold.last_child_ends_blank;
}

fn flip_list_loose_before_last(fold: &mut ChildSequenceFold) {
    fold.list_loose_before_last = !fold.list_loose_before_last;
}

fn flip_last_item_loose_if_nonlast(fold: &mut ChildSequenceFold) {
    fold.last_item_loose_if_nonlast = !fold.last_item_loose_if_nonlast;
}

fn flip_last_item_loose_if_last(fold: &mut ChildSequenceFold) {
    fold.last_item_loose_if_last = !fold.last_item_loose_if_last;
}

const OUTPUT_MUTATIONS: [fn(&mut ChildSequenceFold); 5] = [
    flip_any_nonlast_child_ends_blank,
    flip_last_child_ends_blank,
    flip_list_loose_before_last,
    flip_last_item_loose_if_nonlast,
    flip_last_item_loose_if_last,
];

fn pause_prefix(
    prefix: &[&str],
) -> (
    BlockCheckpoint,
    OpenOutputBindings,
    MaterializationCursor,
    usize,
) {
    let mut parser = ResumableValueBlockParser::begin(SyntaxProfile::CommonMark);
    let mut discarded = EventSink::default();
    let mut absolute_start = 0_usize;
    for (index, line) in prefix.iter().enumerate() {
        parser
            .push_line(
                PhysicalLine {
                    coverage_leaf_id: u64::try_from(index + 1).expect("small leaf ID"),
                    absolute_start,
                    text: line,
                },
                &mut discarded,
            )
            .expect("prefix line");
        absolute_start += line.len();
    }
    let (checkpoint, bindings, cursor) = parser.pause(&mut discarded).expect("pause");
    (checkpoint, bindings, cursor, absolute_start)
}

fn assert_output_mutations_do_not_change_transitions(
    prefix: &[&str],
    target: fn(&BlockKind) -> bool,
    suffix: &[&str],
) {
    let (checkpoint, bindings, cursor, absolute_start) = pause_prefix(prefix);
    let baseline = run_suffix(
        checkpoint.clone(),
        bindings.clone(),
        cursor.clone(),
        absolute_start,
        suffix,
    );
    for mutation in OUTPUT_MUTATIONS {
        let mut changed = checkpoint.clone();
        let frame = changed
            .frames
            .iter_mut()
            .rev()
            .find(|frame| target(&frame.kind) && frame.closed_children.had_child)
            .expect("target frame with historical child output");
        mutation(&mut frame.closed_children);
        assert_eq!(
            checkpoint.transition_checkpoint(),
            changed.transition_checkpoint(),
            "output bit entered transition equality"
        );
        let changed_events = run_suffix(
            changed,
            bindings.clone(),
            cursor.clone(),
            absolute_start,
            suffix,
        );
        assert_eq!(
            normalized_transition_trace(&baseline),
            normalized_transition_trace(&changed_events),
            "output bit changed suffix block transitions"
        );
    }
}

fn is_list(kind: &BlockKind) -> bool {
    matches!(kind, BlockKind::List(_))
}

fn is_item(kind: &BlockKind) -> bool {
    matches!(kind, BlockKind::Item(_))
}

#[test]
fn every_child_output_bit_is_absent_from_list_and_item_transition_state() {
    assert_output_mutations_do_not_change_transitions(
        &["- first\n", "- second\n"],
        is_list,
        &["\n", "outside\n"],
    );
    assert_output_mutations_do_not_change_transitions(
        &["- first paragraph\n", "\n", "  second paragraph\n"],
        is_item,
        &["\n", "- next\n", "\n", "outside\n"],
    );
}

#[test]
fn historical_child_presence_is_irrelevant_when_the_open_path_retains_a_child() {
    let (checkpoint, bindings, cursor, absolute_start) =
        pause_prefix(&["- deleted predecessor\n", "- retained child\n"]);
    let mut without_predecessor = checkpoint.clone();
    let list = without_predecessor
        .frames
        .iter_mut()
        .find(|frame| matches!(frame.kind, BlockKind::List(_)))
        .expect("open list");
    assert!(list.closed_children.had_child);
    list.closed_children = ChildSequenceFold::default();

    assert_eq!(
        checkpoint.transition_checkpoint(),
        without_predecessor.transition_checkpoint(),
        "the retained open item already makes has_any_child exact"
    );
    let baseline = run_suffix(
        checkpoint,
        bindings.clone(),
        cursor.clone(),
        absolute_start,
        &["\n", "outside\n"],
    );
    let changed = run_suffix(
        without_predecessor,
        bindings,
        cursor,
        absolute_start,
        &["\n", "outside\n"],
    );
    assert_eq!(
        normalized_transition_trace(&baseline),
        normalized_transition_trace(&changed)
    );
}

#[test]
fn list_display_metadata_and_last_blank_output_do_not_block_convergence() {
    let (checkpoint, bindings, cursor, absolute_start) =
        pause_prefix(&["7. first\n", "8. retained\n"]);
    let mut changed = checkpoint.clone();
    for frame in &mut changed.frames {
        frame.last_line_blank = !frame.last_line_blank;
        match &mut frame.kind {
            BlockKind::List(list) => {
                list.marker_offset += 17;
                list.padding += 19;
                list.start += 23;
                list.tight = !list.tight;
            }
            BlockKind::Item(item) => {
                item.list_type = ListType::Bullet;
                item.start += 29;
                item.delimiter = ListDelimiter::Paren;
                item.bullet_char = b'*';
                item.tight = !item.tight;
            }
            _ => {}
        }
    }
    assert_eq!(
        checkpoint.transition_checkpoint(),
        changed.transition_checkpoint(),
        "display-only list fields entered transition equality"
    );
    let suffix = &["9. third\n", "\n", "outside\n"];
    let baseline = run_suffix(
        checkpoint,
        bindings.clone(),
        cursor.clone(),
        absolute_start,
        suffix,
    );
    let changed = run_suffix(changed, bindings, cursor, absolute_start, suffix);
    assert_eq!(
        normalized_transition_trace(&baseline),
        normalized_transition_trace(&changed)
    );
}

#[test]
fn code_projection_metadata_does_not_block_convergence() {
    let (checkpoint, bindings, cursor, absolute_start) = pause_prefix(&["``` lang\n", "body\n"]);
    let mut changed = checkpoint.clone();
    let code = changed
        .frames
        .iter_mut()
        .find(|frame| matches!(frame.kind, BlockKind::CodeBlock { .. }))
        .expect("open code block");
    code.last_line_blank = !code.last_line_blank;
    let BlockKind::CodeBlock {
        info,
        literal,
        closed,
        ..
    } = &mut code.kind
    else {
        unreachable!("selected code block")
    };
    info.start = 11;
    info.end = 17;
    literal.start = 19;
    literal.end = 31;
    *closed = !*closed;

    assert_eq!(
        checkpoint.transition_checkpoint(),
        changed.transition_checkpoint(),
        "code output projections entered transition equality"
    );
    let suffix = &["more\n", "```\n", "outside\n"];
    let baseline = run_suffix(
        checkpoint,
        bindings.clone(),
        cursor.clone(),
        absolute_start,
        suffix,
    );
    let changed = run_suffix(changed, bindings, cursor, absolute_start, suffix);
    assert_eq!(
        normalized_transition_trace(&baseline),
        normalized_transition_trace(&changed)
    );
}

#[test]
fn list_looseness_changes_output_but_not_suffix_block_transitions() {
    let prefix = ["- first\n", "- second\n"];
    let mut parser = ResumableValueBlockParser::begin(SyntaxProfile::CommonMark);
    let mut discarded = EventSink::default();
    let mut absolute_start = 0_usize;
    for (index, line) in prefix.iter().enumerate() {
        parser
            .push_line(
                PhysicalLine {
                    coverage_leaf_id: u64::try_from(index + 1).expect("small leaf ID"),
                    absolute_start,
                    text: line,
                },
                &mut discarded,
            )
            .expect("prefix line");
        absolute_start += line.len();
    }

    let (checkpoint, bindings, cursor) = parser.pause(&mut discarded).expect("pause");
    let mut loose_checkpoint = checkpoint.clone();
    let list_frame = loose_checkpoint
        .frames
        .iter_mut()
        .find(|frame| matches!(frame.kind, BlockKind::List(_)))
        .expect("open spanning list frame");
    assert!(list_frame.closed_children.had_child);
    list_frame.closed_children.last_item_loose_if_nonlast = true;

    // The checkpoints carry different accumulated output, but their exact
    // typed block-transition state is equal.
    assert_ne!(checkpoint, loose_checkpoint);
    assert_eq!(
        checkpoint.transition_checkpoint(),
        loose_checkpoint.transition_checkpoint()
    );

    let tight_events = run_suffix(
        checkpoint,
        bindings.clone(),
        cursor.clone(),
        absolute_start,
        &["- third\n"],
    );
    let loose_events = run_suffix(
        loose_checkpoint,
        bindings,
        cursor,
        absolute_start,
        &["- third\n"],
    );

    assert_ne!(tight_events, loose_events, "list property should differ");
    assert_eq!(
        normalized_transition_trace(&tight_events),
        normalized_transition_trace(&loose_events),
        "accumulated looseness changed a later block transition"
    );
}

use std::collections::HashSet;
use std::mem::size_of;

use comrak::nodes::NodeValue;
use comrak::{parse_document, Arena, Options};
use flark_comrak_derived_core_probe::{
    AdvanceStatus, ComparisonStatus, ContainerKind, DerivedBlockMachine, LineKind, RestartState,
};

fn run(source: &str, fuel: usize) -> DerivedBlockMachine {
    let mut machine = DerivedBlockMachine::new(source);
    while !machine.is_complete() {
        let report = machine.advance(fuel);
        assert!(report.work_units <= fuel);
        assert!(report.bytes_inspected <= report.work_units);
    }
    machine
}

fn probe_projection(machine: &DerivedBlockMachine) -> Vec<Vec<&'static str>> {
    let promoted = machine
        .records()
        .iter()
        .flat_map(|record| &record.events)
        .filter_map(|event| match event {
            flark_comrak_derived_core_probe::BlockEvent::PromoteLeaf { id, .. } => Some(*id),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let mut previous_leaf = None;
    machine
        .records()
        .iter()
        .filter_map(|record| {
            if record.chunk.kind == LineKind::Blank {
                return None;
            }
            if previous_leaf == record.chunk.leaf_id {
                return None;
            }
            previous_leaf = record.chunk.leaf_id;
            let mut path = record
                .chunk
                .container_path()
                .into_iter()
                .map(|frame| match frame.kind {
                    ContainerKind::BlockQuote => "quote",
                    ContainerKind::List(_) => "list",
                    ContainerKind::Item(_) => "item",
                })
                .collect::<Vec<_>>();
            path.push(match record.chunk.kind {
                LineKind::Paragraph
                    if record
                        .chunk
                        .leaf_id
                        .is_some_and(|id| promoted.contains(&id)) =>
                {
                    "heading"
                }
                LineKind::Paragraph => "paragraph",
                LineKind::SetextUnderline => "heading",
                LineKind::FenceOpen | LineKind::FenceBody | LineKind::FenceClose => "code",
                LineKind::Blank => unreachable!(),
            });
            Some(path)
        })
        .collect()
}

fn comrak_projection(source: &str) -> Vec<Vec<&'static str>> {
    let arena = Arena::new();
    let root = parse_document(&arena, source, &Options::default());
    let mut result = Vec::new();
    for leaf in root.descendants() {
        if !matches!(
            leaf.data().value,
            NodeValue::Paragraph | NodeValue::Heading(_) | NodeValue::CodeBlock(_)
        ) {
            continue;
        }
        let mut path = Vec::new();
        let mut ancestors = Vec::new();
        let mut cursor = leaf.parent();
        while let Some(node) = cursor {
            ancestors.push(node);
            cursor = node.parent();
        }
        ancestors.reverse();
        for node in ancestors {
            match node.data().value {
                NodeValue::BlockQuote => path.push("quote"),
                NodeValue::List(_) => path.push("list"),
                NodeValue::Item(_) => path.push("item"),
                _ => {}
            }
        }
        path.push(match leaf.data().value {
            NodeValue::Paragraph => "paragraph",
            NodeValue::Heading(_) => "heading",
            NodeValue::CodeBlock(_) => "code",
            _ => unreachable!(),
        });
        result.push(path);
    }
    result
}

#[test]
fn selected_quote_list_fence_projection_matches_comrak() {
    let source = "> - alpha\n>   continuation\n> - ```\n>   code\n>   ```\noutside\n";
    let machine = run(source, 17);

    assert_eq!(probe_projection(&machine), comrak_projection(source));
    assert_eq!(
        machine
            .records()
            .iter()
            .map(|record| record.chunk.kind)
            .collect::<Vec<_>>(),
        vec![
            LineKind::Paragraph,
            LineKind::Paragraph,
            LineKind::FenceOpen,
            LineKind::FenceBody,
            LineKind::FenceClose,
            LineKind::Paragraph,
        ]
    );

    let text = machine.source();
    let contents = machine
        .records()
        .iter()
        .map(|record| &text[record.chunk.content.clone()])
        .collect::<Vec<_>>();
    assert_eq!(
        contents,
        ["alpha", "continuation", "", "code", "", "outside"]
    );

    let first_list = machine.records()[0].chunk.container_path()[1].id;
    let continued_list = machine.records()[1].chunk.container_path()[1].id;
    let replacement_item_list = machine.records()[2].chunk.container_path()[1].id;
    assert_eq!(first_list, continued_list);
    assert_eq!(first_list, replacement_item_list);
    assert_ne!(
        machine.records()[0].chunk.container_path()[2].id,
        machine.records()[2].chunk.container_path()[2].id
    );
}

#[test]
fn paragraph_interruption_rules_preserve_the_comrak_subset() {
    let source = "alpha\n2. does not interrupt\n+ \n1. does interrupt\n";
    let machine = run(source, 11);
    let records = machine.records();

    assert!(records[1].chunk.continues_leaf);
    assert!(records[2].chunk.continues_leaf);
    assert_eq!(records[3].chunk.container_path().len(), 2);
    assert!(matches!(
        records[3].chunk.container_path()[0].kind,
        ContainerKind::List(_)
    ));
    assert_eq!(probe_projection(&machine), comrak_projection(source));
}

#[test]
fn invalid_backtick_info_is_not_opened_as_a_fence() {
    let source = "``` bad`info\nnext\n";
    let machine = run(source, 7);
    assert_eq!(
        machine
            .records()
            .iter()
            .map(|record| record.chunk.kind)
            .collect::<Vec<_>>(),
        [LineKind::Paragraph, LineKind::Paragraph]
    );
    assert!(machine.records()[1].chunk.continues_leaf);
    assert_eq!(probe_projection(&machine), comrak_projection(source));
}

#[test]
fn setext_is_a_retroactive_leaf_promotion_not_a_second_parser_pass() {
    let source = "alpha\n-   \nnext\n";
    let machine = run(source, 5);
    let records = machine.records();

    assert_eq!(records[0].chunk.kind, LineKind::Paragraph);
    assert_eq!(records[1].chunk.kind, LineKind::SetextUnderline);
    assert_eq!(records[0].chunk.leaf_id, records[1].chunk.leaf_id);
    assert!(records[1].events.iter().any(|event| matches!(
        event,
        flark_comrak_derived_core_probe::BlockEvent::PromoteLeaf {
            from: flark_comrak_derived_core_probe::LeafKind::Paragraph,
            to: flark_comrak_derived_core_probe::LeafKind::Heading(2),
            ..
        }
    )));
    assert_eq!(probe_projection(&machine), comrak_projection(source));
}

#[test]
fn giant_setext_scanner_is_cooperatively_resumable() {
    let source = format!("alpha\n{}\n", "=".repeat(10_000_000));
    let mut machine = DerivedBlockMachine::new(source);
    let mut maximum_bytes_per_poll = 0;
    let mut polls = 0;
    while !machine.is_complete() {
        let report = machine.advance(4_096);
        maximum_bytes_per_poll = maximum_bytes_per_poll.max(report.bytes_inspected);
        assert!(report.bytes_inspected <= report.work_units);
        assert!(report.work_units <= 4_096);
        polls += 1;
    }
    assert!(polls > 2_400);
    assert!(maximum_bytes_per_poll <= 4_096);
    assert_eq!(machine.records()[1].chunk.kind, LineKind::SetextUnderline);
}

#[test]
fn supported_fixture_matrix_matches_comrak_block_structure() {
    let fixtures = [
        ("lazy quote", "> alpha\nlazy continuation\n"),
        ("two bullet items", "- alpha\n  continuation\n- beta\n"),
        ("ordered paren", "1) alpha\n2) beta\n"),
        ("tilde fence", "~~~ rust\nlet x = 1;\n~~~\n"),
        ("indented fence", "   ```\ncode\n   ```\n"),
        ("nested quotes", "> > alpha\n> > beta\n"),
        (
            "nested lists",
            "- outer\n  - inner\n    continuation\n- tail\n",
        ),
        ("changed bullet", "- alpha\n+ beta\n"),
        ("longer close", "```\ncode\n`````\n"),
        ("short close stays body", "````\n```\nbody\n````\n"),
        ("quote interrupts", "alpha\n> quote\n"),
        ("fence interrupts", "alpha\n```\ncode\n```\n"),
        ("blank in list", "- alpha\n\n  beta\n"),
    ];

    for (name, source) in fixtures {
        let machine = run(source, 13);
        assert_eq!(
            probe_projection(&machine),
            comrak_projection(source),
            "fixture={name}\nsource={source:?}"
        );
    }
}

#[test]
fn ten_megabyte_line_really_yields_inside_the_line() {
    let source = format!("{}\n", "a".repeat(10_000_000));
    let mut machine = DerivedBlockMachine::new(source);
    let first = machine.advance(64);
    assert_eq!(first.status, AdvanceStatus::Yielded);
    assert_eq!(first.work_units, 64);
    assert!(first.bytes_inspected <= 64);
    assert!(first.bytes_inspected > 0);
    assert_eq!(first.completed_lines, 0);
    assert_eq!(
        machine.offset(),
        0,
        "line is not published before completion"
    );

    let mut polls = 1;
    let mut maximum_bytes_per_poll = first.bytes_inspected;
    while !machine.is_complete() {
        let report = machine.advance(4_096);
        maximum_bytes_per_poll = maximum_bytes_per_poll.max(report.bytes_inspected);
        assert!(report.work_units <= 4_096);
        assert!(report.bytes_inspected <= report.work_units);
        polls += 1;
    }

    assert!(polls > 2_400, "polls={polls}");
    assert!(maximum_bytes_per_poll <= 4_096);
    let record = &machine.records()[0];
    assert!(record.bytes_inspected <= machine.source().len() + 8);
    assert_eq!(record.chunk.content.len(), 10_000_000);
}

#[test]
fn deep_container_work_and_checkpoint_are_value_bounded() {
    let depth = 2_000;
    let source = format!("{}x\n{}y\n", "> ".repeat(depth), "> ".repeat(depth));
    let machine = run(&source, 127);

    assert_eq!(machine.records().len(), 2);
    assert_eq!(machine.records()[0].state_after.depth(), depth);
    assert_eq!(machine.records()[1].state_after.depth(), depth);
    assert!(size_of::<RestartState>() <= 64);
    assert!(machine
        .records()
        .iter()
        .all(|record| record.bytes_inspected <= record.work_units));

    let mut comparison = machine.records()[0]
        .state_after
        .begin_semantic_comparison(&machine.records()[1].state_after);
    let mut polls = 0;
    loop {
        polls += 1;
        match comparison.advance(31) {
            ComparisonStatus::Pending => {}
            ComparisonStatus::Equal => break,
            ComparisonStatus::NotEqual => panic!("equal deep states diverged"),
        }
    }
    assert!(polls > depth / 32, "comparison did not visibly yield");
}

#[test]
fn line_boundary_checkpoint_resumes_against_an_edited_suffix() {
    let original = "> - alpha\n>   beta\noutside\n";
    let edited = "> - alpha\n>   gamma\noutside\n";
    let mut prefix = DerivedBlockMachine::new(original);
    while prefix.records().is_empty() {
        prefix.advance(1);
    }
    let checkpoint = prefix.checkpoint().expect("line boundary checkpoint");
    assert_eq!(checkpoint.offset, "> - alpha\n".len());

    let resumed = run_resumed(edited, checkpoint, 9);
    let clean = run(edited, 9);
    assert!(resumed.state().semantic_eq(clean.state()));
    assert_eq!(
        resumed
            .records()
            .iter()
            .map(|record| (record.chunk.kind, record.chunk.container_path().len()))
            .collect::<Vec<_>>(),
        clean.records()[1..]
            .iter()
            .map(|record| (record.chunk.kind, record.chunk.container_path().len()))
            .collect::<Vec<_>>()
    );
}

fn run_resumed(
    source: &str,
    checkpoint: flark_comrak_derived_core_probe::Checkpoint,
    fuel: usize,
) -> DerivedBlockMachine {
    let mut machine = DerivedBlockMachine::resume(source, checkpoint);
    while !machine.is_complete() {
        let report = machine.advance(fuel);
        assert!(report.work_units <= fuel);
        assert!(report.bytes_inspected <= report.work_units);
    }
    machine
}

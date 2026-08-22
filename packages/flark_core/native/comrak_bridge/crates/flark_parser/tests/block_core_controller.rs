use flark_engine::{SourceStore, SOURCE_CURSOR_WINDOW_BYTES};
use flark_parser::block_core::{
    BlockCommand, BlockKind, CoveragePart, FinalFacts, M11DirectBlockController,
    M11DirectBlockPollStatus, M11_DIRECT_BLOCK_MAX_LEXICAL_SLACK,
    M11_DIRECT_BLOCK_MAX_RETAINED_SOURCE_BYTES,
};
use flark_parser::{
    M11ExactController, M11SourceLinePollStatus, M11SourceLineSource, SnapshotLinePoll,
    SnapshotLineScanner, SnapshotLineSource,
};

fn acknowledge(controller: &mut M11DirectBlockController, commands: &mut Vec<BlockCommand>) {
    commands.push(
        *controller
            .pending_command()
            .expect("one production command is ready"),
    );
    controller
        .acknowledge_command()
        .expect("production command acknowledgement");
}

fn drive(source: &str, fuel: usize) -> Vec<BlockCommand> {
    let store = SourceStore::new(source).expect("source");
    let mut scanner = SnapshotLineScanner::new(store.snapshot()).expect("line scanner");
    let mut controller = M11DirectBlockController::new().expect("direct controller");
    let mut commands = Vec::new();
    acknowledge(&mut controller, &mut commands);

    loop {
        let line = loop {
            match scanner.poll(fuel).expect("line discovery") {
                SnapshotLinePoll::Pending(next) => scanner = next,
                SnapshotLinePoll::Line(line) => break Some(line),
                SnapshotLinePoll::Complete => break None,
            }
        };
        let Some(line) = line else { break };
        let facts = line.facts();
        let mut source = line.into_source().expect("source-backed line");
        let mut admission = <M11DirectBlockController as M11ExactController<
            SnapshotLineSource,
        >>::begin_source_line(&mut controller, facts.identity())
        .expect("line admission");

        let mut matched = false;
        let mut last_source_poll = None;
        for _ in 0..1_000 {
            if source.access_budget() == 0 && source.position() < source.len() {
                source
                    .replenish_access_budget(SOURCE_CURSOR_WINDOW_BYTES)
                    .expect("bounded source grant");
            }
            let physical_budget_before = source.access_budget();
            let physical_position_before = source.position();
            let receipt = <M11DirectBlockController as M11ExactController<
                SnapshotLineSource,
            >>::poll_source_line(&mut controller, &mut admission, &mut source, fuel)
            .expect("source poll");
            assert!(
                receipt.lexical_work_units
                    <= fuel.saturating_add(M11_DIRECT_BLOCK_MAX_LEXICAL_SLACK)
            );
            assert!(receipt.source_first_reads <= fuel);
            assert!(source.access_budget() <= physical_budget_before);
            assert_eq!(
                physical_budget_before - source.access_budget(),
                receipt.source_first_reads,
                "logical scanner lookahead never spends an extra physical source byte",
            );
            assert_eq!(
                source.position() - physical_position_before,
                receipt.source_first_reads,
                "every physical source advance is charged to the caller's poll",
            );
            assert_eq!(receipt.maximum_source_request_rewind_bytes, 0);
            last_source_poll = Some((receipt, source.position(), source.access_budget()));
            if receipt.status == M11SourceLinePollStatus::Matched {
                matched = true;
                break;
            }
        }
        assert!(
            matched,
            "source donor converges under bounded polling: facts={facts:?} last={last_source_poll:?}"
        );

        <M11DirectBlockController as M11ExactController<SnapshotLineSource>>::commit_source_line(
            &mut controller,
            admission,
            facts,
        )
        .expect("source commit");
        scanner = source.finish().expect("line source is exhausted");

        let mut complete = false;
        for _ in 0..100_000 {
            let receipt = controller.poll_line(fuel).expect("grammar poll");
            assert!(receipt.transitions <= fuel);
            match receipt.status {
                M11DirectBlockPollStatus::Pending => {}
                M11DirectBlockPollStatus::CommandReady => {
                    acknowledge(&mut controller, &mut commands);
                }
                M11DirectBlockPollStatus::ExternalWorkReady => {
                    panic!("controller fixture unexpectedly requires reference work");
                }
                M11DirectBlockPollStatus::Complete => {
                    complete = true;
                    break;
                }
            }
        }
        assert!(complete, "line grammar converges under bounded polling");
    }

    controller.begin_finish().expect("finish begins");
    let mut complete = false;
    for _ in 0..100_000 {
        let receipt = controller.poll_finish(fuel).expect("finish poll");
        assert!(receipt.transitions <= fuel);
        match receipt.status {
            M11DirectBlockPollStatus::Pending => {}
            M11DirectBlockPollStatus::CommandReady => acknowledge(&mut controller, &mut commands),
            M11DirectBlockPollStatus::ExternalWorkReady => {
                panic!("controller fixture unexpectedly requires reference work");
            }
            M11DirectBlockPollStatus::Complete => {
                complete = true;
                break;
            }
        }
    }
    assert!(complete, "document finish converges under bounded polling");
    commands
}

fn assert_exact_line_partitions(commands: &[BlockCommand]) {
    let mut covered_byte = 0_u64;
    let mut covered_utf16 = 0_u64;
    for command in commands {
        let source = match command {
            BlockCommand::Coverage { source, .. }
            | BlockCommand::StageTerminator { source, .. }
            | BlockCommand::StageBlankGap { source } => Some(*source),
            _ => None,
        };
        if let Some(source) = source {
            assert_eq!(source.start().byte(), covered_byte);
            assert_eq!(source.start().utf16(), covered_utf16);
            covered_byte = source.end().byte();
            covered_utf16 = source.end().utf16();
        }
        if let BlockCommand::FinishLine { physical } = command {
            assert_eq!(covered_byte, physical.bytes());
            assert_eq!(covered_utf16, physical.utf16());
            covered_byte = 0;
            covered_utf16 = 0;
        }
    }
    assert_eq!((covered_byte, covered_utf16), (0, 0));
}

#[test]
fn source_backed_cm321_is_fuel_invariant_and_preserves_nested_structure() {
    const SOURCE: &str = "- a\n  > b\n  ```\n  c\n  ```\n- d\n";
    let fuel_one = drive(SOURCE, 1);
    assert_eq!(drive(SOURCE, 7), fuel_one);
    assert_eq!(drive(SOURCE, 64), fuel_one);
    assert_exact_line_partitions(&fuel_one);

    let mut depth = 0_usize;
    let mut shape = Vec::new();
    let mut container_claims = Vec::new();
    let mut closed_fence = false;
    let mut list_tightness = Vec::new();
    for command in &fuel_one {
        match command {
            BlockCommand::Enter { kind } => {
                shape.push((*kind, depth));
                depth += 1;
            }
            BlockCommand::Coverage {
                owner,
                part: CoveragePart::ContainerMarker,
                source,
                ..
            } => container_claims.push((
                owner.generations_from_top(),
                source.start().byte()..source.end().byte(),
            )),
            BlockCommand::Close {
                kind, final_facts, ..
            } => {
                depth -= 1;
                match final_facts {
                    FinalFacts::FencedCode(facts) => closed_fence |= facts.closed(),
                    FinalFacts::List { tight } => list_tightness.push(*tight),
                    FinalFacts::None => {}
                }
                assert_eq!(
                    shape
                        .iter()
                        .rev()
                        .find(|(_, item_depth)| *item_depth == depth)
                        .map(|(candidate, _)| candidate),
                    Some(kind)
                );
            }
            _ => {}
        }
    }

    assert!(shape
        .iter()
        .any(|(kind, depth)| { *kind == BlockKind::BlockQuote && *depth == 3 }));
    assert!(shape
        .iter()
        .any(|(kind, depth)| { matches!(kind, BlockKind::FencedCode(_)) && *depth == 3 }));
    assert!(container_claims.contains(&(0, 0..2)));
    assert!(container_claims.contains(&(0, 2..4)));
    assert!(container_claims.contains(&(1, 0..2)));
    assert!(closed_fence);
    assert_eq!(list_tightness, vec![true]);
    assert_eq!(depth, 0);
    assert!(matches!(
        fuel_one.last(),
        Some(BlockCommand::FinishDocument)
    ));
}

#[test]
fn generated_atx_lookahead_never_spends_virtual_source_budget() {
    for fuel in [1, 7, 64] {
        let commands = drive("# heading\n", fuel);
        assert_exact_line_partitions(&commands);
        assert!(commands.iter().any(|command| matches!(
            command,
            BlockCommand::Enter {
                kind: BlockKind::Heading(_)
            }
        )));
    }
}

#[test]
fn long_atx_tail_keeps_additive_work_and_retained_source_bounded() {
    let source_text = format!("# {}\n", "a".repeat(64 * 1024));
    let store = SourceStore::new(&source_text).expect("source");
    let mut scanner = SnapshotLineScanner::new(store.snapshot()).expect("line scanner");
    let line = loop {
        match scanner.poll(64).expect("line discovery") {
            SnapshotLinePoll::Pending(next) => scanner = next,
            SnapshotLinePoll::Line(line) => break line,
            SnapshotLinePoll::Complete => panic!("fixture has one line"),
        }
    };
    let facts = line.facts();
    let mut source = line.into_source().expect("source-backed line");
    let mut controller = M11DirectBlockController::new().expect("direct controller");
    let mut ignored = Vec::new();
    acknowledge(&mut controller, &mut ignored);
    let mut admission =
        <M11DirectBlockController as M11ExactController<SnapshotLineSource>>::begin_source_line(
            &mut controller,
            facts.identity(),
        )
        .expect("line admission");

    loop {
        if source.access_budget() == 0 && source.position() < source.len() {
            source
                .replenish_access_budget(SOURCE_CURSOR_WINDOW_BYTES)
                .expect("bounded source grant");
        }
        let budget_before = source.access_budget();
        let position_before = source.position();
        let receipt =
            <M11DirectBlockController as M11ExactController<SnapshotLineSource>>::poll_source_line(
                &mut controller,
                &mut admission,
                &mut source,
                64,
            )
            .expect("source poll");
        assert!(receipt.lexical_work_units <= 64 + M11_DIRECT_BLOCK_MAX_LEXICAL_SLACK);
        assert!(receipt.source_first_reads <= 64);
        assert_eq!(
            budget_before - source.access_budget(),
            receipt.source_first_reads
        );
        assert_eq!(
            source.position() - position_before,
            receipt.source_first_reads
        );
        assert!(receipt.retained_source_bytes <= M11_DIRECT_BLOCK_MAX_RETAINED_SOURCE_BYTES);
        if receipt.status == M11SourceLinePollStatus::Matched {
            break;
        }
    }

    <M11DirectBlockController as M11ExactController<SnapshotLineSource>>::cancel_source_line(
        &mut controller,
        admission,
    )
    .expect("cancel completed recognition without publication");
    let _scanner = source.finish().expect("source is physically exhausted");
}

#[test]
fn source_backed_cm325_preserves_blank_gap_and_list_tightness() {
    const SOURCE: &str = "* foo\n  * bar\n\n  baz\n";
    let fuel_one = drive(SOURCE, 1);
    assert_eq!(drive(SOURCE, 7), fuel_one);
    assert_eq!(drive(SOURCE, 64), fuel_one);
    assert_exact_line_partitions(&fuel_one);

    let tightness = fuel_one
        .iter()
        .filter_map(|command| match command {
            BlockCommand::Close {
                kind: BlockKind::List(_),
                final_facts: FinalFacts::List { tight },
                ..
            } => Some(*tight),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tightness, vec![true, false]);
    assert!(fuel_one
        .iter()
        .any(|command| matches!(command, BlockCommand::StageBlankGap { .. })));
    assert!(fuel_one.iter().any(|command| matches!(
        command,
        BlockCommand::ResolveBlankGap { owner } if owner.generations_from_top() == 0
    )));
}

fn thematic_count(commands: &[BlockCommand]) -> usize {
    commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                BlockCommand::Enter {
                    kind: BlockKind::ThematicBreak
                }
            )
        })
        .count()
}

#[test]
fn thematic_break_is_an_exact_same_line_leaf_at_fuel_one() {
    let compact = drive("***\n", 1);
    assert_exact_line_partitions(&compact);
    assert_eq!(thematic_count(&compact), 1);
    assert!(compact.windows(4).any(|commands| matches!(
        commands,
        [
            BlockCommand::Enter {
                kind: BlockKind::ThematicBreak,
            },
            BlockCommand::Coverage {
                owner,
                part: CoveragePart::BlockMarker,
                source,
                ..
            },
            BlockCommand::Coverage {
                part: CoveragePart::Terminal,
                ..
            },
            BlockCommand::Close {
                kind: BlockKind::ThematicBreak,
                final_facts: FinalFacts::None,
                ..
            },
        ] if owner.generations_from_top() == 0
            && source.start().byte() == 0
            && source.end().byte() == 3
    )));

    let spaced = drive("  * * *  \r\n", 1);
    assert_exact_line_partitions(&spaced);
    assert_eq!(thematic_count(&spaced), 1);
    assert!(spaced.iter().any(|command| matches!(
        command,
        BlockCommand::Coverage {
            part: CoveragePart::BlockMarker,
            source,
            ..
        } if source.start().byte() == 0 && source.end().byte() == 9
    )));
}

#[test]
fn thematic_precedence_does_not_steal_setext_or_list_markers() {
    let setext = drive("Foo\n---\n", 1);
    assert_eq!(thematic_count(&setext), 0);
    assert!(setext
        .iter()
        .any(|command| matches!(command, BlockCommand::FinalizeParagraph { .. })));

    let list_child = drive("- Foo\n- * * *\n", 1);
    assert_eq!(thematic_count(&list_child), 1);
    let thematic_depth = list_child
        .iter()
        .scan(0_usize, |depth, command| match command {
            BlockCommand::Enter {
                kind: BlockKind::ThematicBreak,
            } => {
                let result = Some(Some(*depth));
                *depth += 1;
                result
            }
            BlockCommand::Enter { .. } => {
                *depth += 1;
                Some(None)
            }
            BlockCommand::Close { .. } => {
                *depth -= 1;
                Some(None)
            }
            _ => Some(None),
        })
        .flatten()
        .next()
        .expect("nested thematic break");
    assert_eq!(thematic_depth, 3, "Document/List/Item own the leaf");
}

#[test]
fn old_container_to_new_top_level_block_is_one_ordered_transaction() {
    for source in [
        "- foo\n***\n- bar\n",
        "- foo\n- bar\n+ baz\n",
        "****\n## foo\n****\n",
        "> foo\n---\n",
    ] {
        let commands = drive(source, 1);
        assert_exact_line_partitions(&commands);
    }
}

#[test]
fn indented_code_keeps_exact_deindent_and_trailing_blank_decision() {
    const SOURCE: &str = "    alpha\n\n    beta\n\noutside\n";
    let commands = drive(SOURCE, 1);
    assert_exact_line_partitions(&commands);
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(
                command,
                BlockCommand::Enter {
                    kind: BlockKind::IndentedCode
                }
            ))
            .count(),
        1,
    );
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(
                command,
                BlockCommand::Coverage {
                    part: CoveragePart::BlockMarker,
                    source,
                    ..
                } if source.start().byte() == 0 && source.end().byte() == 4
            ))
            .count(),
        2,
    );
    assert!(commands.iter().any(|command| matches!(
        command,
        BlockCommand::ResolveTerminator {
            resolution: flark_parser::block_core::TerminatorResolution::ContinueCanonicalNewline
        }
    )));
    assert!(commands.iter().any(|command| matches!(
        command,
        BlockCommand::ResolveTerminator {
            resolution: flark_parser::block_core::TerminatorResolution::CloseNone
        }
    )));
}

#[test]
fn html_block_types_one_through_seven_cross_the_same_exact_command_seam() {
    let cases = [
        (1, "<script>\nx\n</script>\n"),
        (2, "<!--x-->\n"),
        (3, "<?x?>\n"),
        (4, "<!A>\n"),
        (5, "<![CDATA[x]]>\n"),
        (6, "<div>\nx\n\n"),
        (7, "<custom>\nx\n\n"),
    ];
    for (expected_type, source) in cases {
        let commands = drive(source, 1);
        assert_exact_line_partitions(&commands);
        assert!(commands.iter().any(|command| matches!(
            command,
            BlockCommand::Enter {
                kind: BlockKind::HtmlBlock(facts)
            } if facts.block_type().get() == expected_type
        )));
    }
}

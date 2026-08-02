// The upstream donor's exhaustive pause/conformance suite remains in
// `tool/parser_research/comrak_value_block_core`. Production integration is
// covered through `flark_parser`'s block-core controller and writer tests.

use super::*;

fn drive_line(parser: &mut DirectValueBlockParser, line: &str) -> (Vec<DirectCommand>, usize) {
    parser.begin_line(line.to_owned()).expect("begin line");
    let mut commands = Vec::new();
    let mut transitions = 0_usize;
    for _ in 0..1_000_000 {
        let receipt = parser.poll_line(1).expect("poll line");
        assert!(receipt.transitions <= 1);
        transitions += receipt.transitions;
        match receipt.status {
            DirectPollStatus::Pending => {}
            DirectPollStatus::ExternalWorkReady => {
                panic!("deep-container witness has no external work")
            }
            DirectPollStatus::Complete => panic!("FinishLine is acknowledged explicitly"),
            DirectPollStatus::CommandReady => {
                let command = parser.pending_command().expect("ready command").clone();
                let finished = matches!(command, DirectCommand::FinishLine { .. });
                commands.push(command);
                parser.acknowledge_command().expect("acknowledge command");
                if finished {
                    return (commands, transitions);
                }
            }
        }
    }
    panic!("line converges under fuel one")
}

fn apply_stack(commands: &[DirectCommand], stack: &mut Vec<DirectBlockKind>) {
    for command in commands {
        match command {
            DirectCommand::Open { kind } => stack.push(*kind),
            DirectCommand::Close { kind, .. } => {
                assert_eq!(stack.pop(), Some(*kind), "close order is exact LIFO");
            }
            DirectCommand::Consume { .. }
            | DirectCommand::StageTerminator { .. }
            | DirectCommand::ResolveTerminator { .. }
            | DirectCommand::StageBlankGap { .. }
            | DirectCommand::ResolveBlankGap { .. }
            | DirectCommand::FinalizeParagraph { .. }
            | DirectCommand::MarkFencedCodeBoundary { .. }
            | DirectCommand::FinishLine { .. }
            | DirectCommand::FinishDocument => {}
        }
    }
}

#[test]
fn adversarial_deep_mass_close_uses_one_linear_stack_pass() {
    const DEPTH: usize = 512;

    let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).expect("parser");
    assert!(matches!(
        parser.pending_command(),
        Some(DirectCommand::Open {
            kind: DirectBlockKind::Document
        })
    ));
    parser
        .acknowledge_command()
        .expect("acknowledge document open");

    let nested = format!("{}leaf\n", "> ".repeat(DEPTH));
    let (open_commands, _) = drive_line(&mut parser, &nested);
    let mut stack = vec![DirectBlockKind::Document];
    apply_stack(&open_commands, &mut stack);
    assert_eq!(stack.len(), DEPTH + 2, "Document, quotes, Paragraph");
    assert_eq!(stack.last(), Some(&DirectBlockKind::Paragraph));

    let (close_commands, close_transitions) = drive_line(&mut parser, "***\n");
    let direct = parser.parser.direct.as_ref().expect("direct hooks");
    assert_eq!(
        direct.retired_insertions,
        DEPTH + 1,
        "each retired Paragraph/quote is keyed once",
    );
    assert_eq!(
        direct.retired_stack_probes,
        DEPTH + 2,
        "one reverse suffix walk visits each retired frame and the live Document",
    );
    assert!(
        close_transitions <= DEPTH * 3 + 32,
        "grammar control remains linearly fuelled: {close_transitions}",
    );

    let closes_before_replacement = close_commands
        .iter()
        .take_while(|command| {
            !matches!(
                command,
                DirectCommand::Open {
                    kind: DirectBlockKind::ThematicBreak
                }
            )
        })
        .filter_map(|command| match command {
            DirectCommand::Close { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(closes_before_replacement.len(), DEPTH + 1);
    assert_eq!(
        closes_before_replacement.first(),
        Some(&DirectBlockKind::Paragraph)
    );
    assert!(
        closes_before_replacement[1..]
            .iter()
            .all(|kind| *kind == DirectBlockKind::BlockQuote)
    );

    apply_stack(&close_commands, &mut stack);
    assert_eq!(stack, vec![DirectBlockKind::Document]);
}

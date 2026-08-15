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
fn valid_open_html_boundary_is_unavailable_without_poisoning_the_parse() {
    let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).expect("parser");
    parser
        .acknowledge_command()
        .expect("acknowledge document open");

    drive_line(&mut parser, "<a href=\"/bar\\/)\">\n");
    assert!(matches!(
        parser
            .capture_line_boundary_pause_if_available()
            .expect("valid HTML boundary is not a parser failure"),
        DirectLineBoundaryPauseCapture::Unavailable,
    ));

    drive_line(&mut parser, "\n");
    assert!(matches!(
        parser
            .capture_line_boundary_pause_if_available()
            .expect("parse continues after skipped sample"),
        DirectLineBoundaryPauseCapture::Available(_),
    ));
}

#[test]
fn gfm_profile_survives_direct_pause_resume() {
    let mut parser = DirectValueBlockParser::new(SyntaxProfile::Gfm).expect("GFM parser");
    parser
        .acknowledge_command()
        .expect("acknowledge document open");
    drive_line(&mut parser, "ordinary paragraph\n");

    let pause = parser
        .capture_line_boundary_pause()
        .expect("GFM line boundary is resumable");
    let view = pause.pairing_view();
    assert_eq!(view.profile(), SyntaxProfile::Gfm);
    let cursor = DirectLineBoundaryResumeCursor::new(
        u64::try_from(view.line_number()).expect("line number"),
        u64::try_from(view.last_line_length()).expect("line length"),
    )
    .expect("resume cursor");
    let (grammar, output) = pause.into_restart_parts().expect("restart parts");
    let resumed = DirectValueBlockParser::resume_restart_parts(&grammar, output, cursor)
        .expect("resume GFM parser");
    assert_eq!(resumed.parser.profile, SyntaxProfile::Gfm);
}

#[test]
fn optional_capture_propagates_malformed_parser_state() {
    let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).expect("parser");
    parser
        .acknowledge_command()
        .expect("acknowledge document open");
    drive_line(&mut parser, "ordinary paragraph\n");

    let root = parser.parser.tree.root;
    parser.parser.tree.node_mut(root).open = false;
    assert_eq!(
        parser.capture_line_boundary_pause_if_available(),
        Err(ParseError::Invariant(
            "direct pause frame is compact bounded scratch",
        )),
        "optional sampling must not relabel malformed state as unavailable",
    );
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

#[test]
fn leading_reference_remainder_continuation_resumes_without_replaying_prefix_state() {
    let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).expect("parser");
    parser
        .acknowledge_command()
        .expect("acknowledge document open");
    drive_line(&mut parser, "[ref]: /url\n");
    drive_line(&mut parser, "visible\n");

    // The reference actor is independently tested; install the exact state it
    // leaves after joining a VisibleRemainder terminal so this focused donor
    // test exercises only the new retrospective continuation primitive.
    parser
        .parser
        .direct
        .as_mut()
        .expect("direct hooks")
        .reference_finalize_resume_once = Some(DirectReferencePrefixDisposition::VisibleRemainder);
    let continuation = parser
        .capture_leading_reference_remainder_continuation()
        .expect("capture remainder continuation")
        .expect("quiescent top-level remainder is eligible");
    let (grammar, output) = continuation.into_restart_parts();
    let cursor = DirectLineBoundaryResumeCursor::new(1, 11).expect("prefix cursor");
    let mut resumed = DirectValueBlockParser::resume_restart_parts(&grammar, output, cursor)
        .expect("resume remainder continuation");

    let pause = resumed
        .capture_line_boundary_pause()
        .expect("remainder starts at a normal line boundary");
    assert_eq!(
        pause.pairing_view().deferred_role(),
        DirectLineBoundaryDeferredRole::None,
        "removed definition terminator is already Green Gap coverage",
    );
    assert_eq!(
        pause.paragraph,
        Some(DirectPauseParagraphState {
            frame_depth: 1,
            has_visible_content: true,
            may_have_reference_prefix: false,
        }),
        "the visible suffix cannot request reference recognition again",
    );

    let (commands, _) = drive_line(&mut resumed, "changed\n");
    assert!(
        commands
            .iter()
            .all(|command| !matches!(command, DirectCommand::ResolveTerminator { .. })),
        "the removed prefix terminator is not replayed into visible logical text",
    );
}

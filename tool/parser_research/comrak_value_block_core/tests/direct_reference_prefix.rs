use flark_comrak_value_block_core::{
    DirectBlockKind, DirectCommand, DirectCoveragePart, DirectExternalWork, DirectLogicalAction,
    DirectParagraphOutcome, DirectPollStatus, DirectReferenceDefinition,
    DirectReferencePrefixCommitStatus, DirectReferencePrefixContext,
    DirectReferencePrefixOutputAckStatus, DirectReferencePrefixPollStatus,
    DirectReferencePrefixSource, DirectReferencePrefixWork, DirectTerminatorResolution,
    DirectValueBlockParser, SyntaxProfile,
};

#[derive(Debug)]
enum SourceError {
    NonSequential,
}

struct LogicalSource<'a> {
    identity: u64,
    bytes: &'a [u8],
    next: usize,
    used: usize,
}

impl DirectReferencePrefixSource for LogicalSource<'_> {
    type Identity = u64;
    type Error = SourceError;

    fn identity(&self) -> Self::Identity {
        self.identity
    }

    fn available_len(&self) -> usize {
        self.bytes.len()
    }

    fn is_final(&self) -> bool {
        true
    }

    fn access_budget(&self) -> usize {
        1_usize.saturating_sub(self.used)
    }

    fn read_byte(&mut self, relative_offset: usize) -> Result<u8, Self::Error> {
        if relative_offset != self.next || self.used != 0 {
            return Err(SourceError::NonSequential);
        }
        let byte = self.bytes[relative_offset];
        self.next += 1;
        self.used = 1;
        Ok(byte)
    }

    fn raw_codepoint_contribution(&self, _logical_scalar_end_offset: usize) -> u8 {
        1
    }
}

fn started() -> (DirectValueBlockParser, Vec<DirectCommand>) {
    let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
    let root = parser.pending_command().unwrap().clone();
    parser.acknowledge_command().unwrap();
    (parser, vec![root])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drive {
    Complete,
    External,
}

fn poll_line(parser: &mut DirectValueBlockParser, commands: &mut Vec<DirectCommand>) -> Drive {
    for _ in 0..10_000 {
        let receipt = parser.poll_line(1).unwrap();
        assert!(receipt.transitions <= 1);
        match receipt.status {
            DirectPollStatus::Pending => {}
            DirectPollStatus::CommandReady => {
                commands.push(parser.pending_command().unwrap().clone());
                parser.acknowledge_command().unwrap();
            }
            DirectPollStatus::ExternalWorkReady => return Drive::External,
            DirectPollStatus::Complete => return Drive::Complete,
        }
    }
    panic!("line did not converge");
}

fn poll_finish(parser: &mut DirectValueBlockParser, commands: &mut Vec<DirectCommand>) -> Drive {
    for _ in 0..10_000 {
        let receipt = parser.poll_finish(1).unwrap();
        assert!(receipt.transitions <= 1);
        match receipt.status {
            DirectPollStatus::Pending => {}
            DirectPollStatus::CommandReady => {
                commands.push(parser.pending_command().unwrap().clone());
                parser.acknowledge_command().unwrap();
            }
            DirectPollStatus::ExternalWorkReady => return Drive::External,
            DirectPollStatus::Complete => return Drive::Complete,
        }
    }
    panic!("finish did not converge");
}

fn join_reference_work(
    parser: &mut DirectValueBlockParser,
    logical_paragraph: &str,
) -> (
    DirectReferencePrefixCommitStatus,
    Vec<DirectReferenceDefinition>,
) {
    let request = match parser.pending_external_work().unwrap() {
        DirectExternalWork::ReferencePrefixFinalizer { request } => *request,
    };
    assert_eq!(request.logical_base().bytes, 0);
    let mut work: DirectReferencePrefixWork<u64> =
        parser.begin_reference_prefix_work(request, 71).unwrap();
    let mut source = LogicalSource {
        identity: 71,
        bytes: logical_paragraph.as_bytes(),
        next: 0,
        used: 0,
    };
    let mut definitions = Vec::new();
    loop {
        source.used = 0;
        let receipt = work.poll_source(&mut source, 1, false).unwrap();
        assert!(receipt.inspected_bytes <= 1);
        match receipt.status {
            DirectReferencePrefixPollStatus::NeedMore => {}
            DirectReferencePrefixPollStatus::OutputReady => {
                let (definition, ack) = work.take_output().unwrap().acknowledge();
                definitions.push(definition);
                if work.acknowledge_output(ack).unwrap()
                    == DirectReferencePrefixOutputAckStatus::Complete
                {
                    let terminal = match work.take_terminal() {
                        Ok(terminal) => terminal,
                        Err(_) => panic!("completed ack yields its terminal capability"),
                    };
                    return (
                        parser
                            .commit_reference_prefix_terminal(terminal.acknowledge(), 71)
                            .unwrap(),
                        definitions,
                    );
                }
            }
            DirectReferencePrefixPollStatus::Complete => {
                let terminal = match work.take_terminal() {
                    Ok(terminal) => terminal,
                    Err(_) => panic!("complete work yields its terminal capability"),
                };
                let ack = terminal.acknowledge();
                return (
                    parser.commit_reference_prefix_terminal(ack, 71).unwrap(),
                    definitions,
                );
            }
            DirectReferencePrefixPollStatus::Cancelled => panic!("unexpected cancellation"),
        }
    }
}

#[test]
fn rejected_candidate_leaves_already_replayed_paragraph_unchanged() {
    let (mut parser, mut commands) = started();
    parser
        .begin_line("[not a definition]\n".to_owned())
        .unwrap();
    assert_eq!(poll_line(&mut parser, &mut commands), Drive::Complete);
    let command_count_before_finalize = commands.len();
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, DirectCommand::Consume { .. }))
    );

    parser.begin_finish().unwrap();
    assert_eq!(poll_finish(&mut parser, &mut commands), Drive::External);
    let request = parser.pending_external_work().unwrap().request();
    assert_eq!(
        request.context(),
        DirectReferencePrefixContext::ParagraphFinalization
    );
    assert!(request.include_pending_terminator());
    assert_eq!(
        join_reference_work(&mut parser, "[not a definition]\n").0,
        DirectReferencePrefixCommitStatus::ParagraphUnchangedArmed
    );
    assert_eq!(poll_finish(&mut parser, &mut commands), Drive::Complete);
    assert!(commands.len() > command_count_before_finalize);
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command, DirectCommand::Consume { .. }))
            .count(),
        1,
        "finalizer rejection must not replay source through a second classifier"
    );
}

#[test]
fn setext_underline_is_not_reference_definition_lookahead() {
    let (mut parser, mut commands) = started();
    parser.begin_line("[x]:\n".to_owned()).unwrap();
    assert_eq!(poll_line(&mut parser, &mut commands), Drive::Complete);

    parser.begin_line("---\n".to_owned()).unwrap();
    assert_eq!(poll_line(&mut parser, &mut commands), Drive::External);
    assert_eq!(
        parser.pending_external_work().unwrap().request().context(),
        DirectReferencePrefixContext::SetextCandidate
    );
    assert_eq!(
        join_reference_work(&mut parser, "[x]:\n").0,
        DirectReferencePrefixCommitStatus::ParagraphUnchangedArmed
    );
    assert_eq!(poll_line(&mut parser, &mut commands), Drive::Complete);
    assert!(commands.iter().any(|command| matches!(
        command,
        DirectCommand::FinalizeParagraph {
            outcome: DirectParagraphOutcome::SetextHeading { level: 2 }
        }
    )));
}

#[test]
fn reference_only_setext_candidate_reuses_empty_paragraph_shell_as_literal_text() {
    for underline in ["---\n", "===\n"] {
        let (mut parser, mut commands) = started();
        parser.begin_line("[foo]: /url\n".to_owned()).unwrap();
        assert_eq!(poll_line(&mut parser, &mut commands), Drive::Complete);

        parser.begin_line(underline.to_owned()).unwrap();
        assert_eq!(poll_line(&mut parser, &mut commands), Drive::External);
        assert_eq!(
            parser.pending_external_work().unwrap().request().context(),
            DirectReferencePrefixContext::SetextCandidate
        );
        let (status, definitions) = join_reference_work(&mut parser, "[foo]: /url\n");
        assert_eq!(
            status,
            DirectReferencePrefixCommitStatus::ReferenceOnlyArmed
        );
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].normalized_label, "foo");

        let underline_start = commands.len();
        assert_eq!(poll_line(&mut parser, &mut commands), Drive::Complete);
        let underline_commands = &commands[underline_start..];
        assert!(
            !underline_commands.iter().any(|command| matches!(
                command,
                DirectCommand::ResolveTerminator {
                    resolution: DirectTerminatorResolution::ContinueCanonicalNewline
                }
            )),
            "the accepted definition prefix owns its staged terminator"
        );
        assert!(underline_commands.iter().any(|command| matches!(
            command,
            DirectCommand::Consume {
                part: DirectCoveragePart::Content,
                range,
                logical: DirectLogicalAction::CanonicalText,
                ..
            } if range == &(0..3)
        )));
        assert!(
            !underline_commands.iter().any(|command| matches!(
                command,
                DirectCommand::FinalizeParagraph {
                    outcome: DirectParagraphOutcome::SetextHeading { .. }
                } | DirectCommand::Open {
                    kind: DirectBlockKind::Heading(_) | DirectBlockKind::Paragraph
                }
            )),
            "underline {underline:?} must reuse the one open Paragraph: {underline_commands:#?}"
        );

        parser.begin_line("[foo]\n".to_owned()).unwrap();
        assert_eq!(poll_line(&mut parser, &mut commands), Drive::Complete);
        parser.begin_finish().unwrap();
        assert_eq!(poll_finish(&mut parser, &mut commands), Drive::Complete);
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(
                    command,
                    DirectCommand::Open {
                        kind: DirectBlockKind::Paragraph
                    }
                ))
                .count(),
            1,
            "definition prefix, underline, and following reference stay in one parser shell"
        );
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(
                    command,
                    DirectCommand::Close {
                        kind: DirectBlockKind::Paragraph,
                        ..
                    }
                ))
                .count(),
            1
        );
    }
}

#[test]
fn multiline_definition_waits_for_completed_paragraph_projection() {
    let (mut parser, mut commands) = started();
    parser.begin_line("[x]:\n".to_owned()).unwrap();
    assert_eq!(poll_line(&mut parser, &mut commands), Drive::Complete);
    assert!(parser.pending_external_work().is_none());

    parser.begin_line(" /url\n".to_owned()).unwrap();
    assert_eq!(poll_line(&mut parser, &mut commands), Drive::Complete);
    assert!(parser.pending_external_work().is_none());

    parser.begin_finish().unwrap();
    assert_eq!(poll_finish(&mut parser, &mut commands), Drive::External);
    assert_eq!(
        parser.pending_external_work().unwrap().request().context(),
        DirectReferencePrefixContext::ParagraphFinalization
    );
    let (status, definitions) = join_reference_work(&mut parser, "[x]:\n /url\n");
    assert_eq!(
        status,
        DirectReferencePrefixCommitStatus::ReferenceOnlyArmed
    );
    assert!(parser.pending_external_work().is_none());
    let definition = &definitions[0];
    assert_eq!(definition.normalized_label, "x");
    assert_eq!(definition.logical_source.bytes, 0..11);
    assert_eq!(definition.logical_destination.bytes, 6..10);
    assert_eq!(poll_finish(&mut parser, &mut commands), Drive::Complete);
    assert!(!commands.iter().any(|command| matches!(
        command,
        DirectCommand::Close {
            kind: flark_comrak_value_block_core::DirectBlockKind::Paragraph,
            ..
        }
    )));
}

#[test]
fn multiline_reference_candidate_survives_line_boundary_checkpoint() {
    let (mut parser, mut commands) = started();
    parser.begin_line("[x]:\n".to_owned()).unwrap();
    assert_eq!(poll_line(&mut parser, &mut commands), Drive::Complete);
    let pause = parser.capture_line_boundary_pause().unwrap();
    let mut parser = DirectValueBlockParser::resume_line_boundary_pause(pause).unwrap();

    parser.begin_line(" /url\n".to_owned()).unwrap();
    assert_eq!(poll_line(&mut parser, &mut commands), Drive::Complete);
    parser.begin_finish().unwrap();
    assert_eq!(poll_finish(&mut parser, &mut commands), Drive::External);
    let (status, definitions) = join_reference_work(&mut parser, "[x]:\n /url\n");
    assert_eq!(
        status,
        DirectReferencePrefixCommitStatus::ReferenceOnlyArmed
    );
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].normalized_label, "x");
    assert_eq!(definitions[0].logical_destination.bytes, 6..10);
}

#[test]
fn duplicate_occurrences_stream_in_order_before_visible_remainder_terminal() {
    let (mut parser, mut commands) = started();
    for line in ["[x]: /one\n", "[x]: /two\n", "visible\n"] {
        parser.begin_line(line.to_owned()).unwrap();
        assert_eq!(poll_line(&mut parser, &mut commands), Drive::Complete);
    }
    parser.begin_finish().unwrap();
    assert_eq!(poll_finish(&mut parser, &mut commands), Drive::External);
    let (status, definitions) = join_reference_work(&mut parser, "[x]: /one\n[x]: /two\nvisible\n");
    assert_eq!(
        status,
        DirectReferencePrefixCommitStatus::VisibleRemainderArmed
    );
    assert_eq!(definitions.len(), 2);
    assert_eq!(definitions[0].logical_destination.bytes, 5..9);
    assert_eq!(definitions[1].logical_destination.bytes, 15..19);
    assert_eq!(poll_finish(&mut parser, &mut commands), Drive::Complete);
    assert!(commands.iter().any(|command| matches!(
        command,
        DirectCommand::Close {
            kind: flark_comrak_value_block_core::DirectBlockKind::Paragraph,
            ..
        }
    )));
}

#[test]
fn following_block_is_not_consumed_by_reference_finalizer() {
    let (mut parser, mut commands) = started();
    parser.begin_line("[x]:\n".to_owned()).unwrap();
    assert_eq!(poll_line(&mut parser, &mut commands), Drive::Complete);

    parser.begin_line("\n".to_owned()).unwrap();
    assert_eq!(poll_line(&mut parser, &mut commands), Drive::External);
    assert_eq!(
        parser.pending_external_work().unwrap().request().context(),
        DirectReferencePrefixContext::ParagraphFinalization
    );
    assert_eq!(
        join_reference_work(&mut parser, "[x]:\n").0,
        DirectReferencePrefixCommitStatus::ParagraphUnchangedArmed
    );
    assert_eq!(poll_line(&mut parser, &mut commands), Drive::Complete);
    parser.begin_line("# heading\n".to_owned()).unwrap();
    assert_eq!(poll_line(&mut parser, &mut commands), Drive::Complete);
    assert!(
        commands.iter().any(|command| matches!(
            command,
            DirectCommand::Open {
                kind: flark_comrak_value_block_core::DirectBlockKind::Heading(_)
            }
        )),
        "commands: {commands:#?}"
    );
}

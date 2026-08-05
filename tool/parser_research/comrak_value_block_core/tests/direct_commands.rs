use flark_comrak_value_block_core::{
    BlockDocument, BlockKind, DirectBlockKind, DirectClosedChild, DirectCommand,
    DirectCoveragePart, DirectFenceCharacter, DirectFencedCodeBoundary, DirectFencedCodeCloseFacts,
    DirectFencedCodeFacts, DirectFinalFacts, DirectHeadingFacts, DirectItemFacts, DirectLineEnding,
    DirectListFacts, DirectLogicalAction, DirectOwner, DirectParagraphOutcome, DirectPollStatus,
    DirectTerminatorResolution, DirectUnsupported, DirectValueBlockParser, ListDelimiter, ListType,
    NodeId, ParseError, SourceDocument, SyntaxProfile, parse_document,
};

fn acknowledge_pending(parser: &mut DirectValueBlockParser, commands: &mut Vec<DirectCommand>) {
    let command = parser.pending_command().expect("command is ready").clone();
    commands.push(command);
    parser
        .acknowledge_command()
        .expect("command acknowledgement succeeds");
}

fn begin(parser: &mut DirectValueBlockParser, commands: &mut Vec<DirectCommand>) {
    assert_eq!(
        parser.pending_command(),
        Some(&DirectCommand::Open {
            kind: DirectBlockKind::Document,
        })
    );
    acknowledge_pending(parser, commands);
}

fn drive_line(parser: &mut DirectValueBlockParser, line: &str, commands: &mut Vec<DirectCommand>) {
    parser.begin_line(line.to_owned()).expect("line begins");
    let max_polls = line.len().saturating_mul(8).saturating_add(256);
    for _ in 0..max_polls {
        let receipt = parser.poll_line(1).expect("line poll succeeds");
        assert!(
            receipt.transitions <= 1,
            "fuel-one poll overran: {receipt:?}"
        );
        match receipt.status {
            DirectPollStatus::CommandReady => acknowledge_pending(parser, commands),
            DirectPollStatus::Pending => {}
            DirectPollStatus::ExternalWorkReady => {
                panic!("generic direct-line helper does not resolve external work")
            }
            DirectPollStatus::Complete => {
                assert_eq!(parser.retained_line_bytes(), 0);
                assert_eq!(parser.retained_logical_bytes(), 0);
                assert_eq!(parser.legacy_event_count(), 0);
                return;
            }
        }
    }
    panic!("line did not converge under fuel-one polling");
}

fn finish(parser: &mut DirectValueBlockParser, commands: &mut Vec<DirectCommand>) {
    parser.begin_finish().expect("finish begins");
    for _ in 0..100_000 {
        let receipt = parser.poll_finish(1).expect("finish poll succeeds");
        assert!(
            receipt.transitions <= 1,
            "fuel-one poll overran: {receipt:?}"
        );
        match receipt.status {
            DirectPollStatus::CommandReady => acknowledge_pending(parser, commands),
            DirectPollStatus::Pending => {}
            DirectPollStatus::ExternalWorkReady => {
                panic!("generic finish helper does not resolve external work")
            }
            DirectPollStatus::Complete => return,
        }
    }
    panic!("finish did not converge under fuel-one polling");
}

fn drive_source(source: &str) -> (DirectValueBlockParser, Vec<DirectCommand>) {
    let source_document = SourceDocument::new(source);
    let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
    let mut commands = Vec::new();
    begin(&mut parser, &mut commands);
    for leaf in source_document.leaves {
        drive_line(&mut parser, &leaf.text, &mut commands);
    }
    finish(&mut parser, &mut commands);
    assert_exact_line_partitions(&commands);
    (parser, commands)
}

fn assert_exact_line_partitions(commands: &[DirectCommand]) {
    let mut covered = 0_u32;
    for command in commands {
        let range = match command {
            DirectCommand::Consume { range, .. }
            | DirectCommand::StageTerminator { range, .. }
            | DirectCommand::StageBlankGap { range } => Some(range),
            _ => None,
        };
        if let Some(range) = range {
            assert_eq!(range.start, covered, "line source claims are contiguous");
            assert!(range.end >= range.start, "source claim is ordered");
            covered = range.end;
        }
        if let DirectCommand::FinishLine { physical_bytes, .. } = command {
            assert_eq!(covered, *physical_bytes, "line source is covered exactly");
            covered = 0;
        }
    }
    assert_eq!(covered, 0, "document does not retain a partial source line");
}

fn direct_semantic_shape(commands: &[DirectCommand]) -> Vec<(DirectBlockKind, usize)> {
    let mut stack = Vec::<(DirectBlockKind, usize)>::new();
    let mut shape = Vec::new();
    let mut finished = false;
    for command in commands {
        match command {
            DirectCommand::Open { kind } => {
                let index = shape.len();
                shape.push((*kind, stack.len()));
                stack.push((*kind, index));
            }
            DirectCommand::FinalizeParagraph {
                outcome: DirectParagraphOutcome::SetextHeading { level },
            } => {
                let Some((kind, shape_index)) = stack.last_mut() else {
                    panic!("Paragraph finalization has an open stack");
                };
                assert_eq!(*kind, DirectBlockKind::Paragraph);
                *kind = DirectBlockKind::Heading(DirectHeadingFacts {
                    level: *level,
                    setext: true,
                });
                shape[*shape_index].0 = *kind;
            }
            DirectCommand::Close { kind, .. } => {
                assert_eq!(
                    stack.pop().map(|(kind, _)| kind),
                    Some(*kind),
                    "close matches the stack top"
                );
            }
            DirectCommand::FinishDocument => {
                assert!(stack.is_empty(), "document finishes with an empty stack");
                assert!(!finished, "document finishes exactly once");
                finished = true;
            }
            _ => {}
        }
    }
    assert!(finished, "document finish is present");
    shape
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InterpretedClaim {
    part: DirectCoveragePart,
    range: std::ops::Range<u32>,
    owner: DirectBlockKind,
    owner_depth: usize,
    generations_from_top: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InterpretedGapResolution {
    owner: DirectBlockKind,
    owner_depth: usize,
    generations_from_top: u32,
}

fn owner_index(stack: &[DirectBlockKind], owner: DirectOwner) -> usize {
    stack
        .len()
        .checked_sub(owner.generations_from_top() as usize + 1)
        .expect("direct owner names an open stack ancestor")
}

fn interpreted_source(
    commands: &[DirectCommand],
) -> (Vec<InterpretedClaim>, Vec<InterpretedGapResolution>) {
    let mut stack = Vec::new();
    let mut claims = Vec::new();
    let mut gaps = Vec::new();
    for command in commands {
        match command {
            DirectCommand::Open { kind } => stack.push(*kind),
            DirectCommand::Consume {
                owner, part, range, ..
            } => {
                let index = owner_index(&stack, *owner);
                claims.push(InterpretedClaim {
                    part: *part,
                    range: range.clone(),
                    owner: stack[index],
                    owner_depth: index,
                    generations_from_top: owner.generations_from_top(),
                });
            }
            DirectCommand::ResolveBlankGap { owner } => {
                let index = owner_index(&stack, *owner);
                gaps.push(InterpretedGapResolution {
                    owner: stack[index],
                    owner_depth: index,
                    generations_from_top: owner.generations_from_top(),
                });
            }
            DirectCommand::FinalizeParagraph {
                outcome: DirectParagraphOutcome::SetextHeading { level },
            } => {
                let top = stack
                    .last_mut()
                    .expect("Paragraph finalization has an open stack");
                assert_eq!(*top, DirectBlockKind::Paragraph);
                *top = DirectBlockKind::Heading(DirectHeadingFacts {
                    level: *level,
                    setext: true,
                });
            }
            DirectCommand::Close { kind, .. } => {
                assert_eq!(stack.pop(), Some(*kind), "close matches interpreted stack");
            }
            DirectCommand::StageTerminator { .. }
            | DirectCommand::ResolveTerminator { .. }
            | DirectCommand::StageBlankGap { .. }
            | DirectCommand::MarkFencedCodeBoundary { .. }
            | DirectCommand::FinishLine { .. }
            | DirectCommand::FinishDocument => {}
        }
    }
    (claims, gaps)
}

fn bullet_list_facts() -> DirectListFacts {
    DirectListFacts {
        list_type: ListType::Bullet,
        start: 1,
        delimiter: ListDelimiter::Period,
        bullet_char: b'-',
    }
}

fn bullet_item_facts() -> DirectItemFacts {
    DirectItemFacts {
        marker_offset: 0,
        padding: 2,
    }
}

fn list_final_tightness(commands: &[DirectCommand]) -> Vec<bool> {
    commands
        .iter()
        .filter_map(|command| match command {
            DirectCommand::Close {
                kind: DirectBlockKind::List(_),
                final_facts: DirectFinalFacts::List { tight },
                ..
            } => Some(*tight),
            _ => None,
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct LogicalMetric {
    bytes: u64,
    utf16: u64,
}

impl LogicalMetric {
    fn advanced(self, text: &str) -> Self {
        Self {
            bytes: self.bytes + u64::try_from(text.len()).expect("test text length fits u64"),
            utf16: self.utf16
                + u64::try_from(text.encode_utf16().count()).expect("test UTF-16 length fits u64"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InterpretedFence {
    open: DirectFencedCodeFacts,
    closed: bool,
    logical: String,
    info: std::ops::Range<LogicalMetric>,
    literal: std::ops::Range<LogicalMetric>,
}

#[derive(Debug)]
struct OpenFenceProjection {
    open: DirectFencedCodeFacts,
    logical: String,
    metric: LogicalMetric,
    info_end: Option<LogicalMetric>,
    literal_start: Option<LogicalMetric>,
}

#[allow(clippy::too_many_lines)]
fn interpreted_fences(source: &str, commands: &[DirectCommand]) -> Vec<InterpretedFence> {
    let source = SourceDocument::new(source);
    let mut line_index = 0_usize;
    let mut stack = Vec::<(DirectBlockKind, Option<usize>)>::new();
    let mut open = Vec::<OpenFenceProjection>::new();
    let mut finished = Vec::new();

    for command in commands {
        match command {
            DirectCommand::Open { kind } => {
                let projection = if let DirectBlockKind::FencedCode(facts) = kind {
                    let index = open.len();
                    open.push(OpenFenceProjection {
                        open: *facts,
                        logical: String::new(),
                        metric: LogicalMetric::default(),
                        info_end: None,
                        literal_start: None,
                    });
                    Some(index)
                } else {
                    None
                };
                stack.push((*kind, projection));
            }
            DirectCommand::Consume {
                owner,
                range,
                logical,
                ..
            } => {
                let physical_owner = owner_index(
                    &stack.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
                    *owner,
                );
                let logical_owner = match logical {
                    DirectLogicalAction::PartialTab(partial) => owner_index(
                        &stack.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
                        partial.logical_target(),
                    ),
                    DirectLogicalAction::Identity
                    | DirectLogicalAction::CanonicalText
                    | DirectLogicalAction::HiddenUpstream
                    | DirectLogicalAction::CanonicalNewline
                    | DirectLogicalAction::None => physical_owner,
                };
                let Some(fence) = stack[logical_owner].1 else {
                    continue;
                };
                let physical = source.leaves[line_index]
                    .text
                    .get(range.start as usize..range.end as usize)
                    .expect("logical claim is on scalar boundaries");
                let text = match logical {
                    DirectLogicalAction::Identity => physical.to_owned(),
                    DirectLogicalAction::CanonicalText => physical.replace('\0', "\u{fffd}"),
                    DirectLogicalAction::PartialTab(partial) => {
                        " ".repeat(usize::from(partial.remaining_spaces()))
                    }
                    DirectLogicalAction::HiddenUpstream | DirectLogicalAction::None => {
                        String::new()
                    }
                    DirectLogicalAction::CanonicalNewline => "\n".to_owned(),
                };
                open[fence].logical.push_str(&text);
                open[fence].metric = open[fence].metric.advanced(&text);
            }
            DirectCommand::MarkFencedCodeBoundary { boundary } => {
                let fence = stack
                    .last()
                    .and_then(|(_, fence)| *fence)
                    .expect("fenced boundary targets the open stack top");
                let metric = open[fence].metric;
                match boundary {
                    DirectFencedCodeBoundary::InfoEnd => {
                        assert!(open[fence].info_end.replace(metric).is_none());
                    }
                    DirectFencedCodeBoundary::LiteralStart => {
                        assert!(open[fence].info_end.is_some());
                        assert!(open[fence].literal_start.replace(metric).is_none());
                    }
                }
            }
            DirectCommand::FinalizeParagraph {
                outcome: DirectParagraphOutcome::SetextHeading { level },
            } => {
                let (kind, fence) = stack.last_mut().expect("Paragraph finalization has a top");
                assert_eq!(*kind, DirectBlockKind::Paragraph);
                assert!(fence.is_none());
                *kind = DirectBlockKind::Heading(DirectHeadingFacts {
                    level: *level,
                    setext: true,
                });
            }
            DirectCommand::Close {
                kind, final_facts, ..
            } => {
                let (open_kind, projection) = stack.pop().expect("close has an open block");
                assert_eq!(open_kind, *kind);
                if let Some(projection) = projection {
                    let state = &open[projection];
                    let DirectFinalFacts::FencedCode(DirectFencedCodeCloseFacts { closed }) =
                        final_facts
                    else {
                        panic!("FencedCode has typed close facts")
                    };
                    let info_end = state.info_end.expect("fence info boundary is present");
                    let literal_start = state
                        .literal_start
                        .expect("fence literal boundary is present");
                    finished.push(InterpretedFence {
                        open: state.open,
                        closed: *closed,
                        logical: state.logical.clone(),
                        info: LogicalMetric::default()..info_end,
                        literal: literal_start..state.metric,
                    });
                }
            }
            DirectCommand::FinishLine { .. } => line_index += 1,
            DirectCommand::StageTerminator { .. }
            | DirectCommand::ResolveTerminator { .. }
            | DirectCommand::StageBlankGap { .. }
            | DirectCommand::ResolveBlankGap { .. }
            | DirectCommand::FinishDocument => {}
        }
    }
    assert_eq!(line_index, source.leaves.len());
    finished
}

fn legacy_semantic_shape(source: &str) -> Vec<(DirectBlockKind, usize)> {
    fn visit(
        document: &BlockDocument,
        node: NodeId,
        depth: usize,
        output: &mut Vec<(DirectBlockKind, usize)>,
    ) {
        let kind = match document.tree.node(node).kind {
            BlockKind::Document => DirectBlockKind::Document,
            BlockKind::BlockQuote => DirectBlockKind::BlockQuote,
            BlockKind::List(list) => DirectBlockKind::List(DirectListFacts {
                list_type: list.list_type,
                start: u32::try_from(list.start).expect("list start fits direct facts"),
                delimiter: list.delimiter,
                bullet_char: list.bullet_char,
            }),
            BlockKind::Item(item) => DirectBlockKind::Item(DirectItemFacts {
                marker_offset: u16::try_from(item.marker_offset)
                    .expect("item marker offset fits direct facts"),
                padding: u16::try_from(item.padding).expect("item padding fits direct facts"),
            }),
            BlockKind::Paragraph => DirectBlockKind::Paragraph,
            BlockKind::Heading { level, setext, .. } => {
                DirectBlockKind::Heading(DirectHeadingFacts { level, setext })
            }
            BlockKind::CodeBlock {
                fenced: true,
                fence_char,
                fence_length,
                fence_offset,
                ..
            } => DirectBlockKind::FencedCode(DirectFencedCodeFacts {
                fence: match fence_char {
                    b'`' => DirectFenceCharacter::Backtick,
                    b'~' => DirectFenceCharacter::Tilde,
                    _ => panic!("legacy fence has a valid marker"),
                },
                minimum_closing_length: u64::try_from(fence_length)
                    .expect("fence length fits direct facts"),
                fence_offset_columns: u8::try_from(fence_offset)
                    .expect("fence offset fits direct facts"),
            }),
            ref other => panic!("supported corpus unexpectedly produced {other:?}"),
        };
        output.push((kind, depth));
        for child in &document.tree.node(node).children {
            visit(document, *child, depth + 1, output);
        }
    }

    let document =
        parse_document(source, SyntaxProfile::CommonMark).expect("legacy parser succeeds");
    let mut output = Vec::new();
    visit(&document, document.tree.root, 0, &mut output);
    output
}

fn direct_line_failure(line: String) -> ParseError {
    let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
    parser.acknowledge_command().unwrap();
    if let Err(error) = parser.begin_line(line) {
        return error;
    }
    for _ in 0..64 {
        match parser.poll_line(1) {
            Err(error) => return error,
            Ok(receipt) if receipt.status == DirectPollStatus::CommandReady => {
                parser.acknowledge_command().unwrap();
            }
            Ok(_) => {}
        }
    }
    panic!("unsupported line did not fail closed");
}

#[test]
fn crlf_multiline_paragraph_is_direct_bounded_and_stack_shaped() {
    let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
    let mut commands = Vec::new();
    begin(&mut parser, &mut commands);
    drive_line(&mut parser, "alpha\r\n", &mut commands);
    drive_line(&mut parser, "😀", &mut commands);
    finish(&mut parser, &mut commands);

    assert_eq!(
        commands,
        vec![
            DirectCommand::Open {
                kind: DirectBlockKind::Document,
            },
            DirectCommand::Open {
                kind: DirectBlockKind::Paragraph,
            },
            DirectCommand::Consume {
                owner: DirectOwner::TOP,
                part: DirectCoveragePart::Content,
                range: 0..5,
                logical: DirectLogicalAction::CanonicalText,
            },
            DirectCommand::StageTerminator {
                range: 5..7,
                ending: DirectLineEnding::CrLf,
            },
            DirectCommand::FinishLine {
                physical_bytes: 7,
                physical_utf16: 7,
            },
            DirectCommand::ResolveTerminator {
                resolution: DirectTerminatorResolution::ContinueCanonicalNewline,
            },
            DirectCommand::Consume {
                owner: DirectOwner::TOP,
                part: DirectCoveragePart::Content,
                range: 0..4,
                logical: DirectLogicalAction::CanonicalText,
            },
            DirectCommand::FinishLine {
                physical_bytes: 4,
                physical_utf16: 2,
            },
            DirectCommand::Close {
                kind: DirectBlockKind::Paragraph,
                final_facts: DirectFinalFacts::None,
                last_line_blank: false,
                child: DirectClosedChild::default(),
            },
            DirectCommand::Close {
                kind: DirectBlockKind::Document,
                final_facts: DirectFinalFacts::None,
                last_line_blank: false,
                child: DirectClosedChild::default(),
            },
            DirectCommand::FinishDocument,
        ]
    );
}

#[test]
fn empty_and_trailing_newline_have_no_phantom_physical_line() {
    let (empty_parser, empty_commands) = drive_source("");
    assert_eq!(
        empty_commands,
        [
            DirectCommand::Open {
                kind: DirectBlockKind::Document,
            },
            DirectCommand::Close {
                kind: DirectBlockKind::Document,
                final_facts: DirectFinalFacts::None,
                last_line_blank: false,
                child: DirectClosedChild::default(),
            },
            DirectCommand::FinishDocument,
        ]
    );
    assert_eq!(
        direct_semantic_shape(&empty_commands),
        legacy_semantic_shape("")
    );
    assert_eq!(empty_parser.retained_line_bytes(), 0);
    assert_eq!(empty_parser.retained_logical_bytes(), 0);
    assert_eq!(empty_parser.legacy_event_count(), 0);

    let source = "alpha\n";
    let (parser, commands) = drive_source(source);
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command, DirectCommand::FinishLine { .. }))
            .count(),
        1,
        "a trailing terminator belongs to the final real line"
    );
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command, DirectCommand::StageBlankGap { .. }))
            .count(),
        0,
        "a trailing newline does not synthesize a blank line"
    );
    assert_eq!(
        direct_semantic_shape(&commands),
        legacy_semantic_shape(source)
    );
    assert_eq!(parser.retained_line_bytes(), 0);
    assert_eq!(parser.retained_logical_bytes(), 0);
    assert_eq!(parser.legacy_event_count(), 0);
}

#[test]
fn supported_corpus_matches_legacy_semantic_block_shape() {
    for source in [
        "alpha",
        "alpha\r\nbeta",
        "  alpha\n   beta\n\nomega\n",
        "😀 one\rsecond\r\n\r\nlast",
        "first\n\n\nsecond\n",
    ] {
        let (parser, commands) = drive_source(source);
        assert_eq!(
            direct_semantic_shape(&commands),
            legacy_semantic_shape(source),
            "semantic block shape differs for {source:?}"
        );
        assert_eq!(parser.retained_line_bytes(), 0);
        assert_eq!(parser.retained_logical_bytes(), 0);
        assert_eq!(parser.legacy_event_count(), 0);
    }
}

#[test]
fn many_paragraphs_keep_direct_scratch_at_open_depth() {
    const PARAGRAPHS: usize = 128;
    let mut source = String::new();
    for index in 0..PARAGRAPHS {
        source.push_str(&format!("paragraph {index}\n\n"));
    }

    let (parser, commands) = drive_source(&source);
    assert!(parser.scratch_node_count() <= 1, "only the root remains");
    assert_eq!(parser.retained_line_bytes(), 0);
    assert_eq!(parser.retained_logical_bytes(), 0);
    assert_eq!(parser.legacy_event_count(), 0);
    assert_eq!(
        commands
            .iter()
            .filter(|command| {
                matches!(
                    command,
                    DirectCommand::Open {
                        kind: DirectBlockKind::Paragraph
                    }
                )
            })
            .count(),
        PARAGRAPHS
    );
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command, DirectCommand::FinishLine { .. }))
            .count(),
        PARAGRAPHS * 2
    );
    assert_eq!(
        direct_semantic_shape(&commands),
        legacy_semantic_shape(&source)
    );
}

#[test]
fn blank_closes_paragraph_owns_gap_then_reopens_paragraph() {
    let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
    let mut commands = Vec::new();
    begin(&mut parser, &mut commands);
    drive_line(&mut parser, "alpha\n", &mut commands);
    drive_line(&mut parser, "\n", &mut commands);
    drive_line(&mut parser, "beta", &mut commands);
    finish(&mut parser, &mut commands);

    let blank_close = commands
        .windows(4)
        .position(|window| {
            window
                == [
                    DirectCommand::ResolveTerminator {
                        resolution: DirectTerminatorResolution::CloseNone,
                    },
                    DirectCommand::Close {
                        kind: DirectBlockKind::Paragraph,
                        final_facts: DirectFinalFacts::None,
                        last_line_blank: true,
                        child: DirectClosedChild {
                            ends_blank: true,
                            ..DirectClosedChild::default()
                        },
                    },
                    DirectCommand::StageBlankGap { range: 0..1 },
                    DirectCommand::FinishLine {
                        physical_bytes: 1,
                        physical_utf16: 1,
                    },
                ]
        })
        .expect("blank closes the paragraph before staging its whole-line gap");
    assert_eq!(
        &commands[blank_close + 4..blank_close + 7],
        [
            DirectCommand::ResolveBlankGap {
                owner: DirectOwner::TOP,
            },
            DirectCommand::Open {
                kind: DirectBlockKind::Paragraph,
            },
            DirectCommand::Consume {
                owner: DirectOwner::TOP,
                part: DirectCoveragePart::Content,
                range: 0..4,
                logical: DirectLogicalAction::CanonicalText,
            },
        ]
    );
}

#[test]
fn quote_to_list_sibling_closes_old_branch_before_opening_new_branch() {
    let source = "> a\n- b\n";
    let (parser, commands) = drive_source(source);
    assert_eq!(
        direct_semantic_shape(&commands),
        legacy_semantic_shape(source)
    );

    let quote_close = commands
        .iter()
        .position(|command| {
            matches!(
                command,
                DirectCommand::Close {
                    kind: DirectBlockKind::BlockQuote,
                    ..
                }
            )
        })
        .expect("old quote closes");
    let list_open = commands
        .iter()
        .position(|command| {
            matches!(
                command,
                DirectCommand::Open {
                    kind: DirectBlockKind::List(_)
                }
            )
        })
        .expect("root list opens");
    assert!(
        quote_close < list_open,
        "normalized output must not open the root list under the old quote"
    );

    let (claims, _) = interpreted_source(&commands);
    let marker_owners = claims
        .iter()
        .filter(|claim| claim.part == DirectCoveragePart::ContainerMarker)
        .map(|claim| claim.owner)
        .collect::<Vec<_>>();
    assert!(matches!(marker_owners[0], DirectBlockKind::BlockQuote));
    assert!(matches!(marker_owners[1], DirectBlockKind::Item(_)));
    assert_eq!(parser.retained_logical_bytes(), 0);
    assert_eq!(parser.legacy_event_count(), 0);
}

#[test]
fn retained_quote_closes_old_paragraph_before_current_marker_and_nested_list() {
    let source = "> a\n> - b\n";
    let (parser, commands) = drive_source(source);
    assert_eq!(
        direct_semantic_shape(&commands),
        legacy_semantic_shape(source)
    );

    let paragraph_close = commands
        .iter()
        .position(|command| {
            matches!(
                command,
                DirectCommand::Close {
                    kind: DirectBlockKind::Paragraph,
                    ..
                }
            )
        })
        .expect("old paragraph closes");
    let second_quote_marker = commands
        .iter()
        .enumerate()
        .filter(|(_, command)| {
            matches!(
                command,
                DirectCommand::Consume {
                    part: DirectCoveragePart::ContainerMarker,
                    range,
                    ..
                } if *range == (0..2)
            )
        })
        .nth(1)
        .map(|(index, _)| index)
        .expect("second quote marker is claimed");
    let list_open = commands
        .iter()
        .position(|command| {
            matches!(
                command,
                DirectCommand::Open {
                    kind: DirectBlockKind::List(_)
                }
            )
        })
        .expect("nested list opens");
    assert!(paragraph_close < second_quote_marker);
    assert!(second_quote_marker < list_open);
    assert_eq!(parser.retained_logical_bytes(), 0);
    assert_eq!(parser.legacy_event_count(), 0);
}

#[test]
fn marked_blank_floor_resolves_before_its_quote_retires() {
    let source = "> a\n> \n- b\n";
    let (parser, commands) = drive_source(source);
    assert_eq!(
        direct_semantic_shape(&commands),
        legacy_semantic_shape(source)
    );
    assert!(commands.contains(&DirectCommand::StageBlankGap { range: 2..3 }));

    let resolution = commands
        .iter()
        .position(|command| matches!(command, DirectCommand::ResolveBlankGap { .. }))
        .expect("marked blank gap resolves");
    let quote_close = commands
        .iter()
        .position(|command| {
            matches!(
                command,
                DirectCommand::Close {
                    kind: DirectBlockKind::BlockQuote,
                    ..
                }
            )
        })
        .expect("quote retires");
    let root_list_open = commands
        .iter()
        .position(|command| {
            matches!(
                command,
                DirectCommand::Open {
                    kind: DirectBlockKind::List(_)
                }
            )
        })
        .expect("replacement root list opens");
    assert!(resolution < quote_close);
    assert!(quote_close < root_list_open);
    let (_, gaps) = interpreted_source(&commands);
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0].owner, DirectBlockKind::BlockQuote);
    assert_eq!(parser.retained_logical_bytes(), 0);
    assert_eq!(parser.legacy_event_count(), 0);
}

#[test]
fn parent_gap_precedes_new_paragraph_enter_and_deep_quotes_fit_recipe_bound() {
    let indented = "  alpha\n";
    let (_, commands) = drive_source(indented);
    let gap = commands
        .iter()
        .position(|command| {
            matches!(
                command,
                DirectCommand::Consume {
                    owner: DirectOwner::TOP,
                    part: DirectCoveragePart::Gap,
                    range,
                    logical: DirectLogicalAction::None,
                } if *range == (0..2)
            )
        })
        .expect("leading gap is document-owned");
    let paragraph = commands
        .iter()
        .position(|command| {
            matches!(
                command,
                DirectCommand::Open {
                    kind: DirectBlockKind::Paragraph
                }
            )
        })
        .expect("paragraph opens");
    assert!(
        gap < paragraph,
        "physical parent source precedes child Enter"
    );

    let deep = format!("{}x\n", "> ".repeat(64));
    let (parser, deep_commands) = drive_source(&deep);
    assert_eq!(
        direct_semantic_shape(&deep_commands),
        legacy_semantic_shape(&deep)
    );
    assert_eq!(
        deep_commands
            .iter()
            .filter(|command| {
                matches!(
                    command,
                    DirectCommand::Open {
                        kind: DirectBlockKind::BlockQuote
                    }
                )
            })
            .count(),
        64
    );
    assert_eq!(parser.retained_logical_bytes(), 0);
    assert_eq!(parser.legacy_event_count(), 0);
}

#[test]
fn list_item_siblings_keep_typed_facts_markers_and_crlf() {
    let source = "- a\r\n- b\n";
    let (parser, commands) = drive_source(source);
    assert_eq!(
        direct_semantic_shape(&commands),
        legacy_semantic_shape(source)
    );
    assert!(commands.contains(&DirectCommand::Open {
        kind: DirectBlockKind::List(bullet_list_facts()),
    }));
    assert_eq!(
        commands
            .iter()
            .filter(|command| {
                matches!(
                    command,
                    DirectCommand::Open {
                        kind: DirectBlockKind::Item(facts)
                    } if *facts == bullet_item_facts()
                )
            })
            .count(),
        2
    );
    assert!(commands.contains(&DirectCommand::StageTerminator {
        range: 3..5,
        ending: DirectLineEnding::CrLf,
    }));
    assert_eq!(list_final_tightness(&commands), [true]);

    let (claims, _) = interpreted_source(&commands);
    let item_markers = claims
        .iter()
        .filter(|claim| {
            claim.part == DirectCoveragePart::ContainerMarker
                && matches!(claim.owner, DirectBlockKind::Item(_))
        })
        .collect::<Vec<_>>();
    assert_eq!(item_markers.len(), 2);
    assert!(item_markers.iter().all(|claim| claim.range == (0..2)));
    assert!(item_markers.iter().all(|claim| claim.owner_depth == 2));

    let first_item_close = commands
        .iter()
        .position(|command| {
            matches!(
                command,
                DirectCommand::Close {
                    kind: DirectBlockKind::Item(_),
                    ..
                }
            )
        })
        .expect("first item closes");
    let second_item_open = commands
        .iter()
        .enumerate()
        .filter(|(_, command)| {
            matches!(
                command,
                DirectCommand::Open {
                    kind: DirectBlockKind::Item(_)
                }
            )
        })
        .nth(1)
        .map(|(index, _)| index)
        .expect("second item opens");
    assert!(first_item_close < second_item_open);
    assert_eq!(parser.retained_logical_bytes(), 0);
    assert_eq!(parser.legacy_event_count(), 0);
}

#[test]
fn quote_lazy_continuation_reuses_the_existing_paragraph() {
    let source = "> alpha\nlazy *b*";
    let (parser, commands) = drive_source(source);
    assert_eq!(
        direct_semantic_shape(&commands),
        legacy_semantic_shape(source)
    );
    assert_eq!(
        commands
            .iter()
            .filter(|command| {
                matches!(
                    command,
                    DirectCommand::Open {
                        kind: DirectBlockKind::Paragraph
                    }
                )
            })
            .count(),
        1
    );
    let (claims, _) = interpreted_source(&commands);
    assert_eq!(
        claims
            .iter()
            .filter(|claim| claim.part == DirectCoveragePart::ContainerMarker)
            .count(),
        1,
        "the markerless lazy line must not acquire inferred quote syntax"
    );
    assert!(claims.iter().any(|claim| {
        claim.part == DirectCoveragePart::Content
            && claim.range == (0..8)
            && claim.owner == DirectBlockKind::Paragraph
    }));
    assert!(commands.contains(&DirectCommand::ResolveTerminator {
        resolution: DirectTerminatorResolution::ContinueCanonicalNewline,
    }));
    assert_eq!(parser.retained_logical_bytes(), 0);
    assert_eq!(parser.legacy_event_count(), 0);
}

#[test]
fn nested_quote_list_continuation_selects_exact_old_owners() {
    let source = "> - a\r\n>   continuation\n";
    let (parser, commands) = drive_source(source);
    assert_eq!(
        direct_semantic_shape(&commands),
        legacy_semantic_shape(source)
    );
    assert!(commands.contains(&DirectCommand::StageTerminator {
        range: 5..7,
        ending: DirectLineEnding::CrLf,
    }));

    let (claims, _) = interpreted_source(&commands);
    let markers = claims
        .iter()
        .filter(|claim| claim.part == DirectCoveragePart::ContainerMarker)
        .collect::<Vec<_>>();
    assert_eq!(markers.len(), 4);
    for marker in [markers[0], markers[2]] {
        assert_eq!(marker.range, 0..2);
        assert_eq!(marker.owner, DirectBlockKind::BlockQuote);
        assert_eq!(marker.owner_depth, 1);
    }
    for marker in [markers[1], markers[3]] {
        assert_eq!(marker.range, 2..4);
        assert!(matches!(marker.owner, DirectBlockKind::Item(_)));
        assert_eq!(marker.owner_depth, 3);
    }
    assert!(
        markers[2].generations_from_top > markers[0].generations_from_top,
        "the continued paragraph keeps the old quote farther below the stack top"
    );
    assert_eq!(parser.retained_logical_bytes(), 0);
    assert_eq!(parser.legacy_event_count(), 0);
}

#[test]
fn nested_marked_blank_resolves_to_deepest_surviving_quote_including_eof() {
    for source in ["> > a\n> > \n> > b", "> > a\n> > \n"] {
        let (parser, commands) = drive_source(source);
        assert_eq!(
            direct_semantic_shape(&commands),
            legacy_semantic_shape(source),
            "semantic shape differs for {source:?}"
        );
        assert!(commands.contains(&DirectCommand::StageBlankGap { range: 4..5 }));
        let (_, gaps) = interpreted_source(&commands);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].owner, DirectBlockKind::BlockQuote);
        assert_eq!(
            gaps[0].owner_depth, 2,
            "the marked suffix belongs to the inner surviving quote"
        );
        assert_eq!(parser.retained_logical_bytes(), 0);
        assert_eq!(parser.legacy_event_count(), 0);
    }
}

#[test]
fn list_close_facts_distinguish_tight_and_loose_sibling_items() {
    for (source, expected_tight) in [("- a\n- b\n", true), ("- a\n\n- b\n", false)] {
        let (parser, commands) = drive_source(source);
        assert_eq!(
            direct_semantic_shape(&commands),
            legacy_semantic_shape(source)
        );
        assert_eq!(list_final_tightness(&commands), [expected_tight]);
        if !expected_tight {
            let (_, gaps) = interpreted_source(&commands);
            assert_eq!(gaps.len(), 1);
            assert!(matches!(gaps[0].owner, DirectBlockKind::List(_)));
            assert_eq!(gaps[0].owner_depth, 1);
        }
        assert_eq!(parser.retained_logical_bytes(), 0);
        assert_eq!(parser.legacy_event_count(), 0);
    }
}

#[test]
fn close_preserves_intrinsic_last_blank_when_derived_child_summary_is_ambiguous() {
    let (_, directly_blank) = drive_source("- a\n\noutside\n");
    let (_, descendant_blank) = drive_source("- - a\n\noutside\n");
    let ambiguous = DirectClosedChild {
        ends_blank: true,
        item_loose_if_nonlast: true,
        item_loose_if_last: false,
    };

    let directly_blank_item = directly_blank.iter().find_map(|command| match command {
        DirectCommand::Close {
            kind: DirectBlockKind::Item(_),
            last_line_blank,
            child,
            ..
        } if *child == ambiguous && *last_line_blank => Some((*last_line_blank, *child)),
        _ => None,
    });
    let descendant_blank_item = descendant_blank.iter().find_map(|command| match command {
        DirectCommand::Close {
            kind: DirectBlockKind::Item(_),
            last_line_blank,
            child,
            ..
        } if *child == ambiguous && !*last_line_blank => Some((*last_line_blank, *child)),
        _ => None,
    });

    assert_eq!(directly_blank_item, Some((true, ambiguous)));
    assert_eq!(descendant_blank_item, Some((false, ambiguous)));
    assert_eq!(
        directly_blank_item.map(|(_, child)| child),
        descendant_blank_item.map(|(_, child)| child),
        "the derived close summary alone cannot recover intrinsic last-line blankness"
    );
}

#[test]
fn setext_is_one_typed_active_paragraph_transaction_with_exact_marker_ownership() {
    for (source, level, marker, terminal) in [
        ("alpha\n===\n", 1, 0..3, 3..4),
        ("alpha\r\n---\r\n", 2, 0..3, 3..5),
        ("> alpha\n> ===\n", 1, 2..5, 5..6),
    ] {
        let (parser, commands) = drive_source(source);
        assert_eq!(
            direct_semantic_shape(&commands),
            legacy_semantic_shape(source),
            "semantic shape differs for {source:?}"
        );
        let finalize = commands
            .iter()
            .position(|command| {
                matches!(
                    command,
                    DirectCommand::FinalizeParagraph {
                        outcome: DirectParagraphOutcome::SetextHeading {
                            level: actual
                        }
                    } if *actual == level
                )
            })
            .expect("Setext has one typed finalization");
        let resolve = commands
            .iter()
            .position(|command| {
                matches!(
                    command,
                    DirectCommand::ResolveTerminator {
                        resolution: DirectTerminatorResolution::CloseNone
                    }
                )
            })
            .expect("the preceding Paragraph terminator is hidden");
        assert!(resolve < finalize);

        let (claims, _) = interpreted_source(&commands);
        let marker_claim = claims
            .iter()
            .find(|claim| claim.part == DirectCoveragePart::BlockMarker)
            .expect("underline marker is claimed");
        assert_eq!(marker_claim.range, marker);
        assert_eq!(
            marker_claim.owner,
            DirectBlockKind::Heading(DirectHeadingFacts {
                level,
                setext: true,
            })
        );
        let terminal_claim = claims
            .iter()
            .find(|claim| claim.part == DirectCoveragePart::Terminal)
            .expect("underline line ending is claimed");
        assert_eq!(terminal_claim.range, terminal);
        assert_eq!(terminal_claim.owner, marker_claim.owner);
        if source.starts_with('>') {
            let quote_markers = claims
                .iter()
                .filter(|claim| {
                    claim.part == DirectCoveragePart::ContainerMarker
                        && claim.owner == DirectBlockKind::BlockQuote
                })
                .collect::<Vec<_>>();
            assert_eq!(quote_markers.len(), 2);
        }
        assert_eq!(parser.retained_logical_bytes(), 0);
        assert_eq!(parser.legacy_event_count(), 0);
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn fenced_code_streams_exact_source_and_writer_derived_projection_cuts() {
    let source = "  ```` lang😀\r\n body\r\n   ```\r\n  ````  \r\n";
    let (parser, commands) = drive_source(source);
    assert_eq!(
        direct_semantic_shape(&commands),
        legacy_semantic_shape(source)
    );

    let facts = DirectFencedCodeFacts {
        fence: DirectFenceCharacter::Backtick,
        minimum_closing_length: 4,
        fence_offset_columns: 2,
    };
    assert!(commands.contains(&DirectCommand::Open {
        kind: DirectBlockKind::FencedCode(facts),
    }));
    let source_actions = commands
        .iter()
        .filter_map(|command| match command {
            DirectCommand::Consume {
                part,
                range,
                logical,
                ..
            } => Some((*part, range.clone(), *logical)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        source_actions,
        [
            (
                DirectCoveragePart::BlockMarker,
                0..6,
                DirectLogicalAction::None,
            ),
            (
                DirectCoveragePart::Content,
                6..15,
                DirectLogicalAction::CanonicalText,
            ),
            (
                DirectCoveragePart::Content,
                15..17,
                DirectLogicalAction::CanonicalNewline,
            ),
            (
                DirectCoveragePart::ContainerMarker,
                0..1,
                DirectLogicalAction::None,
            ),
            (
                DirectCoveragePart::Content,
                1..5,
                DirectLogicalAction::CanonicalText,
            ),
            (
                DirectCoveragePart::Content,
                5..7,
                DirectLogicalAction::CanonicalNewline,
            ),
            (
                DirectCoveragePart::ContainerMarker,
                0..2,
                DirectLogicalAction::None,
            ),
            (
                DirectCoveragePart::Content,
                2..6,
                DirectLogicalAction::CanonicalText,
            ),
            (
                DirectCoveragePart::Content,
                6..8,
                DirectLogicalAction::CanonicalNewline,
            ),
            (
                DirectCoveragePart::BlockMarker,
                0..8,
                DirectLogicalAction::None,
            ),
            (
                DirectCoveragePart::Terminal,
                8..10,
                DirectLogicalAction::None,
            ),
        ]
    );

    let fences = interpreted_fences(source, &commands);
    assert_eq!(fences.len(), 1);
    let expected_logical = " lang😀\nbody\n ```\n";
    let info = " lang😀";
    let literal_start = " lang😀\n";
    assert_eq!(
        fences[0],
        InterpretedFence {
            open: facts,
            closed: true,
            logical: expected_logical.to_owned(),
            info: LogicalMetric::default()..LogicalMetric::default().advanced(info),
            literal: LogicalMetric::default().advanced(literal_start)
                ..LogicalMetric::default().advanced(expected_logical),
        }
    );
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command, DirectCommand::MarkFencedCodeBoundary { .. }))
            .count(),
        2
    );
    assert_eq!(parser.retained_logical_bytes(), 0);
    assert_eq!(parser.legacy_event_count(), 0);
}

#[test]
fn fenced_code_handles_nested_closed_and_long_run_bare_eof_cases() {
    let quoted = "> ```x\n> body\n> ```\n";
    let (quoted_parser, quoted_commands) = drive_source(quoted);
    assert_eq!(
        direct_semantic_shape(&quoted_commands),
        legacy_semantic_shape(quoted)
    );
    let (claims, _) = interpreted_source(&quoted_commands);
    assert_eq!(
        claims
            .iter()
            .filter(|claim| {
                claim.part == DirectCoveragePart::ContainerMarker
                    && claim.owner == DirectBlockKind::BlockQuote
            })
            .count(),
        3
    );
    let quoted_fence = interpreted_fences(quoted, &quoted_commands)
        .pop()
        .expect("quoted fence is present");
    assert!(quoted_fence.closed);
    assert_eq!(quoted_fence.logical, "x\nbody\n");
    assert_eq!(quoted_parser.retained_logical_bytes(), 0);

    let run = "~".repeat(300);
    let bare = format!("- {run}lang\n  payload");
    let (bare_parser, bare_commands) = drive_source(&bare);
    assert_eq!(
        direct_semantic_shape(&bare_commands),
        legacy_semantic_shape(&bare)
    );
    let bare_fence = interpreted_fences(&bare, &bare_commands)
        .pop()
        .expect("bare EOF fence is present");
    assert_eq!(
        bare_fence.open,
        DirectFencedCodeFacts {
            fence: DirectFenceCharacter::Tilde,
            minimum_closing_length: 300,
            fence_offset_columns: 0,
        }
    );
    assert!(!bare_fence.closed);
    assert_eq!(bare_fence.logical, "lang\npayload");
    assert_eq!(
        bare_fence.info,
        LogicalMetric::default()..LogicalMetric::default().advanced("lang")
    );
    assert_eq!(
        bare_fence.literal,
        LogicalMetric::default().advanced("lang\n")
            ..LogicalMetric::default().advanced("lang\npayload")
    );
    assert_eq!(bare_parser.retained_logical_bytes(), 0);
    assert_eq!(bare_parser.legacy_event_count(), 0);
}

#[test]
fn fenced_code_canonicalizes_all_physical_line_endings_without_staging() {
    for ending in ["\n", "\r", "\r\n"] {
        let source = format!("```rust{ending}x{ending}```{ending}");
        let (parser, commands) = drive_source(&source);
        assert_eq!(
            direct_semantic_shape(&commands),
            legacy_semantic_shape(&source),
            "semantic shape differs for ending {ending:?}"
        );
        let fence = interpreted_fences(&source, &commands)
            .pop()
            .expect("fence is present");
        assert_eq!(fence.logical, "rust\nx\n");
        assert!(fence.closed);
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(
                    command,
                    DirectCommand::Consume {
                        part: DirectCoveragePart::Content,
                        logical: DirectLogicalAction::CanonicalNewline,
                        ..
                    }
                ))
                .count(),
            2
        );
        assert!(commands.iter().any(|command| matches!(
            command,
            DirectCommand::Consume {
                part: DirectCoveragePart::Terminal,
                logical: DirectLogicalAction::None,
                range,
                ..
            } if range.end - range.start
                == u32::try_from(ending.len()).expect("line ending length fits u32")
        )));
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, DirectCommand::StageTerminator { .. }))
        );
        assert_eq!(parser.retained_logical_bytes(), 0);
        assert_eq!(parser.legacy_event_count(), 0);
    }
}

#[test]
fn initial_bom_is_document_gap_before_a_fenced_code_transaction() {
    let source = "\u{feff}```lang\nbody\n```\n";
    let (parser, commands) = drive_source(source);
    assert_eq!(
        direct_semantic_shape(&commands),
        legacy_semantic_shape(source)
    );
    let gap = commands
        .iter()
        .position(|command| {
            matches!(
                command,
                DirectCommand::Consume {
                    owner: DirectOwner::TOP,
                    part: DirectCoveragePart::Gap,
                    range,
                    logical: DirectLogicalAction::None,
                } if *range == (0..3)
            )
        })
        .expect("the ignored BOM remains exactly covered by Document");
    let fence = commands
        .iter()
        .position(|command| {
            matches!(
                command,
                DirectCommand::Open {
                    kind: DirectBlockKind::FencedCode(_),
                }
            )
        })
        .expect("the fence opens after the BOM");
    assert!(gap < fence);
    assert_eq!(parser.retained_logical_bytes(), 0);
    assert_eq!(parser.legacy_event_count(), 0);
}

#[test]
fn unsupported_aggregate_and_unimplemented_block_cases_fail_closed() {
    assert_eq!(
        DirectValueBlockParser::new(SyntaxProfile::Gfm).err(),
        Some(ParseError::DirectUnsupported(
            DirectUnsupported::SyntaxProfile
        ))
    );

    for (label, line) in [("indented code", "    code\n")] {
        assert_eq!(
            direct_line_failure(line.to_owned()),
            ParseError::DirectUnsupported(DirectUnsupported::BlockKind),
            "{label} must fail instead of silently degrading to a paragraph"
        );
    }
    assert_eq!(
        direct_line_failure("\tindented\n".to_owned()),
        ParseError::DirectUnsupported(DirectUnsupported::BlockKind)
    );
    assert_eq!(
        direct_line_failure("x".repeat(8 * 1024 + 1)),
        ParseError::DirectUnsupported(DirectUnsupported::LineTooLarge)
    );
}

#[test]
fn canonical_text_keeps_one_command_while_typed_atoms_remain_writer_work() {
    let paragraph = "a\tb\0😀\n";
    let (_, paragraph_commands) = drive_source(paragraph);
    assert!(paragraph_commands.iter().any(|command| {
        matches!(
            command,
            DirectCommand::Consume {
                owner: DirectOwner::TOP,
                part: DirectCoveragePart::Content,
                range,
                logical: DirectLogicalAction::CanonicalText,
            } if *range == (0..u32::try_from(paragraph.len() - 1).unwrap())
        )
    }));
    assert!(!paragraph_commands.iter().any(|command| {
        matches!(
            command,
            DirectCommand::Consume {
                logical: DirectLogicalAction::Identity,
                ..
            }
        )
    }));

    let atx = "# a\tb\0😀\n";
    let (_, atx_commands) = drive_source(atx);
    assert!(atx_commands.iter().any(|command| {
        matches!(
            command,
            DirectCommand::Consume {
                owner: DirectOwner::TOP,
                part: DirectCoveragePart::Content,
                range,
                logical: DirectLogicalAction::CanonicalText,
            } if *range == (2..u32::try_from(atx.len() - 1).unwrap())
        )
    }));
    assert_eq!(
        direct_semantic_shape(&atx_commands),
        legacy_semantic_shape(atx)
    );

    let fence_source = "```x\t\0\n\tbody\0😀\n```\n";
    let (_, fence_commands) = drive_source(fence_source);
    let fences = interpreted_fences(fence_source, &fence_commands);
    assert_eq!(fences.len(), 1);
    assert_eq!(fences[0].logical, "x\t�\n\tbody�😀\n");
}

#[test]
fn partial_tab_is_checked_and_can_cross_physical_owner_to_descendant_target() {
    let source = ">  ```\n>\tbody\n";
    let (_, commands) = drive_source(source);
    let partial = commands
        .iter()
        .find_map(|command| match command {
            DirectCommand::Consume {
                owner,
                part,
                range,
                logical: DirectLogicalAction::PartialTab(partial),
            } => Some((*owner, *part, range.clone(), *partial)),
            _ => None,
        })
        .expect("fenced body retains its partially consumed quote tab");
    assert_eq!(partial.0, DirectOwner::PARENT_OF_TOP);
    assert_eq!(partial.1, DirectCoveragePart::ContainerMarker);
    assert_eq!(partial.2, 1..2);
    assert_eq!(partial.3.logical_target(), DirectOwner::TOP);
    assert_eq!(partial.3.remaining_spaces(), 1);

    let fences = interpreted_fences(source, &commands);
    assert_eq!(fences.len(), 1);
    assert_eq!(
        fences[0].literal,
        LogicalMetric { bytes: 1, utf16: 1 }..LogicalMetric { bytes: 7, utf16: 7 }
    );
    assert_eq!(fences[0].logical, "\n body\n");
}

#[test]
fn fenced_deindent_retains_each_possible_partial_tab_width() {
    for (source, expected) in [
        (">  ```\n>\tbody\n", 1),
        ("  ```\n\tbody\n", 2),
        (" ```\n\tbody\n", 3),
    ] {
        let (_, commands) = drive_source(source);
        let partial = commands
            .iter()
            .find_map(|command| match command {
                DirectCommand::Consume {
                    logical: DirectLogicalAction::PartialTab(partial),
                    ..
                } => Some(*partial),
                _ => None,
            })
            .expect("fenced deindent emits one typed partial tab");
        assert_eq!(partial.remaining_spaces(), expected, "source {source:?}");
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(
                    command,
                    DirectCommand::Consume {
                        logical: DirectLogicalAction::PartialTab(_),
                        ..
                    }
                ))
                .count(),
            1,
            "source {source:?}"
        );
    }
}

#[test]
fn atx_heading_commands_own_exact_marker_content_hidden_tail_and_eol_ranges() {
    let cases = [
        (
            "### alpha ###   \r\n",
            DirectHeadingFacts {
                level: 3,
                setext: false,
            },
            vec![
                (
                    DirectCoveragePart::BlockMarker,
                    0..4,
                    DirectLogicalAction::None,
                ),
                (
                    DirectCoveragePart::Content,
                    4..9,
                    DirectLogicalAction::CanonicalText,
                ),
                (
                    DirectCoveragePart::BlockMarker,
                    9..16,
                    DirectLogicalAction::None,
                ),
                (
                    DirectCoveragePart::Terminal,
                    16..18,
                    DirectLogicalAction::None,
                ),
            ],
        ),
        (
            "# alpha#   \n",
            DirectHeadingFacts {
                level: 1,
                setext: false,
            },
            vec![
                (
                    DirectCoveragePart::BlockMarker,
                    0..2,
                    DirectLogicalAction::None,
                ),
                (
                    DirectCoveragePart::Content,
                    2..8,
                    DirectLogicalAction::CanonicalText,
                ),
                (
                    DirectCoveragePart::Content,
                    8..11,
                    DirectLogicalAction::HiddenUpstream,
                ),
                (
                    DirectCoveragePart::Terminal,
                    11..12,
                    DirectLogicalAction::None,
                ),
            ],
        ),
        (
            "# alpha\n",
            DirectHeadingFacts {
                level: 1,
                setext: false,
            },
            vec![
                (
                    DirectCoveragePart::BlockMarker,
                    0..2,
                    DirectLogicalAction::None,
                ),
                (
                    DirectCoveragePart::Content,
                    2..7,
                    DirectLogicalAction::CanonicalText,
                ),
                (
                    DirectCoveragePart::Terminal,
                    7..8,
                    DirectLogicalAction::None,
                ),
            ],
        ),
        (
            "# alpha\r",
            DirectHeadingFacts {
                level: 1,
                setext: false,
            },
            vec![
                (
                    DirectCoveragePart::BlockMarker,
                    0..2,
                    DirectLogicalAction::None,
                ),
                (
                    DirectCoveragePart::Content,
                    2..7,
                    DirectLogicalAction::CanonicalText,
                ),
                (
                    DirectCoveragePart::Terminal,
                    7..8,
                    DirectLogicalAction::None,
                ),
            ],
        ),
        (
            "## alpha",
            DirectHeadingFacts {
                level: 2,
                setext: false,
            },
            vec![
                (
                    DirectCoveragePart::BlockMarker,
                    0..3,
                    DirectLogicalAction::None,
                ),
                (
                    DirectCoveragePart::Content,
                    3..8,
                    DirectLogicalAction::CanonicalText,
                ),
            ],
        ),
        (
            "#   \n",
            DirectHeadingFacts {
                level: 1,
                setext: false,
            },
            vec![
                (
                    DirectCoveragePart::BlockMarker,
                    0..4,
                    DirectLogicalAction::None,
                ),
                (
                    DirectCoveragePart::Terminal,
                    4..5,
                    DirectLogicalAction::None,
                ),
            ],
        ),
    ];

    for (source, facts, expected) in cases {
        let (parser, commands) = drive_source(source);
        assert_eq!(
            direct_semantic_shape(&commands),
            legacy_semantic_shape(source)
        );
        assert!(commands.contains(&DirectCommand::Open {
            kind: DirectBlockKind::Heading(facts),
        }));
        let claims = commands
            .iter()
            .filter_map(|command| match command {
                DirectCommand::Consume {
                    owner: DirectOwner::TOP,
                    part,
                    range,
                    logical,
                } => Some((*part, range.clone(), *logical)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(claims, expected, "wrong ATX partition for {source:?}");
        assert_eq!(parser.retained_logical_bytes(), 0);
        assert_eq!(parser.legacy_event_count(), 0);
    }
}

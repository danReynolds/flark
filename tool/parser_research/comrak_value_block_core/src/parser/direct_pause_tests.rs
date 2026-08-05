use super::*;

#[test]
fn partial_tab_construction_is_donor_private_and_checked() {
    for invalid in [0, 4, u8::MAX] {
        assert_eq!(DirectPartialTab::new(DirectOwner::TOP, invalid), None);
    }
    for remaining in 1..=3 {
        let partial = DirectPartialTab::new(DirectOwner::PARENT_OF_TOP, remaining)
            .expect("one through three spaces are exact partial tabs");
        assert_eq!(partial.logical_target(), DirectOwner::PARENT_OF_TOP);
        assert_eq!(partial.remaining_spaces(), remaining);
    }
}

fn acknowledge_pending(parser: &mut DirectValueBlockParser, commands: &mut Vec<DirectCommand>) {
    let command = parser
        .pending_command()
        .expect("a direct command is ready")
        .clone();
    commands.push(command);
    parser
        .acknowledge_command()
        .expect("direct command acknowledgement succeeds");
}

fn started() -> (DirectValueBlockParser, Vec<DirectCommand>) {
    let mut parser =
        DirectValueBlockParser::new(SyntaxProfile::CommonMark).expect("direct parser starts");
    let mut commands = Vec::new();
    acknowledge_pending(&mut parser, &mut commands);
    (parser, commands)
}

fn started_with_legacy_open_new_oracle() -> (DirectValueBlockParser, Vec<DirectCommand>) {
    let (mut parser, commands) = started();
    parser.parser.open_new_scheduler = OpenNewScheduler::LegacyAtomic;
    (parser, commands)
}

fn drive_line(parser: &mut DirectValueBlockParser, line: &str, commands: &mut Vec<DirectCommand>) {
    drive_line_with_fuel(parser, line, commands, 1);
}

fn drive_line_with_fuel(
    parser: &mut DirectValueBlockParser,
    line: &str,
    commands: &mut Vec<DirectCommand>,
    fuel: usize,
) {
    try_drive_line_with_fuel(parser, line, commands, fuel).expect("direct line poll succeeds");
}

fn try_drive_line_with_fuel(
    parser: &mut DirectValueBlockParser,
    line: &str,
    commands: &mut Vec<DirectCommand>,
    fuel: usize,
) -> Result<(), ParseError> {
    assert!(fuel > 0, "test driver needs positive fuel");
    parser.begin_line(line.to_owned())?;
    let limit = line.len().saturating_mul(8).saturating_add(256);
    for _ in 0..limit {
        let receipt = parser.poll_line(fuel)?;
        assert!(receipt.transitions <= fuel, "direct poll overran fuel");
        match receipt.status {
            DirectPollStatus::CommandReady => acknowledge_pending(parser, commands),
            DirectPollStatus::Pending => {}
            DirectPollStatus::ExternalWorkReady => {
                return Err(ParseError::DirectExternalWork(
                    parser
                        .pending_external_work()
                        .expect("external status has work")
                        .request(),
                ));
            }
            DirectPollStatus::Complete => return Ok(()),
        }
    }
    Err(ParseError::Invariant(
        "test direct line converges within its poll bound",
    ))
}

fn finish(parser: &mut DirectValueBlockParser, commands: &mut Vec<DirectCommand>) {
    finish_with_fuel(parser, commands, 1);
}

fn finish_with_fuel(
    parser: &mut DirectValueBlockParser,
    commands: &mut Vec<DirectCommand>,
    fuel: usize,
) {
    assert!(fuel > 0, "test driver needs positive fuel");
    parser.begin_finish().expect("direct finish begins");
    for _ in 0..100_000 {
        let receipt = parser
            .poll_finish(fuel)
            .expect("direct finish poll succeeds");
        assert!(receipt.transitions <= fuel, "direct finish overran fuel");
        match receipt.status {
            DirectPollStatus::CommandReady => acknowledge_pending(parser, commands),
            DirectPollStatus::Pending => {}
            DirectPollStatus::ExternalWorkReady => {
                panic!("finish cannot request reference-prefix work")
            }
            DirectPollStatus::Complete => return,
        }
    }
    panic!("direct finish did not converge");
}

fn parser_after(lines: &[&str]) -> DirectValueBlockParser {
    let (mut parser, mut ignored) = started();
    for line in lines {
        drive_line(&mut parser, line, &mut ignored);
    }
    parser
}

fn try_restart_parts_after(
    lines: &[&str],
) -> Option<(
    DirectGrammarContinuation,
    DirectRestartOutput,
    DirectLineBoundaryResumeCursor,
)> {
    let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).ok()?;
    parser.acknowledge_command().ok()?;
    for line in lines {
        parser.begin_line((*line).to_owned()).ok()?;
        let limit = line.len().saturating_mul(8).saturating_add(256);
        let mut complete = false;
        for _ in 0..limit {
            match parser.poll_line(1).ok()?.status {
                DirectPollStatus::CommandReady => parser.acknowledge_command().ok()?,
                DirectPollStatus::Pending => {}
                DirectPollStatus::ExternalWorkReady => return None,
                DirectPollStatus::Complete => {
                    complete = true;
                    break;
                }
            }
        }
        if !complete {
            return None;
        }
    }
    let pause = parser.capture_line_boundary_pause().ok()?;
    let cursor = resume_cursor(&pause);
    let (grammar, output) = pause.into_restart_parts().ok()?;
    Some((grammar, output, cursor))
}

fn try_feed_restart_line(
    grammar: &DirectGrammarContinuation,
    output: DirectRestartOutput,
    cursor: DirectLineBoundaryResumeCursor,
    line: &str,
) -> Result<(DirectGrammarContinuation, DirectRestartOutput), ParseError> {
    let mut parser = DirectValueBlockParser::resume_restart_parts(grammar, output, cursor)?;
    parser.begin_line(line.to_owned())?;
    let limit = line.len().saturating_mul(8).saturating_add(256);
    for _ in 0..limit {
        match parser.poll_line(1)?.status {
            DirectPollStatus::CommandReady => parser.acknowledge_command()?,
            DirectPollStatus::Pending => {}
            DirectPollStatus::ExternalWorkReady => {
                return Err(ParseError::DirectExternalWork(
                    parser
                        .pending_external_work()
                        .expect("external status has work")
                        .request(),
                ));
            }
            DirectPollStatus::Complete => return parser.capture_restart_parts(),
        }
    }
    Err(ParseError::Invariant(
        "test identical-line transition completes within its poll bound",
    ))
}

fn pause_after(lines: &[&str]) -> DirectLineBoundaryPause {
    parser_after(lines)
        .capture_line_boundary_pause()
        .expect("acknowledged line boundary pauses")
}

fn resume_cursor(pause: &DirectLineBoundaryPause) -> DirectLineBoundaryResumeCursor {
    DirectLineBoundaryResumeCursor::new(
        u64::try_from(pause.cursor.line_number).expect("test line number fits u64"),
        u64::try_from(pause.cursor.last_line_length).expect("test line length fits u64"),
    )
    .expect("captured direct cursor is valid")
}

fn durable_capture_after(
    lines: &[&str],
) -> (
    DirectDurableLineBoundaryHeader,
    Vec<DirectDurableLineBoundaryFrameRecord>,
    DirectLineBoundaryPause,
) {
    let parser = parser_after(lines);
    let pause = parser
        .capture_line_boundary_pause()
        .expect("acknowledged prefix pauses");
    let capture = parser
        .capture_durable_line_boundary_checkpoint()
        .expect("acknowledged prefix has a durable donor capture");
    (capture.header(), capture.frame_records().collect(), pause)
}

fn durable_grammar_capture_after(
    lines: &[&str],
) -> (
    DirectDurableGrammarHeader,
    Vec<DirectDurableGrammarFrameRecord>,
) {
    let capture = parser_after(lines)
        .capture_durable_grammar_line_boundary_checkpoint()
        .expect("acknowledged prefix has a durable grammar capture");
    (capture.header(), capture.frame_records().collect())
}

fn durable_grammar_capture(
    parser: &DirectValueBlockParser,
) -> (
    DirectDurableGrammarHeader,
    Vec<DirectDurableGrammarFrameRecord>,
) {
    let capture = parser
        .capture_durable_grammar_line_boundary_checkpoint()
        .expect("acknowledged parser has a durable grammar capture");
    (capture.header(), capture.frame_records().collect())
}

fn assert_durable_restart_decode_rejects(
    header: DirectDurableLineBoundaryHeader,
    records: Vec<DirectDurableLineBoundaryFrameRecord>,
) {
    assert!(DirectValueBlockParser::decode_durable_restart_parts(header, records).is_err());
}

fn restart_parts_after(
    lines: &[&str],
) -> (
    DirectGrammarContinuation,
    DirectRestartOutput,
    DirectLineBoundaryResumeCursor,
) {
    let pause = pause_after(lines);
    let cursor = resume_cursor(&pause);
    let (grammar, output) = pause
        .into_restart_parts()
        .expect("direct pause splits into restart parts");
    (grammar, output, cursor)
}

fn grammar_projected_from_output(output: &DirectRestartOutput) -> DirectGrammarContinuation {
    project_direct_grammar_continuation(
        output.schema,
        output.profile,
        output.current_frame,
        &output.frames,
        output.deferred,
        output.paragraph,
    )
    .expect("test output projects to grammar")
}

fn restart_frame_outputs(output: &DirectRestartOutput) -> Vec<DirectRestartFrameOutput> {
    output
        .frames
        .iter()
        .map(|frame| DirectRestartFrameOutput {
            kind: frame.kind,
            closed_children: frame.closed_children,
        })
        .collect()
}

fn mutate_compatible_output_accumulators(output: &DirectRestartOutput) -> DirectRestartOutput {
    let mut changed = output.clone();
    for frame in &mut changed.frames {
        frame.closed_children.list_loose_before_last =
            !frame.closed_children.list_loose_before_last;
        match &mut frame.kind {
            DirectBlockKind::List(facts) if facts.list_type == ListType::Ordered => {
                facts.start = if facts.start == 7 { 11 } else { 7 };
            }
            DirectBlockKind::Item(facts) if facts.marker_offset < 3 && facts.padding > 2 => {
                facts.marker_offset += 1;
                facts.padding -= 1;
            }
            DirectBlockKind::Heading(facts) => {
                facts.level = if facts.level == 1 { 2 } else { 1 };
            }
            _ => {}
        }
    }
    changed
}

fn assert_compatible_output_reconstructs_exactly(
    grammar: &DirectGrammarContinuation,
    output: DirectRestartOutput,
    cursor: DirectLineBoundaryResumeCursor,
) {
    let projected = grammar_projected_from_output(&output);
    assert!(grammar.is_future_grammar_compatible(&projected));
    let expected = output.clone();
    let resumed = DirectValueBlockParser::resume_restart_parts(grammar, output, cursor)
        .expect("compatible current output reconstructs");
    let (recaptured_grammar, recaptured_output) = resumed
        .capture_restart_parts()
        .expect("reconstructed parser recaptures restart parts");
    assert_eq!(recaptured_grammar, *grammar);
    assert_eq!(recaptured_output, expected);
}

fn assert_in_memory_split_reconstructs_exact(prefix: &[&str], suffix: &[&str]) {
    let mut uninterrupted = parser_after(prefix);
    let pause = uninterrupted
        .capture_line_boundary_pause()
        .expect("current prefix pauses");
    let cursor = resume_cursor(&pause);
    let (grammar, output) = pause.into_restart_parts().expect("current pause splits");
    let mut resumed = DirectValueBlockParser::resume_restart_parts(&grammar, output, cursor)
        .expect("split restart reconstructs");

    let mut expected = Vec::new();
    let mut actual = Vec::new();
    for line in suffix {
        drive_line(&mut uninterrupted, line, &mut expected);
        drive_line(&mut resumed, line, &mut actual);
    }
    finish(&mut uninterrupted, &mut expected);
    finish(&mut resumed, &mut actual);
    assert_eq!(actual, expected, "split restart changed suffix commands");
}

/// Re-encodes the current semantic continuation behind deliberately foreign
/// positive cursor scalars, then rebinds its coordinate-free bytes to the
/// actual current source cursor and compares the complete suffix command
/// stream with the uninterrupted current parser.
fn assert_coordinate_free_durable_rebound_exact(
    prefix: &[&str],
    suffix: &[&str],
) -> Vec<DirectCommand> {
    let mut uninterrupted = parser_after(prefix);
    let current_pause = uninterrupted
        .capture_line_boundary_pause()
        .expect("current prefix pauses");
    let current_capture = uninterrupted
        .capture_durable_line_boundary_checkpoint()
        .expect("current prefix captures");

    let mut foreign_pause = current_pause.clone();
    foreign_pause.cursor.line_number = foreign_pause
        .cursor
        .line_number
        .checked_add(17)
        .expect("test cursor line remains representable");
    foreign_pause.cursor.last_line_length = if foreign_pause.cursor.last_line_length == 1 {
        "coordinate-shift".len()
    } else {
        1
    };
    let foreign_parser = DirectValueBlockParser::resume_line_boundary_pause(foreign_pause)
        .expect("foreign positive cursor remains a legal runtime pause");
    let foreign_capture = foreign_parser
        .capture_durable_line_boundary_checkpoint()
        .expect("foreign cursor continuation captures");

    assert_eq!(foreign_capture.header(), current_capture.header());
    assert_eq!(
        foreign_capture.frame_records().collect::<Vec<_>>(),
        current_capture.frame_records().collect::<Vec<_>>()
    );
    assert_eq!(&current_capture.header().as_bytes()[32..48], &[0; 16]);

    let mut resumed = DirectValueBlockParser::resume_durable_line_boundary_checkpoint(
        foreign_capture.header(),
        foreign_capture.frame_records(),
        resume_cursor(&current_pause),
    )
    .expect("coordinate-free continuation rebinds to the current cursor");
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    for line in suffix {
        drive_line(&mut uninterrupted, line, &mut expected);
        drive_line(&mut resumed, line, &mut actual);
    }
    finish(&mut uninterrupted, &mut expected);
    finish(&mut resumed, &mut actual);
    assert_eq!(actual, expected, "durable rebound changed suffix commands");
    actual
}

fn assert_suffix_resume_exact(prefix: &[&str], suffix: &[&str]) -> Vec<DirectCommand> {
    let mut uninterrupted = parser_after(prefix);
    let pause = uninterrupted
        .capture_line_boundary_pause()
        .expect("prefix boundary pauses");
    let mut resumed =
        DirectValueBlockParser::resume_line_boundary_pause(pause).expect("prefix boundary resumes");
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    for line in suffix {
        drive_line(&mut uninterrupted, line, &mut expected);
        drive_line(&mut resumed, line, &mut actual);
    }
    finish(&mut uninterrupted, &mut expected);
    finish(&mut resumed, &mut actual);
    assert_eq!(actual, expected, "resumed suffix command stream changed");
    actual
}

fn complete_commands(lines: &[&str], resume_each_line: bool) -> Vec<DirectCommand> {
    complete_commands_with_fuel(lines, resume_each_line, 1)
}

fn complete_commands_with_fuel(
    lines: &[&str],
    resume_each_line: bool,
    fuel: usize,
) -> Vec<DirectCommand> {
    let (mut parser, mut commands) = started();
    for line in lines {
        drive_line_with_fuel(&mut parser, line, &mut commands, fuel);
        if resume_each_line {
            let pause = parser
                .capture_line_boundary_pause()
                .expect("every completed line pauses");
            parser = DirectValueBlockParser::resume_line_boundary_pause(pause)
                .expect("every completed line resumes");
        }
    }
    finish_with_fuel(&mut parser, &mut commands, fuel);
    commands
}

fn command_kind_mask(commands: &[DirectCommand]) -> u16 {
    commands.iter().fold(0_u16, |mask, command| {
        mask | match command {
            DirectCommand::Open { .. } => 1 << 0,
            DirectCommand::Consume { .. } => 1 << 1,
            DirectCommand::StageTerminator { .. } => 1 << 2,
            DirectCommand::ResolveTerminator { .. } => 1 << 3,
            DirectCommand::StageBlankGap { .. } => 1 << 4,
            DirectCommand::ResolveBlankGap { .. } => 1 << 5,
            DirectCommand::FinalizeParagraph { .. } => 1 << 6,
            DirectCommand::MarkFencedCodeBoundary { .. } => 1 << 7,
            DirectCommand::Close { .. } => 1 << 8,
            DirectCommand::FinishLine { .. } => 1 << 9,
            DirectCommand::FinishDocument => 1 << 10,
        }
    })
}

fn assert_durable_line_boundary_equal(
    staged: &DirectValueBlockParser,
    legacy: &DirectValueBlockParser,
    context: &str,
) {
    let staged_pause = staged
        .capture_line_boundary_pause()
        .expect("staged line boundary pauses");
    let legacy_pause = legacy
        .capture_line_boundary_pause()
        .expect("legacy line boundary pauses");
    assert_eq!(staged_pause, legacy_pause, "pause differs after {context}");

    let staged_capture = staged
        .capture_durable_line_boundary_checkpoint()
        .expect("staged line boundary captures");
    let legacy_capture = legacy
        .capture_durable_line_boundary_checkpoint()
        .expect("legacy line boundary captures");
    assert_eq!(
        staged_capture.header(),
        legacy_capture.header(),
        "durable header differs after {context}"
    );
    assert_eq!(
        staged_capture.frame_records().collect::<Vec<_>>(),
        legacy_capture.frame_records().collect::<Vec<_>>(),
        "durable path differs after {context}"
    );
}

#[test]
fn open_new_coroutine_advances_one_handler_family_per_transition() {
    let (mut parser, _) = started();
    parser
        .begin_line("ordinary\n".to_owned())
        .expect("ordinary line begins");

    let expected_stages = [
        OpenNewStage::Start,
        OpenNewStage::BlockQuote,
        OpenNewStage::AtxHeading,
        OpenNewStage::CodeFence,
        OpenNewStage::HtmlBlock,
        OpenNewStage::SetextHeading,
        OpenNewStage::ThematicBreak,
        OpenNewStage::List,
        OpenNewStage::CodeBlock,
        OpenNewStage::Table,
    ];
    for expected in expected_stages {
        let receipt = parser.poll_line(1).expect("one transition polls");
        assert_eq!(receipt.transitions, 1);
        assert_eq!(receipt.status, DirectPollStatus::Pending);
        let stage = match &parser
            .line_work
            .as_ref()
            .expect("line remains active")
            .transition
            .as_ref()
            .expect("buffered line owns a transition")
            .phase
        {
            LinePhase::OpenNew(open) => open.stage,
            phase => panic!("expected OpenNew {expected:?}, got {phase:?}"),
        };
        assert_eq!(stage, expected);
    }

    let receipt = parser.poll_line(1).expect("table transition polls");
    assert_eq!(receipt.transitions, 1);
    assert_eq!(receipt.status, DirectPollStatus::Pending);
    assert!(matches!(
        parser
            .line_work
            .as_ref()
            .expect("line remains active")
            .transition
            .as_ref()
            .expect("buffered line owns a transition")
            .phase,
        LinePhase::PrepareText { .. }
    ));

    let (mut indented, _) = started();
    indented
        .begin_line("    code\n".to_owned())
        .expect("indented line begins");
    assert_eq!(
        indented.poll_line(1).expect("CheckOpen polls").transitions,
        1
    );
    assert_eq!(
        indented
            .poll_line(1)
            .expect("OpenNew setup polls")
            .transitions,
        1
    );
    assert!(matches!(
        indented
            .line_work
            .as_ref()
            .expect("indented line remains active")
            .transition
            .as_ref()
            .expect("buffered line owns a transition")
            .phase,
        LinePhase::OpenNew(OpenNewTransition {
            stage: OpenNewStage::List,
            indented: true,
            ..
        })
    ));
}

#[test]
fn resumable_open_new_matches_pre_refactor_commands_and_durable_pauses() {
    let cases: &[&[&str]] = &[
        &["alpha\r\n", "beta\n", "\n", "omega", "===\n"],
        &["### alpha ###   \r\n", "# beta#   \n", "tail\n"],
        &["> - alpha\r\n", ">   continuation\n", "> \n", "> - beta\n"],
        &["````rust\n", "body\r", "````\n", "tail\n", "===\n"],
        &["> > a\n", "> > \n", "> > b"],
    ];
    let mut exercised_commands = Vec::new();
    for lines in cases {
        let (mut staged, mut staged_commands) = started();
        let (mut legacy, mut legacy_commands) = started_with_legacy_open_new_oracle();
        assert_eq!(staged_commands, legacy_commands);

        for (line_index, line) in lines.iter().enumerate() {
            let staged_start = staged_commands.len();
            let legacy_start = legacy_commands.len();
            try_drive_line_with_fuel(&mut staged, line, &mut staged_commands, 1)
                .expect("staged line succeeds");
            try_drive_line_with_fuel(&mut legacy, line, &mut legacy_commands, 1)
                .expect("legacy line succeeds");
            assert_eq!(
                &staged_commands[staged_start..],
                &legacy_commands[legacy_start..],
                "line command delta differs for {line:?} in {lines:?}"
            );
            assert_eq!(
                staged_commands, legacy_commands,
                "command prefix differs for {line:?} in {lines:?}"
            );
            assert_durable_line_boundary_equal(
                &staged,
                &legacy,
                &format!("line {line_index} {line:?} in {lines:?}"),
            );
        }

        finish(&mut staged, &mut staged_commands);
        finish(&mut legacy, &mut legacy_commands);
        assert_eq!(
            staged_commands, legacy_commands,
            "EOF command stream differs for {lines:?}"
        );
        exercised_commands.extend(staged_commands);
    }
    assert_eq!(
        command_kind_mask(&exercised_commands),
        (1 << 11) - 1,
        "oracle corpus exercises every DirectCommand variant"
    );
}

#[test]
fn resumable_open_new_matches_pre_refactor_fail_closed_exits() {
    for (line, expected) in [
        (
            "***\n",
            ParseError::DirectUnsupported(DirectUnsupported::BlockKind),
        ),
        (
            "<div>\n",
            ParseError::DirectUnsupported(DirectUnsupported::BlockKind),
        ),
    ] {
        let (mut staged, mut staged_commands) = started();
        let (mut legacy, mut legacy_commands) = started_with_legacy_open_new_oracle();
        let staged_error =
            try_drive_line_with_fuel(&mut staged, line, &mut staged_commands, 1).unwrap_err();
        let legacy_error =
            try_drive_line_with_fuel(&mut legacy, line, &mut legacy_commands, 1).unwrap_err();
        assert_eq!(
            staged_error, expected,
            "wrong fail-closed exit for {line:?}"
        );
        assert_eq!(staged_error, legacy_error, "error differs for {line:?}");
        assert_eq!(
            staged_commands, legacy_commands,
            "command prefix before failure differs for {line:?}"
        );
    }
}

#[test]
fn atx_line_boundary_pause_and_restart_preserve_exact_suffix_commands() {
    for prefix in [
        ["### alpha ###   \r\n"].as_slice(),
        ["# alpha#   \n"].as_slice(),
        ["#   \n"].as_slice(),
    ] {
        let pause = pause_after(prefix);
        assert!(matches!(
            pause.frames.last().map(|frame| frame.kind),
            Some(DirectBlockKind::Heading(DirectHeadingFacts {
                setext: false,
                ..
            }))
        ));
        let commands = assert_suffix_resume_exact(prefix, &["tail\n"]);
        assert!(matches!(
            commands.first(),
            Some(DirectCommand::Close {
                kind: DirectBlockKind::Heading(DirectHeadingFacts { setext: false, .. }),
                ..
            })
        ));

        let (header, records, current) = durable_capture_after(prefix);
        let resumed = DirectValueBlockParser::resume_durable_line_boundary_checkpoint(
            header,
            records,
            resume_cursor(&current),
        )
        .expect("durable ATX pause resumes");
        assert_eq!(
            resumed.capture_line_boundary_pause().unwrap(),
            current,
            "durable ATX pause round-trips"
        );
    }
}

#[test]
fn recipe_admission_is_constant_in_physical_line_length_and_fails_closed() {
    fn admitted(
        physical_line_bytes: usize,
        open_depth: usize,
    ) -> (DirectHooks, DirectRecipeAdmissionReceipt) {
        let mut hooks = DirectHooks::new();
        hooks.emission_stack.extend(
            (0..open_depth).map(|index| {
                NodeId(u32::try_from(index).expect("synthetic direct depth fits u32"))
            }),
        );
        hooks
            .begin_recipe(physical_line_bytes)
            .expect("synthetic recipe admission succeeds");
        let receipt = hooks
            .recipe_admission
            .expect("test admission receipt is recorded");
        (hooks, receipt)
    }

    let open_depth = 7;
    let (_, short) = admitted(1, open_depth);
    let (mut giant, giant_receipt) = admitted(10 * 1024 * 1024, open_depth);
    assert_eq!(short.physical_line_bytes, 1);
    assert_eq!(giant_receipt.physical_line_bytes, 10 * 1024 * 1024);
    assert_eq!(
        short.line_length_independent_part(),
        giant_receipt.line_length_independent_part(),
        "line length cannot enter intent capacity requests or the intent limit"
    );
    assert_eq!(
        giant_receipt.requested_slots(),
        DIRECT_INITIAL_PREVIOUS_INTENTS + DIRECT_INITIAL_BODY_INTENTS + 3 * open_depth
    );
    assert_eq!(
        giant_receipt.intent_limit,
        DIRECT_LINE_LOCAL_INTENT_LIMIT + DIRECT_OPEN_FRAME_INTENT_ALLOWANCE * open_depth
    );

    let filler = DirectIntent::ResolveTerminator {
        resolution: DirectTerminatorResolution::CloseNone,
    };
    for _ in 0..giant_receipt.intent_limit {
        giant
            .push_body(filler.clone())
            .expect("fixed proof envelope admits exactly its bounded count");
    }
    assert_eq!(
        giant.push_body(filler),
        Err(ParseError::Invariant("direct line intent bound exceeded")),
        "syntax beyond the fixed/depth-proportional envelope fails closed"
    );
}

#[test]
fn fixed_recipe_admission_preserves_exhaustive_ordinary_command_streams() {
    let cases: &[&[&str]] = &[
        &["alpha\r\n", "beta\n", "\n", "omega", "===\n"],
        &["> - alpha\r\n", ">   continuation\n", "> \n", "> - beta\n"],
        &["````rust\n", "body\r", "````\n", "tail\n", "===\n"],
        &["> > a\n", "> > \n", "> > b"],
    ];
    let mut all_commands = Vec::new();
    for lines in cases {
        let fuel_one = complete_commands_with_fuel(lines, false, 1);
        let fuel_many = complete_commands_with_fuel(lines, false, 64);
        let resumed = complete_commands_with_fuel(lines, true, 7);
        assert_eq!(
            fuel_many, fuel_one,
            "poll fuel changed commands for {lines:?}"
        );
        assert_eq!(
            resumed, fuel_one,
            "line resume changed commands for {lines:?}"
        );
        all_commands.extend(fuel_one);
    }
    assert_eq!(
        command_kind_mask(&all_commands),
        (1 << 11) - 1,
        "the equivalence corpus exercises every direct command variant"
    );
}

#[test]
fn pending_paragraph_terminator_resumes_for_continuation_setext_and_eof() {
    let pause = pause_after(&["alpha\r\n"]);
    assert!(pause.deferred.terminator);
    assert!(!pause.deferred.blank_gap);

    let continued = assert_suffix_resume_exact(&["alpha\r\n"], &["beta"]);
    assert!(continued.contains(&DirectCommand::ResolveTerminator {
        resolution: DirectTerminatorResolution::ContinueCanonicalNewline,
    }));

    let setext = assert_suffix_resume_exact(&["alpha\n"], &["===\n", "tail"]);
    assert!(setext.contains(&DirectCommand::FinalizeParagraph {
        outcome: DirectParagraphOutcome::SetextHeading { level: 1 },
    }));

    let eof = assert_suffix_resume_exact(&["alpha\n"], &[]);
    assert!(eof.contains(&DirectCommand::ResolveTerminator {
        resolution: DirectTerminatorResolution::CloseNone,
    }));
}

#[test]
fn durable_split_checkpoint_round_trips_the_setext_boundary() {
    let mut uninterrupted = parser_after(&["alpha\n"]);
    let pause = uninterrupted
        .capture_line_boundary_pause()
        .expect("Setext prefix pauses");
    let capture = uninterrupted
        .capture_durable_line_boundary_checkpoint()
        .expect("Setext prefix has a durable donor capture");
    let receipt = capture.receipt();
    assert_eq!(receipt.sample_header_bytes, 64);
    assert_eq!(receipt.materialized_path_records, 2);
    assert_eq!(receipt.materialized_path_bytes, 2 * 48);
    assert_eq!(receipt.retained_source_bytes, 0);
    assert_eq!(
        std::mem::size_of::<DirectDurableLineBoundaryHeader>(),
        DIRECT_DURABLE_LINE_BOUNDARY_HEADER_BYTES
    );
    assert_eq!(
        std::mem::size_of::<DirectDurableLineBoundaryFrameRecord>(),
        DIRECT_DURABLE_LINE_BOUNDARY_FRAME_BYTES
    );
    assert!(
        !capture
            .header()
            .as_bytes()
            .windows("alpha".len())
            .any(|window| window == b"alpha")
    );

    let persisted_header = *capture.header().as_bytes();
    let header = DirectDurableLineBoundaryHeader::from_bytes(&persisted_header)
        .expect("persisted donor header decodes");
    let records = capture
        .frame_records()
        .map(|record| {
            DirectDurableLineBoundaryFrameRecord::from_bytes(record.as_bytes())
                .expect("persisted opaque frame decodes")
        })
        .collect::<Vec<_>>();
    let expected_restart_parts = pause
        .clone()
        .into_restart_parts()
        .expect("captured pause splits into expected restart parts");
    let decoded_restart_parts =
        DirectValueBlockParser::decode_durable_restart_parts(header, records.iter().copied())
            .expect("durable sample decodes without a source cursor or parser rebuild");
    assert_eq!(decoded_restart_parts, expected_restart_parts);
    let cursor = resume_cursor(&pause);
    let decoded = direct_durable_parts_into_pause(header, records.iter().copied(), cursor)
        .expect("split parts decode to one donor pause");
    assert_eq!(decoded, pause);
    assert_eq!(
        decoded.paragraph,
        Some(DirectPauseParagraphState {
            frame_depth: 1,
            has_visible_content: true,
            may_have_reference_prefix: false,
        })
    );

    let mut resumed = DirectValueBlockParser::resume_durable_line_boundary_checkpoint(
        header,
        records.iter().copied(),
        cursor,
    )
    .expect("durable Setext checkpoint resumes");
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    for line in ["===\n", "tail"] {
        drive_line(&mut uninterrupted, line, &mut expected);
        drive_line(&mut resumed, line, &mut actual);
    }
    finish(&mut uninterrupted, &mut expected);
    finish(&mut resumed, &mut actual);
    assert_eq!(actual, expected);
    assert!(actual.contains(&DirectCommand::FinalizeParagraph {
        outcome: DirectParagraphOutcome::SetextHeading { level: 1 },
    }));
}

#[test]
fn durable_split_checkpoint_rejects_corruption_reordering_and_old_schema() {
    let parser = parser_after(&["> - alpha\n"]);
    let pause = parser
        .capture_line_boundary_pause()
        .expect("nested prefix pauses");
    let capture = parser
        .capture_durable_line_boundary_checkpoint()
        .expect("nested prefix captures");
    let header = capture.header();
    let records = capture.frame_records().collect::<Vec<_>>();
    let cursor = resume_cursor(&pause);
    assert!(records.len() >= 5);

    let mut wrong_schema = *header.as_bytes();
    direct_durable_write_u32(&mut wrong_schema, 8, DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA + 1);
    let checksum = direct_durable_checksum(DIRECT_DURABLE_CHECKSUM_OFFSET, &wrong_schema[..56]);
    direct_durable_write_u64(&mut wrong_schema, 56, checksum);
    assert!(DirectDurableLineBoundaryHeader::from_bytes(&wrong_schema).is_err());

    let mut schema_one = *header.as_bytes();
    direct_durable_write_u32(&mut schema_one, 8, 1);
    let checksum = direct_durable_checksum(DIRECT_DURABLE_CHECKSUM_OFFSET, &schema_one[..56]);
    direct_durable_write_u64(&mut schema_one, 56, checksum);
    assert!(DirectDurableLineBoundaryHeader::from_bytes(&schema_one).is_err());

    let mut corrupted_header = *header.as_bytes();
    corrupted_header[32] ^= 1;
    assert!(DirectDurableLineBoundaryHeader::from_bytes(&corrupted_header).is_err());

    let mut invalid_paragraph = *header.as_bytes();
    invalid_paragraph[18] = 1;
    let checksum =
        direct_durable_checksum(DIRECT_DURABLE_CHECKSUM_OFFSET, &invalid_paragraph[..56]);
    direct_durable_write_u64(&mut invalid_paragraph, 56, checksum);
    let invalid_paragraph = DirectDurableLineBoundaryHeader::from_bytes(&invalid_paragraph)
        .expect("content-free Paragraph is a decodable future-state shape");
    assert_durable_restart_decode_rejects(invalid_paragraph, records.clone());
    assert!(
        DirectValueBlockParser::resume_durable_line_boundary_checkpoint(
            invalid_paragraph,
            records.iter().copied(),
            cursor
        )
        .is_err()
    );

    let mut missing = records.clone();
    missing.pop();
    assert_durable_restart_decode_rejects(header, missing.clone());
    assert!(
        DirectValueBlockParser::resume_durable_line_boundary_checkpoint(header, missing, cursor)
            .is_err()
    );

    let mut reordered = records.clone();
    reordered.swap(1, 2);
    assert_durable_restart_decode_rejects(header, reordered.clone());
    assert!(
        DirectValueBlockParser::resume_durable_line_boundary_checkpoint(header, reordered, cursor)
            .is_err()
    );

    let mut valid_but_corrupted = *records[1].as_bytes();
    valid_but_corrupted[5] ^= 1;
    let valid_but_corrupted =
        DirectDurableLineBoundaryFrameRecord::from_bytes(&valid_but_corrupted)
            .expect("toggled blank flag remains an individually canonical frame");
    let mut corrupted_records = records.clone();
    corrupted_records[1] = valid_but_corrupted;
    assert_durable_restart_decode_rejects(header, corrupted_records.clone());
    assert!(
        DirectValueBlockParser::resume_durable_line_boundary_checkpoint(
            header,
            corrupted_records,
            cursor
        )
        .is_err()
    );

    let mut foreign_schema = *records[1].as_bytes();
    direct_durable_write_u32(
        &mut foreign_schema,
        0,
        DIRECT_LINE_BOUNDARY_PAUSE_SCHEMA + 1,
    );
    assert!(DirectDurableLineBoundaryFrameRecord::from_bytes(&foreign_schema).is_err());
    let mut foreign_records = records.clone();
    foreign_records[1] = DirectDurableLineBoundaryFrameRecord {
        bytes: foreign_schema,
    };
    assert!(
        DirectValueBlockParser::resume_durable_line_boundary_checkpoint(
            header,
            foreign_records,
            cursor
        )
        .is_err()
    );

    let mut schema_one_frame = *records[1].as_bytes();
    direct_durable_write_u32(&mut schema_one_frame, 0, 1);
    assert!(DirectDurableLineBoundaryFrameRecord::from_bytes(&schema_one_frame).is_err());

    assert_durable_cursor_validation_and_rebinding(header, &records);
}

fn assert_durable_cursor_validation_and_rebinding(
    header: DirectDurableLineBoundaryHeader,
    records: &[DirectDurableLineBoundaryFrameRecord],
) {
    assert!(DirectLineBoundaryResumeCursor::new(0, 0).is_err());
    assert!(
        DirectLineBoundaryResumeCursor::new(1, u64::try_from(DIRECT_MAX_LINE_BYTES + 1).unwrap())
            .is_ok()
    );
    assert_eq!(
        DirectLineBoundaryResumeCursor::new(1, u64::MAX).is_ok(),
        usize::BITS == u64::BITS
    );
    let shifted_line = DirectLineBoundaryResumeCursor::new(27, 9).unwrap();
    assert!(
        DirectValueBlockParser::resume_durable_line_boundary_checkpoint(
            header,
            records.iter().copied(),
            shifted_line
        )
        .is_ok()
    );
    let shifted_length = DirectLineBoundaryResumeCursor::new(1, 1).unwrap();
    assert!(
        DirectValueBlockParser::resume_durable_line_boundary_checkpoint(
            header,
            records.iter().copied(),
            shifted_length
        )
        .is_ok()
    );
}

#[test]
fn durable_semantic_bytes_exclude_positive_line_and_previous_line_coordinates() {
    let (ascii_header, ascii_frames, ascii_pause) = durable_capture_after(&["alpha\n"]);
    let (emoji_header, emoji_frames, emoji_pause) = durable_capture_after(&["😀\n"]);
    assert_ne!(
        ascii_pause.cursor.last_line_length,
        emoji_pause.cursor.last_line_length
    );
    assert_eq!(ascii_header, emoji_header);
    assert_eq!(ascii_frames, emoji_frames);
    assert_eq!(&ascii_header.as_bytes()[32..48], &[0; 16]);

    let mut shifted = ascii_pause.clone();
    shifted.cursor.line_number += 41;
    shifted.cursor.last_line_length = "😀".len();
    let shifted = DirectValueBlockParser::resume_line_boundary_pause(shifted)
        .expect("shifted positive source cursor resumes")
        .capture_durable_line_boundary_checkpoint()
        .expect("shifted source cursor captures");
    assert_eq!(ascii_header, shifted.header());
    assert_eq!(ascii_frames, shifted.frame_records().collect::<Vec<_>>());
}

#[test]
fn coordinate_free_durable_rebound_matches_current_suffix_across_direct_slice() {
    let setext = assert_coordinate_free_durable_rebound_exact(&["😀\n"], &["===\n", "tail"]);
    assert!(setext.contains(&DirectCommand::FinalizeParagraph {
        outcome: DirectParagraphOutcome::SetextHeading { level: 1 },
    }));

    let blank_gap = assert_coordinate_free_durable_rebound_exact(
        &["> - alpha\r\n", ">   \r\n"],
        &["> - beta\n", "tail"],
    );
    assert!(
        blank_gap
            .iter()
            .any(|command| matches!(command, DirectCommand::ResolveBlankGap { .. }))
    );

    let fence = assert_coordinate_free_durable_rebound_exact(
        &[">   ````lang\r\n", ">  body\n"],
        &[">   ````  \n", "tail"],
    );
    assert!(fence.iter().any(|command| {
        matches!(
            command,
            DirectCommand::Close {
                final_facts: DirectFinalFacts::FencedCode(DirectFencedCodeCloseFacts {
                    closed: true
                }),
                ..
            }
        )
    }));

    for prefix in [&["alpha\n"][..], &["alpha\r"][..], &["alpha\r\n"][..]] {
        assert_coordinate_free_durable_rebound_exact(prefix, &["beta"]);
    }
    let typed =
        assert_coordinate_free_durable_rebound_exact(&[">  ```\n"], &[">\tbody\0😀\n", ">  ```\n"]);
    assert!(typed.iter().any(|command| matches!(
        command,
        DirectCommand::Consume {
            owner: DirectOwner::PARENT_OF_TOP,
            logical: DirectLogicalAction::PartialTab(partial),
            ..
        } if partial.logical_target() == DirectOwner::TOP
            && partial.remaining_spaces() == 1
    )));
    assert_coordinate_free_durable_rebound_exact(&["alpha"], &[]);

    let bom = "\u{feff}beta";
    let commands = assert_coordinate_free_durable_rebound_exact(&["alpha\n"], &[bom]);
    assert!(commands.iter().any(|command| {
        matches!(
            command,
            DirectCommand::Consume {
                part: DirectCoveragePart::Content,
                range,
                logical: DirectLogicalAction::CanonicalText,
                ..
            } if *range == (0..u32::try_from(bom.len()).expect("test line fits u32"))
        )
    }));
}

#[test]
fn in_memory_split_reconstructs_the_exact_direct_suffix_command_stream() {
    assert_in_memory_split_reconstructs_exact(&["😀\n"], &["===\n", "tail"]);
    assert_in_memory_split_reconstructs_exact(
        &["> - alpha\r\n", ">   \r\n"],
        &["> - beta\n", "tail"],
    );
    assert_in_memory_split_reconstructs_exact(
        &[">   ````lang\r\n", ">  body\n"],
        &[">   ````  \n", "tail"],
    );
    assert_in_memory_split_reconstructs_exact(&[">  ```\n"], &[">\tbody\0😀\n", ">  ```\n"]);
    assert_in_memory_split_reconstructs_exact(&["a\tb\0\n"], &["tail\t\0"]);
    assert_in_memory_split_reconstructs_exact(&["alpha\r"], &["beta"]);
    assert_in_memory_split_reconstructs_exact(&["alpha"], &[]);
}

#[test]
fn output_only_facts_keep_grammar_equal_and_reconstruct_their_exact_recipe() {
    let (list_grammar, list_output, list_cursor) = restart_parts_after(&["1. alpha\n"]);

    let mut changed_start = list_output.clone();
    let list = changed_start
        .frames
        .iter_mut()
        .find_map(|frame| match &mut frame.kind {
            DirectBlockKind::List(facts) => Some(facts),
            _ => None,
        })
        .expect("ordered list frame");
    assert_eq!(list.start, 1);
    list.start = 7;
    assert_ne!(changed_start, list_output);
    assert_compatible_output_reconstructs_exactly(&list_grammar, changed_start, list_cursor);

    let mut changed_item_decomposition = list_output.clone();
    let item = changed_item_decomposition
        .frames
        .iter_mut()
        .find_map(|frame| match &mut frame.kind {
            DirectBlockKind::Item(facts) => Some(facts),
            _ => None,
        })
        .expect("ordered item frame");
    let effective = u32::from(item.marker_offset) + u32::from(item.padding);
    assert!(item.marker_offset < 3 && item.padding > 2);
    item.marker_offset += 1;
    item.padding -= 1;
    assert_eq!(
        u32::from(item.marker_offset) + u32::from(item.padding),
        effective
    );
    assert_ne!(changed_item_decomposition, list_output);
    assert_compatible_output_reconstructs_exactly(
        &list_grammar,
        changed_item_decomposition,
        list_cursor,
    );

    let (fold_grammar, fold_output, fold_cursor) = restart_parts_after(&["- a\n", "\n", "- b\n"]);
    let mut changed_fold = fold_output.clone();
    let list_frame = changed_fold
        .frames
        .iter_mut()
        .find(|frame| matches!(frame.kind, DirectBlockKind::List(_)))
        .expect("list output frame");
    assert!(list_frame.closed_children.had_child);
    list_frame.closed_children.list_loose_before_last =
        !list_frame.closed_children.list_loose_before_last;
    assert_ne!(changed_fold, fold_output);
    assert_compatible_output_reconstructs_exactly(&fold_grammar, changed_fold, fold_cursor);

    let (heading_grammar, heading_output, heading_cursor) =
        restart_parts_after(&["alpha\n", "===\n"]);
    let mut changed_heading = heading_output.clone();
    let heading = changed_heading
        .frames
        .iter_mut()
        .find_map(|frame| match &mut frame.kind {
            DirectBlockKind::Heading(facts) => Some(facts),
            _ => None,
        })
        .expect("Setext heading output frame");
    heading.level = if heading.level == 1 { 2 } else { 1 };
    assert_ne!(changed_heading, heading_output);
    assert_compatible_output_reconstructs_exactly(
        &heading_grammar,
        changed_heading,
        heading_cursor,
    );
}

#[test]
fn current_output_builder_uses_only_current_frames_and_stabilized_line_local_bits() {
    let (list_grammar, list_output, list_cursor) =
        restart_parts_after(&["1. alpha\n", "\n", "2. beta\n"]);
    let line_local_source = list_output.clone();

    let mut current_output = list_output;
    let list = current_output
        .frames
        .iter_mut()
        .find(|frame| matches!(frame.kind, DirectBlockKind::List(_)))
        .expect("ordered list output frame");
    let DirectBlockKind::List(facts) = &mut list.kind else {
        unreachable!("selected list frame")
    };
    facts.start = 7;
    list.closed_children.list_loose_before_last = !list.closed_children.list_loose_before_last;

    let bound = list_grammar
        .bind_current_restart_output(
            line_local_source.line_local_output(),
            restart_frame_outputs(&current_output),
        )
        .expect("grammar-compatible current list output binds");
    for ((bound_frame, current_frame), retained_frame) in bound
        .frames
        .iter()
        .zip(current_output.frames.iter())
        .zip(line_local_source.frames.iter())
    {
        assert_eq!(bound_frame.kind, current_frame.kind);
        assert_eq!(bound_frame.closed_children, current_frame.closed_children);
        assert_eq!(bound_frame.last_line_blank, retained_frame.last_line_blank);
    }
    assert_compatible_output_reconstructs_exactly(&list_grammar, bound, list_cursor);

    let (heading_grammar, heading_output, heading_cursor) =
        restart_parts_after(&["alpha\n", "===\n"]);
    let mut current_heading = heading_output.clone();
    let facts = current_heading
        .frames
        .iter_mut()
        .find_map(|frame| match &mut frame.kind {
            DirectBlockKind::Heading(facts) => Some(facts),
            _ => None,
        })
        .expect("Setext heading output frame");
    assert_eq!(facts.level, 1);
    facts.level = 2;
    let bound_heading = heading_grammar
        .bind_current_restart_output(
            heading_output.line_local_output(),
            restart_frame_outputs(&current_heading),
        )
        .expect("output-only heading level binds");
    assert_compatible_output_reconstructs_exactly(&heading_grammar, bound_heading, heading_cursor);

    let (current_grammar, current_output, current_cursor) =
        restart_parts_after(&["alpha\n", "\n", "- \n"]);
    let (retained_grammar, retained_output, _) = restart_parts_after(&["\n", "- \n", "  \n"]);
    assert_eq!(current_grammar, retained_grammar);
    assert_ne!(
        current_output
            .frames
            .iter()
            .map(|frame| frame.last_line_blank)
            .collect::<Vec<_>>(),
        retained_output
            .frames
            .iter()
            .map(|frame| frame.last_line_blank)
            .collect::<Vec<_>>()
    );
    let rebound = current_grammar
        .bind_current_restart_output(
            retained_output.line_local_output(),
            restart_frame_outputs(&current_output),
        )
        .expect("authentic grammar-equal line-local sample binds to current frames");
    for ((bound, current), retained) in rebound
        .frames
        .iter()
        .zip(current_output.frames.iter())
        .zip(retained_output.frames.iter())
    {
        assert_eq!(bound.kind, current.kind);
        assert_eq!(bound.closed_children, current.closed_children);
        assert_eq!(bound.last_line_blank, retained.last_line_blank);
    }
    assert_compatible_output_reconstructs_exactly(&current_grammar, rebound, current_cursor);
}

#[test]
fn current_output_builder_rejects_wrong_count_and_invalid_frame_facts() {
    let (ordered_grammar, ordered_output, _) = restart_parts_after(&["1. alpha\n"]);
    let ordered_line_local = ordered_output.line_local_output();
    let ordered_frames = restart_frame_outputs(&ordered_output);

    let mut short = ordered_frames.clone();
    short.pop();
    assert!(
        ordered_grammar
            .bind_current_restart_output(ordered_line_local, short)
            .is_err()
    );
    let mut long = ordered_frames.clone();
    long.push(*long.last().expect("ordered path is nonempty"));
    assert!(
        ordered_grammar
            .bind_current_restart_output(ordered_line_local, long)
            .is_err()
    );

    let mut changed_delimiter = ordered_frames.clone();
    let list = changed_delimiter
        .iter_mut()
        .find_map(|frame| match &mut frame.kind {
            DirectBlockKind::List(facts) => Some(facts),
            _ => None,
        })
        .expect("ordered list frame");
    list.delimiter = ListDelimiter::Paren;
    assert!(
        ordered_grammar
            .bind_current_restart_output(ordered_line_local, changed_delimiter)
            .is_err()
    );

    let mut changed_indent = ordered_frames.clone();
    let item = changed_indent
        .iter_mut()
        .find_map(|frame| match &mut frame.kind {
            DirectBlockKind::Item(facts) => Some(facts),
            _ => None,
        })
        .expect("ordered item frame");
    item.padding += 1;
    assert!(
        ordered_grammar
            .bind_current_restart_output(ordered_line_local, changed_indent)
            .is_err()
    );

    let mut malformed_start = ordered_frames.clone();
    let list = malformed_start
        .iter_mut()
        .find_map(|frame| match &mut frame.kind {
            DirectBlockKind::List(facts) => Some(facts),
            _ => None,
        })
        .expect("ordered list frame");
    list.start = 1_000_000_000;
    assert!(
        ordered_grammar
            .bind_current_restart_output(ordered_line_local, malformed_start)
            .is_err()
    );
}

#[test]
fn current_output_builder_rejects_crossed_and_grammar_affecting_inputs() {
    let (ordered_grammar, ordered_output, _) = restart_parts_after(&["1. alpha\n"]);
    let ordered_frames = restart_frame_outputs(&ordered_output);

    let (_, bullet_output, _) = restart_parts_after(&["- alpha\n"]);
    assert!(
        ordered_grammar
            .bind_current_restart_output(bullet_output.line_local_output(), ordered_frames.clone(),)
            .is_err()
    );

    let (empty_item_grammar, empty_item_output, _) = restart_parts_after(&["- \n"]);
    let mut false_child_presence = restart_frame_outputs(&empty_item_output);
    let item = false_child_presence
        .iter_mut()
        .find(|frame| matches!(frame.kind, DirectBlockKind::Item(_)))
        .expect("empty item frame");
    assert!(!item.closed_children.had_child);
    item.closed_children.had_child = true;
    assert!(
        empty_item_grammar
            .bind_current_restart_output(
                empty_item_output.line_local_output(),
                false_child_presence,
            )
            .is_err()
    );

    let (fence_grammar, fence_output, _) = restart_parts_after(&["```lang\n"]);
    let mut changed_fence = restart_frame_outputs(&fence_output);
    let fence = changed_fence
        .iter_mut()
        .find_map(|frame| match &mut frame.kind {
            DirectBlockKind::FencedCode(facts) => Some(facts),
            _ => None,
        })
        .expect("fenced-code frame");
    fence.minimum_closing_length += 1;
    assert!(
        fence_grammar
            .bind_current_restart_output(fence_output.line_local_output(), changed_fence)
            .is_err()
    );

    let (heading_grammar, heading_output, _) = restart_parts_after(&["alpha\n", "===\n"]);
    let mut malformed_heading = restart_frame_outputs(&heading_output);
    let heading = malformed_heading
        .iter_mut()
        .find_map(|frame| match &mut frame.kind {
            DirectBlockKind::Heading(facts) => Some(facts),
            _ => None,
        })
        .expect("Setext heading frame");
    heading.level = 3;
    assert!(
        heading_grammar
            .bind_current_restart_output(heading_output.line_local_output(), malformed_heading)
            .is_err()
    );
}

#[test]
fn impossible_block_quote_blankness_is_rejected_by_decode_bind_and_resume() {
    let parser = parser_after(&["> alpha\n"]);
    let capture = parser
        .capture_durable_line_boundary_checkpoint()
        .expect("quote prefix captures durably");
    let (grammar, output, cursor) = restart_parts_after(&["> alpha\n"]);
    let mut impossible = output.clone();
    let quote = impossible
        .frames
        .iter_mut()
        .find(|frame| frame.kind == DirectBlockKind::BlockQuote)
        .expect("quote frame exists");
    assert!(!quote.last_line_blank);
    quote.last_line_blank = true;
    assert!(
        DirectValueBlockParser::resume_restart_parts(&grammar, impossible.clone(), cursor).is_err()
    );
    assert!(
        grammar
            .bind_current_restart_output(
                impossible.line_local_output(),
                restart_frame_outputs(&output),
            )
            .is_err()
    );

    let mut records = capture.frame_records().collect::<Vec<_>>();
    let mut quote_record = *records[1].as_bytes();
    assert_eq!(quote_record[4], 1, "second frame is BlockQuote");
    quote_record[5] = 1;
    records[1] = DirectDurableLineBoundaryFrameRecord::from_bytes(&quote_record)
        .expect("true blankness is byte-canonical before shape validation");
    let mut path_checksum = DIRECT_DURABLE_CHECKSUM_OFFSET;
    for record in &records {
        path_checksum = direct_durable_checksum(path_checksum, record.as_bytes());
    }
    let mut header = *capture.header().as_bytes();
    direct_durable_write_u64(&mut header, 48, path_checksum);
    let header_checksum = direct_durable_checksum(DIRECT_DURABLE_CHECKSUM_OFFSET, &header[..56]);
    direct_durable_write_u64(&mut header, 56, header_checksum);
    let header = DirectDurableLineBoundaryHeader::from_bytes(&header)
        .expect("rechecksummed impossible sample has a canonical header");
    assert!(DirectValueBlockParser::decode_durable_restart_parts(header, records).is_err());
}

#[test]
#[allow(clippy::too_many_lines)]
fn reachable_line_local_divergence_stabilizes_after_one_identical_line() {
    struct Sample {
        lines: Vec<&'static str>,
        blankness: Vec<bool>,
        output: DirectRestartOutput,
        cursor: DirectLineBoundaryResumeCursor,
    }
    struct Group {
        grammar: DirectGrammarContinuation,
        samples: Vec<Sample>,
    }

    let alphabet = [
        "alpha\n",
        "\n",
        "> alpha\n",
        "> \n",
        "- alpha\n",
        "- \n",
        "  \n",
        "> - alpha\n",
        ">   \n",
        "1. alpha\n",
        "   beta\n",
        "```\n",
        "code\n",
        "> ```\n",
        "> code\n",
        "===\n",
    ];
    let next_lines = [
        "\n",
        "alpha\n",
        "> \n",
        "> alpha\n",
        "- beta\n",
        "- \n",
        "  beta\n",
        "> - beta\n",
        ">   \n",
        "2. beta\n",
        "   beta\n",
        "```\n",
        "> ```\n",
        "> code\n",
        "outside\n",
        "===\n",
    ];
    let mut groups = Vec::<Group>::new();
    let mut parsed = 0_usize;
    let mut kind_blankness = [[false; 2]; 7];
    let mut blank_without_deferred_gap = 0_usize;
    let mut deferred_gap_without_blank = 0_usize;
    let mut multiple_blank_frames = 0_usize;
    let mut blank_off_current_frame = 0_usize;
    let mut record = |lines: Vec<&'static str>| {
        let Some((grammar, output, cursor)) = try_restart_parts_after(&lines) else {
            return;
        };
        parsed += 1;
        for frame in &output.frames {
            let kind = match frame.kind {
                DirectBlockKind::Document => 0,
                DirectBlockKind::BlockQuote => 1,
                DirectBlockKind::List(_) => 2,
                DirectBlockKind::Item(_) => 3,
                DirectBlockKind::Paragraph => 4,
                DirectBlockKind::Heading(_) => 5,
                DirectBlockKind::FencedCode(_) => 6,
            };
            kind_blankness[kind][usize::from(frame.last_line_blank)] = true;
        }
        let any_blank = output.frames.iter().any(|frame| frame.last_line_blank);
        let blank_depths = output
            .frames
            .iter()
            .enumerate()
            .filter_map(|(depth, frame)| frame.last_line_blank.then_some(depth))
            .collect::<Vec<_>>();
        blank_without_deferred_gap += usize::from(any_blank && !output.deferred.blank_gap);
        deferred_gap_without_blank += usize::from(output.deferred.blank_gap && !any_blank);
        multiple_blank_frames += usize::from(blank_depths.len() > 1);
        blank_off_current_frame += usize::from(
            blank_depths
                .first()
                .is_some_and(|depth| *depth != output.current_frame),
        );
        let sample = Sample {
            lines,
            blankness: blank_depths.iter().fold(
                vec![false; output.frames.len()],
                |mut bits, depth| {
                    bits[*depth] = true;
                    bits
                },
            ),
            output,
            cursor,
        };
        if let Some(group) = groups.iter_mut().find(|group| group.grammar == grammar) {
            let same_blankness = group
                .samples
                .iter()
                .filter(|prior| prior.blankness == sample.blankness)
                .count();
            if same_blankness < 2
                && !group
                    .samples
                    .iter()
                    .any(|prior| prior.output == sample.output)
            {
                group.samples.push(sample);
            }
        } else {
            groups.push(Group {
                grammar,
                samples: vec![sample],
            });
        }
    };

    for first in alphabet {
        record(vec![first]);
        for second in alphabet {
            record(vec![first, second]);
            for third in alphabet {
                record(vec![first, second, third]);
            }
        }
    }
    assert!(parsed > 3000, "bounded corpus unexpectedly lost coverage");
    assert_eq!(groups.len(), 19, "reachable grammar group count changed");
    assert_eq!(
        kind_blankness,
        [
            [true, true],
            [true, false],
            [true, true],
            [true, true],
            [true, false],
            [true, false],
            [true, false],
        ],
        "per-kind line-local reachability changed"
    );
    assert_eq!(blank_without_deferred_gap, 0);
    assert!(deferred_gap_without_blank > 0);
    assert_eq!(multiple_blank_frames, 0);
    assert_eq!(blank_off_current_frame, 0);

    let mut divergent_pairs = 0_usize;
    let mut successful_transitions = 0_usize;
    for group in &groups {
        for left_index in 0..group.samples.len() {
            for right_index in left_index + 1..group.samples.len() {
                let left = &group.samples[left_index];
                let right = &group.samples[right_index];
                if left.blankness == right.blankness {
                    continue;
                }
                divergent_pairs += 1;
                for next in next_lines {
                    let left_post = try_feed_restart_line(
                        &group.grammar,
                        left.output.clone(),
                        left.cursor,
                        next,
                    );
                    let right_post = try_feed_restart_line(
                        &group.grammar,
                        right.output.clone(),
                        right.cursor,
                        next,
                    );
                    let (left_grammar, left_output, right_grammar, right_output) = match (
                        left_post, right_post,
                    ) {
                        (Ok((left_grammar, left_output)), Ok((right_grammar, right_output))) => {
                            (left_grammar, left_output, right_grammar, right_output)
                        }
                        (Err(_), Err(_)) => continue,
                        (left_result, right_result) => panic!(
                            "grammar-equal authentic samples diverged in transition legality: left={:?} right={:?} next={next:?} left_ok={} right_ok={}",
                            left.lines,
                            right.lines,
                            left_result.is_ok(),
                            right_result.is_ok()
                        ),
                    };
                    successful_transitions += 1;
                    assert_eq!(
                        left_grammar, right_grammar,
                        "authentic grammar-equal samples diverged after {:?} vs {:?} on {next:?}",
                        left.lines, right.lines
                    );
                    assert_eq!(
                        left_output.frames.len(),
                        right_output.frames.len(),
                        "post-line paths are not aligned"
                    );
                    for (depth, (left_frame, right_frame)) in left_output
                        .frames
                        .iter()
                        .zip(right_output.frames.iter())
                        .enumerate()
                    {
                        assert_eq!(
                            left_frame.last_line_blank, right_frame.last_line_blank,
                            "authentic survivor blankness differs at depth {depth}: left={:?} right={:?} next={next:?}",
                            left.lines, right.lines
                        );
                    }
                }
            }
        }
    }
    assert_eq!(
        (divergent_pairs, successful_transitions),
        (4, 32),
        "deterministic authentic divergence/property-gate receipt changed"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn adversarial_identical_lines_stabilize_donor_reachable_survivor_blankness() {
    struct Case {
        name: &'static str,
        prefix: &'static [&'static str],
        next: &'static str,
        expect_blank_gap_floor: bool,
        expected_item_has_child: Option<bool>,
        expect_close: bool,
    }

    let cases = [
        Case {
            name: "blank-gap floor resolves into a nonblank sibling",
            prefix: &["> - alpha\n", ">   \n"],
            next: "> - beta\n",
            expect_blank_gap_floor: true,
            expected_item_has_child: Some(true),
            expect_close: true,
        },
        Case {
            name: "blank-gap floor survives another marked blank",
            prefix: &["> - alpha\n", ">   \n"],
            next: ">   \n",
            expect_blank_gap_floor: true,
            expected_item_has_child: Some(true),
            expect_close: false,
        },
        Case {
            name: "empty item closes before a sibling",
            prefix: &["- \n"],
            next: "- beta\n",
            expect_blank_gap_floor: true,
            expected_item_has_child: Some(false),
            expect_close: true,
        },
        Case {
            name: "existing item receives a raw blank",
            prefix: &["- alpha\n"],
            next: "\n",
            expect_blank_gap_floor: false,
            expected_item_has_child: Some(true),
            expect_close: true,
        },
        Case {
            name: "lazy Paragraph continuation omits the quote marker",
            prefix: &["> alpha\n"],
            next: "lazy\n",
            expect_blank_gap_floor: false,
            expected_item_has_child: None,
            expect_close: false,
        },
        Case {
            name: "nested quote list continues",
            prefix: &["> - alpha\n"],
            next: ">   beta\n",
            expect_blank_gap_floor: false,
            expected_item_has_child: Some(true),
            expect_close: false,
        },
        Case {
            name: "nested quote list closes on first lookahead",
            prefix: &["> - alpha\n"],
            next: "- outside\n",
            expect_blank_gap_floor: false,
            expected_item_has_child: Some(true),
            expect_close: true,
        },
        Case {
            name: "fenced content is nonblank",
            prefix: &["> ```\n"],
            next: "> code\n",
            expect_blank_gap_floor: false,
            expected_item_has_child: None,
            expect_close: false,
        },
        Case {
            name: "fenced content receives a marked blank",
            prefix: &["> ```\n"],
            next: "> \n",
            expect_blank_gap_floor: false,
            expected_item_has_child: None,
            expect_close: false,
        },
        Case {
            name: "ordered display accumulator survives continuation",
            prefix: &["1. alpha\n"],
            next: "   beta\n",
            expect_blank_gap_floor: false,
            expected_item_has_child: Some(true),
            expect_close: false,
        },
    ];

    for case in cases {
        let (grammar, base_output, cursor) = restart_parts_after(case.prefix);
        assert_eq!(
            base_output.deferred.blank_gap_floor.is_some(),
            case.expect_blank_gap_floor,
            "precondition failed for {}",
            case.name
        );
        if let Some(expected) = case.expected_item_has_child {
            let actual = base_output
                .frames
                .iter()
                .enumerate()
                .find_map(|(index, frame)| {
                    matches!(frame.kind, DirectBlockKind::Item(_)).then_some(
                        frame.closed_children.had_child || index + 1 < base_output.frames.len(),
                    )
                })
                .expect("case has an Item frame");
            assert_eq!(actual, expected, "precondition failed for {}", case.name);
        }

        let left_output = base_output.clone();
        let mut right_output = mutate_compatible_output_accumulators(&base_output);
        let current = right_output.current_frame;
        let line_local_can_diverge = right_output.deferred.blank_gap
            && matches!(
                right_output.frames[current].kind,
                DirectBlockKind::Document | DirectBlockKind::List(_) | DirectBlockKind::Item(_)
            );
        if line_local_can_diverge {
            right_output.frames[current].last_line_blank =
                !right_output.frames[current].last_line_blank;
        }
        assert_eq!(
            grammar_projected_from_output(&left_output),
            grammar,
            "left pre-line grammar crossed for {}",
            case.name
        );
        assert_eq!(
            grammar_projected_from_output(&right_output),
            grammar,
            "right pre-line grammar crossed for {}",
            case.name
        );
        assert_ne!(
            left_output, right_output,
            "outputs did not diverge for {}",
            case.name
        );
        if line_local_can_diverge {
            assert_ne!(
                left_output.frames[current].last_line_blank,
                right_output.frames[current].last_line_blank,
                "reachable current-frame blankness did not diverge for {}",
                case.name
            );
        }

        let mut left = DirectValueBlockParser::resume_restart_parts(&grammar, left_output, cursor)
            .expect("left divergent output resumes");
        let mut right =
            DirectValueBlockParser::resume_restart_parts(&grammar, right_output, cursor)
                .expect("right divergent output resumes");
        let mut left_commands = Vec::new();
        let mut right_commands = Vec::new();
        drive_line(&mut left, case.next, &mut left_commands);
        drive_line(&mut right, case.next, &mut right_commands);
        if case.expect_close {
            assert!(
                left_commands
                    .iter()
                    .any(|command| matches!(command, DirectCommand::Close { .. })),
                "left path did not close on first lookahead for {}",
                case.name
            );
            assert!(
                right_commands
                    .iter()
                    .any(|command| matches!(command, DirectCommand::Close { .. })),
                "right path did not close on first lookahead for {}",
                case.name
            );
        }

        let (left_grammar, left_output) = left
            .capture_restart_parts()
            .expect("left post-line boundary captures");
        let (right_grammar, right_output) = right
            .capture_restart_parts()
            .expect("right post-line boundary captures");
        assert_eq!(
            left_grammar, right_grammar,
            "identical line changed grammar/control compatibility for {}",
            case.name
        );
        assert_eq!(
            left_output.frames.len(),
            right_output.frames.len(),
            "post-line paths are not aligned for {}",
            case.name
        );
        for (depth, (left_frame, right_frame)) in left_output
            .frames
            .iter()
            .zip(right_output.frames.iter())
            .enumerate()
        {
            assert_eq!(
                left_frame.last_line_blank, right_frame.last_line_blank,
                "surviving frame-local blankness did not stabilize at depth {depth} for {}",
                case.name
            );
        }
    }
}

#[test]
fn grammar_affecting_output_mutations_are_rejected() {
    let (bullet_grammar, mut bullet_output, bullet_cursor) = restart_parts_after(&["- alpha\n"]);
    let bullet = bullet_output
        .frames
        .iter_mut()
        .find_map(|frame| match &mut frame.kind {
            DirectBlockKind::List(facts) => Some(facts),
            _ => None,
        })
        .expect("bullet list frame");
    bullet.bullet_char = if bullet.bullet_char == b'-' {
        b'+'
    } else {
        b'-'
    };
    assert!(
        !bullet_grammar
            .is_future_grammar_compatible(&grammar_projected_from_output(&bullet_output))
    );
    assert!(
        DirectValueBlockParser::resume_restart_parts(&bullet_grammar, bullet_output, bullet_cursor)
            .is_err()
    );

    let (ordered_grammar, ordered_output, ordered_cursor) = restart_parts_after(&["1. alpha\n"]);
    let mut changed_delimiter = ordered_output.clone();
    let list = changed_delimiter
        .frames
        .iter_mut()
        .find_map(|frame| match &mut frame.kind {
            DirectBlockKind::List(facts) => Some(facts),
            _ => None,
        })
        .expect("ordered list frame");
    list.delimiter = match list.delimiter {
        ListDelimiter::Period => ListDelimiter::Paren,
        ListDelimiter::Paren => ListDelimiter::Period,
    };
    assert!(
        DirectValueBlockParser::resume_restart_parts(
            &ordered_grammar,
            changed_delimiter,
            ordered_cursor
        )
        .is_err()
    );

    let mut changed_indent = ordered_output;
    let item = changed_indent
        .frames
        .iter_mut()
        .find_map(|frame| match &mut frame.kind {
            DirectBlockKind::Item(facts) => Some(facts),
            _ => None,
        })
        .expect("ordered item frame");
    assert!(item.padding < 14);
    item.padding += 1;
    assert!(
        DirectValueBlockParser::resume_restart_parts(
            &ordered_grammar,
            changed_indent,
            ordered_cursor
        )
        .is_err()
    );
}

#[test]
fn grammar_affecting_child_fence_and_deferred_mutations_are_rejected() {
    let (empty_item_grammar, mut empty_item_output, empty_item_cursor) =
        restart_parts_after(&["- \n"]);
    let terminal_item = empty_item_output
        .frames
        .last_mut()
        .expect("empty item path is nonempty");
    assert!(matches!(terminal_item.kind, DirectBlockKind::Item(_)));
    assert!(!terminal_item.closed_children.had_child);
    terminal_item.closed_children.had_child = true;
    assert!(
        DirectValueBlockParser::resume_restart_parts(
            &empty_item_grammar,
            empty_item_output,
            empty_item_cursor
        )
        .is_err()
    );

    let (fence_grammar, mut fence_output, fence_cursor) = restart_parts_after(&["```lang\n"]);
    let fence = fence_output
        .frames
        .iter_mut()
        .find_map(|frame| match &mut frame.kind {
            DirectBlockKind::FencedCode(facts) => Some(facts),
            _ => None,
        })
        .expect("fenced-code frame");
    fence.minimum_closing_length += 1;
    assert!(
        DirectValueBlockParser::resume_restart_parts(&fence_grammar, fence_output, fence_cursor)
            .is_err()
    );

    let (deferred_grammar, mut deferred_output, deferred_cursor) =
        restart_parts_after(&["alpha\n"]);
    assert!(deferred_output.deferred.terminator);
    deferred_output.deferred = DirectDeferredState::default();
    assert!(
        DirectValueBlockParser::resume_restart_parts(
            &deferred_grammar,
            deferred_output,
            deferred_cursor
        )
        .is_err()
    );
}

#[test]
fn crossed_restart_output_profile_path_count_and_current_frame_are_rejected() {
    let (grammar, output, cursor) = restart_parts_after(&["> - alpha\n"]);

    let mut crossed_profile = output.clone();
    crossed_profile.profile = SyntaxProfile::Gfm;
    assert!(
        DirectValueBlockParser::resume_restart_parts(&grammar, crossed_profile, cursor).is_err()
    );

    let (_, crossed_path, _) = restart_parts_after(&["> > > alpha\n"]);
    assert_eq!(crossed_path.frames.len(), output.frames.len());
    assert_eq!(crossed_path.current_frame, output.current_frame);
    assert!(DirectValueBlockParser::resume_restart_parts(&grammar, crossed_path, cursor).is_err());

    let mut crossed_count = output.clone();
    let mut shorter = crossed_count.frames.into_vec();
    shorter.pop();
    crossed_count.frames = shorter.into_boxed_slice();
    assert!(DirectValueBlockParser::resume_restart_parts(&grammar, crossed_count, cursor).is_err());

    let mut crossed_current = output;
    crossed_current.current_frame -= 1;
    assert!(
        DirectValueBlockParser::resume_restart_parts(&grammar, crossed_current, cursor).is_err()
    );
}

#[test]
fn long_open_list_full_recipe_inequality_is_a_grammar_false_negative_witness() {
    let (mut old, mut old_commands) = started();
    let (mut current, mut current_commands) = started();
    drive_line(&mut old, "1. alpha\n", &mut old_commands);
    drive_line(&mut current, "7. alpha\n", &mut current_commands);
    old_commands.clear();
    current_commands.clear();
    for _ in 0..4096 {
        drive_line(&mut old, "   continuation\n", &mut old_commands);
        drive_line(&mut current, "   continuation\n", &mut current_commands);
        old_commands.clear();
        current_commands.clear();
    }

    let old_pause = old
        .capture_line_boundary_pause()
        .expect("old long list pauses");
    let current_pause = current
        .capture_line_boundary_pause()
        .expect("current long list pauses");
    assert_eq!(old_pause.cursor, current_pause.cursor);
    assert_ne!(old_pause, current_pause);
    let current_cursor = resume_cursor(&current_pause);
    let (old_grammar, old_output) = old_pause.into_restart_parts().expect("old pause splits");
    let (current_grammar, current_output) = current_pause
        .into_restart_parts()
        .expect("current pause splits");
    assert_ne!(old_output, current_output);
    assert!(old_grammar.is_future_grammar_compatible(&current_grammar));
    assert_compatible_output_reconstructs_exactly(&old_grammar, current_output, current_cursor);
}

#[test]
fn durable_grammar_codec_omits_revision_output_but_keeps_grammar_and_line_local_state() {
    let loose_prefix = ["- a\n", "\n", "- b\n"];
    let tight_prefix = ["- a\n", "- b\n"];
    let (_, loose_output, _) = restart_parts_after(&loose_prefix);
    let (_, tight_output, tight_cursor) = restart_parts_after(&tight_prefix);
    assert_ne!(loose_output, tight_output, "the child-fold output changed");

    let loose_full = parser_after(&loose_prefix)
        .capture_durable_line_boundary_checkpoint()
        .expect("loose full capture");
    let tight_full = parser_after(&tight_prefix)
        .capture_durable_line_boundary_checkpoint()
        .expect("tight full capture");
    assert!(
        loose_full.header() != tight_full.header()
            || !loose_full.frame_records().eq(tight_full.frame_records()),
        "legacy durable bytes retain revision-cumulative output"
    );

    let (loose_header, loose_frames) = durable_grammar_capture_after(&loose_prefix);
    let (tight_header, tight_frames) = durable_grammar_capture_after(&tight_prefix);
    assert_eq!(loose_header, tight_header);
    assert_eq!(loose_frames, tight_frames);
    let (loose_grammar, loose_line_local) =
        DirectValueBlockParser::decode_durable_grammar_restart_parts(loose_header, loose_frames)
            .expect("loose grammar sample decodes");
    let (tight_grammar, tight_line_local) =
        DirectValueBlockParser::decode_durable_grammar_restart_parts(tight_header, tight_frames)
            .expect("tight grammar sample decodes");
    assert!(loose_grammar.is_future_grammar_compatible(&tight_grammar));
    assert_eq!(loose_line_local, tight_line_local);

    let rebound = loose_grammar
        .bind_current_restart_output_from_stabilized_line(
            loose_line_local,
            restart_frame_outputs(&tight_output),
        )
        .expect("current tight output binds to retained grammar");
    assert_compatible_output_reconstructs_exactly(&loose_grammar, rebound, tight_cursor);

    let (changed_header, changed_frames) = durable_grammar_capture_after(&["+ a\n", "+ b\n"]);
    let (changed_grammar, _) = DirectValueBlockParser::decode_durable_grammar_restart_parts(
        changed_header,
        changed_frames,
    )
    .expect("changed grammar sample decodes");
    assert!(!loose_grammar.is_future_grammar_compatible(&changed_grammar));
}

#[test]
fn durable_grammar_equality_ignores_but_codec_preserves_line_local_blankness() {
    let retained_prefix = ["\n", "- \n", "  \n"];
    let current_prefix = ["alpha\n", "\n", "- \n"];
    let (retained_header, retained_frames) = durable_grammar_capture_after(&retained_prefix);
    let (current_header, current_frames) = durable_grammar_capture_after(&current_prefix);
    let (retained_grammar, retained_line_local) =
        DirectValueBlockParser::decode_durable_grammar_restart_parts(
            retained_header,
            retained_frames.clone(),
        )
        .expect("retained sample decodes");
    let (current_grammar, current_line_local) =
        DirectValueBlockParser::decode_durable_grammar_restart_parts(
            current_header,
            current_frames.clone(),
        )
        .expect("current sample decodes");

    assert!(retained_grammar.is_future_grammar_compatible(&current_grammar));
    assert_ne!(retained_frames, current_frames);
    assert_ne!(retained_line_local, current_line_local);
    // Neither opaque line-local value is temporal authority, and equal grammar
    // is not enough to converge at this boundary. The runtime must continue
    // parsing until the complete narrow codec (grammar plus line-local state)
    // is exact; only then can unchanged-suffix induction begin.
}

#[test]
fn differing_line_local_blankness_can_change_the_identical_next_line_close_stream() {
    let mut retained = parser_after(&["\n", "- \n", "  \n"]);
    let mut current = parser_after(&["alpha\n", "\n", "- \n"]);
    let (retained_grammar, _) = retained
        .capture_restart_parts()
        .expect("retained boundary captures");
    let (current_grammar, _) = current
        .capture_restart_parts()
        .expect("current boundary captures");
    assert!(retained_grammar.is_future_grammar_compatible(&current_grammar));

    let mut retained_commands = Vec::new();
    let mut current_commands = Vec::new();
    drive_line(&mut retained, "outside\n", &mut retained_commands);
    drive_line(&mut current, "outside\n", &mut current_commands);

    assert_ne!(
        retained_commands, current_commands,
        "line-local blankness must be part of C convergence if a later close observes it"
    );
    let (retained_header, retained_frames) = durable_grammar_capture(&retained);
    let (current_header, current_frames) = durable_grammar_capture(&current);
    assert_eq!(retained_header, current_header);
    assert_eq!(
        retained_frames, current_frames,
        "after parsing the differing close commands, one identical line stabilizes the complete narrow continuation codec"
    );
}

#[test]
fn grammar_and_line_local_suffix_state_converges_by_one_step_induction() {
    let mut old = parser_after(&["1. alpha\n"]);
    let mut current = parser_after(&["7. alpha\n"]);
    let (old_grammar, old_output) = old.capture_restart_parts().expect("old prefix captures");
    let (current_grammar, current_output) = current
        .capture_restart_parts()
        .expect("current prefix captures");
    assert_ne!(
        old_output, current_output,
        "ordered start is current output"
    );
    assert!(old_grammar.is_future_grammar_compatible(&current_grammar));

    let mut old_commands = Vec::new();
    let mut current_commands = Vec::new();
    for identical_suffix_line in ["   continuation\n", "   \n"] {
        drive_line(&mut old, identical_suffix_line, &mut old_commands);
        drive_line(&mut current, identical_suffix_line, &mut current_commands);
        old_commands.clear();
        current_commands.clear();

        let (old_header, old_frames) = durable_grammar_capture(&old);
        let (current_header, current_frames) = durable_grammar_capture(&current);
        assert_eq!(old_header, current_header);
        assert_eq!(old_frames, current_frames);
        let (old_grammar, old_line_local) =
            DirectValueBlockParser::decode_durable_grammar_restart_parts(old_header, old_frames)
                .expect("old induced sample decodes");
        let (current_grammar, current_line_local) =
            DirectValueBlockParser::decode_durable_grammar_restart_parts(
                current_header,
                current_frames,
            )
            .expect("current induced sample decodes");
        assert_eq!(old_grammar, current_grammar);
        assert_eq!(old_line_local, current_line_local);
    }
}

#[test]
fn durable_split_checkpoint_is_constant_per_sample_and_depth_shared_path_sized() {
    let deep_line = format!("{}alpha\n", "> ".repeat(256));
    let mut uninterrupted = parser_after(&[&deep_line]);
    let pause = uninterrupted
        .capture_line_boundary_pause()
        .expect("depth-258 prefix pauses");
    let capture = uninterrupted
        .capture_durable_line_boundary_checkpoint()
        .expect("depth-258 prefix captures");
    let second_capture = uninterrupted
        .capture_durable_line_boundary_checkpoint()
        .expect("unchanged depth-258 prefix captures again");
    let receipt = capture.receipt();
    assert_eq!(receipt.sample_header_bytes, 64);
    assert_eq!(receipt.materialized_path_records, 258);
    assert_eq!(receipt.materialized_path_bytes, 258 * 48);
    assert!(receipt.materialized_path_bytes < 32 * 1024);
    assert!(640 * receipt.sample_header_bytes < 160 * 1024);
    assert_eq!(receipt.retained_source_bytes, 0);

    let records = capture.frame_records().collect::<Vec<_>>();
    let second_records = second_capture.frame_records().collect::<Vec<_>>();
    assert_eq!(
        records, second_records,
        "unchanged paths are byte-shareable"
    );
    assert_eq!(capture.header(), second_capture.header());

    let mut resumed = DirectValueBlockParser::resume_durable_line_boundary_checkpoint(
        capture.header(),
        records,
        resume_cursor(&pause),
    )
    .expect("depth-258 durable checkpoint resumes");
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    finish(&mut uninterrupted, &mut expected);
    finish(&mut resumed, &mut actual);
    assert_eq!(actual, expected);
}

#[test]
fn bare_eof_paragraph_has_no_synthetic_terminator_after_resume() {
    let pause = pause_after(&["alpha"]);
    assert!(!pause.deferred.terminator);
    let commands = assert_suffix_resume_exact(&["alpha"], &[]);
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, DirectCommand::ResolveTerminator { .. }))
    );
}

#[test]
fn root_and_marked_blank_gaps_keep_exact_owner_depth_across_resume() {
    let root = pause_after(&["\n"]);
    assert!(root.deferred.blank_gap);
    assert_eq!(root.deferred.blank_gap_floor, None);
    let root_commands = assert_suffix_resume_exact(&["\n"], &["alpha"]);
    assert!(root_commands.contains(&DirectCommand::ResolveBlankGap {
        owner: DirectOwner::TOP,
    }));

    let quoted = pause_after(&["> a\n", "> \n"]);
    assert!(quoted.deferred.blank_gap);
    assert_eq!(quoted.deferred.blank_gap_floor, Some(1));
    let quoted_commands = assert_suffix_resume_exact(&["> a\n", "> \n"], &["- b\n"]);
    assert!(quoted_commands.contains(&DirectCommand::ResolveBlankGap {
        owner: DirectOwner::TOP,
    }));

    let nested = pause_after(&["> > a\n", "> > \n"]);
    assert_eq!(nested.deferred.blank_gap_floor, Some(2));
    let nested_commands = assert_suffix_resume_exact(&["> > a\n", "> > \n"], &[]);
    assert!(nested_commands.contains(&DirectCommand::ResolveBlankGap {
        owner: DirectOwner::TOP,
    }));
}

#[test]
fn pairing_view_borrows_exact_path_cursor_and_deferred_role_without_copying() {
    let paragraph = pause_after(&["> - alpha\r\n"]);
    let view = paragraph.pairing_view();
    assert_eq!(view.profile(), SyntaxProfile::CommonMark);
    assert_eq!(view.line_number(), 1);
    assert_eq!(view.last_line_length(), "> - alpha".len());
    assert_eq!(
        view.open_frame_count(),
        paragraph.receipt().retained_open_frames
    );
    let kinds = view.open_kinds().collect::<Vec<_>>();
    assert_eq!(kinds.first(), Some(&DirectBlockKind::Document));
    assert!(matches!(kinds.get(1), Some(DirectBlockKind::BlockQuote)));
    assert!(matches!(kinds.get(2), Some(DirectBlockKind::List(_))));
    assert!(matches!(kinds.get(3), Some(DirectBlockKind::Item(_))));
    assert_eq!(kinds.last(), Some(&DirectBlockKind::Paragraph));
    assert_eq!(view.current_frame_depth(), kinds.len() - 1);
    assert_eq!(
        view.deferred_role(),
        DirectLineBoundaryDeferredRole::Terminator
    );
    let (paragraph_grammar, _) = paragraph
        .into_restart_parts()
        .expect("paragraph pause splits into restart parts");
    assert_eq!(
        paragraph_grammar.deferred_role(),
        DirectLineBoundaryDeferredRole::Terminator
    );

    let blank = pause_after(&["> a\n", "> \n"]);
    assert_eq!(
        blank.pairing_view().deferred_role(),
        DirectLineBoundaryDeferredRole::BlankGap {
            floor_depth: Some(1)
        }
    );
    let (blank_grammar, _) = blank
        .into_restart_parts()
        .expect("blank-gap pause splits into restart parts");
    assert_eq!(
        blank_grammar.deferred_role(),
        DirectLineBoundaryDeferredRole::BlankGap {
            floor_depth: Some(1)
        }
    );
    let closed = pause_after(&["alpha\n", "\n"]);
    assert_eq!(
        closed.pairing_view().deferred_role(),
        DirectLineBoundaryDeferredRole::BlankGap { floor_depth: None }
    );
    let (closed_grammar, _) = closed
        .into_restart_parts()
        .expect("root blank-gap pause splits into restart parts");
    assert_eq!(
        closed_grammar.deferred_role(),
        DirectLineBoundaryDeferredRole::BlankGap { floor_depth: None }
    );

    let none = pause_after(&["```\n"]);
    assert_eq!(
        none.pairing_view().deferred_role(),
        DirectLineBoundaryDeferredRole::None
    );
    let (none_grammar, _) = none
        .into_restart_parts()
        .expect("empty pause splits into restart parts");
    assert_eq!(
        none_grammar.deferred_role(),
        DirectLineBoundaryDeferredRole::None
    );
}

#[test]
fn list_child_folds_survive_repeated_node_id_reconstruction() {
    let prefix = ["- a\n", "\n", "- b\n"];
    let pause = pause_after(&prefix);
    assert!(matches!(pause.frames[1].kind, DirectBlockKind::List(_)));
    assert!(pause.frames[1].closed_children.had_child);
    let commands = assert_suffix_resume_exact(&prefix, &["  continuation\n"]);
    assert!(commands.iter().any(|command| {
        matches!(
            command,
            DirectCommand::Close {
                kind: DirectBlockKind::List(_),
                final_facts: DirectFinalFacts::List { tight: false },
                ..
            }
        )
    }));

    let cases: &[&[&str]] = &[
        &["- a\n", "- b\n"],
        &["- a\n", "\n", "- b\n"],
        &["> - a\r\n", ">   continuation\n", "> \n", "> - b\n"],
    ];
    for lines in cases {
        assert_eq!(
            complete_commands(lines, true),
            complete_commands(lines, false),
            "pause/resume after every list line changed commands for {lines:?}"
        );
    }
}

#[test]
fn fenced_code_continuation_restores_exact_closer_facts_without_reopening() {
    let prefix = [">   ````lang\r\n", ">  body\n"];
    let pause = pause_after(&prefix);
    assert!(matches!(
        pause.frames.last().map(|frame| frame.kind),
        Some(DirectBlockKind::FencedCode(DirectFencedCodeFacts {
            fence: DirectFenceCharacter::Backtick,
            minimum_closing_length: 4,
            fence_offset_columns: 2,
        }))
    ));
    let commands = assert_suffix_resume_exact(&prefix, &[">   ````  \n", "tail"]);
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, DirectCommand::MarkFencedCodeBoundary { .. }))
    );
    assert!(commands.iter().any(|command| {
        matches!(
            command,
            DirectCommand::Close {
                kind: DirectBlockKind::FencedCode(_),
                final_facts: DirectFinalFacts::FencedCode(DirectFencedCodeCloseFacts {
                    closed: true
                }),
                ..
            }
        )
    }));

    let bare_prefix = ["~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~lang\n", "payload"];
    let bare = pause_after(&bare_prefix);
    assert!(matches!(
        bare.frames.last().map(|frame| frame.kind),
        Some(DirectBlockKind::FencedCode(DirectFencedCodeFacts {
            fence: DirectFenceCharacter::Tilde,
            minimum_closing_length: 32,
            fence_offset_columns: 0,
        }))
    ));
    let bare_commands = assert_suffix_resume_exact(&bare_prefix, &[]);
    assert!(bare_commands.iter().any(|command| {
        matches!(
            command,
            DirectCommand::Close {
                final_facts: DirectFinalFacts::FencedCode(DirectFencedCodeCloseFacts {
                    closed: false
                }),
                ..
            }
        )
    }));
}

#[test]
fn line_cursor_prevents_a_resumed_mid_document_bom_from_becoming_initial() {
    let pause = pause_after(&["\u{feff}alpha\n"]);
    assert_eq!(pause.cursor.line_number, 1);
    let second = "\u{feff}beta";
    let commands = assert_suffix_resume_exact(&["\u{feff}alpha\n"], &[second]);
    assert!(commands.iter().any(|command| {
        matches!(
            command,
            DirectCommand::Consume {
                part: DirectCoveragePart::Content,
                range,
                logical: DirectLogicalAction::CanonicalText,
                ..
            } if *range == (0..u32::try_from(second.len()).expect("test line fits u32"))
        )
    }));
}

#[test]
fn setext_stale_paragraph_hook_is_derived_instead_of_serialized() {
    let mut uninterrupted = parser_after(&["alpha\n", "===\n"]);
    assert!(
        uninterrupted
            .parser
            .direct
            .as_ref()
            .expect("direct hooks")
            .paragraph_has_content,
        "the donor hook is intentionally stale after promotion"
    );
    let pause = uninterrupted
        .capture_line_boundary_pause()
        .expect("Setext boundary pauses");
    assert!(matches!(
        pause.frames.last().map(|frame| frame.kind),
        Some(DirectBlockKind::Heading(_))
    ));
    let mut resumed =
        DirectValueBlockParser::resume_line_boundary_pause(pause).expect("Setext boundary resumes");
    assert!(
        !resumed
            .parser
            .direct
            .as_ref()
            .expect("direct hooks")
            .paragraph_has_content
    );

    let mut expected = Vec::new();
    let mut actual = Vec::new();
    drive_line(&mut uninterrupted, "tail", &mut expected);
    drive_line(&mut resumed, "tail", &mut actual);
    finish(&mut uninterrupted, &mut expected);
    finish(&mut resumed, &mut actual);
    assert_eq!(actual, expected);
}

#[test]
fn capture_rejects_nonboundary_and_nonbounded_internal_state() {
    let mut pending_open =
        DirectValueBlockParser::new(SyntaxProfile::CommonMark).expect("direct parser starts");
    assert!(pending_open.capture_line_boundary_pause().is_err());
    pending_open
        .acknowledge_command()
        .expect("document open acknowledges");
    assert!(pending_open.capture_line_boundary_pause().is_err());
    pending_open
        .begin_line("alpha\n".to_owned())
        .expect("line starts");
    assert!(pending_open.capture_line_boundary_pause().is_err());

    let mut content_tamper = parser_after(&["alpha\n"]);
    content_tamper
        .parser
        .tree
        .node_mut(NodeId(0))
        .content
        .line_offsets
        .push(0);
    assert!(content_tamper.capture_line_boundary_pause().is_err());

    let mut table_tamper = parser_after(&["alpha\n"]);
    table_tamper.parser.tree.node_mut(NodeId(0)).table_visited = true;
    assert!(table_tamper.capture_line_boundary_pause().is_err());

    let mut stack_tamper = parser_after(&["alpha\n"]);
    stack_tamper
        .parser
        .direct
        .as_mut()
        .expect("direct hooks")
        .emission_stack[1] = NodeId(99);
    assert!(stack_tamper.capture_line_boundary_pause().is_err());

    let mut topology_tamper = parser_after(&["alpha\n"]);
    topology_tamper
        .parser
        .tree
        .node_mut(NodeId(0))
        .children
        .clear();
    assert!(topology_tamper.capture_line_boundary_pause().is_err());

    let mut finishing = parser_after(&["alpha\n"]);
    finishing.begin_finish().expect("finish starts");
    assert!(finishing.capture_line_boundary_pause().is_err());

    let mut finished = parser_after(&["alpha\n"]);
    finish(&mut finished, &mut Vec::new());
    assert!(finished.capture_line_boundary_pause().is_err());
}

fn assert_resume_rejects(pause: DirectLineBoundaryPause) {
    assert!(DirectValueBlockParser::resume_line_boundary_pause(pause).is_err());
}

#[test]
fn resume_rejects_tampered_header_path_and_deferred_state() {
    let paragraph = pause_after(&["alpha\n"]);

    let mut bad_schema = paragraph.clone();
    bad_schema.schema += 1;
    assert_resume_rejects(bad_schema);

    let mut bad_profile = paragraph.clone();
    bad_profile.profile = SyntaxProfile::Gfm;
    assert_resume_rejects(bad_profile);

    let mut no_frames = paragraph.clone();
    no_frames.frames = Vec::new().into_boxed_slice();
    assert_resume_rejects(no_frames);

    let mut bad_current = paragraph.clone();
    bad_current.current_frame = bad_current.frames.len();
    assert_resume_rejects(bad_current);

    let mut nested_document = paragraph.clone();
    nested_document.frames[1].kind = DirectBlockKind::Document;
    assert_resume_rejects(nested_document);

    let mut floor_without_gap = paragraph.clone();
    floor_without_gap.deferred.blank_gap_floor = Some(0);
    assert_resume_rejects(floor_without_gap);

    let mut two_deferred_roles = paragraph.clone();
    two_deferred_roles.deferred.blank_gap = true;
    assert_resume_rejects(two_deferred_roles);

    let mut terminator_without_paragraph = pause_after(&["\n"]);
    terminator_without_paragraph.deferred.terminator = true;
    terminator_without_paragraph.deferred.blank_gap = false;
    terminator_without_paragraph.deferred.blank_gap_floor = None;
    assert_resume_rejects(terminator_without_paragraph);

    let mut bad_floor_owner = pause_after(&["> a\n", "> \n"]);
    bad_floor_owner.deferred.blank_gap_floor = Some(0);
    assert_resume_rejects(bad_floor_owner);
}

#[test]
fn source_positions_are_proven_absent_from_direct_control_state() {
    let mut parser = parser_after(&["> - alpha\n"]);
    let canonical = parser
        .capture_line_boundary_pause()
        .expect("canonical positions pause");
    for node in &mut parser.parser.tree.nodes {
        node.source_start = Position::new(90_000, 70_000);
        node.source_end = Position::new(80_000, 60_000);
    }
    let shifted = parser
        .capture_line_boundary_pause()
        .expect("shifted positions pause");
    assert_eq!(shifted, canonical);

    let mut canonical_resume = DirectValueBlockParser::resume_line_boundary_pause(canonical)
        .expect("canonical pause resumes");
    let mut shifted_resume =
        DirectValueBlockParser::resume_line_boundary_pause(shifted).expect("shifted pause resumes");
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    drive_line(&mut canonical_resume, ">   beta", &mut expected);
    drive_line(&mut shifted_resume, ">   beta", &mut actual);
    finish(&mut canonical_resume, &mut expected);
    finish(&mut shifted_resume, &mut actual);
    assert_eq!(actual, expected);
}

#[test]
fn repeated_resume_is_exact_across_mixed_supported_transitions() {
    let cases: &[&[&str]] = &[
        &["alpha\r\n", "beta\n", "\n", "omega"],
        &["> - alpha\r\n", ">   continuation\n", "> \n", "> - beta\n"],
        &["````rust\n", "body\r", "````\n", "tail\n", "===\n"],
        &["> > a\n", "> > \n", "> > b"],
    ];
    for lines in cases {
        assert_eq!(
            complete_commands(lines, true),
            complete_commands(lines, false),
            "pause/resume after every line changed commands for {lines:?}"
        );
    }
}

#[test]
fn pause_scale_is_open_depth_not_prefix_or_leaf_payload() {
    let short = pause_after(&["x\n"]);
    let long_line = format!("{}\n", "x".repeat(8_000));
    let long = pause_after(&[&long_line]);
    assert_eq!(long.receipt(), short.receipt());

    let (mut many, mut ignored) = started();
    let mut prefix_bytes = 0;
    for _ in 0..512 {
        for line in ["paragraph\n", "\n"] {
            prefix_bytes += line.len();
            drive_line(&mut many, line, &mut ignored);
        }
    }
    let many_pause = many
        .capture_line_boundary_pause()
        .expect("long closed prefix pauses");
    let one_closed = pause_after(&["paragraph\n", "\n"]);
    assert_eq!(many_pause.receipt(), one_closed.receipt());

    let deep_line = format!("{}x\n", "> ".repeat(64));
    let deep = pause_after(&[&deep_line]);
    let shallow_receipt = short.receipt();
    let deep_receipt = deep.receipt();
    assert_eq!(shallow_receipt.retained_open_frames, 2);
    assert_eq!(deep_receipt.retained_open_frames, 66);
    assert_eq!(shallow_receipt.retained_source_bytes, 0);
    assert_eq!(deep_receipt.retained_source_bytes, 0);
    assert_eq!(
        deep_receipt.estimated_owned_bytes - shallow_receipt.estimated_owned_bytes,
        (deep_receipt.retained_open_frames - shallow_receipt.retained_open_frames)
            * std::mem::size_of::<DirectPauseFrame>()
    );
    eprintln!(
        "DIRECT_PAUSE_SCALE prefix_bytes={prefix_bytes} shallow={shallow_receipt:?} \
         many={:?} deep={deep_receipt:?}",
        many_pause.receipt()
    );
}

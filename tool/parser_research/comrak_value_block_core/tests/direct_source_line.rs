use std::fs;
use std::path::PathBuf;

use flark_comrak_value_block_core::{
    DIRECT_SEGMENTED_LINE_WINDOW_BYTES, DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES,
    DirectBlockKind, DirectCommand, DirectExternalWork, DirectPollStatus,
    DirectReferenceDefinition, DirectReferencePrefixCommitStatus,
    DirectReferencePrefixOutputAckStatus, DirectReferencePrefixPollStatus,
    DirectReferencePrefixSource, DirectReferencePrefixWork, DirectSourceLinePollError,
    DirectSourceLinePollStatus, DirectSourceLineSource, DirectSourceLineWork,
    DirectValueBlockParser, ParseError, SyntaxProfile,
};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceIdentity {
    revision: u64,
    root: u64,
    build: u64,
    line: u64,
    start: usize,
    end: usize,
}

impl SourceIdentity {
    const fn line(len: usize) -> Self {
        Self {
            revision: 11,
            root: 23,
            build: 37,
            line: 0,
            start: 0,
            end: len,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceError {
    Injected,
    Budget,
    NonSequential { requested: usize, expected: usize },
}

struct StrictSource {
    identity: SourceIdentity,
    bytes: Vec<u8>,
    next: usize,
    budget: usize,
    fail_at: Option<usize>,
    reported_len: Option<usize>,
}

struct RepeatedParagraphSource {
    identity: SourceIdentity,
    content_bytes: usize,
    next: usize,
    budget: usize,
}

impl RepeatedParagraphSource {
    fn new(content_bytes: usize) -> Self {
        let physical_bytes = content_bytes + 2;
        Self {
            identity: SourceIdentity::line(physical_bytes),
            content_bytes,
            next: 0,
            budget: 0,
        }
    }

    fn grant(&mut self, budget: usize) {
        self.budget = budget;
    }

    const fn physical_bytes(&self) -> usize {
        self.content_bytes + 2
    }
}

impl DirectSourceLineSource for RepeatedParagraphSource {
    type Identity = SourceIdentity;
    type Error = SourceError;

    fn identity(&self) -> Self::Identity {
        self.identity
    }

    fn len(&self) -> usize {
        self.physical_bytes()
    }

    fn access_budget(&self) -> usize {
        self.budget
    }

    fn read_byte(&mut self, absolute_offset: usize) -> Result<u8, Self::Error> {
        if self.budget == 0 {
            return Err(SourceError::Budget);
        }
        if absolute_offset != self.next {
            return Err(SourceError::NonSequential {
                requested: absolute_offset,
                expected: self.next,
            });
        }
        let byte = if absolute_offset < self.content_bytes {
            b'a'
        } else if absolute_offset == self.content_bytes {
            b'\r'
        } else if absolute_offset == self.content_bytes + 1 {
            b'\n'
        } else {
            return Err(SourceError::Injected);
        };
        self.next += 1;
        self.budget -= 1;
        Ok(byte)
    }
}

impl StrictSource {
    fn new(identity: SourceIdentity, bytes: Vec<u8>) -> Self {
        Self {
            identity,
            bytes,
            next: 0,
            budget: 0,
            fail_at: None,
            reported_len: None,
        }
    }

    fn grant(&mut self, budget: usize) {
        self.budget = budget;
    }
}

impl DirectSourceLineSource for StrictSource {
    type Identity = SourceIdentity;
    type Error = SourceError;

    fn identity(&self) -> Self::Identity {
        self.identity
    }

    fn len(&self) -> usize {
        self.reported_len.unwrap_or(self.bytes.len())
    }

    fn access_budget(&self) -> usize {
        self.budget
    }

    fn read_byte(&mut self, absolute_offset: usize) -> Result<u8, Self::Error> {
        if self.budget == 0 {
            return Err(SourceError::Budget);
        }
        if absolute_offset != self.next {
            return Err(SourceError::NonSequential {
                requested: absolute_offset,
                expected: self.next,
            });
        }
        if self.fail_at == Some(absolute_offset) {
            return Err(SourceError::Injected);
        }
        self.next += 1;
        self.budget -= 1;
        Ok(self.bytes[absolute_offset])
    }
}

struct LogicalSource<'a> {
    identity: u64,
    bytes: &'a [u8],
    next: usize,
    budget: usize,
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
        self.budget
    }

    fn read_byte(&mut self, relative_offset: usize) -> Result<u8, Self::Error> {
        if relative_offset != self.next || self.budget == 0 {
            return Err(SourceError::NonSequential {
                requested: relative_offset,
                expected: self.next,
            });
        }
        let byte = *self
            .bytes
            .get(relative_offset)
            .ok_or(SourceError::Injected)?;
        self.next += 1;
        self.budget -= 1;
        Ok(byte)
    }

    fn raw_codepoint_contribution(&self, _logical_scalar_end_offset: usize) -> u8 {
        1
    }
}

fn started() -> (DirectValueBlockParser, Vec<DirectCommand>) {
    let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
    let open = parser.pending_command().unwrap().clone();
    assert_eq!(
        open,
        DirectCommand::Open {
            kind: DirectBlockKind::Document,
        }
    );
    parser.acknowledge_command().unwrap();
    (parser, vec![open])
}

fn drain_line(parser: &mut DirectValueBlockParser) -> Vec<DirectCommand> {
    let mut commands = Vec::new();
    for _ in 0..128 {
        let receipt = parser.poll_line(1).unwrap();
        assert!(receipt.transitions <= 1);
        match receipt.status {
            DirectPollStatus::Pending => {}
            DirectPollStatus::CommandReady => {
                commands.push(parser.pending_command().unwrap().clone());
                parser.acknowledge_command().unwrap();
            }
            DirectPollStatus::ExternalWorkReady => {
                panic!("source-line ATX proof cannot request reference work")
            }
            DirectPollStatus::Complete => return commands,
        }
    }
    panic!("direct line commands did not converge");
}

fn drive_finish(
    parser: &mut DirectValueBlockParser,
    commands: &mut Vec<DirectCommand>,
) -> DirectPollStatus {
    for _ in 0..10_000 {
        let receipt = parser.poll_finish(1).unwrap();
        match receipt.status {
            DirectPollStatus::Pending => {}
            DirectPollStatus::CommandReady => {
                commands.push(parser.pending_command().unwrap().clone());
                parser.acknowledge_command().unwrap();
            }
            DirectPollStatus::ExternalWorkReady | DirectPollStatus::Complete => {
                return receipt.status;
            }
        }
    }
    panic!("direct finish did not converge")
}

fn resolve_reference_prefix(
    parser: &mut DirectValueBlockParser,
    logical: &str,
) -> (
    DirectReferencePrefixCommitStatus,
    Vec<DirectReferenceDefinition>,
) {
    let request = match parser.pending_external_work().unwrap() {
        DirectExternalWork::ReferencePrefixFinalizer { request } => *request,
    };
    let mut work: DirectReferencePrefixWork<u64> =
        parser.begin_reference_prefix_work(request, 71).unwrap();
    let mut source = LogicalSource {
        identity: 71,
        bytes: logical.as_bytes(),
        next: 0,
        budget: 0,
    };
    let mut definitions = Vec::new();
    loop {
        source.budget = DIRECT_SEGMENTED_LINE_WINDOW_BYTES;
        let receipt = work
            .poll_source(&mut source, DIRECT_SEGMENTED_LINE_WINDOW_BYTES, false)
            .unwrap();
        assert!(receipt.inspected_bytes <= DIRECT_SEGMENTED_LINE_WINDOW_BYTES);
        match receipt.status {
            DirectReferencePrefixPollStatus::NeedMore => {}
            DirectReferencePrefixPollStatus::OutputReady => {
                let (definition, ack) = work.take_output().unwrap().acknowledge();
                definitions.push(definition);
                if work.acknowledge_output(ack).unwrap()
                    == DirectReferencePrefixOutputAckStatus::Complete
                {
                    let terminal = work.take_terminal().ok().unwrap();
                    return (
                        parser
                            .commit_reference_prefix_terminal(terminal.acknowledge(), 71)
                            .unwrap(),
                        definitions,
                    );
                }
            }
            DirectReferencePrefixPollStatus::Complete => {
                let terminal = work.take_terminal().ok().unwrap();
                return (
                    parser
                        .commit_reference_prefix_terminal(terminal.acknowledge(), 71)
                        .unwrap(),
                    definitions,
                );
            }
            DirectReferencePrefixPollStatus::Cancelled => panic!("unexpected cancellation"),
        }
    }
}

fn buffered_line(line: &str) -> (DirectValueBlockParser, Vec<DirectCommand>) {
    let (mut parser, _) = started();
    parser.begin_line(line.to_owned()).unwrap();
    let commands = drain_line(&mut parser);
    (parser, commands)
}

fn try_buffered_commands(line: &str) -> Result<Vec<DirectCommand>, ParseError> {
    let (mut parser, _) = started();
    parser.begin_line(line.to_owned())?;
    let mut commands = Vec::new();
    for _ in 0..128 {
        let receipt = parser.poll_line(1)?;
        match receipt.status {
            DirectPollStatus::Pending => {}
            DirectPollStatus::CommandReady => {
                commands.push(parser.pending_command().unwrap().clone());
                parser.acknowledge_command()?;
            }
            DirectPollStatus::ExternalWorkReady => {
                return Err(ParseError::Invariant(
                    "isolated first line does not request reference work",
                ));
            }
            DirectPollStatus::Complete => return Ok(commands),
        }
    }
    Err(ParseError::Invariant("buffered first line converges"))
}

fn poll_to_match(
    work: &mut DirectSourceLineWork<SourceIdentity>,
    source: &mut StrictSource,
    mut fuel: impl FnMut(usize) -> usize,
) -> (usize, usize) {
    let mut polls = 0;
    let mut maximum_retained = 0;
    loop {
        let next_fuel = fuel(polls).max(1);
        source.grant(4_096);
        let receipt = work.poll_source(source, next_fuel).unwrap();
        assert!(receipt.lexical_work_units <= 4_096);
        assert_eq!(receipt.maximum_source_request_rewind_bytes, 0);
        assert_eq!(receipt.physical_high_water, source.next);
        maximum_retained = maximum_retained.max(receipt.retained_source_bytes);
        polls += 1;
        match receipt.status {
            DirectSourceLinePollStatus::NeedMore => {}
            DirectSourceLinePollStatus::Matched => return (polls, maximum_retained),
        }
    }
}

fn source_line(
    line: &str,
    fuel: impl FnMut(usize) -> usize,
) -> (DirectValueBlockParser, Vec<DirectCommand>) {
    let identity = SourceIdentity::line(line.len());
    let (mut parser, _) = started();
    let mut work = parser.begin_source_line_work(identity, line.len()).unwrap();
    let mut source = StrictSource::new(identity, line.as_bytes().to_vec());
    poll_to_match(&mut work, &mut source, fuel);
    parser
        .commit_source_line(
            work,
            identity,
            u32::try_from(line.encode_utf16().count()).unwrap(),
        )
        .unwrap();
    assert!(parser.retained_line_bytes() <= DIRECT_SEGMENTED_LINE_WINDOW_BYTES);
    let commands = drain_line(&mut parser);
    (parser, commands)
}

fn try_source_commands(line: &str) -> Result<Vec<DirectCommand>, ParseError> {
    let identity = SourceIdentity::line(line.len());
    let (mut parser, _) = started();
    let mut work = parser.begin_source_line_work(identity, line.len())?;
    let mut source = StrictSource::new(identity, line.as_bytes().to_vec());
    poll_to_match(&mut work, &mut source, |_| 31);
    parser.commit_source_line(
        work,
        identity,
        u32::try_from(line.encode_utf16().count()).unwrap(),
    )?;
    let mut commands = Vec::new();
    for _ in 0..128 {
        let receipt = parser.poll_line(1)?;
        match receipt.status {
            DirectPollStatus::Pending => {}
            DirectPollStatus::CommandReady => {
                commands.push(parser.pending_command().unwrap().clone());
                parser.acknowledge_command()?;
            }
            DirectPollStatus::ExternalWorkReady => {
                return Err(ParseError::Invariant(
                    "isolated source line does not request reference work",
                ));
            }
            DirectPollStatus::Complete => return Ok(commands),
        }
    }
    Err(ParseError::Invariant("source-backed first line converges"))
}

#[test]
fn buffered_and_source_backed_atx_have_exact_commands_and_pause_parity() {
    let cases = [
        "# alpha\n",
        " # alpha\n",
        "  ## alpha\r\n",
        "   ### alpha",
        "\u{feff}# document start\n",
        "\u{feff}   #### indented\r\n",
        "### alpha ###   \r\n",
        "# alpha#   \n",
        "# 😀 ##",
        " # a\0β😀 ##\n",
        "######\tβ",
        "#   \n",
        "## alpha",
    ];
    for line in cases {
        let (buffered, buffered_commands) = buffered_line(line);
        for fixed_fuel in [1, 2, 7, 4_090] {
            let (source, source_commands) = source_line(line, |_| fixed_fuel);
            assert_eq!(source_commands, buffered_commands, "line={line:?}");
            assert_eq!(
                source.capture_line_boundary_pause().unwrap(),
                buffered.capture_line_boundary_pause().unwrap(),
                "line-boundary pause differs for {line:?}"
            );
        }
    }

    let mut rng = Lcg(0x6469_7265_6374_6174);
    for case in 0..2_000 {
        let hashes = rng.usize(6) + 1;
        let body_len = rng.usize(96);
        let mut line = "#".repeat(hashes);
        line.push(if case % 3 == 0 { '\t' } else { ' ' });
        for _ in 0..body_len {
            line.push(['a', ' ', '#', 'β', '😀'][rng.usize(5)]);
        }
        match case % 4 {
            0 => line.push('\n'),
            1 => line.push_str("\r\n"),
            2 => line.push('\r'),
            _ => {}
        }
        let (_, expected) = buffered_line(&line);
        let mut fuel_rng = Lcg(u64::try_from(case).unwrap() + 1);
        let (_, actual) = source_line(&line, |_| fuel_rng.usize(31) + 1);
        assert_eq!(actual, expected, "random line={line:?}");
    }
}

#[test]
fn ten_mib_atx_is_one_pass_bounded_and_emits_constant_commands() {
    const BODY_BYTES: usize = 10 * 1024 * 1024;
    let mut line = String::with_capacity(BODY_BYTES + 12);
    line.push_str("# ");
    line.push_str(&"a".repeat(BODY_BYTES));
    line.push_str(" ###   \r\n");
    let identity = SourceIdentity::line(line.len());
    let (mut parser, _) = started();
    let mut work = parser.begin_source_line_work(identity, line.len()).unwrap();
    let mut source = StrictSource::new(identity, line.as_bytes().to_vec());
    let (polls, maximum_retained) = poll_to_match(&mut work, &mut source, |poll| match poll % 4 {
        0 => 4_090,
        1 => 4_031,
        2 => 3_997,
        _ => 4_087,
    });
    assert!(polls > 2_500);
    assert_eq!(source.next, line.len());
    assert!(maximum_retained <= DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES);
    assert!(work.retained_source_bytes() <= DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES);

    parser
        .commit_source_line(work, identity, u32::try_from(line.len()).unwrap())
        .unwrap();
    assert_eq!(parser.retained_line_bytes(), 0);
    let commands = drain_line(&mut parser);
    assert_eq!(
        commands.len(),
        6,
        "ATX command count is line-length invariant"
    );
    assert!(matches!(
        commands.last(),
        Some(DirectCommand::FinishLine {
            physical_bytes,
            physical_utf16,
        }) if *physical_bytes as usize == line.len() && *physical_utf16 as usize == line.len()
    ));
}

#[test]
fn identity_failure_replay_and_cross_parser_admission_fail_closed() {
    let line = "# alpha ###\n";
    let identity = SourceIdentity::line(line.len());

    let (mut wrong_parser, _) = started();
    let mut wrong_work = wrong_parser
        .begin_source_line_work(identity, line.len())
        .unwrap();
    let crossed = SourceIdentity {
        build: 99,
        ..identity
    };
    let mut wrong_source = StrictSource::new(crossed, line.as_bytes().to_vec());
    wrong_source.grant(4_096);
    assert_eq!(
        wrong_work.poll_source(&mut wrong_source, 17),
        Err(DirectSourceLinePollError::WrongSource)
    );
    wrong_source.grant(4_096);
    assert_eq!(
        wrong_work.poll_source(&mut wrong_source, 17),
        Err(DirectSourceLinePollError::PollAfterFailure)
    );
    assert_eq!(wrong_parser.scratch_node_count(), 1);
    assert!(wrong_parser.pending_command().is_none());
    assert!(wrong_parser.begin_line("# later\n".to_owned()).is_err());

    let (mut partial_parser, _) = started();
    let mut partial_work = partial_parser
        .begin_source_line_work(identity, line.len())
        .unwrap();
    let mut partial_source = StrictSource::new(identity, line.as_bytes().to_vec());
    partial_source.grant(1);
    assert_eq!(
        partial_work
            .poll_source(&mut partial_source, 1)
            .unwrap()
            .status,
        DirectSourceLinePollStatus::NeedMore
    );
    partial_source.identity = crossed;
    partial_source.grant(4_096);
    assert_eq!(
        partial_work.poll_source(&mut partial_source, 17),
        Err(DirectSourceLinePollError::WrongSource)
    );
    assert_eq!(partial_source.next, 1);

    let (mut failed_parser, _) = started();
    let mut failed_work = failed_parser
        .begin_source_line_work(identity, line.len())
        .unwrap();
    let mut failed_source = StrictSource::new(identity, line.as_bytes().to_vec());
    failed_source.fail_at = Some(2);
    failed_source.grant(4_096);
    assert_eq!(
        failed_work
            .poll_source(&mut failed_source, 17)
            .unwrap()
            .status,
        DirectSourceLinePollStatus::NeedMore
    );
    failed_source.grant(4_096);
    assert_eq!(
        failed_work.poll_source(&mut failed_source, 17),
        Err(DirectSourceLinePollError::Source(SourceError::Injected))
    );
    failed_source.grant(4_096);
    assert_eq!(
        failed_work.poll_source(&mut failed_source, 17),
        Err(DirectSourceLinePollError::PollAfterFailure)
    );
    assert_eq!(failed_parser.scratch_node_count(), 1);

    let (mut parser_a, _) = started();
    let mut work_a = parser_a
        .begin_source_line_work(identity, line.len())
        .unwrap();
    assert!(
        parser_a
            .begin_source_line_work(identity, line.len())
            .is_err(),
        "one admission blocks duplicate minting"
    );
    let mut source_a = StrictSource::new(identity, line.as_bytes().to_vec());
    poll_to_match(&mut work_a, &mut source_a, |_| 7);

    let (mut parser_b, _) = started();
    let mut work_b = parser_b
        .begin_source_line_work(identity, line.len())
        .unwrap();
    let mut source_b = StrictSource::new(identity, line.as_bytes().to_vec());
    poll_to_match(&mut work_b, &mut source_b, |_| 7);
    assert!(
        parser_b
            .commit_source_line(work_a, identity, u32::try_from(line.len()).unwrap())
            .is_err(),
        "same-boundary work cannot cross parser instances"
    );
    assert_eq!(parser_b.scratch_node_count(), 1);
    parser_b
        .commit_source_line(work_b, identity, u32::try_from(line.len()).unwrap())
        .unwrap();
    let _ = drain_line(&mut parser_b);

    let (mut identity_parser, _) = started();
    let mut identity_work = identity_parser
        .begin_source_line_work(identity, line.len())
        .unwrap();
    let mut identity_source = StrictSource::new(identity, line.as_bytes().to_vec());
    poll_to_match(&mut identity_work, &mut identity_source, |_| 7);
    assert!(
        identity_parser
            .commit_source_line(
                identity_work,
                SourceIdentity {
                    revision: 12,
                    ..identity
                },
                u32::try_from(line.len()).unwrap(),
            )
            .is_err(),
        "commit checks the active full source identity"
    );
    assert_eq!(identity_parser.scratch_node_count(), 1);
}

#[test]
fn atx_rejection_continues_into_the_same_controller_without_precommit_mutation() {
    let line = "plain paragraph text\n";
    let identity = SourceIdentity::line(line.len());
    let (mut parser, _) = started();
    let before_scratch = parser.scratch_node_count();
    let before_legacy = parser.legacy_event_count();
    let before_retained = parser.retained_logical_bytes();
    let mut work = parser.begin_source_line_work(identity, line.len()).unwrap();
    let mut source = StrictSource::new(identity, line.as_bytes().to_vec());
    poll_to_match(&mut work, &mut source, |_| 1);
    assert_eq!(source.next, line.len());
    source.grant(4_096);
    assert_eq!(
        work.poll_source(&mut source, 1),
        Err(DirectSourceLinePollError::PollAfterComplete)
    );
    assert_eq!(parser.scratch_node_count(), before_scratch);
    assert_eq!(parser.legacy_event_count(), before_legacy);
    assert_eq!(parser.retained_logical_bytes(), before_retained);
    assert_eq!(parser.retained_line_bytes(), 0);
    assert!(parser.pending_command().is_none());
    parser
        .commit_source_line(
            work,
            identity,
            u32::try_from(line.encode_utf16().count()).unwrap(),
        )
        .unwrap();
    let commands = drain_line(&mut parser);
    assert_eq!(commands, buffered_line(line).1);
}

#[test]
fn prefix_boundaries_retries_and_join_owned_metrics_fail_closed() {
    for accepted in ["# x\n", " # x\n", "  # x\n", "   # x\n", "\u{feff}   # x\n"] {
        let (_, buffered) = buffered_line(accepted);
        let (source, actual) = source_line(accepted, |_| 1);
        assert_eq!(actual, buffered, "accepted prefix={accepted:?}");
        assert_eq!(
            source.capture_line_boundary_pause().unwrap(),
            buffered_line(accepted)
                .0
                .capture_line_boundary_pause()
                .unwrap()
        );
    }

    for rejected in ["    # x\n", "\t# x\n", " \t# x\n", "   \t# x\n"] {
        let identity = SourceIdentity::line(rejected.len());
        let (mut parser, _) = started();
        let mut work = parser
            .begin_source_line_work(identity, rejected.len())
            .unwrap();
        let mut source = StrictSource::new(identity, rejected.as_bytes().to_vec());
        poll_to_match(&mut work, &mut source, |_| 1);
        assert_eq!(parser.scratch_node_count(), 1);
        parser
            .commit_source_line(
                work,
                identity,
                u32::try_from(rejected.encode_utf16().count()).unwrap(),
            )
            .unwrap();
        assert!(matches!(
            parser.poll_line(64),
            Err(ParseError::DirectUnsupported(
                flark_comrak_value_block_core::DirectUnsupported::SegmentedLine
                    | flark_comrak_value_block_core::DirectUnsupported::BlockKind
            ))
        ));
        assert!(parser.pending_command().is_none());
    }

    let line = " # β😀\n";
    let identity = SourceIdentity::line(line.len());
    let (mut parser, _) = started();
    let mut work = parser.begin_source_line_work(identity, line.len()).unwrap();
    let mut source = StrictSource::new(identity, line.as_bytes().to_vec());
    source.grant(4_096);
    assert_eq!(
        work.poll_source(&mut source, 0),
        Err(DirectSourceLinePollError::ZeroFuel)
    );
    source.grant(0);
    let stalled = work.poll_source(&mut source, 7).unwrap();
    assert_eq!(stalled.status, DirectSourceLinePollStatus::NeedMore);
    assert_eq!(stalled.source_first_reads, 0);
    poll_to_match(&mut work, &mut source, |_| 3);
    assert_eq!(
        work.poll_source(&mut source, 3),
        Err(DirectSourceLinePollError::PollAfterComplete)
    );
    parser.commit_source_line(work, identity, 1).unwrap();
    let commands = drain_line(&mut parser);
    assert!(matches!(
        commands.last(),
        Some(DirectCommand::FinishLine {
            physical_utf16: 1,
            ..
        })
    ));

    let (mut length_parser, _) = started();
    let mut length_work = length_parser
        .begin_source_line_work(identity, line.len())
        .unwrap();
    let mut wrong_length = StrictSource::new(identity, line.as_bytes().to_vec());
    wrong_length.reported_len = Some(line.len() + 1);
    wrong_length.grant(4_096);
    assert_eq!(
        length_work.poll_source(&mut wrong_length, 7),
        Err(DirectSourceLinePollError::WrongSource)
    );
    assert_eq!(
        length_work.poll_source(&mut wrong_length, 7),
        Err(DirectSourceLinePollError::PollAfterFailure)
    );
}

#[test]
fn ten_mib_plain_paragraph_runs_the_exact_controller_with_a_four_kib_window() {
    const CONTENT_BYTES: usize = 10 * 1024 * 1024;
    let mut source = RepeatedParagraphSource::new(CONTENT_BYTES);
    let identity = source.identity;
    let (mut parser, _) = started();
    let mut work = parser
        .begin_source_line_work(identity, source.physical_bytes())
        .unwrap();
    let mut polls = 0;
    loop {
        source.grant(DIRECT_SEGMENTED_LINE_WINDOW_BYTES);
        let receipt = work.poll_source(&mut source, usize::MAX).unwrap();
        assert!(receipt.source_first_reads <= DIRECT_SEGMENTED_LINE_WINDOW_BYTES);
        assert!(receipt.lexical_work_units <= DIRECT_SEGMENTED_LINE_WINDOW_BYTES);
        assert!(receipt.retained_source_bytes <= DIRECT_SEGMENTED_LINE_WINDOW_BYTES);
        assert_eq!(receipt.maximum_source_request_rewind_bytes, 0);
        polls += 1;
        if receipt.status == DirectSourceLinePollStatus::Matched {
            break;
        }
        assert_eq!(receipt.status, DirectSourceLinePollStatus::NeedMore);
    }
    assert!(polls > 2_500);
    assert_eq!(source.next, source.physical_bytes());
    assert!(work.retained_source_bytes() <= DIRECT_SEGMENTED_LINE_WINDOW_BYTES);

    parser
        .commit_source_line(
            work,
            identity,
            u32::try_from(source.physical_bytes()).unwrap(),
        )
        .unwrap();
    assert!(parser.retained_line_bytes() <= DIRECT_SEGMENTED_LINE_WINDOW_BYTES);
    let commands = drain_line(&mut parser);
    assert_eq!(
        commands,
        vec![
            DirectCommand::Open {
                kind: DirectBlockKind::Paragraph,
            },
            DirectCommand::Consume {
                owner: flark_comrak_value_block_core::DirectOwner::TOP,
                part: flark_comrak_value_block_core::DirectCoveragePart::Content,
                range: 0..u32::try_from(CONTENT_BYTES).unwrap(),
                logical: flark_comrak_value_block_core::DirectLogicalAction::CanonicalText,
            },
            DirectCommand::StageTerminator {
                range: u32::try_from(CONTENT_BYTES).unwrap()
                    ..u32::try_from(CONTENT_BYTES + 2).unwrap(),
                ending: flark_comrak_value_block_core::DirectLineEnding::CrLf,
            },
            DirectCommand::FinishLine {
                physical_bytes: u32::try_from(CONTENT_BYTES + 2).unwrap(),
                physical_utf16: u32::try_from(CONTENT_BYTES + 2).unwrap(),
            },
        ]
    );
}

#[test]
fn ten_mib_definition_candidates_keep_donor_owned_reference_outcomes() {
    const TAIL_BYTES: usize = 10 * 1024 * 1024;

    let mut invalid = String::with_capacity(TAIL_BYTES + 32);
    invalid.push_str("[not a definition ");
    invalid.push_str(&"a".repeat(TAIL_BYTES));
    invalid.push_str("]\r\n");
    let (mut parser, mut commands) = source_line(&invalid, |_| 4_031);
    assert!(parser.retained_line_bytes() <= DIRECT_SEGMENTED_LINE_WINDOW_BYTES);
    parser.begin_finish().unwrap();
    assert_eq!(
        drive_finish(&mut parser, &mut commands),
        DirectPollStatus::ExternalWorkReady
    );
    assert_eq!(
        resolve_reference_prefix(&mut parser, &invalid).0,
        DirectReferencePrefixCommitStatus::ParagraphUnchangedArmed
    );
    assert_eq!(
        drive_finish(&mut parser, &mut commands),
        DirectPollStatus::Complete
    );

    let mut valid = String::with_capacity(TAIL_BYTES + 32);
    valid.push_str("[x]: /");
    valid.push_str(&"a".repeat(TAIL_BYTES));
    valid.push('\n');
    let (mut parser, mut commands) = source_line(&valid, |_| 3_997);
    parser.begin_line("visible\n".to_owned()).unwrap();
    commands.extend(drain_line(&mut parser));
    parser.begin_finish().unwrap();
    assert_eq!(
        drive_finish(&mut parser, &mut commands),
        DirectPollStatus::ExternalWorkReady
    );
    let logical = format!("{valid}visible\n");
    let (status, definitions) = resolve_reference_prefix(&mut parser, &logical);
    assert_eq!(
        status,
        DirectReferencePrefixCommitStatus::VisibleRemainderArmed
    );
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].normalized_label, "x");
    assert_eq!(definitions[0].logical_destination.bytes.start, 5);
    assert_eq!(
        definitions[0].logical_destination.bytes.end,
        u64::try_from(5 + 1 + TAIL_BYTES).unwrap()
    );
    assert_eq!(
        drive_finish(&mut parser, &mut commands),
        DirectPollStatus::Complete
    );
}

#[test]
fn segmented_unicode_endings_and_definition_looking_literals_match_buffered_donor() {
    for line in [
        "alpha🙂β",
        "alpha🙂β\n",
        "alpha🙂β\r",
        "alpha🙂β\r\n",
        "[label]: /url\n",
        " [label]: <broken\r\n",
        "[not a definition\n",
        "[x]: /url \"title\"\r\n",
        "\u{feff}[x]: /url\n",
    ] {
        let expected = buffered_line(line).1;
        let (_, actual) = source_line(line, |_| 1);
        assert_eq!(actual, expected, "line={line:?}");
    }
}

fn source_controller_error(line: &str) -> (DirectValueBlockParser, ParseError) {
    let identity = SourceIdentity::line(line.len());
    let (mut parser, _) = started();
    let mut work = parser.begin_source_line_work(identity, line.len()).unwrap();
    let mut source = StrictSource::new(identity, line.as_bytes().to_vec());
    poll_to_match(&mut work, &mut source, |_| 17);
    parser
        .commit_source_line(
            work,
            identity,
            u32::try_from(line.encode_utf16().count()).unwrap(),
        )
        .unwrap();
    let error = loop {
        match parser.poll_line(1) {
            Ok(receipt) => {
                assert_eq!(receipt.status, DirectPollStatus::Pending);
                assert!(parser.pending_command().is_none());
            }
            Err(error) => break error,
        }
    };
    (parser, error)
}

#[test]
fn complete_source_windows_admit_supported_container_and_fence_shapes() {
    for line in ["> quote\n", "- item\n", "``` code\n"] {
        assert_eq!(
            try_source_commands(line).unwrap(),
            try_buffered_commands(line).unwrap(),
            "line={line:?}",
        );
    }
}

#[test]
fn unsupported_block_shapes_are_rejected_only_after_entering_the_controller() {
    for line in ["<div>\n", "---\n", "    code\n"] {
        let (parser, error) = source_controller_error(line);
        assert!(
            matches!(
                error,
                ParseError::DirectUnsupported(
                    flark_comrak_value_block_core::DirectUnsupported::SegmentedLine
                        | flark_comrak_value_block_core::DirectUnsupported::BlockKind
                )
            ),
            "line={line:?}, error={error:?}"
        );
        assert!(parser.pending_command().is_none());
    }
}

#[test]
fn truncated_space_and_special_prefixes_fail_before_semantic_mutation() {
    let mut cases = Vec::new();
    cases.push(format!(
        "{}x\n",
        " ".repeat(DIRECT_SEGMENTED_LINE_WINDOW_BYTES + 1)
    ));
    for prefix in ["-", ">", "#x", "```", "<div"] {
        cases.push(format!(
            "{prefix}{}x\n",
            "a".repeat(DIRECT_SEGMENTED_LINE_WINDOW_BYTES + 1)
        ));
    }
    for line in cases {
        let (parser, error) = source_controller_error(&line);
        assert_eq!(
            error,
            ParseError::DirectUnsupported(
                flark_comrak_value_block_core::DirectUnsupported::SegmentedLine
            ),
            "line prefix={:?}",
            &line[..line.len().min(12)]
        );
        assert_eq!(parser.scratch_node_count(), 1);
        assert_eq!(parser.legacy_event_count(), 0);
        assert_eq!(parser.retained_logical_bytes(), 0);
        assert!(parser.pending_command().is_none());
    }
}

#[test]
fn suspended_source_work_can_be_cancelled_at_every_segment_boundary() {
    const SEGMENTS: usize = 5;
    const CONTENT_BYTES: usize = SEGMENTS * DIRECT_SEGMENTED_LINE_WINDOW_BYTES;
    for boundary in 0..=SEGMENTS {
        let mut source = RepeatedParagraphSource::new(CONTENT_BYTES);
        let identity = source.identity;
        let (mut parser, _) = started();
        let mut work = parser
            .begin_source_line_work(identity, source.physical_bytes())
            .unwrap();
        let target = boundary * DIRECT_SEGMENTED_LINE_WINDOW_BYTES;
        while source.next < target {
            let grant = (target - source.next).min(DIRECT_SEGMENTED_LINE_WINDOW_BYTES);
            source.grant(grant);
            let receipt = work.poll_source(&mut source, grant.max(1)).unwrap();
            assert!(receipt.retained_source_bytes <= DIRECT_SEGMENTED_LINE_WINDOW_BYTES);
        }
        assert_eq!(source.next, target);
        parser.cancel_source_line(work).unwrap();
        parser.begin_line("after\n".to_owned()).unwrap();
        let commands = drain_line(&mut parser);
        assert!(commands.iter().any(|command| matches!(
            command,
            DirectCommand::Open {
                kind: DirectBlockKind::Paragraph
            }
        )));
    }
}

#[derive(Deserialize)]
struct CorpusFixture {
    markdown: String,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[test]
fn source_controller_matches_buffered_parser_for_admitted_lines_across_1322_fixtures() {
    let root = repo_root();
    let paths = [
        root.join("test/fixtures/commonmark/upstream/common_mark_tests.json"),
        root.join("test/fixtures/commonmark/upstream/gfm_tests.json"),
    ];
    let mut fixtures = 0;
    let mut admitted = 0;
    for path in paths {
        let corpus: Vec<CorpusFixture> = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        for fixture in corpus {
            fixtures += 1;
            let line = fixture
                .markdown
                .split_inclusive('\n')
                .next()
                .unwrap_or(fixture.markdown.as_str());
            match (try_buffered_commands(line), try_source_commands(line)) {
                (Ok(expected), Ok(actual)) => {
                    assert_eq!(actual, expected, "first line={line:?}");
                    admitted += 1;
                }
                (Err(_), Err(_)) => {}
                (buffered, source) => {
                    panic!(
                        "buffered/source admission differs for first line {line:?}: \
                         buffered={buffered:?}, source={source:?}"
                    );
                }
            }
        }
    }
    assert_eq!(fixtures, 1_322);
    assert!(
        admitted > 500,
        "unexpectedly narrow admitted corpus: {admitted}"
    );
}

struct Lcg(u64);

impl Lcg {
    fn usize(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 32) as usize) % upper
    }
}

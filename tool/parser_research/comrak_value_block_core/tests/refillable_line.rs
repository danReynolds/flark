use std::cell::Cell;

use flark_comrak_value_block_core::parser::{
    DirectCommand, DirectCoveragePart, DirectFenceCharacter, DirectFencedCodeFacts,
    DirectLineEnding, DirectLogicalAction, DirectPollStatus, DirectUnsupported,
    DirectValueBlockParser, ParseError,
};
use flark_comrak_value_block_core::refillable_line::{
    DEFAULT_REFILL_WINDOW_BYTES, RefillableCancellationToken, RefillableClaimAction,
    RefillableCoverageClaim, RefillableLineContext, RefillableLineError, RefillableLineJob,
    RefillableLineKind, RefillableLineSource, RefillablePollStatus, RefillableSourceReadError,
};
use flark_comrak_value_block_core::source_ledger::{
    RefillableSourceLine, RefillableSourceLineKey, SourceMetric, SourceRevision,
    SourceRootAuthority,
};
use flark_comrak_value_block_core::tree::SyntaxProfile;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SourceStats {
    reads: usize,
    maximum_request: usize,
    bytes_copied: usize,
}

struct ChunkedSource<'source> {
    key: RefillableSourceLineKey,
    bytes: &'source [u8],
    backend_chunk: usize,
    stats: Cell<SourceStats>,
}

impl<'source> ChunkedSource<'source> {
    fn new(key: RefillableSourceLineKey, bytes: &'source [u8], backend_chunk: usize) -> Self {
        assert!(backend_chunk > 0);
        Self {
            key,
            bytes,
            backend_chunk,
            stats: Cell::new(SourceStats::default()),
        }
    }

    fn stats(&self) -> SourceStats {
        self.stats.get()
    }
}

impl RefillableLineSource for ChunkedSource<'_> {
    fn line_key(&self) -> RefillableSourceLineKey {
        self.key
    }

    fn read_window(
        &self,
        relative_start: u64,
        destination: &mut [u8],
    ) -> Result<usize, RefillableSourceReadError> {
        let start =
            usize::try_from(relative_start).map_err(|_| RefillableSourceReadError::Unavailable)?;
        if start > self.bytes.len() {
            return Err(RefillableSourceReadError::Unavailable);
        }
        let read = destination
            .len()
            .min(self.backend_chunk)
            .min(self.bytes.len() - start);
        destination[..read].copy_from_slice(&self.bytes[start..start + read]);
        let old = self.stats.get();
        self.stats.set(SourceStats {
            reads: old.reads + 1,
            maximum_request: old.maximum_request.max(destination.len()),
            bytes_copied: old.bytes_copied + read,
        });
        Ok(read)
    }
}

fn certified_line(text: &str, revision: u64, ordinal: u64, start: u64) -> RefillableSourceLine {
    SourceRootAuthority::new()
        .begin_revision(SourceRevision(revision))
        .lease_refillable_line(ordinal, start, SourceMetric::for_utf8(text))
        .expect("certified refillable line")
}

fn run_to_complete(job: &mut RefillableLineJob, source: &impl RefillableLineSource, fuel: usize) {
    let cancellation = RefillableCancellationToken::default();
    loop {
        let receipt = job.poll(source, fuel, &cancellation).expect("refill poll");
        if receipt.status == RefillablePollStatus::Complete {
            break;
        }
        assert_eq!(receipt.status, RefillablePollStatus::Pending);
    }
}

#[test]
fn ten_mib_paragraph_admission_and_each_poll_are_bounded() {
    const CONTENT_BYTES: usize = 10 * 1024 * 1024;
    let mut text = "a".repeat(CONTENT_BYTES);
    text.push_str("\r\n");
    let line = certified_line(&text, 7, 19, 1_000);
    let source = ChunkedSource::new(line.key(), text.as_bytes(), usize::MAX);
    let mut job = RefillableLineJob::new(
        line,
        RefillableLineContext::Document,
        DEFAULT_REFILL_WINDOW_BYTES,
    )
    .expect("O(1) admission");
    assert_eq!(source.stats().reads, 0, "admission never touches source");

    run_to_complete(&mut job, &source, usize::MAX);

    let result = job.result().expect("completed result");
    assert_eq!(result.kind, RefillableLineKind::Paragraph);
    assert_eq!(result.ending, Some(DirectLineEnding::CrLf));
    assert!(result.coverage_is_complete());
    assert_eq!(result.claims().len(), 2);
    assert_eq!(result.claims()[0].relative_range, 0..CONTENT_BYTES as u64);
    assert_eq!(
        result.claims()[1].relative_range,
        CONTENT_BYTES as u64..CONTENT_BYTES as u64 + 2
    );
    let scan = job.scan_receipt();
    assert_eq!(scan.prefix_bytes_inspected, 1);
    assert_eq!(scan.body_bytes_covered, CONTENT_BYTES as u64 - 1);
    assert_eq!(scan.terminal_bytes_inspected, 2);
    assert_eq!(scan.bytes_inspected, text.len() as u64);
    assert_eq!(scan.source_bytes_read, text.len() as u64);
    assert!(scan.maximum_source_bytes_per_poll <= DEFAULT_REFILL_WINDOW_BYTES);
    assert!(scan.maximum_bytes_inspected_per_poll <= DEFAULT_REFILL_WINDOW_BYTES);
    assert_eq!(scan.scratch_capacity_bytes, DEFAULT_REFILL_WINDOW_BYTES);
    assert_eq!(scan.retained_source_bytes, 0);
    assert_eq!(job.retained_source_bytes(), 0);
    assert!(source.stats().maximum_request <= DEFAULT_REFILL_WINDOW_BYTES);
}

#[test]
fn giant_fenced_literal_streams_body_after_a_bounded_prefix() {
    const CONTENT_BYTES: usize = 10 * 1024 * 1024;
    let mut text = String::with_capacity(CONTENT_BYTES + 1);
    text.push_str("  x");
    text.extend(std::iter::repeat_n('y', CONTENT_BYTES - 3));
    text.push('\n');
    let facts = DirectFencedCodeFacts {
        fence: DirectFenceCharacter::Backtick,
        minimum_closing_length: 3,
        fence_offset_columns: 2,
    };
    let line = certified_line(&text, 8, 20, 2_000);
    let source = ChunkedSource::new(line.key(), text.as_bytes(), 913);
    let mut job = RefillableLineJob::new(
        line,
        RefillableLineContext::FencedCode(facts),
        DEFAULT_REFILL_WINDOW_BYTES,
    )
    .expect("refillable fenced line");

    run_to_complete(&mut job, &source, DEFAULT_REFILL_WINDOW_BYTES);

    let result = job.result().expect("literal result");
    assert_eq!(result.kind, RefillableLineKind::FencedCodeLiteral);
    assert!(result.coverage_is_complete());
    assert_eq!(result.claims().len(), 3);
    assert_eq!(result.claims()[0].relative_range, 0..2);
    assert_eq!(result.claims()[0].part, DirectCoveragePart::ContainerMarker);
    assert_eq!(result.claims()[1].relative_range, 2..CONTENT_BYTES as u64);
    assert_eq!(
        result.claims()[2].action,
        RefillableClaimAction::Consume(DirectLogicalAction::CanonicalNewline)
    );
    let scan = job.scan_receipt();
    assert_eq!(scan.prefix_bytes_inspected, 3);
    assert_eq!(scan.body_bytes_covered, CONTENT_BYTES as u64 - 3);
    assert_eq!(scan.terminal_bytes_inspected, 1);
    assert!(scan.maximum_source_bytes_per_poll <= 913);
    assert_eq!(scan.retained_source_bytes, 0);
}

#[test]
fn utf8_scalar_and_fence_marker_may_split_across_one_byte_refills() {
    let paragraph = "a🙂β\r\n";
    let paragraph_line = certified_line(paragraph, 9, 1, 0);
    let paragraph_source = ChunkedSource::new(paragraph_line.key(), paragraph.as_bytes(), 1);
    let mut paragraph_job =
        RefillableLineJob::new(paragraph_line, RefillableLineContext::Document, 2)
            .expect("two-byte scratch");
    run_to_complete(&mut paragraph_job, &paragraph_source, 2);
    let paragraph_result = paragraph_job.result().expect("paragraph result");
    assert_eq!(paragraph_result.metric.utf16, 6);
    assert_eq!(paragraph_result.claims()[0].metric.utf16, 4);
    assert!(paragraph_result.coverage_is_complete());
    assert_eq!(paragraph_source.stats().maximum_request, 2);

    let opener = "   ~~~~ info🙂\n";
    let opener_line = certified_line(opener, 9, 2, 100);
    let opener_source = ChunkedSource::new(opener_line.key(), opener.as_bytes(), 1);
    let mut opener_job = RefillableLineJob::new(opener_line, RefillableLineContext::Document, 1)
        .expect("one-byte scratch");
    run_to_complete(&mut opener_job, &opener_source, 99);
    let opener_result = opener_job.result().expect("opener result");
    assert_eq!(
        opener_result.kind,
        RefillableLineKind::FencedCodeOpening(DirectFencedCodeFacts {
            fence: DirectFenceCharacter::Tilde,
            minimum_closing_length: 4,
            fence_offset_columns: 3,
        })
    );
    assert_eq!(opener_result.claims()[0].relative_range, 0..7);
    assert!(opener_result.coverage_is_complete());
    assert_eq!(opener_source.stats().reads, opener.len());
}

#[test]
fn fenced_closer_prefix_may_split_across_windows() {
    let text = "  ```   \r\n";
    let facts = DirectFencedCodeFacts {
        fence: DirectFenceCharacter::Backtick,
        minimum_closing_length: 3,
        fence_offset_columns: 2,
    };
    let line = certified_line(text, 10, 3, 500);
    let source = ChunkedSource::new(line.key(), text.as_bytes(), 1);
    let mut job = RefillableLineJob::new(line, RefillableLineContext::FencedCode(facts), 1)
        .expect("one-byte closer");

    run_to_complete(&mut job, &source, 1);

    let result = job.result().expect("closer result");
    assert_eq!(result.kind, RefillableLineKind::FencedCodeClosing);
    assert_eq!(result.claims()[0].relative_range, 0..8);
    assert_eq!(result.claims()[0].part, DirectCoveragePart::BlockMarker);
    assert_eq!(result.claims()[1].relative_range, 8..10);
    assert_eq!(result.claims()[1].part, DirectCoveragePart::Terminal);
    assert_eq!(job.scan_receipt().prefix_bytes_inspected, 8);
    assert_eq!(job.scan_receipt().body_bytes_covered, 0);
}

#[test]
fn cancellation_abandons_ranges_without_retaining_source() {
    let text = "a".repeat(1024 * 1024);
    let line = certified_line(&text, 11, 4, 0);
    let source = ChunkedSource::new(line.key(), text.as_bytes(), usize::MAX);
    let mut job =
        RefillableLineJob::new(line, RefillableLineContext::Document, 64).expect("bounded job");
    let cancellation = RefillableCancellationToken::default();

    let first = job.poll(&source, 64, &cancellation).expect("first poll");
    assert_eq!(first.status, RefillablePollStatus::Pending);
    assert_eq!(first.bytes_inspected, 64);
    cancellation.cancel();
    let cancelled = job.poll(&source, 64, &cancellation).expect("cancel poll");
    assert_eq!(cancelled.status, RefillablePollStatus::Cancelled);
    assert_eq!(cancelled.bytes_inspected, 0);

    let receipt = job.cancellation_receipt().expect("cancellation receipt");
    assert_eq!(receipt.bytes_inspected, 64);
    assert_eq!(receipt.prefix_bytes_inspected, 1);
    assert_eq!(receipt.body_bytes_covered, 61);
    assert_eq!(receipt.scratch_capacity_bytes, 64);
    assert_eq!(receipt.retained_source_bytes, 0);
    assert_eq!(receipt.completed_claims, 0);
    assert!(job.result().is_none());
    assert_eq!(source.stats().reads, 1);
}

#[test]
fn wrong_source_identity_and_metric_mismatch_fail_closed() {
    let text = "alpha\n";
    let expected = certified_line(text, 12, 5, 0);
    let foreign = certified_line(text, 12, 5, 0);
    let foreign_source = ChunkedSource::new(foreign.key(), text.as_bytes(), 8);
    let mut wrong_source_job = RefillableLineJob::new(expected, RefillableLineContext::Document, 8)
        .expect("job descriptor");
    assert_eq!(
        wrong_source_job.poll(&foreign_source, 8, &RefillableCancellationToken::default()),
        Err(RefillableLineError::WrongSourceLine)
    );
    assert_eq!(foreign_source.stats().reads, 0);

    let root = SourceRootAuthority::new();
    let revision = root.begin_revision(SourceRevision(13));
    let bad_metric = SourceMetric {
        bytes: text.len() as u64,
        utf16: text.encode_utf16().count() as u64 + 1,
    };
    let line = revision
        .lease_refillable_line(6, 0, bad_metric)
        .expect("O(1) metric claim");
    let source = ChunkedSource::new(line.key(), text.as_bytes(), 8);
    let mut metric_job =
        RefillableLineJob::new(line, RefillableLineContext::Document, 8).expect("job");
    assert_eq!(
        metric_job.poll(&source, 8, &RefillableCancellationToken::default()),
        Err(RefillableLineError::MetricMismatch {
            source: bad_metric,
            derived: SourceMetric::for_utf8(text),
        })
    );
}

#[test]
fn significant_document_constructs_remain_an_explicit_donor_seam() {
    for text in [
        "# heading\n",
        "> quote\n",
        "- item\n",
        "+ item\n",
        "* * *\n",
        "_ _ _\n",
        "1. item\n",
        "<div>\n",
        "[label]: /url\n",
        "=\n",
    ] {
        let line = certified_line(text, 15, 1, 0);
        let source = ChunkedSource::new(line.key(), text.as_bytes(), 1);
        let mut job =
            RefillableLineJob::new(line, RefillableLineContext::Document, 1).expect("bounded job");
        let cancellation = RefillableCancellationToken::default();
        let error = loop {
            match job.poll(&source, 1, &cancellation) {
                Ok(receipt) => assert_eq!(receipt.status, RefillablePollStatus::Pending),
                Err(error) => break error,
            }
        };
        assert!(
            matches!(error, RefillableLineError::UnsupportedDocumentPrefix { .. }),
            "construct {text:?} must return to the donor grammar, got {error:?}"
        );
    }
}

#[test]
fn standalone_proof_does_not_claim_the_direct_parser_ceiling_is_removed() {
    let text = "a".repeat(DirectValueBlockParser::MAX_LINE_BYTES + 1);
    let mut direct = DirectValueBlockParser::new(SyntaxProfile::CommonMark).expect("direct parser");
    direct
        .acknowledge_command()
        .expect("acknowledge document open");
    assert_eq!(
        direct.begin_line(text.clone()),
        Err(ParseError::DirectUnsupported(
            DirectUnsupported::LineTooLarge
        ))
    );

    let line = certified_line(&text, 16, 1, 0);
    let source = ChunkedSource::new(line.key(), text.as_bytes(), usize::MAX);
    let mut refillable = RefillableLineJob::new(
        line,
        RefillableLineContext::Document,
        DEFAULT_REFILL_WINDOW_BYTES,
    )
    .expect("standalone refillable job");
    run_to_complete(&mut refillable, &source, DEFAULT_REFILL_WINDOW_BYTES);
    assert_eq!(
        refillable.result().expect("refill result").kind,
        RefillableLineKind::Paragraph
    );
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClaimShape {
    part: DirectCoveragePart,
    range: std::ops::Range<u64>,
    action: RefillableClaimAction,
}

fn refillable_shapes(claims: &[RefillableCoverageClaim]) -> Vec<ClaimShape> {
    claims
        .iter()
        .map(|claim| ClaimShape {
            part: claim.part,
            range: claim.relative_range.clone(),
            action: claim.action,
        })
        .collect()
}

fn direct_shapes(lines: &[&str], selected_line: usize) -> Vec<ClaimShape> {
    let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).expect("direct parser");
    assert!(matches!(
        parser.pending_command(),
        Some(DirectCommand::Open { .. })
    ));
    parser.acknowledge_command().expect("document open");
    let mut selected = Vec::new();
    for (line_index, line) in lines.iter().enumerate() {
        parser.begin_line((*line).to_owned()).expect("direct line");
        loop {
            if let Some(command) = parser.pending_command().cloned() {
                if line_index == selected_line {
                    match command {
                        DirectCommand::Consume {
                            part,
                            range,
                            logical,
                            ..
                        } => selected.push(ClaimShape {
                            part,
                            range: u64::from(range.start)..u64::from(range.end),
                            action: RefillableClaimAction::Consume(logical),
                        }),
                        DirectCommand::StageTerminator { range, ending } => {
                            selected.push(ClaimShape {
                                part: DirectCoveragePart::Terminal,
                                range: u64::from(range.start)..u64::from(range.end),
                                action: RefillableClaimAction::StageParagraphTerminator { ending },
                            });
                        }
                        _ => {}
                    }
                }
                parser.acknowledge_command().expect("direct command");
                continue;
            }
            let receipt = parser.poll_line(1).expect("direct poll");
            if receipt.status == DirectPollStatus::Complete {
                break;
            }
        }
    }
    selected
}

fn refillable_shapes_for(text: &str, context: RefillableLineContext) -> Vec<ClaimShape> {
    let line = certified_line(text, 14, 7, 0);
    let source = ChunkedSource::new(line.key(), text.as_bytes(), 1);
    let mut job = RefillableLineJob::new(line, context, 1).expect("refillable line");
    run_to_complete(&mut job, &source, 1);
    refillable_shapes(job.result().expect("result").claims())
}

#[test]
fn admitted_small_lines_match_direct_donor_provenance_cuts() {
    for line in ["alpha🙂\r\n", "  alpha\n", "```` info\r", "   ~~~\n"] {
        assert_eq!(
            refillable_shapes_for(line, RefillableLineContext::Document),
            direct_shapes(&[line], 0),
            "document line {line:?}"
        );
    }

    let facts = DirectFencedCodeFacts {
        fence: DirectFenceCharacter::Backtick,
        minimum_closing_length: 3,
        fence_offset_columns: 2,
    };
    for body in ["  alpha🙂\r\n", "  ```   \n"] {
        assert_eq!(
            refillable_shapes_for(body, RefillableLineContext::FencedCode(facts)),
            direct_shapes(&["  ```\n", body], 1),
            "fenced line {body:?}"
        );
    }
}

#[test]
fn standalone_bom_divergence_proves_refill_result_is_not_parser_authority() {
    let line = "\u{feff}alpha\n";
    let refillable = refillable_shapes_for(line, RefillableLineContext::Document);
    let direct = direct_shapes(&[line], 0);

    assert_ne!(
        refillable, direct,
        "the standalone feasibility recognizer does not own first-line BOM grammar"
    );
    assert_eq!(
        direct.first(),
        Some(&ClaimShape {
            part: DirectCoveragePart::Gap,
            range: 0..3,
            action: RefillableClaimAction::Consume(DirectLogicalAction::None),
        }),
        "the donor, not the caller-selected refill context, owns the BOM cut"
    );
}

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn random_ending(state: &mut u64) -> &'static str {
    match next_random(state) % 4 {
        0 => "",
        1 => "\n",
        2 => "\r",
        _ => "\r\n",
    }
}

fn push_random_tail(text: &mut String, state: &mut u64, maximum: u64) {
    const ALPHABET: &[u8] = b" abcxyz`~";
    let length = next_random(state) % (maximum + 1);
    for _ in 0..length {
        let alphabet_len = u64::try_from(ALPHABET.len()).expect("alphabet length");
        let index = usize::try_from(next_random(state) % alphabet_len).expect("alphabet index");
        text.push(char::from(ALPHABET[index]));
    }
}

#[test]
fn randomized_admitted_slice_matches_direct_donor_cuts() {
    let mut state = 0x05ee_d023_u64;
    for case in 0..256 {
        let mut line = " ".repeat(usize::try_from(next_random(&mut state) % 4).expect("indent"));
        match next_random(&mut state) % 3 {
            0 => line.push('a'),
            1 => {
                let run = usize::try_from(next_random(&mut state) % 8 + 1).expect("marker run");
                line.extend(std::iter::repeat_n('`', run));
            }
            _ => {
                let run = usize::try_from(next_random(&mut state) % 8 + 1).expect("marker run");
                line.extend(std::iter::repeat_n('~', run));
            }
        }
        push_random_tail(&mut line, &mut state, 32);
        line.push_str(random_ending(&mut state));
        assert_eq!(
            refillable_shapes_for(&line, RefillableLineContext::Document),
            direct_shapes(&[&line], 0),
            "random document case {case}: {line:?}"
        );
    }

    let facts = DirectFencedCodeFacts {
        fence: DirectFenceCharacter::Backtick,
        minimum_closing_length: 3,
        fence_offset_columns: 2,
    };
    for case in 0..256 {
        let mut line = " ".repeat(usize::try_from(next_random(&mut state) % 7).expect("indent"));
        match next_random(&mut state) % 3 {
            0 => line.push('a'),
            1 => {
                let run = usize::try_from(next_random(&mut state) % 7 + 1).expect("marker run");
                line.extend(std::iter::repeat_n('`', run));
            }
            _ => {}
        }
        push_random_tail(&mut line, &mut state, 32);
        line.push_str(random_ending(&mut state));
        assert_eq!(
            refillable_shapes_for(&line, RefillableLineContext::FencedCode(facts)),
            direct_shapes(&["  ```\n", &line], 1),
            "random fenced case {case}: {line:?}"
        );
    }
}

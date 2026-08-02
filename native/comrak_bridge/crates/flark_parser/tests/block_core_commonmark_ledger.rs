// SPDX-License-Identifier: MIT

use std::{
    any::Any,
    collections::BTreeMap,
    panic::{catch_unwind, AssertUnwindSafe},
};

use flark_engine::parser_internal::{
    M11InlineProjectionCursorPoll, M11InlineProjectionFact, M11InlineProjectionKind,
    M11RecursiveGreenEvent, M11RecursiveGreenFrameId, M11RecursiveGreenFrameQueryLimits,
    M11RecursiveGreenLogicalAction, M11RecursiveGreenPoint, M11RecursiveGreenRoot,
    M11ReferenceJournal, M11ReferenceJournalRoot, M11ReferenceJournalStatus,
};
use flark_engine::{
    DocumentRuntime, DocumentRuntimeConfig, ParserProfileId, SourceBoundaryAffinity,
    SOURCE_CURSOR_WINDOW_BYTES,
};
use flark_parser::block_core::{
    resolve_m11_recursive_green_paragraph_fence, M11BlockWriter, M11BlockWriterOfferStatus,
    M11BlockWriterPollStatus, M11DirectBlockController, M11DirectBlockControllerError,
    M11DirectBlockError, M11DirectBlockPollStatus, M11DirectBlockUnsupported,
    M11ReferenceRendezvous, M11ReferenceRendezvousStatus, M11_DIRECT_BLOCK_MAX_LEXICAL_SLACK,
    M11_DIRECT_BLOCK_MAX_RETAINED_SOURCE_BYTES,
};
use flark_parser::{
    M11ExactController, M11InlineProjectionJob, M11InlineProjectionJobPollStatus,
    M11InlineProjectionPublication, M11ParserBinding, M11SourceLinePollStatus, M11SourceLineSource,
    SnapshotLinePoll, SnapshotLineScanner, SnapshotLineSource, SourceAdapterError,
};
use sha2::{Digest, Sha256};

const COMMONMARK_FIXTURES: &str =
    include_str!("../../../../../test/fixtures/commonmark/upstream/common_mark_tests.json");
// The production direct controller currently has one CommonMark profile and
// no GFM-profile constructor. Feeding the pinned GFM corpus through that same
// controller would measure profile mismatch, not selected-profile parity.
const COMMONMARK_FIXTURE_SHA256: &str =
    "d431b29d97b6f73e69d547109cf5081578fac931e72afe95639ebe766c1b2a20";
const EXPECTED_EXAMPLES: usize = 652;
const FUEL: usize = 7;
const MAX_POLLS: usize = 1_000_000;

// One snapshot update records an intentional production-grammar promotion.
// Non-explicit failures remain a hard failure below and receive no coverage.
const EXPECTED_ADMITTED: usize = 652;
const EXPECTED_UNSUPPORTED: usize = 0;
const EXPECTED_RECEIPT_SHA256: &str =
    "b7b35caa97e225e86ca8d4d96275a8cc891135fa942e5bb9448a2d45aba7b79a";
const EXPECTED_SEMANTIC_RENDER_EXACT: usize = 384;
const EXPECTED_SEMANTIC_RENDER_DIVERGENT: usize = 6;
const EXPECTED_SEMANTIC_RECEIPT_SHA256: &str =
    "dc6e79a29eb93bf1053d9416d62465568593155b4b5bab34ade2dd243e58ea85";

#[derive(Debug)]
struct Fixture {
    markdown: String,
    html: String,
    example: usize,
    section: String,
}

#[derive(Debug)]
struct DriveReceipt {
    semantic: SemanticOutcome,
}

#[derive(Debug)]
enum SemanticOutcome {
    Exact,
    Missing(&'static str),
    Divergent(String),
}

#[derive(Debug)]
struct GreenBlock {
    frame: M11RecursiveGreenFrameId,
    parent: Option<usize>,
    kind: u16,
    property: Option<(u16, Vec<u8>)>,
    close: Option<(u16, Vec<u8>)>,
    logical: String,
    first_owned_logical_byte: Option<usize>,
}

#[derive(Debug)]
enum DriveFailure {
    Unsupported(M11DirectBlockUnsupported),
    Invalid(String),
}

#[test]
fn commonmark_9_partially_consumed_tab_keeps_one_physical_owner() {
    drive_document(" - foo\n   - bar\n\t - baz\n")
        .expect("CM9 is admitted through the source-backed bounded controller");
}

#[test]
fn commonmark_214_reference_only_paragraph_rebases_the_open_line() {
    drive_document("# [Foo]\n[foo]: /url\n> bar\n")
        .expect("CM214 is admitted through the writer and reference rendezvous");
}

#[test]
fn production_controller_commonmark_coverage_is_monotonic_and_fail_closed() {
    assert_eq!(
        sha256(COMMONMARK_FIXTURES.as_bytes()),
        COMMONMARK_FIXTURE_SHA256
    );
    let fixtures = load_fixtures(COMMONMARK_FIXTURES);
    assert_eq!(fixtures.len(), EXPECTED_EXAMPLES);

    let mut admitted = 0;
    let mut unsupported = 0;
    let mut semantic_exact = 0;
    let mut semantic_missing = BTreeMap::<String, usize>::new();
    let mut semantic_divergent = Vec::new();
    let mut invalid = Vec::new();
    let mut receipt = String::new();
    let mut semantic_receipt = String::new();
    let mut sections = BTreeMap::<String, (usize, usize)>::new();

    for (index, fixture) in fixtures.iter().enumerate() {
        assert_eq!(
            fixture.example,
            index + 1,
            "fixture inventory is contiguous"
        );
        let result = catch_unwind(AssertUnwindSafe(|| drive_fixture(fixture)))
            .unwrap_or_else(|panic| Err(DriveFailure::Invalid(panic_message(&panic))));
        match result {
            Ok(drive) => {
                admitted += 1;
                sections.entry(fixture.section.clone()).or_default().0 += 1;
                receipt.push_str(&format!(
                    "{}\t{}\tadmitted\n",
                    fixture.example, fixture.section
                ));
                match drive.semantic {
                    SemanticOutcome::Exact => {
                        semantic_exact += 1;
                        semantic_receipt.push_str(&format!(
                            "{}\t{}\tsemantic-render-exact\n",
                            fixture.example, fixture.section
                        ));
                    }
                    SemanticOutcome::Missing(mechanism) => {
                        *semantic_missing.entry(mechanism.into()).or_default() += 1;
                        semantic_receipt.push_str(&format!(
                            "{}\t{}\tmissing:{mechanism}\n",
                            fixture.example, fixture.section
                        ));
                    }
                    SemanticOutcome::Divergent(difference) => {
                        semantic_divergent.push(format!(
                            "{} ({:?}): {difference}",
                            fixture.example, fixture.section
                        ));
                        semantic_receipt.push_str(&format!(
                            "{}\t{}\tsemantic-divergent:{difference}\n",
                            fixture.example, fixture.section
                        ));
                    }
                }
            }
            Err(DriveFailure::Unsupported(reason)) => {
                unsupported += 1;
                sections.entry(fixture.section.clone()).or_default().1 += 1;
                receipt.push_str(&format!(
                    "{}\t{}\tunsupported:{reason:?}\n",
                    fixture.example, fixture.section
                ));
            }
            Err(DriveFailure::Invalid(reason)) => {
                invalid.push(format!(
                    "{} ({:?}): {reason}",
                    fixture.example, fixture.section
                ));
                receipt.push_str(&format!(
                    "{}\t{}\tINVALID:{reason}\n",
                    fixture.example, fixture.section
                ));
            }
        }
    }

    assert_eq!(admitted + unsupported + invalid.len(), EXPECTED_EXAMPLES);
    let semantic_receipt_sha256 = sha256(semantic_receipt.as_bytes());
    let section_receipt = sections
        .iter()
        .map(|(section, (admitted, unsupported))| {
            format!("{section}: admitted={admitted} unsupported={unsupported}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let receipt_sha256 = sha256(receipt.as_bytes());
    assert_eq!(
        (admitted, unsupported, receipt_sha256.as_str()),
        (
            EXPECTED_ADMITTED,
            EXPECTED_UNSUPPORTED,
            EXPECTED_RECEIPT_SHA256,
        ),
        "production CommonMark admission changed; update the one checked-in snapshot only after reviewing this deterministic receipt:\n{section_receipt}\n\n{receipt}"
    );
    assert!(
        invalid.is_empty(),
        "production CommonMark ledger rejected non-explicit failures instead of laundering them as unsupported (admitted={admitted} unsupported={unsupported} receipt_sha256={receipt_sha256}):\n{}",
        invalid.join("\n")
    );
    let expected_missing = BTreeMap::from([
        ("autolink-render-value".to_owned(), 19),
        ("heading-inline-authority".to_owned(), 40),
        ("inline-fail-closed".to_owned(), 47),
        ("inline-link-value-replay".to_owned(), 46),
        ("projected-inline-authority".to_owned(), 19),
        ("reference-resolver".to_owned(), 91),
    ]);
    assert_eq!(
        semantic_exact + semantic_missing.values().sum::<usize>() + semantic_divergent.len(),
        EXPECTED_EXAMPLES,
        "semantic ledger did not classify every admitted fixture exactly once"
    );
    assert_eq!(
        (
            semantic_exact,
            &semantic_missing,
            semantic_divergent.len(),
            semantic_receipt_sha256.as_str(),
        ),
        (
            EXPECTED_SEMANTIC_RENDER_EXACT,
            &expected_missing,
            EXPECTED_SEMANTIC_RENDER_DIVERGENT,
            EXPECTED_SEMANTIC_RECEIPT_SHA256,
        ),
        "production semantic/render coverage changed; review the corpus-wide receipt before updating this one snapshot:\n{}",
        semantic_divergent.join("\n")
    );
}

fn drive_document(markdown: &str) -> Result<(), DriveFailure> {
    drive(markdown, None).map(|_| ())
}

fn drive_fixture(fixture: &Fixture) -> Result<DriveReceipt, DriveFailure> {
    drive(&fixture.markdown, Some(&fixture.html))
}

fn drive(markdown: &str, expected_html: Option<&str>) -> Result<DriveReceipt, DriveFailure> {
    let mut runtime = DocumentRuntime::new(markdown, DocumentRuntimeConfig::default())
        .map_err(|error| DriveFailure::Invalid(format!("runtime creation: {error:?}")))?;
    let source_version = runtime
        .current_source_version()
        .ok_or_else(|| DriveFailure::Invalid("runtime omitted its current source".into()))?;
    let mut journal = M11ReferenceJournal::new(&mut runtime, source_version, 1)
        .map_err(|error| DriveFailure::Invalid(format!("reference journal: {error}")))?;
    let writer_lease = runtime
        .snapshot_current_source()
        .map_err(|error| DriveFailure::Invalid(format!("writer source lease: {error:?}")))?;
    let scanner_lease = runtime
        .snapshot_current_source()
        .map_err(|error| DriveFailure::Invalid(format!("scanner source lease: {error:?}")))?;
    let scanner = SnapshotLineScanner::new(scanner_lease)
        .map_err(|error| DriveFailure::Invalid(format!("line scanner creation: {error:?}")))?;
    let mut controller = M11DirectBlockController::new().map_err(map_direct_error)?;
    let mut writer = M11BlockWriter::new(&runtime, writer_lease)
        .map_err(|error| DriveFailure::Invalid(format!("block writer: {error}")))?;

    let driven = drive_integrated(
        markdown,
        scanner,
        &mut controller,
        &mut writer,
        &mut journal,
        &mut runtime,
    );
    let (mut green, mut references) = match driven {
        Ok(roots) => roots,
        Err(error) => {
            cancel_pipeline(&mut writer, &mut journal, &mut runtime)?;
            close_runtime(&mut runtime)?;
            return Err(error);
        }
    };

    require(
        green.source_byte_len()
            == u64::try_from(markdown.len())
                .map_err(|_| DriveFailure::Invalid("source length overflow".into()))?,
        "Green physical byte coverage diverged from source",
    )?;
    require(
        green.source_utf16_len()
            == u64::try_from(markdown.encode_utf16().count())
                .map_err(|_| DriveFailure::Invalid("UTF-16 length overflow".into()))?,
        "Green physical UTF-16 coverage diverged from source",
    )?;
    let semantic = if let Some(expected_html) = expected_html {
        match green_block_projection(markdown, &green, &runtime) {
            Ok(blocks) => semantic_outcome(markdown, expected_html, &blocks, &green, &mut runtime),
            Err(DriveFailure::Invalid(error)) => SemanticOutcome::Divergent(error),
            Err(DriveFailure::Unsupported(_)) => {
                SemanticOutcome::Divergent("semantic replay unexpectedly unsupported".into())
            }
        }
    } else {
        SemanticOutcome::Exact
    };
    release_pipeline_roots(&mut green, &mut references, &mut runtime)?;
    close_runtime(&mut runtime)?;
    Ok(DriveReceipt { semantic })
}

fn drive_integrated(
    _markdown: &str,
    mut scanner: SnapshotLineScanner,
    controller: &mut M11DirectBlockController,
    writer: &mut M11BlockWriter,
    journal: &mut M11ReferenceJournal,
    runtime: &mut DocumentRuntime,
) -> Result<(M11RecursiveGreenRoot, M11ReferenceJournalRoot), DriveFailure> {
    write_pending_command(controller, writer, runtime)?;

    loop {
        let line = loop {
            match scanner
                .poll(FUEL)
                .map_err(|error| DriveFailure::Invalid(format!("line discovery: {error:?}")))?
            {
                SnapshotLinePoll::Pending(next) => scanner = next,
                SnapshotLinePoll::Line(line) => break Some(line),
                SnapshotLinePoll::Complete => break None,
            }
        };
        let Some(line) = line else { break };
        let facts = line.facts();
        let mut source = line
            .into_source()
            .map_err(|error| DriveFailure::Invalid(format!("source line binding: {error:?}")))?;
        let mut admission = <M11DirectBlockController as M11ExactController<
            SnapshotLineSource,
        >>::begin_source_line(controller, facts.identity())
        .map_err(map_source_controller_error)?;

        let mut matched = false;
        for _ in 0..MAX_POLLS {
            if source.access_budget() == 0 && source.position() < source.len() {
                source
                    .replenish_access_budget(SOURCE_CURSOR_WINDOW_BYTES)
                    .map_err(|error| {
                        DriveFailure::Invalid(format!("source budget replenish: {error:?}"))
                    })?;
            }
            let budget_before = source.access_budget();
            let position_before = source.position();
            let poll = <M11DirectBlockController as M11ExactController<
                SnapshotLineSource,
            >>::poll_source_line(controller, &mut admission, &mut source, FUEL)
            .map_err(map_source_controller_error)?;
            require(
                poll.lexical_work_units <= FUEL + M11_DIRECT_BLOCK_MAX_LEXICAL_SLACK,
                "lexical work exceeded fuel plus fixed scanner slack",
            )?;
            require(
                poll.source_first_reads <= FUEL,
                "source reads exceeded caller fuel",
            )?;
            require(
                budget_before - source.access_budget() == poll.source_first_reads,
                "source budget delta did not equal charged reads",
            )?;
            require(
                source.position() - position_before == poll.source_first_reads,
                "source position delta did not equal charged reads",
            )?;
            require(
                poll.physical_high_water <= source.position(),
                "grammar read beyond the source adapter high-water mark",
            )?;
            require(
                poll.retained_source_bytes <= M11_DIRECT_BLOCK_MAX_RETAINED_SOURCE_BYTES,
                "retained source exceeded the production cap",
            )?;
            require(
                poll.maximum_source_request_rewind_bytes == 0,
                "source scanner requested a rewind",
            )?;
            require(
                poll.source_budget_exhausted
                    == (source.access_budget() == 0 && source.position() < source.len()),
                "source exhaustion receipt disagreed with source authority",
            )?;
            if poll.status == M11SourceLinePollStatus::Matched {
                matched = true;
                break;
            }
        }
        require(matched, "source recognition did not converge")?;
        require(
            source.position() == source.len(),
            "source recognition returned a partial-line match",
        )?;

        <M11DirectBlockController as M11ExactController<SnapshotLineSource>>::commit_source_line(
            controller, admission, facts,
        )
        .map_err(map_source_controller_error)?;
        scanner = source
            .finish()
            .map_err(|error| DriveFailure::Invalid(format!("partial source line: {error:?}")))?;

        let mut line_complete = false;
        for _ in 0..MAX_POLLS {
            let poll = controller.poll_line(FUEL).map_err(map_direct_error)?;
            require(
                poll.transitions <= FUEL,
                "grammar transitions exceeded fuel",
            )?;
            match poll.status {
                M11DirectBlockPollStatus::Pending => {}
                M11DirectBlockPollStatus::CommandReady => {
                    write_pending_command(controller, writer, runtime)?;
                }
                M11DirectBlockPollStatus::ExternalWorkReady => {
                    drive_reference_rendezvous(controller, writer, journal, runtime)?;
                }
                M11DirectBlockPollStatus::Complete => {
                    line_complete = true;
                    break;
                }
            }
        }
        require(line_complete, "line grammar did not converge")?;
    }

    controller.begin_finish().map_err(map_direct_error)?;
    for _ in 0..MAX_POLLS {
        let poll = controller.poll_finish(FUEL).map_err(map_direct_error)?;
        require(poll.transitions <= FUEL, "finish transitions exceeded fuel")?;
        match poll.status {
            M11DirectBlockPollStatus::Pending => {}
            M11DirectBlockPollStatus::CommandReady => {
                write_pending_command(controller, writer, runtime)?;
            }
            M11DirectBlockPollStatus::ExternalWorkReady => {
                drive_reference_rendezvous(controller, writer, journal, runtime)?;
            }
            M11DirectBlockPollStatus::Complete => break,
        }
    }
    let mut green = writer.take_root().ok_or_else(|| {
        DriveFailure::Invalid("completed writer omitted its recursive Green root".into())
    })?;
    journal
        .finish_input(runtime)
        .map_err(|error| DriveFailure::Invalid(format!("finish reference journal: {error}")))?;
    let mut journal_complete = false;
    for _ in 0..MAX_POLLS {
        let poll = journal
            .poll(runtime, FUEL)
            .map_err(|error| DriveFailure::Invalid(format!("poll reference journal: {error}")))?;
        require(
            poll.transitions() <= FUEL,
            "reference journal transitions exceeded fuel",
        )?;
        if poll.status() == M11ReferenceJournalStatus::Complete {
            journal_complete = true;
            break;
        }
    }
    if !journal_complete {
        green
            .begin_release(runtime)
            .map_err(|error| DriveFailure::Invalid(format!("release failed Green: {error}")))?;
        while !green
            .poll_release(runtime, 64)
            .map_err(|error| DriveFailure::Invalid(format!("poll failed Green release: {error}")))?
            .complete()
        {}
        return Err(DriveFailure::Invalid(
            "reference journal did not converge".into(),
        ));
    }
    let references = journal.take_root().ok_or_else(|| {
        DriveFailure::Invalid("completed journal omitted its reference root".into())
    })?;
    Ok((green, references))
}

fn write_pending_command(
    controller: &mut M11DirectBlockController,
    writer: &mut M11BlockWriter,
    runtime: &mut DocumentRuntime,
) -> Result<(), DriveFailure> {
    require(
        controller.pending_command().is_some(),
        "controller reported a command without exposing it",
    )?;
    let command = *controller
        .pending_command()
        .ok_or_else(|| DriveFailure::Invalid("ready parser command disappeared".into()))?;
    match writer
        .offer_command(command)
        .map_err(|error| DriveFailure::Invalid(format!("writer command: {error}")))?
    {
        M11BlockWriterOfferStatus::Complete => {}
        M11BlockWriterOfferStatus::Pending => {
            let mut complete = false;
            for _ in 0..MAX_POLLS {
                let poll = writer
                    .poll(runtime, FUEL)
                    .map_err(|error| DriveFailure::Invalid(format!("writer poll: {error}")))?;
                require(
                    poll.transitions() <= FUEL,
                    "writer transitions exceeded fuel",
                )?;
                if matches!(
                    poll.status(),
                    M11BlockWriterPollStatus::CommandComplete
                        | M11BlockWriterPollStatus::DocumentComplete
                ) {
                    complete = true;
                    break;
                }
            }
            require(complete, "writer command did not converge")?;
        }
    }
    controller.acknowledge_command().map_err(map_direct_error)
}

fn drive_reference_rendezvous(
    controller: &mut M11DirectBlockController,
    writer: &mut M11BlockWriter,
    journal: &mut M11ReferenceJournal,
    runtime: &mut DocumentRuntime,
) -> Result<(), DriveFailure> {
    let mut rendezvous = M11ReferenceRendezvous::begin(controller, writer)
        .map_err(|error| DriveFailure::Invalid(format!("begin reference rendezvous: {error}")))?;
    for _ in 0..MAX_POLLS {
        let poll = rendezvous
            .poll(controller, writer, journal, runtime, FUEL)
            .map_err(|error| {
                DriveFailure::Invalid(format!("poll reference rendezvous: {error}"))
            })?;
        require(
            poll.transitions <= FUEL,
            "reference rendezvous transitions exceeded fuel",
        )?;
        if poll.status == M11ReferenceRendezvousStatus::Complete {
            return Ok(());
        }
    }
    Err(DriveFailure::Invalid(
        "reference rendezvous did not converge".into(),
    ))
}

fn cancel_pipeline(
    writer: &mut M11BlockWriter,
    journal: &mut M11ReferenceJournal,
    runtime: &mut DocumentRuntime,
) -> Result<(), DriveFailure> {
    writer
        .begin_cancel(runtime)
        .map_err(|error| DriveFailure::Invalid(format!("cancel writer: {error}")))?;
    journal
        .begin_cancel(runtime)
        .map_err(|error| DriveFailure::Invalid(format!("cancel references: {error}")))?;
    let mut writer_complete = false;
    let mut journal_complete = false;
    for _ in 0..MAX_POLLS {
        if !writer_complete {
            writer_complete = writer
                .poll_cancel(runtime, 64)
                .map_err(|error| DriveFailure::Invalid(format!("poll writer cancel: {error}")))?
                .complete();
        }
        if !journal_complete {
            journal_complete = journal
                .poll_cancel(runtime, 64)
                .map_err(|error| DriveFailure::Invalid(format!("poll reference cancel: {error}")))?
                .complete();
        }
        if writer_complete && journal_complete {
            return Ok(());
        }
    }
    Err(DriveFailure::Invalid(
        "pipeline cancellation did not converge".into(),
    ))
}

fn release_pipeline_roots(
    green: &mut M11RecursiveGreenRoot,
    references: &mut M11ReferenceJournalRoot,
    runtime: &mut DocumentRuntime,
) -> Result<(), DriveFailure> {
    green
        .begin_release(runtime)
        .map_err(|error| DriveFailure::Invalid(format!("release Green: {error}")))?;
    while !green
        .poll_release(runtime, 64)
        .map_err(|error| DriveFailure::Invalid(format!("poll Green release: {error}")))?
        .complete()
    {}
    references
        .begin_release(runtime)
        .map_err(|error| DriveFailure::Invalid(format!("release references: {error}")))?;
    while !references
        .poll_release(runtime, 64)
        .map_err(|error| DriveFailure::Invalid(format!("poll reference release: {error}")))?
        .complete()
    {}
    Ok(())
}

fn close_runtime(runtime: &mut DocumentRuntime) -> Result<(), DriveFailure> {
    runtime
        .begin_close()
        .map_err(|error| DriveFailure::Invalid(format!("begin runtime close: {error}")))?;
    for _ in 0..MAX_POLLS {
        if runtime
            .poll_close(64)
            .map_err(|error| DriveFailure::Invalid(format!("poll runtime close: {error}")))?
            .complete
        {
            return Ok(());
        }
    }
    Err(DriveFailure::Invalid(
        "runtime close did not converge".into(),
    ))
}

fn green_block_projection(
    markdown: &str,
    green: &M11RecursiveGreenRoot,
    runtime: &DocumentRuntime,
) -> Result<Vec<GreenBlock>, DriveFailure> {
    let mut blocks = Vec::<GreenBlock>::new();
    let mut stack = Vec::<usize>::new();
    let mut source_cursor = 0_usize;
    let mut failure = None::<String>;
    green
        .visit_semantic_events_for_diagnostics(runtime, |event| {
            if failure.is_some() {
                return;
            }
            if let Err(error) =
                apply_green_event(markdown, event, &mut blocks, &mut stack, &mut source_cursor)
            {
                failure = Some(error);
            }
        })
        .map_err(|error| DriveFailure::Invalid(format!("Green semantic replay: {error}")))?;
    if let Some(error) = failure {
        return Err(DriveFailure::Invalid(error));
    }
    require(stack.is_empty(), "Green semantic replay left open frames")?;
    require(
        source_cursor == markdown.len(),
        "Green semantic replay did not consume the complete source",
    )?;
    Ok(blocks)
}

#[derive(Debug)]
enum RenderFailure {
    Missing(&'static str),
    Invalid(String),
}

struct InlineProjection {
    source: String,
    facts: Vec<M11InlineProjectionFact>,
}

struct InlineNode {
    fact: M11InlineProjectionFact,
    children: Vec<usize>,
}

struct SemanticRenderer<'a> {
    markdown: &'a str,
    blocks: &'a [GreenBlock],
    green: &'a M11RecursiveGreenRoot,
    runtime: &'a mut DocumentRuntime,
    output: String,
}

fn semantic_outcome(
    markdown: &str,
    expected_html: &str,
    blocks: &[GreenBlock],
    green: &M11RecursiveGreenRoot,
    runtime: &mut DocumentRuntime,
) -> SemanticOutcome {
    let rendered = SemanticRenderer {
        markdown,
        blocks,
        green,
        runtime,
        output: String::new(),
    }
    .render();
    match rendered {
        Ok(actual) if actual == expected_html => SemanticOutcome::Exact,
        Ok(actual) => SemanticOutcome::Divergent(first_text_difference(expected_html, &actual)),
        Err(RenderFailure::Missing(mechanism)) => SemanticOutcome::Missing(mechanism),
        Err(RenderFailure::Invalid(error)) => SemanticOutcome::Divergent(error),
    }
}

impl SemanticRenderer<'_> {
    fn render(mut self) -> Result<String, RenderFailure> {
        let Some((root, _)) = self
            .blocks
            .iter()
            .enumerate()
            .find(|(_, block)| block.parent.is_none())
        else {
            return Err(RenderFailure::Invalid(
                "recursive Green omitted its Document frame".into(),
            ));
        };
        if self.blocks[root].kind != 1 {
            return Err(RenderFailure::Invalid(
                "recursive Green root is not a Document".into(),
            ));
        }
        self.render_children(root)?;
        Ok(self.output)
    }

    fn render_children(&mut self, parent: usize) -> Result<(), RenderFailure> {
        let children = self
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(ordinal, block)| (block.parent == Some(parent)).then_some(ordinal))
            .collect::<Vec<_>>();
        for child in children {
            self.render_block(child)?;
        }
        Ok(())
    }

    fn render_block(&mut self, ordinal: usize) -> Result<(), RenderFailure> {
        let kind = self.blocks[ordinal].kind;
        match kind {
            1 => self.render_children(ordinal),
            2 => {
                self.cr();
                self.output.push_str("<blockquote>");
                self.lf();
                self.render_children(ordinal)?;
                self.cr();
                self.output.push_str("</blockquote>");
                self.lf();
                Ok(())
            }
            3 => {
                let property = required_fact(&self.blocks[ordinal], 1, false)
                    .map_err(RenderFailure::Invalid)?;
                self.cr();
                match read_u8(property, 0).map_err(RenderFailure::Invalid)? {
                    1 => self.output.push_str("<ul>"),
                    2 => {
                        let start = read_u32(property, 4).map_err(RenderFailure::Invalid)?;
                        if start == 1 {
                            self.output.push_str("<ol>");
                        } else {
                            self.output.push_str("<ol start=\"");
                            self.output.push_str(&start.to_string());
                            self.output.push_str("\">");
                        }
                    }
                    _ => {
                        return Err(RenderFailure::Invalid(
                            "recursive Green list style is invalid".into(),
                        ));
                    }
                }
                self.lf();
                self.render_children(ordinal)?;
                self.output.push_str(
                    if read_u8(property, 0).map_err(RenderFailure::Invalid)? == 1 {
                        "</ul>"
                    } else {
                        "</ol>"
                    },
                );
                self.lf();
                Ok(())
            }
            4 => {
                self.cr();
                self.output.push_str("<li>");
                self.render_children(ordinal)?;
                self.output.push_str("</li>");
                self.lf();
                Ok(())
            }
            5 => self.render_paragraph(ordinal),
            6 => {
                let literal = self.blocks[ordinal].logical.clone();
                self.render_code("", &literal)
            }
            7 => {
                let block = &self.blocks[ordinal];
                let close = required_fact(block, 4, true).map_err(RenderFailure::Invalid)?;
                let info_end = read_u64(close, 1).map_err(RenderFailure::Invalid)?;
                let literal_start = read_u64(close, 17).map_err(RenderFailure::Invalid)?;
                let logical_end = read_u64(close, 33).map_err(RenderFailure::Invalid)?;
                let info = logical_slice(&block.logical, 0, info_end)
                    .map_err(RenderFailure::Invalid)?
                    .to_owned();
                let literal = logical_slice(&block.logical, literal_start, logical_end)
                    .map_err(RenderFailure::Invalid)?
                    .to_owned();
                self.render_code(&info, &literal)
            }
            8 => {
                let literal = self.blocks[ordinal].logical.clone();
                self.cr();
                self.output.push_str(&literal);
                self.cr();
                Ok(())
            }
            12 => Err(RenderFailure::Missing("heading-inline-authority")),
            13 => {
                self.cr();
                self.output.push_str("<hr />");
                self.lf();
                Ok(())
            }
            other => Err(RenderFailure::Invalid(format!(
                "recursive Green carries unknown block kind {other}"
            ))),
        }
    }

    fn render_paragraph(&mut self, ordinal: usize) -> Result<(), RenderFailure> {
        let tight = self.paragraph_is_tight(ordinal)?;
        let inline = project_paragraph_inline(
            self.markdown,
            &self.blocks[ordinal],
            self.green,
            self.runtime,
        )?;
        if !tight {
            self.cr();
            self.output.push_str("<p>");
        }
        render_inline_projection(&mut self.output, &inline)?;
        if !tight {
            self.output.push_str("</p>");
            self.lf();
        }
        Ok(())
    }

    fn paragraph_is_tight(&self, paragraph: usize) -> Result<bool, RenderFailure> {
        let Some(item) = self.blocks[paragraph].parent else {
            return Ok(false);
        };
        if self.blocks[item].kind != 4 {
            return Ok(false);
        }
        let Some(list) = self.blocks[item].parent else {
            return Ok(false);
        };
        if self.blocks[list].kind != 3 {
            return Ok(false);
        }
        let close = required_fact(&self.blocks[list], 1, true).map_err(RenderFailure::Invalid)?;
        read_bool(close, 0).map_err(RenderFailure::Invalid)
    }

    fn render_code(&mut self, info: &str, literal: &str) -> Result<(), RenderFailure> {
        let info = comrak::block_spine_facade::normalize_code_info(info).map_err(|error| {
            RenderFailure::Invalid(format!("code info normalization: {error:?}"))
        })?;
        self.cr();
        self.output.push_str("<pre><code");
        if !info.is_empty() {
            let language_end = info
                .as_bytes()
                .iter()
                .position(u8::is_ascii_whitespace)
                .unwrap_or(info.len());
            self.output.push_str(" class=\"language-");
            push_escaped_html(&mut self.output, &info[..language_end]);
            self.output.push('"');
        }
        self.output.push('>');
        push_escaped_html(&mut self.output, literal);
        self.output.push_str("</code></pre>");
        self.lf();
        Ok(())
    }

    fn cr(&mut self) {
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }

    fn lf(&mut self) {
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }
}

fn project_paragraph_inline(
    markdown: &str,
    block: &GreenBlock,
    green: &M11RecursiveGreenRoot,
    runtime: &mut DocumentRuntime,
) -> Result<InlineProjection, RenderFailure> {
    if block.logical.is_empty() {
        return Ok(InlineProjection {
            source: String::new(),
            facts: Vec::new(),
        });
    }
    let point = block
        .first_owned_logical_byte
        .ok_or(RenderFailure::Missing("projected-inline-authority"))?;
    let utf16 = markdown
        .get(..point)
        .ok_or_else(|| RenderFailure::Invalid("Paragraph point is not UTF-8 aligned".into()))?
        .encode_utf16()
        .count();
    let limits = M11RecursiveGreenFrameQueryLimits::new(4096, 1_000_000, 4096, 1_000_000)
        .expect("nonzero semantic gate limits");
    let fence = resolve_m11_recursive_green_paragraph_fence(
        runtime,
        green,
        M11RecursiveGreenPoint::new(point, utf16, SourceBoundaryAffinity::After),
        limits,
    )
    .map_err(|_| RenderFailure::Missing("projected-inline-authority"))?
    .ok_or(RenderFailure::Missing("projected-inline-authority"))?;
    let profile = ParserProfileId::new(1).expect("nonzero parser profile");
    let mut job = M11InlineProjectionJob::new_for_recursive_green_paragraph(
        runtime,
        fence,
        M11ParserBinding::current(profile),
    )
    .map_err(|error| RenderFailure::Invalid(format!("inline job creation: {error}")))?;
    let mut complete = false;
    for _ in 0..MAX_POLLS {
        let poll = job
            .poll(runtime, FUEL)
            .map_err(|error| RenderFailure::Invalid(format!("inline job poll: {error}")))?;
        if poll.status() == M11InlineProjectionJobPollStatus::Complete {
            complete = true;
            break;
        }
    }
    if !complete {
        return Err(RenderFailure::Invalid(
            "inline projection job did not converge".into(),
        ));
    }
    let output = job
        .take_output()
        .ok_or_else(|| RenderFailure::Invalid("inline job omitted output".into()))?;
    let (_, range, _, authority, publication) = output.into_publication_parts().into_parts();
    let source = markdown
        .get(
            usize::try_from(range.start)
                .map_err(|_| RenderFailure::Invalid("inline range start exceeds usize".into()))?
                ..usize::try_from(range.end)
                    .map_err(|_| RenderFailure::Invalid("inline range end exceeds usize".into()))?,
        )
        .ok_or_else(|| RenderFailure::Invalid("inline range is not a source UTF-8 cut".into()))?
        .to_owned();
    let result = match publication {
        M11InlineProjectionPublication::Unsupported(record) => {
            let _ = record.into_encoded();
            if block.logical.contains('[') || block.logical.contains(']') {
                Err(RenderFailure::Missing("reference-resolver"))
            } else {
                Err(RenderFailure::Missing("inline-fail-closed"))
            }
        }
        M11InlineProjectionPublication::Authoritative(mut root) => {
            let mut cursor = root
                .cursor(runtime, green.source(), profile)
                .map_err(|error| RenderFailure::Invalid(format!("inline cursor: {error}")))?;
            let mut facts = Vec::new();
            loop {
                match cursor
                    .poll(runtime)
                    .map_err(|error| RenderFailure::Invalid(format!("inline replay: {error}")))?
                {
                    M11InlineProjectionCursorPoll::Pending { .. } => {}
                    M11InlineProjectionCursorPoll::Fact { fact, .. } => facts.push(fact),
                    M11InlineProjectionCursorPoll::Complete { .. } => break,
                }
            }
            drop(cursor);
            let missing = facts.iter().find_map(|fact| match fact.kind() {
                M11InlineProjectionKind::DirectLink
                | M11InlineProjectionKind::DirectImage
                | M11InlineProjectionKind::ReferenceLink
                | M11InlineProjectionKind::ReferenceImage => Some("inline-link-value-replay"),
                M11InlineProjectionKind::AutolinkUri | M11InlineProjectionKind::AutolinkEmail => {
                    Some("autolink-render-value")
                }
                _ => None,
            });
            root.begin_release(runtime)
                .map_err(|error| RenderFailure::Invalid(format!("release inline root: {error}")))?;
            while !root
                .poll_release(runtime, 64)
                .map_err(|error| {
                    RenderFailure::Invalid(format!("poll inline root release: {error}"))
                })?
                .complete()
            {}
            if let Some(mechanism) = missing {
                Err(RenderFailure::Missing(mechanism))
            } else {
                Ok(InlineProjection { source, facts })
            }
        }
    };
    drop(authority);
    drop(job);
    result
}

fn render_inline_projection(
    output: &mut String,
    projection: &InlineProjection,
) -> Result<(), RenderFailure> {
    let (nodes, roots) = inline_forest(&projection.facts)?;
    render_inline_range(
        output,
        &projection.source,
        &nodes,
        &roots,
        0..projection.source.len(),
    )
}

fn inline_forest(
    facts: &[M11InlineProjectionFact],
) -> Result<(Vec<InlineNode>, Vec<usize>), RenderFailure> {
    let mut nodes = Vec::<InlineNode>::new();
    let mut roots = Vec::<usize>::new();
    let mut stack = Vec::<usize>::new();
    for fact in facts.iter().copied() {
        let range = fact.relative_range();
        while let Some(parent) = stack.last().copied() {
            let content = nodes[parent].fact.relative_content_range();
            if range.start >= content.start && range.end <= content.end {
                break;
            }
            stack.pop();
        }
        let ordinal = nodes.len();
        nodes.push(InlineNode {
            fact,
            children: Vec::new(),
        });
        if let Some(parent) = stack.last().copied() {
            nodes[parent].children.push(ordinal);
        } else {
            roots.push(ordinal);
        }
        let content = fact.relative_content_range();
        if content.start < content.end
            && matches!(
                fact.kind(),
                M11InlineProjectionKind::Emphasis | M11InlineProjectionKind::Strong
            )
        {
            stack.push(ordinal);
        }
    }
    Ok((nodes, roots))
}

fn render_inline_range(
    output: &mut String,
    source: &str,
    nodes: &[InlineNode],
    children: &[usize],
    range: std::ops::Range<usize>,
) -> Result<(), RenderFailure> {
    let mut cursor = range.start;
    for child in children.iter().copied() {
        let fact_range = u32_range_to_usize(nodes[child].fact.relative_range())?;
        if fact_range.start < cursor || fact_range.end > range.end {
            return Err(RenderFailure::Invalid(
                "inline facts overlap or leave their parent content".into(),
            ));
        }
        push_inline_text(
            output,
            source.get(cursor..fact_range.start).ok_or_else(|| {
                RenderFailure::Invalid("inline text gap is not UTF-8 aligned".into())
            })?,
        );
        render_inline_fact(output, source, nodes, child)?;
        cursor = fact_range.end;
    }
    push_inline_text(
        output,
        source.get(cursor..range.end).ok_or_else(|| {
            RenderFailure::Invalid("inline trailing text is not UTF-8 aligned".into())
        })?,
    );
    Ok(())
}

fn render_inline_fact(
    output: &mut String,
    source: &str,
    nodes: &[InlineNode],
    ordinal: usize,
) -> Result<(), RenderFailure> {
    let node = &nodes[ordinal];
    let content = u32_range_to_usize(node.fact.relative_content_range())?;
    match node.fact.kind() {
        M11InlineProjectionKind::Emphasis => {
            output.push_str("<em>");
            render_inline_range(output, source, nodes, &node.children, content)?;
            output.push_str("</em>");
        }
        M11InlineProjectionKind::Strong => {
            output.push_str("<strong>");
            render_inline_range(output, source, nodes, &node.children, content)?;
            output.push_str("</strong>");
        }
        M11InlineProjectionKind::Code => {
            let mut value = source
                .get(content)
                .ok_or_else(|| RenderFailure::Invalid("code content is not UTF-8 aligned".into()))?
                .replace("\r\n", " ")
                .replace(['\r', '\n'], " ");
            if node.fact.flags() & 2 != 0 && value.starts_with(' ') && value.ends_with(' ') {
                value.remove(0);
                value.pop();
            }
            output.push_str("<code>");
            push_escaped_html(output, &value);
            output.push_str("</code>");
        }
        M11InlineProjectionKind::BackslashEscape => {
            push_inline_text(
                output,
                source.get(content).ok_or_else(|| {
                    RenderFailure::Invalid("escape content is not UTF-8 aligned".into())
                })?,
            );
        }
        M11InlineProjectionKind::HardLineBreak => output.push_str("<br />\n"),
        M11InlineProjectionKind::CharacterReference => {
            let (first, second) = node.fact.character_reference().ok_or_else(|| {
                RenderFailure::Invalid("character reference omitted cooked scalars".into())
            })?;
            push_escaped_html(output, &first.to_string());
            if let Some(second) = second {
                push_escaped_html(output, &second.to_string());
            }
        }
        M11InlineProjectionKind::Strikethrough
        | M11InlineProjectionKind::AutolinkUri
        | M11InlineProjectionKind::AutolinkEmail
        | M11InlineProjectionKind::DirectLink
        | M11InlineProjectionKind::DirectImage
        | M11InlineProjectionKind::ReferenceLink
        | M11InlineProjectionKind::ReferenceImage => {
            return Err(RenderFailure::Invalid(
                "preflight allowed an unsupported inline render fact".into(),
            ));
        }
    }
    Ok(())
}

fn u32_range_to_usize(
    range: std::ops::Range<u32>,
) -> Result<std::ops::Range<usize>, RenderFailure> {
    Ok(usize::try_from(range.start)
        .map_err(|_| RenderFailure::Invalid("inline start exceeds usize".into()))?
        ..usize::try_from(range.end)
            .map_err(|_| RenderFailure::Invalid("inline end exceeds usize".into()))?)
}

fn push_inline_text(output: &mut String, text: &str) {
    let normalized = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\0', "\u{fffd}");
    push_escaped_html(output, &normalized);
}

fn push_escaped_html(output: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            _ => output.push(character),
        }
    }
}

fn first_text_difference(expected: &str, actual: &str) -> String {
    let offset = expected
        .as_bytes()
        .iter()
        .zip(actual.as_bytes())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| expected.len().min(actual.len()));
    let expected_end = (offset + 80).min(expected.len());
    let actual_end = (offset + 80).min(actual.len());
    format!(
        "HTML differs at byte {offset}: expected={:?} actual={:?}",
        expected.get(offset..expected_end),
        actual.get(offset..actual_end),
    )
}

fn apply_green_event(
    markdown: &str,
    event: M11RecursiveGreenEvent,
    blocks: &mut Vec<GreenBlock>,
    stack: &mut Vec<usize>,
    source_cursor: &mut usize,
) -> Result<(), String> {
    match event {
        M11RecursiveGreenEvent::Enter { frame, kind } => {
            let ordinal = blocks.len();
            blocks.push(GreenBlock {
                frame,
                parent: stack.last().copied(),
                kind: kind.get(),
                property: None,
                close: None,
                logical: String::new(),
                first_owned_logical_byte: None,
            });
            stack.push(ordinal);
        }
        M11RecursiveGreenEvent::Property(property) => {
            let ordinal = *stack
                .last()
                .ok_or_else(|| "Green property has no open frame".to_owned())?;
            blocks[ordinal].property = Some((property.tag().get(), property.as_bytes().to_vec()));
        }
        M11RecursiveGreenEvent::Coverage {
            physical,
            owner_depth,
            logical,
            ..
        } => {
            let physical_len = usize::try_from(physical.bytes())
                .map_err(|_| "Green physical length exceeds usize".to_owned())?;
            let end = source_cursor
                .checked_add(physical_len)
                .ok_or_else(|| "Green source cursor overflow".to_owned())?;
            let physical_text = markdown
                .get(*source_cursor..end)
                .ok_or_else(|| "Green coverage is not a source UTF-8 cut".to_owned())?;
            let physical_owner = stack
                .len()
                .checked_sub(
                    usize::try_from(owner_depth)
                        .map_err(|_| "Green owner depth exceeds usize".to_owned())?
                        + 1,
                )
                .and_then(|index| stack.get(index).copied())
                .ok_or_else(|| "Green coverage owner is outside the open path".to_owned())?;
            match logical {
                M11RecursiveGreenLogicalAction::None
                | M11RecursiveGreenLogicalAction::HiddenUpstream => {}
                M11RecursiveGreenLogicalAction::Identity => {
                    blocks[physical_owner]
                        .first_owned_logical_byte
                        .get_or_insert(*source_cursor);
                    blocks[physical_owner].logical.push_str(physical_text);
                }
                M11RecursiveGreenLogicalAction::CanonicalText => {
                    if physical_text != "\0" {
                        return Err("canonical-text diagnostic atom is not one NUL".into());
                    }
                    blocks[physical_owner]
                        .first_owned_logical_byte
                        .get_or_insert(*source_cursor);
                    blocks[physical_owner].logical.push('\u{fffd}');
                }
                M11RecursiveGreenLogicalAction::CanonicalNewline => {
                    blocks[physical_owner]
                        .first_owned_logical_byte
                        .get_or_insert(*source_cursor);
                    blocks[physical_owner].logical.push('\n');
                }
                M11RecursiveGreenLogicalAction::PartialTab {
                    target_owner_depth,
                    remaining_spaces,
                } => {
                    if physical_text != "\t" {
                        return Err("partial-tab diagnostic atom is not one tab".into());
                    }
                    let logical_owner = stack
                        .len()
                        .checked_sub(
                            usize::try_from(target_owner_depth).map_err(|_| {
                                "Green logical owner depth exceeds usize".to_owned()
                            })? + 1,
                        )
                        .and_then(|index| stack.get(index).copied())
                        .ok_or_else(|| {
                            "Green partial-tab owner is outside the open path".to_owned()
                        })?;
                    if logical_owner == physical_owner {
                        blocks[logical_owner]
                            .first_owned_logical_byte
                            .get_or_insert(*source_cursor);
                    }
                    blocks[logical_owner]
                        .logical
                        .extend(std::iter::repeat_n(' ', usize::from(remaining_spaces)));
                }
            }
            *source_cursor = end;
        }
        M11RecursiveGreenEvent::RetypeOpen {
            frame,
            kind,
            property,
        } => {
            let ordinal = *stack
                .last()
                .ok_or_else(|| "Green retype has no open frame".to_owned())?;
            if blocks[ordinal].frame != frame {
                return Err("Green retype targets a non-current frame".into());
            }
            blocks[ordinal].kind = kind.get();
            blocks[ordinal].property =
                property.map(|property| (property.tag().get(), property.as_bytes().to_vec()));
        }
        M11RecursiveGreenEvent::Exit {
            frame,
            final_kind,
            close,
            ..
        } => {
            let ordinal = stack
                .pop()
                .ok_or_else(|| "Green exit has no open frame".to_owned())?;
            if blocks[ordinal].frame != frame {
                return Err("Green exit targets a non-current frame".into());
            }
            blocks[ordinal].kind = final_kind.get();
            blocks[ordinal].close =
                close.map(|facts| (facts.tag().get(), facts.as_bytes().to_vec()));
        }
    }
    Ok(())
}

fn required_fact(block: &GreenBlock, tag: u16, close: bool) -> Result<&[u8], String> {
    let fact = if close {
        block.close.as_ref()
    } else {
        block.property.as_ref()
    }
    .ok_or_else(|| format!("Green kind {} omitted fact tag {tag}", block.kind))?;
    if fact.0 != tag {
        return Err(format!(
            "Green kind {} used fact tag {}, expected {tag}",
            block.kind, fact.0
        ));
    }
    Ok(&fact.1)
}

fn read_u8(bytes: &[u8], offset: usize) -> Result<u8, String> {
    bytes
        .get(offset)
        .copied()
        .ok_or_else(|| "Green fact is truncated".to_owned())
}

fn read_bool(bytes: &[u8], offset: usize) -> Result<bool, String> {
    match read_u8(bytes, offset)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err("Green Boolean fact is invalid".into()),
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "Green u32 fact is truncated".to_owned())?;
    Ok(u32::from_le_bytes(slice.try_into().expect("four bytes")))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| "Green u64 fact is truncated".to_owned())?;
    Ok(u64::from_le_bytes(slice.try_into().expect("eight bytes")))
}

fn logical_slice(logical: &str, start: u64, end: u64) -> Result<&str, String> {
    let start = usize::try_from(start).map_err(|_| "logical start exceeds usize".to_owned())?;
    let end = usize::try_from(end).map_err(|_| "logical end exceeds usize".to_owned())?;
    logical
        .get(start..end)
        .ok_or_else(|| "Green logical fact is not a valid UTF-8 slice".to_owned())
}

fn map_direct_error(error: M11DirectBlockError) -> DriveFailure {
    match error {
        M11DirectBlockError::Unsupported(reason) => DriveFailure::Unsupported(reason),
        other => DriveFailure::Invalid(format!("controller: {other:?}")),
    }
}

fn map_source_controller_error(
    error: M11DirectBlockControllerError<SourceAdapterError>,
) -> DriveFailure {
    match error {
        M11DirectBlockControllerError::Controller(M11DirectBlockError::Unsupported(reason)) => {
            DriveFailure::Unsupported(reason)
        }
        other => DriveFailure::Invalid(format!("source controller: {other:?}")),
    }
}

fn require(condition: bool, message: &str) -> Result<(), DriveFailure> {
    if condition {
        Ok(())
    } else {
        Err(DriveFailure::Invalid(message.into()))
    }
}

fn panic_message(panic: &Box<dyn Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .map_or_else(
            || "controller panicked".into(),
            |message| format!("controller panic: {message}"),
        )
}

fn load_fixtures(json: &str) -> Vec<Fixture> {
    const MARKDOWN: &str = "    \"markdown\": ";
    const HTML: &str = "\n    \"html\": ";
    const EXAMPLE: &str = "\n    \"example\": ";
    const SECTION: &str = "\n    \"section\": ";
    let mut fixtures = Vec::new();
    let mut cursor = 0;
    while let Some(markdown_field) = json[cursor..].find(MARKDOWN) {
        cursor += markdown_field + MARKDOWN.len();
        let markdown = json_string(json, &mut cursor);
        cursor += json[cursor..].find(HTML).expect("fixture HTML field") + HTML.len();
        let html = json_string(json, &mut cursor);
        cursor += json[cursor..].find(EXAMPLE).expect("fixture example field") + EXAMPLE.len();
        let number_end = cursor
            + json[cursor..]
                .find(|character: char| !character.is_ascii_digit())
                .expect("fixture example delimiter");
        let example = json[cursor..number_end]
            .parse()
            .expect("fixture example number");
        cursor = number_end;
        cursor += json[cursor..].find(SECTION).expect("fixture section field") + SECTION.len();
        let section = json_string(json, &mut cursor);
        fixtures.push(Fixture {
            markdown,
            html,
            example,
            section,
        });
    }
    fixtures
}

fn json_string(json: &str, cursor: &mut usize) -> String {
    let bytes = json.as_bytes();
    assert_eq!(bytes.get(*cursor), Some(&b'\"'), "JSON string starts here");
    *cursor += 1;
    let mut output = String::new();
    let mut literal_start = *cursor;
    loop {
        match bytes.get(*cursor).copied().expect("terminated JSON string") {
            b'\"' => {
                output.push_str(&json[literal_start..*cursor]);
                *cursor += 1;
                return output;
            }
            b'\\' => {
                output.push_str(&json[literal_start..*cursor]);
                *cursor += 1;
                let escaped = bytes.get(*cursor).copied().expect("JSON escape byte");
                *cursor += 1;
                match escaped {
                    b'\"' => output.push('\"'),
                    b'\\' => output.push('\\'),
                    b'/' => output.push('/'),
                    b'b' => output.push('\u{0008}'),
                    b'f' => output.push('\u{000c}'),
                    b'n' => output.push('\n'),
                    b'r' => output.push('\r'),
                    b't' => output.push('\t'),
                    b'u' => output.push(decode_json_scalar(bytes, cursor)),
                    _ => panic!("invalid JSON escape"),
                }
                literal_start = *cursor;
            }
            _ => *cursor += 1,
        }
    }
}

fn decode_json_scalar(bytes: &[u8], cursor: &mut usize) -> char {
    let first = decode_hex_quad(bytes, cursor);
    let scalar = if (0xd800..=0xdbff).contains(&first) {
        assert_eq!(bytes.get(*cursor..*cursor + 2), Some(&b"\\u"[..]));
        *cursor += 2;
        let second = decode_hex_quad(bytes, cursor);
        assert!((0xdc00..=0xdfff).contains(&second));
        0x1_0000 + ((u32::from(first) - 0xd800) << 10) + (u32::from(second) - 0xdc00)
    } else {
        u32::from(first)
    };
    char::from_u32(scalar).expect("valid JSON Unicode scalar")
}

fn decode_hex_quad(bytes: &[u8], cursor: &mut usize) -> u16 {
    let end = *cursor + 4;
    let digits = std::str::from_utf8(bytes.get(*cursor..end).expect("four JSON hex digits"))
        .expect("ASCII JSON hex digits");
    *cursor = end;
    u16::from_str_radix(digits, 16).expect("valid JSON hex digits")
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// SPDX-License-Identifier: MIT

use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet},
    panic::{catch_unwind, AssertUnwindSafe},
};

use flark_engine::parser_internal::{
    M11InlineLinkValue, M11InlineProjectionFact, M11InlineProjectionKind, M11RecursiveGreenEvent,
    M11RecursiveGreenFrameId, M11RecursiveGreenLogicalAction, M11RecursiveGreenPoint,
    M11RecursiveGreenRoot, M11RecursiveGreenRowQueryLimits, M11ReferenceJournal,
    M11ReferenceJournalRoot, M11ReferenceJournalStatus, M11ReferenceResolution,
    M11ReferenceResolver, M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW,
};
use flark_engine::{
    DocumentRuntime, DocumentRuntimeConfig, ParserProfileId, SourceBoundaryAffinity,
    SOURCE_CURSOR_WINDOW_BYTES,
};
use flark_parser::block_core::{
    resolve_m11_recursive_green_inline_leaf_row_fence, M11BlockWriter, M11BlockWriterOfferStatus,
    M11BlockWriterPollStatus, M11DirectBlockController, M11DirectBlockControllerError,
    M11DirectBlockError, M11DirectBlockPollStatus, M11DirectBlockUnsupported,
    M11ReferenceRendezvous, M11ReferenceRendezvousStatus, M11_DIRECT_BLOCK_MAX_LEXICAL_SLACK,
    M11_DIRECT_BLOCK_MAX_RETAINED_SOURCE_BYTES,
};
use flark_parser::{
    project_m11_gfm_inline, project_m11_gfm_table, M11ExactController, M11GfmInlineNode,
    M11GfmInlineOptions, M11GfmInlineReference, M11GfmTableAlignment, M11GfmTableProjection,
    M11InlineProjectionJob, M11InlineProjectionJobPollStatus, M11InlineProjectionOutcome,
    M11ParserBinding, M11SourceLinePollStatus, M11SourceLineSource, SnapshotLinePoll,
    SnapshotLineScanner, SnapshotLineSource, SourceAdapterError,
};
use sha2::{Digest, Sha256};

const COMMONMARK_FIXTURES: &str =
    include_str!("../../../../../test/fixtures/commonmark/upstream/common_mark_tests.json");
// The normative GFM lane selects the production GFM controller. Complex leaf
// grammar crosses only the bounded typed projector seam; it is never delegated
// to the bundled AST renderer or reparsed outside Rust.
const GFM_FIXTURES: &str =
    include_str!("../../../../../test/fixtures/commonmark/upstream/gfm_tests.json");
const GFM_TASK_LIST_SUPPLEMENT: &str =
    include_str!("../../../../../test/fixtures/v4/task_list_profile_cases_v1.json");
const COMMONMARK_FIXTURE_SHA256: &str =
    "d431b29d97b6f73e69d547109cf5081578fac931e72afe95639ebe766c1b2a20";
const GFM_FIXTURE_SHA256: &str = "ce09eea1c15b61235868465468f6281ec82ab177998e404d9143e1641c4e5b55";
const GFM_TASK_LIST_SUPPLEMENT_SHA256: &str =
    "8a735bd2ce45b2cea42a687f6425d0519f8c9b2a62f77d3cb37b9e404c3e9a69";
const EXPECTED_EXAMPLES: usize = 652;
const EXPECTED_GFM_EXAMPLES: usize = 672;
const FUEL: usize = 7;
const MAX_POLLS: usize = 1_000_000;

// One snapshot update records an intentional production-grammar promotion.
// Non-explicit failures remain a hard failure below and receive no coverage.
const EXPECTED_ADMITTED: usize = 652;
const EXPECTED_UNSUPPORTED: usize = 0;
const EXPECTED_RECEIPT_SHA256: &str =
    "b7b35caa97e225e86ca8d4d96275a8cc891135fa942e5bb9448a2d45aba7b79a";
const EXPECTED_SEMANTIC_RENDER_EXACT: usize = 652;
const EXPECTED_SEMANTIC_RENDER_DIVERGENT: usize = 0;
const EXPECTED_SEMANTIC_RECEIPT_SHA256: &str =
    "360e43e90532263e859914c369e25980f43f05b1330b70c45d41f8182a36498f";

// First complete deterministic GFM 0.29-gfm product-parser receipt. Any
// non-explicit result remains a hard failure.
const EXPECTED_GFM_ADMITTED: usize = 672;
const EXPECTED_GFM_UNSUPPORTED: usize = 0;
const EXPECTED_GFM_RECEIPT_SHA256: &str =
    "6914839f85b9b1f199dddf7754f77aa94e0e9d914a3826910850c0875dbb6ee4";
const EXPECTED_GFM_SEMANTIC_RENDER_EXACT: usize = 672;
const EXPECTED_GFM_SEMANTIC_RENDER_DIVERGENT: usize = 0;
const EXPECTED_GFM_SEMANTIC_RECEIPT_SHA256: &str =
    "076d0465102cbd46e81e0bf9cb3f3b26fdac32964eaa44e740cc999998a0207d";

#[derive(Debug)]
struct Fixture {
    markdown: String,
    html: String,
    example: usize,
    section: String,
    extensions: Vec<String>,
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
fn commonmark_200_empty_destination_retains_its_source_point() {
    drive_document("[foo]: <>\n\n[foo]\n")
        .expect("CM200 retains an authenticated zero-width destination range");
}

#[test]
fn commonmark_215_setext_resolution_skips_unsafe_remainder_checkpoint() {
    drive_document("[foo]: /url\nbar\n===\n[foo]\n")
        .expect("CM215 remains authoritative when the Setext line is active");
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
        let result = catch_unwind(AssertUnwindSafe(|| drive_fixture(fixture, false)))
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
    let expected_missing = BTreeMap::new();
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

#[test]
fn production_controller_gfm_0_29_semantic_coverage_is_complete_and_fail_closed() {
    assert_eq!(sha256(GFM_FIXTURES.as_bytes()), GFM_FIXTURE_SHA256);
    assert_eq!(
        sha256(GFM_TASK_LIST_SUPPLEMENT.as_bytes()),
        GFM_TASK_LIST_SUPPLEMENT_SHA256
    );
    let fixtures = gfm_fixtures();
    assert_eq!(fixtures.len(), EXPECTED_GFM_EXAMPLES);
    assert_eq!(
        fixtures
            .iter()
            .map(|fixture| fixture.example)
            .collect::<Vec<_>>(),
        (1..=EXPECTED_GFM_EXAMPLES).collect::<Vec<_>>(),
        "the selected GFM profile must own every official example exactly once"
    );

    let mut admitted = 0;
    let mut unsupported = 0;
    let mut semantic_exact = 0;
    let mut semantic_missing = BTreeMap::<String, usize>::new();
    let mut semantic_missing_examples = Vec::new();
    let mut semantic_divergent = Vec::new();
    let mut invalid = Vec::new();
    let mut receipt = String::new();
    let mut semantic_receipt = String::new();

    for fixture in &fixtures {
        let result = catch_unwind(AssertUnwindSafe(|| drive_fixture(fixture, true)))
            .unwrap_or_else(|panic| Err(DriveFailure::Invalid(panic_message(&panic))));
        match result {
            Ok(drive) => {
                admitted += 1;
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
                        semantic_missing_examples.push(format!(
                            "{} ({:?}): {mechanism}",
                            fixture.example, fixture.section
                        ));
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

    let receipt_sha256 = sha256(receipt.as_bytes());
    let semantic_receipt_sha256 = sha256(semantic_receipt.as_bytes());
    eprintln!(
        "GFM 0.29-gfm receipt: admitted={admitted} unsupported={unsupported} invalid={} exact={semantic_exact} missing={semantic_missing:?} divergent={} admission_sha256={receipt_sha256} semantic_sha256={semantic_receipt_sha256}",
        invalid.len(),
        semantic_divergent.len(),
    );
    assert_eq!(
        admitted + unsupported + invalid.len(),
        EXPECTED_GFM_EXAMPLES
    );
    assert!(
        invalid.is_empty(),
        "GFM lane rejected non-explicit failures instead of classifying them:\n{}",
        invalid.join("\n")
    );
    let expected_missing = BTreeMap::new();
    assert_eq!(
        (
            admitted,
            unsupported,
            receipt_sha256.as_str(),
            semantic_exact,
            &semantic_missing,
            semantic_divergent.len(),
            semantic_receipt_sha256.as_str(),
        ),
        (
            EXPECTED_GFM_ADMITTED,
            EXPECTED_GFM_UNSUPPORTED,
            EXPECTED_GFM_RECEIPT_SHA256,
            EXPECTED_GFM_SEMANTIC_RENDER_EXACT,
            &expected_missing,
            EXPECTED_GFM_SEMANTIC_RENDER_DIVERGENT,
            EXPECTED_GFM_SEMANTIC_RECEIPT_SHA256,
        ),
        "review the deterministic GFM receipt before updating its snapshot:\nmissing:\n{}\n\ndivergent:\n{}",
        semantic_missing_examples.join("\n"),
        semantic_divergent.join("\n")
    );
    assert_eq!(
        semantic_exact + semantic_missing.values().sum::<usize>() + semantic_divergent.len(),
        admitted,
        "the GFM semantic ledger must classify every admitted fixture exactly once"
    );
}

fn drive_document(markdown: &str) -> Result<(), DriveFailure> {
    drive(markdown, None, false, &[]).map(|_| ())
}

fn drive_fixture(fixture: &Fixture, gfm: bool) -> Result<DriveReceipt, DriveFailure> {
    drive(
        &fixture.markdown,
        Some(&fixture.html),
        gfm,
        &fixture.extensions,
    )
}

fn drive(
    markdown: &str,
    expected_html: Option<&str>,
    gfm: bool,
    extensions: &[String],
) -> Result<DriveReceipt, DriveFailure> {
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
    let mut controller = if gfm {
        M11DirectBlockController::new_gfm()
    } else {
        M11DirectBlockController::new()
    }
    .map_err(map_direct_error)?;
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
            Ok(blocks) => semantic_outcome(
                markdown,
                expected_html,
                gfm,
                extensions,
                &blocks,
                &green,
                &references,
                &mut runtime,
            ),
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
    link_values: Vec<M11InlineLinkValue>,
}

struct InlineNode {
    fact: M11InlineProjectionFact,
    children: Vec<usize>,
}

struct SemanticRenderer<'a> {
    markdown: &'a str,
    blocks: &'a [GreenBlock],
    green: &'a M11RecursiveGreenRoot,
    references: &'a M11ReferenceJournalRoot,
    runtime: &'a mut DocumentRuntime,
    gfm: bool,
    extensions: &'a [String],
    output: String,
}

fn semantic_outcome(
    markdown: &str,
    expected_html: &str,
    gfm: bool,
    extensions: &[String],
    blocks: &[GreenBlock],
    green: &M11RecursiveGreenRoot,
    references: &M11ReferenceJournalRoot,
    runtime: &mut DocumentRuntime,
) -> SemanticOutcome {
    let rendered = SemanticRenderer {
        markdown,
        blocks,
        green,
        references,
        runtime,
        gfm,
        extensions,
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
                if let Some(checked) = item_task_checked(&self.blocks[ordinal])? {
                    if checked {
                        self.output
                            .push_str("<input checked=\"\" disabled=\"\" type=\"checkbox\"> ");
                    } else {
                        self.output
                            .push_str("<input disabled=\"\" type=\"checkbox\"> ");
                    }
                }
                self.render_children(ordinal)?;
                self.output.push_str("</li>");
                self.lf();
                Ok(())
            }
            5 => self.render_paragraph(ordinal),
            6 => {
                let mut literal = self.blocks[ordinal]
                    .logical
                    .trim_end_matches(['\r', '\n'])
                    .to_owned();
                if !literal.is_empty() {
                    literal.push('\n');
                }
                self.render_code("", &literal)
            }
            7 => {
                let block = &self.blocks[ordinal];
                let (info_end, literal_start, logical_end) =
                    fenced_code_logical_bounds(block).map_err(RenderFailure::Invalid)?;
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
                if self.extension_enabled("tagfilter") {
                    push_gfm_tagfiltered_html(&mut self.output, &literal);
                } else {
                    self.output.push_str(&literal);
                }
                self.cr();
                Ok(())
            }
            12 => {
                let property = required_fact(&self.blocks[ordinal], 3, false)
                    .map_err(RenderFailure::Invalid)?;
                let level = read_u8(property, 0).map_err(RenderFailure::Invalid)?;
                if !(1..=6).contains(&level) {
                    return Err(RenderFailure::Invalid(
                        "recursive Green heading level is invalid".into(),
                    ));
                }
                self.cr();
                self.output.push_str(&format!("<h{level}>"));
                self.render_inline_leaf(ordinal)?;
                self.output.push_str(&format!("</h{level}>"));
                self.lf();
                Ok(())
            }
            13 => {
                self.cr();
                self.output.push_str("<hr />");
                self.lf();
                Ok(())
            }
            // Marker-only list items and empty block quotes carry one
            // presentation row so the live editor has a real caret target.
            // They contribute no CommonMark semantic output; their containing
            // Item or BlockQuote emits the structural HTML.
            14 | 15 => Ok(()),
            other => Err(RenderFailure::Invalid(format!(
                "recursive Green carries unknown block kind {other}"
            ))),
        }
    }

    fn render_paragraph(&mut self, ordinal: usize) -> Result<(), RenderFailure> {
        if self.gfm && self.extension_enabled("table") {
            let inline = project_inline_leaf(
                self.markdown,
                &self.blocks[ordinal],
                self.green,
                self.references,
                self.runtime,
            )?;
            match project_m11_gfm_table(&inline.source) {
                Ok(Some(table)) => return self.render_table(&inline, &table),
                Ok(None) => {}
                Err(_) => return Err(RenderFailure::Missing("table-fail-closed")),
            }
        }
        let tight = self.paragraph_is_tight(ordinal)?;
        if !tight {
            self.cr();
            self.output.push_str("<p>");
        }
        self.render_inline_leaf(ordinal)?;
        if !tight {
            self.output.push_str("</p>");
            self.lf();
        }
        Ok(())
    }

    fn render_table(
        &mut self,
        inline: &InlineProjection,
        table: &M11GfmTableProjection,
    ) -> Result<(), RenderFailure> {
        let allow_bare_autolinks = self.extension_enabled("autolink");
        let (nodes, roots) = inline_forest(&inline.facts)?;
        if let Some(preface) = table.preface_range.clone() {
            self.cr();
            self.output.push_str("<p>");
            render_inline_table_range(
                &mut self.output,
                inline,
                &nodes,
                &roots,
                u32_range_to_usize(preface)?,
                allow_bare_autolinks,
            )?;
            self.output.push_str("</p>");
            self.lf();
        }
        self.cr();
        self.output.push_str("<table>\n<thead>\n<tr>\n");
        self.render_table_row(inline, &nodes, &roots, table, &table.header, true)?;
        self.output.push_str("</tr>\n</thead>\n");
        if !table.body.is_empty() {
            self.output.push_str("<tbody>\n");
            for row in &table.body {
                self.output.push_str("<tr>\n");
                self.render_table_row(inline, &nodes, &roots, table, row, false)?;
                self.output.push_str("</tr>\n");
            }
            self.output.push_str("</tbody>\n");
        }
        self.output.push_str("</table>");
        self.lf();
        Ok(())
    }

    fn render_table_row(
        &mut self,
        inline: &InlineProjection,
        nodes: &[InlineNode],
        roots: &[usize],
        table: &M11GfmTableProjection,
        row: &flark_parser::M11GfmTableRow,
        header: bool,
    ) -> Result<(), RenderFailure> {
        let allow_bare_autolinks = self.extension_enabled("autolink");
        for (column, cell) in row.cells.iter().enumerate() {
            let tag = if header { "th" } else { "td" };
            self.output.push('<');
            self.output.push_str(tag);
            match table
                .alignments
                .get(column)
                .copied()
                .unwrap_or(M11GfmTableAlignment::None)
            {
                M11GfmTableAlignment::None => {}
                M11GfmTableAlignment::Left => self.output.push_str(" align=\"left\""),
                M11GfmTableAlignment::Center => self.output.push_str(" align=\"center\""),
                M11GfmTableAlignment::Right => self.output.push_str(" align=\"right\""),
            }
            self.output.push('>');
            if !cell.autocompleted {
                render_inline_table_range(
                    &mut self.output,
                    inline,
                    nodes,
                    roots,
                    u32_range_to_usize(cell.content_range.clone())?,
                    allow_bare_autolinks,
                )?;
            }
            self.output.push_str("</");
            self.output.push_str(tag);
            self.output.push_str(">\n");
        }
        Ok(())
    }

    fn render_inline_leaf(&mut self, ordinal: usize) -> Result<(), RenderFailure> {
        let logical = normalize_inline_logical_source(&self.blocks[ordinal].logical);
        let references = self.inline_references(&logical)?;
        let nodes = project_m11_gfm_inline(
            &logical,
            M11GfmInlineOptions {
                strikethrough: self.extension_enabled("strikethrough"),
                autolink: self.extension_enabled("autolink"),
            },
            &references,
        )
        .map_err(|_| RenderFailure::Missing("bounded-inline-projection"))?;
        let tagfilter = self.extension_enabled("tagfilter");
        render_gfm_inline_nodes(&mut self.output, &nodes, tagfilter)?;
        Ok(())
    }

    fn inline_references(
        &self,
        logical: &str,
    ) -> Result<Vec<M11GfmInlineReference>, RenderFailure> {
        let resolver =
            M11ReferenceResolver::from_live_reference_journal(self.runtime, self.references)
                .map_err(|error| RenderFailure::Invalid(format!("reference resolver: {error}")))?;
        let mut references = Vec::new();
        for normalized_label in candidate_reference_labels(logical) {
            match resolver
                .resolve(self.runtime, &normalized_label, 8 * 1024)
                .map_err(|error| {
                    RenderFailure::Invalid(format!("bounded reference resolution: {error}"))
                })? {
                M11ReferenceResolution::Missing => {}
                M11ReferenceResolution::Unknown => {
                    return Err(RenderFailure::Invalid(
                        "final reference authority returned a prefix-only outcome".to_string(),
                    ));
                }
                M11ReferenceResolution::ValueTooLarge => {
                    return Err(RenderFailure::Missing("bounded-reference-value"));
                }
                M11ReferenceResolution::Resolved(reference) => {
                    references.push(M11GfmInlineReference {
                        normalized_label,
                        destination: reference.cooked_destination().to_owned(),
                        title: reference.cooked_title().unwrap_or_default().to_owned(),
                    });
                }
            }
        }
        Ok(references)
    }

    fn extension_enabled(&self, extension: &str) -> bool {
        self.extensions.iter().any(|value| value == extension)
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

fn item_task_checked(block: &GreenBlock) -> Result<Option<bool>, RenderFailure> {
    let property = required_fact(block, 2, false).map_err(RenderFailure::Invalid)?;
    let task = property
        .get(4)
        .copied()
        .ok_or_else(|| RenderFailure::Invalid("Item task fact is absent".into()))?;
    match task {
        0 => Ok(None),
        1 => Ok(Some(false)),
        2 => Ok(Some(true)),
        _ => Err(RenderFailure::Invalid("Item task fact is invalid".into())),
    }
}

fn fenced_code_logical_bounds(block: &GreenBlock) -> Result<(u64, u64, u64), String> {
    let close = required_fact(block, 4, true)?;
    let info_end = read_u64(close, 1)?;
    let literal_start = read_u64(close, 17)?;
    let logical_end = match close.len() {
        // Legacy semantic-only fenced close facts.
        49 => read_u64(close, 33)?,
        // Current semantic prefix followed by a self-sized versioned RGEO
        // trailer. Its compact width depends on the row-relative coordinates.
        _ if close.get(33..37) == Some(&b"RGEO"[..]) => u64::try_from(block.logical.len())
            .map_err(|_| "fenced-code logical length exceeds u64".to_owned())?,
        _ => return Err("fenced-code close facts have an unsupported schema".to_owned()),
    };
    if info_end > literal_start || literal_start > logical_end {
        return Err("fenced-code logical bounds are reversed".to_owned());
    }
    Ok((info_end, literal_start, logical_end))
}

fn project_inline_leaf(
    markdown: &str,
    block: &GreenBlock,
    green: &M11RecursiveGreenRoot,
    references: &M11ReferenceJournalRoot,
    runtime: &mut DocumentRuntime,
) -> Result<InlineProjection, RenderFailure> {
    if block.logical.is_empty() {
        return Ok(InlineProjection {
            source: String::new(),
            facts: Vec::new(),
            link_values: Vec::new(),
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
    let limits = M11RecursiveGreenRowQueryLimits::new(1, 4096, 1_000_000, 4096, 1_000_000)
        .expect("nonzero semantic gate limits");
    let fence = resolve_m11_recursive_green_inline_leaf_row_fence(
        runtime,
        green,
        M11RecursiveGreenPoint::new(point, utf16, SourceBoundaryAffinity::After),
        limits,
        1_000_000,
    )
    .map_err(|_| RenderFailure::Missing("projected-inline-authority"))?
    .ok_or(RenderFailure::Missing("projected-inline-authority"))?;
    let range = fence.inline_source_range();
    let source = markdown
        .get(
            usize::try_from(range.start)
                .map_err(|_| RenderFailure::Invalid("inline range start exceeds usize".into()))?
                ..usize::try_from(range.end)
                    .map_err(|_| RenderFailure::Invalid("inline range end exceeds usize".into()))?,
        )
        .ok_or_else(|| RenderFailure::Invalid("inline range is not a source UTF-8 cut".into()))?
        .to_owned();
    let expected_outcome_range = u32::try_from(range.start)
        .map_err(|_| RenderFailure::Invalid("inline range start exceeds u32".into()))?
        ..u32::try_from(range.end)
            .map_err(|_| RenderFailure::Invalid("inline range end exceeds u32".into()))?;
    let profile = ParserProfileId::new(1).expect("nonzero parser profile");
    let resolver = M11ReferenceResolver::from_live_reference_journal(runtime, references)
        .map_err(|error| RenderFailure::Invalid(format!("reference resolver: {error}")))?;
    let mut job =
        M11InlineProjectionJob::new_for_recursive_green_inline_leaf_with_reference_resolver(
            runtime,
            fence,
            M11ParserBinding::current(profile),
            resolver,
        )
        .map_err(|error| RenderFailure::Invalid(format!("inline job creation: {error}")))?;
    let mut complete = false;
    let mut poll_error = None;
    for _ in 0..MAX_POLLS {
        let poll = match job.poll(runtime, FUEL) {
            Ok(poll) => poll,
            Err(error) => {
                poll_error = Some(error);
                break;
            }
        };
        if poll.status() == M11InlineProjectionJobPollStatus::Complete {
            complete = true;
            break;
        }
    }
    if let Some(error) = poll_error {
        release_inline_capture(&mut job, runtime)?;
        return Err(RenderFailure::Invalid(format!("inline job poll: {error}")));
    }
    if !complete {
        release_inline_capture(&mut job, runtime)?;
        return Err(RenderFailure::Invalid(
            "inline projection job did not converge".into(),
        ));
    }
    match job.take_outcome() {
        Some(M11InlineProjectionOutcome::Authoritative {
            source: outcome_source,
            source_range,
            parser_profile,
            capture,
        }) => {
            if outcome_source
                != runtime.current_source_version().ok_or_else(|| {
                    RenderFailure::Invalid("inline outcome source is no longer current".into())
                })?
                || source_range != expected_outcome_range
                || parser_profile != profile
            {
                return Err(RenderFailure::Invalid(
                    "inline outcome stamp differs from its recursive-Green fence".into(),
                ));
            }
            let (facts, link_values, _) = capture.into_parts();
            Ok(InlineProjection {
                source,
                facts,
                link_values,
            })
        }
        Some(M11InlineProjectionOutcome::Unsupported { .. }) => {
            Err(RenderFailure::Missing("inline-fail-closed"))
        }
        None => {
            release_inline_capture(&mut job, runtime)?;
            Err(RenderFailure::Invalid(
                "completed inline job omitted its disposition".into(),
            ))
        }
    }
}

fn release_inline_capture(
    job: &mut M11InlineProjectionJob,
    runtime: &mut DocumentRuntime,
) -> Result<(), RenderFailure> {
    job.begin_release(runtime)
        .map_err(|error| RenderFailure::Invalid(format!("begin inline cleanup: {error}")))?;
    for _ in 0..MAX_POLLS {
        if job
            .poll_release(runtime, FUEL)
            .map_err(|error| RenderFailure::Invalid(format!("poll inline cleanup: {error}")))?
            .complete()
        {
            return Ok(());
        }
    }
    Err(RenderFailure::Invalid(
        "inline projection cleanup did not converge".into(),
    ))
}

fn normalize_inline_logical_source(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    for (index, line) in source.split('\n').enumerate() {
        if index != 0 {
            output.push('\n');
        }
        if index == 0 {
            output.push_str(line);
        } else {
            output.push_str(line.trim_start_matches([' ', '\t']));
        }
    }
    output
}

fn candidate_reference_labels(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut labels = BTreeSet::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'[' || byte_is_escaped(bytes, index) {
            index += 1;
            continue;
        }
        let start = index + 1;
        let mut end = start;
        while end < bytes.len() && end - start <= 999 {
            if bytes[end] == b']' && !byte_is_escaped(bytes, end) {
                if let Some(label) = source.get(start..end) {
                    let normalized = comrak::block_spine_facade::normalize_reference_label(label);
                    if !normalized.is_empty() {
                        labels.insert(normalized);
                    }
                }
                break;
            }
            end += 1;
        }
        index += 1;
    }
    labels.into_iter().collect()
}

fn byte_is_escaped(bytes: &[u8], index: usize) -> bool {
    let mut backslashes = 0;
    let mut cursor = index;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        backslashes += 1;
        cursor -= 1;
    }
    backslashes % 2 != 0
}

fn render_gfm_inline_nodes(
    output: &mut String,
    nodes: &[M11GfmInlineNode],
    tagfilter: bool,
) -> Result<(), RenderFailure> {
    for node in nodes {
        match node {
            M11GfmInlineNode::Text(text) => push_escaped_html(output, text),
            M11GfmInlineNode::SoftBreak => output.push('\n'),
            M11GfmInlineNode::LineBreak => output.push_str("<br />\n"),
            M11GfmInlineNode::Code(code) => {
                output.push_str("<code>");
                push_escaped_html(output, code);
                output.push_str("</code>");
            }
            M11GfmInlineNode::Html(html) => {
                if tagfilter && gfm_tagfilter_matches(html) {
                    output.push_str("&lt;");
                    output.push_str(&html[1..]);
                } else {
                    output.push_str(html);
                }
            }
            M11GfmInlineNode::Transparent(children) => {
                render_gfm_inline_nodes(output, children, tagfilter)?;
            }
            M11GfmInlineNode::Emphasis(children) => {
                output.push_str("<em>");
                render_gfm_inline_nodes(output, children, tagfilter)?;
                output.push_str("</em>");
            }
            M11GfmInlineNode::Strong(children) => {
                output.push_str("<strong>");
                render_gfm_inline_nodes(output, children, tagfilter)?;
                output.push_str("</strong>");
            }
            M11GfmInlineNode::Strikethrough(children) => {
                output.push_str("<del>");
                render_gfm_inline_nodes(output, children, tagfilter)?;
                output.push_str("</del>");
            }
            M11GfmInlineNode::Link {
                destination,
                title,
                children,
            } => {
                output.push_str("<a href=\"");
                push_safe_escaped_href(output, destination)?;
                if !title.is_empty() {
                    output.push_str("\" title=\"");
                    push_escaped_html(output, title);
                }
                output.push_str("\">");
                render_gfm_inline_nodes(output, children, tagfilter)?;
                output.push_str("</a>");
            }
            M11GfmInlineNode::Image {
                destination,
                title,
                children,
            } => {
                output.push_str("<img src=\"");
                push_safe_escaped_href(output, destination)?;
                output.push_str("\" alt=\"");
                render_gfm_inline_plain(output, children);
                if !title.is_empty() {
                    output.push_str("\" title=\"");
                    push_escaped_html(output, title);
                }
                output.push_str("\" />");
            }
        }
    }
    Ok(())
}

fn render_gfm_inline_plain(output: &mut String, nodes: &[M11GfmInlineNode]) {
    for node in nodes {
        match node {
            M11GfmInlineNode::Text(text)
            | M11GfmInlineNode::Code(text)
            | M11GfmInlineNode::Html(text) => push_escaped_html(output, text),
            M11GfmInlineNode::SoftBreak | M11GfmInlineNode::LineBreak => output.push(' '),
            M11GfmInlineNode::Emphasis(children)
            | M11GfmInlineNode::Transparent(children)
            | M11GfmInlineNode::Strong(children)
            | M11GfmInlineNode::Strikethrough(children)
            | M11GfmInlineNode::Link { children, .. }
            | M11GfmInlineNode::Image { children, .. } => {
                render_gfm_inline_plain(output, children);
            }
        }
    }
}

fn gfm_tagfilter_matches(literal: &str) -> bool {
    const BLACKLIST: [&str; 9] = [
        "title",
        "textarea",
        "style",
        "xmp",
        "iframe",
        "noembed",
        "noframes",
        "script",
        "plaintext",
    ];
    let bytes = literal.as_bytes();
    if bytes.len() < 3 || bytes[0] != b'<' {
        return false;
    }
    let mut start = 1;
    if bytes[start] == b'/' {
        start += 1;
    }
    let lower = literal[start..].to_ascii_lowercase();
    BLACKLIST.iter().any(|tag| {
        if !lower.starts_with(tag) {
            return false;
        }
        let end = start + tag.len();
        bytes.get(end).is_some_and(|byte| {
            byte.is_ascii_whitespace()
                || *byte == b'>'
                || (*byte == b'/' && bytes.get(end + 1) == Some(&b'>'))
        })
    })
}

fn push_gfm_tagfiltered_html(output: &mut String, literal: &str) {
    let mut cursor = 0;
    while let Some(relative) = literal[cursor..].find('<') {
        let marker = cursor + relative;
        output.push_str(&literal[cursor..marker]);
        if gfm_tagfilter_matches(&literal[marker..]) {
            output.push_str("&lt;");
        } else {
            output.push('<');
        }
        cursor = marker + 1;
    }
    output.push_str(&literal[cursor..]);
}

fn render_inline_table_range(
    output: &mut String,
    projection: &InlineProjection,
    nodes: &[InlineNode],
    roots: &[usize],
    range: std::ops::Range<usize>,
    allow_bare_autolinks: bool,
) -> Result<(), RenderFailure> {
    let mut contained = Vec::new();
    collect_inline_facts_in_range(nodes, roots, &range, &mut contained)?;
    let mut rendered = String::new();
    render_inline_range(
        &mut rendered,
        &projection.source,
        nodes,
        &contained,
        &projection.link_values,
        range,
        allow_bare_autolinks,
    )?;
    // GFM removes the escape byte before a pipe before parsing a table cell.
    // Ordinary escapes have already consumed it; this remaining spelling is
    // the code-span case from GFM example 200.
    output.push_str(&rendered.replace("\\|", "|"));
    Ok(())
}

fn collect_inline_facts_in_range(
    nodes: &[InlineNode],
    candidates: &[usize],
    range: &std::ops::Range<usize>,
    output: &mut Vec<usize>,
) -> Result<(), RenderFailure> {
    for candidate in candidates.iter().copied() {
        let fact_range = u32_range_to_usize(nodes[candidate].fact.relative_range())?;
        if fact_range.start >= range.start && fact_range.end <= range.end {
            output.push(candidate);
        } else if fact_range.start < range.end && fact_range.end > range.start {
            collect_inline_facts_in_range(nodes, &nodes[candidate].children, range, output)?;
        }
    }
    Ok(())
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
                M11InlineProjectionKind::Emphasis
                    | M11InlineProjectionKind::Strong
                    | M11InlineProjectionKind::Strikethrough
                    | M11InlineProjectionKind::DirectLink
                    | M11InlineProjectionKind::DirectImage
                    | M11InlineProjectionKind::ReferenceLink
                    | M11InlineProjectionKind::ReferenceImage
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
    link_values: &[M11InlineLinkValue],
    range: std::ops::Range<usize>,
    allow_bare_autolinks: bool,
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
        render_inline_fact(
            output,
            source,
            nodes,
            child,
            link_values,
            allow_bare_autolinks,
        )?;
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
    link_values: &[M11InlineLinkValue],
    allow_bare_autolinks: bool,
) -> Result<(), RenderFailure> {
    let node = &nodes[ordinal];
    let content = u32_range_to_usize(node.fact.relative_content_range())?;
    match node.fact.kind() {
        M11InlineProjectionKind::Emphasis => {
            output.push_str("<em>");
            render_inline_range(
                output,
                source,
                nodes,
                &node.children,
                link_values,
                content,
                allow_bare_autolinks,
            )?;
            output.push_str("</em>");
        }
        M11InlineProjectionKind::Strong => {
            output.push_str("<strong>");
            render_inline_range(
                output,
                source,
                nodes,
                &node.children,
                link_values,
                content,
                allow_bare_autolinks,
            )?;
            output.push_str("</strong>");
        }
        M11InlineProjectionKind::Code => {
            let value = inline_code_value(source, node)?;
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
        M11InlineProjectionKind::Strikethrough => {
            output.push_str("<del>");
            render_inline_range(
                output,
                source,
                nodes,
                &node.children,
                link_values,
                content,
                allow_bare_autolinks,
            )?;
            output.push_str("</del>");
        }
        M11InlineProjectionKind::AutolinkUri | M11InlineProjectionKind::AutolinkEmail => {
            let visible = source.get(content).ok_or_else(|| {
                RenderFailure::Invalid("autolink content is not UTF-8 aligned".into())
            })?;
            if node.fact.relative_range() == node.fact.relative_content_range()
                && !allow_bare_autolinks
            {
                push_inline_text(output, visible);
                return Ok(());
            }
            let href = match node.fact.kind() {
                M11InlineProjectionKind::AutolinkEmail => format!("mailto:{visible}"),
                M11InlineProjectionKind::AutolinkUri
                    if node.fact.flags() & M11_INLINE_PROJECTION_FLAG_AUTOLINK_URI_WWW != 0 =>
                {
                    format!("http://{visible}")
                }
                M11InlineProjectionKind::AutolinkUri => visible.to_owned(),
                _ => unreachable!(),
            };
            output.push_str("<a href=\"");
            push_escaped_href(output, &href)?;
            output.push_str("\">");
            push_inline_text(output, visible);
            output.push_str("</a>");
        }
        M11InlineProjectionKind::DirectLink | M11InlineProjectionKind::ReferenceLink => {
            let value = inline_link_value(link_values, ordinal)?;
            output.push_str("<a href=\"");
            push_safe_escaped_href(output, value.cooked_destination())?;
            if let Some(title) = value.cooked_title() {
                output.push_str("\" title=\"");
                push_escaped_html(output, title);
            }
            output.push_str("\">");
            render_inline_range(
                output,
                source,
                nodes,
                &node.children,
                link_values,
                content,
                allow_bare_autolinks,
            )?;
            output.push_str("</a>");
        }
        M11InlineProjectionKind::DirectImage | M11InlineProjectionKind::ReferenceImage => {
            let value = inline_link_value(link_values, ordinal)?;
            output.push_str("<img src=\"");
            push_safe_escaped_href(output, value.cooked_destination())?;
            output.push_str("\" alt=\"");
            let mut alt = String::new();
            render_inline_plain_range(
                &mut alt,
                source,
                nodes,
                &node.children,
                link_values,
                content,
            )?;
            push_escaped_html(output, &alt);
            if let Some(title) = value.cooked_title() {
                output.push_str("\" title=\"");
                push_escaped_html(output, title);
            }
            output.push_str("\" />");
        }
    }
    Ok(())
}

fn inline_link_value(
    link_values: &[M11InlineLinkValue],
    fact_ordinal: usize,
) -> Result<&M11InlineLinkValue, RenderFailure> {
    let fact_ordinal = u32::try_from(fact_ordinal)
        .map_err(|_| RenderFailure::Invalid("inline fact ordinal exceeds u32".into()))?;
    link_values
        .iter()
        .find(|value| value.parent_fact_ordinal() == fact_ordinal)
        .ok_or_else(|| RenderFailure::Invalid("link/image fact omitted its cooked value".into()))
}

fn inline_code_value(source: &str, node: &InlineNode) -> Result<String, RenderFailure> {
    let content = u32_range_to_usize(node.fact.relative_content_range())?;
    let mut value = source
        .get(content)
        .ok_or_else(|| RenderFailure::Invalid("code content is not UTF-8 aligned".into()))?
        .replace("\r\n", " ")
        .replace(['\r', '\n'], " ");
    if node.fact.flags() & 2 != 0 && value.starts_with(' ') && value.ends_with(' ') {
        value.remove(0);
        value.pop();
    }
    Ok(value)
}

fn render_inline_plain_range(
    output: &mut String,
    source: &str,
    nodes: &[InlineNode],
    children: &[usize],
    link_values: &[M11InlineLinkValue],
    range: std::ops::Range<usize>,
) -> Result<(), RenderFailure> {
    let mut cursor = range.start;
    for child in children.iter().copied() {
        let fact_range = u32_range_to_usize(nodes[child].fact.relative_range())?;
        if fact_range.start < cursor || fact_range.end > range.end {
            return Err(RenderFailure::Invalid(
                "plain inline facts overlap or leave their parent content".into(),
            ));
        }
        push_inline_plain_text(
            output,
            source.get(cursor..fact_range.start).ok_or_else(|| {
                RenderFailure::Invalid("plain inline gap is not UTF-8 aligned".into())
            })?,
        );
        render_inline_plain_fact(output, source, nodes, child, link_values)?;
        cursor = fact_range.end;
    }
    push_inline_plain_text(
        output,
        source.get(cursor..range.end).ok_or_else(|| {
            RenderFailure::Invalid("plain inline trailing text is not UTF-8 aligned".into())
        })?,
    );
    Ok(())
}

fn render_inline_plain_fact(
    output: &mut String,
    source: &str,
    nodes: &[InlineNode],
    ordinal: usize,
    link_values: &[M11InlineLinkValue],
) -> Result<(), RenderFailure> {
    let node = &nodes[ordinal];
    let content = u32_range_to_usize(node.fact.relative_content_range())?;
    match node.fact.kind() {
        M11InlineProjectionKind::Emphasis
        | M11InlineProjectionKind::Strong
        | M11InlineProjectionKind::Strikethrough
        | M11InlineProjectionKind::DirectLink
        | M11InlineProjectionKind::DirectImage
        | M11InlineProjectionKind::ReferenceLink
        | M11InlineProjectionKind::ReferenceImage => {
            render_inline_plain_range(output, source, nodes, &node.children, link_values, content)?
        }
        M11InlineProjectionKind::Code => output.push_str(&inline_code_value(source, node)?),
        M11InlineProjectionKind::AutolinkUri | M11InlineProjectionKind::AutolinkEmail => {
            output.push_str(source.get(content).ok_or_else(|| {
                RenderFailure::Invalid("plain autolink content is not UTF-8 aligned".into())
            })?);
        }
        M11InlineProjectionKind::BackslashEscape => {
            output.push_str(source.get(content).ok_or_else(|| {
                RenderFailure::Invalid("plain escape content is not UTF-8 aligned".into())
            })?);
        }
        M11InlineProjectionKind::HardLineBreak => output.push(' '),
        M11InlineProjectionKind::CharacterReference => {
            let (first, second) = node.fact.character_reference().ok_or_else(|| {
                RenderFailure::Invalid("plain character reference omitted cooked scalars".into())
            })?;
            output.push(first);
            if let Some(second) = second {
                output.push(second);
            }
        }
    }
    Ok(())
}

fn push_safe_escaped_href(output: &mut String, href: &str) -> Result<(), RenderFailure> {
    if !comrak::html::dangerous_url(href) {
        push_escaped_href(output, href)?;
    }
    Ok(())
}

fn push_escaped_href(output: &mut String, href: &str) -> Result<(), RenderFailure> {
    comrak::html::escape_href(output, href, false)
        .map_err(|_| RenderFailure::Invalid("href escaping failed".into()))
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
    for segment in normalized.split_inclusive('\n') {
        if let Some(line) = segment.strip_suffix('\n') {
            push_escaped_html(output, line.trim_end_matches([' ', '\t']));
            output.push('\n');
        } else {
            push_escaped_html(output, segment);
        }
    }
}

fn push_inline_plain_text(output: &mut String, text: &str) {
    output.push_str(
        &text
            .replace("\r\n", " ")
            .replace(['\r', '\n'], " ")
            .replace('\0', "\u{fffd}"),
    );
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
        let extensions = if json[cursor..].starts_with(",\n    \"extensions\": [") {
            cursor += ",\n    \"extensions\": [".len();
            json_string_array(json, &mut cursor)
        } else {
            Vec::new()
        };
        fixtures.push(Fixture {
            markdown,
            html,
            example,
            section,
            extensions,
        });
    }
    fixtures
}

fn gfm_fixtures() -> Vec<Fixture> {
    let mut fixtures = load_fixtures(GFM_FIXTURES);
    // These two values mirror the hash-pinned supplement above. The imported
    // corpus and supplement use different JSON field orders, while this test's
    // deliberately tiny loader follows the upstream corpus order.
    fixtures.extend([
        Fixture {
            markdown: "- [ ] foo\n- [x] bar\n".into(),
            html: "<ul>\n<li><input disabled=\"\" type=\"checkbox\"> foo</li>\n<li><input checked=\"\" disabled=\"\" type=\"checkbox\"> bar</li>\n</ul>\n".into(),
            example: 279,
            section: "Task list items (extension)".into(),
            extensions: vec!["tasklist".into()],
        },
        Fixture {
            markdown: "- [x] foo\n  - [ ] bar\n  - [x] baz\n- [ ] bim\n".into(),
            html: "<ul>\n<li><input checked=\"\" disabled=\"\" type=\"checkbox\"> foo\n<ul>\n<li><input disabled=\"\" type=\"checkbox\"> bar</li>\n<li><input checked=\"\" disabled=\"\" type=\"checkbox\"> baz</li>\n</ul>\n</li>\n<li><input disabled=\"\" type=\"checkbox\"> bim</li>\n</ul>\n".into(),
            example: 280,
            section: "Task list items (extension)".into(),
            extensions: vec!["tasklist".into()],
        },
    ]);
    fixtures.sort_by_key(|fixture| fixture.example);
    fixtures
}

fn json_string_array(json: &str, cursor: &mut usize) -> Vec<String> {
    let bytes = json.as_bytes();
    let mut values = Vec::new();
    loop {
        while matches!(
            bytes.get(*cursor),
            Some(b' ' | b'\n' | b'\r' | b'\t' | b',')
        ) {
            *cursor += 1;
        }
        match bytes.get(*cursor) {
            Some(b']') => {
                *cursor += 1;
                return values;
            }
            Some(b'\"') => values.push(json_string(json, cursor)),
            _ => panic!("JSON string array value"),
        }
    }
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

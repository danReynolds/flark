use flark_engine::m11_host::M11_CANDIDATE_ARENA_MAX_SLOTS;
use flark_engine::parser_internal::{
    IndentedCodeLineV1, M11BlockSequencePoint, M11IndentedCodeProjectionCursorPoll,
    M11OwnedSnapshotPoll, M11RetainedCandidatePublication, M11SnapshotFrameKind,
    INDENTED_CODE_LINE_FLAG_INTERNAL_BLANK, INDENTED_CODE_WINDOW_MAX_BYTES, M11_MAX_ROLE_RECORDS,
};
use flark_engine::{
    ArenaLimits, DocumentRuntime, DocumentRuntimeConfig, ParserProfileId, RuntimeSourceFactsPoll,
    SourceBoundaryAffinity, SourceFactsRootLimits, SourceFactsScanProfile,
};
use flark_parser::{
    resolve_m11_published_indented_code_leaf_fence, M11CandidateDerivationError, M11CleanParseJob,
    M11CleanParsePoll, M11IndentedCodeProjectionJob, M11IndentedCodeProjectionJobError,
    M11IndentedCodeProjectionJobPollStatus, M11ParserBinding, M11ParserCandidate,
    M11ParserCandidateWriterPoll,
};

const PROFILE: u64 = 0x1c07;

fn binding() -> M11ParserBinding {
    M11ParserBinding::current(ParserProfileId::new(PROFILE).expect("parser profile"))
}

fn runtime(source: &str) -> DocumentRuntime {
    DocumentRuntime::new(
        source,
        DocumentRuntimeConfig {
            arena_limits: ArenaLimits {
                max_slots: M11_CANDIDATE_ARENA_MAX_SLOTS,
                max_live_payload_bytes: 64 * 1024 * 1024,
                max_children_per_node: M11_MAX_ROLE_RECORDS,
            },
            ..DocumentRuntimeConfig::default()
        },
    )
    .expect("runtime")
}

fn prepare_source_facts(runtime: &mut DocumentRuntime) {
    let expected = runtime
        .begin_source_facts(
            SourceFactsScanProfile::new(32).expect("source facts profile"),
            binding().syntax_profile(),
            SourceFactsRootLimits::default(),
        )
        .expect("begin source facts");
    loop {
        match runtime
            .poll_source_facts(257, 17)
            .expect("source facts poll")
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. }
            | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
            RuntimeSourceFactsPoll::Complete { completion, .. } => {
                assert_eq!(completion.source(), expected);
                break;
            }
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
            | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                panic!("clean source-fact scan reported incremental work")
            }
        }
    }
}

fn retain_candidate(runtime: &mut DocumentRuntime) -> M11RetainedCandidatePublication {
    prepare_source_facts(runtime);
    let mut parse = M11CleanParseJob::new(
        runtime
            .snapshot_current_source()
            .expect("exact parse source"),
    )
    .expect("clean parse");
    let result = loop {
        match parse.poll(257).expect("clean parse poll") {
            M11CleanParsePoll::Pending { transitions } => assert!(transitions <= 257),
            M11CleanParsePoll::Complete {
                transitions,
                result,
            } => {
                assert!(transitions <= 257);
                break result;
            }
        }
    };
    let certified = runtime.take_certified_source().expect("certified source");
    let candidate =
        M11ParserCandidate::derive_segmented(certified, result).expect("segmented candidate");
    let mut writer = candidate
        .into_writer(runtime, [0x31; 16], [0x32; 16], 1)
        .expect("candidate writer");
    let publication = loop {
        match writer.poll(runtime, 17).expect("candidate writer poll") {
            M11ParserCandidateWriterPoll::Pending { transitions } => {
                assert!(transitions <= 17);
            }
            M11ParserCandidateWriterPoll::Published {
                transitions,
                publication,
            } => {
                assert!(transitions <= 17);
                break publication;
            }
        }
    };
    drop(writer);

    let mut stream = publication
        .into_snapshot_stream(runtime)
        .expect("snapshot stream");
    assert_eq!(
        stream.begin_frame().expect("snapshot begin").kind,
        M11SnapshotFrameKind::Begin
    );
    loop {
        match stream.poll(runtime, 17).expect("snapshot poll") {
            M11OwnedSnapshotPoll::Pending { transitions } => assert!(transitions <= 17),
            M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                panic!("full candidate unexpectedly requested replay")
            }
            M11OwnedSnapshotPoll::Frame { transitions, frame } => {
                assert!(transitions <= 17);
                if frame.kind == M11SnapshotFrameKind::End {
                    break;
                }
            }
        }
    }
    stream
        .into_retained_publication(runtime)
        .expect("retained publication")
}

fn fence(
    runtime: &DocumentRuntime,
    retained: &M11RetainedCandidatePublication,
    byte: usize,
    utf16: usize,
) -> flark_parser::M11PublishedIndentedCodeLeafFence {
    resolve_m11_published_indented_code_leaf_fence(
        runtime,
        retained,
        M11BlockSequencePoint::new(byte, utf16, SourceBoundaryAffinity::After),
    )
    .expect("published indented-code fence")
}

fn close_retained(retained: &mut M11RetainedCandidatePublication, runtime: &mut DocumentRuntime) {
    retained.begin_close(runtime).expect("begin retained close");
    while !retained
        .poll_close(runtime, 17)
        .expect("retained close poll")
    {}
}

fn close_runtime(mut runtime: DocumentRuntime) {
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(257).expect("runtime close").complete {}
    let metrics = runtime.arena_metrics();
    assert_eq!(metrics.resident_nodes, 0);
    assert_eq!(metrics.live_builds, 0);
    assert_eq!(metrics.reserved_external_payload_bytes, 0);
}

#[test]
fn published_mixed_source_projects_exact_records_without_a_second_indent_parser() {
    let source = "\u{feff}\tα\0\r\n\n      \r    \tβ\r\tlast";
    let mut runtime = runtime(source);
    let version = runtime.current_source_version().expect("source");
    let mut retained = retain_candidate(&mut runtime);
    let fence = fence(&runtime, &retained, 0, 0);
    assert_eq!(fence.source(), version);
    assert_eq!(fence.block_source_range(), 0..source.len() as u32);
    assert_eq!(fence.line_count(), 5);
    assert_eq!(fence.projected_utf8_length(), 17);
    assert_eq!(fence.projected_utf16_length(), 15);
    assert_eq!(fence.terminal_eol_bytes(), 0);
    assert!(fence.has_bof_bom());

    let mut job = M11IndentedCodeProjectionJob::new(&runtime, fence).expect("projection job");
    loop {
        let poll = job.poll(&mut runtime, 1).expect("projection poll");
        assert!(poll.transitions() <= 1);
        if poll.status() == M11IndentedCodeProjectionJobPollStatus::Pending {
            assert_ne!(poll.transitions(), 0, "ready exact job must not stall");
        } else {
            break;
        }
    }
    let mut root = job.take_root().expect("ready projection root");
    drop(job);
    assert_eq!(root.descriptor().source(), version);
    assert_eq!(
        root.descriptor().parser_profile(),
        binding().syntax_profile()
    );
    assert_eq!(
        root.descriptor().physical_block_range(),
        &(0..source.len() as u32)
    );
    assert_eq!(
        root.descriptor().requested_window(),
        &(0..source.len() as u32)
    );
    assert_eq!(root.descriptor().line_count(), 5);
    assert!(root.descriptor().has_synthetic_final_lf());

    let mut cursor = root
        .cursor(
            &runtime,
            version,
            binding().syntax_profile(),
            0..source.len() as u32,
            0..source.len() as u32,
        )
        .expect("typed projection cursor");
    let mut records = Vec::new();
    loop {
        match cursor.poll(&runtime).expect("cursor poll") {
            M11IndentedCodeProjectionCursorPoll::Pending { transitions } => {
                assert!(transitions <= 1);
            }
            M11IndentedCodeProjectionCursorPoll::Line { transitions, line } => {
                assert!(transitions <= 1);
                records.push(line);
            }
            M11IndentedCodeProjectionCursorPoll::Complete { transitions } => {
                assert!(transitions <= 1);
                break;
            }
        }
    }
    assert_eq!(
        records,
        vec![
            IndentedCodeLineV1::code(0, 9, 4, 3).expect("BOM/tab/Unicode/NUL line"),
            IndentedCodeLineV1::internal_blank(9, 1, 0).expect("internal blank"),
            IndentedCodeLineV1::code(10, 7, 4, 2).expect("residual blank source"),
            IndentedCodeLineV1::code(17, 8, 4, 3).expect("post-deindent tab and Unicode"),
            IndentedCodeLineV1::code(25, 5, 1, 4).expect("EOF line"),
        ]
    );
    assert_eq!(records[1].flags(), INDENTED_CODE_LINE_FLAG_INTERNAL_BLANK);
    assert_eq!(
        records[2].flags(),
        0,
        "a lexical blank with residual source content is not the zero-content internal-blank shape"
    );
    drop(cursor);

    root.begin_release(&mut runtime)
        .expect("begin root release");
    loop {
        let poll = root
            .poll_release(&mut runtime, 1)
            .expect("root release poll");
        assert!(poll.receipt().transitions <= 1);
        if poll.complete() {
            break;
        }
    }
    drop(root);
    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

#[test]
fn in_flight_job_cancellation_reclaims_source_and_persistent_work() {
    let source = "\tq\n".repeat(205);
    let mut runtime = runtime(&source);
    let mut retained = retain_candidate(&mut runtime);
    let fence = fence(&runtime, &retained, 0, 0);
    let mut job = M11IndentedCodeProjectionJob::new(&runtime, fence).expect("projection job");
    let poll = job.poll(&mut runtime, 1_300).expect("bounded work");
    assert!(poll.transitions() <= 1_300);

    job.begin_cancel(&mut runtime)
        .expect("begin job cancellation");
    loop {
        let poll = job
            .poll_cancel(&mut runtime, 1)
            .expect("job cancellation poll");
        assert!(poll.transitions() <= 1);
        if poll.complete() {
            break;
        }
    }
    drop(job);
    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

#[test]
fn resolver_fails_closed_for_a_non_indented_structured_or_paragraph_leaf() {
    let source = "paragraph\n\n    code\n";
    let mut runtime = runtime(source);
    let mut retained = retain_candidate(&mut runtime);
    let error = resolve_m11_published_indented_code_leaf_fence(
        &runtime,
        &retained,
        M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
    )
    .expect_err("paragraph must not mint indented-code authority");
    assert!(matches!(
        error,
        M11CandidateDerivationError::PublishedIndentedCodeLeafFenceNotIndentedCode
    ));
    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

#[test]
fn projection_job_rejects_an_exact_leaf_above_the_urgent_window_cap() {
    let source = format!("    {}", "x".repeat(INDENTED_CODE_WINDOW_MAX_BYTES));
    assert!(source.len() > INDENTED_CODE_WINDOW_MAX_BYTES);
    let mut runtime = runtime(&source);
    let mut retained = retain_candidate(&mut runtime);
    let fence = fence(&runtime, &retained, 0, 0);
    let error = M11IndentedCodeProjectionJob::new(&runtime, fence)
        .expect_err("oversized exact leaf must stay off the urgent projection path");
    assert!(matches!(
        error,
        M11IndentedCodeProjectionJobError::WindowTooLarge {
            bytes,
            cap: INDENTED_CODE_WINDOW_MAX_BYTES,
        } if bytes == source.len()
    ));
    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

#[test]
fn projection_job_revalidates_the_published_fence_against_current_source() {
    let source = "    code\n";
    let mut runtime = runtime(source);
    let mut retained = retain_candidate(&mut runtime);
    let fence = fence(&runtime, &retained, 0, 0);
    let base = runtime.current_source_version().expect("base source");
    runtime
        .apply_edit(base, source.len()..source.len(), "\n")
        .expect("advance source");
    let error = M11IndentedCodeProjectionJob::new(&runtime, fence)
        .expect_err("stale published fence must not cross source authority");
    assert!(matches!(
        error,
        M11IndentedCodeProjectionJobError::SourceAuthorityMismatch
    ));
    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

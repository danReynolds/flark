use flark_engine::parser_internal::{
    M11InlineProjectionCursorPoll, M11InlineProjectionKind, M11RecursiveGreenFrameQueryLimits,
    M11RecursiveGreenPoint, M11RecursiveGreenRoot,
};
use flark_engine::{
    DocumentRuntime, DocumentRuntimeConfig, ParserProfileId, SourceBoundaryAffinity,
    SOURCE_CURSOR_WINDOW_BYTES,
};
use flark_parser::block_core::{
    resolve_m11_recursive_green_paragraph_fence, M11BlockWriter, M11BlockWriterOfferStatus,
    M11BlockWriterPollStatus, M11DirectBlockController, M11DirectBlockPollStatus,
};
use flark_parser::{
    M11ExactController, M11InlineProjectionJob, M11InlineProjectionJobPollStatus,
    M11InlineProjectionPublication, M11ParserBinding, M11SourceLinePollStatus, M11SourceLineSource,
    SnapshotLinePoll, SnapshotLineScanner, SnapshotLineSource,
};

fn write_pending_command(
    controller: &mut M11DirectBlockController,
    writer: &mut M11BlockWriter,
    runtime: &mut DocumentRuntime,
    fuel: usize,
) {
    let command = *controller
        .pending_command()
        .expect("one parser command is ready");
    match writer
        .offer_command(command)
        .expect("writer accepts command")
    {
        M11BlockWriterOfferStatus::Complete => {}
        M11BlockWriterOfferStatus::Pending => loop {
            let poll = writer.poll(runtime, fuel).expect("writer poll");
            assert!(poll.transitions() <= fuel);
            if matches!(
                poll.status(),
                M11BlockWriterPollStatus::CommandComplete
                    | M11BlockWriterPollStatus::DocumentComplete
            ) {
                break;
            }
        },
    }
    controller
        .acknowledge_command()
        .expect("parser command acknowledgement");
}

fn drive(source: &str, fuel: usize) -> (DocumentRuntime, M11RecursiveGreenRoot) {
    let mut runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("document runtime");
    let writer_lease = runtime
        .snapshot_current_source()
        .expect("writer source lease");
    let scanner_lease = runtime
        .snapshot_current_source()
        .expect("scanner source lease");
    let mut scanner = SnapshotLineScanner::new(scanner_lease).expect("line scanner");
    let mut controller = M11DirectBlockController::new().expect("direct controller");
    let mut writer = M11BlockWriter::new(&runtime, writer_lease).expect("block writer");
    write_pending_command(&mut controller, &mut writer, &mut runtime, fuel);

    loop {
        let line = loop {
            match scanner.poll(fuel).expect("line discovery") {
                SnapshotLinePoll::Pending(next) => scanner = next,
                SnapshotLinePoll::Line(line) => break Some(line),
                SnapshotLinePoll::Complete => break None,
            }
        };
        let Some(line) = line else { break };
        let facts = line.facts();
        let mut source = line.into_source().expect("source-backed line");
        let mut admission = <M11DirectBlockController as M11ExactController<
            SnapshotLineSource,
        >>::begin_source_line(&mut controller, facts.identity())
        .expect("line admission");
        loop {
            if source.access_budget() == 0 && source.position() < source.len() {
                source
                    .replenish_access_budget(SOURCE_CURSOR_WINDOW_BYTES)
                    .expect("bounded source grant");
            }
            let receipt = <M11DirectBlockController as M11ExactController<
                SnapshotLineSource,
            >>::poll_source_line(&mut controller, &mut admission, &mut source, fuel)
            .expect("source poll");
            assert!(receipt.source_first_reads <= fuel);
            if receipt.status == M11SourceLinePollStatus::Matched {
                break;
            }
        }
        <M11DirectBlockController as M11ExactController<SnapshotLineSource>>::commit_source_line(
            &mut controller,
            admission,
            facts,
        )
        .expect("source commit");
        scanner = source.finish().expect("line source is exhausted");

        loop {
            let receipt = controller.poll_line(fuel).expect("grammar poll");
            assert!(receipt.transitions <= fuel);
            match receipt.status {
                M11DirectBlockPollStatus::Pending => {}
                M11DirectBlockPollStatus::CommandReady => {
                    write_pending_command(&mut controller, &mut writer, &mut runtime, fuel);
                }
                M11DirectBlockPollStatus::ExternalWorkReady => {
                    panic!("paragraph fixture unexpectedly requires reference work");
                }
                M11DirectBlockPollStatus::Complete => break,
            }
        }
    }

    controller.begin_finish().expect("parser finish begins");
    loop {
        let receipt = controller.poll_finish(fuel).expect("finish poll");
        assert!(receipt.transitions <= fuel);
        match receipt.status {
            M11DirectBlockPollStatus::Pending => {}
            M11DirectBlockPollStatus::CommandReady => {
                write_pending_command(&mut controller, &mut writer, &mut runtime, fuel);
            }
            M11DirectBlockPollStatus::ExternalWorkReady => {
                panic!("paragraph fixture unexpectedly requires reference work");
            }
            M11DirectBlockPollStatus::Complete => break,
        }
    }
    let root = writer.take_root().expect("completed recursive Green root");
    (runtime, root)
}

#[test]
fn selected_middle_paragraph_mints_exact_authority_for_inline_projection() {
    const SOURCE: &str = "before\n\n**bold** _em_ `code`\n\nafter\n";
    const INLINE: std::ops::Range<u64> = 8..28;
    const BLOCK: std::ops::Range<u64> = 8..29;

    let (mut runtime, mut green) = drive(SOURCE, 1);
    let limits =
        M11RecursiveGreenFrameQueryLimits::new(64, 4096, 64, 1024).expect("nonzero query limits");
    let fence = resolve_m11_recursive_green_paragraph_fence(
        &runtime,
        &green,
        M11RecursiveGreenPoint::new(12, 12, SourceBoundaryAffinity::After),
        limits,
    )
    .expect("bounded Paragraph query")
    .expect("selected source belongs to a final Paragraph");

    assert_eq!(fence.source(), green.source());
    assert_ne!(fence.frame().get(), 0);
    assert_eq!(fence.block_source_range(), BLOCK);
    assert_eq!(fence.block_source_utf16_range(), BLOCK);
    assert_eq!(fence.inline_source_range(), INLINE);
    assert_eq!(fence.inline_source_utf16_range(), INLINE);
    assert!(fence.receipt().storage_pages_visited() <= 64);
    assert!(fence.receipt().events_scanned() <= 4096);
    assert!(fence.receipt().maximum_open_depth() <= 64);

    let profile = ParserProfileId::new(1).expect("nonzero parser profile");
    let mut job = M11InlineProjectionJob::new_for_recursive_green_paragraph(
        &runtime,
        fence,
        M11ParserBinding::current(profile),
    )
    .expect("inline projection job accepts the minted authority");
    loop {
        let poll = job.poll(&mut runtime, 1).expect("inline projection poll");
        assert!(poll.transitions() <= 1);
        if poll.status() == M11InlineProjectionJobPollStatus::Complete {
            break;
        }
    }
    let output = job.take_output().expect("inline projection output");
    assert_eq!(output.source_range(), 8..28);
    let (_, range, actual_profile, authority, publication) =
        output.into_publication_parts().into_parts();
    assert_eq!(range, 8..28);
    assert_eq!(actual_profile, profile);
    authority
        .validate(&runtime)
        .expect("returned source authority remains exact");

    let M11InlineProjectionPublication::Authoritative(mut inline) = publication else {
        panic!("bold/emphasis/code Paragraph must project authoritatively");
    };
    assert_eq!(inline.descriptor().fact_count(), 3);
    let mut cursor = inline
        .cursor(&runtime, green.source(), profile)
        .expect("inline projection cursor");
    let mut kinds = Vec::new();
    loop {
        match cursor.poll(&runtime).expect("inline cursor poll") {
            M11InlineProjectionCursorPoll::Pending { transitions } => {
                assert!(transitions <= 1);
            }
            M11InlineProjectionCursorPoll::Fact { transitions, fact } => {
                assert!(transitions <= 1);
                kinds.push(fact.kind());
            }
            M11InlineProjectionCursorPoll::Complete { transitions } => {
                assert!(transitions <= 1);
                break;
            }
        }
    }
    assert_eq!(
        kinds,
        vec![
            M11InlineProjectionKind::Strong,
            M11InlineProjectionKind::Emphasis,
            M11InlineProjectionKind::Code,
        ]
    );
    drop(cursor);
    inline
        .begin_release(&mut runtime)
        .expect("begin inline root release");
    while !inline
        .poll_release(&mut runtime, 1)
        .expect("poll inline root release")
        .complete()
    {}
    drop(authority);
    drop(job);

    green
        .begin_release(&mut runtime)
        .expect("begin Green root release");
    while !green
        .poll_release(&mut runtime, 64)
        .expect("poll Green root release")
        .complete()
    {}
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("poll runtime close").complete {}
}

#[test]
fn paragraph_near_twenty_thousand_block_eof_has_prefix_independent_query_work() {
    use std::fmt::Write;

    let mut source = String::with_capacity(280_000);
    for index in 0..20_000 {
        writeln!(&mut source, "block {index}").expect("String writes are infallible");
        source.push('\n');
    }
    let target_start = source.len();
    // Keep the selected paragraph large enough to cross several compact event
    // pages after the variable-width Green encoding raised page density.
    for index in 0..512 {
        writeln!(&mut source, "tail line {index}").expect("String writes are infallible");
    }
    let final_line_start = source.len();
    source.push_str("**tail**\n");
    let target_inline_end = source.len() - 1;

    let (mut runtime, mut green) = drive(&source, 64);
    let limits =
        M11RecursiveGreenFrameQueryLimits::new(12, 2048, 16, 8192).expect("nonzero query limits");
    let point = final_line_start + 3;
    let fence = resolve_m11_recursive_green_paragraph_fence(
        &runtime,
        &green,
        M11RecursiveGreenPoint::new(point, point, SourceBoundaryAffinity::After),
        limits,
    )
    .expect("late Paragraph query remains within fixed work bounds")
    .expect("late source owner is a final Paragraph");
    assert_eq!(
        fence.block_source_range(),
        target_start as u64..source.len() as u64
    );
    assert_eq!(
        fence.inline_source_range(),
        target_start as u64..target_inline_end as u64
    );
    assert!(
        fence.receipt().storage_pages_visited() >= 4,
        "the witness must cross event pages and exercise backward owner recovery; visited {}",
        fence.receipt().storage_pages_visited(),
    );
    assert!(fence.receipt().storage_pages_visited() <= 12);
    assert!(fence.receipt().events_scanned() <= 2048);
    drop(fence);

    green
        .begin_release(&mut runtime)
        .expect("begin Green root release");
    while !green
        .poll_release(&mut runtime, 64)
        .expect("poll Green root release")
        .complete()
    {}
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("poll runtime close").complete {}
}

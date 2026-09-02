use flark_engine::parser_internal::{
    M11RecursiveGreenPoint, M11RecursiveGreenRoot, M11RecursiveGreenRowQueryLimits,
};
use flark_engine::{
    DocumentRuntime, DocumentRuntimeConfig, ParserProfileId, SourceBoundaryAffinity,
    SOURCE_CURSOR_WINDOW_BYTES,
};
use flark_parser::block_core::{
    resolve_m11_recursive_green_inline_leaf_row_fence, M11BlockWriter, M11BlockWriterOfferStatus,
    M11BlockWriterPollStatus, M11DirectBlockController, M11DirectBlockPollStatus,
    M11RecursiveGreenInlineLeafKind,
};
use flark_parser::{
    M11ExactController, M11InlineProjectionJob, M11InlineProjectionJobPollStatus,
    M11InlineProjectionKind, M11InlineProjectionOutcome, M11ParserBinding, M11SourceLinePollStatus,
    M11SourceLineSource, SnapshotLinePoll, SnapshotLineScanner, SnapshotLineSource,
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
        M11RecursiveGreenRowQueryLimits::new(1, 64, 4096, 64, 4096).expect("nonzero query limits");
    let inline_fence = resolve_m11_recursive_green_inline_leaf_row_fence(
        &runtime,
        &green,
        M11RecursiveGreenPoint::new(12, 12, SourceBoundaryAffinity::After),
        limits,
        1024,
    )
    .expect("bounded inline-row query")
    .expect("selected source belongs to an inline-bearing row");

    assert_eq!(
        inline_fence.kind(),
        M11RecursiveGreenInlineLeafKind::Paragraph
    );
    assert_eq!(inline_fence.source(), green.source());
    assert_ne!(inline_fence.frame().get(), 0);
    assert_eq!(inline_fence.block_source_range(), BLOCK);
    assert_eq!(inline_fence.block_source_utf16_range(), BLOCK);
    assert_eq!(inline_fence.inline_source_range(), INLINE);
    assert_eq!(inline_fence.inline_source_utf16_range(), INLINE);
    assert!(inline_fence.receipt().storage_pages_visited() <= 64);
    assert!(inline_fence.receipt().events_scanned() <= 4096);
    assert!(inline_fence.receipt().maximum_open_depth() <= 64);

    let profile = ParserProfileId::new(1).expect("nonzero parser profile");
    let mut job = M11InlineProjectionJob::new_for_recursive_green_inline_leaf(
        &runtime,
        inline_fence,
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
    let M11InlineProjectionOutcome::Authoritative {
        source,
        source_range,
        parser_profile,
        capture,
    } = job.take_outcome().expect("atomic inline outcome")
    else {
        panic!("bold/emphasis/code Paragraph must project authoritatively");
    };
    assert_eq!(source, green.source());
    assert_eq!(source_range, 8..28);
    assert_eq!(parser_profile, profile);
    let kinds = capture
        .facts()
        .iter()
        .copied()
        .map(|fact| fact.kind())
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            M11InlineProjectionKind::Strong,
            M11InlineProjectionKind::Emphasis,
            M11InlineProjectionKind::Code,
        ]
    );
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
fn unicode_setext_heading_mints_exact_atomic_inline_authority() {
    const SOURCE: &str = "before\n\n🦀 **bold**\n---\n\nafter\n";

    let block_start = SOURCE.find('🦀').expect("Setext block start");
    let inline_end = SOURCE.find("\n---").expect("Setext inline end");
    let block_end = SOURCE.find("---\n").expect("Setext underline") + "---\n".len();
    let block_utf16 =
        SOURCE[..block_start].encode_utf16().count()..SOURCE[..block_end].encode_utf16().count();
    let inline_utf16 =
        SOURCE[..block_start].encode_utf16().count()..SOURCE[..inline_end].encode_utf16().count();
    assert_ne!(block_end - block_start, block_utf16.end - block_utf16.start);
    assert_ne!(
        inline_end - block_start,
        inline_utf16.end - inline_utf16.start
    );

    let point = SOURCE.find("bold").expect("Setext content point") + 1;
    let point_utf16 = SOURCE[..point].encode_utf16().count();
    let (mut runtime, mut green) = drive(SOURCE, 1);
    let limits =
        M11RecursiveGreenRowQueryLimits::new(1, 64, 4096, 64, 4096).expect("nonzero query limits");
    let inline_fence = resolve_m11_recursive_green_inline_leaf_row_fence(
        &runtime,
        &green,
        M11RecursiveGreenPoint::new(point, point_utf16, SourceBoundaryAffinity::After),
        limits,
        1024,
    )
    .expect("bounded Setext row query")
    .expect("Setext Heading is inline-bearing");

    assert_eq!(
        inline_fence.kind(),
        M11RecursiveGreenInlineLeafKind::Heading
    );
    assert_eq!(inline_fence.source(), green.source());
    assert_eq!(
        inline_fence.block_source_range(),
        block_start as u64..block_end as u64
    );
    assert_eq!(
        inline_fence.block_source_utf16_range(),
        block_utf16.start as u64..block_utf16.end as u64
    );
    assert_eq!(
        inline_fence.inline_source_range(),
        block_start as u64..inline_end as u64
    );
    assert_eq!(
        inline_fence.inline_source_utf16_range(),
        inline_utf16.start as u64..inline_utf16.end as u64
    );

    let profile = ParserProfileId::new(1).expect("nonzero parser profile");
    let mut job = M11InlineProjectionJob::new_for_recursive_green_inline_leaf(
        &runtime,
        inline_fence,
        M11ParserBinding::current(profile),
    )
    .expect("Setext inline projection job");
    loop {
        let poll = job.poll(&mut runtime, 1).expect("Setext inline poll");
        assert!(poll.transitions() <= 1);
        if poll.status() == M11InlineProjectionJobPollStatus::Complete {
            break;
        }
    }
    let M11InlineProjectionOutcome::Authoritative {
        source,
        source_range,
        parser_profile,
        capture,
    } = job.take_outcome().expect("atomic Setext inline outcome")
    else {
        panic!("Setext Heading must project authoritatively");
    };
    assert_eq!(source, green.source());
    assert_eq!(source_range, block_start as u32..inline_end as u32);
    assert_eq!(parser_profile, profile);
    assert_eq!(capture.facts().len(), 1);
    assert_eq!(capture.facts()[0].kind(), M11InlineProjectionKind::Strong);
    assert_eq!(capture.facts()[0].relative_range(), 5..13);
    assert_eq!(capture.facts()[0].relative_content_range(), 7..11);
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
        M11RecursiveGreenRowQueryLimits::new(1, 12, 2048, 16, 512).expect("nonzero query limits");
    let point = final_line_start + 3;
    let fence = resolve_m11_recursive_green_inline_leaf_row_fence(
        &runtime,
        &green,
        M11RecursiveGreenPoint::new(point, point, SourceBoundaryAffinity::After),
        limits,
        8192,
    )
    .expect("late row query remains within fixed work bounds")
    .expect("late source owner is an inline-bearing row");
    assert_eq!(fence.kind(), M11RecursiveGreenInlineLeafKind::Paragraph);
    assert_eq!(
        fence.block_source_range(),
        target_start as u64..source.len() as u64
    );
    assert_eq!(
        fence.inline_source_range(),
        target_start as u64..target_inline_end as u64
    );
    assert!(fence.receipt().storage_pages_visited() <= 12);
    assert!(fence.receipt().events_scanned() <= 2048);
    assert!(fence.receipt().node_headers_decoded() <= 512);
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

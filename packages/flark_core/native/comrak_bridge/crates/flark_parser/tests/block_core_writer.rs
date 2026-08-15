use flark_engine::parser_internal::{M11RecursiveGreenPoint, M11RecursiveGreenRoot};
use flark_engine::{
    DocumentRuntime, DocumentRuntimeConfig, SourceBoundaryAffinity, SOURCE_CURSOR_WINDOW_BYTES,
};
use flark_parser::block_core::{
    M11BlockWriter, M11BlockWriterOfferStatus, M11BlockWriterPollStatus, M11DirectBlockController,
    M11DirectBlockPollStatus,
};
use flark_parser::{
    M11ExactController, M11SourceLinePollStatus, M11SourceLineSource, SnapshotLinePoll,
    SnapshotLineScanner, SnapshotLineSource,
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
        M11BlockWriterOfferStatus::Pending => {
            let mut complete = false;
            for _ in 0..100_000 {
                let poll = writer.poll(runtime, fuel).expect("writer poll");
                assert!(poll.transitions() <= fuel);
                match poll.status() {
                    M11BlockWriterPollStatus::Pending => {}
                    M11BlockWriterPollStatus::CommandComplete
                    | M11BlockWriterPollStatus::DocumentComplete => {
                        complete = true;
                        break;
                    }
                }
            }
            assert!(complete, "writer command converges under bounded polling");
        }
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
        let mut matched = false;
        for _ in 0..100_000 {
            if source.access_budget() == 0 && source.position() < source.len() {
                source
                    .replenish_access_budget(SOURCE_CURSOR_WINDOW_BYTES)
                    .expect("bounded source grant");
            }
            let receipt = <M11DirectBlockController as M11ExactController<
                SnapshotLineSource,
            >>::poll_source_line(
                &mut controller,
                &mut admission,
                &mut source,
                fuel,
            )
            .expect("source poll");
            assert!(receipt.source_first_reads <= fuel);
            if receipt.status == M11SourceLinePollStatus::Matched {
                matched = true;
                break;
            }
        }
        assert!(matched, "source recognition converges");
        <M11DirectBlockController as M11ExactController<SnapshotLineSource>>::commit_source_line(
            &mut controller,
            admission,
            facts,
        )
        .expect("source commit");
        scanner = source.finish().expect("line source is exhausted");

        let mut complete = false;
        for _ in 0..100_000 {
            let receipt = controller.poll_line(fuel).expect("grammar poll");
            assert!(receipt.transitions <= fuel);
            match receipt.status {
                M11DirectBlockPollStatus::Pending => {}
                M11DirectBlockPollStatus::CommandReady => {
                    write_pending_command(&mut controller, &mut writer, &mut runtime, fuel);
                }
                M11DirectBlockPollStatus::ExternalWorkReady => {
                    panic!("writer fixture unexpectedly requires reference work");
                }
                M11DirectBlockPollStatus::Complete => {
                    complete = true;
                    break;
                }
            }
        }
        assert!(complete, "line grammar converges");
    }

    controller.begin_finish().expect("parser finish begins");
    let mut complete = false;
    for _ in 0..100_000 {
        let receipt = controller.poll_finish(fuel).expect("finish poll");
        assert!(receipt.transitions <= fuel);
        match receipt.status {
            M11DirectBlockPollStatus::Pending => {}
            M11DirectBlockPollStatus::CommandReady => {
                write_pending_command(&mut controller, &mut writer, &mut runtime, fuel);
            }
            M11DirectBlockPollStatus::ExternalWorkReady => {
                panic!("writer fixture unexpectedly requires reference work");
            }
            M11DirectBlockPollStatus::Complete => {
                complete = true;
                break;
            }
        }
    }
    assert!(complete, "parser finish converges");
    let root = writer.take_root().expect("completed recursive Green root");
    (runtime, root)
}

fn close(mut runtime: DocumentRuntime, mut root: M11RecursiveGreenRoot) {
    root.begin_release(&mut runtime)
        .expect("begin root release");
    while !root
        .poll_release(&mut runtime, 64)
        .expect("poll root release")
        .complete()
    {}
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("poll runtime close").complete {}
}

fn ancestry_kinds(
    root: &M11RecursiveGreenRoot,
    runtime: &DocumentRuntime,
    source: &str,
    byte_offset: usize,
) -> Vec<u16> {
    let utf16_offset = source[..byte_offset].encode_utf16().count();
    root.locate_point(
        runtime,
        M11RecursiveGreenPoint::new(byte_offset, utf16_offset, SourceBoundaryAffinity::After),
    )
    .expect("point query")
    .expect("covered source point")
    .ancestry()
    .iter()
    .map(|ancestor| ancestor.kind().get())
    .collect()
}

#[test]
fn controller_to_recursive_green_cm321_is_exact_with_fuel_one() {
    const SOURCE: &str = "- a\n  > b\n  ```\n  c\n  ```\n- d\n";
    let (runtime, root) = drive(SOURCE, 1);
    assert_eq!(root.source_byte_len(), SOURCE.len() as u64);
    assert_eq!(
        root.source_utf16_len(),
        SOURCE.encode_utf16().count() as u64
    );
    assert_eq!(
        ancestry_kinds(&root, &runtime, SOURCE, 8),
        vec![1, 3, 4, 2, 5]
    );
    assert_eq!(
        ancestry_kinds(&root, &runtime, SOURCE, 18),
        vec![1, 3, 4, 7]
    );
    close(runtime, root);
}

#[test]
fn controller_to_recursive_green_cm325_preserves_nested_and_lazy_ancestry() {
    const SOURCE: &str = "* foo\n  * bar\n\n  baz\n";
    let (runtime, root) = drive(SOURCE, 1);
    assert_eq!(
        ancestry_kinds(&root, &runtime, SOURCE, 10),
        vec![1, 3, 4, 3, 4, 5]
    );
    assert_eq!(
        ancestry_kinds(&root, &runtime, SOURCE, 17),
        vec![1, 3, 4, 5]
    );
    close(runtime, root);
}

#[test]
fn controller_to_recursive_green_setext_retype_is_final_at_query_time() {
    const SOURCE: &str = "foo\n---\n";
    let (runtime, root) = drive(SOURCE, 1);
    assert_eq!(ancestry_kinds(&root, &runtime, SOURCE, 1), vec![1, 12]);
    close(runtime, root);
}

#[test]
fn controller_to_recursive_green_thematic_break_is_marker_only() {
    const SOURCE: &str = "  * * *  \r\n";
    let (runtime, root) = drive(SOURCE, 1);
    assert_eq!(root.source_byte_len(), SOURCE.len() as u64);
    assert_eq!(
        root.source_utf16_len(),
        SOURCE.encode_utf16().count() as u64
    );
    assert_eq!(ancestry_kinds(&root, &runtime, SOURCE, 3), vec![1, 13]);
    close(runtime, root);
}

#[test]
fn controller_to_recursive_green_indented_code_and_html_are_exact() {
    const SOURCE: &str = "    code\n\n<div>\nx\n\n";
    let (runtime, root) = drive(SOURCE, 1);
    assert_eq!(root.source_byte_len(), SOURCE.len() as u64);
    assert_eq!(
        root.source_utf16_len(),
        SOURCE.encode_utf16().count() as u64
    );
    assert_eq!(ancestry_kinds(&root, &runtime, SOURCE, 5), vec![1, 6]);
    let html = SOURCE.find("<div>").expect("HTML offset");
    assert_eq!(ancestry_kinds(&root, &runtime, SOURCE, html), vec![1, 8]);
    close(runtime, root);
}

use flark_engine::parser_internal::{
    M11RecursiveGreenPoint, M11RecursiveGreenRoot, M11ReferenceJournal, M11ReferenceJournalRoot,
    M11ReferenceJournalStatus,
};
use flark_engine::{
    DocumentRuntime, DocumentRuntimeConfig, SourceBoundaryAffinity, SOURCE_CURSOR_WINDOW_BYTES,
};
use flark_parser::block_core::{
    M11BlockWriter, M11BlockWriterOfferStatus, M11BlockWriterPollStatus, M11DirectBlockController,
    M11DirectBlockPollStatus, M11ReferenceRendezvous, M11ReferenceRendezvousStatus,
};
use flark_parser::{
    M11ExactController, M11SourceLinePollStatus, M11SourceLineSource, SnapshotLinePoll,
    SnapshotLineScanner, SnapshotLineSource,
};

const FUEL: usize = 1;

fn write_pending_command(
    controller: &mut M11DirectBlockController,
    writer: &mut M11BlockWriter,
    runtime: &mut DocumentRuntime,
) {
    let command = *controller.pending_command().expect("parser command");
    match writer.offer_command(command).expect("writer command") {
        M11BlockWriterOfferStatus::Complete => {}
        M11BlockWriterOfferStatus::Pending => loop {
            let poll = writer.poll(runtime, FUEL).expect("writer poll");
            assert!(poll.transitions() <= FUEL);
            if matches!(
                poll.status(),
                M11BlockWriterPollStatus::CommandComplete
                    | M11BlockWriterPollStatus::DocumentComplete
            ) {
                break;
            }
        },
    }
    controller.acknowledge_command().expect("command ack");
}

fn drive_external_work(
    controller: &mut M11DirectBlockController,
    writer: &mut M11BlockWriter,
    journal: &mut M11ReferenceJournal,
    runtime: &mut DocumentRuntime,
) {
    let mut rendezvous =
        M11ReferenceRendezvous::begin(controller, writer).expect("begin reference rendezvous");
    for _ in 0..1_000_000 {
        let poll = rendezvous
            .poll(controller, writer, journal, runtime, FUEL)
            .expect("poll reference rendezvous");
        assert!(poll.transitions <= FUEL);
        if poll.status == M11ReferenceRendezvousStatus::Complete {
            return;
        }
    }
    panic!("reference rendezvous converges under fuel one");
}

fn drive(
    source: &str,
) -> (
    DocumentRuntime,
    M11RecursiveGreenRoot,
    M11ReferenceJournalRoot,
) {
    let mut runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
    let source_version = runtime.current_source_version().expect("source version");
    let mut journal =
        M11ReferenceJournal::new(&mut runtime, source_version, 1).expect("reference journal");
    let writer_lease = runtime.snapshot_current_source().expect("writer lease");
    let scanner_lease = runtime.snapshot_current_source().expect("scanner lease");
    let mut scanner = SnapshotLineScanner::new(scanner_lease).expect("line scanner");
    let mut controller = M11DirectBlockController::new().expect("direct controller");
    let mut writer = M11BlockWriter::new(&runtime, writer_lease).expect("block writer");
    write_pending_command(&mut controller, &mut writer, &mut runtime);

    loop {
        let line = loop {
            match scanner.poll(FUEL).expect("line discovery") {
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
                    .expect("source grant");
            }
            let receipt = <M11DirectBlockController as M11ExactController<
                SnapshotLineSource,
            >>::poll_source_line(
                &mut controller,
                &mut admission,
                &mut source,
                FUEL,
            )
            .expect("source poll");
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
        scanner = source.finish().expect("line source finish");

        loop {
            let receipt = controller.poll_line(FUEL).expect("grammar poll");
            match receipt.status {
                M11DirectBlockPollStatus::Pending => {}
                M11DirectBlockPollStatus::CommandReady => {
                    write_pending_command(&mut controller, &mut writer, &mut runtime);
                }
                M11DirectBlockPollStatus::ExternalWorkReady => {
                    drive_external_work(&mut controller, &mut writer, &mut journal, &mut runtime);
                }
                M11DirectBlockPollStatus::Complete => break,
            }
        }
    }

    controller.begin_finish().expect("parser finish begins");
    loop {
        let receipt = controller.poll_finish(FUEL).expect("finish poll");
        match receipt.status {
            M11DirectBlockPollStatus::Pending => {}
            M11DirectBlockPollStatus::CommandReady => {
                write_pending_command(&mut controller, &mut writer, &mut runtime);
            }
            M11DirectBlockPollStatus::ExternalWorkReady => {
                drive_external_work(&mut controller, &mut writer, &mut journal, &mut runtime);
            }
            M11DirectBlockPollStatus::Complete => break,
        }
    }
    let green = writer.take_root().expect("recursive Green root");
    journal
        .finish_input(&runtime)
        .expect("finish reference input");
    loop {
        let poll = journal.poll(&mut runtime, 64).expect("finish journal");
        if poll.status() == M11ReferenceJournalStatus::Complete {
            break;
        }
        assert_eq!(poll.status(), M11ReferenceJournalStatus::Pending);
    }
    let references = journal.take_root().expect("reference root");
    (runtime, green, references)
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

fn close(
    mut runtime: DocumentRuntime,
    mut green: M11RecursiveGreenRoot,
    mut references: M11ReferenceJournalRoot,
) {
    green.begin_release(&mut runtime).expect("release Green");
    while !green
        .poll_release(&mut runtime, 64)
        .expect("poll Green release")
        .complete()
    {}
    references
        .begin_release(&mut runtime)
        .expect("release references");
    while !references
        .poll_release(&mut runtime, 64)
        .expect("poll reference release")
        .complete()
    {}
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("poll close").complete {}
}

#[test]
fn reference_only_duplicates_remove_the_paragraph_and_keep_the_first_winner() {
    const SOURCE: &str = "[a]: /1\n[a]: /2";
    let (runtime, green, references) = drive(SOURCE);
    assert_eq!(green.source_byte_len(), SOURCE.len() as u64);
    assert_eq!(
        green.source_utf16_len(),
        SOURCE.encode_utf16().count() as u64
    );
    assert_eq!(green.logical_byte_len(), 0);
    assert_eq!(references.occurrence_count(), 2);
    assert_eq!(
        references.winner_ordinal(&runtime, b"a").expect("winner"),
        Some(0)
    );
    assert_eq!(ancestry_kinds(&green, &runtime, SOURCE, 1), vec![1]);
    close(runtime, green, references);
}

#[test]
fn nested_tab_crlf_reference_prefix_keeps_only_the_visible_remainder_logical() {
    const SOURCE: &str = "> \t[a]: /u\r\n> visible";
    let (runtime, green, references) = drive(SOURCE);
    assert_eq!(green.source_byte_len(), SOURCE.len() as u64);
    assert_eq!(
        green.source_utf16_len(),
        SOURCE.encode_utf16().count() as u64
    );
    assert_eq!(green.logical_byte_len(), "visible".len() as u64);
    assert_eq!(references.occurrence_count(), 1);
    assert_eq!(
        references.winner_ordinal(&runtime, b"a").expect("winner"),
        Some(0)
    );
    let visible = SOURCE.find("visible").expect("visible offset") + 1;
    let ancestry = ancestry_kinds(&green, &runtime, SOURCE, visible);
    assert_eq!(ancestry.last().copied(), Some(5));
    close(runtime, green, references);
}

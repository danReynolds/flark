use flark_engine::parser_internal::{
    M11ParserPageBuild, M11ParserPageBuildStatus, M11ParserPageCursorPoll, M11ParserPageError,
    M11ParserPageRecord, M11ParserRangeStatus, M11_PARSER_PAGE_MAX_POLL_TRANSITIONS,
    M11_PARSER_PAGE_MAX_RECORD_BYTES, M11_PARSER_RANGE_MAX_POLL_BYTES,
};
use flark_engine::{DocumentRuntime, DocumentRuntimeConfig};

fn close_runtime(mut runtime: DocumentRuntime) {
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("runtime close").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    assert_eq!(runtime.arena_metrics().live_builds, 0);
}

fn accept_record(build: &mut M11ParserPageBuild, runtime: &mut DocumentRuntime, bytes: &[u8]) {
    build
        .offer_record(M11ParserPageRecord::new(bytes).expect("bounded record"))
        .expect("offer record");
    loop {
        let poll = build.poll(runtime, 1).expect("bounded record poll");
        assert!(poll.transitions() <= 1);
        match poll.status() {
            M11ParserPageBuildStatus::NeedsInput => break,
            M11ParserPageBuildStatus::Pending => {}
            M11ParserPageBuildStatus::Complete | M11ParserPageBuildStatus::Cancelled => {
                panic!("record input completed before finish")
            }
        }
    }
}

fn finish_build(
    build: &mut M11ParserPageBuild,
    runtime: &mut DocumentRuntime,
) -> flark_engine::parser_internal::M11ParserPageRoot {
    build.finish_input().expect("finish input");
    loop {
        let poll = build.poll(runtime, 1).expect("bounded finish poll");
        assert!(poll.transitions() <= 1);
        match poll.status() {
            M11ParserPageBuildStatus::Pending => {}
            M11ParserPageBuildStatus::Complete => {
                return build.take_root().expect("completed page root");
            }
            M11ParserPageBuildStatus::NeedsInput | M11ParserPageBuildStatus::Cancelled => {
                panic!("closed parser page build requested more input")
            }
        }
    }
}

fn cancel_and_drain(build: &mut M11ParserPageBuild, runtime: &mut DocumentRuntime) -> usize {
    build
        .begin_cancel(runtime)
        .expect("begin parser-page cancel");
    let mut polls = 0;
    loop {
        let poll = build
            .poll_cancel(runtime, 1)
            .expect("fuel-one parser-page cancel");
        assert!(poll.receipt().transitions <= 1);
        polls += 1;
        if poll.complete() {
            return polls;
        }
    }
}

fn prepared_multi_page_build(runtime: &mut DocumentRuntime, text_len: usize) -> M11ParserPageBuild {
    let lease = runtime.snapshot_current_source().expect("source");
    let mut build = M11ParserPageBuild::new(runtime, lease, 0..text_len, 31).expect("page build");
    let record = vec![b'p'; M11_PARSER_PAGE_MAX_RECORD_BYTES];
    for _ in 0..80 {
        accept_record(&mut build, runtime, &record);
    }
    build
}

#[test]
fn exact_range_cursor_is_replayable_and_bounded_by_one_source_page() {
    let prefix = "ignored:";
    let visible = "αβγ\n".repeat(3_000);
    let text = format!("{prefix}{visible}:ignored");
    let mut runtime =
        DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
    let range = prefix.len()..prefix.len() + visible.len();
    let lease = runtime.snapshot_current_source().expect("source");
    let mut build = M11ParserPageBuild::new(&runtime, lease, range.clone(), 7).expect("page build");
    let mut cursor = build.source_cursor().expect("exact range cursor");
    let mut actual = Vec::new();
    let mut output = [0_u8; 31];
    loop {
        let poll = cursor.poll(31, &mut output).expect("bounded source poll");
        assert!(poll.transitions() <= 31);
        assert_eq!(poll.transitions(), poll.bytes_read());
        actual.extend_from_slice(&output[..poll.bytes_read()]);
        if poll.status() == M11ParserRangeStatus::Complete {
            break;
        }
    }
    assert_eq!(actual, visible.as_bytes());
    assert_eq!(cursor.receipt().bytes_read(), visible.len());
    assert_eq!(cursor.receipt().transitions(), visible.len());
    assert!(cursor.receipt().refill_count() > 1);
    assert!(cursor.receipt().maximum_refill_bytes() <= 4 * 1024);
    drop(cursor);

    build
        .begin_cancel(&mut runtime)
        .expect("cancel empty build");
    assert!(build
        .poll_cancel(&mut runtime, 1)
        .expect("empty cancel poll")
        .complete());
    drop(build);
    close_runtime(runtime);
}

#[test]
fn fixed_page_records_replay_exactly_and_transfer_source_authority() {
    let text = "persistent parser page source\n".repeat(400);
    let mut runtime =
        DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
    let lease = runtime.snapshot_current_source().expect("source");
    let mut build =
        M11ParserPageBuild::new(&runtime, lease, 0..text.len(), 11).expect("page build");
    let mut expected = Vec::new();
    for ordinal in 0..180_u16 {
        let len = usize::from(ordinal % 97) + 1;
        let mut record = vec![0_u8; len];
        record[0] = u8::try_from(ordinal % 251).expect("byte");
        for (index, byte) in record.iter_mut().enumerate().skip(1) {
            *byte = u8::try_from((usize::from(ordinal) + index) % 251).expect("byte");
        }
        accept_record(&mut build, &mut runtime, &record);
        expected.push(record);
    }

    let mut root = finish_build(&mut build, &mut runtime);
    assert_eq!(root.stream_tag(), 11);
    assert_eq!(root.record_count(), expected.len() as u64);
    assert_eq!(
        root.payload_bytes(),
        expected.iter().map(Vec::len).sum::<usize>() as u64
    );
    assert_eq!(
        root.encoded_bytes(),
        root.payload_bytes() + 2 * root.record_count()
    );
    assert!(root.page_count() > 2);
    assert_ne!(root.checksum(), [0; 32]);
    let receipt = root.build_receipt();
    assert_eq!(receipt.records(), expected.len() as u64);
    assert_eq!(receipt.leaves_adopted() as u64, root.page_count());
    assert!(receipt.branches_allocated() >= receipt.leaves_adopted() - 1);
    assert!(receipt.node_headers_decoded() > 0);
    assert!(receipt.payload_bytes_inspected() > 0);
    assert!(receipt.items_hashed() > 0);
    assert!(receipt.maximum_live_bins() > 0);
    assert!(receipt.reserved_scratch_bytes() > 0);
    assert!(receipt.maximum_record_copy_bytes() <= M11_PARSER_PAGE_MAX_RECORD_BYTES);
    assert!(receipt.seal_transitions() > 0);
    drop(build);

    let foreign =
        DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("foreign runtime");
    assert!(matches!(
        root.cursor(&foreign),
        Err(M11ParserPageError::WrongRuntime)
    ));
    close_runtime(foreign);

    let mut cursor = root.cursor(&runtime).expect("page cursor");
    let mut actual = Vec::new();
    loop {
        match cursor.poll(&runtime).expect("bounded cursor poll") {
            M11ParserPageCursorPoll::Pending { transitions } => {
                assert_eq!(transitions, 1);
            }
            M11ParserPageCursorPoll::Record {
                transitions,
                record,
            } => {
                assert_eq!(transitions, 1);
                actual.push(record.as_bytes().to_vec());
            }
            M11ParserPageCursorPoll::Complete { transitions } => {
                assert_eq!(transitions, 0);
                break;
            }
        }
    }
    assert_eq!(actual, expected);
    assert_eq!(cursor.receipt().pages_entered(), root.page_count());
    assert_eq!(cursor.receipt().records_emitted(), root.record_count());
    assert_eq!(cursor.receipt().record_bytes_copied(), root.payload_bytes());
    assert!(cursor.receipt().node_headers_decoded() > 0);
    assert!(cursor.receipt().payload_bytes_inspected() > 0);
    assert!(cursor.receipt().items_hashed() > 0);

    let mut derived =
        M11ParserPageBuild::new_from_root(&runtime, &root, 12).expect("derived stream build");
    let mut source_cursor = derived.source_cursor().expect("transferred source cursor");
    let mut source = Vec::new();
    let mut source_page = [0_u8; 4 * 1024];
    loop {
        let poll = source_cursor
            .poll(source_page.len(), &mut source_page)
            .expect("derived source poll");
        source.extend_from_slice(&source_page[..poll.bytes_read()]);
        if poll.status() == M11ParserRangeStatus::Complete {
            break;
        }
    }
    assert_eq!(source, text.as_bytes());
    drop(source_cursor);
    derived
        .begin_cancel(&mut runtime)
        .expect("cancel derived stream");
    while !derived
        .poll_cancel(&mut runtime, 1)
        .expect("derived reclaim")
        .complete()
    {}
    drop(derived);

    root.begin_release(&mut runtime).expect("release page root");
    let mut release_polls = 0;
    loop {
        let poll = root
            .poll_release(&mut runtime, 1)
            .expect("bounded root release");
        assert!(poll.receipt().transitions <= 1);
        release_polls += 1;
        if poll.complete() {
            break;
        }
    }
    assert!(release_polls > root.page_count() as usize);
    drop(root);
    close_runtime(runtime);
}

#[test]
fn owned_drain_replays_across_polls_and_hands_root_back_at_any_point() {
    let text = "owned parser page drain";
    let mut runtime =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
    let mut build = prepared_multi_page_build(&mut runtime, text.len());
    let root = finish_build(&mut build, &mut runtime);
    let page_count = root.page_count();
    let record_count = root.record_count();
    drop(build);

    let foreign =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("foreign runtime");
    let mut drain = root.into_drain();
    assert!(matches!(
        drain.poll(&foreign),
        Err(M11ParserPageError::WrongRuntime)
    ));
    close_runtime(foreign);

    loop {
        match drain.poll(&runtime).expect("owned drain poll") {
            M11ParserPageCursorPoll::Pending { transitions } => {
                assert_eq!(transitions, 1);
            }
            M11ParserPageCursorPoll::Record {
                transitions,
                record,
            } => {
                assert_eq!(transitions, 1);
                assert!(record.as_bytes().iter().all(|byte| *byte == b'p'));
                break;
            }
            M11ParserPageCursorPoll::Complete { .. } => {
                panic!("multi-page drain completed before its first record")
            }
        }
    }

    // A cooperative job can cancel or change phase without losing the root.
    let root = drain.into_root();
    let mut drain = root.into_drain();
    let mut replayed = 0_u64;
    loop {
        match drain.poll(&runtime).expect("restarted owned drain poll") {
            M11ParserPageCursorPoll::Pending { transitions } => {
                assert_eq!(transitions, 1);
            }
            M11ParserPageCursorPoll::Record {
                transitions,
                record,
            } => {
                assert_eq!(transitions, 1);
                assert!(record.as_bytes().iter().all(|byte| *byte == b'p'));
                replayed += 1;
            }
            M11ParserPageCursorPoll::Complete { transitions } => {
                assert_eq!(transitions, 0);
                break;
            }
        }
    }
    assert_eq!(replayed, record_count);
    assert_eq!(drain.receipt().pages_entered(), page_count);
    assert_eq!(drain.receipt().records_emitted(), record_count);

    let mut root = drain.into_root();
    root.begin_release(&mut runtime)
        .expect("release owned-drain root");
    while !root
        .poll_release(&mut runtime, 1)
        .expect("owned-drain reclaim")
        .complete()
    {}
    drop(root);
    close_runtime(runtime);
}

#[test]
fn cancellation_schedules_arena_ownership_and_reclaims_with_fuel_one() {
    let text = "cancel parser pages";
    let mut runtime =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
    let lease = runtime.snapshot_current_source().expect("source");
    let mut build =
        M11ParserPageBuild::new(&runtime, lease, 0..text.len(), 21).expect("page build");
    let record = vec![b'x'; M11_PARSER_PAGE_MAX_RECORD_BYTES];
    for _ in 0..96 {
        accept_record(&mut build, &mut runtime, &record);
    }
    assert!(runtime.arena_metrics().resident_nodes > 0);
    assert!(runtime.arena_metrics().live_builds > 0);

    build
        .begin_cancel(&mut runtime)
        .expect("begin cancellation");
    let mut polls = 0;
    loop {
        let poll = build
            .poll_cancel(&mut runtime, 1)
            .expect("fuel-one cancellation");
        assert!(poll.receipt().transitions <= 1);
        polls += 1;
        if poll.complete() {
            break;
        }
    }
    assert!(polls > 2);
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    assert_eq!(runtime.arena_metrics().live_builds, 0);
    drop(build);
    close_runtime(runtime);
}

#[test]
fn empty_stream_is_an_explicit_zero_page_root() {
    let text = "";
    let mut runtime =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
    let lease = runtime.snapshot_current_source().expect("source");
    let mut build = M11ParserPageBuild::new(&runtime, lease, 0..0, 41).expect("empty build");
    let mut root = finish_build(&mut build, &mut runtime);
    assert_eq!(root.page_count(), 0);
    assert_eq!(root.record_count(), 0);
    assert_eq!(root.payload_bytes(), 0);
    assert_eq!(root.encoded_bytes(), 0);
    assert_eq!(root.checksum(), [0; 32]);
    assert_eq!(root.build_receipt().leaves_adopted(), 0);
    drop(build);

    let mut cursor = root.cursor(&runtime).expect("empty cursor");
    assert!(matches!(
        cursor.poll(&runtime).expect("empty cursor poll"),
        M11ParserPageCursorPoll::Complete { transitions: 0 }
    ));

    let mut source = root.source_cursor().expect("empty source cursor");
    let mut byte = [0_u8; 1];
    let poll = source.poll(1, &mut byte).expect("empty source poll");
    assert_eq!(poll.status(), M11ParserRangeStatus::Complete);
    assert_eq!(poll.transitions(), 0);
    drop(source);

    root.begin_release(&mut runtime)
        .expect("release empty root");
    assert!(root
        .poll_release(&mut runtime, 1)
        .expect("empty release")
        .complete());
    drop(root);
    close_runtime(runtime);
}

#[test]
fn authority_range_tag_record_and_fuel_fail_closed() {
    let text = "é source";
    let mut runtime =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");

    assert!(matches!(
        M11ParserPageBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("source"),
            0..text.len(),
            0,
        ),
        Err(M11ParserPageError::InvalidStreamTag)
    ));
    assert!(matches!(
        M11ParserPageBuild::new(
            &runtime,
            runtime.snapshot_current_source().expect("source"),
            1..text.len(),
            1,
        ),
        Err(M11ParserPageError::InvalidRange)
    ));
    assert!(matches!(
        M11ParserPageRecord::new(&[]),
        Err(M11ParserPageError::RecordEmpty)
    ));
    assert!(matches!(
        M11ParserPageRecord::new(&vec![0; M11_PARSER_PAGE_MAX_RECORD_BYTES + 1]),
        Err(M11ParserPageError::RecordTooLarge { .. })
    ));

    let foreign =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("foreign runtime");
    assert!(matches!(
        M11ParserPageBuild::new(
            &runtime,
            foreign.snapshot_current_source().expect("foreign source"),
            0..text.len(),
            1,
        ),
        Err(M11ParserPageError::SourceAuthorityMismatch)
    ));
    close_runtime(foreign);

    let lease = runtime.snapshot_current_source().expect("source");
    let mut build =
        M11ParserPageBuild::new(&runtime, lease, 0..text.len(), 1).expect("valid build");
    assert!(matches!(
        build.poll(&mut runtime, 0),
        Err(M11ParserPageError::ZeroFuel)
    ));
    assert!(matches!(
        build.poll(&mut runtime, M11_PARSER_PAGE_MAX_POLL_TRANSITIONS + 1),
        Err(M11ParserPageError::PollLimitExceeded)
    ));

    let mut other =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("other runtime");
    assert!(matches!(
        build.poll(&mut other, 1),
        Err(M11ParserPageError::WrongRuntime)
    ));
    close_runtime(other);

    let current = runtime.current_source_version().expect("current source");
    runtime
        .apply_edit(current, text.len()..text.len(), "!")
        .expect("edit source");
    assert!(matches!(
        build.poll(&mut runtime, 1),
        Err(M11ParserPageError::SourceAuthorityMismatch)
    ));
    // Staleness prevents more parse work but never strands arena/source
    // ownership: cancellation authenticates the runtime, not current content.
    cancel_and_drain(&mut build, &mut runtime);
    drop(build);
    close_runtime(runtime);
}

#[test]
fn cancellation_is_safe_during_pending_push_and_every_finish_transition() {
    let text = "phase cancellation";

    // Force a full leaf to start its measured-sequence push while the next
    // record remains pending.
    let mut pushing_runtime =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
    let lease = pushing_runtime.snapshot_current_source().expect("source");
    let mut pushing =
        M11ParserPageBuild::new(&pushing_runtime, lease, 0..text.len(), 51).expect("build");
    let record = [b'x'; M11_PARSER_PAGE_MAX_RECORD_BYTES];
    for _ in 0..15 {
        accept_record(&mut pushing, &mut pushing_runtime, &record);
    }
    pushing
        .offer_record(M11ParserPageRecord::new(&record).expect("record"))
        .expect("pending record");
    let poll = pushing
        .poll(&mut pushing_runtime, 1)
        .expect("begin page push");
    assert_eq!(poll.status(), M11ParserPageBuildStatus::Pending);
    assert!(cancel_and_drain(&mut pushing, &mut pushing_runtime) > 1);
    assert_eq!(pushing_runtime.arena_metrics().resident_nodes, 0);
    assert_eq!(pushing_runtime.arena_metrics().live_builds, 0);
    drop(pushing);
    close_runtime(pushing_runtime);

    // First measure the exact number of fuel-one finish transitions, then
    // cancel an equivalent build after every possible cut, including the
    // completed-but-not-transferred root.
    let mut oracle_runtime =
        DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("oracle runtime");
    let mut oracle = prepared_multi_page_build(&mut oracle_runtime, text.len());
    oracle.finish_input().expect("finish oracle input");
    let mut finish_transitions = 0;
    loop {
        let poll = oracle
            .poll(&mut oracle_runtime, 1)
            .expect("oracle finish poll");
        finish_transitions += poll.transitions();
        if poll.status() == M11ParserPageBuildStatus::Complete {
            break;
        }
    }
    assert!(finish_transitions > 4);
    cancel_and_drain(&mut oracle, &mut oracle_runtime);
    drop(oracle);
    close_runtime(oracle_runtime);

    for cut in 0..=finish_transitions {
        let mut runtime =
            DocumentRuntime::new(text, DocumentRuntimeConfig::default()).expect("runtime");
        let mut build = prepared_multi_page_build(&mut runtime, text.len());
        build.finish_input().expect("finish input");
        for _ in 0..cut {
            let poll = build.poll(&mut runtime, 1).expect("cut poll");
            assert!(poll.transitions() <= 1);
            if poll.status() == M11ParserPageBuildStatus::Complete {
                break;
            }
        }
        cancel_and_drain(&mut build, &mut runtime);
        assert_eq!(runtime.arena_metrics().resident_nodes, 0, "cut={cut}");
        assert_eq!(runtime.arena_metrics().live_builds, 0, "cut={cut}");
        drop(build);
        close_runtime(runtime);
    }
}

#[test]
fn ten_mib_range_and_record_stream_keep_every_poll_fixed_bounded() {
    const TEN_MIB: usize = 10 * 1024 * 1024;
    let text = "q".repeat(TEN_MIB);
    let mut runtime =
        DocumentRuntime::new(&text, DocumentRuntimeConfig::default()).expect("runtime");
    let lease = runtime.snapshot_current_source().expect("source");
    let mut build = M11ParserPageBuild::new(&runtime, lease, 0..TEN_MIB, 61).expect("large build");

    let mut source_cursor = build.source_cursor().expect("large source cursor");
    let mut source_page = [0_u8; M11_PARSER_RANGE_MAX_POLL_BYTES];
    let mut source_bytes = 0usize;
    loop {
        let poll = source_cursor
            .poll(M11_PARSER_RANGE_MAX_POLL_BYTES, &mut source_page)
            .expect("large source poll");
        assert!(poll.transitions() <= M11_PARSER_RANGE_MAX_POLL_BYTES);
        assert!(source_page[..poll.bytes_read()]
            .iter()
            .all(|byte| *byte == b'q'));
        source_bytes += poll.bytes_read();
        if poll.status() == M11ParserRangeStatus::Complete {
            break;
        }
    }
    assert_eq!(source_bytes, TEN_MIB);
    assert_eq!(source_cursor.receipt().bytes_read(), TEN_MIB);
    assert!(source_cursor.receipt().maximum_refill_bytes() <= M11_PARSER_RANGE_MAX_POLL_BYTES);
    drop(source_cursor);

    let record = [b'q'; M11_PARSER_PAGE_MAX_RECORD_BYTES];
    for _ in 0..TEN_MIB / M11_PARSER_PAGE_MAX_RECORD_BYTES {
        accept_record(&mut build, &mut runtime, &record);
    }
    let mut root = finish_build(&mut build, &mut runtime);
    assert_eq!(root.payload_bytes(), TEN_MIB as u64);
    assert_eq!(
        root.record_count(),
        (TEN_MIB / M11_PARSER_PAGE_MAX_RECORD_BYTES) as u64
    );
    assert_eq!(
        root.build_receipt().maximum_record_copy_bytes(),
        M11_PARSER_PAGE_MAX_RECORD_BYTES
    );
    assert!(root.page_count() > 2_000);
    assert!(runtime.arena_metrics().live_payload_bytes < 16 * 1024 * 1024);
    drop(build);

    let mut cursor = root.cursor(&runtime).expect("large page cursor");
    let mut replayed = 0_u64;
    loop {
        match cursor.poll(&runtime).expect("large replay poll") {
            M11ParserPageCursorPoll::Pending { transitions } => {
                assert_eq!(transitions, 1);
            }
            M11ParserPageCursorPoll::Record {
                transitions,
                record,
            } => {
                assert_eq!(transitions, 1);
                assert!(record.as_bytes().iter().all(|byte| *byte == b'q'));
                replayed += 1;
            }
            M11ParserPageCursorPoll::Complete { transitions } => {
                assert_eq!(transitions, 0);
                break;
            }
        }
    }
    assert_eq!(replayed, root.record_count());
    assert_eq!(cursor.receipt().record_bytes_copied(), TEN_MIB as u64);

    root.begin_release(&mut runtime)
        .expect("release large root");
    loop {
        let poll = root
            .poll_release(&mut runtime, M11_PARSER_PAGE_MAX_POLL_TRANSITIONS)
            .expect("large reclaim poll");
        assert!(poll.receipt().transitions <= M11_PARSER_PAGE_MAX_POLL_TRANSITIONS);
        if poll.complete() {
            break;
        }
    }
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
    assert_eq!(runtime.arena_metrics().live_builds, 0);
    drop(root);
    close_runtime(runtime);
}

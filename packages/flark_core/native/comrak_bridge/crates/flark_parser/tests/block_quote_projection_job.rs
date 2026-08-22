use flark_engine::m11_host::M11_CANDIDATE_ARENA_MAX_SLOTS;
use flark_engine::parser_internal::{
    BlockQuoteLineV1, M11BlockQuoteProjectionCursorPoll, M11BlockSequencePoint,
    M11MarkedLineProjectionKind, M11OwnedSnapshotPoll, M11RetainedCandidatePublication,
    M11SnapshotFrameKind, M11_MAX_ROLE_RECORDS,
};
use flark_engine::{
    ArenaLimits, DocumentRuntime, DocumentRuntimeConfig, ParserProfileId, RuntimeSourceFactsPoll,
    SourceBoundaryAffinity, SourceFactsRootLimits, SourceFactsScanProfile,
};
use flark_parser::{
    resolve_m11_published_block_quote_leaf_fence, resolve_m11_published_bullet_list_item_fences,
    resolve_m11_published_bullet_list_item_inline_fence,
    resolve_m11_published_bullet_list_leaf_fence, resolve_m11_published_ordered_list_leaf_fence,
    M11BlockQuoteProjectionJob, M11BlockQuoteProjectionJobPollStatus,
    M11BulletListItemProjectionJob, M11BulletListItemProjectionJobPollStatus,
    M11BulletListItemProjectionOutput, M11BulletListLocalDeltaBoundaryFallback,
    M11BulletListLocalDeltaError, M11BulletListLocalDeltaJob, M11BulletListLocalDeltaPlan,
    M11BulletListLocalDeltaPoll, M11BulletListLocalDeltaResult, M11BulletListLocalDeltaWork,
    M11BulletListProjectionJob, M11CleanDocumentResult, M11CleanLeaf, M11CleanParseJob,
    M11CleanParsePoll, M11InlineProjectionJob, M11LineEnding, M11ListUnsupportedReason,
    M11OrderedListLocalDeltaBoundaryFallback, M11OrderedListLocalDeltaError,
    M11OrderedListLocalDeltaJob, M11OrderedListLocalDeltaPlan, M11OrderedListLocalDeltaPoll,
    M11OrderedListLocalDeltaResult, M11ParserBinding, M11ParserCandidate,
    M11ParserCandidateWriterPoll, M11PublishedBulletListItemInlineFenceOutcome,
    M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES,
};

const PROFILE: u64 = 0x1c08;

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
    let candidate = M11ParserCandidate::derive_segmented(certified, result)
        .expect("segmented Block Quote candidate");
    let mut writer = candidate
        .into_writer(runtime, [0x41; 16], [0x42; 16], 1)
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

fn clean_result(runtime: &DocumentRuntime) -> M11CleanDocumentResult {
    let mut parse = M11CleanParseJob::new(
        runtime
            .snapshot_current_source()
            .expect("exact parse source"),
    )
    .expect("clean parse");
    loop {
        match parse.poll(257).expect("clean parse poll") {
            M11CleanParsePoll::Pending { .. } => {}
            M11CleanParsePoll::Complete { result, .. } => return result,
        }
    }
}

fn fence(
    runtime: &DocumentRuntime,
    retained: &M11RetainedCandidatePublication,
) -> flark_parser::M11PublishedBlockQuoteLeafFence {
    resolve_m11_published_block_quote_leaf_fence(
        runtime,
        retained,
        M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
    )
    .expect("published Block Quote fence")
}

fn bullet_list_fence(
    runtime: &DocumentRuntime,
    retained: &M11RetainedCandidatePublication,
) -> flark_parser::M11PublishedBulletListLeafFence {
    resolve_m11_published_bullet_list_leaf_fence(
        runtime,
        retained,
        M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
    )
    .expect("published Bullet List fence")
}

fn ordered_list_fence(
    runtime: &DocumentRuntime,
    retained: &M11RetainedCandidatePublication,
    byte: usize,
) -> flark_parser::M11PublishedOrderedListLeafFence {
    resolve_m11_published_ordered_list_leaf_fence(
        runtime,
        retained,
        M11BlockSequencePoint::new(
            byte,
            runtime
                .snapshot_current_source()
                .expect("ordered point source")
                .utf16_offset_for_byte(byte)
                .expect("ordered point UTF-16"),
            SourceBoundaryAffinity::After,
        ),
    )
    .expect("published Ordered List fence")
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

fn abort_inline_job(job: &mut M11InlineProjectionJob, runtime: &mut DocumentRuntime) {
    job.begin_abort(runtime).expect("begin inline abort");
    while !job
        .poll_abort(runtime, 17)
        .expect("inline abort poll")
        .complete()
    {}
}

fn complete_compact_item_projection(
    mut job: M11BulletListItemProjectionJob,
    runtime: &mut DocumentRuntime,
) -> M11BulletListItemProjectionOutput {
    loop {
        let poll = job.poll(runtime, 1).expect("compact item projection poll");
        assert!(poll.transitions() <= 1);
        if poll.status() == M11BulletListItemProjectionJobPollStatus::Pending {
            assert_ne!(poll.transitions(), 0, "ready compact job must not stall");
        } else {
            break;
        }
    }
    let output = job.take_output().expect("compact item projection output");
    drop(job);
    output
}

fn long_bullet_list(item_count: usize) -> String {
    let mut source = String::new();
    for ordinal in 0..item_count {
        source.push_str(&format!("- item-{ordinal:05} café 😀\r\n"));
    }
    source
}

fn long_ordered_list(item_count: usize) -> String {
    let mut source = String::new();
    for ordinal in 0..item_count {
        let authored_marker = if ordinal == 0 {
            7
        } else {
            (ordinal * 37) % 999_999_999
        };
        source.push_str(&format!("{authored_marker}. item-{ordinal:05} café 😀\r\n"));
    }
    source
}

fn complete_local_delta(
    mut job: M11BulletListLocalDeltaJob,
    fuels: &[usize],
) -> Result<M11BulletListLocalDeltaResult, M11BulletListLocalDeltaError> {
    assert!(!fuels.is_empty());
    let mut poll_index = 0_usize;
    loop {
        let fuel = fuels[poll_index % fuels.len()];
        poll_index += 1;
        match job.poll(fuel)? {
            M11BulletListLocalDeltaPoll::Pending { transitions } => {
                assert!(transitions <= fuel);
            }
            M11BulletListLocalDeltaPoll::Complete {
                transitions,
                result,
            } => {
                assert!(transitions <= fuel);
                return Ok(result);
            }
        }
    }
}

fn complete_ordered_local_delta(
    mut job: M11OrderedListLocalDeltaJob,
    fuels: &[usize],
) -> Result<M11OrderedListLocalDeltaResult, M11OrderedListLocalDeltaError> {
    assert!(!fuels.is_empty());
    let mut poll_index = 0_usize;
    loop {
        let fuel = fuels[poll_index % fuels.len()];
        poll_index += 1;
        match job.poll(fuel)? {
            M11OrderedListLocalDeltaPoll::Pending { transitions } => {
                assert!(transitions <= fuel);
            }
            M11OrderedListLocalDeltaPoll::Complete {
                transitions,
                result,
            } => {
                assert!(transitions <= fuel);
                return Ok(result);
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct LocalDeltaSignature {
    list_source: std::ops::Range<u32>,
    list_source_utf16: std::ops::Range<u32>,
    marker: u8,
    item_count: u32,
    paragraph_count: u32,
    terminal_empty_relative_start: Option<u32>,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
}

fn unicode_insertion_local_delta(
    fuels: &[usize],
    cancel_before_completion: bool,
) -> (LocalDeltaSignature, M11BulletListLocalDeltaWork) {
    let source = long_bullet_list(1_000);
    let selected = source
        .find("item-00500 café")
        .expect("selected Unicode item");
    let caret = selected + "item-00500 café".len();
    let replacement = "🧪β";
    let mut expected_source = source.clone();
    expected_source.replace_range(caret..caret, replacement);

    let mut runtime = runtime(&source);
    let mut retained = retain_candidate(&mut runtime);
    let point_utf16 = source[..caret].encode_utf16().count();
    let fence = resolve_m11_published_bullet_list_leaf_fence(
        &runtime,
        &retained,
        M11BlockSequencePoint::new(caret, point_utf16, SourceBoundaryAffinity::After),
    )
    .expect("published list fence");
    let mut plan =
        M11BulletListLocalDeltaPlan::new(&runtime, fence, caret..caret).expect("insertion plan");
    let base = plan.source();
    let prefix_byte_end = plan.prefix_witness_byte_end();
    let prefix_utf16_end = plan.prefix_witness_utf16_end();
    let suffix_byte_start = plan.suffix_witness_byte_start();
    let suffix_utf16_start = plan.suffix_witness_utf16_start();
    runtime
        .apply_edit(base, caret..caret, replacement)
        .expect("Unicode insertion");

    let make_witnesses = |runtime: &DocumentRuntime| {
        let prefix = runtime
            .mint_exact_unchanged_prefix_witness(base, prefix_byte_end, prefix_utf16_end)
            .expect("unchanged predecessor");
        let suffix = runtime
            .mint_exact_unchanged_suffix_witness(base, suffix_byte_start, suffix_utf16_start)
            .expect("unchanged successor");
        (prefix, suffix)
    };
    let (prefix, suffix) = make_witnesses(&runtime);
    let mut job = M11BulletListLocalDeltaJob::new(
        plan,
        prefix,
        suffix,
        runtime.snapshot_current_source().expect("target source"),
    )
    .expect("insertion local-delta job");

    if cancel_before_completion {
        assert!(matches!(
            job.poll(7).expect("partial insertion poll"),
            M11BulletListLocalDeltaPoll::Pending { transitions: 7 }
        ));
        let mut cancellation = job
            .cancel_into_source_authority()
            .expect("cancel insertion job");
        plan = cancellation.take_base_plan().expect("restored base plan");
        assert_eq!(plan.source(), base);
        assert_eq!(plan.prefix_witness_byte_end(), prefix_byte_end);
        let cancelled_target = cancellation
            .take_target_source_lease()
            .expect("cancelled target lease");
        assert_eq!(
            cancelled_target.version(),
            runtime.current_source_version().expect("current target")
        );
        drop(cancelled_target);

        let (prefix, suffix) = make_witnesses(&runtime);
        job = M11BulletListLocalDeltaJob::new(
            plan,
            prefix,
            suffix,
            runtime.snapshot_current_source().expect("target source"),
        )
        .expect("resumed insertion local-delta job");
    }

    let mut result = complete_local_delta(job, fuels).expect("Unicode insertion local delta");
    let clean = clean_result(&runtime);
    let [M11CleanLeaf::BulletList {
        source: clean_source,
        source_utf16: clean_source_utf16,
        marker,
        items,
        projected_utf8_length,
        projected_utf16_length,
        tight,
    }] = clean.leaves()
    else {
        panic!("target remains one exact bullet list");
    };
    let terminal = result.terminal();
    assert!(*tight);
    assert_eq!(terminal.list_source, *clean_source);
    assert_eq!(terminal.list_source_utf16, *clean_source_utf16);
    assert_eq!(terminal.item_count, items.len() as u32);
    assert_eq!(terminal.projected_utf8_length, *projected_utf8_length);
    assert_eq!(terminal.projected_utf16_length, *projected_utf16_length);
    assert_eq!(terminal.list_source.end as usize, expected_source.len());
    let signature = LocalDeltaSignature {
        list_source: terminal.list_source.clone(),
        list_source_utf16: terminal.list_source_utf16.clone(),
        marker: *marker,
        item_count: terminal.item_count,
        paragraph_count: terminal.paragraph_count,
        terminal_empty_relative_start: terminal.terminal_empty_relative_start,
        projected_utf8_length: terminal.projected_utf8_length,
        projected_utf16_length: terminal.projected_utf16_length,
    };
    let work = result.work().clone();
    let mut restored = result.take_base_plan().expect("completed base plan");
    assert_eq!(restored.source(), base);
    drop(result.take_target_source_lease());

    if cancel_before_completion {
        let target_one = runtime.current_source_version().expect("first target");
        let second_caret = caret + replacement.len();
        runtime
            .apply_edit(target_one, second_caret..second_caret, "Z")
            .expect("cumulative insertion");
        expected_source.replace_range(second_caret..second_caret, "Z");
        let (prefix, suffix) = make_witnesses(&runtime);
        let second_job = M11BulletListLocalDeltaJob::new(
            restored,
            prefix,
            suffix,
            runtime
                .snapshot_current_source()
                .expect("second target source"),
        )
        .expect("reused completed base plan");
        let mut second =
            complete_local_delta(second_job, &[3, 11]).expect("cumulative local delta");
        let second_clean = clean_result(&runtime);
        let [M11CleanLeaf::BulletList {
            source: second_clean_source,
            ..
        }] = second_clean.leaves()
        else {
            panic!("cumulative target remains one bullet list");
        };
        assert_eq!(second.terminal().list_source, *second_clean_source);
        assert_eq!(
            second.terminal().list_source.end as usize,
            expected_source.len()
        );
        restored = second
            .take_base_plan()
            .expect("twice-restored completed base plan");
        drop(second.take_target_source_lease());
    }
    drop(restored);

    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
    (signature, work)
}

#[test]
fn checkpoint_free_local_delta_matches_clean_parse_for_20k_item_middle_edit() {
    const ITEM_COUNT: usize = 20_000;
    const EDITED_ITEM: usize = 10_000;
    let source = long_bullet_list(ITEM_COUNT);
    let needle = format!("item-{EDITED_ITEM:05}");
    let edit_start = source.find(&needle).expect("edited item");
    let edit_end = edit_start + needle.len();
    let replacement = "EDIT-α😀";
    let mut expected_source = source.clone();
    expected_source.replace_range(edit_start..edit_end, replacement);

    let mut runtime = runtime(&source);
    let mut retained = retain_candidate(&mut runtime);
    let point_utf16 = source[..edit_start].encode_utf16().count();
    let fence = resolve_m11_published_bullet_list_leaf_fence(
        &runtime,
        &retained,
        M11BlockSequencePoint::new(edit_start, point_utf16, SourceBoundaryAffinity::After),
    )
    .expect("published list fence");
    let plan = M11BulletListLocalDeltaPlan::new(&runtime, fence, edit_start..edit_end)
        .expect("checkpoint-free local plan");
    let base = plan.source();
    let prefix_byte_end = plan.prefix_witness_byte_end();
    let prefix_utf16_end = plan.prefix_witness_utf16_end();
    let suffix_byte_start = plan.suffix_witness_byte_start();
    let suffix_utf16_start = plan.suffix_witness_utf16_start();
    runtime
        .apply_edit(base, edit_start..edit_end, replacement)
        .expect("target edit");
    let prefix = runtime
        .mint_exact_unchanged_prefix_witness(base, prefix_byte_end, prefix_utf16_end)
        .expect("unchanged predecessor");
    let suffix = runtime
        .mint_exact_unchanged_suffix_witness(base, suffix_byte_start, suffix_utf16_start)
        .expect("unchanged successor");
    let target_lease = runtime.snapshot_current_source().expect("target source");
    let mut job = M11BulletListLocalDeltaJob::new(plan, prefix, suffix, target_lease)
        .expect("exact local delta job");
    assert!(matches!(
        job.poll(0).expect("zero-fuel readiness poll"),
        M11BulletListLocalDeltaPoll::Pending { transitions: 0 }
    ));
    let mut local = complete_local_delta(job, &[31]).expect("exact local delta");

    let clean = clean_result(&runtime);
    let [M11CleanLeaf::BulletList {
        source: clean_source,
        source_utf16: clean_source_utf16,
        marker,
        items,
        projected_utf8_length,
        projected_utf16_length,
        tight,
    }] = clean.leaves()
    else {
        panic!("target remains one exact bullet list");
    };
    let terminal = local.terminal();
    assert!(*tight);
    assert_eq!(terminal.list_source, *clean_source);
    assert_eq!(terminal.list_source_utf16, *clean_source_utf16);
    assert_eq!(terminal.marker, *marker);
    assert_eq!(terminal.item_count, items.len() as u32);
    assert_eq!(
        terminal.paragraph_count,
        items.iter().filter(|item| item.paragraph.is_some()).count() as u32
    );
    assert_eq!(terminal.projected_utf8_length, *projected_utf8_length);
    assert_eq!(terminal.projected_utf16_length, *projected_utf16_length);
    assert_eq!(terminal.terminal_empty_relative_start, None);
    assert_eq!(terminal.list_source.end as usize, expected_source.len());

    let work = local.work();
    assert_eq!(work.base_physical_lines, 3);
    assert_eq!(work.target_physical_lines, 3);
    assert_eq!(work.base_source_bytes_discovered, work.base_window_bytes);
    assert_eq!(
        work.target_source_bytes_discovered,
        work.target_window_bytes
    );
    assert_eq!(work.base_source_bytes_read, work.base_window_bytes);
    assert_eq!(work.target_source_bytes_read, work.target_window_bytes);
    assert!(work.base_window_bytes < source.len() / 1_000);
    assert!(work.target_window_bytes < expected_source.len() / 1_000);
    drop(local.take_base_plan());
    drop(local.take_target_source_lease());

    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

#[test]
fn ordered_local_delta_is_sequential_bounded_and_exact_on_a_20k_unicode_crlf_list() {
    const ITEM_COUNT: usize = 20_000;
    const EDITED_ITEM: usize = 10_000;
    let source = long_ordered_list(ITEM_COUNT);
    let needle = format!("item-{EDITED_ITEM:05} café");
    let caret = source.find(&needle).expect("selected ordered item") + needle.len();
    let first_replacement = "🧪β";
    let mut expected_source = source.clone();
    expected_source.replace_range(caret..caret, first_replacement);

    let mut runtime = runtime(&source);
    let mut retained = retain_candidate(&mut runtime);
    let fence = ordered_list_fence(&runtime, &retained, caret);
    let mut plan =
        M11OrderedListLocalDeltaPlan::new(&runtime, fence, caret..caret).expect("ordered plan");
    let base = plan.source();
    let prefix_byte_end = plan.prefix_witness_byte_end();
    let prefix_utf16_end = plan.prefix_witness_utf16_end();
    let suffix_byte_start = plan.suffix_witness_byte_start();
    let suffix_utf16_start = plan.suffix_witness_utf16_start();
    runtime
        .apply_edit(base, caret..caret, first_replacement)
        .expect("first ordered edit");

    let witnesses = |runtime: &DocumentRuntime| {
        (
            runtime
                .mint_exact_unchanged_prefix_witness(base, prefix_byte_end, prefix_utf16_end)
                .expect("ordered unchanged predecessor"),
            runtime
                .mint_exact_unchanged_suffix_witness(base, suffix_byte_start, suffix_utf16_start)
                .expect("ordered unchanged successor"),
        )
    };
    let (prefix, suffix) = witnesses(&runtime);
    let mut job = M11OrderedListLocalDeltaJob::new(
        plan,
        prefix,
        suffix,
        runtime.snapshot_current_source().expect("ordered target"),
    )
    .expect("ordered local job");
    assert!(matches!(
        job.poll(0).expect("ordered zero fuel"),
        M11OrderedListLocalDeltaPoll::Pending { transitions: 0 }
    ));
    assert!(matches!(
        job.poll(7).expect("ordered partial work"),
        M11OrderedListLocalDeltaPoll::Pending { transitions: 7 }
    ));
    let mut cancellation = job
        .cancel_into_source_authority()
        .expect("cancel ordered local job");
    plan = cancellation
        .take_base_plan()
        .expect("restored ordered plan");
    assert_eq!(plan.source(), base);
    drop(
        cancellation
            .take_target_source_lease()
            .expect("cancelled ordered target"),
    );

    let (prefix, suffix) = witnesses(&runtime);
    let job = M11OrderedListLocalDeltaJob::new(
        plan,
        prefix,
        suffix,
        runtime
            .snapshot_current_source()
            .expect("resumed ordered target"),
    )
    .expect("resumed ordered local job");
    let mut first = complete_ordered_local_delta(job, &[1, 7, 31]).expect("first ordered delta");
    let clean = clean_result(&runtime);
    let [M11CleanLeaf::OrderedList {
        source: clean_source,
        source_utf16: clean_source_utf16,
        start,
        delimiter,
        items,
        projected_utf8_length,
        projected_utf16_length,
        tight,
    }] = clean.leaves()
    else {
        panic!("first target remains one exact ordered list");
    };
    let terminal = first.terminal();
    assert!(*tight);
    assert_eq!((terminal.start, terminal.delimiter), (*start, *delimiter));
    assert_eq!(terminal.list_source, *clean_source);
    assert_eq!(terminal.list_source_utf16, *clean_source_utf16);
    assert_eq!(terminal.item_count, items.len() as u32);
    assert_eq!(terminal.paragraph_count, ITEM_COUNT as u32);
    assert_eq!(terminal.projected_utf8_length, *projected_utf8_length);
    assert_eq!(terminal.projected_utf16_length, *projected_utf16_length);
    assert_eq!(terminal.list_source.end as usize, expected_source.len());
    assert_eq!(terminal.terminal_empty_relative_start, None);
    let first_work = first.work().clone();
    assert_eq!(
        (
            first_work.base_physical_lines,
            first_work.target_physical_lines
        ),
        (3, 3)
    );
    assert_eq!(
        first_work.base_source_bytes_discovered,
        first_work.base_window_bytes
    );
    assert_eq!(
        first_work.target_source_bytes_discovered,
        first_work.target_window_bytes
    );
    assert!(first_work.base_window_bytes < source.len() / 1_000);
    assert!(first_work.target_window_bytes < expected_source.len() / 1_000);
    plan = first.take_base_plan().expect("completed ordered base plan");
    drop(first.take_target_source_lease());

    let first_target = runtime
        .current_source_version()
        .expect("first ordered target");
    let second_caret = caret + first_replacement.len();
    runtime
        .apply_edit(first_target, second_caret..second_caret, "Z")
        .expect("second ordered edit");
    expected_source.replace_range(second_caret..second_caret, "Z");
    let (prefix, suffix) = witnesses(&runtime);
    let second_job = M11OrderedListLocalDeltaJob::new(
        plan,
        prefix,
        suffix,
        runtime
            .snapshot_current_source()
            .expect("second ordered target"),
    )
    .expect("sequential ordered local job");
    let mut second =
        complete_ordered_local_delta(second_job, &[3, 11]).expect("second ordered delta");
    let second_clean = clean_result(&runtime);
    let [M11CleanLeaf::OrderedList {
        source: second_source,
        source_utf16: second_utf16,
        start: second_start,
        delimiter: second_delimiter,
        projected_utf8_length: second_projected_utf8,
        projected_utf16_length: second_projected_utf16,
        ..
    }] = second_clean.leaves()
    else {
        panic!("second target remains one exact ordered list");
    };
    assert_eq!(second.terminal().list_source, *second_source);
    assert_eq!(second.terminal().list_source_utf16, *second_utf16);
    assert_eq!(
        (second.terminal().start, second.terminal().delimiter),
        (*second_start, *second_delimiter)
    );
    assert_eq!(
        second.terminal().projected_utf8_length,
        *second_projected_utf8
    );
    assert_eq!(
        second.terminal().projected_utf16_length,
        *second_projected_utf16
    );
    assert_eq!(
        second.terminal().list_source.end as usize,
        expected_source.len()
    );
    assert_eq!(
        (
            second.work().base_physical_lines,
            second.work().target_physical_lines,
        ),
        (3, 3)
    );
    drop(second.take_base_plan());
    drop(second.take_target_source_lease());

    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

#[test]
fn ordered_local_delta_accepts_nine_digits_but_falls_back_on_ten_or_delimiter_change() {
    let source = "1. zero\r\n8. before\r\n9. target\r\n10. after\r\n11. tail\r\n";
    let marker_start = source.find("9. target").expect("edited ordered marker");
    let marker_end = marker_start + 2;
    let mut runtime = runtime(source);
    let mut retained = retain_candidate(&mut runtime);
    let fence = ordered_list_fence(&runtime, &retained, marker_start);
    let mut plan = M11OrderedListLocalDeltaPlan::new(&runtime, fence, marker_start..marker_end)
        .expect("ordered marker plan");
    let base = plan.source();
    let prefix_byte_end = plan.prefix_witness_byte_end();
    let prefix_utf16_end = plan.prefix_witness_utf16_end();
    let suffix_byte_start = plan.suffix_witness_byte_start();
    let suffix_utf16_start = plan.suffix_witness_utf16_start();
    let witnesses = |runtime: &DocumentRuntime| {
        (
            runtime
                .mint_exact_unchanged_prefix_witness(base, prefix_byte_end, prefix_utf16_end)
                .expect("marker predecessor"),
            runtime
                .mint_exact_unchanged_suffix_witness(base, suffix_byte_start, suffix_utf16_start)
                .expect("marker successor"),
        )
    };

    let nine_digit = "123456789.";
    runtime
        .apply_edit(base, marker_start..marker_end, nine_digit)
        .expect("nine-digit ordered marker");
    let (prefix, suffix) = witnesses(&runtime);
    let job = M11OrderedListLocalDeltaJob::new(
        plan,
        prefix,
        suffix,
        runtime
            .snapshot_current_source()
            .expect("nine-digit target"),
    )
    .expect("nine-digit local job");
    let mut exact =
        complete_ordered_local_delta(job, &[1, 9]).expect("nine-digit marker remains local");
    let clean = clean_result(&runtime);
    let [M11CleanLeaf::OrderedList {
        source: clean_source,
        source_utf16: clean_utf16,
        start,
        delimiter,
        ..
    }] = clean.leaves()
    else {
        panic!("nine-digit target remains one ordered list");
    };
    assert_eq!(exact.terminal().list_source, *clean_source);
    assert_eq!(exact.terminal().list_source_utf16, *clean_utf16);
    assert_eq!(
        (exact.terminal().start, exact.terminal().delimiter),
        (*start, *delimiter)
    );
    plan = exact.take_base_plan().expect("restored marker base");
    drop(exact.take_target_source_lease());

    let nine_target = runtime
        .current_source_version()
        .expect("nine-digit version");
    let ten_digit = "1234567890.";
    runtime
        .apply_edit(
            nine_target,
            marker_start..marker_start + nine_digit.len(),
            ten_digit,
        )
        .expect("ten-digit marker target");
    let (prefix, suffix) = witnesses(&runtime);
    let mut invalid = M11OrderedListLocalDeltaJob::new(
        plan,
        prefix,
        suffix,
        runtime.snapshot_current_source().expect("ten-digit target"),
    )
    .expect("ten-digit bounded job");
    let error = loop {
        match invalid.poll(11) {
            Ok(M11OrderedListLocalDeltaPoll::Pending { transitions }) => {
                assert!(transitions <= 11);
            }
            Ok(M11OrderedListLocalDeltaPoll::Complete { .. }) => {
                panic!("ten digits must leave the CommonMark ordered-list subset")
            }
            Err(error) => break error,
        }
    };
    assert!(matches!(
        error,
        M11OrderedListLocalDeltaError::ConvergenceMismatch
    ));
    let mut cancellation = invalid
        .cancel_into_source_authority()
        .expect("cancel ten-digit fallback");
    plan = cancellation
        .take_base_plan()
        .expect("ten-digit fallback retained base");
    drop(cancellation.take_target_source_lease());

    let ten_target = runtime.current_source_version().expect("ten-digit version");
    runtime
        .apply_edit(
            ten_target,
            marker_start..marker_start + ten_digit.len(),
            "9)",
        )
        .expect("delimiter-change target");
    let (prefix, suffix) = witnesses(&runtime);
    let mut delimiter_change = M11OrderedListLocalDeltaJob::new(
        plan,
        prefix,
        suffix,
        runtime
            .snapshot_current_source()
            .expect("delimiter-change target"),
    )
    .expect("delimiter-change bounded job");
    let error = loop {
        match delimiter_change.poll(13) {
            Ok(M11OrderedListLocalDeltaPoll::Pending { transitions }) => {
                assert!(transitions <= 13);
            }
            Ok(M11OrderedListLocalDeltaPoll::Complete { .. }) => {
                panic!("delimiter change must leave the original ordered list")
            }
            Err(error) => break error,
        }
    };
    assert!(matches!(
        error,
        M11OrderedListLocalDeltaError::ConvergenceMismatch
    ));
    let mut cancellation = delimiter_change
        .cancel_into_source_authority()
        .expect("cancel delimiter fallback");
    drop(cancellation.take_base_plan());
    drop(cancellation.take_target_source_lease());

    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

#[test]
fn ordered_local_delta_has_typed_edges_and_rejects_stale_target_authority() {
    let source = long_ordered_list(100);
    let mut runtime = runtime(&source);
    let mut retained = retain_candidate(&mut runtime);

    let first = source.find("item-00000").expect("first ordered item");
    let first_fence = ordered_list_fence(&runtime, &retained, first);
    let first_error = match M11OrderedListLocalDeltaPlan::new(&runtime, first_fence, first..first) {
        Ok(_) => panic!("first ordered item requires boundary fallback"),
        Err(error) => error,
    };
    assert!(matches!(
        first_error,
        M11OrderedListLocalDeltaError::BoundaryFallback(
            M11OrderedListLocalDeltaBoundaryFallback::FirstItem
        )
    ));

    let last = source.find("item-00099").expect("last ordered item");
    let last_fence = ordered_list_fence(&runtime, &retained, last);
    let last_error = match M11OrderedListLocalDeltaPlan::new(&runtime, last_fence, last..last) {
        Ok(_) => panic!("last ordered item requires boundary fallback"),
        Err(error) => error,
    };
    assert!(matches!(
        last_error,
        M11OrderedListLocalDeltaError::BoundaryFallback(
            M11OrderedListLocalDeltaBoundaryFallback::LastItem
        )
    ));

    let caret =
        source.find("item-00050 café").expect("stale ordered item") + "item-00050 café".len();
    let fence = ordered_list_fence(&runtime, &retained, caret);
    let plan =
        M11OrderedListLocalDeltaPlan::new(&runtime, fence, caret..caret).expect("stale plan");
    let base = plan.source();
    let prefix_byte_end = plan.prefix_witness_byte_end();
    let prefix_utf16_end = plan.prefix_witness_utf16_end();
    let suffix_byte_start = plan.suffix_witness_byte_start();
    let suffix_utf16_start = plan.suffix_witness_utf16_start();
    let target_one = runtime
        .apply_edit(base, caret..caret, "x")
        .expect("first ordered target");
    let stale_prefix = runtime
        .mint_exact_unchanged_prefix_witness(base, prefix_byte_end, prefix_utf16_end)
        .expect("stale ordered prefix");
    let stale_suffix = runtime
        .mint_exact_unchanged_suffix_witness(base, suffix_byte_start, suffix_utf16_start)
        .expect("stale ordered suffix");
    runtime
        .apply_edit(target_one.source().current(), caret + 1..caret + 1, "y")
        .expect("newer ordered target");
    let error = match M11OrderedListLocalDeltaJob::new(
        plan,
        stale_prefix,
        stale_suffix,
        runtime
            .snapshot_current_source()
            .expect("newer ordered source"),
    ) {
        Ok(_) => panic!("stale ordered witnesses must not bind a newer target"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        M11OrderedListLocalDeltaError::AuthorityMismatch
    ));

    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

#[test]
fn checkpoint_free_unicode_insertion_is_fuel_partition_independent_and_cancellable() {
    let (unit_signature, unit_work) = unicode_insertion_local_delta(&[1], false);
    let (partitioned_signature, partitioned_work) =
        unicode_insertion_local_delta(&[2, 7, 31], true);
    assert_eq!(partitioned_signature, unit_signature);
    assert_eq!(partitioned_work, unit_work);
    assert_eq!(unit_work.base_physical_lines, 3);
    assert_eq!(unit_work.target_physical_lines, 3);
    assert_eq!(
        unit_work.base_source_bytes_discovered,
        unit_work.base_window_bytes
    );
    assert_eq!(
        unit_work.target_source_bytes_discovered,
        unit_work.target_window_bytes
    );
    assert_eq!(
        unit_work.base_source_bytes_read,
        unit_work.base_window_bytes
    );
    assert_eq!(
        unit_work.target_source_bytes_read,
        unit_work.target_window_bytes
    );
}

#[test]
fn checkpoint_free_local_delta_has_typed_boundary_and_hard_window_fallbacks() {
    const ITEM_COUNT: usize = 4_000;
    let source = long_bullet_list(ITEM_COUNT);
    let mut runtime = runtime(&source);
    let mut retained = retain_candidate(&mut runtime);

    let first_caret = source.find("item-00000").expect("first item") + 2;
    let first_fence = resolve_m11_published_bullet_list_leaf_fence(
        &runtime,
        &retained,
        M11BlockSequencePoint::new(
            first_caret,
            source[..first_caret].encode_utf16().count(),
            SourceBoundaryAffinity::After,
        ),
    )
    .expect("first-item list fence");
    let first_error =
        match M11BulletListLocalDeltaPlan::new(&runtime, first_fence, first_caret..first_caret) {
            Ok(_) => panic!("first-item insertion requires typed fallback"),
            Err(error) => error,
        };
    assert!(matches!(
        first_error,
        M11BulletListLocalDeltaError::BoundaryFallback(
            M11BulletListLocalDeltaBoundaryFallback::FirstItem
        )
    ));

    let last_caret = source.find("item-03999").expect("last item") + 2;
    let last_fence = resolve_m11_published_bullet_list_leaf_fence(
        &runtime,
        &retained,
        M11BlockSequencePoint::new(
            last_caret,
            source[..last_caret].encode_utf16().count(),
            SourceBoundaryAffinity::After,
        ),
    )
    .expect("last-item list fence");
    let last_error =
        match M11BulletListLocalDeltaPlan::new(&runtime, last_fence, last_caret..last_caret) {
            Ok(_) => panic!("last-item insertion requires typed fallback"),
            Err(error) => error,
        };
    assert!(matches!(
        last_error,
        M11BulletListLocalDeltaError::BoundaryFallback(
            M11BulletListLocalDeltaBoundaryFallback::LastItem
        )
    ));

    let wide_start = source.find("item-00100").expect("wide start");
    let wide_end = source.find("item-03900").expect("wide end") + "item-03900".len();
    let wide_fence = resolve_m11_published_bullet_list_leaf_fence(
        &runtime,
        &retained,
        M11BlockSequencePoint::new(
            wide_start,
            source[..wide_start].encode_utf16().count(),
            SourceBoundaryAffinity::After,
        ),
    )
    .expect("wide list fence");
    let wide_error =
        match M11BulletListLocalDeltaPlan::new(&runtime, wide_fence, wide_start..wide_end) {
            Ok(_) => panic!("oversized base window requires fallback"),
            Err(error) => error,
        };
    assert!(matches!(
        wide_error,
        M11BulletListLocalDeltaError::WindowTooLarge {
            bytes,
            cap: M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES
        } if bytes > M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES
    ));

    let caret = source.find("item-02000 café").expect("target-cap item") + "item-02000 café".len();
    let fence = resolve_m11_published_bullet_list_leaf_fence(
        &runtime,
        &retained,
        M11BlockSequencePoint::new(
            caret,
            source[..caret].encode_utf16().count(),
            SourceBoundaryAffinity::After,
        ),
    )
    .expect("target-cap list fence");
    let plan =
        M11BulletListLocalDeltaPlan::new(&runtime, fence, caret..caret).expect("target-cap plan");
    let base = plan.source();
    let prefix_byte_end = plan.prefix_witness_byte_end();
    let prefix_utf16_end = plan.prefix_witness_utf16_end();
    let suffix_byte_start = plan.suffix_witness_byte_start();
    let suffix_utf16_start = plan.suffix_witness_utf16_start();
    let oversized = "x".repeat(M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES + 1);
    runtime
        .apply_edit(base, caret..caret, &oversized)
        .expect("oversized target edit");
    let prefix = runtime
        .mint_exact_unchanged_prefix_witness(base, prefix_byte_end, prefix_utf16_end)
        .expect("target-cap prefix");
    let suffix = runtime
        .mint_exact_unchanged_suffix_witness(base, suffix_byte_start, suffix_utf16_start)
        .expect("target-cap suffix");
    let target_error = match M11BulletListLocalDeltaJob::new(
        plan,
        prefix,
        suffix,
        runtime.snapshot_current_source().expect("oversized target"),
    ) {
        Ok(_) => panic!("oversized target window requires fallback"),
        Err(error) => error,
    };
    assert!(matches!(
        target_error,
        M11BulletListLocalDeltaError::WindowTooLarge {
            bytes,
            cap: M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES
        } if bytes > M11_BULLET_LIST_LOCAL_DELTA_MAX_BYTES
    ));

    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

#[test]
fn checkpoint_free_local_delta_rejects_stale_target_authority() {
    let source = long_bullet_list(1_000);
    let caret = source.find("item-00500 café").expect("stale item") + "item-00500 café".len();
    let mut runtime = runtime(&source);
    let mut retained = retain_candidate(&mut runtime);
    let fence = resolve_m11_published_bullet_list_leaf_fence(
        &runtime,
        &retained,
        M11BlockSequencePoint::new(
            caret,
            source[..caret].encode_utf16().count(),
            SourceBoundaryAffinity::After,
        ),
    )
    .expect("stale list fence");
    let plan = M11BulletListLocalDeltaPlan::new(&runtime, fence, caret..caret).expect("stale plan");
    let base = plan.source();
    let prefix_byte_end = plan.prefix_witness_byte_end();
    let prefix_utf16_end = plan.prefix_witness_utf16_end();
    let suffix_byte_start = plan.suffix_witness_byte_start();
    let suffix_utf16_start = plan.suffix_witness_utf16_start();
    let target_one = runtime
        .apply_edit(base, caret..caret, "x")
        .expect("first target");
    let target_one_source = target_one.source().current();
    let stale_prefix = runtime
        .mint_exact_unchanged_prefix_witness(base, prefix_byte_end, prefix_utf16_end)
        .expect("stale prefix");
    let stale_suffix = runtime
        .mint_exact_unchanged_suffix_witness(base, suffix_byte_start, suffix_utf16_start)
        .expect("stale suffix");
    runtime
        .apply_edit(target_one_source, caret + 1..caret + 1, "y")
        .expect("newer target");
    let error = match M11BulletListLocalDeltaJob::new(
        plan,
        stale_prefix,
        stale_suffix,
        runtime
            .snapshot_current_source()
            .expect("newer target source"),
    ) {
        Ok(_) => panic!("stale witnesses must not bind a newer target"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        M11BulletListLocalDeltaError::AuthorityMismatch
    ));

    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

#[test]
fn checkpoint_free_local_delta_uses_predecessor_indent_and_rejects_nested_shape() {
    let source = long_bullet_list(2_000);
    let line_start = source.find("- item-01000").expect("edited line");
    let line_end = line_start + source[line_start..].find("\r\n").expect("line ending");
    let mut runtime = runtime(&source);
    let mut retained = retain_candidate(&mut runtime);
    let point_utf16 = source[..line_start].encode_utf16().count();
    let fence = resolve_m11_published_bullet_list_leaf_fence(
        &runtime,
        &retained,
        M11BlockSequencePoint::new(line_start, point_utf16, SourceBoundaryAffinity::After),
    )
    .expect("published list fence");
    let plan = M11BulletListLocalDeltaPlan::new(&runtime, fence, line_start..line_end)
        .expect("local plan");
    let base = plan.source();
    let prefix_byte_end = plan.prefix_witness_byte_end();
    let prefix_utf16_end = plan.prefix_witness_utf16_end();
    let suffix_byte_start = plan.suffix_witness_byte_start();
    let suffix_utf16_start = plan.suffix_witness_utf16_start();
    runtime
        .apply_edit(base, line_start..line_end, "  - nested")
        .expect("target edit");
    let prefix = runtime
        .mint_exact_unchanged_prefix_witness(base, prefix_byte_end, prefix_utf16_end)
        .expect("unchanged predecessor");
    let suffix = runtime
        .mint_exact_unchanged_suffix_witness(base, suffix_byte_start, suffix_utf16_start)
        .expect("unchanged successor");
    let mut job = M11BulletListLocalDeltaJob::new(
        plan,
        prefix,
        suffix,
        runtime.snapshot_current_source().expect("target source"),
    )
    .expect("local delta job");
    let error = loop {
        match job.poll(17) {
            Ok(M11BulletListLocalDeltaPoll::Pending { transitions }) => {
                assert!(transitions <= 17);
            }
            Ok(M11BulletListLocalDeltaPoll::Complete { .. }) => {
                panic!("nested shape must leave the same-list fast path")
            }
            Err(error) => break error,
        }
    };
    assert!(matches!(
        error,
        M11BulletListLocalDeltaError::UnsupportedList(M11ListUnsupportedReason::Nested)
    ));
    let mut cancellation = job
        .cancel_into_source_authority()
        .expect("cancel faulted local job");
    drop(cancellation.take_base_plan());
    drop(cancellation.take_target_source_lease());

    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

#[test]
fn published_20000_item_list_projects_only_the_selected_unicode_crlf_item() {
    const ITEM_COUNT: usize = 20_000;
    const SELECTED: usize = 10_000;
    let mut source = String::new();
    for ordinal in 0..ITEM_COUNT {
        source.push_str(&format!("- item-{ordinal:04} **café😀**\r\n"));
    }
    let selected_marker = format!("- item-{SELECTED:04}");
    let item_start = source.find(&selected_marker).expect("selected item");
    let item_end = item_start + source[item_start..].find("\r\n").expect("selected CRLF") + 2;
    let content_start = item_start + 2;
    let content_end = item_end - 2;
    let point_byte = content_start + format!("item-{SELECTED:04} ").len();
    let point_utf16 = source[..point_byte].encode_utf16().count();

    let mut runtime = runtime(&source);
    let version = runtime.current_source_version().expect("source");
    let mut retained = retain_candidate(&mut runtime);
    let list_fence = resolve_m11_published_bullet_list_leaf_fence(
        &runtime,
        &retained,
        M11BlockSequencePoint::new(point_byte, point_utf16, SourceBoundaryAffinity::After),
    )
    .expect("published list");
    let list_receipt = list_fence.query_receipt();
    assert!(
        list_fence.block_source_range().len() > 8 * 1024,
        "selected-item lookup must not inherit the old whole-list projection cap"
    );
    drop(list_fence);

    let outcome = resolve_m11_published_bullet_list_item_fences(
        &runtime,
        &retained,
        M11BlockSequencePoint::new(point_byte, point_utf16, SourceBoundaryAffinity::After),
    )
    .expect("selected item fence");
    let M11PublishedBulletListItemInlineFenceOutcome::Inline(item) = outcome else {
        panic!("nonempty item must mint inline authority");
    };
    assert_eq!(item.source(), version);
    assert_eq!(item.item_ordinal(), SELECTED as u32);
    assert_eq!(item.block_source_range(), 0..source.len() as u32);
    assert_eq!(item.item_source_range(), item_start as u32..item_end as u32);
    assert_eq!(
        item.content_source_range(),
        content_start as u32..content_end as u32
    );
    assert_eq!(
        item.content_source_utf16_range(),
        source[..content_start].encode_utf16().count() as u32
            ..source[..content_end].encode_utf16().count() as u32
    );
    assert_eq!(item.binding(), binding());
    assert_eq!(item.query_receipt(), list_receipt);

    let (projection_fence, inline) = item.into_projection_and_inline_fences();
    assert_eq!(projection_fence.source(), version);
    assert_eq!(projection_fence.item_ordinal(), SELECTED as u32);
    assert_eq!(
        projection_fence.source_discovery_bytes(),
        (item_end - item_start) as u32,
        "rank/select must inspect only the selected physical line"
    );
    assert_eq!(projection_fence.physical_line_ending(), M11LineEnding::CrLf);
    assert_eq!(
        projection_fence.canonical_line_ending(),
        M11LineEnding::CrLf
    );
    assert!(!projection_fence.terminal_empty());
    let output = complete_compact_item_projection(
        M11BulletListItemProjectionJob::new(&runtime, projection_fence)
            .expect("compact selected-item job"),
        &mut runtime,
    );
    assert_eq!(output.selected_item_ordinal(), SELECTED as u32);
    assert_eq!(output.canonical_line_ending(), M11LineEnding::CrLf);
    assert!(!output.terminal_empty());
    let (mut root, _, _, _) = output.into_parts();
    assert_eq!(root.descriptor().source(), version);
    assert_eq!(
        root.descriptor().projection_kind(),
        M11MarkedLineProjectionKind::BulletList
    );
    assert_eq!(
        root.descriptor().physical_block_range(),
        &(0..source.len() as u32)
    );
    assert_eq!(
        root.descriptor().requested_window(),
        &(item_start as u32..item_end as u32)
    );
    assert_eq!(root.descriptor().line_count(), 1);
    assert_eq!(root.descriptor().logical_page_count(), 1);
    let projected_utf8 = (item_end - item_start - 2) as u32;
    let projected_utf16 = source[content_start..item_end].encode_utf16().count() as u32;
    assert_eq!(root.descriptor().projected_utf8_length(), projected_utf8);
    assert_eq!(root.descriptor().projected_utf16_length(), projected_utf16);
    let mut cursor = root
        .cursor(
            &runtime,
            version,
            binding().syntax_profile(),
            0..source.len() as u32,
            item_start as u32..item_end as u32,
        )
        .expect("compact selected-item cursor");
    let mut records = Vec::new();
    loop {
        match cursor.poll(&runtime).expect("compact cursor poll") {
            M11BlockQuoteProjectionCursorPoll::Pending { transitions } => {
                assert!(transitions <= 1);
            }
            M11BlockQuoteProjectionCursorPoll::Line { transitions, line } => {
                assert!(transitions <= 1);
                records.push(line);
            }
            M11BlockQuoteProjectionCursorPoll::Complete { transitions } => {
                assert!(transitions <= 1);
                break;
            }
        }
    }
    assert_eq!(
        records,
        vec![BlockQuoteLineV1::bullet_item(
            item_start as u32,
            (item_end - item_start) as u32,
            2,
            0,
            2,
            (content_end - content_start) as u32,
            source[content_start..content_end].encode_utf16().count() as u32,
        )
        .expect("selected Unicode/CRLF item")]
    );
    drop(cursor);
    root.begin_release(&mut runtime)
        .expect("begin compact root release");
    while !root
        .poll_release(&mut runtime, 1)
        .expect("compact root release")
        .complete()
    {}
    drop(root);

    assert_eq!(inline.block_source_range(), 0..source.len() as u32);
    assert_eq!(
        inline.inline_source_range(),
        content_start as u32..content_end as u32
    );
    assert_eq!(inline.query_receipt(), list_receipt);
    let mut job = M11InlineProjectionJob::new_for_published_inline_leaf(&runtime, inline)
        .expect("existing inline job accepts selected item fence");
    abort_inline_job(&mut job, &mut runtime);
    drop(job);

    let cancellation = resolve_m11_published_bullet_list_item_fences(
        &runtime,
        &retained,
        M11BlockSequencePoint::new(point_byte, point_utf16, SourceBoundaryAffinity::After),
    )
    .expect("second selected item fence");
    let M11PublishedBulletListItemInlineFenceOutcome::Inline(cancellation) = cancellation else {
        panic!("nonempty cancellation item must mint authority");
    };
    let (projection_fence, unused_inline) = cancellation.into_projection_and_inline_fences();
    drop(unused_inline);
    let mut cancelled = M11BulletListItemProjectionJob::new(&runtime, projection_fence)
        .expect("cancellable compact job");
    let poll = cancelled
        .poll(&mut runtime, 1)
        .expect("bounded compact work");
    assert!(poll.transitions() <= 1);
    cancelled
        .begin_cancel(&mut runtime)
        .expect("begin compact cancellation");
    loop {
        let poll = cancelled
            .poll_cancel(&mut runtime, 1)
            .expect("compact cancellation poll");
        assert!(poll.transitions() <= 1);
        if poll.complete() {
            break;
        }
    }
    drop(cancelled);

    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

#[test]
fn published_terminal_empty_list_item_returns_typed_no_fence() {
    let source = "\u{feff}  -  α😀\r\n  - beta\r\n-   ";
    let item_start = source.rfind("-   ").expect("terminal item");
    let point = item_start + 1;
    let point_utf16 = source[..point].encode_utf16().count();
    let mut runtime = runtime(source);
    let version = runtime.current_source_version().expect("source");
    let mut retained = retain_candidate(&mut runtime);

    let outcome = resolve_m11_published_bullet_list_item_inline_fence(
        &runtime,
        &retained,
        M11BlockSequencePoint::new(point, point_utf16, SourceBoundaryAffinity::After),
    )
    .expect("terminal empty resolution");
    let M11PublishedBulletListItemInlineFenceOutcome::TerminalEmpty(empty) = outcome else {
        panic!("marker-only terminal item must not mint inline authority");
    };
    assert_eq!(empty.source(), version);
    assert_eq!(empty.item_ordinal(), 2);
    assert_eq!(empty.block_source_range(), 0..source.len() as u32);
    assert_eq!(
        empty.item_source_range(),
        item_start as u32..source.len() as u32
    );
    assert_eq!(
        empty.content_source_range(),
        source.len() as u32..source.len() as u32
    );
    assert_eq!(
        empty.content_source_utf16_range(),
        source.encode_utf16().count() as u32..source.encode_utf16().count() as u32
    );
    assert_eq!(empty.binding(), binding());

    let projection_fence = empty.into_projection_fence();
    assert_eq!(projection_fence.item_ordinal(), 2);
    assert_eq!(
        projection_fence.source_discovery_bytes(),
        4,
        "EOF policy uses the predecessor rank/select terminator without scanning it"
    );
    assert_eq!(projection_fence.physical_line_ending(), M11LineEnding::Eof);
    assert_eq!(
        projection_fence.canonical_line_ending(),
        M11LineEnding::CrLf,
        "unterminated EOF items inherit the authenticated predecessor ending"
    );
    assert!(projection_fence.terminal_empty());
    let output = complete_compact_item_projection(
        M11BulletListItemProjectionJob::new(&runtime, projection_fence)
            .expect("terminal compact projection"),
        &mut runtime,
    );
    assert_eq!(output.selected_item_ordinal(), 2);
    assert_eq!(output.canonical_line_ending(), M11LineEnding::CrLf);
    assert!(output.terminal_empty());
    let (mut root, _, _, _) = output.into_parts();
    assert_eq!(root.descriptor().line_count(), 1);
    assert_eq!(root.descriptor().projected_utf8_length(), 0);
    assert_eq!(root.descriptor().projected_utf16_length(), 0);
    assert_eq!(
        root.descriptor().requested_window(),
        &(item_start as u32..source.len() as u32)
    );
    let mut cursor = root
        .cursor(
            &runtime,
            version,
            binding().syntax_profile(),
            0..source.len() as u32,
            item_start as u32..source.len() as u32,
        )
        .expect("terminal compact cursor");
    let mut records = Vec::new();
    loop {
        match cursor.poll(&runtime).expect("terminal cursor poll") {
            M11BlockQuoteProjectionCursorPoll::Pending { .. } => {}
            M11BlockQuoteProjectionCursorPoll::Line { line, .. } => records.push(line),
            M11BlockQuoteProjectionCursorPoll::Complete { .. } => break,
        }
    }
    assert_eq!(
        records,
        vec![
            BlockQuoteLineV1::bullet_item(item_start as u32, 4, 4, 0, 2, 0, 0,)
                .expect("terminal compact item")
        ]
    );
    drop(cursor);
    root.begin_release(&mut runtime)
        .expect("begin terminal compact release");
    while !root
        .poll_release(&mut runtime, 1)
        .expect("terminal compact release")
        .complete()
    {}
    drop(root);

    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

#[test]
fn terminal_empty_item_uses_constant_rank_select_ending_for_a_long_predecessor() {
    let predecessor_content = "😀".repeat(32 * 1024);
    let source = format!("- {predecessor_content}\r\n-   ");
    let item_start = source.rfind("-   ").expect("terminal item");
    let point = item_start + 1;
    let point_utf16 = source[..point].encode_utf16().count();
    let mut runtime = runtime(&source);
    let mut retained = retain_candidate(&mut runtime);

    let outcome = resolve_m11_published_bullet_list_item_fences(
        &runtime,
        &retained,
        M11BlockSequencePoint::new(point, point_utf16, SourceBoundaryAffinity::After),
    )
    .expect("terminal empty resolution after long predecessor");
    let M11PublishedBulletListItemInlineFenceOutcome::TerminalEmpty(empty) = outcome else {
        panic!("marker-only terminal item must not mint inline authority");
    };
    let projection = empty.into_projection_fence();
    assert_eq!(projection.physical_line_ending(), M11LineEnding::Eof);
    assert_eq!(projection.canonical_line_ending(), M11LineEnding::CrLf);
    assert_eq!(
        projection.source_discovery_bytes(),
        4,
        "128 KiB Unicode predecessor must contribute no scanned source bytes"
    );
    drop(projection);

    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

#[test]
fn published_unicode_crlf_and_lazy_lines_project_with_unit_fuel() {
    let source = "\u{feff}   > α😀\r\n> β\rlazy😀\0";
    let mut runtime = runtime(source);
    let version = runtime.current_source_version().expect("source");
    let mut retained = retain_candidate(&mut runtime);
    let fence = fence(&runtime, &retained);
    assert_eq!(fence.source(), version);
    assert_eq!(fence.block_source_range(), 0..source.len() as u32);
    assert_eq!(fence.line_count(), 3);
    assert_eq!(fence.projected_utf8_length(), 20);
    assert_eq!(fence.projected_utf16_length(), 14);

    let mut job = M11BlockQuoteProjectionJob::new(&runtime, fence).expect("projection job");
    loop {
        let poll = job.poll(&mut runtime, 1).expect("projection poll");
        assert!(poll.transitions() <= 1);
        if poll.status() == M11BlockQuoteProjectionJobPollStatus::Pending {
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
    assert_eq!(root.descriptor().projected_utf8_length(), 20);
    assert_eq!(root.descriptor().projected_utf16_length(), 14);
    assert_eq!(root.descriptor().line_count(), 3);

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
            M11BlockQuoteProjectionCursorPoll::Pending { transitions } => {
                assert!(transitions <= 1);
            }
            M11BlockQuoteProjectionCursorPoll::Line { transitions, line } => {
                assert!(transitions <= 1);
                records.push(line);
            }
            M11BlockQuoteProjectionCursorPoll::Complete { transitions } => {
                assert!(transitions <= 1);
                break;
            }
        }
    }
    assert_eq!(
        records,
        vec![
            BlockQuoteLineV1::marked(0, 16, 8, 6).expect("BOM/Unicode/CRLF marked line"),
            BlockQuoteLineV1::marked(16, 5, 2, 2).expect("Unicode/CR marked line"),
            BlockQuoteLineV1::lazy(21, 9, 9).expect("Unicode/NUL lazy EOF line"),
        ]
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
fn in_flight_block_quote_projection_cancellation_reclaims_every_owner() {
    let source = "> q\n".repeat(500);
    let mut runtime = runtime(&source);
    let mut retained = retain_candidate(&mut runtime);
    let fence = fence(&runtime, &retained);
    let mut job = M11BlockQuoteProjectionJob::new(&runtime, fence).expect("projection job");
    let poll = job.poll(&mut runtime, 31).expect("bounded work");
    assert!(poll.transitions() <= 31);

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
fn published_tight_bullet_items_project_with_unit_fuel_and_distinct_kind() {
    let source = "\u{feff}  -  α😀\r\n  - β\r-   ";
    let mut runtime = runtime(source);
    let version = runtime.current_source_version().expect("source");
    let mut retained = retain_candidate(&mut runtime);
    let fence = bullet_list_fence(&runtime, &retained);
    assert_eq!(fence.source(), version);
    assert_eq!(fence.block_source_range(), 0..source.len() as u32);
    assert_eq!(fence.item_count(), 3);
    assert_eq!(fence.paragraph_count(), 2);
    assert_eq!(fence.marker(), b'-');
    assert_eq!(fence.terminal_empty_relative_start(), Some(23));
    assert_eq!(fence.projected_utf8_length(), 11);
    assert_eq!(fence.projected_utf16_length(), 7);

    let mut job = M11BulletListProjectionJob::new(&runtime, fence).expect("projection job");
    loop {
        let poll = job.poll(&mut runtime, 1).expect("projection poll");
        assert!(poll.transitions() <= 1);
        if poll.status() == M11BlockQuoteProjectionJobPollStatus::Pending {
            assert_ne!(poll.transitions(), 0, "ready exact job must not stall");
        } else {
            break;
        }
    }
    let mut root = job.take_root().expect("ready projection root");
    drop(job);
    assert_eq!(
        root.descriptor().projection_kind(),
        M11MarkedLineProjectionKind::BulletList
    );
    assert_eq!(root.descriptor().source(), version);
    assert_eq!(root.descriptor().projected_utf8_length(), 11);
    assert_eq!(root.descriptor().projected_utf16_length(), 7);
    assert_eq!(root.descriptor().line_count(), 3);

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
            M11BlockQuoteProjectionCursorPoll::Pending { transitions } => {
                assert!(transitions <= 1);
            }
            M11BlockQuoteProjectionCursorPoll::Line { transitions, line } => {
                assert!(transitions <= 1);
                records.push(line);
            }
            M11BlockQuoteProjectionCursorPoll::Complete { transitions } => {
                assert!(transitions <= 1);
                break;
            }
        }
    }
    assert_eq!(
        records,
        vec![
            BlockQuoteLineV1::bullet_item(0, 16, 8, 3, 8, 6, 3).expect("BOM/Unicode/CRLF item"),
            BlockQuoteLineV1::bullet_item(16, 7, 4, 0, 4, 2, 1).expect("Unicode/CR item"),
            BlockQuoteLineV1::bullet_item(23, 4, 4, 0, 2, 0, 0)
                .expect("terminal padded empty item"),
        ]
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
fn terminal_empty_only_bullet_list_keeps_zero_projection_lengths() {
    let source = "-";
    let mut runtime = runtime(source);
    let mut retained = retain_candidate(&mut runtime);
    let fence = bullet_list_fence(&runtime, &retained);
    assert_eq!(fence.item_count(), 1);
    assert_eq!(fence.paragraph_count(), 0);
    assert_eq!(fence.terminal_empty_relative_start(), Some(0));
    assert_eq!(fence.projected_utf8_length(), 0);
    assert_eq!(fence.projected_utf16_length(), 0);

    let mut job = M11BulletListProjectionJob::new(&runtime, fence).expect("projection job");
    loop {
        let poll = job.poll(&mut runtime, 1).expect("projection poll");
        assert!(poll.transitions() <= 1);
        if poll.status() != M11BlockQuoteProjectionJobPollStatus::Pending {
            break;
        }
    }
    let mut root = job.take_root().expect("ready zero-length projection");
    drop(job);
    assert_eq!(root.descriptor().projected_utf8_length(), 0);
    assert_eq!(root.descriptor().projected_utf16_length(), 0);
    root.begin_release(&mut runtime)
        .expect("begin root release");
    while !root
        .poll_release(&mut runtime, 1)
        .expect("root release")
        .complete()
    {}
    drop(root);
    close_retained(&mut retained, &mut runtime);
    drop(retained);
    close_runtime(runtime);
}

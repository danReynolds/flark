use flark_engine::m11_host::M11_CANDIDATE_ARENA_MAX_SLOTS;
use flark_engine::parser_internal::{
    M11BlockSequenceEntryKind, M11BlockSequencePoint, M11CandidateHost, M11CandidatePublication,
    M11InlineProjectionRoot, M11InstalledCandidate, M11OwnedSnapshotPoll,
    M11ParserSourceRangeAuthority, M11RecursiveGreenCoveragePart, M11RecursiveGreenPoint,
    M11ReferenceResolution, M11RetainedCandidatePublication, M11Role, M11SnapshotFrameKind,
    M11SnapshotPoll, M11_MAX_ROLE_RECORDS,
};
use flark_engine::{
    ArenaLimits, DocumentRuntime, DocumentRuntimeConfig, DocumentRuntimeError, ParserProfileId,
    RuntimeSourceFactsPoll, SourceBoundaryAffinity, SourceFactsRootLimits, SourceFactsScanProfile,
    SourceSnapshotLease, SOURCE_CURSOR_WINDOW_BYTES,
};
use flark_parser::{
    resolve_m11_published_block_quote_leaf_fence, resolve_m11_published_bullet_list_leaf_fence,
    M11CandidateDerivationError, M11CandidateRoleBytes, M11CleanParseJob, M11CleanParsePoll,
    M11InlineProjectionJob, M11InlineProjectionJobPollStatus, M11InlineProjectionPublication,
    M11ParserBinding, M11ParserCandidate, M11ParserCandidateWriter, M11ParserCandidateWriterPoll,
    M11ParserInlinePublication, M11PersistentRecursiveGreenBuildStatus,
    M11PersistentRecursiveGreenCleanPlan, M11PersistentRecursiveGreenSession,
    M11ReferenceCookReceipt, M11_GREEN_RECORD_BYTES, M11_INLINE_META_MAGIC, M11_INLINE_SCHEMA,
    M11_PROJECTION_RECORD_BYTES,
};

const DOCUMENT: [u8; 16] = [9; 16];
type SnapshotFrames = (Box<[u8]>, Vec<Box<[u8]>>, Box<[u8]>);

fn producer_runtime(text: &str) -> DocumentRuntime {
    DocumentRuntime::new(
        text,
        DocumentRuntimeConfig {
            arena_limits: ArenaLimits {
                max_slots: M11_CANDIDATE_ARENA_MAX_SLOTS,
                max_live_payload_bytes: 64 * 1024 * 1024,
                max_children_per_node: M11_MAX_ROLE_RECORDS,
            },
            ..DocumentRuntimeConfig::default()
        },
    )
    .expect("producer runtime")
}

fn close_runtime(mut runtime: DocumentRuntime) {
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(256).expect("runtime close").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
}

fn prepare_runtime_source_facts(runtime: &mut DocumentRuntime, spacing: usize) {
    let profile = SourceFactsScanProfile::new(spacing).expect("source-fact profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let expected = runtime
        .begin_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("begin runtime source facts");
    loop {
        match runtime
            .poll_source_facts(7, 3)
            .expect("runtime source-fact poll")
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
                panic!("clean source-fact scan reported incremental progress")
            }
        }
    }
}

fn parse(lease: SourceSnapshotLease) -> flark_parser::M11CleanDocumentResult {
    let mut job = M11CleanParseJob::new(lease).expect("clean parse job");
    loop {
        match job.poll(1).expect("clean parse poll") {
            M11CleanParsePoll::Pending { transitions } => assert_eq!(transitions, 1),
            M11CleanParsePoll::Complete {
                transitions,
                result,
            } => {
                assert!(transitions <= 1);
                return result;
            }
        }
    }
}

fn recursive_green_session(runtime: &mut DocumentRuntime) -> M11PersistentRecursiveGreenSession {
    let plan = M11PersistentRecursiveGreenCleanPlan::new(
        runtime
            .snapshot_current_source()
            .expect("Green scanner lease"),
        runtime
            .snapshot_current_source()
            .expect("Green writer lease"),
        1,
    )
    .expect("recursive Green clean plan");
    let mut build = plan.begin(runtime).expect("recursive Green clean build");
    loop {
        let poll = build.poll(runtime, 64).expect("recursive Green poll");
        if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
            return build.take_session().expect("recursive Green session");
        }
    }
}

fn close_recursive_green_session(
    runtime: &mut DocumentRuntime,
    session: &mut M11PersistentRecursiveGreenSession,
) {
    session.begin_release(runtime).expect("begin Green release");
    while !session
        .poll_release(runtime, 64)
        .expect("poll Green release")
    {}
}

fn inline_publication(
    runtime: &mut DocumentRuntime,
    lease: SourceSnapshotLease,
    result: &flark_parser::M11CleanDocumentResult,
    parser_profile: ParserProfileId,
) -> (
    M11ParserSourceRangeAuthority,
    M11InlineProjectionPublication,
) {
    let visible = result.visible_source().expect("Paragraph visible source");
    let authority = M11ParserSourceRangeAuthority::new(
        runtime,
        lease,
        usize::try_from(visible.start).expect("visible start")
            ..usize::try_from(visible.end).expect("visible end"),
    )
    .expect("inline source authority");
    let mut job = M11InlineProjectionJob::new(
        runtime,
        authority,
        result,
        M11ParserBinding::current(parser_profile),
    )
    .expect("inline Projection job");
    loop {
        let poll = job.poll(runtime, 1).expect("inline Projection poll");
        assert!(poll.transitions() <= 1);
        if poll.status() == M11InlineProjectionJobPollStatus::Complete {
            break;
        }
    }
    let output = job.take_output().expect("inline Projection output");
    let parts = output.into_publication_parts();
    assert_eq!(parts.source(), result.source_version());
    assert_eq!(parts.source_range(), visible);
    assert_eq!(parts.parser_profile(), parser_profile);
    let (_, _, _, authority, publication) = parts.into_parts();
    drop(job);
    (authority, publication)
}

fn release_inline_root(runtime: &mut DocumentRuntime, root: &mut M11InlineProjectionRoot) {
    root.begin_release(runtime)
        .expect("begin inline root release");
    loop {
        let poll = root
            .poll_release(runtime, 1)
            .expect("poll inline root release");
        assert!(poll.receipt().transitions <= 1);
        if poll.complete() {
            break;
        }
    }
}

fn candidate(runtime: &mut DocumentRuntime, spacing: usize) -> M11ParserCandidate {
    prepare_runtime_source_facts(runtime, spacing);
    let result = parse(
        runtime
            .certified_source()
            .expect("completed certification")
            .exact_parse_lease(),
    );
    let certified = runtime
        .take_certified_source()
        .expect("runtime certification");
    M11ParserCandidate::derive_segmented(certified, result).expect("segmented parser candidate")
}

fn node_payload_offset(frame: &[u8]) -> usize {
    let child_count = usize::from(u16::from_le_bytes(
        frame[12..14].try_into().expect("child count"),
    ));
    20 + child_count * 8
}

fn node_payload(frame: &[u8]) -> &[u8] {
    &frame[node_payload_offset(frame)..]
}

fn repair_snapshot_digest(begin: &[u8], nodes: &[Box<[u8]>], end: &mut [u8]) {
    let mut digest = blake3::Hasher::new();
    digest.update(b"flark.candidate.snapshot.v1\0");
    digest.update(begin);
    for node in nodes {
        digest.update(node);
    }
    end[28..].copy_from_slice(digest.finalize().as_bytes());
}

fn expect_semantic_node_rejection(host: &mut M11CandidateHost, begin: &[u8], nodes: &[Box<[u8]>]) {
    host.begin_snapshot(begin).expect("corrupt offer begin");
    let error = nodes
        .iter()
        .find_map(|node| host.offer_node(node).err())
        .expect("semantic measured-sequence mutation must fail during import");
    assert!(error.is_invalid_snapshot());
    while !host.poll_reclaim(1).expect("corrupt offer reclaim") {}
}

fn expect_repaired_semantic_rejection(
    host: &mut M11CandidateHost,
    installed: M11InstalledCandidate,
    begin: &[u8],
    nodes: &[Box<[u8]>],
    mut end: Box<[u8]>,
) {
    repair_snapshot_digest(begin, nodes, &mut end);
    expect_semantic_node_rejection(host, begin, nodes);
    assert_eq!(host.installed(), Some(installed));
}

#[derive(Clone, Copy, Debug)]
enum PersistentTreeMutation {
    LeafByte,
    ChildOrder,
    BranchLeafCount,
    BranchProfile,
    BranchSegmentMeasure,
    BranchCommitment,
}

fn mutated_persistent_nodes(
    pristine: &[Box<[u8]>],
    mutation: PersistentTreeMutation,
) -> Vec<Box<[u8]>> {
    let mut nodes = pristine.to_vec();
    if matches!(mutation, PersistentTreeMutation::LeafByte) {
        let leaf = nodes
            .iter_mut()
            .find(|frame| node_payload(frame).starts_with(b"SFL2"))
            .expect("persistent SourceFacts leaf");
        *leaf.last_mut().expect("leaf semantic byte") ^= 0x01;
        return nodes;
    }

    let branch = nodes
        .iter_mut()
        .find(|frame| node_payload(frame).starts_with(b"SFB2"))
        .expect("persistent SourceFacts branch");
    let payload = node_payload_offset(branch);
    match mutation {
        PersistentTreeMutation::LeafByte => unreachable!("handled above"),
        PersistentTreeMutation::ChildOrder => {
            assert_eq!(
                u16::from_le_bytes(branch[12..14].try_into().expect("branch child count")),
                2
            );
            let left: [u8; 8] = branch[20..28].try_into().expect("left child");
            let right: [u8; 8] = branch[28..36].try_into().expect("right child");
            branch[20..28].copy_from_slice(&right);
            branch[28..36].copy_from_slice(&left);
        }
        PersistentTreeMutation::BranchLeafCount => branch[payload + 8] ^= 0x01,
        PersistentTreeMutation::BranchProfile => branch[payload + 18] ^= 0x01,
        PersistentTreeMutation::BranchSegmentMeasure => branch[payload + 38] ^= 0x01,
        PersistentTreeMutation::BranchCommitment => {
            *branch.last_mut().expect("stored commitment byte") ^= 0x01;
        }
    }
    nodes
}

fn publish(
    runtime: &mut DocumentRuntime,
    candidate: M11ParserCandidate,
    publication: [u8; 16],
    generation: u64,
) -> M11CandidatePublication {
    publish_with_receipt(runtime, candidate, publication, generation).0
}

fn publish_with_receipt(
    runtime: &mut DocumentRuntime,
    candidate: M11ParserCandidate,
    publication: [u8; 16],
    generation: u64,
) -> (M11CandidatePublication, M11ReferenceCookReceipt) {
    let mut writer = candidate
        .into_writer(runtime, DOCUMENT, publication, generation)
        .expect("candidate writer");
    loop {
        match writer.poll(runtime, 1).expect("candidate writer poll") {
            M11ParserCandidateWriterPoll::Pending { transitions } => {
                assert!(transitions <= 1);
            }
            M11ParserCandidateWriterPoll::Published {
                transitions,
                publication,
            } => {
                assert!(transitions <= 1);
                return (*publication, writer.reference_cook_receipt());
            }
        }
    }
}

fn retain(
    runtime: &mut DocumentRuntime,
    candidate: M11ParserCandidate,
    publication: [u8; 16],
    generation: u64,
) -> M11RetainedCandidatePublication {
    let mut writer = candidate
        .into_writer(runtime, DOCUMENT, publication, generation)
        .expect("candidate writer");
    let publication = loop {
        match writer.poll(runtime, 1).expect("candidate writer poll") {
            M11ParserCandidateWriterPoll::Pending { transitions } => {
                assert!(transitions <= 1);
            }
            M11ParserCandidateWriterPoll::Published {
                transitions,
                publication,
            } => {
                assert!(transitions <= 1);
                break publication;
            }
        }
    };
    drop(writer);

    retain_publication(runtime, publication)
}

fn retain_publication(
    runtime: &mut DocumentRuntime,
    publication: Box<M11CandidatePublication>,
) -> M11RetainedCandidatePublication {
    let mut stream = publication
        .into_snapshot_stream(runtime)
        .expect("owned snapshot stream");
    assert_eq!(
        stream.begin_frame().expect("snapshot begin").kind,
        M11SnapshotFrameKind::Begin
    );
    loop {
        match stream.poll(runtime, 1).expect("owned snapshot poll") {
            M11OwnedSnapshotPoll::Pending { transitions } => assert!(transitions <= 1),
            M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                panic!("full candidate unexpectedly requested replay")
            }
            M11OwnedSnapshotPoll::Frame { transitions, frame } => {
                assert!(transitions <= 1);
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

fn collect_frames(
    runtime: &DocumentRuntime,
    publication: &M11CandidatePublication,
) -> SnapshotFrames {
    let mut encoder = publication
        .snapshot_encoder(runtime)
        .expect("snapshot encoder");
    let begin = encoder.begin_frame().expect("begin frame");
    let mut nodes = Vec::new();
    loop {
        match encoder.poll(1).expect("snapshot poll") {
            M11SnapshotPoll::Pending { transitions } => assert_eq!(transitions, 1),
            M11SnapshotPoll::Node { transitions, bytes } => {
                assert_eq!(transitions, 1);
                nodes.push(bytes);
            }
            M11SnapshotPoll::End { transitions, bytes } => {
                assert!(transitions <= 1);
                return (begin, nodes, bytes);
            }
        }
    }
}

fn install(
    runtime: &DocumentRuntime,
    host: &mut M11CandidateHost,
    publication: &M11CandidatePublication,
) -> M11InstalledCandidate {
    let mut encoder = publication
        .snapshot_encoder(runtime)
        .expect("snapshot encoder");
    let begin = encoder.begin_frame().expect("begin frame");
    host.begin_snapshot(&begin).expect("host begin");
    loop {
        match encoder.poll(1).expect("snapshot poll") {
            M11SnapshotPoll::Pending { transitions } => assert_eq!(transitions, 1),
            M11SnapshotPoll::Node { transitions, bytes } => {
                assert_eq!(transitions, 1);
                host.offer_node(&bytes).expect("host node");
            }
            M11SnapshotPoll::End { transitions, bytes } => {
                assert!(transitions <= 1);
                host.finish_snapshot(&bytes).expect("host finish");
                break;
            }
        }
    }
    loop {
        let poll = host.poll_install(1).expect("host install poll");
        assert!(poll.transitions <= 1);
        if let Some(installed) = poll.installed {
            return installed;
        }
    }
}

fn read_record(
    host: &M11CandidateHost,
    installed: M11InstalledCandidate,
    role: M11Role,
    ordinal: u64,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut offset = 0;
    loop {
        let mut chunk = [0_u8; 17];
        let read = host
            .read_role_record(installed, role, ordinal, offset, &mut chunk)
            .expect("bounded role query");
        if read == 0 {
            return bytes;
        }
        bytes.extend_from_slice(&chunk[..read]);
        offset += read;
    }
}

fn assert_installed_records(
    host: &M11CandidateHost,
    installed: M11InstalledCandidate,
    source_facts: &[Vec<u8>],
    green: &[u8],
) {
    assert_eq!(
        host.role_record_count(installed, M11Role::SourceFacts)
            .expect("SourceFacts structure"),
        u64::try_from(source_facts.len()).expect("SourceFacts record count")
    );
    for (ordinal, expected) in source_facts.iter().enumerate() {
        assert_eq!(
            read_record(host, installed, M11Role::SourceFacts, ordinal as u64),
            *expected
        );
    }
    let block = host
        .locate_block_point(
            installed,
            M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
        )
        .expect("installed block query")
        .expect("installed first block");
    assert_eq!(
        block.entry().green().expect("installed Green").as_bytes(),
        green
    );
}

fn close_publication(
    runtime: &mut DocumentRuntime,
    publication: &mut M11CandidatePublication,
) -> usize {
    publication.begin_close(runtime).expect("publication close");
    let mut polls = 0;
    while !publication
        .poll_close(runtime, 1)
        .expect("publication reclaim")
    {
        polls += 1;
    }
    polls
}

fn close_retained(
    runtime: &mut DocumentRuntime,
    publication: &mut M11RetainedCandidatePublication,
) {
    publication
        .begin_close(runtime)
        .expect("retained publication close");
    while !publication
        .poll_close(runtime, 1)
        .expect("retained publication reclaim")
    {}
}

fn close_host(host: &mut M11CandidateHost) -> usize {
    host.begin_close().expect("host close");
    let mut polls = 0;
    while !host.poll_close(1).expect("host reclaim") {
        polls += 1;
    }
    polls
}

#[test]
fn exact_source_to_paged_candidate_to_independent_host_is_atomic_and_queryable() {
    let text: String = (0..600)
        .map(|ordinal| char::from(b'a' + u8::try_from(ordinal % 26).expect("alphabet ordinal")))
        .collect();
    let mut runtime = producer_runtime(&text);
    let source = runtime.current_source_version().expect("source");
    let first_candidate = candidate(&mut runtime, 4);
    let source_fact_records = runtime
        .persistent_source_facts()
        .expect("persistent SourceFacts")
        .page_count();
    assert!(
        source_fact_records > 1,
        "test must force canonical SourceFacts pages"
    );
    let mut first = publish(&mut runtime, first_candidate, [19; 16], 1);
    let (_, first_nodes, _) = collect_frames(&runtime, &first);
    let expected_source_facts: Vec<Vec<u8>> = first_nodes
        .iter()
        .map(|frame| node_payload(frame))
        .filter(|payload| payload.starts_with(b"SFL2"))
        .map(ToOwned::to_owned)
        .collect();
    assert_eq!(
        u64::try_from(expected_source_facts.len()).expect("leaf count"),
        source_fact_records
    );

    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("independent host");
    let installed = install(&runtime, &mut host, &first);
    assert_eq!(installed.parse_generation(), 1);
    let installed_block = host
        .locate_block_point(
            installed,
            M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
        )
        .expect("installed block query")
        .expect("installed first block");
    let expected_green = installed_block
        .entry()
        .green()
        .expect("installed Green")
        .as_bytes()
        .to_vec();
    let expected_projection = installed_block
        .entry()
        .projection()
        .expect("installed Projection")
        .as_bytes()
        .to_vec();
    assert_eq!(expected_green.len(), M11_GREEN_RECORD_BYTES);
    assert_eq!(expected_projection.len(), M11_PROJECTION_RECORD_BYTES);
    assert_installed_records(&host, installed, &expected_source_facts, &expected_green);

    // A same-generation offer is stale before it can disturb the current root.
    let stale_candidate = candidate(&mut runtime, 4);
    let mut stale = publish(&mut runtime, stale_candidate, [20; 16], 1);
    let (stale_begin, _, _) = collect_frames(&runtime, &stale);
    let stale_error = host
        .begin_snapshot(&stale_begin)
        .expect_err("same generation must be stale");
    assert!(stale_error.is_stale_candidate());
    assert_eq!(host.installed(), Some(installed));

    // A repaired transport digest cannot bless mutations to any semantic
    // dimension of the persistent tree. The independent host recomputes every
    // SFL2 leaf and each SFB2 child relation during postorder import.
    let corrupt_candidate = candidate(&mut runtime, 4);
    let mut corrupt = publish(&mut runtime, corrupt_candidate, [21; 16], 2);
    let (begin, pristine_nodes, pristine_end) = collect_frames(&runtime, &corrupt);

    for mutation in [
        PersistentTreeMutation::LeafByte,
        PersistentTreeMutation::ChildOrder,
        PersistentTreeMutation::BranchLeafCount,
        PersistentTreeMutation::BranchProfile,
        PersistentTreeMutation::BranchSegmentMeasure,
        PersistentTreeMutation::BranchCommitment,
    ] {
        let nodes = mutated_persistent_nodes(&pristine_nodes, mutation);
        expect_repaired_semantic_rejection(
            &mut host,
            installed,
            &begin,
            &nodes,
            pristine_end.clone(),
        );
    }

    // Every rejected offer leaves the exact installed root readable.
    assert_installed_records(&host, installed, &expected_source_facts, &expected_green);

    assert!(close_publication(&mut runtime, &mut stale) > 1);
    assert!(close_publication(&mut runtime, &mut corrupt) > 1);
    assert!(close_host(&mut host) > 1);
    assert!(close_publication(&mut runtime, &mut first) > 1);
    close_runtime(runtime);
}

#[test]
fn clean_and_edit_derived_sources_produce_equal_parser_role_records() {
    let text = "persistent paragraph ".repeat(20);
    let mut clean_runtime = producer_runtime(&text);
    let mut edited_runtime = producer_runtime(&(text.clone() + "x"));
    let expected = edited_runtime
        .current_source_version()
        .expect("edit source");
    edited_runtime
        .apply_edit(expected, text.len()..text.len() + 1, "")
        .expect("commit edit");

    let clean = candidate(&mut clean_runtime, 4);
    let edited = candidate(&mut edited_runtime, 4);
    let clean_facts = clean_runtime
        .persistent_source_facts()
        .expect("clean persistent SourceFacts");
    let edited_facts = edited_runtime
        .persistent_source_facts()
        .expect("edited persistent SourceFacts");
    assert_eq!(clean_facts.summary(), edited_facts.summary());
    assert_eq!(clean_facts.page_count(), edited_facts.page_count());
    assert_eq!(
        clean_facts.checkpoint_count(),
        edited_facts.checkpoint_count()
    );
    for role in [
        M11CandidateRoleBytes::Green,
        M11CandidateRoleBytes::Projection,
    ] {
        assert_eq!(
            clean.role_record_count(role),
            edited.role_record_count(role)
        );
        for ordinal in 0..clean.role_record_count(role) {
            assert_eq!(
                clean.role_record(role, ordinal),
                edited.role_record(role, ordinal),
                "semantic role {role:?} record {ordinal} diverged by construction history"
            );
        }
    }
    close_runtime(clean_runtime);
    close_runtime(edited_runtime);
}

#[test]
fn partial_candidate_build_aborts_and_reclaims_with_fuel() {
    let mut runtime = producer_runtime("ordinary paragraph");
    let candidate = candidate(&mut runtime, 4);
    let mut writer: M11ParserCandidateWriter = candidate
        .into_writer(&mut runtime, DOCUMENT, [31; 16], 1)
        .expect("writer");
    assert!(matches!(
        writer.poll(&mut runtime, 1).expect("first writer poll"),
        M11ParserCandidateWriterPoll::Pending { .. }
    ));
    writer.begin_abort(&mut runtime).expect("begin abort");
    let mut polls = 0;
    while !writer.poll_abort(&mut runtime, 1).expect("abort poll") {
        polls += 1;
    }
    assert!(polls > 0);
    close_runtime(runtime);
}

#[test]
fn segmented_candidates_round_trip_typed_block_coverage_without_a_flat_role_fallback() {
    let cases = [
        "p\n\n**q**",
        "é😀\r\n\r\nq",
        "[x]: /target\r\n",
        "safe\n\nnext\n| --- |\n",
    ];
    for (ordinal, text) in cases.into_iter().enumerate() {
        let mut runtime = producer_runtime(text);
        let source = runtime.current_source_version().expect("source");
        let candidate = candidate(&mut runtime, 2);
        assert_eq!(candidate.role_record_count(M11CandidateRoleBytes::Green), 0);
        assert_eq!(
            candidate.role_record_count(M11CandidateRoleBytes::Projection),
            0
        );
        let mut publication = publish(
            &mut runtime,
            candidate,
            [u8::try_from(50 + ordinal).expect("publication byte"); 16],
            1,
        );
        let (_, nodes, _) = collect_frames(&runtime, &publication);
        assert!(
            nodes
                .iter()
                .any(|node| node_payload(node).starts_with(b"BSL1")),
            "{text:?} must retain persistent block coverage",
        );
        let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
        let installed = install(&runtime, &mut host, &publication);
        assert_eq!(installed.parse_generation(), 1);
        close_host(&mut host);
        close_publication(&mut runtime, &mut publication);
        close_runtime(runtime);
    }
}

#[test]
fn recursive_green_role_round_trips_nested_commonmark_to_an_independent_host() {
    const TEXT: &str = "- a\n  > b\n  ```\n  c\n  ```\n- d\n\n* foo\n  * bar\n\n  baz\n";

    let mut runtime = producer_runtime(TEXT);
    let source = runtime.current_source_version().expect("source");
    prepare_runtime_source_facts(&mut runtime, 2);
    let result = parse(
        runtime
            .certified_source()
            .expect("completed certification")
            .exact_parse_lease(),
    );
    let mut green_session = recursive_green_session(&mut runtime);
    let certified = runtime
        .take_certified_source()
        .expect("runtime certification");
    let candidate =
        M11ParserCandidate::derive_with_recursive_green(certified, &result, &green_session)
            .expect("recursive Green candidate");
    let mut writer = candidate
        .into_writer_with_recursive_green(&mut runtime, DOCUMENT, [0x7a; 16], 1, &green_session)
        .expect("recursive Green candidate writer");

    // Create a strictly newer offer over the same source/session. A mutation
    // in this offer must reach semantic node validation instead of stopping at
    // the stale-generation guard.
    prepare_runtime_source_facts(&mut runtime, 2);
    let second_result = parse(
        runtime
            .certified_source()
            .expect("second completed certification")
            .exact_parse_lease(),
    );
    let second_certified = runtime
        .take_certified_source()
        .expect("second runtime certification");
    let second_candidate = M11ParserCandidate::derive_with_recursive_green(
        second_certified,
        &second_result,
        &green_session,
    )
    .expect("second recursive Green candidate");
    let mut second_writer = second_candidate
        .into_writer_with_recursive_green(&mut runtime, DOCUMENT, [0x7b; 16], 2, &green_session)
        .expect("second recursive Green candidate writer");

    // Both candidate journals own independent retained edges before the
    // original parser session is released.
    close_recursive_green_session(&mut runtime, &mut green_session);
    let mut publication = loop {
        match writer.poll(&mut runtime, 1).expect("candidate writer poll") {
            M11ParserCandidateWriterPoll::Pending { transitions } => {
                assert!(transitions <= 1)
            }
            M11ParserCandidateWriterPoll::Published {
                transitions,
                publication,
            } => {
                assert!(transitions <= 1);
                break *publication;
            }
        }
    };
    let mut second_publication = loop {
        match second_writer
            .poll(&mut runtime, 1)
            .expect("second candidate writer poll")
        {
            M11ParserCandidateWriterPoll::Pending { transitions } => {
                assert!(transitions <= 1)
            }
            M11ParserCandidateWriterPoll::Published {
                transitions,
                publication,
            } => {
                assert!(transitions <= 1);
                break *publication;
            }
        }
    };

    let (_, nodes, _) = collect_frames(&runtime, &publication);
    assert!(
        nodes
            .iter()
            .any(|node| node_payload(node).starts_with(b"RGL1")),
        "snapshot must carry the persistent recursive Green tree",
    );

    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("independent host");
    let installed = install(&runtime, &mut host, &publication);
    let point = TEXT.find("> b").expect("nested quote child") + 2;
    let location = host
        .locate_recursive_green_point(
            installed,
            M11RecursiveGreenPoint::new(point, point, SourceBoundaryAffinity::After),
        )
        .expect("recursive Green host query")
        .expect("nested Paragraph location");
    assert_eq!(location.part(), M11RecursiveGreenCoveragePart::Content);
    assert!(location.byte_range().contains(&(point as u64)));
    assert_eq!(location.owner().kind().get(), 5);
    assert_eq!(
        location
            .ancestry()
            .iter()
            .map(|ancestor| ancestor.kind().get())
            .collect::<Vec<_>>(),
        vec![1, 3, 4, 2, 5],
    );
    assert!(location.receipt().storage_pages_visited() > 0);
    assert!(location.receipt().events_scanned() > 0);

    let (second_begin, second_nodes, _) = collect_frames(&runtime, &second_publication);
    let mut corrupt_nodes = second_nodes.clone();
    let leaf = corrupt_nodes
        .iter_mut()
        .find(|node| node_payload(node).starts_with(b"RGL1"))
        .expect("recursive Green leaf");
    *leaf.last_mut().expect("Green event byte") ^= 0x01;
    expect_semantic_node_rejection(&mut host, &second_begin, &corrupt_nodes);
    assert_eq!(host.installed(), Some(installed));

    close_host(&mut host);
    close_publication(&mut runtime, &mut second_publication);
    close_publication(&mut runtime, &mut publication);
    close_runtime(runtime);
}

#[test]
fn segmented_block_queries_preserve_exact_ranges_kinds_and_leaf_relative_paragraph_records() {
    let text = "p\n\n**q**";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");
    let candidate = candidate(&mut runtime, 2);
    let mut publication = publish(&mut runtime, candidate, [60; 16], 1);
    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);

    let first = host
        .locate_block_point(
            installed,
            M11BlockSequencePoint::new(2, 2, SourceBoundaryAffinity::Before),
        )
        .expect("first Paragraph query")
        .expect("first Paragraph");
    assert_eq!(first.entry_ordinal(), 0);
    assert_eq!(first.byte_range(), 0..2);
    assert_eq!(first.utf16_range(), 0..2);
    assert_eq!(first.entry().kind(), M11BlockSequenceEntryKind::Paragraph);

    let blank = host
        .locate_block_point(
            installed,
            M11BlockSequencePoint::new(3, 3, SourceBoundaryAffinity::Before),
        )
        .expect("blank query")
        .expect("blank");
    assert_eq!(blank.entry_ordinal(), 1);
    assert_eq!(blank.byte_range(), 2..3);
    assert_eq!(blank.utf16_range(), 2..3);
    assert_eq!(blank.entry().kind(), M11BlockSequenceEntryKind::Blank);

    let second = host
        .locate_block_point(
            installed,
            M11BlockSequencePoint::new(3, 3, SourceBoundaryAffinity::After),
        )
        .expect("second Paragraph query")
        .expect("second Paragraph");
    assert_eq!(second.entry_ordinal(), 2);
    assert_eq!(second.byte_range(), 3..8);
    assert_eq!(second.utf16_range(), 3..8);
    assert_eq!(second.entry().kind(), M11BlockSequenceEntryKind::Paragraph);
    assert_eq!(second.entry().reference_definition_count(), 0);

    let green = second.entry().green().expect("Paragraph Green").as_bytes();
    assert_eq!(green.len(), M11_GREEN_RECORD_BYTES);
    assert_eq!(&green[..8], b"FLKGR001");
    assert_eq!(green[12], 1);
    assert_eq!(
        u64::from_le_bytes(green[16..24].try_into().expect("Green source start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[24..32].try_into().expect("Green source end")),
        5
    );
    assert_eq!(
        u64::from_le_bytes(green[32..40].try_into().expect("Green inline start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[40..48].try_into().expect("Green inline end")),
        5
    );

    let projection = second
        .entry()
        .projection()
        .expect("Paragraph Projection")
        .as_bytes();
    assert_eq!(projection.len(), M11_PROJECTION_RECORD_BYTES);
    assert_eq!(&projection[..8], b"FLKPR001");
    assert_eq!(projection[12], 1);
    assert_eq!(
        u64::from_le_bytes(
            projection[16..24]
                .try_into()
                .expect("Projection source start")
        ),
        0
    );
    assert_eq!(
        u64::from_le_bytes(
            projection[24..32]
                .try_into()
                .expect("Projection source end")
        ),
        5
    );
    assert_eq!(
        u64::from_le_bytes(
            projection[32..40]
                .try_into()
                .expect("Projection projected start")
        ),
        0
    );
    assert_eq!(
        u64::from_le_bytes(
            projection[40..48]
                .try_into()
                .expect("Projection projected end")
        ),
        5
    );

    close_host(&mut host);
    close_publication(&mut runtime, &mut publication);
    close_runtime(runtime);
}

#[test]
fn fenced_code_publishes_one_structured_block_with_exact_body_projection() {
    let text = "p\n\n```dart\né\n```\n";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");
    let candidate = candidate(&mut runtime, 2);
    let mut publication = publish(&mut runtime, candidate, [61; 16], 1);
    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);

    let fence = host
        .locate_block_point(
            installed,
            M11BlockSequencePoint::new(11, 11, SourceBoundaryAffinity::After),
        )
        .expect("fenced-code query")
        .expect("fenced-code block");
    assert_eq!(fence.entry_ordinal(), 2);
    assert_eq!(fence.byte_range(), 3..18);
    assert_eq!(fence.utf16_range(), 3..17);
    assert_eq!(fence.entry().kind(), M11BlockSequenceEntryKind::Structured);
    assert_eq!(fence.entry().reference_definition_count(), 0);
    assert!(fence.entry().unsupported_reason().is_none());

    let green = fence.entry().green().expect("fenced-code Green").as_bytes();
    assert_eq!(green.len(), M11_GREEN_RECORD_BYTES);
    assert_eq!(&green[..8], b"FLKGR001");
    assert_eq!(green[12], 3);
    assert_eq!(
        u64::from_le_bytes(green[16..24].try_into().expect("source start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[24..32].try_into().expect("source end")),
        15
    );
    assert_eq!(
        u64::from_le_bytes(green[32..40].try_into().expect("body start")),
        8
    );
    assert_eq!(
        u64::from_le_bytes(green[40..48].try_into().expect("body end")),
        11
    );
    assert_eq!(
        u64::from_le_bytes(green[48..56].try_into().expect("fence metadata")),
        0x1_0060
    );
    assert_eq!(
        u32::from_le_bytes(green[56..60].try_into().expect("opener start")),
        0
    );
    assert_eq!(
        u32::from_le_bytes(green[60..64].try_into().expect("opener end")),
        3
    );
    assert_eq!(
        u32::from_le_bytes(green[64..68].try_into().expect("info start")),
        3
    );
    assert_eq!(
        u32::from_le_bytes(green[68..72].try_into().expect("info end")),
        7
    );
    assert_eq!(
        u32::from_le_bytes(green[72..76].try_into().expect("closer start")),
        11
    );
    assert_eq!(
        u32::from_le_bytes(green[76..80].try_into().expect("closer end")),
        14
    );

    let projection = fence
        .entry()
        .projection()
        .expect("fenced-code Projection")
        .as_bytes();
    assert_eq!(projection.len(), M11_PROJECTION_RECORD_BYTES);
    assert_eq!(&projection[..8], b"FLKPR001");
    assert_eq!(projection[12], 3);
    assert_eq!(
        u64::from_le_bytes(projection[32..40].try_into().expect("body start")),
        8
    );
    assert_eq!(
        u64::from_le_bytes(projection[40..48].try_into().expect("body end")),
        11
    );
    assert_eq!(
        u64::from_le_bytes(projection[48..56].try_into().expect("run count")),
        1
    );

    close_host(&mut host);
    close_publication(&mut runtime, &mut publication);
    close_runtime(runtime);
}

#[test]
fn atx_heading_publishes_one_structured_block_with_exact_inline_projection() {
    let text = "p\n\n  ### **β😀** ###  \r\n";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");
    let candidate = candidate(&mut runtime, 2);
    let mut publication = publish(&mut runtime, candidate, [62; 16], 1);
    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);

    let heading = host
        .locate_block_point(
            installed,
            M11BlockSequencePoint::new(11, 11, SourceBoundaryAffinity::After),
        )
        .expect("ATX Heading query")
        .expect("ATX Heading block");
    assert_eq!(heading.entry_ordinal(), 2);
    assert_eq!(heading.byte_range(), 3..27);
    assert_eq!(heading.utf16_range(), 3..24);
    assert_eq!(
        heading.entry().kind(),
        M11BlockSequenceEntryKind::Structured
    );
    assert_eq!(heading.entry().reference_definition_count(), 0);
    assert!(heading.entry().unsupported_reason().is_none());

    let green = heading
        .entry()
        .green()
        .expect("ATX Heading Green")
        .as_bytes();
    assert_eq!(green.len(), M11_GREEN_RECORD_BYTES);
    assert_eq!(&green[..8], b"FLKGR001");
    assert_eq!(green[12], 4);
    assert_eq!(
        u64::from_le_bytes(green[16..24].try_into().expect("source start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[24..32].try_into().expect("source end")),
        24
    );
    assert_eq!(
        u64::from_le_bytes(green[32..40].try_into().expect("content start")),
        6
    );
    assert_eq!(
        u64::from_le_bytes(green[40..48].try_into().expect("content end")),
        16
    );
    assert_eq!(
        u64::from_le_bytes(green[48..56].try_into().expect("ATX metadata")),
        0x503
    );
    assert_eq!(
        u32::from_le_bytes(green[56..60].try_into().expect("opener start")),
        2
    );
    assert_eq!(
        u32::from_le_bytes(green[60..64].try_into().expect("opener end")),
        5
    );
    assert_eq!(
        u32::from_le_bytes(green[64..68].try_into().expect("closer start")),
        17
    );
    assert_eq!(
        u32::from_le_bytes(green[68..72].try_into().expect("closer end")),
        20
    );
    assert_eq!(
        u32::from_le_bytes(green[72..76].try_into().expect("EOL start")),
        22
    );
    assert_eq!(
        u32::from_le_bytes(green[76..80].try_into().expect("EOL end")),
        24
    );

    let projection = heading
        .entry()
        .projection()
        .expect("ATX Heading Projection")
        .as_bytes();
    assert_eq!(projection.len(), M11_PROJECTION_RECORD_BYTES);
    assert_eq!(&projection[..8], b"FLKPR001");
    assert_eq!(projection[12], 4);
    assert_eq!(
        u64::from_le_bytes(projection[32..40].try_into().expect("content start")),
        6
    );
    assert_eq!(
        u64::from_le_bytes(projection[40..48].try_into().expect("content end")),
        16
    );
    assert_eq!(
        u64::from_le_bytes(projection[48..56].try_into().expect("run count")),
        1
    );

    close_host(&mut host);
    close_publication(&mut runtime, &mut publication);
    close_runtime(runtime);
}

#[test]
fn reference_bearing_setext_heading_publishes_exact_geometry_and_projection() {
    let text = "p\n\n[x]: /url\nβ😀\r\n  ---  \r\n";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");
    let candidate = candidate(&mut runtime, 2);
    let mut publication = publish(&mut runtime, candidate, [63; 16], 1);
    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);

    let heading = host
        .locate_block_point(
            installed,
            M11BlockSequencePoint::new(13, 13, SourceBoundaryAffinity::After),
        )
        .expect("Setext Heading query")
        .expect("Setext Heading block");
    assert_eq!(heading.entry_ordinal(), 2);
    assert_eq!(heading.byte_range(), 3..30);
    assert_eq!(heading.utf16_range(), 3..27);
    assert_eq!(
        heading.entry().kind(),
        M11BlockSequenceEntryKind::Structured
    );
    assert_eq!(heading.entry().reference_definition_count(), 1);
    assert!(heading.entry().unsupported_reason().is_none());

    let green = heading
        .entry()
        .green()
        .expect("Setext Heading Green")
        .as_bytes();
    assert_eq!(green.len(), M11_GREEN_RECORD_BYTES);
    assert_eq!(&green[..8], b"FLKGR001");
    assert_eq!(green[12], 5);
    assert_eq!(
        u64::from_le_bytes(green[16..24].try_into().expect("source start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[24..32].try_into().expect("source end")),
        27
    );
    assert_eq!(
        u64::from_le_bytes(green[32..40].try_into().expect("content start")),
        10
    );
    assert_eq!(
        u64::from_le_bytes(green[40..48].try_into().expect("content end")),
        16
    );
    assert_eq!(
        u64::from_le_bytes(green[48..56].try_into().expect("Setext metadata")),
        0x202
    );
    assert_eq!(
        u32::from_le_bytes(green[56..60].try_into().expect("underline start")),
        20
    );
    assert_eq!(
        u32::from_le_bytes(green[60..64].try_into().expect("underline end")),
        23
    );
    assert_eq!(
        u32::from_le_bytes(green[64..68].try_into().expect("EOL start")),
        25
    );
    assert_eq!(
        u32::from_le_bytes(green[68..72].try_into().expect("EOL end")),
        27
    );
    assert_eq!(
        u64::from_le_bytes(green[72..80].try_into().expect("reference count")),
        1
    );

    let projection = heading
        .entry()
        .projection()
        .expect("Setext Heading Projection")
        .as_bytes();
    assert_eq!(projection.len(), M11_PROJECTION_RECORD_BYTES);
    assert_eq!(&projection[..8], b"FLKPR001");
    assert_eq!(projection[12], 5);
    assert_eq!(
        u64::from_le_bytes(projection[32..40].try_into().expect("content start")),
        10
    );
    assert_eq!(
        u64::from_le_bytes(projection[40..48].try_into().expect("content end")),
        16
    );
    assert_eq!(
        u64::from_le_bytes(projection[48..56].try_into().expect("run count")),
        1
    );

    close_host(&mut host);
    close_publication(&mut runtime, &mut publication);
    close_runtime(runtime);
}

#[test]
fn thematic_break_publishes_one_structured_zero_text_projection() {
    let text = "\u{feff} - - -\r";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");
    let candidate = candidate(&mut runtime, 2);
    let mut publication = publish(&mut runtime, candidate, [64; 16], 1);
    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);

    let thematic = host
        .locate_block_point(
            installed,
            M11BlockSequencePoint::new(4, 2, SourceBoundaryAffinity::After),
        )
        .expect("Thematic Break query")
        .expect("Thematic Break block");
    assert_eq!(thematic.entry_ordinal(), 0);
    assert_eq!(thematic.byte_range(), 0..10);
    assert_eq!(thematic.utf16_range(), 0..8);
    assert_eq!(
        thematic.entry().kind(),
        M11BlockSequenceEntryKind::Structured
    );
    assert_eq!(thematic.entry().reference_definition_count(), 0);
    assert!(thematic.entry().unsupported_reason().is_none());

    let green = thematic
        .entry()
        .green()
        .expect("Thematic Break Green")
        .as_bytes();
    assert_eq!(green.len(), M11_GREEN_RECORD_BYTES);
    assert_eq!(&green[..8], b"FLKGR001");
    assert_eq!(green[12], 6);
    assert_eq!(
        u64::from_le_bytes(green[16..24].try_into().expect("source start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[24..32].try_into().expect("source end")),
        10
    );
    assert_eq!(
        u64::from_le_bytes(green[32..40].try_into().expect("visible start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[40..48].try_into().expect("visible end")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[48..56].try_into().expect("thematic metadata")),
        0x52d
    );
    assert_eq!(
        u32::from_le_bytes(green[56..60].try_into().expect("envelope start")),
        4
    );
    assert_eq!(
        u32::from_le_bytes(green[60..64].try_into().expect("envelope end")),
        9
    );
    assert_eq!(
        u32::from_le_bytes(green[64..68].try_into().expect("EOL start")),
        9
    );
    assert_eq!(
        u32::from_le_bytes(green[68..72].try_into().expect("EOL end")),
        10
    );
    assert_eq!(
        u64::from_le_bytes(green[72..80].try_into().expect("marker count")),
        3
    );

    let projection = thematic
        .entry()
        .projection()
        .expect("Thematic Break Projection")
        .as_bytes();
    assert_eq!(projection.len(), M11_PROJECTION_RECORD_BYTES);
    assert_eq!(&projection[..8], b"FLKPR001");
    assert_eq!(projection[12], 6);
    assert_eq!(
        u64::from_le_bytes(projection[16..24].try_into().expect("source start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(projection[24..32].try_into().expect("source end")),
        10
    );
    assert_eq!(
        u64::from_le_bytes(projection[32..40].try_into().expect("projected start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(projection[40..48].try_into().expect("projected end")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(projection[48..56].try_into().expect("run count")),
        0
    );

    close_host(&mut host);
    close_publication(&mut runtime, &mut publication);
    close_runtime(runtime);
}

#[test]
fn indented_code_publishes_variant_seven_source_backed_projection_summary() {
    let text = "\u{feff}\tα\r\n    \tβ\r      γ\0";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");
    let candidate = candidate(&mut runtime, 2);
    let mut publication = publish(&mut runtime, candidate, [65; 16], 1);
    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);

    let code = host
        .locate_block_point(
            installed,
            M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
        )
        .expect("Indented Code query")
        .expect("Indented Code block");
    assert_eq!(code.entry_ordinal(), 0);
    assert_eq!(code.byte_range(), 0..25);
    assert_eq!(code.utf16_range(), 0..20);
    assert_eq!(code.entry().kind(), M11BlockSequenceEntryKind::Structured);
    assert_eq!(code.entry().reference_definition_count(), 0);
    assert!(code.entry().unsupported_reason().is_none());

    let green = code
        .entry()
        .green()
        .expect("Indented Code Green")
        .as_bytes();
    assert_eq!(green.len(), M11_GREEN_RECORD_BYTES);
    assert_eq!(&green[..8], b"FLKGR001");
    assert_eq!(green[12], 7);
    assert_eq!(
        u64::from_le_bytes(green[16..24].try_into().expect("source start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[24..32].try_into().expect("source end")),
        25
    );
    assert_eq!(
        u64::from_le_bytes(green[32..40].try_into().expect("visible start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[40..48].try_into().expect("visible end")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[48..56].try_into().expect("metadata")),
        0x104
    );
    assert_eq!(
        u32::from_le_bytes(green[56..60].try_into().expect("line count")),
        3
    );
    assert_eq!(
        u32::from_le_bytes(green[60..64].try_into().expect("projected UTF-8")),
        13
    );
    assert_eq!(
        u32::from_le_bytes(green[64..68].try_into().expect("projected UTF-16")),
        10
    );
    assert_eq!(
        u32::from_le_bytes(green[68..72].try_into().expect("terminal EOL")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[72..80].try_into().expect("reserved")),
        0
    );

    let projection = code
        .entry()
        .projection()
        .expect("Indented Code Projection")
        .as_bytes();
    assert_eq!(projection.len(), M11_PROJECTION_RECORD_BYTES);
    assert_eq!(&projection[..8], b"FLKPR001");
    assert_eq!(projection[12], 7);
    assert_eq!(
        u64::from_le_bytes(projection[16..24].try_into().expect("source start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(projection[24..32].try_into().expect("source end")),
        25
    );
    assert_eq!(
        u64::from_le_bytes(projection[32..40].try_into().expect("projected start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(projection[40..48].try_into().expect("projected end")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(projection[48..56].try_into().expect("run count")),
        3
    );

    close_host(&mut host);
    close_publication(&mut runtime, &mut publication);
    close_runtime(runtime);
}

#[test]
fn exact_block_quote_publishes_variant_eight_noncontiguous_path_summary() {
    let text = "   > alpha\n> beta\nlazy\n";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");
    let candidate = candidate(&mut runtime, 2);
    let mut publication = publish(&mut runtime, candidate, [66; 16], 1);
    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);

    let quote = host
        .locate_block_point(
            installed,
            M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
        )
        .expect("Block Quote query")
        .expect("Block Quote block");
    assert_eq!(quote.entry_ordinal(), 0);
    assert_eq!(quote.byte_range(), 0..23);
    assert_eq!(quote.utf16_range(), 0..23);
    assert_eq!(quote.entry().kind(), M11BlockSequenceEntryKind::Structured);
    assert_eq!(quote.entry().reference_definition_count(), 0);
    assert!(quote.entry().unsupported_reason().is_none());

    let green = quote.entry().green().expect("Block Quote Green").as_bytes();
    assert_eq!(green.len(), M11_GREEN_RECORD_BYTES);
    assert_eq!(&green[..8], b"FLKGR001");
    assert_eq!(green[12], 8);
    assert_eq!(
        u64::from_le_bytes(green[16..24].try_into().expect("source start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[24..32].try_into().expect("source end")),
        23
    );
    assert_eq!(
        u64::from_le_bytes(green[32..40].try_into().expect("visible start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[40..48].try_into().expect("visible end")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[48..56].try_into().expect("disposition")),
        1
    );
    assert_eq!(
        u32::from_le_bytes(green[56..60].try_into().expect("line count")),
        3
    );
    assert_eq!(
        u32::from_le_bytes(green[60..64].try_into().expect("child first line")),
        0
    );
    assert_eq!(
        u32::from_le_bytes(green[64..68].try_into().expect("child line count")),
        3
    );
    assert_eq!(
        u32::from_le_bytes(green[68..72].try_into().expect("projected UTF-8")),
        16
    );
    assert_eq!(
        u32::from_le_bytes(green[72..76].try_into().expect("projected UTF-16")),
        16
    );
    assert_eq!(
        u32::from_le_bytes(green[76..80].try_into().expect("reserved")),
        0
    );

    let projection = quote
        .entry()
        .projection()
        .expect("Block Quote Projection")
        .as_bytes();
    assert_eq!(projection.len(), M11_PROJECTION_RECORD_BYTES);
    assert_eq!(&projection[..8], b"FLKPR001");
    assert_eq!(projection[12], 8);
    assert_eq!(
        u64::from_le_bytes(projection[16..24].try_into().expect("source start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(projection[24..32].try_into().expect("source end")),
        23
    );
    assert_eq!(
        u64::from_le_bytes(projection[32..40].try_into().expect("projected start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(projection[40..48].try_into().expect("projected end")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(projection[48..56].try_into().expect("run count")),
        3
    );

    close_host(&mut host);
    close_publication(&mut runtime, &mut publication);
    close_runtime(runtime);
}

#[test]
fn exact_bullet_list_publishes_variant_nine_item_path_summary() {
    let text = "\u{feff}- α😀\r\n- beta\n- ";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");
    let candidate = candidate(&mut runtime, 2);
    let mut publication = publish(&mut runtime, candidate, [69; 16], 1);
    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);

    let list = host
        .locate_block_point(
            installed,
            M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
        )
        .expect("Bullet List query")
        .expect("Bullet List block");
    assert_eq!(list.entry_ordinal(), 0);
    assert_eq!(list.byte_range(), 0..22);
    assert_eq!(list.utf16_range(), 0..17);
    assert_eq!(list.entry().kind(), M11BlockSequenceEntryKind::Structured);
    assert_eq!(list.entry().reference_definition_count(), 0);
    assert!(list.entry().unsupported_reason().is_none());

    let green = list.entry().green().expect("Bullet List Green").as_bytes();
    assert_eq!(green.len(), M11_GREEN_RECORD_BYTES);
    assert_eq!(&green[..8], b"FLKGR001");
    assert_eq!(&green[8..12], &1_u32.to_le_bytes());
    assert_eq!(green[12], 9);
    assert_eq!(&green[13..16], &[0; 3]);
    assert_eq!(
        u64::from_le_bytes(green[16..24].try_into().expect("source start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[24..32].try_into().expect("source end")),
        22
    );
    assert_eq!(
        u64::from_le_bytes(green[32..40].try_into().expect("visible start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[40..48].try_into().expect("visible end")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(green[48..56].try_into().expect("metadata")),
        0x12d01
    );
    assert_eq!(
        u32::from_le_bytes(green[56..60].try_into().expect("item count")),
        3
    );
    assert_eq!(
        u32::from_le_bytes(
            green[60..64]
                .try_into()
                .expect("terminal empty relative start")
        ),
        20
    );
    assert_eq!(
        u32::from_le_bytes(green[64..68].try_into().expect("paragraph count")),
        2
    );
    assert_eq!(
        u32::from_le_bytes(green[68..72].try_into().expect("projected UTF-8")),
        13
    );
    assert_eq!(
        u32::from_le_bytes(green[72..76].try_into().expect("projected UTF-16")),
        10
    );
    assert_eq!(
        u32::from_le_bytes(green[76..80].try_into().expect("reserved")),
        0
    );

    let projection = list
        .entry()
        .projection()
        .expect("Bullet List Projection")
        .as_bytes();
    assert_eq!(projection.len(), M11_PROJECTION_RECORD_BYTES);
    assert_eq!(&projection[..8], b"FLKPR001");
    assert_eq!(&projection[8..12], &1_u32.to_le_bytes());
    assert_eq!(projection[12], 9);
    assert_eq!(&projection[13..16], &[0; 3]);
    assert_eq!(
        u64::from_le_bytes(projection[16..24].try_into().expect("source start")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(projection[24..32].try_into().expect("source end")),
        22
    );
    assert_eq!(
        u64::from_le_bytes(
            projection[32..40]
                .try_into()
                .expect("projected source start")
        ),
        0
    );
    assert_eq!(
        u64::from_le_bytes(projection[40..48].try_into().expect("projected source end")),
        0
    );
    assert_eq!(
        u64::from_le_bytes(projection[48..56].try_into().expect("item count")),
        3
    );

    close_host(&mut host);
    close_publication(&mut runtime, &mut publication);
    close_runtime(runtime);
}

#[test]
fn retained_variant_nine_mints_exact_move_only_bullet_list_authority() {
    let text = "p\n\n- α😀\r\n- beta\n- ";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");
    let list_start = text.find("- α").expect("list start");
    let list_utf16_start = text[..list_start].encode_utf16().count();
    let candidate = candidate(&mut runtime, 2);
    let mut publication = retain(&mut runtime, candidate, [70; 16], 1);

    let not_list = resolve_m11_published_bullet_list_leaf_fence(
        &runtime,
        &publication,
        M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
    )
    .expect_err("Paragraph must not mint Bullet List authority");
    assert!(matches!(
        not_list,
        M11CandidateDerivationError::PublishedBulletListLeafFenceNotBulletList
    ));

    let fence = resolve_m11_published_bullet_list_leaf_fence(
        &runtime,
        &publication,
        M11BlockSequencePoint::new(
            list_start + 2,
            list_utf16_start + 2,
            SourceBoundaryAffinity::After,
        ),
    )
    .expect("published Bullet List fence");
    assert_eq!(fence.source(), source);
    assert_eq!(
        fence.block_source_range(),
        u32::try_from(list_start).expect("short fixture")
            ..u32::try_from(text.len()).expect("short fixture")
    );
    assert_eq!(
        fence.block_source_utf16_range(),
        u32::try_from(list_utf16_start).expect("short fixture")
            ..u32::try_from(text.encode_utf16().count()).expect("short fixture")
    );
    assert_eq!(fence.entry_ordinal(), 2);
    assert_eq!(
        fence.binding().syntax_profile(),
        ParserProfileId::new(1).expect("parser profile")
    );
    assert_eq!(fence.item_count(), 3);
    assert_eq!(fence.paragraph_count(), 2);
    assert_eq!(fence.marker(), b'-');
    assert_eq!(fence.terminal_empty_relative_start(), Some(17));
    assert_eq!(fence.projected_utf8_length(), 13);
    assert_eq!(fence.projected_utf16_length(), 10);
    assert!(fence.query_receipt().entries_authenticated() > 0);

    drop(fence);
    close_retained(&mut runtime, &mut publication);
    drop(publication);
    close_runtime(runtime);
}

#[test]
fn retained_variant_eight_mints_exact_move_only_block_quote_authority() {
    let text = "p\n\n   > α😀\r\n> beta\nlazy\n";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");
    let quote_start = text.find("   >").expect("quote start");
    let quote_utf16_start = text[..quote_start].encode_utf16().count();
    let candidate = candidate(&mut runtime, 2);
    let mut publication = retain(&mut runtime, candidate, [68; 16], 1);

    let not_quote = resolve_m11_published_block_quote_leaf_fence(
        &runtime,
        &publication,
        M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
    )
    .expect_err("Paragraph must not mint Block Quote authority");
    assert!(matches!(
        not_quote,
        M11CandidateDerivationError::PublishedBlockQuoteLeafFenceNotBlockQuote
    ));

    let fence = resolve_m11_published_block_quote_leaf_fence(
        &runtime,
        &publication,
        M11BlockSequencePoint::new(
            quote_start + 5,
            quote_utf16_start + 5,
            SourceBoundaryAffinity::After,
        ),
    )
    .expect("published Block Quote fence");
    assert_eq!(fence.source(), source);
    assert_eq!(
        fence.block_source_range(),
        u32::try_from(quote_start).expect("short fixture")
            ..u32::try_from(text.len()).expect("short fixture")
    );
    assert_eq!(
        fence.block_source_utf16_range(),
        u32::try_from(quote_utf16_start).expect("short fixture")
            ..u32::try_from(text.encode_utf16().count()).expect("short fixture")
    );
    assert_eq!(fence.entry_ordinal(), 2);
    assert_eq!(
        fence.binding().syntax_profile(),
        ParserProfileId::new(1).expect("parser profile")
    );
    assert_eq!(fence.line_count(), 3);
    assert_eq!(fence.projected_utf8_length(), 18);
    assert_eq!(fence.projected_utf16_length(), 15);
    assert!(fence.query_receipt().entries_authenticated() > 0);

    drop(fence);
    close_retained(&mut runtime, &mut publication);
    drop(publication);
    close_runtime(runtime);
}

#[test]
fn unsupported_block_quote_shape_stays_literal_in_publication() {
    let text = "> # heading\n";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");
    let candidate = candidate(&mut runtime, 2);
    let mut publication = publish(&mut runtime, candidate, [67; 16], 1);
    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);

    let quote = host
        .locate_block_point(
            installed,
            M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
        )
        .expect("Block Quote query")
        .expect("Block Quote block");
    assert_eq!(quote.byte_range(), 0..12);
    assert_eq!(quote.entry().kind(), M11BlockSequenceEntryKind::Unsupported);
    assert_eq!(
        quote
            .entry()
            .unsupported_reason()
            .expect("typed quote reason")
            .get(),
        0x0003_0004
    );
    assert!(quote.entry().green().is_none());
    assert!(quote.entry().projection().is_none());

    close_host(&mut host);
    close_publication(&mut runtime, &mut publication);
    close_runtime(runtime);
}

#[test]
fn cold_recursive_green_publication_retains_the_session_reference_journal() {
    const DEFINITIONS: usize = 4_096;
    const MAXIMUM_CONSTANT_PUBLICATION_NODES: usize = 64;

    let mut text = String::new();
    text.reserve(DEFINITIONS * 28);
    for ordinal in 0..DEFINITIONS {
        use std::fmt::Write as _;
        writeln!(&mut text, "[label-{ordinal}]: /u/{ordinal}").expect("reference fixture write");
    }
    text.push_str("[early][label-0] [middle][label-2048] [last][label-4095] visible tail\n");

    let mut runtime = producer_runtime(&text);
    let source = runtime.current_source_version().expect("source");
    prepare_runtime_source_facts(&mut runtime, 4_096);
    let result = parse(
        runtime
            .certified_source()
            .expect("completed certification")
            .exact_parse_lease(),
    );
    let mut green_session = recursive_green_session(&mut runtime);
    assert_eq!(
        green_session.reference_occurrence_count(),
        DEFINITIONS as u64
    );
    let session_nodes = runtime.arena_metrics().resident_nodes;
    let certified = runtime
        .take_certified_source()
        .expect("runtime certification");
    let candidate =
        M11ParserCandidate::derive_with_recursive_green(certified, &result, &green_session)
            .expect("recursive Green candidate");
    let mut writer = candidate
        .into_writer_with_recursive_green(&mut runtime, DOCUMENT, [0x7c; 16], 1, &green_session)
        .expect("recursive Green candidate writer");
    assert!(
        runtime.arena_metrics().resident_nodes
            <= session_nodes + MAXIMUM_CONSTANT_PUBLICATION_NODES,
        "writer setup must retain the session roots without a definition-sized copy",
    );

    // The writer owns retained Green and References edges before the parser
    // session is released. Publication must therefore need no source recook.
    close_recursive_green_session(&mut runtime, &mut green_session);
    let publication = loop {
        match writer
            .poll(&mut runtime, 64)
            .expect("candidate writer poll")
        {
            M11ParserCandidateWriterPoll::Pending { transitions } => {
                assert!((1..=64).contains(&transitions));
            }
            M11ParserCandidateWriterPoll::Published {
                transitions,
                publication,
            } => {
                assert!(transitions <= 64);
                break publication;
            }
        }
    };
    assert_eq!(
        writer.reference_cook_receipt(),
        M11ReferenceCookReceipt::default(),
        "cold recursive Green publication must not cook a duplicate reference tree",
    );
    assert!(
        runtime.arena_metrics().resident_nodes
            <= session_nodes + MAXIMUM_CONSTANT_PUBLICATION_NODES,
        "published wrappers must add only constant arena residency",
    );
    drop(writer);

    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("independent host");
    let installed = install(&runtime, &mut host, &publication);
    assert_eq!(installed.parse_generation(), 1);

    let mut retained = retain_publication(&mut runtime, publication);
    loop {
        let poll = retained
            .poll_reference_resolver(&mut runtime, 64)
            .expect("poll reference resolver");
        assert!(poll.transitions() <= 64);
        if poll.ready() {
            assert_eq!(poll.occurrence_count(), DEFINITIONS as u64);
            assert_eq!(poll.indexed_occurrences(), DEFINITIONS as u64);
            assert_eq!(poll.unique_label_count(), DEFINITIONS as u64);
            break;
        }
        assert!(poll.transitions() > 0);
    }
    let resolver = retained
        .reference_resolver(&runtime)
        .expect("reference resolver authority")
        .expect("ready reference resolver");
    for ordinal in [0, DEFINITIONS / 2, DEFINITIONS - 1] {
        let label = format!("label-{ordinal}");
        let destination = format!("/u/{ordinal}");
        let M11ReferenceResolution::Resolved(resolved) = resolver
            .resolve(&runtime, &label, 64)
            .expect("resolve retained reference")
        else {
            panic!("{label} must resolve from the retained session journal");
        };
        assert_eq!(resolved.definition_ordinal(), ordinal as u64);
        assert_eq!(resolved.cooked_destination(), destination);
        assert_eq!(resolved.cooked_title(), None);
        let source_range = resolved.destination_source();
        assert_eq!(
            &text.as_bytes()[source_range.start as usize..source_range.end as usize],
            destination.as_bytes(),
        );
    }
    drop(resolver);

    close_host(&mut host);
    close_retained(&mut runtime, &mut retained);
    close_runtime(runtime);
}

#[test]
fn segmented_block_queries_preserve_unicode_definitions_and_unsupported_suffixes() {
    let cases = [
        (
            "é😀\r\n\r\nq",
            M11BlockSequencePoint::new(10, 7, SourceBoundaryAffinity::After),
            M11BlockSequenceEntryKind::Paragraph,
            10..11,
            7..8,
            0,
            None,
        ),
        (
            "[x]: /target\r\n",
            M11BlockSequencePoint::new(0, 0, SourceBoundaryAffinity::After),
            M11BlockSequenceEntryKind::DefinitionsOnly,
            0..14,
            0..14,
            1,
            None,
        ),
        (
            "safe\n\nnext\n| --- |\n",
            M11BlockSequencePoint::new(6, 6, SourceBoundaryAffinity::After),
            M11BlockSequenceEntryKind::Unsupported,
            6..19,
            6..19,
            0,
            Some(0x0002_0009),
        ),
    ];

    for (ordinal, (text, point, kind, byte_range, utf16_range, definitions, reason)) in
        cases.into_iter().enumerate()
    {
        let mut runtime = producer_runtime(text);
        let source = runtime.current_source_version().expect("source");
        let candidate = candidate(&mut runtime, 2);
        let mut publication = publish(
            &mut runtime,
            candidate,
            [u8::try_from(70 + ordinal).expect("publication byte"); 16],
            1,
        );
        let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
        let installed = install(&runtime, &mut host, &publication);
        let location = host
            .locate_block_point(installed, point)
            .expect("block query")
            .expect("covered point");
        assert_eq!(location.entry().kind(), kind, "{text:?}");
        assert_eq!(location.byte_range(), byte_range, "{text:?}");
        assert_eq!(location.utf16_range(), utf16_range, "{text:?}");
        assert_eq!(
            location.entry().reference_definition_count(),
            definitions,
            "{text:?}"
        );
        assert_eq!(
            location
                .entry()
                .unsupported_reason()
                .map(|reason| reason.get()),
            reason,
            "{text:?}"
        );
        close_host(&mut host);
        close_publication(&mut runtime, &mut publication);
        close_runtime(runtime);
    }
}

#[test]
fn segmented_candidate_has_no_old_128_leaf_ceiling() {
    let mut text = String::new();
    for ordinal in 0..130 {
        if ordinal != 0 {
            text.push('\n');
        }
        text.push_str(&format!("paragraph-{ordinal}\n"));
    }
    let mut runtime = producer_runtime(&text);
    let source = runtime.current_source_version().expect("source");
    let candidate = candidate(&mut runtime, 3);
    let mut publication = publish(&mut runtime, candidate, [61; 16], 1);
    let (_, nodes, _) = collect_frames(&runtime, &publication);
    assert!(
        nodes
            .iter()
            .filter(|node| node_payload(node).starts_with(b"BSL1"))
            .count()
            > 2,
        "259 semantic leaves must span multiple bounded block pages",
    );
    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);
    assert_eq!(installed.parse_generation(), 1);
    close_host(&mut host);
    close_publication(&mut runtime, &mut publication);
    close_runtime(runtime);
}

#[test]
fn partial_segmented_block_build_aborts_to_zero_residency() {
    let mut text = String::new();
    for ordinal in 0..130 {
        if ordinal != 0 {
            text.push('\n');
        }
        text.push_str(&format!("paragraph-{ordinal}\n"));
    }
    let mut runtime = producer_runtime(&text);
    let candidate = candidate(&mut runtime, 3);
    let mut writer = candidate
        .into_writer(&mut runtime, DOCUMENT, [62; 16], 1)
        .expect("segmented writer");
    for _ in 0..20 {
        assert!(matches!(
            writer.poll(&mut runtime, 1).expect("block writer poll"),
            M11ParserCandidateWriterPoll::Pending { transitions: 1 }
        ));
    }
    writer
        .begin_abort(&mut runtime)
        .expect("begin segmented abort");
    while !writer
        .poll_abort(&mut runtime, 1)
        .expect("segmented abort poll")
    {}
    close_runtime(runtime);
}

#[test]
fn source_backed_reference_values_publish_and_install_with_exact_fuel_one_work() {
    let text = "[É😀]: /a&amp;b\\* \"T&amp;\\*😀\"\n";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");
    let candidate = candidate(&mut runtime, 4);
    let (mut publication, receipt) = publish_with_receipt(&mut runtime, candidate, [32; 16], 1);
    assert_eq!(receipt.completed_definitions, 1);
    assert_eq!(receipt.probe_bytes, 24);
    assert_eq!(receipt.count_input_bytes, 22);
    assert_eq!(receipt.emit_input_bytes, 22);
    assert_eq!(receipt.cooked_bytes_emitted, 12);
    assert!(!receipt.cancelled);
    assert!(receipt.maximum_source_window_bytes <= SOURCE_CURSOR_WINDOW_BYTES);
    assert!(receipt.maximum_retained_bytes() <= SOURCE_CURSOR_WINDOW_BYTES * 3);

    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);
    assert_eq!(installed.parse_generation(), 1);
    assert!(close_host(&mut host) > 1);
    assert!(close_publication(&mut runtime, &mut publication) > 1);
    close_runtime(runtime);
}

#[test]
fn persistent_certification_derives_fresh_references_from_a_whole_source_parse() {
    let text = "[persistent]: /fresh \"reference\"\n";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");

    assert!(matches!(
        runtime.certify_current_persistent_source(),
        Err(DocumentRuntimeError::NoPersistentSourceFactsBase)
    ));
    prepare_runtime_source_facts(&mut runtime, 4);

    let certified = runtime
        .certify_current_persistent_source()
        .expect("persistent certification");
    assert_eq!(certified.source(), source);
    assert_eq!(
        certified.parser_profile(),
        ParserProfileId::new(1).expect("parser profile")
    );
    assert_eq!(
        certified.source_facts_profile(),
        SourceFactsScanProfile::new(4).expect("source-fact profile")
    );

    let result = parse(certified.exact_parse_lease());
    let candidate = M11ParserCandidate::derive_segmented_from_persistent(certified, result)
        .expect("persistent parser candidate");
    assert_eq!(candidate.source(), source);
    assert_eq!(candidate.syntax_profile(), 1);

    let (mut publication, receipt) = publish_with_receipt(&mut runtime, candidate, [38; 16], 1);
    assert_eq!(receipt.completed_definitions, 1);
    assert!(receipt.probe_bytes > 0);
    assert!(receipt.count_input_bytes > 0);
    assert!(receipt.emit_input_bytes > 0);
    assert!(receipt.cooked_bytes_emitted > 0);
    assert!(!receipt.cancelled);

    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);
    assert_eq!(installed.parse_generation(), 1);
    assert!(close_host(&mut host) > 1);
    assert!(close_publication(&mut runtime, &mut publication) > 1);
    close_runtime(runtime);
}

#[test]
fn resumable_authoritative_inline_root_publishes_as_projection_schema_v2() {
    let text = "***bold*** and `code`";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");
    prepare_runtime_source_facts(&mut runtime, 4);
    let certified = runtime.take_certified_source().expect("certification");
    let parser_profile = certified.parser_profile();
    let result = parse(certified.exact_parse_lease());
    let (authority, publication) = inline_publication(
        &mut runtime,
        certified.exact_parse_lease(),
        &result,
        parser_profile,
    );
    authority.validate(&runtime).expect("returned source baton");
    drop(authority);
    let M11InlineProjectionPublication::Authoritative(mut root) = publication else {
        panic!("supported inline syntax must produce an authoritative root");
    };
    assert_eq!(root.descriptor().source(), source);
    assert_eq!(root.descriptor().fact_count(), 3);

    let candidate = M11ParserCandidate::derive_with_inline_publication(
        certified,
        &result,
        M11ParserInlinePublication::Authoritative(&root),
    )
    .expect("authoritative parser candidate");
    assert_eq!(
        candidate.role_record_count(M11CandidateRoleBytes::Projection),
        1,
        "typed inline pages must not be replayed as flat Projection records"
    );
    let mut writer = candidate
        .into_writer_with_inline_projection(&mut runtime, DOCUMENT, [40; 16], 1, &root)
        .expect("schema-v2 candidate writer");
    release_inline_root(&mut runtime, &mut root);
    drop(root);

    let mut publication = loop {
        match writer.poll(&mut runtime, 1).expect("candidate writer poll") {
            M11ParserCandidateWriterPoll::Pending { transitions } => {
                assert!(transitions <= 1);
            }
            M11ParserCandidateWriterPoll::Published {
                transitions,
                publication,
            } => {
                assert!(transitions <= 1);
                break *publication;
            }
        }
    };
    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);
    assert_eq!(
        host.role_record_count(installed, M11Role::Projection)
            .expect("Projection count"),
        4,
        "one structural record plus three typed inline pages"
    );
    assert_eq!(
        &read_record(&host, installed, M11Role::Projection, 1)[..4],
        b"IFP2"
    );
    assert!(close_host(&mut host) > 1);
    assert!(close_publication(&mut runtime, &mut publication) > 1);
    close_runtime(runtime);
}

#[test]
fn resumable_unsupported_inline_record_is_exact_schema_v1_metadata() {
    let text = "before name@example.test and <tag> after";
    let mut runtime = producer_runtime(text);
    let source = runtime.current_source_version().expect("source");
    prepare_runtime_source_facts(&mut runtime, 4);
    let certified = runtime.take_certified_source().expect("certification");
    let parser_profile = certified.parser_profile();
    let result = parse(certified.exact_parse_lease());
    let (authority, publication) = inline_publication(
        &mut runtime,
        certified.exact_parse_lease(),
        &result,
        parser_profile,
    );
    authority.validate(&runtime).expect("returned source baton");
    drop(authority);
    let M11InlineProjectionPublication::Unsupported(record) = publication else {
        panic!("lexical hazard must fail the exact Paragraph closed");
    };
    let candidate = M11ParserCandidate::derive_with_inline_publication(
        certified,
        &result,
        M11ParserInlinePublication::Unsupported(record),
    )
    .expect("Unsupported parser candidate");
    assert_eq!(
        candidate.role_record_count(M11CandidateRoleBytes::Projection),
        2
    );
    let metadata = candidate
        .role_record(M11CandidateRoleBytes::Projection, 1)
        .expect("Unsupported metadata")
        .to_vec();
    assert_eq!(&metadata[..8], M11_INLINE_META_MAGIC);
    assert_eq!(
        u32::from_le_bytes(metadata[8..12].try_into().unwrap()),
        M11_INLINE_SCHEMA
    );
    assert_eq!(metadata[12], 2);
    assert_eq!(u32::from_le_bytes(metadata[16..20].try_into().unwrap()), 1);
    assert_eq!(u32::from_le_bytes(metadata[20..24].try_into().unwrap()), 0);
    assert_eq!(u64::from_le_bytes(metadata[24..32].try_into().unwrap()), 0);
    assert_eq!(
        u64::from_le_bytes(metadata[32..40].try_into().unwrap()),
        text.len() as u64
    );

    let mut publication = publish(&mut runtime, candidate, [41; 16], 1);
    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);
    assert_eq!(
        host.role_record_count(installed, M11Role::Projection)
            .expect("Projection count"),
        2
    );
    assert_eq!(
        read_record(&host, installed, M11Role::Projection, 1),
        metadata
    );
    assert!(close_host(&mut host) > 1);
    assert!(close_publication(&mut runtime, &mut publication) > 1);
    close_runtime(runtime);
}

#[test]
fn persistent_empty_terminal_uses_the_owning_block_route_without_inline_publication() {
    let mut runtime = producer_runtime("");
    prepare_runtime_source_facts(&mut runtime, 4);
    let certified = runtime
        .certify_current_persistent_source()
        .expect("persistent certification");
    let source = certified.source();
    let result = parse(certified.exact_parse_lease());
    assert!(result.visible_source().is_none());
    let candidate = M11ParserCandidate::derive_segmented_from_persistent(certified, result)
        .expect("owning empty persistent candidate");
    assert_eq!(
        candidate.role_record_count(M11CandidateRoleBytes::Projection),
        0
    );
    let mut publication = publish(&mut runtime, candidate, [42; 16], 1);
    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);
    assert_eq!(installed.parse_generation(), 1);
    close_host(&mut host);
    close_publication(&mut runtime, &mut publication);
    close_runtime(runtime);
}

#[test]
fn ten_mib_destination_is_cooked_without_document_sized_retention() {
    const PAYLOAD: usize = 10 * 1024 * 1024;
    let text = format!("[x]: /{}\n", "u".repeat(PAYLOAD));
    let mut runtime = producer_runtime(&text);
    let candidate = candidate(&mut runtime, 4096);
    let (mut publication, receipt) = publish_with_receipt(&mut runtime, candidate, [33; 16], 1);
    let raw_destination = PAYLOAD + 1;
    assert_eq!(receipt.completed_definitions, 1);
    assert_eq!(receipt.probe_bytes, raw_destination as u64);
    assert_eq!(receipt.count_input_bytes, raw_destination as u64);
    assert_eq!(receipt.emit_input_bytes, raw_destination as u64);
    assert_eq!(receipt.cooked_bytes_emitted, raw_destination as u64);
    assert!(receipt.maximum_source_window_bytes <= SOURCE_CURSOR_WINDOW_BYTES);
    assert!(receipt.maximum_retained_bytes() <= SOURCE_CURSOR_WINDOW_BYTES * 3);
    assert!(close_publication(&mut runtime, &mut publication) > 1);
    close_runtime(runtime);
}

#[test]
fn ten_mib_title_is_cooked_without_document_sized_retention() {
    const PAYLOAD: usize = 10 * 1024 * 1024;
    let text = format!("[x]: /u \"{}\"\n", "t".repeat(PAYLOAD));
    let mut runtime = producer_runtime(&text);
    let candidate = candidate(&mut runtime, 4096);
    let (mut publication, receipt) = publish_with_receipt(&mut runtime, candidate, [36; 16], 1);
    assert_eq!(receipt.completed_definitions, 1);
    assert_eq!(receipt.probe_bytes, (PAYLOAD + 4) as u64);
    assert_eq!(receipt.count_input_bytes, (PAYLOAD + 2) as u64);
    assert_eq!(receipt.emit_input_bytes, (PAYLOAD + 2) as u64);
    assert_eq!(receipt.cooked_bytes_emitted, (PAYLOAD + 2) as u64);
    assert!(receipt.maximum_source_window_bytes <= SOURCE_CURSOR_WINDOW_BYTES);
    assert!(receipt.maximum_retained_bytes() <= SOURCE_CURSOR_WINDOW_BYTES * 3);
    assert!(close_publication(&mut runtime, &mut publication) > 1);
    close_runtime(runtime);
}

#[test]
fn giant_probe_and_partial_emit_cancel_without_a_publication() {
    const GIANT: usize = 10 * 1024 * 1024;
    let giant_text = format!("[x]: /{}\n", "u".repeat(GIANT));
    let mut probe_runtime = producer_runtime(&giant_text);
    let probe_candidate = candidate(&mut probe_runtime, 4096);
    let mut probe_writer = probe_candidate
        .into_writer(&mut probe_runtime, DOCUMENT, [34; 16], 1)
        .expect("probe writer");
    while probe_writer.reference_cook_receipt().probe_bytes < 17 {
        assert!(matches!(
            probe_writer
                .poll(&mut probe_runtime, 1)
                .expect("probe poll"),
            M11ParserCandidateWriterPoll::Pending { transitions: 1 }
        ));
    }
    probe_writer
        .begin_abort(&mut probe_runtime)
        .expect("probe abort");
    let probe_receipt = probe_writer.reference_cook_receipt();
    assert!(probe_receipt.cancelled);
    assert_eq!(probe_receipt.probe_bytes, 17);
    while !probe_writer
        .poll_abort(&mut probe_runtime, 1)
        .expect("probe abort poll")
    {}
    close_runtime(probe_runtime);

    let partial_text = format!("[x]: /{}\n", "v".repeat(64 * 1024));
    let mut emit_runtime = producer_runtime(&partial_text);
    let emit_candidate = candidate(&mut emit_runtime, 4096);
    let mut emit_writer = emit_candidate
        .into_writer(&mut emit_runtime, DOCUMENT, [35; 16], 1)
        .expect("emit writer");
    loop {
        assert!(matches!(
            emit_writer.poll(&mut emit_runtime, 1).expect("emit poll"),
            M11ParserCandidateWriterPoll::Pending { transitions: 1 }
        ));
        let receipt = emit_writer.reference_cook_receipt();
        if receipt.emit_input_bytes > 8192 && receipt.cooked_bytes_emitted > 8192 {
            break;
        }
    }
    emit_writer
        .begin_abort(&mut emit_runtime)
        .expect("emit abort");
    let emit_receipt = emit_writer.reference_cook_receipt();
    assert!(emit_receipt.cancelled);
    assert!(emit_receipt.maximum_retained_bytes() <= SOURCE_CURSOR_WINDOW_BYTES * 3);
    let mut reclaim_polls = 0;
    while !emit_writer
        .poll_abort(&mut emit_runtime, 1)
        .expect("emit abort poll")
    {
        reclaim_polls += 1;
    }
    assert!(reclaim_polls > 1);
    close_runtime(emit_runtime);
}

#[test]
fn persistent_source_facts_publish_past_the_legacy_flat_fanout_limit() {
    let text = "z".repeat(40_000);
    let mut runtime = producer_runtime(&text);
    let source = runtime.current_source_version().expect("source");
    let candidate = candidate(&mut runtime, 4);
    let records = runtime
        .persistent_source_facts()
        .expect("persistent SourceFacts")
        .page_count();
    assert!(records > 128, "fixture must exceed the removed flat fanout");

    let mut publication = publish(&mut runtime, candidate, [37; 16], 1);
    let mut host = M11CandidateHost::new(DOCUMENT, source, 1).expect("host");
    let installed = install(&runtime, &mut host, &publication);
    assert_eq!(
        host.role_record_count(installed, M11Role::SourceFacts)
            .expect("persistent SourceFacts role"),
        records
    );
    assert!(close_host(&mut host) > 1);
    assert!(close_publication(&mut runtime, &mut publication) > 1);
    close_runtime(runtime);
}

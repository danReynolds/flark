use flark_engine::m11_host::M11_CANDIDATE_ARENA_MAX_SLOTS;
use flark_engine::parser_internal::{
    M11BlockSequenceEntryKind, M11BlockSequencePoint, M11CandidatePublication,
    M11OwnedSnapshotPoll, M11ParserSourceRangeAuthority, M11RetainedCandidatePublication,
    M11SnapshotFrameKind, M11_MAX_ROLE_RECORDS,
};
use flark_engine::{
    ArenaLimits, CertifiedSource, DocumentRuntime, DocumentRuntimeConfig, ParserProfileId,
    RuntimeSourceFactsPoll, SourceBoundaryAffinity, SourceFactsRootLimits, SourceFactsScanProfile,
    SourceSnapshotLease,
};
use flark_parser::{
    resolve_m11_published_inline_leaf_fence, M11CleanDocumentResult, M11CleanParseJob,
    M11CleanParsePoll, M11InlineProjectionJob, M11InlineProjectionJobPollStatus,
    M11InlineProjectionPublication, M11OrdinaryParagraphCropParseJob, M11OrdinaryParagraphCropPlan,
    M11OrdinaryParagraphCropPoll, M11ParserBinding, M11ParserCandidate, M11ParserCandidateWriter,
    M11ParserCandidateWriterPoll, M11ParserInlinePublication,
    M11PublishedInlineLeafFenceResolution,
};

const DOCUMENT: [u8; 16] = [0x71; 16];
const PROFILE: u64 = 31;
const FACT_SPACING: usize = 64;

fn binding() -> M11ParserBinding {
    M11ParserBinding::current(ParserProfileId::new(PROFILE).expect("parser profile"))
}

fn paragraph_source(line_count: usize) -> String {
    let mut source = String::new();
    for ordinal in 0..line_count {
        source.push_str(&format!("line-{ordinal:03}-{}\n", "a".repeat(500)));
    }
    source
}

fn segmented_paragraph_source(paragraph_count: usize) -> String {
    let mut source = String::new();
    for ordinal in 0..paragraph_count {
        source.push_str(&format!(
            "paragraph-{ordinal:04}-{}\ncontinuation-{ordinal:04}-{}\n\n",
            "a".repeat(48),
            "b".repeat(48),
        ));
    }
    source
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

fn certify(runtime: &mut DocumentRuntime) -> CertifiedSource {
    runtime
        .begin_source_facts(
            SourceFactsScanProfile::new(FACT_SPACING).expect("SourceFacts profile"),
            binding().syntax_profile(),
            SourceFactsRootLimits::default(),
        )
        .expect("begin SourceFacts");
    loop {
        match runtime
            .poll_source_facts(4096, 64)
            .expect("SourceFacts poll")
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. }
            | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
            RuntimeSourceFactsPoll::Complete { .. } => {
                return runtime
                    .take_certified_source()
                    .expect("completed certification");
            }
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
            | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                panic!("clean certification reported incremental progress")
            }
        }
    }
}

fn parse(lease: SourceSnapshotLease) -> M11CleanDocumentResult {
    let mut job = M11CleanParseJob::new(lease).expect("clean parse");
    loop {
        match job.poll(64).expect("clean parse poll") {
            M11CleanParsePoll::Pending { .. } => {}
            M11CleanParsePoll::Complete { result, .. } => return result,
        }
    }
}

fn publish(
    runtime: &mut DocumentRuntime,
    candidate: M11ParserCandidate,
    publication: [u8; 16],
    generation: u64,
) -> Box<M11CandidatePublication> {
    let writer = candidate
        .into_writer(runtime, DOCUMENT, publication, generation)
        .expect("candidate writer");
    publish_writer(runtime, writer)
}

fn publish_writer(
    runtime: &mut DocumentRuntime,
    mut writer: M11ParserCandidateWriter,
) -> Box<M11CandidatePublication> {
    loop {
        match writer.poll(runtime, 64).expect("candidate writer poll") {
            M11ParserCandidateWriterPoll::Pending { .. } => {}
            M11ParserCandidateWriterPoll::Published { publication, .. } => return publication,
        }
    }
}

fn publish_flat_candidate_with_fuelled_inline(
    runtime: &mut DocumentRuntime,
    certified: CertifiedSource,
    result: &M11CleanDocumentResult,
    publication: [u8; 16],
    generation: u64,
) -> Box<M11CandidatePublication> {
    let parser_profile = certified.parser_profile();
    let visible = result.visible_source().expect("Paragraph visible source");
    let authority = M11ParserSourceRangeAuthority::new(
        runtime,
        certified.exact_parse_lease(),
        usize::try_from(visible.start).expect("visible start")
            ..usize::try_from(visible.end).expect("visible end"),
    )
    .expect("inline source authority");
    let mut job = M11InlineProjectionJob::new(runtime, authority, result, binding())
        .expect("inline Projection job");
    loop {
        let poll = job.poll(runtime, 1).expect("inline Projection poll");
        assert!(poll.transitions() <= 1);
        if poll.status() == M11InlineProjectionJobPollStatus::Complete {
            break;
        }
    }
    let parts = job
        .take_output()
        .expect("inline Projection output")
        .into_publication_parts();
    assert_eq!(parts.source(), result.source_version());
    assert_eq!(parts.source_range(), visible);
    assert_eq!(parts.parser_profile(), parser_profile);
    let (_, _, _, authority, inline) = parts.into_parts();
    authority.validate(runtime).expect("returned source baton");
    drop(authority);
    drop(job);

    match inline {
        M11InlineProjectionPublication::Authoritative(mut root) => {
            let candidate = M11ParserCandidate::derive_with_inline_publication(
                certified,
                result,
                M11ParserInlinePublication::Authoritative(&root),
            )
            .expect("flat candidate");
            let writer = candidate
                .into_writer_with_inline_projection(
                    runtime,
                    DOCUMENT,
                    publication,
                    generation,
                    &root,
                )
                .expect("flat candidate writer");
            root.begin_release(runtime)
                .expect("begin inline root release");
            while !root
                .poll_release(runtime, 1)
                .expect("poll inline root release")
                .complete()
            {}
            publish_writer(runtime, writer)
        }
        M11InlineProjectionPublication::Unsupported(record) => {
            let candidate = M11ParserCandidate::derive_with_inline_publication(
                certified,
                result,
                M11ParserInlinePublication::Unsupported(record),
            )
            .expect("flat Unsupported candidate");
            publish(runtime, candidate, publication, generation)
        }
    }
}

fn retain(
    runtime: &DocumentRuntime,
    publication: Box<M11CandidatePublication>,
) -> M11RetainedCandidatePublication {
    let mut stream = publication
        .into_snapshot_stream(runtime)
        .expect("snapshot stream");
    assert_eq!(
        stream.begin_frame().expect("snapshot begin").kind,
        M11SnapshotFrameKind::Begin
    );
    loop {
        match stream.poll(runtime, 64).expect("snapshot poll") {
            M11OwnedSnapshotPoll::Pending { .. } => {}
            M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                panic!("full snapshot requested exact replay")
            }
            M11OwnedSnapshotPoll::Frame { frame, .. } => {
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

fn close_retained(
    runtime: &mut DocumentRuntime,
    publication: &mut M11RetainedCandidatePublication,
) {
    publication.begin_close(runtime).expect("begin close");
    while !publication.poll_close(runtime, 1).expect("close poll") {}
}

fn close_runtime(mut runtime: DocumentRuntime) {
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(64).expect("runtime close").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
}

fn exact_writer(
    segmented_base: bool,
) -> (
    DocumentRuntime,
    M11RetainedCandidatePublication,
    M11ParserCandidateWriter,
    usize,
) {
    let source = paragraph_source(48);
    let mut runtime = runtime(&source);
    let base_certified = certify(&mut runtime);
    let mut base_result = parse(base_certified.exact_parse_lease());
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("ordinary checkpoints");
    let edit_start = source.find("line-024-").expect("edit line") + 32;
    let changed = edit_start..edit_start + 1;
    let selection = checkpoints
        .select_crop(changed.clone())
        .expect("crop selection");
    let plan = M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("crop plan");
    let base_publication = if segmented_base {
        let base_candidate = M11ParserCandidate::derive_segmented(base_certified, base_result)
            .expect("segmented base candidate");
        publish(&mut runtime, base_candidate, [0x72; 16], 1)
    } else {
        publish_flat_candidate_with_fuelled_inline(
            &mut runtime,
            base_certified,
            &base_result,
            [0x72; 16],
            1,
        )
    };
    let retained_base = retain(&runtime, base_publication);

    runtime.apply_edit(base, changed, "z").expect("target edit");
    let prefix = runtime
        .mint_exact_unchanged_prefix_witness(
            base,
            selection.restart_prefix_end_byte() as usize,
            selection.restart_prefix_end_utf16() as usize,
        )
        .expect("prefix witness");
    let prefix = runtime
        .take_exact_unchanged_prefix_witness(prefix)
        .expect("fresh prefix witness");
    let suffix = runtime
        .mint_exact_unchanged_suffix_witness(
            base,
            selection.convergence_line_start_byte() as usize,
            selection.convergence_line_start_utf16() as usize,
        )
        .expect("suffix witness");
    let suffix = runtime
        .take_exact_unchanged_suffix_witness(suffix)
        .expect("fresh suffix witness");
    let target_certified = certify(&mut runtime);
    let mut crop = M11OrdinaryParagraphCropParseJob::new(
        plan,
        prefix,
        suffix,
        target_certified.exact_parse_lease(),
        binding(),
    )
    .expect("ordinary crop");
    let cropped = loop {
        match crop.poll(64).expect("crop poll") {
            M11OrdinaryParagraphCropPoll::Pending { .. } => {}
            M11OrdinaryParagraphCropPoll::Complete { result, .. } => break result,
        }
    };
    let input = cropped
        .into_exact_segmented_candidate_input()
        .expect("exact segmented input");
    let candidate = M11ParserCandidate::derive_segmented_reusing_references(
        input,
        binding().syntax_profile(),
        SourceFactsScanProfile::new(FACT_SPACING).expect("SourceFacts profile"),
    )
    .expect("exact candidate");
    let writer = candidate
        .into_writer(&mut runtime, DOCUMENT, [0x73; 16], 2)
        .expect("exact writer");
    drop(target_certified);
    (runtime, retained_base, writer, edit_start)
}

#[test]
fn retained_setext_heading_mints_exact_hot_inline_authority() {
    let source = "p\n\n[x]: /url\nβ😀\r\n  ---  \r\n";
    let mut runtime = runtime(source);
    let certified = certify(&mut runtime);
    let result = parse(certified.exact_parse_lease());
    let candidate =
        M11ParserCandidate::derive_segmented(certified, result).expect("segmented candidate");
    let publication = publish(&mut runtime, candidate, [0x79; 16], 1);
    let mut retained = retain(&runtime, publication);
    let point = M11BlockSequencePoint::new(13, 13, SourceBoundaryAffinity::After);
    let resolution = resolve_m11_published_inline_leaf_fence(&runtime, &retained, point)
        .expect("Setext inline fence");
    let M11PublishedInlineLeafFenceResolution::InlineLeaf(fence) = resolution else {
        panic!("Setext must be inline-bearing");
    };
    assert_eq!(fence.kind(), M11BlockSequenceEntryKind::Structured);
    assert_eq!(fence.block_source_range(), 3..30);
    assert_eq!(fence.block_source_utf16_range(), 3..27);
    assert_eq!(fence.inline_source_range(), 13..19);
    assert_eq!(fence.inline_source_utf16_range(), 13..16);
    assert_eq!(fence.entry_ordinal(), 2);
    assert_eq!(fence.binding(), binding());

    close_retained(&mut runtime, &mut retained);
    close_runtime(runtime);
}

#[test]
fn retained_thematic_break_is_an_exact_non_inline_leaf() {
    let source = "p\n\n - - -  \r\n";
    let mut runtime = runtime(source);
    let certified = certify(&mut runtime);
    let result = parse(certified.exact_parse_lease());
    let candidate =
        M11ParserCandidate::derive_segmented(certified, result).expect("segmented candidate");
    let publication = publish(&mut runtime, candidate, [0x7a; 16], 1);
    let mut retained = retain(&runtime, publication);
    let point = M11BlockSequencePoint::new(4, 4, SourceBoundaryAffinity::After);
    let resolution = resolve_m11_published_inline_leaf_fence(&runtime, &retained, point)
        .expect("Thematic Break inline fence");
    let M11PublishedInlineLeafFenceResolution::NotInlineLeaf {
        kind,
        entry_ordinal,
        source,
        source_utf16,
        ..
    } = resolution
    else {
        panic!("Thematic Break must remain non-inline");
    };
    assert_eq!(kind, M11BlockSequenceEntryKind::Structured);
    assert_eq!(entry_ordinal, 2);
    assert_eq!(source, 3..13);
    assert_eq!(source_utf16, 3..13);

    close_retained(&mut runtime, &mut retained);
    close_runtime(runtime);
}

#[test]
fn retained_indented_code_is_an_exact_non_inline_leaf() {
    let source = "p\n\n    code\n";
    let mut runtime = runtime(source);
    let certified = certify(&mut runtime);
    let result = parse(certified.exact_parse_lease());
    let candidate =
        M11ParserCandidate::derive_segmented(certified, result).expect("segmented candidate");
    let publication = publish(&mut runtime, candidate, [0x7b; 16], 1);
    let mut retained = retain(&runtime, publication);
    let point = M11BlockSequencePoint::new(4, 4, SourceBoundaryAffinity::After);
    let resolution = resolve_m11_published_inline_leaf_fence(&runtime, &retained, point)
        .expect("Indented Code inline fence");
    let M11PublishedInlineLeafFenceResolution::NotInlineLeaf {
        kind,
        entry_ordinal,
        source,
        source_utf16,
        ..
    } = resolution
    else {
        panic!("Indented Code must remain non-inline");
    };
    assert_eq!(kind, M11BlockSequenceEntryKind::Structured);
    assert_eq!(entry_ordinal, 2);
    assert_eq!(source, 3..12);
    assert_eq!(source_utf16, 3..12);

    close_retained(&mut runtime, &mut retained);
    close_runtime(runtime);
}

#[test]
fn ordinary_crop_publication_is_built_from_the_retained_block_splice() {
    let (mut runtime, mut base, mut writer, edit_start) = exact_writer(true);
    let publication = loop {
        match writer
            .poll_reusing_references(&mut runtime, 1, &base)
            .expect("exact writer poll")
        {
            M11ParserCandidateWriterPoll::Pending { transitions } => {
                assert_eq!(transitions, 1);
            }
            M11ParserCandidateWriterPoll::Published {
                transitions,
                publication,
            } => {
                assert_eq!(transitions, 1);
                break publication;
            }
        }
    };
    let selection = publication
        .exact_block_splice_selection()
        .expect("parser block selection");
    assert_eq!(selection.base_entry_range(), 0..1);
    assert_eq!(selection.target_entry_range(), 0..1);
    let receipt = publication
        .exact_block_splice_receipt()
        .expect("producer splice receipt");
    assert_eq!(receipt.base_entries(), 1);
    assert_eq!(receipt.deleted_entries(), 1);
    assert_eq!(receipt.replacement_entries(), 1);
    assert_eq!(receipt.base_storage_pages(), 1);
    assert_eq!(receipt.deleted_storage_pages(), 1);
    assert_eq!(receipt.replacement_storage_pages(), 1);

    let mut target = retain(&runtime, publication);
    let located = target
        .locate_block_point(
            &runtime,
            M11BlockSequencePoint::new(edit_start, edit_start, SourceBoundaryAffinity::After),
        )
        .expect("target block query")
        .expect("target Paragraph");
    assert_eq!(located.entry_ordinal(), 0);
    assert_eq!(located.entry().kind(), M11BlockSequenceEntryKind::Paragraph);
    assert_eq!(
        located.byte_range(),
        0..runtime.current_source_version().unwrap().byte_len() as u64
    );

    close_retained(&mut runtime, &mut target);
    close_retained(&mut runtime, &mut base);
    drop(target);
    drop(base);
    close_runtime(runtime);
}

#[test]
fn ordinary_crop_splice_cancels_before_root_construction() {
    let (mut runtime, mut base, mut writer, _) = exact_writer(true);
    writer.begin_abort(&mut runtime).expect("begin early abort");
    while !writer
        .poll_abort(&mut runtime, 1)
        .expect("early abort poll")
    {}
    drop(writer);
    close_retained(&mut runtime, &mut base);
    drop(base);
    close_runtime(runtime);
}

#[test]
fn ordinary_crop_splice_aborts_after_target_root_construction() {
    let (mut runtime, mut base, mut writer, _) = exact_writer(true);
    for _ in 0..2 {
        assert!(matches!(
            writer
                .poll_reusing_references(&mut runtime, 1, &base)
                .expect("splice construction poll"),
            M11ParserCandidateWriterPoll::Pending { transitions: 1 }
        ));
    }
    writer.begin_abort(&mut runtime).expect("begin root abort");
    while !writer.poll_abort(&mut runtime, 1).expect("root abort poll") {}
    drop(writer);
    close_retained(&mut runtime, &mut base);
    drop(base);
    close_runtime(runtime);
}

#[test]
fn failed_splice_remains_abortable_after_consuming_its_target_lease() {
    let (mut runtime, mut flat_base, mut writer, _) = exact_writer(false);
    assert!(matches!(
        writer
            .poll_reusing_references(&mut runtime, 1, &flat_base)
            .expect("replacement encoding"),
        M11ParserCandidateWriterPoll::Pending { transitions: 1 }
    ));
    assert!(writer
        .poll_reusing_references(&mut runtime, 1, &flat_base)
        .is_err());
    writer
        .begin_abort(&mut runtime)
        .expect("failed splice remains abortable");
    while !writer
        .poll_abort(&mut runtime, 1)
        .expect("failed splice abort poll")
    {}
    drop(writer);
    close_retained(&mut runtime, &mut flat_base);
    drop(flat_base);
    close_runtime(runtime);
}

#[test]
fn segmented_middle_crop_reencodes_only_its_parser_selected_block_window() {
    let source = segmented_paragraph_source(512);
    let mut runtime = runtime(&source);
    let base_certified = certify(&mut runtime);
    let mut base_result = parse(base_certified.exact_parse_lease());
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("segmented checkpoints");
    assert!(checkpoints.is_segmented_top_level());
    let edit_start =
        source.find("paragraph-0256-").expect("middle paragraph") + "paragraph-0256-".len() + 8;
    let changed = edit_start..edit_start + 1;
    let selection = checkpoints
        .select_crop(changed.clone())
        .expect("segmented crop selection");
    let plan = M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("crop plan");

    let base_candidate =
        M11ParserCandidate::derive_segmented(base_certified, base_result).expect("base candidate");
    let base_publication = publish(&mut runtime, base_candidate, [0x74; 16], 1);
    let mut retained_base = retain(&runtime, base_publication);

    runtime.apply_edit(base, changed, "z").expect("target edit");
    let prefix = runtime
        .mint_exact_unchanged_prefix_witness(
            base,
            selection.restart_prefix_end_byte() as usize,
            selection.restart_prefix_end_utf16() as usize,
        )
        .expect("prefix witness");
    let prefix = runtime
        .take_exact_unchanged_prefix_witness(prefix)
        .expect("fresh prefix witness");
    let suffix = runtime
        .mint_exact_unchanged_suffix_witness(
            base,
            selection.convergence_suffix_start_byte() as usize,
            selection.convergence_suffix_start_utf16() as usize,
        )
        .expect("paragraph-opening suffix witness");
    let suffix = runtime
        .take_exact_unchanged_suffix_witness(suffix)
        .expect("fresh suffix witness");
    let target_certified = certify(&mut runtime);
    let mut crop = M11OrdinaryParagraphCropParseJob::new(
        plan,
        prefix,
        suffix,
        target_certified.exact_parse_lease(),
        binding(),
    )
    .expect("segmented crop");
    let cropped = loop {
        match crop.poll(64).expect("crop poll") {
            M11OrdinaryParagraphCropPoll::Pending { .. } => {}
            M11OrdinaryParagraphCropPoll::Complete { result, .. } => break result,
        }
    };
    assert!(cropped.work().crop_source_bytes_discovered() <= 16 * 1024);
    let input = cropped
        .into_exact_segmented_candidate_input()
        .expect("segmented crop input");
    let candidate = M11ParserCandidate::derive_segmented_reusing_references(
        input,
        binding().syntax_profile(),
        SourceFactsScanProfile::new(FACT_SPACING).expect("SourceFacts profile"),
    )
    .expect("target candidate");
    let mut writer = candidate
        .into_writer(&mut runtime, DOCUMENT, [0x75; 16], 2)
        .expect("target writer");
    drop(target_certified);
    let publication = loop {
        match writer
            .poll_reusing_references(&mut runtime, 1, &retained_base)
            .expect("target writer poll")
        {
            M11ParserCandidateWriterPoll::Pending { transitions } => {
                assert_eq!(transitions, 1);
            }
            M11ParserCandidateWriterPoll::Published { publication, .. } => break publication,
        }
    };

    let parser_selection = publication
        .exact_block_splice_selection()
        .expect("parser block selection");
    assert!(parser_selection.base_entry_range().start > 0);
    assert!(
        parser_selection.base_entry_range().end
            < publication
                .exact_block_splice_receipt()
                .expect("producer splice receipt")
                .base_entries()
    );
    let base_range = parser_selection.base_entry_range();
    let target_range = parser_selection.target_entry_range();
    assert_eq!(
        base_range.end - base_range.start,
        target_range.end - target_range.start
    );
    let receipt = publication
        .exact_block_splice_receipt()
        .expect("producer splice receipt");
    assert_eq!(receipt.deleted_entries(), receipt.replacement_entries());
    assert!(receipt.deleted_entries() < receipt.base_entries());

    let mut target = retain(&runtime, publication);
    let located = target
        .locate_block_point(
            &runtime,
            M11BlockSequencePoint::new(edit_start, edit_start, SourceBoundaryAffinity::After),
        )
        .expect("target block query")
        .expect("edited Paragraph");
    assert_eq!(located.entry().kind(), M11BlockSequenceEntryKind::Paragraph);
    assert!(located.byte_range().contains(&(edit_start as u64)));

    close_retained(&mut runtime, &mut target);
    close_retained(&mut runtime, &mut retained_base);
    drop(target);
    drop(retained_base);
    close_runtime(runtime);
}

#[test]
fn segmented_thematic_transition_splices_one_bounded_block_window() {
    const PARAGRAPHS: usize = 1_024;
    const EDITED: usize = PARAGRAPHS / 2;
    let source = segmented_paragraph_source(PARAGRAPHS);
    let paragraph_marker = format!("paragraph-{EDITED:04}-");
    let changed_start = source.find(&paragraph_marker).expect("middle paragraph");
    let separator = source[changed_start..]
        .find("\n\n")
        .map(|offset| changed_start + offset)
        .expect("middle separator");
    let changed = changed_start..separator + 1;

    let mut runtime = runtime(&source);
    let base_certified = certify(&mut runtime);
    let mut base_result = parse(base_certified.exact_parse_lease());
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("segmented checkpoints");
    let selection = checkpoints
        .select_crop(changed.clone())
        .expect("thematic crop selection");
    let plan = M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("crop plan");
    let base_candidate =
        M11ParserCandidate::derive_segmented(base_certified, base_result).expect("base candidate");
    let base_publication = publish(&mut runtime, base_candidate, [0x7b; 16], 1);
    let mut retained_base = retain(&runtime, base_publication);

    runtime
        .apply_edit(base, changed, "***\n")
        .expect("thematic target edit");
    let prefix = runtime
        .mint_exact_unchanged_prefix_witness(
            base,
            selection.restart_prefix_end_byte() as usize,
            selection.restart_prefix_end_utf16() as usize,
        )
        .expect("prefix witness");
    let prefix = runtime
        .take_exact_unchanged_prefix_witness(prefix)
        .expect("fresh prefix witness");
    let suffix = runtime
        .mint_exact_unchanged_suffix_witness(
            base,
            selection.convergence_suffix_start_byte() as usize,
            selection.convergence_suffix_start_utf16() as usize,
        )
        .expect("suffix witness");
    let suffix = runtime
        .take_exact_unchanged_suffix_witness(suffix)
        .expect("fresh suffix witness");
    let target_certified = certify(&mut runtime);
    let mut crop = M11OrdinaryParagraphCropParseJob::new(
        plan,
        prefix,
        suffix,
        target_certified.exact_parse_lease(),
        binding(),
    )
    .expect("segmented thematic crop");
    let cropped = loop {
        match crop.poll(64).expect("crop poll") {
            M11OrdinaryParagraphCropPoll::Pending { .. } => {}
            M11OrdinaryParagraphCropPoll::Complete { result, .. } => break result,
        }
    };
    assert!(cropped.work().crop_source_bytes_discovered() <= 16 * 1024);
    let input = cropped
        .into_exact_segmented_candidate_input()
        .expect("segmented crop input");
    let candidate = M11ParserCandidate::derive_segmented_reusing_references(
        input,
        binding().syntax_profile(),
        SourceFactsScanProfile::new(FACT_SPACING).expect("SourceFacts profile"),
    )
    .expect("target candidate");
    let mut writer = candidate
        .into_writer(&mut runtime, DOCUMENT, [0x7c; 16], 2)
        .expect("target writer");
    drop(target_certified);
    let publication = loop {
        match writer
            .poll_reusing_references(&mut runtime, 1, &retained_base)
            .expect("target writer poll")
        {
            M11ParserCandidateWriterPoll::Pending { transitions } => {
                assert_eq!(transitions, 1);
            }
            M11ParserCandidateWriterPoll::Published { publication, .. } => break publication,
        }
    };

    let parser_selection = publication
        .exact_block_splice_selection()
        .expect("parser block selection");
    assert!(parser_selection.base_entry_range().start > 0);
    let receipt = publication
        .exact_block_splice_receipt()
        .expect("producer splice receipt");
    assert!(
        parser_selection.base_entry_range().end < receipt.base_entries(),
        "the thematic transition must not replace the suffix",
    );
    assert!(receipt.deleted_entries() <= 64);
    assert!(receipt.replacement_entries() <= 64);

    let mut target = retain(&runtime, publication);
    let located = target
        .locate_block_point(
            &runtime,
            M11BlockSequencePoint::new(
                changed_start + 1,
                changed_start + 1,
                SourceBoundaryAffinity::After,
            ),
        )
        .expect("target thematic query")
        .expect("target Thematic Break");
    assert_eq!(
        located.entry().kind(),
        M11BlockSequenceEntryKind::Structured
    );
    let green = located.entry().green().expect("thematic Green").as_bytes();
    let projection = located
        .entry()
        .projection()
        .expect("thematic Projection")
        .as_bytes();
    assert_eq!(green[12], 6);
    assert_eq!(projection[12], 6);
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

    close_retained(&mut runtime, &mut target);
    close_retained(&mut runtime, &mut retained_base);
    drop(target);
    drop(retained_base);
    close_runtime(runtime);
}

#[test]
fn segmented_tight_bullet_edit_splices_one_window_and_matches_clean_publication() {
    const SIDE_PARAGRAPHS: usize = 256;
    let prefix = segmented_paragraph_source(SIDE_PARAGRAPHS);
    let suffix = segmented_paragraph_source(SIDE_PARAGRAPHS);
    let list = "  - α😀 first\r\n  - beta second\r\n\r\n";
    let source = format!("{prefix}{list}{suffix}");
    let changed_start = source.find("beta second").expect("selected list item");
    let changed_start_utf16 = source[..changed_start].encode_utf16().count();
    let changed = changed_start..changed_start + 1;

    let mut runtime = runtime(&source);
    let base_certified = certify(&mut runtime);
    let mut base_result = parse(base_certified.exact_parse_lease());
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("segmented checkpoints");
    let selection = checkpoints
        .select_crop(changed.clone())
        .expect("bullet-list crop selection");
    let plan = M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("crop plan");
    let base_candidate =
        M11ParserCandidate::derive_segmented(base_certified, base_result).expect("base candidate");
    let base_publication = publish(&mut runtime, base_candidate, [0x7d; 16], 1);
    let mut retained_base = retain(&runtime, base_publication);

    runtime
        .apply_edit(base, changed, "β")
        .expect("Unicode list-item edit");
    let prefix_witness = runtime
        .mint_exact_unchanged_prefix_witness(
            base,
            selection.restart_prefix_end_byte() as usize,
            selection.restart_prefix_end_utf16() as usize,
        )
        .expect("prefix witness");
    let prefix_witness = runtime
        .take_exact_unchanged_prefix_witness(prefix_witness)
        .expect("fresh prefix witness");
    let suffix_witness = runtime
        .mint_exact_unchanged_suffix_witness(
            base,
            selection.convergence_suffix_start_byte() as usize,
            selection.convergence_suffix_start_utf16() as usize,
        )
        .expect("suffix witness");
    let suffix_witness = runtime
        .take_exact_unchanged_suffix_witness(suffix_witness)
        .expect("fresh suffix witness");
    let target_certified = certify(&mut runtime);
    let mut crop = M11OrdinaryParagraphCropParseJob::new(
        plan,
        prefix_witness,
        suffix_witness,
        target_certified.exact_parse_lease(),
        binding(),
    )
    .expect("segmented bullet-list crop");
    let cropped = loop {
        match crop.poll(64).expect("crop poll") {
            M11OrdinaryParagraphCropPoll::Pending { .. } => {}
            M11OrdinaryParagraphCropPoll::Complete { result, .. } => break result,
        }
    };
    assert!(cropped.work().crop_source_bytes_discovered() <= 16 * 1024);
    let input = cropped
        .into_exact_segmented_candidate_input()
        .expect("segmented bullet-list input");
    let incremental_candidate = M11ParserCandidate::derive_segmented_reusing_references(
        input,
        binding().syntax_profile(),
        SourceFactsScanProfile::new(FACT_SPACING).expect("SourceFacts profile"),
    )
    .expect("incremental target candidate");
    let clean_result = parse(target_certified.exact_parse_lease());
    let clean_candidate = M11ParserCandidate::derive_segmented(target_certified, clean_result)
        .expect("clean target candidate");

    let mut writer = incremental_candidate
        .into_writer(&mut runtime, DOCUMENT, [0x7e; 16], 2)
        .expect("incremental target writer");
    let incremental_publication = loop {
        match writer
            .poll_reusing_references(&mut runtime, 1, &retained_base)
            .expect("incremental target writer poll")
        {
            M11ParserCandidateWriterPoll::Pending { transitions } => {
                assert_eq!(transitions, 1);
            }
            M11ParserCandidateWriterPoll::Published { publication, .. } => break publication,
        }
    };

    let parser_selection = incremental_publication
        .exact_block_splice_selection()
        .expect("parser block selection");
    let receipt = incremental_publication
        .exact_block_splice_receipt()
        .expect("producer splice receipt");
    assert!(parser_selection.base_entry_range().start > 0);
    assert!(
        parser_selection.base_entry_range().end < receipt.base_entries(),
        "the bullet-list edit must retain both document sides",
    );
    assert!(receipt.deleted_entries() <= 64);
    assert!(receipt.replacement_entries() <= 64);

    let mut incremental = retain(&runtime, incremental_publication);
    let incremental_list = incremental
        .locate_block_point(
            &runtime,
            M11BlockSequencePoint::new(
                changed_start,
                changed_start_utf16,
                SourceBoundaryAffinity::After,
            ),
        )
        .expect("incremental list query")
        .expect("incremental Bullet List");
    assert_eq!(
        incremental_list.entry().kind(),
        M11BlockSequenceEntryKind::Structured
    );
    let incremental_range = incremental_list.byte_range();
    let incremental_green = incremental_list
        .entry()
        .green()
        .expect("incremental list Green")
        .as_bytes()
        .to_vec();
    let incremental_projection = incremental_list
        .entry()
        .projection()
        .expect("incremental list Projection")
        .as_bytes()
        .to_vec();
    assert_eq!(incremental_green[12], 9);
    assert_eq!(incremental_projection[12], 9);

    let clean_publication = publish(&mut runtime, clean_candidate, [0x7f; 16], 2);
    let mut clean = retain(&runtime, clean_publication);
    let clean_list = clean
        .locate_block_point(
            &runtime,
            M11BlockSequencePoint::new(
                changed_start,
                changed_start_utf16,
                SourceBoundaryAffinity::After,
            ),
        )
        .expect("clean list query")
        .expect("clean Bullet List");
    assert_eq!(clean_list.byte_range(), incremental_range);
    assert_eq!(
        clean_list
            .entry()
            .green()
            .expect("clean list Green")
            .as_bytes(),
        incremental_green,
    );
    assert_eq!(
        clean_list
            .entry()
            .projection()
            .expect("clean list Projection")
            .as_bytes(),
        incremental_projection,
    );

    close_retained(&mut runtime, &mut clean);
    close_retained(&mut runtime, &mut incremental);
    close_retained(&mut runtime, &mut retained_base);
    drop(clean);
    drop(incremental);
    drop(retained_base);
    close_runtime(runtime);
}

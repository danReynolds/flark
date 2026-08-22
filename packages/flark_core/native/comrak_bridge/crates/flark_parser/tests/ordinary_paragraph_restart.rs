use flark_engine::{
    DocumentRuntime, DocumentRuntimeConfig, ParserProfileId, SourceFactsScanProfile,
    SourceSnapshotLease,
};
use flark_parser::{
    M11CleanDocumentKind, M11CleanDocumentResult, M11CleanLeaf, M11CleanParseJob,
    M11CleanParsePoll, M11OrdinaryParagraphBofCropParseJob, M11OrdinaryParagraphBofCropPlan,
    M11OrdinaryParagraphBoundaryCropError, M11OrdinaryParagraphBoundaryCropPlanError,
    M11OrdinaryParagraphBoundaryCropPoll, M11OrdinaryParagraphBoundaryCropResult,
    M11OrdinaryParagraphCheckpointError, M11OrdinaryParagraphCropError,
    M11OrdinaryParagraphCropParseJob, M11OrdinaryParagraphCropPlan,
    M11OrdinaryParagraphCropPlanError, M11OrdinaryParagraphCropPoll,
    M11OrdinaryParagraphCropResult, M11OrdinaryParagraphEofCropParseJob,
    M11OrdinaryParagraphEofCropPlan, M11ParserBinding, M11ParserCandidate, M11ParserTerminalFacts,
    M11_ORDINARY_PARAGRAPH_CHECKPOINT_STRIDE_BYTES,
};

const PROFILE: u64 = 31;

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

fn setext_transition_fixture(
    paragraph_count: usize,
    paragraph_ordinal: usize,
    promote: bool,
) -> (String, std::ops::Range<usize>, String) {
    let mut source = segmented_paragraph_source(paragraph_count);
    let continuation = format!("continuation-{paragraph_ordinal:04}-{}", "b".repeat(48));
    let start = source.find(&continuation).expect("transition paragraph");
    let end = start + continuation.len();
    if promote {
        (source, start..end, "---".to_owned())
    } else {
        source.replace_range(start..end, "---");
        (source, start..start + 3, continuation)
    }
}

fn thematic_transition_fixture(
    paragraph_count: usize,
    paragraph_ordinal: usize,
    promote: bool,
) -> (String, std::ops::Range<usize>, String) {
    let mut source = segmented_paragraph_source(paragraph_count);
    let paragraph = format!("paragraph-{paragraph_ordinal:04}-");
    let start = source.find(&paragraph).expect("transition paragraph");
    let separator = source[start..]
        .find("\n\n")
        .map(|offset| start + offset)
        .expect("paragraph separator");
    let end = separator + 1;
    let original = source[start..end].to_owned();
    const THEMATIC: &str = " *  * *   \r\n";
    if promote {
        (source, start..end, THEMATIC.to_owned())
    } else {
        source.replace_range(start..end, THEMATIC);
        (source, start..start + THEMATIC.len(), original)
    }
}

fn indented_code_transition_fixture(
    paragraph_count: usize,
    paragraph_ordinal: usize,
    promote: bool,
) -> (String, std::ops::Range<usize>, String) {
    let mut source = segmented_paragraph_source(paragraph_count);
    let paragraph = format!("paragraph-{paragraph_ordinal:04}-");
    let start = source.find(&paragraph).expect("transition paragraph");
    let separator = source[start..]
        .find("\n\n")
        .map(|offset| start + offset)
        .expect("paragraph separator");
    let end = separator + 1;
    let original = source[start..end].to_owned();
    const INDENTED_CODE: &str = "    code\r\n";
    if promote {
        (source, start..end, INDENTED_CODE.to_owned())
    } else {
        source.replace_range(start..end, INDENTED_CODE);
        (source, start..start + INDENTED_CODE.len(), original)
    }
}

fn reference_prefixed_segmented_source() -> String {
    let mut source = String::from("[early]: /one\n\n");
    source.push_str(&segmented_paragraph_source(64));
    source.push_str("[late]: /two\n\n");
    source.push_str(&segmented_paragraph_source(256));
    source
}

fn parse_job(mut job: M11CleanParseJob) -> M11CleanDocumentResult {
    loop {
        match job.poll(17).expect("parse poll") {
            M11CleanParsePoll::Pending { .. } => {}
            M11CleanParsePoll::Complete { result, .. } => return result,
        }
    }
}

fn parse(lease: SourceSnapshotLease) -> M11CleanDocumentResult {
    parse_job(M11CleanParseJob::new(lease).expect("clean parse"))
}

fn close(mut runtime: DocumentRuntime) {
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(64).expect("close poll").complete {}
}

fn crop(
    mut job: M11OrdinaryParagraphCropParseJob,
) -> Result<M11OrdinaryParagraphCropResult, M11OrdinaryParagraphCropError> {
    loop {
        match job.poll(17)? {
            M11OrdinaryParagraphCropPoll::Pending { .. } => {}
            M11OrdinaryParagraphCropPoll::Complete { result, .. } => return Ok(result),
        }
    }
}

fn bof_crop(
    mut job: M11OrdinaryParagraphBofCropParseJob,
) -> Result<M11OrdinaryParagraphBoundaryCropResult, M11OrdinaryParagraphBoundaryCropError> {
    loop {
        match job.poll(17)? {
            M11OrdinaryParagraphBoundaryCropPoll::Pending { .. } => {}
            M11OrdinaryParagraphBoundaryCropPoll::Complete { result, .. } => return Ok(result),
        }
    }
}

fn eof_crop(
    mut job: M11OrdinaryParagraphEofCropParseJob,
) -> Result<M11OrdinaryParagraphBoundaryCropResult, M11OrdinaryParagraphBoundaryCropError> {
    loop {
        match job.poll(17)? {
            M11OrdinaryParagraphBoundaryCropPoll::Pending { .. } => {}
            M11OrdinaryParagraphBoundaryCropPoll::Complete { result, .. } => return Ok(result),
        }
    }
}

#[test]
fn ordinary_paragraph_checkpoints_are_sparse_committed_boundaries_and_one_take() {
    let source = paragraph_source(48);
    let runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut result = parse(runtime.snapshot_current_source().expect("source lease"));
    assert_eq!(result.kind(), M11CleanDocumentKind::Paragraph);

    let checkpoints = result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("ordinary Paragraph checkpoints");
    assert_eq!(checkpoints.source(), result.source_version());
    assert_eq!(checkpoints.binding(), binding());
    assert!(checkpoints.len() >= 2);
    let stride = M11_ORDINARY_PARAGRAPH_CHECKPOINT_STRIDE_BYTES as usize;
    assert!(checkpoints.len() <= source.len().div_ceil(stride));

    let mut previous_end = None;
    for checkpoint in checkpoints.checkpoints() {
        let prefix_end = checkpoint.prefix_end_byte() as usize;
        assert_eq!(checkpoint.source(), result.source_version());
        assert_eq!(checkpoint.binding(), binding());
        assert_eq!(checkpoint.paragraph_content_start(), 0);
        assert!(prefix_end >= stride);
        assert!(prefix_end < source.len());
        assert_eq!(source.as_bytes()[prefix_end - 1], b'\n');
        assert_eq!(checkpoint.prefix_end_utf16() as usize, prefix_end);
        assert!(checkpoint.next_physical_line_ordinal() > 0);
        if let Some(previous_end) = previous_end {
            assert!(prefix_end - previous_end >= stride);
        }
        previous_end = Some(prefix_end);
    }

    assert!(matches!(
        result.take_ordinary_paragraph_restart_checkpoints(binding()),
        Err(M11OrdinaryParagraphCheckpointError::AlreadyTaken)
    ));
    close(runtime);
}

#[test]
fn restart_checkpoints_preserve_exact_preceding_line_content_and_ending_geometry() {
    for ending in ["\n", "\r\n", "\r"] {
        let mut source = String::new();
        for ordinal in 0..24 {
            source.push_str(&format!("line-{ordinal:03}-{}{}", "a".repeat(500), ending));
        }
        let runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let mut result = parse(runtime.snapshot_current_source().expect("source lease"));
        let checkpoints = result
            .take_ordinary_paragraph_restart_checkpoints(binding())
            .expect("ordinary checkpoints");
        assert!(!checkpoints.is_empty());
        for checkpoint in checkpoints.checkpoints() {
            assert_eq!(
                checkpoint.preceding_line_physical_bytes()
                    - checkpoint.preceding_line_content_bytes(),
                ending.len() as u32
            );
            assert_eq!(
                checkpoint.preceding_line_physical_utf16()
                    - checkpoint.preceding_line_content_utf16(),
                ending.encode_utf16().count() as u32
            );
        }
        close(runtime);
    }
}

#[test]
fn segmented_terminal_retains_topology_even_when_only_its_first_paragraph_has_checkpoints() {
    let source = format!("{}\nsecond paragraph\n", paragraph_source(24));
    let runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut result = parse(runtime.snapshot_current_source().expect("source lease"));
    assert_eq!(result.kind(), M11CleanDocumentKind::Segmented);
    assert_eq!(result.leaves().len(), 3);
    let checkpoints = result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("segmented restart collection");
    assert!(checkpoints.is_segmented_top_level());
    assert!(!checkpoints.is_empty());
    assert!(checkpoints
        .checkpoints()
        .iter()
        .all(|checkpoint| checkpoint.block_entry_ordinal() == 0));
    assert!(matches!(
        result.take_ordinary_paragraph_restart_checkpoints(binding()),
        Err(M11OrdinaryParagraphCheckpointError::AlreadyTaken)
    ));
    close(runtime);
}

#[test]
fn seeded_remainder_parse_matches_a_clean_parse_after_a_tail_edit() {
    let source = paragraph_source(48);
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("ordinary Paragraph checkpoints");
    let checkpoint = checkpoints
        .into_checkpoints()
        .into_iter()
        .next()
        .expect("sparse checkpoint");
    let prefix_end = checkpoint.prefix_end_byte() as usize;
    let prefix_utf16 = checkpoint.prefix_end_utf16() as usize;
    let edit_start = prefix_end + 25;

    let target = runtime
        .apply_edit(base, edit_start..edit_start + 1, "z")
        .expect("tail edit")
        .source()
        .current();
    let witness = runtime
        .mint_exact_unchanged_prefix_witness(base, prefix_end, prefix_utf16)
        .expect("prefix witness");
    let witness = runtime
        .take_exact_unchanged_prefix_witness(witness)
        .expect("fresh prefix witness");
    let resumed_job = M11CleanParseJob::new_for_ordinary_paragraph_remainder(
        checkpoint,
        witness,
        runtime.snapshot_current_source().expect("resumed lease"),
        binding(),
    )
    .expect("resumed parse");
    let resumed = parse_job(resumed_job);
    let clean = parse(runtime.snapshot_current_source().expect("clean lease"));

    assert_eq!(resumed.source_version(), target);
    assert_eq!(resumed.kind(), clean.kind());
    assert_eq!(resumed.source_range(), clean.source_range());
    assert_eq!(resumed.visible_source(), clean.visible_source());
    assert_eq!(resumed.definition_count(), clean.definition_count());
    assert_eq!(
        M11ParserTerminalFacts::derive(&resumed).expect("resumed terminal facts"),
        M11ParserTerminalFacts::derive(&clean).expect("clean terminal facts")
    );

    close(runtime);
}

#[test]
fn authenticated_crop_matches_clean_after_line_and_unicode_shifts() {
    struct Case {
        name: &'static str,
        changed: std::ops::Range<usize>,
        replacement: &'static str,
        ordinal_delta: i64,
    }

    let source = paragraph_source(256);
    let line_80 = source.find("line-080-").expect("line 80");
    let line_83 = source.find("line-083-").expect("line 83");
    let cases = [
        Case {
            name: "inserted lines",
            changed: line_80..line_80,
            replacement: "inserted α😀 line\ninserted second line\n",
            ordinal_delta: 2,
        },
        Case {
            name: "deleted lines",
            changed: line_80..line_83,
            replacement: "",
            ordinal_delta: -3,
        },
        Case {
            name: "different byte and UTF-16 shifts",
            changed: line_80 + 20..line_80 + 30,
            replacement: "世界😀z",
            ordinal_delta: 0,
        },
    ];

    for case in cases {
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect(case.name);
        let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
        let base = base_result.source_version();
        let checkpoints = base_result
            .take_ordinary_paragraph_restart_checkpoints(binding())
            .expect("base checkpoints");
        let selection = checkpoints
            .select_crop(case.changed.clone())
            .expect("crop selection");
        let convergence = &checkpoints.checkpoints()[selection.convergence_index()];
        let base_convergence_next_ordinal = convergence.next_physical_line_ordinal();
        let convergence_physical_bytes = convergence.preceding_line_physical_bytes();
        let convergence_physical_utf16 = convergence.preceding_line_physical_utf16();
        let plan = M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("crop plan");

        let target_text = format!(
            "{}{}{}",
            &source[..case.changed.start],
            case.replacement,
            &source[case.changed.end..]
        );
        let target = runtime
            .apply_edit(base, case.changed.clone(), case.replacement)
            .expect(case.name)
            .source()
            .current();
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
        let target_convergence_start = suffix.target_byte_start();
        let target_convergence_utf16 = suffix.target_utf16_start();
        let crop_job = M11OrdinaryParagraphCropParseJob::new(
            plan,
            prefix,
            suffix,
            runtime.snapshot_current_source().expect("crop lease"),
            binding(),
        )
        .expect("crop job");
        let mut cropped = crop(crop_job).expect(case.name);
        let clean = parse(runtime.snapshot_current_source().expect("clean lease"));

        assert_eq!(cropped.terminal().source_version(), target, "{}", case.name);
        assert_eq!(cropped.terminal().kind(), clean.kind(), "{}", case.name);
        assert_eq!(
            cropped.terminal().source_range(),
            clean.source_range(),
            "{}",
            case.name
        );
        assert_eq!(
            cropped.terminal().visible_source(),
            clean.visible_source(),
            "{}",
            case.name
        );
        assert_eq!(
            cropped.work().convergence_ordinal_delta(),
            case.ordinal_delta,
            "{}",
            case.name
        );
        let crop_range = cropped.work().target_crop_bytes();
        assert_eq!(
            cropped.work().crop_source_bytes_discovered(),
            crop_range.len(),
            "{}",
            case.name
        );
        assert_eq!(
            cropped.work().crop_source_bytes_read(),
            crop_range.len(),
            "{}",
            case.name
        );
        assert!(
            crop_range.len() < target_text.len() / 8,
            "{} scanned {} of {} bytes",
            case.name,
            crop_range.len(),
            target_text.len()
        );

        let next = cropped
            .take_next_restart_checkpoints()
            .expect("target checkpoints");
        assert_eq!(next.source(), target);
        assert_eq!(next.binding(), binding());
        let converged = next
            .checkpoints()
            .iter()
            .find(|checkpoint| {
                checkpoint.preceding_line_start_byte() as usize == target_convergence_start
            })
            .expect("preserved convergence checkpoint");
        assert_eq!(
            converged.preceding_line_start_utf16() as usize,
            target_convergence_utf16
        );
        assert_eq!(
            converged.preceding_line_physical_bytes(),
            convergence_physical_bytes
        );
        assert_eq!(
            converged.preceding_line_physical_utf16(),
            convergence_physical_utf16
        );
        assert_eq!(
            i64::from(converged.next_physical_line_ordinal()),
            i64::from(base_convergence_next_ordinal) + case.ordinal_delta
        );
        for checkpoint in next.checkpoints() {
            let start = checkpoint.preceding_line_start_byte() as usize;
            let end = checkpoint.prefix_end_byte() as usize;
            assert_eq!(
                end - start,
                checkpoint.preceding_line_physical_bytes() as usize
            );
            assert_eq!(target_text.as_bytes()[end - 1], b'\n');
            assert_eq!(
                target_text[..start].encode_utf16().count(),
                checkpoint.preceding_line_start_utf16() as usize
            );
            assert_eq!(
                target_text[..end].encode_utf16().count(),
                checkpoint.prefix_end_utf16() as usize
            );
        }
        assert!(matches!(
            cropped.take_next_restart_checkpoints(),
            Err(M11OrdinaryParagraphCheckpointError::AlreadyTaken)
        ));
        let mut terminal = cropped.into_terminal();
        assert!(matches!(
            terminal.take_ordinary_paragraph_restart_checkpoints(binding()),
            Err(M11OrdinaryParagraphCheckpointError::Ineligible)
        ));
        close(runtime);
    }
}

#[test]
fn middle_edit_in_4096_paragraphs_discovers_only_a_bounded_parser_window() {
    const PARAGRAPHS: usize = 4096;
    let source = segmented_paragraph_source(PARAGRAPHS);
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
    assert_eq!(base_result.kind(), M11CleanDocumentKind::Segmented);
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("segmented checkpoints");
    assert!(checkpoints.is_segmented_top_level());

    let edit_start =
        source.find("paragraph-2048-").expect("middle paragraph") + "paragraph-2048-".len() + 8;
    let changed = edit_start..edit_start + 1;
    let selection = checkpoints
        .select_crop(changed.clone())
        .expect("segmented crop selection");
    assert!(selection.is_segmented_top_level());
    let base_convergence_block = selection.convergence_block_entry_ordinal();
    let plan = M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("crop plan");

    let target = runtime
        .apply_edit(base, changed, "z")
        .expect("middle edit")
        .source()
        .current();
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
    let target_convergence_start = suffix.target_byte_start();
    let suffix = runtime
        .take_exact_unchanged_suffix_witness(suffix)
        .expect("fresh suffix witness");
    let crop_job = M11OrdinaryParagraphCropParseJob::new(
        plan,
        prefix,
        suffix,
        runtime.snapshot_current_source().expect("crop lease"),
        binding(),
    )
    .expect("segmented crop job");
    let mut cropped = crop(crop_job).expect("segmented crop");

    let work = cropped.work();
    let crop_range = work.target_crop_bytes();
    assert_eq!(work.crop_source_bytes_discovered(), crop_range.len());
    assert_eq!(work.crop_source_bytes_read(), crop_range.len());
    assert!(
        work.crop_source_bytes_discovered() <= 16 * 1024,
        "bounded crop discovered {} of {} bytes",
        work.crop_source_bytes_discovered(),
        source.len()
    );
    assert!(
        work.crop_physical_lines_discovered() <= 512,
        "bounded crop discovered {} of {} physical lines",
        work.crop_physical_lines_discovered(),
        PARAGRAPHS * 2
    );
    assert!(work.crop_parser_transitions() <= 4096);

    let next = cropped
        .take_next_restart_checkpoints()
        .expect("target checkpoints");
    assert_eq!(next.source(), target);
    assert!(next.is_segmented_top_level());
    let converged = next
        .checkpoints()
        .iter()
        .find(|checkpoint| {
            checkpoint.paragraph_source_start_byte() as usize == target_convergence_start
        })
        .expect("preserved convergence paragraph");
    assert_eq!(converged.block_entry_ordinal(), base_convergence_block);
    assert!(next.checkpoints().windows(2).all(|pair| {
        pair[0].prefix_end_byte() < pair[1].prefix_end_byte()
            && pair[0].block_entry_ordinal() <= pair[1].block_entry_ordinal()
    }));

    close(runtime);
}

#[test]
fn definition_prefix_freezes_reference_authority_before_a_bounded_tail_crop() {
    let source = reference_prefixed_segmented_source();
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
    assert_eq!(base_result.kind(), M11CleanDocumentKind::Segmented);
    assert_eq!(base_result.definition_count(), 2);
    let last_definition_leaf = base_result
        .leaves()
        .iter()
        .rposition(|leaf| leaf.reference_definition_count() != 0)
        .expect("definition-bearing leaf");
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("reference-frozen tail checkpoints");
    assert_eq!(checkpoints.frozen_reference_definition_count(), Some(2));
    assert!(checkpoints.is_segmented_top_level());
    assert!(!checkpoints.is_empty());
    assert!(matches!(
        checkpoints.select_bof_crop(0..1),
        Err(M11OrdinaryParagraphBoundaryCropPlanError::FrozenReferencesIneligible)
    ));
    assert!(checkpoints.checkpoints().iter().all(|checkpoint| {
        checkpoint.frozen_reference_definition_count() == 2
            && checkpoint.block_entry_ordinal()
                > u64::try_from(last_definition_leaf).expect("leaf ordinal")
    }));

    let definition_edit = source.find("[late]").expect("late definition");
    assert!(matches!(
        checkpoints.select_crop(definition_edit..definition_edit + 1),
        Err(M11OrdinaryParagraphCropPlanError::NoRestartCheckpoint)
    ));

    let edit_start =
        source.find("paragraph-0128-").expect("tail paragraph") + "paragraph-0128-".len() + 8;
    let changed = edit_start..edit_start + 1;
    let selection = checkpoints
        .select_crop(changed.clone())
        .expect("bounded tail crop selection");
    assert!(
        selection.restart_block_entry_ordinal()
            > u64::try_from(last_definition_leaf).expect("leaf ordinal")
    );
    let plan = M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("crop plan");

    let target = runtime
        .apply_edit(base, changed, "z")
        .expect("tail edit")
        .source()
        .current();
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
    let job = M11OrdinaryParagraphCropParseJob::new(
        plan,
        prefix,
        suffix,
        runtime.snapshot_current_source().expect("crop lease"),
        binding(),
    )
    .expect("reference-frozen crop job");
    let mut cropped = crop(job).expect("reference-frozen crop");
    assert!(
        cropped.work().crop_source_bytes_discovered() <= 16 * 1024,
        "reference-frozen crop must remain bounded"
    );
    let next = cropped
        .take_next_restart_checkpoints()
        .expect("target checkpoints");
    assert_eq!(next.source(), target);
    assert_eq!(next.frozen_reference_definition_count(), Some(2));
    assert!(next
        .checkpoints()
        .iter()
        .all(|checkpoint| checkpoint.frozen_reference_definition_count() == 2));

    close(runtime);
}

#[test]
fn reference_frozen_tail_crop_rejects_a_new_definition() {
    let source = reference_prefixed_segmented_source();
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("reference-frozen tail checkpoints");

    let changed_start = source.find("paragraph-0128-").expect("tail paragraph");
    let changed_end = source[changed_start..]
        .find('\n')
        .map(|offset| changed_start + offset + 1)
        .expect("tail paragraph line");
    let changed = changed_start..changed_end;
    let selection = checkpoints
        .select_crop(changed.clone())
        .expect("tail definition crop selection");
    let plan = M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("crop plan");
    runtime
        .apply_edit(base, changed, "[new]: /fresh\n")
        .expect("new definition edit");
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
    let job = M11OrdinaryParagraphCropParseJob::new(
        plan,
        prefix,
        suffix,
        runtime.snapshot_current_source().expect("crop lease"),
        binding(),
    )
    .expect("reference-frozen crop job");
    assert!(matches!(
        crop(job),
        Err(M11OrdinaryParagraphCropError::CropDiverged)
    ));
    let clean = parse(
        runtime
            .snapshot_current_source()
            .expect("clean target lease"),
    );
    assert_eq!(clean.definition_count(), 3);

    close(runtime);
}

#[test]
fn unsupported_suffix_cannot_mint_reference_frozen_restarts() {
    let source = format!(
        "[ref]: /target\n\n{}\n- unsupported list\n{}",
        segmented_paragraph_source(64),
        segmented_paragraph_source(128),
    );
    let runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut result = parse(runtime.snapshot_current_source().expect("source lease"));
    assert!(matches!(
        result.take_ordinary_paragraph_restart_checkpoints(binding()),
        Err(M11OrdinaryParagraphCheckpointError::Ineligible)
    ));
    close(runtime);
}

#[test]
fn segmented_crop_tracks_blank_boundary_splits_and_merges_in_block_ordinals() {
    let source = segmented_paragraph_source(1024);
    let split_anchor =
        source.find("paragraph-0512-").expect("split paragraph") + "paragraph-0512-".len() + 12;
    let merge_separator = source.find("\n\nparagraph-0513-").expect("merge separator") + 1;
    let cases = [
        ("split", split_anchor..split_anchor, "\n\nsplit-", 2_i64),
        ("merge", merge_separator..merge_separator + 1, "", -2_i64),
    ];

    for (name, changed, replacement, expected_block_delta) in cases {
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect(name);
        let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
        let base = base_result.source_version();
        let checkpoints = base_result
            .take_ordinary_paragraph_restart_checkpoints(binding())
            .expect("segmented checkpoints");
        let selection = checkpoints
            .select_crop(changed.clone())
            .expect("segmented crop selection");
        assert!(selection.is_segmented_top_level());
        let base_convergence_block = selection.convergence_block_entry_ordinal();
        let plan = M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("crop plan");

        runtime
            .apply_edit(base, changed, replacement)
            .expect("boundary edit");
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
        let target_convergence_start = suffix.target_byte_start();
        let suffix = runtime
            .take_exact_unchanged_suffix_witness(suffix)
            .expect("fresh suffix witness");
        let job = M11OrdinaryParagraphCropParseJob::new(
            plan,
            prefix,
            suffix,
            runtime.snapshot_current_source().expect("crop lease"),
            binding(),
        )
        .expect("boundary crop job");
        let mut cropped = crop(job).expect(name);
        assert!(cropped.work().crop_source_bytes_discovered() <= 16 * 1024);

        let next = cropped
            .take_next_restart_checkpoints()
            .expect("target checkpoints");
        let converged = next
            .checkpoints()
            .iter()
            .find(|checkpoint| {
                checkpoint.paragraph_source_start_byte() as usize == target_convergence_start
            })
            .expect("shifted convergence paragraph");
        assert_eq!(
            i64::try_from(converged.block_entry_ordinal()).expect("target block ordinal"),
            i64::try_from(base_convergence_block).expect("base block ordinal")
                + expected_block_delta,
            "{name}"
        );
        close(runtime);
    }
}

#[test]
fn crop_rejects_a_suffix_that_maps_to_the_middle_of_a_target_line() {
    let source = paragraph_source(96);
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("base checkpoints");
    let convergence_start = checkpoints.checkpoints()[5].preceding_line_start_byte() as usize;
    let changed = convergence_start - 1..convergence_start;
    assert_eq!(&source[changed.clone()], "\n");
    let selection = checkpoints
        .select_crop(changed.clone())
        .expect("crop selection");
    assert_eq!(
        selection.convergence_line_start_byte() as usize,
        convergence_start
    );
    let plan = M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("crop plan");

    runtime
        .apply_edit(base, changed, " ")
        .expect("join convergence line");
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
        .expect("suffix text remains exact");
    let suffix = runtime
        .take_exact_unchanged_suffix_witness(suffix)
        .expect("fresh suffix witness");
    let crop_job = M11OrdinaryParagraphCropParseJob::new(
        plan,
        prefix,
        suffix,
        runtime.snapshot_current_source().expect("crop lease"),
        binding(),
    )
    .expect("bounded crop starts from exact prefix");
    assert!(matches!(
        crop(crop_job),
        Err(M11OrdinaryParagraphCropError::ConvergenceMismatch)
    ));

    close(runtime);
}

#[test]
fn sole_paragraph_crops_fail_closed_when_a_blank_edit_creates_prior_leaves() {
    let source = paragraph_source(96);

    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("base checkpoints");
    let insertion = source.find("line-048-").expect("middle line") + 24;
    let changed = insertion..insertion;
    let selection = checkpoints
        .select_crop(changed.clone())
        .expect("interior selection");
    let plan = M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("interior plan");
    runtime
        .apply_edit(base, changed, "\n\nsplit paragraph")
        .expect("blank split");
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
    let job = M11OrdinaryParagraphCropParseJob::new(
        plan,
        prefix,
        suffix,
        runtime.snapshot_current_source().expect("crop lease"),
        binding(),
    )
    .expect("interior crop starts");
    assert!(matches!(
        crop(job),
        Err(M11OrdinaryParagraphCropError::CropDiverged)
    ));
    close(runtime);

    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("BOF runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("BOF base lease"));
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("BOF checkpoints");
    let selection = checkpoints.select_bof_crop(0..0).expect("BOF selection");
    let plan = M11OrdinaryParagraphBofCropPlan::new(checkpoints, selection).expect("BOF plan");
    runtime
        .apply_edit(base, 0..0, "new paragraph\n\n")
        .expect("BOF blank split");
    let suffix = runtime
        .mint_exact_unchanged_suffix_witness(
            base,
            selection.convergence_line_start_byte() as usize,
            selection.convergence_line_start_utf16() as usize,
        )
        .expect("BOF suffix witness");
    let suffix = runtime
        .take_exact_unchanged_suffix_witness(suffix)
        .expect("fresh BOF suffix witness");
    let job = M11OrdinaryParagraphBofCropParseJob::new(
        plan,
        suffix,
        runtime.snapshot_current_source().expect("BOF crop lease"),
        binding(),
    )
    .expect("BOF crop starts");
    assert!(matches!(
        bof_crop(job),
        Err(M11OrdinaryParagraphBoundaryCropError::CropDiverged)
    ));
    close(runtime);
}

#[test]
fn bof_crop_matches_clean_rebases_checkpoints_and_supports_the_next_edit() {
    let source = paragraph_source(256);
    let removed_end = source.find("line-003-").expect("fourth base line");
    let replacement = "   new α😀 first\nnew second\nnew third\nnew fourth\nnew fifth\n";
    let changed = 0..removed_end;
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("base checkpoints");
    let selection = checkpoints
        .select_bof_crop(changed.clone())
        .expect("BOF selection");
    let base_convergence = &checkpoints.checkpoints()[selection.convergence_index()];
    let base_convergence_ordinal = base_convergence.next_physical_line_ordinal();
    let plan = M11OrdinaryParagraphBofCropPlan::new(checkpoints, selection).expect("BOF plan");
    let target_text = format!("{replacement}{}", &source[changed.end..]);
    let target = runtime
        .apply_edit(base, changed, replacement)
        .expect("BOF edit")
        .source()
        .current();
    let suffix = runtime
        .mint_exact_unchanged_suffix_witness(
            base,
            selection.convergence_line_start_byte() as usize,
            selection.convergence_line_start_utf16() as usize,
        )
        .expect("BOF suffix witness");
    let suffix = runtime
        .take_exact_unchanged_suffix_witness(suffix)
        .expect("fresh BOF suffix");
    let crop_job = M11OrdinaryParagraphBofCropParseJob::new(
        plan,
        suffix,
        runtime.snapshot_current_source().expect("BOF crop lease"),
        binding(),
    )
    .expect("BOF crop job");
    let mut cropped = bof_crop(crop_job).expect("BOF crop");
    let clean = parse(runtime.snapshot_current_source().expect("clean lease"));
    assert_eq!(cropped.terminal().kind(), clean.kind());
    assert_eq!(cropped.terminal().source_range(), clean.source_range());
    assert_eq!(
        cropped.terminal().visible_source(),
        Some(3..target.byte_len() as u32)
    );
    assert_eq!(cropped.terminal().visible_source(), clean.visible_source());
    assert_eq!(cropped.work().reused_prefix_checkpoints(), 0);
    assert!(cropped.work().reused_suffix_checkpoints() > 0);
    assert_eq!(
        cropped.work().convergence_ordinal_delta(),
        Some(2),
        "five replacement lines replaced three complete lines"
    );
    let crop_range = cropped.work().target_crop_bytes();
    assert_eq!(
        cropped.work().crop_source_bytes_discovered(),
        crop_range.len()
    );
    assert_eq!(cropped.work().crop_source_bytes_read(), crop_range.len());
    assert!(crop_range.len() < target_text.len() / 8);

    let next = cropped
        .take_next_restart_checkpoints()
        .expect("BOF target checkpoints");
    assert_eq!(next.source(), target);
    assert!(next
        .checkpoints()
        .iter()
        .all(|checkpoint| checkpoint.paragraph_content_start() == 3));
    let shifted_convergence = next
        .checkpoints()
        .iter()
        .find(|checkpoint| {
            i64::from(checkpoint.next_physical_line_ordinal())
                == i64::from(base_convergence_ordinal) + 2
        })
        .expect("shifted convergence checkpoint");
    assert_eq!(shifted_convergence.paragraph_content_start(), 3);

    // A second middle edit must be able to consume the target collection.
    // This catches stale base paragraph-content starts in reused suffix
    // checkpoints.
    let second_start = target_text.find("line-100-").expect("second edit line") + 24;
    let second_changed = second_start..second_start + 1;
    let second_selection = next
        .select_crop(second_changed.clone())
        .expect("second crop selection");
    let second_plan =
        M11OrdinaryParagraphCropPlan::new(next, second_selection).expect("second crop plan");
    let second_target_text = format!(
        "{}z{}",
        &target_text[..second_changed.start],
        &target_text[second_changed.end..]
    );
    let second = runtime
        .apply_edit(target, second_changed, "z")
        .expect("second edit")
        .source()
        .current();
    let second_prefix = runtime
        .mint_exact_unchanged_prefix_witness(
            target,
            second_selection.restart_prefix_end_byte() as usize,
            second_selection.restart_prefix_end_utf16() as usize,
        )
        .expect("second prefix witness");
    let second_prefix = runtime
        .take_exact_unchanged_prefix_witness(second_prefix)
        .expect("fresh second prefix");
    let second_suffix = runtime
        .mint_exact_unchanged_suffix_witness(
            target,
            second_selection.convergence_line_start_byte() as usize,
            second_selection.convergence_line_start_utf16() as usize,
        )
        .expect("second suffix witness");
    let second_suffix = runtime
        .take_exact_unchanged_suffix_witness(second_suffix)
        .expect("fresh second suffix");
    let second_job = M11OrdinaryParagraphCropParseJob::new(
        second_plan,
        second_prefix,
        second_suffix,
        runtime
            .snapshot_current_source()
            .expect("second crop lease"),
        binding(),
    )
    .expect("second crop job");
    let second_crop = crop(second_job).expect("second crop");
    let second_clean = parse(
        runtime
            .snapshot_current_source()
            .expect("second clean lease"),
    );
    assert_eq!(second_crop.terminal().source_version(), second);
    assert_eq!(
        second_crop.terminal().visible_source(),
        Some(3..second.byte_len() as u32)
    );
    assert_eq!(
        second_crop.terminal().visible_source(),
        second_clean.visible_source()
    );
    assert_eq!(second_target_text.len(), second.byte_len());

    close(runtime);
}

#[test]
fn eof_crop_accepts_a_unicode_crlf_bullet_list_becoming_terminal_empty() {
    let mut source = paragraph_source(248);
    let list_start = source.len();
    source.push_str("  - α😀\r\n  - β");
    let changed_start = source.rfind('β').expect("terminal item content");
    let changed = changed_start..source.len();
    let target_text = source[..changed_start].to_owned();
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("base checkpoints");
    let selection = checkpoints
        .select_eof_crop(changed.clone())
        .expect("EOF selection");
    let plan = M11OrdinaryParagraphEofCropPlan::new(checkpoints, selection).expect("EOF crop plan");
    let target = runtime
        .apply_edit(base, changed, "")
        .expect("empty terminal Bullet List item")
        .source()
        .current();
    let prefix = runtime
        .mint_exact_unchanged_prefix_witness(
            base,
            selection.restart_prefix_end_byte() as usize,
            selection.restart_prefix_end_utf16() as usize,
        )
        .expect("EOF prefix witness");
    let prefix = runtime
        .take_exact_unchanged_prefix_witness(prefix)
        .expect("fresh EOF prefix");
    let crop_job = M11OrdinaryParagraphEofCropParseJob::new(
        plan,
        prefix,
        runtime.snapshot_current_source().expect("EOF crop lease"),
        binding(),
    )
    .expect("EOF crop job");
    let cropped = eof_crop(crop_job).expect("EOF crop");
    let clean = parse(runtime.snapshot_current_source().expect("clean lease"));

    let Some(M11CleanLeaf::BulletList {
        source: list_source,
        source_utf16: list_source_utf16,
        items,
        projected_utf8_length,
        projected_utf16_length,
        tight,
        ..
    }) = clean.leaves().last()
    else {
        panic!("clean target must end in a Bullet List");
    };
    assert_eq!(
        list_source.clone(),
        list_start as u32..target_text.len() as u32
    );
    assert_eq!(
        list_source_utf16.clone(),
        list_start as u32..target_text.encode_utf16().count() as u32
    );
    assert_eq!(items.len(), 2);
    assert!(items[0].paragraph.is_some());
    assert!(items[1].paragraph.is_none());
    assert_eq!(*projected_utf8_length, 8);
    assert_eq!(*projected_utf16_length, 5);
    assert!(*tight);

    let crop_range = cropped.work().target_crop_bytes();
    assert_eq!(crop_range.end, target_text.len());
    assert!(crop_range.start <= list_start);
    assert!(crop_range.len() < target_text.len() / 8);
    let input = cropped
        .into_exact_segmented_candidate_input()
        .expect("authenticated segmented Bullet List crop");
    let candidate = M11ParserCandidate::derive_segmented_reusing_references(
        input,
        binding().syntax_profile(),
        SourceFactsScanProfile::new(64).expect("SourceFacts profile"),
    )
    .expect("derive exact Bullet List splice candidate");
    assert_eq!(candidate.source(), target);
    drop(candidate);
    close(runtime);
}

#[test]
fn eof_crop_matches_clean_with_unicode_and_line_count_shift() {
    let source = paragraph_source(256);
    let changed_start = source.find("line-248-").expect("tail start");
    let changed = changed_start..source.len();
    let mut replacement = String::new();
    for ordinal in 0..12 {
        replacement.push_str(&format!("tail-{ordinal:02}-世界😀-{}\n", "b".repeat(500)));
    }
    let target_text = format!("{}{}", &source[..changed.start], replacement);
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("base checkpoints");
    let selection = checkpoints
        .select_eof_crop(changed.clone())
        .expect("EOF selection");
    let plan = M11OrdinaryParagraphEofCropPlan::new(checkpoints, selection).expect("EOF plan");
    let target = runtime
        .apply_edit(base, changed, &replacement)
        .expect("EOF edit")
        .source()
        .current();
    let prefix = runtime
        .mint_exact_unchanged_prefix_witness(
            base,
            selection.restart_prefix_end_byte() as usize,
            selection.restart_prefix_end_utf16() as usize,
        )
        .expect("EOF prefix witness");
    let prefix = runtime
        .take_exact_unchanged_prefix_witness(prefix)
        .expect("fresh EOF prefix");
    let crop_job = M11OrdinaryParagraphEofCropParseJob::new(
        plan,
        prefix,
        runtime.snapshot_current_source().expect("EOF crop lease"),
        binding(),
    )
    .expect("EOF crop job");
    let mut cropped = eof_crop(crop_job).expect("EOF crop");
    let clean = parse(runtime.snapshot_current_source().expect("clean lease"));
    assert_eq!(cropped.terminal().source_version(), target);
    assert_eq!(cropped.terminal().kind(), clean.kind());
    assert_eq!(cropped.terminal().source_range(), clean.source_range());
    assert_eq!(cropped.terminal().visible_source(), clean.visible_source());
    assert!(cropped.work().reused_prefix_checkpoints() > 0);
    assert_eq!(cropped.work().reused_suffix_checkpoints(), 0);
    assert_eq!(cropped.work().convergence_ordinal_delta(), None);
    assert!(cropped.work().fresh_crop_checkpoints() > 0);
    let crop_range = cropped.work().target_crop_bytes();
    assert_eq!(
        cropped.work().crop_source_bytes_discovered(),
        crop_range.len()
    );
    assert_eq!(cropped.work().crop_source_bytes_read(), crop_range.len());
    assert!(crop_range.len() < target_text.len() / 8);

    let next = cropped
        .take_next_restart_checkpoints()
        .expect("EOF target checkpoints");
    assert_eq!(next.source(), target);
    for checkpoint in next.checkpoints() {
        let start = checkpoint.preceding_line_start_byte() as usize;
        let end = checkpoint.prefix_end_byte() as usize;
        assert_eq!(
            end - start,
            checkpoint.preceding_line_physical_bytes() as usize
        );
        assert_eq!(
            target_text[..start].encode_utf16().count(),
            checkpoint.preceding_line_start_utf16() as usize
        );
        assert_eq!(
            target_text[..end].encode_utf16().count(),
            checkpoint.prefix_end_utf16() as usize
        );
    }
    close(runtime);
}

#[test]
fn eof_crop_classifies_a_blank_split_as_semantic_divergence_and_remains_cancelable() {
    let source = paragraph_source(128);
    let changed_start = source.find("line-112-").expect("tail line") + 24;
    let changed = changed_start..source.len();
    let replacement = format!("\n\nsplit paragraph{}", &source[changed_start..]);
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("base checkpoints");
    let selection = checkpoints
        .select_eof_crop(changed.clone())
        .expect("EOF selection");
    let plan = M11OrdinaryParagraphEofCropPlan::new(checkpoints, selection).expect("EOF plan");
    runtime
        .apply_edit(base, changed, &replacement)
        .expect("blank split");
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
    let mut job = M11OrdinaryParagraphEofCropParseJob::new(
        plan,
        prefix,
        runtime.snapshot_current_source().expect("EOF crop lease"),
        binding(),
    )
    .expect("EOF crop starts");
    let error = loop {
        match job.poll(17) {
            Ok(M11OrdinaryParagraphBoundaryCropPoll::Pending { .. }) => {}
            Ok(M11OrdinaryParagraphBoundaryCropPoll::Complete { .. }) => {
                panic!("semantic blank split must not publish an EOF crop")
            }
            Err(error) => break error,
        }
    };
    assert!(matches!(
        error,
        M11OrdinaryParagraphBoundaryCropError::CropDiverged
    ));
    let restored = job
        .cancel_into_base_restart_checkpoints()
        .expect("semantic decline restores base checkpoints");
    assert_eq!(restored.source(), base);
    assert_eq!(restored.binding(), binding());
    close(runtime);
}

#[test]
fn edge_selection_leaves_a_whole_source_change_for_the_clean_lane() {
    let source = paragraph_source(48);
    let runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut result = parse(runtime.snapshot_current_source().expect("source lease"));
    let checkpoints = result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("checkpoints");
    assert!(matches!(
        checkpoints.select_bof_crop(0..source.len()),
        Err(M11OrdinaryParagraphBoundaryCropPlanError::WholeSourceIneligible)
    ));
    assert!(matches!(
        checkpoints.select_eof_crop(0..source.len()),
        Err(M11OrdinaryParagraphBoundaryCropPlanError::WholeSourceIneligible)
    ));
    close(runtime);
}

#[test]
fn segmented_top_level_boundary_selections_are_admitted_with_exact_topology() {
    let source = segmented_paragraph_source(128);
    let runtime = DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut result = parse(runtime.snapshot_current_source().expect("source lease"));
    let checkpoints = result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("segmented checkpoints");
    assert!(checkpoints.is_segmented_top_level());
    let selection = checkpoints
        .select_bof_crop(0..1)
        .expect("segmented BOF selection");
    assert!(selection.is_segmented_top_level());
    assert_eq!(
        selection.convergence_suffix_start_byte(),
        checkpoints.checkpoints()[selection.convergence_index()].paragraph_source_start_byte()
    );
    assert!(selection.convergence_block_entry_ordinal() > 0);
    assert_eq!(
        selection.base_block_entry_count(),
        checkpoints.top_level_block_count()
    );
    let selection = checkpoints
        .select_eof_crop(source.len() - 1..source.len())
        .expect("segmented EOF selection");
    assert!(selection.is_segmented_top_level());
    assert_eq!(
        selection.base_block_entry_count(),
        checkpoints.top_level_block_count()
    );
    assert!(selection.restart_block_entry_ordinal() < selection.base_block_entry_count());
    close(runtime);
}

#[test]
fn segmented_bof_crop_tracks_length_and_block_count_changes() {
    let source = segmented_paragraph_source(512);
    let length_start =
        source.find("paragraph-0000-").expect("first paragraph") + "paragraph-0000-".len();
    let split_start = source
        .find("continuation-0000-")
        .expect("first continuation")
        + "continuation-0000-".len();
    let merge_start = source
        .find("\n\nparagraph-0001-")
        .expect("first paragraph separator")
        + 1;
    let cases = [
        (
            "length",
            length_start..length_start + 1,
            "expanded-α",
            0_i64,
        ),
        ("split", split_start..split_start, "\n\nsplit-", 2_i64),
        ("merge", merge_start..merge_start + 1, "", -2_i64),
    ];

    for (name, changed, replacement, expected_block_delta) in cases {
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect(name);
        let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
        let base = base_result.source_version();
        let checkpoints = base_result
            .take_ordinary_paragraph_restart_checkpoints(binding())
            .expect("segmented checkpoints");
        let selection = checkpoints
            .select_bof_crop(0..changed.end)
            .expect("segmented BOF selection");
        assert!(selection.is_segmented_top_level());
        let base_convergence_block = selection.convergence_block_entry_ordinal();
        let plan =
            M11OrdinaryParagraphBofCropPlan::new(checkpoints, selection).expect("BOF crop plan");

        let target = runtime
            .apply_edit(base, changed, replacement)
            .expect("BOF edit")
            .source()
            .current();
        let suffix = runtime
            .mint_exact_unchanged_suffix_witness(
                base,
                selection.convergence_suffix_start_byte() as usize,
                selection.convergence_suffix_start_utf16() as usize,
            )
            .expect("paragraph-opening suffix witness");
        let target_convergence_start = suffix.target_byte_start();
        let suffix = runtime
            .take_exact_unchanged_suffix_witness(suffix)
            .expect("fresh suffix witness");
        let job = M11OrdinaryParagraphBofCropParseJob::new(
            plan,
            suffix,
            runtime.snapshot_current_source().expect("BOF crop lease"),
            binding(),
        )
        .expect("segmented BOF crop job");
        let mut cropped = bof_crop(job).expect(name);
        assert!(
            cropped.work().crop_source_bytes_discovered() <= 16 * 1024,
            "{name} BOF crop was not bounded"
        );
        assert_eq!(cropped.work().reused_prefix_checkpoints(), 0);
        assert!(cropped.work().reused_suffix_checkpoints() > 0);

        let next = cropped
            .take_next_restart_checkpoints()
            .expect("target checkpoints");
        assert_eq!(next.source(), target);
        assert!(next.is_segmented_top_level());
        let converged = next
            .checkpoints()
            .iter()
            .find(|checkpoint| {
                checkpoint.paragraph_source_start_byte() as usize == target_convergence_start
            })
            .expect("shifted convergence Paragraph");
        assert_eq!(
            i64::try_from(converged.block_entry_ordinal()).expect("target block ordinal"),
            i64::try_from(base_convergence_block).expect("base block ordinal")
                + expected_block_delta,
            "{name}"
        );
        cropped
            .into_exact_segmented_candidate_input()
            .expect("segmented BOF candidate input");
        close(runtime);
    }
}

#[test]
fn segmented_eof_crop_tracks_length_and_block_count_changes() {
    let source = segmented_paragraph_source(512);
    let length_start =
        source.find("paragraph-0510-").expect("tail paragraph") + "paragraph-0510-".len();
    let split_start = source
        .find("continuation-0510-")
        .expect("tail continuation")
        + "continuation-0510-".len();
    let merge_start = source
        .find("\n\nparagraph-0511-")
        .expect("tail paragraph separator")
        + 1;
    let cases = [
        (
            "length",
            length_start..length_start + 1,
            "expanded-α",
            0_i64,
        ),
        ("split", split_start..split_start, "\n\nsplit-", 2_i64),
        ("merge", merge_start..merge_start + 1, "", -2_i64),
    ];

    for (name, changed, replacement, expected_block_delta) in cases {
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect(name);
        let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
        let base = base_result.source_version();
        let checkpoints = base_result
            .take_ordinary_paragraph_restart_checkpoints(binding())
            .expect("segmented checkpoints");
        let base_block_count = checkpoints.top_level_block_count();
        let selection = checkpoints
            .select_eof_crop(changed.start..source.len())
            .expect("segmented EOF selection");
        assert!(selection.is_segmented_top_level());
        assert_eq!(selection.base_block_entry_count(), base_block_count);
        let restart_index = selection.restart_index();
        let restart_prefix_end = selection.restart_prefix_end_byte();
        let restart_block_ordinal = selection.restart_block_entry_ordinal();
        let plan =
            M11OrdinaryParagraphEofCropPlan::new(checkpoints, selection).expect("EOF crop plan");

        let target = runtime
            .apply_edit(base, changed, replacement)
            .expect("EOF edit")
            .source()
            .current();
        let prefix = runtime
            .mint_exact_unchanged_prefix_witness(
                base,
                selection.restart_prefix_end_byte() as usize,
                selection.restart_prefix_end_utf16() as usize,
            )
            .expect("EOF prefix witness");
        let prefix = runtime
            .take_exact_unchanged_prefix_witness(prefix)
            .expect("fresh EOF prefix");
        let job = M11OrdinaryParagraphEofCropParseJob::new(
            plan,
            prefix,
            runtime.snapshot_current_source().expect("EOF crop lease"),
            binding(),
        )
        .expect("segmented EOF crop job");
        let mut cropped = eof_crop(job).expect(name);
        assert!(
            cropped.work().crop_source_bytes_discovered() <= 16 * 1024,
            "{name} EOF crop was not bounded"
        );
        assert!(cropped.work().reused_prefix_checkpoints() > 0);
        assert_eq!(cropped.work().reused_suffix_checkpoints(), 0);

        let next = cropped
            .take_next_restart_checkpoints()
            .expect("target checkpoints");
        let expected_block_count =
            u64::try_from(i128::from(base_block_count) + i128::from(expected_block_delta))
                .expect("target block count");
        assert_eq!(next.source(), target);
        assert_eq!(next.top_level_block_count(), expected_block_count, "{name}");
        assert!(next.is_segmented_top_level());
        assert!(next
            .checkpoints()
            .iter()
            .all(|checkpoint| checkpoint.block_entry_ordinal() < expected_block_count));
        let retained_restart = &next.checkpoints()[restart_index];
        assert_eq!(
            retained_restart.prefix_end_byte(),
            restart_prefix_end,
            "{name}"
        );
        assert_eq!(
            retained_restart.block_entry_ordinal(),
            restart_block_ordinal,
            "{name}"
        );
        cropped
            .into_exact_segmented_candidate_input()
            .expect("segmented EOF candidate input");
        close(runtime);
    }
}

#[test]
fn thematic_break_transition_in_4096_blocks_stays_in_one_bounded_parser_crop() {
    const PARAGRAPHS: usize = 4_096;
    const PARAGRAPH_ORDINAL: usize = PARAGRAPHS / 2;
    for promote in [true, false] {
        let (source, changed, replacement) =
            thematic_transition_fixture(PARAGRAPHS, PARAGRAPH_ORDINAL, promote);
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
        let base = base_result.source_version();
        let checkpoints = base_result
            .take_ordinary_paragraph_restart_checkpoints(binding())
            .expect("segmented checkpoints");
        let base_block_count = checkpoints.top_level_block_count();
        let selection = checkpoints
            .select_crop(changed.clone())
            .expect("interior selection");
        let edited_block_ordinal = u64::try_from(PARAGRAPH_ORDINAL * 2).expect("block ordinal");
        assert!(selection.restart_block_entry_ordinal() < edited_block_ordinal);
        assert!(selection.convergence_block_entry_ordinal() > edited_block_ordinal);
        let plan =
            M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("interior plan");
        let target = runtime
            .apply_edit(base, changed, &replacement)
            .expect("transition edit")
            .source()
            .current();
        let prefix = runtime
            .mint_exact_unchanged_prefix_witness(
                base,
                selection.restart_prefix_end_byte() as usize,
                selection.restart_prefix_end_utf16() as usize,
            )
            .expect("prefix witness");
        let prefix = runtime
            .take_exact_unchanged_prefix_witness(prefix)
            .expect("fresh prefix");
        let suffix = runtime
            .mint_exact_unchanged_suffix_witness(
                base,
                selection.convergence_suffix_start_byte() as usize,
                selection.convergence_suffix_start_utf16() as usize,
            )
            .expect("suffix witness");
        let suffix = runtime
            .take_exact_unchanged_suffix_witness(suffix)
            .expect("fresh suffix");
        let job = M11OrdinaryParagraphCropParseJob::new(
            plan,
            prefix,
            suffix,
            runtime.snapshot_current_source().expect("crop lease"),
            binding(),
        )
        .expect("interior crop");
        let mut cropped = crop(job).expect("thematic transition crop");
        let work = cropped.work();
        assert_eq!(
            work.crop_source_bytes_discovered(),
            work.target_crop_bytes().len()
        );
        assert_eq!(
            work.crop_source_bytes_read(),
            work.target_crop_bytes().len()
        );
        assert!(
            work.crop_source_bytes_discovered() <= 16 * 1024,
            "bounded thematic crop discovered {} of {} bytes",
            work.crop_source_bytes_discovered(),
            source.len(),
        );
        assert!(work.crop_physical_lines_discovered() <= 512);
        assert!(work.crop_parser_transitions() <= 4_096);

        let clean = parse(runtime.snapshot_current_source().expect("clean lease"));
        assert_eq!(
            clean
                .leaves()
                .iter()
                .any(|leaf| matches!(leaf, M11CleanLeaf::ThematicBreak { .. })),
            promote,
        );
        let next = cropped
            .take_next_restart_checkpoints()
            .expect("target checkpoints");
        assert_eq!(next.source(), target);
        assert_eq!(next.top_level_block_count(), base_block_count);
        assert!(next.is_segmented_top_level());
        assert!(next
            .checkpoints()
            .iter()
            .all(|checkpoint| checkpoint.block_entry_ordinal() < base_block_count));
        cropped
            .into_exact_segmented_candidate_input()
            .expect("interior segmented input");
        close(runtime);
    }
}

#[test]
fn indented_code_transition_in_4096_blocks_stays_in_one_bounded_parser_crop() {
    const PARAGRAPHS: usize = 4_096;
    const PARAGRAPH_ORDINAL: usize = PARAGRAPHS / 2;
    for promote in [true, false] {
        let (source, changed, replacement) =
            indented_code_transition_fixture(PARAGRAPHS, PARAGRAPH_ORDINAL, promote);
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
        let base = base_result.source_version();
        let checkpoints = base_result
            .take_ordinary_paragraph_restart_checkpoints(binding())
            .expect("segmented checkpoints");
        let base_block_count = checkpoints.top_level_block_count();
        let selection = checkpoints
            .select_crop(changed.clone())
            .expect("interior selection");
        let edited_block_ordinal = u64::try_from(PARAGRAPH_ORDINAL * 2).expect("block ordinal");
        assert!(selection.restart_block_entry_ordinal() < edited_block_ordinal);
        assert!(selection.convergence_block_entry_ordinal() > edited_block_ordinal);
        let plan =
            M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("interior plan");
        let target = runtime
            .apply_edit(base, changed, &replacement)
            .expect("transition edit")
            .source()
            .current();
        let prefix = runtime
            .mint_exact_unchanged_prefix_witness(
                base,
                selection.restart_prefix_end_byte() as usize,
                selection.restart_prefix_end_utf16() as usize,
            )
            .expect("prefix witness");
        let prefix = runtime
            .take_exact_unchanged_prefix_witness(prefix)
            .expect("fresh prefix");
        let suffix = runtime
            .mint_exact_unchanged_suffix_witness(
                base,
                selection.convergence_suffix_start_byte() as usize,
                selection.convergence_suffix_start_utf16() as usize,
            )
            .expect("suffix witness");
        let suffix = runtime
            .take_exact_unchanged_suffix_witness(suffix)
            .expect("fresh suffix");
        let job = M11OrdinaryParagraphCropParseJob::new(
            plan,
            prefix,
            suffix,
            runtime.snapshot_current_source().expect("crop lease"),
            binding(),
        )
        .expect("interior crop");
        let mut cropped = crop(job).expect("indented-code transition crop");
        let work = cropped.work();
        assert_eq!(
            work.crop_source_bytes_discovered(),
            work.target_crop_bytes().len()
        );
        assert_eq!(
            work.crop_source_bytes_read(),
            work.target_crop_bytes().len()
        );
        assert!(
            work.crop_source_bytes_discovered() <= 16 * 1024,
            "bounded indented-code crop discovered {} of {} bytes",
            work.crop_source_bytes_discovered(),
            source.len(),
        );
        assert!(work.crop_physical_lines_discovered() <= 512);
        assert!(work.crop_parser_transitions() <= 4_096);

        let clean = parse(runtime.snapshot_current_source().expect("clean lease"));
        assert_eq!(
            clean
                .leaves()
                .iter()
                .any(|leaf| matches!(leaf, M11CleanLeaf::IndentedCode { .. })),
            promote,
        );
        let next = cropped
            .take_next_restart_checkpoints()
            .expect("target checkpoints");
        assert_eq!(next.source(), target);
        assert_eq!(next.top_level_block_count(), base_block_count);
        assert!(next.is_segmented_top_level());
        assert!(next
            .checkpoints()
            .iter()
            .all(|checkpoint| checkpoint.block_entry_ordinal() < base_block_count));
        cropped
            .into_exact_segmented_candidate_input()
            .expect("interior segmented input");
        close(runtime);
    }
}

#[test]
fn thematic_break_interruption_splits_and_merges_paragraph_topology_by_two() {
    const PARAGRAPHS: usize = 1_024;
    const PARAGRAPH_ORDINAL: usize = PARAGRAPHS / 2;
    for split in [true, false] {
        let mut source = segmented_paragraph_source(PARAGRAPHS);
        let continuation = format!("continuation-{PARAGRAPH_ORDINAL:04}-");
        let insertion = source.find(&continuation).expect("middle continuation");
        let (changed, replacement, expected_delta) = if split {
            (insertion..insertion, "***\n", 2_i64)
        } else {
            source.insert_str(insertion, "***\n");
            (insertion..insertion + 4, "", -2_i64)
        };

        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
        let base = base_result.source_version();
        let checkpoints = base_result
            .take_ordinary_paragraph_restart_checkpoints(binding())
            .expect("segmented checkpoints");
        let base_block_count = checkpoints.top_level_block_count();
        let selection = checkpoints
            .select_crop(changed.clone())
            .expect("interior selection");
        let plan =
            M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("interior plan");
        runtime
            .apply_edit(base, changed, replacement)
            .expect("interruption edit");
        let prefix = runtime
            .mint_exact_unchanged_prefix_witness(
                base,
                selection.restart_prefix_end_byte() as usize,
                selection.restart_prefix_end_utf16() as usize,
            )
            .expect("prefix witness");
        let prefix = runtime
            .take_exact_unchanged_prefix_witness(prefix)
            .expect("fresh prefix");
        let suffix = runtime
            .mint_exact_unchanged_suffix_witness(
                base,
                selection.convergence_suffix_start_byte() as usize,
                selection.convergence_suffix_start_utf16() as usize,
            )
            .expect("suffix witness");
        let suffix = runtime
            .take_exact_unchanged_suffix_witness(suffix)
            .expect("fresh suffix");
        let job = M11OrdinaryParagraphCropParseJob::new(
            plan,
            prefix,
            suffix,
            runtime.snapshot_current_source().expect("crop lease"),
            binding(),
        )
        .expect("interior crop");
        let mut cropped = crop(job).expect("thematic interruption crop");
        assert!(cropped.work().crop_source_bytes_discovered() <= 16 * 1024);
        let expected_block_count =
            u64::try_from(i128::from(base_block_count) + i128::from(expected_delta))
                .expect("target block count");
        let next = cropped
            .take_next_restart_checkpoints()
            .expect("target checkpoints");
        assert_eq!(next.top_level_block_count(), expected_block_count);
        assert!(next
            .checkpoints()
            .iter()
            .all(|checkpoint| checkpoint.block_entry_ordinal() < expected_block_count));
        let clean = parse(runtime.snapshot_current_source().expect("clean lease"));
        assert_eq!(
            clean
                .leaves()
                .iter()
                .any(|leaf| matches!(leaf, M11CleanLeaf::ThematicBreak { .. })),
            split,
        );
        cropped
            .into_exact_segmented_candidate_input()
            .expect("interior segmented input");
        close(runtime);
    }
}

#[test]
fn segmented_interior_crop_transitions_paragraph_and_setext_both_directions() {
    const PARAGRAPH_ORDINAL: usize = 510;
    for promote in [true, false] {
        let (source, changed, replacement) =
            setext_transition_fixture(1024, PARAGRAPH_ORDINAL, promote);
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
        let base = base_result.source_version();
        let checkpoints = base_result
            .take_ordinary_paragraph_restart_checkpoints(binding())
            .expect("segmented checkpoints");
        let base_block_count = checkpoints.top_level_block_count();
        let selection = checkpoints
            .select_crop(changed.clone())
            .expect("interior selection");
        let edited_block_ordinal = u64::try_from(PARAGRAPH_ORDINAL * 2).expect("block ordinal");
        assert!(selection.restart_block_entry_ordinal() < edited_block_ordinal);
        assert!(selection.convergence_block_entry_ordinal() > edited_block_ordinal);
        let plan =
            M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("interior plan");
        runtime
            .apply_edit(base, changed, &replacement)
            .expect("transition edit");
        let prefix = runtime
            .mint_exact_unchanged_prefix_witness(
                base,
                selection.restart_prefix_end_byte() as usize,
                selection.restart_prefix_end_utf16() as usize,
            )
            .expect("prefix witness");
        let prefix = runtime
            .take_exact_unchanged_prefix_witness(prefix)
            .expect("fresh prefix");
        let suffix = runtime
            .mint_exact_unchanged_suffix_witness(
                base,
                selection.convergence_suffix_start_byte() as usize,
                selection.convergence_suffix_start_utf16() as usize,
            )
            .expect("suffix witness");
        let suffix = runtime
            .take_exact_unchanged_suffix_witness(suffix)
            .expect("fresh suffix");
        let job = M11OrdinaryParagraphCropParseJob::new(
            plan,
            prefix,
            suffix,
            runtime.snapshot_current_source().expect("crop lease"),
            binding(),
        )
        .expect("interior crop");
        let mut cropped = crop(job).expect("transition crop");
        assert!(cropped.work().crop_source_bytes_discovered() <= 16 * 1024);
        let clean = parse(runtime.snapshot_current_source().expect("clean lease"));
        assert_eq!(
            clean
                .leaves()
                .iter()
                .any(|leaf| matches!(leaf, M11CleanLeaf::SetextHeading { .. })),
            promote
        );
        let next = cropped
            .take_next_restart_checkpoints()
            .expect("target checkpoints");
        assert_eq!(next.top_level_block_count(), base_block_count);
        assert!(next
            .checkpoints()
            .iter()
            .all(|checkpoint| checkpoint.block_entry_ordinal() < base_block_count));
        cropped
            .into_exact_segmented_candidate_input()
            .expect("interior segmented input");
        close(runtime);
    }
}

#[test]
fn segmented_bof_crop_transitions_paragraph_and_setext_both_directions() {
    for promote in [true, false] {
        let (source, changed, replacement) = setext_transition_fixture(512, 0, promote);
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
        let base = base_result.source_version();
        let checkpoints = base_result
            .take_ordinary_paragraph_restart_checkpoints(binding())
            .expect("segmented checkpoints");
        let base_block_count = checkpoints.top_level_block_count();
        let selection = checkpoints
            .select_bof_crop(0..changed.end)
            .expect("BOF selection");
        let plan = M11OrdinaryParagraphBofCropPlan::new(checkpoints, selection).expect("BOF plan");
        runtime
            .apply_edit(base, changed, &replacement)
            .expect("transition edit");
        let suffix = runtime
            .mint_exact_unchanged_suffix_witness(
                base,
                selection.convergence_suffix_start_byte() as usize,
                selection.convergence_suffix_start_utf16() as usize,
            )
            .expect("suffix witness");
        let suffix = runtime
            .take_exact_unchanged_suffix_witness(suffix)
            .expect("fresh suffix");
        let job = M11OrdinaryParagraphBofCropParseJob::new(
            plan,
            suffix,
            runtime.snapshot_current_source().expect("crop lease"),
            binding(),
        )
        .expect("BOF crop");
        let mut cropped = bof_crop(job).expect("transition crop");
        assert!(cropped.work().crop_source_bytes_discovered() <= 16 * 1024);
        let clean = parse(runtime.snapshot_current_source().expect("clean lease"));
        assert_eq!(
            clean
                .leaves()
                .iter()
                .any(|leaf| matches!(leaf, M11CleanLeaf::SetextHeading { .. })),
            promote
        );
        let next = cropped
            .take_next_restart_checkpoints()
            .expect("target checkpoints");
        assert_eq!(next.top_level_block_count(), base_block_count);
        assert!(next
            .checkpoints()
            .iter()
            .all(|checkpoint| checkpoint.block_entry_ordinal() < base_block_count));
        cropped
            .into_exact_segmented_candidate_input()
            .expect("BOF segmented input");
        close(runtime);
    }
}

#[test]
fn segmented_eof_crop_transitions_paragraph_and_setext_both_directions() {
    const PARAGRAPH_ORDINAL: usize = 511;
    for promote in [true, false] {
        let (source, changed, replacement) =
            setext_transition_fixture(512, PARAGRAPH_ORDINAL, promote);
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
        let base = base_result.source_version();
        let checkpoints = base_result
            .take_ordinary_paragraph_restart_checkpoints(binding())
            .expect("segmented checkpoints");
        let base_block_count = checkpoints.top_level_block_count();
        let selection = checkpoints
            .select_eof_crop(changed.start..source.len())
            .expect("EOF selection");
        let edited_block_ordinal = u64::try_from(PARAGRAPH_ORDINAL * 2).expect("block ordinal");
        assert!(selection.restart_block_entry_ordinal() < edited_block_ordinal);
        let plan = M11OrdinaryParagraphEofCropPlan::new(checkpoints, selection).expect("EOF plan");
        runtime
            .apply_edit(base, changed, &replacement)
            .expect("transition edit");
        let prefix = runtime
            .mint_exact_unchanged_prefix_witness(
                base,
                selection.restart_prefix_end_byte() as usize,
                selection.restart_prefix_end_utf16() as usize,
            )
            .expect("prefix witness");
        let prefix = runtime
            .take_exact_unchanged_prefix_witness(prefix)
            .expect("fresh prefix");
        let job = M11OrdinaryParagraphEofCropParseJob::new(
            plan,
            prefix,
            runtime.snapshot_current_source().expect("crop lease"),
            binding(),
        )
        .expect("EOF crop");
        let mut cropped = eof_crop(job).expect("transition crop");
        assert!(cropped.work().crop_source_bytes_discovered() <= 16 * 1024);
        let clean = parse(runtime.snapshot_current_source().expect("clean lease"));
        assert_eq!(
            clean
                .leaves()
                .iter()
                .any(|leaf| matches!(leaf, M11CleanLeaf::SetextHeading { .. })),
            promote
        );
        let next = cropped
            .take_next_restart_checkpoints()
            .expect("target checkpoints");
        assert_eq!(next.top_level_block_count(), base_block_count);
        assert!(next
            .checkpoints()
            .iter()
            .all(|checkpoint| checkpoint.block_entry_ordinal() < base_block_count));
        cropped
            .into_exact_segmented_candidate_input()
            .expect("EOF segmented input");
        close(runtime);
    }
}

#[test]
fn same_block_restart_declines_a_large_paragraph_to_setext_promotion() {
    let long_paragraph = paragraph_source(24);

    let interior_source = format!("before\n\n{long_paragraph}\n{long_paragraph}");
    let changed_start = interior_source.find("line-023-").expect("promoted line");
    let changed_end = interior_source[changed_start..]
        .find('\n')
        .map(|offset| changed_start + offset)
        .expect("line ending");
    let changed = changed_start..changed_end;
    let mut runtime = DocumentRuntime::new(&interior_source, DocumentRuntimeConfig::default())
        .expect("interior runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("interior checkpoints");
    let selection = checkpoints
        .select_crop(changed.clone())
        .expect("interior selection");
    assert_eq!(selection.restart_block_entry_ordinal(), 2);
    let plan = M11OrdinaryParagraphCropPlan::new(checkpoints, selection).expect("interior plan");
    runtime
        .apply_edit(base, changed, "---")
        .expect("interior promotion");
    let prefix = runtime
        .mint_exact_unchanged_prefix_witness(
            base,
            selection.restart_prefix_end_byte() as usize,
            selection.restart_prefix_end_utf16() as usize,
        )
        .expect("prefix witness");
    let prefix = runtime
        .take_exact_unchanged_prefix_witness(prefix)
        .expect("fresh prefix");
    let suffix = runtime
        .mint_exact_unchanged_suffix_witness(
            base,
            selection.convergence_suffix_start_byte() as usize,
            selection.convergence_suffix_start_utf16() as usize,
        )
        .expect("suffix witness");
    let suffix = runtime
        .take_exact_unchanged_suffix_witness(suffix)
        .expect("fresh suffix");
    let job = M11OrdinaryParagraphCropParseJob::new(
        plan,
        prefix,
        suffix,
        runtime.snapshot_current_source().expect("crop lease"),
        binding(),
    )
    .expect("interior crop");
    assert!(matches!(
        crop(job),
        Err(M11OrdinaryParagraphCropError::CropDiverged)
    ));
    close(runtime);

    let eof_source = format!("before\n\n{long_paragraph}");
    let changed_start = eof_source.rfind("line-023-").expect("final promoted line");
    let changed_end = eof_source[changed_start..]
        .find('\n')
        .map(|offset| changed_start + offset)
        .expect("line ending");
    let changed = changed_start..changed_end;
    let mut runtime =
        DocumentRuntime::new(&eof_source, DocumentRuntimeConfig::default()).expect("EOF runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("EOF checkpoints");
    let selection = checkpoints
        .select_eof_crop(changed.start..eof_source.len())
        .expect("EOF selection");
    assert_eq!(selection.restart_block_entry_ordinal(), 2);
    let plan = M11OrdinaryParagraphEofCropPlan::new(checkpoints, selection).expect("EOF plan");
    runtime
        .apply_edit(base, changed, "---")
        .expect("EOF promotion");
    let prefix = runtime
        .mint_exact_unchanged_prefix_witness(
            base,
            selection.restart_prefix_end_byte() as usize,
            selection.restart_prefix_end_utf16() as usize,
        )
        .expect("prefix witness");
    let prefix = runtime
        .take_exact_unchanged_prefix_witness(prefix)
        .expect("fresh prefix");
    let job = M11OrdinaryParagraphEofCropParseJob::new(
        plan,
        prefix,
        runtime.snapshot_current_source().expect("crop lease"),
        binding(),
    )
    .expect("EOF crop");
    assert!(matches!(
        eof_crop(job),
        Err(M11OrdinaryParagraphBoundaryCropError::CropDiverged)
    ));
    close(runtime);
}

#[test]
fn reference_frozen_segmented_eof_crop_rejects_a_new_definition() {
    let source = reference_prefixed_segmented_source();
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("reference-frozen checkpoints");
    assert_eq!(checkpoints.frozen_reference_definition_count(), Some(2));

    let changed_start = source
        .find("paragraph-0254-")
        .expect("final tail paragraph");
    let changed_end = source[changed_start..]
        .find('\n')
        .map(|offset| changed_start + offset + 1)
        .expect("tail paragraph line");
    let selection = checkpoints
        .select_eof_crop(changed_start..source.len())
        .expect("reference-frozen EOF selection");
    let plan = M11OrdinaryParagraphEofCropPlan::new(checkpoints, selection).expect("EOF crop plan");
    runtime
        .apply_edit(base, changed_start..changed_end, "[new]: /fresh\n")
        .expect("new definition edit");
    let prefix = runtime
        .mint_exact_unchanged_prefix_witness(
            base,
            selection.restart_prefix_end_byte() as usize,
            selection.restart_prefix_end_utf16() as usize,
        )
        .expect("EOF prefix witness");
    let prefix = runtime
        .take_exact_unchanged_prefix_witness(prefix)
        .expect("fresh EOF prefix");
    let job = M11OrdinaryParagraphEofCropParseJob::new(
        plan,
        prefix,
        runtime.snapshot_current_source().expect("EOF crop lease"),
        binding(),
    )
    .expect("reference-frozen EOF crop job");
    assert!(matches!(
        eof_crop(job),
        Err(M11OrdinaryParagraphBoundaryCropError::CropDiverged)
    ));
    let clean = parse(
        runtime
            .snapshot_current_source()
            .expect("clean target lease"),
    );
    assert_eq!(clean.definition_count(), 3);
    close(runtime);
}

#[test]
fn bof_crop_rejects_an_exact_suffix_that_is_not_a_target_line_boundary() {
    let source = paragraph_source(96);
    let mut runtime =
        DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
    let mut base_result = parse(runtime.snapshot_current_source().expect("base lease"));
    let base = base_result.source_version();
    let checkpoints = base_result
        .take_ordinary_paragraph_restart_checkpoints(binding())
        .expect("base checkpoints");
    let convergence_start = checkpoints.checkpoints()[3].preceding_line_start_byte() as usize;
    let changed = 0..convergence_start;
    let selection = checkpoints
        .select_bof_crop(changed.clone())
        .expect("BOF selection");
    let plan = M11OrdinaryParagraphBofCropPlan::new(checkpoints, selection).expect("BOF plan");
    runtime
        .apply_edit(base, changed, "joined prefix without a line ending ")
        .expect("BOF join edit");
    let suffix = runtime
        .mint_exact_unchanged_suffix_witness(
            base,
            selection.convergence_line_start_byte() as usize,
            selection.convergence_line_start_utf16() as usize,
        )
        .expect("suffix text remains exact");
    let suffix = runtime
        .take_exact_unchanged_suffix_witness(suffix)
        .expect("fresh suffix witness");
    let job = M11OrdinaryParagraphBofCropParseJob::new(
        plan,
        suffix,
        runtime.snapshot_current_source().expect("BOF crop lease"),
        binding(),
    )
    .expect("bounded BOF job");
    assert!(matches!(
        bof_crop(job),
        Err(M11OrdinaryParagraphBoundaryCropError::ConvergenceMismatch)
    ));
    close(runtime);
}

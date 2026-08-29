use flark_engine::m11_host::M11_CANDIDATE_ARENA_MAX_SLOTS;
use flark_engine::parser_internal::{
    M11CandidatePublication, M11OwnedSnapshotPoll, M11RetainedCandidatePublication,
    M11SnapshotFrameKind, M11_MAX_ROLE_RECORDS,
};
use flark_engine::{
    ArenaLimits, CertifiedSource, DocumentRuntime, DocumentRuntimeConfig, DocumentRuntimeError,
    ParserProfileId, RuntimeSourceFactsPoll, SourceFactsRootLimits, SourceFactsScanProfile,
    SourceSnapshotLease, SourceStore,
};
use flark_parser::{
    LeadingReferencesCheckpointError, LeadingReferencesRestartCheckpoint,
    M11CandidateDerivationError, M11CleanDocumentKind, M11CleanDocumentResult, M11CleanParseJob,
    M11CleanParsePoll, M11LeadingReferencesCropError, M11LeadingReferencesCropParseJob,
    M11LeadingReferencesCropPoll, M11LeadingReferencesCropResult, M11ParserBinding,
    M11ParserCandidate, M11ParserCandidateWriterPoll, M11ParserTerminalFacts,
};

const PROFILE: u64 = 17;
const DOCUMENT: [u8; 16] = [0x31; 16];

fn binding() -> M11ParserBinding {
    M11ParserBinding::current(ParserProfileId::new(PROFILE).expect("parser profile"))
}

fn close(mut runtime: DocumentRuntime) {
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(64).expect("close poll").complete {}
}

fn certify(runtime: &mut DocumentRuntime) -> CertifiedSource {
    runtime
        .begin_source_facts(
            SourceFactsScanProfile::new(64).expect("source-facts profile"),
            binding().syntax_profile(),
            SourceFactsRootLimits::default(),
        )
        .expect("begin SourceFacts");
    loop {
        match runtime
            .poll_source_facts(64 * 1024, 64)
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
                panic!("clean certification reported incremental progress");
            }
        }
    }
}

fn parse(lease: SourceSnapshotLease) -> M11CleanDocumentResult {
    let mut parse = M11CleanParseJob::new(lease).expect("clean parse");
    loop {
        match parse.poll(64).expect("clean parse poll") {
            M11CleanParsePoll::Pending { .. } => {}
            M11CleanParsePoll::Complete { result, .. } => return result,
        }
    }
}

fn crop_with_fuel(
    mut crop: M11LeadingReferencesCropParseJob,
    fuel: usize,
) -> (usize, M11LeadingReferencesCropResult) {
    let mut transitions = 0;
    loop {
        match crop.poll(fuel).expect("crop poll") {
            M11LeadingReferencesCropPoll::Pending {
                transitions: consumed,
            } => transitions += consumed,
            M11LeadingReferencesCropPoll::Complete {
                transitions: consumed,
                result,
            } => return (transitions + consumed, result),
        }
    }
}

fn crop(crop: M11LeadingReferencesCropParseJob) -> (usize, M11LeadingReferencesCropResult) {
    crop_with_fuel(crop, 64)
}

fn checkpoint(result: &mut M11CleanDocumentResult) -> LeadingReferencesRestartCheckpoint {
    result
        .take_leading_references_restart_checkpoint(binding())
        .expect("eligible leading-reference checkpoint")
}

fn publish_candidate(
    runtime: &mut DocumentRuntime,
    candidate: M11ParserCandidate,
    publication: [u8; 16],
    generation: u64,
) -> Box<M11CandidatePublication> {
    let mut writer = candidate
        .into_writer(runtime, DOCUMENT, publication, generation)
        .expect("candidate writer");
    loop {
        match writer.poll(runtime, 64).expect("candidate writer poll") {
            M11ParserCandidateWriterPoll::Pending { .. } => {}
            M11ParserCandidateWriterPoll::Published { publication, .. } => return publication,
        }
    }
}

fn derive_candidate(
    certified: CertifiedSource,
    result: M11CleanDocumentResult,
) -> M11ParserCandidate {
    M11ParserCandidate::derive_segmented(certified, result).expect("segmented parser candidate")
}

fn retain_publication(
    runtime: &DocumentRuntime,
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
        match stream.poll(runtime, 64).expect("snapshot poll") {
            M11OwnedSnapshotPoll::Pending { .. } => {}
            M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                panic!("full snapshot requested exact-base replay")
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
    publication
        .begin_close(runtime)
        .expect("begin retained close");
    while !publication
        .poll_close(runtime, 1)
        .expect("retained close poll")
    {}
}

#[test]
fn crop_matches_clean_terminal_green_and_projection_after_unicode_tail_edit() {
    let source = "[é]: /世界\r\n[b]: /two\r\nvisible 😀\n";
    let prefix_end = source.find("visible").expect("visible tail");
    let prefix_utf16 = source[..prefix_end].encode_utf16().count();
    let mut runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
    let base_certified = certify(&mut runtime);
    let mut base_result = parse(base_certified.exact_parse_lease());
    let checkpoint = checkpoint(&mut base_result);
    assert_eq!(checkpoint.prefix_end_byte() as usize, prefix_end);
    assert_eq!(checkpoint.prefix_end_utf16() as usize, prefix_utf16);
    assert_eq!(checkpoint.next_physical_line_ordinal(), 2);
    assert_eq!(checkpoint.definition_count(), 2);
    let base = checkpoint.source();
    drop(base_certified);

    let target_text = "shown **well** 😀\n";
    let target = runtime
        .apply_edit(base, prefix_end..source.len(), target_text)
        .expect("tail edit")
        .source()
        .current();
    let witness = runtime
        .mint_exact_unchanged_prefix_witness(base, prefix_end, prefix_utf16)
        .expect("prefix witness");
    let witness = runtime
        .take_exact_unchanged_prefix_witness(witness)
        .expect("fresh witness");
    let target_certified = certify(&mut runtime);
    assert_eq!(target_certified.source(), target);
    let clean = parse(target_certified.exact_parse_lease());
    let clean_facts = M11ParserTerminalFacts::derive(&clean).expect("clean facts");
    let crop_job = M11LeadingReferencesCropParseJob::new(
        checkpoint,
        witness,
        target_certified.exact_parse_lease(),
        binding(),
    )
    .expect("crop job");
    let (_, cropped) = crop(crop_job);

    assert_eq!(cropped.terminal().kind(), clean.kind());
    assert_eq!(cropped.terminal().source_range(), clean.source_range());
    assert_eq!(cropped.terminal().visible_source(), clean.visible_source());
    assert_eq!(
        cropped.terminal().definition_count(),
        clean.definition_count()
    );
    assert!(cropped.terminal().definitions().is_empty());
    assert_eq!(cropped.facts(), &clean_facts);
    assert_eq!(cropped.work().prefix_source_bytes_scanned(), 0);
    assert_eq!(
        cropped.work().crop_source_bytes_discovered(),
        target_text.len()
    );
    assert_eq!(cropped.work().crop_source_bytes_read(), target_text.len());
    assert_eq!(cropped.work().definitions_enumerated(), 0);
    assert_eq!(cropped.work().definitions_cooked(), 0);

    drop(target_certified);
    close(runtime);
}

#[test]
fn exact_crop_writer_requires_the_exact_base_and_aborts_without_leaking() {
    let source = "[x]: /one\n[y]: /two\nvisible\n";
    let prefix_end = source.find("visible").expect("visible tail");
    let prefix_utf16 = source[..prefix_end].encode_utf16().count();
    let profile = SourceFactsScanProfile::new(64).expect("source-facts profile");
    let mut runtime = DocumentRuntime::new(
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
    .expect("runtime");

    let base_certified = certify(&mut runtime);
    let mut base_result = parse(base_certified.exact_parse_lease());
    let base_checkpoint = checkpoint(&mut base_result);
    let base = base_checkpoint.source();
    let base_candidate = derive_candidate(base_certified, base_result);
    let base_publication = publish_candidate(&mut runtime, base_candidate, [0x41; 16], 1);
    let mut retained_base = retain_publication(&runtime, base_publication);

    let target_tail = "first paragraph with enough text to exercise exact publication\n";
    let target = runtime
        .apply_edit(base, prefix_end..source.len(), target_tail)
        .expect("target edit")
        .source()
        .current();
    let witness = runtime
        .mint_exact_unchanged_prefix_witness(base, prefix_end, prefix_utf16)
        .expect("prefix witness");
    let witness = runtime
        .take_exact_unchanged_prefix_witness(witness)
        .expect("fresh prefix witness");
    let target_certified = certify(&mut runtime);
    assert_eq!(target_certified.source(), target);
    let target_clean = parse(target_certified.exact_parse_lease());
    let crop_job = M11LeadingReferencesCropParseJob::new(
        base_checkpoint,
        witness,
        target_certified.exact_parse_lease(),
        binding(),
    )
    .expect("exact crop");
    let (_, cropped) = crop(crop_job);
    let exact_input = cropped
        .into_exact_segmented_candidate_input()
        .expect("typed exact-crop input");
    let exact_candidate = M11ParserCandidate::derive_segmented_reusing_references(
        exact_input,
        binding().syntax_profile(),
        profile,
    )
    .expect("exact segmented candidate");
    let mut exact_writer = exact_candidate
        .into_writer(&mut runtime, DOCUMENT, [0x42; 16], 2)
        .expect("exact writer");

    assert!(matches!(
        exact_writer.poll(&mut runtime, 1),
        Err(M11CandidateDerivationError::ExactBaseReferencesRequired)
    ));
    for _ in 0..6 {
        assert!(matches!(
            exact_writer
                .poll_reusing_references(&mut runtime, 1, &retained_base)
                .expect("exact writer poll"),
            M11ParserCandidateWriterPoll::Pending { transitions: 1 }
        ));
    }
    exact_writer
        .begin_abort(&mut runtime)
        .expect("begin exact writer abort");
    while !exact_writer
        .poll_abort(&mut runtime, 1)
        .expect("exact writer abort poll")
    {}
    assert_eq!(
        exact_writer.reference_cook_receipt().completed_definitions,
        0
    );
    assert!(!exact_writer.reference_cook_receipt().cancelled);
    drop(exact_writer);

    let normal_candidate = derive_candidate(target_certified, target_clean);
    let mut normal_writer = normal_candidate
        .into_writer(&mut runtime, DOCUMENT, [0x43; 16], 2)
        .expect("normal writer");
    assert!(matches!(
        normal_writer.poll_reusing_references(&mut runtime, 1, &retained_base),
        Err(M11CandidateDerivationError::ExactBaseReferencesRequired)
    ));
    assert!(matches!(
        normal_writer
            .poll(&mut runtime, 1)
            .expect("normal writer poll"),
        M11ParserCandidateWriterPoll::Pending { transitions: 1 }
    ));
    normal_writer
        .begin_abort(&mut runtime)
        .expect("begin normal writer abort");
    while !normal_writer
        .poll_abort(&mut runtime, 1)
        .expect("normal writer abort poll")
    {}
    drop(normal_writer);

    close_retained(&mut runtime, &mut retained_base);
    drop(retained_base);
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(1).expect("runtime close poll").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
}

#[test]
fn successful_crop_mints_the_same_restart_for_the_next_revision() {
    let source = "[x]: /one\n[y]: /two\nvisible\n";
    let prefix_end = source.find("visible").expect("visible tail");
    let prefix_utf16 = source[..prefix_end].encode_utf16().count();
    let mut runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
    let base_certified = certify(&mut runtime);
    let mut base_result = parse(base_certified.exact_parse_lease());
    let base_checkpoint = checkpoint(&mut base_result);
    let base = base_checkpoint.source();
    drop(base_certified);

    let first_tail = "first 😀\n";
    let first = runtime
        .apply_edit(base, prefix_end..source.len(), first_tail)
        .expect("first tail edit")
        .source()
        .current();
    let first_witness = runtime
        .mint_exact_unchanged_prefix_witness(base, prefix_end, prefix_utf16)
        .expect("first prefix witness");
    let first_witness = runtime
        .take_exact_unchanged_prefix_witness(first_witness)
        .expect("fresh first witness");
    let first_certified = certify(&mut runtime);
    let first_job = M11LeadingReferencesCropParseJob::new(
        base_checkpoint,
        first_witness,
        first_certified.exact_parse_lease(),
        binding(),
    )
    .expect("first crop");
    let (_, mut first_crop) = crop(first_job);
    let first_checkpoint = first_crop
        .take_next_restart_checkpoint()
        .expect("next restart");
    assert!(matches!(
        first_crop.take_next_restart_checkpoint(),
        Err(LeadingReferencesCheckpointError::AlreadyTaken)
    ));
    assert_eq!(first_checkpoint.source(), first);
    assert_eq!(first_checkpoint.binding(), binding());
    assert_eq!(first_checkpoint.prefix_end_byte() as usize, prefix_end);
    assert_eq!(first_checkpoint.prefix_end_utf16() as usize, prefix_utf16);
    assert_eq!(first_checkpoint.next_physical_line_ordinal(), 2);
    assert_eq!(first_checkpoint.definition_count(), 2);
    drop(first_certified);

    let second_tail = "second, longer tail\n";
    let second = runtime
        .apply_edit(
            first,
            prefix_end..prefix_end + first_tail.len(),
            second_tail,
        )
        .expect("second tail edit")
        .source()
        .current();
    let second_witness = runtime
        .mint_exact_unchanged_prefix_witness(first, prefix_end, prefix_utf16)
        .expect("second prefix witness");
    let second_witness = runtime
        .take_exact_unchanged_prefix_witness(second_witness)
        .expect("fresh second witness");
    let second_certified = certify(&mut runtime);
    let second_clean = parse(second_certified.exact_parse_lease());
    let second_clean_facts =
        M11ParserTerminalFacts::derive(&second_clean).expect("second clean facts");
    let second_job = M11LeadingReferencesCropParseJob::new(
        first_checkpoint,
        second_witness,
        second_certified.exact_parse_lease(),
        binding(),
    )
    .expect("second crop");
    let (_, mut second_crop) = crop(second_job);
    assert_eq!(second_crop.facts(), &second_clean_facts);
    let second_checkpoint = second_crop
        .take_next_restart_checkpoint()
        .expect("third restart");
    assert_eq!(second_checkpoint.source(), second);
    assert_eq!(second_checkpoint.binding(), binding());
    assert_eq!(second_checkpoint.prefix_end_byte() as usize, prefix_end);
    assert_eq!(second_checkpoint.prefix_end_utf16() as usize, prefix_utf16);
    assert_eq!(second_checkpoint.next_physical_line_ordinal(), 2);
    assert_eq!(second_checkpoint.definition_count(), 2);

    drop(second_certified);
    close(runtime);
}

#[test]
fn definitions_only_crop_reproduces_empty_without_opening_an_eof_line() {
    let source = "[x]: /one\n[y]: /two\n";
    let mut runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
    let certified = certify(&mut runtime);
    let mut clean = parse(certified.exact_parse_lease());
    let clean_facts = M11ParserTerminalFacts::derive(&clean).expect("clean facts");
    let checkpoint = checkpoint(&mut clean);
    assert_eq!(checkpoint.prefix_end_byte() as usize, source.len());
    let base = checkpoint.source();
    drop(certified);
    let inserted = runtime
        .apply_edit(base, source.len()..source.len(), "temporary")
        .expect("temporary tail insertion")
        .source()
        .current();
    runtime
        .apply_edit(inserted, source.len()..source.len() + "temporary".len(), "")
        .expect("temporary tail deletion");
    let witness = runtime
        .mint_exact_unchanged_prefix_witness(
            base,
            checkpoint.prefix_end_byte() as usize,
            checkpoint.prefix_end_utf16() as usize,
        )
        .expect("same-source witness");
    let witness = runtime
        .take_exact_unchanged_prefix_witness(witness)
        .expect("fresh witness");
    let certified = certify(&mut runtime);
    let target_clean = parse(certified.exact_parse_lease());
    let target_facts = M11ParserTerminalFacts::derive(&target_clean).expect("target facts");
    let crop_job = M11LeadingReferencesCropParseJob::new(
        checkpoint,
        witness,
        certified.exact_parse_lease(),
        binding(),
    )
    .expect("EOF crop");
    let (_, cropped) = crop(crop_job);
    assert_eq!(cropped.terminal().kind(), M11CleanDocumentKind::Empty);
    assert_eq!(cropped.facts(), &target_facts);
    assert_eq!(cropped.facts(), &clean_facts);
    assert_eq!(cropped.work().crop_source_bytes_discovered(), 0);
    assert_eq!(cropped.work().crop_source_bytes_read(), 0);

    drop(certified);
    close(runtime);
}

#[test]
fn checkpoint_is_one_take_and_ineligible_without_leading_definitions() {
    let store = SourceStore::new("[x]: /one\nvisible\n").expect("source");
    let mut eligible = parse(store.snapshot());
    let _checkpoint = checkpoint(&mut eligible);
    assert!(matches!(
        eligible.take_leading_references_restart_checkpoint(binding()),
        Err(LeadingReferencesCheckpointError::AlreadyTaken)
    ));

    let store = SourceStore::new("plain paragraph\n").expect("source");
    let mut ineligible = parse(store.snapshot());
    assert!(matches!(
        ineligible.take_leading_references_restart_checkpoint(binding()),
        Err(LeadingReferencesCheckpointError::Ineligible)
    ));
    assert!(matches!(
        ineligible.take_leading_references_restart_checkpoint(binding()),
        Err(LeadingReferencesCheckpointError::Ineligible)
    ));
}

#[test]
fn crop_rejects_binding_authority_and_cut_mismatches() {
    let source = "[x]: /one\nvisible\n";

    let mut profile_runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("profile runtime");
    let profile_base_certified = certify(&mut profile_runtime);
    let mut profile_result = parse(profile_base_certified.exact_parse_lease());
    let profile_checkpoint = checkpoint(&mut profile_result);
    let profile_base = profile_checkpoint.source();
    drop(profile_base_certified);
    profile_runtime
        .apply_edit(profile_base, source.len() - 1..source.len(), "!\n")
        .expect("profile target edit");
    let profile_witness = profile_runtime
        .mint_exact_unchanged_prefix_witness(
            profile_base,
            profile_checkpoint.prefix_end_byte() as usize,
            profile_checkpoint.prefix_end_utf16() as usize,
        )
        .expect("profile witness");
    let profile_witness = profile_runtime
        .take_exact_unchanged_prefix_witness(profile_witness)
        .expect("fresh profile witness");
    let profile_certified = certify(&mut profile_runtime);
    let wrong_profile =
        M11ParserBinding::current(ParserProfileId::new(PROFILE + 1).expect("wrong profile"));
    assert!(matches!(
        M11LeadingReferencesCropParseJob::new(
            profile_checkpoint,
            profile_witness,
            profile_certified.exact_parse_lease(),
            wrong_profile,
        ),
        Err(M11LeadingReferencesCropError::BindingMismatch)
    ));
    drop(profile_certified);
    close(profile_runtime);

    let mut cut_runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("cut runtime");
    let cut_base_certified = certify(&mut cut_runtime);
    let mut cut_result = parse(cut_base_certified.exact_parse_lease());
    let cut_checkpoint = checkpoint(&mut cut_result);
    let cut_base = cut_checkpoint.source();
    drop(cut_base_certified);
    cut_runtime
        .apply_edit(cut_base, source.len() - 1..source.len(), "!\n")
        .expect("cut target edit");
    let wrong_end = cut_checkpoint.prefix_end_byte() as usize - 1;
    let cut_witness = cut_runtime
        .mint_exact_unchanged_prefix_witness(cut_base, wrong_end, wrong_end)
        .expect("different unchanged cut");
    let cut_witness = cut_runtime
        .take_exact_unchanged_prefix_witness(cut_witness)
        .expect("fresh cut witness");
    let cut_certified = certify(&mut cut_runtime);
    assert!(matches!(
        M11LeadingReferencesCropParseJob::new(
            cut_checkpoint,
            cut_witness,
            cut_certified.exact_parse_lease(),
            binding(),
        ),
        Err(M11LeadingReferencesCropError::CutMismatch)
    ));
    drop(cut_certified);
    close(cut_runtime);

    let mut authority_runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("authority runtime");
    let authority_base_certified = certify(&mut authority_runtime);
    let mut authority_result = parse(authority_base_certified.exact_parse_lease());
    let authority_checkpoint = checkpoint(&mut authority_result);
    let authority_base = authority_checkpoint.source();
    drop(authority_base_certified);
    authority_runtime
        .apply_edit(authority_base, source.len() - 1..source.len(), "!\n")
        .expect("authority target edit");
    let authority_witness = authority_runtime
        .mint_exact_unchanged_prefix_witness(
            authority_base,
            authority_checkpoint.prefix_end_byte() as usize,
            authority_checkpoint.prefix_end_utf16() as usize,
        )
        .expect("authority witness");
    let authority_witness = authority_runtime
        .take_exact_unchanged_prefix_witness(authority_witness)
        .expect("fresh authority witness");
    let foreign = SourceStore::new(source).expect("foreign source");
    assert!(matches!(
        M11LeadingReferencesCropParseJob::new(
            authority_checkpoint,
            authority_witness,
            foreign.snapshot(),
            binding(),
        ),
        Err(M11LeadingReferencesCropError::AuthorityMismatch)
    ));
    close(authority_runtime);
}

#[test]
fn runtime_rejects_a_witness_made_stale_before_crop_construction() {
    let source = "[x]: /one\nvisible\n";
    let prefix_end = source.find("visible").expect("tail");
    let mut runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
    let base_certified = certify(&mut runtime);
    let mut base_result = parse(base_certified.exact_parse_lease());
    let checkpoint = checkpoint(&mut base_result);
    let base = checkpoint.source();
    drop(base_certified);

    let first = runtime
        .apply_edit(base, prefix_end..source.len(), "first\n")
        .expect("first tail edit")
        .source()
        .current();
    let witness = runtime
        .mint_exact_unchanged_prefix_witness(
            base,
            checkpoint.prefix_end_byte() as usize,
            checkpoint.prefix_end_utf16() as usize,
        )
        .expect("prefix witness");
    runtime
        .apply_edit(first, prefix_end..prefix_end + "first\n".len(), "second\n")
        .expect("second tail edit");
    assert!(matches!(
        runtime.take_exact_unchanged_prefix_witness(witness),
        Err(DocumentRuntimeError::ExactUnchangedPrefixStale)
    ));

    drop(checkpoint);
    close(runtime);
}

fn crop_after_tail_replacement(
    replacement: &str,
) -> Result<M11LeadingReferencesCropResult, M11LeadingReferencesCropError> {
    let source = "[x]: /one\nvisible\n";
    let prefix_end = source.find("visible").expect("tail");
    let mut runtime =
        DocumentRuntime::new(source, DocumentRuntimeConfig::default()).expect("runtime");
    let base_certified = certify(&mut runtime);
    let mut base_result = parse(base_certified.exact_parse_lease());
    let checkpoint = checkpoint(&mut base_result);
    let base = checkpoint.source();
    drop(base_certified);
    runtime
        .apply_edit(base, prefix_end..source.len(), replacement)
        .expect("tail edit");
    let witness = runtime
        .mint_exact_unchanged_prefix_witness(
            base,
            checkpoint.prefix_end_byte() as usize,
            checkpoint.prefix_end_utf16() as usize,
        )
        .expect("prefix witness");
    let witness = runtime
        .take_exact_unchanged_prefix_witness(witness)
        .expect("fresh prefix witness");
    let target_certified = certify(&mut runtime);
    let result = (|| {
        let mut job = M11LeadingReferencesCropParseJob::new(
            checkpoint,
            witness,
            target_certified.exact_parse_lease(),
            binding(),
        )?;
        loop {
            match job.poll(64)? {
                M11LeadingReferencesCropPoll::Pending { .. } => {}
                M11LeadingReferencesCropPoll::Complete { result, .. } => break Ok(result),
            }
        }
    })();
    drop(target_certified);
    close(runtime);
    result
}

#[test]
fn crop_rejects_new_definitions_and_unpublishable_segmented_terminals() {
    assert!(matches!(
        crop_after_tail_replacement("[new]: /target\nvisible\n"),
        Err(M11LeadingReferencesCropError::CropAcceptedDefinition)
    ));
    let segmented = crop_after_tail_replacement("> quote\n");
    assert!(
        matches!(
            &segmented,
            Err(M11LeadingReferencesCropError::TerminalMismatch)
        ),
        "unexpected crop result: {:?}",
        segmented.as_ref().err()
    );
}

#[test]
fn crop_work_is_independent_of_one_four_thousand_and_one_hundred_thousand_definitions() {
    let mut expected_transitions = None;
    for count in [1_usize, 4_096, 100_000] {
        let mut source = String::new();
        for index in 0..count {
            use std::fmt::Write;
            writeln!(&mut source, "[r{index}]: /{index}").expect("definition");
        }
        let tail = "visible\n";
        source.push_str(tail);
        let mut runtime =
            DocumentRuntime::new(&source, DocumentRuntimeConfig::default()).expect("runtime");
        let base_certified = certify(&mut runtime);
        let mut clean_fuel_one = parse(base_certified.exact_parse_lease());
        let mut clean_fuel_sixty_four = parse(base_certified.exact_parse_lease());
        assert_eq!(clean_fuel_one.definition_count(), count);
        assert_eq!(clean_fuel_sixty_four.definition_count(), count);
        let checkpoint_fuel_one = checkpoint(&mut clean_fuel_one);
        let checkpoint_fuel_sixty_four = checkpoint(&mut clean_fuel_sixty_four);
        assert_eq!(checkpoint_fuel_one.definition_count(), count);
        assert_eq!(checkpoint_fuel_sixty_four.definition_count(), count);
        let base = checkpoint_fuel_one.source();
        assert_eq!(checkpoint_fuel_sixty_four.source(), base);
        drop(base_certified);
        runtime
            .apply_edit(base, source.len() - tail.len()..source.len(), "visible!\n")
            .expect("tail edit");
        let witness_fuel_one = runtime
            .mint_exact_unchanged_prefix_witness(
                base,
                checkpoint_fuel_one.prefix_end_byte() as usize,
                checkpoint_fuel_one.prefix_end_utf16() as usize,
            )
            .expect("fuel-one witness");
        let witness_fuel_one = runtime
            .take_exact_unchanged_prefix_witness(witness_fuel_one)
            .expect("fresh fuel-one witness");
        let witness_fuel_sixty_four = runtime
            .mint_exact_unchanged_prefix_witness(
                base,
                checkpoint_fuel_sixty_four.prefix_end_byte() as usize,
                checkpoint_fuel_sixty_four.prefix_end_utf16() as usize,
            )
            .expect("fuel-sixty-four witness");
        let witness_fuel_sixty_four = runtime
            .take_exact_unchanged_prefix_witness(witness_fuel_sixty_four)
            .expect("fresh fuel-sixty-four witness");
        let target_certified = certify(&mut runtime);
        let target_clean = parse(target_certified.exact_parse_lease());
        let target_facts =
            M11ParserTerminalFacts::derive(&target_clean).expect("target clean facts");
        let crop_fuel_one = M11LeadingReferencesCropParseJob::new(
            checkpoint_fuel_one,
            witness_fuel_one,
            target_certified.exact_parse_lease(),
            binding(),
        )
        .expect("fuel-one crop job");
        let crop_fuel_sixty_four = M11LeadingReferencesCropParseJob::new(
            checkpoint_fuel_sixty_four,
            witness_fuel_sixty_four,
            target_certified.exact_parse_lease(),
            binding(),
        )
        .expect("fuel-sixty-four crop job");
        let (transitions_fuel_one, cropped_fuel_one) = crop_with_fuel(crop_fuel_one, 1);
        let (transitions_fuel_sixty_four, cropped_fuel_sixty_four) =
            crop_with_fuel(crop_fuel_sixty_four, 64);
        assert_eq!(transitions_fuel_one, transitions_fuel_sixty_four);
        expected_transitions.get_or_insert(transitions_fuel_one);
        assert_eq!(Some(transitions_fuel_one), expected_transitions);
        for cropped in [&cropped_fuel_one, &cropped_fuel_sixty_four] {
            assert_eq!(cropped.facts(), &target_facts);
            assert_eq!(cropped.terminal().kind(), target_clean.kind());
            assert_eq!(
                cropped.terminal().visible_source(),
                target_clean.visible_source()
            );
            assert_eq!(cropped.terminal().definition_count(), count);
            assert_eq!(cropped.work().prefix_source_bytes_scanned(), 0);
            assert_eq!(
                cropped.work().crop_source_bytes_discovered(),
                "visible!\n".len()
            );
            assert_eq!(cropped.work().crop_source_bytes_read(), "visible!\n".len());
            assert_eq!(cropped.work().reused_definitions(), count);
            assert_eq!(cropped.work().definitions_enumerated(), 0);
            assert_eq!(cropped.work().definitions_cooked(), 0);
        }

        drop(target_certified);
        close(runtime);
    }
}

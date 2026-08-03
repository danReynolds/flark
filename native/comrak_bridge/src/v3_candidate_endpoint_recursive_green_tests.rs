//! Focused recursive-Green topology and publication regressions.

use super::*;

#[test]
fn paragraph_split_publishes_two_roots_atomically_and_retains_distant_suffix() {
    let mut source = String::new();
    for ordinal in 0..512 {
        source.push_str(&format!("Prefix paragraph {ordinal:04}.\n\n"));
    }
    let first_start = source.len();
    source.push_str("joined alpha\n");
    let split_at = source.len();
    let second_start = source.len();
    source.push_str("joined beta\n\n");
    for ordinal in 0..512 {
        source.push_str(&format!("Suffix paragraph {ordinal:04}.\n\n"));
    }
    let distant_start = source
        .find("Suffix paragraph 0511")
        .expect("distant suffix Paragraph");

    let profile = SourceFactsScanProfile::new(8).expect("dense test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [336, 337, 338, 339],
        source_session_identity: 340,
        worker_generation: 1,
    };
    let mut runtime = DocumentRuntime::new(&source, standard_document_runtime_config())
        .expect("paragraph-split runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_source = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start paragraph-split base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("paragraph-split host");
    let base_wire_source = source_version_for(binding, base_completion);
    host.observe_source_version(base_wire_source)
        .expect("host observes paragraph-split base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

    let base_first_frame =
        recursive_green_owner_frame(&host, base_wire_source, first_start, first_start);
    assert_eq!(
        recursive_green_owner_frame(&host, base_wire_source, second_start, second_start),
        base_first_frame,
        "the two physical lines begin in one base Paragraph root",
    );
    let retained_suffix_frame =
        recursive_green_owner_frame(&host, base_wire_source, distant_start, distant_start);

    endpoint
        .cancel_for_edit(&mut runtime)
        .expect("prepare paragraph split");
    let target_source = runtime
        .apply_edit(base_source, split_at..split_at, "\n")
        .expect("split one Paragraph into two")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan paragraph-split SourceFacts");
    assert!(plan.base_byte_range().start > 0);
    assert!(plan.base_byte_range().end < base_source.byte_len());
    assert!(endpoint
        .has_incremental_base_for_plan(&runtime, &plan)
        .expect("paragraph-split exact-base preflight"));
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("paragraph-split target lease");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    let target_wire_source = source_version_for(binding, target_completion);
    host.observe_source_version(target_wire_source)
        .expect("host observes paragraph-split target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start paragraph-split exact candidate");

    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
    assert!(
        target_delivery.packet_frames.len() > 1,
        "the atomic exact delta deliberately crosses a packet-credit boundary",
    );
    let replacement_records = target_delivery
        .packet_frames
        .iter()
        .flatten()
        .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage)
        .map(|(_, records)| *records)
        .sum::<u32>();
    assert_eq!(
        replacement_records, 9,
        "the replacement must carry the complete two-Paragraph topology",
    );
    assert!(target_delivery
        .packet_frames
        .iter()
        .flatten()
        .all(|(kind, _)| *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage));
    assert_eq!(target_delivery.offer.transferred_record_count, 13);
    assert!(target_delivery.offer.target_record_count > 5_000);
    assert!(!target_delivery.contains_recursive_green_branch);
    assert_eq!(
        endpoint.recursive_green_path_receipt(),
        RecursiveGreenPathReceipt {
            local_adoption_deliveries: 1,
            clean_fallback_deliveries: 0,
        },
    );

    let target_first_frame =
        recursive_green_owner_frame(&host, target_wire_source, first_start, first_start);
    let target_second_frame = recursive_green_owner_frame(
        &host,
        target_wire_source,
        second_start + 1,
        second_start + 1,
    );
    assert_ne!(
        target_first_frame, target_second_frame,
        "the committed target must expose two distinct Paragraph roots",
    );
    assert_eq!(
        recursive_green_owner_frame(
            &host,
            target_wire_source,
            distant_start + 1,
            distant_start + 1,
        ),
        retained_suffix_frame,
        "the distant suffix root must retain its exact frame identity",
    );
    assert!(endpoint
        .has_exact_base_for(&runtime, target_source)
        .expect("paragraph-split target exact-base continuity"));

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
#[ignore = "ExactBaseDelta must publish sparse far spanning-Exit repair pages"]
fn large_quote_utf16_edit_publishes_the_repaired_paragraph_endpoint() {
    const QUOTE_LINES: usize = 2_048;
    const EDIT_LINE: usize = QUOTE_LINES / 2;

    let mut source = String::new();
    for ordinal in 0..128 {
        source.push_str(&format!(
            "Prefix paragraph {ordinal:03} keeps the quote away from BOF.\n\n"
        ));
    }
    for ordinal in 0..QUOTE_LINES {
        source.push_str(&format!(
            "> quoted line {ordinal:04} carries alpha through one open paragraph.\n"
        ));
    }
    source.push('\n');
    for ordinal in 0..128 {
        source.push_str(&format!(
            "Suffix paragraph {ordinal:03} keeps the quote away from EOF.\n\n"
        ));
    }

    let edit_line = format!("> quoted line {EDIT_LINE:04} carries alpha");
    let edit_start = source
        .find(&edit_line)
        .map(|line| line + edit_line.find("alpha").expect("alpha in edit line"))
        .expect("middle quote edit");
    let edit_end = edit_start + "alpha".len();
    let row_start = source[..edit_start]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let editable_start = row_start + "> ".len();
    let row_end = edit_end
        + source[edit_end..]
            .find('\n')
            .expect("edited quote row terminator")
        + 1;
    let editable_end = row_end - 1;
    let mut target_text = source.clone();
    target_text.replace_range(edit_start..edit_end, "βeta");
    assert_eq!(target_text.len(), source.len());
    assert_eq!(
        target_text.encode_utf16().count() + 1,
        source.encode_utf16().count()
    );

    let profile = SourceFactsScanProfile::new(8).expect("dense test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [341, 342, 343, 344],
        source_session_identity: 345,
        worker_generation: 1,
    };
    let mut runtime = DocumentRuntime::new(&source, standard_document_runtime_config())
        .expect("large-quote runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_source = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start large-quote base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("large-quote host");
    let base_wire_source = source_version_for(binding, base_completion);
    host.observe_source_version(base_wire_source)
        .expect("host observes large-quote base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

    let base_point_utf16 = source[..edit_start].encode_utf16().count();
    let (base_kind, base_range, base_ancestry) =
        recursive_green_query_shape(&host, base_wire_source, edit_start, base_point_utf16);
    assert_eq!(base_kind, 5);
    assert_eq!(base_ancestry, vec![1, 2, 5]);
    assert_eq!(base_range[0] as usize, editable_start);
    assert_eq!(base_range[1] as usize, editable_end);
    assert_eq!(
        base_range[2] as usize,
        source[..editable_start].encode_utf16().count()
    );
    assert_eq!(
        base_range[3] as usize,
        source[..editable_end].encode_utf16().count()
    );

    endpoint
        .cancel_for_edit(&mut runtime)
        .expect("prepare large-quote edit");
    let target_source = runtime
        .apply_edit(base_source, edit_start..edit_end, "βeta")
        .expect("apply UTF-16-changing quote edit")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan large-quote SourceFacts");
    assert!(endpoint
        .has_incremental_base_for_plan(&runtime, &plan)
        .expect("large-quote exact-base preflight"));
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    let target_wire_source = source_version_for(binding, target_completion);
    host.observe_source_version(target_wire_source)
        .expect("host observes large-quote target");
    endpoint
        .start_incremental(
            &runtime,
            runtime
                .snapshot_current_source()
                .expect("borrow large-quote target"),
            witness,
            binding,
            target_completion,
        )
        .expect("start large-quote exact candidate");

    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
    assert_eq!(
        endpoint.recursive_green_path_receipt(),
        RecursiveGreenPathReceipt {
            local_adoption_deliveries: 1,
            clean_fallback_deliveries: 0,
        }
    );

    let target_point_utf16 = target_text[..edit_start].encode_utf16().count();
    let (target_kind, target_range, target_ancestry) =
        recursive_green_query_shape(&host, target_wire_source, edit_start, target_point_utf16);
    assert_eq!(target_kind, 5);
    assert_eq!(target_ancestry, vec![1, 2, 5]);
    assert_eq!(target_range[0] as usize, editable_start);
    assert_eq!(target_range[1] as usize, editable_end);
    assert_eq!(
        target_range[2] as usize,
        target_text[..editable_start].encode_utf16().count()
    );
    assert_eq!(
        target_range[3] as usize,
        target_text[..editable_end].encode_utf16().count(),
        "the independent host must receive the far spanning Exit repair"
    );
    assert!(endpoint
        .has_exact_base_for(&runtime, target_source)
        .expect("large-quote target exact-base continuity"));

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

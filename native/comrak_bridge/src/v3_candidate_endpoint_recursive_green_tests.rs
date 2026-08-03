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

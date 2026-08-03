//! Tests for the candidate endpoint facade.

use super::*;
use crate::v3_endpoint::standard_document_runtime_config;
use crate::v3_host_store::{
    HostBlockRangeBudget, HostBlockRangeOutcome, HostBlockRangeQuery, HostConfig,
    HostInlineSidecarPayloadKind, HostInlineSidecarQueryOutcome, HostMetricAffinity,
    HostMetricRange, HostPointQuery, HostPollOutcome as NativeHostPollOutcome, HostQueryBudget,
    HostSourceGapReason, HostSourceMetric, HostStructuralOrdinalWindowBudget,
    HostStructuralOrdinalWindowOutcome, HostStructuralOrdinalWindowQuery,
    HostStructuralQueryOutcome,
    HostViewportPresentationPollOutcome as NativeViewportPresentationPollOutcome, HostWorkGrant,
    InlineSidecarHostPollOutcome as NativeInlineSidecarHostPollOutcome, NativeCandidateHost,
    HOST_M11_VIEWPORT_BYTES, HOST_RECURSIVE_GREEN_ANCESTOR_RECORD_BYTES,
    HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES, HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES,
    HOST_RECURSIVE_GREEN_ROW_RANGE_SCHEMA, HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES,
    HOST_RECURSIVE_GREEN_VIEWPORT_HEADER_BYTES, HOST_RECURSIVE_GREEN_VIEWPORT_SCHEMA,
};
use crate::v3_publication_wire::{
    decode_viewport_presentation_child_frame, decode_viewport_presentation_directory,
    decode_viewport_presentation_end_frame, decode_viewport_presentation_parent_frame,
    viewport_presentation_frame_digest256,
};
use flark_engine::m11_host::{M11CandidateHost, M11HostFrameKind};
use flark_engine::parser_internal::{
    M11CandidateBuild, M11CandidateBuildPoll, M11InlineProjectionKind, M11RoleRecords,
};
use flark_engine::{
    ParserProfileId, RuntimeSourceFactsPoll, SourceFactsRootLimits, SourceFactsScanProfile,
    SourceRevision, SourceSeedBuilder,
};
use flark_parser::M11_INLINE_FACT_RECORD_BYTES;
const TEST_SOURCE: &str = "candidate packet test\n";

fn test_streaming(source_fact_records: usize) -> (DocumentRuntime, StreamingCandidate) {
    let mut runtime = DocumentRuntime::new(TEST_SOURCE, standard_document_runtime_config())
        .expect("test runtime");
    let streaming = streaming_for_runtime(&mut runtime, source_fact_records, 1);
    (runtime, streaming)
}

fn streaming_for_runtime(
    runtime: &mut DocumentRuntime,
    source_fact_records: usize,
    generation: u32,
) -> StreamingCandidate {
    let utf16_length = TEST_SOURCE.encode_utf16().count();
    let mut seed = SourceSeedBuilder::new(SourceRevision::new(u64::from(generation)), utf16_length);
    seed.append_page(0..utf16_length, TEST_SOURCE)
        .expect("test source page");
    let source = seed.finalize().expect("test source");
    let records = M11RoleRecords::new(
        (0..source_fact_records).map(|ordinal| {
            vec![u8::try_from(ordinal & 0xff).expect("bounded ordinal")].into_boxed_slice()
        }),
        Box::<[u8]>::from(&b"green"[..]),
        Box::<[u8]>::from(&b"projection"[..]),
    )
    .expect("test role records");
    let publication_seed = u8::try_from(generation.checked_add(1).expect("bounded generation"))
        .expect("bounded generation");
    let mut build = M11CandidateBuild::new(
        runtime,
        [1; 16],
        [publication_seed; 16],
        source.version(),
        u64::from(generation),
        1,
        records,
    )
    .expect("test candidate build");
    build.finish_references(runtime).expect("finish references");
    while let M11CandidateBuildPoll::Pending { .. } =
        build.poll(runtime, 256).expect("candidate build poll")
    {}
    let publication = build.into_publication().expect("test publication");
    let descriptor = publication.descriptor(runtime).expect("test descriptor");
    let stream = Box::new(publication)
        .into_snapshot_stream(runtime)
        .expect("test snapshot stream");
    let record_count =
        u32::try_from(descriptor.canonical_record_count).expect("bounded test record count");
    StreamingCandidate {
        stream: Some(stream),
        sealed_publication: None,
        offer: OfferBegin {
            schema: MANIFEST_SCHEMA,
            offer_id: [generation; 4],
            publication_session: digest_words(descriptor.publication),
            target_host_revision: generation,
            source_version: SourceVersion {
                document_session: [5, 6, 7, 8],
                revision: generation,
                utf8_length: u32::try_from(TEST_SOURCE.len()).expect("bounded source"),
                utf16_length: u32::try_from(utf16_length).expect("bounded source"),
                content_hash128: [generation; 4],
            },
            source_root: split_u64(descriptor.source_root),
            parse_generation: generation,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: descriptor.syntax_profile,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            mode: PublicationMode::FullSnapshot,
            base_ack: None,
            transferred_record_count: record_count,
            target_record_count: record_count,
            limits: OfferLimits {
                maximum_frame_count: u32::try_from(descriptor.maximum_snapshot_frames)
                    .expect("bounded frames"),
                maximum_encoded_frame_bytes: u32::try_from(
                    descriptor.maximum_snapshot_encoded_bytes,
                )
                .expect("bounded bytes"),
                maximum_packet_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
                maximum_frame_bytes: M11_MAX_SNAPSHOT_FRAME_BYTES as u32,
                maximum_program_children: M11_MAX_ROLE_RECORDS as u32,
            },
        },
        descriptor,
        phase: StreamPhase::NeedPacket,
        transport: Some(CandidateTransportDigest::new()),
        next_frame_ordinal: 0,
        next_record_ordinal: 0,
        next_node_ordinal: None,
        packet: PacketBuilder::default(),
        lookahead: None,
        resume_after_packet_credit: false,
        canonical_stream_digest: None,
        commit: None,
        expected_ack: None,
        next_restart: None,
        superseded_exact_base: None,
        exact_base_recovery: None,
    }
}

fn cancel_streaming_to_zero(runtime: DocumentRuntime, streaming: StreamingCandidate) {
    cancel_endpoint_to_zero(
        runtime,
        CandidateEndpoint {
            active: Some(ActiveCandidate::Streaming(Box::new(streaming))),
            cleanup: None,
            retained: None,
            recursive_green: RecursiveGreenEndpointSlot::new(),
            bullet_list_local_edit: None,
            viewport_inline_batch: None,
            pending_viewport_unavailable: None,
            last_viewport_generation: 0,
            hot_inline: None,
            hot_inline_sidecar: None,
            last_hot_inline_generation: 0,
            closing: false,
        },
    );
}

fn cancel_endpoint_to_zero(mut runtime: DocumentRuntime, mut endpoint: CandidateEndpoint) {
    endpoint.cancel().expect("cancel streaming candidate");
    for _ in 0..100_000 {
        if endpoint
            .poll_cleanup(&mut runtime, 1)
            .expect("bounded cleanup")
        {
            assert!(!endpoint.cleanup_pending());
            assert!(!endpoint.has_poll_work());
            runtime.begin_close().expect("begin runtime close");
            while !runtime.poll_close(256).expect("runtime close").complete {}
            assert_eq!(runtime.arena_metrics().resident_nodes, 0);
            return;
        }
    }
    panic!("streaming candidate did not reclaim to zero");
}

fn poll_to_packet_event(
    runtime: &DocumentRuntime,
    streaming: &mut StreamingCandidate,
    fuel: usize,
) -> (usize, CandidateEvent) {
    let mut pending_polls = 0;
    for _ in 0..100_000 {
        match streaming
            .poll_event(runtime, fuel)
            .expect("candidate packet poll")
        {
            CandidatePoll::Pending { transitions } => {
                assert!(transitions <= fuel);
                pending_polls += 1;
            }
            CandidatePoll::Event { transitions, event } => {
                assert!(transitions <= fuel);
                assert!(matches!(event.body, CandidateEventBody::Packet { .. }));
                return (pending_polls, *event);
            }
            CandidatePoll::HotInlineEvent { .. } => {
                panic!("structural stream emitted a hot-inline event")
            }
            CandidatePoll::ViewportPresentationEvent { .. } => {
                panic!("structural stream emitted a viewport event")
            }
            CandidatePoll::ViewportPresentationUnavailable { .. } => {
                panic!("structural stream emitted viewport unavailability")
            }
        }
    }
    panic!("candidate packet did not become available");
}

fn drain_candidate_cleanup(endpoint: &mut CandidateEndpoint, runtime: &mut DocumentRuntime) {
    drain_candidate_cleanup_with_fuel(endpoint, runtime, 1);
}

fn drain_candidate_cleanup_with_fuel(
    endpoint: &mut CandidateEndpoint,
    runtime: &mut DocumentRuntime,
    fuel: usize,
) {
    assert!(fuel > 0);
    for _ in 0..100_000 {
        if !endpoint.cleanup_pending() {
            return;
        }
        endpoint
            .poll_cleanup(runtime, fuel)
            .expect("bounded candidate cleanup");
    }
    panic!("candidate cleanup did not complete");
}

fn complete_clean_source_facts(
    runtime: &mut DocumentRuntime,
    profile: SourceFactsScanProfile,
    parser_profile: ParserProfileId,
    certification_id: u32,
    ui_revision: u32,
) -> (CertifiedSource, SourceFactsCompletionEvent) {
    runtime
        .begin_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("begin clean SourceFacts");
    loop {
        match runtime
            .poll_source_facts(128, 64)
            .expect("bounded clean SourceFacts poll")
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. }
            | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
            RuntimeSourceFactsPoll::Complete { .. } => break,
            RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
            | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                panic!("clean SourceFacts job became incremental")
            }
        }
    }
    let certified = runtime
        .take_certified_source()
        .expect("completed clean certification");
    let facts = certified.facts();
    let fingerprint = facts.fingerprint();
    let completion = SourceFactsCompletionEvent {
        certification_id,
        worker_replica_revision: u32::try_from(certified.source().revision().get())
            .expect("test revision"),
        ui_revision,
        utf16_length: u32::try_from(certified.source().utf16_len()).expect("test UTF-16"),
        intent_high_water: ui_revision,
        fingerprint_algorithm: fingerprint.algorithm(),
        utf8_length: u32::try_from(fingerprint.byte_len()).expect("test bytes"),
        logical_line_breaks: u32::try_from(facts.logical_line_breaks()).expect("test line breaks"),
        checkpoint_spacing_utf16: u32::try_from(facts.profile().checkpoint_spacing_utf16())
            .expect("test checkpoint spacing"),
        checkpoint_count: u32::try_from(facts.checkpoint_count()).expect("test checkpoint count"),
        page_count: u32::try_from(facts.page_count()).expect("test page count"),
        content_hash128: fingerprint.rolling_hash().words(),
        // Candidate authority consumes the content proof. The session
        // certification layer independently authenticates this page proof.
        checkpoint_hash128: [certification_id; 4],
    };
    (certified, completion)
}

fn complete_incremental_source_facts(
    runtime: &mut DocumentRuntime,
) -> Box<PersistentSourceFactsDeltaWitness> {
    loop {
        match runtime
            .poll_source_facts(128, 64)
            .expect("bounded incremental SourceFacts poll")
        {
            RuntimeSourceFactsPoll::Pending(_)
            | RuntimeSourceFactsPoll::PromotionPending { .. }
            | RuntimeSourceFactsPoll::IncrementalScanComplete { .. } => {}
            RuntimeSourceFactsPoll::IncrementalComplete { witness, .. } => return witness,
            RuntimeSourceFactsPoll::ScanComplete { .. }
            | RuntimeSourceFactsPoll::Complete { .. } => {
                panic!("incremental SourceFacts job became clean")
            }
        }
    }
}

fn completion_for_persistent_target(
    runtime: &DocumentRuntime,
    certification_id: u32,
    ui_revision: u32,
) -> SourceFactsCompletionEvent {
    let facts = runtime
        .persistent_source_facts()
        .expect("persistent target facts");
    let summary = facts.summary();
    SourceFactsCompletionEvent {
        certification_id,
        worker_replica_revision: u32::try_from(facts.source().revision().get())
            .expect("test revision"),
        ui_revision,
        utf16_length: u32::try_from(summary.utf16_len()).expect("test UTF-16"),
        intent_high_water: ui_revision,
        fingerprint_algorithm: facts.profile().content_fingerprint_algorithm(),
        utf8_length: u32::try_from(summary.byte_len()).expect("test bytes"),
        logical_line_breaks: u32::try_from(summary.logical_line_breaks())
            .expect("test line breaks"),
        checkpoint_spacing_utf16: u32::try_from(facts.profile().checkpoint_spacing_utf16())
            .expect("test checkpoint spacing"),
        checkpoint_count: u32::try_from(facts.checkpoint_count()).expect("test checkpoint count"),
        page_count: u32::try_from(facts.page_count()).expect("test page count"),
        content_hash128: summary.rolling_hash().words(),
        checkpoint_hash128: [certification_id; 4],
    }
}

fn source_version_for(
    binding: SessionBinding,
    completion: SourceFactsCompletionEvent,
) -> SourceVersion {
    SourceVersion {
        document_session: binding.document_session,
        revision: completion.ui_revision,
        utf8_length: completion.utf8_length,
        utf16_length: completion.utf16_length,
        content_hash128: completion.content_hash128,
    }
}

fn assert_installed_candidate_has_no_inline(
    host: &NativeCandidateHost,
    source_version: SourceVersion,
) {
    let mut output = vec![0_u8; HOST_M11_VIEWPORT_BYTES];
    let outcome = host
        .query_structural(
            HostPointQuery {
                source_version,
                position: HostSourceMetric { bytes: 0, utf16: 0 },
                affinity: HostMetricAffinity::Downstream,
                budget: HostQueryBudget {
                    maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                    maximum_open_depth: 64,
                    maximum_leaf_count: 64,
                    maximum_tree_nodes_visited: 256,
                },
            },
            &mut output,
        )
        .expect("query installed exact candidate");
    match outcome {
        HostStructuralQueryOutcome::Viewport { receipt, .. } => {
            let schema = u32::from_le_bytes(output[8..12].try_into().expect("viewport schema"));
            assert!(
                matches!(schema, 1 | HOST_RECURSIVE_GREEN_VIEWPORT_SCHEMA),
                "canonical publication must expose either legacy structural or recursive-Green authority"
            );
            assert!(receipt.encoded_bytes <= HOST_M11_VIEWPORT_BYTES as u32);
        }
        HostStructuralQueryOutcome::SourceGap {
            reason: HostSourceGapReason::EncodedByteLimit,
            ..
        } => {}
        other => panic!("exact candidate must fail closed without inline authority: {other:?}"),
    }
}

#[derive(Debug)]
struct ExactDelivery {
    offer: OfferBegin,
    ack: StructuralAck,
    packet_frames: Vec<Vec<(CandidateSnapshotFrameKind, u32)>>,
    contains_recursive_green_leaf: bool,
    contains_recursive_green_branch: bool,
}

struct OrdinaryCancellationFixture {
    profile: SourceFactsScanProfile,
    parser_profile: ParserProfileId,
    binding: SessionBinding,
    base_source: String,
    base_version: flark_engine::SourceVersion,
    base_ack: StructuralAck,
    initial_persistent_resident_nodes: usize,
    runtime: DocumentRuntime,
    endpoint: CandidateEndpoint,
    host: NativeCandidateHost,
}

impl OrdinaryCancellationFixture {
    fn new(document_session: [u32; 4]) -> Self {
        let profile = SourceFactsScanProfile::new(2).expect("dense test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session,
            source_session_identity: document_session[3] + 1,
            worker_generation: 1,
        };
        let base_source: String = (0..1_024)
            .map(|ordinal| format!("ordinary prose line {ordinal:04} {}\n", "a".repeat(40)))
            .collect();
        let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
            .expect("ordinary cancellation runtime");
        let (certified, base_completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let base_version = certified.source();
        while !runtime.poll_retirement(256).complete {}
        let initial_persistent_resident_nodes = runtime.arena_metrics().resident_nodes;
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, base_completion)
            .expect("start clean ordinary base candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent candidate host");
        host.observe_source_version(source_version_for(binding, base_completion))
            .expect("host observes ordinary base");
        let base_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
        Self {
            profile,
            parser_profile,
            binding,
            base_source,
            base_version,
            base_ack: base_delivery.ack,
            initial_persistent_resident_nodes,
            runtime,
            endpoint,
            host,
        }
    }

    fn edit_offset(&self, line_ordinal: usize) -> usize {
        let prefix = format!("ordinary prose line {line_ordinal:04} ");
        self.base_source
            .find(&prefix)
            .expect("fixture line")
            .checked_add(prefix.len() + 20)
            .expect("bounded fixture offset")
    }

    fn start_target(
        &mut self,
        edit_start: usize,
        replacement: &str,
        certification_id: u32,
        ui_revision: u32,
    ) -> flark_engine::SourceVersion {
        let current = self
            .runtime
            .current_source_version()
            .expect("current fixture source");
        let target = self
            .runtime
            .apply_edit(current, edit_start..edit_start + 1, replacement)
            .expect("apply ordinary target edit")
            .source()
            .current();
        let plan = self
            .runtime
            .begin_incremental_source_facts(
                self.profile,
                self.parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan bounded SourceFacts replacement");
        assert_eq!(
            plan.base(),
            self.base_version,
            "an uncommitted cancelled target must roll back to the installed base"
        );
        assert!(
            self.endpoint
                .has_incremental_base_for_plan(&self.runtime, &plan)
                .expect("preflight ordinary crop"),
            "replacement must retain exact parser authority for the original base"
        );
        let witness = complete_incremental_source_facts(&mut self.runtime);
        let target_lease = self
            .runtime
            .snapshot_current_source()
            .expect("borrow exact target source");
        let completion =
            completion_for_persistent_target(&self.runtime, certification_id, ui_revision);
        self.host
            .observe_source_version(source_version_for(self.binding, completion))
            .expect("host observes target source");
        self.endpoint
            .start_incremental(
                &self.runtime,
                target_lease,
                witness,
                self.binding,
                completion,
            )
            .expect("start authenticated ordinary crop candidate");
        target
    }

    fn assert_original_base_restored(&self) {
        let retained = self
            .endpoint
            .retained
            .as_ref()
            .expect("cancelled target restores retained base");
        assert_eq!(retained.ack, self.base_ack);
        assert_eq!(
            retained
                .restart
                .as_ref()
                .expect("restored parser restart")
                .source(),
            self.base_version
        );
        assert!(self
            .endpoint
            .has_exact_base_for(&self.runtime, self.base_version)
            .expect("inspect restored exact base"));
    }

    fn deliver_replacement(&mut self, target: flark_engine::SourceVersion) -> ExactDelivery {
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut self.endpoint,
            &mut self.runtime,
            &mut self.host,
        );
        assert!(
            delivery.contains_recursive_green_leaf,
            "the converged replacement must carry definitive recursive-Green authority"
        );
        match delivery.offer.mode {
            PublicationMode::ExactBaseDelta | PublicationMode::ExactBaseReferencesDelta => {
                assert_eq!(delivery.offer.base_ack, Some(self.base_ack));
            }
            PublicationMode::FullSnapshot => {
                assert_eq!(delivery.offer.base_ack, None);
            }
        }
        assert!(delivery.ack.host_revision > self.base_ack.host_revision);
        assert!(
            !self
                .runtime
                .commit_persistent_source_facts_delta(target)
                .expect("inspect delivered SourceFacts transaction"),
            "the delivery helper must mirror production and commit before returning"
        );
        drain_candidate_cleanup(&mut self.endpoint, &mut self.runtime);
        assert!(self
            .endpoint
            .has_exact_base_for(&self.runtime, target)
            .expect("replacement becomes exact base"));
        delivery
    }
}

fn deliver_endpoint_to_independent_host_with_unit_fuel(
    endpoint: &mut CandidateEndpoint,
    runtime: &mut DocumentRuntime,
    host: &mut NativeCandidateHost,
) -> ExactDelivery {
    deliver_endpoint_to_independent_host_with_fuel(endpoint, runtime, host, 1)
}

fn deliver_endpoint_to_independent_host_with_fuel(
    endpoint: &mut CandidateEndpoint,
    runtime: &mut DocumentRuntime,
    host: &mut NativeCandidateHost,
    fuel: usize,
) -> ExactDelivery {
    assert!(fuel > 0);
    let host_transitions = u32::try_from(fuel).expect("bounded host fuel");
    let mut next_event_id = 1_u32;
    let mut pending_event = None;
    let mut offer = None;
    let mut committed = None;
    let mut packet_frames = Vec::new();
    let mut contains_recursive_green_leaf = false;
    let mut contains_recursive_green_branch = false;
    for _ in 0..1_000_000 {
        let event = match pending_event.take() {
            Some(event) => event,
            None => match endpoint.poll(runtime, fuel).unwrap_or_else(|error| {
                panic!(
                    "fuelled producer poll in phase {} (cleanup={}): {error:?}",
                    endpoint.active_phase_for_test(),
                    endpoint.cleanup.is_some(),
                )
            }) {
                CandidatePoll::Pending { transitions } => {
                    assert!(
                        (1..=fuel).contains(&transitions),
                        "a ready fuelled candidate poll must make bounded progress or emit"
                    );
                    continue;
                }
                CandidatePoll::Event { transitions, event } => {
                    assert!(transitions <= fuel);
                    *event
                }
                CandidatePoll::HotInlineEvent { .. } => {
                    panic!("structural delivery emitted a hot-inline event")
                }
                CandidatePoll::ViewportPresentationEvent { .. } => {
                    panic!("structural delivery emitted a viewport event")
                }
                CandidatePoll::ViewportPresentationUnavailable { .. } => {
                    panic!("structural delivery emitted viewport unavailability")
                }
            },
        };
        let event_id = next_event_id;
        next_event_id = next_event_id.checked_add(1).expect("test event id");
        let CandidateEvent { credit, body } = event;
        match body {
            CandidateEventBody::Begin(begin) => {
                host.begin_offer(begin)
                    .expect("independent host begins offer");
                endpoint
                    .accept_credit(credit, event_id)
                    .expect("producer accepts Begin credit");
                offer = Some(begin);
            }
            CandidateEventBody::Packet { encoded } => {
                let packet = decode_publication_packet(&encoded).expect("decode producer packet");
                let offer_id = packet.offer_id;
                let frames = packet
                    .frames()
                    .map(|frame| {
                        let frame = frame.expect("validated producer frame");
                        contains_recursive_green_leaf |=
                            frame.bytes.windows(4).any(|window| window == b"RGL1");
                        contains_recursive_green_branch |=
                            frame.bytes.windows(4).any(|window| window == b"RGB1");
                        let kind = match M11CandidateHost::classify_frame(frame.bytes)
                            .expect("independent frame classification")
                            .kind
                        {
                            M11HostFrameKind::Begin => CandidateSnapshotFrameKind::Begin,
                            M11HostFrameKind::SourceFactsReplacementPage => {
                                CandidateSnapshotFrameKind::SourceFactsReplacementPage
                            }
                            M11HostFrameKind::BlockSequenceReplacementPage => {
                                CandidateSnapshotFrameKind::BlockSequenceReplacementPage
                            }
                            M11HostFrameKind::RecursiveGreenReplacementPage => {
                                CandidateSnapshotFrameKind::RecursiveGreenReplacementPage
                            }
                            M11HostFrameKind::Node => CandidateSnapshotFrameKind::Node,
                            M11HostFrameKind::End => CandidateSnapshotFrameKind::End,
                        };
                        (kind, frame.record_count)
                    })
                    .collect::<Vec<_>>();
                host.admit_packet(packet)
                    .expect("independent host admits packet");
                endpoint
                    .accept_credit(credit, event_id)
                    .expect("producer accepts packet event credit");
                let (credited_offer_id, next_frame_ordinal) = loop {
                    match host
                        .poll(HostWorkGrant {
                            inspect_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
                            copy_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
                            transitions: host_transitions,
                        })
                        .expect("fuelled host packet poll")
                    {
                        NativeHostPollOutcome::Pending => {}
                        NativeHostPollOutcome::PacketCredit {
                            offer_id,
                            next_frame_ordinal,
                        } => break (offer_id, next_frame_ordinal),
                        outcome => panic!("unexpected packet outcome: {outcome:?}"),
                    }
                };
                packet_frames.push(frames);
                assert!(endpoint
                    .handle_host_poll(
                        event_id,
                        offer_id,
                        HostPollPhase::PacketCredit,
                        HostPollResult::Completed(HostPollOutcome::PacketCredit {
                            offer_id: credited_offer_id,
                            next_frame_ordinal,
                        }),
                    )
                    .expect("producer accepts exact host packet credit")
                    .is_none());
            }
            CandidateEventBody::Commit(commit) => {
                host.request_commit(commit)
                    .expect("independent host accepts commit");
                endpoint
                    .accept_credit(credit, event_id)
                    .expect("producer accepts commit event credit");
                let outcome = loop {
                    match host
                        .poll(HostWorkGrant {
                            inspect_bytes: 0,
                            copy_bytes: 0,
                            transitions: host_transitions,
                        })
                        .expect("fuelled host install poll")
                    {
                        NativeHostPollOutcome::Pending => {}
                        outcome @ NativeHostPollOutcome::Committed(_) => break outcome,
                        outcome => panic!("unexpected commit outcome: {outcome:?}"),
                    }
                };
                let NativeHostPollOutcome::Committed(ack) = outcome else {
                    unreachable!("matched above")
                };
                committed = Some(ack);
                pending_event = endpoint
                    .handle_host_poll(
                        event_id,
                        commit.offer_id,
                        HostPollPhase::Commit,
                        HostPollResult::Completed(HostPollOutcome::Committed(ack)),
                    )
                    .expect("producer accepts exact host commit");
            }
            CandidateEventBody::DeliveryAcknowledged(ack) => {
                assert_eq!(committed, Some(ack));
                let target = runtime
                    .current_source_version()
                    .expect("delivered target source");
                runtime
                    .commit_persistent_source_facts_delta(target)
                    .expect("commit delivered SourceFacts target");
                host.acknowledge_delivery(ack)
                    .expect("independent host accepts delivery");
                endpoint
                    .accept_credit(credit, event_id)
                    .expect("producer accepts delivery credit");
                return ExactDelivery {
                    offer: offer.expect("producer emitted Begin"),
                    ack,
                    packet_frames,
                    contains_recursive_green_leaf,
                    contains_recursive_green_branch,
                };
            }
        }
    }
    panic!("fuelled candidate delivery did not complete");
}

fn close_exact_pair_to_zero(
    endpoint: &mut CandidateEndpoint,
    runtime: &mut DocumentRuntime,
    host: &mut NativeCandidateHost,
) {
    close_exact_pair_to_zero_with_fuel(endpoint, runtime, host, 1);
}

fn close_exact_pair_to_zero_with_fuel(
    endpoint: &mut CandidateEndpoint,
    runtime: &mut DocumentRuntime,
    host: &mut NativeCandidateHost,
    fuel: usize,
) {
    assert!(fuel > 0);
    let host_transitions = u32::try_from(fuel).expect("bounded host close fuel");
    endpoint.begin_close().expect("begin producer close");
    for _ in 0..1_000_000 {
        if !endpoint.cleanup_pending() {
            break;
        }
        let poll = endpoint
            .poll(runtime, fuel)
            .expect("fuelled producer close");
        assert!(matches!(poll, CandidatePoll::Pending { transitions } if transitions <= fuel));
    }
    assert!(!endpoint.cleanup_pending());
    runtime.begin_close().expect("begin runtime close");
    for _ in 0..1_000_000 {
        if runtime
            .poll_close(fuel)
            .expect("fuelled runtime close")
            .complete
        {
            break;
        }
    }
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);

    host.begin_close().expect("begin independent host close");
    for _ in 0..1_000_000 {
        match host
            .poll(HostWorkGrant {
                inspect_bytes: 0,
                copy_bytes: 0,
                transitions: host_transitions,
            })
            .expect("fuelled independent host close")
        {
            NativeHostPollOutcome::Pending => {}
            NativeHostPollOutcome::Closed => break,
            outcome => panic!("unexpected close outcome: {outcome:?}"),
        }
    }
    assert!(host.is_removable());
}

#[test]
fn clean_cm321_schema9_point_and_schema10_viewport_are_typed_and_exact() {
    const CM321: &str = "- a\n  > **b** and _c_\n  ```\n  code\n  ```\n- **d**\n";
    let profile = SourceFactsScanProfile::new(4).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [321, 322, 323, 324],
        source_session_identity: 325,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(CM321, standard_document_runtime_config()).expect("CM321 runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start CM321 candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("CM321 host");
    host.observe_source_version(source_version_for(binding, completion))
        .expect("host observes CM321 source");

    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);

    assert_eq!(delivery.offer.mode, PublicationMode::FullSnapshot);
    assert!(
        delivery.contains_recursive_green_leaf,
        "clean CM321 candidate must transport an RGL1 recursive-Green leaf"
    );
    assert_eq!(
        endpoint
            .retained
            .as_ref()
            .expect("installed parser publication")
            .ack,
        delivery.ack
    );
    assert!(
        endpoint
            .recursive_green
            .has_installed_session_for(delivery.ack),
        "installed recursive-Green session must retain the delivered ACK"
    );

    let source_version = source_version_for(binding, completion);
    let selected_byte = CM321.find('b').expect("nested strong content");
    let neighbor_byte = CM321.rfind('d').expect("neighbor Paragraph content");
    let selected_frame =
        recursive_green_owner_frame(&host, source_version, selected_byte, selected_byte);
    let neighbor_frame =
        recursive_green_owner_frame(&host, source_version, neighbor_byte, neighbor_byte);
    assert_ne!(selected_frame, neighbor_frame);

    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    endpoint
        .request_hot_inline(
            &mut runtime,
            InlineRefinementCommand {
                binding,
                refinement_generation: 1,
                source_version,
                base_ack: delivery.ack,
                byte_offset: u32::try_from(selected_byte).expect("selected byte"),
                utf16_offset: u32::try_from(selected_byte).expect("ASCII selected UTF-16"),
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::RecursiveGreenParagraph,
            },
        )
        .expect("request fresh schema-9 Green Paragraph sidecar");
    let pending = commit_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        32_100,
    );
    assert_eq!(pending.hio1_schema, 4, "Green owner uses typed HIO1 schema");
    assert_eq!(
        pending.begin.binding.owner(),
        Some(HotInlineSidecarOwner::RecursiveGreenFrame(selected_frame))
    );
    assert_ne!(pending.begin.binding.block_ordinal & (1_u64 << 63), 0);
    assert_eq!(
        pending.ack.block_ordinal,
        pending.begin.binding.block_ordinal
    );
    assert!(matches!(
        pending.begin.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative { fact_count, .. }
            if fact_count >= 2
    ));

    host.acknowledge_inline_sidecar_delivery(pending.ack)
        .expect("host acknowledges exact Green sidecar");
    endpoint
        .accept_hot_inline_credit(pending.credit, pending.event_id)
        .expect("producer accepts Green sidecar delivery");

    let mut facts = [0_u8; 8 * M11_INLINE_FACT_RECORD_BYTES];
    let fact_count = match host
        .query_inline_sidecar(pending.begin.binding, &mut facts)
        .expect("query installed Green Paragraph sidecar")
    {
        HostInlineSidecarQueryOutcome::Authoritative { fact_count, .. } => fact_count,
        outcome => panic!("Green Paragraph sidecar must be authoritative: {outcome:?}"),
    };
    let kinds = facts
        .chunks_exact(M11_INLINE_FACT_RECORD_BYTES)
        .take(fact_count as usize)
        .map(|record| record[0])
        .collect::<Vec<_>>();
    assert!(kinds.contains(&(M11InlineProjectionKind::Strong as u8)));
    assert!(kinds.contains(&(M11InlineProjectionKind::Emphasis as u8)));

    let mut neighbor_binding = pending.begin.binding;
    neighbor_binding.block_ordinal = HotInlineSidecarOwner::RecursiveGreenFrame(neighbor_frame)
        .into_wire()
        .expect("neighbor frame fits owner slot");
    assert!(matches!(
        host.query_inline_sidecar(neighbor_binding, &mut facts)
            .expect("query neighboring Green owner"),
        HostInlineSidecarQueryOutcome::Unavailable
    ));

    let mut stale = pending.begin;
    stale.offer_id[0] ^= 0x8000_0000;
    stale.publication_session[0] ^= 0x4000_0000;
    stale.binding.refinement_generation = 2;
    stale.base_ack.host_revision = stale
        .base_ack
        .host_revision
        .checked_add(1)
        .expect("stale test revision");
    assert_eq!(
        host.begin_inline_sidecar_offer(stale)
            .expect_err("stale structural ACK cannot attach")
            .reason(),
        crate::v3_host_store::HostRejectReason::BaseMismatch
    );

    for _ in 0..10_000 {
        if !endpoint.hot_inline_has_poll_work() {
            break;
        }
        assert!(
            endpoint
                .poll_hot_inline(&mut runtime, 1)
                .expect("release delivered CM321 point sidecar")
                <= 1
        );
    }
    assert!(!endpoint.hot_inline_has_poll_work());

    let row_window = endpoint
        .recursive_green
        .installed_session(delivery.ack)
        .expect("CM321 Green session remains exact-current")
        .query_renderable_rows(
            &runtime,
            M11RecursiveGreenPoint::new(
                selected_byte,
                selected_byte,
                SourceBoundaryAffinity::After,
            ),
            u64::try_from(CM321.len()).expect("bounded CM321 end"),
            M11RecursiveGreenRowQueryLimits::new(8, 128, 65_536, 64, 65_536)
                .expect("nonzero CM321 row limits"),
        )
        .expect("query CM321 Green rows");
    assert!(row_window.rows().len() >= 2);
    let paragraph_row = &row_window.rows()[0];
    let fence_row = &row_window.rows()[1];
    assert_eq!(paragraph_row.kind().get(), 5);
    assert_eq!(fence_row.kind().get(), 7);
    let paragraph_physical = paragraph_row.physical_range();
    let paragraph_physical_utf16 = paragraph_row.physical_utf16_range();
    let paragraph_editable = paragraph_row
        .editable_range()
        .expect("CM321 Paragraph has contiguous byte edit authority");
    let paragraph_editable_utf16 = paragraph_row
        .editable_utf16_range()
        .expect("CM321 Paragraph has contiguous UTF-16 edit authority");
    assert_eq!(paragraph_physical, 8..22);
    assert_eq!(paragraph_physical_utf16, 8..22);
    assert_eq!(paragraph_editable, 8..21);
    assert_eq!(paragraph_editable_utf16, 8..21);

    let HostStructuralOrdinalWindowOutcome::Window {
        total_entry_count,
        start_entry_ordinal,
        next_entry_ordinal,
        start,
        next,
        complete,
        receipt: ordinal_receipt,
        ..
    } = host
        .query_structural_ordinal_window(HostStructuralOrdinalWindowQuery {
            source_version,
            start_entry_ordinal: paragraph_row.ordinal(),
            budget: HostStructuralOrdinalWindowBudget {
                maximum_entries: 3,
                maximum_storage_pages_visited: 8,
                maximum_tree_nodes_visited: 128,
                maximum_packed_entries_inspected: 1024,
            },
        })
        .expect("query CM321 Green ordinal window")
    else {
        panic!("CM321 Green row ordinals must map to exact source cuts");
    };
    assert_eq!(total_entry_count, 4);
    assert_eq!(start_entry_ordinal, paragraph_row.ordinal());
    assert_eq!(next_entry_ordinal, 4);
    assert_eq!(start, HostSourceMetric { bytes: 8, utf16: 8 });
    assert_eq!(
        next,
        HostSourceMetric {
            bytes: CM321.len() as u32,
            utf16: CM321.len() as u32,
        }
    );
    assert!(complete);
    assert!(ordinal_receipt.storage_pages_visited <= 8);
    assert!(ordinal_receipt.tree_nodes_visited <= 128);
    assert!(ordinal_receipt.packed_entries_inspected <= 1024);

    let requested_range = HostMetricRange { start, end: next };
    let mut row_bytes = vec![0xa5_u8; 16 * 1024];
    let HostBlockRangeOutcome::Page {
        requested_range: observed_request,
        covered_range,
        continuation,
        receipt,
        ..
    } = host
        .query_structural_range(
            HostBlockRangeQuery {
                source_version,
                requested_range,
                budget: HostBlockRangeBudget {
                    maximum_encoded_bytes: row_bytes.len() as u32,
                    maximum_block_count: 8,
                    maximum_storage_pages_visited: 128,
                    maximum_open_depth: 64,
                    maximum_tree_nodes_visited: 65_536,
                },
                continuation: None,
            },
            &mut row_bytes,
        )
        .expect("query schema-10 CM321 row directory")
    else {
        panic!("CM321 row directory must be exact-current");
    };
    assert_eq!(observed_request, requested_range);
    assert_eq!(
        covered_range,
        HostMetricRange {
            start: HostSourceMetric { bytes: 8, utf16: 8 },
            end: HostSourceMetric {
                bytes: CM321.len() as u32,
                utf16: CM321.len() as u32,
            },
        }
    );
    assert!(continuation.is_none());
    assert!(receipt.complete);
    assert_eq!(receipt.block_count, 3);
    assert!(receipt.storage_pages_visited <= 128);
    assert!(receipt.open_depth <= 64);
    assert!(receipt.tree_nodes_visited <= 65_536);
    assert!(receipt.packed_entries_inspected <= 128 * 128);

    let single_row_request = HostMetricRange {
        start: HostSourceMetric { bytes: 8, utf16: 8 },
        end: HostSourceMetric {
            bytes: 22,
            utf16: 22,
        },
    };
    let mut single_row_bytes = vec![0xa5_u8; 16 * 1024];
    let HostBlockRangeOutcome::Page {
        continuation: single_row_continuation,
        receipt: single_row_receipt,
        ..
    } = host
        .query_structural_range(
            HostBlockRangeQuery {
                source_version,
                requested_range: single_row_request,
                budget: HostBlockRangeBudget {
                    maximum_encoded_bytes: single_row_bytes.len() as u32,
                    maximum_block_count: 1,
                    maximum_storage_pages_visited: 128,
                    maximum_open_depth: 64,
                    maximum_tree_nodes_visited: 65_536,
                },
                continuation: None,
            },
            &mut single_row_bytes,
        )
        .expect("query exact nonterminal CM321 Green row")
    else {
        panic!("one exact nonterminal Green row must be a complete range page");
    };
    assert!(single_row_continuation.is_none());
    assert!(single_row_receipt.complete);
    assert_eq!(
        u32::from_le_bytes(
            single_row_bytes[32..36]
                .try_into()
                .expect("single-row completion flag"),
        ),
        1,
    );

    let read_u16 = |offset: usize| {
        u16::from_le_bytes(row_bytes[offset..offset + 2].try_into().expect("wire u16"))
    };
    let read_u32 = |offset: usize| {
        u32::from_le_bytes(row_bytes[offset..offset + 4].try_into().expect("wire u32"))
    };
    let read_u64 = |offset: usize| {
        u64::from_le_bytes(row_bytes[offset..offset + 8].try_into().expect("wire u64"))
    };
    assert_eq!(&row_bytes[..8], b"FLKVR001");
    assert_eq!(read_u32(8), HOST_RECURSIVE_GREEN_ROW_RANGE_SCHEMA);
    assert_eq!(
        read_u32(12),
        HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES as u32
    );
    assert_eq!(read_u32(16), HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES as u32);
    assert_eq!(
        read_u32(20),
        HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES as u32
    );
    assert_eq!(read_u32(24), 3);
    assert_eq!(read_u32(28), 13);
    assert_eq!(read_u32(32), 1);
    assert_eq!(read_u32(36), 0);
    assert_eq!(read_u64(40), paragraph_row.ordinal());
    assert_eq!(read_u64(48), 4);
    assert_eq!(read_u32(56), 1);
    assert_eq!(read_u32(60), 1);
    assert_eq!(read_u32(64), delivery.ack.source_version.revision);
    assert_eq!(read_u32(68), delivery.ack.parse_generation);
    for (index, word) in delivery.ack.publication_session.into_iter().enumerate() {
        assert_eq!(read_u32(72 + index * 4), word);
    }
    assert_eq!(
        receipt.encoded_bytes as usize,
        HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES
            + 3 * HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES
            + 13 * HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES
    );

    let row = HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES;
    assert_eq!(read_u64(row), paragraph_row.ordinal());
    assert_eq!(read_u64(row + 8), selected_frame);
    assert_eq!(read_u16(row + 16), 5);
    assert_eq!(read_u16(row + 18), 0b11);
    assert_eq!(read_u32(row + 20), 0);
    assert_eq!(read_u32(row + 24), 5);
    assert_eq!(read_u16(row + 28), 1);
    assert_eq!(read_u16(row + 30), 1);
    assert_eq!(
        (
            read_u32(row + 32),
            read_u32(row + 36),
            read_u32(row + 40),
            read_u32(row + 44)
        ),
        (8, 8, 22, 22)
    );
    assert_eq!(
        (
            read_u32(row + 48),
            read_u32(row + 52),
            read_u32(row + 56),
            read_u32(row + 60)
        ),
        (8, 8, 21, 21)
    );

    let paths =
        HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES + 3 * HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES;
    let path_offset = |index: usize| paths + index * HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES;
    let kinds = (0..5)
        .map(|index| read_u16(path_offset(index) + 8))
        .collect::<Vec<_>>();
    assert_eq!(kinds, vec![1, 3, 4, 2, 5]);
    let list = path_offset(1);
    assert_eq!(read_u16(list + 10), 0b1110);
    assert_eq!(read_u16(list + 12), 1);
    assert_eq!(
        (
            read_u32(list + 32),
            read_u32(list + 36),
            read_u32(list + 40),
            read_u32(list + 44),
        ),
        (1, u32::from(b'-'), 1, 1)
    );
    let item = path_offset(2);
    assert_eq!(read_u16(item + 10), 0b0110);
    assert_eq!(read_u16(item + 12), 2);
    assert_eq!((read_u32(item + 32), read_u32(item + 36)), (0, 2));
    let quote = path_offset(3);
    assert_eq!(read_u16(quote + 8), 2);
    assert_eq!(read_u16(quote + 10), 0b0010);
    assert_eq!(read_u16(quote + 12), 0);
    let owner = path_offset(4);
    assert_eq!(read_u16(owner + 8), 5);
    assert_eq!(read_u16(owner + 10), 0b0001);
    assert_eq!(
        (
            read_u32(owner + 16),
            read_u32(owner + 20),
            read_u32(owner + 24),
            read_u32(owner + 28)
        ),
        (8, 8, 22, 22)
    );
    let full_row_window = endpoint
        .recursive_green
        .installed_session(delivery.ack)
        .expect("CM321 Green session remains exact-current")
        .query_renderable_rows(
            &runtime,
            M11RecursiveGreenPoint::new(0, 0, SourceBoundaryAffinity::After),
            u64::try_from(CM321.len()).expect("bounded CM321 end"),
            M11RecursiveGreenRowQueryLimits::new(8, 128, 65_536, 64, 65_536)
                .expect("nonzero CM321 full-row limits"),
        )
        .expect("query full CM321 Green rows");
    assert_eq!(full_row_window.start_ordinal(), 0);
    assert_eq!(full_row_window.rows().len(), 4);
    assert_eq!(
        full_row_window
            .rows()
            .iter()
            .map(|row| row.kind().get())
            .collect::<Vec<_>>(),
        vec![5, 5, 7, 5]
    );
    let first_row = &full_row_window.rows()[0];
    let last_row = &full_row_window.rows()[3];
    let first_physical = first_row.physical_range();
    let first_physical_utf16 = first_row.physical_utf16_range();
    let last_physical = last_row.physical_range();
    let last_physical_utf16 = last_row.physical_utf16_range();
    let viewport_command = ViewportInlineBatchCommand {
        binding,
        viewport_generation: 1,
        source_version,
        base_ack: delivery.ack,
        start_entry_ordinal: first_row.ordinal(),
        start_byte_offset: u32::try_from(first_physical.start).expect("bounded first-row start"),
        start_utf16_offset: u32::try_from(first_physical_utf16.start)
            .expect("bounded first-row UTF-16 start"),
        end_byte_offset: u32::try_from(last_physical.end).expect("bounded last-row end"),
        end_utf16_offset: u32::try_from(last_physical_utf16.end)
            .expect("bounded last-row UTF-16 end"),
        limits: ViewportInlineBatchLimits {
            maximum_structural_entries: 4,
            maximum_storage_pages: 25,
            maximum_inline_leaves: 3,
            maximum_inline_leaf_source_bytes: 8 * 1024,
            maximum_inline_source_bytes: 64 * 1024,
            maximum_fact_records: 2_048,
            maximum_projection_bytes: 2 * 1024 * 1024,
            maximum_parser_transitions: 250_000,
        },
    };
    endpoint
        .request_viewport_inline_batch(&runtime, viewport_command)
        .expect("request schema-10 CM321 Green viewport");
    for _ in 0..100_000 {
        if matches!(
            endpoint.viewport_inline_batch,
            Some(ViewportInlineBatchState::Ready(_))
        ) {
            break;
        }
        assert!(
            endpoint
                .poll_viewport_inline_batch(&mut runtime, 1)
                .expect("poll CM321 Green viewport")
                <= 1
        );
    }
    let Some(ViewportInlineBatchState::Ready(ready)) = endpoint.viewport_inline_batch.as_ref()
    else {
        panic!("CM321 Green viewport did not become ready");
    };
    assert_eq!(ready.range_receipt.visited_rows, 4);
    assert_eq!(
        ready
            .leaves
            .iter()
            .map(|leaf| leaf.geometry.entry_ordinal)
            .collect::<Vec<_>>(),
        vec![0, 1, 3],
        "only the three Paragraph rows have HIO1 children"
    );
    let geometry = &ready
        .leaves
        .iter()
        .find(|leaf| leaf.geometry.entry_ordinal == paragraph_row.ordinal())
        .expect("nested Paragraph child remains present")
        .geometry;
    assert_eq!(geometry.entry_ordinal, paragraph_row.ordinal());
    assert_eq!(geometry.frame, paragraph_row.frame());
    assert_eq!(
        geometry.block_source,
        u32::try_from(paragraph_physical.start).unwrap()
            ..u32::try_from(paragraph_physical.end).unwrap()
    );
    assert_eq!(
        geometry.block_source_utf16,
        u32::try_from(paragraph_physical_utf16.start).unwrap()
            ..u32::try_from(paragraph_physical_utf16.end).unwrap()
    );
    assert_eq!(
        geometry.inline_source,
        u32::try_from(paragraph_editable.start).unwrap()
            ..u32::try_from(paragraph_editable.end).unwrap()
    );
    assert_eq!(
        geometry.inline_source_utf16,
        u32::try_from(paragraph_editable_utf16.start).unwrap()
            ..u32::try_from(paragraph_editable_utf16.end).unwrap()
    );
    let (viewport_begin, viewport_ack, authoritative, unsupported, child_closures) =
        deliver_viewport_presentation_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(viewport_begin.envelope.visited_structural_entries, 4);
    assert_eq!(viewport_begin.envelope.ordered_leaf_count, 3);
    assert_eq!(
        viewport_begin.binding.start.block_ordinal,
        first_row.ordinal()
    );
    assert_eq!(
        viewport_begin.binding.next.block_ordinal,
        last_row.ordinal() + 1
    );
    assert_eq!(authoritative, 3);
    assert_eq!(unsupported, 0);
    assert_eq!(child_closures, 3);
    assert_eq!(viewport_ack.base_ack, delivery.ack);
    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

fn recursive_green_query_shape(
    host: &NativeCandidateHost,
    source_version: SourceVersion,
    byte_offset: usize,
    utf16_offset: usize,
) -> (u16, [u32; 4], Vec<u16>) {
    recursive_green_query_shape_with_tree_budget(
        host,
        source_version,
        byte_offset,
        utf16_offset,
        256,
    )
}

fn recursive_green_query_shape_with_tree_budget(
    host: &NativeCandidateHost,
    source_version: SourceVersion,
    byte_offset: usize,
    utf16_offset: usize,
    maximum_tree_nodes_visited: u32,
) -> (u16, [u32; 4], Vec<u16>) {
    let mut output = vec![0_u8; 4 * 1024];
    let outcome = host
        .query_structural(
            HostPointQuery {
                source_version,
                position: HostSourceMetric {
                    bytes: u32::try_from(byte_offset).expect("test byte point"),
                    utf16: u32::try_from(utf16_offset).expect("test UTF-16 point"),
                },
                affinity: HostMetricAffinity::Downstream,
                budget: HostQueryBudget {
                    maximum_encoded_bytes: 4 * 1024,
                    maximum_open_depth: 16,
                    maximum_leaf_count: 64,
                    maximum_tree_nodes_visited,
                },
            },
            &mut output,
        )
        .expect("query recursive-Green viewport");
    let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
        panic!("recursive-Green query returned a gap: {outcome:?}");
    };
    let encoded_bytes = usize::try_from(receipt.encoded_bytes).expect("viewport bytes");
    assert!(encoded_bytes >= 112);
    assert_eq!(
        u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
        9
    );
    let ancestry_count = usize::try_from(u32::from_le_bytes(
        output[36..40].try_into().expect("ancestry count"),
    ))
    .expect("ancestry count fits");
    assert_eq!(encoded_bytes, 112 + ancestry_count * 16);
    let owner_kind = u16::from_le_bytes(output[44..46].try_into().expect("owner kind"));
    let range = [
        u32::from_le_bytes(output[48..52].try_into().expect("byte start")),
        u32::from_le_bytes(output[52..56].try_into().expect("byte end")),
        u32::from_le_bytes(output[56..60].try_into().expect("UTF-16 start")),
        u32::from_le_bytes(output[60..64].try_into().expect("UTF-16 end")),
    ];
    let ancestry = (0..ancestry_count)
        .map(|index| {
            let start = 112 + index * 16;
            u16::from_le_bytes(
                output[start + 8..start + 10]
                    .try_into()
                    .expect("ancestor kind"),
            )
        })
        .collect();
    (owner_kind, range, ancestry)
}

fn recursive_green_owner_frame(
    host: &NativeCandidateHost,
    source_version: SourceVersion,
    byte_offset: usize,
    utf16_offset: usize,
) -> u64 {
    let mut output = vec![0_u8; 4 * 1024];
    let outcome = host
        .query_structural(
            HostPointQuery {
                source_version,
                position: HostSourceMetric {
                    bytes: u32::try_from(byte_offset).expect("test byte point"),
                    utf16: u32::try_from(utf16_offset).expect("test UTF-16 point"),
                },
                affinity: HostMetricAffinity::Downstream,
                budget: HostQueryBudget {
                    maximum_encoded_bytes: 4 * 1024,
                    maximum_open_depth: 16,
                    maximum_leaf_count: 64,
                    maximum_tree_nodes_visited: 256,
                },
            },
            &mut output,
        )
        .expect("query recursive-Green owner");
    let HostStructuralQueryOutcome::Viewport { .. } = outcome else {
        panic!("recursive-Green owner query returned a gap: {outcome:?}");
    };
    assert_eq!(
        u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
        9
    );
    let ancestry_count = usize::try_from(u32::from_le_bytes(
        output[36..40].try_into().expect("ancestry count"),
    ))
    .expect("ancestry count fits");
    let owner_index = usize::try_from(u32::from_le_bytes(
        output[40..44].try_into().expect("owner index"),
    ))
    .expect("owner index fits");
    assert!(owner_index < ancestry_count);
    let start = 112 + owner_index * 16;
    u64::from_le_bytes(output[start..start + 8].try_into().expect("owner frame ID"))
}

fn recursive_green_row_list_tightness(
    host: &NativeCandidateHost,
    source_version: SourceVersion,
    requested_range: HostMetricRange,
) -> Vec<u32> {
    let mut output = vec![0_u8; 8 * 1024];
    let HostBlockRangeOutcome::Page {
        continuation,
        receipt,
        ..
    } = host
        .query_structural_range(
            HostBlockRangeQuery {
                source_version,
                requested_range,
                budget: HostBlockRangeBudget {
                    maximum_encoded_bytes: output.len() as u32,
                    maximum_block_count: 1,
                    maximum_storage_pages_visited: 128,
                    maximum_open_depth: 16,
                    maximum_tree_nodes_visited: 4096,
                },
                continuation: None,
            },
            &mut output,
        )
        .expect("query exact recursive-Green row")
    else {
        panic!("exact recursive-Green row must be available");
    };
    assert!(continuation.is_none());
    assert!(receipt.complete);
    assert_eq!(receipt.block_count, 1);
    assert_eq!(&output[..8], b"FLKVR001");
    assert_eq!(
        u32::from_le_bytes(output[8..12].try_into().expect("row schema")),
        HOST_RECURSIVE_GREEN_ROW_RANGE_SCHEMA,
    );
    let row_count = usize::try_from(u32::from_le_bytes(
        output[24..28].try_into().expect("row count"),
    ))
    .expect("row count fits");
    let path_count = usize::try_from(u32::from_le_bytes(
        output[28..32].try_into().expect("path count"),
    ))
    .expect("path count fits");
    assert_eq!(row_count, 1);
    let row_offset = HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES;
    let path_start = usize::try_from(u32::from_le_bytes(
        output[row_offset + 20..row_offset + 24]
            .try_into()
            .expect("path start"),
    ))
    .expect("path start fits");
    let path_len = usize::try_from(u32::from_le_bytes(
        output[row_offset + 24..row_offset + 28]
            .try_into()
            .expect("path length"),
    ))
    .expect("path length fits");
    assert_eq!(path_start, 0);
    assert_eq!(path_len, path_count);
    let paths_offset = HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES
        + row_count * HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES;
    (0..path_count)
        .filter_map(|index| {
            let offset = paths_offset + index * HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES;
            let kind = u16::from_le_bytes(
                output[offset + 8..offset + 10]
                    .try_into()
                    .expect("path kind"),
            );
            if kind != 3 {
                return None;
            }
            assert_eq!(
                u16::from_le_bytes(
                    output[offset + 12..offset + 14]
                        .try_into()
                        .expect("List fact kind"),
                ),
                1,
            );
            Some(u32::from_le_bytes(
                output[offset + 44..offset + 48]
                    .try_into()
                    .expect("List tightness"),
            ))
        })
        .collect()
}

#[test]
fn nested_local_edit_preempts_legacy_parse_and_installs_exact_recursive_green_delta() {
    const CM321: &str = "- a\n  > b\n  ```\n  c\n  ```\n- d\n";
    const CM325: &str = "* foo\n  * bar\n\n  baz\n";
    const BYTE_DELTA: usize = "* βaz".len() - "baz".len();
    const UTF16_DELTA: usize = 5 - 3;
    let mut source = String::new();
    for ordinal in 0..9_000 {
        source.push_str(&format!(
                "Prefix paragraph {ordinal:05} carries enough ordinary source for sparse restart spacing.\n\n"
            ));
    }
    source.push_str(CM321);
    source.push('\n');
    let cm325_start = source.len();
    source.push_str(CM325);
    source.push('\n');
    for ordinal in 0..1_000 {
        source.push_str(&format!(
            "Trailing paragraph {ordinal:04} remains an unchanged serialized-Green sibling.\n\n"
        ));
    }
    assert!(source.len() > 512 * 1024);

    let profile = SourceFactsScanProfile::new(4).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [331, 332, 333, 334],
        source_session_identity: 335,
        worker_generation: 1,
    };
    let mut runtime = DocumentRuntime::new(&source, standard_document_runtime_config())
        .expect("large CM321 runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_source = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start CM321 base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("CM321 host");
    let base_wire_source = source_version_for(binding, base_completion);
    host.observe_source_version(base_wire_source)
        .expect("host observes CM321 base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

    let distant_byte = source
        .rfind("Trailing paragraph 0999")
        .expect("distant trailing sibling");
    let distant_utf16 = source[..distant_byte].encode_utf16().count();
    let distant_before =
        recursive_green_query_shape(&host, base_wire_source, distant_byte, distant_utf16);
    let base_baz_byte = cm325_start + CM325.find("baz").expect("lazy outer-item Paragraph");
    let base_baz =
        recursive_green_query_shape(&host, base_wire_source, base_baz_byte, base_baz_byte);
    assert_eq!(base_baz.0, 5);
    assert_eq!(base_baz.2, vec![1, 3, 4, 5]);
    let base_bar_start = cm325_start + CM325.find("bar").expect("nested List Paragraph");
    assert_eq!(
        recursive_green_row_list_tightness(
            &host,
            base_wire_source,
            HostMetricRange {
                start: HostSourceMetric {
                    bytes: base_bar_start as u32,
                    utf16: base_bar_start as u32,
                },
                end: HostSourceMetric {
                    bytes: (base_bar_start + 4) as u32,
                    utf16: (base_bar_start + 4) as u32,
                },
            },
        ),
        vec![0, 1],
        "the independent base host sees a loose outer List and tight nested List",
    );

    endpoint
        .cancel_for_edit(&mut runtime)
        .expect("prepare nested edit");
    let edited_byte = base_baz_byte;
    let edited_end = edited_byte + "baz".len();
    runtime
        .apply_edit(base_source, edited_byte..edited_end, "* βaz")
        .expect("turn the lazy outer-item Paragraph into a second nested-list Item");
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("begin incremental source facts");
    assert!(endpoint
        .has_incremental_base_for_plan(&runtime, &plan)
        .expect("exact-base preflight"));
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("target source lease");
    let target_source = target_lease.version();
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    let target_wire_source = source_version_for(binding, target_completion);
    host.observe_source_version(target_wire_source)
        .expect("host observes CM321 target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start nested exact candidate");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact",
        "the candidate waits on recursive-Green adoption before choosing its publication route"
    );

    while endpoint.recursive_green.target_work_pending() {
        let polled = endpoint
            .poll(&mut runtime, 1)
            .expect("advance only recursive-Green adoption");
        assert!(matches!(polled, CandidatePoll::Pending { transitions: 1 }));
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "AwaitingRecursiveGreenExact",
            "the scheduler must not poll a fallback parser before Green adoption resolves"
        );
    }
    assert!(endpoint
        .recursive_green
        .ready_update_for(base_delivery.ack, target_source)
        .is_some());
    let adoption_work = endpoint
        .recursive_green
        .ready_update_for(base_delivery.ack, target_source)
        .expect("completed CM325 structural update")
        .work();
    assert!(adoption_work.source_bytes_read() < 16 * 1024);
    assert!(adoption_work.green_tree_nodes_rebuilt() < 256);

    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
    assert!(target_delivery
        .packet_frames
        .iter()
        .flatten()
        .any(|(kind, _)| { *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage }));
    assert!(target_delivery
        .packet_frames
        .iter()
        .flatten()
        .all(|(kind, _)| { *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage }));
    assert!(target_delivery.contains_recursive_green_leaf);
    assert!(
        !target_delivery.contains_recursive_green_branch,
        "RGB1 branches must be rebuilt by the independent host, not transported"
    );
    assert_eq!(
        endpoint.recursive_green_path_receipt(),
        RecursiveGreenPathReceipt {
            local_adoption_deliveries: 1,
            clean_fallback_deliveries: 0,
        }
    );

    let edited_probe_byte = edited_byte + "* β".len();
    let edited_probe_utf16 = edited_byte + "* β".encode_utf16().count();
    assert_ne!(edited_probe_byte, edited_probe_utf16);
    let edited_shape = recursive_green_query_shape_with_tree_budget(
        &host,
        target_wire_source,
        edited_probe_byte,
        edited_probe_utf16,
        1024,
    );
    assert_eq!(edited_shape.0, 5);
    assert_eq!(
        edited_shape.1,
        [
            (edited_byte + 2) as u32,
            (edited_byte + 6) as u32,
            (edited_byte + 2) as u32,
            (edited_byte + 5) as u32,
        ]
    );
    assert_eq!(
        edited_shape.2,
        vec![1, 3, 4, 3, 4, 5],
        "the independent host must observe a second nested-list Item, not the old lazy Paragraph",
    );
    assert_eq!(
        recursive_green_row_list_tightness(
            &host,
            target_wire_source,
            HostMetricRange {
                start: HostSourceMetric {
                    bytes: (edited_byte + 2) as u32,
                    utf16: (edited_byte + 2) as u32,
                },
                end: HostSourceMetric {
                    bytes: (edited_byte + 7) as u32,
                    utf16: (edited_byte + 6) as u32,
                },
            },
        ),
        vec![1, 0],
        "the independent target host sees a tight outer List and loose nested List",
    );

    let distant_after = recursive_green_query_shape_with_tree_budget(
        &host,
        target_wire_source,
        distant_byte + BYTE_DELTA,
        distant_utf16 + UTF16_DELTA,
        1024,
    );
    assert_eq!(distant_after.0, distant_before.0);
    assert_eq!(distant_after.2, distant_before.2);
    assert_eq!(
        distant_after.1,
        [
            distant_before.1[0] + BYTE_DELTA as u32,
            distant_before.1[1] + BYTE_DELTA as u32,
            distant_before.1[2] + UTF16_DELTA as u32,
            distant_before.1[3] + UTF16_DELTA as u32,
        ],
    );
    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn small_nested_edit_uses_bounded_recursive_green_local_adoption() {
    const SOURCE: &str = "- a\n  > **b** and _c_\n  ```\n  code\n  ```\n- **d**\n";
    const MAXIMUM_REPLACEMENT_RECORDS: u32 = 64;
    let profile = SourceFactsScanProfile::new(4).expect("dense test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [341, 342, 343, 344],
        source_session_identity: 345,
        worker_generation: 1,
    };
    let mut runtime = DocumentRuntime::new(SOURCE, standard_document_runtime_config())
        .expect("small nested runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_source = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start small nested base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("small nested host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes small nested base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

    endpoint
        .cancel_for_edit(&mut runtime)
        .expect("prepare small nested edit");
    let caret = SOURCE.find("b**").expect("nested strong content") + 1;
    let caret_utf16 = SOURCE[..caret].encode_utf16().count();
    assert!(
        !endpoint
            .prepare_bullet_list_local_edit(&runtime, caret..caret, caret_utf16..caret_utf16)
            .expect("classify nested edit route"),
        "the nested quote edit is not admitted by the list-local lane"
    );
    runtime
        .apply_edit(base_source, caret..caret, "x")
        .expect("apply nested insertion");
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan small nested incremental facts");
    assert!(endpoint
        .has_incremental_base_for_plan(&runtime, &plan)
        .expect("small nested exact-base preflight"));
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("small nested target source");
    let target_source = target_lease.version();
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    let target_wire_source = source_version_for(binding, target_completion);
    host.observe_source_version(target_wire_source)
        .expect("host observes small nested target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start small nested candidate");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact"
    );

    while endpoint.recursive_green.target_work_pending() {
        assert!(matches!(
            endpoint
                .poll(&mut runtime, 1)
                .expect("advance small nested Green adoption"),
            CandidatePoll::Pending { transitions: 1 }
        ));
    }
    assert!(endpoint
        .recursive_green
        .ready_update_for(base_delivery.ack, target_source)
        .is_some());
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact"
    );
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(delivery.offer.base_ack, Some(base_delivery.ack));
    assert_eq!(delivery.ack.source_version, target_wire_source);
    assert!(
        delivery.offer.transferred_record_count < delivery.offer.target_record_count,
        "local adoption must retain unchanged exact-base records"
    );
    let recursive_green_replacement_records = delivery
        .packet_frames
        .iter()
        .flatten()
        .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage)
        .map(|(_, records)| *records)
        .sum::<u32>();
    assert!(
        recursive_green_replacement_records > 0
            && recursive_green_replacement_records <= MAXIMUM_REPLACEMENT_RECORDS,
        "local adoption must publish a bounded nonempty recursive-Green replacement"
    );
    assert!(
        delivery
            .packet_frames
            .iter()
            .flatten()
            .all(|(kind, _)| *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage),
        "recursive-Green local adoption must not revive legacy block replacement"
    );
    assert!(delivery.contains_recursive_green_leaf);
    let (owner_kind, _, ancestry) =
        recursive_green_query_shape(&host, target_wire_source, caret - 1, caret_utf16 - 1);
    assert_eq!(owner_kind, 5, "the edited owner remains a Green Paragraph");
    assert!(!ancestry.is_empty());
    assert_eq!(
        endpoint.recursive_green_path_receipt(),
        RecursiveGreenPathReceipt {
            local_adoption_deliveries: 1,
            clean_fallback_deliveries: 0,
        }
    );

    let retained = endpoint
        .retained
        .as_ref()
        .expect("retained small nested target");
    assert_eq!(
        retained
            .publication
            .descriptor(&runtime)
            .expect("small nested target descriptor")
            .source_revision,
        target_source.revision().get()
    );
    assert_eq!(
        retained
            .restart
            .as_ref()
            .expect("small nested target restart authority")
            .source(),
        target_source
    );
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, target_source)
        .expect("small nested target exact-base continuity"));

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn initial_large_fence_retains_recursive_green_authority_for_its_first_edit() {
    const BODY_LINES: usize = 2_500;
    let mut source = String::from("```text\n");
    let body_start = source.len();
    for ordinal in 0..BODY_LINES {
        source.push_str(&format!("line-{ordinal:04}\n"));
    }
    source.push_str("```\n");
    let edit_start =
        body_start + source[body_start..].find("line-1250").expect("middle line") + "line-".len();

    let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [351, 352, 353, 354],
        source_session_identity: 355,
        worker_generation: 1,
    };
    let mut runtime = DocumentRuntime::new(&source, standard_document_runtime_config())
        .expect("large fence runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_source = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start large fence base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("large fence host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes large fence base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert!(base_delivery.contains_recursive_green_leaf);
    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { .. })
    ));

    endpoint
        .cancel_for_edit(&mut runtime)
        .expect("prepare first large fence edit");
    let target = runtime
        .apply_edit(base_source, edit_start..edit_start + 1, "X")
        .expect("edit large fence body")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan large fence SourceFacts replacement");
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("preflight retained recursive Green"),
        "the initial Green snapshot must not depend on a legacy crop for its first edit"
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("borrow large fence target");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    let target_wire_source = source_version_for(binding, target_completion);
    host.observe_source_version(target_wire_source)
        .expect("host observes large fence target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start recursive large fence edit");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact"
    );
    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert!(target_delivery.contains_recursive_green_leaf);
    assert_eq!(target_delivery.ack.source_version, target_wire_source);
    let (owner_kind, _, _) =
        recursive_green_query_shape(&host, target_wire_source, edit_start, edit_start);
    assert_eq!(
        owner_kind, 7,
        "the edited owner remains a Green fenced code row"
    );
    assert!(!runtime
        .commit_persistent_source_facts_delta(target)
        .expect("target SourceFacts already committed by delivery"));

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn bullet_list_local_edit_delivers_exact_delta_with_unit_fuel() {
    let profile = SourceFactsScanProfile::new(4).expect("dense test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [91, 92, 93, 94],
        source_session_identity: 95,
        worker_generation: 1,
    };
    let source: String = (0..200)
        .map(|ordinal| format!("- item-{ordinal:04} café 😀\n"))
        .collect();
    let mut runtime =
        DocumentRuntime::new(&source, standard_document_runtime_config()).expect("list runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start list base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("list host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes list base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

    let caret = source.find("item-0100 café").expect("middle list item") + "item-0100 café".len();
    let caret_utf16 = source[..caret].encode_utf16().count();
    endpoint
        .cancel_for_edit(&mut runtime)
        .expect("prepare edit cancellation");
    assert!(
        !endpoint
            .prepare_bullet_list_local_edit(&runtime, caret..caret, caret_utf16..caret_utf16,)
            .expect("classify local list edit"),
        "recursive-Green authority supersedes the legacy list-only preparation lane"
    );
    let target_version = runtime
        .apply_edit(base_version, caret..caret, "🧪")
        .expect("apply local insertion")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan incremental list facts");
    let eligible = endpoint
        .has_incremental_base_for_plan(&runtime, &plan)
        .expect("local list exact-base preflight");
    let retained = endpoint.retained.as_ref().expect("retained list base");
    let descriptor = retained
        .publication
        .descriptor(&runtime)
        .expect("retained list descriptor");
    assert!(
        eligible,
        "local preflight: base={:?} target={:?} retained={:?} binding={:?} descriptor=({},{},{},{},{}) active={} cleanup={}",
        plan.base(),
        plan.source(),
        retained
            .restart
            .as_ref()
            .map(CandidateRestartAuthority::source),
        retained
            .restart
            .as_ref()
            .map(CandidateRestartAuthority::binding),
        descriptor.source_revision,
        descriptor.source_root,
        descriptor.source_bytes,
        descriptor.source_utf16,
        descriptor.syntax_profile,
        endpoint.active.is_some(),
        endpoint.cleanup.is_some(),
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("local list target source");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    host.observe_source_version(source_version_for(binding, target_completion))
        .expect("host observes list target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start local list candidate");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact"
    );

    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(delivery.offer.base_ack, Some(base_delivery.ack));
    assert_eq!(delivery.ack.source_version.revision, 1);
    assert_eq!(
        runtime.current_source_version().expect("delivered target"),
        target_version
    );
    assert!(
        delivery.packet_frames.iter().flatten().any(|(kind, _)| {
            *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage
        }),
        "local list delivery must publish its recursive-Green splice"
    );
    assert!(delivery
        .packet_frames
        .iter()
        .flatten()
        .all(|(kind, _)| *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage));
    assert_eq!(
        endpoint.recursive_green_path_receipt(),
        RecursiveGreenPathReceipt {
            local_adoption_deliveries: 1,
            clean_fallback_deliveries: 0,
        }
    );
    assert!(
        endpoint.bullet_list_local_edit.is_none(),
        "delivery must clear rolling local authority"
    );
    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

fn started_recursive_green_bullet_edit_fixture(
    document_session: [u32; 4],
) -> (
    DocumentRuntime,
    CandidateEndpoint,
    NativeCandidateHost,
    flark_engine::SourceVersion,
    usize,
    usize,
) {
    let profile = SourceFactsScanProfile::new(4).expect("dense test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session,
        source_session_identity: document_session[3] + 1,
        worker_generation: 1,
    };
    let source: String = (0..120)
        .map(|ordinal| format!("- item-{ordinal:04} café 😀\n"))
        .collect();
    let mut runtime = DocumentRuntime::new(&source, standard_document_runtime_config())
        .expect("list cancellation runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start cancellation list base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("list cancellation host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes cancellation list base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

    let caret = source.find("item-0060 café").expect("middle list item") + "item-0060 café".len();
    let caret_utf16 = source[..caret].encode_utf16().count();
    endpoint
        .cancel_for_edit(&mut runtime)
        .expect("pre-edit cancellation");
    assert!(
        !endpoint
            .prepare_bullet_list_local_edit(&runtime, caret..caret, caret_utf16..caret_utf16,)
            .expect("classify cancellation list edit"),
        "recursive-Green authority must bypass the legacy list-only lane"
    );
    runtime
        .apply_edit(base_version, caret..caret, "x")
        .expect("apply cancellation list edit");
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan cancellation list facts");
    assert!(endpoint
        .has_incremental_base_for_plan(&runtime, &plan)
        .expect("cancellation local preflight"));
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("cancellation target source");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    host.observe_source_version(source_version_for(binding, target_completion))
        .expect("host observes cancellation target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start cancellation local candidate");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact"
    );
    (runtime, endpoint, host, base_version, caret, caret_utf16)
}

#[test]
fn bullet_list_local_edit_cancellation_restores_base_across_pipeline_phases() {
    for (case, phase) in [
        ([101, 102, 103, 104], "AwaitingRecursiveGreenExact"),
        ([111, 112, 113, 114], "BuildingExact"),
        ([121, 122, 123, 124], "Streaming"),
    ] {
        let (mut runtime, mut endpoint, mut host, base, caret, caret_utf16) =
            started_recursive_green_bullet_edit_fixture(case);
        for _ in 0..100_000 {
            if active_candidate_phase(endpoint.active.as_ref()) == phase {
                break;
            }
            match endpoint
                .poll(&mut runtime, 1)
                .expect("unit-fuel phase poll")
            {
                CandidatePoll::Pending { transitions } => assert_eq!(transitions, 1),
                CandidatePoll::Event { .. } => {
                    panic!("phase {phase} was skipped before cancellation")
                }
                CandidatePoll::HotInlineEvent { .. } => {
                    panic!("local structural candidate emitted hot-inline work")
                }
                CandidatePoll::ViewportPresentationEvent { .. } => {
                    panic!("local structural candidate emitted viewport work")
                }
                CandidatePoll::ViewportPresentationUnavailable { .. } => {
                    panic!("local structural candidate emitted viewport unavailability")
                }
            }
        }
        assert_eq!(active_candidate_phase(endpoint.active.as_ref()), phase);
        endpoint
            .cancel_for_edit(&mut runtime)
            .expect("edit cancellation restores exact base");
        assert!(endpoint.active.is_none());
        assert!(endpoint.retained.is_some());
        assert!(endpoint.bullet_list_local_edit.is_none());
        assert!(endpoint
            .has_exact_base_for(&runtime, base)
            .expect("restored exact base remains eligible during target cleanup"));

        if phase == "AwaitingRecursiveGreenExact" {
            assert!(
                !endpoint
                    .prepare_bullet_list_local_edit(
                        &runtime,
                        caret..caret,
                        caret_utf16..caret_utf16,
                    )
                    .expect("classify restored local edit"),
                "restored recursive-Green authority must remain off the legacy list lane"
            );
            endpoint.cancel().expect("normal cancel");
            assert!(
                endpoint.bullet_list_local_edit.is_none(),
                "normal cancellation must discard rolling local authority"
            );
        } else if phase == "BuildingExact" {
            assert!(!endpoint
                .prepare_bullet_list_local_edit(&runtime, 3..3, 3..3)
                .expect("outside-island preparation"));
            assert!(
                endpoint.bullet_list_local_edit.is_none(),
                "outside-island preparation must drop rolling authority"
            );
        }
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    }
}

fn active_candidate_phase(active: Option<&ActiveCandidate>) -> &'static str {
    match active {
        Some(ActiveCandidate::Parsing(_)) => "Parsing",
        Some(ActiveCandidate::Building { .. }) => "Building",
        Some(ActiveCandidate::ParsingExact(_)) => "ParsingExact",
        Some(ActiveCandidate::ParsingOrdinaryExact(_)) => "ParsingOrdinaryExact",
        Some(ActiveCandidate::AwaitingRecursiveGreenExact(_)) => "AwaitingRecursiveGreenExact",
        Some(ActiveCandidate::ParsingBulletListLocal(_)) => "ParsingBulletListLocal",
        Some(ActiveCandidate::ParsingExactFallback(_)) => "ParsingExactFallback",
        Some(ActiveCandidate::BuildingExactFallback { .. }) => "BuildingExactFallback",
        Some(ActiveCandidate::BuildingExact { .. }) => "BuildingExact",
        Some(ActiveCandidate::Streaming(_)) => "Streaming",
        None => "None",
    }
}

fn push_test_frame(builder: &mut PacketBuilder, ordinal: u32, byte_len: usize) {
    assert!(builder
        .can_accept(byte_len, MAXIMUM_PACKET_ENCODED_BYTES)
        .expect("bounded packet metric"));
    builder
        .push(
            ordinal,
            0,
            0,
            [ordinal; 4],
            vec![ordinal as u8; byte_len].into_boxed_slice(),
            false,
        )
        .expect("test packet frame");
}

fn deliver_hot_inline_sidecar_with_unit_fuel(
    endpoint: &mut CandidateEndpoint,
    runtime: &mut DocumentRuntime,
    first_event_id: u32,
) -> (HotInlineSidecarBegin, InlineSidecarAck) {
    let mut next_event_id = first_event_id;
    let mut pending_event = None;
    let mut begin = None;
    for _ in 0..1_000_000 {
        let event = match pending_event.take() {
            Some(event) => event,
            None => match endpoint.poll(runtime, 1).expect("unit-fuel sidecar poll") {
                CandidatePoll::Pending { transitions } => {
                    assert!(transitions <= 1);
                    continue;
                }
                CandidatePoll::HotInlineEvent { transitions, event } => {
                    assert!(transitions <= 1);
                    *event
                }
                CandidatePoll::Event { .. } => {
                    panic!("hot-inline publication must not emit structural events")
                }
                CandidatePoll::ViewportPresentationEvent { .. } => {
                    panic!("hot-inline publication must not emit viewport events")
                }
                CandidatePoll::ViewportPresentationUnavailable { .. } => {
                    panic!("hot-inline publication emitted viewport unavailability")
                }
            },
        };
        let event_id = next_event_id;
        next_event_id = next_event_id.checked_add(1).expect("sidecar event id");
        let HotInlineEvent { credit, body } = event;
        match body {
            HotInlineEventBody::Begin(offer) => {
                assert_eq!(offer.mode, HotInlineSidecarMode::HotInlineSidecar);
                assert_eq!(
                    offer.base_ack,
                    endpoint.retained.as_ref().expect("base").ack
                );
                endpoint
                    .accept_hot_inline_credit(credit, event_id)
                    .expect("accept sidecar Begin credit");
                begin = Some(offer);
            }
            HotInlineEventBody::Packet { encoded } => {
                let packet = decode_publication_packet(&encoded).expect("decode sidecar packet");
                let offer_id = packet.offer_id;
                let next_frame_ordinal = packet
                    .first_frame_ordinal
                    .checked_add(packet.frame_count)
                    .expect("bounded sidecar frame cursor");
                let packet_record_count = packet
                    .frames()
                    .map(|frame| frame.expect("validated sidecar frame").record_count)
                    .sum::<u32>();
                assert!(
                    packet_record_count
                        <= begin
                            .expect("Begin before packet")
                            .envelope
                            .transferred_node_count
                );
                endpoint
                    .accept_hot_inline_credit(credit, event_id)
                    .expect("accept sidecar packet credit");
                pending_event = endpoint
                    .handle_hot_inline_host_poll(
                        event_id,
                        offer_id,
                        InlineSidecarHostPollPhase::PacketCredit,
                        InlineSidecarHostPollResult::Completed(
                            InlineSidecarHostPollOutcome::PacketCredit {
                                offer_id,
                                next_frame_ordinal,
                            },
                        ),
                    )
                    .expect("accept exact sidecar packet cursor");
            }
            HotInlineEventBody::Commit(commit) => {
                endpoint
                    .accept_hot_inline_credit(credit, event_id)
                    .expect("accept sidecar Commit credit");
                let ack = endpoint
                    .hot_inline_sidecar
                    .as_ref()
                    .and_then(|sidecar| sidecar.expected_ack)
                    .expect("producer committed exact sidecar ACK");
                pending_event = endpoint
                    .handle_hot_inline_host_poll(
                        event_id,
                        commit.offer_id,
                        InlineSidecarHostPollPhase::Commit,
                        InlineSidecarHostPollResult::Completed(
                            InlineSidecarHostPollOutcome::Committed(ack),
                        ),
                    )
                    .expect("accept exact sidecar commit ACK");
            }
            HotInlineEventBody::DeliveryAcknowledged(ack) => {
                endpoint
                    .accept_hot_inline_credit(credit, event_id)
                    .expect("accept sidecar delivery credit");
                assert!(endpoint.hot_inline_sidecar.is_none());
                return (begin.expect("sidecar Begin"), ack);
            }
        }
    }
    panic!("unit-fuel sidecar delivery did not complete");
}

struct PendingHotInlineDelivery {
    begin: HotInlineSidecarBegin,
    ack: InlineSidecarAck,
    hio1_schema: u32,
    credit: HotInlineCredit,
    event_id: u32,
}

fn commit_hot_inline_sidecar_to_independent_host_with_unit_fuel(
    endpoint: &mut CandidateEndpoint,
    runtime: &mut DocumentRuntime,
    host: &mut NativeCandidateHost,
    first_event_id: u32,
) -> PendingHotInlineDelivery {
    let mut next_event_id = first_event_id;
    let mut pending_event = None;
    let mut begin = None;
    let mut hio1_schema = None;
    for _ in 0..1_000_000 {
        let event = match pending_event.take() {
            Some(event) => event,
            None => match endpoint.poll(runtime, 1).expect("unit-fuel sidecar poll") {
                CandidatePoll::Pending { transitions } => {
                    assert!(transitions <= 1);
                    continue;
                }
                CandidatePoll::HotInlineEvent { transitions, event } => {
                    assert!(transitions <= 1);
                    *event
                }
                CandidatePoll::Event { .. } => {
                    panic!("hot-inline publication must not emit structural events")
                }
                CandidatePoll::ViewportPresentationEvent { .. } => {
                    panic!("hot-inline publication must not emit viewport events")
                }
                CandidatePoll::ViewportPresentationUnavailable { .. } => {
                    panic!("hot-inline publication emitted viewport unavailability")
                }
            },
        };
        let event_id = next_event_id;
        next_event_id = next_event_id.checked_add(1).expect("sidecar event id");
        let HotInlineEvent { credit, body } = event;
        match body {
            HotInlineEventBody::Begin(offer) => {
                host.begin_inline_sidecar_offer(offer)
                    .expect("independent host begins sidecar offer");
                endpoint
                    .accept_hot_inline_credit(credit, event_id)
                    .expect("accept sidecar Begin credit");
                begin = Some(offer);
            }
            HotInlineEventBody::Packet { encoded } => {
                let packet = decode_publication_packet(&encoded).expect("decode sidecar packet");
                let offer_id = packet.offer_id;
                for decoded in packet.frames() {
                    let frame = decoded.expect("validated sidecar frame");
                    if frame.ordinal == 0 {
                        assert!(frame.bytes.len() >= 24, "HIO1 Begin carries its envelope");
                        hio1_schema = Some(u32::from_le_bytes(
                            frame.bytes[20..24].try_into().expect("HIO1 schema"),
                        ));
                    }
                }
                host.admit_inline_sidecar_packet(packet)
                    .expect("independent host admits sidecar packet");
                endpoint
                    .accept_hot_inline_credit(credit, event_id)
                    .expect("accept sidecar packet credit");
                let (credited_offer_id, next_frame_ordinal) = loop {
                    match host
                        .poll_inline_sidecar(HostWorkGrant {
                            inspect_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
                            copy_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
                            transitions: 1,
                        })
                        .expect("unit-fuel host sidecar packet poll")
                    {
                        NativeInlineSidecarHostPollOutcome::Pending => {}
                        NativeInlineSidecarHostPollOutcome::PacketCredit {
                            offer_id,
                            next_frame_ordinal,
                        } => break (offer_id, next_frame_ordinal),
                        outcome => panic!("unexpected sidecar packet outcome: {outcome:?}"),
                    }
                };
                pending_event = endpoint
                    .handle_hot_inline_host_poll(
                        event_id,
                        offer_id,
                        InlineSidecarHostPollPhase::PacketCredit,
                        InlineSidecarHostPollResult::Completed(
                            InlineSidecarHostPollOutcome::PacketCredit {
                                offer_id: credited_offer_id,
                                next_frame_ordinal,
                            },
                        ),
                    )
                    .expect("producer accepts exact sidecar packet credit");
            }
            HotInlineEventBody::Commit(commit) => {
                host.request_inline_sidecar_commit(commit)
                    .expect("independent host accepts sidecar commit");
                endpoint
                    .accept_hot_inline_credit(credit, event_id)
                    .expect("accept sidecar Commit credit");
                let ack = loop {
                    match host
                        .poll_inline_sidecar(HostWorkGrant {
                            inspect_bytes: 0,
                            copy_bytes: 0,
                            transitions: 1,
                        })
                        .expect("unit-fuel host sidecar install poll")
                    {
                        NativeInlineSidecarHostPollOutcome::Pending => {}
                        NativeInlineSidecarHostPollOutcome::Committed(ack) => break ack,
                        outcome => panic!("unexpected sidecar commit outcome: {outcome:?}"),
                    }
                };
                pending_event = endpoint
                    .handle_hot_inline_host_poll(
                        event_id,
                        commit.offer_id,
                        InlineSidecarHostPollPhase::Commit,
                        InlineSidecarHostPollResult::Completed(
                            InlineSidecarHostPollOutcome::Committed(ack),
                        ),
                    )
                    .expect("producer accepts exact sidecar commit ACK");
            }
            HotInlineEventBody::DeliveryAcknowledged(ack) => {
                return PendingHotInlineDelivery {
                    begin: begin.expect("sidecar Begin"),
                    ack,
                    hio1_schema: hio1_schema.expect("sidecar HIO1 Begin schema"),
                    credit,
                    event_id,
                };
            }
        }
    }
    panic!("unit-fuel sidecar commit to independent host did not complete");
}

fn deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
    endpoint: &mut CandidateEndpoint,
    runtime: &mut DocumentRuntime,
    host: &mut NativeCandidateHost,
    first_event_id: u32,
) -> (HotInlineSidecarBegin, InlineSidecarAck) {
    let pending = commit_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        endpoint,
        runtime,
        host,
        first_event_id,
    );
    host.acknowledge_inline_sidecar_delivery(pending.ack)
        .expect("independent host accepts sidecar delivery");
    endpoint
        .accept_hot_inline_credit(pending.credit, pending.event_id)
        .expect("accept sidecar delivery credit");
    (pending.begin, pending.ack)
}

#[test]
fn source_root_wire_lanes_are_high_word_then_low_word() {
    assert_eq!(split_u64(0x1122_3344_5566_7788), [0x1122_3344, 0x5566_7788]);
}

#[test]
fn selected_list_item_target_requires_its_exact_projection_kind_and_metadata() {
    use InlineRefinementTarget::{
        Automatic, BlockQuoteProjection, BulletListItemInline, BulletListItemProjection,
        OrderedListItemInline, OrderedListItemProjection,
    };
    use M11MarkedLineProjectionKind::{BulletList, OrderedList};

    assert!(list_item_projection_matches_target(
        BulletListItemProjection,
        BulletList,
        false,
    ));
    assert!(list_item_projection_matches_target(
        OrderedListItemProjection,
        OrderedList,
        true,
    ));
    for (target, kind, ordered_metadata) in [
        (BulletListItemProjection, OrderedList, false),
        (BulletListItemProjection, BulletList, true),
        (OrderedListItemProjection, BulletList, true),
        (OrderedListItemProjection, OrderedList, false),
        (Automatic, OrderedList, true),
        (BlockQuoteProjection, BulletList, false),
        (BulletListItemInline, BulletList, false),
        (OrderedListItemInline, OrderedList, true),
    ] {
        assert!(
            !list_item_projection_matches_target(target, kind, ordered_metadata),
            "{target:?} must reject {kind:?} with ordered metadata={ordered_metadata}",
        );
    }
}

fn deliver_viewport_presentation_with_unit_fuel(
    endpoint: &mut CandidateEndpoint,
    runtime: &mut DocumentRuntime,
    host: &mut NativeCandidateHost,
) -> (
    ViewportPresentationBegin,
    ViewportPresentationAck,
    usize,
    usize,
    usize,
) {
    let mut next_event_id = 40_000_u32;
    let mut pending_event = None;
    let mut offer = None;
    let mut authoritative = 0_usize;
    let mut unsupported = 0_usize;
    let mut child_closures = 0_usize;
    let mut active_child = None;
    let mut next_child_frame_ordinal = 0_u32;
    let mut observed_frame_count = 0_u32;
    let mut observed_frame_bytes = 0_u32;
    for _ in 0..1_000_000 {
        let event = match pending_event.take() {
            Some(event) => event,
            None => match endpoint
                .poll(runtime, 1)
                .expect("unit-fuel viewport producer poll")
            {
                CandidatePoll::Pending { transitions } => {
                    assert!(transitions <= 1);
                    continue;
                }
                CandidatePoll::ViewportPresentationEvent { transitions, event } => {
                    assert!(transitions <= 1);
                    *event
                }
                CandidatePoll::Event { .. } => {
                    panic!("viewport delivery emitted a structural event")
                }
                CandidatePoll::HotInlineEvent { .. } => {
                    panic!("viewport delivery emitted a point-sidecar event")
                }
                CandidatePoll::ViewportPresentationUnavailable { .. } => {
                    panic!("admitted viewport delivery became unavailable")
                }
            },
        };
        let event_id = next_event_id;
        next_event_id = next_event_id.checked_add(1).expect("viewport event id");
        let CandidateViewportPresentationEvent { credit, body } = event;
        match body {
            CandidateViewportPresentationEventBody::Begin(begin) => {
                assert_eq!(begin.mode, ViewportPresentationMode::AggregatePage);
                assert_eq!(
                    begin.limits.maximum_frame_count,
                    begin
                        .envelope
                        .ordered_leaf_count
                        .checked_mul(2)
                        .and_then(|count| {
                            count.checked_add(begin.envelope.transferred_node_count)
                        })
                        .and_then(|count| count.checked_add(3))
                        .expect("bounded viewport frame count")
                );
                host.begin_viewport_presentation_offer(begin)
                    .expect("independent host begins viewport offer");
                endpoint
                    .accept_viewport_presentation_credit(credit, event_id)
                    .expect("accept viewport Begin credit");
                assert!(
                    endpoint.has_poll_work(),
                    "accepted viewport Begin must wake packet production"
                );
                offer = Some(begin);
            }
            CandidateViewportPresentationEventBody::Packet { encoded } => {
                let begin = offer.expect("viewport Begin precedes packets");
                let packet = decode_publication_packet(&encoded).expect("decode viewport packet");
                let packet_offer_id = packet.offer_id;
                let first_frame_ordinal = packet.first_frame_ordinal;
                let frame_count = packet.frame_count;
                let end = first_frame_ordinal
                    .checked_add(frame_count)
                    .is_some_and(|next| next == begin.limits.maximum_frame_count);
                for decoded in packet.frames() {
                    let frame = decoded.expect("validated viewport packet frame");
                    let kind = if frame.ordinal == 0 {
                        decode_viewport_presentation_parent_frame(frame.bytes, begin)
                            .expect("decode viewport parent");
                        assert_eq!(frame.record_count, 0);
                        ViewportPresentationFrameKind::Begin
                    } else if frame.ordinal == 1 {
                        let directory = decode_viewport_presentation_directory(frame.bytes, begin)
                            .expect("decode viewport directory");
                        assert_eq!(frame.record_count, begin.envelope.ordered_leaf_count);
                        for entry in directory.entries() {
                            match entry.hio1_envelope.disposition {
                                HotInlineSidecarDisposition::Authoritative { .. } => {
                                    authoritative += 1
                                }
                                HotInlineSidecarDisposition::Unsupported { .. } => unsupported += 1,
                            }
                        }
                        ViewportPresentationFrameKind::Directory
                    } else if frame.ordinal
                        == begin
                            .limits
                            .maximum_frame_count
                            .checked_sub(1)
                            .expect("viewport has End")
                    {
                        let terminal = decode_viewport_presentation_end_frame(frame.bytes, begin)
                            .expect("decode viewport End");
                        assert_eq!(
                            terminal.actual_frame_count,
                            begin.limits.maximum_frame_count
                        );
                        assert!(
                            terminal.actual_encoded_frame_bytes
                                <= begin.limits.maximum_encoded_frame_bytes
                        );
                        assert_eq!(frame.record_count, 0);
                        assert!(active_child.is_none());
                        ViewportPresentationFrameKind::End
                    } else {
                        let child = decode_viewport_presentation_child_frame(frame.bytes, begin)
                            .expect("decode opaque HIO1 child wrapper");
                        assert_eq!(frame.record_count, child.record_count);
                        match child.kind {
                            HotInlineSidecarFrameKind::Begin => {
                                assert_eq!(child.child_frame_ordinal, 0);
                                assert!(active_child.replace(child.directory_index).is_none());
                                next_child_frame_ordinal = 1;
                            }
                            HotInlineSidecarFrameKind::Node => {
                                assert_eq!(active_child, Some(child.directory_index));
                                assert_eq!(child.child_frame_ordinal, next_child_frame_ordinal);
                                assert_eq!(child.record_count, 1);
                                next_child_frame_ordinal = next_child_frame_ordinal
                                    .checked_add(1)
                                    .expect("bounded child frame ordinal");
                            }
                            HotInlineSidecarFrameKind::End => {
                                assert_eq!(active_child, Some(child.directory_index));
                                assert_eq!(child.child_frame_ordinal, next_child_frame_ordinal);
                                assert_eq!(child.record_count, 0);
                                active_child = None;
                                child_closures += 1;
                            }
                        }
                        ViewportPresentationFrameKind::Child
                    };
                    assert_eq!(
                        frame.digest,
                        protocol_digest128_from_blake3(
                            ProtocolDigestDomain::ViewportPresentationFrame,
                            viewport_presentation_frame_digest256(frame.ordinal, kind, frame.bytes,),
                        )
                    );
                    observed_frame_count = observed_frame_count
                        .checked_add(1)
                        .expect("bounded observed frame count");
                    observed_frame_bytes = observed_frame_bytes
                        .checked_add(
                            u32::try_from(frame.bytes.len()).expect("bounded observed frame bytes"),
                        )
                        .expect("bounded observed frame bytes");
                }
                host.admit_viewport_presentation_packet(packet)
                    .expect("independent host admits viewport packet");
                endpoint
                    .accept_viewport_presentation_credit(credit, event_id)
                    .expect("accept viewport packet credit");
                let (credited_offer_id, credited_next_frame_ordinal) = loop {
                    match host
                        .poll_viewport_presentation(HostWorkGrant {
                            inspect_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
                            copy_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
                            transitions: 1,
                        })
                        .expect("independent host polls viewport packet")
                    {
                        NativeViewportPresentationPollOutcome::Pending => {}
                        NativeViewportPresentationPollOutcome::PacketCredit {
                            offer_id,
                            next_frame_ordinal,
                        } => break (offer_id, next_frame_ordinal),
                        outcome => panic!("unexpected viewport packet outcome: {outcome:?}"),
                    }
                };
                assert!(endpoint
                    .handle_viewport_presentation_host_poll(
                        event_id,
                        packet_offer_id,
                        ViewportPresentationHostPollPhase::PacketCredit,
                        ViewportPresentationHostPollResult::Completed(
                            ViewportPresentationHostPollOutcome::PacketCredit {
                                offer_id: credited_offer_id,
                                next_frame_ordinal: credited_next_frame_ordinal,
                            },
                        ),
                    )
                    .expect("accept exact viewport packet credit")
                    .is_none());
                assert!(
                    endpoint.has_poll_work(),
                    "accepted packet credit must wake the next packet or commit"
                );
                if end {
                    assert_eq!(
                        first_frame_ordinal + frame_count,
                        begin.limits.maximum_frame_count
                    );
                }
            }
            CandidateViewportPresentationEventBody::Commit(commit) => {
                let Some(ViewportInlineBatchState::Streaming(streaming)) =
                    endpoint.viewport_inline_batch.as_ref()
                else {
                    panic!("viewport commit retains streaming state")
                };
                let ack = streaming.expected_ack.expect("viewport expected ACK");
                assert_eq!(commit.actual_frame_count, observed_frame_count);
                assert_eq!(commit.actual_encoded_frame_bytes, observed_frame_bytes);
                host.request_viewport_presentation_commit(commit)
                    .expect("independent host accepts viewport commit");
                endpoint
                    .accept_viewport_presentation_credit(credit, event_id)
                    .expect("accept viewport commit credit");
                let committed = loop {
                    match host
                        .poll_viewport_presentation(HostWorkGrant {
                            inspect_bytes: 0,
                            copy_bytes: 0,
                            transitions: 1,
                        })
                        .expect("independent host polls viewport commit")
                    {
                        NativeViewportPresentationPollOutcome::Pending => {}
                        NativeViewportPresentationPollOutcome::Committed(ack) => break ack,
                        outcome => panic!("unexpected viewport commit outcome: {outcome:?}"),
                    }
                };
                assert_eq!(committed, ack);
                pending_event = endpoint
                    .handle_viewport_presentation_host_poll(
                        event_id,
                        commit.offer_id,
                        ViewportPresentationHostPollPhase::Commit,
                        ViewportPresentationHostPollResult::Completed(
                            ViewportPresentationHostPollOutcome::Committed(ack),
                        ),
                    )
                    .expect("accept exact viewport commit");
            }
            CandidateViewportPresentationEventBody::DeliveryAcknowledged(ack) => {
                host.acknowledge_viewport_presentation_delivery(ack)
                    .expect("independent host acknowledges viewport delivery");
                endpoint
                    .accept_viewport_presentation_credit(credit, event_id)
                    .expect("accept viewport delivery credit");
                assert!(endpoint.viewport_inline_batch.is_none());
                return (
                    offer.expect("viewport Begin"),
                    ack,
                    authoritative,
                    unsupported,
                    child_closures,
                );
            }
        }
    }
    panic!("unit-fuel viewport presentation did not complete");
}

#[test]
fn viewport_directory_product_max_fits_only_the_vpb1_wrapper_bound() {
    let bytes = VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES
        + 128 * VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES;
    assert!(bytes > M11_MAX_SNAPSHOT_FRAME_BYTES);
    assert!(bytes <= MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES as usize);
}

#[test]
fn focused_inline_delivery_then_live_checkpoint_viewport_reaches_terminal() {
    const SOURCE: &str = "# A live document\n\
\n\
Write with **bold**, _emphasis_, `inline code`, and ~~strikethrough~~ while Flark keeps canonical Markdown exact.\n\
\n\
Browse <https://commonmark.org> or email <hello@example.com>. Links stay marker-free while their exact targets remain parser-owned.\n\
\n\
## A second idea\n\
\n\
```dart\n\
final message = 'Hello from Flark';\n\
```\n\
\n\
Tap any block to move the live editor, then start typing.";
    let profile = SourceFactsScanProfile::new(4).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [731, 732, 733, 734],
        source_session_identity: 735,
        worker_generation: 1,
    };
    let mut runtime = DocumentRuntime::new(SOURCE, standard_document_runtime_config())
        .expect("checkpoint runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start checkpoint candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("checkpoint host");
    host.observe_source_version(source_version_for(binding, completion))
        .expect("host observes checkpoint source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let viewport_command = |viewport_generation| ViewportInlineBatchCommand {
        binding,
        viewport_generation,
        source_version: delivery.ack.source_version,
        base_ack: delivery.ack,
        start_entry_ordinal: 0,
        start_byte_offset: 0,
        start_utf16_offset: 0,
        end_byte_offset: u32::try_from(SOURCE.len()).expect("bounded source"),
        end_utf16_offset: u32::try_from(SOURCE.encode_utf16().count()).expect("bounded UTF-16"),
        limits: ViewportInlineBatchLimits {
            maximum_structural_entries: 64,
            maximum_storage_pages: 25,
            maximum_inline_leaves: 64,
            maximum_inline_leaf_source_bytes: 8 * 1024,
            maximum_inline_source_bytes: 64 * 1024,
            maximum_fact_records: 2_048,
            maximum_projection_bytes: 512 * 1024,
            maximum_parser_transitions: 250_000,
        },
    };
    let focused_offset = SOURCE
        .find("start typing")
        .expect("focused checkpoint leaf");
    endpoint
        .request_hot_inline(
            &mut runtime,
            InlineRefinementCommand {
                binding,
                refinement_generation: 1,
                source_version: delivery.ack.source_version,
                base_ack: delivery.ack,
                byte_offset: u32::try_from(focused_offset).expect("bounded focused point"),
                utf16_offset: u32::try_from(SOURCE[..focused_offset].encode_utf16().count())
                    .expect("bounded focused UTF-16 point"),
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::Automatic,
            },
        )
        .expect("request checkpoint focused inline");
    let pending_inline = commit_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        30_000,
    );
    assert!(matches!(
        endpoint.request_viewport_inline_batch(&runtime, viewport_command(1)),
        Err(CandidateEndpointError::Busy)
    ));
    host.acknowledge_inline_sidecar_delivery(pending_inline.ack)
        .expect("host acknowledges focused inline delivery");
    endpoint
        .accept_hot_inline_credit(pending_inline.credit, pending_inline.event_id)
        .expect("parser accepts focused inline delivery");
    for _ in 0..10_000 {
        if !endpoint.hot_inline_has_poll_work() {
            break;
        }
        let transitions = endpoint
            .poll_hot_inline(&mut runtime, 1)
            .expect("release delivered focused inline");
        assert!(transitions <= 1);
    }
    assert!(!endpoint.hot_inline_has_poll_work());

    endpoint
        .request_viewport_inline_batch(&runtime, viewport_command(2))
        .expect("accepted checkpoint viewport");
    let initial_parser_transitions = match endpoint.viewport_inline_batch.as_ref() {
        Some(ViewportInlineBatchState::Running(running)) => running.total_parser_transitions,
        _ => panic!("accepted checkpoint viewport must begin in the running phase"),
    };

    let mut preparation_polls = 0_usize;
    let mut preparation_transitions = 0_usize;
    loop {
        match endpoint.viewport_inline_batch.as_ref() {
            Some(ViewportInlineBatchState::Running(_)) => {}
            Some(ViewportInlineBatchState::Ready(ready)) => {
                assert_eq!(ready.leaves.len(), 5);
                assert_eq!(ready.total_ready_roots, 5);
                assert_eq!(
                    ready.total_parser_transitions,
                    initial_parser_transitions
                        + u64::try_from(preparation_transitions)
                            .expect("bounded preparation transitions")
                );
                assert!(ready.total_parser_transitions < 10_000);
                break;
            }
            Some(ViewportInlineBatchState::Streaming(_)) => {
                panic!("direct preparation polling must stop before streaming")
            }
            Some(ViewportInlineBatchState::Cancelling(_)) => {
                panic!("accepted checkpoint viewport entered cleanup")
            }
            None => panic!("accepted checkpoint viewport disappeared"),
        }
        let transitions = endpoint
            .poll_viewport_inline_batch(&mut runtime, 1)
            .expect("bounded checkpoint viewport preparation");
        assert!(transitions <= 1);
        preparation_polls += 1;
        preparation_transitions += transitions;
        assert!(
            preparation_polls < 10_000,
            "checkpoint viewport preparation did not converge"
        );
        if matches!(
            endpoint.viewport_inline_batch,
            Some(ViewportInlineBatchState::Running(_))
        ) {
            assert_eq!(
                transitions, 1,
                "a still-running checkpoint inline job must make unit-fuel progress"
            );
        }
    }

    let (begin, ack, authoritative, unsupported, child_closures) =
        deliver_viewport_presentation_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(begin.binding.viewport_generation, 2);
    assert_eq!(ack.binding.viewport_generation, 2);
    assert_eq!(authoritative + unsupported, 5);
    assert_eq!(child_closures, 5);
    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn default_sixty_four_leaf_profile_fits_the_private_vpb1_stream_ceiling() {
    const LEAF_COUNT: usize = 64;
    // At the default 64-leaf / 2,048-fact / 64-KiB-source caps, the
    // current parser needs at most 84 fact-tree nodes and 98 value-tree
    // nodes after per-root fragmentation. Every authoritative leaf adds
    // one synthetic bundle node. Keep the components explicit so a
    // projection-layout change cannot hide behind an aggregate.
    const MAXIMUM_FACT_TREE_NODES: usize = 84;
    const MAXIMUM_VALUE_TREE_NODES: usize = 98;
    const MAXIMUM_BUNDLE_NODES: usize = LEAF_COUNT;
    const MAXIMUM_TRANSFERRED_NODES: usize =
        MAXIMUM_FACT_TREE_NODES + MAXIMUM_VALUE_TREE_NODES + MAXIMUM_BUNDLE_NODES;
    const HIO1_ENVELOPE_BYTES: usize = 256;
    const IPR3_DESCRIPTOR_BYTES: usize = 280;
    const PRIVATE_STREAM_CEILING: usize = 2 * 1024 * 1024;

    assert_eq!(VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES, 144);
    assert_eq!(VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES, 12);
    assert_eq!(VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES, 192);
    assert_eq!(VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES, 28);
    assert_eq!(VIEWPORT_PRESENTATION_END_FRAME_BYTES, 52);
    assert_eq!(M11_INLINE_META_RECORD_BYTES, 48);
    assert_eq!(M11_MAX_SNAPSHOT_FRAME_BYTES, 5_140);
    assert_eq!(MAXIMUM_TRANSFERRED_NODES, 246);

    let outer_bytes = VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES
        + VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES
        + VIEWPORT_PRESENTATION_END_FRAME_BYTES;
    let per_leaf_bytes = VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES
        + HIO1_ENVELOPE_BYTES
        + IPR3_DESCRIPTOR_BYTES
        + 3 * M11_INLINE_META_RECORD_BYTES
        + 2 * VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES;
    let per_transferred_node_bytes =
        M11_MAX_SNAPSHOT_FRAME_BYTES + VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES;
    let maximum_encoded_bytes = outer_bytes
        + LEAF_COUNT * per_leaf_bytes
        + MAXIMUM_TRANSFERRED_NODES * per_transferred_node_bytes;

    assert_eq!(outer_bytes, 208);
    assert_eq!(per_leaf_bytes, 928);
    assert_eq!(per_transferred_node_bytes, 5_168);
    assert_eq!(maximum_encoded_bytes, 1_330_928);
    assert!(maximum_encoded_bytes <= PRIVATE_STREAM_CEILING);
}

#[test]
fn viewport_inline_batch_publishes_twenty_four_children_then_post_begin_point_waits() {
    const PARAGRAPHS: usize = 24;
    const UNSUPPORTED_ORDINAL: usize = 12;
    let mut source = String::new();
    let mut paragraph_starts = Vec::with_capacity(PARAGRAPHS);
    for ordinal in 0..PARAGRAPHS {
        if ordinal != 0 {
            source.push_str("\n\n");
        }
        paragraph_starts.push(source.len());
        if ordinal == UNSUPPORTED_ORDINAL {
            source.push_str("before <tag>");
        } else {
            source.push_str(&format!(
                "**bold{ordinal:02}** *em{ordinal:02}* `code{ordinal:02}`"
            ));
        }
    }
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [681, 682, 683, 684],
        source_session_identity: 685,
        worker_generation: 1,
    };
    let mut runtime = DocumentRuntime::new(&source, standard_document_runtime_config())
        .expect("viewport batch runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start segmented viewport candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("viewport batch host");
    host.observe_source_version(source_version_for(binding, completion))
        .expect("host observes viewport source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    let retained_descriptor = endpoint
        .retained
        .as_ref()
        .expect("retained viewport base")
        .publication
        .descriptor(&runtime)
        .expect("retained descriptor");
    let baseline_metrics = runtime.arena_metrics();

    let limits = ViewportInlineBatchLimits {
        maximum_structural_entries: 47,
        maximum_storage_pages: 25,
        maximum_inline_leaves: PARAGRAPHS as u32,
        maximum_inline_leaf_source_bytes: 8 * 1024,
        maximum_inline_source_bytes: 8 * 1024,
        maximum_fact_records: ((PARAGRAPHS - 1) * 3) as u64,
        maximum_projection_bytes: 1024 * 1024,
        maximum_parser_transitions: 100_000,
    };
    let command = |generation| ViewportInlineBatchCommand {
        binding,
        viewport_generation: generation,
        source_version: delivery.ack.source_version,
        base_ack: delivery.ack,
        start_entry_ordinal: 0,
        start_byte_offset: 0,
        start_utf16_offset: 0,
        end_byte_offset: u32::try_from(source.len()).expect("bounded source"),
        end_utf16_offset: u32::try_from(source.encode_utf16().count()).expect("bounded UTF-16"),
        limits,
    };
    let mut truncated_end = command(1);
    truncated_end.end_byte_offset = truncated_end.end_byte_offset.saturating_sub(1);
    truncated_end.end_utf16_offset = truncated_end.end_utf16_offset.saturating_sub(1);
    assert!(matches!(
        endpoint.request_viewport_inline_batch(&runtime, truncated_end),
        Err(CandidateEndpointError::InvalidAuthority)
    ));
    assert!(endpoint.viewport_inline_batch.is_none());
    let mut one_byte_leaf_limit = command(1);
    one_byte_leaf_limit.limits.maximum_inline_leaf_source_bytes = 1;
    assert!(matches!(
        endpoint.request_viewport_inline_batch(&runtime, one_byte_leaf_limit),
        Err(CandidateEndpointError::ViewportInlineLimitExceeded(
            "inline leaf source bytes"
        ))
    ));
    assert!(endpoint.viewport_inline_batch.is_none());
    let mut aggregate_source_limited = command(1);
    aggregate_source_limited
        .limits
        .maximum_inline_leaf_source_bytes = 64;
    aggregate_source_limited.limits.maximum_inline_source_bytes = 64;
    let aggregate_source_result =
        endpoint.request_viewport_inline_batch(&runtime, aggregate_source_limited);
    assert!(
        matches!(
            aggregate_source_result,
            Err(CandidateEndpointError::ViewportInlineLimitExceeded(
                "inline source bytes"
            ))
        ),
        "unexpected aggregate source bound result: {aggregate_source_result:?}",
    );
    assert!(endpoint.viewport_inline_batch.is_none());

    let first_blank_start = paragraph_starts[1] - 1;
    let mut blank_only = command(1);
    blank_only.start_entry_ordinal = 1;
    blank_only.start_byte_offset = first_blank_start as u32;
    blank_only.start_utf16_offset = first_blank_start as u32;
    blank_only.end_byte_offset = paragraph_starts[1] as u32;
    blank_only.end_utf16_offset = paragraph_starts[1] as u32;
    blank_only.limits.maximum_structural_entries = 1;
    blank_only.limits.maximum_inline_leaves = 1;
    blank_only.limits.maximum_inline_source_bytes = 1;
    blank_only.limits.maximum_fact_records = 1;
    blank_only.limits.maximum_projection_bytes = 1;
    blank_only.limits.maximum_storage_pages = 25;
    blank_only.limits.maximum_parser_transitions = 100_000;
    assert!(matches!(
        endpoint.request_viewport_inline_batch(&runtime, blank_only),
        Err(CandidateEndpointError::InvalidAuthority)
    ));
    assert!(endpoint.viewport_inline_batch.is_none());

    let mut fact_limited = command(2);
    fact_limited.limits.maximum_fact_records = 1;
    endpoint
        .request_viewport_inline_batch(&runtime, fact_limited)
        .expect("admit asynchronously fact-limited viewport");
    let mut failure_transitions = 0_usize;
    loop {
        match endpoint
            .poll(&mut runtime, 1)
            .expect("fact-limited viewport remains attempt-local")
        {
            CandidatePoll::Pending { transitions } => {
                assert!(transitions <= 1);
                failure_transitions += transitions;
            }
            CandidatePoll::ViewportPresentationUnavailable {
                transitions,
                viewport_generation,
                reason,
            } => {
                assert!(transitions <= 1);
                failure_transitions += transitions;
                assert_eq!(viewport_generation, 2);
                assert_eq!(
                    reason,
                    ViewportPresentationUnavailableReason::BudgetExceeded
                );
                break;
            }
            CandidatePoll::ViewportPresentationEvent { .. }
            | CandidatePoll::Event { .. }
            | CandidatePoll::HotInlineEvent { .. } => {
                panic!("fact-limited viewport must fail before publication")
            }
        }
    }
    assert!(failure_transitions > 0);
    assert!(endpoint.viewport_inline_batch.is_none());
    assert!(endpoint.pending_viewport_unavailable.is_none());

    let mut preempted_failure = command(3);
    preempted_failure.limits.maximum_fact_records = 1;
    endpoint
        .request_viewport_inline_batch(&runtime, preempted_failure)
        .expect("admit preempted fact-limited viewport");
    for _ in 0..1_000_000 {
        assert!(matches!(
            endpoint
                .poll(&mut runtime, 1)
                .expect("advance preempted viewport failure"),
            CandidatePoll::Pending { transitions } if transitions <= 1
        ));
        if endpoint.pending_viewport_unavailable.is_some() {
            break;
        }
    }
    assert_eq!(
        endpoint.pending_viewport_unavailable,
        Some((3, ViewportPresentationUnavailableReason::BudgetExceeded))
    );
    endpoint
        .request_hot_inline(
            &mut runtime,
            InlineRefinementCommand {
                binding,
                refinement_generation: 1,
                source_version: delivery.ack.source_version,
                base_ack: delivery.ack,
                byte_offset: u32::try_from(paragraph_starts[0]).expect("bounded point"),
                utf16_offset: 0,
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::Automatic,
            },
        )
        .expect("focused demand supersedes pending viewport failure");
    assert!(endpoint.pending_viewport_unavailable.is_none());
    for _ in 0..1_000_000 {
        if endpoint.viewport_inline_batch.is_none() {
            break;
        }
        assert!(
            endpoint
                .poll_viewport_inline_batch(&mut runtime, 1)
                .expect("drain superseded failure cleanup")
                <= 1
        );
    }
    assert!(matches!(
        endpoint.hot_inline,
        Some(HotInlineState::AwaitingReferenceResolver(_))
    ));
    endpoint.cancel_hot_inline();
    for _ in 0..1_000_000 {
        if !endpoint.hot_inline_has_poll_work() {
            break;
        }
        assert!(
            endpoint
                .poll_hot_inline(&mut runtime, 1)
                .expect("drain superseding focused demand")
                <= 1
        );
    }
    assert!(endpoint.pending_viewport_unavailable.is_none());
    endpoint
        .request_viewport_inline_batch(&runtime, command(4))
        .expect("start one-walk viewport batch");
    for _ in 0..1_000_000 {
        if matches!(
            endpoint.viewport_inline_batch,
            Some(ViewportInlineBatchState::Ready(_))
        ) {
            break;
        }
        let transitions = endpoint
            .poll_viewport_inline_batch(&mut runtime, 1)
            .expect("unit-fuel viewport poll");
        assert!(transitions <= 1);
    }
    let Some(ViewportInlineBatchState::Ready(ready)) = endpoint.viewport_inline_batch.as_ref()
    else {
        panic!("24-leaf viewport batch did not become ready");
    };
    assert_eq!(ready.command, command(4));
    assert_eq!(ready.descriptor, retained_descriptor);
    assert_eq!(ready.range_receipt.visited_entries(), PARAGRAPHS as u64);
    assert!(ready.range_receipt.storage_pages_visited() <= u64::from(limits.maximum_storage_pages));
    assert_eq!(
        ready.range_receipt.next_byte_offset(),
        u64::try_from(source.len()).expect("bounded source")
    );
    assert_eq!(
        ready.range_receipt.next_utf16_offset(),
        u64::try_from(source.encode_utf16().count()).expect("bounded UTF-16")
    );
    assert_eq!(ready.leaves.len(), PARAGRAPHS);
    assert!(ready.total_inline_source_bytes <= limits.maximum_inline_source_bytes);
    assert!(ready.total_parser_transitions <= limits.maximum_parser_transitions);
    assert_eq!(ready.total_fact_records, limits.maximum_fact_records);
    assert_eq!(ready.total_ready_roots, (PARAGRAPHS - 1) as u32);
    let mut authoritative = 0_usize;
    let mut unsupported = 0_usize;
    for (index, leaf) in ready.leaves.iter().enumerate() {
        assert_eq!(leaf.geometry.kind, M11BlockSequenceEntryKind::Paragraph);
        assert_eq!(leaf.geometry.entry_ordinal, index as u64);
        assert_eq!(
            leaf.geometry.block_source.start,
            paragraph_starts[index] as u32
        );
        assert!(leaf.geometry.block_source.start < leaf.geometry.block_source.end);
        assert!(leaf.geometry.block_source_utf16.start < leaf.geometry.block_source_utf16.end);
        assert!(leaf.geometry.inline_source.start < leaf.geometry.inline_source.end);
        assert!(
            leaf.geometry.inline_source.end - leaf.geometry.inline_source.start
                <= limits.maximum_inline_leaf_source_bytes
        );
        assert!(leaf.geometry.inline_source_utf16.start < leaf.geometry.inline_source_utf16.end);
        assert_eq!(leaf.parser_profile, parser_profile);
        match &leaf.publication {
            ViewportInlineLeafPublication::Authoritative(root) => {
                assert_ne!(index, UNSUPPORTED_ORDINAL);
                assert_eq!(root.descriptor().fact_count(), 3);
                assert_eq!(
                    root.descriptor().source_range(),
                    &leaf.geometry.inline_source
                );
                authoritative += 1;
            }
            ViewportInlineLeafPublication::Unsupported(record) => {
                assert_eq!(index, UNSUPPORTED_ORDINAL);
                assert_eq!(record.source_range(), leaf.geometry.inline_source);
                unsupported += 1;
            }
        }
    }
    assert_eq!(authoritative, PARAGRAPHS - 1);
    assert_eq!(unsupported, 1);

    let (viewport_offer, viewport_ack, published_authoritative, published_unsupported, closures) =
        deliver_viewport_presentation_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(viewport_offer.base_ack, delivery.ack);
    assert_eq!(viewport_offer.binding.viewport_generation, 4);
    assert_eq!(
        viewport_offer.envelope.ordered_leaf_count,
        PARAGRAPHS as u32
    );
    assert_eq!(viewport_ack.base_ack, delivery.ack);
    assert_eq!(viewport_ack.binding, viewport_offer.binding);
    assert_eq!(viewport_ack.envelope, viewport_offer.envelope);
    assert_eq!(published_authoritative, PARAGRAPHS - 1);
    assert_eq!(published_unsupported, 1);
    assert_eq!(closures, PARAGRAPHS);
    let settled_baseline = runtime.arena_metrics();
    assert!(
        settled_baseline.resident_nodes <= baseline_metrics.resident_nodes
            && settled_baseline.live_payload_bytes <= baseline_metrics.live_payload_bytes
            && settled_baseline.reserved_external_payload_bytes
                > baseline_metrics.reserved_external_payload_bytes
            && settled_baseline.pending_reclaims == 0
            && settled_baseline.live_builds == 0,
        "ready viewport roots and authorities must leave only the retained reference index: \
             before={baseline_metrics:?}, after={settled_baseline:?}"
    );

    let mut transition_limited = command(5);
    transition_limited.limits.maximum_parser_transitions = 1;
    assert!(matches!(
        endpoint.request_viewport_inline_batch(&runtime, transition_limited),
        Err(CandidateEndpointError::ViewportInlineLimitExceeded(
            "recursive-Green viewport range budget"
        ))
    ));
    assert!(endpoint.viewport_inline_batch.is_none());
    assert!(endpoint.pending_viewport_unavailable.is_none());

    endpoint
        .request_viewport_inline_batch(&runtime, command(6))
        .expect("start parser-local preempted viewport batch");
    let parser_local_point = InlineRefinementCommand {
        binding,
        refinement_generation: 2,
        source_version: delivery.ack.source_version,
        base_ack: delivery.ack,
        byte_offset: u32::try_from(paragraph_starts[0]).expect("bounded point"),
        utf16_offset: 0,
        affinity: InlinePointAffinity::After,
        target: InlineRefinementTarget::Automatic,
    };
    endpoint
        .request_hot_inline(&mut runtime, parser_local_point)
        .expect("focused point preempts viewport work before Begin escapes");
    assert!(matches!(
        endpoint.viewport_inline_batch,
        Some(ViewportInlineBatchState::Cancelling(ref cleanup))
            if cleanup.hot_replacement.is_some()
    ));
    for _ in 0..1_000_000 {
        if endpoint.viewport_inline_batch.is_none() {
            break;
        }
        assert!(
            endpoint
                .poll_viewport_inline_batch(&mut runtime, 1)
                .expect("drain parser-local viewport preemption")
                <= 1
        );
    }
    assert!(matches!(
        endpoint.hot_inline,
        Some(HotInlineState::AwaitingReferenceResolver(_))
    ));
    endpoint.cancel_hot_inline();
    for _ in 0..1_000_000 {
        if !endpoint.hot_inline_has_poll_work() {
            break;
        }
        assert!(
            endpoint
                .poll_hot_inline(&mut runtime, 1)
                .expect("drain parser-local focused demand")
                <= 1
        );
    }
    assert!(!endpoint.hot_inline_has_poll_work());

    endpoint
        .request_viewport_inline_batch(&runtime, command(7))
        .expect("start post-Begin protected viewport batch");
    let begin_credit = loop {
        match endpoint
            .poll(&mut runtime, 1)
            .expect("derive cancellable viewport stream")
        {
            CandidatePoll::Pending { transitions } => assert!(transitions <= 1),
            CandidatePoll::ViewportPresentationEvent { event, .. } => {
                let CandidateViewportPresentationEvent {
                    credit,
                    body: CandidateViewportPresentationEventBody::Begin(begin),
                } = *event
                else {
                    panic!("cancellable viewport must emit Begin first")
                };
                assert_eq!(begin.binding.viewport_generation, 7);
                break credit;
            }
            CandidatePoll::Event { .. }
            | CandidatePoll::HotInlineEvent { .. }
            | CandidatePoll::ViewportPresentationUnavailable { .. } => {
                panic!("cancellable viewport emitted an unrelated event")
            }
        }
    };
    endpoint
        .accept_viewport_presentation_credit(begin_credit, 50_000)
        .expect("accept cancellable viewport Begin");
    assert!(matches!(
        endpoint
            .poll(&mut runtime, 1)
            .expect("buffer one partial viewport child"),
        CandidatePoll::Pending { transitions: 1 }
    ));
    assert!(matches!(
        endpoint.viewport_inline_batch,
        Some(ViewportInlineBatchState::Streaming(ref streaming))
            if streaming.active.is_some() && !streaming.packet.frames.is_empty()
    ));
    let post_begin_point = InlineRefinementCommand {
        binding,
        refinement_generation: 3,
        source_version: delivery.ack.source_version,
        base_ack: delivery.ack,
        byte_offset: u32::try_from(paragraph_starts[0]).expect("bounded point"),
        utf16_offset: 0,
        affinity: InlinePointAffinity::After,
        target: InlineRefinementTarget::Automatic,
    };
    assert!(matches!(
        endpoint.request_hot_inline(&mut runtime, post_begin_point),
        Err(CandidateEndpointError::Busy)
    ));
    assert!(matches!(
        endpoint.viewport_inline_batch,
        Some(ViewportInlineBatchState::Streaming(ref streaming))
            if streaming.phase != StreamPhase::NeedBegin
    ));
    assert!(endpoint.hot_inline.is_none());
    endpoint.cancel_viewport_presentation();
    assert!(matches!(
        endpoint.viewport_inline_batch,
        Some(ViewportInlineBatchState::Cancelling(_))
    ));
    for _ in 0..1_000_000 {
        if endpoint.viewport_inline_batch.is_none() {
            break;
        }
        assert!(
            endpoint
                .poll_viewport_inline_batch(&mut runtime, 1)
                .expect("drain preempted viewport batch")
                <= 1
        );
    }
    assert!(endpoint.viewport_inline_batch.is_none());
    endpoint
        .request_hot_inline(&mut runtime, post_begin_point)
        .expect("focused point retries after viewport terminal cleanup");
    assert!(matches!(
        endpoint.hot_inline,
        Some(HotInlineState::AwaitingReferenceResolver(_))
    ));
    endpoint.cancel_hot_inline();
    for _ in 0..1_000_000 {
        if !endpoint.hot_inline_has_poll_work() {
            break;
        }
        assert!(
            endpoint
                .poll_hot_inline(&mut runtime, 1)
                .expect("drain urgent point cancellation")
                <= 1
        );
    }
    assert!(!endpoint.hot_inline_has_poll_work());
    let after_preemption = runtime.arena_metrics();
    assert_eq!(
        after_preemption.resident_nodes,
        settled_baseline.resident_nodes
    );
    assert_eq!(
        after_preemption.live_payload_bytes,
        settled_baseline.live_payload_bytes
    );
    assert_eq!(
        after_preemption.reserved_external_payload_bytes,
        settled_baseline.reserved_external_payload_bytes
    );
    assert_eq!(after_preemption.pending_reclaims, 0);
    assert_eq!(after_preemption.live_builds, 0);
    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn late_inline_sidecar_publishes_authoritative_and_unsupported_then_cancels_exactly() {
    const SOURCE: &str = "p\n\n**bold**\n\nq";
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [701, 702, 703, 704],
        source_session_identity: 705,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start segmented candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent host");
    host.observe_source_version(source_version_for(binding, completion))
        .expect("host observes source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    let retained_descriptor = endpoint
        .retained
        .as_ref()
        .expect("retained structural base")
        .publication
        .descriptor(&runtime)
        .expect("retained descriptor");

    let command = |generation: u32, byte_offset: usize| InlineRefinementCommand {
        binding,
        refinement_generation: generation,
        source_version: delivery.ack.source_version,
        base_ack: delivery.ack,
        byte_offset: u32::try_from(byte_offset).expect("bounded point"),
        utf16_offset: u32::try_from(SOURCE[..byte_offset].encode_utf16().count())
            .expect("bounded UTF-16 point"),
        affinity: InlinePointAffinity::After,
        target: InlineRefinementTarget::Automatic,
    };
    let middle = SOURCE.find("**bold**").expect("middle Paragraph");
    endpoint
        .request_hot_inline(&mut runtime, command(1, middle))
        .expect("first demand");
    endpoint
        .request_hot_inline(&mut runtime, command(2, middle + 2))
        .expect("same-leaf demand coalesces");
    assert!(matches!(
        endpoint.request_hot_inline(&mut runtime, command(2, middle + 3)),
        Err(CandidateEndpointError::InvalidAuthority)
    ));
    let (authoritative_begin, authoritative_ack) =
        deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 10_000);
    assert_eq!(authoritative_begin.base_ack, delivery.ack);
    assert_eq!(authoritative_begin.binding.refinement_generation, 2);
    assert_eq!(
        authoritative_begin.binding.physical_start_utf8 as usize,
        middle
    );
    assert!(matches!(
        authoritative_begin.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative { fact_count, .. } if fact_count > 0
    ));
    assert_eq!(
        authoritative_ack.disposition,
        InlineSidecarAckDisposition::Authoritative
    );
    assert_eq!(
        authoritative_ack.transferred_node_count,
        authoritative_begin.envelope.transferred_node_count
    );

    let blank = middle - 1;
    endpoint
        .request_hot_inline(&mut runtime, command(3, blank))
        .expect("blank demand");
    let (unsupported_begin, unsupported_ack) =
        deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 20_000);
    assert!(matches!(
        unsupported_begin.envelope.disposition,
        HotInlineSidecarDisposition::Unsupported {
            reason: HOT_INLINE_UNSUPPORTED_NOT_INLINE_LEAF,
            ..
        }
    ));
    assert_eq!(
        unsupported_ack.disposition,
        InlineSidecarAckDisposition::Unsupported
    );
    assert_eq!(unsupported_ack.transferred_node_count, 1);
    assert_ne!(
        authoritative_begin.publication_session,
        unsupported_begin.publication_session
    );

    let tail = SOURCE.rfind('q').expect("tail Paragraph");
    endpoint
        .request_hot_inline(&mut runtime, command(4, tail))
        .expect("tail demand");
    let cancelled_begin = loop {
        match endpoint
            .poll(&mut runtime, 1)
            .expect("unit-fuel cancellable sidecar poll")
        {
            CandidatePoll::Pending { transitions } => assert!(transitions <= 1),
            CandidatePoll::HotInlineEvent { event, .. } => {
                let HotInlineEvent {
                    credit,
                    body: HotInlineEventBody::Begin(begin),
                } = *event
                else {
                    panic!("cancellable sidecar must begin before packet emission");
                };
                endpoint
                    .accept_hot_inline_credit(credit, 30_000)
                    .expect("accept cancellable Begin credit");
                break begin;
            }
            CandidatePoll::Event { .. } => {
                panic!("late inline demand must not republish structure")
            }
            CandidatePoll::ViewportPresentationEvent { .. } => {
                panic!("late inline demand must not emit viewport work")
            }
            CandidatePoll::ViewportPresentationUnavailable { .. } => {
                panic!("late inline demand emitted stale viewport unavailability")
            }
        }
    };
    assert_eq!(cancelled_begin.binding.refinement_generation, 4);
    endpoint.cancel_hot_inline();
    assert!(endpoint.hot_inline_sidecar.is_none());
    for _ in 0..100_000 {
        if !endpoint.hot_inline_has_poll_work() {
            break;
        }
        assert!(matches!(
            endpoint
                .poll(&mut runtime, 1)
                .expect("fuelled cancelled sidecar reclamation"),
            CandidatePoll::Pending { transitions } if transitions <= 1
        ));
    }
    assert!(!endpoint.hot_inline_has_poll_work());

    assert_eq!(
        endpoint
            .retained
            .as_ref()
            .expect("structural base remains retained")
            .publication
            .descriptor(&runtime)
            .expect("descriptor after inline demands"),
        retained_descriptor,
        "late inline work must not republish the canonical candidate"
    );
    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn character_references_reach_the_independent_host_as_fixed_width_cooked_scalars() {
    const SOURCE: &str = "&copy; &NotEqualTilde;";
    const FACT_RECORD_BYTES: usize = 20;
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [706, 707, 708, 709],
        source_session_identity: 710,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start character-reference candidate");
    let source_version = source_version_for(binding, completion);
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent character-reference host");
    host.observe_source_version(source_version)
        .expect("host observes character-reference source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    endpoint
        .request_hot_inline(
            &mut runtime,
            InlineRefinementCommand {
                binding,
                refinement_generation: 1,
                source_version: delivery.ack.source_version,
                base_ack: delivery.ack,
                byte_offset: 1,
                utf16_offset: 1,
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::Automatic,
            },
        )
        .expect("request character-reference inline authority");
    let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        35_000,
    );
    assert_eq!(begin.binding.physical_start_utf8, 0);
    assert_eq!(begin.binding.physical_end_utf8, SOURCE.len() as u32);
    assert_eq!(begin.binding.visible_start_utf8, 0);
    assert_eq!(begin.binding.visible_end_utf8, SOURCE.len() as u32);
    assert_eq!(begin.binding.visible_start_utf16, 0);
    assert_eq!(
        begin.binding.visible_end_utf16,
        SOURCE.encode_utf16().count() as u32
    );
    assert!(matches!(
        begin.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative { fact_count: 2, .. }
    ));
    assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);

    let mut facts = [0_u8; 2 * FACT_RECORD_BYTES];
    assert!(matches!(
        host.query_inline_sidecar(begin.binding, &mut facts)
            .expect("query character-reference inline sidecar"),
        HostInlineSidecarQueryOutcome::Authoritative {
            fact_count: 2,
            encoded_bytes: 40,
            ..
        }
    ));
    let first = &facts[..FACT_RECORD_BYTES];
    assert_eq!(first[0], M11InlineProjectionKind::CharacterReference as u8);
    assert_eq!(first[1], 1);
    assert_eq!(&first[2..4], &[0; 2]);
    assert_eq!(u32::from_le_bytes(first[4..8].try_into().unwrap()), 0);
    assert_eq!(u32::from_le_bytes(first[8..12].try_into().unwrap()), 6);
    assert_eq!(
        u32::from_le_bytes(first[12..16].try_into().unwrap()),
        '©' as u32
    );
    assert_eq!(u32::from_le_bytes(first[16..20].try_into().unwrap()), 0);

    let second = &facts[FACT_RECORD_BYTES..];
    assert_eq!(second[0], M11InlineProjectionKind::CharacterReference as u8);
    assert_eq!(second[1], 2);
    assert_eq!(&second[2..4], &[0; 2]);
    assert_eq!(u32::from_le_bytes(second[4..8].try_into().unwrap()), 7);
    assert_eq!(u32::from_le_bytes(second[8..12].try_into().unwrap()), 15);
    assert_eq!(
        u32::from_le_bytes(second[12..16].try_into().unwrap()),
        '\u{2242}' as u32
    );
    assert_eq!(
        u32::from_le_bytes(second[16..20].try_into().unwrap()),
        '\u{0338}' as u32
    );

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn retained_reference_winner_reaches_hot_inline_sidecar_without_shortcut_fallback() {
    const SOURCE: &str = "[foo][bar][baz] padding long enough\n\n[baz]: /baz\n";
    const FACT_RECORD_BYTES: usize = 20;
    const LINK_VALUE_PREFIX_BYTES: usize = 16;
    const LINK_VALUE_ENTRY_BYTES: usize = 32;
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [711, 712, 713, 714],
        source_session_identity: 715,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start reference candidate");
    let source_version = source_version_for(binding, completion);
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent reference host");
    host.observe_source_version(source_version)
        .expect("host observes reference source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    endpoint
        .request_hot_inline(
            &mut runtime,
            InlineRefinementCommand {
                binding,
                refinement_generation: 1,
                source_version: delivery.ack.source_version,
                base_ack: delivery.ack,
                byte_offset: 1,
                utf16_offset: 1,
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::Automatic,
            },
        )
        .expect("request reference inline authority");
    assert!(matches!(
        endpoint.hot_inline,
        Some(HotInlineState::AwaitingReferenceResolver(_))
    ));
    let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        35_000,
    );
    assert!(matches!(
        begin.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative { fact_count: 1, .. }
    ));
    assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);

    let mut encoded = [0_u8; 128];
    let HostInlineSidecarQueryOutcome::Authoritative {
        fact_count,
        value_entry_count,
        value_encoded_bytes,
        encoded_bytes,
        ..
    } = host
        .query_inline_sidecar(begin.binding, &mut encoded)
        .expect("query resolved-reference inline sidecar")
    else {
        panic!("resolved reference must publish authoritative inline facts")
    };
    let destination_start = SOURCE.find("/baz").expect("definition destination") as u32;
    assert_eq!(fact_count, 1);
    assert_eq!(value_entry_count, 1);
    assert_eq!(value_encoded_bytes, 52);
    assert_eq!(encoded_bytes, 72);

    let fact = &encoded[..FACT_RECORD_BYTES];
    assert_eq!(fact[0], M11InlineProjectionKind::ReferenceLink as u8);
    assert_eq!(u32::from_le_bytes(fact[4..8].try_into().unwrap()), 5);
    assert_eq!(u32::from_le_bytes(fact[8..12].try_into().unwrap()), 10);
    assert_eq!(u32::from_le_bytes(fact[12..16].try_into().unwrap()), 6);
    assert_eq!(u32::from_le_bytes(fact[16..20].try_into().unwrap()), 3);

    let values = &encoded[FACT_RECORD_BYTES..encoded_bytes as usize];
    assert_eq!(&values[..8], b"FLKIV001");
    assert_eq!(u32::from_le_bytes(values[8..12].try_into().unwrap()), 1);
    assert_eq!(u32::from_le_bytes(values[12..16].try_into().unwrap()), 1);
    let entry = &values[LINK_VALUE_PREFIX_BYTES..];
    assert_eq!(u32::from_le_bytes(entry[0..4].try_into().unwrap()), 0);
    assert_eq!(u32::from_le_bytes(entry[4..8].try_into().unwrap()), 0);
    assert_eq!(
        u32::from_le_bytes(entry[8..12].try_into().unwrap()),
        destination_start
    );
    assert_eq!(u32::from_le_bytes(entry[12..16].try_into().unwrap()), 4);
    assert_eq!(u32::from_le_bytes(entry[16..20].try_into().unwrap()), 0);
    assert_eq!(u32::from_le_bytes(entry[20..24].try_into().unwrap()), 0);
    assert_eq!(u32::from_le_bytes(entry[24..28].try_into().unwrap()), 4);
    assert_eq!(u32::from_le_bytes(entry[28..32].try_into().unwrap()), 0);
    assert_eq!(
        &entry[LINK_VALUE_ENTRY_BYTES..LINK_VALUE_ENTRY_BYTES + 4],
        b"/baz"
    );

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn length_changing_direct_link_edit_before_late_references_recertifies_inline() {
    const BASE_SOURCE: &str = "Read the [Flark architecture notes](https://flark.dev/revision-7 \"Revision 7\").\n\nReference [full][launch notes].\n\n[launch notes]: https://flark.dev/launch \"Launch notes\"\n";
    const ORIGINAL_LABEL: &str = "Flark architecture notes";
    const TARGET_LABEL: &str = "Flark design notes";
    let profile = SourceFactsScanProfile::new(4).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [716, 717, 718, 719],
        source_session_identity: 720,
        worker_generation: 1,
    };
    let mut runtime = DocumentRuntime::new(BASE_SOURCE, standard_document_runtime_config())
        .expect("revision-replacement runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start reference-bearing base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent revision-replacement host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes reference-bearing base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let direct_point = BASE_SOURCE
        .find(ORIGINAL_LABEL)
        .expect("direct-link label point");
    endpoint
        .request_hot_inline(
            &mut runtime,
            InlineRefinementCommand {
                binding,
                refinement_generation: 1,
                source_version: base_delivery.ack.source_version,
                base_ack: base_delivery.ack,
                byte_offset: u32::try_from(direct_point).expect("bounded point"),
                utf16_offset: u32::try_from(BASE_SOURCE[..direct_point].encode_utf16().count())
                    .expect("bounded UTF-16 point"),
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::Automatic,
            },
        )
        .expect("request active direct-link authority");
    let (base_inline, _) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        36_000,
    );
    assert!(matches!(
        base_inline.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative { fact_count: 1, .. }
    ));
    assert!(endpoint.hot_inline_has_poll_work());

    let label_start = BASE_SOURCE.find(ORIGINAL_LABEL).expect("edit label");
    let target_version = runtime
        .apply_edit(
            base_version,
            label_start..label_start + ORIGINAL_LABEL.len(),
            TARGET_LABEL,
        )
        .expect("length-changing direct-link edit")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan direct-link SourceFacts");
    assert!(endpoint
        .has_incremental_base_for_plan(&runtime, &plan)
        .expect("preflight direct-link exact base"));
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("borrow direct-link target");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    host.observe_source_version(source_version_for(binding, target_completion))
        .expect("host observes direct-link target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start direct-link revision replacement");
    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(target_delivery.offer.mode, PublicationMode::FullSnapshot);

    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(!endpoint.hot_inline_has_poll_work());
    endpoint
        .request_hot_inline(
            &mut runtime,
            InlineRefinementCommand {
                binding,
                refinement_generation: 2,
                source_version: target_delivery.ack.source_version,
                base_ack: target_delivery.ack,
                byte_offset: u32::try_from(direct_point).expect("bounded point"),
                utf16_offset: u32::try_from(BASE_SOURCE[..direct_point].encode_utf16().count())
                    .expect("bounded UTF-16 point"),
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::Automatic,
            },
        )
        .expect("request recertified direct-link authority");
    let (target_inline, _) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        37_000,
    );
    assert!(matches!(
        target_inline.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative { fact_count: 1, .. }
    ));
    assert_eq!(target_delivery.ack.source_version.revision, 1);
    assert_eq!(target_version.revision().get(), 1);

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn indented_code_stays_structural_and_inline_sidecar_fails_closed() {
    const SOURCE: &str = "\u{feff}\tα\0\r\n\n      \r    \tβ\r\tlast";
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [706, 707, 708, 709],
        source_session_identity: 710,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start segmented candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent host");
    host.observe_source_version(source_version_for(binding, completion))
        .expect("host observes source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let command = |generation: u32| InlineRefinementCommand {
        binding,
        refinement_generation: generation,
        source_version: delivery.ack.source_version,
        base_ack: delivery.ack,
        byte_offset: 0,
        utf16_offset: 0,
        affinity: InlinePointAffinity::After,
        target: InlineRefinementTarget::Automatic,
    };

    endpoint
        .request_hot_inline(&mut runtime, command(1))
        .expect("indented-code inline demand");
    let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        70_000,
    );
    assert_eq!(begin.binding.refinement_generation, 1);
    assert_eq!(begin.binding.physical_start_utf8, 0);
    assert!(begin.binding.physical_end_utf8 > begin.binding.physical_start_utf8);
    assert!(matches!(
        begin.envelope.disposition,
        HotInlineSidecarDisposition::Unsupported {
            reason: HOT_INLINE_UNSUPPORTED_NOT_INLINE_LEAF,
            ..
        }
    ));
    assert_eq!(ack.disposition, InlineSidecarAckDisposition::Unsupported);

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

fn assert_block_quote_request_reaches_typed_sidecar(
    source: &str,
    session_seed: u32,
    point: usize,
    expected_physical_start: u32,
    expected_physical_end: u32,
    expected_records: &[[u32; 5]],
) {
    const LINE_RECORD_BYTES: usize = 20;
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [
            session_seed,
            session_seed + 1,
            session_seed + 2,
            session_seed + 3,
        ],
        source_session_identity: session_seed + 4,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(source, standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start segmented candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent host");
    host.observe_source_version(source_version_for(binding, completion))
        .expect("host observes source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let point_utf16 = source[..point].encode_utf16().count();
    let command = |generation: u32| InlineRefinementCommand {
        binding,
        refinement_generation: generation,
        source_version: delivery.ack.source_version,
        base_ack: delivery.ack,
        // The active point is source-backed quote content. Marker-owned
        // coverage intentionally does not impersonate its Paragraph child.
        byte_offset: u32::try_from(point).expect("bounded point"),
        utf16_offset: u32::try_from(point_utf16).expect("bounded point"),
        affinity: InlinePointAffinity::After,
        target: InlineRefinementTarget::BlockQuoteProjection,
    };

    endpoint
        .request_hot_inline(&mut runtime, command(1))
        .expect("first block-quote demand");
    endpoint.cancel_hot_inline();
    while endpoint.hot_inline_has_poll_work() {
        assert!(
            endpoint
                .poll_hot_inline(&mut runtime, 1)
                .expect("reclaim cancelled Green block quote")
                <= 1
        );
    }
    assert!(!endpoint.hot_inline_has_poll_work());

    endpoint
        .request_hot_inline(&mut runtime, command(2))
        .expect("replacement block-quote demand");
    let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        80_000,
    );
    let expected_physical_start_utf16 = u32::try_from(
        source[..usize::try_from(expected_physical_start).expect("bounded start")]
            .encode_utf16()
            .count(),
    )
    .expect("bounded source start");
    let expected_physical_end_utf16 = u32::try_from(
        source[..usize::try_from(expected_physical_end).expect("bounded end")]
            .encode_utf16()
            .count(),
    )
    .expect("bounded source end");
    assert_eq!(begin.binding.refinement_generation, 2);
    assert_eq!(begin.binding.physical_start_utf8, expected_physical_start);
    assert_eq!(begin.binding.physical_end_utf8, expected_physical_end);
    assert_eq!(begin.binding.visible_start_utf8, expected_physical_start);
    assert_eq!(begin.binding.visible_end_utf8, expected_physical_end);
    assert_eq!(
        begin.binding.physical_start_utf16,
        expected_physical_start_utf16
    );
    assert_eq!(
        begin.binding.physical_end_utf16,
        expected_physical_end_utf16
    );
    assert_eq!(
        begin.binding.visible_start_utf16,
        expected_physical_start_utf16
    );
    assert_eq!(begin.binding.visible_end_utf16, expected_physical_end_utf16);
    assert!(matches!(
        begin.binding.owner(),
        Some(HotInlineSidecarOwner::RecursiveGreenFrame(_))
    ));
    assert!(matches!(
        begin.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative {
            fact_count: 3,
            logical_page_count: 1,
            ..
        }
    ));
    assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);
    assert_eq!(
        ack.transferred_node_count,
        begin.envelope.transferred_node_count
    );

    let mut encoded_lines = vec![0_u8; expected_records.len() * LINE_RECORD_BYTES];
    let query = host
        .query_inline_sidecar(begin.binding, &mut encoded_lines)
        .expect("query typed block-quote sidecar");
    assert!(matches!(
        query,
        HostInlineSidecarQueryOutcome::Authoritative {
            payload_kind: HostInlineSidecarPayloadKind::BlockQuote,
            fact_count: 3,
            encoded_bytes: 60,
            ..
        }
    ));
    let observed = encoded_lines
        .chunks_exact(LINE_RECORD_BYTES)
        .map(|record| {
            std::array::from_fn::<u32, 5, _>(|field| {
                let start = field * 4;
                u32::from_le_bytes(
                    record[start..start + 4]
                        .try_into()
                        .expect("four-byte line field"),
                )
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(observed, expected_records);

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn block_quote_request_reaches_typed_sidecar_and_reclaims_with_unit_fuel() {
    const SOURCE: &str = "\u{feff}   > α😀\r\n> β\rlazy😀\0";
    assert_block_quote_request_reaches_typed_sidecar(
        SOURCE,
        716,
        8,
        0,
        u32::try_from(SOURCE.len()).expect("bounded source"),
        &[[0, 16, 8, 6, 1], [16, 5, 2, 2, 1], [21, 9, 0, 9, 2]],
    );
}

#[test]
fn block_quote_inline_projects_strong_and_code_into_marker_free_coordinates() {
    const SOURCE: &str = "> **first\n> second** and `code`\n";
    const FACT_RECORD_BYTES: usize = 20;
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [722, 723, 724, 725],
        source_session_identity: 726,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start projected-inline candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent host");
    host.observe_source_version(source_version_for(binding, completion))
        .expect("host observes source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let point = SOURCE.find("second").expect("second quote line") + 1;
    endpoint
        .request_hot_inline(
            &mut runtime,
            InlineRefinementCommand {
                binding,
                refinement_generation: 1,
                source_version: delivery.ack.source_version,
                base_ack: delivery.ack,
                byte_offset: u32::try_from(point).expect("bounded point"),
                utf16_offset: u32::try_from(SOURCE[..point].encode_utf16().count())
                    .expect("bounded UTF-16 point"),
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::BlockQuoteInline,
            },
        )
        .expect("request marker-free block-quote inline authority");
    let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        100_000,
    );
    assert!(matches!(
        begin.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative {
            fact_count: 2,
            link_value_entry_count: 0,
            link_value_encoded_bytes: 0,
            link_value_storage_page_count: 0,
            ..
        }
    ));
    assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);

    let mut encoded = [0_u8; 2 * FACT_RECORD_BYTES];
    assert!(matches!(
        host.query_inline_sidecar(begin.binding, &mut encoded)
            .expect("query projected-inline sidecar"),
        HostInlineSidecarQueryOutcome::Authoritative {
            payload_kind: HostInlineSidecarPayloadKind::ProjectedInline,
            fact_count: 2,
            value_entry_count: 0,
            value_encoded_bytes: 0,
            encoded_bytes: 40,
            ..
        }
    ));

    let decode = |record: &[u8]| {
        (
            record[0],
            u32::from_le_bytes(record[4..8].try_into().expect("fact start")),
            u32::from_le_bytes(record[8..12].try_into().expect("fact length")),
            u32::from_le_bytes(record[12..16].try_into().expect("content start")),
            u32::from_le_bytes(record[16..20].try_into().expect("content length")),
        )
    };
    assert_eq!(
        decode(&encoded[..FACT_RECORD_BYTES]),
        (M11InlineProjectionKind::Strong as u8, 0, 16, 2, 12)
    );
    assert_eq!(
        decode(&encoded[FACT_RECORD_BYTES..]),
        (M11InlineProjectionKind::Code as u8, 21, 6, 22, 4)
    );

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn block_quote_with_sibling_blocks_retains_its_closing_terminator() {
    const SOURCE: &str = "before\n\n> alpha\n> beta\nlazy\n\n*tail*";
    let point = SOURCE.find("alpha").expect("quote content") + 2;
    assert_block_quote_request_reaches_typed_sidecar(
        SOURCE,
        721,
        point,
        8,
        28,
        &[[0, 8, 2, 5, 1], [8, 7, 2, 4, 1], [15, 5, 0, 4, 2]],
    );
}

#[test]
fn legacy_list_item_targets_are_typed_unsupported_on_recursive_green() {
    const SOURCE: &str = "- **bold** *em* `code`\r\n\
             - plain\r\n";
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [726, 727, 728, 729],
        source_session_identity: 730,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start segmented candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent host");
    host.observe_source_version(source_version_for(binding, completion))
        .expect("host observes source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let point = SOURCE.find("bold").expect("selected item content") + 2;
    endpoint
        .request_hot_inline(
            &mut runtime,
            InlineRefinementCommand {
                binding,
                refinement_generation: 1,
                source_version: delivery.ack.source_version,
                base_ack: delivery.ack,
                byte_offset: u32::try_from(point).expect("bounded point"),
                utf16_offset: u32::try_from(point).expect("ASCII point"),
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::BulletListItemProjection,
            },
        )
        .expect("request selected-item structural authority");
    let (projection_begin, projection_ack) =
        deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            75_000,
        );
    assert_eq!(projection_begin.binding.refinement_generation, 1);
    assert!(projection_begin.binding.physical_start_utf8 <= point as u32);
    assert!(projection_begin.binding.physical_end_utf8 > point as u32);
    assert!(matches!(
        projection_begin.envelope.disposition,
        HotInlineSidecarDisposition::Unsupported {
            reason: HOT_INLINE_UNSUPPORTED_LEGACY_BLOCK_TARGET,
            ..
        }
    ));
    assert_eq!(
        projection_ack.disposition,
        InlineSidecarAckDisposition::Unsupported
    );

    endpoint
        .request_hot_inline(
            &mut runtime,
            InlineRefinementCommand {
                binding,
                refinement_generation: 2,
                source_version: delivery.ack.source_version,
                base_ack: delivery.ack,
                byte_offset: u32::try_from(point).expect("bounded point"),
                utf16_offset: u32::try_from(point).expect("ASCII point"),
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::BulletListItemInline,
            },
        )
        .expect("request selected-item inline authority");
    let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        75_000,
    );

    assert_eq!(begin.binding.refinement_generation, 2);
    assert!(begin.binding.physical_start_utf8 <= point as u32);
    assert!(begin.binding.physical_end_utf8 > point as u32);
    assert!(matches!(
        begin.envelope.disposition,
        HotInlineSidecarDisposition::Unsupported {
            reason: HOT_INLINE_UNSUPPORTED_LEGACY_BLOCK_TARGET,
            ..
        }
    ));
    assert_eq!(ack.disposition, InlineSidecarAckDisposition::Unsupported);

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn ordered_item_targets_preserve_exact_metadata_and_fail_closed_across_lifecycle_edges() {
    const SOURCE: &str =
        "- bullet\r\n- tail\r\n\r\n7) first\r\n00042) **bold** 😀\r\n900) tail\r\n0)   ";
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [736, 737, 738, 739],
        source_session_identity: 740,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start ordered candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent host");
    host.observe_source_version(source_version_for(binding, completion))
        .expect("host observes ordered source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let command = |generation: u32, byte_offset: usize, target: InlineRefinementTarget| {
        InlineRefinementCommand {
            binding,
            refinement_generation: generation,
            source_version: delivery.ack.source_version,
            base_ack: delivery.ack,
            byte_offset: u32::try_from(byte_offset).expect("bounded ordered point"),
            utf16_offset: u32::try_from(SOURCE[..byte_offset].encode_utf16().count())
                .expect("bounded ordered UTF-16 point"),
            affinity: InlinePointAffinity::After,
            target,
        }
    };
    let middle = SOURCE.find("**bold**").expect("ordered middle content") + 2;

    if endpoint
        .recursive_green
        .has_installed_session_for(delivery.ack)
    {
        let (owner_kind, _, ancestry) = recursive_green_query_shape(
            &host,
            delivery.ack.source_version,
            middle,
            SOURCE[..middle].encode_utf16().count(),
        );
        assert_eq!(owner_kind, 5);
        assert_eq!(ancestry, vec![1, 3, 4, 5]);
        for (generation, target) in [
            (1, InlineRefinementTarget::BulletListItemProjection),
            (2, InlineRefinementTarget::OrderedListItemProjection),
            (3, InlineRefinementTarget::OrderedListItemInline),
        ] {
            endpoint
                .request_hot_inline(&mut runtime, command(generation, middle, target))
                .expect("legacy list target becomes typed unsupported on Green authority");
            let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
                &mut endpoint,
                &mut runtime,
                &mut host,
                75_000,
            );
            assert!(matches!(
                begin.envelope.disposition,
                HotInlineSidecarDisposition::Unsupported {
                    reason: HOT_INLINE_UNSUPPORTED_LEGACY_BLOCK_TARGET,
                    ..
                }
            ));
            assert_eq!(ack.disposition, InlineSidecarAckDisposition::Unsupported);
        }
        let mut stale = command(4, middle, InlineRefinementTarget::OrderedListItemProjection);
        stale.source_version.revision += 1;
        assert!(matches!(
            endpoint.request_hot_inline(&mut runtime, stale),
            Err(CandidateEndpointError::InvalidAuthority)
        ));
        endpoint
            .request_hot_inline(
                &mut runtime,
                command(4, middle, InlineRefinementTarget::OrderedListItemProjection),
            )
            .expect("request cancellable typed unsupported target");
        endpoint.cancel_hot_inline();
        while endpoint.hot_inline_has_poll_work() {
            assert!(
                endpoint
                    .poll_hot_inline(&mut runtime, 1)
                    .expect("reclaim cancelled ordered target")
                    <= 1
            );
        }
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
        return;
    }

    assert!(matches!(
        endpoint.request_hot_inline(
            &mut runtime,
            command(1, middle, InlineRefinementTarget::BulletListItemProjection,),
        ),
        Err(CandidateEndpointError::Derive(
            M11CandidateDerivationError::PublishedBulletListLeafFenceNotBulletList
        ))
    ));
    assert!(endpoint.hot_inline.is_none());

    let bullet = SOURCE.find("bullet").expect("bullet-list item") + 2;
    assert!(matches!(
        endpoint.request_hot_inline(
            &mut runtime,
            command(1, bullet, InlineRefinementTarget::OrderedListItemProjection,),
        ),
        Err(CandidateEndpointError::Derive(
            M11CandidateDerivationError::PublishedOrderedListLeafFenceNotOrderedList
        ))
    ));
    assert!(endpoint.hot_inline.is_none());

    let mut stale = command(1, middle, InlineRefinementTarget::OrderedListItemProjection);
    stale.source_version.revision = stale
        .source_version
        .revision
        .checked_add(1)
        .expect("bounded stale revision");
    assert!(matches!(
        endpoint.request_hot_inline(&mut runtime, stale),
        Err(CandidateEndpointError::InvalidAuthority)
    ));
    assert!(endpoint.hot_inline.is_none());

    endpoint
        .request_hot_inline(
            &mut runtime,
            command(1, middle, InlineRefinementTarget::OrderedListItemProjection),
        )
        .expect("request cancellable ordered projection");
    assert!(matches!(
        endpoint
            .poll(&mut runtime, 1)
            .expect("bounded ordered projection"),
        CandidatePoll::Pending { transitions } if transitions <= 1
    ));
    endpoint.cancel_hot_inline();
    for _ in 0..100_000 {
        if !endpoint.hot_inline_has_poll_work() {
            break;
        }
        assert!(matches!(
            endpoint
                .poll(&mut runtime, 1)
                .expect("fuelled ordered cancellation"),
            CandidatePoll::Pending { transitions } if transitions <= 1
        ));
    }
    assert!(!endpoint.hot_inline_has_poll_work());

    endpoint
        .request_hot_inline(
            &mut runtime,
            command(2, middle, InlineRefinementTarget::OrderedListItemProjection),
        )
        .expect("request exact ordered projection");
    for _ in 0..100_000 {
        endpoint
            .poll_hot_inline(&mut runtime, 1)
            .expect("fuelled ordered projection");
        if matches!(endpoint.hot_inline, Some(HotInlineState::Ready(_))) {
            break;
        }
    }
    let Some(HotInlineState::Ready(ready)) = endpoint.hot_inline.as_ref() else {
        panic!("ordered projection did not become ready");
    };
    let HotInlineReadyPublication::Authoritative(root) = &ready.publication else {
        panic!("ordered projection must be authoritative");
    };
    let HotInlineProjectionRoot::OrderedListItem {
        root,
        selected_item_ordinal,
        canonical_line_ending,
        opening_marker_start,
        opening_marker_end,
        marker_value,
    } = root.as_ref()
    else {
        panic!("ordered demand must not become a bullet-list root");
    };
    assert_eq!(
        root.descriptor().projection_kind(),
        M11MarkedLineProjectionKind::OrderedList
    );
    assert_eq!(*selected_item_ordinal, 1);
    assert_eq!(
        *canonical_line_ending,
        M11HotInlineCanonicalLineEnding::CrLf
    );
    assert_eq!((*opening_marker_start, *opening_marker_end), (0, 6));
    assert_eq!(*marker_value, 42);

    let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        75_000,
    );
    assert_eq!(begin.binding.refinement_generation, 2);
    assert!(matches!(
        begin.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative {
            fact_count: 1,
            logical_page_count: 1,
            ..
        }
    ));
    assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);

    let terminal = SOURCE.rfind("0)").expect("terminal ordered item");
    endpoint
        .request_hot_inline(
            &mut runtime,
            command(
                3,
                terminal + 1,
                InlineRefinementTarget::OrderedListItemProjection,
            ),
        )
        .expect("request terminal ordered projection");
    for _ in 0..100_000 {
        endpoint
            .poll_hot_inline(&mut runtime, 1)
            .expect("fuelled terminal ordered projection");
        if matches!(endpoint.hot_inline, Some(HotInlineState::Ready(_))) {
            break;
        }
    }
    let Some(HotInlineState::Ready(ready)) = endpoint.hot_inline.as_ref() else {
        panic!("terminal ordered projection did not become ready");
    };
    let HotInlineReadyPublication::Authoritative(root) = &ready.publication else {
        panic!("terminal ordered projection must remain authoritative");
    };
    let HotInlineProjectionRoot::OrderedListItem {
        root,
        selected_item_ordinal,
        canonical_line_ending,
        opening_marker_start,
        opening_marker_end,
        marker_value,
    } = root.as_ref()
    else {
        panic!("terminal ordered demand lost its typed root");
    };
    assert_eq!(
        root.descriptor().projection_kind(),
        M11MarkedLineProjectionKind::OrderedList
    );
    assert_eq!(root.descriptor().projected_utf8_length(), 0);
    assert_eq!(root.descriptor().projected_utf16_length(), 0);
    assert_eq!(*selected_item_ordinal, 3);
    assert_eq!(
        *canonical_line_ending,
        M11HotInlineCanonicalLineEnding::CrLf
    );
    assert_eq!((*opening_marker_start, *opening_marker_end), (0, 2));
    assert_eq!(*marker_value, 0);

    let (terminal_begin, terminal_ack) =
        deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            85_000,
        );
    assert_eq!(terminal_begin.binding.refinement_generation, 3);
    assert!(matches!(
        terminal_begin.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative { fact_count: 1, .. }
    ));
    assert_eq!(
        terminal_ack.disposition,
        InlineSidecarAckDisposition::Authoritative
    );

    endpoint
        .request_hot_inline(
            &mut runtime,
            command(
                4,
                terminal + 1,
                InlineRefinementTarget::OrderedListItemInline,
            ),
        )
        .expect("terminal ordered inline target fails closed as unsupported");
    let (unsupported_begin, unsupported_ack) =
        deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            95_000,
        );
    assert!(matches!(
        unsupported_begin.envelope.disposition,
        HotInlineSidecarDisposition::Unsupported {
            reason: HOT_INLINE_UNSUPPORTED_NOT_INLINE_LEAF,
            ..
        }
    ));
    assert_eq!(
        unsupported_ack.disposition,
        InlineSidecarAckDisposition::Unsupported
    );

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn terminal_empty_list_item_reaches_host_as_marker_free_editable_row() {
    const SOURCE: &str = "- alpha\n-   ";
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [746, 747, 748, 749],
        source_session_identity: 750,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start terminal-empty candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("terminal-empty host");
    host.observe_source_version(source_version_for(binding, completion))
        .expect("host observes terminal-empty source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let eof_bytes = SOURCE.len() as u64;
    let eof_utf16 = SOURCE.encode_utf16().count() as u64;
    let rows = endpoint
        .recursive_green
        .installed_session(delivery.ack)
        .expect("terminal-empty Green session remains current")
        .query_renderable_rows(
            &runtime,
            M11RecursiveGreenPoint::new(0, 0, SourceBoundaryAffinity::After),
            eof_bytes,
            M11RecursiveGreenRowQueryLimits::new(8, 25, 512, 16, 512)
                .expect("terminal-empty row limits"),
        )
        .expect("query terminal-empty rows");
    assert!(rows.complete());
    assert_eq!(rows.rows().len(), 2);
    assert_eq!(rows.rows()[0].kind().get(), 5);
    let first_physical = rows.rows()[0].physical_range();
    let first_physical_utf16 = rows.rows()[0].physical_utf16_range();
    let empty = &rows.rows()[1];
    assert_eq!(empty.kind().get(), 14);
    assert_eq!(empty.physical_range(), eof_bytes..eof_bytes);
    assert_eq!(empty.physical_utf16_range(), eof_utf16..eof_utf16);
    assert_eq!(empty.editable_range(), Some(eof_bytes..eof_bytes));
    assert_eq!(empty.editable_utf16_range(), Some(eof_utf16..eof_utf16));
    assert_eq!(
        empty
            .path()
            .iter()
            .map(|frame| frame.kind().get())
            .collect::<Vec<_>>(),
        vec![1, 3, 4, 14]
    );
    let list = empty.path()[1]
        .property()
        .expect("terminal row retains List facts");
    assert_eq!(list.tag().get(), 1);
    assert_eq!(&list.as_bytes()[..2], &[1, b'-']);
    let item = empty.path()[2]
        .property()
        .expect("terminal row retains Item facts");
    assert_eq!(item.tag().get(), 2);
    let item_start = SOURCE.rfind("-   ").expect("terminal marker-only item") as u64;
    assert_eq!(empty.path()[2].physical_range(), item_start..eof_bytes);

    let (owner_kind, point_range, point_ancestry) = recursive_green_query_shape(
        &host,
        delivery.ack.source_version,
        eof_bytes as usize,
        eof_utf16 as usize,
    );
    assert_eq!(owner_kind, 14, "EOF selects the terminal-empty row");
    assert_eq!(
        point_range,
        [
            eof_bytes as u32,
            eof_bytes as u32,
            eof_utf16 as u32,
            eof_utf16 as u32,
        ]
    );
    assert_eq!(point_ancestry, vec![1, 3, 4, 14]);

    let requested_range = HostMetricRange {
        start: HostSourceMetric { bytes: 0, utf16: 0 },
        end: HostSourceMetric {
            bytes: eof_bytes as u32,
            utf16: eof_utf16 as u32,
        },
    };
    let mut encoded_rows = vec![0xa5_u8; 16 * 1024];
    let HostBlockRangeOutcome::Page {
        covered_range,
        continuation,
        receipt,
        ..
    } = host
        .query_structural_range(
            HostBlockRangeQuery {
                source_version: delivery.ack.source_version,
                requested_range,
                budget: HostBlockRangeBudget {
                    maximum_encoded_bytes: encoded_rows.len() as u32,
                    maximum_block_count: 8,
                    maximum_storage_pages_visited: 25,
                    maximum_open_depth: 16,
                    maximum_tree_nodes_visited: 512,
                },
                continuation: None,
            },
            &mut encoded_rows,
        )
        .expect("query terminal-empty row directory")
    else {
        panic!("terminal-empty rows must reach the independent host");
    };
    assert_eq!(covered_range, requested_range);
    assert!(continuation.is_none());
    assert!(receipt.complete);
    assert_eq!(receipt.block_count, 2);
    assert_eq!(
        receipt.encoded_bytes as usize,
        HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES
            + 2 * HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES
            + 8 * HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES
    );
    let read_u16 = |offset: usize| {
        u16::from_le_bytes(
            encoded_rows[offset..offset + 2]
                .try_into()
                .expect("wire u16"),
        )
    };
    let read_u32 = |offset: usize| {
        u32::from_le_bytes(
            encoded_rows[offset..offset + 4]
                .try_into()
                .expect("wire u32"),
        )
    };
    let empty_record =
        HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES + HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES;
    assert_eq!(read_u16(empty_record + 16), 14);
    assert_eq!(
        (
            read_u32(empty_record + 32),
            read_u32(empty_record + 36),
            read_u32(empty_record + 40),
            read_u32(empty_record + 44),
        ),
        (
            eof_bytes as u32,
            eof_utf16 as u32,
            eof_bytes as u32,
            eof_utf16 as u32
        )
    );
    assert_eq!(
        (
            read_u32(empty_record + 48),
            read_u32(empty_record + 52),
            read_u32(empty_record + 56),
            read_u32(empty_record + 60),
        ),
        (
            eof_bytes as u32,
            eof_utf16 as u32,
            eof_bytes as u32,
            eof_utf16 as u32
        )
    );

    endpoint
        .request_viewport_inline_batch(
            &runtime,
            ViewportInlineBatchCommand {
                binding,
                viewport_generation: 1,
                source_version: delivery.ack.source_version,
                base_ack: delivery.ack,
                start_entry_ordinal: rows.start_ordinal(),
                start_byte_offset: first_physical.start as u32,
                start_utf16_offset: first_physical_utf16.start as u32,
                end_byte_offset: eof_bytes as u32,
                end_utf16_offset: eof_utf16 as u32,
                limits: ViewportInlineBatchLimits {
                    maximum_structural_entries: 2,
                    maximum_storage_pages: 25,
                    maximum_inline_leaves: 1,
                    maximum_inline_leaf_source_bytes: 64,
                    maximum_inline_source_bytes: 64,
                    maximum_fact_records: 64,
                    maximum_projection_bytes: 64 * 1024,
                    maximum_parser_transitions: 10_000,
                },
            },
        )
        .expect("request terminal-empty viewport");
    for _ in 0..10_000 {
        if matches!(
            endpoint.viewport_inline_batch,
            Some(ViewportInlineBatchState::Ready(_))
        ) {
            break;
        }
        assert!(
            endpoint
                .poll_viewport_inline_batch(&mut runtime, 1)
                .expect("poll terminal-empty viewport")
                <= 1
        );
    }
    let Some(ViewportInlineBatchState::Ready(ready)) = endpoint.viewport_inline_batch.as_ref()
    else {
        panic!("terminal-empty viewport did not become ready");
    };
    assert_eq!(ready.range_receipt.visited_entries(), 2);
    assert_eq!(ready.leaves.len(), 1, "the empty row needs no HIO1 child");
    assert_eq!(ready.total_ready_roots, 1);
    let (_, _, authoritative, unsupported, closures) =
        deliver_viewport_presentation_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(authoritative + unsupported, 1);
    assert_eq!(closures, 1);

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn bullet_list_request_reaches_typed_sidecar_and_selected_item_path() {
    const SOURCE: &str = "\u{feff}  -  α😀\r\n  - β\r-   ";
    const ITEM_RECORD_BYTES: usize = 28;
    const VIEWPORT_HEADER_BYTES: usize = 32;
    const GREEN_RECORD_BYTES: usize = 80;
    const PROJECTION_RECORD_BYTES: usize = 56;
    const POINT_PATH_NODE_BYTES: usize = 32;
    const POINT_PATH_BYTES: usize = 3 * POINT_PATH_NODE_BYTES;
    const VIEWPORT_BYTES: usize = VIEWPORT_HEADER_BYTES
        + GREEN_RECORD_BYTES
        + PROJECTION_RECORD_BYTES
        + POINT_PATH_BYTES
        + 3 * ITEM_RECORD_BYTES;
    const TERMINAL_POINT_PATH_BYTES: usize = 2 * POINT_PATH_NODE_BYTES;
    const TERMINAL_VIEWPORT_BYTES: usize = VIEWPORT_HEADER_BYTES
        + GREEN_RECORD_BYTES
        + PROJECTION_RECORD_BYTES
        + TERMINAL_POINT_PATH_BYTES
        + 3 * ITEM_RECORD_BYTES;
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [731, 732, 733, 734],
        source_session_identity: 735,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start segmented candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent host");
    host.observe_source_version(source_version_for(binding, completion))
        .expect("host observes source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let command = |generation: u32| InlineRefinementCommand {
        binding,
        refinement_generation: generation,
        source_version: delivery.ack.source_version,
        base_ack: delivery.ack,
        byte_offset: 20,
        utf16_offset: 15,
        affinity: InlinePointAffinity::After,
        target: InlineRefinementTarget::Automatic,
    };
    if endpoint
        .recursive_green
        .has_installed_session_for(delivery.ack)
    {
        let (owner_kind, range, ancestry) =
            recursive_green_query_shape(&host, delivery.ack.source_version, 20, 15);
        assert_eq!(owner_kind, 5);
        assert_eq!(ancestry, vec![1, 3, 4, 5]);
        let owner_frame = recursive_green_owner_frame(&host, delivery.ack.source_version, 20, 15);
        endpoint
            .request_hot_inline(&mut runtime, command(1))
            .expect("request cancellable Green list Paragraph");
        endpoint.cancel_hot_inline();
        while endpoint.hot_inline_has_poll_work() {
            assert!(
                endpoint
                    .poll_hot_inline(&mut runtime, 1)
                    .expect("reclaim cancelled Green list Paragraph")
                    <= 1
            );
        }
        endpoint
            .request_hot_inline(&mut runtime, command(2))
            .expect("request Green list Paragraph");
        let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            80_000,
        );
        assert_eq!(
            begin.binding.owner(),
            Some(HotInlineSidecarOwner::RecursiveGreenFrame(owner_frame))
        );
        assert_eq!(begin.binding.physical_start_utf8, range[0]);
        assert_eq!(begin.binding.physical_end_utf8, range[1] + 1);
        assert!(matches!(
            begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative { fact_count: 0, .. }
        ));
        assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);
        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    refinement_generation: 3,
                    target: InlineRefinementTarget::BulletListItemProjection,
                    ..command(3)
                },
            )
            .expect("request typed legacy list target");
        let (unsupported, unsupported_ack) =
            deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
                &mut endpoint,
                &mut runtime,
                &mut host,
                80_000,
            );
        assert!(matches!(
            unsupported.envelope.disposition,
            HotInlineSidecarDisposition::Unsupported {
                reason: HOT_INLINE_UNSUPPORTED_LEGACY_BLOCK_TARGET,
                ..
            }
        ));
        assert_eq!(
            unsupported_ack.disposition,
            InlineSidecarAckDisposition::Unsupported
        );
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
        return;
    }
    endpoint
        .request_hot_inline(&mut runtime, command(1))
        .expect("first bullet-list demand");
    assert!(matches!(
        endpoint
            .poll(&mut runtime, 1)
            .expect("bounded bullet-list projection"),
        CandidatePoll::Pending { transitions } if transitions <= 1
    ));
    assert!(endpoint.hot_inline_sidecar.is_none());
    endpoint.cancel_hot_inline();
    for _ in 0..100_000 {
        if !endpoint.hot_inline_has_poll_work() {
            break;
        }
        assert!(matches!(
            endpoint
                .poll(&mut runtime, 1)
                .expect("fuelled bullet-list cancellation"),
            CandidatePoll::Pending { transitions } if transitions <= 1
        ));
    }
    assert!(!endpoint.hot_inline_has_poll_work());

    endpoint
        .request_hot_inline(&mut runtime, command(2))
        .expect("replacement bullet-list demand");
    let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        80_000,
    );
    assert_eq!(begin.binding.refinement_generation, 2);
    assert_eq!(begin.binding.physical_start_utf8, 0);
    assert_eq!(begin.binding.physical_end_utf8, SOURCE.len() as u32);
    assert!(matches!(
        begin.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative {
            fact_count: 3,
            logical_page_count: 1,
            ..
        }
    ));
    assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);

    let mut encoded_items = [0_u8; 3 * ITEM_RECORD_BYTES];
    let query = host
        .query_inline_sidecar(begin.binding, &mut encoded_items)
        .expect("query typed bullet-list sidecar");
    assert!(matches!(
        query,
        HostInlineSidecarQueryOutcome::Authoritative {
            fact_count: 3,
            encoded_bytes: 84,
            ..
        }
    ));
    let observed = encoded_items
        .chunks_exact(ITEM_RECORD_BYTES)
        .map(|record| {
            std::array::from_fn::<u32, 7, _>(|field| {
                let start = field * 4;
                u32::from_le_bytes(record[start..start + 4].try_into().unwrap())
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        observed,
        vec![
            [0, 16, 8, 3, 8, 6, 3],
            [16, 7, 4, 0, 4, 2, 1],
            [23, 4, 4, 0, 2, 0, 0],
        ]
    );

    let mut viewport = [0xa5_u8; VIEWPORT_BYTES];
    let outcome = host
        .query_structural(
            HostPointQuery {
                source_version: delivery.ack.source_version,
                position: HostSourceMetric {
                    bytes: 20,
                    utf16: 15,
                },
                affinity: HostMetricAffinity::Downstream,
                budget: HostQueryBudget {
                    maximum_encoded_bytes: VIEWPORT_BYTES as u32,
                    maximum_open_depth: 3,
                    maximum_leaf_count: 8,
                    maximum_tree_nodes_visited: 256,
                },
            },
            &mut viewport,
        )
        .expect("joined bullet-list viewport");
    let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
        panic!("list and sidecar must author one schema-5 viewport: {outcome:?}");
    };
    assert_eq!(receipt.encoded_bytes, VIEWPORT_BYTES as u32);
    assert_eq!(u32::from_le_bytes(viewport[8..12].try_into().unwrap()), 5);
    assert_eq!(u16::from_le_bytes(viewport[20..22].try_into().unwrap()), 3);
    assert_eq!(viewport[22], 4);
    assert_eq!(
        u32::from_le_bytes(viewport[24..28].try_into().unwrap()),
        POINT_PATH_BYTES as u32
    );
    assert_eq!(
        u32::from_le_bytes(viewport[28..32].try_into().unwrap()),
        (3 * ITEM_RECORD_BYTES) as u32
    );
    let path_start = VIEWPORT_HEADER_BYTES + GREEN_RECORD_BYTES + PROJECTION_RECORD_BYTES;
    let list = &viewport[path_start..path_start + POINT_PATH_NODE_BYTES];
    let item =
        &viewport[path_start + POINT_PATH_NODE_BYTES..path_start + 2 * POINT_PATH_NODE_BYTES];
    let paragraph =
        &viewport[path_start + 2 * POINT_PATH_NODE_BYTES..path_start + POINT_PATH_BYTES];
    assert_eq!((list[0], list[1]), (3, 1));
    assert_eq!((item[0], item[1]), (4, 1));
    assert_eq!((paragraph[0], paragraph[1]), (2, 2));
    assert_eq!(u32::from_le_bytes(item[16..20].try_into().unwrap()), 1);
    assert_eq!(&viewport[path_start + POINT_PATH_BYTES..], &encoded_items);

    let mut terminal_viewport = [0xa5_u8; TERMINAL_VIEWPORT_BYTES];
    let terminal_outcome = host
        .query_structural(
            HostPointQuery {
                source_version: delivery.ack.source_version,
                position: HostSourceMetric {
                    bytes: 23,
                    utf16: 17,
                },
                affinity: HostMetricAffinity::Downstream,
                budget: HostQueryBudget {
                    maximum_encoded_bytes: TERMINAL_VIEWPORT_BYTES as u32,
                    maximum_open_depth: 3,
                    maximum_leaf_count: 8,
                    maximum_tree_nodes_visited: 256,
                },
            },
            &mut terminal_viewport,
        )
        .expect("joined terminal-empty bullet-list viewport");
    let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = terminal_outcome else {
        panic!("terminal list item must author a two-node schema-5 viewport: {terminal_outcome:?}");
    };
    assert_eq!(range.start, HostSourceMetric { bytes: 0, utf16: 0 });
    assert_eq!(
        range.end,
        HostSourceMetric {
            bytes: SOURCE.len() as u32,
            utf16: 21,
        }
    );
    assert_eq!(receipt.encoded_bytes, TERMINAL_VIEWPORT_BYTES as u32);
    assert!(receipt.leaf_count <= 8);
    assert!(receipt.open_depth <= 3);
    assert!(receipt.tree_nodes_visited <= 256);
    assert_eq!(
        u32::from_le_bytes(terminal_viewport[8..12].try_into().unwrap()),
        5
    );
    assert_eq!(
        u16::from_le_bytes(terminal_viewport[20..22].try_into().unwrap()),
        2
    );
    assert_eq!(terminal_viewport[22], 4);
    assert_eq!(
        u32::from_le_bytes(terminal_viewport[24..28].try_into().unwrap()),
        TERMINAL_POINT_PATH_BYTES as u32
    );
    assert_eq!(
        u32::from_le_bytes(terminal_viewport[28..32].try_into().unwrap()),
        (3 * ITEM_RECORD_BYTES) as u32
    );
    let terminal_list = &terminal_viewport[path_start..path_start + POINT_PATH_NODE_BYTES];
    let terminal_item = &terminal_viewport
        [path_start + POINT_PATH_NODE_BYTES..path_start + TERMINAL_POINT_PATH_BYTES];
    assert_eq!((terminal_list[0], terminal_list[1]), (3, 1));
    assert_eq!(
        u16::from_le_bytes(terminal_list[2..4].try_into().unwrap()),
        0
    );
    assert_eq!(
        u32::from_le_bytes(terminal_list[4..8].try_into().unwrap()),
        u32::MAX
    );
    assert_eq!(
        u32::from_le_bytes(terminal_list[8..12].try_into().unwrap()),
        0
    );
    assert_eq!(
        u32::from_le_bytes(terminal_list[12..16].try_into().unwrap()),
        SOURCE.len() as u32
    );
    assert_eq!(
        u32::from_le_bytes(terminal_list[16..20].try_into().unwrap()),
        0
    );
    assert_eq!(
        u32::from_le_bytes(terminal_list[20..24].try_into().unwrap()),
        3
    );
    assert_eq!(
        u32::from_le_bytes(terminal_list[24..28].try_into().unwrap()),
        11
    );
    assert_eq!(
        u32::from_le_bytes(terminal_list[28..32].try_into().unwrap()),
        7
    );
    assert_eq!((terminal_item[0], terminal_item[1]), (4, 3));
    assert_eq!(
        u16::from_le_bytes(terminal_item[2..4].try_into().unwrap()),
        1
    );
    assert_eq!(
        u32::from_le_bytes(terminal_item[4..8].try_into().unwrap()),
        0
    );
    assert_eq!(
        u32::from_le_bytes(terminal_item[8..12].try_into().unwrap()),
        23
    );
    assert_eq!(
        u32::from_le_bytes(terminal_item[12..16].try_into().unwrap()),
        27
    );
    assert_eq!(
        u32::from_le_bytes(terminal_item[16..20].try_into().unwrap()),
        2
    );
    assert_eq!(
        u32::from_le_bytes(terminal_item[20..24].try_into().unwrap()),
        1
    );
    assert_eq!(
        u32::from_le_bytes(terminal_item[24..28].try_into().unwrap()),
        0
    );
    assert_eq!(
        u32::from_le_bytes(terminal_item[28..32].try_into().unwrap()),
        0
    );
    assert_eq!(
        &terminal_viewport[path_start + TERMINAL_POINT_PATH_BYTES..],
        &encoded_items
    );

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn bullet_list_nonzero_leaf_root_joins_absolute_selected_item_path() {
    const SOURCE: &str = "before😀\n\n- α😀\n- beta";
    const LIST_START_BYTES: u32 = 12;
    const LIST_START_UTF16: u32 = 10;
    const ITEM_RECORD_BYTES: usize = 28;
    const VIEWPORT_HEADER_BYTES: usize = 32;
    const GREEN_RECORD_BYTES: usize = 80;
    const PROJECTION_RECORD_BYTES: usize = 56;
    const POINT_PATH_NODE_BYTES: usize = 32;
    const POINT_PATH_BYTES: usize = 3 * POINT_PATH_NODE_BYTES;
    const VIEWPORT_BYTES: usize = VIEWPORT_HEADER_BYTES
        + GREEN_RECORD_BYTES
        + PROJECTION_RECORD_BYTES
        + POINT_PATH_BYTES
        + 2 * ITEM_RECORD_BYTES;
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [736, 737, 738, 739],
        source_session_identity: 740,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start segmented candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent host");
    host.observe_source_version(source_version_for(binding, completion))
        .expect("host observes source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    if endpoint
        .recursive_green
        .has_installed_session_for(delivery.ack)
    {
        let (owner_kind, range, ancestry) =
            recursive_green_query_shape(&host, delivery.ack.source_version, 23, 18);
        assert_eq!(owner_kind, 5);
        assert_eq!(ancestry, vec![1, 3, 4, 5]);
        assert_eq!(range, [23, SOURCE.len() as u32, 18, 22]);
        let owner_frame = recursive_green_owner_frame(&host, delivery.ack.source_version, 23, 18);
        let before_frame = recursive_green_owner_frame(&host, delivery.ack.source_version, 0, 0);
        assert_ne!(owner_frame, before_frame);
        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 1,
                    source_version: delivery.ack.source_version,
                    base_ack: delivery.ack,
                    byte_offset: 23,
                    utf16_offset: 18,
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::Automatic,
                },
            )
            .expect("nonzero-root Green Paragraph demand");
        let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            80_000,
        );
        assert_eq!(
            begin.binding.owner(),
            Some(HotInlineSidecarOwner::RecursiveGreenFrame(owner_frame))
        );
        assert_eq!(begin.binding.physical_start_utf8, range[0]);
        assert_eq!(begin.binding.physical_end_utf8, range[1]);
        assert!(matches!(
            begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative { fact_count: 0, .. }
        ));
        assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
        return;
    }

    endpoint
        .request_hot_inline(
            &mut runtime,
            InlineRefinementCommand {
                binding,
                refinement_generation: 1,
                source_version: delivery.ack.source_version,
                base_ack: delivery.ack,
                byte_offset: 23,
                utf16_offset: 18,
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::Automatic,
            },
        )
        .expect("nonzero-root bullet-list demand");
    let (begin, ack) = deliver_hot_inline_sidecar_to_independent_host_with_unit_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        80_000,
    );
    assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);
    assert_eq!(begin.binding.physical_start_utf8, LIST_START_BYTES);
    assert_eq!(begin.binding.physical_start_utf16, LIST_START_UTF16);
    assert_eq!(begin.binding.physical_end_utf8, SOURCE.len() as u32);
    assert_eq!(
        begin.binding.physical_end_utf16,
        SOURCE.encode_utf16().count() as u32
    );
    assert!(matches!(
        begin.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative {
            fact_count: 2,
            logical_page_count: 1,
            ..
        }
    ));

    let mut encoded_items = [0_u8; 2 * ITEM_RECORD_BYTES];
    let query = host
        .query_inline_sidecar(begin.binding, &mut encoded_items)
        .expect("query typed nonzero-root bullet-list sidecar");
    assert!(matches!(
        query,
        HostInlineSidecarQueryOutcome::Authoritative {
            fact_count: 2,
            encoded_bytes: 56,
            ..
        }
    ));
    let observed = encoded_items
        .chunks_exact(ITEM_RECORD_BYTES)
        .map(|record| {
            std::array::from_fn::<u32, 7, _>(|field| {
                let start = field * 4;
                u32::from_le_bytes(record[start..start + 4].try_into().unwrap())
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(observed, vec![[0, 9, 2, 0, 2, 6, 3], [9, 6, 2, 0, 2, 4, 4]]);

    let mut viewport = [0xa5_u8; VIEWPORT_BYTES];
    let outcome = host
        .query_structural(
            HostPointQuery {
                source_version: delivery.ack.source_version,
                position: HostSourceMetric {
                    bytes: 23,
                    utf16: 18,
                },
                affinity: HostMetricAffinity::Downstream,
                budget: HostQueryBudget {
                    maximum_encoded_bytes: VIEWPORT_BYTES as u32,
                    maximum_open_depth: 3,
                    maximum_leaf_count: 8,
                    maximum_tree_nodes_visited: 256,
                },
            },
            &mut viewport,
        )
        .expect("joined nonzero-root bullet-list viewport");
    let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
        panic!("nonzero-root list and sidecar must author schema 5: {outcome:?}");
    };
    assert_eq!(
        range.start,
        HostSourceMetric {
            bytes: LIST_START_BYTES,
            utf16: LIST_START_UTF16,
        }
    );
    assert_eq!(
        range.end,
        HostSourceMetric {
            bytes: SOURCE.len() as u32,
            utf16: SOURCE.encode_utf16().count() as u32,
        }
    );
    assert_eq!(receipt.encoded_bytes, VIEWPORT_BYTES as u32);
    assert_eq!(u32::from_le_bytes(viewport[8..12].try_into().unwrap()), 5);
    assert_eq!(u16::from_le_bytes(viewport[20..22].try_into().unwrap()), 3);
    assert_eq!(viewport[22], 4);

    let path_start = VIEWPORT_HEADER_BYTES + GREEN_RECORD_BYTES + PROJECTION_RECORD_BYTES;
    let list = &viewport[path_start..path_start + POINT_PATH_NODE_BYTES];
    let item =
        &viewport[path_start + POINT_PATH_NODE_BYTES..path_start + 2 * POINT_PATH_NODE_BYTES];
    let paragraph =
        &viewport[path_start + 2 * POINT_PATH_NODE_BYTES..path_start + POINT_PATH_BYTES];
    assert_eq!((list[0], list[1]), (3, 1));
    assert_eq!(
        (
            u32::from_le_bytes(list[8..12].try_into().unwrap()),
            u32::from_le_bytes(list[12..16].try_into().unwrap())
        ),
        (12, 27)
    );
    assert_eq!((item[0], item[1]), (4, 1));
    assert_eq!(
        (
            u32::from_le_bytes(item[8..12].try_into().unwrap()),
            u32::from_le_bytes(item[12..16].try_into().unwrap())
        ),
        (21, 27)
    );
    assert_eq!(u32::from_le_bytes(item[16..20].try_into().unwrap()), 1);
    assert_eq!((paragraph[0], paragraph[1]), (2, 2));
    assert_eq!(
        (
            u32::from_le_bytes(paragraph[8..12].try_into().unwrap()),
            u32::from_le_bytes(paragraph[12..16].try_into().unwrap())
        ),
        (23, 27)
    );
    assert_eq!(&viewport[path_start + POINT_PATH_BYTES..], &encoded_items);

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn close_after_sidecar_commit_before_delivery_receipt_reclaims_typed_roots() {
    let cases = [
        (
            "\u{feff}   > α😀\r\n> β\rlazy😀\0",
            [721, 722, 723, 724],
            725,
            "block quote",
        ),
        (
            "\u{feff}\tα\0\r\n\n      \r    \tβ\r\tlast",
            [726, 727, 728, 729],
            730,
            "indented code",
        ),
    ];

    for (source, document_session, source_session_identity, label) in cases {
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session,
            source_session_identity,
            worker_generation: 1,
        };
        let mut runtime =
            DocumentRuntime::new(source, standard_document_runtime_config()).expect("runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start segmented candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("independent host");
        host.observe_source_version(source_version_for(binding, completion))
            .expect("host observes source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        drain_candidate_cleanup(&mut endpoint, &mut runtime);

        endpoint
            .request_hot_inline(
                &mut runtime,
                InlineRefinementCommand {
                    binding,
                    refinement_generation: 1,
                    source_version: delivery.ack.source_version,
                    base_ack: delivery.ack,
                    byte_offset: 0,
                    utf16_offset: 0,
                    affinity: InlinePointAffinity::After,
                    target: InlineRefinementTarget::Automatic,
                },
            )
            .unwrap_or_else(|error| panic!("{label} demand failed: {error}"));
        let pending = commit_hot_inline_sidecar_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
            90_000,
        );
        assert!(
            endpoint.hot_inline_sidecar.is_some(),
            "{label} delivery receipt must still own the producer sidecar"
        );
        host.acknowledge_inline_sidecar_delivery(pending.ack)
            .unwrap_or_else(|error| panic!("{label} host delivery failed: {error}"));

        // Match Endpoint::begin_close: cancel the producer first, then
        // latch the document runtime close before any delivery receipt is
        // returned to CandidateEndpoint.
        endpoint
            .begin_close()
            .unwrap_or_else(|error| panic!("{label} close failed: {error}"));
        runtime.cancel_source_facts();
        runtime
            .begin_close()
            .unwrap_or_else(|error| panic!("{label} runtime close failed: {error}"));

        for _ in 0..1_000_000 {
            if !endpoint.cleanup_pending() {
                break;
            }
            endpoint
                .poll_cleanup(&mut runtime, 1)
                .unwrap_or_else(|error| panic!("{label} cleanup failed: {error}"));
        }
        assert!(
            !endpoint.cleanup_pending(),
            "{label} root remained live after close"
        );
        assert!(endpoint.hot_inline_sidecar.is_none());

        for _ in 0..1_000_000 {
            if runtime
                .poll_close(1)
                .unwrap_or_else(|error| panic!("{label} runtime drain failed: {error}"))
                .complete
            {
                break;
            }
        }
        assert_eq!(
            runtime.arena_metrics().resident_nodes,
            0,
            "{label} runtime did not reclaim to zero"
        );
        host.begin_close()
            .unwrap_or_else(|error| panic!("{label} host close failed: {error}"));
        for _ in 0..1_000_000 {
            match host
                .poll(HostWorkGrant {
                    inspect_bytes: 0,
                    copy_bytes: 0,
                    transitions: 1,
                })
                .unwrap_or_else(|error| panic!("{label} host drain failed: {error}"))
            {
                NativeHostPollOutcome::Pending => {}
                NativeHostPollOutcome::Closed => break,
                outcome => panic!("{label} unexpected host close outcome: {outcome:?}"),
            }
        }
        assert!(host.is_removable(), "{label} host did not close to zero");
    }
}

#[test]
fn atx_heading_reaches_independent_host_and_refines_only_its_content() {
    const SOURCE: &str = "p\n\n  ### **β😀** ###  \r\n\n# before <tag>\n";
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [711, 712, 713, 714],
        source_session_identity: 715,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start segmented candidate");
    let source_version = source_version_for(binding, completion);
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent host");
    host.observe_source_version(source_version)
        .expect("host observes source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let heading_start = SOURCE.find("  ###").expect("ATX Heading");
    let heading_end = heading_start + "  ### **β😀** ###  \r\n".len();
    let inline_start = SOURCE.find("**β😀**").expect("heading content");
    let inline_end = inline_start + "**β😀**".len();
    let inline_point = inline_start + 2;
    let inline_point_utf16 = SOURCE[..inline_point].encode_utf16().count();
    let (owner_kind, range, ancestry) =
        recursive_green_query_shape(&host, source_version, inline_point, inline_point_utf16);
    assert_eq!(owner_kind, 12, "the active row is a Green Heading");
    assert_eq!(
        range,
        [
            inline_start as u32,
            inline_end as u32,
            SOURCE[..inline_start].encode_utf16().count() as u32,
            SOURCE[..inline_end].encode_utf16().count() as u32,
        ]
    );
    assert_eq!(ancestry.first(), Some(&1));
    assert_eq!(ancestry.last(), Some(&12));

    let command = |generation: u32, byte_offset: usize| InlineRefinementCommand {
        binding,
        refinement_generation: generation,
        source_version: delivery.ack.source_version,
        base_ack: delivery.ack,
        byte_offset: u32::try_from(byte_offset).expect("bounded point"),
        utf16_offset: u32::try_from(SOURCE[..byte_offset].encode_utf16().count())
            .expect("bounded UTF-16 point"),
        affinity: InlinePointAffinity::After,
        target: InlineRefinementTarget::Automatic,
    };
    endpoint
        .request_hot_inline(&mut runtime, command(1, inline_start + 2))
        .expect("ATX inline demand");
    let (authoritative_begin, authoritative_ack) =
        deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 40_000);
    assert_eq!(authoritative_begin.base_ack, delivery.ack);
    assert_eq!(
        authoritative_begin.binding.physical_start_utf8 as usize,
        heading_start
    );
    assert_eq!(
        authoritative_begin.binding.physical_end_utf8 as usize,
        heading_end
    );
    assert_eq!(
        authoritative_begin.binding.visible_start_utf8 as usize,
        inline_start
    );
    assert_eq!(
        authoritative_begin.binding.visible_end_utf8 as usize,
        inline_end
    );
    assert!(matches!(
        authoritative_begin.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative { fact_count, .. } if fact_count > 0
    ));
    assert_eq!(
        authoritative_ack.disposition,
        InlineSidecarAckDisposition::Authoritative
    );

    let hazard_start = SOURCE.find("before").expect("hazard heading content");
    let hazard_end = hazard_start + "before <tag>".len();
    endpoint
        .request_hot_inline(&mut runtime, command(2, hazard_start))
        .expect("hazard ATX inline demand");
    let (unsupported_begin, unsupported_ack) =
        deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 50_000);
    assert_eq!(
        unsupported_begin.binding.visible_start_utf8 as usize,
        hazard_start
    );
    assert_eq!(
        unsupported_begin.binding.visible_end_utf8 as usize,
        hazard_end
    );
    assert!(matches!(
        unsupported_begin.envelope.disposition,
        HotInlineSidecarDisposition::Unsupported {
            reason: HOT_INLINE_UNSUPPORTED_PARSER,
            ..
        }
    ));
    assert_eq!(
        unsupported_ack.disposition,
        InlineSidecarAckDisposition::Unsupported
    );

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn setext_h1_h2_reach_independent_host_with_content_only_inline_fences() {
    const SOURCE: &str = "**H1 β😀**\r\n  ===  \r\n\n_H2_\n---\n";
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [726, 727, 728, 729],
        source_session_identity: 730,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start segmented Setext candidate");
    let source_version = source_version_for(binding, completion);
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent Setext host");
    host.observe_source_version(source_version)
        .expect("host observes Setext source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let h1_start = 0_usize;
    let h1_inline_end = SOURCE.find("\r\n  ===").expect("H1 content ending");
    let h1_marker_start = SOURCE.find("===").expect("H1 underline");
    let h1_marker_end = h1_marker_start + 3;
    let h1_line_ending_start = h1_marker_end + 2;
    let h1_end = h1_line_ending_start + 2;
    let h2_start = SOURCE.find("_H2_").expect("H2 content");
    let h2_inline_end = h2_start + "_H2_".len();
    let h2_marker_start = SOURCE[h2_start..]
        .find("---")
        .map(|offset| h2_start + offset)
        .expect("H2 underline");
    let h2_marker_end = h2_marker_start + 3;
    let h2_line_ending_start = h2_marker_end;
    let h2_end = h2_line_ending_start + 1;
    let headings = [
        (h1_start, h1_end, h1_start, h1_inline_end),
        (h2_start, h2_end, h2_start, h2_inline_end),
    ];
    for (_source_start, _source_end, inline_start, inline_end) in headings {
        let point = inline_start + 1;
        let point_utf16 = SOURCE[..point].encode_utf16().count();
        let (owner_kind, range, ancestry) =
            recursive_green_query_shape(&host, source_version, point, point_utf16);
        assert_eq!(owner_kind, 12, "the active row is a Green Heading");
        assert_eq!(range[0] as usize, inline_start);
        assert_eq!(range[1] as usize, inline_end);
        assert_eq!(
            range[2] as usize,
            SOURCE[..inline_start].encode_utf16().count()
        );
        assert_eq!(
            range[3] as usize,
            SOURCE[..inline_end].encode_utf16().count()
        );
        assert_eq!(ancestry.first(), Some(&1));
        assert_eq!(ancestry.last(), Some(&12));
    }

    let command = |generation: u32, byte_offset: usize| InlineRefinementCommand {
        binding,
        refinement_generation: generation,
        source_version: delivery.ack.source_version,
        base_ack: delivery.ack,
        byte_offset: u32::try_from(byte_offset).expect("bounded Setext point"),
        utf16_offset: u32::try_from(SOURCE[..byte_offset].encode_utf16().count())
            .expect("bounded Setext UTF-16 point"),
        affinity: InlinePointAffinity::After,
        target: InlineRefinementTarget::Automatic,
    };
    for (generation, physical_start, physical_end, visible_start, visible_end) in [
        (1, h1_start, h1_end, h1_start, h1_inline_end),
        (2, h2_start, h2_end, h2_start, h2_inline_end),
    ] {
        endpoint
            .request_hot_inline(&mut runtime, command(generation, visible_start + 1))
            .expect("Setext inline demand");
        let (begin, ack) =
            deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 40_000);
        assert_eq!(begin.base_ack, delivery.ack);
        assert_eq!(begin.binding.physical_start_utf8 as usize, physical_start);
        assert_eq!(begin.binding.physical_end_utf8 as usize, physical_end);
        assert_eq!(begin.binding.visible_start_utf8 as usize, visible_start);
        assert_eq!(begin.binding.visible_end_utf8 as usize, visible_end);
        assert_eq!(
            begin.binding.physical_start_utf16 as usize,
            SOURCE[..physical_start].encode_utf16().count()
        );
        assert_eq!(
            begin.binding.physical_end_utf16 as usize,
            SOURCE[..physical_end].encode_utf16().count()
        );
        assert_eq!(
            begin.binding.visible_start_utf16 as usize,
            SOURCE[..visible_start].encode_utf16().count()
        );
        assert_eq!(
            begin.binding.visible_end_utf16 as usize,
            SOURCE[..visible_end].encode_utf16().count()
        );
        assert!(matches!(
            begin.envelope.disposition,
            HotInlineSidecarDisposition::Authoritative { fact_count, .. } if fact_count > 0
        ));
        assert_eq!(ack.disposition, InlineSidecarAckDisposition::Authoritative);
    }

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn thematic_break_reaches_independent_host_with_empty_projection_and_not_inline_sidecar() {
    const SOURCE: &str = "p\n\n  - - -  \r\n\nq";
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [731, 732, 733, 734],
        source_session_identity: 735,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new(SOURCE, standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start segmented thematic-break candidate");
    let source_version = source_version_for(binding, completion);
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent thematic-break host");
    host.observe_source_version(source_version)
        .expect("host observes thematic-break source");
    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let thematic_start = SOURCE.find("  - - -").expect("thematic break");
    let marker_start = SOURCE[thematic_start..]
        .find('-')
        .map(|offset| thematic_start + offset)
        .expect("first thematic marker");
    let marker_end = thematic_start + "  - - -".len();
    let line_ending_start = SOURCE[marker_end..]
        .find("\r\n")
        .map(|offset| marker_end + offset)
        .expect("thematic line ending");
    let marker_utf16 = SOURCE[..marker_start].encode_utf16().count();
    let (owner_kind, range, ancestry) =
        recursive_green_query_shape(&host, source_version, marker_start, marker_utf16);
    assert_eq!(owner_kind, 13, "the active row is a Green thematic break");
    assert_eq!(range[0] as usize, thematic_start);
    assert_eq!(range[1] as usize, line_ending_start);
    assert_eq!(
        range[2] as usize,
        SOURCE[..thematic_start].encode_utf16().count()
    );
    assert_eq!(
        range[3] as usize,
        SOURCE[..line_ending_start].encode_utf16().count()
    );
    assert_eq!(ancestry.first(), Some(&1));
    assert_eq!(ancestry.last(), Some(&13));

    endpoint
        .request_hot_inline(
            &mut runtime,
            InlineRefinementCommand {
                binding,
                refinement_generation: 1,
                source_version: delivery.ack.source_version,
                base_ack: delivery.ack,
                byte_offset: u32::try_from(marker_start).expect("bounded thematic point"),
                utf16_offset: u32::try_from(SOURCE[..marker_start].encode_utf16().count())
                    .expect("bounded thematic UTF-16 point"),
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::Automatic,
            },
        )
        .expect("thematic-break inline demand");
    let (begin, ack) =
        deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 60_000);
    assert_eq!(begin.base_ack, delivery.ack);
    assert_eq!(begin.binding.physical_start_utf8 as usize, thematic_start);
    assert_eq!(begin.binding.physical_end_utf8 as usize, line_ending_start);
    assert!(matches!(
        begin.envelope.disposition,
        HotInlineSidecarDisposition::Unsupported {
            reason: HOT_INLINE_UNSUPPORTED_NOT_INLINE_LEAF,
            ..
        }
    ));
    assert_eq!(ack.disposition, InlineSidecarAckDisposition::Unsupported);

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn thematic_break_large_interior_paragraph_transition_stays_exact() {
    const PARAGRAPHS: usize = 4_096;
    const EDITED_PARAGRAPH: usize = PARAGRAPHS / 2;
    const MAXIMUM_TRANSFERRED_RECORDS: u32 = 64;
    const THEMATIC_SOURCE: &str = "  - - -  \r\n";

    let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [736, 737, 738, 739],
        source_session_identity: 740,
        worker_generation: 1,
    };
    let base_source: String = (0..PARAGRAPHS)
        .map(|ordinal| format!("paragraph {ordinal:04} {}\n\n", "a".repeat(32)))
        .collect();
    let mut current_source = base_source;
    let mut runtime = DocumentRuntime::new(&current_source, standard_document_runtime_config())
        .expect("large thematic-break runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut current_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start large thematic-break base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("large thematic-break host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes large thematic-break base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
    let mut current_ack = base_delivery.ack;
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    for phase in 0..2 {
        let middle_marker = format!("paragraph {EDITED_PARAGRAPH:04} ");
        let (edit_range, replacement, expected_middle_kind) = if phase == 0 {
            let middle_start = current_source
                .find(&middle_marker)
                .expect("middle Paragraph");
            let middle_end = current_source[middle_start..]
                .find('\n')
                .map(|offset| middle_start + offset + 1)
                .expect("middle Paragraph line ending");
            (middle_start..middle_end, THEMATIC_SOURCE.to_owned(), 13_u16)
        } else {
            let middle_start = current_source
                .find(THEMATIC_SOURCE)
                .expect("middle thematic break");
            (
                middle_start..middle_start + THEMATIC_SOURCE.len(),
                format!(
                    "paragraph {EDITED_PARAGRAPH:04} replacement {}\n",
                    "z".repeat(24)
                ),
                5_u16,
            )
        };
        let mut target_source = current_source.clone();
        target_source.replace_range(edit_range.clone(), &replacement);
        let target_version = runtime
            .apply_edit(current_version, edit_range, &replacement)
            .expect("apply thematic-break transition")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan thematic-break transition");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight thematic-break transition"),
            "phase {phase} must retain authenticated crop authority"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let ui_revision = u32::try_from(phase + 2).expect("UI revision");
        let base_ui_revision = u32::try_from(phase + 1).expect("base UI revision");
        let completion = completion_for_persistent_target(&runtime, ui_revision, base_ui_revision);
        let source_version = source_version_for(binding, completion);
        host.observe_source_version(source_version)
            .expect("host observes thematic-break transition");
        endpoint
            .start_incremental(
                &runtime,
                runtime
                    .snapshot_current_source()
                    .expect("borrow thematic-break target"),
                witness,
                binding,
                completion,
            )
            .expect("start thematic-break crop");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "AwaitingRecursiveGreenExact",
            "phase {phase} must await recursive-Green adoption before selecting its exact route"
        );
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(delivery.offer.base_ack, Some(current_ack));
        assert!(
            delivery.offer.transferred_record_count <= MAXIMUM_TRANSFERRED_RECORDS,
            "phase {phase} transferred {} of {} records",
            delivery.offer.transferred_record_count,
            delivery.offer.target_record_count
        );
        let recursive_green_replacement_records = delivery
            .packet_frames
            .iter()
            .flatten()
            .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage)
            .map(|(_, records)| *records)
            .sum::<u32>();
        assert!(
            recursive_green_replacement_records > 0
                && recursive_green_replacement_records <= MAXIMUM_TRANSFERRED_RECORDS,
            "phase {phase} must publish one bounded recursive-Green splice"
        );
        assert!(delivery
            .packet_frames
            .iter()
            .flatten()
            .all(|(kind, _)| *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage));

        for ordinal in [0, EDITED_PARAGRAPH, PARAGRAPHS - 1] {
            let paragraph_marker = format!("paragraph {ordinal:04} ");
            let block_start = if ordinal == EDITED_PARAGRAPH && expected_middle_kind == 13 {
                target_source
                    .find(THEMATIC_SOURCE)
                    .expect("target thematic break")
            } else {
                target_source
                    .find(&paragraph_marker)
                    .expect("target Paragraph")
            };
            let block_end = target_source[block_start..]
                .find('\n')
                .map(|offset| block_start + offset + 1)
                .expect("target block line ending");
            let point = if ordinal == EDITED_PARAGRAPH && expected_middle_kind == 13 {
                block_start + 2
            } else {
                block_start + paragraph_marker.len()
            };
            let expected_owner_kind = if ordinal == EDITED_PARAGRAPH {
                expected_middle_kind
            } else {
                5
            };
            let semantic_end = if expected_owner_kind == 13 {
                block_end - 2
            } else {
                block_end - 1
            };
            let point_utf16 = target_source[..point].encode_utf16().count();
            let (owner_kind, range, ancestry) =
                recursive_green_query_shape(&host, source_version, point, point_utf16);
            assert_eq!(owner_kind, expected_owner_kind);
            assert_eq!(range[0] as usize, block_start);
            assert_eq!(range[1] as usize, semantic_end);
            assert_eq!(
                range[2] as usize,
                target_source[..block_start].encode_utf16().count()
            );
            assert_eq!(
                range[3] as usize,
                target_source[..semantic_end].encode_utf16().count()
            );
            assert_eq!(ancestry.first(), Some(&1));
            assert_eq!(ancestry.last(), Some(&expected_owner_kind));
        }

        let retained = endpoint
            .retained
            .as_ref()
            .expect("retained thematic-break target");
        assert!(matches!(
            retained.restart.as_ref(),
            Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
                if *source == target_version
        ));
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(endpoint
            .has_exact_base_for(&runtime, target_version)
            .expect("next thematic-break revision authority"));

        current_source = target_source;
        current_version = target_version;
        current_ack = delivery.ack;
    }

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn setext_large_interior_paragraph_h1_h2_paragraph_sequence_stays_exact() {
    const PARAGRAPHS: usize = 4_096;
    const EDITED_PARAGRAPH: usize = PARAGRAPHS / 2;
    const MAXIMUM_TRANSFERRED_RECORDS: u32 = 64;

    let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [731, 732, 733, 734],
        source_session_identity: 735,
        worker_generation: 1,
    };
    let base_source: String = (0..PARAGRAPHS)
        .map(|ordinal| format!("paragraph {ordinal:04} {}\n\n", "a".repeat(32)))
        .collect();
    let mut current_source = base_source;
    let mut runtime = DocumentRuntime::new(&current_source, standard_document_runtime_config())
        .expect("large Setext runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let mut current_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start large Setext base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("large Setext host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes large Setext base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
    let mut current_ack = base_delivery.ack;
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    for phase in 0..3 {
        let middle_marker = format!("paragraph {EDITED_PARAGRAPH:04} ");
        let middle_start = current_source
            .find(&middle_marker)
            .expect("middle Paragraph");
        let content_line_end = current_source[middle_start..]
            .find('\n')
            .map(|offset| middle_start + offset + 1)
            .expect("middle content line ending");
        let (edit_range, replacement, expected_middle_kind) = match phase {
            0 => (content_line_end..content_line_end + 1, "===\n\n", 12_u16),
            1 => {
                let marker_start = current_source[middle_start..]
                    .find("===\n\n")
                    .map(|offset| middle_start + offset)
                    .expect("H1 underline");
                (marker_start..marker_start + 3, "---", 12_u16)
            }
            2 => {
                let marker_start = current_source[middle_start..]
                    .find("---\n\n")
                    .map(|offset| middle_start + offset)
                    .expect("H2 underline");
                (marker_start..marker_start + 5, "\n", 5_u16)
            }
            _ => unreachable!(),
        };
        let mut target_source = current_source.clone();
        target_source.replace_range(edit_range.clone(), replacement);
        let target_version = runtime
            .apply_edit(current_version, edit_range, replacement)
            .expect("apply Setext phase")
            .source()
            .current();
        let plan = runtime
            .begin_incremental_source_facts(
                profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("plan Setext phase");
        assert!(
            endpoint
                .has_incremental_base_for_plan(&runtime, &plan)
                .expect("preflight Setext phase"),
            "Setext phase {phase} must retain authenticated crop authority"
        );
        let witness = complete_incremental_source_facts(&mut runtime);
        let ui_revision = u32::try_from(phase + 2).expect("UI revision");
        let base_ui_revision = u32::try_from(phase + 1).expect("base UI revision");
        let completion = completion_for_persistent_target(&runtime, ui_revision, base_ui_revision);
        let source_version = source_version_for(binding, completion);
        host.observe_source_version(source_version)
            .expect("host observes Setext phase");
        endpoint
            .start_incremental(
                &runtime,
                runtime
                    .snapshot_current_source()
                    .expect("borrow Setext target"),
                witness,
                binding,
                completion,
            )
            .expect("start Setext crop");
        assert_eq!(
            active_candidate_phase(endpoint.active.as_ref()),
            "AwaitingRecursiveGreenExact",
            "Setext phase {phase} must await recursive-Green adoption before selecting its exact route"
        );
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(delivery.offer.mode, PublicationMode::ExactBaseDelta);
        assert_eq!(delivery.offer.base_ack, Some(current_ack));
        assert!(
            delivery.offer.transferred_record_count <= MAXIMUM_TRANSFERRED_RECORDS,
            "Setext phase {phase} transferred {} of {} records",
            delivery.offer.transferred_record_count,
            delivery.offer.target_record_count
        );
        let recursive_green_replacement_records = delivery
            .packet_frames
            .iter()
            .flatten()
            .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage)
            .map(|(_, records)| *records)
            .sum::<u32>();
        assert!(
            recursive_green_replacement_records > 0
                && recursive_green_replacement_records <= MAXIMUM_TRANSFERRED_RECORDS,
            "Setext phase {phase} must publish one bounded recursive-Green splice"
        );
        assert!(delivery
            .packet_frames
            .iter()
            .flatten()
            .all(|(kind, _)| *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage));

        for ordinal in [0, EDITED_PARAGRAPH, PARAGRAPHS - 1] {
            let marker = format!("paragraph {ordinal:04} ");
            let paragraph_start = target_source
                .find(&marker)
                .expect("target Paragraph marker");
            let content_end = target_source[paragraph_start..]
                .find('\n')
                .map(|offset| paragraph_start + offset + 1)
                .expect("target content line ending");
            let point = paragraph_start + marker.len();
            let point_utf16 = target_source[..point].encode_utf16().count();
            let (owner_kind, range, ancestry) =
                recursive_green_query_shape(&host, source_version, point, point_utf16);
            let expected_owner_kind = if ordinal == EDITED_PARAGRAPH {
                expected_middle_kind
            } else {
                5
            };
            assert_eq!(owner_kind, expected_owner_kind);
            assert_eq!(range[0] as usize, paragraph_start);
            assert_eq!(range[1] as usize, content_end - 1);
            assert_eq!(
                range[2] as usize,
                target_source[..paragraph_start].encode_utf16().count()
            );
            assert_eq!(
                range[3] as usize,
                target_source[..content_end - 1].encode_utf16().count()
            );
            assert_eq!(ancestry.first(), Some(&1));
            assert_eq!(ancestry.last(), Some(&expected_owner_kind));
        }

        let retained = endpoint.retained.as_ref().expect("retained Setext target");
        assert!(matches!(
            retained.restart.as_ref(),
            Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
                if *source == target_version
        ));
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        assert!(endpoint
            .has_exact_base_for(&runtime, target_version)
            .expect("next Setext revision authority"));

        current_source = target_source;
        current_version = target_version;
        current_ack = delivery.ack;
    }

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn whole_paragraph_replacement_keeps_ready_candidate_polling_live() {
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [641, 642, 643, 644],
        source_session_identity: 645,
        worker_generation: 1,
    };
    let mut runtime =
        DocumentRuntime::new("plain", standard_document_runtime_config()).expect("runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_source = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start base candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent host");
    host.observe_source_version(source_version_for(binding, completion))
        .expect("host observes base source");
    deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    runtime
        .apply_edit(base_source, 0..5, "**plain**")
        .expect("replace the whole Paragraph");
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan replacement SourceFacts");
    let incremental = endpoint
        .has_incremental_base_for_plan(&runtime, &plan)
        .expect("inspect exact route");
    let target_completion;
    if incremental {
        let witness = complete_incremental_source_facts(&mut runtime);
        let target = runtime
            .snapshot_current_source()
            .expect("exact replacement target");
        target_completion = completion_for_persistent_target(&runtime, 2, 1);
        endpoint
            .start_incremental(&runtime, target, witness, binding, target_completion)
            .expect("start exact replacement candidate");
    } else {
        assert!(runtime.cancel_source_facts());
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 2, 1);
        target_completion = completion;
        endpoint
            .start(certified, binding, target_completion)
            .expect("start definitive clean replacement candidate");
    }
    host.observe_source_version(source_version_for(binding, target_completion))
        .expect("host observes replacement source");

    let mut zero_progress_phase = None;
    let mut reached_event = false;
    for _ in 0..1_000 {
        let phase = active_candidate_phase(endpoint.active.as_ref());
        match endpoint
            .poll(&mut runtime, 32)
            .expect("bounded replacement candidate poll")
        {
            CandidatePoll::Pending { transitions } => {
                assert!(transitions <= 32);
                if transitions == 0 && zero_progress_phase.is_none() {
                    zero_progress_phase = Some(phase);
                }
            }
            CandidatePoll::Event { transitions, .. } => {
                assert!(transitions <= 32);
                reached_event = true;
                break;
            }
            CandidatePoll::HotInlineEvent { .. } => {
                panic!("replacement structural candidate emitted a hot-inline event")
            }
            CandidatePoll::ViewportPresentationEvent { .. } => {
                panic!("replacement structural candidate emitted a viewport event")
            }
            CandidatePoll::ViewportPresentationUnavailable { .. } => {
                panic!("replacement candidate emitted viewport unavailability")
            }
        }
    }
    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
    assert!(
        reached_event,
        "the replacement candidate did not reach its first bounded offer"
    );
    assert_eq!(
        zero_progress_phase, None,
        "a ready candidate phase must keep the native isolate poll edge alive"
    );
}

#[test]
fn fresh_canonical_publication_omits_inline_presentation_authority() {
    fn delivered_viewport(source: &str, document_seed: u32) -> Vec<u8> {
        let profile = SourceFactsScanProfile::new(8).expect("test profile");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let binding = SessionBinding {
            document_session: [
                document_seed,
                document_seed + 1,
                document_seed + 2,
                document_seed + 3,
            ],
            source_session_identity: document_seed + 4,
            worker_generation: 1,
        };
        let mut runtime = DocumentRuntime::new(source, standard_document_runtime_config())
            .expect("fresh inline runtime");
        let (certified, completion) =
            complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
        let mut endpoint = CandidateEndpoint::new();
        endpoint
            .start(certified, binding, completion)
            .expect("start fresh inline candidate");
        let mut host = NativeCandidateHost::new(HostConfig {
            document_session: binding.document_session,
            grammar_revision: GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: AUTHORITY_MASK_ALL_ROLES,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("fresh inline host");
        let source_version = source_version_for(binding, completion);
        host.observe_source_version(source_version)
            .expect("host observes fresh inline source");
        let delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
            &mut endpoint,
            &mut runtime,
            &mut host,
        );
        assert_eq!(delivery.offer.mode, PublicationMode::FullSnapshot);
        let mut output = vec![0_u8; 4096];
        let outcome = host
            .query_structural(
                HostPointQuery {
                    source_version,
                    position: HostSourceMetric { bytes: 0, utf16: 0 },
                    affinity: HostMetricAffinity::Downstream,
                    budget: HostQueryBudget {
                        maximum_encoded_bytes: output.len() as u32,
                        maximum_open_depth: 64,
                        maximum_leaf_count: 64,
                        maximum_tree_nodes_visited: 256,
                    },
                },
                &mut output,
            )
            .expect("query fresh inline viewport");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("fresh inline candidate must author a viewport: {outcome:?}");
        };
        output.truncate(receipt.encoded_bytes as usize);
        drain_candidate_cleanup(&mut endpoint, &mut runtime);
        close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
        output
    }

    for output in [
        delivered_viewport("A **bold _em_** and `code`.", 641),
        delivered_viewport("plain @ blocker", 631),
    ] {
        assert_eq!(
            u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
            9,
            "fresh canonical publication must remain structure-only until inline demand"
        );
        let ancestry_count = usize::try_from(u32::from_le_bytes(
            output[36..40].try_into().expect("ancestry count"),
        ))
        .expect("ancestry count fits usize");
        assert_eq!(output.len(), 112 + ancestry_count * 16);
    }
}

#[test]
fn ordinary_target_cut_validation_distinguishes_lf_crlf_and_unterminated_eof() {
    let runtime = DocumentRuntime::new("a\nb\r\nc", standard_document_runtime_config())
        .expect("boundary runtime");
    let target = runtime.snapshot_current_source().expect("target lease");
    assert!(target_physical_line_cut_is_exact(&target, 0, 0).expect("BOF cut"));
    assert!(target_physical_line_cut_is_exact(&target, 2, 2).expect("LF cut"));
    assert!(
        !target_physical_line_cut_is_exact(&target, 4, 4).expect("inside CRLF"),
        "a cut between CR and LF is never a physical-line start"
    );
    assert!(target_physical_line_cut_is_exact(&target, 5, 5).expect("CRLF cut"));
    assert!(
        !target_physical_line_cut_is_exact(&target, 6, 6).expect("unterminated EOF"),
        "EOF after unterminated content is not itself a new physical-line start"
    );
    drop(target);
    let mut runtime = runtime;
    runtime.begin_close().expect("begin close");
    while !runtime.poll_close(64).expect("close poll").complete {}
}

#[test]
fn ordinary_paragraph_bof_edit_streams_exact_segmented_delta_and_keeps_late_inline_live() {
    let source = format!(
        "**bold** ordinary paragraph line\n{}",
        "ordinary paragraph line\n".repeat(219)
    );
    assert!(
        source.len()
            > usize::try_from(flark_parser::M11_ORDINARY_PARAGRAPH_CHECKPOINT_STRIDE_BYTES)
                .expect("checkpoint stride"),
        "fixture must cross the sparse ordinary-Paragraph checkpoint stride"
    );
    let profile = SourceFactsScanProfile::new(8).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [681, 682, 683, 684],
        source_session_identity: 685,
        worker_generation: 1,
    };
    let mut runtime = DocumentRuntime::new(&source, standard_document_runtime_config())
        .expect("ordinary-Paragraph runtime");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let source_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, completion)
        .expect("start clean ordinary-Paragraph candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent candidate host");
    host.observe_source_version(source_version_for(binding, completion))
        .expect("host observes ordinary-Paragraph source");

    let delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);

    assert_eq!(delivery.offer.mode, PublicationMode::FullSnapshot);
    assert!(
        endpoint
            .has_exact_base_for(&runtime, source_version)
            .expect("inspect retained ordinary-Paragraph base"),
        "the delivered publication must retain its source- and binding-authenticated \
             ordinary-Paragraph restart collection"
    );
    let target_version = runtime
        .apply_edit(
            source_version,
            source.find("bold").expect("bold source") + 1
                ..source.find("bold").expect("bold source") + 2,
            "O",
        )
        .expect("edit ordinary Paragraph at BOF")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan incremental SourceFacts");
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("inspect planned incremental routing"),
        "an early edit must select BOF-to-convergence parser authority"
    );
    assert!(
        endpoint
            .has_exact_base_for(&runtime, source_version)
            .expect("reinspect retained ordinary-Paragraph base"),
        "an eligibility probe must not consume the move-only restart collection"
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("borrow exact target source");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    host.observe_source_version(source_version_for(binding, target_completion))
        .expect("host observes BOF target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start authenticated BOF crop");
    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(delivery.ack));
    assert!(
        target_delivery.offer.transferred_record_count < target_delivery.offer.target_record_count,
        "exact BOF delta must omit authenticated reused records"
    );
    let mut output = vec![0_u8; 4096];
    let target_source_version = source_version_for(binding, target_completion);
    let outcome = host
        .query_structural(
            HostPointQuery {
                source_version: target_source_version,
                position: HostSourceMetric { bytes: 0, utf16: 0 },
                affinity: HostMetricAffinity::Downstream,
                budget: HostQueryBudget {
                    maximum_encoded_bytes: output.len() as u32,
                    maximum_open_depth: 64,
                    maximum_leaf_count: 64,
                    maximum_tree_nodes_visited: 256,
                },
            },
            &mut output,
        )
        .expect("query exact BOF inline facts");
    let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
        panic!("exact BOF candidate must author a structural viewport: {outcome:?}");
    };
    output.truncate(receipt.encoded_bytes as usize);
    assert_eq!(
        output.len(),
        HOST_RECURSIVE_GREEN_VIEWPORT_HEADER_BYTES + 2 * HOST_RECURSIVE_GREEN_ANCESTOR_RECORD_BYTES
    );
    assert_eq!(
        u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
        HOST_RECURSIVE_GREEN_VIEWPORT_SCHEMA,
        "exact BOF delivery must preserve recursive-Green authority"
    );
    endpoint
        .request_hot_inline(
            &mut runtime,
            InlineRefinementCommand {
                binding,
                refinement_generation: 1,
                source_version: target_source_version,
                base_ack: target_delivery.ack,
                byte_offset: 3,
                utf16_offset: 3,
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::Automatic,
            },
        )
        .expect("late inline demand must queue while superseded-base cleanup is pending");
    let (inline_begin, inline_ack) =
        deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 15_000);
    assert!(matches!(
        inline_begin.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative { fact_count, .. } if fact_count > 0
    ));
    assert_eq!(
        inline_ack.disposition,
        InlineSidecarAckDisposition::Authoritative
    );
    assert!(
        endpoint
            .has_exact_base_for(&runtime, target_version)
            .expect("inspect retained BOF target base"),
        "BOF delivery must retain target checkpoint authority"
    );

    let unsupported_edit = source.find("ordinary").expect("ordinary source");
    let unsupported_version = runtime
        .apply_edit(target_version, unsupported_edit..unsupported_edit, "@")
        .expect("insert exact inline hazard")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan unsupported exact SourceFacts");
    assert!(endpoint
        .has_incremental_base_for_plan(&runtime, &plan)
        .expect("preflight unsupported exact crop"));
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("unsupported exact target source");
    let unsupported_completion = completion_for_persistent_target(&runtime, 3, 2);
    let unsupported_source_version = source_version_for(binding, unsupported_completion);
    host.observe_source_version(unsupported_source_version)
        .expect("host observes unsupported target");
    endpoint
        .start_incremental(
            &runtime,
            target_lease,
            witness,
            binding,
            unsupported_completion,
        )
        .expect("start unsupported exact crop");
    let unsupported_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(
        unsupported_delivery.offer.mode,
        PublicationMode::ExactBaseDelta
    );
    assert_eq!(
        unsupported_delivery.offer.base_ack,
        Some(target_delivery.ack)
    );
    assert!(
        unsupported_delivery.offer.transferred_record_count
            < unsupported_delivery.offer.target_record_count,
        "the next exact crop must retain the immediately preceding base"
    );
    assert_installed_candidate_has_no_inline(&host, unsupported_source_version);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, unsupported_version)
        .expect("unsupported target remains an exact base"));

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn exact_base_survives_mid_parse_cancel_and_replacement_converges() {
    let mut fixture = OrdinaryCancellationFixture::new([721, 722, 723, 724]);
    let first_edit = fixture.edit_offset(512);
    fixture.start_target(first_edit, "Z", 2, 1);
    assert_eq!(
        active_candidate_phase(fixture.endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact"
    );
    assert!(fixture.endpoint.recursive_green.target_work_pending());
    assert!(matches!(
        fixture
            .endpoint
            .poll(&mut fixture.runtime, 1)
            .expect("advance bounded recursive-Green adoption"),
        CandidatePoll::Pending { transitions: 1 }
    ));
    assert_eq!(
        active_candidate_phase(fixture.endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact",
        "fixture must cancel after Green adoption work, not before it starts"
    );

    fixture.endpoint.cancel().expect("cancel target mid-parse");
    drain_candidate_cleanup(&mut fixture.endpoint, &mut fixture.runtime);
    fixture.assert_original_base_restored();

    let replacement_edit = fixture.edit_offset(513);
    let replacement = fixture.start_target(replacement_edit, "Y", 3, 2);
    fixture.deliver_replacement(replacement);
    close_exact_pair_to_zero(
        &mut fixture.endpoint,
        &mut fixture.runtime,
        &mut fixture.host,
    );
}

#[test]
fn exact_base_survives_mid_stream_cancel_and_replacement_converges() {
    let mut fixture = OrdinaryCancellationFixture::new([731, 732, 733, 734]);
    let first_edit = fixture.edit_offset(512);
    fixture.start_target(first_edit, "Z", 2, 1);

    let mut saw_begin = false;
    let mut saw_packet = false;
    for event_id in 1..1_000_000_u32 {
        match fixture
            .endpoint
            .poll(&mut fixture.runtime, 1)
            .expect("advance exact target to stream")
        {
            CandidatePoll::Pending { transitions } => assert_eq!(transitions, 1),
            CandidatePoll::Event { transitions, event } => {
                assert!(transitions <= 1);
                let CandidateEvent { credit, body } = *event;
                match body {
                    CandidateEventBody::Begin(_) => {
                        assert!(!saw_begin);
                        saw_begin = true;
                        fixture
                            .endpoint
                            .accept_credit(credit, event_id)
                            .expect("accept target Begin credit");
                    }
                    CandidateEventBody::Packet { encoded } => {
                        assert!(saw_begin);
                        let packet = decode_publication_packet(&encoded)
                            .expect("decode in-flight exact packet");
                        assert!(packet.frame_count > 0);
                        let offer_id = packet.offer_id;
                        let Some(ActiveCandidate::Streaming(streaming)) =
                            fixture.endpoint.active.as_ref()
                        else {
                            panic!("packet receipt must leave the exact target streaming");
                        };
                        assert!(streaming.stream.is_some());
                        assert!(streaming.sealed_publication.is_none());
                        assert!(streaming.exact_base_recovery.is_some());
                        assert!(matches!(
                            streaming.phase,
                            StreamPhase::AwaitPacketReceipt { .. }
                        ));
                        fixture
                            .endpoint
                            .accept_credit(credit, event_id)
                            .expect("accept target Packet credit");
                        assert!(fixture
                            .endpoint
                            .handle_host_poll(
                                event_id,
                                offer_id,
                                HostPollPhase::PacketCredit,
                                HostPollResult::Rejected(
                                    crate::v3_publication_wire::HostRejectReason::Superseded,
                                ),
                            )
                            .expect("reject target mid-stream")
                            .is_none());
                        saw_packet = true;
                        break;
                    }
                    CandidateEventBody::Commit(_) | CandidateEventBody::DeliveryAcknowledged(_) => {
                        panic!("fixture must cancel before exact target commit")
                    }
                }
            }
            CandidatePoll::HotInlineEvent { .. } => {
                panic!("structural cancellation fixture emitted hot-inline work")
            }
            CandidatePoll::ViewportPresentationEvent { .. } => {
                panic!("structural cancellation fixture emitted viewport work")
            }
            CandidatePoll::ViewportPresentationUnavailable { .. } => {
                panic!("structural fixture emitted viewport unavailability")
            }
        }
    }
    assert!(saw_packet, "exact target did not reach packet streaming");
    assert!(fixture.endpoint.cleanup_pending());
    drain_candidate_cleanup(&mut fixture.endpoint, &mut fixture.runtime);
    fixture.assert_original_base_restored();

    let replacement_edit = fixture.edit_offset(513);
    let replacement = fixture.start_target(replacement_edit, "Y", 3, 2);
    fixture.deliver_replacement(replacement);
    close_exact_pair_to_zero(
        &mut fixture.endpoint,
        &mut fixture.runtime,
        &mut fixture.host,
    );
}

#[test]
fn ordinary_paragraph_middle_edit_streams_exact_segmented_delta() {
    let profile = SourceFactsScanProfile::new(2).expect("dense test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [711, 712, 713, 714],
        source_session_identity: 715,
        worker_generation: 1,
    };
    let mut base_source: String = (0..1_024)
        .map(|ordinal| format!("ordinary prose line {ordinal:04} {}\n", "a".repeat(40)))
        .collect();
    base_source.push_str("\nLate **bold** and _live_.\n");
    let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
        .expect("ordinary exact-delta runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start clean ordinary base candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent candidate host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes ordinary base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

    let edit_start = base_source
        .find("ordinary prose line 0512 ")
        .expect("middle line")
        + "ordinary prose line 0512 ".len()
        + 20;
    let target_version = runtime
        .apply_edit(base_version, edit_start..edit_start + 1, "Z")
        .expect("middle ordinary edit")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan bounded SourceFacts replacement");
    assert!(
        plan.base_byte_range().start > 0 && plan.base_byte_range().end < base_version.byte_len(),
        "fixture must leave exact parser authority on both sides of the changed pages"
    );
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("preflight ordinary crop"),
        "middle edit must have an authenticated restart and convergence line"
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("borrow exact target source");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    host.observe_source_version(source_version_for(binding, target_completion))
        .expect("host observes exact ordinary target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start authenticated ordinary crop candidate");
    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);

    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
    let block_replacement_records = target_delivery
        .packet_frames
        .iter()
        .flatten()
        .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::BlockSequenceReplacementPage)
        .map(|(_, records)| *records)
        .sum::<u32>();
    assert_eq!(
        block_replacement_records, 0,
        "recursive-Green authority must not fall through to legacy block replacement"
    );
    let recursive_green_replacement_records = target_delivery
        .packet_frames
        .iter()
        .flatten()
        .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage)
        .map(|(_, records)| *records)
        .sum::<u32>();
    assert!(
        recursive_green_replacement_records > 0,
        "one local ordinary-Paragraph edit must transfer a Green replacement window"
    );
    let target_source_version = source_version_for(binding, target_completion);
    let mut output = vec![0_u8; 4096];
    let outcome = host
        .query_structural(
            HostPointQuery {
                source_version: target_source_version,
                position: HostSourceMetric {
                    bytes: u32::try_from(edit_start).expect("test edit byte"),
                    utf16: u32::try_from(edit_start).expect("ASCII test edit UTF-16"),
                },
                affinity: HostMetricAffinity::Downstream,
                budget: HostQueryBudget {
                    maximum_encoded_bytes: output.len() as u32,
                    maximum_open_depth: 64,
                    maximum_leaf_count: 64,
                    maximum_tree_nodes_visited: 256,
                },
            },
            &mut output,
        )
        .expect("query independently replayed target");
    let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
        panic!("replayed target must expose the edited Paragraph: {outcome:?}");
    };
    assert_eq!(
        receipt.encoded_bytes as usize,
        HOST_RECURSIVE_GREEN_VIEWPORT_HEADER_BYTES + 2 * HOST_RECURSIVE_GREEN_ANCESTOR_RECORD_BYTES
    );
    assert_eq!(
        u32::from_le_bytes(output[8..12].try_into().expect("viewport schema")),
        HOST_RECURSIVE_GREEN_VIEWPORT_SCHEMA,
        "persistent Green reuse must preserve exact structural authority without inline facts"
    );
    assert!(
        target_delivery.offer.transferred_record_count < target_delivery.offer.target_record_count,
        "exact middle delta must omit authenticated reused records"
    );
    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == target_version
    ));
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, target_version)
        .expect("next-revision exact base"));

    let late_inline_point = base_source
        .find("bold")
        .expect("late recursive-Green inline point");
    endpoint
        .request_hot_inline(
            &mut runtime,
            InlineRefinementCommand {
                binding,
                refinement_generation: 1,
                source_version: target_source_version,
                base_ack: target_delivery.ack,
                byte_offset: u32::try_from(late_inline_point).expect("late inline test byte"),
                utf16_offset: u32::try_from(late_inline_point)
                    .expect("ASCII late inline test UTF-16"),
                affinity: InlinePointAffinity::After,
                target: InlineRefinementTarget::RecursiveGreenParagraph,
            },
        )
        .expect("retained recursive-Green Paragraph query above the old 64-KiB cap");
    let (inline_begin, inline_ack) =
        deliver_hot_inline_sidecar_with_unit_fuel(&mut endpoint, &mut runtime, 25_000);
    assert!(matches!(
        inline_begin.envelope.disposition,
        HotInlineSidecarDisposition::Authoritative { fact_count, .. }
            if fact_count > 0
    ));
    assert_eq!(
        inline_ack.disposition,
        InlineSidecarAckDisposition::Authoritative
    );

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn independent_host_4096_paragraph_distant_middle_edits_remain_bounded_exact_deltas() {
    const PARAGRAPHS: usize = 4_096;

    let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [901, 902, 903, 904],
        source_session_identity: 905,
        worker_generation: 1,
    };
    let base_source: String = (0..PARAGRAPHS)
        .map(|ordinal| format!("paragraph {ordinal:04} {}\n\n", "a".repeat(32)))
        .collect();
    let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
        .expect("4,096-Paragraph runtime");
    let base_started = std::time::Instant::now();
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start 4,096-Paragraph base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent 4,096-Paragraph host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes exact base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    let base_elapsed = base_started.elapsed();
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == base_version
    ));
    assert!(endpoint
        .recursive_green
        .has_installed_session_for(base_delivery.ack));

    let paragraph_start = base_source
        .find("paragraph 2048 ")
        .expect("middle Paragraph");
    let paragraph_line_end = base_source[paragraph_start..]
        .find('\n')
        .map(|offset| paragraph_start + offset)
        .expect("middle Paragraph line ending");
    let edit_start = paragraph_line_end + 1;
    let mut target_source = base_source.clone();
    target_source.replace_range(edit_start..edit_start + 1, "\n\n");
    let paragraph_end = target_source[paragraph_start..]
        .find('\n')
        .map(|offset| paragraph_start + offset + 1)
        .expect("middle Paragraph line ending");
    let incremental_started = std::time::Instant::now();
    let target_version = runtime
        .apply_edit(base_version, edit_start..edit_start + 1, "\n\n")
        .expect("middle Paragraph newline insertion")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan bounded SourceFacts replacement");
    assert_eq!(
        plan.base_byte_range(),
        &(0..base_version.byte_len()),
        "the production SourceFacts page may span the complete fixture"
    );
    assert_eq!(
        plan.exact_parser_base_byte_range(),
        Some(&(edit_start..edit_start + 1)),
        "parser restart must use the exact edit envelope, not the storage page"
    );
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("preflight bounded ordinary crop"),
        "the middle edit must select authenticated restart and convergence authority"
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("borrow exact target");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    let target_source_version = source_version_for(binding, target_completion);
    host.observe_source_version(target_source_version)
        .expect("host observes exact target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start bounded ordinary crop");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact",
        "the 4,096-Paragraph edit must wait on bounded Green adoption, not exact-clean fallback"
    );

    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    let incremental_elapsed = incremental_started.elapsed();

    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
    assert_eq!(target_delivery.ack.source_version, target_source_version);
    assert!(
        target_delivery.offer.transferred_record_count < target_delivery.offer.target_record_count,
        "the exact delta must omit authenticated unchanged records"
    );
    let recursive_green_replacement_records = target_delivery
        .packet_frames
        .iter()
        .flatten()
        .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage)
        .map(|(_, records)| *records)
        .sum::<u32>();
    assert!(
        recursive_green_replacement_records > 0,
        "the exact delta must carry a nonempty recursive-Green replacement window"
    );
    assert!(
        target_delivery
            .packet_frames
            .iter()
            .flatten()
            .all(|(kind, _)| { *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage }),
        "a recursive-Green base must never fall through to legacy block replacement"
    );
    assert!(target_delivery.contains_recursive_green_leaf);
    eprintln!(
        "m11_4096_paragraph_bounded_exact_delta source_bytes={} base_ms={} \
             incremental_ms={} target_records={} transferred_records={} \
             recursive_green_replacement_records={}",
        base_source.len(),
        base_elapsed.as_millis(),
        incremental_elapsed.as_millis(),
        target_delivery.offer.target_record_count,
        target_delivery.offer.transferred_record_count,
        recursive_green_replacement_records,
    );

    let (owner_kind, range, ancestry) = recursive_green_query_shape(
        &host,
        target_source_version,
        paragraph_start + "paragraph 2048 ".len(),
        paragraph_start + "paragraph 2048 ".len(),
    );
    assert_eq!(owner_kind, 5, "the edited owner remains a Green Paragraph");
    let paragraph_content_end = paragraph_end - 1;
    assert_eq!(
        range,
        [
            paragraph_start as u32,
            paragraph_content_end as u32,
            paragraph_start as u32,
            paragraph_content_end as u32,
        ]
    );
    assert!(!ancestry.is_empty());
    let target_lease = runtime
        .snapshot_current_source()
        .expect("reborrow exact installed source");
    assert_eq!(target_lease.version(), target_version);
    let mut cursor = target_lease
        .cursor_in(paragraph_start..paragraph_end)
        .expect("bounded target Paragraph cursor");
    let mut copied = vec![0_u8; paragraph_end - paragraph_start];
    assert_eq!(cursor.read(&mut copied), copied.len());
    drop(cursor.finish().expect("finish target Paragraph cursor"));
    assert_eq!(
        copied,
        target_source.as_bytes()[paragraph_start..paragraph_end],
        "the independently installed semantic range must name exact canonical source"
    );

    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, target_version)
        .expect("target exact-base continuity"));

    let second_paragraph = target_source
        .find("paragraph 3072 ")
        .expect("distant Paragraph");
    let second_edit_start = second_paragraph + "paragraph 3072 ".len() + 16;
    let second_incremental_started = std::time::Instant::now();
    let second_version = runtime
        .apply_edit(
            target_version,
            second_edit_start..second_edit_start + 1,
            "Y",
        )
        .expect("shape-preserving distant Paragraph edit")
        .source()
        .current();
    let second_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan distant SourceFacts replacement");
    assert_eq!(
        second_plan.exact_parser_base_byte_range(),
        Some(&(second_edit_start..second_edit_start + 1))
    );
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &second_plan)
            .expect("preflight distant ordinary crop"),
        "a successful local edit must preserve restart authority in distant unchanged regions"
    );
    let second_witness = complete_incremental_source_facts(&mut runtime);
    let second_completion = completion_for_persistent_target(&runtime, 3, 2);
    let second_source_version = source_version_for(binding, second_completion);
    host.observe_source_version(second_source_version)
        .expect("host observes distant target");
    endpoint
        .start_incremental(
            &runtime,
            runtime
                .snapshot_current_source()
                .expect("borrow distant exact target"),
            second_witness,
            binding,
            second_completion,
        )
        .expect("start distant ordinary crop");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact",
        "moving the caret must wait on retained recursive-Green authority without starting a \
         whole-document clean parse"
    );
    let second_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    let second_incremental_elapsed = second_incremental_started.elapsed();
    assert_eq!(second_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(second_delivery.offer.base_ack, Some(target_delivery.ack));
    assert_eq!(second_delivery.ack.source_version, second_source_version);
    assert!(
        second_delivery.offer.transferred_record_count < second_delivery.offer.target_record_count
    );
    assert!(second_delivery
        .packet_frames
        .iter()
        .flatten()
        .all(|(kind, _)| { *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage }));
    assert_eq!(
        endpoint.recursive_green_path_receipt(),
        RecursiveGreenPathReceipt {
            local_adoption_deliveries: 2,
            clean_fallback_deliveries: 0,
        },
        "both edits must remain on the recursive-Green local-adoption path"
    );
    eprintln!(
        "m11_4096_paragraph_newline_then_distant_delta second_incremental_ms={} \
         target_records={} transferred_records={}",
        second_incremental_elapsed.as_millis(),
        second_delivery.offer.target_record_count,
        second_delivery.offer.transferred_record_count,
    );
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, second_version)
        .expect("distant target exact-base continuity"));
    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn independent_host_4096_paragraph_first_edit_is_bounded_exact_delta() {
    const PARAGRAPHS: usize = 4_096;
    const MAXIMUM_TRANSFERRED_RECORDS: u32 = 64;

    let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [911, 912, 913, 914],
        source_session_identity: 915,
        worker_generation: 1,
    };
    let base_source: String = (0..PARAGRAPHS)
        .map(|ordinal| format!("paragraph {ordinal:04} {}\n\n", "a".repeat(32)))
        .collect();
    let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
        .expect("first-Paragraph runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start first-Paragraph base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent first-Paragraph host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes first-Paragraph base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == base_version
    ));
    assert!(endpoint
        .recursive_green
        .has_installed_session_for(base_delivery.ack));

    let edit_start = base_source
        .find("aaaaaaaa")
        .expect("first Paragraph payload")
        + 4;
    const REPLACEMENT: &str = "expanded";
    let mut target_source = base_source.clone();
    target_source.replace_range(edit_start..edit_start + 1, REPLACEMENT);
    let target_version = runtime
        .apply_edit(base_version, edit_start..edit_start + 1, REPLACEMENT)
        .expect("lengthen first Paragraph")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan first-Paragraph SourceFacts replacement");
    assert_eq!(
        plan.exact_parser_base_byte_range(),
        Some(&(edit_start..edit_start + 1)),
        "BOF selection must follow the exact edit envelope"
    );
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("preflight segmented BOF crop"),
        "a first-block edit must select authenticated BOF-to-Paragraph convergence"
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    let target_source_version = source_version_for(binding, target_completion);
    host.observe_source_version(target_source_version)
        .expect("host observes first-Paragraph target");
    endpoint
        .start_incremental(
            &runtime,
            runtime
                .snapshot_current_source()
                .expect("borrow first-Paragraph target"),
            witness,
            binding,
            target_completion,
        )
        .expect("start segmented BOF crop");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact",
        "the first-block edit must wait on bounded Green adoption, not exact-clean fallback"
    );

    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
    assert_eq!(target_delivery.ack.source_version, target_source_version);
    assert!(
        target_delivery.offer.transferred_record_count <= MAXIMUM_TRANSFERRED_RECORDS,
        "one first-block edit transferred {} of {} records",
        target_delivery.offer.transferred_record_count,
        target_delivery.offer.target_record_count
    );
    assert!(
        target_delivery.offer.transferred_record_count < target_delivery.offer.target_record_count
    );
    let recursive_green_replacement_records = target_delivery
        .packet_frames
        .iter()
        .flatten()
        .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage)
        .map(|(_, records)| *records)
        .sum::<u32>();
    assert!(
        recursive_green_replacement_records > 0
            && recursive_green_replacement_records <= MAXIMUM_TRANSFERRED_RECORDS,
        "the first-block recursive splice must publish a bounded nonempty replacement"
    );
    assert!(target_delivery
        .packet_frames
        .iter()
        .flatten()
        .all(|(kind, _)| { *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage }));

    for ordinal in [0, PARAGRAPHS / 2, PARAGRAPHS - 1] {
        let marker = format!("paragraph {ordinal:04} ");
        let paragraph_start = target_source
            .find(&marker)
            .expect("target Paragraph marker");
        let paragraph_end = target_source[paragraph_start..]
            .find('\n')
            .map(|offset| paragraph_start + offset + 1)
            .expect("target Paragraph line ending");
        let point = paragraph_start + marker.len();
        let (owner_kind, range, ancestry) =
            recursive_green_query_shape(&host, target_source_version, point, point);
        assert_eq!(owner_kind, 5);
        assert_eq!(range[0] as usize, paragraph_start);
        assert_eq!(range[1] as usize, paragraph_end - 1);
        assert_eq!(range[2] as usize, paragraph_start);
        assert_eq!(range[3] as usize, paragraph_end - 1);
        assert!(!ancestry.is_empty());
    }

    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, target_version)
        .expect("first-edit recursive exact base"));

    let next_edit = target_source
        .find("paragraph 2048 ")
        .expect("next-edit Paragraph")
        + "paragraph 2048 ".len();
    let next_version = runtime
        .apply_edit(target_version, next_edit..next_edit + 1, "Q")
        .expect("apply next exact edit")
        .source()
        .current();
    let next_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan next exact edit");
    assert!(endpoint
        .has_incremental_base_for_plan(&runtime, &next_plan)
        .expect("preflight next exact edit"));
    let next_witness = complete_incremental_source_facts(&mut runtime);
    let next_completion = completion_for_persistent_target(&runtime, 3, 2);
    host.observe_source_version(source_version_for(binding, next_completion))
        .expect("host observes next target");
    endpoint
        .start_incremental(
            &runtime,
            runtime
                .snapshot_current_source()
                .expect("borrow next target"),
            next_witness,
            binding,
            next_completion,
        )
        .expect("start next exact edit");
    let next_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(next_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(next_delivery.offer.base_ack, Some(target_delivery.ack));
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, next_version)
        .expect("next-revision exact base"));

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn independent_host_4096_paragraph_final_edit_is_bounded_exact_delta() {
    const PARAGRAPHS: usize = 4_096;
    const MAXIMUM_TRANSFERRED_RECORDS: u32 = 64;

    let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [916, 917, 918, 919],
        source_session_identity: 920,
        worker_generation: 1,
    };
    let base_source: String = (0..PARAGRAPHS)
        .map(|ordinal| format!("paragraph {ordinal:04} {}\n\n", "a".repeat(32)))
        .collect();
    let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
        .expect("final-Paragraph runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start final-Paragraph base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent final-Paragraph host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes final-Paragraph base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == base_version
    ));
    assert!(endpoint
        .recursive_green
        .has_installed_session_for(base_delivery.ack));

    let edit_start = base_source
        .rfind("aaaaaaaa")
        .expect("final Paragraph payload")
        + 4;
    const REPLACEMENT: &str = "expanded";
    let mut target_source = base_source.clone();
    target_source.replace_range(edit_start..edit_start + 1, REPLACEMENT);
    let target_version = runtime
        .apply_edit(base_version, edit_start..edit_start + 1, REPLACEMENT)
        .expect("lengthen final Paragraph")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan final-Paragraph SourceFacts replacement");
    assert_eq!(
        plan.exact_parser_base_byte_range(),
        Some(&(edit_start..edit_start + 1)),
        "EOF selection must follow the exact edit envelope"
    );
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("preflight segmented EOF crop"),
        "a final-block edit must select authenticated Paragraph-to-EOF authority"
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    let target_source_version = source_version_for(binding, target_completion);
    host.observe_source_version(target_source_version)
        .expect("host observes final-Paragraph target");
    endpoint
        .start_incremental(
            &runtime,
            runtime
                .snapshot_current_source()
                .expect("borrow final-Paragraph target"),
            witness,
            binding,
            target_completion,
        )
        .expect("start segmented EOF crop");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact",
        "the final-block edit must wait on bounded Green adoption, not exact-clean fallback"
    );
    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
    assert_eq!(target_delivery.ack.source_version, target_source_version);
    assert!(
        target_delivery.offer.transferred_record_count <= MAXIMUM_TRANSFERRED_RECORDS,
        "one final-block edit transferred {} of {} records",
        target_delivery.offer.transferred_record_count,
        target_delivery.offer.target_record_count
    );
    assert!(
        target_delivery.offer.transferred_record_count < target_delivery.offer.target_record_count
    );
    let recursive_green_replacement_records = target_delivery
        .packet_frames
        .iter()
        .flatten()
        .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage)
        .map(|(_, records)| *records)
        .sum::<u32>();
    assert!(
        recursive_green_replacement_records > 0
            && recursive_green_replacement_records <= MAXIMUM_TRANSFERRED_RECORDS,
        "the final-block Green splice must publish a bounded nonempty replacement"
    );
    assert!(target_delivery
        .packet_frames
        .iter()
        .flatten()
        .all(|(kind, _)| *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage));

    for ordinal in [0, PARAGRAPHS / 2, PARAGRAPHS - 1] {
        let marker = format!("paragraph {ordinal:04} ");
        let paragraph_start = target_source
            .find(&marker)
            .expect("target Paragraph marker");
        let paragraph_end = target_source[paragraph_start..]
            .find('\n')
            .map(|offset| paragraph_start + offset + 1)
            .expect("target Paragraph line ending");
        let point = paragraph_start + marker.len();
        let (owner_kind, range, ancestry) =
            recursive_green_query_shape(&host, target_source_version, point, point);
        assert_eq!(owner_kind, 5);
        assert_eq!(range[0] as usize, paragraph_start);
        assert_eq!(range[1] as usize, paragraph_end - 1);
        assert_eq!(range[2] as usize, paragraph_start);
        assert_eq!(range[3] as usize, paragraph_end - 1);
        assert!(!ancestry.is_empty());
    }

    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == target_version
    ));
    assert_eq!(
        endpoint.recursive_green_path_receipt(),
        RecursiveGreenPathReceipt {
            local_adoption_deliveries: 1,
            clean_fallback_deliveries: 0,
        }
    );
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let next_edit = target_source
        .find("paragraph 2048 ")
        .expect("next-edit Paragraph")
        + "paragraph 2048 ".len();
    let next_version = runtime
        .apply_edit(target_version, next_edit..next_edit + 1, "Q")
        .expect("apply next exact edit")
        .source()
        .current();
    let next_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan next exact edit");
    assert!(endpoint
        .has_incremental_base_for_plan(&runtime, &next_plan)
        .expect("preflight next exact edit"));
    let next_witness = complete_incremental_source_facts(&mut runtime);
    let next_completion = completion_for_persistent_target(&runtime, 3, 2);
    host.observe_source_version(source_version_for(binding, next_completion))
        .expect("host observes next target");
    endpoint
        .start_incremental(
            &runtime,
            runtime
                .snapshot_current_source()
                .expect("borrow next target"),
            next_witness,
            binding,
            next_completion,
        )
        .expect("start next exact edit");
    let next_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(next_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(next_delivery.offer.base_ack, Some(target_delivery.ack));
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, next_version)
        .expect("next-revision exact base"));

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn mixed_atx_and_fence_edits_crop_locally_while_unclosed_fence_falls_back() {
    const PARAGRAPHS: usize = 4_096;

    let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [921, 922, 923, 924],
        source_session_identity: 925,
        worker_generation: 1,
    };
    let mut base_source = String::new();
    for ordinal in 0..PARAGRAPHS {
        base_source.push_str(&format!(
            "paragraph {ordinal:04} {}\ncontinuation {ordinal:04} {}\n\n",
            "a".repeat(64),
            "b".repeat(64),
        ));
        if ordinal == PARAGRAPHS / 2 - 1 {
            base_source.push_str(concat!(
                "## mixed **heading**\n\n",
                "```dart\nlet value = 1;\n```\n\n",
                "    indented value = 1;\n",
                "    indented continuation = 2;\n\n",
                "> quoted value = 1\n",
                "> quoted continuation = 2\n\n",
            ));
        }
    }
    base_source.push_str("> terminal quote value = 1\n> terminal quote continuation = 2\n");

    let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
        .expect("mixed-block runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start mixed-block base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("mixed-block independent host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes mixed-block base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

    let heading_edit = base_source.find("heading").expect("heading content");
    let heading_version = runtime
        .apply_edit(base_version, heading_edit..heading_edit + 1, "H")
        .expect("edit ATX content")
        .source()
        .current();
    let heading_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan ATX SourceFacts replacement");
    assert_eq!(
        heading_plan.exact_parser_base_byte_range(),
        Some(&(heading_edit..heading_edit + 1))
    );
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &heading_plan)
            .expect("preflight ATX crop"),
        "an interior ATX edit must use the surrounding authenticated Paragraph checkpoints"
    );
    let heading_witness = complete_incremental_source_facts(&mut runtime);
    let heading_completion = completion_for_persistent_target(&runtime, 2, 1);
    host.observe_source_version(source_version_for(binding, heading_completion))
        .expect("host observes ATX target");
    endpoint
        .start_incremental(
            &runtime,
            runtime
                .snapshot_current_source()
                .expect("borrow exact ATX target"),
            heading_witness,
            binding,
            heading_completion,
        )
        .expect("start bounded ATX crop");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact"
    );
    let heading_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(heading_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(heading_delivery.offer.base_ack, Some(base_delivery.ack));
    assert!(
        heading_delivery.offer.transferred_record_count
            < heading_delivery.offer.target_record_count
    );
    let heading_source_version = source_version_for(binding, heading_completion);
    let heading_point = heading_edit + 1;
    let (owner_kind, range, ancestry) =
        recursive_green_query_shape(&host, heading_source_version, heading_point, heading_point);
    assert_eq!(owner_kind, 12, "the edited owner remains a Green Heading");
    assert!(range[0] as usize <= heading_point && heading_point < range[1] as usize);
    assert_eq!(ancestry.first(), Some(&1));
    assert_eq!(ancestry.last(), Some(&12));
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let fence_edit = base_source.find("value = 1").expect("fence body") + "value = ".len();
    let fence_version = runtime
        .apply_edit(heading_version, fence_edit..fence_edit + 1, "2")
        .expect("edit fenced-code body")
        .source()
        .current();
    let fence_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan fenced-code SourceFacts replacement");
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &fence_plan)
            .expect("preflight fenced-code crop"),
        "an interior fence-body edit must reuse the same mixed-block crop path"
    );
    let fence_witness = complete_incremental_source_facts(&mut runtime);
    let fence_completion = completion_for_persistent_target(&runtime, 3, 2);
    host.observe_source_version(source_version_for(binding, fence_completion))
        .expect("host observes fenced-code target");
    endpoint
        .start_incremental(
            &runtime,
            runtime
                .snapshot_current_source()
                .expect("borrow exact fenced-code target"),
            fence_witness,
            binding,
            fence_completion,
        )
        .expect("start bounded fenced-code crop");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact"
    );
    let fence_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(fence_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(fence_delivery.offer.base_ack, Some(heading_delivery.ack));
    let fence_source_version = source_version_for(binding, fence_completion);
    let (owner_kind, range, ancestry) =
        recursive_green_query_shape(&host, fence_source_version, fence_edit, fence_edit);
    assert_eq!(owner_kind, 7, "the edited owner remains Green fenced code");
    assert!(range[0] as usize <= fence_edit && fence_edit < range[1] as usize);
    assert_eq!(ancestry.first(), Some(&1));
    assert_eq!(ancestry.last(), Some(&7));
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let indented_edit = base_source
        .find("indented value = 1")
        .expect("indented-code body")
        + "indented value = ".len();
    let indented_version = runtime
        .apply_edit(fence_version, indented_edit..indented_edit + 1, "2")
        .expect("edit indented-code body")
        .source()
        .current();
    let indented_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan indented-code SourceFacts replacement");
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &indented_plan)
            .expect("preflight indented-code crop"),
        "an interior indented-code edit must reuse the same mixed-block crop path"
    );
    let indented_witness = complete_incremental_source_facts(&mut runtime);
    let indented_completion = completion_for_persistent_target(&runtime, 4, 3);
    host.observe_source_version(source_version_for(binding, indented_completion))
        .expect("host observes indented-code target");
    endpoint
        .start_incremental(
            &runtime,
            runtime
                .snapshot_current_source()
                .expect("borrow exact indented-code target"),
            indented_witness,
            binding,
            indented_completion,
        )
        .expect("start bounded indented-code crop");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact"
    );
    let indented_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(
        indented_delivery.offer.mode,
        PublicationMode::ExactBaseDelta
    );
    assert_eq!(indented_delivery.offer.base_ack, Some(fence_delivery.ack));
    assert!(
        indented_delivery.offer.transferred_record_count
            < indented_delivery.offer.target_record_count / 4,
        "the middle indented-code delta must retain the large authenticated prefix and suffix"
    );
    let indented_source_version = source_version_for(binding, indented_completion);
    let (owner_kind, range, ancestry) =
        recursive_green_query_shape(&host, indented_source_version, indented_edit, indented_edit);
    assert_eq!(
        owner_kind, 6,
        "the edited owner remains Green indented code"
    );
    assert!(range[0] as usize <= indented_edit && indented_edit < range[1] as usize);
    assert_eq!(ancestry.first(), Some(&1));
    assert_eq!(ancestry.last(), Some(&6));
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let quote_edit = base_source
        .find("quoted continuation = 2")
        .expect("multiline block-quote body")
        + "quoted continuation = ".len();
    let quote_version = runtime
        .apply_edit(indented_version, quote_edit..quote_edit + 1, "3")
        .expect("edit multiline block-quote body")
        .source()
        .current();
    let quote_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan block-quote SourceFacts replacement");
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &quote_plan)
            .expect("preflight block-quote crop"),
        "an interior exact single-Paragraph block-quote edit must reuse the mixed-block crop path"
    );
    let quote_witness = complete_incremental_source_facts(&mut runtime);
    let quote_completion = completion_for_persistent_target(&runtime, 5, 4);
    host.observe_source_version(source_version_for(binding, quote_completion))
        .expect("host observes block-quote target");
    endpoint
        .start_incremental(
            &runtime,
            runtime
                .snapshot_current_source()
                .expect("borrow exact block-quote target"),
            quote_witness,
            binding,
            quote_completion,
        )
        .expect("start bounded block-quote crop");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact"
    );
    let quote_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(quote_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(quote_delivery.offer.base_ack, Some(indented_delivery.ack));
    assert!(
        quote_delivery.offer.transferred_record_count
            < quote_delivery.offer.target_record_count / 4,
        "the middle block-quote delta must retain the large authenticated prefix and suffix"
    );
    let quote_source_version = source_version_for(binding, quote_completion);
    let (owner_kind, range, ancestry) =
        recursive_green_query_shape(&host, quote_source_version, quote_edit, quote_edit);
    assert_eq!(
        owner_kind, 5,
        "the edited quote child remains a Green Paragraph"
    );
    assert!(range[0] as usize <= quote_edit && quote_edit < range[1] as usize);
    assert_eq!(ancestry.first(), Some(&1));
    assert!(ancestry.contains(&2));
    assert_eq!(ancestry.last(), Some(&5));
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let terminal_quote_edit = base_source
        .rfind("terminal quote continuation = 2")
        .expect("terminal multiline block-quote body")
        + "terminal quote continuation = ".len();
    let terminal_quote_version = runtime
        .apply_edit(
            quote_version,
            terminal_quote_edit..terminal_quote_edit + 1,
            "3",
        )
        .expect("edit terminal multiline block-quote body")
        .source()
        .current();
    let terminal_quote_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan terminal block-quote SourceFacts replacement");
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &terminal_quote_plan)
            .expect("preflight terminal block-quote crop"),
        "a terminal exact single-Paragraph block quote must select authenticated EOF authority"
    );
    let terminal_quote_witness = complete_incremental_source_facts(&mut runtime);
    let terminal_quote_completion = completion_for_persistent_target(&runtime, 6, 5);
    host.observe_source_version(source_version_for(binding, terminal_quote_completion))
        .expect("host observes terminal block-quote target");
    endpoint
        .start_incremental(
            &runtime,
            runtime
                .snapshot_current_source()
                .expect("borrow exact terminal block-quote target"),
            terminal_quote_witness,
            binding,
            terminal_quote_completion,
        )
        .expect("start bounded terminal block-quote crop");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact"
    );
    let terminal_quote_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(
        terminal_quote_delivery.offer.mode,
        PublicationMode::ExactBaseDelta
    );
    assert_eq!(
        terminal_quote_delivery.offer.base_ack,
        Some(quote_delivery.ack)
    );
    assert!(
        terminal_quote_delivery.offer.transferred_record_count
            < terminal_quote_delivery.offer.target_record_count,
        "the terminal block-quote delta must retain its authenticated prefix"
    );
    let terminal_quote_source_version = source_version_for(binding, terminal_quote_completion);
    let (owner_kind, range, ancestry) = recursive_green_query_shape(
        &host,
        terminal_quote_source_version,
        terminal_quote_edit,
        terminal_quote_edit,
    );
    assert_eq!(
        owner_kind, 5,
        "the edited owner remains the Paragraph inside a Green block quote"
    );
    assert!(range[0] as usize <= terminal_quote_edit && terminal_quote_edit < range[1] as usize);
    assert_eq!(ancestry.first(), Some(&1));
    assert!(ancestry.contains(&2));
    assert_eq!(ancestry.last(), Some(&5));
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let terminal_nested_quote_edit = base_source
        .rfind("> terminal quote value = 1")
        .expect("terminal block-quote opener")
        + 1;
    let unsupported_quote_version = runtime
        .apply_edit(
            terminal_quote_version,
            terminal_nested_quote_edit..terminal_nested_quote_edit + 1,
            ">",
        )
        .expect("make the terminal quote nested")
        .source()
        .current();
    let unsupported_quote_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan unsupported terminal block-quote replacement");
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &unsupported_quote_plan)
            .expect("preflight unsupported terminal block-quote crop"),
        "preflight may begin from the exact quote before target semantics are known"
    );
    let unsupported_quote_witness = complete_incremental_source_facts(&mut runtime);
    let unsupported_quote_completion = completion_for_persistent_target(&runtime, 7, 6);
    let unsupported_quote_source_version =
        source_version_for(binding, unsupported_quote_completion);
    host.observe_source_version(unsupported_quote_source_version)
        .expect("host observes unsupported terminal block-quote target");
    endpoint
        .start_incremental(
            &runtime,
            runtime
                .snapshot_current_source()
                .expect("borrow unsupported terminal block-quote target"),
            unsupported_quote_witness,
            binding,
            unsupported_quote_completion,
        )
        .expect("start unsupported terminal block-quote crop");
    let unsupported_quote_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(
        unsupported_quote_delivery.offer.mode,
        PublicationMode::ExactBaseDelta,
        "the definitive Green parser can adopt a nested quote without a whole-document fallback"
    );
    assert_eq!(
        unsupported_quote_delivery.offer.base_ack,
        Some(terminal_quote_delivery.ack)
    );
    assert!(unsupported_quote_delivery
        .packet_frames
        .iter()
        .flatten()
        .any(|(kind, records)| {
            *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage && *records > 0
        }));
    assert!(unsupported_quote_delivery
        .packet_frames
        .iter()
        .flatten()
        .all(|(kind, _)| *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage));
    let nested_content = terminal_nested_quote_edit + 1;
    let (owner_kind, _, ancestry) = recursive_green_query_shape(
        &host,
        unsupported_quote_source_version,
        nested_content,
        nested_content,
    );
    assert_eq!(owner_kind, 5);
    assert_eq!(ancestry.iter().filter(|kind| **kind == 2).count(), 2);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);

    let closing_fence = base_source
        .find("\n```\n\n")
        .map(|offset| offset + 1)
        .expect("closing fence");
    runtime
        .apply_edit(
            unsupported_quote_version,
            closing_fence..closing_fence + 3,
            "",
        )
        .expect("make the middle fence consume its former convergence suffix");
    let divergent_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan divergent fence SourceFacts replacement");
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &divergent_plan)
            .expect("preflight divergent fence crop"),
        "preflight may start from exact authority before semantics are known"
    );
    let divergent_witness = complete_incremental_source_facts(&mut runtime);
    let divergent_completion = completion_for_persistent_target(&runtime, 8, 7);
    host.observe_source_version(source_version_for(binding, divergent_completion))
        .expect("host observes divergent fence target");
    endpoint
        .start_incremental(
            &runtime,
            runtime
                .snapshot_current_source()
                .expect("borrow divergent fence target"),
            divergent_witness,
            binding,
            divergent_completion,
        )
        .expect("start divergent fence crop");
    let divergent_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(
        divergent_delivery.offer.mode,
        PublicationMode::ExactBaseDelta,
        "the definitive clean fallback may retain the exact base when canonical references match"
    );
    assert_eq!(
        divergent_delivery.offer.base_ack,
        Some(unsupported_quote_delivery.ack)
    );
    assert_eq!(
        endpoint.recursive_green_path_receipt(),
        RecursiveGreenPathReceipt {
            local_adoption_deliveries: 6,
            clean_fallback_deliveries: 1,
        },
        "the unclosed fence must still take the definitive clean escape hatch"
    );
    let divergent_source_version = source_version_for(binding, divergent_completion);
    let (owner_kind, _, ancestry) =
        recursive_green_query_shape(&host, divergent_source_version, fence_edit, fence_edit);
    assert_eq!(
        owner_kind, 7,
        "the unclosed fence must consume the former suffix"
    );
    assert_eq!(ancestry.first(), Some(&1));
    assert_eq!(ancestry.last(), Some(&7));
    let divergent_tail_point = base_source
        .rfind("terminal quote continuation")
        .expect("terminal suffix consumed by the unclosed fence")
        - 3;
    let (owner_kind, range, ancestry) = recursive_green_query_shape(
        &host,
        divergent_source_version,
        divergent_tail_point,
        divergent_tail_point,
    );
    assert_eq!(
        owner_kind, 7,
        "the standard 256-node budget must resolve the distant tail inside the unclosed fence"
    );
    assert!(range[0] as usize <= divergent_tail_point && divergent_tail_point < range[1] as usize);
    assert_eq!(ancestry, vec![1, 7]);
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn legacy_segmented_over_cap_edit_locally_adopts_recursive_green() {
    let profile = SourceFactsScanProfile::new(64).expect("bounded test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [716, 717, 718, 719],
        source_session_identity: 720,
        worker_generation: 1,
    };
    let base_source: String = (0..4_096)
        .map(|ordinal| format!("paragraph {ordinal:04} {}\n\n", "a".repeat(32)))
        .collect();
    let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
        .expect("segmented exact-base runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start segmented clean base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent segmented host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes segmented base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == base_version
    ));
    assert!(endpoint
        .recursive_green
        .has_installed_session_for(base_delivery.ack));

    let edit_start = base_source
        .find("paragraph 2048 ")
        .expect("middle segmented Paragraph")
        + "paragraph 2048 ".len()
        + 12;
    let oversized_replacement = "Z".repeat(M11_SEGMENTED_TOP_LEVEL_CROP_MAX_BYTES + 1_024);
    let target_version = runtime
        .apply_edit(
            base_version,
            edit_start..edit_start + 1,
            &oversized_replacement,
        )
        .expect("make the mapped restart window exceed its hard cap")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan segmented SourceFacts replacement");
    assert!(
        plan.base_byte_range().start > 0 && plan.base_byte_range().end < base_version.byte_len(),
        "fixture must retain packed block pages on both sides"
    );
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("preflight exact-clean fallback"),
        "the exact base remains eligible before target-window cap evaluation"
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("borrow segmented target");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    host.observe_source_version(source_version_for(binding, target_completion))
        .expect("host observes segmented target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start typed over-cap exact-clean fallback");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact",
        "the retired segmented byte cap must not force a whole-document clean parse"
    );
    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);

    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
    let replacement_pages = target_delivery
        .packet_frames
        .iter()
        .flatten()
        .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage)
        .map(|(_, records)| usize::try_from(*records).expect("record count"))
        .sum::<usize>();
    assert!(
        (1..16).contains(&replacement_pages),
        "one local edit should transfer only a boundary-local Green splice, got \
             {replacement_pages}"
    );
    assert!(target_delivery
        .packet_frames
        .iter()
        .flatten()
        .all(|(kind, _)| *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage));
    assert!(
        target_delivery.offer.transferred_record_count
            < target_delivery.offer.target_record_count / 4,
        "exact target must omit the large retained block/source-fact majority"
    );
    let target_source_version = source_version_for(binding, target_completion);
    let (owner_kind, range, ancestry) =
        recursive_green_query_shape(&host, target_source_version, edit_start, edit_start);
    assert_eq!(owner_kind, 5);
    assert!(range[0] as usize <= edit_start && edit_start < range[1] as usize);
    assert!(!ancestry.is_empty());
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, target_version)
        .expect("segmented target exact-base continuity"));
    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == target_version
    ));
    assert_eq!(
        endpoint.recursive_green_path_receipt(),
        RecursiveGreenPathReceipt {
            local_adoption_deliveries: 1,
            clean_fallback_deliveries: 0,
        }
    );

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn tight_bullet_item_edit_uses_authenticated_block_delta() {
    let profile = SourceFactsScanProfile::new(64).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [716, 717, 718, 719],
        source_session_identity: 720,
        worker_generation: 1,
    };
    let mut base_source = String::new();
    for ordinal in 0..512 {
        use std::fmt::Write as _;
        writeln!(
            &mut base_source,
            "paragraph {ordinal:04} {}\n",
            "a".repeat(32)
        )
        .expect("paragraph fixture write");
        if ordinal == 255 {
            base_source.push_str("  - α😀 first\r\n  - beta second\r\n\r\n");
        }
    }
    let edit_start = base_source
        .find("beta second")
        .expect("selected Bullet List item");
    let edit_start_utf16 = base_source[..edit_start].encode_utf16().count();
    let mut target_source = base_source.clone();
    target_source.replace_range(edit_start..edit_start + 1, "β");

    let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
        .expect("Bullet List exact-delta runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start clean Bullet List base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent Bullet List host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes Bullet List base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

    let target_version = runtime
        .apply_edit(base_version, edit_start..edit_start + 1, "β")
        .expect("Unicode Bullet List item edit")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan Bullet List SourceFacts replacement");
    assert_eq!(
        plan.exact_parser_base_byte_range(),
        Some(&(edit_start..edit_start + 1)),
    );
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("preflight Bullet List crop"),
        "ordinary Paragraph checkpoints must bracket the changed list"
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("borrow Bullet List target");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    let target_source_version = source_version_for(binding, target_completion);
    host.observe_source_version(target_source_version)
        .expect("host observes Bullet List target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start authenticated Bullet List crop");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact",
    );
    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);

    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
    assert!(
        target_delivery.offer.transferred_record_count <= 64,
        "one list-item edit transferred {} records",
        target_delivery.offer.transferred_record_count,
    );
    assert!(
        target_delivery.offer.transferred_record_count < target_delivery.offer.target_record_count,
        "the target must retain document records outside the changed list"
    );
    let recursive_green_replacement_records = target_delivery
        .packet_frames
        .iter()
        .flatten()
        .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage)
        .map(|(_, records)| *records)
        .sum::<u32>();
    assert!(
        recursive_green_replacement_records > 0 && recursive_green_replacement_records <= 64,
        "one list-item edit must publish one bounded recursive-Green splice"
    );
    assert!(target_delivery
        .packet_frames
        .iter()
        .flatten()
        .all(|(kind, _)| *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage));

    let selected_end = target_source[edit_start..]
        .find("\r\n")
        .map(|offset| edit_start + offset)
        .expect("selected item line ending");
    let (owner_kind, range, ancestry) =
        recursive_green_query_shape(&host, target_source_version, edit_start, edit_start_utf16);
    assert_eq!(
        owner_kind, 5,
        "the edited list row remains a Green Paragraph"
    );
    assert_eq!(range[0] as usize, edit_start);
    assert_eq!(range[1] as usize, selected_end);
    assert_eq!(range[2] as usize, edit_start_utf16);
    assert_eq!(
        range[3] as usize,
        target_source[..selected_end].encode_utf16().count()
    );
    assert_eq!(ancestry, vec![1, 3, 4, 5]);

    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, target_version)
        .expect("Bullet List target exact-base continuity"));
    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == target_version
    ));

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn ordinary_paragraph_eof_edit_and_semantic_split_use_local_adoption() {
    let profile = SourceFactsScanProfile::new(2).expect("dense test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [721, 722, 723, 724],
        source_session_identity: 725,
        worker_generation: 1,
    };
    let base_source: String = (0..1_024)
        .map(|ordinal| format!("ordinary prose line {ordinal:04} {}\n", "a".repeat(40)))
        .collect();
    let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
        .expect("ordinary EOF runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start clean ordinary base candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent candidate host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes ordinary base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

    let edit_start = base_source
        .find("ordinary prose line 1018 ")
        .expect("tail line");
    let replacement: String = (0..10)
        .map(|ordinal| format!("replacement tail {ordinal:02} 世界😀 {}\n", "b".repeat(40)))
        .collect();
    let first_target_source = format!("{}{}", &base_source[..edit_start], replacement);
    let target_version = runtime
        .apply_edit(base_version, edit_start..base_source.len(), &replacement)
        .expect("EOF ordinary edit")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan EOF SourceFacts replacement");
    assert!(
        plan.base_byte_range().start > 0 && plan.base_byte_range().end == base_version.byte_len(),
        "fixture must leave only exact parser prefix authority"
    );
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("preflight EOF crop"),
        "EOF edit must have an authenticated restart"
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("borrow exact EOF target");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    host.observe_source_version(source_version_for(binding, target_completion))
        .expect("host observes exact EOF target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start authenticated EOF crop");
    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);

    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
    assert!(
        target_delivery.offer.transferred_record_count < target_delivery.offer.target_record_count,
        "exact EOF delta must omit authenticated reused records"
    );
    assert_installed_candidate_has_no_inline(&host, source_version_for(binding, target_completion));
    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == target_version
    ));
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, target_version)
        .expect("next EOF exact base"));

    let second_paragraph_start = first_target_source.len() + 1;
    let second_paragraph_start_utf16 = first_target_source.encode_utf16().count() + 1;
    let split_target_version = runtime
        .apply_edit(
            target_version,
            first_target_source.len()..first_target_source.len(),
            "\nsecond paragraph\n",
        )
        .expect("turn the EOF Paragraph into a segmented target")
        .source()
        .current();
    let split_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan EOF semantic split");
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &split_plan)
            .expect("preflight EOF semantic split"),
        "the semantic split must initially select the authenticated EOF crop"
    );
    let split_witness = complete_incremental_source_facts(&mut runtime);
    let split_lease = runtime
        .snapshot_current_source()
        .expect("borrow segmented EOF target");
    let split_completion = completion_for_persistent_target(&runtime, 3, 2);
    let split_source_version = source_version_for(binding, split_completion);
    host.observe_source_version(split_source_version)
        .expect("host observes segmented EOF target");
    endpoint
        .start_incremental(
            &runtime,
            split_lease,
            split_witness,
            binding,
            split_completion,
        )
        .expect("start EOF semantic-split adoption");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact"
    );
    let split_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);

    assert_eq!(split_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(split_delivery.offer.base_ack, Some(target_delivery.ack));
    assert_eq!(split_delivery.ack.source_version, split_source_version);
    assert!(
        split_delivery.offer.transferred_record_count < split_delivery.offer.target_record_count,
        "the semantic split must retain exact records outside its terminal Green splice"
    );
    assert!(split_delivery
        .packet_frames
        .iter()
        .flatten()
        .any(|(kind, _)| *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage));
    assert!(split_delivery
        .packet_frames
        .iter()
        .flatten()
        .all(|(kind, _)| *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage));
    let (owner_kind, range, ancestry) = recursive_green_query_shape(
        &host,
        split_source_version,
        second_paragraph_start + 1,
        second_paragraph_start_utf16 + 1,
    );
    assert_eq!(owner_kind, 5, "the appended block is a Green Paragraph");
    assert_eq!(range[0] as usize, second_paragraph_start);
    assert_eq!(
        range[2] as usize, second_paragraph_start_utf16,
        "the appended Paragraph must retain exact UTF-16 coordinates"
    );
    assert!(!ancestry.is_empty());
    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == split_target_version
    ));
    assert_eq!(
        endpoint.recursive_green_path_receipt(),
        RecursiveGreenPathReceipt {
            local_adoption_deliveries: 2,
            clean_fallback_deliveries: 0,
        },
        "both EOF edits must commit through bounded local Green adoption",
    );
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, split_target_version)
        .expect("segmented EOF target exact-base continuity"));

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn ordinary_crop_blank_boundary_uses_bounded_recursive_green_local_adoption() {
    const MAXIMUM_REPLACEMENT_RECORDS: u32 = 64;
    let profile = SourceFactsScanProfile::new(2).expect("dense test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [731, 732, 733, 734],
        source_session_identity: 735,
        worker_generation: 1,
    };
    let base_source: String = (0..1_024)
        .map(|ordinal| format!("ordinary prose line {ordinal:04} {}\n", "a".repeat(40)))
        .collect();
    let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
        .expect("ordinary local-adoption runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start clean ordinary base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent local-adoption host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes ordinary base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);

    let blank_start = base_source
        .find("ordinary prose line 0512 ")
        .expect("middle ordinary line");
    let blank_end = base_source[blank_start..]
        .find('\n')
        .map(|offset| blank_start + offset)
        .expect("middle line ending");
    let target_version = runtime
        .apply_edit(base_version, blank_start..blank_end, "")
        .expect("insert semantic blank boundary")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan blank-boundary SourceFacts");
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("preflight ordinary crop"),
        "fixture must initially select the bounded ordinary crop"
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("borrow blank-boundary target");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    let target_wire_source = source_version_for(binding, target_completion);
    host.observe_source_version(target_wire_source)
        .expect("host observes blank-boundary target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start bounded blank-boundary adoption");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact"
    );
    while endpoint.recursive_green.target_work_pending() {
        assert!(matches!(
            endpoint
                .poll(&mut runtime, 1)
                .expect("advance blank-boundary Green adoption"),
            CandidatePoll::Pending { transitions: 1 }
        ));
    }
    assert!(endpoint
        .recursive_green
        .ready_update_for(base_delivery.ack, target_version)
        .is_some());
    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);

    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
    assert_eq!(target_delivery.ack.source_version, target_wire_source);
    assert!(
        target_delivery.offer.transferred_record_count < target_delivery.offer.target_record_count,
        "local adoption must retain unchanged exact-base records"
    );
    let recursive_green_replacement_records = target_delivery
        .packet_frames
        .iter()
        .flatten()
        .filter(|(kind, _)| *kind == CandidateSnapshotFrameKind::RecursiveGreenReplacementPage)
        .map(|(_, records)| *records)
        .sum::<u32>();
    assert!(
        recursive_green_replacement_records > 0
            && recursive_green_replacement_records <= MAXIMUM_REPLACEMENT_RECORDS,
        "local adoption must publish a bounded nonempty recursive-Green replacement"
    );
    assert!(
        target_delivery
            .packet_frames
            .iter()
            .flatten()
            .all(|(kind, _)| *kind != CandidateSnapshotFrameKind::BlockSequenceReplacementPage),
        "recursive-Green local adoption must not revive legacy block replacement"
    );
    assert_eq!(
        endpoint.recursive_green_path_receipt(),
        RecursiveGreenPathReceipt {
            local_adoption_deliveries: 1,
            clean_fallback_deliveries: 0,
        }
    );
    let retained = endpoint
        .retained
        .as_ref()
        .expect("retained local-adoption target");
    assert_eq!(
        retained
            .publication
            .descriptor(&runtime)
            .expect("local-adoption descriptor")
            .source_revision,
        target_version.revision().get()
    );
    assert!(matches!(
        retained.restart.as_ref(),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == target_version
    ));
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(
        endpoint
            .has_exact_base_for(&runtime, target_version)
            .expect("segmented target eligibility"),
        "the segmented local-adoption target remains eligible for exact-base discovery"
    );

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn leading_crop_new_definition_falls_back_with_fresh_references() {
    const BASE_SOURCE: &str = "[base]: /base\n!x]: /new\nvisible\n";

    let profile = SourceFactsScanProfile::new(2).expect("dense test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [741, 742, 743, 744],
        source_session_identity: 745,
        worker_generation: 1,
    };
    let mut runtime = DocumentRuntime::new(BASE_SOURCE, standard_document_runtime_config())
        .expect("leading fallback runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start clean leading base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent leading fallback host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes leading base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
    assert_eq!(
        host.role_record_count(flark_engine::m11_host::M11HostRole::References)
            .expect("base References"),
        1
    );

    let edit_start = BASE_SOURCE.find('!').expect("definition edit marker");
    let target_version = runtime
        .apply_edit(base_version, edit_start..edit_start + 1, "[")
        .expect("turn paragraph line into a new definition")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan new-definition SourceFacts");
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("preflight leading crop"),
        "fixture must initially select the retained leading restart"
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("borrow new-definition target");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    host.observe_source_version(source_version_for(binding, target_completion))
        .expect("host observes new-definition target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start leading crop before semantic decline");
    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);

    assert_eq!(target_delivery.offer.mode, PublicationMode::FullSnapshot);
    assert_eq!(target_delivery.offer.base_ack, None);
    assert_eq!(
        target_delivery.offer.transferred_record_count,
        target_delivery.offer.target_record_count
    );
    assert!(target_delivery
        .packet_frames
        .iter()
        .flatten()
        .all(|(kind, _)| *kind != CandidateSnapshotFrameKind::SourceFactsReplacementPage));
    assert_eq!(
        host.role_record_count(flark_engine::m11_host::M11HostRole::References)
            .expect("fresh target References"),
        2,
        "fallback must rebuild References from the definitive target parse"
    );
    let retained = endpoint.retained.as_ref().expect("retained target base");
    let CandidateRestartAuthority::Leading(restart) =
        retained.restart.as_ref().expect("supported target restart")
    else {
        panic!("fresh target must retain leading-reference authority");
    };
    assert_eq!(restart.source(), target_version);
    assert_eq!(
        restart.definition_count(),
        2,
        "fallback must install fresh target checkpoint semantics"
    );
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, target_version)
        .expect("next target exact base"));

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn length_changing_edit_before_late_definition_rebuilds_reference_coordinates() {
    const PARAGRAPHS: usize = 8_192;

    let mut base_source = String::new();
    for ordinal in 0..PARAGRAPHS {
        use std::fmt::Write as _;
        writeln!(
            &mut base_source,
            "Paragraph {ordinal:04} stays definition free.\n"
        )
        .expect("late-definition fixture write");
    }
    base_source.push_str("[late]: /target\n");
    let last_paragraph_start = base_source
        .find("Paragraph 8191")
        .expect("last Paragraph start");
    let last_paragraph_end = base_source[last_paragraph_start..]
        .find("\n\n")
        .map(|offset| last_paragraph_start + offset + 1)
        .expect("last Paragraph end");

    let profile = SourceFactsScanProfile::new(64).expect("dense coordinate-shift profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [746, 747, 748, 749],
        source_session_identity: 750,
        worker_generation: 1,
    };
    let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
        .expect("late-definition runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start late-definition base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent late-definition host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes late-definition base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
    assert_eq!(
        host.role_record_count(flark_engine::m11_host::M11HostRole::References)
            .expect("base References"),
        1
    );
    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == base_version
    ));

    let edit_start = base_source
        .find("Paragraph 0100")
        .expect("early Paragraph edit");
    let equal_length_version = runtime
        .apply_edit(base_version, edit_start..edit_start + 1, "p")
        .expect("equal-length early edit")
        .source()
        .current();
    let equal_length_plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan equal-length SourceFacts");
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &equal_length_plan)
            .expect("preflight equal-length exact base"),
        "the fixture must exercise exact-base clean discovery"
    );
    let equal_length_witness = complete_incremental_source_facts(&mut runtime);
    let equal_length_lease = runtime
        .snapshot_current_source()
        .expect("borrow equal-length target");
    let equal_length_completion = completion_for_persistent_target(&runtime, 2, 1);
    host.observe_source_version(source_version_for(binding, equal_length_completion))
        .expect("host observes equal-length target");
    endpoint
        .start_incremental(
            &runtime,
            equal_length_lease,
            equal_length_witness,
            binding,
            equal_length_completion,
        )
        .expect("start equal-length exact-base edit");
    let equal_length_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(
        equal_length_delivery.offer.mode,
        PublicationMode::ExactBaseDelta,
        "unchanged absolute coordinates remain eligible for reference-root reuse"
    );
    assert_eq!(
        equal_length_delivery.offer.base_ack,
        Some(base_delivery.ack)
    );
    assert!(
        equal_length_delivery.offer.transferred_record_count
            < equal_length_delivery.offer.target_record_count
    );
    assert_eq!(
        host.role_record_count(flark_engine::m11_host::M11HostRole::References)
            .expect("reused equal-length References"),
        1
    );
    assert_eq!(
        endpoint.recursive_green_path_receipt(),
        RecursiveGreenPathReceipt {
            local_adoption_deliveries: 0,
            clean_fallback_deliveries: 1,
        },
        "an edit before reference coverage must take the definitive clean path even when its canonical reference coordinates remain equal",
    );
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, equal_length_version)
        .expect("equal-length exact-base continuity"));

    let target_version = runtime
        .apply_edit(
            equal_length_version,
            edit_start..edit_start + 1,
            "Expanded p",
        )
        .expect("length-changing early edit")
        .source()
        .current();
    let coordinate_delta = "Expanded p".len() - 1;
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan late-definition SourceFacts");
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("preflight exact base"),
        "the fixture must exercise exact-base clean discovery"
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("borrow late-definition target");
    let target_completion = completion_for_persistent_target(&runtime, 3, 2);
    host.observe_source_version(source_version_for(binding, target_completion))
        .expect("host observes late-definition target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start exact-base late-definition edit");
    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);

    assert_eq!(
        target_delivery.offer.mode,
        PublicationMode::FullSnapshot,
        "a reused reference record would retain its old absolute byte/UTF-16 ranges"
    );
    assert_eq!(target_delivery.offer.base_ack, None);
    assert_eq!(
        target_delivery.offer.transferred_record_count,
        target_delivery.offer.target_record_count
    );
    assert_eq!(
        host.role_record_count(flark_engine::m11_host::M11HostRole::References)
            .expect("rebuilt target References"),
        1
    );
    let target_source_version = source_version_for(binding, target_completion);
    let target_last_paragraph_start = last_paragraph_start + coordinate_delta;
    let target_last_paragraph_end = last_paragraph_end + coordinate_delta;
    let (owner_kind, range, ancestry) = recursive_green_query_shape(
        &host,
        target_source_version,
        target_last_paragraph_start + 1,
        target_last_paragraph_start + 1,
    );
    assert_eq!(owner_kind, 5, "the shifted tail remains a Green Paragraph");
    assert_eq!(range[0] as usize, target_last_paragraph_start);
    assert_eq!(range[2] as usize, target_last_paragraph_start);
    assert_eq!(range[1] as usize, target_last_paragraph_end - 1);
    assert_eq!(range[3] as usize, target_last_paragraph_end - 1);
    assert!(!ancestry.is_empty());
    assert_eq!(
        endpoint.recursive_green_path_receipt(),
        RecursiveGreenPathReceipt {
            local_adoption_deliveries: 0,
            clean_fallback_deliveries: 2,
        },
        "the length-changing edit must rebuild shifted reference coordinates through the clean escape hatch",
    );
    assert_eq!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref())
            .expect("target exact-base authority")
            .source(),
        target_version
    );

    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, target_version)
        .expect("rebuilt target exact-base continuity"));
    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

fn leading_references_use_the_production_exact_delta_path<
    const REFERENCES: usize,
    const FUEL: usize,
>() {
    let mut base_source = String::new();
    base_source.reserve(REFERENCES * 24);
    for ordinal in 0..REFERENCES {
        use std::fmt::Write as _;
        writeln!(&mut base_source, "[ref-{ordinal}]: /target-{ordinal}")
            .expect("reference fixture write");
    }
    let tail_start = base_source.len();
    base_source.push_str("live **tail** stays editable\n");

    let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [751, 752, 753, 754],
        source_session_identity: 755,
        worker_generation: 1,
    };
    let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
        .expect("large-reference exact-delta runtime");

    let cold_started = std::time::Instant::now();
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start large-reference base candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent large-reference host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes large-reference base");
    let base_delivery = deliver_endpoint_to_independent_host_with_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        FUEL,
    );
    let cold_elapsed = cold_started.elapsed();
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
    assert_eq!(
        host.role_record_count(flark_engine::m11_host::M11HostRole::References)
            .expect("base References"),
        REFERENCES as u64
    );
    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == base_version
    ));
    assert!(endpoint
        .recursive_green
        .has_installed_session_for(base_delivery.ack));

    let edit_start = tail_start
        + base_source[tail_start..]
            .find("tail")
            .expect("editable tail");
    let target_version = runtime
        .apply_edit(base_version, edit_start..edit_start + 1, "T")
        .expect("bounded tail edit")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan large-reference SourceFacts delta");
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("preflight large-reference crop"),
        "the unchanged definition prefix must select exact crop authority"
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("borrow large-reference target");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    host.observe_source_version(source_version_for(binding, target_completion))
        .expect("host observes large-reference target");

    let exact_started = std::time::Instant::now();
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start production large-reference exact crop");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact",
        "the exact tail edit must wait on bounded Green adoption"
    );
    let target_delivery = deliver_endpoint_to_independent_host_with_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        FUEL,
    );
    let exact_elapsed = exact_started.elapsed();

    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
    let reused_records = target_delivery
        .offer
        .target_record_count
        .checked_sub(target_delivery.offer.transferred_record_count)
        .expect("exact delta cannot transfer more than its target");
    assert!(
        reused_records >= REFERENCES as u32,
        "all canonical reference records must come from the acknowledged base"
    );
    assert_eq!(
        host.role_record_count(flark_engine::m11_host::M11HostRole::References)
            .expect("target References"),
        REFERENCES as u64
    );
    assert_eq!(
        endpoint.recursive_green_path_receipt(),
        RecursiveGreenPathReceipt {
            local_adoption_deliveries: 1,
            clean_fallback_deliveries: 0,
        },
        "the large reference prefix must stay on the bounded recursive-Green adoption path",
    );
    assert_eq!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref())
            .expect("next large-reference restart")
            .source(),
        target_version
    );
    eprintln!(
        "m11_{REFERENCES}_reference_exact_delta source_bytes={} cold_ms={} exact_ms={} \
             base_records={} target_records={} transferred_records={} reused_records={} \
             base_packets={} exact_packets={} mode={:?}",
        base_source.len(),
        cold_elapsed.as_millis(),
        exact_elapsed.as_millis(),
        base_delivery.offer.target_record_count,
        target_delivery.offer.target_record_count,
        target_delivery.offer.transferred_record_count,
        reused_records,
        base_delivery.packet_frames.len(),
        target_delivery.packet_frames.len(),
        target_delivery.offer.mode,
    );

    drain_candidate_cleanup_with_fuel(&mut endpoint, &mut runtime, FUEL);
    assert!(endpoint
        .has_exact_base_for(&runtime, target_version)
        .expect("next large-reference exact base"));
    close_exact_pair_to_zero_with_fuel(&mut endpoint, &mut runtime, &mut host, FUEL);
}

#[test]
fn four_thousand_ninety_six_leading_references_use_the_production_exact_delta_path() {
    leading_references_use_the_production_exact_delta_path::<4_096, 1>();
}

#[test]
#[ignore = "large-scale release benchmark; the 4,096-reference case remains in the default suite"]
fn one_hundred_thousand_leading_references_use_the_production_exact_delta_path() {
    leading_references_use_the_production_exact_delta_path::<100_000, 64>();
}

#[test]
fn frozen_leading_references_allow_bounded_middle_paragraph_exact_delta() {
    const REFERENCES: usize = 2_048;
    const PARAGRAPHS: usize = 2_048;
    const EDITED_PARAGRAPH: usize = PARAGRAPHS / 2;
    const MAXIMUM_TRANSFERRED_RECORDS: u32 = 64;
    const FUEL: usize = 64;

    let mut base_source = String::new();
    base_source.reserve((REFERENCES + PARAGRAPHS) * 56);
    for ordinal in 0..REFERENCES {
        use std::fmt::Write as _;
        writeln!(&mut base_source, "[ref-{ordinal}]: /target-{ordinal}")
            .expect("reference fixture write");
    }
    let tail_start = base_source.len();
    let mut paragraph_ranges = Vec::with_capacity(PARAGRAPHS);
    for ordinal in 0..PARAGRAPHS {
        let start = base_source.len();
        use std::fmt::Write as _;
        writeln!(
            &mut base_source,
            "tail paragraph {ordinal:04} {}\n",
            "a".repeat(32)
        )
        .expect("Paragraph fixture write");
        let end = base_source.len() - 1;
        paragraph_ranges.push(start..end);
    }
    assert_eq!(
        paragraph_ranges.first().expect("first tail").start,
        tail_start
    );

    let profile = SourceFactsScanProfile::new(4_096).expect("production scan profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [761, 762, 763, 764],
        source_session_identity: 765,
        worker_generation: 1,
    };
    let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
        .expect("reference-frozen Paragraph runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start reference-frozen base");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent reference-frozen host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes reference-frozen base");
    let base_delivery = deliver_endpoint_to_independent_host_with_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        FUEL,
    );
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
    assert_eq!(
        host.role_record_count(flark_engine::m11_host::M11HostRole::References)
            .expect("base References"),
        REFERENCES as u64
    );
    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == base_version
    ));
    assert!(endpoint
        .recursive_green
        .has_installed_session_for(base_delivery.ack));

    let edited_range = paragraph_ranges[EDITED_PARAGRAPH].clone();
    let edit_start = edited_range.start
        + base_source[edited_range.clone()]
            .find("aaaaaaaa")
            .expect("editable Paragraph payload")
        + 4;
    const REPLACEMENT: &str = "expanded";
    let coordinate_delta = REPLACEMENT.len() - 1;
    let target_version = runtime
        .apply_edit(base_version, edit_start..edit_start + 1, REPLACEMENT)
        .expect("length-changing middle tail edit")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan reference-frozen SourceFacts delta");
    assert_eq!(
        plan.exact_parser_base_byte_range(),
        Some(&(edit_start..edit_start + 1)),
        "ordinary parser authority must follow the exact edit, not a storage-page envelope"
    );
    assert!(
        endpoint
            .has_incremental_base_for_plan(&runtime, &plan)
            .expect("preflight reference-frozen exact crop"),
        "an unchanged definition prefix must retain ordinary restart and convergence authority"
    );
    let witness = complete_incremental_source_facts(&mut runtime);
    let target_lease = runtime
        .snapshot_current_source()
        .expect("borrow reference-frozen target");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    let target_source_version = source_version_for(binding, target_completion);
    host.observe_source_version(target_source_version)
        .expect("host observes reference-frozen target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start reference-frozen ordinary crop");
    assert_eq!(
        active_candidate_phase(endpoint.active.as_ref()),
        "AwaitingRecursiveGreenExact",
        "a middle tail Paragraph edit must wait on bounded Green adoption"
    );
    let target_delivery = deliver_endpoint_to_independent_host_with_fuel(
        &mut endpoint,
        &mut runtime,
        &mut host,
        FUEL,
    );

    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
    assert_eq!(target_delivery.ack.source_version, target_source_version);
    assert!(
        target_delivery.offer.transferred_record_count <= MAXIMUM_TRANSFERRED_RECORDS,
        "one local Paragraph edit transferred {} records across {REFERENCES} frozen \
             definitions and {PARAGRAPHS} Paragraphs",
        target_delivery.offer.transferred_record_count
    );
    let reused_records = target_delivery
        .offer
        .target_record_count
        .checked_sub(target_delivery.offer.transferred_record_count)
        .expect("exact delta cannot transfer more than its target");
    assert!(
        reused_records >= REFERENCES as u32,
        "the acknowledged base must supply every frozen reference record"
    );
    assert_eq!(
        host.role_record_count(flark_engine::m11_host::M11HostRole::References)
            .expect("retained target References"),
        REFERENCES as u64
    );

    let edited_target_range = edited_range.start..edited_range.end + coordinate_delta - 1;
    let last_base_range = paragraph_ranges[PARAGRAPHS - 1].clone();
    let last_target_range =
        last_base_range.start + coordinate_delta..last_base_range.end + coordinate_delta - 1;
    for (name, paragraph_range, point) in [
        (
            "first tail Paragraph",
            paragraph_ranges[0].start..paragraph_ranges[0].end - 1,
            paragraph_ranges[0].start + 1,
        ),
        (
            "edited middle tail Paragraph",
            edited_target_range,
            edit_start + 1,
        ),
        (
            "last tail Paragraph",
            last_target_range.clone(),
            last_target_range.start + 1,
        ),
    ] {
        let (owner_kind, range, ancestry) =
            recursive_green_query_shape(&host, target_source_version, point, point);
        assert_eq!(owner_kind, 5, "{name} remains a Green Paragraph");
        assert_eq!(range[0] as usize, paragraph_range.start, "{name}");
        assert_eq!(range[2] as usize, paragraph_range.start, "{name}");
        assert_eq!(range[1] as usize, paragraph_range.end, "{name}");
        assert_eq!(range[3] as usize, paragraph_range.end, "{name}");
        assert!(
            !ancestry.is_empty(),
            "{name} retains authenticated ancestry"
        );
    }

    drain_candidate_cleanup_with_fuel(&mut endpoint, &mut runtime, FUEL);
    assert!(
        endpoint
            .has_exact_base_for(&runtime, target_version)
            .expect("next-edit exact-base authority"),
        "the installed target must remain eligible for the next edit"
    );
    assert!(matches!(
        endpoint
            .retained
            .as_ref()
            .and_then(|retained| retained.restart.as_ref()),
        Some(CandidateRestartAuthority::RecursiveGreen { source, .. })
            if *source == target_version
    ));

    close_exact_pair_to_zero_with_fuel(&mut endpoint, &mut runtime, &mut host, FUEL);
}

#[test]
fn exact_base_delta_round_trips_at_the_sixteen_frame_replay_boundary() {
    // The crop grammar intentionally requires the visible remainder to
    // follow the leading definitions directly. A blank separator is an
    // explicit `BlankBoundary` and therefore cannot mint a restart.
    const PREFIX: &str = "[ref]: /target\n";
    const TAIL_BYTES: usize = 1_864;

    let profile = SourceFactsScanProfile::new(2).expect("dense test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let binding = SessionBinding {
        document_session: [701, 702, 703, 704],
        source_session_identity: 705,
        worker_generation: 1,
    };
    let base_source = format!("{PREFIX}{}", "a".repeat(TAIL_BYTES));
    let mut runtime = DocumentRuntime::new(&base_source, standard_document_runtime_config())
        .expect("exact-delta runtime");
    let (certified, base_completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let base_version = certified.source();
    let mut endpoint = CandidateEndpoint::new();
    endpoint
        .start(certified, binding, base_completion)
        .expect("start clean base candidate");
    let mut host = NativeCandidateHost::new(HostConfig {
        document_session: binding.document_session,
        grammar_revision: GRAMMAR_REVISION,
        syntax_profile: 1,
        authority_mask: AUTHORITY_MASK_ALL_ROLES,
        maximum_query_bytes: 64 * 1024,
    })
    .expect("independent candidate host");
    host.observe_source_version(source_version_for(binding, base_completion))
        .expect("host observes exact base");
    let base_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);
    assert_eq!(base_delivery.offer.mode, PublicationMode::FullSnapshot);
    assert!(endpoint
        .has_exact_base_for(&runtime, base_version)
        .expect("inspect retained base"));

    let target_source = format!("{PREFIX}{}", "b".repeat(TAIL_BYTES));
    let target_version = runtime
        .apply_edit(
            base_version,
            PREFIX.len()..base_source.len(),
            &target_source[PREFIX.len()..],
        )
        .expect("replace paragraph tail")
        .source()
        .current();
    let plan = runtime
        .begin_incremental_source_facts(profile, parser_profile, SourceFactsRootLimits::default())
        .expect("plan incremental SourceFacts");
    assert_eq!(plan.source(), target_version);
    let witness = complete_incremental_source_facts(&mut runtime);
    assert_eq!(
        witness.target_page_range().end - witness.target_page_range().start,
        15,
        "fixture must put E4 plus fifteen E5 pages on the 16-frame boundary"
    );
    let target_lease = runtime
        .snapshot_current_source()
        .expect("borrow exact target source");
    let target_completion = completion_for_persistent_target(&runtime, 2, 1);
    host.observe_source_version(source_version_for(binding, target_completion))
        .expect("host observes exact target");
    endpoint
        .start_incremental(&runtime, target_lease, witness, binding, target_completion)
        .expect("start exact-base candidate");
    let target_delivery =
        deliver_endpoint_to_independent_host_with_unit_fuel(&mut endpoint, &mut runtime, &mut host);

    assert_eq!(target_delivery.offer.mode, PublicationMode::ExactBaseDelta);
    assert_eq!(target_delivery.offer.base_ack, Some(base_delivery.ack));
    assert!(
        target_delivery.offer.transferred_record_count < target_delivery.offer.target_record_count,
        "the exact delta must omit authenticated reused records"
    );
    let boundary_packet = target_delivery
        .packet_frames
        .first()
        .expect("exact delta packet");
    assert_eq!(boundary_packet.len(), 16);
    assert_eq!(boundary_packet[0].0, CandidateSnapshotFrameKind::Begin);
    assert!(boundary_packet[1..]
        .iter()
        .all(|(kind, _)| *kind == CandidateSnapshotFrameKind::SourceFactsReplacementPage));
    assert!(
        target_delivery
            .packet_frames
            .iter()
            .skip(1)
            .flatten()
            .any(|(kind, _)| *kind == CandidateSnapshotFrameKind::Node),
        "producer must resume after the full replacement packet receives credit"
    );

    let retained = endpoint.retained.as_ref().expect("retained exact target");
    let descriptor = retained
        .publication
        .descriptor(&runtime)
        .expect("target descriptor");
    assert_eq!(
        u64::from(target_delivery.ack.record_count),
        descriptor.canonical_record_count
    );
    assert_eq!(
        target_delivery.ack.publication_session,
        digest_words(descriptor.publication)
    );
    assert_eq!(
        target_delivery.ack.source_root,
        split_u64(descriptor.source_root)
    );
    assert_eq!(
        target_delivery.ack.source_version,
        source_version_for(binding, target_completion)
    );
    assert_eq!(
        host.role_record_count(flark_engine::m11_host::M11HostRole::References)
            .expect("installed References"),
        1
    );
    assert_eq!(
        host.role_record_count(flark_engine::m11_host::M11HostRole::SourceFacts)
            .expect("installed SourceFacts"),
        runtime
            .persistent_source_facts()
            .expect("target persistent SourceFacts")
            .page_count()
    );
    assert_eq!(
        retained
            .restart
            .as_ref()
            .expect("next-revision parser restart")
            .source(),
        target_version
    );
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    assert!(endpoint
        .has_exact_base_for(&runtime, target_version)
        .expect("next-revision exact base"));

    close_exact_pair_to_zero(&mut endpoint, &mut runtime, &mut host);
}

#[test]
fn one_acknowledged_base_survives_rejection_replaces_without_a_chain_and_closes() {
    let mut fixture = OrdinaryCancellationFixture::new([741, 742, 743, 744]);
    drain_candidate_cleanup(&mut fixture.endpoint, &mut fixture.runtime);
    while !fixture.runtime.poll_retirement(256).complete {}

    assert!(
        fixture
            .endpoint
            .recursive_green
            .has_installed_session_for(fixture.base_ack),
        "the acknowledged base must own a real recursive-Green session"
    );
    let acknowledged_base_resident_nodes = fixture.runtime.arena_metrics().resident_nodes;
    let acknowledged_base_owned_nodes = acknowledged_base_resident_nodes
        .checked_sub(fixture.initial_persistent_resident_nodes)
        .expect("acknowledged base adds parser-owned residency");
    assert!(acknowledged_base_owned_nodes > 0);

    let rejected_edit = fixture.edit_offset(512);
    fixture.start_target(rejected_edit, "Z", 2, 1);
    assert!(fixture.endpoint.recursive_green.target_work_pending());
    assert!(
        fixture
            .endpoint
            .recursive_green
            .owns_recursive_base_authority(fixture.base_ack),
        "the pending Green update must retain restoration authority for the acknowledged base"
    );

    let mut saw_packet = false;
    for event_id in 1..1_000_000_u32 {
        match fixture
            .endpoint
            .poll(&mut fixture.runtime, 1)
            .expect("advance exact target to packet streaming")
        {
            CandidatePoll::Pending { transitions } => assert_eq!(transitions, 1),
            CandidatePoll::Event { transitions, event } => {
                assert!(transitions <= 1);
                let CandidateEvent { credit, body } = *event;
                match body {
                    CandidateEventBody::Begin(_) => fixture
                        .endpoint
                        .accept_credit(credit, event_id)
                        .expect("accept rejected target Begin credit"),
                    CandidateEventBody::Packet { encoded } => {
                        let packet = decode_publication_packet(&encoded)
                            .expect("decode rejected recursive-Green packet");
                        assert!(packet.frame_count > 0);
                        let offer_id = packet.offer_id;
                        fixture
                            .endpoint
                            .accept_credit(credit, event_id)
                            .expect("accept rejected target Packet credit");
                        assert!(fixture
                            .endpoint
                            .handle_host_poll(
                                event_id,
                                offer_id,
                                HostPollPhase::PacketCredit,
                                HostPollResult::Rejected(
                                    crate::v3_publication_wire::HostRejectReason::Superseded,
                                ),
                            )
                            .expect("reject pending recursive-Green update")
                            .is_none());
                        saw_packet = true;
                        break;
                    }
                    CandidateEventBody::Commit(_) | CandidateEventBody::DeliveryAcknowledged(_) => {
                        panic!("fixture must reject the Green update before commit")
                    }
                }
            }
            CandidatePoll::HotInlineEvent { .. } => {
                panic!("structural lifecycle fixture emitted hot-inline work")
            }
            CandidatePoll::ViewportPresentationEvent { .. } => {
                panic!("structural lifecycle fixture emitted viewport work")
            }
            CandidatePoll::ViewportPresentationUnavailable { .. } => {
                panic!("structural lifecycle fixture emitted viewport unavailability")
            }
        }
    }
    assert!(
        saw_packet,
        "pending Green update did not reach packet streaming"
    );
    drain_candidate_cleanup(&mut fixture.endpoint, &mut fixture.runtime);
    while !fixture.runtime.poll_retirement(256).complete {}
    fixture.assert_original_base_restored();
    assert!(
        fixture
            .endpoint
            .recursive_green
            .has_installed_session_for(fixture.base_ack),
        "rejection must restore the acknowledged recursive-Green session"
    );
    assert!(!fixture.endpoint.recursive_green.target_work_pending());
    assert!(!fixture.endpoint.cleanup_pending());

    let replacement_edit = fixture.edit_offset(513);
    let replacement = fixture.start_target(replacement_edit, "Y", 3, 2);
    let replacement_delivery = deliver_endpoint_to_independent_host_with_unit_fuel(
        &mut fixture.endpoint,
        &mut fixture.runtime,
        &mut fixture.host,
    );
    assert!(
        replacement_delivery.contains_recursive_green_leaf,
        "the accumulated replacement must carry definitive recursive-Green authority"
    );
    assert!(
        !fixture
            .runtime
            .commit_persistent_source_facts_delta(replacement)
            .expect("inspect delivered SourceFacts transaction"),
        "the delivery helper must commit the replacement before returning"
    );
    drain_candidate_cleanup(&mut fixture.endpoint, &mut fixture.runtime);
    assert!(fixture
        .endpoint
        .has_exact_base_for(&fixture.runtime, replacement)
        .expect("replacement becomes exact base"));
    while !fixture.runtime.poll_retirement(256).complete {}
    assert!(replacement_delivery.ack.host_revision > fixture.base_ack.host_revision);
    assert!(
        fixture
            .endpoint
            .recursive_green
            .has_installed_session_for(replacement_delivery.ack),
        "the replacement delivery must install its recursive-Green session"
    );
    assert!(
        !fixture
            .endpoint
            .recursive_green
            .has_installed_session_for(fixture.base_ack),
        "the superseded recursive-Green base must not remain installed"
    );
    let replacement_resident_nodes = fixture.runtime.arena_metrics().resident_nodes;

    let persistent_page = fixture
        .runtime
        .persistent_source_facts_page(0)
        .expect("persistent page lookup")
        .expect("replacement persistent page")
        .id();
    fixture
        .endpoint
        .begin_close()
        .expect("begin endpoint close");
    drain_candidate_cleanup(&mut fixture.endpoint, &mut fixture.runtime);
    while !fixture.runtime.poll_retirement(256).complete {}
    assert!(fixture.endpoint.retained.is_none());
    assert!(!fixture
        .endpoint
        .recursive_green
        .has_installed_session_for(replacement_delivery.ack));
    assert_eq!(
        fixture
            .runtime
            .persistent_source_facts_page(0)
            .expect("persistent page lookup")
            .expect("persistent page survives endpoint close")
            .id(),
        persistent_page
    );
    let persistent_baseline = fixture.runtime.arena_metrics();
    assert!(persistent_baseline.resident_nodes > 0);
    assert!(persistent_baseline.resident_nodes < acknowledged_base_resident_nodes);
    assert_eq!(persistent_baseline.pending_reclaims, 0);
    assert_eq!(persistent_baseline.live_builds, 0);
    assert_eq!(persistent_baseline.pending_build_aborts, 0);
    assert_eq!(
        replacement_resident_nodes
            .checked_sub(persistent_baseline.resident_nodes)
            .expect("replacement retains parser-owned residency"),
        acknowledged_base_owned_nodes,
        "the replacement must own one base-sized Green graph, not an old revision chain"
    );

    fixture.runtime.begin_close().expect("begin runtime close");
    while !fixture
        .runtime
        .poll_close(256)
        .expect("poll runtime close")
        .complete
    {}
    assert_eq!(fixture.runtime.arena_metrics().resident_nodes, 0);

    fixture.host.begin_close().expect("begin host close");
    loop {
        match fixture
            .host
            .poll(HostWorkGrant {
                inspect_bytes: 0,
                copy_bytes: 0,
                transitions: 256,
            })
            .expect("poll host close")
        {
            NativeHostPollOutcome::Pending => {}
            NativeHostPollOutcome::Closed => break,
            outcome => panic!("unexpected host close outcome: {outcome:?}"),
        }
    }
    assert!(fixture.host.is_removable());
}

#[test]
fn packet_builder_accumulates_across_small_poll_grants_and_flushes_on_end() {
    let (runtime, mut streaming) = test_streaming(16);
    let (pending_polls, event) = poll_to_packet_event(&runtime, &mut streaming, 1);
    assert!(pending_polls > 1);

    let CandidateEvent {
        credit:
            CandidateCredit::Packet {
                first_frame_ordinal,
                frame_count,
                end,
            },
        body: CandidateEventBody::Packet { encoded },
    } = event
    else {
        panic!("expected one publication packet");
    };
    let packet = decode_publication_packet(&encoded).expect("decode produced packet");
    assert_eq!(first_frame_ordinal, 0);
    assert_eq!(frame_count, packet.frame_count);
    assert!(frame_count > 1);
    assert!(end);
    assert_eq!(
        streaming.phase,
        StreamPhase::AwaitPacketReceipt {
            first_frame_ordinal,
            frame_count,
            end,
        }
    );
    let commit = streaming.commit.expect("end frame seals commit");
    assert_eq!(commit.actual_frame_count, frame_count);
    assert_eq!(
        commit.actual_encoded_frame_bytes,
        packet.aggregate_frame_bytes
    );
    let canonical_digest256 = packet
        .frames()
        .find_map(|frame| {
            M11CandidateHost::classify_frame(frame.expect("validated packet frame").bytes)
                .expect("classify packet frame")
                .canonical_stream_digest256
        })
        .expect("end frame carries canonical stream digest");
    assert_eq!(
        commit.canonical_stream_digest,
        protocol_digest128_from_blake3(ProtocolDigestDomain::CandidateStream, canonical_digest256,)
    );
    let expected_ack = streaming.expected_ack.expect("sealed candidate ack");
    assert_eq!(
        expected_ack.sequence_digest,
        protocol_digest128_from_blake3(
            ProtocolDigestDomain::CandidateAckSequence,
            streaming.descriptor.manifest_digest256,
        )
    );
    assert_ne!(
        canonical_digest256, streaming.descriptor.manifest_digest256,
        "commit and ACK must bind different 256-bit proofs"
    );
    assert_ne!(
        commit.canonical_stream_digest, expected_ack.sequence_digest,
        "commit and ACK must use separate digest domains"
    );
    assert!(streaming.lookahead.is_none());
    cancel_streaming_to_zero(runtime, streaming);
}

#[test]
fn packet_builder_enforces_exact_count_body_and_offer_caps() {
    let mut count_limited = PacketBuilder::default();
    for ordinal in 0..MAXIMUM_PACKET_FRAME_COUNT {
        push_test_frame(&mut count_limited, ordinal, 1);
    }
    assert!(count_limited
        .saturated(MAXIMUM_PACKET_ENCODED_BYTES)
        .expect("count saturation"));
    assert!(!count_limited
        .can_accept(1, MAXIMUM_PACKET_ENCODED_BYTES)
        .expect("count boundary"));

    let mut body_limited = PacketBuilder::default();
    for ordinal in 0..12 {
        push_test_frame(&mut body_limited, ordinal, 5_041);
    }
    push_test_frame(&mut body_limited, 12, 5_044);
    assert_eq!(
        body_limited.aggregate_frame_bytes,
        MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES as usize
    );
    assert!(body_limited
        .saturated(MAXIMUM_PACKET_ENCODED_BYTES)
        .expect("body saturation"));
    assert!(!body_limited
        .can_accept(1, MAXIMUM_PACKET_ENCODED_BYTES)
        .expect("body boundary"));

    let mut offer_limited = PacketBuilder::default();
    push_test_frame(&mut offer_limited, 0, 10);
    let exact_offer_cap = offer_limited.encoded_len().expect("encoded length");
    assert!(offer_limited
        .saturated(exact_offer_cap)
        .expect("offer saturation"));
    assert!(!offer_limited
        .can_accept(1, exact_offer_cap)
        .expect("offer boundary"));
}

#[test]
fn non_fitting_frame_is_retained_as_single_lookahead() {
    let (runtime, mut streaming) = test_streaming(1);
    for ordinal in 0..13 {
        push_test_frame(&mut streaming.packet, ordinal, 5_000);
    }
    streaming.next_frame_ordinal = 13;
    streaming.lookahead = Some(M11SnapshotFrame {
        kind: M11SnapshotFrameKind::Node,
        node_ordinal: Some(12),
        canonical_record_count: 0,
        canonical_stream_digest256: None,
        bytes: vec![0; 1_000].into_boxed_slice(),
    });

    let (_, event) = poll_to_packet_event(&runtime, &mut streaming, 1);
    let CandidateEventBody::Packet { encoded } = event.body else {
        panic!("expected publication packet");
    };
    let packet = decode_publication_packet(&encoded).expect("decode full packet");
    assert_eq!(packet.frame_count, 13);
    assert!(streaming.lookahead.is_some());
    assert!(streaming.packet.frames.is_empty());
    cancel_streaming_to_zero(runtime, streaming);
}

#[test]
fn packet_credit_requires_exact_frame_range_and_host_cursor() {
    let (runtime, mut streaming) = test_streaming(1);
    streaming.phase = StreamPhase::AwaitPacketReceipt {
        first_frame_ordinal: 4,
        frame_count: 3,
        end: false,
    };
    let offer_id = streaming.offer.offer_id;
    let mut endpoint = CandidateEndpoint {
        active: Some(ActiveCandidate::Streaming(Box::new(streaming))),
        cleanup: None,
        retained: None,
        recursive_green: RecursiveGreenEndpointSlot::new(),
        bullet_list_local_edit: None,
        viewport_inline_batch: None,
        pending_viewport_unavailable: None,
        last_viewport_generation: 0,
        hot_inline: None,
        hot_inline_sidecar: None,
        last_hot_inline_generation: 0,
        closing: false,
    };

    assert!(matches!(
        endpoint.accept_credit(
            CandidateCredit::Packet {
                first_frame_ordinal: 4,
                frame_count: 2,
                end: false,
            },
            77,
        ),
        Err(CandidateEndpointError::InvalidState)
    ));
    endpoint
        .accept_credit(
            CandidateCredit::Packet {
                first_frame_ordinal: 4,
                frame_count: 3,
                end: false,
            },
            77,
        )
        .expect("exact packet event credit");
    assert!(matches!(
        endpoint.handle_host_poll(
            77,
            offer_id,
            HostPollPhase::PacketCredit,
            HostPollResult::Completed(HostPollOutcome::PacketCredit {
                offer_id,
                next_frame_ordinal: 6,
            }),
        ),
        Err(CandidateEndpointError::InvalidState)
    ));
    assert!(endpoint
        .handle_host_poll(
            77,
            offer_id,
            HostPollPhase::PacketCredit,
            HostPollResult::Completed(HostPollOutcome::PacketCredit {
                offer_id,
                next_frame_ordinal: 7,
            }),
        )
        .expect("exact host packet cursor")
        .is_none());
    cancel_endpoint_to_zero(runtime, endpoint);
}

#[test]
fn cancellation_reclaims_stream_with_buffered_packet() {
    let (runtime, mut streaming) = test_streaming(32);
    match streaming
        .poll_event(&runtime, 1)
        .expect("first bounded packet poll")
    {
        CandidatePoll::Pending { transitions } => assert!(transitions <= 1),
        CandidatePoll::Event { .. } => panic!("one transition unexpectedly finished stream"),
        CandidatePoll::HotInlineEvent { .. } => {
            panic!("structural stream emitted a hot-inline event")
        }
        CandidatePoll::ViewportPresentationEvent { .. } => {
            panic!("structural stream emitted a viewport event")
        }
        CandidatePoll::ViewportPresentationUnavailable { .. } => {
            panic!("structural stream emitted viewport unavailability")
        }
    }
    assert_eq!(streaming.phase, StreamPhase::NeedPacket);
    assert!(!streaming.packet.frames.is_empty());
    cancel_streaming_to_zero(runtime, streaming);
}

#[test]
fn clean_parse_cancellation_drops_immediately_while_cleanup_drains() {
    let (mut runtime, mut prior) = test_streaming(32);
    let prior_stream = prior.stream.take().expect("prior stream");
    let profile = SourceFactsScanProfile::new(4).expect("test profile");
    let parser_profile = ParserProfileId::new(1).expect("parser profile");
    let (certified, completion) =
        complete_clean_source_facts(&mut runtime, profile, parser_profile, 1, 0);
    let binding = SessionBinding {
        document_session: [801, 802, 803, 804],
        source_session_identity: 805,
        worker_generation: 1,
    };
    let mut endpoint = CandidateEndpoint::new();
    endpoint.cleanup = Some(CandidateCleanup::Stream {
        stream: Box::new(prior_stream),
        begun: false,
    });
    endpoint
        .start(certified, binding, completion)
        .expect("clean parse may overlap bounded prior cleanup");
    assert!(matches!(endpoint.active, Some(ActiveCandidate::Parsing(_))));

    endpoint.cancel().expect("cancel clean parse");
    assert!(endpoint.active.is_none());
    assert!(endpoint.cleanup.is_some());
    drain_candidate_cleanup(&mut endpoint, &mut runtime);
    runtime.begin_close().expect("begin runtime close");
    while !runtime.poll_close(256).expect("runtime close").complete {}
    assert_eq!(runtime.arena_metrics().resident_nodes, 0);
}

#[test]
fn candidate_commit_is_invariant_to_packet_regrouping() {
    let (runtime, mut streaming) = test_streaming(16);
    let (_, event) = poll_to_packet_event(&runtime, &mut streaming, 256);
    let CandidateEventBody::Packet { encoded } = event.body else {
        panic!("expected publication packet");
    };
    let original = decode_publication_packet(&encoded).expect("decode original packet");
    let frames: Vec<_> = original
        .frames()
        .map(|frame| frame.expect("validated frame"))
        .collect();
    let split = frames.len() / 2;
    assert!(split > 0 && split < frames.len());

    let inputs: Vec<_> = frames
        .iter()
        .map(|frame| PublicationPacketFrameInput {
            record_count: frame.record_count,
            digest: frame.digest,
            bytes: frame.bytes,
        })
        .collect();
    let mut first_bytes = vec![0; MAXIMUM_PACKET_ENCODED_BYTES];
    let first_len = encode_publication_packet_into(
        PublicationPacketInput {
            offer_id: original.offer_id,
            first_frame_ordinal: original.first_frame_ordinal,
            first_record_ordinal: original.first_record_ordinal,
            frames: &inputs[..split],
        },
        &mut first_bytes,
    )
    .expect("encode first regrouped packet");
    first_bytes.truncate(first_len);
    let mut second_bytes = vec![0; MAXIMUM_PACKET_ENCODED_BYTES];
    let second_len = encode_publication_packet_into(
        PublicationPacketInput {
            offer_id: original.offer_id,
            first_frame_ordinal: frames[split].ordinal,
            first_record_ordinal: frames[split].first_record_ordinal,
            frames: &inputs[split..],
        },
        &mut second_bytes,
    )
    .expect("encode second regrouped packet");
    second_bytes.truncate(second_len);

    let first = decode_publication_packet(&first_bytes).expect("decode first regrouped packet");
    let second = decode_publication_packet(&second_bytes).expect("decode second regrouped packet");
    let mut transport = CandidateTransportDigest::new();
    for packet in [first, second] {
        for frame in packet.frames() {
            let frame = frame.expect("validated regrouped frame");
            let metadata = M11CandidateHost::classify_frame(frame.bytes)
                .expect("independent frame classification");
            assert_eq!(metadata.canonical_record_count, frame.record_count);
            let kind = match metadata.kind {
                M11HostFrameKind::Begin => CandidateSnapshotFrameKind::Begin,
                M11HostFrameKind::SourceFactsReplacementPage => {
                    CandidateSnapshotFrameKind::SourceFactsReplacementPage
                }
                M11HostFrameKind::BlockSequenceReplacementPage => {
                    CandidateSnapshotFrameKind::BlockSequenceReplacementPage
                }
                M11HostFrameKind::RecursiveGreenReplacementPage => {
                    CandidateSnapshotFrameKind::RecursiveGreenReplacementPage
                }
                M11HostFrameKind::Node => CandidateSnapshotFrameKind::Node,
                M11HostFrameKind::End => CandidateSnapshotFrameKind::End,
            };
            let digest256 = transport
                .push(
                    frame.ordinal,
                    frame.first_record_ordinal,
                    frame.record_count,
                    kind,
                    frame.bytes,
                )
                .expect("regrouped transport frame");
            assert_eq!(
                protocol_digest128_from_blake3(ProtocolDigestDomain::CandidateFrame, digest256),
                frame.digest
            );
        }
    }
    let receipt = transport.finish();
    let commit = streaming.commit.expect("sealed candidate commit");
    assert_eq!(receipt.frame_count, commit.actual_frame_count);
    assert_eq!(
        receipt.encoded_frame_bytes,
        commit.actual_encoded_frame_bytes
    );
    assert_eq!(
        protocol_digest128_from_blake3(ProtocolDigestDomain::CandidateTransport, receipt.digest256),
        commit.rolling_transport_digest
    );
    cancel_streaming_to_zero(runtime, streaming);
}

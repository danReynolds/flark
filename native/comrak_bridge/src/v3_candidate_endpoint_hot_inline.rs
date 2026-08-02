//! Hot-inline sidecar preparation and credited HIO1 streaming.
//!
//! Refinement derivation, endpoint scheduling, and cross-lane cleanup remain
//! owned by `CandidateEndpoint`; this module owns sidecar offer construction
//! and the prepared wire stream.

use super::*;
fn encode_not_inline_leaf_metadata(kind: M11BlockSequenceEntryKind) -> Box<[u8]> {
    let mut encoded = [0_u8; 12];
    encoded[0..4].copy_from_slice(b"HUN1");
    encoded[4..8].copy_from_slice(&1_u32.to_le_bytes());
    encoded[8] = kind as u8;
    encoded.into()
}

fn encode_legacy_block_target_metadata(target: InlineRefinementTarget) -> Box<[u8]> {
    let target = match target {
        InlineRefinementTarget::BulletListItemInline => 1,
        InlineRefinementTarget::BulletListItemProjection => 2,
        InlineRefinementTarget::OrderedListItemInline => 3,
        InlineRefinementTarget::OrderedListItemProjection => 4,
        InlineRefinementTarget::Automatic | InlineRefinementTarget::RecursiveGreenParagraph => 0,
    };
    let mut encoded = [0_u8; 12];
    encoded[0..4].copy_from_slice(b"HUT1");
    encoded[4..8].copy_from_slice(&1_u32.to_le_bytes());
    encoded[8] = target;
    encoded.into()
}

fn derive_hot_inline_identity(
    domain: &[u8],
    command: InlineRefinementCommand,
    block_ordinal: u64,
) -> [u32; 4] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.v3.hot-inline-sidecar.identity.v1\0");
    hasher.update(domain);
    hasher.update(&[0]);
    for word in command.binding.document_session {
        hasher.update(&word.to_le_bytes());
    }
    hasher.update(&command.binding.source_session_identity.to_le_bytes());
    hasher.update(&command.binding.worker_generation.to_le_bytes());
    hasher.update(&command.refinement_generation.to_le_bytes());
    hasher.update(&block_ordinal.to_le_bytes());
    hasher.update(&command.byte_offset.to_le_bytes());
    hasher.update(&command.utf16_offset.to_le_bytes());
    hasher.update(&[match command.affinity {
        InlinePointAffinity::Before => 1,
        InlinePointAffinity::After => 2,
    }]);
    hash_source_version(&mut hasher, command.source_version);
    hash_structural_ack(&mut hasher, command.base_ack);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    let mut identity = digest_words(bytes);
    if identity == [0; 4] {
        identity[0] = 1;
    }
    identity
}

impl CandidateEndpoint {
    pub(super) fn begin_hot_inline_sidecar(
        &mut self,
        runtime: &DocumentRuntime,
        ready: HotInlineReady,
    ) -> Result<(), CandidateEndpointError> {
        if self.hot_inline_sidecar.is_some() || self.active.is_some() || self.cleanup.is_some() {
            return Err(CandidateEndpointError::Busy);
        }
        let (
            command,
            physical_range,
            physical_range_utf16,
            visible_range,
            visible_range_utf16,
            owner,
            parser_profile,
            authority,
            publication,
        ) = ready.into_parts();
        let Some(retained) = self.retained.as_ref() else {
            self.schedule_failed_hot_inline_publication(publication, authority);
            return Err(CandidateEndpointError::InvalidAuthority);
        };
        if retained.ack != command.base_ack {
            self.schedule_failed_hot_inline_publication(publication, authority);
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let binding_result = match owner {
            HotInlineLeafOwner::BlockOrdinal(entry_ordinal) => {
                retained.publication.hot_inline_sidecar_binding(
                    runtime,
                    parser_profile,
                    u64::from(command.refinement_generation),
                    entry_ordinal,
                    physical_range,
                    visible_range,
                    physical_range_utf16,
                    visible_range_utf16,
                )
            }
            HotInlineLeafOwner::RecursiveGreenFrame(frame) => retained
                .publication
                .recursive_green_hot_inline_sidecar_binding(
                    runtime,
                    parser_profile,
                    u64::from(command.refinement_generation),
                    frame,
                    physical_range,
                    visible_range,
                    physical_range_utf16,
                    visible_range_utf16,
                ),
        };
        let binding = match binding_result {
            Ok(binding) => binding,
            Err(error) => {
                self.schedule_failed_hot_inline_publication(publication, authority);
                return Err(error.into());
            }
        };
        let wire_binding = hot_inline_wire_binding(&binding);
        let (encoder, root, authority) = match publication {
            HotInlineReadyPublication::Authoritative(root) => {
                let encoder = match root.as_ref() {
                    HotInlineProjectionRoot::Inline(root) => {
                        M11HotInlineSidecarSnapshotEncoder::authoritative(runtime, binding, root)
                    }
                    HotInlineProjectionRoot::IndentedCode(root) => {
                        M11HotInlineSidecarSnapshotEncoder::authoritative_indented_code(
                            runtime, binding, root,
                        )
                    }
                    HotInlineProjectionRoot::BlockQuote(root) => {
                        M11HotInlineSidecarSnapshotEncoder::authoritative_block_quote(
                            runtime, binding, root,
                        )
                    }
                    HotInlineProjectionRoot::BulletList(root) => {
                        M11HotInlineSidecarSnapshotEncoder::authoritative_bullet_list(
                            runtime, binding, root,
                        )
                    }
                    HotInlineProjectionRoot::BulletListItem {
                        root,
                        selected_item_ordinal,
                        canonical_line_ending,
                    } => M11HotInlineSidecarSnapshotEncoder::authoritative_bullet_list_item(
                        runtime,
                        binding,
                        root,
                        *selected_item_ordinal,
                        *canonical_line_ending,
                    ),
                    HotInlineProjectionRoot::OrderedListItem {
                        root,
                        selected_item_ordinal,
                        canonical_line_ending,
                        opening_marker_start,
                        opening_marker_end,
                        marker_value,
                    } => M11HotInlineSidecarSnapshotEncoder::authoritative_ordered_list_item(
                        runtime,
                        binding,
                        root,
                        *selected_item_ordinal,
                        *canonical_line_ending,
                        *opening_marker_start,
                        *opening_marker_end,
                        *marker_value,
                    ),
                };
                match encoder {
                    Ok(encoder) => (encoder, Some(root), authority),
                    Err(error) => {
                        self.hot_inline = Some(HotInlineState::Releasing {
                            root,
                            authority,
                            begun: false,
                            replacement: None,
                        });
                        return Err(error.into());
                    }
                }
            }
            HotInlineReadyPublication::Unsupported(unsupported) => {
                let (reason, metadata) = match unsupported {
                    HotInlineUnsupported::NotInlineLeaf { kind } => (
                        HOT_INLINE_UNSUPPORTED_NOT_INLINE_LEAF,
                        encode_not_inline_leaf_metadata(kind),
                    ),
                    HotInlineUnsupported::Parser(record) => {
                        (HOT_INLINE_UNSUPPORTED_PARSER, record.into_encoded())
                    }
                    HotInlineUnsupported::LegacyBlockTarget { target } => (
                        HOT_INLINE_UNSUPPORTED_LEGACY_BLOCK_TARGET,
                        encode_legacy_block_target_metadata(target),
                    ),
                };
                (
                    M11HotInlineSidecarSnapshotEncoder::unsupported(
                        runtime, binding, reason, metadata,
                    )?,
                    None,
                    authority,
                )
            }
        };
        let descriptor = encoder.descriptor();
        let maximum_frame_bytes = match u32::try_from(M11_MAX_SNAPSHOT_FRAME_BYTES) {
            Ok(value) => value,
            Err(_) => {
                self.schedule_hot_inline_root_release(root, authority);
                return Err(CandidateEndpointError::MetricOverflow);
            }
        };
        let Some(maximum_frame_count) = descriptor.transferred_node_count().checked_add(2) else {
            self.schedule_hot_inline_root_release(root, authority);
            return Err(CandidateEndpointError::MetricOverflow);
        };
        let Some(maximum_encoded_frame_bytes) =
            maximum_frame_count.checked_mul(maximum_frame_bytes)
        else {
            self.schedule_hot_inline_root_release(root, authority);
            return Err(CandidateEndpointError::MetricOverflow);
        };
        let owner_wire = wire_binding.block_ordinal;
        let offer_id = derive_hot_inline_identity(b"offer", command, owner_wire);
        let publication_session =
            derive_hot_inline_identity(b"publication-session", command, owner_wire);
        if publication_session == command.base_ack.publication_session {
            self.schedule_hot_inline_root_release(root, authority);
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let offer = HotInlineSidecarBegin {
            schema: HOT_INLINE_SIDECAR_SCHEMA,
            mode: HotInlineSidecarMode::HotInlineSidecar,
            offer_id,
            publication_session,
            base_ack: command.base_ack,
            binding: wire_binding,
            envelope: hot_inline_envelope_from_descriptor(descriptor),
            limits: OfferLimits {
                maximum_frame_count,
                maximum_encoded_frame_bytes,
                maximum_packet_bytes: u32::try_from(MAXIMUM_PACKET_ENCODED_BYTES)
                    .expect("bounded packet bytes fit u32"),
                maximum_frame_bytes,
                maximum_program_children: u32::try_from(M11_MAX_ROLE_RECORDS)
                    .expect("bounded program children fit u32"),
            },
        };
        self.hot_inline_sidecar = Some(StreamingHotInlineSidecar {
            encoder,
            root,
            authority,
            offer,
            phase: StreamPhase::NeedBegin,
            transport: Some(HotInlineSidecarTransportDigest::new()),
            next_frame_ordinal: 0,
            next_record_ordinal: 0,
            next_node_ordinal: None,
            packet: PacketBuilder::default(),
            lookahead: None,
            root_stream_digest: None,
            commit: None,
            expected_ack: None,
        });
        Ok(())
    }
}

impl StreamingHotInlineSidecar {
    pub(super) fn poll_event(
        &mut self,
        runtime: &DocumentRuntime,
        fuel: usize,
    ) -> Result<CandidatePoll, CandidateEndpointError> {
        match self.phase {
            StreamPhase::NeedBegin => {
                self.phase = StreamPhase::AwaitBeginReceipt;
                Ok(CandidatePoll::HotInlineEvent {
                    transitions: 0,
                    event: Box::new(HotInlineEvent {
                        credit: HotInlineCredit::Begin,
                        body: HotInlineEventBody::Begin(self.offer),
                    }),
                })
            }
            StreamPhase::NeedPacket => self.poll_packet(runtime, fuel),
            StreamPhase::NeedCommit => {
                let commit = self.commit.ok_or(CandidateEndpointError::InvalidState)?;
                self.phase = StreamPhase::AwaitCommitReceipt;
                Ok(CandidatePoll::HotInlineEvent {
                    transitions: 0,
                    event: Box::new(HotInlineEvent {
                        credit: HotInlineCredit::Commit,
                        body: HotInlineEventBody::Commit(commit),
                    }),
                })
            }
            _ => Ok(CandidatePoll::Pending { transitions: 0 }),
        }
    }

    fn poll_packet(
        &mut self,
        runtime: &DocumentRuntime,
        fuel: usize,
    ) -> Result<CandidatePoll, CandidateEndpointError> {
        let maximum_packet_bytes = usize::try_from(self.offer.limits.maximum_packet_bytes)
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let mut transitions = 0_usize;
        loop {
            if self.packet.end || self.packet.saturated(maximum_packet_bytes)? {
                let event = self.take_packet_event()?;
                return Ok(CandidatePoll::HotInlineEvent {
                    transitions,
                    event: Box::new(event),
                });
            }
            let polled = if let Some(frame) = self.lookahead.take() {
                M11HotInlineSidecarSnapshotPoll::Frame {
                    transitions: 0,
                    frame,
                }
            } else if transitions == fuel {
                return Ok(CandidatePoll::Pending { transitions });
            } else if self.next_frame_ordinal == 0 {
                if !self.packet.frames.is_empty() {
                    return Err(CandidateEndpointError::InvalidState);
                }
                M11HotInlineSidecarSnapshotPoll::Frame {
                    transitions: 0,
                    frame: self.encoder.begin_frame()?,
                }
            } else {
                self.encoder.poll(runtime, fuel - transitions)?
            };
            match polled {
                M11HotInlineSidecarSnapshotPoll::Pending {
                    transitions: consumed,
                } => {
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    return Ok(CandidatePoll::Pending { transitions });
                }
                M11HotInlineSidecarSnapshotPoll::Frame {
                    transitions: consumed,
                    frame,
                } => {
                    transitions = checked_add(transitions, consumed)?;
                    if transitions > fuel {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    if !self
                        .packet
                        .can_accept(frame.bytes.len(), maximum_packet_bytes)?
                    {
                        if self.packet.frames.is_empty() || self.lookahead.is_some() {
                            return Err(CandidateEndpointError::InvalidState);
                        }
                        self.lookahead = Some(frame);
                        let event = self.take_packet_event()?;
                        return Ok(CandidatePoll::HotInlineEvent {
                            transitions,
                            event: Box::new(event),
                        });
                    }
                    self.append_frame(frame)?;
                    if self.packet.end || self.packet.saturated(maximum_packet_bytes)? {
                        let event = self.take_packet_event()?;
                        return Ok(CandidatePoll::HotInlineEvent {
                            transitions,
                            event: Box::new(event),
                        });
                    }
                    if transitions == fuel {
                        return Ok(CandidatePoll::Pending { transitions });
                    }
                }
            }
        }
    }

    fn append_frame(
        &mut self,
        frame: M11HotInlineSidecarFrame,
    ) -> Result<(), CandidateEndpointError> {
        let ordinal = self.next_frame_ordinal;
        let first_record_ordinal = self.next_record_ordinal;
        let (wire_kind, record_count) = match frame.kind {
            M11HotInlineSidecarFrameKind::Begin => (HotInlineSidecarFrameKind::Begin, 0),
            M11HotInlineSidecarFrameKind::Node => (HotInlineSidecarFrameKind::Node, 1),
            M11HotInlineSidecarFrameKind::End => (HotInlineSidecarFrameKind::End, 0),
        };
        if matches!(frame.kind, M11HotInlineSidecarFrameKind::Begin) != (ordinal == 0)
            || frame.bytes.is_empty()
            || frame.bytes.len() > M11_MAX_SNAPSHOT_FRAME_BYTES
        {
            return Err(CandidateEndpointError::InvalidState);
        }
        if frame.kind == M11HotInlineSidecarFrameKind::Node {
            let node_ordinal = frame
                .node_ordinal
                .ok_or(CandidateEndpointError::InvalidState)?;
            if self
                .next_node_ordinal
                .is_some_and(|expected| node_ordinal != expected)
            {
                return Err(CandidateEndpointError::InvalidState);
            }
            self.next_node_ordinal = Some(
                node_ordinal
                    .checked_add(1)
                    .ok_or(CandidateEndpointError::MetricOverflow)?,
            );
        } else if frame.node_ordinal.is_some() {
            return Err(CandidateEndpointError::InvalidState);
        }
        let next_frame_ordinal = ordinal
            .checked_add(1)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let next_record_ordinal = first_record_ordinal
            .checked_add(record_count)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let digest256 = self
            .transport
            .as_mut()
            .ok_or(CandidateEndpointError::InvalidState)?
            .push(
                ordinal,
                first_record_ordinal,
                record_count,
                wire_kind,
                &frame.bytes,
            )?;
        let end = frame.kind == M11HotInlineSidecarFrameKind::End;
        self.packet.push(
            ordinal,
            first_record_ordinal,
            record_count,
            protocol_digest128_from_blake3(ProtocolDigestDomain::HotInlineSidecarFrame, digest256),
            frame.bytes,
            end,
        )?;
        self.next_frame_ordinal = next_frame_ordinal;
        self.next_record_ordinal = next_record_ordinal;
        if end {
            self.finish_frame_stream(
                frame
                    .root_stream_digest256
                    .ok_or(CandidateEndpointError::InvalidState)?,
            )?;
        } else if frame.root_stream_digest256.is_some() {
            return Err(CandidateEndpointError::InvalidState);
        }
        Ok(())
    }

    fn finish_frame_stream(
        &mut self,
        root_stream_digest256: [u8; 32],
    ) -> Result<(), CandidateEndpointError> {
        if self.next_record_ordinal != self.offer.envelope.transferred_node_count {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let root_stream_digest = protocol_digest128_from_blake3(
            ProtocolDigestDomain::HotInlineSidecarRootStream,
            root_stream_digest256,
        );
        self.root_stream_digest = Some(root_stream_digest);
        let transport = self
            .transport
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?
            .finish();
        if transport.frame_count != self.next_frame_ordinal
            || transport.frame_count > self.offer.limits.maximum_frame_count
            || transport.encoded_frame_bytes > self.offer.limits.maximum_encoded_frame_bytes
        {
            return Err(CandidateEndpointError::InvalidState);
        }
        self.commit = Some(HotInlineSidecarCommitRequest {
            offer_id: self.offer.offer_id,
            actual_frame_count: transport.frame_count,
            actual_encoded_frame_bytes: transport.encoded_frame_bytes,
            rolling_transport_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::HotInlineSidecarTransport,
                transport.digest256,
            ),
            root_stream_digest,
        });
        self.expected_ack = Some(InlineSidecarAck {
            publication_session: self.offer.publication_session,
            base_ack: self.offer.base_ack,
            refinement_generation: self.offer.binding.refinement_generation,
            block_ordinal: self.offer.binding.block_ordinal,
            transferred_node_count: self.offer.envelope.transferred_node_count,
            disposition: match self.offer.envelope.disposition {
                HotInlineSidecarDisposition::Authoritative { .. } => {
                    InlineSidecarAckDisposition::Authoritative
                }
                HotInlineSidecarDisposition::Unsupported { .. } => {
                    InlineSidecarAckDisposition::Unsupported
                }
            },
            hio1_envelope_digest256: self.offer.envelope.hio1_envelope_digest256,
            root_stream_digest,
        });
        Ok(())
    }

    fn take_packet_event(&mut self) -> Result<HotInlineEvent, CandidateEndpointError> {
        let packet = std::mem::take(&mut self.packet);
        let first_frame_ordinal = packet
            .first_frame_ordinal
            .ok_or(CandidateEndpointError::InvalidState)?;
        let frame_count = u32::try_from(packet.frames.len())
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let end = packet.end;
        let encoded = packet.encode(self.offer.offer_id)?;
        self.phase = StreamPhase::AwaitPacketReceipt {
            first_frame_ordinal,
            frame_count,
            end,
        };
        Ok(HotInlineEvent {
            credit: HotInlineCredit::Packet {
                first_frame_ordinal,
                frame_count,
                end,
            },
            body: HotInlineEventBody::Packet { encoded },
        })
    }
}

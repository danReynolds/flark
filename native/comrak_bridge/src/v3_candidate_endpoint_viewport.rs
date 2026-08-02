//! Viewport presentation preparation and credited VPB1 streaming.
//!
//! Request admission, inline derivation, cancellation, and scheduling remain
//! owned by `CandidateEndpoint`; this module owns the prepared wire stream.

use super::*;
fn derive_viewport_identity(domain: &[u8], command: ViewportInlineBatchCommand) -> [u32; 4] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.v3.viewport-presentation.identity.v1\0");
    hasher.update(domain);
    hasher.update(&[0]);
    for word in command.binding.document_session {
        hasher.update(&word.to_le_bytes());
    }
    hasher.update(&command.binding.source_session_identity.to_le_bytes());
    hasher.update(&command.binding.worker_generation.to_le_bytes());
    hasher.update(&command.viewport_generation.to_le_bytes());
    hasher.update(&command.start_entry_ordinal.to_le_bytes());
    hasher.update(&command.start_byte_offset.to_le_bytes());
    hasher.update(&command.start_utf16_offset.to_le_bytes());
    hasher.update(&command.end_byte_offset.to_le_bytes());
    hasher.update(&command.end_utf16_offset.to_le_bytes());
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
pub(super) fn prepare_viewport_presentation(
    runtime: &DocumentRuntime,
    retained: &RetainedCandidateBase,
    ready: ViewportInlineBatchReady,
) -> Result<StreamingViewportPresentation, ViewportPresentationPreparationFailure> {
    let ViewportInlineBatchReady {
        command,
        descriptor,
        range_receipt,
        mut leaves,
        total_inline_source_bytes,
        total_parser_transitions,
        total_fact_records,
        total_ready_roots,
    } = ready;
    let mut prepared = Vec::new();
    let mut directory = Vec::new();
    let reserve_result = prepared
        .try_reserve_exact(leaves.len())
        .and_then(|()| directory.try_reserve_exact(leaves.len()));
    if reserve_result.is_err() {
        return Err(ViewportPresentationPreparationFailure {
            error: CandidateEndpointError::AllocationFailed,
            cleanup: ViewportInlineBatchCleanup {
                active_job: None,
                active_abort_begun: false,
                ready: leaves,
                prepared,
                active_child: None,
                releasing: None,
                hot_replacement: None,
            },
        });
    }
    let fail = |error, leaves, prepared| ViewportPresentationPreparationFailure {
        error,
        cleanup: ViewportInlineBatchCleanup {
            active_job: None,
            active_abort_begun: false,
            ready: leaves,
            prepared,
            active_child: None,
            releasing: None,
            hot_replacement: None,
        },
    };
    let retained_descriptor = match retained.publication.descriptor(runtime) {
        Ok(descriptor) => descriptor,
        Err(error) => return Err(fail(error.into(), leaves, prepared)),
    };
    if retained.ack != command.base_ack || retained_descriptor != descriptor {
        return Err(fail(
            CandidateEndpointError::InvalidAuthority,
            leaves,
            prepared,
        ));
    }

    let mut transferred_node_count = 0_u32;
    let mut descriptor_fact_count = 0_u64;
    let mut authoritative_root_count = 0_u32;
    let mut maximum_encoded_child_bytes = 0_u64;
    while let Some(leaf) = leaves.pop() {
        let ordered_child_index = match u32::try_from(leaves.len()) {
            Ok(value) => value,
            Err(_) => {
                leaves.push(leaf);
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        };
        let global_row_ordinal = leaf.geometry.entry_ordinal;
        let binding = match retained
            .publication
            .recursive_green_hot_inline_sidecar_binding(
                runtime,
                leaf.parser_profile,
                u64::from(command.viewport_generation),
                leaf.geometry.frame,
                leaf.geometry.block_source.clone(),
                leaf.geometry.inline_source.clone(),
                leaf.geometry.block_source_utf16.clone(),
                leaf.geometry.inline_source_utf16.clone(),
            ) {
            Ok(binding) => binding,
            Err(error) => {
                leaves.push(leaf);
                return Err(fail(error.into(), leaves, prepared));
            }
        };
        let wire_binding = hot_inline_wire_binding(&binding);
        let ViewportInlineLeafReady {
            geometry,
            parser_profile,
            mut authority,
            publication,
        } = leaf;
        let (publication, descriptor) = match publication {
            ViewportInlineLeafPublication::Authoritative(root) => {
                match M11HotInlineSidecarSnapshotEncoder::authoritative(
                    runtime,
                    binding.clone(),
                    &root,
                ) {
                    Ok(encoder) => {
                        let descriptor = encoder.descriptor();
                        drop(encoder);
                        authoritative_root_count = match authoritative_root_count.checked_add(1) {
                            Some(value) => value,
                            None => {
                                leaves.push(ViewportInlineLeafReady {
                                    geometry,
                                    parser_profile,
                                    authority,
                                    publication: ViewportInlineLeafPublication::Authoritative(root),
                                });
                                return Err(fail(
                                    CandidateEndpointError::MetricOverflow,
                                    leaves,
                                    prepared,
                                ));
                            }
                        };
                        (
                            ViewportPreparedChildPublication::Authoritative(root),
                            descriptor,
                        )
                    }
                    Err(error) => {
                        leaves.push(ViewportInlineLeafReady {
                            geometry,
                            parser_profile,
                            authority,
                            publication: ViewportInlineLeafPublication::Authoritative(root),
                        });
                        return Err(fail(error.into(), leaves, prepared));
                    }
                }
            }
            ViewportInlineLeafPublication::Unsupported(record) => {
                let metadata = record.into_encoded();
                match M11HotInlineSidecarSnapshotEncoder::unsupported(
                    runtime,
                    binding.clone(),
                    HOT_INLINE_UNSUPPORTED_PARSER,
                    metadata.clone(),
                ) {
                    Ok(encoder) => {
                        let descriptor = encoder.descriptor();
                        drop(encoder);
                        (
                            ViewportPreparedChildPublication::Unsupported(metadata),
                            descriptor,
                        )
                    }
                    Err(error) => {
                        drop(metadata);
                        drop(authority.take());
                        return Err(fail(error.into(), leaves, prepared));
                    }
                }
            }
        };
        let hio1_envelope = hot_inline_envelope_from_descriptor(descriptor);
        transferred_node_count =
            match transferred_node_count.checked_add(hio1_envelope.transferred_node_count) {
                Some(value) => value,
                None => {
                    prepared.push(ViewportPreparedChild {
                        geometry,
                        parser_profile,
                        authority,
                        publication,
                        binding,
                        directory: ViewportPresentationDirectoryEntry {
                            ordered_child_index,
                            global_row_ordinal,
                            binding: wire_binding,
                            hio1_envelope,
                        },
                    });
                    return Err(fail(
                        CandidateEndpointError::MetricOverflow,
                        leaves,
                        prepared,
                    ));
                }
            };
        let child_frame_count = match hio1_envelope.transferred_node_count.checked_add(2) {
            Some(value) => value,
            None => {
                prepared.push(ViewportPreparedChild {
                    geometry,
                    parser_profile,
                    authority,
                    publication,
                    binding,
                    directory: ViewportPresentationDirectoryEntry {
                        ordered_child_index,
                        global_row_ordinal,
                        binding: wire_binding,
                        hio1_envelope,
                    },
                });
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        };
        maximum_encoded_child_bytes = match maximum_encoded_child_bytes
            .checked_add(u64::from(hio1_envelope.hio1_encoded_bytes))
            .and_then(|bytes| bytes.checked_add(u64::from(hio1_envelope.ipr2_descriptor_bytes)))
            // HIO1 Begin has a small fixed header before the authenticated
            // envelope and optional descriptor; End is likewise fixed-width.
            // Three public metadata-record widths safely dominate both
            // private fixed headers without depending on their exact layout.
            .and_then(|bytes| {
                bytes.checked_add(
                    u64::try_from(M11_INLINE_META_RECORD_BYTES)
                        .ok()?
                        .checked_mul(3)?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    u64::from(hio1_envelope.transferred_node_count)
                        .checked_mul(u64::try_from(M11_MAX_SNAPSHOT_FRAME_BYTES).ok()?)?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    u64::from(child_frame_count)
                        * u64::try_from(VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES).ok()?,
                )
            }) {
            Some(value) => value,
            None => {
                prepared.push(ViewportPreparedChild {
                    geometry,
                    parser_profile,
                    authority,
                    publication,
                    binding,
                    directory: ViewportPresentationDirectoryEntry {
                        ordered_child_index,
                        global_row_ordinal,
                        binding: wire_binding,
                        hio1_envelope,
                    },
                });
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        };
        if let HotInlineSidecarDisposition::Authoritative { fact_count, .. } =
            hio1_envelope.disposition
        {
            descriptor_fact_count = match descriptor_fact_count.checked_add(fact_count) {
                Some(value) => value,
                None => {
                    prepared.push(ViewportPreparedChild {
                        geometry,
                        parser_profile,
                        authority,
                        publication,
                        binding,
                        directory: ViewportPresentationDirectoryEntry {
                            ordered_child_index,
                            global_row_ordinal,
                            binding: wire_binding,
                            hio1_envelope,
                        },
                    });
                    return Err(fail(
                        CandidateEndpointError::MetricOverflow,
                        leaves,
                        prepared,
                    ));
                }
            };
        }
        let entry = ViewportPresentationDirectoryEntry {
            ordered_child_index,
            global_row_ordinal,
            binding: wire_binding,
            hio1_envelope,
        };
        directory.push(entry);
        prepared.push(ViewportPreparedChild {
            geometry,
            parser_profile,
            authority,
            publication,
            binding,
            directory: entry,
        });
    }
    directory.reverse();

    let leaf_count = match u32::try_from(directory.len()) {
        Ok(value) => value,
        Err(_) => {
            return Err(fail(
                CandidateEndpointError::MetricOverflow,
                leaves,
                prepared,
            ));
        }
    };
    if descriptor_fact_count != total_fact_records
        || total_ready_roots != authoritative_root_count
        || prepared.len() != directory.len()
    {
        return Err(fail(
            CandidateEndpointError::InvalidAuthority,
            leaves,
            prepared,
        ));
    }
    let binding = ViewportPresentationBinding {
        viewport_generation: command.viewport_generation,
        requested_range: ViewportPresentationMetricRange {
            start_utf8: command.start_byte_offset,
            start_utf16: command.start_utf16_offset,
            end_utf8: command.end_byte_offset,
            end_utf16: command.end_utf16_offset,
        },
        covered_range: ViewportPresentationMetricRange {
            start_utf8: command.start_byte_offset,
            start_utf16: command.start_utf16_offset,
            end_utf8: match u32::try_from(range_receipt.next_byte_offset) {
                Ok(value) => value,
                Err(_) => {
                    return Err(fail(
                        CandidateEndpointError::MetricOverflow,
                        leaves,
                        prepared,
                    ));
                }
            },
            end_utf16: match u32::try_from(range_receipt.next_utf16_offset) {
                Ok(value) => value,
                Err(_) => {
                    return Err(fail(
                        CandidateEndpointError::MetricOverflow,
                        leaves,
                        prepared,
                    ));
                }
            },
        },
        start: ViewportPresentationVisitStart {
            block_ordinal: command.start_entry_ordinal,
            utf8_offset: command.start_byte_offset,
            utf16_offset: command.start_utf16_offset,
        },
        next: ViewportPresentationVisitStart {
            block_ordinal: range_receipt.next_row_ordinal,
            utf8_offset: match u32::try_from(range_receipt.next_byte_offset) {
                Ok(value) => value,
                Err(_) => {
                    return Err(fail(
                        CandidateEndpointError::MetricOverflow,
                        leaves,
                        prepared,
                    ));
                }
            },
            utf16_offset: match u32::try_from(range_receipt.next_utf16_offset) {
                Ok(value) => value,
                Err(_) => {
                    return Err(fail(
                        CandidateEndpointError::MetricOverflow,
                        leaves,
                        prepared,
                    ));
                }
            },
        },
        complete: range_receipt.next_byte_offset == u64::from(command.end_byte_offset)
            && range_receipt.next_utf16_offset == u64::from(command.end_utf16_offset),
    };
    let query_limits = ViewportPresentationQueryLimits {
        maximum_structural_entries: command.limits.maximum_structural_entries,
        maximum_storage_pages: command.limits.maximum_storage_pages,
        maximum_inline_leaves: command.limits.maximum_inline_leaves,
        maximum_inline_leaf_source_bytes: command.limits.maximum_inline_leaf_source_bytes,
        maximum_inline_source_bytes: match u32::try_from(command.limits.maximum_inline_source_bytes)
        {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        maximum_fact_records: match u32::try_from(command.limits.maximum_fact_records) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        maximum_encoded_frame_bytes: match u32::try_from(command.limits.maximum_projection_bytes) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        maximum_parser_transitions: match u32::try_from(command.limits.maximum_parser_transitions) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
    };
    let envelope = crate::v3_publication_wire::ViewportPresentationEnvelopeMetrics {
        visited_structural_entries: match u32::try_from(range_receipt.visited_rows) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        visited_storage_pages: match u32::try_from(range_receipt.storage_pages_visited) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        ordered_leaf_count: leaf_count,
        inline_source_bytes: match u32::try_from(total_inline_source_bytes) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        fact_count: match u32::try_from(total_fact_records) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        transferred_node_count,
        parser_transitions: match u32::try_from(total_parser_transitions) {
            Ok(value) => value,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        },
        aggregate_envelope_digest256: [0; 32],
    };
    let directory_entries_bytes = match directory
        .len()
        .checked_mul(VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES)
    {
        Some(value) => value,
        None => {
            return Err(fail(
                CandidateEndpointError::MetricOverflow,
                leaves,
                prepared,
            ));
        }
    };
    let directory_bytes =
        match VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES.checked_add(directory_entries_bytes) {
            Some(value) => value,
            None => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        };
    let frame_count = match leaf_count
        .checked_mul(2)
        .and_then(|count| count.checked_add(transferred_node_count))
        .and_then(|count| count.checked_add(3))
    {
        Some(value) => value,
        None => {
            return Err(fail(
                CandidateEndpointError::MetricOverflow,
                leaves,
                prepared,
            ));
        }
    };
    // This is the one authoritative admission check for the private VPB1
    // stream. It charges the exact selected HIO1 envelopes, IPR3 descriptors,
    // transferred-node ceiling, VPB1 wrappers, and directory. Do not add an
    // earlier logical-page * maximum-record charge: logical pages are semantic
    // units, not independently transferred maximum-size frames, and that
    // approximation rejects valid bounded batches long before this transport
    // limit is reached.
    let maximum_encoded_bytes = match maximum_encoded_child_bytes
        .checked_add(u64::try_from(VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES).unwrap_or(u64::MAX))
        .and_then(|bytes| bytes.checked_add(u64::try_from(directory_bytes).ok()?))
        .and_then(|bytes| {
            bytes.checked_add(u64::try_from(VIEWPORT_PRESENTATION_END_FRAME_BYTES).ok()?)
        })
        .and_then(|bytes| u32::try_from(bytes).ok())
    {
        Some(value) => value,
        None => {
            return Err(fail(
                CandidateEndpointError::MetricOverflow,
                leaves,
                prepared,
            ));
        }
    };
    let maximum_child_frame_bytes =
        match VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES.checked_add(M11_MAX_SNAPSHOT_FRAME_BYTES) {
            Some(value) => value,
            None => {
                return Err(fail(
                    CandidateEndpointError::MetricOverflow,
                    leaves,
                    prepared,
                ));
            }
        };
    if maximum_encoded_bytes > query_limits.maximum_encoded_frame_bytes {
        return Err(fail(
            CandidateEndpointError::ViewportInlineLimitExceeded("encoded frame bytes"),
            leaves,
            prepared,
        ));
    }
    let maximum_frame_bytes = match u32::try_from(directory_bytes.max(maximum_child_frame_bytes)) {
        Ok(value) => value,
        Err(_) => {
            return Err(fail(
                CandidateEndpointError::MetricOverflow,
                leaves,
                prepared,
            ));
        }
    };
    let limits = ViewportPresentationOfferLimits {
        maximum_frame_count: frame_count,
        maximum_encoded_frame_bytes: query_limits.maximum_encoded_frame_bytes,
        maximum_packet_bytes: u32::try_from(MAXIMUM_PACKET_ENCODED_BYTES)
            .expect("bounded packet bytes fit u32"),
        maximum_frame_bytes,
        maximum_program_children: u32::try_from(M11_MAX_ROLE_RECORDS)
            .expect("bounded program children fit u32"),
    };
    let offer_id = derive_viewport_identity(b"offer", command);
    let publication_session = derive_viewport_identity(b"publication-session", command);
    if offer_id == publication_session
        || publication_session == command.base_ack.publication_session
    {
        return Err(fail(
            CandidateEndpointError::InvalidAuthority,
            leaves,
            prepared,
        ));
    }
    let mut offer = ViewportPresentationBegin {
        schema: MANIFEST_SCHEMA,
        mode: ViewportPresentationMode::AggregatePage,
        offer_id,
        publication_session,
        base_ack: command.base_ack,
        binding,
        envelope,
        query_limits,
        limits,
    };
    let mut directory_frame = Vec::new();
    if directory_frame.try_reserve_exact(directory_bytes).is_err() {
        return Err(fail(
            CandidateEndpointError::AllocationFailed,
            leaves,
            prepared,
        ));
    }
    directory_frame.resize(directory_bytes, 0);
    let written = match encode_viewport_presentation_directory_into(
        offer,
        &directory,
        &mut directory_frame,
    ) {
        Ok(written) => written,
        Err(error) => return Err(fail(error.into(), leaves, prepared)),
    };
    if written != directory_frame.len() {
        return Err(fail(CandidateEndpointError::InvalidState, leaves, prepared));
    }
    offer.envelope.aggregate_envelope_digest256 =
        match viewport_presentation_aggregate_envelope_digest256(
            offer.binding,
            offer.envelope,
            &directory_frame,
        ) {
            Ok(digest) => digest,
            Err(_) => {
                return Err(fail(
                    CandidateEndpointError::InvalidAuthority,
                    leaves,
                    prepared,
                ));
            }
        };
    Ok(StreamingViewportPresentation {
        offer,
        directory,
        directory_frame: Some(directory_frame.into_boxed_slice()),
        pending: prepared,
        active: None,
        releasing: None,
        phase: StreamPhase::NeedBegin,
        transport: Some(ViewportPresentationTransportDigest::new()),
        next_frame_ordinal: 0,
        next_record_ordinal: 0,
        packet: PacketBuilder::default(),
        lookahead: None,
        actual_child_frame_count: 0,
        actual_child_encoded_bytes: 0,
        commit: None,
        expected_ack: None,
    })
}

impl StreamingViewportPresentation {
    pub(super) fn poll_event(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<CandidatePoll, CandidateEndpointError> {
        match self.phase {
            StreamPhase::NeedBegin => {
                self.phase = StreamPhase::AwaitBeginReceipt;
                Ok(CandidatePoll::ViewportPresentationEvent {
                    transitions: 0,
                    event: Box::new(CandidateViewportPresentationEvent {
                        credit: ViewportPresentationCredit::Begin,
                        body: CandidateViewportPresentationEventBody::Begin(self.offer),
                    }),
                })
            }
            StreamPhase::NeedPacket => self.poll_packet(runtime, fuel),
            StreamPhase::NeedCommit => {
                let commit = self.commit.ok_or(CandidateEndpointError::InvalidState)?;
                self.phase = StreamPhase::AwaitCommitReceipt;
                Ok(CandidatePoll::ViewportPresentationEvent {
                    transitions: 0,
                    event: Box::new(CandidateViewportPresentationEvent {
                        credit: ViewportPresentationCredit::Commit,
                        body: CandidateViewportPresentationEventBody::Commit(commit),
                    }),
                })
            }
            _ => Ok(CandidatePoll::Pending { transitions: 0 }),
        }
    }

    fn poll_packet(
        &mut self,
        runtime: &mut DocumentRuntime,
        fuel: usize,
    ) -> Result<CandidatePoll, CandidateEndpointError> {
        let maximum_packet_bytes = usize::try_from(self.offer.limits.maximum_packet_bytes)
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let maximum_frame_bytes = usize::try_from(self.offer.limits.maximum_frame_bytes)
            .map_err(|_| CandidateEndpointError::MetricOverflow)?;
        let mut transitions = 0_usize;
        loop {
            if self.packet.end || self.packet.saturated(maximum_packet_bytes)? {
                return Ok(CandidatePoll::ViewportPresentationEvent {
                    transitions,
                    event: Box::new(self.take_packet_event()?),
                });
            }
            if self.next_frame_ordinal == 0 {
                let mut bytes = vec![0_u8; VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES];
                let written =
                    encode_viewport_presentation_parent_frame_into(self.offer, &mut bytes)?;
                if written != bytes.len() {
                    return Err(CandidateEndpointError::InvalidState);
                }
                if !self.packet.can_accept_with_frame_limit(
                    bytes.len(),
                    maximum_packet_bytes,
                    maximum_frame_bytes,
                )? {
                    return Err(CandidateEndpointError::InvalidState);
                }
                self.append_outer_frame(
                    ViewportPresentationFrameKind::Begin,
                    0,
                    bytes.into_boxed_slice(),
                    false,
                )?;
                continue;
            }
            if self.next_frame_ordinal == 1 {
                let bytes = self
                    .directory_frame
                    .take()
                    .ok_or(CandidateEndpointError::InvalidState)?;
                if !self.packet.can_accept_with_frame_limit(
                    bytes.len(),
                    maximum_packet_bytes,
                    maximum_frame_bytes,
                )? {
                    if self.packet.frames.is_empty() {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    self.directory_frame = Some(bytes);
                    return Ok(CandidatePoll::ViewportPresentationEvent {
                        transitions,
                        event: Box::new(self.take_packet_event()?),
                    });
                }
                self.append_outer_frame(
                    ViewportPresentationFrameKind::Directory,
                    self.offer.envelope.ordered_leaf_count,
                    bytes,
                    false,
                )?;
                continue;
            }

            if let Some(releasing) = self.releasing.as_mut() {
                if transitions == fuel {
                    return Ok(CandidatePoll::Pending { transitions });
                }
                if !releasing.begun {
                    releasing.root.begin_release(runtime)?;
                    releasing.begun = true;
                    transitions = checked_add(transitions, 1)?;
                    if transitions == fuel {
                        return Ok(CandidatePoll::Pending { transitions });
                    }
                }
                let polled = releasing.root.poll_release(runtime, fuel - transitions)?;
                transitions = checked_add(transitions, polled.receipt().transitions)?;
                if transitions > fuel {
                    return Err(CandidateEndpointError::InvalidState);
                }
                if !polled.complete() {
                    return Ok(CandidatePoll::Pending { transitions });
                }
                let released = self
                    .releasing
                    .take()
                    .ok_or(CandidateEndpointError::InvalidState)?;
                drop(released.root);
                drop(released.authority);
                continue;
            }

            if self.active.is_none() {
                if let Some(mut prepared) = self.pending.pop() {
                    let expected_directory_index = self
                        .offer
                        .envelope
                        .ordered_leaf_count
                        .checked_sub(
                            u32::try_from(self.pending.len())
                                .map_err(|_| CandidateEndpointError::MetricOverflow)?,
                        )
                        .and_then(|count| count.checked_sub(1))
                        .ok_or(CandidateEndpointError::InvalidState)?;
                    if prepared.directory.ordered_child_index != expected_directory_index
                        || prepared.parser_profile.get()
                            != prepared.directory.binding.parser_profile
                        || hot_inline_wire_binding(&prepared.binding) != prepared.directory.binding
                        || prepared.geometry.entry_ordinal != prepared.directory.global_row_ordinal
                        || prepared.directory.binding.owner()
                            != Some(HotInlineSidecarOwner::RecursiveGreenFrame(
                                prepared.geometry.frame.get(),
                            ))
                    {
                        self.pending.push(prepared);
                        return Err(CandidateEndpointError::InvalidAuthority);
                    }
                    let (encoder, root) = match prepared.publication {
                        ViewportPreparedChildPublication::Authoritative(root) => (
                            M11HotInlineSidecarSnapshotEncoder::authoritative(
                                runtime,
                                prepared.binding,
                                &root,
                            )?,
                            Some(root),
                        ),
                        ViewportPreparedChildPublication::Unsupported(metadata) => (
                            M11HotInlineSidecarSnapshotEncoder::unsupported(
                                runtime,
                                prepared.binding,
                                HOT_INLINE_UNSUPPORTED_PARSER,
                                metadata,
                            )?,
                            None,
                        ),
                    };
                    self.active = Some(ViewportActiveChild {
                        directory_index: expected_directory_index,
                        encoder,
                        root,
                        authority: prepared.authority.take(),
                        next_frame_ordinal: 0,
                        next_node_ordinal: None,
                    });
                } else {
                    let transport = self
                        .transport
                        .as_ref()
                        .ok_or(CandidateEndpointError::InvalidState)?
                        .receipt();
                    let actual_frame_count = transport
                        .frame_count
                        .checked_add(1)
                        .ok_or(CandidateEndpointError::MetricOverflow)?;
                    let actual_encoded_frame_bytes = transport
                        .encoded_frame_bytes
                        .checked_add(
                            u32::try_from(VIEWPORT_PRESENTATION_END_FRAME_BYTES)
                                .expect("VPB1 End width fits u32"),
                        )
                        .ok_or(CandidateEndpointError::MetricOverflow)?;
                    let mut bytes = vec![0_u8; VIEWPORT_PRESENTATION_END_FRAME_BYTES];
                    let written = encode_viewport_presentation_end_frame_into(
                        self.offer,
                        ViewportPresentationEndFrame {
                            ordered_leaf_count: self.offer.envelope.ordered_leaf_count,
                            actual_frame_count,
                            actual_encoded_frame_bytes,
                            aggregate_envelope_digest256: self
                                .offer
                                .envelope
                                .aggregate_envelope_digest256,
                        },
                        &mut bytes,
                    )?;
                    if written != bytes.len() {
                        return Err(CandidateEndpointError::InvalidState);
                    }
                    if !self.packet.can_accept_with_frame_limit(
                        bytes.len(),
                        maximum_packet_bytes,
                        maximum_frame_bytes,
                    )? {
                        if self.packet.frames.is_empty() {
                            return Err(CandidateEndpointError::InvalidState);
                        }
                        return Ok(CandidatePoll::ViewportPresentationEvent {
                            transitions,
                            event: Box::new(self.take_packet_event()?),
                        });
                    }
                    self.append_outer_frame(
                        ViewportPresentationFrameKind::End,
                        0,
                        bytes.into_boxed_slice(),
                        true,
                    )?;
                    self.finish_frame_stream()?;
                    return Ok(CandidatePoll::ViewportPresentationEvent {
                        transitions,
                        event: Box::new(self.take_packet_event()?),
                    });
                }
            }

            let polled = if let Some(frame) = self.lookahead.take() {
                M11HotInlineSidecarSnapshotPoll::Frame {
                    transitions: 0,
                    frame,
                }
            } else {
                let active = self
                    .active
                    .as_mut()
                    .ok_or(CandidateEndpointError::InvalidState)?;
                if active.next_frame_ordinal == 0 {
                    M11HotInlineSidecarSnapshotPoll::Frame {
                        transitions: 0,
                        frame: active.encoder.begin_frame()?,
                    }
                } else if transitions == fuel {
                    return Ok(CandidatePoll::Pending { transitions });
                } else {
                    active.encoder.poll(runtime, fuel - transitions)?
                }
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
                    let wrapped_len = VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES
                        .checked_add(frame.bytes.len())
                        .ok_or(CandidateEndpointError::MetricOverflow)?;
                    if !self.packet.can_accept_with_frame_limit(
                        wrapped_len,
                        maximum_packet_bytes,
                        maximum_frame_bytes,
                    )? {
                        if self.packet.frames.is_empty() || self.lookahead.is_some() {
                            return Err(CandidateEndpointError::InvalidState);
                        }
                        self.lookahead = Some(frame);
                        return Ok(CandidatePoll::ViewportPresentationEvent {
                            transitions,
                            event: Box::new(self.take_packet_event()?),
                        });
                    }
                    self.append_child_frame(frame, wrapped_len)?;
                    if self.packet.saturated(maximum_packet_bytes)? {
                        return Ok(CandidatePoll::ViewportPresentationEvent {
                            transitions,
                            event: Box::new(self.take_packet_event()?),
                        });
                    }
                    if transitions == fuel {
                        return Ok(CandidatePoll::Pending { transitions });
                    }
                }
            }
        }
    }

    fn append_child_frame(
        &mut self,
        frame: M11HotInlineSidecarFrame,
        wrapped_len: usize,
    ) -> Result<(), CandidateEndpointError> {
        let active = self
            .active
            .as_mut()
            .ok_or(CandidateEndpointError::InvalidState)?;
        let child_frame_ordinal = active.next_frame_ordinal;
        let (kind, record_count) = match frame.kind {
            M11HotInlineSidecarFrameKind::Begin => (HotInlineSidecarFrameKind::Begin, 0),
            M11HotInlineSidecarFrameKind::Node => (HotInlineSidecarFrameKind::Node, 1),
            M11HotInlineSidecarFrameKind::End => (HotInlineSidecarFrameKind::End, 0),
        };
        if matches!(frame.kind, M11HotInlineSidecarFrameKind::Begin) != (child_frame_ordinal == 0)
            || frame.bytes.is_empty()
            || frame.bytes.len() > M11_MAX_SNAPSHOT_FRAME_BYTES
        {
            return Err(CandidateEndpointError::InvalidState);
        }
        if frame.kind == M11HotInlineSidecarFrameKind::Node {
            let node_ordinal = frame
                .node_ordinal
                .ok_or(CandidateEndpointError::InvalidState)?;
            if active
                .next_node_ordinal
                .is_some_and(|expected| expected != node_ordinal)
            {
                return Err(CandidateEndpointError::InvalidState);
            }
            active.next_node_ordinal = Some(
                node_ordinal
                    .checked_add(1)
                    .ok_or(CandidateEndpointError::MetricOverflow)?,
            );
        } else if frame.node_ordinal.is_some() {
            return Err(CandidateEndpointError::InvalidState);
        }
        if frame.kind == M11HotInlineSidecarFrameKind::End && frame.root_stream_digest256.is_none()
            || frame.kind != M11HotInlineSidecarFrameKind::End
                && frame.root_stream_digest256.is_some()
        {
            return Err(CandidateEndpointError::InvalidState);
        }
        let mut wrapped = Vec::new();
        wrapped
            .try_reserve_exact(wrapped_len)
            .map_err(|_| CandidateEndpointError::AllocationFailed)?;
        wrapped.resize(wrapped_len, 0);
        let written = encode_viewport_presentation_child_frame_into(
            self.offer,
            ViewportPresentationChildFrameInput {
                directory_index: active.directory_index,
                child_frame_ordinal,
                kind,
                record_count,
                payload: &frame.bytes,
            },
            &mut wrapped,
        )?;
        if written != wrapped.len() {
            return Err(CandidateEndpointError::InvalidState);
        }
        active.next_frame_ordinal = active
            .next_frame_ordinal
            .checked_add(1)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let end = frame.kind == M11HotInlineSidecarFrameKind::End;
        self.actual_child_frame_count = self
            .actual_child_frame_count
            .checked_add(1)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        self.actual_child_encoded_bytes = self
            .actual_child_encoded_bytes
            .checked_add(
                u32::try_from(wrapped.len()).map_err(|_| CandidateEndpointError::MetricOverflow)?,
            )
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        self.append_outer_frame(
            ViewportPresentationFrameKind::Child,
            record_count,
            wrapped.into_boxed_slice(),
            false,
        )?;
        if end {
            let mut active = self
                .active
                .take()
                .ok_or(CandidateEndpointError::InvalidState)?;
            let directory = self
                .directory
                .get(
                    usize::try_from(active.directory_index)
                        .map_err(|_| CandidateEndpointError::MetricOverflow)?,
                )
                .ok_or(CandidateEndpointError::InvalidState)?;
            let expected_child_frames = directory
                .hio1_envelope
                .transferred_node_count
                .checked_add(2)
                .ok_or(CandidateEndpointError::MetricOverflow)?;
            if active.next_frame_ordinal != expected_child_frames {
                self.active = Some(active);
                return Err(CandidateEndpointError::InvalidAuthority);
            }
            drop(active.encoder);
            if let Some(root) = active.root.take() {
                self.releasing = Some(ReleasingViewportInlineRoot {
                    root,
                    authority: active.authority.take(),
                    begun: false,
                });
            } else {
                drop(active.authority.take());
            }
        }
        Ok(())
    }

    fn append_outer_frame(
        &mut self,
        kind: ViewportPresentationFrameKind,
        record_count: u32,
        bytes: Box<[u8]>,
        end: bool,
    ) -> Result<(), CandidateEndpointError> {
        let ordinal = self.next_frame_ordinal;
        let first_record_ordinal = self.next_record_ordinal;
        let next_frame_ordinal = ordinal
            .checked_add(1)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let next_record_ordinal = first_record_ordinal
            .checked_add(record_count)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        let receipt = self
            .transport
            .as_ref()
            .ok_or(CandidateEndpointError::InvalidState)?
            .receipt();
        if receipt
            .encoded_frame_bytes
            .checked_add(
                u32::try_from(bytes.len()).map_err(|_| CandidateEndpointError::MetricOverflow)?,
            )
            .ok_or(CandidateEndpointError::MetricOverflow)?
            > self.offer.limits.maximum_encoded_frame_bytes
            || next_frame_ordinal > self.offer.limits.maximum_frame_count
        {
            return Err(CandidateEndpointError::ViewportInlineLimitExceeded(
                "publication transport",
            ));
        }
        let digest256 = self
            .transport
            .as_mut()
            .ok_or(CandidateEndpointError::InvalidState)?
            .push(ordinal, first_record_ordinal, record_count, kind, &bytes)?;
        self.packet.push(
            ordinal,
            first_record_ordinal,
            record_count,
            protocol_digest128_from_blake3(
                ProtocolDigestDomain::ViewportPresentationFrame,
                digest256,
            ),
            bytes,
            end,
        )?;
        self.next_frame_ordinal = next_frame_ordinal;
        self.next_record_ordinal = next_record_ordinal;
        Ok(())
    }

    fn finish_frame_stream(&mut self) -> Result<(), CandidateEndpointError> {
        let expected_records = self
            .offer
            .envelope
            .ordered_leaf_count
            .checked_add(self.offer.envelope.transferred_node_count)
            .ok_or(CandidateEndpointError::MetricOverflow)?;
        if self.next_record_ordinal != expected_records
            || self.next_frame_ordinal != self.offer.limits.maximum_frame_count
            || self.active.is_some()
            || !self.pending.is_empty()
            || self.releasing.is_some()
        {
            return Err(CandidateEndpointError::InvalidAuthority);
        }
        let transport = self
            .transport
            .take()
            .ok_or(CandidateEndpointError::InvalidState)?
            .finish()?;
        if transport.frame_count != self.next_frame_ordinal
            || transport.encoded_frame_bytes > self.offer.limits.maximum_encoded_frame_bytes
        {
            return Err(CandidateEndpointError::InvalidState);
        }
        let root_digest256 = viewport_presentation_root_stream_digest256(
            self.offer.envelope.aggregate_envelope_digest256,
            transport,
        );
        let root_stream_digest = protocol_digest128_from_blake3(
            ProtocolDigestDomain::ViewportPresentationRootStream,
            root_digest256,
        );
        self.commit = Some(ViewportPresentationCommitRequest {
            offer_id: self.offer.offer_id,
            actual_frame_count: transport.frame_count,
            actual_encoded_frame_bytes: transport.encoded_frame_bytes,
            rolling_transport_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::ViewportPresentationTransport,
                transport.digest256,
            ),
            aggregate_root_stream_digest: root_stream_digest,
        });
        self.expected_ack = Some(ViewportPresentationAck {
            publication_session: self.offer.publication_session,
            base_ack: self.offer.base_ack,
            binding: self.offer.binding,
            envelope: self.offer.envelope,
            actual_frame_count: transport.frame_count,
            actual_encoded_frame_bytes: transport.encoded_frame_bytes,
            aggregate_root_stream_digest: root_stream_digest,
        });
        Ok(())
    }

    fn take_packet_event(
        &mut self,
    ) -> Result<CandidateViewportPresentationEvent, CandidateEndpointError> {
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
        Ok(CandidateViewportPresentationEvent {
            credit: ViewportPresentationCredit::Packet {
                first_frame_ordinal,
                frame_count,
                end,
            },
            body: CandidateViewportPresentationEventBody::Packet { encoded },
        })
    }
}

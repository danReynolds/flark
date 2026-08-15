//! Independent host-side owner for one bounded aggregate VPB1 page.
//!
//! VPB1 public wrappers are validated in one ordered pass. Their embedded HIO1
//! bytes never become public query bytes: each complete child is independently
//! imported into its own `M11HostInlineSidecar`, then retained only as one
//! member of the bounded installed page.

use flark_engine::m11_host::{
    M11CandidateHost, M11HostFrameKind, M11HostInlineLinkValues, M11HostInlineProjectionCursorPoll,
    M11HostInlineSidecar, M11HostInlineSidecarBase, M11HostInlineSidecarBinding,
    M11HostInlineSidecarDescriptor, M11HostInlineSidecarOwner, M11HostInlineSidecarQuery,
    M11HostLimits, M11_HOST_MAXIMUM_FRAME_BYTES, M11_HOST_MAXIMUM_PROGRAM_CHILDREN,
};
use flark_parser::{M11_INLINE_FACT_RECORD_BYTES, M11_INLINE_META_RECORD_BYTES};

use super::*;
use crate::v3_publication_wire::{
    decode_viewport_presentation_child_frame, decode_viewport_presentation_directory,
    decode_viewport_presentation_end_frame, decode_viewport_presentation_parent_frame,
    encode_viewport_presentation_parent_frame_into, protocol_digest128_from_blake3,
    viewport_presentation_aggregate_envelope_digest256,
    viewport_presentation_root_stream_digest256, HotInlineSidecarDisposition,
    HotInlineSidecarOwner, ProtocolDigestDomain, ViewportPresentationAck,
    ViewportPresentationBegin, ViewportPresentationCommitRequest,
    ViewportPresentationDirectoryEntry, ViewportPresentationFrameKind,
    ViewportPresentationTransportDigest, IPR3_DESCRIPTOR_BYTES,
    MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES, MAXIMUM_PACKET_ENCODED_BYTES, MAXIMUM_PACKET_FRAME_COUNT,
    PACKET_FRAME_DESCRIPTOR_BYTES, PACKET_HEADER_BYTES, VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES,
};

pub(crate) const HOST_VIEWPORT_PRESENTATION_SCHEMA: u32 = 10;
pub(crate) const HOST_VIEWPORT_PRESENTATION_HEADER_BYTES: usize = 160;
pub(crate) const HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES: usize = 152;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HostViewportPresentationPollOutcome {
    Pending,
    PacketCredit {
        offer_id: Id128,
        next_frame_ordinal: u32,
    },
    Committed(ViewportPresentationAck),
    AbortComplete {
        offer_id: Id128,
    },
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HostViewportPresentationQueryOutcome {
    Available {
        encoded_bytes: u32,
        entry_count: u32,
    },
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RootPhase {
    Parent,
    Directory,
    Children,
    End,
    Complete,
}

struct ActiveViewportChild {
    directory_index: u32,
    entry: ViewportPresentationDirectoryEntry,
    binding: M11HostInlineSidecarBinding,
    sidecar: M11HostInlineSidecar,
    next_frame_ordinal: u32,
    next_node_ordinal: Option<u64>,
    accepted_node_count: u32,
    installing: bool,
}

struct InstalledViewportChild {
    entry: ViewportPresentationDirectoryEntry,
    binding: M11HostInlineSidecarBinding,
    sidecar: M11HostInlineSidecar,
}

struct ActiveViewportOffer {
    begin: ViewportPresentationBegin,
    base: M11HostInlineSidecarBase,
    engine_limits: M11HostLimits,
    maximum_query_bytes: u32,
    phase: OfferPhase,
    root_phase: RootPhase,
    transport: ViewportPresentationTransportDigest,
    next_frame_ordinal: u32,
    accepted_record_count: u32,
    accepted_frame_bytes: u32,
    pending_packet: Option<OwnedPacket>,
    directory: Vec<ViewportPresentationDirectoryEntry>,
    children: Vec<InstalledViewportChild>,
    current_child: Option<ActiveViewportChild>,
    aggregate_root_stream_digest256: Option<[u8; 32]>,
    commit: Option<ViewportPresentationCommitRequest>,
}

struct InstalledViewportPage {
    begin: ViewportPresentationBegin,
    ack: ViewportPresentationAck,
    maximum_query_bytes: u32,
    children: Vec<InstalledViewportChild>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PublicPayloadKind {
    Inline,
    IndentedCode,
    BlockQuote,
    BulletList,
    OrderedListItem,
    Unsupported,
}

impl PublicPayloadKind {
    const fn wire(self) -> u8 {
        match self {
            Self::Inline => M11_LEAF_PROJECTION_PAYLOAD_INLINE,
            Self::IndentedCode => M11_LEAF_PROJECTION_PAYLOAD_INDENTED_CODE,
            Self::BlockQuote => M11_LEAF_PROJECTION_PAYLOAD_BLOCK_QUOTE,
            Self::BulletList => M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST,
            Self::OrderedListItem => M11_LEAF_PROJECTION_PAYLOAD_ORDERED_LIST_ITEM,
            Self::Unsupported => u8::MAX,
        }
    }

    const fn direct_sidecar_kind(self) -> Option<HostInlineSidecarPayloadKind> {
        match self {
            Self::Inline => Some(HostInlineSidecarPayloadKind::Inline),
            Self::IndentedCode => Some(HostInlineSidecarPayloadKind::IndentedCode),
            Self::BlockQuote => Some(HostInlineSidecarPayloadKind::BlockQuote),
            Self::BulletList => Some(HostInlineSidecarPayloadKind::BulletList),
            Self::OrderedListItem => Some(HostInlineSidecarPayloadKind::OrderedListItem),
            Self::Unsupported => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PublicPayloadPlan {
    kind: PublicPayloadKind,
    record_count: u32,
    offset: u32,
    length: u32,
    unsupported_reason: u32,
}

struct ReclaimChild {
    sidecar: M11HostInlineSidecar,
    begun: bool,
}

#[derive(Default)]
struct ViewportReclaim {
    children: Vec<ReclaimChild>,
    abort_offer: Option<Id128>,
}

pub(super) struct ViewportPresentationHost {
    active: Option<ActiveViewportOffer>,
    installed: Option<InstalledViewportPage>,
    pending_delivery_ack: Option<ViewportPresentationAck>,
    reclaim: Option<ViewportReclaim>,
    closing: bool,
    closed: bool,
}

impl ViewportPresentationHost {
    pub(super) const fn new() -> Self {
        Self {
            active: None,
            installed: None,
            pending_delivery_ack: None,
            reclaim: None,
            closing: false,
            closed: false,
        }
    }

    pub(super) fn has_foreground_work(&self) -> bool {
        self.active.is_some()
            || self
                .reclaim
                .as_ref()
                .is_some_and(|reclaim| reclaim.abort_offer.is_some())
    }

    pub(super) fn invalidate(&mut self) {
        self.pending_delivery_ack = None;
        let active = self.active.take();
        let installed = self.installed.take();
        self.queue_reclaim(active, installed, None);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn begin(
        &mut self,
        begin: ViewportPresentationBegin,
        installed_ack: StructuralAck,
        current: SourceVersion,
        base: M11HostInlineSidecarBase,
        engine_limits: M11HostLimits,
        maximum_query_bytes: u32,
    ) -> Result<(), HostStoreError> {
        if self.closing || self.closed {
            return Err(HostStoreError::new(
                HostRejectReason::Closed,
                "viewport host is closing",
            ));
        }
        if self.active.is_some()
            || self
                .reclaim
                .as_ref()
                .is_some_and(|reclaim| reclaim.abort_offer.is_some())
        {
            return Err(HostStoreError::new(
                HostRejectReason::Backpressure,
                "another viewport offer still owns host work",
            ));
        }
        if begin.base_ack != installed_ack || begin.base_ack.source_version != current {
            return Err(HostStoreError::new(
                HostRejectReason::BaseMismatch,
                "viewport offer does not bind the exact installed structural base",
            ));
        }
        let child_count = u64::from(begin.envelope.ordered_leaf_count);
        let transferred_nodes = u64::from(begin.envelope.transferred_node_count);
        let maximum_host_nodes = engine_limits
            .maximum_snapshot_nodes
            .min(u64::try_from(engine_limits.arena_max_slots).unwrap_or(u64::MAX));
        let maximum_root_frames = maximum_host_nodes
            .checked_add(child_count.saturating_mul(2))
            .and_then(|count| count.checked_add(3))
            .unwrap_or(u64::MAX);
        if begin.envelope.ordered_leaf_count > begin.query_limits.maximum_inline_leaves
            || begin.envelope.ordered_leaf_count > begin.limits.maximum_program_children
            || begin.envelope.ordered_leaf_count as usize
                > engine_limits.arena_max_children_per_node
            || transferred_nodes > maximum_host_nodes
            || u64::from(begin.limits.maximum_frame_count) > maximum_root_frames
            || u64::from(begin.limits.maximum_encoded_frame_bytes)
                > engine_limits.maximum_snapshot_wire_bytes
            || begin.limits.maximum_program_children as usize > M11_HOST_MAXIMUM_PROGRAM_CHILDREN
            || begin.limits.maximum_program_children as usize
                > engine_limits.arena_max_children_per_node
        {
            return Err(HostStoreError::new(
                HostRejectReason::ForegroundBoundExceeded,
                "viewport offer exceeds the bounded independent host envelope",
            ));
        }
        let mut parent = [0_u8; VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES];
        encode_viewport_presentation_parent_frame_into(begin, &mut parent).map_err(|_| {
            HostStoreError::new(
                HostRejectReason::ForegroundBoundExceeded,
                "viewport offer exceeds its validated public envelope",
            )
        })?;
        let child_capacity = usize::try_from(begin.envelope.ordered_leaf_count)
            .map_err(|_| HostStoreError::invalid("viewport child count exceeds this target"))?;
        let mut directory = Vec::new();
        let mut children = Vec::new();
        directory.try_reserve_exact(child_capacity).map_err(|_| {
            HostStoreError::new(
                HostRejectReason::AllocationFailed,
                "viewport directory allocation failed",
            )
        })?;
        children.try_reserve_exact(child_capacity).map_err(|_| {
            HostStoreError::new(
                HostRejectReason::AllocationFailed,
                "viewport child allocation failed",
            )
        })?;
        self.pending_delivery_ack = None;
        self.active = Some(ActiveViewportOffer {
            begin,
            base,
            engine_limits,
            maximum_query_bytes,
            phase: OfferPhase::Receiving,
            root_phase: RootPhase::Parent,
            transport: ViewportPresentationTransportDigest::new(),
            next_frame_ordinal: 0,
            accepted_record_count: 0,
            accepted_frame_bytes: 0,
            pending_packet: None,
            directory,
            children,
            current_child: None,
            aggregate_root_stream_digest256: None,
            commit: None,
        });
        Ok(())
    }

    pub(super) fn admit_packet(
        &mut self,
        packet: PublicationPacket<'_>,
    ) -> Result<(), HostStoreError> {
        let active = self.active.as_mut().ok_or_else(|| {
            HostStoreError::new(HostRejectReason::WrongOffer, "no active viewport offer")
        })?;
        if packet.offer_id != active.begin.offer_id {
            return Err(HostStoreError::new(
                HostRejectReason::WrongOffer,
                "viewport packet belongs to another offer",
            ));
        }
        if active.phase != OfferPhase::Receiving || active.pending_packet.is_some() {
            return Err(HostStoreError::new(
                HostRejectReason::Backpressure,
                "the prior viewport packet has not returned credit",
            ));
        }
        let limits = active.begin.limits;
        let packet_bytes = u32::try_from(packet.encoded().len()).map_err(|_| {
            HostStoreError::new(
                HostRejectReason::ForegroundBoundExceeded,
                "viewport packet length exceeds this target",
            )
        })?;
        let maximum_records = active
            .begin
            .envelope
            .ordered_leaf_count
            .checked_add(active.begin.envelope.transferred_node_count)
            .ok_or_else(|| HostStoreError::invalid("viewport record limit overflowed"))?;
        let next_frame_ordinal = packet.first_frame_ordinal.checked_add(packet.frame_count);
        let next_record_ordinal = packet
            .first_record_ordinal
            .checked_add(packet.aggregate_record_count);
        let next_frame_bytes = active
            .accepted_frame_bytes
            .checked_add(packet.aggregate_frame_bytes);
        if packet.first_frame_ordinal != active.next_frame_ordinal
            || packet.first_record_ordinal != active.accepted_record_count
            || packet.frame_count == 0
            || packet.frame_count > MAXIMUM_PACKET_FRAME_COUNT
            || packet.aggregate_frame_bytes > MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES
            || packet.encoded().len() > MAXIMUM_PACKET_ENCODED_BYTES
            || packet_bytes > limits.maximum_packet_bytes
            || next_frame_ordinal.is_none_or(|next| next > limits.maximum_frame_count)
            || next_record_ordinal.is_none_or(|next| next > maximum_records)
            || next_frame_bytes.is_none_or(|next| {
                next > limits.maximum_encoded_frame_bytes
                    || next > active.begin.query_limits.maximum_encoded_frame_bytes
            })
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "viewport packet order or aggregate envelope changed",
            ));
        }
        let bytes = copy_packet_storage(packet.encoded())?;
        active.pending_packet = Some(OwnedPacket {
            offer_id: packet.offer_id,
            first_frame_ordinal: packet.first_frame_ordinal,
            first_record_ordinal: packet.first_record_ordinal,
            frame_count: packet.frame_count,
            aggregate_record_count: packet.aggregate_record_count,
            aggregate_frame_bytes: packet.aggregate_frame_bytes,
            first_accepted_frame_bytes: active.accepted_frame_bytes,
            next_index: 0,
            directory_offset: 0,
            body_offset: 0,
            next_record_ordinal: packet.first_record_ordinal,
            end_range: None,
            bytes,
        });
        Ok(())
    }

    pub(super) fn request_commit(
        &mut self,
        request: ViewportPresentationCommitRequest,
    ) -> Result<(), HostStoreError> {
        let active = self.active.as_mut().ok_or_else(|| {
            HostStoreError::new(HostRejectReason::WrongOffer, "no active viewport offer")
        })?;
        if request.offer_id != active.begin.offer_id {
            return Err(HostStoreError::new(
                HostRejectReason::WrongOffer,
                "viewport commit belongs to another offer",
            ));
        }
        if active.phase != OfferPhase::AwaitingCommit
            || active.pending_packet.is_some()
            || active.current_child.is_some()
            || active.root_phase != RootPhase::Complete
        {
            return Err(HostStoreError::new(
                HostRejectReason::Invalid,
                "viewport commit arrived before one complete credited page",
            ));
        }
        let transport = active.transport.receipt();
        let root_digest = active
            .aggregate_root_stream_digest256
            .ok_or_else(|| HostStoreError::invalid("viewport End claim is missing"))?;
        let child_count = u32::try_from(active.children.len())
            .map_err(|_| HostStoreError::invalid("viewport child count overflowed"))?;
        if request.actual_frame_count != transport.frame_count
            || request.actual_encoded_frame_bytes != transport.encoded_frame_bytes
            || request.actual_frame_count != active.next_frame_ordinal
            || request.actual_encoded_frame_bytes != active.accepted_frame_bytes
            || child_count != active.begin.envelope.ordered_leaf_count
            || request.rolling_transport_digest
                != protocol_digest128_from_blake3(
                    ProtocolDigestDomain::ViewportPresentationTransport,
                    transport.digest256,
                )
            || request.aggregate_root_stream_digest
                != protocol_digest128_from_blake3(
                    ProtocolDigestDomain::ViewportPresentationRootStream,
                    root_digest,
                )
        {
            active.phase = OfferPhase::Failed;
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "viewport commit totals or exact digests changed",
            ));
        }
        active.commit = Some(request);
        active.phase = OfferPhase::Installing;
        Ok(())
    }

    pub(super) fn abort(&mut self, offer_id: Id128) -> Result<(), HostStoreError> {
        let active = self.active.take().ok_or_else(|| {
            HostStoreError::new(HostRejectReason::WrongOffer, "no active viewport offer")
        })?;
        if active.begin.offer_id != offer_id {
            self.active = Some(active);
            return Err(HostStoreError::new(
                HostRejectReason::WrongOffer,
                "viewport abort belongs to another offer",
            ));
        }
        self.queue_reclaim(Some(active), None, Some(offer_id));
        Ok(())
    }

    pub(super) fn poll(
        &mut self,
        grant: HostWorkGrant,
    ) -> Result<HostViewportPresentationPollOutcome, HostStoreError> {
        if self.closed {
            return Ok(HostViewportPresentationPollOutcome::Closed);
        }
        if self.closing {
            self.poll_reclaim(grant.transitions)?;
            return if self.reclaim.is_none() {
                self.closed = true;
                Ok(HostViewportPresentationPollOutcome::Closed)
            } else {
                Ok(HostViewportPresentationPollOutcome::Pending)
            };
        }
        if self.reclaim.is_some() {
            let abort = self.poll_reclaim(grant.transitions)?;
            if let Some(offer_id) = abort {
                return Ok(HostViewportPresentationPollOutcome::AbortComplete { offer_id });
            }
            return Ok(HostViewportPresentationPollOutcome::Pending);
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.pending_packet.is_some())
        {
            return self.poll_packet(grant);
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.phase == OfferPhase::Installing)
        {
            if grant.transitions == 0 {
                return Ok(HostViewportPresentationPollOutcome::Pending);
            }
            return self.finish_install();
        }
        Ok(HostViewportPresentationPollOutcome::Pending)
    }

    pub(super) fn acknowledge_delivery(
        &mut self,
        ack: ViewportPresentationAck,
    ) -> Result<(), HostStoreError> {
        if self.pending_delivery_ack != Some(ack) {
            return Err(HostStoreError::new(
                HostRejectReason::Invalid,
                "viewport delivery proof does not match the installed ACK",
            ));
        }
        self.pending_delivery_ack = None;
        Ok(())
    }

    fn poll_packet(
        &mut self,
        grant: HostWorkGrant,
    ) -> Result<HostViewportPresentationPollOutcome, HostStoreError> {
        let mut packet = self
            .active
            .as_mut()
            .and_then(|active| active.pending_packet.take())
            .ok_or_else(|| HostStoreError::invalid("pending viewport packet disappeared"))?;
        match self.process_packet(&mut packet, grant) {
            Ok(false) => {
                self.active
                    .as_mut()
                    .ok_or_else(|| HostStoreError::invalid("active viewport offer disappeared"))?
                    .pending_packet = Some(packet);
                Ok(HostViewportPresentationPollOutcome::Pending)
            }
            Ok(true) => {
                let active = self
                    .active
                    .as_ref()
                    .ok_or_else(|| HostStoreError::invalid("active viewport offer disappeared"))?;
                Ok(HostViewportPresentationPollOutcome::PacketCredit {
                    offer_id: packet.offer_id,
                    next_frame_ordinal: active.next_frame_ordinal,
                })
            }
            Err(error) => {
                if let Some(active) = self.active.as_mut() {
                    active.phase = OfferPhase::Failed;
                    active.pending_packet = None;
                }
                Err(error)
            }
        }
    }

    fn process_packet(
        &mut self,
        packet: &mut OwnedPacket,
        mut grant: HostWorkGrant,
    ) -> Result<bool, HostStoreError> {
        let directory_bytes = usize::try_from(packet.frame_count)
            .ok()
            .and_then(|count| count.checked_mul(PACKET_FRAME_DESCRIPTOR_BYTES))
            .ok_or_else(|| HostStoreError::invalid("viewport packet directory overflowed"))?;
        let body_start = PACKET_HEADER_BYTES
            .checked_add(directory_bytes)
            .ok_or_else(|| HostStoreError::invalid("viewport packet body offset overflowed"))?;
        let expected_packet_bytes = body_start
            .checked_add(packet.aggregate_frame_bytes as usize)
            .ok_or_else(|| HostStoreError::invalid("viewport packet envelope overflowed"))?;
        if packet.bytes.len() != expected_packet_bytes {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "copied viewport packet envelope changed",
            ));
        }

        loop {
            if self
                .active
                .as_ref()
                .and_then(|active| active.current_child.as_ref())
                .is_some_and(|child| child.installing)
            {
                if grant.transitions == 0 {
                    return Ok(false);
                }
                let consumed = self.poll_current_child_install()?;
                if consumed == 0 {
                    return Ok(false);
                }
                grant.transitions = grant.transitions.saturating_sub(consumed);
                continue;
            }
            if packet.next_index >= packet.frame_count || grant.transitions == 0 {
                break;
            }
            if grant.inspect_bytes < PACKET_FRAME_DESCRIPTOR_BYTES as u32 {
                break;
            }
            let descriptor_start = PACKET_HEADER_BYTES
                .checked_add(packet.directory_offset)
                .ok_or_else(|| HostStoreError::invalid("viewport descriptor offset overflowed"))?;
            let descriptor_end = descriptor_start
                .checked_add(PACKET_FRAME_DESCRIPTOR_BYTES)
                .filter(|end| *end <= body_start)
                .ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "viewport descriptor table ended early",
                    )
                })?;
            let descriptor = &packet.bytes[descriptor_start..descriptor_end];
            let frame_bytes = read_u32(descriptor, 0);
            let record_count = read_u32(descriptor, 4);
            let digest = [
                read_u32(descriptor, 8),
                read_u32(descriptor, 12),
                read_u32(descriptor, 16),
                read_u32(descriptor, 20),
            ];
            let maximum_frame_bytes = self
                .active
                .as_ref()
                .ok_or_else(|| HostStoreError::invalid("active viewport offer disappeared"))?
                .begin
                .limits
                .maximum_frame_bytes;
            if frame_bytes == 0 || frame_bytes > maximum_frame_bytes {
                return Err(HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "viewport frame exceeds its admitted public envelope",
                ));
            }
            let frame_start = body_start
                .checked_add(packet.body_offset)
                .ok_or_else(|| HostStoreError::invalid("viewport frame offset overflowed"))?;
            let frame_end = frame_start
                .checked_add(frame_bytes as usize)
                .filter(|end| *end <= packet.bytes.len())
                .ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "viewport frame lengths exceed the aggregate body",
                    )
                })?;
            let inspect_bytes = frame_bytes
                .checked_add(PACKET_FRAME_DESCRIPTOR_BYTES as u32)
                .ok_or_else(|| HostStoreError::invalid("viewport inspection fuel overflowed"))?;
            if grant.inspect_bytes < inspect_bytes || grant.copy_bytes < frame_bytes {
                break;
            }
            let ordinal = packet
                .first_frame_ordinal
                .checked_add(packet.next_index)
                .ok_or_else(|| HostStoreError::invalid("viewport frame ordinal overflowed"))?;
            let next_record_ordinal = packet
                .next_record_ordinal
                .checked_add(record_count)
                .ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "viewport record ordinals overflowed",
                    )
                })?;
            self.process_frame(PendingFrame {
                offer_id: packet.offer_id,
                ordinal,
                first_record_ordinal: packet.next_record_ordinal,
                record_count,
                digest,
                bytes: &packet.bytes[frame_start..frame_end],
            })?;
            packet.next_index += 1;
            packet.directory_offset += PACKET_FRAME_DESCRIPTOR_BYTES;
            packet.body_offset += frame_bytes as usize;
            packet.next_record_ordinal = next_record_ordinal;
            grant.inspect_bytes -= inspect_bytes;
            grant.copy_bytes -= frame_bytes;
            grant.transitions -= 1;
        }

        if self
            .active
            .as_ref()
            .and_then(|active| active.current_child.as_ref())
            .is_some_and(|child| child.installing)
        {
            return Ok(false);
        }
        if packet.next_index < packet.frame_count {
            return Ok(false);
        }
        let expected_records = packet
            .first_record_ordinal
            .checked_add(packet.aggregate_record_count)
            .ok_or_else(|| HostStoreError::invalid("viewport record aggregate overflowed"))?;
        let expected_frame_bytes = packet
            .first_accepted_frame_bytes
            .checked_add(packet.aggregate_frame_bytes)
            .ok_or_else(|| HostStoreError::invalid("viewport byte aggregate overflowed"))?;
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| HostStoreError::invalid("active viewport offer disappeared"))?;
        if packet.directory_offset != directory_bytes
            || packet.body_offset != packet.aggregate_frame_bytes as usize
            || packet.next_record_ordinal != expected_records
            || active.next_frame_ordinal
                != packet
                    .first_frame_ordinal
                    .checked_add(packet.frame_count)
                    .ok_or_else(|| HostStoreError::invalid("viewport frame aggregate overflowed"))?
            || active.accepted_record_count != expected_records
            || active.accepted_frame_bytes != expected_frame_bytes
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "viewport packet descriptor aggregates changed",
            ));
        }
        Ok(true)
    }

    fn process_frame(&mut self, frame: PendingFrame<'_>) -> Result<(), HostStoreError> {
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| HostStoreError::invalid("active viewport offer disappeared"))?;
        let byte_len = u32::try_from(frame.bytes.len()).map_err(|_| {
            HostStoreError::new(
                HostRejectReason::ForegroundBoundExceeded,
                "viewport frame length exceeds this target",
            )
        })?;
        if frame.offer_id != active.begin.offer_id
            || frame.ordinal != active.next_frame_ordinal
            || frame.first_record_ordinal != active.accepted_record_count
            || byte_len > active.begin.limits.maximum_frame_bytes
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "viewport frame metadata disagrees with its offer",
            ));
        }
        let kind = match active.root_phase {
            RootPhase::Parent => ViewportPresentationFrameKind::Begin,
            RootPhase::Directory => ViewportPresentationFrameKind::Directory,
            RootPhase::Children => ViewportPresentationFrameKind::Child,
            RootPhase::End => ViewportPresentationFrameKind::End,
            RootPhase::Complete => {
                return Err(HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "viewport root carried frames after End",
                ));
            }
        };
        let digest256 = active
            .transport
            .push(
                frame.ordinal,
                frame.first_record_ordinal,
                frame.record_count,
                kind,
                frame.bytes,
            )
            .map_err(|_| {
                HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "viewport transport sequence changed",
                )
            })?;
        if protocol_digest128_from_blake3(
            ProtocolDigestDomain::ViewportPresentationFrame,
            digest256,
        ) != frame.digest
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "viewport frame digest changed",
            ));
        }

        match kind {
            ViewportPresentationFrameKind::Begin => {
                if frame.record_count != 0 {
                    return Err(HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "viewport Parent claimed semantic records",
                    ));
                }
                decode_viewport_presentation_parent_frame(frame.bytes, active.begin)
                    .map_err(map_viewport_decode_error)?;
                active.root_phase = RootPhase::Directory;
            }
            ViewportPresentationFrameKind::Directory => {
                if frame.record_count != active.begin.envelope.ordered_leaf_count {
                    return Err(HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "viewport Directory record count changed",
                    ));
                }
                let directory = decode_viewport_presentation_directory(frame.bytes, active.begin)
                    .map_err(map_viewport_decode_error)?;
                let aggregate_digest = viewport_presentation_aggregate_envelope_digest256(
                    active.begin.binding,
                    active.begin.envelope,
                    directory.encoded(),
                )
                .map_err(map_viewport_decode_error)?;
                if aggregate_digest != active.begin.envelope.aggregate_envelope_digest256 {
                    return Err(HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "viewport aggregate envelope digest changed",
                    ));
                }
                active.directory.extend(directory.entries());
                validate_public_page_bound(active)?;
                active.root_phase = if active.directory.is_empty() {
                    RootPhase::End
                } else {
                    RootPhase::Children
                };
            }
            ViewportPresentationFrameKind::Child => {
                process_child_frame(active, frame.bytes)?;
            }
            ViewportPresentationFrameKind::End => {
                if frame.record_count != 0
                    || active.current_child.is_some()
                    || active.children.len() != active.directory.len()
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "viewport End arrived before every child installed",
                    ));
                }
                let end = decode_viewport_presentation_end_frame(frame.bytes, active.begin)
                    .map_err(map_viewport_decode_error)?;
                let receipt = active.transport.receipt();
                if end.actual_frame_count != receipt.frame_count
                    || end.actual_encoded_frame_bytes != receipt.encoded_frame_bytes
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "viewport End totals changed",
                    ));
                }
                active.aggregate_root_stream_digest256 =
                    Some(viewport_presentation_root_stream_digest256(
                        active.begin.envelope.aggregate_envelope_digest256,
                        receipt,
                    ));
                active.root_phase = RootPhase::Complete;
                active.phase = OfferPhase::AwaitingCommit;
            }
        }
        active.next_frame_ordinal = active
            .next_frame_ordinal
            .checked_add(1)
            .ok_or_else(|| HostStoreError::invalid("viewport frame ordinal overflowed"))?;
        active.accepted_record_count = active
            .accepted_record_count
            .checked_add(frame.record_count)
            .ok_or_else(|| HostStoreError::invalid("viewport record count overflowed"))?;
        active.accepted_frame_bytes = active
            .accepted_frame_bytes
            .checked_add(byte_len)
            .ok_or_else(|| HostStoreError::invalid("viewport byte count overflowed"))?;
        Ok(())
    }

    fn poll_current_child_install(&mut self) -> Result<u32, HostStoreError> {
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| HostStoreError::invalid("active viewport offer disappeared"))?;
        let mut child = active
            .current_child
            .take()
            .ok_or_else(|| HostStoreError::invalid("active viewport child disappeared"))?;
        if !child.installing {
            active.current_child = Some(child);
            return Ok(0);
        }
        let polled = child.sidecar.poll_install(1).map_err(map_engine_error)?;
        let consumed = u32::try_from(polled.transitions)
            .map_err(|_| HostStoreError::invalid("viewport child install fuel overflowed"))?;
        if polled.transitions > 1 {
            return Err(HostStoreError::new(
                HostRejectReason::InternalFault,
                "viewport child install exceeded its one-transition grant",
            ));
        }
        if !polled.installed {
            active.current_child = Some(child);
            return Ok(consumed);
        }
        active.children.push(InstalledViewportChild {
            entry: child.entry,
            binding: child.binding,
            sidecar: child.sidecar,
        });
        active.root_phase = if active.children.len() == active.directory.len() {
            RootPhase::End
        } else {
            RootPhase::Children
        };
        Ok(consumed.max(1))
    }

    fn finish_install(&mut self) -> Result<HostViewportPresentationPollOutcome, HostStoreError> {
        let active = self
            .active
            .take()
            .ok_or_else(|| HostStoreError::invalid("installed viewport offer disappeared"))?;
        let commit = active
            .commit
            .ok_or_else(|| HostStoreError::invalid("installed viewport lost commit proof"))?;
        let ack = ViewportPresentationAck {
            publication_session: active.begin.publication_session,
            base_ack: active.begin.base_ack,
            binding: active.begin.binding,
            envelope: active.begin.envelope,
            actual_frame_count: commit.actual_frame_count,
            actual_encoded_frame_bytes: commit.actual_encoded_frame_bytes,
            aggregate_root_stream_digest: commit.aggregate_root_stream_digest,
        };
        let prior = self.installed.replace(InstalledViewportPage {
            begin: active.begin,
            ack,
            maximum_query_bytes: active.maximum_query_bytes,
            children: active.children,
        });
        self.pending_delivery_ack = Some(ack);
        if let Some(prior) = prior {
            self.queue_reclaim(None, Some(prior), None);
        }
        Ok(HostViewportPresentationPollOutcome::Committed(ack))
    }

    fn queue_reclaim(
        &mut self,
        active: Option<ActiveViewportOffer>,
        installed: Option<InstalledViewportPage>,
        abort_offer: Option<Id128>,
    ) {
        let reclaim = self.reclaim.get_or_insert_with(ViewportReclaim::default);
        if reclaim.abort_offer.is_none() {
            reclaim.abort_offer = abort_offer;
        }
        if let Some(mut active) = active {
            active.pending_packet = None;
            if let Some(child) = active.current_child.take() {
                reclaim.children.push(ReclaimChild {
                    sidecar: child.sidecar,
                    begun: false,
                });
            }
            reclaim
                .children
                .extend(active.children.drain(..).map(|child| ReclaimChild {
                    sidecar: child.sidecar,
                    begun: false,
                }));
        }
        if let Some(mut installed) = installed {
            reclaim
                .children
                .extend(installed.children.drain(..).map(|child| ReclaimChild {
                    sidecar: child.sidecar,
                    begun: false,
                }));
        }
        if reclaim.children.is_empty() && reclaim.abort_offer.is_none() {
            self.reclaim = None;
        }
    }

    fn poll_reclaim(&mut self, transitions: u32) -> Result<Option<Id128>, HostStoreError> {
        if transitions == 0 {
            return Ok(None);
        }
        let Some(reclaim) = self.reclaim.as_mut() else {
            return Ok(None);
        };
        let mut remaining = transitions;
        while remaining > 0 {
            let Some(child) = reclaim.children.last_mut() else {
                let abort = reclaim.abort_offer.take();
                self.reclaim = None;
                return Ok(abort);
            };
            if !child.begun {
                child.sidecar.begin_close().map_err(map_engine_error)?;
                child.begun = true;
                remaining -= 1;
                continue;
            }
            if child.sidecar.poll_close(1).map_err(map_engine_error)? {
                reclaim.children.pop();
            }
            remaining -= 1;
        }
        Ok(None)
    }

    pub(super) fn begin_close(&mut self) {
        if self.closed || self.closing {
            return;
        }
        self.closing = true;
        self.pending_delivery_ack = None;
        let active = self.active.take();
        let installed = self.installed.take();
        self.queue_reclaim(active, installed, None);
        if self.reclaim.is_none() {
            self.closed = true;
        }
    }

    pub(super) fn poll_close(&mut self, transitions: u32) -> Result<bool, HostStoreError> {
        if self.closed {
            return Ok(true);
        }
        if !self.closing {
            return Ok(false);
        }
        self.poll_reclaim(transitions)?;
        if self.reclaim.is_none() {
            self.closed = true;
        }
        Ok(self.closed)
    }

    pub(super) fn query(
        &self,
        ack: ViewportPresentationAck,
        installed_ack: StructuralAck,
        maximum_encoded_bytes: u32,
        output: &mut [u8],
    ) -> Result<HostViewportPresentationQueryOutcome, HostStoreError> {
        let Some(installed) = self.installed.as_ref() else {
            return Ok(HostViewportPresentationQueryOutcome::Unavailable);
        };
        if ack != installed.ack || ack.base_ack != installed_ack {
            return Ok(HostViewportPresentationQueryOutcome::Unavailable);
        }

        let directory_bytes = installed
            .children
            .len()
            .checked_mul(HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES)
            .ok_or_else(|| HostStoreError::invalid("viewport query directory overflowed"))?;
        let payload_start = HOST_VIEWPORT_PRESENTATION_HEADER_BYTES
            .checked_add(directory_bytes)
            .ok_or_else(|| HostStoreError::invalid("viewport query header overflowed"))?;
        let mut payload_cursor = u32::try_from(payload_start)
            .map_err(|_| HostStoreError::invalid("viewport query offset exceeds u32"))?;
        let mut plans = Vec::new();
        plans
            .try_reserve_exact(installed.children.len())
            .map_err(|_| {
                HostStoreError::new(
                    HostRejectReason::AllocationFailed,
                    "viewport query plan allocation failed",
                )
            })?;
        for child in &installed.children {
            let mut plan = plan_public_payload(child)?;
            plan.offset = payload_cursor;
            payload_cursor = payload_cursor.checked_add(plan.length).ok_or_else(|| {
                HostStoreError::invalid("viewport public payload offsets overflowed")
            })?;
            plans.push(plan);
        }
        let encoded_bytes = usize::try_from(payload_cursor)
            .map_err(|_| HostStoreError::invalid("viewport query length exceeds this target"))?;
        let admitted_maximum = usize::try_from(installed.maximum_query_bytes)
            .map_err(|_| HostStoreError::invalid("viewport host query bound overflowed"))?;
        let requested_maximum = usize::try_from(maximum_encoded_bytes)
            .map_err(|_| HostStoreError::invalid("viewport caller query bound overflowed"))?;
        if encoded_bytes > admitted_maximum
            || encoded_bytes > requested_maximum
            || encoded_bytes > output.len()
            || encoded_bytes > installed.begin.query_limits.maximum_encoded_frame_bytes as usize
        {
            return Err(HostStoreError::new(
                HostRejectReason::QueryBoundExceeded,
                "viewport aggregate page exceeds the caller or host query bound",
            ));
        }

        let page = &mut output[..encoded_bytes];
        page.fill(0);
        for (child, plan) in installed.children.iter().zip(&plans) {
            let start = usize::try_from(plan.offset)
                .map_err(|_| HostStoreError::invalid("viewport payload offset exceeds target"))?;
            let end = start
                .checked_add(plan.length as usize)
                .ok_or_else(|| HostStoreError::invalid("viewport payload range overflowed"))?;
            encode_public_payload(child, *plan, &mut page[start..end])?;
        }
        write_public_directory(page, installed, &plans)?;
        write_public_header(page, installed, payload_start)?;
        let digest = public_page_digest256(installed.ack, page);
        page[128..160].copy_from_slice(&digest);

        Ok(HostViewportPresentationQueryOutcome::Available {
            encoded_bytes: payload_cursor,
            entry_count: u32::try_from(installed.children.len())
                .map_err(|_| HostStoreError::invalid("viewport entry count exceeds u32"))?,
        })
    }
}

fn plan_public_payload(
    child: &InstalledViewportChild,
) -> Result<PublicPayloadPlan, HostStoreError> {
    let query = child
        .sidecar
        .query(&child.binding)
        .map_err(map_engine_error)?
        .ok_or_else(|| {
            HostStoreError::new(
                HostRejectReason::InternalFault,
                "installed viewport child is not queryable",
            )
        })?;
    match query {
        M11HostInlineSidecarQuery::ProjectedInline { .. } => Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "projected-inline sidecars require the direct query lane",
        )),
        M11HostInlineSidecarQuery::Authoritative {
            descriptor,
            link_values,
            ..
        } => {
            let mut plan = authoritative_payload_plan(
                child,
                PublicPayloadKind::Inline,
                M11_INLINE_FACT_RECORD_BYTES,
            )?;
            let (
                envelope_value_entry_count,
                envelope_value_encoded_bytes,
                envelope_value_storage_page_count,
            ) = match child.entry.hio1_envelope.disposition {
                HotInlineSidecarDisposition::Authoritative {
                    link_value_entry_count,
                    link_value_encoded_bytes,
                    link_value_storage_page_count,
                    ..
                } => (
                    link_value_entry_count,
                    link_value_encoded_bytes,
                    link_value_storage_page_count,
                ),
                HotInlineSidecarDisposition::Unsupported { .. } => {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "viewport child engine and wire dispositions disagree",
                    ));
                }
            };
            if descriptor.fact_count() != u64::from(plan.record_count)
                || descriptor.link_value_entry_count() != envelope_value_entry_count
                || descriptor.link_value_encoded_bytes() != envelope_value_encoded_bytes
                || descriptor.link_value_storage_page_count() != envelope_value_storage_page_count
                || link_values.entry_count() != envelope_value_entry_count
                || link_values.encoded_bytes() != envelope_value_encoded_bytes
            {
                return Err(HostStoreError::new(
                    HostRejectReason::InternalFault,
                    "viewport inline value envelope changed after installation",
                ));
            }
            plan.length = plan
                .length
                .checked_add(envelope_value_encoded_bytes)
                .ok_or_else(|| {
                    HostStoreError::invalid("viewport inline payload length overflowed")
                })?;
            Ok(plan)
        }
        M11HostInlineSidecarQuery::IndentedCode { .. } => authoritative_payload_plan(
            child,
            PublicPayloadKind::IndentedCode,
            M11_INDENTED_CODE_LINE_RECORD_BYTES,
        ),
        M11HostInlineSidecarQuery::BlockQuote { .. } => authoritative_payload_plan(
            child,
            PublicPayloadKind::BlockQuote,
            M11_BLOCK_QUOTE_LINE_RECORD_BYTES,
        ),
        M11HostInlineSidecarQuery::BulletList { .. } => authoritative_payload_plan(
            child,
            PublicPayloadKind::BulletList,
            M11_BULLET_LIST_ITEM_RECORD_BYTES,
        ),
        M11HostInlineSidecarQuery::OrderedList { .. } => {
            let mut plan = authoritative_payload_plan(
                child,
                PublicPayloadKind::OrderedListItem,
                M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES,
            )?;
            if plan.record_count != 1 {
                return Err(HostStoreError::new(
                    HostRejectReason::InternalFault,
                    "ordered-list viewport child lost its selected-item cardinality",
                ));
            }
            plan.length = M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES as u32;
            Ok(plan)
        }
        M11HostInlineSidecarQuery::Unsupported { metadata } => {
            let reason = match child.entry.hio1_envelope.disposition {
                HotInlineSidecarDisposition::Unsupported { reason, .. } => reason,
                HotInlineSidecarDisposition::Authoritative { .. } => {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "viewport child engine and wire dispositions disagree",
                    ));
                }
            };
            if metadata.len() > M11_INLINE_META_RECORD_BYTES {
                return Err(HostStoreError::new(
                    HostRejectReason::InternalFault,
                    "unsupported viewport child exceeds the public metadata bound",
                ));
            }
            Ok(PublicPayloadPlan {
                kind: PublicPayloadKind::Unsupported,
                record_count: 0,
                offset: 0,
                length: u32::try_from(metadata.len())
                    .map_err(|_| HostStoreError::invalid("viewport metadata length exceeds u32"))?,
                unsupported_reason: reason,
            })
        }
    }
}

fn authoritative_payload_plan(
    child: &InstalledViewportChild,
    kind: PublicPayloadKind,
    record_bytes: usize,
) -> Result<PublicPayloadPlan, HostStoreError> {
    let fact_count = match child.entry.hio1_envelope.disposition {
        HotInlineSidecarDisposition::Authoritative { fact_count, .. } => fact_count,
        HotInlineSidecarDisposition::Unsupported { .. } => {
            return Err(HostStoreError::new(
                HostRejectReason::InternalFault,
                "viewport child engine and wire dispositions disagree",
            ));
        }
    };
    let record_count = u32::try_from(fact_count)
        .map_err(|_| HostStoreError::invalid("viewport public record count exceeds u32"))?;
    let length = usize::try_from(fact_count)
        .ok()
        .and_then(|count| count.checked_mul(record_bytes))
        .and_then(|bytes| u32::try_from(bytes).ok())
        .ok_or_else(|| HostStoreError::invalid("viewport public payload length overflowed"))?;
    Ok(PublicPayloadPlan {
        kind,
        record_count,
        offset: 0,
        length,
        unsupported_reason: 0,
    })
}

fn encode_public_payload(
    child: &InstalledViewportChild,
    plan: PublicPayloadPlan,
    output: &mut [u8],
) -> Result<(), HostStoreError> {
    if output.len() != plan.length as usize {
        return Err(HostStoreError::invalid(
            "viewport public payload slice disagrees with its plan",
        ));
    }
    let query = child
        .sidecar
        .query(&child.binding)
        .map_err(map_engine_error)?
        .ok_or_else(|| {
            HostStoreError::new(
                HostRejectReason::InternalFault,
                "installed viewport child disappeared during query",
            )
        })?;
    let outcome = match (plan.kind, query) {
        (
            PublicPayloadKind::Inline,
            M11HostInlineSidecarQuery::Authoritative {
                descriptor,
                mut cursor,
                link_values,
            },
        ) => encode_inline_payload(
            descriptor,
            &mut cursor,
            link_values,
            plan.record_count,
            output,
        )?,
        (
            PublicPayloadKind::IndentedCode,
            M11HostInlineSidecarQuery::IndentedCode {
                descriptor,
                mut cursor,
            },
        ) => encode_indented_code_payload(
            descriptor,
            &mut cursor,
            child.entry.hio1_envelope.disposition,
            child.entry.binding,
            output,
        )?,
        (
            PublicPayloadKind::BlockQuote,
            M11HostInlineSidecarQuery::BlockQuote { descriptor, cursor },
        ) => query_marked_line_sidecar(
            descriptor,
            cursor,
            child.entry.hio1_envelope.disposition,
            child.entry.binding,
            output,
            output.len(),
            HostMarkedLinePayloadKind::BlockQuote,
        )?,
        (
            PublicPayloadKind::BulletList,
            M11HostInlineSidecarQuery::BulletList {
                descriptor, cursor, ..
            },
        ) => query_marked_line_sidecar(
            descriptor,
            cursor,
            child.entry.hio1_envelope.disposition,
            child.entry.binding,
            output,
            output.len(),
            HostMarkedLinePayloadKind::BulletList,
        )?,
        (
            PublicPayloadKind::OrderedListItem,
            M11HostInlineSidecarQuery::OrderedList {
                selected_item_ordinal,
                selected_item_line_ending,
                opening_marker_start,
                opening_marker_end,
                marker_value,
                descriptor,
                cursor,
            },
        ) => query_ordered_list_item_sidecar(
            descriptor,
            cursor,
            child.entry.hio1_envelope.disposition,
            child.entry.binding,
            output,
            output.len(),
            selected_item_ordinal,
            selected_item_line_ending,
            opening_marker_start,
            opening_marker_end,
            marker_value,
        )?,
        (PublicPayloadKind::Unsupported, M11HostInlineSidecarQuery::Unsupported { metadata }) => {
            let reason = match child.entry.hio1_envelope.disposition {
                HotInlineSidecarDisposition::Unsupported { reason, .. } => reason,
                HotInlineSidecarDisposition::Authoritative { .. } => {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "viewport unsupported disposition changed during query",
                    ));
                }
            };
            if metadata.len() != output.len() {
                return Err(HostStoreError::new(
                    HostRejectReason::InternalFault,
                    "viewport unsupported metadata length changed during query",
                ));
            }
            output.copy_from_slice(metadata);
            HostInlineSidecarQueryOutcome::Unsupported {
                reason,
                metadata_bytes: metadata.len() as u32,
            }
        }
        _ => {
            return Err(HostStoreError::new(
                HostRejectReason::InternalFault,
                "viewport child public payload kind changed during query",
            ));
        }
    };
    let (expected_fact_count, expected_value_entry_count, expected_value_encoded_bytes) =
        match child.entry.hio1_envelope.disposition {
            HotInlineSidecarDisposition::Authoritative {
                fact_count,
                link_value_entry_count,
                link_value_encoded_bytes,
                ..
            } => (
                u32::try_from(fact_count)
                    .map_err(|_| HostStoreError::invalid("viewport fact count exceeds u32"))?,
                link_value_entry_count,
                link_value_encoded_bytes,
            ),
            HotInlineSidecarDisposition::Unsupported { .. } => (0, 0, 0),
        };
    match outcome {
        HostInlineSidecarQueryOutcome::Authoritative {
            payload_kind,
            fact_count,
            value_entry_count,
            value_encoded_bytes,
            encoded_bytes,
            ..
        } if plan.kind != PublicPayloadKind::Unsupported
            && Some(payload_kind) == plan.kind.direct_sidecar_kind()
            && fact_count == plan.record_count
            && fact_count == expected_fact_count
            && value_entry_count == expected_value_entry_count
            && value_encoded_bytes == expected_value_encoded_bytes
            && encoded_bytes == plan.length =>
        {
            Ok(())
        }
        HostInlineSidecarQueryOutcome::Unsupported {
            reason,
            metadata_bytes,
        } if plan.kind == PublicPayloadKind::Unsupported
            && reason == plan.unsupported_reason
            && metadata_bytes == plan.length =>
        {
            Ok(())
        }
        _ => Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "viewport public payload receipt changed after preflight",
        )),
    }
}

fn encode_inline_payload(
    descriptor: M11HostInlineSidecarDescriptor,
    cursor: &mut flark_engine::m11_host::M11HostInlineProjectionCursor<'_>,
    link_values: M11HostInlineLinkValues<'_>,
    expected_fact_count: u32,
    output: &mut [u8],
) -> Result<HostInlineSidecarQueryOutcome, HostStoreError> {
    let expected_fact_bytes = usize::try_from(expected_fact_count)
        .ok()
        .and_then(|count| count.checked_mul(M11_INLINE_FACT_RECORD_BYTES))
        .ok_or_else(|| HostStoreError::invalid("viewport inline payload length overflowed"))?;
    let expected_value_bytes = usize::try_from(descriptor.link_value_encoded_bytes())
        .map_err(|_| HostStoreError::invalid("viewport inline value length exceeds this target"))?;
    let expected_bytes = expected_fact_bytes
        .checked_add(expected_value_bytes)
        .ok_or_else(|| HostStoreError::invalid("viewport inline payload length overflowed"))?;
    if output.len() != expected_bytes {
        return Err(HostStoreError::invalid(
            "viewport inline payload slice changed after preflight",
        ));
    }
    if descriptor.fact_count() != u64::from(expected_fact_count)
        || descriptor.link_value_entry_count() != link_values.entry_count()
        || descriptor.link_value_encoded_bytes() != link_values.encoded_bytes()
    {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "viewport inline descriptor disagrees with its value lane",
        ));
    }
    let (fact_output, value_output) = output.split_at_mut(expected_fact_bytes);
    let mut fact_count = 0_usize;
    loop {
        match cursor.poll().map_err(map_engine_error)? {
            M11HostInlineProjectionCursorPoll::Fact(fact) => {
                let start = fact_count
                    .checked_mul(M11_INLINE_FACT_RECORD_BYTES)
                    .ok_or_else(|| HostStoreError::invalid("viewport inline offset overflowed"))?;
                let end = start
                    .checked_add(M11_INLINE_FACT_RECORD_BYTES)
                    .ok_or_else(|| HostStoreError::invalid("viewport inline offset overflowed"))?;
                let record = fact_output.get_mut(start..end).ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "viewport inline cursor exceeded its envelope",
                    )
                })?;
                encode_inline_projection_fact_record(fact, record)?;
                fact_count += 1;
            }
            M11HostInlineProjectionCursorPoll::Complete => break,
        }
    }
    if fact_count != expected_fact_count as usize {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "viewport inline cursor disagrees with its envelope",
        ));
    }
    let value_receipt = link_values.copy(value_output).map_err(map_engine_error)?;
    let tree_nodes_visited = cursor
        .tree_nodes_visited()
        .checked_add(value_receipt.tree_nodes_visited)
        .and_then(|visited| visited.checked_add(1))
        .ok_or_else(|| HostStoreError::invalid("viewport inline receipt overflowed"))?;
    if value_receipt.entry_count != descriptor.link_value_entry_count()
        || value_output.len() != expected_value_bytes
        || tree_nodes_visited > descriptor.maximum_tree_nodes_visited()
    {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "viewport inline cursor disagrees with its authenticated descriptor",
        ));
    }
    Ok(HostInlineSidecarQueryOutcome::Authoritative {
        payload_kind: HostInlineSidecarPayloadKind::Inline,
        fact_count: expected_fact_count,
        value_entry_count: value_receipt.entry_count,
        value_encoded_bytes: descriptor.link_value_encoded_bytes(),
        encoded_bytes: expected_bytes as u32,
        tree_nodes_visited: u32::try_from(tree_nodes_visited)
            .map_err(|_| HostStoreError::invalid("viewport inline receipt overflowed"))?,
    })
}

fn encode_indented_code_payload(
    descriptor: flark_engine::m11_host::M11HostIndentedCodeSidecarDescriptor,
    cursor: &mut flark_engine::m11_host::M11HostIndentedCodeCursor<'_>,
    disposition: HotInlineSidecarDisposition,
    binding: HotInlineSidecarBinding,
    output: &mut [u8],
) -> Result<HostInlineSidecarQueryOutcome, HostStoreError> {
    let (logical_page_count, line_count, storage_page_count, ordered_commitment256) =
        match disposition {
            HotInlineSidecarDisposition::Authoritative {
                logical_page_count,
                fact_count,
                storage_page_count,
                ordered_commitment256,
                ..
            } => (
                logical_page_count,
                fact_count,
                storage_page_count,
                ordered_commitment256,
            ),
            HotInlineSidecarDisposition::Unsupported { .. } => {
                return Err(HostStoreError::new(
                    HostRejectReason::InternalFault,
                    "viewport indented-code disposition changed during query",
                ));
            }
        };
    if descriptor.physical_start() != binding.physical_start_utf8
        || descriptor.physical_end() != binding.physical_end_utf8
        || descriptor.window_start() != binding.visible_start_utf8
        || descriptor.window_end() != binding.visible_end_utf8
        || descriptor.logical_page_count() != logical_page_count
        || descriptor.line_count() != line_count
        || descriptor.storage_page_count() != storage_page_count
        || descriptor.ordered_commitment256() != ordered_commitment256
        || descriptor.projection_flags() & !1 != 0
    {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "viewport indented-code descriptor disagrees with its envelope",
        ));
    }
    let expected_line_count = usize::try_from(line_count)
        .map_err(|_| HostStoreError::invalid("viewport line count exceeds this target"))?;
    if output.len()
        != expected_line_count
            .checked_mul(M11_INDENTED_CODE_LINE_RECORD_BYTES)
            .ok_or_else(|| HostStoreError::invalid("viewport line payload overflowed"))?
    {
        return Err(HostStoreError::invalid(
            "viewport indented-code payload slice changed after preflight",
        ));
    }
    let mut encoded_line_count = 0_usize;
    loop {
        match cursor.poll().map_err(map_engine_error)? {
            M11HostIndentedCodeCursorPoll::Line(line) => {
                let start = encoded_line_count
                    .checked_mul(M11_INDENTED_CODE_LINE_RECORD_BYTES)
                    .ok_or_else(|| HostStoreError::invalid("viewport line offset overflowed"))?;
                let end = start
                    .checked_add(M11_INDENTED_CODE_LINE_RECORD_BYTES)
                    .ok_or_else(|| HostStoreError::invalid("viewport line offset overflowed"))?;
                let record = output.get_mut(start..end).ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "viewport line cursor exceeded its envelope",
                    )
                })?;
                record[0..4].copy_from_slice(&line.relative_line_start().to_le_bytes());
                record[4..8].copy_from_slice(&line.physical_source_length().to_le_bytes());
                record[8..12].copy_from_slice(&line.hidden_prefix_length().to_le_bytes());
                record[12..16].copy_from_slice(&line.content_length().to_le_bytes());
                record[16..20].copy_from_slice(&line.flags().to_le_bytes());
                encoded_line_count += 1;
            }
            M11HostIndentedCodeCursorPoll::Complete => break,
        }
    }
    if encoded_line_count != expected_line_count {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "viewport line cursor disagrees with its envelope",
        ));
    }
    Ok(HostInlineSidecarQueryOutcome::Authoritative {
        payload_kind: HostInlineSidecarPayloadKind::IndentedCode,
        fact_count: line_count as u32,
        value_entry_count: 0,
        value_encoded_bytes: 0,
        encoded_bytes: output.len() as u32,
        tree_nodes_visited: u32::try_from(cursor.tree_nodes_visited())
            .map_err(|_| HostStoreError::invalid("viewport line receipt overflowed"))?,
    })
}

fn write_public_directory(
    page: &mut [u8],
    installed: &InstalledViewportPage,
    plans: &[PublicPayloadPlan],
) -> Result<(), HostStoreError> {
    let source = installed.ack.base_ack.source_version;
    for (index, (child, plan)) in installed.children.iter().zip(plans).enumerate() {
        let start = HOST_VIEWPORT_PRESENTATION_HEADER_BYTES
            .checked_add(
                index
                    .checked_mul(HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES)
                    .ok_or_else(|| {
                        HostStoreError::invalid("viewport directory offset overflowed")
                    })?,
            )
            .ok_or_else(|| HostStoreError::invalid("viewport directory offset overflowed"))?;
        let end = start
            .checked_add(HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES)
            .ok_or_else(|| HostStoreError::invalid("viewport directory offset overflowed"))?;
        let entry = page.get_mut(start..end).ok_or_else(|| {
            HostStoreError::new(
                HostRejectReason::InternalFault,
                "viewport directory escaped its preflighted page",
            )
        })?;
        let frame_id = match child.entry.binding.owner() {
            Some(HotInlineSidecarOwner::RecursiveGreenFrame(frame_id)) => frame_id,
            Some(HotInlineSidecarOwner::BlockOrdinal(_)) | None => {
                return Err(HostStoreError::new(
                    HostRejectReason::InternalFault,
                    "schema-10 viewport child lost its recursive-Green owner frame",
                ));
            }
        };
        put_u32(entry, 0, child.entry.ordered_child_index);
        put_u32(entry, 4, source.revision);
        put_id128(entry, 8, source.document_session);
        put_u32_array(entry, 24, installed.ack.base_ack.source_root);
        put_u32_array(entry, 32, source.content_hash128);
        put_u32(entry, 48, source.utf8_length);
        put_u32(entry, 52, source.utf16_length);
        put_u32(entry, 56, installed.ack.base_ack.parse_generation);
        put_u64(entry, 64, child.entry.binding.parser_profile);
        put_u64(entry, 72, child.entry.binding.refinement_generation);
        put_u64(entry, 80, child.entry.global_row_ordinal);
        put_u64(entry, 88, frame_id);
        put_u32(entry, 96, child.entry.binding.physical_start_utf8);
        put_u32(entry, 100, child.entry.binding.physical_end_utf8);
        put_u32(entry, 104, child.entry.binding.visible_start_utf8);
        put_u32(entry, 108, child.entry.binding.visible_end_utf8);
        put_u32(entry, 112, child.entry.binding.physical_start_utf16);
        put_u32(entry, 116, child.entry.binding.physical_end_utf16);
        put_u32(entry, 120, child.entry.binding.visible_start_utf16);
        put_u32(entry, 124, child.entry.binding.visible_end_utf16);
        entry[128] = plan.kind.wire();
        entry[129] = if plan.kind == PublicPayloadKind::Unsupported {
            2
        } else {
            1
        };
        put_u32(entry, 132, plan.record_count);
        put_u32(entry, 136, plan.offset);
        put_u32(entry, 140, plan.length);
        put_u32(entry, 144, plan.unsupported_reason);
    }
    Ok(())
}

fn write_public_header(
    page: &mut [u8],
    installed: &InstalledViewportPage,
    payload_start: usize,
) -> Result<(), HostStoreError> {
    let page_bytes = u32::try_from(page.len())
        .map_err(|_| HostStoreError::invalid("viewport page length exceeds u32"))?;
    let header = page
        .get_mut(..HOST_VIEWPORT_PRESENTATION_HEADER_BYTES)
        .ok_or_else(|| HostStoreError::invalid("viewport public header is truncated"))?;
    header[..8].copy_from_slice(VIEWPORT_MAGIC);
    put_u32(header, 8, HOST_VIEWPORT_PRESENTATION_SCHEMA);
    put_u32(header, 12, HOST_VIEWPORT_PRESENTATION_HEADER_BYTES as u32);
    put_u32(
        header,
        16,
        HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES as u32,
    );
    put_u32(
        header,
        20,
        u32::try_from(installed.children.len())
            .map_err(|_| HostStoreError::invalid("viewport entry count exceeds u32"))?,
    );
    put_u32(
        header,
        24,
        u32::try_from(payload_start)
            .map_err(|_| HostStoreError::invalid("viewport payload offset exceeds u32"))?,
    );
    put_u32(header, 28, page_bytes);
    put_id128(header, 32, installed.ack.publication_session);
    put_id128(header, 48, installed.ack.base_ack.publication_session);
    put_u32(header, 64, installed.ack.binding.viewport_generation);
    put_u32(header, 68, u32::from(installed.ack.binding.complete));
    put_metric_range(header, 72, installed.ack.binding.requested_range);
    put_metric_range(header, 88, installed.ack.binding.covered_range);
    put_u32(header, 104, installed.ack.actual_frame_count);
    put_u32(header, 108, installed.ack.actual_encoded_frame_bytes);
    put_id128(header, 112, installed.ack.aggregate_root_stream_digest);
    Ok(())
}

fn public_page_digest256(ack: ViewportPresentationAck, page: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"flark.v3.viewport-presentation.public-page.schema10.v1\0");
    hash_id128(&mut hasher, ack.publication_session);
    hash_structural_ack(&mut hasher, ack.base_ack);
    hash_viewport_binding(&mut hasher, ack.binding);
    hash_viewport_envelope(&mut hasher, ack.envelope);
    hasher.update(&ack.actual_frame_count.to_le_bytes());
    hasher.update(&ack.actual_encoded_frame_bytes.to_le_bytes());
    hash_id128(&mut hasher, ack.aggregate_root_stream_digest);
    hasher.update(page);
    *hasher.finalize().as_bytes()
}

fn hash_structural_ack(hasher: &mut blake3::Hasher, ack: StructuralAck) {
    hash_id128(hasher, ack.publication_session);
    hasher.update(&ack.host_revision.to_le_bytes());
    hash_id128(hasher, ack.source_version.document_session);
    hasher.update(&ack.source_version.revision.to_le_bytes());
    hasher.update(&ack.source_version.utf8_length.to_le_bytes());
    hasher.update(&ack.source_version.utf16_length.to_le_bytes());
    hash_u32_array(hasher, ack.source_version.content_hash128);
    hash_u32_array(hasher, ack.source_root);
    hasher.update(&ack.parse_generation.to_le_bytes());
    hasher.update(&ack.grammar_revision.to_le_bytes());
    hasher.update(&ack.syntax_profile.to_le_bytes());
    hasher.update(&ack.authority_mask.to_le_bytes());
    hasher.update(&ack.record_count.to_le_bytes());
    hash_id128(hasher, ack.sequence_digest);
    hash_id128(hasher, ack.manifest_digest);
}

fn hash_viewport_binding(
    hasher: &mut blake3::Hasher,
    binding: crate::v3_publication_wire::ViewportPresentationBinding,
) {
    hasher.update(&binding.viewport_generation.to_le_bytes());
    hash_metric_range(hasher, binding.requested_range);
    hash_metric_range(hasher, binding.covered_range);
    hash_visit_start(hasher, binding.start);
    hash_visit_start(hasher, binding.next);
    hasher.update(&[u8::from(binding.complete)]);
}

fn hash_viewport_envelope(
    hasher: &mut blake3::Hasher,
    envelope: crate::v3_publication_wire::ViewportPresentationEnvelopeMetrics,
) {
    hasher.update(&envelope.visited_structural_entries.to_le_bytes());
    hasher.update(&envelope.visited_storage_pages.to_le_bytes());
    hasher.update(&envelope.ordered_leaf_count.to_le_bytes());
    hasher.update(&envelope.inline_source_bytes.to_le_bytes());
    hasher.update(&envelope.fact_count.to_le_bytes());
    hasher.update(&envelope.transferred_node_count.to_le_bytes());
    hasher.update(&envelope.parser_transitions.to_le_bytes());
    hasher.update(&envelope.aggregate_envelope_digest256);
}

fn hash_metric_range(
    hasher: &mut blake3::Hasher,
    range: crate::v3_publication_wire::ViewportPresentationMetricRange,
) {
    hasher.update(&range.start_utf8.to_le_bytes());
    hasher.update(&range.start_utf16.to_le_bytes());
    hasher.update(&range.end_utf8.to_le_bytes());
    hasher.update(&range.end_utf16.to_le_bytes());
}

fn hash_visit_start(
    hasher: &mut blake3::Hasher,
    start: crate::v3_publication_wire::ViewportPresentationVisitStart,
) {
    hasher.update(&start.block_ordinal.to_le_bytes());
    hasher.update(&start.utf8_offset.to_le_bytes());
    hasher.update(&start.utf16_offset.to_le_bytes());
}

fn hash_id128(hasher: &mut blake3::Hasher, value: Id128) {
    hash_u32_array(hasher, value);
}

fn hash_u32_array<const N: usize>(hasher: &mut blake3::Hasher, value: [u32; N]) {
    for word in value {
        hasher.update(&word.to_le_bytes());
    }
}

fn put_metric_range(
    output: &mut [u8],
    offset: usize,
    range: crate::v3_publication_wire::ViewportPresentationMetricRange,
) {
    put_u32(output, offset, range.start_utf8);
    put_u32(output, offset + 4, range.start_utf16);
    put_u32(output, offset + 8, range.end_utf8);
    put_u32(output, offset + 12, range.end_utf16);
}

fn put_id128(output: &mut [u8], offset: usize, value: Id128) {
    put_u32_array(output, offset, value);
}

fn put_u32_array<const N: usize>(output: &mut [u8], offset: usize, value: [u32; N]) {
    for (index, word) in value.into_iter().enumerate() {
        put_u32(output, offset + index * 4, word);
    }
}

fn put_u64(output: &mut [u8], offset: usize, value: u64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(output: &mut [u8], offset: usize, value: u32) {
    output[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn process_child_frame(
    active: &mut ActiveViewportOffer,
    bytes: &[u8],
) -> Result<(), HostStoreError> {
    let wrapper = decode_viewport_presentation_child_frame(bytes, active.begin)
        .map_err(map_viewport_decode_error)?;
    if wrapper.payload().len() > M11_HOST_MAXIMUM_FRAME_BYTES {
        return Err(HostStoreError::new(
            HostRejectReason::CorruptPublication,
            "viewport child payload exceeds the independent HIO1 host bound",
        ));
    }
    let expected_directory_index = u32::try_from(active.children.len())
        .map_err(|_| HostStoreError::invalid("viewport child index overflowed"))?;
    match wrapper.kind {
        HotInlineSidecarFrameKind::Begin => {
            if active.current_child.is_some()
                || wrapper.directory_index != expected_directory_index
                || wrapper.child_frame_ordinal != 0
                || wrapper.record_count != 0
            {
                return Err(HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "viewport HIO1 Begin is out of child order",
                ));
            }
            let entry = *active
                .directory
                .get(wrapper.directory_index as usize)
                .ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "viewport child escaped its directory",
                    )
                })?;
            validate_inline_sidecar_begin_frame(wrapper.payload(), entry.hio1_envelope)?;
            let owner = match entry.binding.owner().ok_or_else(|| {
                HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "viewport child lost its typed HIO1 owner",
                )
            })? {
                HotInlineSidecarOwner::BlockOrdinal(ordinal) => {
                    M11HostInlineSidecarOwner::BlockOrdinal(ordinal)
                }
                HotInlineSidecarOwner::RecursiveGreenFrame(frame) => {
                    M11HostInlineSidecarOwner::RecursiveGreenFrame(frame)
                }
            };
            let binding = M11HostInlineSidecarBinding::new_for_owner(
                active.base.clone(),
                entry.binding.refinement_generation,
                owner,
                entry.binding.physical_start_utf8,
                entry.binding.physical_end_utf8,
                entry.binding.visible_start_utf8,
                entry.binding.visible_end_utf8,
                entry.binding.physical_start_utf16,
                entry.binding.physical_end_utf16,
                entry.binding.visible_start_utf16,
                entry.binding.visible_end_utf16,
            )
            .map_err(map_engine_error)?;
            let mut sidecar = M11HostInlineSidecar::new(active.base.clone(), active.engine_limits);
            sidecar
                .begin_snapshot(binding.clone(), wrapper.payload())
                .map_err(map_engine_error)?;
            active.current_child = Some(ActiveViewportChild {
                directory_index: wrapper.directory_index,
                entry,
                binding,
                sidecar,
                next_frame_ordinal: 1,
                next_node_ordinal: None,
                accepted_node_count: 0,
                installing: false,
            });
        }
        HotInlineSidecarFrameKind::Node => {
            let child = active.current_child.as_mut().ok_or_else(|| {
                HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "viewport HIO1 Node has no active child",
                )
            })?;
            if child.installing
                || wrapper.directory_index != child.directory_index
                || wrapper.child_frame_ordinal != child.next_frame_ordinal
                || wrapper.record_count != 1
                || child.accepted_node_count >= child.entry.hio1_envelope.transferred_node_count
            {
                return Err(HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "viewport HIO1 Node order changed",
                ));
            }
            let metadata =
                M11CandidateHost::classify_frame(wrapper.payload()).map_err(map_engine_error)?;
            if metadata.kind != M11HostFrameKind::Node {
                return Err(HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "viewport public Node wrapper carried another HIO1 frame kind",
                ));
            }
            let node_ordinal = metadata.node_ordinal.ok_or_else(|| {
                HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "viewport HIO1 Node lost its engine ordinal",
                )
            })?;
            if child
                .next_node_ordinal
                .is_some_and(|expected| expected != node_ordinal)
            {
                return Err(HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "viewport HIO1 Node ordinal changed",
                ));
            }
            child
                .sidecar
                .offer_node(wrapper.payload())
                .map_err(map_engine_error)?;
            child.next_node_ordinal = Some(node_ordinal.checked_add(1).ok_or_else(|| {
                HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "viewport HIO1 Node ordinal overflowed",
                )
            })?);
            child.accepted_node_count += 1;
            child.next_frame_ordinal += 1;
        }
        HotInlineSidecarFrameKind::End => {
            let child = active.current_child.as_mut().ok_or_else(|| {
                HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "viewport HIO1 End has no active child",
                )
            })?;
            if child.installing
                || wrapper.directory_index != child.directory_index
                || wrapper.child_frame_ordinal != child.next_frame_ordinal
                || wrapper.record_count != 0
                || child.accepted_node_count != child.entry.hio1_envelope.transferred_node_count
            {
                return Err(HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "viewport HIO1 End arrived before its complete child closure",
                ));
            }
            let metadata =
                M11CandidateHost::classify_frame(wrapper.payload()).map_err(map_engine_error)?;
            if metadata.kind != M11HostFrameKind::End
                || metadata.canonical_stream_digest256.is_none()
            {
                return Err(HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "viewport public End wrapper carried another HIO1 frame kind",
                ));
            }
            child
                .sidecar
                .finish_snapshot(wrapper.payload())
                .map_err(map_engine_error)?;
            child.next_frame_ordinal += 1;
            child.installing = true;
        }
    }
    Ok(())
}

fn validate_public_page_bound(active: &ActiveViewportOffer) -> Result<(), HostStoreError> {
    validate_public_page_bound_values(
        &active.directory,
        active.maximum_query_bytes,
        active.begin.query_limits.maximum_encoded_frame_bytes,
    )
}

fn validate_public_page_bound_values(
    directory: &[ViewportPresentationDirectoryEntry],
    maximum_query_bytes: u32,
    maximum_semantic_bytes: u32,
) -> Result<(), HostStoreError> {
    let mut bytes = HOST_VIEWPORT_PRESENTATION_HEADER_BYTES
        .checked_add(
            directory
                .len()
                .checked_mul(HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES)
                .ok_or_else(|| HostStoreError::invalid("viewport page directory overflowed"))?,
        )
        .ok_or_else(|| HostStoreError::invalid("viewport page header overflowed"))?;
    for entry in directory {
        let payload_bytes = match entry.hio1_envelope.disposition {
            HotInlineSidecarDisposition::Authoritative {
                fact_count,
                link_value_encoded_bytes,
                ..
            } => {
                let record_bytes =
                    if entry.hio1_envelope.ipr2_descriptor_bytes == IPR3_DESCRIPTOR_BYTES {
                        M11_INLINE_FACT_RECORD_BYTES
                    } else {
                        M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES
                    };
                usize::try_from(fact_count)
                    .ok()
                    .and_then(|count| count.checked_mul(record_bytes))
                    .and_then(|bytes| {
                        bytes.checked_add(usize::try_from(link_value_encoded_bytes).ok()?)
                    })
                    .ok_or_else(|| {
                        HostStoreError::invalid("viewport public payload bound overflowed")
                    })?
            }
            HotInlineSidecarDisposition::Unsupported { .. } => M11_INLINE_META_RECORD_BYTES,
        };
        bytes = bytes
            .checked_add(payload_bytes)
            .ok_or_else(|| HostStoreError::invalid("viewport public aggregate bound overflowed"))?;
    }
    let query_limit = usize::try_from(maximum_query_bytes)
        .map_err(|_| HostStoreError::invalid("viewport host query bound overflowed"))?;
    if bytes > query_limit || bytes > maximum_semantic_bytes as usize {
        return Err(HostStoreError::new(
            HostRejectReason::QueryBoundExceeded,
            "viewport page cannot fit the admitted host query bound",
        ));
    }
    Ok(())
}

fn map_viewport_decode_error(_: crate::v3_publication_wire::DecodeError) -> HostStoreError {
    HostStoreError::new(
        HostRejectReason::CorruptPublication,
        "viewport public frame is invalid",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dense_entry(fact_count: u64) -> ViewportPresentationDirectoryEntry {
        ViewportPresentationDirectoryEntry {
            ordered_child_index: 0,
            global_row_ordinal: 0,
            binding: HotInlineSidecarBinding {
                parser_profile: 1,
                refinement_generation: 1,
                block_ordinal: HotInlineSidecarOwner::RecursiveGreenFrame(1)
                    .into_wire()
                    .expect("test frame fits owner slot"),
                physical_start_utf8: 0,
                physical_end_utf8: 1,
                visible_start_utf8: 0,
                visible_end_utf8: 1,
                physical_start_utf16: 0,
                physical_end_utf16: 1,
                visible_start_utf16: 0,
                visible_end_utf16: 1,
            },
            hio1_envelope: crate::v3_publication_wire::HotInlineSidecarEnvelopeMetrics {
                hio1_encoded_bytes: crate::v3_publication_wire::HIO1_ENVELOPE_BYTES,
                ipr2_descriptor_bytes:
                    crate::v3_publication_wire::BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES,
                transferred_node_count: 1,
                hio1_envelope_digest256: [1; 32],
                disposition: HotInlineSidecarDisposition::Authoritative {
                    logical_page_count: 1,
                    fact_count,
                    storage_page_count: 1,
                    link_value_entry_count: 0,
                    link_value_encoded_bytes: 0,
                    link_value_storage_page_count: 0,
                    ordered_commitment256: [2; 32],
                },
            },
        }
    }

    fn direct_link_entry() -> ViewportPresentationDirectoryEntry {
        let mut entry = dense_entry(1);
        entry.hio1_envelope.ipr2_descriptor_bytes = IPR3_DESCRIPTOR_BYTES;
        entry.hio1_envelope.disposition = HotInlineSidecarDisposition::Authoritative {
            logical_page_count: 1,
            fact_count: 1,
            storage_page_count: 1,
            link_value_entry_count: 1,
            link_value_encoded_bytes: 49,
            link_value_storage_page_count: 1,
            ordered_commitment256: [2; 32],
        };
        entry
    }

    #[test]
    fn viewport_dense_page_admission_honors_the_standard_64k_query_ceiling() {
        assert!(validate_public_page_bound_values(
            &[dense_entry(1_359)],
            64 * 1024,
            4 * 1024 * 1024,
        )
        .is_ok());
        assert_eq!(
            validate_public_page_bound_values(&[dense_entry(1_360)], 64 * 1024, 4 * 1024 * 1024)
                .expect_err("one record beyond the exact 64KiB page must fail")
                .reason(),
            HostRejectReason::QueryBoundExceeded
        );
    }

    #[test]
    fn viewport_page_admission_charges_the_inline_value_lane() {
        let exact_bytes = HOST_VIEWPORT_PRESENTATION_HEADER_BYTES
            + HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES
            + M11_INLINE_FACT_RECORD_BYTES
            + 49;
        assert!(validate_public_page_bound_values(
            &[direct_link_entry()],
            exact_bytes as u32,
            4 * 1024 * 1024,
        )
        .is_ok());
        assert_eq!(
            validate_public_page_bound_values(
                &[direct_link_entry()],
                (exact_bytes - 1) as u32,
                4 * 1024 * 1024,
            )
            .expect_err("the FLKIV lane must participate in aggregate admission")
            .reason(),
            HostRejectReason::QueryBoundExceeded
        );
    }
}

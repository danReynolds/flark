//! C ABI for the independent generation-checked M1.1 candidate host.

use std::ffi::c_void;
use std::mem::{align_of, offset_of, size_of};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::OnceLock;

use flark_engine::m11_host::M11_HOST_MAXIMUM_FRAME_BYTES;
use flark_parser::{M11_GREEN_RECORD_BYTES, M11_PROJECTION_RECORD_BYTES};

use crate::v3_host_registry::{HostHandle, HostRegistry, HostRegistryError};
use crate::v3_host_store::{
    HostBlockRangeBudget, HostBlockRangeContinuation, HostBlockRangeOutcome, HostBlockRangeQuery,
    HostBlockRangeReceipt, HostConfig, HostInlineSidecarQueryOutcome, HostMetricAffinity,
    HostMetricRange, HostPointQuery, HostPollOutcome, HostQueryBudget, HostRejectReason,
    HostSourceGapReason, HostSourceMetric, HostStoreError, HostStructuralOrdinalWindowBudget,
    HostStructuralOrdinalWindowFailureReason, HostStructuralOrdinalWindowOutcome,
    HostStructuralOrdinalWindowQuery, HostStructuralOrdinalWindowReceipt,
    HostStructuralQueryOutcome, HostViewportPresentationPollOutcome,
    HostViewportPresentationQueryOutcome, HostViewportReceipt, HostWorkGrant,
    InlineSidecarHostPollOutcome, HOST_BLOCK_RANGE_CONTINUATION_BYTES,
    HOST_BLOCK_RANGE_HEADER_BYTES, HOST_BLOCK_RANGE_RECORD_BYTES,
    HOST_INLINE_SIDECAR_MAXIMUM_QUERY_BYTES, HOST_M11_VIEWPORT_BYTES,
    HOST_STRUCTURAL_ORDINAL_WINDOW_MAXIMUM_ENTRIES,
    HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES, HOST_VIEWPORT_PRESENTATION_HEADER_BYTES,
    HOST_VIEWPORT_PRESENTATION_MAXIMUM_QUERY_BYTES, HOST_VIEWPORT_PRESENTATION_SCHEMA,
};
use crate::v3_publication_wire::{
    decode_publication_packet_envelope, CommitRequest, HotInlineSidecarBegin,
    HotInlineSidecarBinding, HotInlineSidecarCommitRequest, HotInlineSidecarDisposition,
    HotInlineSidecarEnvelopeMetrics, HotInlineSidecarMode, InlineSidecarAck,
    InlineSidecarAckDisposition, OfferBegin, OfferLimits, PublicationMode, SourceVersion,
    StructuralAck, ViewportPresentationAck, ViewportPresentationBegin, ViewportPresentationBinding,
    ViewportPresentationCommitRequest, ViewportPresentationEnvelopeMetrics,
    ViewportPresentationMetricRange, ViewportPresentationMode, ViewportPresentationOfferLimits,
    ViewportPresentationQueryLimits, ViewportPresentationVisitStart,
    MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES, MAXIMUM_PACKET_ENCODED_BYTES,
};
use crate::v3_wire::Status;

pub const FLARK_V3_HOST_NATIVE_ABI_VERSION: u32 = 0x0003_0005;
pub const FLARK_V3_HOST_MAXIMUM_FRAME_BYTES: u32 = M11_HOST_MAXIMUM_FRAME_BYTES as u32;
pub const FLARK_V3_HOST_MAXIMUM_PACKET_BYTES: u32 = MAXIMUM_PACKET_ENCODED_BYTES as u32;
pub const FLARK_V3_HOST_MAXIMUM_QUERY_BYTES: u32 = 64 * 1024;
pub const FLARK_V3_HOST_INLINE_SIDECAR_MAXIMUM_QUERY_BYTES: u32 =
    HOST_INLINE_SIDECAR_MAXIMUM_QUERY_BYTES;
pub const FLARK_V3_HOST_VIEWPORT_PRESENTATION_MAXIMUM_QUERY_BYTES: u32 =
    HOST_VIEWPORT_PRESENTATION_MAXIMUM_QUERY_BYTES;
pub const FLARK_V3_HOST_M11_GREEN_RECORD_BYTES: u32 = M11_GREEN_RECORD_BYTES as u32;
pub const FLARK_V3_HOST_M11_PROJECTION_RECORD_BYTES: u32 = M11_PROJECTION_RECORD_BYTES as u32;
pub const FLARK_V3_HOST_POINT_QUERY_SCHEMA: u32 = 1;
pub const FLARK_V3_HOST_BLOCK_RANGE_QUERY_SCHEMA: u32 = 1;
pub const FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES: usize = HOST_BLOCK_RANGE_CONTINUATION_BYTES;
pub const FLARK_V3_HOST_BLOCK_RANGE_HEADER_BYTES: u32 = HOST_BLOCK_RANGE_HEADER_BYTES as u32;
pub const FLARK_V3_HOST_BLOCK_RANGE_RECORD_BYTES: u32 = HOST_BLOCK_RANGE_RECORD_BYTES as u32;
pub const FLARK_V3_HOST_STRUCTURAL_ORDINAL_WINDOW_QUERY_SCHEMA: u32 = 1;
pub const FLARK_V3_HOST_STRUCTURAL_ORDINAL_WINDOW_MAXIMUM_ENTRIES: u32 =
    HOST_STRUCTURAL_ORDINAL_WINDOW_MAXIMUM_ENTRIES;
pub const FLARK_V3_HOST_INLINE_SIDECAR_QUERY_SCHEMA: u32 = 3;
pub const FLARK_V3_HOST_M11_VIEWPORT_BYTES: u32 = HOST_M11_VIEWPORT_BYTES as u32;
pub const FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_SCHEMA: u32 = 1;
pub const FLARK_V3_HOST_VIEWPORT_PRESENTATION_PAGE_SCHEMA: u32 = HOST_VIEWPORT_PRESENTATION_SCHEMA;
pub const FLARK_V3_HOST_VIEWPORT_PRESENTATION_PAGE_HEADER_BYTES: u32 =
    HOST_VIEWPORT_PRESENTATION_HEADER_BYTES as u32;
pub const FLARK_V3_HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES: u32 =
    HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES as u32;
pub const FLARK_V3_HOST_VIEWPORT_PRESENTATION_MAXIMUM_FRAME_BYTES: u32 =
    MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES;
pub const FLARK_V3_HOST_VIEWPORT_PRESENTATION_BEGIN_BYTES: u32 =
    size_of::<FlarkV3HostViewportPresentationBegin>() as u32;
pub const FLARK_V3_HOST_VIEWPORT_PRESENTATION_COMMIT_REQUEST_BYTES: u32 =
    size_of::<FlarkV3HostViewportPresentationCommitRequest>() as u32;
pub const FLARK_V3_HOST_VIEWPORT_PRESENTATION_ACK_BYTES: u32 =
    size_of::<FlarkV3HostViewportPresentationAck>() as u32;
pub const FLARK_V3_HOST_VIEWPORT_PRESENTATION_POLL_RECEIPT_BYTES: u32 =
    size_of::<FlarkV3HostViewportPresentationPollReceipt>() as u32;
pub const FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_BYTES: u32 =
    size_of::<FlarkV3HostViewportPresentationQuery>() as u32;
pub const FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_RECEIPT_BYTES: u32 =
    size_of::<FlarkV3HostViewportPresentationQueryReceipt>() as u32;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostHandle {
    pub slot: u32,
    pub generation: u32,
}

impl From<FlarkV3HostHandle> for HostHandle {
    fn from(handle: FlarkV3HostHandle) -> Self {
        Self::from_parts(handle.slot, handle.generation)
    }
}

impl From<HostHandle> for FlarkV3HostHandle {
    fn from(handle: HostHandle) -> Self {
        Self {
            slot: handle.slot(),
            generation: handle.generation(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostConfig {
    pub abi_version: u32,
    pub struct_size: u32,
    pub document_session: [u32; 4],
    pub grammar_revision: u32,
    pub syntax_profile: u32,
    pub authority_mask: u32,
    pub maximum_query_bytes: u32,
    pub reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostSourceVersion {
    pub document_session: [u32; 4],
    pub revision: u32,
    pub utf8_length: u32,
    pub utf16_length: u32,
    pub content_hash128: [u32; 4],
}

impl From<FlarkV3HostSourceVersion> for SourceVersion {
    fn from(source: FlarkV3HostSourceVersion) -> Self {
        Self {
            document_session: source.document_session,
            revision: source.revision,
            utf8_length: source.utf8_length,
            utf16_length: source.utf16_length,
            content_hash128: source.content_hash128,
        }
    }
}

impl From<SourceVersion> for FlarkV3HostSourceVersion {
    fn from(source: SourceVersion) -> Self {
        Self {
            document_session: source.document_session,
            revision: source.revision,
            utf8_length: source.utf8_length,
            utf16_length: source.utf16_length,
            content_hash128: source.content_hash128,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostOfferBegin {
    pub offer_id: [u32; 4],
    pub publication_session: [u32; 4],
    pub target_host_revision: u32,
    pub source_version: FlarkV3HostSourceVersion,
    /// Canonical Dart order: `[highWord, lowWord]`.
    pub source_root: [u32; 2],
    pub parse_generation: u32,
    pub grammar_revision: u32,
    pub syntax_profile: u32,
    pub authority_mask: u32,
    pub transferred_record_count: u32,
    pub target_record_count: u32,
    pub maximum_frame_count: u32,
    pub maximum_encoded_frame_bytes: u32,
    pub maximum_packet_bytes: u32,
    pub maximum_frame_bytes: u32,
    pub maximum_program_children: u32,
    pub reserved: [u32; 3],
}

impl FlarkV3HostOfferBegin {
    fn into_offer(self, mode: PublicationMode, base_ack: Option<StructuralAck>) -> OfferBegin {
        OfferBegin {
            schema: 1,
            offer_id: self.offer_id,
            publication_session: self.publication_session,
            target_host_revision: self.target_host_revision,
            source_version: self.source_version.into(),
            source_root: self.source_root,
            parse_generation: self.parse_generation,
            grammar_revision: self.grammar_revision,
            syntax_profile: self.syntax_profile,
            authority_mask: self.authority_mask,
            mode,
            base_ack,
            transferred_record_count: self.transferred_record_count,
            target_record_count: self.target_record_count,
            limits: OfferLimits {
                maximum_frame_count: self.maximum_frame_count,
                maximum_encoded_frame_bytes: self.maximum_encoded_frame_bytes,
                maximum_packet_bytes: self.maximum_packet_bytes,
                maximum_frame_bytes: self.maximum_frame_bytes,
                maximum_program_children: self.maximum_program_children,
            },
        }
    }
}

impl From<FlarkV3HostOfferBegin> for OfferBegin {
    fn from(begin: FlarkV3HostOfferBegin) -> Self {
        begin.into_offer(PublicationMode::FullSnapshot, None)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostCommitRequest {
    pub offer_id: [u32; 4],
    pub actual_frame_count: u32,
    pub actual_encoded_frame_bytes: u32,
    pub rolling_transport_digest: [u32; 4],
    pub canonical_stream_digest: [u32; 4],
}

impl From<FlarkV3HostCommitRequest> for CommitRequest {
    fn from(request: FlarkV3HostCommitRequest) -> Self {
        Self {
            offer_id: request.offer_id,
            actual_frame_count: request.actual_frame_count,
            actual_encoded_frame_bytes: request.actual_encoded_frame_bytes,
            rolling_transport_digest: request.rolling_transport_digest,
            canonical_stream_digest: request.canonical_stream_digest,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostStructuralAck {
    pub publication_session: [u32; 4],
    pub host_revision: u32,
    pub source_version: FlarkV3HostSourceVersion,
    /// Canonical Dart order: `[highWord, lowWord]`.
    pub source_root: [u32; 2],
    pub parse_generation: u32,
    pub grammar_revision: u32,
    pub syntax_profile: u32,
    pub authority_mask: u32,
    pub record_count: u32,
    pub sequence_digest: [u32; 4],
    pub manifest_digest: [u32; 4],
}

impl From<StructuralAck> for FlarkV3HostStructuralAck {
    fn from(ack: StructuralAck) -> Self {
        Self {
            publication_session: ack.publication_session,
            host_revision: ack.host_revision,
            source_version: ack.source_version.into(),
            source_root: ack.source_root,
            parse_generation: ack.parse_generation,
            grammar_revision: ack.grammar_revision,
            syntax_profile: ack.syntax_profile,
            authority_mask: ack.authority_mask,
            record_count: ack.record_count,
            sequence_digest: ack.sequence_digest,
            manifest_digest: ack.manifest_digest,
        }
    }
}

impl From<FlarkV3HostStructuralAck> for StructuralAck {
    fn from(ack: FlarkV3HostStructuralAck) -> Self {
        Self {
            publication_session: ack.publication_session,
            host_revision: ack.host_revision,
            source_version: ack.source_version.into(),
            source_root: ack.source_root,
            parse_generation: ack.parse_generation,
            grammar_revision: ack.grammar_revision,
            syntax_profile: ack.syntax_profile,
            authority_mask: ack.authority_mask,
            record_count: ack.record_count,
            sequence_digest: ack.sequence_digest,
            manifest_digest: ack.manifest_digest,
        }
    }
}

/// Lossless little-endian `u64` lanes for C and Dart FFI targets.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostU64 {
    pub low_word: u32,
    pub high_word: u32,
}

impl From<FlarkV3HostU64> for u64 {
    fn from(value: FlarkV3HostU64) -> Self {
        u64::from(value.low_word) | (u64::from(value.high_word) << 32)
    }
}

impl From<u64> for FlarkV3HostU64 {
    fn from(value: u64) -> Self {
        Self {
            low_word: value as u32,
            high_word: (value >> 32) as u32,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostInlineSidecarBinding {
    pub parser_profile: FlarkV3HostU64,
    pub refinement_generation: FlarkV3HostU64,
    pub block_ordinal: FlarkV3HostU64,
    pub physical_start_utf8: u32,
    pub physical_end_utf8: u32,
    pub visible_start_utf8: u32,
    pub visible_end_utf8: u32,
    pub physical_start_utf16: u32,
    pub physical_end_utf16: u32,
    pub visible_start_utf16: u32,
    pub visible_end_utf16: u32,
}

impl From<FlarkV3HostInlineSidecarBinding> for HotInlineSidecarBinding {
    fn from(binding: FlarkV3HostInlineSidecarBinding) -> Self {
        Self {
            parser_profile: binding.parser_profile.into(),
            refinement_generation: binding.refinement_generation.into(),
            block_ordinal: binding.block_ordinal.into(),
            physical_start_utf8: binding.physical_start_utf8,
            physical_end_utf8: binding.physical_end_utf8,
            visible_start_utf8: binding.visible_start_utf8,
            visible_end_utf8: binding.visible_end_utf8,
            physical_start_utf16: binding.physical_start_utf16,
            physical_end_utf16: binding.physical_end_utf16,
            visible_start_utf16: binding.visible_start_utf16,
            visible_end_utf16: binding.visible_end_utf16,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostInlineSidecarDisposition {
    /// `1` is authoritative and `2` is unsupported.
    pub disposition: u32,
    pub reason: u32,
    pub logical_page_count: FlarkV3HostU64,
    pub fact_count: FlarkV3HostU64,
    pub storage_page_count: FlarkV3HostU64,
    pub link_value_entry_count: u32,
    pub link_value_encoded_bytes: u32,
    pub link_value_storage_page_count: FlarkV3HostU64,
    /// Eight little-endian `u32` lanes of one full engine digest.
    pub commitment256: [u32; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostInlineSidecarBegin {
    pub schema: u32,
    /// `1` is the hot-inline sidecar protocol.
    pub mode: u32,
    pub offer_id: [u32; 4],
    pub publication_session: [u32; 4],
    pub base_ack: FlarkV3HostStructuralAck,
    pub binding: FlarkV3HostInlineSidecarBinding,
    pub hio1_encoded_bytes: u32,
    pub ipr2_descriptor_bytes: u32,
    pub transferred_node_count: u32,
    pub sidecar_disposition: FlarkV3HostInlineSidecarDisposition,
    pub hio1_envelope_digest256: [u32; 8],
    pub maximum_frame_count: u32,
    pub maximum_encoded_frame_bytes: u32,
    pub maximum_packet_bytes: u32,
    pub maximum_frame_bytes: u32,
    pub maximum_program_children: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostInlineSidecarCommitRequest {
    pub offer_id: [u32; 4],
    pub actual_frame_count: u32,
    pub actual_encoded_frame_bytes: u32,
    pub rolling_transport_digest: [u32; 4],
    pub root_stream_digest: [u32; 4],
}

impl From<FlarkV3HostInlineSidecarCommitRequest> for HotInlineSidecarCommitRequest {
    fn from(request: FlarkV3HostInlineSidecarCommitRequest) -> Self {
        Self {
            offer_id: request.offer_id,
            actual_frame_count: request.actual_frame_count,
            actual_encoded_frame_bytes: request.actual_encoded_frame_bytes,
            rolling_transport_digest: request.rolling_transport_digest,
            root_stream_digest: request.root_stream_digest,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostInlineSidecarAck {
    pub publication_session: [u32; 4],
    pub base_ack: FlarkV3HostStructuralAck,
    pub refinement_generation: FlarkV3HostU64,
    pub block_ordinal: FlarkV3HostU64,
    pub transferred_node_count: u32,
    /// `1` is authoritative and `2` is unsupported.
    pub disposition: u32,
    pub hio1_envelope_digest256: [u32; 8],
    pub root_stream_digest: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostInlineSidecarPollReceipt {
    pub rejection_reason: u32,
    pub outcome: u32,
    pub offer_id: [u32; 4],
    pub next_frame_ordinal: u32,
    pub ack: FlarkV3HostInlineSidecarAck,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostInlineSidecarQuery {
    pub schema: u32,
    pub struct_size: u32,
    pub binding: FlarkV3HostInlineSidecarBinding,
    pub maximum_encoded_bytes: u32,
    pub reserved: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostInlineSidecarQueryReceipt {
    pub rejection_reason: u32,
    /// `0` is unavailable, `1` authoritative, and `2` unsupported.
    pub outcome: u32,
    pub reason: u32,
    pub encoded_bytes: u32,
    pub fact_count: u32,
    pub tree_nodes_visited: u32,
    pub value_entry_count: u32,
    pub value_encoded_bytes: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostViewportPresentationMetricRange {
    pub start_utf8: u32,
    pub start_utf16: u32,
    pub end_utf8: u32,
    pub end_utf16: u32,
}

impl From<FlarkV3HostViewportPresentationMetricRange> for ViewportPresentationMetricRange {
    fn from(range: FlarkV3HostViewportPresentationMetricRange) -> Self {
        Self {
            start_utf8: range.start_utf8,
            start_utf16: range.start_utf16,
            end_utf8: range.end_utf8,
            end_utf16: range.end_utf16,
        }
    }
}

impl From<ViewportPresentationMetricRange> for FlarkV3HostViewportPresentationMetricRange {
    fn from(range: ViewportPresentationMetricRange) -> Self {
        Self {
            start_utf8: range.start_utf8,
            start_utf16: range.start_utf16,
            end_utf8: range.end_utf8,
            end_utf16: range.end_utf16,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostViewportPresentationVisitStart {
    pub block_ordinal: FlarkV3HostU64,
    pub utf8_offset: u32,
    pub utf16_offset: u32,
}

impl From<FlarkV3HostViewportPresentationVisitStart> for ViewportPresentationVisitStart {
    fn from(start: FlarkV3HostViewportPresentationVisitStart) -> Self {
        Self {
            block_ordinal: start.block_ordinal.into(),
            utf8_offset: start.utf8_offset,
            utf16_offset: start.utf16_offset,
        }
    }
}

impl From<ViewportPresentationVisitStart> for FlarkV3HostViewportPresentationVisitStart {
    fn from(start: ViewportPresentationVisitStart) -> Self {
        Self {
            block_ordinal: start.block_ordinal.into(),
            utf8_offset: start.utf8_offset,
            utf16_offset: start.utf16_offset,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostViewportPresentationBinding {
    pub viewport_generation: u32,
    pub requested_range: FlarkV3HostViewportPresentationMetricRange,
    pub covered_range: FlarkV3HostViewportPresentationMetricRange,
    pub start: FlarkV3HostViewportPresentationVisitStart,
    pub next: FlarkV3HostViewportPresentationVisitStart,
    /// Zero is false and one is true.
    pub complete: u32,
}

impl From<ViewportPresentationBinding> for FlarkV3HostViewportPresentationBinding {
    fn from(binding: ViewportPresentationBinding) -> Self {
        Self {
            viewport_generation: binding.viewport_generation,
            requested_range: binding.requested_range.into(),
            covered_range: binding.covered_range.into(),
            start: binding.start.into(),
            next: binding.next.into(),
            complete: u32::from(binding.complete),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostViewportPresentationEnvelope {
    pub visited_structural_entries: u32,
    pub visited_storage_pages: u32,
    pub ordered_leaf_count: u32,
    pub inline_source_bytes: u32,
    pub fact_count: u32,
    pub transferred_node_count: u32,
    pub parser_transitions: u32,
    pub aggregate_envelope_digest256: [u32; 8],
}

impl From<FlarkV3HostViewportPresentationEnvelope> for ViewportPresentationEnvelopeMetrics {
    fn from(envelope: FlarkV3HostViewportPresentationEnvelope) -> Self {
        Self {
            visited_structural_entries: envelope.visited_structural_entries,
            visited_storage_pages: envelope.visited_storage_pages,
            ordered_leaf_count: envelope.ordered_leaf_count,
            inline_source_bytes: envelope.inline_source_bytes,
            fact_count: envelope.fact_count,
            transferred_node_count: envelope.transferred_node_count,
            parser_transitions: envelope.parser_transitions,
            aggregate_envelope_digest256: digest256_from_words(
                envelope.aggregate_envelope_digest256,
            ),
        }
    }
}

impl From<ViewportPresentationEnvelopeMetrics> for FlarkV3HostViewportPresentationEnvelope {
    fn from(envelope: ViewportPresentationEnvelopeMetrics) -> Self {
        Self {
            visited_structural_entries: envelope.visited_structural_entries,
            visited_storage_pages: envelope.visited_storage_pages,
            ordered_leaf_count: envelope.ordered_leaf_count,
            inline_source_bytes: envelope.inline_source_bytes,
            fact_count: envelope.fact_count,
            transferred_node_count: envelope.transferred_node_count,
            parser_transitions: envelope.parser_transitions,
            aggregate_envelope_digest256: digest256_to_words(envelope.aggregate_envelope_digest256),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostViewportPresentationQueryLimits {
    pub maximum_structural_entries: u32,
    pub maximum_storage_pages: u32,
    pub maximum_inline_leaves: u32,
    pub maximum_inline_leaf_source_bytes: u32,
    pub maximum_inline_source_bytes: u32,
    pub maximum_fact_records: u32,
    pub maximum_encoded_frame_bytes: u32,
    pub maximum_parser_transitions: u32,
}

impl From<FlarkV3HostViewportPresentationQueryLimits> for ViewportPresentationQueryLimits {
    fn from(limits: FlarkV3HostViewportPresentationQueryLimits) -> Self {
        Self {
            maximum_structural_entries: limits.maximum_structural_entries,
            maximum_storage_pages: limits.maximum_storage_pages,
            maximum_inline_leaves: limits.maximum_inline_leaves,
            maximum_inline_leaf_source_bytes: limits.maximum_inline_leaf_source_bytes,
            maximum_inline_source_bytes: limits.maximum_inline_source_bytes,
            maximum_fact_records: limits.maximum_fact_records,
            maximum_encoded_frame_bytes: limits.maximum_encoded_frame_bytes,
            maximum_parser_transitions: limits.maximum_parser_transitions,
        }
    }
}

impl From<ViewportPresentationQueryLimits> for FlarkV3HostViewportPresentationQueryLimits {
    fn from(limits: ViewportPresentationQueryLimits) -> Self {
        Self {
            maximum_structural_entries: limits.maximum_structural_entries,
            maximum_storage_pages: limits.maximum_storage_pages,
            maximum_inline_leaves: limits.maximum_inline_leaves,
            maximum_inline_leaf_source_bytes: limits.maximum_inline_leaf_source_bytes,
            maximum_inline_source_bytes: limits.maximum_inline_source_bytes,
            maximum_fact_records: limits.maximum_fact_records,
            maximum_encoded_frame_bytes: limits.maximum_encoded_frame_bytes,
            maximum_parser_transitions: limits.maximum_parser_transitions,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostViewportPresentationOfferLimits {
    pub maximum_frame_count: u32,
    pub maximum_encoded_frame_bytes: u32,
    pub maximum_packet_bytes: u32,
    pub maximum_frame_bytes: u32,
    pub maximum_program_children: u32,
}

impl From<FlarkV3HostViewportPresentationOfferLimits> for ViewportPresentationOfferLimits {
    fn from(limits: FlarkV3HostViewportPresentationOfferLimits) -> Self {
        Self {
            maximum_frame_count: limits.maximum_frame_count,
            maximum_encoded_frame_bytes: limits.maximum_encoded_frame_bytes,
            maximum_packet_bytes: limits.maximum_packet_bytes,
            maximum_frame_bytes: limits.maximum_frame_bytes,
            maximum_program_children: limits.maximum_program_children,
        }
    }
}

impl From<ViewportPresentationOfferLimits> for FlarkV3HostViewportPresentationOfferLimits {
    fn from(limits: ViewportPresentationOfferLimits) -> Self {
        Self {
            maximum_frame_count: limits.maximum_frame_count,
            maximum_encoded_frame_bytes: limits.maximum_encoded_frame_bytes,
            maximum_packet_bytes: limits.maximum_packet_bytes,
            maximum_frame_bytes: limits.maximum_frame_bytes,
            maximum_program_children: limits.maximum_program_children,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostViewportPresentationBegin {
    pub schema: u32,
    /// `1` is the aggregate VPB1 page mode.
    pub mode: u32,
    pub offer_id: [u32; 4],
    pub publication_session: [u32; 4],
    pub base_ack: FlarkV3HostStructuralAck,
    pub binding: FlarkV3HostViewportPresentationBinding,
    pub envelope: FlarkV3HostViewportPresentationEnvelope,
    pub query_limits: FlarkV3HostViewportPresentationQueryLimits,
    pub limits: FlarkV3HostViewportPresentationOfferLimits,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostViewportPresentationCommitRequest {
    pub offer_id: [u32; 4],
    pub actual_frame_count: u32,
    pub actual_encoded_frame_bytes: u32,
    pub rolling_transport_digest: [u32; 4],
    pub aggregate_root_stream_digest: [u32; 4],
}

impl From<FlarkV3HostViewportPresentationCommitRequest> for ViewportPresentationCommitRequest {
    fn from(request: FlarkV3HostViewportPresentationCommitRequest) -> Self {
        Self {
            offer_id: request.offer_id,
            actual_frame_count: request.actual_frame_count,
            actual_encoded_frame_bytes: request.actual_encoded_frame_bytes,
            rolling_transport_digest: request.rolling_transport_digest,
            aggregate_root_stream_digest: request.aggregate_root_stream_digest,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostViewportPresentationAck {
    pub publication_session: [u32; 4],
    pub base_ack: FlarkV3HostStructuralAck,
    pub binding: FlarkV3HostViewportPresentationBinding,
    pub envelope: FlarkV3HostViewportPresentationEnvelope,
    pub actual_frame_count: u32,
    pub actual_encoded_frame_bytes: u32,
    pub aggregate_root_stream_digest: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostViewportPresentationPollReceipt {
    pub rejection_reason: u32,
    /// `0` pending, `1` packet credit, `2` committed, `3` abort complete,
    /// and `4` closed.
    pub outcome: u32,
    pub offer_id: [u32; 4],
    pub next_frame_ordinal: u32,
    pub ack: FlarkV3HostViewportPresentationAck,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostViewportPresentationQuery {
    pub schema: u32,
    pub struct_size: u32,
    pub ack: FlarkV3HostViewportPresentationAck,
    pub maximum_encoded_bytes: u32,
    pub reserved: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostViewportPresentationQueryReceipt {
    pub rejection_reason: u32,
    /// `0` is unavailable and `1` is an available aggregate page.
    pub outcome: u32,
    pub encoded_bytes: u32,
    pub entry_count: u32,
    pub reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostCallReceipt {
    /// Zero on success; otherwise one of `FLARK_V3_HOST_REJECT_*`.
    pub rejection_reason: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostWorkGrant {
    pub inspect_bytes: u32,
    pub copy_bytes: u32,
    pub transitions: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostPollReceipt {
    pub rejection_reason: u32,
    pub outcome: u32,
    pub offer_id: [u32; 4],
    pub next_frame_ordinal: u32,
    pub ack: FlarkV3HostStructuralAck,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostId128 {
    pub words: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostPointQuery {
    pub schema: u32,
    pub struct_size: u32,
    pub source_version: FlarkV3HostSourceVersion,
    pub position_utf8: u32,
    pub position_utf16: u32,
    /// `0` is upstream and `1` is downstream.
    pub affinity: u32,
    pub maximum_encoded_bytes: u32,
    pub maximum_open_depth: u32,
    pub maximum_leaf_count: u32,
    pub maximum_tree_nodes_visited: u32,
    pub reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostPointQueryReceipt {
    pub rejection_reason: u32,
    /// `1` is a structural viewport and `2` is a typed source gap.
    pub outcome: u32,
    pub gap_reason: u32,
    pub encoded_bytes: u32,
    pub leaf_count: u32,
    pub open_depth: u32,
    pub tree_nodes_visited: u32,
    pub summary_nodes_skipped: u32,
    pub range_start_utf8: u32,
    pub range_start_utf16: u32,
    pub range_end_utf8: u32,
    pub range_end_utf16: u32,
    pub source_version: FlarkV3HostSourceVersion,
    pub reserved: [u32; 5],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FlarkV3HostBlockRangeQuery {
    pub schema: u32,
    pub struct_size: u32,
    pub source_version: FlarkV3HostSourceVersion,
    pub requested_start_utf8: u32,
    pub requested_start_utf16: u32,
    pub requested_end_utf8: u32,
    pub requested_end_utf16: u32,
    pub maximum_encoded_bytes: u32,
    pub maximum_block_count: u32,
    pub maximum_storage_pages_visited: u32,
    pub maximum_open_depth: u32,
    pub maximum_tree_nodes_visited: u32,
    pub continuation_length: u32,
    pub continuation: [u8; FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES],
    pub reserved: [u32; 4],
}

impl Default for FlarkV3HostBlockRangeQuery {
    fn default() -> Self {
        Self {
            schema: 0,
            struct_size: 0,
            source_version: FlarkV3HostSourceVersion::default(),
            requested_start_utf8: 0,
            requested_start_utf16: 0,
            requested_end_utf8: 0,
            requested_end_utf16: 0,
            maximum_encoded_bytes: 0,
            maximum_block_count: 0,
            maximum_storage_pages_visited: 0,
            maximum_open_depth: 0,
            maximum_tree_nodes_visited: 0,
            continuation_length: 0,
            continuation: [0; FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES],
            reserved: [0; 4],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FlarkV3HostBlockRangeQueryReceipt {
    pub rejection_reason: u32,
    /// `1` is a structural range chunk and `2` is a typed source gap.
    pub outcome: u32,
    pub gap_reason: u32,
    pub encoded_bytes: u32,
    pub block_count: u32,
    pub storage_pages_visited: u32,
    pub open_depth: u32,
    pub tree_nodes_visited: u32,
    pub packed_entries_inspected: u32,
    pub summary_nodes_skipped: u32,
    /// Bit zero is set only when the requested range is complete.
    pub flags: u32,
    pub coverage_start_utf8: u32,
    pub coverage_start_utf16: u32,
    pub coverage_end_utf8: u32,
    pub coverage_end_utf16: u32,
    pub source_version: FlarkV3HostSourceVersion,
    pub continuation_length: u32,
    pub continuation: [u8; FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES],
    pub reserved: [u32; 4],
}

impl Default for FlarkV3HostBlockRangeQueryReceipt {
    fn default() -> Self {
        Self {
            rejection_reason: 0,
            outcome: 0,
            gap_reason: 0,
            encoded_bytes: 0,
            block_count: 0,
            storage_pages_visited: 0,
            open_depth: 0,
            tree_nodes_visited: 0,
            packed_entries_inspected: 0,
            summary_nodes_skipped: 0,
            flags: 0,
            coverage_start_utf8: 0,
            coverage_start_utf16: 0,
            coverage_end_utf8: 0,
            coverage_end_utf16: 0,
            source_version: FlarkV3HostSourceVersion::default(),
            continuation_length: 0,
            continuation: [0; FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES],
            reserved: [0; 4],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostStructuralOrdinalWindowQuery {
    pub schema: u32,
    pub struct_size: u32,
    pub source_version: FlarkV3HostSourceVersion,
    pub start_entry_ordinal: FlarkV3HostU64,
    pub maximum_entries: u32,
    pub maximum_storage_pages_visited: u32,
    pub maximum_tree_nodes_visited: u32,
    pub maximum_packed_entries_inspected: u32,
    pub reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FlarkV3HostStructuralOrdinalWindowQueryReceipt {
    pub rejection_reason: u32,
    /// `1` is an exact window and `2` is a typed fail-closed result.
    pub outcome: u32,
    pub failure_reason: u32,
    /// Bit zero is set exactly when `next_entry_ordinal == total_entry_count`.
    pub flags: u32,
    pub total_entry_count: FlarkV3HostU64,
    pub start_entry_ordinal: FlarkV3HostU64,
    pub next_entry_ordinal: FlarkV3HostU64,
    pub start_utf8: u32,
    pub start_utf16: u32,
    pub next_utf8: u32,
    pub next_utf16: u32,
    pub storage_pages_visited: u32,
    pub tree_nodes_visited: u32,
    pub packed_entries_inspected: u32,
    pub summary_nodes_skipped: u32,
    pub source_version: FlarkV3HostSourceVersion,
    pub reserved: [u32; 4],
}

static HOST_REGISTRY: OnceLock<HostRegistry> = OnceLock::new();

struct FinalizerToken {
    handle: HostHandle,
}

fn registry() -> &'static HostRegistry {
    HOST_REGISTRY.get_or_init(HostRegistry::production)
}

#[derive(Debug)]
enum NativeHostError {
    InvalidArgument,
    InvalidConfig,
    BoundExceeded,
    Registry(HostRegistryError),
    Host(HostStoreError),
}

impl From<HostRegistryError> for NativeHostError {
    fn from(error: HostRegistryError) -> Self {
        Self::Registry(error)
    }
}

impl From<HostStoreError> for NativeHostError {
    fn from(error: HostStoreError) -> Self {
        Self::Host(error)
    }
}

type NativeResult<T = ()> = Result<T, NativeHostError>;

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
pub extern "C" fn flark_v3_host_native_abi_version() -> u32 {
    FLARK_V3_HOST_NATIVE_ABI_VERSION
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Writes the production-independent default config shell. The caller must set
/// its document session before create.
///
/// # Safety
/// `out_config` must be aligned and uniquely writable for one config.
pub unsafe extern "C" fn flark_v3_host_config_standard(out_config: *mut FlarkV3HostConfig) -> u32 {
    ffi_guard(|| {
        write_output(
            out_config,
            FlarkV3HostConfig {
                abi_version: FLARK_V3_HOST_NATIVE_ABI_VERSION,
                struct_size: size_of::<FlarkV3HostConfig>() as u32,
                grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
                syntax_profile: 1,
                authority_mask: 0x1f,
                maximum_query_bytes: 64 * 1024,
                ..FlarkV3HostConfig::default()
            },
        )
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `config` is readable and `out_handle` uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_host_create(
    config: *const FlarkV3HostConfig,
    out_handle: *mut FlarkV3HostHandle,
) -> u32 {
    ffi_guard(|| {
        write_output(out_handle, FlarkV3HostHandle::default())?;
        let config = host_config(read_input(config)?)?;
        let handle = registry().create(config)?;
        write_output(out_handle, handle.into())
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `source` is readable and `out_receipt` uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_host_observe_source(
    handle: FlarkV3HostHandle,
    source: *const FlarkV3HostSourceVersion,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let source = SourceVersion::from(read_input(source)?);
        registry()
            .with_host(handle.into(), |host| host.observe_source_version(source))?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `begin` is readable and `out_receipt` uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_host_begin_offer(
    handle: FlarkV3HostHandle,
    begin: *const FlarkV3HostOfferBegin,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let begin = OfferBegin::from(read_input(begin)?);
        registry()
            .with_host(handle.into(), |host| host.begin_offer(begin))?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Begins one exact-base hot-inline sidecar offer without entering structural
/// publication state.
///
/// # Safety
/// `begin` is readable and `out_receipt` uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_host_begin_inline_sidecar_offer(
    handle: FlarkV3HostHandle,
    begin: *const FlarkV3HostInlineSidecarBegin,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let begin = host_inline_sidecar_begin(read_input(begin)?)?;
        registry()
            .with_host(handle.into(), |host| host.begin_inline_sidecar_offer(begin))?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Begins one exact-base aggregate VPB1 page offer without entering structural
/// or singleton-sidecar publication state.
///
/// # Safety
/// `begin` is readable and disjoint from the uniquely writable `out_receipt`.
pub unsafe extern "C" fn flark_v3_host_begin_viewport_presentation_offer(
    handle: FlarkV3HostHandle,
    begin: *const FlarkV3HostViewportPresentationBegin,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let begin = host_viewport_presentation_begin(read_input(begin)?)?;
        registry()
            .with_host(handle.into(), |host| {
                host.begin_viewport_presentation_offer(begin)
            })?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Begins an exact-base References delta.
///
/// This is a distinct ABI operation rather than a mode flag on the full
/// snapshot entry point. `base_ack` must exactly equal the ACK for the root
/// currently installed in this host.
///
/// # Safety
/// `begin` and `base_ack` are readable and `out_receipt` is uniquely writable
/// for this call.
pub unsafe extern "C" fn flark_v3_host_begin_references_delta(
    handle: FlarkV3HostHandle,
    begin: *const FlarkV3HostOfferBegin,
    base_ack: *const FlarkV3HostStructuralAck,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let begin = read_input(begin)?.into_offer(
            PublicationMode::ExactBaseReferencesDelta,
            Some(StructuralAck::from(read_input(base_ack)?)),
        );
        registry()
            .with_host(handle.into(), |host| host.begin_offer(begin))?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Begins a typed exact-base delta.
///
/// This is distinct from both a full snapshot and the legacy References-only
/// delta. `base_ack` must exactly equal the ACK for the root currently
/// installed in this host; the admitted e4/e5 program then reuses canonical
/// References and replays persistent SourceFacts before ordinary target nodes.
///
/// # Safety
/// `begin` and `base_ack` are readable and `out_receipt` is uniquely writable
/// for this call.
pub unsafe extern "C" fn flark_v3_host_begin_exact_base_delta(
    handle: FlarkV3HostHandle,
    begin: *const FlarkV3HostOfferBegin,
    base_ack: *const FlarkV3HostStructuralAck,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let begin = read_input(begin)?.into_offer(
            PublicationMode::ExactBaseDelta,
            Some(StructuralAck::from(read_input(base_ack)?)),
        );
        registry()
            .with_host(handle.into(), |host| host.begin_offer(begin))?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Admits one exact FPK3 packet. The host copies it once, then validates its
/// descriptors and complete frames incrementally under poll grants. Malformed
/// pointer/length/magic/version/envelope input returns ABI `Invalid` with a
/// zero rejection receipt because no valid offer-scoped command existed.
///
/// # Safety
/// `packet` is readable for `packet_length` bytes and disjoint from the
/// uniquely writable `out_receipt`. A null packet is valid only at zero length.
pub unsafe extern "C" fn flark_v3_host_admit_packet(
    handle: FlarkV3HostHandle,
    packet: *const u8,
    packet_length: u32,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let packet = read_buffer(packet, packet_length, FLARK_V3_HOST_MAXIMUM_PACKET_BYTES)?;
        registry()
            .admit_packet(handle.into(), packet)?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Admits one exact FPK3 packet to the active hot-inline sidecar offer.
///
/// # Safety
/// `packet` is readable for `packet_length` bytes and disjoint from the
/// uniquely writable `out_receipt`.
pub unsafe extern "C" fn flark_v3_host_admit_inline_sidecar_packet(
    handle: FlarkV3HostHandle,
    packet: *const u8,
    packet_length: u32,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let encoded = read_buffer(packet, packet_length, FLARK_V3_HOST_MAXIMUM_PACKET_BYTES)?;
        let packet = decode_publication_packet_envelope(encoded)
            .map_err(|_| HostRegistryError::InvalidPacketEnvelope)?;
        registry()
            .with_host(handle.into(), |host| {
                host.admit_inline_sidecar_packet(packet)
            })?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Admits one exact FPK3 packet to the active aggregate VPB1 page offer.
/// Malformed pointer, bound, magic, version, or envelope input is rejected
/// before the addressed host observes it.
///
/// # Safety
/// `packet` is readable for `packet_length` bytes and disjoint from the
/// uniquely writable `out_receipt`.
pub unsafe extern "C" fn flark_v3_host_admit_viewport_presentation_packet(
    handle: FlarkV3HostHandle,
    packet: *const u8,
    packet_length: u32,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let packet = read_buffer(packet, packet_length, FLARK_V3_HOST_MAXIMUM_PACKET_BYTES)?;
        registry()
            .admit_viewport_presentation_packet(handle.into(), packet)?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `request` is readable and `out_receipt` uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_host_request_commit(
    handle: FlarkV3HostHandle,
    request: *const FlarkV3HostCommitRequest,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let request = CommitRequest::from(read_input(request)?);
        registry()
            .with_host(handle.into(), |host| host.request_commit(request))?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `request` is readable and `out_receipt` uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_host_request_inline_sidecar_commit(
    handle: FlarkV3HostHandle,
    request: *const FlarkV3HostInlineSidecarCommitRequest,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let request = HotInlineSidecarCommitRequest::from(read_input(request)?);
        registry()
            .with_host(handle.into(), |host| {
                host.request_inline_sidecar_commit(request)
            })?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `request` is readable and disjoint from the uniquely writable
/// `out_receipt`.
pub unsafe extern "C" fn flark_v3_host_request_viewport_presentation_commit(
    handle: FlarkV3HostHandle,
    request: *const FlarkV3HostViewportPresentationCommitRequest,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let request = ViewportPresentationCommitRequest::from(read_input(request)?);
        registry()
            .with_host(handle.into(), |host| {
                host.request_viewport_presentation_commit(request)
            })?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `offer_id` is readable and `out_receipt` uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_host_abort_offer(
    handle: FlarkV3HostHandle,
    offer_id: *const FlarkV3HostId128,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let offer_id = read_input(offer_id)?.words;
        registry()
            .with_host(handle.into(), |host| host.abort_offer(offer_id))?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `offer_id` is readable and `out_receipt` uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_host_abort_inline_sidecar_offer(
    handle: FlarkV3HostHandle,
    offer_id: *const FlarkV3HostId128,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let offer_id = read_input(offer_id)?.words;
        registry()
            .with_host(handle.into(), |host| {
                host.abort_inline_sidecar_offer(offer_id)
            })?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `offer_id` is readable and disjoint from the uniquely writable
/// `out_receipt`.
pub unsafe extern "C" fn flark_v3_host_abort_viewport_presentation_offer(
    handle: FlarkV3HostHandle,
    offer_id: *const FlarkV3HostId128,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let offer_id = read_input(offer_id)?.words;
        registry()
            .with_host(handle.into(), |host| {
                host.abort_viewport_presentation_offer(offer_id)
            })?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `out_receipt` is aligned and uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_host_poll(
    handle: FlarkV3HostHandle,
    grant: FlarkV3HostWorkGrant,
    out_receipt: *mut FlarkV3HostPollReceipt,
) -> u32 {
    ffi_guard(|| {
        write_output(out_receipt, FlarkV3HostPollReceipt::default())?;
        let result = registry().with_host(handle.into(), |host| {
            host.poll(HostWorkGrant {
                inspect_bytes: grant.inspect_bytes,
                copy_bytes: grant.copy_bytes,
                transitions: grant.transitions,
            })
        })?;
        match result {
            Ok(outcome) => write_output(out_receipt, poll_receipt(outcome)),
            Err(error) => {
                write_output(
                    out_receipt,
                    FlarkV3HostPollReceipt {
                        rejection_reason: reject_reason(error.reason()),
                        ..FlarkV3HostPollReceipt::default()
                    },
                )?;
                Err(NativeHostError::Host(error))
            }
        }
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Advances only the active hot-inline sidecar lifecycle under the shared
/// bounded work-grant shape.
///
/// # Safety
/// `out_receipt` is aligned and uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_host_poll_inline_sidecar(
    handle: FlarkV3HostHandle,
    grant: FlarkV3HostWorkGrant,
    out_receipt: *mut FlarkV3HostInlineSidecarPollReceipt,
) -> u32 {
    ffi_guard(|| {
        write_output(out_receipt, FlarkV3HostInlineSidecarPollReceipt::default())?;
        let result = registry().with_host(handle.into(), |host| {
            host.poll_inline_sidecar(HostWorkGrant {
                inspect_bytes: grant.inspect_bytes,
                copy_bytes: grant.copy_bytes,
                transitions: grant.transitions,
            })
        })?;
        match result {
            Ok(outcome) => write_output(out_receipt, inline_sidecar_poll_receipt(outcome)),
            Err(error) => {
                write_output(
                    out_receipt,
                    FlarkV3HostInlineSidecarPollReceipt {
                        rejection_reason: reject_reason(error.reason()),
                        ..FlarkV3HostInlineSidecarPollReceipt::default()
                    },
                )?;
                Err(NativeHostError::Host(error))
            }
        }
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Advances only the active aggregate VPB1 page lifecycle under the shared
/// bounded work-grant shape.
///
/// # Safety
/// `out_receipt` is aligned and uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_host_poll_viewport_presentation(
    handle: FlarkV3HostHandle,
    grant: FlarkV3HostWorkGrant,
    out_receipt: *mut FlarkV3HostViewportPresentationPollReceipt,
) -> u32 {
    ffi_guard(|| {
        write_output(
            out_receipt,
            FlarkV3HostViewportPresentationPollReceipt::default(),
        )?;
        let result = registry().with_host(handle.into(), |host| {
            host.poll_viewport_presentation(HostWorkGrant {
                inspect_bytes: grant.inspect_bytes,
                copy_bytes: grant.copy_bytes,
                transitions: grant.transitions,
            })
        })?;
        match result {
            Ok(outcome) => write_output(out_receipt, viewport_presentation_poll_receipt(outcome)),
            Err(error) => {
                write_output(
                    out_receipt,
                    FlarkV3HostViewportPresentationPollReceipt {
                        rejection_reason: reject_reason(error.reason()),
                        ..FlarkV3HostViewportPresentationPollReceipt::default()
                    },
                )?;
                Err(NativeHostError::Host(error))
            }
        }
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `ack` is readable and `out_receipt` uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_host_acknowledge_delivery(
    handle: FlarkV3HostHandle,
    ack: *const FlarkV3HostStructuralAck,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let ack = StructuralAck::from(read_input(ack)?);
        registry()
            .with_host(handle.into(), |host| host.acknowledge_delivery(ack))?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `ack` is readable and `out_receipt` uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_host_acknowledge_inline_sidecar_delivery(
    handle: FlarkV3HostHandle,
    ack: *const FlarkV3HostInlineSidecarAck,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let ack = host_inline_sidecar_ack(read_input(ack)?)?;
        registry()
            .with_host(handle.into(), |host| {
                host.acknowledge_inline_sidecar_delivery(ack)
            })?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `ack` is readable and disjoint from the uniquely writable `out_receipt`.
pub unsafe extern "C" fn flark_v3_host_acknowledge_viewport_presentation_delivery(
    handle: FlarkV3HostHandle,
    ack: *const FlarkV3HostViewportPresentationAck,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        let ack = host_viewport_presentation_ack(read_input(ack)?)?;
        registry()
            .with_host(handle.into(), |host| {
                host.acknowledge_viewport_presentation_delivery(ack)
            })?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Authors one bounded point-query viewport or typed source-gap receipt. The
/// output is caller-owned fixed scratch; no source or document-sized buffer is
/// copied.
///
/// # Safety
/// `query` is readable. `output` is uniquely writable for `capacity` bytes and
/// disjoint from the uniquely writable `out_receipt`. A null output is valid
/// only at zero capacity.
pub unsafe extern "C" fn flark_v3_host_query_structural(
    handle: FlarkV3HostHandle,
    query: *const FlarkV3HostPointQuery,
    output: *mut u8,
    capacity: u32,
    out_receipt: *mut FlarkV3HostPointQueryReceipt,
) -> u32 {
    ffi_guard(|| {
        write_output(out_receipt, FlarkV3HostPointQueryReceipt::default())?;
        let query = host_point_query(read_input(query)?)?;
        let output = write_buffer(output, capacity, FLARK_V3_HOST_MAXIMUM_QUERY_BYTES)?;
        match registry().with_host(handle.into(), |host| host.query_structural(query, output))? {
            Ok(outcome) => write_output(out_receipt, point_query_receipt(outcome)),
            Err(error) => {
                write_output(
                    out_receipt,
                    FlarkV3HostPointQueryReceipt {
                        rejection_reason: reject_reason(error.reason()),
                        ..FlarkV3HostPointQueryReceipt::default()
                    },
                )?;
                Err(NativeHostError::Host(error))
            }
        }
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Authors one bounded, structure-only top-level block range page or typed
/// source-gap receipt. Normal truncation returns an opaque continuation in
/// the receipt.
///
/// # Safety
/// `query` is readable. `output` is uniquely writable for `capacity` bytes and
/// disjoint from the uniquely writable `out_receipt`. A null output is valid
/// only at zero capacity.
pub unsafe extern "C" fn flark_v3_host_query_structural_range(
    handle: FlarkV3HostHandle,
    query: *const FlarkV3HostBlockRangeQuery,
    output: *mut u8,
    capacity: u32,
    out_receipt: *mut FlarkV3HostBlockRangeQueryReceipt,
) -> u32 {
    ffi_guard(|| {
        write_output(out_receipt, FlarkV3HostBlockRangeQueryReceipt::default())?;
        let query = host_block_range_query(read_input(query)?)?;
        if capacity != query.budget.maximum_encoded_bytes {
            return Err(NativeHostError::InvalidArgument);
        }
        let output = write_buffer(output, capacity, FLARK_V3_HOST_MAXIMUM_QUERY_BYTES)?;
        match registry().with_host(handle.into(), |host| {
            host.query_structural_range(query, output)
        })? {
            Ok(outcome) => write_output(out_receipt, block_range_query_receipt(outcome)),
            Err(error) => {
                write_output(
                    out_receipt,
                    FlarkV3HostBlockRangeQueryReceipt {
                        rejection_reason: reject_reason(error.reason()),
                        ..FlarkV3HostBlockRangeQueryReceipt::default()
                    },
                )?;
                Err(NativeHostError::Host(error))
            }
        }
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Locates one bounded top-level structural ordinal window and its exact
/// UTF-8/UTF-16 cuts. No structural records or source bytes are copied.
///
/// # Safety
/// `query` is readable and disjoint from the uniquely writable `out_receipt`.
pub unsafe extern "C" fn flark_v3_host_query_structural_ordinal_window(
    handle: FlarkV3HostHandle,
    query: *const FlarkV3HostStructuralOrdinalWindowQuery,
    out_receipt: *mut FlarkV3HostStructuralOrdinalWindowQueryReceipt,
) -> u32 {
    ffi_guard(|| {
        write_output(
            out_receipt,
            FlarkV3HostStructuralOrdinalWindowQueryReceipt::default(),
        )?;
        let query = host_structural_ordinal_window_query(read_input(query)?)?;
        match registry().with_host(handle.into(), |host| {
            host.query_structural_ordinal_window(query)
        })? {
            Ok(outcome) => write_output(
                out_receipt,
                structural_ordinal_window_query_receipt(outcome),
            ),
            Err(error) => {
                write_output(
                    out_receipt,
                    FlarkV3HostStructuralOrdinalWindowQueryReceipt {
                        rejection_reason: reject_reason(error.reason()),
                        ..FlarkV3HostStructuralOrdinalWindowQueryReceipt::default()
                    },
                )?;
                Err(NativeHostError::Host(error))
            }
        }
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Copies one exact installed sidecar root into bounded caller-owned scratch.
/// Product queries normally consume the sidecar joined into the structural
/// viewport; this narrow operation preserves a typed native seam for
/// independent validation and unavailable/unsupported certificates.
///
/// # Safety
/// `query` is readable. `output` is uniquely writable for `capacity` bytes and
/// disjoint from `out_receipt`.
pub unsafe extern "C" fn flark_v3_host_query_inline_sidecar(
    handle: FlarkV3HostHandle,
    query: *const FlarkV3HostInlineSidecarQuery,
    output: *mut u8,
    capacity: u32,
    out_receipt: *mut FlarkV3HostInlineSidecarQueryReceipt,
) -> u32 {
    ffi_guard(|| {
        write_output(out_receipt, FlarkV3HostInlineSidecarQueryReceipt::default())?;
        let query = read_input(query)?;
        let (binding, maximum_encoded_bytes) = host_inline_sidecar_query(query)?;
        if capacity != maximum_encoded_bytes {
            return Err(NativeHostError::InvalidArgument);
        }
        let output = write_buffer(
            output,
            capacity,
            FLARK_V3_HOST_INLINE_SIDECAR_MAXIMUM_QUERY_BYTES,
        )?;
        match registry().with_host(handle.into(), |host| {
            host.query_inline_sidecar(binding, output)
        })? {
            Ok(outcome) => write_output(out_receipt, inline_sidecar_query_receipt(outcome)),
            Err(error) => {
                write_output(
                    out_receipt,
                    FlarkV3HostInlineSidecarQueryReceipt {
                        rejection_reason: reject_reason(error.reason()),
                        ..FlarkV3HostInlineSidecarQueryReceipt::default()
                    },
                )?;
                Err(NativeHostError::Host(error))
            }
        }
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// Copies one exact installed aggregate VPB1 page into bounded caller-owned
/// scratch. A mismatched or superseded ACK reports the typed unavailable
/// outcome. Too-small scratch reports QUERY_BOUND_EXCEEDED and leaves
/// `encoded_bytes` and `entry_count` zero.
///
/// # Safety
/// `query` is readable. `output` is uniquely writable for `capacity` bytes.
/// Query, output, and receipt ranges must be pairwise disjoint.
pub unsafe extern "C" fn flark_v3_host_query_viewport_presentation(
    handle: FlarkV3HostHandle,
    query: *const FlarkV3HostViewportPresentationQuery,
    output: *mut u8,
    capacity: u32,
    out_receipt: *mut FlarkV3HostViewportPresentationQueryReceipt,
) -> u32 {
    ffi_guard(|| {
        write_output(
            out_receipt,
            FlarkV3HostViewportPresentationQueryReceipt::default(),
        )?;
        let (ack, maximum_encoded_bytes) = host_viewport_presentation_query(read_input(query)?)?;
        if capacity != maximum_encoded_bytes {
            return Err(NativeHostError::InvalidArgument);
        }
        let output = write_buffer(
            output,
            capacity,
            FLARK_V3_HOST_VIEWPORT_PRESENTATION_MAXIMUM_QUERY_BYTES,
        )?;
        match registry().with_host(handle.into(), |host| {
            host.query_viewport_presentation(ack, maximum_encoded_bytes, output)
        })? {
            Ok(outcome) => write_output(out_receipt, viewport_presentation_query_receipt(outcome)),
            Err(error) => {
                write_output(
                    out_receipt,
                    FlarkV3HostViewportPresentationQueryReceipt {
                        rejection_reason: reject_reason(error.reason()),
                        ..FlarkV3HostViewportPresentationQueryReceipt::default()
                    },
                )?;
                Err(NativeHostError::Host(error))
            }
        }
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `out_receipt` is aligned and uniquely writable for this call.
pub unsafe extern "C" fn flark_v3_host_close(
    handle: FlarkV3HostHandle,
    out_receipt: *mut FlarkV3HostCallReceipt,
) -> u32 {
    ffi_host_call(out_receipt, || {
        registry()
            .with_host(handle.into(), |host| host.begin_close())?
            .map_err(Into::into)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
pub extern "C" fn flark_v3_host_remove(handle: FlarkV3HostHandle) -> u32 {
    ffi_guard(|| registry().remove(handle.into()).map_err(Into::into))
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
pub extern "C" fn flark_v3_host_emergency_destroy(handle: FlarkV3HostHandle) -> u32 {
    ffi_guard(|| {
        registry().emergency_destroy(handle.into())?;
        Ok(())
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `out_token` is aligned and uniquely writable. The returned token is
/// consumed exactly once by release or emergency finalize.
pub unsafe extern "C" fn flark_v3_host_finalizer_token_create(
    handle: FlarkV3HostHandle,
    out_token: *mut *mut c_void,
) -> u32 {
    ffi_guard(|| {
        write_output(out_token, std::ptr::null_mut())?;
        let handle = HostHandle::from(handle);
        registry().validate_live(handle)?;
        let token = Box::into_raw(Box::new(FinalizerToken { handle })).cast::<c_void>();
        write_output(out_token, token)
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `token` is one still-live token returned by token_create.
pub unsafe extern "C" fn flark_v3_host_finalizer_token_release(token: *mut c_void) -> u32 {
    ffi_guard(|| {
        if token.is_null() {
            return Err(NativeHostError::InvalidArgument);
        }
        unsafe { drop(Box::from_raw(token.cast::<FinalizerToken>())) };
        Ok(())
    })
}

#[cfg_attr(not(target_arch = "wasm32"), no_mangle)]
/// # Safety
/// `token` is null or one still-live token returned by token_create.
pub unsafe extern "C" fn flark_v3_host_emergency_finalize(token: *mut c_void) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if token.is_null() {
            return;
        }
        let token = unsafe { Box::from_raw(token.cast::<FinalizerToken>()) };
        let _ = registry().emergency_destroy(token.handle);
    }));
}

fn host_config(config: FlarkV3HostConfig) -> NativeResult<HostConfig> {
    if config.abi_version != FLARK_V3_HOST_NATIVE_ABI_VERSION
        || config.struct_size as usize != size_of::<FlarkV3HostConfig>()
        || config.reserved != [0; 4]
    {
        return Err(NativeHostError::InvalidConfig);
    }
    HostConfig {
        document_session: config.document_session,
        grammar_revision: config.grammar_revision,
        syntax_profile: config.syntax_profile,
        authority_mask: config.authority_mask,
        maximum_query_bytes: config.maximum_query_bytes,
    }
    .validate()
    .map_err(|_| NativeHostError::InvalidConfig)
}

fn host_inline_sidecar_begin(
    begin: FlarkV3HostInlineSidecarBegin,
) -> NativeResult<HotInlineSidecarBegin> {
    if begin.mode != 1 {
        return Err(NativeHostError::InvalidArgument);
    }
    let disposition = match begin.sidecar_disposition.disposition {
        1 if begin.sidecar_disposition.reason == 0 => HotInlineSidecarDisposition::Authoritative {
            logical_page_count: begin.sidecar_disposition.logical_page_count.into(),
            fact_count: begin.sidecar_disposition.fact_count.into(),
            storage_page_count: begin.sidecar_disposition.storage_page_count.into(),
            link_value_entry_count: begin.sidecar_disposition.link_value_entry_count,
            link_value_encoded_bytes: begin.sidecar_disposition.link_value_encoded_bytes,
            link_value_storage_page_count: begin
                .sidecar_disposition
                .link_value_storage_page_count
                .into(),
            ordered_commitment256: digest256_from_words(begin.sidecar_disposition.commitment256),
        },
        2 if u64::from(begin.sidecar_disposition.logical_page_count) == 0
            && u64::from(begin.sidecar_disposition.fact_count) == 0
            && u64::from(begin.sidecar_disposition.storage_page_count) == 0
            && begin.sidecar_disposition.link_value_entry_count == 0
            && begin.sidecar_disposition.link_value_encoded_bytes == 0
            && u64::from(begin.sidecar_disposition.link_value_storage_page_count) == 0 =>
        {
            HotInlineSidecarDisposition::Unsupported {
                reason: begin.sidecar_disposition.reason,
                metadata_commitment256: digest256_from_words(
                    begin.sidecar_disposition.commitment256,
                ),
            }
        }
        _ => return Err(NativeHostError::InvalidArgument),
    };
    Ok(HotInlineSidecarBegin {
        schema: begin.schema,
        mode: HotInlineSidecarMode::HotInlineSidecar,
        offer_id: begin.offer_id,
        publication_session: begin.publication_session,
        base_ack: begin.base_ack.into(),
        binding: begin.binding.into(),
        envelope: HotInlineSidecarEnvelopeMetrics {
            hio1_encoded_bytes: begin.hio1_encoded_bytes,
            ipr2_descriptor_bytes: begin.ipr2_descriptor_bytes,
            transferred_node_count: begin.transferred_node_count,
            hio1_envelope_digest256: digest256_from_words(begin.hio1_envelope_digest256),
            disposition,
        },
        limits: OfferLimits {
            maximum_frame_count: begin.maximum_frame_count,
            maximum_encoded_frame_bytes: begin.maximum_encoded_frame_bytes,
            maximum_packet_bytes: begin.maximum_packet_bytes,
            maximum_frame_bytes: begin.maximum_frame_bytes,
            maximum_program_children: begin.maximum_program_children,
        },
    })
}

fn host_inline_sidecar_ack(ack: FlarkV3HostInlineSidecarAck) -> NativeResult<InlineSidecarAck> {
    let disposition = match ack.disposition {
        1 => InlineSidecarAckDisposition::Authoritative,
        2 => InlineSidecarAckDisposition::Unsupported,
        _ => return Err(NativeHostError::InvalidArgument),
    };
    Ok(InlineSidecarAck {
        publication_session: ack.publication_session,
        base_ack: ack.base_ack.into(),
        refinement_generation: ack.refinement_generation.into(),
        block_ordinal: ack.block_ordinal.into(),
        transferred_node_count: ack.transferred_node_count,
        disposition,
        hio1_envelope_digest256: digest256_from_words(ack.hio1_envelope_digest256),
        root_stream_digest: ack.root_stream_digest,
    })
}

fn ffi_inline_sidecar_ack(ack: InlineSidecarAck) -> FlarkV3HostInlineSidecarAck {
    FlarkV3HostInlineSidecarAck {
        publication_session: ack.publication_session,
        base_ack: ack.base_ack.into(),
        refinement_generation: ack.refinement_generation.into(),
        block_ordinal: ack.block_ordinal.into(),
        transferred_node_count: ack.transferred_node_count,
        disposition: match ack.disposition {
            InlineSidecarAckDisposition::Authoritative => 1,
            InlineSidecarAckDisposition::Unsupported => 2,
        },
        hio1_envelope_digest256: digest256_to_words(ack.hio1_envelope_digest256),
        root_stream_digest: ack.root_stream_digest,
    }
}

fn host_viewport_presentation_binding(
    binding: FlarkV3HostViewportPresentationBinding,
) -> NativeResult<ViewportPresentationBinding> {
    let complete = match binding.complete {
        0 => false,
        1 => true,
        _ => return Err(NativeHostError::InvalidArgument),
    };
    Ok(ViewportPresentationBinding {
        viewport_generation: binding.viewport_generation,
        requested_range: binding.requested_range.into(),
        covered_range: binding.covered_range.into(),
        start: binding.start.into(),
        next: binding.next.into(),
        complete,
    })
}

fn host_viewport_presentation_begin(
    begin: FlarkV3HostViewportPresentationBegin,
) -> NativeResult<ViewportPresentationBegin> {
    if begin.mode != 1 {
        return Err(NativeHostError::InvalidArgument);
    }
    Ok(ViewportPresentationBegin {
        schema: begin.schema,
        mode: ViewportPresentationMode::AggregatePage,
        offer_id: begin.offer_id,
        publication_session: begin.publication_session,
        base_ack: begin.base_ack.into(),
        binding: host_viewport_presentation_binding(begin.binding)?,
        envelope: begin.envelope.into(),
        query_limits: begin.query_limits.into(),
        limits: begin.limits.into(),
    })
}

fn host_viewport_presentation_ack(
    ack: FlarkV3HostViewportPresentationAck,
) -> NativeResult<ViewportPresentationAck> {
    Ok(ViewportPresentationAck {
        publication_session: ack.publication_session,
        base_ack: ack.base_ack.into(),
        binding: host_viewport_presentation_binding(ack.binding)?,
        envelope: ack.envelope.into(),
        actual_frame_count: ack.actual_frame_count,
        actual_encoded_frame_bytes: ack.actual_encoded_frame_bytes,
        aggregate_root_stream_digest: ack.aggregate_root_stream_digest,
    })
}

fn ffi_viewport_presentation_ack(
    ack: ViewportPresentationAck,
) -> FlarkV3HostViewportPresentationAck {
    FlarkV3HostViewportPresentationAck {
        publication_session: ack.publication_session,
        base_ack: ack.base_ack.into(),
        binding: ack.binding.into(),
        envelope: ack.envelope.into(),
        actual_frame_count: ack.actual_frame_count,
        actual_encoded_frame_bytes: ack.actual_encoded_frame_bytes,
        aggregate_root_stream_digest: ack.aggregate_root_stream_digest,
    }
}

fn host_viewport_presentation_query(
    query: FlarkV3HostViewportPresentationQuery,
) -> NativeResult<(ViewportPresentationAck, u32)> {
    if query.schema != FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_SCHEMA
        || query.struct_size as usize != size_of::<FlarkV3HostViewportPresentationQuery>()
        || query.maximum_encoded_bytes == 0
        || query.maximum_encoded_bytes > FLARK_V3_HOST_VIEWPORT_PRESENTATION_MAXIMUM_QUERY_BYTES
        || query.reserved != [0; 3]
    {
        return Err(NativeHostError::InvalidArgument);
    }
    Ok((
        host_viewport_presentation_ack(query.ack)?,
        query.maximum_encoded_bytes,
    ))
}

fn host_inline_sidecar_query(
    query: FlarkV3HostInlineSidecarQuery,
) -> NativeResult<(HotInlineSidecarBinding, u32)> {
    if query.schema != FLARK_V3_HOST_INLINE_SIDECAR_QUERY_SCHEMA
        || query.struct_size as usize != size_of::<FlarkV3HostInlineSidecarQuery>()
        || query.maximum_encoded_bytes == 0
        || query.maximum_encoded_bytes > FLARK_V3_HOST_INLINE_SIDECAR_MAXIMUM_QUERY_BYTES
        || query.reserved != [0; 3]
    {
        return Err(NativeHostError::InvalidArgument);
    }
    Ok((query.binding.into(), query.maximum_encoded_bytes))
}

fn digest256_from_words(words: [u32; 8]) -> [u8; 32] {
    let mut digest = [0_u8; 32];
    for (index, word) in words.into_iter().enumerate() {
        let start = index * 4;
        digest[start..start + 4].copy_from_slice(&word.to_le_bytes());
    }
    digest
}

fn digest256_to_words(digest: [u8; 32]) -> [u32; 8] {
    let mut words = [0_u32; 8];
    for (index, word) in words.iter_mut().enumerate() {
        let start = index * 4;
        *word = u32::from_le_bytes(
            digest[start..start + 4]
                .try_into()
                .expect("one fixed digest lane"),
        );
    }
    words
}

fn host_point_query(query: FlarkV3HostPointQuery) -> NativeResult<HostPointQuery> {
    if query.schema != FLARK_V3_HOST_POINT_QUERY_SCHEMA
        || query.struct_size as usize != size_of::<FlarkV3HostPointQuery>()
        || query.reserved != [0; 4]
    {
        return Err(NativeHostError::InvalidArgument);
    }
    let affinity = match query.affinity {
        0 => HostMetricAffinity::Upstream,
        1 => HostMetricAffinity::Downstream,
        _ => return Err(NativeHostError::InvalidArgument),
    };
    Ok(HostPointQuery {
        source_version: query.source_version.into(),
        position: HostSourceMetric {
            bytes: query.position_utf8,
            utf16: query.position_utf16,
        },
        affinity,
        budget: HostQueryBudget {
            maximum_encoded_bytes: query.maximum_encoded_bytes,
            maximum_open_depth: query.maximum_open_depth,
            maximum_leaf_count: query.maximum_leaf_count,
            maximum_tree_nodes_visited: query.maximum_tree_nodes_visited,
        },
    })
}

fn host_block_range_query(query: FlarkV3HostBlockRangeQuery) -> NativeResult<HostBlockRangeQuery> {
    if query.schema != FLARK_V3_HOST_BLOCK_RANGE_QUERY_SCHEMA
        || query.struct_size as usize != size_of::<FlarkV3HostBlockRangeQuery>()
        || query.reserved != [0; 4]
    {
        return Err(NativeHostError::InvalidArgument);
    }
    let continuation = match query.continuation_length as usize {
        0 if query.continuation == [0; FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES] => None,
        FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES => {
            Some(HostBlockRangeContinuation::from_encoded(query.continuation))
        }
        _ => return Err(NativeHostError::InvalidArgument),
    };
    Ok(HostBlockRangeQuery {
        source_version: query.source_version.into(),
        requested_range: HostMetricRange {
            start: HostSourceMetric {
                bytes: query.requested_start_utf8,
                utf16: query.requested_start_utf16,
            },
            end: HostSourceMetric {
                bytes: query.requested_end_utf8,
                utf16: query.requested_end_utf16,
            },
        },
        budget: HostBlockRangeBudget {
            maximum_encoded_bytes: query.maximum_encoded_bytes,
            maximum_block_count: query.maximum_block_count,
            maximum_storage_pages_visited: query.maximum_storage_pages_visited,
            maximum_open_depth: query.maximum_open_depth,
            maximum_tree_nodes_visited: query.maximum_tree_nodes_visited,
        },
        continuation,
    })
}

fn host_structural_ordinal_window_query(
    query: FlarkV3HostStructuralOrdinalWindowQuery,
) -> NativeResult<HostStructuralOrdinalWindowQuery> {
    if query.schema != FLARK_V3_HOST_STRUCTURAL_ORDINAL_WINDOW_QUERY_SCHEMA
        || query.struct_size as usize != size_of::<FlarkV3HostStructuralOrdinalWindowQuery>()
        || query.reserved != [0; 4]
    {
        return Err(NativeHostError::InvalidArgument);
    }
    Ok(HostStructuralOrdinalWindowQuery {
        source_version: query.source_version.into(),
        start_entry_ordinal: query.start_entry_ordinal.into(),
        budget: HostStructuralOrdinalWindowBudget {
            maximum_entries: query.maximum_entries,
            maximum_storage_pages_visited: query.maximum_storage_pages_visited,
            maximum_tree_nodes_visited: query.maximum_tree_nodes_visited,
            maximum_packed_entries_inspected: query.maximum_packed_entries_inspected,
        },
    })
}

fn point_query_receipt(outcome: HostStructuralQueryOutcome) -> FlarkV3HostPointQueryReceipt {
    match outcome {
        HostStructuralQueryOutcome::Viewport {
            source_version,
            range,
            receipt,
        } => point_query_receipt_fields(1, 0, source_version, range, receipt),
        HostStructuralQueryOutcome::SourceGap {
            source_version,
            range,
            reason,
            receipt,
        } => point_query_receipt_fields(2, gap_reason(reason), source_version, range, receipt),
    }
}

fn structural_ordinal_window_query_receipt(
    outcome: HostStructuralOrdinalWindowOutcome,
) -> FlarkV3HostStructuralOrdinalWindowQueryReceipt {
    match outcome {
        HostStructuralOrdinalWindowOutcome::Window {
            source_version,
            total_entry_count,
            start_entry_ordinal,
            next_entry_ordinal,
            start,
            next,
            complete,
            receipt,
        } => FlarkV3HostStructuralOrdinalWindowQueryReceipt {
            outcome: 1,
            flags: u32::from(complete),
            total_entry_count: total_entry_count.into(),
            start_entry_ordinal: start_entry_ordinal.into(),
            next_entry_ordinal: next_entry_ordinal.into(),
            start_utf8: start.bytes,
            start_utf16: start.utf16,
            next_utf8: next.bytes,
            next_utf16: next.utf16,
            source_version: source_version.into(),
            ..structural_ordinal_window_receipt_fields(receipt)
        },
        HostStructuralOrdinalWindowOutcome::Failure {
            source_version,
            total_entry_count,
            start_entry_ordinal,
            reason,
            receipt,
        } => FlarkV3HostStructuralOrdinalWindowQueryReceipt {
            outcome: 2,
            failure_reason: structural_ordinal_window_failure_reason(reason),
            total_entry_count: total_entry_count.into(),
            start_entry_ordinal: start_entry_ordinal.into(),
            source_version: source_version.into(),
            ..structural_ordinal_window_receipt_fields(receipt)
        },
    }
}

fn structural_ordinal_window_receipt_fields(
    receipt: HostStructuralOrdinalWindowReceipt,
) -> FlarkV3HostStructuralOrdinalWindowQueryReceipt {
    FlarkV3HostStructuralOrdinalWindowQueryReceipt {
        storage_pages_visited: receipt.storage_pages_visited,
        tree_nodes_visited: receipt.tree_nodes_visited,
        packed_entries_inspected: receipt.packed_entries_inspected,
        summary_nodes_skipped: receipt.summary_nodes_skipped,
        ..FlarkV3HostStructuralOrdinalWindowQueryReceipt::default()
    }
}

const fn structural_ordinal_window_failure_reason(
    reason: HostStructuralOrdinalWindowFailureReason,
) -> u32 {
    match reason {
        HostStructuralOrdinalWindowFailureReason::UnavailableFacts => 1,
        HostStructuralOrdinalWindowFailureReason::EntryWindowLimit => 2,
        HostStructuralOrdinalWindowFailureReason::StoragePageLimit => 3,
        HostStructuralOrdinalWindowFailureReason::TreeNodeLimit => 4,
        HostStructuralOrdinalWindowFailureReason::PackedEntryLimit => 5,
        HostStructuralOrdinalWindowFailureReason::OrdinalOutOfRange => 6,
        HostStructuralOrdinalWindowFailureReason::UndecodableClosure => 7,
    }
}

fn block_range_query_receipt(outcome: HostBlockRangeOutcome) -> FlarkV3HostBlockRangeQueryReceipt {
    match outcome {
        HostBlockRangeOutcome::Page {
            source_version,
            covered_range,
            continuation,
            receipt,
            ..
        } => block_range_query_receipt_fields(
            1,
            0,
            source_version,
            covered_range,
            continuation,
            receipt,
        ),
        HostBlockRangeOutcome::SourceGap {
            source_version,
            requested_range,
            reason,
            receipt,
        } => block_range_query_receipt_fields(
            2,
            gap_reason(reason),
            source_version,
            requested_range,
            None,
            receipt,
        ),
    }
}

fn block_range_query_receipt_fields(
    outcome: u32,
    gap_reason: u32,
    source_version: SourceVersion,
    coverage: HostMetricRange,
    continuation: Option<HostBlockRangeContinuation>,
    receipt: HostBlockRangeReceipt,
) -> FlarkV3HostBlockRangeQueryReceipt {
    let (continuation_length, continuation) = match continuation {
        Some(continuation) => (
            FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES as u32,
            continuation.encoded(),
        ),
        None => (0, [0; FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES]),
    };
    FlarkV3HostBlockRangeQueryReceipt {
        outcome,
        gap_reason,
        encoded_bytes: receipt.encoded_bytes,
        block_count: receipt.block_count,
        storage_pages_visited: receipt.storage_pages_visited,
        open_depth: receipt.open_depth,
        tree_nodes_visited: receipt.tree_nodes_visited,
        packed_entries_inspected: receipt.packed_entries_inspected,
        summary_nodes_skipped: receipt.summary_nodes_skipped,
        flags: u32::from(receipt.complete),
        coverage_start_utf8: coverage.start.bytes,
        coverage_start_utf16: coverage.start.utf16,
        coverage_end_utf8: coverage.end.bytes,
        coverage_end_utf16: coverage.end.utf16,
        source_version: source_version.into(),
        continuation_length,
        continuation,
        ..FlarkV3HostBlockRangeQueryReceipt::default()
    }
}

fn point_query_receipt_fields(
    outcome: u32,
    gap_reason: u32,
    source_version: SourceVersion,
    range: HostMetricRange,
    receipt: HostViewportReceipt,
) -> FlarkV3HostPointQueryReceipt {
    FlarkV3HostPointQueryReceipt {
        outcome,
        gap_reason,
        encoded_bytes: receipt.encoded_bytes,
        leaf_count: receipt.leaf_count,
        open_depth: receipt.open_depth,
        tree_nodes_visited: receipt.tree_nodes_visited,
        summary_nodes_skipped: receipt.summary_nodes_skipped,
        range_start_utf8: range.start.bytes,
        range_start_utf16: range.start.utf16,
        range_end_utf8: range.end.bytes,
        range_end_utf16: range.end.utf16,
        source_version: source_version.into(),
        ..FlarkV3HostPointQueryReceipt::default()
    }
}

const fn gap_reason(reason: HostSourceGapReason) -> u32 {
    match reason {
        HostSourceGapReason::OpenDepthLimit => 1,
        HostSourceGapReason::EncodedByteLimit => 2,
        HostSourceGapReason::LeafLimit => 3,
        HostSourceGapReason::TreeNodeLimit => 4,
        HostSourceGapReason::UndecodableClosure => 5,
        HostSourceGapReason::UnavailableFacts => 6,
    }
}

fn poll_receipt(outcome: HostPollOutcome) -> FlarkV3HostPollReceipt {
    match outcome {
        HostPollOutcome::Pending => FlarkV3HostPollReceipt::default(),
        HostPollOutcome::PacketCredit {
            offer_id,
            next_frame_ordinal,
        } => FlarkV3HostPollReceipt {
            outcome: 1,
            offer_id,
            next_frame_ordinal,
            ..FlarkV3HostPollReceipt::default()
        },
        HostPollOutcome::Committed(ack) => FlarkV3HostPollReceipt {
            outcome: 2,
            offer_id: ack.publication_session,
            ack: ack.into(),
            ..FlarkV3HostPollReceipt::default()
        },
        HostPollOutcome::AbortComplete { offer_id } => FlarkV3HostPollReceipt {
            outcome: 3,
            offer_id,
            ..FlarkV3HostPollReceipt::default()
        },
        HostPollOutcome::Closed => FlarkV3HostPollReceipt {
            outcome: 4,
            ..FlarkV3HostPollReceipt::default()
        },
    }
}

fn inline_sidecar_poll_receipt(
    outcome: InlineSidecarHostPollOutcome,
) -> FlarkV3HostInlineSidecarPollReceipt {
    match outcome {
        InlineSidecarHostPollOutcome::Pending => FlarkV3HostInlineSidecarPollReceipt::default(),
        InlineSidecarHostPollOutcome::PacketCredit {
            offer_id,
            next_frame_ordinal,
        } => FlarkV3HostInlineSidecarPollReceipt {
            outcome: 1,
            offer_id,
            next_frame_ordinal,
            ..FlarkV3HostInlineSidecarPollReceipt::default()
        },
        InlineSidecarHostPollOutcome::Committed(ack) => FlarkV3HostInlineSidecarPollReceipt {
            outcome: 2,
            offer_id: ack.publication_session,
            ack: ffi_inline_sidecar_ack(ack),
            ..FlarkV3HostInlineSidecarPollReceipt::default()
        },
        InlineSidecarHostPollOutcome::AbortComplete { offer_id } => {
            FlarkV3HostInlineSidecarPollReceipt {
                outcome: 3,
                offer_id,
                ..FlarkV3HostInlineSidecarPollReceipt::default()
            }
        }
        InlineSidecarHostPollOutcome::Closed => FlarkV3HostInlineSidecarPollReceipt {
            outcome: 4,
            ..FlarkV3HostInlineSidecarPollReceipt::default()
        },
    }
}

fn viewport_presentation_poll_receipt(
    outcome: HostViewportPresentationPollOutcome,
) -> FlarkV3HostViewportPresentationPollReceipt {
    match outcome {
        HostViewportPresentationPollOutcome::Pending => {
            FlarkV3HostViewportPresentationPollReceipt::default()
        }
        HostViewportPresentationPollOutcome::PacketCredit {
            offer_id,
            next_frame_ordinal,
        } => FlarkV3HostViewportPresentationPollReceipt {
            outcome: 1,
            offer_id,
            next_frame_ordinal,
            ..FlarkV3HostViewportPresentationPollReceipt::default()
        },
        HostViewportPresentationPollOutcome::Committed(ack) => {
            FlarkV3HostViewportPresentationPollReceipt {
                outcome: 2,
                offer_id: ack.publication_session,
                ack: ffi_viewport_presentation_ack(ack),
                ..FlarkV3HostViewportPresentationPollReceipt::default()
            }
        }
        HostViewportPresentationPollOutcome::AbortComplete { offer_id } => {
            FlarkV3HostViewportPresentationPollReceipt {
                outcome: 3,
                offer_id,
                ..FlarkV3HostViewportPresentationPollReceipt::default()
            }
        }
        HostViewportPresentationPollOutcome::Closed => FlarkV3HostViewportPresentationPollReceipt {
            outcome: 4,
            ..FlarkV3HostViewportPresentationPollReceipt::default()
        },
    }
}

fn inline_sidecar_query_receipt(
    outcome: HostInlineSidecarQueryOutcome,
) -> FlarkV3HostInlineSidecarQueryReceipt {
    match outcome {
        HostInlineSidecarQueryOutcome::Authoritative {
            fact_count,
            value_entry_count,
            value_encoded_bytes,
            encoded_bytes,
            tree_nodes_visited,
        } => FlarkV3HostInlineSidecarQueryReceipt {
            outcome: 1,
            encoded_bytes,
            fact_count,
            tree_nodes_visited,
            value_entry_count,
            value_encoded_bytes,
            ..FlarkV3HostInlineSidecarQueryReceipt::default()
        },
        HostInlineSidecarQueryOutcome::Unsupported {
            reason,
            metadata_bytes,
        } => FlarkV3HostInlineSidecarQueryReceipt {
            outcome: 2,
            reason,
            encoded_bytes: metadata_bytes,
            ..FlarkV3HostInlineSidecarQueryReceipt::default()
        },
        HostInlineSidecarQueryOutcome::Unavailable => {
            FlarkV3HostInlineSidecarQueryReceipt::default()
        }
    }
}

fn viewport_presentation_query_receipt(
    outcome: HostViewportPresentationQueryOutcome,
) -> FlarkV3HostViewportPresentationQueryReceipt {
    match outcome {
        HostViewportPresentationQueryOutcome::Available {
            encoded_bytes,
            entry_count,
        } => FlarkV3HostViewportPresentationQueryReceipt {
            outcome: 1,
            encoded_bytes,
            entry_count,
            ..FlarkV3HostViewportPresentationQueryReceipt::default()
        },
        HostViewportPresentationQueryOutcome::Unavailable => {
            FlarkV3HostViewportPresentationQueryReceipt::default()
        }
    }
}

fn ffi_host_call(
    out_receipt: *mut FlarkV3HostCallReceipt,
    operation: impl FnOnce() -> NativeResult,
) -> u32 {
    ffi_guard(|| {
        unsafe { write_output(out_receipt, FlarkV3HostCallReceipt::default())? };
        match operation() {
            Ok(()) => Ok(()),
            Err(NativeHostError::Host(error)) => unsafe { reject_call(out_receipt, error) },
            Err(error) => Err(error),
        }
    })
}

unsafe fn reject_call(
    out_receipt: *mut FlarkV3HostCallReceipt,
    error: HostStoreError,
) -> NativeResult {
    unsafe {
        write_output(
            out_receipt,
            FlarkV3HostCallReceipt {
                rejection_reason: reject_reason(error.reason()),
            },
        )?
    };
    Err(NativeHostError::Host(error))
}

const fn reject_reason(reason: HostRejectReason) -> u32 {
    match reason {
        HostRejectReason::Invalid => 1,
        HostRejectReason::Backpressure => 2,
        HostRejectReason::StaleSource => 3,
        HostRejectReason::ExactSourceMismatch => 4,
        HostRejectReason::BaseMismatch => 6,
        HostRejectReason::WrongOffer => 7,
        HostRejectReason::CorruptPublication => 8,
        HostRejectReason::QueryBoundExceeded => 9,
        HostRejectReason::ForegroundBoundExceeded => 10,
        HostRejectReason::Superseded => 11,
        HostRejectReason::Closed => 12,
        // Resource exhaustion and implementation faults are not expected-race
        // rejection values. Their non-OK native status is authoritative.
        HostRejectReason::NotReady
        | HostRejectReason::AllocationFailed
        | HostRejectReason::InternalFault => 0,
    }
}

fn ffi_guard(operation: impl FnOnce() -> NativeResult) -> u32 {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(())) => u32::from(Status::Ok.code()),
        Ok(Err(error)) => u32::from(native_status(&error).code()),
        Err(_) => u32::from(Status::InternalFault.code()),
    }
}

fn native_status(error: &NativeHostError) -> Status {
    match error {
        NativeHostError::InvalidArgument | NativeHostError::InvalidConfig => Status::Invalid,
        NativeHostError::BoundExceeded => Status::ForegroundBoundExceeded,
        NativeHostError::Registry(error) => match error {
            HostRegistryError::InvalidLimit
            | HostRegistryError::InvalidConfig
            | HostRegistryError::InvalidPacketEnvelope
            | HostRegistryError::InvalidHandle => Status::Invalid,
            HostRegistryError::StaleHandle => Status::Closed,
            HostRegistryError::CapacityExceeded | HostRegistryError::AllocationFailed => {
                Status::AllocationFailed
            }
            HostRegistryError::Poisoned => Status::InternalFault,
            HostRegistryError::InUse => Status::Backpressure,
            HostRegistryError::NotRemovable => Status::InvalidState,
        },
        NativeHostError::Host(error) => match error.reason() {
            HostRejectReason::Invalid => Status::Invalid,
            HostRejectReason::Backpressure => Status::Backpressure,
            HostRejectReason::StaleSource
            | HostRejectReason::ExactSourceMismatch
            | HostRejectReason::BaseMismatch
            | HostRejectReason::WrongOffer
            | HostRejectReason::CorruptPublication
            | HostRejectReason::Superseded => Status::InvalidState,
            HostRejectReason::QueryBoundExceeded | HostRejectReason::ForegroundBoundExceeded => {
                Status::ForegroundBoundExceeded
            }
            HostRejectReason::Closed => Status::Closed,
            HostRejectReason::NotReady => Status::NotReady,
            HostRejectReason::AllocationFailed => Status::AllocationFailed,
            HostRejectReason::InternalFault => Status::InternalFault,
        },
    }
}

unsafe fn read_input<T: Copy>(pointer: *const T) -> NativeResult<T> {
    if pointer.is_null() || !(pointer as usize).is_multiple_of(align_of::<T>()) {
        return Err(NativeHostError::InvalidArgument);
    }
    Ok(unsafe { pointer.read() })
}

unsafe fn write_output<T>(pointer: *mut T, value: T) -> NativeResult {
    if pointer.is_null() || !(pointer as usize).is_multiple_of(align_of::<T>()) {
        return Err(NativeHostError::InvalidArgument);
    }
    unsafe { pointer.write(value) };
    Ok(())
}

unsafe fn read_buffer<'a>(pointer: *const u8, length: u32, maximum: u32) -> NativeResult<&'a [u8]> {
    if length > maximum {
        return Err(NativeHostError::BoundExceeded);
    }
    if length == 0 {
        return Ok(&[]);
    }
    if pointer.is_null() {
        return Err(NativeHostError::InvalidArgument);
    }
    Ok(unsafe { std::slice::from_raw_parts(pointer, length as usize) })
}

unsafe fn write_buffer<'a>(
    pointer: *mut u8,
    length: u32,
    maximum: u32,
) -> NativeResult<&'a mut [u8]> {
    if length > maximum {
        return Err(NativeHostError::BoundExceeded);
    }
    if length == 0 {
        return Ok(&mut []);
    }
    if pointer.is_null() {
        return Err(NativeHostError::InvalidArgument);
    }
    Ok(unsafe { std::slice::from_raw_parts_mut(pointer, length as usize) })
}

// Compile-time C signature guards. Any drift requires an intentional ABI and
// header revision.
const _: extern "C" fn() -> u32 = flark_v3_host_native_abi_version;
const _: unsafe extern "C" fn(*mut FlarkV3HostConfig) -> u32 = flark_v3_host_config_standard;
const _: unsafe extern "C" fn(*const FlarkV3HostConfig, *mut FlarkV3HostHandle) -> u32 =
    flark_v3_host_create;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostSourceVersion,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_observe_source;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostOfferBegin,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_begin_offer;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostInlineSidecarBegin,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_begin_inline_sidecar_offer;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostViewportPresentationBegin,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_begin_viewport_presentation_offer;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostOfferBegin,
    *const FlarkV3HostStructuralAck,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_begin_references_delta;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostOfferBegin,
    *const FlarkV3HostStructuralAck,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_begin_exact_base_delta;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const u8,
    u32,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_admit_packet;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const u8,
    u32,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_admit_inline_sidecar_packet;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const u8,
    u32,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_admit_viewport_presentation_packet;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostCommitRequest,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_request_commit;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostInlineSidecarCommitRequest,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_request_inline_sidecar_commit;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostViewportPresentationCommitRequest,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_request_viewport_presentation_commit;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostId128,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_abort_offer;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostId128,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_abort_inline_sidecar_offer;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostId128,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_abort_viewport_presentation_offer;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    FlarkV3HostWorkGrant,
    *mut FlarkV3HostPollReceipt,
) -> u32 = flark_v3_host_poll;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    FlarkV3HostWorkGrant,
    *mut FlarkV3HostInlineSidecarPollReceipt,
) -> u32 = flark_v3_host_poll_inline_sidecar;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    FlarkV3HostWorkGrant,
    *mut FlarkV3HostViewportPresentationPollReceipt,
) -> u32 = flark_v3_host_poll_viewport_presentation;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostStructuralAck,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_acknowledge_delivery;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostInlineSidecarAck,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_acknowledge_inline_sidecar_delivery;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostViewportPresentationAck,
    *mut FlarkV3HostCallReceipt,
) -> u32 = flark_v3_host_acknowledge_viewport_presentation_delivery;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostPointQuery,
    *mut u8,
    u32,
    *mut FlarkV3HostPointQueryReceipt,
) -> u32 = flark_v3_host_query_structural;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostBlockRangeQuery,
    *mut u8,
    u32,
    *mut FlarkV3HostBlockRangeQueryReceipt,
) -> u32 = flark_v3_host_query_structural_range;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostStructuralOrdinalWindowQuery,
    *mut FlarkV3HostStructuralOrdinalWindowQueryReceipt,
) -> u32 = flark_v3_host_query_structural_ordinal_window;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostInlineSidecarQuery,
    *mut u8,
    u32,
    *mut FlarkV3HostInlineSidecarQueryReceipt,
) -> u32 = flark_v3_host_query_inline_sidecar;
const _: unsafe extern "C" fn(
    FlarkV3HostHandle,
    *const FlarkV3HostViewportPresentationQuery,
    *mut u8,
    u32,
    *mut FlarkV3HostViewportPresentationQueryReceipt,
) -> u32 = flark_v3_host_query_viewport_presentation;
const _: unsafe extern "C" fn(FlarkV3HostHandle, *mut FlarkV3HostCallReceipt) -> u32 =
    flark_v3_host_close;
const _: extern "C" fn(FlarkV3HostHandle) -> u32 = flark_v3_host_remove;
const _: extern "C" fn(FlarkV3HostHandle) -> u32 = flark_v3_host_emergency_destroy;
const _: unsafe extern "C" fn(FlarkV3HostHandle, *mut *mut c_void) -> u32 =
    flark_v3_host_finalizer_token_create;
const _: unsafe extern "C" fn(*mut c_void) -> u32 = flark_v3_host_finalizer_token_release;
const _: unsafe extern "C" fn(*mut c_void) = flark_v3_host_emergency_finalize;

const _: () = {
    assert!(M11_HOST_MAXIMUM_FRAME_BYTES <= u32::MAX as usize);
    assert!(FLARK_V3_HOST_MAXIMUM_FRAME_BYTES == 5_140);
    assert!(FLARK_V3_HOST_MAXIMUM_PACKET_BYTES == 71_724);
    assert!(FLARK_V3_HOST_M11_GREEN_RECORD_BYTES == 80);
    assert!(FLARK_V3_HOST_M11_PROJECTION_RECORD_BYTES == 56);
    assert!(FLARK_V3_HOST_M11_VIEWPORT_BYTES == 156);
    assert!(FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES == 64);
    assert!(FLARK_V3_HOST_BLOCK_RANGE_HEADER_BYTES == 32);
    assert!(FLARK_V3_HOST_BLOCK_RANGE_RECORD_BYTES == 160);
    assert!(FLARK_V3_HOST_STRUCTURAL_ORDINAL_WINDOW_MAXIMUM_ENTRIES == 4096);
    assert!(FLARK_V3_HOST_VIEWPORT_PRESENTATION_PAGE_SCHEMA == 10);
    assert!(FLARK_V3_HOST_VIEWPORT_PRESENTATION_PAGE_HEADER_BYTES == 160);
    assert!(FLARK_V3_HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES == 152);
    assert!(FLARK_V3_HOST_VIEWPORT_PRESENTATION_MAXIMUM_FRAME_BYTES == 65_536);
    assert!(size_of::<FlarkV3HostHandle>() == 8);
    assert!(align_of::<FlarkV3HostHandle>() == 4);
    assert!(offset_of!(FlarkV3HostHandle, generation) == 4);
    assert!(size_of::<FlarkV3HostConfig>() == 56);
    assert!(offset_of!(FlarkV3HostConfig, reserved) == 40);
    assert!(size_of::<FlarkV3HostSourceVersion>() == 44);
    assert!(size_of::<FlarkV3HostOfferBegin>() == 144);
    assert!(size_of::<FlarkV3HostCommitRequest>() == 56);
    assert!(size_of::<FlarkV3HostStructuralAck>() == 124);
    assert!(size_of::<FlarkV3HostU64>() == 8);
    assert!(size_of::<FlarkV3HostInlineSidecarBinding>() == 56);
    assert!(size_of::<FlarkV3HostInlineSidecarDisposition>() == 80);
    assert!(size_of::<FlarkV3HostInlineSidecarBegin>() == 364);
    assert!(offset_of!(FlarkV3HostInlineSidecarBegin, base_ack) == 40);
    assert!(offset_of!(FlarkV3HostInlineSidecarBegin, binding) == 164);
    assert!(size_of::<FlarkV3HostInlineSidecarCommitRequest>() == 56);
    assert!(size_of::<FlarkV3HostInlineSidecarAck>() == 212);
    assert!(size_of::<FlarkV3HostInlineSidecarPollReceipt>() == 240);
    assert!(size_of::<FlarkV3HostInlineSidecarQuery>() == 80);
    assert!(size_of::<FlarkV3HostInlineSidecarQueryReceipt>() == 32);
    assert!(size_of::<FlarkV3HostViewportPresentationMetricRange>() == 16);
    assert!(size_of::<FlarkV3HostViewportPresentationVisitStart>() == 16);
    assert!(size_of::<FlarkV3HostViewportPresentationBinding>() == 72);
    assert!(offset_of!(FlarkV3HostViewportPresentationBinding, complete) == 68);
    assert!(size_of::<FlarkV3HostViewportPresentationEnvelope>() == 60);
    assert!(
        offset_of!(
            FlarkV3HostViewportPresentationEnvelope,
            aggregate_envelope_digest256
        ) == 28
    );
    assert!(size_of::<FlarkV3HostViewportPresentationQueryLimits>() == 32);
    assert!(size_of::<FlarkV3HostViewportPresentationOfferLimits>() == 20);
    assert!(size_of::<FlarkV3HostViewportPresentationBegin>() == 348);
    assert!(offset_of!(FlarkV3HostViewportPresentationBegin, base_ack) == 40);
    assert!(offset_of!(FlarkV3HostViewportPresentationBegin, binding) == 164);
    assert!(offset_of!(FlarkV3HostViewportPresentationBegin, envelope) == 236);
    assert!(offset_of!(FlarkV3HostViewportPresentationBegin, query_limits) == 296);
    assert!(offset_of!(FlarkV3HostViewportPresentationBegin, limits) == 328);
    assert!(size_of::<FlarkV3HostViewportPresentationCommitRequest>() == 56);
    assert!(size_of::<FlarkV3HostViewportPresentationAck>() == 296);
    assert!(offset_of!(FlarkV3HostViewportPresentationAck, base_ack) == 16);
    assert!(offset_of!(FlarkV3HostViewportPresentationAck, binding) == 140);
    assert!(offset_of!(FlarkV3HostViewportPresentationAck, envelope) == 212);
    assert!(
        offset_of!(
            FlarkV3HostViewportPresentationAck,
            aggregate_root_stream_digest
        ) == 280
    );
    assert!(size_of::<FlarkV3HostViewportPresentationPollReceipt>() == 324);
    assert!(offset_of!(FlarkV3HostViewportPresentationPollReceipt, ack) == 28);
    assert!(size_of::<FlarkV3HostViewportPresentationQuery>() == 320);
    assert!(offset_of!(FlarkV3HostViewportPresentationQuery, ack) == 8);
    assert!(offset_of!(FlarkV3HostViewportPresentationQuery, maximum_encoded_bytes) == 304);
    assert!(offset_of!(FlarkV3HostViewportPresentationQuery, reserved) == 308);
    assert!(size_of::<FlarkV3HostViewportPresentationQueryReceipt>() == 32);
    assert!(offset_of!(FlarkV3HostViewportPresentationQueryReceipt, reserved) == 16);
    assert!(FLARK_V3_HOST_VIEWPORT_PRESENTATION_BEGIN_BYTES == 348);
    assert!(FLARK_V3_HOST_VIEWPORT_PRESENTATION_COMMIT_REQUEST_BYTES == 56);
    assert!(FLARK_V3_HOST_VIEWPORT_PRESENTATION_ACK_BYTES == 296);
    assert!(FLARK_V3_HOST_VIEWPORT_PRESENTATION_POLL_RECEIPT_BYTES == 324);
    assert!(FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_BYTES == 320);
    assert!(FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_RECEIPT_BYTES == 32);
    assert!(size_of::<FlarkV3HostPollReceipt>() == 152);
    assert!(size_of::<FlarkV3HostPointQuery>() == 96);
    assert!(offset_of!(FlarkV3HostPointQuery, source_version) == 8);
    assert!(offset_of!(FlarkV3HostPointQuery, position_utf8) == 52);
    assert!(offset_of!(FlarkV3HostPointQuery, affinity) == 60);
    assert!(offset_of!(FlarkV3HostPointQuery, maximum_encoded_bytes) == 64);
    assert!(offset_of!(FlarkV3HostPointQuery, reserved) == 80);
    assert!(size_of::<FlarkV3HostPointQueryReceipt>() == 112);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, outcome) == 4);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, encoded_bytes) == 12);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, range_start_utf8) == 32);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, source_version) == 48);
    assert!(offset_of!(FlarkV3HostPointQueryReceipt, reserved) == 92);
    assert!(size_of::<FlarkV3HostBlockRangeQuery>() == 172);
    assert!(offset_of!(FlarkV3HostBlockRangeQuery, source_version) == 8);
    assert!(offset_of!(FlarkV3HostBlockRangeQuery, requested_start_utf8) == 52);
    assert!(offset_of!(FlarkV3HostBlockRangeQuery, maximum_encoded_bytes) == 68);
    assert!(offset_of!(FlarkV3HostBlockRangeQuery, continuation_length) == 88);
    assert!(offset_of!(FlarkV3HostBlockRangeQuery, continuation) == 92);
    assert!(offset_of!(FlarkV3HostBlockRangeQuery, reserved) == 156);
    assert!(size_of::<FlarkV3HostBlockRangeQueryReceipt>() == 188);
    assert!(offset_of!(FlarkV3HostBlockRangeQueryReceipt, outcome) == 4);
    assert!(offset_of!(FlarkV3HostBlockRangeQueryReceipt, encoded_bytes) == 12);
    assert!(offset_of!(FlarkV3HostBlockRangeQueryReceipt, flags) == 40);
    assert!(offset_of!(FlarkV3HostBlockRangeQueryReceipt, coverage_start_utf8) == 44);
    assert!(offset_of!(FlarkV3HostBlockRangeQueryReceipt, source_version) == 60);
    assert!(offset_of!(FlarkV3HostBlockRangeQueryReceipt, continuation_length) == 104);
    assert!(offset_of!(FlarkV3HostBlockRangeQueryReceipt, continuation) == 108);
    assert!(offset_of!(FlarkV3HostBlockRangeQueryReceipt, reserved) == 172);
    assert!(size_of::<FlarkV3HostStructuralOrdinalWindowQuery>() == 92);
    assert!(offset_of!(FlarkV3HostStructuralOrdinalWindowQuery, source_version) == 8);
    assert!(offset_of!(FlarkV3HostStructuralOrdinalWindowQuery, start_entry_ordinal) == 52);
    assert!(offset_of!(FlarkV3HostStructuralOrdinalWindowQuery, maximum_entries) == 60);
    assert!(offset_of!(FlarkV3HostStructuralOrdinalWindowQuery, reserved) == 76);
    assert!(size_of::<FlarkV3HostStructuralOrdinalWindowQueryReceipt>() == 132);
    assert!(
        offset_of!(
            FlarkV3HostStructuralOrdinalWindowQueryReceipt,
            total_entry_count
        ) == 16
    );
    assert!(
        offset_of!(
            FlarkV3HostStructuralOrdinalWindowQueryReceipt,
            start_entry_ordinal
        ) == 24
    );
    assert!(
        offset_of!(
            FlarkV3HostStructuralOrdinalWindowQueryReceipt,
            next_entry_ordinal
        ) == 32
    );
    assert!(offset_of!(FlarkV3HostStructuralOrdinalWindowQueryReceipt, start_utf8) == 40);
    assert!(
        offset_of!(
            FlarkV3HostStructuralOrdinalWindowQueryReceipt,
            storage_pages_visited
        ) == 56
    );
    assert!(
        offset_of!(
            FlarkV3HostStructuralOrdinalWindowQueryReceipt,
            source_version
        ) == 72
    );
    assert!(offset_of!(FlarkV3HostStructuralOrdinalWindowQueryReceipt, reserved) == 116);
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v3_host_registry::MAXIMUM_RESIDENT_HOSTS;
    use crate::v3_host_store::source_root_u64;
    use crate::v3_publication_wire::{
        encode_publication_packet_into, PublicationPacketFrameInput, PublicationPacketInput,
    };

    fn close_and_remove(handle: FlarkV3HostHandle) {
        let mut call = FlarkV3HostCallReceipt::default();
        assert_eq!(unsafe { flark_v3_host_close(handle, &mut call) }, 0);
        loop {
            let mut poll = FlarkV3HostPollReceipt::default();
            assert_eq!(
                unsafe {
                    flark_v3_host_poll(
                        handle,
                        FlarkV3HostWorkGrant {
                            inspect_bytes: 0,
                            copy_bytes: 0,
                            transitions: 256,
                        },
                        &mut poll,
                    )
                },
                0
            );
            if poll.outcome == 4 {
                break;
            }
        }
        assert_eq!(flark_v3_host_remove(handle), 0);
    }

    #[test]
    fn source_root_uses_dart_high_low_word_order() {
        assert_eq!(
            source_root_u64([0x0123_4567, 0x89ab_cdef]),
            0x0123_4567_89ab_cdef
        );
    }

    #[test]
    fn ffi_create_close_poll_remove_is_generation_checked() {
        let mut config = FlarkV3HostConfig::default();
        assert_eq!(unsafe { flark_v3_host_config_standard(&mut config) }, 0);
        config.document_session = [91, 92, 93, 94];
        let mut handle = FlarkV3HostHandle::default();
        assert_eq!(unsafe { flark_v3_host_create(&config, &mut handle) }, 0);
        assert_ne!(handle.slot, 0);

        let mut call = FlarkV3HostCallReceipt::default();
        assert_eq!(unsafe { flark_v3_host_close(handle, &mut call) }, 0);
        let mut poll = FlarkV3HostPollReceipt::default();
        assert_eq!(
            unsafe {
                flark_v3_host_poll(
                    handle,
                    FlarkV3HostWorkGrant {
                        inspect_bytes: 0,
                        copy_bytes: 0,
                        transitions: 1,
                    },
                    &mut poll,
                )
            },
            0
        );
        assert_eq!(poll.outcome, 4);
        assert_eq!(flark_v3_host_remove(handle), 0);
        assert_eq!(
            flark_v3_host_remove(handle),
            u32::from(Status::Closed.code())
        );
    }

    #[test]
    fn ffi_layout_matches_declared_header_contract() {
        assert_eq!(FLARK_V3_HOST_NATIVE_ABI_VERSION, 0x0003_0005);
        assert_eq!(size_of::<FlarkV3HostConfig>(), 56);
        assert_eq!(size_of::<FlarkV3HostOfferBegin>(), 144);
        assert_eq!(size_of::<FlarkV3HostStructuralAck>(), 124);
        assert_eq!(size_of::<FlarkV3HostInlineSidecarBinding>(), 56);
        assert_eq!(size_of::<FlarkV3HostInlineSidecarBegin>(), 364);
        assert_eq!(size_of::<FlarkV3HostInlineSidecarCommitRequest>(), 56);
        assert_eq!(size_of::<FlarkV3HostInlineSidecarAck>(), 212);
        assert_eq!(size_of::<FlarkV3HostInlineSidecarPollReceipt>(), 240);
        assert_eq!(size_of::<FlarkV3HostInlineSidecarQuery>(), 80);
        assert_eq!(size_of::<FlarkV3HostInlineSidecarQueryReceipt>(), 32);
        assert_eq!(
            size_of::<FlarkV3HostViewportPresentationBegin>(),
            FLARK_V3_HOST_VIEWPORT_PRESENTATION_BEGIN_BYTES as usize
        );
        assert_eq!(
            size_of::<FlarkV3HostViewportPresentationCommitRequest>(),
            FLARK_V3_HOST_VIEWPORT_PRESENTATION_COMMIT_REQUEST_BYTES as usize
        );
        assert_eq!(
            size_of::<FlarkV3HostViewportPresentationAck>(),
            FLARK_V3_HOST_VIEWPORT_PRESENTATION_ACK_BYTES as usize
        );
        assert_eq!(
            size_of::<FlarkV3HostViewportPresentationPollReceipt>(),
            FLARK_V3_HOST_VIEWPORT_PRESENTATION_POLL_RECEIPT_BYTES as usize
        );
        assert_eq!(
            size_of::<FlarkV3HostViewportPresentationQuery>(),
            FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_BYTES as usize
        );
        assert_eq!(
            size_of::<FlarkV3HostViewportPresentationQueryReceipt>(),
            FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_RECEIPT_BYTES as usize
        );
        assert_eq!(size_of::<FlarkV3HostPollReceipt>(), 152);
        assert_eq!(size_of::<FlarkV3HostPointQuery>(), 96);
        assert_eq!(size_of::<FlarkV3HostPointQueryReceipt>(), 112);
        assert_eq!(size_of::<FlarkV3HostStructuralOrdinalWindowQuery>(), 92);
        assert_eq!(
            size_of::<FlarkV3HostStructuralOrdinalWindowQueryReceipt>(),
            132
        );
        assert_eq!(MAXIMUM_RESIDENT_HOSTS, 2_048);
        assert_eq!(FLARK_V3_HOST_MAXIMUM_PACKET_BYTES, 71_724);
        let header = include_str!("../flark_comrak_bridge.h");
        assert!(header.contains("FLARK_V3_HOST_NATIVE_ABI_VERSION UINT32_C(0x00030005)"));
        assert!(header.contains("flark_v3_host_begin_references_delta"));
        assert!(header.contains("flark_v3_host_begin_exact_base_delta"));
        assert!(header.contains("flark_v3_host_begin_inline_sidecar_offer"));
        assert!(header.contains("flark_v3_host_admit_inline_sidecar_packet"));
        assert!(header.contains("flark_v3_host_poll_inline_sidecar"));
        assert!(header.contains("flark_v3_host_query_inline_sidecar"));
        assert!(header.contains("flark_v3_host_begin_viewport_presentation_offer"));
        assert!(header.contains("flark_v3_host_admit_viewport_presentation_packet"));
        assert!(header.contains("flark_v3_host_request_viewport_presentation_commit"));
        assert!(header.contains("flark_v3_host_abort_viewport_presentation_offer"));
        assert!(header.contains("flark_v3_host_poll_viewport_presentation"));
        assert!(header.contains("flark_v3_host_acknowledge_viewport_presentation_delivery"));
        assert!(header.contains("flark_v3_host_query_viewport_presentation"));
        assert!(
            header.contains("FLARK_V3_HOST_VIEWPORT_PRESENTATION_POLL_RECEIPT_BYTES UINT32_C(324)")
        );
        assert!(header.contains("FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_BYTES UINT32_C(320)"));
        assert!(
            header.contains("FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_RECEIPT_BYTES UINT32_C(32)")
        );
        assert!(header.contains("flark_v3_host_query_structural_range"));
        assert!(header.contains("FlarkV3HostBlockRangeQuery"));
        assert!(header.contains("FLARK_V3_HOST_BLOCK_RANGE_RECORD_BYTES UINT32_C(160)"));
        assert!(header.contains("flark_v3_host_query_structural_ordinal_window"));
        assert!(header.contains("FlarkV3HostStructuralOrdinalWindowQueryReceipt"));
        assert!(header
            .contains("FLARK_V3_HOST_STRUCTURAL_ORDINAL_WINDOW_MAXIMUM_ENTRIES UINT32_C(4096)"));
        assert!(header.contains("FLARK_V3_HOST_MAXIMUM_PACKET_BYTES UINT32_C(71724)"));
        assert!(header.contains("flark_v3_host_admit_packet"));
        assert!(!header.contains("flark_v3_host_admit_chunk"));
        assert!(!header.contains("FlarkV3HostChunk"));
    }

    #[test]
    fn ordinal_window_native_receipts_preserve_exact_success_and_canonical_failure() {
        let source = SourceVersion {
            document_session: [1601, 1602, 1603, 1604],
            revision: 7,
            utf8_length: 8191,
            utf16_length: 8191,
            content_hash128: [1611, 1612, 1613, 1614],
        };
        let work = HostStructuralOrdinalWindowReceipt {
            storage_pages_visited: 2,
            tree_nodes_visited: 43,
            packed_entries_inspected: 386,
            summary_nodes_skipped: 11,
        };
        let success =
            structural_ordinal_window_query_receipt(HostStructuralOrdinalWindowOutcome::Window {
                source_version: source,
                total_entry_count: 8191,
                start_entry_ordinal: 4095,
                next_entry_ordinal: 4192,
                start: HostSourceMetric {
                    bytes: 4095,
                    utf16: 4095,
                },
                next: HostSourceMetric {
                    bytes: 4192,
                    utf16: 4192,
                },
                complete: false,
                receipt: work,
            });
        assert_eq!(success.rejection_reason, 0);
        assert_eq!(success.outcome, 1);
        assert_eq!(success.failure_reason, 0);
        assert_eq!(success.flags, 0);
        assert_eq!(u64::from(success.total_entry_count), 8191);
        assert_eq!(u64::from(success.start_entry_ordinal), 4095);
        assert_eq!(u64::from(success.next_entry_ordinal), 4192);
        assert_eq!((success.start_utf8, success.start_utf16), (4095, 4095));
        assert_eq!((success.next_utf8, success.next_utf16), (4192, 4192));
        assert_eq!(success.storage_pages_visited, 2);
        assert_eq!(success.tree_nodes_visited, 43);
        assert_eq!(success.packed_entries_inspected, 386);
        assert_eq!(success.summary_nodes_skipped, 11);
        assert_eq!(success.source_version, source.into());
        assert_eq!(success.reserved, [0; 4]);

        let failure =
            structural_ordinal_window_query_receipt(HostStructuralOrdinalWindowOutcome::Failure {
                source_version: source,
                total_entry_count: 8191,
                start_entry_ordinal: 8192,
                reason: HostStructuralOrdinalWindowFailureReason::OrdinalOutOfRange,
                receipt: HostStructuralOrdinalWindowReceipt::default(),
            });
        assert_eq!(failure.rejection_reason, 0);
        assert_eq!(failure.outcome, 2);
        assert_eq!(failure.failure_reason, 6);
        assert_eq!(failure.flags, 0);
        assert_eq!(u64::from(failure.total_entry_count), 8191);
        assert_eq!(u64::from(failure.start_entry_ordinal), 8192);
        assert_eq!(u64::from(failure.next_entry_ordinal), 0);
        assert_eq!(
            (
                failure.start_utf8,
                failure.start_utf16,
                failure.next_utf8,
                failure.next_utf16
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(failure.storage_pages_visited, 0);
        assert_eq!(failure.tree_nodes_visited, 0);
        assert_eq!(failure.packed_entries_inspected, 0);
        assert_eq!(failure.summary_nodes_skipped, 0);
        assert_eq!(failure.source_version, source.into());
        assert_eq!(failure.reserved, [0; 4]);
    }

    #[test]
    fn viewport_presentation_ffi_round_trips_canonical_begin_ack_and_receipts() {
        let source = FlarkV3HostSourceVersion {
            document_session: [301, 302, 303, 304],
            revision: 7,
            utf8_length: 80,
            utf16_length: 80,
            content_hash128: [305, 306, 307, 308],
        };
        let base_ack = FlarkV3HostStructuralAck {
            publication_session: [311, 312, 313, 314],
            host_revision: 4,
            source_version: source,
            source_root: [315, 316],
            parse_generation: 5,
            grammar_revision: 1,
            syntax_profile: 1,
            authority_mask: 0x1f,
            record_count: 12,
            sequence_digest: [317, 318, 319, 320],
            manifest_digest: [321, 322, 323, 324],
        };
        let binding = FlarkV3HostViewportPresentationBinding {
            viewport_generation: 9,
            requested_range: FlarkV3HostViewportPresentationMetricRange {
                start_utf8: 10,
                start_utf16: 10,
                end_utf8: 70,
                end_utf16: 70,
            },
            covered_range: FlarkV3HostViewportPresentationMetricRange {
                start_utf8: 20,
                start_utf16: 20,
                end_utf8: 50,
                end_utf16: 50,
            },
            start: FlarkV3HostViewportPresentationVisitStart {
                block_ordinal: 40_u64.into(),
                utf8_offset: 20,
                utf16_offset: 20,
            },
            next: FlarkV3HostViewportPresentationVisitStart {
                block_ordinal: 43_u64.into(),
                utf8_offset: 50,
                utf16_offset: 50,
            },
            complete: 0,
        };
        let envelope = FlarkV3HostViewportPresentationEnvelope {
            visited_structural_entries: 3,
            visited_storage_pages: 1,
            ordered_leaf_count: 2,
            inline_source_bytes: 30,
            fact_count: 6,
            transferred_node_count: 4,
            parser_transitions: 12,
            aggregate_envelope_digest256: [331, 332, 333, 334, 335, 336, 337, 338],
        };
        let begin = FlarkV3HostViewportPresentationBegin {
            schema: 1,
            mode: 1,
            offer_id: [341, 342, 343, 344],
            publication_session: [345, 346, 347, 348],
            base_ack,
            binding,
            envelope,
            query_limits: FlarkV3HostViewportPresentationQueryLimits {
                maximum_structural_entries: 128,
                maximum_storage_pages: 32,
                maximum_inline_leaves: 24,
                maximum_inline_leaf_source_bytes: 16 * 1024,
                maximum_inline_source_bytes: 256 * 1024,
                maximum_fact_records: 16 * 1024,
                maximum_encoded_frame_bytes: 256 * 1024,
                maximum_parser_transitions: 128 * 1024,
            },
            limits: FlarkV3HostViewportPresentationOfferLimits {
                maximum_frame_count: 64,
                maximum_encoded_frame_bytes: 256 * 1024,
                maximum_packet_bytes: FLARK_V3_HOST_MAXIMUM_PACKET_BYTES,
                maximum_frame_bytes: FLARK_V3_HOST_VIEWPORT_PRESENTATION_MAXIMUM_FRAME_BYTES,
                maximum_program_children: 256,
            },
        };
        let mapped = host_viewport_presentation_begin(begin).expect("canonical VPB1 begin");
        assert_eq!(mapped.offer_id, begin.offer_id);
        assert_eq!(mapped.publication_session, begin.publication_session);
        assert_eq!(mapped.base_ack, StructuralAck::from(base_ack));
        assert_eq!(
            mapped.binding,
            host_viewport_presentation_binding(binding).unwrap()
        );
        assert_eq!(mapped.envelope, envelope.into());
        assert_eq!(mapped.query_limits, begin.query_limits.into());
        assert_eq!(mapped.limits, begin.limits.into());

        let ack = ViewportPresentationAck {
            publication_session: begin.publication_session,
            base_ack: base_ack.into(),
            binding: mapped.binding,
            envelope: mapped.envelope,
            actual_frame_count: 11,
            actual_encoded_frame_bytes: 4_096,
            aggregate_root_stream_digest: [351, 352, 353, 354],
        };
        let ffi_ack = ffi_viewport_presentation_ack(ack);
        assert_eq!(
            host_viewport_presentation_ack(ffi_ack).expect("canonical VPB1 ACK"),
            ack
        );

        let committed =
            viewport_presentation_poll_receipt(HostViewportPresentationPollOutcome::Committed(ack));
        assert_eq!(committed.outcome, 2);
        assert_eq!(committed.offer_id, ack.publication_session);
        assert_eq!(committed.ack, ffi_ack);
        let available =
            viewport_presentation_query_receipt(HostViewportPresentationQueryOutcome::Available {
                encoded_bytes: 1_024,
                entry_count: 2,
            });
        assert_eq!(available.outcome, 1);
        assert_eq!(available.encoded_bytes, 1_024);
        assert_eq!(available.entry_count, 2);

        assert!(matches!(
            host_viewport_presentation_begin(FlarkV3HostViewportPresentationBegin {
                mode: 2,
                ..begin
            }),
            Err(NativeHostError::InvalidArgument)
        ));
        assert!(matches!(
            host_viewport_presentation_begin(FlarkV3HostViewportPresentationBegin {
                binding: FlarkV3HostViewportPresentationBinding {
                    complete: 2,
                    ..binding
                },
                ..begin
            }),
            Err(NativeHostError::InvalidArgument)
        ));
        let query = FlarkV3HostViewportPresentationQuery {
            schema: FLARK_V3_HOST_VIEWPORT_PRESENTATION_QUERY_SCHEMA,
            struct_size: size_of::<FlarkV3HostViewportPresentationQuery>() as u32,
            ack: ffi_ack,
            maximum_encoded_bytes: 4_096,
            reserved: [0; 3],
        };
        assert_eq!(
            host_viewport_presentation_query(query).expect("canonical VPB1 query"),
            (ack, 4_096)
        );
        assert!(matches!(
            host_viewport_presentation_query(FlarkV3HostViewportPresentationQuery {
                struct_size: 0,
                ..query
            }),
            Err(NativeHostError::InvalidArgument)
        ));
    }

    #[test]
    fn viewport_packet_ffi_envelope_decodes_before_host_admission() {
        let mut config = FlarkV3HostConfig::default();
        assert_eq!(unsafe { flark_v3_host_config_standard(&mut config) }, 0);
        config.document_session = [401, 402, 403, 404];
        let mut handle = FlarkV3HostHandle::default();
        assert_eq!(unsafe { flark_v3_host_create(&config, &mut handle) }, 0);

        let mut receipt = FlarkV3HostCallReceipt {
            rejection_reason: u32::MAX,
        };
        let malformed = b"not-an-fpk3-packet";
        assert_eq!(
            unsafe {
                flark_v3_host_admit_viewport_presentation_packet(
                    handle,
                    malformed.as_ptr(),
                    malformed.len() as u32,
                    &mut receipt,
                )
            },
            u32::from(Status::Invalid.code())
        );
        assert_eq!(receipt.rejection_reason, 0);

        let frame_bytes = [0x7f_u8];
        let frames = [PublicationPacketFrameInput {
            record_count: 0,
            digest: [1, 2, 3, 4],
            bytes: &frame_bytes,
        }];
        let mut encoded = vec![0_u8; MAXIMUM_PACKET_ENCODED_BYTES];
        let written = encode_publication_packet_into(
            PublicationPacketInput {
                offer_id: [411, 412, 413, 414],
                first_frame_ordinal: 0,
                first_record_ordinal: 0,
                frames: &frames,
            },
            &mut encoded,
        )
        .expect("encode FPK3 envelope");
        receipt.rejection_reason = u32::MAX;
        assert_eq!(
            unsafe {
                flark_v3_host_admit_viewport_presentation_packet(
                    handle,
                    encoded.as_ptr(),
                    written as u32,
                    &mut receipt,
                )
            },
            u32::from(Status::InvalidState.code())
        );
        assert_eq!(receipt.rejection_reason, 7);
        close_and_remove(handle);
    }

    #[test]
    fn packet_ffi_is_raw_bounded_and_envelope_validated_before_admission() {
        let document_session = [201, 202, 203, 204];
        let mut config = FlarkV3HostConfig::default();
        assert_eq!(unsafe { flark_v3_host_config_standard(&mut config) }, 0);
        config.document_session = document_session;
        let mut handle = FlarkV3HostHandle::default();
        assert_eq!(unsafe { flark_v3_host_create(&config, &mut handle) }, 0);

        let source = FlarkV3HostSourceVersion {
            document_session,
            revision: 1,
            utf8_length: 1,
            utf16_length: 1,
            content_hash128: [9, 10, 11, 12],
        };
        let mut receipt = FlarkV3HostCallReceipt::default();
        assert_eq!(
            unsafe { flark_v3_host_observe_source(handle, &source, &mut receipt) },
            0
        );
        let offer_id = [211, 212, 213, 214];
        let begin = FlarkV3HostOfferBegin {
            offer_id,
            publication_session: [221, 222, 223, 224],
            target_host_revision: 1,
            source_version: source,
            source_root: [1, 1],
            parse_generation: 1,
            grammar_revision: crate::FLARK_V3_GRAMMAR_REVISION,
            syntax_profile: 1,
            authority_mask: 0x1f,
            transferred_record_count: 1,
            target_record_count: 1,
            maximum_frame_count: 3,
            maximum_encoded_frame_bytes: 32,
            maximum_packet_bytes: FLARK_V3_HOST_MAXIMUM_PACKET_BYTES,
            maximum_frame_bytes: 32,
            maximum_program_children: 16,
            ..FlarkV3HostOfferBegin::default()
        };
        assert_eq!(
            unsafe { flark_v3_host_begin_offer(handle, &begin, &mut receipt) },
            0
        );

        let frame_bytes = [0x7f_u8];
        let frames = [PublicationPacketFrameInput {
            record_count: 0,
            digest: [1, 2, 3, 4],
            bytes: &frame_bytes,
        }];
        let mut packet = vec![0_u8; MAXIMUM_PACKET_ENCODED_BYTES];
        let written = encode_publication_packet_into(
            PublicationPacketInput {
                offer_id,
                first_frame_ordinal: 0,
                first_record_ordinal: 0,
                frames: &frames,
            },
            &mut packet,
        )
        .expect("encode FPK3 packet");
        packet.truncate(written);

        let mut corrupt = packet.clone();
        corrupt[0] ^= 0xff;
        receipt.rejection_reason = u32::MAX;
        assert_eq!(
            unsafe {
                flark_v3_host_admit_packet(
                    handle,
                    corrupt.as_ptr(),
                    corrupt.len() as u32,
                    &mut receipt,
                )
            },
            u32::from(Status::Invalid.code())
        );
        assert_eq!(receipt.rejection_reason, 0);

        assert_eq!(
            unsafe {
                flark_v3_host_admit_packet(
                    handle,
                    packet.as_ptr(),
                    packet.len() as u32,
                    &mut receipt,
                )
            },
            0
        );
        assert_eq!(receipt.rejection_reason, 0);
        assert_eq!(
            unsafe {
                flark_v3_host_admit_packet(
                    handle,
                    packet.as_ptr(),
                    packet.len() as u32,
                    &mut receipt,
                )
            },
            u32::from(Status::Backpressure.code())
        );
        assert_eq!(receipt.rejection_reason, 2);

        receipt.rejection_reason = u32::MAX;
        assert_eq!(
            unsafe {
                flark_v3_host_admit_packet(
                    handle,
                    std::ptr::null(),
                    FLARK_V3_HOST_MAXIMUM_PACKET_BYTES + 1,
                    &mut receipt,
                )
            },
            u32::from(Status::ForegroundBoundExceeded.code())
        );
        assert_eq!(receipt.rejection_reason, 0);
        close_and_remove(handle);
    }

    #[test]
    fn point_query_ffi_layout_maps_exact_metrics_affinity_and_typed_receipts() {
        let source = FlarkV3HostSourceVersion {
            document_session: [1, 2, 3, 4],
            revision: 7,
            utf8_length: 11,
            utf16_length: 9,
            content_hash128: [5, 6, 7, 8],
        };
        let request = FlarkV3HostPointQuery {
            schema: FLARK_V3_HOST_POINT_QUERY_SCHEMA,
            struct_size: size_of::<FlarkV3HostPointQuery>() as u32,
            source_version: source,
            position_utf8: 6,
            position_utf16: 4,
            affinity: 1,
            maximum_encoded_bytes: 4096,
            maximum_open_depth: 16,
            maximum_leaf_count: 64,
            maximum_tree_nodes_visited: 256,
            reserved: [0; 4],
        };
        let mapped = host_point_query(request).expect("valid point query ABI");
        assert_eq!(mapped.source_version, source.into());
        assert_eq!(mapped.position, HostSourceMetric { bytes: 6, utf16: 4 });
        assert_eq!(mapped.affinity, HostMetricAffinity::Downstream);
        assert_eq!(mapped.budget.maximum_encoded_bytes, 4096);
        assert_eq!(mapped.budget.maximum_open_depth, 16);
        assert_eq!(mapped.budget.maximum_leaf_count, 64);
        assert_eq!(mapped.budget.maximum_tree_nodes_visited, 256);

        let source_version = SourceVersion::from(source);
        let range = HostMetricRange {
            start: HostSourceMetric { bytes: 2, utf16: 2 },
            end: HostSourceMetric {
                bytes: 11,
                utf16: 9,
            },
        };
        let receipt = point_query_receipt(HostStructuralQueryOutcome::SourceGap {
            source_version,
            range,
            reason: HostSourceGapReason::TreeNodeLimit,
            receipt: HostViewportReceipt {
                encoded_bytes: 0,
                leaf_count: 1,
                open_depth: 1,
                tree_nodes_visited: 3,
                summary_nodes_skipped: 2,
            },
        });
        assert_eq!(receipt.rejection_reason, 0);
        assert_eq!(receipt.outcome, 2);
        assert_eq!(receipt.gap_reason, 4);
        assert_eq!(receipt.encoded_bytes, 0);
        assert_eq!(receipt.leaf_count, 1);
        assert_eq!(receipt.open_depth, 1);
        assert_eq!(receipt.tree_nodes_visited, 3);
        assert_eq!(receipt.summary_nodes_skipped, 2);
        assert_eq!(receipt.range_start_utf8, 2);
        assert_eq!(receipt.range_start_utf16, 2);
        assert_eq!(receipt.range_end_utf8, 11);
        assert_eq!(receipt.range_end_utf16, 9);
        assert_eq!(receipt.source_version, source);

        for invalid in [
            FlarkV3HostPointQuery {
                affinity: 2,
                ..request
            },
            FlarkV3HostPointQuery {
                struct_size: 0,
                ..request
            },
            FlarkV3HostPointQuery {
                reserved: [0, 0, 0, 1],
                ..request
            },
        ] {
            assert!(matches!(
                host_point_query(invalid),
                Err(NativeHostError::InvalidArgument)
            ));
        }
    }

    #[test]
    fn block_range_ffi_maps_exact_bounds_opaque_continuation_and_receipt() {
        let source = FlarkV3HostSourceVersion {
            document_session: [41, 42, 43, 44],
            revision: 9,
            utf8_length: 100,
            utf16_length: 90,
            content_hash128: [45, 46, 47, 48],
        };
        let continuation_bytes = [0x5a; FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES];
        let request = FlarkV3HostBlockRangeQuery {
            schema: FLARK_V3_HOST_BLOCK_RANGE_QUERY_SCHEMA,
            struct_size: size_of::<FlarkV3HostBlockRangeQuery>() as u32,
            source_version: source,
            requested_start_utf8: 11,
            requested_start_utf16: 9,
            requested_end_utf8: 77,
            requested_end_utf16: 68,
            maximum_encoded_bytes: 16 * 1024,
            maximum_block_count: 64,
            maximum_storage_pages_visited: 8,
            maximum_open_depth: 32,
            maximum_tree_nodes_visited: 512,
            continuation_length: FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES as u32,
            continuation: continuation_bytes,
            reserved: [0; 4],
        };
        let mapped = host_block_range_query(request).expect("valid block-range query ABI");
        assert_eq!(mapped.source_version, source.into());
        assert_eq!(
            mapped.requested_range,
            HostMetricRange {
                start: HostSourceMetric {
                    bytes: 11,
                    utf16: 9,
                },
                end: HostSourceMetric {
                    bytes: 77,
                    utf16: 68,
                },
            }
        );
        assert_eq!(mapped.budget.maximum_encoded_bytes, 16 * 1024);
        assert_eq!(mapped.budget.maximum_block_count, 64);
        assert_eq!(mapped.budget.maximum_storage_pages_visited, 8);
        assert_eq!(mapped.budget.maximum_open_depth, 32);
        assert_eq!(mapped.budget.maximum_tree_nodes_visited, 512);
        assert_eq!(
            mapped.continuation.expect("opaque continuation").encoded(),
            continuation_bytes
        );

        let continuation = HostBlockRangeContinuation::from_encoded(continuation_bytes);
        let receipt = block_range_query_receipt(HostBlockRangeOutcome::Page {
            source_version: source.into(),
            requested_range: mapped.requested_range,
            covered_range: HostMetricRange {
                start: HostSourceMetric { bytes: 7, utf16: 6 },
                end: HostSourceMetric {
                    bytes: 51,
                    utf16: 44,
                },
            },
            continuation: Some(continuation),
            receipt: HostBlockRangeReceipt {
                encoded_bytes: 352,
                block_count: 2,
                storage_pages_visited: 1,
                open_depth: 4,
                tree_nodes_visited: 13,
                packed_entries_inspected: 7,
                summary_nodes_skipped: 3,
                complete: false,
            },
        });
        assert_eq!(receipt.rejection_reason, 0);
        assert_eq!(receipt.outcome, 1);
        assert_eq!(receipt.gap_reason, 0);
        assert_eq!(receipt.encoded_bytes, 352);
        assert_eq!(receipt.block_count, 2);
        assert_eq!(receipt.storage_pages_visited, 1);
        assert_eq!(receipt.open_depth, 4);
        assert_eq!(receipt.tree_nodes_visited, 13);
        assert_eq!(receipt.packed_entries_inspected, 7);
        assert_eq!(receipt.summary_nodes_skipped, 3);
        assert_eq!(receipt.flags, 0);
        assert_eq!(receipt.coverage_start_utf8, 7);
        assert_eq!(receipt.coverage_start_utf16, 6);
        assert_eq!(receipt.coverage_end_utf8, 51);
        assert_eq!(receipt.coverage_end_utf16, 44);
        assert_eq!(receipt.source_version, source);
        assert_eq!(
            receipt.continuation_length,
            FLARK_V3_HOST_BLOCK_RANGE_CONTINUATION_BYTES as u32
        );
        assert_eq!(receipt.continuation, continuation_bytes);

        for invalid in [
            FlarkV3HostBlockRangeQuery {
                continuation_length: 0,
                ..request
            },
            FlarkV3HostBlockRangeQuery {
                continuation_length: 1,
                ..request
            },
            FlarkV3HostBlockRangeQuery {
                struct_size: 0,
                ..request
            },
            FlarkV3HostBlockRangeQuery {
                reserved: [0, 0, 0, 1],
                ..request
            },
        ] {
            assert!(matches!(
                host_block_range_query(invalid),
                Err(NativeHostError::InvalidArgument)
            ));
        }
    }
}

//! Independent native M1.1 host-store protocol owner.
//!
//! The parser endpoint and this store never share arenas or object handles.
//! One admitted packet owns a bounded sequence of closed engine snapshot
//! frames. The host copies that packet once, then independently classifies and
//! digests whole frames under poll fuel, reconstructs them in a separate arena,
//! and installs only after a causal Commit request.

use std::fmt;

use flark_engine::m11_host::{
    M11CandidateHost, M11HostBlockAffinity, M11HostBlockKind, M11HostBlockQuoteCursor,
    M11HostBlockQuoteCursorPoll, M11HostBlockQuoteLine, M11HostBlockQuoteSidecarDescriptor,
    M11HostBlockUnsupportedReason, M11HostCanonicalLineEnding, M11HostError, M11HostFrameKind,
    M11HostIndentedCodeCursorPoll, M11HostInlineProjectionCursorPoll, M11HostInlineProjectionFact,
    M11HostInlineSidecar, M11HostInlineSidecarBinding, M11HostInlineSidecarOwner,
    M11HostInlineSidecarQuery, M11HostInstalledCandidate, M11HostLimits,
    M11HostPersistentBlockDescriptor, M11HostPersistentBlockLocation,
    M11HostPersistentBlockOrdinalWindow, M11HostPersistentBlockVisitControl,
    M11HostPersistentBlockVisitDisposition, M11HostPersistentBlockVisitEntry,
    M11HostPersistentBlockVisitStart, M11HostPersistentRecursiveGreenDescriptor,
    M11HostRecursiveGreenCoveragePart, M11HostRecursiveGreenLocation,
    M11HostRecursiveGreenLogicalAtom, M11HostRecursiveGreenPointQueryOutcome,
    M11HostRecursiveGreenRow, M11HostRecursiveGreenRowEditCapability,
    M11HostRecursiveGreenRowOrdinalWindow, M11HostRecursiveGreenRowPath,
    M11HostRecursiveGreenRowQueryLimit, M11HostRecursiveGreenRowQueryOutcome, M11HostRole,
    M11HostSourceVersion, M11_CANDIDATE_ARENA_MAX_SLOTS, M11_HOST_MAXIMUM_FRAME_BYTES,
    M11_HOST_MAXIMUM_PROGRAM_CHILDREN,
};
use flark_parser::{
    M11_GREEN_RECORD_BYTES, M11_INLINE_FACTS_PER_PAGE, M11_INLINE_FACT_RECORD_BYTES,
    M11_INLINE_META_MAGIC, M11_INLINE_META_RECORD_BYTES, M11_INLINE_PAGE_HEADER_BYTES,
    M11_INLINE_PAGE_MAGIC, M11_INLINE_SCHEMA, M11_PROJECTION_RECORD_BYTES,
};

use crate::v3_publication_wire::{
    protocol_digest128_from_blake3, CandidateSnapshotFrameKind, CandidateTransportDigest,
    CommitRequest, Digest128, HotInlineSidecarBegin, HotInlineSidecarBinding,
    HotInlineSidecarCommitRequest, HotInlineSidecarDisposition, HotInlineSidecarEnvelopeMetrics,
    HotInlineSidecarFrameKind, HotInlineSidecarOwner, HotInlineSidecarTransportDigest, Id128,
    InlineSidecarAck, InlineSidecarAckDisposition, OfferBegin, ProtocolDigestDomain,
    PublicationMode, PublicationPacket, SourceVersion, StructuralAck,
    BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES, HOT_INLINE_SIDECAR_SCHEMA,
    MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES, MAXIMUM_PACKET_ENCODED_BYTES, MAXIMUM_PACKET_FRAME_COUNT,
    PACKET_FRAME_DESCRIPTOR_BYTES, PACKET_HEADER_BYTES,
    PROJECTED_INLINE_PROJECTION_DESCRIPTOR_BYTES,
};

#[path = "v3_viewport_host.rs"]
mod viewport_presentation_host;

use viewport_presentation_host::ViewportPresentationHost;
pub(crate) use viewport_presentation_host::{
    HostViewportPresentationPollOutcome, HostViewportPresentationQueryOutcome,
    HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES, HOST_VIEWPORT_PRESENTATION_HEADER_BYTES,
    HOST_VIEWPORT_PRESENTATION_SCHEMA,
};

pub(crate) const HOST_INLINE_SIDECAR_MAXIMUM_QUERY_BYTES: u32 = 128 * 1024;
pub(crate) const HOST_VIEWPORT_PRESENTATION_MAXIMUM_QUERY_BYTES: u32 = 256 * 1024;

const SUPPORTED_MANIFEST_SCHEMA: u32 = 1;
const KNOWN_AUTHORITY_BITS: u32 = 0x1f;
const VIEWPORT_MAGIC: &[u8; 8] = b"FLKVP001";
const BLOCK_RANGE_MAGIC: &[u8; 8] = b"FLKVR001";
const BLOCK_RANGE_CONTINUATION_MAGIC: &[u8; 8] = b"FLKRC001";
const GREEN_MAGIC: &[u8; 8] = b"FLKGR001";
const PROJECTION_MAGIC: &[u8; 8] = b"FLKPR001";
const M11_ROLE_SCHEMA: u32 = 1;
const M11_FENCED_CODE_VARIANT: u8 = 3;
const M11_FENCE_CLOSED_FLAG: u64 = 1 << 16;
const M11_FENCE_METADATA_MASK: u64 = 0x1_ffff;
const M11_FENCE_ABSENT_CUT: u32 = u32::MAX;
const M11_ATX_HEADING_VARIANT: u8 = 4;
const M11_ATX_HEADING_CLOSED_FLAG: u64 = 1 << 8;
const M11_ATX_HEADING_OPENING_INDENT_SHIFT: u32 = 9;
const M11_ATX_HEADING_BOF_BOM_FLAG: u64 = 1 << 11;
const M11_ATX_HEADING_METADATA_MASK: u64 = 0xfff;
const M11_ATX_HEADING_ABSENT_CUT: u32 = u32::MAX;
const M11_SETEXT_HEADING_VARIANT: u8 = 5;
const M11_SETEXT_HEADING_OPENING_INDENT_SHIFT: u32 = 8;
const M11_SETEXT_HEADING_METADATA_MASK: u64 = 0x3ff;
const M11_THEMATIC_BREAK_VARIANT: u8 = 6;
const M11_THEMATIC_BREAK_OPENING_INDENT_SHIFT: u32 = 8;
const M11_THEMATIC_BREAK_BOF_BOM_FLAG: u64 = 1 << 10;
const M11_THEMATIC_BREAK_METADATA_MASK: u64 = 0x7ff;
const M11_INDENTED_CODE_VARIANT: u8 = 7;
const M11_INDENTED_CODE_DEINDENT_COLUMNS: u64 = 4;
const M11_INDENTED_CODE_BOF_BOM_FLAG: u64 = 1 << 8;
const M11_INDENTED_CODE_METADATA_MASK: u64 = 0x1ff;
const M11_BLOCK_QUOTE_VARIANT: u8 = 8;
const M11_BLOCK_QUOTE_EXACT_SINGLE_PARAGRAPH_DISPOSITION: u64 = 1;
const M11_BULLET_LIST_VARIANT: u8 = 9;
const M11_BULLET_LIST_EXACT_TIGHT_DISPOSITION: u64 = 1;
const M11_BULLET_LIST_MARKER_SHIFT: u32 = 8;
const M11_BULLET_LIST_TIGHT_FLAG: u64 = 1 << 16;
const M11_BULLET_LIST_METADATA_MASK: u64 = 0x1_ffff;
const M11_BULLET_LIST_NO_TERMINAL_EMPTY: u32 = u32::MAX;
const M11_ORDERED_LIST_VARIANT: u8 = 10;
const M11_ORDERED_LIST_EXACT_TIGHT_DISPOSITION: u64 = 1;
const M11_ORDERED_LIST_DELIMITER_SHIFT: u32 = 8;
const M11_ORDERED_LIST_TIGHT_FLAG: u64 = 1 << 16;
const M11_ORDERED_LIST_METADATA_MASK: u64 = 0x1_ffff;
const M11_ORDERED_LIST_NO_TERMINAL_EMPTY: u32 = u32::MAX;
const M11_VIEWPORT_HEADER_BYTES: usize = 20;
const M11_VIEWPORT_INLINE_SCHEMA: u32 = 8;
const M11_VIEWPORT_INLINE_HEADER_BYTES: usize = 24;
const M11_VIEWPORT_V3_SCHEMA: u32 = 3;
const M11_VIEWPORT_V3_HEADER_BYTES: usize = 28;
const M11_VIEWPORT_V4_SCHEMA: u32 = 4;
const M11_VIEWPORT_V4_HEADER_BYTES: usize = 32;
const M11_VIEWPORT_V5_SCHEMA: u32 = 5;
const M11_VIEWPORT_V5_HEADER_BYTES: usize = 32;
const M11_VIEWPORT_V6_SCHEMA: u32 = 6;
const M11_VIEWPORT_V6_HEADER_BYTES: usize = 32;
const M11_VIEWPORT_V7_SCHEMA: u32 = 7;
/// Generic recursive-Green selected-leaf viewport.
///
/// Unlike schemas 1 through 8, schema 9 carries opaque recursive-Green kind
/// registry values and authenticated ancestry rather than a fixed Markdown
/// block-role projection. Its shape is independent of the current point
/// lookup algorithm so the linear implementation can be replaced by a Green
/// zipper without another wire migration.
pub const HOST_RECURSIVE_GREEN_VIEWPORT_SCHEMA: u32 = 9;
pub const HOST_RECURSIVE_GREEN_VIEWPORT_HEADER_BYTES: usize = 112;
pub const HOST_RECURSIVE_GREEN_ANCESTOR_RECORD_BYTES: usize = 16;
pub const HOST_RECURSIVE_GREEN_KIND_REGISTRY_SCHEMA: u32 = 1;
pub const HOST_RECURSIVE_GREEN_COVERAGE_SCHEMA: u32 = 1;
pub const HOST_RECURSIVE_GREEN_LOGICAL_ATOM_SCHEMA: u32 = 1;
const HOST_RECURSIVE_GREEN_EMPTY_ITEM_ROW_KIND: u16 = 14;
const HOST_RECURSIVE_GREEN_ANCESTOR_OWNER_FLAG: u16 = 1;
const M11_VIEWPORT_V7_HEADER_BYTES: usize = 32;
const M11_LEAF_PROJECTION_PAYLOAD_INLINE: u8 = 1;
const M11_LEAF_PROJECTION_PAYLOAD_INDENTED_CODE: u8 = 2;
const M11_LEAF_PROJECTION_PAYLOAD_BLOCK_QUOTE: u8 = 3;
const M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST: u8 = 4;
const M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST_ITEM: u8 = 5;
const M11_LEAF_PROJECTION_PAYLOAD_ORDERED_LIST_ITEM: u8 = 6;
const M11_INDENTED_CODE_LINE_RECORD_BYTES: usize = 20;
const M11_BLOCK_QUOTE_LINE_RECORD_BYTES: usize = 20;
const M11_BULLET_LIST_ITEM_RECORD_BYTES: usize = 28;
const M11_BULLET_LIST_ITEM_META_BYTES: usize = 8;
const M11_ORDERED_LIST_ITEM_RECORD_BYTES: usize = 28;
const M11_ORDERED_LIST_ITEM_META_BYTES: usize = 20;
const M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES: usize =
    M11_ORDERED_LIST_ITEM_META_BYTES + M11_ORDERED_LIST_ITEM_RECORD_BYTES;
const M11_POINT_PATH_NODE_RECORD_BYTES: usize = 40;
const M11_POINT_PATH_V5_NODE_RECORD_BYTES: usize = 32;
const M11_BLOCK_QUOTE_POINT_PATH_NODE_COUNT: usize = 2;
const M11_BLOCK_QUOTE_POINT_PATH_BYTES: usize =
    M11_POINT_PATH_NODE_RECORD_BYTES * M11_BLOCK_QUOTE_POINT_PATH_NODE_COUNT;
const M11_POINT_PATH_KIND_BLOCK_QUOTE: u8 = 1;
const M11_POINT_PATH_KIND_PARAGRAPH: u8 = 2;
const M11_POINT_PATH_KIND_LIST: u8 = 3;
const M11_POINT_PATH_KIND_LIST_ITEM: u8 = 4;
const M11_POINT_PATH_FLAG_NONCONTIGUOUS: u8 = 1;
const M11_POINT_PATH_FLAG_SELECTED: u8 = 1 << 1;
const M11_POINT_PATH_ROOT_PARENT: u32 = u32::MAX;
const M11_INLINE_OVERLAY_BEGIN_HEADER_BYTES: usize = 16;
const M11_INLINE_OVERLAY_DIGEST_BYTES: usize = 32;
pub const HOST_BLOCK_RANGE_HEADER_BYTES: usize = 32;
pub const HOST_BLOCK_RANGE_RECORD_BYTES: usize =
    8 + 4 * 4 + M11_GREEN_RECORD_BYTES + M11_PROJECTION_RECORD_BYTES;
pub const HOST_BLOCK_RANGE_CONTINUATION_BYTES: usize = 64;
pub const HOST_STRUCTURAL_ORDINAL_WINDOW_MAXIMUM_ENTRIES: u32 = 4096;
const HOST_BLOCK_RANGE_SCHEMA: u32 = 1;
pub const HOST_RECURSIVE_GREEN_ROW_RANGE_SCHEMA: u32 = 11;
pub const HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES: usize = 96;
pub const HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES: usize = 64;
pub const HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES: usize = 48;
const HOST_BLOCK_RANGE_COMPLETE_FLAG: u32 = 1;
const HOST_RECURSIVE_GREEN_ROW_SELECTED_FLAG: u16 = 1;
const HOST_RECURSIVE_GREEN_ROW_INLINE_FLAG: u16 = 1 << 1;
const HOST_RECURSIVE_GREEN_ROW_LITERAL_FLAG: u16 = 1 << 2;
const HOST_RECURSIVE_GREEN_PATH_ROW_OWNER_FLAG: u16 = 1;
const HOST_RECURSIVE_GREEN_PATH_CONTAINER_FLAG: u16 = 1 << 1;
const HOST_RECURSIVE_GREEN_PATH_OPEN_FACT_FLAG: u16 = 1 << 2;
const HOST_RECURSIVE_GREEN_PATH_CLOSE_FACT_FLAG: u16 = 1 << 3;
pub const HOST_M11_VIEWPORT_BYTES: usize =
    M11_VIEWPORT_HEADER_BYTES + M11_GREEN_RECORD_BYTES + M11_PROJECTION_RECORD_BYTES;
const V3_PRODUCT_HOST_ARENA_MAX_SLOTS: usize = 262_144;
const V3_PRODUCT_HOST_ARENA_MAX_LIVE_PAYLOAD_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostMarkedLinePayloadKind {
    BlockQuote,
    BulletList,
}

impl HostMarkedLinePayloadKind {
    const fn record_bytes(self) -> usize {
        match self {
            Self::BlockQuote => M11_BLOCK_QUOTE_LINE_RECORD_BYTES,
            Self::BulletList => M11_BULLET_LIST_ITEM_RECORD_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostConfig {
    pub document_session: Id128,
    pub grammar_revision: u32,
    pub syntax_profile: u32,
    pub authority_mask: u32,
    pub maximum_query_bytes: u32,
}

impl HostConfig {
    pub fn validate(self) -> Result<Self, HostStoreError> {
        if self.document_session == [0; 4]
            || self.grammar_revision == 0
            || self.syntax_profile == 0
            || self.authority_mask == 0
            || self.authority_mask & !KNOWN_AUTHORITY_BITS != 0
            || self.maximum_query_bytes == 0
            || self.maximum_query_bytes > 64 * 1024
        {
            return Err(HostStoreError::invalid("invalid host configuration"));
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostRejectReason {
    Invalid,
    Backpressure,
    StaleSource,
    ExactSourceMismatch,
    BaseMismatch,
    WrongOffer,
    CorruptPublication,
    QueryBoundExceeded,
    ForegroundBoundExceeded,
    #[allow(dead_code)] // Stable Dart rejection code; no current host path emits it.
    Superseded,
    Closed,
    NotReady,
    AllocationFailed,
    InternalFault,
}

#[derive(Debug)]
pub struct HostStoreError {
    reason: HostRejectReason,
    message: &'static str,
}

impl HostStoreError {
    const fn new(reason: HostRejectReason, message: &'static str) -> Self {
        Self { reason, message }
    }

    const fn invalid(message: &'static str) -> Self {
        Self::new(HostRejectReason::Invalid, message)
    }

    #[must_use]
    pub const fn reason(&self) -> HostRejectReason {
        self.reason
    }
}

impl fmt::Display for HostStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for HostStoreError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostPollOutcome {
    Pending,
    PacketCredit {
        offer_id: Id128,
        next_frame_ordinal: u32,
    },
    Committed(StructuralAck),
    AbortComplete {
        offer_id: Id128,
    },
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InlineSidecarHostPollOutcome {
    Pending,
    PacketCredit {
        offer_id: Id128,
        next_frame_ordinal: u32,
    },
    Committed(InlineSidecarAck),
    AbortComplete {
        offer_id: Id128,
    },
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostInlineSidecarQueryOutcome {
    Authoritative {
        payload_kind: HostInlineSidecarPayloadKind,
        fact_count: u32,
        value_entry_count: u32,
        value_encoded_bytes: u32,
        encoded_bytes: u32,
        tree_nodes_visited: u32,
    },
    Unsupported {
        reason: u32,
        metadata_bytes: u32,
    },
    Unavailable,
}

/// Stable record format returned by a direct sidecar query.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostInlineSidecarPayloadKind {
    Inline = 1,
    IndentedCode = 2,
    BlockQuote = 3,
    BulletList = 4,
    ProjectedInline = 5,
    OrderedListItem = 6,
}

impl HostInlineSidecarPayloadKind {
    pub const fn wire(self) -> u32 {
        self as u32
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostWorkGrant {
    pub inspect_bytes: u32,
    pub copy_bytes: u32,
    pub transitions: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HostSourceMetric {
    pub bytes: u32,
    pub utf16: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostMetricAffinity {
    Upstream,
    Downstream,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostQueryBudget {
    pub maximum_encoded_bytes: u32,
    pub maximum_open_depth: u32,
    pub maximum_leaf_count: u32,
    pub maximum_tree_nodes_visited: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostPointQuery {
    pub source_version: SourceVersion,
    pub position: HostSourceMetric,
    pub affinity: HostMetricAffinity,
    pub budget: HostQueryBudget,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HostMetricRange {
    pub start: HostSourceMetric,
    pub end: HostSourceMetric,
}

/// Caller-owned hard bounds for one top-level structural range page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostBlockRangeBudget {
    pub maximum_encoded_bytes: u32,
    pub maximum_block_count: u32,
    pub maximum_storage_pages_visited: u32,
    pub maximum_open_depth: u32,
    pub maximum_tree_nodes_visited: u32,
}

/// Opaque, installed-publication-bound resume claim.
///
/// The fixed bytes cross native and Wasm ABIs unchanged. Only this host-store
/// module interprets them; callers must repeat the original exact source and
/// requested range on every page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostBlockRangeContinuation([u8; HOST_BLOCK_RANGE_CONTINUATION_BYTES]);

impl HostBlockRangeContinuation {
    #[must_use]
    pub const fn from_encoded(encoded: [u8; HOST_BLOCK_RANGE_CONTINUATION_BYTES]) -> Self {
        Self(encoded)
    }

    #[must_use]
    pub const fn encoded(self) -> [u8; HOST_BLOCK_RANGE_CONTINUATION_BYTES] {
        self.0
    }
}

/// Exact-current half-open source range requested from the persistent block
/// sequence. A continuation is valid only with this same source and range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostBlockRangeQuery {
    pub source_version: SourceVersion,
    pub requested_range: HostMetricRange,
    pub budget: HostBlockRangeBudget,
    pub continuation: Option<HostBlockRangeContinuation>,
}

/// Bounded work and copy receipt for one range page.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HostBlockRangeReceipt {
    pub encoded_bytes: u32,
    pub block_count: u32,
    pub storage_pages_visited: u32,
    pub open_depth: u32,
    pub tree_nodes_visited: u32,
    pub packed_entries_inspected: u32,
    pub summary_nodes_skipped: u32,
    pub complete: bool,
}

/// One bounded page or a typed range-wide source gap. Normal page truncation
/// returns a continuation rather than a gap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostBlockRangeOutcome {
    Page {
        source_version: SourceVersion,
        requested_range: HostMetricRange,
        covered_range: HostMetricRange,
        continuation: Option<HostBlockRangeContinuation>,
        receipt: HostBlockRangeReceipt,
    },
    SourceGap {
        source_version: SourceVersion,
        requested_range: HostMetricRange,
        reason: HostSourceGapReason,
        receipt: HostBlockRangeReceipt,
    },
}

/// Caller-owned hard bounds for one direct structural ordinal-window lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostStructuralOrdinalWindowBudget {
    pub maximum_entries: u32,
    pub maximum_storage_pages_visited: u32,
    pub maximum_tree_nodes_visited: u32,
    pub maximum_packed_entries_inspected: u32,
}

/// Exact-current top-level structural ordinal window request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostStructuralOrdinalWindowQuery {
    pub source_version: SourceVersion,
    pub start_entry_ordinal: u64,
    pub budget: HostStructuralOrdinalWindowBudget,
}

/// Bounded work performed by an ordinal-window lookup.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HostStructuralOrdinalWindowReceipt {
    pub storage_pages_visited: u32,
    pub tree_nodes_visited: u32,
    pub packed_entries_inspected: u32,
    pub summary_nodes_skipped: u32,
}

/// Typed fail-closed reason for a structurally valid ordinal query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostStructuralOrdinalWindowFailureReason {
    UnavailableFacts,
    EntryWindowLimit,
    StoragePageLimit,
    TreeNodeLimit,
    PackedEntryLimit,
    OrdinalOutOfRange,
    UndecodableClosure,
}

/// One exact measured ordinal window or a typed failure. Authority failures
/// remain host errors so existing stale/exact-source rejection semantics are
/// preserved across every query API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostStructuralOrdinalWindowOutcome {
    Window {
        source_version: SourceVersion,
        total_entry_count: u64,
        start_entry_ordinal: u64,
        next_entry_ordinal: u64,
        start: HostSourceMetric,
        next: HostSourceMetric,
        complete: bool,
        receipt: HostStructuralOrdinalWindowReceipt,
    },
    Failure {
        source_version: SourceVersion,
        total_entry_count: u64,
        start_entry_ordinal: u64,
        reason: HostStructuralOrdinalWindowFailureReason,
        receipt: HostStructuralOrdinalWindowReceipt,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HostViewportReceipt {
    pub encoded_bytes: u32,
    pub leaf_count: u32,
    pub open_depth: u32,
    pub tree_nodes_visited: u32,
    pub summary_nodes_skipped: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostSourceGapReason {
    OpenDepthLimit,
    EncodedByteLimit,
    LeafLimit,
    TreeNodeLimit,
    UndecodableClosure,
    UnavailableFacts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostStructuralQueryOutcome {
    Viewport {
        source_version: SourceVersion,
        range: HostMetricRange,
        receipt: HostViewportReceipt,
    },
    SourceGap {
        source_version: SourceVersion,
        range: HostMetricRange,
        reason: HostSourceGapReason,
        receipt: HostViewportReceipt,
    },
}

/// One packet-sized ownership transfer plus the exact retained decode cursor.
///
/// The packet body is never copied again. Descriptor and frame offsets advance
/// only after one complete frame fits the poll grant and passes independent
/// host validation.
struct OwnedPacket {
    offer_id: Id128,
    first_frame_ordinal: u32,
    first_record_ordinal: u32,
    frame_count: u32,
    aggregate_record_count: u32,
    aggregate_frame_bytes: u32,
    first_accepted_frame_bytes: u32,
    next_index: u32,
    directory_offset: usize,
    body_offset: usize,
    next_record_ordinal: u32,
    end_range: Option<(usize, usize)>,
    bytes: Vec<u8>,
}

#[derive(Clone, Copy)]
struct PendingFrame<'payload> {
    offer_id: Id128,
    ordinal: u32,
    first_record_ordinal: u32,
    record_count: u32,
    digest: Digest128,
    bytes: &'payload [u8],
}

struct RetainedFrame {
    storage: Vec<u8>,
    start: usize,
    end: usize,
}

impl RetainedFrame {
    fn bytes(&self) -> &[u8] {
        &self.storage[self.start..self.end]
    }
}

fn copy_packet_storage(encoded: &[u8]) -> Result<Vec<u8>, HostStoreError> {
    let mut storage = Vec::new();
    reserve_packet_storage(&mut storage, encoded.len())?;
    storage.extend_from_slice(encoded);
    Ok(storage)
}

fn reserve_packet_storage(storage: &mut Vec<u8>, additional: usize) -> Result<(), HostStoreError> {
    storage.try_reserve_exact(additional).map_err(|_| {
        HostStoreError::new(
            HostRejectReason::AllocationFailed,
            "packet ownership allocation failed",
        )
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OfferPhase {
    Receiving,
    AwaitingCommit,
    Installing,
    Failed,
}

struct ActiveOffer {
    begin: OfferBegin,
    exact_base: Option<M11HostInstalledCandidate>,
    /// The engine must finish the bounded SourceFacts replay step before this
    /// host may inspect the next exact-base frame or return packet credit.
    exact_replay_required: bool,
    transport: Option<CandidateTransportDigest>,
    next_frame_ordinal: u32,
    next_node_ordinal: Option<u64>,
    accepted_record_count: u32,
    accepted_frame_bytes: u32,
    pending_packet: Option<OwnedPacket>,
    retained_end: Option<RetainedFrame>,
    canonical_stream_digest256: Option<[u8; 32]>,
    commit: Option<CommitRequest>,
    phase: OfferPhase,
}

impl ActiveOffer {
    fn new(begin: OfferBegin, exact_base: Option<M11HostInstalledCandidate>) -> Self {
        Self {
            begin,
            exact_base,
            exact_replay_required: false,
            transport: Some(CandidateTransportDigest::new()),
            next_frame_ordinal: 0,
            next_node_ordinal: None,
            accepted_record_count: 0,
            accepted_frame_bytes: 0,
            pending_packet: None,
            retained_end: None,
            canonical_stream_digest256: None,
            commit: None,
            phase: OfferPhase::Receiving,
        }
    }
}

struct ActiveInlineSidecarOffer {
    begin: HotInlineSidecarBegin,
    binding: M11HostInlineSidecarBinding,
    transport: Option<HotInlineSidecarTransportDigest>,
    next_frame_ordinal: u32,
    next_node_ordinal: Option<u64>,
    accepted_node_count: u32,
    accepted_frame_bytes: u32,
    pending_packet: Option<OwnedPacket>,
    retained_end: Option<RetainedFrame>,
    root_stream_digest256: Option<[u8; 32]>,
    commit: Option<HotInlineSidecarCommitRequest>,
    phase: OfferPhase,
}

impl ActiveInlineSidecarOffer {
    fn new(begin: HotInlineSidecarBegin, binding: M11HostInlineSidecarBinding) -> Self {
        Self {
            begin,
            binding,
            transport: Some(HotInlineSidecarTransportDigest::new()),
            next_frame_ordinal: 0,
            next_node_ordinal: None,
            accepted_node_count: 0,
            accepted_frame_bytes: 0,
            pending_packet: None,
            retained_end: None,
            root_stream_digest256: None,
            commit: None,
            phase: OfferPhase::Receiving,
        }
    }
}

struct InstalledInlineSidecar {
    begin: HotInlineSidecarBegin,
    binding: M11HostInlineSidecarBinding,
    ack: InlineSidecarAck,
}

/// Persistent host-side publication state. The first exact offer lazily
/// creates the independent engine arena because observe-source intentionally
/// carries no parser-owned source-root handle.
pub struct NativeCandidateHost {
    config: HostConfig,
    engine_limits: M11HostLimits,
    current_source: Option<SourceVersion>,
    engine_source: Option<(u32, [u32; 2])>,
    engine: Option<M11CandidateHost>,
    inline_sidecar: Option<M11HostInlineSidecar>,
    viewport_presentation: ViewportPresentationHost,
    active: Option<ActiveOffer>,
    active_inline_sidecar: Option<ActiveInlineSidecarOffer>,
    aborting_offer: Option<Id128>,
    aborting_inline_sidecar_offer: Option<Id128>,
    installed_ack: Option<StructuralAck>,
    pending_delivery_ack: Option<StructuralAck>,
    installed_inline_sidecar: Option<InstalledInlineSidecar>,
    pending_inline_sidecar_delivery_ack: Option<InlineSidecarAck>,
    background_reclaim_pending: bool,
    inline_sidecar_reclaim_pending: bool,
    closing: bool,
    closed: bool,
}

impl NativeCandidateHost {
    pub fn new(config: HostConfig) -> Result<Self, HostStoreError> {
        let limits = M11HostLimits {
            arena_max_slots: V3_PRODUCT_HOST_ARENA_MAX_SLOTS,
            arena_max_live_payload_bytes: V3_PRODUCT_HOST_ARENA_MAX_LIVE_PAYLOAD_BYTES,
            maximum_snapshot_nodes: M11_CANDIDATE_ARENA_MAX_SLOTS as u64,
            maximum_query_bytes: config.maximum_query_bytes as usize,
            ..M11HostLimits::default()
        };
        Self::new_with_limits(config, limits)
    }

    pub(crate) fn new_with_limits(
        config: HostConfig,
        engine_limits: M11HostLimits,
    ) -> Result<Self, HostStoreError> {
        Ok(Self {
            config: config.validate()?,
            engine_limits,
            current_source: None,
            engine_source: None,
            engine: None,
            inline_sidecar: None,
            viewport_presentation: ViewportPresentationHost::new(),
            active: None,
            active_inline_sidecar: None,
            aborting_offer: None,
            aborting_inline_sidecar_offer: None,
            installed_ack: None,
            pending_delivery_ack: None,
            installed_inline_sidecar: None,
            pending_inline_sidecar_delivery_ack: None,
            background_reclaim_pending: false,
            inline_sidecar_reclaim_pending: false,
            closing: false,
            closed: false,
        })
    }

    #[must_use]
    pub const fn is_removable(&self) -> bool {
        self.closed
    }

    pub fn observe_source_version(&mut self, source: SourceVersion) -> Result<(), HostStoreError> {
        self.require_open()?;
        if source.document_session != self.config.document_session {
            return Err(HostStoreError::new(
                HostRejectReason::ExactSourceMismatch,
                "source belongs to another document session",
            ));
        }
        if let Some(current) = self.current_source {
            if source.revision < current.revision {
                return Err(HostStoreError::new(
                    HostRejectReason::StaleSource,
                    "source revision moved backward",
                ));
            }
            if source.revision == current.revision {
                return if source == current {
                    Ok(())
                } else {
                    Err(HostStoreError::new(
                        HostRejectReason::ExactSourceMismatch,
                        "same revision changed exact source facts",
                    ))
                };
            }
        }

        if self.active.take().is_some() {
            self.background_reclaim_pending |= self.abort_engine_snapshot()?;
        }
        if self.active_inline_sidecar.take().is_some() {
            self.inline_sidecar_reclaim_pending |= self.abort_inline_sidecar_engine_snapshot()?;
        }
        self.viewport_presentation.invalidate();
        // A strictly newer exact source is stronger authority than either
        // offer-scoped abort handshake. The caller suppresses those stale
        // parser tickets when the edit becomes visible, so no consumer remains
        // for AbortComplete. Preserve the already-started bounded reclamation,
        // but release the offer identities that would otherwise backpressure
        // every replacement publication indefinitely.
        self.aborting_offer = None;
        self.aborting_inline_sidecar_offer = None;
        self.current_source = Some(source);
        Ok(())
    }

    pub fn begin_offer(&mut self, begin: OfferBegin) -> Result<(), HostStoreError> {
        self.require_open()?;
        if self.active.is_some()
            || self.aborting_offer.is_some()
            || self.active_inline_sidecar.is_some()
            || self.aborting_inline_sidecar_offer.is_some()
            || self.viewport_presentation.has_foreground_work()
        {
            return Err(HostStoreError::new(
                HostRejectReason::Backpressure,
                "another offer still owns host work",
            ));
        }
        self.validate_offer(begin)?;
        self.prepare_engine_for(begin)?;
        let exact_base = match begin.mode {
            PublicationMode::FullSnapshot => None,
            PublicationMode::ExactBaseReferencesDelta | PublicationMode::ExactBaseDelta => Some(
                self.engine
                    .as_ref()
                    .and_then(M11CandidateHost::installed)
                    .ok_or_else(|| {
                        HostStoreError::new(
                            HostRejectReason::BaseMismatch,
                            "the exact References base is not installed",
                        )
                    })?,
            ),
        };
        // A distinct, strictly newer full snapshot is the exact recovery path
        // when the old parser died after host commit but before delivery.
        self.pending_delivery_ack = None;
        self.active = Some(ActiveOffer::new(begin, exact_base));
        Ok(())
    }

    pub fn begin_inline_sidecar_offer(
        &mut self,
        begin: HotInlineSidecarBegin,
    ) -> Result<(), HostStoreError> {
        self.require_open()?;
        if self.active.is_some()
            || self.aborting_offer.is_some()
            || self.active_inline_sidecar.is_some()
            || self.aborting_inline_sidecar_offer.is_some()
            || self.viewport_presentation.has_foreground_work()
        {
            return Err(HostStoreError::new(
                HostRejectReason::Backpressure,
                "another structural or sidecar offer still owns host work",
            ));
        }
        self.validate_inline_sidecar_offer(begin)?;
        let engine = self
            .engine
            .as_ref()
            .ok_or_else(|| HostStoreError::new(HostRejectReason::BaseMismatch, "no base engine"))?;
        let installed = engine.installed().ok_or_else(|| {
            HostStoreError::new(
                HostRejectReason::BaseMismatch,
                "hot-inline base lost its installed capability",
            )
        })?;
        let base = engine
            .inline_sidecar_base(installed, begin.binding.parser_profile)
            .map_err(map_engine_error)?;
        if let Some(sidecar) = self.inline_sidecar.as_mut() {
            let changed = sidecar
                .observe_base(base.clone())
                .map_err(map_engine_error)?;
            if changed {
                self.installed_inline_sidecar = None;
                self.pending_inline_sidecar_delivery_ack = None;
                self.inline_sidecar_reclaim_pending = true;
            }
        } else {
            self.inline_sidecar = Some(M11HostInlineSidecar::new(base.clone(), self.engine_limits));
        }
        let owner = match begin
            .binding
            .owner()
            .ok_or_else(|| HostStoreError::invalid("invalid hot-inline owner identity"))?
        {
            HotInlineSidecarOwner::BlockOrdinal(ordinal) => {
                M11HostInlineSidecarOwner::BlockOrdinal(ordinal)
            }
            HotInlineSidecarOwner::RecursiveGreenFrame(frame) => {
                M11HostInlineSidecarOwner::RecursiveGreenFrame(frame)
            }
        };
        let binding = M11HostInlineSidecarBinding::new_for_owner(
            base,
            begin.binding.refinement_generation,
            owner,
            begin.binding.physical_start_utf8,
            begin.binding.physical_end_utf8,
            begin.binding.visible_start_utf8,
            begin.binding.visible_end_utf8,
            begin.binding.physical_start_utf16,
            begin.binding.physical_end_utf16,
            begin.binding.visible_start_utf16,
            begin.binding.visible_end_utf16,
        )
        .map_err(map_engine_error)?;
        self.active_inline_sidecar = Some(ActiveInlineSidecarOffer::new(begin, binding));
        Ok(())
    }

    pub fn begin_viewport_presentation_offer(
        &mut self,
        begin: crate::v3_publication_wire::ViewportPresentationBegin,
    ) -> Result<(), HostStoreError> {
        self.require_open()?;
        if self.active.is_some()
            || self.aborting_offer.is_some()
            || self.active_inline_sidecar.is_some()
            || self.aborting_inline_sidecar_offer.is_some()
        {
            return Err(HostStoreError::new(
                HostRejectReason::Backpressure,
                "another structural or sidecar offer still owns host work",
            ));
        }
        let current = self.current_source.ok_or_else(|| {
            HostStoreError::new(HostRejectReason::NotReady, "no exact source was observed")
        })?;
        let installed_ack = self.installed_ack.ok_or_else(|| {
            HostStoreError::new(
                HostRejectReason::BaseMismatch,
                "viewport offer has no installed structural base",
            )
        })?;
        if self.pending_delivery_ack.is_some() || self.pending_inline_sidecar_delivery_ack.is_some()
        {
            return Err(HostStoreError::new(
                HostRejectReason::Backpressure,
                "the structural or sidecar delivery proof is still pending",
            ));
        }
        let engine = self
            .engine
            .as_ref()
            .ok_or_else(|| HostStoreError::invalid("host engine was not initialized"))?;
        let installed = engine
            .installed()
            .ok_or_else(|| HostStoreError::invalid("installed ACK lost its engine root"))?;
        let base = engine
            .inline_sidecar_base(installed, u64::from(self.config.syntax_profile))
            .map_err(map_engine_error)?;
        self.viewport_presentation.begin(
            begin,
            installed_ack,
            current,
            base,
            self.engine_limits,
            HOST_VIEWPORT_PRESENTATION_MAXIMUM_QUERY_BYTES,
        )
    }

    pub fn admit_viewport_presentation_packet(
        &mut self,
        packet: PublicationPacket<'_>,
    ) -> Result<(), HostStoreError> {
        self.require_open()?;
        self.viewport_presentation.admit_packet(packet)
    }

    pub fn request_viewport_presentation_commit(
        &mut self,
        request: crate::v3_publication_wire::ViewportPresentationCommitRequest,
    ) -> Result<(), HostStoreError> {
        self.require_open()?;
        self.viewport_presentation.request_commit(request)
    }

    pub fn abort_viewport_presentation_offer(
        &mut self,
        offer_id: Id128,
    ) -> Result<(), HostStoreError> {
        self.require_open()?;
        self.viewport_presentation.abort(offer_id)
    }

    pub fn poll_viewport_presentation(
        &mut self,
        grant: HostWorkGrant,
    ) -> Result<HostViewportPresentationPollOutcome, HostStoreError> {
        if self.closed {
            return Ok(HostViewportPresentationPollOutcome::Closed);
        }
        if self.closing {
            return Ok(HostViewportPresentationPollOutcome::Pending);
        }
        self.viewport_presentation.poll(grant)
    }

    pub fn acknowledge_viewport_presentation_delivery(
        &mut self,
        ack: crate::v3_publication_wire::ViewportPresentationAck,
    ) -> Result<(), HostStoreError> {
        self.require_open()?;
        self.viewport_presentation.acknowledge_delivery(ack)
    }

    pub fn query_viewport_presentation(
        &self,
        ack: crate::v3_publication_wire::ViewportPresentationAck,
        maximum_encoded_bytes: u32,
        output: &mut [u8],
    ) -> Result<HostViewportPresentationQueryOutcome, HostStoreError> {
        self.require_open()?;
        let installed_ack = self.installed_ack.ok_or_else(|| {
            HostStoreError::new(
                HostRejectReason::BaseMismatch,
                "viewport query has no installed structural base",
            )
        })?;
        self.viewport_presentation
            .query(ack, installed_ack, maximum_encoded_bytes, output)
    }

    /// Admits one envelope-validated packet with exactly one bounded copy.
    ///
    /// Descriptor sums, per-frame limits, syntax, order, and digests are
    /// deliberately deferred to fuelled polling. Admission therefore remains
    /// constant-time in the packet frame count.
    pub fn admit_packet(&mut self, packet: PublicationPacket<'_>) -> Result<(), HostStoreError> {
        self.require_open()?;
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| HostStoreError::new(HostRejectReason::WrongOffer, "no active offer"))?;
        if active.begin.offer_id != packet.offer_id {
            return Err(HostStoreError::new(
                HostRejectReason::WrongOffer,
                "packet belongs to another offer",
            ));
        }
        if active.phase != OfferPhase::Receiving || active.pending_packet.is_some() {
            return Err(HostStoreError::new(
                HostRejectReason::Backpressure,
                "the prior publication transfer has not returned credit",
            ));
        }

        let limits = active.begin.limits;
        let packet_bytes = u32::try_from(packet.encoded().len()).map_err(|_| {
            HostStoreError::new(
                HostRejectReason::ForegroundBoundExceeded,
                "packet length exceeds the host target",
            )
        })?;
        let next_frame_ordinal = packet.first_frame_ordinal.checked_add(packet.frame_count);
        let next_record_ordinal = packet
            .first_record_ordinal
            .checked_add(packet.aggregate_record_count);
        let next_accepted_frame_bytes = active
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
            || next_record_ordinal.is_none_or(|next| next > active.begin.transferred_record_count)
            || next_accepted_frame_bytes
                .is_none_or(|next| next > limits.maximum_encoded_frame_bytes)
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "packet order or aggregate envelope changed",
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

    pub fn admit_inline_sidecar_packet(
        &mut self,
        packet: PublicationPacket<'_>,
    ) -> Result<(), HostStoreError> {
        self.require_open()?;
        let active = self.active_inline_sidecar.as_mut().ok_or_else(|| {
            HostStoreError::new(HostRejectReason::WrongOffer, "no active hot-inline offer")
        })?;
        if active.begin.offer_id != packet.offer_id {
            return Err(HostStoreError::new(
                HostRejectReason::WrongOffer,
                "sidecar packet belongs to another offer",
            ));
        }
        if active.phase != OfferPhase::Receiving || active.pending_packet.is_some() {
            return Err(HostStoreError::new(
                HostRejectReason::Backpressure,
                "the prior sidecar transfer has not returned credit",
            ));
        }

        let limits = active.begin.limits;
        let packet_bytes = u32::try_from(packet.encoded().len()).map_err(|_| {
            HostStoreError::new(
                HostRejectReason::ForegroundBoundExceeded,
                "sidecar packet length exceeds the host target",
            )
        })?;
        let next_frame_ordinal = packet.first_frame_ordinal.checked_add(packet.frame_count);
        let next_node_ordinal = packet
            .first_record_ordinal
            .checked_add(packet.aggregate_record_count);
        let next_accepted_frame_bytes = active
            .accepted_frame_bytes
            .checked_add(packet.aggregate_frame_bytes);
        if packet.first_frame_ordinal != active.next_frame_ordinal
            || packet.first_record_ordinal != active.accepted_node_count
            || packet.frame_count == 0
            || packet.frame_count > MAXIMUM_PACKET_FRAME_COUNT
            || packet.aggregate_frame_bytes > MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES
            || packet.encoded().len() > MAXIMUM_PACKET_ENCODED_BYTES
            || packet_bytes > limits.maximum_packet_bytes
            || next_frame_ordinal.is_none_or(|next| next > limits.maximum_frame_count)
            || next_node_ordinal
                .is_none_or(|next| next > active.begin.envelope.transferred_node_count)
            || next_accepted_frame_bytes
                .is_none_or(|next| next > limits.maximum_encoded_frame_bytes)
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "sidecar packet order or aggregate envelope changed",
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

    pub fn request_commit(&mut self, request: CommitRequest) -> Result<(), HostStoreError> {
        self.require_open()?;
        let mut active = self
            .active
            .take()
            .ok_or_else(|| HostStoreError::new(HostRejectReason::WrongOffer, "no active offer"))?;
        let result = self.request_commit_inner(&mut active, request);
        if result.is_err() {
            active.phase = OfferPhase::Failed;
            active.pending_packet = None;
            if let Ok(aborted) = self.abort_engine_snapshot() {
                self.background_reclaim_pending |= aborted;
            }
        }
        self.active = Some(active);
        result
    }

    fn request_commit_inner(
        &mut self,
        active: &mut ActiveOffer,
        request: CommitRequest,
    ) -> Result<(), HostStoreError> {
        if request.offer_id != active.begin.offer_id {
            return Err(HostStoreError::new(
                HostRejectReason::WrongOffer,
                "commit belongs to another offer",
            ));
        }
        if active.phase != OfferPhase::AwaitingCommit || active.pending_packet.is_some() {
            return Err(HostStoreError::new(
                HostRejectReason::Invalid,
                "commit arrived before one complete credited snapshot",
            ));
        }
        let transport = active
            .transport
            .as_ref()
            .ok_or_else(|| HostStoreError::invalid("transport digest was already consumed"))?
            .receipt();
        let stream_digest = active
            .canonical_stream_digest256
            .ok_or_else(|| HostStoreError::invalid("snapshot End claim is missing"))?;
        if request.actual_frame_count != transport.frame_count
            || request.actual_encoded_frame_bytes != transport.encoded_frame_bytes
            || request.actual_frame_count != active.next_frame_ordinal
            || request.actual_encoded_frame_bytes != active.accepted_frame_bytes
            || active.accepted_record_count != active.begin.transferred_record_count
            || request.rolling_transport_digest
                != protocol_digest128_from_blake3(
                    ProtocolDigestDomain::CandidateTransport,
                    transport.digest256,
                )
            || request.canonical_stream_digest
                != protocol_digest128_from_blake3(
                    ProtocolDigestDomain::CandidateStream,
                    stream_digest,
                )
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "commit totals or exact digests changed",
            ));
        }
        let end = active
            .retained_end
            .as_ref()
            .ok_or_else(|| HostStoreError::invalid("snapshot End frame is missing"))?;
        let engine = self
            .engine
            .as_mut()
            .ok_or_else(|| HostStoreError::invalid("host engine was not initialized"))?;
        engine
            .finish_snapshot(end.bytes())
            .map_err(map_engine_error)?;
        if engine.active_snapshot_digest256() != Some(stream_digest) {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "engine and transport stream digests disagree",
            ));
        }
        active.retained_end = None;
        active.transport = None;
        active.commit = Some(request);
        active.phase = OfferPhase::Installing;
        Ok(())
    }

    pub fn request_inline_sidecar_commit(
        &mut self,
        request: HotInlineSidecarCommitRequest,
    ) -> Result<(), HostStoreError> {
        self.require_open()?;
        let mut active = self.active_inline_sidecar.take().ok_or_else(|| {
            HostStoreError::new(HostRejectReason::WrongOffer, "no active hot-inline offer")
        })?;
        let result = self.request_inline_sidecar_commit_inner(&mut active, request);
        if result.is_err() {
            active.phase = OfferPhase::Failed;
            active.pending_packet = None;
            if let Ok(aborted) = self.abort_inline_sidecar_engine_snapshot() {
                self.inline_sidecar_reclaim_pending |= aborted;
            }
        }
        self.active_inline_sidecar = Some(active);
        result
    }

    fn request_inline_sidecar_commit_inner(
        &mut self,
        active: &mut ActiveInlineSidecarOffer,
        request: HotInlineSidecarCommitRequest,
    ) -> Result<(), HostStoreError> {
        if request.offer_id != active.begin.offer_id {
            return Err(HostStoreError::new(
                HostRejectReason::WrongOffer,
                "sidecar commit belongs to another offer",
            ));
        }
        if active.phase != OfferPhase::AwaitingCommit || active.pending_packet.is_some() {
            return Err(HostStoreError::new(
                HostRejectReason::Invalid,
                "sidecar commit arrived before one complete credited root",
            ));
        }
        let transport = active
            .transport
            .as_ref()
            .ok_or_else(|| HostStoreError::invalid("sidecar transport digest was consumed"))?
            .receipt();
        let stream_digest = active
            .root_stream_digest256
            .ok_or_else(|| HostStoreError::invalid("sidecar End claim is missing"))?;
        if request.actual_frame_count != transport.frame_count
            || request.actual_encoded_frame_bytes != transport.encoded_frame_bytes
            || request.actual_frame_count != active.next_frame_ordinal
            || request.actual_encoded_frame_bytes != active.accepted_frame_bytes
            || active.accepted_node_count != active.begin.envelope.transferred_node_count
            || request.rolling_transport_digest
                != protocol_digest128_from_blake3(
                    ProtocolDigestDomain::HotInlineSidecarTransport,
                    transport.digest256,
                )
            || request.root_stream_digest
                != protocol_digest128_from_blake3(
                    ProtocolDigestDomain::HotInlineSidecarRootStream,
                    stream_digest,
                )
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "sidecar commit totals or exact digests changed",
            ));
        }
        let end = active
            .retained_end
            .as_ref()
            .ok_or_else(|| HostStoreError::invalid("sidecar End frame is missing"))?;
        self.inline_sidecar
            .as_mut()
            .ok_or_else(|| HostStoreError::invalid("sidecar engine was not initialized"))?
            .finish_snapshot(end.bytes())
            .map_err(map_engine_error)?;
        active.retained_end = None;
        active.transport = None;
        active.commit = Some(request);
        active.phase = OfferPhase::Installing;
        Ok(())
    }

    pub fn abort_offer(&mut self, offer_id: Id128) -> Result<(), HostStoreError> {
        self.require_open()?;
        let active = self
            .active
            .take()
            .ok_or_else(|| HostStoreError::new(HostRejectReason::WrongOffer, "no active offer"))?;
        if active.begin.offer_id != offer_id {
            self.active = Some(active);
            return Err(HostStoreError::new(
                HostRejectReason::WrongOffer,
                "abort belongs to another offer",
            ));
        }
        self.background_reclaim_pending |= self.abort_engine_snapshot()?;
        self.aborting_offer = Some(offer_id);
        Ok(())
    }

    pub fn abort_inline_sidecar_offer(&mut self, offer_id: Id128) -> Result<(), HostStoreError> {
        self.require_open()?;
        let active = self.active_inline_sidecar.take().ok_or_else(|| {
            HostStoreError::new(HostRejectReason::WrongOffer, "no active hot-inline offer")
        })?;
        if active.begin.offer_id != offer_id {
            self.active_inline_sidecar = Some(active);
            return Err(HostStoreError::new(
                HostRejectReason::WrongOffer,
                "sidecar abort belongs to another offer",
            ));
        }
        self.inline_sidecar_reclaim_pending |= self.abort_inline_sidecar_engine_snapshot()?;
        self.aborting_inline_sidecar_offer = Some(offer_id);
        Ok(())
    }

    pub fn poll(&mut self, grant: HostWorkGrant) -> Result<HostPollOutcome, HostStoreError> {
        if self.closed {
            return Ok(HostPollOutcome::Closed);
        }
        if self.closing {
            return self.poll_close(grant.transitions);
        }
        if let Some(offer_id) = self.aborting_offer {
            if grant.transitions == 0 {
                return Ok(HostPollOutcome::Pending);
            }
            let complete = self.engine.as_mut().map_or(Ok(true), |engine| {
                engine
                    .poll_reclaim(grant.transitions as usize)
                    .map_err(map_engine_error)
            })?;
            if complete {
                self.background_reclaim_pending = false;
                self.aborting_offer = None;
                return Ok(HostPollOutcome::AbortComplete { offer_id });
            }
            return Ok(HostPollOutcome::Pending);
        }

        if self
            .active
            .as_ref()
            .is_some_and(|active| active.pending_packet.is_some())
        {
            return self.poll_pending_packet(grant);
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.phase == OfferPhase::Installing)
        {
            return self.poll_install(grant.transitions);
        }
        if grant.transitions > 0 && self.background_reclaim_pending {
            let complete = self.engine.as_mut().map_or(Ok(true), |engine| {
                engine
                    .poll_reclaim(grant.transitions as usize)
                    .map_err(map_engine_error)
            })?;
            self.background_reclaim_pending = !complete;
            return Ok(HostPollOutcome::Pending);
        }
        if grant.transitions > 0 && self.inline_sidecar_reclaim_pending {
            let complete = self.inline_sidecar.as_mut().map_or(Ok(true), |sidecar| {
                sidecar
                    .poll_reclaim(grant.transitions as usize)
                    .map_err(map_engine_error)
            })?;
            self.inline_sidecar_reclaim_pending = !complete;
        }
        Ok(HostPollOutcome::Pending)
    }

    pub fn poll_inline_sidecar(
        &mut self,
        grant: HostWorkGrant,
    ) -> Result<InlineSidecarHostPollOutcome, HostStoreError> {
        if self.closed {
            return Ok(InlineSidecarHostPollOutcome::Closed);
        }
        if self.closing {
            return Ok(InlineSidecarHostPollOutcome::Pending);
        }
        if let Some(offer_id) = self.aborting_inline_sidecar_offer {
            if grant.transitions == 0 {
                return Ok(InlineSidecarHostPollOutcome::Pending);
            }
            let complete = self.inline_sidecar.as_mut().map_or(Ok(true), |sidecar| {
                sidecar
                    .poll_reclaim(grant.transitions as usize)
                    .map_err(map_engine_error)
            })?;
            if complete {
                self.inline_sidecar_reclaim_pending = false;
                self.aborting_inline_sidecar_offer = None;
                return Ok(InlineSidecarHostPollOutcome::AbortComplete { offer_id });
            }
            return Ok(InlineSidecarHostPollOutcome::Pending);
        }
        if self
            .active_inline_sidecar
            .as_ref()
            .is_some_and(|active| active.pending_packet.is_some())
        {
            return self.poll_inline_sidecar_pending_packet(grant);
        }
        if self
            .active_inline_sidecar
            .as_ref()
            .is_some_and(|active| active.phase == OfferPhase::Installing)
        {
            return self.poll_inline_sidecar_install(grant.transitions);
        }
        if grant.transitions > 0 && self.inline_sidecar_reclaim_pending {
            let complete = self.inline_sidecar.as_mut().map_or(Ok(true), |sidecar| {
                sidecar
                    .poll_reclaim(grant.transitions as usize)
                    .map_err(map_engine_error)
            })?;
            self.inline_sidecar_reclaim_pending = !complete;
        }
        Ok(InlineSidecarHostPollOutcome::Pending)
    }

    pub fn acknowledge_delivery(&mut self, ack: StructuralAck) -> Result<(), HostStoreError> {
        self.require_open()?;
        if self.pending_delivery_ack != Some(ack) {
            return Err(HostStoreError::new(
                HostRejectReason::Invalid,
                "delivery proof does not match the installed ACK",
            ));
        }
        self.pending_delivery_ack = None;
        Ok(())
    }

    pub fn acknowledge_inline_sidecar_delivery(
        &mut self,
        ack: InlineSidecarAck,
    ) -> Result<(), HostStoreError> {
        self.require_open()?;
        if self.pending_inline_sidecar_delivery_ack != Some(ack) {
            return Err(HostStoreError::new(
                HostRejectReason::Invalid,
                "sidecar delivery proof does not match the installed ACK",
            ));
        }
        self.pending_inline_sidecar_delivery_ack = None;
        Ok(())
    }

    pub fn query_inline_sidecar(
        &self,
        binding: crate::v3_publication_wire::HotInlineSidecarBinding,
        output: &mut [u8],
    ) -> Result<HostInlineSidecarQueryOutcome, HostStoreError> {
        self.require_open()?;
        let Some(installed) = self.installed_inline_sidecar.as_ref() else {
            return Ok(HostInlineSidecarQueryOutcome::Unavailable);
        };
        if installed.begin.binding != binding
            || installed.ack.base_ack
                != self.installed_ack.ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::BaseMismatch,
                        "sidecar query has no structural base",
                    )
                })?
        {
            return Ok(HostInlineSidecarQueryOutcome::Unavailable);
        }
        let sidecar = self
            .inline_sidecar
            .as_ref()
            .ok_or_else(|| HostStoreError::invalid("installed sidecar engine disappeared"))?;
        match sidecar
            .query(&installed.binding)
            .map_err(map_engine_error)?
        {
            Some(M11HostInlineSidecarQuery::Authoritative {
                descriptor,
                mut cursor,
                link_values,
            }) => {
                let expected_fact_count = match installed.begin.envelope.disposition {
                    HotInlineSidecarDisposition::Authoritative { fact_count, .. } => {
                        usize::try_from(fact_count).map_err(|_| {
                            HostStoreError::invalid("sidecar fact count exceeds this target")
                        })?
                    }
                    HotInlineSidecarDisposition::Unsupported { .. } => {
                        return Err(HostStoreError::new(
                            HostRejectReason::InternalFault,
                            "sidecar engine and wire dispositions disagree",
                        ));
                    }
                };
                let expected_fact_bytes = expected_fact_count
                    .checked_mul(M11_INLINE_FACT_RECORD_BYTES)
                    .ok_or_else(|| HostStoreError::invalid("sidecar fact bytes overflowed"))?;
                let expected_value_bytes =
                    usize::try_from(link_values.encoded_bytes()).map_err(|_| {
                        HostStoreError::invalid("sidecar link-value bytes exceed this target")
                    })?;
                let expected_bytes = expected_fact_bytes
                    .checked_add(expected_value_bytes)
                    .ok_or_else(|| HostStoreError::invalid("sidecar query bytes overflowed"))?;
                if expected_fact_bytes > self.config.maximum_query_bytes as usize
                    || expected_value_bytes > self.config.maximum_query_bytes as usize
                    || expected_bytes > output.len()
                    || expected_bytes > HOST_INLINE_SIDECAR_MAXIMUM_QUERY_BYTES as usize
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::QueryBoundExceeded,
                        "sidecar fact, value, or combined query bound was exceeded",
                    ));
                }
                if descriptor.link_value_entry_count() != link_values.entry_count()
                    || descriptor.link_value_encoded_bytes() != link_values.encoded_bytes()
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "sidecar descriptor and link-value query disagree",
                    ));
                }
                let (encoded_output, _) = output.split_at_mut(expected_bytes);
                let (fact_output, value_output) = encoded_output.split_at_mut(expected_fact_bytes);
                let mut fact_count = 0_usize;
                loop {
                    match cursor.poll().map_err(map_engine_error)? {
                        M11HostInlineProjectionCursorPoll::Fact(fact) => {
                            let start = fact_count
                                .checked_mul(M11_INLINE_FACT_RECORD_BYTES)
                                .ok_or_else(|| {
                                    HostStoreError::invalid("sidecar fact offset overflowed")
                                })?;
                            let end = start.checked_add(M11_INLINE_FACT_RECORD_BYTES).ok_or_else(
                                || HostStoreError::invalid("sidecar fact offset overflowed"),
                            )?;
                            let record = fact_output.get_mut(start..end).ok_or_else(|| {
                                HostStoreError::new(
                                    HostRejectReason::QueryBoundExceeded,
                                    "sidecar query output is too small",
                                )
                            })?;
                            encode_inline_projection_fact_record(fact, record)?;
                            fact_count += 1;
                        }
                        M11HostInlineProjectionCursorPoll::Complete => break,
                    }
                }
                let encoded_bytes = fact_count
                    .checked_mul(M11_INLINE_FACT_RECORD_BYTES)
                    .ok_or_else(|| HostStoreError::invalid("sidecar fact bytes overflowed"))?;
                let value_receipt = link_values.copy(value_output).map_err(map_engine_error)?;
                let tree_nodes_visited = cursor
                    .tree_nodes_visited()
                    .checked_add(value_receipt.tree_nodes_visited)
                    .and_then(|visited| visited.checked_add(1))
                    .ok_or_else(|| HostStoreError::invalid("sidecar query receipt overflowed"))?;
                if fact_count != expected_fact_count
                    || encoded_bytes != expected_fact_bytes
                    || value_receipt.entry_count != link_values.entry_count()
                    || value_output.len() != expected_value_bytes
                    || tree_nodes_visited > descriptor.maximum_tree_nodes_visited()
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "sidecar query disagrees with its authenticated descriptor",
                    ));
                }
                Ok(HostInlineSidecarQueryOutcome::Authoritative {
                    payload_kind: HostInlineSidecarPayloadKind::Inline,
                    fact_count: u32::try_from(fact_count)
                        .map_err(|_| HostStoreError::invalid("sidecar fact count overflowed"))?,
                    value_entry_count: value_receipt.entry_count,
                    value_encoded_bytes: link_values.encoded_bytes(),
                    encoded_bytes: u32::try_from(expected_bytes)
                        .map_err(|_| HostStoreError::invalid("sidecar query bytes overflowed"))?,
                    tree_nodes_visited: u32::try_from(tree_nodes_visited)
                        .map_err(|_| HostStoreError::invalid("sidecar query receipt overflowed"))?,
                })
            }
            Some(M11HostInlineSidecarQuery::ProjectedInline {
                descriptor,
                mut cursor,
            }) => {
                let expected_fact_count = match installed.begin.envelope.disposition {
                    HotInlineSidecarDisposition::Authoritative {
                        fact_count,
                        link_value_entry_count: 0,
                        link_value_encoded_bytes: 0,
                        link_value_storage_page_count: 0,
                        ..
                    } => usize::try_from(fact_count).map_err(|_| {
                        HostStoreError::invalid("projected-inline fact count exceeds this target")
                    })?,
                    _ => {
                        return Err(HostStoreError::new(
                            HostRejectReason::InternalFault,
                            "projected-inline engine and wire dispositions disagree",
                        ));
                    }
                };
                let inline = descriptor.inline();
                let binding = installed.begin.binding;
                if inline.source_start() != binding.physical_start_utf8
                    || inline.source_end() != binding.physical_end_utf8
                    || descriptor.projected_utf8_length()
                        > binding
                            .physical_end_utf8
                            .saturating_sub(binding.physical_start_utf8)
                    || inline.link_value_entry_count() != 0
                    || inline.link_value_encoded_bytes() != 0
                    || inline.link_value_storage_page_count() != 0
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "projected-inline descriptor disagrees with its physical authority",
                    ));
                }
                let expected_bytes = expected_fact_count
                    .checked_mul(M11_INLINE_FACT_RECORD_BYTES)
                    .ok_or_else(|| {
                        HostStoreError::invalid("projected-inline fact bytes overflowed")
                    })?;
                if expected_bytes > output.len()
                    || expected_bytes > self.config.maximum_query_bytes as usize
                    || expected_bytes > HOST_INLINE_SIDECAR_MAXIMUM_QUERY_BYTES as usize
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::QueryBoundExceeded,
                        "projected-inline query output is too small",
                    ));
                }
                let mut fact_count = 0_usize;
                loop {
                    match cursor.poll().map_err(map_engine_error)? {
                        M11HostInlineProjectionCursorPoll::Fact(fact) => {
                            let start = fact_count
                                .checked_mul(M11_INLINE_FACT_RECORD_BYTES)
                                .ok_or_else(|| {
                                    HostStoreError::invalid(
                                        "projected-inline fact offset overflowed",
                                    )
                                })?;
                            let end = start.checked_add(M11_INLINE_FACT_RECORD_BYTES).ok_or_else(
                                || {
                                    HostStoreError::invalid(
                                        "projected-inline fact offset overflowed",
                                    )
                                },
                            )?;
                            encode_inline_projection_fact_record(
                                fact,
                                output.get_mut(start..end).ok_or_else(|| {
                                    HostStoreError::new(
                                        HostRejectReason::QueryBoundExceeded,
                                        "projected-inline query output is too small",
                                    )
                                })?,
                            )?;
                            fact_count += 1;
                        }
                        M11HostInlineProjectionCursorPoll::Complete => break,
                    }
                }
                let tree_nodes_visited =
                    cursor.tree_nodes_visited().checked_add(1).ok_or_else(|| {
                        HostStoreError::invalid("projected-inline query receipt overflowed")
                    })?;
                if fact_count != expected_fact_count
                    || tree_nodes_visited > inline.maximum_tree_nodes_visited()
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "projected-inline query disagrees with its authenticated descriptor",
                    ));
                }
                Ok(HostInlineSidecarQueryOutcome::Authoritative {
                    payload_kind: HostInlineSidecarPayloadKind::ProjectedInline,
                    fact_count: u32::try_from(fact_count).map_err(|_| {
                        HostStoreError::invalid("projected-inline fact count overflowed")
                    })?,
                    value_entry_count: 0,
                    value_encoded_bytes: 0,
                    encoded_bytes: u32::try_from(expected_bytes).map_err(|_| {
                        HostStoreError::invalid("projected-inline query bytes overflowed")
                    })?,
                    tree_nodes_visited: u32::try_from(tree_nodes_visited).map_err(|_| {
                        HostStoreError::invalid("projected-inline query receipt overflowed")
                    })?,
                })
            }
            Some(M11HostInlineSidecarQuery::IndentedCode {
                descriptor,
                mut cursor,
            }) => {
                let (
                    logical_page_count,
                    expected_line_count,
                    storage_page_count,
                    ordered_commitment256,
                ) = match installed.begin.envelope.disposition {
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
                            "sidecar engine and wire dispositions disagree",
                        ));
                    }
                };
                let binding = installed.begin.binding;
                if descriptor.physical_start() != binding.physical_start_utf8
                    || descriptor.physical_end() != binding.physical_end_utf8
                    || descriptor.window_start() != binding.visible_start_utf8
                    || descriptor.window_end() != binding.visible_end_utf8
                    || descriptor.logical_page_count() != logical_page_count
                    || descriptor.line_count() != expected_line_count
                    || descriptor.storage_page_count() != storage_page_count
                    || descriptor.ordered_commitment256() != ordered_commitment256
                    || descriptor.projection_flags() & !1 != 0
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "indented-code sidecar descriptor disagrees with its envelope",
                    ));
                }
                let expected_line_count = usize::try_from(expected_line_count).map_err(|_| {
                    HostStoreError::invalid("indented-code sidecar line count exceeds this target")
                })?;
                let expected_bytes = expected_line_count
                    .checked_mul(M11_INDENTED_CODE_LINE_RECORD_BYTES)
                    .ok_or_else(|| {
                        HostStoreError::invalid("indented-code sidecar query bytes overflowed")
                    })?;
                if expected_bytes > output.len()
                    || expected_bytes > self.config.maximum_query_bytes as usize
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::QueryBoundExceeded,
                        "indented-code sidecar query output is too small",
                    ));
                }
                let mut line_count = 0_usize;
                loop {
                    match cursor.poll().map_err(map_engine_error)? {
                        M11HostIndentedCodeCursorPoll::Line(line) => {
                            let start = line_count
                                .checked_mul(M11_INDENTED_CODE_LINE_RECORD_BYTES)
                                .ok_or_else(|| {
                                    HostStoreError::invalid(
                                        "indented-code sidecar record offset overflowed",
                                    )
                                })?;
                            let end = start
                                .checked_add(M11_INDENTED_CODE_LINE_RECORD_BYTES)
                                .ok_or_else(|| {
                                    HostStoreError::invalid(
                                        "indented-code sidecar record offset overflowed",
                                    )
                                })?;
                            let record = output.get_mut(start..end).ok_or_else(|| {
                                HostStoreError::new(
                                    HostRejectReason::QueryBoundExceeded,
                                    "indented-code sidecar query output is too small",
                                )
                            })?;
                            record[0..4].copy_from_slice(&line.relative_line_start().to_le_bytes());
                            record[4..8]
                                .copy_from_slice(&line.physical_source_length().to_le_bytes());
                            record[8..12]
                                .copy_from_slice(&line.hidden_prefix_length().to_le_bytes());
                            record[12..16].copy_from_slice(&line.content_length().to_le_bytes());
                            record[16..20].copy_from_slice(&line.flags().to_le_bytes());
                            line_count += 1;
                        }
                        M11HostIndentedCodeCursorPoll::Complete => break,
                    }
                }
                let encoded_bytes = line_count
                    .checked_mul(M11_INDENTED_CODE_LINE_RECORD_BYTES)
                    .ok_or_else(|| {
                        HostStoreError::invalid("indented-code sidecar query bytes overflowed")
                    })?;
                if line_count != expected_line_count || encoded_bytes != expected_bytes {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "indented-code sidecar cursor disagrees with its envelope",
                    ));
                }
                Ok(HostInlineSidecarQueryOutcome::Authoritative {
                    payload_kind: HostInlineSidecarPayloadKind::IndentedCode,
                    fact_count: u32::try_from(line_count).map_err(|_| {
                        HostStoreError::invalid("indented-code sidecar line count overflowed")
                    })?,
                    value_entry_count: 0,
                    value_encoded_bytes: 0,
                    encoded_bytes: u32::try_from(encoded_bytes).map_err(|_| {
                        HostStoreError::invalid("indented-code sidecar query bytes overflowed")
                    })?,
                    tree_nodes_visited: u32::try_from(cursor.tree_nodes_visited()).map_err(
                        |_| {
                            HostStoreError::invalid(
                                "indented-code sidecar query receipt overflowed",
                            )
                        },
                    )?,
                })
            }
            Some(M11HostInlineSidecarQuery::BlockQuote { descriptor, cursor }) => {
                query_marked_line_sidecar(
                    descriptor,
                    cursor,
                    installed.begin.envelope.disposition,
                    installed.begin.binding,
                    output,
                    self.config.maximum_query_bytes as usize,
                    HostMarkedLinePayloadKind::BlockQuote,
                )
            }
            Some(M11HostInlineSidecarQuery::BulletList {
                descriptor, cursor, ..
            }) => query_marked_line_sidecar(
                descriptor,
                cursor,
                installed.begin.envelope.disposition,
                installed.begin.binding,
                output,
                self.config.maximum_query_bytes as usize,
                HostMarkedLinePayloadKind::BulletList,
            ),
            Some(M11HostInlineSidecarQuery::OrderedList {
                selected_item_ordinal,
                selected_item_line_ending,
                opening_marker_start,
                opening_marker_end,
                marker_value,
                descriptor,
                cursor,
            }) => query_ordered_list_item_sidecar(
                descriptor,
                cursor,
                installed.begin.envelope.disposition,
                installed.begin.binding,
                output,
                self.config.maximum_query_bytes as usize,
                selected_item_ordinal,
                selected_item_line_ending,
                opening_marker_start,
                opening_marker_end,
                marker_value,
            ),
            Some(M11HostInlineSidecarQuery::Unsupported { metadata }) => {
                if metadata.len() > output.len()
                    || metadata.len() > self.config.maximum_query_bytes as usize
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::QueryBoundExceeded,
                        "sidecar unsupported metadata exceeds the query bound",
                    ));
                }
                output[..metadata.len()].copy_from_slice(metadata);
                let reason = match installed.begin.envelope.disposition {
                    HotInlineSidecarDisposition::Unsupported { reason, .. } => reason,
                    HotInlineSidecarDisposition::Authoritative { .. } => {
                        return Err(HostStoreError::new(
                            HostRejectReason::InternalFault,
                            "sidecar engine and wire dispositions disagree",
                        ));
                    }
                };
                Ok(HostInlineSidecarQueryOutcome::Unsupported {
                    reason,
                    metadata_bytes: u32::try_from(metadata.len()).map_err(|_| {
                        HostStoreError::invalid("sidecar metadata length overflowed")
                    })?,
                })
            }
            None => Ok(HostInlineSidecarQueryOutcome::Unavailable),
        }
    }

    fn inline_sidecar_matches_block(
        &self,
        location: &M11HostPersistentBlockLocation,
        range: HostMetricRange,
        query: HostPointQuery,
    ) -> bool {
        let Some(installed) = self.installed_inline_sidecar.as_ref() else {
            return false;
        };
        let binding = installed.begin.binding;
        self.installed_ack == Some(installed.ack.base_ack)
            && binding.parser_profile == u64::from(self.config.syntax_profile)
            && binding.owner()
                == Some(HotInlineSidecarOwner::BlockOrdinal(
                    location.entry_ordinal(),
                ))
            && binding.physical_start_utf8 == range.start.bytes
            && binding.physical_end_utf8 == range.end.bytes
            && binding.physical_start_utf16 == range.start.utf16
            && binding.physical_end_utf16 == range.end.utf16
            && binding.visible_start_utf8 >= binding.physical_start_utf8
            && binding.visible_end_utf8 <= binding.physical_end_utf8
            && binding.visible_start_utf16 >= binding.physical_start_utf16
            && binding.visible_end_utf16 <= binding.physical_end_utf16
            && point_selects_sidecar_window(binding, query)
    }

    #[cfg(test)]
    pub fn role_record_count(&self, role: M11HostRole) -> Result<u64, HostStoreError> {
        let (engine, installed) = self.query_root()?;
        engine
            .role_record_count(installed, role)
            .map_err(map_engine_error)
    }

    /// Authors one exact-current bounded structural viewport into caller-owned
    /// scratch. Persistent block roles route the point through their measured
    /// tree and return one exact coverage entry; older single-root candidates
    /// retain the flat Green/Projection path without changing the Dart query
    /// model or admitting Dart-side role scans.
    pub fn query_structural(
        &self,
        query: HostPointQuery,
        output: &mut [u8],
    ) -> Result<HostStructuralQueryOutcome, HostStoreError> {
        self.validate_point_query(query)?;
        let (engine, installed) = self.query_root()?;
        if let Some(descriptor) = engine
            .persistent_recursive_green_descriptor(installed)
            .map_err(map_engine_error)?
        {
            return self.query_recursive_green(query, output, engine, installed, descriptor);
        }
        if let Some(descriptor) = engine
            .persistent_block_descriptor(installed)
            .map_err(map_engine_error)?
        {
            return self.query_persistent_blocks(query, output, engine, installed, descriptor);
        }
        self.query_structural_legacy(query, output)
    }

    /// Authors one bounded, point-free page of exact-current top-level block
    /// structure. Inline sidecars remain on the selected-leaf point path.
    pub fn query_structural_range(
        &self,
        query: HostBlockRangeQuery,
        output: &mut [u8],
    ) -> Result<HostBlockRangeOutcome, HostStoreError> {
        self.validate_block_range_query(query)?;
        let (engine, installed) = self.query_root()?;
        if let Some(descriptor) = engine
            .persistent_recursive_green_descriptor(installed)
            .map_err(map_engine_error)?
        {
            return self.query_recursive_green_range(query, output, engine, installed, descriptor);
        }
        let Some(descriptor) = engine
            .persistent_block_descriptor(installed)
            .map_err(map_engine_error)?
        else {
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::UnavailableFacts,
                HostBlockRangeReceipt::default(),
            ));
        };
        if descriptor.source_bytes() != u64::from(query.source_version.utf8_length)
            || descriptor.source_utf16() != u64::from(query.source_version.utf16_length)
            || descriptor.entry_count() > u64::from(u32::MAX)
            || (descriptor.entry_count() == 0)
                != (descriptor.source_bytes() == 0
                    && descriptor.source_utf16() == 0
                    && descriptor.storage_page_count() == 0
                    && descriptor.tree_height() == 0)
        {
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::UndecodableClosure,
                HostBlockRangeReceipt::default(),
            ));
        }

        let minimum_encoded_bytes = HOST_BLOCK_RANGE_HEADER_BYTES
            + usize::from(descriptor.entry_count() != 0) * HOST_BLOCK_RANGE_RECORD_BYTES;
        let admitted_encoded_bytes = usize::try_from(query.budget.maximum_encoded_bytes)
            .unwrap_or(usize::MAX)
            .min(self.config.maximum_query_bytes as usize)
            .min(output.len());
        if admitted_encoded_bytes < minimum_encoded_bytes {
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::EncodedByteLimit,
                HostBlockRangeReceipt::default(),
            ));
        }
        let open_depth = u32::from(descriptor.tree_height()).max(1);
        if query.budget.maximum_open_depth < open_depth {
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::OpenDepthLimit,
                HostBlockRangeReceipt::default(),
            ));
        }

        if descriptor.entry_count() == 0 {
            encode_block_range_header(&mut output[..HOST_BLOCK_RANGE_HEADER_BYTES], 0, true);
            return Ok(HostBlockRangeOutcome::Page {
                source_version: query.source_version,
                requested_range: query.requested_range,
                covered_range: query.requested_range,
                continuation: None,
                receipt: HostBlockRangeReceipt {
                    encoded_bytes: HOST_BLOCK_RANGE_HEADER_BYTES as u32,
                    open_depth,
                    complete: true,
                    ..HostBlockRangeReceipt::default()
                },
            });
        }

        let maximum_output_blocks = (admitted_encoded_bytes - HOST_BLOCK_RANGE_HEADER_BYTES)
            / HOST_BLOCK_RANGE_RECORD_BYTES;
        let maximum_entries = u32::try_from(maximum_output_blocks)
            .unwrap_or(u32::MAX)
            .min(query.budget.maximum_block_count);
        if maximum_entries == 0 {
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::EncodedByteLimit,
                HostBlockRangeReceipt::default(),
            ));
        }

        // An initial query authenticates one packed leaf while locating the
        // block intersecting the requested start. The consecutive visitor
        // then authenticates its own bounded pages. Charge both operations to
        // the public storage-page budget even though the first visitor page is
        // normally the same physical page as the point lookup.
        let point_storage_pages = u32::from(query.continuation.is_none());
        let maximum_visit_storage_pages = query
            .budget
            .maximum_storage_pages_visited
            .checked_sub(point_storage_pages)
            .unwrap_or(0);
        if maximum_visit_storage_pages == 0 {
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::LeafLimit,
                HostBlockRangeReceipt::default(),
            ));
        }
        let maximum_point_nodes = if query.continuation.is_none() {
            descriptor.maximum_tree_nodes_visited()
        } else {
            0
        };
        let maximum_visit_nodes = descriptor
            .maximum_consecutive_visit_node_headers(maximum_visit_storage_pages)
            .checked_add(maximum_point_nodes)
            .ok_or_else(|| HostStoreError::invalid("range query work bound overflowed"))?;
        if maximum_visit_nodes > u64::from(query.budget.maximum_tree_nodes_visited) {
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::TreeNodeLimit,
                HostBlockRangeReceipt::default(),
            ));
        }

        let mut point_nodes = 0_u64;
        let mut point_summaries = 0_u64;
        let mut point_entries_inspected = 0_u64;
        let visit_start = match query.continuation {
            Some(continuation) => decode_block_range_continuation(
                continuation,
                query,
                descriptor,
                installed,
                self.installed_ack
                    .ok_or_else(|| HostStoreError::invalid("installed range ACK disappeared"))?,
            )?,
            None => {
                let Some(location) = engine
                    .persistent_block_point(
                        installed,
                        u64::from(query.requested_range.start.bytes),
                        u64::from(query.requested_range.start.utf16),
                        M11HostBlockAffinity::After,
                    )
                    .map_err(map_engine_error)?
                else {
                    return Ok(block_range_gap(
                        query,
                        HostSourceGapReason::UndecodableClosure,
                        HostBlockRangeReceipt::default(),
                    ));
                };
                let receipt = location.receipt();
                point_nodes = receipt.node_headers_decoded();
                point_summaries = receipt.summary_combinations();
                point_entries_inspected = receipt.entries_authenticated();
                M11HostPersistentBlockVisitStart::new(
                    location.entry_ordinal(),
                    location.byte_start(),
                    location.utf16_start(),
                )
            }
        };

        let mut malformed = false;
        let mut reached_requested_end = false;
        let mut expected_ordinal = visit_start.entry_ordinal();
        let mut covered_start = None;
        let mut covered_end = None;
        let mut block_count = 0_usize;
        let visit = engine
            .visit_persistent_blocks(
                installed,
                visit_start,
                maximum_entries,
                maximum_visit_storage_pages,
                |entry| {
                    if malformed {
                        return M11HostPersistentBlockVisitControl::Stop;
                    }
                    if entry.entry_ordinal() != expected_ordinal {
                        malformed = true;
                        return M11HostPersistentBlockVisitControl::Stop;
                    }
                    let Some(range) = persistent_block_visit_range(query.source_version, entry)
                    else {
                        malformed = true;
                        return M11HostPersistentBlockVisitControl::Stop;
                    };
                    let byte_start_reached_end =
                        range.start.bytes >= query.requested_range.end.bytes;
                    let utf16_start_reached_end =
                        range.start.utf16 >= query.requested_range.end.utf16;
                    if byte_start_reached_end != utf16_start_reached_end {
                        malformed = true;
                        return M11HostPersistentBlockVisitControl::Stop;
                    }
                    if byte_start_reached_end {
                        reached_requested_end = true;
                        return M11HostPersistentBlockVisitControl::Stop;
                    }
                    if covered_start.is_none() {
                        let byte_before_start =
                            range.end.bytes <= query.requested_range.start.bytes;
                        let utf16_before_start =
                            range.end.utf16 <= query.requested_range.start.utf16;
                        if byte_before_start != utf16_before_start || byte_before_start {
                            malformed = true;
                            return M11HostPersistentBlockVisitControl::Stop;
                        }
                    }
                    let mut green = [0_u8; M11_GREEN_RECORD_BYTES];
                    let mut projection = [0_u8; M11_PROJECTION_RECORD_BYTES];
                    if !persistent_block_records(&entry, range, &mut green, &mut projection) {
                        malformed = true;
                        return M11HostPersistentBlockVisitControl::Stop;
                    }
                    let mut record = [0_u8; HOST_BLOCK_RANGE_RECORD_BYTES];
                    encode_block_range_record(
                        &mut record,
                        entry.entry_ordinal(),
                        range,
                        &green,
                        &projection,
                    );
                    let record_start =
                        HOST_BLOCK_RANGE_HEADER_BYTES + block_count * HOST_BLOCK_RANGE_RECORD_BYTES;
                    let record_end = record_start + HOST_BLOCK_RANGE_RECORD_BYTES;
                    output[record_start..record_end].copy_from_slice(&record);
                    block_count += 1;
                    covered_start.get_or_insert(range.start);
                    covered_end = Some(range.end);
                    match expected_ordinal.checked_add(1) {
                        Some(next) => expected_ordinal = next,
                        None => {
                            malformed = true;
                            return M11HostPersistentBlockVisitControl::Stop;
                        }
                    }
                    let byte_end_reached = range.end.bytes >= query.requested_range.end.bytes;
                    let utf16_end_reached = range.end.utf16 >= query.requested_range.end.utf16;
                    if byte_end_reached != utf16_end_reached {
                        malformed = true;
                        return M11HostPersistentBlockVisitControl::Stop;
                    }
                    if byte_end_reached {
                        reached_requested_end = true;
                        M11HostPersistentBlockVisitControl::Stop
                    } else {
                        M11HostPersistentBlockVisitControl::Continue
                    }
                },
            )
            .map_err(map_engine_error)?
            .ok_or_else(|| HostStoreError::invalid("persistent block range root disappeared"))?;

        let tree_nodes_visited = point_nodes
            .checked_add(visit.node_headers_decoded())
            .ok_or_else(|| HostStoreError::invalid("range query tree receipt overflowed"))?;
        let summary_nodes_skipped = point_summaries
            .checked_add(visit.summary_combinations())
            .ok_or_else(|| HostStoreError::invalid("range query summary receipt overflowed"))?;
        let packed_entries_inspected = point_entries_inspected
            .checked_add(visit.entries_authenticated())
            .ok_or_else(|| HostStoreError::invalid("range query entry receipt overflowed"))?;
        let storage_pages_visited = u64::from(point_storage_pages)
            .checked_add(visit.storage_pages_visited())
            .ok_or_else(|| HostStoreError::invalid("range query page receipt overflowed"))?;
        if malformed
            || block_count == 0
            || u64::try_from(block_count).map_or(true, |count| count > visit.visited_entries())
            || storage_pages_visited > u64::from(query.budget.maximum_storage_pages_visited)
            || tree_nodes_visited > u64::from(query.budget.maximum_tree_nodes_visited)
        {
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::UndecodableClosure,
                HostBlockRangeReceipt {
                    storage_pages_visited: u32::try_from(storage_pages_visited).unwrap_or(u32::MAX),
                    open_depth,
                    tree_nodes_visited: u32::try_from(tree_nodes_visited).unwrap_or(u32::MAX),
                    packed_entries_inspected: u32::try_from(packed_entries_inspected)
                        .unwrap_or(u32::MAX),
                    summary_nodes_skipped: u32::try_from(summary_nodes_skipped).unwrap_or(u32::MAX),
                    ..HostBlockRangeReceipt::default()
                },
            ));
        }
        let complete = reached_requested_end
            || visit.disposition() == M11HostPersistentBlockVisitDisposition::Complete;
        if !complete
            && visit.disposition() == M11HostPersistentBlockVisitDisposition::VisitorStopped
        {
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::UndecodableClosure,
                HostBlockRangeReceipt::default(),
            ));
        }
        let covered_range = HostMetricRange {
            start: covered_start.expect("nonempty range page has a start"),
            end: covered_end.expect("nonempty range page has an end"),
        };
        let continuation = if complete {
            None
        } else {
            Some(encode_block_range_continuation(
                query,
                installed,
                self.installed_ack
                    .ok_or_else(|| HostStoreError::invalid("installed range ACK disappeared"))?,
                visit.next_entry_ordinal(),
                visit.next_byte_offset(),
                visit.next_utf16_offset(),
            )?)
        };
        let encoded_bytes = HOST_BLOCK_RANGE_HEADER_BYTES
            .checked_add(
                block_count
                    .checked_mul(HOST_BLOCK_RANGE_RECORD_BYTES)
                    .ok_or_else(|| HostStoreError::invalid("range page length overflowed"))?,
            )
            .ok_or_else(|| HostStoreError::invalid("range page length overflowed"))?;
        encode_block_range_header(
            &mut output[..HOST_BLOCK_RANGE_HEADER_BYTES],
            u32::try_from(block_count)
                .map_err(|_| HostStoreError::invalid("range block count overflowed"))?,
            complete,
        );
        Ok(HostBlockRangeOutcome::Page {
            source_version: query.source_version,
            requested_range: query.requested_range,
            covered_range,
            continuation,
            receipt: HostBlockRangeReceipt {
                encoded_bytes: u32::try_from(encoded_bytes)
                    .map_err(|_| HostStoreError::invalid("range page length overflowed"))?,
                block_count: u32::try_from(block_count)
                    .map_err(|_| HostStoreError::invalid("range block count overflowed"))?,
                storage_pages_visited: u32::try_from(storage_pages_visited)
                    .map_err(|_| HostStoreError::invalid("range page receipt overflowed"))?,
                open_depth,
                tree_nodes_visited: u32::try_from(tree_nodes_visited)
                    .map_err(|_| HostStoreError::invalid("range page receipt overflowed"))?,
                packed_entries_inspected: u32::try_from(packed_entries_inspected)
                    .map_err(|_| HostStoreError::invalid("range page receipt overflowed"))?,
                summary_nodes_skipped: u32::try_from(summary_nodes_skipped)
                    .map_err(|_| HostStoreError::invalid("range page receipt overflowed"))?,
                complete,
            },
        })
    }

    fn query_recursive_green_range(
        &self,
        query: HostBlockRangeQuery,
        output: &mut [u8],
        engine: &M11CandidateHost,
        installed: M11HostInstalledCandidate,
        descriptor: M11HostPersistentRecursiveGreenDescriptor,
    ) -> Result<HostBlockRangeOutcome, HostStoreError> {
        if descriptor.source_bytes() != u64::from(query.source_version.utf8_length)
            || descriptor.source_utf16() != u64::from(query.source_version.utf16_length)
            || descriptor.event_count() == 0
            || descriptor.renderable_row_count() == 0
        {
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::UndecodableClosure,
                HostBlockRangeReceipt::default(),
            ));
        }
        if query.continuation.is_some() {
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::UnavailableFacts,
                HostBlockRangeReceipt::default(),
            ));
        }
        let admitted_encoded_bytes = usize::try_from(query.budget.maximum_encoded_bytes)
            .unwrap_or(usize::MAX)
            .min(self.config.maximum_query_bytes as usize)
            .min(output.len());
        if admitted_encoded_bytes < HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES {
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::EncodedByteLimit,
                HostBlockRangeReceipt::default(),
            ));
        }
        let maximum_events_scanned = u64::from(query.budget.maximum_storage_pages_visited)
            .checked_mul(128)
            .ok_or_else(|| HostStoreError::invalid("Green row event budget overflowed"))?;
        let Some(outcome) = engine
            .persistent_recursive_green_rows(
                installed,
                u64::from(query.requested_range.start.bytes),
                u64::from(query.requested_range.start.utf16),
                u64::from(query.requested_range.end.bytes),
                query.budget.maximum_block_count,
                u64::from(query.budget.maximum_storage_pages_visited),
                maximum_events_scanned,
                usize::try_from(query.budget.maximum_open_depth)
                    .map_err(|_| HostStoreError::invalid("Green row depth exceeds this target"))?,
                u64::from(query.budget.maximum_tree_nodes_visited),
            )
            .map_err(map_engine_error)?
        else {
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::UnavailableFacts,
                HostBlockRangeReceipt::default(),
            ));
        };
        let window = match outcome {
            M11HostRecursiveGreenRowQueryOutcome::Window(window) => window,
            M11HostRecursiveGreenRowQueryOutcome::BudgetExceeded(exceeded) => {
                let reason = match exceeded.limit() {
                    M11HostRecursiveGreenRowQueryLimit::StoragePages
                    | M11HostRecursiveGreenRowQueryLimit::EventsScanned => {
                        HostSourceGapReason::LeafLimit
                    }
                    M11HostRecursiveGreenRowQueryLimit::TreeNodes => {
                        HostSourceGapReason::TreeNodeLimit
                    }
                    M11HostRecursiveGreenRowQueryLimit::OpenDepth => {
                        HostSourceGapReason::OpenDepthLimit
                    }
                };
                return Ok(block_range_gap(
                    query,
                    reason,
                    // The engine retains exact overrun telemetry, but this
                    // wire receipt describes admitted bounded work and cannot
                    // carry counters beyond the caller's declared envelope.
                    HostBlockRangeReceipt::default(),
                ));
            }
        };
        let row_count = window.row_count();
        if !window.complete() {
            // RecursiveGreen does not mint structural-range continuations.
            // A bounded row quantum that cannot prove the entire requested
            // cut therefore fails closed instead of returning a partial page.
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::UnavailableFacts,
                HostBlockRangeReceipt::default(),
            ));
        }
        let row_extent = match (window.row(0), window.row(row_count.saturating_sub(1))) {
            (Some(first), Some(last)) => HostMetricRange {
                start: HostSourceMetric {
                    bytes: u32::try_from(first.byte_start())
                        .map_err(|_| HostStoreError::invalid("Green row span exceeds wire"))?,
                    utf16: u32::try_from(first.utf16_start())
                        .map_err(|_| HostStoreError::invalid("Green row span exceeds wire"))?,
                },
                end: HostSourceMetric {
                    bytes: u32::try_from(last.byte_end())
                        .map_err(|_| HostStoreError::invalid("Green row span exceeds wire"))?,
                    utf16: u32::try_from(last.utf16_end())
                        .map_err(|_| HostStoreError::invalid("Green row span exceeds wire"))?,
                },
            },
            _ => query.requested_range,
        };
        // Coverage is the authenticated convex cut, not a claim that every
        // source byte belongs to a render row. This retains blank separators
        // between the final row and the next ordinal boundary.
        let covered_range = HostMetricRange {
            start: HostSourceMetric {
                bytes: row_extent
                    .start
                    .bytes
                    .min(query.requested_range.start.bytes),
                utf16: row_extent
                    .start
                    .utf16
                    .min(query.requested_range.start.utf16),
            },
            end: HostSourceMetric {
                bytes: row_extent.end.bytes.max(query.requested_range.end.bytes),
                utf16: row_extent.end.utf16.max(query.requested_range.end.utf16),
            },
        };
        let mut path_count = 0_usize;
        for index in 0..row_count {
            path_count = path_count
                .checked_add(
                    window
                        .row(index)
                        .ok_or_else(|| HostStoreError::invalid("Green row disappeared"))?
                        .path_len(),
                )
                .ok_or_else(|| HostStoreError::invalid("Green row path count overflowed"))?;
        }
        let encoded_bytes = HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES
            .checked_add(
                row_count
                    .checked_mul(HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES)
                    .ok_or_else(|| HostStoreError::invalid("Green row bytes overflowed"))?,
            )
            .and_then(|bytes| {
                path_count
                    .checked_mul(HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES)
                    .and_then(|paths| bytes.checked_add(paths))
            })
            .ok_or_else(|| HostStoreError::invalid("Green row bytes overflowed"))?;
        if encoded_bytes > admitted_encoded_bytes {
            return Ok(block_range_gap(
                query,
                HostSourceGapReason::EncodedByteLimit,
                HostBlockRangeReceipt::default(),
            ));
        }
        let ack = self
            .installed_ack
            .ok_or_else(|| HostStoreError::invalid("installed Green row ACK disappeared"))?;
        let selected_row_index = (!query.continuation.is_some() && row_count != 0).then_some(0_u32);
        encode_recursive_green_row_header(
            &mut output[..HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES],
            u32::try_from(row_count)
                .map_err(|_| HostStoreError::invalid("Green row count exceeds wire"))?,
            u32::try_from(path_count)
                .map_err(|_| HostStoreError::invalid("Green path count exceeds wire"))?,
            true,
            selected_row_index,
            window.start_ordinal(),
            window.total_rows(),
            ack,
        );
        let mut next_path = 0_u32;
        for index in 0..row_count {
            let row = window
                .row(index)
                .ok_or_else(|| HostStoreError::invalid("Green row disappeared"))?;
            let row_offset = HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES
                + index * HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES;
            let row_output: &mut [u8; HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES] = (&mut output
                [row_offset..row_offset + HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES])
                .try_into()
                .expect("fixed Green row record");
            encode_recursive_green_row_record(
                row_output,
                row,
                next_path,
                selected_row_index == u32::try_from(index).ok(),
            )?;
            for path_index in 0..row.path_len() {
                let path = row
                    .path(path_index)
                    .ok_or_else(|| HostStoreError::invalid("Green row path disappeared"))?;
                let path_ordinal = usize::try_from(next_path)
                    .map_err(|_| HostStoreError::invalid("Green path ordinal overflowed"))?;
                let path_offset = HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES
                    + row_count * HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES
                    + path_ordinal * HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES;
                let path_output: &mut [u8; HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES] =
                    (&mut output
                        [path_offset..path_offset + HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES])
                        .try_into()
                        .expect("fixed Green path record");
                encode_recursive_green_path_record(
                    path_output,
                    path,
                    path_index + 1 == row.path_len(),
                )?;
                next_path = next_path
                    .checked_add(1)
                    .ok_or_else(|| HostStoreError::invalid("Green path ordinal overflowed"))?;
            }
        }
        Ok(HostBlockRangeOutcome::Page {
            source_version: query.source_version,
            requested_range: query.requested_range,
            covered_range,
            continuation: None,
            receipt: HostBlockRangeReceipt {
                encoded_bytes: u32::try_from(encoded_bytes)
                    .map_err(|_| HostStoreError::invalid("Green row bytes exceed wire"))?,
                block_count: u32::try_from(row_count)
                    .map_err(|_| HostStoreError::invalid("Green row count exceeds wire"))?,
                storage_pages_visited: u32::try_from(window.storage_pages_visited())
                    .map_err(|_| HostStoreError::invalid("Green row receipt exceeds wire"))?,
                open_depth: u32::try_from(window.maximum_open_depth())
                    .map_err(|_| HostStoreError::invalid("Green row receipt exceeds wire"))?,
                tree_nodes_visited: u32::try_from(window.node_headers_decoded())
                    .map_err(|_| HostStoreError::invalid("Green row receipt exceeds wire"))?,
                packed_entries_inspected: u32::try_from(window.events_scanned())
                    .map_err(|_| HostStoreError::invalid("Green row receipt exceeds wire"))?,
                summary_nodes_skipped: u32::try_from(window.summary_combinations())
                    .map_err(|_| HostStoreError::invalid("Green row receipt exceeds wire"))?,
                // This page proved that its rows cover the exact requested
                // source cut and RecursiveGreen does not mint continuations.
                complete: true,
            },
        })
    }

    /// Locates one exact top-level structural ordinal window without scanning
    /// the source prefix or copying block records.
    ///
    /// The installed measured sequence derives both UTF-8/UTF-16 cuts. At
    /// most two bounded packed pages are authenticated regardless of the
    /// requested ordinal or the number of entries skipped by the window.
    pub fn query_structural_ordinal_window(
        &self,
        query: HostStructuralOrdinalWindowQuery,
    ) -> Result<HostStructuralOrdinalWindowOutcome, HostStoreError> {
        self.validate_structural_ordinal_window_query(query)?;
        let (engine, installed) = self.query_root()?;
        if let Some(descriptor) = engine
            .persistent_recursive_green_descriptor(installed)
            .map_err(map_engine_error)?
        {
            return self.query_recursive_green_ordinal_window(query, engine, installed, descriptor);
        }
        let Some(descriptor) = engine
            .persistent_block_descriptor(installed)
            .map_err(map_engine_error)?
        else {
            return Ok(structural_ordinal_window_failure(
                query,
                0,
                HostStructuralOrdinalWindowFailureReason::UnavailableFacts,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        };
        if descriptor.source_bytes() != u64::from(query.source_version.utf8_length)
            || descriptor.source_utf16() != u64::from(query.source_version.utf16_length)
            || (descriptor.entry_count() == 0)
                != (descriptor.source_bytes() == 0
                    && descriptor.source_utf16() == 0
                    && descriptor.storage_page_count() == 0
                    && descriptor.tree_height() == 0)
        {
            return Ok(structural_ordinal_window_failure(
                query,
                0,
                HostStructuralOrdinalWindowFailureReason::UndecodableClosure,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        }
        let total_entry_count = descriptor.entry_count();
        if query.budget.maximum_entries == 0
            || query.budget.maximum_entries > HOST_STRUCTURAL_ORDINAL_WINDOW_MAXIMUM_ENTRIES
        {
            return Ok(structural_ordinal_window_failure(
                query,
                total_entry_count,
                HostStructuralOrdinalWindowFailureReason::EntryWindowLimit,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        }
        if query.start_entry_ordinal > total_entry_count {
            return Ok(structural_ordinal_window_failure(
                query,
                total_entry_count,
                HostStructuralOrdinalWindowFailureReason::OrdinalOutOfRange,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        }

        let next_entry_ordinal = query
            .start_entry_ordinal
            .checked_add(u64::from(query.budget.maximum_entries))
            .unwrap_or(u64::MAX)
            .min(total_entry_count);
        let internal_boundary_count =
            u64::from(
                query.start_entry_ordinal > 0 && query.start_entry_ordinal < total_entry_count,
            ) + u64::from(next_entry_ordinal > 0 && next_entry_ordinal < total_entry_count);
        let maximum_tree_nodes = descriptor
            .maximum_tree_nodes_visited()
            .checked_mul(internal_boundary_count)
            .ok_or_else(|| HostStoreError::invalid("ordinal-window tree bound overflowed"))?;
        // Each metric seek authenticates no more than six bounded packed leaf
        // headers in the checked AVL path, including the explicit selected
        // page decode. One additional bounded prefix scan derives its exact
        // local cut.
        let maximum_packed_entries = u64::from(descriptor.maximum_entries_scanned())
            .checked_mul(7)
            .and_then(|bound| bound.checked_mul(internal_boundary_count))
            .ok_or_else(|| HostStoreError::invalid("ordinal-window entry bound overflowed"))?;
        if internal_boundary_count > u64::from(query.budget.maximum_storage_pages_visited) {
            return Ok(structural_ordinal_window_failure(
                query,
                total_entry_count,
                HostStructuralOrdinalWindowFailureReason::StoragePageLimit,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        }
        if maximum_tree_nodes > u64::from(query.budget.maximum_tree_nodes_visited) {
            return Ok(structural_ordinal_window_failure(
                query,
                total_entry_count,
                HostStructuralOrdinalWindowFailureReason::TreeNodeLimit,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        }
        if maximum_packed_entries > u64::from(query.budget.maximum_packed_entries_inspected) {
            return Ok(structural_ordinal_window_failure(
                query,
                total_entry_count,
                HostStructuralOrdinalWindowFailureReason::PackedEntryLimit,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        }

        let Some(window) = engine
            .persistent_block_ordinal_window(
                installed,
                query.start_entry_ordinal,
                query.budget.maximum_entries,
            )
            .map_err(map_engine_error)?
        else {
            return Ok(structural_ordinal_window_failure(
                query,
                0,
                HostStructuralOrdinalWindowFailureReason::UnavailableFacts,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        };
        let receipt = structural_ordinal_window_receipt(window)?;
        if receipt.storage_pages_visited > query.budget.maximum_storage_pages_visited
            || receipt.tree_nodes_visited > query.budget.maximum_tree_nodes_visited
            || receipt.packed_entries_inspected > query.budget.maximum_packed_entries_inspected
        {
            return Err(HostStoreError::new(
                HostRejectReason::InternalFault,
                "ordinal-window engine exceeded its admitted work",
            ));
        }
        let start = structural_ordinal_window_metric(
            window.start_byte_offset(),
            window.start_utf16_offset(),
        )?;
        let next = structural_ordinal_window_metric(
            window.next_byte_offset(),
            window.next_utf16_offset(),
        )?;
        if window.total_entry_count() != total_entry_count
            || window.start_entry_ordinal() != query.start_entry_ordinal
            || window.next_entry_ordinal() != next_entry_ordinal
            || window.complete() != (next_entry_ordinal == total_entry_count)
            || start.bytes > next.bytes
            || start.utf16 > next.utf16
            || next.bytes > query.source_version.utf8_length
            || next.utf16 > query.source_version.utf16_length
            || (next_entry_ordinal == total_entry_count)
                != (next.bytes == query.source_version.utf8_length
                    && next.utf16 == query.source_version.utf16_length)
        {
            return Ok(structural_ordinal_window_failure(
                query,
                0,
                HostStructuralOrdinalWindowFailureReason::UndecodableClosure,
                receipt,
            ));
        }
        Ok(HostStructuralOrdinalWindowOutcome::Window {
            source_version: query.source_version,
            total_entry_count,
            start_entry_ordinal: query.start_entry_ordinal,
            next_entry_ordinal,
            start,
            next,
            complete: window.complete(),
            receipt,
        })
    }

    fn query_recursive_green_ordinal_window(
        &self,
        query: HostStructuralOrdinalWindowQuery,
        engine: &M11CandidateHost,
        installed: M11HostInstalledCandidate,
        descriptor: M11HostPersistentRecursiveGreenDescriptor,
    ) -> Result<HostStructuralOrdinalWindowOutcome, HostStoreError> {
        if descriptor.source_bytes() != u64::from(query.source_version.utf8_length)
            || descriptor.source_utf16() != u64::from(query.source_version.utf16_length)
            || descriptor.event_count() == 0
            || descriptor.storage_page_count() == 0
            || descriptor.tree_height() == 0
        {
            return Ok(structural_ordinal_window_failure(
                query,
                0,
                HostStructuralOrdinalWindowFailureReason::UndecodableClosure,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        }
        let total_entry_count = descriptor.renderable_row_count();
        if query.budget.maximum_entries == 0
            || query.budget.maximum_entries > HOST_STRUCTURAL_ORDINAL_WINDOW_MAXIMUM_ENTRIES
        {
            return Ok(structural_ordinal_window_failure(
                query,
                total_entry_count,
                HostStructuralOrdinalWindowFailureReason::EntryWindowLimit,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        }
        if query.start_entry_ordinal > total_entry_count {
            return Ok(structural_ordinal_window_failure(
                query,
                total_entry_count,
                HostStructuralOrdinalWindowFailureReason::OrdinalOutOfRange,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        }
        let next_entry_ordinal = query
            .start_entry_ordinal
            .saturating_add(u64::from(query.budget.maximum_entries))
            .min(total_entry_count);
        let boundary_seeks = u64::from(query.start_entry_ordinal < total_entry_count)
            + u64::from(
                next_entry_ordinal < total_entry_count
                    && next_entry_ordinal != query.start_entry_ordinal,
            );
        // Each row-rank seek authenticates its selected Exit page and, in the
        // cross-page case, one summary-selected Enter page.
        let maximum_storage_pages = boundary_seeks
            .checked_mul(2)
            .ok_or_else(|| HostStoreError::invalid("Green ordinal page bound overflowed"))?;
        let maximum_tree_nodes = boundary_seeks
            .checked_mul(2)
            .and_then(|seeks| seeks.checked_mul(u64::from(descriptor.tree_height()) + 1))
            .ok_or_else(|| HostStoreError::invalid("Green ordinal tree bound overflowed"))?;
        let maximum_events = maximum_storage_pages
            .checked_mul(128)
            .ok_or_else(|| HostStoreError::invalid("Green ordinal event bound overflowed"))?;
        if maximum_storage_pages > u64::from(query.budget.maximum_storage_pages_visited) {
            return Ok(structural_ordinal_window_failure(
                query,
                total_entry_count,
                HostStructuralOrdinalWindowFailureReason::StoragePageLimit,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        }
        if maximum_tree_nodes > u64::from(query.budget.maximum_tree_nodes_visited) {
            return Ok(structural_ordinal_window_failure(
                query,
                total_entry_count,
                HostStructuralOrdinalWindowFailureReason::TreeNodeLimit,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        }
        if maximum_events > u64::from(query.budget.maximum_packed_entries_inspected) {
            return Ok(structural_ordinal_window_failure(
                query,
                total_entry_count,
                HostStructuralOrdinalWindowFailureReason::PackedEntryLimit,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        }
        let Some(window) = engine
            .persistent_recursive_green_row_ordinal_window(
                installed,
                query.start_entry_ordinal,
                query.budget.maximum_entries,
            )
            .map_err(map_engine_error)?
        else {
            return Ok(structural_ordinal_window_failure(
                query,
                0,
                HostStructuralOrdinalWindowFailureReason::UnavailableFacts,
                HostStructuralOrdinalWindowReceipt::default(),
            ));
        };
        let receipt = recursive_green_ordinal_window_receipt(window)?;
        if receipt.storage_pages_visited > query.budget.maximum_storage_pages_visited
            || receipt.tree_nodes_visited > query.budget.maximum_tree_nodes_visited
            || receipt.packed_entries_inspected > query.budget.maximum_packed_entries_inspected
        {
            return Err(HostStoreError::new(
                HostRejectReason::InternalFault,
                "Green ordinal-window engine exceeded its admitted work",
            ));
        }
        let start = structural_ordinal_window_metric(
            window.start_byte_offset(),
            window.start_utf16_offset(),
        )?;
        let next = structural_ordinal_window_metric(
            window.next_byte_offset(),
            window.next_utf16_offset(),
        )?;
        if window.total_entry_count() != total_entry_count
            || window.start_entry_ordinal() != query.start_entry_ordinal
            || window.next_entry_ordinal() != next_entry_ordinal
            || window.complete() != (next_entry_ordinal == total_entry_count)
            || start.bytes > next.bytes
            || start.utf16 > next.utf16
            || next.bytes > query.source_version.utf8_length
            || next.utf16 > query.source_version.utf16_length
            || (next_entry_ordinal == total_entry_count)
                != (next.bytes == query.source_version.utf8_length
                    && next.utf16 == query.source_version.utf16_length)
        {
            return Ok(structural_ordinal_window_failure(
                query,
                0,
                HostStructuralOrdinalWindowFailureReason::UndecodableClosure,
                receipt,
            ));
        }
        Ok(HostStructuralOrdinalWindowOutcome::Window {
            source_version: query.source_version,
            total_entry_count,
            start_entry_ordinal: query.start_entry_ordinal,
            next_entry_ordinal,
            start,
            next,
            complete: window.complete(),
            receipt,
        })
    }

    fn query_recursive_green(
        &self,
        query: HostPointQuery,
        output: &mut [u8],
        engine: &M11CandidateHost,
        installed: M11HostInstalledCandidate,
        descriptor: M11HostPersistentRecursiveGreenDescriptor,
    ) -> Result<HostStructuralQueryOutcome, HostStoreError> {
        let whole_range = whole_source_range(query.source_version);
        if descriptor.source_bytes() != u64::from(query.source_version.utf8_length)
            || descriptor.source_utf16() != u64::from(query.source_version.utf16_length)
            || descriptor.event_count() == 0
            || descriptor.storage_page_count() == 0
            || descriptor.tree_height() == 0
        {
            return Ok(source_gap(
                query.source_version,
                whole_range,
                HostSourceGapReason::UndecodableClosure,
                HostViewportReceipt::default(),
            ));
        }

        // The point zipper is bounded by selected ancestry and measured-tree
        // height rather than total document events/pages. Reserve only the
        // caller-admitted schema-9 ancestry envelope before touching output;
        // its authenticated work receipt is checked below before encoding.
        let maximum_accepted_ancestry = u64::from(query.budget.maximum_open_depth);
        let maximum_encoded_bytes = u64::try_from(HOST_RECURSIVE_GREEN_VIEWPORT_HEADER_BYTES)
            .expect("fixed Green viewport header fits u64")
            .checked_add(
                maximum_accepted_ancestry
                    .checked_mul(
                        u64::try_from(HOST_RECURSIVE_GREEN_ANCESTOR_RECORD_BYTES)
                            .expect("fixed Green ancestor record fits u64"),
                    )
                    .ok_or_else(|| {
                        HostStoreError::invalid("Green viewport byte bound overflowed")
                    })?,
            )
            .ok_or_else(|| HostStoreError::invalid("Green viewport byte bound overflowed"))?;
        if maximum_encoded_bytes > u64::from(query.budget.maximum_encoded_bytes)
            || maximum_encoded_bytes > u64::from(self.config.maximum_query_bytes)
        {
            return Ok(source_gap(
                query.source_version,
                whole_range,
                HostSourceGapReason::EncodedByteLimit,
                HostViewportReceipt::default(),
            ));
        }
        let maximum_encoded_bytes = usize::try_from(maximum_encoded_bytes)
            .map_err(|_| HostStoreError::invalid("Green viewport bytes exceed this target"))?;
        if output.len() < maximum_encoded_bytes {
            return Err(HostStoreError::new(
                HostRejectReason::QueryBoundExceeded,
                "query output scratch is smaller than the admitted Green viewport",
            ));
        }

        let Some(point_outcome) = engine
            .persistent_recursive_green_point(
                installed,
                u64::from(query.position.bytes),
                u64::from(query.position.utf16),
                match query.affinity {
                    HostMetricAffinity::Upstream => M11HostBlockAffinity::Before,
                    HostMetricAffinity::Downstream => M11HostBlockAffinity::After,
                },
                u64::from(query.budget.maximum_tree_nodes_visited),
            )
            .map_err(map_engine_error)?
        else {
            return Ok(source_gap(
                query.source_version,
                whole_range,
                HostSourceGapReason::UnavailableFacts,
                HostViewportReceipt::default(),
            ));
        };
        let location = match point_outcome {
            M11HostRecursiveGreenPointQueryOutcome::Location(location) => location,
            M11HostRecursiveGreenPointQueryOutcome::NotFound => {
                return Ok(source_gap(
                    query.source_version,
                    whole_range,
                    HostSourceGapReason::UnavailableFacts,
                    HostViewportReceipt::default(),
                ));
            }
            M11HostRecursiveGreenPointQueryOutcome::BudgetExceeded(exceeded) => {
                let open_depth = u32::try_from(exceeded.maximum_open_depth()).map_err(|_| {
                    HostStoreError::invalid("Green point budget receipt exceeds the wire")
                })?;
                let tree_nodes_visited =
                    u32::try_from(exceeded.node_headers_decoded()).map_err(|_| {
                        HostStoreError::invalid("Green point budget receipt exceeds the wire")
                    })?;
                let summary_nodes_skipped = u32::try_from(exceeded.summary_combinations())
                    .map_err(|_| {
                        HostStoreError::invalid("Green point budget receipt exceeds the wire")
                    })?;
                let leaf_count = u32::try_from(exceeded.storage_pages_visited()).map_err(|_| {
                    HostStoreError::invalid("Green point budget receipt exceeds the wire")
                })?;
                return Ok(source_gap(
                    query.source_version,
                    whole_range,
                    HostSourceGapReason::TreeNodeLimit,
                    HostViewportReceipt {
                        encoded_bytes: 0,
                        leaf_count,
                        open_depth,
                        tree_nodes_visited,
                        summary_nodes_skipped,
                    },
                ));
            }
        };

        let ancestry_count = u32::try_from(location.ancestry_len())
            .map_err(|_| HostStoreError::invalid("Green ancestry depth exceeds the wire"))?;
        let owner_index = u32::try_from(location.owner_index())
            .map_err(|_| HostStoreError::invalid("Green owner index exceeds the wire"))?;
        let range = HostMetricRange {
            start: HostSourceMetric {
                bytes: u32::try_from(location.byte_start())
                    .map_err(|_| HostStoreError::invalid("Green byte range exceeds the wire"))?,
                utf16: u32::try_from(location.utf16_start())
                    .map_err(|_| HostStoreError::invalid("Green UTF-16 range exceeds the wire"))?,
            },
            end: HostSourceMetric {
                bytes: u32::try_from(location.byte_end())
                    .map_err(|_| HostStoreError::invalid("Green byte range exceeds the wire"))?,
                utf16: u32::try_from(location.utf16_end())
                    .map_err(|_| HostStoreError::invalid("Green UTF-16 range exceeds the wire"))?,
            },
        };
        let physical_bytes = u32::try_from(location.physical_bytes())
            .map_err(|_| HostStoreError::invalid("Green physical metric exceeds the wire"))?;
        let physical_utf16 = u32::try_from(location.physical_utf16())
            .map_err(|_| HostStoreError::invalid("Green physical metric exceeds the wire"))?;
        let logical_bytes = u32::try_from(location.logical_bytes())
            .map_err(|_| HostStoreError::invalid("Green logical metric exceeds the wire"))?;
        let logical_utf16 = u32::try_from(location.logical_utf16())
            .map_err(|_| HostStoreError::invalid("Green logical metric exceeds the wire"))?;
        let events_scanned = u32::try_from(location.events_scanned())
            .map_err(|_| HostStoreError::invalid("Green query receipt exceeds the wire"))?;
        let storage_pages_visited = u32::try_from(location.storage_pages_visited())
            .map_err(|_| HostStoreError::invalid("Green query receipt exceeds the wire"))?;
        let open_depth = u32::try_from(location.maximum_open_depth())
            .map_err(|_| HostStoreError::invalid("Green query receipt exceeds the wire"))?;
        let tree_nodes_visited = u32::try_from(location.node_headers_decoded())
            .map_err(|_| HostStoreError::invalid("Green query receipt exceeds the wire"))?;
        let summary_nodes_skipped = u32::try_from(location.summary_combinations())
            .map_err(|_| HostStoreError::invalid("Green query receipt exceeds the wire"))?;
        let encoded_bytes = HOST_RECURSIVE_GREEN_VIEWPORT_HEADER_BYTES
            .checked_add(
                location
                    .ancestry_len()
                    .checked_mul(HOST_RECURSIVE_GREEN_ANCESTOR_RECORD_BYTES)
                    .ok_or_else(|| HostStoreError::invalid("Green viewport bytes overflowed"))?,
            )
            .ok_or_else(|| HostStoreError::invalid("Green viewport bytes overflowed"))?;
        let encoded_bytes_u32 = u32::try_from(encoded_bytes)
            .map_err(|_| HostStoreError::invalid("Green viewport bytes exceed the wire"))?;
        let receipt = HostViewportReceipt {
            encoded_bytes: 0,
            leaf_count: storage_pages_visited,
            open_depth,
            tree_nodes_visited,
            summary_nodes_skipped,
        };

        // Kind 14 is the parser-authenticated terminal-empty Item row. It is
        // the one renderable owner whose exact physical viewport is a point:
        // the editable insertion position at document EOF. Keep every other
        // zero-width or interior point fail-closed.
        let authenticated_empty_item_eof = location.owner().kind()
            == HOST_RECURSIVE_GREEN_EMPTY_ITEM_ROW_KIND
            && range.start.bytes == query.source_version.utf8_length
            && range.end.bytes == query.source_version.utf8_length
            && range.start.utf16 == query.source_version.utf16_length
            && range.end.utf16 == query.source_version.utf16_length
            && query.position == range.start
            && query.affinity == HostMetricAffinity::Downstream
            && physical_bytes == 0
            && physical_utf16 == 0
            && logical_bytes == 0
            && logical_utf16 == 0
            && location.part() == M11HostRecursiveGreenCoveragePart::Content
            && location.logical_atom() == M11HostRecursiveGreenLogicalAtom::Identity;
        let nonempty_range =
            range.start.bytes < range.end.bytes && range.start.utf16 < range.end.utf16;

        if ancestry_count == 0
            || owner_index >= ancestry_count
            || (!nonempty_range && !authenticated_empty_item_eof)
            || range.end.bytes > query.source_version.utf8_length
            || range.end.utf16 > query.source_version.utf16_length
            || physical_bytes != range.end.bytes - range.start.bytes
            || physical_utf16 != range.end.utf16 - range.start.utf16
            || location.owner()
                != location
                    .ancestor(location.owner_index())
                    .expect("validated Green owner index")
            || u64::from(events_scanned) > descriptor.event_count()
            || u64::from(storage_pages_visited) > descriptor.storage_page_count()
            || location.ancestry_len() > location.maximum_open_depth()
        {
            return Ok(source_gap(
                query.source_version,
                range,
                HostSourceGapReason::UndecodableClosure,
                receipt,
            ));
        }
        if open_depth > query.budget.maximum_open_depth {
            return Ok(source_gap(
                query.source_version,
                range,
                HostSourceGapReason::OpenDepthLimit,
                // The zipper keeps exact overrun telemetry internally, but a
                // wire receipt is caller-admitted work authority. Do not send
                // a counter that necessarily exceeds the declared envelope.
                HostViewportReceipt::default(),
            ));
        }
        if encoded_bytes > maximum_encoded_bytes {
            return Ok(source_gap(
                query.source_version,
                range,
                HostSourceGapReason::UndecodableClosure,
                receipt,
            ));
        }
        if storage_pages_visited > query.budget.maximum_leaf_count {
            return Ok(source_gap(
                query.source_version,
                range,
                HostSourceGapReason::LeafLimit,
                HostViewportReceipt::default(),
            ));
        }
        if tree_nodes_visited > query.budget.maximum_tree_nodes_visited {
            return Ok(source_gap(
                query.source_version,
                range,
                HostSourceGapReason::TreeNodeLimit,
                HostViewportReceipt::default(),
            ));
        }

        encode_recursive_green_viewport(
            query,
            &location,
            range,
            physical_bytes,
            physical_utf16,
            logical_bytes,
            logical_utf16,
            events_scanned,
            storage_pages_visited,
            open_depth,
            &mut output[..encoded_bytes],
        )?;
        Ok(HostStructuralQueryOutcome::Viewport {
            source_version: query.source_version,
            range,
            receipt: HostViewportReceipt {
                encoded_bytes: encoded_bytes_u32,
                ..receipt
            },
        })
    }

    fn query_persistent_blocks(
        &self,
        query: HostPointQuery,
        output: &mut [u8],
        engine: &M11CandidateHost,
        installed: M11HostInstalledCandidate,
        descriptor: M11HostPersistentBlockDescriptor,
    ) -> Result<HostStructuralQueryOutcome, HostStoreError> {
        let whole_range = whole_source_range(query.source_version);
        if descriptor.source_bytes() != u64::from(query.source_version.utf8_length)
            || descriptor.source_utf16() != u64::from(query.source_version.utf16_length)
            || (descriptor.entry_count() == 0)
                != (descriptor.source_bytes() == 0
                    && descriptor.source_utf16() == 0
                    && descriptor.storage_page_count() == 0
                    && descriptor.tree_height() == 0)
        {
            return Ok(source_gap(
                query.source_version,
                whole_range,
                HostSourceGapReason::UndecodableClosure,
                HostViewportReceipt::default(),
            ));
        }

        let structural_encoded_bytes = HOST_M11_VIEWPORT_BYTES as u32;
        let structural_maximum_open_depth = u32::from(descriptor.tree_height()).max(1);
        let structural_maximum_leaf_count = descriptor.maximum_entries_scanned();
        // Green and Projection wrappers were authenticated and cached at
        // installation, so a point query does not charge two fictitious arena
        // visits for them. Tree work is the measured descent; leaf work is the
        // bounded packed-page scan reported separately below.
        let structural_maximum_tree_nodes_visited =
            u32::try_from(descriptor.maximum_tree_nodes_visited())
                .map_err(|_| HostStoreError::invalid("block query work bound overflowed"))?;
        let gap = if query.budget.maximum_encoded_bytes < structural_encoded_bytes
            || self.config.maximum_query_bytes < HOST_M11_VIEWPORT_BYTES as u32
        {
            Some(HostSourceGapReason::EncodedByteLimit)
        } else if query.budget.maximum_open_depth < structural_maximum_open_depth {
            Some(HostSourceGapReason::OpenDepthLimit)
        } else if query.budget.maximum_leaf_count < structural_maximum_leaf_count {
            Some(HostSourceGapReason::LeafLimit)
        } else if query.budget.maximum_tree_nodes_visited < structural_maximum_tree_nodes_visited {
            Some(HostSourceGapReason::TreeNodeLimit)
        } else {
            None
        };
        if let Some(reason) = gap {
            return Ok(source_gap(
                query.source_version,
                whole_range,
                reason,
                HostViewportReceipt::default(),
            ));
        }
        if output.len() < HOST_M11_VIEWPORT_BYTES {
            return Err(HostStoreError::new(
                HostRejectReason::QueryBoundExceeded,
                "query output scratch is smaller than the admitted viewport",
            ));
        }

        let location = engine
            .persistent_block_point(
                installed,
                u64::from(query.position.bytes),
                u64::from(query.position.utf16),
                match query.affinity {
                    HostMetricAffinity::Upstream => M11HostBlockAffinity::Before,
                    HostMetricAffinity::Downstream => M11HostBlockAffinity::After,
                },
            )
            .map_err(map_engine_error)?;
        let mut green = [0_u8; M11_GREEN_RECORD_BYTES];
        let mut projection = [0_u8; M11_PROJECTION_RECORD_BYTES];
        let (range, mut receipt, inline_sidecar_matches) = match location {
            None => {
                if descriptor.entry_count() != 0
                    || query.source_version.utf8_length != 0
                    || query.source_version.utf16_length != 0
                    || !synthesize_empty_block_records(&mut green, &mut projection)
                {
                    return Ok(source_gap(
                        query.source_version,
                        whole_range,
                        HostSourceGapReason::UndecodableClosure,
                        HostViewportReceipt::default(),
                    ));
                }
                (
                    whole_range,
                    HostViewportReceipt {
                        encoded_bytes: structural_encoded_bytes,
                        leaf_count: 0,
                        open_depth: 1,
                        tree_nodes_visited: 0,
                        summary_nodes_skipped: 0,
                    },
                    false,
                )
            }
            Some(location) => {
                let Some(range) = persistent_block_location_range(query, &location) else {
                    return Ok(source_gap(
                        query.source_version,
                        whole_range,
                        HostSourceGapReason::UndecodableClosure,
                        HostViewportReceipt::default(),
                    ));
                };
                let query_receipt = location.receipt();
                let leaf_count = query_receipt.entries_scanned();
                let tree_nodes_visited = u32::try_from(query_receipt.node_headers_decoded())
                    .map_err(|_| HostStoreError::invalid("block query receipt overflowed"))?;
                let summary_nodes_skipped = u32::try_from(query_receipt.summary_combinations())
                    .map_err(|_| HostStoreError::invalid("block query receipt overflowed"))?;
                if location.entry_ordinal() >= descriptor.entry_count()
                    || location.storage_page_ordinal() >= descriptor.storage_page_count()
                    || leaf_count == 0
                    || leaf_count > descriptor.maximum_entries_scanned()
                    || tree_nodes_visited > structural_maximum_tree_nodes_visited
                    || leaf_count > query.budget.maximum_leaf_count
                    || tree_nodes_visited > query.budget.maximum_tree_nodes_visited
                {
                    return Ok(source_gap(
                        query.source_version,
                        range,
                        HostSourceGapReason::UndecodableClosure,
                        HostViewportReceipt {
                            leaf_count,
                            tree_nodes_visited,
                            summary_nodes_skipped,
                            ..HostViewportReceipt::default()
                        },
                    ));
                }
                if !persistent_block_records(&location, range, &mut green, &mut projection)
                    || !m11_records_describe_query_range(query, range, &green, &projection)
                {
                    return Ok(source_gap(
                        query.source_version,
                        range,
                        HostSourceGapReason::UndecodableClosure,
                        HostViewportReceipt {
                            leaf_count,
                            open_depth: structural_maximum_open_depth,
                            tree_nodes_visited,
                            summary_nodes_skipped,
                            ..HostViewportReceipt::default()
                        },
                    ));
                }
                let inline_sidecar_matches =
                    self.inline_sidecar_matches_block(&location, range, query);
                (
                    range,
                    HostViewportReceipt {
                        encoded_bytes: structural_encoded_bytes,
                        leaf_count,
                        open_depth: structural_maximum_open_depth,
                        tree_nodes_visited,
                        summary_nodes_skipped,
                    },
                    inline_sidecar_matches,
                )
            }
        };

        let installed_sidecar = inline_sidecar_matches
            .then_some(self.installed_inline_sidecar.as_ref())
            .flatten();
        let mut inline_query = match installed_sidecar {
            Some(installed_sidecar) => self
                .inline_sidecar
                .as_ref()
                .ok_or_else(|| HostStoreError::invalid("installed sidecar engine disappeared"))?
                .query(&installed_sidecar.binding)
                .map_err(map_engine_error)?,
            None => None,
        };
        let (
            inline_encoded_bytes,
            inline_logical_pages,
            inline_maximum_open_depth,
            inline_maximum_tree_nodes_visited,
        ) = match (inline_query.as_ref(), installed_sidecar) {
            (Some(M11HostInlineSidecarQuery::ProjectedInline { .. }), Some(_installed_sidecar)) => {
                (0, 0, 0, 0)
            }
            (
                Some(M11HostInlineSidecarQuery::Authoritative { descriptor, .. }),
                Some(installed_sidecar),
            ) => {
                let HotInlineSidecarDisposition::Authoritative {
                    logical_page_count,
                    fact_count,
                    storage_page_count,
                    link_value_entry_count,
                    link_value_storage_page_count,
                    link_value_encoded_bytes,
                    ..
                } = installed_sidecar.begin.envelope.disposition
                else {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "sidecar engine and wire dispositions disagree",
                    ));
                };
                let binding = installed_sidecar.begin.binding;
                if descriptor.source_start() != binding.visible_start_utf8
                    || descriptor.source_end() != binding.visible_end_utf8
                    || descriptor.logical_page_count() != logical_page_count
                    || descriptor.fact_count() != fact_count
                    || descriptor.storage_page_count() != storage_page_count
                    || descriptor.link_value_entry_count() != link_value_entry_count
                    || descriptor.link_value_storage_page_count() != link_value_storage_page_count
                    || descriptor.link_value_encoded_bytes() != link_value_encoded_bytes
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "sidecar query descriptor disagrees with HIO1",
                    ));
                }
                let fact_bytes = usize::try_from(fact_count)
                    .ok()
                    .and_then(|facts| facts.checked_mul(M11_INLINE_FACT_RECORD_BYTES))
                    .ok_or_else(|| HostStoreError::invalid("sidecar fact bytes overflowed"))?;
                let value_bytes = usize::try_from(link_value_encoded_bytes).map_err(|_| {
                    HostStoreError::invalid("sidecar link-value bytes exceed this target")
                })?;
                (
                    M11_INLINE_META_RECORD_BYTES
                        .checked_add(fact_bytes)
                        .and_then(|bytes| bytes.checked_add(value_bytes))
                        .ok_or_else(|| {
                            HostStoreError::invalid("sidecar viewport bytes overflowed")
                        })?,
                    logical_page_count,
                    descriptor.maximum_open_depth().max(1),
                    descriptor.maximum_tree_nodes_visited(),
                )
            }
            (
                Some(M11HostInlineSidecarQuery::IndentedCode { descriptor, .. }),
                Some(installed_sidecar),
            ) => {
                let HotInlineSidecarDisposition::Authoritative {
                    logical_page_count,
                    fact_count: line_count,
                    storage_page_count,
                    ordered_commitment256,
                    ..
                } = installed_sidecar.begin.envelope.disposition
                else {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "sidecar engine and wire dispositions disagree",
                    ));
                };
                let binding = installed_sidecar.begin.binding;
                if green[12] != M11_INDENTED_CODE_VARIANT
                    || line_count == 0
                    || u64::from(read_u32(&green, 56)) != line_count
                    || read_u64(&projection, 48) != line_count
                    || binding.physical_start_utf8 != range.start.bytes
                    || binding.physical_end_utf8 != range.end.bytes
                    || binding.physical_start_utf16 != range.start.utf16
                    || binding.physical_end_utf16 != range.end.utf16
                    || binding.visible_start_utf8 != binding.physical_start_utf8
                    || binding.visible_end_utf8 != binding.physical_end_utf8
                    || binding.visible_start_utf16 != binding.physical_start_utf16
                    || binding.visible_end_utf16 != binding.physical_end_utf16
                    || descriptor.physical_start() != binding.physical_start_utf8
                    || descriptor.physical_end() != binding.physical_end_utf8
                    || descriptor.window_start() != binding.visible_start_utf8
                    || descriptor.window_end() != binding.visible_end_utf8
                    || descriptor.projection_flags() & !1 != 0
                    || descriptor.logical_page_count() != logical_page_count
                    || descriptor.line_count() != line_count
                    || descriptor.storage_page_count() != storage_page_count
                    || descriptor.ordered_commitment256() != ordered_commitment256
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "indented-code sidecar does not match its structural block",
                    ));
                }
                let line_bytes = usize::try_from(line_count)
                    .ok()
                    .and_then(|lines| lines.checked_mul(M11_INDENTED_CODE_LINE_RECORD_BYTES))
                    .ok_or_else(|| {
                        HostStoreError::invalid("indented-code sidecar viewport bytes overflowed")
                    })?;
                (
                    line_bytes,
                    logical_page_count,
                    descriptor.maximum_open_depth().max(1),
                    descriptor.maximum_tree_nodes_visited(),
                )
            }
            (
                Some(M11HostInlineSidecarQuery::BlockQuote { descriptor, .. }),
                Some(installed_sidecar),
            ) => {
                let HotInlineSidecarDisposition::Authoritative {
                    logical_page_count,
                    fact_count: line_count,
                    storage_page_count,
                    ordered_commitment256,
                    ..
                } = installed_sidecar.begin.envelope.disposition
                else {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "sidecar engine and wire dispositions disagree",
                    ));
                };
                let binding = installed_sidecar.begin.binding;
                if green[12] != M11_BLOCK_QUOTE_VARIANT
                    || line_count == 0
                    || u64::from(read_u32(&green, 56)) != line_count
                    || read_u32(&green, 60) != 0
                    || u64::from(read_u32(&green, 64)) != line_count
                    || read_u64(&projection, 48) != line_count
                    || binding.physical_start_utf8 != range.start.bytes
                    || binding.physical_end_utf8 != range.end.bytes
                    || binding.physical_start_utf16 != range.start.utf16
                    || binding.physical_end_utf16 != range.end.utf16
                    || binding.visible_start_utf8 != binding.physical_start_utf8
                    || binding.visible_end_utf8 != binding.physical_end_utf8
                    || binding.visible_start_utf16 != binding.physical_start_utf16
                    || binding.visible_end_utf16 != binding.physical_end_utf16
                    || descriptor.physical_start() != binding.physical_start_utf8
                    || descriptor.physical_end() != binding.physical_end_utf8
                    || descriptor.window_start() != binding.visible_start_utf8
                    || descriptor.window_end() != binding.visible_end_utf8
                    || descriptor.projected_utf8_length() != read_u32(&green, 68)
                    || descriptor.projected_utf16_length() != read_u32(&green, 72)
                    || descriptor.logical_page_count() != logical_page_count
                    || descriptor.line_count() != line_count
                    || descriptor.storage_page_count() != storage_page_count
                    || descriptor.ordered_commitment256() != ordered_commitment256
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "block-quote sidecar does not match its structural path",
                    ));
                }
                let line_bytes = usize::try_from(line_count)
                    .ok()
                    .and_then(|lines| lines.checked_mul(M11_BLOCK_QUOTE_LINE_RECORD_BYTES))
                    .ok_or_else(|| {
                        HostStoreError::invalid("block-quote sidecar viewport bytes overflowed")
                    })?;
                (
                    line_bytes,
                    logical_page_count,
                    descriptor.maximum_open_depth().max(2),
                    descriptor.maximum_tree_nodes_visited(),
                )
            }
            (
                Some(M11HostInlineSidecarQuery::BulletList {
                    selected_item_ordinal,
                    selected_item_line_ending,
                    descriptor,
                    ..
                }),
                Some(installed_sidecar),
            ) => {
                let HotInlineSidecarDisposition::Authoritative {
                    logical_page_count,
                    fact_count: projected_item_count,
                    storage_page_count,
                    ordered_commitment256,
                    ..
                } = installed_sidecar.begin.envelope.disposition
                else {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "sidecar engine and wire dispositions disagree",
                    ));
                };
                let binding = installed_sidecar.begin.binding;
                let structural_item_count = u64::from(read_u32(&green, 56));
                let compact_item = selected_item_ordinal.is_some();
                if !validate_bullet_list_structural_summary(
                    range,
                    &green,
                    &projection,
                    structural_item_count,
                ) || binding.physical_start_utf8 != range.start.bytes
                    || binding.physical_end_utf8 != range.end.bytes
                    || binding.physical_start_utf16 != range.start.utf16
                    || binding.physical_end_utf16 != range.end.utf16
                    || descriptor.physical_start() != binding.physical_start_utf8
                    || descriptor.physical_end() != binding.physical_end_utf8
                    || descriptor.window_start() != binding.visible_start_utf8
                    || descriptor.window_end() != binding.visible_end_utf8
                    || descriptor.logical_page_count() != logical_page_count
                    || descriptor.line_count() != projected_item_count
                    || descriptor.storage_page_count() != storage_page_count
                    || descriptor.ordered_commitment256() != ordered_commitment256
                    || compact_item
                        != (binding.visible_start_utf8 != binding.physical_start_utf8
                            || binding.visible_end_utf8 != binding.physical_end_utf8)
                    || compact_item
                        != (binding.visible_start_utf16 != binding.physical_start_utf16
                            || binding.visible_end_utf16 != binding.physical_end_utf16)
                    || compact_item != selected_item_line_ending.is_some()
                    || if let Some(selected_item_ordinal) = selected_item_ordinal {
                        projected_item_count != 1
                            || u64::from(*selected_item_ordinal) >= structural_item_count
                    } else {
                        projected_item_count != structural_item_count
                            || descriptor.projected_utf8_length() != read_u32(&green, 68)
                            || descriptor.projected_utf16_length() != read_u32(&green, 72)
                    }
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "bullet-list sidecar does not match its structural path",
                    ));
                }
                let item_record_bytes = usize::try_from(projected_item_count)
                    .ok()
                    .and_then(|items| items.checked_mul(M11_BULLET_LIST_ITEM_RECORD_BYTES))
                    .ok_or_else(|| {
                        HostStoreError::invalid("bullet-list sidecar viewport bytes overflowed")
                    })?;
                let item_bytes = item_record_bytes
                    .checked_add(if compact_item {
                        M11_BULLET_LIST_ITEM_META_BYTES
                    } else {
                        0
                    })
                    .ok_or_else(|| {
                        HostStoreError::invalid("bullet-list sidecar viewport bytes overflowed")
                    })?;
                (
                    item_bytes,
                    logical_page_count,
                    descriptor.maximum_open_depth().max(3),
                    descriptor.maximum_tree_nodes_visited(),
                )
            }
            (
                Some(M11HostInlineSidecarQuery::OrderedList {
                    selected_item_ordinal,
                    selected_item_line_ending: _,
                    opening_marker_start,
                    opening_marker_end,
                    marker_value,
                    descriptor,
                    ..
                }),
                Some(installed_sidecar),
            ) => {
                let HotInlineSidecarDisposition::Authoritative {
                    logical_page_count,
                    fact_count: projected_item_count,
                    storage_page_count,
                    ordered_commitment256,
                    ..
                } = installed_sidecar.begin.envelope.disposition
                else {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "sidecar engine and wire dispositions disagree",
                    ));
                };
                let binding = installed_sidecar.begin.binding;
                let structural_item_count = u64::from(read_u32(&green, 56));
                if !validate_ordered_list_structural_summary(
                    range,
                    &green,
                    &projection,
                    structural_item_count,
                ) || binding.physical_start_utf8 != range.start.bytes
                    || binding.physical_end_utf8 != range.end.bytes
                    || binding.physical_start_utf16 != range.start.utf16
                    || binding.physical_end_utf16 != range.end.utf16
                    || descriptor.physical_start() != binding.physical_start_utf8
                    || descriptor.physical_end() != binding.physical_end_utf8
                    || descriptor.window_start() != binding.visible_start_utf8
                    || descriptor.window_end() != binding.visible_end_utf8
                    || logical_page_count != 1
                    || projected_item_count != 1
                    || storage_page_count != 1
                    || descriptor.logical_page_count() != 1
                    || descriptor.line_count() != 1
                    || descriptor.storage_page_count() != 1
                    || descriptor.ordered_commitment256() != ordered_commitment256
                    || u64::from(*selected_item_ordinal) >= structural_item_count
                    || *selected_item_ordinal == u32::MAX
                    || *opening_marker_start >= *opening_marker_end
                    || !(2..=10).contains(&opening_marker_end.saturating_sub(*opening_marker_start))
                    || *marker_value > 999_999_999
                    || *selected_item_ordinal == 0 && *marker_value != read_u32(&green, 76)
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "ordered-list sidecar does not match its structural path",
                    ));
                }
                (
                    M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES,
                    logical_page_count,
                    descriptor.maximum_open_depth().max(3),
                    descriptor.maximum_tree_nodes_visited(),
                )
            }
            (
                Some(M11HostInlineSidecarQuery::Unsupported { metadata }),
                Some(installed_sidecar),
            ) => {
                if metadata.is_empty()
                    || !matches!(
                        installed_sidecar.begin.envelope.disposition,
                        HotInlineSidecarDisposition::Unsupported { .. }
                    )
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "unsupported sidecar lost its authenticated metadata",
                    ));
                }
                (M11_INLINE_META_RECORD_BYTES, 0, 1, 0)
            }
            (None, None) => (0, 0, 0, 0),
            _ => {
                return Err(HostStoreError::new(
                    HostRejectReason::InternalFault,
                    "installed sidecar did not match its exact query",
                ));
            }
        };
        if matches!(
            inline_query,
            Some(M11HostInlineSidecarQuery::ProjectedInline { .. })
        ) {
            // Projected coordinates need a dedicated nested-projection
            // viewport schema. Keep this checkpoint on the direct sidecar
            // query lane instead of reinterpreting them as physical offsets.
            inline_query = None;
        }
        let leaf_projection_payload_kind = match inline_query.as_ref() {
            Some(M11HostInlineSidecarQuery::BlockQuote { .. }) => {
                M11_LEAF_PROJECTION_PAYLOAD_BLOCK_QUOTE
            }
            Some(M11HostInlineSidecarQuery::BulletList {
                selected_item_ordinal: Some(_),
                ..
            }) => M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST_ITEM,
            Some(M11HostInlineSidecarQuery::BulletList {
                selected_item_ordinal: None,
                ..
            }) => M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST,
            Some(M11HostInlineSidecarQuery::OrderedList { .. }) => {
                M11_LEAF_PROJECTION_PAYLOAD_ORDERED_LIST_ITEM
            }
            Some(M11HostInlineSidecarQuery::IndentedCode { .. }) => {
                M11_LEAF_PROJECTION_PAYLOAD_INDENTED_CODE
            }
            Some(
                M11HostInlineSidecarQuery::Authoritative { .. }
                | M11HostInlineSidecarQuery::Unsupported { .. },
            ) => M11_LEAF_PROJECTION_PAYLOAD_INLINE,
            Some(M11HostInlineSidecarQuery::ProjectedInline { .. }) => {
                unreachable!("projected-inline query was removed before physical viewport encoding")
            }
            None => 0,
        };
        let has_leaf_projection = leaf_projection_payload_kind != 0;
        let viewport_header_bytes = match leaf_projection_payload_kind {
            M11_LEAF_PROJECTION_PAYLOAD_ORDERED_LIST_ITEM => M11_VIEWPORT_V7_HEADER_BYTES,
            M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST_ITEM => M11_VIEWPORT_V6_HEADER_BYTES,
            M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST => M11_VIEWPORT_V5_HEADER_BYTES,
            M11_LEAF_PROJECTION_PAYLOAD_BLOCK_QUOTE => M11_VIEWPORT_V4_HEADER_BYTES,
            M11_LEAF_PROJECTION_PAYLOAD_INDENTED_CODE => M11_VIEWPORT_V3_HEADER_BYTES,
            M11_LEAF_PROJECTION_PAYLOAD_INLINE => M11_VIEWPORT_INLINE_HEADER_BYTES,
            _ => M11_VIEWPORT_HEADER_BYTES,
        };
        let point_path_node_count = match leaf_projection_payload_kind {
            M11_LEAF_PROJECTION_PAYLOAD_BLOCK_QUOTE => M11_BLOCK_QUOTE_POINT_PATH_NODE_COUNT,
            M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST
            | M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST_ITEM => {
                bullet_list_point_path_node_count(query, range, &green)?
            }
            M11_LEAF_PROJECTION_PAYLOAD_ORDERED_LIST_ITEM => {
                ordered_list_point_path_node_count(query, range, &green)?
            }
            _ => 0,
        };
        let point_path_bytes = match leaf_projection_payload_kind {
            M11_LEAF_PROJECTION_PAYLOAD_BLOCK_QUOTE => M11_BLOCK_QUOTE_POINT_PATH_BYTES,
            M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST
            | M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST_ITEM => point_path_node_count
                .checked_mul(M11_POINT_PATH_V5_NODE_RECORD_BYTES)
                .ok_or_else(|| HostStoreError::invalid("point-path bytes overflowed"))?,
            M11_LEAF_PROJECTION_PAYLOAD_ORDERED_LIST_ITEM => point_path_node_count
                .checked_mul(M11_POINT_PATH_V5_NODE_RECORD_BYTES)
                .ok_or_else(|| HostStoreError::invalid("point-path bytes overflowed"))?,
            _ => 0,
        };
        let encoded_bytes = viewport_header_bytes
            .checked_add(M11_GREEN_RECORD_BYTES)
            .and_then(|bytes| bytes.checked_add(M11_PROJECTION_RECORD_BYTES))
            .and_then(|bytes| bytes.checked_add(point_path_bytes))
            .and_then(|bytes| bytes.checked_add(inline_encoded_bytes))
            .ok_or_else(|| HostStoreError::invalid("sidecar viewport bytes overflowed"))?;
        let inline_leaf_count = u32::try_from(inline_logical_pages)
            .ok()
            .and_then(|pages| {
                pages.checked_add(match leaf_projection_payload_kind {
                    M11_LEAF_PROJECTION_PAYLOAD_BLOCK_QUOTE
                    | M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST
                    | M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST_ITEM
                    | M11_LEAF_PROJECTION_PAYLOAD_ORDERED_LIST_ITEM => {
                        u32::try_from(point_path_node_count).ok()?
                    }
                    _ => u32::from(has_leaf_projection),
                })
            })
            .ok_or_else(|| HostStoreError::invalid("sidecar query leaf bound overflowed"))?;
        let maximum_leaf_count = structural_maximum_leaf_count
            .checked_add(inline_leaf_count)
            .ok_or_else(|| HostStoreError::invalid("sidecar query leaf bound overflowed"))?;
        let maximum_open_depth = structural_maximum_open_depth.max(inline_maximum_open_depth);
        let inline_maximum_tree_nodes_visited = u32::try_from(inline_maximum_tree_nodes_visited)
            .map_err(|_| HostStoreError::invalid("sidecar query work bound overflowed"))?;
        let maximum_tree_nodes_visited = structural_maximum_tree_nodes_visited
            .checked_add(inline_maximum_tree_nodes_visited)
            .ok_or_else(|| HostStoreError::invalid("sidecar query work bound overflowed"))?;
        let encoded_bytes_u32 = u32::try_from(encoded_bytes)
            .map_err(|_| HostStoreError::invalid("sidecar viewport bytes overflowed"))?;
        let gap = if query.budget.maximum_encoded_bytes < encoded_bytes_u32
            || self.config.maximum_query_bytes < encoded_bytes_u32
        {
            Some(HostSourceGapReason::EncodedByteLimit)
        } else if query.budget.maximum_open_depth < maximum_open_depth {
            Some(HostSourceGapReason::OpenDepthLimit)
        } else if query.budget.maximum_leaf_count < maximum_leaf_count {
            Some(HostSourceGapReason::LeafLimit)
        } else if query.budget.maximum_tree_nodes_visited < maximum_tree_nodes_visited {
            Some(HostSourceGapReason::TreeNodeLimit)
        } else {
            None
        };
        if let Some(reason) = gap {
            return Ok(source_gap(
                query.source_version,
                range,
                reason,
                HostViewportReceipt::default(),
            ));
        }
        if output.len() < encoded_bytes {
            return Err(HostStoreError::new(
                HostRejectReason::QueryBoundExceeded,
                "query output scratch is smaller than the sidecar viewport",
            ));
        }

        let (header, records) = output[..encoded_bytes].split_at_mut(viewport_header_bytes);
        let (green_output, projection_output) = records.split_at_mut(M11_GREEN_RECORD_BYTES);
        let (projection_output, projection_and_payload) =
            projection_output.split_at_mut(M11_PROJECTION_RECORD_BYTES);
        let (point_path_output, inline_output) =
            projection_and_payload.split_at_mut(point_path_bytes);
        header[..8].copy_from_slice(VIEWPORT_MAGIC);
        header[8..12].copy_from_slice(
            &match leaf_projection_payload_kind {
                M11_LEAF_PROJECTION_PAYLOAD_ORDERED_LIST_ITEM => M11_VIEWPORT_V7_SCHEMA,
                M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST_ITEM => M11_VIEWPORT_V6_SCHEMA,
                M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST => M11_VIEWPORT_V5_SCHEMA,
                M11_LEAF_PROJECTION_PAYLOAD_BLOCK_QUOTE => M11_VIEWPORT_V4_SCHEMA,
                M11_LEAF_PROJECTION_PAYLOAD_INDENTED_CODE => M11_VIEWPORT_V3_SCHEMA,
                M11_LEAF_PROJECTION_PAYLOAD_INLINE => M11_VIEWPORT_INLINE_SCHEMA,
                _ => M11_ROLE_SCHEMA,
            }
            .to_le_bytes(),
        );
        header[12..16].copy_from_slice(&(M11_GREEN_RECORD_BYTES as u32).to_le_bytes());
        header[16..20].copy_from_slice(&(M11_PROJECTION_RECORD_BYTES as u32).to_le_bytes());
        green_output.copy_from_slice(&green);
        projection_output.copy_from_slice(&projection);
        if leaf_projection_payload_kind == M11_LEAF_PROJECTION_PAYLOAD_BLOCK_QUOTE
            && !encode_block_quote_point_path(
                point_path_output,
                range,
                green_output,
                projection_output,
            )
        {
            return Err(HostStoreError::new(
                HostRejectReason::InternalFault,
                "block-quote point path disagrees with its structural summary",
            ));
        }
        if let Some(inline_query) = inline_query.take() {
            let payload_bytes = u32::try_from(inline_encoded_bytes)
                .map_err(|_| HostStoreError::invalid("sidecar viewport bytes overflowed"))?;
            if matches!(
                leaf_projection_payload_kind,
                M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST
                    | M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST_ITEM
                    | M11_LEAF_PROJECTION_PAYLOAD_ORDERED_LIST_ITEM
            ) {
                header[20..22].copy_from_slice(
                    &u16::try_from(point_path_node_count)
                        .map_err(|_| HostStoreError::invalid("point-path node count overflowed"))?
                        .to_le_bytes(),
                );
                header[22] = leaf_projection_payload_kind;
                header[23] = 0;
                header[24..28].copy_from_slice(
                    &u32::try_from(point_path_bytes)
                        .map_err(|_| HostStoreError::invalid("point-path bytes overflowed"))?
                        .to_le_bytes(),
                );
                header[28..32].copy_from_slice(&payload_bytes.to_le_bytes());
            } else if leaf_projection_payload_kind == M11_LEAF_PROJECTION_PAYLOAD_BLOCK_QUOTE {
                header[20..22].copy_from_slice(
                    &u16::try_from(M11_BLOCK_QUOTE_POINT_PATH_NODE_COUNT)
                        .expect("fixed point path count fits")
                        .to_le_bytes(),
                );
                header[22] = M11_LEAF_PROJECTION_PAYLOAD_BLOCK_QUOTE;
                header[23] = 0;
                header[24..28].copy_from_slice(
                    &u32::try_from(M11_BLOCK_QUOTE_POINT_PATH_BYTES)
                        .expect("fixed point path bytes fit")
                        .to_le_bytes(),
                );
                header[28..32].copy_from_slice(&payload_bytes.to_le_bytes());
            } else if leaf_projection_payload_kind == M11_LEAF_PROJECTION_PAYLOAD_INDENTED_CODE {
                header[20] = M11_LEAF_PROJECTION_PAYLOAD_INDENTED_CODE;
                header[21..24].fill(0);
                header[24..28].copy_from_slice(&payload_bytes.to_le_bytes());
            } else {
                header[20..24].copy_from_slice(&payload_bytes.to_le_bytes());
            }
            let installed_sidecar =
                installed_sidecar.expect("matched sidecar accompanies its engine query");
            let binding = installed_sidecar.begin.binding;
            let projection_tree_nodes_visited = match inline_query {
                M11HostInlineSidecarQuery::Authoritative {
                    descriptor,
                    mut cursor,
                    link_values,
                } => {
                    let (metadata, inline_payload) =
                        inline_output.split_at_mut(M11_INLINE_META_RECORD_BYTES);
                    metadata.fill(0);
                    metadata[..8].copy_from_slice(M11_INLINE_META_MAGIC);
                    metadata[8..12].copy_from_slice(&M11_INLINE_SCHEMA.to_le_bytes());
                    metadata[12] = 1;
                    metadata[16..20].copy_from_slice(&self.config.syntax_profile.to_le_bytes());
                    metadata[20..24].copy_from_slice(
                        &u32::try_from(descriptor.fact_count())
                            .map_err(|_| HostStoreError::invalid("sidecar fact count overflowed"))?
                            .to_le_bytes(),
                    );
                    metadata[24..32]
                        .copy_from_slice(&u64::from(binding.visible_start_utf8).to_le_bytes());
                    metadata[32..40]
                        .copy_from_slice(&u64::from(binding.visible_end_utf8).to_le_bytes());
                    metadata[40..44]
                        .copy_from_slice(&(M11_INLINE_FACT_RECORD_BYTES as u32).to_le_bytes());

                    let fact_bytes = usize::try_from(descriptor.fact_count())
                        .ok()
                        .and_then(|count| count.checked_mul(M11_INLINE_FACT_RECORD_BYTES))
                        .ok_or_else(|| HostStoreError::invalid("sidecar fact bytes overflowed"))?;
                    let (fact_output, value_output) = inline_payload.split_at_mut(fact_bytes);
                    let mut fact_count = 0_usize;
                    loop {
                        match cursor.poll().map_err(map_engine_error)? {
                            M11HostInlineProjectionCursorPoll::Fact(fact) => {
                                let start = fact_count
                                    .checked_mul(M11_INLINE_FACT_RECORD_BYTES)
                                    .ok_or_else(|| {
                                        HostStoreError::invalid("sidecar fact offset overflowed")
                                    })?;
                                let end = start
                                    .checked_add(M11_INLINE_FACT_RECORD_BYTES)
                                    .ok_or_else(|| {
                                        HostStoreError::invalid("sidecar fact offset overflowed")
                                    })?;
                                let record = fact_output.get_mut(start..end).ok_or_else(|| {
                                    HostStoreError::new(
                                        HostRejectReason::InternalFault,
                                        "sidecar cursor exceeded its authenticated envelope",
                                    )
                                })?;
                                encode_inline_projection_fact_record(fact, record)?;
                                fact_count += 1;
                            }
                            M11HostInlineProjectionCursorPoll::Complete => break,
                        }
                    }
                    let value_receipt = link_values.copy(value_output).map_err(map_engine_error)?;
                    let tree_nodes_visited = cursor
                        .tree_nodes_visited()
                        .checked_add(value_receipt.tree_nodes_visited)
                        .and_then(|visited| visited.checked_add(1))
                        .ok_or_else(|| {
                            HostStoreError::invalid("sidecar query receipt overflowed")
                        })?;
                    if u64::try_from(fact_count).ok() != Some(descriptor.fact_count())
                        || value_receipt.entry_count != descriptor.link_value_entry_count()
                        || link_values.entry_count() != descriptor.link_value_entry_count()
                        || value_output.len() != descriptor.link_value_encoded_bytes() as usize
                        || tree_nodes_visited > descriptor.maximum_tree_nodes_visited()
                    {
                        return Err(HostStoreError::new(
                            HostRejectReason::InternalFault,
                            "sidecar cursor disagrees with its authenticated descriptor",
                        ));
                    }
                    tree_nodes_visited
                }
                M11HostInlineSidecarQuery::IndentedCode {
                    descriptor,
                    mut cursor,
                } => {
                    let mut line_count = 0_usize;
                    loop {
                        match cursor.poll().map_err(map_engine_error)? {
                            M11HostIndentedCodeCursorPoll::Line(line) => {
                                let start = line_count
                                    .checked_mul(M11_INDENTED_CODE_LINE_RECORD_BYTES)
                                    .ok_or_else(|| {
                                        HostStoreError::invalid(
                                            "indented-code record offset overflowed",
                                        )
                                    })?;
                                let end = start
                                    .checked_add(M11_INDENTED_CODE_LINE_RECORD_BYTES)
                                    .ok_or_else(|| {
                                        HostStoreError::invalid(
                                            "indented-code record offset overflowed",
                                        )
                                    })?;
                                let record =
                                    inline_output.get_mut(start..end).ok_or_else(|| {
                                        HostStoreError::new(
                                        HostRejectReason::InternalFault,
                                        "indented-code cursor exceeded its authenticated envelope",
                                    )
                                    })?;
                                record[0..4]
                                    .copy_from_slice(&line.relative_line_start().to_le_bytes());
                                record[4..8]
                                    .copy_from_slice(&line.physical_source_length().to_le_bytes());
                                record[8..12]
                                    .copy_from_slice(&line.hidden_prefix_length().to_le_bytes());
                                record[12..16]
                                    .copy_from_slice(&line.content_length().to_le_bytes());
                                record[16..20].copy_from_slice(&line.flags().to_le_bytes());
                                line_count += 1;
                            }
                            M11HostIndentedCodeCursorPoll::Complete => break,
                        }
                    }
                    if u64::try_from(line_count).ok() != Some(descriptor.line_count())
                        || line_count.checked_mul(M11_INDENTED_CODE_LINE_RECORD_BYTES)
                            != Some(inline_output.len())
                    {
                        return Err(HostStoreError::new(
                            HostRejectReason::InternalFault,
                            "indented-code cursor disagrees with its authenticated descriptor",
                        ));
                    }
                    cursor.tree_nodes_visited()
                }
                M11HostInlineSidecarQuery::BlockQuote {
                    descriptor,
                    mut cursor,
                } => {
                    let mut line_count = 0_usize;
                    loop {
                        match cursor.poll().map_err(map_engine_error)? {
                            M11HostBlockQuoteCursorPoll::Line(line) => {
                                if line.continuation_prefix_start() != 0
                                    || line.continuation_prefix_end() != 0
                                    || !matches!(line.flags(), 1 | 2)
                                {
                                    return Err(HostStoreError::new(
                                        HostRejectReason::InternalFault,
                                        "block-quote cursor carried a non-quote line record",
                                    ));
                                }
                                let start = line_count
                                    .checked_mul(M11_BLOCK_QUOTE_LINE_RECORD_BYTES)
                                    .ok_or_else(|| {
                                        HostStoreError::invalid(
                                            "block-quote record offset overflowed",
                                        )
                                    })?;
                                let end = start
                                    .checked_add(M11_BLOCK_QUOTE_LINE_RECORD_BYTES)
                                    .ok_or_else(|| {
                                        HostStoreError::invalid(
                                            "block-quote record offset overflowed",
                                        )
                                    })?;
                                let record =
                                    inline_output.get_mut(start..end).ok_or_else(|| {
                                        HostStoreError::new(
                                        HostRejectReason::InternalFault,
                                        "block-quote cursor exceeded its authenticated envelope",
                                    )
                                    })?;
                                record[0..4]
                                    .copy_from_slice(&line.relative_line_start().to_le_bytes());
                                record[4..8]
                                    .copy_from_slice(&line.physical_source_length().to_le_bytes());
                                record[8..12]
                                    .copy_from_slice(&line.hidden_prefix_length().to_le_bytes());
                                record[12..16]
                                    .copy_from_slice(&line.content_length().to_le_bytes());
                                record[16..20].copy_from_slice(&line.flags().to_le_bytes());
                                line_count += 1;
                            }
                            M11HostBlockQuoteCursorPoll::Complete => break,
                        }
                    }
                    if u64::try_from(line_count).ok() != Some(descriptor.line_count())
                        || line_count.checked_mul(M11_BLOCK_QUOTE_LINE_RECORD_BYTES)
                            != Some(inline_output.len())
                    {
                        return Err(HostStoreError::new(
                            HostRejectReason::InternalFault,
                            "block-quote cursor disagrees with its authenticated descriptor",
                        ));
                    }
                    cursor.tree_nodes_visited()
                }
                M11HostInlineSidecarQuery::BulletList {
                    selected_item_ordinal,
                    selected_item_line_ending,
                    descriptor,
                    mut cursor,
                } => {
                    let item_output = match (selected_item_ordinal, selected_item_line_ending) {
                        (Some(ordinal), Some(line_ending)) => {
                            let (metadata, records) =
                                inline_output.split_at_mut(M11_BULLET_LIST_ITEM_META_BYTES);
                            metadata.fill(0);
                            metadata[..4].copy_from_slice(&ordinal.to_le_bytes());
                            metadata[4] = match line_ending {
                                M11HostCanonicalLineEnding::Lf => 1,
                                M11HostCanonicalLineEnding::CrLf => 2,
                                M11HostCanonicalLineEnding::Cr => 3,
                            };
                            records
                        }
                        (None, None) => inline_output,
                        _ => {
                            return Err(HostStoreError::new(
                                HostRejectReason::InternalFault,
                                "compact bullet item lost its canonical editing metadata",
                            ));
                        }
                    };
                    let mut item_count = 0_usize;
                    let mut selected = None;
                    let mut last = None;
                    let mut saw_terminal_empty = false;
                    loop {
                        match cursor.poll().map_err(map_engine_error)? {
                            M11HostBlockQuoteCursorPoll::Line(item) => {
                                let content_utf16 = item.bullet_content_utf16_length();
                                if saw_terminal_empty
                                    || item.hidden_prefix_length() == 0
                                    || item.continuation_prefix_start()
                                        >= item.continuation_prefix_end()
                                    || item.continuation_prefix_end() > item.hidden_prefix_length()
                                    || (item.content_length() == 0) != (content_utf16 == 0)
                                    || content_utf16 > item.content_length()
                                {
                                    return Err(HostStoreError::new(
                                        HostRejectReason::InternalFault,
                                        "bullet-list cursor carried invalid item geometry",
                                    ));
                                }
                                saw_terminal_empty = item.content_length() == 0;
                                let start = item_count
                                    .checked_mul(M11_BULLET_LIST_ITEM_RECORD_BYTES)
                                    .ok_or_else(|| {
                                        HostStoreError::invalid(
                                            "bullet-list item offset overflowed",
                                        )
                                    })?;
                                let end = start
                                    .checked_add(M11_BULLET_LIST_ITEM_RECORD_BYTES)
                                    .ok_or_else(|| {
                                        HostStoreError::invalid(
                                            "bullet-list item offset overflowed",
                                        )
                                    })?;
                                let record = item_output.get_mut(start..end).ok_or_else(|| {
                                    HostStoreError::new(
                                        HostRejectReason::InternalFault,
                                        "bullet-list cursor exceeded its authenticated envelope",
                                    )
                                })?;
                                record[0..4]
                                    .copy_from_slice(&item.relative_line_start().to_le_bytes());
                                record[4..8]
                                    .copy_from_slice(&item.physical_source_length().to_le_bytes());
                                record[8..12]
                                    .copy_from_slice(&item.hidden_prefix_length().to_le_bytes());
                                record[12..16].copy_from_slice(
                                    &item.continuation_prefix_start().to_le_bytes(),
                                );
                                record[16..20]
                                    .copy_from_slice(&item.continuation_prefix_end().to_le_bytes());
                                record[20..24]
                                    .copy_from_slice(&item.content_length().to_le_bytes());
                                record[24..28].copy_from_slice(&content_utf16.to_le_bytes());

                                let ordinal = match selected_item_ordinal {
                                    Some(ordinal) => ordinal,
                                    None => u32::try_from(item_count).map_err(|_| {
                                        HostStoreError::invalid(
                                            "bullet-list item ordinal overflowed",
                                        )
                                    })?,
                                };
                                let absolute_end = range
                                    .start
                                    .bytes
                                    .checked_add(item.relative_line_start())
                                    .and_then(|start| {
                                        start.checked_add(item.physical_source_length())
                                    })
                                    .ok_or_else(|| {
                                        HostStoreError::invalid(
                                            "bullet-list item source range overflowed",
                                        )
                                    })?;
                                let selects_here = selected_item_ordinal.is_some()
                                    || query.position.bytes < absolute_end
                                    || (query.position.bytes == absolute_end
                                        && query.affinity == HostMetricAffinity::Upstream);
                                if selected.is_none() && selects_here {
                                    selected = Some((ordinal, item));
                                }
                                last = Some((ordinal, item));
                                item_count += 1;
                            }
                            M11HostBlockQuoteCursorPoll::Complete => break,
                        }
                    }
                    if u64::try_from(item_count).ok() != Some(descriptor.line_count())
                        || item_count.checked_mul(M11_BULLET_LIST_ITEM_RECORD_BYTES)
                            != Some(item_output.len())
                    {
                        return Err(HostStoreError::new(
                            HostRejectReason::InternalFault,
                            "bullet-list cursor disagrees with its authenticated descriptor",
                        ));
                    }
                    let selected = selected.or(last).ok_or_else(|| {
                        HostStoreError::new(
                            HostRejectReason::InternalFault,
                            "bullet-list cursor contained no selected item",
                        )
                    })?;
                    if selected_item_ordinal.is_some() {
                        let item = selected.1;
                        let eol = item.physical_eol_length();
                        let expected_relative_start = binding
                            .visible_start_utf8
                            .checked_sub(binding.physical_start_utf8)
                            .ok_or_else(|| {
                                HostStoreError::invalid(
                                    "selected bullet item escaped its list binding",
                                )
                            })?;
                        let projected_utf8 =
                            item.content_length().checked_add(eol).ok_or_else(|| {
                                HostStoreError::invalid("selected item UTF-8 length overflowed")
                            })?;
                        let projected_utf16 = item
                            .bullet_content_utf16_length()
                            .checked_add(eol)
                            .ok_or_else(|| {
                            HostStoreError::invalid("selected item UTF-16 length overflowed")
                        })?;
                        let physical_utf16 = item
                            .hidden_prefix_length()
                            .checked_add(projected_utf16)
                            .ok_or_else(|| {
                                HostStoreError::invalid(
                                    "selected item physical UTF-16 length overflowed",
                                )
                            })?;
                        let canonical_eol_length =
                            selected_item_line_ending.map(|line_ending| match line_ending {
                                M11HostCanonicalLineEnding::Lf | M11HostCanonicalLineEnding::Cr => {
                                    1
                                }
                                M11HostCanonicalLineEnding::CrLf => 2,
                            });
                        if item_count != 1
                            || item.relative_line_start() != expected_relative_start
                            || item
                                .relative_line_start()
                                .checked_add(item.physical_source_length())
                                != binding
                                    .visible_end_utf8
                                    .checked_sub(binding.physical_start_utf8)
                            || binding
                                .visible_end_utf16
                                .checked_sub(binding.visible_start_utf16)
                                != Some(physical_utf16)
                            || eol != 0 && canonical_eol_length != Some(eol)
                            || descriptor.projected_utf8_length() != projected_utf8
                            || descriptor.projected_utf16_length() != projected_utf16
                        {
                            return Err(HostStoreError::new(
                                HostRejectReason::InternalFault,
                                "selected bullet item disagrees with its compact authority",
                            ));
                        }
                    }
                    if !encode_bullet_list_point_path(
                        point_path_output,
                        point_path_node_count,
                        range,
                        &green,
                        &projection,
                        selected.0,
                        selected.1,
                    ) {
                        return Err(HostStoreError::new(
                            HostRejectReason::InternalFault,
                            "bullet-list point path disagrees with its structural summary",
                        ));
                    }
                    cursor.tree_nodes_visited()
                }
                M11HostInlineSidecarQuery::OrderedList {
                    selected_item_ordinal,
                    selected_item_line_ending,
                    opening_marker_start,
                    opening_marker_end,
                    marker_value,
                    descriptor,
                    cursor,
                } => {
                    let (selected_item, tree_nodes_visited) = encode_ordered_list_item_payload(
                        inline_output,
                        binding,
                        descriptor,
                        cursor,
                        selected_item_ordinal,
                        selected_item_line_ending,
                        opening_marker_start,
                        opening_marker_end,
                        marker_value,
                    )?;
                    if !encode_ordered_list_point_path(
                        point_path_output,
                        point_path_node_count,
                        range,
                        &green,
                        &projection,
                        selected_item_ordinal,
                        selected_item,
                    ) {
                        return Err(HostStoreError::new(
                            HostRejectReason::InternalFault,
                            "ordered-list point path disagrees with its structural summary",
                        ));
                    }
                    tree_nodes_visited
                }
                M11HostInlineSidecarQuery::ProjectedInline { .. } => {
                    return Err(HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "projected-inline query reached physical viewport encoding",
                    ));
                }
                M11HostInlineSidecarQuery::Unsupported { .. } => {
                    if inline_output.len() != M11_INLINE_META_RECORD_BYTES {
                        return Err(HostStoreError::new(
                            HostRejectReason::InternalFault,
                            "unsupported sidecar viewport has trailing payload bytes",
                        ));
                    }
                    inline_output.fill(0);
                    inline_output[..8].copy_from_slice(M11_INLINE_META_MAGIC);
                    inline_output[8..12].copy_from_slice(&M11_INLINE_SCHEMA.to_le_bytes());
                    inline_output[12] = 2;
                    inline_output[16..20]
                        .copy_from_slice(&self.config.syntax_profile.to_le_bytes());
                    inline_output[24..32]
                        .copy_from_slice(&u64::from(binding.visible_start_utf8).to_le_bytes());
                    inline_output[32..40]
                        .copy_from_slice(&u64::from(binding.visible_end_utf8).to_le_bytes());
                    inline_output[40..44]
                        .copy_from_slice(&(M11_INLINE_FACT_RECORD_BYTES as u32).to_le_bytes());
                    0
                }
            };
            let projection_tree_nodes_visited = u32::try_from(projection_tree_nodes_visited)
                .map_err(|_| HostStoreError::invalid("sidecar query receipt overflowed"))?;
            if projection_tree_nodes_visited > inline_maximum_tree_nodes_visited {
                return Err(HostStoreError::new(
                    HostRejectReason::InternalFault,
                    "sidecar query exceeded its authenticated work bound",
                ));
            }
            receipt.tree_nodes_visited = receipt
                .tree_nodes_visited
                .checked_add(projection_tree_nodes_visited)
                .ok_or_else(|| HostStoreError::invalid("sidecar query receipt overflowed"))?;
            receipt.leaf_count = receipt
                .leaf_count
                .checked_add(inline_leaf_count)
                .ok_or_else(|| HostStoreError::invalid("sidecar query receipt overflowed"))?;
            receipt.open_depth = receipt.open_depth.max(inline_maximum_open_depth);
        }
        receipt.encoded_bytes = encoded_bytes_u32;
        Ok(HostStructuralQueryOutcome::Viewport {
            source_version: query.source_version,
            range,
            receipt,
        })
    }

    fn query_structural_legacy(
        &self,
        query: HostPointQuery,
        output: &mut [u8],
    ) -> Result<HostStructuralQueryOutcome, HostStoreError> {
        self.validate_point_query(query)?;
        let range = whole_source_range(query.source_version);
        let (engine, installed) = self.query_root()?;

        let green_count = engine
            .role_record_count(installed, M11HostRole::Green)
            .map_err(map_engine_error)?;
        let projection_count = engine
            .role_record_count(installed, M11HostRole::Projection)
            .map_err(map_engine_error)?;
        let persistent_inline = engine
            .persistent_inline_projection_descriptor(installed)
            .map_err(map_engine_error)?;
        if green_count != 1
            || projection_count == 0
            || persistent_inline.is_some_and(|inline| inline.structural_record_count() != 1)
        {
            return Ok(source_gap(
                query.source_version,
                range,
                HostSourceGapReason::UnavailableFacts,
                HostViewportReceipt {
                    tree_nodes_visited: 2,
                    ..HostViewportReceipt::default()
                },
            ));
        }

        let legacy_inline = if persistent_inline.is_some() || projection_count == 1 {
            None
        } else {
            let mut metadata = [0_u8; M11_INLINE_META_RECORD_BYTES];
            let written = engine
                .read_role_record(installed, M11HostRole::Projection, 1, 0, &mut metadata)
                .map_err(map_engine_error)?;
            if written != metadata.len()
                || engine
                    .read_role_record(
                        installed,
                        M11HostRole::Projection,
                        1,
                        metadata.len(),
                        &mut [0_u8; 1],
                    )
                    .map_err(map_engine_error)?
                    != 0
            {
                return Ok(source_gap(
                    query.source_version,
                    range,
                    HostSourceGapReason::UndecodableClosure,
                    HostViewportReceipt {
                        tree_nodes_visited: 2,
                        ..HostViewportReceipt::default()
                    },
                ));
            }
            match validate_inline_metadata(
                &metadata,
                self.config.syntax_profile,
                query.source_version,
                projection_count,
            ) {
                Some(inline) => Some((metadata, inline)),
                None => {
                    return Ok(source_gap(
                        query.source_version,
                        range,
                        HostSourceGapReason::UndecodableClosure,
                        HostViewportReceipt {
                            tree_nodes_visited: 2,
                            ..HostViewportReceipt::default()
                        },
                    ));
                }
            }
        };
        let inline_encoded_bytes = match (persistent_inline, legacy_inline) {
            (Some(inline), None) => usize::try_from(inline.fact_count())
                .ok()
                .and_then(|facts| facts.checked_mul(M11_INLINE_FACT_RECORD_BYTES))
                .and_then(|facts| M11_INLINE_META_RECORD_BYTES.checked_add(facts))
                .and_then(|facts| facts.checked_add(inline.link_value_encoded_bytes() as usize)),
            (None, Some((_, inline))) => Some(inline.encoded_bytes),
            (None, None) => None,
            (Some(_), Some(_)) => unreachable!("Projection schemas are mutually exclusive"),
        };
        let encoded_bytes = match inline_encoded_bytes {
            Some(inline_bytes) => M11_VIEWPORT_INLINE_HEADER_BYTES
                .checked_add(M11_GREEN_RECORD_BYTES)
                .and_then(|value| value.checked_add(M11_PROJECTION_RECORD_BYTES))
                .and_then(|value| value.checked_add(inline_bytes)),
            None => Some(HOST_M11_VIEWPORT_BYTES),
        }
        .ok_or_else(|| HostStoreError::invalid("viewport byte count overflowed"))?;
        let leaf_count_u64 = match persistent_inline {
            Some(inline) => inline.logical_page_count().checked_add(3),
            None => green_count.checked_add(projection_count),
        }
        .ok_or_else(|| HostStoreError::invalid("viewport leaf count overflowed"))?;
        let leaf_count = u32::try_from(leaf_count_u64)
            .map_err(|_| HostStoreError::invalid("viewport leaf count overflowed"))?;
        let open_depth = persistent_inline.map_or(1, |inline| inline.maximum_open_depth().max(1));
        let maximum_tree_nodes_visited_u64 = match persistent_inline {
            Some(inline) => inline.maximum_tree_nodes_visited().checked_add(3),
            None => Some(leaf_count_u64),
        }
        .ok_or_else(|| HostStoreError::invalid("viewport tree-work bound overflowed"))?;
        let maximum_tree_nodes_visited = u32::try_from(maximum_tree_nodes_visited_u64)
            .map_err(|_| HostStoreError::invalid("viewport tree-work bound overflowed"))?;
        let encoded_bytes_u32 = u32::try_from(encoded_bytes)
            .map_err(|_| HostStoreError::invalid("viewport byte count overflowed"))?;
        let gap = if query.budget.maximum_encoded_bytes < encoded_bytes_u32
            || self.config.maximum_query_bytes < encoded_bytes_u32
        {
            Some(HostSourceGapReason::EncodedByteLimit)
        } else if query.budget.maximum_open_depth < open_depth {
            Some(HostSourceGapReason::OpenDepthLimit)
        } else if query.budget.maximum_leaf_count < leaf_count {
            Some(HostSourceGapReason::LeafLimit)
        } else if query.budget.maximum_tree_nodes_visited < maximum_tree_nodes_visited {
            Some(HostSourceGapReason::TreeNodeLimit)
        } else {
            None
        };
        if let Some(reason) = gap {
            return Ok(source_gap(
                query.source_version,
                range,
                reason,
                HostViewportReceipt::default(),
            ));
        }
        if output.len() < encoded_bytes {
            return Err(HostStoreError::new(
                HostRejectReason::QueryBoundExceeded,
                "query output scratch is smaller than the admitted viewport",
            ));
        }

        let has_inline = inline_encoded_bytes.is_some();
        let header_bytes = if has_inline {
            M11_VIEWPORT_INLINE_HEADER_BYTES
        } else {
            M11_VIEWPORT_HEADER_BYTES
        };
        let (header, records) = output[..encoded_bytes].split_at_mut(header_bytes);
        let (green, projection) = records.split_at_mut(M11_GREEN_RECORD_BYTES);
        let (projection, inline_output) = projection.split_at_mut(M11_PROJECTION_RECORD_BYTES);
        let green_written = engine
            .read_role_record(installed, M11HostRole::Green, 0, 0, green)
            .map_err(map_engine_error)?;
        let projection_written = engine
            .read_role_record(installed, M11HostRole::Projection, 0, 0, projection)
            .map_err(map_engine_error)?;
        let persistent_describes_projection = persistent_inline.is_none_or(|inline| {
            projection.len() == M11_PROJECTION_RECORD_BYTES
                && projection_is_inline_bearing(projection)
                && u64::from(inline.source_start()) == read_u64(projection, 32)
                && u64::from(inline.source_end()) == read_u64(projection, 40)
        });
        if green_written != M11_GREEN_RECORD_BYTES
            || projection_written != M11_PROJECTION_RECORD_BYTES
            || !m11_records_describe_query_range(query, range, green, projection)
            || legacy_inline
                .as_ref()
                .is_some_and(|(_, facts)| !inline_describes_projection(*facts, projection))
            || !persistent_describes_projection
        {
            return Ok(source_gap(
                query.source_version,
                range,
                HostSourceGapReason::UndecodableClosure,
                HostViewportReceipt {
                    leaf_count: 2,
                    open_depth: 1,
                    tree_nodes_visited: 2,
                    ..HostViewportReceipt::default()
                },
            ));
        }

        header[..8].copy_from_slice(VIEWPORT_MAGIC);
        header[8..12].copy_from_slice(
            &if has_inline {
                M11_VIEWPORT_INLINE_SCHEMA
            } else {
                M11_ROLE_SCHEMA
            }
            .to_le_bytes(),
        );
        header[12..16].copy_from_slice(&(M11_GREEN_RECORD_BYTES as u32).to_le_bytes());
        header[16..20].copy_from_slice(&(M11_PROJECTION_RECORD_BYTES as u32).to_le_bytes());
        let mut actual_tree_nodes_visited = u64::from(leaf_count);
        if let Some((metadata, inline)) = legacy_inline {
            header[20..24].copy_from_slice(
                &u32::try_from(inline.encoded_bytes)
                    .map_err(|_| HostStoreError::invalid("inline viewport overflowed"))?
                    .to_le_bytes(),
            );
            inline_output[..M11_INLINE_META_RECORD_BYTES].copy_from_slice(&metadata);
            let mut fact_output = &mut inline_output[M11_INLINE_META_RECORD_BYTES..];
            let mut remaining_facts = inline.fact_count;
            for page_ordinal in 0..inline.page_count {
                let page_facts = remaining_facts.min(M11_INLINE_FACTS_PER_PAGE);
                let mut page_header = [0_u8; M11_INLINE_PAGE_HEADER_BYTES];
                let role_ordinal = u64::try_from(page_ordinal + 2)
                    .map_err(|_| HostStoreError::invalid("inline page ordinal overflowed"))?;
                let header_written = engine
                    .read_role_record(
                        installed,
                        M11HostRole::Projection,
                        role_ordinal,
                        0,
                        &mut page_header,
                    )
                    .map_err(map_engine_error)?;
                let fact_bytes = page_facts
                    .checked_mul(M11_INLINE_FACT_RECORD_BYTES)
                    .ok_or_else(|| HostStoreError::invalid("inline page byte count overflowed"))?;
                if header_written != page_header.len()
                    || !valid_inline_page_header(&page_header, page_ordinal, page_facts)
                    || engine
                        .read_role_record(
                            installed,
                            M11HostRole::Projection,
                            role_ordinal,
                            M11_INLINE_PAGE_HEADER_BYTES,
                            &mut fact_output[..fact_bytes],
                        )
                        .map_err(map_engine_error)?
                        != fact_bytes
                    || engine
                        .read_role_record(
                            installed,
                            M11HostRole::Projection,
                            role_ordinal,
                            M11_INLINE_PAGE_HEADER_BYTES + fact_bytes,
                            &mut [0_u8; 1],
                        )
                        .map_err(map_engine_error)?
                        != 0
                    || !valid_inline_fact_records(&fact_output[..fact_bytes], inline.leaf_bytes)
                {
                    return Ok(source_gap(
                        query.source_version,
                        range,
                        HostSourceGapReason::UndecodableClosure,
                        HostViewportReceipt {
                            leaf_count,
                            open_depth,
                            tree_nodes_visited: leaf_count,
                            ..HostViewportReceipt::default()
                        },
                    ));
                }
                fact_output = &mut fact_output[fact_bytes..];
                remaining_facts -= page_facts;
            }
            if remaining_facts != 0 || !fact_output.is_empty() {
                return Ok(source_gap(
                    query.source_version,
                    range,
                    HostSourceGapReason::UndecodableClosure,
                    HostViewportReceipt {
                        leaf_count,
                        open_depth,
                        tree_nodes_visited: leaf_count,
                        ..HostViewportReceipt::default()
                    },
                ));
            }
        } else if let Some(inline) = persistent_inline {
            let inline_bytes =
                inline_encoded_bytes.expect("persistent inline descriptor has an encoded viewport");
            header[20..24].copy_from_slice(
                &u32::try_from(inline_bytes)
                    .map_err(|_| HostStoreError::invalid("inline viewport overflowed"))?
                    .to_le_bytes(),
            );
            let (metadata, inline_payload) =
                inline_output.split_at_mut(M11_INLINE_META_RECORD_BYTES);
            metadata.fill(0);
            metadata[..8].copy_from_slice(M11_INLINE_META_MAGIC);
            metadata[8..12].copy_from_slice(&M11_INLINE_SCHEMA.to_le_bytes());
            metadata[12] = 1;
            metadata[16..20].copy_from_slice(&self.config.syntax_profile.to_le_bytes());
            metadata[20..24].copy_from_slice(
                &u32::try_from(inline.fact_count())
                    .map_err(|_| HostStoreError::invalid("inline fact count overflowed"))?
                    .to_le_bytes(),
            );
            metadata[24..32].copy_from_slice(&u64::from(inline.source_start()).to_le_bytes());
            metadata[32..40].copy_from_slice(&u64::from(inline.source_end()).to_le_bytes());
            metadata[40..44].copy_from_slice(&(M11_INLINE_FACT_RECORD_BYTES as u32).to_le_bytes());

            let fact_bytes = usize::try_from(inline.fact_count())
                .ok()
                .and_then(|count| count.checked_mul(M11_INLINE_FACT_RECORD_BYTES))
                .ok_or_else(|| HostStoreError::invalid("inline fact bytes overflowed"))?;
            let (fact_output, link_value_output) = inline_payload.split_at_mut(fact_bytes);
            let mut cursor = engine
                .persistent_inline_projection_cursor(installed)
                .map_err(map_engine_error)?
                .ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "persistent Projection descriptor lost its typed cursor",
                    )
                })?;
            let mut next_fact = 0_usize;
            loop {
                match cursor.poll().map_err(map_engine_error)? {
                    M11HostInlineProjectionCursorPoll::Fact(fact) => {
                        let start = next_fact
                            .checked_mul(M11_INLINE_FACT_RECORD_BYTES)
                            .ok_or_else(|| {
                                HostStoreError::invalid("inline fact offset overflowed")
                            })?;
                        let end =
                            start
                                .checked_add(M11_INLINE_FACT_RECORD_BYTES)
                                .ok_or_else(|| {
                                    HostStoreError::invalid("inline fact offset overflowed")
                                })?;
                        let record = fact_output.get_mut(start..end).ok_or_else(|| {
                            HostStoreError::new(
                                HostRejectReason::InternalFault,
                                "typed inline cursor exceeded its descriptor",
                            )
                        })?;
                        encode_inline_projection_fact_record(fact, record)?;
                        next_fact += 1;
                    }
                    M11HostInlineProjectionCursorPoll::Complete => break,
                }
            }
            let copied_link_value_entries = engine
                .copy_persistent_inline_link_values(installed, link_value_output)
                .map_err(map_engine_error)?
                .ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "persistent Projection descriptor lost its link-value lane",
                    )
                })?;
            let inline_tree_nodes_visited = cursor
                .tree_nodes_visited()
                .checked_add(copied_link_value_entries.tree_nodes_visited)
                .and_then(|visited| visited.checked_add(1))
                .ok_or_else(|| HostStoreError::invalid("inline tree receipt overflowed"))?;
            if u64::try_from(next_fact).ok() != Some(inline.fact_count())
                || next_fact
                    .checked_mul(M11_INLINE_FACT_RECORD_BYTES)
                    .is_none_or(|bytes| bytes != fact_output.len())
                || copied_link_value_entries.entry_count != inline.link_value_entry_count()
                || link_value_output.len() != inline.link_value_encoded_bytes() as usize
                || inline_tree_nodes_visited > inline.maximum_tree_nodes_visited()
            {
                return Ok(source_gap(
                    query.source_version,
                    range,
                    HostSourceGapReason::UndecodableClosure,
                    HostViewportReceipt {
                        leaf_count,
                        open_depth,
                        tree_nodes_visited: u32::try_from(
                            inline_tree_nodes_visited.min(u64::from(u32::MAX)),
                        )
                        .expect("clamped tree receipt"),
                        ..HostViewportReceipt::default()
                    },
                ));
            }
            actual_tree_nodes_visited = inline_tree_nodes_visited
                .checked_add(3)
                .ok_or_else(|| HostStoreError::invalid("tree receipt overflowed"))?;
        }
        Ok(HostStructuralQueryOutcome::Viewport {
            source_version: query.source_version,
            range,
            receipt: HostViewportReceipt {
                encoded_bytes: encoded_bytes_u32,
                leaf_count,
                open_depth,
                tree_nodes_visited: u32::try_from(actual_tree_nodes_visited)
                    .map_err(|_| HostStoreError::invalid("tree receipt overflowed"))?,
                summary_nodes_skipped: 0,
            },
        })
    }

    fn validate_point_query(&self, query: HostPointQuery) -> Result<(), HostStoreError> {
        self.require_open()?;
        let current = self.current_source.ok_or_else(|| {
            HostStoreError::new(HostRejectReason::NotReady, "no exact source was observed")
        })?;
        if query.source_version != current {
            return Err(HostStoreError::new(
                HostRejectReason::ExactSourceMismatch,
                "query does not bind the exact current source",
            ));
        }
        let position = query.position;
        let end = HostSourceMetric {
            bytes: current.utf8_length,
            utf16: current.utf16_length,
        };
        let starts_together = (position.bytes == 0) == (position.utf16 == 0);
        let ends_together = (position.bytes == end.bytes) == (position.utf16 == end.utf16);
        let prefix_can_be_utf8 = position.bytes >= position.utf16;
        let suffix_can_be_utf8 = end
            .bytes
            .checked_sub(position.bytes)
            .zip(end.utf16.checked_sub(position.utf16))
            .is_some_and(|(bytes, utf16)| bytes >= utf16);
        if position.bytes > end.bytes
            || position.utf16 > end.utf16
            || !starts_together
            || !ends_together
            || !prefix_can_be_utf8
            || !suffix_can_be_utf8
        {
            return Err(HostStoreError::new(
                HostRejectReason::Invalid,
                "query position is not a possible exact UTF-8/UTF-16 source cut",
            ));
        }
        Ok(())
    }

    fn validate_block_range_query(&self, query: HostBlockRangeQuery) -> Result<(), HostStoreError> {
        self.require_open()?;
        let current = self.current_source.ok_or_else(|| {
            HostStoreError::new(HostRejectReason::NotReady, "no exact source was observed")
        })?;
        if query.source_version != current {
            return Err(HostStoreError::new(
                HostRejectReason::ExactSourceMismatch,
                "range query does not bind the exact current source",
            ));
        }
        if query.budget.maximum_encoded_bytes == 0
            || query.budget.maximum_block_count == 0
            || query.budget.maximum_storage_pages_visited == 0
            || query.budget.maximum_open_depth == 0
            || query.budget.maximum_tree_nodes_visited == 0
        {
            return Err(HostStoreError::invalid(
                "range query budgets must be nonzero",
            ));
        }
        let range = query.requested_range;
        let end = HostSourceMetric {
            bytes: current.utf8_length,
            utf16: current.utf16_length,
        };
        let reversed = range.start.bytes > range.end.bytes || range.start.utf16 > range.end.utf16;
        let metrics_disagree =
            (range.start.bytes == range.end.bytes) != (range.start.utf16 == range.end.utf16);
        let empty_nonempty_source = range.start == range.end && (end.bytes != 0 || end.utf16 != 0);
        if reversed
            || metrics_disagree
            || empty_nonempty_source
            || !host_source_cut_is_possible(range.start, end)
            || !host_source_cut_is_possible(range.end, end)
        {
            return Err(HostStoreError::invalid(
                "range query is not a possible exact UTF-8/UTF-16 source range",
            ));
        }
        if query.continuation.is_some() && range.start == range.end {
            return Err(HostStoreError::invalid(
                "empty source range cannot own a continuation",
            ));
        }
        Ok(())
    }

    fn validate_structural_ordinal_window_query(
        &self,
        query: HostStructuralOrdinalWindowQuery,
    ) -> Result<(), HostStoreError> {
        self.require_open()?;
        let current = self.current_source.ok_or_else(|| {
            HostStoreError::new(HostRejectReason::NotReady, "no exact source was observed")
        })?;
        if query.source_version.document_session == current.document_session
            && query.source_version.revision < current.revision
        {
            return Err(HostStoreError::new(
                HostRejectReason::StaleSource,
                "ordinal-window query targets an older source",
            ));
        }
        if query.source_version != current {
            return Err(HostStoreError::new(
                HostRejectReason::ExactSourceMismatch,
                "ordinal-window query does not bind the exact current source",
            ));
        }
        Ok(())
    }

    pub fn begin_close(&mut self) -> Result<(), HostStoreError> {
        if self.closed || self.closing {
            return Ok(());
        }
        self.viewport_presentation.begin_close();
        self.active = None;
        self.active_inline_sidecar = None;
        self.aborting_offer = None;
        self.aborting_inline_sidecar_offer = None;
        self.pending_delivery_ack = None;
        self.pending_inline_sidecar_delivery_ack = None;
        self.installed_inline_sidecar = None;
        if let Some(engine) = self.engine.as_mut() {
            engine.begin_close().map_err(map_engine_error)?;
        }
        if let Some(sidecar) = self.inline_sidecar.as_mut() {
            sidecar.begin_close().map_err(map_engine_error)?;
        }
        self.closing = true;
        if self.engine.is_none()
            && self.inline_sidecar.is_none()
            && self.viewport_presentation.poll_close(0)?
        {
            self.closed = true;
        }
        Ok(())
    }

    fn poll_pending_packet(
        &mut self,
        grant: HostWorkGrant,
    ) -> Result<HostPollOutcome, HostStoreError> {
        if self.background_reclaim_pending {
            if grant.transitions == 0 {
                return Ok(HostPollOutcome::Pending);
            }
            let complete = self.engine.as_mut().map_or(Ok(true), |engine| {
                engine
                    .poll_reclaim(grant.transitions as usize)
                    .map_err(map_engine_error)
            })?;
            self.background_reclaim_pending = !complete;
            // Reclamation owns this grant. The one copied packet and its cursor
            // remain live for a later bounded poll.
            return Ok(HostPollOutcome::Pending);
        }
        if self.inline_sidecar_reclaim_pending {
            if grant.transitions == 0 {
                return Ok(HostPollOutcome::Pending);
            }
            let complete = self.inline_sidecar.as_mut().map_or(Ok(true), |sidecar| {
                sidecar
                    .poll_reclaim(grant.transitions as usize)
                    .map_err(map_engine_error)
            })?;
            self.inline_sidecar_reclaim_pending = !complete;
            // Structural replacement may invalidate the current sidecar only
            // after its prior sibling has fully retired. Preserve the copied
            // structural packet and spend this quantum on that prerequisite.
            return Ok(HostPollOutcome::Pending);
        }

        let mut packet = self
            .active
            .as_mut()
            .and_then(|active| active.pending_packet.take())
            .ok_or_else(|| HostStoreError::invalid("pending packet disappeared"))?;
        let result = self.process_packet_frames(&mut packet, grant);
        match result {
            Ok(false) => {
                self.active
                    .as_mut()
                    .ok_or_else(|| HostStoreError::invalid("active offer disappeared"))?
                    .pending_packet = Some(packet);
                Ok(HostPollOutcome::Pending)
            }
            Ok(true) => {
                let offer_id = packet.offer_id;
                if let Some((start, end)) = packet.end_range {
                    self.active
                        .as_mut()
                        .ok_or_else(|| HostStoreError::invalid("active offer disappeared"))?
                        .retained_end = Some(RetainedFrame {
                        storage: packet.bytes,
                        start,
                        end,
                    });
                }
                let next_frame_ordinal = self
                    .active
                    .as_ref()
                    .ok_or_else(|| HostStoreError::invalid("active offer disappeared"))?
                    .next_frame_ordinal;
                Ok(HostPollOutcome::PacketCredit {
                    offer_id,
                    next_frame_ordinal,
                })
            }
            Err(error) => {
                self.fail_active_offer();
                Err(error)
            }
        }
    }

    fn poll_inline_sidecar_pending_packet(
        &mut self,
        grant: HostWorkGrant,
    ) -> Result<InlineSidecarHostPollOutcome, HostStoreError> {
        if self.inline_sidecar_reclaim_pending {
            if grant.transitions == 0 {
                return Ok(InlineSidecarHostPollOutcome::Pending);
            }
            let complete = self.inline_sidecar.as_mut().map_or(Ok(true), |sidecar| {
                sidecar
                    .poll_reclaim(grant.transitions as usize)
                    .map_err(map_engine_error)
            })?;
            self.inline_sidecar_reclaim_pending = !complete;
            return Ok(InlineSidecarHostPollOutcome::Pending);
        }

        let mut packet = self
            .active_inline_sidecar
            .as_mut()
            .and_then(|active| active.pending_packet.take())
            .ok_or_else(|| HostStoreError::invalid("pending sidecar packet disappeared"))?;
        let result = self.process_inline_sidecar_packet_frames(&mut packet, grant);
        match result {
            Ok(false) => {
                self.active_inline_sidecar
                    .as_mut()
                    .ok_or_else(|| HostStoreError::invalid("active sidecar offer disappeared"))?
                    .pending_packet = Some(packet);
                Ok(InlineSidecarHostPollOutcome::Pending)
            }
            Ok(true) => {
                let offer_id = packet.offer_id;
                if let Some((start, end)) = packet.end_range {
                    self.active_inline_sidecar
                        .as_mut()
                        .ok_or_else(|| HostStoreError::invalid("active sidecar offer disappeared"))?
                        .retained_end = Some(RetainedFrame {
                        storage: packet.bytes,
                        start,
                        end,
                    });
                }
                let next_frame_ordinal = self
                    .active_inline_sidecar
                    .as_ref()
                    .ok_or_else(|| HostStoreError::invalid("active sidecar offer disappeared"))?
                    .next_frame_ordinal;
                Ok(InlineSidecarHostPollOutcome::PacketCredit {
                    offer_id,
                    next_frame_ordinal,
                })
            }
            Err(error) => {
                self.fail_active_inline_sidecar_offer();
                Err(error)
            }
        }
    }

    fn process_inline_sidecar_packet_frames(
        &mut self,
        packet: &mut OwnedPacket,
        mut grant: HostWorkGrant,
    ) -> Result<bool, HostStoreError> {
        let directory_bytes = usize::try_from(packet.frame_count)
            .ok()
            .and_then(|count| count.checked_mul(PACKET_FRAME_DESCRIPTOR_BYTES))
            .ok_or_else(|| HostStoreError::invalid("sidecar packet directory overflowed"))?;
        let body_start = PACKET_HEADER_BYTES
            .checked_add(directory_bytes)
            .ok_or_else(|| HostStoreError::invalid("sidecar packet body offset overflowed"))?;
        let expected_packet_bytes = body_start
            .checked_add(packet.aggregate_frame_bytes as usize)
            .ok_or_else(|| HostStoreError::invalid("sidecar packet byte envelope overflowed"))?;
        if packet.bytes.len() != expected_packet_bytes {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "copied sidecar packet envelope changed",
            ));
        }

        while grant.transitions > 0 && packet.next_index < packet.frame_count {
            if grant.inspect_bytes < PACKET_FRAME_DESCRIPTOR_BYTES as u32 {
                break;
            }
            let descriptor_start = PACKET_HEADER_BYTES
                .checked_add(packet.directory_offset)
                .ok_or_else(|| HostStoreError::invalid("sidecar descriptor offset overflowed"))?;
            let descriptor_end = descriptor_start
                .checked_add(PACKET_FRAME_DESCRIPTOR_BYTES)
                .filter(|end| *end <= body_start)
                .ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "sidecar descriptor table ended early",
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
            if frame_bytes == 0 {
                return Err(HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "sidecar packet contains an empty frame",
                ));
            }
            let maximum_frame_bytes = self
                .active_inline_sidecar
                .as_ref()
                .ok_or_else(|| HostStoreError::invalid("active sidecar offer disappeared"))?
                .begin
                .limits
                .maximum_frame_bytes;
            if frame_bytes > maximum_frame_bytes
                || usize::try_from(frame_bytes)
                    .is_ok_and(|bytes| bytes > M11_HOST_MAXIMUM_FRAME_BYTES)
            {
                return Err(HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "sidecar frame exceeds the admitted envelope",
                ));
            }
            let frame_start = body_start
                .checked_add(packet.body_offset)
                .ok_or_else(|| HostStoreError::invalid("sidecar frame offset overflowed"))?;
            let frame_end = frame_start
                .checked_add(frame_bytes as usize)
                .filter(|end| *end <= packet.bytes.len())
                .ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "sidecar frame lengths exceed aggregate body",
                    )
                })?;
            let inspect_bytes = frame_bytes
                .checked_add(PACKET_FRAME_DESCRIPTOR_BYTES as u32)
                .ok_or_else(|| HostStoreError::invalid("sidecar inspection fuel overflowed"))?;
            if grant.inspect_bytes < inspect_bytes || grant.copy_bytes < frame_bytes {
                break;
            }
            let ordinal = packet
                .first_frame_ordinal
                .checked_add(packet.next_index)
                .ok_or_else(|| HostStoreError::invalid("sidecar frame ordinal overflowed"))?;
            let next_record_ordinal = packet
                .next_record_ordinal
                .checked_add(record_count)
                .ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "sidecar node ordinals overflowed",
                    )
                })?;
            let is_end = self.process_inline_sidecar_frame(PendingFrame {
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
            if is_end {
                packet.end_range = Some((frame_start, frame_end));
            }
            grant.inspect_bytes -= inspect_bytes;
            grant.copy_bytes -= frame_bytes;
            grant.transitions -= 1;
        }

        if packet.next_index < packet.frame_count {
            return Ok(false);
        }
        let expected_node_ordinal = packet
            .first_record_ordinal
            .checked_add(packet.aggregate_record_count)
            .ok_or_else(|| HostStoreError::invalid("sidecar node aggregate overflowed"))?;
        let expected_accepted_frame_bytes = packet
            .first_accepted_frame_bytes
            .checked_add(packet.aggregate_frame_bytes)
            .ok_or_else(|| HostStoreError::invalid("sidecar byte aggregate overflowed"))?;
        let active = self
            .active_inline_sidecar
            .as_ref()
            .ok_or_else(|| HostStoreError::invalid("active sidecar offer disappeared"))?;
        if packet.directory_offset != directory_bytes
            || packet.body_offset != packet.aggregate_frame_bytes as usize
            || packet.next_record_ordinal != expected_node_ordinal
            || active.next_frame_ordinal
                != packet
                    .first_frame_ordinal
                    .checked_add(packet.frame_count)
                    .ok_or_else(|| HostStoreError::invalid("sidecar frame aggregate overflowed"))?
            || active.accepted_node_count != expected_node_ordinal
            || active.accepted_frame_bytes != expected_accepted_frame_bytes
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "sidecar packet descriptor aggregates changed",
            ));
        }
        Ok(true)
    }

    /// Processes as many whole frames as fit the grant. A descriptor or body
    /// cursor is never advanced for a frame that does not fit in full.
    fn process_packet_frames(
        &mut self,
        packet: &mut OwnedPacket,
        mut grant: HostWorkGrant,
    ) -> Result<bool, HostStoreError> {
        let directory_bytes = usize::try_from(packet.frame_count)
            .ok()
            .and_then(|count| count.checked_mul(PACKET_FRAME_DESCRIPTOR_BYTES))
            .ok_or_else(|| HostStoreError::invalid("packet directory length overflowed"))?;
        let body_start = PACKET_HEADER_BYTES
            .checked_add(directory_bytes)
            .ok_or_else(|| HostStoreError::invalid("packet body offset overflowed"))?;
        let expected_packet_bytes = body_start
            .checked_add(packet.aggregate_frame_bytes as usize)
            .ok_or_else(|| HostStoreError::invalid("packet byte envelope overflowed"))?;
        if packet.bytes.len() != expected_packet_bytes {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "copied packet envelope changed",
            ));
        }

        while grant.transitions > 0 {
            if self
                .active
                .as_ref()
                .is_some_and(|active| active.exact_replay_required)
            {
                let replay = self
                    .engine
                    .as_mut()
                    .ok_or_else(|| HostStoreError::invalid("host engine was not initialized"))?
                    .poll_exact_base_delta_replay(1)
                    .map_err(map_engine_error)?;
                let consumed = u32::try_from(replay.transitions)
                    .map_err(|_| HostStoreError::invalid("replay work overflowed"))?;
                if consumed > grant.transitions
                    || (consumed == 0
                        && !replay.ready_for_replacement_page
                        && !replay.ready_for_nodes)
                {
                    return Err(HostStoreError::invalid(
                        "exact-base replay violated its bounded poll contract",
                    ));
                }
                grant.transitions -= consumed;
                if replay.ready_for_replacement_page || replay.ready_for_nodes {
                    self.active
                        .as_mut()
                        .ok_or_else(|| HostStoreError::invalid("active offer disappeared"))?
                        .exact_replay_required = false;
                }
                if consumed != 0
                    || self
                        .active
                        .as_ref()
                        .is_some_and(|active| active.exact_replay_required)
                {
                    continue;
                }
            }
            if packet.next_index >= packet.frame_count {
                break;
            }
            if grant.inspect_bytes < PACKET_FRAME_DESCRIPTOR_BYTES as u32 {
                break;
            }
            let descriptor_start = PACKET_HEADER_BYTES
                .checked_add(packet.directory_offset)
                .ok_or_else(|| HostStoreError::invalid("packet descriptor offset overflowed"))?;
            let descriptor_end = descriptor_start
                .checked_add(PACKET_FRAME_DESCRIPTOR_BYTES)
                .filter(|end| *end <= body_start)
                .ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "packet descriptor table ended early",
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
            if frame_bytes == 0 {
                return Err(HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "packet contains an empty frame",
                ));
            }
            let maximum_frame_bytes = self
                .active
                .as_ref()
                .ok_or_else(|| HostStoreError::invalid("active offer disappeared"))?
                .begin
                .limits
                .maximum_frame_bytes;
            if frame_bytes > maximum_frame_bytes
                || usize::try_from(frame_bytes)
                    .is_ok_and(|bytes| bytes > M11_HOST_MAXIMUM_FRAME_BYTES)
            {
                return Err(HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "packet frame exceeds the admitted frame envelope",
                ));
            }

            let frame_start = body_start
                .checked_add(packet.body_offset)
                .ok_or_else(|| HostStoreError::invalid("packet frame offset overflowed"))?;
            let frame_end = frame_start
                .checked_add(frame_bytes as usize)
                .filter(|end| *end <= packet.bytes.len())
                .ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "packet frame lengths exceed the aggregate body",
                    )
                })?;
            let inspect_bytes = frame_bytes
                .checked_add(PACKET_FRAME_DESCRIPTOR_BYTES as u32)
                .ok_or_else(|| HostStoreError::invalid("frame inspection fuel overflowed"))?;
            if grant.inspect_bytes < inspect_bytes || grant.copy_bytes < frame_bytes {
                break;
            }

            let ordinal = packet
                .first_frame_ordinal
                .checked_add(packet.next_index)
                .ok_or_else(|| HostStoreError::invalid("packet frame ordinal overflowed"))?;
            let next_record_ordinal = packet
                .next_record_ordinal
                .checked_add(record_count)
                .ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "packet record ordinals overflowed",
                    )
                })?;
            let is_end = self.process_frame(PendingFrame {
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
            if is_end {
                packet.end_range = Some((frame_start, frame_end));
            }
            grant.inspect_bytes -= inspect_bytes;
            grant.copy_bytes -= frame_bytes;
            grant.transitions -= 1;
        }

        if packet.next_index < packet.frame_count
            || self
                .active
                .as_ref()
                .is_some_and(|active| active.exact_replay_required)
        {
            return Ok(false);
        }
        let expected_record_ordinal = packet
            .first_record_ordinal
            .checked_add(packet.aggregate_record_count)
            .ok_or_else(|| HostStoreError::invalid("packet record aggregate overflowed"))?;
        let expected_accepted_frame_bytes = packet
            .first_accepted_frame_bytes
            .checked_add(packet.aggregate_frame_bytes)
            .ok_or_else(|| HostStoreError::invalid("packet byte aggregate overflowed"))?;
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| HostStoreError::invalid("active offer disappeared"))?;
        if packet.directory_offset != directory_bytes
            || packet.body_offset != packet.aggregate_frame_bytes as usize
            || packet.next_record_ordinal != expected_record_ordinal
            || active.next_frame_ordinal
                != packet
                    .first_frame_ordinal
                    .checked_add(packet.frame_count)
                    .ok_or_else(|| HostStoreError::invalid("packet frame aggregate overflowed"))?
            || active.accepted_record_count != expected_record_ordinal
            || active.accepted_frame_bytes != expected_accepted_frame_bytes
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "packet descriptor aggregates changed",
            ));
        }
        Ok(true)
    }

    /// Performs the independent per-frame checks inside a packet. Returns
    /// whether this frame was snapshot End; the caller retains its already
    /// owned packet storage without another copy.
    fn process_frame(&mut self, frame: PendingFrame<'_>) -> Result<bool, HostStoreError> {
        let active = self
            .active
            .as_mut()
            .ok_or_else(|| HostStoreError::invalid("active offer disappeared"))?;
        let metadata = M11CandidateHost::classify_frame(frame.bytes).map_err(map_engine_error)?;
        let protocol_kind = match metadata.kind {
            M11HostFrameKind::Begin => CandidateSnapshotFrameKind::Begin,
            M11HostFrameKind::Node => CandidateSnapshotFrameKind::Node,
            M11HostFrameKind::End => CandidateSnapshotFrameKind::End,
            M11HostFrameKind::SourceFactsReplacementPage => {
                CandidateSnapshotFrameKind::SourceFactsReplacementPage
            }
            M11HostFrameKind::BlockSequenceReplacementPage => {
                CandidateSnapshotFrameKind::BlockSequenceReplacementPage
            }
            M11HostFrameKind::RecursiveGreenReplacementPage => {
                CandidateSnapshotFrameKind::RecursiveGreenReplacementPage
            }
        };
        let byte_len = u32::try_from(frame.bytes.len()).map_err(|_| {
            HostStoreError::new(
                HostRejectReason::ForegroundBoundExceeded,
                "frame length exceeds the host target",
            )
        })?;
        if frame.offer_id != active.begin.offer_id
            || frame.ordinal != active.next_frame_ordinal
            || metadata.canonical_record_count != frame.record_count
            || frame.first_record_ordinal != active.accepted_record_count
            || frame
                .first_record_ordinal
                .checked_add(frame.record_count)
                .is_none_or(|end| end > active.begin.transferred_record_count)
            || byte_len > active.begin.limits.maximum_frame_bytes
            || frame.bytes.len() > M11_HOST_MAXIMUM_FRAME_BYTES
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "transport and engine canonical record metadata disagree",
            ));
        }
        match metadata.kind {
            M11HostFrameKind::Begin if frame.ordinal == 0 => {
                let engine = self
                    .engine
                    .as_mut()
                    .ok_or_else(|| HostStoreError::invalid("host engine was not initialized"))?;
                match (active.begin.mode, active.exact_base) {
                    (PublicationMode::FullSnapshot, None) => {
                        engine
                            .begin_snapshot(frame.bytes)
                            .map_err(map_engine_error)?;
                    }
                    (PublicationMode::ExactBaseReferencesDelta, Some(exact_base)) => {
                        engine
                            .begin_references_delta(exact_base, frame.bytes)
                            .map_err(map_engine_error)?;
                    }
                    (PublicationMode::ExactBaseDelta, Some(exact_base)) => {
                        engine
                            .begin_exact_base_delta(exact_base, frame.bytes)
                            .map_err(map_engine_error)?;
                        active.exact_replay_required = true;
                    }
                    _ => {
                        return Err(HostStoreError::new(
                            HostRejectReason::BaseMismatch,
                            "offer mode lost its exact installed base",
                        ));
                    }
                }
                let authority = engine.active_authority().ok_or_else(|| {
                    HostStoreError::invalid("accepted Begin lost its active authority")
                })?;
                if authority.publication_identity != id128_bytes(active.begin.publication_session)
                    || authority.parse_generation != u64::from(active.begin.parse_generation)
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::ExactSourceMismatch,
                        "Begin frame does not bind the declared publication authority",
                    ));
                }
            }
            M11HostFrameKind::Node
                if frame.ordinal > 0 && active.canonical_stream_digest256.is_none() =>
            {
                let node_ordinal = metadata.node_ordinal.ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "node frame lost its engine ordinal",
                    )
                })?;
                if active
                    .next_node_ordinal
                    .is_some_and(|expected| node_ordinal != expected)
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "node ordinal changed",
                    ));
                }
                active.next_node_ordinal = Some(node_ordinal.checked_add(1).ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "node ordinal overflowed",
                    )
                })?);
                self.engine
                    .as_mut()
                    .ok_or_else(|| HostStoreError::invalid("host engine was not initialized"))?
                    .offer_node(frame.bytes)
                    .map_err(map_engine_error)?;
            }
            M11HostFrameKind::SourceFactsReplacementPage
                if frame.ordinal > 0
                    && active.begin.mode == PublicationMode::ExactBaseDelta
                    && active.canonical_stream_digest256.is_none() =>
            {
                self.engine
                    .as_mut()
                    .ok_or_else(|| HostStoreError::invalid("host engine was not initialized"))?
                    .offer_source_facts_replacement_page(frame.bytes)
                    .map_err(map_engine_error)?;
                active.exact_replay_required = true;
            }
            M11HostFrameKind::BlockSequenceReplacementPage
                if frame.ordinal > 0
                    && active.begin.mode == PublicationMode::ExactBaseDelta
                    && active.canonical_stream_digest256.is_none() =>
            {
                self.engine
                    .as_mut()
                    .ok_or_else(|| HostStoreError::invalid("host engine was not initialized"))?
                    .offer_block_sequence_replacement_page(frame.bytes)
                    .map_err(map_engine_error)?;
                active.exact_replay_required = true;
            }
            M11HostFrameKind::RecursiveGreenReplacementPage
                if frame.ordinal > 0
                    && active.begin.mode == PublicationMode::ExactBaseDelta
                    && active.canonical_stream_digest256.is_none() =>
            {
                self.engine
                    .as_mut()
                    .ok_or_else(|| HostStoreError::invalid("host engine was not initialized"))?
                    .offer_recursive_green_replacement_page(frame.bytes)
                    .map_err(map_engine_error)?;
                active.exact_replay_required = true;
            }
            M11HostFrameKind::End
                if frame.ordinal > 0
                    && active.canonical_stream_digest256.is_none()
                    && metadata.canonical_stream_digest256.is_some() =>
            {
                active.canonical_stream_digest256 = metadata.canonical_stream_digest256;
                active.phase = OfferPhase::AwaitingCommit;
            }
            _ => {
                return Err(HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "snapshot frame kind or ordinal changed",
                ));
            }
        }
        let transport = active
            .transport
            .as_mut()
            .ok_or_else(|| HostStoreError::invalid("transport digest is unavailable"))?;
        let digest256 = transport
            .push(
                frame.ordinal,
                frame.first_record_ordinal,
                frame.record_count,
                protocol_kind,
                frame.bytes,
            )
            .map_err(|_| {
                HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "transport digest sequence overflowed",
                )
            })?;
        if protocol_digest128_from_blake3(ProtocolDigestDomain::CandidateFrame, digest256)
            != frame.digest
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "frame digest changed",
            ));
        }
        active.next_frame_ordinal = active
            .next_frame_ordinal
            .checked_add(1)
            .ok_or_else(|| HostStoreError::invalid("frame ordinal overflow"))?;
        active.accepted_record_count = active
            .accepted_record_count
            .checked_add(frame.record_count)
            .ok_or_else(|| HostStoreError::invalid("record count overflow"))?;
        active.accepted_frame_bytes = active
            .accepted_frame_bytes
            .checked_add(byte_len)
            .ok_or_else(|| HostStoreError::invalid("encoded byte count overflow"))?;
        Ok(metadata.kind == M11HostFrameKind::End)
    }

    fn process_inline_sidecar_frame(
        &mut self,
        frame: PendingFrame<'_>,
    ) -> Result<bool, HostStoreError> {
        let active = self
            .active_inline_sidecar
            .as_mut()
            .ok_or_else(|| HostStoreError::invalid("active sidecar offer disappeared"))?;
        let byte_len = u32::try_from(frame.bytes.len()).map_err(|_| {
            HostStoreError::new(
                HostRejectReason::ForegroundBoundExceeded,
                "sidecar frame length exceeds the host target",
            )
        })?;
        if frame.offer_id != active.begin.offer_id
            || frame.ordinal != active.next_frame_ordinal
            || frame.first_record_ordinal != active.accepted_node_count
            || frame
                .first_record_ordinal
                .checked_add(frame.record_count)
                .is_none_or(|end| end > active.begin.envelope.transferred_node_count)
            || byte_len > active.begin.limits.maximum_frame_bytes
            || frame.bytes.len() > M11_HOST_MAXIMUM_FRAME_BYTES
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "sidecar transport metadata disagrees with its offer",
            ));
        }

        let (protocol_kind, stream_digest, is_end) = if frame.ordinal == 0 {
            if frame.record_count != 0 {
                return Err(HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "sidecar Begin claimed a transferred node",
                ));
            }
            validate_inline_sidecar_begin_frame(frame.bytes, active.begin.envelope)?;
            self.inline_sidecar
                .as_mut()
                .ok_or_else(|| HostStoreError::invalid("sidecar engine was not initialized"))?
                .begin_snapshot(active.binding.clone(), frame.bytes)
                .map_err(map_engine_error)?;
            (HotInlineSidecarFrameKind::Begin, None, false)
        } else {
            let metadata =
                M11CandidateHost::classify_frame(frame.bytes).map_err(map_engine_error)?;
            match metadata.kind {
                M11HostFrameKind::Node
                    if active.root_stream_digest256.is_none() && frame.record_count == 1 =>
                {
                    let node_ordinal = metadata.node_ordinal.ok_or_else(|| {
                        HostStoreError::new(
                            HostRejectReason::CorruptPublication,
                            "sidecar Node lost its engine ordinal",
                        )
                    })?;
                    if active
                        .next_node_ordinal
                        .is_some_and(|expected| node_ordinal != expected)
                    {
                        return Err(HostStoreError::new(
                            HostRejectReason::CorruptPublication,
                            "sidecar Node ordinal changed",
                        ));
                    }
                    active.next_node_ordinal =
                        Some(node_ordinal.checked_add(1).ok_or_else(|| {
                            HostStoreError::new(
                                HostRejectReason::CorruptPublication,
                                "sidecar Node ordinal overflowed",
                            )
                        })?);
                    self.inline_sidecar
                        .as_mut()
                        .ok_or_else(|| {
                            HostStoreError::invalid("sidecar engine was not initialized")
                        })?
                        .offer_node(frame.bytes)
                        .map_err(map_engine_error)?;
                    (HotInlineSidecarFrameKind::Node, None, false)
                }
                M11HostFrameKind::End
                    if active.root_stream_digest256.is_none()
                        && frame.record_count == 0
                        && metadata.canonical_stream_digest256.is_some() =>
                {
                    active.root_stream_digest256 = metadata.canonical_stream_digest256;
                    active.phase = OfferPhase::AwaitingCommit;
                    (
                        HotInlineSidecarFrameKind::End,
                        metadata.canonical_stream_digest256,
                        true,
                    )
                }
                _ => {
                    return Err(HostStoreError::new(
                        HostRejectReason::CorruptPublication,
                        "sidecar frame kind or ordinal changed",
                    ));
                }
            }
        };

        let transport = active
            .transport
            .as_mut()
            .ok_or_else(|| HostStoreError::invalid("sidecar transport digest is unavailable"))?;
        let digest256 = transport
            .push(
                frame.ordinal,
                frame.first_record_ordinal,
                frame.record_count,
                protocol_kind,
                frame.bytes,
            )
            .map_err(|_| {
                HostStoreError::new(
                    HostRejectReason::CorruptPublication,
                    "sidecar transport digest sequence overflowed",
                )
            })?;
        if protocol_digest128_from_blake3(ProtocolDigestDomain::HotInlineSidecarFrame, digest256)
            != frame.digest
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "sidecar frame digest changed",
            ));
        }
        if is_end && stream_digest != active.root_stream_digest256 {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "sidecar End stream digest changed",
            ));
        }
        active.next_frame_ordinal = active
            .next_frame_ordinal
            .checked_add(1)
            .ok_or_else(|| HostStoreError::invalid("sidecar frame ordinal overflow"))?;
        active.accepted_node_count = active
            .accepted_node_count
            .checked_add(frame.record_count)
            .ok_or_else(|| HostStoreError::invalid("sidecar node count overflow"))?;
        active.accepted_frame_bytes = active
            .accepted_frame_bytes
            .checked_add(byte_len)
            .ok_or_else(|| HostStoreError::invalid("sidecar byte count overflow"))?;
        Ok(is_end)
    }

    fn fail_active_offer(&mut self) {
        if let Some(active) = self.active.as_mut() {
            active.phase = OfferPhase::Failed;
            active.pending_packet = None;
        }
        if let Ok(aborted) = self.abort_engine_snapshot() {
            self.background_reclaim_pending |= aborted;
        }
    }

    fn fail_active_inline_sidecar_offer(&mut self) {
        if let Some(active) = self.active_inline_sidecar.as_mut() {
            active.phase = OfferPhase::Failed;
            active.pending_packet = None;
        }
        if let Ok(aborted) = self.abort_inline_sidecar_engine_snapshot() {
            self.inline_sidecar_reclaim_pending |= aborted;
        }
    }

    fn poll_install(&mut self, transitions: u32) -> Result<HostPollOutcome, HostStoreError> {
        if transitions == 0 {
            return Ok(HostPollOutcome::Pending);
        }
        let poll = self
            .engine
            .as_mut()
            .ok_or_else(|| HostStoreError::invalid("host engine was not initialized"))?
            .poll_install(transitions as usize)
            .map_err(map_engine_error)?;
        let Some(installed) = poll.installed else {
            return Ok(HostPollOutcome::Pending);
        };
        self.finish_install(installed)
    }

    fn poll_inline_sidecar_install(
        &mut self,
        transitions: u32,
    ) -> Result<InlineSidecarHostPollOutcome, HostStoreError> {
        if transitions == 0 {
            return Ok(InlineSidecarHostPollOutcome::Pending);
        }
        let poll = self
            .inline_sidecar
            .as_mut()
            .ok_or_else(|| HostStoreError::invalid("sidecar engine was not initialized"))?
            .poll_install(transitions as usize)
            .map_err(map_engine_error)?;
        if !poll.installed {
            return Ok(InlineSidecarHostPollOutcome::Pending);
        }
        let active = self
            .active_inline_sidecar
            .take()
            .ok_or_else(|| HostStoreError::invalid("installed sidecar offer disappeared"))?;
        let commit = active
            .commit
            .ok_or_else(|| HostStoreError::invalid("installed sidecar lost commit proof"))?;
        let disposition = match active.begin.envelope.disposition {
            HotInlineSidecarDisposition::Authoritative { .. } => {
                InlineSidecarAckDisposition::Authoritative
            }
            HotInlineSidecarDisposition::Unsupported { .. } => {
                InlineSidecarAckDisposition::Unsupported
            }
        };
        let ack = InlineSidecarAck {
            publication_session: active.begin.publication_session,
            base_ack: active.begin.base_ack,
            refinement_generation: active.begin.binding.refinement_generation,
            block_ordinal: active.begin.binding.block_ordinal,
            transferred_node_count: active.begin.envelope.transferred_node_count,
            disposition,
            hio1_envelope_digest256: active.begin.envelope.hio1_envelope_digest256,
            root_stream_digest: commit.root_stream_digest,
        };
        self.inline_sidecar_reclaim_pending |= self.installed_inline_sidecar.is_some();
        self.installed_inline_sidecar = Some(InstalledInlineSidecar {
            begin: active.begin,
            binding: active.binding,
            ack,
        });
        self.pending_inline_sidecar_delivery_ack = Some(ack);
        Ok(InlineSidecarHostPollOutcome::Committed(ack))
    }

    fn finish_install(
        &mut self,
        installed: M11HostInstalledCandidate,
    ) -> Result<HostPollOutcome, HostStoreError> {
        // The engine atomically replaced its installed root before returning
        // this capability. A prior ACK therefore means one complete historical
        // closure is now on the arena's fuelled retirement queue. Remember the
        // work before any validation below can return: the next pending packet
        // must pay this reclaim debt rather than growing residency by revision
        // ancestry until arena allocation fails.
        self.background_reclaim_pending |= self.installed_ack.is_some();
        self.viewport_presentation.invalidate();
        let active = self
            .active
            .take()
            .ok_or_else(|| HostStoreError::invalid("installed offer disappeared"))?;
        let _commit = active
            .commit
            .ok_or_else(|| HostStoreError::invalid("installed offer lost commit proof"))?;
        let engine = self
            .engine
            .as_ref()
            .ok_or_else(|| HostStoreError::invalid("host engine was not initialized"))?;
        let record_count = canonical_record_count(engine, installed)?;
        if installed.parse_generation() != u64::from(active.begin.parse_generation)
            || installed.source_revision() != u64::from(active.begin.source_version.revision)
            || installed.publication_identity() != id128_bytes(active.begin.publication_session)
            || record_count != active.begin.target_record_count
        {
            return Err(HostStoreError::new(
                HostRejectReason::CorruptPublication,
                "installed authority or canonical record count changed",
            ));
        }
        let manifest_digest256 = engine
            .installed_manifest_digest256(installed)
            .map_err(map_engine_error)?;
        let sequence_digest = protocol_digest128_from_blake3(
            ProtocolDigestDomain::CandidateAckSequence,
            manifest_digest256,
        );
        let manifest_digest = protocol_digest128_from_blake3(
            ProtocolDigestDomain::CandidateManifest,
            manifest_digest256,
        );
        let ack = StructuralAck {
            publication_session: active.begin.publication_session,
            host_revision: active.begin.target_host_revision,
            source_version: active.begin.source_version,
            source_root: active.begin.source_root,
            parse_generation: active.begin.parse_generation,
            grammar_revision: active.begin.grammar_revision,
            syntax_profile: active.begin.syntax_profile,
            authority_mask: active.begin.authority_mask,
            record_count,
            sequence_digest,
            manifest_digest,
        };
        self.installed_ack = Some(ack);
        self.pending_delivery_ack = Some(ack);
        if let Some(sidecar) = self.inline_sidecar.as_mut() {
            let base = engine
                .inline_sidecar_base(installed, u64::from(active.begin.syntax_profile))
                .map_err(map_engine_error)?;
            if sidecar.observe_base(base).map_err(map_engine_error)? {
                self.installed_inline_sidecar = None;
                self.pending_inline_sidecar_delivery_ack = None;
                self.inline_sidecar_reclaim_pending = true;
            }
        }
        Ok(HostPollOutcome::Committed(ack))
    }

    fn poll_close(&mut self, transitions: u32) -> Result<HostPollOutcome, HostStoreError> {
        if self.closed {
            return Ok(HostPollOutcome::Closed);
        }
        if transitions == 0 {
            return Ok(HostPollOutcome::Pending);
        }
        let structural_complete = self.engine.as_mut().map_or(Ok(true), |engine| {
            engine
                .poll_close(transitions as usize)
                .map_err(map_engine_error)
        })?;
        if !structural_complete {
            return Ok(HostPollOutcome::Pending);
        }
        let sidecar_complete = self.inline_sidecar.as_mut().map_or(Ok(true), |sidecar| {
            sidecar
                .poll_close(transitions as usize)
                .map_err(map_engine_error)
        })?;
        if !sidecar_complete {
            return Ok(HostPollOutcome::Pending);
        }
        let viewport_complete = self.viewport_presentation.poll_close(transitions)?;
        if viewport_complete {
            self.closed = true;
            return Ok(HostPollOutcome::Closed);
        }
        Ok(HostPollOutcome::Pending)
    }

    fn validate_offer(&self, begin: OfferBegin) -> Result<(), HostStoreError> {
        let current = self.current_source.ok_or_else(|| {
            HostStoreError::new(HostRejectReason::NotReady, "no exact source was observed")
        })?;
        if begin.schema != SUPPORTED_MANIFEST_SCHEMA
            || begin.offer_id == [0; 4]
            || begin.publication_session == [0; 4]
            || begin.publication_session == self.config.document_session
            || begin.target_host_revision == 0
            || begin.source_root == [0; 2]
            || begin.parse_generation == 0
            || begin.grammar_revision != self.config.grammar_revision
            || begin.syntax_profile != self.config.syntax_profile
            || begin.authority_mask != self.config.authority_mask
            || begin.transferred_record_count == 0
            || begin.target_record_count == 0
        {
            return Err(HostStoreError::new(
                HostRejectReason::BaseMismatch,
                "offer declaration is not one exact M1.1 publication",
            ));
        }
        if begin.source_version.revision < current.revision {
            return Err(HostStoreError::new(
                HostRejectReason::StaleSource,
                "offer targets an older source",
            ));
        }
        if begin.source_version != current {
            return Err(HostStoreError::new(
                HostRejectReason::ExactSourceMismatch,
                "offer does not bind exact current source",
            ));
        }
        if begin.target_host_revision != begin.parse_generation
            || self
                .installed_ack
                .is_some_and(|ack| begin.target_host_revision <= ack.host_revision)
        {
            return Err(HostStoreError::new(
                HostRejectReason::BaseMismatch,
                "host revision must equal a strictly newer parse generation",
            ));
        }
        match (begin.mode, begin.base_ack) {
            (PublicationMode::FullSnapshot, None)
                if begin.transferred_record_count == begin.target_record_count => {}
            (PublicationMode::ExactBaseReferencesDelta, Some(base))
                if self.installed_ack == Some(base)
                    && self.pending_delivery_ack.is_none()
                    && begin.publication_session != base.publication_session
                    && begin.grammar_revision == base.grammar_revision
                    && begin.syntax_profile == base.syntax_profile
                    && begin.authority_mask == base.authority_mask
                    && begin.parse_generation > base.parse_generation
                    && begin.source_version.document_session
                        == base.source_version.document_session
                    && begin.source_version.revision > base.source_version.revision =>
            {
                let engine = self.engine.as_ref().ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::BaseMismatch,
                        "delta base ACK has no installed engine root",
                    )
                })?;
                let installed = engine.installed().ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::BaseMismatch,
                        "delta base ACK lost its installed capability",
                    )
                })?;
                let reused_references = engine
                    .role_record_count(installed, M11HostRole::References)
                    .map_err(map_engine_error)?;
                if u64::from(begin.transferred_record_count).checked_add(reused_references)
                    != Some(u64::from(begin.target_record_count))
                {
                    return Err(HostStoreError::new(
                        HostRejectReason::BaseMismatch,
                        "transferred records plus exact-base References do not form the target",
                    ));
                }
            }
            (PublicationMode::ExactBaseDelta, Some(base))
                if self.installed_ack == Some(base)
                    && self.pending_delivery_ack.is_none()
                    && begin.publication_session != base.publication_session
                    && begin.grammar_revision == base.grammar_revision
                    && begin.syntax_profile == base.syntax_profile
                    && begin.authority_mask == base.authority_mask
                    && begin.parse_generation > base.parse_generation
                    && begin.source_version.document_session
                        == base.source_version.document_session
                    && begin.source_version.revision > base.source_version.revision
                    && begin.transferred_record_count <= begin.target_record_count =>
            {
                let engine = self.engine.as_ref().ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::BaseMismatch,
                        "delta base ACK has no installed engine root",
                    )
                })?;
                if engine.installed().is_none() {
                    return Err(HostStoreError::new(
                        HostRejectReason::BaseMismatch,
                        "delta base ACK lost its installed capability",
                    ));
                }
            }
            _ => {
                return Err(HostStoreError::new(
                    HostRejectReason::BaseMismatch,
                    "publication mode does not bind its exact installed base",
                ));
            }
        }
        if let Some(orphan) = self.pending_delivery_ack {
            if begin.publication_session == orphan.publication_session
                || begin.parse_generation <= orphan.parse_generation
            {
                return Err(HostStoreError::new(
                    HostRejectReason::Backpressure,
                    "orphaned delivery proof requires a distinct newer recovery snapshot",
                ));
            }
        }
        self.validate_publication_limits(begin.limits)
    }

    fn validate_inline_sidecar_offer(
        &self,
        begin: HotInlineSidecarBegin,
    ) -> Result<(), HostStoreError> {
        let current = self.current_source.ok_or_else(|| {
            HostStoreError::new(HostRejectReason::NotReady, "no exact source was observed")
        })?;
        let installed_ack = self.installed_ack.ok_or_else(|| {
            HostStoreError::new(
                HostRejectReason::BaseMismatch,
                "hot-inline offer has no installed structural base",
            )
        })?;
        if begin.schema != HOT_INLINE_SIDECAR_SCHEMA
            || begin.offer_id == [0; 4]
            || begin.publication_session == [0; 4]
            || begin.publication_session == self.config.document_session
            || begin.publication_session == begin.base_ack.publication_session
            || begin.base_ack != installed_ack
            || begin.base_ack.source_version != current
            || begin.base_ack.grammar_revision != self.config.grammar_revision
            || begin.base_ack.syntax_profile != self.config.syntax_profile
            || begin.base_ack.authority_mask != self.config.authority_mask
            || begin.binding.parser_profile != u64::from(self.config.syntax_profile)
            || begin.binding.refinement_generation == 0
            || self.pending_delivery_ack.is_some()
            || self.pending_inline_sidecar_delivery_ack.is_some()
        {
            return Err(HostStoreError::new(
                HostRejectReason::BaseMismatch,
                "hot-inline offer does not bind the exact installed structural ACK",
            ));
        }
        if let Some(installed) = self.installed_inline_sidecar.as_ref() {
            if begin.binding.refinement_generation <= installed.ack.refinement_generation
                || begin.publication_session == installed.ack.publication_session
            {
                return Err(HostStoreError::new(
                    HostRejectReason::BaseMismatch,
                    "hot-inline refinement generation is not strictly newer",
                ));
            }
        }
        let expected_frames = begin
            .envelope
            .transferred_node_count
            .checked_add(2)
            .ok_or_else(|| HostStoreError::invalid("sidecar frame count overflowed"))?;
        let valid_disposition = match begin.envelope.disposition {
            HotInlineSidecarDisposition::Authoritative { .. } => matches!(
                begin.envelope.ipr2_descriptor_bytes,
                crate::v3_publication_wire::IPR3_DESCRIPTOR_BYTES
                    | PROJECTED_INLINE_PROJECTION_DESCRIPTOR_BYTES
                    | crate::v3_publication_wire::INDENTED_CODE_PROJECTION_DESCRIPTOR_BYTES
                    | BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES
            ),
            HotInlineSidecarDisposition::Unsupported { reason, .. } => {
                reason != 0
                    && begin.envelope.ipr2_descriptor_bytes == 0
                    && begin.envelope.transferred_node_count == 1
            }
        };
        if begin.envelope.hio1_encoded_bytes != crate::v3_publication_wire::HIO1_ENVELOPE_BYTES
            || !valid_disposition
            || expected_frames > begin.limits.maximum_frame_count
        {
            return Err(HostStoreError::new(
                HostRejectReason::Invalid,
                "hot-inline envelope metrics are invalid",
            ));
        }
        self.validate_publication_limits(begin.limits)
    }

    fn validate_publication_limits(
        &self,
        limits: crate::v3_publication_wire::OfferLimits,
    ) -> Result<(), HostStoreError> {
        let maximum_host_nodes = self
            .engine_limits
            .maximum_snapshot_nodes
            .min(u64::try_from(self.engine_limits.arena_max_slots).unwrap_or(u64::MAX));
        let maximum_host_frames = maximum_host_nodes.saturating_add(2);
        let minimum_one_frame_packet_bytes = u64::try_from(PACKET_HEADER_BYTES)
            .ok()
            .and_then(|bytes| bytes.checked_add(u64::try_from(PACKET_FRAME_DESCRIPTOR_BYTES).ok()?))
            .and_then(|bytes| bytes.checked_add(u64::from(limits.maximum_frame_bytes)));
        if limits.maximum_frame_count == 0
            || limits.maximum_encoded_frame_bytes == 0
            || limits.maximum_packet_bytes == 0
            || limits.maximum_frame_bytes == 0
            || limits.maximum_program_children == 0
            || limits.maximum_frame_bytes > limits.maximum_encoded_frame_bytes
            || limits.maximum_packet_bytes as usize > MAXIMUM_PACKET_ENCODED_BYTES
            || limits.maximum_frame_bytes > MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES
            || limits.maximum_frame_bytes as usize > M11_HOST_MAXIMUM_FRAME_BYTES
            || minimum_one_frame_packet_bytes
                .is_none_or(|bytes| bytes > u64::from(limits.maximum_packet_bytes))
            || limits.maximum_program_children as usize > M11_HOST_MAXIMUM_PROGRAM_CHILDREN
            || u64::from(limits.maximum_frame_count) > maximum_host_frames
            || u64::from(limits.maximum_encoded_frame_bytes)
                > self.engine_limits.maximum_snapshot_wire_bytes
            || limits.maximum_program_children as usize
                > self.engine_limits.arena_max_children_per_node
        {
            return Err(HostStoreError::new(
                HostRejectReason::ForegroundBoundExceeded,
                "offer limits exceed the exact M1.1 host envelope",
            ));
        }
        Ok(())
    }

    fn prepare_engine_for(&mut self, begin: OfferBegin) -> Result<(), HostStoreError> {
        let source = M11HostSourceVersion {
            source_root: source_root_u64(begin.source_root),
            source_revision: u64::from(begin.source_version.revision),
            source_bytes: u64::from(begin.source_version.utf8_length),
            source_utf16: u64::from(begin.source_version.utf16_length),
        };
        if let Some(engine) = self.engine.as_mut() {
            let (engine_revision, engine_root) = self
                .engine_source
                .ok_or_else(|| HostStoreError::invalid("engine source authority is missing"))?;
            if engine_revision == begin.source_version.revision && engine_root == begin.source_root
            {
                engine
                    .observe_source_version(source)
                    .map_err(map_engine_error)?;
            } else if engine_revision == begin.source_version.revision {
                // Full SourceVersion equality was proved above. A replacement
                // parser replica may legitimately allocate a fresh root ID for
                // those same exact bytes.
                engine
                    .rebind_source_replica(source)
                    .map_err(map_engine_error)?;
                self.engine_source = Some((begin.source_version.revision, begin.source_root));
            } else {
                engine
                    .observe_source_version(source)
                    .map_err(map_engine_error)?;
                self.engine_source = Some((begin.source_version.revision, begin.source_root));
            }
        } else {
            self.engine = Some(
                M11CandidateHost::new_with_limits(
                    id128_bytes(self.config.document_session),
                    source,
                    self.config.syntax_profile,
                    self.engine_limits,
                )
                .map_err(map_engine_error)?,
            );
            self.engine_source = Some((begin.source_version.revision, begin.source_root));
        }
        Ok(())
    }

    fn query_root(&self) -> Result<(&M11CandidateHost, M11HostInstalledCandidate), HostStoreError> {
        self.require_open()?;
        let current = self.current_source.ok_or_else(|| {
            HostStoreError::new(HostRejectReason::NotReady, "no exact source was observed")
        })?;
        let ack = self.installed_ack.ok_or_else(|| {
            HostStoreError::new(
                HostRejectReason::NotReady,
                "no structural root is installed",
            )
        })?;
        if ack.source_version != current {
            return Err(HostStoreError::new(
                HostRejectReason::ExactSourceMismatch,
                "installed structure is not exact current source",
            ));
        }
        let engine = self
            .engine
            .as_ref()
            .ok_or_else(|| HostStoreError::invalid("host engine was not initialized"))?;
        let installed = engine
            .installed()
            .ok_or_else(|| HostStoreError::invalid("installed ACK lost its engine root"))?;
        Ok((engine, installed))
    }

    fn abort_engine_snapshot(&mut self) -> Result<bool, HostStoreError> {
        if let Some(engine) = self.engine.as_mut() {
            return engine.abort_snapshot().map_err(map_engine_error);
        }
        Ok(false)
    }

    fn abort_inline_sidecar_engine_snapshot(&mut self) -> Result<bool, HostStoreError> {
        if let Some(sidecar) = self.inline_sidecar.as_mut() {
            return sidecar.abort_snapshot().map_err(map_engine_error);
        }
        Ok(false)
    }

    fn require_open(&self) -> Result<(), HostStoreError> {
        if self.closing || self.closed {
            Err(HostStoreError::new(
                HostRejectReason::Closed,
                "candidate host is closing",
            ))
        } else {
            Ok(())
        }
    }
}

fn host_source_cut_is_possible(position: HostSourceMetric, end: HostSourceMetric) -> bool {
    let starts_together = (position.bytes == 0) == (position.utf16 == 0);
    let ends_together = (position.bytes == end.bytes) == (position.utf16 == end.utf16);
    let prefix_can_be_utf8 = position.bytes >= position.utf16;
    let suffix_can_be_utf8 = end
        .bytes
        .checked_sub(position.bytes)
        .zip(end.utf16.checked_sub(position.utf16))
        .is_some_and(|(bytes, utf16)| bytes >= utf16);
    position.bytes <= end.bytes
        && position.utf16 <= end.utf16
        && starts_together
        && ends_together
        && prefix_can_be_utf8
        && suffix_can_be_utf8
}

fn whole_source_range(source: SourceVersion) -> HostMetricRange {
    HostMetricRange {
        start: HostSourceMetric::default(),
        end: HostSourceMetric {
            bytes: source.utf8_length,
            utf16: source.utf16_length,
        },
    }
}

/// Encodes schema 9 into one caller-owned slice.
///
/// Header (all integer lanes little-endian):
///
/// - 0..32: magic, viewport/header/record schemas and widths
/// - 32..48: flags, ancestry count, owner index, owner kind/coverage/atom
/// - 48..80: physical range plus physical/logical metrics
/// - 80..100: queried point/affinity and atom-specific arguments
/// - 100..112: authenticated linear-query receipt
///
/// Each 16-byte ancestry record is `(frame_id: u64, kind: u16, flags: u16,
/// reserved: u32)`. Kind values are opaque members of the versioned registry;
/// the host does not reinterpret them as one of the legacy flat block roles.
#[allow(clippy::too_many_arguments)]
fn encode_recursive_green_viewport(
    query: HostPointQuery,
    location: &M11HostRecursiveGreenLocation,
    range: HostMetricRange,
    physical_bytes: u32,
    physical_utf16: u32,
    logical_bytes: u32,
    logical_utf16: u32,
    events_scanned: u32,
    storage_pages_visited: u32,
    open_depth: u32,
    output: &mut [u8],
) -> Result<(), HostStoreError> {
    let expected = HOST_RECURSIVE_GREEN_VIEWPORT_HEADER_BYTES
        .checked_add(
            location
                .ancestry_len()
                .checked_mul(HOST_RECURSIVE_GREEN_ANCESTOR_RECORD_BYTES)
                .ok_or_else(|| HostStoreError::invalid("Green viewport bytes overflowed"))?,
        )
        .ok_or_else(|| HostStoreError::invalid("Green viewport bytes overflowed"))?;
    if output.len() != expected {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "Green viewport encoder received the wrong output width",
        ));
    }
    output.fill(0);
    output[..8].copy_from_slice(VIEWPORT_MAGIC);
    output[8..12].copy_from_slice(&HOST_RECURSIVE_GREEN_VIEWPORT_SCHEMA.to_le_bytes());
    output[12..16].copy_from_slice(
        &u32::try_from(HOST_RECURSIVE_GREEN_VIEWPORT_HEADER_BYTES)
            .expect("fixed Green header fits u32")
            .to_le_bytes(),
    );
    output[16..20].copy_from_slice(
        &u32::try_from(HOST_RECURSIVE_GREEN_ANCESTOR_RECORD_BYTES)
            .expect("fixed Green ancestor record fits u32")
            .to_le_bytes(),
    );
    output[20..24].copy_from_slice(&HOST_RECURSIVE_GREEN_KIND_REGISTRY_SCHEMA.to_le_bytes());
    output[24..28].copy_from_slice(&HOST_RECURSIVE_GREEN_COVERAGE_SCHEMA.to_le_bytes());
    output[28..32].copy_from_slice(&HOST_RECURSIVE_GREEN_LOGICAL_ATOM_SCHEMA.to_le_bytes());
    output[32..36].copy_from_slice(&0_u32.to_le_bytes());
    output[36..40].copy_from_slice(
        &u32::try_from(location.ancestry_len())
            .map_err(|_| HostStoreError::invalid("Green ancestry depth exceeds the wire"))?
            .to_le_bytes(),
    );
    output[40..44].copy_from_slice(
        &u32::try_from(location.owner_index())
            .map_err(|_| HostStoreError::invalid("Green owner index exceeds the wire"))?
            .to_le_bytes(),
    );
    output[44..46].copy_from_slice(&location.owner().kind().to_le_bytes());
    output[46] = recursive_green_coverage_tag(location.part());
    let (logical_atom, logical_argument0, logical_argument1) =
        recursive_green_logical_atom_wire(location.logical_atom());
    output[47] = logical_atom;
    output[48..52].copy_from_slice(&range.start.bytes.to_le_bytes());
    output[52..56].copy_from_slice(&range.end.bytes.to_le_bytes());
    output[56..60].copy_from_slice(&range.start.utf16.to_le_bytes());
    output[60..64].copy_from_slice(&range.end.utf16.to_le_bytes());
    output[64..68].copy_from_slice(&physical_bytes.to_le_bytes());
    output[68..72].copy_from_slice(&physical_utf16.to_le_bytes());
    output[72..76].copy_from_slice(&logical_bytes.to_le_bytes());
    output[76..80].copy_from_slice(&logical_utf16.to_le_bytes());
    output[80..84].copy_from_slice(&query.position.bytes.to_le_bytes());
    output[84..88].copy_from_slice(&query.position.utf16.to_le_bytes());
    output[88..92].copy_from_slice(
        &match query.affinity {
            HostMetricAffinity::Upstream => 0_u32,
            HostMetricAffinity::Downstream => 1_u32,
        }
        .to_le_bytes(),
    );
    output[92..96].copy_from_slice(&logical_argument0.to_le_bytes());
    output[96..100].copy_from_slice(&logical_argument1.to_le_bytes());
    output[100..104].copy_from_slice(&events_scanned.to_le_bytes());
    output[104..108].copy_from_slice(&storage_pages_visited.to_le_bytes());
    output[108..112].copy_from_slice(&open_depth.to_le_bytes());

    for index in 0..location.ancestry_len() {
        let ancestor = location.ancestor(index).ok_or_else(|| {
            HostStoreError::new(
                HostRejectReason::InternalFault,
                "Green ancestry changed during viewport encoding",
            )
        })?;
        let start = HOST_RECURSIVE_GREEN_VIEWPORT_HEADER_BYTES
            + index * HOST_RECURSIVE_GREEN_ANCESTOR_RECORD_BYTES;
        let record = &mut output[start..start + HOST_RECURSIVE_GREEN_ANCESTOR_RECORD_BYTES];
        record[..8].copy_from_slice(&ancestor.frame_id().to_le_bytes());
        record[8..10].copy_from_slice(&ancestor.kind().to_le_bytes());
        let flags = if index == location.owner_index() {
            HOST_RECURSIVE_GREEN_ANCESTOR_OWNER_FLAG
        } else {
            0
        };
        record[10..12].copy_from_slice(&flags.to_le_bytes());
        record[12..16].copy_from_slice(&0_u32.to_le_bytes());
    }
    Ok(())
}

const fn recursive_green_coverage_tag(part: M11HostRecursiveGreenCoveragePart) -> u8 {
    match part {
        M11HostRecursiveGreenCoveragePart::Content => 1,
        M11HostRecursiveGreenCoveragePart::ContainerMarker => 2,
        M11HostRecursiveGreenCoveragePart::BlockMarker => 3,
        M11HostRecursiveGreenCoveragePart::Gap => 4,
        M11HostRecursiveGreenCoveragePart::Terminal => 5,
    }
}

const fn recursive_green_logical_atom_wire(
    atom: M11HostRecursiveGreenLogicalAtom,
) -> (u8, u32, u32) {
    match atom {
        M11HostRecursiveGreenLogicalAtom::None => (0, 0, 0),
        M11HostRecursiveGreenLogicalAtom::Identity => (1, 0, 0),
        M11HostRecursiveGreenLogicalAtom::TabToSpaces {
            target_owner_depth,
            spaces,
        } => (2, target_owner_depth, spaces as u32),
        M11HostRecursiveGreenLogicalAtom::HiddenUpstream => (3, 0, 0),
        M11HostRecursiveGreenLogicalAtom::LfToLf => (4, 0, 0),
        M11HostRecursiveGreenLogicalAtom::CrLfToLf => (5, 0, 0),
        M11HostRecursiveGreenLogicalAtom::LoneCrToLf => (6, 0, 0),
        M11HostRecursiveGreenLogicalAtom::NulToReplacement => (7, 0, 0),
    }
}

fn block_range_gap(
    query: HostBlockRangeQuery,
    reason: HostSourceGapReason,
    mut receipt: HostBlockRangeReceipt,
) -> HostBlockRangeOutcome {
    receipt.encoded_bytes = 0;
    receipt.block_count = 0;
    receipt.complete = false;
    HostBlockRangeOutcome::SourceGap {
        source_version: query.source_version,
        requested_range: query.requested_range,
        reason,
        receipt,
    }
}

fn decode_block_range_continuation(
    continuation: HostBlockRangeContinuation,
    query: HostBlockRangeQuery,
    descriptor: M11HostPersistentBlockDescriptor,
    installed: M11HostInstalledCandidate,
    ack: StructuralAck,
) -> Result<M11HostPersistentBlockVisitStart, HostStoreError> {
    let bytes = continuation.encoded();
    if bytes[..8] != *BLOCK_RANGE_CONTINUATION_MAGIC
        || read_u32(&bytes, 8) != HOST_BLOCK_RANGE_SCHEMA
        || read_u32(&bytes, 12) != 0
    {
        return Err(HostStoreError::invalid(
            "range continuation envelope is invalid",
        ));
    }
    if ack.source_version != query.source_version
        || installed.source_revision() != u64::from(ack.source_version.revision)
        || installed.parse_generation() != u64::from(ack.parse_generation)
        || installed.publication_identity() != id128_bytes(ack.publication_session)
        || bytes[16..32] != id128_bytes(ack.publication_session)
        || read_u32(&bytes, 32) != ack.parse_generation
    {
        return Err(HostStoreError::new(
            HostRejectReason::ExactSourceMismatch,
            "range continuation does not bind the exact installed publication",
        ));
    }
    let next_ordinal = read_u32(&bytes, 36);
    let next = HostSourceMetric {
        bytes: read_u32(&bytes, 40),
        utf16: read_u32(&bytes, 44),
    };
    let encoded_range = HostMetricRange {
        start: HostSourceMetric {
            bytes: read_u32(&bytes, 48),
            utf16: read_u32(&bytes, 52),
        },
        end: HostSourceMetric {
            bytes: read_u32(&bytes, 56),
            utf16: read_u32(&bytes, 60),
        },
    };
    if encoded_range != query.requested_range
        || u64::from(next_ordinal) >= descriptor.entry_count()
        || next.bytes <= query.requested_range.start.bytes
        || next.utf16 <= query.requested_range.start.utf16
        || next.bytes >= query.requested_range.end.bytes
        || next.utf16 >= query.requested_range.end.utf16
    {
        return Err(HostStoreError::invalid(
            "range continuation does not resume inside its requested range",
        ));
    }
    Ok(M11HostPersistentBlockVisitStart::new(
        u64::from(next_ordinal),
        u64::from(next.bytes),
        u64::from(next.utf16),
    ))
}

fn encode_block_range_continuation(
    query: HostBlockRangeQuery,
    installed: M11HostInstalledCandidate,
    ack: StructuralAck,
    next_ordinal: u64,
    next_bytes: u64,
    next_utf16: u64,
) -> Result<HostBlockRangeContinuation, HostStoreError> {
    if ack.source_version != query.source_version
        || installed.source_revision() != u64::from(ack.source_version.revision)
        || installed.parse_generation() != u64::from(ack.parse_generation)
        || installed.publication_identity() != id128_bytes(ack.publication_session)
    {
        return Err(HostStoreError::invalid(
            "installed range continuation authority changed",
        ));
    }
    let next_ordinal = u32::try_from(next_ordinal)
        .map_err(|_| HostStoreError::invalid("range continuation ordinal overflowed"))?;
    let next_bytes = u32::try_from(next_bytes)
        .map_err(|_| HostStoreError::invalid("range continuation byte cut overflowed"))?;
    let next_utf16 = u32::try_from(next_utf16)
        .map_err(|_| HostStoreError::invalid("range continuation UTF-16 cut overflowed"))?;
    if next_bytes <= query.requested_range.start.bytes
        || next_utf16 <= query.requested_range.start.utf16
        || next_bytes >= query.requested_range.end.bytes
        || next_utf16 >= query.requested_range.end.utf16
    {
        return Err(HostStoreError::invalid(
            "range continuation does not make bounded source progress",
        ));
    }
    let mut bytes = [0_u8; HOST_BLOCK_RANGE_CONTINUATION_BYTES];
    bytes[..8].copy_from_slice(BLOCK_RANGE_CONTINUATION_MAGIC);
    bytes[8..12].copy_from_slice(&HOST_BLOCK_RANGE_SCHEMA.to_le_bytes());
    bytes[16..32].copy_from_slice(&id128_bytes(ack.publication_session));
    bytes[32..36].copy_from_slice(&ack.parse_generation.to_le_bytes());
    bytes[36..40].copy_from_slice(&next_ordinal.to_le_bytes());
    bytes[40..44].copy_from_slice(&next_bytes.to_le_bytes());
    bytes[44..48].copy_from_slice(&next_utf16.to_le_bytes());
    bytes[48..52].copy_from_slice(&query.requested_range.start.bytes.to_le_bytes());
    bytes[52..56].copy_from_slice(&query.requested_range.start.utf16.to_le_bytes());
    bytes[56..60].copy_from_slice(&query.requested_range.end.bytes.to_le_bytes());
    bytes[60..64].copy_from_slice(&query.requested_range.end.utf16.to_le_bytes());
    Ok(HostBlockRangeContinuation::from_encoded(bytes))
}

fn encode_block_range_header(output: &mut [u8], block_count: u32, complete: bool) {
    debug_assert_eq!(output.len(), HOST_BLOCK_RANGE_HEADER_BYTES);
    output.fill(0);
    output[..8].copy_from_slice(BLOCK_RANGE_MAGIC);
    output[8..12].copy_from_slice(&HOST_BLOCK_RANGE_SCHEMA.to_le_bytes());
    output[12..16].copy_from_slice(&(HOST_BLOCK_RANGE_HEADER_BYTES as u32).to_le_bytes());
    output[16..20].copy_from_slice(&(HOST_BLOCK_RANGE_RECORD_BYTES as u32).to_le_bytes());
    output[20..24].copy_from_slice(&block_count.to_le_bytes());
    output[24..28].copy_from_slice(
        &(if complete {
            HOST_BLOCK_RANGE_COMPLETE_FLAG
        } else {
            0
        })
        .to_le_bytes(),
    );
}

fn encode_block_range_record(
    output: &mut [u8; HOST_BLOCK_RANGE_RECORD_BYTES],
    entry_ordinal: u64,
    range: HostMetricRange,
    green: &[u8; M11_GREEN_RECORD_BYTES],
    projection: &[u8; M11_PROJECTION_RECORD_BYTES],
) {
    output[..8].copy_from_slice(&entry_ordinal.to_le_bytes());
    output[8..12].copy_from_slice(&range.start.bytes.to_le_bytes());
    output[12..16].copy_from_slice(&range.start.utf16.to_le_bytes());
    output[16..20].copy_from_slice(&range.end.bytes.to_le_bytes());
    output[20..24].copy_from_slice(&range.end.utf16.to_le_bytes());
    output[24..24 + M11_GREEN_RECORD_BYTES].copy_from_slice(green);
    output[24 + M11_GREEN_RECORD_BYTES..].copy_from_slice(projection);
}

fn encode_recursive_green_row_header(
    output: &mut [u8],
    row_count: u32,
    path_count: u32,
    complete: bool,
    selected_row_index: Option<u32>,
    start_ordinal: u64,
    total_rows: u64,
    ack: StructuralAck,
) {
    debug_assert_eq!(output.len(), HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES);
    output.fill(0);
    output[..8].copy_from_slice(BLOCK_RANGE_MAGIC);
    output[8..12].copy_from_slice(&HOST_RECURSIVE_GREEN_ROW_RANGE_SCHEMA.to_le_bytes());
    output[12..16]
        .copy_from_slice(&(HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES as u32).to_le_bytes());
    output[16..20].copy_from_slice(&(HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES as u32).to_le_bytes());
    output[20..24]
        .copy_from_slice(&(HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES as u32).to_le_bytes());
    output[24..28].copy_from_slice(&row_count.to_le_bytes());
    output[28..32].copy_from_slice(&path_count.to_le_bytes());
    output[32..36].copy_from_slice(
        &(if complete {
            HOST_BLOCK_RANGE_COMPLETE_FLAG
        } else {
            0
        })
        .to_le_bytes(),
    );
    output[36..40].copy_from_slice(&selected_row_index.unwrap_or(u32::MAX).to_le_bytes());
    output[40..48].copy_from_slice(&start_ordinal.to_le_bytes());
    output[48..56].copy_from_slice(&total_rows.to_le_bytes());
    output[56..60].copy_from_slice(&HOST_RECURSIVE_GREEN_KIND_REGISTRY_SCHEMA.to_le_bytes());
    output[60..64].copy_from_slice(&1_u32.to_le_bytes());
    output[64..68].copy_from_slice(&ack.source_version.revision.to_le_bytes());
    output[68..72].copy_from_slice(&ack.parse_generation.to_le_bytes());
    output[72..88].copy_from_slice(&id128_bytes(ack.publication_session));
}

fn encode_recursive_green_row_record(
    output: &mut [u8; HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES],
    row: M11HostRecursiveGreenRow<'_>,
    path_start: u32,
    selected: bool,
) -> Result<(), HostStoreError> {
    output.fill(0);
    let (presentation, mut flags) = match row.kind() {
        5 | 12 => (1_u16, HOST_RECURSIVE_GREEN_ROW_INLINE_FLAG),
        7 => (2, HOST_RECURSIVE_GREEN_ROW_LITERAL_FLAG),
        6 => (3, HOST_RECURSIVE_GREEN_ROW_LITERAL_FLAG),
        8 => (4, HOST_RECURSIVE_GREEN_ROW_LITERAL_FLAG),
        13 => (5, HOST_RECURSIVE_GREEN_ROW_LITERAL_FLAG),
        // A terminal-empty Item is an inline-shaped presentation row without
        // an inline payload. Its zero-width editable point and authenticated
        // Item ancestor are sufficient for marker-free editing.
        HOST_RECURSIVE_GREEN_EMPTY_ITEM_ROW_KIND => (1, 0),
        _ => {
            return Err(HostStoreError::invalid(
                "Green row kind is outside the renderable registry",
            ));
        }
    };
    if selected {
        flags |= HOST_RECURSIVE_GREEN_ROW_SELECTED_FLAG;
    }
    let (edit_capability, editable) = match row.edit_capability() {
        M11HostRecursiveGreenRowEditCapability::Contiguous => {
            let bytes = row
                .editable_byte_start()
                .zip(row.editable_byte_end())
                .ok_or_else(|| {
                    HostStoreError::invalid("contiguous Green row omitted byte edit span")
                })?;
            let utf16 = row
                .editable_utf16_start()
                .zip(row.editable_utf16_end())
                .ok_or_else(|| {
                    HostStoreError::invalid("contiguous Green row omitted UTF-16 edit span")
                })?;
            (1_u16, Some((bytes, utf16)))
        }
        M11HostRecursiveGreenRowEditCapability::ProjectedReserved => {
            flags &= !HOST_RECURSIVE_GREEN_ROW_INLINE_FLAG;
            (2_u16, None)
        }
        M11HostRecursiveGreenRowEditCapability::Unavailable => {
            flags &= !HOST_RECURSIVE_GREEN_ROW_INLINE_FLAG;
            (3_u16, None)
        }
    };
    output[..8].copy_from_slice(&row.ordinal().to_le_bytes());
    output[8..16].copy_from_slice(&row.frame_id().to_le_bytes());
    output[16..18].copy_from_slice(&row.kind().to_le_bytes());
    output[18..20].copy_from_slice(&flags.to_le_bytes());
    output[20..24].copy_from_slice(&path_start.to_le_bytes());
    output[24..28].copy_from_slice(
        &u32::try_from(row.path_len())
            .map_err(|_| HostStoreError::invalid("Green row path exceeds wire"))?
            .to_le_bytes(),
    );
    output[28..30].copy_from_slice(&presentation.to_le_bytes());
    output[30..32].copy_from_slice(&edit_capability.to_le_bytes());
    encode_metric_quad(
        &mut output[32..48],
        row.byte_start(),
        row.utf16_start(),
        row.byte_end(),
        row.utf16_end(),
    )?;
    if let Some(((byte_start, byte_end), (utf16_start, utf16_end))) = editable {
        encode_metric_quad(
            &mut output[48..64],
            byte_start,
            utf16_start,
            byte_end,
            utf16_end,
        )?;
    }
    Ok(())
}

fn encode_recursive_green_path_record(
    output: &mut [u8; HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES],
    path: M11HostRecursiveGreenRowPath<'_>,
    row_owner: bool,
) -> Result<(), HostStoreError> {
    output.fill(0);
    let mut flags = if row_owner {
        HOST_RECURSIVE_GREEN_PATH_ROW_OWNER_FLAG
    } else {
        0
    };
    if matches!(path.kind(), 2..=4) {
        flags |= HOST_RECURSIVE_GREEN_PATH_CONTAINER_FLAG;
    }
    if path.property_tag() != 0 {
        flags |= HOST_RECURSIVE_GREEN_PATH_OPEN_FACT_FLAG;
    }
    // Tag 6 is the engine-private cached row-geometry trailer. It is covered
    // by the Green commitment but is not a presentation fact on the host
    // wire. Preserve semantic close facts (1..=5) and hide only that internal
    // transport tag from consumers.
    let presentation_close_tag = match path.close_tag() {
        value @ 0..=5 => value,
        6 => 0,
        _ => {
            return Err(HostStoreError::invalid(
                "unknown Green presentation fact tag",
            ));
        }
    };
    if presentation_close_tag != 0 {
        flags |= HOST_RECURSIVE_GREEN_PATH_CLOSE_FACT_FLAG;
    }
    let fact_kind = match path.property_tag().max(presentation_close_tag) {
        0 => 0_u16,
        value @ 1..=5 => value,
        _ => {
            return Err(HostStoreError::invalid(
                "unknown Green presentation fact tag",
            ));
        }
    };
    let mut fact = [0_u8; 10];
    if path.property_len() > fact.len() {
        return Err(HostStoreError::invalid(
            "Green presentation fact exceeds normalized envelope",
        ));
    }
    for (index, value) in fact.iter_mut().take(path.property_len()).enumerate() {
        *value = path
            .property_byte(index)
            .ok_or_else(|| HostStoreError::invalid("Green presentation fact truncated"))?;
    }
    let mut args = [0_u32; 4];
    match fact_kind {
        0 => {}
        1 => {
            if path.property_len() != 8 || path.close_len() != 1 {
                return Err(HostStoreError::invalid("Green List facts changed shape"));
            }
            args[0] = u32::from(fact[0]);
            args[1] = u32::from(if fact[0] == 1 { fact[1] } else { fact[2] });
            args[2] = u32::from_le_bytes(fact[4..8].try_into().expect("List start"));
            args[3] = u32::from(
                path.close_byte(0)
                    .ok_or_else(|| HostStoreError::invalid("Green List tightness is absent"))?,
            );
        }
        2 => {
            if path.property_len() != 4 {
                return Err(HostStoreError::invalid("Green Item facts changed shape"));
            }
            args[0] = u32::from(u16::from_le_bytes(
                fact[..2].try_into().expect("Item offset"),
            ));
            args[1] = u32::from(u16::from_le_bytes(
                fact[2..4].try_into().expect("Item padding"),
            ));
        }
        3 => {
            if path.property_len() != 2 {
                return Err(HostStoreError::invalid("Green Heading facts changed shape"));
            }
            args[0] = u32::from(fact[0]);
            args[1] = u32::from(fact[1]);
        }
        4 => {
            if path.property_len() != 10 {
                return Err(HostStoreError::invalid("Green Code facts changed shape"));
            }
            args[0] = u32::from(fact[0]);
            args[1] = u32::from(fact[1]);
            let minimum = u64::from_le_bytes(fact[2..10].try_into().expect("Code minimum"));
            args[2] = minimum as u32;
            args[3] = (minimum >> 32) as u32;
        }
        5 => {
            if path.property_len() != 1 {
                return Err(HostStoreError::invalid("Green HTML facts changed shape"));
            }
            args[0] = u32::from(fact[0]);
        }
        _ => unreachable!(),
    }
    output[..8].copy_from_slice(&path.frame_id().to_le_bytes());
    output[8..10].copy_from_slice(&path.kind().to_le_bytes());
    output[10..12].copy_from_slice(&flags.to_le_bytes());
    output[12..14].copy_from_slice(&fact_kind.to_le_bytes());
    encode_metric_quad(
        &mut output[16..32],
        path.byte_start(),
        path.utf16_start(),
        path.byte_end(),
        path.utf16_end(),
    )?;
    for (index, value) in args.into_iter().enumerate() {
        let start = 32 + index * 4;
        output[start..start + 4].copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn encode_metric_quad(
    output: &mut [u8],
    start_bytes: u64,
    start_utf16: u64,
    end_bytes: u64,
    end_utf16: u64,
) -> Result<(), HostStoreError> {
    if output.len() != 16 {
        return Err(HostStoreError::invalid("Green metric quad changed width"));
    }
    for (index, value) in [start_bytes, start_utf16, end_bytes, end_utf16]
        .into_iter()
        .enumerate()
    {
        let value = u32::try_from(value)
            .map_err(|_| HostStoreError::invalid("Green row metric exceeds wire"))?;
        let start = index * 4;
        output[start..start + 4].copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn source_gap(
    source_version: SourceVersion,
    range: HostMetricRange,
    reason: HostSourceGapReason,
    receipt: HostViewportReceipt,
) -> HostStructuralQueryOutcome {
    HostStructuralQueryOutcome::SourceGap {
        source_version,
        range,
        reason,
        receipt,
    }
}

fn m11_records_describe_query_range(
    query: HostPointQuery,
    expected: HostMetricRange,
    green: &[u8],
    projection: &[u8],
) -> bool {
    if green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || &green[..8] != GREEN_MAGIC
        || &projection[..8] != PROJECTION_MAGIC
        || read_u32(green, 8) != M11_ROLE_SCHEMA
        || read_u32(projection, 8) != M11_ROLE_SCHEMA
        || green[13..16] != [0; 3]
        || projection[13..16] != [0; 3]
        || green[12] > M11_ORDERED_LIST_VARIANT
        || green[12] != projection[12]
    {
        return false;
    }
    let green_start = read_u64(green, 16);
    let green_end = read_u64(green, 24);
    let projection_start = read_u64(projection, 16);
    let projection_end = read_u64(projection, 24);
    if green_start != u64::from(expected.start.bytes)
        || green_end != u64::from(expected.end.bytes)
        || projection_start != green_start
        || projection_end != green_end
    {
        return false;
    }
    point_selects_root(
        u64::from(query.position.bytes),
        green_start,
        green_end,
        u64::from(query.source_version.utf8_length),
        query.affinity,
    )
}

fn point_selects_root(
    point: u64,
    start: u64,
    end: u64,
    source_end: u64,
    affinity: HostMetricAffinity,
) -> bool {
    if start > end || end > source_end || point < start || point > end {
        return false;
    }
    if start == end {
        return point == start;
    }
    match affinity {
        HostMetricAffinity::Upstream => point > start || start == 0,
        HostMetricAffinity::Downstream => point < end || end == source_end,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum M11LiteralBlockKind {
    Blank,
    DefinitionsOnly,
    Unsupported(u32),
}

trait PersistentBlockRecordView {
    fn byte_start(&self) -> u64;
    fn byte_end(&self) -> u64;
    fn utf16_start(&self) -> u64;
    fn utf16_end(&self) -> u64;
    fn kind(&self) -> M11HostBlockKind;
    fn source_bytes(&self) -> u64;
    fn source_utf16(&self) -> u64;
    fn reference_definition_count(&self) -> u64;
    fn unsupported_reason(&self) -> Option<M11HostBlockUnsupportedReason>;
    fn green_record(&self) -> Option<&[u8]>;
    fn projection_record(&self) -> Option<&[u8]>;
}

impl PersistentBlockRecordView for M11HostPersistentBlockLocation {
    fn byte_start(&self) -> u64 {
        self.byte_start()
    }

    fn byte_end(&self) -> u64 {
        self.byte_end()
    }

    fn utf16_start(&self) -> u64 {
        self.utf16_start()
    }

    fn utf16_end(&self) -> u64 {
        self.utf16_end()
    }

    fn kind(&self) -> M11HostBlockKind {
        self.kind()
    }

    fn source_bytes(&self) -> u64 {
        self.source_bytes()
    }

    fn source_utf16(&self) -> u64 {
        self.source_utf16()
    }

    fn reference_definition_count(&self) -> u64 {
        self.reference_definition_count()
    }

    fn unsupported_reason(&self) -> Option<M11HostBlockUnsupportedReason> {
        self.unsupported_reason()
    }

    fn green_record(&self) -> Option<&[u8]> {
        self.green_record()
    }

    fn projection_record(&self) -> Option<&[u8]> {
        self.projection_record()
    }
}

impl PersistentBlockRecordView for M11HostPersistentBlockVisitEntry<'_> {
    fn byte_start(&self) -> u64 {
        (*self).byte_start()
    }

    fn byte_end(&self) -> u64 {
        (*self).byte_end()
    }

    fn utf16_start(&self) -> u64 {
        (*self).utf16_start()
    }

    fn utf16_end(&self) -> u64 {
        (*self).utf16_end()
    }

    fn kind(&self) -> M11HostBlockKind {
        (*self).kind()
    }

    fn source_bytes(&self) -> u64 {
        (*self).source_bytes()
    }

    fn source_utf16(&self) -> u64 {
        (*self).source_utf16()
    }

    fn reference_definition_count(&self) -> u64 {
        (*self).reference_definition_count()
    }

    fn unsupported_reason(&self) -> Option<M11HostBlockUnsupportedReason> {
        (*self).unsupported_reason()
    }

    fn green_record(&self) -> Option<&[u8]> {
        (*self).green_record()
    }

    fn projection_record(&self) -> Option<&[u8]> {
        (*self).projection_record()
    }
}

fn persistent_block_view_range(
    source_version: SourceVersion,
    location: &impl PersistentBlockRecordView,
) -> Option<HostMetricRange> {
    let range = HostMetricRange {
        start: HostSourceMetric {
            bytes: u32::try_from(location.byte_start()).ok()?,
            utf16: u32::try_from(location.utf16_start()).ok()?,
        },
        end: HostSourceMetric {
            bytes: u32::try_from(location.byte_end()).ok()?,
            utf16: u32::try_from(location.utf16_end()).ok()?,
        },
    };
    if range.start.bytes >= range.end.bytes
        || range.start.utf16 >= range.end.utf16
        || range.end.bytes > source_version.utf8_length
        || range.end.utf16 > source_version.utf16_length
        || u64::from(range.end.bytes - range.start.bytes) != location.source_bytes()
        || u64::from(range.end.utf16 - range.start.utf16) != location.source_utf16()
    {
        return None;
    }
    Some(range)
}

fn persistent_block_location_range(
    query: HostPointQuery,
    location: &M11HostPersistentBlockLocation,
) -> Option<HostMetricRange> {
    let range = persistent_block_view_range(query.source_version, location)?;
    if !point_selects_root(
        u64::from(query.position.bytes),
        u64::from(range.start.bytes),
        u64::from(range.end.bytes),
        u64::from(query.source_version.utf8_length),
        query.affinity,
    ) || !point_selects_root(
        u64::from(query.position.utf16),
        u64::from(range.start.utf16),
        u64::from(range.end.utf16),
        u64::from(query.source_version.utf16_length),
        query.affinity,
    ) {
        return None;
    }
    Some(range)
}

fn persistent_block_visit_range(
    source_version: SourceVersion,
    location: M11HostPersistentBlockVisitEntry<'_>,
) -> Option<HostMetricRange> {
    persistent_block_view_range(source_version, &location)
}

fn persistent_block_records(
    location: &impl PersistentBlockRecordView,
    range: HostMetricRange,
    green: &mut [u8],
    projection: &mut [u8],
) -> bool {
    match location.kind() {
        M11HostBlockKind::Paragraph => {
            if location.unsupported_reason().is_some() {
                return false;
            }
            let (Some(relative_green), Some(relative_projection)) =
                (location.green_record(), location.projection_record())
            else {
                return false;
            };
            if !validate_leaf_relative_paragraph_records(
                location.source_bytes(),
                location.reference_definition_count(),
                relative_green,
                relative_projection,
            ) {
                return false;
            }
            green.copy_from_slice(relative_green);
            projection.copy_from_slice(relative_projection);
            translate_leaf_relative_records(u64::from(range.start.bytes), green, projection)
        }
        M11HostBlockKind::Structured => {
            if location.unsupported_reason().is_some() {
                return false;
            }
            let (Some(relative_green), Some(relative_projection)) =
                (location.green_record(), location.projection_record())
            else {
                return false;
            };
            match relative_green.get(12) {
                Some(&M11_FENCED_CODE_VARIANT)
                    if validate_leaf_relative_fenced_code_records(
                        location.source_bytes(),
                        location.reference_definition_count(),
                        relative_green,
                        relative_projection,
                    ) =>
                {
                    green.copy_from_slice(relative_green);
                    projection.copy_from_slice(relative_projection);
                    translate_leaf_relative_fenced_code_records(
                        u64::from(range.start.bytes),
                        green,
                        projection,
                    )
                }
                Some(&M11_ATX_HEADING_VARIANT)
                    if validate_leaf_relative_atx_heading_records(
                        u64::from(range.start.bytes),
                        location.source_bytes(),
                        location.reference_definition_count(),
                        relative_green,
                        relative_projection,
                    ) =>
                {
                    green.copy_from_slice(relative_green);
                    projection.copy_from_slice(relative_projection);
                    translate_leaf_relative_atx_heading_records(
                        u64::from(range.start.bytes),
                        green,
                        projection,
                    )
                }
                Some(&M11_SETEXT_HEADING_VARIANT)
                    if validate_leaf_relative_setext_heading_records(
                        location.source_bytes(),
                        location.reference_definition_count(),
                        relative_green,
                        relative_projection,
                    ) =>
                {
                    green.copy_from_slice(relative_green);
                    projection.copy_from_slice(relative_projection);
                    translate_leaf_relative_setext_heading_records(
                        u64::from(range.start.bytes),
                        green,
                        projection,
                    )
                }
                Some(&M11_THEMATIC_BREAK_VARIANT)
                    if validate_leaf_relative_thematic_break_records(
                        u64::from(range.start.bytes),
                        location.source_bytes(),
                        location.reference_definition_count(),
                        relative_green,
                        relative_projection,
                    ) =>
                {
                    green.copy_from_slice(relative_green);
                    projection.copy_from_slice(relative_projection);
                    translate_leaf_relative_thematic_break_records(
                        u64::from(range.start.bytes),
                        green,
                        projection,
                    )
                }
                Some(&M11_INDENTED_CODE_VARIANT)
                    if validate_leaf_relative_indented_code_records(
                        u64::from(range.start.bytes),
                        location.source_bytes(),
                        location.source_utf16(),
                        location.reference_definition_count(),
                        relative_green,
                        relative_projection,
                    ) =>
                {
                    green.copy_from_slice(relative_green);
                    projection.copy_from_slice(relative_projection);
                    translate_leaf_relative_records(u64::from(range.start.bytes), green, projection)
                }
                Some(&M11_BLOCK_QUOTE_VARIANT)
                    if validate_leaf_relative_block_quote_records(
                        location.source_bytes(),
                        location.source_utf16(),
                        location.reference_definition_count(),
                        relative_green,
                        relative_projection,
                    ) =>
                {
                    green.copy_from_slice(relative_green);
                    projection.copy_from_slice(relative_projection);
                    translate_leaf_relative_records(u64::from(range.start.bytes), green, projection)
                }
                Some(&M11_BULLET_LIST_VARIANT)
                    if validate_leaf_relative_bullet_list_records(
                        location.source_bytes(),
                        location.source_utf16(),
                        location.reference_definition_count(),
                        relative_green,
                        relative_projection,
                    ) =>
                {
                    green.copy_from_slice(relative_green);
                    projection.copy_from_slice(relative_projection);
                    translate_leaf_relative_records(u64::from(range.start.bytes), green, projection)
                }
                Some(&M11_ORDERED_LIST_VARIANT)
                    if validate_leaf_relative_ordered_list_records(
                        location.source_bytes(),
                        location.source_utf16(),
                        location.reference_definition_count(),
                        relative_green,
                        relative_projection,
                    ) =>
                {
                    green.copy_from_slice(relative_green);
                    projection.copy_from_slice(relative_projection);
                    translate_leaf_relative_records(u64::from(range.start.bytes), green, projection)
                }
                _ => false,
            }
        }
        M11HostBlockKind::Blank => {
            location.green_record().is_none()
                && location.projection_record().is_none()
                && location.unsupported_reason().is_none()
                && synthesize_literal_block_records(
                    M11LiteralBlockKind::Blank,
                    range,
                    location.reference_definition_count(),
                    green,
                    projection,
                )
        }
        M11HostBlockKind::DefinitionsOnly => {
            location.green_record().is_none()
                && location.projection_record().is_none()
                && location.unsupported_reason().is_none()
                && synthesize_literal_block_records(
                    M11LiteralBlockKind::DefinitionsOnly,
                    range,
                    location.reference_definition_count(),
                    green,
                    projection,
                )
        }
        M11HostBlockKind::Unsupported => {
            let Some(reason) = location.unsupported_reason() else {
                return false;
            };
            location.green_record().is_none()
                && location.projection_record().is_none()
                && synthesize_literal_block_records(
                    M11LiteralBlockKind::Unsupported(reason.get()),
                    range,
                    location.reference_definition_count(),
                    green,
                    projection,
                )
        }
    }
}

fn validate_leaf_relative_paragraph_records(
    leaf_bytes: u64,
    reference_definition_count: u64,
    green: &[u8],
    projection: &[u8],
) -> bool {
    if leaf_bytes == 0
        || green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || &green[..8] != GREEN_MAGIC
        || &projection[..8] != PROJECTION_MAGIC
        || read_u32(green, 8) != M11_ROLE_SCHEMA
        || read_u32(projection, 8) != M11_ROLE_SCHEMA
        || green[12] != 1
        || projection[12] != 1
        || green[13..16] != [0; 3]
        || projection[13..16] != [0; 3]
        || read_u64(green, 16) != 0
        || read_u64(green, 24) != leaf_bytes
        || read_u64(projection, 16) != 0
        || read_u64(projection, 24) != leaf_bytes
        || read_u64(green, 48) != reference_definition_count
        || read_u32(green, 56) != 0
        || read_u32(green, 60) != 0
        || read_u64(green, 64) != 0
        || read_u64(green, 72) != 0
        || read_u64(projection, 48) != 1
    {
        return false;
    }
    let visible_start = read_u64(green, 32);
    let visible_end = read_u64(green, 40);
    visible_start < visible_end
        && visible_end <= leaf_bytes
        && read_u64(projection, 32) == visible_start
        && read_u64(projection, 40) == visible_end
}

fn validate_leaf_relative_atx_heading_records(
    leaf_start: u64,
    leaf_bytes: u64,
    reference_definition_count: u64,
    green: &[u8],
    projection: &[u8],
) -> bool {
    if leaf_bytes == 0
        || leaf_bytes > u64::from(u32::MAX)
        || reference_definition_count != 0
        || green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || &green[..8] != GREEN_MAGIC
        || &projection[..8] != PROJECTION_MAGIC
        || read_u32(green, 8) != M11_ROLE_SCHEMA
        || read_u32(projection, 8) != M11_ROLE_SCHEMA
        || green[12] != M11_ATX_HEADING_VARIANT
        || projection[12] != M11_ATX_HEADING_VARIANT
        || green[13..16] != [0; 3]
        || projection[13..16] != [0; 3]
        || read_u64(green, 16) != 0
        || read_u64(green, 24) != leaf_bytes
        || read_u64(projection, 16) != 0
        || read_u64(projection, 24) != leaf_bytes
        || read_u64(projection, 48) != 1
    {
        return false;
    }

    let content_start = read_u64(green, 32);
    let content_end = read_u64(green, 40);
    if content_start > content_end
        || content_end > leaf_bytes
        || read_u64(projection, 32) != content_start
        || read_u64(projection, 40) != content_end
    {
        return false;
    }

    let metadata = read_u64(green, 48);
    let level = u64::from(metadata as u8);
    let closed = metadata & M11_ATX_HEADING_CLOSED_FLAG != 0;
    let opening_indent = (metadata >> M11_ATX_HEADING_OPENING_INDENT_SHIFT) & 0x3;
    let has_bof_bom = metadata & M11_ATX_HEADING_BOF_BOM_FLAG != 0;
    if metadata & !M11_ATX_HEADING_METADATA_MASK != 0 || !(1..=6).contains(&level) {
        return false;
    }

    let opening_start = u64::from(read_u32(green, 56));
    let opening_end = u64::from(read_u32(green, 60));
    let closing_start = read_u32(green, 64);
    let closing_end = read_u32(green, 68);
    let line_ending_start = u64::from(read_u32(green, 72));
    let line_ending_end = u64::from(read_u32(green, 76));
    if opening_start >= opening_end
        || opening_end - opening_start != level
        || opening_start != opening_indent + if has_bof_bom { 3 } else { 0 }
        || has_bof_bom && leaf_start != 0
        || opening_end > content_start
        || line_ending_start > line_ending_end
        || line_ending_end != leaf_bytes
        || line_ending_end - line_ending_start > 2
    {
        return false;
    }
    if closed {
        let closing_start = u64::from(closing_start);
        let closing_end = u64::from(closing_end);
        closing_start != u64::from(M11_ATX_HEADING_ABSENT_CUT)
            && closing_end != u64::from(M11_ATX_HEADING_ABSENT_CUT)
            && content_end <= closing_start
            && closing_start < closing_end
            && closing_end <= line_ending_start
    } else {
        closing_start == M11_ATX_HEADING_ABSENT_CUT
            && closing_end == M11_ATX_HEADING_ABSENT_CUT
            && content_end <= line_ending_start
    }
}

fn validate_leaf_relative_fenced_code_records(
    leaf_bytes: u64,
    reference_definition_count: u64,
    green: &[u8],
    projection: &[u8],
) -> bool {
    if leaf_bytes == 0
        || leaf_bytes > u64::from(u32::MAX)
        || reference_definition_count != 0
        || green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || &green[..8] != GREEN_MAGIC
        || &projection[..8] != PROJECTION_MAGIC
        || read_u32(green, 8) != M11_ROLE_SCHEMA
        || read_u32(projection, 8) != M11_ROLE_SCHEMA
        || green[12] != M11_FENCED_CODE_VARIANT
        || projection[12] != M11_FENCED_CODE_VARIANT
        || green[13..16] != [0; 3]
        || projection[13..16] != [0; 3]
        || read_u64(green, 16) != 0
        || read_u64(green, 24) != leaf_bytes
        || read_u64(projection, 16) != 0
        || read_u64(projection, 24) != leaf_bytes
        || read_u64(projection, 48) != 1
    {
        return false;
    }

    let body_start = read_u64(green, 32);
    let body_end = read_u64(green, 40);
    if body_start > body_end
        || body_end > leaf_bytes
        || read_u64(projection, 32) != body_start
        || read_u64(projection, 40) != body_end
    {
        return false;
    }

    let metadata = read_u64(green, 48);
    let marker = metadata as u8;
    let opening_indent = ((metadata >> 8) & 0xff) as u8;
    let closed = metadata & M11_FENCE_CLOSED_FLAG != 0;
    if metadata & !M11_FENCE_METADATA_MASK != 0
        || !matches!(marker, b'`' | b'~')
        || opening_indent > 3
    {
        return false;
    }

    let opening_start = u64::from(read_u32(green, 56));
    let opening_end = u64::from(read_u32(green, 60));
    let info_start = u64::from(read_u32(green, 64));
    let info_end = u64::from(read_u32(green, 68));
    if opening_start >= opening_end
        || opening_end - opening_start < 3
        || opening_end > leaf_bytes
        || info_start != opening_end
        || info_start > info_end
        || info_end > body_start
        || body_start - info_end > 2
    {
        return false;
    }

    let closing_start = read_u32(green, 72);
    let closing_end = read_u32(green, 76);
    if closed {
        let closing_start = u64::from(closing_start);
        let closing_end = u64::from(closing_end);
        closing_start != u64::from(M11_FENCE_ABSENT_CUT)
            && closing_end != u64::from(M11_FENCE_ABSENT_CUT)
            && closing_start < closing_end
            && closing_end - closing_start >= opening_end - opening_start
            && body_end <= closing_start
            && closing_start - body_end <= 3
            && closing_end <= leaf_bytes
    } else {
        closing_start == M11_FENCE_ABSENT_CUT
            && closing_end == M11_FENCE_ABSENT_CUT
            && body_end == leaf_bytes
    }
}

fn validate_leaf_relative_setext_heading_records(
    leaf_bytes: u64,
    reference_definition_count: u64,
    green: &[u8],
    projection: &[u8],
) -> bool {
    if leaf_bytes == 0
        || leaf_bytes > u64::from(u32::MAX)
        || green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || &green[..8] != GREEN_MAGIC
        || &projection[..8] != PROJECTION_MAGIC
        || read_u32(green, 8) != M11_ROLE_SCHEMA
        || read_u32(projection, 8) != M11_ROLE_SCHEMA
        || green[12] != M11_SETEXT_HEADING_VARIANT
        || projection[12] != M11_SETEXT_HEADING_VARIANT
        || green[13..16] != [0; 3]
        || projection[13..16] != [0; 3]
        || read_u64(green, 16) != 0
        || read_u64(green, 24) != leaf_bytes
        || read_u64(projection, 16) != 0
        || read_u64(projection, 24) != leaf_bytes
        || read_u64(green, 72) != reference_definition_count
        || read_u64(projection, 48) != 1
    {
        return false;
    }

    let inline_start = read_u64(green, 32);
    let inline_end = read_u64(green, 40);
    if inline_start >= inline_end
        || inline_end > leaf_bytes
        || read_u64(projection, 32) != inline_start
        || read_u64(projection, 40) != inline_end
    {
        return false;
    }

    let metadata = read_u64(green, 48);
    let level = metadata as u8;
    let opening_indent = (metadata >> M11_SETEXT_HEADING_OPENING_INDENT_SHIFT) & u64::from(0x3_u8);
    if metadata & !M11_SETEXT_HEADING_METADATA_MASK != 0 || !matches!(level, 1 | 2) {
        return false;
    }

    let underline_start = u64::from(read_u32(green, 56));
    let underline_end = u64::from(read_u32(green, 60));
    let line_ending_start = u64::from(read_u32(green, 64));
    let line_ending_end = u64::from(read_u32(green, 68));
    underline_start
        .checked_sub(inline_end)
        .is_some_and(|gap| matches!(gap.checked_sub(opening_indent), Some(1 | 2)))
        && underline_start < underline_end
        && underline_end <= line_ending_start
        && line_ending_start <= line_ending_end
        && line_ending_end == leaf_bytes
        && line_ending_end - line_ending_start <= 2
}

fn validate_leaf_relative_thematic_break_records(
    leaf_start: u64,
    leaf_bytes: u64,
    reference_definition_count: u64,
    green: &[u8],
    projection: &[u8],
) -> bool {
    if leaf_bytes == 0
        || leaf_bytes > u64::from(u32::MAX)
        || reference_definition_count != 0
        || green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || &green[..8] != GREEN_MAGIC
        || &projection[..8] != PROJECTION_MAGIC
        || read_u32(green, 8) != M11_ROLE_SCHEMA
        || read_u32(projection, 8) != M11_ROLE_SCHEMA
        || green[12] != M11_THEMATIC_BREAK_VARIANT
        || projection[12] != M11_THEMATIC_BREAK_VARIANT
        || green[13..16] != [0; 3]
        || projection[13..16] != [0; 3]
        || read_u64(green, 16) != 0
        || read_u64(green, 24) != leaf_bytes
        || read_u64(projection, 16) != 0
        || read_u64(projection, 24) != leaf_bytes
        || read_u64(green, 32) != 0
        || read_u64(green, 40) != 0
        || read_u64(projection, 32) != 0
        || read_u64(projection, 40) != 0
        || read_u64(projection, 48) != 0
    {
        return false;
    }

    let metadata = read_u64(green, 48);
    let marker = metadata as u8;
    let opening_indent = (metadata >> M11_THEMATIC_BREAK_OPENING_INDENT_SHIFT) & u64::from(0x3_u8);
    let has_bof_bom = metadata & M11_THEMATIC_BREAK_BOF_BOM_FLAG != 0;
    if metadata & !M11_THEMATIC_BREAK_METADATA_MASK != 0
        || !matches!(marker, b'*' | b'-' | b'_')
        || has_bof_bom && leaf_start != 0
    {
        return false;
    }

    let marker_start = u64::from(read_u32(green, 56));
    let marker_end = u64::from(read_u32(green, 60));
    let line_ending_start = u64::from(read_u32(green, 64));
    let line_ending_end = u64::from(read_u32(green, 68));
    let marker_count = read_u64(green, 72);
    marker_start == opening_indent + if has_bof_bom { 3 } else { 0 }
        && marker_start < marker_end
        && marker_end <= line_ending_start
        && marker_count >= 3
        && marker_count <= marker_end - marker_start
        && line_ending_start <= line_ending_end
        && line_ending_end == leaf_bytes
        && line_ending_end - line_ending_start <= 2
}

fn validate_leaf_relative_indented_code_records(
    leaf_start: u64,
    leaf_bytes: u64,
    leaf_utf16: u64,
    reference_definition_count: u64,
    green: &[u8],
    projection: &[u8],
) -> bool {
    if leaf_bytes == 0
        || leaf_bytes > u64::from(u32::MAX)
        || leaf_utf16 == 0
        || leaf_utf16 > u64::from(u32::MAX)
        || reference_definition_count != 0
        || green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || &green[..8] != GREEN_MAGIC
        || &projection[..8] != PROJECTION_MAGIC
        || read_u32(green, 8) != M11_ROLE_SCHEMA
        || read_u32(projection, 8) != M11_ROLE_SCHEMA
        || green[12] != M11_INDENTED_CODE_VARIANT
        || projection[12] != M11_INDENTED_CODE_VARIANT
        || green[13..16] != [0; 3]
        || projection[13..16] != [0; 3]
        || read_u64(green, 16) != 0
        || read_u64(green, 24) != leaf_bytes
        || read_u64(projection, 16) != 0
        || read_u64(projection, 24) != leaf_bytes
        || read_u64(green, 32) != 0
        || read_u64(green, 40) != 0
        || read_u64(projection, 32) != 0
        || read_u64(projection, 40) != 0
        || read_u64(green, 72) != 0
    {
        return false;
    }

    let metadata = read_u64(green, 48);
    let deindent_columns = metadata & 0xff;
    let has_bof_bom = metadata & M11_INDENTED_CODE_BOF_BOM_FLAG != 0;
    let line_count = u64::from(read_u32(green, 56));
    let projected_utf8 = u64::from(read_u32(green, 60));
    let projected_utf16 = u64::from(read_u32(green, 64));
    let terminal_line_ending_bytes = u64::from(read_u32(green, 68));
    let projection_runs = read_u64(projection, 48);

    metadata & !M11_INDENTED_CODE_METADATA_MASK == 0
        && deindent_columns == M11_INDENTED_CODE_DEINDENT_COLUMNS
        && (!has_bof_bom || leaf_start == 0)
        && line_count > 0
        && projection_runs == line_count
        && projected_utf8 > 0
        && projected_utf8 < leaf_bytes
        && projected_utf16 > 0
        && projected_utf16 < leaf_utf16
        && terminal_line_ending_bytes <= 2
        && terminal_line_ending_bytes <= projected_utf8
        && terminal_line_ending_bytes <= projected_utf16
}

fn query_marked_line_sidecar(
    descriptor: M11HostBlockQuoteSidecarDescriptor,
    mut cursor: M11HostBlockQuoteCursor<'_>,
    disposition: HotInlineSidecarDisposition,
    binding: HotInlineSidecarBinding,
    output: &mut [u8],
    maximum_query_bytes: usize,
    kind: HostMarkedLinePayloadKind,
) -> Result<HostInlineSidecarQueryOutcome, HostStoreError> {
    let (logical_page_count, expected_record_count, storage_page_count, ordered_commitment256) =
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
                    "marked-line sidecar engine and wire dispositions disagree",
                ));
            }
        };
    if descriptor.physical_start() != binding.physical_start_utf8
        || descriptor.physical_end() != binding.physical_end_utf8
        || descriptor.window_start() != binding.visible_start_utf8
        || descriptor.window_end() != binding.visible_end_utf8
        || descriptor.logical_page_count() != logical_page_count
        || descriptor.line_count() != expected_record_count
        || descriptor.storage_page_count() != storage_page_count
        || descriptor.ordered_commitment256() != ordered_commitment256
    {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "marked-line sidecar descriptor disagrees with its envelope",
        ));
    }
    let expected_record_count = usize::try_from(expected_record_count)
        .map_err(|_| HostStoreError::invalid("marked-line record count exceeds this target"))?;
    let record_bytes = kind.record_bytes();
    let expected_bytes = expected_record_count
        .checked_mul(record_bytes)
        .ok_or_else(|| HostStoreError::invalid("marked-line sidecar query bytes overflowed"))?;
    if expected_bytes > output.len() || expected_bytes > maximum_query_bytes {
        return Err(HostStoreError::new(
            HostRejectReason::QueryBoundExceeded,
            "marked-line sidecar query output is too small",
        ));
    }

    let mut record_count = 0_usize;
    let mut saw_terminal_empty = false;
    loop {
        match cursor.poll().map_err(map_engine_error)? {
            M11HostBlockQuoteCursorPoll::Line(line) => {
                match kind {
                    HostMarkedLinePayloadKind::BlockQuote => {
                        if line.continuation_prefix_start() != 0
                            || line.continuation_prefix_end() != 0
                            || !matches!(line.flags(), 1 | 2)
                        {
                            return Err(HostStoreError::new(
                                HostRejectReason::InternalFault,
                                "block-quote sidecar carried a non-quote line record",
                            ));
                        }
                    }
                    HostMarkedLinePayloadKind::BulletList => {
                        let content_utf16 = line.bullet_content_utf16_length();
                        if saw_terminal_empty
                            || line.hidden_prefix_length() == 0
                            || line.continuation_prefix_start() >= line.continuation_prefix_end()
                            || line.continuation_prefix_end() > line.hidden_prefix_length()
                            || (line.content_length() == 0) != (content_utf16 == 0)
                            || content_utf16 > line.content_length()
                        {
                            return Err(HostStoreError::new(
                                HostRejectReason::InternalFault,
                                "bullet-list sidecar carried invalid item geometry",
                            ));
                        }
                        saw_terminal_empty = line.content_length() == 0;
                    }
                }
                let start = record_count.checked_mul(record_bytes).ok_or_else(|| {
                    HostStoreError::invalid("marked-line sidecar record offset overflowed")
                })?;
                let end = start.checked_add(record_bytes).ok_or_else(|| {
                    HostStoreError::invalid("marked-line sidecar record offset overflowed")
                })?;
                let record = output.get_mut(start..end).ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::QueryBoundExceeded,
                        "marked-line sidecar query output is too small",
                    )
                })?;
                record[0..4].copy_from_slice(&line.relative_line_start().to_le_bytes());
                record[4..8].copy_from_slice(&line.physical_source_length().to_le_bytes());
                record[8..12].copy_from_slice(&line.hidden_prefix_length().to_le_bytes());
                match kind {
                    HostMarkedLinePayloadKind::BlockQuote => {
                        record[12..16].copy_from_slice(&line.content_length().to_le_bytes());
                        record[16..20].copy_from_slice(&line.flags().to_le_bytes());
                    }
                    HostMarkedLinePayloadKind::BulletList => {
                        record[12..16]
                            .copy_from_slice(&line.continuation_prefix_start().to_le_bytes());
                        record[16..20]
                            .copy_from_slice(&line.continuation_prefix_end().to_le_bytes());
                        record[20..24].copy_from_slice(&line.content_length().to_le_bytes());
                        record[24..28]
                            .copy_from_slice(&line.bullet_content_utf16_length().to_le_bytes());
                    }
                }
                record_count += 1;
            }
            M11HostBlockQuoteCursorPoll::Complete => break,
        }
    }
    let encoded_bytes = record_count
        .checked_mul(record_bytes)
        .ok_or_else(|| HostStoreError::invalid("marked-line sidecar query bytes overflowed"))?;
    if record_count != expected_record_count || encoded_bytes != expected_bytes {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "marked-line sidecar cursor disagrees with its envelope",
        ));
    }
    Ok(HostInlineSidecarQueryOutcome::Authoritative {
        payload_kind: match kind {
            HostMarkedLinePayloadKind::BlockQuote => HostInlineSidecarPayloadKind::BlockQuote,
            HostMarkedLinePayloadKind::BulletList => HostInlineSidecarPayloadKind::BulletList,
        },
        fact_count: u32::try_from(record_count)
            .map_err(|_| HostStoreError::invalid("marked-line record count overflowed"))?,
        value_entry_count: 0,
        value_encoded_bytes: 0,
        encoded_bytes: u32::try_from(encoded_bytes)
            .map_err(|_| HostStoreError::invalid("marked-line query bytes overflowed"))?,
        tree_nodes_visited: u32::try_from(cursor.tree_nodes_visited())
            .map_err(|_| HostStoreError::invalid("marked-line query receipt overflowed"))?,
    })
}

#[allow(clippy::too_many_arguments)]
fn query_ordered_list_item_sidecar(
    descriptor: M11HostBlockQuoteSidecarDescriptor,
    cursor: M11HostBlockQuoteCursor<'_>,
    disposition: HotInlineSidecarDisposition,
    binding: HotInlineSidecarBinding,
    output: &mut [u8],
    maximum_query_bytes: usize,
    selected_item_ordinal: u32,
    selected_item_line_ending: M11HostCanonicalLineEnding,
    opening_marker_start: u32,
    opening_marker_end: u32,
    marker_value: u32,
) -> Result<HostInlineSidecarQueryOutcome, HostStoreError> {
    let (logical_page_count, item_count, storage_page_count, ordered_commitment256) =
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
                    "ordered-list sidecar engine and wire dispositions disagree",
                ));
            }
        };
    if logical_page_count != 1
        || item_count != 1
        || storage_page_count != 1
        || descriptor.physical_start() != binding.physical_start_utf8
        || descriptor.physical_end() != binding.physical_end_utf8
        || descriptor.window_start() != binding.visible_start_utf8
        || descriptor.window_end() != binding.visible_end_utf8
        || descriptor.logical_page_count() != logical_page_count
        || descriptor.line_count() != item_count
        || descriptor.storage_page_count() != storage_page_count
        || descriptor.ordered_commitment256() != ordered_commitment256
    {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "ordered-list sidecar descriptor disagrees with its compact envelope",
        ));
    }
    if M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES > output.len()
        || M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES > maximum_query_bytes
    {
        return Err(HostStoreError::new(
            HostRejectReason::QueryBoundExceeded,
            "ordered-list sidecar query output is too small",
        ));
    }
    let (_, tree_nodes_visited) = encode_ordered_list_item_payload(
        &mut output[..M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES],
        binding,
        descriptor,
        cursor,
        selected_item_ordinal,
        selected_item_line_ending,
        opening_marker_start,
        opening_marker_end,
        marker_value,
    )?;
    Ok(HostInlineSidecarQueryOutcome::Authoritative {
        payload_kind: HostInlineSidecarPayloadKind::OrderedListItem,
        fact_count: 1,
        value_entry_count: 0,
        value_encoded_bytes: 0,
        encoded_bytes: M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES as u32,
        tree_nodes_visited: u32::try_from(tree_nodes_visited)
            .map_err(|_| HostStoreError::invalid("ordered-list query receipt overflowed"))?,
    })
}

#[allow(clippy::too_many_arguments)]
fn encode_ordered_list_item_payload(
    output: &mut [u8],
    binding: HotInlineSidecarBinding,
    descriptor: M11HostBlockQuoteSidecarDescriptor,
    mut cursor: M11HostBlockQuoteCursor<'_>,
    selected_item_ordinal: u32,
    selected_item_line_ending: M11HostCanonicalLineEnding,
    opening_marker_start: u32,
    opening_marker_end: u32,
    marker_value: u32,
) -> Result<(M11HostBlockQuoteLine, u64), HostStoreError> {
    if output.len() != M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES
        || selected_item_ordinal == u32::MAX
        || opening_marker_start >= opening_marker_end
        || !(2..=10).contains(&opening_marker_end.saturating_sub(opening_marker_start))
        || marker_value > 999_999_999
    {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "ordered-list sidecar carried invalid editing metadata",
        ));
    }
    let item = match cursor.poll().map_err(map_engine_error)? {
        M11HostBlockQuoteCursorPoll::Line(item) => item,
        M11HostBlockQuoteCursorPoll::Complete => {
            return Err(HostStoreError::new(
                HostRejectReason::InternalFault,
                "ordered-list sidecar contained no selected item",
            ));
        }
    };
    if !matches!(
        cursor.poll().map_err(map_engine_error)?,
        M11HostBlockQuoteCursorPoll::Complete
    ) {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "ordered-list sidecar contained more than one selected item",
        ));
    }

    let content_utf16 = item.ordered_content_utf16_length();
    let eol = item.physical_eol_length();
    let canonical_eol_length = match selected_item_line_ending {
        M11HostCanonicalLineEnding::Lf | M11HostCanonicalLineEnding::Cr => 1,
        M11HostCanonicalLineEnding::CrLf => 2,
    };
    let expected_relative_start = binding
        .visible_start_utf8
        .checked_sub(binding.physical_start_utf8)
        .ok_or_else(|| HostStoreError::invalid("ordered-list item escaped its list binding"))?;
    let expected_relative_end = binding
        .visible_end_utf8
        .checked_sub(binding.physical_start_utf8)
        .ok_or_else(|| HostStoreError::invalid("ordered-list item escaped its list binding"))?;
    let projected_utf8 = item
        .content_length()
        .checked_add(eol)
        .ok_or_else(|| HostStoreError::invalid("ordered-list item UTF-8 length overflowed"))?;
    let projected_utf16 = content_utf16
        .checked_add(eol)
        .ok_or_else(|| HostStoreError::invalid("ordered-list item UTF-16 length overflowed"))?;
    let physical_utf16 = item
        .hidden_prefix_length()
        .checked_add(projected_utf16)
        .ok_or_else(|| HostStoreError::invalid("ordered-list item UTF-16 length overflowed"))?;
    if item.hidden_prefix_length() == 0
        || item.continuation_prefix_start() >= item.continuation_prefix_end()
        || item.continuation_prefix_end() > item.hidden_prefix_length()
        || opening_marker_start < item.continuation_prefix_start()
        || opening_marker_end > item.continuation_prefix_end()
        || opening_marker_end > item.hidden_prefix_length()
        || (item.content_length() == 0) != (content_utf16 == 0)
        || content_utf16 > item.content_length()
        || eol != canonical_eol_length
        || item
            .hidden_prefix_length()
            .checked_add(item.content_length())
            .and_then(|length| length.checked_add(eol))
            != Some(item.physical_source_length())
        || item.relative_line_start() != expected_relative_start
        || item
            .relative_line_start()
            .checked_add(item.physical_source_length())
            != Some(expected_relative_end)
        || binding
            .visible_end_utf16
            .checked_sub(binding.visible_start_utf16)
            != Some(physical_utf16)
        || descriptor.projected_utf8_length() != projected_utf8
        || descriptor.projected_utf16_length() != projected_utf16
    {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "ordered-list selected item disagrees with its compact authority",
        ));
    }

    output.fill(0);
    output[0..4].copy_from_slice(&selected_item_ordinal.to_le_bytes());
    output[4] = match selected_item_line_ending {
        M11HostCanonicalLineEnding::Lf => 1,
        M11HostCanonicalLineEnding::CrLf => 2,
        M11HostCanonicalLineEnding::Cr => 3,
    };
    output[8..12].copy_from_slice(&opening_marker_start.to_le_bytes());
    output[12..16].copy_from_slice(&opening_marker_end.to_le_bytes());
    output[16..20].copy_from_slice(&marker_value.to_le_bytes());
    let record = &mut output[M11_ORDERED_LIST_ITEM_META_BYTES..];
    record[0..4].copy_from_slice(&item.relative_line_start().to_le_bytes());
    record[4..8].copy_from_slice(&item.physical_source_length().to_le_bytes());
    record[8..12].copy_from_slice(&item.hidden_prefix_length().to_le_bytes());
    record[12..16].copy_from_slice(&item.continuation_prefix_start().to_le_bytes());
    record[16..20].copy_from_slice(&item.continuation_prefix_end().to_le_bytes());
    record[20..24].copy_from_slice(&item.content_length().to_le_bytes());
    record[24..28].copy_from_slice(&content_utf16.to_le_bytes());
    Ok((item, cursor.tree_nodes_visited()))
}

fn point_selects_sidecar_window(binding: HotInlineSidecarBinding, query: HostPointQuery) -> bool {
    // A downstream point at the selected window's end normally belongs to the
    // following sibling. When that end is also the enclosing physical block
    // end, there is no later sibling inside this authority; accepting it keeps
    // a terminal list item selectable at document EOF.
    let byte_selected = query.position.bytes > binding.visible_start_utf8
        && query.position.bytes < binding.visible_end_utf8
        || query.position.bytes == binding.visible_start_utf8
            && query.affinity == HostMetricAffinity::Downstream
        || query.position.bytes == binding.visible_end_utf8
            && (query.affinity == HostMetricAffinity::Upstream
                || binding.visible_end_utf8 == binding.physical_end_utf8);
    let utf16_selected = query.position.utf16 > binding.visible_start_utf16
        && query.position.utf16 < binding.visible_end_utf16
        || query.position.utf16 == binding.visible_start_utf16
            && query.affinity == HostMetricAffinity::Downstream
        || query.position.utf16 == binding.visible_end_utf16
            && (query.affinity == HostMetricAffinity::Upstream
                || binding.visible_end_utf16 == binding.physical_end_utf16);
    byte_selected && utf16_selected
}

fn validate_leaf_relative_block_quote_records(
    leaf_bytes: u64,
    leaf_utf16: u64,
    reference_definition_count: u64,
    green: &[u8],
    projection: &[u8],
) -> bool {
    if leaf_bytes == 0
        || leaf_bytes > u64::from(u32::MAX)
        || leaf_utf16 == 0
        || leaf_utf16 > u64::from(u32::MAX)
        || reference_definition_count != 0
        || green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || &green[..8] != GREEN_MAGIC
        || &projection[..8] != PROJECTION_MAGIC
        || read_u32(green, 8) != M11_ROLE_SCHEMA
        || read_u32(projection, 8) != M11_ROLE_SCHEMA
        || green[12] != M11_BLOCK_QUOTE_VARIANT
        || projection[12] != M11_BLOCK_QUOTE_VARIANT
        || green[13..16] != [0; 3]
        || projection[13..16] != [0; 3]
        || read_u64(green, 16) != 0
        || read_u64(green, 24) != leaf_bytes
        || read_u64(projection, 16) != 0
        || read_u64(projection, 24) != leaf_bytes
        || read_u64(green, 32) != 0
        || read_u64(green, 40) != 0
        || read_u64(projection, 32) != 0
        || read_u64(projection, 40) != 0
        || read_u64(green, 48) != M11_BLOCK_QUOTE_EXACT_SINGLE_PARAGRAPH_DISPOSITION
        || read_u32(green, 76) != 0
    {
        return false;
    }

    let line_count = u64::from(read_u32(green, 56));
    let child_first_line = u64::from(read_u32(green, 60));
    let child_line_count = u64::from(read_u32(green, 64));
    let projected_utf8 = u64::from(read_u32(green, 68));
    let projected_utf16 = u64::from(read_u32(green, 72));
    line_count > 0
        && child_first_line == 0
        && child_line_count == line_count
        && read_u64(projection, 48) == line_count
        && projected_utf8 > 0
        && projected_utf8 < leaf_bytes
        && projected_utf16 > 0
        && projected_utf16 < leaf_utf16
}

fn validate_bullet_list_structural_summary(
    range: HostMetricRange,
    green: &[u8],
    projection: &[u8],
    expected_item_count: u64,
) -> bool {
    if green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || green[12] != M11_BULLET_LIST_VARIANT
        || projection[12] != M11_BULLET_LIST_VARIANT
        || green[13..16] != [0; 3]
        || projection[13..16] != [0; 3]
        || read_u64(green, 16) != u64::from(range.start.bytes)
        || read_u64(green, 24) != u64::from(range.end.bytes)
        || read_u64(projection, 16) != u64::from(range.start.bytes)
        || read_u64(projection, 24) != u64::from(range.end.bytes)
        || read_u64(green, 32) != u64::from(range.start.bytes)
        || read_u64(green, 40) != u64::from(range.start.bytes)
        || read_u64(projection, 32) != u64::from(range.start.bytes)
        || read_u64(projection, 40) != u64::from(range.start.bytes)
        || read_u32(green, 76) != 0
    {
        return false;
    }
    let metadata = read_u64(green, 48);
    let disposition = metadata & 0xff;
    let marker = ((metadata >> M11_BULLET_LIST_MARKER_SHIFT) & 0xff) as u8;
    let item_count = read_u32(green, 56);
    let terminal_empty_start = read_u32(green, 60);
    let paragraph_count = read_u32(green, 64);
    let projected_utf8 = read_u32(green, 68);
    let projected_utf16 = read_u32(green, 72);
    let source_utf8 = range.end.bytes.saturating_sub(range.start.bytes);
    let source_utf16 = range.end.utf16.saturating_sub(range.start.utf16);
    let terminal_shape_is_valid = if terminal_empty_start == M11_BULLET_LIST_NO_TERMINAL_EMPTY {
        paragraph_count == item_count
    } else {
        paragraph_count.checked_add(1) == Some(item_count) && terminal_empty_start < source_utf8
    };
    disposition == M11_BULLET_LIST_EXACT_TIGHT_DISPOSITION
        && metadata & !M11_BULLET_LIST_METADATA_MASK == 0
        && metadata & M11_BULLET_LIST_TIGHT_FLAG != 0
        && matches!(marker, b'-' | b'+' | b'*')
        && item_count > 0
        && u64::from(item_count) == expected_item_count
        && read_u64(projection, 48) == u64::from(item_count)
        && terminal_shape_is_valid
        && projected_utf8 <= source_utf8
        && projected_utf16 <= source_utf16
        && (projected_utf8 == 0) == (projected_utf16 == 0)
        && (projected_utf8 != 0
            || (item_count == 1 && paragraph_count == 0 && terminal_empty_start == 0))
}

fn validate_ordered_list_structural_summary(
    range: HostMetricRange,
    green: &[u8],
    projection: &[u8],
    expected_item_count: u64,
) -> bool {
    if green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || green[12] != M11_ORDERED_LIST_VARIANT
        || projection[12] != M11_ORDERED_LIST_VARIANT
        || green[13..16] != [0; 3]
        || projection[13..16] != [0; 3]
        || read_u64(green, 16) != u64::from(range.start.bytes)
        || read_u64(green, 24) != u64::from(range.end.bytes)
        || read_u64(projection, 16) != u64::from(range.start.bytes)
        || read_u64(projection, 24) != u64::from(range.end.bytes)
        || read_u64(green, 32) != u64::from(range.start.bytes)
        || read_u64(green, 40) != u64::from(range.start.bytes)
        || read_u64(projection, 32) != u64::from(range.start.bytes)
        || read_u64(projection, 40) != u64::from(range.start.bytes)
    {
        return false;
    }
    let metadata = read_u64(green, 48);
    let disposition = metadata & 0xff;
    let delimiter = ((metadata >> M11_ORDERED_LIST_DELIMITER_SHIFT) & 0xff) as u8;
    let item_count = read_u32(green, 56);
    let terminal_empty_start = read_u32(green, 60);
    let paragraph_count = read_u32(green, 64);
    let projected_utf8 = read_u32(green, 68);
    let projected_utf16 = read_u32(green, 72);
    let list_start = read_u32(green, 76);
    let source_utf8 = range.end.bytes.saturating_sub(range.start.bytes);
    let source_utf16 = range.end.utf16.saturating_sub(range.start.utf16);
    let terminal_shape_is_valid = if terminal_empty_start == M11_ORDERED_LIST_NO_TERMINAL_EMPTY {
        paragraph_count == item_count
    } else {
        paragraph_count.checked_add(1) == Some(item_count) && terminal_empty_start < source_utf8
    };
    disposition == M11_ORDERED_LIST_EXACT_TIGHT_DISPOSITION
        && metadata & !M11_ORDERED_LIST_METADATA_MASK == 0
        && metadata & M11_ORDERED_LIST_TIGHT_FLAG != 0
        && matches!(delimiter, b'.' | b')')
        && list_start <= 999_999_999
        && item_count > 0
        && u64::from(item_count) == expected_item_count
        && read_u64(projection, 48) == u64::from(item_count)
        && terminal_shape_is_valid
        && projected_utf8 <= source_utf8
        && projected_utf16 <= source_utf16
        && (projected_utf8 == 0) == (projected_utf16 == 0)
        && (projected_utf8 != 0
            || (item_count == 1 && paragraph_count == 0 && terminal_empty_start == 0))
}

fn validate_leaf_relative_bullet_list_records(
    leaf_bytes: u64,
    leaf_utf16: u64,
    reference_definition_count: u64,
    green: &[u8],
    projection: &[u8],
) -> bool {
    let (Ok(leaf_bytes), Ok(leaf_utf16)) = (u32::try_from(leaf_bytes), u32::try_from(leaf_utf16))
    else {
        return false;
    };
    if leaf_bytes == 0
        || leaf_utf16 == 0
        || reference_definition_count != 0
        || green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || &green[..8] != GREEN_MAGIC
        || &projection[..8] != PROJECTION_MAGIC
        || read_u32(green, 8) != M11_ROLE_SCHEMA
        || read_u32(projection, 8) != M11_ROLE_SCHEMA
    {
        return false;
    }
    let item_count = u64::from(read_u32(green, 56));
    validate_bullet_list_structural_summary(
        HostMetricRange {
            start: HostSourceMetric { bytes: 0, utf16: 0 },
            end: HostSourceMetric {
                bytes: leaf_bytes,
                utf16: leaf_utf16,
            },
        },
        green,
        projection,
        item_count,
    )
}

fn validate_leaf_relative_ordered_list_records(
    leaf_bytes: u64,
    leaf_utf16: u64,
    reference_definition_count: u64,
    green: &[u8],
    projection: &[u8],
) -> bool {
    let (Ok(leaf_bytes), Ok(leaf_utf16)) = (u32::try_from(leaf_bytes), u32::try_from(leaf_utf16))
    else {
        return false;
    };
    if leaf_bytes == 0
        || leaf_utf16 == 0
        || reference_definition_count != 0
        || green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || &green[..8] != GREEN_MAGIC
        || &projection[..8] != PROJECTION_MAGIC
        || read_u32(green, 8) != M11_ROLE_SCHEMA
        || read_u32(projection, 8) != M11_ROLE_SCHEMA
    {
        return false;
    }
    let item_count = u64::from(read_u32(green, 56));
    validate_ordered_list_structural_summary(
        HostMetricRange {
            start: HostSourceMetric { bytes: 0, utf16: 0 },
            end: HostSourceMetric {
                bytes: leaf_bytes,
                utf16: leaf_utf16,
            },
        },
        green,
        projection,
        item_count,
    )
}

fn bullet_list_point_path_node_count(
    query: HostPointQuery,
    range: HostMetricRange,
    green: &[u8],
) -> Result<usize, HostStoreError> {
    if green.len() != M11_GREEN_RECORD_BYTES || green[12] != M11_BULLET_LIST_VARIANT {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "bullet-list point path has no structural summary",
        ));
    }
    let item_count = read_u32(green, 56);
    let terminal_empty_start = read_u32(green, 60);
    let paragraph_count = read_u32(green, 64);
    if terminal_empty_start == M11_BULLET_LIST_NO_TERMINAL_EMPTY {
        return Ok(3);
    }
    if paragraph_count.checked_add(1) != Some(item_count) {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "bullet-list terminal-empty summary is inconsistent",
        ));
    }
    let terminal_start = range
        .start
        .bytes
        .checked_add(terminal_empty_start)
        .ok_or_else(|| HostStoreError::invalid("bullet-list terminal item offset overflowed"))?;
    let terminal_selected = item_count == 1
        || query.position.bytes > terminal_start
        || (query.position.bytes == terminal_start
            && query.affinity == HostMetricAffinity::Downstream);
    Ok(if terminal_selected { 2 } else { 3 })
}

fn ordered_list_point_path_node_count(
    query: HostPointQuery,
    range: HostMetricRange,
    green: &[u8],
) -> Result<usize, HostStoreError> {
    if green.len() != M11_GREEN_RECORD_BYTES || green[12] != M11_ORDERED_LIST_VARIANT {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "ordered-list point path has no structural summary",
        ));
    }
    let item_count = read_u32(green, 56);
    let terminal_empty_start = read_u32(green, 60);
    let paragraph_count = read_u32(green, 64);
    if terminal_empty_start == M11_ORDERED_LIST_NO_TERMINAL_EMPTY {
        return Ok(3);
    }
    if paragraph_count.checked_add(1) != Some(item_count) {
        return Err(HostStoreError::new(
            HostRejectReason::InternalFault,
            "ordered-list terminal-empty summary is inconsistent",
        ));
    }
    let terminal_start = range
        .start
        .bytes
        .checked_add(terminal_empty_start)
        .ok_or_else(|| HostStoreError::invalid("ordered-list terminal item offset overflowed"))?;
    let terminal_selected = item_count == 1
        || query.position.bytes > terminal_start
        || (query.position.bytes == terminal_start
            && query.affinity == HostMetricAffinity::Downstream);
    Ok(if terminal_selected { 2 } else { 3 })
}

fn encode_bullet_list_point_path(
    output: &mut [u8],
    node_count: usize,
    range: HostMetricRange,
    green: &[u8],
    projection: &[u8],
    selected_ordinal: u32,
    selected_item: M11HostBlockQuoteLine,
) -> bool {
    let item_count = read_u32(green, 56);
    if output.len() != node_count.saturating_mul(M11_POINT_PATH_V5_NODE_RECORD_BYTES)
        || !validate_bullet_list_structural_summary(range, green, projection, u64::from(item_count))
        || selected_ordinal >= item_count
    {
        return false;
    }
    let relative_start = selected_item.relative_line_start();
    let item_start = match range.start.bytes.checked_add(relative_start) {
        Some(value) => value,
        None => return false,
    };
    let item_end = match item_start.checked_add(selected_item.physical_source_length()) {
        Some(value) => value,
        None => return false,
    };
    let paragraph_start = match item_start.checked_add(selected_item.hidden_prefix_length()) {
        Some(value) => value,
        None => return false,
    };
    let paragraph_end = match paragraph_start.checked_add(selected_item.content_length()) {
        Some(value) => value,
        None => return false,
    };
    if item_start < range.start.bytes
        || item_start >= item_end
        || item_end > range.end.bytes
        || paragraph_end > item_end
        || selected_item.hidden_prefix_length() == 0
        || selected_item.continuation_prefix_start() >= selected_item.continuation_prefix_end()
        || selected_item.continuation_prefix_end() > selected_item.hidden_prefix_length()
    {
        return false;
    }
    let content_utf16 = selected_item.bullet_content_utf16_length();
    if (selected_item.content_length() == 0) != (content_utf16 == 0)
        || content_utf16 > selected_item.content_length()
    {
        return false;
    }
    let terminal_empty_start = read_u32(green, 60);
    let selected_is_empty = selected_item.content_length() == 0;
    if selected_is_empty
        != (node_count == 2
            && selected_ordinal.checked_add(1) == Some(item_count)
            && terminal_empty_start == relative_start)
        || (!selected_is_empty && node_count != 3)
    {
        return false;
    }
    let eol = selected_item.physical_eol_length();
    if eol > 2 {
        return false;
    }
    let item_projected_utf8 = match selected_item.content_length().checked_add(eol) {
        Some(value) => value,
        None => return false,
    };
    let item_projected_utf16 = match content_utf16.checked_add(eol) {
        Some(value) => value,
        None => return false,
    };

    let (list_node, descendants) = output.split_at_mut(M11_POINT_PATH_V5_NODE_RECORD_BYTES);
    encode_v5_point_path_node(
        list_node,
        M11_POINT_PATH_KIND_LIST,
        M11_POINT_PATH_FLAG_NONCONTIGUOUS,
        0,
        M11_POINT_PATH_ROOT_PARENT,
        range.start.bytes,
        range.end.bytes,
        0,
        item_count,
        read_u32(green, 68),
        read_u32(green, 72),
    );
    let (item_node, paragraph_node) = descendants.split_at_mut(M11_POINT_PATH_V5_NODE_RECORD_BYTES);
    encode_v5_point_path_node(
        item_node,
        M11_POINT_PATH_KIND_LIST_ITEM,
        M11_POINT_PATH_FLAG_NONCONTIGUOUS
            | if selected_is_empty {
                M11_POINT_PATH_FLAG_SELECTED
            } else {
                0
            },
        1,
        0,
        item_start,
        item_end,
        selected_ordinal,
        1,
        item_projected_utf8,
        item_projected_utf16,
    );
    if selected_is_empty {
        return paragraph_node.is_empty();
    }
    if paragraph_node.len() != M11_POINT_PATH_V5_NODE_RECORD_BYTES
        || paragraph_start >= paragraph_end
    {
        return false;
    }
    encode_v5_point_path_node(
        paragraph_node,
        M11_POINT_PATH_KIND_PARAGRAPH,
        M11_POINT_PATH_FLAG_SELECTED,
        2,
        1,
        paragraph_start,
        paragraph_end,
        selected_ordinal,
        1,
        selected_item.content_length(),
        content_utf16,
    );
    true
}

fn encode_ordered_list_point_path(
    output: &mut [u8],
    node_count: usize,
    range: HostMetricRange,
    green: &[u8],
    projection: &[u8],
    selected_ordinal: u32,
    selected_item: M11HostBlockQuoteLine,
) -> bool {
    let item_count = read_u32(green, 56);
    if output.len() != node_count.saturating_mul(M11_POINT_PATH_V5_NODE_RECORD_BYTES)
        || !validate_ordered_list_structural_summary(
            range,
            green,
            projection,
            u64::from(item_count),
        )
        || selected_ordinal >= item_count
    {
        return false;
    }
    let relative_start = selected_item.relative_line_start();
    let item_start = match range.start.bytes.checked_add(relative_start) {
        Some(value) => value,
        None => return false,
    };
    let item_end = match item_start.checked_add(selected_item.physical_source_length()) {
        Some(value) => value,
        None => return false,
    };
    let paragraph_start = match item_start.checked_add(selected_item.hidden_prefix_length()) {
        Some(value) => value,
        None => return false,
    };
    let paragraph_end = match paragraph_start.checked_add(selected_item.content_length()) {
        Some(value) => value,
        None => return false,
    };
    if item_start < range.start.bytes
        || item_start >= item_end
        || item_end > range.end.bytes
        || paragraph_end > item_end
        || selected_item.hidden_prefix_length() == 0
        || selected_item.continuation_prefix_start() >= selected_item.continuation_prefix_end()
        || selected_item.continuation_prefix_end() > selected_item.hidden_prefix_length()
    {
        return false;
    }
    let content_utf16 = selected_item.ordered_content_utf16_length();
    if (selected_item.content_length() == 0) != (content_utf16 == 0)
        || content_utf16 > selected_item.content_length()
    {
        return false;
    }
    let terminal_empty_start = read_u32(green, 60);
    let selected_is_empty = selected_item.content_length() == 0;
    if selected_is_empty
        != (node_count == 2
            && selected_ordinal.checked_add(1) == Some(item_count)
            && terminal_empty_start == relative_start)
        || (!selected_is_empty && node_count != 3)
    {
        return false;
    }
    let eol = selected_item.physical_eol_length();
    if eol > 2 {
        return false;
    }
    let item_projected_utf8 = match selected_item.content_length().checked_add(eol) {
        Some(value) => value,
        None => return false,
    };
    let item_projected_utf16 = match content_utf16.checked_add(eol) {
        Some(value) => value,
        None => return false,
    };

    let (list_node, descendants) = output.split_at_mut(M11_POINT_PATH_V5_NODE_RECORD_BYTES);
    encode_v5_point_path_node(
        list_node,
        M11_POINT_PATH_KIND_LIST,
        M11_POINT_PATH_FLAG_NONCONTIGUOUS,
        0,
        M11_POINT_PATH_ROOT_PARENT,
        range.start.bytes,
        range.end.bytes,
        0,
        item_count,
        read_u32(green, 68),
        read_u32(green, 72),
    );
    let (item_node, paragraph_node) = descendants.split_at_mut(M11_POINT_PATH_V5_NODE_RECORD_BYTES);
    encode_v5_point_path_node(
        item_node,
        M11_POINT_PATH_KIND_LIST_ITEM,
        M11_POINT_PATH_FLAG_NONCONTIGUOUS
            | if selected_is_empty {
                M11_POINT_PATH_FLAG_SELECTED
            } else {
                0
            },
        1,
        0,
        item_start,
        item_end,
        selected_ordinal,
        1,
        item_projected_utf8,
        item_projected_utf16,
    );
    if selected_is_empty {
        return paragraph_node.is_empty();
    }
    if paragraph_node.len() != M11_POINT_PATH_V5_NODE_RECORD_BYTES
        || paragraph_start >= paragraph_end
    {
        return false;
    }
    encode_v5_point_path_node(
        paragraph_node,
        M11_POINT_PATH_KIND_PARAGRAPH,
        M11_POINT_PATH_FLAG_SELECTED,
        2,
        1,
        paragraph_start,
        paragraph_end,
        selected_ordinal,
        1,
        selected_item.content_length(),
        content_utf16,
    );
    true
}

#[allow(clippy::too_many_arguments)]
fn encode_v5_point_path_node(
    output: &mut [u8],
    kind: u8,
    flags: u8,
    depth: u16,
    parent: u32,
    source_start: u32,
    source_end: u32,
    first_run: u32,
    run_count: u32,
    projected_utf8: u32,
    projected_utf16: u32,
) {
    debug_assert_eq!(output.len(), M11_POINT_PATH_V5_NODE_RECORD_BYTES);
    output.fill(0);
    output[0] = kind;
    output[1] = flags;
    output[2..4].copy_from_slice(&depth.to_le_bytes());
    output[4..8].copy_from_slice(&parent.to_le_bytes());
    output[8..12].copy_from_slice(&source_start.to_le_bytes());
    output[12..16].copy_from_slice(&source_end.to_le_bytes());
    output[16..20].copy_from_slice(&first_run.to_le_bytes());
    output[20..24].copy_from_slice(&run_count.to_le_bytes());
    output[24..28].copy_from_slice(&projected_utf8.to_le_bytes());
    output[28..32].copy_from_slice(&projected_utf16.to_le_bytes());
}

fn encode_block_quote_point_path(
    output: &mut [u8],
    range: HostMetricRange,
    green: &[u8],
    projection: &[u8],
) -> bool {
    if output.len() != M11_BLOCK_QUOTE_POINT_PATH_BYTES
        || green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || green[12] != M11_BLOCK_QUOTE_VARIANT
        || projection[12] != M11_BLOCK_QUOTE_VARIANT
        || read_u64(green, 16) != u64::from(range.start.bytes)
        || read_u64(green, 24) != u64::from(range.end.bytes)
        || read_u64(projection, 16) != u64::from(range.start.bytes)
        || read_u64(projection, 24) != u64::from(range.end.bytes)
        || read_u64(green, 48) != M11_BLOCK_QUOTE_EXACT_SINGLE_PARAGRAPH_DISPOSITION
    {
        return false;
    }
    let line_count = read_u32(green, 56);
    let child_first_line = read_u32(green, 60);
    let child_line_count = read_u32(green, 64);
    let projected_utf8 = read_u32(green, 68);
    let projected_utf16 = read_u32(green, 72);
    if line_count == 0
        || child_first_line != 0
        || child_line_count != line_count
        || read_u64(projection, 48) != u64::from(line_count)
    {
        return false;
    }

    let (ancestor, selected_leaf) = output.split_at_mut(M11_POINT_PATH_NODE_RECORD_BYTES);
    ancestor.fill(0);
    ancestor[0] = M11_POINT_PATH_KIND_BLOCK_QUOTE;
    ancestor[2..4].copy_from_slice(&0_u16.to_le_bytes());
    ancestor[4..8].copy_from_slice(&M11_POINT_PATH_ROOT_PARENT.to_le_bytes());
    ancestor[8..12].copy_from_slice(&range.start.bytes.to_le_bytes());
    ancestor[12..16].copy_from_slice(&range.end.bytes.to_le_bytes());
    ancestor[16..20].copy_from_slice(&range.start.utf16.to_le_bytes());
    ancestor[20..24].copy_from_slice(&range.end.utf16.to_le_bytes());
    ancestor[24..28].copy_from_slice(&0_u32.to_le_bytes());
    ancestor[28..32].copy_from_slice(&line_count.to_le_bytes());
    ancestor[32..36].copy_from_slice(&projected_utf8.to_le_bytes());
    ancestor[36..40].copy_from_slice(&projected_utf16.to_le_bytes());

    selected_leaf.fill(0);
    selected_leaf[0] = M11_POINT_PATH_KIND_PARAGRAPH;
    selected_leaf[1] = M11_POINT_PATH_FLAG_NONCONTIGUOUS | M11_POINT_PATH_FLAG_SELECTED;
    selected_leaf[2..4].copy_from_slice(&1_u16.to_le_bytes());
    selected_leaf[4..8].copy_from_slice(&0_u32.to_le_bytes());
    selected_leaf[8..12].copy_from_slice(&range.start.bytes.to_le_bytes());
    selected_leaf[12..16].copy_from_slice(&range.end.bytes.to_le_bytes());
    selected_leaf[16..20].copy_from_slice(&range.start.utf16.to_le_bytes());
    selected_leaf[20..24].copy_from_slice(&range.end.utf16.to_le_bytes());
    selected_leaf[24..28].copy_from_slice(&child_first_line.to_le_bytes());
    selected_leaf[28..32].copy_from_slice(&child_line_count.to_le_bytes());
    selected_leaf[32..36].copy_from_slice(&projected_utf8.to_le_bytes());
    selected_leaf[36..40].copy_from_slice(&projected_utf16.to_le_bytes());
    true
}

fn translate_leaf_relative_records(
    leaf_start: u64,
    green: &mut [u8],
    projection: &mut [u8],
) -> bool {
    for record in [green, projection] {
        for offset in [16, 24, 32, 40] {
            let Some(absolute) = read_u64(record, offset).checked_add(leaf_start) else {
                return false;
            };
            record[offset..offset + 8].copy_from_slice(&absolute.to_le_bytes());
        }
    }
    true
}

fn translate_leaf_relative_fenced_code_records(
    leaf_start: u64,
    green: &mut [u8],
    projection: &mut [u8],
) -> bool {
    if !translate_leaf_relative_records(leaf_start, green, projection) {
        return false;
    }
    for offset in [56, 60, 64, 68, 72, 76] {
        let relative = read_u32(green, offset);
        if relative == M11_FENCE_ABSENT_CUT {
            continue;
        }
        let Some(absolute) = u64::from(relative).checked_add(leaf_start) else {
            return false;
        };
        let Ok(absolute) = u32::try_from(absolute) else {
            return false;
        };
        green[offset..offset + 4].copy_from_slice(&absolute.to_le_bytes());
    }
    true
}

fn translate_leaf_relative_atx_heading_records(
    leaf_start: u64,
    green: &mut [u8],
    projection: &mut [u8],
) -> bool {
    if !translate_leaf_relative_records(leaf_start, green, projection) {
        return false;
    }
    for offset in [56, 60, 64, 68, 72, 76] {
        let relative = read_u32(green, offset);
        if relative == M11_ATX_HEADING_ABSENT_CUT {
            continue;
        }
        let Some(absolute) = u64::from(relative).checked_add(leaf_start) else {
            return false;
        };
        let Ok(absolute) = u32::try_from(absolute) else {
            return false;
        };
        green[offset..offset + 4].copy_from_slice(&absolute.to_le_bytes());
    }
    true
}

fn translate_leaf_relative_setext_heading_records(
    leaf_start: u64,
    green: &mut [u8],
    projection: &mut [u8],
) -> bool {
    if !translate_leaf_relative_records(leaf_start, green, projection) {
        return false;
    }
    for offset in [56, 60, 64, 68] {
        let Some(absolute) = u64::from(read_u32(green, offset)).checked_add(leaf_start) else {
            return false;
        };
        let Ok(absolute) = u32::try_from(absolute) else {
            return false;
        };
        green[offset..offset + 4].copy_from_slice(&absolute.to_le_bytes());
    }
    true
}

fn translate_leaf_relative_thematic_break_records(
    leaf_start: u64,
    green: &mut [u8],
    projection: &mut [u8],
) -> bool {
    if !translate_leaf_relative_records(leaf_start, green, projection) {
        return false;
    }
    for offset in [56, 60, 64, 68] {
        let Some(absolute) = u64::from(read_u32(green, offset)).checked_add(leaf_start) else {
            return false;
        };
        let Ok(absolute) = u32::try_from(absolute) else {
            return false;
        };
        green[offset..offset + 4].copy_from_slice(&absolute.to_le_bytes());
    }
    true
}

fn synthesize_empty_block_records(green: &mut [u8], projection: &mut [u8]) -> bool {
    if green.len() != M11_GREEN_RECORD_BYTES || projection.len() != M11_PROJECTION_RECORD_BYTES {
        return false;
    }
    green.fill(0);
    green[..8].copy_from_slice(GREEN_MAGIC);
    green[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
    projection.fill(0);
    projection[..8].copy_from_slice(PROJECTION_MAGIC);
    projection[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
    true
}

fn synthesize_literal_block_records(
    kind: M11LiteralBlockKind,
    range: HostMetricRange,
    reference_definition_count: u64,
    green: &mut [u8],
    projection: &mut [u8],
) -> bool {
    if green.len() != M11_GREEN_RECORD_BYTES
        || projection.len() != M11_PROJECTION_RECORD_BYTES
        || range.start.bytes >= range.end.bytes
        || range.start.utf16 >= range.end.utf16
    {
        return false;
    }
    let (variant, reason_tag, reason_detail, projected_end, projection_runs): (
        u8,
        u32,
        u32,
        u32,
        u64,
    ) = match kind {
        M11LiteralBlockKind::Blank if reference_definition_count == 0 => {
            (2, 1, 0, range.end.bytes, 1)
        }
        M11LiteralBlockKind::DefinitionsOnly if reference_definition_count != 0 => {
            (0, 0, 0, range.start.bytes, 0)
        }
        M11LiteralBlockKind::Unsupported(reason) if reference_definition_count == 0 => {
            let tag = reason >> 16;
            let detail = reason & 0xffff;
            let opener = match tag {
                // Generic unsupported opener: the detail already is the
                // stable public opener code.
                2 if (1..=9).contains(&detail) => detail,
                // Parser-certified unsupported block-quote shapes collapse
                // to the public BlockQuote opener while preserving literal
                // source authority.
                3 if (1..=13).contains(&detail) => 1,
                // Parser-certified unsupported list shapes collapse to the
                // public List opener.
                4 if (1..=9).contains(&detail) => 7,
                _ => return false,
            };
            (2, 2, opener, range.end.bytes, 1)
        }
        M11LiteralBlockKind::Blank
        | M11LiteralBlockKind::DefinitionsOnly
        | M11LiteralBlockKind::Unsupported(_) => return false,
    };

    green.fill(0);
    green[..8].copy_from_slice(GREEN_MAGIC);
    green[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
    green[12] = variant;
    green[16..24].copy_from_slice(&u64::from(range.start.bytes).to_le_bytes());
    green[24..32].copy_from_slice(&u64::from(range.end.bytes).to_le_bytes());
    green[32..40].copy_from_slice(&u64::from(range.start.bytes).to_le_bytes());
    green[40..48].copy_from_slice(&u64::from(range.start.bytes).to_le_bytes());
    green[48..56].copy_from_slice(&reference_definition_count.to_le_bytes());
    green[56..60].copy_from_slice(&reason_tag.to_le_bytes());
    green[60..64].copy_from_slice(&reason_detail.to_le_bytes());

    projection.fill(0);
    projection[..8].copy_from_slice(PROJECTION_MAGIC);
    projection[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
    projection[12] = variant;
    projection[16..24].copy_from_slice(&u64::from(range.start.bytes).to_le_bytes());
    projection[24..32].copy_from_slice(&u64::from(range.end.bytes).to_le_bytes());
    projection[32..40].copy_from_slice(&u64::from(range.start.bytes).to_le_bytes());
    projection[40..48].copy_from_slice(&u64::from(projected_end).to_le_bytes());
    projection[48..56].copy_from_slice(&projection_runs.to_le_bytes());
    true
}

#[derive(Clone, Copy)]
struct InlineViewportFacts {
    fact_count: usize,
    page_count: usize,
    encoded_bytes: usize,
    leaf_start: u64,
    leaf_end: u64,
    leaf_bytes: usize,
}

fn validate_inline_metadata(
    bytes: &[u8; M11_INLINE_META_RECORD_BYTES],
    syntax_profile: u32,
    source: SourceVersion,
    projection_count: u64,
) -> Option<InlineViewportFacts> {
    let disposition = bytes[12];
    let fact_count = usize::try_from(read_u32(bytes, 20)).ok()?;
    let leaf_start = read_u64(bytes, 24);
    let leaf_end = read_u64(bytes, 32);
    let leaf_bytes = usize::try_from(leaf_end.checked_sub(leaf_start)?).ok()?;
    let page_count = fact_count.div_ceil(M11_INLINE_FACTS_PER_PAGE);
    let expected_projection_count = u64::try_from(2_usize.checked_add(page_count)?).ok()?;
    let fact_bytes = fact_count.checked_mul(M11_INLINE_FACT_RECORD_BYTES)?;
    let encoded_bytes = M11_INLINE_META_RECORD_BYTES.checked_add(fact_bytes)?;
    (&bytes[..8] == M11_INLINE_META_MAGIC
        && read_u32(bytes, 8) == M11_INLINE_SCHEMA
        && matches!(disposition, 1 | 2)
        && bytes[13..16] == [0; 3]
        && read_u32(bytes, 16) == syntax_profile
        && leaf_start <= leaf_end
        && leaf_end <= u64::from(source.utf8_length)
        && read_u32(bytes, 40) == M11_INLINE_FACT_RECORD_BYTES as u32
        && read_u32(bytes, 44) == 0
        && (disposition == 1 || fact_count == 0)
        && projection_count == expected_projection_count)
        .then_some(InlineViewportFacts {
            fact_count,
            page_count,
            encoded_bytes,
            leaf_start,
            leaf_end,
            leaf_bytes,
        })
}

fn inline_describes_projection(inline: InlineViewportFacts, projection: &[u8]) -> bool {
    projection.len() == M11_PROJECTION_RECORD_BYTES
        && projection_is_inline_bearing(projection)
        && inline.leaf_start == read_u64(projection, 32)
        && inline.leaf_end == read_u64(projection, 40)
}

fn projection_is_inline_bearing(projection: &[u8]) -> bool {
    projection.get(12).is_some_and(|variant| {
        matches!(
            *variant,
            1 | M11_ATX_HEADING_VARIANT | M11_SETEXT_HEADING_VARIANT
        )
    })
}

fn valid_inline_page_header(
    bytes: &[u8; M11_INLINE_PAGE_HEADER_BYTES],
    page_ordinal: usize,
    fact_count: usize,
) -> bool {
    &bytes[..8] == M11_INLINE_PAGE_MAGIC
        && read_u32(bytes, 8) == M11_INLINE_SCHEMA
        && usize::try_from(read_u32(bytes, 12)).ok() == Some(page_ordinal)
        && usize::try_from(read_u32(bytes, 16)).ok() == Some(fact_count)
        && read_u32(bytes, 20) == 0
}

fn encode_inline_projection_fact_record(
    fact: M11HostInlineProjectionFact,
    record: &mut [u8],
) -> Result<(), HostStoreError> {
    if record.len() != M11_INLINE_FACT_RECORD_BYTES {
        return Err(HostStoreError::invalid(
            "inline fact record has a noncanonical width",
        ));
    }
    record.fill(0);
    record[0] = fact.kind() as u8;
    record[4..8].copy_from_slice(&fact.relative_start().to_le_bytes());
    record[8..12].copy_from_slice(&fact.relative_len().to_le_bytes());
    if let Some((first, second)) = fact.character_reference() {
        record[1] = if second.is_some() { 2 } else { 1 };
        record[12..16].copy_from_slice(&(first as u32).to_le_bytes());
        record[16..20].copy_from_slice(&second.map_or(0, |scalar| scalar as u32).to_le_bytes());
    } else {
        record[1] = fact.flags();
        let content_start = fact
            .relative_start()
            .checked_add(fact.content_offset())
            .ok_or_else(|| HostStoreError::invalid("inline fact content offset overflowed"))?;
        record[12..16].copy_from_slice(&content_start.to_le_bytes());
        record[16..20].copy_from_slice(&fact.content_len().to_le_bytes());
    }
    Ok(())
}

fn valid_inline_fact_records(bytes: &[u8], leaf_bytes: usize) -> bool {
    bytes
        .chunks_exact(M11_INLINE_FACT_RECORD_BYTES)
        .all(|record| {
            let kind = record[0];
            let flags = record[1];
            let start = u64::from(read_u32(record, 4));
            let source_len = u64::from(read_u32(record, 8));
            let Some(end) = start.checked_add(source_len) else {
                return false;
            };
            if record[2..4] != [0; 2] || source_len == 0 || end > leaf_bytes as u64 {
                return false;
            }
            if kind == 9 {
                let first = read_u32(record, 12);
                let second = read_u32(record, 16);
                return (4..=33).contains(&source_len)
                    && char::from_u32(first).is_some()
                    && match flags {
                        1 => second == 0,
                        2 => second != 0 && char::from_u32(second).is_some(),
                        _ => false,
                    };
            }
            if !matches!(kind, 1..=8)
                || (kind != 3 && flags != 0)
                || (kind == 3 && flags & !0x03 != 0)
            {
                return false;
            }
            let content_start = u64::from(read_u32(record, 12));
            let Some(content_end) = content_start.checked_add(u64::from(read_u32(record, 16)))
            else {
                return false;
            };
            if content_start < start || content_end > end {
                return false;
            }
            let opener = content_start - start;
            let closer = end - content_end;
            match kind {
                1 => opener == 1 && closer == 1,
                2 => opener == 2 && closer == 2,
                3 => opener > 0 && opener == closer,
                4 => matches!(opener, 1 | 2) && opener == closer,
                5 | 6 => opener == 1 && closer == 1,
                7 => end - start == 2 && opener == 1 && closer == 0,
                8 => opener > 0 && matches!(content_end - content_start, 1 | 2) && closer == 0,
                _ => false,
            }
        })
}

fn validate_inline_sidecar_begin_frame(
    frame: &[u8],
    envelope: HotInlineSidecarEnvelopeMetrics,
) -> Result<(), HostStoreError> {
    let hio1_bytes = usize::try_from(envelope.hio1_encoded_bytes)
        .map_err(|_| HostStoreError::invalid("HIO1 length exceeds this target"))?;
    let descriptor_bytes = usize::try_from(envelope.ipr2_descriptor_bytes)
        .map_err(|_| HostStoreError::invalid("sidecar descriptor length exceeds this target"))?;
    let expected_frame_bytes = M11_INLINE_OVERLAY_BEGIN_HEADER_BYTES
        .checked_add(hio1_bytes)
        .and_then(|bytes| bytes.checked_add(descriptor_bytes))
        .ok_or_else(|| HostStoreError::invalid("sidecar Begin length overflowed"))?;
    if hio1_bytes < M11_INLINE_OVERLAY_DIGEST_BYTES
        || frame.len() != expected_frame_bytes
        || read_u32(frame, 4) != envelope.hio1_encoded_bytes
        || read_u32(frame, 8) != envelope.ipr2_descriptor_bytes
    {
        return Err(HostStoreError::new(
            HostRejectReason::CorruptPublication,
            "sidecar Begin geometry disagrees with its offer",
        ));
    }
    let digest_start = M11_INLINE_OVERLAY_BEGIN_HEADER_BYTES
        .checked_add(hio1_bytes - M11_INLINE_OVERLAY_DIGEST_BYTES)
        .ok_or_else(|| HostStoreError::invalid("HIO1 digest offset overflowed"))?;
    let digest_end = digest_start
        .checked_add(M11_INLINE_OVERLAY_DIGEST_BYTES)
        .ok_or_else(|| HostStoreError::invalid("HIO1 digest offset overflowed"))?;
    if frame.get(digest_start..digest_end) != Some(&envelope.hio1_envelope_digest256) {
        return Err(HostStoreError::new(
            HostRejectReason::CorruptPublication,
            "sidecar Begin HIO1 digest disagrees with its offer",
        ));
    }
    Ok(())
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("fixed record"))
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("fixed record"))
}

impl Drop for NativeCandidateHost {
    fn drop(&mut self) {
        if self.closed {
            return;
        }
        // Explicitly abnormal containment used only by registry emergency
        // destruction/finalization. Normal ownership always uses fuelled close.
        let _ = self.begin_close();
        if let Some(engine) = self.engine.as_mut() {
            let _ = engine.poll_close(usize::MAX);
        }
        if let Some(sidecar) = self.inline_sidecar.as_mut() {
            let _ = sidecar.poll_close(usize::MAX);
        }
        self.closed = true;
    }
}

fn canonical_record_count(
    engine: &M11CandidateHost,
    installed: M11HostInstalledCandidate,
) -> Result<u32, HostStoreError> {
    let total = [
        M11HostRole::SourceFacts,
        M11HostRole::Green,
        M11HostRole::Projection,
        M11HostRole::References,
        M11HostRole::CleanEofOnly,
    ]
    .into_iter()
    .try_fold(0_u64, |total, role| {
        engine
            .role_record_count(installed, role)
            .map_err(map_engine_error)
            .and_then(|count| {
                total.checked_add(count).ok_or_else(|| {
                    HostStoreError::new(
                        HostRejectReason::InternalFault,
                        "canonical record count overflowed",
                    )
                })
            })
    })?;
    u32::try_from(total).map_err(|_| {
        HostStoreError::new(
            HostRejectReason::ForegroundBoundExceeded,
            "canonical record count exceeds the v1 transport",
        )
    })
}

fn map_engine_error(error: M11HostError) -> HostStoreError {
    if error.is_cross_authority() {
        HostStoreError::new(
            HostRejectReason::ExactSourceMismatch,
            "candidate crossed host authority",
        )
    } else if error.is_base_mismatch() {
        HostStoreError::new(
            HostRejectReason::BaseMismatch,
            "candidate does not match the exact installed base",
        )
    } else if error.is_stale() {
        HostStoreError::new(HostRejectReason::StaleSource, "candidate is stale")
    } else if error.is_backpressure() {
        HostStoreError::new(HostRejectReason::Backpressure, "host engine is busy")
    } else if error.is_not_ready() {
        HostStoreError::new(HostRejectReason::NotReady, "host engine is not ready")
    } else if error.is_zero_fuel() {
        HostStoreError::new(HostRejectReason::Invalid, "host poll fuel is zero")
    } else if error.is_resource_limit() {
        HostStoreError::new(
            HostRejectReason::AllocationFailed,
            "host engine resource envelope was exceeded",
        )
    } else if error.is_invalid() {
        HostStoreError::new(
            HostRejectReason::CorruptPublication,
            "candidate snapshot is invalid",
        )
    } else {
        HostStoreError::new(
            HostRejectReason::InternalFault,
            "candidate host engine failed",
        )
    }
}

fn structural_ordinal_window_failure(
    query: HostStructuralOrdinalWindowQuery,
    total_entry_count: u64,
    reason: HostStructuralOrdinalWindowFailureReason,
    receipt: HostStructuralOrdinalWindowReceipt,
) -> HostStructuralOrdinalWindowOutcome {
    HostStructuralOrdinalWindowOutcome::Failure {
        source_version: query.source_version,
        total_entry_count,
        start_entry_ordinal: query.start_entry_ordinal,
        reason,
        receipt,
    }
}

fn structural_ordinal_window_metric(
    bytes: u64,
    utf16: u64,
) -> Result<HostSourceMetric, HostStoreError> {
    Ok(HostSourceMetric {
        bytes: u32::try_from(bytes)
            .map_err(|_| HostStoreError::invalid("ordinal-window UTF-8 cut overflowed"))?,
        utf16: u32::try_from(utf16)
            .map_err(|_| HostStoreError::invalid("ordinal-window UTF-16 cut overflowed"))?,
    })
}

fn structural_ordinal_window_receipt(
    window: M11HostPersistentBlockOrdinalWindow,
) -> Result<HostStructuralOrdinalWindowReceipt, HostStoreError> {
    Ok(HostStructuralOrdinalWindowReceipt {
        storage_pages_visited: u32::try_from(window.storage_pages_visited())
            .map_err(|_| HostStoreError::invalid("ordinal-window page receipt overflowed"))?,
        tree_nodes_visited: u32::try_from(window.node_headers_decoded())
            .map_err(|_| HostStoreError::invalid("ordinal-window tree receipt overflowed"))?,
        packed_entries_inspected: u32::try_from(window.packed_entries_inspected())
            .map_err(|_| HostStoreError::invalid("ordinal-window entry receipt overflowed"))?,
        summary_nodes_skipped: u32::try_from(window.summary_combinations())
            .map_err(|_| HostStoreError::invalid("ordinal-window summary receipt overflowed"))?,
    })
}

fn recursive_green_ordinal_window_receipt(
    window: M11HostRecursiveGreenRowOrdinalWindow,
) -> Result<HostStructuralOrdinalWindowReceipt, HostStoreError> {
    Ok(HostStructuralOrdinalWindowReceipt {
        storage_pages_visited: u32::try_from(window.storage_pages_visited())
            .map_err(|_| HostStoreError::invalid("Green ordinal page receipt overflowed"))?,
        tree_nodes_visited: u32::try_from(window.node_headers_decoded())
            .map_err(|_| HostStoreError::invalid("Green ordinal tree receipt overflowed"))?,
        packed_entries_inspected: u32::try_from(window.packed_entries_inspected())
            .map_err(|_| HostStoreError::invalid("Green ordinal event receipt overflowed"))?,
        summary_nodes_skipped: u32::try_from(window.summary_combinations())
            .map_err(|_| HostStoreError::invalid("Green ordinal summary receipt overflowed"))?,
    })
}

pub(crate) fn source_root_u64(root: [u32; 2]) -> u64 {
    (u64::from(root[0]) << 32) | u64::from(root[1])
}

fn id128_bytes(identity: Id128) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    for (index, word) in identity.into_iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v3_endpoint::standard_document_runtime_config;
    use crate::v3_publication_wire::{
        decode_publication_packet_envelope, encode_publication_packet_into,
        encode_viewport_presentation_child_frame_into, encode_viewport_presentation_directory_into,
        encode_viewport_presentation_end_frame_into,
        encode_viewport_presentation_parent_frame_into,
        viewport_presentation_aggregate_envelope_digest256,
        viewport_presentation_root_stream_digest256, PublicationPacketFrameInput,
        PublicationPacketInput, ViewportPresentationBegin, ViewportPresentationBinding,
        ViewportPresentationChildFrameInput, ViewportPresentationCommitRequest,
        ViewportPresentationDirectoryEntry, ViewportPresentationEndFrame,
        ViewportPresentationEnvelopeMetrics, ViewportPresentationFrameKind,
        ViewportPresentationMetricRange, ViewportPresentationMode, ViewportPresentationOfferLimits,
        ViewportPresentationQueryLimits, ViewportPresentationTransportDigest,
        ViewportPresentationVisitStart, VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES,
        VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES, VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES,
        VIEWPORT_PRESENTATION_END_FRAME_BYTES, VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES,
    };
    use flark_engine::m11_host::{M11HostInlineProjectionDescriptor, M11HostLimits};
    use flark_engine::parser_internal::{
        splice_m11_block_sequence_atomic, BlockQuoteLineV1, M11BlockQuoteProjectionBuild,
        M11BlockQuoteProjectionBuildStatus, M11BlockRoleRecord, M11BlockSequenceBuild,
        M11BlockSequenceBuildStatus, M11BlockSequenceEntry, M11BlockSequenceRoot,
        M11BlockUnsupportedReason, M11CandidateBuild, M11CandidateBuildPoll,
        M11HotInlineCanonicalLineEnding, M11HotInlineSidecarDescriptor,
        M11HotInlineSidecarDisposition, M11HotInlineSidecarFrame, M11HotInlineSidecarFrameKind,
        M11HotInlineSidecarSnapshotEncoder, M11HotInlineSidecarSnapshotPoll, M11InlineLinkValue,
        M11InlineProjectionBuild, M11InlineProjectionBuildStatus, M11InlineProjectionFact,
        M11InlineProjectionKind, M11OwnedSnapshotPoll, M11ReferenceRange, M11ReferenceRecord,
        M11RoleRecords, M11SnapshotFrameKind,
    };
    use flark_engine::{
        DocumentRuntime, ParserProfileId, RuntimeSourceFactsPoll, SourceFactsRootLimits,
        SourceFactsScanProfile, SourceRevision, SourceSeedBuilder, SOURCE_SEED_PAGE_MAX_UTF16,
    };
    use flark_parser::{
        M11CleanParseJob, M11CleanParsePoll, M11ParserCandidate, M11ParserCandidateWriterPoll,
        M11PersistentRecursiveGreenBuildStatus, M11PersistentRecursiveGreenCleanPlan,
    };

    struct TestFrame {
        offer_id: Id128,
        ordinal: u32,
        first_record_ordinal: u32,
        record_count: u32,
        digest: Digest128,
        bytes: Box<[u8]>,
    }

    struct TestSnapshot {
        source: SourceVersion,
        offer: OfferBegin,
        frames: Vec<TestFrame>,
        commit: CommitRequest,
    }

    struct TestInlineSidecar {
        offer_id: Id128,
        publication_session: Id128,
        binding: crate::v3_publication_wire::HotInlineSidecarBinding,
        envelope: crate::v3_publication_wire::HotInlineSidecarEnvelopeMetrics,
        limits: crate::v3_publication_wire::OfferLimits,
        frames: Vec<TestFrame>,
        commit: HotInlineSidecarCommitRequest,
    }

    struct TestViewportPresentation {
        begin: ViewportPresentationBegin,
        frames: Vec<TestFrame>,
        commit: ViewportPresentationCommitRequest,
    }

    impl TestInlineSidecar {
        fn begin(&self, base_ack: StructuralAck) -> HotInlineSidecarBegin {
            HotInlineSidecarBegin {
                schema: HOT_INLINE_SIDECAR_SCHEMA,
                mode: crate::v3_publication_wire::HotInlineSidecarMode::HotInlineSidecar,
                offer_id: self.offer_id,
                publication_session: self.publication_session,
                base_ack,
                binding: self.binding,
                envelope: self.envelope,
                limits: self.limits,
            }
        }
    }

    fn source_store(text: &str, revision: u64) -> flark_engine::SourceStore {
        let utf16 = text.encode_utf16().count();
        let mut seed = SourceSeedBuilder::new(SourceRevision::new(revision), utf16);
        if text.is_empty() {
            seed.append_page(0..0, "").expect("empty source page");
        } else {
            let mut byte_start = 0;
            let mut utf16_start = 0;
            while byte_start < text.len() {
                let mut byte_end = (byte_start + SOURCE_SEED_PAGE_MAX_UTF16).min(text.len());
                while !text.is_char_boundary(byte_end) {
                    byte_end -= 1;
                }
                let page = &text[byte_start..byte_end];
                let utf16_end = utf16_start + page.encode_utf16().count();
                seed.append_page(utf16_start..utf16_end, page)
                    .expect("source page");
                byte_start = byte_end;
                utf16_start = utf16_end;
            }
        }
        seed.finalize().expect("source store")
    }

    fn push_test_u32(output: &mut Vec<u8>, value: u32) {
        output.extend_from_slice(&value.to_le_bytes());
    }

    fn push_test_u64(output: &mut Vec<u8>, value: u64) {
        output.extend_from_slice(&value.to_le_bytes());
    }

    fn test_green_record(source_bytes: u64) -> Box<[u8]> {
        let mut output = Vec::with_capacity(M11_GREEN_RECORD_BYTES);
        output.extend_from_slice(GREEN_MAGIC);
        push_test_u32(&mut output, M11_ROLE_SCHEMA);
        output.extend_from_slice(&[1, 0, 0, 0]);
        push_test_u64(&mut output, 0);
        push_test_u64(&mut output, source_bytes);
        push_test_u64(&mut output, 0);
        push_test_u64(&mut output, source_bytes);
        push_test_u64(&mut output, 0);
        push_test_u32(&mut output, 0);
        push_test_u32(&mut output, 0);
        push_test_u64(&mut output, 0);
        push_test_u64(&mut output, 0);
        assert_eq!(output.len(), M11_GREEN_RECORD_BYTES);
        output.into_boxed_slice()
    }

    fn test_projection_record(source_bytes: u64) -> Box<[u8]> {
        let mut output = Vec::with_capacity(M11_PROJECTION_RECORD_BYTES);
        output.extend_from_slice(PROJECTION_MAGIC);
        push_test_u32(&mut output, M11_ROLE_SCHEMA);
        output.extend_from_slice(&[1, 0, 0, 0]);
        push_test_u64(&mut output, 0);
        push_test_u64(&mut output, source_bytes);
        push_test_u64(&mut output, 0);
        push_test_u64(&mut output, source_bytes);
        push_test_u64(&mut output, 1);
        assert_eq!(output.len(), M11_PROJECTION_RECORD_BYTES);
        output.into_boxed_slice()
    }

    fn snapshot(
        document_session: Id128,
        publication_session: Id128,
        revision: u32,
        parse_generation: u32,
        source_facts_records: usize,
    ) -> TestSnapshot {
        let text = format!("revision {revision}\n");
        snapshot_with_text(
            document_session,
            publication_session,
            revision,
            parse_generation,
            source_facts_records,
            &text,
        )
    }

    fn snapshot_with_text(
        document_session: Id128,
        publication_session: Id128,
        revision: u32,
        parse_generation: u32,
        source_facts_records: usize,
        text: &str,
    ) -> TestSnapshot {
        snapshot_with_role_validity(
            document_session,
            publication_session,
            revision,
            parse_generation,
            source_facts_records,
            text,
            true,
            false,
        )
    }

    fn snapshot_with_reference(
        document_session: Id128,
        publication_session: Id128,
        revision: u32,
        parse_generation: u32,
    ) -> TestSnapshot {
        snapshot_with_role_validity(
            document_session,
            publication_session,
            revision,
            parse_generation,
            1,
            "[a]: /x\n",
            true,
            true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn snapshot_with_role_validity(
        document_session: Id128,
        publication_session: Id128,
        revision: u32,
        parse_generation: u32,
        source_facts_records: usize,
        text: &str,
        valid_query_roles: bool,
        include_reference: bool,
    ) -> TestSnapshot {
        let mut runtime =
            DocumentRuntime::new(text, standard_document_runtime_config()).expect("test runtime");
        let source_store = source_store(text, u64::from(revision));
        let source_version = source_store.version();
        let green = if valid_query_roles {
            test_green_record(source_version.byte_len() as u64)
        } else {
            Box::<[u8]>::from(&b"not-green"[..])
        };
        let projection = if valid_query_roles {
            test_projection_record(source_version.byte_len() as u64)
        } else {
            Box::<[u8]>::from(&b"not-projection"[..])
        };
        let records = M11RoleRecords::new(
            (0..source_facts_records).map(|ordinal| {
                vec![
                    u8::try_from(ordinal & 0xff).expect("bounded ordinal"),
                    u8::try_from((ordinal >> 8) & 0xff).expect("bounded ordinal"),
                ]
                .into_boxed_slice()
            }),
            green,
            projection,
        )
        .expect("role records");
        let mut build = M11CandidateBuild::new(
            &mut runtime,
            id128_bytes(document_session),
            id128_bytes(publication_session),
            source_version,
            u64::from(parse_generation),
            1,
            records,
        )
        .expect("candidate build");
        if include_reference {
            build
                .offer_reference(
                    &runtime,
                    M11ReferenceRecord::new(
                        M11ReferenceRange::new(0..7, 0..7),
                        M11ReferenceRange::new(1..2, 1..2),
                        M11ReferenceRange::new(5..7, 5..7),
                        None,
                        Box::<[u8]>::from(&b"a"[..]),
                        Box::<[u8]>::from(&b"/x"[..]),
                        None,
                    ),
                )
                .expect("offer reference");
            while !build.references_idle() {
                assert!(matches!(
                    build.poll(&mut runtime, 1).expect("reference poll"),
                    M11CandidateBuildPoll::Pending { .. }
                ));
            }
        }
        build
            .finish_references(&runtime)
            .expect("finish references");
        while let M11CandidateBuildPoll::Pending { .. } =
            build.poll(&mut runtime, 256).expect("candidate poll")
        {}
        let publication = build.into_publication().expect("publication");
        let source = SourceVersion {
            document_session,
            revision,
            utf8_length: u32::try_from(text.len()).expect("source bytes"),
            utf16_length: u32::try_from(text.encode_utf16().count()).expect("source UTF-16"),
            content_hash128: [revision, revision + 1, revision + 2, revision + 3],
        };
        snapshot_from_publication(
            runtime,
            publication,
            source,
            publication_session,
            parse_generation,
        )
    }

    fn snapshot_from_publication(
        mut runtime: DocumentRuntime,
        publication: flark_engine::parser_internal::M11CandidatePublication,
        source: SourceVersion,
        publication_session: Id128,
        parse_generation: u32,
    ) -> TestSnapshot {
        let descriptor = publication.descriptor(&runtime).expect("descriptor");
        let mut stream = match Box::new(publication).into_snapshot_stream(&runtime) {
            Ok(stream) => stream,
            Err(failure) => {
                let (error, mut publication) = failure.into_parts();
                publication
                    .begin_close(&mut runtime)
                    .expect("failed publication close");
                while !publication
                    .poll_close(&mut runtime, 256)
                    .expect("failed publication reclaim")
                {}
                panic!("snapshot stream: {error}")
            }
        };
        let mut frames = vec![stream.begin_frame().expect("begin frame")];
        loop {
            match stream.poll(&runtime, 256).expect("snapshot poll") {
                M11OwnedSnapshotPoll::Pending { .. } => {}
                M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                    panic!("full snapshot requested exact-base replay")
                }
                M11OwnedSnapshotPoll::Frame { frame, .. } => {
                    let complete = frame.kind == M11SnapshotFrameKind::End;
                    frames.push(frame);
                    if complete {
                        break;
                    }
                }
            }
        }
        stream.begin_close(&mut runtime).expect("producer close");
        while !stream
            .poll_close(&mut runtime, 256)
            .expect("producer reclaim")
        {}
        runtime.begin_close().expect("begin runtime close");
        while !runtime.poll_close(256).expect("runtime reclaim").complete {}

        test_snapshot_from_frames(
            descriptor,
            frames,
            source,
            publication_session,
            parse_generation,
        )
    }

    fn test_snapshot_from_frames(
        descriptor: flark_engine::parser_internal::M11CandidateDescriptor,
        frames: Vec<flark_engine::parser_internal::M11SnapshotFrame>,
        source: SourceVersion,
        publication_session: Id128,
        parse_generation: u32,
    ) -> TestSnapshot {
        let frame_count = u32::try_from(frames.len()).expect("frame count");
        let encoded_bytes = frames
            .iter()
            .try_fold(0_u32, |total, frame| {
                total.checked_add(u32::try_from(frame.bytes.len()).ok()?)
            })
            .expect("encoded bytes");
        let maximum_frame_bytes = frames
            .iter()
            .map(|frame| u32::try_from(frame.bytes.len()).expect("frame bytes"))
            .max()
            .expect("snapshot has Begin and End frames");
        let canonical_record_count =
            u32::try_from(descriptor.canonical_record_count).expect("record count");
        let offer = OfferBegin {
            schema: 1,
            offer_id: [parse_generation + 100, 2, 3, 4],
            publication_session,
            target_host_revision: parse_generation,
            source_version: source,
            source_root: [
                u32::try_from(descriptor.source_root >> 32).expect("root high"),
                u32::try_from(descriptor.source_root & u64::from(u32::MAX)).expect("root low"),
            ],
            parse_generation,
            grammar_revision: 1,
            syntax_profile: 1,
            authority_mask: 0x1f,
            mode: PublicationMode::FullSnapshot,
            base_ack: None,
            transferred_record_count: canonical_record_count,
            target_record_count: canonical_record_count,
            limits: crate::v3_publication_wire::OfferLimits {
                maximum_frame_count: frame_count,
                maximum_encoded_frame_bytes: encoded_bytes,
                maximum_packet_bytes: u32::try_from(MAXIMUM_PACKET_ENCODED_BYTES)
                    .expect("packet ceiling"),
                maximum_frame_bytes,
                maximum_program_children: 128,
            },
        };
        let mut transport = CandidateTransportDigest::new();
        let mut first_record_ordinal = 0_u32;
        let mut stream_digest = None;
        let mut test_frames = Vec::with_capacity(frames.len());
        for (ordinal, frame) in frames.into_iter().enumerate() {
            let ordinal = u32::try_from(ordinal).expect("frame ordinal");
            let kind = match frame.kind {
                M11SnapshotFrameKind::Begin => CandidateSnapshotFrameKind::Begin,
                M11SnapshotFrameKind::SourceFactsReplacementPage => {
                    CandidateSnapshotFrameKind::SourceFactsReplacementPage
                }
                M11SnapshotFrameKind::BlockSequenceReplacementPage => {
                    CandidateSnapshotFrameKind::BlockSequenceReplacementPage
                }
                M11SnapshotFrameKind::RecursiveGreenReplacementPage => {
                    CandidateSnapshotFrameKind::RecursiveGreenReplacementPage
                }
                M11SnapshotFrameKind::Node => CandidateSnapshotFrameKind::Node,
                M11SnapshotFrameKind::End => CandidateSnapshotFrameKind::End,
            };
            let digest256 = transport
                .push(
                    ordinal,
                    first_record_ordinal,
                    frame.canonical_record_count,
                    kind,
                    &frame.bytes,
                )
                .expect("transport digest");
            if let Some(digest) = frame.canonical_stream_digest256 {
                stream_digest = Some(digest);
            }
            test_frames.push(TestFrame {
                offer_id: offer.offer_id,
                ordinal,
                first_record_ordinal,
                record_count: frame.canonical_record_count,
                digest: protocol_digest128_from_blake3(
                    ProtocolDigestDomain::CandidateFrame,
                    digest256,
                ),
                bytes: frame.bytes,
            });
            first_record_ordinal += frame.canonical_record_count;
        }
        let receipt = transport.finish();
        let commit = CommitRequest {
            offer_id: offer.offer_id,
            actual_frame_count: receipt.frame_count,
            actual_encoded_frame_bytes: receipt.encoded_frame_bytes,
            rolling_transport_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::CandidateTransport,
                receipt.digest256,
            ),
            canonical_stream_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::CandidateStream,
                stream_digest.expect("End stream digest"),
            ),
        };
        TestSnapshot {
            source,
            offer,
            frames: test_frames,
            commit,
        }
    }

    fn test_inline_sidecar_from_frames(
        offer_id: Id128,
        publication_session: Id128,
        binding: crate::v3_publication_wire::HotInlineSidecarBinding,
        descriptor: M11HotInlineSidecarDescriptor,
        frames: Vec<M11HotInlineSidecarFrame>,
    ) -> TestInlineSidecar {
        let disposition = match descriptor.disposition() {
            M11HotInlineSidecarDisposition::Authoritative {
                logical_page_count,
                fact_count,
                storage_page_count,
                link_value_entry_count,
                link_value_storage_page_count,
                link_value_encoded_bytes,
                ordered_commitment256,
            } => HotInlineSidecarDisposition::Authoritative {
                logical_page_count,
                fact_count,
                storage_page_count,
                link_value_entry_count,
                link_value_storage_page_count,
                link_value_encoded_bytes,
                ordered_commitment256,
            },
            M11HotInlineSidecarDisposition::ProjectedInlineAuthoritative {
                logical_page_count,
                fact_count,
                storage_page_count,
                ordered_commitment256,
                ..
            } => HotInlineSidecarDisposition::Authoritative {
                logical_page_count,
                fact_count,
                storage_page_count,
                link_value_entry_count: 0,
                link_value_storage_page_count: 0,
                link_value_encoded_bytes: 0,
                ordered_commitment256,
            },
            M11HotInlineSidecarDisposition::IndentedCodeAuthoritative {
                logical_page_count,
                line_count,
                storage_page_count,
                ordered_commitment256,
            } => HotInlineSidecarDisposition::Authoritative {
                logical_page_count,
                fact_count: line_count,
                storage_page_count,
                link_value_entry_count: 0,
                link_value_encoded_bytes: 0,
                link_value_storage_page_count: 0,
                ordered_commitment256,
            },
            M11HotInlineSidecarDisposition::BlockQuoteAuthoritative {
                logical_page_count,
                line_count,
                storage_page_count,
                ordered_commitment256,
            } => HotInlineSidecarDisposition::Authoritative {
                logical_page_count,
                fact_count: line_count,
                storage_page_count,
                link_value_entry_count: 0,
                link_value_encoded_bytes: 0,
                link_value_storage_page_count: 0,
                ordered_commitment256,
            },
            M11HotInlineSidecarDisposition::BulletListAuthoritative {
                selected_item_ordinal: _,
                selected_item_line_ending: _,
                logical_page_count,
                item_count,
                storage_page_count,
                ordered_commitment256,
            } => HotInlineSidecarDisposition::Authoritative {
                logical_page_count,
                fact_count: item_count,
                storage_page_count,
                link_value_entry_count: 0,
                link_value_encoded_bytes: 0,
                link_value_storage_page_count: 0,
                ordered_commitment256,
            },
            M11HotInlineSidecarDisposition::OrderedListAuthoritative {
                selected_item_ordinal: _,
                selected_item_line_ending: _,
                opening_marker_start: _,
                opening_marker_end: _,
                marker_value: _,
                logical_page_count,
                item_count,
                storage_page_count,
                ordered_commitment256,
            } => HotInlineSidecarDisposition::Authoritative {
                logical_page_count,
                fact_count: item_count,
                storage_page_count,
                link_value_entry_count: 0,
                link_value_encoded_bytes: 0,
                link_value_storage_page_count: 0,
                ordered_commitment256,
            },
            M11HotInlineSidecarDisposition::Unsupported {
                reason,
                metadata_commitment256,
            } => HotInlineSidecarDisposition::Unsupported {
                reason,
                metadata_commitment256,
            },
        };
        let envelope = crate::v3_publication_wire::HotInlineSidecarEnvelopeMetrics {
            hio1_encoded_bytes: descriptor.hio1_encoded_bytes(),
            ipr2_descriptor_bytes: descriptor.ipr2_descriptor_bytes(),
            transferred_node_count: descriptor.transferred_node_count(),
            hio1_envelope_digest256: descriptor.hio1_envelope_digest256(),
            disposition,
        };
        let frame_count = u32::try_from(frames.len()).expect("sidecar frame count");
        let encoded_frame_bytes = frames
            .iter()
            .try_fold(0_u32, |total, frame| {
                total.checked_add(u32::try_from(frame.bytes.len()).ok()?)
            })
            .expect("sidecar encoded bytes");
        let maximum_frame_bytes = frames
            .iter()
            .map(|frame| u32::try_from(frame.bytes.len()).expect("sidecar frame bytes"))
            .max()
            .expect("sidecar contains Begin and End");
        let limits = crate::v3_publication_wire::OfferLimits {
            maximum_frame_count: frame_count,
            maximum_encoded_frame_bytes: encoded_frame_bytes,
            maximum_packet_bytes: u32::try_from(MAXIMUM_PACKET_ENCODED_BYTES)
                .expect("packet ceiling"),
            maximum_frame_bytes,
            maximum_program_children: 128,
        };

        let mut transport = HotInlineSidecarTransportDigest::new();
        let mut first_node_ordinal = 0_u32;
        let mut root_stream_digest = None;
        let mut test_frames = Vec::with_capacity(frames.len());
        for (ordinal, frame) in frames.into_iter().enumerate() {
            let ordinal = u32::try_from(ordinal).expect("sidecar frame ordinal");
            let (kind, node_count) = match frame.kind {
                M11HotInlineSidecarFrameKind::Begin => (HotInlineSidecarFrameKind::Begin, 0),
                M11HotInlineSidecarFrameKind::Node => (HotInlineSidecarFrameKind::Node, 1),
                M11HotInlineSidecarFrameKind::End => (HotInlineSidecarFrameKind::End, 0),
            };
            let digest256 = transport
                .push(ordinal, first_node_ordinal, node_count, kind, &frame.bytes)
                .expect("sidecar transport digest");
            if let Some(digest) = frame.root_stream_digest256 {
                root_stream_digest = Some(digest);
            }
            test_frames.push(TestFrame {
                offer_id,
                ordinal,
                first_record_ordinal: first_node_ordinal,
                record_count: node_count,
                digest: protocol_digest128_from_blake3(
                    ProtocolDigestDomain::HotInlineSidecarFrame,
                    digest256,
                ),
                bytes: frame.bytes,
            });
            first_node_ordinal = first_node_ordinal
                .checked_add(node_count)
                .expect("sidecar node ordinal");
        }
        assert_eq!(
            first_node_ordinal, envelope.transferred_node_count,
            "sidecar descriptor and closure disagree"
        );
        let receipt = transport.finish();
        let commit = HotInlineSidecarCommitRequest {
            offer_id,
            actual_frame_count: receipt.frame_count,
            actual_encoded_frame_bytes: receipt.encoded_frame_bytes,
            rolling_transport_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::HotInlineSidecarTransport,
                receipt.digest256,
            ),
            root_stream_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::HotInlineSidecarRootStream,
                root_stream_digest.expect("sidecar End stream digest"),
            ),
        };
        TestInlineSidecar {
            offer_id,
            publication_session,
            binding,
            envelope,
            limits,
            frames: test_frames,
            commit,
        }
    }

    fn collect_inline_sidecar_frames(
        runtime: &DocumentRuntime,
        mut encoder: M11HotInlineSidecarSnapshotEncoder,
    ) -> (M11HotInlineSidecarDescriptor, Vec<M11HotInlineSidecarFrame>) {
        let descriptor = encoder.descriptor();
        let mut frames = vec![encoder.begin_frame().expect("sidecar Begin")];
        loop {
            match encoder.poll(runtime, 256).expect("sidecar snapshot poll") {
                M11HotInlineSidecarSnapshotPoll::Pending { .. } => {}
                M11HotInlineSidecarSnapshotPoll::Frame { frame, .. } => {
                    let complete = frame.kind == M11HotInlineSidecarFrameKind::End;
                    frames.push(frame);
                    if complete {
                        break;
                    }
                }
            }
        }
        (descriptor, frames)
    }

    fn wire_inline_sidecar_binding(
        binding: &flark_engine::parser_internal::M11HotInlineSidecarBinding,
    ) -> crate::v3_publication_wire::HotInlineSidecarBinding {
        let owner = match binding.owner() {
            flark_engine::parser_internal::M11HotInlineSidecarOwner::BlockOrdinal(ordinal) => {
                crate::v3_publication_wire::HotInlineSidecarOwner::BlockOrdinal(ordinal)
            }
            flark_engine::parser_internal::M11HotInlineSidecarOwner::RecursiveGreenFrame(frame) => {
                crate::v3_publication_wire::HotInlineSidecarOwner::RecursiveGreenFrame(frame.get())
            }
        };
        crate::v3_publication_wire::HotInlineSidecarBinding {
            parser_profile: binding.parser_profile().get(),
            refinement_generation: binding.refinement_generation(),
            block_ordinal: owner.into_wire().expect("test owner fits the wire"),
            physical_start_utf8: binding.physical_range().start,
            physical_end_utf8: binding.physical_range().end,
            visible_start_utf8: binding.visible_range().start,
            visible_end_utf8: binding.visible_range().end,
            physical_start_utf16: binding.physical_range_utf16().start,
            physical_end_utf16: binding.physical_range_utf16().end,
            visible_start_utf16: binding.visible_range_utf16().start,
            visible_end_utf16: binding.visible_range_utf16().end,
        }
    }

    fn snapshot_with_inline_sidecar_pair(
        document_session: Id128,
        structural_publication_session: Id128,
        revision: u32,
        parse_generation: u32,
    ) -> (TestSnapshot, TestInlineSidecar, TestInlineSidecar) {
        snapshot_with_inline_sidecar_pair_kind(
            document_session,
            structural_publication_session,
            revision,
            parse_generation,
            InlineSidecarFixtureKind::Strong,
        )
    }

    fn snapshot_with_direct_link_sidecar_pair(
        document_session: Id128,
        structural_publication_session: Id128,
        revision: u32,
        parse_generation: u32,
    ) -> (TestSnapshot, TestInlineSidecar, TestInlineSidecar) {
        snapshot_with_inline_sidecar_pair_kind(
            document_session,
            structural_publication_session,
            revision,
            parse_generation,
            InlineSidecarFixtureKind::DirectLink,
        )
    }

    #[derive(Clone, Copy)]
    enum InlineSidecarFixtureKind {
        Strong,
        DirectLink,
    }

    fn snapshot_with_inline_sidecar_pair_kind(
        document_session: Id128,
        structural_publication_session: Id128,
        revision: u32,
        parse_generation: u32,
        kind: InlineSidecarFixtureKind,
    ) -> (TestSnapshot, TestInlineSidecar, TestInlineSidecar) {
        const UNSUPPORTED_REASON: u32 = 0x2000_0002;
        let text = match kind {
            InlineSidecarFixtureKind::Strong => "**x**",
            InlineSidecarFixtureKind::DirectLink => "[x](&bsol;*)",
        };
        let source_bytes = u32::try_from(text.len()).expect("sidecar source bytes");
        let source_utf16 = u32::try_from(text.encode_utf16().count()).expect("sidecar UTF-16");
        let mut runtime = DocumentRuntime::from_source_store(
            source_store(text, u64::from(revision)),
            standard_document_runtime_config(),
        )
        .expect("sidecar test runtime");
        let source = runtime
            .current_source_version()
            .expect("sidecar test source");
        let parser_profile = ParserProfileId::new(1).expect("sidecar parser profile");
        let scan_profile = SourceFactsScanProfile::new(32).expect("sidecar scan profile");
        install_persistent_source_facts(&mut runtime, scan_profile, parser_profile);
        let block_lease = runtime
            .snapshot_current_source()
            .expect("sidecar block source lease");
        let mut blocks =
            M11BlockSequenceBuild::new(&runtime, block_lease).expect("sidecar block build");
        blocks
            .offer_entry(block_paragraph(text.len(), text.encode_utf16().count()))
            .expect("sidecar paragraph");
        loop {
            match blocks
                .poll(&mut runtime, 32)
                .expect("sidecar paragraph poll")
                .status()
            {
                M11BlockSequenceBuildStatus::NeedsInput => break,
                M11BlockSequenceBuildStatus::Pending => {}
                M11BlockSequenceBuildStatus::Complete | M11BlockSequenceBuildStatus::Cancelled => {
                    panic!("sidecar block build ended before input")
                }
            }
        }
        blocks.finish_input().expect("finish sidecar blocks");
        loop {
            match blocks
                .poll(&mut runtime, 64)
                .expect("finish sidecar block root")
                .status()
            {
                M11BlockSequenceBuildStatus::Pending => {}
                M11BlockSequenceBuildStatus::Complete => break,
                M11BlockSequenceBuildStatus::NeedsInput
                | M11BlockSequenceBuildStatus::Cancelled => {
                    panic!("finished sidecar block build returned the wrong state")
                }
            }
        }
        let mut block_root = blocks.take_root().expect("sidecar block root");
        drop(blocks);
        let mut candidate = M11CandidateBuild::new_with_persistent_source_facts_and_blocks(
            &mut runtime,
            id128_bytes(document_session),
            id128_bytes(structural_publication_session),
            source,
            u64::from(parse_generation),
            1,
            scan_profile,
            &block_root,
        )
        .expect("sidecar structural candidate");
        block_root
            .begin_release(&mut runtime)
            .expect("release sidecar block root");
        while !block_root
            .poll_release(&mut runtime, 64)
            .expect("poll sidecar block release")
            .complete()
        {}
        drop(block_root);
        candidate
            .finish_references(&runtime)
            .expect("finish sidecar base references");
        while matches!(
            candidate
                .poll(&mut runtime, 256)
                .expect("sidecar base candidate poll"),
            M11CandidateBuildPoll::Pending { .. }
        ) {}
        let publication = candidate
            .into_publication()
            .expect("sidecar base publication");
        let descriptor = publication
            .descriptor(&runtime)
            .expect("sidecar base descriptor");
        let mut stream = Box::new(publication)
            .into_snapshot_stream(&runtime)
            .expect("sidecar base snapshot");
        let mut structural_frames = vec![stream.begin_frame().expect("sidecar base Begin")];
        loop {
            match stream.poll(&runtime, 256).expect("sidecar base traversal") {
                M11OwnedSnapshotPoll::Pending { .. } => {}
                M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                    panic!("full sidecar base requested replay")
                }
                M11OwnedSnapshotPoll::Frame { frame, .. } => {
                    let complete = frame.kind == M11SnapshotFrameKind::End;
                    structural_frames.push(frame);
                    if complete {
                        break;
                    }
                }
            }
        }
        let mut retained = stream
            .into_retained_publication(&runtime)
            .expect("retain delivered sidecar base");

        let source_lease = runtime
            .snapshot_current_source()
            .expect("sidecar inline source lease");
        let mut inline_build =
            M11InlineProjectionBuild::new(&runtime, source_lease, 0..text.len(), parser_profile)
                .expect("sidecar inline build");
        match kind {
            InlineSidecarFixtureKind::Strong => inline_build
                .offer_page(&[M11InlineProjectionFact::new(
                    M11InlineProjectionKind::Strong,
                    0,
                    0..source_bytes,
                    2..3,
                )
                .expect("sidecar Strong fact")])
                .expect("sidecar inline page"),
            InlineSidecarFixtureKind::DirectLink => inline_build
                .offer_page_with_link_values(
                    &[M11InlineProjectionFact::new(
                        M11InlineProjectionKind::DirectLink,
                        0,
                        0..12,
                        1..2,
                    )
                    .expect("sidecar direct-link fact")],
                    &[M11InlineLinkValue::new(0, 4..11, None, "*", None)
                        .expect("sidecar direct-link value")],
                )
                .expect("sidecar direct-link page"),
        }
        loop {
            match inline_build
                .poll(&mut runtime, 32)
                .expect("sidecar inline page poll")
                .status()
            {
                M11InlineProjectionBuildStatus::NeedsPage => break,
                M11InlineProjectionBuildStatus::Pending => {}
                M11InlineProjectionBuildStatus::Complete
                | M11InlineProjectionBuildStatus::Cancelled => {
                    panic!("sidecar inline build ended before input")
                }
            }
        }
        inline_build
            .finish_input()
            .expect("finish sidecar inline input");
        let mut inline_root = loop {
            match inline_build
                .poll(&mut runtime, 32)
                .expect("finish sidecar inline root")
                .status()
            {
                M11InlineProjectionBuildStatus::Pending => {}
                M11InlineProjectionBuildStatus::Complete => {
                    break inline_build.take_root().expect("sidecar inline root");
                }
                M11InlineProjectionBuildStatus::NeedsPage
                | M11InlineProjectionBuildStatus::Cancelled => {
                    panic!("finished sidecar inline build returned the wrong state")
                }
            }
        };

        let authoritative_binding = retained
            .hot_inline_sidecar_binding(
                &runtime,
                parser_profile,
                1,
                0,
                0..source_bytes,
                0..source_bytes,
                0..source_utf16,
                0..source_utf16,
            )
            .expect("authoritative sidecar binding");
        let authoritative_wire_binding = wire_inline_sidecar_binding(&authoritative_binding);
        let authoritative_encoder = M11HotInlineSidecarSnapshotEncoder::authoritative(
            &runtime,
            authoritative_binding,
            &inline_root,
        )
        .expect("authoritative sidecar encoder");
        let (authoritative_descriptor, authoritative_frames) =
            collect_inline_sidecar_frames(&runtime, authoritative_encoder);
        inline_root
            .begin_release(&mut runtime)
            .expect("release sidecar inline root");
        while !inline_root
            .poll_release(&mut runtime, 64)
            .expect("poll sidecar inline release")
            .complete()
        {}
        drop(inline_root);

        let unsupported_binding = retained
            .hot_inline_sidecar_binding(
                &runtime,
                parser_profile,
                2,
                0,
                0..source_bytes,
                0..source_bytes,
                0..source_utf16,
                0..source_utf16,
            )
            .expect("unsupported sidecar binding");
        let unsupported_wire_binding = wire_inline_sidecar_binding(&unsupported_binding);
        let unsupported_encoder = M11HotInlineSidecarSnapshotEncoder::unsupported(
            &runtime,
            unsupported_binding,
            UNSUPPORTED_REASON,
            Box::<[u8]>::from(&b"parser unsupported"[..]),
        )
        .expect("unsupported sidecar encoder");
        let (unsupported_descriptor, unsupported_frames) =
            collect_inline_sidecar_frames(&runtime, unsupported_encoder);

        retained
            .begin_close(&mut runtime)
            .expect("begin retained sidecar base close");
        while !retained
            .poll_close(&mut runtime, 256)
            .expect("poll retained sidecar base close")
        {}
        runtime.begin_close().expect("begin sidecar runtime close");
        while !runtime
            .poll_close(256)
            .expect("poll sidecar runtime close")
            .complete
        {}

        let wire_source = SourceVersion {
            document_session,
            revision,
            utf8_length: source_bytes,
            utf16_length: source_utf16,
            content_hash128: [revision, revision + 1, revision + 2, revision + 3],
        };
        let structural = test_snapshot_from_frames(
            descriptor,
            structural_frames,
            wire_source,
            structural_publication_session,
            parse_generation,
        );
        let authoritative = test_inline_sidecar_from_frames(
            [701, 702, 703, 704],
            [711, 712, 713, 714],
            authoritative_wire_binding,
            authoritative_descriptor,
            authoritative_frames,
        );
        let unsupported = test_inline_sidecar_from_frames(
            [721, 722, 723, 724],
            [731, 732, 733, 734],
            unsupported_wire_binding,
            unsupported_descriptor,
            unsupported_frames,
        );
        (structural, authoritative, unsupported)
    }

    fn snapshot_with_unsupported_viewport_children(
        document_session: Id128,
        structural_publication_session: Id128,
        child_count: u32,
    ) -> (TestSnapshot, Vec<TestInlineSidecar>) {
        const REVISION: u32 = 1;
        const PARSE_GENERATION: u32 = 1;
        const BLOCK_TEXT: &str = "x\n";
        const UNSUPPORTED_REASON: u32 = 0x2000_0002;
        assert!(child_count > 0);
        let text = BLOCK_TEXT.repeat(child_count as usize);
        let mut runtime = DocumentRuntime::from_source_store(
            source_store(&text, u64::from(REVISION)),
            standard_document_runtime_config(),
        )
        .expect("viewport test runtime");
        let source = runtime
            .current_source_version()
            .expect("viewport test source");
        let parser_profile = ParserProfileId::new(1).expect("viewport parser profile");
        let scan_profile = SourceFactsScanProfile::new(32).expect("viewport scan profile");
        install_persistent_source_facts(&mut runtime, scan_profile, parser_profile);
        let block_lease = runtime
            .snapshot_current_source()
            .expect("viewport block source lease");
        let mut blocks =
            M11BlockSequenceBuild::new(&runtime, block_lease).expect("viewport block build");
        for _ in 0..child_count {
            blocks
                .offer_entry(block_paragraph(BLOCK_TEXT.len(), BLOCK_TEXT.len()))
                .expect("viewport paragraph");
            loop {
                match blocks
                    .poll(&mut runtime, 32)
                    .expect("viewport paragraph poll")
                    .status()
                {
                    M11BlockSequenceBuildStatus::NeedsInput => break,
                    M11BlockSequenceBuildStatus::Pending => {}
                    M11BlockSequenceBuildStatus::Complete
                    | M11BlockSequenceBuildStatus::Cancelled => {
                        panic!("viewport block build ended before input")
                    }
                }
            }
        }
        blocks.finish_input().expect("finish viewport blocks");
        loop {
            match blocks
                .poll(&mut runtime, 64)
                .expect("finish viewport block root")
                .status()
            {
                M11BlockSequenceBuildStatus::Pending => {}
                M11BlockSequenceBuildStatus::Complete => break,
                M11BlockSequenceBuildStatus::NeedsInput
                | M11BlockSequenceBuildStatus::Cancelled => {
                    panic!("finished viewport block build returned the wrong state")
                }
            }
        }
        let mut block_root = blocks.take_root().expect("viewport block root");
        drop(blocks);
        let mut candidate = M11CandidateBuild::new_with_persistent_source_facts_and_blocks(
            &mut runtime,
            id128_bytes(document_session),
            id128_bytes(structural_publication_session),
            source,
            u64::from(PARSE_GENERATION),
            1,
            scan_profile,
            &block_root,
        )
        .expect("viewport structural candidate");
        block_root
            .begin_release(&mut runtime)
            .expect("release viewport block root");
        while !block_root
            .poll_release(&mut runtime, 64)
            .expect("poll viewport block release")
            .complete()
        {}
        drop(block_root);
        candidate
            .finish_references(&runtime)
            .expect("finish viewport references");
        while matches!(
            candidate
                .poll(&mut runtime, 256)
                .expect("viewport candidate poll"),
            M11CandidateBuildPoll::Pending { .. }
        ) {}
        let publication = candidate
            .into_publication()
            .expect("viewport structural publication");
        let descriptor = publication
            .descriptor(&runtime)
            .expect("viewport structural descriptor");
        let mut stream = Box::new(publication)
            .into_snapshot_stream(&runtime)
            .expect("viewport structural snapshot");
        let mut structural_frames = vec![stream.begin_frame().expect("viewport structural Begin")];
        loop {
            match stream.poll(&runtime, 256).expect("viewport traversal") {
                M11OwnedSnapshotPoll::Pending { .. } => {}
                M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                    panic!("full viewport snapshot requested replay")
                }
                M11OwnedSnapshotPoll::Frame { frame, .. } => {
                    let complete = frame.kind == M11SnapshotFrameKind::End;
                    structural_frames.push(frame);
                    if complete {
                        break;
                    }
                }
            }
        }
        let mut retained = stream
            .into_retained_publication(&runtime)
            .expect("retain delivered viewport base");
        let mut children = Vec::with_capacity(child_count as usize);
        for child_index in 0..child_count {
            let start = child_index
                .checked_mul(BLOCK_TEXT.len() as u32)
                .expect("viewport child start");
            let end = start
                .checked_add(BLOCK_TEXT.len() as u32)
                .expect("viewport child end");
            let binding = retained
                .hot_inline_sidecar_binding(
                    &runtime,
                    parser_profile,
                    1,
                    u64::from(child_index),
                    start..end,
                    start..end,
                    start..end,
                    start..end,
                )
                .expect("viewport unsupported binding");
            let wire_binding = wire_inline_sidecar_binding(&binding);
            let encoder = M11HotInlineSidecarSnapshotEncoder::unsupported(
                &runtime,
                binding,
                UNSUPPORTED_REASON,
                Box::<[u8]>::from(&b"parser unsupported"[..]),
            )
            .expect("viewport unsupported encoder");
            let (sidecar_descriptor, sidecar_frames) =
                collect_inline_sidecar_frames(&runtime, encoder);
            children.push(test_inline_sidecar_from_frames(
                [900 + child_index, 901, 902, 903],
                [1000 + child_index, 1001, 1002, 1003],
                wire_binding,
                sidecar_descriptor,
                sidecar_frames,
            ));
        }
        retained
            .begin_close(&mut runtime)
            .expect("begin retained viewport base close");
        while !retained
            .poll_close(&mut runtime, 256)
            .expect("poll retained viewport base close")
        {}
        runtime.begin_close().expect("begin viewport runtime close");
        while !runtime
            .poll_close(256)
            .expect("poll viewport runtime close")
            .complete
        {}

        let wire_source = SourceVersion {
            document_session,
            revision: REVISION,
            utf8_length: text.len() as u32,
            utf16_length: text.encode_utf16().count() as u32,
            content_hash128: [REVISION, REVISION + 1, REVISION + 2, REVISION + 3],
        };
        let structural = test_snapshot_from_frames(
            descriptor,
            structural_frames,
            wire_source,
            structural_publication_session,
            PARSE_GENERATION,
        );
        (structural, children)
    }

    fn viewport_presentation_from_children(
        base_ack: StructuralAck,
        children: &[TestInlineSidecar],
    ) -> TestViewportPresentation {
        let child_count = u32::try_from(children.len()).expect("viewport child count");
        let inline_source_bytes = children
            .iter()
            .map(|child| {
                child
                    .binding
                    .visible_end_utf8
                    .checked_sub(child.binding.visible_start_utf8)
                    .expect("viewport child range")
            })
            .sum::<u32>();
        let fact_count = children
            .iter()
            .map(|child| match child.envelope.disposition {
                HotInlineSidecarDisposition::Authoritative { fact_count, .. } => {
                    u32::try_from(fact_count).expect("viewport fact count")
                }
                HotInlineSidecarDisposition::Unsupported { .. } => 0,
            })
            .sum::<u32>();
        let transferred_node_count = children
            .iter()
            .map(|child| child.envelope.transferred_node_count)
            .sum::<u32>();
        let binding = ViewportPresentationBinding {
            viewport_generation: 1,
            requested_range: ViewportPresentationMetricRange {
                start_utf8: 0,
                start_utf16: 0,
                end_utf8: base_ack.source_version.utf8_length,
                end_utf16: base_ack.source_version.utf16_length,
            },
            covered_range: ViewportPresentationMetricRange {
                start_utf8: 0,
                start_utf16: 0,
                end_utf8: base_ack.source_version.utf8_length,
                end_utf16: base_ack.source_version.utf16_length,
            },
            start: ViewportPresentationVisitStart {
                block_ordinal: 0,
                utf8_offset: 0,
                utf16_offset: 0,
            },
            next: ViewportPresentationVisitStart {
                block_ordinal: u64::from(child_count),
                utf8_offset: base_ack.source_version.utf8_length,
                utf16_offset: base_ack.source_version.utf16_length,
            },
            complete: true,
        };
        let mut envelope = ViewportPresentationEnvelopeMetrics {
            visited_structural_entries: child_count,
            visited_storage_pages: 1,
            ordered_leaf_count: child_count,
            inline_source_bytes,
            fact_count,
            transferred_node_count,
            parser_transitions: child_count,
            aggregate_envelope_digest256: [0; 32],
        };
        let expected_frame_count = child_count
            .checked_mul(2)
            .and_then(|count| count.checked_add(transferred_node_count))
            .and_then(|count| count.checked_add(3))
            .expect("viewport frame count");
        let limits = ViewportPresentationOfferLimits {
            maximum_frame_count: expected_frame_count,
            maximum_encoded_frame_bytes:
                crate::v3_session_wire::MAXIMUM_VIEWPORT_ENCODED_FRAME_BYTES,
            maximum_packet_bytes: MAXIMUM_PACKET_ENCODED_BYTES as u32,
            maximum_frame_bytes: MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES,
            maximum_program_children: 128,
        };
        let query_limits = ViewportPresentationQueryLimits {
            maximum_structural_entries: child_count,
            maximum_storage_pages: 1,
            maximum_inline_leaves: child_count,
            maximum_inline_leaf_source_bytes: children
                .iter()
                .map(|child| child.binding.visible_end_utf8 - child.binding.visible_start_utf8)
                .max()
                .expect("viewport child source bytes"),
            maximum_inline_source_bytes: inline_source_bytes,
            maximum_fact_records: fact_count.max(1),
            maximum_encoded_frame_bytes:
                crate::v3_session_wire::MAXIMUM_VIEWPORT_ENCODED_FRAME_BYTES,
            maximum_parser_transitions: child_count,
        };
        let mut begin = ViewportPresentationBegin {
            schema: 1,
            mode: ViewportPresentationMode::AggregatePage,
            offer_id: [1101, 1102, 1103, 1104],
            publication_session: [1111, 1112, 1113, 1114],
            base_ack,
            binding,
            envelope,
            query_limits,
            limits,
        };
        let entries = children
            .iter()
            .enumerate()
            .map(|(index, child)| ViewportPresentationDirectoryEntry {
                ordered_child_index: index as u32,
                global_row_ordinal: child.binding.block_ordinal,
                binding: child.binding,
                hio1_envelope: child.envelope,
            })
            .collect::<Vec<_>>();
        let directory_capacity = VIEWPORT_PRESENTATION_DIRECTORY_HEADER_BYTES
            + entries.len() * VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES;
        let mut directory = vec![0_u8; directory_capacity];
        let directory_bytes =
            encode_viewport_presentation_directory_into(begin, &entries, &mut directory)
                .expect("encode viewport directory");
        directory.truncate(directory_bytes);
        envelope.aggregate_envelope_digest256 =
            viewport_presentation_aggregate_envelope_digest256(binding, envelope, &directory)
                .expect("viewport aggregate envelope digest");
        begin.envelope = envelope;
        let directory_bytes =
            encode_viewport_presentation_directory_into(begin, &entries, &mut directory)
                .expect("re-encode authenticated viewport directory");
        directory.truncate(directory_bytes);

        let mut parent = vec![0_u8; VIEWPORT_PRESENTATION_PARENT_FRAME_BYTES];
        encode_viewport_presentation_parent_frame_into(begin, &mut parent)
            .expect("encode viewport parent");
        let mut root_frames = vec![
            (ViewportPresentationFrameKind::Begin, 0_u32, parent),
            (
                ViewportPresentationFrameKind::Directory,
                child_count,
                directory,
            ),
        ];
        for (directory_index, child) in children.iter().enumerate() {
            for frame in &child.frames {
                let kind = if frame.ordinal == 0 {
                    HotInlineSidecarFrameKind::Begin
                } else if frame.ordinal + 1 == child.frames.len() as u32 {
                    HotInlineSidecarFrameKind::End
                } else {
                    HotInlineSidecarFrameKind::Node
                };
                let mut wrapper =
                    vec![0_u8; VIEWPORT_PRESENTATION_CHILD_HEADER_BYTES + frame.bytes.len()];
                let wrapper_bytes = encode_viewport_presentation_child_frame_into(
                    begin,
                    ViewportPresentationChildFrameInput {
                        directory_index: directory_index as u32,
                        child_frame_ordinal: frame.ordinal,
                        kind,
                        record_count: frame.record_count,
                        payload: &frame.bytes,
                    },
                    &mut wrapper,
                )
                .expect("encode viewport child wrapper");
                wrapper.truncate(wrapper_bytes);
                root_frames.push((
                    ViewportPresentationFrameKind::Child,
                    frame.record_count,
                    wrapper,
                ));
            }
        }
        let actual_frame_count = u32::try_from(root_frames.len() + 1).expect("root frame count");
        assert_eq!(actual_frame_count, expected_frame_count);
        let actual_encoded_frame_bytes = root_frames
            .iter()
            .map(|(_, _, bytes)| bytes.len() as u32)
            .sum::<u32>()
            .checked_add(VIEWPORT_PRESENTATION_END_FRAME_BYTES as u32)
            .expect("viewport root bytes");
        let mut end = vec![0_u8; VIEWPORT_PRESENTATION_END_FRAME_BYTES];
        encode_viewport_presentation_end_frame_into(
            begin,
            ViewportPresentationEndFrame {
                ordered_leaf_count: child_count,
                actual_frame_count,
                actual_encoded_frame_bytes,
                aggregate_envelope_digest256: envelope.aggregate_envelope_digest256,
            },
            &mut end,
        )
        .expect("encode viewport End");
        root_frames.push((ViewportPresentationFrameKind::End, 0, end));

        let mut transport = ViewportPresentationTransportDigest::new();
        let mut first_record_ordinal = 0_u32;
        let mut frames = Vec::with_capacity(root_frames.len());
        for (ordinal, (kind, record_count, bytes)) in root_frames.into_iter().enumerate() {
            let ordinal = ordinal as u32;
            let digest256 = transport
                .push(ordinal, first_record_ordinal, record_count, kind, &bytes)
                .expect("viewport transport digest");
            frames.push(TestFrame {
                offer_id: begin.offer_id,
                ordinal,
                first_record_ordinal,
                record_count,
                digest: protocol_digest128_from_blake3(
                    ProtocolDigestDomain::ViewportPresentationFrame,
                    digest256,
                ),
                bytes: bytes.into_boxed_slice(),
            });
            first_record_ordinal = first_record_ordinal
                .checked_add(record_count)
                .expect("viewport record ordinal");
        }
        let receipt = transport.finish().expect("complete viewport transport");
        let root_digest = viewport_presentation_root_stream_digest256(
            envelope.aggregate_envelope_digest256,
            receipt,
        );
        let commit = ViewportPresentationCommitRequest {
            offer_id: begin.offer_id,
            actual_frame_count: receipt.frame_count,
            actual_encoded_frame_bytes: receipt.encoded_frame_bytes,
            rolling_transport_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::ViewportPresentationTransport,
                receipt.digest256,
            ),
            aggregate_root_stream_digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::ViewportPresentationRootStream,
                root_digest,
            ),
        };
        TestViewportPresentation {
            begin,
            frames,
            commit,
        }
    }

    fn snapshot_with_compact_list_item_sidecar(
        document_session: Id128,
        structural_publication_session: Id128,
        revision: u32,
        parse_generation: u32,
        item_count: u32,
        selected_item_ordinal: u32,
        ordered: bool,
    ) -> (TestSnapshot, TestInlineSidecar, u32, u32) {
        const CONTENT_BYTES: u32 = 8;
        const EOL_BYTES: u32 = 2;

        assert!(item_count > 1);
        assert!(selected_item_ordinal < item_count);
        let (
            item,
            hidden_prefix_bytes,
            continuation_start,
            continuation_end,
            opening_marker_start,
            opening_marker_end,
            marker_value,
        ) = if ordered {
            ("  42) item0000\r\n", 6, 2, 6, 2, 5, 42)
        } else {
            ("- item0000\r\n", 2, 0, 2, 0, 1, 0)
        };
        let item_bytes = u32::try_from(item.len()).expect("fixed item bytes");
        let source_bytes = item_bytes.checked_mul(item_count).expect("list bytes");
        let projected_bytes = (CONTENT_BYTES + EOL_BYTES)
            .checked_mul(item_count)
            .expect("list projected bytes");
        let selected_start = item_bytes
            .checked_mul(selected_item_ordinal)
            .expect("selected item start");
        let selected_end = selected_start
            .checked_add(item_bytes)
            .expect("selected item end");
        let text = item.repeat(usize::try_from(item_count).expect("item count"));
        assert_eq!(
            u32::try_from(text.len()).expect("source bytes"),
            source_bytes
        );

        let mut runtime = DocumentRuntime::from_source_store(
            source_store(&text, u64::from(revision)),
            standard_document_runtime_config(),
        )
        .expect("compact bullet-item runtime");
        let source = runtime.current_source_version().expect("current source");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(32).expect("scan profile");
        install_persistent_source_facts(&mut runtime, scan_profile, parser_profile);

        let block_lease = runtime
            .snapshot_current_source()
            .expect("bullet-list block source lease");
        let mut blocks =
            M11BlockSequenceBuild::new(&runtime, block_lease).expect("bullet-list block build");
        blocks
            .offer_entry(if ordered {
                block_ordered_list(
                    source_bytes,
                    42,
                    b')',
                    item_count,
                    projected_bytes,
                    projected_bytes,
                )
            } else {
                block_bullet_list(source_bytes, item_count, projected_bytes, projected_bytes)
            })
            .expect("bullet-list block");
        loop {
            match blocks
                .poll(&mut runtime, 32)
                .expect("bullet-list block poll")
                .status()
            {
                M11BlockSequenceBuildStatus::NeedsInput => break,
                M11BlockSequenceBuildStatus::Pending => {}
                M11BlockSequenceBuildStatus::Complete | M11BlockSequenceBuildStatus::Cancelled => {
                    panic!("bullet-list block build ended before input")
                }
            }
        }
        blocks.finish_input().expect("finish bullet-list blocks");
        loop {
            match blocks
                .poll(&mut runtime, 64)
                .expect("finish bullet-list block root")
                .status()
            {
                M11BlockSequenceBuildStatus::Pending => {}
                M11BlockSequenceBuildStatus::Complete => break,
                M11BlockSequenceBuildStatus::NeedsInput
                | M11BlockSequenceBuildStatus::Cancelled => {
                    panic!("finished bullet-list block build returned the wrong state")
                }
            }
        }
        let mut block_root = blocks.take_root().expect("bullet-list block root");
        drop(blocks);
        let mut candidate = M11CandidateBuild::new_with_persistent_source_facts_and_blocks(
            &mut runtime,
            id128_bytes(document_session),
            id128_bytes(structural_publication_session),
            source,
            u64::from(parse_generation),
            1,
            scan_profile,
            &block_root,
        )
        .expect("bullet-list structural candidate");
        block_root
            .begin_release(&mut runtime)
            .expect("release bullet-list block root");
        while !block_root
            .poll_release(&mut runtime, 64)
            .expect("poll bullet-list block release")
            .complete()
        {}
        drop(block_root);
        candidate
            .finish_references(&runtime)
            .expect("finish bullet-list references");
        while matches!(
            candidate
                .poll(&mut runtime, 256)
                .expect("bullet-list candidate poll"),
            M11CandidateBuildPoll::Pending { .. }
        ) {}
        let publication = candidate
            .into_publication()
            .expect("bullet-list publication");
        let structural_descriptor = publication
            .descriptor(&runtime)
            .expect("bullet-list descriptor");
        let mut stream = Box::new(publication)
            .into_snapshot_stream(&runtime)
            .expect("bullet-list snapshot stream");
        let mut structural_frames = vec![stream.begin_frame().expect("bullet-list Begin")];
        loop {
            match stream.poll(&runtime, 256).expect("bullet-list traversal") {
                M11OwnedSnapshotPoll::Pending { .. } => {}
                M11OwnedSnapshotPoll::ReplayRequired { .. } => {
                    panic!("full bullet-list snapshot requested replay")
                }
                M11OwnedSnapshotPoll::Frame { frame, .. } => {
                    let complete = frame.kind == M11SnapshotFrameKind::End;
                    structural_frames.push(frame);
                    if complete {
                        break;
                    }
                }
            }
        }
        let mut retained = stream
            .into_retained_publication(&runtime)
            .expect("retain delivered bullet-list base");

        let projection_lease = runtime
            .snapshot_current_source()
            .expect("compact bullet-item source lease");
        let physical_block_range = 0..usize::try_from(source_bytes).expect("source bytes");
        let requested_window = usize::try_from(selected_start).expect("selected start")
            ..usize::try_from(selected_end).expect("selected end");
        let mut projection = if ordered {
            M11BlockQuoteProjectionBuild::new_ordered_list(
                &runtime,
                projection_lease,
                physical_block_range,
                requested_window,
                CONTENT_BYTES + EOL_BYTES,
                CONTENT_BYTES + EOL_BYTES,
                parser_profile,
            )
        } else {
            M11BlockQuoteProjectionBuild::new_bullet_list(
                &runtime,
                projection_lease,
                physical_block_range,
                requested_window,
                CONTENT_BYTES + EOL_BYTES,
                CONTENT_BYTES + EOL_BYTES,
                parser_profile,
            )
        }
        .expect("compact list-item projection");
        let selected_line = if ordered {
            BlockQuoteLineV1::ordered_item(
                selected_start,
                item_bytes,
                hidden_prefix_bytes,
                continuation_start,
                continuation_end,
                CONTENT_BYTES,
                CONTENT_BYTES,
            )
            .expect("selected ordered-item geometry")
        } else {
            BlockQuoteLineV1::bullet_item(
                selected_start,
                item_bytes,
                hidden_prefix_bytes,
                continuation_start,
                continuation_end,
                CONTENT_BYTES,
                CONTENT_BYTES,
            )
            .expect("selected bullet-item geometry")
        };
        projection
            .offer_page(&[selected_line])
            .expect("offer selected bullet item");
        loop {
            match projection
                .poll(&mut runtime, 32)
                .expect("selected bullet-item page poll")
                .status()
            {
                M11BlockQuoteProjectionBuildStatus::NeedsPage => break,
                M11BlockQuoteProjectionBuildStatus::Pending => {}
                M11BlockQuoteProjectionBuildStatus::Complete
                | M11BlockQuoteProjectionBuildStatus::Cancelled => {
                    panic!("compact bullet-item build ended before input")
                }
            }
        }
        projection
            .finish_input()
            .expect("finish selected bullet-item input");
        let mut projection_root = loop {
            match projection
                .poll(&mut runtime, 32)
                .expect("finish selected bullet-item root")
                .status()
            {
                M11BlockQuoteProjectionBuildStatus::Pending => {}
                M11BlockQuoteProjectionBuildStatus::Complete => {
                    break projection.take_root().expect("selected bullet-item root");
                }
                M11BlockQuoteProjectionBuildStatus::NeedsPage
                | M11BlockQuoteProjectionBuildStatus::Cancelled => {
                    panic!("finished compact bullet-item build returned the wrong state")
                }
            }
        };
        drop(projection);

        let binding = retained
            .hot_inline_sidecar_binding(
                &runtime,
                parser_profile,
                1,
                0,
                0..source_bytes,
                selected_start..selected_end,
                0..source_bytes,
                selected_start..selected_end,
            )
            .expect("compact bullet-item binding");
        let wire_binding = wire_inline_sidecar_binding(&binding);
        let encoder = if ordered {
            M11HotInlineSidecarSnapshotEncoder::authoritative_ordered_list_item(
                &runtime,
                binding,
                &projection_root,
                selected_item_ordinal,
                M11HotInlineCanonicalLineEnding::CrLf,
                opening_marker_start,
                opening_marker_end,
                marker_value,
            )
        } else {
            M11HotInlineSidecarSnapshotEncoder::authoritative_bullet_list_item(
                &runtime,
                binding,
                &projection_root,
                selected_item_ordinal,
                M11HotInlineCanonicalLineEnding::CrLf,
            )
        }
        .expect("compact list-item encoder");
        let (sidecar_descriptor, sidecar_frames) = collect_inline_sidecar_frames(&runtime, encoder);
        projection_root
            .begin_release(&mut runtime)
            .expect("release compact bullet-item root");
        while !projection_root
            .poll_release(&mut runtime, 64)
            .expect("poll compact bullet-item release")
            .complete()
        {}
        drop(projection_root);

        retained
            .begin_close(&mut runtime)
            .expect("begin retained bullet-list close");
        while !retained
            .poll_close(&mut runtime, 256)
            .expect("poll retained bullet-list close")
        {}
        runtime
            .begin_close()
            .expect("begin bullet-list runtime close");
        while !runtime
            .poll_close(256)
            .expect("poll bullet-list runtime close")
            .complete
        {}

        let wire_source = SourceVersion {
            document_session,
            revision,
            utf8_length: source_bytes,
            utf16_length: source_bytes,
            content_hash128: [revision, revision + 1, revision + 2, revision + 3],
        };
        let structural = test_snapshot_from_frames(
            structural_descriptor,
            structural_frames,
            wire_source,
            structural_publication_session,
            parse_generation,
        );
        let sidecar = test_inline_sidecar_from_frames(
            [781, 782, 783, 784],
            [791, 792, 793, 794],
            wire_binding,
            sidecar_descriptor,
            sidecar_frames,
        );
        (structural, sidecar, selected_start, selected_end)
    }

    fn snapshot_with_compact_bullet_item_sidecar(
        document_session: Id128,
        structural_publication_session: Id128,
        revision: u32,
        parse_generation: u32,
        item_count: u32,
        selected_item_ordinal: u32,
    ) -> (TestSnapshot, TestInlineSidecar, u32, u32) {
        snapshot_with_compact_list_item_sidecar(
            document_session,
            structural_publication_session,
            revision,
            parse_generation,
            item_count,
            selected_item_ordinal,
            false,
        )
    }

    fn snapshot_with_compact_ordered_item_sidecar(
        document_session: Id128,
        structural_publication_session: Id128,
        revision: u32,
        parse_generation: u32,
        item_count: u32,
        selected_item_ordinal: u32,
    ) -> (TestSnapshot, TestInlineSidecar, u32, u32) {
        snapshot_with_compact_list_item_sidecar(
            document_session,
            structural_publication_session,
            revision,
            parse_generation,
            item_count,
            selected_item_ordinal,
            true,
        )
    }

    fn install_persistent_source_facts(
        runtime: &mut DocumentRuntime,
        scan_profile: SourceFactsScanProfile,
        parser_profile: ParserProfileId,
    ) {
        runtime
            .begin_source_facts(
                scan_profile,
                parser_profile,
                SourceFactsRootLimits::default(),
            )
            .expect("begin persistent SourceFacts");
        loop {
            match runtime
                .poll_source_facts(4096, 64)
                .expect("poll persistent SourceFacts")
            {
                RuntimeSourceFactsPoll::Pending(_)
                | RuntimeSourceFactsPoll::PromotionPending { .. }
                | RuntimeSourceFactsPoll::ScanComplete { .. } => {}
                RuntimeSourceFactsPoll::Complete { .. } => break,
                RuntimeSourceFactsPoll::IncrementalScanComplete { .. }
                | RuntimeSourceFactsPoll::IncrementalComplete { .. } => {
                    panic!("clean SourceFacts build reported incremental progress")
                }
            }
        }
    }

    fn recursive_green_snapshot(
        document_session: Id128,
        publication_session: Id128,
        revision: u32,
        parse_generation: u32,
        text: &str,
    ) -> TestSnapshot {
        let mut runtime = DocumentRuntime::from_source_store(
            source_store(text, u64::from(revision)),
            standard_document_runtime_config(),
        )
        .expect("recursive Green test runtime");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(32).expect("scan profile");
        install_persistent_source_facts(&mut runtime, scan_profile, parser_profile);

        let parse_lease = runtime
            .certified_source()
            .expect("certified recursive Green source")
            .exact_parse_lease();
        let mut parse = M11CleanParseJob::new(parse_lease).expect("clean parse job");
        let result = loop {
            match parse.poll(64).expect("clean parse poll") {
                M11CleanParsePoll::Pending { .. } => {}
                M11CleanParsePoll::Complete { result, .. } => break result,
            }
        };

        let plan = M11PersistentRecursiveGreenCleanPlan::new(
            runtime
                .snapshot_current_source()
                .expect("recursive Green scanner lease"),
            runtime
                .snapshot_current_source()
                .expect("recursive Green writer lease"),
            1,
        )
        .expect("recursive Green plan");
        let mut build = plan.begin(&mut runtime).expect("recursive Green build");
        let mut session = loop {
            let poll = build.poll(&mut runtime, 64).expect("recursive Green poll");
            if poll.status() == M11PersistentRecursiveGreenBuildStatus::Complete {
                break build.take_session().expect("recursive Green session");
            }
        };
        let source = runtime.current_source_version().expect("current source");
        let certified = runtime
            .take_certified_source()
            .expect("recursive Green certification");
        let candidate =
            M11ParserCandidate::derive_with_recursive_green(certified, &result, &session)
                .expect("recursive Green candidate");
        let mut writer = candidate
            .into_writer_with_recursive_green(
                &mut runtime,
                id128_bytes(document_session),
                id128_bytes(publication_session),
                u64::from(parse_generation),
                &session,
            )
            .expect("recursive Green candidate writer");
        session
            .begin_release(&mut runtime)
            .expect("begin recursive Green session release");
        while !session
            .poll_release(&mut runtime, 64)
            .expect("poll recursive Green session release")
        {}
        let publication = loop {
            match writer
                .poll(&mut runtime, 64)
                .expect("recursive Green candidate poll")
            {
                M11ParserCandidateWriterPoll::Pending { .. } => {}
                M11ParserCandidateWriterPoll::Published { publication, .. } => {
                    break *publication;
                }
            }
        };
        let wire_source = SourceVersion {
            document_session,
            revision,
            utf8_length: u32::try_from(text.len()).expect("source bytes"),
            utf16_length: u32::try_from(text.encode_utf16().count()).expect("source UTF-16"),
            content_hash128: [revision, revision + 1, revision + 2, revision + 3],
        };
        assert_eq!(source.byte_len(), text.len());
        snapshot_from_publication(
            runtime,
            publication,
            wire_source,
            publication_session,
            parse_generation,
        )
    }

    fn persistent_inline_snapshot(
        document_session: Id128,
        publication_session: Id128,
        revision: u32,
        parse_generation: u32,
        logical_pages: usize,
    ) -> TestSnapshot {
        persistent_inline_snapshot_with_kind(
            document_session,
            publication_session,
            revision,
            parse_generation,
            logical_pages,
            PersistentInlineFixtureKind::Strong,
        )
    }

    fn persistent_escape_snapshot(
        document_session: Id128,
        publication_session: Id128,
        revision: u32,
        parse_generation: u32,
        logical_pages: usize,
    ) -> TestSnapshot {
        persistent_inline_snapshot_with_kind(
            document_session,
            publication_session,
            revision,
            parse_generation,
            logical_pages,
            PersistentInlineFixtureKind::Escape,
        )
    }

    fn persistent_direct_link_snapshot(
        document_session: Id128,
        publication_session: Id128,
        revision: u32,
        parse_generation: u32,
    ) -> TestSnapshot {
        persistent_inline_snapshot_with_kind(
            document_session,
            publication_session,
            revision,
            parse_generation,
            1,
            PersistentInlineFixtureKind::DirectLink,
        )
    }

    #[derive(Clone, Copy)]
    enum PersistentInlineFixtureKind {
        Strong,
        Escape,
        DirectLink,
    }

    fn persistent_inline_snapshot_with_kind(
        document_session: Id128,
        publication_session: Id128,
        revision: u32,
        parse_generation: u32,
        logical_pages: usize,
        kind: PersistentInlineFixtureKind,
    ) -> TestSnapshot {
        let text = if logical_pages == 0 {
            "plain paragraph".to_owned()
        } else {
            match kind {
                PersistentInlineFixtureKind::Strong => "**x** ".repeat(logical_pages),
                PersistentInlineFixtureKind::Escape => "\\* ".repeat(logical_pages),
                PersistentInlineFixtureKind::DirectLink => "[x](&bsol;*) ".repeat(logical_pages),
            }
        };
        let mut runtime = DocumentRuntime::from_source_store(
            source_store(&text, u64::from(revision)),
            standard_document_runtime_config(),
        )
        .expect("test runtime");
        let source = runtime.current_source_version().expect("current source");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(32).expect("scan profile");
        install_persistent_source_facts(&mut runtime, scan_profile, parser_profile);

        let lease = runtime
            .snapshot_current_source()
            .expect("inline source lease");
        let source_bytes = source.byte_len();
        let mut inline_build =
            M11InlineProjectionBuild::new(&runtime, lease, 0..source_bytes, parser_profile)
                .expect("inline Projection build");
        for ordinal in 0..logical_pages {
            let stride = match kind {
                PersistentInlineFixtureKind::Strong => 6,
                PersistentInlineFixtureKind::Escape => 3,
                PersistentInlineFixtureKind::DirectLink => 13,
            };
            let start = u32::try_from(ordinal * stride).expect("inline coordinate");
            let (fact_kind, range, content) = match kind {
                PersistentInlineFixtureKind::Strong => (
                    M11InlineProjectionKind::Strong,
                    start..start + 5,
                    start + 2..start + 3,
                ),
                PersistentInlineFixtureKind::Escape => (
                    M11InlineProjectionKind::BackslashEscape,
                    start..start + 2,
                    start + 1..start + 2,
                ),
                PersistentInlineFixtureKind::DirectLink => (
                    M11InlineProjectionKind::DirectLink,
                    start..start + 12,
                    start + 1..start + 2,
                ),
            };
            let fact =
                M11InlineProjectionFact::new(fact_kind, 0, range, content).expect("inline fact");
            if matches!(kind, PersistentInlineFixtureKind::DirectLink) {
                let value = M11InlineLinkValue::new(
                    u32::try_from(ordinal).expect("fact ordinal"),
                    start + 4..start + 11,
                    None,
                    "*",
                    None,
                )
                .expect("direct-link value");
                inline_build
                    .offer_page_with_link_values(&[fact], &[value])
                    .expect("direct-link page");
            } else {
                inline_build.offer_page(&[fact]).expect("inline page");
            }
            loop {
                match inline_build
                    .poll(&mut runtime, 32)
                    .expect("poll inline page")
                    .status()
                {
                    M11InlineProjectionBuildStatus::NeedsPage => break,
                    M11InlineProjectionBuildStatus::Pending => {}
                    M11InlineProjectionBuildStatus::Complete
                    | M11InlineProjectionBuildStatus::Cancelled => {
                        panic!("inline build ended before input")
                    }
                }
            }
        }
        inline_build.finish_input().expect("finish inline input");
        let mut inline_root = loop {
            match inline_build
                .poll(&mut runtime, 32)
                .expect("finish inline root")
                .status()
            {
                M11InlineProjectionBuildStatus::Pending => {}
                M11InlineProjectionBuildStatus::Complete => {
                    break inline_build.take_root().expect("inline root");
                }
                M11InlineProjectionBuildStatus::NeedsPage
                | M11InlineProjectionBuildStatus::Cancelled => {
                    panic!("finished inline build returned the wrong state")
                }
            }
        };
        let mut candidate =
            M11CandidateBuild::new_with_persistent_source_facts_and_inline_projection(
                &mut runtime,
                id128_bytes(document_session),
                id128_bytes(publication_session),
                source,
                u64::from(parse_generation),
                1,
                scan_profile,
                M11RoleRecords::persistent(
                    test_green_record(source_bytes as u64),
                    test_projection_record(source_bytes as u64),
                )
                .expect("persistent inline role records"),
                &inline_root,
            )
            .expect("persistent inline candidate");
        inline_root
            .begin_release(&mut runtime)
            .expect("release original inline root");
        while !inline_root
            .poll_release(&mut runtime, 64)
            .expect("poll inline release")
            .complete()
        {}
        drop(inline_root);
        candidate
            .finish_references(&runtime)
            .expect("finish references");
        while let M11CandidateBuildPoll::Pending { .. } = candidate
            .poll(&mut runtime, 256)
            .expect("persistent candidate poll")
        {}
        let publication = candidate.into_publication().expect("publication");
        let wire_source = SourceVersion {
            document_session,
            revision,
            utf8_length: u32::try_from(text.len()).expect("source bytes"),
            utf16_length: u32::try_from(text.encode_utf16().count()).expect("source UTF-16"),
            content_hash128: [revision, revision + 1, revision + 2, revision + 3],
        };
        snapshot_from_publication(
            runtime,
            publication,
            wire_source,
            publication_session,
            parse_generation,
        )
    }

    fn block_paragraph(source_bytes: usize, source_utf16: usize) -> M11BlockSequenceEntry {
        M11BlockSequenceEntry::paragraph(
            source_bytes,
            source_utf16,
            0,
            M11BlockRoleRecord::new(&test_green_record(source_bytes as u64)).expect("block Green"),
            M11BlockRoleRecord::new(&test_projection_record(source_bytes as u64))
                .expect("block Projection"),
        )
        .expect("block paragraph")
    }

    fn fenced_code_role_records(
        source_bytes: u32,
        body: std::ops::Range<u32>,
        opening_marker: std::ops::Range<u32>,
        raw_info: std::ops::Range<u32>,
        closing_marker: Option<std::ops::Range<u32>>,
        marker: u8,
        opening_indent: u8,
    ) -> (M11BlockRoleRecord, M11BlockRoleRecord) {
        let mut green = [0_u8; M11_GREEN_RECORD_BYTES];
        green[..8].copy_from_slice(GREEN_MAGIC);
        green[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        green[12] = M11_FENCED_CODE_VARIANT;
        green[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        green[32..40].copy_from_slice(&u64::from(body.start).to_le_bytes());
        green[40..48].copy_from_slice(&u64::from(body.end).to_le_bytes());
        let metadata = u64::from(marker)
            | (u64::from(opening_indent) << 8)
            | closing_marker.as_ref().map_or(0, |_| M11_FENCE_CLOSED_FLAG);
        green[48..56].copy_from_slice(&metadata.to_le_bytes());
        green[56..60].copy_from_slice(&opening_marker.start.to_le_bytes());
        green[60..64].copy_from_slice(&opening_marker.end.to_le_bytes());
        green[64..68].copy_from_slice(&raw_info.start.to_le_bytes());
        green[68..72].copy_from_slice(&raw_info.end.to_le_bytes());
        let closing_marker = closing_marker.unwrap_or(M11_FENCE_ABSENT_CUT..M11_FENCE_ABSENT_CUT);
        green[72..76].copy_from_slice(&closing_marker.start.to_le_bytes());
        green[76..80].copy_from_slice(&closing_marker.end.to_le_bytes());

        let mut projection = [0_u8; M11_PROJECTION_RECORD_BYTES];
        projection[..8].copy_from_slice(PROJECTION_MAGIC);
        projection[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        projection[12] = M11_FENCED_CODE_VARIANT;
        projection[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        projection[32..40].copy_from_slice(&u64::from(body.start).to_le_bytes());
        projection[40..48].copy_from_slice(&u64::from(body.end).to_le_bytes());
        projection[48..56].copy_from_slice(&1_u64.to_le_bytes());
        (
            M11BlockRoleRecord::new(&green).expect("fenced Green"),
            M11BlockRoleRecord::new(&projection).expect("fenced Projection"),
        )
    }

    fn block_fenced_code() -> M11BlockSequenceEntry {
        let (green, projection) =
            fenced_code_role_records(14, 8..10, 0..3, 3..7, Some(10..13), b'`', 0);
        M11BlockSequenceEntry::structured(14, 14, 0, green, projection)
            .expect("structured fenced code")
    }

    fn atx_heading_role_records(
        source_bytes: u32,
        content: std::ops::Range<u32>,
        opening_marker: std::ops::Range<u32>,
        closing_marker: Option<std::ops::Range<u32>>,
        line_ending: std::ops::Range<u32>,
        level: u8,
        opening_indent: u8,
        has_bof_bom: bool,
    ) -> (M11BlockRoleRecord, M11BlockRoleRecord) {
        let mut green = [0_u8; M11_GREEN_RECORD_BYTES];
        green[..8].copy_from_slice(GREEN_MAGIC);
        green[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        green[12] = M11_ATX_HEADING_VARIANT;
        green[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        green[32..40].copy_from_slice(&u64::from(content.start).to_le_bytes());
        green[40..48].copy_from_slice(&u64::from(content.end).to_le_bytes());
        let metadata = u64::from(level)
            | closing_marker
                .as_ref()
                .map_or(0, |_| M11_ATX_HEADING_CLOSED_FLAG)
            | (u64::from(opening_indent) << M11_ATX_HEADING_OPENING_INDENT_SHIFT)
            | if has_bof_bom {
                M11_ATX_HEADING_BOF_BOM_FLAG
            } else {
                0
            };
        green[48..56].copy_from_slice(&metadata.to_le_bytes());
        green[56..60].copy_from_slice(&opening_marker.start.to_le_bytes());
        green[60..64].copy_from_slice(&opening_marker.end.to_le_bytes());
        let closing_marker =
            closing_marker.unwrap_or(M11_ATX_HEADING_ABSENT_CUT..M11_ATX_HEADING_ABSENT_CUT);
        green[64..68].copy_from_slice(&closing_marker.start.to_le_bytes());
        green[68..72].copy_from_slice(&closing_marker.end.to_le_bytes());
        green[72..76].copy_from_slice(&line_ending.start.to_le_bytes());
        green[76..80].copy_from_slice(&line_ending.end.to_le_bytes());

        let mut projection = [0_u8; M11_PROJECTION_RECORD_BYTES];
        projection[..8].copy_from_slice(PROJECTION_MAGIC);
        projection[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        projection[12] = M11_ATX_HEADING_VARIANT;
        projection[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        projection[32..40].copy_from_slice(&u64::from(content.start).to_le_bytes());
        projection[40..48].copy_from_slice(&u64::from(content.end).to_le_bytes());
        projection[48..56].copy_from_slice(&1_u64.to_le_bytes());
        (
            M11BlockRoleRecord::new(&green).expect("ATX Heading Green"),
            M11BlockRoleRecord::new(&projection).expect("ATX Heading Projection"),
        )
    }

    fn block_atx_heading() -> M11BlockSequenceEntry {
        let (green, projection) =
            atx_heading_role_records(24, 6..16, 2..5, Some(17..20), 22..24, 3, 2, false);
        M11BlockSequenceEntry::structured(24, 21, 0, green, projection)
            .expect("structured ATX Heading")
    }

    fn setext_heading_role_records(
        source_bytes: u32,
        inline_source: std::ops::Range<u32>,
        underline_marker: std::ops::Range<u32>,
        line_ending: std::ops::Range<u32>,
        level: u8,
        opening_indent: u8,
        reference_definition_count: u64,
    ) -> (M11BlockRoleRecord, M11BlockRoleRecord) {
        let mut green = [0_u8; M11_GREEN_RECORD_BYTES];
        green[..8].copy_from_slice(GREEN_MAGIC);
        green[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        green[12] = M11_SETEXT_HEADING_VARIANT;
        green[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        green[32..40].copy_from_slice(&u64::from(inline_source.start).to_le_bytes());
        green[40..48].copy_from_slice(&u64::from(inline_source.end).to_le_bytes());
        let metadata = u64::from(level)
            | (u64::from(opening_indent) << M11_SETEXT_HEADING_OPENING_INDENT_SHIFT);
        green[48..56].copy_from_slice(&metadata.to_le_bytes());
        green[56..60].copy_from_slice(&underline_marker.start.to_le_bytes());
        green[60..64].copy_from_slice(&underline_marker.end.to_le_bytes());
        green[64..68].copy_from_slice(&line_ending.start.to_le_bytes());
        green[68..72].copy_from_slice(&line_ending.end.to_le_bytes());
        green[72..80].copy_from_slice(&reference_definition_count.to_le_bytes());

        let mut projection = [0_u8; M11_PROJECTION_RECORD_BYTES];
        projection[..8].copy_from_slice(PROJECTION_MAGIC);
        projection[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        projection[12] = M11_SETEXT_HEADING_VARIANT;
        projection[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        projection[32..40].copy_from_slice(&u64::from(inline_source.start).to_le_bytes());
        projection[40..48].copy_from_slice(&u64::from(inline_source.end).to_le_bytes());
        projection[48..56].copy_from_slice(&1_u64.to_le_bytes());
        (
            M11BlockRoleRecord::new(&green).expect("Setext Heading Green"),
            M11BlockRoleRecord::new(&projection).expect("Setext Heading Projection"),
        )
    }

    fn block_setext_heading() -> M11BlockSequenceEntry {
        let (green, projection) = setext_heading_role_records(20, 0..9, 13..16, 18..20, 1, 2, 0);
        M11BlockSequenceEntry::structured(20, 20, 0, green, projection)
            .expect("structured Setext Heading")
    }

    fn thematic_break_role_records(
        source_bytes: u32,
        marker_envelope: std::ops::Range<u32>,
        line_ending: std::ops::Range<u32>,
        marker: u8,
        marker_count: u64,
        opening_indent: u8,
        has_bof_bom: bool,
    ) -> (M11BlockRoleRecord, M11BlockRoleRecord) {
        let mut green = [0_u8; M11_GREEN_RECORD_BYTES];
        green[..8].copy_from_slice(GREEN_MAGIC);
        green[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        green[12] = M11_THEMATIC_BREAK_VARIANT;
        green[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        let metadata = u64::from(marker)
            | (u64::from(opening_indent) << M11_THEMATIC_BREAK_OPENING_INDENT_SHIFT)
            | if has_bof_bom {
                M11_THEMATIC_BREAK_BOF_BOM_FLAG
            } else {
                0
            };
        green[48..56].copy_from_slice(&metadata.to_le_bytes());
        green[56..60].copy_from_slice(&marker_envelope.start.to_le_bytes());
        green[60..64].copy_from_slice(&marker_envelope.end.to_le_bytes());
        green[64..68].copy_from_slice(&line_ending.start.to_le_bytes());
        green[68..72].copy_from_slice(&line_ending.end.to_le_bytes());
        green[72..80].copy_from_slice(&marker_count.to_le_bytes());

        let mut projection = [0_u8; M11_PROJECTION_RECORD_BYTES];
        projection[..8].copy_from_slice(PROJECTION_MAGIC);
        projection[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        projection[12] = M11_THEMATIC_BREAK_VARIANT;
        projection[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        (
            M11BlockRoleRecord::new(&green).expect("thematic-break Green"),
            M11BlockRoleRecord::new(&projection).expect("thematic-break Projection"),
        )
    }

    fn block_thematic_break() -> M11BlockSequenceEntry {
        let (green, projection) = thematic_break_role_records(10, 2..7, 8..10, b'*', 3, 2, false);
        M11BlockSequenceEntry::structured(10, 10, 0, green, projection)
            .expect("structured thematic break")
    }

    fn indented_code_role_records(
        source_bytes: u32,
        line_count: u32,
        projected_utf8: u32,
        projected_utf16: u32,
        terminal_line_ending_bytes: u32,
        has_bof_bom: bool,
    ) -> (M11BlockRoleRecord, M11BlockRoleRecord) {
        let mut green = [0_u8; M11_GREEN_RECORD_BYTES];
        green[..8].copy_from_slice(GREEN_MAGIC);
        green[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        green[12] = M11_INDENTED_CODE_VARIANT;
        green[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        let metadata = M11_INDENTED_CODE_DEINDENT_COLUMNS
            | if has_bof_bom {
                M11_INDENTED_CODE_BOF_BOM_FLAG
            } else {
                0
            };
        green[48..56].copy_from_slice(&metadata.to_le_bytes());
        green[56..60].copy_from_slice(&line_count.to_le_bytes());
        green[60..64].copy_from_slice(&projected_utf8.to_le_bytes());
        green[64..68].copy_from_slice(&projected_utf16.to_le_bytes());
        green[68..72].copy_from_slice(&terminal_line_ending_bytes.to_le_bytes());

        let mut projection = [0_u8; M11_PROJECTION_RECORD_BYTES];
        projection[..8].copy_from_slice(PROJECTION_MAGIC);
        projection[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        projection[12] = M11_INDENTED_CODE_VARIANT;
        projection[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        projection[48..56].copy_from_slice(&u64::from(line_count).to_le_bytes());
        (
            M11BlockRoleRecord::new(&green).expect("indented-code Green"),
            M11BlockRoleRecord::new(&projection).expect("indented-code Projection"),
        )
    }

    fn block_indented_code() -> M11BlockSequenceEntry {
        let (green, projection) = indented_code_role_records(26, 3, 17, 16, 0, false);
        M11BlockSequenceEntry::structured(26, 25, 0, green, projection)
            .expect("structured indented code")
    }

    fn block_quote_role_records(
        source_bytes: u32,
        line_count: u32,
        projected_utf8: u32,
        projected_utf16: u32,
    ) -> (M11BlockRoleRecord, M11BlockRoleRecord) {
        let mut green = [0_u8; M11_GREEN_RECORD_BYTES];
        green[..8].copy_from_slice(GREEN_MAGIC);
        green[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        green[12] = M11_BLOCK_QUOTE_VARIANT;
        green[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        green[48..56]
            .copy_from_slice(&M11_BLOCK_QUOTE_EXACT_SINGLE_PARAGRAPH_DISPOSITION.to_le_bytes());
        green[56..60].copy_from_slice(&line_count.to_le_bytes());
        green[60..64].copy_from_slice(&0_u32.to_le_bytes());
        green[64..68].copy_from_slice(&line_count.to_le_bytes());
        green[68..72].copy_from_slice(&projected_utf8.to_le_bytes());
        green[72..76].copy_from_slice(&projected_utf16.to_le_bytes());

        let mut projection = [0_u8; M11_PROJECTION_RECORD_BYTES];
        projection[..8].copy_from_slice(PROJECTION_MAGIC);
        projection[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        projection[12] = M11_BLOCK_QUOTE_VARIANT;
        projection[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        projection[48..56].copy_from_slice(&u64::from(line_count).to_le_bytes());
        (
            M11BlockRoleRecord::new(&green).expect("block-quote Green"),
            M11BlockRoleRecord::new(&projection).expect("block-quote Projection"),
        )
    }

    fn block_quote() -> M11BlockSequenceEntry {
        let (green, projection) = block_quote_role_records(23, 3, 16, 16);
        M11BlockSequenceEntry::structured(23, 23, 0, green, projection)
            .expect("structured block quote")
    }

    fn bullet_list_role_records(
        source_bytes: u32,
        item_count: u32,
        projected_utf8: u32,
        projected_utf16: u32,
    ) -> (M11BlockRoleRecord, M11BlockRoleRecord) {
        let mut green = [0_u8; M11_GREEN_RECORD_BYTES];
        green[..8].copy_from_slice(GREEN_MAGIC);
        green[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        green[12] = M11_BULLET_LIST_VARIANT;
        green[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        let metadata = M11_BULLET_LIST_EXACT_TIGHT_DISPOSITION
            | (u64::from(b'-') << M11_BULLET_LIST_MARKER_SHIFT)
            | M11_BULLET_LIST_TIGHT_FLAG;
        green[48..56].copy_from_slice(&metadata.to_le_bytes());
        green[56..60].copy_from_slice(&item_count.to_le_bytes());
        green[60..64].copy_from_slice(&M11_BULLET_LIST_NO_TERMINAL_EMPTY.to_le_bytes());
        green[64..68].copy_from_slice(&item_count.to_le_bytes());
        green[68..72].copy_from_slice(&projected_utf8.to_le_bytes());
        green[72..76].copy_from_slice(&projected_utf16.to_le_bytes());

        let mut projection = [0_u8; M11_PROJECTION_RECORD_BYTES];
        projection[..8].copy_from_slice(PROJECTION_MAGIC);
        projection[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        projection[12] = M11_BULLET_LIST_VARIANT;
        projection[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        projection[48..56].copy_from_slice(&u64::from(item_count).to_le_bytes());
        (
            M11BlockRoleRecord::new(&green).expect("bullet-list Green"),
            M11BlockRoleRecord::new(&projection).expect("bullet-list Projection"),
        )
    }

    fn block_bullet_list(
        source_bytes: u32,
        item_count: u32,
        projected_utf8: u32,
        projected_utf16: u32,
    ) -> M11BlockSequenceEntry {
        let (green, projection) =
            bullet_list_role_records(source_bytes, item_count, projected_utf8, projected_utf16);
        M11BlockSequenceEntry::structured(
            usize::try_from(source_bytes).expect("bullet-list bytes"),
            usize::try_from(source_bytes).expect("ASCII bullet-list UTF-16"),
            0,
            green,
            projection,
        )
        .expect("structured bullet list")
    }

    fn ordered_list_role_records(
        source_bytes: u32,
        list_start: u32,
        delimiter: u8,
        item_count: u32,
        projected_utf8: u32,
        projected_utf16: u32,
    ) -> (M11BlockRoleRecord, M11BlockRoleRecord) {
        let mut green = [0_u8; M11_GREEN_RECORD_BYTES];
        green[..8].copy_from_slice(GREEN_MAGIC);
        green[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        green[12] = M11_ORDERED_LIST_VARIANT;
        green[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        let metadata = M11_ORDERED_LIST_EXACT_TIGHT_DISPOSITION
            | (u64::from(delimiter) << M11_ORDERED_LIST_DELIMITER_SHIFT)
            | M11_ORDERED_LIST_TIGHT_FLAG;
        green[48..56].copy_from_slice(&metadata.to_le_bytes());
        green[56..60].copy_from_slice(&item_count.to_le_bytes());
        green[60..64].copy_from_slice(&M11_ORDERED_LIST_NO_TERMINAL_EMPTY.to_le_bytes());
        green[64..68].copy_from_slice(&item_count.to_le_bytes());
        green[68..72].copy_from_slice(&projected_utf8.to_le_bytes());
        green[72..76].copy_from_slice(&projected_utf16.to_le_bytes());
        green[76..80].copy_from_slice(&list_start.to_le_bytes());

        let mut projection = [0_u8; M11_PROJECTION_RECORD_BYTES];
        projection[..8].copy_from_slice(PROJECTION_MAGIC);
        projection[8..12].copy_from_slice(&M11_ROLE_SCHEMA.to_le_bytes());
        projection[12] = M11_ORDERED_LIST_VARIANT;
        projection[24..32].copy_from_slice(&u64::from(source_bytes).to_le_bytes());
        projection[48..56].copy_from_slice(&u64::from(item_count).to_le_bytes());
        (
            M11BlockRoleRecord::new(&green).expect("ordered-list Green"),
            M11BlockRoleRecord::new(&projection).expect("ordered-list Projection"),
        )
    }

    fn block_ordered_list(
        source_bytes: u32,
        list_start: u32,
        delimiter: u8,
        item_count: u32,
        projected_utf8: u32,
        projected_utf16: u32,
    ) -> M11BlockSequenceEntry {
        let (green, projection) = ordered_list_role_records(
            source_bytes,
            list_start,
            delimiter,
            item_count,
            projected_utf8,
            projected_utf16,
        );
        M11BlockSequenceEntry::structured(
            usize::try_from(source_bytes).expect("ordered-list bytes"),
            usize::try_from(source_bytes).expect("ASCII ordered-list UTF-16"),
            0,
            green,
            projection,
        )
        .expect("structured ordered list")
    }

    fn persistent_block_snapshot(
        document_session: Id128,
        publication_session: Id128,
        revision: u32,
        parse_generation: u32,
        text: &str,
        entries: Vec<M11BlockSequenceEntry>,
        include_reference: bool,
    ) -> TestSnapshot {
        persistent_block_snapshot_with_root_transform(
            document_session,
            publication_session,
            revision,
            parse_generation,
            text,
            entries,
            include_reference,
            |_, root| root,
        )
    }

    fn persistent_block_snapshot_with_root_transform(
        document_session: Id128,
        publication_session: Id128,
        revision: u32,
        parse_generation: u32,
        text: &str,
        entries: Vec<M11BlockSequenceEntry>,
        include_reference: bool,
        transform: impl FnOnce(&mut DocumentRuntime, M11BlockSequenceRoot) -> M11BlockSequenceRoot,
    ) -> TestSnapshot {
        let mut runtime = DocumentRuntime::from_source_store(
            source_store(text, u64::from(revision)),
            standard_document_runtime_config(),
        )
        .expect("test runtime");
        let source = runtime.current_source_version().expect("current source");
        let parser_profile = ParserProfileId::new(1).expect("parser profile");
        let scan_profile = SourceFactsScanProfile::new(32).expect("scan profile");
        install_persistent_source_facts(&mut runtime, scan_profile, parser_profile);

        let lease = runtime
            .snapshot_current_source()
            .expect("block source lease");
        let mut blocks = M11BlockSequenceBuild::new(&runtime, lease).expect("block build");
        for entry in entries {
            blocks.offer_entry(entry).expect("offer block entry");
            loop {
                match blocks
                    .poll(&mut runtime, 32)
                    .expect("poll block entry")
                    .status()
                {
                    M11BlockSequenceBuildStatus::NeedsInput => break,
                    M11BlockSequenceBuildStatus::Pending => {}
                    M11BlockSequenceBuildStatus::Complete
                    | M11BlockSequenceBuildStatus::Cancelled => {
                        panic!("block build ended before input")
                    }
                }
            }
        }
        blocks.finish_input().expect("finish block input");
        loop {
            match blocks
                .poll(&mut runtime, 64)
                .expect("finish block root")
                .status()
            {
                M11BlockSequenceBuildStatus::Pending => {}
                M11BlockSequenceBuildStatus::Complete => break,
                M11BlockSequenceBuildStatus::NeedsInput
                | M11BlockSequenceBuildStatus::Cancelled => {
                    panic!("finished block build returned the wrong state")
                }
            }
        }
        let block_root = blocks.take_root().expect("block root");
        drop(blocks);
        let mut block_root = transform(&mut runtime, block_root);
        let mut candidate = M11CandidateBuild::new_with_persistent_source_facts_and_blocks(
            &mut runtime,
            id128_bytes(document_session),
            id128_bytes(publication_session),
            source,
            u64::from(parse_generation),
            1,
            scan_profile,
            &block_root,
        )
        .expect("persistent block candidate");
        block_root
            .begin_release(&mut runtime)
            .expect("release original block root");
        while !block_root
            .poll_release(&mut runtime, 64)
            .expect("poll block root release")
            .complete()
        {}
        drop(block_root);

        if include_reference {
            candidate
                .offer_reference(
                    &runtime,
                    M11ReferenceRecord::new(
                        M11ReferenceRange::new(0..7, 0..7),
                        M11ReferenceRange::new(1..2, 1..2),
                        M11ReferenceRange::new(5..7, 5..7),
                        None,
                        Box::<[u8]>::from(&b"a"[..]),
                        Box::<[u8]>::from(&b"/x"[..]),
                        None,
                    ),
                )
                .expect("offer block reference");
            while !candidate.references_idle() {
                assert!(matches!(
                    candidate.poll(&mut runtime, 1).expect("reference poll"),
                    M11CandidateBuildPoll::Pending { .. }
                ));
            }
        }
        candidate
            .finish_references(&runtime)
            .expect("finish references");
        while let M11CandidateBuildPoll::Pending { .. } = candidate
            .poll(&mut runtime, 256)
            .expect("persistent block candidate poll")
        {}
        let publication = candidate.into_publication().expect("block publication");
        let wire_source = SourceVersion {
            document_session,
            revision,
            utf8_length: u32::try_from(text.len()).expect("source bytes"),
            utf16_length: u32::try_from(text.encode_utf16().count()).expect("source UTF-16"),
            content_hash128: [revision, revision + 1, revision + 2, revision + 3],
        };
        snapshot_from_publication(
            runtime,
            publication,
            wire_source,
            publication_session,
            parse_generation,
        )
    }

    fn admit_and_credit(host: &mut NativeCandidateHost, frame: &TestFrame) {
        let encoded = packet_bytes(std::slice::from_ref(frame));
        admit_packet_bytes(host, &encoded);
        assert!(matches!(
            host.poll(HostWorkGrant {
                inspect_bytes: M11_HOST_MAXIMUM_FRAME_BYTES as u32
                    + PACKET_FRAME_DESCRIPTOR_BYTES as u32,
                copy_bytes: M11_HOST_MAXIMUM_FRAME_BYTES as u32,
                transitions: 1,
            })
            .unwrap_or_else(|error| panic!("frame {} poll: {error:?}", frame.ordinal)),
            HostPollOutcome::PacketCredit { .. }
        ));
    }

    fn packet_bytes(frames: &[TestFrame]) -> Vec<u8> {
        let first = frames.first().expect("packet has at least one frame");
        let frames = frames
            .iter()
            .map(|frame| PublicationPacketFrameInput {
                record_count: frame.record_count,
                digest: frame.digest,
                bytes: &frame.bytes,
            })
            .collect::<Vec<_>>();
        let mut output = vec![0_u8; MAXIMUM_PACKET_ENCODED_BYTES];
        let written = encode_publication_packet_into(
            PublicationPacketInput {
                offer_id: first.offer_id,
                first_frame_ordinal: first.ordinal,
                first_record_ordinal: first.first_record_ordinal,
                frames: &frames,
            },
            &mut output,
        )
        .expect("encode test packet");
        output.truncate(written);
        output
    }

    fn admit_packet_bytes(host: &mut NativeCandidateHost, bytes: &[u8]) {
        let packet = decode_publication_packet_envelope(bytes).expect("packet envelope");
        host.admit_packet(packet).expect("admit packet");
    }

    fn poll_installed(host: &mut NativeCandidateHost) -> StructuralAck {
        loop {
            match host
                .poll(HostWorkGrant {
                    inspect_bytes: 0,
                    copy_bytes: 0,
                    transitions: 256,
                })
                .expect("install poll")
            {
                HostPollOutcome::Pending => {}
                HostPollOutcome::Committed(ack) => return ack,
                outcome => panic!("unexpected install outcome: {outcome:?}"),
            }
        }
    }

    fn install(host: &mut NativeCandidateHost, snapshot: &TestSnapshot) -> StructuralAck {
        host.observe_source_version(snapshot.source)
            .expect("observe exact source");
        host.begin_offer(snapshot.offer).expect("begin offer");
        for frame in &snapshot.frames {
            admit_and_credit(host, frame);
        }
        host.request_commit(snapshot.commit)
            .expect("commit request");
        loop {
            match host
                .poll(HostWorkGrant {
                    inspect_bytes: 0,
                    copy_bytes: 0,
                    transitions: 256,
                })
                .expect("install poll")
            {
                HostPollOutcome::Pending => {}
                HostPollOutcome::Committed(ack) => return ack,
                outcome => panic!("unexpected install outcome: {outcome:?}"),
            }
        }
    }

    fn admit_and_credit_inline_sidecar(host: &mut NativeCandidateHost, frame: &TestFrame) {
        let encoded = packet_bytes(std::slice::from_ref(frame));
        let packet = decode_publication_packet_envelope(&encoded).expect("sidecar packet envelope");
        host.admit_inline_sidecar_packet(packet)
            .expect("admit sidecar packet");
        assert!(matches!(
            host.poll_inline_sidecar(HostWorkGrant {
                inspect_bytes: M11_HOST_MAXIMUM_FRAME_BYTES as u32
                    + PACKET_FRAME_DESCRIPTOR_BYTES as u32,
                copy_bytes: M11_HOST_MAXIMUM_FRAME_BYTES as u32,
                transitions: 1,
            })
            .unwrap_or_else(|error| panic!("sidecar frame {} poll: {error:?}", frame.ordinal)),
            InlineSidecarHostPollOutcome::PacketCredit { .. }
        ));
    }

    fn reject_first_inline_sidecar_frame(
        host: &mut NativeCandidateHost,
        frame: &TestFrame,
    ) -> HostStoreError {
        let encoded = packet_bytes(std::slice::from_ref(frame));
        let packet = decode_publication_packet_envelope(&encoded).expect("sidecar packet envelope");
        host.admit_inline_sidecar_packet(packet)
            .expect("admit sidecar packet");
        host.poll_inline_sidecar(HostWorkGrant {
            inspect_bytes: M11_HOST_MAXIMUM_FRAME_BYTES as u32
                + PACKET_FRAME_DESCRIPTOR_BYTES as u32,
            copy_bytes: M11_HOST_MAXIMUM_FRAME_BYTES as u32,
            transitions: 1,
        })
        .expect_err("mismatched sidecar Begin must fail")
    }

    fn install_inline_sidecar(
        host: &mut NativeCandidateHost,
        sidecar: &TestInlineSidecar,
        base_ack: StructuralAck,
    ) -> InlineSidecarAck {
        host.begin_inline_sidecar_offer(sidecar.begin(base_ack))
            .expect("begin sidecar offer");
        for frame in &sidecar.frames {
            admit_and_credit_inline_sidecar(host, frame);
        }
        host.request_inline_sidecar_commit(sidecar.commit)
            .expect("sidecar commit request");
        loop {
            match host
                .poll_inline_sidecar(HostWorkGrant {
                    inspect_bytes: 0,
                    copy_bytes: 0,
                    transitions: 256,
                })
                .expect("sidecar install poll")
            {
                InlineSidecarHostPollOutcome::Pending => {}
                InlineSidecarHostPollOutcome::Committed(ack) => return ack,
                outcome => panic!("unexpected sidecar install outcome: {outcome:?}"),
            }
        }
    }

    fn admit_and_credit_viewport(
        host: &mut NativeCandidateHost,
        frame: &TestFrame,
    ) -> Result<(), HostStoreError> {
        let encoded = packet_bytes(std::slice::from_ref(frame));
        let packet =
            decode_publication_packet_envelope(&encoded).expect("viewport packet envelope");
        host.admit_viewport_presentation_packet(packet)?;
        for _ in 0..16 {
            match host.poll_viewport_presentation(HostWorkGrant {
                inspect_bytes: MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES
                    + PACKET_FRAME_DESCRIPTOR_BYTES as u32,
                copy_bytes: MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES,
                transitions: 1,
            })? {
                HostViewportPresentationPollOutcome::Pending => {}
                HostViewportPresentationPollOutcome::PacketCredit {
                    next_frame_ordinal, ..
                } => {
                    assert_eq!(next_frame_ordinal, frame.ordinal + 1);
                    return Ok(());
                }
                outcome => panic!("unexpected viewport frame outcome: {outcome:?}"),
            }
        }
        panic!("viewport frame credit did not converge")
    }

    fn install_viewport_presentation(
        host: &mut NativeCandidateHost,
        presentation: &TestViewportPresentation,
    ) -> crate::v3_publication_wire::ViewportPresentationAck {
        host.begin_viewport_presentation_offer(presentation.begin)
            .expect("begin viewport presentation");
        for frame in &presentation.frames {
            admit_and_credit_viewport(host, frame)
                .unwrap_or_else(|error| panic!("viewport frame {}: {error:?}", frame.ordinal));
        }
        host.request_viewport_presentation_commit(presentation.commit)
            .expect("viewport commit request");
        for _ in 0..16 {
            match host
                .poll_viewport_presentation(HostWorkGrant {
                    inspect_bytes: 0,
                    copy_bytes: 0,
                    transitions: 1,
                })
                .expect("viewport install poll")
            {
                HostViewportPresentationPollOutcome::Pending => {}
                HostViewportPresentationPollOutcome::Committed(ack) => return ack,
                outcome => panic!("unexpected viewport install outcome: {outcome:?}"),
            }
        }
        panic!("viewport install did not converge")
    }

    fn finish_viewport_abort(host: &mut NativeCandidateHost, offer_id: Id128) {
        for _ in 0..512 {
            match host
                .poll_viewport_presentation(HostWorkGrant {
                    inspect_bytes: 0,
                    copy_bytes: 0,
                    transitions: 1,
                })
                .expect("viewport abort poll")
            {
                HostViewportPresentationPollOutcome::Pending => {}
                HostViewportPresentationPollOutcome::AbortComplete {
                    offer_id: completed,
                } => {
                    assert_eq!(completed, offer_id);
                    return;
                }
                outcome => panic!("unexpected viewport abort outcome: {outcome:?}"),
            }
        }
        panic!("viewport abort did not converge")
    }

    fn joined_sidecar_query_budget(host: &NativeCandidateHost) -> HostQueryBudget {
        let (blocks, mut budget) = persistent_block_query_plan(host);
        let sidecar = host
            .installed_inline_sidecar
            .as_ref()
            .expect("installed sidecar");
        let (
            inline_bytes,
            inline_pages,
            inline_depth,
            inline_tree_nodes,
            header_bytes,
            point_path_bytes,
            additional_leaf_count,
        ) = host
            .inline_sidecar
            .as_ref()
            .expect("sidecar engine")
            .query(&sidecar.binding)
            .expect("sidecar query plan")
            .map(|query| match query {
                M11HostInlineSidecarQuery::Authoritative { descriptor, .. } => (
                    M11_INLINE_META_RECORD_BYTES
                        + usize::try_from(descriptor.fact_count()).expect("sidecar facts")
                            * M11_INLINE_FACT_RECORD_BYTES,
                    u32::try_from(descriptor.logical_page_count()).expect("sidecar pages"),
                    descriptor.maximum_open_depth(),
                    u32::try_from(descriptor.maximum_tree_nodes_visited())
                        .expect("sidecar tree nodes"),
                    M11_VIEWPORT_INLINE_HEADER_BYTES,
                    0,
                    1,
                ),
                M11HostInlineSidecarQuery::ProjectedInline { .. } => {
                    panic!("projected-inline sidecars use the direct query lane")
                }
                M11HostInlineSidecarQuery::IndentedCode { descriptor, .. } => (
                    usize::try_from(descriptor.line_count()).expect("indented-code lines")
                        * M11_INDENTED_CODE_LINE_RECORD_BYTES,
                    u32::try_from(descriptor.logical_page_count()).expect("indented-code pages"),
                    descriptor.maximum_open_depth(),
                    u32::try_from(descriptor.maximum_tree_nodes_visited())
                        .expect("indented-code tree nodes"),
                    M11_VIEWPORT_V3_HEADER_BYTES,
                    0,
                    1,
                ),
                M11HostInlineSidecarQuery::BlockQuote { descriptor, .. } => (
                    usize::try_from(descriptor.line_count()).expect("block-quote lines")
                        * M11_BLOCK_QUOTE_LINE_RECORD_BYTES,
                    u32::try_from(descriptor.logical_page_count()).expect("block-quote pages"),
                    descriptor.maximum_open_depth().max(2),
                    u32::try_from(descriptor.maximum_tree_nodes_visited())
                        .expect("block-quote tree nodes"),
                    M11_VIEWPORT_V4_HEADER_BYTES,
                    M11_BLOCK_QUOTE_POINT_PATH_BYTES,
                    u32::try_from(M11_BLOCK_QUOTE_POINT_PATH_NODE_COUNT).expect("point-path nodes"),
                ),
                M11HostInlineSidecarQuery::BulletList {
                    selected_item_ordinal,
                    descriptor,
                    ..
                } => {
                    let compact_item = selected_item_ordinal.is_some();
                    (
                        usize::try_from(descriptor.line_count()).expect("bullet-list items")
                            * M11_BULLET_LIST_ITEM_RECORD_BYTES
                            + if compact_item {
                                M11_BULLET_LIST_ITEM_META_BYTES
                            } else {
                                0
                            },
                        u32::try_from(descriptor.logical_page_count()).expect("bullet-list pages"),
                        descriptor.maximum_open_depth().max(3),
                        u32::try_from(descriptor.maximum_tree_nodes_visited())
                            .expect("bullet-list tree nodes"),
                        if compact_item {
                            M11_VIEWPORT_V6_HEADER_BYTES
                        } else {
                            M11_VIEWPORT_V5_HEADER_BYTES
                        },
                        3 * M11_POINT_PATH_V5_NODE_RECORD_BYTES,
                        3,
                    )
                }
                M11HostInlineSidecarQuery::OrderedList { descriptor, .. } => (
                    M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES,
                    u32::try_from(descriptor.logical_page_count()).expect("ordered-list pages"),
                    descriptor.maximum_open_depth().max(3),
                    u32::try_from(descriptor.maximum_tree_nodes_visited())
                        .expect("ordered-list tree nodes"),
                    M11_VIEWPORT_V7_HEADER_BYTES,
                    3 * M11_POINT_PATH_V5_NODE_RECORD_BYTES,
                    3,
                ),
                M11HostInlineSidecarQuery::Unsupported { .. } => (
                    M11_INLINE_META_RECORD_BYTES,
                    0,
                    1,
                    0,
                    M11_VIEWPORT_INLINE_HEADER_BYTES,
                    0,
                    1,
                ),
            })
            .expect("matching installed sidecar");
        budget.maximum_encoded_bytes = u32::try_from(
            header_bytes
                + M11_GREEN_RECORD_BYTES
                + M11_PROJECTION_RECORD_BYTES
                + point_path_bytes
                + inline_bytes,
        )
        .expect("joined viewport bytes");
        budget.maximum_open_depth = budget.maximum_open_depth.max(inline_depth);
        budget.maximum_leaf_count = blocks
            .maximum_entries_scanned()
            .checked_add(inline_pages)
            .and_then(|leaves| leaves.checked_add(additional_leaf_count))
            .expect("joined viewport leaves");
        budget.maximum_tree_nodes_visited = budget
            .maximum_tree_nodes_visited
            .checked_add(inline_tree_nodes)
            .expect("joined viewport tree nodes");
        budget
    }

    #[test]
    fn hot_inline_begin_binds_outer_descriptor_length_and_hio1_digest() {
        let document = [735, 736, 737, 738];
        let (base, authoritative, _) =
            snapshot_with_inline_sidecar_pair(document, [745, 746, 747, 748], 1, 1);

        let mut size_host = host_for(document);
        let size_base_ack = install(&mut size_host, &base);
        size_host
            .acknowledge_delivery(size_base_ack)
            .expect("acknowledge size-test base");
        let mut wrong_size = authoritative.begin(size_base_ack);
        assert_eq!(
            wrong_size.envelope.ipr2_descriptor_bytes,
            crate::v3_publication_wire::IPR3_DESCRIPTOR_BYTES
        );
        wrong_size.envelope.ipr2_descriptor_bytes = BLOCK_QUOTE_PROJECTION_DESCRIPTOR_BYTES;
        size_host
            .begin_inline_sidecar_offer(wrong_size)
            .expect("both authoritative descriptor widths are admitted provisionally");
        let error = reject_first_inline_sidecar_frame(&mut size_host, &authoritative.frames[0]);
        assert_eq!(error.reason(), HostRejectReason::CorruptPublication);
        close_host(&mut size_host);

        let mut digest_host = host_for(document);
        let digest_base_ack = install(&mut digest_host, &base);
        digest_host
            .acknowledge_delivery(digest_base_ack)
            .expect("acknowledge digest-test base");
        let mut wrong_digest = authoritative.begin(digest_base_ack);
        wrong_digest.envelope.hio1_envelope_digest256[0] ^= 1;
        digest_host
            .begin_inline_sidecar_offer(wrong_digest)
            .expect("digest is authenticated against the ordinal-0 frame");
        let error = reject_first_inline_sidecar_frame(&mut digest_host, &authoritative.frames[0]);
        assert_eq!(error.reason(), HostRejectReason::CorruptPublication);
        close_host(&mut digest_host);
    }

    #[test]
    fn hot_inline_sidecar_installs_as_sibling_joins_block_query_and_invalidates_on_new_base() {
        let document = [741, 742, 743, 744];
        let (base, authoritative, unsupported) =
            snapshot_with_inline_sidecar_pair(document, [751, 752, 753, 754], 1, 1);
        let mut host = host_for(document);
        let base_ack = install(&mut host, &base);
        host.acknowledge_delivery(base_ack)
            .expect("acknowledge structural base");

        let authoritative_ack = install_inline_sidecar(&mut host, &authoritative, base_ack);
        assert_eq!(authoritative_ack.base_ack, base_ack);
        assert_eq!(
            authoritative_ack.disposition,
            InlineSidecarAckDisposition::Authoritative
        );
        assert_eq!(
            host.installed_ack,
            Some(base_ack),
            "a sibling sidecar never replaces StructuralAck"
        );
        let mut typed = [0_u8; M11_INLINE_FACT_RECORD_BYTES];
        assert!(matches!(
            host.query_inline_sidecar(authoritative.binding, &mut typed)
                .expect("typed authoritative sidecar query"),
            HostInlineSidecarQueryOutcome::Authoritative {
                payload_kind: HostInlineSidecarPayloadKind::Inline,
                fact_count: 1,
                encoded_bytes,
                ..
            } if encoded_bytes == M11_INLINE_FACT_RECORD_BYTES as u32
        ));
        assert_eq!(typed[0], M11InlineProjectionKind::Strong as u8);
        assert_eq!(read_u32(&typed, 4), 0);
        assert_eq!(read_u32(&typed, 8), 5);
        assert_eq!(read_u32(&typed, 12), 2);
        assert_eq!(read_u32(&typed, 16), 1);

        let (_, structural_only_budget) = persistent_block_query_plan(&host);
        let mut undersized_viewport = [0xa5; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                query_for(
                    base.source,
                    HostSourceMetric::default(),
                    HostMetricAffinity::Downstream,
                    structural_only_budget,
                ),
                &mut undersized_viewport,
            )
            .expect("sidecar-aware structural budget gate");
        assert!(matches!(
            outcome,
            HostStructuralQueryOutcome::SourceGap {
                reason: HostSourceGapReason::EncodedByteLimit,
                ..
            }
        ));
        assert_eq!(
            undersized_viewport, [0xa5; HOST_M11_VIEWPORT_BYTES],
            "a budget gap must not claim or mutate viewport output"
        );

        let budget = joined_sidecar_query_budget(&host);
        let mut viewport = vec![0xa5; budget.maximum_encoded_bytes as usize];
        let outcome = host
            .query_structural(
                query_for(
                    base.source,
                    HostSourceMetric::default(),
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut viewport,
            )
            .expect("joined authoritative block query");
        let HostStructuralQueryOutcome::Viewport { receipt, range, .. } = outcome else {
            panic!("joined authoritative sidecar must render a viewport: {outcome:?}");
        };
        assert_eq!(
            range,
            HostMetricRange {
                start: HostSourceMetric::default(),
                end: HostSourceMetric { bytes: 5, utf16: 5 },
            }
        );
        assert_eq!(read_u32(&viewport, 8), M11_VIEWPORT_INLINE_SCHEMA);
        assert_eq!(receipt.encoded_bytes, budget.maximum_encoded_bytes);
        let metadata =
            M11_VIEWPORT_INLINE_HEADER_BYTES + M11_GREEN_RECORD_BYTES + M11_PROJECTION_RECORD_BYTES;
        assert_eq!(&viewport[metadata..metadata + 8], M11_INLINE_META_MAGIC);
        assert_eq!(viewport[metadata + 12], 1);
        assert_eq!(read_u32(&viewport, metadata + 20), 1);
        assert_eq!(read_u64(&viewport, metadata + 24), 0);
        assert_eq!(read_u64(&viewport, metadata + 32), 5);
        assert_eq!(viewport[metadata + M11_INLINE_META_RECORD_BYTES], 2);

        host.acknowledge_inline_sidecar_delivery(authoritative_ack)
            .expect("acknowledge authoritative sidecar");
        let unsupported_ack = install_inline_sidecar(&mut host, &unsupported, base_ack);
        assert_eq!(unsupported_ack.base_ack, base_ack);
        assert_eq!(
            unsupported_ack.disposition,
            InlineSidecarAckDisposition::Unsupported
        );
        let mut metadata_output = [0_u8; 64];
        assert_eq!(
            host.query_inline_sidecar(unsupported.binding, &mut metadata_output)
                .expect("typed unsupported sidecar query"),
            HostInlineSidecarQueryOutcome::Unsupported {
                reason: 0x2000_0002,
                metadata_bytes: 18,
            }
        );
        assert_eq!(&metadata_output[..18], b"parser unsupported");

        let budget = joined_sidecar_query_budget(&host);
        let mut viewport = vec![0xa5; budget.maximum_encoded_bytes as usize];
        let outcome = host
            .query_structural(
                query_for(
                    base.source,
                    HostSourceMetric::default(),
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut viewport,
            )
            .expect("joined unsupported block query");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("joined unsupported sidecar must render a viewport: {outcome:?}");
        };
        assert_eq!(read_u32(&viewport, 8), M11_VIEWPORT_INLINE_SCHEMA);
        assert_eq!(viewport[metadata + 12], 2);
        assert_eq!(read_u32(&viewport, metadata + 20), 0);
        assert_eq!(receipt.encoded_bytes, budget.maximum_encoded_bytes);
        host.acknowledge_inline_sidecar_delivery(unsupported_ack)
            .expect("acknowledge unsupported sidecar");

        let replacement = snapshot(document, [761, 762, 763, 764], 2, 2, 1);
        let (replacement_ack, sidecar_reclaim_polls) =
            install_back_to_back(&mut host, &replacement);
        assert!(
            sidecar_reclaim_polls > 0,
            "structural transfer must fuel-drain the replaced sidecar first"
        );
        assert_ne!(replacement_ack, base_ack);
        assert_eq!(
            host.query_inline_sidecar(unsupported.binding, &mut metadata_output)
                .expect("query invalidated sidecar"),
            HostInlineSidecarQueryOutcome::Unavailable
        );
        close_host(&mut host);
    }

    #[test]
    fn compact_bullet_item_sidecar_keeps_a_2000_item_viewport_constant_and_selection_local() {
        const ITEM_COUNT: u32 = 2_000;
        const SELECTED_ITEM: u32 = 1_537;
        const EXPECTED_VIEWPORT_BYTES: usize = M11_VIEWPORT_V6_HEADER_BYTES
            + M11_GREEN_RECORD_BYTES
            + M11_PROJECTION_RECORD_BYTES
            + 3 * M11_POINT_PATH_V5_NODE_RECORD_BYTES
            + M11_BULLET_LIST_ITEM_META_BYTES
            + M11_BULLET_LIST_ITEM_RECORD_BYTES;

        let document = [801, 802, 803, 804];
        let (base, compact, selected_start, selected_end) =
            snapshot_with_compact_bullet_item_sidecar(
                document,
                [811, 812, 813, 814],
                1,
                1,
                ITEM_COUNT,
                SELECTED_ITEM,
            );
        let mut host = host_for(document);
        let base_ack = install(&mut host, &base);
        host.acknowledge_delivery(base_ack)
            .expect("acknowledge compact bullet-list base");
        let sidecar_ack = install_inline_sidecar(&mut host, &compact, base_ack);

        let budget = joined_sidecar_query_budget(&host);
        assert_eq!(
            budget.maximum_encoded_bytes as usize,
            EXPECTED_VIEWPORT_BYTES
        );
        assert_eq!(
            EXPECTED_VIEWPORT_BYTES, 300,
            "compact viewport size must not carry all 2,000 item records"
        );
        let mut viewport = vec![0xa5; EXPECTED_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                query_for(
                    base.source,
                    HostSourceMetric {
                        bytes: selected_start + 3,
                        utf16: selected_start + 3,
                    },
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut viewport,
            )
            .expect("compact bullet-item structural query");
        let HostStructuralQueryOutcome::Viewport { receipt, range, .. } = outcome else {
            panic!("compact bullet item must join its structural block: {outcome:?}");
        };
        assert_eq!(range.start, HostSourceMetric::default());
        assert_eq!(
            range.end,
            HostSourceMetric {
                bytes: base.source.utf8_length,
                utf16: base.source.utf16_length,
            }
        );
        assert_eq!(receipt.encoded_bytes as usize, EXPECTED_VIEWPORT_BYTES);
        assert_eq!(read_u32(&viewport, 8), M11_VIEWPORT_V6_SCHEMA);
        assert_eq!(
            u16::from_le_bytes(viewport[20..22].try_into().expect("point-path count")),
            3
        );
        assert_eq!(viewport[22], M11_LEAF_PROJECTION_PAYLOAD_BULLET_LIST_ITEM);
        assert_eq!(
            read_u32(&viewport, 24),
            (3 * M11_POINT_PATH_V5_NODE_RECORD_BYTES) as u32
        );
        assert_eq!(
            read_u32(&viewport, 28),
            (M11_BULLET_LIST_ITEM_META_BYTES + M11_BULLET_LIST_ITEM_RECORD_BYTES) as u32
        );

        let point_path =
            M11_VIEWPORT_V6_HEADER_BYTES + M11_GREEN_RECORD_BYTES + M11_PROJECTION_RECORD_BYTES;
        assert_eq!(viewport[point_path], M11_POINT_PATH_KIND_LIST);
        assert_eq!(read_u32(&viewport, point_path + 20), ITEM_COUNT);
        let item_node = point_path + M11_POINT_PATH_V5_NODE_RECORD_BYTES;
        assert_eq!(viewport[item_node], M11_POINT_PATH_KIND_LIST_ITEM);
        assert_eq!(read_u32(&viewport, item_node + 8), selected_start);
        assert_eq!(read_u32(&viewport, item_node + 12), selected_end);
        assert_eq!(read_u32(&viewport, item_node + 16), SELECTED_ITEM);
        assert_eq!(read_u32(&viewport, item_node + 24), 10);
        let paragraph_node = item_node + M11_POINT_PATH_V5_NODE_RECORD_BYTES;
        assert_eq!(viewport[paragraph_node], M11_POINT_PATH_KIND_PARAGRAPH);
        assert_eq!(read_u32(&viewport, paragraph_node + 8), selected_start + 2);
        assert_eq!(
            read_u32(&viewport, paragraph_node + 12),
            selected_start + 10
        );
        assert_eq!(read_u32(&viewport, paragraph_node + 16), SELECTED_ITEM);
        assert_eq!(read_u32(&viewport, paragraph_node + 24), 8);

        let payload = point_path + 3 * M11_POINT_PATH_V5_NODE_RECORD_BYTES;
        assert_eq!(read_u32(&viewport, payload), SELECTED_ITEM);
        assert_eq!(viewport[payload + 4], 2, "canonical CRLF code");
        assert_eq!(&viewport[payload + 5..payload + 8], &[0; 3]);
        let record = payload + M11_BULLET_LIST_ITEM_META_BYTES;
        assert_eq!(read_u32(&viewport, record), selected_start);
        assert_eq!(read_u32(&viewport, record + 4), 12);
        assert_eq!(read_u32(&viewport, record + 8), 2);
        assert_eq!(read_u32(&viewport, record + 12), 0);
        assert_eq!(read_u32(&viewport, record + 16), 2);
        assert_eq!(read_u32(&viewport, record + 20), 8);
        assert_eq!(read_u32(&viewport, record + 24), 8);

        let mut outside = vec![0xa5; EXPECTED_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                query_for(
                    base.source,
                    HostSourceMetric {
                        bytes: selected_end + 3,
                        utf16: selected_end + 3,
                    },
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut outside,
            )
            .expect("outside compact bullet-item query");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("outside query must still return structural authority: {outcome:?}");
        };
        assert_eq!(read_u32(&outside, 8), M11_ROLE_SCHEMA);
        assert_eq!(
            receipt.encoded_bytes as usize,
            M11_VIEWPORT_HEADER_BYTES + M11_GREEN_RECORD_BYTES + M11_PROJECTION_RECORD_BYTES
        );
        host.acknowledge_inline_sidecar_delivery(sidecar_ack)
            .expect("acknowledge compact bullet-item sidecar");
        close_host(&mut host);
    }

    #[test]
    fn compact_ordered_item_sidecar_has_a_distinct_constant_viewport_and_exact_marker_receipt() {
        const ITEM_COUNT: u32 = 2_000;
        const SELECTED_ITEM: u32 = 1_537;
        const EXPECTED_VIEWPORT_BYTES: usize = M11_VIEWPORT_V7_HEADER_BYTES
            + M11_GREEN_RECORD_BYTES
            + M11_PROJECTION_RECORD_BYTES
            + 3 * M11_POINT_PATH_V5_NODE_RECORD_BYTES
            + M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES;

        let document = [821, 822, 823, 824];
        let (base, compact, selected_start, selected_end) =
            snapshot_with_compact_ordered_item_sidecar(
                document,
                [831, 832, 833, 834],
                1,
                1,
                ITEM_COUNT,
                SELECTED_ITEM,
            );
        let mut host = host_for(document);
        let base_ack = install(&mut host, &base);
        host.acknowledge_delivery(base_ack)
            .expect("acknowledge compact ordered-list base");
        let sidecar_ack = install_inline_sidecar(&mut host, &compact, base_ack);

        let mut raw_payload = [0xa5; M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES];
        assert!(matches!(
            host.query_inline_sidecar(compact.binding, &mut raw_payload)
                .expect("compact ordered-item sidecar query"),
            HostInlineSidecarQueryOutcome::Authoritative {
                payload_kind: HostInlineSidecarPayloadKind::OrderedListItem,
                fact_count: 1,
                encoded_bytes,
                tree_nodes_visited,
                ..
            } if encoded_bytes == M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES as u32
                && tree_nodes_visited > 0
        ));
        assert_eq!(read_u32(&raw_payload, 0), SELECTED_ITEM);
        assert_eq!(raw_payload[4], 2, "canonical CRLF code");
        assert_eq!(&raw_payload[5..8], &[0; 3]);
        assert_eq!(read_u32(&raw_payload, 8), 2);
        assert_eq!(read_u32(&raw_payload, 12), 5);
        assert_eq!(read_u32(&raw_payload, 16), 42);
        assert_eq!(read_u32(&raw_payload, 20), selected_start);
        assert_eq!(read_u32(&raw_payload, 24), 16);
        assert_eq!(read_u32(&raw_payload, 28), 6);
        assert_eq!(read_u32(&raw_payload, 32), 2);
        assert_eq!(read_u32(&raw_payload, 36), 6);
        assert_eq!(read_u32(&raw_payload, 40), 8);
        assert_eq!(read_u32(&raw_payload, 44), 8);

        let budget = joined_sidecar_query_budget(&host);
        assert_eq!(
            budget.maximum_encoded_bytes as usize,
            EXPECTED_VIEWPORT_BYTES
        );
        assert_eq!(
            EXPECTED_VIEWPORT_BYTES, 312,
            "ordered compact viewport size must not carry all 2,000 item records"
        );
        let mut viewport = vec![0xa5; EXPECTED_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                query_for(
                    base.source,
                    HostSourceMetric {
                        bytes: selected_start + 8,
                        utf16: selected_start + 8,
                    },
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut viewport,
            )
            .expect("compact ordered-item structural query");
        let HostStructuralQueryOutcome::Viewport { receipt, range, .. } = outcome else {
            panic!("compact ordered item must join its structural block: {outcome:?}");
        };
        assert_eq!(range.start, HostSourceMetric::default());
        assert_eq!(
            range.end,
            HostSourceMetric {
                bytes: base.source.utf8_length,
                utf16: base.source.utf16_length,
            }
        );
        assert_eq!(receipt.encoded_bytes as usize, EXPECTED_VIEWPORT_BYTES);
        assert_eq!(read_u32(&viewport, 8), M11_VIEWPORT_V7_SCHEMA);
        assert_eq!(
            u16::from_le_bytes(viewport[20..22].try_into().expect("point-path count")),
            3
        );
        assert_eq!(viewport[22], M11_LEAF_PROJECTION_PAYLOAD_ORDERED_LIST_ITEM);
        assert_eq!(viewport[23], 0);
        assert_eq!(
            read_u32(&viewport, 24),
            (3 * M11_POINT_PATH_V5_NODE_RECORD_BYTES) as u32
        );
        assert_eq!(
            read_u32(&viewport, 28),
            M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES as u32
        );

        let green = M11_VIEWPORT_V7_HEADER_BYTES;
        assert_eq!(viewport[green + 12], M11_ORDERED_LIST_VARIANT);
        assert_eq!(
            ((read_u64(&viewport, green + 48) >> M11_ORDERED_LIST_DELIMITER_SHIFT) & 0xff) as u8,
            b')'
        );
        assert_eq!(read_u32(&viewport, green + 76), 42);

        let point_path =
            M11_VIEWPORT_V7_HEADER_BYTES + M11_GREEN_RECORD_BYTES + M11_PROJECTION_RECORD_BYTES;
        assert_eq!(viewport[point_path], M11_POINT_PATH_KIND_LIST);
        assert_eq!(read_u32(&viewport, point_path + 20), ITEM_COUNT);
        let item_node = point_path + M11_POINT_PATH_V5_NODE_RECORD_BYTES;
        assert_eq!(viewport[item_node], M11_POINT_PATH_KIND_LIST_ITEM);
        assert_eq!(read_u32(&viewport, item_node + 8), selected_start);
        assert_eq!(read_u32(&viewport, item_node + 12), selected_end);
        assert_eq!(read_u32(&viewport, item_node + 16), SELECTED_ITEM);
        assert_eq!(read_u32(&viewport, item_node + 24), 10);
        let paragraph_node = item_node + M11_POINT_PATH_V5_NODE_RECORD_BYTES;
        assert_eq!(viewport[paragraph_node], M11_POINT_PATH_KIND_PARAGRAPH);
        assert_eq!(read_u32(&viewport, paragraph_node + 8), selected_start + 6);
        assert_eq!(
            read_u32(&viewport, paragraph_node + 12),
            selected_start + 14
        );
        assert_eq!(read_u32(&viewport, paragraph_node + 16), SELECTED_ITEM);
        assert_eq!(read_u32(&viewport, paragraph_node + 24), 8);

        let payload = point_path + 3 * M11_POINT_PATH_V5_NODE_RECORD_BYTES;
        assert_eq!(
            &viewport[payload..payload + M11_ORDERED_LIST_ITEM_PAYLOAD_BYTES],
            &raw_payload
        );

        let mut outside = vec![0xa5; EXPECTED_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                query_for(
                    base.source,
                    HostSourceMetric {
                        bytes: selected_end + 8,
                        utf16: selected_end + 8,
                    },
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut outside,
            )
            .expect("outside compact ordered-item query");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("outside query must still return structural authority: {outcome:?}");
        };
        assert_eq!(read_u32(&outside, 8), M11_ROLE_SCHEMA);
        assert_eq!(
            receipt.encoded_bytes as usize,
            M11_VIEWPORT_HEADER_BYTES + M11_GREEN_RECORD_BYTES + M11_PROJECTION_RECORD_BYTES
        );
        host.acknowledge_inline_sidecar_delivery(sidecar_ack)
            .expect("acknowledge compact ordered-item sidecar");
        close_host(&mut host);
    }

    #[test]
    fn ordered_list_structural_admission_fails_closed_on_unknown_or_inconsistent_metadata() {
        let (green, projection) = ordered_list_role_records(32, 42, b')', 2, 20, 20);
        assert!(validate_leaf_relative_ordered_list_records(
            32,
            32,
            0,
            green.as_bytes(),
            projection.as_bytes(),
        ));

        let mut malformed_green = green.as_bytes().to_vec();
        let unknown_metadata = read_u64(&malformed_green, 48) | (1 << 17);
        malformed_green[48..56].copy_from_slice(&unknown_metadata.to_le_bytes());
        assert!(!validate_leaf_relative_ordered_list_records(
            32,
            32,
            0,
            &malformed_green,
            projection.as_bytes(),
        ));

        malformed_green.copy_from_slice(green.as_bytes());
        malformed_green[49] = b'-';
        assert!(!validate_leaf_relative_ordered_list_records(
            32,
            32,
            0,
            &malformed_green,
            projection.as_bytes(),
        ));

        malformed_green.copy_from_slice(green.as_bytes());
        malformed_green[76..80].copy_from_slice(&1_000_000_000_u32.to_le_bytes());
        assert!(!validate_leaf_relative_ordered_list_records(
            32,
            32,
            0,
            &malformed_green,
            projection.as_bytes(),
        ));

        let mut malformed_projection = projection.as_bytes().to_vec();
        malformed_projection[48..56].copy_from_slice(&3_u64.to_le_bytes());
        assert!(!validate_leaf_relative_ordered_list_records(
            32,
            32,
            0,
            green.as_bytes(),
            &malformed_projection,
        ));
    }

    #[test]
    fn ordered_list_initial_viewport_admits_zero_padded_start_and_arbitrary_later_ordinal() {
        const SOURCE: &str = "007) **alpha**\r\n9) beta\r\n";
        let document = [841, 842, 843, 844];
        let snapshot = persistent_block_snapshot(
            document,
            [851, 852, 853, 854],
            1,
            1,
            SOURCE,
            vec![block_ordered_list(SOURCE.len() as u32, 7, b')', 2, 17, 17)],
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (_, budget) = persistent_block_query_plan(&host);
        for position in [
            HostSourceMetric { bytes: 0, utf16: 0 },
            HostSourceMetric {
                bytes: 19,
                utf16: 19,
            },
            HostSourceMetric {
                bytes: SOURCE.len() as u32,
                utf16: SOURCE.len() as u32,
            },
        ] {
            let affinity = if position.bytes == SOURCE.len() as u32 {
                HostMetricAffinity::Upstream
            } else {
                HostMetricAffinity::Downstream
            };
            let mut viewport = [0xa5; HOST_M11_VIEWPORT_BYTES];
            let outcome = host
                .query_structural(
                    query_for(snapshot.source, position, affinity, budget),
                    &mut viewport,
                )
                .expect("ordered-list initial structural query");
            let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
                panic!("ordered-list structural closure must be decodable: {outcome:?}");
            };
            assert_eq!(range, whole_source_range(snapshot.source));
            assert_eq!(receipt.encoded_bytes as usize, HOST_M11_VIEWPORT_BYTES);
            assert_eq!(read_u32(&viewport, 8), M11_ROLE_SCHEMA);
            assert_eq!(
                viewport[M11_VIEWPORT_HEADER_BYTES + 12],
                M11_ORDERED_LIST_VARIANT
            );
            assert_eq!(read_u32(&viewport, M11_VIEWPORT_HEADER_BYTES + 76), 7);
        }
        close_host(&mut host);
    }

    #[test]
    fn references_delta_requires_fresh_target_exact_ack_and_exact_record_accounting() {
        let document = [401, 402, 403, 404];
        let base = snapshot_with_reference(document, [411, 412, 413, 414], 1, 1);
        let target = snapshot_with_reference(document, [421, 422, 423, 424], 2, 2);
        let mut host = host_for(document);
        let base_ack = install(&mut host, &base);
        host.acknowledge_delivery(base_ack)
            .expect("acknowledge exact base");

        let references = host
            .role_record_count(M11HostRole::References)
            .expect("base References count");
        assert_eq!(references, 1);
        host.observe_source_version(target.source)
            .expect("observe target source");
        let mut delta = target.offer;
        delta.mode = PublicationMode::ExactBaseReferencesDelta;
        delta.base_ack = Some(base_ack);
        delta.transferred_record_count = delta
            .target_record_count
            .checked_sub(u32::try_from(references).expect("References count"))
            .expect("target contains reused References");

        let mut wrong_ack = delta;
        wrong_ack.base_ack.as_mut().expect("base").manifest_digest[0] ^= 1;
        let error = host.begin_offer(wrong_ack).expect_err("wrong base ACK");
        assert_eq!(error.reason(), HostRejectReason::BaseMismatch);

        let mut stale_identity = delta;
        stale_identity.publication_session = base_ack.publication_session;
        let error = host
            .begin_offer(stale_identity)
            .expect_err("target identity must be fresh");
        assert_eq!(error.reason(), HostRejectReason::BaseMismatch);

        let mut wrong_count = delta;
        wrong_count.transferred_record_count -= 1;
        let error = host
            .begin_offer(wrong_count)
            .expect_err("record accounting must be exact");
        assert_eq!(error.reason(), HostRejectReason::BaseMismatch);

        host.begin_offer(delta).expect("exact References delta");
        host.abort_offer(delta.offer_id)
            .expect("abort staged delta");
        assert!(matches!(
            host.poll(HostWorkGrant {
                inspect_bytes: 0,
                copy_bytes: 0,
                transitions: 1,
            })
            .expect("poll abort"),
            HostPollOutcome::AbortComplete { .. }
        ));
        assert_eq!(
            host.installed_ack,
            Some(base_ack),
            "validation and cancellation preserve the installed base"
        );
        close_host(&mut host);
    }

    /// Installs without granting any idle host polls between revisions.
    ///
    /// A pending frame may spend its transition grant retiring the previous
    /// installed root before returning transfer credit. This is the production
    /// shape when edits arrive continuously rather than leaving an idle gap
    /// after every commit.
    fn install_back_to_back(
        host: &mut NativeCandidateHost,
        snapshot: &TestSnapshot,
    ) -> (StructuralAck, usize) {
        host.observe_source_version(snapshot.source)
            .expect("observe exact source");
        host.begin_offer(snapshot.offer).expect("begin offer");
        let mut reclaim_polls = 0;
        for frame in &snapshot.frames {
            let encoded = packet_bytes(std::slice::from_ref(frame));
            admit_packet_bytes(host, &encoded);
            loop {
                match host
                    .poll(HostWorkGrant {
                        inspect_bytes: M11_HOST_MAXIMUM_FRAME_BYTES as u32
                            + PACKET_FRAME_DESCRIPTOR_BYTES as u32,
                        copy_bytes: M11_HOST_MAXIMUM_FRAME_BYTES as u32,
                        transitions: 1,
                    })
                    .expect("back-to-back frame poll")
                {
                    HostPollOutcome::Pending => reclaim_polls += 1,
                    HostPollOutcome::PacketCredit { offer_id, .. } => {
                        assert_eq!(offer_id, snapshot.offer.offer_id);
                        break;
                    }
                    outcome => panic!("unexpected frame outcome: {outcome:?}"),
                }
            }
        }
        host.request_commit(snapshot.commit)
            .expect("back-to-back commit request");
        loop {
            match host
                .poll(HostWorkGrant {
                    inspect_bytes: 0,
                    copy_bytes: 0,
                    transitions: 256,
                })
                .expect("back-to-back install poll")
            {
                HostPollOutcome::Pending => {}
                HostPollOutcome::Committed(ack) => return (ack, reclaim_polls),
                outcome => panic!("unexpected install outcome: {outcome:?}"),
            }
        }
    }

    fn host_for(document_session: Id128) -> NativeCandidateHost {
        NativeCandidateHost::new(HostConfig {
            document_session,
            grammar_revision: 1,
            syntax_profile: 1,
            authority_mask: 0x1f,
            maximum_query_bytes: 64 * 1024,
        })
        .expect("host")
    }

    fn query_for(
        source_version: SourceVersion,
        position: HostSourceMetric,
        affinity: HostMetricAffinity,
        budget: HostQueryBudget,
    ) -> HostPointQuery {
        HostPointQuery {
            source_version,
            position,
            affinity,
            budget,
        }
    }

    fn block_range_budget(
        host: &NativeCandidateHost,
        maximum_encoded_bytes: u32,
        maximum_block_count: u32,
        maximum_storage_pages_visited: u32,
    ) -> HostBlockRangeBudget {
        let (engine, installed) = host.query_root().expect("installed query root");
        let descriptor = engine
            .persistent_block_descriptor(installed)
            .expect("persistent block descriptor")
            .expect("persistent block roles");
        HostBlockRangeBudget {
            maximum_encoded_bytes,
            maximum_block_count,
            maximum_storage_pages_visited,
            maximum_open_depth: u32::from(descriptor.tree_height()).max(1),
            maximum_tree_nodes_visited: u32::try_from(
                descriptor
                    .maximum_tree_nodes_visited()
                    .checked_add(
                        descriptor
                            .maximum_consecutive_visit_node_headers(maximum_storage_pages_visited),
                    )
                    .expect("range tree-work bound"),
            )
            .expect("range tree-work fits u32"),
        }
    }

    fn block_range_query(
        source_version: SourceVersion,
        requested_range: HostMetricRange,
        budget: HostBlockRangeBudget,
        continuation: Option<HostBlockRangeContinuation>,
    ) -> HostBlockRangeQuery {
        HostBlockRangeQuery {
            source_version,
            requested_range,
            budget,
            continuation,
        }
    }

    fn structural_ordinal_window_budget(
        host: &NativeCandidateHost,
        maximum_entries: u32,
    ) -> HostStructuralOrdinalWindowBudget {
        let (engine, installed) = host.query_root().expect("installed query root");
        let descriptor = engine
            .persistent_block_descriptor(installed)
            .expect("persistent block descriptor")
            .expect("persistent block roles");
        HostStructuralOrdinalWindowBudget {
            maximum_entries,
            maximum_storage_pages_visited: 2,
            maximum_tree_nodes_visited: u32::try_from(
                descriptor
                    .maximum_tree_nodes_visited()
                    .checked_mul(2)
                    .expect("ordinal-window tree bound"),
            )
            .expect("ordinal-window tree bound fits u32"),
            maximum_packed_entries_inspected: descriptor
                .maximum_entries_scanned()
                .checked_mul(14)
                .expect("ordinal-window packed-entry bound"),
        }
    }

    fn structural_ordinal_window_query(
        source_version: SourceVersion,
        start_entry_ordinal: u64,
        budget: HostStructuralOrdinalWindowBudget,
    ) -> HostStructuralOrdinalWindowQuery {
        HostStructuralOrdinalWindowQuery {
            source_version,
            start_entry_ordinal,
            budget,
        }
    }

    const FULL_QUERY_BUDGET: HostQueryBudget = HostQueryBudget {
        maximum_encoded_bytes: 4 * 1024,
        maximum_open_depth: 16,
        maximum_leaf_count: 64,
        maximum_tree_nodes_visited: 256,
    };

    fn persistent_query_plan(
        host: &NativeCandidateHost,
    ) -> (M11HostInlineProjectionDescriptor, HostQueryBudget, usize) {
        let (engine, installed) = host.query_root().expect("installed query root");
        let descriptor = engine
            .persistent_inline_projection_descriptor(installed)
            .expect("persistent Projection descriptor")
            .expect("schema-v2 Projection");
        let encoded_bytes = M11_VIEWPORT_INLINE_HEADER_BYTES
            + M11_GREEN_RECORD_BYTES
            + M11_PROJECTION_RECORD_BYTES
            + M11_INLINE_META_RECORD_BYTES
            + usize::try_from(descriptor.fact_count()).expect("fact count")
                * M11_INLINE_FACT_RECORD_BYTES
            + usize::try_from(descriptor.link_value_encoded_bytes()).expect("link-value bytes");
        (
            descriptor,
            HostQueryBudget {
                maximum_encoded_bytes: u32::try_from(encoded_bytes).expect("viewport bytes"),
                maximum_open_depth: descriptor.maximum_open_depth(),
                maximum_leaf_count: u32::try_from(descriptor.logical_page_count() + 3)
                    .expect("leaf count"),
                maximum_tree_nodes_visited: u32::try_from(
                    descriptor.maximum_tree_nodes_visited() + 3,
                )
                .expect("tree-work bound"),
            },
            encoded_bytes,
        )
    }

    fn persistent_block_query_plan(
        host: &NativeCandidateHost,
    ) -> (M11HostPersistentBlockDescriptor, HostQueryBudget) {
        let (engine, installed) = host.query_root().expect("installed query root");
        let descriptor = engine
            .persistent_block_descriptor(installed)
            .expect("persistent block descriptor")
            .expect("persistent block roles");
        (
            descriptor,
            HostQueryBudget {
                maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                maximum_open_depth: u32::from(descriptor.tree_height()).max(1),
                maximum_leaf_count: descriptor.maximum_entries_scanned(),
                maximum_tree_nodes_visited: u32::try_from(descriptor.maximum_tree_nodes_visited())
                    .expect("tree-work bound"),
            },
        )
    }

    fn close_host(host: &mut NativeCandidateHost) {
        host.begin_close().expect("begin host close");
        loop {
            match host
                .poll(HostWorkGrant {
                    inspect_bytes: 0,
                    copy_bytes: 0,
                    transitions: 256,
                })
                .expect("host close poll")
            {
                HostPollOutcome::Pending => {}
                HostPollOutcome::Closed => return,
                outcome => panic!("unexpected close outcome: {outcome:?}"),
            }
        }
    }

    #[test]
    fn packet_cursor_consumes_only_whole_fuelled_frames_and_credits_once() {
        let document = [91, 92, 93, 94];
        let snapshot = snapshot(document, [101, 102, 103, 104], 1, 1, 2);
        assert!(
            snapshot.frames.len() >= 3,
            "test needs a multi-frame packet"
        );
        let mut host = host_for(document);
        host.observe_source_version(snapshot.source)
            .expect("observe exact source");
        host.begin_offer(snapshot.offer).expect("begin offer");

        let mut encoded = packet_bytes(&snapshot.frames);
        admit_packet_bytes(&mut host, &encoded);
        // Admission made the sole bounded ownership copy. The transport buffer
        // can be immediately reused without affecting the retained packet.
        encoded.fill(0xa5);

        let first_bytes = u32::try_from(snapshot.frames[0].bytes.len()).expect("first frame");
        assert_eq!(
            host.poll(HostWorkGrant {
                inspect_bytes: PACKET_FRAME_DESCRIPTOR_BYTES as u32 - 1,
                copy_bytes: first_bytes,
                transitions: 1,
            })
            .expect("sub-descriptor packet poll"),
            HostPollOutcome::Pending
        );
        assert_eq!(
            host.active
                .as_ref()
                .and_then(|active| active.pending_packet.as_ref())
                .expect("unread packet")
                .next_index,
            0,
            "a grant below one descriptor cannot inspect or advance it"
        );
        assert_eq!(
            host.poll(HostWorkGrant {
                inspect_bytes: first_bytes + PACKET_FRAME_DESCRIPTOR_BYTES as u32,
                copy_bytes: first_bytes,
                transitions: 1,
            })
            .expect("first packet poll"),
            HostPollOutcome::Pending,
            "a partial packet must not return transfer credit"
        );
        let active = host.active.as_ref().expect("active offer");
        assert_eq!(active.next_frame_ordinal, 1);
        assert_eq!(
            active
                .pending_packet
                .as_ref()
                .expect("retained packet")
                .next_index,
            1
        );

        let second_bytes =
            u32::try_from(snapshot.frames[1].bytes.len()).expect("second frame bytes");
        assert_eq!(
            host.poll(HostWorkGrant {
                inspect_bytes: second_bytes + PACKET_FRAME_DESCRIPTOR_BYTES as u32 - 1,
                copy_bytes: second_bytes,
                transitions: 8,
            })
            .expect("undersized packet poll"),
            HostPollOutcome::Pending
        );
        assert_eq!(
            host.active
                .as_ref()
                .and_then(|active| active.pending_packet.as_ref())
                .expect("retained cursor")
                .next_index,
            1,
            "a frame cursor must not advance until the whole frame fits"
        );

        assert_eq!(
            host.poll(HostWorkGrant {
                inspect_bytes: MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES
                    + MAXIMUM_PACKET_FRAME_COUNT * PACKET_FRAME_DESCRIPTOR_BYTES as u32,
                copy_bytes: MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES,
                transitions: MAXIMUM_PACKET_FRAME_COUNT,
            })
            .expect("complete packet poll"),
            HostPollOutcome::PacketCredit {
                offer_id: snapshot.offer.offer_id,
                next_frame_ordinal: u32::try_from(snapshot.frames.len()).expect("frame count"),
            }
        );
        assert!(
            host.active
                .as_ref()
                .is_some_and(|active| active.pending_packet.is_none()),
            "packet storage must release exactly at credit"
        );
        host.request_commit(snapshot.commit).expect("packet commit");
        poll_installed(&mut host);
        close_host(&mut host);
    }

    #[test]
    fn oversized_packet_frame_rejects_on_descriptor_with_normal_poll_grant() {
        const NORMAL_POLL_BYTES: u32 = 16 * 1024;

        let document = [95, 96, 97, 98];
        let mut snapshot = snapshot(document, [105, 106, 107, 108], 1, 1, 2);
        let oversized_frame_bytes = NORMAL_POLL_BYTES + 1;
        snapshot.offer.limits.maximum_encoded_frame_bytes = oversized_frame_bytes;

        let oversized_frame = TestFrame {
            offer_id: snapshot.offer.offer_id,
            ordinal: 0,
            first_record_ordinal: 0,
            record_count: 0,
            digest: [0; 4],
            bytes: vec![0xa5; oversized_frame_bytes as usize].into_boxed_slice(),
        };
        let encoded = packet_bytes(&[oversized_frame]);

        let mut host = host_for(document);
        host.observe_source_version(snapshot.source)
            .expect("observe exact source");
        host.begin_offer(snapshot.offer).expect("begin offer");
        admit_packet_bytes(&mut host, &encoded);

        let error = host
            .poll(HostWorkGrant {
                inspect_bytes: NORMAL_POLL_BYTES,
                copy_bytes: NORMAL_POLL_BYTES,
                transitions: 1,
            })
            .expect_err("oversized descriptor must reject instead of remaining pending");
        assert_eq!(error.reason(), HostRejectReason::CorruptPublication);
        let active = host.active.as_ref().expect("failed offer retained");
        assert_eq!(active.phase, OfferPhase::Failed);
        assert!(active.pending_packet.is_none());
        close_host(&mut host);
    }

    #[test]
    fn packet_storage_reserve_failure_is_typed() {
        let mut storage = Vec::new();
        let error = reserve_packet_storage(&mut storage, usize::MAX)
            .expect_err("impossible packet capacity must be rejected");

        assert_eq!(error.reason(), HostRejectReason::AllocationFailed);
        assert!(storage.is_empty());
    }

    #[test]
    fn packet_completion_rejects_header_aggregate_that_descriptors_do_not_prove() {
        let document = [111, 112, 113, 114];
        let snapshot = snapshot(document, [121, 122, 123, 124], 1, 1, 2);
        let mut host = host_for(document);
        host.observe_source_version(snapshot.source)
            .expect("observe exact source");
        host.begin_offer(snapshot.offer).expect("begin offer");

        let mut encoded = packet_bytes(&snapshot.frames);
        let aggregate_record_offset = PACKET_HEADER_BYTES - 8;
        let aggregate_records = read_u32(&encoded, aggregate_record_offset);
        assert!(aggregate_records > 0);
        encoded[aggregate_record_offset..aggregate_record_offset + 4]
            .copy_from_slice(&(aggregate_records - 1).to_le_bytes());
        admit_packet_bytes(&mut host, &encoded);

        let error = host
            .poll(HostWorkGrant {
                inspect_bytes: MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES
                    + MAXIMUM_PACKET_FRAME_COUNT * PACKET_FRAME_DESCRIPTOR_BYTES as u32,
                copy_bytes: MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES,
                transitions: MAXIMUM_PACKET_FRAME_COUNT,
            })
            .expect_err("descriptor aggregate must match the fixed header");
        assert_eq!(error.reason(), HostRejectReason::CorruptPublication);
        let active = host.active.as_ref().expect("failed offer retained");
        assert_eq!(active.phase, OfferPhase::Failed);
        assert!(active.pending_packet.is_none());
        close_host(&mut host);
    }

    #[test]
    fn corrupt_mid_packet_aborts_staging_but_preserves_installed_queries() {
        let document = [131, 132, 133, 134];
        let baseline = snapshot(document, [141, 142, 143, 144], 1, 1, 1);
        let replacement = snapshot(document, [151, 152, 153, 154], 1, 2, 2);
        assert!(replacement.frames.len() >= 3);
        let mut host = host_for(document);
        let installed = install(&mut host, &baseline);
        host.observe_source_version(replacement.source)
            .expect("same exact source");
        host.begin_offer(replacement.offer)
            .expect("replacement offer");

        let mut encoded = packet_bytes(&replacement.frames);
        let body_start =
            PACKET_HEADER_BYTES + replacement.frames.len() * PACKET_FRAME_DESCRIPTOR_BYTES;
        let second_end =
            body_start + replacement.frames[0].bytes.len() + replacement.frames[1].bytes.len();
        encoded[second_end - 1] ^= 0x80;
        admit_packet_bytes(&mut host, &encoded);

        let first_bytes =
            u32::try_from(replacement.frames[0].bytes.len()).expect("first frame bytes");
        assert_eq!(
            host.poll(HostWorkGrant {
                inspect_bytes: first_bytes + PACKET_FRAME_DESCRIPTOR_BYTES as u32,
                copy_bytes: first_bytes,
                transitions: 1,
            })
            .expect("first packet frame"),
            HostPollOutcome::Pending
        );
        let mut viewport = [0_u8; HOST_M11_VIEWPORT_BYTES];
        assert!(matches!(
            host.query_structural(
                query_for(
                    replacement.source,
                    HostSourceMetric::default(),
                    HostMetricAffinity::Downstream,
                    FULL_QUERY_BUDGET,
                ),
                &mut viewport,
            )
            .expect("query while replacement is staging"),
            HostStructuralQueryOutcome::Viewport { .. }
        ));

        let error = host
            .poll(HostWorkGrant {
                inspect_bytes: MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES
                    + MAXIMUM_PACKET_FRAME_COUNT * PACKET_FRAME_DESCRIPTOR_BYTES as u32,
                copy_bytes: MAXIMUM_PACKET_AGGREGATE_FRAME_BYTES,
                transitions: MAXIMUM_PACKET_FRAME_COUNT,
            })
            .expect_err("corrupt second frame must abort the packet");
        assert_eq!(error.reason(), HostRejectReason::CorruptPublication);
        assert_eq!(host.installed_ack, Some(installed));
        assert!(matches!(
            host.query_structural(
                query_for(
                    replacement.source,
                    HostSourceMetric::default(),
                    HostMetricAffinity::Downstream,
                    FULL_QUERY_BUDGET,
                ),
                &mut viewport,
            )
            .expect("installed query after failed replacement"),
            HostStructuralQueryOutcome::Viewport { .. }
        ));

        for _ in 0..256 {
            if !host.background_reclaim_pending {
                break;
            }
            assert_eq!(
                host.poll(HostWorkGrant {
                    inspect_bytes: 0,
                    copy_bytes: 0,
                    transitions: 1,
                })
                .expect("fuel failed staging reclaim"),
                HostPollOutcome::Pending
            );
        }
        assert!(!host.background_reclaim_pending);
        assert_eq!(host.installed_ack, Some(installed));
        close_host(&mut host);
    }

    #[test]
    fn product_host_reserves_two_roots_but_admits_only_131072_snapshot_nodes() {
        let document = [161, 162, 163, 164];
        let snapshot = snapshot(document, [171, 172, 173, 174], 1, 1, 1);
        let mut host = host_for(document);
        assert_eq!(host.engine_limits.arena_max_slots, 262_144);
        assert_eq!(
            host.engine_limits.arena_max_live_payload_bytes,
            128 * 1024 * 1024
        );
        assert_eq!(
            host.engine_limits.maximum_snapshot_nodes,
            M11_CANDIDATE_ARENA_MAX_SLOTS as u64
        );
        assert_eq!(
            host.engine_limits.maximum_snapshot_wire_bytes,
            512 * 1024 * 1024
        );
        host.observe_source_version(snapshot.source)
            .expect("observe exact source");

        let mut at_limit = snapshot.offer;
        at_limit.limits.maximum_frame_count = M11_CANDIDATE_ARENA_MAX_SLOTS as u32 + 2;
        at_limit.limits.maximum_encoded_frame_bytes = 512 * 1024 * 1024;
        host.begin_offer(at_limit)
            .expect("Begin plus 131072 Nodes plus End fits the product host");
        host.abort_offer(at_limit.offer_id)
            .expect("abort boundary offer");
        assert_eq!(
            host.poll(HostWorkGrant {
                inspect_bytes: 0,
                copy_bytes: 0,
                transitions: 1,
            })
            .expect("abort boundary offer"),
            HostPollOutcome::AbortComplete {
                offer_id: at_limit.offer_id,
            }
        );

        let mut over_limit = snapshot.offer;
        over_limit.limits.maximum_frame_count = M11_CANDIDATE_ARENA_MAX_SLOTS as u32 + 3;
        assert_eq!(
            host.begin_offer(over_limit)
                .expect_err("one offered snapshot cannot exceed 131072 Nodes")
                .reason(),
            HostRejectReason::ForegroundBoundExceeded
        );
        close_host(&mut host);
    }

    #[test]
    fn point_query_authors_exact_viewport_at_bof_interior_and_eof_for_both_affinities() {
        let document = [101, 102, 103, 104];
        let snapshot = snapshot_with_text(document, [111, 112, 113, 114], 7, 9, 1, "a😀b\n");
        assert_eq!(snapshot.source.utf8_length, 7);
        assert_eq!(snapshot.source.utf16_length, 5);
        let mut host = host_for(document);
        install(&mut host, &snapshot);

        for position in [
            HostSourceMetric { bytes: 0, utf16: 0 },
            HostSourceMetric { bytes: 1, utf16: 1 },
            HostSourceMetric { bytes: 5, utf16: 3 },
            HostSourceMetric { bytes: 7, utf16: 5 },
        ] {
            for affinity in [HostMetricAffinity::Upstream, HostMetricAffinity::Downstream] {
                let mut output = [0xa5; HOST_M11_VIEWPORT_BYTES];
                let outcome = host
                    .query_structural(
                        query_for(snapshot.source, position, affinity, FULL_QUERY_BUDGET),
                        &mut output,
                    )
                    .expect("point query");
                let HostStructuralQueryOutcome::Viewport {
                    source_version,
                    range,
                    receipt,
                } = outcome
                else {
                    panic!("admitted query must return a viewport: {outcome:?}");
                };
                assert_eq!(source_version, snapshot.source);
                assert_eq!(range, whole_source_range(snapshot.source));
                assert_eq!(
                    receipt,
                    HostViewportReceipt {
                        encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                        leaf_count: 2,
                        open_depth: 1,
                        tree_nodes_visited: 2,
                        summary_nodes_skipped: 0,
                    }
                );
                assert_eq!(&output[..8], VIEWPORT_MAGIC);
                assert_eq!(read_u32(&output, 8), M11_ROLE_SCHEMA);
                assert_eq!(read_u32(&output, 12), M11_GREEN_RECORD_BYTES as u32);
                assert_eq!(read_u32(&output, 16), M11_PROJECTION_RECORD_BYTES as u32);
                assert_eq!(&output[20..28], GREEN_MAGIC);
                assert_eq!(&output[100..108], PROJECTION_MAGIC);
            }
        }
        close_host(&mut host);
    }

    #[test]
    fn persistent_block_range_is_half_open_and_preserves_unicode_crlf_metrics() {
        let document = [581, 582, 583, 584];
        let snapshot = persistent_block_snapshot(
            document,
            [591, 592, 593, 594],
            1,
            1,
            "é\r\n\nz",
            vec![
                block_paragraph(4, 3),
                M11BlockSequenceEntry::blank(1, 1).expect("blank"),
                block_paragraph(1, 1),
            ],
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let requested = HostMetricRange {
            start: HostSourceMetric { bytes: 2, utf16: 1 },
            end: HostSourceMetric { bytes: 5, utf16: 4 },
        };
        let budget = block_range_budget(
            &host,
            (HOST_BLOCK_RANGE_HEADER_BYTES + 8 * HOST_BLOCK_RANGE_RECORD_BYTES) as u32,
            8,
            8,
        );
        let mut output = [0xa5; HOST_BLOCK_RANGE_HEADER_BYTES + 8 * HOST_BLOCK_RANGE_RECORD_BYTES];
        let HostBlockRangeOutcome::Page {
            covered_range,
            continuation,
            receipt,
            ..
        } = host
            .query_structural_range(
                block_range_query(snapshot.source, requested, budget, None),
                &mut output,
            )
            .expect("Unicode/CRLF range query")
        else {
            panic!("Unicode/CRLF range must be materialized");
        };
        assert_eq!(
            covered_range,
            HostMetricRange {
                start: HostSourceMetric::default(),
                end: HostSourceMetric { bytes: 5, utf16: 4 },
            }
        );
        assert!(receipt.complete);
        assert_eq!(receipt.block_count, 2);
        assert!(continuation.is_none());
        assert_eq!(&output[..8], BLOCK_RANGE_MAGIC);
        assert_eq!(read_u32(&output, 8), HOST_BLOCK_RANGE_SCHEMA);
        assert_eq!(read_u32(&output, 12), HOST_BLOCK_RANGE_HEADER_BYTES as u32);
        assert_eq!(read_u32(&output, 16), HOST_BLOCK_RANGE_RECORD_BYTES as u32);
        assert_eq!(read_u32(&output, 20), 2);
        assert_eq!(read_u32(&output, 24), HOST_BLOCK_RANGE_COMPLETE_FLAG);
        assert_eq!(read_u64(&output, HOST_BLOCK_RANGE_HEADER_BYTES), 0);
        assert_eq!(read_u32(&output, HOST_BLOCK_RANGE_HEADER_BYTES + 8), 0);
        assert_eq!(read_u32(&output, HOST_BLOCK_RANGE_HEADER_BYTES + 12), 0);
        assert_eq!(read_u32(&output, HOST_BLOCK_RANGE_HEADER_BYTES + 16), 4);
        assert_eq!(read_u32(&output, HOST_BLOCK_RANGE_HEADER_BYTES + 20), 3);
        let second = HOST_BLOCK_RANGE_HEADER_BYTES + HOST_BLOCK_RANGE_RECORD_BYTES;
        assert_eq!(read_u64(&output, second), 1);
        assert_eq!(read_u32(&output, second + 8), 4);
        assert_eq!(read_u32(&output, second + 12), 3);
        assert_eq!(read_u32(&output, second + 16), 5);
        assert_eq!(read_u32(&output, second + 20), 4);
        close_host(&mut host);
    }

    #[test]
    fn persistent_block_range_pages_sixty_five_plus_blocks_with_exact_resume() {
        const BLOCKS: usize = 70;
        const FIRST_PAGE_BLOCKS: usize = 65;
        let document = [561, 562, 563, 564];
        let snapshot = persistent_block_snapshot(
            document,
            [571, 572, 573, 574],
            1,
            1,
            &"x".repeat(BLOCKS),
            (0..BLOCKS).map(|_| block_paragraph(1, 1)).collect(),
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let requested = whole_source_range(snapshot.source);
        let budget = block_range_budget(
            &host,
            (HOST_BLOCK_RANGE_HEADER_BYTES + FIRST_PAGE_BLOCKS * HOST_BLOCK_RANGE_RECORD_BYTES)
                as u32,
            FIRST_PAGE_BLOCKS as u32,
            8,
        );
        let mut first = [0_u8;
            HOST_BLOCK_RANGE_HEADER_BYTES + FIRST_PAGE_BLOCKS * HOST_BLOCK_RANGE_RECORD_BYTES];
        let HostBlockRangeOutcome::Page {
            covered_range: first_coverage,
            continuation: Some(continuation),
            receipt: first_receipt,
            ..
        } = host
            .query_structural_range(
                block_range_query(snapshot.source, requested, budget, None),
                &mut first,
            )
            .expect("first range page")
        else {
            panic!("first range page must truncate with a continuation");
        };
        assert!(!first_receipt.complete);
        assert_eq!(first_receipt.block_count, FIRST_PAGE_BLOCKS as u32);
        assert_eq!(first_coverage.start, HostSourceMetric::default());
        assert_eq!(
            first_coverage.end,
            HostSourceMetric {
                bytes: FIRST_PAGE_BLOCKS as u32,
                utf16: FIRST_PAGE_BLOCKS as u32,
            }
        );
        assert_eq!(read_u32(&first, 24), 0);
        let last_first =
            HOST_BLOCK_RANGE_HEADER_BYTES + (FIRST_PAGE_BLOCKS - 1) * HOST_BLOCK_RANGE_RECORD_BYTES;
        assert_eq!(read_u64(&first, last_first), 64);

        let mut second = [0_u8;
            HOST_BLOCK_RANGE_HEADER_BYTES + FIRST_PAGE_BLOCKS * HOST_BLOCK_RANGE_RECORD_BYTES];
        let HostBlockRangeOutcome::Page {
            covered_range: second_coverage,
            continuation: None,
            receipt: second_receipt,
            ..
        } = host
            .query_structural_range(
                block_range_query(snapshot.source, requested, budget, Some(continuation)),
                &mut second,
            )
            .expect("second range page")
        else {
            panic!("second range page must complete the request");
        };
        assert!(second_receipt.complete);
        assert_eq!(
            second_receipt.block_count,
            (BLOCKS - FIRST_PAGE_BLOCKS) as u32
        );
        assert_eq!(second_coverage.start, first_coverage.end);
        assert_eq!(second_coverage.end, requested.end);
        assert_eq!(read_u64(&second, HOST_BLOCK_RANGE_HEADER_BYTES), 65);
        assert_eq!(read_u32(&second, 24), HOST_BLOCK_RANGE_COMPLETE_FLAG);
        close_host(&mut host);
    }

    #[test]
    fn persistent_block_range_rejects_stale_or_tampered_resume_without_authority() {
        let document = [541, 542, 543, 544];
        let snapshot = persistent_block_snapshot(
            document,
            [551, 552, 553, 554],
            1,
            1,
            "xxx",
            (0..3).map(|_| block_paragraph(1, 1)).collect(),
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let requested = whole_source_range(snapshot.source);
        let budget = block_range_budget(
            &host,
            (HOST_BLOCK_RANGE_HEADER_BYTES + HOST_BLOCK_RANGE_RECORD_BYTES) as u32,
            1,
            4,
        );
        let mut first = [0_u8; HOST_BLOCK_RANGE_HEADER_BYTES + HOST_BLOCK_RANGE_RECORD_BYTES];
        let HostBlockRangeOutcome::Page {
            continuation: Some(continuation),
            ..
        } = host
            .query_structural_range(
                block_range_query(snapshot.source, requested, budget, None),
                &mut first,
            )
            .expect("bounded first page")
        else {
            panic!("one-block page must return a continuation");
        };

        let mut stale = continuation.encoded();
        stale[16] ^= 1;
        let mut untouched = [0xa5; HOST_BLOCK_RANGE_HEADER_BYTES + HOST_BLOCK_RANGE_RECORD_BYTES];
        let error = host
            .query_structural_range(
                block_range_query(
                    snapshot.source,
                    requested,
                    budget,
                    Some(HostBlockRangeContinuation::from_encoded(stale)),
                ),
                &mut untouched,
            )
            .expect_err("foreign publication continuation");
        assert_eq!(error.reason(), HostRejectReason::ExactSourceMismatch);
        assert_eq!(
            untouched,
            [0xa5; HOST_BLOCK_RANGE_HEADER_BYTES + HOST_BLOCK_RANGE_RECORD_BYTES]
        );

        let mut tampered = continuation.encoded();
        tampered[40] ^= 1;
        let mut no_authority =
            [0xa5; HOST_BLOCK_RANGE_HEADER_BYTES + HOST_BLOCK_RANGE_RECORD_BYTES];
        assert!(host
            .query_structural_range(
                block_range_query(
                    snapshot.source,
                    requested,
                    budget,
                    Some(HostBlockRangeContinuation::from_encoded(tampered)),
                ),
                &mut no_authority,
            )
            .is_err());
        assert_eq!(
            no_authority,
            [0xa5; HOST_BLOCK_RANGE_HEADER_BYTES + HOST_BLOCK_RANGE_RECORD_BYTES]
        );

        let (engine, installed) = host.query_root().expect("installed query root");
        let descriptor = engine
            .persistent_block_descriptor(installed)
            .expect("persistent block descriptor")
            .expect("persistent block roles");
        let initial_tree_bound =
            descriptor
                .maximum_tree_nodes_visited()
                .checked_add(descriptor.maximum_consecutive_visit_node_headers(
                    budget.maximum_storage_pages_visited - 1,
                ))
                .expect("initial range tree bound");
        let too_little_tree = HostBlockRangeBudget {
            maximum_tree_nodes_visited: u32::try_from(initial_tree_bound - 1)
                .expect("tree bound fits u32"),
            ..budget
        };
        let mut gap_output = [0xa5; HOST_BLOCK_RANGE_HEADER_BYTES + HOST_BLOCK_RANGE_RECORD_BYTES];
        let HostBlockRangeOutcome::SourceGap {
            receipt, reason, ..
        } = host
            .query_structural_range(
                block_range_query(snapshot.source, requested, too_little_tree, None),
                &mut gap_output,
            )
            .expect("tree preflight gap")
        else {
            panic!("insufficient tree budget must be a source gap");
        };
        assert_eq!(reason, HostSourceGapReason::TreeNodeLimit);
        assert_eq!(receipt.encoded_bytes, 0);
        assert_eq!(receipt.block_count, 0);
        assert!(!receipt.complete);
        assert_eq!(
            gap_output,
            [0xa5; HOST_BLOCK_RANGE_HEADER_BYTES + HOST_BLOCK_RANGE_RECORD_BYTES]
        );

        let point_only_page_budget = HostBlockRangeBudget {
            maximum_storage_pages_visited: 1,
            ..budget
        };
        let mut page_gap_output =
            [0xa5; HOST_BLOCK_RANGE_HEADER_BYTES + HOST_BLOCK_RANGE_RECORD_BYTES];
        let HostBlockRangeOutcome::SourceGap {
            receipt, reason, ..
        } = host
            .query_structural_range(
                block_range_query(snapshot.source, requested, point_only_page_budget, None),
                &mut page_gap_output,
            )
            .expect("point plus visitor page preflight gap")
        else {
            panic!("one initial storage-page credit cannot also visit a range page");
        };
        assert_eq!(reason, HostSourceGapReason::LeafLimit);
        assert_eq!(receipt, HostBlockRangeReceipt::default());
        assert_eq!(
            page_gap_output,
            [0xa5; HOST_BLOCK_RANGE_HEADER_BYTES + HOST_BLOCK_RANGE_RECORD_BYTES]
        );
        close_host(&mut host);
    }

    #[test]
    fn persistent_block_range_large_document_work_remains_local() {
        const BLOCKS: usize = 2048;
        const START: u32 = 1800;
        const VISIBLE: u32 = 64;
        let document = [521, 522, 523, 524];
        let snapshot = persistent_block_snapshot(
            document,
            [531, 532, 533, 534],
            1,
            1,
            &"x".repeat(BLOCKS),
            (0..BLOCKS).map(|_| block_paragraph(1, 1)).collect(),
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let requested = HostMetricRange {
            start: HostSourceMetric {
                bytes: START,
                utf16: START,
            },
            end: HostSourceMetric {
                bytes: START + VISIBLE,
                utf16: START + VISIBLE,
            },
        };
        let budget = block_range_budget(
            &host,
            (HOST_BLOCK_RANGE_HEADER_BYTES + VISIBLE as usize * HOST_BLOCK_RANGE_RECORD_BYTES)
                as u32,
            VISIBLE,
            8,
        );
        let mut output = [0_u8;
            HOST_BLOCK_RANGE_HEADER_BYTES + VISIBLE as usize * HOST_BLOCK_RANGE_RECORD_BYTES];
        let HostBlockRangeOutcome::Page {
            covered_range,
            receipt,
            continuation,
            ..
        } = host
            .query_structural_range(
                block_range_query(snapshot.source, requested, budget, None),
                &mut output,
            )
            .expect("large-document local range")
        else {
            panic!("large-document local range must materialize");
        };
        assert_eq!(covered_range, requested);
        assert_eq!(receipt.block_count, VISIBLE);
        assert!(receipt.complete);
        assert!(continuation.is_none());
        assert_eq!(
            receipt.encoded_bytes,
            (HOST_BLOCK_RANGE_HEADER_BYTES + VISIBLE as usize * HOST_BLOCK_RANGE_RECORD_BYTES)
                as u32
        );
        assert!(receipt.storage_pages_visited <= 8);
        assert!(receipt.tree_nodes_visited < BLOCKS as u32);
        assert!(receipt.packed_entries_inspected < BLOCKS as u32);
        assert_eq!(
            read_u64(&output, HOST_BLOCK_RANGE_HEADER_BYTES),
            u64::from(START)
        );
        close_host(&mut host);
    }

    #[test]
    fn structural_ordinal_windows_cover_boundaries_authority_and_preflight_failures() {
        const ENTRIES: usize = 130;
        let document = [1501, 1502, 1503, 1504];
        let entries = (0..ENTRIES)
            .map(|ordinal| {
                if ordinal % 2 == 0 {
                    block_paragraph(1, 1)
                } else {
                    M11BlockSequenceEntry::blank(1, 1).expect("blank")
                }
            })
            .collect();
        let structural_snapshot = persistent_block_snapshot(
            document,
            [1511, 1512, 1513, 1514],
            1,
            1,
            &"x".repeat(ENTRIES),
            entries,
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &structural_snapshot);
        let budget = structural_ordinal_window_budget(&host, 10);

        for (start_ordinal, expected_next, complete) in
            [(0_u64, 10_u64, false), (65, 75, false), (129, 130, true)]
        {
            let HostStructuralOrdinalWindowOutcome::Window {
                total_entry_count,
                start_entry_ordinal,
                next_entry_ordinal,
                start,
                next,
                complete: actual_complete,
                receipt,
                ..
            } = host
                .query_structural_ordinal_window(structural_ordinal_window_query(
                    structural_snapshot.source,
                    start_ordinal,
                    budget,
                ))
                .expect("ordinal-window query")
            else {
                panic!("valid ordinal window must succeed");
            };
            assert_eq!(total_entry_count, ENTRIES as u64);
            assert_eq!(start_entry_ordinal, start_ordinal);
            assert_eq!(next_entry_ordinal, expected_next);
            assert_eq!(start.bytes, start_ordinal as u32);
            assert_eq!(start.utf16, start_ordinal as u32);
            assert_eq!(next.bytes, expected_next as u32);
            assert_eq!(next.utf16, expected_next as u32);
            assert_eq!(actual_complete, complete);
            assert!(receipt.storage_pages_visited <= budget.maximum_storage_pages_visited);
            assert!(receipt.tree_nodes_visited <= budget.maximum_tree_nodes_visited);
            assert!(receipt.packed_entries_inspected <= budget.maximum_packed_entries_inspected);
        }

        let HostStructuralOrdinalWindowOutcome::Window {
            start,
            next,
            complete,
            receipt,
            ..
        } = host
            .query_structural_ordinal_window(structural_ordinal_window_query(
                structural_snapshot.source,
                ENTRIES as u64,
                budget,
            ))
            .expect("terminal ordinal window")
        else {
            panic!("terminal ordinal is a valid empty window");
        };
        assert_eq!(start, next);
        assert_eq!(start.bytes, ENTRIES as u32);
        assert!(complete);
        assert_eq!(receipt, HostStructuralOrdinalWindowReceipt::default());

        let HostStructuralOrdinalWindowOutcome::Failure {
            total_entry_count,
            reason,
            receipt,
            ..
        } = host
            .query_structural_ordinal_window(structural_ordinal_window_query(
                structural_snapshot.source,
                ENTRIES as u64 + 1,
                budget,
            ))
            .expect("out-of-range typed failure")
        else {
            panic!("out-of-range ordinal must fail closed");
        };
        assert_eq!(total_entry_count, ENTRIES as u64);
        assert_eq!(
            reason,
            HostStructuralOrdinalWindowFailureReason::OrdinalOutOfRange
        );
        assert_eq!(receipt, HostStructuralOrdinalWindowReceipt::default());

        let middle = structural_ordinal_window_query(structural_snapshot.source, 65, budget);
        let budget_failures = [
            (
                HostStructuralOrdinalWindowBudget {
                    maximum_entries: 0,
                    ..budget
                },
                HostStructuralOrdinalWindowFailureReason::EntryWindowLimit,
            ),
            (
                HostStructuralOrdinalWindowBudget {
                    maximum_entries: HOST_STRUCTURAL_ORDINAL_WINDOW_MAXIMUM_ENTRIES + 1,
                    ..budget
                },
                HostStructuralOrdinalWindowFailureReason::EntryWindowLimit,
            ),
            (
                HostStructuralOrdinalWindowBudget {
                    maximum_storage_pages_visited: 1,
                    ..budget
                },
                HostStructuralOrdinalWindowFailureReason::StoragePageLimit,
            ),
            (
                HostStructuralOrdinalWindowBudget {
                    maximum_tree_nodes_visited: budget.maximum_tree_nodes_visited - 1,
                    ..budget
                },
                HostStructuralOrdinalWindowFailureReason::TreeNodeLimit,
            ),
            (
                HostStructuralOrdinalWindowBudget {
                    maximum_packed_entries_inspected: budget.maximum_packed_entries_inspected - 1,
                    ..budget
                },
                HostStructuralOrdinalWindowFailureReason::PackedEntryLimit,
            ),
        ];
        for (failing_budget, expected_reason) in budget_failures {
            let HostStructuralOrdinalWindowOutcome::Failure {
                total_entry_count,
                start_entry_ordinal,
                reason,
                receipt,
                ..
            } = host
                .query_structural_ordinal_window(HostStructuralOrdinalWindowQuery {
                    budget: failing_budget,
                    ..middle
                })
                .expect("typed budget failure")
            else {
                panic!("insufficient ordinal-window budget must fail closed");
            };
            assert_eq!(total_entry_count, ENTRIES as u64);
            assert_eq!(start_entry_ordinal, 65);
            assert_eq!(reason, expected_reason);
            assert_eq!(receipt, HostStructuralOrdinalWindowReceipt::default());
        }

        let stale = SourceVersion {
            revision: structural_snapshot.source.revision - 1,
            ..structural_snapshot.source
        };
        let stale_error = host
            .query_structural_ordinal_window(structural_ordinal_window_query(stale, 0, budget))
            .expect_err("older ordinal-window source");
        assert_eq!(stale_error.reason(), HostRejectReason::StaleSource);
        close_host(&mut host);

        let empty_document = [1521, 1522, 1523, 1524];
        let empty = persistent_block_snapshot(
            empty_document,
            [1531, 1532, 1533, 1534],
            1,
            1,
            "",
            vec![],
            false,
        );
        let mut empty_host = host_for(empty_document);
        install(&mut empty_host, &empty);
        let HostStructuralOrdinalWindowOutcome::Window {
            total_entry_count,
            start,
            next,
            complete,
            receipt,
            ..
        } = empty_host
            .query_structural_ordinal_window(structural_ordinal_window_query(
                empty.source,
                0,
                HostStructuralOrdinalWindowBudget {
                    maximum_entries: 10,
                    maximum_storage_pages_visited: 0,
                    maximum_tree_nodes_visited: 0,
                    maximum_packed_entries_inspected: 0,
                },
            ))
            .expect("empty ordinal window")
        else {
            panic!("empty structural publication must return an empty exact window");
        };
        assert_eq!(total_entry_count, 0);
        assert_eq!(start, HostSourceMetric::default());
        assert_eq!(next, HostSourceMetric::default());
        assert!(complete);
        assert_eq!(receipt, HostStructuralOrdinalWindowReceipt::default());
        close_host(&mut empty_host);

        let legacy_document = [1541, 1542, 1543, 1544];
        let legacy = snapshot(legacy_document, [1551, 1552, 1553, 1554], 1, 1, 1);
        let mut legacy_host = host_for(legacy_document);
        install(&mut legacy_host, &legacy);
        let HostStructuralOrdinalWindowOutcome::Failure {
            total_entry_count,
            reason,
            receipt,
            ..
        } = legacy_host
            .query_structural_ordinal_window(structural_ordinal_window_query(
                legacy.source,
                0,
                HostStructuralOrdinalWindowBudget {
                    maximum_entries: 10,
                    maximum_storage_pages_visited: 2,
                    maximum_tree_nodes_visited: 256,
                    maximum_packed_entries_inspected: 1024,
                },
            ))
            .expect("unavailable ordinal facts")
        else {
            panic!("legacy publication has no persistent ordinal facts");
        };
        assert_eq!(total_entry_count, 0);
        assert_eq!(
            reason,
            HostStructuralOrdinalWindowFailureReason::UnavailableFacts
        );
        assert_eq!(receipt, HostStructuralOrdinalWindowReceipt::default());
        close_host(&mut legacy_host);
    }

    #[test]
    fn persistent_block_range_default_quantum_survives_maximally_sparse_splice_pages() {
        const VISIBLE_BLOCKS: usize = 24;
        // A paragraph with the production 80-byte Green and 56-byte
        // Projection records packs 22 entries into one arena page.
        const ENTRIES_PER_INITIAL_PAGE: usize = 22;
        const DEFAULT_ENCODED_BYTES: u32 = 4 * 1024;
        const DEFAULT_AUTHENTICATED_PAGES: u32 = 25;
        const DEFAULT_OPEN_DEPTH: u32 = 16;
        const DEFAULT_TREE_NODES: u32 = 320;

        let source_len = VISIBLE_BLOCKS * ENTRIES_PER_INITIAL_PAGE;
        let document = [501, 502, 503, 504];
        let snapshot = persistent_block_snapshot_with_root_transform(
            document,
            [511, 512, 513, 514],
            1,
            1,
            &"x".repeat(source_len),
            (0..source_len).map(|_| block_paragraph(1, 1)).collect(),
            false,
            |runtime, mut root| {
                assert_eq!(
                    root.storage_page_count(),
                    VISIBLE_BLOCKS as u64,
                    "the dense base must start with one full leaf per splice region"
                );
                // Collapse each original packed leaf to one source-equivalent
                // block, from right to left so every semantic cut stays stable.
                // Exact-page replacements do not coalesce with neighbouring
                // retained leaves, producing the maximally sparse persistent
                // topology that disjoint localized edits can leave behind.
                for page in (0..VISIBLE_BLOCKS).rev() {
                    let start = (page * ENTRIES_PER_INITIAL_PAGE) as u64;
                    let end = start + ENTRIES_PER_INITIAL_PAGE as u64;
                    let target_lease = runtime
                        .snapshot_current_source()
                        .expect("same-source splice lease");
                    let replacement = [block_paragraph(
                        ENTRIES_PER_INITIAL_PAGE,
                        ENTRIES_PER_INITIAL_PAGE,
                    )];
                    let (next, receipt) = splice_m11_block_sequence_atomic(
                        runtime,
                        &root,
                        target_lease,
                        start..end,
                        &replacement,
                    )
                    .expect("disjoint sparse-leaf splice");
                    assert_eq!(receipt.deleted_entries(), ENTRIES_PER_INITIAL_PAGE as u64);
                    assert_eq!(receipt.replacement_entries(), 1);
                    root.begin_release(runtime)
                        .expect("release prior splice root");
                    while !root
                        .poll_release(runtime, 64)
                        .expect("poll prior splice root release")
                        .complete()
                    {}
                    drop(root);
                    root = next;
                }
                assert_eq!(root.entry_count(), VISIBLE_BLOCKS as u64);
                assert_eq!(
                    root.storage_page_count(),
                    VISIBLE_BLOCKS as u64,
                    "each visible block must occupy its own retained leaf"
                );
                root
            },
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (engine, installed) = host.query_root().expect("installed sparse query root");
        let descriptor = engine
            .persistent_block_descriptor(installed)
            .expect("sparse block descriptor")
            .expect("persistent sparse block roles");
        assert_eq!(descriptor.entry_count(), VISIBLE_BLOCKS as u64);
        assert_eq!(descriptor.storage_page_count(), VISIBLE_BLOCKS as u64);
        assert!(u32::from(descriptor.tree_height()) <= DEFAULT_OPEN_DEPTH);
        assert!(
            descriptor.maximum_tree_nodes_visited()
                + descriptor
                    .maximum_consecutive_visit_node_headers(DEFAULT_AUTHENTICATED_PAGES - 1,)
                <= u64::from(DEFAULT_TREE_NODES),
            "the public-equivalent default must preflight this fragmented tree"
        );

        let requested = whole_source_range(snapshot.source);
        let old_four_page_budget = HostBlockRangeBudget {
            maximum_encoded_bytes: DEFAULT_ENCODED_BYTES,
            maximum_block_count: VISIBLE_BLOCKS as u32,
            maximum_storage_pages_visited: 4,
            maximum_open_depth: DEFAULT_OPEN_DEPTH,
            maximum_tree_nodes_visited: DEFAULT_TREE_NODES,
        };
        let mut old_output = [0_u8; DEFAULT_ENCODED_BYTES as usize];
        let HostBlockRangeOutcome::Page {
            continuation: Some(_),
            receipt: old_receipt,
            ..
        } = host
            .query_structural_range(
                block_range_query(snapshot.source, requested, old_four_page_budget, None),
                &mut old_output,
            )
            .expect("old four-page range quantum")
        else {
            panic!("the old sparse-tree quantum must truncate with a continuation");
        };
        assert_eq!(
            old_receipt.block_count, 3,
            "one point-authentication plus three visitor pages exhausts the old budget"
        );
        assert_eq!(old_receipt.storage_pages_visited, 4);
        assert!(!old_receipt.complete);

        let default_budget = HostBlockRangeBudget {
            maximum_encoded_bytes: DEFAULT_ENCODED_BYTES,
            maximum_block_count: VISIBLE_BLOCKS as u32,
            maximum_storage_pages_visited: DEFAULT_AUTHENTICATED_PAGES,
            maximum_open_depth: DEFAULT_OPEN_DEPTH,
            maximum_tree_nodes_visited: DEFAULT_TREE_NODES,
        };
        let mut output = [0_u8; DEFAULT_ENCODED_BYTES as usize];
        let HostBlockRangeOutcome::Page {
            covered_range,
            continuation,
            receipt,
            ..
        } = host
            .query_structural_range(
                block_range_query(snapshot.source, requested, default_budget, None),
                &mut output,
            )
            .expect("default sparse-tree range quantum")
        else {
            panic!("the default sparse-tree quantum must return a page");
        };
        assert_eq!(covered_range, requested);
        assert_eq!(receipt.block_count, VISIBLE_BLOCKS as u32);
        assert_eq!(
            receipt.encoded_bytes,
            (HOST_BLOCK_RANGE_HEADER_BYTES + VISIBLE_BLOCKS * HOST_BLOCK_RANGE_RECORD_BYTES) as u32
        );
        assert_eq!(
            receipt.storage_pages_visited, DEFAULT_AUTHENTICATED_PAGES,
            "the receipt must include the initial point page and all 24 visited leaves"
        );
        assert!(receipt.tree_nodes_visited <= DEFAULT_TREE_NODES);
        assert!(receipt.complete);
        assert!(continuation.is_none());
        for ordinal in 0..VISIBLE_BLOCKS {
            let record = HOST_BLOCK_RANGE_HEADER_BYTES + ordinal * HOST_BLOCK_RANGE_RECORD_BYTES;
            assert_eq!(read_u64(&output, record), ordinal as u64);
            assert_eq!(
                read_u32(&output, record + 8),
                (ordinal * ENTRIES_PER_INITIAL_PAGE) as u32
            );
            assert_eq!(
                read_u32(&output, record + 16),
                ((ordinal + 1) * ENTRIES_PER_INITIAL_PAGE) as u32
            );
        }
        close_host(&mut host);
    }

    #[test]
    fn persistent_blocks_select_exact_paragraphs_and_blank_by_boundary_affinity() {
        let document = [601, 602, 603, 604];
        let snapshot = persistent_block_snapshot(
            document,
            [611, 612, 613, 614],
            1,
            1,
            "p\n\n**q**",
            vec![
                block_paragraph(2, 2),
                M11BlockSequenceEntry::blank(1, 1).expect("blank"),
                block_paragraph(5, 5),
            ],
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (descriptor, budget) = persistent_block_query_plan(&host);
        assert_eq!(descriptor.entry_count(), 3);
        assert_eq!(descriptor.storage_page_count(), 1);

        let cases = [
            (
                HostSourceMetric { bytes: 0, utf16: 0 },
                HostMetricAffinity::Downstream,
                0,
                2,
                1,
            ),
            (
                HostSourceMetric { bytes: 2, utf16: 2 },
                HostMetricAffinity::Upstream,
                0,
                2,
                1,
            ),
            (
                HostSourceMetric { bytes: 2, utf16: 2 },
                HostMetricAffinity::Downstream,
                2,
                3,
                2,
            ),
            (
                HostSourceMetric { bytes: 3, utf16: 3 },
                HostMetricAffinity::Upstream,
                2,
                3,
                2,
            ),
            (
                HostSourceMetric { bytes: 3, utf16: 3 },
                HostMetricAffinity::Downstream,
                3,
                8,
                1,
            ),
        ];
        for (position, affinity, start, end, variant) in cases {
            let mut output = [0xa5; HOST_M11_VIEWPORT_BYTES];
            let outcome = host
                .query_structural(
                    query_for(snapshot.source, position, affinity, budget),
                    &mut output,
                )
                .expect("persistent block point query");
            let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
                panic!("persistent block point must return a viewport: {outcome:?}");
            };
            assert_eq!(
                range,
                HostMetricRange {
                    start: HostSourceMetric {
                        bytes: start,
                        utf16: start,
                    },
                    end: HostSourceMetric {
                        bytes: end,
                        utf16: end,
                    },
                }
            );
            assert_eq!(&output[..8], VIEWPORT_MAGIC);
            assert_eq!(read_u32(&output, 8), M11_ROLE_SCHEMA);
            assert_eq!(output[M11_VIEWPORT_HEADER_BYTES + 12], variant);
            assert_eq!(
                output[M11_VIEWPORT_HEADER_BYTES + M11_GREEN_RECORD_BYTES + 12],
                variant
            );
            assert_eq!(
                read_u64(&output, M11_VIEWPORT_HEADER_BYTES + 16),
                u64::from(start)
            );
            assert_eq!(
                read_u64(&output, M11_VIEWPORT_HEADER_BYTES + 24),
                u64::from(end)
            );
            assert_eq!(
                read_u64(
                    &output,
                    M11_VIEWPORT_HEADER_BYTES + M11_GREEN_RECORD_BYTES + 16
                ),
                u64::from(start)
            );
            assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
            assert!(receipt.leaf_count >= 1);
            assert!(receipt.leaf_count <= descriptor.maximum_entries_scanned());
            assert!(
                receipt.tree_nodes_visited
                    <= u32::try_from(descriptor.maximum_tree_nodes_visited()).unwrap()
            );
            if variant == 2 {
                assert_eq!(
                    read_u32(&output, M11_VIEWPORT_HEADER_BYTES + 56),
                    1,
                    "blank coverage synthesizes the existing literal fallback reason"
                );
            }
        }
        close_host(&mut host);
    }

    #[test]
    fn persistent_blocks_preserve_unicode_crlf_dual_ranges_and_literal_kinds() {
        let document = [621, 622, 623, 624];
        let unicode = persistent_block_snapshot(
            document,
            [631, 632, 633, 634],
            1,
            1,
            "é\r\n\nz",
            vec![
                block_paragraph(4, 3),
                M11BlockSequenceEntry::blank(1, 1).expect("blank"),
                block_paragraph(1, 1),
            ],
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &unicode);
        let (_, budget) = persistent_block_query_plan(&host);
        for (position, affinity, expected) in [
            (
                HostSourceMetric { bytes: 4, utf16: 3 },
                HostMetricAffinity::Upstream,
                HostMetricRange {
                    start: HostSourceMetric { bytes: 0, utf16: 0 },
                    end: HostSourceMetric { bytes: 4, utf16: 3 },
                },
            ),
            (
                HostSourceMetric { bytes: 4, utf16: 3 },
                HostMetricAffinity::Downstream,
                HostMetricRange {
                    start: HostSourceMetric { bytes: 4, utf16: 3 },
                    end: HostSourceMetric { bytes: 5, utf16: 4 },
                },
            ),
            (
                HostSourceMetric { bytes: 5, utf16: 4 },
                HostMetricAffinity::Downstream,
                HostMetricRange {
                    start: HostSourceMetric { bytes: 5, utf16: 4 },
                    end: HostSourceMetric { bytes: 6, utf16: 5 },
                },
            ),
        ] {
            let mut output = [0; HOST_M11_VIEWPORT_BYTES];
            let HostStructuralQueryOutcome::Viewport { range, .. } = host
                .query_structural(
                    query_for(unicode.source, position, affinity, budget),
                    &mut output,
                )
                .expect("Unicode/CRLF block query")
            else {
                panic!("Unicode/CRLF block query must return a viewport");
            };
            assert_eq!(range, expected);
            assert_eq!(
                read_u64(&output, M11_VIEWPORT_HEADER_BYTES + 16),
                u64::from(expected.start.bytes)
            );
            assert_eq!(
                read_u64(&output, M11_VIEWPORT_HEADER_BYTES + 24),
                u64::from(expected.end.bytes)
            );
        }
        close_host(&mut host);

        let definitions_document = [641, 642, 643, 644];
        let definitions = persistent_block_snapshot(
            definitions_document,
            [651, 652, 653, 654],
            1,
            1,
            "[a]: /x\n",
            vec![M11BlockSequenceEntry::definitions_only(8, 8, 1).expect("definitions-only")],
            true,
        );
        let mut definitions_host = host_for(definitions_document);
        install(&mut definitions_host, &definitions);
        let (_, definitions_budget) = persistent_block_query_plan(&definitions_host);
        let mut output = [0; HOST_M11_VIEWPORT_BYTES];
        let HostStructuralQueryOutcome::Viewport { range, .. } = definitions_host
            .query_structural(
                query_for(
                    definitions.source,
                    HostSourceMetric::default(),
                    HostMetricAffinity::Downstream,
                    definitions_budget,
                ),
                &mut output,
            )
            .expect("definitions-only query")
        else {
            panic!("definitions-only coverage must be a literal viewport");
        };
        assert_eq!(range, whole_source_range(definitions.source));
        assert_eq!(output[M11_VIEWPORT_HEADER_BYTES + 12], 0);
        assert_eq!(read_u64(&output, M11_VIEWPORT_HEADER_BYTES + 48), 1);
        assert_eq!(
            read_u64(
                &output,
                M11_VIEWPORT_HEADER_BYTES + M11_GREEN_RECORD_BYTES + 48
            ),
            0
        );
        close_host(&mut definitions_host);

        let unsupported_document = [661, 662, 663, 664];
        let unsupported = persistent_block_snapshot(
            unsupported_document,
            [671, 672, 673, 674],
            1,
            1,
            "# h\n",
            vec![M11BlockSequenceEntry::unsupported(
                4,
                4,
                M11BlockUnsupportedReason::new(0x0002_0002).expect("ATX reason"),
            )
            .expect("unsupported")],
            false,
        );
        let mut unsupported_host = host_for(unsupported_document);
        install(&mut unsupported_host, &unsupported);
        let (_, unsupported_budget) = persistent_block_query_plan(&unsupported_host);
        let mut output = [0; HOST_M11_VIEWPORT_BYTES];
        let HostStructuralQueryOutcome::Viewport { range, .. } = unsupported_host
            .query_structural(
                query_for(
                    unsupported.source,
                    HostSourceMetric::default(),
                    HostMetricAffinity::Downstream,
                    unsupported_budget,
                ),
                &mut output,
            )
            .expect("unsupported query")
        else {
            panic!("unsupported coverage must be a literal viewport");
        };
        assert_eq!(range, whole_source_range(unsupported.source));
        assert_eq!(output[M11_VIEWPORT_HEADER_BYTES + 12], 2);
        assert_eq!(read_u32(&output, M11_VIEWPORT_HEADER_BYTES + 56), 2);
        assert_eq!(read_u32(&output, M11_VIEWPORT_HEADER_BYTES + 60), 2);
        close_host(&mut unsupported_host);

        for (document, publication, reason, expected_public_opener) in [
            ([681, 682, 683, 684], [691, 692, 693, 694], 0x0003_0005, 1),
            ([701, 702, 703, 704], [711, 712, 713, 714], 0x0004_0006, 7),
        ] {
            let unsupported = persistent_block_snapshot(
                document,
                publication,
                1,
                1,
                "# h\n",
                vec![M11BlockSequenceEntry::unsupported(
                    4,
                    4,
                    M11BlockUnsupportedReason::new(reason).expect("typed unsupported reason"),
                )
                .expect("unsupported")],
                false,
            );
            let mut host = host_for(document);
            install(&mut host, &unsupported);
            let (_, budget) = persistent_block_query_plan(&host);
            let mut output = [0; HOST_M11_VIEWPORT_BYTES];
            let HostStructuralQueryOutcome::Viewport { .. } = host
                .query_structural(
                    query_for(
                        unsupported.source,
                        HostSourceMetric::default(),
                        HostMetricAffinity::Downstream,
                        budget,
                    ),
                    &mut output,
                )
                .expect("typed unsupported query")
            else {
                panic!("typed unsupported coverage must stay literal");
            };
            assert_eq!(read_u32(&output, M11_VIEWPORT_HEADER_BYTES + 56), 2);
            assert_eq!(
                read_u32(&output, M11_VIEWPORT_HEADER_BYTES + 60),
                expected_public_opener
            );
            close_host(&mut host);
        }
    }

    #[test]
    fn persistent_blocks_scale_past_128_entries_preflight_budget_and_keep_empty_semantics() {
        let empty_document = [681, 682, 683, 684];
        let empty = persistent_block_snapshot(
            empty_document,
            [691, 692, 693, 694],
            1,
            1,
            "",
            vec![],
            false,
        );
        let mut empty_host = host_for(empty_document);
        install(&mut empty_host, &empty);
        let (empty_descriptor, empty_budget) = persistent_block_query_plan(&empty_host);
        assert_eq!(empty_descriptor.entry_count(), 0);
        let mut empty_output = [0xa5; HOST_M11_VIEWPORT_BYTES];
        let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = empty_host
            .query_structural(
                query_for(
                    empty.source,
                    HostSourceMetric::default(),
                    HostMetricAffinity::Downstream,
                    empty_budget,
                ),
                &mut empty_output,
            )
            .expect("empty persistent block query")
        else {
            panic!("empty persistent block root must keep the existing empty viewport");
        };
        assert_eq!(range, whole_source_range(empty.source));
        assert_eq!(empty_output[M11_VIEWPORT_HEADER_BYTES + 12], 0);
        assert_eq!(receipt.leaf_count, 0);
        assert_eq!(receipt.tree_nodes_visited, 0);
        close_host(&mut empty_host);

        const PAIRS: usize = 130;
        let document = [701, 702, 703, 704];
        let mut entries = Vec::with_capacity(PAIRS * 2);
        for _ in 0..PAIRS {
            entries.push(block_paragraph(1, 1));
            entries.push(M11BlockSequenceEntry::blank(1, 1).expect("blank"));
        }
        let snapshot = persistent_block_snapshot(
            document,
            [711, 712, 713, 714],
            1,
            1,
            &"x\n".repeat(PAIRS),
            entries,
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (descriptor, budget) = persistent_block_query_plan(&host);
        assert_eq!(descriptor.entry_count(), (PAIRS * 2) as u64);
        assert!(descriptor.storage_page_count() > 1);
        assert!(descriptor.tree_height() > 1);
        assert_eq!(descriptor.maximum_entries_scanned(), 64);

        let mut untouched = [0xa5; HOST_M11_VIEWPORT_BYTES];
        let too_small = HostQueryBudget {
            maximum_tree_nodes_visited: budget.maximum_tree_nodes_visited - 1,
            ..budget
        };
        assert_eq!(
            host.query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric {
                        bytes: snapshot.source.utf8_length,
                        utf16: snapshot.source.utf16_length,
                    },
                    HostMetricAffinity::Upstream,
                    too_small,
                ),
                &mut untouched,
            )
            .expect("preflight tree gap"),
            source_gap(
                snapshot.source,
                whole_source_range(snapshot.source),
                HostSourceGapReason::TreeNodeLimit,
                HostViewportReceipt::default(),
            )
        );
        assert_eq!(untouched, [0xa5; HOST_M11_VIEWPORT_BYTES]);

        let mut output = [0; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric {
                        bytes: snapshot.source.utf8_length,
                        utf16: snapshot.source.utf16_length,
                    },
                    HostMetricAffinity::Upstream,
                    budget,
                ),
                &mut output,
            )
            .expect("large persistent block query");
        let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
            panic!("large block tree must return one bounded viewport: {outcome:?}");
        };
        assert_eq!(
            range,
            HostMetricRange {
                start: HostSourceMetric {
                    bytes: snapshot.source.utf8_length - 1,
                    utf16: snapshot.source.utf16_length - 1,
                },
                end: HostSourceMetric {
                    bytes: snapshot.source.utf8_length,
                    utf16: snapshot.source.utf16_length,
                },
            }
        );
        assert!(receipt.leaf_count <= descriptor.maximum_entries_scanned());
        assert!(receipt.tree_nodes_visited <= budget.maximum_tree_nodes_visited);
        assert_eq!(read_u32(&output, 8), M11_ROLE_SCHEMA);
        close_host(&mut host);
    }

    #[test]
    fn persistent_block_record_validation_rejects_corrupt_relative_closure() {
        let mut green = test_green_record(5).into_vec();
        let projection = test_projection_record(5);
        assert!(validate_leaf_relative_paragraph_records(
            5,
            0,
            &green,
            &projection
        ));
        green[24..32].copy_from_slice(&6_u64.to_le_bytes());
        assert!(!validate_leaf_relative_paragraph_records(
            5,
            0,
            &green,
            &projection
        ));

        let (green, projection) =
            fenced_code_role_records(14, 8..10, 0..3, 3..7, Some(10..13), b'`', 0);
        assert!(validate_leaf_relative_fenced_code_records(
            14,
            0,
            green.as_bytes(),
            projection.as_bytes(),
        ));
        let mut bad_green = green.as_bytes().to_vec();
        bad_green[76..80].copy_from_slice(&12_u32.to_le_bytes());
        assert!(!validate_leaf_relative_fenced_code_records(
            14,
            0,
            &bad_green,
            projection.as_bytes(),
        ));

        let (green, projection) =
            atx_heading_role_records(24, 6..16, 2..5, Some(17..20), 22..24, 3, 2, false);
        assert!(validate_leaf_relative_atx_heading_records(
            3,
            24,
            0,
            green.as_bytes(),
            projection.as_bytes(),
        ));
        let mut bad_green = green.as_bytes().to_vec();
        bad_green[56..60].copy_from_slice(&1_u32.to_le_bytes());
        assert!(!validate_leaf_relative_atx_heading_records(
            3,
            24,
            0,
            &bad_green,
            projection.as_bytes(),
        ));
        let (bom_green, bom_projection) =
            atx_heading_role_records(13, 6..7, 4..5, Some(8..11), 11..13, 1, 1, true);
        assert!(validate_leaf_relative_atx_heading_records(
            0,
            13,
            0,
            bom_green.as_bytes(),
            bom_projection.as_bytes(),
        ));
        assert!(
            !validate_leaf_relative_atx_heading_records(
                3,
                13,
                0,
                bom_green.as_bytes(),
                bom_projection.as_bytes(),
            ),
            "a BOM claim away from absolute BOF must fail closed"
        );

        let (green, projection) = setext_heading_role_records(20, 0..9, 13..16, 18..20, 1, 2, 7);
        assert!(validate_leaf_relative_setext_heading_records(
            20,
            7,
            green.as_bytes(),
            projection.as_bytes(),
        ));
        let mut bad_green = green.as_bytes().to_vec();
        bad_green[48..56].copy_from_slice(&3_u64.to_le_bytes());
        assert!(
            !validate_leaf_relative_setext_heading_records(
                20,
                7,
                &bad_green,
                projection.as_bytes(),
            ),
            "only Setext H1/H2 metadata is admissible"
        );
        let mut bad_green = green.as_bytes().to_vec();
        bad_green[56..60].copy_from_slice(&11_u32.to_le_bytes());
        assert!(
            !validate_leaf_relative_setext_heading_records(
                20,
                7,
                &bad_green,
                projection.as_bytes(),
            ),
            "underline marker start must authenticate the encoded indent"
        );
        let mut bad_green = green.as_bytes().to_vec();
        bad_green[72..80].copy_from_slice(&6_u64.to_le_bytes());
        assert!(
            !validate_leaf_relative_setext_heading_records(
                20,
                7,
                &bad_green,
                projection.as_bytes(),
            ),
            "Setext metadata must agree with the block entry definition count"
        );

        let (green, projection) = thematic_break_role_records(10, 2..7, 8..10, b'*', 3, 2, false);
        assert!(validate_leaf_relative_thematic_break_records(
            3,
            10,
            0,
            green.as_bytes(),
            projection.as_bytes(),
        ));
        let mut bad_green = green.as_bytes().to_vec();
        bad_green[72..80].copy_from_slice(&6_u64.to_le_bytes());
        assert!(
            !validate_leaf_relative_thematic_break_records(
                3,
                10,
                0,
                &bad_green,
                projection.as_bytes(),
            ),
            "marker count cannot exceed its exact source envelope"
        );
        let (bom_green, bom_projection) =
            thematic_break_role_records(7, 3..6, 6..7, b'_', 3, 0, true);
        assert!(validate_leaf_relative_thematic_break_records(
            0,
            7,
            0,
            bom_green.as_bytes(),
            bom_projection.as_bytes(),
        ));
        assert!(
            !validate_leaf_relative_thematic_break_records(
                1,
                7,
                0,
                bom_green.as_bytes(),
                bom_projection.as_bytes(),
            ),
            "a thematic-break BOM claim away from absolute BOF must fail closed"
        );

        let (green, projection) = indented_code_role_records(26, 3, 17, 16, 0, false);
        assert!(validate_leaf_relative_indented_code_records(
            7,
            26,
            25,
            0,
            green.as_bytes(),
            projection.as_bytes(),
        ));
        let mut bad_green = green.as_bytes().to_vec();
        bad_green[48..56].copy_from_slice(&3_u64.to_le_bytes());
        assert!(
            !validate_leaf_relative_indented_code_records(
                7,
                26,
                25,
                0,
                &bad_green,
                projection.as_bytes(),
            ),
            "indented code must retain the parser-certified four-column recipe"
        );
        let mut bad_projection = projection.as_bytes().to_vec();
        bad_projection[48..56].copy_from_slice(&2_u64.to_le_bytes());
        assert!(
            !validate_leaf_relative_indented_code_records(
                7,
                26,
                25,
                0,
                green.as_bytes(),
                &bad_projection,
            ),
            "projection run count must equal the certified physical line count"
        );
        let (bom_green, bom_projection) = indented_code_role_records(13, 1, 6, 6, 2, true);
        assert!(validate_leaf_relative_indented_code_records(
            0,
            13,
            11,
            0,
            bom_green.as_bytes(),
            bom_projection.as_bytes(),
        ));
        assert!(
            !validate_leaf_relative_indented_code_records(
                1,
                13,
                11,
                0,
                bom_green.as_bytes(),
                bom_projection.as_bytes(),
            ),
            "an indented-code BOM claim away from absolute BOF must fail closed"
        );

        let range = HostMetricRange {
            start: HostSourceMetric { bytes: 4, utf16: 4 },
            end: HostSourceMetric { bytes: 5, utf16: 5 },
        };
        let mut green = [0; M11_GREEN_RECORD_BYTES];
        let mut projection = [0; M11_PROJECTION_RECORD_BYTES];
        assert!(
            synthesize_literal_block_records(
                M11LiteralBlockKind::Unsupported(0x0002_0005),
                range,
                0,
                &mut green,
                &mut projection,
            ),
            "structured Setext variant 5 must not consume the existing unsupported-opener detail 5"
        );
        assert_eq!(green[12], 2);
        assert_eq!(read_u32(&green, 56), 2);
        assert_eq!(read_u32(&green, 60), 5);
        assert!(
            synthesize_literal_block_records(
                M11LiteralBlockKind::Unsupported(0x0002_0006),
                range,
                0,
                &mut green,
                &mut projection,
            ),
            "structured thematic-break variant 6 must not consume unsupported-opener detail 6"
        );
        assert_eq!(green[12], 2);
        assert_eq!(read_u32(&green, 56), 2);
        assert_eq!(read_u32(&green, 60), 6);
        assert!(
            synthesize_literal_block_records(
                M11LiteralBlockKind::Unsupported(0x0002_0008),
                range,
                0,
                &mut green,
                &mut projection,
            ),
            "structured indented-code variant 7 must not consume unsupported-opener detail 8"
        );
        assert_eq!(green[12], 2);
        assert_eq!(read_u32(&green, 56), 2);
        assert_eq!(read_u32(&green, 60), 8);
        assert!(!synthesize_literal_block_records(
            M11LiteralBlockKind::Unsupported(0x0002_000a),
            range,
            0,
            &mut green,
            &mut projection,
        ));
    }

    #[test]
    fn persistent_indented_code_query_validates_and_translates_exact_summary() {
        let document = [735, 736, 737, 738];
        let snapshot = persistent_block_snapshot(
            document,
            [745, 746, 747, 748],
            1,
            1,
            "p\n\n    alpha\r\n      β\n\tgamma",
            vec![
                block_paragraph(2, 2),
                M11BlockSequenceEntry::blank(1, 1).expect("blank"),
                block_indented_code(),
            ],
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (_, budget) = persistent_block_query_plan(&host);
        let mut output = [0; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric {
                        bytes: 15,
                        utf16: 15,
                    },
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut output,
            )
            .expect("indented-code query");
        let HostStructuralQueryOutcome::Viewport { range, .. } = outcome else {
            panic!("expected indented-code viewport: {outcome:?}");
        };
        assert_eq!(
            range,
            HostMetricRange {
                start: HostSourceMetric { bytes: 3, utf16: 3 },
                end: HostSourceMetric {
                    bytes: 29,
                    utf16: 28,
                },
            }
        );

        let green_start = M11_VIEWPORT_HEADER_BYTES;
        let projection_start = green_start + M11_GREEN_RECORD_BYTES;
        let green = &output[green_start..projection_start];
        let projection = &output[projection_start..projection_start + M11_PROJECTION_RECORD_BYTES];
        assert_eq!(green[12], M11_INDENTED_CODE_VARIANT);
        assert_eq!(projection[12], M11_INDENTED_CODE_VARIANT);
        assert_eq!(read_u64(green, 16), 3);
        assert_eq!(read_u64(green, 24), 29);
        assert_eq!(read_u64(green, 32), 3);
        assert_eq!(read_u64(green, 40), 3);
        assert_eq!(read_u32(green, 56), 3);
        assert_eq!(read_u32(green, 60), 17);
        assert_eq!(read_u32(green, 64), 16);
        assert_eq!(read_u32(green, 68), 0);
        assert_eq!(read_u64(projection, 32), 3);
        assert_eq!(read_u64(projection, 40), 3);
        assert_eq!(read_u64(projection, 48), 3);
        close_host(&mut host);
    }

    #[test]
    fn persistent_block_quote_query_preserves_noncontiguous_path_summary() {
        let document = [739, 740, 741, 742];
        let snapshot = persistent_block_snapshot(
            document,
            [749, 750, 751, 752],
            1,
            1,
            "p\n\n   > alpha\n> beta\nlazy\n",
            vec![
                block_paragraph(2, 2),
                M11BlockSequenceEntry::blank(1, 1).expect("blank"),
                block_quote(),
            ],
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (_, budget) = persistent_block_query_plan(&host);
        let mut output = [0; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric {
                        bytes: 15,
                        utf16: 15,
                    },
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut output,
            )
            .expect("block-quote query");
        let HostStructuralQueryOutcome::Viewport { range, .. } = outcome else {
            panic!("expected block-quote viewport: {outcome:?}");
        };
        assert_eq!(
            range,
            HostMetricRange {
                start: HostSourceMetric { bytes: 3, utf16: 3 },
                end: HostSourceMetric {
                    bytes: 26,
                    utf16: 26,
                },
            }
        );

        let green_start = M11_VIEWPORT_HEADER_BYTES;
        let projection_start = green_start + M11_GREEN_RECORD_BYTES;
        let green = &output[green_start..projection_start];
        let projection = &output[projection_start..projection_start + M11_PROJECTION_RECORD_BYTES];
        assert_eq!(green[12], M11_BLOCK_QUOTE_VARIANT);
        assert_eq!(projection[12], M11_BLOCK_QUOTE_VARIANT);
        assert_eq!(read_u64(green, 16), 3);
        assert_eq!(read_u64(green, 24), 26);
        assert_eq!(read_u64(green, 32), 3);
        assert_eq!(read_u64(green, 40), 3);
        assert_eq!(
            read_u64(green, 48),
            M11_BLOCK_QUOTE_EXACT_SINGLE_PARAGRAPH_DISPOSITION
        );
        assert_eq!(read_u32(green, 56), 3);
        assert_eq!(read_u32(green, 60), 0);
        assert_eq!(read_u32(green, 64), 3);
        assert_eq!(read_u32(green, 68), 16);
        assert_eq!(read_u32(green, 72), 16);
        assert_eq!(read_u64(projection, 32), 3);
        assert_eq!(read_u64(projection, 40), 3);
        assert_eq!(read_u64(projection, 48), 3);
        let mut point_path = [0_u8; M11_BLOCK_QUOTE_POINT_PATH_BYTES];
        assert!(encode_block_quote_point_path(
            &mut point_path,
            range,
            green,
            projection,
        ));
        assert_eq!(point_path[0], M11_POINT_PATH_KIND_BLOCK_QUOTE);
        assert_eq!(point_path[1], 0);
        assert_eq!(read_u32(&point_path, 4), M11_POINT_PATH_ROOT_PARENT);
        assert_eq!(read_u32(&point_path, 8), 3);
        assert_eq!(read_u32(&point_path, 12), 26);
        assert_eq!(read_u32(&point_path, 28), 3);
        let leaf = M11_POINT_PATH_NODE_RECORD_BYTES;
        assert_eq!(point_path[leaf], M11_POINT_PATH_KIND_PARAGRAPH);
        assert_eq!(
            point_path[leaf + 1],
            M11_POINT_PATH_FLAG_NONCONTIGUOUS | M11_POINT_PATH_FLAG_SELECTED
        );
        assert_eq!(
            u16::from_le_bytes(
                point_path[leaf + 2..leaf + 4]
                    .try_into()
                    .expect("leaf depth")
            ),
            1
        );
        assert_eq!(read_u32(&point_path, leaf + 4), 0);
        assert_eq!(read_u32(&point_path, leaf + 28), 3);
        close_host(&mut host);
    }

    #[test]
    fn persistent_structured_fence_query_validates_and_translates_exact_ranges() {
        let document = [741, 742, 743, 744];
        let snapshot = persistent_block_snapshot(
            document,
            [751, 752, 753, 754],
            1,
            1,
            "p\n\n```dart\nx\n```\n",
            vec![
                block_paragraph(2, 2),
                M11BlockSequenceEntry::blank(1, 1).expect("blank"),
                block_fenced_code(),
            ],
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (_, budget) = persistent_block_query_plan(&host);
        let mut output = [0; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric {
                        bytes: 11,
                        utf16: 11,
                    },
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut output,
            )
            .expect("fenced-code query");
        let HostStructuralQueryOutcome::Viewport { range, .. } = outcome else {
            panic!("structured fence must produce an exact viewport: {outcome:?}");
        };
        assert_eq!(
            range,
            HostMetricRange {
                start: HostSourceMetric { bytes: 3, utf16: 3 },
                end: HostSourceMetric {
                    bytes: 17,
                    utf16: 17,
                },
            }
        );
        let green =
            &output[M11_VIEWPORT_HEADER_BYTES..M11_VIEWPORT_HEADER_BYTES + M11_GREEN_RECORD_BYTES];
        let projection = &output[M11_VIEWPORT_HEADER_BYTES + M11_GREEN_RECORD_BYTES..];
        assert_eq!(green[12], M11_FENCED_CODE_VARIANT);
        assert_eq!(projection[12], M11_FENCED_CODE_VARIANT);
        assert_eq!(read_u64(green, 16), 3);
        assert_eq!(read_u64(green, 24), 17);
        assert_eq!(read_u64(green, 32), 11);
        assert_eq!(read_u64(green, 40), 13);
        assert_eq!(read_u32(green, 56), 3);
        assert_eq!(read_u32(green, 60), 6);
        assert_eq!(read_u32(green, 64), 6);
        assert_eq!(read_u32(green, 68), 10);
        assert_eq!(read_u32(green, 72), 13);
        assert_eq!(read_u32(green, 76), 16);
        assert_eq!(read_u64(projection, 32), 11);
        assert_eq!(read_u64(projection, 40), 13);
        close_host(&mut host);
    }

    #[test]
    fn persistent_structured_atx_query_validates_and_translates_exact_ranges() {
        let document = [761, 762, 763, 764];
        let snapshot = persistent_block_snapshot(
            document,
            [771, 772, 773, 774],
            1,
            1,
            "p\n\n  ### **β😀** ###  \r\n",
            vec![
                block_paragraph(2, 2),
                M11BlockSequenceEntry::blank(1, 1).expect("blank"),
                block_atx_heading(),
            ],
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (_, budget) = persistent_block_query_plan(&host);
        let mut output = [0; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric {
                        bytes: 11,
                        utf16: 11,
                    },
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut output,
            )
            .expect("ATX Heading query");
        let HostStructuralQueryOutcome::Viewport { range, .. } = outcome else {
            panic!("structured ATX Heading must produce an exact viewport: {outcome:?}");
        };
        assert_eq!(
            range,
            HostMetricRange {
                start: HostSourceMetric { bytes: 3, utf16: 3 },
                end: HostSourceMetric {
                    bytes: 27,
                    utf16: 24,
                },
            }
        );
        let green =
            &output[M11_VIEWPORT_HEADER_BYTES..M11_VIEWPORT_HEADER_BYTES + M11_GREEN_RECORD_BYTES];
        let projection = &output[M11_VIEWPORT_HEADER_BYTES + M11_GREEN_RECORD_BYTES..];
        assert_eq!(green[12], M11_ATX_HEADING_VARIANT);
        assert_eq!(projection[12], M11_ATX_HEADING_VARIANT);
        assert_eq!(read_u64(green, 16), 3);
        assert_eq!(read_u64(green, 24), 27);
        assert_eq!(read_u64(green, 32), 9);
        assert_eq!(read_u64(green, 40), 19);
        assert_eq!(read_u64(green, 48), 0x503);
        assert_eq!(read_u32(green, 56), 5);
        assert_eq!(read_u32(green, 60), 8);
        assert_eq!(read_u32(green, 64), 20);
        assert_eq!(read_u32(green, 68), 23);
        assert_eq!(read_u32(green, 72), 25);
        assert_eq!(read_u32(green, 76), 27);
        assert_eq!(read_u64(projection, 16), 3);
        assert_eq!(read_u64(projection, 24), 27);
        assert_eq!(read_u64(projection, 32), 9);
        assert_eq!(read_u64(projection, 40), 19);
        assert_eq!(read_u64(projection, 48), 1);
        close_host(&mut host);
    }

    #[test]
    fn persistent_structured_setext_query_validates_and_translates_exact_ranges() {
        let document = [781, 782, 783, 784];
        let snapshot = persistent_block_snapshot(
            document,
            [791, 792, 793, 794],
            1,
            1,
            "p\n\n**title**\r\n  ===  \r\n",
            vec![
                block_paragraph(2, 2),
                M11BlockSequenceEntry::blank(1, 1).expect("blank"),
                block_setext_heading(),
            ],
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (_, budget) = persistent_block_query_plan(&host);
        let mut output = [0; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric { bytes: 5, utf16: 5 },
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut output,
            )
            .expect("Setext Heading query");
        let HostStructuralQueryOutcome::Viewport { range, .. } = outcome else {
            panic!("structured Setext Heading must produce an exact viewport: {outcome:?}");
        };
        assert_eq!(
            range,
            HostMetricRange {
                start: HostSourceMetric { bytes: 3, utf16: 3 },
                end: HostSourceMetric {
                    bytes: 23,
                    utf16: 23,
                },
            }
        );
        let green =
            &output[M11_VIEWPORT_HEADER_BYTES..M11_VIEWPORT_HEADER_BYTES + M11_GREEN_RECORD_BYTES];
        let projection = &output[M11_VIEWPORT_HEADER_BYTES + M11_GREEN_RECORD_BYTES..];
        assert_eq!(green[12], M11_SETEXT_HEADING_VARIANT);
        assert_eq!(projection[12], M11_SETEXT_HEADING_VARIANT);
        assert_eq!(read_u64(green, 16), 3);
        assert_eq!(read_u64(green, 24), 23);
        assert_eq!(read_u64(green, 32), 3);
        assert_eq!(read_u64(green, 40), 12);
        assert_eq!(read_u64(green, 48), 0x201);
        assert_eq!(read_u32(green, 56), 16);
        assert_eq!(read_u32(green, 60), 19);
        assert_eq!(read_u32(green, 64), 21);
        assert_eq!(read_u32(green, 68), 23);
        assert_eq!(read_u64(green, 72), 0);
        assert_eq!(read_u64(projection, 16), 3);
        assert_eq!(read_u64(projection, 24), 23);
        assert_eq!(read_u64(projection, 32), 3);
        assert_eq!(read_u64(projection, 40), 12);
        assert_eq!(read_u64(projection, 48), 1);
        close_host(&mut host);
    }

    #[test]
    fn persistent_structured_thematic_break_query_is_atomic_and_marker_free() {
        let document = [795, 796, 797, 798];
        let snapshot = persistent_block_snapshot(
            document,
            [801, 802, 803, 804],
            1,
            1,
            "p\n\n  * * * \r\n",
            vec![
                block_paragraph(2, 2),
                M11BlockSequenceEntry::blank(1, 1).expect("blank"),
                block_thematic_break(),
            ],
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (_, budget) = persistent_block_query_plan(&host);
        let mut output = [0; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric { bytes: 7, utf16: 7 },
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut output,
            )
            .expect("thematic-break query");
        let HostStructuralQueryOutcome::Viewport { range, .. } = outcome else {
            panic!("structured thematic break must produce an exact viewport: {outcome:?}");
        };
        assert_eq!(
            range,
            HostMetricRange {
                start: HostSourceMetric { bytes: 3, utf16: 3 },
                end: HostSourceMetric {
                    bytes: 13,
                    utf16: 13,
                },
            }
        );
        let green =
            &output[M11_VIEWPORT_HEADER_BYTES..M11_VIEWPORT_HEADER_BYTES + M11_GREEN_RECORD_BYTES];
        let projection = &output[M11_VIEWPORT_HEADER_BYTES + M11_GREEN_RECORD_BYTES..];
        assert_eq!(green[12], M11_THEMATIC_BREAK_VARIANT);
        assert_eq!(projection[12], M11_THEMATIC_BREAK_VARIANT);
        assert_eq!(read_u64(green, 16), 3);
        assert_eq!(read_u64(green, 24), 13);
        assert_eq!(read_u64(green, 32), 3);
        assert_eq!(read_u64(green, 40), 3);
        assert_eq!(read_u64(green, 48), 0x22a);
        assert_eq!(read_u32(green, 56), 5);
        assert_eq!(read_u32(green, 60), 10);
        assert_eq!(read_u32(green, 64), 11);
        assert_eq!(read_u32(green, 68), 13);
        assert_eq!(read_u64(green, 72), 3);
        assert_eq!(read_u64(projection, 16), 3);
        assert_eq!(read_u64(projection, 24), 13);
        assert_eq!(read_u64(projection, 32), 3);
        assert_eq!(read_u64(projection, 40), 3);
        assert_eq!(read_u64(projection, 48), 0);
        close_host(&mut host);
    }

    #[test]
    fn persistent_authoritative_empty_root_emits_flkin_v2_instead_of_collapsing_to_v1() {
        let document = [501, 502, 503, 504];
        let snapshot = persistent_inline_snapshot(document, [511, 512, 513, 514], 1, 1, 0);
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (descriptor, budget, encoded_bytes) = persistent_query_plan(&host);
        assert_eq!(descriptor.logical_page_count(), 0);
        assert_eq!(descriptor.fact_count(), 0);
        assert_eq!(descriptor.storage_page_count(), 0);
        assert_eq!(encoded_bytes, 208);

        let mut output = vec![0xa5; encoded_bytes];
        let outcome = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric::default(),
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut output,
            )
            .expect("authoritative empty query");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("authoritative empty root must produce a viewport: {outcome:?}");
        };
        assert_eq!(receipt.encoded_bytes, 208);
        assert_eq!(receipt.leaf_count, 3);
        assert_eq!(&output[..8], VIEWPORT_MAGIC);
        assert_eq!(read_u32(&output, 8), M11_VIEWPORT_INLINE_SCHEMA);
        let metadata =
            M11_VIEWPORT_INLINE_HEADER_BYTES + M11_GREEN_RECORD_BYTES + M11_PROJECTION_RECORD_BYTES;
        assert_eq!(&output[metadata..metadata + 8], M11_INLINE_META_MAGIC);
        assert_eq!(output[metadata + 12], 1);
        assert_eq!(read_u32(&output, metadata + 20), 0);
        assert_eq!(
            read_u64(&output, metadata + 24),
            u64::from(descriptor.source_start())
        );
        assert_eq!(
            read_u64(&output, metadata + 32),
            u64::from(descriptor.source_end())
        );
        close_host(&mut host);
    }

    #[test]
    fn persistent_typed_pages_synthesize_the_existing_flkin_fact_contract() {
        let document = [521, 522, 523, 524];
        let snapshot = persistent_inline_snapshot(document, [531, 532, 533, 534], 1, 1, 3);
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (descriptor, budget, encoded_bytes) = persistent_query_plan(&host);
        assert_eq!(descriptor.logical_page_count(), 3);
        assert_eq!(descriptor.fact_count(), 3);

        let mut output = vec![0; encoded_bytes];
        let outcome = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric::default(),
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut output,
            )
            .expect("persistent FLKIN query");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("persistent facts must produce a viewport: {outcome:?}");
        };
        assert_eq!(receipt.encoded_bytes as usize, encoded_bytes);
        assert_eq!(receipt.leaf_count, 6);
        assert!(receipt.tree_nodes_visited <= budget.maximum_tree_nodes_visited);

        let metadata =
            M11_VIEWPORT_INLINE_HEADER_BYTES + M11_GREEN_RECORD_BYTES + M11_PROJECTION_RECORD_BYTES;
        assert_eq!(&output[metadata..metadata + 8], M11_INLINE_META_MAGIC);
        assert_eq!(output[metadata + 12], 1);
        assert_eq!(read_u32(&output, metadata + 16), 1);
        assert_eq!(read_u32(&output, metadata + 20), 3);
        assert_eq!(
            read_u32(&output, metadata + 40),
            M11_INLINE_FACT_RECORD_BYTES as u32
        );
        let facts = &output[metadata + M11_INLINE_META_RECORD_BYTES..];
        assert!(valid_inline_fact_records(
            facts,
            usize::try_from(descriptor.source_end() - descriptor.source_start())
                .expect("leaf bytes")
        ));
        for (ordinal, fact) in facts.chunks_exact(M11_INLINE_FACT_RECORD_BYTES).enumerate() {
            assert_eq!(fact[0], 2);
            assert_eq!(fact[1], 0);
            assert_eq!(read_u32(fact, 4), u32::try_from(ordinal * 6).unwrap());
            assert_eq!(read_u32(fact, 8), 5);
            assert_eq!(read_u32(fact, 12), u32::try_from(ordinal * 6 + 2).unwrap());
            assert_eq!(read_u32(fact, 16), 1);
        }
        close_host(&mut host);
    }

    #[test]
    fn persistent_direct_link_appends_the_authenticated_flkiv_value_lane() {
        let document = [523, 524, 525, 526];
        let snapshot = persistent_direct_link_snapshot(document, [533, 534, 535, 536], 1, 1);
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (descriptor, budget, encoded_bytes) = persistent_query_plan(&host);
        assert_eq!(descriptor.fact_count(), 1);
        assert_eq!(descriptor.link_value_entry_count(), 1);
        assert_eq!(descriptor.link_value_encoded_bytes(), 49);

        let mut output = vec![0; encoded_bytes];
        let outcome = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric::default(),
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut output,
            )
            .expect("persistent direct-link query");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("persistent direct link must produce a viewport: {outcome:?}");
        };
        assert_eq!(receipt.encoded_bytes as usize, encoded_bytes);

        let metadata =
            M11_VIEWPORT_INLINE_HEADER_BYTES + M11_GREEN_RECORD_BYTES + M11_PROJECTION_RECORD_BYTES;
        let fact = &output[metadata + M11_INLINE_META_RECORD_BYTES
            ..metadata + M11_INLINE_META_RECORD_BYTES + M11_INLINE_FACT_RECORD_BYTES];
        assert_eq!(fact[0], M11InlineProjectionKind::DirectLink as u8);
        assert_eq!(read_u32(fact, 4), 0);
        assert_eq!(read_u32(fact, 8), 12);
        assert_eq!(read_u32(fact, 12), 1);
        assert_eq!(read_u32(fact, 16), 1);

        let values =
            &output[metadata + M11_INLINE_META_RECORD_BYTES + M11_INLINE_FACT_RECORD_BYTES..];
        assert_eq!(&values[..8], b"FLKIV001");
        assert_eq!(read_u32(values, 8), 1);
        assert_eq!(read_u32(values, 12), 1);
        assert_eq!(read_u32(values, 16), 0);
        assert_eq!(read_u32(values, 24), 4);
        assert_eq!(read_u32(values, 28), 7);
        assert_eq!(read_u32(values, 40), 1);
        assert_eq!(&values[48..], b"*");
        close_host(&mut host);
    }

    #[test]
    fn persistent_escape_facts_synthesize_canonical_non_container_viewport_records() {
        let document = [525, 526, 527, 528];
        let snapshot = persistent_escape_snapshot(document, [535, 536, 537, 538], 1, 1, 3);
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (descriptor, budget, encoded_bytes) = persistent_query_plan(&host);
        assert_eq!(descriptor.logical_page_count(), 3);
        assert_eq!(descriptor.fact_count(), 3);

        let mut output = vec![0; encoded_bytes];
        let outcome = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric::default(),
                    HostMetricAffinity::Downstream,
                    budget,
                ),
                &mut output,
            )
            .expect("persistent escape query");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("persistent escapes must produce a viewport: {outcome:?}");
        };
        assert_eq!(receipt.encoded_bytes as usize, encoded_bytes);

        let metadata =
            M11_VIEWPORT_INLINE_HEADER_BYTES + M11_GREEN_RECORD_BYTES + M11_PROJECTION_RECORD_BYTES;
        let facts = &output[metadata + M11_INLINE_META_RECORD_BYTES..];
        assert!(valid_inline_fact_records(
            facts,
            usize::try_from(descriptor.source_end() - descriptor.source_start())
                .expect("leaf bytes")
        ));
        for (ordinal, fact) in facts.chunks_exact(M11_INLINE_FACT_RECORD_BYTES).enumerate() {
            let start = u32::try_from(ordinal * 3).expect("escape coordinate");
            assert_eq!(fact[0], M11InlineProjectionKind::BackslashEscape as u8);
            assert_eq!(fact[1], 0);
            assert_eq!(read_u32(fact, 4), start);
            assert_eq!(read_u32(fact, 8), 2);
            assert_eq!(read_u32(fact, 12), start + 1);
            assert_eq!(read_u32(fact, 16), 1);
        }
        close_host(&mut host);
    }

    #[test]
    fn viewport_escape_validator_rejects_flags_closers_and_noncanonical_widths() {
        let mut fact = [0_u8; M11_INLINE_FACT_RECORD_BYTES];
        fact[0] = M11InlineProjectionKind::BackslashEscape as u8;
        fact[4..8].copy_from_slice(&2_u32.to_le_bytes());
        fact[8..12].copy_from_slice(&2_u32.to_le_bytes());
        fact[12..16].copy_from_slice(&3_u32.to_le_bytes());
        fact[16..20].copy_from_slice(&1_u32.to_le_bytes());
        assert!(valid_inline_fact_records(&fact, 8));

        let mut flagged = fact;
        flagged[1] = 1;
        assert!(!valid_inline_fact_records(&flagged, 8));

        let mut container = fact;
        container[8..12].copy_from_slice(&3_u32.to_le_bytes());
        assert!(
            !valid_inline_fact_records(&container, 8),
            "an escape has no closer and is never a container"
        );

        let mut wide_content = fact;
        wide_content[8..12].copy_from_slice(&3_u32.to_le_bytes());
        wide_content[16..20].copy_from_slice(&2_u32.to_le_bytes());
        assert!(!valid_inline_fact_records(&wide_content, 8));

        let mut wide_opener = fact;
        wide_opener[12..16].copy_from_slice(&4_u32.to_le_bytes());
        assert!(!valid_inline_fact_records(&wide_opener, 8));
    }

    #[test]
    fn viewport_hard_line_break_validator_requires_marker_eol_and_collapsed_closer() {
        let mut fact = [0_u8; M11_INLINE_FACT_RECORD_BYTES];
        fact[0] = M11InlineProjectionKind::HardLineBreak as u8;
        fact[4..8].copy_from_slice(&2_u32.to_le_bytes());
        fact[8..12].copy_from_slice(&4_u32.to_le_bytes());
        fact[12..16].copy_from_slice(&4_u32.to_le_bytes());
        fact[16..20].copy_from_slice(&2_u32.to_le_bytes());
        assert!(valid_inline_fact_records(&fact, 8));

        let mut one_byte_eol = fact;
        one_byte_eol[8..12].copy_from_slice(&3_u32.to_le_bytes());
        one_byte_eol[12..16].copy_from_slice(&4_u32.to_le_bytes());
        one_byte_eol[16..20].copy_from_slice(&1_u32.to_le_bytes());
        assert!(valid_inline_fact_records(&one_byte_eol, 8));

        let mut flagged = fact;
        flagged[1] = 1;
        assert!(!valid_inline_fact_records(&flagged, 8));

        let mut no_marker = fact;
        no_marker[12..16].copy_from_slice(&2_u32.to_le_bytes());
        no_marker[16..20].copy_from_slice(&4_u32.to_le_bytes());
        assert!(!valid_inline_fact_records(&no_marker, 8));

        let mut wide_eol = fact;
        wide_eol[8..12].copy_from_slice(&5_u32.to_le_bytes());
        wide_eol[16..20].copy_from_slice(&3_u32.to_le_bytes());
        assert!(!valid_inline_fact_records(&wide_eol, 8));

        let mut closer = fact;
        closer[8..12].copy_from_slice(&5_u32.to_le_bytes());
        assert!(!valid_inline_fact_records(&closer, 8));
    }

    #[test]
    fn viewport_character_reference_record_carries_typed_scalars_at_fixed_width() {
        let fact = M11HostInlineProjectionFact::from(
            M11InlineProjectionFact::new_character_reference(2..17, '\u{2242}', Some('\u{0338}'))
                .expect("character-reference fact"),
        );
        let mut record = [0_u8; M11_INLINE_FACT_RECORD_BYTES];
        encode_inline_projection_fact_record(fact, &mut record)
            .expect("encode character-reference viewport record");
        assert_eq!(record[0], M11InlineProjectionKind::CharacterReference as u8);
        assert_eq!(record[1], 2);
        assert_eq!(read_u32(&record, 4), 2);
        assert_eq!(read_u32(&record, 8), 15);
        assert_eq!(read_u32(&record, 12), '\u{2242}' as u32);
        assert_eq!(read_u32(&record, 16), '\u{0338}' as u32);
        assert!(valid_inline_fact_records(&record, 32));

        let single = M11HostInlineProjectionFact::from(
            M11InlineProjectionFact::new_character_reference(2..8, '©', None)
                .expect("single-scalar character-reference fact"),
        );
        encode_inline_projection_fact_record(single, &mut record)
            .expect("encode single-scalar viewport record");
        assert_eq!(record[1], 1);
        assert_eq!(read_u32(&record, 12), '©' as u32);
        assert_eq!(read_u32(&record, 16), 0);
        assert!(valid_inline_fact_records(&record, 32));
    }

    #[test]
    fn viewport_character_reference_validator_rejects_malformed_scalar_records() {
        let mut fact = [0_u8; M11_INLINE_FACT_RECORD_BYTES];
        fact[0] = M11InlineProjectionKind::CharacterReference as u8;
        fact[1] = 2;
        fact[4..8].copy_from_slice(&2_u32.to_le_bytes());
        fact[8..12].copy_from_slice(&15_u32.to_le_bytes());
        fact[12..16].copy_from_slice(&('\u{2242}' as u32).to_le_bytes());
        fact[16..20].copy_from_slice(&('\u{0338}' as u32).to_le_bytes());
        assert!(valid_inline_fact_records(&fact, 32));

        for count in [0, 3] {
            let mut malformed = fact;
            malformed[1] = count;
            assert!(!valid_inline_fact_records(&malformed, 32));
        }

        let mut nonzero_single_sentinel = fact;
        nonzero_single_sentinel[1] = 1;
        assert!(!valid_inline_fact_records(&nonzero_single_sentinel, 32));

        let mut zero_second_for_count_two = fact;
        zero_second_for_count_two[16..20].copy_from_slice(&0_u32.to_le_bytes());
        assert!(!valid_inline_fact_records(&zero_second_for_count_two, 32));

        let mut invalid_first = fact;
        invalid_first[12..16].copy_from_slice(&0xd800_u32.to_le_bytes());
        assert!(!valid_inline_fact_records(&invalid_first, 32));

        let mut invalid_second = fact;
        invalid_second[16..20].copy_from_slice(&0x11_0000_u32.to_le_bytes());
        assert!(!valid_inline_fact_records(&invalid_second, 32));

        let mut short_source = fact;
        short_source[8..12].copy_from_slice(&3_u32.to_le_bytes());
        assert!(!valid_inline_fact_records(&short_source, 32));

        let mut long_source = fact;
        long_source[8..12].copy_from_slice(&34_u32.to_le_bytes());
        assert!(!valid_inline_fact_records(&long_source, 64));

        let mut reserved = fact;
        reserved[2] = 1;
        assert!(!valid_inline_fact_records(&reserved, 32));
    }

    #[test]
    fn persistent_query_budget_gaps_before_cursor_output_for_every_dimension() {
        let document = [541, 542, 543, 544];
        let snapshot = persistent_inline_snapshot(document, [551, 552, 553, 554], 1, 1, 3);
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (_, admitted, encoded_bytes) = persistent_query_plan(&host);
        let cases = [
            (
                HostQueryBudget {
                    maximum_encoded_bytes: admitted.maximum_encoded_bytes - 1,
                    ..admitted
                },
                HostSourceGapReason::EncodedByteLimit,
            ),
            (
                HostQueryBudget {
                    maximum_open_depth: admitted.maximum_open_depth - 1,
                    ..admitted
                },
                HostSourceGapReason::OpenDepthLimit,
            ),
            (
                HostQueryBudget {
                    maximum_leaf_count: admitted.maximum_leaf_count - 1,
                    ..admitted
                },
                HostSourceGapReason::LeafLimit,
            ),
            (
                HostQueryBudget {
                    maximum_tree_nodes_visited: admitted.maximum_tree_nodes_visited - 1,
                    ..admitted
                },
                HostSourceGapReason::TreeNodeLimit,
            ),
        ];
        for (budget, reason) in cases {
            let mut output = vec![0xa5; encoded_bytes];
            let outcome = host
                .query_structural(
                    query_for(
                        snapshot.source,
                        HostSourceMetric::default(),
                        HostMetricAffinity::Downstream,
                        budget,
                    ),
                    &mut output,
                )
                .expect("bounded persistent query");
            assert_eq!(
                outcome,
                source_gap(
                    snapshot.source,
                    whole_source_range(snapshot.source),
                    reason,
                    HostViewportReceipt::default()
                )
            );
            assert!(output.iter().all(|byte| *byte == 0xa5));
        }
        close_host(&mut host);
    }

    #[test]
    fn persistent_query_admits_over_flat_fanout_only_after_descriptor_budget() {
        const LOGICAL_PAGES: usize = 129;
        let document = [561, 562, 563, 564];
        let snapshot =
            persistent_inline_snapshot(document, [571, 572, 573, 574], 1, 1, LOGICAL_PAGES);
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let (descriptor, admitted, encoded_bytes) = persistent_query_plan(&host);
        assert_eq!(descriptor.logical_page_count(), LOGICAL_PAGES as u64);

        let mut untouched = vec![0xa5; encoded_bytes];
        let low_leaf = HostQueryBudget {
            maximum_leaf_count: admitted.maximum_leaf_count - 1,
            ..admitted
        };
        assert_eq!(
            host.query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric::default(),
                    HostMetricAffinity::Downstream,
                    low_leaf,
                ),
                &mut untouched,
            )
            .expect("low-budget persistent query"),
            source_gap(
                snapshot.source,
                whole_source_range(snapshot.source),
                HostSourceGapReason::LeafLimit,
                HostViewportReceipt::default()
            )
        );
        assert!(untouched.iter().all(|byte| *byte == 0xa5));

        let mut output = vec![0; encoded_bytes];
        let outcome = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric::default(),
                    HostMetricAffinity::Downstream,
                    admitted,
                ),
                &mut output,
            )
            .expect("admitted persistent query");
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = outcome else {
            panic!("admitted persistent root must produce a viewport: {outcome:?}");
        };
        assert_eq!(receipt.leaf_count, LOGICAL_PAGES as u32 + 3);
        assert_eq!(receipt.encoded_bytes as usize, encoded_bytes);
        assert!(receipt.tree_nodes_visited <= admitted.maximum_tree_nodes_visited);
        let metadata =
            M11_VIEWPORT_INLINE_HEADER_BYTES + M11_GREEN_RECORD_BYTES + M11_PROJECTION_RECORD_BYTES;
        assert_eq!(
            read_u32(&output, metadata + 20),
            u32::try_from(LOGICAL_PAGES).unwrap()
        );
        close_host(&mut host);
    }

    #[test]
    fn point_query_rejects_stale_source_and_impossible_metrics_without_output() {
        let document = [121, 122, 123, 124];
        let snapshot = snapshot_with_text(document, [131, 132, 133, 134], 4, 4, 1, "a😀b\n");
        let mut host = host_for(document);
        install(&mut host, &snapshot);

        let mut stale = snapshot.source;
        stale.revision -= 1;
        let mut output = [0xa5; HOST_M11_VIEWPORT_BYTES];
        let error = host
            .query_structural(
                query_for(
                    stale,
                    HostSourceMetric { bytes: 0, utf16: 0 },
                    HostMetricAffinity::Downstream,
                    FULL_QUERY_BUDGET,
                ),
                &mut output,
            )
            .expect_err("stale query authority");
        assert_eq!(error.reason(), HostRejectReason::ExactSourceMismatch);
        assert_eq!(output, [0xa5; HOST_M11_VIEWPORT_BYTES]);

        for impossible in [
            HostSourceMetric { bytes: 8, utf16: 5 },
            HostSourceMetric { bytes: 7, utf16: 6 },
            HostSourceMetric { bytes: 7, utf16: 4 },
            HostSourceMetric { bytes: 2, utf16: 3 },
        ] {
            let mut output = [0xa5; HOST_M11_VIEWPORT_BYTES];
            let error = host
                .query_structural(
                    query_for(
                        snapshot.source,
                        impossible,
                        HostMetricAffinity::Downstream,
                        FULL_QUERY_BUDGET,
                    ),
                    &mut output,
                )
                .expect_err("impossible metric pair");
            assert_eq!(error.reason(), HostRejectReason::Invalid);
            assert_eq!(output, [0xa5; HOST_M11_VIEWPORT_BYTES]);
        }
        close_host(&mut host);
    }

    #[test]
    fn query_budget_gaps_have_deterministic_precedence_and_truthful_zero_work() {
        let document = [141, 142, 143, 144];
        let snapshot = snapshot(document, [151, 152, 153, 154], 1, 1, 1);
        let mut host = host_for(document);
        install(&mut host, &snapshot);

        let cases = [
            (
                HostQueryBudget {
                    maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32 - 1,
                    maximum_open_depth: 0,
                    maximum_leaf_count: 0,
                    maximum_tree_nodes_visited: 0,
                },
                HostSourceGapReason::EncodedByteLimit,
            ),
            (
                HostQueryBudget {
                    maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                    maximum_open_depth: 0,
                    maximum_leaf_count: 0,
                    maximum_tree_nodes_visited: 0,
                },
                HostSourceGapReason::OpenDepthLimit,
            ),
            (
                HostQueryBudget {
                    maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                    maximum_open_depth: 1,
                    maximum_leaf_count: 1,
                    maximum_tree_nodes_visited: 0,
                },
                HostSourceGapReason::LeafLimit,
            ),
            (
                HostQueryBudget {
                    maximum_encoded_bytes: HOST_M11_VIEWPORT_BYTES as u32,
                    maximum_open_depth: 1,
                    maximum_leaf_count: 2,
                    maximum_tree_nodes_visited: 1,
                },
                HostSourceGapReason::TreeNodeLimit,
            ),
        ];
        for (budget, expected_reason) in cases {
            let mut output = [0xa5; HOST_M11_VIEWPORT_BYTES];
            let outcome = host
                .query_structural(
                    query_for(
                        snapshot.source,
                        HostSourceMetric { bytes: 0, utf16: 0 },
                        HostMetricAffinity::Downstream,
                        budget,
                    ),
                    &mut output,
                )
                .expect("typed budget gap");
            let HostStructuralQueryOutcome::SourceGap {
                source_version,
                range,
                reason,
                receipt,
            } = outcome
            else {
                panic!("bounded query must gap: {outcome:?}");
            };
            assert_eq!(source_version, snapshot.source);
            assert_eq!(range, whole_source_range(snapshot.source));
            assert_eq!(reason, expected_reason);
            assert_eq!(receipt, HostViewportReceipt::default());
            assert_eq!(output, [0xa5; HOST_M11_VIEWPORT_BYTES]);
        }
        close_host(&mut host);
    }

    #[test]
    fn undecodable_installed_roles_return_typed_gap_and_never_claim_output_bytes() {
        let document = [201, 202, 203, 204];
        let snapshot = snapshot_with_role_validity(
            document,
            [211, 212, 213, 214],
            1,
            1,
            1,
            "paragraph\n",
            false,
            false,
        );
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let mut output = [0xa5; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric { bytes: 0, utf16: 0 },
                    HostMetricAffinity::Downstream,
                    FULL_QUERY_BUDGET,
                ),
                &mut output,
            )
            .expect("malformed installed role is a typed structural gap");
        let HostStructuralQueryOutcome::SourceGap {
            range,
            reason,
            receipt,
            ..
        } = outcome
        else {
            panic!("malformed role must not become a viewport: {outcome:?}");
        };
        assert_eq!(range, whole_source_range(snapshot.source));
        assert_eq!(reason, HostSourceGapReason::UndecodableClosure);
        assert_eq!(receipt.encoded_bytes, 0);
        assert_eq!(receipt.leaf_count, 2);
        assert_eq!(receipt.open_depth, 1);
        assert_eq!(receipt.tree_nodes_visited, 2);
        assert_ne!(&output[..8], VIEWPORT_MAGIC);
        close_host(&mut host);
    }

    #[test]
    fn admitted_viewport_rejects_undersized_bridge_scratch_before_writing() {
        let document = [161, 162, 163, 164];
        let snapshot = snapshot(document, [171, 172, 173, 174], 1, 1, 1);
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let mut output = [0xa5; HOST_M11_VIEWPORT_BYTES - 1];
        let error = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric { bytes: 0, utf16: 0 },
                    HostMetricAffinity::Downstream,
                    FULL_QUERY_BUDGET,
                ),
                &mut output,
            )
            .expect_err("undersized fixed scratch");
        assert_eq!(error.reason(), HostRejectReason::QueryBoundExceeded);
        assert_eq!(output, [0xa5; HOST_M11_VIEWPORT_BYTES - 1]);
        close_host(&mut host);
    }

    #[test]
    fn ten_megabyte_source_query_keeps_the_same_constant_copy_and_work_receipt() {
        let document = [181, 182, 183, 184];
        let text = "a".repeat(10 * 1024 * 1024);
        let snapshot = snapshot_with_text(document, [191, 192, 193, 194], 1, 1, 1, &text);
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let mut output = [0_u8; HOST_M11_VIEWPORT_BYTES];
        let outcome = host
            .query_structural(
                query_for(
                    snapshot.source,
                    HostSourceMetric {
                        bytes: snapshot.source.utf8_length,
                        utf16: snapshot.source.utf16_length,
                    },
                    HostMetricAffinity::Upstream,
                    FULL_QUERY_BUDGET,
                ),
                &mut output,
            )
            .expect("large exact-source query");
        let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = outcome else {
            panic!("large source must retain fixed M1.1 viewport: {outcome:?}");
        };
        assert_eq!(range, whole_source_range(snapshot.source));
        assert_eq!(receipt.encoded_bytes, HOST_M11_VIEWPORT_BYTES as u32);
        assert_eq!(receipt.leaf_count, 2);
        assert_eq!(receipt.open_depth, 1);
        assert_eq!(receipt.tree_nodes_visited, 2);
        assert_eq!(receipt.summary_nodes_skipped, 0);
        close_host(&mut host);
    }

    #[test]
    fn superseded_near_capacity_reclaims_before_replacement_packet_credit() {
        let document = [41, 42, 43, 44];
        let old = snapshot(document, [51, 52, 53, 54], 1, 1, 128);
        let replacement = snapshot(document, [61, 62, 63, 64], 2, 3, 1);
        let limits = M11HostLimits {
            arena_max_slots: 150,
            maximum_snapshot_nodes: 150,
            ..M11HostLimits::default()
        };
        let mut host = NativeCandidateHost::new_with_limits(
            HostConfig {
                document_session: document,
                grammar_revision: 1,
                syntax_profile: 1,
                authority_mask: 0x1f,
                maximum_query_bytes: 64 * 1024,
            },
            limits,
        )
        .expect("bounded host");
        host.observe_source_version(old.source).expect("old source");
        host.begin_offer(old.offer).expect("old offer");
        for frame in old.frames.iter().take(old.frames.len() - 2) {
            admit_and_credit(&mut host, frame);
        }

        host.observe_source_version(replacement.source)
            .expect("superseding source");
        host.begin_offer(replacement.offer)
            .expect("replacement offer");
        let begin = &replacement.frames[0];
        let encoded = packet_bytes(std::slice::from_ref(begin));
        admit_packet_bytes(&mut host, &encoded);

        let mut pending_polls = 0;
        loop {
            match host
                .poll(HostWorkGrant {
                    inspect_bytes: M11_HOST_MAXIMUM_FRAME_BYTES as u32
                        + PACKET_FRAME_DESCRIPTOR_BYTES as u32,
                    copy_bytes: M11_HOST_MAXIMUM_FRAME_BYTES as u32,
                    transitions: 1,
                })
                .expect("reclaim-before-credit poll")
            {
                HostPollOutcome::Pending => pending_polls += 1,
                HostPollOutcome::PacketCredit {
                    next_frame_ordinal, ..
                } => {
                    assert_eq!(next_frame_ordinal, 1);
                    break;
                }
                outcome => panic!("unexpected replacement outcome: {outcome:?}"),
            }
            assert!(pending_polls < 512, "background reclaim must converge");
        }
        assert!(
            pending_polls > 100,
            "the replacement must not spend unreclaimed near-capacity staging"
        );

        for frame in &replacement.frames[1..] {
            admit_and_credit(&mut host, frame);
        }
        host.request_commit(replacement.commit)
            .expect("replacement commit");
        loop {
            match host
                .poll(HostWorkGrant {
                    inspect_bytes: 0,
                    copy_bytes: 0,
                    transitions: 16,
                })
                .expect("replacement install")
            {
                HostPollOutcome::Pending => {}
                HostPollOutcome::Committed(ack) => {
                    assert_eq!(ack.parse_generation, 3);
                    break;
                }
                outcome => panic!("unexpected install outcome: {outcome:?}"),
            }
        }
        host.begin_close().expect("host close");
        while !matches!(
            host.poll(HostWorkGrant {
                inspect_bytes: 0,
                copy_bytes: 0,
                transitions: 256,
            })
            .expect("host close poll"),
            HostPollOutcome::Closed
        ) {}
    }

    #[test]
    fn newer_source_absorbs_unpolled_structural_abort_before_replacement_begin() {
        let document = [81, 82, 83, 84];
        let old = snapshot(document, [91, 92, 93, 94], 1, 1, 128);
        let replacement = snapshot(document, [101, 102, 103, 104], 2, 2, 1);
        let limits = M11HostLimits {
            arena_max_slots: 150,
            maximum_snapshot_nodes: 150,
            ..M11HostLimits::default()
        };
        let mut host = NativeCandidateHost::new_with_limits(
            HostConfig {
                document_session: document,
                grammar_revision: 1,
                syntax_profile: 1,
                authority_mask: 0x1f,
                maximum_query_bytes: 64 * 1024,
            },
            limits,
        )
        .expect("bounded host");
        host.observe_source_version(old.source).expect("old source");
        host.begin_offer(old.offer).expect("old offer");
        for frame in old.frames.iter().take(old.frames.len() - 2) {
            admit_and_credit(&mut host, frame);
        }
        host.abort_offer(old.offer.offer_id)
            .expect("accepted structural abort");
        assert_eq!(host.aborting_offer, Some(old.offer.offer_id));
        assert!(host.background_reclaim_pending);

        host.observe_source_version(replacement.source)
            .expect("newer source absorbs the stale abort handshake");
        assert_eq!(host.aborting_offer, None);
        assert!(
            host.background_reclaim_pending,
            "source adoption must preserve the bounded reclaim obligation"
        );
        host.begin_offer(replacement.offer)
            .expect("replacement begins without an orphaned abort poll");
        let begin = &replacement.frames[0];
        let encoded = packet_bytes(std::slice::from_ref(begin));
        admit_packet_bytes(&mut host, &encoded);
        let mut pending_polls = 0;
        loop {
            match host
                .poll(HostWorkGrant {
                    inspect_bytes: M11_HOST_MAXIMUM_FRAME_BYTES as u32
                        + PACKET_FRAME_DESCRIPTOR_BYTES as u32,
                    copy_bytes: M11_HOST_MAXIMUM_FRAME_BYTES as u32,
                    transitions: 1,
                })
                .expect("reclaim-before-replacement-credit poll")
            {
                HostPollOutcome::Pending => pending_polls += 1,
                HostPollOutcome::PacketCredit {
                    next_frame_ordinal, ..
                } => {
                    assert_eq!(next_frame_ordinal, 1);
                    break;
                }
                outcome => panic!("unexpected replacement outcome: {outcome:?}"),
            }
            assert!(pending_polls < 512, "absorbed abort reclaim must converge");
        }
        assert!(
            pending_polls > 100,
            "replacement credit must wait for retained near-capacity reclaim"
        );
        close_host(&mut host);
    }

    #[test]
    fn equal_source_does_not_absorb_unpolled_structural_abort() {
        let document = [111, 112, 113, 114];
        let old = snapshot(document, [121, 122, 123, 124], 1, 1, 1);
        let same_source_replacement = snapshot(document, [131, 132, 133, 134], 1, 2, 1);
        let mut host = host_for(document);
        host.observe_source_version(old.source).expect("old source");
        host.begin_offer(old.offer).expect("old offer");
        host.abort_offer(old.offer.offer_id)
            .expect("accepted structural abort");

        host.observe_source_version(old.source)
            .expect("equal source observation is idempotent");
        assert_eq!(host.aborting_offer, Some(old.offer.offer_id));
        assert_eq!(
            host.begin_offer(same_source_replacement.offer)
                .expect_err("equal authority cannot cancel an abort handshake")
                .reason(),
            HostRejectReason::Backpressure
        );
        assert_eq!(
            host.poll(HostWorkGrant {
                inspect_bytes: 0,
                copy_bytes: 0,
                transitions: 1,
            })
            .expect("complete original abort"),
            HostPollOutcome::AbortComplete {
                offer_id: old.offer.offer_id,
            }
        );
        host.begin_offer(same_source_replacement.offer)
            .expect("replacement begins after exact abort completion");
        close_host(&mut host);
    }

    #[test]
    fn newer_source_absorbs_unpolled_sidecar_abort_before_structural_begin() {
        let document = [141, 142, 143, 144];
        let (base, sidecar, _) =
            snapshot_with_inline_sidecar_pair(document, [151, 152, 153, 154], 1, 1);
        let (replacement, _, _) =
            snapshot_with_inline_sidecar_pair(document, [161, 162, 163, 164], 2, 2);
        let mut host = host_for(document);
        let base_ack = install(&mut host, &base);
        host.acknowledge_delivery(base_ack)
            .expect("acknowledge structural base");
        host.begin_inline_sidecar_offer(sidecar.begin(base_ack))
            .expect("sidecar offer");
        for frame in sidecar.frames.iter().take(sidecar.frames.len() - 1) {
            admit_and_credit_inline_sidecar(&mut host, frame);
        }
        host.abort_inline_sidecar_offer(sidecar.offer_id)
            .expect("accepted sidecar abort");
        assert_eq!(host.aborting_inline_sidecar_offer, Some(sidecar.offer_id));
        assert!(host.inline_sidecar_reclaim_pending);

        host.observe_source_version(replacement.source)
            .expect("newer source absorbs the stale sidecar abort handshake");
        assert_eq!(host.aborting_inline_sidecar_offer, None);
        assert!(
            host.inline_sidecar_reclaim_pending,
            "source adoption must preserve sidecar reclaim debt"
        );
        host.begin_offer(replacement.offer)
            .expect("structural replacement begins without sidecar backpressure");
        let begin = &replacement.frames[0];
        let encoded = packet_bytes(std::slice::from_ref(begin));
        admit_packet_bytes(&mut host, &encoded);
        let mut pending_polls = 0;
        loop {
            match host
                .poll(HostWorkGrant {
                    inspect_bytes: M11_HOST_MAXIMUM_FRAME_BYTES as u32
                        + PACKET_FRAME_DESCRIPTOR_BYTES as u32,
                    copy_bytes: M11_HOST_MAXIMUM_FRAME_BYTES as u32,
                    transitions: 1,
                })
                .expect("sidecar-reclaim-before-structural-credit poll")
            {
                HostPollOutcome::Pending => pending_polls += 1,
                HostPollOutcome::PacketCredit {
                    next_frame_ordinal, ..
                } => {
                    assert_eq!(next_frame_ordinal, 1);
                    break;
                }
                outcome => panic!("unexpected structural outcome: {outcome:?}"),
            }
            assert!(
                pending_polls < 512,
                "absorbed sidecar reclaim must converge"
            );
        }
        assert!(
            pending_polls > 0,
            "replacement credit must wait for retained sidecar reclaim"
        );
        close_host(&mut host);
    }

    #[test]
    fn viewport_schema8_24_child_lifecycle_is_atomic_and_queryable() {
        const CHILD_COUNT: u32 = 24;
        const UNSUPPORTED_REASON: u32 = 0x2000_0002;
        let document = [401, 402, 403, 404];
        let (structural, children) = snapshot_with_unsupported_viewport_children(
            document,
            [411, 412, 413, 414],
            CHILD_COUNT,
        );
        let mut host = host_for(document);
        let base_ack = install(&mut host, &structural);
        host.acknowledge_delivery(base_ack)
            .expect("acknowledge viewport structural base");
        let presentation = viewport_presentation_from_children(base_ack, &children);
        let ack = install_viewport_presentation(&mut host, &presentation);

        let expected_bytes = HOST_VIEWPORT_PRESENTATION_HEADER_BYTES
            + CHILD_COUNT as usize * HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES
            + CHILD_COUNT as usize * b"parser unsupported".len();
        let mut undersized = vec![0xa5; expected_bytes];
        assert_eq!(
            host.query_viewport_presentation(
                ack,
                u32::try_from(expected_bytes - 1).expect("undersized viewport query"),
                &mut undersized,
            )
            .expect_err("viewport query ceiling must fail before mutation")
            .reason(),
            HostRejectReason::QueryBoundExceeded
        );
        assert!(
            undersized.iter().all(|byte| *byte == 0xa5),
            "query preflight must reject bounds before output mutation"
        );

        let mut wrong_ack = ack;
        wrong_ack.publication_session[0] ^= 1;
        let mut output = vec![0_u8; 64 * 1024];
        assert_eq!(
            host.query_viewport_presentation(wrong_ack, output.len() as u32, &mut output)
                .expect("wrong viewport ACK query"),
            HostViewportPresentationQueryOutcome::Unavailable
        );
        let outcome = host
            .query_viewport_presentation(ack, output.len() as u32, &mut output)
            .expect("exact viewport ACK query");
        assert_eq!(
            outcome,
            HostViewportPresentationQueryOutcome::Available {
                encoded_bytes: expected_bytes as u32,
                entry_count: CHILD_COUNT,
            }
        );
        let page = &output[..expected_bytes];
        assert_eq!(&page[..8], VIEWPORT_MAGIC);
        assert_eq!(read_u32(page, 8), HOST_VIEWPORT_PRESENTATION_SCHEMA);
        assert_eq!(
            read_u32(page, 12),
            HOST_VIEWPORT_PRESENTATION_HEADER_BYTES as u32
        );
        assert_eq!(
            read_u32(page, 16),
            HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES as u32
        );
        assert_eq!(read_u32(page, 20), CHILD_COUNT);
        assert_eq!(
            read_u32(page, 24),
            (HOST_VIEWPORT_PRESENTATION_HEADER_BYTES
                + CHILD_COUNT as usize * HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES)
                as u32
        );
        assert_eq!(read_u32(page, 28), expected_bytes as u32);
        assert!(
            page[128..160].iter().any(|byte| *byte != 0),
            "schema-8 page must carry its exact-ACK binding digest"
        );
        for index in 0..CHILD_COUNT as usize {
            let start = HOST_VIEWPORT_PRESENTATION_HEADER_BYTES
                + index * HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES;
            let entry = &page[start..start + HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES];
            assert_eq!(read_u32(entry, 0), index as u32);
            assert_eq!(read_u32(entry, 4), base_ack.source_version.revision);
            assert_eq!(&entry[8..24], &id128_bytes(document));
            assert_eq!(read_u64(entry, 80), index as u64);
            assert_eq!(entry[120], u8::MAX);
            assert_eq!(entry[121], 2);
            assert_eq!(read_u32(entry, 124), 0);
            assert_eq!(read_u32(entry, 132), b"parser unsupported".len() as u32);
            assert_eq!(read_u32(entry, 136), UNSUPPORTED_REASON);
            let expected_payload_offset = HOST_VIEWPORT_PRESENTATION_HEADER_BYTES
                + CHILD_COUNT as usize * HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES
                + index * b"parser unsupported".len();
            assert_eq!(read_u32(entry, 128), expected_payload_offset as u32);
        }
        assert!(
            !page.windows(4).any(|window| window == b"HIO1"),
            "opaque HIO1 frames must never escape into the public schema-8 page"
        );
        assert_eq!(
            host.acknowledge_viewport_presentation_delivery(wrong_ack)
                .expect_err("wrong viewport delivery proof")
                .reason(),
            HostRejectReason::Invalid
        );
        host.acknowledge_viewport_presentation_delivery(ack)
            .expect("exact viewport delivery proof");
        close_host(&mut host);
    }

    #[test]
    fn viewport_inline_payload_appends_the_authenticated_flkiv_value_lane() {
        let document = [405, 406, 407, 408];
        let (structural, direct_link, _) =
            snapshot_with_direct_link_sidecar_pair(document, [415, 416, 417, 418], 1, 1);
        let mut host = host_for(document);
        let base_ack = install(&mut host, &structural);
        host.acknowledge_delivery(base_ack)
            .expect("acknowledge direct-link structural base");
        let presentation =
            viewport_presentation_from_children(base_ack, std::slice::from_ref(&direct_link));
        let ack = install_viewport_presentation(&mut host, &presentation);

        const FACT_BYTES: usize = M11_INLINE_FACT_RECORD_BYTES;
        const VALUE_BYTES: usize = 16 + 32 + 1;
        let expected_bytes = HOST_VIEWPORT_PRESENTATION_HEADER_BYTES
            + HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES
            + FACT_BYTES
            + VALUE_BYTES;
        let mut output = vec![0_u8; expected_bytes];
        assert_eq!(
            host.query_viewport_presentation(ack, expected_bytes as u32, &mut output)
                .expect("query direct-link viewport"),
            HostViewportPresentationQueryOutcome::Available {
                encoded_bytes: expected_bytes as u32,
                entry_count: 1,
            }
        );

        let entry_start = HOST_VIEWPORT_PRESENTATION_HEADER_BYTES;
        let entry =
            &output[entry_start..entry_start + HOST_VIEWPORT_PRESENTATION_DIRECTORY_ENTRY_BYTES];
        let payload_start = read_u32(entry, 128) as usize;
        assert_eq!(entry[120], 1);
        assert_eq!(entry[121], 1);
        assert_eq!(read_u32(entry, 124), 1);
        assert_eq!(read_u32(entry, 132), (FACT_BYTES + VALUE_BYTES) as u32);
        let fact = &output[payload_start..payload_start + FACT_BYTES];
        assert_eq!(fact[0], M11InlineProjectionKind::DirectLink as u8);
        let values = &output[payload_start + FACT_BYTES..expected_bytes];
        assert_eq!(&values[..8], b"FLKIV001");
        assert_eq!(read_u32(values, 8), 1);
        assert_eq!(read_u32(values, 12), 1);
        assert_eq!(read_u32(values, 16), 0);
        assert_eq!(read_u32(values, 40), 1);
        assert_eq!(values[48], b'*');

        host.acknowledge_viewport_presentation_delivery(ack)
            .expect("acknowledge direct-link viewport delivery");
        close_host(&mut host);
    }

    #[test]
    fn viewport_corruption_order_and_fuelled_cancel_fail_closed() {
        let document = [421, 422, 423, 424];
        let (structural, children) =
            snapshot_with_unsupported_viewport_children(document, [431, 432, 433, 434], 2);
        let mut host = host_for(document);
        let base_ack = install(&mut host, &structural);
        host.acknowledge_delivery(base_ack)
            .expect("acknowledge negative-test structural base");
        let presentation = viewport_presentation_from_children(base_ack, &children);

        host.begin_viewport_presentation_offer(presentation.begin)
            .expect("begin corrupt viewport");
        let parent = &presentation.frames[0];
        let mut corrupt_bytes = parent.bytes.to_vec();
        *corrupt_bytes.last_mut().expect("viewport parent bytes") ^= 1;
        let corrupt = TestFrame {
            offer_id: parent.offer_id,
            ordinal: parent.ordinal,
            first_record_ordinal: parent.first_record_ordinal,
            record_count: parent.record_count,
            digest: parent.digest,
            bytes: corrupt_bytes.into_boxed_slice(),
        };
        assert_eq!(
            admit_and_credit_viewport(&mut host, &corrupt)
                .expect_err("corrupt viewport digest")
                .reason(),
            HostRejectReason::CorruptPublication
        );
        host.abort_viewport_presentation_offer(presentation.begin.offer_id)
            .expect("abort corrupt viewport");
        finish_viewport_abort(&mut host, presentation.begin.offer_id);

        host.begin_viewport_presentation_offer(presentation.begin)
            .expect("begin out-of-order viewport");
        let directory = &presentation.frames[1];
        let mut order_transport = ViewportPresentationTransportDigest::new();
        let digest256 = order_transport
            .push(
                0,
                0,
                directory.record_count,
                ViewportPresentationFrameKind::Begin,
                &directory.bytes,
            )
            .expect("out-of-order viewport digest");
        let out_of_order = TestFrame {
            offer_id: presentation.begin.offer_id,
            ordinal: 0,
            first_record_ordinal: 0,
            record_count: directory.record_count,
            digest: protocol_digest128_from_blake3(
                ProtocolDigestDomain::ViewportPresentationFrame,
                digest256,
            ),
            bytes: directory.bytes.clone(),
        };
        assert_eq!(
            admit_and_credit_viewport(&mut host, &out_of_order)
                .expect_err("Directory cannot replace Parent")
                .reason(),
            HostRejectReason::CorruptPublication
        );
        host.abort_viewport_presentation_offer(presentation.begin.offer_id)
            .expect("abort out-of-order viewport");
        finish_viewport_abort(&mut host, presentation.begin.offer_id);

        host.begin_viewport_presentation_offer(presentation.begin)
            .expect("begin cancellable viewport");
        for frame in presentation.frames.iter().take(3) {
            admit_and_credit_viewport(&mut host, frame).expect("partial viewport frame");
        }
        host.abort_viewport_presentation_offer(presentation.begin.offer_id)
            .expect("abort partial viewport child");
        finish_viewport_abort(&mut host, presentation.begin.offer_id);
        host.begin_viewport_presentation_offer(presentation.begin)
            .expect("fuelled cancellation releases viewport backpressure");
        host.abort_viewport_presentation_offer(presentation.begin.offer_id)
            .expect("abort post-reclaim viewport");
        finish_viewport_abort(&mut host, presentation.begin.offer_id);
        close_host(&mut host);
    }

    #[test]
    fn back_to_back_revisions_do_not_accumulate_manifest_ancestry() {
        let document = [201, 202, 203, 204];
        let first = snapshot(document, [301, 302, 303, 304], 1, 1, 1);
        let closure_nodes = first
            .frames
            .len()
            .checked_sub(2)
            .expect("snapshot has Begin and End");
        assert!(closure_nodes > 1, "receipt requires a nontrivial closure");
        let arena_max_slots = closure_nodes
            .checked_mul(2)
            .expect("two-root arena envelope");
        let limits = M11HostLimits {
            // Installation legitimately overlaps the current and candidate
            // closures, but there is no room for a third historical root.
            arena_max_slots,
            maximum_snapshot_nodes: arena_max_slots as u64,
            ..M11HostLimits::default()
        };
        let mut host = NativeCandidateHost::new_with_limits(
            HostConfig {
                document_session: document,
                grammar_revision: 1,
                syntax_profile: 1,
                authority_mask: 0x1f,
                maximum_query_bytes: 64 * 1024,
            },
            limits,
        )
        .expect("two-root host");

        let (first_ack, first_reclaim_polls) = install_back_to_back(&mut host, &first);
        assert_eq!(first_ack.source_version, first.source);
        assert_eq!(first_reclaim_polls, 0);

        let mut observed_reclaim_barrier = false;
        for revision in 2..=64_u32 {
            let publication = [300 + revision, 302, 303, 304];
            let next = snapshot(document, publication, revision, revision, 1);
            assert_eq!(
                next.frames.len() - 2,
                closure_nodes,
                "the stress receipt requires a stable current-root footprint"
            );
            let (ack, reclaim_polls) = install_back_to_back(&mut host, &next);
            assert_eq!(ack.source_version, next.source);
            assert_eq!(ack.host_revision, revision);
            if revision > 2 {
                observed_reclaim_barrier |= reclaim_polls > 0;
            }

            let mut output = [0_u8; HOST_M11_VIEWPORT_BYTES];
            assert!(matches!(
                host.query_structural(
                    query_for(
                        next.source,
                        HostSourceMetric {
                            bytes: next.source.utf8_length,
                            utf16: next.source.utf16_length,
                        },
                        HostMetricAffinity::Upstream,
                        FULL_QUERY_BUDGET,
                    ),
                    &mut output,
                )
                .expect("current revision query"),
                HostStructuralQueryOutcome::Viewport { .. }
            ));
        }
        assert!(
            observed_reclaim_barrier,
            "a third revision must wait for retirement in a two-root arena"
        );
        assert_eq!(
            host.installed_ack
                .expect("latest ACK")
                .source_version
                .revision,
            64,
            "the host retains only current authority, not revision ancestry"
        );
        close_host(&mut host);
    }

    #[test]
    fn recursive_green_schema9_zipper_admits_large_document_under_default_budget() {
        const TARGET: &str = "- a\n  > b\n  ```\n  c\n  ```\n- d\n";
        let paragraph = format!("{}\n\n", "x".repeat(1024));
        let padding = paragraph.repeat(300);
        let large_source = format!("{padding}{TARGET}{padding}");
        assert!(large_source.len() > 512 * 1024);

        let small_source = TARGET.to_owned();
        let mut receipts = Vec::new();
        let mut event_work = Vec::new();
        for (index, source) in [small_source, large_source].into_iter().enumerate() {
            let document = [0x321 + index as u32, 2, 3, 4];
            let publication = [0x323 + index as u32, 6, 7, 8];
            let point = source.find("> b").expect("nested quote") + 2;
            let snapshot = recursive_green_snapshot(document, publication, 1, 1, &source);
            let mut host = host_for(document);
            install(&mut host, &snapshot);

            let (engine, installed) = host.query_root().expect("installed Green query root");
            let descriptor = engine
                .persistent_recursive_green_descriptor(installed)
                .expect("Green descriptor query")
                .expect("recursive Green descriptor");
            if index == 1 {
                assert!(descriptor.event_count() > u64::from(FULL_QUERY_BUDGET.maximum_leaf_count));
            }
            let query = query_for(
                snapshot.source,
                HostSourceMetric {
                    bytes: u32::try_from(point).expect("point bytes"),
                    utf16: u32::try_from(source[..point].encode_utf16().count())
                        .expect("point UTF-16"),
                },
                HostMetricAffinity::Downstream,
                FULL_QUERY_BUDGET,
            );
            let mut output = vec![0xa5; FULL_QUERY_BUDGET.maximum_encoded_bytes as usize];
            let HostStructuralQueryOutcome::Viewport { range, receipt, .. } = host
                .query_structural(query, &mut output)
                .expect("default-budget Green query")
            else {
                panic!("recursive Green zipper must return schema 9 under the default budget");
            };
            assert!(range.start.bytes <= point as u32 && (point as u32) < range.end.bytes);
            assert_eq!(read_u32(&output, 8), HOST_RECURSIVE_GREEN_VIEWPORT_SCHEMA);
            assert_eq!(read_u32(&output, 36), 5);
            assert_eq!(read_u32(&output, 40), 4);
            assert_eq!(u16::from_le_bytes([output[44], output[45]]), 5);
            assert_eq!(output[46], 1, "nested `b` is parser-certified content");
            let events_scanned = read_u32(&output, 100);
            let storage_pages_visited = read_u32(&output, 104);
            assert!(events_scanned > 0);
            assert!(u64::from(events_scanned) <= descriptor.event_count());
            assert_eq!(receipt.leaf_count, storage_pages_visited);
            assert!(receipt.leaf_count <= FULL_QUERY_BUDGET.maximum_leaf_count);
            assert!(receipt.tree_nodes_visited <= FULL_QUERY_BUDGET.maximum_tree_nodes_visited);
            let kinds = (0..5)
                .map(|ancestor| {
                    let offset = HOST_RECURSIVE_GREEN_VIEWPORT_HEADER_BYTES
                        + ancestor * HOST_RECURSIVE_GREEN_ANCESTOR_RECORD_BYTES;
                    assert_ne!(read_u64(&output, offset), 0);
                    let flags = u16::from_le_bytes([output[offset + 10], output[offset + 11]]);
                    assert_eq!(
                        flags,
                        if ancestor == 4 {
                            HOST_RECURSIVE_GREEN_ANCESTOR_OWNER_FLAG
                        } else {
                            0
                        }
                    );
                    u16::from_le_bytes([output[offset + 8], output[offset + 9]])
                })
                .collect::<Vec<_>>();
            assert_eq!(kinds, vec![1, 3, 4, 2, 5]);
            assert_eq!(
                receipt.encoded_bytes as usize,
                HOST_RECURSIVE_GREEN_VIEWPORT_HEADER_BYTES
                    + 5 * HOST_RECURSIVE_GREEN_ANCESTOR_RECORD_BYTES
            );
            assert!(output[receipt.encoded_bytes as usize..]
                .iter()
                .all(|byte| *byte == 0xa5));

            if index == 1 {
                let mut under_budget = query;
                under_budget.budget.maximum_leaf_count = storage_pages_visited - 1;
                let mut untouched = vec![0xa5; output.len()];
                assert!(matches!(
                    host.query_structural(under_budget, &mut untouched)
                        .expect("under-budget Green query"),
                    HostStructuralQueryOutcome::SourceGap {
                        reason: HostSourceGapReason::LeafLimit,
                        ..
                    }
                ));
                assert!(untouched.iter().all(|byte| *byte == 0xa5));
            }
            receipts.push(receipt);
            event_work.push(events_scanned);
            close_host(&mut host);
        }

        assert!(
            receipts[1].leaf_count <= receipts[0].leaf_count + 4,
            "small={:?}, large={:?}, event_work={event_work:?}",
            receipts[0],
            receipts[1],
        );
        assert!(event_work[1] <= receipts[1].leaf_count * 128);
        assert!(receipts[1].tree_nodes_visited <= receipts[0].tree_nodes_visited + 128);
    }

    #[test]
    fn recursive_green_budget_gaps_return_only_admitted_receipts() {
        const SOURCE: &str = "- a\n  > b\n  ```\n  c\n  ```\n- d\n";
        let document = [0x341, 2, 3, 4];
        let snapshot = recursive_green_snapshot(document, [0x343, 6, 7, 8], 1, 1, SOURCE);
        let mut host = host_for(document);
        install(&mut host, &snapshot);

        let point = SOURCE.find("> b").expect("nested quote") + 2;
        let query = query_for(
            snapshot.source,
            HostSourceMetric {
                bytes: u32::try_from(point).expect("point bytes"),
                utf16: u32::try_from(SOURCE[..point].encode_utf16().count()).expect("point UTF-16"),
            },
            HostMetricAffinity::Downstream,
            FULL_QUERY_BUDGET,
        );
        let mut output = vec![0_u8; FULL_QUERY_BUDGET.maximum_encoded_bytes as usize];
        let HostStructuralQueryOutcome::Viewport { receipt, .. } = host
            .query_structural(query, &mut output)
            .expect("admitted recursive-Green query")
        else {
            panic!("recursive-Green fixture must produce a viewport");
        };
        assert!(receipt.open_depth > 0);
        assert!(receipt.leaf_count > 0);
        assert!(receipt.tree_nodes_visited > 0);

        let mut low_depth = query;
        low_depth.budget.maximum_open_depth = receipt.open_depth - 1;
        let mut low_leaf = query;
        low_leaf.budget.maximum_leaf_count = receipt.leaf_count - 1;
        let mut low_tree = query;
        low_tree.budget.maximum_tree_nodes_visited = receipt.tree_nodes_visited - 1;
        for (under_budget, expected_reason) in [
            (low_depth, HostSourceGapReason::OpenDepthLimit),
            (low_leaf, HostSourceGapReason::LeafLimit),
            (low_tree, HostSourceGapReason::TreeNodeLimit),
        ] {
            let mut untouched = vec![0xa5; output.len()];
            let HostStructuralQueryOutcome::SourceGap {
                reason,
                receipt: gap_receipt,
                ..
            } = host
                .query_structural(under_budget, &mut untouched)
                .expect("under-budget Green query")
            else {
                panic!("under-budget Green query must return a typed gap");
            };
            assert_eq!(reason, expected_reason);
            if expected_reason == HostSourceGapReason::TreeNodeLimit {
                // Hard tree fuel stops before attempting the first
                // out-of-authority header, so its exact consumed receipt is
                // safe to expose. Depth/leaf limits are checked after the
                // location is complete and therefore retain an empty receipt.
                assert_eq!(
                    gap_receipt.tree_nodes_visited,
                    under_budget.budget.maximum_tree_nodes_visited
                );
                assert!(gap_receipt.open_depth <= under_budget.budget.maximum_open_depth);
                assert!(gap_receipt.leaf_count <= under_budget.budget.maximum_leaf_count);
                assert_eq!(gap_receipt.encoded_bytes, 0);
            } else {
                assert_eq!(
                    gap_receipt,
                    HostViewportReceipt::default(),
                    "a post-check budget gap cannot claim out-of-authority work"
                );
            }
            assert!(untouched.iter().all(|byte| *byte == 0xa5));
        }
        close_host(&mut host);
    }

    #[test]
    fn recursive_green_row_range_retains_visible_suffix_after_leading_references() {
        const REFERENCES: usize = 128;
        let mut source = String::new();
        for ordinal in 0..REFERENCES {
            source.push_str(&format!("[r{ordinal}]: /target/{ordinal}\n"));
        }
        let visible_start = source.len();
        source.push_str("visible **bold** tail\n");
        let point = visible_start + "visible ".len();
        let document = [0x351, 2, 3, 4];
        let snapshot = recursive_green_snapshot(document, [0x353, 6, 7, 8], 1, 1, &source);
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let requested = HostMetricRange {
            start: HostSourceMetric {
                bytes: point as u32,
                utf16: point as u32,
            },
            end: HostSourceMetric {
                bytes: point as u32 + 1,
                utf16: point as u32 + 1,
            },
        };
        let budget = HostBlockRangeBudget {
            maximum_encoded_bytes: 4_096,
            maximum_block_count: 24,
            maximum_storage_pages_visited: 25,
            maximum_open_depth: 16,
            maximum_tree_nodes_visited: 512,
        };
        let mut output = vec![0xa5; budget.maximum_encoded_bytes as usize];
        let HostBlockRangeOutcome::Page {
            covered_range,
            continuation,
            receipt,
            ..
        } = host
            .query_structural_range(
                block_range_query(snapshot.source, requested, budget, None),
                &mut output,
            )
            .expect("reference-prefix recursive-Green row range")
        else {
            panic!("reference-prefix recursive-Green row range must be exact");
        };
        assert_eq!(covered_range, whole_source_range(snapshot.source));
        assert!(continuation.is_none());
        assert_eq!(receipt.block_count, 1);
        assert!(receipt.complete);
        assert!(receipt.storage_pages_visited <= 25);
        assert!(receipt.tree_nodes_visited <= 512);
        assert_eq!(&output[..8], BLOCK_RANGE_MAGIC);
        close_host(&mut host);
    }

    #[test]
    fn recursive_green_row_range_includes_terminal_empty_item_at_eof() {
        let source = "- alpha\n-   ";
        let document = [0x359, 2, 3, 4];
        let snapshot = recursive_green_snapshot(document, [0x35a, 6, 7, 8], 1, 1, source);
        let mut host = host_for(document);
        install(&mut host, &snapshot);
        let suffix_start = source.len() - 1;
        let requested = HostMetricRange {
            start: HostSourceMetric {
                bytes: suffix_start as u32,
                utf16: source[..suffix_start].encode_utf16().count() as u32,
            },
            end: HostSourceMetric {
                bytes: source.len() as u32,
                utf16: source.encode_utf16().count() as u32,
            },
        };
        let budget = HostBlockRangeBudget {
            maximum_encoded_bytes: 4_096,
            maximum_block_count: 1,
            maximum_storage_pages_visited: 25,
            maximum_open_depth: 16,
            maximum_tree_nodes_visited: 512,
        };
        let mut output = vec![0xa5; budget.maximum_encoded_bytes as usize];
        let HostBlockRangeOutcome::Page {
            covered_range,
            continuation,
            receipt,
            ..
        } = host
            .query_structural_range(
                block_range_query(snapshot.source, requested, budget, None),
                &mut output,
            )
            .expect("terminal-empty recursive-Green suffix range")
        else {
            panic!("terminal-empty suffix must return its collapsed row");
        };
        assert_eq!(covered_range, requested);
        assert!(continuation.is_none());
        assert!(receipt.complete);
        assert_eq!(receipt.block_count, 1);
        assert_eq!(read_u32(&output, 24), 1);
        assert_eq!(read_u32(&output, 28), 4);
        let row = HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES;
        assert_eq!(
            u16::from_le_bytes(output[row + 16..row + 18].try_into().expect("row kind")),
            14,
        );
        assert_eq!(read_u32(&output, row + 32), source.len() as u32);
        assert_eq!(read_u32(&output, row + 40), source.len() as u32);
        assert_eq!(read_u32(&output, row + 48), source.len() as u32);
        assert_eq!(read_u32(&output, row + 56), source.len() as u32);
        close_host(&mut host);
    }

    #[test]
    fn recursive_green_row_wire_hides_cached_geometry_but_keeps_semantic_facts() {
        let fixtures = [
            ("plain\n", 1_usize, 5_u16, 0_u16, false, false),
            ("# title\n", 3, 12, 3, true, false),
            ("```dart\ncode\n```\n", 9, 7, 4, true, true),
        ];
        for (ordinal, (source, point, owner_kind, fact_kind, has_open, has_close)) in
            fixtures.into_iter().enumerate()
        {
            let document = [0x361 + ordinal as u32, 2, 3, 4];
            let snapshot =
                recursive_green_snapshot(document, [0x371 + ordinal as u32, 6, 7, 8], 1, 1, source);
            let mut host = host_for(document);
            install(&mut host, &snapshot);
            let requested = HostMetricRange {
                start: HostSourceMetric {
                    bytes: point as u32,
                    utf16: point as u32,
                },
                end: HostSourceMetric {
                    bytes: point as u32 + 1,
                    utf16: point as u32 + 1,
                },
            };
            let budget = HostBlockRangeBudget {
                maximum_encoded_bytes: 4_096,
                maximum_block_count: 24,
                maximum_storage_pages_visited: 25,
                maximum_open_depth: 16,
                maximum_tree_nodes_visited: 512,
            };
            let mut output = vec![0xa5; budget.maximum_encoded_bytes as usize];
            let HostBlockRangeOutcome::Page { receipt, .. } = host
                .query_structural_range(
                    block_range_query(snapshot.source, requested, budget, None),
                    &mut output,
                )
                .expect("schema-11 recursive-Green row range")
            else {
                panic!("schema-11 fixture must produce one row");
            };
            assert_eq!(receipt.block_count, 1);
            let row_count = read_u32(&output, 24) as usize;
            let path_count = read_u32(&output, 28) as usize;
            assert_eq!(row_count, 1);
            assert!(path_count >= 2);
            let row_offset = HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES;
            let path_start = read_u32(&output, row_offset + 20) as usize;
            let path_len = read_u32(&output, row_offset + 24) as usize;
            let owner_ordinal = path_start + path_len - 1;
            let owner_offset = HOST_RECURSIVE_GREEN_ROW_RANGE_HEADER_BYTES
                + row_count * HOST_RECURSIVE_GREEN_ROW_RECORD_BYTES
                + owner_ordinal * HOST_RECURSIVE_GREEN_ROW_PATH_RECORD_BYTES;
            let encoded_owner_kind = u16::from_le_bytes(
                output[owner_offset + 8..owner_offset + 10]
                    .try_into()
                    .unwrap(),
            );
            let flags = u16::from_le_bytes(
                output[owner_offset + 10..owner_offset + 12]
                    .try_into()
                    .unwrap(),
            );
            let encoded_fact_kind = u16::from_le_bytes(
                output[owner_offset + 12..owner_offset + 14]
                    .try_into()
                    .unwrap(),
            );
            assert_eq!(encoded_owner_kind, owner_kind);
            assert_ne!(flags & HOST_RECURSIVE_GREEN_PATH_ROW_OWNER_FLAG, 0);
            assert_eq!(
                flags & HOST_RECURSIVE_GREEN_PATH_OPEN_FACT_FLAG != 0,
                has_open
            );
            assert_eq!(
                flags & HOST_RECURSIVE_GREEN_PATH_CLOSE_FACT_FLAG != 0,
                has_close
            );
            assert_eq!(encoded_fact_kind, fact_kind);
            close_host(&mut host);
        }
    }

    #[test]
    fn declared_offer_envelope_cannot_exceed_configured_host_limits() {
        let document = [71, 72, 73, 74];
        let snapshot = snapshot(document, [81, 82, 83, 84], 1, 1, 1);
        let limits = M11HostLimits {
            arena_max_slots: 16,
            maximum_snapshot_nodes: 16,
            maximum_snapshot_wire_bytes: 8 * 1024,
            arena_max_children_per_node: 8,
            ..M11HostLimits::default()
        };
        let mut host = NativeCandidateHost::new_with_limits(
            HostConfig {
                document_session: document,
                grammar_revision: 1,
                syntax_profile: 1,
                authority_mask: 0x1f,
                maximum_query_bytes: 64 * 1024,
            },
            limits,
        )
        .expect("bounded host");
        host.observe_source_version(snapshot.source)
            .expect("exact source");

        let mut too_many_frames = snapshot.offer;
        too_many_frames.limits.maximum_frame_count = 19;
        assert_eq!(
            host.begin_offer(too_many_frames)
                .expect_err("declared frame envelope must fit")
                .reason(),
            HostRejectReason::ForegroundBoundExceeded
        );

        let mut too_many_children = snapshot.offer;
        too_many_children.limits.maximum_program_children = 9;
        assert_eq!(
            host.begin_offer(too_many_children)
                .expect_err("declared child envelope must fit")
                .reason(),
            HostRejectReason::ForegroundBoundExceeded
        );

        host.begin_close().expect("host close");
        assert_eq!(
            host.poll(HostWorkGrant {
                inspect_bytes: 0,
                copy_bytes: 0,
                transitions: 1,
            })
            .expect("host close poll"),
            HostPollOutcome::Closed
        );
    }
}

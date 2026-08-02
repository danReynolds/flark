//! Narrow public ownership seam for the independent M1.1 candidate host.
//!
//! This module exposes values and byte-copy operations only. Producer arena
//! identities, build journals, and committed roots remain private to the
//! engine. Native FFI and a future main-context Wasm adapter can therefore use
//! the same host implementation without enabling the parser-internal feature.

use std::fmt;

use crate::block_quote_projection::{
    BlockQuoteLineV1, PersistentM11BlockQuoteProjectionDescriptor,
    PersistentM11BlockQuoteProjectionHostCursor, PersistentM11BlockQuoteProjectionHostCursorPoll,
};
use crate::block_sequence::{
    maximum_consecutive_block_visit_node_headers, M11BlockSequenceEntryKind,
    M11BlockSequenceLocation, M11BlockSequenceOrdinalWindow, M11BlockSequencePoint,
    M11BlockSequenceVisitControl, M11BlockSequenceVisitDisposition, M11BlockSequenceVisitEntry,
    M11BlockSequenceVisitStart, M11_BLOCK_SEQUENCE_ENTRIES_PER_PAGE_MAX,
};
use crate::candidate_manifest::{CandidateRole, ManifestError, StrongIdentity};
use crate::host_store::{
    classify_snapshot_frame, CandidateHostError, CandidateHostInstallPoll, CandidateHostLimits,
    CandidateHostReplayPoll, CandidateHostStore, InstalledCandidateSnapshot,
    InstalledPersistentBlockDescriptor, InstalledPersistentInlineProjectionDescriptor,
    InstalledPersistentRecursiveGreenDescriptor, SnapshotFrameKind, SnapshotFrameMetadata,
    M11_MAXIMUM_SNAPSHOT_CHILDREN, M11_MAXIMUM_SNAPSHOT_FRAME_BYTES,
};
use crate::identity::{SourceRevision, SourceRootId};
use crate::indented_code_projection::{
    IndentedCodeLineV1, PersistentM11IndentedCodeProjectionDescriptor,
    PersistentM11IndentedCodeProjectionHostCursor,
    PersistentM11IndentedCodeProjectionHostCursorPoll,
};
use crate::inline_overlay::{
    M11InlineOverlayBase, M11InlineOverlayBinding, M11InlineOverlayHostMatch,
    M11InlineOverlayHostStore, M11InlineOverlayInstallPoll, M11InlineOverlayOwner,
    M11InlineOverlayTransportError,
};
use crate::inline_projection::{
    encode_persistent_inline_link_values, M11InlineProjectionFact, M11InlineProjectionKind,
    PersistentM11InlineLinkValueEncodeReceipt, PersistentM11InlineProjectionDescriptor,
    PersistentM11InlineProjectionHostCursor, PersistentM11InlineProjectionHostCursorPoll,
};
use crate::recursive_green::{
    M11RecursiveGreenCoveragePart, M11RecursiveGreenLocation, M11RecursiveGreenLogicalAtom,
    M11RecursiveGreenPoint, M11RecursiveGreenRenderableRow, M11RecursiveGreenRowBudgetExceeded,
    M11RecursiveGreenRowEditCapability, M11RecursiveGreenRowOrdinalWindow,
    M11RecursiveGreenRowPathFrame, M11RecursiveGreenRowQueryLimit, M11RecursiveGreenRowQueryLimits,
    M11RecursiveGreenRowQueryOutcome, M11RecursiveGreenRowWindow,
};
use crate::source::{SourceBoundaryAffinity, SourceVersion};
use crate::storage::{ArenaError, PageArena};
use crate::ParserProfileId;

/// Hard byte ceiling for every self-contained M1.1 snapshot frame.
pub const M11_HOST_MAXIMUM_FRAME_BYTES: usize = M11_MAXIMUM_SNAPSHOT_FRAME_BYTES;
/// Hard child-arity ceiling represented by the M1.1 frame bound.
pub const M11_HOST_MAXIMUM_PROGRAM_CHILDREN: usize = M11_MAXIMUM_SNAPSHOT_CHILDREN;
/// Production slot envelope for one independently published M1.1 candidate.
/// The independent host reserves a separate overlap budget for replacement.
pub const M11_CANDIDATE_ARENA_MAX_SLOTS: usize = 131_072;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostLimits {
    pub arena_max_slots: usize,
    pub arena_max_live_payload_bytes: usize,
    pub arena_max_children_per_node: usize,
    pub maximum_snapshot_nodes: u64,
    pub maximum_snapshot_wire_bytes: u64,
    pub maximum_query_bytes: usize,
}

impl Default for M11HostLimits {
    fn default() -> Self {
        let limits = CandidateHostLimits::default();
        Self {
            arena_max_slots: limits.arena.max_slots,
            arena_max_live_payload_bytes: limits.arena.max_live_payload_bytes,
            arena_max_children_per_node: limits.arena.max_children_per_node,
            maximum_snapshot_nodes: limits.maximum_snapshot_nodes,
            maximum_snapshot_wire_bytes: limits.maximum_snapshot_wire_bytes,
            maximum_query_bytes: limits.maximum_query_bytes,
        }
    }
}

impl M11HostLimits {
    fn engine(self) -> CandidateHostLimits {
        CandidateHostLimits {
            arena: crate::storage::ArenaLimits {
                max_slots: self.arena_max_slots,
                max_live_payload_bytes: self.arena_max_live_payload_bytes,
                max_children_per_node: self.arena_max_children_per_node,
            },
            maximum_snapshot_nodes: self.maximum_snapshot_nodes,
            maximum_snapshot_wire_bytes: self.maximum_snapshot_wire_bytes,
            maximum_query_bytes: self.maximum_query_bytes,
        }
    }
}

/// Exact source authority observed by the host. It carries no source bytes or
/// parser-owned handles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostSourceVersion {
    pub source_root: u64,
    pub source_revision: u64,
    pub source_bytes: u64,
    pub source_utf16: u64,
}

impl M11HostSourceVersion {
    fn engine(self) -> Result<SourceVersion, M11HostError> {
        let root = SourceRootId::from_wire(self.source_root)
            .ok_or_else(|| M11HostError::invalid("source root must be nonzero"))?;
        let byte_len = usize::try_from(self.source_bytes)
            .map_err(|_| M11HostError::invalid("source byte length exceeds this target"))?;
        let utf16_len = usize::try_from(self.source_utf16)
            .map_err(|_| M11HostError::invalid("source UTF-16 length exceeds this target"))?;
        Ok(SourceVersion::from_authenticated_parts(
            SourceRevision::new(self.source_revision),
            root,
            byte_len,
            utf16_len,
        ))
    }
}

/// Stable semantic kind of one closed snapshot frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostFrameKind {
    Begin,
    SourceFactsReplacementPage,
    BlockSequenceReplacementPage,
    RecursiveGreenReplacementPage,
    Node,
    End,
}

/// Independently decoded frame metadata. Canonical records are counted by the
/// engine schema, never trusted from transport declarations. Exact-base
/// programs report only physically transferred records; reused canonical
/// content remains part of the target manifest but is not transfer credit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostFrameMetadata {
    pub kind: M11HostFrameKind,
    pub node_ordinal: Option<u64>,
    pub canonical_record_count: u32,
    pub canonical_stream_digest256: Option<[u8; 32]>,
}

impl From<SnapshotFrameMetadata> for M11HostFrameMetadata {
    fn from(metadata: SnapshotFrameMetadata) -> Self {
        Self {
            kind: match metadata.kind {
                SnapshotFrameKind::Begin => M11HostFrameKind::Begin,
                SnapshotFrameKind::SourceFactsReplacementPage => {
                    M11HostFrameKind::SourceFactsReplacementPage
                }
                SnapshotFrameKind::BlockSequenceReplacementPage => {
                    M11HostFrameKind::BlockSequenceReplacementPage
                }
                SnapshotFrameKind::RecursiveGreenReplacementPage => {
                    M11HostFrameKind::RecursiveGreenReplacementPage
                }
                SnapshotFrameKind::Node => M11HostFrameKind::Node,
                SnapshotFrameKind::End => M11HostFrameKind::End,
            },
            node_ordinal: metadata.node_ordinal,
            canonical_record_count: metadata.canonical_record_count,
            canonical_stream_digest256: metadata.canonical_stream_digest256,
        }
    }
}

/// Canonical role exposed through bounded record reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostRole {
    SourceFacts,
    Green,
    Projection,
    References,
    CleanEofOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostInlineProjectionDescriptor {
    source_start: u32,
    source_end: u32,
    structural_record_count: u64,
    logical_page_count: u64,
    fact_count: u64,
    storage_page_count: u64,
    link_value_entry_count: u32,
    link_value_storage_page_count: u64,
    link_value_encoded_bytes: u32,
    maximum_open_depth: u32,
    maximum_tree_nodes_visited: u64,
}

impl M11HostInlineProjectionDescriptor {
    #[must_use]
    pub const fn source_start(self) -> u32 {
        self.source_start
    }

    #[must_use]
    pub const fn source_end(self) -> u32 {
        self.source_end
    }

    #[must_use]
    pub const fn structural_record_count(self) -> u64 {
        self.structural_record_count
    }

    #[must_use]
    pub const fn logical_page_count(self) -> u64 {
        self.logical_page_count
    }

    #[must_use]
    pub const fn fact_count(self) -> u64 {
        self.fact_count
    }

    #[must_use]
    pub const fn storage_page_count(self) -> u64 {
        self.storage_page_count
    }

    #[must_use]
    pub const fn link_value_entry_count(self) -> u32 {
        self.link_value_entry_count
    }

    #[must_use]
    pub const fn link_value_storage_page_count(self) -> u64 {
        self.link_value_storage_page_count
    }

    #[must_use]
    pub const fn link_value_encoded_bytes(self) -> u32 {
        self.link_value_encoded_bytes
    }

    #[must_use]
    pub const fn maximum_open_depth(self) -> u32 {
        self.maximum_open_depth
    }

    #[must_use]
    pub const fn maximum_tree_nodes_visited(self) -> u64 {
        self.maximum_tree_nodes_visited
    }
}

impl From<InstalledPersistentInlineProjectionDescriptor> for M11HostInlineProjectionDescriptor {
    fn from(descriptor: InstalledPersistentInlineProjectionDescriptor) -> Self {
        Self {
            source_start: descriptor.source_start,
            source_end: descriptor.source_end,
            structural_record_count: descriptor.structural_record_count,
            logical_page_count: descriptor.logical_page_count,
            fact_count: descriptor.fact_count,
            storage_page_count: descriptor.storage_page_count,
            link_value_entry_count: descriptor.link_value_entry_count,
            link_value_storage_page_count: descriptor.link_value_storage_page_count,
            link_value_encoded_bytes: descriptor.link_value_encoded_bytes,
            maximum_open_depth: descriptor.maximum_open_depth,
            maximum_tree_nodes_visited: descriptor.maximum_tree_nodes_visited,
        }
    }
}

/// Boundary ownership used by one exact block point lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostBlockAffinity {
    Before,
    After,
}

impl M11HostBlockAffinity {
    const fn engine(self) -> SourceBoundaryAffinity {
        match self {
            Self::Before => SourceBoundaryAffinity::Before,
            Self::After => SourceBoundaryAffinity::After,
        }
    }
}

/// Authenticated worst-case plan for one point lookup over installed blocks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostPersistentBlockDescriptor {
    source_bytes: u64,
    source_utf16: u64,
    entry_count: u64,
    reference_definition_count: u64,
    storage_page_count: u64,
    tree_height: u16,
    maximum_entries_scanned: u32,
    maximum_tree_nodes_visited: u64,
}

impl M11HostPersistentBlockDescriptor {
    #[must_use]
    pub const fn source_bytes(self) -> u64 {
        self.source_bytes
    }

    #[must_use]
    pub const fn source_utf16(self) -> u64 {
        self.source_utf16
    }

    #[must_use]
    pub const fn entry_count(self) -> u64 {
        self.entry_count
    }

    #[must_use]
    pub const fn reference_definition_count(self) -> u64 {
        self.reference_definition_count
    }

    #[must_use]
    pub const fn storage_page_count(self) -> u64 {
        self.storage_page_count
    }

    #[must_use]
    pub const fn tree_height(self) -> u16 {
        self.tree_height
    }

    #[must_use]
    pub const fn maximum_entries_scanned(self) -> u32 {
        self.maximum_entries_scanned
    }

    #[must_use]
    pub const fn maximum_tree_nodes_visited(self) -> u64 {
        self.maximum_tree_nodes_visited
    }

    /// Conservative header-decode admission bound for one consecutive visit.
    ///
    /// A contiguous run spans at most one initial AVL boundary path, one
    /// terminal boundary path, and the minimal binary subtree connecting the
    /// admitted packed leaves. `7 * pages + 3 * height + 3` additionally
    /// covers the root preflight and direct-child header authentication
    /// performed by each checked branch decode.
    #[must_use]
    pub const fn maximum_consecutive_visit_node_headers(self, maximum_storage_pages: u32) -> u64 {
        maximum_consecutive_block_visit_node_headers(self.tree_height, maximum_storage_pages)
    }
}

impl From<InstalledPersistentBlockDescriptor> for M11HostPersistentBlockDescriptor {
    fn from(descriptor: InstalledPersistentBlockDescriptor) -> Self {
        Self {
            source_bytes: descriptor.source_bytes,
            source_utf16: descriptor.source_utf16,
            entry_count: descriptor.entry_count,
            reference_definition_count: descriptor.reference_definition_count,
            storage_page_count: descriptor.storage_page_count,
            tree_height: descriptor.tree_height,
            maximum_entries_scanned: u32::try_from(
                descriptor
                    .entry_count
                    .min(M11_BLOCK_SEQUENCE_ENTRIES_PER_PAGE_MAX as u64),
            )
            .expect("the packed block-page bound fits u32"),
            maximum_tree_nodes_visited: descriptor.maximum_tree_nodes_visited,
        }
    }
}

/// Authenticated shape of the installed persistent recursive Green role.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostPersistentRecursiveGreenDescriptor {
    source_bytes: u64,
    source_utf16: u64,
    event_count: u64,
    renderable_row_count: u64,
    storage_page_count: u64,
    tree_height: u16,
}

impl M11HostPersistentRecursiveGreenDescriptor {
    #[must_use]
    pub const fn source_bytes(self) -> u64 {
        self.source_bytes
    }

    #[must_use]
    pub const fn source_utf16(self) -> u64 {
        self.source_utf16
    }

    #[must_use]
    pub const fn event_count(self) -> u64 {
        self.event_count
    }

    #[must_use]
    pub const fn renderable_row_count(self) -> u64 {
        self.renderable_row_count
    }

    #[must_use]
    pub const fn storage_page_count(self) -> u64 {
        self.storage_page_count
    }

    #[must_use]
    pub const fn tree_height(self) -> u16 {
        self.tree_height
    }
}

impl From<InstalledPersistentRecursiveGreenDescriptor>
    for M11HostPersistentRecursiveGreenDescriptor
{
    fn from(descriptor: InstalledPersistentRecursiveGreenDescriptor) -> Self {
        Self {
            source_bytes: descriptor.source_bytes,
            source_utf16: descriptor.source_utf16,
            event_count: descriptor.event_count,
            renderable_row_count: descriptor.renderable_row_count,
            storage_page_count: descriptor.storage_page_count,
            tree_height: descriptor.tree_height,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostRecursiveGreenAncestor {
    frame_id: u64,
    kind: u16,
}

impl M11HostRecursiveGreenAncestor {
    #[must_use]
    pub const fn frame_id(self) -> u64 {
        self.frame_id
    }

    #[must_use]
    pub const fn kind(self) -> u16 {
        self.kind
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostRecursiveGreenCoveragePart {
    Content,
    ContainerMarker,
    BlockMarker,
    Gap,
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostRecursiveGreenLogicalAtom {
    None,
    Identity,
    TabToSpaces { target_owner_depth: u32, spaces: u8 },
    HiddenUpstream,
    LfToLf,
    CrLfToLf,
    LoneCrToLf,
    NulToReplacement,
}

/// One independently authenticated source point and its final Green ancestry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11HostRecursiveGreenLocation(M11RecursiveGreenLocation);

impl M11HostRecursiveGreenLocation {
    #[must_use]
    pub fn byte_start(&self) -> u64 {
        self.0.byte_range().start
    }

    #[must_use]
    pub fn byte_end(&self) -> u64 {
        self.0.byte_range().end
    }

    #[must_use]
    pub fn utf16_start(&self) -> u64 {
        self.0.utf16_range().start
    }

    #[must_use]
    pub fn utf16_end(&self) -> u64 {
        self.0.utf16_range().end
    }

    #[must_use]
    pub const fn owner_index(&self) -> usize {
        self.0.owner_index()
    }

    #[must_use]
    pub fn ancestry_len(&self) -> usize {
        self.0.ancestry().len()
    }

    #[must_use]
    pub fn ancestor(&self, index: usize) -> Option<M11HostRecursiveGreenAncestor> {
        self.0
            .ancestry()
            .get(index)
            .map(|ancestor| M11HostRecursiveGreenAncestor {
                frame_id: ancestor.frame().get(),
                kind: ancestor.kind().get(),
            })
    }

    #[must_use]
    pub fn owner(&self) -> M11HostRecursiveGreenAncestor {
        let owner = self.0.owner();
        M11HostRecursiveGreenAncestor {
            frame_id: owner.frame().get(),
            kind: owner.kind().get(),
        }
    }

    #[must_use]
    pub const fn physical_bytes(&self) -> u64 {
        self.0.physical_metric().bytes()
    }

    #[must_use]
    pub const fn physical_utf16(&self) -> u64 {
        self.0.physical_metric().utf16()
    }

    #[must_use]
    pub const fn logical_bytes(&self) -> u64 {
        self.0.logical_metric().bytes()
    }

    #[must_use]
    pub const fn logical_utf16(&self) -> u64 {
        self.0.logical_metric().utf16()
    }

    #[must_use]
    pub const fn part(&self) -> M11HostRecursiveGreenCoveragePart {
        match self.0.part() {
            M11RecursiveGreenCoveragePart::Content => M11HostRecursiveGreenCoveragePart::Content,
            M11RecursiveGreenCoveragePart::ContainerMarker => {
                M11HostRecursiveGreenCoveragePart::ContainerMarker
            }
            M11RecursiveGreenCoveragePart::BlockMarker => {
                M11HostRecursiveGreenCoveragePart::BlockMarker
            }
            M11RecursiveGreenCoveragePart::Gap => M11HostRecursiveGreenCoveragePart::Gap,
            M11RecursiveGreenCoveragePart::Terminal => M11HostRecursiveGreenCoveragePart::Terminal,
        }
    }

    #[must_use]
    pub const fn logical_atom(&self) -> M11HostRecursiveGreenLogicalAtom {
        match self.0.logical_atom() {
            M11RecursiveGreenLogicalAtom::None => M11HostRecursiveGreenLogicalAtom::None,
            M11RecursiveGreenLogicalAtom::Identity => M11HostRecursiveGreenLogicalAtom::Identity,
            M11RecursiveGreenLogicalAtom::TabToSpaces {
                target_owner_depth,
                spaces,
            } => M11HostRecursiveGreenLogicalAtom::TabToSpaces {
                target_owner_depth,
                spaces,
            },
            M11RecursiveGreenLogicalAtom::HiddenUpstream => {
                M11HostRecursiveGreenLogicalAtom::HiddenUpstream
            }
            M11RecursiveGreenLogicalAtom::LfToLf => M11HostRecursiveGreenLogicalAtom::LfToLf,
            M11RecursiveGreenLogicalAtom::CrLfToLf => M11HostRecursiveGreenLogicalAtom::CrLfToLf,
            M11RecursiveGreenLogicalAtom::LoneCrToLf => {
                M11HostRecursiveGreenLogicalAtom::LoneCrToLf
            }
            M11RecursiveGreenLogicalAtom::NulToReplacement => {
                M11HostRecursiveGreenLogicalAtom::NulToReplacement
            }
        }
    }

    #[must_use]
    pub const fn storage_pages_visited(&self) -> u64 {
        self.0.receipt().storage_pages_visited()
    }

    #[must_use]
    pub const fn events_scanned(&self) -> u64 {
        self.0.receipt().events_scanned()
    }

    #[must_use]
    pub const fn maximum_open_depth(&self) -> usize {
        self.0.receipt().maximum_open_depth()
    }

    #[must_use]
    pub const fn node_headers_decoded(&self) -> u64 {
        self.0.receipt().node_headers_decoded()
    }

    #[must_use]
    pub const fn summary_combinations(&self) -> u64 {
        self.0.receipt().summary_combinations()
    }
}

pub struct M11HostRecursiveGreenRowWindow(M11RecursiveGreenRowWindow);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostRecursiveGreenRowQueryLimit {
    StoragePages,
    EventsScanned,
    TreeNodes,
    OpenDepth,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostRecursiveGreenRowBudgetExceeded(M11RecursiveGreenRowBudgetExceeded);

impl M11HostRecursiveGreenRowBudgetExceeded {
    #[must_use]
    pub const fn limit(self) -> M11HostRecursiveGreenRowQueryLimit {
        match self.0.limit() {
            M11RecursiveGreenRowQueryLimit::StoragePages => {
                M11HostRecursiveGreenRowQueryLimit::StoragePages
            }
            M11RecursiveGreenRowQueryLimit::EventsScanned => {
                M11HostRecursiveGreenRowQueryLimit::EventsScanned
            }
            M11RecursiveGreenRowQueryLimit::TreeNodes => {
                M11HostRecursiveGreenRowQueryLimit::TreeNodes
            }
            M11RecursiveGreenRowQueryLimit::OpenDepth => {
                M11HostRecursiveGreenRowQueryLimit::OpenDepth
            }
        }
    }

    #[must_use]
    pub const fn storage_pages_visited(self) -> u64 {
        self.0.receipt().storage_pages_visited()
    }

    #[must_use]
    pub const fn events_scanned(self) -> u64 {
        self.0.receipt().events_scanned()
    }

    #[must_use]
    pub const fn maximum_open_depth(self) -> usize {
        self.0.receipt().maximum_open_depth()
    }

    #[must_use]
    pub const fn node_headers_decoded(self) -> u64 {
        self.0.receipt().node_headers_decoded()
    }

    #[must_use]
    pub const fn summary_combinations(self) -> u64 {
        self.0.receipt().summary_combinations()
    }
}

pub enum M11HostRecursiveGreenRowQueryOutcome {
    Window(M11HostRecursiveGreenRowWindow),
    BudgetExceeded(M11HostRecursiveGreenRowBudgetExceeded),
}

impl M11HostRecursiveGreenRowWindow {
    #[must_use]
    pub const fn start_ordinal(&self) -> u64 {
        self.0.start_ordinal()
    }
    #[must_use]
    pub const fn total_rows(&self) -> u64 {
        self.0.total_rows()
    }
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.0.complete()
    }
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.0.rows().len()
    }
    #[must_use]
    pub fn row(&self, index: usize) -> Option<M11HostRecursiveGreenRow<'_>> {
        self.0.rows().get(index).map(M11HostRecursiveGreenRow)
    }
    #[must_use]
    pub const fn storage_pages_visited(&self) -> u64 {
        self.0.receipt().storage_pages_visited()
    }
    #[must_use]
    pub const fn events_scanned(&self) -> u64 {
        self.0.receipt().events_scanned()
    }
    #[must_use]
    pub const fn maximum_open_depth(&self) -> usize {
        self.0.receipt().maximum_open_depth()
    }
    #[must_use]
    pub const fn node_headers_decoded(&self) -> u64 {
        self.0.receipt().node_headers_decoded()
    }
    #[must_use]
    pub const fn summary_combinations(&self) -> u64 {
        self.0.receipt().summary_combinations()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostRecursiveGreenRowOrdinalWindow(M11RecursiveGreenRowOrdinalWindow);

impl M11HostRecursiveGreenRowOrdinalWindow {
    #[must_use]
    pub const fn total_entry_count(self) -> u64 {
        self.0.total_rows()
    }
    #[must_use]
    pub const fn start_entry_ordinal(self) -> u64 {
        self.0.start_ordinal()
    }
    #[must_use]
    pub const fn next_entry_ordinal(self) -> u64 {
        self.0.next_ordinal()
    }
    #[must_use]
    pub const fn start_byte_offset(self) -> u64 {
        self.0.start_bytes()
    }
    #[must_use]
    pub const fn start_utf16_offset(self) -> u64 {
        self.0.start_utf16()
    }
    #[must_use]
    pub const fn next_byte_offset(self) -> u64 {
        self.0.next_bytes()
    }
    #[must_use]
    pub const fn next_utf16_offset(self) -> u64 {
        self.0.next_utf16()
    }
    #[must_use]
    pub const fn complete(self) -> bool {
        self.0.complete()
    }
    #[must_use]
    pub const fn storage_pages_visited(self) -> u64 {
        self.0.receipt().storage_pages_visited()
    }
    #[must_use]
    pub const fn node_headers_decoded(self) -> u64 {
        self.0.receipt().node_headers_decoded()
    }
    #[must_use]
    pub const fn summary_combinations(self) -> u64 {
        self.0.receipt().summary_combinations()
    }
    #[must_use]
    pub const fn packed_entries_inspected(self) -> u64 {
        self.0.receipt().events_scanned()
    }
}

#[derive(Clone, Copy)]
pub struct M11HostRecursiveGreenRow<'a>(&'a M11RecursiveGreenRenderableRow);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostRecursiveGreenRowEditCapability {
    Contiguous,
    ProjectedReserved,
    Unavailable,
}

impl<'a> M11HostRecursiveGreenRow<'a> {
    #[must_use]
    pub const fn ordinal(self) -> u64 {
        self.0.ordinal()
    }
    #[must_use]
    pub const fn frame_id(self) -> u64 {
        self.0.frame().get()
    }
    #[must_use]
    pub const fn kind(self) -> u16 {
        self.0.kind().get()
    }
    #[must_use]
    pub fn byte_start(self) -> u64 {
        self.0.physical_range().start
    }
    #[must_use]
    pub fn byte_end(self) -> u64 {
        self.0.physical_range().end
    }
    #[must_use]
    pub fn utf16_start(self) -> u64 {
        self.0.physical_utf16_range().start
    }
    #[must_use]
    pub fn utf16_end(self) -> u64 {
        self.0.physical_utf16_range().end
    }
    #[must_use]
    pub const fn edit_capability(self) -> M11HostRecursiveGreenRowEditCapability {
        match self.0.edit_capability() {
            M11RecursiveGreenRowEditCapability::Contiguous => {
                M11HostRecursiveGreenRowEditCapability::Contiguous
            }
            M11RecursiveGreenRowEditCapability::ProjectedReserved => {
                M11HostRecursiveGreenRowEditCapability::ProjectedReserved
            }
            M11RecursiveGreenRowEditCapability::Unavailable => {
                M11HostRecursiveGreenRowEditCapability::Unavailable
            }
        }
    }
    #[must_use]
    pub fn editable_byte_start(self) -> Option<u64> {
        self.0.editable_range().map(|range| range.start)
    }
    #[must_use]
    pub fn editable_byte_end(self) -> Option<u64> {
        self.0.editable_range().map(|range| range.end)
    }
    #[must_use]
    pub fn editable_utf16_start(self) -> Option<u64> {
        self.0.editable_utf16_range().map(|range| range.start)
    }
    #[must_use]
    pub fn editable_utf16_end(self) -> Option<u64> {
        self.0.editable_utf16_range().map(|range| range.end)
    }
    #[must_use]
    pub fn path_len(self) -> usize {
        self.0.path().len()
    }
    #[must_use]
    pub fn path(self, index: usize) -> Option<M11HostRecursiveGreenRowPath<'a>> {
        self.0.path().get(index).map(M11HostRecursiveGreenRowPath)
    }
}

#[derive(Clone, Copy)]
pub struct M11HostRecursiveGreenRowPath<'a>(&'a M11RecursiveGreenRowPathFrame);

impl M11HostRecursiveGreenRowPath<'_> {
    #[must_use]
    pub const fn frame_id(self) -> u64 {
        self.0.frame().get()
    }
    #[must_use]
    pub const fn kind(self) -> u16 {
        self.0.kind().get()
    }
    #[must_use]
    pub fn byte_start(self) -> u64 {
        self.0.physical_range().start
    }
    #[must_use]
    pub fn byte_end(self) -> u64 {
        self.0.physical_range().end
    }
    #[must_use]
    pub fn utf16_start(self) -> u64 {
        self.0.physical_utf16_range().start
    }
    #[must_use]
    pub fn utf16_end(self) -> u64 {
        self.0.physical_utf16_range().end
    }
    #[must_use]
    pub fn property_tag(self) -> u16 {
        self.0.property().map_or(0, |fact| fact.tag().get())
    }
    #[must_use]
    pub fn property_len(self) -> usize {
        self.0.property().map_or(0, |fact| fact.as_bytes().len())
    }
    #[must_use]
    pub fn property_byte(self, index: usize) -> Option<u8> {
        self.0
            .property()
            .and_then(|fact| fact.as_bytes().get(index).copied())
    }
    #[must_use]
    pub fn close_tag(self) -> u16 {
        self.0.close().map_or(0, |fact| fact.tag().get())
    }
    #[must_use]
    pub fn close_len(self) -> usize {
        self.0.close().map_or(0, |fact| fact.as_bytes().len())
    }
    #[must_use]
    pub fn close_byte(self, index: usize) -> Option<u8> {
        self.0
            .close()
            .and_then(|fact| fact.as_bytes().get(index).copied())
    }
}

/// One exact structural ordinal window located from the installed measured
/// block sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostPersistentBlockOrdinalWindow {
    inner: M11BlockSequenceOrdinalWindow,
}

impl M11HostPersistentBlockOrdinalWindow {
    #[must_use]
    pub const fn total_entry_count(self) -> u64 {
        self.inner.total_entry_count()
    }

    #[must_use]
    pub const fn start_entry_ordinal(self) -> u64 {
        self.inner.start_entry_ordinal()
    }

    #[must_use]
    pub const fn next_entry_ordinal(self) -> u64 {
        self.inner.next_entry_ordinal()
    }

    #[must_use]
    pub const fn start_byte_offset(self) -> u64 {
        self.inner.start_byte_offset()
    }

    #[must_use]
    pub const fn start_utf16_offset(self) -> u64 {
        self.inner.start_utf16_offset()
    }

    #[must_use]
    pub const fn next_byte_offset(self) -> u64 {
        self.inner.next_byte_offset()
    }

    #[must_use]
    pub const fn next_utf16_offset(self) -> u64 {
        self.inner.next_utf16_offset()
    }

    #[must_use]
    pub const fn complete(self) -> bool {
        self.inner.complete()
    }

    #[must_use]
    pub const fn storage_pages_visited(self) -> u64 {
        self.inner.receipt().storage_pages_visited()
    }

    #[must_use]
    pub const fn node_headers_decoded(self) -> u64 {
        self.inner.receipt().node_headers_decoded()
    }

    #[must_use]
    pub const fn summary_combinations(self) -> u64 {
        self.inner.receipt().summary_combinations()
    }

    #[must_use]
    pub const fn packed_entries_inspected(self) -> u64 {
        self.inner.receipt().packed_entries_inspected()
    }
}

/// Semantic kind of one selected exact source-coverage entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostBlockKind {
    Paragraph,
    Structured,
    Blank,
    DefinitionsOnly,
    Unsupported,
}

const fn host_block_kind(kind: M11BlockSequenceEntryKind) -> M11HostBlockKind {
    match kind {
        M11BlockSequenceEntryKind::Paragraph => M11HostBlockKind::Paragraph,
        M11BlockSequenceEntryKind::Structured => M11HostBlockKind::Structured,
        M11BlockSequenceEntryKind::Blank => M11HostBlockKind::Blank,
        M11BlockSequenceEntryKind::DefinitionsOnly => M11HostBlockKind::DefinitionsOnly,
        M11BlockSequenceEntryKind::Unsupported => M11HostBlockKind::Unsupported,
    }
}

/// Nonzero parser-owned reason code for literal unsupported coverage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostBlockUnsupportedReason(u32);

impl M11HostBlockUnsupportedReason {
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Complete bounded work actually performed by one block point lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostBlockQueryReceipt {
    node_headers_decoded: u64,
    summary_combinations: u64,
    payload_bytes_inspected: u64,
    entries_authenticated: u64,
    entries_scanned: u32,
}

impl M11HostBlockQueryReceipt {
    #[must_use]
    pub const fn node_headers_decoded(self) -> u64 {
        self.node_headers_decoded
    }

    #[must_use]
    pub const fn summary_combinations(self) -> u64 {
        self.summary_combinations
    }

    #[must_use]
    pub const fn payload_bytes_inspected(self) -> u64 {
        self.payload_bytes_inspected
    }

    #[must_use]
    pub const fn entries_authenticated(self) -> u64 {
        self.entries_authenticated
    }

    #[must_use]
    pub const fn entries_scanned(self) -> u32 {
        self.entries_scanned
    }
}

/// One host-owned value view over a selected persistent block entry.
///
/// Its records are authority-free and leaf-relative. Consumers must validate
/// them before translating any fixed source range into absolute coordinates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11HostPersistentBlockLocation(M11BlockSequenceLocation);

impl M11HostPersistentBlockLocation {
    #[must_use]
    pub const fn entry_ordinal(&self) -> u64 {
        self.0.entry_ordinal()
    }

    #[must_use]
    pub const fn storage_page_ordinal(&self) -> u64 {
        self.0.storage_page_ordinal()
    }

    #[must_use]
    pub fn byte_start(&self) -> u64 {
        self.0.byte_range().start
    }

    #[must_use]
    pub fn byte_end(&self) -> u64 {
        self.0.byte_range().end
    }

    #[must_use]
    pub fn utf16_start(&self) -> u64 {
        self.0.utf16_range().start
    }

    #[must_use]
    pub fn utf16_end(&self) -> u64 {
        self.0.utf16_range().end
    }

    #[must_use]
    pub fn kind(&self) -> M11HostBlockKind {
        host_block_kind(self.0.entry().kind())
    }

    #[must_use]
    pub const fn source_bytes(&self) -> u64 {
        self.0.entry().source_byte_len()
    }

    #[must_use]
    pub const fn source_utf16(&self) -> u64 {
        self.0.entry().source_utf16_len()
    }

    #[must_use]
    pub const fn reference_definition_count(&self) -> u64 {
        self.0.entry().reference_definition_count()
    }

    #[must_use]
    pub fn unsupported_reason(&self) -> Option<M11HostBlockUnsupportedReason> {
        self.0
            .entry()
            .unsupported_reason()
            .map(|reason| M11HostBlockUnsupportedReason(reason.get()))
    }

    #[must_use]
    pub fn green_record(&self) -> Option<&[u8]> {
        self.0.entry().green().map(|record| record.as_bytes())
    }

    #[must_use]
    pub fn projection_record(&self) -> Option<&[u8]> {
        self.0.entry().projection().map(|record| record.as_bytes())
    }

    #[must_use]
    pub fn receipt(&self) -> M11HostBlockQueryReceipt {
        let receipt = self.0.receipt();
        M11HostBlockQueryReceipt {
            node_headers_decoded: receipt.node_headers_decoded(),
            summary_combinations: receipt.summary_combinations(),
            payload_bytes_inspected: receipt.payload_bytes_inspected(),
            entries_authenticated: receipt.entries_authenticated(),
            entries_scanned: u32::from(receipt.entries_scanned()),
        }
    }
}

/// Exact semantic resume point for one bounded consecutive block visit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostPersistentBlockVisitStart {
    entry_ordinal: u64,
    byte_offset: u64,
    utf16_offset: u64,
}

impl M11HostPersistentBlockVisitStart {
    #[must_use]
    pub const fn new(entry_ordinal: u64, byte_offset: u64, utf16_offset: u64) -> Self {
        Self {
            entry_ordinal,
            byte_offset,
            utf16_offset,
        }
    }

    #[must_use]
    pub const fn entry_ordinal(self) -> u64 {
        self.entry_ordinal
    }

    #[must_use]
    pub const fn byte_offset(self) -> u64 {
        self.byte_offset
    }

    #[must_use]
    pub const fn utf16_offset(self) -> u64 {
        self.utf16_offset
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostPersistentBlockVisitControl {
    Continue,
    Stop,
}

/// One synchronously borrowed block entry with exact absolute source geometry.
///
/// Semantic ordinal and dual-coordinate ranges cross this seam. Packed-page
/// ordinals, arena identities, and measured-tree paths do not.
#[derive(Clone, Copy, Debug)]
pub struct M11HostPersistentBlockVisitEntry<'entry> {
    inner: M11BlockSequenceVisitEntry<'entry>,
}

impl<'entry> M11HostPersistentBlockVisitEntry<'entry> {
    #[must_use]
    pub const fn entry_ordinal(self) -> u64 {
        self.inner.entry_ordinal()
    }

    #[must_use]
    pub const fn byte_start(self) -> u64 {
        self.inner.byte_start()
    }

    #[must_use]
    pub const fn byte_end(self) -> u64 {
        self.inner.byte_end()
    }

    #[must_use]
    pub const fn utf16_start(self) -> u64 {
        self.inner.utf16_start()
    }

    #[must_use]
    pub const fn utf16_end(self) -> u64 {
        self.inner.utf16_end()
    }

    #[must_use]
    pub fn kind(self) -> M11HostBlockKind {
        host_block_kind(self.inner.entry().kind())
    }

    #[must_use]
    pub const fn source_bytes(self) -> u64 {
        self.inner.entry().source_byte_len()
    }

    #[must_use]
    pub const fn source_utf16(self) -> u64 {
        self.inner.entry().source_utf16_len()
    }

    #[must_use]
    pub const fn reference_definition_count(self) -> u64 {
        self.inner.entry().reference_definition_count()
    }

    #[must_use]
    pub fn unsupported_reason(self) -> Option<M11HostBlockUnsupportedReason> {
        self.inner
            .entry()
            .unsupported_reason()
            .map(|reason| M11HostBlockUnsupportedReason(reason.get()))
    }

    #[must_use]
    pub fn green_record(self) -> Option<&'entry [u8]> {
        self.inner.entry().green().map(|record| record.as_bytes())
    }

    #[must_use]
    pub fn projection_record(self) -> Option<&'entry [u8]> {
        self.inner
            .entry()
            .projection()
            .map(|record| record.as_bytes())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostPersistentBlockVisitDisposition {
    Complete,
    EntryLimit,
    StoragePageLimit,
    VisitorStopped,
}

/// Aggregate work and exact next semantic cut from one bounded direct visit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostPersistentBlockVisitReceipt {
    visited_entries: u64,
    storage_pages_visited: u64,
    next_entry_ordinal: u64,
    next_byte_offset: u64,
    next_utf16_offset: u64,
    disposition: M11HostPersistentBlockVisitDisposition,
    node_headers_decoded: u64,
    summary_combinations: u64,
    payload_bytes_inspected: u64,
    entries_authenticated: u64,
}

impl M11HostPersistentBlockVisitReceipt {
    #[must_use]
    pub const fn visited_entries(self) -> u64 {
        self.visited_entries
    }

    #[must_use]
    pub const fn storage_pages_visited(self) -> u64 {
        self.storage_pages_visited
    }

    #[must_use]
    pub const fn next_entry_ordinal(self) -> u64 {
        self.next_entry_ordinal
    }

    #[must_use]
    pub const fn next_byte_offset(self) -> u64 {
        self.next_byte_offset
    }

    #[must_use]
    pub const fn next_utf16_offset(self) -> u64 {
        self.next_utf16_offset
    }

    #[must_use]
    pub const fn disposition(self) -> M11HostPersistentBlockVisitDisposition {
        self.disposition
    }

    #[must_use]
    pub const fn node_headers_decoded(self) -> u64 {
        self.node_headers_decoded
    }

    #[must_use]
    pub const fn summary_combinations(self) -> u64 {
        self.summary_combinations
    }

    #[must_use]
    pub const fn payload_bytes_inspected(self) -> u64 {
        self.payload_bytes_inspected
    }

    #[must_use]
    pub const fn entries_authenticated(self) -> u64 {
        self.entries_authenticated
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum M11HostInlineProjectionKind {
    Emphasis = 1,
    Strong = 2,
    Code = 3,
    Strikethrough = 4,
    AutolinkUri = 5,
    AutolinkEmail = 6,
    BackslashEscape = 7,
    HardLineBreak = 8,
    CharacterReference = 9,
    DirectLink = 10,
    DirectImage = 11,
    ReferenceLink = 12,
    ReferenceImage = 13,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum M11HostInlineProjectionFactPayload {
    Marked {
        content_offset: u32,
        content_len: u32,
    },
    CharacterReference {
        first: char,
        second: Option<char>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostInlineProjectionFact {
    kind: M11HostInlineProjectionKind,
    flags: u8,
    relative_start: u32,
    relative_len: u32,
    payload: M11HostInlineProjectionFactPayload,
}

impl M11HostInlineProjectionFact {
    #[must_use]
    pub const fn kind(self) -> M11HostInlineProjectionKind {
        self.kind
    }

    #[must_use]
    pub const fn flags(self) -> u8 {
        self.flags
    }

    #[must_use]
    pub const fn relative_start(self) -> u32 {
        self.relative_start
    }

    #[must_use]
    pub const fn relative_len(self) -> u32 {
        self.relative_len
    }

    #[must_use]
    pub const fn content_offset(self) -> u32 {
        match self.payload {
            M11HostInlineProjectionFactPayload::Marked { content_offset, .. } => content_offset,
            M11HostInlineProjectionFactPayload::CharacterReference { .. } => 0,
        }
    }

    #[must_use]
    pub const fn content_len(self) -> u32 {
        match self.payload {
            M11HostInlineProjectionFactPayload::Marked { content_len, .. } => content_len,
            M11HostInlineProjectionFactPayload::CharacterReference { .. } => self.relative_len,
        }
    }

    /// Parser-cooked Unicode scalar value(s) for one character reference.
    #[must_use]
    pub const fn character_reference(self) -> Option<(char, Option<char>)> {
        match self.payload {
            M11HostInlineProjectionFactPayload::CharacterReference { first, second } => {
                Some((first, second))
            }
            M11HostInlineProjectionFactPayload::Marked { .. } => None,
        }
    }
}

impl From<M11InlineProjectionFact> for M11HostInlineProjectionFact {
    fn from(fact: M11InlineProjectionFact) -> Self {
        let relative = fact.relative_range();
        let payload = match fact.character_reference() {
            Some((first, second)) => {
                M11HostInlineProjectionFactPayload::CharacterReference { first, second }
            }
            None => {
                let content = fact.relative_content_range();
                M11HostInlineProjectionFactPayload::Marked {
                    content_offset: content.start - relative.start,
                    content_len: content.end - content.start,
                }
            }
        };
        Self {
            kind: match fact.kind() {
                M11InlineProjectionKind::Emphasis => M11HostInlineProjectionKind::Emphasis,
                M11InlineProjectionKind::Strong => M11HostInlineProjectionKind::Strong,
                M11InlineProjectionKind::Code => M11HostInlineProjectionKind::Code,
                M11InlineProjectionKind::Strikethrough => {
                    M11HostInlineProjectionKind::Strikethrough
                }
                M11InlineProjectionKind::AutolinkUri => M11HostInlineProjectionKind::AutolinkUri,
                M11InlineProjectionKind::AutolinkEmail => {
                    M11HostInlineProjectionKind::AutolinkEmail
                }
                M11InlineProjectionKind::BackslashEscape => {
                    M11HostInlineProjectionKind::BackslashEscape
                }
                M11InlineProjectionKind::HardLineBreak => {
                    M11HostInlineProjectionKind::HardLineBreak
                }
                M11InlineProjectionKind::CharacterReference => {
                    M11HostInlineProjectionKind::CharacterReference
                }
                M11InlineProjectionKind::DirectLink => M11HostInlineProjectionKind::DirectLink,
                M11InlineProjectionKind::DirectImage => M11HostInlineProjectionKind::DirectImage,
                M11InlineProjectionKind::ReferenceLink => {
                    M11HostInlineProjectionKind::ReferenceLink
                }
                M11InlineProjectionKind::ReferenceImage => {
                    M11HostInlineProjectionKind::ReferenceImage
                }
            },
            flags: fact.flags(),
            relative_start: relative.start,
            relative_len: relative.end - relative.start,
            payload,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostInlineProjectionCursorPoll {
    Fact(M11HostInlineProjectionFact),
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostInlineLinkValueCopyReceipt {
    pub entry_count: u32,
    pub tree_nodes_visited: u64,
}

impl From<PersistentM11InlineLinkValueEncodeReceipt> for M11HostInlineLinkValueCopyReceipt {
    fn from(receipt: PersistentM11InlineLinkValueEncodeReceipt) -> Self {
        Self {
            entry_count: receipt.entry_count,
            tree_nodes_visited: receipt.tree_nodes_visited,
        }
    }
}

/// Opaque typed cursor over an already-installed and validated persistent
/// inline Projection. Raw arena identities and `IFP2` bytes never cross this
/// boundary.
pub struct M11HostInlineProjectionCursor<'host> {
    inner: PersistentM11InlineProjectionHostCursor<'host>,
}

impl M11HostInlineProjectionCursor<'_> {
    pub fn poll(&mut self) -> Result<M11HostInlineProjectionCursorPoll, M11HostError> {
        match self.inner.poll().map_err(|_| {
            M11HostError::invalid("installed persistent Projection cursor became invalid")
        })? {
            PersistentM11InlineProjectionHostCursorPoll::Fact { fact } => {
                Ok(M11HostInlineProjectionCursorPoll::Fact(fact.into()))
            }
            PersistentM11InlineProjectionHostCursorPoll::Complete => {
                Ok(M11HostInlineProjectionCursorPoll::Complete)
            }
        }
    }

    #[must_use]
    pub const fn tree_nodes_visited(&self) -> u64 {
        self.inner.tree_nodes_visited()
    }
}

/// Opaque exact structural authority accepted by one sibling inline sidecar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11HostInlineSidecarBase(M11InlineOverlayBase);

/// Exact HIO1 block fence and monotonic refinement generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11HostInlineSidecarBinding(M11InlineOverlayBinding);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostInlineSidecarOwner {
    BlockOrdinal(u64),
    RecursiveGreenFrame(u64),
}

impl M11HostInlineSidecarBinding {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        base: M11HostInlineSidecarBase,
        refinement_generation: u64,
        block_ordinal: u64,
        physical_start_utf8: u32,
        physical_end_utf8: u32,
        visible_start_utf8: u32,
        visible_end_utf8: u32,
        physical_start_utf16: u32,
        physical_end_utf16: u32,
        visible_start_utf16: u32,
        visible_end_utf16: u32,
    ) -> Result<Self, M11HostError> {
        Self::new_for_owner(
            base,
            refinement_generation,
            M11HostInlineSidecarOwner::BlockOrdinal(block_ordinal),
            physical_start_utf8,
            physical_end_utf8,
            visible_start_utf8,
            visible_end_utf8,
            physical_start_utf16,
            physical_end_utf16,
            visible_start_utf16,
            visible_end_utf16,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_for_owner(
        base: M11HostInlineSidecarBase,
        refinement_generation: u64,
        owner: M11HostInlineSidecarOwner,
        physical_start_utf8: u32,
        physical_end_utf8: u32,
        visible_start_utf8: u32,
        visible_end_utf8: u32,
        physical_start_utf16: u32,
        physical_end_utf16: u32,
        visible_start_utf16: u32,
        visible_end_utf16: u32,
    ) -> Result<Self, M11HostError> {
        let owner = match owner {
            M11HostInlineSidecarOwner::BlockOrdinal(ordinal) => {
                M11InlineOverlayOwner::BlockOrdinal(ordinal)
            }
            M11HostInlineSidecarOwner::RecursiveGreenFrame(frame) => {
                M11InlineOverlayOwner::RecursiveGreenFrame(frame)
            }
        };
        M11InlineOverlayBinding::new(
            base.0,
            refinement_generation,
            owner,
            physical_start_utf8..physical_end_utf8,
            visible_start_utf8..visible_end_utf8,
            physical_start_utf16..physical_end_utf16,
            visible_start_utf16..visible_end_utf16,
        )
        .map(Self)
        .map_err(|_| M11HostError::invalid("invalid hot-inline block binding"))
    }
}

#[cfg(test)]
impl M11HostInlineSidecarBinding {
    pub(crate) fn from_engine_test(binding: M11InlineOverlayBinding) -> Self {
        Self(binding)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostInlineSidecarInstallPoll {
    pub transitions: usize,
    pub installed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostInlineSidecarDescriptor {
    source_start: u32,
    source_end: u32,
    logical_page_count: u64,
    fact_count: u64,
    storage_page_count: u64,
    link_value_entry_count: u32,
    link_value_storage_page_count: u64,
    link_value_encoded_bytes: u32,
    maximum_open_depth: u32,
    maximum_tree_nodes_visited: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostIndentedCodeSidecarDescriptor {
    physical_start: u32,
    physical_end: u32,
    window_start: u32,
    window_end: u32,
    projection_flags: u32,
    logical_page_count: u64,
    line_count: u64,
    storage_page_count: u64,
    ordered_commitment256: [u8; 32],
    maximum_open_depth: u32,
    maximum_tree_nodes_visited: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostBlockQuoteSidecarDescriptor {
    physical_start: u32,
    physical_end: u32,
    window_start: u32,
    window_end: u32,
    projected_utf8_length: u32,
    projected_utf16_length: u32,
    logical_page_count: u64,
    line_count: u64,
    storage_page_count: u64,
    ordered_commitment256: [u8; 32],
    maximum_open_depth: u32,
    maximum_tree_nodes_visited: u64,
}

impl M11HostBlockQuoteSidecarDescriptor {
    fn from_projection(
        descriptor: PersistentM11BlockQuoteProjectionDescriptor,
    ) -> Result<Self, M11HostError> {
        let physical = descriptor.physical_block_range();
        let window = descriptor.requested_window();
        Ok(Self {
            physical_start: physical.start,
            physical_end: physical.end,
            window_start: window.start,
            window_end: window.end,
            projected_utf8_length: descriptor.projected_utf8_length(),
            projected_utf16_length: descriptor.projected_utf16_length(),
            logical_page_count: descriptor.logical_page_count(),
            line_count: descriptor.line_count(),
            storage_page_count: descriptor.storage_page_count(),
            ordered_commitment256: descriptor.ordered_commitment256(),
            maximum_open_depth: descriptor.maximum_query_open_depth(),
            maximum_tree_nodes_visited: descriptor
                .maximum_query_tree_nodes_visited()
                .ok_or_else(|| M11HostError::invalid("block-quote query work bound overflowed"))?,
        })
    }

    #[must_use]
    pub const fn physical_start(self) -> u32 {
        self.physical_start
    }

    #[must_use]
    pub const fn physical_end(self) -> u32 {
        self.physical_end
    }

    #[must_use]
    pub const fn window_start(self) -> u32 {
        self.window_start
    }

    #[must_use]
    pub const fn window_end(self) -> u32 {
        self.window_end
    }

    #[must_use]
    pub const fn projected_utf8_length(self) -> u32 {
        self.projected_utf8_length
    }

    #[must_use]
    pub const fn projected_utf16_length(self) -> u32 {
        self.projected_utf16_length
    }

    #[must_use]
    pub const fn logical_page_count(self) -> u64 {
        self.logical_page_count
    }

    #[must_use]
    pub const fn line_count(self) -> u64 {
        self.line_count
    }

    #[must_use]
    pub const fn storage_page_count(self) -> u64 {
        self.storage_page_count
    }

    #[must_use]
    pub const fn ordered_commitment256(self) -> [u8; 32] {
        self.ordered_commitment256
    }

    #[must_use]
    pub const fn maximum_open_depth(self) -> u32 {
        self.maximum_open_depth
    }

    #[must_use]
    pub const fn maximum_tree_nodes_visited(self) -> u64 {
        self.maximum_tree_nodes_visited
    }
}

impl M11HostIndentedCodeSidecarDescriptor {
    fn from_projection(
        descriptor: PersistentM11IndentedCodeProjectionDescriptor,
    ) -> Result<Self, M11HostError> {
        let physical = descriptor.physical_block_range();
        let window = descriptor.requested_window();
        Ok(Self {
            physical_start: physical.start,
            physical_end: physical.end,
            window_start: window.start,
            window_end: window.end,
            projection_flags: descriptor.projection_flags(),
            logical_page_count: descriptor.logical_page_count(),
            line_count: descriptor.line_count(),
            storage_page_count: descriptor.storage_page_count(),
            ordered_commitment256: descriptor.ordered_commitment256(),
            maximum_open_depth: descriptor.maximum_query_open_depth(),
            maximum_tree_nodes_visited: descriptor.maximum_query_tree_nodes_visited().ok_or_else(
                || M11HostError::invalid("indented-code query work bound overflowed"),
            )?,
        })
    }

    #[must_use]
    pub const fn physical_start(self) -> u32 {
        self.physical_start
    }

    #[must_use]
    pub const fn physical_end(self) -> u32 {
        self.physical_end
    }

    #[must_use]
    pub const fn window_start(self) -> u32 {
        self.window_start
    }

    #[must_use]
    pub const fn window_end(self) -> u32 {
        self.window_end
    }

    #[must_use]
    pub const fn projection_flags(self) -> u32 {
        self.projection_flags
    }

    #[must_use]
    pub const fn logical_page_count(self) -> u64 {
        self.logical_page_count
    }

    #[must_use]
    pub const fn line_count(self) -> u64 {
        self.line_count
    }

    #[must_use]
    pub const fn storage_page_count(self) -> u64 {
        self.storage_page_count
    }

    #[must_use]
    pub const fn ordered_commitment256(self) -> [u8; 32] {
        self.ordered_commitment256
    }

    #[must_use]
    pub const fn maximum_open_depth(self) -> u32 {
        self.maximum_open_depth
    }

    #[must_use]
    pub const fn maximum_tree_nodes_visited(self) -> u64 {
        self.maximum_tree_nodes_visited
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostIndentedCodeLine {
    relative_line_start: u32,
    physical_source_length: u32,
    hidden_prefix_length: u32,
    content_length: u32,
    flags: u32,
}

impl From<IndentedCodeLineV1> for M11HostIndentedCodeLine {
    fn from(line: IndentedCodeLineV1) -> Self {
        Self {
            relative_line_start: line.relative_line_start(),
            physical_source_length: line.physical_source_length(),
            hidden_prefix_length: line.hidden_prefix_length(),
            content_length: line.content_length(),
            flags: line.flags(),
        }
    }
}

impl M11HostIndentedCodeLine {
    #[must_use]
    pub const fn relative_line_start(self) -> u32 {
        self.relative_line_start
    }

    #[must_use]
    pub const fn physical_source_length(self) -> u32 {
        self.physical_source_length
    }

    #[must_use]
    pub const fn hidden_prefix_length(self) -> u32 {
        self.hidden_prefix_length
    }

    #[must_use]
    pub const fn content_length(self) -> u32 {
        self.content_length
    }

    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }

    #[must_use]
    pub const fn physical_eol_length(self) -> u32 {
        self.physical_source_length
            .saturating_sub(self.hidden_prefix_length)
            .saturating_sub(self.content_length)
    }

    #[must_use]
    pub const fn is_internal_blank(self) -> bool {
        self.flags == 1
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostIndentedCodeCursorPoll {
    Line(M11HostIndentedCodeLine),
    Complete,
}

pub struct M11HostIndentedCodeCursor<'host> {
    inner: PersistentM11IndentedCodeProjectionHostCursor<'host>,
}

impl M11HostIndentedCodeCursor<'_> {
    pub fn poll(&mut self) -> Result<M11HostIndentedCodeCursorPoll, M11HostError> {
        match self.inner.poll().map_err(|_| {
            M11HostError::invalid("installed indented-code projection cursor became invalid")
        })? {
            PersistentM11IndentedCodeProjectionHostCursorPoll::Line { line } => {
                Ok(M11HostIndentedCodeCursorPoll::Line(line.into()))
            }
            PersistentM11IndentedCodeProjectionHostCursorPoll::Complete => {
                Ok(M11HostIndentedCodeCursorPoll::Complete)
            }
        }
    }

    #[must_use]
    pub const fn tree_nodes_visited(&self) -> u64 {
        self.inner.tree_nodes_visited()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostBlockQuoteLine {
    relative_line_start: u32,
    physical_source_length: u32,
    hidden_prefix_length: u32,
    continuation_prefix_start: u32,
    continuation_prefix_end: u32,
    content_length: u32,
    flags: u32,
}

impl From<BlockQuoteLineV1> for M11HostBlockQuoteLine {
    fn from(line: BlockQuoteLineV1) -> Self {
        Self {
            relative_line_start: line.relative_line_start(),
            physical_source_length: line.physical_source_length(),
            hidden_prefix_length: line.hidden_prefix_length(),
            continuation_prefix_start: line.continuation_prefix_start(),
            continuation_prefix_end: line.continuation_prefix_end(),
            content_length: line.content_length(),
            flags: line.flags(),
        }
    }
}

impl M11HostBlockQuoteLine {
    #[must_use]
    pub const fn relative_line_start(self) -> u32 {
        self.relative_line_start
    }

    #[must_use]
    pub const fn physical_source_length(self) -> u32 {
        self.physical_source_length
    }

    #[must_use]
    pub const fn hidden_prefix_length(self) -> u32 {
        self.hidden_prefix_length
    }

    #[must_use]
    pub const fn continuation_prefix_start(self) -> u32 {
        self.continuation_prefix_start
    }

    #[must_use]
    pub const fn continuation_prefix_end(self) -> u32 {
        self.continuation_prefix_end
    }

    #[must_use]
    pub const fn content_length(self) -> u32 {
        self.content_length
    }

    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }

    /// The final record word is the visible content's UTF-16 length when the
    /// enclosing authenticated query kind is `BulletList`.
    #[must_use]
    pub const fn bullet_content_utf16_length(self) -> u32 {
        self.flags
    }

    /// The final record word is the visible content's UTF-16 length when the
    /// enclosing authenticated query kind is `OrderedList`.
    #[must_use]
    pub const fn ordered_content_utf16_length(self) -> u32 {
        self.flags
    }

    #[must_use]
    pub const fn physical_eol_length(self) -> u32 {
        self.physical_source_length
            .saturating_sub(self.hidden_prefix_length)
            .saturating_sub(self.content_length)
    }

    #[must_use]
    pub const fn is_marked(self) -> bool {
        self.flags == 1
    }

    #[must_use]
    pub const fn is_lazy(self) -> bool {
        self.flags == 2
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostBlockQuoteCursorPoll {
    Line(M11HostBlockQuoteLine),
    Complete,
}

pub struct M11HostBlockQuoteCursor<'host> {
    inner: PersistentM11BlockQuoteProjectionHostCursor<'host>,
}

impl M11HostBlockQuoteCursor<'_> {
    pub fn poll(&mut self) -> Result<M11HostBlockQuoteCursorPoll, M11HostError> {
        match self.inner.poll().map_err(|_| {
            M11HostError::invalid("installed block-quote projection cursor became invalid")
        })? {
            PersistentM11BlockQuoteProjectionHostCursorPoll::Line { line } => {
                Ok(M11HostBlockQuoteCursorPoll::Line(line.into()))
            }
            PersistentM11BlockQuoteProjectionHostCursorPoll::Complete => {
                Ok(M11HostBlockQuoteCursorPoll::Complete)
            }
        }
    }

    #[must_use]
    pub const fn tree_nodes_visited(&self) -> u64 {
        self.inner.tree_nodes_visited()
    }
}

impl M11HostInlineSidecarDescriptor {
    fn from_projection(
        descriptor: PersistentM11InlineProjectionDescriptor,
    ) -> Result<Self, M11HostError> {
        let source = descriptor.source_range();
        Ok(Self {
            source_start: source.start,
            source_end: source.end,
            logical_page_count: descriptor.logical_page_count(),
            fact_count: descriptor.fact_count(),
            storage_page_count: descriptor.storage_page_count(),
            link_value_entry_count: descriptor.link_value_entry_count(),
            link_value_storage_page_count: descriptor.link_value_storage_page_count(),
            link_value_encoded_bytes: descriptor.link_value_encoded_bytes(),
            maximum_open_depth: descriptor.maximum_query_open_depth(),
            maximum_tree_nodes_visited: descriptor
                .maximum_query_tree_nodes_visited()
                .ok_or_else(|| M11HostError::invalid("hot-inline query work bound overflowed"))?,
        })
    }

    #[must_use]
    pub const fn source_start(self) -> u32 {
        self.source_start
    }

    #[must_use]
    pub const fn source_end(self) -> u32 {
        self.source_end
    }

    #[must_use]
    pub const fn logical_page_count(self) -> u64 {
        self.logical_page_count
    }

    #[must_use]
    pub const fn fact_count(self) -> u64 {
        self.fact_count
    }

    #[must_use]
    pub const fn storage_page_count(self) -> u64 {
        self.storage_page_count
    }

    #[must_use]
    pub const fn link_value_entry_count(self) -> u32 {
        self.link_value_entry_count
    }

    #[must_use]
    pub const fn link_value_storage_page_count(self) -> u64 {
        self.link_value_storage_page_count
    }

    #[must_use]
    pub const fn link_value_encoded_bytes(self) -> u32 {
        self.link_value_encoded_bytes
    }

    #[must_use]
    pub const fn maximum_open_depth(self) -> u32 {
        self.maximum_open_depth
    }

    #[must_use]
    pub const fn maximum_tree_nodes_visited(self) -> u64 {
        self.maximum_tree_nodes_visited
    }
}

// The authoritative query intentionally owns its cursor inline. This is a
// live-render query surface, so boxing would impose an allocation per query
// and would change the public cursor-bearing variant solely to optimize the
// uncommon unsupported result.
#[allow(clippy::large_enum_variant)]
pub enum M11HostInlineSidecarQuery<'host> {
    Authoritative {
        descriptor: M11HostInlineSidecarDescriptor,
        cursor: M11HostInlineProjectionCursor<'host>,
        link_values: M11HostInlineLinkValues<'host>,
    },
    IndentedCode {
        descriptor: M11HostIndentedCodeSidecarDescriptor,
        cursor: M11HostIndentedCodeCursor<'host>,
    },
    BlockQuote {
        descriptor: M11HostBlockQuoteSidecarDescriptor,
        cursor: M11HostBlockQuoteCursor<'host>,
    },
    BulletList {
        selected_item_ordinal: Option<u32>,
        selected_item_line_ending: Option<M11HostCanonicalLineEnding>,
        descriptor: M11HostBlockQuoteSidecarDescriptor,
        cursor: M11HostBlockQuoteCursor<'host>,
    },
    OrderedList {
        selected_item_ordinal: u32,
        selected_item_line_ending: M11HostCanonicalLineEnding,
        opening_marker_start: u32,
        opening_marker_end: u32,
        marker_value: u32,
        descriptor: M11HostBlockQuoteSidecarDescriptor,
        cursor: M11HostBlockQuoteCursor<'host>,
    },
    Unsupported {
        metadata: &'host [u8],
    },
}

pub struct M11HostInlineLinkValues<'host> {
    arena: &'host PageArena,
    root: Option<crate::ArenaId>,
    descriptor: PersistentM11InlineProjectionDescriptor,
}

impl M11HostInlineLinkValues<'_> {
    #[must_use]
    pub const fn entry_count(&self) -> u32 {
        self.descriptor.link_value_entry_count()
    }

    #[must_use]
    pub const fn encoded_bytes(&self) -> u32 {
        self.descriptor.link_value_encoded_bytes()
    }

    pub fn copy(
        &self,
        output: &mut [u8],
    ) -> Result<M11HostInlineLinkValueCopyReceipt, M11HostError> {
        encode_persistent_inline_link_values(self.arena, self.root, self.descriptor, output)
            .map(Into::into)
            .map_err(|_| M11HostError::invalid("installed inline link values became invalid"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11HostCanonicalLineEnding {
    Lf,
    CrLf,
    Cr,
}

/// Narrow independent importer for one revision-bound inline sidecar slot.
///
/// It owns a separate arena from [`M11CandidateHost`]. Structural replacement
/// is expressed only through [`Self::observe_base`], which retires the prior
/// sidecar without inserting it into the canonical candidate.
pub struct M11HostInlineSidecar(M11InlineOverlayHostStore);

impl M11HostInlineSidecar {
    pub fn new(base: M11HostInlineSidecarBase, limits: M11HostLimits) -> Self {
        Self(M11InlineOverlayHostStore::new(base.0, limits.engine()))
    }

    pub fn begin_snapshot(
        &mut self,
        binding: M11HostInlineSidecarBinding,
        frame: &[u8],
    ) -> Result<(), M11HostError> {
        self.0.begin_snapshot(binding.0, frame).map_err(Into::into)
    }

    pub fn offer_node(&mut self, frame: &[u8]) -> Result<(), M11HostError> {
        self.0.offer_node(frame).map_err(Into::into)
    }

    pub fn finish_snapshot(&mut self, frame: &[u8]) -> Result<(), M11HostError> {
        self.0.finish_snapshot(frame).map_err(Into::into)
    }

    pub fn poll_install(
        &mut self,
        fuel: usize,
    ) -> Result<M11HostInlineSidecarInstallPoll, M11HostError> {
        let M11InlineOverlayInstallPoll {
            transitions,
            installed,
        } = self.0.poll_install(fuel)?;
        Ok(M11HostInlineSidecarInstallPoll {
            transitions,
            installed,
        })
    }

    pub fn query(
        &self,
        binding: &M11HostInlineSidecarBinding,
    ) -> Result<Option<M11HostInlineSidecarQuery<'_>>, M11HostError> {
        let matched = self.0.query(&binding.0.query())?;
        Ok(match matched {
            Some(M11InlineOverlayHostMatch::InlineAuthoritative {
                descriptor,
                cursor,
                link_value_arena,
                link_value_root,
                ..
            }) => Some(M11HostInlineSidecarQuery::Authoritative {
                descriptor: M11HostInlineSidecarDescriptor::from_projection(descriptor)?,
                cursor: M11HostInlineProjectionCursor { inner: cursor },
                link_values: M11HostInlineLinkValues {
                    arena: link_value_arena,
                    root: link_value_root,
                    descriptor,
                },
            }),
            Some(M11InlineOverlayHostMatch::IndentedCodeAuthoritative {
                descriptor,
                cursor,
                ..
            }) => Some(M11HostInlineSidecarQuery::IndentedCode {
                descriptor: M11HostIndentedCodeSidecarDescriptor::from_projection(descriptor)?,
                cursor: M11HostIndentedCodeCursor { inner: cursor },
            }),
            Some(M11InlineOverlayHostMatch::BlockQuoteAuthoritative {
                descriptor, cursor, ..
            }) => Some(M11HostInlineSidecarQuery::BlockQuote {
                descriptor: M11HostBlockQuoteSidecarDescriptor::from_projection(descriptor)?,
                cursor: M11HostBlockQuoteCursor { inner: cursor },
            }),
            Some(M11InlineOverlayHostMatch::BulletListAuthoritative {
                envelope,
                descriptor,
                cursor,
            }) => {
                let (selected_item_ordinal, selected_item_line_ending) = match envelope
                    .disposition()
                {
                    crate::inline_overlay::M11InlineOverlayDisposition::Authoritative {
                        selected_item_ordinal,
                        selected_item_line_ending,
                        ..
                    } => (
                        *selected_item_ordinal,
                        selected_item_line_ending.map(|ending| match ending {
                            crate::inline_overlay::M11InlineOverlayCanonicalLineEnding::Lf => {
                                M11HostCanonicalLineEnding::Lf
                            }
                            crate::inline_overlay::M11InlineOverlayCanonicalLineEnding::CrLf => {
                                M11HostCanonicalLineEnding::CrLf
                            }
                            crate::inline_overlay::M11InlineOverlayCanonicalLineEnding::Cr => {
                                M11HostCanonicalLineEnding::Cr
                            }
                        }),
                    ),
                    crate::inline_overlay::M11InlineOverlayDisposition::Unsupported { .. } => {
                        return Err(M11HostError::invalid(
                            "authoritative bullet-list match lost its disposition",
                        ));
                    }
                };
                Some(M11HostInlineSidecarQuery::BulletList {
                    selected_item_ordinal,
                    selected_item_line_ending,
                    descriptor: M11HostBlockQuoteSidecarDescriptor::from_projection(descriptor)?,
                    cursor: M11HostBlockQuoteCursor { inner: cursor },
                })
            }
            Some(M11InlineOverlayHostMatch::OrderedListAuthoritative {
                envelope,
                descriptor,
                cursor,
            }) => {
                let ordered_item = match envelope.disposition() {
                    crate::inline_overlay::M11InlineOverlayDisposition::Authoritative {
                        ordered_item: Some(item),
                        ..
                    } => *item,
                    _ => {
                        return Err(M11HostError::invalid(
                            "authoritative ordered-list match lost its item metadata",
                        ));
                    }
                };
                let selected_item_line_ending = match ordered_item.selected_item_line_ending {
                    crate::inline_overlay::M11InlineOverlayCanonicalLineEnding::Lf => {
                        M11HostCanonicalLineEnding::Lf
                    }
                    crate::inline_overlay::M11InlineOverlayCanonicalLineEnding::CrLf => {
                        M11HostCanonicalLineEnding::CrLf
                    }
                    crate::inline_overlay::M11InlineOverlayCanonicalLineEnding::Cr => {
                        M11HostCanonicalLineEnding::Cr
                    }
                };
                Some(M11HostInlineSidecarQuery::OrderedList {
                    selected_item_ordinal: ordered_item.selected_item_ordinal,
                    selected_item_line_ending,
                    opening_marker_start: ordered_item.opening_marker_start,
                    opening_marker_end: ordered_item.opening_marker_end,
                    marker_value: ordered_item.marker_value,
                    descriptor: M11HostBlockQuoteSidecarDescriptor::from_projection(descriptor)?,
                    cursor: M11HostBlockQuoteCursor { inner: cursor },
                })
            }
            Some(M11InlineOverlayHostMatch::Unsupported { metadata, .. }) => {
                Some(M11HostInlineSidecarQuery::Unsupported { metadata })
            }
            None => None,
        })
    }

    pub fn observe_base(&mut self, base: M11HostInlineSidecarBase) -> Result<bool, M11HostError> {
        self.0.observe_base(base.0).map_err(Into::into)
    }

    pub fn abort_snapshot(&mut self) -> Result<bool, M11HostError> {
        self.0.abort_snapshot().map_err(Into::into)
    }

    pub fn poll_reclaim(&mut self, fuel: usize) -> Result<bool, M11HostError> {
        self.0
            .poll_retire(fuel)
            .map(|poll| poll.complete)
            .map_err(Into::into)
    }

    pub fn begin_close(&mut self) -> Result<(), M11HostError> {
        self.0.begin_close().map_err(Into::into)
    }

    pub fn poll_close(&mut self, fuel: usize) -> Result<bool, M11HostError> {
        self.0.poll_close(fuel).map_err(Into::into)
    }
}

#[cfg(test)]
impl M11HostInlineSidecar {
    pub(crate) fn from_engine_test(store: M11InlineOverlayHostStore) -> Self {
        Self(store)
    }
}

impl M11HostRole {
    const fn engine(self) -> CandidateRole {
        match self {
            Self::SourceFacts => CandidateRole::SourceFacts,
            Self::Green => CandidateRole::Green,
            Self::Projection => CandidateRole::Projection,
            Self::References => CandidateRole::References,
            Self::CleanEofOnly => CandidateRole::CleanEofOnly,
        }
    }
}

/// Opaque installed-root capability. It is useful only with the host that
/// produced it and contains no arena address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostInstalledCandidate(InstalledCandidateSnapshot);

impl M11HostInstalledCandidate {
    #[must_use]
    pub const fn source_revision(self) -> u64 {
        self.0.source_revision().get()
    }

    #[must_use]
    pub const fn parse_generation(self) -> u64 {
        self.0.parse_generation().get()
    }

    #[must_use]
    pub const fn publication_identity(self) -> [u8; 16] {
        self.0.publication_identity().0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostInstallPoll {
    pub transitions: usize,
    pub installed: Option<M11HostInstalledCandidate>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostReplayPoll {
    pub transitions: usize,
    pub ready_for_replacement_page: bool,
    pub ready_for_nodes: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11HostActiveAuthority {
    pub publication_identity: [u8; 16],
    pub parse_generation: u64,
}

#[derive(Debug)]
enum M11HostErrorInner {
    Invalid(&'static str),
    Host(CandidateHostError),
    InlineSidecar(M11InlineOverlayTransportError),
}

/// Classified failure from the independent host seam.
#[derive(Debug)]
pub struct M11HostError(M11HostErrorInner);

impl M11HostError {
    const fn invalid(message: &'static str) -> Self {
        Self(M11HostErrorInner::Invalid(message))
    }

    fn candidate_error(&self) -> Option<&CandidateHostError> {
        match &self.0 {
            M11HostErrorInner::Host(error)
            | M11HostErrorInner::InlineSidecar(M11InlineOverlayTransportError::Host(error)) => {
                Some(error)
            }
            _ => None,
        }
    }

    #[must_use]
    pub fn is_invalid(&self) -> bool {
        matches!(
            &self.0,
            M11HostErrorInner::Invalid(_)
                | M11HostErrorInner::InlineSidecar(
                    M11InlineOverlayTransportError::Overlay(_)
                        | M11InlineOverlayTransportError::Projection(_)
                        | M11InlineOverlayTransportError::IndentedCodeProjection(_)
                        | M11InlineOverlayTransportError::BlockQuoteProjection(_)
                        | M11InlineOverlayTransportError::InvalidProgram(_)
                )
        ) || self.candidate_error().is_some_and(|error| {
            matches!(
                error,
                CandidateHostError::InvalidLimits
                    | CandidateHostError::InvalidFrame(_)
                    | CandidateHostError::Manifest(_)
                    | CandidateHostError::Reference(_)
                    | CandidateHostError::BlockSequence(_)
                    | CandidateHostError::SourceFacts(_)
            )
        })
    }

    #[must_use]
    pub fn is_cross_authority(&self) -> bool {
        matches!(
            self.0,
            M11HostErrorInner::Host(CandidateHostError::CrossAuthority)
                | M11HostErrorInner::Host(CandidateHostError::Manifest(
                    ManifestError::CrossAuthority
                ))
        )
    }

    #[must_use]
    pub fn is_base_mismatch(&self) -> bool {
        self.candidate_error()
            .is_some_and(|error| matches!(error, CandidateHostError::BaseMismatch))
            || matches!(
                &self.0,
                M11HostErrorInner::InlineSidecar(M11InlineOverlayTransportError::SlotBaseMismatch)
            )
    }

    #[must_use]
    pub fn is_stale(&self) -> bool {
        self.candidate_error()
            .is_some_and(|error| matches!(error, CandidateHostError::StaleCandidate))
            || matches!(
                &self.0,
                M11HostErrorInner::InlineSidecar(M11InlineOverlayTransportError::StaleGeneration)
            )
    }

    #[must_use]
    pub fn is_backpressure(&self) -> bool {
        self.candidate_error()
            .is_some_and(|error| matches!(error, CandidateHostError::Busy))
            || matches!(
                &self.0,
                M11HostErrorInner::InlineSidecar(M11InlineOverlayTransportError::Install(_))
            )
    }

    #[must_use]
    pub fn is_not_ready(&self) -> bool {
        self.candidate_error()
            .is_some_and(|error| matches!(error, CandidateHostError::NoOffer))
            || matches!(
                &self.0,
                M11HostErrorInner::InlineSidecar(M11InlineOverlayTransportError::NoOffer)
            )
    }

    #[must_use]
    pub fn is_zero_fuel(&self) -> bool {
        self.candidate_error()
            .is_some_and(|error| matches!(error, CandidateHostError::ZeroFuel))
    }

    #[must_use]
    pub fn is_resource_limit(&self) -> bool {
        self.candidate_error().is_some_and(|error| {
            matches!(
                error,
                CandidateHostError::AllocationFailed
                    | CandidateHostError::Arena(
                        ArenaError::CapacityExceeded
                            | ArenaError::PayloadTooLarge
                            | ArenaError::TooManyChildren
                            | ArenaError::PayloadBudgetExceeded
                            | ArenaError::BuildCapacityExceeded
                            | ArenaError::AllocationFailed
                    )
            )
        })
    }
}

impl fmt::Display for M11HostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            M11HostErrorInner::Invalid(message) => formatter.write_str(message),
            M11HostErrorInner::Host(error) => error.fmt(formatter),
            M11HostErrorInner::InlineSidecar(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11HostError {}

impl From<CandidateHostError> for M11HostError {
    fn from(error: CandidateHostError) -> Self {
        Self(M11HostErrorInner::Host(error))
    }
}

impl From<M11InlineOverlayTransportError> for M11HostError {
    fn from(error: M11InlineOverlayTransportError) -> Self {
        Self(M11HostErrorInner::InlineSidecar(error))
    }
}

/// Persistent independent host. It owns a distinct arena and accepts either
/// self-contained snapshot bytes or an exact-base program bound to one opaque
/// installed capability minted by this same host.
pub struct M11CandidateHost(CandidateHostStore);

impl M11CandidateHost {
    pub fn new(
        document: [u8; 16],
        source: M11HostSourceVersion,
        syntax_profile: u32,
    ) -> Result<Self, M11HostError> {
        Self::new_with_limits(document, source, syntax_profile, M11HostLimits::default())
    }

    pub fn new_with_limits(
        document: [u8; 16],
        source: M11HostSourceVersion,
        syntax_profile: u32,
        limits: M11HostLimits,
    ) -> Result<Self, M11HostError> {
        Ok(Self(CandidateHostStore::new(
            StrongIdentity::new(document).map_err(|_| M11HostError::invalid("invalid document"))?,
            source.engine()?,
            syntax_profile,
            limits.engine(),
        )?))
    }

    pub fn classify_frame(frame: &[u8]) -> Result<M11HostFrameMetadata, M11HostError> {
        classify_snapshot_frame(frame)
            .map(Into::into)
            .map_err(Into::into)
    }

    pub fn observe_source_version(
        &mut self,
        source: M11HostSourceVersion,
    ) -> Result<bool, M11HostError> {
        self.0
            .observe_source_version(source.engine()?)
            .map_err(Into::into)
    }

    /// Rebinds only parser-replica root identity for an already authenticated
    /// exact source. Callers must prove content identity outside the engine.
    pub fn rebind_source_replica(
        &mut self,
        source: M11HostSourceVersion,
    ) -> Result<(), M11HostError> {
        self.0
            .rebind_source_replica(source.engine()?)
            .map_err(Into::into)
    }

    pub fn begin_snapshot(&mut self, frame: &[u8]) -> Result<(), M11HostError> {
        self.0.begin_snapshot(frame).map_err(Into::into)
    }

    /// Begins an exact-base program whose virtual ordinal zero is the
    /// canonical References root of `base`.
    ///
    /// The opaque base capability must still be this host's currently
    /// installed candidate. No producer arena identity crosses this seam.
    pub fn begin_references_delta(
        &mut self,
        base: M11HostInstalledCandidate,
        frame: &[u8],
    ) -> Result<(), M11HostError> {
        self.0
            .begin_references_delta(base.0, frame)
            .map_err(Into::into)
    }

    /// Begins a typed exact-base transaction which reuses References and may
    /// splice SourceFacts and BlockSequence replacement pages before ordinary
    /// target nodes.
    pub fn begin_exact_base_delta(
        &mut self,
        base: M11HostInstalledCandidate,
        frame: &[u8],
    ) -> Result<(), M11HostError> {
        self.0
            .begin_exact_base_delta(base.0, frame)
            .map_err(Into::into)
    }

    pub fn offer_source_facts_replacement_page(
        &mut self,
        frame: &[u8],
    ) -> Result<(), M11HostError> {
        self.0
            .offer_source_facts_replacement_page(frame)
            .map_err(Into::into)
    }

    pub fn offer_block_sequence_replacement_page(
        &mut self,
        frame: &[u8],
    ) -> Result<(), M11HostError> {
        self.0
            .offer_block_sequence_replacement_page(frame)
            .map_err(Into::into)
    }

    pub fn offer_recursive_green_replacement_page(
        &mut self,
        frame: &[u8],
    ) -> Result<(), M11HostError> {
        self.0
            .offer_recursive_green_replacement_page(frame)
            .map_err(Into::into)
    }

    pub fn poll_exact_base_delta_replay(
        &mut self,
        fuel: usize,
    ) -> Result<M11HostReplayPoll, M11HostError> {
        let CandidateHostReplayPoll {
            transitions,
            ready_for_replacement_page,
            ready_for_nodes,
        } = self.0.poll_exact_base_delta_replay(fuel)?;
        Ok(M11HostReplayPoll {
            transitions,
            ready_for_replacement_page,
            ready_for_nodes,
        })
    }

    #[must_use]
    pub fn active_authority(&self) -> Option<M11HostActiveAuthority> {
        self.0
            .active_authority()
            .map(|authority| M11HostActiveAuthority {
                publication_identity: authority.publication.0,
                parse_generation: authority.parse_generation.get(),
            })
    }

    pub fn offer_node(&mut self, frame: &[u8]) -> Result<(), M11HostError> {
        self.0.offer_node(frame).map_err(Into::into)
    }

    pub fn finish_snapshot(&mut self, frame: &[u8]) -> Result<(), M11HostError> {
        self.0.finish_snapshot(frame).map_err(Into::into)
    }

    #[must_use]
    pub fn active_snapshot_digest256(&self) -> Option<[u8; 32]> {
        self.0.active_snapshot_digest256()
    }

    pub fn poll_install(&mut self, fuel: usize) -> Result<M11HostInstallPoll, M11HostError> {
        let CandidateHostInstallPoll {
            transitions,
            installed,
        } = self.0.poll_install(fuel)?;
        Ok(M11HostInstallPoll {
            transitions,
            installed: installed.map(M11HostInstalledCandidate),
        })
    }

    #[must_use]
    pub fn installed(&self) -> Option<M11HostInstalledCandidate> {
        self.0.installed_snapshot().map(M11HostInstalledCandidate)
    }

    pub fn inline_sidecar_base(
        &self,
        installed: M11HostInstalledCandidate,
        parser_profile: u64,
    ) -> Result<M11HostInlineSidecarBase, M11HostError> {
        let parser_profile = ParserProfileId::new(parser_profile)
            .ok_or_else(|| M11HostError::invalid("hot-inline parser profile must be nonzero"))?;
        self.0
            .inline_overlay_base(installed.0, parser_profile)
            .map(M11HostInlineSidecarBase)
            .map_err(Into::into)
    }

    pub fn installed_manifest_digest256(
        &self,
        installed: M11HostInstalledCandidate,
    ) -> Result<[u8; 32], M11HostError> {
        self.0
            .installed_manifest_digest256(installed.0)
            .map_err(Into::into)
    }

    pub fn persistent_inline_projection_descriptor(
        &self,
        installed: M11HostInstalledCandidate,
    ) -> Result<Option<M11HostInlineProjectionDescriptor>, M11HostError> {
        self.0
            .persistent_inline_projection_descriptor(installed.0)
            .map(|descriptor| descriptor.map(Into::into))
            .map_err(Into::into)
    }

    pub fn persistent_inline_projection_cursor(
        &self,
        installed: M11HostInstalledCandidate,
    ) -> Result<Option<M11HostInlineProjectionCursor<'_>>, M11HostError> {
        self.0
            .persistent_inline_projection_cursor(installed.0)
            .map(|cursor| cursor.map(|inner| M11HostInlineProjectionCursor { inner }))
            .map_err(Into::into)
    }

    /// Copies the complete authenticated `FLKIV001` companion payload.
    ///
    /// `output` must have exactly the descriptor's `link_value_encoded_bytes`.
    /// A leaf with no direct links/images uses the canonical absent lane and
    /// therefore requires an empty output.
    pub fn copy_persistent_inline_link_values(
        &self,
        installed: M11HostInstalledCandidate,
        output: &mut [u8],
    ) -> Result<Option<M11HostInlineLinkValueCopyReceipt>, M11HostError> {
        self.0
            .copy_persistent_inline_link_values(installed.0, output)
            .map(|receipt| receipt.map(Into::into))
            .map_err(Into::into)
    }

    pub fn persistent_block_descriptor(
        &self,
        installed: M11HostInstalledCandidate,
    ) -> Result<Option<M11HostPersistentBlockDescriptor>, M11HostError> {
        self.0
            .persistent_block_descriptor(installed.0)
            .map(|descriptor| descriptor.map(Into::into))
            .map_err(Into::into)
    }

    pub fn persistent_recursive_green_descriptor(
        &self,
        installed: M11HostInstalledCandidate,
    ) -> Result<Option<M11HostPersistentRecursiveGreenDescriptor>, M11HostError> {
        self.0
            .persistent_recursive_green_descriptor(installed.0)
            .map(|descriptor| descriptor.map(Into::into))
            .map_err(Into::into)
    }

    pub fn persistent_recursive_green_point(
        &self,
        installed: M11HostInstalledCandidate,
        byte_offset: u64,
        utf16_offset: u64,
        affinity: M11HostBlockAffinity,
    ) -> Result<Option<M11HostRecursiveGreenLocation>, M11HostError> {
        let byte_offset = usize::try_from(byte_offset)
            .map_err(|_| M11HostError::invalid("Green byte point exceeds this target"))?;
        let utf16_offset = usize::try_from(utf16_offset)
            .map_err(|_| M11HostError::invalid("Green UTF-16 point exceeds this target"))?;
        self.0
            .persistent_recursive_green_point(
                installed.0,
                M11RecursiveGreenPoint::new(byte_offset, utf16_offset, affinity.engine()),
            )
            .map(|location| location.map(M11HostRecursiveGreenLocation))
            .map_err(Into::into)
    }

    pub fn persistent_recursive_green_rows(
        &self,
        installed: M11HostInstalledCandidate,
        byte_offset: u64,
        utf16_offset: u64,
        requested_end_byte: u64,
        maximum_rows: u32,
        maximum_storage_pages_visited: u64,
        maximum_events_scanned: u64,
        maximum_open_depth: usize,
        maximum_tree_nodes_visited: u64,
    ) -> Result<Option<M11HostRecursiveGreenRowQueryOutcome>, M11HostError> {
        let byte_offset = usize::try_from(byte_offset)
            .map_err(|_| M11HostError::invalid("Green row byte point exceeds this target"))?;
        let utf16_offset = usize::try_from(utf16_offset)
            .map_err(|_| M11HostError::invalid("Green row UTF-16 point exceeds this target"))?;
        let limits = M11RecursiveGreenRowQueryLimits::new(
            maximum_rows,
            maximum_storage_pages_visited,
            maximum_events_scanned,
            maximum_open_depth,
            maximum_tree_nodes_visited,
        )
        .ok_or_else(|| M11HostError::invalid("Green row query limits must be nonzero"))?;
        self.0
            .persistent_recursive_green_rows(
                installed.0,
                M11RecursiveGreenPoint::new(
                    byte_offset,
                    utf16_offset,
                    SourceBoundaryAffinity::After,
                ),
                requested_end_byte,
                limits,
            )
            .map(|outcome| {
                outcome.map(|outcome| match outcome {
                    M11RecursiveGreenRowQueryOutcome::Window(window) => {
                        M11HostRecursiveGreenRowQueryOutcome::Window(
                            M11HostRecursiveGreenRowWindow(window),
                        )
                    }
                    M11RecursiveGreenRowQueryOutcome::BudgetExceeded(exceeded) => {
                        M11HostRecursiveGreenRowQueryOutcome::BudgetExceeded(
                            M11HostRecursiveGreenRowBudgetExceeded(exceeded),
                        )
                    }
                })
            })
            .map_err(Into::into)
    }

    pub fn persistent_recursive_green_row_ordinal_window(
        &self,
        installed: M11HostInstalledCandidate,
        start_ordinal: u64,
        maximum_rows: u32,
    ) -> Result<Option<M11HostRecursiveGreenRowOrdinalWindow>, M11HostError> {
        self.0
            .persistent_recursive_green_row_ordinal_window(installed.0, start_ordinal, maximum_rows)
            .map(|window| window.map(M11HostRecursiveGreenRowOrdinalWindow))
            .map_err(Into::into)
    }

    pub fn persistent_block_point(
        &self,
        installed: M11HostInstalledCandidate,
        byte_offset: u64,
        utf16_offset: u64,
        affinity: M11HostBlockAffinity,
    ) -> Result<Option<M11HostPersistentBlockLocation>, M11HostError> {
        let byte_offset = usize::try_from(byte_offset)
            .map_err(|_| M11HostError::invalid("block byte point exceeds this target"))?;
        let utf16_offset = usize::try_from(utf16_offset)
            .map_err(|_| M11HostError::invalid("block UTF-16 point exceeds this target"))?;
        self.0
            .persistent_block_point(
                installed.0,
                M11BlockSequencePoint::new(byte_offset, utf16_offset, affinity.engine()),
            )
            .map(|location| location.map(M11HostPersistentBlockLocation))
            .map_err(Into::into)
    }

    pub fn persistent_block_ordinal_window(
        &self,
        installed: M11HostInstalledCandidate,
        start_entry_ordinal: u64,
        maximum_entries: u32,
    ) -> Result<Option<M11HostPersistentBlockOrdinalWindow>, M11HostError> {
        self.0
            .persistent_block_ordinal_window(installed.0, start_entry_ordinal, maximum_entries)
            .map(|window| window.map(|inner| M11HostPersistentBlockOrdinalWindow { inner }))
            .map_err(Into::into)
    }

    pub fn visit_persistent_blocks(
        &self,
        installed: M11HostInstalledCandidate,
        start: M11HostPersistentBlockVisitStart,
        maximum_entries: u32,
        maximum_storage_pages: u32,
        mut visitor: impl FnMut(
            M11HostPersistentBlockVisitEntry<'_>,
        ) -> M11HostPersistentBlockVisitControl,
    ) -> Result<Option<M11HostPersistentBlockVisitReceipt>, M11HostError> {
        let start = M11BlockSequenceVisitStart {
            entry_ordinal: start.entry_ordinal,
            byte_offset: start.byte_offset,
            utf16_offset: start.utf16_offset,
        };
        self.0
            .visit_persistent_blocks(
                installed.0,
                start,
                maximum_entries,
                maximum_storage_pages,
                |entry| match visitor(M11HostPersistentBlockVisitEntry { inner: entry }) {
                    M11HostPersistentBlockVisitControl::Continue => {
                        M11BlockSequenceVisitControl::Continue
                    }
                    M11HostPersistentBlockVisitControl::Stop => M11BlockSequenceVisitControl::Stop,
                },
            )
            .map(|receipt| {
                receipt.map(|receipt| {
                    let inspection = receipt.inspection();
                    M11HostPersistentBlockVisitReceipt {
                        visited_entries: receipt.visited_entries(),
                        storage_pages_visited: receipt.storage_pages_visited(),
                        next_entry_ordinal: receipt.next_entry_ordinal(),
                        next_byte_offset: receipt.next_byte_offset(),
                        next_utf16_offset: receipt.next_utf16_offset(),
                        disposition: match receipt.disposition() {
                            M11BlockSequenceVisitDisposition::Complete => {
                                M11HostPersistentBlockVisitDisposition::Complete
                            }
                            M11BlockSequenceVisitDisposition::EntryLimit => {
                                M11HostPersistentBlockVisitDisposition::EntryLimit
                            }
                            M11BlockSequenceVisitDisposition::StoragePageLimit => {
                                M11HostPersistentBlockVisitDisposition::StoragePageLimit
                            }
                            M11BlockSequenceVisitDisposition::VisitorStopped => {
                                M11HostPersistentBlockVisitDisposition::VisitorStopped
                            }
                        },
                        node_headers_decoded: inspection.node_headers_decoded,
                        summary_combinations: inspection.summary_combinations,
                        payload_bytes_inspected: inspection.spec.payload_bytes_inspected,
                        entries_authenticated: inspection.spec.spec_items_hashed,
                    }
                })
            })
            .map_err(Into::into)
    }

    pub fn role_record_count(
        &self,
        installed: M11HostInstalledCandidate,
        role: M11HostRole,
    ) -> Result<u64, M11HostError> {
        self.0
            .role_record_count(installed.0, role.engine())
            .map_err(Into::into)
    }

    pub fn read_role_record(
        &self,
        installed: M11HostInstalledCandidate,
        role: M11HostRole,
        ordinal: u64,
        offset: usize,
        output: &mut [u8],
    ) -> Result<usize, M11HostError> {
        self.0
            .read_role_record_at(installed.0, role.engine(), ordinal, offset, output)
            .map_err(Into::into)
    }

    pub fn abort_snapshot(&mut self) -> Result<bool, M11HostError> {
        self.0.abort_snapshot().map_err(Into::into)
    }

    pub fn poll_reclaim(&mut self, fuel: usize) -> Result<bool, M11HostError> {
        self.0.poll_reclaim(fuel).map_err(Into::into)
    }

    pub fn begin_close(&mut self) -> Result<(), M11HostError> {
        self.0.begin_close().map_err(Into::into)
    }

    pub fn poll_close(&mut self, fuel: usize) -> Result<bool, M11HostError> {
        self.0.poll_close(fuel).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        host_block_kind, M11CandidateHost, M11HostBlockKind, M11HostInlineProjectionFact,
        M11HostInlineProjectionKind, M11HostSourceVersion,
    };
    use crate::block_sequence::M11BlockSequenceEntryKind;
    use crate::inline_projection::{M11InlineProjectionFact, M11InlineProjectionKind};

    #[test]
    fn structured_block_kind_maps_to_the_host_surface() {
        assert_eq!(
            host_block_kind(M11BlockSequenceEntryKind::Structured),
            M11HostBlockKind::Structured
        );
    }

    #[test]
    fn hard_line_break_maps_to_the_host_fact_without_losing_geometry() {
        let fact =
            M11InlineProjectionFact::new(M11InlineProjectionKind::HardLineBreak, 0, 7..11, 9..11)
                .expect("hard line break fact");
        let host = M11HostInlineProjectionFact::from(fact);
        assert_eq!(host.kind(), M11HostInlineProjectionKind::HardLineBreak);
        assert_eq!(host.flags(), 0);
        assert_eq!(host.relative_start(), 7);
        assert_eq!(host.relative_len(), 4);
        assert_eq!(host.content_offset(), 2);
        assert_eq!(host.content_len(), 2);
    }

    #[test]
    fn character_reference_maps_to_the_host_fact_without_retyping_scalars_as_geometry() {
        let fact =
            M11InlineProjectionFact::new_character_reference(7..22, '\u{2242}', Some('\u{0338}'))
                .expect("character-reference fact");
        let host = M11HostInlineProjectionFact::from(fact);
        assert_eq!(host.kind(), M11HostInlineProjectionKind::CharacterReference);
        assert_eq!(host.flags(), 0);
        assert_eq!(host.relative_start(), 7);
        assert_eq!(host.relative_len(), 15);
        assert_eq!(host.content_offset(), 0);
        assert_eq!(host.content_len(), 15);
        assert_eq!(
            host.character_reference(),
            Some(('\u{2242}', Some('\u{0338}')))
        );
    }

    #[test]
    fn block_replacement_page_requires_an_active_exact_base_replay() {
        let mut host = M11CandidateHost::new(
            [1; 16],
            M11HostSourceVersion {
                source_root: 1,
                source_revision: 0,
                source_bytes: 0,
                source_utf16: 0,
            },
            1,
        )
        .expect("empty host");

        assert!(host.offer_block_sequence_replacement_page(&[0xe6]).is_err());

        host.begin_close().expect("close host");
        while !host.poll_close(1).expect("poll close") {}
    }
}

//! Final generic hierarchical-green representation discriminator.
//!
//! Ordinary local subtrees are packed into kind-agnostic sibling pages. Large
//! sibling ranges use the crate's one persistent-sequence implementation, and
//! pages with external semantic children use the shared arena's bounded packed
//! edge table. One arena transaction spans the changed leaf through the root
//! and manifest. This is a bakeoff artifact, not a stabilized parser API.

use std::fmt;
use std::ops::Range;

use crate::arena::{ArenaBuildTransaction, ArenaOwnerHandle};
use crate::persistent_sequence::{
    SequenceMutationReceipt, SequenceNodeKind, SequenceSpec, StreamingSequenceBuilder,
    sequence_node, splice_root_in_transaction,
};
use crate::record_forest::{ChildSequenceAggregate, ClosedChildAggregate};
use crate::{
    ARENA_PAGE_BYTES, ArenaError, ArenaId, GenericAffinity, GenericBlockKind, GenericCoordinate,
    GenericGreenMetric, GenericSourceKind, OwnedArenaRef, OwnerTransferError, PageArena,
};

const VERSION: u8 = 1;
const SIBLING_LEAF_TAG: u8 = 0xb1;
const SIBLING_BRANCH_TAG: u8 = 0xb2;
const GREEN_ROOT_TAG: u8 = 0xb3;
const GREEN_MANIFEST_TAG: u8 = 0xb4;
const LEAF_HEADER_BYTES: usize = 32;
const ENTRY_BYTES: usize = 40;
const BRANCH_BYTES: usize = 40;
const ROOT_BYTES: usize = 56;
const MANIFEST_BYTES: usize = 56;
const NO_EDGE: u16 = u16::MAX;
const MODELED_ALLOCATOR_BYTES_PER_PAGE: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompactBlockFacts(pub u64);

impl CompactBlockFacts {
    #[must_use]
    pub const fn paragraph() -> Self {
        Self(0)
    }

    #[must_use]
    pub fn heading(level: u8, setext: bool) -> Self {
        Self(u64::from(level) | (u64::from(setext) << 3))
    }

    #[must_use]
    pub fn list(start: u32, marker: u8, delimiter: u8, tight: bool) -> Self {
        Self(
            u64::from(start)
                | (u64::from(marker) << 32)
                | (u64::from(delimiter) << 40)
                | (u64::from(tight) << 48),
        )
    }

    #[must_use]
    pub fn fence(character: u8, length: u8, offset: u8) -> Self {
        Self(u64::from(character) | (u64::from(length) << 8) | (u64::from(offset) << 16))
    }

    #[must_use]
    pub fn html(block_type: u8) -> Self {
        Self(u64::from(block_type))
    }

    #[must_use]
    pub fn table(columns: u16, packed_alignments: u32) -> Self {
        Self(u64::from(columns) | (u64::from(packed_alignments) << 16))
    }

    #[must_use]
    pub fn item(offset: u16, padding: u16) -> Self {
        Self(u64::from(offset) | (u64::from(padding) << 16))
    }

    #[must_use]
    pub const fn with_list_tight(self, tight: bool) -> Self {
        Self((self.0 & !(1_u64 << 48)) | ((tight as u64) << 48))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HierarchicalSiblingEntry {
    pub block: u64,
    pub coverage: u64,
    pub kind: GenericBlockKind,
    pub source_kind: GenericSourceKind,
    pub local_metric: GenericGreenMetric,
    pub facts: CompactBlockFacts,
    pub contribution: ClosedChildAggregate,
    /// Optional generic semantic child root. Its source follows the local run.
    pub child: Option<ArenaId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HierarchicalRootSpec {
    pub block: u64,
    pub kind: GenericBlockKind,
    pub facts: CompactBlockFacts,
    pub epoch: u64,
    pub source_revision: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SiblingSummary {
    leaves: u64,
    items: u64,
    height: u16,
    metric: GenericGreenMetric,
    fold: ChildSequenceAggregate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HierarchicalGreenError {
    Arena(ArenaError),
    Invalid(&'static str),
    Corrupt(&'static str),
    Overflow(&'static str),
    NotFound,
    StaleCursor,
}

impl From<ArenaError> for HierarchicalGreenError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

fn legacy_owner_transfer_error(failure: OwnerTransferError) -> HierarchicalGreenError {
    // This bakeoff representation is not selected and retains its old
    // copyable error contract. Production authority paths preserve the owner.
    let OwnerTransferError { error, owner } = failure;
    drop(owner);
    HierarchicalGreenError::Arena(error)
}

impl fmt::Display for HierarchicalGreenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arena(error) => error.fmt(formatter),
            Self::Invalid(message) => write!(formatter, "invalid hierarchical green: {message}"),
            Self::Corrupt(message) => write!(formatter, "corrupt hierarchical green: {message}"),
            Self::Overflow(field) => write!(formatter, "hierarchical green {field} overflow"),
            Self::NotFound => formatter.write_str("hierarchical green value not found"),
            Self::StaleCursor => formatter.write_str("stale hierarchical green cursor"),
        }
    }
}

impl std::error::Error for HierarchicalGreenError {}

#[derive(Debug)]
struct SiblingSequenceSpec;

impl SequenceSpec for SiblingSequenceSpec {
    type Summary = SiblingSummary;
    type Error = HierarchicalGreenError;
    type BranchPayload = [u8; BRANCH_BYTES];

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(SIBLING_LEAF_TAG) {
            return Ok(None);
        }
        decode_leaf_header(payload).map(Some)
    }

    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(SIBLING_BRANCH_TAG) {
            return Ok(None);
        }
        decode_branch(payload).map(Some)
    }

    fn encode_branch(summary: Self::Summary) -> Self::BranchPayload {
        encode_branch(summary)
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        Ok(SiblingSummary {
            leaves: left
                .leaves
                .checked_add(right.leaves)
                .ok_or(HierarchicalGreenError::Overflow("leaf count"))?,
            items: left
                .items
                .checked_add(right.items)
                .ok_or(HierarchicalGreenError::Overflow("item count"))?,
            height: left
                .height
                .max(right.height)
                .checked_add(1)
                .ok_or(HierarchicalGreenError::Overflow("sequence height"))?,
            metric: checked_metric_add(left.metric, right.metric)?,
            fold: left.fold.followed_by(right.fold),
        })
    }

    fn leaves(summary: Self::Summary) -> u64 {
        summary.leaves
    }

    fn height(summary: Self::Summary) -> u16 {
        summary.height
    }

    fn invalid(message: &'static str) -> Self::Error {
        HierarchicalGreenError::Corrupt(message)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HierarchicalBuildReceipt {
    pub leaf_pages_allocated: usize,
    pub branch_nodes_allocated: usize,
    pub root_nodes_allocated: usize,
    pub manifest_nodes_allocated: usize,
    pub payload_bytes_copied: usize,
    pub edge_bytes_copied: usize,
    pub sequence_nodes_visited: usize,
    pub sequence_leaves_reused: usize,
    pub maximum_typed_page_buffer_bytes: usize,
    pub maximum_encoded_page_buffer_bytes: usize,
    pub maximum_edge_buffer_bytes: usize,
    pub maximum_streaming_roots: usize,
    pub maximum_streaming_bin_bytes: usize,
    pub maximum_live_owner_handles: usize,
    pub owner_journal_capacity: usize,
    pub owner_journal_bytes: usize,
    pub entries_decoded: usize,
}

#[derive(Debug)]
pub struct HierarchicalGreenDocument {
    owner: OwnedArenaRef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedRoot {
    block: u64,
    kind: GenericBlockKind,
    facts: CompactBlockFacts,
    epoch: u64,
    summary: SiblingSummary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedManifest {
    epoch: u64,
    source_revision: u64,
    block: u64,
    summary: SiblingSummary,
}

impl HierarchicalGreenDocument {
    pub fn build(
        arena: &mut PageArena,
        spec: HierarchicalRootSpec,
        entries: impl IntoIterator<Item = HierarchicalSiblingEntry>,
        receipt: &mut HierarchicalBuildReceipt,
    ) -> Result<Self, HierarchicalGreenError> {
        if spec.block == 0 || spec.epoch == 0 || spec.source_revision == 0 {
            return Err(HierarchicalGreenError::Invalid(
                "root identity and generations must be nonzero",
            ));
        }
        let mut transaction = ArenaBuildTransaction::new(arena);
        let mut sequence = StreamingSequenceBuilder::<SiblingSequenceSpec>::default();
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let mut page = Vec::new();
        for entry in entries {
            validate_entry(transaction.arena(), entry)?;
            if !page.is_empty() && !entry_fits(&page, entry) {
                flush_page(
                    &mut transaction,
                    &mut sequence,
                    &mut sequence_receipt,
                    &mut page,
                    receipt,
                )?;
            }
            page.push(entry);
            receipt.maximum_typed_page_buffer_bytes = receipt
                .maximum_typed_page_buffer_bytes
                .max(page.capacity() * std::mem::size_of::<HierarchicalSiblingEntry>());
            if !entry_fits_existing(&page) {
                return Err(HierarchicalGreenError::Invalid(
                    "one sibling entry exceeds a packed page",
                ));
            }
        }
        if !page.is_empty() {
            flush_page(
                &mut transaction,
                &mut sequence,
                &mut sequence_receipt,
                &mut page,
                receipt,
            )?;
        }
        let sequence_root = sequence
            .finish(&mut transaction, &mut sequence_receipt)?
            .ok_or(HierarchicalGreenError::Invalid("empty sibling sequence"))?;
        let summary = sequence_node::<SiblingSequenceSpec>(
            transaction.arena(),
            transaction.id(&sequence_root),
        )?
        .0;
        let root = allocate_root(
            &mut transaction,
            DecodedRoot {
                block: spec.block,
                kind: spec.kind,
                facts: if spec.kind == GenericBlockKind::List {
                    spec.facts.with_list_tight(summary.fold.list_is_tight())
                } else {
                    spec.facts
                },
                epoch: spec.epoch,
                summary,
            },
            sequence_root,
            receipt,
        )?;
        let manifest = allocate_manifest(
            &mut transaction,
            DecodedManifest {
                epoch: spec.epoch,
                source_revision: spec.source_revision,
                block: spec.block,
                summary,
            },
            root,
            receipt,
        )?;
        merge_sequence_receipt(receipt, sequence_receipt);
        sync_transaction_receipt(receipt, &transaction);
        Ok(Self {
            owner: transaction.take(manifest),
        })
    }

    #[must_use]
    pub const fn root_id(&self) -> ArenaId {
        self.owner.id()
    }

    pub fn metric(&self, arena: &PageArena) -> Result<GenericGreenMetric, HierarchicalGreenError> {
        Ok(decode_document(arena, self.owner.id())?.0.summary.metric)
    }

    pub fn block_count(&self, arena: &PageArena) -> Result<u64, HierarchicalGreenError> {
        Ok(decode_document(arena, self.owner.id())?.0.summary.items + 1)
    }

    pub fn list_is_tight(&self, arena: &PageArena) -> Result<bool, HierarchicalGreenError> {
        let (_, root, _) = decode_document(arena, self.owner.id())?;
        Ok(root.summary.fold.list_is_tight())
    }

    pub fn leaf_count(&self, arena: &PageArena) -> Result<u64, HierarchicalGreenError> {
        Ok(decode_document(arena, self.owner.id())?.1.summary.leaves)
    }

    pub fn leaf_at(
        &self,
        arena: &PageArena,
        leaf_index: u64,
    ) -> Result<Option<ArenaId>, HierarchicalGreenError> {
        let (_, _, sequence) = decode_document(arena, self.owner.id())?;
        locate_leaf(arena, sequence, leaf_index).map(|value| value.map(|located| located.0))
    }

    pub fn source_lookup(
        &self,
        arena: &PageArena,
        offset: u64,
        coordinate: GenericCoordinate,
        affinity: GenericAffinity,
    ) -> Result<Option<HierarchicalSourceHit>, HierarchicalGreenError> {
        source_lookup_document(arena, self.owner.id(), offset, coordinate, affinity)
    }

    pub fn viewport(
        &self,
        arena: &PageArena,
        range: Range<u64>,
        coordinate: GenericCoordinate,
        receipt: &mut HierarchicalViewportReceipt,
    ) -> Result<Vec<HierarchicalViewportEntry>, HierarchicalGreenError> {
        viewport_document(arena, self.owner.id(), range, coordinate, receipt)
    }

    pub fn replace_at_cursor(
        &self,
        arena: &mut PageArena,
        cursor: HierarchicalSourceCursor,
        replacement: HierarchicalSiblingEntry,
        receipt: &mut HierarchicalBuildReceipt,
    ) -> Result<Self, HierarchicalGreenError> {
        if cursor.manifest != self.owner.id() {
            return Err(HierarchicalGreenError::StaleCursor);
        }
        validate_entry(arena, replacement)?;
        let (manifest, root, sequence_root) = decode_document(arena, self.owner.id())?;
        let (leaf, _) = locate_leaf(arena, sequence_root, cursor.leaf_index)?
            .ok_or(HierarchicalGreenError::StaleCursor)?;
        if leaf != cursor.leaf {
            return Err(HierarchicalGreenError::StaleCursor);
        }
        let mut entries = decode_leaf(arena, leaf)?;
        receipt.entries_decoded += entries.len();
        let local = usize::from(cursor.local_index);
        let current = entries
            .get(local)
            .copied()
            .ok_or(HierarchicalGreenError::StaleCursor)?;
        if current.block != cursor.block
            || current.coverage != cursor.coverage
            || replacement.block != current.block
            || replacement.coverage != current.coverage
        {
            return Err(HierarchicalGreenError::StaleCursor);
        }
        entries[local] = replacement;
        let (payload, edges) = encode_leaf(arena, &entries)?;
        let mut transaction = ArenaBuildTransaction::new(arena);
        let (next_leaf, allocation) = transaction.allocate_packed(&payload, &edges)?;
        receipt.leaf_pages_allocated += 1;
        receipt.payload_bytes_copied += allocation.payload_bytes_copied;
        receipt.edge_bytes_copied += allocation.edge_bytes_copied;
        receipt.maximum_encoded_page_buffer_bytes = receipt
            .maximum_encoded_page_buffer_bytes
            .max(payload.capacity());
        receipt.maximum_edge_buffer_bytes = receipt
            .maximum_edge_buffer_bytes
            .max(edges.capacity() * std::mem::size_of::<ArenaId>());
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let next_sequence = splice_root_in_transaction::<SiblingSequenceSpec>(
            &mut transaction,
            Some(sequence_root),
            cursor.leaf_index..cursor.leaf_index + 1,
            vec![next_leaf],
            &mut sequence_receipt,
        )?
        .ok_or(HierarchicalGreenError::Corrupt(
            "edit removed sibling sequence",
        ))?;
        let summary = sequence_node::<SiblingSequenceSpec>(
            transaction.arena(),
            transaction.id(&next_sequence),
        )?
        .0;
        let next_root = allocate_root(
            &mut transaction,
            DecodedRoot {
                facts: if root.kind == GenericBlockKind::List {
                    root.facts.with_list_tight(summary.fold.list_is_tight())
                } else {
                    root.facts
                },
                epoch: root
                    .epoch
                    .checked_add(1)
                    .ok_or(HierarchicalGreenError::Overflow("epoch"))?,
                summary,
                ..root
            },
            next_sequence,
            receipt,
        )?;
        let next_manifest = allocate_manifest(
            &mut transaction,
            DecodedManifest {
                epoch: manifest
                    .epoch
                    .checked_add(1)
                    .ok_or(HierarchicalGreenError::Overflow("manifest epoch"))?,
                source_revision: manifest
                    .source_revision
                    .checked_add(1)
                    .ok_or(HierarchicalGreenError::Overflow("source revision"))?,
                summary,
                ..manifest
            },
            next_root,
            receipt,
        )?;
        merge_sequence_receipt(receipt, sequence_receipt);
        sync_transaction_receipt(receipt, &transaction);
        Ok(Self {
            owner: transaction.take(next_manifest),
        })
    }

    pub fn release_later(self, arena: &mut PageArena) -> Result<(), HierarchicalGreenError> {
        arena
            .release_later(self.owner)
            .map_err(legacy_owner_transfer_error)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HierarchicalSourceCursor {
    manifest: ArenaId,
    leaf: ArenaId,
    leaf_index: u64,
    local_index: u16,
    block: u64,
    coverage: u64,
}

impl HierarchicalSourceCursor {
    #[must_use]
    pub const fn page_id(self) -> ArenaId {
        self.leaf
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HierarchicalQueryReceipt {
    pub sequence_nodes_visited: usize,
    pub entries_examined: usize,
    pub child_documents_descended: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HierarchicalSourceHit {
    pub owner: u64,
    pub coverage: u64,
    pub kind: GenericBlockKind,
    pub source_kind: GenericSourceKind,
    pub facts: CompactBlockFacts,
    pub enclosing: Vec<u64>,
    pub byte_range: Range<u64>,
    pub utf16_range: Range<u64>,
    pub cursor: HierarchicalSourceCursor,
    pub receipt: HierarchicalQueryReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HierarchicalViewportEntry {
    pub owner: u64,
    pub coverage: u64,
    pub kind: GenericBlockKind,
    pub facts: CompactBlockFacts,
    pub byte_range: Range<u64>,
    pub utf16_range: Range<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HierarchicalViewportReceipt {
    pub sequence_nodes_visited: usize,
    pub leaves_visited: usize,
    pub entries_examined: usize,
    pub summary_nodes_skipped: usize,
    pub maximum_stack: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HierarchicalRetainedReceipt {
    pub live_nodes: usize,
    pub live_payload_bytes: usize,
    pub live_edge_bytes: usize,
    pub live_storage_bytes: usize,
    pub slot_capacity: usize,
    pub slot_storage_bytes: usize,
    pub heap_page_allocations: usize,
    pub modeled_allocator_overhead_bytes: usize,
    pub root_handle_bytes: usize,
    pub accounted_retained_bytes: usize,
    pub high_water_storage_bytes: usize,
}

#[must_use]
pub fn hierarchical_retained_receipt(
    arena: &PageArena,
    retained_root_handles: usize,
) -> HierarchicalRetainedReceipt {
    let metrics = arena.metrics();
    let allocator = metrics.heap_page_allocations * MODELED_ALLOCATOR_BYTES_PER_PAGE;
    let handles = retained_root_handles * std::mem::size_of::<ArenaId>();
    HierarchicalRetainedReceipt {
        live_nodes: metrics.live_nodes,
        live_payload_bytes: metrics.live_payload_bytes,
        live_edge_bytes: metrics.live_edge_bytes,
        live_storage_bytes: metrics.live_storage_bytes,
        slot_capacity: metrics.slot_capacity,
        slot_storage_bytes: metrics.slot_storage_bytes,
        heap_page_allocations: metrics.heap_page_allocations,
        modeled_allocator_overhead_bytes: allocator,
        root_handle_bytes: handles,
        accounted_retained_bytes: metrics.live_storage_bytes
            + metrics.slot_storage_bytes
            + allocator
            + handles,
        high_water_storage_bytes: metrics.high_water_storage_bytes,
    }
}

fn validate_entry(
    arena: &PageArena,
    entry: HierarchicalSiblingEntry,
) -> Result<(), HierarchicalGreenError> {
    if entry.block == 0
        || entry.coverage == 0
        || entry.local_metric.bytes == 0
        || entry.local_metric.utf16 == 0
        || entry.local_metric.bytes < entry.local_metric.utf16
    {
        return Err(HierarchicalGreenError::Invalid("invalid sibling entry"));
    }
    if let Some(child) = entry.child {
        let _ = decode_document(arena, child)?;
    }
    Ok(())
}

fn entry_fits(existing: &[HierarchicalSiblingEntry], next: HierarchicalSiblingEntry) -> bool {
    let entries = existing.len() + 1;
    let edges = existing
        .iter()
        .filter(|entry| entry.child.is_some())
        .count()
        + usize::from(next.child.is_some());
    LEAF_HEADER_BYTES + entries * ENTRY_BYTES + edges * 8 <= ARENA_PAGE_BYTES
}

fn entry_fits_existing(entries: &[HierarchicalSiblingEntry]) -> bool {
    let edges = entries.iter().filter(|entry| entry.child.is_some()).count();
    LEAF_HEADER_BYTES + entries.len() * ENTRY_BYTES + edges * 8 <= ARENA_PAGE_BYTES
}

fn flush_page(
    transaction: &mut ArenaBuildTransaction<'_>,
    sequence: &mut StreamingSequenceBuilder<SiblingSequenceSpec>,
    sequence_receipt: &mut SequenceMutationReceipt,
    page: &mut Vec<HierarchicalSiblingEntry>,
    receipt: &mut HierarchicalBuildReceipt,
) -> Result<(), HierarchicalGreenError> {
    let (payload, edges) = encode_leaf(transaction.arena(), page)?;
    receipt.maximum_encoded_page_buffer_bytes = receipt
        .maximum_encoded_page_buffer_bytes
        .max(payload.capacity());
    receipt.maximum_edge_buffer_bytes = receipt
        .maximum_edge_buffer_bytes
        .max(edges.capacity() * std::mem::size_of::<ArenaId>());
    let (leaf, allocation) = transaction.allocate_packed(&payload, &edges)?;
    receipt.leaf_pages_allocated += 1;
    receipt.payload_bytes_copied += allocation.payload_bytes_copied;
    receipt.edge_bytes_copied += allocation.edge_bytes_copied;
    sequence.push_handle(transaction, leaf, sequence_receipt)?;
    page.clear();
    Ok(())
}

fn encode_leaf(
    arena: &PageArena,
    entries: &[HierarchicalSiblingEntry],
) -> Result<(Vec<u8>, Vec<ArenaId>), HierarchicalGreenError> {
    if entries.is_empty() {
        return Err(HierarchicalGreenError::Invalid("empty sibling page"));
    }
    let mut metric = GenericGreenMetric::default();
    let mut fold = ChildSequenceAggregate::default();
    let mut edges = Vec::new();
    for entry in entries {
        validate_entry(arena, *entry)?;
        metric = checked_metric_add(metric, entry_total_metric(arena, *entry)?)?;
        fold = fold.followed_by(ChildSequenceAggregate::singleton(entry.contribution));
        if let Some(child) = entry.child {
            edges.push(child);
        }
    }
    let mut payload = Vec::with_capacity(LEAF_HEADER_BYTES + entries.len() * ENTRY_BYTES);
    payload.push(SIBLING_LEAF_TAG);
    payload.push(VERSION);
    push_u16(
        &mut payload,
        u16::try_from(entries.len())
            .map_err(|_| HierarchicalGreenError::Overflow("leaf entry count"))?,
    );
    push_u64(&mut payload, metric.bytes);
    push_u64(&mut payload, metric.utf16);
    payload.push(encode_fold(fold));
    payload.extend_from_slice(&[0; 3]);
    push_u64(&mut payload, entries[0].block);
    let mut next_edge = 0_u16;
    for entry in entries {
        push_u64(&mut payload, entry.block);
        push_u64(&mut payload, entry.coverage);
        push_u64(&mut payload, entry.facts.0);
        push_u32(
            &mut payload,
            u32::try_from(entry.local_metric.bytes)
                .map_err(|_| HierarchicalGreenError::Overflow("local bytes"))?,
        );
        push_u32(
            &mut payload,
            u32::try_from(entry.local_metric.utf16)
                .map_err(|_| HierarchicalGreenError::Overflow("local UTF-16"))?,
        );
        payload.push(entry.kind as u8);
        payload.push(entry.source_kind as u8);
        payload.push(encode_closed(entry.contribution));
        payload.push(u8::from(entry.child.is_some()));
        if entry.child.is_some() {
            push_u16(&mut payload, next_edge);
            next_edge += 1;
        } else {
            push_u16(&mut payload, NO_EDGE);
        }
        push_u16(&mut payload, 0);
    }
    if payload.len() + edges.len() * 8 > ARENA_PAGE_BYTES {
        return Err(HierarchicalGreenError::Invalid(
            "encoded sibling page overflow",
        ));
    }
    Ok((payload, edges))
}

fn decode_leaf(
    arena: &PageArena,
    leaf: ArenaId,
) -> Result<Vec<HierarchicalSiblingEntry>, HierarchicalGreenError> {
    let payload = arena.payload(leaf)?;
    let declared = decode_leaf_header(payload)?;
    let mut decoder = Decoder::new(payload);
    decoder.take(LEAF_HEADER_BYTES)?;
    let mut entries = Vec::with_capacity(
        usize::try_from(declared.items)
            .map_err(|_| HierarchicalGreenError::Overflow("decoded leaf entries"))?,
    );
    let mut expected_edge = 0_u16;
    for _ in 0..declared.items {
        let block = decoder.u64()?;
        let coverage = decoder.u64()?;
        let facts = CompactBlockFacts(decoder.u64()?);
        let local_metric = GenericGreenMetric {
            bytes: u64::from(decoder.u32()?),
            utf16: u64::from(decoder.u32()?),
        };
        let kind = decode_kind(decoder.u8()?)?;
        let source_kind = decode_source_kind(decoder.u8()?)?;
        let contribution = decode_closed(decoder.u8()?)?;
        let has_child = match decoder.u8()? {
            0 => false,
            1 => true,
            _ => return Err(HierarchicalGreenError::Corrupt("invalid child flag")),
        };
        let edge = decoder.u16()?;
        if decoder.u16()? != 0 {
            return Err(HierarchicalGreenError::Corrupt("entry padding"));
        }
        let child = if has_child {
            if edge != expected_edge {
                return Err(HierarchicalGreenError::Corrupt("noncanonical child edge"));
            }
            expected_edge += 1;
            Some(arena.packed_child_at(leaf, usize::from(edge))?)
        } else {
            if edge != NO_EDGE {
                return Err(HierarchicalGreenError::Corrupt("unexpected child edge"));
            }
            None
        };
        entries.push(HierarchicalSiblingEntry {
            block,
            coverage,
            kind,
            source_kind,
            local_metric,
            facts,
            contribution,
            child,
        });
    }
    if !decoder.is_empty() || usize::from(expected_edge) != arena.packed_child_count(leaf)? {
        return Err(HierarchicalGreenError::Corrupt("leaf edge count mismatch"));
    }
    let (payload_check, edges_check) = encode_leaf(arena, &entries)?;
    if payload_check != payload
        || edges_check.len() != arena.packed_child_count(leaf)?
        || decode_leaf_header(&payload_check)? != declared
    {
        return Err(HierarchicalGreenError::Corrupt("leaf roundtrip mismatch"));
    }
    Ok(entries)
}

fn decode_leaf_header(payload: &[u8]) -> Result<SiblingSummary, HierarchicalGreenError> {
    if payload.len() < LEAF_HEADER_BYTES {
        return Err(HierarchicalGreenError::Corrupt("short sibling leaf"));
    }
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != SIBLING_LEAF_TAG || decoder.u8()? != VERSION {
        return Err(HierarchicalGreenError::Corrupt("wrong sibling leaf header"));
    }
    let items = u64::from(decoder.u16()?);
    let metric = GenericGreenMetric {
        bytes: decoder.u64()?,
        utf16: decoder.u64()?,
    };
    let fold = decode_fold(decoder.u8()?)?;
    if decoder.take(3)? != [0; 3] || decoder.u64()? == 0 {
        return Err(HierarchicalGreenError::Corrupt(
            "invalid sibling leaf summary",
        ));
    }
    let expected = LEAF_HEADER_BYTES
        + usize::try_from(items).map_err(|_| HierarchicalGreenError::Overflow("leaf items"))?
            * ENTRY_BYTES;
    if items == 0 || payload.len() != expected {
        return Err(HierarchicalGreenError::Corrupt(
            "sibling leaf size mismatch",
        ));
    }
    Ok(SiblingSummary {
        leaves: 1,
        items,
        height: 1,
        metric,
        fold,
    })
}

fn encode_branch(summary: SiblingSummary) -> [u8; BRANCH_BYTES] {
    let mut output = Vec::with_capacity(BRANCH_BYTES);
    output.push(SIBLING_BRANCH_TAG);
    output.push(VERSION);
    push_u16(&mut output, summary.height);
    push_u64(&mut output, summary.leaves);
    push_u64(&mut output, summary.items);
    push_u64(&mut output, summary.metric.bytes);
    push_u64(&mut output, summary.metric.utf16);
    output.push(encode_fold(summary.fold));
    output.extend_from_slice(&[0; 3]);
    output.try_into().expect("fixed sibling branch")
}

fn decode_branch(payload: &[u8]) -> Result<SiblingSummary, HierarchicalGreenError> {
    if payload.len() != BRANCH_BYTES {
        return Err(HierarchicalGreenError::Corrupt("wrong sibling branch size"));
    }
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != SIBLING_BRANCH_TAG || decoder.u8()? != VERSION {
        return Err(HierarchicalGreenError::Corrupt(
            "wrong sibling branch header",
        ));
    }
    let summary = SiblingSummary {
        height: decoder.u16()?,
        leaves: decoder.u64()?,
        items: decoder.u64()?,
        metric: GenericGreenMetric {
            bytes: decoder.u64()?,
            utf16: decoder.u64()?,
        },
        fold: decode_fold(decoder.u8()?)?,
    };
    if decoder.take(3)? != [0; 3] || !decoder.is_empty() || summary.height < 2 || summary.leaves < 2
    {
        return Err(HierarchicalGreenError::Corrupt("invalid sibling branch"));
    }
    Ok(summary)
}

fn allocate_root(
    transaction: &mut ArenaBuildTransaction<'_>,
    root: DecodedRoot,
    sequence: ArenaOwnerHandle,
    receipt: &mut HierarchicalBuildReceipt,
) -> Result<ArenaOwnerHandle, HierarchicalGreenError> {
    let payload = encode_root(root);
    let (owner, allocation) = transaction.allocate(&payload, &[transaction.id(&sequence)])?;
    transaction.release(sequence)?;
    receipt.root_nodes_allocated += 1;
    receipt.payload_bytes_copied += allocation.payload_bytes_copied;
    receipt.edge_bytes_copied += allocation.edge_bytes_copied;
    Ok(owner)
}

fn allocate_manifest(
    transaction: &mut ArenaBuildTransaction<'_>,
    manifest: DecodedManifest,
    root: ArenaOwnerHandle,
    receipt: &mut HierarchicalBuildReceipt,
) -> Result<ArenaOwnerHandle, HierarchicalGreenError> {
    let payload = encode_manifest(manifest);
    let (owner, allocation) = transaction.allocate(&payload, &[transaction.id(&root)])?;
    transaction.release(root)?;
    receipt.manifest_nodes_allocated += 1;
    receipt.payload_bytes_copied += allocation.payload_bytes_copied;
    receipt.edge_bytes_copied += allocation.edge_bytes_copied;
    Ok(owner)
}

fn encode_root(root: DecodedRoot) -> [u8; ROOT_BYTES] {
    let mut output = Vec::with_capacity(ROOT_BYTES);
    output.push(GREEN_ROOT_TAG);
    output.push(VERSION);
    output.push(root.kind as u8);
    output.push(0);
    push_u64(&mut output, root.block);
    push_u64(&mut output, root.facts.0);
    push_u64(&mut output, root.epoch);
    push_u64(&mut output, root.summary.items);
    push_u64(&mut output, root.summary.metric.bytes);
    push_u64(&mut output, root.summary.metric.utf16);
    output.push(encode_fold(root.summary.fold));
    output.extend_from_slice(&[0; 3]);
    output.try_into().expect("fixed hierarchical root")
}

fn decode_root(payload: &[u8]) -> Result<DecodedRoot, HierarchicalGreenError> {
    if payload.len() != ROOT_BYTES {
        return Err(HierarchicalGreenError::Corrupt("wrong green root size"));
    }
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != GREEN_ROOT_TAG || decoder.u8()? != VERSION {
        return Err(HierarchicalGreenError::Corrupt("wrong green root header"));
    }
    let kind = decode_kind(decoder.u8()?)?;
    if decoder.u8()? != 0 {
        return Err(HierarchicalGreenError::Corrupt("green root padding"));
    }
    let root = DecodedRoot {
        kind,
        block: decoder.u64()?,
        facts: CompactBlockFacts(decoder.u64()?),
        epoch: decoder.u64()?,
        summary: SiblingSummary {
            leaves: 0,
            height: 0,
            items: decoder.u64()?,
            metric: GenericGreenMetric {
                bytes: decoder.u64()?,
                utf16: decoder.u64()?,
            },
            fold: decode_fold(decoder.u8()?)?,
        },
    };
    if decoder.take(3)? != [0; 3] || !decoder.is_empty() || root.block == 0 || root.epoch == 0 {
        return Err(HierarchicalGreenError::Corrupt("invalid green root"));
    }
    Ok(root)
}

fn encode_manifest(manifest: DecodedManifest) -> [u8; MANIFEST_BYTES] {
    let mut output = Vec::with_capacity(MANIFEST_BYTES);
    output.push(GREEN_MANIFEST_TAG);
    output.push(VERSION);
    output.extend_from_slice(&[0; 2]);
    push_u64(&mut output, manifest.epoch);
    push_u64(&mut output, manifest.source_revision);
    push_u64(&mut output, manifest.block);
    push_u64(&mut output, manifest.summary.metric.bytes);
    push_u64(&mut output, manifest.summary.metric.utf16);
    push_u64(&mut output, manifest.summary.items);
    output.extend_from_slice(&[0; 4]);
    output.try_into().expect("fixed hierarchical manifest")
}

fn decode_manifest(payload: &[u8]) -> Result<DecodedManifest, HierarchicalGreenError> {
    if payload.len() != MANIFEST_BYTES {
        return Err(HierarchicalGreenError::Corrupt("wrong green manifest size"));
    }
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != GREEN_MANIFEST_TAG || decoder.u8()? != VERSION || decoder.take(2)? != [0; 2]
    {
        return Err(HierarchicalGreenError::Corrupt(
            "wrong green manifest header",
        ));
    }
    let manifest = DecodedManifest {
        epoch: decoder.u64()?,
        source_revision: decoder.u64()?,
        block: decoder.u64()?,
        summary: SiblingSummary {
            leaves: 0,
            height: 0,
            metric: GenericGreenMetric {
                bytes: decoder.u64()?,
                utf16: decoder.u64()?,
            },
            items: decoder.u64()?,
            fold: ChildSequenceAggregate::default(),
        },
    };
    if decoder.take(4)? != [0; 4]
        || !decoder.is_empty()
        || manifest.epoch == 0
        || manifest.source_revision == 0
        || manifest.block == 0
    {
        return Err(HierarchicalGreenError::Corrupt("invalid green manifest"));
    }
    Ok(manifest)
}

fn decode_document(
    arena: &PageArena,
    manifest_id: ArenaId,
) -> Result<(DecodedManifest, DecodedRoot, ArenaId), HierarchicalGreenError> {
    let manifest = decode_manifest(arena.payload(manifest_id)?)?;
    let root_id = arena.children(manifest_id)?[0]
        .ok_or(HierarchicalGreenError::Corrupt("manifest has no root"))?;
    let mut root = decode_root(arena.payload(root_id)?)?;
    let sequence = arena.children(root_id)?[0].ok_or(HierarchicalGreenError::Corrupt(
        "green root has no sequence",
    ))?;
    let summary = sequence_node::<SiblingSequenceSpec>(arena, sequence)?.0;
    root.summary.leaves = summary.leaves;
    root.summary.height = summary.height;
    if manifest.block != root.block
        || manifest.epoch != root.epoch
        || manifest.summary.items != summary.items
        || manifest.summary.metric != summary.metric
        || root.summary.items != summary.items
        || root.summary.metric != summary.metric
        || root.summary.fold != summary.fold
    {
        return Err(HierarchicalGreenError::Corrupt(
            "manifest/root summary mismatch",
        ));
    }
    Ok((manifest, root, sequence))
}

fn entry_total_metric(
    arena: &PageArena,
    entry: HierarchicalSiblingEntry,
) -> Result<GenericGreenMetric, HierarchicalGreenError> {
    let child_metric = entry
        .child
        .map(|child| decode_document(arena, child).map(|value| value.0.summary.metric))
        .transpose()?
        .unwrap_or_default();
    checked_metric_add(entry.local_metric, child_metric)
}

fn locate_leaf(
    arena: &PageArena,
    root: ArenaId,
    leaf_index: u64,
) -> Result<Option<(ArenaId, usize)>, HierarchicalGreenError> {
    if leaf_index >= sequence_node::<SiblingSequenceSpec>(arena, root)?.0.leaves {
        return Ok(None);
    }
    let mut node = root;
    let mut index = leaf_index;
    let mut visited = 0;
    loop {
        visited += 1;
        match sequence_node::<SiblingSequenceSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => return Ok(Some((node, visited))),
            SequenceNodeKind::Branch { left, right } => {
                let left_leaves = sequence_node::<SiblingSequenceSpec>(arena, left)?.0.leaves;
                if index < left_leaves {
                    node = left;
                } else {
                    index -= left_leaves;
                    node = right;
                }
            }
        }
    }
}

fn source_lookup_document(
    arena: &PageArena,
    manifest_id: ArenaId,
    offset: u64,
    coordinate: GenericCoordinate,
    affinity: GenericAffinity,
) -> Result<Option<HierarchicalSourceHit>, HierarchicalGreenError> {
    let (_, root, sequence) = decode_document(arena, manifest_id)?;
    let total = coordinate_value(root.summary.metric, coordinate);
    if offset > total {
        return Ok(None);
    }
    let mut node = sequence;
    let mut local_offset = offset;
    let mut byte_base = 0;
    let mut utf16_base = 0;
    let mut leaf_index = 0;
    let mut receipt = HierarchicalQueryReceipt::default();
    loop {
        receipt.sequence_nodes_visited += 1;
        match sequence_node::<SiblingSequenceSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => {
                let entries = decode_leaf(arena, node)?;
                let mut selected = None;
                let mut local_byte = byte_base;
                let mut local_utf16 = utf16_base;
                let mut cursor = 0;
                for (index, entry) in entries.iter().enumerate() {
                    receipt.entries_examined += 1;
                    let entry_metric = entry_total_metric(arena, *entry)?;
                    let length = coordinate_value(entry_metric, coordinate);
                    let end = cursor + length;
                    if local_offset < end
                        || (local_offset == end
                            && (affinity == GenericAffinity::Upstream
                                || index + 1 == entries.len()))
                    {
                        selected = Some((index, *entry, local_offset.saturating_sub(cursor)));
                        break;
                    }
                    cursor = end;
                    local_byte += entry_metric.bytes;
                    local_utf16 += entry_metric.utf16;
                }
                let (index, entry, within) = selected.ok_or(HierarchicalGreenError::NotFound)?;
                let local_length = coordinate_value(entry.local_metric, coordinate);
                if within > local_length
                    || (within == local_length
                        && entry.child.is_some()
                        && affinity == GenericAffinity::Downstream)
                {
                    let child = entry.child.ok_or(HierarchicalGreenError::Corrupt(
                        "query escaped local source without child",
                    ))?;
                    let child_offset = within.saturating_sub(local_length);
                    let mut child_hit =
                        source_lookup_document(arena, child, child_offset, coordinate, affinity)?
                            .ok_or(HierarchicalGreenError::NotFound)?;
                    receipt.child_documents_descended += 1;
                    receipt.sequence_nodes_visited += child_hit.receipt.sequence_nodes_visited;
                    receipt.entries_examined += child_hit.receipt.entries_examined;
                    let mut enclosing = vec![root.block, entry.block];
                    enclosing.extend(child_hit.enclosing);
                    child_hit.enclosing = enclosing;
                    child_hit.receipt = receipt;
                    return Ok(Some(child_hit));
                }
                let byte_range = local_byte..local_byte + entry.local_metric.bytes;
                let utf16_range = local_utf16..local_utf16 + entry.local_metric.utf16;
                return Ok(Some(HierarchicalSourceHit {
                    owner: entry.block,
                    coverage: entry.coverage,
                    kind: entry.kind,
                    source_kind: entry.source_kind,
                    facts: entry.facts,
                    enclosing: vec![root.block, entry.block],
                    byte_range,
                    utf16_range,
                    cursor: HierarchicalSourceCursor {
                        manifest: manifest_id,
                        leaf: node,
                        leaf_index,
                        local_index: u16::try_from(index)
                            .map_err(|_| HierarchicalGreenError::Overflow("local entry index"))?,
                        block: entry.block,
                        coverage: entry.coverage,
                    },
                    receipt,
                }));
            }
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<SiblingSequenceSpec>(arena, left)?.0;
                let left_length = coordinate_value(left_summary.metric, coordinate);
                if local_offset < left_length
                    || (local_offset == left_length && affinity == GenericAffinity::Upstream)
                {
                    node = left;
                } else {
                    local_offset -= left_length;
                    byte_base += left_summary.metric.bytes;
                    utf16_base += left_summary.metric.utf16;
                    leaf_index += left_summary.leaves;
                    node = right;
                }
            }
        }
    }
}

struct ViewTask {
    node: ArenaId,
    base: GenericGreenMetric,
}

fn viewport_document(
    arena: &PageArena,
    manifest_id: ArenaId,
    range: Range<u64>,
    coordinate: GenericCoordinate,
    receipt: &mut HierarchicalViewportReceipt,
) -> Result<Vec<HierarchicalViewportEntry>, HierarchicalGreenError> {
    if range.start > range.end {
        return Err(HierarchicalGreenError::Invalid("reversed viewport"));
    }
    let (_, _, sequence) = decode_document(arena, manifest_id)?;
    let mut output = Vec::new();
    let mut stack = vec![ViewTask {
        node: sequence,
        base: GenericGreenMetric::default(),
    }];
    while let Some(task) = stack.pop() {
        receipt.sequence_nodes_visited += 1;
        let (summary, kind) = sequence_node::<SiblingSequenceSpec>(arena, task.node)?;
        let end = checked_metric_add(task.base, summary.metric)?;
        if coordinate_value(task.base, coordinate) >= range.end
            || coordinate_value(end, coordinate) <= range.start
        {
            receipt.summary_nodes_skipped += 1;
            continue;
        }
        match kind {
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<SiblingSequenceSpec>(arena, left)?.0;
                stack.push(ViewTask {
                    node: right,
                    base: checked_metric_add(task.base, left_summary.metric)?,
                });
                stack.push(ViewTask {
                    node: left,
                    base: task.base,
                });
                receipt.maximum_stack = receipt.maximum_stack.max(stack.len());
            }
            SequenceNodeKind::Leaf => {
                receipt.leaves_visited += 1;
                let entries = decode_leaf(arena, task.node)?;
                let mut base = task.base;
                for entry in entries {
                    receipt.entries_examined += 1;
                    let total = entry_total_metric(arena, entry)?;
                    let end = checked_metric_add(base, total)?;
                    if coordinate_value(base, coordinate) < range.end
                        && coordinate_value(end, coordinate) > range.start
                    {
                        output.push(HierarchicalViewportEntry {
                            owner: entry.block,
                            coverage: entry.coverage,
                            kind: entry.kind,
                            facts: entry.facts,
                            byte_range: base.bytes..base.bytes + entry.local_metric.bytes,
                            utf16_range: base.utf16..base.utf16 + entry.local_metric.utf16,
                        });
                    }
                    base = end;
                }
            }
        }
    }
    Ok(output)
}

fn merge_sequence_receipt(
    receipt: &mut HierarchicalBuildReceipt,
    sequence: SequenceMutationReceipt,
) {
    receipt.branch_nodes_allocated += sequence.branches_allocated;
    receipt.payload_bytes_copied += sequence.branch_payload_bytes_copied;
    receipt.edge_bytes_copied += sequence.child_references_added * 8;
    receipt.sequence_nodes_visited += sequence.nodes_visited;
    receipt.sequence_leaves_reused += sequence.leaves_reused;
    receipt.maximum_streaming_roots = receipt
        .maximum_streaming_roots
        .max(sequence.maximum_streaming_roots);
    receipt.maximum_streaming_bin_bytes = receipt
        .maximum_streaming_bin_bytes
        .max(sequence.maximum_streaming_bin_bytes);
}

fn sync_transaction_receipt(
    receipt: &mut HierarchicalBuildReceipt,
    transaction: &ArenaBuildTransaction<'_>,
) {
    receipt.maximum_live_owner_handles = receipt
        .maximum_live_owner_handles
        .max(transaction.maximum_live_owners());
    receipt.owner_journal_capacity = receipt
        .owner_journal_capacity
        .max(transaction.owner_journal_capacity());
    receipt.owner_journal_bytes = receipt
        .owner_journal_bytes
        .max(transaction.owner_journal_bytes());
}

fn checked_metric_add(
    left: GenericGreenMetric,
    right: GenericGreenMetric,
) -> Result<GenericGreenMetric, HierarchicalGreenError> {
    Ok(GenericGreenMetric {
        bytes: left
            .bytes
            .checked_add(right.bytes)
            .ok_or(HierarchicalGreenError::Overflow("source bytes"))?,
        utf16: left
            .utf16
            .checked_add(right.utf16)
            .ok_or(HierarchicalGreenError::Overflow("source UTF-16"))?,
    })
}

const fn coordinate_value(metric: GenericGreenMetric, coordinate: GenericCoordinate) -> u64 {
    match coordinate {
        GenericCoordinate::Bytes => metric.bytes,
        GenericCoordinate::Utf16 => metric.utf16,
    }
}

fn decode_kind(value: u8) -> Result<GenericBlockKind, HierarchicalGreenError> {
    match value {
        1 => Ok(GenericBlockKind::Document),
        2 => Ok(GenericBlockKind::BlockQuote),
        3 => Ok(GenericBlockKind::List),
        4 => Ok(GenericBlockKind::Item),
        5 => Ok(GenericBlockKind::Table),
        6 => Ok(GenericBlockKind::TableRow),
        7 => Ok(GenericBlockKind::TableCell),
        8 => Ok(GenericBlockKind::Paragraph),
        9 => Ok(GenericBlockKind::Heading),
        10 => Ok(GenericBlockKind::FencedCode),
        11 => Ok(GenericBlockKind::Html),
        12 => Ok(GenericBlockKind::ThematicBreak),
        13 => Ok(GenericBlockKind::IndentedCode),
        _ => Err(HierarchicalGreenError::Corrupt("unknown block kind")),
    }
}

fn decode_source_kind(value: u8) -> Result<GenericSourceKind, HierarchicalGreenError> {
    match value {
        1 => Ok(GenericSourceKind::Terminal),
        2 => Ok(GenericSourceKind::Gap),
        3 => Ok(GenericSourceKind::ContainerMarker),
        _ => Err(HierarchicalGreenError::Corrupt("unknown source kind")),
    }
}

const fn encode_closed(value: ClosedChildAggregate) -> u8 {
    (value.ends_blank as u8)
        | ((value.item_loose_if_nonlast as u8) << 1)
        | ((value.item_loose_if_last as u8) << 2)
}

fn decode_closed(value: u8) -> Result<ClosedChildAggregate, HierarchicalGreenError> {
    if value & !0b111 != 0 {
        return Err(HierarchicalGreenError::Corrupt("invalid closed-child bits"));
    }
    Ok(ClosedChildAggregate {
        ends_blank: value & 1 != 0,
        item_loose_if_nonlast: value & 2 != 0,
        item_loose_if_last: value & 4 != 0,
    })
}

const fn encode_fold(value: ChildSequenceAggregate) -> u8 {
    (value.had_child as u8)
        | ((value.any_nonlast_child_ends_blank as u8) << 1)
        | ((value.last_child_ends_blank as u8) << 2)
        | ((value.list_loose_before_last as u8) << 3)
        | ((value.last_item_loose_if_nonlast as u8) << 4)
        | ((value.last_item_loose_if_last as u8) << 5)
}

fn decode_fold(value: u8) -> Result<ChildSequenceAggregate, HierarchicalGreenError> {
    if value & !0b11_1111 != 0 {
        return Err(HierarchicalGreenError::Corrupt("invalid child-fold bits"));
    }
    Ok(ChildSequenceAggregate {
        had_child: value & 1 != 0,
        any_nonlast_child_ends_blank: value & 2 != 0,
        last_child_ends_blank: value & 4 != 0,
        list_loose_before_last: value & 8 != 0,
        last_item_loose_if_nonlast: value & 16 != 0,
        last_item_loose_if_last: value & 32 != 0,
    })
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

struct Decoder<'a> {
    remaining: &'a [u8],
}

impl<'a> Decoder<'a> {
    const fn new(remaining: &'a [u8]) -> Self {
        Self { remaining }
    }

    const fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], HierarchicalGreenError> {
        let (value, remaining) = self
            .remaining
            .split_at_checked(count)
            .ok_or(HierarchicalGreenError::Corrupt("truncated scalar"))?;
        self.remaining = remaining;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, HierarchicalGreenError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, HierarchicalGreenError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("two-byte scalar"),
        ))
    }

    fn u32(&mut self) -> Result<u32, HierarchicalGreenError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four-byte scalar"),
        ))
    }

    fn u64(&mut self) -> Result<u64, HierarchicalGreenError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("eight-byte scalar"),
        ))
    }
}

//! Isolated hierarchical packed-green-tree representation challenger.
//!
//! This is not the selected output format. It intentionally exercises the
//! shared arena, streaming sequence, and top-level transaction while making
//! the representation's lookup and fanout costs measurable.

use std::fmt;

use crate::arena::{ArenaBuildTransaction, ArenaOwnerHandle};
use crate::persistent_sequence::{
    SequenceMutationReceipt, SequenceNodeKind, SequenceSpec, StreamingSequenceBuilder,
    sequence_node, splice_root_in_transaction,
};
use crate::record_forest::{ChildSequenceAggregate, ClosedChildAggregate, ForestBlockId};
use crate::{ARENA_PAGE_BYTES, ArenaError, ArenaId, OwnedArenaRef, OwnerTransferError, PageArena};

const VERSION: u8 = 1;
const CHILD_LEAF_TAG: u8 = 0x91;
const CHILD_BRANCH_TAG: u8 = 0x92;
const CONTAINER_TAG: u8 = 0x93;
const CHILD_HEADER_BYTES: usize = 28;
const CHILD_ENTRY_BYTES: usize = 48;
const CHILD_BRANCH_BYTES: usize = 44;
const CONTAINER_BYTES: usize = 52;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GreenMetric {
    pub bytes: u64,
    pub utf16: u64,
}

impl GreenMetric {
    fn checked_add(self, other: Self) -> Result<Self, GreenTreeError> {
        Ok(Self {
            bytes: self
                .bytes
                .checked_add(other.bytes)
                .ok_or(GreenTreeError::Overflow("source bytes"))?,
            utf16: self
                .utf16
                .checked_add(other.utf16)
                .ok_or(GreenTreeError::Overflow("source UTF-16"))?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum GreenContainerKind {
    Document = 1,
    BlockQuote = 2,
    List = 3,
}

impl GreenContainerKind {
    fn decode(value: u8) -> Result<Self, GreenTreeError> {
        match value {
            1 => Ok(Self::Document),
            2 => Ok(Self::BlockQuote),
            3 => Ok(Self::List),
            _ => Err(GreenTreeError::Corrupt("unknown green container kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenContainerSpec {
    pub block: ForestBlockId,
    pub kind: GreenContainerKind,
    /// Source prefix semantically owned by this container (for example `> `).
    pub prefix: GreenMetric,
    /// Trailing blank/gap source semantically owned by this container.
    pub suffix: GreenMetric,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum GreenGapOwner {
    Item = 1,
    List = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GreenItemParagraph {
    pub item: ForestBlockId,
    pub paragraph: ForestBlockId,
    pub marker: GreenMetric,
    pub content: GreenMetric,
    pub trailing_gap: GreenMetric,
    pub trailing_gap_owner: GreenGapOwner,
    pub contribution: ClosedChildAggregate,
}

impl GreenItemParagraph {
    fn metric(self) -> Result<GreenMetric, GreenTreeError> {
        self.marker
            .checked_add(self.content)?
            .checked_add(self.trailing_gap)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GreenSourcePart {
    ContainerMarker,
    ItemMarker,
    ParagraphContent,
    Gap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenSourceHit {
    pub owner: ForestBlockId,
    pub part: GreenSourcePart,
    pub enclosing: Vec<ForestBlockId>,
    pub receipt: GreenQueryReceipt,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GreenQueryReceipt {
    pub arena_nodes_visited: usize,
    pub packed_entries_examined: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GreenMutationReceipt {
    pub payload_bytes_copied: usize,
    pub leaf_pages_allocated: usize,
    pub branch_nodes_allocated: usize,
    pub container_nodes_allocated: usize,
    pub sequence_nodes_visited: usize,
    pub suffix_pages_reused: usize,
    pub child_references_added: usize,
    pub maximum_streaming_roots: usize,
    pub maximum_streaming_bin_bytes: usize,
    pub maximum_live_owner_handles: usize,
    pub owner_journal_capacity: usize,
    pub owner_journal_bytes: usize,
    pub maximum_typed_page_buffer_bytes: usize,
    pub maximum_leaf_buffer_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GreenTreeError {
    Arena(ArenaError),
    Corrupt(&'static str),
    Invalid(&'static str),
    Overflow(&'static str),
    NotFound,
}

impl From<ArenaError> for GreenTreeError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

fn legacy_owner_transfer_error(failure: OwnerTransferError) -> GreenTreeError {
    // This non-selected hierarchical challenger has a legacy copyable error
    // surface. Isolate its lossy adapter; selected paths preserve the owner.
    let OwnerTransferError { error, owner } = failure;
    drop(owner);
    GreenTreeError::Arena(error)
}

impl fmt::Display for GreenTreeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arena(error) => error.fmt(formatter),
            Self::Corrupt(message) => write!(formatter, "corrupt green tree: {message}"),
            Self::Invalid(message) => write!(formatter, "invalid green tree: {message}"),
            Self::Overflow(field) => write!(formatter, "green tree {field} overflow"),
            Self::NotFound => formatter.write_str("green tree value not found"),
        }
    }
}

impl std::error::Error for GreenTreeError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GreenSequenceSummary {
    leaves: u64,
    items: u64,
    height: u16,
    metric: GreenMetric,
    fold: ChildSequenceAggregate,
}

#[derive(Debug)]
struct GreenSequenceSpec;

impl SequenceSpec for GreenSequenceSpec {
    type Summary = GreenSequenceSummary;
    type Error = GreenTreeError;
    type BranchPayload = [u8; CHILD_BRANCH_BYTES];

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(CHILD_LEAF_TAG) {
            return Ok(None);
        }
        decode_child_page(payload).map(|entries| {
            let metric = fold_metric(&entries)?;
            let fold = fold_entries(&entries);
            Ok(Some(GreenSequenceSummary {
                leaves: 1,
                items: u64::try_from(entries.len())
                    .map_err(|_| GreenTreeError::Overflow("child count"))?,
                height: 1,
                metric,
                fold,
            }))
        })?
    }

    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() != Some(CHILD_BRANCH_TAG) {
            return Ok(None);
        }
        decode_branch(payload).map(Some)
    }

    fn encode_branch(summary: Self::Summary) -> Self::BranchPayload {
        encode_branch(summary)
    }

    fn combine(left: Self::Summary, right: Self::Summary) -> Result<Self::Summary, Self::Error> {
        Ok(GreenSequenceSummary {
            leaves: left
                .leaves
                .checked_add(right.leaves)
                .ok_or(GreenTreeError::Overflow("sequence leaves"))?,
            items: left
                .items
                .checked_add(right.items)
                .ok_or(GreenTreeError::Overflow("sequence items"))?,
            height: left
                .height
                .max(right.height)
                .checked_add(1)
                .ok_or(GreenTreeError::Overflow("sequence height"))?,
            metric: left.metric.checked_add(right.metric)?,
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
        GreenTreeError::Corrupt(message)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DecodedContainer {
    spec: GreenContainerSpec,
    child_metric: GreenMetric,
    child_count: u64,
}

#[derive(Debug)]
pub struct GreenTree {
    owner: OwnedArenaRef,
}

impl GreenTree {
    pub fn build_list(
        arena: &mut PageArena,
        containers: &[GreenContainerSpec],
        items: impl IntoIterator<Item = GreenItemParagraph>,
        receipt: &mut GreenMutationReceipt,
    ) -> Result<Self, GreenTreeError> {
        validate_container_specs(containers)?;
        let mut transaction = ArenaBuildTransaction::new(arena);
        let mut sequence = StreamingSequenceBuilder::<GreenSequenceSpec>::default();
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let mut page = Vec::with_capacity(child_page_capacity());
        receipt.maximum_typed_page_buffer_bytes = receipt
            .maximum_typed_page_buffer_bytes
            .max(page.capacity() * std::mem::size_of::<GreenItemParagraph>());
        for item in items {
            validate_item(item)?;
            page.push(item);
            if page.len() == child_page_capacity() {
                flush_child_page(
                    &mut transaction,
                    &mut sequence,
                    &mut sequence_receipt,
                    &mut page,
                    receipt,
                )?;
            }
        }
        if !page.is_empty() {
            flush_child_page(
                &mut transaction,
                &mut sequence,
                &mut sequence_receipt,
                &mut page,
                receipt,
            )?;
        }
        let sequence_root = sequence.finish(&mut transaction, &mut sequence_receipt)?;
        merge_sequence_receipt(receipt, sequence_receipt);
        let (child_metric, child_count) = if let Some(root) = sequence_root.as_ref() {
            let summary =
                sequence_node::<GreenSequenceSpec>(transaction.arena(), transaction.id(root))?.0;
            (summary.metric, summary.items)
        } else {
            (GreenMetric::default(), 0)
        };
        let root = build_container_chain(
            &mut transaction,
            containers,
            sequence_root,
            child_metric,
            child_count,
            receipt,
        )?;
        sync_transaction_receipt(receipt, &transaction);
        Ok(Self {
            owner: transaction.take(root),
        })
    }

    #[must_use]
    pub const fn root_id(&self) -> ArenaId {
        self.owner.id()
    }

    pub fn metric(&self, arena: &PageArena) -> Result<GreenMetric, GreenTreeError> {
        decode_container(arena.payload(self.owner.id())?).and_then(container_metric)
    }

    pub fn list_is_tight(&self, arena: &PageArena) -> Result<bool, GreenTreeError> {
        let (_, sequence_root, _) = decode_chain(arena, self.owner.id())?;
        let Some(sequence_root) = sequence_root else {
            return Ok(true);
        };
        Ok(sequence_node::<GreenSequenceSpec>(arena, sequence_root)?
            .0
            .fold
            .list_is_tight())
    }

    pub fn child_page_count(&self, arena: &PageArena) -> Result<u64, GreenTreeError> {
        let (_, sequence_root, _) = decode_chain(arena, self.owner.id())?;
        sequence_root
            .map(|root| sequence_node::<GreenSequenceSpec>(arena, root).map(|node| node.0.leaves))
            .transpose()
            .map(Option::unwrap_or_default)
    }

    pub fn child_page_at(
        &self,
        arena: &PageArena,
        page_index: u64,
    ) -> Result<Option<ArenaId>, GreenTreeError> {
        let (_, sequence_root, _) = decode_chain(arena, self.owner.id())?;
        locate_leaf(arena, sequence_root, page_index)
    }

    pub fn source_lookup_bytes(
        &self,
        arena: &PageArena,
        byte_offset: u64,
    ) -> Result<Option<GreenSourceHit>, GreenTreeError> {
        source_lookup(arena, self.owner.id(), byte_offset, false)
    }

    pub fn source_lookup_utf16(
        &self,
        arena: &PageArena,
        utf16_offset: u64,
    ) -> Result<Option<GreenSourceHit>, GreenTreeError> {
        source_lookup(arena, self.owner.id(), utf16_offset, true)
    }

    pub fn replace_item(
        &self,
        arena: &mut PageArena,
        item_index: u64,
        expected_item: ForestBlockId,
        replacement: GreenItemParagraph,
        receipt: &mut GreenMutationReceipt,
    ) -> Result<Self, GreenTreeError> {
        validate_item(replacement)?;
        let (containers, sequence_root, _) = decode_chain(arena, self.owner.id())?;
        let sequence_root = sequence_root.ok_or(GreenTreeError::NotFound)?;
        let location =
            locate_item(arena, sequence_root, item_index)?.ok_or(GreenTreeError::NotFound)?;
        let mut entries = decode_child_page(arena.payload(location.page)?)?;
        let local = usize::try_from(location.local_item)
            .map_err(|_| GreenTreeError::Overflow("local child index"))?;
        if entries[local].item != expected_item || replacement.item != expected_item {
            return Err(GreenTreeError::Invalid("item identity changed"));
        }
        entries[local] = replacement;
        let payload = encode_child_page(&entries)?;
        let mut transaction = ArenaBuildTransaction::new(arena);
        let (leaf, allocation) = transaction.allocate(&payload, &[])?;
        receipt.payload_bytes_copied += allocation.payload_bytes_copied;
        receipt.leaf_pages_allocated += 1;
        receipt.maximum_leaf_buffer_bytes =
            receipt.maximum_leaf_buffer_bytes.max(payload.capacity());
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let next_sequence = splice_root_in_transaction::<GreenSequenceSpec>(
            &mut transaction,
            Some(sequence_root),
            location.leaf_index..location.leaf_index + 1,
            vec![leaf],
            &mut sequence_receipt,
        )?
        .ok_or(GreenTreeError::Corrupt(
            "replacement removed child sequence",
        ))?;
        let summary = sequence_node::<GreenSequenceSpec>(
            transaction.arena(),
            transaction.id(&next_sequence),
        )?
        .0;
        let root = build_container_chain(
            &mut transaction,
            &containers
                .iter()
                .map(|value| value.spec)
                .collect::<Vec<_>>(),
            Some(next_sequence),
            summary.metric,
            summary.items,
            receipt,
        )?;
        receipt.sequence_nodes_visited += location.nodes_visited;
        merge_sequence_receipt(receipt, sequence_receipt);
        sync_transaction_receipt(receipt, &transaction);
        Ok(Self {
            owner: transaction.take(root),
        })
    }

    /// Deliberately linear fallback used to expose the cost of omitting a
    /// global `BlockId` directory.
    pub fn find_block_linear(
        &self,
        arena: &PageArena,
        block: ForestBlockId,
    ) -> Result<(bool, GreenQueryReceipt), GreenTreeError> {
        let (containers, sequence_root, mut receipt) = decode_chain(arena, self.owner.id())?;
        if containers
            .iter()
            .any(|container| container.spec.block == block)
        {
            return Ok((true, receipt));
        }
        let Some(root) = sequence_root else {
            return Ok((false, receipt));
        };
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            receipt.arena_nodes_visited += 1;
            match sequence_node::<GreenSequenceSpec>(arena, node)?.1 {
                SequenceNodeKind::Leaf => {
                    for entry in decode_child_page(arena.payload(node)?)? {
                        receipt.packed_entries_examined += 1;
                        if entry.item == block || entry.paragraph == block {
                            return Ok((true, receipt));
                        }
                    }
                }
                SequenceNodeKind::Branch { left, right } => {
                    stack.push(right);
                    stack.push(left);
                }
            }
        }
        Ok((false, receipt))
    }

    pub fn release_later(self, arena: &mut PageArena) -> Result<(), GreenTreeError> {
        arena
            .release_later(self.owner)
            .map_err(legacy_owner_transfer_error)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
struct ItemLocation {
    page: ArenaId,
    leaf_index: u64,
    local_item: u64,
    nodes_visited: usize,
}

fn child_page_capacity() -> usize {
    (ARENA_PAGE_BYTES - CHILD_HEADER_BYTES) / CHILD_ENTRY_BYTES
}

fn validate_container_specs(containers: &[GreenContainerSpec]) -> Result<(), GreenTreeError> {
    if containers.is_empty()
        || containers.last().map(|value| value.kind) != Some(GreenContainerKind::List)
        || containers.iter().any(|value| value.block.0 == 0)
    {
        return Err(GreenTreeError::Invalid(
            "container chain must end in a nonzero list",
        ));
    }
    Ok(())
}

fn validate_item(item: GreenItemParagraph) -> Result<(), GreenTreeError> {
    if item.item.0 == 0 || item.paragraph.0 == 0 || item.item == item.paragraph {
        return Err(GreenTreeError::Invalid("invalid item microtree identity"));
    }
    let _ = item.metric()?;
    Ok(())
}

fn flush_child_page(
    transaction: &mut ArenaBuildTransaction<'_>,
    builder: &mut StreamingSequenceBuilder<GreenSequenceSpec>,
    sequence_receipt: &mut SequenceMutationReceipt,
    page: &mut Vec<GreenItemParagraph>,
    receipt: &mut GreenMutationReceipt,
) -> Result<(), GreenTreeError> {
    let payload = encode_child_page(page)?;
    receipt.maximum_leaf_buffer_bytes = receipt.maximum_leaf_buffer_bytes.max(payload.capacity());
    let (leaf, allocation) = transaction.allocate(&payload, &[])?;
    receipt.payload_bytes_copied += allocation.payload_bytes_copied;
    receipt.leaf_pages_allocated += 1;
    builder.push_handle(transaction, leaf, sequence_receipt)?;
    page.clear();
    Ok(())
}

fn build_container_chain(
    transaction: &mut ArenaBuildTransaction<'_>,
    containers: &[GreenContainerSpec],
    mut child: Option<ArenaOwnerHandle>,
    mut child_metric: GreenMetric,
    mut child_count: u64,
    receipt: &mut GreenMutationReceipt,
) -> Result<ArenaOwnerHandle, GreenTreeError> {
    for spec in containers.iter().rev() {
        let decoded = DecodedContainer {
            spec: *spec,
            child_metric,
            child_count,
        };
        let payload = encode_container(decoded)?;
        let children = child
            .as_ref()
            .map_or_else(Vec::new, |child| vec![transaction.id(child)]);
        let (next, allocation) = transaction.allocate(&payload, &children)?;
        if let Some(child) = child {
            transaction.release(child)?;
        }
        receipt.payload_bytes_copied += allocation.payload_bytes_copied;
        receipt.child_references_added += allocation.child_references_added;
        receipt.container_nodes_allocated += 1;
        child_metric = container_metric(decoded)?;
        child_count = 1;
        child = Some(next);
    }
    child.ok_or(GreenTreeError::Invalid("empty green container chain"))
}

fn sync_transaction_receipt(
    receipt: &mut GreenMutationReceipt,
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

fn merge_sequence_receipt(receipt: &mut GreenMutationReceipt, sequence: SequenceMutationReceipt) {
    receipt.branch_nodes_allocated += sequence.branches_allocated;
    receipt.payload_bytes_copied += sequence.branch_payload_bytes_copied;
    receipt.sequence_nodes_visited += sequence.nodes_visited;
    receipt.suffix_pages_reused += sequence.leaves_reused;
    receipt.child_references_added += sequence.child_references_added;
    receipt.maximum_streaming_roots = receipt
        .maximum_streaming_roots
        .max(sequence.maximum_streaming_roots);
    receipt.maximum_streaming_bin_bytes = receipt
        .maximum_streaming_bin_bytes
        .max(sequence.maximum_streaming_bin_bytes);
}

fn fold_metric(entries: &[GreenItemParagraph]) -> Result<GreenMetric, GreenTreeError> {
    entries
        .iter()
        .try_fold(GreenMetric::default(), |metric, entry| {
            metric.checked_add(entry.metric()?)
        })
}

fn fold_entries(entries: &[GreenItemParagraph]) -> ChildSequenceAggregate {
    entries
        .iter()
        .fold(ChildSequenceAggregate::default(), |fold, entry| {
            fold.followed_by(ChildSequenceAggregate::singleton(entry.contribution))
        })
}

fn encode_child_page(entries: &[GreenItemParagraph]) -> Result<Vec<u8>, GreenTreeError> {
    if entries.is_empty() || entries.len() > child_page_capacity() {
        return Err(GreenTreeError::Invalid("invalid green child page count"));
    }
    let metric = fold_metric(entries)?;
    let fold = fold_entries(entries);
    let mut output = Vec::with_capacity(CHILD_HEADER_BYTES + entries.len() * CHILD_ENTRY_BYTES);
    output.push(CHILD_LEAF_TAG);
    output.push(VERSION);
    push_u16(
        &mut output,
        u16::try_from(entries.len()).map_err(|_| GreenTreeError::Overflow("child page count"))?,
    );
    push_u64(&mut output, metric.bytes);
    push_u64(&mut output, metric.utf16);
    output.push(encode_fold(fold));
    output.extend_from_slice(&[0; 7]);
    for entry in entries {
        push_u64(&mut output, entry.item.0);
        push_u64(&mut output, entry.paragraph.0);
        encode_metric_u32(entry.marker, &mut output)?;
        encode_metric_u32(entry.content, &mut output)?;
        encode_metric_u32(entry.trailing_gap, &mut output)?;
        output.push(entry.trailing_gap_owner as u8);
        output.push(encode_closed(entry.contribution));
        output.extend_from_slice(&[0; 6]);
    }
    debug_assert_eq!(
        output.len(),
        CHILD_HEADER_BYTES + entries.len() * CHILD_ENTRY_BYTES
    );
    Ok(output)
}

fn decode_child_page(payload: &[u8]) -> Result<Vec<GreenItemParagraph>, GreenTreeError> {
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != CHILD_LEAF_TAG || decoder.u8()? != VERSION {
        return Err(GreenTreeError::Corrupt("wrong green child page header"));
    }
    let count = usize::from(decoder.u16()?);
    let declared_metric = GreenMetric {
        bytes: decoder.u64()?,
        utf16: decoder.u64()?,
    };
    let declared_fold = decode_fold(decoder.u8()?)?;
    if decoder.take(7)? != [0; 7] {
        return Err(GreenTreeError::Corrupt("green child header padding"));
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let item = ForestBlockId(decoder.u64()?);
        let paragraph = ForestBlockId(decoder.u64()?);
        let marker = decoder.metric_u32()?;
        let content = decoder.metric_u32()?;
        let trailing_gap = decoder.metric_u32()?;
        let trailing_gap_owner = match decoder.u8()? {
            1 => GreenGapOwner::Item,
            2 => GreenGapOwner::List,
            _ => return Err(GreenTreeError::Corrupt("unknown green gap owner")),
        };
        let contribution = decode_closed(decoder.u8()?)?;
        if decoder.take(6)? != [0; 6] {
            return Err(GreenTreeError::Corrupt("green child entry padding"));
        }
        let entry = GreenItemParagraph {
            item,
            paragraph,
            marker,
            content,
            trailing_gap,
            trailing_gap_owner,
            contribution,
        };
        validate_item(entry)?;
        entries.push(entry);
    }
    if entries.is_empty()
        || !decoder.is_empty()
        || fold_metric(&entries)? != declared_metric
        || fold_entries(&entries) != declared_fold
    {
        return Err(GreenTreeError::Corrupt("green child page summary mismatch"));
    }
    Ok(entries)
}

fn encode_branch(summary: GreenSequenceSummary) -> [u8; CHILD_BRANCH_BYTES] {
    let mut output = Vec::with_capacity(CHILD_BRANCH_BYTES);
    output.push(CHILD_BRANCH_TAG);
    output.push(VERSION);
    push_u16(&mut output, summary.height);
    push_u64(&mut output, summary.leaves);
    push_u64(&mut output, summary.items);
    push_u64(&mut output, summary.metric.bytes);
    push_u64(&mut output, summary.metric.utf16);
    output.push(encode_fold(summary.fold));
    output.extend_from_slice(&[0; 7]);
    output
        .try_into()
        .expect("fixed green sequence branch encoding")
}

fn decode_branch(payload: &[u8]) -> Result<GreenSequenceSummary, GreenTreeError> {
    if payload.len() != CHILD_BRANCH_BYTES {
        return Err(GreenTreeError::Corrupt("wrong green branch size"));
    }
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != CHILD_BRANCH_TAG || decoder.u8()? != VERSION {
        return Err(GreenTreeError::Corrupt("wrong green branch header"));
    }
    let summary = GreenSequenceSummary {
        height: decoder.u16()?,
        leaves: decoder.u64()?,
        items: decoder.u64()?,
        metric: GreenMetric {
            bytes: decoder.u64()?,
            utf16: decoder.u64()?,
        },
        fold: decode_fold(decoder.u8()?)?,
    };
    if decoder.take(7)? != [0; 7] || !decoder.is_empty() || summary.height < 2 || summary.leaves < 2
    {
        return Err(GreenTreeError::Corrupt("invalid green branch summary"));
    }
    Ok(summary)
}

fn encode_container(container: DecodedContainer) -> Result<[u8; CONTAINER_BYTES], GreenTreeError> {
    let mut output = Vec::with_capacity(CONTAINER_BYTES);
    output.push(CONTAINER_TAG);
    output.push(VERSION);
    output.push(container.spec.kind as u8);
    output.push(0);
    push_u64(&mut output, container.spec.block.0);
    encode_metric_u32(container.spec.prefix, &mut output)?;
    encode_metric_u32(container.spec.suffix, &mut output)?;
    push_u64(&mut output, container.child_metric.bytes);
    push_u64(&mut output, container.child_metric.utf16);
    push_u64(&mut output, container.child_count);
    output
        .try_into()
        .map_err(|_| GreenTreeError::Corrupt("fixed green container encoding"))
}

fn decode_container(payload: &[u8]) -> Result<DecodedContainer, GreenTreeError> {
    if payload.len() != CONTAINER_BYTES {
        return Err(GreenTreeError::Corrupt("wrong green container size"));
    }
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != CONTAINER_TAG || decoder.u8()? != VERSION {
        return Err(GreenTreeError::Corrupt("wrong green container header"));
    }
    let kind = GreenContainerKind::decode(decoder.u8()?)?;
    if decoder.u8()? != 0 {
        return Err(GreenTreeError::Corrupt("green container padding"));
    }
    let container = DecodedContainer {
        spec: GreenContainerSpec {
            kind,
            block: ForestBlockId(decoder.u64()?),
            prefix: decoder.metric_u32()?,
            suffix: decoder.metric_u32()?,
        },
        child_metric: GreenMetric {
            bytes: decoder.u64()?,
            utf16: decoder.u64()?,
        },
        child_count: decoder.u64()?,
    };
    if container.spec.block.0 == 0 || !decoder.is_empty() {
        return Err(GreenTreeError::Corrupt("invalid green container"));
    }
    Ok(container)
}

fn container_metric(container: DecodedContainer) -> Result<GreenMetric, GreenTreeError> {
    container
        .spec
        .prefix
        .checked_add(container.child_metric)?
        .checked_add(container.spec.suffix)
}

fn decode_chain(
    arena: &PageArena,
    root: ArenaId,
) -> Result<(Vec<DecodedContainer>, Option<ArenaId>, GreenQueryReceipt), GreenTreeError> {
    let mut containers = Vec::new();
    let mut current = root;
    let mut receipt = GreenQueryReceipt::default();
    loop {
        receipt.arena_nodes_visited += 1;
        let decoded = decode_container(arena.payload(current)?)?;
        containers.push(decoded);
        let children = arena.children(current)?;
        if children[1].is_some() || (decoded.child_count == 0) != children[0].is_none() {
            return Err(GreenTreeError::Corrupt(
                "green container child edge mismatch",
            ));
        }
        let Some(child) = children[0] else {
            return Ok((containers, None, receipt));
        };
        if arena.payload(child)?.first().copied() == Some(CONTAINER_TAG) {
            current = child;
        } else {
            return Ok((containers, Some(child), receipt));
        }
    }
}

fn locate_leaf(
    arena: &PageArena,
    root: Option<ArenaId>,
    leaf_index: u64,
) -> Result<Option<ArenaId>, GreenTreeError> {
    let Some(mut node) = root else {
        return Ok(None);
    };
    if leaf_index >= sequence_node::<GreenSequenceSpec>(arena, node)?.0.leaves {
        return Ok(None);
    }
    let mut index = leaf_index;
    loop {
        match sequence_node::<GreenSequenceSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => return Ok(Some(node)),
            SequenceNodeKind::Branch { left, right } => {
                let left_leaves = sequence_node::<GreenSequenceSpec>(arena, left)?.0.leaves;
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

fn locate_item(
    arena: &PageArena,
    root: ArenaId,
    item_index: u64,
) -> Result<Option<ItemLocation>, GreenTreeError> {
    if item_index >= sequence_node::<GreenSequenceSpec>(arena, root)?.0.items {
        return Ok(None);
    }
    let mut node = root;
    let mut index = item_index;
    let mut leaf_index = 0;
    let mut visited = 0;
    loop {
        visited += 1;
        match sequence_node::<GreenSequenceSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => {
                return Ok(Some(ItemLocation {
                    page: node,
                    leaf_index,
                    local_item: index,
                    nodes_visited: visited,
                }));
            }
            SequenceNodeKind::Branch { left, right } => {
                let left_summary = sequence_node::<GreenSequenceSpec>(arena, left)?.0;
                if index < left_summary.items {
                    node = left;
                } else {
                    index -= left_summary.items;
                    leaf_index += left_summary.leaves;
                    node = right;
                }
            }
        }
    }
}

fn source_lookup(
    arena: &PageArena,
    root: ArenaId,
    mut offset: u64,
    utf16: bool,
) -> Result<Option<GreenSourceHit>, GreenTreeError> {
    let mut node = root;
    let mut path = Vec::new();
    let mut receipt = GreenQueryReceipt::default();
    loop {
        receipt.arena_nodes_visited += 1;
        let container = decode_container(arena.payload(node)?)?;
        let total = coordinate(container_metric(container)?, utf16);
        if offset >= total {
            return Ok(None);
        }
        path.push(container.spec.block);
        let prefix = coordinate(container.spec.prefix, utf16);
        if offset < prefix {
            return Ok(Some(GreenSourceHit {
                owner: container.spec.block,
                part: GreenSourcePart::ContainerMarker,
                enclosing: path,
                receipt,
            }));
        }
        offset -= prefix;
        let child_len = coordinate(container.child_metric, utf16);
        if offset >= child_len {
            return Ok(Some(GreenSourceHit {
                owner: container.spec.block,
                part: GreenSourcePart::Gap,
                enclosing: path,
                receipt,
            }));
        }
        let child = arena.children(node)?[0]
            .ok_or(GreenTreeError::Corrupt("green source path has no child"))?;
        if arena.payload(child)?.first().copied() == Some(CONTAINER_TAG) {
            node = child;
            continue;
        }
        return lookup_sequence_source(arena, child, offset, utf16, path, receipt).map(Some);
    }
}

fn lookup_sequence_source(
    arena: &PageArena,
    mut node: ArenaId,
    mut offset: u64,
    utf16: bool,
    mut path: Vec<ForestBlockId>,
    mut receipt: GreenQueryReceipt,
) -> Result<GreenSourceHit, GreenTreeError> {
    loop {
        receipt.arena_nodes_visited += 1;
        match sequence_node::<GreenSequenceSpec>(arena, node)?.1 {
            SequenceNodeKind::Leaf => {
                let entries = decode_child_page(arena.payload(node)?)?;
                for entry in entries {
                    receipt.packed_entries_examined += 1;
                    let length = coordinate(entry.metric()?, utf16);
                    if offset >= length {
                        offset -= length;
                        continue;
                    }
                    path.push(entry.item);
                    let marker = coordinate(entry.marker, utf16);
                    if offset < marker {
                        return Ok(GreenSourceHit {
                            owner: entry.item,
                            part: GreenSourcePart::ItemMarker,
                            enclosing: path,
                            receipt,
                        });
                    }
                    offset -= marker;
                    let content = coordinate(entry.content, utf16);
                    if offset < content {
                        path.push(entry.paragraph);
                        return Ok(GreenSourceHit {
                            owner: entry.paragraph,
                            part: GreenSourcePart::ParagraphContent,
                            enclosing: path,
                            receipt,
                        });
                    }
                    if entry.trailing_gap_owner == GreenGapOwner::List {
                        path.pop();
                    }
                    return Ok(GreenSourceHit {
                        owner: match entry.trailing_gap_owner {
                            GreenGapOwner::Item => entry.item,
                            GreenGapOwner::List => *path
                                .last()
                                .ok_or(GreenTreeError::Corrupt("gap has no list owner"))?,
                        },
                        part: GreenSourcePart::Gap,
                        enclosing: path,
                        receipt,
                    });
                }
                return Err(GreenTreeError::Corrupt("source offset escaped green leaf"));
            }
            SequenceNodeKind::Branch { left, right } => {
                let left_metric = sequence_node::<GreenSequenceSpec>(arena, left)?.0.metric;
                let left_length = coordinate(left_metric, utf16);
                if offset < left_length {
                    node = left;
                } else {
                    offset -= left_length;
                    node = right;
                }
            }
        }
    }
}

const fn coordinate(metric: GreenMetric, utf16: bool) -> u64 {
    if utf16 { metric.utf16 } else { metric.bytes }
}

fn encode_metric_u32(metric: GreenMetric, output: &mut Vec<u8>) -> Result<(), GreenTreeError> {
    push_u32(
        output,
        u32::try_from(metric.bytes).map_err(|_| GreenTreeError::Overflow("local source bytes"))?,
    );
    push_u32(
        output,
        u32::try_from(metric.utf16).map_err(|_| GreenTreeError::Overflow("local source UTF-16"))?,
    );
    Ok(())
}

const fn encode_closed(value: ClosedChildAggregate) -> u8 {
    (value.ends_blank as u8)
        | ((value.item_loose_if_nonlast as u8) << 1)
        | ((value.item_loose_if_last as u8) << 2)
}

fn decode_closed(value: u8) -> Result<ClosedChildAggregate, GreenTreeError> {
    if value & !0b111 != 0 {
        return Err(GreenTreeError::Corrupt("invalid closed-child bits"));
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

fn decode_fold(value: u8) -> Result<ChildSequenceAggregate, GreenTreeError> {
    if value & !0b11_1111 != 0 {
        return Err(GreenTreeError::Corrupt("invalid child-fold bits"));
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

    fn take(&mut self, count: usize) -> Result<&'a [u8], GreenTreeError> {
        let (value, remaining) = self
            .remaining
            .split_at_checked(count)
            .ok_or(GreenTreeError::Corrupt("truncated green scalar"))?;
        self.remaining = remaining;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, GreenTreeError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, GreenTreeError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("two-byte green scalar"),
        ))
    }

    fn u32(&mut self) -> Result<u32, GreenTreeError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four-byte green scalar"),
        ))
    }

    fn u64(&mut self) -> Result<u64, GreenTreeError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("eight-byte green scalar"),
        ))
    }

    fn metric_u32(&mut self) -> Result<GreenMetric, GreenTreeError> {
        Ok(GreenMetric {
            bytes: u64::from(self.u32()?),
            utf16: u64::from(self.u32()?),
        })
    }
}

/// Exact arena fanout discriminator for packed nodes containing several
/// out-of-line child roots.
pub fn prove_three_external_children_are_unrepresentable(
    arena: &mut PageArena,
) -> Result<ArenaError, GreenTreeError> {
    let mut owners = Vec::new();
    for _ in 0..3 {
        owners.push(arena.allocate(&[0], &[])?.owner);
    }
    let ids = owners.iter().map(OwnedArenaRef::id).collect::<Vec<_>>();
    let error = arena
        .allocate(&[CONTAINER_TAG], &ids)
        .expect_err("arena has a two-child hard cap");
    for owner in owners {
        arena
            .release_later(owner)
            .map_err(legacy_owner_transfer_error)?;
    }
    Ok(error)
}

//! Grammar-free persistent finalized-record output challenger.
//!
//! Parser mutations are deliberately absent from the persistent format. Open
//! state lives in a bounded overlay; finalized blocks, preorder, total source
//! coverage, and bounded presentation facts live in independent packed
//! components owned by one arena manifest.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;

use crate::arena::{ArenaBuildTransaction, ArenaOwnerHandle};
use crate::persistent_sequence::{
    PersistentSequence, PersistentSequenceRef, SealedSequenceLeaf, SequenceMutationReceipt,
    SequenceNodeKind as CoreSequenceNodeKind, SequenceSpec, StreamingSequenceBuilder,
    sequence_node as persistent_sequence_node,
};
use crate::presentation::{
    PresentationEpoch, PresentationError, PresentationFactLease, PresentationLookup,
    PresentationRange, PresentationRequest, PresentationUnknownRange, PresentationUnknownReason,
    presentation_contract_root, query_presentation_root,
};
use crate::{ARENA_PAGE_BYTES, ArenaError, ArenaId, OwnedArenaRef, OwnerTransferError, PageArena};

const VERSION: u8 = 1;
const SEQUENCE_BRANCH_TAG: u8 = 0x61;
const RECORD_PAGE_TAG: u8 = 0x62;
const ORDER_PAGE_TAG: u8 = 0x63;
const COVERAGE_PAGE_TAG: u8 = 0x64;
const OVERLAY_FRAME_TAG: u8 = 0x65;
const COMPONENT_TAG: u8 = 0x67;
const COMPONENT_PAIR_TAG: u8 = 0x68;
const MANIFEST_TAG: u8 = 0x69;
const DIRECT_CHILD_PAGE_TAG: u8 = 0x6a;
const CONTAINER_FOLD_BINDING_TAG: u8 = 0x6b;

const SEQUENCE_BRANCH_BYTES: usize = 56;
const RECORD_HEADER_BYTES: usize = 4;
const RECORD_BYTES: usize = 84;
const ORDER_HEADER_BYTES: usize = 4;
const ORDER_ENTRY_BYTES: usize = 8;
const COVERAGE_HEADER_BYTES: usize = 36;
const COVERAGE_ENTRY_BYTES: usize = 48;
const OVERLAY_FRAME_BYTES: usize = 80;
const DIRECT_CHILD_HEADER_BYTES: usize = 12;
const DIRECT_CHILD_ENTRY_BYTES: usize = 16;
const CONTAINER_FOLD_BINDING_BYTES: usize = 28;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ForestBlockId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ForestCoverageId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ForestPropertyId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ForestRunCursorId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ForestAnchor {
    pub coverage: ForestCoverageId,
    pub local_bytes: u32,
    pub local_utf16: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockRecord {
    pub id: ForestBlockId,
    pub parent: Option<ForestBlockId>,
    pub kind_tag: u16,
    pub context: u64,
    pub property: Option<ForestPropertyId>,
    pub start: ForestAnchor,
    pub end: ForestAnchor,
    pub content: Option<ForestRunCursorId>,
    pub subtree_last: ForestBlockId,
    pub terminal: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CoverageSegmentKind {
    Terminal = 1,
    Gap = 2,
    ContainerMarker = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverageAffinity {
    Upstream,
    Downstream,
}

/// One half-open member of the total, non-overlapping source partition.
///
/// `owner` is either the terminal leaf or, for a gap/marker, the innermost
/// containing block. Enclosing containers are recovered through record parent
/// links instead of duplicating every overlapping block interval here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoverageSegment {
    pub owner: ForestBlockId,
    pub kind: CoverageSegmentKind,
    pub start: ForestAnchor,
    pub end: ForestAnchor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFrame {
    pub block: ForestBlockId,
    pub parent: Option<ForestBlockId>,
    pub kind_tag: u16,
    pub context: u64,
    pub start: ForestAnchor,
    pub current: ForestAnchor,
    pub pending: Option<ForestRunCursorId>,
}

/// Exact output-only contribution of one finalized direct child.
///
/// This is intentionally the same finite state consumed by `CommonMark` list
/// tightness. It is not parser continuation state and therefore cannot delay
/// structural convergence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClosedChildAggregate {
    pub ends_blank: bool,
    pub item_loose_if_nonlast: bool,
    pub item_loose_if_last: bool,
}

/// Associative summary of an ordered direct-child range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // Exact finite grammar-output state, not options.
pub struct ChildSequenceAggregate {
    pub had_child: bool,
    pub any_nonlast_child_ends_blank: bool,
    pub last_child_ends_blank: bool,
    pub list_loose_before_last: bool,
    pub last_item_loose_if_nonlast: bool,
    pub last_item_loose_if_last: bool,
}

impl ChildSequenceAggregate {
    #[must_use]
    pub const fn singleton(child: ClosedChildAggregate) -> Self {
        Self {
            had_child: true,
            any_nonlast_child_ends_blank: false,
            last_child_ends_blank: child.ends_blank,
            list_loose_before_last: false,
            last_item_loose_if_nonlast: child.item_loose_if_nonlast,
            last_item_loose_if_last: child.item_loose_if_last,
        }
    }

    #[must_use]
    pub const fn followed_by(self, suffix: Self) -> Self {
        if !self.had_child {
            return suffix;
        }
        if !suffix.had_child {
            return self;
        }
        Self {
            had_child: true,
            any_nonlast_child_ends_blank: self.any_nonlast_child_ends_blank
                || self.last_child_ends_blank
                || suffix.any_nonlast_child_ends_blank,
            last_child_ends_blank: suffix.last_child_ends_blank,
            list_loose_before_last: self.list_loose_before_last
                || self.last_item_loose_if_nonlast
                || suffix.list_loose_before_last,
            last_item_loose_if_nonlast: suffix.last_item_loose_if_nonlast,
            last_item_loose_if_last: suffix.last_item_loose_if_last,
        }
    }

    #[must_use]
    pub const fn list_is_tight(self) -> bool {
        !(self.list_loose_before_last || self.last_item_loose_if_last)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContainerFoldSemantics {
    /// Lists and items inherit their end-blank state through the last child.
    pub descends_through_last_child: bool,
    /// Only item blocks contribute the two list-looseness variants.
    pub is_item: bool,
    pub last_line_blank: bool,
}

impl ContainerFoldSemantics {
    #[must_use]
    pub const fn closed_summary(self, children: ChildSequenceAggregate) -> ClosedChildAggregate {
        let ends_blank = self.last_line_blank
            || (self.descends_through_last_child && children.last_child_ends_blank);
        let (item_loose_if_nonlast, item_loose_if_last) = if self.is_item {
            (
                self.last_line_blank
                    || children.any_nonlast_child_ends_blank
                    || children.last_child_ends_blank,
                children.any_nonlast_child_ends_blank,
            )
        } else {
            (false, false)
        };
        ClosedChildAggregate {
            ends_blank,
            item_loose_if_nonlast,
            item_loose_if_last,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectChildAggregate {
    pub child: ForestBlockId,
    pub summary: ClosedChildAggregate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerFoldInput {
    pub container: ForestBlockId,
    pub semantics: ContainerFoldSemantics,
    pub children: Vec<DirectChildAggregate>,
}

// `OpenFrame` is a grammar-free structural view, not a restart checkpoint.
// Exact typed parser continuation and convergence equality stay parser-private
// and are adopted only after the parser's independent checkpoint gate passes.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnknownRange {
    pub start: Option<ForestAnchor>,
    pub end: Option<ForestAnchor>,
}

pub trait CoverageOrderOracle {
    fn rank(&self, coverage: ForestCoverageId) -> Result<u64, RecordForestError>;

    fn compare(
        &self,
        left: ForestCoverageId,
        right: ForestCoverageId,
    ) -> Result<Ordering, RecordForestError> {
        Ok(self.rank(left)?.cmp(&self.rank(right)?))
    }
}

#[derive(Clone, Debug, Default)]
pub struct ExplicitCoverageOrder {
    ranks: BTreeMap<ForestCoverageId, u64>,
}

impl ExplicitCoverageOrder {
    pub fn from_ids(
        ids: impl IntoIterator<Item = ForestCoverageId>,
    ) -> Result<Self, RecordForestError> {
        let mut ranks = BTreeMap::new();
        for (rank, id) in ids.into_iter().enumerate() {
            if ranks
                .insert(
                    id,
                    u64::try_from(rank)
                        .map_err(|_| RecordForestError::Overflow("coverage rank"))?,
                )
                .is_some()
            {
                return Err(RecordForestError::Invalid("duplicate coverage ID"));
            }
        }
        Ok(Self { ranks })
    }
}

impl CoverageOrderOracle for ExplicitCoverageOrder {
    fn rank(&self, coverage: ForestCoverageId) -> Result<u64, RecordForestError> {
        self.ranks
            .get(&coverage)
            .copied()
            .ok_or(RecordForestError::Invalid("unknown coverage ID"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordForestError {
    Arena(ArenaError),
    Corrupt(&'static str),
    Invalid(&'static str),
    Overflow(&'static str),
    NotFound,
}

impl From<ArenaError> for RecordForestError {
    fn from(value: ArenaError) -> Self {
        Self::Arena(value)
    }
}

fn legacy_owner_transfer_error(failure: OwnerTransferError) -> RecordForestError {
    // Record forest is a retained representation challenger with a legacy
    // copyable error. Selected storage authority must preserve `failure.owner`.
    let OwnerTransferError { error, owner } = failure;
    drop(owner);
    RecordForestError::Arena(error)
}

impl fmt::Display for RecordForestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arena(error) => error.fmt(formatter),
            Self::Corrupt(message) => write!(formatter, "corrupt record forest: {message}"),
            Self::Invalid(message) => {
                write!(formatter, "invalid record forest operation: {message}")
            }
            Self::Overflow(field) => write!(formatter, "record forest {field} overflow"),
            Self::NotFound => formatter.write_str("record forest value was not found"),
        }
    }
}

impl std::error::Error for RecordForestError {}

const fn encode_closed_child(value: ClosedChildAggregate) -> u8 {
    (value.ends_blank as u8)
        | ((value.item_loose_if_nonlast as u8) << 1)
        | ((value.item_loose_if_last as u8) << 2)
}

const fn decode_closed_child(value: u8) -> Option<ClosedChildAggregate> {
    if value & !0b111 != 0 {
        return None;
    }
    Some(ClosedChildAggregate {
        ends_blank: value & 1 != 0,
        item_loose_if_nonlast: value & 2 != 0,
        item_loose_if_last: value & 4 != 0,
    })
}

const fn encode_child_sequence(value: ChildSequenceAggregate) -> u8 {
    (value.had_child as u8)
        | ((value.any_nonlast_child_ends_blank as u8) << 1)
        | ((value.last_child_ends_blank as u8) << 2)
        | ((value.list_loose_before_last as u8) << 3)
        | ((value.last_item_loose_if_nonlast as u8) << 4)
        | ((value.last_item_loose_if_last as u8) << 5)
}

const fn decode_child_sequence(value: u8) -> Option<ChildSequenceAggregate> {
    if value & !0b11_1111 != 0 {
        return None;
    }
    Some(ChildSequenceAggregate {
        had_child: value & 1 != 0,
        any_nonlast_child_ends_blank: value & 2 != 0,
        last_child_ends_blank: value & 4 != 0,
        list_loose_before_last: value & 8 != 0,
        last_item_loose_if_nonlast: value & 16 != 0,
        last_item_loose_if_last: value & 32 != 0,
    })
}

const fn encode_container_semantics(value: ContainerFoldSemantics) -> u8 {
    (value.descends_through_last_child as u8)
        | ((value.is_item as u8) << 1)
        | ((value.last_line_blank as u8) << 2)
}

const fn decode_container_semantics(value: u8) -> Option<ContainerFoldSemantics> {
    if value & !0b111 != 0 {
        return None;
    }
    Some(ContainerFoldSemantics {
        descends_through_last_child: value & 1 != 0,
        is_item: value & 2 != 0,
        last_line_blank: value & 4 != 0,
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordForestReceipt {
    pub payload_bytes_copied: usize,
    pub leaf_pages_allocated: usize,
    pub branch_nodes_allocated: usize,
    pub nodes_visited: usize,
    pub pages_reused: usize,
    pub records_rewritten: usize,
    pub overlay_nodes_allocated: usize,
    pub overlay_nodes_visited: usize,
    pub child_references_added: usize,
    pub maximum_temporary_bytes: usize,
    pub maximum_streaming_sequence_roots: usize,
    pub maximum_streaming_sequence_bin_slots: usize,
    pub maximum_streaming_sequence_bin_bytes: usize,
    pub maximum_transaction_live_owners: usize,
    pub maximum_transaction_journal_slots: usize,
    pub maximum_transaction_journal_capacity: usize,
    pub maximum_transaction_journal_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SequenceKind {
    Records = 1,
    Order = 2,
    Coverage = 3,
    DirectChildren = 4,
    ContainerFolds = 5,
}

impl SequenceKind {
    fn from_u8(value: u8) -> Result<Self, RecordForestError> {
        match value {
            1 => Ok(Self::Records),
            2 => Ok(Self::Order),
            3 => Ok(Self::Coverage),
            4 => Ok(Self::DirectChildren),
            5 => Ok(Self::ContainerFolds),
            _ => Err(RecordForestError::Corrupt("unknown sequence kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SequenceSummary {
    leaves: u64,
    items: u64,
    height: u16,
    first_key: u128,
    last_key: u128,
}

#[derive(Debug)]
struct SealedForestPage {
    owner: OwnedArenaRef,
}

#[allow(clippy::too_many_lines)] // One centralized validator for every forest leaf codec.
fn leaf_summary(payload: &[u8]) -> Result<(SequenceKind, SequenceSummary), RecordForestError> {
    let mut decoder = Decoder::new(payload);
    let tag = decoder.u8()?;
    if decoder.u8()? != VERSION {
        return Err(RecordForestError::Corrupt("wrong forest leaf version"));
    }
    let count = u64::from(decoder.u16()?);
    let (kind, first_key, last_key, expected) = match tag {
        RECORD_PAGE_TAG => {
            let expected = RECORD_HEADER_BYTES
                .checked_add(
                    usize::try_from(count)
                        .map_err(|_| RecordForestError::Overflow("record count"))?
                        .checked_mul(RECORD_BYTES)
                        .ok_or(RecordForestError::Overflow("record bytes"))?,
                )
                .ok_or(RecordForestError::Overflow("record page"))?;
            let first = if count == 0 {
                0
            } else {
                u128::from(u64_at(payload, RECORD_HEADER_BYTES)?)
            };
            let last = if count == 0 {
                0
            } else {
                let index = usize::try_from(count - 1)
                    .map_err(|_| RecordForestError::Overflow("record index"))?;
                u128::from(u64_at(payload, RECORD_HEADER_BYTES + index * RECORD_BYTES)?)
            };
            (SequenceKind::Records, first, last, expected)
        }
        ORDER_PAGE_TAG => {
            let expected = ORDER_HEADER_BYTES
                .checked_add(
                    usize::try_from(count)
                        .map_err(|_| RecordForestError::Overflow("order count"))?
                        .checked_mul(ORDER_ENTRY_BYTES)
                        .ok_or(RecordForestError::Overflow("order bytes"))?,
                )
                .ok_or(RecordForestError::Overflow("order page"))?;
            (SequenceKind::Order, 0, 0, expected)
        }
        COVERAGE_PAGE_TAG => {
            let expected = COVERAGE_HEADER_BYTES
                .checked_add(
                    usize::try_from(count)
                        .map_err(|_| RecordForestError::Overflow("coverage count"))?
                        .checked_mul(COVERAGE_ENTRY_BYTES)
                        .ok_or(RecordForestError::Overflow("coverage bytes"))?,
                )
                .ok_or(RecordForestError::Overflow("coverage page"))?;
            let first = u128_at(payload, 4)?;
            let last = u128_at(payload, 20)?;
            (SequenceKind::Coverage, first, last, expected)
        }
        DIRECT_CHILD_PAGE_TAG => {
            let expected = DIRECT_CHILD_HEADER_BYTES
                .checked_add(
                    usize::try_from(count)
                        .map_err(|_| RecordForestError::Overflow("direct child count"))?
                        .checked_mul(DIRECT_CHILD_ENTRY_BYTES)
                        .ok_or(RecordForestError::Overflow("direct child bytes"))?,
                )
                .ok_or(RecordForestError::Overflow("direct child page"))?;
            if payload.len() < DIRECT_CHILD_HEADER_BYTES {
                return Err(RecordForestError::Corrupt("short direct child page"));
            }
            let aggregate = payload[4];
            if payload[5..DIRECT_CHILD_HEADER_BYTES] != [0; 7]
                || decode_child_sequence(aggregate).is_none()
            {
                return Err(RecordForestError::Corrupt(
                    "invalid direct child page summary",
                ));
            }
            (
                SequenceKind::DirectChildren,
                u128::from(aggregate),
                0,
                expected,
            )
        }
        CONTAINER_FOLD_BINDING_TAG => {
            if payload.len() != CONTAINER_FOLD_BINDING_BYTES {
                return Err(RecordForestError::Corrupt(
                    "wrong container fold binding size",
                ));
            }
            let container = u64_at(payload, 4)?;
            if container == 0 || count != 1 {
                return Err(RecordForestError::Corrupt("invalid container fold binding"));
            }
            (
                SequenceKind::ContainerFolds,
                u128::from(container),
                u128::from(container),
                CONTAINER_FOLD_BINDING_BYTES,
            )
        }
        _ => return Err(RecordForestError::Corrupt("unknown forest leaf tag")),
    };
    if count == 0 || payload.len() != expected {
        return Err(RecordForestError::Corrupt("invalid forest leaf size"));
    }
    Ok((
        kind,
        SequenceSummary {
            leaves: 1,
            items: count,
            height: 1,
            first_key,
            last_key,
        },
    ))
}

fn decode_branch(payload: &[u8]) -> Result<(SequenceKind, SequenceSummary), RecordForestError> {
    if payload.len() != SEQUENCE_BRANCH_BYTES {
        return Err(RecordForestError::Corrupt("wrong sequence branch size"));
    }
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != SEQUENCE_BRANCH_TAG || decoder.u8()? != VERSION {
        return Err(RecordForestError::Corrupt("wrong sequence branch header"));
    }
    let kind = SequenceKind::from_u8(decoder.u8()?)?;
    if decoder.u8()? != 0 {
        return Err(RecordForestError::Corrupt("sequence branch padding"));
    }
    let summary = SequenceSummary {
        height: decoder.u16()?,
        leaves: decoder.u64()?,
        items: decoder.u64()?,
        first_key: decoder.u128()?,
        last_key: decoder.u128()?,
    };
    if decoder.u16()? != 0 || !decoder.is_empty() || summary.leaves < 2 || summary.height < 2 {
        return Err(RecordForestError::Corrupt(
            "invalid sequence branch summary",
        ));
    }
    Ok((kind, summary))
}

fn encode_branch(kind: SequenceKind, summary: SequenceSummary) -> [u8; SEQUENCE_BRANCH_BYTES] {
    let mut payload = Vec::with_capacity(SEQUENCE_BRANCH_BYTES);
    payload.push(SEQUENCE_BRANCH_TAG);
    payload.push(VERSION);
    payload.push(kind as u8);
    payload.push(0);
    push_u16(&mut payload, summary.height);
    push_u64(&mut payload, summary.leaves);
    push_u64(&mut payload, summary.items);
    push_u128(&mut payload, summary.first_key);
    push_u128(&mut payload, summary.last_key);
    push_u16(&mut payload, 0);
    payload.try_into().expect("fixed sequence branch encoding")
}

fn combine_summary(
    kind: SequenceKind,
    left: SequenceSummary,
    right: SequenceSummary,
) -> Result<SequenceSummary, RecordForestError> {
    let (first_key, last_key) = if kind == SequenceKind::DirectChildren {
        let left = decode_child_sequence(
            u8::try_from(left.first_key)
                .map_err(|_| RecordForestError::Corrupt("direct child fold overflow"))?,
        )
        .ok_or(RecordForestError::Corrupt("invalid left direct child fold"))?;
        let right = decode_child_sequence(
            u8::try_from(right.first_key)
                .map_err(|_| RecordForestError::Corrupt("direct child fold overflow"))?,
        )
        .ok_or(RecordForestError::Corrupt(
            "invalid right direct child fold",
        ))?;
        (
            u128::from(encode_child_sequence(left.followed_by(right))),
            0,
        )
    } else {
        (left.first_key, right.last_key)
    };
    Ok(SequenceSummary {
        leaves: left
            .leaves
            .checked_add(right.leaves)
            .ok_or(RecordForestError::Overflow("sequence leaves"))?,
        items: left
            .items
            .checked_add(right.items)
            .ok_or(RecordForestError::Overflow("sequence items"))?,
        height: left
            .height
            .max(right.height)
            .checked_add(1)
            .ok_or(RecordForestError::Overflow("sequence height"))?,
        first_key,
        last_key,
    })
}

#[derive(Debug)]
struct ForestSequenceSpec;

impl SequenceSpec for ForestSequenceSpec {
    type Summary = (SequenceKind, SequenceSummary);
    type Error = RecordForestError;
    type BranchPayload = [u8; SEQUENCE_BRANCH_BYTES];

    fn leaf_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        match payload.first().copied() {
            Some(
                RECORD_PAGE_TAG
                | ORDER_PAGE_TAG
                | COVERAGE_PAGE_TAG
                | DIRECT_CHILD_PAGE_TAG
                | CONTAINER_FOLD_BINDING_TAG,
            ) => leaf_summary(payload).map(Some),
            _ => Ok(None),
        }
    }

    fn branch_summary(payload: &[u8]) -> Result<Option<Self::Summary>, Self::Error> {
        if payload.first().copied() == Some(SEQUENCE_BRANCH_TAG) {
            decode_branch(payload).map(Some)
        } else {
            Ok(None)
        }
    }

    fn encode_branch((kind, summary): Self::Summary) -> Self::BranchPayload {
        encode_branch(kind, summary)
    }

    fn combine(
        (left_kind, left): Self::Summary,
        (right_kind, right): Self::Summary,
    ) -> Result<Self::Summary, Self::Error> {
        if left_kind != right_kind {
            return Err(RecordForestError::Invalid("mixed sequence kinds"));
        }
        Ok((left_kind, combine_summary(left_kind, left, right)?))
    }

    fn leaves((_, summary): Self::Summary) -> u64 {
        summary.leaves
    }

    fn height((_, summary): Self::Summary) -> u16 {
        summary.height
    }

    fn invalid(message: &'static str) -> Self::Error {
        RecordForestError::Corrupt(message)
    }
}

fn sequence_node(
    arena: &PageArena,
    id: ArenaId,
) -> Result<(SequenceKind, SequenceSummary, CoreSequenceNodeKind), RecordForestError> {
    let ((kind, summary), node_kind) = persistent_sequence_node::<ForestSequenceSpec>(arena, id)?;
    Ok((kind, summary, node_kind))
}

#[derive(Debug, Default)]
struct ForestSequence {
    inner: PersistentSequence<ForestSequenceSpec>,
}

impl ForestSequence {
    fn from_pages(
        arena: &mut PageArena,
        pages: Vec<SealedForestPage>,
        receipt: &mut RecordForestReceipt,
    ) -> Result<Self, RecordForestError> {
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let inner = PersistentSequence::from_leaves(
            arena,
            pages
                .into_iter()
                .map(|page| SealedSequenceLeaf::new(page.owner))
                .collect(),
            &mut sequence_receipt,
        )?;
        merge_sequence_receipt(receipt, sequence_receipt);
        Ok(Self { inner })
    }

    fn as_ref(&self) -> ForestSequenceRef {
        ForestSequenceRef {
            root: self.inner.root_id(),
        }
    }

    fn splice_leaves(
        &self,
        arena: &mut PageArena,
        range: Range<u64>,
        replacements: Vec<SealedForestPage>,
        receipt: &mut RecordForestReceipt,
    ) -> Result<Self, RecordForestError> {
        let mut sequence_receipt = SequenceMutationReceipt::default();
        let inner = self.inner.splice_leaves(
            arena,
            range,
            replacements
                .into_iter()
                .map(|page| SealedSequenceLeaf::new(page.owner))
                .collect(),
            &mut sequence_receipt,
        )?;
        merge_sequence_receipt(receipt, sequence_receipt);
        Ok(Self { inner })
    }

    fn into_owner(self) -> Option<OwnedArenaRef> {
        self.inner.into_owner()
    }

    fn release_later(self, arena: &mut PageArena) -> Result<(), RecordForestError> {
        self.inner.release_later(arena)
    }
}

fn merge_sequence_receipt(receipt: &mut RecordForestReceipt, sequence: SequenceMutationReceipt) {
    receipt.leaf_pages_allocated += sequence.leaves_adopted;
    receipt.branch_nodes_allocated += sequence.branches_allocated;
    receipt.payload_bytes_copied += sequence.branch_payload_bytes_copied;
    receipt.nodes_visited += sequence.nodes_visited;
    receipt.child_references_added += sequence.child_references_added;
    receipt.pages_reused += sequence.leaves_reused;
    receipt.maximum_streaming_sequence_roots = receipt
        .maximum_streaming_sequence_roots
        .max(sequence.maximum_streaming_roots);
    receipt.maximum_streaming_sequence_bin_slots = receipt
        .maximum_streaming_sequence_bin_slots
        .max(sequence.maximum_streaming_bin_slots);
    receipt.maximum_streaming_sequence_bin_bytes = receipt
        .maximum_streaming_sequence_bin_bytes
        .max(sequence.maximum_streaming_bin_bytes);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ForestSequenceRef {
    root: Option<ArenaId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LeafLocation {
    page: ArenaId,
    leaf_index: u64,
    item_prefix: u64,
    local_item: u64,
    nodes_visited: usize,
}

impl ForestSequenceRef {
    const fn core(self) -> PersistentSequenceRef<ForestSequenceSpec> {
        PersistentSequenceRef::from_root(self.root)
    }

    fn summary(self, arena: &PageArena) -> Result<SequenceSummary, RecordForestError> {
        self.core()
            .summary(arena)
            .map(|summary| summary.map_or_else(SequenceSummary::default, |value| value.1))
    }

    fn locate_item(
        self,
        arena: &PageArena,
        item_index: u64,
    ) -> Result<Option<LeafLocation>, RecordForestError> {
        let Some(mut node) = self.root else {
            return Ok(None);
        };
        if item_index >= self.summary(arena)?.items {
            return Ok(None);
        }
        let mut index = item_index;
        let mut leaf_prefix = 0_u64;
        let mut item_prefix = 0_u64;
        let mut visited = 0_usize;
        loop {
            visited += 1;
            let (_, summary, kind) = sequence_node(arena, node)?;
            match kind {
                CoreSequenceNodeKind::Leaf => {
                    return Ok(Some(LeafLocation {
                        page: node,
                        leaf_index: leaf_prefix,
                        item_prefix,
                        local_item: index,
                        nodes_visited: visited,
                    }));
                }
                CoreSequenceNodeKind::Branch { left, right } => {
                    let left_summary = sequence_node(arena, left)?.1;
                    if index < left_summary.items {
                        node = left;
                    } else {
                        index -= left_summary.items;
                        leaf_prefix += left_summary.leaves;
                        item_prefix += left_summary.items;
                        node = right;
                    }
                }
            }
            if summary.items == 0 {
                return Err(RecordForestError::Corrupt(
                    "empty node in nonempty sequence",
                ));
            }
        }
    }

    fn locate_leaf(
        self,
        arena: &PageArena,
        leaf_index: u64,
    ) -> Result<Option<ArenaId>, RecordForestError> {
        self.core().locate_leaf(arena, leaf_index)
    }

    fn contains_node(self, arena: &PageArena, needle: ArenaId) -> Result<bool, RecordForestError> {
        self.core().contains_node(arena, needle)
    }

    fn right_partition_root(self, arena: &PageArena) -> Result<Option<ArenaId>, RecordForestError> {
        self.core().right_partition_root(arena)
    }
}

fn allocate_leaf_payloads(
    arena: &mut PageArena,
    payloads: Vec<Vec<u8>>,
    receipt: &mut RecordForestReceipt,
) -> Result<Vec<SealedForestPage>, RecordForestError> {
    let mut transaction = ArenaBuildTransaction::new(arena);
    let mut handles = Vec::<ArenaOwnerHandle>::with_capacity(payloads.len());
    for payload in payloads {
        receipt.maximum_temporary_bytes = receipt.maximum_temporary_bytes.max(payload.len());
        let (handle, allocation) = transaction.allocate(&payload, &[])?;
        receipt.payload_bytes_copied += allocation.payload_bytes_copied;
        handles.push(handle);
    }
    Ok(handles
        .into_iter()
        .map(|handle| SealedForestPage {
            owner: transaction.take(handle),
        })
        .collect())
}

fn encode_record_page(records: &[BlockRecord]) -> Result<Vec<u8>, RecordForestError> {
    if records.is_empty() || records.windows(2).any(|pair| pair[0].id >= pair[1].id) {
        return Err(RecordForestError::Invalid(
            "record page is empty or unordered",
        ));
    }
    let mut output = Vec::with_capacity(RECORD_HEADER_BYTES + records.len() * RECORD_BYTES);
    output.push(RECORD_PAGE_TAG);
    output.push(VERSION);
    push_u16(
        &mut output,
        u16::try_from(records.len()).map_err(|_| RecordForestError::Overflow("record count"))?,
    );
    for record in records {
        encode_record(*record, &mut output);
    }
    if output.len() > ARENA_PAGE_BYTES {
        return Err(RecordForestError::Invalid("record page exceeds arena page"));
    }
    Ok(output)
}

fn encode_record(record: BlockRecord, output: &mut Vec<u8>) {
    let start = output.len();
    push_u64(output, record.id.0);
    push_u64(output, record.parent.map_or(0, |value| value.0));
    push_u16(output, record.kind_tag);
    push_u16(output, u16::from(record.terminal));
    push_u64(output, record.context);
    push_u64(output, record.property.map_or(0, |value| value.0));
    encode_anchor(record.start, output);
    encode_anchor(record.end, output);
    push_u64(output, record.content.map_or(0, |value| value.0));
    push_u64(output, record.subtree_last.0);
    debug_assert_eq!(output.len() - start, RECORD_BYTES);
}

fn decode_record_page(payload: &[u8]) -> Result<Vec<BlockRecord>, RecordForestError> {
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != RECORD_PAGE_TAG || decoder.u8()? != VERSION {
        return Err(RecordForestError::Corrupt("wrong record page header"));
    }
    let count = usize::from(decoder.u16()?);
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        records.push(BlockRecord {
            id: ForestBlockId(decoder.u64()?),
            parent: nonzero(decoder.u64()?).map(ForestBlockId),
            kind_tag: decoder.u16()?,
            terminal: match decoder.u16()? {
                0 => false,
                1 => true,
                _ => return Err(RecordForestError::Corrupt("invalid terminal flag")),
            },
            context: decoder.u64()?,
            property: nonzero(decoder.u64()?).map(ForestPropertyId),
            start: decoder.anchor()?,
            end: decoder.anchor()?,
            content: nonzero(decoder.u64()?).map(ForestRunCursorId),
            subtree_last: ForestBlockId(decoder.u64()?),
        });
    }
    if !decoder.is_empty()
        || records.is_empty()
        || records.windows(2).any(|pair| pair[0].id >= pair[1].id)
    {
        return Err(RecordForestError::Corrupt("invalid record page"));
    }
    Ok(records)
}

#[derive(Debug, Default)]
pub struct BlockRecordTable {
    sequence: ForestSequence,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockRecordTableRef {
    root: Option<ArenaId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordLookupReceipt {
    pub nodes_visited: usize,
    pub records_examined: usize,
}

impl BlockRecordTable {
    pub fn from_records(
        arena: &mut PageArena,
        records: &[BlockRecord],
        receipt: &mut RecordForestReceipt,
    ) -> Result<Self, RecordForestError> {
        if records.is_empty() {
            return Ok(Self::default());
        }
        if records.windows(2).any(|pair| pair[0].id >= pair[1].id) {
            return Err(RecordForestError::Invalid(
                "records are not sorted by stable ID",
            ));
        }
        let capacity = (ARENA_PAGE_BYTES - RECORD_HEADER_BYTES) / RECORD_BYTES;
        let payloads = records
            .chunks(capacity)
            .map(encode_record_page)
            .collect::<Result<Vec<_>, RecordForestError>>()?;
        let pages = allocate_leaf_payloads(arena, payloads, receipt)?;
        Ok(Self {
            sequence: ForestSequence::from_pages(arena, pages, receipt)?,
        })
    }

    #[must_use]
    pub fn as_ref(&self) -> BlockRecordTableRef {
        BlockRecordTableRef {
            root: self.sequence.as_ref().root,
        }
    }

    pub fn upsert(
        &self,
        arena: &mut PageArena,
        record: BlockRecord,
        receipt: &mut RecordForestReceipt,
    ) -> Result<Self, RecordForestError> {
        if self.sequence.inner.root_id().is_none() {
            return Self::from_records(arena, &[record], receipt);
        }
        let (leaf_index, page, mut records) = self.as_ref().find_page(arena, record.id, receipt)?;
        match records.binary_search_by_key(&record.id, |value| value.id) {
            Ok(index) => {
                if records[index] != record {
                    records[index] = record;
                    receipt.records_rewritten += 1;
                }
            }
            Err(index) => records.insert(index, record),
        }
        let capacity = (ARENA_PAGE_BYTES - RECORD_HEADER_BYTES) / RECORD_BYTES;
        let payloads = records
            .chunks(capacity)
            .map(encode_record_page)
            .collect::<Result<Vec<_>, RecordForestError>>()?;
        let replacements = allocate_leaf_payloads(arena, payloads, receipt)?;
        let sequence = self.sequence.splice_leaves(
            arena,
            leaf_index..leaf_index + 1,
            replacements,
            receipt,
        )?;
        debug_assert!(arena.contains(page));
        Ok(Self { sequence })
    }

    pub fn remove(
        &self,
        arena: &mut PageArena,
        id: ForestBlockId,
        receipt: &mut RecordForestReceipt,
    ) -> Result<Self, RecordForestError> {
        let (leaf_index, _, mut records) = self.as_ref().find_page(arena, id, receipt)?;
        let index = records
            .binary_search_by_key(&id, |value| value.id)
            .map_err(|_| RecordForestError::NotFound)?;
        records.remove(index);
        receipt.records_rewritten += 1;
        let capacity = (ARENA_PAGE_BYTES - RECORD_HEADER_BYTES) / RECORD_BYTES;
        let payloads = records
            .chunks(capacity)
            .map(encode_record_page)
            .collect::<Result<Vec<_>, RecordForestError>>()?;
        let replacements = allocate_leaf_payloads(arena, payloads, receipt)?;
        let sequence = self.sequence.splice_leaves(
            arena,
            leaf_index..leaf_index + 1,
            replacements,
            receipt,
        )?;
        Ok(Self { sequence })
    }

    pub fn release_later(self, arena: &mut PageArena) -> Result<(), RecordForestError> {
        self.sequence.release_later(arena)
    }
}

impl BlockRecordTableRef {
    #[must_use]
    pub const fn from_root(root: Option<ArenaId>) -> Self {
        Self { root }
    }

    fn find_page(
        self,
        arena: &PageArena,
        id: ForestBlockId,
        receipt: &mut RecordForestReceipt,
    ) -> Result<(u64, ArenaId, Vec<BlockRecord>), RecordForestError> {
        let Some(mut node) = self.root else {
            return Err(RecordForestError::NotFound);
        };
        let mut leaf_index = 0_u64;
        loop {
            receipt.nodes_visited += 1;
            let (kind, _, node_kind) = sequence_node(arena, node)?;
            if kind != SequenceKind::Records {
                return Err(RecordForestError::Corrupt("record root has wrong kind"));
            }
            match node_kind {
                CoreSequenceNodeKind::Leaf => {
                    return Ok((leaf_index, node, decode_record_page(arena.payload(node)?)?));
                }
                CoreSequenceNodeKind::Branch { left, right } => {
                    let left_summary = sequence_node(arena, left)?.1;
                    if u128::from(id.0) <= left_summary.last_key {
                        node = left;
                    } else {
                        leaf_index += left_summary.leaves;
                        node = right;
                    }
                }
            }
        }
    }

    pub fn get(
        self,
        arena: &PageArena,
        id: ForestBlockId,
    ) -> Result<(Option<BlockRecord>, RecordLookupReceipt), RecordForestError> {
        let mut receipt = RecordForestReceipt::default();
        let (_, _, records) = match self.find_page(arena, id, &mut receipt) {
            Ok(value) => value,
            Err(RecordForestError::NotFound) => {
                return Ok((None, RecordLookupReceipt::default()));
            }
            Err(error) => return Err(error),
        };
        let result = records.binary_search_by_key(&id, |record| record.id).ok();
        Ok((
            result.map(|index| records[index]),
            RecordLookupReceipt {
                nodes_visited: receipt.nodes_visited,
                records_examined: records.len(),
            },
        ))
    }

    pub fn page_for(
        self,
        arena: &PageArena,
        id: ForestBlockId,
    ) -> Result<Option<ArenaId>, RecordForestError> {
        let mut receipt = RecordForestReceipt::default();
        match self.find_page(arena, id, &mut receipt) {
            Ok((_, page, _)) => Ok(Some(page)),
            Err(RecordForestError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

fn encode_order_page(entries: &[ForestBlockId]) -> Result<Vec<u8>, RecordForestError> {
    if entries.is_empty() {
        return Err(RecordForestError::Invalid("empty order page"));
    }
    let mut output = Vec::with_capacity(ORDER_HEADER_BYTES + entries.len() * ORDER_ENTRY_BYTES);
    output.push(ORDER_PAGE_TAG);
    output.push(VERSION);
    push_u16(
        &mut output,
        u16::try_from(entries.len()).map_err(|_| RecordForestError::Overflow("order entries"))?,
    );
    for entry in entries {
        push_u64(&mut output, entry.0);
    }
    if output.len() > ARENA_PAGE_BYTES {
        return Err(RecordForestError::Invalid("order page exceeds arena page"));
    }
    Ok(output)
}

fn decode_order_page(payload: &[u8]) -> Result<Vec<ForestBlockId>, RecordForestError> {
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != ORDER_PAGE_TAG || decoder.u8()? != VERSION {
        return Err(RecordForestError::Corrupt("wrong order page header"));
    }
    let count = usize::from(decoder.u16()?);
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        entries.push(ForestBlockId(decoder.u64()?));
    }
    if entries.is_empty() || !decoder.is_empty() {
        return Err(RecordForestError::Corrupt("invalid order page"));
    }
    Ok(entries)
}

fn encode_order_pages_forward(
    entries: &[ForestBlockId],
    capacity: usize,
) -> Result<Vec<Vec<u8>>, RecordForestError> {
    entries
        .chunks(capacity)
        .map(encode_order_page)
        .collect::<Result<Vec<_>, RecordForestError>>()
}

/// Packs from the right edge, leaving at most one partial page at the left.
/// This is the useful invariant for a stable prefix gap: repeated prepends fill
/// the same partial page instead of manufacturing one tiny page per edit.
fn encode_order_pages_reverse(
    entries: &[ForestBlockId],
    capacity: usize,
) -> Result<Vec<Vec<u8>>, RecordForestError> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let first_len = match entries.len() % capacity {
        0 => capacity,
        remainder => remainder,
    };
    let mut pages = vec![encode_order_page(&entries[..first_len])?];
    pages.extend(
        entries[first_len..]
            .chunks(capacity)
            .map(encode_order_page)
            .collect::<Result<Vec<_>, RecordForestError>>()?,
    );
    Ok(pages)
}

/// Splits an overflowing local leaf into evenly occupied leaves. This is the
/// conventional B+tree leaf invariant: no edit can create an unbounded trail
/// of one-entry pages even when the same interior gap is hit repeatedly.
fn encode_order_pages_balanced(
    entries: &[ForestBlockId],
    capacity: usize,
) -> Result<Vec<Vec<u8>>, RecordForestError> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let page_count = entries.len().div_ceil(capacity);
    let small = entries.len() / page_count;
    let large_pages = entries.len() % page_count;
    let mut pages = Vec::with_capacity(page_count);
    let mut start = 0;
    for index in 0..page_count {
        let len = small + usize::from(index < large_pages);
        pages.push(encode_order_page(&entries[start..start + len])?);
        start += len;
    }
    Ok(pages)
}

#[derive(Debug)]
pub struct BlockOrder {
    sequence: ForestSequence,
    page_capacity: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockOrderRef {
    root: Option<ArenaId>,
}

impl BlockOrder {
    pub fn from_entries(
        arena: &mut PageArena,
        entries: &[ForestBlockId],
        page_capacity: usize,
        receipt: &mut RecordForestReceipt,
    ) -> Result<Self, RecordForestError> {
        let maximum = (ARENA_PAGE_BYTES - ORDER_HEADER_BYTES) / ORDER_ENTRY_BYTES;
        if page_capacity == 0 || page_capacity > maximum {
            return Err(RecordForestError::Invalid("invalid order page capacity"));
        }
        let payloads = encode_order_pages_forward(entries, page_capacity)?;
        let pages = allocate_leaf_payloads(arena, payloads, receipt)?;
        Ok(Self {
            sequence: ForestSequence::from_pages(arena, pages, receipt)?,
            page_capacity,
        })
    }

    #[must_use]
    pub fn as_ref(&self) -> BlockOrderRef {
        BlockOrderRef {
            root: self.sequence.as_ref().root,
        }
    }

    #[allow(clippy::too_many_lines)] // Boundary/interior cases share one ownership transaction.
    pub fn splice(
        &self,
        arena: &mut PageArena,
        range: Range<u64>,
        replacement: &[ForestBlockId],
        receipt: &mut RecordForestReceipt,
    ) -> Result<Self, RecordForestError> {
        let summary = self.sequence.as_ref().summary(arena)?;
        if range.start > range.end || range.end > summary.items {
            return Err(RecordForestError::Invalid("order splice is out of range"));
        }
        if summary.items == 0 {
            return Self::from_entries(arena, replacement, self.page_capacity, receipt);
        }
        if range.is_empty() {
            let location = self
                .sequence
                .as_ref()
                .locate_item(arena, range.start.min(summary.items - 1))?
                .ok_or(RecordForestError::NotFound)?;
            let boundary = if range.start == 0 {
                Some(0)
            } else if range.start == summary.items {
                Some(summary.leaves)
            } else {
                (location.local_item == 0).then_some(location.leaf_index)
            };
            if let Some(boundary) = boundary {
                if replacement.is_empty() {
                    return Ok(Self {
                        sequence: self.sequence.splice_leaves(
                            arena,
                            boundary..boundary,
                            Vec::new(),
                            receipt,
                        )?,
                        page_capacity: self.page_capacity,
                    });
                }
                if boundary < summary.leaves {
                    let adjacent = self
                        .sequence
                        .as_ref()
                        .locate_leaf(arena, boundary)?
                        .ok_or(RecordForestError::NotFound)?;
                    let mut combined = replacement.to_vec();
                    combined.extend(decode_order_page(arena.payload(adjacent)?)?);
                    let payloads = encode_order_pages_reverse(&combined, self.page_capacity)?;
                    let pages = allocate_leaf_payloads(arena, payloads, receipt)?;
                    return Ok(Self {
                        sequence: self.sequence.splice_leaves(
                            arena,
                            boundary..boundary + 1,
                            pages,
                            receipt,
                        )?,
                        page_capacity: self.page_capacity,
                    });
                }
                let adjacent_index = boundary - 1;
                let adjacent = self
                    .sequence
                    .as_ref()
                    .locate_leaf(arena, adjacent_index)?
                    .ok_or(RecordForestError::NotFound)?;
                let mut combined = decode_order_page(arena.payload(adjacent)?)?;
                combined.extend_from_slice(replacement);
                let payloads = encode_order_pages_forward(&combined, self.page_capacity)?;
                let pages = allocate_leaf_payloads(arena, payloads, receipt)?;
                return Ok(Self {
                    sequence: self.sequence.splice_leaves(
                        arena,
                        adjacent_index..boundary,
                        pages,
                        receipt,
                    )?,
                    page_capacity: self.page_capacity,
                });
            }
            let first_page = location.leaf_index;
            let first_id = self
                .sequence
                .as_ref()
                .locate_leaf(arena, first_page)?
                .ok_or(RecordForestError::NotFound)?;
            let first_entries = decode_order_page(arena.payload(first_id)?)?;
            let insertion = if range.start == summary.items {
                first_entries.len()
            } else {
                usize::try_from(location.local_item)
                    .map_err(|_| RecordForestError::Overflow("order insertion"))?
            };
            let mut combined = Vec::new();
            combined.extend_from_slice(&first_entries[..insertion]);
            combined.extend_from_slice(replacement);
            combined.extend_from_slice(&first_entries[insertion..]);
            let payloads = encode_order_pages_balanced(&combined, self.page_capacity)?;
            let pages = allocate_leaf_payloads(arena, payloads, receipt)?;
            return Ok(Self {
                sequence: self.sequence.splice_leaves(
                    arena,
                    first_page..first_page + 1,
                    pages,
                    receipt,
                )?,
                page_capacity: self.page_capacity,
            });
        }
        let start_location = self
            .sequence
            .as_ref()
            .locate_item(arena, range.start)?
            .ok_or(RecordForestError::NotFound)?;
        let end_location = self
            .sequence
            .as_ref()
            .locate_item(arena, range.end - 1)?
            .ok_or(RecordForestError::NotFound)?;
        let first_page = start_location.leaf_index;
        let last_page = end_location.leaf_index;
        let first_id = self
            .sequence
            .as_ref()
            .locate_leaf(arena, first_page)?
            .ok_or(RecordForestError::NotFound)?;
        let last_id = self
            .sequence
            .as_ref()
            .locate_leaf(arena, last_page)?
            .ok_or(RecordForestError::NotFound)?;
        let first_entries = decode_order_page(arena.payload(first_id)?)?;
        let last_entries = if first_id == last_id {
            first_entries.clone()
        } else {
            decode_order_page(arena.payload(last_id)?)?
        };
        let start_local = usize::try_from(start_location.local_item)
            .map_err(|_| RecordForestError::Overflow("order local start"))?;
        let end_local = usize::try_from(end_location.local_item + 1)
            .map_err(|_| RecordForestError::Overflow("order local end"))?;
        let mut combined = Vec::new();
        combined.extend_from_slice(&first_entries[..start_local]);
        combined.extend_from_slice(replacement);
        combined.extend_from_slice(&last_entries[end_local..]);
        let payloads = encode_order_pages_balanced(&combined, self.page_capacity)?;
        let pages = allocate_leaf_payloads(arena, payloads, receipt)?;
        let sequence =
            self.sequence
                .splice_leaves(arena, first_page..last_page + 1, pages, receipt)?;
        Ok(Self {
            sequence,
            page_capacity: self.page_capacity,
        })
    }

    pub fn release_later(self, arena: &mut PageArena) -> Result<(), RecordForestError> {
        self.sequence.release_later(arena)
    }
}

impl BlockOrderRef {
    #[must_use]
    pub const fn root_id(self) -> Option<ArenaId> {
        self.root
    }

    pub fn page_count(self, arena: &PageArena) -> Result<u64, RecordForestError> {
        ForestSequenceRef { root: self.root }
            .summary(arena)
            .map(|summary| summary.leaves)
    }

    pub fn height(self, arena: &PageArena) -> Result<u16, RecordForestError> {
        ForestSequenceRef { root: self.root }
            .summary(arena)
            .map(|summary| summary.height)
    }

    pub fn len(self, arena: &PageArena) -> Result<u64, RecordForestError> {
        ForestSequenceRef { root: self.root }
            .summary(arena)
            .map(|summary| summary.items)
    }

    pub fn get(
        self,
        arena: &PageArena,
        index: u64,
    ) -> Result<(Option<ForestBlockId>, usize), RecordForestError> {
        let sequence = ForestSequenceRef { root: self.root };
        let Some(location) = sequence.locate_item(arena, index)? else {
            return Ok((None, 0));
        };
        let entries = decode_order_page(arena.payload(location.page)?)?;
        let local = usize::try_from(location.local_item)
            .map_err(|_| RecordForestError::Overflow("order local index"))?;
        Ok((entries.get(local).copied(), location.nodes_visited))
    }

    pub fn contains_node(self, arena: &PageArena, id: ArenaId) -> Result<bool, RecordForestError> {
        ForestSequenceRef { root: self.root }.contains_node(arena, id)
    }

    pub fn right_partition_root(
        self,
        arena: &PageArena,
    ) -> Result<Option<ArenaId>, RecordForestError> {
        ForestSequenceRef { root: self.root }.right_partition_root(arena)
    }

    pub fn page_at(
        self,
        arena: &PageArena,
        index: u64,
    ) -> Result<Option<ArenaId>, RecordForestError> {
        ForestSequenceRef { root: self.root }.locate_leaf(arena, index)
    }
}

fn fold_direct_children(entries: &[DirectChildAggregate]) -> ChildSequenceAggregate {
    entries
        .iter()
        .fold(ChildSequenceAggregate::default(), |fold, entry| {
            fold.followed_by(ChildSequenceAggregate::singleton(entry.summary))
        })
}

fn encode_direct_child_page(
    entries: &[DirectChildAggregate],
) -> Result<Vec<u8>, RecordForestError> {
    if entries.is_empty() || entries.iter().any(|entry| entry.child.0 == 0) {
        return Err(RecordForestError::Invalid("invalid direct child page"));
    }
    let mut output =
        Vec::with_capacity(DIRECT_CHILD_HEADER_BYTES + entries.len() * DIRECT_CHILD_ENTRY_BYTES);
    output.push(DIRECT_CHILD_PAGE_TAG);
    output.push(VERSION);
    push_u16(
        &mut output,
        u16::try_from(entries.len())
            .map_err(|_| RecordForestError::Overflow("direct child count"))?,
    );
    output.push(encode_child_sequence(fold_direct_children(entries)));
    output.extend_from_slice(&[0; 7]);
    for entry in entries {
        push_u64(&mut output, entry.child.0);
        output.push(encode_closed_child(entry.summary));
        output.extend_from_slice(&[0; 7]);
    }
    if output.len() > ARENA_PAGE_BYTES {
        return Err(RecordForestError::Invalid(
            "direct child page exceeds arena page",
        ));
    }
    Ok(output)
}

fn decode_direct_child_page(
    payload: &[u8],
) -> Result<Vec<DirectChildAggregate>, RecordForestError> {
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != DIRECT_CHILD_PAGE_TAG || decoder.u8()? != VERSION {
        return Err(RecordForestError::Corrupt("wrong direct child page header"));
    }
    let count = usize::from(decoder.u16()?);
    let encoded_fold = decoder.u8()?;
    if decoder.take(7)? != [0; 7] {
        return Err(RecordForestError::Corrupt("direct child header padding"));
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let child = ForestBlockId(decoder.u64()?);
        let summary = decode_closed_child(decoder.u8()?).ok_or(RecordForestError::Corrupt(
            "invalid direct child contribution",
        ))?;
        if child.0 == 0 || decoder.take(7)? != [0; 7] {
            return Err(RecordForestError::Corrupt("invalid direct child entry"));
        }
        entries.push(DirectChildAggregate { child, summary });
    }
    if entries.is_empty()
        || !decoder.is_empty()
        || encode_child_sequence(fold_direct_children(&entries)) != encoded_fold
    {
        return Err(RecordForestError::Corrupt(
            "invalid direct child page aggregate",
        ));
    }
    Ok(entries)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ContainerFoldBinding {
    container: ForestBlockId,
    semantics: ContainerFoldSemantics,
    fold: ChildSequenceAggregate,
    child_count: u64,
}

fn encode_container_fold_binding(
    binding: ContainerFoldBinding,
) -> Result<[u8; CONTAINER_FOLD_BINDING_BYTES], RecordForestError> {
    if binding.container.0 == 0 || binding.fold.had_child != (binding.child_count != 0) {
        return Err(RecordForestError::Invalid("invalid container fold binding"));
    }
    let mut output = Vec::with_capacity(CONTAINER_FOLD_BINDING_BYTES);
    output.push(CONTAINER_FOLD_BINDING_TAG);
    output.push(VERSION);
    push_u16(&mut output, 1);
    push_u64(&mut output, binding.container.0);
    output.push(encode_container_semantics(binding.semantics));
    output.push(encode_child_sequence(binding.fold));
    output.extend_from_slice(&[0; 6]);
    push_u64(&mut output, binding.child_count);
    Ok(output
        .try_into()
        .expect("fixed container fold binding encoding"))
}

fn decode_container_fold_binding(
    payload: &[u8],
) -> Result<ContainerFoldBinding, RecordForestError> {
    if payload.len() != CONTAINER_FOLD_BINDING_BYTES {
        return Err(RecordForestError::Corrupt(
            "wrong container fold binding size",
        ));
    }
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != CONTAINER_FOLD_BINDING_TAG
        || decoder.u8()? != VERSION
        || decoder.u16()? != 1
    {
        return Err(RecordForestError::Corrupt(
            "wrong container fold binding header",
        ));
    }
    let container = ForestBlockId(decoder.u64()?);
    let semantics = decode_container_semantics(decoder.u8()?).ok_or(RecordForestError::Corrupt(
        "invalid container fold semantics",
    ))?;
    let fold = decode_child_sequence(decoder.u8()?)
        .ok_or(RecordForestError::Corrupt("invalid container child fold"))?;
    if decoder.take(6)? != [0; 6] {
        return Err(RecordForestError::Corrupt("container fold binding padding"));
    }
    let child_count = decoder.u64()?;
    if container.0 == 0 || fold.had_child != (child_count != 0) || !decoder.is_empty() {
        return Err(RecordForestError::Corrupt("invalid container fold binding"));
    }
    Ok(ContainerFoldBinding {
        container,
        semantics,
        fold,
        child_count,
    })
}

fn direct_child_sequence_from_entries(
    arena: &mut PageArena,
    entries: &[DirectChildAggregate],
    receipt: &mut RecordForestReceipt,
) -> Result<ForestSequence, RecordForestError> {
    if entries.is_empty() {
        return Ok(ForestSequence::default());
    }
    let capacity = (ARENA_PAGE_BYTES - DIRECT_CHILD_HEADER_BYTES) / DIRECT_CHILD_ENTRY_BYTES;
    let payloads = entries
        .chunks(capacity)
        .map(encode_direct_child_page)
        .collect::<Result<Vec<_>, RecordForestError>>()?;
    let pages = allocate_leaf_payloads(arena, payloads, receipt)?;
    ForestSequence::from_pages(arena, pages, receipt)
}

fn seal_container_fold_binding(
    arena: &mut PageArena,
    binding: ContainerFoldBinding,
    children: ForestSequence,
    receipt: &mut RecordForestReceipt,
) -> Result<SealedForestPage, RecordForestError> {
    let payload = encode_container_fold_binding(binding)?;
    let mut transaction = ArenaBuildTransaction::new(arena);
    let child = children
        .into_owner()
        .map(|owner| transaction.track(owner))
        .transpose()?;
    let child_ids = child
        .as_ref()
        .map_or_else(Vec::new, |child| vec![transaction.id(child)]);
    let (owner, allocation) = transaction.allocate(&payload, &child_ids)?;
    if let Some(child) = child {
        transaction.release(child)?;
    }
    receipt.payload_bytes_copied += allocation.payload_bytes_copied;
    receipt.child_references_added += allocation.child_references_added;
    Ok(SealedForestPage {
        owner: transaction.take(owner),
    })
}

#[derive(Debug, Default)]
pub struct ContainerChildFoldIndex {
    sequence: ForestSequence,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContainerChildFoldIndexRef {
    root: Option<ArenaId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerFoldView {
    pub container: ForestBlockId,
    pub semantics: ContainerFoldSemantics,
    pub children: ChildSequenceAggregate,
    pub child_count: u64,
}

impl ContainerFoldView {
    #[must_use]
    pub const fn list_is_tight(self) -> bool {
        self.children.list_is_tight()
    }

    #[must_use]
    pub const fn closed_summary(self) -> ClosedChildAggregate {
        self.semantics.closed_summary(self.children)
    }
}

impl ContainerChildFoldIndex {
    pub fn from_containers(
        arena: &mut PageArena,
        containers: &[ContainerFoldInput],
        receipt: &mut RecordForestReceipt,
    ) -> Result<Self, RecordForestError> {
        if containers
            .windows(2)
            .any(|pair| pair[0].container >= pair[1].container)
            || containers.iter().any(|input| input.container.0 == 0)
        {
            return Err(RecordForestError::Invalid(
                "container folds are not sorted by stable ID",
            ));
        }
        let mut bindings = Vec::with_capacity(containers.len());
        for input in containers {
            let built = (|| {
                let children = direct_child_sequence_from_entries(arena, &input.children, receipt)?;
                let summary = children.as_ref().summary(arena)?;
                let fold =
                    decode_child_sequence(u8::try_from(summary.first_key).map_err(|_| {
                        RecordForestError::Corrupt("direct child fold summary overflow")
                    })?)
                    .unwrap_or_default();
                seal_container_fold_binding(
                    arena,
                    ContainerFoldBinding {
                        container: input.container,
                        semantics: input.semantics,
                        fold,
                        child_count: u64::try_from(input.children.len())
                            .map_err(|_| RecordForestError::Overflow("direct child count"))?,
                    },
                    children,
                    receipt,
                )
            })();
            match built {
                Ok(binding) => bindings.push(binding),
                Err(error) => {
                    for binding in bindings {
                        arena
                            .release_later(binding.owner)
                            .map_err(legacy_owner_transfer_error)?;
                    }
                    return Err(error);
                }
            }
        }
        Ok(Self {
            sequence: ForestSequence::from_pages(arena, bindings, receipt)?,
        })
    }

    #[must_use]
    pub fn as_ref(&self) -> ContainerChildFoldIndexRef {
        ContainerChildFoldIndexRef {
            root: self.sequence.as_ref().root,
        }
    }

    /// Replaces one exact direct-child contribution. Only one child page, one
    /// container binding, and logarithmic paths in the two persistent trees
    /// are copied. Call this once per ancestor whose derived summary changes.
    pub fn replace_child(
        &self,
        arena: &mut PageArena,
        container: ForestBlockId,
        child_index: u64,
        expected_child: ForestBlockId,
        summary: ClosedChildAggregate,
        receipt: &mut RecordForestReceipt,
    ) -> Result<Self, RecordForestError> {
        let (binding_index, binding_page, binding, child_root, visited) =
            self.as_ref().find_binding(arena, container)?;
        receipt.nodes_visited += visited;
        if child_index >= binding.child_count {
            return Err(RecordForestError::Invalid(
                "direct child replacement is out of range",
            ));
        }
        let child_root = child_root.ok_or(RecordForestError::Corrupt(
            "nonempty container binding has no child root",
        ))?;
        // Complete all caller/corruption preflight before minting the temporary
        // owner used by the actual persistent splice.
        let location = ForestSequenceRef {
            root: Some(child_root),
        }
        .locate_item(arena, child_index)?
        .ok_or(RecordForestError::NotFound)?;
        receipt.nodes_visited += location.nodes_visited;
        let mut entries = decode_direct_child_page(arena.payload(location.page)?)?;
        let local = usize::try_from(location.local_item)
            .map_err(|_| RecordForestError::Overflow("direct child local index"))?;
        if entries[local].child != expected_child {
            return Err(RecordForestError::Invalid(
                "direct child identity changed at replacement index",
            ));
        }
        entries[local].summary = summary;
        let pages =
            allocate_leaf_payloads(arena, vec![encode_direct_child_page(&entries)?], receipt)?;
        let borrowed_owner = arena.retain(child_root)?;
        let borrowed = ForestSequence {
            inner: PersistentSequence::from_owner(borrowed_owner),
        };
        let update = borrowed.splice_leaves(
            arena,
            location.leaf_index..location.leaf_index + 1,
            pages,
            receipt,
        );
        borrowed.release_later(arena)?;
        let updated_children = update?;
        let child_summary = updated_children.as_ref().summary(arena)?;
        let fold = decode_child_sequence(
            u8::try_from(child_summary.first_key)
                .map_err(|_| RecordForestError::Corrupt("direct child fold overflow"))?,
        )
        .ok_or(RecordForestError::Corrupt("invalid direct child fold"))?;
        let replacement = seal_container_fold_binding(
            arena,
            ContainerFoldBinding { fold, ..binding },
            updated_children,
            receipt,
        )?;
        let sequence = self.sequence.splice_leaves(
            arena,
            binding_index..binding_index + 1,
            vec![replacement],
            receipt,
        )?;
        debug_assert!(arena.contains(binding_page));
        Ok(Self { sequence })
    }

    pub fn release_later(self, arena: &mut PageArena) -> Result<(), RecordForestError> {
        self.sequence.release_later(arena)
    }
}

impl ContainerChildFoldIndexRef {
    fn find_binding(
        self,
        arena: &PageArena,
        container: ForestBlockId,
    ) -> Result<(u64, ArenaId, ContainerFoldBinding, Option<ArenaId>, usize), RecordForestError>
    {
        let Some(mut node) = self.root else {
            return Err(RecordForestError::NotFound);
        };
        let mut leaf_index = 0_u64;
        let mut visited = 0_usize;
        loop {
            visited += 1;
            let (kind, _, node_kind) = sequence_node(arena, node)?;
            if kind != SequenceKind::ContainerFolds {
                return Err(RecordForestError::Corrupt(
                    "container fold root has wrong kind",
                ));
            }
            match node_kind {
                CoreSequenceNodeKind::Leaf => {
                    let binding = decode_container_fold_binding(arena.payload(node)?)?;
                    if binding.container != container {
                        return Err(RecordForestError::NotFound);
                    }
                    let children = arena.children(node)?;
                    if children[1].is_some() || (binding.child_count == 0) != children[0].is_none()
                    {
                        return Err(RecordForestError::Corrupt(
                            "container fold child edge disagrees with binding",
                        ));
                    }
                    return Ok((leaf_index, node, binding, children[0], visited));
                }
                CoreSequenceNodeKind::Branch { left, right } => {
                    let left_summary = sequence_node(arena, left)?.1;
                    if u128::from(container.0) <= left_summary.last_key {
                        node = left;
                    } else {
                        leaf_index += left_summary.leaves;
                        node = right;
                    }
                }
            }
        }
    }

    pub fn get(
        self,
        arena: &PageArena,
        container: ForestBlockId,
    ) -> Result<(Option<ContainerFoldView>, usize), RecordForestError> {
        match self.find_binding(arena, container) {
            Ok((_, _, binding, _, visited)) => Ok((
                Some(ContainerFoldView {
                    container: binding.container,
                    semantics: binding.semantics,
                    children: binding.fold,
                    child_count: binding.child_count,
                }),
                visited,
            )),
            Err(RecordForestError::NotFound) => Ok((None, 0)),
            Err(error) => Err(error),
        }
    }

    pub fn direct_child_page_at(
        self,
        arena: &PageArena,
        container: ForestBlockId,
        page_index: u64,
    ) -> Result<Option<ArenaId>, RecordForestError> {
        let (_, _, _, child_root, _) = self.find_binding(arena, container)?;
        ForestSequenceRef { root: child_root }.locate_leaf(arena, page_index)
    }

    pub fn direct_child_page_count(
        self,
        arena: &PageArena,
        container: ForestBlockId,
    ) -> Result<u64, RecordForestError> {
        let (_, _, _, child_root, _) = self.find_binding(arena, container)?;
        ForestSequenceRef { root: child_root }
            .summary(arena)
            .map(|summary| summary.leaves)
    }

    pub fn contains_node(
        self,
        arena: &PageArena,
        needle: ArenaId,
    ) -> Result<bool, RecordForestError> {
        ForestSequenceRef { root: self.root }.contains_node(arena, needle)
    }
}

fn encode_coverage_page(entries: &[CoverageSegment]) -> Result<Vec<u8>, RecordForestError> {
    if entries.is_empty() {
        return Err(RecordForestError::Invalid("empty coverage page"));
    }
    let mut output =
        Vec::with_capacity(COVERAGE_HEADER_BYTES + entries.len() * COVERAGE_ENTRY_BYTES);
    output.push(COVERAGE_PAGE_TAG);
    output.push(VERSION);
    push_u16(
        &mut output,
        u16::try_from(entries.len())
            .map_err(|_| RecordForestError::Overflow("coverage entries"))?,
    );
    push_u128(&mut output, stable_anchor_key(entries[0].start));
    push_u128(
        &mut output,
        stable_anchor_key(entries.last().expect("nonempty coverage page").end),
    );
    for entry in entries {
        push_u64(&mut output, entry.owner.0);
        output.push(entry.kind as u8);
        output.extend_from_slice(&[0; 7]);
        encode_anchor(entry.start, &mut output);
        encode_anchor(entry.end, &mut output);
    }
    if output.len() > ARENA_PAGE_BYTES {
        return Err(RecordForestError::Invalid(
            "coverage page exceeds arena page",
        ));
    }
    Ok(output)
}

fn decode_coverage_page(payload: &[u8]) -> Result<Vec<CoverageSegment>, RecordForestError> {
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != COVERAGE_PAGE_TAG || decoder.u8()? != VERSION {
        return Err(RecordForestError::Corrupt("wrong coverage page header"));
    }
    let count = usize::from(decoder.u16()?);
    let _first = decoder.u128()?;
    let _last = decoder.u128()?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let owner = ForestBlockId(decoder.u64()?);
        let kind = match decoder.u8()? {
            1 => CoverageSegmentKind::Terminal,
            2 => CoverageSegmentKind::Gap,
            3 => CoverageSegmentKind::ContainerMarker,
            _ => return Err(RecordForestError::Corrupt("unknown coverage segment kind")),
        };
        if decoder.take(7)? != [0; 7] {
            return Err(RecordForestError::Corrupt("coverage segment padding"));
        }
        entries.push(CoverageSegment {
            owner,
            kind,
            start: decoder.anchor()?,
            end: decoder.anchor()?,
        });
    }
    if entries.is_empty() || !decoder.is_empty() {
        return Err(RecordForestError::Corrupt("invalid coverage page"));
    }
    Ok(entries)
}

fn encode_coverage_pages_reverse(
    entries: &[CoverageSegment],
    capacity: usize,
) -> Result<Vec<Vec<u8>>, RecordForestError> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let first_len = match entries.len() % capacity {
        0 => capacity,
        remainder => remainder,
    };
    let mut pages = vec![encode_coverage_page(&entries[..first_len])?];
    pages.extend(
        entries[first_len..]
            .chunks(capacity)
            .map(encode_coverage_page)
            .collect::<Result<Vec<_>, RecordForestError>>()?,
    );
    Ok(pages)
}

#[derive(Debug, Default)]
pub struct CoveragePartition {
    sequence: ForestSequence,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoveragePartitionRef {
    root: Option<ArenaId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoverageLookupReceipt {
    pub nodes_visited: usize,
    pub segments_examined: usize,
    pub parent_records_visited: usize,
    pub frontier_nodes_visited: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralBlock {
    Open(OpenFrame),
    Finalized(BlockRecord),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnclosingBlocks {
    pub segment: CoverageSegment,
    /// Innermost to outermost, beginning with `segment.owner`.
    pub blocks: Vec<StructuralBlock>,
    pub receipt: CoverageLookupReceipt,
}

impl CoveragePartition {
    /// Builds a total half-open partition. Adjacent segments must share an
    /// exact boundary, so a blank viewport can never fall between index facts.
    pub fn from_segments(
        arena: &mut PageArena,
        segments: &[CoverageSegment],
        document_start: ForestAnchor,
        document_end: ForestAnchor,
        order: &impl CoverageOrderOracle,
        receipt: &mut RecordForestReceipt,
    ) -> Result<Self, RecordForestError> {
        if segments.is_empty() {
            if document_start == document_end {
                return Ok(Self::default());
            }
            return Err(RecordForestError::Invalid(
                "nonempty document has no coverage partition",
            ));
        }
        if segments[0].start != document_start
            || segments
                .last()
                .is_none_or(|entry| entry.end != document_end)
        {
            return Err(RecordForestError::Invalid(
                "coverage partition does not span the document",
            ));
        }
        for (index, segment) in segments.iter().enumerate() {
            if segment.owner.0 == 0
                || compare_anchor(segment.start, segment.end, order)? != Ordering::Less
            {
                return Err(RecordForestError::Invalid("invalid coverage segment"));
            }
            if index > 0 && segments[index - 1].end != segment.start {
                return Err(RecordForestError::Invalid(
                    "coverage partition has a gap or overlap",
                ));
            }
        }
        let capacity = (ARENA_PAGE_BYTES - COVERAGE_HEADER_BYTES) / COVERAGE_ENTRY_BYTES;
        let payloads = segments
            .chunks(capacity)
            .map(encode_coverage_page)
            .collect::<Result<Vec<_>, RecordForestError>>()?;
        let pages = allocate_leaf_payloads(arena, payloads, receipt)?;
        Ok(Self {
            sequence: ForestSequence::from_pages(arena, pages, receipt)?,
        })
    }

    #[must_use]
    pub fn as_ref(&self) -> CoveragePartitionRef {
        CoveragePartitionRef {
            root: self.sequence.as_ref().root,
        }
    }

    pub fn release_later(self, arena: &mut PageArena) -> Result<(), RecordForestError> {
        self.sequence.release_later(arena)
    }

    /// Prefix form of the production page-local coverage splice. It exists in
    /// this gate to prove that branch summaries contain stable anchors rather
    /// than revision-relative ranks. Pages are packed from the stable suffix
    /// edge, leaving one partial prefix page that absorbs repeated prepends.
    pub fn prepend_segment(
        &self,
        arena: &mut PageArena,
        segment: CoverageSegment,
        order: &impl CoverageOrderOracle,
        receipt: &mut RecordForestReceipt,
    ) -> Result<Self, RecordForestError> {
        let summary = self.sequence.as_ref().summary(arena)?;
        if summary.leaves == 0 {
            return Err(RecordForestError::Invalid(
                "cannot prepend to empty coverage",
            ));
        }
        let mut combined = vec![segment];
        let first_page = self
            .sequence
            .as_ref()
            .locate_leaf(arena, 0)?
            .ok_or(RecordForestError::Corrupt("missing coverage prefix page"))?;
        combined.extend(decode_coverage_page(arena.payload(first_page)?)?);
        let first = combined[1];
        if segment.end != first.start
            || compare_anchor(segment.start, segment.end, order)? != Ordering::Less
        {
            return Err(RecordForestError::Invalid(
                "prepended coverage is not exactly adjacent",
            ));
        }
        let capacity = (ARENA_PAGE_BYTES - COVERAGE_HEADER_BYTES) / COVERAGE_ENTRY_BYTES;
        let payloads = encode_coverage_pages_reverse(&combined, capacity)?;
        let pages = allocate_leaf_payloads(arena, payloads, receipt)?;
        Ok(Self {
            sequence: self.sequence.splice_leaves(arena, 0..1, pages, receipt)?,
        })
    }
}

impl CoveragePartitionRef {
    #[must_use]
    pub const fn from_root(root: Option<ArenaId>) -> Self {
        Self { root }
    }

    pub fn page_count(self, arena: &PageArena) -> Result<u64, RecordForestError> {
        ForestSequenceRef { root: self.root }
            .summary(arena)
            .map(|summary| summary.leaves)
    }

    pub fn height(self, arena: &PageArena) -> Result<u16, RecordForestError> {
        ForestSequenceRef { root: self.root }
            .summary(arena)
            .map(|summary| summary.height)
    }

    /// Whole-tree identity witness for tests; never a query hot path.
    pub fn contains_node(self, arena: &PageArena, id: ArenaId) -> Result<bool, RecordForestError> {
        ForestSequenceRef { root: self.root }.contains_node(arena, id)
    }

    pub fn page_at(
        self,
        arena: &PageArena,
        index: u64,
    ) -> Result<Option<ArenaId>, RecordForestError> {
        ForestSequenceRef { root: self.root }.locate_leaf(arena, index)
    }

    pub fn lookup(
        self,
        arena: &PageArena,
        point: ForestAnchor,
        affinity: CoverageAffinity,
        order: &impl CoverageOrderOracle,
    ) -> Result<(Option<CoverageSegment>, CoverageLookupReceipt), RecordForestError> {
        let Some(mut node) = self.root else {
            return Ok((None, CoverageLookupReceipt::default()));
        };
        let root_summary = sequence_node(arena, node)?.1;
        let root_start = stable_anchor_from_key(root_summary.first_key);
        let root_end = stable_anchor_from_key(root_summary.last_key);
        let outside = match affinity {
            CoverageAffinity::Downstream => {
                compare_anchor(point, root_start, order)? == Ordering::Less
                    || compare_anchor(point, root_end, order)? != Ordering::Less
            }
            CoverageAffinity::Upstream => {
                compare_anchor(point, root_start, order)? != Ordering::Greater
                    || compare_anchor(point, root_end, order)? == Ordering::Greater
            }
        };
        if outside {
            return Ok((None, CoverageLookupReceipt::default()));
        }
        let mut receipt = CoverageLookupReceipt::default();
        loop {
            receipt.nodes_visited += 1;
            let (kind, _, node_kind) = sequence_node(arena, node)?;
            if kind != SequenceKind::Coverage {
                return Err(RecordForestError::Corrupt("coverage root has wrong kind"));
            }
            match node_kind {
                CoreSequenceNodeKind::Leaf => {
                    let entries = decode_coverage_page(arena.payload(node)?)?;
                    receipt.segments_examined += entries.len();
                    let mut found = None;
                    let target = anchor_key(point, order)?;
                    for entry in entries {
                        let start = anchor_key(entry.start, order)?;
                        let end = anchor_key(entry.end, order)?;
                        let contains = match affinity {
                            CoverageAffinity::Downstream => start <= target && target < end,
                            CoverageAffinity::Upstream => start < target && target <= end,
                        };
                        if contains {
                            found = Some(entry);
                            break;
                        }
                    }
                    return Ok((found, receipt));
                }
                CoreSequenceNodeKind::Branch { left, right } => {
                    let left_summary = sequence_node(arena, left)?.1;
                    let left_end = stable_anchor_from_key(left_summary.last_key);
                    let comparison = compare_anchor(point, left_end, order)?;
                    let choose_left = match affinity {
                        CoverageAffinity::Downstream => comparison == Ordering::Less,
                        CoverageAffinity::Upstream => comparison != Ordering::Greater,
                    };
                    if choose_left {
                        node = left;
                    } else {
                        node = right;
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)] // Query dependencies stay explicit in the prototype.
    pub fn enclosing_blocks(
        self,
        arena: &PageArena,
        point: ForestAnchor,
        affinity: CoverageAffinity,
        order: &impl CoverageOrderOracle,
        records: BlockRecordTableRef,
        frontier: Option<&OpenOverlaySnapshot>,
        fuel: usize,
    ) -> Result<Option<EnclosingBlocks>, RecordForestError> {
        let (segment, mut receipt) = self.lookup(arena, point, affinity, order)?;
        let Some(segment) = segment else {
            return Ok(None);
        };
        let mut open = BTreeMap::new();
        if let Some(frontier) = frontier {
            let (frames, visited) = frontier.frames(arena, fuel)?;
            receipt.frontier_nodes_visited = visited;
            for frame in frames {
                open.insert(frame.block, frame);
            }
        }
        let mut cursor = Some(segment.owner);
        let mut blocks = Vec::new();
        while let Some(block) = cursor {
            if blocks.len().saturating_add(receipt.frontier_nodes_visited) >= fuel {
                return Err(RecordForestError::Invalid(
                    "coverage parent traversal exhausted fuel",
                ));
            }
            if let Some(frame) = open.get(&block).copied() {
                cursor = frame.parent;
                blocks.push(StructuralBlock::Open(frame));
                continue;
            }
            let (record, lookup) = records.get(arena, block)?;
            receipt.nodes_visited += lookup.nodes_visited;
            receipt.parent_records_visited += 1;
            let record = record.ok_or(RecordForestError::Corrupt(
                "coverage owner is missing from record table",
            ))?;
            cursor = record.parent;
            blocks.push(StructuralBlock::Finalized(record));
        }
        Ok(Some(EnclosingBlocks {
            segment,
            blocks,
            receipt,
        }))
    }
}

#[derive(Debug, Default)]
pub struct OpenOverlay {
    owner: Option<OwnedArenaRef>,
    depth: u32,
}

#[derive(Debug)]
pub struct OpenOverlaySnapshot {
    owner: Option<OwnedArenaRef>,
    pub depth: u32,
    pub unknown: UnknownRange,
}

impl OpenOverlay {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            owner: None,
            depth: 0,
        }
    }

    #[must_use]
    pub const fn depth(&self) -> u32 {
        self.depth
    }

    #[must_use]
    pub fn root_id(&self) -> Option<ArenaId> {
        self.owner.as_ref().map(OwnedArenaRef::id)
    }

    pub fn push(
        &mut self,
        arena: &mut PageArena,
        frame: OpenFrame,
        receipt: &mut RecordForestReceipt,
    ) -> Result<(), RecordForestError> {
        if frame.parent != self.top(arena)?.map(|value| value.block) {
            return Err(RecordForestError::Invalid("open overlay parent mismatch"));
        }
        let next_depth = self
            .depth
            .checked_add(1)
            .ok_or(RecordForestError::Overflow("overlay depth"))?;
        let payload = encode_overlay_frame(frame, next_depth);
        let children = self
            .owner
            .as_ref()
            .map_or_else(Vec::new, |owner| vec![owner.id()]);
        let allocation = arena.allocate(&payload, &children)?;
        if let Some(old) = self.owner.replace(allocation.owner) {
            arena
                .release_later(old)
                .map_err(legacy_owner_transfer_error)?;
        }
        self.depth = next_depth;
        receipt.overlay_nodes_allocated += 1;
        receipt.payload_bytes_copied += allocation.receipt.payload_bytes_copied;
        receipt.child_references_added += allocation.receipt.child_references_added;
        Ok(())
    }

    pub fn update_top(
        &mut self,
        arena: &mut PageArena,
        update: impl FnOnce(&mut OpenFrame),
        receipt: &mut RecordForestReceipt,
    ) -> Result<(), RecordForestError> {
        let old_id = self
            .owner
            .as_ref()
            .map(OwnedArenaRef::id)
            .ok_or(RecordForestError::Invalid("empty open overlay"))?;
        let (mut frame, depth) = decode_overlay_frame(arena, old_id)?;
        update(&mut frame);
        let parent = arena.children(old_id)?[0];
        let children = parent.map_or_else(Vec::new, |id| vec![id]);
        let allocation = arena.allocate(&encode_overlay_frame(frame, depth), &children)?;
        let old = self
            .owner
            .replace(allocation.owner)
            .expect("preflight proved overlay owner exists");
        arena
            .release_later(old)
            .map_err(legacy_owner_transfer_error)?;
        receipt.overlay_nodes_allocated += 1;
        receipt.payload_bytes_copied += allocation.receipt.payload_bytes_copied;
        receipt.child_references_added += allocation.receipt.child_references_added;
        Ok(())
    }

    pub fn pop(&mut self, arena: &mut PageArena) -> Result<OpenFrame, RecordForestError> {
        let old_id = self
            .owner
            .as_ref()
            .map(OwnedArenaRef::id)
            .ok_or(RecordForestError::Invalid("empty open overlay"))?;
        let (frame, depth) = decode_overlay_frame(arena, old_id)?;
        if depth != self.depth {
            return Err(RecordForestError::Corrupt("overlay depth mismatch"));
        }
        let parent = arena.children(old_id)?[0];
        let next = parent.map(|id| arena.retain(id)).transpose()?;
        let old = std::mem::replace(&mut self.owner, next)
            .expect("preflight proved overlay owner exists");
        arena
            .release_later(old)
            .map_err(legacy_owner_transfer_error)?;
        self.depth -= 1;
        Ok(frame)
    }

    pub fn snapshot(
        &self,
        arena: &mut PageArena,
        unknown: UnknownRange,
    ) -> Result<OpenOverlaySnapshot, RecordForestError> {
        Ok(OpenOverlaySnapshot {
            owner: self
                .owner
                .as_ref()
                .map(|owner| arena.retain(owner.id()))
                .transpose()?,
            depth: self.depth,
            unknown,
        })
    }

    pub fn top(&self, arena: &PageArena) -> Result<Option<OpenFrame>, RecordForestError> {
        self.owner
            .as_ref()
            .map(|owner| decode_overlay_frame(arena, owner.id()).map(|value| value.0))
            .transpose()
    }

    pub fn frames(
        &self,
        arena: &PageArena,
        fuel: usize,
    ) -> Result<(Vec<OpenFrame>, usize), RecordForestError> {
        load_overlay(arena, self.root_id(), fuel)
    }

    pub fn release_later(mut self, arena: &mut PageArena) -> Result<(), RecordForestError> {
        if let Some(owner) = self.owner.take() {
            arena
                .release_later(owner)
                .map_err(legacy_owner_transfer_error)?;
        }
        Ok(())
    }
}

impl OpenOverlaySnapshot {
    #[must_use]
    pub fn root_id(&self) -> Option<ArenaId> {
        self.owner.as_ref().map(OwnedArenaRef::id)
    }

    pub fn frames(
        &self,
        arena: &PageArena,
        fuel: usize,
    ) -> Result<(Vec<OpenFrame>, usize), RecordForestError> {
        load_overlay(arena, self.root_id(), fuel)
    }

    pub fn release_later(mut self, arena: &mut PageArena) -> Result<(), RecordForestError> {
        if let Some(owner) = self.owner.take() {
            arena
                .release_later(owner)
                .map_err(legacy_owner_transfer_error)?;
        }
        Ok(())
    }

    fn into_owner(mut self) -> Option<OwnedArenaRef> {
        self.owner.take()
    }
}

fn encode_overlay_frame(frame: OpenFrame, depth: u32) -> [u8; OVERLAY_FRAME_BYTES] {
    let mut output = Vec::with_capacity(OVERLAY_FRAME_BYTES);
    output.push(OVERLAY_FRAME_TAG);
    output.push(VERSION);
    push_u16(&mut output, frame.kind_tag);
    push_u32(&mut output, depth);
    push_u64(&mut output, frame.block.0);
    push_u64(&mut output, frame.parent.map_or(0, |value| value.0));
    push_u64(&mut output, frame.context);
    encode_anchor(frame.start, &mut output);
    encode_anchor(frame.current, &mut output);
    push_u64(&mut output, frame.pending.map_or(0, |value| value.0));
    push_u64(&mut output, 0);
    output.try_into().expect("fixed overlay frame encoding")
}

fn decode_overlay_frame(
    arena: &PageArena,
    id: ArenaId,
) -> Result<(OpenFrame, u32), RecordForestError> {
    let payload = arena.payload(id)?;
    if payload.len() != OVERLAY_FRAME_BYTES {
        return Err(RecordForestError::Corrupt("wrong overlay frame size"));
    }
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != OVERLAY_FRAME_TAG || decoder.u8()? != VERSION {
        return Err(RecordForestError::Corrupt("wrong overlay frame header"));
    }
    let kind_tag = decoder.u16()?;
    let depth = decoder.u32()?;
    let frame = OpenFrame {
        block: ForestBlockId(decoder.u64()?),
        parent: nonzero(decoder.u64()?).map(ForestBlockId),
        kind_tag,
        context: decoder.u64()?,
        start: decoder.anchor()?,
        current: decoder.anchor()?,
        pending: nonzero(decoder.u64()?).map(ForestRunCursorId),
    };
    if decoder.u64()? != 0 || !decoder.is_empty() {
        return Err(RecordForestError::Corrupt("overlay padding"));
    }
    Ok((frame, depth))
}

fn load_overlay(
    arena: &PageArena,
    root: Option<ArenaId>,
    fuel: usize,
) -> Result<(Vec<OpenFrame>, usize), RecordForestError> {
    let mut frames = Vec::new();
    let mut cursor = root;
    let mut visited = 0_usize;
    while let Some(id) = cursor {
        if visited == fuel {
            return Err(RecordForestError::Invalid(
                "overlay traversal exhausted fuel",
            ));
        }
        let (frame, _) = decode_overlay_frame(arena, id)?;
        frames.push(frame);
        visited += 1;
        cursor = arena.children(id)?[0];
    }
    frames.reverse();
    Ok((frames, visited))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComponentRole {
    Records = 1,
    Order = 2,
    Coverage = 3,
    Frontier = 4,
    Presentation = 5,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateSequenceRole {
    Records,
    Order,
    Coverage,
}

impl CandidateSequenceRole {
    const fn component(self) -> ComponentRole {
        match self {
            Self::Records => ComponentRole::Records,
            Self::Order => ComponentRole::Order,
            Self::Coverage => ComponentRole::Coverage,
        }
    }

    const fn bit(self) -> u8 {
        1 << (self.component() as u8 - 1)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CandidateTemporaryBounds {
    pub live_owner_handles: usize,
    pub maximum_live_owner_handles: usize,
    pub owner_journal_slots: usize,
    pub owner_journal_capacity: usize,
    pub owner_journal_bytes: usize,
}

/// One rollback boundary from the first streamed component leaf through the
/// final composite manifest allocation.
///
/// This builder is deliberately representation-neutral within each sequence:
/// callers provide already encoded forest leaves lazily. It therefore proves
/// the candidate lifetime and temporary-owner bound without selecting plain
/// preorder, Euler-tour order, or a direct-child side index.
#[derive(Debug)]
pub struct RecordForestCandidateTransaction<'a> {
    transaction: ArenaBuildTransaction<'a>,
    epoch: PresentationEpoch,
    structural_range: PresentationRange,
    unknown: UnknownRange,
    components: Vec<(ComponentRole, ArenaOwnerHandle)>,
    seen_roles: u8,
}

impl<'a> RecordForestCandidateTransaction<'a> {
    pub fn new(
        arena: &'a mut PageArena,
        epoch: PresentationEpoch,
        structural_range: PresentationRange,
        unknown: UnknownRange,
    ) -> Self {
        Self {
            transaction: ArenaBuildTransaction::new(arena),
            epoch,
            structural_range,
            unknown,
            components: Vec::new(),
            seen_roles: 0,
        }
    }

    #[must_use]
    pub fn temporary_bounds(&self) -> CandidateTemporaryBounds {
        CandidateTemporaryBounds {
            live_owner_handles: self.transaction.live_owners(),
            maximum_live_owner_handles: self.transaction.maximum_live_owners(),
            owner_journal_slots: self.transaction.owner_journal_slots(),
            owner_journal_capacity: self.transaction.owner_journal_capacity(),
            owner_journal_bytes: self.transaction.owner_journal_bytes(),
        }
    }

    /// Streams encoded leaf payloads directly into the shared binomial-carry
    /// sequence builder. `payloads` is never collected by this API.
    pub fn stream_sequence_component(
        mut self,
        role: CandidateSequenceRole,
        payloads: impl IntoIterator<Item = Vec<u8>>,
        receipt: &mut RecordForestReceipt,
    ) -> Result<Self, RecordForestError> {
        if self.seen_roles & role.bit() != 0 {
            return Err(RecordForestError::Invalid(
                "candidate sequence role was added twice",
            ));
        }
        self.seen_roles |= role.bit();
        let mut builder = StreamingSequenceBuilder::<ForestSequenceSpec>::default();
        let mut sequence_receipt = SequenceMutationReceipt::default();
        for payload in payloads {
            receipt.maximum_temporary_bytes = receipt.maximum_temporary_bytes.max(payload.len());
            let (leaf, allocation) = self.transaction.allocate(&payload, &[])?;
            receipt.payload_bytes_copied += allocation.payload_bytes_copied;
            builder.push_handle(&mut self.transaction, leaf, &mut sequence_receipt)?;
        }
        if let Some(root) = builder.finish(&mut self.transaction, &mut sequence_receipt)? {
            self.components.push((role.component(), root));
        }
        merge_sequence_receipt(receipt, sequence_receipt);
        self.sync_receipt(receipt);
        Ok(self)
    }

    /// Adds the bounded open-frame spine without allocating it outside the
    /// candidate rollback boundary.
    pub fn with_open_overlay(
        mut self,
        frames: impl IntoIterator<Item = OpenFrame>,
        receipt: &mut RecordForestReceipt,
    ) -> Result<Self, RecordForestError> {
        let bit = 1 << (ComponentRole::Frontier as u8 - 1);
        if self.seen_roles & bit != 0 {
            return Err(RecordForestError::Invalid(
                "candidate frontier was added twice",
            ));
        }
        self.seen_roles |= bit;
        let mut root = None;
        for (index, frame) in frames.into_iter().enumerate() {
            let depth = u32::try_from(index + 1)
                .map_err(|_| RecordForestError::Overflow("overlay depth"))?;
            let children = root
                .as_ref()
                .map_or_else(Vec::new, |root| vec![self.transaction.id(root)]);
            let (next, allocation) = self
                .transaction
                .allocate(&encode_overlay_frame(frame, depth), &children)?;
            if let Some(root) = root {
                self.transaction.release(root)?;
            }
            root = Some(next);
            receipt.overlay_nodes_allocated += 1;
            receipt.payload_bytes_copied += allocation.payload_bytes_copied;
            receipt.child_references_added += allocation.child_references_added;
        }
        if let Some(root) = root {
            self.components.push((ComponentRole::Frontier, root));
        }
        self.sync_receipt(receipt);
        Ok(self)
    }

    pub fn commit(
        self,
        coverage_order: &impl CoverageOrderOracle,
        receipt: &mut RecordForestReceipt,
    ) -> Result<RecordForestManifest, RecordForestError> {
        self.commit_inner(coverage_order, receipt, None)
    }

    #[cfg(test)]
    fn commit_failing_after_allocation(
        self,
        coverage_order: &impl CoverageOrderOracle,
        receipt: &mut RecordForestReceipt,
        fail_after: usize,
    ) -> Result<RecordForestManifest, RecordForestError> {
        self.commit_inner(coverage_order, receipt, Some(fail_after))
    }

    fn commit_inner(
        mut self,
        coverage_order: &impl CoverageOrderOracle,
        receipt: &mut RecordForestReceipt,
        fail_after: Option<usize>,
    ) -> Result<RecordForestManifest, RecordForestError> {
        validate_composite_contract(
            self.transaction.arena(),
            self.epoch,
            self.structural_range,
            self.unknown,
            self.components.iter().find_map(|(role, owner)| {
                (*role == ComponentRole::Coverage).then(|| self.transaction.id(owner))
            }),
            None,
            coverage_order,
        )?;
        let mut allocation_index = 0_usize;
        let mut wrappers = Vec::with_capacity(self.components.len());
        for (role, component) in std::mem::take(&mut self.components) {
            let child = self.transaction.id(&component);
            let (wrapper, allocation) = self
                .transaction
                .allocate(&[COMPONENT_TAG, VERSION, role as u8, 0], &[child])?;
            allocation_index += 1;
            if fail_after == Some(allocation_index) {
                return Err(RecordForestError::Invalid(
                    "forced candidate commit allocation failure",
                ));
            }
            self.transaction.release(component)?;
            receipt.payload_bytes_copied += allocation.payload_bytes_copied;
            receipt.child_references_added += allocation.child_references_added;
            wrappers.push(wrapper);
        }
        let component_root = build_component_tree_with_fault(
            &mut self.transaction,
            wrappers,
            receipt,
            &mut allocation_index,
            fail_after,
        )?;
        let payload = encode_manifest(self.epoch, self.structural_range, self.unknown);
        let children = component_root
            .as_ref()
            .map_or_else(Vec::new, |root| vec![self.transaction.id(root)]);
        let (manifest, allocation) = self.transaction.allocate(&payload, &children)?;
        allocation_index += 1;
        if fail_after == Some(allocation_index) {
            return Err(RecordForestError::Invalid(
                "forced candidate commit allocation failure",
            ));
        }
        if let Some(root) = component_root {
            self.transaction.release(root)?;
        }
        receipt.payload_bytes_copied += allocation.payload_bytes_copied;
        receipt.child_references_added += allocation.child_references_added;
        self.sync_receipt(receipt);
        Ok(RecordForestManifest {
            owner: self.transaction.take(manifest),
        })
    }

    fn sync_receipt(&self, receipt: &mut RecordForestReceipt) {
        receipt.maximum_transaction_live_owners = receipt
            .maximum_transaction_live_owners
            .max(self.transaction.maximum_live_owners());
        receipt.maximum_transaction_journal_slots = receipt
            .maximum_transaction_journal_slots
            .max(self.transaction.owner_journal_slots());
        receipt.maximum_transaction_journal_capacity = receipt
            .maximum_transaction_journal_capacity
            .max(self.transaction.owner_journal_capacity());
        receipt.maximum_transaction_journal_bytes = receipt
            .maximum_transaction_journal_bytes
            .max(self.transaction.owner_journal_bytes());
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ForestComponents {
    pub records: Option<ArenaId>,
    pub order: Option<ArenaId>,
    pub coverage: Option<ArenaId>,
    pub frontier: Option<ArenaId>,
    pub presentation: Option<ArenaId>,
}

#[derive(Debug)]
pub struct RecordForestManifest {
    owner: OwnedArenaRef,
}

impl RecordForestManifest {
    #[allow(clippy::too_many_arguments)] // Composite authority is intentionally explicit.
    pub fn build(
        arena: &mut PageArena,
        epoch: PresentationEpoch,
        structural_range: PresentationRange,
        records: BlockRecordTable,
        order: BlockOrder,
        coverage: CoveragePartition,
        overlay: OpenOverlaySnapshot,
        presentation: Option<PresentationFactLease>,
        coverage_order: &impl CoverageOrderOracle,
        receipt: &mut RecordForestReceipt,
    ) -> Result<Self, RecordForestError> {
        let unknown = overlay.unknown;
        let components = [
            (ComponentRole::Records, records.sequence.into_owner()),
            (ComponentRole::Order, order.sequence.into_owner()),
            (ComponentRole::Coverage, coverage.sequence.into_owner()),
            (ComponentRole::Frontier, overlay.into_owner()),
            (
                ComponentRole::Presentation,
                presentation.map(PresentationFactLease::into_owner),
            ),
        ];
        let mut transaction = ArenaBuildTransaction::new(arena);
        let components = components
            .into_iter()
            .filter_map(|(role, owner)| {
                owner.map(|owner| transaction.track(owner).map(|owner| (role, owner)))
            })
            .collect::<Result<Vec<_>, ArenaError>>()?;
        validate_composite_contract(
            transaction.arena(),
            epoch,
            structural_range,
            unknown,
            components.iter().find_map(|(role, owner)| {
                (*role == ComponentRole::Coverage).then(|| transaction.id(owner))
            }),
            components.iter().find_map(|(role, owner)| {
                (*role == ComponentRole::Presentation).then(|| transaction.id(owner))
            }),
            coverage_order,
        )?;
        let mut owners = Vec::<ArenaOwnerHandle>::new();
        for (role, component) in components {
            let child = transaction.id(&component);
            let (wrapper, allocation) =
                transaction.allocate(&[COMPONENT_TAG, VERSION, role as u8, 0], &[child])?;
            transaction.release(component)?;
            receipt.payload_bytes_copied += allocation.payload_bytes_copied;
            receipt.child_references_added += allocation.child_references_added;
            owners.push(wrapper);
        }
        let component_root = build_component_tree(&mut transaction, owners, receipt)?;
        let payload = encode_manifest(epoch, structural_range, unknown);
        let children = component_root
            .as_ref()
            .map_or_else(Vec::new, |owner| vec![transaction.id(owner)]);
        let (manifest, allocation) = transaction.allocate(&payload, &children)?;
        if let Some(owner) = component_root {
            transaction.release(owner)?;
        }
        receipt.payload_bytes_copied += allocation.payload_bytes_copied;
        receipt.child_references_added += allocation.child_references_added;
        Ok(Self {
            owner: transaction.take(manifest),
        })
    }

    #[must_use]
    pub const fn root_id(&self) -> ArenaId {
        self.owner.id()
    }

    pub fn unknown(&self, arena: &PageArena) -> Result<UnknownRange, RecordForestError> {
        decode_manifest(arena.payload(self.owner.id())?).map(|view| view.unknown)
    }

    pub fn epoch(&self, arena: &PageArena) -> Result<PresentationEpoch, RecordForestError> {
        decode_manifest(arena.payload(self.owner.id())?).map(|view| view.epoch)
    }

    pub fn structural_range(
        &self,
        arena: &PageArena,
    ) -> Result<PresentationRange, RecordForestError> {
        decode_manifest(arena.payload(self.owner.id())?).map(|view| view.structural_range)
    }

    pub fn components(&self, arena: &PageArena) -> Result<ForestComponents, RecordForestError> {
        let mut result = ForestComponents::default();
        let mut stack = arena.children(self.owner.id())?[0]
            .into_iter()
            .collect::<Vec<_>>();
        while let Some(id) = stack.pop() {
            let payload = arena.payload(id)?;
            match payload.first().copied() {
                Some(COMPONENT_TAG) => {
                    if payload.len() != 4 || payload[1] != VERSION || payload[3] != 0 {
                        return Err(RecordForestError::Corrupt("invalid component wrapper"));
                    }
                    let component = arena.children(id)?[0]
                        .ok_or(RecordForestError::Corrupt("component has no child"))?;
                    match payload[2] {
                        1 => result.records = Some(component),
                        2 => result.order = Some(component),
                        3 => result.coverage = Some(component),
                        4 => result.frontier = Some(component),
                        5 => result.presentation = Some(component),
                        _ => return Err(RecordForestError::Corrupt("unknown component role")),
                    }
                }
                Some(COMPONENT_PAIR_TAG) => {
                    let children = arena.children(id)?;
                    stack.push(children[1].ok_or(RecordForestError::Corrupt("pair has no right"))?);
                    stack.push(children[0].ok_or(RecordForestError::Corrupt("pair has no left"))?);
                }
                _ => return Err(RecordForestError::Corrupt("unknown component node")),
            }
        }
        Ok(result)
    }

    pub fn query_presentation(
        &self,
        arena: &PageArena,
        expected_epoch: PresentationEpoch,
        requested: PresentationRequest,
        coverage_order: &impl CoverageOrderOracle,
    ) -> Result<PresentationLookup, PresentationError> {
        let view = decode_manifest(arena.payload(self.owner.id())?)?;
        if view.epoch != expected_epoch {
            let reason = if view.epoch.source != expected_epoch.source {
                PresentationUnknownReason::StaleSourceRevision
            } else if view.epoch.grammar != expected_epoch.grammar {
                PresentationUnknownReason::StaleGrammarRevision
            } else if view.epoch.generation != expected_epoch.generation {
                PresentationUnknownReason::StaleParseGeneration
            } else {
                PresentationUnknownReason::StaleSemanticRoot
            };
            return Ok(PresentationLookup::Unknown(PresentationUnknownRange {
                range: requested.range,
                reason,
            }));
        }
        if !presentation_range_contains(view.structural_range, requested.range, coverage_order)? {
            return Ok(PresentationLookup::Unknown(PresentationUnknownRange {
                range: requested.range,
                reason: PresentationUnknownReason::OutsideProvenRange,
            }));
        }
        let Some(root) = self.components(arena)?.presentation else {
            return Ok(PresentationLookup::Unknown(PresentationUnknownRange {
                range: requested.range,
                reason: PresentationUnknownReason::MissingLease,
            }));
        };
        query_presentation_root(arena, root, expected_epoch, requested, coverage_order)
    }

    pub fn release_later(self, arena: &mut PageArena) -> Result<(), RecordForestError> {
        arena
            .release_later(self.owner)
            .map_err(legacy_owner_transfer_error)?;
        Ok(())
    }
}

fn build_component_tree(
    transaction: &mut ArenaBuildTransaction<'_>,
    mut owners: Vec<ArenaOwnerHandle>,
    receipt: &mut RecordForestReceipt,
) -> Result<Option<ArenaOwnerHandle>, RecordForestError> {
    if owners.is_empty() {
        return Ok(None);
    }
    while owners.len() > 1 {
        let mut next = Vec::with_capacity(owners.len().div_ceil(2));
        let mut iterator = owners.into_iter();
        while let Some(left) = iterator.next() {
            if let Some(right) = iterator.next() {
                let children = [transaction.id(&left), transaction.id(&right)];
                let (pair, allocation) =
                    transaction.allocate(&[COMPONENT_PAIR_TAG, VERSION], &children)?;
                transaction.release(left)?;
                transaction.release(right)?;
                receipt.payload_bytes_copied += allocation.payload_bytes_copied;
                receipt.child_references_added += allocation.child_references_added;
                next.push(pair);
            } else {
                next.push(left);
            }
        }
        owners = next;
    }
    Ok(owners.pop())
}

fn build_component_tree_with_fault(
    transaction: &mut ArenaBuildTransaction<'_>,
    mut owners: Vec<ArenaOwnerHandle>,
    receipt: &mut RecordForestReceipt,
    allocation_index: &mut usize,
    fail_after: Option<usize>,
) -> Result<Option<ArenaOwnerHandle>, RecordForestError> {
    if owners.is_empty() {
        return Ok(None);
    }
    while owners.len() > 1 {
        let mut next = Vec::with_capacity(owners.len().div_ceil(2));
        let mut iterator = owners.into_iter();
        while let Some(left) = iterator.next() {
            if let Some(right) = iterator.next() {
                let children = [transaction.id(&left), transaction.id(&right)];
                let (pair, allocation) =
                    transaction.allocate(&[COMPONENT_PAIR_TAG, VERSION], &children)?;
                *allocation_index += 1;
                if fail_after == Some(*allocation_index) {
                    return Err(RecordForestError::Invalid(
                        "forced candidate commit allocation failure",
                    ));
                }
                transaction.release(left)?;
                transaction.release(right)?;
                receipt.payload_bytes_copied += allocation.payload_bytes_copied;
                receipt.child_references_added += allocation.child_references_added;
                next.push(pair);
            } else {
                next.push(left);
            }
        }
        owners = next;
    }
    Ok(owners.pop())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompositeManifestView {
    epoch: PresentationEpoch,
    structural_range: PresentationRange,
    unknown: UnknownRange,
}

fn validate_composite_contract(
    arena: &PageArena,
    epoch: PresentationEpoch,
    structural_range: PresentationRange,
    unknown: UnknownRange,
    coverage_root: Option<ArenaId>,
    presentation_root: Option<ArenaId>,
    order: &impl CoverageOrderOracle,
) -> Result<(), RecordForestError> {
    if compare_anchor(structural_range.start, structural_range.end, order)? == Ordering::Greater {
        return Err(RecordForestError::Invalid("reversed structural range"));
    }
    let coverage_summary = ForestSequenceRef {
        root: coverage_root,
    }
    .summary(arena)?;
    if coverage_summary.items == 0 {
        if structural_range.start != structural_range.end {
            return Err(RecordForestError::Invalid(
                "empty coverage has a nonempty structural range",
            ));
        }
    } else if stable_anchor_from_key(coverage_summary.first_key) != structural_range.start
        || stable_anchor_from_key(coverage_summary.last_key) != structural_range.end
    {
        return Err(RecordForestError::Invalid(
            "coverage and structural range disagree",
        ));
    }
    if let Some(root) = presentation_root {
        let contract = presentation_contract_root(arena, root)
            .map_err(|_| RecordForestError::Invalid("unreadable presentation contract"))?;
        if contract.epoch != epoch {
            return Err(RecordForestError::Invalid(
                "presentation epoch differs from forest epoch",
            ));
        }
        if !presentation_range_contains(structural_range, contract.request.range, order)? {
            return Err(RecordForestError::Invalid(
                "presentation range exceeds structural authority",
            ));
        }
        if let (Some(start), Some(end)) = (unknown.start, unknown.end)
            && presentation_ranges_intersect(
                contract.request.range,
                PresentationRange { start, end },
                order,
            )?
        {
            return Err(RecordForestError::Invalid(
                "presentation range overlaps unknown structure",
            ));
        }
    }
    Ok(())
}

fn presentation_range_contains(
    outer: PresentationRange,
    inner: PresentationRange,
    order: &impl CoverageOrderOracle,
) -> Result<bool, RecordForestError> {
    Ok(
        compare_anchor(outer.start, inner.start, order)? != Ordering::Greater
            && compare_anchor(inner.end, outer.end, order)? != Ordering::Greater,
    )
}

fn presentation_ranges_intersect(
    left: PresentationRange,
    right: PresentationRange,
    order: &impl CoverageOrderOracle,
) -> Result<bool, RecordForestError> {
    Ok(
        compare_anchor(left.start, right.end, order)? == Ordering::Less
            && compare_anchor(right.start, left.end, order)? == Ordering::Less,
    )
}

fn encode_manifest(
    epoch: PresentationEpoch,
    structural_range: PresentationRange,
    unknown: UnknownRange,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(100);
    output.push(MANIFEST_TAG);
    output.push(VERSION);
    output.push(u8::from(unknown.start.is_some()));
    output.push(u8::from(unknown.end.is_some()));
    push_u64(&mut output, epoch.source.0);
    push_u64(&mut output, epoch.grammar.0);
    push_u64(&mut output, epoch.generation.0);
    push_u64(&mut output, epoch.semantic_root.0);
    encode_anchor(structural_range.start, &mut output);
    encode_anchor(structural_range.end, &mut output);
    encode_anchor(unknown.start.unwrap_or_default(), &mut output);
    encode_anchor(unknown.end.unwrap_or_default(), &mut output);
    output
}

fn decode_manifest(payload: &[u8]) -> Result<CompositeManifestView, RecordForestError> {
    let mut decoder = Decoder::new(payload);
    if decoder.u8()? != MANIFEST_TAG || decoder.u8()? != VERSION {
        return Err(RecordForestError::Corrupt("wrong manifest header"));
    }
    let has_start = decoder.u8()?;
    let has_end = decoder.u8()?;
    let epoch = PresentationEpoch {
        source: crate::SourceRevision(decoder.u64()?),
        grammar: crate::GrammarRevision(decoder.u64()?),
        generation: crate::ParseGeneration(decoder.u64()?),
        semantic_root: crate::SemanticRootGeneration(decoder.u64()?),
    };
    let structural_range = PresentationRange {
        start: decoder.anchor()?,
        end: decoder.anchor()?,
    };
    let start = decoder.anchor()?;
    let end = decoder.anchor()?;
    if !decoder.is_empty() || has_start > 1 || has_end > 1 {
        return Err(RecordForestError::Corrupt("invalid manifest payload"));
    }
    Ok(CompositeManifestView {
        epoch,
        structural_range,
        unknown: UnknownRange {
            start: (has_start == 1).then_some(start),
            end: (has_end == 1).then_some(end),
        },
    })
}

fn encode_anchor(anchor: ForestAnchor, output: &mut Vec<u8>) {
    push_u64(output, anchor.coverage.0);
    push_u32(output, anchor.local_bytes);
    push_u32(output, anchor.local_utf16);
}

fn nonzero(value: u64) -> Option<u64> {
    (value != 0).then_some(value)
}

fn u64_at(payload: &[u8], offset: usize) -> Result<u64, RecordForestError> {
    let bytes = payload
        .get(offset..offset + 8)
        .ok_or(RecordForestError::Corrupt("truncated summary key"))?;
    Ok(u64::from_le_bytes(
        bytes.try_into().expect("summary key has eight bytes"),
    ))
}

fn u128_at(payload: &[u8], offset: usize) -> Result<u128, RecordForestError> {
    let end = offset
        .checked_add(16)
        .ok_or(RecordForestError::Overflow("u128 offset"))?;
    let bytes = payload
        .get(offset..end)
        .ok_or(RecordForestError::Corrupt("short u128"))?;
    Ok(u128::from_le_bytes(
        bytes.try_into().expect("u128 slice has exact length"),
    ))
}

fn anchor_key(
    anchor: ForestAnchor,
    order: &impl CoverageOrderOracle,
) -> Result<u128, RecordForestError> {
    Ok((u128::from(order.rank(anchor.coverage)?) << 64)
        | (u128::from(anchor.local_bytes) << 32)
        | u128::from(anchor.local_utf16))
}

const fn stable_anchor_key(anchor: ForestAnchor) -> u128 {
    (anchor.coverage.0 as u128) << 64
        | (anchor.local_bytes as u128) << 32
        | anchor.local_utf16 as u128
}

fn stable_anchor_from_key(key: u128) -> ForestAnchor {
    ForestAnchor {
        coverage: ForestCoverageId(
            u64::try_from(key >> 64).expect("stable anchor coverage occupies 64 bits"),
        ),
        local_bytes: u32::try_from((key >> 32) & u128::from(u32::MAX))
            .expect("stable anchor byte offset occupies 32 bits"),
        local_utf16: u32::try_from(key & u128::from(u32::MAX))
            .expect("stable anchor UTF-16 offset occupies 32 bits"),
    }
}

fn compare_anchor(
    left: ForestAnchor,
    right: ForestAnchor,
    order: &impl CoverageOrderOracle,
) -> Result<Ordering, RecordForestError> {
    Ok(anchor_key(left, order)?.cmp(&anchor_key(right, order)?))
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

fn push_u128(output: &mut Vec<u8>, value: u128) {
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

    fn take(&mut self, count: usize) -> Result<&'a [u8], RecordForestError> {
        let (value, remaining) = self
            .remaining
            .split_at_checked(count)
            .ok_or(RecordForestError::Corrupt("truncated scalar"))?;
        self.remaining = remaining;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8, RecordForestError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, RecordForestError> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .expect("decoder requested two bytes"),
        ))
    }

    fn u32(&mut self) -> Result<u32, RecordForestError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .expect("decoder requested four bytes"),
        ))
    }

    fn u64(&mut self) -> Result<u64, RecordForestError> {
        Ok(u64::from_le_bytes(
            self.take(8)?
                .try_into()
                .expect("decoder requested eight bytes"),
        ))
    }

    fn u128(&mut self) -> Result<u128, RecordForestError> {
        Ok(u128::from_le_bytes(
            self.take(16)?
                .try_into()
                .expect("decoder requested exactly sixteen bytes"),
        ))
    }

    fn anchor(&mut self) -> Result<ForestAnchor, RecordForestError> {
        Ok(ForestAnchor {
            coverage: ForestCoverageId(self.u64()?),
            local_bytes: self.u32()?,
            local_utf16: self.u32()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NumericOrder;

    impl CoverageOrderOracle for NumericOrder {
        fn rank(&self, coverage: ForestCoverageId) -> Result<u64, RecordForestError> {
            Ok(coverage.0)
        }
    }

    fn test_epoch() -> PresentationEpoch {
        PresentationEpoch {
            source: crate::SourceRevision(1),
            grammar: crate::GrammarRevision(1),
            generation: crate::ParseGeneration(1),
            semantic_root: crate::SemanticRootGeneration(1),
        }
    }

    const fn test_anchor(coverage: u64) -> ForestAnchor {
        ForestAnchor {
            coverage: ForestCoverageId(coverage),
            local_bytes: 0,
            local_utf16: 0,
        }
    }

    fn test_record(id: u64) -> BlockRecord {
        BlockRecord {
            id: ForestBlockId(id),
            parent: None,
            kind_tag: 1,
            context: 0,
            property: None,
            start: test_anchor(id),
            end: test_anchor(id + 1),
            content: Some(ForestRunCursorId(id)),
            subtree_last: ForestBlockId(id),
            terminal: true,
        }
    }

    fn one_record_payload() -> Vec<u8> {
        encode_record_page(&[test_record(1)]).expect("one record page")
    }

    fn one_order_payload() -> Vec<u8> {
        encode_order_page(&[ForestBlockId(1)]).expect("one order page")
    }

    fn one_coverage_payload() -> Vec<u8> {
        encode_coverage_page(&[CoverageSegment {
            owner: ForestBlockId(1),
            kind: CoverageSegmentKind::Terminal,
            start: test_anchor(1),
            end: test_anchor(2),
        }])
        .expect("one coverage page")
    }

    fn small_candidate<'a>(
        arena: &'a mut PageArena,
        receipt: &mut RecordForestReceipt,
    ) -> RecordForestCandidateTransaction<'a> {
        RecordForestCandidateTransaction::new(
            arena,
            test_epoch(),
            PresentationRange {
                start: test_anchor(1),
                end: test_anchor(2),
            },
            UnknownRange::default(),
        )
        .stream_sequence_component(
            CandidateSequenceRole::Records,
            [one_record_payload()],
            receipt,
        )
        .expect("record component")
        .stream_sequence_component(CandidateSequenceRole::Order, [one_order_payload()], receipt)
        .expect("order component")
        .stream_sequence_component(
            CandidateSequenceRole::Coverage,
            [one_coverage_payload()],
            receipt,
        )
        .expect("coverage component")
        .with_open_overlay(
            [OpenFrame {
                block: ForestBlockId(2),
                parent: None,
                kind_tag: 1,
                context: 0,
                start: test_anchor(2),
                current: test_anchor(2),
                pending: None,
            }],
            receipt,
        )
        .expect("frontier component")
    }

    #[test]
    fn failed_leaf_batch_page_n_reclaims_every_prior_page() {
        let mut arena = PageArena::new();
        let mut receipt = RecordForestReceipt::default();
        let error = allocate_leaf_payloads(
            &mut arena,
            vec![vec![RECORD_PAGE_TAG], vec![0; ARENA_PAGE_BYTES + 1]],
            &mut receipt,
        )
        .expect_err("second page is deliberately oversized");
        assert_eq!(
            error,
            RecordForestError::Arena(ArenaError::PayloadTooLarge(ARENA_PAGE_BYTES + 1))
        );
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(16).expect("rollback reclaim");
        }
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn candidate_cancel_and_every_commit_allocation_failure_reclaim_from_first_component() {
        for boundary in 0..=4 {
            let mut arena = PageArena::new();
            let mut receipt = RecordForestReceipt::default();
            let mut candidate = RecordForestCandidateTransaction::new(
                &mut arena,
                test_epoch(),
                PresentationRange {
                    start: test_anchor(1),
                    end: test_anchor(2),
                },
                UnknownRange::default(),
            );
            if boundary > 0 {
                candidate = candidate
                    .stream_sequence_component(
                        CandidateSequenceRole::Records,
                        [one_record_payload()],
                        &mut receipt,
                    )
                    .unwrap();
            }
            if boundary > 1 {
                candidate = candidate
                    .stream_sequence_component(
                        CandidateSequenceRole::Order,
                        [one_order_payload()],
                        &mut receipt,
                    )
                    .unwrap();
            }
            if boundary > 2 {
                candidate = candidate
                    .stream_sequence_component(
                        CandidateSequenceRole::Coverage,
                        [one_coverage_payload()],
                        &mut receipt,
                    )
                    .unwrap();
            }
            if boundary > 3 {
                candidate = candidate.with_open_overlay([], &mut receipt).unwrap();
            }
            drop(candidate);
            while arena.metrics().pending_releases != 0 {
                arena.poll_reclaim(32).unwrap();
            }
            assert_eq!(arena.metrics().live_nodes, 0, "cancel boundary {boundary}");
        }

        for fail_after in 1..=8 {
            let mut arena = PageArena::new();
            let mut receipt = RecordForestReceipt::default();
            let candidate = small_candidate(&mut arena, &mut receipt);
            let error = candidate
                .commit_failing_after_allocation(&NumericOrder, &mut receipt, fail_after)
                .expect_err("forced commit stage");
            assert_eq!(
                error,
                RecordForestError::Invalid("forced candidate commit allocation failure")
            );
            while arena.metrics().pending_releases != 0 {
                arena.poll_reclaim(64).unwrap();
            }
            assert_eq!(
                arena.metrics().live_nodes,
                0,
                "commit allocation {fail_after}"
            );
        }
    }

    #[test]
    fn candidate_page_n_failure_rolls_back_prior_components_and_stream_prefix() {
        let mut arena = PageArena::new();
        let mut receipt = RecordForestReceipt::default();
        let candidate = RecordForestCandidateTransaction::new(
            &mut arena,
            test_epoch(),
            PresentationRange {
                start: test_anchor(1),
                end: test_anchor(2),
            },
            UnknownRange::default(),
        )
        .stream_sequence_component(
            CandidateSequenceRole::Records,
            [one_record_payload()],
            &mut receipt,
        )
        .unwrap();
        let error = candidate
            .stream_sequence_component(
                CandidateSequenceRole::Order,
                [one_order_payload(), vec![0; ARENA_PAGE_BYTES + 1]],
                &mut receipt,
            )
            .expect_err("second order page exceeds arena cap");
        assert_eq!(
            error,
            RecordForestError::Arena(ArenaError::PayloadTooLarge(ARENA_PAGE_BYTES + 1))
        );
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(64).unwrap();
        }
        assert_eq!(arena.metrics().live_nodes, 0);
    }

    #[test]
    fn hundred_thousand_block_candidate_stream_has_logarithmic_temporary_owners() {
        const BLOCKS: u64 = 100_000;
        let mut arena = PageArena::new();
        let mut receipt = RecordForestReceipt::default();
        let record_capacity = (ARENA_PAGE_BYTES - RECORD_HEADER_BYTES) / RECORD_BYTES;
        let order_capacity = (ARENA_PAGE_BYTES - ORDER_HEADER_BYTES) / ORDER_ENTRY_BYTES;
        let coverage_capacity = (ARENA_PAGE_BYTES - COVERAGE_HEADER_BYTES) / COVERAGE_ENTRY_BYTES;
        let candidate = RecordForestCandidateTransaction::new(
            &mut arena,
            test_epoch(),
            PresentationRange {
                start: test_anchor(1),
                end: test_anchor(BLOCKS + 1),
            },
            UnknownRange::default(),
        )
        .stream_sequence_component(
            CandidateSequenceRole::Records,
            (1..=BLOCKS).step_by(record_capacity).map(|start| {
                let end = (start + u64::try_from(record_capacity).unwrap()).min(BLOCKS + 1);
                encode_record_page(&(start..end).map(test_record).collect::<Vec<_>>()).unwrap()
            }),
            &mut receipt,
        )
        .unwrap()
        .stream_sequence_component(
            CandidateSequenceRole::Order,
            (1..=BLOCKS).step_by(order_capacity).map(|start| {
                let end = (start + u64::try_from(order_capacity).unwrap()).min(BLOCKS + 1);
                encode_order_page(&(start..end).map(ForestBlockId).collect::<Vec<_>>()).unwrap()
            }),
            &mut receipt,
        )
        .unwrap()
        .stream_sequence_component(
            CandidateSequenceRole::Coverage,
            (1..=BLOCKS).step_by(coverage_capacity).map(|start| {
                let end = (start + u64::try_from(coverage_capacity).unwrap()).min(BLOCKS + 1);
                encode_coverage_page(
                    &(start..end)
                        .map(|id| CoverageSegment {
                            owner: ForestBlockId(id),
                            kind: CoverageSegmentKind::Terminal,
                            start: test_anchor(id),
                            end: test_anchor(id + 1),
                        })
                        .collect::<Vec<_>>(),
                )
                .unwrap()
            }),
            &mut receipt,
        )
        .unwrap();
        let before_commit = candidate.temporary_bounds();
        assert!(before_commit.maximum_live_owner_handles <= 24);
        assert!(before_commit.owner_journal_slots <= 24);
        assert!(before_commit.owner_journal_capacity <= 32);
        assert!(receipt.maximum_streaming_sequence_roots <= 12);
        assert!(receipt.maximum_streaming_sequence_bin_slots <= 32);
        assert!(receipt.maximum_temporary_bytes <= ARENA_PAGE_BYTES);
        let manifest = candidate.commit(&NumericOrder, &mut receipt).unwrap();
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(16_384).unwrap();
        }
        let retained = arena.metrics();
        let caller_page_scratch_bytes = (record_capacity * std::mem::size_of::<BlockRecord>())
            .max(order_capacity * std::mem::size_of::<ForestBlockId>())
            .max(coverage_capacity * std::mem::size_of::<CoverageSegment>());
        let accounted_retained_bytes = retained.live_payload_bytes + retained.slot_storage_bytes;
        eprintln!(
            "candidate_stream blocks={BLOCKS} arena_nodes={} arena_payload_bytes={} slot_capacity={} slot_storage_bytes={} accounted_retained_bytes={} accounted_bytes_per_block={}.{:02} caller_page_scratch={} max_leaf_buffer={} max_stream_roots={} stream_bin_slots={} stream_bin_bytes={} max_live_owners={} owner_journal_slots={} owner_journal_capacity={} owner_journal_bytes={}",
            retained.live_nodes,
            retained.live_payload_bytes,
            retained.slot_capacity,
            retained.slot_storage_bytes,
            accounted_retained_bytes,
            accounted_retained_bytes / usize::try_from(BLOCKS).unwrap(),
            accounted_retained_bytes * 100 / usize::try_from(BLOCKS).unwrap() % 100,
            caller_page_scratch_bytes,
            receipt.maximum_temporary_bytes,
            receipt.maximum_streaming_sequence_roots,
            receipt.maximum_streaming_sequence_bin_slots,
            receipt.maximum_streaming_sequence_bin_bytes,
            receipt.maximum_transaction_live_owners,
            receipt.maximum_transaction_journal_slots,
            receipt.maximum_transaction_journal_capacity,
            receipt.maximum_transaction_journal_bytes,
        );
        assert!(
            (140..=145).contains(&(retained.live_payload_bytes / usize::try_from(BLOCKS).unwrap()))
        );
        manifest.release_later(&mut arena).unwrap();
        while arena.metrics().pending_releases != 0 {
            arena.poll_reclaim(16_384).unwrap();
        }
        assert_eq!(arena.metrics().live_nodes, 0);
    }
}

//! Throwaway challenger for one persistent Euler structural sequence.
//!
//! The important mechanism is [`EulerSummary::followed_by`].  It is an exact
//! monoid over arbitrary Enter/Exit token fragments.  For a balanced container
//! interior, the Enter tokens at the minimum pre-Enter depth are exactly the
//! direct children.  Their order-sensitive fold can therefore be answered
//! without a separate sequence per container.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ops::Range;
use std::sync::Arc;

pub mod serialized_green;

pub const DEFAULT_PAGE_TOKENS: usize = 256;

/// Honest fixed-width codec model used by the receipt.
pub const PACKED_ENTER_BYTES: usize = 10; // tag + u64 BlockId + 3-bit summary byte
pub const PACKED_EXIT_BYTES: usize = 1;
pub const PACKED_TOKEN_PAGE_HEADER_BYTES: usize = 16;
pub const PACKED_BRANCH_SUMMARY_BYTES: usize = 56;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClosedChildSummary {
    pub ends_blank: bool,
    pub item_loose_if_nonlast: bool,
    pub item_loose_if_last: bool,
}

impl ClosedChildSummary {
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self {
            ends_blank: bits & 1 != 0,
            item_loose_if_nonlast: bits & 2 != 0,
            item_loose_if_last: bits & 4 != 0,
        }
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.ends_blank as u8
            | ((self.item_loose_if_nonlast as u8) << 1)
            | ((self.item_loose_if_last as u8) << 2)
    }
}

/// The exact order-sensitive finite fold used by CommonMark list tightness.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChildSequenceSummary {
    pub had_child: bool,
    pub any_nonlast_child_ends_blank: bool,
    pub last_child_ends_blank: bool,
    pub list_loose_before_last: bool,
    pub last_item_loose_if_nonlast: bool,
    pub last_item_loose_if_last: bool,
}

impl ChildSequenceSummary {
    #[must_use]
    pub const fn singleton(child: ClosedChildSummary) -> Self {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    Enter {
        block: BlockId,
        closed: ClosedChildSummary,
    },
    Exit,
}

impl Token {
    #[must_use]
    pub const fn packed_bytes(self) -> usize {
        match self {
            Self::Enter { .. } => PACKED_ENTER_BYTES,
            Self::Exit => PACKED_EXIT_BYTES,
        }
    }
}

/// Constant-size associative summary over arbitrary Euler token fragments.
///
/// `minimum_enter_depth` is measured immediately before each Enter, relative
/// to the start of this fragment.  Every Enter in the right operand is shifted
/// by the left operand's balance.  Therefore only the minimum from each side
/// can possibly be the combined minimum; higher-depth labels can be discarded
/// permanently.  Equal minima combine in source order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EulerSummary {
    pub tokens: u64,
    pub enters: u64,
    pub balance: i64,
    pub minimum_prefix: i64,
    pub maximum_prefix: i64,
    pub minimum_enter_depth: Option<i64>,
    pub outermost: ChildSequenceSummary,
    pub outermost_count: u64,
    pub first_outermost: Option<BlockId>,
    pub last_outermost: Option<BlockId>,
}

impl EulerSummary {
    #[must_use]
    pub const fn token(token: Token) -> Self {
        match token {
            Token::Enter { block, closed } => Self {
                tokens: 1,
                enters: 1,
                balance: 1,
                minimum_prefix: 0,
                maximum_prefix: 1,
                minimum_enter_depth: Some(0),
                outermost: ChildSequenceSummary::singleton(closed),
                outermost_count: 1,
                first_outermost: Some(block),
                last_outermost: Some(block),
            },
            Token::Exit => Self {
                tokens: 1,
                enters: 0,
                balance: -1,
                minimum_prefix: -1,
                maximum_prefix: 0,
                minimum_enter_depth: None,
                outermost: ChildSequenceSummary {
                    had_child: false,
                    any_nonlast_child_ends_blank: false,
                    last_child_ends_blank: false,
                    list_loose_before_last: false,
                    last_item_loose_if_nonlast: false,
                    last_item_loose_if_last: false,
                },
                outermost_count: 0,
                first_outermost: None,
                last_outermost: None,
            },
        }
    }

    #[must_use]
    pub fn followed_by(self, suffix: Self) -> Self {
        let shifted_suffix_minimum = suffix.minimum_enter_depth.map(|depth| self.balance + depth);
        let minimum_enter_depth = match (self.minimum_enter_depth, shifted_suffix_minimum) {
            (None, None) => None,
            (Some(left), None) => Some(left),
            (None, Some(right)) => Some(right),
            (Some(left), Some(right)) => Some(left.min(right)),
        };
        let left_is_minimum = self.minimum_enter_depth == minimum_enter_depth;
        let right_is_minimum = shifted_suffix_minimum == minimum_enter_depth;
        let (outermost, outermost_count, first_outermost, last_outermost) =
            match (left_is_minimum, right_is_minimum) {
                (true, true) => (
                    self.outermost.followed_by(suffix.outermost),
                    self.outermost_count + suffix.outermost_count,
                    self.first_outermost.or(suffix.first_outermost),
                    suffix.last_outermost.or(self.last_outermost),
                ),
                (true, false) => (
                    self.outermost,
                    self.outermost_count,
                    self.first_outermost,
                    self.last_outermost,
                ),
                (false, true) => (
                    suffix.outermost,
                    suffix.outermost_count,
                    suffix.first_outermost,
                    suffix.last_outermost,
                ),
                (false, false) => (ChildSequenceSummary::default(), 0, None, None),
            };
        Self {
            tokens: self.tokens + suffix.tokens,
            enters: self.enters + suffix.enters,
            balance: self.balance + suffix.balance,
            minimum_prefix: self
                .minimum_prefix
                .min(self.balance + suffix.minimum_prefix),
            maximum_prefix: self
                .maximum_prefix
                .max(self.balance + suffix.maximum_prefix),
            minimum_enter_depth,
            outermost,
            outermost_count,
            first_outermost,
            last_outermost,
        }
    }

    #[must_use]
    pub fn from_tokens(tokens: &[Token]) -> Self {
        tokens
            .iter()
            .copied()
            .fold(Self::default(), |summary, token| {
                summary.followed_by(Self::token(token))
            })
    }

    #[must_use]
    pub const fn is_balanced_forest(self) -> bool {
        self.balance == 0 && self.minimum_prefix >= 0
    }

    pub fn direct_children(self) -> Result<ChildSequenceSummary, QueryError> {
        if !self.is_balanced_forest() {
            return Err(QueryError::NotBalancedInterior);
        }
        if self.enters == 0 {
            return Ok(ChildSequenceSummary::default());
        }
        if self.minimum_enter_depth != Some(0) {
            return Err(QueryError::CorruptSummary);
        }
        Ok(self.outermost)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MutationReceipt {
    pub nodes_visited: usize,
    pub nodes_allocated: usize,
    pub token_pages_allocated: usize,
    pub packed_token_bytes_copied: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueryReceipt {
    pub nodes_visited: usize,
    pub whole_nodes_folded: usize,
    pub tokens_scanned: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EnumerationReceipt {
    pub nodes_visited: usize,
    pub nodes_skipped: usize,
    pub tokens_scanned: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryError {
    RangeOutOfBounds,
    NotBalancedInterior,
    CorruptSummary,
    DuplicateBlock(BlockId),
    UnmatchedExit,
    UnclosedBlock(BlockId),
    MissingBoundary(BlockId),
}

impl fmt::Display for QueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RangeOutOfBounds => formatter.write_str("range is outside the sequence"),
            Self::NotBalancedInterior => formatter.write_str("range is not a balanced forest"),
            Self::CorruptSummary => formatter.write_str("Euler summary is internally inconsistent"),
            Self::DuplicateBlock(block) => write!(formatter, "duplicate block {block:?}"),
            Self::UnmatchedExit => formatter.write_str("unmatched Exit token"),
            Self::UnclosedBlock(block) => write!(formatter, "unclosed block {block:?}"),
            Self::MissingBoundary(block) => write!(formatter, "missing boundary for {block:?}"),
        }
    }
}

impl std::error::Error for QueryError {}

#[derive(Clone, Debug)]
struct Node {
    page_id: u64,
    priority: u64,
    page: Arc<[Token]>,
    left: Option<Arc<Node>>,
    right: Option<Arc<Node>>,
    page_summary: EulerSummary,
    summary: EulerSummary,
}

impl Node {
    fn new(
        page_id: u64,
        page: Arc<[Token]>,
        left: Option<Arc<Self>>,
        right: Option<Arc<Self>>,
        receipt: &mut MutationReceipt,
    ) -> Arc<Self> {
        debug_assert!(!page.is_empty());
        let page_summary = EulerSummary::from_tokens(&page);
        let summary = summary_of(&left)
            .followed_by(page_summary)
            .followed_by(summary_of(&right));
        receipt.nodes_allocated += 1;
        Arc::new(Self {
            page_id,
            priority: page_priority(page_id),
            page,
            left,
            right,
            page_summary,
            summary,
        })
    }
}

fn summary_of(node: &Option<Arc<Node>>) -> EulerSummary {
    node.as_ref()
        .map_or_else(EulerSummary::default, |node| node.summary)
}

fn token_len(node: &Option<Arc<Node>>) -> u64 {
    summary_of(node).tokens
}

fn page_priority(page_id: u64) -> u64 {
    let mut value = page_id.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn merge(
    left: Option<Arc<Node>>,
    right: Option<Arc<Node>>,
    receipt: &mut MutationReceipt,
) -> Option<Arc<Node>> {
    match (left, right) {
        (None, right) => right,
        (left, None) => left,
        (Some(left), Some(right)) if left.priority >= right.priority => {
            receipt.nodes_visited += 1;
            let merged = merge(left.right.clone(), Some(right), receipt);
            Some(Node::new(
                left.page_id,
                left.page.clone(),
                left.left.clone(),
                merged,
                receipt,
            ))
        }
        (Some(left), Some(right)) => {
            receipt.nodes_visited += 1;
            let merged = merge(Some(left), right.left.clone(), receipt);
            Some(Node::new(
                right.page_id,
                right.page.clone(),
                merged,
                right.right.clone(),
                receipt,
            ))
        }
    }
}

fn split(
    root: Option<Arc<Node>>,
    at: u64,
    next_page_id: &mut u64,
    receipt: &mut MutationReceipt,
) -> (Option<Arc<Node>>, Option<Arc<Node>>) {
    let Some(root) = root else {
        return (None, None);
    };
    receipt.nodes_visited += 1;
    let left_tokens = token_len(&root.left);
    let page_tokens = u64::try_from(root.page.len()).expect("page length fits u64");
    if at < left_tokens {
        let (before, after) = split(root.left.clone(), at, next_page_id, receipt);
        let rebuilt = Node::new(
            root.page_id,
            root.page.clone(),
            after,
            root.right.clone(),
            receipt,
        );
        return (before, Some(rebuilt));
    }
    if at > left_tokens + page_tokens {
        let (before, after) = split(
            root.right.clone(),
            at - left_tokens - page_tokens,
            next_page_id,
            receipt,
        );
        let rebuilt = Node::new(
            root.page_id,
            root.page.clone(),
            root.left.clone(),
            before,
            receipt,
        );
        return (Some(rebuilt), after);
    }
    if at == left_tokens {
        let right = Node::new(
            root.page_id,
            root.page.clone(),
            None,
            root.right.clone(),
            receipt,
        );
        return (root.left.clone(), Some(right));
    }
    if at == left_tokens + page_tokens {
        let left = Node::new(
            root.page_id,
            root.page.clone(),
            root.left.clone(),
            None,
            receipt,
        );
        return (Some(left), root.right.clone());
    }

    let local = usize::try_from(at - left_tokens).expect("local split fits usize");
    let before_page: Arc<[Token]> = Arc::from(&root.page[..local]);
    let after_page: Arc<[Token]> = Arc::from(&root.page[local..]);
    let before_id = *next_page_id;
    *next_page_id += 1;
    let after_id = *next_page_id;
    *next_page_id += 1;
    receipt.token_pages_allocated += 2;
    receipt.packed_token_bytes_copied += before_page
        .iter()
        .chain(after_page.iter())
        .copied()
        .map(Token::packed_bytes)
        .sum::<usize>();
    let before_node = Some(Node::new(before_id, before_page, None, None, receipt));
    let after_node = Some(Node::new(after_id, after_page, None, None, receipt));
    (
        merge(root.left.clone(), before_node, receipt),
        merge(after_node, root.right.clone(), receipt),
    )
}

#[derive(Clone, Debug)]
pub struct EulerSequence {
    root: Option<Arc<Node>>,
    next_page_id: u64,
    page_tokens: usize,
}

impl EulerSequence {
    #[must_use]
    pub fn from_tokens(tokens: &[Token], page_tokens: usize) -> Self {
        assert!(page_tokens > 0);
        let mut receipt = MutationReceipt::default();
        let mut root = None;
        let mut next_page_id = 1_u64;
        for page in tokens.chunks(page_tokens) {
            let page: Arc<[Token]> = Arc::from(page);
            let node = Some(Node::new(next_page_id, page, None, None, &mut receipt));
            next_page_id += 1;
            root = merge(root, node, &mut receipt);
        }
        Self {
            root,
            next_page_id,
            page_tokens,
        }
    }

    #[must_use]
    pub fn len(&self) -> u64 {
        token_len(&self.root)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    #[must_use]
    pub fn summary(&self) -> EulerSummary {
        summary_of(&self.root)
    }

    pub fn range_summary(
        &self,
        range: Range<u64>,
        receipt: &mut QueryReceipt,
    ) -> Result<EulerSummary, QueryError> {
        if range.start > range.end || range.end > self.len() {
            return Err(QueryError::RangeOutOfBounds);
        }
        Ok(range_summary_node(&self.root, 0, &range, receipt))
    }

    pub fn direct_child_summary(
        &self,
        interior: Range<u64>,
        receipt: &mut QueryReceipt,
    ) -> Result<ChildSequenceSummary, QueryError> {
        self.range_summary(interior, receipt)?.direct_children()
    }

    pub fn collect_outermost(
        &self,
        interior: Range<u64>,
        receipt: &mut EnumerationReceipt,
    ) -> Result<Vec<(BlockId, ClosedChildSummary)>, QueryError> {
        if interior.start > interior.end || interior.end > self.len() {
            return Err(QueryError::RangeOutOfBounds);
        }
        let mut validation = QueryReceipt::default();
        self.range_summary(interior.clone(), &mut validation)?
            .direct_children()?;
        let mut depth = 0_i64;
        let mut output = Vec::new();
        collect_outermost_node(&self.root, 0, &interior, &mut depth, &mut output, receipt);
        if depth != 0 {
            return Err(QueryError::NotBalancedInterior);
        }
        Ok(output)
    }

    pub fn replace_token(
        &self,
        index: u64,
        token: Token,
        receipt: &mut MutationReceipt,
    ) -> Result<Self, QueryError> {
        self.splice(index..index + 1, &[token], receipt)
    }

    pub fn splice(
        &self,
        range: Range<u64>,
        replacement: &[Token],
        receipt: &mut MutationReceipt,
    ) -> Result<Self, QueryError> {
        if range.start > range.end || range.end > self.len() {
            return Err(QueryError::RangeOutOfBounds);
        }
        let mut next_page_id = self.next_page_id;
        let (prefix, rest) = split(self.root.clone(), range.start, &mut next_page_id, receipt);
        let (_, suffix) = split(rest, range.end - range.start, &mut next_page_id, receipt);
        let replacement_root =
            build_replacement(replacement, self.page_tokens, &mut next_page_id, receipt);
        let root = merge(merge(prefix, replacement_root, receipt), suffix, receipt);
        Ok(Self {
            root,
            next_page_id,
            page_tokens: self.page_tokens,
        })
    }

    /// Moves one already-balanced subtree range. `destination_after_cut` is a
    /// rank in the sequence after removing `range`, making destination
    /// semantics explicit and avoiding hidden coordinate repair.
    pub fn cut_and_insert(
        &self,
        range: Range<u64>,
        destination_after_cut: u64,
        receipt: &mut MutationReceipt,
    ) -> Result<Self, QueryError> {
        if range.start > range.end || range.end > self.len() {
            return Err(QueryError::RangeOutOfBounds);
        }
        let moved_len = range.end - range.start;
        if destination_after_cut > self.len() - moved_len {
            return Err(QueryError::RangeOutOfBounds);
        }
        let mut next_page_id = self.next_page_id;
        let (before, rest) = split(self.root.clone(), range.start, &mut next_page_id, receipt);
        let (moved, after) = split(rest, moved_len, &mut next_page_id, receipt);
        let without = merge(before, after, receipt);
        let (destination_left, destination_right) =
            split(without, destination_after_cut, &mut next_page_id, receipt);
        let root = merge(
            merge(destination_left, moved, receipt),
            destination_right,
            receipt,
        );
        Ok(Self {
            root,
            next_page_id,
            page_tokens: self.page_tokens,
        })
    }

    #[must_use]
    pub fn tokens(&self) -> Vec<Token> {
        let mut output = Vec::with_capacity(usize::try_from(self.len()).unwrap_or(0));
        collect_tokens(&self.root, &mut output);
        output
    }

    pub fn page_identity_at(&self, token_index: u64) -> Result<PageIdentity, QueryError> {
        if token_index >= self.len() {
            return Err(QueryError::RangeOutOfBounds);
        }
        let mut node = self.root.as_ref().expect("nonempty after bounds check");
        let mut index = token_index;
        loop {
            let left = token_len(&node.left);
            if index < left {
                node = node.left.as_ref().expect("left rank exists");
                continue;
            }
            let page_len = u64::try_from(node.page.len()).expect("page length fits u64");
            if index < left + page_len {
                return Ok(PageIdentity {
                    page_id: node.page_id,
                    allocation: Arc::as_ptr(&node.page) as *const Token as usize,
                    local_index: usize::try_from(index - left).expect("local index fits"),
                });
            }
            index -= left + page_len;
            node = node.right.as_ref().expect("right rank exists");
        }
    }

    /// Finds the matching Exit using only one exact Enter rank and associative
    /// range summaries. This deliberately favors a small proof over an
    /// optimized descent: it is O(log^2 tokens), not a linear scan.
    pub fn matching_exit_exclusive(
        &self,
        enter: u64,
        receipt: &mut QueryReceipt,
    ) -> Result<u64, QueryError> {
        if enter >= self.len() {
            return Err(QueryError::RangeOutOfBounds);
        }
        let mut token_receipt = QueryReceipt::default();
        let token = self.range_summary(enter..enter + 1, &mut token_receipt)?;
        *receipt = QueryReceipt {
            nodes_visited: receipt.nodes_visited + token_receipt.nodes_visited,
            whole_nodes_folded: receipt.whole_nodes_folded + token_receipt.whole_nodes_folded,
            tokens_scanned: receipt.tokens_scanned + token_receipt.tokens_scanned,
        };
        if token.enters != 1 || token.balance != 1 {
            return Err(QueryError::MissingBoundary(BlockId(0)));
        }
        let start = enter + 1;
        let mut full_receipt = QueryReceipt::default();
        let full = self.range_summary(start..self.len(), &mut full_receipt)?;
        receipt.nodes_visited += full_receipt.nodes_visited;
        receipt.whole_nodes_folded += full_receipt.whole_nodes_folded;
        receipt.tokens_scanned += full_receipt.tokens_scanned;
        if full.minimum_prefix >= 0 {
            return Err(QueryError::NotBalancedInterior);
        }
        let mut low = start + 1;
        let mut high = self.len();
        while low < high {
            let middle = low + (high - low) / 2;
            let mut step = QueryReceipt::default();
            let prefix = self.range_summary(start..middle, &mut step)?;
            receipt.nodes_visited += step.nodes_visited;
            receipt.whole_nodes_folded += step.whole_nodes_folded;
            receipt.tokens_scanned += step.tokens_scanned;
            if prefix.minimum_prefix < 0 {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        let mut validation_receipt = QueryReceipt::default();
        let through_exit = self.range_summary(start..low, &mut validation_receipt)?;
        receipt.nodes_visited += validation_receipt.nodes_visited;
        receipt.whole_nodes_folded += validation_receipt.whole_nodes_folded;
        receipt.tokens_scanned += validation_receipt.tokens_scanned;
        if through_exit.balance != -1 || through_exit.minimum_prefix != -1 {
            return Err(QueryError::CorruptSummary);
        }
        Ok(low)
    }

    #[must_use]
    pub fn memory_stats(&self) -> MemoryStats {
        memory_stats_for_roots(std::slice::from_ref(&self.root))
    }

    #[must_use]
    pub fn shared_memory_stats(sequences: &[&Self]) -> MemoryStats {
        let roots = sequences
            .iter()
            .map(|sequence| sequence.root.clone())
            .collect::<Vec<_>>();
        memory_stats_for_roots(&roots)
    }
}

/// Exact partial-parse contract for an open container.
///
/// Finalized direct children may already live as one balanced Euler forest,
/// but an active child has no truthful `ClosedChildSummary` yet.  It remains
/// in the O(depth) parser/open overlay.  No placeholder Enter token is allowed
/// into the persistent sequence, and the finalized container property remains
/// unknown until exact EOF closure or suffix adoption supplies that summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenContainerPrefix {
    pub container: BlockId,
    pub committed_children: Range<u64>,
    pub active_direct_child: Option<BlockId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenProperty<T> {
    Exact(T),
    UnknownActiveChild(BlockId),
}

impl OpenContainerPrefix {
    pub fn committed_summary(
        &self,
        sequence: &EulerSequence,
        receipt: &mut QueryReceipt,
    ) -> Result<ChildSequenceSummary, QueryError> {
        sequence.direct_child_summary(self.committed_children.clone(), receipt)
    }

    pub fn current_property(
        &self,
        sequence: &EulerSequence,
        receipt: &mut QueryReceipt,
    ) -> Result<OpenProperty<ChildSequenceSummary>, QueryError> {
        let committed = self.committed_summary(sequence, receipt)?;
        Ok(match self.active_direct_child {
            Some(block) => OpenProperty::UnknownActiveChild(block),
            None => OpenProperty::Exact(committed),
        })
    }
}

fn build_replacement(
    tokens: &[Token],
    page_tokens: usize,
    next_page_id: &mut u64,
    receipt: &mut MutationReceipt,
) -> Option<Arc<Node>> {
    let mut root = None;
    for chunk in tokens.chunks(page_tokens) {
        let page: Arc<[Token]> = Arc::from(chunk);
        receipt.token_pages_allocated += 1;
        receipt.packed_token_bytes_copied += chunk
            .iter()
            .copied()
            .map(Token::packed_bytes)
            .sum::<usize>();
        let node = Some(Node::new(*next_page_id, page, None, None, receipt));
        *next_page_id += 1;
        root = merge(root, node, receipt);
    }
    root
}

fn range_summary_node(
    node: &Option<Arc<Node>>,
    node_start: u64,
    range: &Range<u64>,
    receipt: &mut QueryReceipt,
) -> EulerSummary {
    let Some(node) = node else {
        return EulerSummary::default();
    };
    receipt.nodes_visited += 1;
    let node_end = node_start + node.summary.tokens;
    if range.start <= node_start && node_end <= range.end {
        receipt.whole_nodes_folded += 1;
        return node.summary;
    }
    if node_end <= range.start || range.end <= node_start {
        return EulerSummary::default();
    }
    let left_len = token_len(&node.left);
    let page_start = node_start + left_len;
    let page_end = page_start + u64::try_from(node.page.len()).expect("page length fits");
    let mut summary = range_summary_node(&node.left, node_start, range, receipt);
    let local_start = usize::try_from(range.start.saturating_sub(page_start))
        .unwrap_or(usize::MAX)
        .min(node.page.len());
    let local_end = usize::try_from(range.end.saturating_sub(page_start))
        .unwrap_or(usize::MAX)
        .min(node.page.len());
    if local_start < local_end && page_start < range.end && range.start < page_end {
        if local_start == 0 && local_end == node.page.len() {
            receipt.whole_nodes_folded += 1;
            summary = summary.followed_by(node.page_summary);
        } else {
            receipt.tokens_scanned += local_end - local_start;
            summary = summary.followed_by(EulerSummary::from_tokens(
                &node.page[local_start..local_end],
            ));
        }
    }
    summary.followed_by(range_summary_node(&node.right, page_end, range, receipt))
}

fn collect_outermost_node(
    node: &Option<Arc<Node>>,
    node_start: u64,
    range: &Range<u64>,
    depth: &mut i64,
    output: &mut Vec<(BlockId, ClosedChildSummary)>,
    receipt: &mut EnumerationReceipt,
) {
    let Some(node) = node else {
        return;
    };
    let node_end = node_start + node.summary.tokens;
    if node_end <= range.start || range.end <= node_start {
        return;
    }
    receipt.nodes_visited += 1;
    if range.start <= node_start && node_end <= range.end {
        match node.summary.minimum_enter_depth {
            None => {
                *depth += node.summary.balance;
                receipt.nodes_skipped += 1;
                return;
            }
            Some(minimum) if *depth + minimum > 0 => {
                *depth += node.summary.balance;
                receipt.nodes_skipped += 1;
                return;
            }
            _ => {}
        }
    }
    let left_len = token_len(&node.left);
    let page_start = node_start + left_len;
    let page_end = page_start + u64::try_from(node.page.len()).expect("page length fits");
    collect_outermost_node(&node.left, node_start, range, depth, output, receipt);
    let local_start = usize::try_from(range.start.saturating_sub(page_start))
        .unwrap_or(usize::MAX)
        .min(node.page.len());
    let local_end = usize::try_from(range.end.saturating_sub(page_start))
        .unwrap_or(usize::MAX)
        .min(node.page.len());
    if local_start < local_end && page_start < range.end && range.start < page_end {
        for token in &node.page[local_start..local_end] {
            receipt.tokens_scanned += 1;
            match *token {
                Token::Enter { block, closed } => {
                    if *depth == 0 {
                        output.push((block, closed));
                    }
                    *depth += 1;
                }
                Token::Exit => *depth -= 1,
            }
        }
    }
    collect_outermost_node(&node.right, page_end, range, depth, output, receipt);
}

fn collect_tokens(node: &Option<Arc<Node>>, output: &mut Vec<Token>) {
    let Some(node) = node else {
        return;
    };
    collect_tokens(&node.left, output);
    output.extend_from_slice(&node.page);
    collect_tokens(&node.right, output);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageIdentity {
    pub page_id: u64,
    pub allocation: usize,
    pub local_index: usize,
}

/// An exact token cursor with no absolute document rank.
///
/// The page allocation remains stable when an unchanged suffix page is reused.
/// Resolving it requires a root-specific page-to-node map and parent edges;
/// that required secondary structure is measured rather than hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StableEnterCursor {
    pub page_allocation: usize,
    pub local_index: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParentSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ParentEdge {
    parent: usize,
    side: ParentSide,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StableDirectoryBuildReceipt {
    pub nodes_visited: usize,
    pub tokens_scanned: usize,
    pub block_locators: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StableDirectoryMemory {
    /// Packed lower bound if this is a standalone BlockId-keyed structure.
    pub standalone_packed_lower_bound: usize,
    /// Packed model when the 10-byte page/slot locator is embedded in the
    /// existing block record and replaces its 8-byte `subtree_last` field.
    pub integrated_record_net_bytes: isize,
    /// Root-specific page/node and parent-edge lower bound. This excludes the
    /// persistent-map branching needed by a real immutable implementation.
    pub root_navigation_lower_bound: usize,
    /// Rust object payload lower bound for this prototype's hash maps. Hash
    /// table control bytes, spare capacity, and allocator metadata are absent.
    pub runtime_map_payload_lower_bound: usize,
}

/// Root-specific correctness model for stable BlockId lookup.
///
/// `build` is intentionally O(document) and therefore not an accepted update
/// path. A production Euler sequence would have to maintain the same facts
/// transactionally from page split/merge deltas in O(changed pages * log n).
/// That extra index is the principal architectural cost exposed by this gate.
#[derive(Clone, Debug, Default)]
pub struct StableBoundaryDirectory {
    blocks: HashMap<BlockId, StableEnterCursor>,
    page_nodes: HashMap<usize, usize>,
    parents: HashMap<usize, ParentEdge>,
    nodes: HashMap<usize, Arc<Node>>,
    root: Option<usize>,
}

impl StableBoundaryDirectory {
    pub fn build(
        sequence: &EulerSequence,
        receipt: &mut StableDirectoryBuildReceipt,
    ) -> Result<Self, QueryError> {
        let mut directory = Self::default();
        let Some(root) = &sequence.root else {
            return Ok(directory);
        };
        let root_pointer = Arc::as_ptr(root) as usize;
        directory.root = Some(root_pointer);
        let mut stack = vec![(root.clone(), None)];
        while let Some((node, parent)) = stack.pop() {
            receipt.nodes_visited += 1;
            let node_pointer = Arc::as_ptr(&node) as usize;
            if let Some(parent) = parent {
                directory.parents.insert(node_pointer, parent);
            }
            let page_pointer = Arc::as_ptr(&node.page) as *const Token as usize;
            if directory
                .page_nodes
                .insert(page_pointer, node_pointer)
                .is_some()
            {
                return Err(QueryError::CorruptSummary);
            }
            for (local, token) in node.page.iter().copied().enumerate() {
                receipt.tokens_scanned += 1;
                if let Token::Enter { block, .. } = token {
                    let cursor = StableEnterCursor {
                        page_allocation: page_pointer,
                        local_index: u32::try_from(local)
                            .map_err(|_| QueryError::CorruptSummary)?,
                    };
                    if directory.blocks.insert(block, cursor).is_some() {
                        return Err(QueryError::DuplicateBlock(block));
                    }
                    receipt.block_locators += 1;
                }
            }
            if let Some(left) = &node.left {
                stack.push((
                    left.clone(),
                    Some(ParentEdge {
                        parent: node_pointer,
                        side: ParentSide::Left,
                    }),
                ));
            }
            if let Some(right) = &node.right {
                stack.push((
                    right.clone(),
                    Some(ParentEdge {
                        parent: node_pointer,
                        side: ParentSide::Right,
                    }),
                ));
            }
            directory.nodes.insert(node_pointer, node);
        }
        Ok(directory)
    }

    pub fn cursor(&self, block: BlockId) -> Result<StableEnterCursor, QueryError> {
        self.blocks
            .get(&block)
            .copied()
            .ok_or(QueryError::MissingBoundary(block))
    }

    pub fn enter_rank(&self, block: BlockId) -> Result<u64, QueryError> {
        let cursor = self.cursor(block)?;
        let mut node_pointer = *self
            .page_nodes
            .get(&cursor.page_allocation)
            .ok_or(QueryError::CorruptSummary)?;
        let node = self
            .nodes
            .get(&node_pointer)
            .ok_or(QueryError::CorruptSummary)?;
        let local = usize::try_from(cursor.local_index).map_err(|_| QueryError::CorruptSummary)?;
        match node.page.get(local) {
            Some(Token::Enter { block: actual, .. }) if *actual == block => {}
            _ => return Err(QueryError::CorruptSummary),
        }
        let mut rank =
            token_len(&node.left) + u64::try_from(local).map_err(|_| QueryError::CorruptSummary)?;
        while let Some(edge) = self.parents.get(&node_pointer).copied() {
            let parent = self
                .nodes
                .get(&edge.parent)
                .ok_or(QueryError::CorruptSummary)?;
            if edge.side == ParentSide::Right {
                rank += token_len(&parent.left)
                    + u64::try_from(parent.page.len()).map_err(|_| QueryError::CorruptSummary)?;
            }
            node_pointer = edge.parent;
        }
        if Some(node_pointer) != self.root {
            return Err(QueryError::CorruptSummary);
        }
        Ok(rank)
    }

    pub fn block_range(
        &self,
        sequence: &EulerSequence,
        block: BlockId,
        receipt: &mut QueryReceipt,
    ) -> Result<Range<u64>, QueryError> {
        let enter = self.enter_rank(block)?;
        let exit_exclusive = sequence.matching_exit_exclusive(enter, receipt)?;
        Ok(enter..exit_exclusive)
    }

    #[must_use]
    pub fn memory(&self) -> StableDirectoryMemory {
        let blocks = self.blocks.len();
        let pages = self.page_nodes.len();
        let nodes = self.nodes.len();
        StableDirectoryMemory {
            // BlockId(8) + logical page id(8) + local slot(2).
            standalone_packed_lower_bound: blocks * 18,
            // The locator can live in BlockRecord; matching parentheses makes
            // its old subtree_last BlockId redundant: 10 - 8 = +2/block.
            integrated_record_net_bytes: isize::try_from(blocks * 2).unwrap_or(isize::MAX),
            // PageId->node (16) and node->parent+side (17). A production
            // persistent map will be larger; this is only a lower bound.
            root_navigation_lower_bound: pages * 16 + nodes.saturating_sub(1) * 17,
            runtime_map_payload_lower_bound: blocks
                * (std::mem::size_of::<BlockId>() + std::mem::size_of::<StableEnterCursor>())
                + pages * (std::mem::size_of::<usize>() * 2)
                + self.parents.len()
                    * (std::mem::size_of::<usize>() + std::mem::size_of::<ParentEdge>())
                + nodes * (std::mem::size_of::<usize>() + std::mem::size_of::<Arc<Node>>()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryStats {
    pub token_pages: usize,
    pub tree_nodes: usize,
    pub tokens: usize,
    pub enters: usize,
    pub packed_token_bytes: usize,
    pub packed_leaf_payload_bytes: usize,
    pub packed_branch_payload_bytes: usize,
    pub packed_payload_bytes: usize,
    pub runtime_node_bytes: usize,
    pub runtime_token_allocation_bytes: usize,
    pub runtime_heap_bytes: usize,
}

fn memory_stats_for_roots(roots: &[Option<Arc<Node>>]) -> MemoryStats {
    let mut seen_nodes = HashSet::new();
    let mut seen_pages = HashSet::new();
    let mut stack = roots
        .iter()
        .filter_map(Clone::clone)
        .collect::<Vec<Arc<Node>>>();
    let mut stats = MemoryStats::default();
    while let Some(node) = stack.pop() {
        let node_pointer = Arc::as_ptr(&node) as usize;
        if !seen_nodes.insert(node_pointer) {
            continue;
        }
        stats.tree_nodes += 1;
        if let Some(left) = &node.left {
            stack.push(left.clone());
        }
        if let Some(right) = &node.right {
            stack.push(right.clone());
        }
        let page_pointer = Arc::as_ptr(&node.page) as *const Token as usize;
        if seen_pages.insert(page_pointer) {
            stats.token_pages += 1;
            stats.tokens += node.page.len();
            for token in node.page.iter().copied() {
                stats.packed_token_bytes += token.packed_bytes();
                stats.enters += usize::from(matches!(token, Token::Enter { .. }));
            }
            // Arc's two counters are included. Allocator metadata is not,
            // and is called out separately in the written receipt.
            stats.runtime_token_allocation_bytes +=
                2 * std::mem::size_of::<usize>() + node.page.len() * std::mem::size_of::<Token>();
        }
    }
    stats.packed_leaf_payload_bytes =
        stats.packed_token_bytes + stats.token_pages * PACKED_TOKEN_PAGE_HEADER_BYTES;
    stats.packed_branch_payload_bytes =
        stats.token_pages.saturating_sub(1) * PACKED_BRANCH_SUMMARY_BYTES;
    stats.packed_payload_bytes =
        stats.packed_leaf_payload_bytes + stats.packed_branch_payload_bytes;
    stats.runtime_node_bytes =
        stats.tree_nodes * (2 * std::mem::size_of::<usize>() + std::mem::size_of::<Node>());
    stats.runtime_heap_bytes = stats.runtime_node_bytes + stats.runtime_token_allocation_bytes;
    stats
}

/// Absolute-rank directory used only as a correctness oracle.
///
/// This is deliberately *not* an accepted production locator: any prefix
/// insertion rebases every suffix rank.  Its packed lower bound is reported so
/// the Euler design cannot hide the unresolved BlockId-to-boundary cost.
#[derive(Clone, Debug, Default)]
pub struct AbsoluteBoundaryOracle {
    boundaries: HashMap<BlockId, Range<u64>>,
}

impl AbsoluteBoundaryOracle {
    pub fn build(tokens: &[Token]) -> Result<Self, QueryError> {
        let mut stack = Vec::new();
        let mut boundaries = HashMap::new();
        for (index, token) in tokens.iter().copied().enumerate() {
            let index = u64::try_from(index).expect("token index fits u64");
            match token {
                Token::Enter { block, .. } => {
                    if boundaries.contains_key(&block)
                        || stack.iter().any(|(candidate, _)| *candidate == block)
                    {
                        return Err(QueryError::DuplicateBlock(block));
                    }
                    stack.push((block, index));
                }
                Token::Exit => {
                    let Some((block, enter)) = stack.pop() else {
                        return Err(QueryError::UnmatchedExit);
                    };
                    boundaries.insert(block, enter..index + 1);
                }
            }
        }
        if let Some((block, _)) = stack.pop() {
            return Err(QueryError::UnclosedBlock(block));
        }
        Ok(Self { boundaries })
    }

    pub fn block_range(&self, block: BlockId) -> Result<Range<u64>, QueryError> {
        self.boundaries
            .get(&block)
            .cloned()
            .ok_or(QueryError::MissingBoundary(block))
    }

    pub fn interior(&self, block: BlockId) -> Result<Range<u64>, QueryError> {
        let range = self.block_range(block)?;
        Ok(range.start + 1..range.end - 1)
    }

    #[must_use]
    pub fn packed_lower_bound_bytes(&self) -> usize {
        // BlockId + Enter rank + Exit-exclusive rank, no hash table overhead.
        self.boundaries.len() * 24
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContainerSemantics {
    pub descends_through_last_child: bool,
    pub is_item: bool,
    pub last_line_blank: bool,
}

impl ContainerSemantics {
    #[must_use]
    pub const fn closed_summary(self, children: ChildSequenceSummary) -> ClosedChildSummary {
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
        ClosedChildSummary {
            ends_blank,
            item_loose_if_nonlast,
            item_loose_if_last,
        }
    }
}

/// Test/oracle tree. It is not part of the proposed persistent representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleTree {
    pub block: BlockId,
    pub closed: ClosedChildSummary,
    pub children: Vec<Self>,
}

impl OracleTree {
    pub fn encode(&self, output: &mut Vec<Token>) {
        output.push(Token::Enter {
            block: self.block,
            closed: self.closed,
        });
        for child in &self.children {
            child.encode(output);
        }
        output.push(Token::Exit);
    }

    #[must_use]
    pub fn encoded(&self) -> Vec<Token> {
        let mut output = Vec::new();
        self.encode(&mut output);
        output
    }

    #[must_use]
    pub fn direct_child_summary(&self) -> ChildSequenceSummary {
        self.children
            .iter()
            .fold(ChildSequenceSummary::default(), |summary, child| {
                summary.followed_by(ChildSequenceSummary::singleton(child.closed))
            })
    }

    pub fn walk<'a>(&'a self, output: &mut Vec<&'a Self>) {
        output.push(self);
        for child in &self.children {
            child.walk(output);
        }
    }
}

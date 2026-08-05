//! Persistent `CommonMark` physical-line summaries for the Crop source root.
//!
//! Crop's pinned summary counts LF and intentionally treats CR only as part of
//! CRLF. `CommonMark` also treats a lone CR as a physical line ending, so the
//! generic restart cursor needs one additional summary. This index retains no
//! source bytes: leaves contain only an associative line summary and cover at
//! most [`LINE_INDEX_LEAF_BYTES`] bytes in the authoritative Crop root.

use std::mem;
use std::ops::Range;
use std::sync::Arc;

use super::CropSnapshotLease;

pub(super) const LINE_INDEX_LEAF_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ContentMetric {
    pub bytes: usize,
    pub utf16: usize,
}

impl ContentMetric {
    fn plus(self, other: Self) -> Self {
        Self {
            bytes: self
                .bytes
                .checked_add(other.bytes)
                .expect("source byte metric cannot overflow usize"),
            utf16: self
                .utf16
                .checked_add(other.utf16)
                .expect("source UTF-16 metric cannot overflow usize"),
        }
    }
}

/// Associative summary for `CRLF | CR | LF` physical line endings.
///
/// `line_breaks` counts CRLF once even when CR and LF live in different
/// leaves. `prefix_content` and `suffix_content` exclude line-ending bytes.
/// `last_completed_content` is the content before the final ending.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CommonMarkLineSummary {
    pub bytes: usize,
    pub utf16: usize,
    pub line_breaks: usize,
    pub first_byte: Option<u8>,
    pub last_byte: Option<u8>,
    pub prefix_content: ContentMetric,
    pub suffix_content: ContentMetric,
    pub last_completed_content: Option<ContentMetric>,
}

impl CommonMarkLineSummary {
    fn from_str(text: &str) -> Self {
        if text.is_empty() {
            return Self::default();
        }

        let bytes = text.as_bytes();
        let mut offset = 0;
        let mut current = ContentMetric::default();
        let mut prefix = None;
        let mut last_completed = None;
        let mut line_breaks = 0usize;

        while offset < bytes.len() {
            let ending_bytes = match bytes[offset] {
                b'\r' if bytes.get(offset + 1) == Some(&b'\n') => 2,
                b'\r' | b'\n' => 1,
                _ => 0,
            };
            if ending_bytes != 0 {
                prefix.get_or_insert(current);
                last_completed = Some(current);
                current = ContentMetric::default();
                line_breaks = line_breaks
                    .checked_add(1)
                    .expect("line count cannot exceed source byte length");
                offset += ending_bytes;
                continue;
            }

            let scalar = text[offset..]
                .chars()
                .next()
                .expect("a valid nonempty UTF-8 suffix has one scalar");
            current.bytes += scalar.len_utf8();
            current.utf16 += scalar.len_utf16();
            offset += scalar.len_utf8();
        }

        Self {
            bytes: text.len(),
            utf16: text.encode_utf16().count(),
            line_breaks,
            first_byte: bytes.first().copied(),
            last_byte: bytes.last().copied(),
            prefix_content: prefix.unwrap_or(current),
            suffix_content: current,
            last_completed_content: last_completed,
        }
    }

    fn combine(self, right: Self) -> Self {
        if self.bytes == 0 {
            return right;
        }
        if right.bytes == 0 {
            return self;
        }

        let cross_crlf = self.last_byte == Some(b'\r') && right.first_byte == Some(b'\n');
        let mut line_breaks = self
            .line_breaks
            .checked_add(right.line_breaks)
            .expect("line count cannot exceed source byte length");
        if cross_crlf {
            line_breaks = line_breaks
                .checked_sub(1)
                .expect("a cross-leaf CRLF contains two local endings");
        }

        let prefix_content = if self.line_breaks == 0 {
            self.suffix_content.plus(right.prefix_content)
        } else {
            self.prefix_content
        };
        let suffix_content = if right.line_breaks == 0 {
            self.suffix_content.plus(right.suffix_content)
        } else {
            right.suffix_content
        };
        let last_completed_content = match right.line_breaks {
            0 => self.last_completed_content,
            1 if cross_crlf => self.last_completed_content,
            1 => Some(
                self.suffix_content.plus(
                    right
                        .last_completed_content
                        .expect("one line ending has completed content"),
                ),
            ),
            _ => right.last_completed_content,
        };

        Self {
            bytes: self
                .bytes
                .checked_add(right.bytes)
                .expect("source byte length cannot overflow usize"),
            utf16: self
                .utf16
                .checked_add(right.utf16)
                .expect("source UTF-16 length cannot overflow usize"),
            line_breaks,
            first_byte: self.first_byte,
            last_byte: right.last_byte,
            prefix_content,
            suffix_content,
            last_completed_content,
        }
    }
}

#[derive(Debug)]
enum LineIndexNode {
    Leaf {
        summary: CommonMarkLineSummary,
    },
    Branch {
        left: Arc<Self>,
        right: Arc<Self>,
        summary: CommonMarkLineSummary,
        height: usize,
        leaves: usize,
        nodes: usize,
    },
}

impl LineIndexNode {
    const fn summary(&self) -> CommonMarkLineSummary {
        match self {
            Self::Leaf { summary } | Self::Branch { summary, .. } => *summary,
        }
    }

    const fn height(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Branch { height, .. } => *height,
        }
    }

    const fn leaves(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Branch { leaves, .. } => *leaves,
        }
    }

    const fn nodes(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Branch { nodes, .. } => *nodes,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CommonMarkLineIndexUpdateReceipt {
    pub old_nodes: usize,
    pub new_nodes: usize,
    pub new_leaves: usize,
    pub new_height: usize,
    pub tree_nodes_visited: usize,
    pub tree_nodes_allocated: usize,
    pub summary_subtrees_reused: usize,
    pub coalescing_edge_nodes_visited: usize,
    pub leaf_coalesces: usize,
    pub replacement_bytes_scanned: usize,
    pub boundary_bytes_scanned: usize,
    pub maximum_boundary_scratch_bytes: usize,
    pub retained_source_roots: usize,
    pub retained_source_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CommonMarkLineIndexQueryReceipt {
    pub tree_nodes_visited: usize,
    pub summary_subtrees_reused: usize,
    pub boundary_bytes_scanned: usize,
    pub maximum_boundary_scratch_bytes: usize,
    pub adjacent_bytes_read: usize,
    pub index_height: usize,
    pub index_leaves: usize,
    pub retained_source_roots: usize,
    pub retained_source_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CommonMarkLineIndexRetention {
    pub summary_nodes: usize,
    pub leaves: usize,
    pub height: usize,
    pub maximum_leaf_bytes: usize,
    pub estimated_summary_payload_bytes: usize,
    pub retained_source_roots: usize,
    pub retained_source_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct IndexWork {
    tree_nodes_visited: usize,
    tree_nodes_allocated: usize,
    summary_subtrees_reused: usize,
    coalescing_edge_nodes_visited: usize,
    leaf_coalesces: usize,
    replacement_bytes_scanned: usize,
    boundary_bytes_scanned: usize,
    maximum_boundary_scratch_bytes: usize,
}

#[derive(Debug)]
pub(super) struct CommonMarkLineIndex {
    root: Option<Arc<LineIndexNode>>,
    last_update: CommonMarkLineIndexUpdateReceipt,
}

impl CommonMarkLineIndex {
    pub(super) fn from_text(text: &str) -> Self {
        let mut work = IndexWork::default();
        let root = build_text_index(text, &mut work);
        let mut index = Self {
            root,
            last_update: CommonMarkLineIndexUpdateReceipt::default(),
        };
        let retention = index.retention();
        index.last_update = CommonMarkLineIndexUpdateReceipt {
            old_nodes: 0,
            new_nodes: retention.summary_nodes,
            new_leaves: retention.leaves,
            new_height: retention.height,
            tree_nodes_visited: 0,
            tree_nodes_allocated: work.tree_nodes_allocated,
            summary_subtrees_reused: 0,
            coalescing_edge_nodes_visited: 0,
            leaf_coalesces: 0,
            replacement_bytes_scanned: text.len(),
            boundary_bytes_scanned: 0,
            maximum_boundary_scratch_bytes: 0,
            retained_source_roots: 0,
            retained_source_bytes: 0,
        };
        index
    }

    pub(super) fn edited(
        &self,
        source: &CropSnapshotLease,
        range: Range<usize>,
        replacement: &str,
    ) -> Self {
        debug_assert!(range.start <= range.end);
        debug_assert!(range.end <= source.len_bytes());
        debug_assert!(source.is_char_boundary(range.start));
        debug_assert!(source.is_char_boundary(range.end));

        let old_nodes = self.root.as_deref().map_or(0, LineIndexNode::nodes);
        let mut work = IndexWork::default();
        let (prefix, after_start) =
            split_index(self.root.clone(), range.start, source, 0, &mut work);
        let (discarded, suffix) =
            split_index(after_start, range.len(), source, range.start, &mut work);
        drop(discarded);
        let replacement_root = build_text_index(replacement, &mut work);
        work.replacement_bytes_scanned = replacement.len();
        let prefix = normalize_right_edge(prefix, &mut work);
        let suffix = normalize_left_edge(suffix, &mut work);
        let root = join_boundary(
            join_boundary(prefix, replacement_root, &mut work),
            suffix,
            &mut work,
        );
        let expected_bytes = source
            .len_bytes()
            .checked_sub(range.len())
            .and_then(|bytes| bytes.checked_add(replacement.len()))
            .expect("validated Crop replacement has a representable byte length");
        debug_assert_eq!(
            root.as_deref().map_or(0, |node| node.summary().bytes),
            expected_bytes
        );

        let new_nodes = root.as_deref().map_or(0, LineIndexNode::nodes);
        let new_leaves = root.as_deref().map_or(0, LineIndexNode::leaves);
        let new_height = root.as_deref().map_or(0, LineIndexNode::height);
        Self {
            root,
            last_update: CommonMarkLineIndexUpdateReceipt {
                old_nodes,
                new_nodes,
                new_leaves,
                new_height,
                tree_nodes_visited: work.tree_nodes_visited,
                tree_nodes_allocated: work.tree_nodes_allocated,
                summary_subtrees_reused: work.summary_subtrees_reused,
                coalescing_edge_nodes_visited: work.coalescing_edge_nodes_visited,
                leaf_coalesces: work.leaf_coalesces,
                replacement_bytes_scanned: work.replacement_bytes_scanned,
                boundary_bytes_scanned: work.boundary_bytes_scanned,
                maximum_boundary_scratch_bytes: work.maximum_boundary_scratch_bytes,
                retained_source_roots: 0,
                retained_source_bytes: 0,
            },
        }
    }

    pub(super) fn prefix_summary(
        &self,
        source: &CropSnapshotLease,
        offset: usize,
    ) -> (CommonMarkLineSummary, CommonMarkLineIndexQueryReceipt) {
        debug_assert!(offset <= source.len_bytes());
        debug_assert!(source.is_char_boundary(offset));
        debug_assert_eq!(
            self.root.as_deref().map_or(0, |node| node.summary().bytes),
            source.len_bytes()
        );
        let mut work = IndexWork::default();
        let summary = query_prefix(self.root.as_ref(), offset, source, 0, &mut work);
        let retention = self.retention();
        (
            summary,
            CommonMarkLineIndexQueryReceipt {
                tree_nodes_visited: work.tree_nodes_visited,
                summary_subtrees_reused: work.summary_subtrees_reused,
                boundary_bytes_scanned: work.boundary_bytes_scanned,
                maximum_boundary_scratch_bytes: work.maximum_boundary_scratch_bytes,
                adjacent_bytes_read: 0,
                index_height: retention.height,
                index_leaves: retention.leaves,
                retained_source_roots: 0,
                retained_source_bytes: 0,
            },
        )
    }

    /// Returns the associative summary for the exact suffix beginning at
    /// `offset` without walking that suffix's source bytes.
    ///
    /// Complete right-hand subtrees are consumed from their summaries. Only
    /// the one index leaf containing `offset` can be copied and rescanned, so
    /// the query is logarithmic in the index plus at most one bounded leaf.
    pub(super) fn suffix_summary(
        &self,
        source: &CropSnapshotLease,
        offset: usize,
    ) -> (CommonMarkLineSummary, CommonMarkLineIndexQueryReceipt) {
        debug_assert!(offset <= source.len_bytes());
        debug_assert!(source.is_char_boundary(offset));
        debug_assert_eq!(
            self.root.as_deref().map_or(0, |node| node.summary().bytes),
            source.len_bytes()
        );
        let mut work = IndexWork::default();
        let summary = query_suffix(self.root.as_ref(), offset, source, 0, &mut work);
        let retention = self.retention();
        (
            summary,
            CommonMarkLineIndexQueryReceipt {
                tree_nodes_visited: work.tree_nodes_visited,
                summary_subtrees_reused: work.summary_subtrees_reused,
                boundary_bytes_scanned: work.boundary_bytes_scanned,
                maximum_boundary_scratch_bytes: work.maximum_boundary_scratch_bytes,
                adjacent_bytes_read: 0,
                index_height: retention.height,
                index_leaves: retention.leaves,
                retained_source_roots: 0,
                retained_source_bytes: 0,
            },
        )
    }

    pub(super) const fn last_update(&self) -> CommonMarkLineIndexUpdateReceipt {
        self.last_update
    }

    pub(super) fn total_bytes(&self) -> usize {
        self.root.as_deref().map_or(0, |node| node.summary().bytes)
    }

    pub(super) fn total_utf16(&self) -> usize {
        self.root.as_deref().map_or(0, |node| node.summary().utf16)
    }

    pub(super) fn retention(&self) -> CommonMarkLineIndexRetention {
        let nodes = self.root.as_deref().map_or(0, LineIndexNode::nodes);
        CommonMarkLineIndexRetention {
            summary_nodes: nodes,
            leaves: self.root.as_deref().map_or(0, LineIndexNode::leaves),
            height: self.root.as_deref().map_or(0, LineIndexNode::height),
            maximum_leaf_bytes: LINE_INDEX_LEAF_BYTES,
            estimated_summary_payload_bytes: nodes.saturating_mul(mem::size_of::<LineIndexNode>()),
            retained_source_roots: 0,
            retained_source_bytes: 0,
        }
    }
}

fn build_text_index(text: &str, work: &mut IndexWork) -> Option<Arc<LineIndexNode>> {
    if text.is_empty() {
        return None;
    }
    let mut leaves = Vec::with_capacity(text.len().div_ceil(LINE_INDEX_LEAF_BYTES));
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + LINE_INDEX_LEAF_BYTES).min(text.len());
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            end = start
                + text[start..]
                    .chars()
                    .next()
                    .expect("a nonempty UTF-8 suffix has one scalar")
                    .len_utf8();
        }
        leaves.push(new_leaf(
            CommonMarkLineSummary::from_str(&text[start..end]),
            work,
        ));
        start = end;
    }
    Some(build_balanced(&leaves, work))
}

fn build_balanced(leaves: &[Arc<LineIndexNode>], work: &mut IndexWork) -> Arc<LineIndexNode> {
    debug_assert!(!leaves.is_empty());
    if leaves.len() == 1 {
        work.summary_subtrees_reused += 1;
        return Arc::clone(&leaves[0]);
    }
    let midpoint = leaves.len() / 2;
    let left = build_balanced(&leaves[..midpoint], work);
    let right = build_balanced(&leaves[midpoint..], work);
    new_branch(left, right, work)
}

fn new_leaf(summary: CommonMarkLineSummary, work: &mut IndexWork) -> Arc<LineIndexNode> {
    debug_assert!(summary.bytes > 0);
    debug_assert!(summary.bytes <= LINE_INDEX_LEAF_BYTES);
    work.tree_nodes_allocated += 1;
    Arc::new(LineIndexNode::Leaf { summary })
}

fn new_branch(
    left: Arc<LineIndexNode>,
    right: Arc<LineIndexNode>,
    work: &mut IndexWork,
) -> Arc<LineIndexNode> {
    work.tree_nodes_allocated += 1;
    Arc::new(LineIndexNode::Branch {
        summary: left.summary().combine(right.summary()),
        height: 1 + left.height().max(right.height()),
        leaves: left
            .leaves()
            .checked_add(right.leaves())
            .expect("leaf count cannot exceed source bytes"),
        nodes: 1usize
            .checked_add(left.nodes())
            .and_then(|nodes| nodes.checked_add(right.nodes()))
            .expect("node count cannot exceed address space"),
        left,
        right,
    })
}

fn join_optional_raw(
    left: Option<Arc<LineIndexNode>>,
    right: Option<Arc<LineIndexNode>>,
    work: &mut IndexWork,
) -> Option<Arc<LineIndexNode>> {
    match (left, right) {
        (None, None) => None,
        (Some(node), None) | (None, Some(node)) => {
            work.summary_subtrees_reused += 1;
            Some(node)
        }
        (Some(left), Some(right)) => Some(join_nodes(left, right, work)),
    }
}

#[derive(Clone, Copy)]
enum EdgeDirection {
    Left,
    Right,
}

fn edge_summaries(
    node: &Arc<LineIndexNode>,
    direction: EdgeDirection,
    summaries: &mut [Option<CommonMarkLineSummary>; 2],
    count: &mut usize,
    work: &mut IndexWork,
) {
    if *count == summaries.len() {
        return;
    }
    work.coalescing_edge_nodes_visited += 1;
    match node.as_ref() {
        LineIndexNode::Leaf { summary } => {
            summaries[*count] = Some(*summary);
            *count += 1;
        }
        LineIndexNode::Branch { left, right, .. } => match direction {
            EdgeDirection::Left => {
                edge_summaries(left, direction, summaries, count, work);
                edge_summaries(right, direction, summaries, count, work);
            }
            EdgeDirection::Right => {
                edge_summaries(right, direction, summaries, count, work);
                edge_summaries(left, direction, summaries, count, work);
            }
        },
    }
}

fn first_two_edge_summaries(
    root: &Arc<LineIndexNode>,
    direction: EdgeDirection,
    work: &mut IndexWork,
) -> [Option<CommonMarkLineSummary>; 2] {
    let mut summaries = [None, None];
    let mut count = 0;
    edge_summaries(root, direction, &mut summaries, &mut count, work);
    summaries
}

fn pop_leftmost(
    root: &Arc<LineIndexNode>,
    work: &mut IndexWork,
) -> (Option<Arc<LineIndexNode>>, CommonMarkLineSummary) {
    work.coalescing_edge_nodes_visited += 1;
    match root.as_ref() {
        LineIndexNode::Leaf { summary } => (None, *summary),
        LineIndexNode::Branch { left, right, .. } => {
            let (left_rest, summary) = pop_leftmost(left, work);
            (
                join_optional_raw(left_rest, Some(Arc::clone(right)), work),
                summary,
            )
        }
    }
}

fn pop_rightmost(
    root: &Arc<LineIndexNode>,
    work: &mut IndexWork,
) -> (Option<Arc<LineIndexNode>>, CommonMarkLineSummary) {
    work.coalescing_edge_nodes_visited += 1;
    match root.as_ref() {
        LineIndexNode::Leaf { summary } => (None, *summary),
        LineIndexNode::Branch { left, right, .. } => {
            let (right_rest, summary) = pop_rightmost(right, work);
            (
                join_optional_raw(Some(Arc::clone(left)), right_rest, work),
                summary,
            )
        }
    }
}

/// Repairs the sole possible underfilled pair created at a split's left edge.
/// Existing interior pairs already satisfy the utilization invariant.
fn normalize_left_edge(
    root: Option<Arc<LineIndexNode>>,
    work: &mut IndexWork,
) -> Option<Arc<LineIndexNode>> {
    let root = root?;
    let [Some(first), Some(second)] = first_two_edge_summaries(&root, EdgeDirection::Left, work)
    else {
        return Some(root);
    };
    if first.bytes + second.bytes > LINE_INDEX_LEAF_BYTES {
        return Some(root);
    }
    let (after_first, popped_first) = pop_leftmost(&root, work);
    let after_first = after_first.expect("two observed leaves remain poppable");
    let (after_second, popped_second) = pop_leftmost(&after_first, work);
    debug_assert_eq!(first, popped_first);
    debug_assert_eq!(second, popped_second);
    work.leaf_coalesces += 1;
    let merged = new_leaf(popped_first.combine(popped_second), work);
    join_optional_raw(Some(merged), after_second, work)
}

/// Repairs the sole possible underfilled pair created at a split's right edge.
fn normalize_right_edge(
    root: Option<Arc<LineIndexNode>>,
    work: &mut IndexWork,
) -> Option<Arc<LineIndexNode>> {
    let root = root?;
    let [Some(last), Some(previous)] = first_two_edge_summaries(&root, EdgeDirection::Right, work)
    else {
        return Some(root);
    };
    if previous.bytes + last.bytes > LINE_INDEX_LEAF_BYTES {
        return Some(root);
    }
    let (before_last, popped_last) = pop_rightmost(&root, work);
    let before_last = before_last.expect("two observed leaves remain poppable");
    let (before_previous, popped_previous) = pop_rightmost(&before_last, work);
    debug_assert_eq!(last, popped_last);
    debug_assert_eq!(previous, popped_previous);
    work.leaf_coalesces += 1;
    let merged = new_leaf(popped_previous.combine(popped_last), work);
    join_optional_raw(before_previous, Some(merged), work)
}

/// Joins two already-normalized trees and coalesces their one new adjacency.
fn join_boundary(
    left: Option<Arc<LineIndexNode>>,
    right: Option<Arc<LineIndexNode>>,
    work: &mut IndexWork,
) -> Option<Arc<LineIndexNode>> {
    let (left, right) = match (left, right) {
        (Some(left), Some(right)) => (left, right),
        (left, right) => return join_optional_raw(left, right, work),
    };
    let left_edge = first_two_edge_summaries(&left, EdgeDirection::Right, work)[0]
        .expect("a nonempty left tree has a right edge");
    let right_edge = first_two_edge_summaries(&right, EdgeDirection::Left, work)[0]
        .expect("a nonempty right tree has a left edge");
    if left_edge.bytes + right_edge.bytes > LINE_INDEX_LEAF_BYTES {
        return join_optional_raw(Some(left), Some(right), work);
    }

    let (left_rest, popped_left) = pop_rightmost(&left, work);
    let (right_rest, popped_right) = pop_leftmost(&right, work);
    debug_assert_eq!(left_edge, popped_left);
    debug_assert_eq!(right_edge, popped_right);
    work.leaf_coalesces += 1;
    let merged = new_leaf(popped_left.combine(popped_right), work);
    join_optional_raw(
        join_optional_raw(left_rest, Some(merged), work),
        right_rest,
        work,
    )
}

fn join_nodes(
    left: Arc<LineIndexNode>,
    right: Arc<LineIndexNode>,
    work: &mut IndexWork,
) -> Arc<LineIndexNode> {
    if left.height() > right.height() + 1 {
        let LineIndexNode::Branch {
            left: left_left,
            right: left_right,
            ..
        } = left.as_ref()
        else {
            unreachable!("an imbalanced left tree cannot be one leaf")
        };
        let joined_right = join_nodes(Arc::clone(left_right), right, work);
        return rebalance(Arc::clone(left_left), joined_right, work);
    }
    if right.height() > left.height() + 1 {
        let LineIndexNode::Branch {
            left: right_left,
            right: right_right,
            ..
        } = right.as_ref()
        else {
            unreachable!("an imbalanced right tree cannot be one leaf")
        };
        let joined_left = join_nodes(left, Arc::clone(right_left), work);
        return rebalance(joined_left, Arc::clone(right_right), work);
    }
    new_branch(left, right, work)
}

fn rebalance(
    left: Arc<LineIndexNode>,
    right: Arc<LineIndexNode>,
    work: &mut IndexWork,
) -> Arc<LineIndexNode> {
    if left.height() > right.height() + 1 {
        let LineIndexNode::Branch {
            left: left_left,
            right: left_right,
            ..
        } = left.as_ref()
        else {
            unreachable!("an imbalanced left tree cannot be one leaf")
        };
        if left_left.height() >= left_right.height() {
            let rotated_right = new_branch(Arc::clone(left_right), right, work);
            return new_branch(Arc::clone(left_left), rotated_right, work);
        }
        let LineIndexNode::Branch {
            left: middle_left,
            right: middle_right,
            ..
        } = left_right.as_ref()
        else {
            unreachable!("a double rotation has a branch middle")
        };
        let rotated_left = new_branch(Arc::clone(left_left), Arc::clone(middle_left), work);
        let rotated_right = new_branch(Arc::clone(middle_right), right, work);
        return new_branch(rotated_left, rotated_right, work);
    }
    if right.height() > left.height() + 1 {
        let LineIndexNode::Branch {
            left: right_left,
            right: right_right,
            ..
        } = right.as_ref()
        else {
            unreachable!("an imbalanced right tree cannot be one leaf")
        };
        if right_right.height() >= right_left.height() {
            let rotated_left = new_branch(left, Arc::clone(right_left), work);
            return new_branch(rotated_left, Arc::clone(right_right), work);
        }
        let LineIndexNode::Branch {
            left: middle_left,
            right: middle_right,
            ..
        } = right_left.as_ref()
        else {
            unreachable!("a double rotation has a branch middle")
        };
        let rotated_left = new_branch(left, Arc::clone(middle_left), work);
        let rotated_right = new_branch(Arc::clone(middle_right), Arc::clone(right_right), work);
        return new_branch(rotated_left, rotated_right, work);
    }
    new_branch(left, right, work)
}

fn split_index(
    root: Option<Arc<LineIndexNode>>,
    offset: usize,
    source: &CropSnapshotLease,
    source_start: usize,
    work: &mut IndexWork,
) -> (Option<Arc<LineIndexNode>>, Option<Arc<LineIndexNode>>) {
    let Some(root) = root else {
        debug_assert_eq!(offset, 0);
        return (None, None);
    };
    let total = root.summary().bytes;
    debug_assert!(offset <= total);
    if offset == 0 {
        work.summary_subtrees_reused += 1;
        return (None, Some(root));
    }
    if offset == total {
        work.summary_subtrees_reused += 1;
        return (Some(root), None);
    }

    work.tree_nodes_visited += 1;
    match root.as_ref() {
        LineIndexNode::Leaf { .. } => {
            debug_assert!(total <= LINE_INDEX_LEAF_BYTES);
            let scratch = source
                .root
                .byte_slice(source_start..source_start + total)
                .to_string();
            debug_assert_eq!(scratch.len(), total);
            debug_assert!(scratch.is_char_boundary(offset));
            work.boundary_bytes_scanned += total;
            work.maximum_boundary_scratch_bytes =
                work.maximum_boundary_scratch_bytes.max(scratch.len());
            let left = new_leaf(CommonMarkLineSummary::from_str(&scratch[..offset]), work);
            let right = new_leaf(CommonMarkLineSummary::from_str(&scratch[offset..]), work);
            (Some(left), Some(right))
        }
        LineIndexNode::Branch { left, right, .. } => {
            let left_bytes = left.summary().bytes;
            match offset.cmp(&left_bytes) {
                std::cmp::Ordering::Less => {
                    let (before, after) =
                        split_index(Some(Arc::clone(left)), offset, source, source_start, work);
                    let after = join_optional_raw(after, Some(Arc::clone(right)), work);
                    (before, after)
                }
                std::cmp::Ordering::Equal => {
                    work.summary_subtrees_reused += 2;
                    (Some(Arc::clone(left)), Some(Arc::clone(right)))
                }
                std::cmp::Ordering::Greater => {
                    let (before, after) = split_index(
                        Some(Arc::clone(right)),
                        offset - left_bytes,
                        source,
                        source_start + left_bytes,
                        work,
                    );
                    let before = join_optional_raw(Some(Arc::clone(left)), before, work);
                    (before, after)
                }
            }
        }
    }
}

fn query_prefix(
    root: Option<&Arc<LineIndexNode>>,
    offset: usize,
    source: &CropSnapshotLease,
    source_start: usize,
    work: &mut IndexWork,
) -> CommonMarkLineSummary {
    let Some(root) = root else {
        debug_assert_eq!(offset, 0);
        return CommonMarkLineSummary::default();
    };
    debug_assert!(offset <= root.summary().bytes);
    if offset == 0 {
        return CommonMarkLineSummary::default();
    }
    if offset == root.summary().bytes {
        work.summary_subtrees_reused += 1;
        return root.summary();
    }

    work.tree_nodes_visited += 1;
    match root.as_ref() {
        LineIndexNode::Leaf { .. } => {
            debug_assert!(offset < root.summary().bytes);
            let scratch = source
                .root
                .byte_slice(source_start..source_start + offset)
                .to_string();
            debug_assert_eq!(scratch.len(), offset);
            work.boundary_bytes_scanned += scratch.len();
            work.maximum_boundary_scratch_bytes =
                work.maximum_boundary_scratch_bytes.max(scratch.len());
            CommonMarkLineSummary::from_str(&scratch)
        }
        LineIndexNode::Branch { left, right, .. } => {
            let left_bytes = left.summary().bytes;
            if offset <= left_bytes {
                query_prefix(Some(left), offset, source, source_start, work)
            } else {
                work.summary_subtrees_reused += 1;
                left.summary().combine(query_prefix(
                    Some(right),
                    offset - left_bytes,
                    source,
                    source_start + left_bytes,
                    work,
                ))
            }
        }
    }
}

fn query_suffix(
    root: Option<&Arc<LineIndexNode>>,
    offset: usize,
    source: &CropSnapshotLease,
    source_start: usize,
    work: &mut IndexWork,
) -> CommonMarkLineSummary {
    let Some(root) = root else {
        debug_assert_eq!(offset, 0);
        return CommonMarkLineSummary::default();
    };
    debug_assert!(offset <= root.summary().bytes);
    if offset == 0 {
        work.summary_subtrees_reused += 1;
        return root.summary();
    }
    if offset == root.summary().bytes {
        return CommonMarkLineSummary::default();
    }

    work.tree_nodes_visited += 1;
    match root.as_ref() {
        LineIndexNode::Leaf { .. } => {
            debug_assert!(offset < root.summary().bytes);
            let source_end = source_start + root.summary().bytes;
            let scratch = source
                .root
                .byte_slice(source_start + offset..source_end)
                .to_string();
            debug_assert_eq!(scratch.len(), root.summary().bytes - offset);
            work.boundary_bytes_scanned += scratch.len();
            work.maximum_boundary_scratch_bytes =
                work.maximum_boundary_scratch_bytes.max(scratch.len());
            CommonMarkLineSummary::from_str(&scratch)
        }
        LineIndexNode::Branch { left, right, .. } => {
            let left_bytes = left.summary().bytes;
            match offset.cmp(&left_bytes) {
                std::cmp::Ordering::Less => {
                    let left_suffix = query_suffix(Some(left), offset, source, source_start, work);
                    work.summary_subtrees_reused += 1;
                    left_suffix.combine(right.summary())
                }
                std::cmp::Ordering::Equal => {
                    work.summary_subtrees_reused += 1;
                    right.summary()
                }
                std::cmp::Ordering::Greater => query_suffix(
                    Some(right),
                    offset - left_bytes,
                    source,
                    source_start + left_bytes,
                    work,
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_leaf_summaries(node: &Arc<LineIndexNode>, leaves: &mut Vec<CommonMarkLineSummary>) {
        match node.as_ref() {
            LineIndexNode::Leaf { summary } => leaves.push(*summary),
            LineIndexNode::Branch { left, right, .. } => {
                collect_leaf_summaries(left, leaves);
                collect_leaf_summaries(right, leaves);
            }
        }
    }

    fn assert_index_invariants(index: &CommonMarkLineIndex) {
        fn walk(node: &Arc<LineIndexNode>) -> CommonMarkLineSummary {
            match node.as_ref() {
                LineIndexNode::Leaf { summary } => {
                    assert!((1..=LINE_INDEX_LEAF_BYTES).contains(&summary.bytes));
                    *summary
                }
                LineIndexNode::Branch {
                    left,
                    right,
                    summary,
                    height,
                    leaves,
                    nodes,
                } => {
                    let left_summary = walk(left);
                    let right_summary = walk(right);
                    assert_eq!(*summary, left_summary.combine(right_summary));
                    assert_eq!(*height, 1 + left.height().max(right.height()));
                    assert_eq!(*leaves, left.leaves() + right.leaves());
                    assert_eq!(*nodes, 1 + left.nodes() + right.nodes());
                    assert!(left.height().abs_diff(right.height()) <= 1);
                    *summary
                }
            }
        }

        let Some(root) = index.root.as_ref() else {
            return;
        };
        let _ = walk(root);
        let mut leaves = Vec::new();
        collect_leaf_summaries(root, &mut leaves);
        for pair in leaves.windows(2) {
            assert!(
                pair[0].bytes + pair[1].bytes > LINE_INDEX_LEAF_BYTES,
                "adjacent leaves must coalesce whenever they fit one bounded page: {leaves:?}"
            );
        }
        assert_eq!(root.nodes(), 2 * root.leaves() - 1);
        let proportional_leaf_ceiling = 2 * root.summary().bytes.div_ceil(LINE_INDEX_LEAF_BYTES);
        assert!(root.leaves() <= proportional_leaf_ceiling.max(1));
    }

    #[test]
    fn line_summary_is_associative_across_every_crlf_partition() {
        let text = "α\r\nβ\rγ\n😀tail\r\n";
        let boundaries: Vec<_> = (0..=text.len())
            .filter(|offset| text.is_char_boundary(*offset))
            .collect();
        let whole = CommonMarkLineSummary::from_str(text);
        for first in boundaries.iter().copied() {
            for second in boundaries.iter().copied().filter(|second| *second >= first) {
                let left = CommonMarkLineSummary::from_str(&text[..first]);
                let middle = CommonMarkLineSummary::from_str(&text[first..second]);
                let right = CommonMarkLineSummary::from_str(&text[second..]);
                assert_eq!(left.combine(middle).combine(right), whole);
                assert_eq!(left.combine(middle.combine(right)), whole);
            }
        }
    }

    #[test]
    fn persistent_index_split_and_join_preserve_exact_summary() {
        let text = format!(
            "{}\r\n{}\r{}\n",
            "a".repeat(5000),
            "😀".repeat(1500),
            "z".repeat(5000)
        );
        let source = CropSnapshotLease::from_text(&text);
        let index = CommonMarkLineIndex::from_text(&text);
        let range = 4095..9002;
        assert!(text.is_char_boundary(range.start));
        assert!(text.is_char_boundary(range.end));
        let replacement = "β\r\nreplacement";
        let edited = index.edited(&source, range.clone(), replacement);
        let mut expected = text;
        expected.replace_range(range, replacement);
        assert_eq!(
            edited.root.as_ref().expect("nonempty").summary(),
            CommonMarkLineSummary::from_str(&expected)
        );
        assert_index_invariants(&edited);
        assert!(edited.retention().height <= 2 * edited.retention().leaves.ilog2() as usize + 3);
        assert_eq!(edited.last_update().retained_source_bytes, 0);
        assert!(edited.last_update().maximum_boundary_scratch_bytes <= LINE_INDEX_LEAF_BYTES);
        assert!(edited.last_update().boundary_bytes_scanned <= 2 * LINE_INDEX_LEAF_BYTES);
    }

    #[test]
    fn hundred_thousand_same_cut_edits_do_not_accumulate_fragment_leaves() {
        let mut text = "a".repeat(2 * LINE_INDEX_LEAF_BYTES);
        let mut source = CropSnapshotLease::from_text(&text);
        let mut index = CommonMarkLineIndex::from_text(&text);
        let cut = 2_000;
        let mut coalesces = 0usize;

        for edit in 0..100_000 {
            let (range, replacement) = if edit % 2 == 0 {
                (cut..cut, "x")
            } else {
                (cut..cut + 1, "")
            };
            let next_index = index.edited(&source, range.clone(), replacement);
            coalesces += next_index.last_update().leaf_coalesces;
            let (next_source, _) = source
                .edit(range.clone(), replacement)
                .expect("the repeated edit remains scalar exact");
            text.replace_range(range, replacement);
            source = next_source;
            index = next_index;

            if edit % 1_000 == 999 {
                assert_index_invariants(&index);
                assert_eq!(source.materialize_for_testing(), text);
                let retention = index.retention();
                let leaf_ceiling = 2 * text.len().div_ceil(LINE_INDEX_LEAF_BYTES);
                assert!(retention.leaves <= leaf_ceiling.max(1));
                assert_eq!(retention.summary_nodes, 2 * retention.leaves - 1);
                assert!(index.last_update().coalescing_edge_nodes_visited > 0);
                assert_eq!(index.last_update().retained_source_bytes, 0);
            }
        }

        assert!(
            coalesces >= 50_000,
            "every insert/delete churn pair repairs at least one local edge"
        );
        assert_index_invariants(&index);
        assert!(index.retention().leaves <= 4);
        assert_eq!(
            index.retention().summary_nodes,
            2 * index.retention().leaves - 1
        );
    }
}

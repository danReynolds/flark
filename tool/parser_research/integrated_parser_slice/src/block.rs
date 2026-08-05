//! Resumable block-to-inline commitment slice.
//!
//! The supported profile is intentionally narrow: blank-separated paragraphs,
//! block quotes, bullet-list items, and lazy paragraph continuation.  The job
//! reads [`PersistentSource`] through its cursor, never materializes a line or
//! document string, and emits the exact [`SegmentedLeaf`] representation used
//! by the shared lexer and grammar job.
//!
//! Source provenance follows the same forward cursor. While a line's block
//! ownership is undecided, at most 256 prefix bytes live in a checkpoint; CRLF
//! lives in a separate two-byte checkpoint. A proven continuation transfers
//! those immutable piece handles into the open leaf, while a context change
//! trims the prefix checkpoint to its first required anchor. Payload is never
//! copied and no leaf performs a random source-tree extraction. Checkpoint and
//! cursor-tree work remain explicit in [`BlockWorkReceipt`]. This does not make
//! the job scheduler-admissible: descriptor/frontier allocation and all work
//! dimensions still need permit preflight.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[cfg(feature = "crop-research")]
use crate::crop_source::{
    CropRangeCapture, CropRangeDescriptor, CropSnapshotLease, CropSourceCursor,
};

use crate::frontier::{FrontierError, LeafId, RetainedBytes, SegmentedLeaf, SegmentedLeafBuilder};
use crate::source::{
    AnchoredByte, CapturedSourceFragment, CertifiedSourceBoundary, CursorStartMetrics,
    PersistentSource, SourceCapture, SourceCaptureError, SourceCaptureMetrics, SourceCursor,
    SourceRootIdentity,
};

/// Maximum work accepted by one block poll.
pub const MAX_BLOCK_POLL_WORK: usize = 4 * 1024;
/// Maximum supported container depth in this commitment slice.
pub const MAX_BLOCK_CONTAINER_DEPTH: usize = 16;
/// Maximum physical prefix retained while classifying a line.
pub const MAX_BLOCK_PREFIX_BYTES: usize = 256;
/// Largest synchronous source-capture checkpoint merge: an undecided prefix
/// plus the longest physical line ending. This is an explicit atomic ceiling,
/// not a scheduler-admission claim.
pub const MAX_BLOCK_CAPTURE_CHECKPOINT_BYTES: usize = MAX_BLOCK_PREFIX_BYTES + 2;
/// CRLF is the largest physical separator held pending until the following
/// line proves that it continues the same leaf.
pub const MAX_BLOCK_SEPARATOR_BYTES: usize = 2;
/// Maximum leaves copied while sealing one bounded output page.
pub const BLOCK_LEAVES_PER_PAGE: usize = 32;
/// Prefix classification has a fixed local ceiling, but aggregate receipt
/// dimensions are not yet preflighted against the caller's transition fuel.
pub const MAX_BLOCK_ATOMIC_PREFIX_UNITS: usize = 4 * 1024;
/// Honest admission flag for the receipt-driven scheduler. A later integration
/// must budget every receipt dimension rather than treating transitions alone
/// as a hard resource slice.
pub const BLOCK_POLL_IS_MEASURED_SCHEDULER_ADMISSIBLE: bool = false;

const MAX_PAGE_TREE_LEVELS: usize = usize::BITS as usize;

static NEXT_BLOCK_LEAF_ID: AtomicU64 = AtomicU64::new(1);

fn mint_leaf_id() -> Result<LeafId, BlockError> {
    NEXT_BLOCK_LEAF_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map(LeafId)
        .map_err(|_| BlockError::IdentityExhausted)
}

/// One supported enclosing block container.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlockContainer {
    #[default]
    BlockQuote,
    BulletItem {
        marker: u8,
        continuation_indent: u8,
    },
}

/// Fixed-size value context retained by one leaf; no container `Vec` is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockContext {
    frames: [BlockContainer; MAX_BLOCK_CONTAINER_DEPTH],
    depth: u8,
}

impl Default for BlockContext {
    fn default() -> Self {
        Self {
            frames: [BlockContainer::BlockQuote; MAX_BLOCK_CONTAINER_DEPTH],
            depth: 0,
        }
    }
}

impl BlockContext {
    #[must_use]
    pub const fn depth(self) -> usize {
        self.depth as usize
    }

    #[must_use]
    pub fn frames(&self) -> &[BlockContainer] {
        &self.frames[..self.depth()]
    }

    fn push(&mut self, frame: BlockContainer, offset: usize) -> Result<(), BlockError> {
        let depth = self.depth();
        if depth == MAX_BLOCK_CONTAINER_DEPTH {
            return Err(BlockError::Unsupported(BlockUnsupported {
                offset,
                feature: UnsupportedFeature::ContainerDepth,
            }));
        }
        self.frames[depth] = frame;
        self.depth += 1;
        Ok(())
    }
}

/// A sealed block leaf, ready for `SharedLexer` and `GrammarJob`.
#[derive(Clone, Debug)]
pub struct BlockLeaf {
    pub id: LeafId,
    pub context: BlockContext,
    pub input: SegmentedLeaf,
    pub physical_start: usize,
    pub physical_end: usize,
}

#[derive(Debug)]
struct BlockLeafPage {
    leaves: Box<[Arc<BlockLeaf>]>,
}

#[derive(Debug)]
enum BlockPageTree {
    Leaf {
        page: Arc<BlockLeafPage>,
        leaves: usize,
    },
    Branch {
        left: Arc<Self>,
        right: Arc<Self>,
        leaves: usize,
    },
}

impl BlockPageTree {
    const fn len(&self) -> usize {
        match self {
            Self::Leaf { leaves, .. } | Self::Branch { leaves, .. } => *leaves,
        }
    }

    fn concat(left: Arc<Self>, right: Arc<Self>) -> Arc<Self> {
        let leaves = left.len() + right.len();
        Arc::new(Self::Branch {
            left,
            right,
            leaves,
        })
    }
}

/// Persistent sealed block output. Cloning this handle does not copy leaves.
#[derive(Clone, Debug)]
pub struct BlockOutput {
    root: Option<Arc<BlockPageTree>>,
    leaf_count: usize,
    source_identity: SourceRootIdentity,
    receipt: BlockWorkReceipt,
}

impl BlockOutput {
    #[must_use]
    pub const fn len(&self) -> usize {
        self.leaf_count
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.leaf_count == 0
    }

    #[must_use]
    pub const fn source_identity(&self) -> SourceRootIdentity {
        self.source_identity
    }

    #[must_use]
    pub const fn receipt(&self) -> BlockWorkReceipt {
        self.receipt
    }

    /// Iterates leaves in source order with a fixed-depth traversal stack.
    #[must_use]
    pub fn leaves(&self) -> BlockLeaves {
        BlockLeaves::new(self.root.clone())
    }
}

/// Allocation-free source-order iterator over the persistent leaf tree.
pub struct BlockLeaves {
    pending: [Option<Arc<BlockPageTree>>; MAX_PAGE_TREE_LEVELS],
    pending_len: usize,
    page: Option<Arc<BlockLeafPage>>,
    page_index: usize,
}

/// One fixed-storage, source-order traversal transition over block leaves.
#[derive(Debug)]
pub enum BlockLeafStep {
    /// One tree/page boundary was advanced; no leaf is available yet.
    Progress,
    /// The next source-order leaf handle was cloned.
    Leaf(Arc<BlockLeaf>),
    /// The persistent output tree is exhausted.
    Done,
}

impl BlockLeaves {
    fn new(root: Option<Arc<BlockPageTree>>) -> Self {
        let mut result = Self {
            pending: std::array::from_fn(|_| None),
            pending_len: 0,
            page: None,
            page_index: 0,
        };
        if let Some(root) = root {
            result.push(root);
        }
        result
    }

    fn push(&mut self, node: Arc<BlockPageTree>) {
        assert!(self.pending_len < MAX_PAGE_TREE_LEVELS);
        self.pending[self.pending_len] = Some(node);
        self.pending_len += 1;
    }

    fn pop(&mut self) -> Option<Arc<BlockPageTree>> {
        if self.pending_len == 0 {
            return None;
        }
        self.pending_len -= 1;
        self.pending[self.pending_len].take()
    }

    /// Advances at most one persistent tree, page-boundary, or leaf-handle
    /// transition. Unlike [`Iterator::next`], this never hides a fixed-depth
    /// tree walk inside one caller operation.
    #[must_use]
    pub fn step(&mut self) -> BlockLeafStep {
        if let Some(page) = &self.page {
            if let Some(leaf) = page.leaves.get(self.page_index) {
                self.page_index += 1;
                return BlockLeafStep::Leaf(leaf.clone());
            }
            self.page = None;
            self.page_index = 0;
            return BlockLeafStep::Progress;
        }
        let Some(node) = self.pop() else {
            return BlockLeafStep::Done;
        };
        match node.as_ref() {
            BlockPageTree::Leaf { page, .. } => self.page = Some(page.clone()),
            BlockPageTree::Branch { left, right, .. } => {
                self.push(right.clone());
                self.push(left.clone());
            }
        }
        BlockLeafStep::Progress
    }
}

impl Iterator for BlockLeaves {
    type Item = Arc<BlockLeaf>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.step() {
                BlockLeafStep::Progress => {}
                BlockLeafStep::Leaf(leaf) => return Some(leaf),
                BlockLeafStep::Done => return None,
            }
        }
    }
}

/// Explicitly unsupported syntax or resource shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedFeature {
    PrefixBeyondBound,
    ContainerDepth,
    IndentedCode,
    OrderedList,
    AtxHeading,
    FenceOrThematicBreak,
    BareCarriageReturn,
}

/// Location and reason for leaving the deliberately limited block profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockUnsupported {
    pub offset: usize,
    pub feature: UnsupportedFeature,
}

/// Construction or profile error. There is deliberately no parser fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockError {
    Unsupported(BlockUnsupported),
    Frontier(FrontierError),
    SourceCapture(SourceCaptureError),
    IdentityExhausted,
}

impl fmt::Display for BlockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(value) => write!(
                formatter,
                "unsupported block profile {:?} at byte {}",
                value.feature, value.offset
            ),
            Self::Frontier(error) => error.fmt(formatter),
            Self::SourceCapture(error) => error.fmt(formatter),
            Self::IdentityExhausted => formatter.write_str("block leaf identity space exhausted"),
        }
    }
}

impl std::error::Error for BlockError {}

impl From<FrontierError> for BlockError {
    fn from(value: FrontierError) -> Self {
        Self::Frontier(value)
    }
}

impl From<SourceCaptureError> for BlockError {
    fn from(value: SourceCaptureError) -> Self {
        Self::SourceCapture(value)
    }
}

/// Auditable work for a complete or failed block job.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockWorkReceipt {
    /// Physical source bytes pulled from the persistent cursor exactly once.
    pub source_bytes_inspected: usize,
    /// Persistent source-tree nodes inspected by cursor positioning,
    /// sequential next-piece descents, boundary-certification fallbacks, and
    /// bounded-fragment extraction.
    pub source_index_nodes_examined: usize,
    /// Sequential piece-edge transitions and the persistent tree nodes needed
    /// to descend to the next immutable piece.
    pub source_piece_transitions: usize,
    pub source_cursor_tree_nodes_descended: usize,
    /// Crop's safe public chunk iterator borrows its Rope. The owned research
    /// adapter instead keeps one reusable chunk scratch alive across polls;
    /// these counters make that extra source-byte copy impossible to hide.
    #[cfg(feature = "crop-research")]
    pub source_chunk_loads: usize,
    #[cfg(feature = "crop-research")]
    pub source_chunk_bytes_copied: usize,
    #[cfg(feature = "crop-research")]
    pub max_atomic_source_chunk_bytes_copied: usize,
    /// Largest cursor construction/next-piece descent performed atomically.
    pub max_atomic_source_cursor_tree_nodes: usize,
    /// Source bytes read only to certify scalar boundaries outside the main
    /// sequential block scan.
    pub source_boundary_bytes_examined: usize,
    /// Source-fragment tree nodes allocated while sealing leaves.
    pub source_fragment_nodes_allocated: usize,
    /// Payload copied into sealed source fragments. The zero-copy capture and
    /// indexed fragment paths both require this to remain zero.
    pub source_fragment_payload_bytes_copied: usize,
    /// Bytes captured and piece-run leaf allocations made during the existing
    /// sequential scan rather than through per-leaf random source seeks.
    pub source_capture_bytes_observed: usize,
    pub source_capture_piece_runs: usize,
    /// Immutable-buffer refcount handle clones performed by capture. This is
    /// allocation-free but may still scale with physical-line checkpoints.
    pub source_capture_buffer_handle_clones: usize,
    /// Handle-level work for bounded undecided-prefix/separator checkpoints.
    /// These bytes were observed only once by the source cursor.
    pub source_capture_checkpoint_bytes_merged: usize,
    pub source_capture_prefix_bytes_discarded: usize,
    pub source_capture_checkpoint_piece_runs: usize,
    pub source_capture_checkpoint_tree_nodes_examined: usize,
    pub max_atomic_source_capture_checkpoint_bytes: usize,
    pub max_atomic_source_capture_checkpoint_piece_runs: usize,
    pub max_atomic_source_capture_checkpoint_tree_nodes: usize,
    pub max_atomic_source_capture_nodes_allocated: usize,
    /// Fuelled state-machine transitions, including EOF and tree composition.
    pub parser_transitions: usize,
    /// Bytes copied into the fixed 256-byte prefix scratch buffer.
    pub prefix_bytes_copied: usize,
    /// Additional whitespace bytes streamed after bounded evidence committed
    /// the line to blank-or-indented-code classification.
    pub streamed_blank_candidate_bytes: usize,
    /// Actual prefix-buffer byte probes performed, including backtracking.
    pub prefix_bytes_examined: usize,
    /// Actual active/new-container frame transitions performed by prefix scans.
    pub prefix_frame_transitions: usize,
    /// Descriptor pushes made into the existing frontier builder.
    pub descriptor_operations: usize,
    /// Virtual bytes represented without source spans.
    pub virtual_bytes_emitted: usize,
    /// Direct allocation requests made by this module (not allocator internals).
    pub block_allocation_requests: usize,
    /// Frontier builder/cursor allocation sites whose request counts are not
    /// observable through their public APIs. This is intentionally nonzero.
    pub unmetered_upstream_allocation_sites: usize,
    /// Exact retained descriptor lower bound accumulated from sealed leaves.
    pub retained_descriptors: RetainedBytes,
    /// Lower-bound requested bytes for `BlockLeaf`, leaf-page handles, and
    /// output-tree nodes. Allocator headers/slack and source storage are absent.
    pub retained_block_structure_bytes: usize,
    /// Retained allocation bodies represented by the preceding lower bound.
    pub retained_block_allocations: usize,
    /// Bounded source-fragment handles retained by sealed leaves. These share
    /// immutable buffers but do not retain the originating whole-source root.
    pub source_fragment_handles_retained: usize,
    pub leaves_sealed: usize,
    pub output_pages_sealed: usize,
    pub output_tree_nodes: usize,
    /// Largest fully metered local prefix-analysis unit count in one tick.
    pub max_atomic_prefix_units: usize,
    /// Largest bounded leaf-page copy performed in one transition.
    pub max_atomic_leaf_handles_copied: usize,
    /// Largest bounded fixed-forest slot scan in one transition.
    pub max_atomic_output_tree_slots: usize,
}

/// Result of one bounded poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockStatus {
    Pending,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockPoll {
    pub status: BlockStatus,
    /// Fuelled state-machine transitions, not a claim about aggregate CPU work.
    pub work: usize,
    /// Counter deltas for this poll plus conservative cumulative atomic
    /// high-water ceilings. Callers must use these dimensions until the block
    /// job adopts the scheduler's permit preflight protocol.
    pub receipt_delta: BlockWorkReceipt,
}

#[derive(Debug)]
struct OpenLeaf {
    id: LeafId,
    context: BlockContext,
    builder: SegmentedLeafBuilder,
    capture: BlockCapture,
    physical_start: usize,
    physical_end: usize,
    lines: usize,
}

#[derive(Debug)]
enum BlockSource {
    Persistent(Arc<PersistentSource>),
    #[cfg(feature = "crop-research")]
    Crop(Arc<CropSnapshotLease>),
}

impl BlockSource {
    fn identity(&self) -> SourceRootIdentity {
        match self {
            Self::Persistent(source) => source.identity(),
            #[cfg(feature = "crop-research")]
            Self::Crop(source) => source.identity(),
        }
    }

    fn cursor(&self) -> (BlockCursor, CursorStartMetrics) {
        match self {
            Self::Persistent(source) => source
                .cursor_at_metered(0)
                .map(|(cursor, metrics)| (BlockCursor::Persistent(cursor), metrics))
                .expect("zero is within every source root"),
            #[cfg(feature = "crop-research")]
            Self::Crop(source) => source
                .cursor_at(0)
                .map(|(cursor, metrics)| (BlockCursor::Crop(cursor), metrics))
                .expect("zero is within every Crop source root"),
        }
    }

    fn builder(&self) -> SegmentedLeafBuilder {
        match self {
            Self::Persistent(source) => SegmentedLeafBuilder::new(source.clone()),
            #[cfg(feature = "crop-research")]
            Self::Crop(source) => SegmentedLeafBuilder::new_crop(source),
        }
    }

    fn capture(&self, start: CertifiedSourceBoundary) -> BlockCapture {
        match self {
            Self::Persistent(_) => BlockCapture::Persistent(SourceCapture::new(start)),
            #[cfg(feature = "crop-research")]
            Self::Crop(_) => BlockCapture::Crop(CropRangeCapture::new(start)),
        }
    }
}

#[derive(Debug)]
enum BlockCursor {
    Persistent(SourceCursor),
    #[cfg(feature = "crop-research")]
    Crop(CropSourceCursor),
}

#[derive(Clone, Copy, Debug, Default)]
struct BlockCursorAdvance {
    piece_transitions: usize,
    tree_nodes_descended: usize,
    #[cfg(feature = "crop-research")]
    chunk_loads: usize,
    #[cfg(feature = "crop-research")]
    chunk_bytes_copied: usize,
}

impl BlockCursor {
    #[cfg(feature = "crop-research")]
    fn crop_chunk_metrics(&self) -> crate::crop_source::CropCursorMetrics {
        match self {
            Self::Persistent(_) => crate::crop_source::CropCursorMetrics::default(),
            Self::Crop(cursor) => cursor.metrics(),
        }
    }

    fn peek_metered(&mut self) -> (Option<AnchoredByte>, BlockCursorAdvance) {
        match self {
            Self::Persistent(cursor) => {
                let (item, advance) = cursor.peek_metered();
                (
                    item,
                    BlockCursorAdvance {
                        piece_transitions: advance.piece_transitions,
                        tree_nodes_descended: advance.tree_nodes_descended,
                        ..BlockCursorAdvance::default()
                    },
                )
            }
            #[cfg(feature = "crop-research")]
            Self::Crop(cursor) => {
                let before = cursor.metrics();
                let (item, advance) = cursor.peek_metered();
                let after = cursor.metrics();
                (
                    item,
                    BlockCursorAdvance {
                        piece_transitions: advance.piece_transitions,
                        tree_nodes_descended: advance.tree_nodes_descended,
                        chunk_loads: after.chunk_loads - before.chunk_loads,
                        chunk_bytes_copied: after.chunk_bytes_copied - before.chunk_bytes_copied,
                    },
                )
            }
        }
    }

    fn next(&mut self) -> Option<AnchoredByte> {
        match self {
            Self::Persistent(cursor) => cursor.next(),
            #[cfg(feature = "crop-research")]
            Self::Crop(cursor) => cursor.next_byte(),
        }
    }

    fn next_captured(
        &mut self,
        capture: &mut BlockCapture,
    ) -> Result<Option<AnchoredByte>, SourceCaptureError> {
        match (self, capture) {
            (Self::Persistent(cursor), BlockCapture::Persistent(capture)) => {
                cursor.next_captured(capture)
            }
            #[cfg(feature = "crop-research")]
            (Self::Crop(cursor), BlockCapture::Crop(capture)) => {
                let offset = cursor.offset();
                let root = cursor.source_identity();
                let item = cursor.next_byte();
                if item.is_some() {
                    capture.observe(root, offset)?;
                }
                Ok(item)
            }
            #[cfg(feature = "crop-research")]
            _ => Err(SourceCaptureError::DifferentSource),
        }
    }

    fn certified_boundary(&self) -> Option<CertifiedSourceBoundary> {
        match self {
            Self::Persistent(cursor) => cursor.certified_boundary(),
            #[cfg(feature = "crop-research")]
            Self::Crop(cursor) => cursor.certified_boundary(),
        }
    }
}

#[derive(Debug)]
// Temporary dual-backend plumbing preserves the existing capture's allocation
// and metering behavior. A selected Crop-only source removes this enum; boxing
// the custom variant here would add one allocation per checkpoint instead.
#[allow(clippy::large_enum_variant)]
enum BlockCapture {
    Persistent(SourceCapture),
    #[cfg(feature = "crop-research")]
    Crop(CropRangeCapture),
}

enum FinishedBlockCapture {
    Persistent(CapturedSourceFragment),
    #[cfg(feature = "crop-research")]
    Crop(CropRangeDescriptor),
}

impl BlockCapture {
    fn certified_start(&self) -> CertifiedSourceBoundary {
        match self {
            Self::Persistent(capture) => capture.certified_start(),
            #[cfg(feature = "crop-research")]
            Self::Crop(capture) => capture.certified_start(),
        }
    }

    fn certified_end(&self) -> CertifiedSourceBoundary {
        match self {
            Self::Persistent(capture) => capture.certified_end(),
            #[cfg(feature = "crop-research")]
            Self::Crop(capture) => capture.certified_end(),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Persistent(capture) => capture.len(),
            #[cfg(feature = "crop-research")]
            Self::Crop(capture) => capture.len(),
        }
    }

    fn metrics(&self) -> SourceCaptureMetrics {
        match self {
            Self::Persistent(capture) => capture.metrics(),
            #[cfg(feature = "crop-research")]
            Self::Crop(capture) => capture.metrics(),
        }
    }

    fn append_bounded(
        &mut self,
        suffix: Self,
        maximum_bytes: usize,
    ) -> Result<(), SourceCaptureError> {
        match (self, suffix) {
            (Self::Persistent(target), Self::Persistent(suffix)) => {
                target.append_bounded(suffix, maximum_bytes)
            }
            #[cfg(feature = "crop-research")]
            (Self::Crop(target), Self::Crop(suffix)) => {
                target.append_bounded(suffix, maximum_bytes)
            }
            #[cfg(feature = "crop-research")]
            _ => Err(SourceCaptureError::DifferentSource),
        }
    }

    fn retain_suffix_bounded(
        self,
        start: CertifiedSourceBoundary,
        maximum_prefix_bytes: usize,
    ) -> Result<Self, SourceCaptureError> {
        match self {
            Self::Persistent(capture) => capture
                .retain_suffix_bounded(start, maximum_prefix_bytes)
                .map(Self::Persistent),
            #[cfg(feature = "crop-research")]
            Self::Crop(capture) => capture
                .retain_suffix_bounded(start, maximum_prefix_bytes)
                .map(Self::Crop),
        }
    }

    fn finish(
        self,
        end: CertifiedSourceBoundary,
    ) -> Result<FinishedBlockCapture, SourceCaptureError> {
        match self {
            Self::Persistent(capture) => capture.finish(end).map(FinishedBlockCapture::Persistent),
            #[cfg(feature = "crop-research")]
            Self::Crop(capture) => capture.finish(end).map(FinishedBlockCapture::Crop),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct LinePlan {
    context: BlockContext,
    content_start: usize,
    virtual_spaces: usize,
    virtual_anchor: usize,
    continuation: bool,
}

#[derive(Clone, Copy, Debug)]
enum PrefixAnalysis {
    NeedMore,
    Blank,
    Ready(LinePlan),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Scan,
    AppendPage,
    FinalizeTree,
    Done,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineCaptureState {
    Undecided,
    Committed,
    /// The bounded checkpoint was released after 256 whitespace bytes. More
    /// whitespace remains blank; a later nonspace is unsupported indented code
    /// in this narrow slice. No provenance from this line can enter a leaf.
    StreamingBlankOrIndented,
}

/// Resumable source-to-block job.
#[derive(Debug)]
pub struct BlockJob {
    source: BlockSource,
    cursor: BlockCursor,
    offset: usize,
    line_start: usize,
    pending_cr: bool,
    prefix: [u8; MAX_BLOCK_PREFIX_BYTES],
    prefix_boundaries: [Option<CertifiedSourceBoundary>; MAX_BLOCK_PREFIX_BYTES + 1],
    prefix_len: usize,
    prefix_all_whitespace: bool,
    plan: Option<LinePlan>,
    /// Source read while the current line's ownership is undecided. Its hard
    /// size ceiling is [`MAX_BLOCK_CAPTURE_CHECKPOINT_BYTES`].
    line_capture: Option<BlockCapture>,
    /// Current CR/LF held outside the leaf until the next line proves a join.
    separator_capture: Option<BlockCapture>,
    /// Previous line ending awaiting the current line's continuation decision.
    pending_join: Option<BlockCapture>,
    line_capture_state: LineCaptureState,
    open: Option<OpenLeaf>,
    leaf_page: Vec<Arc<BlockLeaf>>,
    forest: [Option<Arc<BlockPageTree>>; MAX_PAGE_TREE_LEVELS],
    carry: Option<Arc<BlockPageTree>>,
    carry_level: usize,
    finalize_level: isize,
    final_root: Option<Arc<BlockPageTree>>,
    phase: Phase,
    finalizing: bool,
    result: Option<BlockOutput>,
    error: Option<BlockError>,
    receipt: BlockWorkReceipt,
}

impl BlockJob {
    /// Starts one exact source revision. The only eager allocations are the
    /// source cursor's depth stack and one fixed-capacity 32-handle leaf page.
    ///
    /// # Panics
    ///
    /// Panics only if the persistent source rejects its universally valid zero
    /// boundary, which would violate `PersistentSource`'s public invariant.
    #[must_use]
    pub fn new(source: Arc<PersistentSource>) -> Self {
        Self::new_with_source(BlockSource::Persistent(source))
    }

    /// Starts the same block state machine over one Crop snapshot lease. The
    /// job owns this sole strong lease; sealed leaves retain only root-bound
    /// range descriptors and weak bindings.
    #[cfg(feature = "crop-research")]
    #[must_use]
    pub fn new_crop(source: Arc<CropSnapshotLease>) -> Self {
        Self::new_with_source(BlockSource::Crop(source))
    }

    fn new_with_source(source: BlockSource) -> Self {
        let (cursor, start) = source.cursor();
        #[cfg(feature = "crop-research")]
        let initial_crop = cursor.crop_chunk_metrics();
        let initial_boundary = cursor
            .certified_boundary()
            .expect("complete source cursors certify zero");
        let mut prefix_boundaries = [None; MAX_BLOCK_PREFIX_BYTES + 1];
        prefix_boundaries[0] = Some(initial_boundary);
        Self {
            source,
            cursor,
            offset: 0,
            line_start: 0,
            pending_cr: false,
            prefix: [0; MAX_BLOCK_PREFIX_BYTES],
            prefix_boundaries,
            prefix_len: 0,
            prefix_all_whitespace: true,
            plan: None,
            line_capture: None,
            separator_capture: None,
            pending_join: None,
            line_capture_state: LineCaptureState::Undecided,
            open: None,
            leaf_page: Vec::with_capacity(BLOCK_LEAVES_PER_PAGE),
            forest: std::array::from_fn(|_| None),
            carry: None,
            carry_level: 0,
            finalize_level: MAX_PAGE_TREE_LEVELS.cast_signed() - 1,
            final_root: None,
            phase: Phase::Scan,
            finalizing: false,
            result: None,
            error: None,
            receipt: BlockWorkReceipt {
                source_index_nodes_examined: start.index_nodes_visited,
                max_atomic_source_cursor_tree_nodes: start.index_nodes_visited,
                #[cfg(feature = "crop-research")]
                source_chunk_loads: initial_crop.chunk_loads,
                #[cfg(feature = "crop-research")]
                source_chunk_bytes_copied: initial_crop.chunk_bytes_copied,
                #[cfg(feature = "crop-research")]
                max_atomic_source_chunk_bytes_copied: initial_crop.maximum_chunk_bytes,
                block_allocation_requests: 1,
                // SourceCursor owns one Vec allocation site; each opened leaf
                // invokes frontier builder allocation sites not exposed as a
                // count. The latter are added when the leaf opens.
                unmetered_upstream_allocation_sites: 1,
                ..BlockWorkReceipt::default()
            },
        }
    }

    /// Advances at most `fuel` byte, output-tree, or finalization transitions.
    ///
    /// # Panics
    ///
    /// Panics if fuel is zero or above [`MAX_BLOCK_POLL_WORK`].
    pub fn poll(&mut self, fuel: usize) -> BlockPoll {
        assert!(fuel > 0 && fuel <= MAX_BLOCK_POLL_WORK);
        let before = self.receipt;
        let mut work = 0;
        while work < fuel && !matches!(self.phase, Phase::Done | Phase::Failed) {
            self.tick();
            self.receipt.parser_transitions += 1;
            work += 1;
        }
        BlockPoll {
            status: match self.phase {
                Phase::Done => BlockStatus::Ready,
                Phase::Failed => BlockStatus::Failed,
                Phase::Scan | Phase::AppendPage | Phase::FinalizeTree => BlockStatus::Pending,
            },
            work,
            receipt_delta: receipt_delta(&self.receipt, &before),
        }
    }

    #[must_use]
    pub fn result(&self) -> Option<&BlockOutput> {
        self.result.as_ref()
    }

    #[must_use]
    pub fn error(&self) -> Option<&BlockError> {
        self.error.as_ref()
    }

    #[must_use]
    pub const fn receipt(&self) -> BlockWorkReceipt {
        self.receipt
    }

    fn tick(&mut self) {
        let result = match self.phase {
            Phase::Scan => self.scan_tick(),
            Phase::AppendPage => {
                self.append_page_tick();
                Ok(())
            }
            Phase::FinalizeTree => {
                self.finalize_tree_tick();
                Ok(())
            }
            Phase::Done | Phase::Failed => Ok(()),
        };
        if let Err(error) = result {
            self.discard_all_capture_state();
            self.error = Some(error);
            self.phase = Phase::Failed;
        }
    }

    fn scan_tick(&mut self) -> Result<(), BlockError> {
        let (next, advance) = self.cursor.peek_metered();
        self.charge_cursor_advance(advance);
        let Some(next) = next else {
            return self.finish_source();
        };
        let byte_offset = self.offset;
        let byte_start = self.cursor.certified_boundary();

        if self.pending_cr {
            self.pending_cr = false;
            if next.byte == b'\n' {
                let capture = self
                    .separator_capture
                    .as_mut()
                    .expect("a pending CR owns its separator checkpoint");
                let content_end = capture.certified_start();
                let item = self.cursor.next_captured(capture)?.expect("peeked byte");
                debug_assert_eq!(item.byte, b'\n');
                self.receipt.source_bytes_inspected += 1;
                self.offset += 1;
                let next_line = self
                    .cursor
                    .certified_boundary()
                    .expect("CRLF ends at a scalar boundary");
                return self.finish_line(byte_offset - 1, self.offset, content_end, next_line);
            }
            let _ = self.cursor.next().expect("peeked byte");
            self.receipt.source_bytes_inspected += 1;
            self.offset += 1;
            return Err(BlockError::Unsupported(BlockUnsupported {
                offset: byte_offset - 1,
                feature: UnsupportedFeature::BareCarriageReturn,
            }));
        }

        if matches!(next.byte, b'\r' | b'\n') {
            debug_assert!(self.separator_capture.is_none());
            self.separator_capture = Some(
                self.source
                    .capture(byte_start.expect("physical CR/LF starts at a scalar boundary")),
            );
            let item = self
                .cursor
                .next_captured(
                    self.separator_capture
                        .as_mut()
                        .expect("separator capture was installed"),
                )?
                .expect("peeked byte");
            self.receipt.source_bytes_inspected += 1;
            self.offset += 1;
            if item.byte == b'\r' {
                self.pending_cr = true;
                return Ok(());
            }
            let next_line = self
                .cursor
                .certified_boundary()
                .expect("LF ends at a scalar boundary");
            return self.finish_line(
                byte_offset,
                self.offset,
                byte_start.expect("physical LF starts at a scalar boundary"),
                next_line,
            );
        }

        let item = match self.line_capture_state {
            LineCaptureState::Committed => {
                let open = self
                    .open
                    .as_mut()
                    .expect("a committed line has a capture owner");
                self.cursor.next_captured(&mut open.capture)?
            }
            LineCaptureState::Undecided => {
                let source = &self.source;
                let capture = self.line_capture.get_or_insert_with(|| {
                    source
                        .capture(byte_start.expect("an undecided line starts at a scalar boundary"))
                });
                self.cursor.next_captured(capture)?
            }
            LineCaptureState::StreamingBlankOrIndented => self.cursor.next(),
        }
        .expect("peeked byte");
        self.receipt.source_bytes_inspected += 1;
        self.offset += 1;
        let byte_end = self.cursor.certified_boundary();
        self.observe_line_byte(item.byte, byte_offset, byte_end)?;
        if let Some(plan) = self.plan {
            let has_owned_input = plan.content_start < self.offset || plan.virtual_spaces > 0;
            // Prefix recognition is byte-oriented and can decide after the
            // leading byte of a multi-byte scalar. A source capture may only
            // be sliced or sealed at a scalar boundary, so defer commitment
            // until the same cursor certifies the complete scalar.
            if has_owned_input
                && byte_end.is_some()
                && self.line_capture_state == LineCaptureState::Undecided
            {
                self.commit_line_capture(plan)?;
            }
        } else if self.prefix_len == MAX_BLOCK_PREFIX_BYTES && self.prefix_all_whitespace {
            self.discard_line_checkpoint();
            self.line_capture_state = LineCaptureState::StreamingBlankOrIndented;
        }
        Ok(())
    }

    fn charge_cursor_advance(&mut self, advance: BlockCursorAdvance) {
        self.receipt.source_piece_transitions += advance.piece_transitions;
        self.receipt.source_cursor_tree_nodes_descended += advance.tree_nodes_descended;
        self.receipt.source_index_nodes_examined += advance.tree_nodes_descended;
        self.receipt.max_atomic_source_cursor_tree_nodes = self
            .receipt
            .max_atomic_source_cursor_tree_nodes
            .max(advance.tree_nodes_descended);
        #[cfg(feature = "crop-research")]
        {
            self.receipt.source_chunk_loads += advance.chunk_loads;
            self.receipt.source_chunk_bytes_copied += advance.chunk_bytes_copied;
            self.receipt.max_atomic_source_chunk_bytes_copied = self
                .receipt
                .max_atomic_source_chunk_bytes_copied
                .max(advance.chunk_bytes_copied);
        }
    }

    fn observe_line_byte(
        &mut self,
        byte: u8,
        offset: usize,
        byte_end: Option<CertifiedSourceBoundary>,
    ) -> Result<(), BlockError> {
        if self.line_capture_state == LineCaptureState::StreamingBlankOrIndented {
            if matches!(byte, b' ' | b'\t') {
                self.receipt.streamed_blank_candidate_bytes += 1;
                return Ok(());
            }
            return Err(BlockError::Unsupported(BlockUnsupported {
                offset,
                feature: UnsupportedFeature::IndentedCode,
            }));
        }
        if self.plan.is_some() {
            return Ok(());
        }
        self.prefix_all_whitespace &= matches!(byte, b' ' | b'\t');
        if self.prefix_len == MAX_BLOCK_PREFIX_BYTES {
            return Err(BlockError::Unsupported(BlockUnsupported {
                offset,
                feature: UnsupportedFeature::PrefixBeyondBound,
            }));
        }
        self.prefix[self.prefix_len] = byte;
        self.prefix_len += 1;
        self.prefix_boundaries[self.prefix_len] = byte_end;
        self.receipt.prefix_bytes_copied += 1;
        let mut meter = PrefixMeter::default();
        let analysis = analyze_prefix(
            &self.prefix[..self.prefix_len],
            false,
            self.line_start,
            self.open.as_ref().map(|open| open.context),
            &mut meter,
        );
        self.charge_prefix_meter(meter);
        match analysis? {
            PrefixAnalysis::Ready(plan) => self.plan = Some(plan),
            PrefixAnalysis::NeedMore | PrefixAnalysis::Blank => {}
        }
        Ok(())
    }

    fn finish_line(
        &mut self,
        content_end: usize,
        next_line_start: usize,
        content_end_boundary: CertifiedSourceBoundary,
        next_line_boundary: CertifiedSourceBoundary,
    ) -> Result<(), BlockError> {
        let analysis = if self.line_capture_state == LineCaptureState::StreamingBlankOrIndented {
            PrefixAnalysis::Blank
        } else if let Some(plan) = self.plan {
            PrefixAnalysis::Ready(plan)
        } else if self.prefix_len == MAX_BLOCK_PREFIX_BYTES && self.prefix_all_whitespace {
            PrefixAnalysis::Blank
        } else {
            let mut meter = PrefixMeter::default();
            let analysis = analyze_prefix(
                &self.prefix[..self.prefix_len],
                true,
                self.line_start,
                self.open.as_ref().map(|open| open.context),
                &mut meter,
            );
            self.charge_prefix_meter(meter);
            analysis?
        };
        match analysis {
            PrefixAnalysis::NeedMore => unreachable!("terminated line is decidable"),
            PrefixAnalysis::Blank => {
                self.discard_line_checkpoint();
                self.discard_pending_join();
                self.seal_open_leaf()?;
            }
            PrefixAnalysis::Ready(plan)
                if plan.content_start >= content_end && plan.virtual_spaces == 0 =>
            {
                self.discard_line_checkpoint();
                self.discard_pending_join();
                self.seal_open_leaf()?;
            }
            PrefixAnalysis::Ready(plan) => {
                if self.line_capture_state == LineCaptureState::Undecided {
                    self.commit_line_capture(plan)?;
                }
                self.commit_line(plan, content_end, content_end_boundary)?;
            }
        }
        let separator = self.separator_capture.take();
        if self.open.is_some() {
            debug_assert!(self.pending_join.is_none());
            self.pending_join = separator;
        } else if let Some(separator) = separator {
            self.charge_capture_metrics(separator.metrics());
        }
        self.line_start = next_line_start;
        self.prefix_len = 0;
        self.prefix_boundaries[0] = Some(next_line_boundary);
        self.prefix_all_whitespace = true;
        self.plan = None;
        self.line_capture = None;
        self.line_capture_state = LineCaptureState::Undecided;
        Ok(())
    }

    fn commit_line(
        &mut self,
        plan: LinePlan,
        content_end: usize,
        content_end_boundary: CertifiedSourceBoundary,
    ) -> Result<(), BlockError> {
        let content_start = self.boundary_for_line_offset(plan.content_start)?;
        let virtual_anchor = (plan.virtual_spaces > 0)
            .then(|| self.boundary_for_line_offset(plan.virtual_anchor))
            .transpose()?;
        let open = self.open.as_mut().expect("line has an open leaf");
        if open.lines > 0 {
            open.builder.push_certified_virtual_newline(content_start)?;
            self.receipt.descriptor_operations += 1;
            self.receipt.virtual_bytes_emitted += 1;
        }
        if plan.virtual_spaces > 0 {
            open.builder.push_certified_virtual_tab_spaces(
                virtual_anchor.expect("positive virtual spaces have an anchor"),
                plan.virtual_spaces,
            )?;
            self.receipt.descriptor_operations += 1;
            self.receipt.virtual_bytes_emitted += plan.virtual_spaces;
        }
        if plan.content_start < content_end {
            open.builder
                .push_certified_source(content_start, content_end_boundary)?;
            self.receipt.descriptor_operations += 1;
        }
        open.physical_end = content_end;
        open.lines += 1;
        Ok(())
    }

    fn commit_line_capture(&mut self, plan: LinePlan) -> Result<(), BlockError> {
        let checkpoint = self
            .line_capture
            .take()
            .expect("a non-empty decided line owns a checkpoint");
        assert!(
            checkpoint.len() <= MAX_BLOCK_CAPTURE_CHECKPOINT_BYTES,
            "undecided line checkpoint exceeded its fixed byte ceiling"
        );
        let continues = plan.continuation
            && self
                .open
                .as_ref()
                .is_some_and(|open| open.context == plan.context);
        if continues {
            let open = self.open.as_mut().expect("continuation has an open leaf");
            if let Some(separator) = self.pending_join.take() {
                open.capture
                    .append_bounded(separator, MAX_BLOCK_SEPARATOR_BYTES)?;
            }
            open.capture
                .append_bounded(checkpoint, MAX_BLOCK_CAPTURE_CHECKPOINT_BYTES)?;
        } else {
            self.discard_pending_join();
            self.seal_open_leaf()?;
            let required_start = if plan.virtual_spaces > 0 {
                plan.content_start.min(plan.virtual_anchor)
            } else {
                plan.content_start
            };
            let required_boundary = self.boundary_for_line_offset(required_start)?;
            let checkpoint =
                checkpoint.retain_suffix_bounded(required_boundary, MAX_BLOCK_PREFIX_BYTES)?;
            self.open_leaf(plan.context, checkpoint)?;
        }
        self.line_capture_state = LineCaptureState::Committed;
        Ok(())
    }

    fn boundary_for_line_offset(
        &self,
        absolute: usize,
    ) -> Result<CertifiedSourceBoundary, BlockError> {
        let relative = absolute
            .checked_sub(self.line_start)
            .ok_or(BlockError::Frontier(
                FrontierError::NonMonotonicSourceRange(absolute..absolute),
            ))?;
        self.prefix_boundaries
            .get(relative)
            .copied()
            .flatten()
            .ok_or(BlockError::Frontier(
                FrontierError::SourceBoundarySplitsScalar(absolute),
            ))
    }

    fn open_leaf(
        &mut self,
        context: BlockContext,
        capture: BlockCapture,
    ) -> Result<(), BlockError> {
        let id = mint_leaf_id()?;
        self.open = Some(OpenLeaf {
            id,
            context,
            builder: self.source.builder(),
            capture,
            physical_start: self.line_start,
            physical_end: self.line_start,
            lines: 0,
        });
        // SegmentedLeafBuilder currently has private allocation metrics. Charge
        // its observable allocation site without pretending to know calls.
        self.receipt.unmetered_upstream_allocation_sites += 1;
        Ok(())
    }

    fn seal_open_leaf(&mut self) -> Result<(), BlockError> {
        let Some(open) = self.open.take() else {
            return Ok(());
        };
        let capture_end = open.capture.certified_end();
        let captured = open.capture.finish(capture_end)?;
        let retains_source_fragment = matches!(captured, FinishedBlockCapture::Persistent(_));
        let input = match captured {
            FinishedBlockCapture::Persistent(captured) => {
                open.builder.finish_with_capture(captured)?
            }
            #[cfg(feature = "crop-research")]
            FinishedBlockCapture::Crop(captured) => open.builder.finish_crop(captured)?,
        };
        let construction = input.construction_metrics();
        self.receipt.source_index_nodes_examined += construction.boundary_index_nodes_visited
            + construction
                .fragment_extraction
                .boundary_index_nodes_visited;
        self.receipt.source_boundary_bytes_examined += construction.boundary_bytes_examined
            + construction.fragment_extraction.boundary_bytes_examined;
        self.charge_capture_metrics(construction.sequential_capture);
        let retained = input.retained_descriptor_bytes();
        add_retained(&mut self.receipt.retained_descriptors, retained);
        let leaf = Arc::new(BlockLeaf {
            id: open.id,
            context: open.context,
            input,
            physical_start: open.physical_start,
            physical_end: open.physical_end,
        });
        self.receipt.block_allocation_requests += 1;
        self.receipt.retained_block_structure_bytes +=
            std::mem::size_of::<BlockLeaf>() + 2 * std::mem::size_of::<usize>();
        self.receipt.retained_block_allocations += 1;
        if retains_source_fragment {
            self.receipt.source_fragment_handles_retained += 1;
        }
        self.receipt.leaves_sealed += 1;
        self.leaf_page.push(leaf);
        if self.leaf_page.len() == BLOCK_LEAVES_PER_PAGE {
            self.seal_leaf_page();
        }
        Ok(())
    }

    fn charge_capture_metrics(&mut self, metrics: SourceCaptureMetrics) {
        self.receipt.source_fragment_nodes_allocated += metrics.nodes_allocated;
        self.receipt.source_fragment_payload_bytes_copied += metrics.payload_bytes_copied;
        self.receipt.source_capture_bytes_observed += metrics.bytes_observed;
        self.receipt.source_capture_piece_runs += metrics.piece_runs;
        self.receipt.source_capture_buffer_handle_clones += metrics.buffer_handle_clones;
        self.receipt.source_capture_checkpoint_bytes_merged += metrics.checkpoint_bytes_merged;
        self.receipt.source_capture_prefix_bytes_discarded +=
            metrics.checkpoint_prefix_bytes_discarded;
        self.receipt.source_capture_checkpoint_piece_runs += metrics.checkpoint_piece_runs_merged;
        self.receipt.source_capture_checkpoint_tree_nodes_examined +=
            metrics.checkpoint_tree_nodes_examined;
        self.receipt.max_atomic_source_capture_checkpoint_bytes = self
            .receipt
            .max_atomic_source_capture_checkpoint_bytes
            .max(metrics.max_atomic_checkpoint_bytes);
        self.receipt.max_atomic_source_capture_checkpoint_piece_runs = self
            .receipt
            .max_atomic_source_capture_checkpoint_piece_runs
            .max(metrics.max_atomic_checkpoint_piece_runs);
        self.receipt.max_atomic_source_capture_checkpoint_tree_nodes = self
            .receipt
            .max_atomic_source_capture_checkpoint_tree_nodes
            .max(metrics.max_atomic_checkpoint_tree_nodes);
        self.receipt.max_atomic_source_capture_nodes_allocated = self
            .receipt
            .max_atomic_source_capture_nodes_allocated
            .max(metrics.max_atomic_nodes_allocated);
    }

    fn discard_line_checkpoint(&mut self) {
        if let Some(capture) = self.line_capture.take() {
            self.charge_capture_metrics(capture.metrics());
        }
        self.line_capture_state = LineCaptureState::Undecided;
    }

    fn discard_pending_join(&mut self) {
        if let Some(capture) = self.pending_join.take() {
            self.charge_capture_metrics(capture.metrics());
        }
    }

    fn discard_all_capture_state(&mut self) {
        self.discard_line_checkpoint();
        self.discard_pending_join();
        if let Some(capture) = self.separator_capture.take() {
            self.charge_capture_metrics(capture.metrics());
        }
        if let Some(open) = self.open.take() {
            self.charge_capture_metrics(open.capture.metrics());
        }
    }

    fn seal_leaf_page(&mut self) {
        if self.leaf_page.is_empty() {
            return;
        }
        let next = Vec::with_capacity(BLOCK_LEAVES_PER_PAGE);
        self.receipt.block_allocation_requests += 1;
        let leaves = std::mem::replace(&mut self.leaf_page, next);
        let copied = leaves.len();
        self.receipt.max_atomic_leaf_handles_copied =
            self.receipt.max_atomic_leaf_handles_copied.max(copied);
        let page = Arc::new(BlockLeafPage {
            leaves: leaves.into_boxed_slice(),
        });
        let count = page.leaves.len();
        let tree = Arc::new(BlockPageTree::Leaf {
            page,
            leaves: count,
        });
        self.receipt.block_allocation_requests += 2;
        self.receipt.retained_block_structure_bytes += copied
            * std::mem::size_of::<Arc<BlockLeaf>>()
            + std::mem::size_of::<BlockLeafPage>()
            + std::mem::size_of::<BlockPageTree>()
            + 4 * std::mem::size_of::<usize>();
        self.receipt.retained_block_allocations += 3;
        self.receipt.output_pages_sealed += 1;
        self.receipt.output_tree_nodes += 1;
        self.carry = Some(tree);
        self.carry_level = 0;
        self.phase = Phase::AppendPage;
    }

    fn append_page_tick(&mut self) {
        let carry = self.carry.take().expect("append phase owns a carry");
        if let Some(left) = self.forest[self.carry_level].take() {
            self.carry = Some(BlockPageTree::concat(left, carry));
            self.receipt.block_allocation_requests += 1;
            self.receipt.retained_block_structure_bytes +=
                std::mem::size_of::<BlockPageTree>() + 2 * std::mem::size_of::<usize>();
            self.receipt.retained_block_allocations += 1;
            self.receipt.output_tree_nodes += 1;
            self.carry_level += 1;
            return;
        }
        self.forest[self.carry_level] = Some(carry);
        self.phase = if self.finalizing {
            Phase::FinalizeTree
        } else {
            Phase::Scan
        };
    }

    fn finish_source(&mut self) -> Result<(), BlockError> {
        if self.pending_cr {
            return Err(BlockError::Unsupported(BlockUnsupported {
                offset: self.offset - 1,
                feature: UnsupportedFeature::BareCarriageReturn,
            }));
        }
        let end = self
            .cursor
            .certified_boundary()
            .expect("EOF is a certified source boundary");
        if self.line_start < self.offset || self.prefix_len > 0 || self.plan.is_some() {
            self.finish_line(self.offset, self.offset, end, end)?;
        }
        self.discard_pending_join();
        self.seal_open_leaf()?;
        self.finalizing = true;
        if self.carry.is_none() {
            self.seal_leaf_page();
        }
        if self.carry.is_none() {
            self.phase = Phase::FinalizeTree;
        }
        Ok(())
    }

    fn finalize_tree_tick(&mut self) {
        let mut slots = 0;
        while self.finalize_level >= 0 {
            slots += 1;
            let level = usize::try_from(self.finalize_level).expect("nonnegative level");
            self.finalize_level -= 1;
            if let Some(tree) = self.forest[level].take() {
                self.final_root = Some(match self.final_root.take() {
                    None => tree,
                    Some(left) => {
                        self.receipt.block_allocation_requests += 1;
                        self.receipt.retained_block_structure_bytes +=
                            std::mem::size_of::<BlockPageTree>() + 2 * std::mem::size_of::<usize>();
                        self.receipt.retained_block_allocations += 1;
                        self.receipt.output_tree_nodes += 1;
                        BlockPageTree::concat(left, tree)
                    }
                });
                self.receipt.max_atomic_output_tree_slots =
                    self.receipt.max_atomic_output_tree_slots.max(slots);
                return;
            }
        }
        self.receipt.max_atomic_output_tree_slots =
            self.receipt.max_atomic_output_tree_slots.max(slots);
        self.result = Some(BlockOutput {
            root: self.final_root.take(),
            leaf_count: self.receipt.leaves_sealed,
            source_identity: self.source.identity(),
            receipt: self.receipt,
        });
        self.phase = Phase::Done;
    }

    fn charge_prefix_meter(&mut self, meter: PrefixMeter) {
        assert!(
            meter.byte_units + meter.frame_units <= MAX_BLOCK_ATOMIC_PREFIX_UNITS,
            "fixed prefix classifier exceeded its declared atomic ceiling"
        );
        self.receipt.prefix_bytes_examined += meter.byte_units;
        self.receipt.prefix_frame_transitions += meter.frame_units;
        self.receipt.max_atomic_prefix_units = self
            .receipt
            .max_atomic_prefix_units
            .max(meter.byte_units + meter.frame_units);
    }
}

fn add_retained(target: &mut RetainedBytes, value: RetainedBytes) {
    target.payload += value.payload;
    target.structure += value.structure;
    target.allocations += value.allocations;
}

fn receipt_delta(after: &BlockWorkReceipt, before: &BlockWorkReceipt) -> BlockWorkReceipt {
    BlockWorkReceipt {
        source_bytes_inspected: after.source_bytes_inspected - before.source_bytes_inspected,
        source_index_nodes_examined: after.source_index_nodes_examined
            - before.source_index_nodes_examined,
        source_piece_transitions: after.source_piece_transitions - before.source_piece_transitions,
        source_cursor_tree_nodes_descended: after.source_cursor_tree_nodes_descended
            - before.source_cursor_tree_nodes_descended,
        #[cfg(feature = "crop-research")]
        source_chunk_loads: after.source_chunk_loads - before.source_chunk_loads,
        #[cfg(feature = "crop-research")]
        source_chunk_bytes_copied: after.source_chunk_bytes_copied
            - before.source_chunk_bytes_copied,
        #[cfg(feature = "crop-research")]
        max_atomic_source_chunk_bytes_copied: after.max_atomic_source_chunk_bytes_copied,
        max_atomic_source_cursor_tree_nodes: after.max_atomic_source_cursor_tree_nodes,
        source_boundary_bytes_examined: after.source_boundary_bytes_examined
            - before.source_boundary_bytes_examined,
        source_fragment_nodes_allocated: after.source_fragment_nodes_allocated
            - before.source_fragment_nodes_allocated,
        source_fragment_payload_bytes_copied: after.source_fragment_payload_bytes_copied
            - before.source_fragment_payload_bytes_copied,
        source_capture_bytes_observed: after.source_capture_bytes_observed
            - before.source_capture_bytes_observed,
        source_capture_piece_runs: after.source_capture_piece_runs
            - before.source_capture_piece_runs,
        source_capture_buffer_handle_clones: after.source_capture_buffer_handle_clones
            - before.source_capture_buffer_handle_clones,
        source_capture_checkpoint_bytes_merged: after.source_capture_checkpoint_bytes_merged
            - before.source_capture_checkpoint_bytes_merged,
        source_capture_prefix_bytes_discarded: after.source_capture_prefix_bytes_discarded
            - before.source_capture_prefix_bytes_discarded,
        source_capture_checkpoint_piece_runs: after.source_capture_checkpoint_piece_runs
            - before.source_capture_checkpoint_piece_runs,
        source_capture_checkpoint_tree_nodes_examined: after
            .source_capture_checkpoint_tree_nodes_examined
            - before.source_capture_checkpoint_tree_nodes_examined,
        max_atomic_source_capture_checkpoint_bytes: after
            .max_atomic_source_capture_checkpoint_bytes,
        max_atomic_source_capture_checkpoint_piece_runs: after
            .max_atomic_source_capture_checkpoint_piece_runs,
        max_atomic_source_capture_checkpoint_tree_nodes: after
            .max_atomic_source_capture_checkpoint_tree_nodes,
        max_atomic_source_capture_nodes_allocated: after.max_atomic_source_capture_nodes_allocated,
        parser_transitions: after.parser_transitions - before.parser_transitions,
        prefix_bytes_copied: after.prefix_bytes_copied - before.prefix_bytes_copied,
        streamed_blank_candidate_bytes: after.streamed_blank_candidate_bytes
            - before.streamed_blank_candidate_bytes,
        prefix_bytes_examined: after.prefix_bytes_examined - before.prefix_bytes_examined,
        prefix_frame_transitions: after.prefix_frame_transitions - before.prefix_frame_transitions,
        descriptor_operations: after.descriptor_operations - before.descriptor_operations,
        virtual_bytes_emitted: after.virtual_bytes_emitted - before.virtual_bytes_emitted,
        block_allocation_requests: after.block_allocation_requests
            - before.block_allocation_requests,
        unmetered_upstream_allocation_sites: after.unmetered_upstream_allocation_sites
            - before.unmetered_upstream_allocation_sites,
        retained_descriptors: RetainedBytes {
            payload: after.retained_descriptors.payload - before.retained_descriptors.payload,
            structure: after.retained_descriptors.structure - before.retained_descriptors.structure,
            allocations: after.retained_descriptors.allocations
                - before.retained_descriptors.allocations,
        },
        retained_block_structure_bytes: after.retained_block_structure_bytes
            - before.retained_block_structure_bytes,
        retained_block_allocations: after.retained_block_allocations
            - before.retained_block_allocations,
        source_fragment_handles_retained: after.source_fragment_handles_retained
            - before.source_fragment_handles_retained,
        leaves_sealed: after.leaves_sealed - before.leaves_sealed,
        output_pages_sealed: after.output_pages_sealed - before.output_pages_sealed,
        output_tree_nodes: after.output_tree_nodes - before.output_tree_nodes,
        max_atomic_prefix_units: after.max_atomic_prefix_units,
        max_atomic_leaf_handles_copied: after.max_atomic_leaf_handles_copied,
        max_atomic_output_tree_slots: after.max_atomic_output_tree_slots,
    }
}

#[derive(Clone, Copy)]
struct PrefixCursor<'a> {
    bytes: &'a [u8],
    ix: usize,
    tab_start: usize,
    spaces_remaining: usize,
    residual_tab: Option<usize>,
    terminated: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct PrefixMeter {
    byte_units: usize,
    frame_units: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanControl {
    NeedMore,
}

impl<'a> PrefixCursor<'a> {
    const fn new(bytes: &'a [u8], terminated: bool) -> Self {
        Self {
            bytes,
            ix: 0,
            tab_start: 0,
            spaces_remaining: 0,
            residual_tab: None,
            terminated,
        }
    }

    fn byte(self, meter: &mut PrefixMeter) -> Result<Option<u8>, ScanControl> {
        meter.byte_units += 1;
        if let Some(byte) = self.bytes.get(self.ix) {
            Ok(Some(*byte))
        } else if self.terminated {
            Ok(None)
        } else {
            Err(ScanControl::NeedMore)
        }
    }

    fn scan_ch(&mut self, expected: u8, meter: &mut PrefixMeter) -> Result<bool, ScanControl> {
        if self.byte(meter)? == Some(expected) {
            self.ix += 1;
            self.spaces_remaining = 0;
            self.residual_tab = None;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn scan_space_inner(
        &mut self,
        mut spaces: usize,
        meter: &mut PrefixMeter,
    ) -> Result<usize, ScanControl> {
        meter.frame_units += 1;
        let residual = self.spaces_remaining.min(spaces);
        self.spaces_remaining -= residual;
        spaces -= residual;
        while spaces > 0 {
            match self.byte(meter)? {
                Some(b' ') => {
                    self.ix += 1;
                    spaces -= 1;
                    self.residual_tab = None;
                }
                Some(b'\t') => {
                    let tab_offset = self.ix;
                    let width = 4 - (self.ix - self.tab_start) % 4;
                    self.ix += 1;
                    self.tab_start = self.ix;
                    let consumed = width.min(spaces);
                    spaces -= consumed;
                    self.spaces_remaining = width - consumed;
                    self.residual_tab = (self.spaces_remaining > 0).then_some(tab_offset);
                }
                _ => break,
            }
        }
        Ok(spaces)
    }

    fn scan_space(&mut self, spaces: usize, meter: &mut PrefixMeter) -> Result<bool, ScanControl> {
        Ok(self.scan_space_inner(spaces, meter)? == 0)
    }

    fn scan_space_upto(
        &mut self,
        spaces: usize,
        meter: &mut PrefixMeter,
    ) -> Result<usize, ScanControl> {
        Ok(spaces - self.scan_space_inner(spaces, meter)?)
    }

    fn blank_from(mut self, meter: &mut PrefixMeter) -> Result<bool, ScanControl> {
        self.spaces_remaining = 0;
        loop {
            match self.byte(meter)? {
                Some(b' ' | b'\t') => self.ix += 1,
                Some(_) => return Ok(false),
                None => return Ok(true),
            }
        }
    }
}

fn analyze_prefix(
    bytes: &[u8],
    terminated: bool,
    line_start: usize,
    active: Option<BlockContext>,
    meter: &mut PrefixMeter,
) -> Result<PrefixAnalysis, BlockError> {
    match analyze_prefix_inner(bytes, terminated, line_start, active, meter) {
        Ok(value) => Ok(value),
        Err(ScanControlOrBlock::NeedMore) => Ok(PrefixAnalysis::NeedMore),
        Err(ScanControlOrBlock::Block(error)) => Err(error),
    }
}

fn analyze_prefix_inner(
    bytes: &[u8],
    terminated: bool,
    line_start: usize,
    active: Option<BlockContext>,
    meter: &mut PrefixMeter,
) -> Result<PrefixAnalysis, ScanControlOrBlock> {
    let mut cursor = PrefixCursor::new(bytes, terminated);
    let mut matched = 0;
    if let Some(context) = active {
        for frame in context.frames() {
            meter.frame_units += 1;
            let saved = cursor;
            let matched_frame = match *frame {
                BlockContainer::BlockQuote => {
                    let _ = cursor.scan_space_upto(3, meter)?;
                    if cursor.scan_ch(b'>', meter)? {
                        let _ = cursor.scan_space(1, meter)?;
                        true
                    } else {
                        false
                    }
                }
                BlockContainer::BulletItem {
                    continuation_indent,
                    ..
                } => cursor.scan_space(usize::from(continuation_indent), meter)?,
            };
            if !matched_frame {
                cursor = saved;
                break;
            }
            matched += 1;
        }
    }

    if cursor.blank_from(meter)? {
        return Ok(PrefixAnalysis::Blank);
    }

    let active_context = active.unwrap_or_default();
    let unmatched = matched < active_context.depth();
    if unmatched {
        if starts_supported_container(cursor, meter)? {
            cursor = PrefixCursor::new(bytes, terminated);
            return parse_new_line(cursor, BlockContext::default(), false, line_start, meter);
        }
        reject_known_unsupported(cursor, line_start, meter)?;
        let _ = cursor.scan_space_upto(3, meter)?;
        return Ok(PrefixAnalysis::Ready(plan_from_cursor(
            cursor,
            active_context,
            true,
            line_start,
        )));
    }

    parse_new_line(cursor, active_context, active.is_some(), line_start, meter)
}

#[derive(Debug)]
enum ScanControlOrBlock {
    NeedMore,
    Block(BlockError),
}

impl From<ScanControl> for ScanControlOrBlock {
    fn from(_: ScanControl) -> Self {
        Self::NeedMore
    }
}

fn unsupported(offset: usize, feature: UnsupportedFeature) -> ScanControlOrBlock {
    ScanControlOrBlock::Block(BlockError::Unsupported(BlockUnsupported {
        offset,
        feature,
    }))
}

impl From<ScanControlOrBlock> for BlockError {
    fn from(value: ScanControlOrBlock) -> Self {
        match value {
            ScanControlOrBlock::NeedMore => unreachable!(),
            ScanControlOrBlock::Block(error) => error,
        }
    }
}

fn parse_new_line(
    mut cursor: PrefixCursor<'_>,
    mut context: BlockContext,
    continuing_context: bool,
    line_start: usize,
    meter: &mut PrefixMeter,
) -> Result<PrefixAnalysis, ScanControlOrBlock> {
    let initial_depth = context.depth();
    loop {
        meter.frame_units += 1;
        let saved = cursor;
        let _ = cursor.scan_space_upto(3, meter)?;
        if cursor.scan_ch(b'>', meter)? {
            let marker_offset = line_start + cursor.ix - 1;
            let _ = cursor.scan_space(1, meter)?;
            context
                .push(BlockContainer::BlockQuote, marker_offset)
                .map_err(ScanControlOrBlock::Block)?;
            continue;
        }
        cursor = saved;
        let outer = cursor.scan_space_upto(3, meter)?;
        let marker_offset = line_start + cursor.ix;
        let Some(marker @ (b'-' | b'+' | b'*')) = cursor.byte(meter)? else {
            cursor = saved;
            break;
        };
        cursor.ix += 1;
        let marker_at_eol = cursor.byte(meter)?.is_none();
        if !marker_at_eol && !cursor.scan_space(1, meter)? {
            cursor = saved;
            break;
        }
        let extra = if marker_at_eol {
            0
        } else {
            cursor.scan_space_upto(3, meter)?
        };
        let indent = outer + 2 + extra;
        let continuation_indent = u8::try_from(indent)
            .map_err(|_| unsupported(marker_offset, UnsupportedFeature::ContainerDepth))?;
        context
            .push(
                BlockContainer::BulletItem {
                    marker,
                    continuation_indent,
                },
                marker_offset,
            )
            .map_err(ScanControlOrBlock::Block)?;
    }

    if cursor.blank_from(meter)? {
        return Ok(PrefixAnalysis::Blank);
    }
    reject_known_unsupported(cursor, line_start, meter)?;
    let indent = cursor.scan_space_upto(4, meter)?;
    if indent == 4 {
        return Err(unsupported(
            line_start + cursor.ix,
            UnsupportedFeature::IndentedCode,
        ));
    }
    let added_container = context.depth() > initial_depth;
    Ok(PrefixAnalysis::Ready(plan_from_cursor(
        cursor,
        context,
        continuing_context && !added_container,
        line_start,
    )))
}

fn starts_supported_container(
    mut cursor: PrefixCursor<'_>,
    meter: &mut PrefixMeter,
) -> Result<bool, ScanControl> {
    let _ = cursor.scan_space_upto(3, meter)?;
    if cursor.byte(meter)? == Some(b'>') {
        return Ok(true);
    }
    let Some(marker @ (b'-' | b'+' | b'*')) = cursor.byte(meter)? else {
        return Ok(false);
    };
    cursor.ix += 1;
    let follows_space = cursor.byte(meter)?.is_none() || cursor.scan_space(1, meter)?;
    let _ = marker;
    Ok(follows_space)
}

fn reject_known_unsupported(
    mut cursor: PrefixCursor<'_>,
    line_start: usize,
    meter: &mut PrefixMeter,
) -> Result<(), ScanControlOrBlock> {
    let _ = cursor.scan_space_upto(3, meter)?;
    let offset = line_start + cursor.ix;
    let Some(first) = cursor.byte(meter)? else {
        return Ok(());
    };
    if first == b'#' {
        cursor.ix += 1;
        if matches!(cursor.byte(meter)?, None | Some(b' ' | b'\t')) {
            return Err(unsupported(offset, UnsupportedFeature::AtxHeading));
        }
    }
    if matches!(first, b'`' | b'~' | b'-' | b'*' | b'_') {
        meter.byte_units += 3;
        if cursor.bytes.get(cursor.ix..cursor.ix + 3) == Some(&[first, first, first]) {
            return Err(unsupported(
                offset,
                UnsupportedFeature::FenceOrThematicBreak,
            ));
        }
        if cursor.ix + 3 > cursor.bytes.len() && !cursor.terminated {
            return Err(ScanControlOrBlock::NeedMore);
        }
    }
    if first.is_ascii_digit() {
        let mut digits = 0;
        while digits < 10 {
            meter.byte_units += 1;
            if !cursor
                .bytes
                .get(cursor.ix + digits)
                .is_some_and(u8::is_ascii_digit)
            {
                break;
            }
            digits += 1;
        }
        meter.byte_units += 1;
        let delimiter = cursor.bytes.get(cursor.ix + digits).copied();
        if delimiter.is_none() && !cursor.terminated && digits < 10 {
            return Err(ScanControlOrBlock::NeedMore);
        }
        if matches!(delimiter, Some(b'.' | b')')) {
            meter.byte_units += 1;
            let after = cursor.bytes.get(cursor.ix + digits + 1).copied();
            if after.is_none() && !cursor.terminated {
                return Err(ScanControlOrBlock::NeedMore);
            }
            if after.is_none_or(|byte| matches!(byte, b' ' | b'\t')) {
                return Err(unsupported(offset, UnsupportedFeature::OrderedList));
            }
        }
    }
    Ok(())
}

fn plan_from_cursor(
    cursor: PrefixCursor<'_>,
    context: BlockContext,
    continuation: bool,
    line_start: usize,
) -> LinePlan {
    LinePlan {
        context,
        content_start: line_start + cursor.ix,
        virtual_spaces: cursor.spaces_remaining,
        virtual_anchor: line_start + cursor.residual_tab.unwrap_or(cursor.ix),
        continuation,
    }
}

//! Persistent UTF-8 source storage for the integrated parser slice.
//!
//! This module deliberately stops at source ownership. It does not scan
//! Markdown. Immutable buffers give bytes stable [`Anchor`]s, while an AVL
//! piece tree supplies byte and line-break metrics. Splits reuse old buffers;
//! bounded edit-boundary compaction may deliberately copy at most two pieces
//! so tiny edits do not leave one buffer and tree leaf per keystroke.

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Maximum number of UTF-8 bytes referenced by one piece-tree leaf.
pub const MAX_PIECE_BYTES: usize = 4 * 1024;

static NEXT_BUFFER_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_SOURCE_ROOT_ID: AtomicU64 = AtomicU64::new(1);

fn mint_monotonic(counter: &AtomicU64, name: &str) -> u64 {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .unwrap_or_else(|_| panic!("{name} identity space exhausted"))
}

/// Exact identity of one immutable source root.
///
/// Equal content does not imply equal identity. Clones retain this value and
/// every construction or edit mints a new one without hashing or pointer IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceRootIdentity(pub u64);

impl SourceRootIdentity {
    pub(crate) fn mint() -> Self {
        Self(mint_monotonic(&NEXT_SOURCE_ROOT_ID, "source root"))
    }
}

/// Identity of one immutable source buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferId(pub u64);

impl BufferId {
    /// Research-only synthetic anchor namespace for source backends that do
    /// not expose stable leaf allocation identities. Exact cross-revision
    /// reuse for those backends must use edit provenance, never this value.
    #[cfg(feature = "crop-research")]
    pub(crate) const fn from_crop_root(root: SourceRootIdentity) -> Self {
        Self(root.0 ^ (1_u64 << 63))
    }
}

/// A stable byte position in an immutable buffer.
///
/// Document byte offsets shift after edits. An anchor does not: unchanged
/// bytes keep the same buffer identity and buffer-relative offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Anchor {
    pub buffer_id: BufferId,
    pub offset: usize,
}

/// One byte returned by a non-flattening source cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnchoredByte {
    pub byte: u8,
    pub anchor: Anchor,
}

#[derive(Debug)]
struct Buffer {
    id: BufferId,
    text: Arc<str>,
    compactable: bool,
}

impl Buffer {
    fn copy_from(text: &str, compactable: bool, allocations: &mut AllocationMetrics) -> Arc<Self> {
        debug_assert!(!text.is_empty());
        debug_assert!(text.len() <= MAX_PIECE_BYTES);
        allocations.new_buffers += 1;
        allocations.immutable_bytes_copied += text.len();
        allocations.copied_bytes += text.len();
        Arc::new(Self {
            id: BufferId(mint_monotonic(&NEXT_BUFFER_ID, "source buffer")),
            text: Arc::from(text),
            compactable,
        })
    }
}

#[derive(Clone, Debug)]
struct Piece {
    buffer: Arc<Buffer>,
    range: Range<usize>,
    line_breaks: usize,
}

impl Piece {
    fn new(buffer: Arc<Buffer>, range: Range<usize>) -> Self {
        debug_assert!(range.start < range.end);
        debug_assert!(range.end <= buffer.text.len());
        debug_assert!(buffer.text.is_char_boundary(range.start));
        debug_assert!(buffer.text.is_char_boundary(range.end));
        debug_assert!(range.len() <= MAX_PIECE_BYTES);
        let line_breaks = count_line_breaks(&buffer.text.as_bytes()[range.clone()]);
        Self {
            buffer,
            range,
            line_breaks,
        }
    }

    fn len(&self) -> usize {
        self.range.len()
    }

    fn byte(&self, offset: usize) -> AnchoredByte {
        let buffer_offset = self.range.start + offset;
        AnchoredByte {
            byte: self.buffer.text.as_bytes()[buffer_offset],
            anchor: Anchor {
                buffer_id: self.buffer.id,
                offset: buffer_offset,
            },
        }
    }

    fn text(&self) -> &str {
        &self.buffer.text[self.range.clone()]
    }

    fn is_boundary_compactable(&self) -> bool {
        self.buffer.compactable || self.range.start != 0 || self.range.end != self.buffer.text.len()
    }
}

type Link = Option<Arc<Node>>;

#[derive(Debug)]
enum Node {
    Leaf {
        piece: Piece,
        metrics: NodeMetrics,
    },
    Branch {
        left: Arc<Node>,
        right: Arc<Node>,
        metrics: NodeMetrics,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct NodeMetrics {
    bytes: usize,
    line_breaks: usize,
    pieces: usize,
    depth: usize,
    max_piece_bytes: usize,
}

impl Node {
    fn metrics(&self) -> NodeMetrics {
        match self {
            Self::Leaf { metrics, .. } | Self::Branch { metrics, .. } => *metrics,
        }
    }

    fn len(&self) -> usize {
        self.metrics().bytes
    }

    fn depth(&self) -> usize {
        self.metrics().depth
    }
}

/// Observable tree metrics. They are computed incrementally in every node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TreeMetrics {
    pub bytes: usize,
    pub line_breaks: usize,
    pub pieces: usize,
    pub depth: usize,
    pub max_piece_bytes: usize,
}

impl From<NodeMetrics> for TreeMetrics {
    fn from(value: NodeMetrics) -> Self {
        Self {
            bytes: value.bytes,
            line_breaks: value.line_breaks,
            pieces: value.pieces,
            depth: value.depth,
            max_piece_bytes: value.max_piece_bytes,
        }
    }
}

/// Allocations attributable to one construction or edit.
///
/// `copied_bytes` counts both explicit payload-copy stages: first into the
/// bounded page builder, then into immutable `Arc<str>` storage. It includes
/// replacement bytes and explicitly compacted old boundary bytes; it excludes
/// piece descriptors, allocator-internal movement, and test materialization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AllocationMetrics {
    pub new_nodes: usize,
    pub new_buffers: usize,
    /// Bytes copied from input fragments into the bounded staging page.
    pub staged_bytes_copied: usize,
    /// Bytes copied from a staging page into immutable buffer storage.
    pub immutable_bytes_copied: usize,
    /// Sum of every explicit payload-byte copy above.
    pub copied_bytes: usize,
}

/// Exact retained immutable-buffer diagnostics for one source or fragment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BufferRetentionMetrics {
    /// Distinct [`BufferId`] values reachable from the root.
    pub unique_buffers: usize,
    /// Sum of complete reachable buffer allocations, deduplicated by ID.
    pub retained_buffer_bytes: usize,
    /// Bytes referenced by live piece ranges.
    pub referenced_piece_bytes: usize,
    /// Retained buffer bytes outside live piece ranges.
    pub unreferenced_retained_bytes: usize,
    /// Largest independently retained immutable buffer.
    pub max_buffer_bytes: usize,
}

/// One deduplicatable immutable-buffer allocation for root-set audits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RetainedBufferAllocation {
    pub id: BufferId,
    pub bytes: usize,
}

/// Index work performed while positioning one source cursor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CursorStartMetrics {
    pub index_nodes_visited: usize,
}

/// Work needed to move a sequential cursor across one immutable piece edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CursorAdvanceMetrics {
    pub piece_transitions: usize,
    pub tree_nodes_descended: usize,
}

/// A scalar-safe position certified by a sequential source cursor.
///
/// The fields are private deliberately: downstream builders may trust this
/// capability without repeating an O(log document) source lookup.  A
/// certificate can only be obtained while a cursor is positioned at a UTF-8
/// boundary in the exact source revision that minted it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CertifiedSourceBoundary {
    source: SourceRootIdentity,
    offset: usize,
}

impl CertifiedSourceBoundary {
    #[cfg(feature = "crop-research")]
    pub(crate) const fn for_backend(source: SourceRootIdentity, offset: usize) -> Self {
        Self { source, offset }
    }

    /// Revision-local byte position of this boundary.
    #[must_use]
    pub const fn offset(self) -> usize {
        self.offset
    }

    /// Exact source revision that certified the boundary.
    #[must_use]
    pub const fn source_identity(self) -> SourceRootIdentity {
        self.source
    }
}

/// Work and retained structure produced by a sequential source capture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceCaptureMetrics {
    /// Source bytes observed once through the caller's existing cursor pass.
    pub bytes_observed: usize,
    /// Piece-run leaf allocations made while building the capture. A bounded
    /// checkpoint merge may make an old checkpoint leaf transient and retain
    /// a replacement in the destination; both real allocation requests are
    /// counted.
    pub piece_runs: usize,
    /// Tree nodes allocated to make the bounded fragment independently owned.
    pub nodes_allocated: usize,
    /// Payload bytes copied while capturing. This is always zero: captured
    /// pieces retain immutable buffer handles.
    pub payload_bytes_copied: usize,
    /// `Arc<Buffer>` handle clones performed while a new checkpoint observes
    /// a piece run or while an already-sealed checkpoint tree is traversed.
    /// This is not a heap allocation, but it is visible refcount work.
    pub buffer_handle_clones: usize,
    /// Bytes transferred from bounded sequential checkpoints. They were
    /// observed exactly once by the source cursor; this counts handle-level
    /// transfer work, not another source-byte read.
    pub checkpoint_bytes_merged: usize,
    /// Immutable piece-run handles traversed while merging checkpoints.
    pub checkpoint_piece_runs_merged: usize,
    /// Prefix bytes intentionally released from a bounded checkpoint after a
    /// line is classified. They were inspected once but are not retained by
    /// the newly opened leaf.
    pub checkpoint_prefix_bytes_discarded: usize,
    /// Existing capture-tree nodes traversed while merging checkpoints.
    pub checkpoint_tree_nodes_examined: usize,
    /// Largest checkpoint merge performed by one caller transition.
    pub max_atomic_checkpoint_bytes: usize,
    pub max_atomic_checkpoint_piece_runs: usize,
    pub max_atomic_checkpoint_tree_nodes: usize,
    /// Largest source-fragment node-allocation batch performed by one capture
    /// operation. The fixed forest bounds this by the machine word width plus
    /// its final fixed-forest fold.
    pub max_atomic_nodes_allocated: usize,
}

/// Explicit cost of a random-access bounded-fragment extraction.
///
/// This fallback is exact, but one invocation performs tree-index work. A
/// full block scan should use [`SourceCapture`] instead so leaf construction
/// remains linear in the already-inspected source rather than
/// O(leaves * log document).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FragmentExtractionMetrics {
    pub boundary_index_nodes_visited: usize,
    pub boundary_bytes_examined: usize,
    pub structural_nodes_allocated: usize,
    pub payload_bytes_copied: usize,
}

/// Misuse rejected by the sequential capture protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceCaptureError {
    DifferentSource,
    NonContiguous { expected: usize, actual: usize },
    EndBoundaryMismatch { expected: usize, actual: usize },
    CheckpointBeyondBound { bytes: usize, maximum: usize },
}

impl fmt::Display for SourceCaptureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DifferentSource => {
                formatter.write_str("capture and cursor use different source revisions")
            }
            Self::NonContiguous { expected, actual } => write!(
                formatter,
                "capture expected source byte {expected}, cursor is at {actual}"
            ),
            Self::EndBoundaryMismatch { expected, actual } => write!(
                formatter,
                "capture ends at {expected}, supplied boundary is {actual}"
            ),
            Self::CheckpointBeyondBound { bytes, maximum } => write!(
                formatter,
                "checkpoint contains {bytes} bytes, above merge bound {maximum}"
            ),
        }
    }
}

impl std::error::Error for SourceCaptureError {}

/// Metrics for one persistent edit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EditMetrics {
    pub allocations: AllocationMetrics,
    pub result: TreeMetrics,
    pub unchanged_prefix_bytes: usize,
    pub unchanged_suffix_bytes: usize,
    /// Unchanged bytes copied from at most the two edit-boundary pieces.
    pub copied_existing_source_bytes: usize,
    /// Caller-supplied replacement bytes copied into immutable buffers.
    pub copied_replacement_bytes: usize,
    /// Existing prefix bytes whose anchors changed during boundary compaction.
    pub compacted_prefix_bytes: usize,
    /// Existing suffix bytes whose anchors changed during boundary compaction.
    pub compacted_suffix_bytes: usize,
}

/// Errors rejected before changing a source root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceError {
    InvalidRange {
        range: Range<usize>,
        source_len: usize,
    },
    NotCharBoundary(usize),
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRange { range, source_len } => {
                write!(
                    formatter,
                    "invalid edit range {range:?} for {source_len} bytes"
                )
            }
            Self::NotCharBoundary(offset) => {
                write!(formatter, "byte offset {offset} splits a UTF-8 scalar")
            }
        }
    }
}

impl std::error::Error for SourceError {}

/// An immutable stable-anchor source fragment retained from an edit split.
///
/// The root is opaque to callers, but its bytes and buffer anchors remain
/// available without flattening. Keeping this fragment in provenance gives a
/// parser an exact stable region, not a probabilistic hash assertion.
#[derive(Clone, Debug, Default)]
pub struct SourceFragment {
    root: Link,
}

impl SourceFragment {
    #[must_use]
    pub fn len_bytes(&self) -> usize {
        link_len(&self.root)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    #[must_use]
    pub fn metrics(&self) -> TreeMetrics {
        link_metrics(&self.root).into()
    }

    /// Exact immutable-buffer retention reachable from this fragment.
    #[must_use]
    pub fn buffer_retention(&self) -> BufferRetentionMetrics {
        buffer_retention(&self.root)
    }

    /// Sorted unique buffer identities retained by this bounded fragment.
    /// Intended for lifetime audits; cursor paths do not allocate this list.
    #[must_use]
    pub fn retained_buffer_ids(&self) -> Vec<BufferId> {
        retained_buffer_ids(&self.root)
    }

    /// Buffer allocations retained by this fragment, sorted by identity.
    /// Consumers can union these records across a committed root set without
    /// double-counting buffers shared by many small leaves.
    #[must_use]
    pub fn retained_buffer_allocations(&self) -> Vec<RetainedBufferAllocation> {
        retained_buffer_allocations(&self.root)
    }

    #[must_use]
    pub fn cursor(&self) -> SourceCursor {
        SourceCursor::at_start(self.root.clone())
    }

    /// Creates a cursor at a fragment-relative byte offset.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::InvalidRange`] if `offset` is beyond this
    /// bounded fragment.
    pub fn cursor_at(&self, offset: usize) -> Result<SourceCursor, SourceError> {
        SourceCursor::new(self.root.clone(), offset, None)
    }

    /// Fragment-relative cursor creation with exact tree-node accounting.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::InvalidRange`] if `offset` exceeds this fragment.
    pub fn cursor_at_metered(
        &self,
        offset: usize,
    ) -> Result<(SourceCursor, CursorStartMetrics), SourceError> {
        SourceCursor::new_metered(self.root.clone(), offset, None)
    }

    #[must_use]
    pub fn byte_at(&self, offset: usize) -> Option<AnchoredByte> {
        self.cursor_at(offset).ok()?.next()
    }

    #[must_use]
    pub fn anchor_at(&self, offset: usize) -> Option<Anchor> {
        self.byte_at(offset).map(|item| item.anchor)
    }

    #[must_use]
    pub fn first_anchor(&self) -> Option<Anchor> {
        self.cursor().next().map(|item| item.anchor)
    }

    /// Test/batch helper. Edit and cursor paths do not call this method.
    #[must_use]
    pub fn materialize(&self) -> String {
        materialize_root(&self.root)
    }

    /// Exact equality of stable buffer ranges, independent of tree shape.
    ///
    /// This compares piece identities and ranges rather than content hashes.
    /// It is O(number of retained piece runs), never O(payload bytes).
    #[must_use]
    pub fn same_anchored_layout(&self, other: &Self) -> bool {
        let mut left = PieceRunCursor::new(self.root.clone());
        let mut right = PieceRunCursor::new(other.root.clone());
        loop {
            match (left.next(), right.next()) {
                (None, None) => return true,
                (Some(left), Some(right))
                    if left.buffer.id == right.buffer.id && left.range == right.range => {}
                _ => return false,
            }
        }
    }
}

/// A fixed-forest capture fed by the source cursor that is already scanning a
/// block. It retains buffer runs but never copies payload bytes.
///
/// Capturing during the existing sequential pass is the production-shaped
/// path for leaf provenance: it avoids a fresh tree descent for every sealed
/// leaf. [`PersistentSource::fragment`] remains available as an explicitly
/// metered random-access fallback.
pub struct SourceCapture {
    source: SourceRootIdentity,
    document_start: usize,
    next_offset: usize,
    forest: BuildForest,
    pending_buffer: Option<Arc<Buffer>>,
    pending_range: Range<usize>,
    allocations: AllocationMetrics,
    metrics: SourceCaptureMetrics,
}

impl fmt::Debug for SourceCapture {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceCapture")
            .field("source", &self.source)
            .field("document_start", &self.document_start)
            .field("next_offset", &self.next_offset)
            .field("metrics", &self.metrics)
            .finish_non_exhaustive()
    }
}

impl SourceCapture {
    /// Starts a capture at a certified cursor boundary.
    #[must_use]
    pub fn new(start: CertifiedSourceBoundary) -> Self {
        Self {
            source: start.source,
            document_start: start.offset,
            next_offset: start.offset,
            forest: BuildForest::new(),
            pending_buffer: None,
            pending_range: 0..0,
            allocations: AllocationMetrics::default(),
            metrics: SourceCaptureMetrics::default(),
        }
    }

    fn observe(
        &mut self,
        source: SourceRootIdentity,
        document_offset: usize,
        piece: &Piece,
        piece_offset: usize,
    ) -> Result<(), SourceCaptureError> {
        if source != self.source {
            return Err(SourceCaptureError::DifferentSource);
        }
        if document_offset != self.next_offset {
            return Err(SourceCaptureError::NonContiguous {
                expected: self.next_offset,
                actual: document_offset,
            });
        }
        let buffer_offset = piece.range.start + piece_offset;
        let can_extend = self.pending_buffer.as_ref().is_some_and(|buffer| {
            buffer.id == piece.buffer.id && self.pending_range.end == buffer_offset
        });
        if can_extend {
            self.pending_range.end += 1;
        } else {
            self.seal_pending();
            self.pending_buffer = Some(piece.buffer.clone());
            self.metrics.buffer_handle_clones += 1;
            self.pending_range = buffer_offset..buffer_offset + 1;
        }
        self.next_offset += 1;
        self.metrics.bytes_observed += 1;
        Ok(())
    }

    fn seal_pending(&mut self) {
        let Some(buffer) = self.pending_buffer.take() else {
            return;
        };
        let range = std::mem::replace(&mut self.pending_range, 0..0);
        let before = self.allocations.new_nodes;
        self.forest.push(
            new_leaf(Piece::new(buffer, range), &mut self.allocations),
            &mut self.allocations,
        );
        self.metrics.piece_runs += 1;
        self.metrics.max_atomic_nodes_allocated = self
            .metrics
            .max_atomic_nodes_allocated
            .max(self.allocations.new_nodes - before);
    }

    /// Current exact end of this capture, certified without a source lookup.
    #[must_use]
    pub const fn certified_end(&self) -> CertifiedSourceBoundary {
        CertifiedSourceBoundary {
            source: self.source,
            offset: self.next_offset,
        }
    }

    /// Exact start of this capture, certified without a source lookup.
    #[must_use]
    pub const fn certified_start(&self) -> CertifiedSourceBoundary {
        CertifiedSourceBoundary {
            source: self.source,
            offset: self.document_start,
        }
    }

    /// Number of source bytes observed by this capture.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.next_offset - self.document_start
    }

    /// Whether no physical source byte has been observed.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.next_offset == self.document_start
    }

    /// Work accumulated so far, including allocations already performed by a
    /// checkpoint that may subsequently be merged, retained, or discarded.
    #[must_use]
    pub const fn metrics(&self) -> SourceCaptureMetrics {
        let mut metrics = self.metrics;
        metrics.nodes_allocated = self.allocations.new_nodes;
        metrics
    }

    /// Appends a small checkpoint captured by the same forward cursor.
    ///
    /// The suffix's source bytes are not read again and payload is never
    /// copied. Immutable piece handles are transferred in source order. The
    /// caller supplies a hard byte ceiling because this merge is synchronous;
    /// block parsing uses only its fixed-size undecided line prefix (plus a
    /// two-byte CRLF checkpoint). Tree traversal and allocation high-water
    /// counts remain visible in [`SourceCaptureMetrics`].
    ///
    /// # Errors
    ///
    /// Rejects a different revision, a non-contiguous suffix, or a checkpoint
    /// larger than `maximum_bytes` before mutating either capture.
    pub fn append_bounded(
        &mut self,
        suffix: Self,
        maximum_bytes: usize,
    ) -> Result<(), SourceCaptureError> {
        if suffix.source != self.source {
            return Err(SourceCaptureError::DifferentSource);
        }
        if suffix.document_start != self.next_offset {
            return Err(SourceCaptureError::NonContiguous {
                expected: self.next_offset,
                actual: suffix.document_start,
            });
        }
        let bytes = suffix.len();
        if bytes > maximum_bytes {
            return Err(SourceCaptureError::CheckpointBeyondBound {
                bytes,
                maximum: maximum_bytes,
            });
        }

        let suffix_end = suffix.next_offset;
        let suffix_metrics = suffix.metrics;
        let suffix_allocations = suffix.allocations;
        let forest = suffix.forest;
        let pending_buffer = suffix.pending_buffer;
        let pending_range = suffix.pending_range;

        add_allocation_metrics(&mut self.allocations, suffix_allocations);
        self.metrics.bytes_observed += suffix_metrics.bytes_observed;
        self.metrics.piece_runs += suffix_metrics.piece_runs;
        self.metrics.buffer_handle_clones += suffix_metrics.buffer_handle_clones;
        self.metrics.checkpoint_bytes_merged += suffix_metrics.checkpoint_bytes_merged + bytes;
        self.metrics.checkpoint_piece_runs_merged += suffix_metrics.checkpoint_piece_runs_merged;
        self.metrics.checkpoint_tree_nodes_examined +=
            suffix_metrics.checkpoint_tree_nodes_examined;
        self.metrics.max_atomic_checkpoint_bytes = self
            .metrics
            .max_atomic_checkpoint_bytes
            .max(suffix_metrics.max_atomic_checkpoint_bytes)
            .max(bytes);
        self.metrics.max_atomic_checkpoint_piece_runs = self
            .metrics
            .max_atomic_checkpoint_piece_runs
            .max(suffix_metrics.max_atomic_checkpoint_piece_runs);
        self.metrics.max_atomic_checkpoint_tree_nodes = self
            .metrics
            .max_atomic_checkpoint_tree_nodes
            .max(suffix_metrics.max_atomic_checkpoint_tree_nodes);
        self.metrics.max_atomic_nodes_allocated = self
            .metrics
            .max_atomic_nodes_allocated
            .max(suffix_metrics.max_atomic_nodes_allocated);

        let mut merged_runs = 0;
        let mut cloned_runs = 0;
        let mut examined_nodes = 0;
        let before_transfer_nodes = self.allocations.new_nodes;
        for root in forest.levels.into_iter().rev().flatten() {
            let mut cursor = FixedPieceRunCursor::new(root);
            while let Some(piece) = cursor.next_metered(&mut examined_nodes) {
                merged_runs += 1;
                cloned_runs += 1;
                self.append_piece_run(piece);
            }
        }
        if let Some(buffer) = pending_buffer {
            merged_runs += 1;
            self.append_piece_run(Piece::new(buffer, pending_range));
        }
        self.metrics.checkpoint_piece_runs_merged += merged_runs;
        self.metrics.buffer_handle_clones += cloned_runs;
        self.metrics.checkpoint_tree_nodes_examined += examined_nodes;
        self.metrics.max_atomic_checkpoint_piece_runs = self
            .metrics
            .max_atomic_checkpoint_piece_runs
            .max(merged_runs);
        self.metrics.max_atomic_checkpoint_tree_nodes = self
            .metrics
            .max_atomic_checkpoint_tree_nodes
            .max(examined_nodes);
        self.metrics.max_atomic_nodes_allocated = self
            .metrics
            .max_atomic_nodes_allocated
            .max(self.allocations.new_nodes - before_transfer_nodes);
        debug_assert_eq!(self.next_offset, suffix_end);
        Ok(())
    }

    /// Drops an already-inspected prefix from a small undecided checkpoint.
    ///
    /// This is the companion to [`Self::append_bounded`] for a context change:
    /// the old leaf is sealed before the line, while the new leaf retains only
    /// the source envelope beginning at its first required physical/virtual
    /// anchor. Piece handles are sliced and transferred without source-tree
    /// lookup, source-byte reread, or payload copy.
    ///
    /// # Errors
    ///
    /// Rejects another source revision, a boundary outside the capture, or a
    /// discarded prefix larger than `maximum_prefix_bytes`.
    pub fn retain_suffix_bounded(
        self,
        start: CertifiedSourceBoundary,
        maximum_prefix_bytes: usize,
    ) -> Result<Self, SourceCaptureError> {
        if start.source != self.source {
            return Err(SourceCaptureError::DifferentSource);
        }
        if start.offset < self.document_start || start.offset > self.next_offset {
            return Err(SourceCaptureError::NonContiguous {
                expected: self.document_start,
                actual: start.offset,
            });
        }
        let discarded = start.offset - self.document_start;
        if discarded > maximum_prefix_bytes {
            return Err(SourceCaptureError::CheckpointBeyondBound {
                bytes: discarded,
                maximum: maximum_prefix_bytes,
            });
        }

        let old_start = self.document_start;
        let old_end = self.next_offset;
        let old_metrics = self.metrics;
        let old_allocations = self.allocations;
        let forest = self.forest;
        let pending_buffer = self.pending_buffer;
        let pending_range = self.pending_range;
        let mut retained = Self::new(start);
        retained.metrics = old_metrics;
        retained.metrics.checkpoint_prefix_bytes_discarded += discarded;
        retained.allocations = old_allocations;

        let mut document_offset = old_start;
        let mut examined_nodes = 0;
        let mut retained_runs = 0;
        let mut cloned_runs = 0;
        let before_transfer_nodes = retained.allocations.new_nodes;
        for root in forest.levels.into_iter().rev().flatten() {
            let mut cursor = FixedPieceRunCursor::new(root);
            while let Some(piece) = cursor.next_metered(&mut examined_nodes) {
                cloned_runs += 1;
                let piece_start = document_offset;
                let piece_end = piece_start + piece.len();
                document_offset = piece_end;
                if piece_end <= start.offset {
                    continue;
                }
                let trim = start.offset.saturating_sub(piece_start);
                let range = piece.range.start + trim..piece.range.end;
                retained.append_piece_run(Piece::new(piece.buffer, range));
                retained_runs += 1;
            }
        }
        if let Some(buffer) = pending_buffer {
            let piece = Piece::new(buffer, pending_range);
            let piece_start = document_offset;
            let piece_end = piece_start + piece.len();
            document_offset = piece_end;
            if piece_end > start.offset {
                let trim = start.offset.saturating_sub(piece_start);
                let range = piece.range.start + trim..piece.range.end;
                retained.append_piece_run(Piece::new(piece.buffer, range));
                retained_runs += 1;
            }
        }
        debug_assert_eq!(document_offset, old_end);
        debug_assert_eq!(retained.next_offset, old_end);
        retained.metrics.checkpoint_tree_nodes_examined += examined_nodes;
        retained.metrics.checkpoint_piece_runs_merged += retained_runs;
        retained.metrics.buffer_handle_clones += cloned_runs;
        retained.metrics.max_atomic_checkpoint_bytes =
            retained.metrics.max_atomic_checkpoint_bytes.max(discarded);
        retained.metrics.max_atomic_checkpoint_piece_runs = retained
            .metrics
            .max_atomic_checkpoint_piece_runs
            .max(retained_runs);
        retained.metrics.max_atomic_checkpoint_tree_nodes = retained
            .metrics
            .max_atomic_checkpoint_tree_nodes
            .max(examined_nodes);
        retained.metrics.max_atomic_nodes_allocated = retained
            .metrics
            .max_atomic_nodes_allocated
            .max(retained.allocations.new_nodes - before_transfer_nodes);
        Ok(retained)
    }

    fn append_piece_run(&mut self, piece: Piece) {
        let piece_len = piece.len();
        let can_extend = self.pending_buffer.as_ref().is_some_and(|buffer| {
            buffer.id == piece.buffer.id && self.pending_range.end == piece.range.start
        });
        if can_extend {
            self.pending_range.end = piece.range.end;
        } else {
            self.seal_pending();
            self.pending_buffer = Some(piece.buffer);
            self.pending_range = piece.range;
        }
        self.next_offset += piece_len;
    }

    /// Seals at the exact certified boundary reached by the scanning cursor.
    ///
    /// # Errors
    ///
    /// Rejects a boundary from another revision or one that does not equal the
    /// byte immediately after the captured range.
    pub fn finish(
        mut self,
        end: CertifiedSourceBoundary,
    ) -> Result<CapturedSourceFragment, SourceCaptureError> {
        if end.source != self.source {
            return Err(SourceCaptureError::DifferentSource);
        }
        if end.offset != self.next_offset {
            return Err(SourceCaptureError::EndBoundaryMismatch {
                expected: self.next_offset,
                actual: end.offset,
            });
        }
        self.seal_pending();
        let forest = std::mem::replace(&mut self.forest, BuildForest::new());
        let before = self.allocations.new_nodes;
        let root = forest.finish(&mut self.allocations);
        self.metrics.max_atomic_nodes_allocated = self
            .metrics
            .max_atomic_nodes_allocated
            .max(self.allocations.new_nodes - before);
        self.metrics.nodes_allocated = self.allocations.new_nodes;
        debug_assert_eq!(self.allocations.copied_bytes, 0);
        Ok(CapturedSourceFragment {
            source: self.source,
            document: self.document_start..self.next_offset,
            fragment: SourceFragment { root },
            metrics: self.metrics,
        })
    }
}

/// Independently retained, bounded source captured during an existing scan.
#[derive(Clone, Debug)]
pub struct CapturedSourceFragment {
    source: SourceRootIdentity,
    document: Range<usize>,
    fragment: SourceFragment,
    metrics: SourceCaptureMetrics,
}

impl CapturedSourceFragment {
    #[must_use]
    pub const fn source_identity(&self) -> SourceRootIdentity {
        self.source
    }

    #[must_use]
    pub fn document_range(&self) -> Range<usize> {
        self.document.clone()
    }

    #[must_use]
    pub const fn metrics(&self) -> SourceCaptureMetrics {
        self.metrics
    }

    #[must_use]
    pub fn fragment(&self) -> &SourceFragment {
        &self.fragment
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        SourceRootIdentity,
        Range<usize>,
        SourceFragment,
        SourceCaptureMetrics,
    ) {
        (self.source, self.document, self.fragment, self.metrics)
    }
}

/// Exact offset mapping and retained source for one unchanged edit region.
#[derive(Clone, Debug)]
pub struct UnchangedRegion {
    pub old: Range<usize>,
    pub new: Range<usize>,
    fragment: SourceFragment,
}

impl UnchangedRegion {
    #[must_use]
    pub fn fragment(&self) -> &SourceFragment {
        &self.fragment
    }

    /// Maps an old byte position within this unchanged region to the new root.
    /// End positions are intentionally excluded; this maps source bytes rather
    /// than the ambiguous boundary between two regions.
    #[must_use]
    pub fn map_old_byte(&self, old_offset: usize) -> Option<usize> {
        if self.old.contains(&old_offset) {
            Some(self.new.start + (old_offset - self.old.start))
        } else {
            None
        }
    }
}

/// Exact stable-anchor provenance outside explicitly compacted boundaries.
#[derive(Clone, Debug)]
pub struct EditProvenance {
    /// Prefix whose original stable anchors remain exact.
    pub prefix: UnchangedRegion,
    /// Suffix whose original stable anchors remain exact.
    pub suffix: UnchangedRegion,
    /// Logically unchanged old prefix bytes copied into new boundary buffers.
    pub compacted_prefix: Range<usize>,
    /// Logically unchanged old suffix bytes copied into new boundary buffers.
    pub compacted_suffix: Range<usize>,
}

impl EditProvenance {
    #[must_use]
    pub fn map_old_byte(&self, old_offset: usize) -> Option<usize> {
        self.prefix
            .map_old_byte(old_offset)
            .or_else(|| self.suffix.map_old_byte(old_offset))
    }
}

/// Result of an immutable source edit.
#[derive(Clone, Debug)]
pub struct EditOutcome {
    pub source: PersistentSource,
    pub provenance: EditProvenance,
    pub metrics: EditMetrics,
}

/// A persistent, balanced, UTF-8 piece tree.
#[derive(Clone, Debug)]
pub struct PersistentSource {
    root: Link,
    identity: SourceRootIdentity,
}

impl Default for PersistentSource {
    fn default() -> Self {
        Self {
            root: None,
            identity: SourceRootIdentity::mint(),
        }
    }
}

impl PersistentSource {
    /// Copies initial text into independent immutable buffers and pieces no
    /// larger than [`MAX_PIECE_BYTES`].
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        Self::from_text_with_metrics(text).0
    }

    #[must_use]
    pub fn from_text_with_metrics(text: &str) -> (Self, AllocationMetrics) {
        let mut allocations = AllocationMetrics::default();
        let root = build_buffer(text, false, &mut allocations);
        (
            Self {
                root,
                identity: SourceRootIdentity::mint(),
            },
            allocations,
        )
    }

    /// Exact immutable source-root identity.
    #[must_use]
    pub const fn identity(&self) -> SourceRootIdentity {
        self.identity
    }

    #[must_use]
    pub fn len_bytes(&self) -> usize {
        link_len(&self.root)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    #[must_use]
    pub fn line_breaks(&self) -> usize {
        link_metrics(&self.root).line_breaks
    }

    #[must_use]
    pub fn metrics(&self) -> TreeMetrics {
        link_metrics(&self.root).into()
    }

    /// Exact immutable-buffer retention reachable from this root.
    #[must_use]
    pub fn buffer_retention(&self) -> BufferRetentionMetrics {
        buffer_retention(&self.root)
    }

    /// Sorted unique buffer identities reachable from this source root.
    /// Intended for lifetime audits, not hot cursor paths.
    #[must_use]
    pub fn retained_buffer_ids(&self) -> Vec<BufferId> {
        retained_buffer_ids(&self.root)
    }

    /// Buffer allocations reachable from this source, sorted by identity.
    #[must_use]
    pub fn retained_buffer_allocations(&self) -> Vec<RetainedBufferAllocation> {
        retained_buffer_allocations(&self.root)
    }

    #[must_use]
    pub fn byte_at(&self, offset: usize) -> Option<AnchoredByte> {
        let mut cursor = self.cursor_at(offset).ok()?;
        cursor.next()
    }

    #[must_use]
    pub fn anchor_at(&self, offset: usize) -> Option<Anchor> {
        self.byte_at(offset).map(|item| item.anchor)
    }

    #[must_use]
    pub fn is_char_boundary(&self, offset: usize) -> bool {
        self.is_char_boundary_metered(offset).0
    }

    /// Checks a UTF-8 boundary while exposing every source-index node and
    /// payload byte inspected by the lookup.
    #[must_use]
    pub fn is_char_boundary_metered(&self, offset: usize) -> (bool, CursorStartMetrics, usize) {
        if offset == 0 || offset == self.len_bytes() {
            return (true, CursorStartMetrics::default(), 0);
        }
        let Ok((mut cursor, metrics)) = self.cursor_at_metered(offset) else {
            return (false, CursorStartMetrics::default(), 0);
        };
        let Some(item) = cursor.next() else {
            return (false, metrics, 0);
        };
        (item.byte & 0b1100_0000 != 0b1000_0000, metrics, 1)
    }

    #[must_use]
    pub fn cursor(&self) -> SourceCursor {
        SourceCursor::new(self.root.clone(), 0, Some(self.identity))
            .unwrap_or_else(|_| unreachable!("zero is always within a source root"))
    }

    /// Creates a byte cursor in O(tree depth), without flattening source text.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::InvalidRange`] when `offset` is beyond the end.
    pub fn cursor_at(&self, offset: usize) -> Result<SourceCursor, SourceError> {
        SourceCursor::new(self.root.clone(), offset, Some(self.identity))
    }

    /// Creates a cursor and reports every tree node inspected to position it.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::InvalidRange`] when `offset` is beyond the end.
    pub fn cursor_at_metered(
        &self,
        offset: usize,
    ) -> Result<(SourceCursor, CursorStartMetrics), SourceError> {
        SourceCursor::new_metered(self.root.clone(), offset, Some(self.identity))
    }

    /// Extracts a bounded stable-anchor fragment without copying payload.
    ///
    /// This random-access convenience performs O(log document) structural
    /// work and reports it explicitly. Parser construction should prefer a
    /// [`SourceCapture`] fed by its existing sequential cursor.
    ///
    /// # Errors
    ///
    /// Returns the same range and UTF-8 boundary errors as [`Self::edit`].
    pub fn fragment(
        &self,
        range: Range<usize>,
    ) -> Result<(SourceFragment, FragmentExtractionMetrics), SourceError> {
        if range.start > range.end || range.end > self.len_bytes() {
            return Err(SourceError::InvalidRange {
                range,
                source_len: self.len_bytes(),
            });
        }
        let (start_boundary, start_index, start_bytes) = self.is_char_boundary_metered(range.start);
        if !start_boundary {
            return Err(SourceError::NotCharBoundary(range.start));
        }
        let (end_boundary, end_index, end_bytes) = self.is_char_boundary_metered(range.end);
        if !end_boundary {
            return Err(SourceError::NotCharBoundary(range.end));
        }
        let mut allocations = AllocationMetrics::default();
        let (_, tail) = split(self.root.clone(), range.start, &mut allocations);
        let (root, _) = split(tail, range.len(), &mut allocations);
        debug_assert_eq!(link_len(&root), range.len());
        debug_assert_eq!(allocations.copied_bytes, 0);
        Ok((
            SourceFragment { root },
            FragmentExtractionMetrics {
                boundary_index_nodes_visited: start_index.index_nodes_visited
                    + end_index.index_nodes_visited,
                boundary_bytes_examined: start_bytes + end_bytes,
                structural_nodes_allocated: allocations.new_nodes,
                payload_bytes_copied: allocations.copied_bytes,
            },
        ))
    }

    /// Applies one scalar-safe byte splice. Work is proportional to tree depth
    /// plus replacement pieces. Eligible boundary fragments are compacted by
    /// copying at most one piece per side, bounded by `2 * MAX_PIECE_BYTES`.
    ///
    /// # Errors
    ///
    /// Returns [`SourceError::InvalidRange`] for an out-of-bounds range and
    /// [`SourceError::NotCharBoundary`] when either endpoint splits UTF-8.
    pub fn edit(&self, range: Range<usize>, replacement: &str) -> Result<EditOutcome, SourceError> {
        if range.start > range.end || range.end > self.len_bytes() {
            return Err(SourceError::InvalidRange {
                range,
                source_len: self.len_bytes(),
            });
        }
        if !self.is_char_boundary(range.start) {
            return Err(SourceError::NotCharBoundary(range.start));
        }
        if !self.is_char_boundary(range.end) {
            return Err(SourceError::NotCharBoundary(range.end));
        }

        let old_len = self.len_bytes();
        let mut allocations = AllocationMetrics::default();
        let (prefix_root, tail) = split(self.root.clone(), range.start, &mut allocations);
        let (_, suffix_root) = split(tail, range.end - range.start, &mut allocations);
        let no_op = range.is_empty() && replacement.is_empty();
        let (stable_prefix_root, compacted_prefix, compacted_prefix_bytes) = if no_op {
            (prefix_root, None, 0)
        } else {
            take_right_compactable_boundary(prefix_root, &mut allocations)
        };
        let (stable_suffix_root, compacted_suffix, compacted_suffix_bytes) = if no_op {
            (suffix_root, None, 0)
        } else {
            take_left_compactable_boundary(suffix_root, &mut allocations)
        };
        let replacement_root = build_parts(
            &[
                compacted_prefix.as_ref().map_or("", Piece::text),
                replacement,
                compacted_suffix.as_ref().map_or("", Piece::text),
            ],
            true,
            &mut allocations,
        );
        let root = concat(
            concat(
                stable_prefix_root.clone(),
                replacement_root,
                &mut allocations,
            ),
            stable_suffix_root.clone(),
            &mut allocations,
        );
        let new_suffix_start = range.start + replacement.len();
        let source = Self {
            root,
            identity: SourceRootIdentity::mint(),
        };
        let result = source.metrics();
        debug_assert_eq!(result.bytes, old_len - range.len() + replacement.len());
        let copied_existing_source_bytes = compacted_prefix_bytes + compacted_suffix_bytes;
        debug_assert!(copied_existing_source_bytes <= 2 * MAX_PIECE_BYTES);
        let rewritten_bytes = copied_existing_source_bytes + replacement.len();
        debug_assert_eq!(allocations.staged_bytes_copied, rewritten_bytes);
        debug_assert_eq!(allocations.immutable_bytes_copied, rewritten_bytes);
        debug_assert_eq!(allocations.copied_bytes, rewritten_bytes * 2);
        let stable_prefix_end = range.start - compacted_prefix_bytes;
        let stable_suffix_old_start = range.end + compacted_suffix_bytes;
        let stable_suffix_new_start = new_suffix_start + compacted_suffix_bytes;

        Ok(EditOutcome {
            source,
            provenance: EditProvenance {
                prefix: UnchangedRegion {
                    old: 0..stable_prefix_end,
                    new: 0..stable_prefix_end,
                    fragment: SourceFragment {
                        root: stable_prefix_root,
                    },
                },
                suffix: UnchangedRegion {
                    old: stable_suffix_old_start..old_len,
                    new: stable_suffix_new_start..result.bytes,
                    fragment: SourceFragment {
                        root: stable_suffix_root,
                    },
                },
                compacted_prefix: stable_prefix_end..range.start,
                compacted_suffix: range.end..stable_suffix_old_start,
            },
            metrics: EditMetrics {
                allocations,
                result,
                unchanged_prefix_bytes: range.start,
                unchanged_suffix_bytes: old_len - range.end,
                copied_existing_source_bytes,
                copied_replacement_bytes: replacement.len(),
                compacted_prefix_bytes,
                compacted_suffix_bytes,
            },
        })
    }

    /// Test/batch helper. The edit implementation never calls this method.
    #[must_use]
    pub fn materialize(&self) -> String {
        materialize_root(&self.root)
    }

    /// Audits balancing, cached metrics, UTF-8 piece boundaries, and leaf size.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic if any tree, metric, piece, or UTF-8 invariant is
    /// violated.
    pub fn validate(&self) -> Result<(), String> {
        let computed = validate_node(&self.root)?;
        if computed != link_metrics(&self.root) {
            return Err("root metrics differ from recursively computed metrics".to_owned());
        }
        Ok(())
    }
}

impl From<&str> for PersistentSource {
    fn from(value: &str) -> Self {
        Self::from_text(value)
    }
}

/// Stateful in-order byte cursor. It retains only an O(tree-depth) traversal
/// stack and the current piece.
#[derive(Debug)]
pub struct SourceCursor {
    pending: Vec<Arc<Node>>,
    current: Option<Piece>,
    piece_offset: usize,
    absolute_offset: usize,
    source_len: usize,
    source_identity: Option<SourceRootIdentity>,
}

impl SourceCursor {
    fn at_start(root: Link) -> Self {
        Self::new(root, 0, None)
            .unwrap_or_else(|_| unreachable!("zero is always within a source root"))
    }

    fn new(
        root: Link,
        offset: usize,
        source_identity: Option<SourceRootIdentity>,
    ) -> Result<Self, SourceError> {
        Self::new_metered(root, offset, source_identity).map(|(cursor, _)| cursor)
    }

    fn new_metered(
        root: Link,
        mut offset: usize,
        source_identity: Option<SourceRootIdentity>,
    ) -> Result<(Self, CursorStartMetrics), SourceError> {
        let source_len = link_len(&root);
        if offset > source_len {
            return Err(SourceError::InvalidRange {
                range: offset..offset,
                source_len,
            });
        }
        let mut cursor = Self {
            pending: Vec::with_capacity(link_metrics(&root).depth),
            current: None,
            piece_offset: 0,
            absolute_offset: offset,
            source_len,
            source_identity,
        };
        let mut metrics = CursorStartMetrics::default();
        let Some(mut node) = root else {
            return Ok((cursor, metrics));
        };
        if offset == source_len {
            return Ok((cursor, metrics));
        }
        loop {
            metrics.index_nodes_visited += 1;
            match node.as_ref() {
                Node::Leaf { piece, .. } => {
                    cursor.current = Some(piece.clone());
                    cursor.piece_offset = offset;
                    return Ok((cursor, metrics));
                }
                Node::Branch { left, right, .. } => {
                    if offset < left.len() {
                        cursor.pending.push(right.clone());
                        node = left.clone();
                    } else {
                        offset -= left.len();
                        node = right.clone();
                    }
                }
            }
        }
    }

    fn descend_left(&mut self, mut node: Arc<Node>) -> usize {
        let mut nodes = 0;
        loop {
            nodes += 1;
            match node.as_ref() {
                Node::Leaf { piece, .. } => {
                    self.current = Some(piece.clone());
                    self.piece_offset = 0;
                    return nodes;
                }
                Node::Branch { left, right, .. } => {
                    self.pending.push(right.clone());
                    node = left.clone();
                }
            }
        }
    }

    fn advance_piece(&mut self) -> CursorAdvanceMetrics {
        let mut metrics = CursorAdvanceMetrics {
            piece_transitions: self.current.is_some().into(),
            tree_nodes_descended: 0,
        };
        self.current = None;
        if let Some(node) = self.pending.pop() {
            metrics.tree_nodes_descended = self.descend_left(node);
        }
        metrics
    }

    /// Returns the next byte without advancing its document position.
    ///
    /// It may perform the ordinary fixed-depth transition to the next already
    /// retained piece. This hook lets a parser route CR/LF bytes into a tiny
    /// separator checkpoint before consuming them, rather than capturing and
    /// later trimming a giant leaf.
    #[must_use]
    pub fn peek(&mut self) -> Option<AnchoredByte> {
        self.peek_metered().0
    }

    /// Metered form of [`Self::peek`]. The returned tree work is bounded by
    /// the persistent source depth, but it is not silently folded into a
    /// nominal one-byte parser transition.
    #[must_use]
    pub fn peek_metered(&mut self) -> (Option<AnchoredByte>, CursorAdvanceMetrics) {
        let mut metrics = CursorAdvanceMetrics::default();
        loop {
            let Some(piece) = self.current.as_ref() else {
                return (None, metrics);
            };
            if self.piece_offset < piece.len() {
                return (Some(piece.byte(self.piece_offset)), metrics);
            }
            let advance = self.advance_piece();
            metrics.piece_transitions += advance.piece_transitions;
            metrics.tree_nodes_descended += advance.tree_nodes_descended;
        }
    }

    /// Returns a scalar-safe capability for the cursor's current position.
    /// Fragment cursors intentionally return `None`: only a complete source
    /// revision can certify coordinates for downstream builders.
    #[must_use]
    pub fn certified_boundary(&self) -> Option<CertifiedSourceBoundary> {
        let source = self.source_identity?;
        if self.absolute_offset == 0 || self.absolute_offset == self.source_len {
            return Some(CertifiedSourceBoundary {
                source,
                offset: self.absolute_offset,
            });
        }
        let boundary = match self.current.as_ref() {
            Some(piece) if self.piece_offset < piece.len() => {
                piece.byte(self.piece_offset).byte & 0b1100_0000 != 0b1000_0000
            }
            // Immutable pieces begin and end only at scalar boundaries.
            Some(_) | None => true,
        };
        boundary.then_some(CertifiedSourceBoundary {
            source,
            offset: self.absolute_offset,
        })
    }

    /// Reads one byte while feeding the same already-inspected piece into a
    /// bounded capture. No source index lookup and no payload copy occurs.
    ///
    /// # Errors
    ///
    /// Rejects a capture from another source or one not contiguous with this
    /// cursor position.
    pub fn next_captured(
        &mut self,
        capture: &mut SourceCapture,
    ) -> Result<Option<AnchoredByte>, SourceCaptureError> {
        self.next_internal(Some(capture))
    }

    fn next_internal(
        &mut self,
        mut capture: Option<&mut SourceCapture>,
    ) -> Result<Option<AnchoredByte>, SourceCaptureError> {
        loop {
            let Some(piece) = self.current.as_ref() else {
                return Ok(None);
            };
            if self.piece_offset < piece.len() {
                if let Some(capture) = &mut capture {
                    let Some(source) = self.source_identity else {
                        return Err(SourceCaptureError::DifferentSource);
                    };
                    capture.observe(source, self.absolute_offset, piece, self.piece_offset)?;
                }
                let item = piece.byte(self.piece_offset);
                self.piece_offset += 1;
                self.absolute_offset += 1;
                return Ok(Some(item));
            }
            let _ = self.advance_piece();
        }
    }
}

impl Iterator for SourceCursor {
    type Item = AnchoredByte;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_internal(None)
            .unwrap_or_else(|_| unreachable!("ordinary cursor reads do not capture"))
    }
}

struct PieceRunCursor {
    pending: Vec<Arc<Node>>,
}

impl PieceRunCursor {
    fn new(root: Link) -> Self {
        let mut pending = Vec::with_capacity(link_metrics(&root).depth);
        if let Some(root) = root {
            pending.push(root);
        }
        Self { pending }
    }
}

impl Iterator for PieceRunCursor {
    type Item = Piece;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let node = self.pending.pop()?;
            match node.as_ref() {
                Node::Leaf { piece, .. } => return Some(piece.clone()),
                Node::Branch { left, right, .. } => {
                    self.pending.push(right.clone());
                    self.pending.push(left.clone());
                }
            }
        }
    }
}

/// Allocation-free source-order cursor used only by fixed-size checkpoint
/// operations. The persistent source depth is bounded by the address width.
struct FixedPieceRunCursor {
    pending: [Option<Arc<Node>>; usize::BITS as usize],
    pending_len: usize,
}

impl FixedPieceRunCursor {
    fn new(root: Arc<Node>) -> Self {
        let mut cursor = Self {
            pending: std::array::from_fn(|_| None),
            pending_len: 0,
        };
        cursor.push(root);
        cursor
    }

    fn push(&mut self, node: Arc<Node>) {
        assert!(self.pending_len < self.pending.len());
        self.pending[self.pending_len] = Some(node);
        self.pending_len += 1;
    }

    fn pop(&mut self) -> Option<Arc<Node>> {
        if self.pending_len == 0 {
            return None;
        }
        self.pending_len -= 1;
        self.pending[self.pending_len].take()
    }

    fn next_metered(&mut self, nodes_examined: &mut usize) -> Option<Piece> {
        loop {
            let node = self.pop()?;
            *nodes_examined += 1;
            match node.as_ref() {
                Node::Leaf { piece, .. } => return Some(piece.clone()),
                Node::Branch { left, right, .. } => {
                    self.push(right.clone());
                    self.push(left.clone());
                }
            }
        }
    }
}

fn add_allocation_metrics(target: &mut AllocationMetrics, value: AllocationMetrics) {
    target.new_nodes += value.new_nodes;
    target.new_buffers += value.new_buffers;
    target.staged_bytes_copied += value.staged_bytes_copied;
    target.immutable_bytes_copied += value.immutable_bytes_copied;
    target.copied_bytes += value.copied_bytes;
}

fn build_buffer(text: &str, compactable: bool, allocations: &mut AllocationMetrics) -> Link {
    build_parts(&[text], compactable, allocations)
}

fn build_parts(parts: &[&str], compactable: bool, allocations: &mut AllocationMetrics) -> Link {
    let total_bytes = parts.iter().map(|part| part.len()).sum::<usize>();
    if total_bytes == 0 {
        return None;
    }
    let mut forest = BuildForest::new();
    let mut page = String::with_capacity(MAX_PIECE_BYTES);
    for part in parts {
        let mut remainder = *part;
        while !remainder.is_empty() {
            let available = MAX_PIECE_BYTES - page.len();
            let mut take = available.min(remainder.len());
            while take > 0 && !remainder.is_char_boundary(take) {
                take -= 1;
            }
            if take == 0 {
                seal_text_page(&mut page, compactable, &mut forest, allocations);
                continue;
            }
            page.push_str(&remainder[..take]);
            allocations.staged_bytes_copied += take;
            allocations.copied_bytes += take;
            remainder = &remainder[take..];
            if page.len() == MAX_PIECE_BYTES {
                seal_text_page(&mut page, compactable, &mut forest, allocations);
            }
        }
    }
    seal_text_page(&mut page, compactable, &mut forest, allocations);
    forest.finish(allocations)
}

struct BuildForest {
    levels: [Link; usize::BITS as usize],
}

impl BuildForest {
    fn new() -> Self {
        Self {
            levels: std::array::from_fn(|_| None),
        }
    }

    fn push(&mut self, mut tree: Arc<Node>, allocations: &mut AllocationMetrics) {
        for level in &mut self.levels {
            match level.take() {
                None => {
                    *level = Some(tree);
                    return;
                }
                Some(left) => tree = new_branch(left, tree, allocations),
            }
        }
        unreachable!("a source cannot contain more leaves than addressable bytes")
    }

    fn finish(self, allocations: &mut AllocationMetrics) -> Link {
        let mut root = None;
        for tree in self.levels.into_iter().rev().flatten() {
            root = concat(root, Some(tree), allocations);
        }
        root
    }
}

fn seal_text_page(
    page: &mut String,
    compactable: bool,
    forest: &mut BuildForest,
    allocations: &mut AllocationMetrics,
) {
    if page.is_empty() {
        return;
    }
    let buffer = Buffer::copy_from(page, compactable, allocations);
    forest.push(
        new_leaf(
            Piece::new(buffer.clone(), 0..buffer.text.len()),
            allocations,
        ),
        allocations,
    );
    page.clear();
}

fn first_piece(node: &Arc<Node>) -> &Piece {
    match node.as_ref() {
        Node::Leaf { piece, .. } => piece,
        Node::Branch { left, .. } => first_piece(left),
    }
}

fn last_piece(node: &Arc<Node>) -> &Piece {
    match node.as_ref() {
        Node::Leaf { piece, .. } => piece,
        Node::Branch { right, .. } => last_piece(right),
    }
}

fn take_right_compactable_boundary(
    root: Link,
    allocations: &mut AllocationMetrics,
) -> (Link, Option<Piece>, usize) {
    let Some(node) = root.as_ref() else {
        return (None, None, 0);
    };
    let piece = last_piece(node).clone();
    if !piece.is_boundary_compactable() {
        return (root, None, 0);
    }
    let piece_len = piece.len();
    let stable_len = node.len() - piece_len;
    let (stable, boundary) = split(root, stable_len, allocations);
    debug_assert_eq!(link_metrics(&boundary).pieces, 1);
    (stable, Some(piece), piece_len)
}

fn take_left_compactable_boundary(
    root: Link,
    allocations: &mut AllocationMetrics,
) -> (Link, Option<Piece>, usize) {
    let Some(node) = root.as_ref() else {
        return (None, None, 0);
    };
    let piece = first_piece(node).clone();
    if !piece.is_boundary_compactable() {
        return (root, None, 0);
    }
    let piece_len = piece.len();
    let (boundary, stable) = split(root, piece_len, allocations);
    debug_assert_eq!(link_metrics(&boundary).pieces, 1);
    (stable, Some(piece), piece_len)
}

fn new_leaf(piece: Piece, allocations: &mut AllocationMetrics) -> Arc<Node> {
    allocations.new_nodes += 1;
    let metrics = NodeMetrics {
        bytes: piece.len(),
        line_breaks: piece.line_breaks,
        pieces: 1,
        depth: 1,
        max_piece_bytes: piece.len(),
    };
    Arc::new(Node::Leaf { piece, metrics })
}

fn new_branch(left: Arc<Node>, right: Arc<Node>, allocations: &mut AllocationMetrics) -> Arc<Node> {
    allocations.new_nodes += 1;
    let left_metrics = left.metrics();
    let right_metrics = right.metrics();
    let metrics = NodeMetrics {
        bytes: left_metrics.bytes + right_metrics.bytes,
        line_breaks: left_metrics.line_breaks + right_metrics.line_breaks,
        pieces: left_metrics.pieces + right_metrics.pieces,
        depth: 1 + left_metrics.depth.max(right_metrics.depth),
        max_piece_bytes: left_metrics
            .max_piece_bytes
            .max(right_metrics.max_piece_bytes),
    };
    Arc::new(Node::Branch {
        left,
        right,
        metrics,
    })
}

fn concat(left: Link, right: Link, allocations: &mut AllocationMetrics) -> Link {
    match (left, right) {
        (None, right) => right,
        (left, None) => left,
        (Some(left), Some(right)) => Some(join(left, right, allocations)),
    }
}

fn join(left: Arc<Node>, right: Arc<Node>, allocations: &mut AllocationMetrics) -> Arc<Node> {
    if left.depth() > right.depth() + 1 {
        let Node::Branch {
            left: left_left,
            right: left_right,
            ..
        } = left.as_ref()
        else {
            unreachable!("an imbalanced left tree cannot be a leaf")
        };
        let joined = join(left_right.clone(), right, allocations);
        return balance(left_left.clone(), joined, allocations);
    }
    if right.depth() > left.depth() + 1 {
        let Node::Branch {
            left: right_left,
            right: right_right,
            ..
        } = right.as_ref()
        else {
            unreachable!("an imbalanced right tree cannot be a leaf")
        };
        let joined = join(left, right_left.clone(), allocations);
        return balance(joined, right_right.clone(), allocations);
    }
    new_branch(left, right, allocations)
}

fn balance(left: Arc<Node>, right: Arc<Node>, allocations: &mut AllocationMetrics) -> Arc<Node> {
    if left.depth() > right.depth() + 1 {
        let Node::Branch {
            left: left_left,
            right: left_right,
            ..
        } = left.as_ref()
        else {
            unreachable!("an imbalanced left tree cannot be a leaf")
        };
        if left_left.depth() >= left_right.depth() {
            let new_right = new_branch(left_right.clone(), right, allocations);
            return new_branch(left_left.clone(), new_right, allocations);
        }
        let Node::Branch {
            left: middle_left,
            right: middle_right,
            ..
        } = left_right.as_ref()
        else {
            unreachable!("left-right rotation requires a branch")
        };
        let new_left = new_branch(left_left.clone(), middle_left.clone(), allocations);
        let new_right = new_branch(middle_right.clone(), right, allocations);
        return new_branch(new_left, new_right, allocations);
    }
    if right.depth() > left.depth() + 1 {
        let Node::Branch {
            left: right_left,
            right: right_right,
            ..
        } = right.as_ref()
        else {
            unreachable!("an imbalanced right tree cannot be a leaf")
        };
        if right_right.depth() >= right_left.depth() {
            let new_left = new_branch(left, right_left.clone(), allocations);
            return new_branch(new_left, right_right.clone(), allocations);
        }
        let Node::Branch {
            left: middle_left,
            right: middle_right,
            ..
        } = right_left.as_ref()
        else {
            unreachable!("right-left rotation requires a branch")
        };
        let new_left = new_branch(left, middle_left.clone(), allocations);
        let new_right = new_branch(middle_right.clone(), right_right.clone(), allocations);
        return new_branch(new_left, new_right, allocations);
    }
    new_branch(left, right, allocations)
}

fn split(root: Link, offset: usize, allocations: &mut AllocationMetrics) -> (Link, Link) {
    let Some(node) = root else {
        debug_assert_eq!(offset, 0);
        return (None, None);
    };
    debug_assert!(offset <= node.len());
    if offset == 0 {
        return (None, Some(node));
    }
    if offset == node.len() {
        return (Some(node), None);
    }
    match node.as_ref() {
        Node::Leaf { piece, .. } => {
            let left_range = piece.range.start..piece.range.start + offset;
            let right_range = piece.range.start + offset..piece.range.end;
            (
                Some(new_leaf(
                    Piece::new(piece.buffer.clone(), left_range),
                    allocations,
                )),
                Some(new_leaf(
                    Piece::new(piece.buffer.clone(), right_range),
                    allocations,
                )),
            )
        }
        Node::Branch { left, right, .. } => match offset.cmp(&left.len()) {
            std::cmp::Ordering::Less => {
                let (prefix, left_tail) = split(Some(left.clone()), offset, allocations);
                (prefix, concat(left_tail, Some(right.clone()), allocations))
            }
            std::cmp::Ordering::Equal => (Some(left.clone()), Some(right.clone())),
            std::cmp::Ordering::Greater => {
                let (right_prefix, suffix) =
                    split(Some(right.clone()), offset - left.len(), allocations);
                (
                    concat(Some(left.clone()), right_prefix, allocations),
                    suffix,
                )
            }
        },
    }
}

fn link_len(link: &Link) -> usize {
    link.as_deref().map_or(0, Node::len)
}

fn link_metrics(link: &Link) -> NodeMetrics {
    link.as_deref()
        .map_or(NodeMetrics::default(), Node::metrics)
}

fn buffer_retention(root: &Link) -> BufferRetentionMetrics {
    fn visit(
        node: &Arc<Node>,
        buffers: &mut BTreeMap<BufferId, usize>,
        referenced_piece_bytes: &mut usize,
    ) {
        match node.as_ref() {
            Node::Leaf { piece, .. } => {
                buffers.insert(piece.buffer.id, piece.buffer.text.len());
                *referenced_piece_bytes += piece.len();
            }
            Node::Branch { left, right, .. } => {
                visit(left, buffers, referenced_piece_bytes);
                visit(right, buffers, referenced_piece_bytes);
            }
        }
    }

    let mut buffers = BTreeMap::new();
    let mut referenced_piece_bytes = 0;
    if let Some(root) = root {
        visit(root, &mut buffers, &mut referenced_piece_bytes);
    }
    let retained_buffer_bytes = buffers.values().sum::<usize>();
    BufferRetentionMetrics {
        unique_buffers: buffers.len(),
        retained_buffer_bytes,
        referenced_piece_bytes,
        unreferenced_retained_bytes: retained_buffer_bytes.saturating_sub(referenced_piece_bytes),
        max_buffer_bytes: buffers.values().copied().max().unwrap_or(0),
    }
}

fn retained_buffer_ids(root: &Link) -> Vec<BufferId> {
    retained_buffer_allocations(root)
        .into_iter()
        .map(|buffer| buffer.id)
        .collect()
}

fn retained_buffer_allocations(root: &Link) -> Vec<RetainedBufferAllocation> {
    fn sizes_visit(node: &Arc<Node>, sizes: &mut BTreeMap<BufferId, usize>) {
        match node.as_ref() {
            Node::Leaf { piece, .. } => {
                sizes.insert(piece.buffer.id, piece.buffer.text.len());
            }
            Node::Branch { left, right, .. } => {
                sizes_visit(left, sizes);
                sizes_visit(right, sizes);
            }
        }
    }
    let mut sizes = BTreeMap::new();
    if let Some(root) = root {
        sizes_visit(root, &mut sizes);
    }
    sizes
        .into_iter()
        .map(|(id, bytes)| RetainedBufferAllocation { id, bytes })
        .collect()
}

fn materialize_root(root: &Link) -> String {
    let bytes = SourceCursor::at_start(root.clone())
        .map(|item| item.byte)
        .collect::<Vec<_>>();
    String::from_utf8(bytes).expect("piece boundaries preserve valid UTF-8")
}

fn validate_node(root: &Link) -> Result<NodeMetrics, String> {
    let Some(node) = root else {
        return Ok(NodeMetrics::default());
    };
    match node.as_ref() {
        Node::Leaf { piece, metrics } => {
            if piece.range.is_empty() || piece.len() > MAX_PIECE_BYTES {
                return Err(format!("invalid piece size {}", piece.len()));
            }
            if piece.buffer.text.len() > MAX_PIECE_BYTES
                || piece.range.end > piece.buffer.text.len()
                || !piece.buffer.text.is_char_boundary(piece.range.start)
                || !piece.buffer.text.is_char_boundary(piece.range.end)
            {
                return Err("piece range is not a valid UTF-8 buffer slice".to_owned());
            }
            let expected = NodeMetrics {
                bytes: piece.len(),
                line_breaks: count_line_breaks(&piece.buffer.text.as_bytes()[piece.range.clone()]),
                pieces: 1,
                depth: 1,
                max_piece_bytes: piece.len(),
            };
            if *metrics != expected {
                return Err("leaf metrics are stale".to_owned());
            }
            Ok(expected)
        }
        Node::Branch {
            left,
            right,
            metrics,
        } => {
            let left_metrics = validate_node(&Some(left.clone()))?;
            let right_metrics = validate_node(&Some(right.clone()))?;
            if left_metrics.depth.abs_diff(right_metrics.depth) > 1 {
                return Err(format!(
                    "AVL imbalance: left depth {}, right depth {}",
                    left_metrics.depth, right_metrics.depth
                ));
            }
            let expected = NodeMetrics {
                bytes: left_metrics.bytes + right_metrics.bytes,
                line_breaks: left_metrics.line_breaks + right_metrics.line_breaks,
                pieces: left_metrics.pieces + right_metrics.pieces,
                depth: 1 + left_metrics.depth.max(right_metrics.depth),
                max_piece_bytes: left_metrics
                    .max_piece_bytes
                    .max(right_metrics.max_piece_bytes),
            };
            if *metrics != expected {
                return Err("branch metrics are stale".to_owned());
            }
            Ok(expected)
        }
    }
}

fn count_line_breaks(bytes: &[u8]) -> usize {
    let mut count = 0;
    for &byte in bytes {
        count += usize::from(byte == b'\n');
    }
    count
}

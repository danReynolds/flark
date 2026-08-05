//! Crop-backed immutable source revisions and scalar-only edit lineage.
//!
//! Only [`SourceStore`] and an active parser job own strong Crop snapshot
//! wrappers. Edit records retain root identities and coordinate mappings, never
//! a historical root, Rope node, source slice, or weak lease.

use std::fmt;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use crop::Rope;

use crate::{SourceRevision, SourceRootId, SourceTransition};

mod commonmark_line_index;

use commonmark_line_index::{
    CommonMarkLineIndex, CommonMarkLineIndexQueryReceipt, CommonMarkLineIndexRetention,
    CommonMarkLineIndexUpdateReceipt,
};

static NEXT_SOURCE_ROOT: AtomicU64 = AtomicU64::new(1);

/// Maximum synchronous source bytes copied when a cursor refills its scratch.
/// The pinned Crop revision currently uses smaller leaves, but this local cap
/// keeps the runtime invariant true if that implementation detail changes.
pub const SOURCE_CURSOR_COPY_CAP_BYTES: usize = 4 * 1024;

fn try_mint_source_root() -> Option<SourceRootId> {
    NEXT_SOURCE_ROOT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .ok()
        .map(SourceRootId)
}

fn mint_source_root() -> SourceRootId {
    try_mint_source_root().expect("source root identity space exhausted")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceRangeDescriptor {
    pub root: SourceRootId,
    pub start: usize,
    pub end: usize,
}

impl SourceRangeDescriptor {
    #[must_use]
    pub const fn len(self) -> usize {
        self.end - self.start
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnchangedRegion {
    pub old: Range<usize>,
    pub new: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CropEditProvenance {
    pub from: SourceRootId,
    pub to: SourceRootId,
    pub edited_old: Range<usize>,
    pub edited_new: Range<usize>,
    pub prefix: UnchangedRegion,
    pub suffix: UnchangedRegion,
}

impl CropEditProvenance {
    #[must_use]
    pub fn map_unchanged(&self, root: SourceRootId, range: Range<usize>) -> Option<Range<usize>> {
        if root != self.from || range.is_empty() || range.start > range.end {
            return None;
        }
        if contains(&self.prefix.old, &range) {
            return Some(map_region(&self.prefix, range));
        }
        if contains(&self.suffix.old, &range) {
            return Some(map_region(&self.suffix, range));
        }
        None
    }

    #[must_use]
    pub fn map_descriptor(
        &self,
        descriptor: SourceRangeDescriptor,
    ) -> Option<SourceRangeDescriptor> {
        let mapped = self.map_unchanged(descriptor.root, descriptor.start..descriptor.end)?;
        Some(SourceRangeDescriptor {
            root: self.to,
            start: mapped.start,
            end: mapped.end,
        })
    }
}

fn contains(container: &Range<usize>, candidate: &Range<usize>) -> bool {
    candidate.start >= container.start && candidate.end <= container.end
}

fn map_region(region: &UnchangedRegion, range: Range<usize>) -> Range<usize> {
    let start = region.new.start + (range.start - region.old.start);
    start..start + range.len()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceError {
    InvalidRange,
    NotCharBoundary(usize),
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRange => formatter.write_str("invalid source byte range"),
            Self::NotCharBoundary(offset) => {
                write!(formatter, "source byte {offset} splits a UTF-8 scalar")
            }
        }
    }
}

impl std::error::Error for SourceError {}

/// One immutable Crop root wrapper.
#[derive(Debug)]
pub struct CropSnapshotLease {
    root: Rope,
    identity: SourceRootId,
}

impl CropSnapshotLease {
    #[must_use]
    pub fn from_text(text: &str) -> Arc<Self> {
        Self::from_text_with_identity(text, mint_source_root())
    }

    #[must_use]
    pub const fn identity(&self) -> SourceRootId {
        self.identity
    }

    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.root.byte_len()
    }

    /// Exact UTF-16 code-unit length from Crop's persistent tree aggregate.
    /// This is O(1); callers must not recover the value by rescanning source.
    #[must_use]
    pub fn len_utf16(&self) -> usize {
        self.root.utf16_len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }

    #[must_use]
    pub fn is_char_boundary(&self, offset: usize) -> bool {
        offset <= self.len_bytes() && self.root.is_char_boundary(offset)
    }

    pub fn descriptor(&self, range: Range<usize>) -> Result<SourceRangeDescriptor, SourceError> {
        validate_range(self, &range)?;
        Ok(SourceRangeDescriptor {
            root: self.identity,
            start: range.start,
            end: range.end,
        })
    }

    pub fn edit(
        self: &Arc<Self>,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<(Arc<Self>, CropEditProvenance), SourceError> {
        validate_range(self, &range)?;
        Ok(self.edit_validated(range, replacement, mint_source_root()))
    }

    fn from_text_with_identity(text: &str, identity: SourceRootId) -> Arc<Self> {
        debug_assert_ne!(identity, SourceRootId(0));
        Arc::new(Self {
            root: Rope::from(text),
            identity,
        })
    }

    fn edit_validated(
        self: &Arc<Self>,
        range: Range<usize>,
        replacement: &str,
        identity: SourceRootId,
    ) -> (Arc<Self>, CropEditProvenance) {
        debug_assert!(validate_range(self, &range).is_ok());
        debug_assert_ne!(identity, SourceRootId(0));
        let old_len = self.len_bytes();
        let mut root = self.root.clone();
        root.replace(range.clone(), replacement);
        let next = Arc::new(Self { root, identity });
        let edited_new = range.start..range.start + replacement.len();
        let provenance = CropEditProvenance {
            from: self.identity,
            to: next.identity,
            edited_old: range.clone(),
            edited_new: edited_new.clone(),
            prefix: UnchangedRegion {
                old: 0..range.start,
                new: 0..range.start,
            },
            suffix: UnchangedRegion {
                old: range.end..old_len,
                new: edited_new.end..next.len_bytes(),
            },
        };
        (next, provenance)
    }

    #[must_use]
    pub fn cursor(self: &Arc<Self>) -> CropSourceCursor {
        self.cursor_at(0)
            .expect("zero is a boundary in every valid Crop snapshot")
    }

    pub fn cursor_at(self: &Arc<Self>, offset: usize) -> Result<CropSourceCursor, SourceError> {
        if offset > self.len_bytes() {
            return Err(SourceError::InvalidRange);
        }
        if !self.is_char_boundary(offset) {
            return Err(SourceError::NotCharBoundary(offset));
        }
        Ok(CropSourceCursor::new(Arc::clone(self), offset))
    }

    /// Test/oracle helper. Runtime parsing must use [`CropSourceCursor`].
    #[must_use]
    pub fn materialize_for_testing(&self) -> String {
        self.root.to_string()
    }

    fn copy_chunk_from(&self, offset: usize, target: &mut Vec<u8>) -> bool {
        target.clear();
        let Some(chunk) = self.root.byte_slice(offset..).chunks().next() else {
            return false;
        };
        let mut copied = chunk.len().min(SOURCE_CURSOR_COPY_CAP_BYTES);
        while copied > 0 && !chunk.is_char_boundary(copied) {
            copied -= 1;
        }
        debug_assert!(
            copied > 0,
            "a nonempty UTF-8 chunk has a boundary within four bytes"
        );
        target.extend_from_slice(&chunk.as_bytes()[..copied]);
        true
    }

    /// Checks whether `offset` is the start of one physical source line.
    ///
    /// This deliberately does not recover a line number by scanning from the
    /// beginning of the document. Crop answers the scalar-boundary question
    /// from its tree, then at most the byte immediately before and after the
    /// cut are inspected to distinguish LF, lone CR, and the middle of CRLF.
    fn check_physical_line_start(&self, offset: usize) -> PhysicalLineStartCheck {
        if offset > self.len_bytes() || !self.is_char_boundary(offset) {
            return PhysicalLineStartCheck {
                scalar_boundary: false,
                physical_line_start: false,
                adjacent_bytes_read: 0,
            };
        }
        if offset == 0 {
            return PhysicalLineStartCheck {
                scalar_boundary: true,
                physical_line_start: true,
                adjacent_bytes_read: 0,
            };
        }
        let previous = self
            .root
            .byte_slice(..offset)
            .bytes()
            .next_back()
            .expect("a positive source boundary has a preceding byte");
        let mut adjacent_bytes_read = 1;
        let physical_line_start = match previous {
            b'\n' => true,
            b'\r' if offset == self.len_bytes() => true,
            b'\r' => {
                adjacent_bytes_read += 1;
                self.root
                    .byte_slice(offset..)
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte != b'\n')
            }
            _ => false,
        };
        PhysicalLineStartCheck {
            scalar_boundary: true,
            physical_line_start,
            adjacent_bytes_read,
        }
    }
}

/// Bounded source observation used by storage checkpoint resolution.
///
/// It is deliberately crate-private and carries no lineage or checkpoint
/// authority. The storage wrapper consumes the observation together with an
/// actual manifest-derived sequence boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PhysicalLineStartCheck {
    pub scalar_boundary: bool,
    pub physical_line_start: bool,
    pub adjacent_bytes_read: usize,
}

/// Bounded, non-owning metric observation for one exact source prefix.
///
/// This is coordinate data only. It carries no source lease, lineage proof,
/// or parser authority, so a caller must still join it with the current root
/// descriptor and the retained-prefix proof before using it for restart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourcePrefixMetric {
    pub root: SourceRootId,
    pub bytes: usize,
    pub utf16: usize,
}

/// LF-specific physical-line receipt used by the first cross-build Setext
/// gate.
///
/// This intentionally does not claim to be a general `CommonMark` line cursor:
/// the selected proof requires an exact LF immediately before `offset`. Crop's
/// line summary supplies the ordinal and previous line start without scanning
/// the retained prefix; one borrowed adjacent byte certifies the LF. CRLF,
/// lone CR, BOF, and bare-EOF checkpoints remain separate gates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceLfLineBoundaryMetric {
    pub root: SourceRootId,
    pub offset: usize,
    pub completed_line_ordinal: u64,
    pub previous_content_bytes: u64,
    pub adjacent_bytes_read: usize,
}

/// Non-replayable current-source byte cut accepted by the Crop-backed store.
///
/// This is only coordinate certification. The eventual composite restart root
/// must still join it with parser/checkpoint authority before parsing resumes.
#[derive(Debug)]
#[must_use = "a certified source cut must be queried or discarded"]
#[allow(dead_code)] // Consumed by the generic restart coordinator after this source proof gate.
pub(crate) struct CertifiedSourceByteCut {
    snapshot: SourceSnapshotDescriptor,
    offset: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Consumed by the generic restart coordinator after this source proof gate.
pub(crate) struct SourcePhysicalLinePredecessor {
    content_bytes: u64,
    content_utf16: u64,
}

#[allow(dead_code)]
impl SourcePhysicalLinePredecessor {
    pub(crate) const fn content_bytes(self) -> u64 {
        self.content_bytes
    }

    pub(crate) const fn content_utf16(self) -> u64 {
        self.content_utf16
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // Observable proof receipt for the upcoming composite gate.
pub(crate) struct SourcePhysicalLineQueryReceipt {
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

impl From<CommonMarkLineIndexQueryReceipt> for SourcePhysicalLineQueryReceipt {
    fn from(receipt: CommonMarkLineIndexQueryReceipt) -> Self {
        Self {
            tree_nodes_visited: receipt.tree_nodes_visited,
            summary_subtrees_reused: receipt.summary_subtrees_reused,
            boundary_bytes_scanned: receipt.boundary_bytes_scanned,
            maximum_boundary_scratch_bytes: receipt.maximum_boundary_scratch_bytes,
            adjacent_bytes_read: receipt.adjacent_bytes_read,
            index_height: receipt.index_height,
            index_leaves: receipt.index_leaves,
            retained_source_roots: receipt.retained_source_roots,
            retained_source_bytes: receipt.retained_source_bytes,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Consumed by the generic restart coordinator after this source proof gate.
pub(crate) struct SourcePhysicalLineQuery {
    snapshot: SourceSnapshotDescriptor,
    offset: usize,
    line_ordinal: u64,
    physical_line_start: bool,
    previous: Option<SourcePhysicalLinePredecessor>,
    receipt: SourcePhysicalLineQueryReceipt,
}

/// The physical ending terminating one exact source line.
///
/// `BareEof` means the line consumes the remaining source without a line-ending
/// byte. It also describes the empty physical line at BOF/EOF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourcePhysicalLineEnding {
    Lf,
    LoneCr,
    CrLf,
    BareEof,
}

impl SourcePhysicalLineEnding {
    #[must_use]
    pub const fn bytes(self) -> usize {
        match self {
            Self::Lf | Self::LoneCr => 1,
            Self::CrLf => 2,
            Self::BareEof => 0,
        }
    }
}

/// Observable work performed while resolving one physical-line descriptor.
///
/// Complete suffix subtrees are reused from the persistent line index. Source
/// bytes are scanned only inside the one bounded index leaf containing the
/// requested start; `adjacent_bytes_read` covers boundary validation and the
/// one- or two-byte ending classification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourcePhysicalLineDescriptorReceipt {
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

impl From<CommonMarkLineIndexQueryReceipt> for SourcePhysicalLineDescriptorReceipt {
    fn from(receipt: CommonMarkLineIndexQueryReceipt) -> Self {
        Self {
            tree_nodes_visited: receipt.tree_nodes_visited,
            summary_subtrees_reused: receipt.summary_subtrees_reused,
            boundary_bytes_scanned: receipt.boundary_bytes_scanned,
            maximum_boundary_scratch_bytes: receipt.maximum_boundary_scratch_bytes,
            adjacent_bytes_read: receipt.adjacent_bytes_read,
            index_height: receipt.index_height,
            index_leaves: receipt.index_leaves,
            retained_source_roots: receipt.retained_source_roots,
            retained_source_bytes: receipt.retained_source_bytes,
        }
    }
}

/// Query-only coordinates for one exact physical source line.
///
/// This descriptor is bound to a complete source snapshot but intentionally
/// carries no Crop lease, cursor, lineage proof, parser checkpoint, writer
/// capability, or Markdown classification. It may bound recognition reads;
/// it cannot authorize a source claim or publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourcePhysicalLineDescriptor {
    source: SourceSnapshotDescriptor,
    start: usize,
    content_end: usize,
    end: usize,
    content_utf16: usize,
    physical_utf16: usize,
    ending: SourcePhysicalLineEnding,
    receipt: SourcePhysicalLineDescriptorReceipt,
}

impl SourcePhysicalLineDescriptor {
    #[must_use]
    pub const fn source(self) -> SourceSnapshotDescriptor {
        self.source
    }

    #[must_use]
    pub const fn start(self) -> usize {
        self.start
    }

    #[must_use]
    pub const fn content_end(self) -> usize {
        self.content_end
    }

    #[must_use]
    pub const fn end(self) -> usize {
        self.end
    }

    /// Exact UTF-16 width of the line content, excluding its physical ending.
    #[must_use]
    pub const fn content_utf16(self) -> usize {
        self.content_utf16
    }

    /// Exact UTF-16 width of the complete physical line, including its ending.
    #[must_use]
    pub const fn physical_utf16(self) -> usize {
        self.physical_utf16
    }

    #[must_use]
    pub const fn ending(self) -> SourcePhysicalLineEnding {
        self.ending
    }

    #[must_use]
    pub const fn receipt(self) -> SourcePhysicalLineDescriptorReceipt {
        self.receipt
    }
}

#[allow(dead_code)]
impl SourcePhysicalLineQuery {
    pub(crate) const fn snapshot(self) -> SourceSnapshotDescriptor {
        self.snapshot
    }

    pub(crate) const fn offset(self) -> usize {
        self.offset
    }

    pub(crate) const fn line_ordinal(self) -> u64 {
        self.line_ordinal
    }

    pub(crate) const fn is_physical_line_start(self) -> bool {
        self.physical_line_start
    }

    pub(crate) const fn previous(self) -> Option<SourcePhysicalLinePredecessor> {
        self.previous
    }

    pub(crate) const fn receipt(self) -> SourcePhysicalLineQueryReceipt {
        self.receipt
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // Observable proof receipt for the upcoming composite gate.
pub(crate) struct SourceLineIndexUpdateReceipt {
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

impl From<CommonMarkLineIndexUpdateReceipt> for SourceLineIndexUpdateReceipt {
    fn from(receipt: CommonMarkLineIndexUpdateReceipt) -> Self {
        Self {
            old_nodes: receipt.old_nodes,
            new_nodes: receipt.new_nodes,
            new_leaves: receipt.new_leaves,
            new_height: receipt.new_height,
            tree_nodes_visited: receipt.tree_nodes_visited,
            tree_nodes_allocated: receipt.tree_nodes_allocated,
            summary_subtrees_reused: receipt.summary_subtrees_reused,
            coalescing_edge_nodes_visited: receipt.coalescing_edge_nodes_visited,
            leaf_coalesces: receipt.leaf_coalesces,
            replacement_bytes_scanned: receipt.replacement_bytes_scanned,
            boundary_bytes_scanned: receipt.boundary_bytes_scanned,
            maximum_boundary_scratch_bytes: receipt.maximum_boundary_scratch_bytes,
            retained_source_roots: receipt.retained_source_roots,
            retained_source_bytes: receipt.retained_source_bytes,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // Observable proof receipt for the upcoming composite gate.
pub(crate) struct SourceLineIndexRetention {
    pub summary_nodes: usize,
    pub leaves: usize,
    pub height: usize,
    pub maximum_leaf_bytes: usize,
    pub estimated_summary_payload_bytes: usize,
    pub retained_source_roots: usize,
    pub retained_source_bytes: usize,
}

impl From<CommonMarkLineIndexRetention> for SourceLineIndexRetention {
    fn from(retention: CommonMarkLineIndexRetention) -> Self {
        Self {
            summary_nodes: retention.summary_nodes,
            leaves: retention.leaves,
            height: retention.height,
            maximum_leaf_bytes: retention.maximum_leaf_bytes,
            estimated_summary_payload_bytes: retention.estimated_summary_payload_bytes,
            retained_source_roots: retention.retained_source_roots,
            retained_source_bytes: retention.retained_source_bytes,
        }
    }
}

/// One actor-minted pair of decoder cursors for a same-root line-boundary
/// restart.
///
/// The pair is crate-private, linear, and exposes no source lease or arbitrary
/// cursor constructor. Both roles are minted together only after the source
/// store validates one scalar-exact physical-line start. Exact EOF is also
/// admitted so a fully acknowledged bare-EOF line can resume into sealing.
#[must_use = "a resume cursor pair must enter one candidate or be discarded"]
#[derive(Debug)]
#[allow(dead_code)] // The composite writer checkpoint will become the production caller.
pub(crate) struct SourceResumeCursorPair {
    descriptor: SourceSnapshotDescriptor,
    total_utf16: usize,
    offset: usize,
    physical_line_start: bool,
    authoritative: CropSourceCursor,
    recognition: CropSourceCursor,
}

#[allow(dead_code)] // The actor seam is production-shaped but not yet composite-wired.
impl SourceResumeCursorPair {
    #[must_use]
    pub(crate) const fn descriptor(&self) -> SourceSnapshotDescriptor {
        self.descriptor
    }

    #[must_use]
    pub(crate) const fn total_utf16(&self) -> usize {
        self.total_utf16
    }

    #[must_use]
    pub(crate) const fn offset(&self) -> usize {
        self.offset
    }

    #[must_use]
    pub(crate) const fn is_physical_line_start(&self) -> bool {
        self.physical_line_start
    }

    pub(crate) fn into_cursors(self) -> (CropSourceCursor, CropSourceCursor) {
        (self.authoritative, self.recognition)
    }
}

/// One non-cloneable authorization to read a particular store revision.
///
/// The wrapper keeps the live-document actor from exposing its raw `Arc`.
/// Turning it into a cursor consumes the authorization, so production parser
/// code cannot retain both a snapshot handle and arbitrary clones of the same
/// historical Crop root. The actor is responsible for issuing at most one
/// active parser lease.
#[derive(Debug)]
pub struct SourceSnapshotLease {
    revision: SourceRevision,
    root: Arc<CropSnapshotLease>,
}

/// Explicitly cloneable read-only view for diagnostics, tests, and source
/// presentation. It can stream bytes but cannot be converted into the linear
/// parser/build lease consumed by the source-bound candidate writer.
#[derive(Clone, Debug)]
pub struct SourceQuerySnapshot {
    revision: SourceRevision,
    root: Arc<CropSnapshotLease>,
}

/// Borrowed source observation used by the production live-document seam.
///
/// Unlike [`SourceQuerySnapshot`], this view owns no `Arc`, cannot mint an
/// owning cursor, and cannot outlive the shared borrow of the worker actor.
/// Consequently an external observer can neither become the last Crop-root
/// owner nor overlap an edit admitted through `&mut LiveDocumentStore`.
#[derive(Clone, Copy, Debug)]
pub struct SourceQueryView<'a> {
    revision: SourceRevision,
    root: &'a CropSnapshotLease,
}

/// Bounded source-copy receipt for derived, non-authoritative presentation.
///
/// This remains crate-private because a raw source range is not parser
/// authority. The inline materializer may use it only after packed green has
/// produced the exact physical interval.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DerivedSourceReadReceipt {
    pub(crate) chunks_visited: usize,
    pub(crate) bytes_copied: usize,
    pub(crate) maximum_chunk_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DerivedSourceReadError {
    Source(SourceError),
    CapExceeded,
    Overflow,
}

impl SourceQueryView<'_> {
    #[must_use]
    pub const fn revision(&self) -> SourceRevision {
        self.revision
    }

    #[must_use]
    pub fn identity(&self) -> SourceRootId {
        self.root.identity()
    }

    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.root.len_bytes()
    }

    #[must_use]
    pub fn len_utf16(&self) -> usize {
        self.root.len_utf16()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }

    /// Test/oracle materialization borrows the actor; it creates no historical
    /// root owner and therefore cannot influence the last-`Arc` drop lane.
    #[must_use]
    pub fn materialize_for_testing(&self) -> String {
        self.root.materialize_for_testing()
    }

    /// Appends one packed-green-derived physical interval without cloning the
    /// Crop root or minting a parser/build lease.
    ///
    /// The total output cap is checked before the first byte is copied, so an
    /// over-cap leaf cannot partially materialize. The borrow also prevents an
    /// edit from overlapping the read and cannot become the last `Arc` owner of
    /// a historical root.
    pub(crate) fn append_bounded_derived_range(
        &self,
        range: Range<usize>,
        output: &mut Vec<u8>,
        max_output_bytes: usize,
    ) -> Result<DerivedSourceReadReceipt, DerivedSourceReadError> {
        validate_range(self.root, &range).map_err(DerivedSourceReadError::Source)?;
        let final_len = output
            .len()
            .checked_add(range.len())
            .ok_or(DerivedSourceReadError::Overflow)?;
        if final_len > max_output_bytes {
            return Err(DerivedSourceReadError::CapExceeded);
        }

        let initial_len = output.len();
        let mut receipt = DerivedSourceReadReceipt::default();
        for chunk in self.root.root.byte_slice(range).chunks() {
            let bytes = chunk.as_bytes();
            output.extend_from_slice(bytes);
            receipt.chunks_visited += 1;
            receipt.bytes_copied += bytes.len();
            receipt.maximum_chunk_bytes = receipt.maximum_chunk_bytes.max(bytes.len());
        }
        if output.len() != final_len || receipt.bytes_copied != final_len - initial_len {
            output.truncate(initial_len);
            return Err(DerivedSourceReadError::Overflow);
        }
        Ok(receipt)
    }
}

impl SourceQuerySnapshot {
    #[must_use]
    pub const fn revision(&self) -> SourceRevision {
        self.revision
    }

    #[must_use]
    pub fn identity(&self) -> SourceRootId {
        self.root.identity()
    }

    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.root.len_bytes()
    }

    /// Exact whole-snapshot UTF-16 metric carried by the Crop root.
    #[must_use]
    pub fn len_utf16(&self) -> usize {
        self.root.len_utf16()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }

    #[must_use]
    pub fn cursor(&self) -> CropSourceCursor {
        self.root.cursor()
    }

    #[must_use]
    pub fn materialize_for_testing(&self) -> String {
        self.root.materialize_for_testing()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn weak_observer_for_testing(&self) -> Weak<CropSnapshotLease> {
        Arc::downgrade(&self.root)
    }
}

impl SourceSnapshotLease {
    /// Creates a non-owning observer used only by lifetime proof tests.
    #[doc(hidden)]
    #[must_use]
    pub fn weak_observer_for_testing(&self) -> Weak<CropSnapshotLease> {
        Arc::downgrade(&self.root)
    }

    #[must_use]
    pub const fn revision(&self) -> SourceRevision {
        self.revision
    }

    #[must_use]
    pub fn identity(&self) -> SourceRootId {
        self.root.identity()
    }

    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.root.len_bytes()
    }

    /// Exact whole-snapshot UTF-16 metric carried by the Crop root.
    #[must_use]
    pub fn len_utf16(&self) -> usize {
        self.root.len_utf16()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }

    #[must_use]
    pub fn is_char_boundary(&self, offset: usize) -> bool {
        self.root.is_char_boundary(offset)
    }

    pub fn descriptor(&self, range: Range<usize>) -> Result<SourceRangeDescriptor, SourceError> {
        self.root.descriptor(range)
    }

    #[must_use]
    pub fn cursor(self) -> CropSourceCursor {
        CropSourceCursor::new(self.root, 0)
    }

    pub fn cursor_at(self, offset: usize) -> Result<CropSourceCursor, SourceError> {
        if offset > self.root.len_bytes() {
            return Err(SourceError::InvalidRange);
        }
        if !self.root.is_char_boundary(offset) {
            return Err(SourceError::NotCharBoundary(offset));
        }
        Ok(CropSourceCursor::new(self.root, offset))
    }

    /// Test/oracle helper. Runtime parsing must consume the lease into a cursor.
    #[must_use]
    pub fn materialize_for_testing(&self) -> String {
        self.root.materialize_for_testing()
    }
}

fn validate_range(source: &CropSnapshotLease, range: &Range<usize>) -> Result<(), SourceError> {
    if range.start > range.end || range.end > source.len_bytes() {
        return Err(SourceError::InvalidRange);
    }
    if !source.is_char_boundary(range.start) {
        return Err(SourceError::NotCharBoundary(range.start));
    }
    if !source.is_char_boundary(range.end) {
        return Err(SourceError::NotCharBoundary(range.end));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CursorMetrics {
    pub chunk_loads: usize,
    pub chunk_bytes_copied: usize,
    pub maximum_chunk_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceByte {
    pub root: SourceRootId,
    pub offset: usize,
    pub byte: u8,
}

/// Owned, poll-safe cursor with one reusable Crop-chunk scratch buffer.
#[derive(Debug)]
pub struct CropSourceCursor {
    lease: Arc<CropSnapshotLease>,
    offset: usize,
    chunk_start: usize,
    chunk: Vec<u8>,
    metrics: CursorMetrics,
}

impl CropSourceCursor {
    fn new(lease: Arc<CropSnapshotLease>, offset: usize) -> Self {
        let mut cursor = Self {
            lease,
            offset,
            chunk_start: offset,
            chunk: Vec::new(),
            metrics: CursorMetrics::default(),
        };
        let _ = cursor.load_chunk();
        cursor
    }

    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    #[must_use]
    pub fn source_identity(&self) -> SourceRootId {
        self.lease.identity
    }

    #[must_use]
    pub const fn metrics(&self) -> CursorMetrics {
        self.metrics
    }

    /// Exact heap scratch retained by this cursor's reusable Crop chunk.
    /// The owned source root is accounted separately as one active cursor
    /// role; its shared tree bytes are intentionally not double-counted.
    #[must_use]
    pub(crate) fn retained_scratch_bytes(&self) -> usize {
        self.chunk.capacity()
    }

    fn chunk_offset(&self) -> Option<usize> {
        self.offset
            .checked_sub(self.chunk_start)
            .filter(|offset| *offset < self.chunk.len())
    }

    fn load_chunk(&mut self) -> bool {
        if self.offset == self.lease.len_bytes() {
            self.chunk.clear();
            self.chunk_start = self.offset;
            return false;
        }
        self.chunk_start = self.offset;
        if !self.lease.copy_chunk_from(self.offset, &mut self.chunk) {
            return false;
        }
        self.metrics.chunk_loads += 1;
        self.metrics.chunk_bytes_copied += self.chunk.len();
        self.metrics.maximum_chunk_bytes = self.metrics.maximum_chunk_bytes.max(self.chunk.len());
        true
    }

    pub fn next_byte(&mut self) -> Option<SourceByte> {
        if self.chunk_offset().is_none() && !self.load_chunk() {
            return None;
        }
        let relative = self.chunk_offset()?;
        let result = SourceByte {
            root: self.lease.identity,
            offset: self.offset,
            byte: self.chunk[relative],
        };
        self.offset += 1;
        Some(result)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceProjectionSessionError {
    Source(SourceError),
    Invalid(&'static str),
    Overflow(&'static str),
}

impl From<SourceError> for SourceProjectionSessionError {
    fn from(value: SourceError) -> Self {
        Self::Source(value)
    }
}

impl fmt::Display for SourceProjectionSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "source projection session error: {self:?}")
    }
}

impl std::error::Error for SourceProjectionSessionError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceProjectionSessionReceipt {
    pub(crate) descriptor: SourceSnapshotDescriptor,
    pub(crate) cursor_nonce: u64,
    pub(crate) passes_started: u64,
    pub(crate) passes_finished: u64,
    pub(crate) passes_cancelled: u64,
    pub(crate) cursor_roles_minted: u64,
    pub(crate) forward_cursor_jumps: u64,
    pub(crate) source_bytes_read: u64,
    pub(crate) chunk_loads: u64,
    pub(crate) chunk_bytes_copied: u64,
    pub(crate) maximum_chunk_bytes: usize,
    pub(crate) maximum_cursor_scratch_bytes: usize,
    pub(crate) maximum_live_cursor_roles: usize,
}

/// One non-cloneable root lease for all passes in a reference-prefix
/// transaction. Each pass lazily starts at its first authenticated physical
/// request and is strictly forward-only thereafter.
#[derive(Debug)]
pub(crate) struct SourceProjectionSession {
    descriptor: SourceSnapshotDescriptor,
    cursor_nonce: u64,
    root: Arc<CropSnapshotLease>,
    cursor: Option<CropSourceCursor>,
    pass_active: bool,
    pass_lower_bound: Option<usize>,
    receipt: SourceProjectionSessionReceipt,
}

impl SourceProjectionSession {
    pub(crate) const fn descriptor(&self) -> SourceSnapshotDescriptor {
        self.descriptor
    }

    pub(crate) const fn cursor_nonce(&self) -> u64 {
        self.cursor_nonce
    }

    pub(crate) fn begin_pass_at(
        &mut self,
        expected: SourceSnapshotDescriptor,
        cursor_nonce: u64,
        authenticated_lower_bound: usize,
    ) -> Result<(), SourceProjectionSessionError> {
        if self.pass_active
            || self.cursor.is_some()
            || self.pass_lower_bound.is_some()
            || expected != self.descriptor
            || cursor_nonce != self.cursor_nonce
            || cursor_nonce == 0
            || authenticated_lower_bound > self.descriptor.bytes
            || !self.root.is_char_boundary(authenticated_lower_bound)
            || self.root.identity != self.descriptor.root
            || self.root.len_bytes() != self.descriptor.bytes
        {
            return Err(SourceProjectionSessionError::Invalid(
                "projection pass crossed source, cursor, or active-pass authority",
            ));
        }
        self.receipt.passes_started = self.receipt.passes_started.checked_add(1).ok_or(
            SourceProjectionSessionError::Overflow("projection pass count"),
        )?;
        self.pass_active = true;
        self.pass_lower_bound = Some(authenticated_lower_bound);
        Ok(())
    }

    pub(crate) fn read_pass_byte(
        &mut self,
        expected: SourceSnapshotDescriptor,
        cursor_nonce: u64,
        absolute: usize,
    ) -> Result<u8, SourceProjectionSessionError> {
        let lower_bound = self
            .pass_lower_bound
            .ok_or(SourceProjectionSessionError::Invalid(
                "projection pass lost its authenticated physical lower bound",
            ))?;
        if !self.pass_active
            || expected != self.descriptor
            || cursor_nonce != self.cursor_nonce
            || absolute < lower_bound
            || absolute >= self.descriptor.bytes
        {
            return Err(SourceProjectionSessionError::Invalid(
                "projection pass byte is outside its active source",
            ));
        }
        if self
            .cursor
            .as_ref()
            .is_some_and(|cursor| cursor.offset() < absolute)
        {
            self.retire_active_cursor()?;
            self.receipt.forward_cursor_jumps =
                self.receipt.forward_cursor_jumps.checked_add(1).ok_or(
                    SourceProjectionSessionError::Overflow("projection forward cursor jumps"),
                )?;
        }
        if self.cursor.is_none() {
            self.cursor = Some(self.root.cursor_at(absolute)?);
            self.receipt.cursor_roles_minted =
                self.receipt.cursor_roles_minted.checked_add(1).ok_or(
                    SourceProjectionSessionError::Overflow("projection cursor role count"),
                )?;
            self.receipt.maximum_live_cursor_roles = self.receipt.maximum_live_cursor_roles.max(1);
        }
        let cursor = self.cursor.as_mut().expect("projection cursor was minted");
        if cursor.offset() != absolute {
            return Err(SourceProjectionSessionError::Invalid(
                "projection pass requested a non-sequential physical byte",
            ));
        }
        let source = cursor
            .next_byte()
            .ok_or(SourceProjectionSessionError::Invalid(
                "projection cursor reached source EOF",
            ))?;
        if source.root != self.descriptor.root || source.offset != absolute {
            return Err(SourceProjectionSessionError::Invalid(
                "projection cursor returned a crossed source byte",
            ));
        }
        self.receipt.source_bytes_read = self.receipt.source_bytes_read.checked_add(1).ok_or(
            SourceProjectionSessionError::Overflow("projection source bytes read"),
        )?;
        Ok(source.byte)
    }

    fn retire_active_cursor(&mut self) -> Result<(), SourceProjectionSessionError> {
        let Some(cursor) = self.cursor.take() else {
            return Ok(());
        };
        let metrics = cursor.metrics();
        self.receipt.chunk_loads =
            self.receipt
                .chunk_loads
                .checked_add(u64::try_from(metrics.chunk_loads).map_err(|_| {
                    SourceProjectionSessionError::Overflow("projection chunk loads")
                })?)
                .ok_or(SourceProjectionSessionError::Overflow(
                    "projection chunk loads",
                ))?;
        self.receipt.chunk_bytes_copied =
            self.receipt
                .chunk_bytes_copied
                .checked_add(u64::try_from(metrics.chunk_bytes_copied).map_err(|_| {
                    SourceProjectionSessionError::Overflow("projection chunk bytes")
                })?)
                .ok_or(SourceProjectionSessionError::Overflow(
                    "projection chunk bytes",
                ))?;
        self.receipt.maximum_chunk_bytes = self
            .receipt
            .maximum_chunk_bytes
            .max(metrics.maximum_chunk_bytes);
        self.receipt.maximum_cursor_scratch_bytes = self
            .receipt
            .maximum_cursor_scratch_bytes
            .max(cursor.retained_scratch_bytes());
        Ok(())
    }

    pub(crate) fn finish_pass(
        &mut self,
        expected: SourceSnapshotDescriptor,
        cursor_nonce: u64,
    ) -> Result<(), SourceProjectionSessionError> {
        if !self.pass_active || expected != self.descriptor || cursor_nonce != self.cursor_nonce {
            return Err(SourceProjectionSessionError::Invalid(
                "projection pass finish crossed source or cursor authority",
            ));
        }
        self.retire_active_cursor()?;
        self.pass_active = false;
        self.pass_lower_bound = None;
        self.receipt.passes_finished = self.receipt.passes_finished.checked_add(1).ok_or(
            SourceProjectionSessionError::Overflow("projection finished pass count"),
        )?;
        Ok(())
    }

    /// Cancels one active pass while preserving the session for another
    /// authenticated pass. This is also the fail-closed cleanup used by a
    /// dropped parser-side pass adapter.
    pub(crate) fn cancel_pass(
        &mut self,
        expected: SourceSnapshotDescriptor,
        cursor_nonce: u64,
    ) -> Result<(), SourceProjectionSessionError> {
        if !self.pass_active || expected != self.descriptor || cursor_nonce != self.cursor_nonce {
            return Err(SourceProjectionSessionError::Invalid(
                "projection pass cancel crossed source or cursor authority",
            ));
        }
        self.retire_active_cursor()?;
        self.pass_active = false;
        self.pass_lower_bound = None;
        self.receipt.passes_cancelled = self.receipt.passes_cancelled.checked_add(1).ok_or(
            SourceProjectionSessionError::Overflow("projection cancelled pass count"),
        )?;
        Ok(())
    }

    /// Retires the sole extra root lease and returns its complete scratch/read
    /// accounting. A live pass makes retirement fail closed.
    pub(crate) fn retire(
        self,
        expected: SourceSnapshotDescriptor,
        cursor_nonce: u64,
    ) -> Result<SourceProjectionSessionReceipt, SourceProjectionSessionError> {
        if self.pass_active
            || self.cursor.is_some()
            || self.pass_lower_bound.is_some()
            || expected != self.descriptor
            || cursor_nonce != self.cursor_nonce
        {
            return Err(SourceProjectionSessionError::Invalid(
                "projection session retired before its pass completed",
            ));
        }
        let Self {
            descriptor: _,
            cursor_nonce: _,
            root,
            cursor: _,
            pass_active: _,
            pass_lower_bound: _,
            receipt,
        } = self;
        drop(root);
        Ok(receipt)
    }

    /// Cancels the transaction-local session, including a live pass, and
    /// releases its sole source-root role. The exact source/cursor join is
    /// still required so a crossed actor cannot dispose another transaction.
    pub(crate) fn cancel(
        mut self,
        expected: SourceSnapshotDescriptor,
        cursor_nonce: u64,
    ) -> Result<SourceProjectionSessionReceipt, SourceProjectionSessionError> {
        if expected != self.descriptor || cursor_nonce != self.cursor_nonce {
            return Err(SourceProjectionSessionError::Invalid(
                "projection session cancel crossed source or cursor authority",
            ));
        }
        if self.pass_active {
            self.cancel_pass(expected, cursor_nonce)?;
        } else if self.cursor.is_some() || self.pass_lower_bound.is_some() {
            return Err(SourceProjectionSessionError::Invalid(
                "projection session cancel found partial pass state",
            ));
        }
        let Self {
            descriptor: _,
            cursor_nonce: _,
            root,
            cursor: _,
            pass_active: _,
            pass_lower_bound: _,
            receipt,
        } = self;
        drop(root);
        Ok(receipt)
    }

    #[cfg(test)]
    pub(crate) fn weak_root_for_test(&self) -> Weak<CropSnapshotLease> {
        Arc::downgrade(&self.root)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditRecord {
    pub transition: SourceTransition,
    pub old_len: usize,
    pub new_len: usize,
    pub edited_old: Range<usize>,
    pub edited_new: Range<usize>,
    pub prefix: UnchangedRegion,
    pub suffix: UnchangedRegion,
}

/// Persistent scalar-lineage storage. A mapping job snapshots one root in O(1)
/// and source edits path-copy only the slot addressed by the next revision.
///
/// The tree is a fixed-width segment tree over ring slots. A leaf owns an
/// [`Arc<EditRecord>`], never a Crop root or source slice. Old jobs may
/// therefore retain an exact scalar view while the live ring overwrites the
/// same logical slots in a newer tree version.
#[derive(Debug)]
enum LineageNode {
    Branch {
        left: Option<Arc<Self>>,
        right: Option<Arc<Self>>,
    },
    Record(Arc<EditRecord>),
}

#[derive(Debug)]
struct TreeInsert {
    root: Arc<LineageNode>,
    shape_nodes_added: usize,
    allocated_nodes: usize,
}

fn insert_record(
    node: Option<&Arc<LineageNode>>,
    bounds: Range<usize>,
    slot: usize,
    record: Arc<EditRecord>,
) -> TreeInsert {
    debug_assert!(bounds.start <= slot && slot < bounds.end);
    if bounds.len() == 1 {
        return TreeInsert {
            root: Arc::new(LineageNode::Record(record)),
            shape_nodes_added: usize::from(node.is_none()),
            allocated_nodes: 1,
        };
    }

    let (old_left, old_right) = match node.map(Arc::as_ref) {
        Some(LineageNode::Branch { left, right }) => (left.as_ref(), right.as_ref()),
        Some(LineageNode::Record(_)) => {
            unreachable!("lineage record appeared above a leaf slot")
        }
        None => (None, None),
    };
    let midpoint = bounds.start + bounds.len() / 2;
    let branch_was_missing = usize::from(node.is_none());
    if slot < midpoint {
        let inserted = insert_record(old_left, bounds.start..midpoint, slot, record);
        TreeInsert {
            root: Arc::new(LineageNode::Branch {
                left: Some(inserted.root),
                right: old_right.cloned(),
            }),
            shape_nodes_added: branch_was_missing + inserted.shape_nodes_added,
            allocated_nodes: 1 + inserted.allocated_nodes,
        }
    } else {
        let inserted = insert_record(old_right, midpoint..bounds.end, slot, record);
        TreeInsert {
            root: Arc::new(LineageNode::Branch {
                left: old_left.cloned(),
                right: Some(inserted.root),
            }),
            shape_nodes_added: branch_was_missing + inserted.shape_nodes_added,
            allocated_nodes: 1 + inserted.allocated_nodes,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TreeLookup<'a> {
    record: Option<&'a EditRecord>,
    nodes_read: usize,
}

fn lookup_record(
    mut node: Option<&Arc<LineageNode>>,
    mut bounds: Range<usize>,
    slot: usize,
) -> TreeLookup<'_> {
    debug_assert!(bounds.start <= slot && slot < bounds.end);
    let mut nodes_read = 0;
    loop {
        let Some(current) = node.map(Arc::as_ref) else {
            return TreeLookup {
                record: None,
                nodes_read,
            };
        };
        nodes_read += 1;
        if bounds.len() == 1 {
            return match current {
                LineageNode::Record(record) => TreeLookup {
                    record: Some(record),
                    nodes_read,
                },
                LineageNode::Branch { .. } => {
                    unreachable!("lineage branch appeared at a leaf slot")
                }
            };
        }

        let LineageNode::Branch { left, right } = current else {
            unreachable!("lineage record appeared above a leaf slot")
        };
        let midpoint = bounds.start + bounds.len() / 2;
        if slot < midpoint {
            node = left.as_ref();
            bounds.end = midpoint;
        } else {
            node = right.as_ref();
            bounds.start = midpoint;
        }
    }
}

fn maximum_lookup_nodes(capacity: usize) -> usize {
    let mut widest_branch = capacity;
    let mut nodes = 1;
    while widest_branch > 1 {
        widest_branch = widest_branch.div_ceil(2);
        nodes += 1;
    }
    nodes
}

fn revision_slot(revision: SourceRevision, capacity: usize) -> usize {
    let capacity = u64::try_from(capacity).expect("lineage capacity fits in u64");
    usize::try_from(revision.0 % capacity).expect("lineage slot fits in usize")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineageRetention {
    pub records: usize,
    pub capacity: usize,
    pub retained_source_roots: usize,
    pub tree_nodes: usize,
    pub maximum_tree_nodes: usize,
    pub last_update_tree_nodes: usize,
    pub maximum_update_tree_nodes: usize,
}

#[derive(Debug)]
pub struct EditLineageRing {
    root: Option<Arc<LineageNode>>,
    capacity: usize,
    records: usize,
    tree_nodes: usize,
    last_update_tree_nodes: usize,
    maximum_update_tree_nodes: usize,
}

/// Fully allocated next lineage state. Publishing it performs only field
/// assignments, so no logical failure or allocation remains after preflight.
#[derive(Debug)]
struct PreparedLineagePush {
    root: Arc<LineageNode>,
    records: usize,
    tree_nodes: usize,
    last_update_tree_nodes: usize,
}

impl EditLineageRing {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "lineage capacity must be positive");
        Self {
            root: None,
            capacity,
            records: 0,
            tree_nodes: 0,
            last_update_tree_nodes: 0,
            maximum_update_tree_nodes: maximum_lookup_nodes(capacity),
        }
    }

    fn prepare_push(&self, record: Arc<EditRecord>) -> PreparedLineagePush {
        let slot = revision_slot(record.transition.base_revision, self.capacity);
        let inserted = insert_record(self.root.as_ref(), 0..self.capacity, slot, record);
        let tree_nodes = self
            .tree_nodes
            .checked_add(inserted.shape_nodes_added)
            .expect("lineage shape count is bounded by its configured capacity");
        let prepared = PreparedLineagePush {
            root: inserted.root,
            records: self.records.saturating_add(1).min(self.capacity),
            tree_nodes,
            last_update_tree_nodes: inserted.allocated_nodes,
        };
        debug_assert!(prepared.last_update_tree_nodes <= self.maximum_update_tree_nodes);
        debug_assert!(prepared.tree_nodes <= self.capacity.saturating_mul(2).saturating_sub(1));
        prepared
    }

    fn commit_push(&mut self, prepared: PreparedLineagePush) {
        self.root = Some(prepared.root);
        self.records = prepared.records;
        self.tree_nodes = prepared.tree_nodes;
        self.last_update_tree_nodes = prepared.last_update_tree_nodes;
    }

    #[must_use]
    pub fn retention(&self) -> LineageRetention {
        LineageRetention {
            records: self.records,
            capacity: self.capacity,
            retained_source_roots: 0,
            tree_nodes: self.tree_nodes,
            maximum_tree_nodes: self.capacity.saturating_mul(2).saturating_sub(1),
            last_update_tree_nodes: self.last_update_tree_nodes,
            maximum_update_tree_nodes: self.maximum_update_tree_nodes,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptedEdit {
    pub transition: SourceTransition,
    pub record: Arc<EditRecord>,
}

/// Opaque ownership of one Crop root whose destruction has been removed from
/// the edit-admission turn.
///
/// This authority is deliberately neither [`Clone`] nor a source-reading
/// lease. The host takes it from the live document and drops it on its chosen
/// native disposer or Web/Wasm idle lane. The descriptor is observational and
/// cannot be upgraded back into parser authority.
#[derive(Debug)]
#[must_use = "a retired source root must be transferred to the host's disposal lane"]
pub struct RetiredSourceRoot {
    descriptor: SourceSnapshotDescriptor,
    root: Arc<CropSnapshotLease>,
    line_index: CommonMarkLineIndex,
}

impl RetiredSourceRoot {
    fn new(
        revision: SourceRevision,
        root: Arc<CropSnapshotLease>,
        line_index: CommonMarkLineIndex,
    ) -> Self {
        let descriptor = SourceSnapshotDescriptor {
            revision,
            root: root.identity(),
            bytes: root.len_bytes(),
        };
        debug_assert_eq!(line_index.total_bytes(), root.len_bytes());
        debug_assert_eq!(line_index.total_utf16(), root.len_utf16());
        Self {
            descriptor,
            root,
            line_index,
        }
    }

    #[must_use]
    pub const fn descriptor(&self) -> SourceSnapshotDescriptor {
        self.descriptor
    }

    /// Runs this root's ownership release on the caller's current lane. Hosts
    /// call this only after moving the capability to their native disposer or
    /// Web/Wasm idle callback.
    pub fn dispose(self) {
        let Self {
            descriptor: _,
            root,
            line_index,
        } = self;
        drop((root, line_index));
    }

    /// Non-owning lifetime witness compiled only into crate unit tests.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn weak_observer_for_testing(&self) -> Weak<CropSnapshotLease> {
        Arc::downgrade(&self.root)
    }
}

/// Non-forgeable, fully allocated next source state. Only the live-document
/// actor can pair this with a coordinator admission and consume it.
#[derive(Debug)]
pub(crate) struct PreparedSourceEdit {
    expected: SourceSnapshotDescriptor,
    next_root: Arc<CropSnapshotLease>,
    next_line_index: CommonMarkLineIndex,
    record: Arc<EditRecord>,
    lineage: PreparedLineagePush,
}

impl PreparedSourceEdit {
    #[must_use]
    pub(crate) fn transition(&self) -> SourceTransition {
        self.record.transition
    }

    /// Extracts the unpublished Crop root so a post-prepare admission failure
    /// does not synchronously destroy an arbitrarily large replacement tree.
    pub(crate) fn into_retired_root(self) -> RetiredSourceRoot {
        let Self {
            expected: _,
            next_root,
            next_line_index,
            record,
            lineage: _,
        } = self;
        RetiredSourceRoot::new(
            record.transition.target_revision,
            next_root,
            next_line_index,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceStoreError {
    Source(SourceError),
    StaleRevision {
        expected: SourceRevision,
        actual: SourceRevision,
    },
    SnapshotMismatch {
        expected: SourceSnapshotDescriptor,
        actual: SourceSnapshotDescriptor,
    },
    NotPhysicalLineStart {
        offset: usize,
    },
    RevisionExhausted,
    SourceRootExhausted,
    InvalidLineageCapacity,
}

impl From<SourceError> for SourceStoreError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

impl fmt::Display for SourceStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => error.fmt(formatter),
            Self::StaleRevision { expected, actual } => write!(
                formatter,
                "source edit expected revision {expected:?}, current is {actual:?}"
            ),
            Self::SnapshotMismatch { expected, actual } => write!(
                formatter,
                "source edit expected snapshot {expected:?}, current is {actual:?}"
            ),
            Self::NotPhysicalLineStart { offset } => {
                write!(
                    formatter,
                    "source byte {offset} is not a physical-line start"
                )
            }
            Self::RevisionExhausted => formatter.write_str("source revision exhausted"),
            Self::SourceRootExhausted => formatter.write_str("source root identity exhausted"),
            Self::InvalidLineageCapacity => {
                formatter.write_str("source lineage capacity must be positive")
            }
        }
    }
}

impl std::error::Error for SourceStoreError {}

/// Owns exactly one current source root plus scalar-only edit history.
#[derive(Debug)]
pub struct SourceStore {
    revision: SourceRevision,
    root: Arc<CropSnapshotLease>,
    line_index: CommonMarkLineIndex,
    lineage: EditLineageRing,
}

impl SourceStore {
    #[must_use]
    pub fn new(text: &str, lineage_capacity: usize) -> Self {
        Self {
            revision: SourceRevision(0),
            root: CropSnapshotLease::from_text(text),
            line_index: CommonMarkLineIndex::from_text(text),
            lineage: EditLineageRing::new(lineage_capacity),
        }
    }

    /// Fallible production constructor. Unlike the proof-harness constructor,
    /// source-root exhaustion and invalid retention configuration are reported
    /// without partially constructing a live document.
    pub(crate) fn try_new(text: &str, lineage_capacity: usize) -> Result<Self, SourceStoreError> {
        if lineage_capacity == 0 {
            return Err(SourceStoreError::InvalidLineageCapacity);
        }
        let identity = try_mint_source_root().ok_or(SourceStoreError::SourceRootExhausted)?;
        Ok(Self {
            revision: SourceRevision(0),
            root: CropSnapshotLease::from_text_with_identity(text, identity),
            line_index: CommonMarkLineIndex::from_text(text),
            lineage: EditLineageRing::new(lineage_capacity),
        })
    }

    #[must_use]
    pub const fn revision(&self) -> SourceRevision {
        self.revision
    }

    #[must_use]
    pub fn root_id(&self) -> SourceRootId {
        self.root.identity
    }

    #[must_use]
    pub fn descriptor(&self) -> SourceSnapshotDescriptor {
        SourceSnapshotDescriptor {
            revision: self.revision,
            root: self.root.identity,
            bytes: self.root.len_bytes(),
        }
    }

    /// Issues the sole parser/build lease. Production callers reach this only
    /// through the worker-owned live-document actor.
    #[must_use]
    pub(crate) fn issue_parser_lease(&self) -> SourceSnapshotLease {
        SourceSnapshotLease {
            revision: self.revision,
            root: Arc::clone(&self.root),
        }
    }

    /// Issues the second actor-owned cursor role used for bounded recognition
    /// lookahead. No cloneable query wrapper exists between the store and the
    /// candidate that receives this cursor.
    pub(crate) fn issue_recognition_cursor(&self) -> (usize, CropSourceCursor) {
        (self.root.len_utf16(), self.root.cursor())
    }

    /// Issues one transaction-local active-Paragraph replay session. The
    /// session owns exactly one additional root lease and may hold at most one
    /// forward-only cursor per explicitly delimited pass. CandidateWriter must
    /// retire it before canonical replacement begins.
    pub(crate) fn issue_projection_session(
        &self,
        cursor_nonce: u64,
    ) -> Result<SourceProjectionSession, SourceProjectionSessionError> {
        if cursor_nonce == 0 {
            return Err(SourceProjectionSessionError::Invalid(
                "projection cursor nonce must be nonzero",
            ));
        }
        Ok(SourceProjectionSession {
            descriptor: self.descriptor(),
            cursor_nonce,
            root: Arc::clone(&self.root),
            cursor: None,
            pass_active: false,
            pass_lower_bound: None,
            receipt: SourceProjectionSessionReceipt {
                descriptor: self.descriptor(),
                cursor_nonce,
                passes_started: 0,
                passes_finished: 0,
                passes_cancelled: 0,
                cursor_roles_minted: 0,
                forward_cursor_jumps: 0,
                source_bytes_read: 0,
                chunk_loads: 0,
                chunk_bytes_copied: 0,
                maximum_chunk_bytes: 0,
                maximum_cursor_scratch_bytes: 0,
                maximum_live_cursor_roles: 0,
            },
        })
    }

    /// Atomically validates and mints the two source-cursor roles for one
    /// same-build line-boundary resume.
    ///
    /// No lease or arbitrary-offset constructor escapes this operation. The
    /// returned cursors share this exact immutable root and begin at the same
    /// validated scalar cut. A non-line-start EOF cut is accepted only because
    /// the source-ledger continuation separately proves that the bare-EOF line
    /// was fully acknowledged.
    #[allow(dead_code)] // The actor seam is production-shaped but not yet composite-wired.
    pub(crate) fn issue_resume_cursor_pair(
        &self,
        offset: usize,
    ) -> Result<SourceResumeCursorPair, SourceError> {
        if offset > self.root.len_bytes() {
            return Err(SourceError::InvalidRange);
        }
        let check = self.root.check_physical_line_start(offset);
        if !check.scalar_boundary {
            return Err(SourceError::NotCharBoundary(offset));
        }
        if !check.physical_line_start && offset != self.root.len_bytes() {
            return Err(SourceError::InvalidRange);
        }
        Ok(SourceResumeCursorPair {
            descriptor: self.descriptor(),
            total_utf16: self.root.len_utf16(),
            offset,
            physical_line_start: check.physical_line_start,
            authoritative: CropSourceCursor::new(Arc::clone(&self.root), offset),
            recognition: CropSourceCursor::new(Arc::clone(&self.root), offset),
        })
    }

    /// Cloneable source observation with no build/manifest authority.
    #[must_use]
    pub fn query_snapshot(&self) -> SourceQuerySnapshot {
        SourceQuerySnapshot {
            revision: self.revision,
            root: Arc::clone(&self.root),
        }
    }

    /// Production observation path. The returned value borrows this store and
    /// never participates in Crop-root ownership.
    #[must_use]
    pub(crate) fn query_view(&self) -> SourceQueryView<'_> {
        SourceQueryView {
            revision: self.revision,
            root: self.root.as_ref(),
        }
    }

    /// Bounded scalar/physical-line validation for storage-owned checkpoint
    /// resolution. The copyable result is observation only; it cannot start a
    /// lineage job or mint a parser checkpoint by itself.
    pub(crate) fn check_physical_line_start(&self, offset: usize) -> PhysicalLineStartCheck {
        self.root.check_physical_line_start(offset)
    }

    /// Certifies one scalar-exact byte cut against the current source root.
    ///
    /// The certificate is deliberately non-cloneable and the query rechecks
    /// its complete descriptor, so a cut cannot survive an edit or cross to a
    /// different document merely because its scalar offset happens to match.
    #[allow(dead_code)] // Wired by the generic restart coordinator after this source proof gate.
    pub(crate) fn certify_current_byte_cut(
        &self,
        snapshot: SourceSnapshotDescriptor,
        offset: usize,
    ) -> Result<CertifiedSourceByteCut, SourceStoreError> {
        let actual = self.descriptor();
        if snapshot != actual {
            return Err(SourceStoreError::SnapshotMismatch {
                expected: snapshot,
                actual,
            });
        }
        if offset > self.root.len_bytes() {
            return Err(SourceError::InvalidRange.into());
        }
        if !self.root.is_char_boundary(offset) {
            return Err(SourceError::NotCharBoundary(offset).into());
        }
        Ok(CertifiedSourceByteCut { snapshot, offset })
    }

    /// Resolves the `CommonMark` physical-line predecessor at a certified cut.
    ///
    /// The persistent summary supplies every complete subtree. At most one
    /// summary leaf (4 KiB) and two bytes adjacent to the cut are read from the
    /// authoritative Crop root. CRLF is one ending even when its bytes straddle
    /// either a Crop chunk or line-index leaf boundary.
    #[allow(dead_code)] // Wired by the generic restart coordinator after this source proof gate.
    #[allow(clippy::needless_pass_by_value)] // Consuming the certificate is its replay boundary.
    pub(crate) fn query_physical_line_at_cut(
        &self,
        cut: CertifiedSourceByteCut,
    ) -> Result<SourcePhysicalLineQuery, SourceStoreError> {
        let actual = self.descriptor();
        if cut.snapshot != actual {
            return Err(SourceStoreError::SnapshotMismatch {
                expected: cut.snapshot,
                actual,
            });
        }
        if cut.offset > self.root.len_bytes() {
            return Err(SourceError::InvalidRange.into());
        }
        if !self.root.is_char_boundary(cut.offset) {
            return Err(SourceError::NotCharBoundary(cut.offset).into());
        }

        let (summary, index_receipt) = self.line_index.prefix_summary(&self.root, cut.offset);
        let previous_byte = if cut.offset == 0 {
            None
        } else {
            self.root.root.byte_slice(..cut.offset).bytes().next_back()
        };
        let mut adjacent_bytes_read = usize::from(previous_byte.is_some());
        let next_byte = if previous_byte == Some(b'\r') && cut.offset < self.root.len_bytes() {
            adjacent_bytes_read += 1;
            self.root.root.byte_slice(cut.offset..).bytes().next()
        } else {
            None
        };
        let inside_crlf = previous_byte == Some(b'\r') && next_byte == Some(b'\n');
        let physical_line_start = cut.offset == 0
            || previous_byte == Some(b'\n')
            || (previous_byte == Some(b'\r') && !inside_crlf);
        let line_ordinal = summary
            .line_breaks
            .checked_sub(usize::from(inside_crlf))
            .ok_or(SourceError::InvalidRange)?;
        let previous = if physical_line_start && cut.offset != 0 {
            let metric = summary
                .last_completed_content
                .ok_or(SourceError::InvalidRange)?;
            Some(SourcePhysicalLinePredecessor {
                content_bytes: u64::try_from(metric.bytes)
                    .map_err(|_| SourceError::InvalidRange)?,
                content_utf16: u64::try_from(metric.utf16)
                    .map_err(|_| SourceError::InvalidRange)?,
            })
        } else {
            None
        };
        let mut receipt = SourcePhysicalLineQueryReceipt::from(index_receipt);
        receipt.adjacent_bytes_read = adjacent_bytes_read;
        Ok(SourcePhysicalLineQuery {
            snapshot: actual,
            offset: cut.offset,
            line_ordinal: u64::try_from(line_ordinal).map_err(|_| SourceError::InvalidRange)?,
            physical_line_start,
            previous,
            receipt,
        })
    }

    /// Resolves the exact extent of one physical line from the persistent
    /// `CommonMark` line index.
    ///
    /// The caller supplies the complete current snapshot descriptor and an
    /// exact physical-line start. Complete suffix subtrees answer the endpoint
    /// query from associative summaries; at most the one 4 KiB index leaf
    /// containing `start` is scanned. The result is observational only and
    /// mints no source-reading or writer authority.
    pub fn query_physical_line_descriptor(
        &self,
        expected: SourceSnapshotDescriptor,
        start: usize,
    ) -> Result<SourcePhysicalLineDescriptor, SourceStoreError> {
        let actual = self.descriptor();
        if expected != actual {
            return Err(SourceStoreError::SnapshotMismatch { expected, actual });
        }
        if start > self.root.len_bytes() {
            return Err(SourceError::InvalidRange.into());
        }
        let start_check = self.root.check_physical_line_start(start);
        if !start_check.scalar_boundary {
            return Err(SourceError::NotCharBoundary(start).into());
        }
        if !start_check.physical_line_start {
            return Err(SourceStoreError::NotPhysicalLineStart { offset: start });
        }

        let (suffix, index_receipt) = self.line_index.suffix_summary(&self.root, start);
        let content_end = start
            .checked_add(suffix.prefix_content.bytes)
            .ok_or(SourceError::InvalidRange)?;
        let (ending, ending_bytes_read) = if suffix.line_breaks == 0 {
            if content_end != self.root.len_bytes() {
                return Err(SourceError::InvalidRange.into());
            }
            (SourcePhysicalLineEnding::BareEof, 0)
        } else {
            let first = self
                .root
                .root
                .byte_slice(content_end..)
                .bytes()
                .next()
                .ok_or(SourceError::InvalidRange)?;
            match first {
                b'\n' => (SourcePhysicalLineEnding::Lf, 1),
                b'\r' => {
                    let next = self.root.root.byte_slice(content_end + 1..).bytes().next();
                    if next == Some(b'\n') {
                        (SourcePhysicalLineEnding::CrLf, 2)
                    } else {
                        (
                            SourcePhysicalLineEnding::LoneCr,
                            1 + usize::from(next.is_some()),
                        )
                    }
                }
                _ => return Err(SourceError::InvalidRange.into()),
            }
        };
        let ending_bytes = ending.bytes();
        let end = content_end
            .checked_add(ending_bytes)
            .filter(|end| *end <= self.root.len_bytes())
            .ok_or(SourceError::InvalidRange)?;
        let content_utf16 = suffix.prefix_content.utf16;
        let physical_utf16 = content_utf16
            .checked_add(ending_bytes)
            .ok_or(SourceError::InvalidRange)?;
        let mut receipt = SourcePhysicalLineDescriptorReceipt::from(index_receipt);
        receipt.adjacent_bytes_read = start_check
            .adjacent_bytes_read
            .checked_add(ending_bytes_read)
            .ok_or(SourceError::InvalidRange)?;

        Ok(SourcePhysicalLineDescriptor {
            source: actual,
            start,
            content_end,
            end,
            content_utf16,
            physical_utf16,
            ending,
            receipt,
        })
    }

    #[allow(dead_code)] // Observable proof receipt for the upcoming composite gate.
    pub(crate) fn line_index_last_update(&self) -> SourceLineIndexUpdateReceipt {
        self.line_index.last_update().into()
    }

    #[allow(dead_code)] // Observable proof receipt for the upcoming composite gate.
    pub(crate) fn line_index_retention(&self) -> SourceLineIndexRetention {
        self.line_index.retention().into()
    }

    /// Borrows one exact byte from the current source without minting a source
    /// lease, cursor, snapshot owner, or lineage capability. The copyable
    /// result is observation only; higher-level storage must still join it with
    /// source-lineage and manifest authority before reuse.
    pub(crate) fn observe_byte_at(&self, offset: usize) -> Result<Option<SourceByte>, SourceError> {
        if offset > self.root.len_bytes() {
            return Err(SourceError::InvalidRange);
        }
        Ok(self
            .root
            .root
            .byte_slice(offset..)
            .bytes()
            .next()
            .map(|byte| SourceByte {
                root: self.root.identity,
                offset,
                byte,
            }))
    }

    /// Observes the byte and UTF-16 measures of one scalar-exact current
    /// source prefix without materializing or retaining that prefix.
    pub(crate) fn observe_prefix_metric_at(
        &self,
        offset: usize,
    ) -> Result<SourcePrefixMetric, SourceError> {
        if offset > self.root.len_bytes() {
            return Err(SourceError::InvalidRange);
        }
        if !self.root.is_char_boundary(offset) {
            return Err(SourceError::NotCharBoundary(offset));
        }
        Ok(SourcePrefixMetric {
            root: self.root.identity,
            bytes: offset,
            utf16: self.root.root.utf16_code_unit_of_byte(offset),
        })
    }

    /// Re-derives the donor cursor scalars for the narrow LF-only Setext
    /// restart. The result is observation, not restart authority; callers must
    /// still consume it through a retained-prefix lineage proof.
    pub(crate) fn observe_lf_line_boundary_at(
        &self,
        offset: usize,
    ) -> Result<SourceLfLineBoundaryMetric, SourceError> {
        if offset == 0 || offset > self.root.len_bytes() {
            return Err(SourceError::InvalidRange);
        }
        if !self.root.is_char_boundary(offset) {
            return Err(SourceError::NotCharBoundary(offset));
        }
        let previous = self
            .root
            .root
            .byte_slice(..offset)
            .bytes()
            .next_back()
            .ok_or(SourceError::InvalidRange)?;
        if previous != b'\n' {
            return Err(SourceError::InvalidRange);
        }
        let adjacent_bytes_read = if offset >= 2 {
            let before_lf = self
                .root
                .root
                .byte_slice(..offset - 1)
                .bytes()
                .next_back()
                .ok_or(SourceError::InvalidRange)?;
            if before_lf == b'\r' {
                return Err(SourceError::InvalidRange);
            }
            2
        } else {
            1
        };
        let line = self.root.root.line_of_byte(offset);
        if line == 0 {
            return Err(SourceError::InvalidRange);
        }
        let previous_start = self.root.root.byte_of_line(line - 1);
        let previous_content_bytes = offset
            .checked_sub(previous_start)
            .and_then(|width| width.checked_sub(1))
            .ok_or(SourceError::InvalidRange)?;
        Ok(SourceLfLineBoundaryMetric {
            root: self.root.identity,
            offset,
            completed_line_ordinal: u64::try_from(line).map_err(|_| SourceError::InvalidRange)?,
            previous_content_bytes: u64::try_from(previous_content_bytes)
                .map_err(|_| SourceError::InvalidRange)?,
            adjacent_bytes_read,
        })
    }

    #[must_use]
    pub const fn lineage(&self) -> &EditLineageRing {
        &self.lineage
    }

    pub fn apply_edit(
        &mut self,
        expected_revision: SourceRevision,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<AcceptedEdit, SourceStoreError> {
        if expected_revision != self.revision {
            return Err(SourceStoreError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let expected = self.descriptor();
        let prepared = self.prepare_edit(expected, range, replacement)?;
        let (accepted, retired) = self.commit_prepared_edit(prepared);
        // This public method is a source/lineage proof harness, not the live
        // editor admission path. Make its synchronous destruction explicit.
        drop(retired);
        Ok(accepted)
    }

    /// Validates and fully allocates one exact edit without changing the live
    /// source root, revision, or lineage. The returned authority is linear and
    /// crate-private so scalar descriptors cannot be upgraded into commits.
    pub(crate) fn prepare_edit(
        &self,
        expected: SourceSnapshotDescriptor,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<PreparedSourceEdit, SourceStoreError> {
        let actual = self.descriptor();
        if expected != actual {
            return Err(SourceStoreError::SnapshotMismatch { expected, actual });
        }
        let target_revision = SourceRevision(
            self.revision
                .0
                .checked_add(1)
                .ok_or(SourceStoreError::RevisionExhausted)?,
        );
        validate_range(&self.root, &range)?;
        let next_identity = try_mint_source_root().ok_or(SourceStoreError::SourceRootExhausted)?;
        let old_len = self.root.len_bytes();
        let old_root = self.root.identity;
        let next_line_index = self
            .line_index
            .edited(&self.root, range.clone(), replacement);
        let (next_root, provenance) = self.root.edit_validated(range, replacement, next_identity);
        debug_assert_eq!(next_line_index.total_bytes(), next_root.len_bytes());
        debug_assert_eq!(next_line_index.total_utf16(), next_root.len_utf16());
        let transition = SourceTransition {
            base_revision: self.revision,
            target_revision,
            base_root: old_root,
            result_root: next_root.identity,
        };
        let record = Arc::new(EditRecord {
            transition,
            old_len,
            new_len: next_root.len_bytes(),
            edited_old: provenance.edited_old,
            edited_new: provenance.edited_new,
            prefix: provenance.prefix,
            suffix: provenance.suffix,
        });
        let lineage = self.lineage.prepare_push(Arc::clone(&record));
        Ok(PreparedSourceEdit {
            expected,
            next_root,
            next_line_index,
            record,
            lineage,
        })
    }

    /// Publishes a previously prepared edit with no validation, allocation, or
    /// other fallible operation after the initial invariant check.
    pub(crate) fn commit_prepared_edit(
        &mut self,
        prepared: PreparedSourceEdit,
    ) -> (AcceptedEdit, RetiredSourceRoot) {
        assert_eq!(
            self.descriptor(),
            prepared.expected,
            "prepared source edit must commit to the store that issued it"
        );
        let PreparedSourceEdit {
            expected: _,
            next_root,
            next_line_index,
            record,
            lineage,
        } = prepared;
        let transition = record.transition;
        let retired_revision = self.revision;
        self.lineage.commit_push(lineage);
        let retired_root = std::mem::replace(&mut self.root, next_root);
        let retired_line_index = std::mem::replace(&mut self.line_index, next_line_index);
        self.revision = transition.target_revision;
        (
            AcceptedEdit { transition, record },
            RetiredSourceRoot::new(retired_revision, retired_root, retired_line_index),
        )
    }

    pub fn map_range_from(
        &self,
        revision: SourceRevision,
        range: Range<usize>,
    ) -> Result<LineageMapJob, LineageError> {
        LineageMapJob::range(self, revision, range)
    }

    pub fn map_boundary_from(
        &self,
        revision: SourceRevision,
        offset: usize,
        affinity: BoundaryAffinity,
    ) -> Result<LineageMapJob, LineageError> {
        LineageMapJob::boundary(self, revision, offset, affinity)
    }

    /// Starts the storage half of one suffix-adoption proof.
    ///
    /// Unlike four independent [`LineageMapJob`]s, this job freezes one
    /// immutable lineage snapshot and validates each retained edit record once
    /// while mapping all adoption regions. It is crate-private because parser
    /// or composer code must never upgrade echoed offsets into storage reuse
    /// authority.
    #[allow(dead_code)] // Wired by the next storage-derived BaseAdoptionProof stage.
    pub(crate) fn begin_lineage_adoption_bundle(
        &self,
        from: SourceSnapshotDescriptor,
        old_restart: usize,
        old_convergence: usize,
        convergence_affinity: BoundaryAffinity,
    ) -> Result<LineageAdoptionBundleJob, LineageAdoptionBundleError> {
        LineageAdoptionBundleJob::new(
            self,
            from,
            old_restart,
            old_convergence,
            convergence_affinity,
        )
    }

    /// Selects one provenance-chosen restart or the independently proven zero
    /// fallback in a single immutable lineage pass.
    #[allow(dead_code)] // Consumed by candidate initialization in the next wiring stage.
    pub(crate) fn begin_restart_selection(
        &self,
        from: SourceSnapshotDescriptor,
        preferred_restart: usize,
    ) -> Result<RestartSelectionJob, RestartSelectionError> {
        RestartSelectionJob::new(self, from, preferred_restart)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryAffinity {
    Before,
    After,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MapValue {
    Range(Range<usize>),
    Boundary {
        offset: usize,
        affinity: BoundaryAffinity,
    },
}

/// Exact source snapshot identity carried by a source-store-derived mapping
/// proof. The descriptor is data; only [`LineageMappingProof`] certifies that
/// two descriptors were connected by the retained edit lineage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSnapshotDescriptor {
    pub revision: SourceRevision,
    pub root: SourceRootId,
    pub bytes: usize,
}

/// One unchanged range or boundary mapped through a validated source lineage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProvenSourceMapping {
    Range {
        from: Range<usize>,
        to: Range<usize>,
    },
    Boundary {
        from: usize,
        to: usize,
        affinity: BoundaryAffinity,
    },
}

/// Non-cloneable evidence minted only by a successfully completed
/// [`LineageMapJob`]. Callers can inspect its exact descriptors and mapping but
/// cannot construct or replay a proof from echoed revision/range scalars.
///
/// ```compile_fail
/// use flark_v3_runtime_slice::{
///     LineageMappingProof, ProvenSourceMapping, SourceRevision, SourceRootId,
///     SourceSnapshotDescriptor,
/// };
/// let descriptor = SourceSnapshotDescriptor {
///     revision: SourceRevision(1),
///     root: SourceRootId(1),
///     bytes: 1,
/// };
/// let _forged = LineageMappingProof {
///     from: descriptor,
///     to: descriptor,
///     mapping: ProvenSourceMapping::Range { from: 0..1, to: 0..1 },
/// };
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct LineageMappingProof {
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    mapping: ProvenSourceMapping,
}

impl LineageMappingProof {
    #[must_use]
    pub const fn from(&self) -> SourceSnapshotDescriptor {
        self.from
    }

    #[must_use]
    pub const fn to(&self) -> SourceSnapshotDescriptor {
        self.to
    }

    #[must_use]
    pub const fn mapping(&self) -> &ProvenSourceMapping {
        &self.mapping
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapStatus {
    Pending {
        processed_records: usize,
        remaining_records: usize,
    },
    ProvenRange(Range<usize>),
    ProvenBoundary(usize),
    Changed {
        at_revision: SourceRevision,
    },
    Failed(LineageError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineageError {
    FutureRevision,
    HistoryExpired,
    InvalidRange,
    BrokenChain,
}

impl fmt::Display for LineageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FutureRevision => formatter.write_str("lineage revision is in the future"),
            Self::HistoryExpired => formatter.write_str("lineage revision has expired"),
            Self::InvalidRange => formatter.write_str("lineage range is invalid"),
            Self::BrokenChain => formatter.write_str("lineage edit chain is broken"),
        }
    }
}

impl std::error::Error for LineageError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineageWorkMetrics {
    pub constructor_records_examined: usize,
    pub constructor_records_validated: usize,
    pub constructor_tree_nodes_read: usize,
    pub poll_records_examined: usize,
    pub poll_records_validated: usize,
    pub poll_mapping_attempts: usize,
    pub poll_records_mapped: usize,
    pub poll_tree_nodes_read: usize,
    pub maximum_tree_nodes_per_lookup: usize,
    pub records_copied: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineageSnapshotRetention {
    /// Scalar records reachable from this job's immutable root.
    pub records: usize,
    /// Scalar tree nodes that may be released when the last snapshot drops.
    /// This is a retention bound, not a claim of fuelled cancellation: the
    /// current `Arc` witness can synchronously release this many nodes.
    pub tree_nodes: usize,
    pub maximum_tree_nodes: usize,
    pub retained_source_roots: usize,
}

/// Region whose exact unchanged lineage is required before storage may retain
/// an old prefix or suffix. This is diagnostic data, not reuse authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Executable Stage 0; production adoption wiring is deliberately later.
pub(crate) enum LineageAdoptionRegion {
    RestartBoundary,
    RetainedPrefix,
    ConvergenceBoundary,
    RetainedTail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum LineageAdoptionBundleError {
    Lineage(LineageError),
    SnapshotMismatch {
        supplied: SourceSnapshotDescriptor,
        lineage: SourceSnapshotDescriptor,
    },
    InvalidBoundaries,
}

impl From<LineageError> for LineageAdoptionBundleError {
    fn from(error: LineageError) -> Self {
        Self::Lineage(error)
    }
}

/// Honest work receipt for the one-pass adoption lineage job.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct LineageAdoptionBundleMetrics {
    pub constructor_records_examined: usize,
    pub constructor_records_validated: usize,
    pub constructor_tree_nodes_read: usize,
    pub poll_records_examined: usize,
    pub poll_records_validated: usize,
    pub poll_mapping_attempts: usize,
    pub poll_mappings_succeeded: usize,
    pub poll_tree_nodes_read: usize,
    pub maximum_tree_nodes_per_lookup: usize,
    pub records_copied: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ProvenRetainedPrefix {
    Empty {
        from: usize,
        to: usize,
    },
    Range {
        from: Range<usize>,
        to: Range<usize>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum LineageAdoptionBundleStatus {
    Pending {
        processed_records: usize,
        remaining_records: usize,
    },
    Proven {
        restart: usize,
        prefix: ProvenRetainedPrefix,
        convergence: usize,
        tail: Range<usize>,
    },
    Changed {
        region: LineageAdoptionRegion,
        at_revision: SourceRevision,
    },
    Failed(LineageAdoptionBundleError),
}

/// Non-cloneable, storage-minted evidence that all source regions participating
/// in one prefix/current/suffix splice were mapped through one exact lineage
/// snapshot. Its scalar views can corroborate a storage proof, but only the
/// live-document actor may consume the value into adoption authority.
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct LineageAdoptionBundleProof {
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    restart: ProvenSourceMapping,
    prefix: ProvenRetainedPrefix,
    convergence: ProvenSourceMapping,
    tail: ProvenSourceMapping,
}

#[allow(dead_code)]
impl LineageAdoptionBundleProof {
    #[must_use]
    pub(crate) const fn from(&self) -> SourceSnapshotDescriptor {
        self.from
    }

    #[must_use]
    pub(crate) const fn to(&self) -> SourceSnapshotDescriptor {
        self.to
    }

    #[must_use]
    pub(crate) const fn restart(&self) -> &ProvenSourceMapping {
        &self.restart
    }

    #[must_use]
    pub(crate) const fn prefix(&self) -> &ProvenRetainedPrefix {
        &self.prefix
    }

    #[must_use]
    pub(crate) const fn convergence(&self) -> &ProvenSourceMapping {
        &self.convergence
    }

    #[must_use]
    pub(crate) const fn tail(&self) -> &ProvenSourceMapping {
        &self.tail
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct AdoptionMapState {
    original: MapValue,
    current: MapValue,
}

#[allow(dead_code)]
impl AdoptionMapState {
    fn new(value: MapValue) -> Self {
        Self {
            original: value.clone(),
            current: value,
        }
    }

    fn map(&mut self, record: &EditRecord) -> bool {
        let Some(mapped) = map_value(&self.current, record) else {
            return false;
        };
        self.current = mapped;
        true
    }

    fn proof(&self) -> Option<ProvenSourceMapping> {
        match (&self.original, &self.current) {
            (MapValue::Range(from), MapValue::Range(to)) => Some(ProvenSourceMapping::Range {
                from: from.clone(),
                to: to.clone(),
            }),
            (
                MapValue::Boundary {
                    offset: from,
                    affinity: from_affinity,
                },
                MapValue::Boundary {
                    offset: to,
                    affinity: to_affinity,
                },
            ) if from_affinity == to_affinity => Some(ProvenSourceMapping::Boundary {
                from: *from,
                to: *to,
                affinity: *from_affinity,
            }),
            _ => None,
        }
    }

    fn current_boundary(&self) -> Option<usize> {
        match self.current {
            MapValue::Boundary { offset, .. } => Some(offset),
            MapValue::Range(_) => None,
        }
    }

    fn current_range(&self) -> Option<Range<usize>> {
        match &self.current {
            MapValue::Range(range) => Some(range.clone()),
            MapValue::Boundary { .. } => None,
        }
    }
}

/// Maps the restart boundary, retained prefix, convergence boundary, and
/// retained tail in one immutable lineage pass.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct LineageAdoptionBundleJob {
    root: Option<Arc<LineageNode>>,
    capacity: usize,
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    next_revision: SourceRevision,
    expected_root: SourceRootId,
    expected_len: usize,
    total_records: usize,
    processed_records: usize,
    first_record_prevalidated: bool,
    restart: AdoptionMapState,
    prefix: Option<AdoptionMapState>,
    convergence: AdoptionMapState,
    tail: AdoptionMapState,
    changed: Option<(LineageAdoptionRegion, SourceRevision)>,
    terminal: Option<LineageAdoptionBundleStatus>,
    metrics: LineageAdoptionBundleMetrics,
    retention: LineageSnapshotRetention,
}

#[allow(dead_code)]
impl LineageAdoptionBundleJob {
    fn new(
        store: &SourceStore,
        from: SourceSnapshotDescriptor,
        old_restart: usize,
        old_convergence: usize,
        convergence_affinity: BoundaryAffinity,
    ) -> Result<Self, LineageAdoptionBundleError> {
        let prepared = prepare_lineage(store, from.revision)?;
        let lineage_from = SourceSnapshotDescriptor {
            revision: from.revision,
            root: prepared.expected_root,
            bytes: prepared.source_len,
        };
        if from != lineage_from {
            return Err(LineageAdoptionBundleError::SnapshotMismatch {
                supplied: from,
                lineage: lineage_from,
            });
        }
        // An empty retained tail has no suffix to adopt and should take the
        // clean EOF completion path instead of minting vacuous authority.
        if old_restart > old_convergence || old_convergence >= from.bytes {
            return Err(LineageAdoptionBundleError::InvalidBoundaries);
        }

        let to = SourceSnapshotDescriptor {
            revision: prepared.snapshot_revision,
            root: prepared.snapshot_root,
            bytes: prepared.snapshot_len,
        };
        let restart = AdoptionMapState::new(MapValue::Boundary {
            offset: old_restart,
            affinity: BoundaryAffinity::Before,
        });
        let prefix =
            (old_restart > 0).then(|| AdoptionMapState::new(MapValue::Range(0..old_restart)));
        let convergence = AdoptionMapState::new(MapValue::Boundary {
            offset: old_convergence,
            affinity: convergence_affinity,
        });
        let tail = AdoptionMapState::new(MapValue::Range(old_convergence..from.bytes));
        let mut job = Self {
            root: prepared.root,
            capacity: prepared.capacity,
            from,
            to,
            next_revision: from.revision,
            expected_root: from.root,
            expected_len: from.bytes,
            total_records: prepared.total_records,
            processed_records: 0,
            first_record_prevalidated: prepared.total_records > 0
                && prepared.metrics.constructor_records_validated == 1,
            restart,
            prefix,
            convergence,
            tail,
            changed: None,
            terminal: None,
            metrics: LineageAdoptionBundleMetrics {
                constructor_records_examined: prepared.metrics.constructor_records_examined,
                constructor_records_validated: prepared.metrics.constructor_records_validated,
                constructor_tree_nodes_read: prepared.metrics.constructor_tree_nodes_read,
                maximum_tree_nodes_per_lookup: prepared.metrics.maximum_tree_nodes_per_lookup,
                records_copied: prepared.metrics.records_copied,
                ..LineageAdoptionBundleMetrics::default()
            },
            retention: prepared.retention,
        };
        if job.total_records == 0 {
            job.finish();
        }
        Ok(job)
    }

    #[must_use]
    pub(crate) const fn metrics(&self) -> LineageAdoptionBundleMetrics {
        self.metrics
    }

    #[must_use]
    pub(crate) const fn retention(&self) -> LineageSnapshotRetention {
        self.retention
    }

    #[must_use]
    pub(crate) fn poll(&mut self, fuel: usize) -> LineageAdoptionBundleStatus {
        if let Some(status) = &self.terminal {
            return status.clone();
        }

        let mut spent = 0;
        while spent < fuel && self.processed_records < self.total_records {
            let slot = revision_slot(self.next_revision, self.capacity);
            let lookup = lookup_record(self.root.as_ref(), 0..self.capacity, slot);
            self.metrics.poll_tree_nodes_read += lookup.nodes_read;
            self.metrics.maximum_tree_nodes_per_lookup = self
                .metrics
                .maximum_tree_nodes_per_lookup
                .max(lookup.nodes_read);
            let Some(record) = lookup.record else {
                return self.fail(LineageAdoptionBundleError::Lineage(
                    LineageError::BrokenChain,
                ));
            };
            self.metrics.poll_records_examined += 1;
            if self.first_record_prevalidated {
                debug_assert_eq!(self.processed_records, 0);
                self.first_record_prevalidated = false;
            } else {
                if !record_is_valid(
                    record,
                    self.next_revision,
                    self.expected_root,
                    self.expected_len,
                ) {
                    return self.fail(LineageAdoptionBundleError::Lineage(
                        LineageError::BrokenChain,
                    ));
                }
                self.metrics.poll_records_validated += 1;
            }

            if self.changed.is_none() {
                let at_revision = record.transition.target_revision;
                let mut unchanged = map_adoption_state(
                    &mut self.restart,
                    LineageAdoptionRegion::RestartBoundary,
                    record,
                    &mut self.metrics,
                    &mut self.changed,
                );
                if unchanged {
                    unchanged = match &mut self.prefix {
                        Some(prefix) => map_adoption_state(
                            prefix,
                            LineageAdoptionRegion::RetainedPrefix,
                            record,
                            &mut self.metrics,
                            &mut self.changed,
                        ),
                        None => true,
                    };
                }
                if unchanged {
                    unchanged = map_adoption_state(
                        &mut self.convergence,
                        LineageAdoptionRegion::ConvergenceBoundary,
                        record,
                        &mut self.metrics,
                        &mut self.changed,
                    );
                }
                if unchanged {
                    unchanged = map_adoption_state(
                        &mut self.tail,
                        LineageAdoptionRegion::RetainedTail,
                        record,
                        &mut self.metrics,
                        &mut self.changed,
                    );
                }
                if unchanged {
                    self.changed = self
                        .alignment_error(record.new_len)
                        .map(|region| (region, at_revision));
                }
                // A changed region makes the whole adoption proof impossible,
                // but the immutable lineage chain is still validated to its
                // frozen target descriptor.
                debug_assert!(
                    self.changed
                        .is_none_or(|(_, changed_at)| changed_at == at_revision),
                    "a newly changed bundle region names the record that changed it"
                );
            }
            self.next_revision = record.transition.target_revision;
            self.expected_root = record.transition.result_root;
            self.expected_len = record.new_len;
            self.processed_records += 1;
            spent += 1;
        }

        if self.processed_records == self.total_records {
            self.finish()
        } else {
            LineageAdoptionBundleStatus::Pending {
                processed_records: self.processed_records,
                remaining_records: self.total_records - self.processed_records,
            }
        }
    }

    pub(crate) fn into_proof(
        self,
    ) -> Result<LineageAdoptionBundleProof, LineageAdoptionBundleStatus> {
        let Some(LineageAdoptionBundleStatus::Proven { .. }) = self.terminal.as_ref() else {
            return Err(self
                .terminal
                .unwrap_or(LineageAdoptionBundleStatus::Pending {
                    processed_records: self.processed_records,
                    remaining_records: self.total_records - self.processed_records,
                }));
        };
        let restart = self
            .restart
            .proof()
            .ok_or(LineageAdoptionBundleStatus::Failed(
                LineageAdoptionBundleError::Lineage(LineageError::BrokenChain),
            ))?;
        let current_restart = match restart {
            ProvenSourceMapping::Boundary { to, .. } => to,
            ProvenSourceMapping::Range { .. } => unreachable!("restart is a boundary"),
        };
        let prefix = if let Some(prefix) = self.prefix {
            match prefix.proof() {
                Some(ProvenSourceMapping::Range { from, to }) => {
                    ProvenRetainedPrefix::Range { from, to }
                }
                _ => {
                    return Err(LineageAdoptionBundleStatus::Failed(
                        LineageAdoptionBundleError::Lineage(LineageError::BrokenChain),
                    ));
                }
            }
        } else if current_restart == 0 {
            ProvenRetainedPrefix::Empty { from: 0, to: 0 }
        } else {
            return Err(LineageAdoptionBundleStatus::Failed(
                LineageAdoptionBundleError::Lineage(LineageError::BrokenChain),
            ));
        };
        let convergence = self
            .convergence
            .proof()
            .ok_or(LineageAdoptionBundleStatus::Failed(
                LineageAdoptionBundleError::Lineage(LineageError::BrokenChain),
            ))?;
        let tail = self
            .tail
            .proof()
            .ok_or(LineageAdoptionBundleStatus::Failed(
                LineageAdoptionBundleError::Lineage(LineageError::BrokenChain),
            ))?;
        Ok(LineageAdoptionBundleProof {
            from: self.from,
            to: self.to,
            restart,
            prefix,
            convergence,
            tail,
        })
    }

    fn finish(&mut self) -> LineageAdoptionBundleStatus {
        if self.next_revision != self.to.revision
            || self.expected_root != self.to.root
            || self.expected_len != self.to.bytes
        {
            return self.fail(LineageAdoptionBundleError::Lineage(
                LineageError::BrokenChain,
            ));
        }
        let status = if let Some((region, at_revision)) = self.changed {
            LineageAdoptionBundleStatus::Changed {
                region,
                at_revision,
            }
        } else {
            if self.alignment_error(self.to.bytes).is_some() {
                return self.fail(LineageAdoptionBundleError::Lineage(
                    LineageError::BrokenChain,
                ));
            }
            let Some(restart) = self.restart.current_boundary() else {
                return self.fail(LineageAdoptionBundleError::Lineage(
                    LineageError::BrokenChain,
                ));
            };
            let prefix = if let Some(prefix) = &self.prefix {
                let Some(to) = prefix.current_range() else {
                    return self.fail(LineageAdoptionBundleError::Lineage(
                        LineageError::BrokenChain,
                    ));
                };
                let MapValue::Range(from) = &prefix.original else {
                    return self.fail(LineageAdoptionBundleError::Lineage(
                        LineageError::BrokenChain,
                    ));
                };
                ProvenRetainedPrefix::Range {
                    from: from.clone(),
                    to,
                }
            } else if restart == 0 {
                ProvenRetainedPrefix::Empty { from: 0, to: 0 }
            } else {
                return self.fail(LineageAdoptionBundleError::Lineage(
                    LineageError::BrokenChain,
                ));
            };
            let Some(convergence) = self.convergence.current_boundary() else {
                return self.fail(LineageAdoptionBundleError::Lineage(
                    LineageError::BrokenChain,
                ));
            };
            let Some(tail) = self.tail.current_range() else {
                return self.fail(LineageAdoptionBundleError::Lineage(
                    LineageError::BrokenChain,
                ));
            };
            LineageAdoptionBundleStatus::Proven {
                restart,
                prefix,
                convergence,
                tail,
            }
        };
        self.terminal = Some(status.clone());
        status
    }

    fn alignment_error(&self, target_len: usize) -> Option<LineageAdoptionRegion> {
        let Some(restart) = self.restart.current_boundary() else {
            return Some(LineageAdoptionRegion::RestartBoundary);
        };
        match &self.prefix {
            Some(prefix) => {
                let Some(range) = prefix.current_range() else {
                    return Some(LineageAdoptionRegion::RetainedPrefix);
                };
                if range.start != 0 || range.end != restart {
                    return Some(LineageAdoptionRegion::RetainedPrefix);
                }
            }
            None if restart != 0 => return Some(LineageAdoptionRegion::RetainedPrefix),
            None => {}
        }
        let Some(convergence) = self.convergence.current_boundary() else {
            return Some(LineageAdoptionRegion::ConvergenceBoundary);
        };
        if restart > convergence {
            return Some(LineageAdoptionRegion::ConvergenceBoundary);
        }
        let Some(tail) = self.tail.current_range() else {
            return Some(LineageAdoptionRegion::RetainedTail);
        };
        if tail.start != convergence {
            return Some(LineageAdoptionRegion::ConvergenceBoundary);
        }
        if tail.end != target_len {
            return Some(LineageAdoptionRegion::RetainedTail);
        }
        None
    }

    fn fail(&mut self, error: LineageAdoptionBundleError) -> LineageAdoptionBundleStatus {
        let status = LineageAdoptionBundleStatus::Failed(error);
        self.terminal = Some(status.clone());
        status
    }
}

#[allow(dead_code)]
fn map_adoption_state(
    state: &mut AdoptionMapState,
    region: LineageAdoptionRegion,
    record: &EditRecord,
    metrics: &mut LineageAdoptionBundleMetrics,
    changed: &mut Option<(LineageAdoptionRegion, SourceRevision)>,
) -> bool {
    metrics.poll_mapping_attempts += 1;
    if state.map(record) {
        metrics.poll_mappings_succeeded += 1;
        true
    } else {
        *changed = Some((region, record.transition.target_revision));
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum RestartSelectionRegion {
    PreferredBoundary,
    RetainedPrefix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum RestartSelectionError {
    Lineage(LineageError),
    SnapshotMismatch {
        supplied: SourceSnapshotDescriptor,
        lineage: SourceSnapshotDescriptor,
    },
    InvalidBoundary,
}

impl From<LineageError> for RestartSelectionError {
    fn from(error: LineageError) -> Self {
        Self::Lineage(error)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct RestartSelectionMetrics {
    pub constructor_records_examined: usize,
    pub constructor_records_validated: usize,
    pub constructor_tree_nodes_read: usize,
    pub poll_records_examined: usize,
    pub poll_records_validated: usize,
    pub poll_mapping_attempts: usize,
    pub poll_mappings_succeeded: usize,
    pub poll_tree_nodes_read: usize,
    pub maximum_tree_nodes_per_lookup: usize,
    pub records_copied: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct RejectedPreferredRestart {
    pub region: RestartSelectionRegion,
    pub at_revision: SourceRevision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ProvenRestartSelection {
    Preferred {
        boundary: ProvenSourceMapping,
        prefix: ProvenRetainedPrefix,
    },
    ZeroFallback {
        boundary: ProvenSourceMapping,
        rejected: Option<RejectedPreferredRestart>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum RestartSelectionStatus {
    Pending {
        processed_records: usize,
        remaining_records: usize,
    },
    Selected {
        old: usize,
        current: usize,
        used_zero_fallback: bool,
    },
    Failed(RestartSelectionError),
}

/// Non-cloneable proof binding one selected parser start to a single frozen
/// source-lineage snapshot. It can start parsing but cannot retain or attach a
/// green prefix; the final adoption bundle must reproduce it exactly.
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct RestartSelectionProof {
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    selection: ProvenRestartSelection,
}

#[allow(dead_code)]
impl RestartSelectionProof {
    #[must_use]
    pub(crate) const fn from(&self) -> SourceSnapshotDescriptor {
        self.from
    }

    #[must_use]
    pub(crate) const fn to(&self) -> SourceSnapshotDescriptor {
        self.to
    }

    #[must_use]
    pub(crate) const fn selection(&self) -> &ProvenRestartSelection {
        &self.selection
    }

    #[must_use]
    pub(crate) fn selected_boundaries(&self) -> (usize, usize) {
        let mapping = match &self.selection {
            ProvenRestartSelection::Preferred { boundary, .. }
            | ProvenRestartSelection::ZeroFallback { boundary, .. } => boundary,
        };
        match mapping {
            ProvenSourceMapping::Boundary { from, to, .. } => (*from, *to),
            ProvenSourceMapping::Range { .. } => unreachable!("restart proof is a boundary"),
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct RestartSelectionJob {
    root: Option<Arc<LineageNode>>,
    capacity: usize,
    from: SourceSnapshotDescriptor,
    to: SourceSnapshotDescriptor,
    next_revision: SourceRevision,
    expected_root: SourceRootId,
    expected_len: usize,
    total_records: usize,
    processed_records: usize,
    first_record_prevalidated: bool,
    preferred: AdoptionMapState,
    prefix: Option<AdoptionMapState>,
    zero: AdoptionMapState,
    rejected: Option<RejectedPreferredRestart>,
    terminal: Option<RestartSelectionStatus>,
    metrics: RestartSelectionMetrics,
    retention: LineageSnapshotRetention,
}

#[allow(dead_code)]
impl RestartSelectionJob {
    fn new(
        store: &SourceStore,
        from: SourceSnapshotDescriptor,
        preferred_restart: usize,
    ) -> Result<Self, RestartSelectionError> {
        let prepared = prepare_lineage(store, from.revision)?;
        let lineage_from = SourceSnapshotDescriptor {
            revision: from.revision,
            root: prepared.expected_root,
            bytes: prepared.source_len,
        };
        if from != lineage_from {
            return Err(RestartSelectionError::SnapshotMismatch {
                supplied: from,
                lineage: lineage_from,
            });
        }
        if preferred_restart > from.bytes {
            return Err(RestartSelectionError::InvalidBoundary);
        }
        let to = SourceSnapshotDescriptor {
            revision: prepared.snapshot_revision,
            root: prepared.snapshot_root,
            bytes: prepared.snapshot_len,
        };
        let mut job = Self {
            root: prepared.root,
            capacity: prepared.capacity,
            from,
            to,
            next_revision: from.revision,
            expected_root: from.root,
            expected_len: from.bytes,
            total_records: prepared.total_records,
            processed_records: 0,
            first_record_prevalidated: prepared.total_records > 0
                && prepared.metrics.constructor_records_validated == 1,
            preferred: AdoptionMapState::new(MapValue::Boundary {
                offset: preferred_restart,
                affinity: BoundaryAffinity::Before,
            }),
            prefix: (preferred_restart > 0)
                .then(|| AdoptionMapState::new(MapValue::Range(0..preferred_restart))),
            zero: AdoptionMapState::new(MapValue::Boundary {
                offset: 0,
                affinity: BoundaryAffinity::Before,
            }),
            rejected: None,
            terminal: None,
            metrics: RestartSelectionMetrics {
                constructor_records_examined: prepared.metrics.constructor_records_examined,
                constructor_records_validated: prepared.metrics.constructor_records_validated,
                constructor_tree_nodes_read: prepared.metrics.constructor_tree_nodes_read,
                maximum_tree_nodes_per_lookup: prepared.metrics.maximum_tree_nodes_per_lookup,
                records_copied: prepared.metrics.records_copied,
                ..RestartSelectionMetrics::default()
            },
            retention: prepared.retention,
        };
        if job.total_records == 0 {
            job.finish();
        }
        Ok(job)
    }

    #[must_use]
    pub(crate) const fn metrics(&self) -> RestartSelectionMetrics {
        self.metrics
    }

    #[must_use]
    pub(crate) const fn retention(&self) -> LineageSnapshotRetention {
        self.retention
    }

    #[must_use]
    pub(crate) fn poll(&mut self, fuel: usize) -> RestartSelectionStatus {
        if let Some(status) = &self.terminal {
            return status.clone();
        }
        let mut spent = 0;
        while spent < fuel && self.processed_records < self.total_records {
            let slot = revision_slot(self.next_revision, self.capacity);
            let lookup = lookup_record(self.root.as_ref(), 0..self.capacity, slot);
            self.metrics.poll_tree_nodes_read += lookup.nodes_read;
            self.metrics.maximum_tree_nodes_per_lookup = self
                .metrics
                .maximum_tree_nodes_per_lookup
                .max(lookup.nodes_read);
            let Some(record) = lookup.record else {
                return self.fail(RestartSelectionError::Lineage(LineageError::BrokenChain));
            };
            self.metrics.poll_records_examined += 1;
            if self.first_record_prevalidated {
                debug_assert_eq!(self.processed_records, 0);
                self.first_record_prevalidated = false;
            } else {
                if !record_is_valid(
                    record,
                    self.next_revision,
                    self.expected_root,
                    self.expected_len,
                ) {
                    return self.fail(RestartSelectionError::Lineage(LineageError::BrokenChain));
                }
                self.metrics.poll_records_validated += 1;
            }

            let at_revision = record.transition.target_revision;
            if self.rejected.is_none() {
                let mut preferred_survives =
                    map_restart_state(&mut self.preferred, record, &mut self.metrics);
                if !preferred_survives {
                    self.rejected = Some(RejectedPreferredRestart {
                        region: RestartSelectionRegion::PreferredBoundary,
                        at_revision,
                    });
                }
                if preferred_survives {
                    preferred_survives = match &mut self.prefix {
                        Some(prefix) => map_restart_state(prefix, record, &mut self.metrics),
                        None => true,
                    };
                    if !preferred_survives {
                        self.rejected = Some(RejectedPreferredRestart {
                            region: RestartSelectionRegion::RetainedPrefix,
                            at_revision,
                        });
                    }
                }
                if preferred_survives && let Some(region) = self.preferred_alignment_error() {
                    self.rejected = Some(RejectedPreferredRestart {
                        region,
                        at_revision,
                    });
                }
            }
            if !map_restart_state(&mut self.zero, record, &mut self.metrics) {
                return self.fail(RestartSelectionError::Lineage(LineageError::BrokenChain));
            }
            if self.zero.current_boundary() != Some(0) {
                return self.fail(RestartSelectionError::Lineage(LineageError::BrokenChain));
            }

            self.next_revision = record.transition.target_revision;
            self.expected_root = record.transition.result_root;
            self.expected_len = record.new_len;
            self.processed_records += 1;
            spent += 1;
        }
        if self.processed_records == self.total_records {
            self.finish()
        } else {
            RestartSelectionStatus::Pending {
                processed_records: self.processed_records,
                remaining_records: self.total_records - self.processed_records,
            }
        }
    }

    pub(crate) fn into_proof(self) -> Result<RestartSelectionProof, RestartSelectionStatus> {
        let Some(RestartSelectionStatus::Selected { .. }) = self.terminal.as_ref() else {
            return Err(self.terminal.unwrap_or(RestartSelectionStatus::Pending {
                processed_records: self.processed_records,
                remaining_records: self.total_records - self.processed_records,
            }));
        };
        let zero = self.zero.proof().ok_or(RestartSelectionStatus::Failed(
            RestartSelectionError::Lineage(LineageError::BrokenChain),
        ))?;
        let selection = if let Some(rejected) = self.rejected {
            ProvenRestartSelection::ZeroFallback {
                boundary: zero,
                rejected: Some(rejected),
            }
        } else {
            let boundary = self
                .preferred
                .proof()
                .ok_or(RestartSelectionStatus::Failed(
                    RestartSelectionError::Lineage(LineageError::BrokenChain),
                ))?;
            let current = match &boundary {
                ProvenSourceMapping::Boundary { to, .. } => *to,
                ProvenSourceMapping::Range { .. } => unreachable!("restart is a boundary"),
            };
            let prefix = if let Some(prefix) = self.prefix {
                match prefix.proof() {
                    Some(ProvenSourceMapping::Range { from, to }) => {
                        ProvenRetainedPrefix::Range { from, to }
                    }
                    _ => {
                        return Err(RestartSelectionStatus::Failed(
                            RestartSelectionError::Lineage(LineageError::BrokenChain),
                        ));
                    }
                }
            } else if current == 0 {
                ProvenRetainedPrefix::Empty { from: 0, to: 0 }
            } else {
                return Err(RestartSelectionStatus::Failed(
                    RestartSelectionError::Lineage(LineageError::BrokenChain),
                ));
            };
            ProvenRestartSelection::Preferred { boundary, prefix }
        };
        Ok(RestartSelectionProof {
            from: self.from,
            to: self.to,
            selection,
        })
    }

    fn preferred_alignment_error(&self) -> Option<RestartSelectionRegion> {
        let Some(boundary) = self.preferred.current_boundary() else {
            return Some(RestartSelectionRegion::PreferredBoundary);
        };
        match &self.prefix {
            Some(prefix) => {
                let Some(range) = prefix.current_range() else {
                    return Some(RestartSelectionRegion::RetainedPrefix);
                };
                if range.start != 0 || range.end != boundary {
                    return Some(RestartSelectionRegion::RetainedPrefix);
                }
            }
            None if boundary != 0 => return Some(RestartSelectionRegion::RetainedPrefix),
            None => {}
        }
        None
    }

    fn finish(&mut self) -> RestartSelectionStatus {
        if self.next_revision != self.to.revision
            || self.expected_root != self.to.root
            || self.expected_len != self.to.bytes
            || self.zero.current_boundary() != Some(0)
        {
            return self.fail(RestartSelectionError::Lineage(LineageError::BrokenChain));
        }
        if self.rejected.is_none()
            && let Some(region) = self.preferred_alignment_error()
        {
            // With no record there is no revision that changed the
            // preferred prefix. Construction invariants should therefore
            // make this unreachable rather than inventing a cause.
            let _ = region;
            return self.fail(RestartSelectionError::Lineage(LineageError::BrokenChain));
        }
        let (old, current, used_zero_fallback) = if self.rejected.is_some() {
            (0, 0, true)
        } else {
            let Some(current) = self.preferred.current_boundary() else {
                return self.fail(RestartSelectionError::Lineage(LineageError::BrokenChain));
            };
            let MapValue::Boundary { offset: old, .. } = &self.preferred.original else {
                return self.fail(RestartSelectionError::Lineage(LineageError::BrokenChain));
            };
            (*old, current, false)
        };
        let status = RestartSelectionStatus::Selected {
            old,
            current,
            used_zero_fallback,
        };
        self.terminal = Some(status.clone());
        status
    }

    fn fail(&mut self, error: RestartSelectionError) -> RestartSelectionStatus {
        let status = RestartSelectionStatus::Failed(error);
        self.terminal = Some(status.clone());
        status
    }
}

#[allow(dead_code)]
fn map_restart_state(
    state: &mut AdoptionMapState,
    record: &EditRecord,
    metrics: &mut RestartSelectionMetrics,
) -> bool {
    metrics.poll_mapping_attempts += 1;
    if state.map(record) {
        metrics.poll_mappings_succeeded += 1;
        true
    } else {
        false
    }
}

#[derive(Debug)]
pub struct LineageMapJob {
    root: Option<Arc<LineageNode>>,
    capacity: usize,
    snapshot_revision: SourceRevision,
    snapshot_root: SourceRootId,
    snapshot_len: usize,
    base_revision: SourceRevision,
    base_root: SourceRootId,
    base_len: usize,
    next_revision: SourceRevision,
    expected_root: SourceRootId,
    expected_len: usize,
    total_records: usize,
    processed_records: usize,
    original_value: MapValue,
    value: MapValue,
    changed_at: Option<SourceRevision>,
    terminal: Option<MapStatus>,
    metrics: LineageWorkMetrics,
    retention: LineageSnapshotRetention,
}

impl LineageMapJob {
    fn range(
        store: &SourceStore,
        revision: SourceRevision,
        range: Range<usize>,
    ) -> Result<Self, LineageError> {
        let prepared = prepare_lineage(store, revision)?;
        let source_len = prepared.source_len;
        if range.is_empty() || range.start > range.end || range.end > source_len {
            return Err(LineageError::InvalidRange);
        }
        let terminal = (prepared.total_records == 0).then(|| MapStatus::ProvenRange(range.clone()));
        let original_value = MapValue::Range(range);
        Ok(Self {
            root: prepared.root,
            capacity: prepared.capacity,
            snapshot_revision: prepared.snapshot_revision,
            snapshot_root: prepared.snapshot_root,
            snapshot_len: prepared.snapshot_len,
            base_revision: revision,
            base_root: prepared.expected_root,
            base_len: source_len,
            next_revision: revision,
            expected_root: prepared.expected_root,
            expected_len: source_len,
            total_records: prepared.total_records,
            processed_records: 0,
            value: original_value.clone(),
            original_value,
            changed_at: None,
            terminal,
            metrics: prepared.metrics,
            retention: prepared.retention,
        })
    }

    fn boundary(
        store: &SourceStore,
        revision: SourceRevision,
        offset: usize,
        affinity: BoundaryAffinity,
    ) -> Result<Self, LineageError> {
        let prepared = prepare_lineage(store, revision)?;
        let source_len = prepared.source_len;
        if offset > source_len {
            return Err(LineageError::InvalidRange);
        }
        let terminal = (prepared.total_records == 0).then_some(MapStatus::ProvenBoundary(offset));
        let original_value = MapValue::Boundary { offset, affinity };
        Ok(Self {
            root: prepared.root,
            capacity: prepared.capacity,
            snapshot_revision: prepared.snapshot_revision,
            snapshot_root: prepared.snapshot_root,
            snapshot_len: prepared.snapshot_len,
            base_revision: revision,
            base_root: prepared.expected_root,
            base_len: source_len,
            next_revision: revision,
            expected_root: prepared.expected_root,
            expected_len: source_len,
            total_records: prepared.total_records,
            processed_records: 0,
            value: original_value.clone(),
            original_value,
            changed_at: None,
            terminal,
            metrics: prepared.metrics,
            retention: prepared.retention,
        })
    }

    #[must_use]
    pub const fn metrics(&self) -> LineageWorkMetrics {
        self.metrics
    }

    #[must_use]
    pub const fn retention(&self) -> LineageSnapshotRetention {
        self.retention
    }

    /// Consumes a completed mapping job and transfers its source-store-derived
    /// lineage evidence. A pending, changed, or failed job yields its exact
    /// terminal status instead of a partially authoritative descriptor.
    pub fn into_proof(self) -> Result<LineageMappingProof, MapStatus> {
        let Some(status) = self.terminal.clone() else {
            return Err(MapStatus::Pending {
                processed_records: self.processed_records,
                remaining_records: self.total_records - self.processed_records,
            });
        };
        let mapping = match (&self.original_value, &self.value, status) {
            (MapValue::Range(from), MapValue::Range(to), MapStatus::ProvenRange(proven))
                if *to == proven =>
            {
                ProvenSourceMapping::Range {
                    from: from.clone(),
                    to: to.clone(),
                }
            }
            (
                MapValue::Boundary {
                    offset: from,
                    affinity: from_affinity,
                },
                MapValue::Boundary {
                    offset: to,
                    affinity: to_affinity,
                },
                MapStatus::ProvenBoundary(proven),
            ) if *to == proven && from_affinity == to_affinity => ProvenSourceMapping::Boundary {
                from: *from,
                to: *to,
                affinity: *from_affinity,
            },
            (_, _, other) => return Err(other),
        };
        Ok(LineageMappingProof {
            from: SourceSnapshotDescriptor {
                revision: self.base_revision,
                root: self.base_root,
                bytes: self.base_len,
            },
            to: SourceSnapshotDescriptor {
                revision: self.snapshot_revision,
                root: self.snapshot_root,
                bytes: self.snapshot_len,
            },
            mapping,
        })
    }

    #[must_use]
    pub fn poll(&mut self, fuel: usize) -> MapStatus {
        if let Some(status) = &self.terminal {
            return status.clone();
        }

        let mut spent = 0;
        while spent < fuel && self.processed_records < self.total_records {
            let slot = revision_slot(self.next_revision, self.capacity);
            let lookup = lookup_record(self.root.as_ref(), 0..self.capacity, slot);
            self.metrics.poll_tree_nodes_read += lookup.nodes_read;
            self.metrics.maximum_tree_nodes_per_lookup = self
                .metrics
                .maximum_tree_nodes_per_lookup
                .max(lookup.nodes_read);
            let Some(record) = lookup.record else {
                return self.fail(LineageError::BrokenChain);
            };
            self.metrics.poll_records_examined += 1;
            if !record_is_valid(
                record,
                self.next_revision,
                self.expected_root,
                self.expected_len,
            ) {
                return self.fail(LineageError::BrokenChain);
            }
            self.metrics.poll_records_validated += 1;

            if self.changed_at.is_none() {
                self.metrics.poll_mapping_attempts += 1;
                if let Some(mapped) = map_value(&self.value, record) {
                    self.value = mapped;
                    self.metrics.poll_records_mapped += 1;
                } else {
                    self.changed_at = Some(record.transition.target_revision);
                }
            }

            self.next_revision = record.transition.target_revision;
            self.expected_root = record.transition.result_root;
            self.expected_len = record.new_len;
            self.processed_records += 1;
            spent += 1;
        }

        if self.processed_records == self.total_records {
            if self.next_revision != self.snapshot_revision
                || self.expected_root != self.snapshot_root
                || self.expected_len != self.snapshot_len
            {
                return self.fail(LineageError::BrokenChain);
            }
            let status = self.changed_at.map_or_else(
                || match &self.value {
                    MapValue::Range(range) => MapStatus::ProvenRange(range.clone()),
                    MapValue::Boundary { offset, .. } => MapStatus::ProvenBoundary(*offset),
                },
                |at_revision| MapStatus::Changed { at_revision },
            );
            self.terminal = Some(status.clone());
            status
        } else {
            MapStatus::Pending {
                processed_records: self.processed_records,
                remaining_records: self.total_records - self.processed_records,
            }
        }
    }

    fn fail(&mut self, error: LineageError) -> MapStatus {
        let status = MapStatus::Failed(error);
        self.terminal = Some(status.clone());
        status
    }
}

#[derive(Debug)]
struct PreparedLineage {
    root: Option<Arc<LineageNode>>,
    capacity: usize,
    snapshot_revision: SourceRevision,
    snapshot_root: SourceRootId,
    snapshot_len: usize,
    total_records: usize,
    source_len: usize,
    expected_root: SourceRootId,
    metrics: LineageWorkMetrics,
    retention: LineageSnapshotRetention,
}

fn prepare_lineage(
    store: &SourceStore,
    revision: SourceRevision,
) -> Result<PreparedLineage, LineageError> {
    if revision > store.revision {
        return Err(LineageError::FutureRevision);
    }
    let retention = store.lineage.retention();
    let snapshot_retention = LineageSnapshotRetention {
        records: retention.records,
        tree_nodes: retention.tree_nodes,
        maximum_tree_nodes: retention.maximum_tree_nodes,
        retained_source_roots: 0,
    };
    if revision == store.revision {
        return Ok(PreparedLineage {
            root: None,
            capacity: retention.capacity,
            snapshot_revision: store.revision,
            snapshot_root: store.root.identity,
            snapshot_len: store.root.len_bytes(),
            total_records: 0,
            source_len: store.root.len_bytes(),
            expected_root: store.root.identity,
            metrics: LineageWorkMetrics::default(),
            retention: LineageSnapshotRetention::default(),
        });
    }

    let delta = store.revision.0 - revision.0;
    let retained_records =
        u64::try_from(retention.records).expect("retained record count fits u64");
    if delta > retained_records {
        return Err(LineageError::HistoryExpired);
    }
    let root = store.lineage.root.clone();
    let slot = revision_slot(revision, retention.capacity);
    let lookup = lookup_record(root.as_ref(), 0..retention.capacity, slot);
    let Some(first) = lookup.record else {
        return Err(LineageError::BrokenChain);
    };
    let mut metrics = LineageWorkMetrics {
        constructor_records_examined: 1,
        constructor_tree_nodes_read: lookup.nodes_read,
        maximum_tree_nodes_per_lookup: lookup.nodes_read,
        ..LineageWorkMetrics::default()
    };
    if !record_is_valid(first, revision, first.transition.base_root, first.old_len) {
        return Err(LineageError::BrokenChain);
    }
    metrics.constructor_records_validated = 1;
    let source_len = first.old_len;
    let expected_root = first.transition.base_root;
    Ok(PreparedLineage {
        root,
        capacity: retention.capacity,
        snapshot_revision: store.revision,
        snapshot_root: store.root.identity,
        snapshot_len: store.root.len_bytes(),
        total_records: usize::try_from(delta)
            .expect("history delta cannot exceed the usize-sized ring"),
        source_len,
        expected_root,
        metrics,
        retention: snapshot_retention,
    })
}

fn record_is_valid(
    record: &EditRecord,
    expected_revision: SourceRevision,
    expected_root: SourceRootId,
    expected_len: usize,
) -> bool {
    let Some(target_revision) = expected_revision.0.checked_add(1).map(SourceRevision) else {
        return false;
    };
    if record.transition.base_revision != expected_revision
        || record.transition.target_revision != target_revision
        || record.transition.base_root != expected_root
        || record.transition.result_root == expected_root
        || record.old_len != expected_len
        || record.edited_old.start > record.edited_old.end
        || record.edited_old.end > record.old_len
        || record.edited_new.start > record.edited_new.end
        || record.edited_new.end > record.new_len
        || record.edited_old.start != record.edited_new.start
    {
        return false;
    }
    let removed = record.edited_old.end - record.edited_old.start;
    let inserted = record.edited_new.end - record.edited_new.start;
    let Some(expected_new_len) = record
        .old_len
        .checked_sub(removed)
        .and_then(|length| length.checked_add(inserted))
    else {
        return false;
    };
    expected_new_len == record.new_len
        && record.prefix.old == (0..record.edited_old.start)
        && record.prefix.new == (0..record.edited_new.start)
        && record.suffix.old == (record.edited_old.end..record.old_len)
        && record.suffix.new == (record.edited_new.end..record.new_len)
}

fn map_value(value: &MapValue, record: &EditRecord) -> Option<MapValue> {
    match value {
        MapValue::Range(range) => map_nonempty_range(range, record).map(MapValue::Range),
        MapValue::Boundary { offset, affinity } => {
            map_boundary(*offset, *affinity, record).map(|offset| MapValue::Boundary {
                offset,
                affinity: *affinity,
            })
        }
    }
}

fn map_nonempty_range(range: &Range<usize>, record: &EditRecord) -> Option<Range<usize>> {
    if range.is_empty() {
        return None;
    }
    if range.end <= record.edited_old.start {
        return Some(range.clone());
    }
    if range.start >= record.edited_old.end {
        let start = record.edited_new.end + (range.start - record.edited_old.end);
        return Some(start..start + range.len());
    }
    None
}

fn map_boundary(offset: usize, affinity: BoundaryAffinity, record: &EditRecord) -> Option<usize> {
    let old = &record.edited_old;
    let new = &record.edited_new;
    if offset < old.start {
        return Some(offset);
    }
    if offset > old.end {
        return Some(new.end + (offset - old.end));
    }
    if old.is_empty() && offset == old.start {
        return Some(match affinity {
            BoundaryAffinity::Before => new.start,
            BoundaryAffinity::After => new.end,
        });
    }
    if offset == old.start {
        return Some(match affinity {
            BoundaryAffinity::Before => new.start,
            BoundaryAffinity::After => new.end,
        });
    }
    if offset == old.end {
        return Some(match affinity {
            BoundaryAffinity::Before => new.start,
            BoundaryAffinity::After => new.end,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive_physical_line_query(text: &str, cut: usize) -> (u64, bool, Option<(u64, u64)>) {
        assert!(cut <= text.len());
        assert!(text.is_char_boundary(cut));
        let bytes = text.as_bytes();
        let mut starts = vec![(0usize, None)];
        let mut offset = 0;
        let mut content_bytes = 0usize;
        let mut content_utf16 = 0usize;
        while offset < bytes.len() {
            let ending = match bytes[offset] {
                b'\r' if bytes.get(offset + 1) == Some(&b'\n') => 2,
                b'\r' | b'\n' => 1,
                _ => 0,
            };
            if ending != 0 {
                offset += ending;
                starts.push((offset, Some((content_bytes, content_utf16))));
                content_bytes = 0;
                content_utf16 = 0;
                continue;
            }
            let scalar = text[offset..].chars().next().expect("valid UTF-8 suffix");
            content_bytes += scalar.len_utf8();
            content_utf16 += scalar.len_utf16();
            offset += scalar.len_utf8();
        }

        let ordinal = starts.partition_point(|(start, _)| *start <= cut) - 1;
        let exact = starts
            .get(ordinal)
            .filter(|(start, _)| *start == cut)
            .and_then(|(_, previous)| *previous);
        (
            u64::try_from(ordinal).expect("test line ordinal fits u64"),
            starts[ordinal].0 == cut,
            exact.map(|(content_bytes, content_utf16)| {
                (
                    u64::try_from(content_bytes).expect("test byte length fits u64"),
                    u64::try_from(content_utf16).expect("test UTF-16 length fits u64"),
                )
            }),
        )
    }

    fn assert_physical_line_query(store: &SourceStore, text: &str, cut: usize) {
        let certificate = store
            .certify_current_byte_cut(store.descriptor(), cut)
            .expect("current scalar cut");
        let actual = store
            .query_physical_line_at_cut(certificate)
            .expect("certified current cut");
        let expected = naive_physical_line_query(text, cut);
        assert_eq!(actual.line_ordinal(), expected.0, "cut {cut} in {text:?}");
        assert_eq!(
            actual.is_physical_line_start(),
            expected.1,
            "cut {cut} in {text:?}"
        );
        assert_eq!(
            actual
                .previous()
                .map(|previous| (previous.content_bytes(), previous.content_utf16())),
            expected.2,
            "cut {cut} in {text:?}"
        );
        let receipt = actual.receipt();
        assert!(receipt.tree_nodes_visited <= receipt.index_height.max(1));
        assert!(receipt.maximum_boundary_scratch_bytes <= 4 * 1024);
        assert!(receipt.boundary_bytes_scanned <= 4 * 1024);
        assert!(receipt.adjacent_bytes_read <= 2);
        assert_eq!(receipt.retained_source_roots, 0);
        assert_eq!(receipt.retained_source_bytes, 0);
    }

    fn deterministic_index(state: u64, len: usize) -> usize {
        let len = u64::try_from(len).expect("test collection length fits u64");
        usize::try_from(state % len).expect("modulo result fits usize")
    }

    #[test]
    fn completed_suffix_mapping_mints_root_and_revision_bound_proof() {
        let mut store = SourceStore::new("alpha\nsuffix", 8);
        let old_root = store.root_id();
        store
            .apply_edit(SourceRevision(0), 0..0, "new\n")
            .expect("valid prefix insertion");
        let current_root = store.root_id();
        let mut job = store
            .map_range_from(SourceRevision(0), 6..12)
            .expect("retained source lineage");

        assert_eq!(
            job.poll(1),
            MapStatus::ProvenRange(10..16),
            "unchanged suffix maps after the inserted prefix"
        );
        let proof = job.into_proof().expect("completed unchanged proof");
        assert_eq!(
            proof.from(),
            SourceSnapshotDescriptor {
                revision: SourceRevision(0),
                root: old_root,
                bytes: 12,
            }
        );
        assert_eq!(
            proof.to(),
            SourceSnapshotDescriptor {
                revision: SourceRevision(1),
                root: current_root,
                bytes: 16,
            }
        );
        assert_eq!(
            proof.mapping(),
            &ProvenSourceMapping::Range {
                from: 6..12,
                to: 10..16,
            }
        );
    }

    #[test]
    fn pending_or_changed_lineage_cannot_be_upgraded_to_a_proof() {
        let mut pending_store = SourceStore::new("abcdef", 8);
        pending_store
            .apply_edit(SourceRevision(0), 0..0, "x")
            .expect("valid edit");
        let pending = pending_store
            .map_boundary_from(SourceRevision(0), 3, BoundaryAffinity::After)
            .expect("retained source lineage");
        assert_eq!(
            pending.into_proof(),
            Err(MapStatus::Pending {
                processed_records: 0,
                remaining_records: 1,
            })
        );

        let mut changed_store = SourceStore::new("abcdef", 8);
        changed_store
            .apply_edit(SourceRevision(0), 2..4, "XX")
            .expect("valid overlapping edit");
        let mut changed = changed_store
            .map_range_from(SourceRevision(0), 1..5)
            .expect("retained source lineage");
        assert_eq!(
            changed.poll(1),
            MapStatus::Changed {
                at_revision: SourceRevision(1),
            }
        );
        assert_eq!(
            changed.into_proof(),
            Err(MapStatus::Changed {
                at_revision: SourceRevision(1),
            })
        );
    }

    #[test]
    fn scalar_snapshot_job_does_not_retain_a_crop_root() {
        let mut store = SourceStore::new("abc", 8);
        let old_lease = store.issue_parser_lease();
        let old_root = Arc::downgrade(&old_lease.root);
        store
            .apply_edit(SourceRevision(0), 0..0, "x")
            .expect("valid edit");
        let job = store
            .map_boundary_from(SourceRevision(0), 0, BoundaryAffinity::Before)
            .expect("scalar history retained");

        drop(old_lease);
        assert!(old_root.upgrade().is_none());
        assert_eq!(job.retention().retained_source_roots, 0);
    }

    #[test]
    fn a_broken_snapshot_chain_is_reported_under_poll_fuel() {
        let mut store = SourceStore::new("abc", 8);
        for revision in 0..3 {
            store
                .apply_edit(SourceRevision(revision), 0..0, "x")
                .expect("valid edit");
        }

        let slot = revision_slot(SourceRevision(1), store.lineage.capacity);
        let lookup = lookup_record(store.lineage.root.as_ref(), 0..store.lineage.capacity, slot);
        let mut corrupt = lookup.record.expect("second record exists").clone();
        corrupt.transition.base_root = SourceRootId(u64::MAX);
        let inserted = insert_record(
            store.lineage.root.as_ref(),
            0..store.lineage.capacity,
            slot,
            Arc::new(corrupt),
        );
        assert_eq!(inserted.shape_nodes_added, 0);
        store.lineage.root = Some(inserted.root);

        let mut job = store
            .map_boundary_from(SourceRevision(0), 0, BoundaryAffinity::Before)
            .expect("the first record is still structurally valid");
        assert_eq!(
            job.poll(1),
            MapStatus::Pending {
                processed_records: 1,
                remaining_records: 2,
            }
        );
        assert_eq!(job.poll(1), MapStatus::Failed(LineageError::BrokenChain));
        assert_eq!(job.metrics().poll_records_examined, 2);
        assert_eq!(job.metrics().poll_records_validated, 1);
        assert_eq!(job.poll(0), MapStatus::Failed(LineageError::BrokenChain));
    }

    fn corrupt_lineage_record_base_root(store: &mut SourceStore, revision: SourceRevision) {
        let slot = revision_slot(revision, store.lineage.capacity);
        let lookup = lookup_record(store.lineage.root.as_ref(), 0..store.lineage.capacity, slot);
        let mut corrupt = lookup.record.expect("target lineage record exists").clone();
        corrupt.transition.base_root = SourceRootId(u64::MAX);
        let inserted = insert_record(
            store.lineage.root.as_ref(),
            0..store.lineage.capacity,
            slot,
            Arc::new(corrupt),
        );
        assert_eq!(inserted.shape_nodes_added, 0);
        store.lineage.root = Some(inserted.root);
    }

    #[test]
    fn restart_and_adoption_jobs_validate_a_broken_second_record_after_yielding() {
        fn edited_store() -> (SourceStore, SourceSnapshotDescriptor) {
            let mut store = SourceStore::new("AAAABBBBCCCC", 8);
            let base = store.descriptor();
            for revision in 0..3 {
                store
                    .apply_edit(SourceRevision(revision), 5..5, "x")
                    .expect("middle insertion keeps both retained regions unchanged");
            }
            corrupt_lineage_record_base_root(&mut store, SourceRevision(1));
            (store, base)
        }

        let (restart_store, restart_base) = edited_store();
        let mut restart = restart_store
            .begin_restart_selection(restart_base, 4)
            .expect("the first lineage record remains valid");
        assert_eq!(
            restart.poll(1),
            RestartSelectionStatus::Pending {
                processed_records: 1,
                remaining_records: 2,
            }
        );
        let restart_failure = RestartSelectionStatus::Failed(RestartSelectionError::Lineage(
            LineageError::BrokenChain,
        ));
        assert_eq!(restart.poll(1), restart_failure);
        assert_eq!(restart.into_proof(), Err(restart_failure));

        let (adoption_store, adoption_base) = edited_store();
        let mut adoption = adoption_store
            .begin_lineage_adoption_bundle(adoption_base, 4, 8, BoundaryAffinity::After)
            .expect("the first lineage record remains valid");
        assert_eq!(
            adoption.poll(1),
            LineageAdoptionBundleStatus::Pending {
                processed_records: 1,
                remaining_records: 2,
            }
        );
        let adoption_failure = LineageAdoptionBundleStatus::Failed(
            LineageAdoptionBundleError::Lineage(LineageError::BrokenChain),
        );
        assert_eq!(adoption.poll(1), adoption_failure);
        assert_eq!(adoption.into_proof(), Err(adoption_failure));
    }

    #[test]
    fn adoption_bundle_maps_four_regions_in_one_fuelled_lineage_pass() {
        let mut store = SourceStore::new("AAAABBBBCCCC", 8);
        let base = store.descriptor();
        store
            .apply_edit(SourceRevision(0), 4..8, "XX")
            .expect("replace only the reparsed middle");
        store
            .apply_edit(SourceRevision(1), 5..5, "Q")
            .expect("insert again inside the reparsed middle");

        let mut job = store
            .begin_lineage_adoption_bundle(base, 4, 8, BoundaryAffinity::After)
            .expect("one exact lineage snapshot");
        assert_eq!(
            job.poll(0),
            LineageAdoptionBundleStatus::Pending {
                processed_records: 0,
                remaining_records: 2,
            }
        );
        assert_eq!(
            job.poll(1),
            LineageAdoptionBundleStatus::Pending {
                processed_records: 1,
                remaining_records: 1,
            }
        );
        assert_eq!(
            job.poll(1),
            LineageAdoptionBundleStatus::Proven {
                restart: 4,
                prefix: ProvenRetainedPrefix::Range {
                    from: 0..4,
                    to: 0..4,
                },
                convergence: 7,
                tail: 7..11,
            }
        );
        let metrics = job.metrics();
        assert_eq!(metrics.poll_records_examined, 2);
        assert_eq!(
            metrics.constructor_records_validated + metrics.poll_records_validated,
            2
        );
        assert_eq!(metrics.poll_mapping_attempts, 8);
        assert_eq!(metrics.poll_mappings_succeeded, 8);
        assert_eq!(metrics.records_copied, 0);

        let proof = job.into_proof().expect("all four regions are unchanged");
        assert_eq!(proof.from(), base);
        assert_eq!(proof.to(), store.descriptor());
        assert_eq!(
            proof.restart(),
            &ProvenSourceMapping::Boundary {
                from: 4,
                to: 4,
                affinity: BoundaryAffinity::Before,
            }
        );
        assert_eq!(
            proof.prefix(),
            &ProvenRetainedPrefix::Range {
                from: 0..4,
                to: 0..4,
            }
        );
        assert_eq!(
            proof.convergence(),
            &ProvenSourceMapping::Boundary {
                from: 8,
                to: 7,
                affinity: BoundaryAffinity::After,
            }
        );
        assert_eq!(
            proof.tail(),
            &ProvenSourceMapping::Range {
                from: 8..12,
                to: 7..11,
            }
        );
        assert_eq!(job_retained_source_roots(&store, base), 0);
    }

    fn job_retained_source_roots(store: &SourceStore, base: SourceSnapshotDescriptor) -> usize {
        store
            .begin_lineage_adoption_bundle(base, 4, 8, BoundaryAffinity::After)
            .expect("same retained scalar lineage")
            .retention()
            .retained_source_roots
    }

    #[test]
    fn adoption_bundle_identifies_the_first_changed_authority_region() {
        let mut prefix_store = SourceStore::new("AAAABBBBCCCC", 8);
        let prefix_base = prefix_store.descriptor();
        prefix_store
            .apply_edit(SourceRevision(0), 1..2, "Z")
            .expect("equal-length edit inside retained prefix");
        let mut prefix_job = prefix_store
            .begin_lineage_adoption_bundle(prefix_base, 4, 8, BoundaryAffinity::After)
            .expect("valid bundle request");
        assert_eq!(
            prefix_job.poll(1),
            LineageAdoptionBundleStatus::Changed {
                region: LineageAdoptionRegion::RetainedPrefix,
                at_revision: SourceRevision(1),
            }
        );
        assert_eq!(prefix_job.metrics().poll_mapping_attempts, 2);
        assert_eq!(
            prefix_job.into_proof(),
            Err(LineageAdoptionBundleStatus::Changed {
                region: LineageAdoptionRegion::RetainedPrefix,
                at_revision: SourceRevision(1),
            })
        );

        let mut tail_store = SourceStore::new("AAAABBBBCCCC", 8);
        let tail_base = tail_store.descriptor();
        tail_store
            .apply_edit(SourceRevision(0), 9..10, "Z")
            .expect("equal-length edit inside retained tail");
        let mut tail_job = tail_store
            .begin_lineage_adoption_bundle(tail_base, 4, 8, BoundaryAffinity::After)
            .expect("valid bundle request");
        assert_eq!(
            tail_job.poll(1),
            LineageAdoptionBundleStatus::Changed {
                region: LineageAdoptionRegion::RetainedTail,
                at_revision: SourceRevision(1),
            }
        );
        assert_eq!(tail_job.metrics().poll_mapping_attempts, 4);
        assert_eq!(tail_job.metrics().poll_mappings_succeeded, 3);
    }

    #[test]
    fn adoption_bundle_mints_empty_prefix_only_from_zero_before_boundary() {
        let mut store = SourceStore::new("BBBBCCCC", 8);
        let base = store.descriptor();
        store
            .apply_edit(SourceRevision(0), 0..4, "XX")
            .expect("replace the parsed prefix at source zero");
        let mut job = store
            .begin_lineage_adoption_bundle(base, 0, 4, BoundaryAffinity::After)
            .expect("zero restart is a typed fallback");
        assert_eq!(
            job.poll(1),
            LineageAdoptionBundleStatus::Proven {
                restart: 0,
                prefix: ProvenRetainedPrefix::Empty { from: 0, to: 0 },
                convergence: 2,
                tail: 2..6,
            }
        );
        assert_eq!(
            job.into_proof().expect("typed zero prefix").prefix(),
            &ProvenRetainedPrefix::Empty { from: 0, to: 0 }
        );
    }

    #[test]
    fn adoption_bundle_rejects_boundary_affinity_that_detaches_the_tail() {
        let mut store = SourceStore::new("AAAABBBBCCCC", 8);
        let base = store.descriptor();
        store
            .apply_edit(SourceRevision(0), 8..8, "XX")
            .expect("insert exactly at the convergence boundary");

        let mut before = store
            .begin_lineage_adoption_bundle(base, 4, 8, BoundaryAffinity::Before)
            .expect("the affinity is tested by the frozen bundle");
        assert_eq!(
            before.poll(1),
            LineageAdoptionBundleStatus::Changed {
                region: LineageAdoptionRegion::ConvergenceBoundary,
                at_revision: SourceRevision(1),
            },
            "a before-affinity boundary cannot authorize an after-insertion tail"
        );

        let mut after = store
            .begin_lineage_adoption_bundle(base, 4, 8, BoundaryAffinity::After)
            .expect("right-suffix-preserving affinity");
        assert_eq!(
            after.poll(1),
            LineageAdoptionBundleStatus::Proven {
                restart: 4,
                prefix: ProvenRetainedPrefix::Range {
                    from: 0..4,
                    to: 0..4,
                },
                convergence: 10,
                tail: 10..14,
            }
        );
    }

    #[test]
    fn adoption_bundle_rejects_echoed_snapshot_or_vacuous_suffix() {
        let store = SourceStore::new("AAAABBBBCCCC", 8);
        let base = store.descriptor();
        let forged = SourceSnapshotDescriptor {
            root: SourceRootId(base.root.0 + 1),
            ..base
        };
        assert!(matches!(
            store.begin_lineage_adoption_bundle(forged, 4, 8, BoundaryAffinity::After),
            Err(LineageAdoptionBundleError::SnapshotMismatch {
                supplied,
                lineage,
            }) if supplied == forged && lineage == base
        ));
        assert!(matches!(
            store.begin_lineage_adoption_bundle(base, 4, 12, BoundaryAffinity::After),
            Err(LineageAdoptionBundleError::InvalidBoundaries)
        ));
        assert!(matches!(
            store.begin_lineage_adoption_bundle(base, 9, 8, BoundaryAffinity::After),
            Err(LineageAdoptionBundleError::InvalidBoundaries)
        ));
    }

    #[test]
    fn adoption_bundle_keeps_one_historical_target_after_the_store_advances() {
        let mut store = SourceStore::new("AAAABBBBCCCC", 8);
        let base = store.descriptor();
        store
            .apply_edit(SourceRevision(0), 4..8, "XX")
            .expect("first middle edit");
        let frozen_target = store.descriptor();
        let mut job = store
            .begin_lineage_adoption_bundle(base, 4, 8, BoundaryAffinity::After)
            .expect("freeze revision-one lineage");

        store
            .apply_edit(SourceRevision(1), 4..6, "YYY")
            .expect("live source advances after job construction");
        assert_eq!(
            job.poll(1),
            LineageAdoptionBundleStatus::Proven {
                restart: 4,
                prefix: ProvenRetainedPrefix::Range {
                    from: 0..4,
                    to: 0..4,
                },
                convergence: 6,
                tail: 6..10,
            }
        );
        let proof = job.into_proof().expect("historical snapshot is coherent");
        assert_eq!(proof.to(), frozen_target);
        assert_ne!(proof.to(), store.descriptor());
    }

    #[test]
    fn restart_selection_keeps_an_unchanged_prefix_in_one_lineage_pass() {
        let mut store = SourceStore::new("AAAABBBBCCCC", 8);
        let base = store.descriptor();
        store
            .apply_edit(SourceRevision(0), 6..8, "X")
            .expect("edit remains after the preferred restart");
        let mut job = store
            .begin_restart_selection(base, 4)
            .expect("preferred checkpoint belongs to the base snapshot");
        assert_eq!(
            job.poll(0),
            RestartSelectionStatus::Pending {
                processed_records: 0,
                remaining_records: 1,
            }
        );
        assert_eq!(
            job.poll(1),
            RestartSelectionStatus::Selected {
                old: 4,
                current: 4,
                used_zero_fallback: false,
            }
        );
        let metrics = job.metrics();
        assert_eq!(metrics.poll_records_examined, 1);
        assert_eq!(
            metrics.constructor_records_validated + metrics.poll_records_validated,
            1
        );
        assert_eq!(metrics.poll_mapping_attempts, 3);
        assert_eq!(metrics.poll_mappings_succeeded, 3);
        assert_eq!(metrics.records_copied, 0);
        assert_eq!(job.retention().retained_source_roots, 0);

        let proof = job.into_proof().expect("preferred restart is proven");
        assert_eq!(proof.from(), base);
        assert_eq!(proof.to(), store.descriptor());
        assert_eq!(proof.selected_boundaries(), (4, 4));
        assert_eq!(
            proof.selection(),
            &ProvenRestartSelection::Preferred {
                boundary: ProvenSourceMapping::Boundary {
                    from: 4,
                    to: 4,
                    affinity: BoundaryAffinity::Before,
                },
                prefix: ProvenRetainedPrefix::Range {
                    from: 0..4,
                    to: 0..4,
                },
            }
        );
    }

    #[test]
    fn restart_selection_uses_before_affinity_for_an_exact_boundary_insert() {
        let mut store = SourceStore::new("AAAABBBBCCCC", 8);
        let base = store.descriptor();
        store
            .apply_edit(SourceRevision(0), 4..4, "XX")
            .expect("insert exactly at preferred restart");
        let mut job = store
            .begin_restart_selection(base, 4)
            .expect("preferred checkpoint");
        assert_eq!(
            job.poll(1),
            RestartSelectionStatus::Selected {
                old: 4,
                current: 4,
                used_zero_fallback: false,
            },
            "the inserted bytes remain after the selected parser start"
        );
        assert_eq!(
            job.into_proof()
                .expect("mapped preferred start")
                .selection(),
            &ProvenRestartSelection::Preferred {
                boundary: ProvenSourceMapping::Boundary {
                    from: 4,
                    to: 4,
                    affinity: BoundaryAffinity::Before,
                },
                prefix: ProvenRetainedPrefix::Range {
                    from: 0..4,
                    to: 0..4,
                },
            }
        );
    }

    #[test]
    fn restart_and_convergence_cuts_keep_their_sides_on_touching_edits() {
        let mut starting_at_restart = SourceStore::new("AAAABBBBCCCC", 8);
        let base = starting_at_restart.descriptor();
        starting_at_restart
            .apply_edit(SourceRevision(0), 4..6, "")
            .expect("delete beginning at the restart");
        let mut restart = starting_at_restart
            .begin_restart_selection(base, 4)
            .expect("preferred restart belongs to the base");
        assert_eq!(
            restart.poll(1),
            RestartSelectionStatus::Selected {
                old: 4,
                current: 4,
                used_zero_fallback: false,
            },
            "Before affinity starts before a deletion that begins at restart"
        );

        let mut ending_at_restart = SourceStore::new("AAAABBBBCCCC", 8);
        let base = ending_at_restart.descriptor();
        ending_at_restart
            .apply_edit(SourceRevision(0), 2..4, "")
            .expect("delete ending at the restart");
        let mut restart = ending_at_restart
            .begin_restart_selection(base, 4)
            .expect("preferred restart belongs to the base");
        assert_eq!(
            restart.poll(1),
            RestartSelectionStatus::Selected {
                old: 0,
                current: 0,
                used_zero_fallback: true,
            },
            "an edit ending at restart changes the complete retained prefix"
        );

        let mut ending_at_convergence = SourceStore::new("AAAABBBBCCCC", 8);
        let base = ending_at_convergence.descriptor();
        ending_at_convergence
            .apply_edit(SourceRevision(0), 6..8, "Z")
            .expect("replacement ends at convergence");
        let mut adoption = ending_at_convergence
            .begin_lineage_adoption_bundle(base, 4, 8, BoundaryAffinity::After)
            .expect("valid convergence cut");
        assert_eq!(
            adoption.poll(1),
            LineageAdoptionBundleStatus::Proven {
                restart: 4,
                prefix: ProvenRetainedPrefix::Range {
                    from: 0..4,
                    to: 0..4,
                },
                convergence: 7,
                tail: 7..11,
            },
            "After affinity follows a replacement ending at the suffix edge"
        );

        let mut starting_at_convergence = SourceStore::new("AAAABBBBCCCC", 8);
        let base = starting_at_convergence.descriptor();
        starting_at_convergence
            .apply_edit(SourceRevision(0), 8..10, "Z")
            .expect("replacement begins at convergence");
        let mut adoption = starting_at_convergence
            .begin_lineage_adoption_bundle(base, 4, 8, BoundaryAffinity::After)
            .expect("valid convergence cut");
        assert_eq!(
            adoption.poll(1),
            LineageAdoptionBundleStatus::Changed {
                region: LineageAdoptionRegion::RetainedTail,
                at_revision: SourceRevision(1),
            },
            "a replacement beginning at convergence changes the retained suffix"
        );
    }

    #[test]
    fn restart_selection_falls_back_to_independently_mapped_zero() {
        let mut store = SourceStore::new("AAAABBBBCCCC", 8);
        let base = store.descriptor();
        store
            .apply_edit(SourceRevision(0), 1..2, "Z")
            .expect("equal-length edit invalidates retained prefix");
        let mut job = store
            .begin_restart_selection(base, 4)
            .expect("preferred and zero are frozen together");
        assert_eq!(
            job.poll(1),
            RestartSelectionStatus::Selected {
                old: 0,
                current: 0,
                used_zero_fallback: true,
            }
        );
        assert_eq!(job.metrics().poll_mapping_attempts, 3);
        let proof = job.into_proof().expect("zero fallback is proven");
        assert_eq!(proof.selected_boundaries(), (0, 0));
        assert_eq!(
            proof.selection(),
            &ProvenRestartSelection::ZeroFallback {
                boundary: ProvenSourceMapping::Boundary {
                    from: 0,
                    to: 0,
                    affinity: BoundaryAffinity::Before,
                },
                rejected: Some(RejectedPreferredRestart {
                    region: RestartSelectionRegion::RetainedPrefix,
                    at_revision: SourceRevision(1),
                }),
            }
        );
    }

    #[test]
    fn restart_selection_rejects_echoed_snapshot_and_invalid_boundary() {
        let store = SourceStore::new("AAAABBBBCCCC", 8);
        let base = store.descriptor();
        let forged = SourceSnapshotDescriptor {
            bytes: base.bytes - 1,
            ..base
        };
        assert!(matches!(
            store.begin_restart_selection(forged, 4),
            Err(RestartSelectionError::SnapshotMismatch {
                supplied,
                lineage,
            }) if supplied == forged && lineage == base
        ));
        assert!(matches!(
            store.begin_restart_selection(base, base.bytes + 1),
            Err(RestartSelectionError::InvalidBoundary)
        ));
    }

    #[test]
    fn commonmark_line_query_covers_bof_lf_crlf_lone_cr_unicode_and_bare_eof() {
        for text in [
            "",
            "bare EOF 😀",
            "\n",
            "\r",
            "\r\n",
            "α\nβ\r\n😀\rbare",
            "a\r\nb\rc\nd😀",
        ] {
            let store = SourceStore::new(text, 8);
            for cut in (0..=text.len()).filter(|cut| text.is_char_boundary(*cut)) {
                assert_physical_line_query(&store, text, cut);
            }
        }

        let bare = SourceStore::new("bare EOF 😀", 8);
        let eof = bare
            .query_physical_line_at_cut(
                bare.certify_current_byte_cut(bare.descriptor(), bare.descriptor().bytes)
                    .unwrap(),
            )
            .unwrap();
        assert!(!eof.is_physical_line_start());
        assert_eq!(eof.previous(), None);

        let empty = SourceStore::new("", 8);
        let bof = empty
            .query_physical_line_at_cut(
                empty
                    .certify_current_byte_cut(empty.descriptor(), 0)
                    .unwrap(),
            )
            .unwrap();
        assert!(bof.is_physical_line_start());
        assert_eq!(bof.line_ordinal(), 0);
        assert_eq!(bof.previous(), None);
    }

    #[test]
    fn crlf_across_summary_and_crop_chunks_is_one_physical_ending() {
        let text = format!("x{}", "\r\n".repeat(20_000));
        let store = SourceStore::new(&text, 8);

        let mut crop_offset = 0usize;
        let mut crossing = None;
        let mut chunks = store.root.root.byte_slice(..).chunks();
        let mut previous = chunks.next().expect("large source has a first Crop chunk");
        crop_offset += previous.len();
        for chunk in chunks {
            if previous.as_bytes().last() == Some(&b'\r')
                && chunk.as_bytes().first() == Some(&b'\n')
            {
                crossing = Some(crop_offset);
                break;
            }
            previous = chunk;
            crop_offset += previous.len();
        }
        let crop_crossing = crossing.expect("fixture places CRLF across a Crop chunk boundary");
        assert_physical_line_query(&store, &text, crop_crossing);
        assert_physical_line_query(&store, &text, crop_crossing + 1);
        let inside = store
            .query_physical_line_at_cut(
                store
                    .certify_current_byte_cut(store.descriptor(), crop_crossing)
                    .unwrap(),
            )
            .unwrap();
        assert!(!inside.is_physical_line_start());

        let summary_crossing_text = format!("{}\r\nnext", "a".repeat(4_095));
        let summary_store = SourceStore::new(&summary_crossing_text, 8);
        assert_physical_line_query(&summary_store, &summary_crossing_text, 4_096);
        assert_physical_line_query(&summary_store, &summary_crossing_text, 4_097);
        let after = summary_store
            .query_physical_line_at_cut(
                summary_store
                    .certify_current_byte_cut(summary_store.descriptor(), 4_097)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(after.line_ordinal(), 1);
        assert_eq!(
            after.previous(),
            Some(SourcePhysicalLinePredecessor {
                content_bytes: 4_095,
                content_utf16: 4_095,
            })
        );
    }

    #[test]
    fn million_lone_cr_blank_lines_query_with_logarithmic_work() {
        let text = "\r".repeat(1_000_000);
        let store = SourceStore::new(&text, 8);
        let cut = 750_001;
        let query = store
            .query_physical_line_at_cut(
                store
                    .certify_current_byte_cut(store.descriptor(), cut)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(query.line_ordinal(), 750_001);
        assert!(query.is_physical_line_start());
        assert_eq!(
            query.previous(),
            Some(SourcePhysicalLinePredecessor {
                content_bytes: 0,
                content_utf16: 0,
            })
        );
        let receipt = query.receipt();
        assert!(receipt.tree_nodes_visited <= receipt.index_height);
        assert!(receipt.boundary_bytes_scanned <= 4 * 1024);
        assert_eq!(receipt.retained_source_bytes, 0);
        let retention = store.line_index_retention();
        assert_eq!(retention.retained_source_roots, 0);
        assert_eq!(retention.retained_source_bytes, 0);
        assert!(retention.leaves <= 2 * text.len().div_ceil(4 * 1024));
    }

    #[test]
    fn random_unicode_and_line_ending_edits_match_naive_oracle_without_rebuilds() {
        let mut text = "seed😀\r\nalpha\rbeta\n尾".repeat(40);
        let mut store = SourceStore::new(&text, 32);
        let replacements = ["", "x", "😀", "\n", "\r", "\r\n", "β\r😀\n"];
        let mut state = 0xD1B5_4A32_D192_ED03u64;

        for edit in 0..1_000 {
            let boundaries: Vec<_> = (0..=text.len())
                .filter(|offset| text.is_char_boundary(*offset))
                .collect();
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let first = boundaries[deterministic_index(state, boundaries.len())];
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let second = boundaries[deterministic_index(state, boundaries.len())];
            let range = first.min(second)..first.max(second);
            let replacement =
                replacements[deterministic_index(state.rotate_left(17), replacements.len())];
            store
                .apply_edit(store.revision(), range.clone(), replacement)
                .expect("differential edit remains scalar exact");
            text.replace_range(range, replacement);
            assert_eq!(store.query_view().materialize_for_testing(), text);

            let update = store.line_index_last_update();
            let retention = store.line_index_retention();
            assert_eq!(update.replacement_bytes_scanned, replacement.len());
            assert!(update.boundary_bytes_scanned <= 2 * 4 * 1024);
            assert!(update.maximum_boundary_scratch_bytes <= 4 * 1024);
            assert_eq!(update.retained_source_roots, 0);
            assert_eq!(update.retained_source_bytes, 0);
            assert_eq!(retention.retained_source_roots, 0);
            assert_eq!(retention.retained_source_bytes, 0);
            assert!(update.tree_nodes_visited <= 2 * update.old_nodes.max(1).ilog2() as usize + 6);
            assert!(
                update.coalescing_edge_nodes_visited <= 20 * retention.height.saturating_add(1)
            );
            if !text.is_empty() {
                assert!(retention.leaves <= 2 * text.len().div_ceil(4 * 1024));
                assert_eq!(retention.summary_nodes, 2 * retention.leaves - 1);
            }

            let current_boundaries: Vec<_> = (0..=text.len())
                .filter(|offset| text.is_char_boundary(*offset))
                .collect();
            for sample in 0..12 {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let cut = current_boundaries[deterministic_index(state, current_boundaries.len())];
                assert_physical_line_query(&store, &text, cut);
                if sample == 0 && edit % 100 == 0 {
                    assert_physical_line_query(&store, &text, 0);
                    assert_physical_line_query(&store, &text, text.len());
                }
            }
        }
    }

    #[test]
    fn physical_line_cut_rejects_wrong_and_stale_source_roots() {
        let mut store = SourceStore::new("a\r\nb", 8);
        let stale_descriptor = store.descriptor();
        let stale_cut = store
            .certify_current_byte_cut(stale_descriptor, 3)
            .expect("current line start");
        let other = SourceStore::new("a\r\nb", 8);
        assert!(matches!(
            other.certify_current_byte_cut(stale_descriptor, 3),
            Err(SourceStoreError::SnapshotMismatch { .. })
        ));
        store
            .apply_edit(store.revision(), 0..0, "x")
            .expect("advance source root");
        assert!(matches!(
            store.query_physical_line_at_cut(stale_cut),
            Err(SourceStoreError::SnapshotMismatch { .. })
        ));
        assert!(matches!(
            store.certify_current_byte_cut(store.descriptor(), store.descriptor().bytes + 1),
            Err(SourceStoreError::Source(SourceError::InvalidRange))
        ));
        let unicode = SourceStore::new("😀", 8);
        assert!(matches!(
            unicode.certify_current_byte_cut(unicode.descriptor(), 1),
            Err(SourceStoreError::Source(SourceError::NotCharBoundary(1)))
        ));
    }

    #[test]
    fn physical_line_start_check_is_scalar_exact_and_reads_at_most_two_adjacent_bytes() {
        let store = SourceStore::new("alpha\r\nbeta\rgamma\n😀tail", 8);
        let cases = [
            (0, true, 0),
            (5, false, 1),
            (6, false, 2),
            (7, true, 1),
            (11, false, 1),
            (12, true, 2),
            (17, false, 1),
            (18, true, 1),
            (19, false, 0),
            (22, false, 1),
            (26, false, 1),
        ];
        for (offset, expected, expected_reads) in cases {
            let check = store.check_physical_line_start(offset);
            assert_eq!(check.physical_line_start, expected, "offset {offset}");
            assert_eq!(check.adjacent_bytes_read, expected_reads, "offset {offset}");
            assert!(check.adjacent_bytes_read <= 2);
        }
        assert!(!store.check_physical_line_start(19).scalar_boundary);
        assert!(store.check_physical_line_start(18).scalar_boundary);
        assert_eq!(
            store
                .check_physical_line_start(store.descriptor().bytes)
                .adjacent_bytes_read,
            1
        );
    }

    #[test]
    fn resume_cursor_pair_is_same_root_and_rejects_wrong_scalar_or_crlf_cuts() {
        let store = SourceStore::new("a\nb\rc\r\nd😀", 8);
        let descriptor = store.descriptor();

        for (offset, physical_line_start, first_byte) in [
            (0, true, Some(b'a')),
            (2, true, Some(b'b')),
            (4, true, Some(b'c')),
            (7, true, Some(b'd')),
            (descriptor.bytes, false, None),
        ] {
            let pair = store.issue_resume_cursor_pair(offset).unwrap();
            assert_eq!(pair.descriptor(), descriptor);
            assert_eq!(pair.total_utf16(), "a\nb\rc\r\nd😀".encode_utf16().count());
            assert_eq!(pair.offset(), offset);
            assert_eq!(pair.is_physical_line_start(), physical_line_start);
            let (mut authoritative, mut recognition) = pair.into_cursors();
            assert_eq!(authoritative.source_identity(), descriptor.root);
            assert_eq!(recognition.source_identity(), descriptor.root);
            assert_eq!(authoritative.offset(), offset);
            assert_eq!(recognition.offset(), offset);
            assert_eq!(authoritative.next_byte().map(|byte| byte.byte), first_byte);
            assert_eq!(recognition.next_byte().map(|byte| byte.byte), first_byte);
        }

        assert_eq!(
            store.issue_resume_cursor_pair(3).unwrap_err(),
            SourceError::InvalidRange,
            "an ordinary mid-line scalar boundary is not resumable"
        );
        assert_eq!(
            store.issue_resume_cursor_pair(6).unwrap_err(),
            SourceError::InvalidRange,
            "the interior of CRLF is not resumable"
        );
        assert_eq!(
            store.issue_resume_cursor_pair(9).unwrap_err(),
            SourceError::NotCharBoundary(9),
            "a UTF-8 continuation byte is not a scalar boundary"
        );
        assert_eq!(
            store
                .issue_resume_cursor_pair(descriptor.bytes + 1)
                .unwrap_err(),
            SourceError::InvalidRange
        );
    }

    #[test]
    fn projection_session_replays_far_then_earlier_ranges_with_one_live_cursor_role() {
        let store = SourceStore::new("zero α middle omega", 8);
        let descriptor = store.descriptor();
        let baseline_roots = Arc::strong_count(&store.root);
        let mut session = store.issue_projection_session(17).unwrap();
        assert_eq!(Arc::strong_count(&store.root), baseline_roots + 1);

        let far = "zero α middle ".len();
        session.begin_pass_at(descriptor, 17, far).unwrap();
        assert_eq!(
            session.read_pass_byte(descriptor, 17, far - 1),
            Err(SourceProjectionSessionError::Invalid(
                "projection pass byte is outside its active source"
            ))
        );
        assert_eq!(session.read_pass_byte(descriptor, 17, far).unwrap(), b'o');
        assert_eq!(
            session.read_pass_byte(descriptor, 17, far + 2).unwrap(),
            b'e'
        );
        assert_eq!(
            session.read_pass_byte(descriptor, 17, far),
            Err(SourceProjectionSessionError::Invalid(
                "projection pass requested a non-sequential physical byte"
            ))
        );
        session.finish_pass(descriptor, 17).unwrap();

        session.begin_pass_at(descriptor, 17, 0).unwrap();
        assert_eq!(session.read_pass_byte(descriptor, 17, 0).unwrap(), b'z');
        assert_eq!(
            session.read_pass_byte(descriptor, 18, 1),
            Err(SourceProjectionSessionError::Invalid(
                "projection pass byte is outside its active source"
            ))
        );
        session.finish_pass(descriptor, 17).unwrap();
        assert_eq!(
            session.begin_pass_at(descriptor, 18, 0),
            Err(SourceProjectionSessionError::Invalid(
                "projection pass crossed source, cursor, or active-pass authority"
            ))
        );

        let receipt = session.retire(descriptor, 17).unwrap();
        assert_eq!(receipt.descriptor, descriptor);
        assert_eq!(receipt.cursor_nonce, 17);
        assert_eq!(receipt.passes_started, 2);
        assert_eq!(receipt.passes_finished, 2);
        assert_eq!(receipt.passes_cancelled, 0);
        assert_eq!(receipt.cursor_roles_minted, 3);
        assert_eq!(receipt.forward_cursor_jumps, 1);
        assert_eq!(receipt.source_bytes_read, 3);
        assert_eq!(receipt.maximum_live_cursor_roles, 1);
        assert!(receipt.maximum_cursor_scratch_bytes > 0);
        assert_eq!(Arc::strong_count(&store.root), baseline_roots);
    }

    #[test]
    fn projection_session_cancellation_releases_its_root_and_rejects_crossed_cleanup() {
        let store = SourceStore::new("abc", 8);
        let descriptor = store.descriptor();
        let baseline_roots = Arc::strong_count(&store.root);

        let mut session = store.issue_projection_session(23).unwrap();
        let weak = session.weak_root_for_test();
        session.begin_pass_at(descriptor, 23, 1).unwrap();
        assert_eq!(session.read_pass_byte(descriptor, 23, 1).unwrap(), b'b');
        session.cancel_pass(descriptor, 23).unwrap();
        assert_eq!(Arc::strong_count(&store.root), baseline_roots + 1);
        assert_eq!(
            session.begin_pass_at(descriptor, 24, 0),
            Err(SourceProjectionSessionError::Invalid(
                "projection pass crossed source, cursor, or active-pass authority"
            ))
        );
        let receipt = session.cancel(descriptor, 23).unwrap();
        assert_eq!(receipt.passes_started, 1);
        assert_eq!(receipt.passes_finished, 0);
        assert_eq!(receipt.passes_cancelled, 1);
        assert_eq!(Arc::strong_count(&store.root), baseline_roots);
        assert!(
            weak.upgrade().is_some(),
            "the store still owns its current root"
        );

        let mut other = SourceStore::new("old", 8);
        let old_descriptor = other.descriptor();
        let old_weak = Arc::downgrade(&other.root);
        let session = other.issue_projection_session(29).unwrap();
        let prepared = other.prepare_edit(old_descriptor, 0..3, "new").unwrap();
        let (_, retired) = other.commit_prepared_edit(prepared);
        drop(retired);
        assert!(
            old_weak.upgrade().is_some(),
            "the session pins only its old root"
        );
        session.cancel(old_descriptor, 29).unwrap();
        assert!(old_weak.upgrade().is_none());
    }
}

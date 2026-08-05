//! Research-only Crop-backed immutable source lease.
//!
//! The public contract deliberately exposes no Crop node, pointer, or subtree
//! identity. Flark mints exact revision identities and derives unchanged-range
//! mappings from the edit operation itself. A parser job owns one strong lease;
//! leaf descriptors retain only a weak lease plus copyable coordinates.

use std::fmt;
use std::ops::Range;
use std::sync::{Arc, Weak};

use crop::Rope;

use crate::source::{
    Anchor, AnchoredByte, BufferId, CertifiedSourceBoundary, CursorAdvanceMetrics,
    CursorStartMetrics, SourceCaptureError, SourceCaptureMetrics, SourceRootIdentity,
};

/// Exact root-bound range. It owns no source allocation or root handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CropRangeDescriptor {
    pub root: SourceRootIdentity,
    pub start: usize,
    pub end: usize,
}

impl CropRangeDescriptor {
    #[must_use]
    pub const fn len(self) -> usize {
        self.end - self.start
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// One exact unchanged coordinate mapping derived from an accepted edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CropUnchangedRegion {
    pub old: Range<usize>,
    pub new: Range<usize>,
}

/// Complete lineage for one immutable source splice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CropEditProvenance {
    pub from: SourceRootIdentity,
    pub to: SourceRootIdentity,
    pub edited_old: Range<usize>,
    pub inserted_bytes: usize,
    pub prefix: CropUnchangedRegion,
    pub suffix: CropUnchangedRegion,
}

impl CropEditProvenance {
    /// Maps a range only when the edit operation proves every byte unchanged.
    #[must_use]
    pub fn map_unchanged(
        &self,
        root: SourceRootIdentity,
        range: Range<usize>,
    ) -> Option<Range<usize>> {
        if root != self.from || range.start > range.end {
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

    /// Exact convergence proof for an old descriptor against the new root.
    #[must_use]
    pub fn map_descriptor(&self, descriptor: CropRangeDescriptor) -> Option<CropRangeDescriptor> {
        let mapped = self.map_unchanged(descriptor.root, descriptor.start..descriptor.end)?;
        Some(CropRangeDescriptor {
            root: self.to,
            start: mapped.start,
            end: mapped.end,
        })
    }
}

fn contains(container: &Range<usize>, candidate: &Range<usize>) -> bool {
    candidate.start >= container.start && candidate.end <= container.end
}

fn map_region(region: &CropUnchangedRegion, range: Range<usize>) -> Range<usize> {
    let start = region.new.start + (range.start - region.old.start);
    start..start + range.len()
}

/// Failure to bind a root-bound descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CropSourceError {
    WrongRoot,
    InvalidRange,
    NotCharBoundary(usize),
    LeaseExpired,
}

impl fmt::Display for CropSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongRoot => formatter.write_str("range descriptor belongs to another root"),
            Self::InvalidRange => formatter.write_str("invalid Crop source range"),
            Self::NotCharBoundary(offset) => {
                write!(
                    formatter,
                    "Crop source boundary {offset} splits a UTF-8 scalar"
                )
            }
            Self::LeaseExpired => formatter.write_str("Crop source snapshot lease expired"),
        }
    }
}

impl std::error::Error for CropSourceError {}

/// One immutable Crop root owned by the outer grammar snapshot/job.
#[derive(Debug)]
pub struct CropSnapshotLease {
    root: Rope,
    identity: SourceRootIdentity,
}

impl CropSnapshotLease {
    #[must_use]
    pub fn from_text(text: &str) -> Arc<Self> {
        Arc::new(Self {
            root: Rope::from(text),
            identity: SourceRootIdentity::mint(),
        })
    }

    #[must_use]
    pub const fn identity(&self) -> SourceRootIdentity {
        self.identity
    }

    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.root.byte_len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }

    #[must_use]
    pub fn is_char_boundary(&self, offset: usize) -> bool {
        offset <= self.len_bytes() && self.root.is_char_boundary(offset)
    }

    /// # Errors
    ///
    /// Rejects an out-of-bounds range or either endpoint splitting a UTF-8
    /// scalar.
    pub fn descriptor(&self, range: Range<usize>) -> Result<CropRangeDescriptor, CropSourceError> {
        validate_range(self, &range)?;
        Ok(CropRangeDescriptor {
            root: self.identity,
            start: range.start,
            end: range.end,
        })
    }

    /// # Panics
    ///
    /// Only if Crop violates the invariant that zero is a valid boundary for
    /// every immutable Rope.
    #[must_use]
    pub fn cursor(self: &Arc<Self>) -> CropSourceCursor {
        CropSourceCursor::new(self.clone(), 0)
            .expect("zero is within every valid Crop source snapshot")
            .0
    }

    /// # Errors
    ///
    /// Rejects an out-of-bounds offset or one that splits a UTF-8 scalar.
    pub fn cursor_at(
        self: &Arc<Self>,
        offset: usize,
    ) -> Result<(CropSourceCursor, CursorStartMetrics), CropSourceError> {
        CropSourceCursor::new(self.clone(), offset)
    }

    /// Applies one scalar-safe splice and returns exact operation-derived
    /// unchanged prefix/suffix mappings. No content hash authorizes reuse.
    ///
    /// # Errors
    ///
    /// Rejects an out-of-bounds edit or either endpoint splitting a UTF-8
    /// scalar.
    pub fn edit(
        self: &Arc<Self>,
        range: Range<usize>,
        replacement: &str,
    ) -> Result<(Arc<Self>, CropEditProvenance), CropSourceError> {
        validate_range(self, &range)?;
        let old_len = self.len_bytes();
        let mut root = self.root.clone();
        root.replace(range.clone(), replacement);
        let next = Arc::new(Self {
            root,
            identity: SourceRootIdentity::mint(),
        });
        let suffix_new_start = range.start + replacement.len();
        let provenance = CropEditProvenance {
            from: self.identity,
            to: next.identity,
            edited_old: range.clone(),
            inserted_bytes: replacement.len(),
            prefix: CropUnchangedRegion {
                old: 0..range.start,
                new: 0..range.start,
            },
            suffix: CropUnchangedRegion {
                old: range.end..old_len,
                new: suffix_new_start..next.len_bytes(),
            },
        };
        Ok((next, provenance))
    }

    #[must_use]
    pub fn materialize(&self) -> String {
        self.root.to_string()
    }

    #[must_use]
    pub(crate) const fn anchor(&self, offset: usize) -> Anchor {
        Anchor {
            buffer_id: BufferId::from_crop_root(self.identity),
            offset,
        }
    }

    pub(crate) fn copy_chunk_from(&self, offset: usize, target: &mut Vec<u8>) -> bool {
        target.clear();
        let Some(chunk) = self.root.byte_slice(offset..).chunks().next() else {
            return false;
        };
        target.extend_from_slice(chunk.as_bytes());
        true
    }
}

fn validate_range(source: &CropSnapshotLease, range: &Range<usize>) -> Result<(), CropSourceError> {
    if range.start > range.end || range.end > source.len_bytes() {
        return Err(CropSourceError::InvalidRange);
    }
    if !source.is_char_boundary(range.start) {
        return Err(CropSourceError::NotCharBoundary(range.start));
    }
    if !source.is_char_boundary(range.end) {
        return Err(CropSourceError::NotCharBoundary(range.end));
    }
    Ok(())
}

/// Metrics made explicit because safe Crop iterators borrow their Rope. This
/// owned cursor uses one reusable chunk scratch allocation; source bytes are
/// copied once per traversed Crop chunk, never per leaf descriptor or poll.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CropCursorMetrics {
    pub chunk_loads: usize,
    pub chunk_bytes_copied: usize,
    pub maximum_chunk_bytes: usize,
}

/// Long-lived source cursor whose current chunk survives cooperative polls.
#[derive(Debug)]
pub struct CropSourceCursor {
    lease: Arc<CropSnapshotLease>,
    offset: usize,
    chunk_start: usize,
    chunk: Vec<u8>,
    metrics: CropCursorMetrics,
}

impl CropSourceCursor {
    fn new(
        lease: Arc<CropSnapshotLease>,
        offset: usize,
    ) -> Result<(Self, CursorStartMetrics), CropSourceError> {
        if offset > lease.len_bytes() {
            return Err(CropSourceError::InvalidRange);
        }
        if !lease.is_char_boundary(offset) {
            return Err(CropSourceError::NotCharBoundary(offset));
        }
        let mut cursor = Self {
            lease,
            offset,
            chunk_start: offset,
            chunk: Vec::new(),
            metrics: CropCursorMetrics::default(),
        };
        let _ = cursor.load_chunk();
        Ok((
            cursor,
            // Crop does not expose internal B-tree node visits. Chunk loads
            // below count exact indexed operations; inventing one node per
            // lookup would make the shared receipt look more exact than it is.
            CursorStartMetrics::default(),
        ))
    }

    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    #[must_use]
    pub fn source_identity(&self) -> SourceRootIdentity {
        self.lease.identity
    }

    #[must_use]
    pub const fn metrics(&self) -> CropCursorMetrics {
        self.metrics
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

    /// Peeks one byte and reports only a chunk transition. The cursor and its
    /// scratch allocation remain live across later polls.
    pub fn peek_metered(&mut self) -> (Option<AnchoredByte>, CursorAdvanceMetrics) {
        let loaded = if self.chunk_offset().is_none() {
            self.load_chunk()
        } else {
            false
        };
        let Some(relative) = self.chunk_offset() else {
            return (None, CursorAdvanceMetrics::default());
        };
        (
            Some(AnchoredByte {
                byte: self.chunk[relative],
                anchor: self.lease.anchor(self.offset),
            }),
            CursorAdvanceMetrics {
                piece_transitions: usize::from(loaded),
                tree_nodes_descended: 0,
            },
        )
    }

    pub fn next_byte(&mut self) -> Option<AnchoredByte> {
        let item = self.peek_metered().0?;
        self.offset += 1;
        Some(item)
    }

    #[must_use]
    pub fn certified_boundary(&self) -> Option<crate::source::CertifiedSourceBoundary> {
        let boundary = if self.offset == 0 || self.offset == self.lease.len_bytes() {
            true
        } else if let Some(relative) = self.chunk_offset() {
            self.chunk[relative] & 0b1100_0000 != 0b1000_0000
        } else {
            // Crop chunks and byte slices are UTF-8 scalar aligned.
            true
        };
        boundary.then(|| {
            crate::source::CertifiedSourceBoundary::for_backend(self.lease.identity, self.offset)
        })
    }
}

/// Weak source reference retained by each logical leaf. It is not a root
/// lease; the outer parse job must keep the one strong lease alive.
#[derive(Clone, Debug)]
pub(crate) struct CropLeafSource {
    pub lease: Weak<CropSnapshotLease>,
}

/// Descriptor-only companion to `SourceCapture`. It records the exact scanned
/// envelope and checkpoint accounting, but owns no root, tree node, or source
/// byte. This is the key difference tested by the Crop adapter gate.
#[derive(Debug)]
pub(crate) struct CropRangeCapture {
    root: SourceRootIdentity,
    start: usize,
    next: usize,
    metrics: SourceCaptureMetrics,
}

impl CropRangeCapture {
    pub fn new(start: CertifiedSourceBoundary) -> Self {
        Self {
            root: start.source_identity(),
            start: start.offset(),
            next: start.offset(),
            metrics: SourceCaptureMetrics {
                ..SourceCaptureMetrics::default()
            },
        }
    }

    pub fn observe(
        &mut self,
        root: SourceRootIdentity,
        offset: usize,
    ) -> Result<(), SourceCaptureError> {
        if root != self.root {
            return Err(SourceCaptureError::DifferentSource);
        }
        if offset != self.next {
            return Err(SourceCaptureError::NonContiguous {
                expected: self.next,
                actual: offset,
            });
        }
        self.next += 1;
        self.metrics.bytes_observed += 1;
        Ok(())
    }

    pub const fn certified_start(&self) -> CertifiedSourceBoundary {
        CertifiedSourceBoundary::for_backend(self.root, self.start)
    }

    pub const fn certified_end(&self) -> CertifiedSourceBoundary {
        CertifiedSourceBoundary::for_backend(self.root, self.next)
    }

    pub const fn len(&self) -> usize {
        self.next - self.start
    }

    pub const fn metrics(&self) -> SourceCaptureMetrics {
        self.metrics
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn append_bounded(
        &mut self,
        suffix: Self,
        maximum_bytes: usize,
    ) -> Result<(), SourceCaptureError> {
        if suffix.root != self.root {
            return Err(SourceCaptureError::DifferentSource);
        }
        if suffix.start != self.next {
            return Err(SourceCaptureError::NonContiguous {
                expected: self.next,
                actual: suffix.start,
            });
        }
        let bytes = suffix.len();
        if bytes > maximum_bytes {
            return Err(SourceCaptureError::CheckpointBeyondBound {
                bytes,
                maximum: maximum_bytes,
            });
        }
        self.next = suffix.next;
        self.metrics.bytes_observed += suffix.metrics.bytes_observed;
        self.metrics.checkpoint_bytes_merged += suffix.metrics.checkpoint_bytes_merged + bytes;
        self.metrics.checkpoint_prefix_bytes_discarded +=
            suffix.metrics.checkpoint_prefix_bytes_discarded;
        self.metrics.max_atomic_checkpoint_bytes = self
            .metrics
            .max_atomic_checkpoint_bytes
            .max(suffix.metrics.max_atomic_checkpoint_bytes)
            .max(bytes);
        Ok(())
    }

    pub fn retain_suffix_bounded(
        mut self,
        start: CertifiedSourceBoundary,
        maximum_prefix_bytes: usize,
    ) -> Result<Self, SourceCaptureError> {
        if start.source_identity() != self.root {
            return Err(SourceCaptureError::DifferentSource);
        }
        if start.offset() < self.start || start.offset() > self.next {
            return Err(SourceCaptureError::NonContiguous {
                expected: self.start,
                actual: start.offset(),
            });
        }
        let discarded = start.offset() - self.start;
        if discarded > maximum_prefix_bytes {
            return Err(SourceCaptureError::CheckpointBeyondBound {
                bytes: discarded,
                maximum: maximum_prefix_bytes,
            });
        }
        self.start = start.offset();
        self.metrics.checkpoint_prefix_bytes_discarded += discarded;
        self.metrics.max_atomic_checkpoint_bytes =
            self.metrics.max_atomic_checkpoint_bytes.max(discarded);
        Ok(self)
    }

    pub fn finish(
        self,
        end: CertifiedSourceBoundary,
    ) -> Result<CropRangeDescriptor, SourceCaptureError> {
        if end.source_identity() != self.root {
            return Err(SourceCaptureError::DifferentSource);
        }
        if end.offset() != self.next {
            return Err(SourceCaptureError::EndBoundaryMismatch {
                expected: self.next,
                actual: end.offset(),
            });
        }
        Ok(CropRangeDescriptor {
            root: self.root,
            start: self.start,
            end: self.next,
        })
    }
}

impl CropLeafSource {
    pub fn bind(&self) -> Result<Arc<CropSnapshotLease>, CropSourceError> {
        self.lease.upgrade().ok_or(CropSourceError::LeaseExpired)
    }
}

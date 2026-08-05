//! Shared block/inline lexical frontier for the integrated parser slice.
//!
//! This module intentionally stops before `CommonMark` resolution. It proves a
//! narrower ownership boundary: block parsing can describe a logical inline
//! leaf without flattening source, and table classification plus later inline
//! resolution consume one immutable lexical event root. Backslash escapes,
//! backtick runs, emphasis runs, brackets, and table pipes are recognized only
//! by the lexer in this module.

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[cfg(feature = "crop-research")]
use crate::crop_source::{
    CropLeafSource, CropRangeDescriptor, CropSnapshotLease, CropSourceCursor,
};

use crate::packed::{
    PackedPage, PackedPageBuilder, PackedPageIterator, PackedPageSequence, PACKED_PAGE_BYTES,
};
use crate::source::{
    Anchor, AnchoredByte, CapturedSourceFragment, CertifiedSourceBoundary,
    FragmentExtractionMetrics, PersistentSource, SourceCaptureMetrics, SourceCursor,
    SourceFragment, SourceRootIdentity,
};

/// Maximum work accepted by the research poll contract.
pub const MAX_LEXER_POLL_WORK: usize = 4 * 1024;
/// Small gaps are cheaper to consume sequentially; larger excluded ranges seek
/// through the persistent source index in one bounded logarithmic operation.
pub const MAX_SEQUENTIAL_SOURCE_SKIP: usize = 64;

const EVENTS_PER_PAGE: usize = 128;
const MAX_PAGE_TREE_LEVELS: usize = usize::BITS as usize;
const ARC_COUNTER_BYTES: usize = 2 * std::mem::size_of::<usize>();
const SEGMENT_SOURCE: u8 = 0;
const SEGMENT_VIRTUAL_NEWLINE: u8 = 1;
const SEGMENT_VIRTUAL_SPACES: u8 = 2;
const SEGMENT_KIND_MASK: u8 = 0b11;
const SEGMENT_BACKWARD: u8 = 0b100;
const SEGMENT_DELTA_SHIFT: u32 = 3;
const SEGMENT_EXTENDED_DELTA: usize = 31;
const DESCRIPTOR_PAGE_BYTES: usize = PACKED_PAGE_BYTES;

static NEXT_SEGMENTED_LEAF_ID: AtomicU64 = AtomicU64::new(1);

fn mint_segmented_leaf_identity() -> SegmentedLeafIdentity {
    let id = NEXT_SEGMENTED_LEAF_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .unwrap_or_else(|_| panic!("segmented leaf identity space exhausted"));
    SegmentedLeafIdentity(id)
}

/// Deterministic lower bound for heap retained by one immutable representation.
///
/// This counts requested allocation bodies and reference-count words. It omits
/// allocator headers, size-class slack, stacks, and separately shared source
/// storage, so it must not be presented as process RSS.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetainedBytes {
    /// Encoded events or descriptor-array elements.
    pub payload: usize,
    /// Tree nodes, page records, and reference-count words.
    pub structure: usize,
    /// Number of distinct heap allocations retained by this root.
    pub allocations: usize,
}

impl RetainedBytes {
    /// Accounted payload plus structural bytes.
    #[must_use]
    pub const fn total(self) -> usize {
        self.payload + self.structure
    }

    fn add(&mut self, other: Self) {
        self.payload += other.payload;
        self.structure += other.structure;
        self.allocations += other.allocations;
    }
}

/// Why a byte exists in logical inline input but not in physical source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtualReason {
    /// A stripped container prefix leaves a logical line join.
    ContainerLineJoin,
    /// A partially consumed tab contributes logical indentation spaces.
    TabExpansion,
}

/// Attachment of a virtual byte to the physical source revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtualAttachment {
    /// Revision-local byte boundary used for diagnostics and range mapping.
    pub document_offset: usize,
    /// Stable adjacent byte anchor, absent only for an empty source.
    pub anchor: Option<Anchor>,
    /// True when the virtual byte is attached after `anchor` (normally EOF).
    pub after_anchor: bool,
}

/// A validated physical source slice retained by one logical leaf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhysicalSpan {
    /// Revision-local byte envelope. Stable identity comes from the anchors.
    pub document: Range<usize>,
    /// Anchor of the first byte.
    pub first: Anchor,
    /// Anchor of the last byte. The span is non-empty.
    pub last: Anchor,
}

/// One immutable descriptor in logical inline input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SegmentDescriptor {
    /// Bytes read directly from persistent source.
    Source(PhysicalSpan),
    /// Synthetic newline or tab-expansion spaces with no invented source span.
    Virtual {
        byte: u8,
        count: usize,
        attachment: VirtualAttachment,
        reason: VirtualReason,
    },
}

/// Errors detected while constructing or updating the frontier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrontierError {
    InvalidSourceRange {
        range: Range<usize>,
        source_len: usize,
    },
    NonMonotonicSourceRange(Range<usize>),
    NonMonotonicVirtualAnchor(usize),
    SourceBoundarySplitsScalar(usize),
    InvalidVirtualAnchor {
        offset: usize,
        source_len: usize,
    },
    InvalidVirtualCount,
    InvalidVirtualByte(u8),
    LogicalLengthOverflow,
    LeafAlreadyOpen,
    NoOpenLeaf,
    UnknownLeaf(LeafId),
    DuplicateLeaf(LeafId),
    CertifiedBoundaryFromDifferentSource,
    CapturedSourceFromDifferentRevision,
    CapturedSourceDoesNotCover {
        required: Range<usize>,
        captured: Range<usize>,
    },
}

impl fmt::Display for FrontierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSourceRange { range, source_len } => {
                write!(
                    formatter,
                    "invalid source range {range:?} for {source_len} bytes"
                )
            }
            Self::NonMonotonicSourceRange(range) => {
                write!(formatter, "non-monotonic source range {range:?}")
            }
            Self::NonMonotonicVirtualAnchor(offset) => {
                write!(formatter, "non-monotonic virtual anchor {offset}")
            }
            Self::SourceBoundarySplitsScalar(offset) => {
                write!(formatter, "source boundary {offset} splits a UTF-8 scalar")
            }
            Self::InvalidVirtualAnchor { offset, source_len } => write!(
                formatter,
                "virtual anchor {offset} is outside {source_len} source bytes"
            ),
            Self::InvalidVirtualCount => {
                formatter.write_str("virtual segment count must be nonzero")
            }
            Self::InvalidVirtualByte(byte) => {
                write!(formatter, "unsupported virtual byte 0x{byte:02x}")
            }
            Self::LogicalLengthOverflow => {
                formatter.write_str("logical leaf length exceeds addressable memory")
            }
            Self::LeafAlreadyOpen => formatter.write_str("an output leaf is already open"),
            Self::NoOpenLeaf => formatter.write_str("no output leaf is open"),
            Self::UnknownLeaf(id) => write!(formatter, "unknown output leaf {}", id.0),
            Self::DuplicateLeaf(id) => write!(formatter, "duplicate output leaf {}", id.0),
            Self::CertifiedBoundaryFromDifferentSource => {
                formatter.write_str("certified boundary belongs to another source revision")
            }
            Self::CapturedSourceFromDifferentRevision => {
                formatter.write_str("captured fragment belongs to another source revision")
            }
            Self::CapturedSourceDoesNotCover { required, captured } => write!(
                formatter,
                "captured source {captured:?} does not cover leaf provenance {required:?}"
            ),
        }
    }
}

impl std::error::Error for FrontierError {}

/// Auditable source-provenance construction for one sealed logical leaf.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SegmentedLeafConstructionMetrics {
    /// Random source-index nodes inspected by compatibility validation calls.
    pub boundary_index_nodes_visited: usize,
    pub boundary_bytes_examined: usize,
    /// Structural work of the random-access bounded-fragment fallback.
    pub fragment_extraction: FragmentExtractionMetrics,
    /// Work of a fragment captured during the caller's existing source pass.
    pub sequential_capture: SourceCaptureMetrics,
    /// True only for the O(source bytes + piece runs) sequential capture path.
    pub used_sequential_capture: bool,
}

/// Builder for an immutable segmented logical leaf.
#[derive(Debug)]
pub struct SegmentedLeafBuilder {
    source: BuilderSource,
    pages: PackedPageSequence,
    page: PackedPageBuilder,
    scratch: Vec<u8>,
    segment_count: usize,
    reference_offset: usize,
    logical_len: usize,
    first_document: Option<usize>,
    last_descriptor_virtual: bool,
    construction: SegmentedLeafConstructionMetrics,
}

#[derive(Debug)]
enum BuilderSource {
    Persistent(Arc<PersistentSource>),
    #[cfg(feature = "crop-research")]
    Crop(CropLeafSource),
}

impl BuilderSource {
    fn identity(&self) -> SourceRootIdentity {
        match self {
            Self::Persistent(source) => source.identity(),
            #[cfg(feature = "crop-research")]
            Self::Crop(source) => source
                .bind()
                .expect("parse job retains Crop lease")
                .identity(),
        }
    }

    fn len_bytes(&self) -> usize {
        match self {
            Self::Persistent(source) => source.len_bytes(),
            #[cfg(feature = "crop-research")]
            Self::Crop(source) => source
                .bind()
                .expect("parse job retains Crop lease")
                .len_bytes(),
        }
    }

    fn is_char_boundary_metered(
        &self,
        offset: usize,
    ) -> (bool, crate::source::CursorStartMetrics, usize) {
        match self {
            Self::Persistent(source) => source.is_char_boundary_metered(offset),
            #[cfg(feature = "crop-research")]
            Self::Crop(source) => {
                let source = source.bind().expect("parse job retains Crop lease");
                // Crop exposes the result, not the internal node/byte probes.
                // Exact chunk-index operation counts remain separate telemetry.
                (
                    source.is_char_boundary(offset),
                    crate::source::CursorStartMetrics::default(),
                    0,
                )
            }
        }
    }
}

impl SegmentedLeafBuilder {
    /// Starts a leaf over one persistent source revision.
    #[must_use]
    pub fn new(source: Arc<PersistentSource>) -> Self {
        Self {
            source: BuilderSource::Persistent(source),
            pages: PackedPageSequence::default(),
            page: PackedPageBuilder::new(),
            scratch: Vec::with_capacity(24),
            segment_count: 0,
            reference_offset: 0,
            logical_len: 0,
            first_document: None,
            last_descriptor_virtual: false,
            construction: SegmentedLeafConstructionMetrics::default(),
        }
    }

    /// Starts a descriptor-only leaf over the outer job's one Crop root lease.
    /// The builder and sealed leaf retain weak references, never another root
    /// lease or a per-leaf source capture.
    #[cfg(feature = "crop-research")]
    #[must_use]
    pub(crate) fn new_crop(source: &Arc<CropSnapshotLease>) -> Self {
        Self {
            source: BuilderSource::Crop(CropLeafSource {
                lease: Arc::downgrade(source),
            }),
            pages: PackedPageSequence::default(),
            page: PackedPageBuilder::new(),
            scratch: Vec::with_capacity(24),
            segment_count: 0,
            reference_offset: 0,
            logical_len: 0,
            first_document: None,
            last_descriptor_virtual: false,
            construction: SegmentedLeafConstructionMetrics::default(),
        }
    }

    /// Appends a non-empty monotonic physical source span.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty, out-of-bounds, non-monotonic, or
    /// non-UTF-8-boundary range.
    pub fn push_source(&mut self, range: Range<usize>) -> Result<(), FrontierError> {
        if range.start >= range.end || range.end > self.source.len_bytes() {
            return Err(FrontierError::InvalidSourceRange {
                range,
                source_len: self.source.len_bytes(),
            });
        }
        if range.start < self.reference_offset {
            return Err(FrontierError::NonMonotonicSourceRange(range));
        }
        let (start_boundary, start_index, start_bytes) =
            self.source.is_char_boundary_metered(range.start);
        self.construction.boundary_index_nodes_visited += start_index.index_nodes_visited;
        self.construction.boundary_bytes_examined += start_bytes;
        if !start_boundary {
            return Err(FrontierError::SourceBoundarySplitsScalar(range.start));
        }
        let (end_boundary, end_index, end_bytes) = self.source.is_char_boundary_metered(range.end);
        self.construction.boundary_index_nodes_visited += end_index.index_nodes_visited;
        self.construction.boundary_bytes_examined += end_bytes;
        if !end_boundary {
            return Err(FrontierError::SourceBoundarySplitsScalar(range.end));
        }
        self.push_source_validated(range)
    }

    /// Appends a source range whose endpoints were observed by the parser's
    /// existing sequential cursor. This path performs no source-tree lookup.
    ///
    /// # Errors
    ///
    /// Rejects capabilities from another source, empty ranges, or
    /// non-monotonic ranges.
    pub fn push_certified_source(
        &mut self,
        start: CertifiedSourceBoundary,
        end: CertifiedSourceBoundary,
    ) -> Result<(), FrontierError> {
        if start.source_identity() != self.source.identity()
            || end.source_identity() != self.source.identity()
        {
            return Err(FrontierError::CertifiedBoundaryFromDifferentSource);
        }
        self.push_source_validated(start.offset()..end.offset())
    }

    fn push_source_validated(&mut self, range: Range<usize>) -> Result<(), FrontierError> {
        if range.start >= range.end || range.end > self.source.len_bytes() {
            return Err(FrontierError::InvalidSourceRange {
                range,
                source_len: self.source.len_bytes(),
            });
        }
        if range.start < self.reference_offset {
            return Err(FrontierError::NonMonotonicSourceRange(range));
        }
        self.logical_len = self
            .logical_len
            .checked_add(range.len())
            .ok_or(FrontierError::LogicalLengthOverflow)?;
        self.scratch.clear();
        encode_segment_header(
            SEGMENT_SOURCE,
            self.reference_offset,
            range.start,
            &mut self.scratch,
        );
        encode_varint(range.len(), &mut self.scratch);
        self.append_scratch();
        self.segment_count += 1;
        self.reference_offset = range.end;
        self.first_document.get_or_insert(range.start);
        self.last_descriptor_virtual = false;
        Ok(())
    }

    /// Appends one virtual logical newline.
    ///
    /// # Errors
    ///
    /// Returns an error if `anchor_offset` lies beyond the source revision.
    pub fn push_virtual_newline(&mut self, anchor_offset: usize) -> Result<(), FrontierError> {
        self.push_virtual(b'\n', 1, anchor_offset, VirtualReason::ContainerLineJoin)
    }

    /// Certified equivalent of [`Self::push_virtual_newline`], avoiding a
    /// source-index boundary lookup.
    ///
    /// # Errors
    ///
    /// Rejects a capability from another revision or a non-monotonic anchor.
    pub fn push_certified_virtual_newline(
        &mut self,
        anchor: CertifiedSourceBoundary,
    ) -> Result<(), FrontierError> {
        self.push_certified_virtual(b'\n', 1, anchor, VirtualReason::ContainerLineJoin)
    }

    /// Appends spaces contributed by partial tab expansion.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero count or an anchor beyond the source.
    pub fn push_virtual_tab_spaces(
        &mut self,
        anchor_offset: usize,
        count: usize,
    ) -> Result<(), FrontierError> {
        self.push_virtual(b' ', count, anchor_offset, VirtualReason::TabExpansion)
    }

    /// Certified equivalent of [`Self::push_virtual_tab_spaces`].
    ///
    /// # Errors
    ///
    /// Rejects a zero count, a capability from another revision, or a
    /// non-monotonic anchor.
    pub fn push_certified_virtual_tab_spaces(
        &mut self,
        anchor: CertifiedSourceBoundary,
        count: usize,
    ) -> Result<(), FrontierError> {
        self.push_certified_virtual(b' ', count, anchor, VirtualReason::TabExpansion)
    }

    fn push_certified_virtual(
        &mut self,
        byte: u8,
        count: usize,
        anchor: CertifiedSourceBoundary,
        reason: VirtualReason,
    ) -> Result<(), FrontierError> {
        if anchor.source_identity() != self.source.identity() {
            return Err(FrontierError::CertifiedBoundaryFromDifferentSource);
        }
        self.push_virtual_validated(byte, count, anchor.offset(), reason)
    }

    fn push_virtual(
        &mut self,
        byte: u8,
        count: usize,
        anchor_offset: usize,
        reason: VirtualReason,
    ) -> Result<(), FrontierError> {
        if count == 0 {
            return Err(FrontierError::InvalidVirtualCount);
        }
        if !matches!(byte, b'\n' | b' ') {
            return Err(FrontierError::InvalidVirtualByte(byte));
        }
        if anchor_offset > self.source.len_bytes() {
            return Err(FrontierError::InvalidVirtualAnchor {
                offset: anchor_offset,
                source_len: self.source.len_bytes(),
            });
        }
        let (boundary, index, bytes) = self.source.is_char_boundary_metered(anchor_offset);
        self.construction.boundary_index_nodes_visited += index.index_nodes_visited;
        self.construction.boundary_bytes_examined += bytes;
        if !boundary {
            return Err(FrontierError::SourceBoundarySplitsScalar(anchor_offset));
        }
        self.push_virtual_validated(byte, count, anchor_offset, reason)
    }

    fn push_virtual_validated(
        &mut self,
        byte: u8,
        count: usize,
        anchor_offset: usize,
        reason: VirtualReason,
    ) -> Result<(), FrontierError> {
        if count == 0 {
            return Err(FrontierError::InvalidVirtualCount);
        }
        if !matches!(byte, b'\n' | b' ') {
            return Err(FrontierError::InvalidVirtualByte(byte));
        }
        if anchor_offset > self.source.len_bytes() {
            return Err(FrontierError::InvalidVirtualAnchor {
                offset: anchor_offset,
                source_len: self.source.len_bytes(),
            });
        }
        if anchor_offset < self.reference_offset {
            return Err(FrontierError::NonMonotonicVirtualAnchor(anchor_offset));
        }
        self.logical_len = self
            .logical_len
            .checked_add(count)
            .ok_or(FrontierError::LogicalLengthOverflow)?;
        let kind = match reason {
            VirtualReason::ContainerLineJoin => SEGMENT_VIRTUAL_NEWLINE,
            VirtualReason::TabExpansion => SEGMENT_VIRTUAL_SPACES,
        };
        self.scratch.clear();
        encode_segment_header(
            kind,
            self.reference_offset,
            anchor_offset,
            &mut self.scratch,
        );
        if kind == SEGMENT_VIRTUAL_SPACES {
            encode_varint(count, &mut self.scratch);
        }
        self.append_scratch();
        self.segment_count += 1;
        self.reference_offset = anchor_offset;
        self.first_document.get_or_insert(anchor_offset);
        self.last_descriptor_virtual = true;
        Ok(())
    }

    fn append_scratch(&mut self) {
        debug_assert!(self.scratch.len() <= DESCRIPTOR_PAGE_BYTES);
        if self.page.remaining() < self.scratch.len() {
            self.seal_page();
        }
        assert!(self.page.try_push_bytes(&self.scratch));
    }

    fn seal_page(&mut self) {
        if self.page.is_empty() {
            return;
        }
        let page = std::mem::take(&mut self.page).seal();
        self.pages = self.pages.push_back(page);
    }

    fn required_source_window(&mut self) -> Range<usize> {
        let Some(mut start) = self.first_document else {
            return 0..0;
        };
        let mut end = self.reference_offset;
        let source_len = self.source.len_bytes();
        if start == end && start == source_len && source_len > 0 {
            start -= 1;
            while start > 0 {
                let (boundary, index, bytes) = self.source.is_char_boundary_metered(start);
                self.construction.boundary_index_nodes_visited += index.index_nodes_visited;
                self.construction.boundary_bytes_examined += bytes;
                if boundary {
                    break;
                }
                start -= 1;
            }
        }
        if self.last_descriptor_virtual && end < source_len {
            end += 1;
            while end < source_len {
                let (boundary, index, bytes) = self.source.is_char_boundary_metered(end);
                self.construction.boundary_index_nodes_visited += index.index_nodes_visited;
                self.construction.boundary_bytes_examined += bytes;
                if boundary {
                    break;
                }
                end += 1;
            }
        }
        start..end
    }

    fn seal_pages(&mut self) {
        self.seal_page();
    }

    /// Seals with a fragment captured during the parser's existing sequential
    /// source pass. This is the preferred construction path: no per-leaf tree
    /// descent or payload copy is introduced.
    ///
    /// # Errors
    ///
    /// Rejects a capture from another revision or one that omits bytes needed
    /// for physical spans or virtual attachments.
    pub fn finish_with_capture(
        mut self,
        captured: CapturedSourceFragment,
    ) -> Result<SegmentedLeaf, FrontierError> {
        let required = self.required_source_window();
        if captured.source_identity() != self.source.identity() {
            return Err(FrontierError::CapturedSourceFromDifferentRevision);
        }
        let captured_range = captured.document_range();
        if required.start < captured_range.start || required.end > captured_range.end {
            return Err(FrontierError::CapturedSourceDoesNotCover {
                required,
                captured: captured_range,
            });
        }
        self.seal_pages();
        let (_, document, source_fragment, capture_metrics) = captured.into_parts();
        self.construction.sequential_capture = capture_metrics;
        self.construction.used_sequential_capture = true;
        Ok(self.finish_with_source(LeafSource::Persistent(source_fragment), document))
    }

    /// Seals root-bound descriptors for a Crop job. The parser's outer root
    /// lease remains the sole strong owner; this leaf carries only coordinates
    /// and a weak binding used while the job is alive.
    #[cfg(feature = "crop-research")]
    pub(crate) fn finish_crop(
        mut self,
        captured: CropRangeDescriptor,
    ) -> Result<SegmentedLeaf, FrontierError> {
        let required = self.required_source_window();
        if captured.root != self.source.identity() {
            return Err(FrontierError::CapturedSourceFromDifferentRevision);
        }
        let captured_range = captured.start..captured.end;
        if required.start < captured_range.start || required.end > captured_range.end {
            return Err(FrontierError::CapturedSourceDoesNotCover {
                required,
                captured: captured_range,
            });
        }
        self.seal_pages();
        let source = match &self.source {
            BuilderSource::Crop(source) => source.clone(),
            BuilderSource::Persistent(_) => {
                return Err(FrontierError::CapturedSourceFromDifferentRevision);
            }
        };
        Ok(self.finish_with_source(LeafSource::Crop(source), required))
    }

    /// Seals the immutable descriptor root. Clones retain only the bounded
    /// source envelope, never the originating `Arc<PersistentSource>`.
    ///
    /// This compatibility path extracts the envelope by indexed slicing and
    /// reports that cost in [`SegmentedLeaf::construction_metrics`]. New block
    /// parsing should use [`Self::finish_with_capture`].
    ///
    /// # Panics
    ///
    /// Panics only if earlier successful descriptor validation failed to
    /// establish a scalar-safe, in-bounds source envelope.
    #[must_use]
    pub fn finish(mut self) -> SegmentedLeaf {
        let required = self.required_source_window();
        let source = match &self.source {
            BuilderSource::Persistent(source) => source,
            #[cfg(feature = "crop-research")]
            BuilderSource::Crop(_) => panic!("Crop descriptor builders must use finish_crop"),
        };
        let (source_fragment, extraction) = source
            .fragment(required.clone())
            .expect("builder validated every source boundary");
        self.construction.fragment_extraction = extraction;
        self.seal_pages();
        self.finish_with_source(LeafSource::Persistent(source_fragment), required)
    }

    fn finish_with_source(
        self,
        leaf_source: LeafSource,
        document_window: Range<usize>,
    ) -> SegmentedLeaf {
        SegmentedLeaf {
            source_identity: self.source.identity(),
            source_len: self.source.len_bytes(),
            source: leaf_source,
            document_window,
            document_shift: 0,
            pages: self.pages,
            segment_count: self.segment_count,
            logical_len: self.logical_len,
            first_document: self.first_document,
            identity: mint_segmented_leaf_identity(),
            construction: self.construction,
        }
    }
}

/// Exact capability identity for one immutable logical-input root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SegmentedLeafIdentity(pub u64);

/// Exact failure to reuse a leaf against a newer current source root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeafRebindError {
    WindowOverflow,
    Source(crate::source::SourceError),
    StableAnchorLayoutChanged,
}

impl fmt::Display for LeafRebindError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WindowOverflow => formatter.write_str("rebased source window overflows usize"),
            Self::Source(error) => error.fmt(formatter),
            Self::StableAnchorLayoutChanged => formatter
                .write_str("current source no longer contains the exact anchored leaf window"),
        }
    }
}

impl std::error::Error for LeafRebindError {}

impl From<crate::source::SourceError> for LeafRebindError {
    fn from(value: crate::source::SourceError) -> Self {
        Self::Source(value)
    }
}

/// Immutable block-to-inline input. Source text is never flattened or copied.
#[derive(Clone, Debug)]
enum LeafSource {
    Persistent(SourceFragment),
    #[cfg(feature = "crop-research")]
    Crop(CropLeafSource),
}

#[derive(Clone, Debug)]
pub struct SegmentedLeaf {
    source_identity: SourceRootIdentity,
    source_len: usize,
    source: LeafSource,
    document_window: Range<usize>,
    document_shift: isize,
    pages: PackedPageSequence,
    segment_count: usize,
    logical_len: usize,
    first_document: Option<usize>,
    identity: SegmentedLeafIdentity,
    construction: SegmentedLeafConstructionMetrics,
}

impl SegmentedLeaf {
    /// Logical byte length including virtual bytes and excluding stripped prefixes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.logical_len
    }

    /// Whether the logical leaf contains no bytes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.logical_len == 0
    }

    /// Number of logical segment descriptors.
    #[must_use]
    pub const fn descriptor_count(&self) -> usize {
        self.segment_count
    }

    /// Exact identity shared by clones, independent of content hashes.
    #[must_use]
    pub const fn identity(&self) -> SegmentedLeafIdentity {
        self.identity
    }

    /// Source revision used by the currently bound fragment.
    #[must_use]
    pub const fn source_identity(&self) -> SourceRootIdentity {
        self.source_identity
    }

    /// Exact bounded physical source retained by this leaf.
    #[must_use]
    pub fn retained_source_metrics(&self) -> crate::source::BufferRetentionMetrics {
        match &self.source {
            LeafSource::Persistent(source) => source.buffer_retention(),
            #[cfg(feature = "crop-research")]
            LeafSource::Crop(_) => crate::source::BufferRetentionMetrics::default(),
        }
    }

    /// Sorted buffer IDs retained by this bounded leaf fragment. Lifetime
    /// audit helper; hot parser paths do not allocate this list.
    #[must_use]
    pub fn retained_source_buffer_ids(&self) -> Vec<crate::source::BufferId> {
        match &self.source {
            LeafSource::Persistent(source) => source.retained_buffer_ids(),
            #[cfg(feature = "crop-research")]
            LeafSource::Crop(_) => Vec::new(),
        }
    }

    /// Deduplicatable buffer allocations retained by this leaf fragment.
    #[must_use]
    pub fn retained_source_buffer_allocations(
        &self,
    ) -> Vec<crate::source::RetainedBufferAllocation> {
        match &self.source {
            LeafSource::Persistent(source) => source.retained_buffer_allocations(),
            #[cfg(feature = "crop-research")]
            LeafSource::Crop(_) => Vec::new(),
        }
    }

    /// Construction work, including whether provenance came from the
    /// sequential capture path or the indexed compatibility fallback.
    #[must_use]
    pub const fn construction_metrics(&self) -> SegmentedLeafConstructionMetrics {
        self.construction
    }

    /// Current revision-local envelope retained for physical and virtual
    /// provenance.
    #[must_use]
    pub fn document_window(&self) -> Range<usize> {
        self.document_window.clone()
    }

    /// Reuses this logical leaf against a current source root at a possibly
    /// shifted document position.
    ///
    /// The new window is extracted without payload copying and compared by
    /// exact immutable buffer IDs and ranges. Equal text, equal length, or a
    /// hash collision cannot authorize reuse. Interior edits therefore reject
    /// even when both endpoint offsets remain unchanged.
    ///
    /// This prototype comparison is synchronous O(piece runs) and is not a
    /// scheduler-admissible hot-path proof for a giant fragmented leaf. The
    /// production convergence seam must carry a persistent exact range-root
    /// capability or resume this comparison under fuel.
    ///
    /// # Errors
    ///
    /// Rejects an overflowing/out-of-bounds shifted window or any change to
    /// the exact stable-anchor piece layout.
    pub fn rebind_to_current(
        &self,
        current: &PersistentSource,
        current_window_start: usize,
    ) -> Result<(Self, FragmentExtractionMetrics), LeafRebindError> {
        let window_len = self.document_window.len();
        let current_end = current_window_start
            .checked_add(window_len)
            .ok_or(LeafRebindError::WindowOverflow)?;
        let (fragment, metrics) = current.fragment(current_window_start..current_end)?;
        let old_fragment = match &self.source {
            LeafSource::Persistent(source) => source,
            #[cfg(feature = "crop-research")]
            LeafSource::Crop(_) => return Err(LeafRebindError::StableAnchorLayoutChanged),
        };
        if !old_fragment.same_anchored_layout(&fragment) {
            return Err(LeafRebindError::StableAnchorLayoutChanged);
        }
        let old_start = isize::try_from(self.document_window.start)
            .map_err(|_| LeafRebindError::WindowOverflow)?;
        let new_start =
            isize::try_from(current_window_start).map_err(|_| LeafRebindError::WindowOverflow)?;
        let delta = new_start
            .checked_sub(old_start)
            .ok_or(LeafRebindError::WindowOverflow)?;
        let document_shift = self
            .document_shift
            .checked_add(delta)
            .ok_or(LeafRebindError::WindowOverflow)?;
        let mut rebound = self.clone();
        rebound.source_identity = current.identity();
        rebound.source_len = current.len_bytes();
        rebound.source = LeafSource::Persistent(fragment);
        rebound.document_window = current_window_start..current_end;
        rebound.document_shift = document_shift;
        Ok((rebound, metrics))
    }

    /// Number of independently sealed descriptor pages.
    #[must_use]
    pub fn descriptor_page_count(&self) -> usize {
        self.pages.page_count()
    }

    /// Decodes immutable descriptors in source order.
    ///
    /// Decoded anchors are recovered from the retained persistent source root;
    /// the packed stream stores no duplicate per-segment anchors.
    #[must_use]
    pub fn descriptors(&self) -> SegmentDescriptors<'_> {
        SegmentDescriptors {
            leaf: self,
            decoder: DescriptorDecoder::new(&self.pages, self.segment_count),
        }
    }

    /// Lower-bound retained bytes for the packed descriptor stream alone.
    ///
    /// Shared persistent-source storage and the small `SegmentedLeaf` handle
    /// are deliberately excluded. The returned payload is the exact encoded
    /// stream length; structure includes the requested reference-count words.
    #[must_use]
    pub fn retained_descriptor_bytes(&self) -> RetainedBytes {
        let pages = self.pages.page_count();
        RetainedBytes {
            payload: self.pages.payload_bytes(),
            structure: self.pages.accounted_structural_bytes(),
            allocations: self.pages.allocated_sequence_nodes() + pages * 2,
        }
    }

    /// Creates an amortized-O(1) sequential logical cursor.
    #[must_use]
    pub fn cursor(&self) -> LogicalCursor {
        LogicalCursor::new(self.clone())
    }

    fn shift_document_offset(&self, offset: usize) -> usize {
        offset
            .checked_add_signed(self.document_shift)
            .expect("validated leaf document shift remains addressable")
    }

    fn anchor_at_document(&self, offset: usize) -> Option<Anchor> {
        match &self.source {
            LeafSource::Persistent(source) => offset
                .checked_sub(self.document_window.start)
                .and_then(|relative| source.anchor_at(relative)),
            #[cfg(feature = "crop-research")]
            LeafSource::Crop(source) => {
                let lease = source.bind().expect("parse job retains Crop lease");
                (offset < lease.len_bytes()).then(|| lease.anchor(offset))
            }
        }
    }

    fn cursor_at_document(
        &self,
        offset: usize,
    ) -> Result<(LeafSourceCursor, crate::source::CursorStartMetrics), ()> {
        match &self.source {
            LeafSource::Persistent(source) => {
                let relative = offset.checked_sub(self.document_window.start).ok_or(())?;
                source
                    .cursor_at_metered(relative)
                    .map(|(cursor, metrics)| (LeafSourceCursor::Persistent(cursor), metrics))
                    .map_err(|_| ())
            }
            #[cfg(feature = "crop-research")]
            LeafSource::Crop(source) => source
                .bind()
                .and_then(|lease| lease.cursor_at(offset))
                .map(|(cursor, metrics)| (LeafSourceCursor::Crop(cursor), metrics))
                .map_err(|_| ()),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum PackedSegment {
    Source {
        start: usize,
        len: usize,
    },
    Virtual {
        byte: u8,
        count: usize,
        anchor_offset: usize,
        reason: VirtualReason,
    },
}

#[derive(Clone, Copy, Debug)]
enum CursorSegment {
    Source {
        start: usize,
        len: usize,
    },
    Virtual {
        byte: u8,
        count: usize,
        anchor_offset: usize,
        attachment: Option<VirtualAttachment>,
        reason: VirtualReason,
    },
}

impl From<PackedSegment> for CursorSegment {
    fn from(value: PackedSegment) -> Self {
        match value {
            PackedSegment::Source { start, len } => Self::Source { start, len },
            PackedSegment::Virtual {
                byte,
                count,
                anchor_offset,
                reason,
            } => Self::Virtual {
                byte,
                count,
                anchor_offset,
                attachment: None,
                reason,
            },
        }
    }
}

/// Decoding iterator over a compact immutable segmented-input root.
#[derive(Debug)]
pub struct SegmentDescriptors<'a> {
    leaf: &'a SegmentedLeaf,
    decoder: DescriptorDecoder,
}

impl Iterator for SegmentDescriptors<'_> {
    type Item = SegmentDescriptor;

    fn next(&mut self) -> Option<Self::Item> {
        let packed = self.decoder.next_segment()?;
        match packed {
            PackedSegment::Source { start, len } => {
                let start = self.leaf.shift_document_offset(start);
                let end = start.checked_add(len)?;
                Some(SegmentDescriptor::Source(PhysicalSpan {
                    document: start..end,
                    first: self.leaf.anchor_at_document(start)?,
                    last: self.leaf.anchor_at_document(end.checked_sub(1)?)?,
                }))
            }
            PackedSegment::Virtual {
                byte,
                count,
                anchor_offset,
                reason,
            } => Some(SegmentDescriptor::Virtual {
                byte,
                count,
                attachment: virtual_attachment(
                    self.leaf,
                    self.leaf.shift_document_offset(anchor_offset),
                ),
                reason,
            }),
        }
    }
}

fn encode_segment_header(kind: u8, previous: usize, current: usize, encoded: &mut Vec<u8>) {
    let (backward, magnitude) = if current >= previous {
        (false, current - previous)
    } else {
        (true, previous - current)
    };
    let inline_delta = magnitude.min(SEGMENT_EXTENDED_DELTA);
    let delta_bits = u8::try_from(inline_delta).expect("inline segment delta is at most 31");
    let direction = if backward { SEGMENT_BACKWARD } else { 0 };
    encoded.push((delta_bits << SEGMENT_DELTA_SHIFT) | direction | kind);
    if magnitude >= SEGMENT_EXTENDED_DELTA {
        encode_varint(magnitude, encoded);
    }
}

#[derive(Debug)]
struct DescriptorDecoder {
    pages: PackedPageIterator,
    page: Option<Arc<PackedPage>>,
    byte: usize,
    remaining: usize,
    reference_offset: usize,
}

impl DescriptorDecoder {
    fn new(pages: &PackedPageSequence, remaining: usize) -> Self {
        Self {
            pages: pages.pages(),
            page: None,
            byte: 0,
            remaining,
            reference_offset: 0,
        }
    }

    fn read_byte(&mut self) -> Option<u8> {
        loop {
            if let Some(page) = &self.page {
                if let Some(byte) = page.payload().get(self.byte).copied() {
                    self.byte += 1;
                    return Some(byte);
                }
            }
            self.page = self.pages.next();
            self.byte = 0;
            self.page.as_ref()?;
        }
    }

    fn read_varint(&mut self) -> Option<usize> {
        let mut value = 0usize;
        for shift in (0..usize::BITS).step_by(7) {
            let byte = self.read_byte()?;
            value |= usize::from(byte & 0x7f).checked_shl(shift)?;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn next_segment(&mut self) -> Option<PackedSegment> {
        if self.remaining == 0 {
            return None;
        }
        let header = self.read_byte()?;
        let inline_delta = usize::from(header >> SEGMENT_DELTA_SHIFT);
        let magnitude = if inline_delta == SEGMENT_EXTENDED_DELTA {
            self.read_varint()?
        } else {
            inline_delta
        };
        let current = if header & SEGMENT_BACKWARD == 0 {
            self.reference_offset.checked_add(magnitude)?
        } else {
            self.reference_offset.checked_sub(magnitude)?
        };
        let packed = match header & SEGMENT_KIND_MASK {
            SEGMENT_SOURCE => {
                let len = self.read_varint()?;
                self.reference_offset = current.checked_add(len)?;
                PackedSegment::Source {
                    start: current,
                    len,
                }
            }
            SEGMENT_VIRTUAL_NEWLINE => {
                self.reference_offset = current;
                PackedSegment::Virtual {
                    byte: b'\n',
                    count: 1,
                    anchor_offset: current,
                    reason: VirtualReason::ContainerLineJoin,
                }
            }
            SEGMENT_VIRTUAL_SPACES => {
                let count = self.read_varint()?;
                self.reference_offset = current;
                PackedSegment::Virtual {
                    byte: b' ',
                    count,
                    anchor_offset: current,
                    reason: VirtualReason::TabExpansion,
                }
            }
            _ => return None,
        };
        self.remaining -= 1;
        Some(packed)
    }
}

fn virtual_attachment(leaf: &SegmentedLeaf, document_offset: usize) -> VirtualAttachment {
    let (anchor, after_anchor) = if document_offset < leaf.source_len {
        (leaf.anchor_at_document(document_offset), false)
    } else if document_offset > 0 {
        (leaf.anchor_at_document(document_offset - 1), true)
    } else {
        (None, false)
    };
    VirtualAttachment {
        document_offset,
        anchor,
        after_anchor,
    }
}

/// Physical or virtual provenance of one logical byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalOrigin {
    Source(Anchor),
    Virtual {
        attachment: VirtualAttachment,
        reason: VirtualReason,
    },
}

/// One byte presented to the shared lexical owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogicalByte {
    pub byte: u8,
    pub logical_offset: usize,
    pub origin: LogicalOrigin,
}

/// One bounded cursor transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorStep {
    /// A logical byte became available.
    Byte(LogicalByte),
    /// A descriptor boundary or excluded physical byte was advanced.
    Progress,
    /// The logical leaf is exhausted.
    Done,
}

/// Auditable sequential-cursor counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CursorMetrics {
    pub operations: usize,
    pub logical_bytes: usize,
    pub descriptor_entries: usize,
    /// Excluded physical bytes actually read while advancing between spans.
    pub excluded_source_bytes: usize,
    /// Excluded physical bytes bypassed without inspection by indexed seeks.
    pub skipped_source_bytes: usize,
    pub source_seek_operations: usize,
    pub source_seek_index_nodes: usize,
    /// Extra copy required only by source backends whose safe public chunk
    /// iterators borrow the root. The current Crop research adapter keeps one
    /// reusable chunk scratch across polls and reports those copies here.
    pub source_chunk_loads: usize,
    pub source_chunk_bytes_copied: usize,
    pub maximum_source_chunk_bytes_copied: usize,
}

/// Sequential segmented-input cursor. `step` never searches the descriptor list.
#[derive(Debug)]
enum LeafSourceCursor {
    Persistent(SourceCursor),
    #[cfg(feature = "crop-research")]
    Crop(CropSourceCursor),
}

impl LeafSourceCursor {
    fn next(&mut self) -> Option<AnchoredByte> {
        match self {
            Self::Persistent(cursor) => cursor.next(),
            #[cfg(feature = "crop-research")]
            Self::Crop(cursor) => cursor.next_byte(),
        }
    }

    fn chunk_metrics(&self) -> (usize, usize, usize) {
        match self {
            Self::Persistent(_) => (0, 0, 0),
            #[cfg(feature = "crop-research")]
            Self::Crop(cursor) => {
                let metrics = cursor.metrics();
                (
                    metrics.chunk_loads,
                    metrics.chunk_bytes_copied,
                    metrics.maximum_chunk_bytes,
                )
            }
        }
    }
}

#[derive(Debug)]
pub struct LogicalCursor {
    leaf: SegmentedLeaf,
    source_cursor: Option<LeafSourceCursor>,
    source_lookahead: Option<AnchoredByte>,
    last_source_anchor: Option<Anchor>,
    physical_offset: usize,
    descriptors: DescriptorDecoder,
    current: Option<CursorSegment>,
    segment_offset: usize,
    logical_offset: usize,
    metrics: CursorMetrics,
}

impl LogicalCursor {
    fn new(leaf: SegmentedLeaf) -> Self {
        let cursor_start = leaf.first_document.map(|offset| {
            let offset = leaf.shift_document_offset(offset);
            if offset == leaf.source_len && offset > 0 {
                offset - 1
            } else {
                offset
            }
        });
        let source_cursor = cursor_start.map(|offset| {
            leaf.cursor_at_document(offset)
                .expect("validated source offset")
                .0
        });
        let (source_chunk_loads, source_chunk_bytes_copied, maximum_source_chunk_bytes_copied) =
            source_cursor
                .as_ref()
                .map_or((0, 0, 0), LeafSourceCursor::chunk_metrics);
        Self {
            physical_offset: cursor_start.unwrap_or(0),
            descriptors: DescriptorDecoder::new(&leaf.pages, leaf.segment_count),
            leaf,
            source_cursor,
            source_lookahead: None,
            last_source_anchor: None,
            current: None,
            segment_offset: 0,
            logical_offset: 0,
            metrics: CursorMetrics {
                source_chunk_loads,
                source_chunk_bytes_copied,
                maximum_source_chunk_bytes_copied,
                ..CursorMetrics::default()
            },
        }
    }

    /// Performs at most one descriptor, physical-byte, or logical-byte transition.
    ///
    /// # Panics
    ///
    /// Panics only if an immutable leaf violates builder invariants, such as a
    /// validated physical span extending beyond its retained source revision.
    pub fn step(&mut self) -> CursorStep {
        if self.current.is_none() {
            let Some(segment) = self.descriptors.next_segment() else {
                return CursorStep::Done;
            };
            self.current = Some(self.shift_segment(segment).into());
            self.metrics.operations += 1;
            self.metrics.descriptor_entries += 1;
            return CursorStep::Progress;
        }

        match self.current.expect("entered cursor has a decoded segment") {
            CursorSegment::Source { start, len } => {
                let target = start + self.segment_offset;
                if self.physical_offset < target {
                    if self.seek_large_gap(target) {
                        self.metrics.operations += 1;
                        return CursorStep::Progress;
                    }
                    let _ = self
                        .take_source_byte()
                        .expect("validated monotonic source range remains in source");
                    self.metrics.operations += 1;
                    self.metrics.excluded_source_bytes += 1;
                    return CursorStep::Progress;
                }

                let item = self
                    .take_source_byte()
                    .expect("validated source span remains in source");
                let logical = LogicalByte {
                    byte: item.byte,
                    logical_offset: self.logical_offset,
                    origin: LogicalOrigin::Source(item.anchor),
                };
                self.segment_offset += 1;
                self.logical_offset += 1;
                self.metrics.operations += 1;
                self.metrics.logical_bytes += 1;
                if self.segment_offset == len {
                    self.advance_segment();
                }
                CursorStep::Byte(logical)
            }
            CursorSegment::Virtual {
                byte,
                count,
                anchor_offset,
                attachment,
                reason,
            } => {
                if self.physical_offset < anchor_offset {
                    if self.seek_large_gap(anchor_offset) {
                        self.metrics.operations += 1;
                        return CursorStep::Progress;
                    }
                    let _ = self
                        .take_source_byte()
                        .expect("validated virtual anchor remains in source");
                    self.metrics.operations += 1;
                    self.metrics.excluded_source_bytes += 1;
                    return CursorStep::Progress;
                }
                let attachment = attachment.unwrap_or_else(|| {
                    let resolved = if anchor_offset < self.leaf.source_len {
                        VirtualAttachment {
                            document_offset: anchor_offset,
                            anchor: self.peek_source_byte().map(|item| item.anchor),
                            after_anchor: false,
                        }
                    } else {
                        VirtualAttachment {
                            document_offset: anchor_offset,
                            anchor: self.last_source_anchor,
                            after_anchor: anchor_offset > 0,
                        }
                    };
                    self.current = Some(CursorSegment::Virtual {
                        byte,
                        count,
                        anchor_offset,
                        attachment: Some(resolved),
                        reason,
                    });
                    resolved
                });
                let logical = LogicalByte {
                    byte,
                    logical_offset: self.logical_offset,
                    origin: LogicalOrigin::Virtual { attachment, reason },
                };
                self.segment_offset += 1;
                self.logical_offset += 1;
                self.metrics.operations += 1;
                self.metrics.logical_bytes += 1;
                if self.segment_offset == count {
                    self.advance_segment();
                }
                CursorStep::Byte(logical)
            }
        }
    }

    fn advance_segment(&mut self) {
        self.current = None;
        self.segment_offset = 0;
    }

    fn peek_source_byte(&mut self) -> Option<AnchoredByte> {
        if self.source_lookahead.is_none() {
            self.source_lookahead = self.read_source_cursor();
        }
        self.source_lookahead
    }

    fn take_source_byte(&mut self) -> Option<AnchoredByte> {
        let item = self
            .source_lookahead
            .take()
            .or_else(|| self.read_source_cursor())?;
        self.physical_offset += 1;
        self.last_source_anchor = Some(item.anchor);
        Some(item)
    }

    fn read_source_cursor(&mut self) -> Option<AnchoredByte> {
        let cursor = self.source_cursor.as_mut()?;
        let before = cursor.chunk_metrics();
        let item = cursor.next();
        let after = cursor.chunk_metrics();
        self.metrics.source_chunk_loads += after.0 - before.0;
        self.metrics.source_chunk_bytes_copied += after.1 - before.1;
        self.metrics.maximum_source_chunk_bytes_copied =
            self.metrics.maximum_source_chunk_bytes_copied.max(after.2);
        item
    }

    fn seek_large_gap(&mut self, target: usize) -> bool {
        let gap = target - self.physical_offset;
        if gap <= MAX_SEQUENTIAL_SOURCE_SKIP {
            return false;
        }
        // Start one byte before the target so EOF virtual attachments retain
        // an exact adjacent anchor without a second index lookup. Only that one
        // excluded byte is inspected; the rest are explicitly metered skipped.
        let cursor_offset = target - 1;
        let (cursor, start) = self
            .leaf
            .cursor_at_document(cursor_offset)
            .expect("validated monotonic descriptor target remains in source");
        let chunk = cursor.chunk_metrics();
        self.metrics.source_chunk_loads += chunk.0;
        self.metrics.source_chunk_bytes_copied += chunk.1;
        self.metrics.maximum_source_chunk_bytes_copied =
            self.metrics.maximum_source_chunk_bytes_copied.max(chunk.2);
        self.source_cursor = Some(cursor);
        self.source_lookahead = None;
        self.physical_offset = cursor_offset;
        let _ = self
            .take_source_byte()
            .expect("target predecessor remains in source");
        self.metrics.excluded_source_bytes += 1;
        self.metrics.skipped_source_bytes += gap - 1;
        self.metrics.source_seek_operations += 1;
        self.metrics.source_seek_index_nodes += start.index_nodes_visited;
        true
    }

    fn shift_segment(&self, segment: PackedSegment) -> PackedSegment {
        match segment {
            PackedSegment::Source { start, len } => PackedSegment::Source {
                start: self.leaf.shift_document_offset(start),
                len,
            },
            PackedSegment::Virtual {
                byte,
                count,
                anchor_offset,
                reason,
            } => PackedSegment::Virtual {
                byte,
                count,
                anchor_offset: self.leaf.shift_document_offset(anchor_offset),
                reason,
            },
        }
    }

    /// Current operation counters.
    #[must_use]
    pub const fn metrics(&self) -> CursorMetrics {
        self.metrics
    }
}

/// Start point of a lexical event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogicalPoint {
    pub offset: usize,
}

impl From<LogicalByte> for LogicalPoint {
    fn from(value: LogicalByte) -> Self {
        Self {
            offset: value.logical_offset,
        }
    }
}

/// Candidate event recognized by the one shared lexer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexicalEventKind {
    BackslashEscape { escaped: u8 },
    BacktickRun { len: usize },
    EmphasisRun { marker: u8, len: usize },
    OpenBracket,
    CloseBracket,
    TablePipe,
}

/// Exact logical range for one lexical event.
///
/// Provenance is intentionally not duplicated per event. Logical offsets map
/// through the separate immutable [`SegmentedLeaf`] descriptor root retained
/// by [`LexicalView`]. Keeping anchors out of dense event pages is necessary:
/// punctuation-heavy input can produce one candidate event per byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LexicalEvent {
    pub kind: LexicalEventKind,
    pub start: LogicalPoint,
    pub end: usize,
}

#[derive(Debug)]
struct LexicalEventPage {
    encoded: Box<[u8]>,
    events: usize,
    preceding_end: usize,
}

#[derive(Debug)]
enum PageTree {
    Leaf(Arc<LexicalEventPage>),
    Branch {
        left: Arc<PageTree>,
        right: Arc<PageTree>,
        pages: usize,
        events: usize,
    },
}

impl PageTree {
    fn pages(&self) -> usize {
        match self {
            Self::Leaf(_) => 1,
            Self::Branch { pages, .. } => *pages,
        }
    }

    fn events(&self) -> usize {
        match self {
            Self::Leaf(page) => page.events,
            Self::Branch { events, .. } => *events,
        }
    }

    fn retained_bytes(&self) -> RetainedBytes {
        let mut retained = RetainedBytes {
            payload: 0,
            structure: std::mem::size_of::<Self>() + ARC_COUNTER_BYTES,
            allocations: 1,
        };
        match self {
            Self::Leaf(page) => {
                retained.add(RetainedBytes {
                    payload: page.encoded.len(),
                    structure: std::mem::size_of::<LexicalEventPage>() + ARC_COUNTER_BYTES,
                    allocations: 2,
                });
            }
            Self::Branch { left, right, .. } => {
                retained.add(left.retained_bytes());
                retained.add(right.retained_bytes());
            }
        }
        retained
    }

    fn concat(left: Arc<Self>, right: Arc<Self>) -> Arc<Self> {
        Arc::new(Self::Branch {
            pages: left.pages() + right.pages(),
            events: left.events() + right.events(),
            left,
            right,
        })
    }
}

/// Immutable view supplied to a grammar consumer.
#[derive(Clone, Debug, Default)]
pub struct LexicalView {
    root: Option<Arc<PageTree>>,
    input: Option<SegmentedLeaf>,
}

impl LexicalView {
    /// Number of immutable event pages.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.root.as_deref().map_or(0, PageTree::pages)
    }

    /// Number of lexical events.
    #[must_use]
    pub fn event_count(&self) -> usize {
        self.root.as_deref().map_or(0, PageTree::events)
    }

    /// Iterates source order without flattening the page tree.
    #[must_use]
    pub fn events(&self) -> LexicalEvents {
        LexicalEvents::new(self.root.clone())
    }

    /// Creates a fixed-storage, transition-metered cursor over the immutable
    /// event tree.
    ///
    /// Unlike [`LexicalEvents`], this cursor never grows a traversal `Vec` and
    /// never hides a tree walk inside `Iterator::next`: one call to
    /// [`MeteredLexicalCursor::step`] visits at most one tree node or decodes
    /// at most one bounded lexical record.
    #[must_use]
    pub fn metered_cursor(&self) -> MeteredLexicalCursor {
        MeteredLexicalCursor::new(self.root.clone())
    }

    /// Exact proof that two consumers received the same immutable lexical root.
    #[must_use]
    pub fn shares_root_with(&self, other: &Self) -> bool {
        match (&self.root, &other.root) {
            (None, None) => match (&self.input, &other.input) {
                (None, None) => true,
                (Some(left), Some(right)) => left.identity() == right.identity(),
                _ => false,
            },
            (Some(left), Some(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }

    /// Segmented origin map referenced by encoded logical offsets.
    #[must_use]
    pub fn input(&self) -> Option<&SegmentedLeaf> {
        self.input.as_ref()
    }

    /// Lower-bound bytes retained by compact event pages and their tree.
    ///
    /// The separately shared segmented input is excluded; call
    /// [`SegmentedLeaf::retained_descriptor_bytes`] for that root.
    #[must_use]
    pub fn retained_event_bytes(&self) -> RetainedBytes {
        self.root
            .as_deref()
            .map_or_else(RetainedBytes::default, PageTree::retained_bytes)
    }
}

/// Source-ordered event iterator over an immutable page tree.
pub struct LexicalEvents {
    pending: Vec<Arc<PageTree>>,
    current: Option<PageDecoder>,
}

/// Exact work performed by a fixed-storage lexical cursor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LexicalCursorMetrics {
    /// Reference-counted root handles acquired when cursors are created.
    pub root_clones: usize,
    /// Immutable tree nodes inspected.
    pub tree_nodes: usize,
    /// Leaf pages entered.
    pub pages_entered: usize,
    /// Lexical records decoded.
    pub events: usize,
    /// Encoded record bytes actually inspected.
    pub decoded_bytes: usize,
}

/// One auditable lexical-cursor transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexicalCursorStep {
    /// One immutable tree node was visited or one exhausted page was left.
    Progress,
    /// One complete lexical record was decoded.
    Event(LexicalEvent),
    /// The immutable event root is exhausted.
    Done,
}

/// Fixed-storage cursor over a persistent lexical event tree.
///
/// The pending tree path is bounded by the machine word width because the
/// lexer forest has at most [`MAX_PAGE_TREE_LEVELS`] levels. No operation in
/// this cursor allocates after construction.
#[derive(Debug)]
pub struct MeteredLexicalCursor {
    pending: [Option<Arc<PageTree>>; MAX_PAGE_TREE_LEVELS],
    pending_len: usize,
    current: Option<PageDecoder>,
    metrics: LexicalCursorMetrics,
}

impl MeteredLexicalCursor {
    fn new(root: Option<Arc<PageTree>>) -> Self {
        let root_clones = usize::from(root.is_some());
        let mut pending = std::array::from_fn(|_| None);
        let pending_len = if let Some(root) = root {
            pending[0] = Some(root);
            1
        } else {
            0
        };
        Self {
            pending,
            pending_len,
            current: None,
            metrics: LexicalCursorMetrics {
                root_clones,
                ..LexicalCursorMetrics::default()
            },
        }
    }

    /// Performs at most one tree-node visit, page transition, or record
    /// decode. A decoded lexical record is at most 12 bytes (one header, one
    /// escaped byte, and one `usize` varint), independent of document size.
    ///
    /// # Panics
    ///
    /// Panics only if an immutable lexical page violates the shared lexer's
    /// encoding invariant or the fixed tree path exceeds the word-sized lexer
    /// forest bound.
    #[must_use]
    pub fn step(&mut self) -> LexicalCursorStep {
        if let Some(current) = &mut self.current {
            if current.remaining == 0 {
                self.current = None;
                return LexicalCursorStep::Progress;
            }
            let before = current.position;
            let event = current
                .next()
                .expect("validated immutable lexical page decodes completely");
            self.metrics.events += 1;
            self.metrics.decoded_bytes += current.position - before;
            return LexicalCursorStep::Event(event);
        }

        if self.pending_len == 0 {
            return LexicalCursorStep::Done;
        }
        self.pending_len -= 1;
        let node = self.pending[self.pending_len]
            .take()
            .expect("occupied fixed lexical traversal slot");
        self.metrics.tree_nodes += 1;
        match node.as_ref() {
            PageTree::Leaf(page) => {
                self.current = Some(PageDecoder::new(page.clone()));
                self.metrics.pages_entered += 1;
            }
            PageTree::Branch { left, right, .. } => {
                assert!(self.pending_len + 2 <= MAX_PAGE_TREE_LEVELS);
                self.pending[self.pending_len] = Some(right.clone());
                self.pending[self.pending_len + 1] = Some(left.clone());
                self.pending_len += 2;
            }
        }
        LexicalCursorStep::Progress
    }

    /// Exact cumulative cursor counters.
    #[must_use]
    pub const fn metrics(&self) -> LexicalCursorMetrics {
        self.metrics
    }
}

impl LexicalEvents {
    fn new(root: Option<Arc<PageTree>>) -> Self {
        let mut pending = Vec::new();
        if let Some(root) = root {
            pending.push(root);
        }
        Self {
            pending,
            current: None,
        }
    }
}

impl Iterator for LexicalEvents {
    type Item = LexicalEvent;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = &mut self.current {
                if let Some(event) = current.next() {
                    return Some(event);
                }
                self.current = None;
            }
            let node = self.pending.pop()?;
            match node.as_ref() {
                PageTree::Leaf(page) => self.current = Some(PageDecoder::new(page.clone())),
                PageTree::Branch { left, right, .. } => {
                    self.pending.push(right.clone());
                    self.pending.push(left.clone());
                }
            }
        }
    }
}

const TAG_ESCAPE: u8 = 0;
const TAG_BACKTICK: u8 = 1;
const TAG_STAR: u8 = 2;
const TAG_UNDERSCORE: u8 = 3;
const TAG_OPEN_BRACKET: u8 = 4;
const TAG_CLOSE_BRACKET: u8 = 5;
const TAG_PIPE: u8 = 6;
const EXTENDED_GAP: usize = 31;

#[derive(Debug)]
struct PageDecoder {
    page: Arc<LexicalEventPage>,
    position: usize,
    remaining: usize,
    previous_end: usize,
}

impl PageDecoder {
    fn new(page: Arc<LexicalEventPage>) -> Self {
        Self {
            position: 0,
            remaining: page.events,
            previous_end: page.preceding_end,
            page,
        }
    }

    fn next(&mut self) -> Option<LexicalEvent> {
        if self.remaining == 0 {
            return None;
        }
        let header = *self.page.encoded.get(self.position)?;
        self.position += 1;
        let tag = header & 0b111;
        let inline_gap = usize::from(header >> 3);
        let gap = if inline_gap == EXTENDED_GAP {
            decode_varint(&self.page.encoded, &mut self.position)?
        } else {
            inline_gap
        };
        let start = self.previous_end.checked_add(gap)?;
        let (kind, len) = match tag {
            TAG_ESCAPE => {
                let escaped = *self.page.encoded.get(self.position)?;
                self.position += 1;
                (LexicalEventKind::BackslashEscape { escaped }, 2)
            }
            TAG_BACKTICK => {
                let run_len = decode_varint(&self.page.encoded, &mut self.position)?;
                (LexicalEventKind::BacktickRun { len: run_len }, run_len)
            }
            TAG_STAR | TAG_UNDERSCORE => {
                let run_len = decode_varint(&self.page.encoded, &mut self.position)?;
                (
                    LexicalEventKind::EmphasisRun {
                        marker: if tag == TAG_STAR { b'*' } else { b'_' },
                        len: run_len,
                    },
                    run_len,
                )
            }
            TAG_OPEN_BRACKET => (LexicalEventKind::OpenBracket, 1),
            TAG_CLOSE_BRACKET => (LexicalEventKind::CloseBracket, 1),
            TAG_PIPE => (LexicalEventKind::TablePipe, 1),
            _ => return None,
        };
        let end = start.checked_add(len)?;
        self.previous_end = end;
        self.remaining -= 1;
        Some(LexicalEvent {
            kind,
            start: LogicalPoint { offset: start },
            end,
        })
    }
}

fn encode_varint(mut value: usize, encoded: &mut Vec<u8>) {
    loop {
        let byte = u8::try_from(value & 0x7f).expect("seven-bit mask fits u8");
        value >>= 7;
        if value == 0 {
            encoded.push(byte);
            return;
        }
        encoded.push(byte | 0x80);
    }
}

fn decode_varint(encoded: &[u8], position: &mut usize) -> Option<usize> {
    let mut value = 0usize;
    for shift in (0..usize::BITS).step_by(7) {
        let byte = *encoded.get(*position)?;
        *position += 1;
        value |= usize::from(byte & 0x7f).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// Table-side consumer of the shared lexical root.
#[derive(Clone, Debug)]
pub struct TableLexicalInput {
    view: LexicalView,
}

/// Inline-side consumer of the same shared lexical root.
#[derive(Clone, Debug)]
pub struct InlineLexicalInput {
    view: LexicalView,
}

/// Both grammar consumers, created atomically from one sealed lexical root.
#[derive(Clone, Debug)]
pub struct LexicalConsumers {
    pub table: TableLexicalInput,
    pub inline: InlineLexicalInput,
}

/// Audit receipt proving consumers inspect events rather than source bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConsumerReceipt {
    pub lexical_events_examined: usize,
    pub source_bytes_examined: usize,
}

/// Table-pipe result from the shared event pages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TablePipeSummary {
    pub logical_offsets: Vec<usize>,
    pub receipt: ConsumerReceipt,
}

impl TableLexicalInput {
    /// Classifies unescaped table-pipe candidates without re-reading source.
    #[must_use]
    pub fn classify(&self) -> TablePipeSummary {
        let mut summary = TablePipeSummary::default();
        for event in self.view.events() {
            summary.receipt.lexical_events_examined += 1;
            if event.kind == LexicalEventKind::TablePipe {
                summary.logical_offsets.push(event.start.offset);
            }
        }
        summary
    }

    /// Immutable event view used by table recognition.
    #[must_use]
    pub const fn view(&self) -> &LexicalView {
        &self.view
    }
}

impl InlineLexicalInput {
    /// Iterates the same event pages used by table classification.
    #[must_use]
    pub fn events(&self) -> LexicalEvents {
        self.view.events()
    }

    /// Creates a fixed-storage, transition-metered cursor over the shared
    /// lexical root.
    #[must_use]
    pub fn metered_cursor(&self) -> MeteredLexicalCursor {
        self.view.metered_cursor()
    }

    /// Records event-only inspection for a later inline resolver.
    #[must_use]
    pub fn audit(&self) -> ConsumerReceipt {
        ConsumerReceipt {
            lexical_events_examined: self.events().count(),
            source_bytes_examined: 0,
        }
    }

    /// Immutable event view used by inline resolution.
    #[must_use]
    pub const fn view(&self) -> &LexicalView {
        &self.view
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunKind {
    Backtick,
    Emphasis(u8),
}

#[derive(Clone, Copy, Debug)]
struct OpenRun {
    kind: RunKind,
    start: LogicalPoint,
    len: usize,
}

impl OpenRun {
    fn accepts(self, byte: LogicalByte) -> bool {
        self.start.offset + self.len == byte.logical_offset
            && match self.kind {
                RunKind::Backtick => byte.byte == b'`',
                RunKind::Emphasis(marker) => byte.byte == marker,
            }
    }

    fn event(self) -> LexicalEvent {
        let kind = match self.kind {
            RunKind::Backtick => LexicalEventKind::BacktickRun { len: self.len },
            RunKind::Emphasis(marker) => LexicalEventKind::EmphasisRun {
                marker,
                len: self.len,
            },
        };
        LexicalEvent {
            kind,
            start: self.start,
            end: self.start.offset + self.len,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResumePhase {
    Read,
    Process,
    EofFlush,
    EofCombine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LexerPhase {
    Read,
    Process,
    SealPage(ResumePhase),
    AppendPage(ResumePhase),
    EofFlush,
    EofCombine,
    Done,
}

/// Status of one bounded lexer poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexerStatus {
    Pending,
    Ready,
}

/// Work performed by one lexer poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LexerPoll {
    pub status: LexerStatus,
    pub work: usize,
}

/// One fuelled lexical job. Allocation and page-tree composition are phases.
#[derive(Debug)]
pub struct SharedLexer {
    input: SegmentedLeaf,
    cursor: LogicalCursor,
    phase: LexerPhase,
    pending_byte: Option<LogicalByte>,
    pending_escape: Option<LogicalPoint>,
    run: Option<OpenRun>,
    page: Vec<u8>,
    page_events: usize,
    page_preceding_end: usize,
    previous_event_end: usize,
    forest: [Option<Arc<PageTree>>; MAX_PAGE_TREE_LEVELS],
    carry: Option<Arc<PageTree>>,
    carry_level: usize,
    finalize_level: isize,
    final_root: Option<Arc<PageTree>>,
    result: Option<LexicalView>,
    total_work: usize,
    max_poll_work: usize,
}

impl SharedLexer {
    /// Starts the only lexical pass over one logical leaf.
    #[must_use]
    pub fn new(leaf: &SegmentedLeaf) -> Self {
        Self {
            cursor: leaf.cursor(),
            input: leaf.clone(),
            phase: LexerPhase::Read,
            pending_byte: None,
            pending_escape: None,
            run: None,
            page: Vec::with_capacity(EVENTS_PER_PAGE * 2),
            page_events: 0,
            page_preceding_end: 0,
            previous_event_end: 0,
            forest: std::array::from_fn(|_| None),
            carry: None,
            carry_level: 0,
            finalize_level: MAX_PAGE_TREE_LEVELS.cast_signed() - 1,
            final_root: None,
            result: None,
            total_work: 0,
            max_poll_work: 0,
        }
    }

    /// Advances at most `fuel` cursor, event, seal, or persistent-tree transitions.
    ///
    /// # Panics
    ///
    /// Panics if `fuel` is zero or exceeds [`MAX_LEXER_POLL_WORK`].
    pub fn poll(&mut self, fuel: usize) -> LexerPoll {
        assert!(fuel > 0 && fuel <= MAX_LEXER_POLL_WORK);
        let mut work = 0;
        while work < fuel && self.phase != LexerPhase::Done {
            self.tick();
            work += 1;
        }
        self.total_work += work;
        self.max_poll_work = self.max_poll_work.max(work);
        LexerPoll {
            status: if self.phase == LexerPhase::Done {
                LexerStatus::Ready
            } else {
                LexerStatus::Pending
            },
            work,
        }
    }

    fn tick(&mut self) {
        match self.phase {
            LexerPhase::Read => match self.cursor.step() {
                CursorStep::Byte(byte) => {
                    self.pending_byte = Some(byte);
                    self.phase = LexerPhase::Process;
                }
                CursorStep::Progress => {}
                CursorStep::Done => self.phase = LexerPhase::EofFlush,
            },
            LexerPhase::Process => self.process_pending(),
            LexerPhase::SealPage(resume) => self.seal_page(resume),
            LexerPhase::AppendPage(resume) => self.append_page(resume),
            LexerPhase::EofFlush => self.flush_eof(),
            LexerPhase::EofCombine => self.combine_final_root(),
            LexerPhase::Done => {}
        }
    }

    fn process_pending(&mut self) {
        let byte = self.pending_byte.expect("process phase has a byte");
        if let Some(mut run) = self.run {
            if run.accepts(byte) {
                run.len += 1;
                self.run = Some(run);
                self.pending_byte = None;
                self.phase = LexerPhase::Read;
                return;
            }
            self.run = None;
            self.emit(run.event(), ResumePhase::Process);
            return;
        }

        if let Some(backslash) = self.pending_escape.take() {
            if byte.byte.is_ascii_punctuation() {
                self.pending_byte = None;
                self.emit(
                    LexicalEvent {
                        kind: LexicalEventKind::BackslashEscape { escaped: byte.byte },
                        start: backslash,
                        end: byte.logical_offset + 1,
                    },
                    ResumePhase::Read,
                );
            }
            // A non-punctuation byte remains pending and is processed normally
            // on the next fuelled transition.
            return;
        }

        let point = LogicalPoint::from(byte);
        match byte.byte {
            b'\\' => {
                self.pending_escape = Some(point);
                self.pending_byte = None;
                self.phase = LexerPhase::Read;
            }
            b'`' => {
                self.run = Some(OpenRun {
                    kind: RunKind::Backtick,
                    start: point,
                    len: 1,
                });
                self.pending_byte = None;
                self.phase = LexerPhase::Read;
            }
            marker @ (b'*' | b'_') => {
                self.run = Some(OpenRun {
                    kind: RunKind::Emphasis(marker),
                    start: point,
                    len: 1,
                });
                self.pending_byte = None;
                self.phase = LexerPhase::Read;
            }
            b'[' | b']' | b'|' => {
                let kind = match byte.byte {
                    b'[' => LexicalEventKind::OpenBracket,
                    b']' => LexicalEventKind::CloseBracket,
                    b'|' => LexicalEventKind::TablePipe,
                    _ => unreachable!(),
                };
                self.pending_byte = None;
                self.emit(
                    LexicalEvent {
                        kind,
                        start: point,
                        end: byte.logical_offset + 1,
                    },
                    ResumePhase::Read,
                );
            }
            _ => {
                self.pending_byte = None;
                self.phase = LexerPhase::Read;
            }
        }
    }

    fn emit(&mut self, event: LexicalEvent, resume: ResumePhase) {
        if self.page_events == 0 {
            self.page_preceding_end = self.previous_event_end;
        }
        let gap = event
            .start
            .offset
            .checked_sub(self.previous_event_end)
            .expect("lexical events are emitted in non-overlapping source order");
        let tag = match event.kind {
            LexicalEventKind::BackslashEscape { .. } => TAG_ESCAPE,
            LexicalEventKind::BacktickRun { .. } => TAG_BACKTICK,
            LexicalEventKind::EmphasisRun { marker: b'*', .. } => TAG_STAR,
            LexicalEventKind::EmphasisRun { marker: b'_', .. } => TAG_UNDERSCORE,
            LexicalEventKind::EmphasisRun { marker, .. } => {
                unreachable!("unsupported emphasis marker {marker}")
            }
            LexicalEventKind::OpenBracket => TAG_OPEN_BRACKET,
            LexicalEventKind::CloseBracket => TAG_CLOSE_BRACKET,
            LexicalEventKind::TablePipe => TAG_PIPE,
        };
        let inline_gap = gap.min(EXTENDED_GAP);
        let gap_bits = u8::try_from(inline_gap).expect("inline gap is at most 31");
        self.page.push((gap_bits << 3) | tag);
        if gap >= EXTENDED_GAP {
            encode_varint(gap, &mut self.page);
        }
        match event.kind {
            LexicalEventKind::BackslashEscape { escaped } => self.page.push(escaped),
            LexicalEventKind::BacktickRun { len } | LexicalEventKind::EmphasisRun { len, .. } => {
                encode_varint(len, &mut self.page);
            }
            LexicalEventKind::OpenBracket
            | LexicalEventKind::CloseBracket
            | LexicalEventKind::TablePipe => {}
        }
        self.page_events += 1;
        self.previous_event_end = event.end;
        self.phase = if self.page_events == EVENTS_PER_PAGE {
            LexerPhase::SealPage(resume)
        } else {
            resume.into()
        };
    }

    fn seal_page(&mut self, resume: ResumePhase) {
        let encoded = std::mem::replace(&mut self.page, Vec::with_capacity(EVENTS_PER_PAGE * 2));
        let events = std::mem::take(&mut self.page_events);
        debug_assert!(events > 0);
        let page = Arc::new(LexicalEventPage {
            encoded: encoded.into_boxed_slice(),
            events,
            preceding_end: self.page_preceding_end,
        });
        self.carry = Some(Arc::new(PageTree::Leaf(page)));
        self.carry_level = 0;
        self.phase = LexerPhase::AppendPage(resume);
    }

    fn append_page(&mut self, resume: ResumePhase) {
        let carry = self.carry.take().expect("append phase has a carry root");
        if let Some(left) = self.forest[self.carry_level].take() {
            self.carry = Some(PageTree::concat(left, carry));
            self.carry_level += 1;
            assert!(self.carry_level < MAX_PAGE_TREE_LEVELS);
            return;
        }
        self.forest[self.carry_level] = Some(carry);
        self.phase = resume.into();
    }

    fn flush_eof(&mut self) {
        self.pending_escape = None;
        if let Some(run) = self.run.take() {
            self.emit(run.event(), ResumePhase::EofFlush);
            return;
        }
        if self.page_events > 0 {
            self.phase = LexerPhase::SealPage(ResumePhase::EofCombine);
            return;
        }
        self.phase = LexerPhase::EofCombine;
    }

    fn combine_final_root(&mut self) {
        while self.finalize_level >= 0 {
            let level = self.finalize_level.cast_unsigned();
            self.finalize_level -= 1;
            if let Some(root) = self.forest[level].take() {
                self.final_root = Some(match self.final_root.take() {
                    None => root,
                    Some(left) => PageTree::concat(left, root),
                });
                return;
            }
        }
        self.result = Some(LexicalView {
            root: self.final_root.take(),
            input: Some(self.input.clone()),
        });
        self.phase = LexerPhase::Done;
    }

    /// Sealed lexical consumers, available only after a ready poll.
    #[must_use]
    pub fn consumers(&self) -> Option<LexicalConsumers> {
        let view = self.result.clone()?;
        Some(LexicalConsumers {
            table: TableLexicalInput { view: view.clone() },
            inline: InlineLexicalInput { view },
        })
    }

    /// Cursor work, including excluded source bytes and descriptor transitions.
    #[must_use]
    pub const fn cursor_metrics(&self) -> CursorMetrics {
        self.cursor.metrics()
    }

    /// Total charged lexer work.
    #[must_use]
    pub const fn total_work(&self) -> usize {
        self.total_work
    }

    /// Largest charged poll.
    #[must_use]
    pub const fn max_poll_work(&self) -> usize {
        self.max_poll_work
    }
}

impl From<ResumePhase> for LexerPhase {
    fn from(value: ResumePhase) -> Self {
        match value {
            ResumePhase::Read => Self::Read,
            ResumePhase::Process => Self::Process,
            ResumePhase::EofFlush => Self::EofFlush,
            ResumePhase::EofCombine => Self::EofCombine,
        }
    }
}

/// Stable identity of one block-owned logical leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LeafId(pub u64);

/// Immutable base classification written when a leaf is sealed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseLeafKind {
    Paragraph,
}

/// Keyed metadata that may promote a previously sealed paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeafMetadataOverlay {
    Setext {
        level: u8,
    },
    Table {
        columns: usize,
        pipe_offsets: Arc<[usize]>,
    },
}

/// Immutable sealed leaf prefix record.
#[derive(Clone, Debug)]
pub struct SealedLeafOutput {
    pub id: LeafId,
    pub base_kind: BaseLeafKind,
    pub input: SegmentedLeaf,
}

#[derive(Debug)]
struct OpenLeafOutput {
    id: LeafId,
    base_kind: BaseLeafKind,
    input: SegmentedLeafBuilder,
}

/// Block-output frontier with immutable sealed records and keyed promotions.
#[derive(Debug, Default)]
pub struct LeafOutputFrontier {
    sealed: Vec<Arc<SealedLeafOutput>>,
    open: Option<OpenLeafOutput>,
    overlays: BTreeMap<LeafId, LeafMetadataOverlay>,
}

impl LeafOutputFrontier {
    /// Opens one paragraph output without mutating already sealed records.
    ///
    /// # Errors
    ///
    /// Returns [`FrontierError::LeafAlreadyOpen`] if a leaf is already open.
    pub fn begin_leaf(
        &mut self,
        id: LeafId,
        base_kind: BaseLeafKind,
        source: Arc<PersistentSource>,
    ) -> Result<(), FrontierError> {
        if self.open.is_some() {
            return Err(FrontierError::LeafAlreadyOpen);
        }
        if self.sealed.iter().any(|leaf| leaf.id == id) {
            return Err(FrontierError::DuplicateLeaf(id));
        }
        self.open = Some(OpenLeafOutput {
            id,
            base_kind,
            input: SegmentedLeafBuilder::new(source),
        });
        Ok(())
    }

    /// Mutable descriptor builder for the currently open leaf only.
    ///
    /// # Errors
    ///
    /// Returns [`FrontierError::NoOpenLeaf`] when there is no open leaf.
    pub fn open_input(&mut self) -> Result<&mut SegmentedLeafBuilder, FrontierError> {
        self.open
            .as_mut()
            .map(|open| &mut open.input)
            .ok_or(FrontierError::NoOpenLeaf)
    }

    /// Seals the open leaf into an immutable prefix record.
    ///
    /// # Errors
    ///
    /// Returns [`FrontierError::NoOpenLeaf`] when there is no open leaf.
    pub fn seal_open(&mut self) -> Result<Arc<SealedLeafOutput>, FrontierError> {
        let open = self.open.take().ok_or(FrontierError::NoOpenLeaf)?;
        let sealed = Arc::new(SealedLeafOutput {
            id: open.id,
            base_kind: open.base_kind,
            input: open.input.finish(),
        });
        self.sealed.push(sealed.clone());
        Ok(sealed)
    }

    /// Promotes a leaf through a keyed setext overlay.
    ///
    /// # Errors
    ///
    /// Returns [`FrontierError::UnknownLeaf`] if `id` is not open or sealed.
    pub fn promote_setext(&mut self, id: LeafId, level: u8) -> Result<(), FrontierError> {
        self.require_leaf(id)?;
        self.overlays
            .insert(id, LeafMetadataOverlay::Setext { level });
        Ok(())
    }

    /// Promotes a leaf through table metadata derived from the shared lexer.
    ///
    /// # Errors
    ///
    /// Returns [`FrontierError::UnknownLeaf`] if `id` is not open or sealed.
    pub fn promote_table(
        &mut self,
        id: LeafId,
        pipes: &TablePipeSummary,
    ) -> Result<(), FrontierError> {
        self.require_leaf(id)?;
        self.overlays.insert(
            id,
            LeafMetadataOverlay::Table {
                columns: pipes.logical_offsets.len() + 1,
                pipe_offsets: Arc::from(pipes.logical_offsets.clone()),
            },
        );
        Ok(())
    }

    fn require_leaf(&self, id: LeafId) -> Result<(), FrontierError> {
        if self.open.as_ref().is_some_and(|open| open.id == id)
            || self.sealed.iter().any(|leaf| leaf.id == id)
        {
            Ok(())
        } else {
            Err(FrontierError::UnknownLeaf(id))
        }
    }

    /// Immutable sealed record. Later promotions preserve its pointer identity.
    #[must_use]
    pub fn sealed_leaf(&self, id: LeafId) -> Option<Arc<SealedLeafOutput>> {
        self.sealed.iter().find(|leaf| leaf.id == id).cloned()
    }

    /// Current keyed promotion, if any.
    #[must_use]
    pub fn overlay(&self, id: LeafId) -> Option<&LeafMetadataOverlay> {
        self.overlays.get(&id)
    }

    /// Number of immutable prefix records.
    #[must_use]
    pub fn sealed_len(&self) -> usize {
        self.sealed.len()
    }
}

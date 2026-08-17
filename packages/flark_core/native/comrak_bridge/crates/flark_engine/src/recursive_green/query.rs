//! Exact source-coordinate lookup over packed recursive Green events.

use std::ops::Range;

use crate::document::DocumentRuntime;
use crate::measured_sequence::{
    MeasuredSequenceRef, SequenceInspectionReceipt, SequenceLeafVisitControl, SequenceNodeCache,
    SequenceSpecInspection, SequenceSummaryPartitionDirection,
};
use crate::parser_pages::{M11ParserPageError, M11ParserSourceRangeAuthority};
use crate::source::{SourceBoundaryAffinity, SourceVersion};

use super::build::{
    M11RecursiveGreenRoot, M11RecursiveGreenSliceOpenFrameBase, M11RecursiveGreenSliceRoot,
};
use super::codec::{
    decode_leaf, decode_packed_event, is_renderable_row_kind,
    M11RecursiveGreenCachedRowEditCapability, M11RecursiveGreenCoveragePart,
    M11RecursiveGreenError, M11RecursiveGreenFrameId, M11RecursiveGreenKind,
    M11RecursiveGreenLogicalAtom, M11RecursiveGreenSourceMetric, PackedGreenEvent,
    RecursiveGreenSpec, RecursiveGreenSummary, EMPTY_BLOCK_QUOTE_ROW_KIND, EMPTY_ITEM_ROW_KIND,
};

const fn empty_container_parent_kind(row_kind: u16) -> Option<u16> {
    match row_kind {
        EMPTY_ITEM_ROW_KIND => Some(4),
        EMPTY_BLOCK_QUOTE_ROW_KIND => Some(2),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenPoint {
    byte_offset: usize,
    utf16_offset: usize,
    affinity: SourceBoundaryAffinity,
}

impl M11RecursiveGreenPoint {
    #[must_use]
    pub const fn new(
        byte_offset: usize,
        utf16_offset: usize,
        affinity: SourceBoundaryAffinity,
    ) -> Self {
        Self {
            byte_offset,
            utf16_offset,
            affinity,
        }
    }

    #[must_use]
    pub const fn byte_offset(self) -> usize {
        self.byte_offset
    }
    #[must_use]
    pub const fn utf16_offset(self) -> usize {
        self.utf16_offset
    }
    #[must_use]
    pub const fn affinity(self) -> SourceBoundaryAffinity {
        self.affinity
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenAncestor {
    frame: M11RecursiveGreenFrameId,
    kind: M11RecursiveGreenKind,
}

impl M11RecursiveGreenAncestor {
    #[must_use]
    pub const fn frame(self) -> M11RecursiveGreenFrameId {
        self.frame
    }
    #[must_use]
    pub const fn kind(self) -> M11RecursiveGreenKind {
        self.kind
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11RecursiveGreenQueryReceipt {
    node_headers_decoded: u64,
    summary_combinations: u64,
    payload_bytes_inspected: u64,
    events_authenticated: u64,
    storage_pages_visited: u64,
    events_scanned: u64,
    maximum_open_depth: usize,
}

impl M11RecursiveGreenQueryReceipt {
    #[must_use]
    pub const fn node_headers_decoded(self) -> u64 {
        self.node_headers_decoded
    }
    #[must_use]
    pub const fn summary_combinations(self) -> u64 {
        self.summary_combinations
    }
    #[must_use]
    pub const fn payload_bytes_inspected(self) -> u64 {
        self.payload_bytes_inspected
    }
    #[must_use]
    pub const fn events_authenticated(self) -> u64 {
        self.events_authenticated
    }
    #[must_use]
    pub const fn storage_pages_visited(self) -> u64 {
        self.storage_pages_visited
    }
    #[must_use]
    pub const fn events_scanned(self) -> u64 {
        self.events_scanned
    }
    #[must_use]
    pub const fn maximum_open_depth(self) -> usize {
        self.maximum_open_depth
    }
}

/// Explicit work admission for one frame-range query.
///
/// The current query walks authenticated event pages from the beginning of the
/// root until the selected frame closes. These limits make that first
/// production implementation honest and caller-bounded while the measured
/// Green summary is extended with a direct ancestry zipper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenFrameQueryLimits {
    maximum_storage_pages_visited: u64,
    maximum_events_scanned: u64,
    maximum_open_depth: usize,
    maximum_inline_source_bytes: u64,
}

impl M11RecursiveGreenFrameQueryLimits {
    #[must_use]
    pub const fn new(
        maximum_storage_pages_visited: u64,
        maximum_events_scanned: u64,
        maximum_open_depth: usize,
        maximum_inline_source_bytes: u64,
    ) -> Option<Self> {
        if maximum_storage_pages_visited == 0
            || maximum_events_scanned == 0
            || maximum_open_depth == 0
            || maximum_inline_source_bytes == 0
        {
            return None;
        }
        Some(Self {
            maximum_storage_pages_visited,
            maximum_events_scanned,
            maximum_open_depth,
            maximum_inline_source_bytes,
        })
    }

    #[must_use]
    pub const fn maximum_storage_pages_visited(self) -> u64 {
        self.maximum_storage_pages_visited
    }

    #[must_use]
    pub const fn maximum_events_scanned(self) -> u64 {
        self.maximum_events_scanned
    }

    #[must_use]
    pub const fn maximum_open_depth(self) -> usize {
        self.maximum_open_depth
    }

    #[must_use]
    pub const fn maximum_inline_source_bytes(self) -> u64 {
        self.maximum_inline_source_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenFrameQueryBound {
    StoragePagesVisited,
    EventsScanned,
    OpenDepth,
    TreeNodesVisited,
    InlineSourceBytes,
}

#[derive(Debug)]
pub enum M11RecursiveGreenFrameQueryError {
    BoundExceeded(M11RecursiveGreenFrameQueryBound),
    Green(M11RecursiveGreenError),
    SourceAuthority(M11ParserPageError),
}

impl std::fmt::Display for M11RecursiveGreenFrameQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BoundExceeded(bound) => {
                write!(formatter, "recursive-green frame query exceeded {bound:?}")
            }
            Self::Green(error) => error.fmt(formatter),
            Self::SourceAuthority(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for M11RecursiveGreenFrameQueryError {}

impl From<M11RecursiveGreenError> for M11RecursiveGreenFrameQueryError {
    fn from(error: M11RecursiveGreenError) -> Self {
        Self::Green(error)
    }
}

impl From<M11ParserPageError> for M11RecursiveGreenFrameQueryError {
    fn from(error: M11ParserPageError) -> Self {
        Self::SourceAuthority(error)
    }
}

/// Move-only authority for one parser-selected recursive-Green frame.
///
/// `block_source_range` is the exact physical frame envelope from `Enter` to
/// `Exit`. `inline_source_range` is the exact contiguous source-backed logical
/// content admitted for inline parsing. A projected or physically disjoint
/// logical leaf is rejected rather than widened into a caller-controlled
/// range.
#[must_use = "recursive-green frame fences must be consumed by parser work or deliberately dropped"]
pub struct M11RecursiveGreenFrameFence {
    source: SourceVersion,
    frame: M11RecursiveGreenFrameId,
    kind: M11RecursiveGreenKind,
    block_source: Range<u64>,
    block_source_utf16: Range<u64>,
    inline_source: Range<u64>,
    inline_source_utf16: Range<u64>,
    receipt: M11RecursiveGreenQueryReceipt,
    authority: M11ParserSourceRangeAuthority,
}

impl std::fmt::Debug for M11RecursiveGreenFrameFence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("M11RecursiveGreenFrameFence")
            .field("source", &self.source)
            .field("frame", &self.frame)
            .field("kind", &self.kind)
            .field("block_source", &self.block_source)
            .field("block_source_utf16", &self.block_source_utf16)
            .field("inline_source", &self.inline_source)
            .field("inline_source_utf16", &self.inline_source_utf16)
            .field("receipt", &self.receipt)
            .finish_non_exhaustive()
    }
}

impl M11RecursiveGreenFrameFence {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn frame(&self) -> M11RecursiveGreenFrameId {
        self.frame
    }

    #[must_use]
    pub const fn kind(&self) -> M11RecursiveGreenKind {
        self.kind
    }

    #[must_use]
    pub fn block_source_range(&self) -> Range<u64> {
        self.block_source.clone()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> Range<u64> {
        self.block_source_utf16.clone()
    }

    #[must_use]
    pub fn inline_source_range(&self) -> Range<u64> {
        self.inline_source.clone()
    }

    #[must_use]
    pub fn inline_source_utf16_range(&self) -> Range<u64> {
        self.inline_source_utf16.clone()
    }

    #[must_use]
    pub const fn receipt(&self) -> M11RecursiveGreenQueryReceipt {
        self.receipt
    }

    #[doc(hidden)]
    pub fn into_inline_authority(self) -> (M11ParserSourceRangeAuthority, Range<u64>) {
        (self.authority, self.inline_source)
    }
}

/// Move-only authority for one physically disjoint, parser-projected frame.
///
/// Unlike [`M11RecursiveGreenFrameFence`], this fence does not claim a
/// contiguous inline range. The projected metrics are independently folded
/// from authenticated Green coverage, while the authority covers the exact
/// physical container envelope consumed by later projection work.
#[must_use = "recursive-green projected-frame fences must be consumed by parser work or deliberately dropped"]
pub struct M11RecursiveGreenProjectedFrameFence {
    source: SourceVersion,
    frame: M11RecursiveGreenFrameId,
    kind: M11RecursiveGreenKind,
    block_source: Range<u64>,
    block_source_utf16: Range<u64>,
    line_count: u64,
    projected_source: M11RecursiveGreenSourceMetric,
    receipt: M11RecursiveGreenQueryReceipt,
    authority: M11ParserSourceRangeAuthority,
}

impl std::fmt::Debug for M11RecursiveGreenProjectedFrameFence {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("M11RecursiveGreenProjectedFrameFence")
            .field("source", &self.source)
            .field("frame", &self.frame)
            .field("kind", &self.kind)
            .field("block_source", &self.block_source)
            .field("block_source_utf16", &self.block_source_utf16)
            .field("line_count", &self.line_count)
            .field("projected_source", &self.projected_source)
            .field("receipt", &self.receipt)
            .finish_non_exhaustive()
    }
}

impl M11RecursiveGreenProjectedFrameFence {
    #[must_use]
    pub const fn source(&self) -> SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn frame(&self) -> M11RecursiveGreenFrameId {
        self.frame
    }

    #[must_use]
    pub const fn kind(&self) -> M11RecursiveGreenKind {
        self.kind
    }

    #[must_use]
    pub fn block_source_range(&self) -> Range<u64> {
        self.block_source.clone()
    }

    #[must_use]
    pub fn block_source_utf16_range(&self) -> Range<u64> {
        self.block_source_utf16.clone()
    }

    #[must_use]
    pub const fn line_count(&self) -> u64 {
        self.line_count
    }

    #[must_use]
    pub const fn projected_source_metric(&self) -> M11RecursiveGreenSourceMetric {
        self.projected_source
    }

    #[must_use]
    pub const fn receipt(&self) -> M11RecursiveGreenQueryReceipt {
        self.receipt
    }

    #[doc(hidden)]
    pub fn into_projected_authority(self) -> (M11ParserSourceRangeAuthority, Range<u64>) {
        (self.authority, self.block_source)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenLocation {
    byte_range: Range<u64>,
    utf16_range: Range<u64>,
    physical: M11RecursiveGreenSourceMetric,
    logical: M11RecursiveGreenSourceMetric,
    part: M11RecursiveGreenCoveragePart,
    atom: M11RecursiveGreenLogicalAtom,
    owner_index: usize,
    ancestry: Vec<M11RecursiveGreenAncestor>,
    receipt: M11RecursiveGreenQueryReceipt,
    zipper_open: Vec<PointZipperOpenFrame>,
    renderable_rows_before: u64,
}

impl M11RecursiveGreenLocation {
    #[must_use]
    pub fn byte_range(&self) -> Range<u64> {
        self.byte_range.clone()
    }
    #[must_use]
    pub fn utf16_range(&self) -> Range<u64> {
        self.utf16_range.clone()
    }
    #[must_use]
    pub const fn physical_metric(&self) -> M11RecursiveGreenSourceMetric {
        self.physical
    }
    #[must_use]
    pub const fn logical_metric(&self) -> M11RecursiveGreenSourceMetric {
        self.logical
    }
    #[must_use]
    pub const fn part(&self) -> M11RecursiveGreenCoveragePart {
        self.part
    }
    #[must_use]
    pub const fn logical_atom(&self) -> M11RecursiveGreenLogicalAtom {
        self.atom
    }
    #[must_use]
    pub const fn owner_index(&self) -> usize {
        self.owner_index
    }
    #[must_use]
    pub fn owner(&self) -> M11RecursiveGreenAncestor {
        self.ancestry[self.owner_index]
    }
    #[must_use]
    pub fn ancestry(&self) -> &[M11RecursiveGreenAncestor] {
        &self.ancestry
    }
    #[must_use]
    pub const fn receipt(&self) -> M11RecursiveGreenQueryReceipt {
        self.receipt
    }
}

/// The result of one point lookup under an exact measured-tree node budget.
///
/// `NotFound` is intentionally distinct from a missing persistent Green role
/// at the host boundary. `BudgetExceeded` never carries a partially resolved
/// location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenPointQueryOutcome {
    Location(M11RecursiveGreenLocation),
    NotFound,
    BudgetExceeded(M11RecursiveGreenPointBudgetExceeded),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenPointBudgetExceeded {
    receipt: M11RecursiveGreenQueryReceipt,
}

impl M11RecursiveGreenPointBudgetExceeded {
    #[must_use]
    pub const fn receipt(self) -> M11RecursiveGreenQueryReceipt {
        self.receipt
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenRowQueryLimits {
    maximum_rows: u32,
    maximum_storage_pages_visited: u64,
    maximum_events_scanned: u64,
    maximum_open_depth: usize,
    maximum_tree_nodes_visited: u64,
}

impl M11RecursiveGreenRowQueryLimits {
    #[must_use]
    pub const fn new(
        maximum_rows: u32,
        maximum_storage_pages_visited: u64,
        maximum_events_scanned: u64,
        maximum_open_depth: usize,
        maximum_tree_nodes_visited: u64,
    ) -> Option<Self> {
        if maximum_rows == 0
            || maximum_storage_pages_visited == 0
            || maximum_events_scanned == 0
            || maximum_open_depth == 0
            || maximum_tree_nodes_visited == 0
        {
            return None;
        }
        Some(Self {
            maximum_rows,
            maximum_storage_pages_visited,
            maximum_events_scanned,
            maximum_open_depth,
            maximum_tree_nodes_visited,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenRowPathFrame {
    frame: M11RecursiveGreenFrameId,
    kind: M11RecursiveGreenKind,
    physical: Range<u64>,
    physical_utf16: Range<u64>,
    property: Option<super::codec::M11RecursiveGreenPropertyChunk>,
    close: Option<super::codec::M11RecursiveGreenCloseFacts>,
}

impl M11RecursiveGreenRowPathFrame {
    #[must_use]
    pub const fn frame(&self) -> M11RecursiveGreenFrameId {
        self.frame
    }
    #[must_use]
    pub const fn kind(&self) -> M11RecursiveGreenKind {
        self.kind
    }
    #[must_use]
    pub fn physical_range(&self) -> Range<u64> {
        self.physical.clone()
    }
    #[must_use]
    pub fn physical_utf16_range(&self) -> Range<u64> {
        self.physical_utf16.clone()
    }
    #[must_use]
    pub const fn property(&self) -> Option<super::codec::M11RecursiveGreenPropertyChunk> {
        self.property
    }
    #[must_use]
    pub const fn close(&self) -> Option<super::codec::M11RecursiveGreenCloseFacts> {
        self.close
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenRenderableRow {
    ordinal: u64,
    frame: M11RecursiveGreenFrameId,
    kind: M11RecursiveGreenKind,
    physical: Range<u64>,
    physical_utf16: Range<u64>,
    edit_capability: M11RecursiveGreenRowEditCapability,
    editable: Option<Range<u64>>,
    editable_utf16: Option<Range<u64>>,
    editable_segments: Vec<M11RecursiveGreenRowEditableSegment>,
    path: Vec<M11RecursiveGreenRowPathFrame>,
}

/// One exact identity-source segment in a parser-certified projected row.
///
/// Adjacent segments are separated only by `HiddenUpstream` coverage. The
/// ordered collection is therefore sufficient to paint and hit-test the row
/// without treating hidden container prefixes as editable display text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenRowEditableSegment {
    bytes: Range<u64>,
    utf16: Range<u64>,
}

impl M11RecursiveGreenRowEditableSegment {
    #[must_use]
    pub fn byte_range(&self) -> Range<u64> {
        self.bytes.clone()
    }

    #[must_use]
    pub fn utf16_range(&self) -> Range<u64> {
        self.utf16.clone()
    }
}

/// Whether one renderable row has an exact contiguous active-edit cut.
///
/// `ProjectedReserved` reserves the clean keyed-projection path without
/// claiming that such a payload exists today. Valid disjoint rows are
/// `Unavailable`: their structure and ancestry remain exact, while display
/// projection and editing fail closed for that row only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenRowEditCapability {
    Contiguous,
    ProjectedReserved,
    Unavailable,
}

impl M11RecursiveGreenRenderableRow {
    #[must_use]
    pub const fn ordinal(&self) -> u64 {
        self.ordinal
    }
    #[must_use]
    pub const fn frame(&self) -> M11RecursiveGreenFrameId {
        self.frame
    }
    #[must_use]
    pub const fn kind(&self) -> M11RecursiveGreenKind {
        self.kind
    }
    #[must_use]
    pub fn physical_range(&self) -> Range<u64> {
        self.physical.clone()
    }
    #[must_use]
    pub fn physical_utf16_range(&self) -> Range<u64> {
        self.physical_utf16.clone()
    }
    #[must_use]
    pub const fn edit_capability(&self) -> M11RecursiveGreenRowEditCapability {
        self.edit_capability
    }
    #[must_use]
    pub fn editable_range(&self) -> Option<Range<u64>> {
        self.editable.clone()
    }
    #[must_use]
    pub fn editable_utf16_range(&self) -> Option<Range<u64>> {
        self.editable_utf16.clone()
    }
    #[must_use]
    pub fn editable_segments(&self) -> &[M11RecursiveGreenRowEditableSegment] {
        &self.editable_segments
    }
    #[must_use]
    pub fn path(&self) -> &[M11RecursiveGreenRowPathFrame] {
        &self.path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenRowWindow {
    start_ordinal: u64,
    total_rows: u64,
    complete: bool,
    rows: Vec<M11RecursiveGreenRenderableRow>,
    receipt: M11RecursiveGreenQueryReceipt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenRowQueryLimit {
    StoragePages,
    EventsScanned,
    TreeNodes,
    OpenDepth,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenRowBudgetExceeded {
    limit: M11RecursiveGreenRowQueryLimit,
    receipt: M11RecursiveGreenQueryReceipt,
}

impl M11RecursiveGreenRowBudgetExceeded {
    #[must_use]
    pub const fn limit(self) -> M11RecursiveGreenRowQueryLimit {
        self.limit
    }

    #[must_use]
    pub const fn receipt(self) -> M11RecursiveGreenQueryReceipt {
        self.receipt
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenRowQueryOutcome {
    Window(M11RecursiveGreenRowWindow),
    BudgetExceeded(M11RecursiveGreenRowBudgetExceeded),
}

impl M11RecursiveGreenRowWindow {
    #[must_use]
    pub const fn start_ordinal(&self) -> u64 {
        self.start_ordinal
    }
    #[must_use]
    pub const fn total_rows(&self) -> u64 {
        self.total_rows
    }
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.complete
    }
    #[must_use]
    pub fn rows(&self) -> &[M11RecursiveGreenRenderableRow] {
        &self.rows
    }
    #[must_use]
    pub const fn receipt(&self) -> M11RecursiveGreenQueryReceipt {
        self.receipt
    }
}

/// Exact physical cuts for a half-open global renderable-row ordinal window.
///
/// The cuts are selected by the measured `renderable_row_exits` monoid, so
/// lookup work depends on tree height and bounded packed pages rather than on
/// the number of rows skipped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenRowOrdinalWindow {
    total_rows: u64,
    start_ordinal: u64,
    next_ordinal: u64,
    start_bytes: u64,
    start_utf16: u64,
    next_bytes: u64,
    next_utf16: u64,
    receipt: M11RecursiveGreenQueryReceipt,
}

impl M11RecursiveGreenRowOrdinalWindow {
    #[must_use]
    pub const fn total_rows(self) -> u64 {
        self.total_rows
    }
    #[must_use]
    pub const fn start_ordinal(self) -> u64 {
        self.start_ordinal
    }
    #[must_use]
    pub const fn next_ordinal(self) -> u64 {
        self.next_ordinal
    }
    #[must_use]
    pub const fn start_bytes(self) -> u64 {
        self.start_bytes
    }
    #[must_use]
    pub const fn start_utf16(self) -> u64 {
        self.start_utf16
    }
    #[must_use]
    pub const fn next_bytes(self) -> u64 {
        self.next_bytes
    }
    #[must_use]
    pub const fn next_utf16(self) -> u64 {
        self.next_utf16
    }
    #[must_use]
    pub const fn complete(self) -> bool {
        self.next_ordinal == self.total_rows
    }
    #[must_use]
    pub const fn receipt(self) -> M11RecursiveGreenQueryReceipt {
        self.receipt
    }
}

#[derive(Clone, Copy)]
struct QueryOpenFrame {
    frame: M11RecursiveGreenFrameId,
    kind: M11RecursiveGreenKind,
}

struct PendingLocation {
    byte_range: Range<u64>,
    utf16_range: Range<u64>,
    physical: M11RecursiveGreenSourceMetric,
    logical: M11RecursiveGreenSourceMetric,
    part: M11RecursiveGreenCoveragePart,
    atom: M11RecursiveGreenLogicalAtom,
    owner_index: usize,
    ancestry: Vec<M11RecursiveGreenAncestor>,
}

#[derive(Clone, Copy)]
struct FrameQueryOpenFrame {
    frame: M11RecursiveGreenFrameId,
    kind: M11RecursiveGreenKind,
    block_byte_start: u64,
    block_utf16_start: u64,
    inline_byte_start: Option<u64>,
    inline_byte_end: u64,
    inline_utf16_start: Option<u64>,
    inline_utf16_end: u64,
    gap_after_inline: bool,
    inline_is_contiguous_source: bool,
}

impl FrameQueryOpenFrame {
    const fn new(
        frame: M11RecursiveGreenFrameId,
        kind: M11RecursiveGreenKind,
        block_byte_start: u64,
        block_utf16_start: u64,
    ) -> Self {
        Self {
            frame,
            kind,
            block_byte_start,
            block_utf16_start,
            inline_byte_start: None,
            inline_byte_end: block_byte_start,
            inline_utf16_start: None,
            inline_utf16_end: block_utf16_start,
            gap_after_inline: false,
            inline_is_contiguous_source: true,
        }
    }
}

struct ResolvedFrame {
    frame: M11RecursiveGreenFrameId,
    kind: M11RecursiveGreenKind,
    block_source: Range<u64>,
    block_source_utf16: Range<u64>,
    inline_source: Range<u64>,
    inline_source_utf16: Range<u64>,
}

impl M11RecursiveGreenRoot {
    /// Returns either an exact row window or the precise caller budget that
    /// prevented one. Budget exhaustion remains distinct from malformed Green
    /// state for integrations that can surface a typed source gap.
    pub fn locate_renderable_rows_bounded(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
        requested_end_byte: u64,
        limits: M11RecursiveGreenRowQueryLimits,
    ) -> Result<M11RecursiveGreenRowQueryOutcome, M11RecursiveGreenError> {
        self.ensure_runtime(runtime)?;
        let covered_bytes = usize::try_from(self.summary.physical_bytes)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        let covered_utf16 = usize::try_from(self.summary.physical_utf16)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        let absolute_byte = self
            .source_base
            .bytes()
            .checked_add(
                u64::try_from(point.byte_offset)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
            )
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let absolute_utf16 = self
            .source_base
            .utf16()
            .checked_add(
                u64::try_from(point.utf16_offset)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
            )
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        if point.byte_offset > covered_bytes
            || point.utf16_offset > covered_utf16
            || u64::try_from(
                self.lease()?.utf16_offset_for_byte(
                    usize::try_from(absolute_byte)
                        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
                )?,
            )
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
                != absolute_utf16
            || requested_end_byte > self.summary.physical_bytes
        {
            return Err(M11RecursiveGreenError::InvalidPoint);
        }
        let tree = self
            .tree
            .as_ref()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        locate_renderable_rows_in_arena(
            runtime.producer_arena(),
            tree.as_ref(),
            self.summary,
            point,
            requested_end_byte,
            limits,
        )
    }

    /// Returns a bounded source-ordered window of parser-authored renderable
    /// rows and their exact recursive container paths.
    pub fn locate_renderable_rows(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
        requested_end_byte: u64,
        limits: M11RecursiveGreenRowQueryLimits,
    ) -> Result<M11RecursiveGreenRowWindow, M11RecursiveGreenError> {
        match self.locate_renderable_rows_bounded(runtime, point, requested_end_byte, limits)? {
            M11RecursiveGreenRowQueryOutcome::Window(window) => Ok(window),
            M11RecursiveGreenRowQueryOutcome::BudgetExceeded(_) => {
                Err(M11RecursiveGreenError::ZeroFuel)
            }
        }
    }

    /// Selects the physical coverage owner at one exact source point, verifies
    /// that its final kind is one of `expected_kinds`, and mints move-only
    /// authority for its contiguous inline source.
    ///
    /// The caller supplies only a semantic kind and authenticated byte/UTF-16
    /// point, never a range. Close-time retypes are observed before the fence
    /// is returned, so a Paragraph promoted to a Setext Heading cannot escape
    /// as stale Paragraph authority.
    pub fn locate_frame_fence_for_kinds(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
        expected_kinds: &[M11RecursiveGreenKind],
        limits: M11RecursiveGreenFrameQueryLimits,
    ) -> Result<Option<M11RecursiveGreenFrameFence>, M11RecursiveGreenFrameQueryError> {
        self.ensure_runtime(runtime)?;
        let covered_bytes = usize::try_from(self.summary.physical_bytes)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        let covered_utf16 = usize::try_from(self.summary.physical_utf16)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        let absolute_byte = self
            .source_base
            .bytes()
            .checked_add(
                u64::try_from(point.byte_offset)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
            )
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let absolute_utf16 = self
            .source_base
            .utf16()
            .checked_add(
                u64::try_from(point.utf16_offset)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
            )
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        if point.byte_offset > covered_bytes
            || point.utf16_offset > covered_utf16
            || self
                .lease()?
                .utf16_offset_for_byte(
                    usize::try_from(absolute_byte)
                        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
                )
                .map_err(M11RecursiveGreenError::from)?
                != usize::try_from(absolute_utf16)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        {
            return Err(M11RecursiveGreenError::InvalidPoint.into());
        }
        if self.summary.physical_bytes == 0 {
            return Ok(None);
        }
        let effective_byte = match (point.affinity, point.byte_offset) {
            (SourceBoundaryAffinity::Before, offset) if offset > 0 => offset - 1,
            (_, offset) if offset == covered_bytes => offset - 1,
            (_, offset) => offset,
        };
        let effective_byte =
            u64::try_from(effective_byte).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        let tree = self
            .tree
            .as_ref()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let arena = runtime.producer_arena();
        let mut inspection = SequenceInspectionReceipt::default();
        let mut callback_inspection = SequenceSpecInspection::default();
        let mut events_scanned = 0_u64;
        let mut storage_pages_visited = 0_u64;
        let mut maximum_open_depth = 0_usize;
        let point_leaf = tree
            .as_ref()
            .locate_leaf_containing_metric(
                arena,
                effective_byte,
                |summary| summary.physical_bytes,
                &mut inspection,
            )?
            .ok_or(M11RecursiveGreenError::Corrupt(
                "recursive-green point has no coverage leaf",
            ))?;
        if storage_pages_visited == limits.maximum_storage_pages_visited {
            return Err(M11RecursiveGreenFrameQueryError::BoundExceeded(
                M11RecursiveGreenFrameQueryBound::StoragePagesVisited,
            ));
        }
        storage_pages_visited += 1;
        let point_prefix = point_leaf
            .prefix
            .unwrap_or_else(RecursiveGreenSummary::empty);
        let external_open_at_leaf = u64::try_from(point_prefix.balance).map_err(|_| {
            M11RecursiveGreenError::Corrupt("recursive-green prefix has negative open depth")
        })?;
        let point_payload = arena
            .payload(point_leaf.id)
            .map_err(M11RecursiveGreenError::from)?;
        let mut local_inspection = SequenceSpecInspection::default();
        let point_decoded = decode_leaf(point_payload, &mut local_inspection)?.ok_or(
            M11RecursiveGreenError::Corrupt("measured Green leaf changed kind"),
        )?;
        accumulate_query_spec_inspection(&mut callback_inspection, local_inspection)?;

        #[derive(Clone, Copy)]
        struct LocalOpen {
            frame: M11RecursiveGreenFrameId,
            event_ordinal: u64,
        }

        let mut local_open = Vec::<LocalOpen>::new();
        local_open
            .try_reserve_exact(limits.maximum_open_depth.min(64))
            .map_err(|_| M11RecursiveGreenError::InvalidState)?;
        let mut external_closes = 0_u64;
        let mut point_source_bytes = point_prefix.physical_bytes;
        let mut point_source_utf16 = point_prefix.physical_utf16;
        let mut point_cursor = 0_usize;
        let mut target_event_ordinal = None;
        let mut target_frame = None;
        let mut external_owner_rank = None;
        for local_event_ordinal in 0..point_decoded.events {
            if events_scanned == limits.maximum_events_scanned {
                return Err(M11RecursiveGreenFrameQueryError::BoundExceeded(
                    M11RecursiveGreenFrameQueryBound::EventsScanned,
                ));
            }
            let event = decode_packed_event(point_decoded.event_bytes, &mut point_cursor)?;
            events_scanned = events_scanned
                .checked_add(1)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            let event_ordinal = point_prefix
                .events
                .checked_add(u64::from(local_event_ordinal))
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            match event {
                PackedGreenEvent::Enter { frame, .. } => {
                    local_open.push(LocalOpen {
                        frame,
                        event_ordinal,
                    });
                }
                PackedGreenEvent::RetypeOpen { frame, .. } => {
                    if let Some(current) = local_open.last() {
                        if current.frame != frame {
                            return Err(M11RecursiveGreenError::Corrupt(
                                "retype does not target the locally open top",
                            )
                            .into());
                        }
                    }
                }
                PackedGreenEvent::Exit { frame, .. } => {
                    if let Some(current) = local_open.pop() {
                        if current.frame != frame {
                            return Err(M11RecursiveGreenError::Corrupt(
                                "exit differs from its locally open frame",
                            )
                            .into());
                        }
                    } else {
                        external_closes = external_closes
                            .checked_add(1)
                            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                        if external_closes > external_open_at_leaf {
                            return Err(M11RecursiveGreenError::Corrupt(
                                "leaf prefix closes beyond its external open depth",
                            )
                            .into());
                        }
                    }
                }
                PackedGreenEvent::Coverage {
                    physical,
                    owner_depth,
                    ..
                } => {
                    let byte_end = point_source_bytes
                        .checked_add(physical.bytes())
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                    let utf16_end = point_source_utf16
                        .checked_add(physical.utf16())
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                    if effective_byte >= point_source_bytes && effective_byte < byte_end {
                        let owner_depth = usize::try_from(owner_depth)
                            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
                        let external_remaining = external_open_at_leaf
                            .checked_sub(external_closes)
                            .ok_or(M11RecursiveGreenError::Corrupt(
                                "leaf prefix external depth underflow",
                            ))?;
                        let total_depth = usize::try_from(external_remaining)
                            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
                            .checked_add(local_open.len())
                            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                        if total_depth > limits.maximum_open_depth {
                            return Err(M11RecursiveGreenFrameQueryError::BoundExceeded(
                                M11RecursiveGreenFrameQueryBound::OpenDepth,
                            ));
                        }
                        maximum_open_depth = maximum_open_depth.max(total_depth);
                        if owner_depth >= total_depth {
                            return Err(M11RecursiveGreenError::Corrupt(
                                "coverage owner is outside the point ancestry",
                            )
                            .into());
                        }
                        if owner_depth < local_open.len() {
                            let owner = local_open[local_open.len() - 1 - owner_depth];
                            target_event_ordinal = Some(owner.event_ordinal);
                            target_frame = Some(owner.frame);
                        } else {
                            external_owner_rank = Some(
                                u64::try_from(owner_depth - local_open.len())
                                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
                            );
                        }
                        break;
                    }
                    point_source_bytes = byte_end;
                    point_source_utf16 = utf16_end;
                }
                PackedGreenEvent::Property(_) => {}
            }
        }

        if target_event_ordinal.is_none() {
            let external_owner_rank = external_owner_rank.ok_or(
                M11RecursiveGreenError::Corrupt("recursive-green point has no coverage event"),
            )?;
            let prefix_end = point_leaf.ordinal;
            if prefix_end == 0 {
                return Err(M11RecursiveGreenError::Corrupt(
                    "point owner predates an empty leaf prefix",
                )
                .into());
            }
            let threshold = external_closes
                .checked_add(external_owner_rank)
                .and_then(|value| value.checked_add(1))
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            let prefix_opens = point_prefix.unmatched_opens()?;
            if prefix_opens < threshold {
                return Err(M11RecursiveGreenError::Corrupt(
                    "point owner rank exceeds unmatched prefix opens",
                )
                .into());
            }

            // Find the latest leaf boundary whose suffix still contains the
            // requested unmatched Enter. Fully covered AVL subtrees answer
            // each probe from their stored structural summary, so the search
            // is logarithmic in prefix leaves rather than linear in blocks.
            let mut included = 0_u64;
            let mut excluded = prefix_end;
            while included + 1 < excluded {
                let middle = included + (excluded - included) / 2;
                let opens = tree
                    .as_ref()
                    .range_summary(arena, middle..prefix_end, &mut inspection)?
                    .ok_or(M11RecursiveGreenError::Corrupt(
                        "nonempty recursive-green suffix has no summary",
                    ))?
                    .unmatched_opens()?;
                if opens >= threshold {
                    included = middle;
                } else {
                    excluded = middle;
                }
            }
            let owner_leaf_ordinal = included;
            let suffix = tree.as_ref().range_summary(
                arena,
                owner_leaf_ordinal + 1..prefix_end,
                &mut inspection,
            )?;
            let (suffix_opens, suffix_closes) = match suffix {
                Some(summary) => (summary.unmatched_opens()?, summary.unmatched_closes()?),
                None => (0, 0),
            };
            let surviving_suffix_opens = suffix_opens.saturating_sub(external_closes);
            let mut owner_rank_in_leaf = external_owner_rank
                .checked_sub(surviving_suffix_opens)
                .ok_or(M11RecursiveGreenError::Corrupt(
                    "summary-guided owner leaf skipped the requested open",
                ))?;
            let mut closes_needed = external_closes
                .saturating_sub(suffix_opens)
                .checked_add(suffix_closes)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            let owner_leaf = tree
                .as_ref()
                .locate_leaf_with_prefix(arena, owner_leaf_ordinal, &mut inspection)?
                .ok_or(M11RecursiveGreenError::Corrupt(
                    "summary-guided owner leaf is absent",
                ))?;
            if storage_pages_visited == limits.maximum_storage_pages_visited {
                return Err(M11RecursiveGreenFrameQueryError::BoundExceeded(
                    M11RecursiveGreenFrameQueryBound::StoragePagesVisited,
                ));
            }
            storage_pages_visited += 1;
            let payload = arena
                .payload(owner_leaf.id)
                .map_err(M11RecursiveGreenError::from)?;
            let mut local_inspection = SequenceSpecInspection::default();
            let decoded = decode_leaf(payload, &mut local_inspection)?.ok_or(
                M11RecursiveGreenError::Corrupt("measured Green leaf changed kind"),
            )?;
            accumulate_query_spec_inspection(&mut callback_inspection, local_inspection)?;
            let mut cursor = 0_usize;
            let mut events = Vec::new();
            events
                .try_reserve_exact(decoded.events as usize)
                .map_err(|_| M11RecursiveGreenError::InvalidState)?;
            for _ in 0..decoded.events {
                if events_scanned == limits.maximum_events_scanned {
                    return Err(M11RecursiveGreenFrameQueryError::BoundExceeded(
                        M11RecursiveGreenFrameQueryBound::EventsScanned,
                    ));
                }
                events.push(decode_packed_event(decoded.event_bytes, &mut cursor)?);
                events_scanned = events_scanned
                    .checked_add(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            }
            if cursor != decoded.event_bytes.len() {
                return Err(M11RecursiveGreenError::Corrupt(
                    "recursive-green owner leaf retained trailing bytes",
                )
                .into());
            }
            let owner_prefix_events = owner_leaf.prefix.map_or(0, |summary| summary.events);
            for (index, event) in events.into_iter().enumerate().rev() {
                match event {
                    PackedGreenEvent::Exit { .. } => {
                        closes_needed = closes_needed
                            .checked_add(1)
                            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                    }
                    PackedGreenEvent::Enter { .. } if closes_needed != 0 => {
                        closes_needed -= 1;
                    }
                    PackedGreenEvent::Enter { .. } if owner_rank_in_leaf != 0 => {
                        owner_rank_in_leaf -= 1;
                    }
                    PackedGreenEvent::Enter { frame, .. } => {
                        target_frame = Some(frame);
                        target_event_ordinal = Some(
                            owner_prefix_events
                                .checked_add(
                                    u64::try_from(index)
                                        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
                                )
                                .ok_or(M11RecursiveGreenError::CounterOverflow)?,
                        );
                        break;
                    }
                    PackedGreenEvent::Property(_)
                    | PackedGreenEvent::Coverage { .. }
                    | PackedGreenEvent::RetypeOpen { .. } => {}
                }
            }
        }

        let target_event_ordinal = target_event_ordinal.ok_or(M11RecursiveGreenError::Corrupt(
            "point owner Enter was not found",
        ))?;
        let target_frame = target_frame.ok_or(M11RecursiveGreenError::Corrupt(
            "point owner frame was not found",
        ))?;
        let mut open = Vec::<FrameQueryOpenFrame>::new();
        open.try_reserve_exact(limits.maximum_open_depth.min(64))
            .map_err(|_| M11RecursiveGreenError::InvalidState)?;
        let mut source_bytes = 0_u64;
        let mut source_utf16 = 0_u64;
        let mut current_depth = 0_usize;
        let mut selected_depth = None;
        let mut traversal_initialized = false;
        let mut resolved: Option<ResolvedFrame> = None;
        let mut point_has_no_matching_frame = false;
        let mut bound_exceeded = None;

        tree.as_ref().visit_leaves_from_metric(
            arena,
            target_event_ordinal,
            |summary| summary.events,
            &mut inspection,
            |leaf| {
                if storage_pages_visited == limits.maximum_storage_pages_visited {
                    bound_exceeded = Some(M11RecursiveGreenFrameQueryBound::StoragePagesVisited);
                    return Ok(SequenceLeafVisitControl::Stop);
                }
                storage_pages_visited = storage_pages_visited
                    .checked_add(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                let prefix = leaf.prefix.unwrap_or_else(RecursiveGreenSummary::empty);
                if !traversal_initialized {
                    source_bytes = prefix.physical_bytes;
                    source_utf16 = prefix.physical_utf16;
                    current_depth = usize::try_from(prefix.balance).map_err(|_| {
                        M11RecursiveGreenError::Corrupt(
                            "recursive-green forward prefix has negative depth",
                        )
                    })?;
                    traversal_initialized = true;
                } else if source_bytes != prefix.physical_bytes
                    || source_utf16 != prefix.physical_utf16
                {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "recursive-green forward source prefix changed",
                    ));
                }
                let payload = arena.payload(leaf.id)?;
                let mut local_inspection = SequenceSpecInspection::default();
                let decoded = decode_leaf(payload, &mut local_inspection)?.ok_or(
                    M11RecursiveGreenError::Corrupt("measured Green leaf changed kind"),
                )?;
                accumulate_query_spec_inspection(&mut callback_inspection, local_inspection)?;
                let mut cursor = 0_usize;
                let mut event_ordinal = prefix.events;
                let mut stop = false;
                for _ in 0..decoded.events {
                    if events_scanned == limits.maximum_events_scanned {
                        bound_exceeded = Some(M11RecursiveGreenFrameQueryBound::EventsScanned);
                        stop = true;
                        break;
                    }
                    let event = decode_packed_event(decoded.event_bytes, &mut cursor)?;
                    events_scanned = events_scanned
                        .checked_add(1)
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                    if event_ordinal < target_event_ordinal {
                        match event {
                            PackedGreenEvent::Enter { .. } => {
                                current_depth = current_depth
                                    .checked_add(1)
                                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                            }
                            PackedGreenEvent::Exit { .. } => {
                                current_depth = current_depth.checked_sub(1).ok_or(
                                    M11RecursiveGreenError::Corrupt(
                                        "recursive-green forward prefix depth underflow",
                                    ),
                                )?;
                            }
                            PackedGreenEvent::Coverage { physical, .. } => {
                                source_bytes = source_bytes
                                    .checked_add(physical.bytes())
                                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                                source_utf16 = source_utf16
                                    .checked_add(physical.utf16())
                                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                            }
                            PackedGreenEvent::Property(_) | PackedGreenEvent::RetypeOpen { .. } => {
                            }
                        }
                        event_ordinal = event_ordinal
                            .checked_add(1)
                            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                        continue;
                    }

                    match event {
                        PackedGreenEvent::Enter { frame, kind } => {
                            let next_depth = current_depth
                                .checked_add(1)
                                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                            if next_depth > limits.maximum_open_depth {
                                bound_exceeded = Some(M11RecursiveGreenFrameQueryBound::OpenDepth);
                                stop = true;
                                break;
                            }
                            if open.is_empty() {
                                if event_ordinal != target_event_ordinal || frame != target_frame {
                                    return Err(M11RecursiveGreenError::Corrupt(
                                        "summary-guided owner Enter changed",
                                    ));
                                }
                                selected_depth = Some(current_depth);
                            }
                            open.push(FrameQueryOpenFrame::new(
                                frame,
                                kind,
                                source_bytes,
                                source_utf16,
                            ));
                            current_depth = next_depth;
                            maximum_open_depth = maximum_open_depth.max(current_depth);
                        }
                        PackedGreenEvent::RetypeOpen { frame, kind, .. } => {
                            let current = open.last_mut().ok_or(
                                M11RecursiveGreenError::Corrupt("retype has no queried open frame"),
                            )?;
                            if current.frame != frame {
                                return Err(M11RecursiveGreenError::Corrupt(
                                    "retype does not target the queried open top",
                                ));
                            }
                            current.kind = kind;
                        }
                        PackedGreenEvent::Exit {
                            frame, final_kind, ..
                        } => {
                            let current = open.pop().ok_or(M11RecursiveGreenError::Corrupt(
                                "exit has no queried open frame",
                            ))?;
                            if current.frame != frame || current.kind != final_kind {
                                return Err(M11RecursiveGreenError::Corrupt(
                                    "exit differs from its queried open frame",
                                ));
                            }
                            current_depth = current_depth.checked_sub(1).ok_or(
                                M11RecursiveGreenError::Corrupt(
                                    "recursive-green queried depth underflow",
                                ),
                            )?;
                            if frame == target_frame {
                                let Some(inline_byte_start) = current.inline_byte_start else {
                                    point_has_no_matching_frame = true;
                                    stop = true;
                                    break;
                                };
                                let Some(inline_utf16_start) = current.inline_utf16_start else {
                                    return Err(M11RecursiveGreenError::Corrupt(
                                        "inline byte and UTF-16 starts differ",
                                    ));
                                };
                                if !expected_kinds.contains(&final_kind)
                                    || !current.inline_is_contiguous_source
                                {
                                    point_has_no_matching_frame = true;
                                } else {
                                    resolved = Some(ResolvedFrame {
                                        frame,
                                        kind: final_kind,
                                        block_source: current.block_byte_start..source_bytes,
                                        block_source_utf16: current.block_utf16_start..source_utf16,
                                        inline_source: inline_byte_start..current.inline_byte_end,
                                        inline_source_utf16: inline_utf16_start
                                            ..current.inline_utf16_end,
                                    });
                                }
                                stop = true;
                                break;
                            }
                        }
                        PackedGreenEvent::Coverage {
                            physical,
                            owner_depth,
                            part,
                            atom,
                        } => {
                            let byte_end = source_bytes
                                .checked_add(physical.bytes())
                                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                            let utf16_end = source_utf16
                                .checked_add(physical.utf16())
                                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                            let physical_owner_depth = usize::try_from(owner_depth)
                                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
                            let physical_owner = current_depth
                                .checked_sub(physical_owner_depth + 1)
                                .ok_or(M11RecursiveGreenError::Corrupt(
                                    "coverage owner is outside the queried open path",
                                ))?;
                            let logical_owner = match atom {
                                M11RecursiveGreenLogicalAtom::None
                                | M11RecursiveGreenLogicalAtom::HiddenUpstream => None,
                                M11RecursiveGreenLogicalAtom::TabToSpaces {
                                    target_owner_depth,
                                    ..
                                } => {
                                    let depth = usize::try_from(target_owner_depth)
                                        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
                                    Some(current_depth.checked_sub(depth + 1).ok_or(
                                        M11RecursiveGreenError::Corrupt(
                                            "logical coverage owner is outside the queried path",
                                        ),
                                    )?)
                                }
                                _ => Some(physical_owner),
                            };
                            let selected_depth =
                                selected_depth.ok_or(M11RecursiveGreenError::Corrupt(
                                    "queried coverage precedes its selected frame",
                                ))?;
                            let selected =
                                open.first_mut().ok_or(M11RecursiveGreenError::Corrupt(
                                    "queried coverage lost its selected frame",
                                ))?;
                            let compatible = logical_owner == Some(selected_depth)
                                && part == M11RecursiveGreenCoveragePart::Content
                                && matches!(
                                    atom,
                                    M11RecursiveGreenLogicalAtom::Identity
                                        | M11RecursiveGreenLogicalAtom::LfToLf
                                        | M11RecursiveGreenLogicalAtom::CrLfToLf
                                        | M11RecursiveGreenLogicalAtom::LoneCrToLf
                                );
                            if compatible {
                                if selected.gap_after_inline {
                                    selected.inline_is_contiguous_source = false;
                                }
                                if selected.inline_byte_start.is_none() {
                                    selected.inline_byte_start = Some(source_bytes);
                                    selected.inline_utf16_start = Some(source_utf16);
                                }
                                selected.inline_byte_end = byte_end;
                                selected.inline_utf16_end = utf16_end;
                                selected.gap_after_inline = false;
                            } else if logical_owner == Some(selected_depth)
                                && !atom.logical_metric(physical).is_empty()
                            {
                                selected.inline_is_contiguous_source = false;
                            } else if selected.inline_byte_start.is_some() {
                                selected.gap_after_inline = true;
                            }
                            if selected.inline_byte_start.is_some_and(|start| {
                                selected.inline_byte_end.saturating_sub(start)
                                    > limits.maximum_inline_source_bytes
                            }) {
                                bound_exceeded =
                                    Some(M11RecursiveGreenFrameQueryBound::InlineSourceBytes);
                                stop = true;
                            }
                            source_bytes = byte_end;
                            source_utf16 = utf16_end;
                            if stop {
                                break;
                            }
                        }
                        PackedGreenEvent::Property(_) => {}
                    }
                    event_ordinal = event_ordinal
                        .checked_add(1)
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                }
                if !stop && cursor != decoded.event_bytes.len() {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "recursive-green leaf retained trailing bytes",
                    ));
                }
                Ok(if stop {
                    SequenceLeafVisitControl::Stop
                } else {
                    SequenceLeafVisitControl::Continue
                })
            },
        )?;
        inspection.spec.payload_bytes_inspected = inspection
            .spec
            .payload_bytes_inspected
            .checked_add(callback_inspection.payload_bytes_inspected)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        inspection.spec.spec_items_hashed = inspection
            .spec
            .spec_items_hashed
            .checked_add(callback_inspection.spec_items_hashed)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        if let Some(bound) = bound_exceeded {
            return Err(M11RecursiveGreenFrameQueryError::BoundExceeded(bound));
        }
        if point_has_no_matching_frame {
            return Ok(None);
        }
        let Some(resolved) = resolved else {
            return Err(M11RecursiveGreenError::Corrupt(
                "selected recursive-green frame did not close",
            )
            .into());
        };
        let receipt = M11RecursiveGreenQueryReceipt {
            node_headers_decoded: inspection.node_headers_decoded,
            summary_combinations: inspection.summary_combinations,
            payload_bytes_inspected: inspection.spec.payload_bytes_inspected,
            events_authenticated: inspection.spec.spec_items_hashed,
            storage_pages_visited,
            events_scanned,
            maximum_open_depth,
        };
        let inline_start = usize::try_from(resolved.inline_source.start)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        let inline_end = usize::try_from(resolved.inline_source.end)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        let authority = M11ParserSourceRangeAuthority::new(
            runtime,
            self.lease()?.duplicate(),
            inline_start..inline_end,
        )?;
        Ok(Some(M11RecursiveGreenFrameFence {
            source: self.source(),
            frame: resolved.frame,
            kind: resolved.kind,
            block_source: resolved.block_source,
            block_source_utf16: resolved.block_source_utf16,
            inline_source: resolved.inline_source,
            inline_source_utf16: resolved.inline_source_utf16,
            receipt,
            authority,
        }))
    }

    /// Selects one final inline-bearing renderable row through the bounded
    /// row zipper and mints exact source authority from its cached close
    /// geometry.
    ///
    /// Unlike [`Self::locate_frame_fence_for_kinds`], this path does not need
    /// to replay every event from a long-lived frame's `Enter` to its `Exit`.
    /// The row query authenticates the final kind, frame, physical envelope,
    /// and contiguous editable range before this method creates authority.
    pub fn locate_renderable_row_fence_for_kinds(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
        expected_kinds: &[M11RecursiveGreenKind],
        limits: M11RecursiveGreenRowQueryLimits,
        maximum_inline_source_bytes: u64,
    ) -> Result<Option<M11RecursiveGreenFrameFence>, M11RecursiveGreenFrameQueryError> {
        if maximum_inline_source_bytes == 0 || limits.maximum_rows != 1 {
            return Err(M11RecursiveGreenError::InvalidState.into());
        }
        let requested_end_byte = self.summary.physical_bytes;
        let window = match self.locate_renderable_rows_bounded(
            runtime,
            point,
            requested_end_byte,
            limits,
        )? {
            M11RecursiveGreenRowQueryOutcome::Window(window) => window,
            M11RecursiveGreenRowQueryOutcome::BudgetExceeded(exceeded) => {
                let bound = match exceeded.limit() {
                    M11RecursiveGreenRowQueryLimit::StoragePages => {
                        M11RecursiveGreenFrameQueryBound::StoragePagesVisited
                    }
                    M11RecursiveGreenRowQueryLimit::EventsScanned => {
                        M11RecursiveGreenFrameQueryBound::EventsScanned
                    }
                    M11RecursiveGreenRowQueryLimit::TreeNodes => {
                        M11RecursiveGreenFrameQueryBound::TreeNodesVisited
                    }
                    M11RecursiveGreenRowQueryLimit::OpenDepth => {
                        M11RecursiveGreenFrameQueryBound::OpenDepth
                    }
                };
                return Err(M11RecursiveGreenFrameQueryError::BoundExceeded(bound));
            }
        };
        let Some(row) = window.rows().first() else {
            return Ok(None);
        };
        let effective_byte = match (point.affinity, point.byte_offset) {
            (SourceBoundaryAffinity::Before, offset) if offset > 0 => offset - 1,
            (_, offset)
                if u64::try_from(offset).ok() == Some(self.summary.physical_bytes)
                    && offset > 0 =>
            {
                offset - 1
            }
            (_, offset) => offset,
        };
        let effective_byte =
            u64::try_from(effective_byte).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        if effective_byte < row.physical.start || effective_byte >= row.physical.end {
            // The row zipper may legitimately advance across an unrendered
            // separator. Such a row is useful to viewport callers, but it
            // does not own this inline-refinement point.
            return Ok(None);
        }
        if !expected_kinds.contains(&row.kind)
            || row.edit_capability != M11RecursiveGreenRowEditCapability::Contiguous
        {
            return Ok(None);
        }
        let Some(inline_source) = row.editable.clone() else {
            return Err(M11RecursiveGreenError::Corrupt(
                "contiguous recursive-Green row omitted editable bytes",
            )
            .into());
        };
        let Some(inline_source_utf16) = row.editable_utf16.clone() else {
            return Err(M11RecursiveGreenError::Corrupt(
                "contiguous recursive-Green row omitted editable UTF-16",
            )
            .into());
        };
        if inline_source.end.saturating_sub(inline_source.start) > maximum_inline_source_bytes {
            return Err(M11RecursiveGreenFrameQueryError::BoundExceeded(
                M11RecursiveGreenFrameQueryBound::InlineSourceBytes,
            ));
        }
        let inline_start = usize::try_from(inline_source.start)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        let inline_end = usize::try_from(inline_source.end)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        let authority = M11ParserSourceRangeAuthority::new(
            runtime,
            self.lease()?.duplicate(),
            inline_start..inline_end,
        )?;
        Ok(Some(M11RecursiveGreenFrameFence {
            source: self.source(),
            frame: row.frame,
            kind: row.kind,
            block_source: row.physical.clone(),
            block_source_utf16: row.physical_utf16.clone(),
            inline_source,
            inline_source_utf16,
            receipt: window.receipt(),
            authority,
        }))
    }

    /// Selects one top-level projected container with exactly one direct
    /// child, and independently folds its source projection from authenticated
    /// Green coverage.
    ///
    /// This is intentionally stricter than a general ancestor query. The
    /// exact final path must be `root -> container -> child` (the point may
    /// land on container-owned marker coverage and therefore omit the child
    /// from its point ancestry), and the complete container event interval
    /// must close exactly one child of `expected_child_kind`. Callers provide
    /// semantic kinds, never source ranges or projected metrics.
    pub fn locate_single_child_projected_container_fence(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
        expected_root_kind: M11RecursiveGreenKind,
        expected_container_kind: M11RecursiveGreenKind,
        expected_child_kind: M11RecursiveGreenKind,
        limits: M11RecursiveGreenRowQueryLimits,
        maximum_source_bytes: u64,
    ) -> Result<Option<M11RecursiveGreenProjectedFrameFence>, M11RecursiveGreenFrameQueryError>
    {
        self.ensure_runtime(runtime)?;
        if limits.maximum_rows != 1 || maximum_source_bytes == 0 {
            return Err(M11RecursiveGreenError::InvalidState.into());
        }
        if point.byte_offset > self.source().byte_len()
            || point.utf16_offset > self.source().utf16_len()
            || self
                .lease()?
                .utf16_offset_for_byte(point.byte_offset)
                .map_err(M11RecursiveGreenError::from)?
                != point.utf16_offset
        {
            return Err(M11RecursiveGreenError::InvalidPoint.into());
        }
        if self.summary.physical_bytes == 0 {
            return Ok(None);
        }

        let tree = self
            .tree
            .as_ref()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        let tree = tree.as_ref();
        let arena = runtime.producer_arena();
        let mut work = PointZipperWork::default();
        let Some(location) =
            locate_point_in_arena_zipper_prepared(arena, tree, self.summary, point, &mut work)?
        else {
            return Ok(None);
        };
        if let Some(bound) =
            frame_bound_for_row_query_work(&work, limits, location.zipper_open.len())
        {
            return Err(M11RecursiveGreenFrameQueryError::BoundExceeded(bound));
        }
        let root_leaf_count = tree
            .summary(arena, &mut work.inspection)?
            .ok_or(M11RecursiveGreenError::Corrupt(
                "projected-container query lost its root measure",
            ))?
            .leaves();

        // Only top-level single-child containers are admitted by this first
        // projection authority. Marker-owned point coverage may stop at the
        // container; content-owned coverage includes its child.
        if !(location.zipper_open.len() == 2 || location.zipper_open.len() == 3) {
            return Ok(None);
        }
        let root_open = location.zipper_open[0];
        let container_open = location.zipper_open[1];
        let root_boundary =
            point_zipper_frame_boundary(arena, tree, root_leaf_count, root_open, &mut work)?;
        let container_boundary =
            point_zipper_frame_boundary(arena, tree, root_leaf_count, container_open, &mut work)?;
        if root_boundary.final_kind != expected_root_kind
            || container_boundary.final_kind != expected_container_kind
        {
            return Ok(None);
        }
        if let Some(child_open) = location.zipper_open.get(2).copied() {
            let child_boundary =
                point_zipper_frame_boundary(arena, tree, root_leaf_count, child_open, &mut work)?;
            if child_boundary.final_kind != expected_child_kind {
                return Ok(None);
            }
        }

        // The UTF-8 BOM is root-owned nonlogical Gap coverage rather than part of the
        // BlockQuote frame, while the marked-line projector deliberately owns
        // complete physical lines (and validates the BOM as hidden prefix).
        // Admit only the exact parser-authenticated BOF geometry; arbitrary
        // hidden top-level source such as a reference definition must never be
        // widened into this container.
        let enter_leaf_events = work.decode_leaf_events(arena, container_open.enter_leaf)?;
        let include_bof_hidden_prefix = container_open.byte_start == 3
            && container_open.utf16_start == 1
            && container_open
                .enter_event_index
                .checked_sub(1)
                .is_some_and(|index| {
                    matches!(
                            enter_leaf_events.get(index),
                        Some(PackedGreenEvent::Coverage {
                            physical,
                            part: M11RecursiveGreenCoveragePart::Gap,
                            atom: M11RecursiveGreenLogicalAtom::None,
                            ..
                        })
                            if physical.bytes() == 3
                                && physical.utf16() == 1
                    )
                });
        let fenced_byte_start = if include_bof_hidden_prefix {
            0
        } else {
            container_open.byte_start
        };
        let fenced_utf16_start = if include_bof_hidden_prefix {
            0
        } else {
            container_open.utf16_start
        };

        let block_bytes = container_boundary
            .byte_end
            .checked_sub(container_open.byte_start)
            .ok_or(M11RecursiveGreenError::Corrupt(
                "projected container ends before its Enter",
            ))?;
        let block_utf16 = container_boundary
            .utf16_end
            .checked_sub(container_open.utf16_start)
            .ok_or(M11RecursiveGreenError::Corrupt(
                "projected container UTF-16 ends before its Enter",
            ))?;
        let fenced_bytes = container_boundary
            .byte_end
            .checked_sub(fenced_byte_start)
            .ok_or(M11RecursiveGreenError::Corrupt(
                "projected container fence starts after its Exit",
            ))?;
        if block_bytes == 0 || fenced_bytes > maximum_source_bytes {
            return if fenced_bytes > maximum_source_bytes {
                Err(M11RecursiveGreenFrameQueryError::BoundExceeded(
                    M11RecursiveGreenFrameQueryBound::InlineSourceBytes,
                ))
            } else {
                Ok(None)
            };
        }

        let mut relative_depth = 0_usize;
        let mut direct_child = None;
        let mut direct_children = 0_u32;
        let mut observed_physical_bytes = 0_u64;
        let mut observed_physical_utf16 = 0_u64;
        let mut projected_bytes = 0_u64;
        let mut projected_utf16 = 0_u64;
        let mut completed_lines = 0_u64;
        let mut projected_since_line_ending = false;

        for leaf_ordinal in container_open.enter_leaf_ordinal..=container_boundary.exit_leaf_ordinal
        {
            let leaf = tree
                .locate_leaf_with_prefix(arena, leaf_ordinal, &mut work.inspection)?
                .ok_or(M11RecursiveGreenError::Corrupt(
                    "projected-container traversal lost a Green leaf",
                ))?;
            let events = work.decode_leaf_events(arena, leaf.id)?;
            let first = if leaf_ordinal == container_open.enter_leaf_ordinal {
                container_open.enter_event_index + 1
            } else {
                0
            };
            let last = if leaf_ordinal == container_boundary.exit_leaf_ordinal {
                container_boundary.exit_event_index
            } else {
                events.len()
            };
            for event in events.into_iter().take(last).skip(first) {
                match event {
                    PackedGreenEvent::Enter { frame, .. } => {
                        if relative_depth == 0 {
                            direct_children = direct_children
                                .checked_add(1)
                                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                            direct_child = Some(frame);
                        }
                        relative_depth = relative_depth
                            .checked_add(1)
                            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                    }
                    PackedGreenEvent::Exit {
                        frame, final_kind, ..
                    } => {
                        relative_depth = relative_depth.checked_sub(1).ok_or(
                            M11RecursiveGreenError::Corrupt(
                                "projected-container child depth underflowed",
                            ),
                        )?;
                        if relative_depth == 0
                            && (direct_child != Some(frame) || final_kind != expected_child_kind)
                        {
                            return Ok(None);
                        }
                    }
                    PackedGreenEvent::Coverage {
                        physical,
                        part,
                        atom,
                        ..
                    } => {
                        observed_physical_bytes = observed_physical_bytes
                            .checked_add(physical.bytes())
                            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                        observed_physical_utf16 = observed_physical_utf16
                            .checked_add(physical.utf16())
                            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                        let retained_closing_terminator = part
                            == M11RecursiveGreenCoveragePart::Terminal
                            && atom == M11RecursiveGreenLogicalAtom::None;
                        if retained_closing_terminator
                            || !matches!(
                                atom,
                                M11RecursiveGreenLogicalAtom::None
                                    | M11RecursiveGreenLogicalAtom::HiddenUpstream
                            )
                        {
                            projected_bytes = projected_bytes
                                .checked_add(physical.bytes())
                                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                            projected_utf16 = projected_utf16
                                .checked_add(physical.utf16())
                                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                            if matches!(
                                atom,
                                M11RecursiveGreenLogicalAtom::LfToLf
                                    | M11RecursiveGreenLogicalAtom::CrLfToLf
                                    | M11RecursiveGreenLogicalAtom::LoneCrToLf
                            ) || retained_closing_terminator
                            {
                                completed_lines = completed_lines
                                    .checked_add(1)
                                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                                projected_since_line_ending = false;
                            } else {
                                projected_since_line_ending = true;
                            }
                        }
                    }
                    PackedGreenEvent::Property(_) | PackedGreenEvent::RetypeOpen { .. } => {}
                }
            }
            if let Some(bound) =
                frame_bound_for_row_query_work(&work, limits, location.zipper_open.len())
            {
                return Err(M11RecursiveGreenFrameQueryError::BoundExceeded(bound));
            }
        }
        if relative_depth != 0
            || direct_children != 1
            || observed_physical_bytes != block_bytes
            || observed_physical_utf16 != block_utf16
        {
            return Err(M11RecursiveGreenError::Corrupt(
                "projected-container fold disagrees with its authenticated frame envelope",
            )
            .into());
        }
        let line_count = completed_lines
            .checked_add(u64::from(projected_since_line_ending))
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        if line_count == 0 || projected_bytes == 0 || projected_utf16 == 0 {
            return Ok(None);
        }

        let block_start = usize::try_from(fenced_byte_start)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        let block_end = usize::try_from(container_boundary.byte_end)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        let authority = M11ParserSourceRangeAuthority::new(
            runtime,
            self.lease()?.duplicate(),
            block_start..block_end,
        )?;
        let receipt = work.finish_receipt(location.zipper_open.len())?;
        Ok(Some(M11RecursiveGreenProjectedFrameFence {
            source: self.source(),
            frame: container_open.frame,
            kind: container_boundary.final_kind,
            block_source: fenced_byte_start..container_boundary.byte_end,
            block_source_utf16: fenced_utf16_start..container_boundary.utf16_end,
            line_count,
            projected_source: M11RecursiveGreenSourceMetric::from_validated(
                projected_bytes,
                projected_utf16,
            ),
            receipt,
            authority,
        }))
    }

    /// Compatibility entry point for callers that admit exactly one final
    /// recursive-Green kind.
    pub fn locate_frame_fence(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
        expected_kind: M11RecursiveGreenKind,
        limits: M11RecursiveGreenFrameQueryLimits,
    ) -> Result<Option<M11RecursiveGreenFrameFence>, M11RecursiveGreenFrameQueryError> {
        self.locate_frame_fence_for_kinds(runtime, point, &[expected_kind], limits)
    }

    /// Resolves one exact source point and its final recursive Green ancestry.
    ///
    /// This first production surface performs one authenticated ordered event
    /// visit, preserving correctness for close-time retypes such as Setext.
    /// The receipt makes its linear page count explicit; the measured summary
    /// already carries the open-witness algebra needed by the bounded zipper
    /// optimization without changing this API.
    pub fn locate_point(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
    ) -> Result<Option<M11RecursiveGreenLocation>, M11RecursiveGreenError> {
        self.ensure_runtime(runtime)?;
        if point.byte_offset > self.source().byte_len()
            || point.utf16_offset > self.source().utf16_len()
            || self.lease()?.utf16_offset_for_byte(point.byte_offset)? != point.utf16_offset
        {
            return Err(M11RecursiveGreenError::InvalidPoint);
        }
        let tree = self
            .tree
            .as_ref()
            .ok_or(M11RecursiveGreenError::InvalidState)?;
        locate_point_in_arena(runtime.producer_arena(), tree.as_ref(), self.summary, point)
    }
}

/// One bounded renderable-row window together with the inline-leaf fences its
/// single shared walk minted.
///
/// `fences` is index-aligned with `window.rows()`: `None` marks a row whose
/// final kind or edit capability disqualifies it, exactly as the per-point
/// fence query would report for that row's own start. Every returned fence
/// carries the shared window receipt, which reports the one walk that
/// authenticated all of them.
pub struct M11RecursiveGreenRowFenceWindow {
    window: M11RecursiveGreenRowWindow,
    fences: Vec<Option<M11RecursiveGreenFrameFence>>,
}

impl M11RecursiveGreenRowFenceWindow {
    #[must_use]
    pub const fn window(&self) -> &M11RecursiveGreenRowWindow {
        &self.window
    }

    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        M11RecursiveGreenRowWindow,
        Vec<Option<M11RecursiveGreenFrameFence>>,
    ) {
        (self.window, self.fences)
    }
}

impl M11RecursiveGreenSliceRoot {
    pub fn locate_renderable_rows_bounded(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
        requested_end_byte: u64,
        limits: M11RecursiveGreenRowQueryLimits,
    ) -> Result<M11RecursiveGreenRowQueryOutcome, M11RecursiveGreenError> {
        let byte_base = self.source_base.bytes();
        let utf16_base = self.source_base.utf16();
        let local_point = M11RecursiveGreenPoint::new(
            usize::try_from(
                u64::try_from(point.byte_offset)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
                    .checked_sub(byte_base)
                    .ok_or(M11RecursiveGreenError::InvalidPoint)?,
            )
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
            usize::try_from(
                u64::try_from(point.utf16_offset)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
                    .checked_sub(utf16_base)
                    .ok_or(M11RecursiveGreenError::InvalidPoint)?,
            )
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
            point.affinity,
        );
        let local_end = requested_end_byte
            .checked_sub(byte_base)
            .ok_or(M11RecursiveGreenError::InvalidPoint)?;
        match self
            .root
            .locate_renderable_rows_bounded(runtime, local_point, local_end, limits)?
        {
            M11RecursiveGreenRowQueryOutcome::Window(mut window) => {
                offset_slice_row_window(
                    &mut window,
                    byte_base,
                    utf16_base,
                    self.row_base,
                    &self.open_frame_bases,
                )?;
                Ok(M11RecursiveGreenRowQueryOutcome::Window(window))
            }
            M11RecursiveGreenRowQueryOutcome::BudgetExceeded(exceeded) => {
                Ok(M11RecursiveGreenRowQueryOutcome::BudgetExceeded(exceeded))
            }
        }
    }

    pub fn locate_renderable_rows(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
        requested_end_byte: u64,
        limits: M11RecursiveGreenRowQueryLimits,
    ) -> Result<M11RecursiveGreenRowWindow, M11RecursiveGreenError> {
        match self.locate_renderable_rows_bounded(runtime, point, requested_end_byte, limits)? {
            M11RecursiveGreenRowQueryOutcome::Window(window) => Ok(window),
            M11RecursiveGreenRowQueryOutcome::BudgetExceeded(_) => {
                Err(M11RecursiveGreenError::ZeroFuel)
            }
        }
    }

    pub fn locate_renderable_row_fence_for_kinds(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
        expected_kinds: &[M11RecursiveGreenKind],
        limits: M11RecursiveGreenRowQueryLimits,
        maximum_inline_source_bytes: u64,
    ) -> Result<Option<M11RecursiveGreenFrameFence>, M11RecursiveGreenFrameQueryError> {
        if maximum_inline_source_bytes == 0 || limits.maximum_rows != 1 {
            return Err(M11RecursiveGreenError::InvalidState.into());
        }
        let requested_end = self
            .source_base
            .bytes()
            .checked_add(self.root.summary.physical_bytes)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let window =
            match self.locate_renderable_rows_bounded(runtime, point, requested_end, limits)? {
                M11RecursiveGreenRowQueryOutcome::Window(window) => window,
                M11RecursiveGreenRowQueryOutcome::BudgetExceeded(exceeded) => {
                    let bound = match exceeded.limit() {
                        M11RecursiveGreenRowQueryLimit::StoragePages => {
                            M11RecursiveGreenFrameQueryBound::StoragePagesVisited
                        }
                        M11RecursiveGreenRowQueryLimit::EventsScanned => {
                            M11RecursiveGreenFrameQueryBound::EventsScanned
                        }
                        M11RecursiveGreenRowQueryLimit::TreeNodes => {
                            M11RecursiveGreenFrameQueryBound::TreeNodesVisited
                        }
                        M11RecursiveGreenRowQueryLimit::OpenDepth => {
                            M11RecursiveGreenFrameQueryBound::OpenDepth
                        }
                    };
                    return Err(M11RecursiveGreenFrameQueryError::BoundExceeded(bound));
                }
            };
        let Some(row) = window.rows.first() else {
            return Ok(None);
        };
        let effective_byte = match (point.affinity, point.byte_offset) {
            (SourceBoundaryAffinity::Before, offset) if offset > 0 => offset - 1,
            (_, offset) if u64::try_from(offset).ok() == Some(requested_end) && offset > 0 => {
                offset - 1
            }
            (_, offset) => offset,
        };
        let effective_byte =
            u64::try_from(effective_byte).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        if effective_byte < row.physical.start || effective_byte >= row.physical.end {
            return Ok(None);
        }
        if !expected_kinds.contains(&row.kind)
            || row.edit_capability != M11RecursiveGreenRowEditCapability::Contiguous
        {
            return Ok(None);
        }
        let Some(inline_source) = row.editable.clone() else {
            return Err(M11RecursiveGreenError::Corrupt(
                "contiguous recursive-Green slice row omitted editable bytes",
            )
            .into());
        };
        let Some(inline_source_utf16) = row.editable_utf16.clone() else {
            return Err(M11RecursiveGreenError::Corrupt(
                "contiguous recursive-Green slice row omitted editable UTF-16",
            )
            .into());
        };
        if inline_source.end.saturating_sub(inline_source.start) > maximum_inline_source_bytes {
            return Err(M11RecursiveGreenFrameQueryError::BoundExceeded(
                M11RecursiveGreenFrameQueryBound::InlineSourceBytes,
            ));
        }
        let authority = M11ParserSourceRangeAuthority::new(
            runtime,
            self.root.lease()?.duplicate(),
            usize::try_from(inline_source.start)
                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
                ..usize::try_from(inline_source.end)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
        )?;
        Ok(Some(M11RecursiveGreenFrameFence {
            source: self.root.source(),
            frame: row.frame,
            kind: row.kind,
            block_source: row.physical.clone(),
            block_source_utf16: row.physical_utf16.clone(),
            inline_source,
            inline_source_utf16,
            receipt: window.receipt,
            authority,
        }))
    }

    /// Locates a bounded renderable-row window and mints the inline-leaf
    /// fence of every qualifying row through the one shared walk.
    ///
    /// Row-for-row this applies the same admission as
    /// [`Self::locate_renderable_row_fence_for_kinds`] anchored at each row's
    /// own start: a row outside `expected_kinds` or without a contiguous edit
    /// capability yields `None`, a contiguous row missing its editable
    /// geometry is corrupt, and a contiguous inline run larger than
    /// `maximum_inline_source_bytes` fails the whole request closed with the
    /// same typed bound. Budget exhaustion maps to the same typed bounds as
    /// the per-point query. No caller-supplied range can widen a fence: every
    /// range is parser-authored row geometry from the walk itself.
    pub fn locate_renderable_row_fences_for_kinds(
        &self,
        runtime: &DocumentRuntime,
        point: M11RecursiveGreenPoint,
        expected_kinds: &[M11RecursiveGreenKind],
        limits: M11RecursiveGreenRowQueryLimits,
        maximum_inline_source_bytes: u64,
    ) -> Result<M11RecursiveGreenRowFenceWindow, M11RecursiveGreenFrameQueryError> {
        if maximum_inline_source_bytes == 0 {
            return Err(M11RecursiveGreenError::InvalidState.into());
        }
        let requested_end = self
            .source_base
            .bytes()
            .checked_add(self.root.summary.physical_bytes)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let window =
            match self.locate_renderable_rows_bounded(runtime, point, requested_end, limits)? {
                M11RecursiveGreenRowQueryOutcome::Window(window) => window,
                M11RecursiveGreenRowQueryOutcome::BudgetExceeded(exceeded) => {
                    let bound = match exceeded.limit() {
                        M11RecursiveGreenRowQueryLimit::StoragePages => {
                            M11RecursiveGreenFrameQueryBound::StoragePagesVisited
                        }
                        M11RecursiveGreenRowQueryLimit::EventsScanned => {
                            M11RecursiveGreenFrameQueryBound::EventsScanned
                        }
                        M11RecursiveGreenRowQueryLimit::TreeNodes => {
                            M11RecursiveGreenFrameQueryBound::TreeNodesVisited
                        }
                        M11RecursiveGreenRowQueryLimit::OpenDepth => {
                            M11RecursiveGreenFrameQueryBound::OpenDepth
                        }
                    };
                    return Err(M11RecursiveGreenFrameQueryError::BoundExceeded(bound));
                }
            };
        let mut fences = Vec::new();
        fences
            .try_reserve_exact(window.rows.len())
            .map_err(|_| M11RecursiveGreenError::InvalidState)?;
        for row in &window.rows {
            if !expected_kinds.contains(&row.kind)
                || row.edit_capability != M11RecursiveGreenRowEditCapability::Contiguous
            {
                fences.push(None);
                continue;
            }
            let Some(inline_source) = row.editable.clone() else {
                return Err(M11RecursiveGreenError::Corrupt(
                    "contiguous recursive-Green slice row omitted editable bytes",
                )
                .into());
            };
            let Some(inline_source_utf16) = row.editable_utf16.clone() else {
                return Err(M11RecursiveGreenError::Corrupt(
                    "contiguous recursive-Green slice row omitted editable UTF-16",
                )
                .into());
            };
            if inline_source.end.saturating_sub(inline_source.start) > maximum_inline_source_bytes {
                return Err(M11RecursiveGreenFrameQueryError::BoundExceeded(
                    M11RecursiveGreenFrameQueryBound::InlineSourceBytes,
                ));
            }
            let authority = M11ParserSourceRangeAuthority::new(
                runtime,
                self.root.lease()?.duplicate(),
                usize::try_from(inline_source.start)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
                    ..usize::try_from(inline_source.end)
                        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
            )?;
            fences.push(Some(M11RecursiveGreenFrameFence {
                source: self.root.source(),
                frame: row.frame,
                kind: row.kind,
                block_source: row.physical.clone(),
                block_source_utf16: row.physical_utf16.clone(),
                inline_source,
                inline_source_utf16,
                receipt: window.receipt,
                authority,
            }));
        }
        Ok(M11RecursiveGreenRowFenceWindow { window, fences })
    }
}

fn offset_slice_row_window(
    window: &mut M11RecursiveGreenRowWindow,
    byte_base: u64,
    utf16_base: u64,
    row_base: u64,
    open_frame_bases: &[M11RecursiveGreenSliceOpenFrameBase],
) -> Result<(), M11RecursiveGreenError> {
    window.start_ordinal = window
        .start_ordinal
        .checked_add(row_base)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    window.total_rows = window
        .total_rows
        .checked_add(row_base)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    for row in &mut window.rows {
        row.ordinal = row
            .ordinal
            .checked_add(row_base)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        offset_slice_range(&mut row.physical, byte_base)?;
        offset_slice_range(&mut row.physical_utf16, utf16_base)?;
        if let Some(editable) = &mut row.editable {
            offset_slice_range(editable, byte_base)?;
        }
        if let Some(editable_utf16) = &mut row.editable_utf16 {
            offset_slice_range(editable_utf16, utf16_base)?;
        }
        for segment in &mut row.editable_segments {
            offset_slice_range(&mut segment.bytes, byte_base)?;
            offset_slice_range(&mut segment.utf16, utf16_base)?;
        }
        for frame in &mut row.path {
            offset_slice_range(&mut frame.physical, byte_base)?;
            offset_slice_range(&mut frame.physical_utf16, utf16_base)?;
            if let Some(open) = open_frame_bases
                .iter()
                .find(|open| open.frame() == frame.frame)
                .copied()
            {
                frame.physical.start = open.physical_start().bytes();
                frame.physical_utf16.start = open.physical_start().utf16();
                if frame.physical.start > frame.physical.end
                    || frame.physical_utf16.start > frame.physical_utf16.end
                {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "slice open-frame base follows its certified close",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn offset_slice_range(range: &mut Range<u64>, offset: u64) -> Result<(), M11RecursiveGreenError> {
    range.start = range
        .start
        .checked_add(offset)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    range.end = range
        .end
        .checked_add(offset)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    Ok(())
}

pub(super) fn locate_point_in_arena(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    summary: RecursiveGreenSummary,
    point: M11RecursiveGreenPoint,
) -> Result<Option<M11RecursiveGreenLocation>, M11RecursiveGreenError> {
    match locate_point_in_arena_zipper_bounded(arena, tree, summary, point, u64::MAX)? {
        M11RecursiveGreenPointQueryOutcome::Location(location) => Ok(Some(location)),
        M11RecursiveGreenPointQueryOutcome::NotFound => Ok(None),
        M11RecursiveGreenPointQueryOutcome::BudgetExceeded(_) => Err(
            M11RecursiveGreenError::Corrupt("unlimited recursive Green point query exhausted"),
        ),
    }
}

pub(super) fn locate_point_in_arena_bounded(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    summary: RecursiveGreenSummary,
    point: M11RecursiveGreenPoint,
    maximum_tree_nodes_visited: u64,
) -> Result<M11RecursiveGreenPointQueryOutcome, M11RecursiveGreenError> {
    if maximum_tree_nodes_visited == 0 {
        return Err(M11RecursiveGreenError::InvalidState);
    }
    locate_point_in_arena_zipper_bounded(arena, tree, summary, point, maximum_tree_nodes_visited)
}

pub(super) fn locate_renderable_rows_in_arena(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    summary: RecursiveGreenSummary,
    point: M11RecursiveGreenPoint,
    requested_end_byte: u64,
    limits: M11RecursiveGreenRowQueryLimits,
) -> Result<M11RecursiveGreenRowQueryOutcome, M11RecursiveGreenError> {
    let total_rows = summary.renderable_row_exits;
    if total_rows == 0 {
        return Ok(M11RecursiveGreenRowQueryOutcome::Window(
            M11RecursiveGreenRowWindow {
                start_ordinal: 0,
                total_rows: 0,
                complete: true,
                rows: Vec::new(),
                receipt: M11RecursiveGreenQueryReceipt::default(),
            },
        ));
    }
    let mut work = PointZipperWork::with_node_cache();
    let start_location =
        locate_point_in_arena_zipper_prepared(arena, tree, summary, point, &mut work)?.ok_or(
            M11RecursiveGreenError::Corrupt("nonempty renderable Green root has no start location"),
        )?;
    let start_ordinal = start_location.renderable_rows_before.min(total_rows);
    let root_leaf_count = tree_summary(arena, tree, &mut work)?
        .ok_or(M11RecursiveGreenError::Corrupt(
            "renderable Green query lost its root measure",
        ))?
        .leaves();
    // The point lookup has already authenticated the complete open path. If
    // the requested point is inside a renderable row, retain that exact open
    // for the first row instead of rank-selecting the same row again.
    let mut start_row_open = None;
    for candidate in start_location.zipper_open.iter().rev().copied() {
        let boundary =
            point_zipper_frame_boundary(arena, tree, root_leaf_count, candidate, &mut work)?;
        if is_renderable_row_kind(boundary.final_kind) {
            start_row_open = Some(candidate);
            break;
        }
    }
    let mut rows = Vec::new();
    rows.try_reserve_exact(
        usize::try_from(limits.maximum_rows)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
    )
    .map_err(|_| M11RecursiveGreenError::InvalidState)?;
    let mut ordinal = start_ordinal;
    let mut complete = false;
    let mut maximum_open_depth = 0_usize;
    while ordinal < total_rows && rows.len() < limits.maximum_rows as usize {
        let open = if ordinal == start_ordinal {
            match start_row_open {
                Some(open) => open,
                None => point_zipper_open_for_row_ordinal(arena, tree, ordinal, &mut work)?.ok_or(
                    M11RecursiveGreenError::Corrupt("renderable-row rank/select omitted its frame"),
                )?,
            }
        } else {
            point_zipper_open_for_row_ordinal(arena, tree, ordinal, &mut work)?.ok_or(
                M11RecursiveGreenError::Corrupt("renderable-row rank/select omitted its frame"),
            )?
        };
        let boundary = point_zipper_frame_boundary(arena, tree, root_leaf_count, open, &mut work)?;
        if !is_renderable_row_kind(boundary.final_kind) {
            return Err(M11RecursiveGreenError::Corrupt(
                "renderable-row summary selected a non-row frame",
            ));
        }
        let empty_container_row_at_requested_end =
            empty_container_parent_kind(boundary.final_kind.get()).is_some()
                && open.byte_start == requested_end_byte
                && boundary.byte_end == requested_end_byte;
        if open.byte_start >= requested_end_byte
            && ordinal != start_ordinal
            && !empty_container_row_at_requested_end
        {
            complete = true;
            break;
        }
        let editable = point_zipper_row_editable(arena, tree, open, boundary, &mut work)?;
        let mut path = Vec::new();
        if let Some(expected_parent_kind) = empty_container_parent_kind(boundary.final_kind.get()) {
            if open.byte_start != boundary.byte_end
                || open.utf16_start != boundary.utf16_end
                || editable.bytes != Some(open.byte_start..open.byte_start)
                || editable.utf16 != Some(open.utf16_start..open.utf16_start)
            {
                return Err(M11RecursiveGreenError::Corrupt(
                    "empty-container row carried nonempty geometry",
                ));
            }
            let point_byte = usize::try_from(open.byte_start)
                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
            let point_utf16 = usize::try_from(open.utf16_start)
                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
            let location = locate_point_in_arena_zipper_prepared(
                arena,
                tree,
                summary,
                M11RecursiveGreenPoint::new(
                    point_byte,
                    point_utf16,
                    SourceBoundaryAffinity::Before,
                ),
                &mut work,
            )?
            .ok_or(M11RecursiveGreenError::Corrupt(
                "empty-container row has no predecessor ancestry",
            ))?;
            path.try_reserve_exact(location.zipper_open.len() + 1)
                .map_err(|_| M11RecursiveGreenError::InvalidState)?;
            for candidate in location.zipper_open.iter().copied() {
                let candidate_boundary = point_zipper_frame_boundary(
                    arena,
                    tree,
                    root_leaf_count,
                    candidate,
                    &mut work,
                )?;
                path.push(M11RecursiveGreenRowPathFrame {
                    frame: candidate.frame,
                    kind: candidate_boundary.final_kind,
                    physical: candidate.byte_start..candidate_boundary.byte_end,
                    physical_utf16: candidate.utf16_start..candidate_boundary.utf16_end,
                    property: candidate_boundary.final_property,
                    close: candidate_boundary.close,
                });
            }
            if path
                .last()
                .is_none_or(|ancestor| ancestor.kind().get() != expected_parent_kind)
            {
                return Err(M11RecursiveGreenError::Corrupt(
                    "empty-container row has the wrong direct parent",
                ));
            }
            path.push(M11RecursiveGreenRowPathFrame {
                frame: open.frame,
                kind: boundary.final_kind,
                physical: open.byte_start..boundary.byte_end,
                physical_utf16: open.utf16_start..boundary.utf16_end,
                property: boundary.final_property,
                close: boundary.close,
            });
        } else if let Some((anchor_byte, anchor_utf16)) = editable.ancestry_point {
            let can_reuse_start = ordinal == start_ordinal
                && start_row_open.is_some_and(|candidate| candidate.frame == open.frame);
            let queried_location = if can_reuse_start {
                None
            } else {
                let point_byte = usize::try_from(anchor_byte)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
                let point_utf16 = usize::try_from(anchor_utf16)
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
                Some(
                    locate_point_in_arena_zipper_prepared(
                        arena,
                        tree,
                        summary,
                        M11RecursiveGreenPoint::new(
                            point_byte,
                            point_utf16,
                            SourceBoundaryAffinity::After,
                        ),
                        &mut work,
                    )?
                    .ok_or(M11RecursiveGreenError::Corrupt(
                        "renderable row has no editable point ancestry",
                    ))?,
                )
            };
            let location = queried_location.as_ref().unwrap_or(&start_location);
            let row_index = location
                .zipper_open
                .iter()
                .position(|candidate| candidate.frame == open.frame)
                .ok_or(M11RecursiveGreenError::Corrupt(
                    "renderable row is absent from its editable ancestry",
                ))?;
            path.try_reserve_exact(row_index + 1)
                .map_err(|_| M11RecursiveGreenError::InvalidState)?;
            for candidate in location.zipper_open.iter().take(row_index + 1).copied() {
                let candidate_boundary = point_zipper_frame_boundary(
                    arena,
                    tree,
                    root_leaf_count,
                    candidate,
                    &mut work,
                )?;
                path.push(M11RecursiveGreenRowPathFrame {
                    frame: candidate.frame,
                    kind: candidate_boundary.final_kind,
                    physical: candidate.byte_start..candidate_boundary.byte_end,
                    physical_utf16: candidate.utf16_start..candidate_boundary.utf16_end,
                    property: candidate_boundary.final_property,
                    close: candidate_boundary.close,
                });
            }
        } else {
            path.push(M11RecursiveGreenRowPathFrame {
                frame: open.frame,
                kind: boundary.final_kind,
                physical: open.byte_start..boundary.byte_end,
                physical_utf16: open.utf16_start..boundary.utf16_end,
                property: boundary.final_property,
                close: boundary.close,
            });
        }
        rows.push(M11RecursiveGreenRenderableRow {
            ordinal,
            frame: open.frame,
            kind: boundary.final_kind,
            physical: open.byte_start..boundary.byte_end,
            physical_utf16: open.utf16_start..boundary.utf16_end,
            edit_capability: editable.capability,
            editable: editable.bytes,
            editable_utf16: editable.utf16,
            editable_segments: editable.segments,
            path,
        });
        ordinal = ordinal
            .checked_add(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let open_depth = rows.last().map_or(0, |row| row.path.len());
        maximum_open_depth = maximum_open_depth.max(open_depth);
        if let Some(limit) = query_work_exceeded_limit(&work, limits, maximum_open_depth) {
            return Ok(M11RecursiveGreenRowQueryOutcome::BudgetExceeded(
                M11RecursiveGreenRowBudgetExceeded {
                    limit,
                    receipt: work.finish_receipt(maximum_open_depth)?,
                },
            ));
        }
    }
    if ordinal >= total_rows {
        complete = true;
    } else if !complete {
        // Filling the caller's row quantum is not itself evidence that the
        // requested source cut is partial. Peek at exactly the next measured
        // row boundary so an ordinal cut ending in separator bytes can close
        // without inventing a separator row or scanning farther.
        let next = point_zipper_open_for_row_ordinal(arena, tree, ordinal, &mut work)?.ok_or(
            M11RecursiveGreenError::Corrupt("renderable-row successor omitted its frame"),
        )?;
        complete = next.byte_start >= requested_end_byte;
        if let Some(limit) = query_work_exceeded_limit(&work, limits, maximum_open_depth) {
            return Ok(M11RecursiveGreenRowQueryOutcome::BudgetExceeded(
                M11RecursiveGreenRowBudgetExceeded {
                    limit,
                    receipt: work.finish_receipt(maximum_open_depth)?,
                },
            ));
        }
    }
    let receipt = work.finish_receipt(maximum_open_depth)?;
    Ok(M11RecursiveGreenRowQueryOutcome::Window(
        M11RecursiveGreenRowWindow {
            start_ordinal,
            total_rows,
            complete,
            rows,
            receipt,
        },
    ))
}

pub(super) fn locate_renderable_row_ordinal_window_in_arena(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    summary: RecursiveGreenSummary,
    start_ordinal: u64,
    maximum_rows: u32,
) -> Result<M11RecursiveGreenRowOrdinalWindow, M11RecursiveGreenError> {
    let total_rows = summary.renderable_row_exits;
    if maximum_rows == 0 || start_ordinal > total_rows {
        return Err(M11RecursiveGreenError::InvalidPoint);
    }
    let next_ordinal = start_ordinal
        .saturating_add(u64::from(maximum_rows))
        .min(total_rows);
    let mut work = PointZipperWork::default();
    let terminal_cut = (summary.physical_bytes, summary.physical_utf16);
    let start_cut = if start_ordinal == total_rows {
        terminal_cut
    } else {
        let open = point_zipper_open_for_row_ordinal(arena, tree, start_ordinal, &mut work)?
            .ok_or(M11RecursiveGreenError::Corrupt(
                "renderable-row start ordinal omitted its Enter",
            ))?;
        (open.byte_start, open.utf16_start)
    };
    let next_cut = if next_ordinal == start_ordinal {
        start_cut
    } else if next_ordinal == total_rows {
        terminal_cut
    } else {
        let open = point_zipper_open_for_row_ordinal(arena, tree, next_ordinal, &mut work)?.ok_or(
            M11RecursiveGreenError::Corrupt("renderable-row next ordinal omitted its Enter"),
        )?;
        (open.byte_start, open.utf16_start)
    };
    if start_cut.0 > next_cut.0
        || start_cut.1 > next_cut.1
        || (start_ordinal == next_ordinal) != (start_cut == next_cut)
        || (next_ordinal == total_rows) != (next_cut == terminal_cut)
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "renderable-row ordinal cuts disagree with measured coverage",
        ));
    }
    Ok(M11RecursiveGreenRowOrdinalWindow {
        total_rows,
        start_ordinal,
        next_ordinal,
        start_bytes: start_cut.0,
        start_utf16: start_cut.1,
        next_bytes: next_cut.0,
        next_utf16: next_cut.1,
        receipt: work.finish_receipt(0)?,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PointZipperOpenFrame {
    frame: M11RecursiveGreenFrameId,
    kind: M11RecursiveGreenKind,
    enter_leaf: crate::ArenaId,
    enter_leaf_ordinal: u64,
    enter_event_index: usize,
    byte_start: u64,
    utf16_start: u64,
}

struct PreparedPointLocation {
    byte_range: Range<u64>,
    utf16_range: Range<u64>,
    physical: M11RecursiveGreenSourceMetric,
    logical: M11RecursiveGreenSourceMetric,
    part: M11RecursiveGreenCoveragePart,
    atom: M11RecursiveGreenLogicalAtom,
    owner_index: usize,
    zipper_open: Vec<PointZipperOpenFrame>,
    renderable_rows_before: u64,
}

#[derive(Default)]
struct PointZipperWork {
    inspection: SequenceInspectionReceipt,
    decoded: SequenceSpecInspection,
    storage_pages_visited: u64,
    events_scanned: u64,
    decoded_leaves: Vec<(crate::ArenaId, Vec<PackedGreenEvent>)>,
    frame_boundaries: Vec<(PointZipperOpenFrame, PointZipperFrameBoundary)>,
    // Walk-scoped authenticated node memo. `Some` shares each node's
    // decode-and-validate across every descent of one query walk; `None`
    // preserves the historical per-descent decode pattern exactly, which the
    // bounded point queries rely on for their precise header-fuel receipts.
    node_cache: Option<SequenceNodeCache<RecursiveGreenSpec>>,
}

fn tree_summary(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    work: &mut PointZipperWork,
) -> Result<
    Option<crate::measured_sequence::SequenceMeasure<RecursiveGreenSummary>>,
    M11RecursiveGreenError,
> {
    match work.node_cache.as_mut() {
        Some(cache) => tree.summary_with_node_cache(arena, &mut work.inspection, cache),
        None => tree.summary(arena, &mut work.inspection),
    }
}

fn tree_locate_leaf_with_prefix(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    leaf_index: u64,
    work: &mut PointZipperWork,
) -> Result<
    Option<crate::measured_sequence::LocatedSequenceLeaf<RecursiveGreenSummary>>,
    M11RecursiveGreenError,
> {
    match work.node_cache.as_mut() {
        Some(cache) => tree.locate_leaf_with_prefix_with_node_cache(
            arena,
            leaf_index,
            &mut work.inspection,
            cache,
        ),
        None => tree.locate_leaf_with_prefix(arena, leaf_index, &mut work.inspection),
    }
}

fn tree_locate_leaf_containing_metric(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    position: u64,
    metric: impl Fn(RecursiveGreenSummary) -> u64,
    work: &mut PointZipperWork,
) -> Result<
    Option<crate::measured_sequence::LocatedSequenceLeaf<RecursiveGreenSummary>>,
    M11RecursiveGreenError,
> {
    match work.node_cache.as_mut() {
        Some(cache) => tree.locate_leaf_containing_metric_with_node_cache(
            arena,
            position,
            metric,
            &mut work.inspection,
            cache,
        ),
        None => tree.locate_leaf_containing_metric(arena, position, metric, &mut work.inspection),
    }
}

fn tree_locate_leaf_by_monotone_summary(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    range: Range<u64>,
    direction: SequenceSummaryPartitionDirection,
    work: &mut PointZipperWork,
    predicate: impl FnMut(RecursiveGreenSummary) -> Result<bool, M11RecursiveGreenError>,
) -> Result<
    Option<crate::measured_sequence::LocatedSequenceSummaryPartition<RecursiveGreenSummary>>,
    M11RecursiveGreenError,
> {
    match work.node_cache.as_mut() {
        Some(cache) => tree.locate_leaf_by_monotone_summary_with_node_cache(
            arena,
            range,
            direction,
            &mut work.inspection,
            cache,
            predicate,
        ),
        None => tree.locate_leaf_by_monotone_summary(
            arena,
            range,
            direction,
            &mut work.inspection,
            predicate,
        ),
    }
}

impl PointZipperWork {
    fn with_node_cache() -> Self {
        Self {
            node_cache: Some(SequenceNodeCache::new()),
            ..Self::default()
        }
    }

    fn decode_leaf_events(
        &mut self,
        arena: &crate::storage::PageArena,
        id: crate::ArenaId,
    ) -> Result<Vec<PackedGreenEvent>, M11RecursiveGreenError> {
        if let Some((_, events)) = self
            .decoded_leaves
            .iter()
            .find(|(decoded_id, _)| *decoded_id == id)
        {
            return Ok(events.clone());
        }
        self.storage_pages_visited = self
            .storage_pages_visited
            .checked_add(1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let payload = arena.payload(id)?;
        let mut local_inspection = SequenceSpecInspection::default();
        let decoded = decode_leaf(payload, &mut local_inspection)?.ok_or(
            M11RecursiveGreenError::Corrupt("measured Green leaf changed kind"),
        )?;
        accumulate_query_spec_inspection(&mut self.decoded, local_inspection)?;
        let mut events = Vec::new();
        events
            .try_reserve_exact(decoded.events as usize)
            .map_err(|_| M11RecursiveGreenError::InvalidState)?;
        let mut cursor = 0_usize;
        for _ in 0..decoded.events {
            events.push(decode_packed_event(decoded.event_bytes, &mut cursor)?);
            self.events_scanned = self
                .events_scanned
                .checked_add(1)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        }
        if cursor != decoded.event_bytes.len() {
            return Err(M11RecursiveGreenError::Corrupt(
                "recursive-green leaf retained trailing bytes",
            ));
        }
        self.decoded_leaves
            .try_reserve(1)
            .map_err(|_| M11RecursiveGreenError::InvalidState)?;
        self.decoded_leaves.push((id, events.clone()));
        Ok(events)
    }

    fn finish_receipt(
        &self,
        maximum_open_depth: usize,
    ) -> Result<M11RecursiveGreenQueryReceipt, M11RecursiveGreenError> {
        let payload_bytes_inspected = self
            .inspection
            .spec
            .payload_bytes_inspected
            .checked_add(self.decoded.payload_bytes_inspected)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let events_authenticated = self
            .inspection
            .spec
            .spec_items_hashed
            .checked_add(self.decoded.spec_items_hashed)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        Ok(M11RecursiveGreenQueryReceipt {
            node_headers_decoded: self.inspection.node_headers_decoded,
            summary_combinations: self.inspection.summary_combinations,
            payload_bytes_inspected,
            events_authenticated,
            storage_pages_visited: self.storage_pages_visited,
            events_scanned: self.events_scanned,
            maximum_open_depth,
        })
    }
}

fn point_zipper_external_open(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    prefix_end: u64,
    prefix: RecursiveGreenSummary,
    external_closes: u64,
    rank_from_inner: u64,
    work: &mut PointZipperWork,
) -> Result<PointZipperOpenFrame, M11RecursiveGreenError> {
    if prefix_end == 0 {
        return Err(M11RecursiveGreenError::Corrupt(
            "point ancestry predates an empty Green prefix",
        ));
    }
    let threshold = external_closes
        .checked_add(rank_from_inner)
        .and_then(|value| value.checked_add(1))
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    if prefix.unmatched_opens()? < threshold {
        return Err(M11RecursiveGreenError::Corrupt(
            "point ancestry rank exceeds unmatched Green opens",
        ));
    }

    let owner_leaf = tree_locate_leaf_by_monotone_summary(
        arena,
        tree,
        0..prefix_end,
        SequenceSummaryPartitionDirection::Reverse,
        work,
        |suffix| Ok(suffix.unmatched_opens()? >= threshold),
    )?
    .ok_or(M11RecursiveGreenError::Corrupt(
        "summary-guided Green ancestry leaf is absent",
    ))?;
    let suffix = owner_leaf.accumulated;
    let (suffix_opens, suffix_closes) = match suffix {
        Some(summary) => (summary.unmatched_opens()?, summary.unmatched_closes()?),
        None => (0, 0),
    };
    let surviving_suffix_opens = suffix_opens.saturating_sub(external_closes);
    let mut owner_rank_in_leaf = rank_from_inner.checked_sub(surviving_suffix_opens).ok_or(
        M11RecursiveGreenError::Corrupt(
            "summary-guided Green owner leaf skipped the requested open",
        ),
    )?;
    let mut closes_needed = external_closes
        .saturating_sub(suffix_opens)
        .checked_add(suffix_closes)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let events = work.decode_leaf_events(arena, owner_leaf.id)?;
    for (index, event) in events.iter().copied().enumerate().rev() {
        match event {
            PackedGreenEvent::Exit { .. } => {
                closes_needed = closes_needed
                    .checked_add(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            }
            PackedGreenEvent::Enter { .. } if closes_needed != 0 => closes_needed -= 1,
            PackedGreenEvent::Enter { .. } if owner_rank_in_leaf != 0 => {
                owner_rank_in_leaf -= 1;
            }
            PackedGreenEvent::Enter { frame, kind } => {
                let suffix = owner_leaf
                    .accumulated
                    .unwrap_or_else(RecursiveGreenSummary::empty);
                let mut byte_start = prefix
                    .physical_bytes
                    .checked_sub(suffix.physical_bytes)
                    .and_then(|value| value.checked_sub(owner_leaf.summary.physical_bytes))
                    .ok_or(M11RecursiveGreenError::Corrupt(
                        "summary-guided Green Enter byte prefix underflowed",
                    ))?;
                let mut utf16_start = prefix
                    .physical_utf16
                    .checked_sub(suffix.physical_utf16)
                    .and_then(|value| value.checked_sub(owner_leaf.summary.physical_utf16))
                    .ok_or(M11RecursiveGreenError::Corrupt(
                        "summary-guided Green Enter UTF-16 prefix underflowed",
                    ))?;
                for prior in events.iter().take(index) {
                    if let PackedGreenEvent::Coverage { physical, .. } = prior {
                        byte_start = byte_start
                            .checked_add(physical.bytes())
                            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                        utf16_start = utf16_start
                            .checked_add(physical.utf16())
                            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                    }
                }
                return Ok(PointZipperOpenFrame {
                    frame,
                    kind,
                    enter_leaf: owner_leaf.id,
                    enter_leaf_ordinal: owner_leaf.ordinal,
                    enter_event_index: index,
                    byte_start,
                    utf16_start,
                });
            }
            PackedGreenEvent::Property(_)
            | PackedGreenEvent::Coverage { .. }
            | PackedGreenEvent::RetypeOpen { .. } => {}
        }
    }
    Err(M11RecursiveGreenError::Corrupt(
        "summary-guided Green ancestry Enter was not found",
    ))
}

#[derive(Clone, Copy)]
struct PointZipperFrameBoundary {
    final_kind: M11RecursiveGreenKind,
    final_property: Option<super::codec::M11RecursiveGreenPropertyChunk>,
    close: Option<super::codec::M11RecursiveGreenCloseFacts>,
    byte_end: u64,
    utf16_end: u64,
    exit_leaf_ordinal: u64,
    exit_event_index: usize,
}

fn point_zipper_frame_boundary(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    root_leaf_count: u64,
    open: PointZipperOpenFrame,
    work: &mut PointZipperWork,
) -> Result<PointZipperFrameBoundary, M11RecursiveGreenError> {
    if let Some((_, boundary)) = work
        .frame_boundaries
        .iter()
        .find(|(cached_open, _)| *cached_open == open)
    {
        return Ok(*boundary);
    }
    let boundary = point_zipper_frame_boundary_uncached(arena, tree, root_leaf_count, open, work)?;
    work.frame_boundaries
        .try_reserve(1)
        .map_err(|_| M11RecursiveGreenError::InvalidState)?;
    work.frame_boundaries.push((open, boundary));
    Ok(boundary)
}

fn point_zipper_frame_boundary_uncached(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    root_leaf_count: u64,
    open: PointZipperOpenFrame,
    work: &mut PointZipperWork,
) -> Result<PointZipperFrameBoundary, M11RecursiveGreenError> {
    let start_index = open
        .enter_event_index
        .checked_add(1)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let events = work.decode_leaf_events(arena, open.enter_leaf)?;
    if start_index > events.len() {
        return Err(M11RecursiveGreenError::Corrupt(
            "open Green frame event is outside its leaf",
        ));
    }
    let mut relative_depth = 0_i64;
    let mut byte_end = open.byte_start;
    let mut utf16_end = open.utf16_start;
    let mut final_property = None;
    for (index, event) in events.into_iter().enumerate().skip(start_index) {
        match event {
            PackedGreenEvent::Enter { .. } => {
                relative_depth = relative_depth
                    .checked_add(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            }
            PackedGreenEvent::Exit {
                frame,
                final_kind,
                close,
                ..
            } if relative_depth == 0 => {
                if frame != open.frame {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "summary-selected Green Exit differs from its Enter",
                    ));
                }
                return Ok(PointZipperFrameBoundary {
                    final_kind,
                    final_property,
                    close,
                    byte_end,
                    utf16_end,
                    exit_leaf_ordinal: open.enter_leaf_ordinal,
                    exit_event_index: index,
                });
            }
            PackedGreenEvent::Exit { .. } => {
                relative_depth = relative_depth
                    .checked_sub(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                if relative_depth < 0 {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "Green frame continuation depth underflowed",
                    ));
                }
            }
            PackedGreenEvent::Coverage { physical, .. } => {
                byte_end = byte_end
                    .checked_add(physical.bytes())
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                utf16_end = utf16_end
                    .checked_add(physical.utf16())
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            }
            PackedGreenEvent::Property(property) if relative_depth == 0 => {
                final_property = Some(property);
            }
            PackedGreenEvent::RetypeOpen {
                frame, property, ..
            } if relative_depth == 0 => {
                if frame != open.frame {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "Green retype differs from its selected open frame",
                    ));
                }
                final_property = property;
            }
            PackedGreenEvent::Property(_) | PackedGreenEvent::RetypeOpen { .. } => {}
        }
    }

    let range_start = open
        .enter_leaf_ordinal
        .checked_add(1)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    if range_start >= root_leaf_count {
        return Err(M11RecursiveGreenError::Corrupt(
            "open Green frame reaches beyond the final leaf",
        ));
    }
    let exit_leaf = tree_locate_leaf_by_monotone_summary(
        arena,
        tree,
        range_start..root_leaf_count,
        SequenceSummaryPartitionDirection::Forward,
        work,
        |candidate| {
            Ok(relative_depth
                .checked_add(candidate.minimum_prefix)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?
                < 0)
        },
    )?
    .ok_or(M11RecursiveGreenError::Corrupt(
        "open Green frame has no matching Exit",
    ))?;
    let before_exit_leaf = exit_leaf
        .accumulated
        .unwrap_or_else(RecursiveGreenSummary::empty);
    relative_depth = relative_depth
        .checked_add(before_exit_leaf.balance)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    if relative_depth < 0 {
        return Err(M11RecursiveGreenError::Corrupt(
            "Green Exit search crossed before its selected leaf",
        ));
    }
    // The partition search already authenticated the exact summary of every
    // leaf between the Enter leaf and the selected Exit leaf. `byte_end` and
    // `utf16_end` have likewise consumed the remainder of the Enter leaf, so
    // advancing by that accumulated summary produces the selected leaf's
    // absolute source prefix without a second root-to-leaf traversal.
    byte_end = byte_end
        .checked_add(before_exit_leaf.physical_bytes)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    utf16_end = utf16_end
        .checked_add(before_exit_leaf.physical_utf16)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    for (index, event) in work
        .decode_leaf_events(arena, exit_leaf.id)?
        .into_iter()
        .enumerate()
    {
        match event {
            PackedGreenEvent::Enter { .. } => {
                relative_depth = relative_depth
                    .checked_add(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            }
            PackedGreenEvent::Exit {
                frame,
                final_kind,
                close,
                ..
            } if relative_depth == 0 => {
                if frame != open.frame {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "summary-selected Green Exit differs from its Enter",
                    ));
                }
                return Ok(PointZipperFrameBoundary {
                    final_kind,
                    final_property,
                    close,
                    byte_end,
                    utf16_end,
                    exit_leaf_ordinal: exit_leaf.ordinal,
                    exit_event_index: index,
                });
            }
            PackedGreenEvent::Exit { .. } => {
                relative_depth = relative_depth
                    .checked_sub(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                if relative_depth < 0 {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "Green Exit leaf depth underflowed",
                    ));
                }
            }
            PackedGreenEvent::Coverage { physical, .. } => {
                byte_end = byte_end
                    .checked_add(physical.bytes())
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                utf16_end = utf16_end
                    .checked_add(physical.utf16())
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            }
            PackedGreenEvent::Property(property) if relative_depth == 0 => {
                final_property = Some(property);
            }
            PackedGreenEvent::RetypeOpen {
                frame, property, ..
            } if relative_depth == 0 => {
                if frame != open.frame {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "Green retype differs from its selected open frame",
                    ));
                }
                final_property = property;
            }
            PackedGreenEvent::Property(_) | PackedGreenEvent::RetypeOpen { .. } => {}
        }
    }
    Err(M11RecursiveGreenError::Corrupt(
        "summary-selected Green Exit leaf omitted the matching event",
    ))
}

fn point_zipper_final_kind(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    root_leaf_count: u64,
    open: PointZipperOpenFrame,
    work: &mut PointZipperWork,
) -> Result<M11RecursiveGreenKind, M11RecursiveGreenError> {
    Ok(point_zipper_frame_boundary(arena, tree, root_leaf_count, open, work)?.final_kind)
}

fn point_zipper_open_property(
    arena: &crate::storage::PageArena,
    open: PointZipperOpenFrame,
    work: &mut PointZipperWork,
) -> Result<Option<super::codec::M11RecursiveGreenPropertyChunk>, M11RecursiveGreenError> {
    let events = work.decode_leaf_events(arena, open.enter_leaf)?;
    if events.get(open.enter_event_index).is_none_or(
        |event| !matches!(event, PackedGreenEvent::Enter { frame, .. } if *frame == open.frame),
    ) {
        return Err(M11RecursiveGreenError::Corrupt(
            "Green frame property lost its Enter",
        ));
    }
    Ok(match events.get(open.enter_event_index + 1) {
        Some(PackedGreenEvent::Property(property)) => Some(*property),
        _ => None,
    })
}

fn point_zipper_open_for_row_ordinal(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    ordinal: u64,
    work: &mut PointZipperWork,
) -> Result<Option<PointZipperOpenFrame>, M11RecursiveGreenError> {
    let Some(leaf) = tree_locate_leaf_containing_metric(
        arena,
        tree,
        ordinal,
        |summary| summary.renderable_row_exits,
        work,
    )?
    else {
        return Ok(None);
    };
    let prefix = leaf.prefix.unwrap_or_else(RecursiveGreenSummary::empty);
    let target_local =
        ordinal
            .checked_sub(prefix.renderable_row_exits)
            .ok_or(M11RecursiveGreenError::Corrupt(
                "renderable-row ordinal precedes its selected leaf",
            ))?;
    let events = work.decode_leaf_events(arena, leaf.id)?;
    let external_open_at_leaf = u64::try_from(prefix.balance).map_err(|_| {
        M11RecursiveGreenError::Corrupt("renderable-row leaf prefix has negative depth")
    })?;
    let mut local_open = Vec::<PointZipperOpenFrame>::new();
    let mut external_closes = 0_u64;
    let mut local_rows = 0_u64;
    let mut source_bytes = prefix.physical_bytes;
    let mut source_utf16 = prefix.physical_utf16;
    for (index, event) in events.into_iter().enumerate() {
        match event {
            PackedGreenEvent::Enter { frame, kind } => {
                local_open.push(PointZipperOpenFrame {
                    frame,
                    kind,
                    enter_leaf: leaf.id,
                    enter_leaf_ordinal: leaf.ordinal,
                    enter_event_index: index,
                    byte_start: source_bytes,
                    utf16_start: source_utf16,
                });
            }
            PackedGreenEvent::RetypeOpen { frame, kind, .. } => {
                let current = local_open
                    .last_mut()
                    .ok_or(M11RecursiveGreenError::Corrupt(
                        "row-leaf retype has no local frame",
                    ))?;
                if current.frame != frame {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "row-leaf retype differs from its local frame",
                    ));
                }
                current.kind = kind;
            }
            PackedGreenEvent::Exit {
                frame, final_kind, ..
            } => {
                if is_renderable_row_kind(final_kind) && local_rows == target_local {
                    let selected = if let Some(current) = local_open.last().copied() {
                        if current.frame != frame {
                            return Err(M11RecursiveGreenError::Corrupt(
                                "renderable-row Exit differs from its local Enter",
                            ));
                        }
                        current
                    } else {
                        point_zipper_external_open(
                            arena,
                            tree,
                            leaf.ordinal,
                            prefix,
                            external_closes,
                            0,
                            work,
                        )?
                    };
                    if selected.frame != frame {
                        return Err(M11RecursiveGreenError::Corrupt(
                            "renderable-row external Enter differs from its Exit",
                        ));
                    }
                    return Ok(Some(selected));
                }
                if let Some(current) = local_open.pop() {
                    if current.frame != frame || current.kind != final_kind {
                        return Err(M11RecursiveGreenError::Corrupt(
                            "row-leaf Exit differs from its local frame",
                        ));
                    }
                } else {
                    external_closes = external_closes
                        .checked_add(1)
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                    if external_closes > external_open_at_leaf {
                        return Err(M11RecursiveGreenError::Corrupt(
                            "row leaf closes beyond its external depth",
                        ));
                    }
                }
                if is_renderable_row_kind(final_kind) {
                    local_rows = local_rows
                        .checked_add(1)
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                }
            }
            PackedGreenEvent::Coverage { physical, .. } => {
                source_bytes = source_bytes
                    .checked_add(physical.bytes())
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                source_utf16 = source_utf16
                    .checked_add(physical.utf16())
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            }
            PackedGreenEvent::Property(_) => {}
        }
    }
    Err(M11RecursiveGreenError::Corrupt(
        "selected renderable-row leaf omitted its Exit",
    ))
}

struct PointZipperRowEditable {
    capability: M11RecursiveGreenRowEditCapability,
    bytes: Option<Range<u64>>,
    utf16: Option<Range<u64>>,
    segments: Vec<M11RecursiveGreenRowEditableSegment>,
    ancestry_point: Option<(u64, u64)>,
}

fn validate_fenced_close_semantic(
    tag: u16,
    bytes: &[u8],
) -> Result<((u64, u64), (u64, u64)), M11RecursiveGreenError> {
    if tag != 4 || bytes.len() != 49 {
        return Err(M11RecursiveGreenError::Corrupt(
            "fenced-code row carried invalid close facts",
        ));
    }
    let read_metric = |offset: usize| {
        u64::from_le_bytes(
            bytes[offset..offset + 8]
                .try_into()
                .expect("validated fenced-code close-fact width"),
        )
    };
    let info_end = (read_metric(1), read_metric(9));
    let literal_start = (read_metric(17), read_metric(25));
    let logical_end = (read_metric(33), read_metric(41));
    if info_end.0 > literal_start.0
        || info_end.1 > literal_start.1
        || literal_start.0 > logical_end.0
        || literal_start.1 > logical_end.1
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "fenced-code logical projection bounds are reversed",
        ));
    }
    Ok((literal_start, logical_end))
}

fn validate_cached_fenced_close_semantic(bytes: &[u8]) -> Result<(), M11RecursiveGreenError> {
    if bytes.len() != 33 || !matches!(bytes[0], 0 | 1) {
        return Err(M11RecursiveGreenError::Corrupt(
            "fenced-code cached semantic facts are invalid",
        ));
    }
    let read_metric = |offset: usize| {
        u64::from_le_bytes(
            bytes[offset..offset + 8]
                .try_into()
                .expect("validated cached fenced-code semantic width"),
        )
    };
    let info_end = (read_metric(1), read_metric(9));
    let literal_start = (read_metric(17), read_metric(25));
    if info_end.0 > literal_start.0 || info_end.1 > literal_start.1 {
        return Err(M11RecursiveGreenError::Corrupt(
            "fenced-code cached semantic bounds are reversed",
        ));
    }
    Ok(())
}

fn point_zipper_row_editable(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    open: PointZipperOpenFrame,
    boundary: PointZipperFrameBoundary,
    work: &mut PointZipperWork,
) -> Result<PointZipperRowEditable, M11RecursiveGreenError> {
    let cached = if let Some(close) = boundary.close.as_ref() {
        match (boundary.final_kind.get(), close.tag().get()) {
            (7, 4) => {
                if let Some(cached) = close.cached_row_editable(33)? {
                    validate_cached_fenced_close_semantic(cached.0)?;
                    Some(cached)
                } else if let Some(cached) = close.cached_row_editable(1)? {
                    if !matches!(cached.0, [0] | [1]) {
                        return Err(M11RecursiveGreenError::Corrupt(
                            "fenced-code cached geometry has invalid closed flag",
                        ));
                    }
                    Some(cached)
                } else if close.as_bytes().len() == 49 {
                    None
                } else {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "fenced-code row carried invalid close facts",
                    ));
                }
            }
            (7, _) => {
                return Err(M11RecursiveGreenError::Corrupt(
                    "fenced-code row carried invalid close facts",
                ))
            }
            (_, 6) => Some(close.cached_row_editable(0)?.ok_or(
                M11RecursiveGreenError::Corrupt("row carried invalid cached geometry width"),
            )?),
            _ => None,
        }
    } else {
        None
    };
    if let Some((_, cached)) = cached {
        let relative_start = cached.start();
        let relative_end = cached.end();
        let frame_bytes = boundary.byte_end.checked_sub(open.byte_start).ok_or(
            M11RecursiveGreenError::Corrupt("cached row ends before its Enter"),
        )?;
        let frame_utf16 = boundary.utf16_end.checked_sub(open.utf16_start).ok_or(
            M11RecursiveGreenError::Corrupt("cached row UTF-16 ends before its Enter"),
        )?;
        if relative_end.bytes() > frame_bytes || relative_end.utf16() > frame_utf16 {
            return Err(M11RecursiveGreenError::Corrupt(
                "cached row-editable geometry exceeds its frame",
            ));
        }
        let start = open
            .byte_start
            .checked_add(relative_start.bytes())
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let utf16_start = open
            .utf16_start
            .checked_add(relative_start.utf16())
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let end = open
            .byte_start
            .checked_add(relative_end.bytes())
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let utf16_end = open
            .utf16_start
            .checked_add(relative_end.utf16())
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let ancestry_point =
            (start != end || utf16_start != utf16_end).then_some((start, utf16_start));
        if cached.capability() == M11RecursiveGreenCachedRowEditCapability::Contiguous {
            return Ok(PointZipperRowEditable {
                capability: M11RecursiveGreenRowEditCapability::Contiguous,
                bytes: Some(start..end),
                utf16: Some(utf16_start..utf16_end),
                segments: Vec::new(),
                ancestry_point,
            });
        }
        // An unavailable cached scalar cannot distinguish a genuinely
        // non-projectable row from several identity cuts separated by hidden
        // container prefixes. Re-scan that one bounded row and preserve the
        // stronger fact only when every interior gap is HiddenUpstream.
    }
    let fenced_literal = if boundary.final_kind.get() == 7 {
        let close = boundary.close.ok_or(M11RecursiveGreenError::Corrupt(
            "fenced-code row omitted its close facts",
        ))?;
        let bytes = close.as_bytes();
        let (literal_start, logical_end) =
            validate_fenced_close_semantic(close.tag().get(), bytes)?;
        Some((literal_start, logical_end))
    } else {
        None
    };
    let mut relative_depth = 0_usize;
    let mut editable_start = None;
    let mut editable_utf16_start = None;
    let mut editable_end = open.byte_start;
    let mut editable_utf16_end = open.utf16_start;
    let mut logical_bytes = 0_u64;
    let mut logical_utf16 = 0_u64;
    let mut editable_logical_start = None;
    let mut editable_logical_utf16_start = None;
    let mut editable_logical_end = 0_u64;
    let mut editable_logical_utf16_end = 0_u64;
    let mut empty_literal_cut = fenced_literal
        .filter(|(start, _)| *start == (0, 0))
        .map(|_| (open.byte_start, open.utf16_start));
    let mut gap_after = false;
    let mut gap_is_hidden_container = true;
    let mut contiguous = true;
    let mut projected_safe = true;
    let mut editable_segments = Vec::<M11RecursiveGreenRowEditableSegment>::new();
    for leaf_ordinal in open.enter_leaf_ordinal..=boundary.exit_leaf_ordinal {
        let leaf = tree_locate_leaf_with_prefix(arena, tree, leaf_ordinal, work)?.ok_or(
            M11RecursiveGreenError::Corrupt("renderable-row traversal lost a Green leaf"),
        )?;
        let events = work.decode_leaf_events(arena, leaf.id)?;
        let first = if leaf_ordinal == open.enter_leaf_ordinal {
            open.enter_event_index + 1
        } else {
            0
        };
        let last = if leaf_ordinal == boundary.exit_leaf_ordinal {
            boundary.exit_event_index
        } else {
            events.len()
        };
        let prefix = leaf.prefix.unwrap_or_else(RecursiveGreenSummary::empty);
        let mut source_bytes = prefix.physical_bytes;
        let mut source_utf16 = prefix.physical_utf16;
        for event in events.iter().take(first) {
            if let PackedGreenEvent::Coverage { physical, .. } = event {
                source_bytes = source_bytes
                    .checked_add(physical.bytes())
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                source_utf16 = source_utf16
                    .checked_add(physical.utf16())
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            }
        }
        for event in events.into_iter().take(last).skip(first) {
            match event {
                PackedGreenEvent::Enter { .. } => {
                    relative_depth = relative_depth
                        .checked_add(1)
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                }
                PackedGreenEvent::Exit { .. } => {
                    relative_depth =
                        relative_depth
                            .checked_sub(1)
                            .ok_or(M11RecursiveGreenError::Corrupt(
                                "renderable-row child depth underflowed",
                            ))?;
                }
                PackedGreenEvent::Coverage {
                    physical,
                    owner_depth,
                    part,
                    atom,
                } => {
                    let byte_end = source_bytes
                        .checked_add(physical.bytes())
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                    let utf16_end = source_utf16
                        .checked_add(physical.utf16())
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                    let logical = atom.logical_metric(physical);
                    let logical_byte_end = logical_bytes
                        .checked_add(logical.bytes())
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                    let logical_utf16_end = logical_utf16
                        .checked_add(logical.utf16())
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                    let compatible_source = usize::try_from(owner_depth)
                        .ok()
                        .is_some_and(|depth| depth == relative_depth)
                        && part == M11RecursiveGreenCoveragePart::Content
                        && matches!(
                            atom,
                            M11RecursiveGreenLogicalAtom::Identity
                                | M11RecursiveGreenLogicalAtom::LfToLf
                                | M11RecursiveGreenLogicalAtom::CrLfToLf
                                | M11RecursiveGreenLogicalAtom::LoneCrToLf
                        );
                    let compatible = compatible_source
                        && fenced_literal.is_none_or(|(literal_start, logical_end)| {
                            (logical.bytes() != 0 || logical.utf16() != 0)
                                && logical_bytes >= literal_start.0
                                && logical_utf16 >= literal_start.1
                                && logical_byte_end <= logical_end.0
                                && logical_utf16_end <= logical_end.1
                        });
                    if compatible {
                        if gap_after {
                            contiguous = false;
                            projected_safe &= gap_is_hidden_container;
                            editable_segments.push(M11RecursiveGreenRowEditableSegment {
                                bytes: source_bytes..byte_end,
                                utf16: source_utf16..utf16_end,
                            });
                        } else if let Some(segment) = editable_segments.last_mut() {
                            segment.bytes.end = byte_end;
                            segment.utf16.end = utf16_end;
                        } else {
                            editable_segments.push(M11RecursiveGreenRowEditableSegment {
                                bytes: source_bytes..byte_end,
                                utf16: source_utf16..utf16_end,
                            });
                        }
                        editable_start.get_or_insert(source_bytes);
                        editable_utf16_start.get_or_insert(source_utf16);
                        editable_end = byte_end;
                        editable_utf16_end = utf16_end;
                        if fenced_literal.is_some() {
                            editable_logical_start.get_or_insert(logical_bytes);
                            editable_logical_utf16_start.get_or_insert(logical_utf16);
                            editable_logical_end = logical_byte_end;
                            editable_logical_utf16_end = logical_utf16_end;
                        }
                        gap_after = false;
                        gap_is_hidden_container = true;
                    } else if editable_start.is_some() {
                        gap_after = true;
                        gap_is_hidden_container &= atom
                            == M11RecursiveGreenLogicalAtom::HiddenUpstream
                            || (atom == M11RecursiveGreenLogicalAtom::None
                                && part == M11RecursiveGreenCoveragePart::ContainerMarker
                                && usize::try_from(owner_depth)
                                    .ok()
                                    .is_some_and(|depth| depth > relative_depth));
                    }
                    if let Some((literal_start, _)) = fenced_literal {
                        if empty_literal_cut.is_none()
                            && (logical_byte_end, logical_utf16_end) == literal_start
                        {
                            empty_literal_cut = Some((byte_end, utf16_end));
                        }
                    }
                    source_bytes = byte_end;
                    source_utf16 = utf16_end;
                    logical_bytes = logical_byte_end;
                    logical_utf16 = logical_utf16_end;
                }
                PackedGreenEvent::Property(_) | PackedGreenEvent::RetypeOpen { .. } => {}
            }
        }
    }
    let ancestry_point = match (editable_start, editable_utf16_start) {
        (Some(bytes), Some(utf16)) => Some((bytes, utf16)),
        (None, None) => None,
        _ => {
            return Err(M11RecursiveGreenError::Corrupt(
                "renderable-row byte and UTF-16 editable starts differ",
            ));
        }
    };
    if !contiguous {
        if projected_safe && editable_segments.len() > 1 {
            return Ok(PointZipperRowEditable {
                capability: M11RecursiveGreenRowEditCapability::ProjectedReserved,
                bytes: Some(editable_start.expect("segments have a start")..editable_end),
                utf16: Some(
                    editable_utf16_start.expect("segments have a UTF-16 start")..editable_utf16_end,
                ),
                segments: editable_segments,
                ancestry_point,
            });
        }
        return Ok(PointZipperRowEditable {
            capability: M11RecursiveGreenRowEditCapability::Unavailable,
            bytes: None,
            utf16: None,
            segments: Vec::new(),
            ancestry_point,
        });
    }
    if let Some((literal_start, logical_end)) = fenced_literal {
        if (logical_bytes, logical_utf16) != logical_end {
            return Err(M11RecursiveGreenError::Corrupt(
                "fenced-code close facts differ from its logical coverage",
            ));
        }
        if literal_start == logical_end {
            let (byte, utf16) = empty_literal_cut.ok_or(M11RecursiveGreenError::Corrupt(
                "empty fenced-code literal has no exact physical cut",
            ))?;
            return Ok(PointZipperRowEditable {
                capability: M11RecursiveGreenRowEditCapability::Contiguous,
                bytes: Some(byte..byte),
                utf16: Some(utf16..utf16),
                segments: Vec::new(),
                ancestry_point: None,
            });
        }
        if editable_logical_start != Some(literal_start.0)
            || editable_logical_utf16_start != Some(literal_start.1)
            || editable_logical_end != logical_end.0
            || editable_logical_utf16_end != logical_end.1
        {
            return Ok(PointZipperRowEditable {
                capability: M11RecursiveGreenRowEditCapability::Unavailable,
                bytes: None,
                utf16: None,
                segments: Vec::new(),
                ancestry_point,
            });
        }
    }
    let (bytes, utf16) = match ancestry_point {
        Some((start, utf16_start)) => (
            Some(start..editable_end),
            Some(utf16_start..editable_utf16_end),
        ),
        None => (
            Some(open.byte_start..open.byte_start),
            Some(open.utf16_start..open.utf16_start),
        ),
    };
    Ok(PointZipperRowEditable {
        capability: M11RecursiveGreenRowEditCapability::Contiguous,
        bytes,
        utf16,
        segments: Vec::new(),
        ancestry_point,
    })
}

fn query_work_exceeded_limit(
    pending: &PointZipperWork,
    limits: M11RecursiveGreenRowQueryLimits,
    open_depth: usize,
) -> Option<M11RecursiveGreenRowQueryLimit> {
    if pending.storage_pages_visited > limits.maximum_storage_pages_visited {
        Some(M11RecursiveGreenRowQueryLimit::StoragePages)
    } else if pending.events_scanned > limits.maximum_events_scanned {
        Some(M11RecursiveGreenRowQueryLimit::EventsScanned)
    } else if pending.inspection.node_headers_decoded > limits.maximum_tree_nodes_visited {
        Some(M11RecursiveGreenRowQueryLimit::TreeNodes)
    } else if open_depth > limits.maximum_open_depth {
        Some(M11RecursiveGreenRowQueryLimit::OpenDepth)
    } else {
        None
    }
}

fn frame_bound_for_row_query_work(
    pending: &PointZipperWork,
    limits: M11RecursiveGreenRowQueryLimits,
    open_depth: usize,
) -> Option<M11RecursiveGreenFrameQueryBound> {
    match query_work_exceeded_limit(pending, limits, open_depth) {
        Some(M11RecursiveGreenRowQueryLimit::StoragePages) => {
            Some(M11RecursiveGreenFrameQueryBound::StoragePagesVisited)
        }
        Some(M11RecursiveGreenRowQueryLimit::EventsScanned) => {
            Some(M11RecursiveGreenFrameQueryBound::EventsScanned)
        }
        Some(M11RecursiveGreenRowQueryLimit::TreeNodes) => {
            Some(M11RecursiveGreenFrameQueryBound::TreeNodesVisited)
        }
        Some(M11RecursiveGreenRowQueryLimit::OpenDepth) => {
            Some(M11RecursiveGreenFrameQueryBound::OpenDepth)
        }
        None => None,
    }
}

fn locate_point_in_arena_zipper_bounded(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    summary: RecursiveGreenSummary,
    point: M11RecursiveGreenPoint,
    maximum_tree_nodes_visited: u64,
) -> Result<M11RecursiveGreenPointQueryOutcome, M11RecursiveGreenError> {
    let mut work = PointZipperWork {
        inspection: SequenceInspectionReceipt::with_node_header_limit(maximum_tree_nodes_visited)
            .ok_or(M11RecursiveGreenError::InvalidState)?,
        ..PointZipperWork::default()
    };
    let mut maximum_open_depth = 0_usize;
    let resolved: Result<Option<M11RecursiveGreenLocation>, M11RecursiveGreenError> =
        (|| {
            let Some(mut prepared) =
                locate_point_in_arena_zipper_prepared(arena, tree, summary, point, &mut work)?
            else {
                return Ok(None);
            };
            let root_leaf_count = tree
                .summary(arena, &mut work.inspection)?
                .ok_or(M11RecursiveGreenError::Corrupt(
                    "nonempty Green query lost its root measure",
                ))?
                .leaves();
            let is_after_eof = point.affinity == SourceBoundaryAffinity::After
                && u64::try_from(point.byte_offset).ok() == Some(summary.physical_bytes)
                && u64::try_from(point.utf16_offset).ok() == Some(summary.physical_utf16);
            if is_after_eof && prepared.renderable_rows_before < summary.renderable_row_exits {
                let row = point_zipper_open_for_row_ordinal(
                    arena,
                    tree,
                    prepared.renderable_rows_before,
                    &mut work,
                )?
                .ok_or(M11RecursiveGreenError::Corrupt(
                    "EOF renderable-row summary omitted its successor",
                ))?;
                let boundary =
                    point_zipper_frame_boundary(arena, tree, root_leaf_count, row, &mut work)?;
                let expected_parent_kind = empty_container_parent_kind(boundary.final_kind.get());
                if expected_parent_kind.is_some()
                    && row.byte_start == summary.physical_bytes
                    && row.utf16_start == summary.physical_utf16
                    && boundary.byte_end == summary.physical_bytes
                    && boundary.utf16_end == summary.physical_utf16
                {
                    let expected_parent_kind =
                        expected_parent_kind.expect("empty-container kind was checked above");
                    let parent = prepared.zipper_open.last().copied().ok_or(
                        M11RecursiveGreenError::Corrupt(
                            "empty EOF row omitted its container parent",
                        ),
                    )?;
                    let parent_kind =
                        point_zipper_final_kind(arena, tree, root_leaf_count, parent, &mut work)?;
                    if parent_kind.get() != expected_parent_kind {
                        return Err(M11RecursiveGreenError::Corrupt(
                            "empty EOF row has the wrong direct parent",
                        ));
                    }
                    prepared.owner_index = prepared.zipper_open.len();
                    prepared.zipper_open.push(row);
                    prepared.byte_range = summary.physical_bytes..summary.physical_bytes;
                    prepared.utf16_range = summary.physical_utf16..summary.physical_utf16;
                    prepared.physical = M11RecursiveGreenSourceMetric::new(0, 0)
                        .expect("empty source metric is valid");
                    prepared.logical = M11RecursiveGreenSourceMetric::new(0, 0)
                        .expect("empty logical metric is valid");
                    prepared.part = M11RecursiveGreenCoveragePart::Content;
                    prepared.atom = M11RecursiveGreenLogicalAtom::Identity;
                }
            }
            maximum_open_depth = prepared.zipper_open.len();
            let mut ancestry = Vec::new();
            ancestry
                .try_reserve_exact(prepared.zipper_open.len())
                .map_err(|_| M11RecursiveGreenError::InvalidState)?;
            for frame in prepared.zipper_open.iter().copied() {
                let kind = point_zipper_final_kind(arena, tree, root_leaf_count, frame, &mut work)?;
                ancestry.push(M11RecursiveGreenAncestor {
                    frame: frame.frame,
                    kind,
                });
            }
            let receipt = work.finish_receipt(ancestry.len())?;
            Ok(Some(M11RecursiveGreenLocation {
                byte_range: prepared.byte_range,
                utf16_range: prepared.utf16_range,
                physical: prepared.physical,
                logical: prepared.logical,
                part: prepared.part,
                atom: prepared.atom,
                owner_index: prepared.owner_index,
                ancestry,
                receipt,
                zipper_open: prepared.zipper_open,
                renderable_rows_before: prepared.renderable_rows_before,
            }))
        })();

    // Measured-sequence fuel exhaustion deliberately uses the spec-invalid
    // sentinel internally. Intercept it before it can escape as corruption.
    if work.inspection.node_header_limit_exhausted() {
        return Ok(M11RecursiveGreenPointQueryOutcome::BudgetExceeded(
            M11RecursiveGreenPointBudgetExceeded {
                receipt: work.finish_receipt(maximum_open_depth)?,
            },
        ));
    }

    match resolved? {
        Some(location) => Ok(M11RecursiveGreenPointQueryOutcome::Location(location)),
        None => Ok(M11RecursiveGreenPointQueryOutcome::NotFound),
    }
}

fn locate_point_in_arena_zipper_prepared(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    summary: RecursiveGreenSummary,
    point: M11RecursiveGreenPoint,
    work: &mut PointZipperWork,
) -> Result<Option<PreparedPointLocation>, M11RecursiveGreenError> {
    let point_bytes =
        u64::try_from(point.byte_offset).map_err(|_| M11RecursiveGreenError::InvalidPoint)?;
    let point_utf16 =
        u64::try_from(point.utf16_offset).map_err(|_| M11RecursiveGreenError::InvalidPoint)?;
    if point_bytes > summary.physical_bytes || point_utf16 > summary.physical_utf16 {
        return Err(M11RecursiveGreenError::InvalidPoint);
    }
    if summary.physical_bytes == 0 {
        return Ok(None);
    }
    let effective_byte = match (point.affinity, point.byte_offset) {
        (SourceBoundaryAffinity::Before, offset) if offset > 0 => offset - 1,
        (_, offset) if u64::try_from(offset).ok() == Some(summary.physical_bytes) => offset - 1,
        (_, offset) => offset,
    };
    let effective_byte =
        u64::try_from(effective_byte).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let effective_utf16 = match (point.affinity, point_utf16) {
        (SourceBoundaryAffinity::Before, offset) if offset > 0 => offset - 1,
        (_, offset) if offset == summary.physical_utf16 => offset - 1,
        (_, offset) => offset,
    };

    let point_leaf = tree_locate_leaf_containing_metric(
        arena,
        tree,
        effective_byte,
        |summary| summary.physical_bytes,
        work,
    )?
    .ok_or(M11RecursiveGreenError::Corrupt(
        "recursive-green point has no coverage leaf",
    ))?;
    let point_prefix = point_leaf
        .prefix
        .unwrap_or_else(RecursiveGreenSummary::empty);
    let external_open_at_leaf = u64::try_from(point_prefix.balance).map_err(|_| {
        M11RecursiveGreenError::Corrupt("recursive-green point prefix has negative depth")
    })?;
    let events = work.decode_leaf_events(arena, point_leaf.id)?;
    let mut local_open = Vec::<PointZipperOpenFrame>::new();
    local_open
        .try_reserve_exact(32)
        .map_err(|_| M11RecursiveGreenError::InvalidState)?;
    let mut external_closes = 0_u64;
    let mut source_bytes = point_prefix.physical_bytes;
    let mut source_utf16 = point_prefix.physical_utf16;
    let mut renderable_rows_before = point_prefix.renderable_row_exits;
    let mut selected = None;
    for (index, event) in events.into_iter().enumerate() {
        match event {
            PackedGreenEvent::Enter { frame, kind } => {
                local_open.push(PointZipperOpenFrame {
                    frame,
                    kind,
                    enter_leaf: point_leaf.id,
                    enter_leaf_ordinal: point_leaf.ordinal,
                    enter_event_index: index,
                    byte_start: source_bytes,
                    utf16_start: source_utf16,
                });
            }
            PackedGreenEvent::RetypeOpen { frame, kind, .. } => {
                if let Some(current) = local_open.last_mut() {
                    if current.frame != frame {
                        return Err(M11RecursiveGreenError::Corrupt(
                            "point-leaf retype differs from its open frame",
                        ));
                    }
                    current.kind = kind;
                }
            }
            PackedGreenEvent::Exit {
                frame, final_kind, ..
            } => {
                if let Some(current) = local_open.pop() {
                    if current.frame != frame || current.kind != final_kind {
                        return Err(M11RecursiveGreenError::Corrupt(
                            "point-leaf Exit differs from its open frame",
                        ));
                    }
                } else {
                    external_closes = external_closes
                        .checked_add(1)
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                    if external_closes > external_open_at_leaf {
                        return Err(M11RecursiveGreenError::Corrupt(
                            "point leaf closes beyond its external depth",
                        ));
                    }
                }
                if is_renderable_row_kind(final_kind) {
                    renderable_rows_before = renderable_rows_before
                        .checked_add(1)
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                }
            }
            PackedGreenEvent::Coverage {
                physical,
                owner_depth,
                part,
                atom,
            } => {
                let byte_end = source_bytes
                    .checked_add(physical.bytes())
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                let utf16_end = source_utf16
                    .checked_add(physical.utf16())
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                if effective_byte >= source_bytes && effective_byte < byte_end {
                    let external_remaining =
                        external_open_at_leaf.checked_sub(external_closes).ok_or(
                            M11RecursiveGreenError::Corrupt("point external depth underflowed"),
                        )?;
                    let total_depth = usize::try_from(external_remaining)
                        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
                        .checked_add(local_open.len())
                        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                    let owner_depth = usize::try_from(owner_depth)
                        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
                    if total_depth == 0 || owner_depth >= total_depth {
                        return Err(M11RecursiveGreenError::Corrupt(
                            "point coverage owner is outside its ancestry",
                        ));
                    }
                    selected = Some((
                        source_bytes..byte_end,
                        source_utf16..utf16_end,
                        physical,
                        atom.logical_metric(physical),
                        part,
                        atom,
                        owner_depth,
                        external_remaining,
                        local_open.clone(),
                    ));
                    break;
                }
                source_bytes = byte_end;
                source_utf16 = utf16_end;
            }
            PackedGreenEvent::Property(_) => {}
        }
    }
    let Some((
        byte_range,
        utf16_range,
        physical,
        logical,
        part,
        atom,
        owner_depth,
        external_remaining,
        local_open,
    )) = selected
    else {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive-green point leaf has no selected coverage",
        ));
    };
    if effective_utf16 < utf16_range.start || effective_utf16 >= utf16_range.end {
        return Err(M11RecursiveGreenError::InvalidPoint);
    }

    let total_depth = usize::try_from(external_remaining)
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        .checked_add(local_open.len())
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let mut open = Vec::<PointZipperOpenFrame>::new();
    open.try_reserve_exact(total_depth)
        .map_err(|_| M11RecursiveGreenError::InvalidState)?;
    for rank_from_inner in (0..external_remaining).rev() {
        open.push(point_zipper_external_open(
            arena,
            tree,
            point_leaf.ordinal,
            point_prefix,
            external_closes,
            rank_from_inner,
            work,
        )?);
    }
    open.extend(local_open);
    if open.len() != total_depth {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive-green zipper ancestry depth changed",
        ));
    }
    let owner_index =
        open.len()
            .checked_sub(owner_depth + 1)
            .ok_or(M11RecursiveGreenError::Corrupt(
                "recursive-green zipper owner index underflowed",
            ))?;
    Ok(Some(PreparedPointLocation {
        byte_range,
        utf16_range,
        physical,
        logical,
        part,
        atom,
        owner_index,
        zipper_open: open,
        renderable_rows_before,
    }))
}

#[cfg(test)]
pub(super) fn locate_point_in_arena_linear(
    arena: &crate::storage::PageArena,
    tree: MeasuredSequenceRef<'_, RecursiveGreenSpec>,
    summary: RecursiveGreenSummary,
    point: M11RecursiveGreenPoint,
) -> Result<Option<M11RecursiveGreenLocation>, M11RecursiveGreenError> {
    let point_bytes =
        u64::try_from(point.byte_offset).map_err(|_| M11RecursiveGreenError::InvalidPoint)?;
    let point_utf16 =
        u64::try_from(point.utf16_offset).map_err(|_| M11RecursiveGreenError::InvalidPoint)?;
    if point_bytes > summary.physical_bytes || point_utf16 > summary.physical_utf16 {
        return Err(M11RecursiveGreenError::InvalidPoint);
    }
    if summary.physical_bytes == 0 {
        return Ok(None);
    }
    let effective_byte = match (point.affinity, point.byte_offset) {
        (SourceBoundaryAffinity::Before, offset) if offset > 0 => offset - 1,
        (_, offset) if u64::try_from(offset).ok() == Some(summary.physical_bytes) => offset - 1,
        (_, offset) => offset,
    };
    let effective_byte =
        u64::try_from(effective_byte).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let mut inspection = SequenceInspectionReceipt::default();
    let mut callback_inspection = SequenceSpecInspection::default();
    let mut open = Vec::<QueryOpenFrame>::new();
    open.try_reserve_exact(64)
        .map_err(|_| M11RecursiveGreenError::InvalidState)?;
    let mut location: Option<PendingLocation> = None;
    let mut source_bytes = 0_u64;
    let mut source_utf16 = 0_u64;
    let mut events_scanned = 0_u64;
    let mut storage_pages_visited = 0_u64;
    let mut maximum_open_depth = 0_usize;

    tree.visit_leaves_from_metric(
        arena,
        0,
        |summary| summary.events,
        &mut inspection,
        |leaf| {
            storage_pages_visited = storage_pages_visited
                .checked_add(1)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            let payload = arena.payload(leaf.id)?;
            let mut local_inspection = SequenceSpecInspection::default();
            let decoded = decode_leaf(payload, &mut local_inspection)?.ok_or(
                M11RecursiveGreenError::Corrupt("measured Green leaf changed kind"),
            )?;
            callback_inspection.payload_bytes_inspected = callback_inspection
                .payload_bytes_inspected
                .checked_add(local_inspection.payload_bytes_inspected)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            callback_inspection.spec_items_hashed = callback_inspection
                .spec_items_hashed
                .checked_add(local_inspection.spec_items_hashed)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            let mut cursor = 0;
            for _ in 0..decoded.events {
                let event = decode_packed_event(decoded.event_bytes, &mut cursor)?;
                events_scanned = events_scanned
                    .checked_add(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                match event {
                    PackedGreenEvent::Enter { frame, kind } => {
                        open.push(QueryOpenFrame { frame, kind });
                        maximum_open_depth = maximum_open_depth.max(open.len());
                    }
                    PackedGreenEvent::RetypeOpen { frame, kind, .. } => {
                        let current = open
                            .last_mut()
                            .ok_or(M11RecursiveGreenError::Corrupt("retype has no open frame"))?;
                        if current.frame != frame {
                            return Err(M11RecursiveGreenError::Corrupt(
                                "retype does not target the open top",
                            ));
                        }
                        current.kind = kind;
                        update_captured_kind(&mut location, frame, kind);
                    }
                    PackedGreenEvent::Exit {
                        frame, final_kind, ..
                    } => {
                        let current = open
                            .pop()
                            .ok_or(M11RecursiveGreenError::Corrupt("exit has no open frame"))?;
                        if current.frame != frame || current.kind != final_kind {
                            return Err(M11RecursiveGreenError::Corrupt(
                                "exit differs from its open frame",
                            ));
                        }
                        update_captured_kind(&mut location, frame, final_kind);
                    }
                    PackedGreenEvent::Coverage {
                        physical,
                        owner_depth,
                        part,
                        atom,
                    } => {
                        let byte_end = source_bytes
                            .checked_add(physical.bytes())
                            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                        let utf16_end = source_utf16
                            .checked_add(physical.utf16())
                            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                        if location.is_none()
                            && effective_byte >= source_bytes
                            && effective_byte < byte_end
                        {
                            let depth = usize::try_from(owner_depth)
                                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
                            let owner_index = open.len().checked_sub(depth + 1).ok_or(
                                M11RecursiveGreenError::Corrupt(
                                    "coverage owner is outside the open path",
                                ),
                            )?;
                            let mut ancestry = Vec::new();
                            ancestry
                                .try_reserve_exact(open.len())
                                .map_err(|_| M11RecursiveGreenError::InvalidState)?;
                            ancestry.extend(open.iter().map(|frame| M11RecursiveGreenAncestor {
                                frame: frame.frame,
                                kind: frame.kind,
                            }));
                            location = Some(PendingLocation {
                                byte_range: source_bytes..byte_end,
                                utf16_range: source_utf16..utf16_end,
                                physical,
                                logical: atom.logical_metric(physical),
                                part,
                                atom,
                                owner_index,
                                ancestry,
                            });
                        }
                        source_bytes = byte_end;
                        source_utf16 = utf16_end;
                    }
                    PackedGreenEvent::Property(_) => {}
                }
            }
            if cursor != decoded.event_bytes.len() {
                return Err(M11RecursiveGreenError::Corrupt(
                    "recursive-green leaf retained trailing bytes",
                ));
            }
            Ok(SequenceLeafVisitControl::Continue)
        },
    )?;
    inspection.spec.payload_bytes_inspected = inspection
        .spec
        .payload_bytes_inspected
        .checked_add(callback_inspection.payload_bytes_inspected)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    inspection.spec.spec_items_hashed = inspection
        .spec
        .spec_items_hashed
        .checked_add(callback_inspection.spec_items_hashed)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    if !open.is_empty()
        || source_bytes != summary.physical_bytes
        || source_utf16 != summary.physical_utf16
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive-green query traversal changed the root summary",
        ));
    }
    let Some(location) = location else {
        return Err(M11RecursiveGreenError::Corrupt(
            "recursive-green point has no coverage event",
        ));
    };
    let effective_utf16 = match (point.affinity, point_utf16) {
        (SourceBoundaryAffinity::Before, offset) if offset > 0 => offset - 1,
        (_, offset) if offset == summary.physical_utf16 => offset - 1,
        (_, offset) => offset,
    };
    if effective_utf16 < location.utf16_range.start || effective_utf16 >= location.utf16_range.end {
        return Err(M11RecursiveGreenError::InvalidPoint);
    }
    let receipt = M11RecursiveGreenQueryReceipt {
        node_headers_decoded: inspection.node_headers_decoded,
        summary_combinations: inspection.summary_combinations,
        payload_bytes_inspected: inspection.spec.payload_bytes_inspected,
        events_authenticated: inspection.spec.spec_items_hashed,
        storage_pages_visited,
        events_scanned,
        maximum_open_depth,
    };
    Ok(Some(M11RecursiveGreenLocation {
        byte_range: location.byte_range,
        utf16_range: location.utf16_range,
        physical: location.physical,
        logical: location.logical,
        part: location.part,
        atom: location.atom,
        owner_index: location.owner_index,
        ancestry: location.ancestry,
        receipt,
        zipper_open: Vec::new(),
        renderable_rows_before: 0,
    }))
}

fn accumulate_query_spec_inspection(
    total: &mut SequenceSpecInspection,
    addition: SequenceSpecInspection,
) -> Result<(), M11RecursiveGreenError> {
    total.payload_bytes_inspected = total
        .payload_bytes_inspected
        .checked_add(addition.payload_bytes_inspected)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    total.spec_items_hashed = total
        .spec_items_hashed
        .checked_add(addition.spec_items_hashed)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    Ok(())
}

fn update_captured_kind(
    location: &mut Option<PendingLocation>,
    frame: M11RecursiveGreenFrameId,
    kind: M11RecursiveGreenKind,
) {
    let Some(location) = location else {
        return;
    };
    if let Some(ancestor) = location
        .ancestry
        .iter_mut()
        .find(|ancestor| ancestor.frame == frame)
    {
        ancestor.kind = kind;
    }
}

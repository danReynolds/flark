//! Source- and event-authenticated structural replacement of committed Green.
//!
//! Unlike the single-coverage fast path, this operation may replace an
//! arbitrary balanced sequence of block events.  It still touches only the
//! two event-boundary leaves, the replacement payload, and logarithmic AVL
//! paths.  Unchanged prefix and suffix leaves remain the exact committed arena
//! objects from the base root.

use std::fmt;
use std::ops::Range;

use crate::document::{DocumentRuntime, ExactUnchangedPrefixWitness, ExactUnchangedSuffixWitness};
use crate::identity::RuntimeIdentity;
use crate::measured_sequence::{
    begin_measured_sequence_seal, splice_measured_sequence_atomic,
    splice_measured_sequence_build_root_atomic, SequenceInspectionReceipt, SequenceMutationReceipt,
    SequenceSummaryPartitionDirection,
};
use crate::source::SourceSnapshotLease;
use crate::storage::PageArena;
use crate::ArenaId;

use super::build::{
    allocate_recursive_green_identity, M11RecursiveGreenBuildReceipt, M11RecursiveGreenRoot,
};
use super::codec::{
    decode_leaf, decode_packed_event, is_renderable_row_kind, packed_event_len,
    packed_event_summary, M11RecursiveGreenError, M11RecursiveGreenEvent, M11RecursiveGreenFrameId,
    M11RecursiveGreenKind, PackedGreenEvent, RecursiveGreenSpec, RecursiveGreenSummary,
};

/// One frame on a parser/Green-certified restart boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenBoundaryFrame {
    frame: M11RecursiveGreenFrameId,
    kind: M11RecursiveGreenKind,
}

impl M11RecursiveGreenBoundaryFrame {
    #[must_use]
    pub const fn frame(self) -> M11RecursiveGreenFrameId {
        self.frame
    }

    #[must_use]
    pub const fn kind(self) -> M11RecursiveGreenKind {
        self.kind
    }
}

/// Move-only authority for one quiescent event/source/open-path cut.
///
/// The identity is allocated with the Green build and inherited by its sealed
/// root. A same-source checkpoint from a different build therefore cannot be
/// crossed into an adoption transaction.
#[must_use = "a structural Green boundary must be consumed by adoption or discarded"]
pub struct M11RecursiveGreenStructuralBoundary {
    runtime_identity: RuntimeIdentity,
    green_identity: RuntimeIdentity,
    source: crate::SourceVersion,
    event_cut: u64,
    physical: super::codec::M11RecursiveGreenSourceMetric,
    logical: super::codec::M11RecursiveGreenSourceMetric,
    open: Box<[M11RecursiveGreenBoundaryFrame]>,
}

/// Parser-internal replica of one move-only boundary, scoped to exactly one
/// adoption transaction.
///
/// This is deliberately not `Clone`: the trusted parser-internal consumer can
/// ask an original committed boundary to mint a replica for a private
/// transaction, and that transaction must consume the replica with the same
/// identity before it can splice. The feature-gated parser seam is an explicit
/// trust boundary; this wrapper prevents accidental general cloning rather
/// than defending against a deliberately repeated internal mint request.
#[doc(hidden)]
#[must_use = "a structural boundary transaction replica must be consumed or discarded"]
pub struct M11RecursiveGreenStructuralBoundaryTransactionReplica {
    transaction_id: u64,
    boundary: M11RecursiveGreenStructuralBoundary,
}

impl fmt::Debug for M11RecursiveGreenStructuralBoundary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("M11RecursiveGreenStructuralBoundary")
            .field("source", &self.source)
            .field("event_cut", &self.event_cut)
            .field("physical", &self.physical)
            .field("logical", &self.logical)
            .field("open", &self.open)
            .finish_non_exhaustive()
    }
}

impl M11RecursiveGreenStructuralBoundary {
    pub(super) fn from_build(
        runtime_identity: RuntimeIdentity,
        green_identity: RuntimeIdentity,
        source: crate::SourceVersion,
        event_cut: u64,
        physical: super::codec::M11RecursiveGreenSourceMetric,
        logical: super::codec::M11RecursiveGreenSourceMetric,
        open: impl ExactSizeIterator<Item = (M11RecursiveGreenFrameId, M11RecursiveGreenKind)>,
    ) -> Result<Self, M11RecursiveGreenError> {
        let mut frames = Vec::new();
        frames
            .try_reserve_exact(open.len())
            .map_err(|_| M11RecursiveGreenError::InvalidState)?;
        frames.extend(open.map(|(frame, kind)| M11RecursiveGreenBoundaryFrame { frame, kind }));
        if frames.is_empty() {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        Ok(Self {
            runtime_identity,
            green_identity,
            source,
            event_cut,
            physical,
            logical,
            open: frames.into_boxed_slice(),
        })
    }

    #[must_use]
    pub const fn source(&self) -> crate::SourceVersion {
        self.source
    }

    #[must_use]
    pub const fn event_cut(&self) -> u64 {
        self.event_cut
    }

    #[must_use]
    pub const fn physical_metric(&self) -> super::codec::M11RecursiveGreenSourceMetric {
        self.physical
    }

    #[must_use]
    pub const fn logical_metric(&self) -> super::codec::M11RecursiveGreenSourceMetric {
        self.logical
    }

    #[must_use]
    pub fn open_path(&self) -> &[M11RecursiveGreenBoundaryFrame] {
        &self.open
    }

    /// Mints one parser-transaction replica without making committed Green
    /// boundary authority generally cloneable.
    #[doc(hidden)]
    pub fn replicate_for_parser_transaction(
        &self,
        transaction_id: u64,
    ) -> Result<M11RecursiveGreenStructuralBoundaryTransactionReplica, M11RecursiveGreenError> {
        if transaction_id == 0 {
            return Err(M11RecursiveGreenError::InvalidState);
        }
        let mut open = Vec::new();
        open.try_reserve_exact(self.open.len())
            .map_err(|_| M11RecursiveGreenError::InvalidState)?;
        open.extend_from_slice(&self.open);
        Ok(M11RecursiveGreenStructuralBoundaryTransactionReplica {
            transaction_id,
            boundary: Self {
                runtime_identity: self.runtime_identity,
                green_identity: self.green_identity,
                source: self.source,
                event_cut: self.event_cut,
                physical: self.physical,
                logical: self.logical,
                open: open.into_boxed_slice(),
            },
        })
    }

    pub(super) fn rebound(
        runtime_identity: RuntimeIdentity,
        green_identity: RuntimeIdentity,
        source: crate::SourceVersion,
        event_cut: u64,
        physical: super::codec::M11RecursiveGreenSourceMetric,
        logical: super::codec::M11RecursiveGreenSourceMetric,
        open: Box<[M11RecursiveGreenBoundaryFrame]>,
    ) -> Self {
        Self {
            runtime_identity,
            green_identity,
            source,
            event_cut,
            physical,
            logical,
            open,
        }
    }
}

impl M11RecursiveGreenStructuralBoundaryTransactionReplica {
    /// Recovers move-only boundary authority only for the transaction that
    /// minted this replica.
    #[doc(hidden)]
    pub fn into_boundary_for_parser_transaction(
        self,
        transaction_id: u64,
    ) -> Result<M11RecursiveGreenStructuralBoundary, M11RecursiveGreenError> {
        if transaction_id == 0 || self.transaction_id != transaction_id {
            return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
        }
        Ok(self.boundary)
    }
}

/// Move-only proof that one authenticated structural splice preserved the
/// exact Green prefix and suffix surrounding its replacement.
///
/// The capability is minted only by
/// [`splice_m11_recursive_green_structural_atomic`]. It can rebind an existing
/// base boundary to the new root without accepting caller-supplied coordinate
/// deltas or Green identities.
#[doc(hidden)]
#[must_use = "structural splice rebase authority must be used or discarded"]
pub struct M11RecursiveGreenStructuralSpliceRebase {
    runtime_identity: RuntimeIdentity,
    base_green_identity: RuntimeIdentity,
    target_green_identity: RuntimeIdentity,
    base_source: crate::SourceVersion,
    target_source: crate::SourceVersion,
    base_event_start: u64,
    base_event_end: u64,
    target_event_end: u64,
    base_physical_start: super::codec::M11RecursiveGreenSourceMetric,
    base_physical_end: super::codec::M11RecursiveGreenSourceMetric,
    target_physical_end: super::codec::M11RecursiveGreenSourceMetric,
    base_logical_start: super::codec::M11RecursiveGreenSourceMetric,
    base_logical_end: super::codec::M11RecursiveGreenSourceMetric,
    target_logical_end: super::codec::M11RecursiveGreenSourceMetric,
    target_event_count: u64,
    target_physical_total: super::codec::M11RecursiveGreenSourceMetric,
    target_logical_total: super::codec::M11RecursiveGreenSourceMetric,
}

impl M11RecursiveGreenStructuralSpliceRebase {
    /// Consumes one boundary strictly inside the splice's unchanged prefix and
    /// rebinds it to the target Green identity. Prefix coordinates are exact
    /// identities because the splice lineage proved that source unchanged.
    pub fn rebase_prefix(
        &self,
        mut boundary: M11RecursiveGreenStructuralBoundary,
    ) -> Result<M11RecursiveGreenStructuralBoundary, M11RecursiveGreenError> {
        self.validate_base_boundary(&boundary)?;
        if boundary.event_cut >= self.base_event_start
            || boundary.physical.bytes() > self.base_physical_start.bytes()
            || boundary.physical.utf16() > self.base_physical_start.utf16()
            || boundary.logical.bytes() > self.base_logical_start.bytes()
            || boundary.logical.utf16() > self.base_logical_start.utf16()
        {
            return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
        }
        self.validate_target_coordinates(boundary.event_cut, boundary.physical, boundary.logical)?;
        boundary.green_identity = self.target_green_identity;
        boundary.source = self.target_source;
        Ok(boundary)
    }

    /// Consumes one boundary strictly inside the splice's unchanged suffix and
    /// rebinds it using the authenticated event, physical, and logical deltas
    /// at convergence.
    pub fn rebase_suffix(
        &self,
        mut boundary: M11RecursiveGreenStructuralBoundary,
    ) -> Result<M11RecursiveGreenStructuralBoundary, M11RecursiveGreenError> {
        self.validate_base_boundary(&boundary)?;
        if boundary.event_cut <= self.base_event_end
            || boundary.physical.bytes() < self.base_physical_end.bytes()
            || boundary.physical.utf16() < self.base_physical_end.utf16()
            || boundary.logical.bytes() < self.base_logical_end.bytes()
            || boundary.logical.utf16() < self.base_logical_end.utf16()
        {
            return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
        }
        let event_cut = translate_cut(
            boundary.event_cut,
            self.base_event_end,
            self.target_event_end,
        )?;
        let physical = translate_metric(
            boundary.physical,
            self.base_physical_end,
            self.target_physical_end,
        )?;
        let logical = translate_metric(
            boundary.logical,
            self.base_logical_end,
            self.target_logical_end,
        )?;
        self.validate_target_coordinates(event_cut, physical, logical)?;
        boundary.green_identity = self.target_green_identity;
        boundary.source = self.target_source;
        boundary.event_cut = event_cut;
        boundary.physical = physical;
        boundary.logical = logical;
        Ok(boundary)
    }

    fn validate_base_boundary(
        &self,
        boundary: &M11RecursiveGreenStructuralBoundary,
    ) -> Result<(), M11RecursiveGreenError> {
        if boundary.runtime_identity != self.runtime_identity
            || boundary.green_identity != self.base_green_identity
            || boundary.source != self.base_source
        {
            return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
        }
        Ok(())
    }

    fn validate_target_coordinates(
        &self,
        event_cut: u64,
        physical: super::codec::M11RecursiveGreenSourceMetric,
        logical: super::codec::M11RecursiveGreenSourceMetric,
    ) -> Result<(), M11RecursiveGreenError> {
        if event_cut > self.target_event_count
            || physical.bytes() > self.target_physical_total.bytes()
            || physical.utf16() > self.target_physical_total.utf16()
            || logical.bytes() > self.target_logical_total.bytes()
            || logical.utf16() > self.target_logical_total.utf16()
        {
            return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
        }
        Ok(())
    }
}

fn translate_cut(value: u64, base: u64, target: u64) -> Result<u64, M11RecursiveGreenError> {
    if target >= base {
        value
            .checked_add(target - base)
            .ok_or(M11RecursiveGreenError::CounterOverflow)
    } else {
        value
            .checked_sub(base - target)
            .ok_or(M11RecursiveGreenError::CounterOverflow)
    }
}

fn translate_metric(
    value: super::codec::M11RecursiveGreenSourceMetric,
    base: super::codec::M11RecursiveGreenSourceMetric,
    target: super::codec::M11RecursiveGreenSourceMetric,
) -> Result<super::codec::M11RecursiveGreenSourceMetric, M11RecursiveGreenError> {
    super::codec::M11RecursiveGreenSourceMetric::new(
        translate_cut(value.bytes(), base.bytes(), target.bytes())?,
        translate_cut(value.utf16(), base.utf16(), target.utf16())?,
    )
    .ok_or(M11RecursiveGreenError::CounterOverflow)
}
use super::splice::{
    abort_seal_after_failure, add_inspection, build_replacement_pages, derive_coverage_atoms,
    metric_between, release_tree_after_failure, validate_lineage,
};

/// One complete changed packed-leaf event segment in an exact Green delta.
///
/// A segment names the base events removed from publication and the target
/// events replacing them. The owning selection authenticates ordering and the
/// unchanged event gaps between segments.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenStructuralSpliceSegment {
    base_event_start: u64,
    base_event_end: u64,
    target_event_start: u64,
    target_event_end: u64,
}

impl M11RecursiveGreenStructuralSpliceSegment {
    pub fn new(
        base_event_range: Range<u64>,
        target_event_range: Range<u64>,
    ) -> Result<Self, M11RecursiveGreenError> {
        if base_event_range.start >= base_event_range.end
            || target_event_range.start >= target_event_range.end
        {
            return Err(M11RecursiveGreenError::InvalidPoint);
        }
        Ok(Self {
            base_event_start: base_event_range.start,
            base_event_end: base_event_range.end,
            target_event_start: target_event_range.start,
            target_event_end: target_event_range.end,
        })
    }

    #[must_use]
    pub fn base_event_range(&self) -> Range<u64> {
        self.base_event_start..self.base_event_end
    }

    #[must_use]
    pub fn target_event_range(&self) -> Range<u64> {
        self.target_event_start..self.target_event_end
    }
}

/// Parser-selected complete changed packed-leaf segments for one exact
/// structural Green splice.
///
/// The segments are not authority by themselves. They name the exact events
/// removed from the retained base and the exact replacement events in the
/// locally authenticated target. The first segment is the primary parser
/// splice; later segments are distinct far leaves repaired for frames which
/// remain open at convergence. Segments are sorted, nonoverlapping, and every
/// unchanged gap has the same event length in base and target coordinates.
/// Publication must revalidate them against both sealed roots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenStructuralSpliceSelection {
    segments: Box<[M11RecursiveGreenStructuralSpliceSegment]>,
}

impl M11RecursiveGreenStructuralSpliceSelection {
    /// Binds one base and target packed-leaf range at the same event cut.
    pub fn new(
        base_event_range: Range<u64>,
        target_event_range: Range<u64>,
    ) -> Result<Self, M11RecursiveGreenError> {
        let mut segments = Vec::new();
        segments
            .try_reserve_exact(1)
            .map_err(|_| M11RecursiveGreenError::InvalidState)?;
        segments.push(M11RecursiveGreenStructuralSpliceSegment::new(
            base_event_range,
            target_event_range,
        )?);
        Self::from_segments(segments.into_boxed_slice())
    }

    /// Validates a complete ordered sequence of changed packed-leaf ranges.
    ///
    /// The first base and target cuts must match. Every later target start is
    /// derived from the preceding target end plus the unchanged base gap,
    /// which carries the cumulative event delta without signed arithmetic.
    pub fn from_segments(
        segments: Box<[M11RecursiveGreenStructuralSpliceSegment]>,
    ) -> Result<Self, M11RecursiveGreenError> {
        let Some(first) = segments.first() else {
            return Err(M11RecursiveGreenError::InvalidPoint);
        };
        if first.base_event_start != first.target_event_start {
            return Err(M11RecursiveGreenError::InvalidPoint);
        }
        for pair in segments.windows(2) {
            let previous = pair[0];
            let next = pair[1];
            if next.base_event_start < previous.base_event_end
                || next.target_event_start < previous.target_event_end
            {
                return Err(M11RecursiveGreenError::InvalidPoint);
            }
            let base_gap = next
                .base_event_start
                .checked_sub(previous.base_event_end)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            let expected_target_start = previous
                .target_event_end
                .checked_add(base_gap)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            if next.target_event_start != expected_target_start {
                return Err(M11RecursiveGreenError::InvalidPoint);
            }
        }
        Ok(Self { segments })
    }

    #[must_use]
    pub fn segments(&self) -> &[M11RecursiveGreenStructuralSpliceSegment] {
        &self.segments
    }

    /// Fallibly duplicates the bounded semantic selection without invoking an
    /// infallible `Box<[T]>` allocation at a transport boundary.
    pub fn try_clone(&self) -> Result<Self, M11RecursiveGreenError> {
        let mut segments = Vec::new();
        segments
            .try_reserve_exact(self.segments.len())
            .map_err(|_| {
                M11RecursiveGreenError::Arena(crate::storage::ArenaError::AllocationFailed)
            })?;
        segments.extend_from_slice(&self.segments);
        Ok(Self {
            segments: segments.into_boxed_slice(),
        })
    }
}

#[cfg(test)]
mod structural_splice_selection_tests {
    use super::{
        M11RecursiveGreenStructuralSpliceSegment, M11RecursiveGreenStructuralSpliceSelection,
    };

    fn segment(
        base: std::ops::Range<u64>,
        target: std::ops::Range<u64>,
    ) -> M11RecursiveGreenStructuralSpliceSegment {
        M11RecursiveGreenStructuralSpliceSegment::new(base, target).expect("valid segment")
    }

    #[test]
    fn multi_range_selection_carries_growth_and_shrinkage_across_unchanged_gaps() {
        let selection = M11RecursiveGreenStructuralSpliceSelection::from_segments(
            vec![
                segment(10..14, 10..16),
                segment(20..25, 22..27),
                segment(30..33, 32..35),
            ]
            .into_boxed_slice(),
        )
        .expect("ordered sparse selection");

        assert_eq!(selection.segments().len(), 3);
        assert_eq!(selection.segments()[1].base_event_range(), 20..25);
        assert_eq!(selection.segments()[1].target_event_range(), 22..27);
        assert_eq!(selection.try_clone().expect("fallible clone"), selection);

        let shrinking = M11RecursiveGreenStructuralSpliceSelection::from_segments(
            vec![segment(10..16, 10..14), segment(20..25, 18..23)].into_boxed_slice(),
        )
        .expect("ordered sparse shrinking selection");
        assert_eq!(shrinking.segments()[1].target_event_range(), 18..23);
    }

    #[test]
    fn multi_range_selection_rejects_overlap_and_forged_target_coordinates() {
        for segments in [
            vec![segment(10..14, 10..16), segment(13..18, 15..20)],
            vec![segment(10..14, 10..16), segment(20..25, 21..26)],
            vec![segment(10..14, 10..16), segment(20..25, 23..28)],
        ] {
            assert!(M11RecursiveGreenStructuralSpliceSelection::from_segments(
                segments.into_boxed_slice(),
            )
            .is_err());
        }
    }
}

/// Exact bounded work performed by one structural Green adoption.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenStructuralSpliceReceipt {
    selection: M11RecursiveGreenStructuralSpliceSelection,
    base_events: u64,
    deleted_events: u64,
    replacement_events: u64,
    unchanged_events_preserved: u64,
    boundary_events_decoded: u64,
    boundary_events_reencoded: u64,
    base_storage_pages: u64,
    deleted_storage_pages: u64,
    replacement_storage_pages: u64,
    reused_storage_pages: u64,
    node_headers_decoded: u64,
    summary_combinations: u64,
    payload_bytes_inspected: u64,
    events_authenticated: u64,
    tree_nodes_visited: usize,
    branches_allocated: usize,
    maximum_atomic_height: u16,
    seal_transitions: usize,
    lineage_transitions: usize,
    base_maximum_frame_id: u64,
    target_maximum_frame_id: u64,
}

impl M11RecursiveGreenStructuralSpliceReceipt {
    /// Exact semantic event ranges selected by the successful splice.
    #[must_use]
    pub const fn selection(&self) -> &M11RecursiveGreenStructuralSpliceSelection {
        &self.selection
    }

    #[must_use]
    pub const fn base_events(&self) -> u64 {
        self.base_events
    }
    #[must_use]
    pub const fn deleted_events(&self) -> u64 {
        self.deleted_events
    }
    #[must_use]
    pub const fn replacement_events(&self) -> u64 {
        self.replacement_events
    }
    #[must_use]
    pub const fn unchanged_events_preserved(&self) -> u64 {
        self.unchanged_events_preserved
    }
    #[must_use]
    pub const fn boundary_events_decoded(&self) -> u64 {
        self.boundary_events_decoded
    }
    #[must_use]
    pub const fn boundary_events_reencoded(&self) -> u64 {
        self.boundary_events_reencoded
    }
    #[must_use]
    pub const fn base_storage_pages(&self) -> u64 {
        self.base_storage_pages
    }
    #[must_use]
    pub const fn deleted_storage_pages(&self) -> u64 {
        self.deleted_storage_pages
    }
    #[must_use]
    pub const fn replacement_storage_pages(&self) -> u64 {
        self.replacement_storage_pages
    }
    #[must_use]
    pub const fn reused_storage_pages(&self) -> u64 {
        self.reused_storage_pages
    }
    #[must_use]
    pub const fn node_headers_decoded(&self) -> u64 {
        self.node_headers_decoded
    }
    #[must_use]
    pub const fn summary_combinations(&self) -> u64 {
        self.summary_combinations
    }
    #[must_use]
    pub const fn payload_bytes_inspected(&self) -> u64 {
        self.payload_bytes_inspected
    }
    #[must_use]
    pub const fn events_authenticated(&self) -> u64 {
        self.events_authenticated
    }
    #[must_use]
    pub const fn tree_nodes_visited(&self) -> usize {
        self.tree_nodes_visited
    }
    #[must_use]
    pub const fn branches_allocated(&self) -> usize {
        self.branches_allocated
    }
    #[must_use]
    pub const fn maximum_atomic_height(&self) -> u16 {
        self.maximum_atomic_height
    }
    #[must_use]
    pub const fn seal_transitions(&self) -> usize {
        self.seal_transitions
    }
    #[must_use]
    pub const fn lineage_transitions(&self) -> usize {
        self.lineage_transitions
    }
    #[must_use]
    pub const fn base_maximum_frame_id(&self) -> u64 {
        self.base_maximum_frame_id
    }
    #[must_use]
    pub const fn target_maximum_frame_id(&self) -> u64 {
        self.target_maximum_frame_id
    }
}

struct StructuralSplicePlan {
    storage_range: Range<u64>,
    events: Vec<PackedGreenEvent>,
    deleted_summary: RecursiveGreenSummary,
    boundary_events_decoded: u64,
    prefix_events_retained: usize,
    boundary_events_retained: u64,
    inspection: SequenceInspectionReceipt,
    end_leaf_ordinal: u64,
    end_event_index: usize,
}

/// One bounded repair to an Exit retained beyond a structural convergence cut.
///
/// A local replacement can change cached state owned by a frame which stays
/// open at convergence. The matching Exit remains in the authenticated suffix,
/// so the splice repairs that one event by summary-guided relative depth rather
/// than scanning or repacking the intervening suffix.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M11RecursiveGreenSpanningExitRepair {
    /// Translate an existing frame-relative cached row trailer by the exact
    /// geometry delta observed at convergence. All grammar-owned close bytes
    /// and close-state bits remain unchanged.
    TranslateCachedRow {
        frame: M11RecursiveGreenFrameId,
        base_convergence_end: super::codec::M11RecursiveGreenSourceMetric,
        target_convergence_end: super::codec::M11RecursiveGreenSourceMetric,
    },
    /// Replace the complete Exit state with parser/writer-certified target
    /// facts, as used at the pre-Document-close terminal boundary.
    Exact {
        frame: M11RecursiveGreenFrameId,
        final_kind: M11RecursiveGreenKind,
        close: Option<super::codec::M11RecursiveGreenCloseFacts>,
        last_line_blank: bool,
        child: super::codec::M11RecursiveGreenClosedChild,
    },
}

impl M11RecursiveGreenSpanningExitRepair {
    const fn frame(self) -> M11RecursiveGreenFrameId {
        match self {
            Self::TranslateCachedRow { frame, .. } | Self::Exact { frame, .. } => frame,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct LocatedSpanningExitRepair {
    leaf_ordinal: u64,
    event_index: usize,
    replacement: PackedGreenEvent,
}

struct FarSpanningExitRepairLeaf {
    base_leaf_ordinal: u64,
    base_event_start: u64,
    events: Vec<PackedGreenEvent>,
}

/// Replaces one exact balanced event interval and adopts its unchanged suffix.
///
/// The two move-only boundaries must have been minted by the exact Green build
/// inherited by `base`; their open paths must be identical. `events` must
/// cover the target interval between the unchanged start and
/// `target_end_physical`, leave that authenticated path unchanged, and may
/// mint frames only above the base root's persisted maximum. This makes
/// inserted-frame identity independent of the unchanged suffix's historical
/// numbering.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn splice_m11_recursive_green_structural_atomic(
    runtime: &mut DocumentRuntime,
    base: &M11RecursiveGreenRoot,
    target_lease: SourceSnapshotLease,
    prefix: Option<ExactUnchangedPrefixWitness>,
    suffix: Option<ExactUnchangedSuffixWitness>,
    start: M11RecursiveGreenStructuralBoundary,
    end: M11RecursiveGreenStructuralBoundary,
    target_end_physical: super::codec::M11RecursiveGreenSourceMetric,
    events: &[M11RecursiveGreenEvent],
) -> Result<
    (
        M11RecursiveGreenRoot,
        M11RecursiveGreenStructuralSpliceReceipt,
        M11RecursiveGreenStructuralBoundary,
        M11RecursiveGreenStructuralBoundary,
        M11RecursiveGreenStructuralSpliceRebase,
    ),
    M11RecursiveGreenError,
> {
    splice_m11_recursive_green_structural_with_spanning_exit_repairs_atomic(
        runtime,
        base,
        target_lease,
        prefix,
        suffix,
        start,
        end,
        target_end_physical,
        events,
        &[],
    )
}

/// Replaces one balanced event interval and repairs the bounded set of
/// retained Exits belonging to frames still open at convergence.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn splice_m11_recursive_green_structural_with_spanning_exit_repairs_atomic(
    runtime: &mut DocumentRuntime,
    base: &M11RecursiveGreenRoot,
    target_lease: SourceSnapshotLease,
    prefix: Option<ExactUnchangedPrefixWitness>,
    suffix: Option<ExactUnchangedSuffixWitness>,
    start: M11RecursiveGreenStructuralBoundary,
    end: M11RecursiveGreenStructuralBoundary,
    target_end_physical: super::codec::M11RecursiveGreenSourceMetric,
    events: &[M11RecursiveGreenEvent],
    spanning_exit_repairs: &[M11RecursiveGreenSpanningExitRepair],
) -> Result<
    (
        M11RecursiveGreenRoot,
        M11RecursiveGreenStructuralSpliceReceipt,
        M11RecursiveGreenStructuralBoundary,
        M11RecursiveGreenStructuralBoundary,
        M11RecursiveGreenStructuralSpliceRebase,
    ),
    M11RecursiveGreenError,
> {
    base.ensure_storage_live(runtime)?;
    let target_source = target_lease.version();
    if runtime.current_source_version() != Some(target_source) {
        return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
    }
    let base_source = base.source();
    if start.runtime_identity != runtime.producer_identity()
        || end.runtime_identity != runtime.producer_identity()
        || start.green_identity != base.green_identity
        || end.green_identity != base.green_identity
        || start.source != base_source
        || end.source != base_source
        || start.event_cut >= end.event_cut
        || end.event_cut > base.event_count()
        || start.physical.bytes() > end.physical.bytes()
        || start.physical.utf16() > end.physical.utf16()
        || start.logical.bytes() > end.logical.bytes()
        || start.logical.utf16() > end.logical.utf16()
        || end.open.len() > start.open.len()
        || start.open[..end.open.len()] != end.open[..]
    {
        return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
    }
    let base_event_range = start.event_cut..end.event_cut;
    let base_byte_range = usize::try_from(start.physical.bytes())
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        ..usize::try_from(end.physical.bytes())
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let target_byte_range = usize::try_from(start.physical.bytes())
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        ..usize::try_from(target_end_physical.bytes())
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let external_open_depth = start.open.len();
    let external_closes = external_open_depth
        .checked_sub(end.open.len())
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    if base_event_range.start >= base_event_range.end
        || base_event_range.end > base.event_count()
        || base_byte_range.start > base_byte_range.end
        || base_byte_range.end > base_source.byte_len()
        || target_byte_range.start > target_byte_range.end
        || target_byte_range.end > target_source.byte_len()
        || external_open_depth == 0
        || events.is_empty()
    {
        return Err(M11RecursiveGreenError::InvalidPoint);
    }

    let base_lease = base.lease()?;
    let base_utf16_start = base_lease.utf16_offset_for_byte(base_byte_range.start)?;
    let base_utf16_end = base_lease.utf16_offset_for_byte(base_byte_range.end)?;
    let target_utf16_start = target_lease.utf16_offset_for_byte(target_byte_range.start)?;
    let target_utf16_end = target_lease.utf16_offset_for_byte(target_byte_range.end)?;
    if base_utf16_start
        != usize::try_from(start.physical.utf16())
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        || base_utf16_end
            != usize::try_from(end.physical.utf16())
                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        || target_utf16_start
            != usize::try_from(start.physical.utf16())
                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        || target_utf16_end
            != usize::try_from(target_end_physical.utf16())
                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
    {
        return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
    }
    let lineage_transitions = validate_lineage(
        runtime,
        base_source,
        target_source,
        prefix,
        suffix,
        &base_byte_range,
        &target_byte_range,
        base_utf16_start,
        base_utf16_end,
        target_utf16_start,
        target_utf16_end,
    )?;

    let tree = base
        .tree
        .as_ref()
        .ok_or(M11RecursiveGreenError::InvalidState)?;
    let mut plan = plan_structural_splice(
        runtime.producer_arena(),
        tree,
        &base_event_range,
        start.physical,
        start.logical,
        end.physical,
        end.logical,
    )?;
    let located_exit_repairs = plan_spanning_exit_repairs(
        runtime.producer_arena(),
        tree,
        base,
        target_source,
        &end,
        target_end_physical,
        spanning_exit_repairs,
        &mut plan.inspection,
    )?;
    if plan.deleted_summary.unmatched_closes()?
        != u64::try_from(external_closes).map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        || plan.deleted_summary.unmatched_opens()? != 0
        || plan.deleted_summary.oldest_open.is_some()
    {
        return Err(M11RecursiveGreenError::InvalidPoint);
    }

    let (replacement, replacement_summary) = pack_structural_fragment(
        &target_lease,
        &target_byte_range,
        &start.open,
        &end.open,
        base.maximum_frame_id(),
        events,
    )?;
    if replacement_summary.physical_bytes
        != u64::try_from(target_byte_range.end - target_byte_range.start)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        || replacement_summary.physical_utf16
            != u64::try_from(target_utf16_end - target_utf16_start)
                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        || replacement_summary.unmatched_closes()?
            != u64::try_from(external_closes)
                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        || replacement_summary.unmatched_opens()? != 0
        || replacement_summary.oldest_open.is_some()
    {
        return Err(M11RecursiveGreenError::IncompleteCoverage);
    }

    let replacement_events = replacement_summary.events;
    let boundary_events_retained = plan.boundary_events_retained;
    let boundary_events_reencoded = plan
        .boundary_events_retained
        .checked_add(replacement_events)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let transport_event_start = base_event_range
        .start
        .checked_sub(
            u64::try_from(plan.prefix_events_retained)
                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
        )
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let base_transport_event_end = transport_event_start
        .checked_add(base_event_range.end - base_event_range.start)
        .and_then(|end| end.checked_add(boundary_events_retained))
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let target_transport_event_end = transport_event_start
        .checked_add(replacement_events)
        .and_then(|end| end.checked_add(boundary_events_retained))
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    plan.events.splice(
        plan.prefix_events_retained..plan.prefix_events_retained,
        replacement,
    );

    let mut far_repair_leaves = Vec::<FarSpanningExitRepairLeaf>::new();
    let mut repair_cursor = 0_usize;
    while repair_cursor < located_exit_repairs.len() {
        let leaf_ordinal = located_exit_repairs[repair_cursor].leaf_ordinal;
        let group_end = located_exit_repairs[repair_cursor..]
            .iter()
            .position(|repair| repair.leaf_ordinal != leaf_ordinal)
            .map_or(located_exit_repairs.len(), |offset| repair_cursor + offset);
        if leaf_ordinal == plan.end_leaf_ordinal {
            for repair in &located_exit_repairs[repair_cursor..group_end] {
                let suffix_index = repair
                    .event_index
                    .checked_sub(plan.end_event_index)
                    .ok_or(M11RecursiveGreenError::InvalidEvent)?;
                let event_index = plan
                    .prefix_events_retained
                    .checked_add(replacement_events as usize)
                    .and_then(|index| index.checked_add(suffix_index))
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                let event = plan
                    .events
                    .get_mut(event_index)
                    .ok_or(M11RecursiveGreenError::InvalidEvent)?;
                *event = repair.replacement;
            }
        } else {
            if leaf_ordinal < plan.storage_range.end {
                return Err(M11RecursiveGreenError::InvalidEvent);
            }
            let located = tree
                .as_ref()
                .locate_leaf_with_prefix(
                    runtime.producer_arena(),
                    leaf_ordinal,
                    &mut plan.inspection,
                )?
                .ok_or(M11RecursiveGreenError::InvalidPoint)?;
            let mut events =
                decode_events(runtime.producer_arena(), located.id, &mut plan.inspection)?;
            for repair in &located_exit_repairs[repair_cursor..group_end] {
                let event = events
                    .get_mut(repair.event_index)
                    .ok_or(M11RecursiveGreenError::InvalidEvent)?;
                *event = repair.replacement;
            }
            far_repair_leaves.push(FarSpanningExitRepairLeaf {
                base_leaf_ordinal: leaf_ordinal,
                base_event_start: located.prefix.map_or(0, |summary| summary.events),
                events,
            });
        }
        repair_cursor = group_end;
    }

    let mut selected_segments = Vec::new();
    selected_segments
        .try_reserve_exact(
            1_usize
                .checked_add(far_repair_leaves.len())
                .ok_or(M11RecursiveGreenError::CounterOverflow)?,
        )
        .map_err(|_| M11RecursiveGreenError::InvalidState)?;
    selected_segments.push(M11RecursiveGreenStructuralSpliceSegment::new(
        transport_event_start..base_transport_event_end,
        transport_event_start..target_transport_event_end,
    )?);
    for far in &far_repair_leaves {
        let event_count =
            u64::try_from(far.events.len()).map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
        let base_event_end = far
            .base_event_start
            .checked_add(event_count)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let unchanged_gap = far
            .base_event_start
            .checked_sub(base_transport_event_end)
            .ok_or(M11RecursiveGreenError::InvalidEvent)?;
        let target_event_start = target_transport_event_end
            .checked_add(unchanged_gap)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let target_event_end = target_event_start
            .checked_add(event_count)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        selected_segments.push(M11RecursiveGreenStructuralSpliceSegment::new(
            far.base_event_start..base_event_end,
            target_event_start..target_event_end,
        )?);
    }
    let selection = M11RecursiveGreenStructuralSpliceSelection::from_segments(
        selected_segments.into_boxed_slice(),
    )?;

    let mut mutation = SequenceMutationReceipt::default();
    add_inspection(&mut mutation.inspection, plan.inspection)?;
    let mut session = runtime.producer_arena_mut().begin_build()?;
    let replacement_root = build_replacement_pages(&mut session, &plan.events, &mut mutation)?;
    let main_replacement_storage_pages = u64::try_from(mutation.leaves_adopted)
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let mut root = splice_measured_sequence_atomic::<RecursiveGreenSpec>(
        &mut session,
        tree,
        plan.storage_range.clone(),
        Some(replacement_root),
        &mut mutation,
    )?
    .ok_or(M11RecursiveGreenError::Corrupt(
        "structural Green splice produced an empty root",
    ))?;
    let removed_main_pages = plan
        .storage_range
        .end
        .checked_sub(plan.storage_range.start)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    for far in far_repair_leaves {
        let adopted_before = mutation.leaves_adopted;
        let replacement = build_replacement_pages(&mut session, &far.events, &mut mutation)?;
        if mutation.leaves_adopted != adopted_before.saturating_add(1) {
            return Err(M11RecursiveGreenError::InvalidEvent);
        }
        let target_leaf_ordinal = far
            .base_leaf_ordinal
            .checked_sub(removed_main_pages)
            .and_then(|ordinal| ordinal.checked_add(main_replacement_storage_pages))
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        root = splice_measured_sequence_build_root_atomic::<RecursiveGreenSpec>(
            &mut session,
            root,
            target_leaf_ordinal
                ..target_leaf_ordinal
                    .checked_add(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?,
            Some(replacement),
            &mut mutation,
        )?
        .ok_or(M11RecursiveGreenError::Corrupt(
            "spanning Exit repair produced an empty root",
        ))?;
    }
    let deleted_storage_pages = removed_main_pages
        .checked_add(
            u64::try_from(
                located_exit_repairs
                    .iter()
                    .filter(|repair| repair.leaf_ordinal != plan.end_leaf_ordinal)
                    .map(|repair| repair.leaf_ordinal)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
            )
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
        )
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let replacement_storage_pages = u64::try_from(mutation.leaves_adopted)
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    mutation.leaves_reused = usize::try_from(
        base.storage_page_count()
            .checked_sub(deleted_storage_pages)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?,
    )
    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let build = session.suspend()?;
    let mut seal = match begin_measured_sequence_seal(runtime.producer_arena_mut(), build, root) {
        Ok(seal) => seal,
        Err(failure) => {
            let error = failure.error;
            let _root = failure.root;
            runtime.producer_arena_mut().abort_build(failure.build)?;
            return Err(error.into());
        }
    };
    let mut seal_transitions = 0_usize;
    let target_tree = loop {
        let poll = match seal.poll(runtime.producer_arena_mut(), 1) {
            Ok(poll) => poll,
            Err(error) => {
                abort_seal_after_failure(runtime, seal);
                return Err(error.into());
            }
        };
        let Some(next_seal_transitions) = seal_transitions.checked_add(poll.transitions) else {
            abort_seal_after_failure(runtime, seal);
            return Err(M11RecursiveGreenError::CounterOverflow);
        };
        seal_transitions = next_seal_transitions;
        if let Some(tree) = poll.root {
            break tree;
        }
    };

    let mut final_inspection = SequenceInspectionReceipt::default();
    let measure = match target_tree
        .as_ref()
        .summary(runtime.producer_arena(), &mut final_inspection)
    {
        Ok(Some(measure)) => measure,
        Ok(None) => {
            release_tree_after_failure(runtime, target_tree);
            return Err(M11RecursiveGreenError::Corrupt(
                "structural Green splice sealed an empty root",
            ));
        }
        Err(error) => {
            release_tree_after_failure(runtime, target_tree);
            return Err(error);
        }
    };
    if let Err(error) = add_inspection(&mut mutation.inspection, final_inspection) {
        release_tree_after_failure(runtime, target_tree);
        return Err(error);
    }
    let summary = measure.summary();
    let expected_events = base
        .event_count()
        .checked_sub(base_event_range.end - base_event_range.start)
        .and_then(|value| value.checked_add(replacement_events))
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    if summary.physical_bytes
        != u64::try_from(target_source.byte_len())
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        || summary.physical_utf16
            != u64::try_from(target_source.utf16_len())
                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        || summary.events != expected_events
        || summary.balance != 0
        || summary.minimum_prefix != 0
        || summary.oldest_open.is_some()
    {
        release_tree_after_failure(runtime, target_tree);
        return Err(M11RecursiveGreenError::IncompleteCoverage);
    }

    let target_event_cut = start
        .event_cut
        .checked_add(replacement_events)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let receipt = match make_structural_receipt(
        base,
        selection,
        base_event_range.end - base_event_range.start,
        replacement_events,
        plan.boundary_events_decoded,
        boundary_events_reencoded,
        deleted_storage_pages,
        replacement_storage_pages,
        mutation,
        seal_transitions,
        lineage_transitions,
        summary.max_frame_id,
    ) {
        Ok(receipt) => receipt,
        Err(error) => {
            release_tree_after_failure(runtime, target_tree);
            return Err(error);
        }
    };
    let build_receipt =
        M11RecursiveGreenBuildReceipt::from_mutation(0, summary, mutation, seal_transitions);
    let target_green_identity = allocate_recursive_green_identity()?;
    let target_logical = super::codec::M11RecursiveGreenSourceMetric::new(
        start
            .logical
            .bytes()
            .checked_add(replacement_summary.logical_bytes)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?,
        start
            .logical
            .utf16()
            .checked_add(replacement_summary.logical_utf16)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?,
    )
    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let rebase = M11RecursiveGreenStructuralSpliceRebase {
        runtime_identity: runtime.producer_identity(),
        base_green_identity: start.green_identity,
        target_green_identity,
        base_source,
        target_source,
        base_event_start: start.event_cut,
        base_event_end: end.event_cut,
        target_event_end: target_event_cut,
        base_physical_start: start.physical,
        base_physical_end: end.physical,
        target_physical_end: target_end_physical,
        base_logical_start: start.logical,
        base_logical_end: end.logical,
        target_logical_end: target_logical,
        target_event_count: summary.events,
        target_physical_total: super::codec::M11RecursiveGreenSourceMetric::from_validated(
            summary.physical_bytes,
            summary.physical_utf16,
        ),
        target_logical_total: super::codec::M11RecursiveGreenSourceMetric::from_validated(
            summary.logical_bytes,
            summary.logical_utf16,
        ),
    };
    let target_start_boundary = M11RecursiveGreenStructuralBoundary::rebound(
        runtime.producer_identity(),
        target_green_identity,
        target_source,
        start.event_cut,
        start.physical,
        start.logical,
        start.open.to_vec().into_boxed_slice(),
    );
    let target_end_boundary = M11RecursiveGreenStructuralBoundary::rebound(
        runtime.producer_identity(),
        target_green_identity,
        target_source,
        target_event_cut,
        target_end_physical,
        target_logical,
        end.open,
    );
    Ok((
        M11RecursiveGreenRoot::from_splice(
            runtime.producer_identity(),
            target_green_identity,
            target_lease,
            summary,
            measure.leaves(),
            measure.height(),
            target_tree,
            build_receipt,
        ),
        receipt,
        target_start_boundary,
        target_end_boundary,
        rebase,
    ))
}

fn plan_structural_splice(
    arena: &PageArena,
    tree: &super::build::GreenSequenceTree,
    event_range: &Range<u64>,
    start_physical: super::codec::M11RecursiveGreenSourceMetric,
    start_logical: super::codec::M11RecursiveGreenSourceMetric,
    end_physical: super::codec::M11RecursiveGreenSourceMetric,
    end_logical: super::codec::M11RecursiveGreenSourceMetric,
) -> Result<StructuralSplicePlan, M11RecursiveGreenError> {
    let mut inspection = SequenceInspectionReceipt::default();
    let start = tree
        .as_ref()
        .locate_leaf_containing_metric(
            arena,
            event_range.start,
            |summary| summary.events,
            &mut inspection,
        )?
        .ok_or(M11RecursiveGreenError::InvalidPoint)?;
    let end_position = event_range
        .end
        .checked_sub(1)
        .ok_or(M11RecursiveGreenError::InvalidPoint)?;
    let end = tree
        .as_ref()
        .locate_leaf_containing_metric(
            arena,
            end_position,
            |summary| summary.events,
            &mut inspection,
        )?
        .ok_or(M11RecursiveGreenError::InvalidPoint)?;

    let start_events = decode_events(arena, start.id, &mut inspection)?;
    let start_prefix_events = start.prefix.map_or(0, |summary| summary.events);
    let start_index = usize::try_from(
        event_range
            .start
            .checked_sub(start_prefix_events)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?,
    )
    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    if start_index >= start_events.len() {
        return Err(M11RecursiveGreenError::InvalidPoint);
    }
    let prefix_summary = fold_events(&start_events[..start_index])?;
    let absolute_prefix = combine_optional(start.prefix, prefix_summary)?;
    let observed_prefix = absolute_prefix.unwrap_or_else(RecursiveGreenSummary::empty);
    if observed_prefix.physical_bytes != start_physical.bytes()
        || observed_prefix.physical_utf16 != start_physical.utf16()
        || observed_prefix.logical_bytes != start_logical.bytes()
        || observed_prefix.logical_utf16 != start_logical.utf16()
    {
        return Err(M11RecursiveGreenError::InvalidPoint);
    }

    let (end_events, boundary_events_decoded) = if end.id == start.id {
        (
            start_events.clone(),
            u64::try_from(start_events.len())
                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
        )
    } else {
        let decoded = decode_events(arena, end.id, &mut inspection)?;
        let count = start_events
            .len()
            .checked_add(decoded.len())
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        (decoded, count)
    };
    let end_prefix_events = end.prefix.map_or(0, |summary| summary.events);
    let end_index = usize::try_from(
        event_range
            .end
            .checked_sub(end_prefix_events)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?,
    )
    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    if end_index == 0 || end_index > end_events.len() {
        return Err(M11RecursiveGreenError::InvalidPoint);
    }

    let deleted_summary = if start.id == end.id {
        fold_events(&start_events[start_index..end_index])?
            .ok_or(M11RecursiveGreenError::InvalidPoint)?
    } else {
        let start_tail = fold_events(&start_events[start_index..])?;
        let middle =
            tree.as_ref()
                .range_summary(arena, start.ordinal + 1..end.ordinal, &mut inspection)?;
        let end_head = fold_events(&end_events[..end_index])?;
        combine_optional(combine_optional(start_tail, middle)?, end_head)?
            .ok_or(M11RecursiveGreenError::InvalidPoint)?
    };
    let expected_physical_bytes = end_physical
        .bytes()
        .checked_sub(start_physical.bytes())
        .ok_or(M11RecursiveGreenError::InvalidPoint)?;
    let expected_physical_utf16 = end_physical
        .utf16()
        .checked_sub(start_physical.utf16())
        .ok_or(M11RecursiveGreenError::InvalidPoint)?;
    let expected_logical_bytes = end_logical
        .bytes()
        .checked_sub(start_logical.bytes())
        .ok_or(M11RecursiveGreenError::InvalidPoint)?;
    let expected_logical_utf16 = end_logical
        .utf16()
        .checked_sub(start_logical.utf16())
        .ok_or(M11RecursiveGreenError::InvalidPoint)?;
    if deleted_summary.physical_bytes != expected_physical_bytes
        || deleted_summary.physical_utf16 != expected_physical_utf16
        || deleted_summary.logical_bytes != expected_logical_bytes
        || deleted_summary.logical_utf16 != expected_logical_utf16
    {
        return Err(M11RecursiveGreenError::InvalidPoint);
    }

    let mut retained = Vec::new();
    let suffix_len = end_events.len() - end_index;
    retained
        .try_reserve(start_index + suffix_len)
        .map_err(|_| M11RecursiveGreenError::InvalidState)?;
    retained.extend_from_slice(&start_events[..start_index]);
    retained.extend_from_slice(&end_events[end_index..]);
    Ok(StructuralSplicePlan {
        storage_range: start.ordinal
            ..end
                .ordinal
                .checked_add(1)
                .ok_or(M11RecursiveGreenError::CounterOverflow)?,
        events: retained,
        deleted_summary,
        boundary_events_decoded,
        prefix_events_retained: start_index,
        boundary_events_retained: u64::try_from(start_index + suffix_len)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
        inspection,
        end_leaf_ordinal: end.ordinal,
        end_event_index: end_index,
    })
}

fn pack_structural_fragment(
    lease: &SourceSnapshotLease,
    target_range: &Range<usize>,
    start_open: &[M11RecursiveGreenBoundaryFrame],
    end_open: &[M11RecursiveGreenBoundaryFrame],
    base_maximum_frame_id: u64,
    events: &[M11RecursiveGreenEvent],
) -> Result<(Vec<PackedGreenEvent>, RecursiveGreenSummary), M11RecursiveGreenError> {
    let mut packed = Vec::new();
    packed
        .try_reserve(events.len())
        .map_err(|_| M11RecursiveGreenError::InvalidState)?;
    let mut open: Vec<(M11RecursiveGreenFrameId, M11RecursiveGreenKind)> = Vec::new();
    open.try_reserve(start_open.len().saturating_add(32))
        .map_err(|_| M11RecursiveGreenError::InvalidState)?;
    open.extend(start_open.iter().map(|frame| (frame.frame, frame.kind)));
    let mut source_cursor = target_range.start;
    let mut last_new_frame = base_maximum_frame_id;
    let mut property_adjacent = false;

    for event in events.iter().copied() {
        match event {
            M11RecursiveGreenEvent::Enter { frame, kind } => {
                if frame.get() <= last_new_frame {
                    return Err(M11RecursiveGreenError::InvalidEvent);
                }
                last_new_frame = frame.get();
                open.push((frame, kind));
                packed.push(PackedGreenEvent::Enter { frame, kind });
                property_adjacent = true;
            }
            M11RecursiveGreenEvent::Property(property) => {
                if !property_adjacent || open.is_empty() {
                    return Err(M11RecursiveGreenError::InvalidEvent);
                }
                packed.push(PackedGreenEvent::Property(property));
            }
            M11RecursiveGreenEvent::Coverage {
                physical,
                owner_depth,
                part,
                logical,
            } => {
                if physical.is_empty()
                    || usize::try_from(owner_depth)
                        .ok()
                        .is_none_or(|depth| depth >= open.len())
                {
                    return Err(M11RecursiveGreenError::InvalidEvent);
                }
                logical.validate(owner_depth)?;
                let physical_bytes = usize::try_from(physical.bytes())
                    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
                let end = source_cursor
                    .checked_add(physical_bytes)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                if end > target_range.end || metric_between(lease, source_cursor, end)? != physical
                {
                    return Err(M11RecursiveGreenError::IncompleteCoverage);
                }
                packed.extend(derive_coverage_atoms(
                    lease,
                    source_cursor..end,
                    owner_depth,
                    part,
                    logical,
                )?);
                source_cursor = end;
                property_adjacent = false;
            }
            M11RecursiveGreenEvent::RetypeOpen {
                frame,
                kind,
                property,
            } => {
                let Some(top) = open.last_mut() else {
                    return Err(M11RecursiveGreenError::InvalidEvent);
                };
                if top.0 != frame {
                    return Err(M11RecursiveGreenError::InvalidEvent);
                }
                top.1 = kind;
                packed.push(PackedGreenEvent::RetypeOpen {
                    frame,
                    kind,
                    property,
                });
                property_adjacent = false;
            }
            M11RecursiveGreenEvent::Exit {
                frame,
                final_kind,
                close,
                last_line_blank,
                child,
            } => {
                if open.pop() != Some((frame, final_kind)) {
                    return Err(M11RecursiveGreenError::InvalidEvent);
                }
                packed.push(PackedGreenEvent::Exit {
                    frame,
                    final_kind,
                    close,
                    last_line_blank,
                    child,
                });
                property_adjacent = false;
            }
        }
    }
    if source_cursor != target_range.end
        || open.len() != end_open.len()
        || !open
            .iter()
            .zip(end_open)
            .all(|((frame, kind), expected)| *frame == expected.frame && *kind == expected.kind)
        || packed.is_empty()
    {
        return Err(M11RecursiveGreenError::IncompleteCoverage);
    }
    let summary = fold_events(&packed)?.ok_or(M11RecursiveGreenError::InvalidEvent)?;
    Ok((packed, summary))
}

fn decode_events(
    arena: &PageArena,
    id: ArenaId,
    inspection: &mut SequenceInspectionReceipt,
) -> Result<Vec<PackedGreenEvent>, M11RecursiveGreenError> {
    let payload = arena.payload(id)?;
    let leaf = decode_leaf(payload, &mut inspection.spec)?.ok_or(
        M11RecursiveGreenError::Corrupt("structural splice selected a branch payload"),
    )?;
    let mut cursor = 0_usize;
    let mut events = Vec::with_capacity(usize::from(leaf.events));
    for _ in 0..leaf.events {
        events.push(decode_packed_event(leaf.event_bytes, &mut cursor)?);
    }
    if cursor != leaf.event_bytes.len() {
        return Err(M11RecursiveGreenError::Corrupt(
            "structural splice did not consume its boundary page",
        ));
    }
    Ok(events)
}

fn validate_exact_spanning_exit_scope(
    base: &M11RecursiveGreenRoot,
    target_source: crate::SourceVersion,
    end: &M11RecursiveGreenStructuralBoundary,
    target_end_physical: super::codec::M11RecursiveGreenSourceMetric,
    repairs: &[M11RecursiveGreenSpanningExitRepair],
) -> Result<(), M11RecursiveGreenError> {
    if !repairs
        .iter()
        .any(|repair| matches!(repair, M11RecursiveGreenSpanningExitRepair::Exact { .. }))
    {
        return Ok(());
    }
    let base_physical = super::codec::M11RecursiveGreenSourceMetric::new(
        base.source_byte_len(),
        base.source_utf16_len(),
    )
    .ok_or(M11RecursiveGreenError::Corrupt(
        "recursive-Green root has invalid physical totals",
    ))?;
    let base_logical = super::codec::M11RecursiveGreenSourceMetric::new(
        base.logical_byte_len(),
        base.logical_utf16_len(),
    )
    .ok_or(M11RecursiveGreenError::Corrupt(
        "recursive-Green root has invalid logical totals",
    ))?;
    let target_physical = super::codec::M11RecursiveGreenSourceMetric::new(
        u64::try_from(target_source.byte_len())
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
        u64::try_from(target_source.utf16_len())
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?,
    )
    .ok_or(M11RecursiveGreenError::SourceAuthorityMismatch)?;
    if repairs.len() != 1
        || end.open.len() != 1
        || end.event_cut.checked_add(1) != Some(base.event_count())
        || end.physical != base_physical
        || end.logical != base_logical
        || target_end_physical != target_physical
    {
        return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
    }
    Ok(())
}

fn plan_spanning_exit_repairs(
    arena: &PageArena,
    tree: &super::build::GreenSequenceTree,
    base: &M11RecursiveGreenRoot,
    target_source: crate::SourceVersion,
    end: &M11RecursiveGreenStructuralBoundary,
    target_end_physical: super::codec::M11RecursiveGreenSourceMetric,
    repairs: &[M11RecursiveGreenSpanningExitRepair],
    inspection: &mut SequenceInspectionReceipt,
) -> Result<Vec<LocatedSpanningExitRepair>, M11RecursiveGreenError> {
    validate_exact_spanning_exit_scope(base, target_source, end, target_end_physical, repairs)?;
    let mut located_repairs = Vec::new();
    located_repairs
        .try_reserve_exact(repairs.len())
        .map_err(|_| M11RecursiveGreenError::InvalidState)?;
    for (repair_index, repair) in repairs.iter().copied().enumerate() {
        if repairs[..repair_index]
            .iter()
            .any(|candidate| candidate.frame() == repair.frame())
        {
            return Err(M11RecursiveGreenError::InvalidEvent);
        }
        let open_index = end
            .open
            .iter()
            .position(|candidate| candidate.frame == repair.frame())
            .ok_or(M11RecursiveGreenError::SourceAuthorityMismatch)?;
        let relative_depth = end
            .open
            .len()
            .checked_sub(open_index + 1)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
        let (leaf_ordinal, event_index, old) = locate_spanning_exit(
            arena,
            tree,
            end.event_cut,
            relative_depth,
            repair.frame(),
            inspection,
        )?;
        let replacement = apply_spanning_exit_repair(
            old,
            repair,
            end.open[open_index].kind,
            end.physical,
            target_end_physical,
        )?;
        if packed_event_len(old) != packed_event_len(replacement) {
            return Err(M11RecursiveGreenError::InvalidEvent);
        }
        if old != replacement {
            located_repairs.push(LocatedSpanningExitRepair {
                leaf_ordinal,
                event_index,
                replacement,
            });
        }
    }
    located_repairs.sort_unstable_by_key(|repair| (repair.leaf_ordinal, repair.event_index));
    Ok(located_repairs)
}

fn locate_spanning_exit(
    arena: &PageArena,
    tree: &super::build::GreenSequenceTree,
    event_cut: u64,
    wanted_relative_depth: usize,
    expected_frame: M11RecursiveGreenFrameId,
    inspection: &mut SequenceInspectionReceipt,
) -> Result<(u64, usize, PackedGreenEvent), M11RecursiveGreenError> {
    let start = tree
        .as_ref()
        .locate_leaf_containing_metric(arena, event_cut, |summary| summary.events, inspection)?
        .ok_or(M11RecursiveGreenError::InvalidPoint)?;
    let start_prefix_events = start.prefix.map_or(0, |summary| summary.events);
    let start_index = usize::try_from(
        event_cut
            .checked_sub(start_prefix_events)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?,
    )
    .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let mut relative_depth = 0_i64;
    let wanted_relative_depth = i64::try_from(wanted_relative_depth)
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let start_events = decode_events(arena, start.id, inspection)?;
    if start_index > start_events.len() {
        return Err(M11RecursiveGreenError::Corrupt(
            "spanning Exit cut escaped its selected leaf",
        ));
    }
    if let Some(found) = find_spanning_exit_in_leaf(
        &start_events,
        start_index,
        &mut relative_depth,
        wanted_relative_depth,
        expected_frame,
    )? {
        return Ok((start.ordinal, found.0, found.1));
    }

    let range_start = start
        .ordinal
        .checked_add(1)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let leaf_count = tree
        .as_ref()
        .summary(arena, inspection)?
        .ok_or(M11RecursiveGreenError::InvalidState)?
        .leaves();
    if range_start >= leaf_count {
        return Err(M11RecursiveGreenError::Corrupt(
            "spanning Green frame has no retained Exit",
        ));
    }
    let exit_leaf = tree
        .as_ref()
        .locate_leaf_by_monotone_summary(
            arena,
            range_start..leaf_count,
            SequenceSummaryPartitionDirection::Forward,
            inspection,
            |candidate| {
                Ok(relative_depth
                    .checked_add(candidate.minimum_prefix)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?
                    < -wanted_relative_depth)
            },
        )?
        .ok_or(M11RecursiveGreenError::Corrupt(
            "spanning Green frame has no summary-selected Exit",
        ))?;
    if let Some(before) = exit_leaf.accumulated {
        relative_depth = relative_depth
            .checked_add(before.balance)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    }
    let events = decode_events(arena, exit_leaf.id, inspection)?;
    let found = find_spanning_exit_in_leaf(
        &events,
        0,
        &mut relative_depth,
        wanted_relative_depth,
        expected_frame,
    )?
    .ok_or(M11RecursiveGreenError::Corrupt(
        "summary-selected Green leaf omitted its spanning Exit",
    ))?;
    Ok((exit_leaf.ordinal, found.0, found.1))
}

fn find_spanning_exit_in_leaf(
    events: &[PackedGreenEvent],
    start: usize,
    relative_depth: &mut i64,
    wanted_relative_depth: i64,
    expected_frame: M11RecursiveGreenFrameId,
) -> Result<Option<(usize, PackedGreenEvent)>, M11RecursiveGreenError> {
    for (index, event) in events.iter().copied().enumerate().skip(start) {
        match event {
            PackedGreenEvent::Enter { .. } => {
                *relative_depth = relative_depth
                    .checked_add(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            }
            PackedGreenEvent::Exit { frame, .. } if *relative_depth == -wanted_relative_depth => {
                if frame != expected_frame {
                    return Err(M11RecursiveGreenError::Corrupt(
                        "relative-depth spanning Exit differs from its open frame",
                    ));
                }
                return Ok(Some((index, event)));
            }
            PackedGreenEvent::Exit { .. } => {
                *relative_depth = relative_depth
                    .checked_sub(1)
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
            }
            PackedGreenEvent::Property(_)
            | PackedGreenEvent::Coverage { .. }
            | PackedGreenEvent::RetypeOpen { .. } => {}
        }
    }
    Ok(None)
}

fn apply_spanning_exit_repair(
    old: PackedGreenEvent,
    repair: M11RecursiveGreenSpanningExitRepair,
    boundary_kind: M11RecursiveGreenKind,
    base_convergence_physical: super::codec::M11RecursiveGreenSourceMetric,
    target_convergence_physical: super::codec::M11RecursiveGreenSourceMetric,
) -> Result<PackedGreenEvent, M11RecursiveGreenError> {
    let PackedGreenEvent::Exit {
        frame,
        final_kind,
        close,
        last_line_blank,
        child,
    } = old
    else {
        return Err(M11RecursiveGreenError::InvalidEvent);
    };
    if frame != repair.frame() {
        return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
    }
    match repair {
        M11RecursiveGreenSpanningExitRepair::TranslateCachedRow {
            base_convergence_end,
            target_convergence_end,
            ..
        } => {
            if !is_renderable_row_kind(boundary_kind) || !is_renderable_row_kind(final_kind) {
                return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
            }
            let authenticated_target_convergence_end = translate_metric(
                base_convergence_end,
                base_convergence_physical,
                target_convergence_physical,
            )?;
            if authenticated_target_convergence_end != target_convergence_end {
                return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
            }
            let close = close.ok_or(M11RecursiveGreenError::InvalidEvent)?;
            let (semantic, cached) = close
                .split_cached_row_editable()?
                .ok_or(M11RecursiveGreenError::InvalidEvent)?;
            let start =
                translate_row_cut(cached.start(), base_convergence_end, target_convergence_end)?;
            let end =
                translate_row_cut(cached.end(), base_convergence_end, target_convergence_end)?;
            let authenticated_end = translate_metric(
                cached.end(),
                base_convergence_physical,
                target_convergence_physical,
            )?;
            if start != cached.start() || end != authenticated_end {
                return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
            }
            let cached = super::codec::M11RecursiveGreenCachedRowEditable::new(
                cached.capability(),
                start,
                end,
            )
            .ok_or(M11RecursiveGreenError::InvalidEvent)?;
            Ok(PackedGreenEvent::Exit {
                frame,
                final_kind,
                close: Some(
                    super::codec::M11RecursiveGreenCloseFacts::new_with_cached_row_editable(
                        close.tag(),
                        semantic,
                        cached,
                    )?,
                ),
                last_line_blank,
                child,
            })
        }
        M11RecursiveGreenSpanningExitRepair::Exact {
            final_kind: repair_final_kind,
            close: repair_close,
            last_line_blank,
            child,
            ..
        } => {
            if repair_final_kind != final_kind
                || repair_final_kind != boundary_kind
                || repair_close != close
            {
                return Err(M11RecursiveGreenError::SourceAuthorityMismatch);
            }
            Ok(PackedGreenEvent::Exit {
                frame,
                final_kind,
                close,
                last_line_blank,
                child,
            })
        }
    }
}

fn translate_row_cut(
    value: super::codec::M11RecursiveGreenSourceMetric,
    base: super::codec::M11RecursiveGreenSourceMetric,
    target: super::codec::M11RecursiveGreenSourceMetric,
) -> Result<super::codec::M11RecursiveGreenSourceMetric, M11RecursiveGreenError> {
    let after = value.bytes() >= base.bytes() && value.utf16() >= base.utf16();
    let before = value.bytes() <= base.bytes() && value.utf16() <= base.utf16();
    if !after && !before {
        return Err(M11RecursiveGreenError::InvalidEvent);
    }
    if after {
        translate_metric(value, base, target)
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod spanning_exit_repair_tests {
    use super::{
        apply_spanning_exit_repair, M11RecursiveGreenSpanningExitRepair, PackedGreenEvent,
    };
    use crate::recursive_green::codec::{
        M11RecursiveGreenCachedRowEditCapability, M11RecursiveGreenCachedRowEditable,
        M11RecursiveGreenCloseFacts, M11RecursiveGreenClosedChild, M11RecursiveGreenFactTag,
        M11RecursiveGreenFrameId, M11RecursiveGreenKind, M11RecursiveGreenSourceMetric,
    };

    fn metric(bytes: u64, utf16: u64) -> M11RecursiveGreenSourceMetric {
        M11RecursiveGreenSourceMetric::new(bytes, utf16).expect("valid source metric")
    }

    fn cached_close(
        tag: u16,
        semantic: &[u8],
        start: M11RecursiveGreenSourceMetric,
        end: M11RecursiveGreenSourceMetric,
    ) -> M11RecursiveGreenCloseFacts {
        M11RecursiveGreenCloseFacts::new_with_cached_row_editable(
            M11RecursiveGreenFactTag::new(tag).expect("nonzero fact tag"),
            semantic,
            M11RecursiveGreenCachedRowEditable::new(
                M11RecursiveGreenCachedRowEditCapability::Contiguous,
                start,
                end,
            )
            .expect("ordered cached row"),
        )
        .expect("cached close facts")
    }

    fn exit(
        frame: M11RecursiveGreenFrameId,
        kind: M11RecursiveGreenKind,
        close: Option<M11RecursiveGreenCloseFacts>,
    ) -> PackedGreenEvent {
        PackedGreenEvent::Exit {
            frame,
            final_kind: kind,
            close,
            last_line_blank: false,
            child: M11RecursiveGreenClosedChild::new(false, false, false),
        }
    }

    #[test]
    fn exact_repair_rejects_kind_and_close_fact_substitution() {
        let frame = M11RecursiveGreenFrameId::new(1).expect("frame");
        let kind = M11RecursiveGreenKind::new(1).expect("kind");
        let other_kind = M11RecursiveGreenKind::new(2).expect("other kind");
        let close =
            M11RecursiveGreenCloseFacts::new(M11RecursiveGreenFactTag::new(1).expect("tag"), &[7])
                .expect("close");
        let other_close = M11RecursiveGreenCloseFacts::new(
            M11RecursiveGreenFactTag::new(2).expect("other tag"),
            &[7],
        )
        .expect("same-width close");
        let old = exit(frame, kind, Some(close));
        let physical = metric(100, 90);

        for repair in [
            M11RecursiveGreenSpanningExitRepair::Exact {
                frame,
                final_kind: other_kind,
                close: Some(close),
                last_line_blank: true,
                child: M11RecursiveGreenClosedChild::new(true, false, false),
            },
            M11RecursiveGreenSpanningExitRepair::Exact {
                frame,
                final_kind: kind,
                close: Some(other_close),
                last_line_blank: true,
                child: M11RecursiveGreenClosedChild::new(true, false, false),
            },
        ] {
            assert!(apply_spanning_exit_repair(old, repair, kind, physical, physical).is_err());
        }

        let repaired = apply_spanning_exit_repair(
            old,
            M11RecursiveGreenSpanningExitRepair::Exact {
                frame,
                final_kind: kind,
                close: Some(close),
                last_line_blank: true,
                child: M11RecursiveGreenClosedChild::new(true, true, false),
            },
            kind,
            physical,
            physical,
        )
        .expect("parser-certified close state");
        assert_eq!(
            repaired,
            PackedGreenEvent::Exit {
                frame,
                final_kind: kind,
                close: Some(close),
                last_line_blank: true,
                child: M11RecursiveGreenClosedChild::new(true, true, false),
            }
        );
    }

    #[test]
    fn cached_row_translation_uses_independent_authenticated_utf8_and_utf16_deltas() {
        let frame = M11RecursiveGreenFrameId::new(1).expect("frame");
        let kind = M11RecursiveGreenKind::new(5).expect("Paragraph kind");
        let start = metric(10, 8);
        let end = metric(80, 65);
        let close = cached_close(1, &[0xA5], start, end);
        let old = exit(frame, kind, Some(close));

        // Replacing one UTF-16 surrogate pair with three ASCII code units is
        // -1 UTF-8 byte and +1 UTF-16 code unit at every retained suffix cut.
        let base_physical = metric(100, 80);
        let target_physical = metric(99, 81);
        let base_convergence_end = metric(60, 45);
        let target_convergence_end = metric(59, 46);
        let repaired = apply_spanning_exit_repair(
            old,
            M11RecursiveGreenSpanningExitRepair::TranslateCachedRow {
                frame,
                base_convergence_end,
                target_convergence_end,
            },
            kind,
            base_physical,
            target_physical,
        )
        .expect("independent metric translation");
        let PackedGreenEvent::Exit {
            final_kind,
            close: Some(repaired_close),
            last_line_blank,
            child,
            ..
        } = repaired
        else {
            panic!("translated Exit shape")
        };
        let (semantic, cached) = repaired_close
            .split_cached_row_editable()
            .expect("canonical trailer")
            .expect("cached row");
        assert_eq!(final_kind, kind);
        assert_eq!(repaired_close.tag(), close.tag());
        assert_eq!(semantic, &[0xA5]);
        assert_eq!(
            cached.capability(),
            M11RecursiveGreenCachedRowEditCapability::Contiguous
        );
        assert_eq!(cached.start(), start);
        assert_eq!(cached.end(), metric(79, 66));
        assert!(!last_line_blank);
        assert_eq!(
            child,
            M11RecursiveGreenClosedChild::new(false, false, false)
        );
    }

    #[test]
    fn cached_row_translation_rejects_unbound_delta_and_thresholds() {
        let frame = M11RecursiveGreenFrameId::new(1).expect("frame");
        let kind = M11RecursiveGreenKind::new(5).expect("Paragraph kind");
        let start = metric(10, 8);
        let end = metric(80, 65);
        let old = exit(frame, kind, Some(cached_close(1, &[0xA5], start, end)));
        let base_physical = metric(100, 80);
        let target_physical = metric(99, 81);

        for (base_convergence_end, target_convergence_end) in [
            // The caller's UTF-16 delta omits the source-authenticated +1.
            (metric(60, 45), metric(59, 45)),
            // The pair has the right delta but would move the retained start.
            (start, metric(9, 9)),
            // The pair has the right delta but leaves the retained end stale.
            (metric(90, 70), metric(89, 71)),
        ] {
            assert!(apply_spanning_exit_repair(
                old,
                M11RecursiveGreenSpanningExitRepair::TranslateCachedRow {
                    frame,
                    base_convergence_end,
                    target_convergence_end,
                },
                kind,
                base_physical,
                target_physical,
            )
            .is_err());
        }
    }
}

fn fold_events(
    events: &[PackedGreenEvent],
) -> Result<Option<RecursiveGreenSummary>, M11RecursiveGreenError> {
    let mut summary = None;
    for event in events.iter().copied() {
        summary = combine_optional(summary, Some(packed_event_summary(event)?))?;
    }
    Ok(summary)
}

fn combine_optional(
    left: Option<RecursiveGreenSummary>,
    right: Option<RecursiveGreenSummary>,
) -> Result<Option<RecursiveGreenSummary>, M11RecursiveGreenError> {
    match (left, right) {
        (Some(left), Some(right)) => Ok(Some(left.checked_followed_by(right)?)),
        (Some(value), None) | (None, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)]
fn make_structural_receipt(
    base: &M11RecursiveGreenRoot,
    selection: M11RecursiveGreenStructuralSpliceSelection,
    deleted_events: u64,
    replacement_events: u64,
    boundary_events_decoded: u64,
    boundary_events_reencoded: u64,
    deleted_storage_pages: u64,
    replacement_storage_pages: u64,
    mutation: SequenceMutationReceipt,
    seal_transitions: usize,
    lineage_transitions: usize,
    target_maximum_frame_id: u64,
) -> Result<M11RecursiveGreenStructuralSpliceReceipt, M11RecursiveGreenError> {
    let primary = selection
        .segments()
        .first()
        .ok_or(M11RecursiveGreenError::InvalidPoint)?;
    let base_event_range = primary.base_event_range();
    let target_event_range = primary.target_event_range();
    let boundary_events_retained = boundary_events_reencoded
        .checked_sub(replacement_events)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let transported_base_events = deleted_events
        .checked_add(boundary_events_retained)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let base_storage_pages = base.storage_page_count();
    let reused_storage_pages = base_storage_pages
        .checked_sub(deleted_storage_pages)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    let target_events = base
        .event_count()
        .checked_sub(deleted_events)
        .and_then(|events| events.checked_add(replacement_events))
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    if base_event_range.end - base_event_range.start != transported_base_events
        || target_event_range.end - target_event_range.start != boundary_events_reencoded
        || selection.segments().iter().any(|segment| {
            segment.base_event_end > base.event_count() || segment.target_event_end > target_events
        })
        || u64::try_from(mutation.leaves_deleted)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
            != deleted_storage_pages
        || u64::try_from(mutation.leaves_reused)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
            != reused_storage_pages
        || u64::try_from(mutation.committed_leaves_retained)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
            != base_storage_pages
        || u64::try_from(mutation.leaves_adopted)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
            != replacement_storage_pages
    {
        return Err(M11RecursiveGreenError::Corrupt(
            "structural splice receipt differs from measured mutation work",
        ));
    }
    Ok(M11RecursiveGreenStructuralSpliceReceipt {
        selection,
        base_events: base.event_count(),
        deleted_events,
        replacement_events,
        unchanged_events_preserved: base
            .event_count()
            .checked_sub(deleted_events)
            .ok_or(M11RecursiveGreenError::CounterOverflow)?,
        boundary_events_decoded,
        boundary_events_reencoded,
        base_storage_pages,
        deleted_storage_pages,
        replacement_storage_pages,
        reused_storage_pages,
        node_headers_decoded: mutation.inspection.node_headers_decoded,
        summary_combinations: mutation.inspection.summary_combinations,
        payload_bytes_inspected: mutation.inspection.spec.payload_bytes_inspected,
        events_authenticated: mutation.inspection.spec.spec_items_hashed,
        tree_nodes_visited: mutation.nodes_visited,
        branches_allocated: mutation.branches_allocated,
        maximum_atomic_height: mutation.maximum_atomic_height,
        seal_transitions,
        lineage_transitions,
        base_maximum_frame_id: base.maximum_frame_id(),
        target_maximum_frame_id,
    })
}

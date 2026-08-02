//! Source- and event-authenticated structural replacement of committed Green.
//!
//! Unlike the single-coverage fast path, this operation may replace an
//! arbitrary balanced sequence of block events.  It still touches only the
//! two event-boundary leaves, the replacement payload, and logarithmic AVL
//! paths.  Unchanged prefix and suffix leaves remain the exact committed arena
//! objects from the base root.

use std::fmt;
use std::ops::Range;

use crate::candidate_manifest::StrongIdentity;
use crate::document::{DocumentRuntime, ExactUnchangedPrefixWitness, ExactUnchangedSuffixWitness};
use crate::measured_sequence::{
    begin_measured_sequence_seal, splice_measured_sequence_atomic, SequenceInspectionReceipt,
    SequenceMutationReceipt,
};
use crate::source::SourceSnapshotLease;
use crate::storage::PageArena;
use crate::ArenaId;

use super::build::{
    allocate_recursive_green_identity, M11RecursiveGreenBuildReceipt, M11RecursiveGreenRoot,
};
use super::codec::{
    decode_leaf, decode_packed_event, packed_event_summary, M11RecursiveGreenError,
    M11RecursiveGreenEvent, M11RecursiveGreenFrameId, M11RecursiveGreenKind, PackedGreenEvent,
    RecursiveGreenSpec, RecursiveGreenSummary,
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
    runtime_identity: StrongIdentity,
    green_identity: StrongIdentity,
    source: crate::SourceVersion,
    event_cut: u64,
    physical: super::codec::M11RecursiveGreenSourceMetric,
    logical: super::codec::M11RecursiveGreenSourceMetric,
    open: Box<[M11RecursiveGreenBoundaryFrame]>,
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
        runtime_identity: StrongIdentity,
        green_identity: StrongIdentity,
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

    pub(super) fn rebound(
        runtime_identity: StrongIdentity,
        green_identity: StrongIdentity,
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
use super::splice::{
    abort_seal_after_failure, add_inspection, build_replacement_pages, derive_coverage_atoms,
    metric_between, release_tree_after_failure, validate_lineage,
};

/// Parser-selected event intervals for one exact structural Green splice.
///
/// The ranges are not authority by themselves.  They name the exact events
/// removed from the retained base and the exact replacement events in the
/// locally authenticated target; publication must revalidate them against
/// those two roots before using them as an exact-base delta.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11RecursiveGreenStructuralSpliceSelection {
    base_event_start: u64,
    base_event_end: u64,
    target_event_start: u64,
    target_event_end: u64,
}

impl M11RecursiveGreenStructuralSpliceSelection {
    /// Binds base and target event ranges at the same semantic event cut.
    ///
    /// # Errors
    ///
    /// Returns [`M11RecursiveGreenError::InvalidPoint`] for reversed ranges
    /// or ranges that do not begin at the same event ordinal.
    pub fn new(
        base_event_range: Range<u64>,
        target_event_range: Range<u64>,
    ) -> Result<Self, M11RecursiveGreenError> {
        if base_event_range.start > base_event_range.end
            || target_event_range.start > target_event_range.end
            || base_event_range.start != target_event_range.start
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
    pub fn base_event_range(self) -> Range<u64> {
        self.base_event_start..self.base_event_end
    }

    #[must_use]
    pub fn target_event_range(self) -> Range<u64> {
        self.target_event_start..self.target_event_end
    }
}

/// Exact bounded work performed by one structural Green adoption.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    pub const fn selection(self) -> M11RecursiveGreenStructuralSpliceSelection {
        self.selection
    }

    #[must_use]
    pub const fn base_events(self) -> u64 {
        self.base_events
    }
    #[must_use]
    pub const fn deleted_events(self) -> u64 {
        self.deleted_events
    }
    #[must_use]
    pub const fn replacement_events(self) -> u64 {
        self.replacement_events
    }
    #[must_use]
    pub const fn unchanged_events_preserved(self) -> u64 {
        self.unchanged_events_preserved
    }
    #[must_use]
    pub const fn boundary_events_decoded(self) -> u64 {
        self.boundary_events_decoded
    }
    #[must_use]
    pub const fn boundary_events_reencoded(self) -> u64 {
        self.boundary_events_reencoded
    }
    #[must_use]
    pub const fn base_storage_pages(self) -> u64 {
        self.base_storage_pages
    }
    #[must_use]
    pub const fn deleted_storage_pages(self) -> u64 {
        self.deleted_storage_pages
    }
    #[must_use]
    pub const fn replacement_storage_pages(self) -> u64 {
        self.replacement_storage_pages
    }
    #[must_use]
    pub const fn reused_storage_pages(self) -> u64 {
        self.reused_storage_pages
    }
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
    pub const fn tree_nodes_visited(self) -> usize {
        self.tree_nodes_visited
    }
    #[must_use]
    pub const fn branches_allocated(self) -> usize {
        self.branches_allocated
    }
    #[must_use]
    pub const fn maximum_atomic_height(self) -> u16 {
        self.maximum_atomic_height
    }
    #[must_use]
    pub const fn seal_transitions(self) -> usize {
        self.seal_transitions
    }
    #[must_use]
    pub const fn lineage_transitions(self) -> usize {
        self.lineage_transitions
    }
    #[must_use]
    pub const fn base_maximum_frame_id(self) -> u64 {
        self.base_maximum_frame_id
    }
    #[must_use]
    pub const fn target_maximum_frame_id(self) -> u64 {
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
        || start.open != end.open
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
    if plan.deleted_summary.balance != 0
        || plan.deleted_summary.minimum_prefix != 0
        || plan.deleted_summary.oldest_open.is_some()
    {
        return Err(M11RecursiveGreenError::InvalidPoint);
    }

    let (replacement, replacement_summary) = pack_structural_fragment(
        &target_lease,
        &target_byte_range,
        external_open_depth,
        base.maximum_frame_id(),
        events,
    )?;
    if replacement_summary.physical_bytes
        != u64::try_from(target_byte_range.end - target_byte_range.start)
            .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        || replacement_summary.physical_utf16
            != u64::try_from(target_utf16_end - target_utf16_start)
                .map_err(|_| M11RecursiveGreenError::CounterOverflow)?
        || replacement_summary.balance != 0
        || replacement_summary.minimum_prefix != 0
        || replacement_summary.oldest_open.is_some()
    {
        return Err(M11RecursiveGreenError::IncompleteCoverage);
    }

    let replacement_events = replacement_summary.events;
    let boundary_events_reencoded = plan
        .boundary_events_retained
        .checked_add(replacement_events)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    plan.events.splice(
        plan.prefix_events_retained..plan.prefix_events_retained,
        replacement,
    );

    let mut mutation = SequenceMutationReceipt::default();
    add_inspection(&mut mutation.inspection, plan.inspection)?;
    let mut session = runtime.producer_arena_mut().begin_build()?;
    let replacement_root = build_replacement_pages(&mut session, &plan.events, &mut mutation)?;
    let replacement_storage_pages = u64::try_from(mutation.leaves_adopted)
        .map_err(|_| M11RecursiveGreenError::CounterOverflow)?;
    let root = splice_measured_sequence_atomic::<RecursiveGreenSpec>(
        &mut session,
        tree,
        plan.storage_range.clone(),
        Some(replacement_root),
        &mut mutation,
    )?
    .ok_or(M11RecursiveGreenError::Corrupt(
        "structural Green splice produced an empty root",
    ))?;
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
    let selection = M11RecursiveGreenStructuralSpliceSelection::new(
        base_event_range.clone(),
        start.event_cut..target_event_cut,
    )?;
    let receipt = match make_structural_receipt(
        base,
        selection,
        base_event_range.end - base_event_range.start,
        replacement_events,
        plan.boundary_events_decoded,
        boundary_events_reencoded,
        plan.storage_range.end - plan.storage_range.start,
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
        start.open,
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
    })
}

fn pack_structural_fragment(
    lease: &SourceSnapshotLease,
    target_range: &Range<usize>,
    external_open_depth: usize,
    base_maximum_frame_id: u64,
    events: &[M11RecursiveGreenEvent],
) -> Result<(Vec<PackedGreenEvent>, RecursiveGreenSummary), M11RecursiveGreenError> {
    let mut packed = Vec::new();
    packed
        .try_reserve(events.len())
        .map_err(|_| M11RecursiveGreenError::InvalidState)?;
    let mut open: Vec<(M11RecursiveGreenFrameId, M11RecursiveGreenKind)> = Vec::new();
    open.try_reserve(32)
        .map_err(|_| M11RecursiveGreenError::InvalidState)?;
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
                let total_depth = external_open_depth
                    .checked_add(open.len())
                    .ok_or(M11RecursiveGreenError::CounterOverflow)?;
                if physical.is_empty()
                    || usize::try_from(owner_depth)
                        .ok()
                        .is_none_or(|depth| depth >= total_depth)
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
    if source_cursor != target_range.end || !open.is_empty() || packed.is_empty() {
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
    let base_event_range = selection.base_event_range();
    let target_event_range = selection.target_event_range();
    let base_storage_pages = base.storage_page_count();
    let reused_storage_pages = base_storage_pages
        .checked_sub(deleted_storage_pages)
        .ok_or(M11RecursiveGreenError::CounterOverflow)?;
    if base_event_range.end - base_event_range.start != deleted_events
        || target_event_range.end - target_event_range.start != replacement_events
        || base_event_range.end > base.event_count()
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
